//! P8.2 — Webull historical-data feasibility probe (PLAN-LAB.md §1 matrix).
//!
//! Runs through the app's REAL transport (shared throttle 1.1 s, HMAC-SHA1
//! signing, TTL cache) with the app's REAL credentials (settings file + OS
//! keyring; env fallback) — never reads .env directly.
//!
//! Matrix: NVDA + SPY · active ATM/OTM put+call · expired 1/7/30/90/365 days
//! ago (status variants) · bars lookback count 1/7/30/90/365 · pagination.
//!
//! Output: sanitized JSON report on stdout — NO keys/tokens/credentials.
//! Roughly 40-70 calls → 1-2 minutes at the shared 1.1 s throttle.

use std::time::Instant;

use chrono::{Duration, NaiveDate};
use radar_core::webull::contracts::parse_occ;
use radar_core::webull::rest::{
    Credentials, FetchError, HttpFetch, PATH_OPTION_BARS, PATH_OPTION_CONTRACTS,
};
use radar_core::webull::transport::WebullClient;
use serde_json::{Value, json};

// ---------- report helpers ----------

#[derive(serde::Serialize)]
struct CallLog {
    step: String,
    status: u16,
    latency_ms: u128,
    rows: usize,
    note: String,
}

struct Prober {
    client: WebullClient,
    log: Vec<CallLog>,
}

impl Prober {
    fn call(&mut self, step: &str, path: &str, params: &[(String, String)]) -> (u16, Value) {
        let t = Instant::now();
        let res = self.client.get(path, params);
        let ms = t.elapsed().as_millis();
        match res {
            Ok((status, body)) => {
                let rows = radar_core::webull::rest::extract_list(&body).len();
                println!("[{ms:>5}ms] {status} {step} — {rows} rows");
                self.log.push(CallLog {
                    step: step.to_string(),
                    status,
                    latency_ms: ms,
                    rows,
                    note: String::new(),
                });
                (status, body)
            }
            Err(FetchError::NonZero(status, body)) => {
                let clipped: String = body.chars().take(200).collect();
                println!("[{ms:>5}ms] {status} {step} — error: {clipped}");
                self.log.push(CallLog {
                    step: step.to_string(),
                    status,
                    latency_ms: ms,
                    rows: 0,
                    note: clipped,
                });
                (status, Value::Null)
            }
            Err(e) => {
                println!("[{ms:>5}ms] NET {step} — {e}");
                self.log.push(CallLog {
                    step: step.to_string(),
                    status: 0,
                    latency_ms: ms,
                    rows: 0,
                    note: format!("network: {e}"),
                });
                (0, Value::Null)
            }
        }
    }
}

// ---------- bars analysis ----------

/// earliest/latest bar time + closes from a bars payload (`{result:[{symbol,
/// result:[{time, close, ...}]}]}`); times compared as strings like the app.
fn bars_summary(body: &Value) -> Vec<(String, usize, String, String, f64)> {
    let items = match body {
        Value::Object(_) => body.get("result").cloned().unwrap_or(Value::Null),
        v => v.clone(),
    };
    let Value::Array(items) = items else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|it| {
            let sym = it.get("symbol")?.as_str()?.replace(' ', "");
            let bars = it.get("result")?.as_array()?;
            let mut times: Vec<(String, f64)> = bars
                .iter()
                .map(|b| {
                    (
                        b.get("time").map(|t| t.to_string()).unwrap_or_default(),
                        radar_core::webull::rest::to_f64(
                            b.get("close").unwrap_or(&Value::Null),
                            0.0,
                        ),
                    )
                })
                .collect();
            times.sort_by(|a, b| a.0.cmp(&b.0));
            let (first_t, _) = times.first()?.clone();
            let (last_t, last_c) = times.last()?.clone();
            Some((sym, times.len(), first_t, last_t, last_c))
        })
        .collect()
}

fn bars_params(batch: &[String], count: u32) -> Vec<(String, String)> {
    vec![
        ("symbols".into(), batch.join(",")),
        ("category".into(), "US_OPTION".into()),
        ("timespan".into(), "D".into()),
        ("count".into(), count.to_string()),
        ("real_time_required".into(), "false".into()),
    ]
}

