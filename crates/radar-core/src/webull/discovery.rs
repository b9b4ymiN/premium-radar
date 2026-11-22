//! Contract discovery — windows/pagination scan, OCC collection, strike
//! selection and nearest-to-spot ranking. Ported from the Python reference
//! (`_fetch_window`, `fetch_option_contracts`, `collect_occ_symbols`,
//! `fetch_spot`, `pick_ws_symbols`, `fetch_symbols`). Network-agnostic via
//! [`HttpFetch`](super::rest::HttpFetch); the clock is injected so every
//! path is deterministic in tests.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::{Duration, NaiveDate};
use serde_json::Value;

use super::contracts::{OccMeta, parse_occ};
use super::rest::{
    FetchError, HttpFetch, MAX_ANALYZE, MAX_EXPIRIES, PATH_OPTION_CONTRACTS, PATH_STOCK_SNAPSHOT,
    STRIKE_BAND, STRIKES_EACH_SIDE, WINDOWS, extract_list, get_json, to_f64,
};
use crate::quant::OptType;

/// Progress sink (`on_progress` in Python).
pub type Progress<'a> = Option<&'a mut dyn FnMut(String)>;
/// Cancellation probe (`stale` in Python) — return true to abort.
pub type Stale<'a> = Option<&'a dyn Fn() -> bool>;

fn stale_err() -> FetchError {
    FetchError::Network("stale".into())
}

