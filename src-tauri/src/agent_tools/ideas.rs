//! Executors for the `ideas` tool family (W6 P2): find_ideas, scan_chain,
//! what_if. Also home of `ensure_scanned` — the shared "make the engine hold
//! a READY chain for this symbol" helper that starts the same scan the main
//! screen's button starts, then polls until quotes arrive.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::{NaiveDate, Utc};
use radar_core::agent::tools::ideas::{
    WhatIfInput, premium_per_share, summarize_easy_ideas, summarize_universe_ideas,
    summarize_what_if,
};
use radar_core::easy::{build_ideas, source_token};
use radar_core::easy_rank::rank_universe;
use radar_core::engine::Engine;
use radar_core::quant::{OptType, realized_vol};
use radar_core::screener::{Filters, Mode};
use radar_core::webull::stocks::fetch_stock_bars;
use serde_json::{Value, json};
use tauri::Manager;

use super::ToolCtx;
use crate::{AppState, lock};

const POLL_MS: u64 = 500;
const TIMEOUT_MS: u64 = 90_000;

/// `None` = this family does not own `name`.
pub fn execute(ctx: &ToolCtx, name: &str, args: &Value) -> Option<Result<String, String>> {
    match name {
        "find_ideas" => Some(run_find_ideas(ctx, args)),
        "scan_chain" => Some(run_scan_chain(ctx, args)),
        "what_if" => Some(run_what_if(ctx, args)),
        _ => None,
    }
}

// ---------- ensure_scanned ----------

/// The chain for `symbol` is READY when the engine snapshot has a live spot
/// and at least one row of that underlying — the scan itself never
/// "finishes" (it keeps streaming), so this is the readiness contract.
fn chain_is_ready(engine: &mut Engine, symbol: &str) -> bool {
    let snap = engine.snapshot();
    snap.spot.unwrap_or(0.0) > 0.0 && snap.rows.iter().any(|r| r.q.underlying == symbol)
}

/// Poll until READY, watching the run's stop flag. Checks cancel and
/// readiness before the deadline so an already-ready chain never sleeps.
fn wait_until_ready(
    engine: &Arc<Mutex<Engine>>,
    cancel: &AtomicBool,
    symbol: &str,
    poll_ms: u64,
    timeout_ms: u64,
) -> Result<(), String> {
    let poll = Duration::from_millis(poll_ms);
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("หยุดตามที่ผู้ใช้สั่ง".into());
        }
        {
            let mut guard = lock(engine);
            if chain_is_ready(&mut guard, symbol) {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "รอข้อมูลของ {symbol} ไม่สำเร็จภายใน {} วินาที — สแกนอาจช้าหรือการเชื่อมต่อมีปัญหา \
                 ลองเรียกเครื่องมือนี้อีกครั้ง",
                timeout_ms / 1000
            ));
        }
        std::thread::sleep(poll);
    }
}

/// Thai error for "cannot start a scan from here" — pure so a test can
/// assert on it without touching the Tauri app-handle path.
fn no_app_error(symbol: &str) -> String {
    format!(
        "ยังไม่มีข้อมูลของ {symbol} ในหน้าจอหลัก และตอนนี้เริ่มสแกนใหม่ให้เองไม่ได้ \
         — ขอให้ผู้ใช้เปิดหน้าจอหลักของแอปแล้วกดสแกน {symbol} ก่อน จากนั้นเรียกเครื่องมือนี้อีกครั้ง"
    )
}

/// `ensure_scanned` with the scan start injected as a closure, so the
/// wait/cancel/timeout logic is testable without any AppHandle: tests pass
/// a plain `FnOnce() -> Result<(), String>` instead of the Tauri state path.
fn ensure_scanned_using(
    engine: &Arc<Mutex<Engine>>,
    cancel: &AtomicBool,
    symbol: &str,
    start: impl FnOnce() -> Result<(), String>,
    poll_ms: u64,
    timeout_ms: u64,
) -> Result<(), String> {
    {
        let mut guard = lock(engine);
        if chain_is_ready(&mut guard, symbol) {
            return Ok(());
        }
    }
    start()?;
    wait_until_ready(engine, cancel, symbol, poll_ms, timeout_ms)
}

/// Ok immediately when the engine already holds a READY chain for `symbol`;
/// otherwise start the scan through the same code path as the `start_scan`
/// command (so the main screen follows along) and wait for it.
fn ensure_scanned(ctx: &ToolCtx, symbol: &str) -> Result<(), String> {
    let Some(app) = ctx.app.clone() else {
        return Err(no_app_error(symbol));
    };
    let state = app.state::<AppState>();
    ensure_scanned_using(
        &ctx.engine,
        &ctx.cancel,
        symbol,
        || crate::start_scan_for(&app, &state, symbol),
        POLL_MS,
        TIMEOUT_MS,
    )
}

