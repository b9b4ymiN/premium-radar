//! Probe  endpoints ใหม่จาก llms.txt (ก่อนออกแบบ v2) — bars/depths/accounts/
//! positions/instrument · ทดสอบทั้ง path แบบ `/openapi/...` (แบบที่ใช้อยู่)
//! กับแบบ doc เปล่า · ใช้ transport เดิม (throttle+signing) · รายงาน sanitized

use std::time::Instant;

use radar_core::webull::rest::{
    Credentials, FetchError, HttpFetch, PATH_OPTION_BARS,
};
use radar_core::webull::transport::WebullClient;
use serde_json::{json, Value};

struct P {
    client: WebullClient,
    log: Vec<Value>,
}

impl P {
    fn try_get(&mut self, step: &str, path: &str, params: &[(String, String)]) -> (u16, Value) {
        let t = Instant::now();
        let ms = t.elapsed().as_millis();
        match self.client.get(path, params) {
            Ok((status, body)) => {
                println!("[{ms:>5}ms] {status} GET {step}");
                self.log.push(json!({"step": step, "method": "GET", "path": path, "status": status, "ms": ms}));
                (status, body)
            }
            Err(FetchError::NonZero(status, b)) => {
                let clip: String = b.chars().take(120).collect();
                println!("[{ms:>5}ms] {status} GET {step} — {clip}");
                self.log.push(json!({"step": step, "method": "GET", "path": path, "status": status, "ms": ms, "err": clip}));
                (status, Value::Null)
            }
            Err(e) => {
                println!("[{ms:>5}ms] NET GET {step} — {e}");
                self.log.push(json!({"step": step, "method": "GET", "path": path, "status": 0, "ms": ms}));
                (0, Value::Null)
            }
        }
    }

    fn try_post(&mut self, step: &str, path: &str, body: &Value) -> (u16, Value) {
        let t = Instant::now();
        let ms = t.elapsed().as_millis();
        match self.client.post_json(path, &body.to_string()) {
            Ok((status, val)) => {
                println!("[{ms:>5}ms] {status} POST {step}");
                self.log.push(json!({"step": step, "method": "POST", "path": path, "status": status, "ms": ms}));
                (status, val)
            }
            Err(FetchError::NonZero(status, b)) => {
                let clip: String = b.chars().take(120).collect();
                println!("[{ms:>5}ms] {status} POST {step} — {clip}");
                self.log.push(json!({"step": step, "method": "POST", "path": path, "status": status, "ms": ms, "err": clip}));
                (status, Value::Null)
            }
            Err(e) => {
                println!("[{ms:>5}ms] NET POST {step} — {e}");
                self.log.push(json!({"step": step, "method": "POST", "path": path, "status": 0, "ms": ms}));
                (0, Value::Null)
            }
        }
    }
}

fn bars_depth(body: &Value) -> Vec<(String, usize, String, String)> {
    let mut out = Vec::new();
    if let Some(arr) = body.get("result").and_then(Value::as_array) {
        for it in arr {
            let sym = it.get("symbol").and_then(Value::as_str).unwrap_or("?").to_string();
            let bars = it.get("result").and_then(Value::as_array).cloned().unwrap_or_default();
            let n = bars.len();
            let first = bars.first().and_then(|b| b.get("time")).map(|t| t.to_string()).unwrap_or_default();
            let last = bars.last().and_then(|b| b.get("time")).map(|t| t.to_string()).unwrap_or_default();
            out.push((sym, n, first, last));
        }
    }
    out
}

