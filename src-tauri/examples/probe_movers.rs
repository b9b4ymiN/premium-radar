//! W13 audit probe — why "Most Active" shows no mega-caps.
//!
//! Read-only: GET screener routes only, ≤10 calls, ≥1 s apart, one client
//! per call with the cache off. Prints public market data only — params,
//! status, row count, the first 15 rows and where the mega-caps sit. Never
//! prints credentials, tokens, headers or full raw bodies.
//!
//! Run from `src-tauri`:
//!   cargo run --example probe_movers

use std::thread::sleep;
use std::time::Duration;

use radar_core::webull::rest::{FetchError, HttpFetch};
use radar_core::webull::sign::SignatureScheme;
use radar_core::webull::transport::WebullClient;
use serde_json::Value;

const PATH_TOP_ACTIVES: &str = "/market-data/screeners/top-actives/list";
const PATH_GAINERS_LOSERS: &str = "/market-data/screeners/gainers-losers/list";
const MEGA_CAPS: [&str; 7] = ["AAPL", "MSFT", "NVDA", "AMZN", "GOOGL", "META", "TSLA"];

/// (id, path, query) for one probe call.
type Probe<'a> = (&'a str, &'a str, Vec<(&'a str, &'a str)>);

/// The probes this audit needs, in call order.
fn plan() -> Vec<Probe<'static>> {
    vec![
        (
            "app-current",
            PATH_TOP_ACTIVES,
            vec![
                ("category", "US_STOCK"),
                ("rank_type", "VOLUME"),
                ("sort_by", "VOLUME"),
                ("direction", "DESC"),
            ],
        ),
        (
            "turnover-rank",
            PATH_TOP_ACTIVES,
            vec![
                ("category", "US_STOCK"),
                ("rank_type", "TURNOVER"),
                ("sort_by", "TURNOVER"),
                ("direction", "DESC"),
            ],
        ),
        (
            "turnover-rank-by-volume",
            PATH_TOP_ACTIVES,
            vec![
                ("category", "US_STOCK"),
                ("rank_type", "TURNOVER"),
                ("sort_by", "VOLUME"),
                ("direction", "DESC"),
            ],
        ),
        (
            "volume-rank-by-change",
            PATH_TOP_ACTIVES,
            vec![
                ("category", "US_STOCK"),
                ("rank_type", "VOLUME"),
                ("sort_by", "CHANGE_RATIO"),
                ("direction", "DESC"),
            ],
        ),
        (
            "gainers-app-current",
            PATH_GAINERS_LOSERS,
            vec![
                ("rank_type", "DAY_1"),
                ("category", "US_STOCK"),
                ("sort_by", "CHANGE_RATIO"),
                ("direction", "DESC"),
            ],
        ),
    ]
}

/// `symbol price volume (turnover | turnover_rate | market_value)` for one row.
fn row_line(row: &Value, rank: usize) -> String {
    let num = |key: &str| -> String {
        row.get(key)
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_else(|| "--".into())
    };
    let symbol = row.get("symbol").and_then(Value::as_str).unwrap_or("?");
    let extra = ["turnover", "turnover_rate", "market_value"]
        .iter()
        .find(|key| row.get(**key).is_some_and(|v| !v.is_null()))
        .map(|key| format!(" {}={}", key, num(key)))
        .unwrap_or_default();
    format!(
        "{rank:>3} {symbol:<6} price={} vol={}{extra}",
        num("price").chars().take(10).collect::<String>(),
        num("volume").chars().take(14).collect::<String>(),
    )
}

fn print_call(id: &str, path: &str, params: &[(&str, &str)], status: u16, body: &Value) {
    let query = params
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    println!("\n=== {id} — GET {path}?{query}");
    println!("status={status}");
    let rows = match body {
        Value::Array(items) => items.clone(),
        Value::Object(map) => map
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    println!("rows={}", rows.len());
    for (rank, row) in rows.iter().take(15).enumerate() {
        println!("{}", row_line(row, rank + 1));
    }
    let positions: Vec<String> = MEGA_CAPS
        .iter()
        .map(|cap| {
            let rank = rows
                .iter()
                .position(|row| row.get("symbol").and_then(Value::as_str) == Some(*cap));
            match rank {
                Some(index) => format!("{cap}=#{}", index + 1),
                None => format!("{cap}=absent"),
            }
        })
        .collect();
    println!("mega-caps: {}", positions.join(" "));
    let top5: Vec<String> = rows
        .iter()
        .take(5)
        .filter_map(|row| {
            row.get("symbol")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    println!("top5: {}", top5.join(","));
}

fn main() {
    let app_data = std::env::var("APPDATA")
        .map(|a| format!("{a}/premium-radar"))
        .unwrap_or_else(|_| ".".into());
    let mut settings = premium_radar_lib::settings::AppSettings::load(&app_data);
    settings.pull_secrets_from_keyring();
    let creds = settings
        .webull_credentials()
        .expect("no creds — run the app setup first");

    println!(
        "probe_movers — {} ET-ish local clock, host api.webull.co.th, scheme v3",
        chrono::Utc::now().to_rfc3339()
    );

    for (index, (id, path, params)) in plan().into_iter().enumerate() {
        if index > 0 {
            sleep(Duration::from_secs(1));
        }
        // fresh cache-less client per call — no earlier answer may serve
        let mut client = WebullClient::new(creds.clone())
            .with_signature_scheme(SignatureScheme::V3)
            .without_cache();
        let owned: Vec<(String, String)> = params
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        let result = client.get(path, &owned);
        match result {
            Ok((status, body)) => print_call(id, path, &params, status, &body),
            Err(FetchError::NonZero(status, body)) => {
                // keep the error text short — a provider message only, no body dump
                let note: String = serde_json::from_str::<Value>(&body)
                    .ok()
                    .and_then(|v| {
                        v.get("message")
                            .or_else(|| v.get("error_description"))
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .unwrap_or(body.chars().take(120).collect());
                println!("\n=== {id} — GET {path} ({} params)", params.len());
                println!("status={status} note={note}");
            }
            Err(other) => println!("\n=== {id} — transport error: {other}"),
        }
    }
    println!("\ndone — {} calls", plan().len());
}