// ---------- find_ideas ----------

fn run_find_ideas(ctx: &ToolCtx, args: &Value) -> Result<String, String> {
    let symbol = args
        .get("symbol")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_uppercase);
    let budget = args.get("budget_usd").and_then(Value::as_f64);
    match symbol {
        None => {
            // whole universe: the override replaces profile cash for this
            // call only — settings here is already a private snapshot clone
            let mut settings = ctx.settings.clone();
            if let Some(b) = budget {
                settings.easy_profile.cash = b;
            }
            let profile = settings.easy_profile.clone();
            if !profile.onboarded {
                return Ok(json!({
                    "as_of": Utc::now().to_rfc3339(),
                    "source": "settings",
                    "picks": [],
                    "note": "ผู้ใช้ยังไม่ได้ตั้งค่าโปรไฟล์โหมดง่าย (หุ้นที่สนใจ เงินที่พร้อมวาง \
                             ระดับความเสี่ยง) — ขอให้ผู้ใช้เปิดโหมดง่ายแล้วตอบคำถามเริ่มต้นก่อน \
                             จากนั้นเรียก find_ideas อีกครั้ง"
                })
                .to_string());
            }
            let input = match crate::easy_universe::load_rank_input(&settings, &ctx.app_data_dir) {
                Ok(input) => input,
                Err(e) => {
                    return Ok(json!({
                        "as_of": Utc::now().to_rfc3339(),
                        "source": "recorder",
                        "picks": [],
                        "note": format!("ยังไม่มีข้อมูลให้จัดอันดับทั้งจักรวาล: {e} — ถ้าเพิ่งเปิดแอป \
                                         ลองอีกครั้งหลังข้อมูลของวันถูกบันทึก")
                    })
                    .to_string());
                }
            };
            let ranked = rank_universe(&input, &profile, Utc::now());
            Ok(summarize_universe_ideas(&ranked).to_string())
        }
        Some(symbol) => {
            ensure_scanned(ctx, &symbol)?;
            let (rows, spot, source, atm_iv) = {
                let snap = lock(&ctx.engine).snapshot();
                let spot = snap.spot.unwrap_or(0.0);
                if spot <= 0.0 || !snap.rows.iter().any(|r| r.q.underlying == symbol) {
                    return Err(format!(
                        "ยังไม่มีข้อมูลของ {symbol} — รอสแกนเสร็จแล้วเรียก find_ideas อีกครั้ง"
                    ));
                }
                (
                    snap.rows,
                    spot,
                    snap.stock
                        .as_ref()
                        .map(|s| source_token(s.src.as_str()))
                        .unwrap_or("API")
                        .to_string(),
                    snap.stats.as_ref().and_then(|s| s.atm_iv),
                )
            };
            let mut profile = ctx.settings.easy_profile.clone();
            if let Some(b) = budget {
                profile.cash = b;
            }
            // realised vol degrades to "no data" exactly like easy_ideas —
            // a missing bars call never fails the whole tool
            let realized = ctx.settings.webull_client().and_then(|mut client| {
                let closes: Vec<f64> =
                    fetch_stock_bars(&mut client, std::slice::from_ref(&symbol), "D", 45)
                        .unwrap_or_default()
                        .into_iter()
                        .filter_map(|b| b.close)
                        .collect();
                realized_vol(&closes, 30)
            });
            let ideas = build_ideas(
                &symbol,
                spot,
                &source,
                &rows,
                &profile,
                atm_iv,
                realized,
                Utc::now(),
            );
            Ok(summarize_easy_ideas(&ideas).to_string())
        }
    }
}

// ---------- scan_chain ----------

