//! Executors for the `ui` tool family (W6 P3). `plan_ui_call` (pure, in
//! radar-core) checks the call against the screen the UI last reported; this
//! file then emits one `AgentEvent::UiAction` on the run's event channel,
//! parks the tool until the frontend acks through the `agent_ui_result`
//! command (or the 5 s budget / the cancel flag ends the wait), and keeps
//! the ack's screen context as the new current screen.
//!
//! The ack table and the wait are Tauri-free on purpose: unit tests exercise
//! them without ever building a `ToolCtx` (which carries an AppHandle and an
//! ipc Channel — the test exe fails to load on Windows when those are
//! linked in, 0xC0000139).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use radar_core::agent::AgentEvent;
use radar_core::agent::tools::ui::plan_ui_call;
use serde::Deserialize;
use serde_json::{Value, json};

use super::ToolCtx;

/// รอ ack จาก UI นานสุด 5 วินาที (ตามสัญญาใน `src/agent/types.ts`)
const ACK_TIMEOUT: Duration = Duration::from_secs(5);

/// ช่วงพักระหว่างที่รอ ack เพื่อกลับมาเช็ค cancel flag
const CANCEL_POLL: Duration = Duration::from_millis(50);

/// Payload ของคำสั่ง `agent_ui_result` — ack ของ UI สำหรับ ui_action
/// หนึ่งงาน (รูปตรงกับ `UiResult` ใน `src/agent/types.ts`)
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct UiResult {
    pub ui_id: String,
    pub ok: bool,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub screen_context: Option<Value>,
}

/// ตาราง ack ค้างอยู่: ui_id → ช่องทางส่งผลกลับให้ tool ที่กำลังรอ
/// แชร์ระหว่าง `AgentControl` (ฝั่ง command) กับ `UiBridge` ฝั่ง executor
pub struct PendingAcks {
    waiters: Mutex<HashMap<String, Sender<UiResult>>>,
    next_id: AtomicU64,
}

impl PendingAcks {
    pub fn new() -> Self {
        Self {
            waiters: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
        }
    }

    /// จอง ui_id ตัวใหม่พร้อมช่องรอผล — ต้องเรียกก่อน emit UiAction เพื่อให้
    /// ack ที่มาเร็วที่สุดไม่หลุด
    pub fn register(&self) -> (String, Receiver<UiResult>) {
        let id = format!("ui-{}", self.next_id.fetch_add(1, Ordering::SeqCst) + 1);
        let (tx, rx) = std::sync::mpsc::channel();
        crate::lock(&self.waiters).insert(id.clone(), tx);
        (id, rx)
    }

    /// ส่ง ack ให้ผู้รอ — คืน true เมื่อมีผู้รอ ui_id นั้นอยู่จริง
    pub fn resolve(&self, result: UiResult) -> bool {
        let sender = crate::lock(&self.waiters).remove(&result.ui_id);
        match sender {
            Some(tx) => {
                // ผู้รอเลิกรอไปแล้ว (timeout/cancel) — ack หายไปเฉยๆ ถือว่าจบ
                let _ = tx.send(result);
                true
            }
            None => false,
        }
    }

    /// ล้างรายการรอ (ทุก path ของ executor เรียกเสมอ — timeout/cancel ก็ต้อง
    /// ไม่ leak ช่องเก่าไว้ในตาราง)
    pub fn remove(&self, ui_id: &str) {
        crate::lock(&self.waiters).remove(ui_id);
    }
}

impl Default for PendingAcks {
    fn default() -> Self {
        Self::new()
    }
}

/// ผลของการรอ ack
#[derive(Debug, PartialEq)]
pub enum AckOutcome {
    Delivered(UiResult),
    TimedOut,
    Cancelled,
}

/// รอ ack โดยไล่ดู cancel flag คู่ไปด้วย — ไม่ยึดติดกับ Channel ใดๆ
/// จึงทดสอบแบบ pure ได้ (ผู้ส่งถูกทิ้งกลางคัน = จบแบบ TimedOut)
pub fn wait_for_ack(rx: &Receiver<UiResult>, cancel: &AtomicBool, timeout: Duration) -> AckOutcome {
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return AckOutcome::Cancelled;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return AckOutcome::TimedOut;
        }
        match rx.recv_timeout(remaining.min(CANCEL_POLL)) {
            Ok(result) => return AckOutcome::Delivered(result),
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return AckOutcome::TimedOut,
        }
    }
}

/// สะพานสั่งหน้าจอของ run หนึ่งๆ: ส่ง UiAction ออกทาง event channel ตัว
/// เดียวกับที่ stream เหตุการณ์อื่นของ run และรอ ack ผ่านตารางเดียวกับ
/// คำสั่ง `agent_ui_result`
pub struct UiBridge {
    acks: Arc<PendingAcks>,
    emit: Box<dyn Fn(AgentEvent) + Send>,
}

