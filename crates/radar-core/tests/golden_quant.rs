//! Golden tests against the Python reference, pinned in `tests/fixtures/`
//! (repo root). Regenerate with `python tools/dump_fixtures.py`.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use std::path::PathBuf;

use radar_core::quant::{OptType, bs_greeks, bs_price, implied_vol, years_to_expiry};
use radar_core::webull::contracts::parse_occ;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn load<T: for<'de> Deserialize<'de>>(name: &str) -> T {
    let raw =
        std::fs::read(fixture_path(name)).unwrap_or_else(|e| panic!("read fixture {name}: {e}"));
    serde_json::from_str(std::str::from_utf8(&raw).unwrap())
        .unwrap_or_else(|e| panic!("parse fixture {name}: {e}"))
}

fn cp(s: &str) -> OptType {
    OptType::parse(s).unwrap_or_else(|| panic!("bad cp token {s}"))
}

const TOL: f64 = 1e-6;

// ---------- quant_bs.json ----------

#[derive(Deserialize)]
struct BsFile {
    cases: Vec<BsCase>,
}
#[derive(Deserialize)]
struct BsCase {
    cp: String,
    #[serde(rename = "S")]
    s: f64,
    #[serde(rename = "K")]
    k: f64,
    #[serde(rename = "T")]
    t: f64,
    r: f64,
    q: f64,
    vol: f64,
    expected: BsExpected,
    #[serde(rename = "tag", default)]
    _tag: String,
}
#[derive(Deserialize)]
struct BsExpected {
    price: f64,
    // greeks are absent on degenerate cases (Python skips div-zero inputs)
    #[serde(default)]
    delta: Option<f64>,
    #[serde(default)]
    gamma: Option<f64>,
    #[serde(default)]
    vega: Option<f64>,
    #[serde(default)]
    theta: Option<f64>,
}

#[test]
fn golden_bs_price_and_greeks() {
    let f: BsFile = load("quant_bs.json");
    assert!(
        f.cases.len() >= 100,
        "expected a full grid, got {}",
        f.cases.len()
    );
    for (i, c) in f.cases.iter().enumerate() {
        let opt = cp(&c.cp);
        let price = bs_price(opt, c.s, c.k, c.t, c.r, c.q, c.vol);
        assert!(
            (price - c.expected.price).abs() < TOL,
            "case {i} ({}) price: got {price}, want {}",
            c._tag,
            c.expected.price
        );
        let g = bs_greeks(opt, c.s, c.k, c.t, c.r, c.q, c.vol);
        if let Some(w) = c.expected.delta {
            assert!(
                (g.delta - w).abs() < TOL,
                "case {i} ({}) delta: {} vs {w}",
                c._tag,
                g.delta
            );
        }
        if let Some(w) = c.expected.gamma {
            assert!((g.gamma - w).abs() < TOL, "case {i} gamma");
        }
        if let Some(w) = c.expected.vega {
            assert!((g.vega - w).abs() < TOL, "case {i} vega");
        }
        if let Some(w) = c.expected.theta {
            assert!((g.theta - w).abs() < TOL, "case {i} theta");
        }
    }
}

// ---------- quant_iv.json ----------

#[derive(Deserialize)]
struct IvFile {
    cases: Vec<IvCase>,
}
#[derive(Deserialize)]
struct IvCase {
    cp: String,
    #[serde(rename = "S")]
    s: f64,
    #[serde(rename = "K")]
    k: f64,
    #[serde(rename = "T")]
    t: f64,
    r: f64,
    q: f64,
    market_price: f64,
    expected_iv: Option<f64>,
}

#[test]
fn golden_implied_vol() {
    let f: IvFile = load("quant_iv.json");
    let mut solved = 0;
    for (i, c) in f.cases.iter().enumerate() {
        let got = implied_vol(cp(&c.cp), c.market_price, c.s, c.k, c.t, c.r, c.q);
        match (got, c.expected_iv) {
            (Some(g), Some(w)) => {
                assert!((g - w).abs() < TOL, "case {i} iv: got {g}, want {w}");
                solved += 1;
            }
            (None, None) => {}
            (g, w) => panic!("case {i} solvability mismatch: got {g:?}, want {w:?}"),
        }
    }
    assert!(
        solved >= 100,
        "expected a meaningful solved set, got {solved}"
    );
}

// ---------- occ_parse.json ----------

#[derive(Deserialize)]
struct OccFile {
    meta: OccMeta_,
    cases: Vec<OccCase>,
}
#[derive(Deserialize)]
struct OccMeta_ {
    reference_time_utc: DateTime<Utc>,
}
#[derive(Deserialize)]
struct OccCase {
    symbol: String,
    expected: Option<OccExpected>,
}
#[derive(Deserialize, Debug)]
struct OccExpected {
    underlying: String,
    option_type: String,
    strike: f64,
    expiry_date: NaiveDate,
    t_years: f64,
    #[serde(default)]
    _dte: Option<f64>,
}

#[test]
fn golden_occ_parse_and_years() {
    let f: OccFile = load("occ_parse.json");
    let now = f.meta.reference_time_utc;
    let (mut valid, mut invalid) = (0, 0);
    for (i, c) in f.cases.iter().enumerate() {
        match (parse_occ(&c.symbol), &c.expected) {
            (Some(m), Some(w)) => {
                assert_eq!(m.underlying, w.underlying, "case {i} underlying");
                assert_eq!(m.option_type.as_str(), w.option_type, "case {i} type");
                assert_eq!(m.strike, w.strike, "case {i} strike");
                assert_eq!(m.expiry, w.expiry_date, "case {i} expiry");
                let t = years_to_expiry(m.expiry, now);
                assert!(
                    (t - w.t_years).abs() < 1e-9,
                    "case {i} t_years: got {t}, want {}",
                    w.t_years
                );
                valid += 1;
            }
            (None, None) => invalid += 1,
            (g, w) => panic!("case {i} parse mismatch: got {g:?}, want {w:?}"),
        }
    }
    assert!(
        valid >= 20 && invalid >= 3,
        "valid={valid} invalid={invalid}"
    );
}