fn value_id(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

// ---------- window scan (mirrors _fetch_window) ----------

#[allow(clippy::too_many_arguments)]
pub fn fetch_window<F: HttpFetch>(
    f: &mut F,
    symbol: &str,
    a: NaiveDate,
    b: NaiveDate,
    gte: Option<&str>,
    lte: Option<&str>,
    max_pages: usize,
    stale: Stale<'_>,
) -> Result<Vec<Value>, FetchError> {
    let mut rows = Vec::new();
    let mut key: Option<String> = None;
    let mut pages = 0usize;
    while pages < max_pages {
        if let Some(true) = stale.map(|s| s()) {
            return Err(stale_err());
        }
        let mut params: Vec<(String, String)> = vec![
            ("underlying_symbols".into(), symbol.into()),
            ("category".into(), "US_OPTION".into()),
            ("status".into(), "LISTING".into()),
            ("start_date".into(), a.to_string()),
            ("end_date".into(), b.to_string()),
        ];
        if let Some(g) = gte {
            params.push(("strike_price_gte".into(), g.into()));
        }
        if let Some(l) = lte {
            params.push(("strike_price_lte".into(), l.into()));
        }
        if let Some(k) = &key {
            params.push(("pagination_key".into(), k.clone()));
        }
        let data = get_json(f, PATH_OPTION_CONTRACTS, &params)?;
        rows.extend(extract_list(&data));
        pages += 1;
        key = super::rest::pagination_key(&data);
        if key.is_none() {
            break;
        }
    }
    Ok(rows)
}

// ---------- contract scan across windows (mirrors fetch_option_contracts) ----------

#[allow(clippy::too_many_arguments)]
pub fn fetch_option_contracts<F: HttpFetch>(
    f: &mut F,
    symbol: &str,
    gte: Option<&str>,
    lte: Option<&str>,
    last_day: Option<NaiveDate>,
    max_pages: usize,
    need_expiries: Option<usize>,
    today: NaiveDate,
    stale: Stale<'_>,
    mut on_progress: Progress<'_>,
) -> Result<Vec<Value>, FetchError> {
    let today_s = today.to_string();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<Value> = Vec::new();
    let mut exps: BTreeSet<String> = BTreeSet::new();
    for (x, y) in WINDOWS {
        let mut a = today + Duration::days(x);
        let mut b = today + Duration::days(y);
        if let Some(last) = last_day {
            if a > last {
                continue;
            }
            if b > last {
                b = last;
            }
        }
        if a < today {
            a = today;
        }
        let rows = fetch_window(f, symbol, a, b, gte, lte, max_pages, stale)?;
        for c in &rows {
            let iid = value_id(c.get("instrument_id").unwrap_or(&Value::Null));
            let ed = c
                .get("expiration_date")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if seen.contains(&iid) || ed < today_s {
                continue;
            }
            seen.insert(iid);
            out.push(c.clone());
            exps.insert(ed);
        }
        if let Some(p) = on_progress.as_mut() {
            p(format!(
                "contracts {}..{}: +{} (total {}, {} expiries)",
                a,
                b,
                rows.len(),
                out.len(),
                exps.len()
            ));
        }
        if need_expiries.is_some_and(|need| exps.len() >= need) {
            break;
        }
    }
    Ok(out)
}

// ---------- OCC collection (mirrors collect_occ_symbols) ----------

/// Recursively collect valid OCC symbols from a raw contracts payload.
pub fn collect_occ_symbols(node: &Value, out: &mut BTreeSet<String>) {
    match node {
        Value::Object(o) => {
            if let Some(Value::String(sym)) = o
                .get("symbol")
                .filter(|v| v.as_str().is_some_and(|s| parse_occ(s).is_some()))
            {
                out.insert(sym.replace(' ', ""));
            }
            for child in o.values() {
                collect_occ_symbols(child, out);
            }
        }
        Value::Array(a) => {
            for child in a {
                collect_occ_symbols(child, out);
            }
        }
        _ => {}
    }
}

// ---------- spot (mirrors fetch_spot) ----------

pub fn fetch_spot<F: HttpFetch>(f: &mut F, root: &str) -> Result<Option<f64>, FetchError> {
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
    let list = extract_list(&data);
    let Some(first) = list.first() else {
        return Ok(None);
    };
    let p = to_f64(first.get("price").unwrap_or(&Value::Null), 0.0);
    Ok((p > 0.0).then_some(p))
}

/// ±STRIKE_BAND around spot formatted like Python `f"{v:.2f}"`.
pub fn strike_band(spot: f64) -> (String, String) {
    (
        format!("{:.2}", spot * (1.0 - STRIKE_BAND)),
        format!("{:.2}", spot * (1.0 + STRIKE_BAND)),
    )
}

// ---------- nearest-to-spot ranking (mirrors pick_ws_symbols) ----------

/// Rank symbols nearest-to-spot first, per (expiry, side); then interleave by
/// rank. Used for WebSocket subscriptions and REST ordering.
pub fn pick_ws_symbols(syms: &[String], spot: f64, limit: usize) -> Vec<String> {
    // Python `limit or MAX_CONTRACTS`: zero means "use the default".
    let limit = if limit == 0 {
        super::rest::MAX_CONTRACTS
    } else {
        limit
    };
    let mut groups: HashMap<(NaiveDate, OptType), Vec<(f64, &String)>> = HashMap::new();
    for sy in syms {
        if let Some(m) = parse_occ(sy) {
            let d = (m.strike - spot).abs();
            groups
                .entry((m.expiry, m.option_type))
                .or_default()
                .push((d, sy));
        }
    }
    let mut ranked: Vec<(usize, NaiveDate, &String)> = Vec::new();
    for list in groups.values() {
        let mut list = list.clone();
        // Python sorts (distance, symbol): equidistant strikes tie-break by symbol
        list.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(b.1))
        });
        for (rank, (_, sy)) in list.iter().enumerate() {
            // expiry here is only a tiebreak; recover it from parse for sort parity
            if let Some(m) = parse_occ(sy) {
                ranked.push((rank, m.expiry, sy));
            }
        }
    }
    ranked.sort();
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, _, sy)| sy.clone())
        .collect()
}

// ---------- strike/expiry selection (pure tail of fetch_symbols) ----------

