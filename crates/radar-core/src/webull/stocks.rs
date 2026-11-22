//! Stock market-data endpoints (verified by probe 2026-11-23 — PLAN-APP2 §1)
//!
//! ระวัง: endpoints กลุ่มนี้ path **ไม่มี** prefix `/openapi` ต่างจากของเดิม
//! (probe: `/openapi/market-data/stocks/bars/list` → 404, แบบไม่มี prefix → 200)

use serde_json::{Value, json};

use super::rest::{FetchError, HttpFetch, extract_list, to_f64};

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
        return Err(FetchError::NonZero(status, "stock bars".into()));
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
        return Err(FetchError::NonZero(status, "instrument profiles".into()));
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
}
