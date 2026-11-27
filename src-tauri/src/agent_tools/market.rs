//! Executors for the `market` tool family (W6 P2). Each tool fetches through
//! the same code paths and caches the UI commands use (`get_movers`,
//! `get_sectors`, `get_stock_overview`, `get_stock_history`), then hands the
//! domain structs to the pure summarisers in
//! `radar_core::agent::tools::market`. No engine lock is taken — nothing here
//! reads the engine.

use serde_json::{Value, json};

use radar_core::agent::tools::market as pure;
use radar_core::webull::fundamentals::{
    AnalystRating, AnalystTarget, CompanyProfile, DividendCalendar, DividendEvent, EarningsWindow,
    StockOverview, Unavailable, fetch_stock_overview,
};
use radar_core::webull::quotes::ny_today;
use radar_core::webull::rest::{opt_f64, opt_i64, opt_str};
use radar_core::webull::screeners::{MoverList, MoverRow, SectorRow, fetch_movers, fetch_sectors};
use radar_core::webull::stocks::{StockBar, fetch_stock_bars, fetch_stock_snapshots};
use radar_core::webull::transport::WebullClient;

use super::ToolCtx;
use crate::settings::AppSettings;

/// Every tool in this family needs the broker before it can answer.
const SETUP_HINT: &str = "ยังไม่ได้ตั้ง credentials — เปิดหน้า Setup ก่อน";

/// `None` = this family does not own `name`.
pub fn execute(ctx: &ToolCtx, name: &str, args: &Value) -> Option<Result<String, String>> {
    execute_with(&ctx.settings, &ctx.app_data_dir, name, args)
}

/// `execute` on plain inputs. Tests call this instead: building a
/// `ToolCtx` (it carries an `Option<tauri::AppHandle>`) in test code links
/// the Tauri app runtime into the test exe, which then fails to load on
/// Windows (0xC0000139).
fn execute_with(
    settings: &AppSettings,
    app_data_dir: &str,
    name: &str,
    args: &Value,
) -> Option<Result<String, String>> {
    match name {
        "market_pulse" => Some(market_pulse(settings, app_data_dir)),
        "stock_brief" => Some(stock_brief(settings, args)),
        "price_behavior" => Some(price_behavior(settings, args)),
        _ => None,
    }
}

// ---------- shared helpers ----------

/// Trim + uppercase, then check `^[A-Z][A-Z0-9.\-]{0,9}$` by hand (no regex
/// crate). Runs before the credentials check so a bad argument is reported
/// as such even when setup is missing.
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

fn to_json_string(summary: &Value) -> Result<String, String> {
    serde_json::to_string(summary).map_err(|e| format!("สร้างผลลัพธ์ไม่สำเร็จ: {e}"))
}

// ---------- market_pulse ----------

fn market_pulse(settings: &AppSettings, app_data_dir: &str) -> Result<String, String> {
    let Some(mut client) = settings.webull_client() else {
        return Err(SETUP_HINT.into());
    };
    let as_of = chrono::Utc::now().to_rfc3339();

    let store = crate::load_watchlists(app_data_dir);
    let (watchlist_name, symbols) = match store.active_list() {
        Some(list) => (
            Some(list.name.clone()),
            list.symbols.iter().take(20).cloned().collect::<Vec<_>>(),
        ),
        None => (None, Vec::new()),
    };

    let gainers = movers_or_error(&mut client, MoverList::Gainers);
    let losers = movers_or_error(&mut client, MoverList::Losers);
    let sectors = sectors_or_error(&mut client);
    // home_overview_on consumes the client, so the quotes stay last.
    let (quote_rows, quotes_error) = crate::home_overview_on(Some(client), &symbols);
    let quotes = match quotes_error.as_deref() {
        Some(reason) => Err(reason),
        None => Ok(quote_rows.as_slice()),
    };

    let summary = pure::summarize_market_pulse(
        &as_of,
        watchlist_name.as_deref(),
        quotes,
        rows_part(&gainers),
        rows_part(&losers),
        rows_part(&sectors),
    );
    to_json_string(&summary)
}

/// `Result<Vec<T>, String>` in the shape the pulse summariser takes.
fn rows_part<T>(part: &Result<Vec<T>, String>) -> Result<&[T], &str> {
    match part {
        Ok(rows) => Ok(rows.as_slice()),
        Err(reason) => Err(reason.as_str()),
    }
}

