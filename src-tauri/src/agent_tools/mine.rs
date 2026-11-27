//! Executors for the `mine` tool family (W6 P2): the user's portfolio,
//! Easy profile and watchlists, plus `read_playbook` (the playbook family
//! has no executor module of its own in this task's scope, and its data is
//! embedded — so it answers from here).
//!
//! Executors run on the blocking pool: the Webull fetchers below are called
//! directly, and no engine lock is needed for anything this family reads.

use std::time::Duration;

use radar_core::agent::tools::{mine, playbook};
use radar_core::watchlists::WatchlistStore;
use radar_core::webull::watchlists::BrokerWatchlist;
use serde_json::{Value, json};

use super::ToolCtx;
use super::ui::{self, AckOutcome};

/// รอผู้ใช้กดอนุญาตบนการ์ดยืนยันนานสุด 120 วินาที — spec ของ tool ให้ 125 วินาที
/// (เผื่อเวลาเขียนไฟล์หลังผู้ใช้กด)
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(120);

/// `None` = this family does not own `name`.
pub fn execute(ctx: &ToolCtx, name: &str, args: &Value) -> Option<Result<String, String>> {
    match name {
        "my_portfolio" => Some(my_portfolio(ctx)),
        "my_profile" => Some(my_profile(ctx)),
        "my_watchlists" => Some(my_watchlists(ctx)),
        "save_watchlist" => Some(save_watchlist(ctx, args)),
        "read_playbook" => Some(read_playbook(args)),
        _ => None,
    }
}

/// Mirrors the `get_portfolio` command: first account only, and a
/// balance/orders failure degrades into the answer's `*_error` fields
/// instead of failing the whole tool. The account identifier is used for
/// the fetch calls and never reaches the summariser.
fn my_portfolio(ctx: &ToolCtx) -> Result<String, String> {
    use radar_core::webull::trading::{
        fetch_accounts, fetch_balance, fetch_open_orders, fetch_positions,
    };
    let as_of = chrono::Utc::now();
    let Some(mut client) = ctx.settings.webull_client() else {
        return Err("ยังไม่ได้ตั้ง credentials — เปิดหน้า Setup ก่อน แล้วค่อยถามพอร์ตอีกครั้ง".into());
    };
    let accounts = fetch_accounts(&mut client)
        .map_err(|e| format!("ดึงบัญชีไม่สำเร็จ: {} — เดี๋ยวลองใหม่อีกครั้ง", e.user_message()))?;
    let Some(acc) = accounts.first() else {
        return Ok(serde_json::json!({
            "note": "ไม่พบบัญชีในสิทธิ์นี้ — ตรวจว่า API key เปิดสิทธิ์อ่านการเทรดแล้ว",
            "as_of": as_of.to_rfc3339(),
            "source": "webull_rest",
        })
        .to_string());
    };
    let positions = fetch_positions(&mut client, &acc.account_id).map_err(|e| {
        format!(
            "ดึงสถานะถือครองไม่สำเร็จ: {} — เดี๋ยวลองใหม่อีกครั้ง",
            e.user_message()
        )
    })?;
    let (balance, balance_error) = match fetch_balance(&mut client, &acc.account_id) {
        Ok(balance) => (Some(balance), None),
        Err(e) => (None, Some(format!("ดึงยอดเงินไม่สำเร็จ: {}", e.user_message()))),
    };
    let (orders, orders_error) = match fetch_open_orders(&mut client, &acc.account_id) {
        Ok(orders) => (orders, None),
        Err(e) => (
            Vec::new(),
            Some(format!("ดึงคำสั่งที่ยังค้างไม่สำเร็จ: {}", e.user_message())),
        ),
    };
    Ok(mine::summarize_portfolio(
        &positions,
        balance.as_ref(),
        balance_error.as_deref(),
        &orders,
        orders_error.as_deref(),
        as_of,
    )
    .to_string())
}

