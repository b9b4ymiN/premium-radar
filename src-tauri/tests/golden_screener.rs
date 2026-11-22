//! Golden tests for the screener pipeline (enrich / grade / filters /
//! chain-stats) against the Python reference fixtures.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

use premium_radar_lib::quant::OptType;
use premium_radar_lib::screener::{
    chain_stats, enrich_rows_raw, otm_curve, short_candidates, candidates_in_input_order,
    EnrichedRow, Filters, Mode, QuoteRow,
};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures").join(name)
}

fn load_json(name: &str) -> Value {
    let raw = std::fs::read(fixture_path(name)).unwrap_or_else(|e| panic!("read {name}: {e}"));
    serde_json::from_slice(&raw).unwrap_or_else(|e| panic!("parse {name}: {e}"))
}

// ---------- shared fixture shapes ----------

#[derive(Deserialize)]
struct MetaOnly {
    #[serde(rename = "reference_time_utc")]
    _reference_time_utc: String,
}

#[derive(Deserialize)]
struct EnrichFile {
    meta: MetaOnly,
    cases: Vec<EnrichCase>,
}
#[derive(Deserialize)]
struct EnrichCase {
    name: String,
    spot: f64,
    rows: Vec<RawRow>,
    expected: Vec<Value>,
}
#[derive(Deserialize)]
struct RawRow {
    symbol: String,
    underlying: String,
    expiry: NaiveDate,
    option_type: String,
    strike: f64,
    #[serde(default)]
    price: f64,
    #[serde(default)]
    bid: f64,
    #[serde(default)]
    ask: f64,
    #[serde(default)]
    volume: f64,
    #[serde(default)]
    oi: f64,
    #[serde(default)]
    change: f64,
    #[serde(default)]
    change_ratio: f64,
    #[serde(default)]
    src: String,
}

fn to_quote(r: &RawRow) -> QuoteRow {
    QuoteRow {
        symbol: r.symbol.clone(),
        underlying: r.underlying.clone(),
        expiry: r.expiry,
        option_type: OptType::parse(&r.option_type).expect("valid type in fixture"),
        strike: r.strike,
        price: r.price,
        bid: r.bid,
        ask: r.ask,
        volume: r.volume,
        oi: r.oi,
        change: r.change,
        change_ratio: r.change_ratio,
        src: r.src.clone(),
    }
}

fn num_close(got: f64, want: f64, key: &str, sym: &str) {
    assert!(
        (got - want).abs() < 1e-9,
        "{sym}: field {key}: got {got}, want {want}"
    );
}

fn opt_num(got: Option<f64>, want: &Value, key: &str, sym: &str) {
    match want {
        Value::Null => assert!(got.is_none(), "{sym}: {key}: expected null, got {got:?}"),
        Value::Number(n) => {
            let w = n.as_f64().expect("finite number");
            let g = got.unwrap_or_else(|| panic!("{sym}: {key}: expected {w}, got None"));
            num_close(g, w, key, sym);
        }
        _ => panic!("{sym}: {key}: unexpected fixture type"),
    }
}

fn opt_text(got: Option<&str>, want: &Value, key: &str, sym: &str) {
    match want {
        Value::Null => assert!(got.is_none(), "{sym}: {key}: expected null"),
        Value::String(s) => assert_eq!(got, Some(s.as_str()), "{sym}: {key} mismatch"),
        _ => panic!("{sym}: {key}: unexpected fixture type"),
    }
}

fn assert_row_matches(r: &EnrichedRow, want: &Value) {
    let sym = r.q.symbol.as_str();
    opt_num(Some(r.t), &want["T"], "T", sym);
    opt_num(Some(r.dte), &want["dte"], "dte", sym);
    opt_num(Some(r.chg_pct), &want["chg_pct"], "chg_pct", sym);
    opt_num(r.vol_oi, &want["vol_oi"], "vol_oi", sym);
    for (got, key) in [
        (r.iv, "iv"), (r.delta, "delta"), (r.gamma, "gamma"), (r.vega, "vega"),
        (r.theta, "theta"), (r.pop, "pop"), (r.credit, "credit"),
        (r.collateral, "collateral"), (r.yld, "yld"), (r.ann, "ann"),
        (r.be, "be"), (r.cushion, "cushion"), (r.p_itm, "p_itm"),
        (r.p_touch, "p_touch"), (r.sig_dist, "sig_dist"),
        (r.strike_dist, "strike_dist"), (r.day_inc, "day_inc"),
        (r.if_called, "if_called"), (r.if_called_ann, "if_called_ann"),
        (r.basis, "basis"), (r.score, "score"), (r.spread_pct, "spread_pct"),
        (r.prem, "prem"), (r.net_cash, "net_cash"), (r.max_loss, "max_loss"),
        (r.max_profit, "max_profit"),
    ] {
        opt_num(got, &want[key], key, sym);
    }
    opt_text(r.grade.map(|g| g.as_str()), &want["grade"], "grade", sym);
    opt_text(r.why.as_deref(), &want["why"], "why", sym);
}

// ---------- enrich_grade.json ----------

#[test]
fn golden_enrich_and_grade() {
    let f: EnrichFile =
        serde_json::from_value(load_json("enrich_grade.json")).expect("fixture shape");
    let now: DateTime<Utc> = f
        .meta
        ._reference_time_utc
        .parse()
        .expect("reference time RFC3339");
    let mut total = 0usize;
    for case in &f.cases {
        let quotes: Vec<QuoteRow> = case.rows.iter().map(to_quote).collect();
        let enriched = enrich_rows_raw(quotes, case.spot, now);
        assert_eq!(enriched.len(), case.expected.len(), "case {}", case.name);
        for (r, want) in enriched.iter().zip(&case.expected) {
            assert_row_matches(r, want);
            total += 1;
        }
    }
    assert!(total >= 300, "expected a full enrich sweep, checked {total}");
}

