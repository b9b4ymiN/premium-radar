//! Tauri commands for the agent harness (W6): a streaming `agent_ask` over
//! `tauri::ipc::Channel<AgentEvent>` plus its `agent_cancel` counterpart.
//! The run itself lives on the blocking pool; the engine lock is taken
//! only while a tool executes inside the executor closure and released
//! before every LLM round-trip (the same contract as `ask_copilot`).

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use radar_core::agent::{AgentEvent, AgentOutcome, AgentRequest, Mode, Registry, RunLimits};
use radar_core::copilot::ChatMessage;
use serde::Deserialize;

use crate::agent_tools::ui::{PendingAcks, UiBridge};
use crate::agent_tools::{ToolCtx, dispatch};
use crate::{AppState, lock, off_ui_thread};

/// One agent question: the chat history plus whatever the frontend knows
/// about the current screen (page, anchors — the UI ships it from P3).
#[derive(Deserialize)]
pub struct AgentAskInput {
    pub messages: Vec<ChatMessage>,
    pub screen_context: Option<serde_json::Value>,
}

/// The agent's tool registry: every tool family's specs. A registration
/// failure is a programming bug (bad or duplicate spec name), so it
/// surfaces as the command's Err — never a panic.
pub fn build_registry() -> Result<Registry, String> {
    use radar_core::agent::tools::{ideas, market, mine, playbook, present, screener, ui};
    let mut registry = Registry::new();
    let families = [
        screener::screener_specs(),
        market::specs(),
        ideas::specs(),
        mine::specs(),
        playbook::specs(),
        ui::specs(),
        present::specs(),
    ];
    for spec in families.into_iter().flatten() {
        registry.register(spec)?;
    }
    Ok(registry)
}

/// Serialises agent runs and carries the cooperative stop flag: `running`
/// makes a second concurrent `agent_ask` fail fast instead of interleaving
/// two event streams, `cancel` is what the runner checks before every HTTP
/// round-trip. The Arc lets the blocking-pool run share the same flag.
/// `acks` pairs emitted `ui_action` events with the `agent_ui_result`
/// acks coming back from the frontend.
pub struct AgentControl {
    running: AtomicBool,
    cancel: Arc<AtomicBool>,
    acks: Arc<PendingAcks>,
}

impl AgentControl {
    pub fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            cancel: Arc::new(AtomicBool::new(false)),
            acks: Arc::new(PendingAcks::new()),
        }
    }

    /// Claim the single agent slot and arm the guard that frees it when
    /// `agent_ask` leaves — answer, error and cancellation all drop the
    /// guard, so a failed run can never leave the agent stuck "busy". A
    /// stale cancel left by the previous run is cleared so it cannot kill
    /// this one.
    fn begin_run(&self) -> Result<RunningGuard<'_>, &'static str> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err("AI กำลังตอบคำถามก่อนหน้าอยู่ — กดหยุดก่อน หรือรอให้ตอบเสร็จ");
        }
        self.cancel.store(false, Ordering::SeqCst);
        Ok(RunningGuard {
            running: &self.running,
        })
    }

    /// Stop the live run (if any) before its next step. The flag is set
    /// even when no run is active (harmless — the next start clears it);
    /// the return value says whether a run was active at the moment of
    /// the request.
    fn request_cancel(&self) -> bool {
        self.cancel.store(true, Ordering::SeqCst);
        self.running.load(Ordering::SeqCst)
    }
}

