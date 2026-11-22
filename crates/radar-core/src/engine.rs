//! State engine — coalescing store for live quotes + enrichment pipeline +
//! screener compose. Ported from `_LatestSnapshots` + `Dashboard._drain_events`
//! in the Python reference. Pure (no Tauri/no threads) — the Tauri event pump
//! wraps this in src-tauri.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::screener::{
    ChainStats, EnrichedRow, Filters, QuoteRow, chain_stats, enrich_rows_raw, short_candidates,
};
use crate::webull::contracts::parse_occ;
use crate::webull::rest::to_f64;

/// Data source badge — honest freshness labels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize)]
pub enum DataSource {
    #[default]
    #[serde(rename = "API")]
    Rest,
    /// สด — live WebSocket push
    #[serde(rename = "สด")]
    Ws,
    /// ราคาปิด — daily bars fallback (market closed)
    #[serde(rename = "ราคาปิด")]
    Bars,
}

impl DataSource {
    pub fn as_str(self) -> &'static str {
        match self {
            DataSource::Ws => "สด",
            DataSource::Rest => "API",
            DataSource::Bars => "ราคาปิด",
        }
    }
}

/// A single option-contract quote update (from WS push or REST snapshot).
#[derive(Clone, Debug)]
pub struct QuoteUpdate {
    pub symbol: String,
    pub price: f64,
    pub volume: f64,
    pub change: f64,
    pub change_ratio: f64,
    pub oi: f64,
    pub bid: f64,
    pub ask: f64,
    pub src: DataSource,
}

/// Stock-level quote update.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct StockUpdate {
    pub price: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub pre_close: f64,
    pub volume: f64,
    pub change: f64,
    pub change_ratio: f64,
    pub src: DataSource,
}

/// Result of a full engine cycle: enriched rows + screener output + stats.
#[derive(Clone, Debug, serde::Serialize)]
pub struct EngineSnapshot {
    pub rows: Vec<EnrichedRow>,
    pub candidates: Vec<EnrichedRow>,
    pub spot: Option<f64>,
    pub stock: Option<StockUpdate>,
    pub stats: Option<ChainStats>,
}

/// Coalescing state engine. In production, the Tauri event pump pushes updates
/// into this and reads `snapshot()` at ~800ms intervals for the frontend.
pub struct Engine {
    /// Latest quote per OCC symbol — newer pushes overwrite older ones.
    latest: HashMap<String, QuoteUpdate>,
    /// Latest stock-level quote.
    stock: Option<StockUpdate>,
    /// Known symbol universe (from discovery).
    symbols: Vec<String>,
    /// Current spot price (from stock quote or discovery).
    spot: Option<f64>,
    /// Cached enriched rows (recomputed on demand).
    enriched_cache: Option<Vec<EnrichedRow>>,
    /// Active filters.
    filters: Filters,
    /// Selected expiry for chain_stats (None = first expiry).
    selected_expiry: Option<chrono::NaiveDate>,
    /// Clock for enrichment (injected for determinism).
    now: DateTime<Utc>,
}