/// Pick active contracts near spot via the Lab quality gate
/// (`lab::quality::select_near_spot`) — spot ต้อง valid เสมอ
/// (P8.2 เดา field ผิดได้ spot=0 แล้วตก strike ต่ำสุด — ตอนนี้เป็น error)
fn pick_contracts(rows: &[Value], spot: f64) -> Result<Vec<(String, String)>, String> {
    let mut contracts = Vec::new();
    for r in rows {
        let Some(sym) = r.get("symbol").and_then(Value::as_str) else {
            continue;
        };
        let Some(m) = parse_occ(sym) else { continue };
        contracts.push(radar_core::lab::types::ContractRef {
            occ: sym.replace(' ', ""),
            underlying: m.underlying,
            expiry: m.expiry,
            option_type: m.option_type,
            strike: m.strike,
            multiplier: 100.0,
        });
    }
    let picks = radar_core::lab::quality::select_near_spot(&contracts, spot, 0.85)
        .map_err(|e| format!("{}: {}", e.field, e.reason_th))?;
    Ok(vec![
        (
            format!("ATM put K={}", picks.atm_put.strike),
            picks.atm_put.occ,
        ),
        (
            format!("ATM call K={}", picks.atm_call.strike),
            picks.atm_call.occ,
        ),
        (
            format!("OTM put K={}", picks.otm_put.strike),
            picks.otm_put.occ,
        ),
        (
            format!("OTM call K={}", picks.otm_call.strike),
            picks.otm_call.occ,
        ),
    ])
}

fn contracts_params(
    symbol: &str,
    a: NaiveDate,
    b: NaiveDate,
    status: Option<&str>,
    key: Option<&str>,
) -> Vec<(String, String)> {
    let mut p: Vec<(String, String)> = vec![
        ("underlying_symbols".into(), symbol.into()),
        ("category".into(), "US_OPTION".into()),
        ("start_date".into(), a.to_string()),
        ("end_date".into(), b.to_string()),
    ];
    if let Some(s) = status {
        p.push(("status".into(), s.into()));
    }
    if let Some(k) = key {
        p.push(("pagination_key".into(), k.into()));
    }
    p
}