/// Filter the discovered OCC symbols down to the analyzable set:
/// first `MAX_EXPIRIES` expiries, `STRIKES_EACH_SIDE` strikes per side
/// (ATM inclusive), sorted by (expiry, strike, type), capped at
/// `MAX_ANALYZE`. Returns (symbols, effective spot) — when no live spot is
/// known, the median strike stands in (Python fallback).
pub fn select_symbols(
    root: &str,
    found: &BTreeSet<String>,
    spot: Option<f64>,
    today: NaiveDate,
) -> Result<(Vec<String>, f64), FetchError> {
    let mut metas: BTreeMap<String, OccMeta> = BTreeMap::new();
    for s in found {
        if let Some(m) = parse_occ(s).filter(|m| m.underlying == root) {
            metas.insert(s.clone(), m);
        }
    }
    if metas.is_empty() {
        return Err(FetchError::Network(format!(
            "ไม่พบ option contracts ของ {root}"
        )));
    }

    let mut expiries: Vec<NaiveDate> = metas
        .values()
        .map(|m| m.expiry)
        .filter(|e| *e >= today)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(MAX_EXPIRIES)
        .collect();
    expiries.sort();
    let expiry_set: HashSet<NaiveDate> = expiries.into_iter().collect();

    let mut all_strikes: Vec<f64> = metas.values().map(|m| m.strike).collect();
    all_strikes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    all_strikes.dedup_by(|a, b| a == b);
    let spot = spot.unwrap_or_else(|| all_strikes[all_strikes.len() / 2]);

    let puts: Vec<f64> = {
        let mut v: Vec<f64> = all_strikes.iter().copied().filter(|k| *k <= spot).collect();
        v.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)); // descending
        v.truncate(STRIKES_EACH_SIDE);
        v
    };
    let calls: Vec<f64> = {
        let mut v: Vec<f64> = all_strikes.iter().copied().filter(|k| *k >= spot).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); // ascending
        v.truncate(STRIKES_EACH_SIDE);
        v
    };
    let put_keep: Vec<f64> = puts;
    let call_keep: Vec<f64> = calls;

    let mut chosen: Vec<(NaiveDate, f64, &str, &String)> = metas
        .iter()
        .filter(|(_, m)| {
            expiry_set.contains(&m.expiry)
                && match m.option_type {
                    OptType::Put => put_keep.contains(&m.strike),
                    OptType::Call => call_keep.contains(&m.strike),
                }
        })
        .map(|(s, m)| (m.expiry, m.strike, m.option_type.as_str(), s))
        .collect();
    chosen.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.2.cmp(b.2))
    });
    let symbols: Vec<String> = chosen
        .into_iter()
        .take(MAX_ANALYZE)
        .map(|(_, _, _, s)| s.clone())
        .collect();
    if symbols.is_empty() {
        return Err(FetchError::Network(format!(
            "ไม่มี contract ที่ผ่านตัวกรองของ {root}"
        )));
    }
    Ok((symbols, spot))
}

// ---------- full flow (mirrors fetch_symbols) ----------