impl Engine {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            latest: HashMap::new(),
            stock: None,
            symbols: Vec::new(),
            spot: None,
            enriched_cache: None,
            filters: Filters::default(),
            selected_expiry: None,
            now,
        }
    }

    /// Set the symbol universe from discovery (fetch_symbols_flow output).
    pub fn set_symbols(&mut self, symbols: Vec<String>, spot: Option<f64>) {
        self.symbols = symbols;
        self.spot = spot;
        self.latest.clear();
        self.enriched_cache = None;
    }

    /// Push an option-contract quote (WS or REST). Newest wins per symbol.
    /// Empty WS snapshots (market closed, all-zero) are rejected like Python.
    pub fn push_quote(&mut self, update: QuoteUpdate) {
        if update.price <= 0.0 && update.bid <= 0.0 && update.ask <= 0.0 {
            return; // empty snapshot — keep existing data
        }
        // mid-price fallback when price is zero but bid/ask exist
        let mut u = update;
        if u.price <= 0.0 && u.bid > 0.0 && u.ask > 0.0 {
            u.price = (u.bid + u.ask) / 2.0;
        }
        // fill oi/bid/ask from previous when missing (Python py 2687-2689)
        if u.oi <= 0.0
            && let Some(prev) = self.latest.get(&u.symbol)
        {
            u.oi = prev.oi;
        }
        if u.bid <= 0.0
            && let Some(prev) = self.latest.get(&u.symbol)
        {
            u.bid = prev.bid;
        }
        if u.ask <= 0.0
            && let Some(prev) = self.latest.get(&u.symbol)
        {
            u.ask = prev.ask;
        }
        self.latest.insert(u.symbol.clone(), u);
        self.enriched_cache = None; // invalidate
    }

    /// Push a stock-level quote (updates spot).
    pub fn push_stock(&mut self, update: StockUpdate) {
        if update.price > 0.0 {
            self.spot = Some(update.price);
        }
        self.stock = Some(update);
        self.enriched_cache = None;
    }

    /// Seed from a REST snapshot row (lower priority than WS).
    pub fn seed_quote(&mut self, update: QuoteUpdate) {
        // WS data always wins (Python: `cur and not cur.get("seed")` → skip)
        if self
            .latest
            .get(&update.symbol)
            .is_some_and(|e| e.src == DataSource::Ws)
        {
            return;
        }
        // BARS fallback never overrides a real quote
        if update.src == DataSource::Bars
            && self
                .latest
                .get(&update.symbol)
                .is_some_and(|e| e.price > 0.0 || e.bid > 0.0 || e.ask > 0.0)
        {
            return;
        }
        self.push_quote(update);
    }

    /// Set the active filters (invalidates screener cache).
    pub fn set_filters(&mut self, filters: Filters) {
        self.filters = filters;
        self.enriched_cache = None;
    }

    /// Build QuoteRows from the latest quotes + OCC metadata.
    fn build_quote_rows(&self) -> Vec<QuoteRow> {
        self.symbols
            .iter()
            .filter_map(|sym| {
                let update = self.latest.get(sym)?;
                let meta = parse_occ(sym)?;
                Some(QuoteRow {
                    symbol: sym.clone(),
                    underlying: meta.underlying,
                    expiry: meta.expiry,
                    option_type: meta.option_type,
                    strike: meta.strike,
                    price: update.price,
                    bid: update.bid,
                    ask: update.ask,
                    volume: update.volume,
                    oi: update.oi,
                    change: update.change,
                    change_ratio: update.change_ratio,
                    src: update.src.as_str().to_string(),
                })
            })
            .collect()
    }

    /// Enrich all rows and run the screener (recomputes if cache invalidated).
    fn enriched(&mut self) -> &[EnrichedRow] {
        if self.enriched_cache.is_none() {
            let rows = self.build_quote_rows();
            let spot = self.spot.unwrap_or(0.0);
            self.enriched_cache = Some(enrich_rows_raw(rows, spot, self.now));
        }
        self.enriched_cache.as_deref().unwrap_or(&[])
    }

    /// Produce a full snapshot for the frontend: enriched rows, screened
    /// candidates, spot, stock stats, chain stats.
    pub fn snapshot(&mut self) -> EngineSnapshot {
        let all = self.enriched().to_vec();
        let spot = self.spot.unwrap_or(0.0);
        let refs: Vec<&EnrichedRow> = all.iter().collect();
        let candidates: Vec<EnrichedRow> = short_candidates(&refs, spot, &self.filters)
            .into_iter()
            .cloned()
            .collect();
        // chain_stats on the selected or first expiry
        let stats = self
            .selected_expiry
            .or_else(|| all.first().map(|r| r.q.expiry))
            .map(|exp| chain_stats(&refs, spot, exp))
            .filter(|s| s.atm_iv.is_some());
        EngineSnapshot {
            rows: all,
            candidates,
            spot: self.spot,
            stock: self.stock.clone(),
            stats,
        }
    }

    /// Current spot price.
    pub fn spot(&self) -> Option<f64> {
        self.spot
    }

    /// Count of rows with a live (non-seed) WS source.
    pub fn live_count(&self) -> usize {
        self.latest
            .values()
            .filter(|q| q.src == DataSource::Ws)
            .count()
    }

    /// Total known quotes.
    pub fn quote_count(&self) -> usize {
        self.latest.len()
    }

    /// Symbols that have quotes but are no longer in the universe (stale).
    pub fn stale_symbols(&self) -> Vec<String> {
        let universe: std::collections::HashSet<&String> = self.symbols.iter().collect();
        self.latest
            .keys()
            .filter(|s| !universe.contains(s))
            .cloned()
            .collect()
    }
}

