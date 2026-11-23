//! REST seeding pipeline — port of `Dashboard._rest_fetch` in the Python
//! reference: stock snapshot → option snapshots (OI_BATCH per call) →
//! daily-bars fallback for contracts that still have no price.

use chrono::{NaiveDate, Utc};
use chrono_tz::America::New_York;
use serde_json::Value;

use crate::engine::{DataSource, QuoteUpdate, StockUpdate, parse_quote_update, parse_stock_update};
use crate::webull::rest::{
    FetchError, HttpFetch, PATH_OPTION_BARS, PATH_OPTION_SNAPSHOT, PATH_STOCK_SNAPSHOT,
    extract_list, get_json, to_f64,
};

/// Option snapshots per REST call (Python `OI_BATCH = 20`).
pub const OI_BATCH: usize = 20;

/// Today on the New York calendar — the market the screener runs on
/// (Python `_ny_today`).
pub fn ny_today() -> NaiveDate {
    Utc::now().with_timezone(&New_York).date_naive()
}

/// Stock snapshot → `StockUpdate` (src REST). Mirrors the stock half of
/// `_rest_fetch`; parse_stock_update rejects price<=0 rows like the engine.
pub fn fetch_stock_update<F: HttpFetch>(
    f: &mut F,
    root: &str,
) -> Result<Option<StockUpdate>, FetchError> {
    Ok(fetch_stock_snapshot(f, root)?
        .as_ref()
        .and_then(|row| parse_stock_update(row, DataSource::Rest)))
}

/// Regular-session REST snapshot, preserving optional fields for display consumers.
pub fn fetch_stock_snapshot<F: HttpFetch>(
    f: &mut F,
    root: &str,
) -> Result<Option<Value>, FetchError> {
    let data = get_json(
        f,
        PATH_STOCK_SNAPSHOT,
        &[
            ("symbols".into(), root.into()),
            ("category".into(), "US_STOCK".into()),
            ("extend_hour_required".into(), "false".into()),
            ("overnight_required".into(), "false".into()),
        ],
    )?;
    Ok(extract_list(&data).into_iter().next())
}

/// Option snapshot for one batch → (usable seeds, symbols still without a
/// price). A symbol counts as seeded only when price/bid/ask is positive
/// (Python drops all-zero rows into `empty` for the bars fallback).
pub fn fetch_option_seeds<F: HttpFetch>(
    f: &mut F,
    batch: &[String],
) -> Result<(Vec<QuoteUpdate>, Vec<String>), FetchError> {
    let data = get_json(
        f,
        PATH_OPTION_SNAPSHOT,
        &[
            ("symbols".into(), batch.join(",")),
            ("category".into(), "US_OPTION".into()),
        ],
    )?;
    let mut seeds = Vec::new();
    let mut seeded: std::collections::HashSet<String> = std::collections::HashSet::new();
    for it in extract_list(&data) {
        let Some(q) = parse_quote_update(&it, DataSource::Rest) else {
            continue;
        };
        if q.price > 0.0 || q.bid > 0.0 || q.ask > 0.0 {
            seeded.insert(q.symbol.clone());
            seeds.push(q);
        }
    }
    let empty: Vec<String> = batch
        .iter()
        .filter(|s| !seeded.contains(*s))
        .cloned()
        .collect();
    Ok((seeds, empty))
}

/// Daily-bars fallback for contracts the snapshot could not price.
pub fn fetch_bars_seeds<F: HttpFetch>(
    f: &mut F,
    batch: &[String],
) -> Result<Vec<QuoteUpdate>, FetchError> {
    let data = get_json(
        f,
        PATH_OPTION_BARS,
        &[
            ("symbols".into(), batch.join(",")),
            ("category".into(), "US_OPTION".into()),
            ("timespan".into(), "D".into()),
            ("count".into(), "3".into()),
            ("real_time_required".into(), "false".into()),
        ],
    )?;
    Ok(bars_to_seeds(&data))
}