/// Gainers/losers through the UI's own movers cache: same key, same stored
/// shape as `get_movers` writes, so the agent and the screener page share one
/// entry. A cached value that no longer decodes is treated as a miss.
fn movers_or_error(client: &mut WebullClient, list: MoverList) -> Result<Vec<MoverRow>, String> {
    let key = format!("movers:{list:?}");
    if let Some(rows) = cached_movers(&key) {
        return Ok(rows);
    }
    let rows = fetch_movers(client, list).map_err(|e| e.user_message())?;
    crate::lock(&crate::MOVERS_CACHE).put(
        &key,
        json!({
            "list": list, "rows": &rows,
            "fetched_at": chrono::Utc::now().to_rfc3339(), "source": "REST",
        }),
    );
    Ok(rows)
}

/// Sector breadth through the same cache `get_sectors` uses.
fn sectors_or_error(client: &mut WebullClient) -> Result<Vec<SectorRow>, String> {
    if let Some(rows) = cached_sectors() {
        return Ok(rows);
    }
    let rows = fetch_sectors(client).map_err(|e| e.user_message())?;
    crate::lock(&crate::MOVERS_CACHE).put(
        "sectors",
        json!({
            "rows": &rows,
            "fetched_at": chrono::Utc::now().to_rfc3339(), "source": "REST",
        }),
    );
    Ok(rows)
}

fn cached_movers(key: &str) -> Option<Vec<MoverRow>> {
    let cached = crate::lock(&crate::MOVERS_CACHE).get(key)?;
    cached
        .get("rows")?
        .as_array()?
        .iter()
        .map(mover_from_value)
        .collect()
}

fn cached_sectors() -> Option<Vec<SectorRow>> {
    let cached = crate::lock(&crate::MOVERS_CACHE).get("sectors")?;
    cached
        .get("rows")?
        .as_array()?
        .iter()
        .map(sector_from_value)
        .collect()
}

// ---------- stock_brief ----------

fn stock_brief(settings: &AppSettings, args: &Value) -> Result<String, String> {
    let symbol = ticker_arg(args)?;
    let Some(mut client) = settings.webull_client() else {
        return Err(SETUP_HINT.into());
    };
    let as_of = chrono::Utc::now().to_rfc3339();

    // Extended hours on: a pre/after-hours move is part of the answer.
    let (quote, quote_error) =
        match fetch_stock_snapshots(&mut client, std::slice::from_ref(&symbol), true) {
            Ok(snapshots) => (
                snapshots
                    .into_iter()
                    .find(|snapshot| snapshot.price.unwrap_or(0.0) > 0.0),
                None,
            ),
            Err(error) => (None, Some(error.user_message())),
        };

    let overview = overview_with_cache(&mut client, &symbol);
    let summary = pure::summarize_stock_brief(
        &as_of,
        ny_today(),
        &symbol,
        quote.as_ref(),
        quote_error.as_deref(),
        Some(&overview),
        &[],
    );
    to_json_string(&summary)
}

/// Company facts through the UI's overview cache (key = symbol, stored exactly
/// as `get_stock_overview` writes it; only landed answers are cached, same
/// rule). A family the broker could not serve is reported by
/// `overview.unavailable`, so this never fails the tool.
fn overview_with_cache(client: &mut WebullClient, symbol: &str) -> StockOverview {
    if let Some(cached) = crate::lock(&crate::OVERVIEW_CACHE).get(symbol)
        && let Some(overview) = cached.get("overview").filter(|o| o.is_object())
    {
        return overview_from_value(overview);
    }
    let overview = fetch_stock_overview(client, symbol);
    let landed = overview.profile.is_some() || overview.next_earnings.is_some();
    if landed {
        crate::lock(&crate::OVERVIEW_CACHE).put(
            symbol,
            json!({
                "symbol": symbol, "overview": &overview,
                "fetched_at": chrono::Utc::now().to_rfc3339(),
            }),
        );
    }
    overview
}

// ---------- price_behavior ----------