fn my_profile(ctx: &ToolCtx) -> Result<String, String> {
    Ok(mine::summarize_profile(&ctx.settings.easy_profile, chrono::Utc::now()).to_string())
}

fn my_watchlists(ctx: &ToolCtx) -> Result<String, String> {
    use radar_core::webull::watchlists::fetch_watchlists;
    let as_of = chrono::Utc::now();
    let store = crate::load_watchlists(&ctx.app_data_dir);
    // the broker half reuses the same 60s cache entry as get_broker_watchlists
    // (same key, same shape) so an agent answer and the UI share one call
    let (broker, broker_error) = match broker_lists_cached() {
        Some(lists) => (lists, None),
        None => match ctx.settings.webull_client() {
            None => (
                Vec::new(),
                Some(
                    "ยังไม่ได้ตั้ง credentials — เปิดหน้า Setup ก่อน จึงจะดู watchlist ของโบรกเกอร์ได้".into(),
                ),
            ),
            Some(mut client) => match fetch_watchlists(&mut client) {
                Ok(lists) => {
                    cache_broker_lists(&lists);
                    (lists, None)
                }
                Err(e) => (
                    Vec::new(),
                    Some(format!(
                        "ดึง watchlist ของโบรกเกอร์ไม่สำเร็จ: {}",
                        e.user_message()
                    )),
                ),
            },
        },
    };
    Ok(
        mine::summarize_watchlists(&store, Some(&broker), broker_error.as_deref(), as_of)
            .to_string(),
    )
}

/// Broker lists from the shared cache, mapped back into the domain type.
/// A cache entry whose shape drifted returns `None` so the caller fetches.
fn broker_lists_cached() -> Option<Vec<BrokerWatchlist>> {
    let cached = crate::lock(&crate::MOVERS_CACHE).get("broker-watchlists")?;
    let rows = cached.get("lists")?.as_array()?;
    if rows.is_empty() {
        return None;
    }
    Some(
        rows.iter()
            .filter_map(|row| {
                Some(BrokerWatchlist {
                    id: row.get("id")?.as_str()?.to_string(),
                    name: row
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("ไม่มีชื่อ")
                        .to_string(),
                    update_time: None,
                })
            })
            .collect(),
    )
}

/// Refresh the shared cache entry in the exact shape `get_broker_watchlists`
/// writes, so the UI can reuse the fetch this tool just paid for.
fn cache_broker_lists(lists: &[BrokerWatchlist]) {
    let value = serde_json::json!({
        "lists": lists,
        "fetched_at": chrono::Utc::now().to_rfc3339(),
        "source": "REST",
    });
    crate::lock(&crate::MOVERS_CACHE).put("broker-watchlists", value);
}

fn read_playbook(args: &Value) -> Result<String, String> {
    let name = args.get("name").and_then(Value::as_str).unwrap_or("");
    Ok(playbook::summarize_playbook(name, chrono::Utc::now())?.to_string())
}

// ---------- save_watchlist (P4-T3): the first local write ----------

