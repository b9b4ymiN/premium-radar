//! Executors for the screener tool family (W6 P2-T4): `contract_detail` and
//! `chain_stats`. Both answer purely from the engine snapshot that
//! find_ideas / scan_chain already fetched — the lock is held only for the
//! short moment the snapshot is read, then the pure summariser in
//! `radar_core::agent::tools::screener` runs unlocked.

use std::sync::{Arc, Mutex};

use serde_json::Value;

use radar_core::agent::tools::screener as pure;
use radar_core::engine::Engine;

use super::ToolCtx;
use crate::lock;

/// Nothing scanned yet — both tools need a chain the engine already holds.
const SCAN_FIRST: &str =
    "ยังไม่มีข้อมูลตาราง option ในระบบ — เรียก find_ideas หรือ scan_chain ของหุ้นที่สนใจก่อน แล้วค่อยลองอีกครั้ง";

/// `None` = this family does not own `name`.
pub fn execute(ctx: &ToolCtx, name: &str, args: &Value) -> Option<Result<String, String>> {
    execute_with(&ctx.engine, name, args)
}

/// `execute` on plain inputs. Tests call this instead of building a
/// `ToolCtx` — the test exe must stay free of the Tauri app-handle path
/// (see `market::execute_with`).
fn execute_with(
    engine: &Arc<Mutex<Engine>>,
    name: &str,
    args: &Value,
) -> Option<Result<String, String>> {
    match name {
        "contract_detail" => Some(contract_detail(engine, args)),
        "chain_stats" => Some(chain_stats(engine)),
        _ => None,
    }
}

fn contract_detail(engine: &Arc<Mutex<Engine>>, args: &Value) -> Result<String, String> {
    let ref_symbol = args
        .get("symbol")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_uppercase();
    if ref_symbol.is_empty() {
        return Err(
            "ต้องระบุ symbol เป็นรหัสสัญญา (OCC) เช่น NVDA261218P00175000 — เอาได้จาก find_ideas หรือ scan_chain"
                .into(),
        );
    }
    let (row, spot) = {
        let snap = lock(engine).snapshot();
        (
            snap.rows.iter().find(|r| r.q.symbol == ref_symbol).cloned(),
            snap.spot,
        )
    };
    let Some(row) = row else {
        return Err(format!(
            "ไม่พบสัญญา {ref_symbol} ในข้อมูลที่แอปถืออยู่ — เรียก find_ideas หรือ scan_chain \
             ของหุ้นตัวนั้นก่อน แล้วค่อยดูรายละเอียดสัญญาอีกครั้ง"
        ));
    };
    let out = pure::summarize_contract(&row, spot, &chrono::Utc::now().to_rfc3339());
    serde_json::to_string(&out).map_err(|e| format!("สร้างผลลัพธ์ไม่สำเร็จ: {e}"))
}

