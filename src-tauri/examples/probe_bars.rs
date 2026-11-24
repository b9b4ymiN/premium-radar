//! Bars-per-timespan live audit for `get_stock_history` (contract W16a).
//!
//! One POST `/market-data/stocks/bars/list` per timespan for AAPL — the exact
//! production call (`fetch_stock_bars`, no `trading_sessions` filter) — asking
//! `count=1200` first and retrying once with 500 only when the host rejects
//! the larger count. At most 10 live calls, all read-only. Prints market data
//! only: HTTP status, bar count, first/last raw `time` and, for intraday
//! timespans, how many bars sit outside the 09:30–16:00 ET regular session.
//! A bar counts as extended only when BOTH the EST (UTC-5) and EDT (UTC-4)
//! readings of its timestamp fall outside the session, so a DST edge can never
//! fake extended hours. No credentials, headers or account data are printed.
//!
//! Run from `src-tauri`:
//!   cargo run --example probe_bars

use radar_core::webull::rest::FetchError;
use radar_core::webull::stocks::{StockBar, fetch_stock_bars};

const SYMBOL: &str = "AAPL";
const TIMESPANS: [&str; 8] = ["M5", "M15", "M30", "M60", "M240", "D", "W", "M"];
const CALL_BUDGET: u32 = 10;

fn main() {
    let app_data = std::env::var("APPDATA")
        .map(|a| format!("{a}/premium-radar"))
        .unwrap_or_else(|_| ".".into());
    let mut settings = premium_radar_lib::settings::AppSettings::load(&app_data);
    settings.pull_secrets_from_keyring();
    let mut client = settings
        .webull_client()
        .expect("no creds — run the app setup first")
        .without_cache();

    println!(
        "probe_bars · {SYMBOL} · POST /market-data/stocks/bars/list · real_time_required=false"
    );
    println!(
        "regular session = 09:30–16:00 ET; a bar is extended only outside BOTH the EST(-5) and EDT(-4) readings"
    );
    println!();
    println!(
        "{:<5} {:>4} {:>6}  {:<26} {:<26} {:>9}",
        "ts", "http", "bars", "first_time", "last_time", "ext"
    );
    let mut calls = 0u32;
    for ts in TIMESPANS {
        let mut asked = 1200u32;
        loop {
            calls += 1;
            match fetch_stock_bars(&mut client, &[SYMBOL.to_string()], ts, asked) {
                Ok(bars) => {
                    report(ts, asked, &bars);
                    break;
                }
                Err(e) => {
                    let (status, note) = match &e {
                        FetchError::NonZero(status, body) => (*status, err_note(body)),
                        other => (0, other.to_string()),
                    };
                    println!(
                        "{:<5} {:>4} {:>6}  {:<26} {:<26} {:>9}  asked={asked} err={note}",
                        ts, status, "-", "-", "-", "-"
                    );
                    // one retry, only when the larger count was rejected
                    if asked > 500 && calls < CALL_BUDGET {
                        println!("      ↳ count {asked} rejected — retrying once with 500");
                        asked = 500;
                        continue;
                    }
                    break;
                }
            }
        }
    }
    println!();
    println!("live calls: {calls}/{CALL_BUDGET}");
}

/// One audit row for a 200 answer.
fn report(ts: &str, asked: u32, bars: &[StockBar]) {
    let first = bars.first().map(|b| b.time.as_str()).unwrap_or("-");
    let last = bars.last().map(|b| b.time.as_str()).unwrap_or("-");
    let ext = if matches!(ts, "M5" | "M15" | "M30" | "M60" | "M240") {
        let mut outside = 0usize;
        let mut unparsed = 0usize;
        for bar in bars {
            match extended(&bar.time) {
                Some(true) => outside += 1,
                Some(false) => {}
                None => unparsed += 1,
            }
        }
        let mut cell = format!("{outside}/{}", bars.len());
        if unparsed > 0 {
            cell.push_str(&format!(" (unparsed={unparsed})"));
        }
        cell
    } else {
        "-".to_string()
    };
    println!(
        "{:<5} {:>4} {:>6}  {:<26} {:<26} {:>9}  asked={asked}",
        ts,
        200,
        bars.len(),
        first,
        last,
        ext
    );
}

/// Is this timestamp outside the regular session under every ET reading?
/// `None` when the API's time string does not parse — reported, never guessed.
fn extended(time: &str) -> Option<bool> {
    let utc = chrono::DateTime::parse_from_str(time, "%Y-%m-%dT%H:%M:%S%.f%z").ok()?;
    let secs = utc
        .with_timezone(&chrono::Utc)
        .timestamp()
        .rem_euclid(86_400);
    const OPEN: i64 = 9 * 3600 + 30 * 60;
    const CLOSE: i64 = 16 * 3600;
    let regular = |shift: i64| (OPEN..CLOSE).contains(&((secs - shift).rem_euclid(86_400)));
    Some(!(regular(5 * 3600) || regular(4 * 3600)))
}

/// Provider error_code + message, clipped — a trading message never reaches
/// here (this probe only calls market-data).
fn err_note(body: &str) -> String {
    let text = match serde_json::from_str::<serde_json::Value>(body) {
        Ok(v) => {
            let code = v.get("error_code").and_then(|x| x.as_str()).unwrap_or("");
            let msg = v.get("message").and_then(|x| x.as_str()).unwrap_or("");
            format!("{code} {msg}")
        }
        Err(_) => body.to_string(),
    };
    text.trim().chars().take(120).collect()
}
