//! Executors for the `mine` tool family (W6 P2): the user's portfolio,
//! Easy profile and watchlists, plus `read_playbook` (the playbook family
//! has no executor module of its own in this task's scope, and its data is
//! embedded — so it answers from here).
//!
//! Executors run on the blocking pool: the Webull fetchers below are called
//! directly, and no engine lock is needed for anything this family reads.

use radar_core::agent::tools::{mine, playbook};
use radar_core::webull::watchlists::BrokerWatchlist;
use serde_json::Value;

use super::ToolCtx;

/// `None` = this family does not own `name`.
pub fn execute(ctx: &ToolCtx, name: &str, args: &Value) -> Option<Result<String, String>> {
    match name {
        "my_portfolio" => Some(my_portfolio(ctx)),
        "my_profile" => Some(my_profile(ctx)),
        "my_watchlists" => Some(my_watchlists(ctx)),
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
