//! Stock market-data endpoints (verified by probe 2026-11-23 — PLAN-APP2 §1)
//!
//! ระวัง: endpoints กลุ่มนี้ path **ไม่มี** prefix `/openapi` ต่างจากของเดิม
//! (probe: `/openapi/market-data/stocks/bars/list` → 404, แบบไม่มี prefix → 200)

use serde_json::{Value, json};

use super::rest::{FetchError, HttpFetch, extract_list, opt_f64, opt_i64, opt_str, to_f64};

/// POST `/market-data/stocks/bars/list` — batch OHLCV (probe: D×1200 ≈ 4.8 ปี)
pub const PATH_STOCK_BARS: &str = "/market-data/stocks/bars/list";
/// GET `/trading/instruments/stocks/profiles/list`
pub const PATH_INSTRUMENT_PROFILES: &str = "/trading/instruments/stocks/profiles/list";

/// Timespan enum ตาม docs (S5..Y)
pub fn timespan_valid(ts: &str) -> bool {
    matches!(
        ts,
        "S5" | "S15"
            | "M1"
            | "M5"
            | "M15"
            | "M30"
            | "M60"
            | "M120"
            | "M240"
            | "D"
            | "W"
            | "M"
            | "Y"
    )
}

/// แท่งเดียว normalize แล้ว (ตัวเลขจาก API เป็น string — to_f64 ปลอดภัย;
/// ค่าที่ไม่มี/เพี้ยนเป็น None ไม่ใช่ 0)
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct StockBar {
    /// ISO+0000 ตาม raw (ET-midnight anchor สำหรับแท่ง D)
    pub time: String,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
    pub volume: Option<f64>,
}

/// ดึง bars หลายหุ้น (สูงสุด 20 symbols/ครั้ง ตาม docs) — response จาก API
/// เรียงใหม่→เก่า, ที่นี่เรียงเก่า→ใหม่ + dedupe ตาม time
pub fn fetch_stock_bars<F: HttpFetch>(
    f: &mut F,
    symbols: &[String],
    timespan: &str,
    count: u32,
) -> Result<Vec<StockBar>, FetchError> {
    debug_assert!(symbols.len() <= 20, "docs: max 20 symbols/batch");
    let body = json!({
        "symbols": symbols,
        "category": "US_STOCK",
        "timespan": timespan,
        "count": count,
        "real_time_required": false,
    });
    let (status, data) = f.post_json(PATH_STOCK_BARS, &body.to_string())?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    Ok(parse_stock_bars(&data))
}