fn main() {
    let app_data = std::env::var("APPDATA")
        .map(|a| format!("{a}/premium-radar"))
        .unwrap_or_else(|_| ".".into());
    let mut settings = premium_radar_lib::settings::AppSettings::load(&app_data);
    settings.pull_secrets_from_keyring();
    let creds: Credentials = settings
        .webull_credentials()
        .expect("no creds — run app setup");
    println!("creds: {}", creds.masked());
    let mut p = P { client: WebullClient::new(creds), log: Vec::new() };
    let mut report = json!({"probed_at": chrono::Utc::now().to_string()});

    // 1) stock bars — POST ทั้ง 2 รูปแบบ path
    for path in [
        "/openapi/market-data/stocks/bars/list",
        "/market-data/stocks/bars/list",
    ] {
        let body = json!({
            "symbols": ["NVDA"], "category": "US_STOCK",
            "timespan": "D", "count": 1200, "real_time_required": false
        });
        let (st, v) = p.try_post(&format!("stock bars D ×1200 {path}"), path, &body);
        let d = bars_depth(&v);
        println!("    depth: {d:?}");
        report["stock_bars_daily"] = json!({"path": path, "status": st, "depth": d});
        if st == 200 { break; }
    }
    // minute bars
    for path in [
        "/openapi/market-data/stocks/bars/list",
        "/market-data/stocks/bars/list",
    ] {
        let body = json!({
            "symbols": ["NVDA"], "category": "US_STOCK",
            "timespan": "M5", "count": 100, "real_time_required": false
        });
        let (st, v) = p.try_post(&format!("stock bars M5 ×100 {path}"), path, &body);
        let d = bars_depth(&v);
        println!("    depth: {d:?}");
        report["stock_bars_m5"] = json!({"path": path, "status": st, "depth": d});
        if st == 200 { break; }
    }

    // 2) depths (order book)
    for path in [
        "/openapi/market-data/stocks/depths/list",
        "/market-data/stocks/depths/list",
    ] {
        let (st, v) = p.try_get(
            &format!("depths {path}"),
            path,
            &[
                ("symbol".into(), "NVDA".into()),
                ("category".into(), "US_STOCK".into()),
                ("depth".into(), "10".into()),
                ("overnight_required".into(), "false".into()),
            ],
        );
        let n_bids = v.get("bids").and_then(Value::as_array).map(|a| a.len());
        report["depths"] = json!({"path": path, "status": st, "bid_levels": n_bids});
        if st == 200 { break; }
    }

    // 3) accounts → positions + balance
    let mut account_id = None;
    for path in ["/openapi/trading/accounts/list", "/trading/accounts/list"] {
        let (st, v) = p.try_get(&format!("accounts {path}"), path, &[]);
        let list = radar_core::webull::rest::extract_list(&v);
        report["accounts"] = json!({"path": path, "status": st, "count": list.len()});
        if st == 200 && !list.is_empty() {
            account_id = list[0].get("account_id").and_then(Value::as_str).map(String::from);
            break;
        }
    }
    if let Some(aid) = account_id {
        for path in ["/openapi/trading/assets/positions/list", "/trading/assets/positions/list"] {
            let (st, v) = p.try_get(
                &format!("positions {path}"),
                path,
                &[("account_id".into(), aid.clone())],
            );
            let list = radar_core::webull::rest::extract_list(&v);
            let syms: Vec<String> = list
                .iter()
                .filter_map(|r| r.get("symbol").and_then(Value::as_str).map(String::from))
                .collect();
            println!("    positions: {}", syms.join(","));
            report["positions"] = json!({"path": path, "status": st, "count": list.len(), "symbols": syms});
            if st == 200 { break; }
        }
        for path in ["/openapi/trading/assets/balances/get", "/trading/assets/balances/get"] {
            let (st, v) = p.try_get(
                &format!("balance {path}"),
                path,
                &[("account_id".into(), aid.clone())],
            );
            report["balance"] = json!({"path": path, "status": st, "keys": v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default()});
            if st == 200 { break; }
        }
    }

    // 4) instrument profile
    for path in [
        "/openapi/trading/instruments/stocks/profiles/list",
        "/trading/instruments/stocks/profiles/list",
    ] {
        let (st, v) = p.try_get(
            &format!("instrument {path}"),
            path,
            &[
                ("category".into(), "US_STOCK".into()),
                ("symbols".into(), "NVDA,SPY".into()),
            ],
        );
        let list = radar_core::webull::rest::extract_list(&v);
        let names: Vec<String> = list
            .iter()
            .filter_map(|r| r.get("name").and_then(Value::as_str).map(String::from))
            .collect();
        println!("    names: {names:?}");
        report["instrument"] = json!({"path": path, "status": st, "count": list.len(), "names": names});
        if st == 200 { break; }
    }

    // 5) (อ้างอิง) option bars GET ยังเวิร์กไหม — ใช้ path เดิม
    let (st, v) = p.try_get(
        "option bars (existing path)",
        PATH_OPTION_BARS,
        &[
            ("symbols".into(), "NVDA261204P00175000".into()),
            ("category".into(), "US_OPTION".into()),
            ("timespan".into(), "D".into()),
            ("count".into(), "30".into()),
            ("real_time_required".into(), "false".into()),
        ],
    );
    report["option_bars_ref"] = json!({"status": st, "depth": bars_depth(&v)});

    report["log"] = json!(p.log);
    let out = serde_json::to_string_pretty(&report).expect("ser");
    let path = std::path::Path::new("../tmp/codex-consult/p10-probe-apis.json");
    std::fs::write(path, out).ok();
    println!("\nreport → {}", path.display());
}