fn run_scan_chain(ctx: &ToolCtx, args: &Value) -> Result<String, String> {
    let symbol = args
        .get("symbol")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_uppercase)
        .ok_or_else(|| "ต้องระบุ symbol ของหุ้นที่จะสแกน เช่น NVDA".to_string())?;
    let strategy = args
        .get("strategy")
        .and_then(Value::as_str)
        .unwrap_or("CSP");
    let risk = args
        .get("risk")
        .and_then(Value::as_str)
        .unwrap_or("BALANCED");
    let expiry = args
        .get("expiry")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let budget = args
        .get("budget_usd")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let count = args
        .get("count")
        .and_then(Value::as_i64)
        .unwrap_or(5)
        .clamp(1, 10);

    let sel_exp = expiry
        .map(|e| {
            NaiveDate::parse_from_str(e, "%Y-%m-%d").map_err(|_| {
                format!("expiry ต้องเป็นวันที่รูปแบบ YYYY-MM-DD เช่น 2026-12-18 — ได้รับ '{e}'")
            })
        })
        .transpose()?;

    // the same filter mapping the set_filters command applies — the agent
    // drives the main screen, so the app shows exactly these candidates
    let mode = match strategy {
        "CSP" => Mode::Csp,
        "CC" => Mode::Cc,
        _ => Mode::Both,
    };
    let (dmin, dmax) = match risk {
        "SAFE" => (0.10, 0.20),
        "BALANCED" => (0.15, 0.30),
        "AGGR" => (0.25, 0.40),
        _ => (0.0, 1.0), // ALL
    };
    let filters = Filters {
        mode,
        sel_exp,
        all_exp: sel_exp.is_none(),
        dmin,
        dmax,
        relax: false,
        budget,
    };

    ensure_scanned(ctx, &symbol)?;

    let (top, stats) = {
        let mut engine = lock(&ctx.engine);
        engine.set_filters(filters);
        // reuse the copilot formatters so agent rows and chat rows match
        let top = radar_core::copilot::execute_tool(
            "top_candidates",
            &json!({ "count": count }).to_string(),
            &mut engine,
        );
        let stats = radar_core::copilot::execute_tool("chain_stats", "{}", &mut engine);
        (top, stats)
    };
    let top: Value = serde_json::from_str(&top).unwrap_or(Value::Null);
    let stats: Value = serde_json::from_str(&stats).unwrap_or(Value::Null);

    let mut out = json!({
        "as_of": Utc::now().to_rfc3339(),
        "source": "engine",
    });
    if top.get("candidates").is_some() {
        out["candidates"] = top["candidates"].clone();
        if let Some(r) = top.get("returned") {
            out["returned"] = r.clone();
        }
    } else {
        out["candidates"] = json!([]);
        out["note"] = top
            .get("error")
            .cloned()
            .unwrap_or(json!("ยังไม่มีสัญญาที่ผ่านตัวคัดกรองด้วยเงื่อนไขที่ตั้งไว้"));
    }
    for src in [&top, &stats] {
        if let Some(s) = src.get("spot").filter(|s| !s.is_null()) {
            out["spot"] = s.clone();
            break;
        }
    }
    if let Some(st) = stats.get("stats") {
        let mut chain = serde_json::Map::new();
        for k in ["atm_iv", "em", "skew"] {
            if let Some(v) = st.get(k).filter(|v| !v.is_null()) {
                chain.insert(k.into(), v.clone());
            }
        }
        if !chain.is_empty() {
            out["chain"] = Value::Object(chain);
        }
    }
    Ok(out.to_string())
}

// ---------- what_if ----------