fn price_behavior(settings: &AppSettings, args: &Value) -> Result<String, String> {
    let symbol = ticker_arg(args)?;
    let Some(mut client) = settings.webull_client() else {
        return Err(SETUP_HINT.into());
    };
    // The runner fills schema defaults, but stay safe if it ever changes.
    let lookback = args.get("lookback").and_then(Value::as_str).unwrap_or("3m");
    let Some(days) = pure::lookback_days(lookback) else {
        return Err(format!(
            "lookback '{lookback}' ไม่รองรับ — ใช้ 1m, 3m, 6m หรือ 1y"
        ));
    };

    // The chart page's daily cache holds 1200 bars, more than every lookback
    // asks for; fall back to a direct fetch when the entry is missing,
    // malformed or too short to summarise.
    let bars = match cached_daily_bars(&symbol) {
        Some(bars) if bars.len() > days => bars,
        _ => fetch_stock_bars(
            &mut client,
            std::slice::from_ref(&symbol),
            "D",
            pure::lookback_fetch_count(days),
        )
        .map_err(|e| format!("ดึงราคาย้อนหลังไม่สำเร็จ: {}", e.user_message()))?,
    };

    let as_of = chrono::Utc::now().to_rfc3339();
    let summary = pure::summarize_price_behavior(&as_of, &symbol, lookback, &bars, days)?;
    to_json_string(&summary)
}

fn cached_daily_bars(symbol: &str) -> Option<Vec<StockBar>> {
    let cached = crate::lock(&crate::BARS_DAILY_CACHE).get(&format!("{symbol}:D"))?;
    cached
        .get("bars")?
        .as_array()?
        .iter()
        .map(bar_from_value)
        .collect()
}

// ---------- cache decoders ----------
// radar-core's webull rows are Serialize-only (they only ever crossed the
// IPC bridge), so reading them back out of the shared JSON caches means
// decoding by hand. A row missing its required key fails the whole entry —
// the caller treats that as a cache miss and fetches.

fn mover_from_value(row: &Value) -> Option<MoverRow> {
    Some(MoverRow {
        symbol: row.get("symbol")?.as_str()?.to_string(),
        name: opt_str(row.get("name")),
        price: opt_f64(row.get("price")),
        change_ratio: opt_f64(row.get("change_ratio")),
        volume: opt_f64(row.get("volume")),
        market_value: opt_f64(row.get("market_value")),
        pe_ttm: opt_f64(row.get("pe_ttm")),
        relative_volume_10d: opt_f64(row.get("relative_volume_10d")),
        dividend_yield: opt_f64(row.get("dividend_yield")),
        exchange_code: opt_str(row.get("exchange_code")),
        last_trade_time: opt_str(row.get("last_trade_time")),
    })
}

fn sector_from_value(row: &Value) -> Option<SectorRow> {
    Some(SectorRow {
        id: row.get("id")?.as_str()?.to_string(),
        name: opt_str(row.get("name")),
        change_ratio: opt_f64(row.get("change_ratio")),
        advanced: opt_f64(row.get("advanced")),
        declined: opt_f64(row.get("declined")),
        flat: opt_f64(row.get("flat")),
        volume: opt_f64(row.get("volume")),
        market_value: opt_f64(row.get("market_value")),
        leader: row
            .get("leader")
            .filter(|leader| !leader.is_null())
            .and_then(mover_from_value),
    })
}

fn bar_from_value(row: &Value) -> Option<StockBar> {
    Some(StockBar {
        time: row.get("time")?.as_str()?.to_string(),
        open: opt_f64(row.get("open")),
        high: opt_f64(row.get("high")),
        low: opt_f64(row.get("low")),
        close: opt_f64(row.get("close")),
        volume: opt_f64(row.get("volume")),
    })
}

fn company_profile_from_value(v: &Value) -> CompanyProfile {
    CompanyProfile {
        company_name: opt_str(v.get("company_name")).unwrap_or_default(),
        industries: v
            .get("industries")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        profile: opt_str(v.get("profile")),
        exhibition_code: opt_str(v.get("exhibition_code")),
        ceo: opt_str(v.get("ceo")),
        employees: opt_f64(v.get("employees")),
        establish_date: opt_str(v.get("establish_date")),
        address: opt_str(v.get("address")),
    }
}

fn analyst_target_from_value(v: &Value) -> AnalystTarget {
    AnalystTarget {
        mean: opt_f64(v.get("mean")),
        low: opt_f64(v.get("low")),
        high: opt_f64(v.get("high")),
        median: opt_f64(v.get("median")),
        currency: opt_str(v.get("currency")),
    }
}