impl UiBridge {
    pub(crate) fn new(acks: Arc<PendingAcks>, emit: Box<dyn Fn(AgentEvent) + Send>) -> Self {
        Self { acks, emit }
    }
}

/// `None` = this family does not own `name`.
pub fn execute(ctx: &ToolCtx, name: &str, args: &Value) -> Option<Result<String, String>> {
    if !matches!(name, "ui_go" | "ui_select" | "ui_point") {
        return None;
    }
    Some(execute_ui(ctx, name, args))
}

/// ส่ง UiAction หนึ่งออกทาง bridge ของ run แล้วรอ ack จนกว่าจะหมดเวลา —
/// แบบเดียวกับที่ ui tools ใช้ เผื่อครอบครัวอื่น (เช่น `present_idea`)
/// สั่ง UI ด้วย action ที่สร้างเองแล้วอยากรอผลด้วย
/// `Err` = run นี้ไม่มี bridge (ข้อความเป็น error JSON สำหรับโมเดล)
pub(crate) fn emit_and_wait(
    ctx: &ToolCtx,
    action: Value,
    timeout: Duration,
) -> Result<AckOutcome, String> {
    let Some(bridge) = ctx.ui.as_ref() else {
        return Err(json!({
            "error": "ui_unavailable",
            "message": "ระบบหน้าจอไม่พร้อมใช้งานในตอนนี้ — ตอบผู้ใช้เป็นข้อความแทน",
        })
        .to_string());
    };

    let (ui_id, rx) = bridge.acks.register();
    (bridge.emit)(AgentEvent::UiAction {
        ui_id: ui_id.clone(),
        action,
    });
    let outcome = wait_for_ack(&rx, &ctx.cancel, timeout);
    bridge.acks.remove(&ui_id);
    Ok(outcome)
}

fn execute_ui(ctx: &ToolCtx, name: &str, args: &Value) -> Result<String, String> {
    let screen = crate::lock(&ctx.screen).clone();
    let action = match plan_ui_call(name, args, screen.as_ref()) {
        Ok(action) => action,
        // error JSON ส่งกลับเป็น Ok เฉยๆ — โมเดลต้องอ่าน hint/allowed
        // เพื่อแก้การเรียกเองได้
        Err(error) => return Ok(error.to_string()),
    };

    let outcome = match emit_and_wait(ctx, action, ACK_TIMEOUT) {
        Err(unavailable) => return Ok(unavailable),
        Ok(outcome) => outcome,
    };

    match outcome {
        AckOutcome::Cancelled => Err("หยุดตามที่ผู้ใช้สั่ง".into()),
        AckOutcome::TimedOut => Ok(json!({
            "error": "ui_timeout",
            "message": "หน้าจอไม่ตอบกลับภายใน 5 วินาที — ไม่แน่ใจว่าทำตามคำสั่งสำเร็จหรือไม่ บอกผู้ใช้ตรงๆ และเสนอลองใหม่ หรือตอบจากข้อมูลที่มีอยู่แทน",
        })
        .to_string()),
        AckOutcome::Delivered(ack) => {
            if !ack.ok {
                return Ok(json!({
                    "error": "ui_failed",
                    "message": ack.detail.unwrap_or_else(|| "หน้าจอทำตามคำสั่งไม่สำเร็จ".into()),
                })
                .to_string());
            }
            // screen ที่ ack พามา = สิ่งที่ผู้ใช้เห็นตอนนี้ ใช้ตรวจการเรียก
            // ui ครั้งถัดไปของ run เดียวกัน
            if let Some(new_screen) = ack.screen_context.filter(|s| s.is_object()) {
                *crate::lock(&ctx.screen) = Some(new_screen);
            }
            let current = crate::lock(&ctx.screen).clone();
            Ok(render_success(current.as_ref()).to_string())
        }
    }
}