/// Pure port of `_bars_to_seeds`: last daily close per symbol, change against
/// the previous close, oi/bid/ask zeroed (src BARS — never overrides a real
/// quote in the engine).
pub fn bars_to_seeds(data: &Value) -> Vec<QuoteUpdate> {
    let items = match data {
        Value::Object(_) => data.get("result").cloned().unwrap_or(Value::Null),
        v => v.clone(),
    };
    let Value::Array(items) = items else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for it in items {
        let sym = it
            .get("symbol")
            .and_then(Value::as_str)
            .unwrap_or("")
            .replace(' ', "");
        // sort bars by their `time` value compared as strings (Python: str(b.get("time")))
        let mut bars: Vec<(String, &Value)> = it
            .get("result")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|b| (b.get("time").map(|t| t.to_string()).unwrap_or_default(), b))
                    .collect()
            })
            .unwrap_or_default();
        bars.sort_by(|a, b| a.0.cmp(&b.0));
        if sym.is_empty() || bars.is_empty() {
            continue;
        }
        let last = bars[bars.len() - 1].1;
        let prev = if bars.len() > 1 {
            Some(bars[bars.len() - 2].1)
        } else {
            None
        };
        let close = to_f64(last.get("close").unwrap_or(&Value::Null), 0.0);
        if close <= 0.0 {
            continue;
        }
        let pc = prev
            .map(|p| to_f64(p.get("close").unwrap_or(&Value::Null), 0.0))
            .unwrap_or(0.0);
        out.push(QuoteUpdate {
            symbol: sym,
            price: close,
            volume: to_f64(last.get("volume").unwrap_or(&Value::Null), 0.0),
            change: if pc > 0.0 { close - pc } else { 0.0 },
            change_ratio: if pc > 0.0 { (close - pc) / pc } else { 0.0 },
            oi: 0.0,
            bid: 0.0,
            ask: 0.0,
            src: DataSource::Bars,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::VecDeque;

    /// Path-keyed mock: each call pops the queued response for that path.
    struct MockFetch {
        map: VecDeque<(String, Value)>,
    }

    impl HttpFetch for MockFetch {
        fn get(
            &mut self,
            path: &str,
            _params: &[(String, String)],
        ) -> Result<(u16, Value), FetchError> {
            match self.map.pop_front() {
                Some((p, v)) if p == path => Ok((200, v)),
                _ => Err(FetchError::Network(format!("no mock for {path}"))),
            }
        }
    }

    #[test]
    fn stock_update_from_wrapped_data() {
        let mut f = MockFetch {
            map: VecDeque::from(vec![(
                PATH_STOCK_SNAPSHOT.into(),
                json!({"data": [{"symbol": "NVDA", "price": 182.45, "open": 180.0,
                    "high": 183.7, "low": 179.1, "pre_close": 180.14,
                    "change": 2.31, "change_ratio": 0.0128}]}),
            )]),
        };
        let stk = fetch_stock_update(&mut f, "NVDA").unwrap().unwrap();
        assert_eq!(stk.price, 182.45);
        assert_eq!(stk.pre_close, 180.14);
        assert_eq!(stk.src, DataSource::Rest);
    }

    #[test]
    fn option_seeds_split_usable_and_empty() {
        let mut f = MockFetch {
            map: VecDeque::from(vec![(
                PATH_OPTION_SNAPSHOT.into(),
                json!({"data": [
                    {"symbol": "AAPL260116C00100000", "price": 1.5, "bid": 1.4, "ask": 1.6,
                     "open_interest": 120, "volume": 10},
                    {"symbol": "AAPL260116C00105000", "price": 0.0, "bid": 0.0, "ask": 0.0},
                    {"symbol": "not-an-occ", "price": 9.9}
                ]}),
            )]),
        };
        let batch = vec![
            "AAPL260116C00100000".into(),
            "AAPL260116C00105000".into(),
            "AAPL260116P00095000".into(), // absent from the response
        ];
        let (seeds, empty) = fetch_option_seeds(&mut f, &batch).unwrap();
        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].symbol, "AAPL260116C00100000");
        assert_eq!(seeds[0].oi, 120.0);
        // all-zero row + unseen symbol both fall through to the bars fallback
        assert_eq!(empty, vec!["AAPL260116C00105000", "AAPL260116P00095000"]);
    }

    #[test]
    fn bars_to_seeds_both_shapes_and_sort() {
        let dict_shape = json!({"result": [
            {"symbol": "AAPL260116C00100000", "result": [
                {"time": 200, "close": 1.1},
                {"time": 100, "close": 1.0, "volume": 5},
                {"time": 300, "close": 1.3},
            ]},
            {"symbol": "AAPL260116C00105000", "result": [
                {"time": 100, "close": 0.0},
            ]},
            {"symbol": "", "result": [{"time": 1, "close": 2.0}]},
            {"symbol": "AAPL260116C00110000", "result": []},
        ]});
        let seeds = bars_to_seeds(&dict_shape);
        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].symbol, "AAPL260116C00100000");
        // last bar (time 300) wins; change vs the previous close (1.1)
        assert_eq!(seeds[0].price, 1.3);
        assert!((seeds[0].change - 0.2).abs() < 1e-9);
        assert_eq!(seeds[0].volume, 0.0); // last bar has no volume
        assert_eq!(seeds[0].src, DataSource::Bars);

        let list_shape = json!([{"symbol": "TSLA260116P00300000",
            "result": [{"time": 10, "close": 2.5}]}]);
        let seeds = bars_to_seeds(&list_shape);
        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].price, 2.5);
        assert_eq!(seeds[0].change, 0.0); // single bar → no prev close
    }
}