/// Trim + uppercase, then check `^[A-Z][A-Z0-9.\-]{0,9}$` by hand (no regex
/// crate) — กติกาเดียวกับ `ticker_arg` ของ market family เลย เพราะ scope ของ
/// task นี้แตะ market.rs ไม่ได้
fn ticker_arg(args: &Value) -> Result<String, String> {
    let symbol = args
        .get("symbol")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_uppercase();
    let mut chars = symbol.chars();
    let shape_ok = matches!(chars.next(), Some(c) if c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && symbol.len() <= 10;
    if shape_ok {
        Ok(symbol)
    } else {
        Err(format!(
            "รหัสหุ้น '{symbol}' ไม่ถูกต้อง — ใช้ตัวอักษรภาษาอังกฤษขึ้นต้นด้วย A-Z ตามด้วยตัวอักษร ตัวเลข . หรือ - ไม่เกิน 10 ตัว เช่น NVDA หรือ BRK.B"
        ))
    }
}

/// หาลิสต์เป้าหมายก่อนถามผู้ใช้: ระบุชื่อ → ต้องมีชื่อนั้นจริง, ไม่ระบุ → ลิสต์
/// ที่กำลังใช้อยู่ — ไม่พบคืน error JSON พร้อมชื่อลิสต์ที่มี ให้โมเดลแก้การเรียก
fn resolve_target_list(
    store: &WatchlistStore,
    list_name: Option<&str>,
) -> Result<(String, String), Value> {
    let joined_names = || {
        store
            .lists
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    match list_name {
        Some(name) => match store.lists.iter().find(|l| l.name == name) {
            Some(list) => Ok((list.id.clone(), list.name.clone())),
            None => Err(json!({
                "error": "list_not_found",
                "message": format!(
                    "ไม่พบลิสต์ชื่อ '{name}' — ลิสต์ที่มีอยู่: {} (เรียก my_watchlists ดูได้)",
                    joined_names()
                ),
            })),
        },
        None => match store.active_list() {
            Some(list) => Ok((list.id.clone(), list.name.clone())),
            None => Err(json!({
                "error": "no_active_list",
                "message": format!(
                    "ยังไม่มีลิสต์ที่กำลังใช้ — ระบุชื่อลิสต์ในช่อง list โดยเลือกจาก: {}",
                    joined_names()
                ),
            })),
        },
    }
}

/// ประตูก่อนแตะไฟล์: การเขียนทุกครั้งต้องผ่านการ์ดยืนยันบนหน้าจอ — ไม่มี ui
/// bridge = ห้ามเขียนเลย คืน error JSON สำหรับโมเดล (`Some`) / เดินหน้าต่อ (`None`)
fn write_gate(ui_available: bool) -> Option<String> {
    (!ui_available).then(|| {
        json!({
            "ok": false,
            "error": "ui_unavailable",
            "message": "หน้าจอไม่พร้อมใช้งาน — การแก้ลิสต์ต้องมีผู้ใช้กดอนุญาตบนการ์ดยืนยันเสมอ ตอบผู้ใช้เป็นข้อความแล้วชวนลองใหม่ภายหลัง",
        })
        .to_string()
    })
}

/// ตัวเขียนเดียวของ tool นี้ ใช้ core fns ชุดเดียวกับคำสั่ง `save_watchlist`
/// ของแอป — เรียกหลังผู้ใช้กดอนุญาตเท่านั้น คืน "added"/"removed" หรือ
/// ข้อความไทยจาก `WatchlistError::reason_th`
fn apply_watchlist_change(
    store: &mut WatchlistStore,
    action: &str,
    symbol: &str,
    list_id: &str,
) -> Result<&'static str, String> {
    let Some(list) = store.lists.iter_mut().find(|l| l.id == list_id) else {
        return Err("ไม่พบลิสต์นี้".into());
    };
    match action {
        "add" => radar_core::watchlists::add_symbol(list, symbol)
            .map_err(|e| e.reason_th())
            .map(|()| "added"),
        "remove" => {
            radar_core::watchlists::remove_symbol(list, symbol);
            Ok("removed")
        }
        other => Err(format!("ไม่รู้จัก action: {other}")),
    }
}

/// เพิ่ม/เอาหุ้นออกจากลิสต์ของแอปเอง — ตรวจทุกอย่างก่อนถามผู้ใช้ (ticker,
/// ลิสต์ที่มีจริง) แล้วโชว์การ์ด `{kind:"confirm"}` รอได้สูงสุด 120 วินาที:
/// อนุญาต → เขียนผ่าน `apply_watchlist_change` + `crate::save_watchlists`;
/// ปฏิเสธ → `refused:true` พร้อมข้อความห้ามขอซ้ำ; หมดเวลา → `confirm_timeout`;
/// กดหยุด → Err ให้ run จบเป็น cancelled; ไม่มีหน้าจอ → ไม่เขียนเลย
fn save_watchlist(ctx: &ToolCtx, args: &Value) -> Result<String, String> {
    let Some(action) = args
        .get("action")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    else {
        return Ok(json!({
            "error": "bad_args",
            "message": "ขาดช่อง action — ใส่ \"add\" (เพิ่มหุ้น) หรือ \"remove\" (เอาหุ้นออก) แล้วเรียกใหม่",
        })
        .to_string());
    };
    let symbol = ticker_arg(args)?;
    let list_param = args
        .get("list")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    // ชุดเริ่มต้นเหมือนคำสั่ง save_watchlist: ยังไม่มีไฟล์ + ไม่เคยสร้าง → default
    let mut store = crate::load_watchlists(&ctx.app_data_dir);
    if store.lists.is_empty() {
        store = WatchlistStore::with_default("NVDA");
    }
    let (list_id, list_name) = match resolve_target_list(&store, list_param) {
        Ok(target) => target,
        Err(error) => return Ok(error.to_string()),
    };

    // ประตูเขียน — อยู่หลังการตรวจ args (ให้โมเดลเห็น error แก้เองได้) แต่ก่อน
    // การ์ดยืนยันเสมอ: ไม่มีหน้าจอ = ไม่มีทางเขียน
    if let Some(blocked) = write_gate(ctx.ui.is_some()) {
        return Ok(blocked);
    }

    let (title, detail) = mine::confirm_text(&action, &symbol, &list_name);
    let confirm_action = json!({"kind": "confirm", "title": title, "detail": detail});
    let outcome = match ui::emit_and_wait(ctx, confirm_action, CONFIRM_TIMEOUT) {
        Ok(outcome) => outcome,
        // เกิดได้แม้เพิ่งเช็ค write_gate ผ่าน (bridge ถูกหยิบออกระหว่างทาง) —
        // ยังไม่เขียนอะไร ส่ง error ต่อให้โมเดลเหมือนเดิม
        Err(unavailable) => return Ok(unavailable),
    };

    match outcome {
        AckOutcome::Cancelled => Err("หยุดตามที่ผู้ใช้สั่ง".into()),
        AckOutcome::TimedOut => Ok(json!({
            "ok": false,
            "error": "confirm_timeout",
            "message": "ผู้ใช้ยังไม่กดอนุญาตภายใน 2 นาที — ยังไม่ได้แก้ลิสต์ แจ้งผู้ใช้ก่อน แล้วค่อยถามใหม่เมื่อผู้ใช้พร้อมกดอนุญาต",
        })
        .to_string()),
        AckOutcome::Delivered(ack) if !ack.ok => Ok(json!({
            "ok": false,
            "refused": true,
            "message": "ผู้ใช้ไม่อนุญาต — อย่าขอซ้ำในคำถามนี้ บอกผู้ใช้ว่ารับทราบ",
        })
        .to_string()),
        AckOutcome::Delivered(_) => {
            // the user may have edited the lists while the card was open —
            // re-read so the approved change lands on the current file
            let mut store = crate::load_watchlists(&ctx.app_data_dir);
            if store.lists.is_empty() {
                store = WatchlistStore::with_default("NVDA");
            }
            let done = match apply_watchlist_change(&mut store, &action, &symbol, &list_id) {
                Ok(done) => done,
                Err(reason) => {
                    return Ok(json!({
                        "ok": false,
                        "error": "apply_failed",
                        "message": reason,
                    })
                    .to_string())
                }
            };
            crate::save_watchlists(&ctx.app_data_dir, &store)
                .map_err(|e| format!("บันทึก watchlist ไม่สำเร็จ: {e}"))?;
            let symbols: Vec<&str> = store
                .lists
                .iter()
                .find(|l| l.id == list_id)
                .map(|l| l.symbols.iter().take(30).map(String::as_str).collect())
                .unwrap_or_default();
            Ok(json!({
                "ok": true,
                "done": done,
                "list": list_name,
                "symbols": symbols,
            })
            .to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> WatchlistStore {
        let mut store = WatchlistStore::with_default("NVDA");
        store.create_list("เทรดสั้น").expect("create list");
        store
    }

    #[test]
    fn apply_adds_and_removes_through_the_core_fns() {
        let mut s = store();
        let id = s.lists[0].id.clone();
        assert_eq!(
            apply_watchlist_change(&mut s, "add", "VLO", &id).unwrap(),
            "added"
        );
        assert_eq!(s.lists[0].symbols, vec!["NVDA", "VLO"]);
        assert_eq!(
            apply_watchlist_change(&mut s, "remove", "NVDA", &id).unwrap(),
            "removed"
        );
        assert_eq!(s.lists[0].symbols, vec!["VLO"]);
    }

    #[test]
    fn apply_duplicate_add_surfaces_the_thai_error() {
        let mut s = store();
        let id = s.lists[0].id.clone();
        let err = apply_watchlist_change(&mut s, "add", "NVDA", &id).unwrap_err();
        assert_eq!(err, "หุ้นตัวนี้อยู่ในลิสต์แล้ว");
        assert_eq!(s.lists[0].symbols, vec!["NVDA"], "duplicate adds nothing");
    }

    #[test]
    fn apply_unknown_list_or_action_fails_without_touching_the_store() {
        let mut s = store();
        assert_eq!(
            apply_watchlist_change(&mut s, "add", "VLO", "wl-999").unwrap_err(),
            "ไม่พบลิสต์นี้"
        );
        let first_id = s.lists[0].id.clone();
        assert_eq!(
            apply_watchlist_change(&mut s, "rename", "VLO", &first_id).unwrap_err(),
            "ไม่รู้จัก action: rename"
        );
        assert_eq!(s.lists[0].symbols, vec!["NVDA"], "nothing changed");
    }

    #[test]
    fn resolve_target_list_picks_named_or_active_and_lists_names_on_miss() {
        let s = store();
        let (id, name) = resolve_target_list(&s, Some("เทรดสั้น")).unwrap();
        assert_eq!(
            (name.as_str(), id.as_str()),
            ("เทรดสั้น", s.lists[1].id.as_str())
        );
        let (id, name) = resolve_target_list(&s, None).unwrap();
        assert_eq!(
            (name.as_str(), id.as_str()),
            ("รายการหลัก", s.lists[0].id.as_str()),
            "no name ⇒ the active list"
        );

        let miss = resolve_target_list(&s, Some("ไม่มีลิสต์นี้")).unwrap_err();
        assert_eq!(miss["error"], "list_not_found");
        let msg = miss["message"].as_str().unwrap();
        assert!(
            msg.contains("รายการหลัก") && msg.contains("เทรดสั้น"),
            "must list the real names: {msg}"
        );

        let mut no_active = store();
        no_active.active = None;
        let err = resolve_target_list(&no_active, None).unwrap_err();
        assert_eq!(err["error"], "no_active_list");
    }

    /// Runner-level guard: no ui bridge ⇒ the executor returns the
    /// `ui_unavailable` JSON before any store mutation — `apply_watchlist_change`
    /// (the only writer) sits behind an approved confirm, which needs the bridge.
    #[test]
    fn write_gate_blocks_every_write_when_there_is_no_ui_bridge() {
        let blocked = write_gate(false).unwrap();
        let v: Value = serde_json::from_str(&blocked).expect("valid json");
        assert_eq!(v["ok"], json!(false));
        assert_eq!(v["error"], "ui_unavailable");
        assert!(
            v["message"].as_str().is_some_and(|m| !m.is_empty()),
            "model needs a Thai hint: {blocked}"
        );
        assert!(write_gate(true).is_none(), "ui present ⇒ the gate opens");
    }
}