/// Pure parser — แถวแรกของ result เท่านั้น (single-symbol ใช้ shared กับ batch)
pub fn parse_stock_bars(data: &Value) -> Vec<StockBar> {
    let Some(arr) = data.get("result").and_then(Value::as_array) else {
        return Vec::new();
    };
    let Some(first) = arr.first() else {
        return Vec::new();
    };
    let mut bars: Vec<StockBar> = first
        .get("result")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|b| {
                    let time = b.get("time")?.to_string().trim_matches('"').to_string();
                    if time.is_empty() {
                        return None;
                    }
                    Some(StockBar {
                        time,
                        open: opt_num(b.get("open")),
                        high: opt_num(b.get("high")),
                        low: opt_num(b.get("low")),
                        close: opt_num(b.get("close")),
                        volume: opt_num(b.get("volume")),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // API ส่งใหม่→เก่า: เรียงเก่า→ใหม่ (เทียบ string ISO ได้เพราะ format เดียวกัน)
    bars.sort_by(|a, b| a.time.cmp(&b.time));
    bars.dedup_by(|a, b| a.time == b.time);
    bars
}

fn opt_num(v: Option<&Value>) -> Option<f64> {
    let x = to_f64(v.unwrap_or(&Value::Null), f64::NAN);
    x.is_finite().then_some(x)
}

/// โปรไฟล์หลักทรัพย์ — เฉพาะ fields ที่ API ให้จริง (ไม่มี fundamentals)
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct InstrumentProfile {
    pub symbol: String,
    pub name: String,
    pub instrument_id: String,
    pub exchange_code: String,
    pub currency: String,
    pub sub_category: String,
    pub status: String,
    pub shortable: bool,
    pub fractionable: bool,
    pub marginable: bool,
    pub overnight_trading_supported: bool,
    pub lot_size: Option<f64>,
}

pub fn fetch_instrument_profiles<F: HttpFetch>(
    f: &mut F,
    symbols: &[String],
) -> Result<Vec<InstrumentProfile>, FetchError> {
    let (status, data) = f.get(
        PATH_INSTRUMENT_PROFILES,
        &[
            ("category".into(), "US_STOCK".into()),
            ("symbols".into(), symbols.join(",")),
        ],
    )?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    Ok(extract_list(&data)
        .iter()
        .map(|r| InstrumentProfile {
            symbol: r.get("symbol").and_then(Value::as_str).unwrap_or("").into(),
            name: r.get("name").and_then(Value::as_str).unwrap_or("").into(),
            instrument_id: r
                .get("instrument_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            exchange_code: r
                .get("exchange_code")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            currency: r
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            sub_category: r
                .get("sub_category")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            status: r.get("status").and_then(Value::as_str).unwrap_or("").into(),
            shortable: r.get("shortable").and_then(Value::as_bool).unwrap_or(false),
            fractionable: r
                .get("fractionable")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            marginable: r
                .get("marginable")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            overnight_trading_supported: r
                .get("overnight_trading_supported")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            lot_size: opt_num(r.get("lot_size")),
        })
        .collect())
}

// ---------- batch snapshots (reference "snapshot", live-verified 2026-11-23) ----------

/// GET `/market-data/stocks/snapshots/list` — the documented batch endpoint.
/// The single-symbol legacy path in `quotes.rs` stays what the scanner polls.
pub const PATH_STOCK_SNAPSHOTS: &str = "/market-data/stocks/snapshots/list";

/// Symbols per snapshot request (docs: maximum 100).
pub const SNAPSHOT_BATCH: usize = 100;

/// Pre-market / after-hours block of a stock quote.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct ExtendedQuote {
    pub last_price: Option<f64>,
    /// Change against the regular-session close.
    pub change: Option<f64>,
    pub change_ratio: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub volume: Option<f64>,
    pub last_trade_time_ms: Option<i64>,
}

/// One stock as the broker quotes it. With extended hours off (what the quote
/// lists use) `price`/`volume`/`change` describe the regular session; with
/// them on those fields follow the extended session and the regular close
/// sits in `close`. Either way a missing number stays `None`.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct StockSnapshot {
    pub symbol: String,
    pub price: Option<f64>,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub pre_close: Option<f64>,
    pub close: Option<f64>,
    pub volume: Option<f64>,
    pub change: Option<f64>,
    pub change_ratio: Option<f64>,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub bid_size: Option<f64>,
    pub ask_size: Option<f64>,
    /// Broker session tag of the quote (observed "PRE" during pre-market).
    pub trade_status: Option<String>,
    /// When the broker produced the quote (epoch ms).
    pub quote_time_ms: Option<i64>,
    /// When the stock last traded (epoch ms).
    pub last_trade_time_ms: Option<i64>,
    pub market_value: Option<f64>,
    pub pe_ratio: Option<f64>,
    pub eps: Option<f64>,
    pub eps_ttm: Option<f64>,
    pub pb_ratio: Option<f64>,
    /// Dividend yield as a fraction (0.007966 = 0.80 %).
    pub dividend_yield: Option<f64>,
    pub fifty_two_wk_high: Option<f64>,
    pub fifty_two_wk_low: Option<f64>,
    /// `None` until an extended-hours trade exists for the symbol.
    pub extended: Option<ExtendedQuote>,
}

/// Parse one snapshot row; rows without a symbol are skipped.
pub fn parse_stock_snapshot(row: &Value) -> Option<StockSnapshot> {
    let symbol = opt_str(row.get("symbol"))?;
    let extended = opt_f64(row.get("extend_hour_last_price"))
        .filter(|price| *price > 0.0)
        .map(|last_price| ExtendedQuote {
            last_price: Some(last_price),
            change: opt_f64(row.get("extend_hour_change")),
            change_ratio: opt_f64(row.get("extend_hour_change_ratio")),
            high: opt_f64(row.get("extend_hour_high")),
            low: opt_f64(row.get("extend_hour_low")),
            volume: opt_f64(row.get("extend_hour_volume")),
            last_trade_time_ms: opt_i64(row.get("extend_hour_last_trade_time")),
        });
    Some(StockSnapshot {
        symbol,
        price: opt_f64(row.get("price")),
        open: opt_f64(row.get("open")),
        high: opt_f64(row.get("high")),
        low: opt_f64(row.get("low")),
        pre_close: opt_f64(row.get("pre_close")),
        close: opt_f64(row.get("close")),
        volume: opt_f64(row.get("volume")),
        change: opt_f64(row.get("change")),
        change_ratio: opt_f64(row.get("change_ratio")),
        bid: opt_f64(row.get("bid")),
        ask: opt_f64(row.get("ask")),
        bid_size: opt_f64(row.get("bid_size")),
        ask_size: opt_f64(row.get("ask_size")),
        trade_status: opt_str(row.get("trade_status")),
        quote_time_ms: opt_i64(row.get("quote_time")),
        last_trade_time_ms: opt_i64(row.get("last_trade_time")),
        market_value: opt_f64(row.get("market_value")),
        pe_ratio: opt_f64(row.get("pe_ratio")),
        eps: opt_f64(row.get("eps")),
        eps_ttm: opt_f64(row.get("eps_ttm")),
        pb_ratio: opt_f64(row.get("pb_ratio")),
        dividend_yield: opt_f64(row.get("yield")),
        fifty_two_wk_high: opt_f64(row.get("fifty_two_wk_high")),
        fifty_two_wk_low: opt_f64(row.get("fifty_two_wk_low")),
        extended,
    })
}

/// Parse a snapshots response (bare array or `{data: [...]}` envelope).
pub fn parse_stock_snapshots(data: &Value) -> Vec<StockSnapshot> {
    extract_list(data)
        .iter()
        .filter_map(parse_stock_snapshot)
        .collect()
}

/// Quotes for many symbols in ONE request — stocks and ETFs alike answer
/// under `US_STOCK`. `extended_hours` on adds the pre/after-hours block and
/// makes `price` follow that session (see [`StockSnapshot`]).
pub fn fetch_stock_snapshots<F: HttpFetch>(
    f: &mut F,
    symbols: &[String],
    extended_hours: bool,
) -> Result<Vec<StockSnapshot>, FetchError> {
    debug_assert!(
        symbols.len() <= SNAPSHOT_BATCH,
        "docs: max 100 symbols per query"
    );
    if symbols.is_empty() {
        return Ok(Vec::new());
    }
    let (status, data) = f.get(
        PATH_STOCK_SNAPSHOTS,
        &[
            ("symbols".into(), symbols.join(",")),
            ("category".into(), "US_STOCK".into()),
            ("extend_hour_required".into(), extended_hours.to_string()),
            // Overnight data is a separate subscription; asking for it fails
            // the whole request with 403 MARKET_DATA_NOT_SUBSCRIBED.
            ("overnight_required".into(), "false".into()),
        ],
    )?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    Ok(parse_stock_snapshots(&data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bars_sorted_ascending_deduped_and_strings_parsed() {
        let raw = json!({"result": [{
            "symbol": "NVDA",
            "result": [
                {"time": "2026-11-20T05:00:00.000+0000", "open": "125.0", "high": "126.0", "low": "124.0", "close": "125.5", "volume": "633"},
                {"time": "2026-11-18T05:00:00.000+0000", "open": "140.0", "high": "141.0", "low": "139.0", "close": "140.5", "volume": "500"},
                {"time": "2026-11-18T05:00:00.000+0000", "open": "140.0", "high": "141.0", "low": "139.0", "close": "140.5", "volume": "500"},
                {"time": "2026-11-19T05:00:00.000+0000", "close": null}
            ]
        }]});
        let bars = parse_stock_bars(&raw);
        assert_eq!(bars.len(), 3); // dedupe
        assert!(bars[0].time.starts_with("2026-11-18"));
        assert!(bars[2].time.starts_with("2026-11-20"));
        assert_eq!(bars[0].close, Some(140.5));
        assert_eq!(bars[1].volume, None); // null → None ไม่ใช่ 0
    }

    #[test]
    fn missing_result_returns_empty_not_panic() {
        assert!(parse_stock_bars(&json!({})).is_empty());
        assert!(parse_stock_bars(&json!({"result": []})).is_empty());
        assert!(parse_stock_bars(&json!({"result": [{"symbol": "X"}]})).is_empty());
    }

    #[test]
    fn timespan_enum_matches_docs() {
        assert!(timespan_valid("D") && timespan_valid("M5") && timespan_valid("Y"));
        assert!(!timespan_valid("d") && !timespan_valid("H1"));
    }

    /// An NVDA row exactly as the Thailand host returned it during pre-market
    /// on 2026-11-23 with `extend_hour_required=true`.
    fn live_extended_row() -> serde_json::Value {
        json!({
            "ask": "136.6000", "ask_size": "8", "bid": "136.5800", "bid_size": "6",
            "change": "-20.450000", "change_ratio": "-0.140088", "close": "125.530000",
            "eps": "4.8979", "eps_ttm": "9.3068", "extend_hour_change": "11.063200",
            "extend_hour_change_ratio": "0.088132", "extend_hour_high": "138.0000",
            "extend_hour_last_price": "136.5932", "extend_hour_last_trade_time": 1795432180808i64,
            "extend_hour_low": "134.6219", "extend_hour_volume": "4975186",
            "fifty_two_wk_high": "243.370000", "fifty_two_wk_low": "124.500000",
            "high": "143.7250", "instrument_id": "913257561",
            "last_trade_time": 1795208386463i64, "low": "124.5000",
            "market_value": "3025273000000.0000", "open": "143.5200",
            "pe_ratio": "13.487987", "pre_close": "145.980000", "price": "136.5932",
            "quote_time": 1795432178700i64, "symbol": "NVDA", "trade_status": "PRE",
            "volume": "4975186", "yield": "0.007966",
        })
    }

    #[test]
    fn snapshot_parses_live_extended_row() {
        let s = parse_stock_snapshot(&live_extended_row()).unwrap();
        assert_eq!(s.symbol, "NVDA");
        // with extended hours on, price follows the pre-market session …
        assert_eq!(s.price, Some(136.5932));
        assert_eq!(s.volume, Some(4_975_186.0));
        // … while the regular session stays readable on its own fields
        assert_eq!(s.close, Some(125.53));
        assert_eq!(s.pre_close, Some(145.98));
        assert_eq!(s.change, Some(-20.45));
        assert_eq!(s.trade_status.as_deref(), Some("PRE"));
        assert_eq!(s.pe_ratio, Some(13.487987));
        assert_eq!(s.market_value, Some(3_025_273_000_000.0));
        assert_eq!(s.dividend_yield, Some(0.007966));
        assert_eq!(s.fifty_two_wk_low, Some(124.5));
        assert_eq!(s.quote_time_ms, Some(1_795_432_178_700));
        let ext = s.extended.expect("a pre-market trade exists");
        assert_eq!(ext.last_price, Some(136.5932));
        assert_eq!(ext.change, Some(11.0632));
        assert_eq!(ext.change_ratio, Some(0.088132));
        assert_eq!(ext.last_trade_time_ms, Some(1_795_432_180_808));
    }

    #[test]
    fn snapshot_without_extended_trade_has_no_extended_block() {
        let s = parse_stock_snapshot(&json!({
            "symbol": "AAPL", "price": "342.5600", "pre_close": "340.10",
            "change": "2.46", "change_ratio": "0.0072", "volume": "123",
            "extend_hour_last_price": "0", "trade_status": "", "yield": "",
            "pe_ratio": null,
        }))
        .unwrap();
        assert_eq!(s.price, Some(342.56));
        assert!(s.extended.is_none(), "a zero extended price is no trade");
        assert_eq!(s.trade_status, None);
        assert_eq!(s.dividend_yield, None);
        assert_eq!(s.pe_ratio, None);
        assert_eq!(s.quote_time_ms, None);
    }

    #[test]
    fn snapshots_parse_both_envelopes_and_skip_symbolless_rows() {
        let bare = json!([live_extended_row(), {"price": "1"}]);
        assert_eq!(parse_stock_snapshots(&bare).len(), 1);
        let wrapped = json!({"data": [live_extended_row()]});
        assert_eq!(parse_stock_snapshots(&wrapped).len(), 1);
        assert!(parse_stock_snapshots(&json!({})).is_empty());
    }

    /// Records the request it was asked for and answers with one row.
    #[derive(Default)]
    struct RecordingFetch {
        calls: usize,
        params: Vec<(String, String)>,
    }

    impl HttpFetch for RecordingFetch {
        fn get(
            &mut self,
            path: &str,
            params: &[(String, String)],
        ) -> Result<(u16, Value), FetchError> {
            assert_eq!(path, PATH_STOCK_SNAPSHOTS);
            self.calls += 1;
            self.params = params.to_vec();
            Ok((200, json!([live_extended_row()])))
        }
    }

    #[test]
    fn batch_request_never_asks_for_overnight_data() {
        fn value_of<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
            params
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str())
        }
        let mut f = RecordingFetch::default();
        let quotes =
            fetch_stock_snapshots(&mut f, &["NVDA".to_string(), "SPY".to_string()], false).unwrap();
        assert_eq!(quotes.len(), 1);
        let first = f.params.clone();
        assert_eq!(value_of(&first, "symbols"), Some("NVDA,SPY"));
        assert_eq!(value_of(&first, "category"), Some("US_STOCK"));
        assert_eq!(value_of(&first, "extend_hour_required"), Some("false"));
        // overnight data is a subscription most keys lack and 403s the request
        assert_eq!(value_of(&first, "overnight_required"), Some("false"));

        // no symbols → no request at all
        assert!(fetch_stock_snapshots(&mut f, &[], true).unwrap().is_empty());
        assert_eq!(f.calls, 1);
        fetch_stock_snapshots(&mut f, &["AAPL".to_string()], true).unwrap();
        assert_eq!(f.calls, 2);
        assert_eq!(value_of(&f.params, "extend_hour_required"), Some("true"));
        assert_eq!(value_of(&f.params, "overnight_required"), Some("false"));
    }
}