fn main() {
    let today = chrono::Local::now().date_naive();
    // `smoke` arg = targeted P8.3 check (valid-spot ATM/OTM picks only)
    let smoke = std::env::args().any(|a| a == "smoke");
    println!(
        "== {} {today} (NY-market dates computed locally) ==",
        if smoke { "P8.3 smoke" } else { "P8.2 probe" }
    );

    // --- credentials: app settings + keyring (same as the running app) ---
    let app_data_dir = std::env::var("APPDATA")
        .map(|a| format!("{a}/premium-radar"))
        .unwrap_or_else(|_| ".".into());
    let mut settings = premium_radar_lib::settings::AppSettings::load(&app_data_dir);
    settings.pull_secrets_from_keyring();
    let creds: Credentials = settings
        .webull_credentials()
        .expect("no webull credentials in settings/keyring/env — run the app setup first");
    println!(
        "creds: {} · access_token present: {}",
        creds.masked(),
        creds.access_token.is_some()
    );

    let mut prober = Prober {
        client: settings
            .webull_client()
            .expect("no webull credentials in settings/keyring/env — run the app setup first"),
        log: Vec::new(),
    };

    let mut report = json!({
        "probe_date": today.to_string(),
        "underlyings": {},
    });

    for symbol in ["NVDA", "SPY"] {
        println!("\n===== {symbol} =====");
        let mut sym_report = json!({});

        // 1) spot via stock snapshot (sanity + token check in one call)
        let (st, body) = prober.call(
            &format!("{symbol} stock snapshot"),
            "/openapi/market-data/stock/snapshot",
            &[
                ("symbols".into(), symbol.into()),
                ("category".into(), "US_STOCK".into()),
                ("extend_hour_required".into(), "false".into()),
                ("overnight_required".into(), "false".into()),
            ],
        );
        // spot ใช้ field `price` จริงของ schema (P8.2 เดา last_price/close ผิด)
        // — invalid spot ต้องหยุด ไม่ fallback (amendments §3)
        let spot = radar_core::webull::rest::extract_list(&body)
            .first()
            .map(|r| radar_core::webull::rest::to_f64(r.get("price").unwrap_or(&Value::Null), 0.0))
            .unwrap_or(0.0);
        sym_report["stock_snapshot_status"] = json!(st);
        sym_report["spot"] = json!(spot);
        if st != 200 {
            println!("  !! stock snapshot failed — token/entitlement issue; skipping {symbol}");
            report["underlyings"][symbol] = sym_report;
            continue;
        }
        if spot <= 0.0 {
            println!("  !! spot ไม่ valid ({spot}) — ปฏิเสธการเลือกสัญญาของ {symbol} ตามกติกา P8.3");
            sym_report["selection_rejected"] =
                json!(format!("spot={spot} ไม่ valid — ห้ามเลือกด้วย moneyness"));
            report["underlyings"][symbol] = sym_report;
            continue;
        }

        // 2) active contracts 8-14 DTE (LISTING) → ATM/OTM picks
        let (_, cbody) = prober.call(
            &format!("{symbol} contracts LISTING 8-14d"),
            PATH_OPTION_CONTRACTS,
            &contracts_params(
                symbol,
                today + Duration::days(8),
                today + Duration::days(14),
                Some("LISTING"),
                None,
            ),
        );
        let rows = radar_core::webull::rest::extract_list(&cbody);
        sym_report["active_contracts_rows"] = json!(rows.len());
        let picks = match pick_contracts(&rows, spot) {
            Ok(p) => p,
            Err(reason) => {
                println!("  !! เลือกสัญญาไม่ได้: {reason}");
                sym_report["selection_rejected"] = json!(reason);
                report["underlyings"][symbol] = sym_report;
                continue;
            }
        };
        sym_report["active_picks"] = json!(
            picks
                .iter()
                .map(|(l, s)| json!({"label": l, "occ": s}))
                .collect::<Vec<_>>()
        );

        // 3) bars lookback ladder on the ATM call, count sweep
        let occs: Vec<String> = picks.iter().map(|(_, s)| s.clone()).collect();
        if let Some((_, atm_call)) = picks.iter().find(|(l, _)| l.starts_with("ATM call")) {
            let mut ladder = Vec::new();
            for count in [1u32, 7, 30, 90, 365] {
                let (st, b) = prober.call(
                    &format!("{symbol} ATM call bars count={count}"),
                    PATH_OPTION_BARS,
                    &bars_params(std::slice::from_ref(atm_call), count),
                );
                let sum = bars_summary(&b);
                ladder.push(json!({
                    "count": count, "status": st,
                    "bars": sum.iter().map(|(s, n, f, l, c)| json!({
                        "occ": s, "n": n, "earliest": f, "latest": l, "last_close": c
                    })).collect::<Vec<_>>(),
                }));
            }
            sym_report["active_bars_ladder"] = json!(ladder);
        }

        // 4) other picks at count=365
        let others: Vec<String> = picks
            .iter()
            .filter(|(l, _)| !l.starts_with("ATM call"))
            .map(|(_, s)| s.clone())
            .collect();
        if !others.is_empty() {
            let (st, b) = prober.call(
                &format!("{symbol} other picks bars count=365"),
                PATH_OPTION_BARS,
                &bars_params(&others, 365),
            );
            sym_report["other_picks_bars"] = json!(bars_summary(&b)
                .into_iter()
                .map(|(s, n, f, l, c)| json!({"occ": s, "n": n, "earliest": f, "latest": l, "last_close": c}))
                .collect::<Vec<_>>());
            sym_report["other_picks_status"] = json!(st);
        }

        // smoke mode: จบหลัง ATM/OTM targeted check (expired/pagination
        // P8.2 พิสูจน์ไปแล้ว — ไม่เปลือง quota ซ้ำ)
        if smoke {
            report["underlyings"][symbol] = sym_report;
            continue;
        }

        // 5) expired-contract discovery: find a status variant that returns
        //    rows for an expiry window in the past (test on ~30d ago)
        let past30 = today - Duration::days(30);
        let mut winner: Option<String> = None;
        for variant in ["EXPIRED", "", "DELISTED", "UNLISTED"] {
            let (st, b) = prober.call(
                &format!("{symbol} contracts ~30d-ago status={variant:?}"),
                PATH_OPTION_CONTRACTS,
                &contracts_params(
                    symbol,
                    past30 - Duration::days(2),
                    past30 + Duration::days(2),
                    (!variant.is_empty()).then_some(variant),
                    None,
                ),
            );
            let n = radar_core::webull::rest::extract_list(&b).len();
            if st == 200 && n > 0 {
                winner = Some(variant.to_string());
                sym_report["expired_status_variant"] = json!(variant);
                sym_report["expired_rows_30d"] = json!(n);
                break;
            }
        }
        if winner.is_none() {
            sym_report["expired_status_variant"] = Value::Null;
            sym_report["expired_note"] = json!("no status variant returned expired rows");
        } else {
            // sweep expiry windows 1/7/30/90/365 days ago; ATM put+call bars 365
            let mut windows = Vec::new();
            for back in [1i64, 7, 30, 90, 365] {
                let d = today - Duration::days(back);
                let (_, b) = prober.call(
                    &format!("{symbol} contracts -{back}d"),
                    PATH_OPTION_CONTRACTS,
                    &contracts_params(
                        symbol,
                        d - Duration::days(2),
                        d + Duration::days(2),
                        winner.as_deref(),
                        None,
                    ),
                );
                let rows = radar_core::webull::rest::extract_list(&b);
                let picks = match pick_contracts(&rows, spot) {
                    Ok(p) => p,
                    Err(reason) => {
                        windows.push(json!({
                            "days_ago": back, "rows": rows.len(),
                            "selection_rejected": reason,
                        }));
                        continue;
                    }
                };
                let occs: Vec<String> = picks.iter().take(2).map(|(_, s)| s.clone()).collect();
                let mut entry = json!({
                    "days_ago": back, "rows": rows.len(),
                    "picks": picks.iter().take(2).map(|(l, s)| json!({"label": l, "occ": s})).collect::<Vec<_>>(),
                });
                if !occs.is_empty() {
                    let (st, bb) = prober.call(
                        &format!("{symbol} expired -{back}d bars 365"),
                        PATH_OPTION_BARS,
                        &bars_params(&occs, 365),
                    );
                    entry["bars_status"] = json!(st);
                    entry["bars"] = json!(bars_summary(&bb)
                        .into_iter()
                        .map(|(s, n, f, l, c)| json!({"occ": s, "n": n, "earliest": f, "latest": l, "last_close": c}))
                        .collect::<Vec<_>>());
                }
                windows.push(entry);
            }
            sym_report["expired_windows"] = json!(windows);
        }

        // 6) pagination on contracts (LISTING 0-365, follow one pagination_key)
        let (_st, b) = prober.call(
            &format!("{symbol} contracts page1"),
            PATH_OPTION_CONTRACTS,
            &contracts_params(
                symbol,
                today,
                today + Duration::days(365),
                Some("LISTING"),
                None,
            ),
        );
        let page1 = radar_core::webull::rest::extract_list(&b).len();
        let key = radar_core::webull::rest::pagination_key(&b);
        let mut pag = json!({"page1_rows": page1, "pagination_key_present": key.is_some()});
        if let Some(k) = key {
            let (st2, b2) = prober.call(
                &format!("{symbol} contracts page2"),
                PATH_OPTION_CONTRACTS,
                &contracts_params(
                    symbol,
                    today,
                    today + Duration::days(365),
                    Some("LISTING"),
                    Some(&k),
                ),
            );
            pag["page2_rows"] = json!(radar_core::webull::rest::extract_list(&b2).len());
            pag["page2_status"] = json!(st2);
        }
        sym_report["contracts_pagination"] = pag;

        report["underlyings"][symbol] = sym_report;
        let _ = &occs; // silence unused in builds without picks
    }

    report["call_log"] = json!(prober.log);
    let out = serde_json::to_string_pretty(&report).expect("serialize report");
    let file = if smoke {
        "p8.3-smoke-report.json"
    } else {
        "p8.2-probe-report.json"
    };
    let path = std::path::Path::new("../tmp/codex-consult").join(file);
    match std::fs::write(&path, &out) {
        Ok(_) => println!("\nreport → {}", path.display()),
        Err(e) => {
            // fallback next to the binary (target dir) if tmp/ moved
            std::fs::write(file, &out).ok();
            println!("\nreport → {file} ({e})");
        }
    }
}