/// Parse a Webull REST/WS snapshot JSON object into a QuoteUpdate.
pub fn parse_quote_update(v: &Value, src: DataSource) -> Option<QuoteUpdate> {
    let symbol = v.get("symbol").and_then(Value::as_str)?.replace(' ', "");
    parse_occ(&symbol)?;
    Some(QuoteUpdate {
        symbol,
        price: to_f64(v.get("price").unwrap_or(&Value::Null), 0.0),
        volume: to_f64(v.get("volume").unwrap_or(&Value::Null), 0.0),
        change: to_f64(v.get("change").unwrap_or(&Value::Null), 0.0),
        change_ratio: to_f64(v.get("change_ratio").unwrap_or(&Value::Null), 0.0),
        oi: to_f64(
            v.get("open_interest")
                .or_else(|| v.get("openInterest"))
                .unwrap_or(&Value::Null),
            0.0,
        ),
        bid: to_f64(v.get("bid").unwrap_or(&Value::Null), 0.0),
        ask: to_f64(v.get("ask").unwrap_or(&Value::Null), 0.0),
        src,
    })
}

/// Parse a stock snapshot JSON object into a StockUpdate.
pub fn parse_stock_update(v: &Value, src: DataSource) -> Option<StockUpdate> {
    let price = to_f64(v.get("price").unwrap_or(&Value::Null), 0.0);
    if price <= 0.0 {
        return None;
    }
    Some(StockUpdate {
        price,
        open: to_f64(v.get("open").unwrap_or(&Value::Null), 0.0),
        high: to_f64(v.get("high").unwrap_or(&Value::Null), 0.0),
        low: to_f64(v.get("low").unwrap_or(&Value::Null), 0.0),
        pre_close: to_f64(
            v.get("pre_close")
                .or_else(|| v.get("preClose"))
                .or_else(|| v.get("prev_close"))
                .unwrap_or(&Value::Null),
            0.0,
        ),
        volume: to_f64(v.get("volume").unwrap_or(&Value::Null), 0.0),
        change: to_f64(v.get("change").unwrap_or(&Value::Null), 0.0),
        change_ratio: to_f64(v.get("change_ratio").unwrap_or(&Value::Null), 0.0),
        src,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        "2026-11-22T22:00:00+00:00".parse().unwrap()
    }

    fn q(sym: &str, price: f64, bid: f64, ask: f64, src: DataSource) -> QuoteUpdate {
        QuoteUpdate {
            symbol: sym.into(),
            price,
            volume: 100.0,
            change: 0.0,
            change_ratio: 0.0,
            oi: 500.0,
            bid,
            ask,
            src,
        }
    }

    #[test]
    fn newest_wins_per_symbol() {
        let mut e = Engine::new(now());
        e.set_symbols(vec!["NVDA261218P00175000".into()], Some(182.45));
        e.push_quote(q("NVDA261218P00175000", 4.20, 4.15, 4.25, DataSource::Ws));
        e.push_quote(q("NVDA261218P00175000", 4.30, 4.25, 4.35, DataSource::Ws));
        assert_eq!(e.quote_count(), 1);
        let snap = e.snapshot();
        assert_eq!(snap.rows[0].prem, Some(4.25)); // credit from bid=4.25
    }

    #[test]
    fn empty_ws_snapshot_rejected() {
        let mut e = Engine::new(now());
        e.set_symbols(vec!["NVDA261218P00175000".into()], Some(182.45));
        e.push_quote(q("NVDA261218P00175000", 4.20, 4.15, 4.25, DataSource::Ws));
        e.push_quote(q("NVDA261218P00175000", 0.0, 0.0, 0.0, DataSource::Ws));
        assert_eq!(e.quote_count(), 1); // didn't overwrite
    }

    #[test]
    fn ws_beats_rest_seed() {
        let mut e = Engine::new(now());
        e.set_symbols(vec!["NVDA261218P00175000".into()], Some(182.45));
        e.seed_quote(q("NVDA261218P00175000", 4.10, 4.05, 4.15, DataSource::Rest));
        e.seed_quote(q("NVDA261218P00175000", 4.00, 3.95, 4.05, DataSource::Rest));
        assert_eq!(e.quote_count(), 1);
        // WS push overwrites REST
        e.push_quote(q("NVDA261218P00175000", 4.30, 4.25, 4.35, DataSource::Ws));
        // further REST seed is rejected
        e.seed_quote(q("NVDA261218P00175000", 4.00, 3.95, 4.05, DataSource::Rest));
        let snap = e.snapshot();
        assert_eq!(snap.rows[0].prem, Some(4.25)); // still WS bid
        assert_eq!(e.live_count(), 1);
    }

    #[test]
    fn bars_never_overrides_real_quote() {
        let mut e = Engine::new(now());
        e.set_symbols(vec!["NVDA261218P00175000".into()], Some(182.45));
        e.push_quote(q("NVDA261218P00175000", 4.20, 4.15, 4.25, DataSource::Rest));
        e.seed_quote(q("NVDA261218P00175000", 4.00, 0.0, 0.0, DataSource::Bars));
        assert_eq!(e.quote_count(), 1);
        let snap = e.snapshot();
        assert_eq!(snap.rows[0].prem, Some(4.15)); // REST bid preserved
    }

    #[test]
    fn mid_price_fallback() {
        let mut e = Engine::new(now());
        e.set_symbols(vec!["NVDA261218P00175000".into()], Some(182.45));
        e.push_quote(QuoteUpdate {
            symbol: "NVDA261218P00175000".into(),
            price: 0.0,
            volume: 0.0,
            change: 0.0,
            change_ratio: 0.0,
            oi: 0.0,
            bid: 4.10,
            ask: 4.20,
            src: DataSource::Ws,
        });
        let snap = e.snapshot();
        assert!(snap.rows[0].iv.is_some()); // IV solved from mid=4.15
    }

    #[test]
    fn oi_backfill_from_previous() {
        let mut e = Engine::new(now());
        e.set_symbols(vec!["NVDA261218P00175000".into()], Some(182.45));
        e.push_quote(q("NVDA261218P00175000", 4.20, 4.15, 4.25, DataSource::Ws));
        // next push has oi=0 → backfill from previous
        e.push_quote(QuoteUpdate {
            symbol: "NVDA261218P00175000".into(),
            price: 4.25,
            volume: 200.0,
            change: 0.0,
            change_ratio: 0.0,
            oi: 0.0,
            bid: 4.20,
            ask: 4.30,
            src: DataSource::Ws,
        });
        let snap = e.snapshot();
        assert_eq!(snap.rows[0].q.oi, 500.0); // backfilled
    }

    #[test]
    fn screener_filters_applied() {
        let mut e = Engine::new(now());
        e.set_symbols(
            vec![
                "NVDA261218P00175000".into(),
                "NVDA261218P00165000".into(),
                "NVDA261218C00195000".into(),
            ],
            Some(182.45),
        );
        e.push_stock(StockUpdate {
            price: 182.45,
            ..Default::default()
        });
        e.push_quote(q("NVDA261218P00175000", 4.20, 4.15, 4.25, DataSource::Ws));
        e.push_quote(q("NVDA261218P00165000", 2.25, 2.20, 2.30, DataSource::Ws));
        e.push_quote(q("NVDA261218C00195000", 2.20, 2.15, 2.25, DataSource::Ws));
        let snap = e.snapshot();
        assert!(snap.spot.is_some());
        assert!(!snap.candidates.is_empty());
        // all 3 should be enriched
        assert_eq!(snap.rows.len(), 3);
    }

    #[test]
    fn parse_quote_update_from_json() {
        let v = serde_json::json!({
            "symbol": "NVDA261218P00175000",
            "price": 4.20,
            "volume": 1842,
            "bid": 4.15,
            "ask": 4.25,
            "open_interest": 3120
        });
        let q = parse_quote_update(&v, DataSource::Rest).unwrap();
        assert_eq!(q.symbol, "NVDA261218P00175000");
        assert_eq!(q.price, 4.20);
        assert_eq!(q.oi, 3120.0);
        assert_eq!(q.src, DataSource::Rest);
        // invalid symbol returns None
        let bad = serde_json::json!({"symbol": "JUNK", "price": 1.0});
        assert!(parse_quote_update(&bad, DataSource::Rest).is_none());
    }
}
