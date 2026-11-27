//! Screener tools for the agent registry. `top_candidates` is no longer an
//! agent tool (the agent's `scan_chain` in the ideas family replaces it —
//! it applies the filters and returns the same formatter's rows); the
//! legacy copilot chat keeps `copilot::screener_tools()` until P4.

use serde_json::{Map, Value, json};

use crate::agent::contract::{Mode, Tier, ToolSpec};
use crate::quant::OptType;
use crate::screener::{ChainStats, EnrichedRow};

pub fn screener_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "contract_detail".into(),
            tier: Tier::Read,
            modes: vec![Mode::Easy, Mode::Pro],
            description: "ดูรายละเอียดครบทุกด้านของสัญญาหนึ่งตัวจากรหัสสัญญา (OCC symbol) เช่น จุดคุ้มทุน ระยะห่างจากราคาหุ้นปัจจุบัน และเกรดของสัญญา\nใช้เมื่อ: ผู้ใช้ถามเจาะลึกสัญญาที่ระบุชัดเจน หรืออยากได้ตัวเลขที่ไม่ได้อยู่ในรายการสรุปของ find_ideas / scan_chain\nไม่ใช้เมื่อ: ยังไม่รู้ว่าสนใจสัญญาไหน — เรียก find_ideas หรือ scan_chain เพื่อหาสัญญาที่น่าสนใจก่อน".into(),
            examples: vec![json!({"symbol": "NVDA261218P00175000"})],
            parameters: json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "รหัสสัญญา (OCC symbol) เช่น NVDA261218P00175000"
                    }
                },
                "required": ["symbol"],
                "additionalProperties": false
            }),
            timeout_ms: 5000,
            webull_cost: 0,
        },
        ToolSpec {
            name: "chain_stats".into(),
            tier: Tier::Read,
            modes: vec![Mode::Pro],
            description: "ดูภาพรวมของหุ้นที่แอปกำลังเปิดดูอยู่ทั้งตัว: ความผันผวนที่ตลาดคาด ช่วงราคาที่คาดว่าจะเคลื่อนไหว และการเทียบสัดส่วนฝั่งพุตกับฝั่งคอล\nใช้เมื่อ: ผู้ใช้อยากเข้าใจบรรยากาศตลาดของหุ้นนี้ก่อนลงรายละเอียดสัญญา\nไม่ใช้เมื่อ: อยากได้ข้อมูลระดับสัญญา (ใช้ scan_chain หรือ contract_detail)".into(),
            examples: vec![json!({})],
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            timeout_ms: 5000,
            webull_cost: 0,
        },
    ]
}

// ---------- compact results (W6 P2-T4) ----------
// Same convention as the ideas/market summarisers: every number the model
// could ask about is pre-converted to its user-facing unit here, so the
// model never multiplies or divides.

/// Round to 2 decimals; a whole number collapses to an integer.
fn num2(v: f64) -> Value {
    if !v.is_finite() {
        return Value::Null;
    }
    let r = (v * 100.0).round() / 100.0;
    if r.fract().abs() < 1e-9 {
        json!(r as i64)
    } else {
        json!(r)
    }
}

/// Greeks keep 4 decimals.
fn num4(v: f64) -> Value {
    if !v.is_finite() {
        return Value::Null;
    }
    json!((v * 10_000.0).round() / 10_000.0)
}

fn positive(spot: Option<f64>) -> Option<f64> {
    spot.filter(|s| s.is_finite() && *s > 0.0)
}