fn run_what_if(ctx: &ToolCtx, args: &Value) -> Result<String, String> {
    let ref_symbol = args
        .get("ref")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "ต้องระบุ ref เป็นรหัสสัญญา OCC เช่น NVDA261218P00175000 — ได้จาก find_ideas หรือ scan_chain"
                .to_string()
        })?;
    let price = args.get("price_at_expiry_usd").and_then(Value::as_f64);
    let covered = args
        .get("covered")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let basis = args.get("basis_usd").and_then(Value::as_f64);

    let (row, spot) = {
        let snap = lock(&ctx.engine).snapshot();
        (
            snap.rows.iter().find(|r| r.q.symbol == ref_symbol).cloned(),
            snap.spot.unwrap_or(0.0),
        )
    };
    let Some(row) = row else {
        return Err(format!(
            "ไม่พบสัญญา {ref_symbol} ในข้อมูลที่แอปถืออยู่ — เรียก find_ideas หรือ scan_chain \
             ของหุ้นตัวนั้นก่อน แล้วค่อยถาม what_if อีกครั้ง"
        ));
    };
    if spot <= 0.0 {
        return Err("ยังไม่มีราคาหุ้นปัจจุบันในระบบ — เรียก find_ideas หรือ scan_chain ก่อน".into());
    }
    let is_call = row.q.option_type == OptType::Call;
    let Some(premium) = premium_per_share(row.prem, row.credit) else {
        return Err(format!(
            "สัญญา {ref_symbol} ยังไม่มีราคาพรีเมียมที่ใช้คำนวณได้ — เลือกสัญญาอื่น \
             หรือเรียกใหม่หลังข้อมูลโหลดครบ"
        ));
    };
    let Some(iv) = row.iv else {
        return Err(format!(
            "สัญญา {ref_symbol} ยังไม่มีค่าความผันผวน (IV) ที่ใช้คำนวณได้ — เลือกสัญญาอื่น \
             หรือเรียกใหม่หลังข้อมูลโหลดครบ"
        ));
    };
    let lab = crate::lab_analyze(crate::LabAnalyzeInput {
        symbol: ref_symbol.to_string(),
        spot,
        strike: row.q.strike,
        premium,
        iv,
        t: row.t,
        is_call,
        covered,
        basis,
    })?;
    let summary = summarize_what_if(
        &lab,
        &WhatIfInput {
            ref_symbol: ref_symbol.to_string(),
            spot,
            strike: row.q.strike,
            premium,
            is_call,
            covered,
            basis,
            price_at_expiry: price,
            as_of: Utc::now().to_rfc3339(),
        },
    )?;
    Ok(summary.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use radar_core::engine::{DataSource, QuoteUpdate};

    // Tests deliberately build the engine/cancel pair directly instead of a
    // ToolCtx: the test exe must stay free of the Tauri app-handle path
    // (see ensure_scanned_using) so its imports load without the app
    // manifest the real binary gets.

    fn ready_engine() -> Engine {
        let mut e = Engine::new(Utc::now());
        let occ = "NVDA261218P00175000";
        e.set_symbols(vec![occ.to_string()], Some(191.25));
        e.push_quote(QuoteUpdate {
            symbol: occ.into(),
            price: 2.0,
            volume: 10.0,
            change: 0.0,
            change_ratio: 0.0,
            oi: 100.0,
            bid: 1.9,
            ask: 2.1,
            src: DataSource::Rest,
        });
        e
    }

    fn harness(engine: Engine) -> (Arc<Mutex<Engine>>, Arc<AtomicBool>) {
        (
            Arc::new(Mutex::new(engine)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    #[test]
    fn chain_is_ready_needs_spot_and_a_row_of_the_symbol() {
        let mut e = ready_engine();
        assert!(chain_is_ready(&mut e, "NVDA"));
        assert!(
            !chain_is_ready(&mut e, "AAPL"),
            "another symbol is not READY"
        );
        let mut empty = Engine::new(Utc::now());
        assert!(!chain_is_ready(&mut empty, "NVDA"));
    }

    #[test]
    fn wait_until_ready_returns_ok_when_the_chain_is_already_there() {
        let (engine, cancel) = harness(ready_engine());
        assert!(wait_until_ready(&engine, &cancel, "NVDA", 5, 50).is_ok());
    }

    #[test]
    fn wait_until_ready_times_out_with_a_thai_retry_hint() {
        let (engine, cancel) = harness(Engine::new(Utc::now()));
        let err = wait_until_ready(&engine, &cancel, "NVDA", 5, 30).unwrap_err();
        assert!(err.contains("NVDA") && err.contains("อีกครั้ง"), "{err}");
    }

    #[test]
    fn wait_until_ready_stops_as_soon_as_the_user_cancels() {
        let (engine, cancel) = harness(Engine::new(Utc::now()));
        cancel.store(true, Ordering::SeqCst);
        let err = wait_until_ready(&engine, &cancel, "NVDA", 5, 30_000).unwrap_err();
        assert_eq!(err, "หยุดตามที่ผู้ใช้สั่ง");
    }

    #[test]
    fn no_app_error_names_the_symbol_and_the_fix() {
        let err = no_app_error("NVDA");
        assert!(
            err.contains("NVDA") && err.contains("หน้าจอหลัก"),
            "error should name the symbol and the fix: {err}"
        );
    }

    #[test]
    fn ensure_scanned_using_skips_the_start_when_already_ready() {
        let (engine, cancel) = harness(ready_engine());
        assert!(
            ensure_scanned_using(
                &engine,
                &cancel,
                "NVDA",
                || panic!("start must not run when the chain is already READY"),
                5,
                30
            )
            .is_ok()
        );
    }

    #[test]
    fn ensure_scanned_using_propagates_a_failed_start() {
        let (engine, cancel) = harness(Engine::new(Utc::now()));
        let err = ensure_scanned_using(
            &engine,
            &cancel,
            "NVDA",
            || Err("ไม่พบ Webull API keys".into()),
            5,
            30,
        )
        .unwrap_err();
        assert_eq!(err, "ไม่พบ Webull API keys");
    }
}
