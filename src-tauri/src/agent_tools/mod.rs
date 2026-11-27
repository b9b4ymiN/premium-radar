//! Tool execution for the agent harness (W6 P2): fetches the data each tool
//! needs and hands it to the pure summarisers in `radar_core::agent::tools`.
//! Every family file answers only the tool names it owns (`None` = not
//! mine), so families stay in disjoint files. The runner has already
//! validated `args` against the tool's schema before `dispatch` is called.
//!
//! Executors run on the blocking pool (inside `agent_ask`'s run): blocking
//! Webull calls are fine here, and the engine lock must only be held for
//! the short moment a snapshot is read — never across a network call.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use radar_core::agent::ToolSpec;
use radar_core::engine::Engine;
use serde_json::Value;

use crate::lock;
use crate::settings::AppSettings;

pub mod ideas;
pub mod market;
pub mod mine;
pub mod ui;

/// Everything a tool executor may read. `settings` is a snapshot taken when
/// the run started; `app` is `None` in unit tests (tools that must drive the
/// app — e.g. start a scan — return a Thai error when it is missing);
/// `cancel` is the run's stop flag, which long waits must poll. `screen` is
/// the screen context the UI last reported (seeded from the request, then
/// updated by every ui ack) and `ui` is the bridge that lets a tool drive
/// the screen — `None` when there is no event channel to emit on.
pub struct ToolCtx {
    pub engine: Arc<Mutex<Engine>>,
    pub settings: AppSettings,
    pub app_data_dir: String,
    pub cancel: Arc<AtomicBool>,
    pub app: Option<tauri::AppHandle>,
    pub screen: Arc<Mutex<Option<Value>>>,
    pub ui: Option<ui::UiBridge>,
}

/// Route one validated tool call to its family. `Ok` is the compact JSON
/// string the model reads; `Err` is a Thai message telling the model (and
/// through it the user) what to do next. The ui family answers first: its
/// results are plain JSON strings too, and only it knows how to park on the
/// frontend ack.
pub fn dispatch(ctx: &ToolCtx, spec: &ToolSpec, args: &Value) -> Result<String, String> {
    let name = spec.name.as_str();
    if let Some(result) = ui::execute(ctx, name, args) {
        return result;
    }
    if let Some(result) = market::execute(ctx, name, args) {
        return result;
    }
    if let Some(result) = ideas::execute(ctx, name, args) {
        return result;
    }
    if let Some(result) = mine::execute(ctx, name, args) {
        return result;
    }
    // the screener tools still answer through the copilot executor
    let mut engine = lock(&ctx.engine);
    Ok(radar_core::copilot::execute_tool(
        name,
        &args.to_string(),
        &mut engine,
    ))
}
