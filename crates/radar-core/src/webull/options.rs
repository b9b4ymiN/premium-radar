//! Broker option quotes and trades (reference "option-snapshot" and
//! "option-tick", both live-verified 2026-11-23). The scanner keeps seeding
//! prices through the legacy snapshot path in `quotes.rs`; this module is the
//! typed view for wherever the UI shows what Webull itself quotes — greeks,
//! implied volatility, sizes and the quote's own timestamp.

use serde_json::Value;

use super::rest::{FetchError, HttpFetch, extract_list, opt_f64, opt_i64, opt_str};

/// GET `/market-data/options/snapshots/list`
pub const PATH_OPTION_SNAPSHOTS: &str = "/market-data/options/snapshots/list";
/// GET `/market-data/options/ticks/list`
pub const PATH_OPTION_TICKS: &str = "/market-data/options/ticks/list";

/// Option symbols per snapshot request (same batch the scanner uses).
pub const QUOTE_BATCH: usize = 20;

/// One option contract as the broker quotes it. Every number is optional: the
/// broker omits or blanks fields it has no value for, and a blank must stay
/// "unknown", never become 0.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct OptionQuote {
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
    pub open_interest: Option<f64>,
    pub strike_price: Option<f64>,
    /// Implied volatility as a fraction (0.8511 = 85.11 %).
    pub imp_vol: Option<f64>,
    pub delta: Option<f64>,
    pub gamma: Option<f64>,
    pub theta: Option<f64>,
    pub vega: Option<f64>,
    pub rho: Option<f64>,
    pub deal_amount: Option<f64>,
    /// When the broker produced this quote (epoch ms).
    pub quote_time_ms: Option<i64>,
    /// When the contract last traded (epoch ms).
    pub last_trade_time_ms: Option<i64>,
    /// 0 = real-time; above 0 the feed is delayed by that many minutes.
    pub delay_minutes: Option<i64>,
}

impl OptionQuote {
    /// Mid price when both sides stand around a positive value.
    pub fn mid(&self) -> Option<f64> {
        match (self.bid, self.ask) {
            (Some(bid), Some(ask)) if bid > 0.0 && ask >= bid => Some((bid + ask) / 2.0),
            _ => None,
        }
    }

    /// Bid-ask spread as a percentage of the mid; `None` for missing or
    /// crossed quotes (a crossed book has no meaningful mid).
    pub fn spread_pct(&self) -> Option<f64> {
        let mid = self.mid()?;
        if mid <= 0.0 {
            return None;
        }
        Some((self.ask? - self.bid?) / mid * 100.0)
    }

    /// True when the quote carries no price information at all.
    pub fn is_empty(&self) -> bool {
        self.price.unwrap_or(0.0) <= 0.0
            && self.bid.unwrap_or(0.0) <= 0.0
            && self.ask.unwrap_or(0.0) <= 0.0
    }
}

/// One trade of the option tape. `side` is the broker's aggressor code
/// (observed "NB", "NS", "N").
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct OptionTick {
    /// Trade time (epoch ms).
    pub time_ms: Option<i64>,
    pub price: Option<f64>,
    pub volume: Option<f64>,
    pub side: Option<String>,
}

/// Parse one snapshot row; rows without a symbol are skipped.
pub fn parse_option_quote(row: &Value) -> Option<OptionQuote> {
    let symbol = opt_str(row.get("symbol"))?;
    Some(OptionQuote {
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
        open_interest: opt_f64(row.get("open_interest")),
        strike_price: opt_f64(row.get("strike_price")),
        imp_vol: opt_f64(row.get("imp_vol")),
        delta: opt_f64(row.get("delta")),
        gamma: opt_f64(row.get("gamma")),
        theta: opt_f64(row.get("theta")),
        vega: opt_f64(row.get("vega")),
        rho: opt_f64(row.get("rho")),
        deal_amount: opt_f64(row.get("deal_amount")),
        quote_time_ms: opt_i64(row.get("quote_time")),
        last_trade_time_ms: opt_i64(row.get("last_trade_time")),
        delay_minutes: opt_i64(row.get("delay_minutes")),
    })
}