impl Default for AgentControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Frees the single agent slot on drop — every early `?` return between
/// [`AgentControl::begin_run`] and the end of `agent_ask` runs this, so
/// `running` is always cleared.
struct RunningGuard<'a> {
    running: &'a AtomicBool,
}

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// Ask the agent a question. Every step of the run streams to the frontend
/// through `on_event` (Thinking / ToolStart / ToolEnd / Answer / Error /
/// Cancelled); the command result only says how the run ended: "answer"
/// or "cancelled". One run at a time — a second concurrent call is
/// rejected with a Thai message.
#[tauri::command]
pub async fn agent_ask(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: AgentAskInput,
    on_event: tauri::ipc::Channel<AgentEvent>,
) -> Result<String, String> {
    let (cfg, mode, settings) = {
        let settings = lock(&state.settings);
        (
            settings.llm_config(),
            Mode::from_ui_mode(&settings.ui_mode),
            settings.clone(),
        )
    };
    let cfg = cfg.ok_or("ยังไม่ได้ตั้งค่า AI — เปิดหน้า Setup (ไอคอน ⚙) เพื่อใส่ LLM")?;
    let registry = build_registry()?;
    let _running = state.agent.begin_run()?;

    let cancel = state.agent.cancel.clone();
    // The screen context rides in two places: the request (a system message
    // for the model) and the tool ctx (the anchor source for ui calls).
    let req_screen_context = input.screen_context;
    // The ui bridge emits on a clone of the run's event channel (a dead
    // channel flips the cancel flag, same rule as the run's own emitter) and
    // parks on the shared ack table that `agent_ui_result` resolves.
    let ui_bridge = {
        let on_event = on_event.clone();
        let cancel_on_dead_channel = cancel.clone();
        UiBridge::new(
            state.agent.acks.clone(),
            Box::new(move |event: AgentEvent| {
                if on_event.send(event).is_err() {
                    cancel_on_dead_channel.store(true, Ordering::SeqCst);
                }
            }),
        )
    };
    let ctx = ToolCtx {
        engine: state.engine.clone(),
        settings,
        app_data_dir: state.app_data_dir.clone(),
        cancel: cancel.clone(),
        app: Some(app),
        screen: Arc::new(Mutex::new(req_screen_context.clone())),
        ui: Some(ui_bridge),
    };
    let req = AgentRequest {
        mode,
        system_prompt: radar_core::agent::prompts::system_prompt(mode),
        history: input.messages,
        screen_context: req_screen_context,
    };

    // The whole loop is blocking HTTP — off the UI thread. The engine lock
    // is taken only while a tool executes inside the executor and released
    // before each LLM round-trip, so `get_snapshot` polling and the scan
    // worker never block on a slow answer.
    let outcome = off_ui_thread(move || {
        let cancel_on_dead_channel = cancel.clone();
        let mut emit = move |event: AgentEvent| {
            // A dead channel (window closed) must not abort the run
            // mid-tool; flip the cancel flag instead so the loop stops
            // before the next HTTP round-trip.
            if on_event.send(event).is_err() {
                cancel_on_dead_channel.store(true, Ordering::SeqCst);
            }
        };
        let limits = RunLimits::default();
        radar_core::agent::run_agent(
            &cfg,
            &registry,
            &req,
            &limits,
            &cancel,
            &mut |spec, args| dispatch(&ctx, spec, args),
            &mut emit,
        )
    })
    .await?;

    match outcome {
        AgentOutcome::Answer(_) => Ok("answer".into()),
        AgentOutcome::Cancelled => Ok("cancelled".into()),
    }
}

/// Stop the running agent answer; returns whether a run was active.
#[tauri::command]
pub fn agent_cancel(state: tauri::State<'_, AppState>) -> bool {
    state.agent.request_cancel()
}