// ---------- screener_filters.json ----------

#[derive(Deserialize)]
struct FilterFile {
    scenarios: Vec<Scenario>,
}
#[derive(Deserialize)]
struct Scenario {
    case: String,
    #[serde(default)]
    _risk: String,
    mode: String,
    dmin: f64,
    dmax: f64,
    sel_exp: Option<NaiveDate>,
    all_exp: bool,
    relax: bool,
    budget: f64,
    expected_candidates_input_order: Vec<String>,
    expected_app_order: Vec<String>,
}

fn enriched_cases(f: &EnrichFile, now: DateTime<Utc>) -> HashMap<String, Vec<EnrichedRow>> {
    f.cases
        .iter()
        .map(|c| {
            let quotes: Vec<QuoteRow> = c.rows.iter().map(to_quote).collect();
            (c.name.clone(), enrich_rows_raw(quotes, c.spot, now))
        })
        .collect()
}

#[test]
fn golden_short_candidates_both_orders() {
    let ef: EnrichFile =
        serde_json::from_value(load_json("enrich_grade.json")).expect("fixture shape");
    let now: DateTime<Utc> = ef
        .meta
        ._reference_time_utc
        .parse()
        .expect("reference time RFC3339");
    let ff: FilterFile =
        serde_json::from_value(load_json("screener_filters.json")).expect("fixture shape");
    let cases = enriched_cases(&ef, now);

    for sc in &ff.scenarios {
        let rows = cases
            .get(&sc.case)
            .unwrap_or_else(|| panic!("unknown case {}", sc.case));
        let refs: Vec<&EnrichedRow> = rows.iter().collect();
        let spot = ef
            .cases
            .iter()
            .find(|c| c.name == sc.case)
            .expect("case spot")
            .spot;
        let filters = Filters {
            mode: Mode::parse(&sc.mode).expect("valid mode"),
            sel_exp: sc.sel_exp,
            all_exp: sc.all_exp,
            dmin: sc.dmin,
            dmax: sc.dmax,
            relax: sc.relax,
            budget: sc.budget,
        };
        let input_order: Vec<String> =
            candidates_in_input_order(&refs, spot, &filters).iter().map(|r| r.q.symbol.clone()).collect();
        assert_eq!(
            input_order, sc.expected_candidates_input_order,
            "scenario {} input order",
            sc.case
        );
        let app_order: Vec<String> =
            short_candidates(&refs, spot, &filters).iter().map(|r| r.q.symbol.clone()).collect();
        assert_eq!(app_order, sc.expected_app_order, "scenario {} app order", sc.case);
    }
}

// ---------- chain_stats.json ----------

#[derive(Deserialize)]
struct ChainFile {
    scenarios: Vec<ChainScenario>,
}
#[derive(Deserialize)]
struct ChainScenario {
    case: String,
    spot: f64,
    sel_exp: NaiveDate,
    expected: Value,
    #[serde(default)]
    _atm_iv_for: Value,
    #[serde(default)]
    _otm_curve: Vec<(f64, f64)>,
}

#[test]
fn golden_chain_stats() {
    let ef: EnrichFile =
        serde_json::from_value(load_json("enrich_grade.json")).expect("fixture shape");
    let now: DateTime<Utc> = ef
        .meta
        ._reference_time_utc
        .parse()
        .expect("reference time RFC3339");
    let cf: ChainFile =
        serde_json::from_value(load_json("chain_stats.json")).expect("fixture shape");
    let cases = enriched_cases(&ef, now);

    for sc in &cf.scenarios {
        let rows = cases
            .get(&sc.case)
            .unwrap_or_else(|| panic!("unknown case {}", sc.case));
        let refs: Vec<&EnrichedRow> = rows.iter().collect();
        let stats = chain_stats(&refs, sc.spot, sc.sel_exp);
        let label = format!("case {} exp {}", sc.case, sc.sel_exp);
        let want = &sc.expected;
        match (&stats.atm_iv, &want["atm_iv"]) {
            (Some(a), Value::Number(n)) => {
                num_close(*a, n.as_f64().unwrap(), "atm_iv", &label)
            }
            (None, Value::Null) => {}
            other => panic!("{label}: atm_iv mismatch {other:?}"),
        }
        match (&stats.em, &want["em"]) {
            (Some(a), Value::Number(n)) => num_close(*a, n.as_f64().unwrap(), "em", &label),
            (None, Value::Null) => {}
            other => panic!("{label}: em mismatch {other:?}"),
        }
        match (&stats.skew, &want["skew"]) {
            (Some(a), Value::Number(n)) => num_close(*a, n.as_f64().unwrap(), "skew", &label),
            (None, Value::Null) => {}
            other => panic!("{label}: skew mismatch {other:?}"),
        }
        // otm_curve recorded alongside: compare when present
        if let Value::Array(want_curve) = &sc._otm_curve {
            let got = otm_curve(&refs, sc.spot);
            assert_eq!(got.len(), want_curve.len(), "{label}: otm_curve length");
            for (g, w) in got.iter().zip(want_curve.iter()) {
                let arr = w.as_array().expect("pair");
                num_close(g.0, arr[0].as_f64().unwrap(), "otm strike", &label);
                num_close(g.1, arr[1].as_f64().unwrap(), "otm iv", &label);
            }
        }
    }
}