fn chain_stats(engine: &Arc<Mutex<Engine>>) -> Result<String, String> {
    let (stats, spot, symbol) = {
        let snap = lock(engine).snapshot();
        (
            snap.stats,
            snap.spot,
            snap.stock
                .as_ref()
                .map(|s| s.symbol.clone())
                .or_else(|| snap.rows.first().map(|r| r.q.underlying.clone())),
        )
    };
    // stats only survive the snapshot when an ATM IV solved, so a chain with
    // no computable stats reads the same as no chain at all
    let (Some(stats), Some(symbol)) = (stats, symbol) else {
        return Err(SCAN_FIRST.into());
    };
    let out = pure::summarize_chain_stats(&stats, spot, &symbol, &chrono::Utc::now().to_rfc3339());
    serde_json::to_string(&out).map_err(|e| format!("สร้างผลลัพธ์ไม่สำเร็จ: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use radar_core::engine::{DataSource, QuoteUpdate};
    use serde_json::json;

    // No `ToolCtx` here — see `execute_with`. A one-row NVDA chain near the
    // money so the enrichment can solve an IV and the snapshot keeps stats.
    fn ready_engine() -> Engine {
        let mut e = Engine::new(Utc::now());
        let occ = "NVDA261218P00190000";
        e.set_symbols(vec![occ.to_string()], Some(191.25));
        e.push_quote(QuoteUpdate {
            symbol: occ.into(),
            price: 3.0,
            volume: 1200.0,
            change: 0.1,
            change_ratio: 0.05,
            oi: 4800.0,
            bid: 2.95,
            ask: 3.05,
            src: DataSource::Rest,
        });
        e
    }

    fn run(engine: Engine, name: &str, args: &Value) -> Option<Result<String, String>> {
        execute_with(&Arc::new(Mutex::new(engine)), name, args)
    }

    #[test]
    fn other_names_stay_with_the_copilot_fallback() {
        assert!(run(Engine::new(Utc::now()), "top_candidates", &json!({})).is_none());
        assert!(run(ready_engine(), "market_pulse", &json!({})).is_none());
    }

    #[test]
    fn contract_detail_without_a_symbol_arg_is_a_thai_error() {
        let err = run(ready_engine(), "contract_detail", &json!({}))
            .expect("family owns contract_detail")
            .expect_err("missing symbol");
        assert!(err.contains("OCC"), "{err}");
    }

    #[test]
    fn contract_detail_unknown_ref_tells_the_model_to_scan_first() {
        let err = run(
            ready_engine(),
            "contract_detail",
            &json!({"symbol": "AAPL261218C00250000"}),
        )
        .expect("family owns contract_detail")
        .expect_err("unknown ref");
        assert!(
            err.contains("find_ideas") && err.contains("scan_chain"),
            "{err}"
        );
    }

    #[test]
    fn contract_detail_returns_the_compact_summary_of_the_scanned_row() {
        let out = run(
            ready_engine(),
            "contract_detail",
            &json!({"symbol": "nvda261218p00190000"}),
        )
        .expect("family owns contract_detail")
        .expect("row found");
        let v: Value = serde_json::from_str(&out).expect("valid json");
        assert_eq!(v["ref"], "NVDA261218P00190000");
        assert_eq!(v["symbol"], "NVDA");
        assert_eq!(v["side"], "put");
        assert_eq!(v["strike"], json!(190));
        // bid 2.95 is the premium the enrichment used, per share → ×100
        assert_eq!(v["premium_per_share_usd"], json!(2.95));
        assert_eq!(v["premium_usd"], json!(295));
        assert_eq!(v["cash_needed_usd"], json!(19000));
        assert_eq!(v["spot"], json!(191.25));
        assert_eq!(v["source"], "engine");
        assert!(serde_json::to_string(&v).unwrap().len() <= 1500);
    }

    #[test]
    fn chain_stats_without_a_scanned_chain_is_a_thai_error() {
        let err = run(Engine::new(Utc::now()), "chain_stats", &json!({}))
            .expect("family owns chain_stats")
            .expect_err("no data");
        assert!(
            err.contains("find_ideas") && err.contains("scan_chain"),
            "{err}"
        );
    }

    #[test]
    fn chain_stats_summarises_the_engine_snapshot() {
        let out = run(ready_engine(), "chain_stats", &json!({}))
            .expect("family owns chain_stats")
            .expect("stats present");
        let v: Value = serde_json::from_str(&out).expect("valid json");
        assert_eq!(v["symbol"], "NVDA");
        assert_eq!(v["spot"], json!(191.25));
        // the single row IS the ATM row, so its IV × 100 is atm_iv_pct
        let row_iv = ready_engine().snapshot().rows[0]
            .iv
            .expect("IV solved for the near-the-money fixture");
        assert_eq!(
            v["atm_iv_pct"],
            json!((row_iv * 100.0 * 100.0).round() / 100.0)
        );
        assert!(v["expected_move_usd"].is_number());
        assert!(v["expected_move_pct"].is_number());
        assert_eq!(v["source"], "engine");
    }
}