/// Parse a snapshots response (bare array or `{data: [...]}` envelope).
pub fn parse_option_quotes(data: &Value) -> Vec<OptionQuote> {
    extract_list(data)
        .iter()
        .filter_map(parse_option_quote)
        .collect()
}

/// Snapshots for up to [`QUOTE_BATCH`] OCC symbols — one request.
pub fn fetch_option_quotes<F: HttpFetch>(
    f: &mut F,
    symbols: &[String],
) -> Result<Vec<OptionQuote>, FetchError> {
    debug_assert!(symbols.len() <= QUOTE_BATCH, "docs: batch like the scanner");
    let (status, data) = f.get(
        PATH_OPTION_SNAPSHOTS,
        &[
            ("symbols".into(), symbols.join(",")),
            ("category".into(), "US_OPTION".into()),
        ],
    )?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    Ok(parse_option_quotes(&data))
}

/// Parse a ticks response `{symbol, instrument_id, result: [...]}`; the broker
/// returns the latest trade first and that order is kept.
pub fn parse_option_ticks(data: &Value) -> Vec<OptionTick> {
    data.get("result")
        .and_then(Value::as_array)
        .map(|trades| {
            trades
                .iter()
                .map(|trade| OptionTick {
                    time_ms: opt_i64(trade.get("time")),
                    price: opt_f64(trade.get("price")),
                    volume: opt_f64(trade.get("volume")),
                    side: opt_str(trade.get("side")),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The last `count` trades of one OCC symbol (docs: up to 1200).
pub fn fetch_option_ticks<F: HttpFetch>(
    f: &mut F,
    symbol: &str,
    count: u32,
) -> Result<Vec<OptionTick>, FetchError> {
    let (status, data) = f.get(
        PATH_OPTION_TICKS,
        &[
            ("symbol".into(), symbol.to_string()),
            ("category".into(), "US_OPTION".into()),
            ("count".into(), count.to_string()),
        ],
    )?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    Ok(parse_option_ticks(&data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::VecDeque;

    /// A row exactly as the Thailand host returned it on 2026-11-23:
    /// numbers as strings, timestamps as epoch-millisecond integers.
    fn live_row() -> Value {
        json!({
            "ask": "55.75", "ask_size": "30", "bid": "53.35", "bid_size": "1",
            "change": "19.96", "change_ratio": "0.5853", "close": "54.06",
            "deal_amount": "7645.36", "delay_minutes": 0, "delta": "-0.9728",
            "gamma": "0.003", "high": "54.55", "imp_vol": "0.8511",
            "instrument_id": "1064528925", "last_trade_time": 1795208309936i64,
            "low": "35.00", "open": "35.00", "open_interest": "2385",
            "pre_close": "34.10", "price": "54.06", "quote_time": 1795209300147i64,
            "rho": "-0.0466", "strike_price": "180.00", "symbol": "NVDA261204P00180000",
            "theta": "-0.0456", "vega": "0.0159", "volume": "168",
        })
    }

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
    fn parses_live_row_with_greeks_and_times() {
        let q = parse_option_quote(&live_row()).expect("symbol present");
        assert_eq!(q.symbol, "NVDA261204P00180000");
        assert_eq!(q.bid, Some(53.35));
        assert_eq!(q.ask, Some(55.75));
        assert_eq!(q.bid_size, Some(1.0));
        assert_eq!(q.ask_size, Some(30.0));
        assert_eq!(q.open_interest, Some(2385.0));
        assert_eq!(q.strike_price, Some(180.0));
        assert_eq!(q.imp_vol, Some(0.8511));
        assert_eq!(q.delta, Some(-0.9728));
        assert_eq!(q.gamma, Some(0.003));
        assert_eq!(q.theta, Some(-0.0456));
        assert_eq!(q.vega, Some(0.0159));
        assert_eq!(q.rho, Some(-0.0466));
        assert_eq!(q.quote_time_ms, Some(1_795_209_300_147));
        assert_eq!(q.last_trade_time_ms, Some(1_795_208_309_936));
        assert_eq!(q.delay_minutes, Some(0));
        assert!(!q.is_empty());
    }

    #[test]
    fn mid_and_spread_follow_the_book() {
        let q = parse_option_quote(&live_row()).unwrap();
        assert_eq!(q.mid(), Some(54.55));
        // (ask - bid) / mid, in percent — same arithmetic as the implementation
        let expected = (55.75f64 - 53.35) / 54.55 * 100.0;
        assert_eq!(q.spread_pct(), Some(expected));
        assert!((expected - 4.4).abs() < 0.01);

        let crossed = OptionQuote {
            bid: Some(10.0),
            ask: Some(9.0),
            ..Default::default()
        };
        assert_eq!(crossed.mid(), None);
        assert_eq!(crossed.spread_pct(), None);

        let blank = OptionQuote::default();
        assert!(blank.is_empty());
        assert_eq!(blank.mid(), None);
        assert_eq!(blank.spread_pct(), None);
    }

    #[test]
    fn blank_fields_stay_unknown_not_zero() {
        let mut row = live_row();
        row["imp_vol"] = json!("");
        row["open_interest"] = json!(null);
        row["delta"] = json!("junk");
        row.as_object_mut().unwrap().remove("vega");
        let q = parse_option_quote(&row).unwrap();
        assert_eq!(q.imp_vol, None);
        assert_eq!(q.open_interest, None);
        assert_eq!(q.delta, None);
        assert_eq!(q.vega, None);
        // a genuine zero is kept
        assert_eq!(q.delay_minutes, Some(0));
    }

    #[test]
    fn quotes_parse_bare_array_and_data_envelope() {
        let bare = json!([live_row(), {"symbol": "", "price": "1.0"}]);
        assert_eq!(parse_option_quotes(&bare).len(), 1);
        let wrapped = json!({"data": [live_row()]});
        assert_eq!(parse_option_quotes(&wrapped).len(), 1);
        assert!(parse_option_quotes(&json!({})).is_empty());
    }

    #[test]
    fn fetch_uses_the_documented_path_and_category() {
        let mut f = MockFetch {
            map: VecDeque::from(vec![(PATH_OPTION_SNAPSHOTS.into(), json!([live_row()]))]),
        };
        let quotes = fetch_option_quotes(
            &mut f,
            &[
                "NVDA261204P00180000".to_string(),
                "NVDA261204C00180000".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(quotes.len(), 1);
        assert_eq!(quotes[0].symbol, "NVDA261204P00180000");
    }

    #[test]
    fn ticks_keep_latest_first_order_and_skip_nothing() {
        let data = json!({
            "symbol": "NVDA261204P00180000", "instrument_id": "1064528925",
            "result": [
                {"time": "1795208309936", "price": "54.06", "volume": "2", "side": "NB"},
                {"time": "1795208300000", "price": "53.90", "volume": "10", "side": "NS"},
            ],
        });
        let ticks = parse_option_ticks(&data);
        assert_eq!(ticks.len(), 2);
        assert_eq!(ticks[0].time_ms, Some(1_795_208_309_936));
        assert_eq!(ticks[0].price, Some(54.06));
        assert_eq!(ticks[0].side.as_deref(), Some("NB"));
        assert_eq!(ticks[1].volume, Some(10.0));
        // shapes without a result list are simply empty
        assert!(parse_option_ticks(&json!({"result": "oops"})).is_empty());
        assert!(parse_option_ticks(&json!([])).is_empty());
    }
}