/// `contract_detail` result. `EnrichedRow` mixes per-share (`prem`, `theta`)
/// and per-contract (`credit`, `collateral`) values with fraction
/// probabilities; this emits one unit per key — `premium_usd` is per
/// contract, `theta_usd_per_day` is per contract (the row's theta is per
/// share per day — see `day_inc = |theta| × 100` in the enrichment), and
/// every `_pct` is already a percentage. Nulls are omitted.
pub fn summarize_contract(row: &EnrichedRow, spot: Option<f64>, as_of: &str) -> Value {
    let q = &row.q;
    let mut o = Map::new();
    o.insert("ref".into(), json!(q.symbol));
    o.insert("symbol".into(), json!(q.underlying));
    o.insert(
        "side".into(),
        json!(if q.option_type == OptType::Put {
            "put"
        } else {
            "call"
        }),
    );
    o.insert("strike".into(), num2(q.strike));
    o.insert("expiry".into(), json!(q.expiry));
    o.insert("days".into(), num2(row.dte));
    if let Some(s) = positive(spot) {
        o.insert("spot".into(), num2(s));
    }
    if let Some(p) = row.prem {
        o.insert("premium_per_share_usd".into(), num2(p));
    }
    // credit is per contract (prem × 100); derive it when only prem exists
    if let Some(c) = row.credit.or_else(|| row.prem.map(|p| p * 100.0)) {
        o.insert("premium_usd".into(), num2(c));
    }
    if let Some(c) = row.collateral {
        o.insert("cash_needed_usd".into(), num2(c));
    }
    if let Some(be) = row.be {
        o.insert("breakeven".into(), num2(be));
    }
    if let Some(c) = row.cushion {
        o.insert("cushion_pct".into(), num2(c));
    }
    if let Some(p) = row.pop {
        o.insert("pop_pct".into(), num2(p * 100.0));
    }
    if let Some(p) = row.p_itm {
        o.insert("p_itm_pct".into(), num2(p * 100.0));
    }
    if let Some(p) = row.p_touch {
        o.insert("p_touch_pct".into(), num2(p * 100.0));
    }
    if let Some(a) = row.ann {
        o.insert("annual_pct".into(), num2(a));
    }
    if let Some(y) = row.yld {
        o.insert("period_pct".into(), num2(y));
    }
    if let Some(iv) = row.iv {
        o.insert("iv_pct".into(), num2(iv * 100.0));
    }
    if let Some(d) = row.delta {
        o.insert("delta".into(), num4(d));
    }
    if let Some(t) = row.theta {
        o.insert("theta_usd_per_day".into(), num2(t * 100.0));
    }
    if let Some(v) = row.vol_oi {
        o.insert("vol_oi".into(), num2(v));
    }
    o.insert("open_interest".into(), num2(q.oi));
    o.insert("volume".into(), num2(q.volume));
    if let Some(s) = row.spread_pct {
        o.insert("spread_pct".into(), num2(s));
    }
    if let Some(g) = row.grade {
        o.insert("grade".into(), json!(g.as_str()));
    }
    o.insert("as_of".into(), json!(as_of));
    o.insert("source".into(), json!("engine"));
    Value::Object(o)
}