/// Spot → strike band → windowed scan (retry without band when empty) → OCC
/// collection → strike/expiry selection. Returns (symbols, spot).
pub fn fetch_symbols_flow<F: HttpFetch>(
    f: &mut F,
    root: &str,
    today: NaiveDate,
    stale: Stale<'_>,
    mut on_progress: Progress<'_>,
) -> Result<(Vec<String>, f64), FetchError> {
    // Python fetch_spot catches everything -> None; the flow then continues
    // band-less with the median-strike fallback. Degrade, never abort.
    let spot = fetch_spot(f, root).ok().flatten();
    let (gte, lte) = spot
        .map(strike_band)
        .map(|(g, l)| (Some(g), Some(l)))
        .unwrap_or((None, None));
    let mut progress = |msg: String| {
        if let Some(p) = on_progress.as_deref_mut() {
            p(msg);
        }
    };
    let mut contracts = fetch_option_contracts(
        f,
        root,
        gte.as_deref(),
        lte.as_deref(),
        None,
        60,
        Some(MAX_EXPIRIES + 1),
        today,
        stale,
        Some(&mut progress),
    )?;
    if contracts.is_empty() && gte.is_some() {
        contracts = fetch_option_contracts(
            f,
            root,
            None,
            None,
            None,
            60,
            Some(MAX_EXPIRIES + 1),
            today,
            stale,
            Some(&mut progress),
        )?;
    }
    let mut found = BTreeSet::new();
    collect_occ_symbols(&Value::Array(contracts), &mut found);
    select_symbols(root, &found, spot, today)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strike_band_format_matches_python() {
        let (g, l) = strike_band(182.45);
        assert_eq!(g, "127.71");
        assert_eq!(l, "237.19");
        let (g2, l2) = strike_band(58.12);
        assert_eq!(g2, "40.68");
        assert_eq!(l2, "75.56");
    }

    #[test]
    fn pick_ws_interleaves_ranks_across_groups() {
        let syms: Vec<String> = [
            "A261224P00180000",
            "A261224P00170000",
            "A261224C00200000",
            "A270116C00210000",
            "A261224P00175000",
            "A270116C00200000",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let picked = pick_ws_symbols(&syms, 1820.0_f64 / 10.0, 4);
        // group P 261224 by |strike-182|: 175(7) < 170(12) < 180(2)... wait: 180 is closest (2)
        // P: 180(2),175(7),170(12); C261224: 200(18); C270116: 200(18),210(28)
        // ranked interleave by rank: (0,exp,sym): P180 + C200@261224 + C200@270116 -> then rank1 P175 ...
        assert_eq!(picked.len(), 4);
        // rank-0 trio first; within equal (rank, expiry) the symbol string decides: C < P
        assert_eq!(picked[0], "A261224C00200000");
        assert_eq!(picked[1], "A261224P00180000");
        assert_eq!(picked[2], "A270116C00200000");
        assert_eq!(picked[3], "A261224P00175000");
    }

    #[test]
    fn collect_occ_walks_nested_payloads() {
        let payload = serde_json::json!([
            {"instrument_id": 1, "symbol": "NVDA261218P00175000", "derived": {"symbol": "NVDA261218C00195000"}},
            {"symbol": "NOT_A_SYMBOL"},
            [{"symbol": "BRK.B270206C00300000"}]
        ]);
        let mut out = BTreeSet::new();
        collect_occ_symbols(&payload, &mut out);
        assert_eq!(out.len(), 3);
        assert!(out.contains("NVDA261218P00175000") && out.contains("BRK.B270206C00300000"));
    }

    #[test]
    fn select_symbols_keeps_atm_both_sides_and_caps() {
        // 2 expiries × strikes 160..200 step 10 around spot 182.45
        let mut found = BTreeSet::new();
        for exp in ["2026-12-18", "2027-01-15"] {
            for k in [160.0, 170.0, 180.0, 190.0, 200.0] {
                for t in ["P", "C"] {
                    found.insert(format!(
                        "A{}{}{:08}",
                        &exp.replace('-', "")[2..],
                        t,
                        (k * 1000.0) as i64
                    ));
                }
            }
        }
        let (symbols, spot) = select_symbols(
            "A",
            &found,
            Some(182.45),
            NaiveDate::from_ymd_opt(2026, 11, 22).unwrap(),
        )
        .unwrap();
        assert!((spot - 182.45).abs() < 1e-9);
        // puts keep strikes <= 182.45 (160,170,180), calls keep >= 182.45 (190,200)
        // => 5 per expiry x 2 expiries = 10 (within the 20/side cap)
        assert_eq!(symbols.len(), 10);
        // sorted by (expiry, strike, type): first is 2026-12-18 strike 160 put
        assert!(symbols[0].starts_with("A261218P00160000"));
    }

    #[test]
    fn select_symbols_median_fallback_when_no_spot() {
        let mut found = BTreeSet::new();
        for k in [100.0, 150.0, 200.0, 250.0] {
            found.insert(format!("A261218C{:08}", (k * 1000.0) as i64));
        }
        let (_, spot) = select_symbols(
            "A",
            &found,
            None,
            NaiveDate::from_ymd_opt(2026, 11, 22).unwrap(),
        )
        .unwrap();
        assert_eq!(spot, 200.0); // sorted[4/2] = sorted[2]
    }
}