/// ผลลัพธ์ success ที่โมเดลอ่านต่อ: หน้าที่เปิดอยู่ + anchor แรกสุด 30 ตัว
/// เพื่อให้รู้ว่าตอนนี้ชี้อะไรได้บ้าง
fn render_success(screen: Option<&Value>) -> Value {
    let page = screen
        .and_then(|s| s.get("page"))
        .cloned()
        .unwrap_or(Value::Null);
    let anchors: Vec<Value> = screen
        .and_then(|s| s.get("anchors"))
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .take(30)
                .filter_map(|r| r.get("id").cloned())
                .collect()
        })
        .unwrap_or_default();
    json!({"ok": true, "page": page, "anchors": anchors})
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ack(ui_id: &str, ok: bool) -> UiResult {
        UiResult {
            ui_id: ui_id.into(),
            ok,
            detail: None,
            screen_context: None,
        }
    }

    #[test]
    fn ui_result_decodes_the_wire_shape_from_types_ts() {
        let result: UiResult = serde_json::from_value(json!({
            "ui_id": "ui-7",
            "ok": false,
            "detail": "anchor หายไปจากหน้าจอแล้ว",
            "screen_context": null
        }))
        .expect("decode");
        assert_eq!(result.ui_id, "ui-7");
        assert!(!result.ok);
        assert_eq!(result.detail.as_deref(), Some("anchor หายไปจากหน้าจอแล้ว"));
        // ช่อง optional ที่ไม่ส่งมาต้อง decode เป็น None ได้
        let bare: UiResult =
            serde_json::from_value(json!({"ui_id": "ui-1", "ok": true})).expect("decode bare");
        assert_eq!(
            bare,
            UiResult {
                ui_id: "ui-1".into(),
                ok: true,
                detail: None,
                screen_context: None,
            }
        );
    }

    #[test]
    fn register_makes_increasing_ids_and_resolve_delivers_once() {
        let acks = PendingAcks::new();
        let (first, rx) = acks.register();
        let (second, _) = acks.register();
        assert_eq!(first, "ui-1");
        assert_eq!(second, "ui-2");

        assert!(acks.resolve(ack(&first, true)));
        let outcome = wait_for_ack(&rx, &AtomicBool::new(false), Duration::from_millis(200));
        assert_eq!(outcome, AckOutcome::Delivered(ack(&first, true)));
        // ack ซ้ำ/ui_id เก่า → ไม่มีผู้รอแล้ว
        assert!(!acks.resolve(ack(&first, true)));
    }

    #[test]
    fn resolve_unknown_id_returns_false() {
        let acks = PendingAcks::new();
        assert!(!acks.resolve(ack("ui-99", true)));
    }

    #[test]
    fn wait_times_out_when_no_ack_arrives() {
        let acks = PendingAcks::new();
        let (id, rx) = acks.register();
        let started = Instant::now();
        let outcome = wait_for_ack(&rx, &AtomicBool::new(false), Duration::from_millis(80));
        assert_eq!(outcome, AckOutcome::TimedOut);
        assert!(started.elapsed() >= Duration::from_millis(80));
        assert_ne!(id, "");
    }

    #[test]
    fn cancel_set_before_or_during_the_wait_stops_it() {
        let acks = PendingAcks::new();
        // ตั้ง cancel ไว้ก่อนแล้ว — จบทันทีไม่ต้องรอ timeout
        let (_, rx) = acks.register();
        let flag = AtomicBool::new(true);
        let started = Instant::now();
        assert_eq!(
            wait_for_ack(&rx, &flag, Duration::from_secs(5)),
            AckOutcome::Cancelled
        );
        assert!(started.elapsed() < Duration::from_secs(1));

        // cancel มาทีหลังระหว่างรอ — ยกเลิกก่อน deadline
        let (_, rx) = acks.register();
        let flag = Arc::new(AtomicBool::new(false));
        let setter = {
            let flag = flag.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(30));
                flag.store(true, Ordering::SeqCst);
            })
        };
        assert_eq!(
            wait_for_ack(&rx, &flag, Duration::from_secs(5)),
            AckOutcome::Cancelled
        );
        assert!(setter.join().is_ok());
    }

    #[test]
    fn remove_drops_the_waiter_so_late_acks_find_nothing() {
        let acks = PendingAcks::new();
        let (id, rx) = acks.register();
        acks.remove(&id);
        assert!(!acks.resolve(ack(&id, true)), "entry must be gone");
        // ผู้ส่งถูกทิ้ง → รอต่อจบเป็น timeout ไม่ใช่ค้างเป็นก้อน
        assert_eq!(
            wait_for_ack(&rx, &AtomicBool::new(false), Duration::from_millis(50)),
            AckOutcome::TimedOut
        );
    }

    #[test]
    fn success_payload_reads_page_and_caps_anchors_at_30() {
        let anchors: Vec<Value> = (0..40)
            .map(|i| json!({"id": format!("a:{i}"), "label": "x"}))
            .collect();
        let screen = json!({"mode": "pro", "page": "monitor", "anchors": anchors});
        let out = render_success(Some(&screen));
        assert_eq!(out["ok"], json!(true));
        assert_eq!(out["page"], json!("monitor"));
        assert_eq!(out["anchors"].as_array().unwrap().len(), 30);
        assert_eq!(out["anchors"][0], json!("a:0"));
        assert_eq!(out["anchors"][29], json!("a:29"));

        let none = render_success(None);
        assert_eq!(none["ok"], json!(true));
        assert_eq!(none["page"], json!(null));
        assert_eq!(none["anchors"], json!([]));
    }
}