/// The frontend's ack for one `ui_action` the agent emitted. Returns true
/// when a ui call was still waiting for this `ui_id` — a late or unknown
/// ack (e.g. after a timeout) simply reports false.
#[tauri::command]
pub fn agent_ui_result(
    state: tauri::State<'_, AppState>,
    result: crate::agent_tools::ui::UiResult,
) -> bool {
    state.agent.acks.resolve(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_core::agent::Mode;

    /// Every family registers cleanly (no duplicate names across files) and
    /// `contract_detail` is served in both UI modes. The exact per-mode
    /// tool lists are pinned once all families have landed.
    #[test]
    fn build_registry_registers_every_family_without_name_clashes() {
        let registry = build_registry().expect("all tool families register cleanly");
        for mode in [Mode::Easy, Mode::Pro] {
            let names: Vec<&str> = registry
                .for_mode(mode)
                .iter()
                .map(|spec| spec.name.as_str())
                .collect();
            assert!(
                names.contains(&"contract_detail"),
                "mode {mode:?}: {names:?}"
            );
        }
    }

    /// The tools each mode sees after P3: the P2 read set plus the three ui
    /// tools (Easy 11 = 8 read + 3 ui; Pro 15 = 12 read + 3 ui). P4's
    /// present/save_watchlist land later.
    #[test]
    fn p3_tools_per_mode() {
        let registry = build_registry().expect("all tool families register cleanly");
        let names = |mode| {
            let mut names: Vec<String> = registry
                .for_mode(mode)
                .iter()
                .map(|spec| spec.name.clone())
                .collect();
            names.sort();
            names
        };
        let easy = [
            "contract_detail",
            "find_ideas",
            "market_pulse",
            "my_portfolio",
            "my_profile",
            "read_playbook",
            "stock_brief",
            "ui_go",
            "ui_point",
            "ui_select",
            "what_if",
        ];
        let mut pro: Vec<&str> = easy.to_vec();
        pro.extend([
            "chain_stats",
            "my_watchlists",
            "price_behavior",
            "scan_chain",
        ]);
        pro.sort_unstable();
        assert_eq!(names(Mode::Easy), easy);
        assert_eq!(names(Mode::Pro), pro);
        // ui tools are Tier::Ui, not silent read tools
        for name in ["ui_go", "ui_select", "ui_point"] {
            assert_eq!(
                registry.get(name).map(|spec| spec.tier),
                Some(radar_core::agent::Tier::Ui)
            );
        }
    }

    /// The guard must free the slot on ANY exit — an early `?`/Err return
    /// drops it, so a failed run never leaves the agent stuck "busy".
    #[test]
    fn running_guard_clears_the_flag_on_early_return() {
        fn early_failing_run(control: &AgentControl) -> Result<(), String> {
            let _guard = control.begin_run().expect("first run starts");
            Err("เรียก LLM ไม่สำเร็จ".into())
        }
        let control = AgentControl::new();
        assert!(early_failing_run(&control).is_err());
        assert!(
            !control.running.load(Ordering::SeqCst),
            "the flag must be clear after the error path"
        );
    }

    #[test]
    fn begin_run_rejects_the_second_concurrent_run_and_resets_stale_cancel() {
        let control = AgentControl::new();
        control.cancel.store(true, Ordering::SeqCst); // stale stop from a previous run
        let guard = control.begin_run().expect("first run starts");
        assert!(
            !control.cancel.load(Ordering::SeqCst),
            "a stale cancel must not kill this run"
        );
        assert_eq!(
            control.begin_run().err(),
            Some("AI กำลังตอบคำถามก่อนหน้าอยู่ — กดหยุดก่อน หรือรอให้ตอบเสร็จ")
        );
        drop(guard);
        assert!(
            control.begin_run().is_ok(),
            "the slot frees again once the guard drops"
        );
    }

    /// `agent_cancel` semantics: the stop flag is always set; the return
    /// value reports whether a run was active at the moment of the request.
    #[test]
    fn request_cancel_sets_the_flag_and_reports_whether_a_run_was_active() {
        let control = AgentControl::new();
        assert!(
            !control.request_cancel(),
            "no run yet — the request must report nothing was active"
        );
        assert!(control.cancel.load(Ordering::SeqCst));
        let _guard = control.begin_run().expect("run starts");
        assert!(control.request_cancel(), "an active run is reported");
        assert!(control.cancel.load(Ordering::SeqCst));
    }
}
