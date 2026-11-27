//! Agent tools — `ideas` family (W6 P2): find_ideas, scan_chain, what_if.
//! Specs + pure summarisers; the Tauri crate fetches data and calls the
//! summarisers. Every number a user could ask about is precomputed here so
//! the model never has to do arithmetic.

use serde_json::{Map, Value, json};

use crate::agent::contract::{Mode, Tier, ToolSpec};
use crate::easy::{EasyIdea, EasyIdeas};
use crate::easy_rank::UniverseIdeas;
use crate::lab::payoff::{CostInputs, CoveredCall, ShortLeg, cc_pnl_at, leg_pnl_at};

pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "find_ideas".into(),
            tier: Tier::Read,
            modes: vec![Mode::Easy, Mode::Pro],
            description: "หาไอเดียขายพรีเมียม (ขายพุตโดยกเงินสดครบมูลค่าหุ้นไว้) ให้ผู้ใช้ — ไม่ระบุ symbol = จัดอันดับจากหุ้นทั้งจักรวาลของแอปด้วยโปรไฟล์ที่ผู้ใช้ตั้งไว้, ระบุ symbol = ไอเดียของหุ้นตัวนั้น (เครื่องมือจะรอให้แอปสแกนข้อมูลจนพร้อม อาจใช้เวลาถึง 90 วินาที)\nใช้เมื่อ: ผู้ใช้ขอไอเดีย/ข้อเสนอแนะแบบรวม ถามว่าคืนนี้หุ้นตัวไหนน่าเข้า หรืออยากได้ไอเดียของหุ้นใดหุ้นหนึ่ง\nไม่ใช้เมื่อ: อยากกรองสัญญาเองตามเงื่อนไขระดับโปร (ใช้ scan_chain) หรืออยากรู้ผลลัพธ์ถ้าราคาหุ้นไปอยู่ราคาหนึ่งตอนหมดอายุ (ใช้ what_if)".into(),
            examples: vec![
                json!({}),
                json!({"symbol": "NVDA"}),
                json!({"symbol": "NVDA", "budget_usd": 5000}),
            ],
            parameters: json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "ticker ของหุ้น เช่น NVDA — ไม่ระบุ = จัดอันดับทั้งจักรวาลจากข้อมูลที่บันทึกไว้ล่าสุด"
                    },
                    "budget_usd": {
                        "type": "number",
                        "minimum": 0,
                        "description": "เงินที่พร้อมวางไว้ต่อสัญญา (USD) — ถ้าระบุ จะใช้แทนเงินในโปรไฟล์เฉพาะการเรียกครั้งนี้ ไม่มีการบันทึก"
                    }
                },
                "additionalProperties": false
            }),
            // no symbol: recorder read + paced Webull enrichment of the
            // shortlist (≤ 15 × 2 calls); with symbol: a full scan + 1 bars
            // call — 60 covers the worst path to a READY chain.
            timeout_ms: 95_000,
            webull_cost: 60,
        },
        ToolSpec {
            name: "scan_chain".into(),
            tier: Tier::Read,
            modes: vec![Mode::Pro],
            description: "สแกนตาราง option ของหุ้นหนึ่งตัวด้วยตัวคัดกรองของโหมดโปร (กลยุทธ์ ระดับความเสี่ยง งบต่อสัญญา) แล้วคืนสัญญาที่น่าสนใจที่สุด N ตัวพร้อมตัวเลขสรุปของหุ้น — การเรียกนี้เปลี่ยนตัวคัดกรองและหน้าจอหลักของแอปให้แสดงผลตามเงื่อนไขเดียวกัน (ตั้งใจให้เป็นเช่นนั้น เพราะเอเจนต์ขับหน้าจอ) และจะรอให้ข้อมูลพร้อมก่อน อาจใช้เวลาถึง 90 วินาที\nใช้เมื่อ: ผู้ใช้อยากกรองสัญญาของหุ้นตัวเฉพาะตามเงื่อนไขที่ตัวเองกำหนด\nไม่ใช้เมื่อ: ผู้ใช้อยากได้ไอเดียสำเร็จรูปภาษาคนทั่วไป (ใช้ find_ideas) หรืออยากดูภาพรวมความผันผวนของหุ้นที่สแกนอยู่แล้วโดยไม่เปลี่ยนตัวคัดกรอง (ใช้ chain_stats)".into(),
            examples: vec![
                json!({"symbol": "NVDA"}),
                json!({"symbol": "NVDA", "strategy": "CC", "risk": "SAFE", "count": 3}),
                json!({"symbol": "AAPL", "expiry": "2026-12-18", "budget_usd": 5000}),
            ],
            parameters: json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "ticker ของหุ้นที่จะสแกน เช่น NVDA"
                    },
                    "strategy": {
                        "type": "string",
                        "enum": ["CSP", "CC", "BOTH"],
                        "description": "กลยุทธ์: CSP = ขายพุต, CC = ขายคอลที่มีหุ้นครอบคลุม, BOTH = ทั้งสอง",
                        "default": "CSP"
                    },
                    "risk": {
                        "type": "string",
                        "enum": ["SAFE", "BALANCED", "AGGR", "ALL"],
                        "description": "ระดับความเสี่ยง (กรองช่วง delta ของสัญญา) — ALL = ไม่จำกัด",
                        "default": "BALANCED"
                    },
                    "expiry": {
                        "type": "string",
                        "description": "วันหมดอายุของสัญญา รูปแบบ YYYY-MM-DD เช่น 2026-12-18 — ไม่ระบุ = ทุกวันหมดอายุ"
                    },
                    "budget_usd": {
                        "type": "number",
                        "minimum": 0,
                        "description": "งบต่อสัญญา (USD) — 0 (ค่าตั้งต้น) = ไม่จำกัดงบ",
                        "default": 0
                    },
                    "count": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 10,
                        "description": "จำนวนสัญญาที่ต้องการดู",
                        "default": 5
                    }
                },
                "required": ["symbol"],
                "additionalProperties": false
            }),
            // a full chain scan (discovery + seeded quotes + refresh passes)
            // on the way to READY
            timeout_ms: 95_000,
            webull_cost: 60,
        },
        ToolSpec {
            name: "what_if".into(),
            tier: Tier::Read,
            modes: vec![Mode::Easy, Mode::Pro],
            description: "ตอบคำถามแบบ \"ถ้าหุ้นอยู่ที่ราคา X ตอนหมดอายุ จะได้หรือเสียเท่าไร\" ของสัญญาหนึ่งตัวที่ระบุด้วยรหัส OCC — พร้อมจุดคุ้มทุน กำไร/ขาดทุนสูงสุด โอกาสได้กำไร โอกาสต้องรับหุ้น และตารางผลลัพธ์ตามราคาหุ้น\nใช้เมื่อ: ผู้ใช้ถามสถานการณ์สมมติของราคาหุ้น หรืออยากเข้าใจความเสี่ยงของสัญญาที่กำลังพิจารณา\nไม่ใช้เมื่อ: ยังไม่มีรหัสสัญญา — เรียก find_ideas หรือ scan_chain ก่อน; หรืออยากดูราคาสดของสัญญา (ใช้ contract_detail)".into(),
            examples: vec![
                json!({"ref": "NVDA261218P00175000"}),
                json!({"ref": "NVDA261218P00175000", "price_at_expiry_usd": 160}),
                json!({"ref": "AAPL261218C00250000", "covered": true, "basis_usd": 240.5}),
            ],
            parameters: json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "รหัสสัญญา (OCC symbol) เช่น NVDA261218P00175000 — ได้จาก find_ideas หรือ scan_chain"
                    },
                    "price_at_expiry_usd": {
                        "type": "number",
                        "minimum": 0.01,
                        "description": "ราคาหุ้นที่สมมติตอนหมดอายุ (USD) — ไม่ระบุ = ดูแค่ภาพรวมและตารางสถานการณ์"
                    },
                    "covered": {
                        "type": "boolean",
                        "description": "true = วิเคราะห์แบบถือหุ้นคู่กับการขายคอล (covered call) — ต้องระบุ basis_usd ด้วย",
                        "default": false
                    },
                    "basis_usd": {
                        "type": "number",
                        "minimum": 0,
                        "description": "ราคาเฉลี่ยที่ซื้อหุ้นไว้ต่อหุ้น (USD) — ใช้เมื่อ covered=true"
                    }
                },
                "required": ["ref"],
                "additionalProperties": false
            }),
            // reads the already-scanned engine snapshot only — the ref comes
            // from find_ideas / scan_chain, which did the fetching
            timeout_ms: 5_000,
            webull_cost: 0,
        },
    ]
}