/// `chain_stats` result. `ChainStats.atm_iv` is a fraction and `em` is the
/// ±1σ move in USD; `skew` is already put-IV% minus call-IV% (positive =
/// puts pricier). The engine snapshot only keeps stats whose ATM IV
/// solved, so an all-`None` struct never reaches this summariser.
pub fn summarize_chain_stats(
    stats: &ChainStats,
    spot: Option<f64>,
    symbol: &str,
    as_of: &str,
) -> Value {
    let mut o = Map::new();
    o.insert("symbol".into(), json!(symbol));
    if let Some(s) = positive(spot) {
        o.insert("spot".into(), num2(s));
    }
    if let Some(iv) = stats.atm_iv {
        o.insert("atm_iv_pct".into(), num2(iv * 100.0));
    }
    if let Some(em) = stats.em {
        o.insert("expected_move_usd".into(), num2(em));
        if let Some(s) = positive(spot) {
            o.insert("expected_move_pct".into(), num2(em / s * 100.0));
        }
    }
    if let Some(sk) = stats.skew {
        o.insert("skew".into(), num2(sk));
    }
    o.insert("as_of".into(), json!(as_of));
    o.insert("source".into(), json!("engine"));
    Value::Object(o)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::contract::ToolErrorCode;
    use crate::agent::registry::Registry;
    use crate::screener::{Grade, QuoteRow};

    #[test]
    fn two_specs_whose_names_stay_within_the_copilot_screener_set() {
        let specs = screener_specs();
        let spec_names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(spec_names, vec!["contract_detail", "chain_stats"]);
        // top_candidates stays a copilot-only tool; the agent set is a subset
        let copilot_names: Vec<String> = crate::copilot::screener_tools()
            .iter()
            .map(|t| t.function.name.clone())
            .collect();
        for name in &spec_names {
            assert!(
                copilot_names.iter().any(|c| c == name),
                "{name} must exist in the copilot set"
            );
        }
        assert!(copilot_names.iter().any(|c| c == "top_candidates"));
    }

    #[test]
    fn specs_follow_the_tool_conventions() {
        for spec in screener_specs() {
            assert_eq!(spec.tier, Tier::Read);
            assert_eq!(spec.timeout_ms, 5000);
            assert_eq!(spec.webull_cost, 0);
            assert!(!spec.examples.is_empty(), "{} needs examples", spec.name);
            assert!(
                spec.description.contains("ใช้เมื่อ:"),
                "{}: {}",
                spec.name,
                spec.description
            );
            assert!(
                spec.description.contains("ไม่ใช้เมื่อ:"),
                "{}: {}",
                spec.name,
                spec.description
            );
            assert_eq!(
                spec.parameters.get("additionalProperties"),
                Some(&json!(false)),
                "{} must lock its schema",
                spec.name
            );
            assert!(
                spec.to_tool_def()
                    .function
                    .description
                    .contains("ตัวอย่างการเรียก:")
            );
        }
    }

    #[test]
    fn modes_are_per_tool_and_examples_validate_in_their_own_modes() {
        let by_name = |n: &str| screener_specs().into_iter().find(|s| s.name == n).unwrap();
        assert_eq!(
            by_name("contract_detail").modes,
            vec![Mode::Easy, Mode::Pro]
        );
        assert_eq!(by_name("chain_stats").modes, vec![Mode::Pro]);

        let mut reg = Registry::new();
        for spec in screener_specs() {
            reg.register(spec.clone()).expect("register screener spec");
        }
        for spec in screener_specs() {
            for mode in spec.modes.clone() {
                for ex in &spec.examples {
                    let args = ex.to_string();
                    let validated = reg.validate(&spec.name, mode, &args).unwrap_or_else(|e| {
                        panic!("example {args} of {} failed: {}", spec.name, e.message)
                    });
                    assert!(validated.is_object());
                }
            }
        }
        // chain_stats is hidden from Easy, not just unlisted
        let err = reg.validate("chain_stats", Mode::Easy, "{}").unwrap_err();
        assert_eq!(err.code, ToolErrorCode::NotInMode);
    }

    // ---------- compact results ----------

    fn quote() -> QuoteRow {
        QuoteRow {
            symbol: "NVDA261218P00175000".into(),
            underlying: "NVDA".into(),
            expiry: chrono::NaiveDate::from_ymd_opt(2026, 12, 18).expect("valid date"),
            option_type: OptType::Put,
            strike: 175.0,
            price: 1.85,
            bid: 1.85,
            ask: 1.95,
            volume: 1200.0,
            oi: 4800.0,
            change: 0.1,
            change_ratio: 0.05,
            src: "REST".into(),
        }
    }

    /// A row where every optional field is set to a value whose unit the
    /// tests can pin down (fractions vs percentages, per share vs per
    /// contract).
    fn full_row() -> EnrichedRow {
        let mut r = bare_row();
        r.vol_oi = Some(0.25); // volume 1200 / oi 4800
        r.iv = Some(0.482); // fraction
        r.delta = Some(-0.1523);
        r.theta = Some(-0.0523); // per share per day
        r.pop = Some(0.72); // fraction
        r.credit = Some(185.0); // per contract
        r.collateral = Some(17_500.0); // per contract
        r.yld = Some(1.06); // already %
        r.ann = Some(18.4); // already %
        r.be = Some(173.15);
        r.cushion = Some(8.65); // already %
        r.p_itm = Some(0.18); // fraction
        r.p_touch = Some(0.35); // fraction
        r.spread_pct = Some(5.41); // already %
        r.prem = Some(1.85); // per share
        r.grade = Some(Grade::A);
        r
    }

    /// Only the non-optional economics; every `Option` stays `None`.
    fn bare_row() -> EnrichedRow {
        let q = quote();
        let dte = 21.0;
        EnrichedRow {
            t: dte / 365.0,
            dte,
            chg_pct: 5.0,
            q,
            vol_oi: None,
            iv: None,
            delta: None,
            gamma: None,
            vega: None,
            theta: None,
            pop: None,
            credit: None,
            collateral: None,
            yld: None,
            ann: None,
            be: None,
            cushion: None,
            p_itm: None,
            p_touch: None,
            sig_dist: None,
            strike_dist: None,
            day_inc: None,
            if_called: None,
            if_called_ann: None,
            basis: None,
            score: None,
            spread_pct: None,
            prem: None,
            net_cash: None,
            max_loss: None,
            max_profit: None,
            grade: None,
            why: None,
        }
    }

    const AS_OF: &str = "2026-11-27T10:00:00+00:00";

    #[test]
    fn contract_result_converts_every_unit() {
        let out = summarize_contract(&full_row(), Some(189.3), AS_OF);
        assert_eq!(out["ref"], "NVDA261218P00175000");
        assert_eq!(out["symbol"], "NVDA");
        assert_eq!(out["side"], "put");
        assert_eq!(out["expiry"], "2026-12-18");
        // per share vs per contract stay distinct
        assert_eq!(out["premium_per_share_usd"], json!(1.85));
        assert_eq!(out["premium_usd"], json!(185));
        assert_eq!(out["cash_needed_usd"], json!(17500));
        // fractions become percentages
        assert_eq!(out["pop_pct"], json!(72));
        assert_eq!(out["p_itm_pct"], json!(18));
        assert_eq!(out["p_touch_pct"], json!(35));
        assert_eq!(out["iv_pct"], json!(48.2));
        // already-percent fields pass through unchanged
        assert_eq!(out["period_pct"], json!(1.06));
        assert_eq!(out["annual_pct"], json!(18.4));
        assert_eq!(out["cushion_pct"], json!(8.65));
        assert_eq!(out["spread_pct"], json!(5.41));
        // theta is per share per day → per contract per day
        assert_eq!(out["theta_usd_per_day"], json!(-5.23));
        // greeks keep 4 decimals
        assert_eq!(out["delta"], json!(-0.1523));
        assert_eq!(out["vol_oi"], json!(0.25));
        assert_eq!(out["open_interest"], json!(4800));
        assert_eq!(out["volume"], json!(1200));
        assert_eq!(out["grade"], "A");
        assert_eq!(out["spot"], json!(189.3));
        assert_eq!(out["as_of"], AS_OF);
        assert_eq!(out["source"], "engine");
    }

    #[test]
    fn contract_result_omits_nulls_and_keeps_the_always_keys() {
        let out = summarize_contract(&bare_row(), None, AS_OF);
        for absent in [
            "spot",
            "premium_per_share_usd",
            "premium_usd",
            "cash_needed_usd",
            "breakeven",
            "cushion_pct",
            "pop_pct",
            "p_itm_pct",
            "p_touch_pct",
            "annual_pct",
            "period_pct",
            "iv_pct",
            "delta",
            "theta_usd_per_day",
            "vol_oi",
            "spread_pct",
            "grade",
        ] {
            assert!(out.get(absent).is_none(), "{absent} must be omitted");
        }
        // identity + quote fields the model can always anchor on
        for present in [
            "ref",
            "symbol",
            "side",
            "strike",
            "expiry",
            "days",
            "open_interest",
            "volume",
            "as_of",
            "source",
        ] {
            assert!(out.get(present).is_some(), "{present} must always be there");
        }
    }

    #[test]
    fn contract_result_stays_under_1500_bytes() {
        let out = summarize_contract(&full_row(), Some(189.3), AS_OF);
        let size = serde_json::to_string(&out).unwrap().len();
        assert!(size <= 1500, "contract_detail result is {size} bytes");
    }

    #[test]
    fn chain_stats_result_converts_every_unit() {
        let stats = ChainStats {
            atm_iv: Some(0.45),
            em: Some(8.9),
            skew: Some(3.2),
        };
        let out = summarize_chain_stats(&stats, Some(178.0), "NVDA", AS_OF);
        assert_eq!(out["symbol"], "NVDA");
        assert_eq!(out["spot"], json!(178));
        assert_eq!(out["atm_iv_pct"], json!(45));
        assert_eq!(out["expected_move_usd"], json!(8.9));
        assert_eq!(out["expected_move_pct"], json!(5));
        assert_eq!(out["skew"], json!(3.2));
        assert_eq!(out["as_of"], AS_OF);
        assert_eq!(out["source"], "engine");
    }

    #[test]
    fn chain_stats_result_omits_nulls_and_missing_spot() {
        let out = summarize_chain_stats(&ChainStats::default(), None, "NVDA", AS_OF);
        // only the always-keys survive an empty stats struct
        assert_eq!(
            serde_json::to_string(&out).unwrap(),
            r#"{"as_of":"2026-11-27T10:00:00+00:00","source":"engine","symbol":"NVDA"}"#
        );
        // em without a spot still lands, its _pct does not
        let out = summarize_chain_stats(
            &ChainStats {
                em: Some(8.9),
                ..ChainStats::default()
            },
            None,
            "NVDA",
            AS_OF,
        );
        assert!(out.get("expected_move_usd").is_some());
        assert!(out.get("expected_move_pct").is_none());
    }
}