fn analyst_rating_from_value(v: &Value) -> AnalystRating {
    AnalystRating {
        number: opt_f64(v.get("number")),
        strong_buy: opt_f64(v.get("strong_buy")),
        buy: opt_f64(v.get("buy")),
        hold: opt_f64(v.get("hold")),
        sell: opt_f64(v.get("sell")),
        under_perform: opt_f64(v.get("under_perform")),
    }
}

fn earnings_window_from_value(v: &Value) -> EarningsWindow {
    EarningsWindow {
        start_date: opt_str(v.get("start_date")).unwrap_or_default(),
        end_date: opt_str(v.get("end_date")).unwrap_or_default(),
        fiscal_year: opt_i64(v.get("fiscal_year")),
        fiscal_period: opt_i64(v.get("fiscal_period")),
        eps_est: opt_f64(v.get("eps_est")),
        rev_est: opt_f64(v.get("rev_est")),
    }
}

fn dividend_event_from_value(v: &Value) -> DividendEvent {
    DividendEvent {
        amount: opt_f64(v.get("amount")),
        currency: opt_str(v.get("currency")),
        div_type: opt_str(v.get("div_type")),
        ex_div_date: opt_str(v.get("ex_div_date")),
        record_date: opt_str(v.get("record_date")),
        pay_date: opt_str(v.get("pay_date")),
    }
}

fn dividend_calendar_from_value(v: &Value) -> DividendCalendar {
    let events = |key: &str| -> Vec<DividendEvent> {
        v.get(key)
            .and_then(Value::as_array)
            .map(|rows| rows.iter().map(dividend_event_from_value).collect())
            .unwrap_or_default()
    };
    DividendCalendar {
        upcoming: events("upcoming"),
        history: events("history"),
    }
}

/// The family names `fetch_stock_overview` reports in `unavailable`. A cached
/// entry can only hold these five, which is what lets the `&'static str`
/// section survive the round trip; anything else is dropped.
const UNAVAILABLE_SECTIONS: [&str; 5] = [
    "company profile",
    "analyst target",
    "analyst rating",
    "earnings date",
    "dividend calendar",
];

fn static_section(name: &str) -> Option<&'static str> {
    UNAVAILABLE_SECTIONS.iter().copied().find(|s| *s == name)
}