// ---------- shared number formatting ----------

/// Round to 2 decimals; a whole number is emitted as an integer so five
/// ideas still fit the ~1.5 KB result budget.
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

/// Percentages round to 1 decimal — plenty for the model to reason with,
/// and it keeps a five-idea answer inside the result budget.
fn pct1(v: f64) -> Value {
    if !v.is_finite() {
        return Value::Null;
    }
    let r = (v * 10.0).round() / 10.0;
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

// ---------- find_ideas ----------

/// One sell-premium idea in the compact shape the model reads. False flags
/// are omitted (warn/cash_short/recommended only appear when true) and the
/// symbol lives at the top level (single symbol) or on the universe pick,
/// so five ideas still fit the result budget.
fn idea_json(i: &EasyIdea) -> Value {
    let mut o = Map::new();
    o.insert("ref".into(), json!(i.occ));
    o.insert("level".into(), json!(i.level.as_str()));
    if i.recommended {
        o.insert("recommended".into(), json!(true));
    }
    o.insert("strike".into(), num2(i.strike));
    o.insert("expiry".into(), json!(i.expiry));
    o.insert("days".into(), num2(i.days));
    o.insert("premium_usd".into(), num2(i.credit));
    o.insert("cash_needed_usd".into(), num2(i.cost));
    o.insert("period_pct".into(), pct1(i.period_pct));
    if let Some(a) = i.annual_pct {
        o.insert("annual_pct".into(), pct1(a));
    }
    o.insert("breakeven".into(), num2(i.breakeven));
    o.insert("below_spot_pct".into(), pct1(i.below_spot_pct));
    o.insert("assign_pct".into(), pct1(i.assign_pct));
    o.insert("worst_30_usd".into(), num2(i.worst_30_usd));
    o.insert("max_loss_usd".into(), num2(i.max_loss_usd));
    if i.warn {
        o.insert("warn".into(), json!(true));
    }
    if i.cash_short {
        o.insert("cash_short".into(), json!(true));
    }
    Value::Object(o)
}

/// `find_ideas` for one scanned symbol: at most 5 of the engine-computed
/// ideas plus the why-now numbers. An empty idea list becomes an explicit
/// Thai note (never an error) so the model can explain and move on.
pub fn summarize_easy_ideas(ideas: &EasyIdeas) -> Value {
    let mut o = Map::new();
    o.insert("as_of".into(), json!(ideas.computed_at));
    o.insert("source".into(), json!("engine"));
    o.insert("symbol".into(), json!(ideas.symbol));
    o.insert("spot".into(), num2(ideas.spot));
    if let Some(iv) = ideas.why_now.iv_pct {
        o.insert("iv_pct".into(), num2(iv));
    }
    if let Some(rv) = ideas.why_now.realized_30d_pct {
        o.insert("realized_30d_pct".into(), num2(rv));
    }
    let list: Vec<Value> = ideas.ideas.iter().take(5).map(idea_json).collect();
    if list.is_empty() {
        o.insert(
            "note".into(),
            json!(
                ideas
                    .skip_reason_th
                    .clone()
                    .unwrap_or_else(|| "ยังไม่มีไอเดียที่ผ่านเกณฑ์ของโปรไฟล์ผู้ใช้จากข้อมูลปัจจุบัน".into())
            ),
        );
    }
    o.insert("ideas".into(), Value::Array(list));
    Value::Object(o)
}

/// `find_ideas` over the whole universe: the ranking's picks (already ≤ 3)
/// with each pick's why-now numbers and earnings note, plus the data day it
/// used and the budget-short note when nothing fits the cash.
pub fn summarize_universe_ideas(u: &UniverseIdeas) -> Value {
    let mut o = Map::new();
    o.insert(
        "as_of".into(),
        json!(u.as_of.clone().unwrap_or_else(|| u.computed_at.clone())),
    );
    o.insert(
        "source".into(),
        json!(match u.source.as_str() {
            "LOCAL" => "local_file",
            _ => "recorder",
        }),
    );
    if let Some(d) = &u.session_date {
        o.insert("data_day".into(), json!(d));
    }
    o.insert("scanned_symbols".into(), json!(u.scanned_symbols));
    o.insert("candidate_symbols".into(), json!(u.candidate_symbols));
    let picks: Vec<Value> = u
        .picks
        .iter()
        .take(5)
        .map(|p| {
            let mut v = idea_json(&p.idea);
            if let Value::Object(map) = &mut v {
                map.insert("symbol".into(), json!(p.idea.symbol));
                map.insert("rank".into(), json!(p.rank));
                map.insert("score".into(), num2(p.score));
                if let Some(iv) = p.iv_pct {
                    map.insert("iv_pct".into(), num2(iv));
                }
                if let Some(rv) = p.realized_30d_pct {
                    map.insert("realized_30d_pct".into(), num2(rv));
                }
                if let Some(e) = &p.earnings_date {
                    map.insert("earnings_date".into(), json!(e));
                }
            }
            v
        })
        .collect();
    if picks.is_empty() {
        o.insert(
            "note".into(),
            json!(
                u.budget_short
                    .as_ref()
                    .map(|b| b.text_th.clone())
                    .unwrap_or_else(|| u.agent_say_th.clone())
            ),
        );
    } else if let Some(b) = &u.budget_short {
        o.insert("budget_short_usd".into(), num2(b.min_cash_usd));
        o.insert("budget_short_symbol".into(), json!(b.cheapest_symbol));
    }
    // "เงินที่ต้องมี" ของหุ้นที่ user สนใจ — compact ≤ 5 แถว ไม่มีข้อความ
    // ยาว (ประโยคไทยเต็มอยู่ที่หน้าบ้านแล้ว) เพื่ออยู่ใน byte budget
    if !u.watch_cash.is_empty() {
        let list: Vec<Value> = u
            .watch_cash
            .iter()
            .take(5)
            .map(|w| {
                json!({
                    "symbol": w.symbol,
                    "spot": w.spot.map(num2).unwrap_or(Value::Null),
                    "min_cash_usd": w.min_cash_usd.map(num2).unwrap_or(Value::Null),
                    "any_cash_usd": w.any_cash_usd.map(num2).unwrap_or(Value::Null),
                    "affordable": w.affordable,
                })
            })
            .collect();
        o.insert("watch_cash".into(), Value::Array(list));
    }
    o.insert("picks".into(), Value::Array(picks));
    Value::Object(o)
}

// ---------- what_if ----------

/// The per-share premium of a screener row. `EnrichedRow::prem` is per
/// share; `EnrichedRow::credit` is per contract (premium × 100), so it is
/// only a fallback and is divided back. lab_analyze and the payoff math
/// take the per-share value.
pub fn premium_per_share(prem: Option<f64>, credit: Option<f64>) -> Option<f64> {
    prem.filter(|p| p.is_finite() && *p > 0.0).or_else(|| {
        credit
            .filter(|c| c.is_finite() && *c > 0.0)
            .map(|c| c / 100.0)
    })
}

/// Everything `summarize_what_if` needs beyond the lab_analyze JSON it
/// condenses (spot/strike/premium/iv/t went into that call already).
pub struct WhatIfInput {
    pub ref_symbol: String,
    pub spot: f64,
    pub strike: f64,
    /// premium PER SHARE — see [`premium_per_share`]
    pub premium: f64,
    pub is_call: bool,
    pub covered: bool,
    pub basis: Option<f64>,
    pub price_at_expiry: Option<f64>,
    pub as_of: String,
}

/// Condense a `lab_analyze` result for the model: headline numbers from the
/// lab JSON, a ≤ 7-point scenario table (spot ±%, strike, break-even) and —
/// when a price was given — the P/L at exactly that price, recomputed with
/// the same round-trip fees the lab uses (`leg_pnl_at` / `cc_pnl_at`).
pub fn summarize_what_if(lab: &Value, x: &WhatIfInput) -> Result<Value, String> {
    if x.covered && x.basis.is_none() {
        return Err(
            "covered=true ต้องระบุ basis_usd (ราคาที่ซื้อหุ้นไว้ต่อหุ้น) ด้วย — เพิ่ม basis_usd แล้วเรียกใหม่".into(),
        );
    }
    let fees_rt = CostInputs::default().per_side() * 2.0; // เข้า+ออก เท่า lab_analyze
    let leg = ShortLeg {
        is_call: x.is_call,
        k: x.strike,
        c: x.premium,
        m: 100.0,
    };
    let cc = x
        .basis
        .filter(|b| *b > 0.0)
        .map(|b| CoveredCall { leg, b, s0: x.spot });
    let pnl_at = |s: f64| match cc {
        Some(cc) => cc_pnl_at(cc, s, fees_rt),
        None => leg_pnl_at(leg, s, fees_rt),
    };

    // headline economics: the covered view reads the covered_call section
    let (be, max_profit, max_loss) = if x.covered {
        let c = &lab["covered_call"];
        (
            c["breakeven"].as_f64(),
            c["max_profit"].as_f64(),
            c["max_loss"].as_f64(),
        )
    } else {
        let l = &lab["leg"]["round_trip"];
        (
            l["breakeven"].as_f64(),
            l["max_profit"].as_f64(),
            l["max_loss"].as_f64(),
        )
    };

    // scenario table: spot ±10/±5/0 %, strike and the break-even, deduped
    // to cents and sorted — at most 7 distinct points by construction
    let mut pts: Vec<f64> = vec![
        x.spot * 0.90,
        x.spot * 0.95,
        x.spot,
        x.spot * 1.05,
        x.spot * 1.10,
        x.strike,
    ];
    if let Some(b) = be {
        pts.push(b);
    }
    pts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    pts.dedup_by(|a, b| (*a * 100.0).round() == (*b * 100.0).round());
    pts.truncate(7);
    let scenarios: Vec<Value> = pts
        .iter()
        .map(|&s| {
            json!({
                "price_usd": num2(s),
                "pnl_usd": num2(pnl_at(s)),
            })
        })
        .collect();

    let mut o = Map::new();
    o.insert("as_of".into(), json!(x.as_of));
    o.insert("source".into(), json!("engine"));
    o.insert("ref".into(), json!(x.ref_symbol));
    o.insert("side".into(), json!(if x.is_call { "call" } else { "put" }));
    o.insert("covered".into(), json!(x.covered));
    o.insert("spot".into(), num2(x.spot));
    o.insert("strike".into(), num2(x.strike));
    o.insert("premium_usd".into(), num2(x.premium * 100.0));
    if let Some(be) = be {
        o.insert("breakeven".into(), num2(be));
    }
    o.insert("max_profit_usd".into(), num2(max_profit.unwrap_or(0.0)));
    if let Some(ml) = max_loss {
        o.insert("max_loss_usd".into(), num2(ml));
    }
    if let Some(p) = lab["pop"].as_f64() {
        o.insert("chance_of_profit_pct".into(), num2(p * 100.0));
    }
    if let Some(p) = lab["p_itm"].as_f64() {
        o.insert("chance_itm_pct".into(), num2(p * 100.0));
    }
    if let Some(p) = lab["p_touch"].as_f64() {
        o.insert("chance_touch_pct".into(), num2(p * 100.0));
    }
    if let Some(d) = lab["delta"].as_f64() {
        o.insert("delta".into(), num4(d));
    }
    o.insert("scenarios".into(), Value::Array(scenarios));
    if let Some(p) = x.price_at_expiry {
        o.insert(
            "pnl_at_price".into(),
            json!({
                "price_usd": num2(p),
                "pnl_usd": num2(pnl_at(p)),
            }),
        );
    }
    Ok(Value::Object(o))
}

#[cfg(test)]
mod premium_unit_tests {
    use super::premium_per_share;

    #[test]
    fn prefers_the_per_share_premium() {
        assert_eq!(premium_per_share(Some(1.8), Some(180.0)), Some(1.8));
    }

    #[test]
    fn falls_back_to_credit_divided_by_the_multiplier() {
        let p = premium_per_share(None, Some(180.0)).expect("fallback");
        assert!((p - 1.8).abs() < 1e-9);
    }

    #[test]
    fn missing_or_zero_values_give_none() {
        assert_eq!(premium_per_share(None, None), None);
        assert_eq!(premium_per_share(Some(0.0), Some(0.0)), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::registry::Registry;
    use crate::easy::{EasyCases, IdeaLevel, Stance, WhyNow};
    use crate::easy_rank::{BudgetShort, OiContext, ScoreParts, UniversePick, WatchCash};

    // ---------- specs ----------

    #[test]
    fn three_specs_with_expected_names_and_modes() {
        let specs = specs();
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["find_ideas", "scan_chain", "what_if"]);
        let modes = |n: &str| specs.iter().find(|s| s.name == n).unwrap().modes.clone();
        assert_eq!(modes("find_ideas"), vec![Mode::Easy, Mode::Pro]);
        assert_eq!(modes("scan_chain"), vec![Mode::Pro]);
        assert_eq!(modes("what_if"), vec![Mode::Easy, Mode::Pro]);
    }

    #[test]
    fn specs_follow_the_tool_conventions() {
        for spec in specs() {
            assert_eq!(spec.tier, Tier::Read, "{}", spec.name);
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
            assert!(spec.webull_cost > 0 || spec.name == "what_if");
            assert!(spec.timeout_ms >= 5_000, "{}", spec.name);
        }
    }

    #[test]
    fn every_example_passes_validation_in_its_modes_and_scan_chain_is_pro_only() {
        let mut reg = Registry::new();
        for spec in specs() {
            reg.register(spec.clone()).expect("register ideas spec");
        }
        for spec in specs() {
            for mode in spec.modes.clone() {
                for ex in &spec.examples {
                    let args = ex.to_string();
                    reg.validate(&spec.name, mode, &args).unwrap_or_else(|e| {
                        panic!("example {args} of {} failed: {}", spec.name, e.message)
                    });
                }
            }
        }
        // scan_chain must be hidden from Easy, not just unlisted
        let err = reg.validate("scan_chain", Mode::Easy, "{}").unwrap_err();
        assert_eq!(err.code.as_str(), "not_in_mode");
        // and find_ideas' budget override stays a non-negative number
        assert!(
            reg.validate("find_ideas", Mode::Easy, r#"{"budget_usd": -5}"#)
                .is_err()
        );
    }

    // ---------- find_ideas summarisers ----------

    fn idea(n: usize) -> EasyIdea {
        let strike = 150.0 + n as f64 * 5.0;
        EasyIdea {
            occ: format!("NVDA261218P{:08}", (strike * 1000.0) as i64),
            symbol: "NVDA".into(),
            level: IdeaLevel::Safe,
            recommended: n == 0,
            strike,
            expiry: "2026-12-18".into(),
            days: 21.0,
            premium: 1.5 + n as f64 * 0.25,
            credit: (1.5 + n as f64 * 0.25) * 100.0,
            cost: strike * 100.0,
            period_pct: 1.14,
            annual_pct: Some(19.86),
            breakeven: strike - 1.75,
            below_spot_pct: 8.55 - n as f64,
            assign_pct: 15.2,
            cases: EasyCases {
                keep_pct: 70.0,
                between_pct: 14.8,
                loss_pct: 15.2,
            },
            worst_30_usd: -3250.0,
            max_loss_usd: -(strike * 100.0) + 175.0,
            warn: false,
            cash_short: false,
        }
    }

    fn easy_ideas(count: usize) -> EasyIdeas {
        EasyIdeas {
            symbol: "NVDA".into(),
            spot: 191.25,
            source: "LIVE".into(),
            stance: Stance::Want,
            agent_say_th: String::new(),
            ideas: (0..count).map(idea).collect(),
            skip_reason_th: None,
            why_now: WhyNow {
                iv_pct: Some(48.2),
                realized_30d_pct: Some(41.3),
                text_th: String::new(),
            },
            computed_at: "2026-11-27T10:00:00+00:00".into(),
        }
    }

    #[test]
    fn summarize_easy_ideas_caps_at_five_and_carries_ref_as_of_source() {
        let out = summarize_easy_ideas(&easy_ideas(6));
        assert_eq!(out["as_of"], "2026-11-27T10:00:00+00:00");
        assert_eq!(out["source"], "engine");
        assert_eq!(out["symbol"], "NVDA");
        let ideas = out["ideas"].as_array().unwrap();
        assert_eq!(ideas.len(), 5);
        for i in ideas {
            assert!(i["ref"].as_str().unwrap().starts_with("NVDA"));
            assert!(i["cash_needed_usd"].is_number());
            assert!(i["premium_usd"].is_number());
        }
        // whole numbers collapse to ints, keeping 5 ideas inside the budget
        assert_eq!(out["ideas"][0]["days"], json!(21));
    }

    #[test]
    fn summarize_easy_ideas_stays_under_1500_bytes_on_a_realistic_fixture() {
        let out = summarize_easy_ideas(&easy_ideas(5));
        let size = serde_json::to_string(&out).unwrap().len();
        assert!(size <= 1500, "easy ideas result is {size} bytes");
    }

    #[test]
    fn summarize_easy_ideas_empty_becomes_a_note_not_an_error() {
        let mut fixture = easy_ideas(0);
        fixture.skip_reason_th = Some("ยังไม่มีข้อมูลครบ".into());
        let out = summarize_easy_ideas(&fixture);
        assert_eq!(out["ideas"].as_array().unwrap().len(), 0);
        assert_eq!(out["note"], "ยังไม่มีข้อมูลครบ");
    }

    fn pick(n: u32) -> UniversePick {
        UniversePick {
            rank: n,
            idea: idea(n as usize),
            score: 72.5 - f64::from(n),
            parts: ScoreParts {
                vrp: Some(0.8),
                premium: Some(0.6),
                fit: Some(0.9),
                cushion: Some(0.5),
                liquidity: Some(0.7),
                event_penalty: 1.0,
            },
            reasons_th: vec!["เหตุผลยาวๆ ที่ตัดทิ้งเพื่อประหยัดพื้นที่".into()],
            cautions_th: vec![],
            iv_pct: Some(48.2),
            realized_30d_pct: Some(41.3),
            earnings_date: (n == 1).then(|| "2026-12-10".to_string()),
            oi: OiContext {
                put_wall_strike: Some(170.0),
                put_wall_oi: Some(12_000.0),
                oi_change_pct: None,
                note_th: String::new(),
            },
        }
    }

    #[test]
    fn summarize_universe_ideas_keeps_refs_data_day_and_budget_note() {
        let u = UniverseIdeas {
            session_date: Some("2026-11-26".into()),
            as_of: Some("2026-11-26T21:00:00+00:00".into()),
            source: "LOCAL".into(),
            scanned_symbols: 40,
            candidate_symbols: 6,
            market_note_th: String::new(),
            picks: vec![pick(1), pick(2), pick(3)],
            budget_short: None,
            watch_cash: vec![
                WatchCash {
                    symbol: "NVDA".into(),
                    spot: Some(178.4),
                    min_cash_usd: Some(16_500.0),
                    any_cash_usd: Some(16_500.0),
                    affordable: false,
                    text_th: "NVDA ราคา $178 · ขาย 1 สัญญาต้องกันเงินราว $16,500 (ยอมซื้อ 100 หุ้นที่ $165)"
                        .into(),
                },
                WatchCash {
                    symbol: "PFE".into(),
                    spot: Some(28.46),
                    min_cash_usd: None,
                    any_cash_usd: Some(2_600.0),
                    affordable: false,
                    text_th: "PFE ราคา $28 · ยังไม่มีสัญญาที่ปลอดภัยพอในรอบนี้ — ตัวที่ถูกสุดต้องกันเงิน $2,600"
                        .into(),
                },
            ],
            agent_say_th: String::new(),
            computed_at: "2026-11-27T09:00:00+00:00".into(),
        };
        let out = summarize_universe_ideas(&u);
        assert_eq!(out["as_of"], "2026-11-26T21:00:00+00:00");
        assert_eq!(out["source"], "local_file");
        assert_eq!(out["data_day"], "2026-11-26");
        let picks = out["picks"].as_array().unwrap();
        assert_eq!(picks.len(), 3);
        for p in picks {
            assert!(p["ref"].as_str().unwrap().starts_with("NVDA"));
            assert_eq!(p["symbol"], "NVDA");
            assert!(p["rank"].is_number());
            assert!(p["score"].is_number());
        }
        assert_eq!(out["picks"][0]["earnings_date"], "2026-12-10");
        assert!(out.get("budget_short_usd").is_none());
        let watch = out["watch_cash"].as_array().unwrap();
        assert_eq!(watch.len(), 2);
        assert_eq!(watch[0]["symbol"], "NVDA");
        assert_eq!(watch[0]["min_cash_usd"], json!(16_500));
        assert_eq!(watch[1]["min_cash_usd"], Value::Null);
        assert_eq!(watch[1]["affordable"], json!(false));
        let size = serde_json::to_string(&out).unwrap().len();
        assert!(size <= 1500, "universe result is {size} bytes");
    }

    #[test]
    fn summarize_universe_ideas_empty_uses_budget_short_note() {
        let u = UniverseIdeas {
            session_date: Some("2026-11-26".into()),
            as_of: None,
            source: "API".into(),
            scanned_symbols: 40,
            candidate_symbols: 0,
            market_note_th: String::new(),
            picks: vec![],
            budget_short: Some(BudgetShort {
                min_cash_usd: 3800.0,
                cheapest_symbol: "F".into(),
                text_th: "งบยังไม่พอ".into(),
            }),
            watch_cash: vec![],
            agent_say_th: String::new(),
            computed_at: "2026-11-27T09:00:00+00:00".into(),
        };
        let out = summarize_universe_ideas(&u);
        assert_eq!(out["picks"].as_array().unwrap().len(), 0);
        assert_eq!(out["note"], "งบยังไม่พอ");
        assert_eq!(out["source"], "recorder");
        assert!(out.get("watch_cash").is_none(), "watch_cash ว่าง ⇒ ไม่มี key");
        // as_of falls back to the computation time
        assert_eq!(out["as_of"], "2026-11-27T09:00:00+00:00");
    }

    // ---------- what_if ----------

    fn lab_json() -> Value {
        json!({
            "symbol": "NVDA261218P00175000",
            "fees_round_trip": 1.327,
            "leg": {
                "open": {"max_profit": 198.67, "max_loss": null, "breakeven": null, "collateral": null},
                "round_trip": {"max_profit": 197.34, "max_loss": -17282.66, "breakeven": 172.01, "collateral": 17500.0}
            },
            "pop": 0.72,
            "p_itm": 0.18,
            "p_touch": 0.35,
            "delta": -0.1523,
            "covered_call": null,
            "call_capital_today": null,
            "scenarios": [],
            "cc_scenarios": null
        })
    }

    fn what_if_input(price: Option<f64>) -> WhatIfInput {
        WhatIfInput {
            ref_symbol: "NVDA261218P00175000".into(),
            spot: 175.6,
            strike: 175.0,
            premium: 2.0,
            is_call: false,
            covered: false,
            basis: None,
            price_at_expiry: price,
            as_of: "2026-11-27T10:05:00+00:00".into(),
        }
    }

    #[test]
    fn what_if_pnl_at_price_equals_leg_pnl_at() {
        let out = summarize_what_if(&lab_json(), &what_if_input(Some(160.0))).unwrap();
        let fees_rt = CostInputs::default().per_side() * 2.0;
        let leg = ShortLeg {
            is_call: false,
            k: 175.0,
            c: 2.0,
            m: 100.0,
        };
        let expected = leg_pnl_at(leg, 160.0, fees_rt);
        assert_eq!(out["pnl_at_price"]["price_usd"], json!(160));
        assert_eq!(out["pnl_at_price"]["pnl_usd"], num2(expected));
        assert_eq!(out["breakeven"], json!(172.01));
        assert_eq!(out["max_profit_usd"], num2(197.34));
        assert_eq!(out["max_loss_usd"], num2(-17282.66));
        assert_eq!(out["chance_of_profit_pct"], json!(72));
        assert_eq!(out["chance_itm_pct"], json!(18));
        assert_eq!(out["chance_touch_pct"], json!(35));
        assert_eq!(out["delta"], num4(-0.1523));
    }

    #[test]
    fn what_if_scenario_table_is_capped_and_covers_the_breakeven_region() {
        let out = summarize_what_if(&lab_json(), &what_if_input(None)).unwrap();
        let rows = out["scenarios"].as_array().unwrap();
        assert!(rows.len() <= 7, "{} scenario rows", rows.len());
        let prices: Vec<f64> = rows
            .iter()
            .filter_map(|r| r["price_usd"].as_f64())
            .collect();
        // 158.04 (−10%) … 184.38 (+10%) with 172.01 (BE) inside the span
        assert!(prices.first().unwrap() < &172.01 && prices.last().unwrap() > &172.01);
        assert!(prices.contains(&172.01));
        // every scenario pnl is leg_pnl_at at that price
        let fees_rt = CostInputs::default().per_side() * 2.0;
        let leg = ShortLeg {
            is_call: false,
            k: 175.0,
            c: 2.0,
            m: 100.0,
        };
        for row in rows {
            let p = row["price_usd"].as_f64().unwrap();
            assert_eq!(row["pnl_usd"], num2(leg_pnl_at(leg, p, fees_rt)));
        }
        assert!(out.get("pnl_at_price").is_none());
    }

    #[test]
    fn what_if_covered_without_basis_is_an_error() {
        let mut x = what_if_input(None);
        x.covered = true;
        let err = summarize_what_if(&lab_json(), &x).unwrap_err();
        assert!(
            err.contains("basis_usd"),
            "error should say how to fix: {err}"
        );
    }

    #[test]
    fn what_if_covered_uses_cc_pnl_at_and_the_covered_call_section() {
        let mut lab = lab_json();
        lab["covered_call"] = json!({
            "max_profit": 1298.67,
            "max_loss": -16998.67,
            "breakeven": 169.01,
            "capital_today": 17560.0
        });
        let mut x = what_if_input(Some(190.0));
        x.covered = true;
        x.basis = Some(170.0);
        x.is_call = true;
        x.strike = 175.0;
        let out = summarize_what_if(&lab, &x).unwrap();
        let fees_rt = CostInputs::default().per_side() * 2.0;
        let cc = CoveredCall {
            leg: ShortLeg {
                is_call: true,
                k: 175.0,
                c: 2.0,
                m: 100.0,
            },
            b: 170.0,
            s0: 175.6,
        };
        assert_eq!(
            out["pnl_at_price"]["pnl_usd"],
            num2(cc_pnl_at(cc, 190.0, fees_rt))
        );
        assert_eq!(out["breakeven"], json!(169.01));
        assert_eq!(out["max_profit_usd"], num2(1298.67));
        assert_eq!(out["max_loss_usd"], num2(-16998.67));
    }
}