fn overview_from_value(v: &Value) -> StockOverview {
    let family = |key: &str| v.get(key).filter(|f| f.is_object());
    StockOverview {
        profile: family("profile").map(company_profile_from_value),
        analyst_target: family("analyst_target").map(analyst_target_from_value),
        analyst_rating: family("analyst_rating").map(analyst_rating_from_value),
        next_earnings: family("next_earnings").map(earnings_window_from_value),
        dividends: family("dividends").map(dividend_calendar_from_value),
        unavailable: v
            .get("unavailable")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| {
                        let section = static_section(row.get("section")?.as_str()?)?;
                        Some(Unavailable {
                            section,
                            reason: opt_str(row.get("reason")).unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Default settings carry no credentials, so every fetch path short
    /// circuits — exactly what the offline tests below need. (No `ToolCtx`
    /// here — see `execute_with`.)
    fn run(name: &str, args: &Value) -> Option<Result<String, String>> {
        execute_with(&AppSettings::default(), "", name, args)
    }

    #[test]
    fn unknown_name_is_not_ours() {
        assert!(run("contract_detail", &json!({})).is_none());
    }

    #[test]
    fn ticker_arg_normalises_and_validates() {
        assert_eq!(ticker_arg(&json!({"symbol": " nvda "})), Ok("NVDA".into()));
        assert_eq!(ticker_arg(&json!({"symbol": "BRK.B"})), Ok("BRK.B".into()));
        assert_eq!(ticker_arg(&json!({"symbol": "A-B2"})), Ok("A-B2".into()));
        assert!(ticker_arg(&json!({})).is_err());
        assert!(ticker_arg(&json!({"symbol": ""})).is_err());
        assert!(ticker_arg(&json!({"symbol": "1NVDA"})).is_err()); // ตัวเลขนำหน้า
        assert!(ticker_arg(&json!({"symbol": "NVDA!"})).is_err()); // อักขระแปลก
        assert!(ticker_arg(&json!({"symbol": "TOOLONGTICKER"})).is_err()); // เกิน 10 ตัว
    }

    #[test]
    fn bad_ticker_is_rejected_before_any_fetch() {
        for name in ["stock_brief", "price_behavior"] {
            let err = run(name, &json!({"symbol": "1BAD!"}))
                .expect("family owns the name")
                .expect_err("invalid ticker");
            assert!(err.contains("ไม่ถูกต้อง"), "{name}: {err}");
        }
    }

    #[test]
    fn missing_credentials_short_circuits_every_tool() {
        for (name, args) in [
            ("market_pulse", json!({})),
            ("stock_brief", json!({"symbol": "NVDA"})),
            ("price_behavior", json!({"symbol": "NVDA"})),
        ] {
            let err = run(name, &args)
                .expect("family owns the name")
                .expect_err("no credentials in default settings");
            assert_eq!(err, SETUP_HINT, "{name}: {err}");
        }
    }

    #[test]
    fn movers_cache_entry_decodes_back_into_rows() {
        let rows = vec![
            MoverRow {
                symbol: "NVDA".into(),
                price: Some(136.59),
                change_ratio: Some(-0.1401),
                ..Default::default()
            },
            MoverRow {
                symbol: "TSLA".into(),
                ..Default::default()
            },
        ];
        // Same shape `get_movers` (and the executor) writes.
        crate::lock(&crate::MOVERS_CACHE).put(
            "movers:Gainers",
            json!({
                "list": "gainers", "rows": &rows,
                "fetched_at": "2026-11-27T09:00:00+00:00", "source": "REST",
            }),
        );
        assert_eq!(cached_movers("movers:Gainers"), Some(rows));

        // แถวที่ decode ไม่ได้ = cache miss (executor จะไป fetch แทน)
        crate::lock(&crate::MOVERS_CACHE).put("movers:Gainers", json!({"rows": [{"price": 1.0}]}));
        assert_eq!(cached_movers("movers:Gainers"), None);
    }

    #[test]
    fn sectors_cache_entry_decodes_back_into_rows() {
        let mut row = SectorRow {
            id: "9".into(),
            name: Some("Technology".into()),
            change_ratio: Some(0.004),
            advanced: Some(150.0),
            declined: Some(140.0),
            ..Default::default()
        };
        row.leader = Some(MoverRow {
            symbol: "AAPL".into(),
            price: Some(250.0),
            ..Default::default()
        });
        crate::lock(&crate::MOVERS_CACHE).put(
            "sectors",
            json!({"rows": [&row], "fetched_at": "t", "source": "REST"}),
        );
        assert_eq!(cached_sectors(), Some(vec![row]));
    }

    #[test]
    fn overview_round_trips_through_the_cache_shape() {
        let overview = StockOverview {
            profile: Some(CompanyProfile {
                company_name: "NVIDIA Corp".into(),
                industries: vec!["Semiconductors".into()],
                employees: Some(29600.0),
                ..Default::default()
            }),
            analyst_target: Some(AnalystTarget {
                mean: Some(244.18),
                ..Default::default()
            }),
            analyst_rating: Some(AnalystRating {
                number: Some(63.0),
                strong_buy: Some(34.0),
                ..Default::default()
            }),
            next_earnings: Some(EarningsWindow {
                start_date: "2026-12-18".into(),
                end_date: "2026-12-22".into(),
                fiscal_year: Some(2027),
                eps_est: Some(0.83),
                ..Default::default()
            }),
            dividends: Some(DividendCalendar {
                upcoming: vec![DividendEvent {
                    ex_div_date: Some("2026-12-03".into()),
                    amount: Some(0.01),
                    ..Default::default()
                }],
                history: Vec::new(),
            }),
            unavailable: vec![Unavailable {
                section: "dividend calendar",
                reason: "no data".into(),
            }],
        };
        let encoded = serde_json::to_value(&overview).expect("serializable");
        assert_eq!(overview_from_value(&encoded), overview);
    }

    #[test]
    fn daily_bars_cache_entry_decodes_back_into_bars() {
        let bars = vec![StockBar {
            time: "2026-11-26".into(),
            open: Some(135.0),
            high: Some(137.0),
            low: Some(134.0),
            close: Some(136.5),
            volume: Some(1.0e8),
        }];
        // Shape `get_stock_history` leaves under "<symbol>:D".
        crate::lock(&crate::BARS_DAILY_CACHE).put(
            "NVDA:D",
            json!({"bars": &bars, "ma5": [], "ma20": [], "timespan": "D", "note": "x"}),
        );
        assert_eq!(cached_daily_bars("NVDA"), Some(bars));
        assert_eq!(cached_daily_bars("MISSING"), None);
    }
}
