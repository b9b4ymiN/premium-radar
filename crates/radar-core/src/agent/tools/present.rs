//! Agent tools — `present` family (W6 P4): `present_idea`, the structured
//! recommendation card. Specs + pure card builder; the Tauri crate reads
//! the contract from the engine and emits the card.

use chrono::NaiveDate;
use serde::Serialize;
use serde_json::{Value, json};

use crate::agent::contract::{Mode, Tier, ToolSpec};
use crate::agent::grounding::{collect_facts, ungrounded};
use crate::agent::tools::ideas::premium_per_share;
use crate::easy::EasyIdea;
use crate::lab::payoff::{CostInputs, ShortLeg, leg_pnl_at};
use crate::quant::OptType;
use crate::screener::EnrichedRow;

/// พาดหัวการ์ด (ตัวอักษร ไม่ใช่ไบต์)
pub const MAX_HEADLINE_CHARS: usize = 80;
/// คำตอบแต่ละข้อ (ตัวอักษร ไม่ใช่ไบต์)
pub const MAX_ANSWER_CHARS: usize = 240;

/// ตัวเลขบนการ์ดทั้งหมด — ตรงกับ `numbers` ใน `IdeaCardData`
/// (`src/agent/types.ts`): Rust คำนวณจากสัญญา โมเดลแตะไม่ได้
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CardNumbers {
    pub strike: f64,
    pub expiry: String,
    pub days: f64,
    pub premium_usd: f64,
    pub cash_needed_usd: f64,
    pub breakeven: f64,
    pub assign_pct: Option<f64>,
    pub worst_30_usd: Option<f64>,
    pub max_loss_usd: Option<f64>,
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// การ์ดของไอเดียที่เครื่องง่ายคำนวณไว้แล้ว — แค่ย้ายค่าเข้าตำแหน่งของการ์ด
pub fn card_numbers_from_easy(idea: &EasyIdea) -> CardNumbers {
    CardNumbers {
        strike: idea.strike,
        expiry: idea.expiry.clone(),
        days: idea.days,
        premium_usd: round2(idea.credit),
        cash_needed_usd: round2(idea.cost),
        breakeven: round2(idea.breakeven),
        assign_pct: Some(idea.assign_pct),
        worst_30_usd: Some(round2(idea.worst_30_usd)),
        max_loss_usd: Some(round2(idea.max_loss_usd)),
    }
}

/// การ์ดของแถวสัญญาใดๆ จาก engine: เงินที่ได้ = credit ต่อสัญญา, ทุนที่ต้อง
/// กัน = collateral (ไม่มี → put คิด strike×100, call คิดหุ้น 100 ตัวที่ราคา
/// วันนี้ เหมือน `call_capital_today` ของ lab), ถ้าหุ้นลง 30% = P/L ที่ expiry
/// ด้วย payoff/ค่า fee ชุดเดียวกับ `lab_analyze` (ไป-กลับ), ขาดทุนสูงสุดของ
/// call ไม่มีขอบเขต → null
pub fn card_numbers_from_row(row: &EnrichedRow, spot: f64, as_of_date: NaiveDate) -> CardNumbers {
    let strike = row.q.strike;
    let is_call = row.q.option_type == OptType::Call;
    let credit = row.credit.unwrap_or_default();
    let cash_fallback = if is_call {
        spot * 100.0
    } else {
        strike * 100.0
    };
    let cash = row
        .collateral
        .filter(|c| c.is_finite() && *c > 0.0)
        .unwrap_or(cash_fallback);
    let premium = premium_per_share(row.prem, row.credit);
    let breakeven = row.be.filter(|b| b.is_finite()).unwrap_or_else(|| {
        premium
            .map(|p| if is_call { strike + p } else { strike - p })
            .unwrap_or(strike)
    });
    let worst_30_usd = premium.map(|p| {
        // ชุด fee เดียวกับ scenario table ของ lab_analyze: per_side × 2 (เข้า+ออก)
        let fees_round_trip = CostInputs::default().per_side() * 2.0;
        leg_pnl_at(
            ShortLeg {
                is_call,
                k: strike,
                c: p,
                m: 100.0,
            },
            spot * 0.7,
            fees_round_trip,
        )
    });
    CardNumbers {
        strike,
        expiry: row.q.expiry.format("%Y-%m-%d").to_string(),
        days: (row.q.expiry - as_of_date).num_days().max(0) as f64,
        premium_usd: round2(credit),
        cash_needed_usd: round2(cash),
        breakeven: round2(breakeven),
        assign_pct: row
            .p_itm
            .filter(|p| p.is_finite())
            .map(|p| round2(p * 100.0)),
        worst_30_usd: worst_30_usd.map(round2),
        max_loss_usd: (!is_call).then(|| round2(strike * 100.0 - credit)),
    }
}

/// ห้ามใช้ map/ปมนามธรรมในนี้ — build_card ต้องไล่ field เองเพื่อให้ key
/// ของ JSON ตรงกับ `IdeaCardData` เป๊ะ (types.ts เป็น contract)
fn text_field(args: &Value, field: &str, max_chars: usize) -> Result<String, Value> {
    let Some(raw) = args.get(field).and_then(Value::as_str) else {
        return Err(json!({
            "error": "bad_args",
            "message": format!("ขาดช่อง {field} — ใส่ข้อความภาษาไทยของช่องนี้แล้วเรียกใหม่"),
        }));
    };
    let text = raw.trim();
    if text.is_empty() {
        return Err(json!({
            "error": "bad_args",
            "message": format!("ช่อง {field} ห้ามว่าง — เขียนข้อความภาษาไทยของช่องนี้แล้วเรียกใหม่"),
        }));
    }
    let length = text.chars().count();
    if length > max_chars {
        return Err(json!({
            "error": "text_too_long",
            "field": field,
            "message": format!(
                "ช่อง {field} ยาว {length} ตัวอักษร เกินสูงสุด {max_chars} ตัวอักษร — สั้นลงแล้วเรียกใหม่"
            ),
        }));
    }
    Ok(text.to_string())
}

/// คำตอบหนึ่งข้อ — error ของความยาวบอกว่าเป็นข้อไหน ไม่ใช่แค่ชื่อช่อง
fn answer_field(args: &Value, field: &str) -> Result<String, Value> {
    text_field(args, field, MAX_ANSWER_CHARS).map_err(|mut error| {
        if error["error"] == "text_too_long"
            && let Some(message) = error["message"].as_str()
        {
            error["message"] = json!(format!("คำตอบข้อ {field} — {message}"));
        }
        error
    })
}

/// ประกอบการ์ด (`IdeaCardData` JSON) จากคำตอบห้าข้อของโมเดล + ตัวเลขที่
/// Rust คำนวณไว้: บังคับความยาวทุกช่อง แล้วตรวจว่าตัวเลขทุกตัวในพาดหัว
/// กับคำตอบมีที่มาจากตัวเลขของการ์ด (หรือ fact พิเศษที่ผู้เรียกส่งมา
/// เช่น IV ของ why-now) — ตัวไหนไม่มีที่มาคืน `ungrounded_numbers`
/// พร้อมรายการ token และคำใบ้แก้ตัวเลขนั้น
pub fn build_card(
    args: &Value,
    numbers: CardNumbers,
    symbol: &str,
    as_of: &str,
    extra_facts: &[&str],
) -> Result<Value, Value> {
    let reference = args
        .get("ref")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            json!({
                "error": "bad_args",
                "message": "ขาดช่อง ref — ระบุรหัสสัญญา (OCC symbol) เช่น NVDA261218P00175000",
            })
        })?
        .to_string();
    let headline = text_field(args, "headline", MAX_HEADLINE_CHARS)?;
    let gain = answer_field(args, "gain")?;
    let tradeoff = answer_field(args, "tradeoff")?;
    let chance = answer_field(args, "chance")?;
    let why_now = answer_field(args, "why_now")?;
    let worst = answer_field(args, "worst")?;

    let mut facts = vec![
        numbers.strike,
        numbers.days,
        numbers.premium_usd,
        numbers.cash_needed_usd,
        numbers.breakeven,
        // 30 คือชื่อของฉากของ worst_30_usd เอง — การ์ดเขียนว่า "ถ้าหุ้นลง 30%"
        // อยู่แล้ว โมเดลจึงมีสิทธิอ้าง 30% ในข้อแย่สุดเสมอ
        30.0,
    ];
    facts.extend(
        [
            &numbers.assign_pct,
            &numbers.worst_30_usd,
            &numbers.max_loss_usd,
        ]
        .into_iter()
        .flatten(),
    );
    // โมเดลเล่าขาดทุนเป็นจำนวน ("ขาดทุนราว $3,509") ในขณะที่ fact เป็นลบ —
    // สำเนา |worst_30| ให้รูปที่อ่านได้ (facts ตั้งใจ permissive อยู่แล้ว)
    if let Some(worst) = numbers.worst_30_usd {
        facts.push(worst.abs());
    }
    facts.extend(collect_facts(extra_facts));

    let texts: [(&str, &str); 6] = [
        ("headline", &headline),
        ("gain", &gain),
        ("tradeoff", &tradeoff),
        ("chance", &chance),
        ("why_now", &why_now),
        ("worst", &worst),
    ];
    let mut tokens: Vec<String> = Vec::new();
    let mut fields: Vec<String> = Vec::new();
    for (field, text) in &texts {
        for token in ungrounded(text, &facts) {
            if !tokens.contains(&token) {
                tokens.push(token.clone());
            }
            if !fields.contains(&field.to_string()) {
                fields.push(field.to_string());
            }
        }
    }
    if !tokens.is_empty() {
        return Err(json!({
            "error": "ungrounded_numbers",
            "tokens": tokens,
            "fields": fields,
            "message": format!(
                "ตัวเลขเหล่านี้ยังไม่มีที่มา: {} — เอาตัวเลขนี้ออก หรือเรียก tool ที่ให้ตัวเลขนี้ก่อน",
                tokens.join(", ")
            ),
        }));
    }

    Ok(json!({
        "ref": reference,
        "symbol": symbol,
        "headline": headline,
        "answers": {
            "gain": gain,
            "tradeoff": tradeoff,
            "chance": chance,
            "why_now": why_now,
            "worst": worst,
        },
        "numbers": {
            "strike": numbers.strike,
            "expiry": numbers.expiry,
            "days": numbers.days,
            "premium_usd": numbers.premium_usd,
            "cash_needed_usd": numbers.cash_needed_usd,
            "breakeven": numbers.breakeven,
            "assign_pct": numbers.assign_pct,
            "worst_30_usd": numbers.worst_30_usd,
            "max_loss_usd": numbers.max_loss_usd,
        },
        "as_of": as_of,
    }))
}

pub fn specs() -> Vec<ToolSpec> {
    vec![ToolSpec {
        name: "present_idea".into(),
        tier: Tier::Ui,
        modes: vec![Mode::Easy, Mode::Pro],
        description: "แสดงการ์ดคำแนะนำสัญญาหนึ่งตัวในหน้าต่างแชต — คุณเขียนแค่คำตอบภาษาไทย 5 ข้อ (gain ได้เงินเท่าไร, tradeoff แลกกับอะไร, chance โอกาสแค่ไหน, why_now ทำไมช่วงนี้, worst แย่สุดคืออะไร) ส่วนตัวเลขบนการ์ด (เงินที่ได้ทันที เงินที่ต้องกันไว้ จุดคุ้มทุน โอกาสต้องซื้อหุ้น ฯลฯ) แอปคำนวณจากรหัสสัญญาให้เอง แล้วส่งกลับมาในช่อง numbers เพื่อให้อ้างอิงต่อได้\nใช้เมื่อ: กำลังจะแนะนำหรือสรุปสัญญาใดสัญญาหนึ่งให้ผู้ใช้ตัดสินใจ — ต้องเรียกทุกครั้งแทนการพิมพ์คำแนะนำเป็นข้อความ\nไม่ใช้เมื่อ: ยังไม่ได้เลือกสัญญาเจาะจง (เรียก find_ideas หรือ scan_chain ก่อน) หรือผู้ใช้แค่ถามข้อมูล/เปรียบเทียบหลายตัว (ตอบเป็นข้อความแทน)\nตัวเลขบนการ์ดแอปใส่ให้เอง ไม่ต้องพิมพ์ตัวเลขในคำตอบ 5 ข้อ ยกเว้นอ้างตัวเลขที่ tool ส่งมาแล้ว".into(),
        examples: vec![
            json!({
                "ref": "NVDA261218P00175000",
                "headline": "รับเงินทันทีแลกกับการพร้อมซื้อ NVDA ในราคาที่คุณยอมรับ",
                "gain": "เงินเข้าบัญชีทันทีเมื่อขายสัญญานี้ เป็นของคุณไม่ว่าจะเกิดอะไรขึ้น",
                "tradeoff": "คุณสัญญาว่าถ้าวันหมดอายุราคาหุ้นต่ำกว่าที่ตกลง คุณจะซื้อหุ้น 100 หุ้นในราคานั้น",
                "chance": "โอกาสที่จะต้องซื้อหุ้นจริงๆ ต่ำกว่าครึ่ง ส่วนใหญ่แล้วจบแบบเก็บเงินได้ฟรี",
                "why_now": "ช่วงนี้คนยอมจ่ายค่าประกันราคาหุ้นแพงกว่าปกติ เป็นช่วงที่ขายได้ราคาดี",
                "worst": "ถ้าหุ้นร่วงแรง คุณต้องซื้อหุ้นในราคาที่สูงกว่าตลาด ขาดทุนบนกระดาษจนกว่าราคาจะกลับมา"
            }),
            json!({
                "ref": "AAPL261218C00250000",
                "headline": "ขายสัญญาของหุ้นที่คุณถืออยู่ เก็บเงินเพิ่มทุกเดือน",
                "gain": "ได้เงินสดเข้ามาทันทีจากการขายสัญญา ไม่ต้องทำอะไรเพิ่ม",
                "tradeoff": "ถ้าหุ้นวิ่งขึ้นเกินราคาที่ตกลง คุณต้องขายหุ้นที่ถืออยู่ในราคานั้น ได้กำไรน้อยกว่าที่ควร",
                "chance": "โอกาสที่หุ้นจะวิ่งขึ้นเกินราคาตกลงไม่สูง แต่ต้องยอมรับได้ถ้าเกิดขึ้น",
                "why_now": "ราคาหุ้นนิ่งๆ มาสักระยะ การเก็บเงินจากสัญญาช่วยเพิ่มผลตอบแทนได้",
                "worst": "ถ้าหุ้นพุ่งขึ้นมาก คุณเสียโอกาสกำไรส่วนที่เกินราคาที่ตกลงไว้"
            }),
        ],
        parameters: json!({
            "type": "object",
            "properties": {
                "ref": {
                    "type": "string",
                    "description": "รหัสสัญญา (OCC symbol) ที่จะแนะนำ เช่น NVDA261218P00175000 — ต้องเป็นตัวที่ได้จาก find_ideas หรือ scan_chain แล้ว"
                },
                "headline": {
                    "type": "string",
                    "maxLength": 80,
                    "description": "พาดหัวการ์ดภาษาไทยสั้นๆ ไม่เกิน 80 ตัวอักษร เขียนด้วยคำง่ายๆ ไม่ใช้ศัพท์ตัวเลือกซื้อ"
                },
                "gain": {
                    "type": "string",
                    "maxLength": 240,
                    "description": "คำตอบข้อ ได้เงินเท่าไร — ภาษาไทยเข้าใจง่าย ไม่เกิน 240 ตัวอักษร"
                },
                "tradeoff": {
                    "type": "string",
                    "maxLength": 240,
                    "description": "คำตอบข้อ แลกกับอะไร — สิ่งที่ผู้ใช้ต้องยอมรับ"
                },
                "chance": {
                    "type": "string",
                    "maxLength": 240,
                    "description": "คำตอบข้อ โอกาสแค่ไหน — ความน่าจะเป็นที่ผู้ใช้สนใจ"
                },
                "why_now": {
                    "type": "string",
                    "maxLength": 240,
                    "description": "คำตอบข้อ ทำไมช่วงนี้ — เหตุผลจากข้อมูลที่ tool ให้มา"
                },
                "worst": {
                    "type": "string",
                    "maxLength": 240,
                    "description": "คำตอบข้อ แย่สุดคืออะไร — สถานการณ์เลวร้ายที่สุด"
                }
            },
            "required": ["ref", "headline", "gain", "tradeoff", "chance", "why_now", "worst"],
            "additionalProperties": false
        }),
        timeout_ms: 6000,
        webull_cost: 0,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::registry::Registry;
    use crate::easy::{EasyCases, IdeaLevel};
    use crate::screener::QuoteRow;

    fn numbers() -> CardNumbers {
        CardNumbers {
            strike: 175.0,
            expiry: "2026-12-18".into(),
            days: 21.0,
            premium_usd: 183.0,
            cash_needed_usd: 17_500.0,
            breakeven: 173.17,
            assign_pct: Some(18.0),
            worst_30_usd: Some(-3508.5),
            max_loss_usd: Some(17_317.0),
        }
    }

    fn args(overrides: &[(&str, Value)]) -> Value {
        let mut base = json!({
            "ref": "NVDA261218P00175000",
            "headline": "รับเงินทันทีแลกกับการพร้อมซื้อ NVDA ในราคาที่คุณยอมรับ",
            "gain": "เงินเข้าบัญชีทันทีเมื่อขายสัญญานี้ เป็นของคุณไม่ว่าจะเกิดอะไรขึ้น",
            "tradeoff": "ถ้าวันหมดอายุราคาหุ้นต่ำกว่าที่ตกลง คุณจะซื้อหุ้น 100 หุ้นในราคานั้น",
            "chance": "โอกาสที่จะต้องซื้อหุ้นจริงๆ ไม่ถึงหนึ่งในห้า",
            "why_now": "ช่วงนี้คนยอมจ่ายค่าประกันราคาหุ้นแพงกว่าปกติ",
            "worst": "ถ้าหุ้นร่วงแรง ต้องซื้อหุ้นในราคาสูงกว่าตลาด ขาดทุนบนกระดาษ"
        });
        for (key, value) in overrides {
            base[*key] = value.clone();
        }
        base
    }

    // ---------- specs ----------

    #[test]
    fn present_spec_follows_the_conventions() {
        let specs = specs();
        assert_eq!(specs.len(), 1);
        let spec = &specs[0];
        assert_eq!(spec.name, "present_idea");
        assert_eq!(spec.tier, Tier::Ui);
        assert_eq!(spec.modes, vec![Mode::Easy, Mode::Pro]);
        assert_eq!(spec.timeout_ms, 6000);
        assert_eq!(spec.webull_cost, 0);
        assert!(spec.description.contains("ใช้เมื่อ:"), "{}", spec.description);
        assert!(
            spec.description.contains("ไม่ใช้เมื่อ:"),
            "{}",
            spec.description
        );
        assert!(
            spec.description.contains("ต้องเรียกทุกครั้งแทนการพิมพ์คำแนะนำ"),
            "{}",
            spec.description
        );
        assert_eq!(
            spec.parameters.get("additionalProperties"),
            Some(&json!(false)),
            "schema must lock its fields"
        );
        assert_eq!(spec.examples.len(), 2);
        assert!(
            spec.to_tool_def()
                .function
                .description
                .contains("ตัวอย่างการเรียก:")
        );
    }

    #[test]
    fn examples_pass_schema_validation_in_both_modes() {
        let mut reg = Registry::new();
        for spec in specs() {
            reg.register(spec).expect("register present spec");
        }
        for mode in [Mode::Easy, Mode::Pro] {
            for ex in &specs()[0].examples {
                let payload = ex.to_string();
                reg.validate("present_idea", mode, &payload)
                    .unwrap_or_else(|e| {
                        panic!(
                            "example failed validation in {mode:?}: {} — {}",
                            e.message, payload
                        )
                    });
            }
        }
    }

    // ---------- build_card: lengths ----------

    #[test]
    fn long_headline_and_answers_are_rejected_with_the_field_name() {
        let long_headline: String = "ท".repeat(MAX_HEADLINE_CHARS + 1);
        let err = build_card(
            &args(&[("headline", json!(long_headline))]),
            numbers(),
            "NVDA",
            "2026-11-27T10:00:00Z",
            &[],
        )
        .expect_err("81-char headline");
        assert_eq!(err["error"], "text_too_long");
        assert_eq!(err["field"], "headline");

        let long_answer: String = "ท".repeat(MAX_ANSWER_CHARS + 1);
        let err = build_card(
            &args(&[("chance", json!(long_answer))]),
            numbers(),
            "NVDA",
            "2026-11-27T10:00:00Z",
            &[],
        )
        .expect_err("241-char answer");
        assert_eq!(err["error"], "text_too_long");
        assert_eq!(err["field"], "chance");
        assert!(
            err["message"].as_str().unwrap().contains("คำตอบข้อ chance"),
            "answer errors name the question: {err}"
        );

        // ความยาวนับเป็นตัวอักษร ไม่ใช่ไบต์ — 80 ตัวอักษรไทย (~240 ไบต์) ผ่าน
        let ok_headline: String = "ท".repeat(MAX_HEADLINE_CHARS);
        assert!(
            build_card(
                &args(&[("headline", json!(ok_headline))]),
                numbers(),
                "NVDA",
                "2026-11-27T10:00:00Z",
                &[],
            )
            .is_ok()
        );
    }

    #[test]
    fn missing_or_blank_fields_are_bad_args() {
        for broken in [json!({}), args(&[("worst", json!("   "))])] {
            let err = build_card(&broken, numbers(), "NVDA", "2026-11-27T10:00:00Z", &[])
                .expect_err("must fail");
            assert_eq!(err["error"], "bad_args", "{broken}");
        }
    }

    // ---------- build_card: grounding ----------

    #[test]
    fn numbers_from_the_card_pass_grounding() {
        let card = build_card(
            &args(&[
                ("gain", json!("คุณได้เงินมา $183 ทันทีต่อหนึ่งสัญญา")),
                ("tradeoff", json!("ต้องกันเงินไว้ $17,500 พร้อมซื้อหุ้นที่ $175")),
                ("chance", json!("โอกาสต้องซื้อหุ้นราว 18%")),
                ("worst", json!("ถ้าหุ้นลง 30% จะขาดทุนบนกระดาษราว $3,509")),
            ]),
            numbers(),
            "NVDA",
            "2026-11-27T10:00:00Z",
            &[],
        )
        .expect("card numbers ground the answers");
        assert_eq!(card["numbers"]["premium_usd"], json!(183.0));
    }

    #[test]
    fn extra_fact_strings_ground_the_why_now_numbers() {
        // atm_iv เป็นเศษส่วน 0.48 → token "48%" อ่านใน percent space ได้
        let card = build_card(
            &args(&[("why_now", json!("ราคาสะท้อนการแกว่ง 48% ต่อปี แพงกว่าปกติ"))]),
            numbers(),
            "NVDA",
            "2026-11-27T10:00:00Z",
            &[r#"{"atm_iv": 0.48}"#],
        )
        .expect("the extra IV fact grounds 48%");
        assert_eq!(
            card["answers"]["why_now"],
            json!("ราคาสะท้อนการแกว่ง 48% ต่อปี แพงกว่าปกติ")
        );
    }

    #[test]
    fn invented_numbers_are_rejected_with_tokens_and_hint() {
        let err = build_card(
            &args(&[
                ("gain", json!("คุณได้เงินมา $999 ทันที")),
                ("headline", json!("รับ $999 ทันที ง่ายกว่าที่คิด")),
            ]),
            numbers(),
            "NVDA",
            "2026-11-27T10:00:00Z",
            &[],
        )
        .expect_err("$999 appears nowhere");
        assert_eq!(err["error"], "ungrounded_numbers");
        assert_eq!(err["tokens"], json!(["$999"]));
        assert!(err["fields"].as_array().unwrap().contains(&json!("gain")));
        assert!(
            err["fields"]
                .as_array()
                .unwrap()
                .contains(&json!("headline"))
        );
        let message = err["message"].as_str().unwrap();
        assert!(
            message.contains("เอาตัวเลขนี้ออก หรือเรียก tool ที่ให้ตัวเลขนี้ก่อน"),
            "hint tells the model how to fix it: {message}"
        );
    }

    // ---------- build_card: JSON shape ----------

    #[test]
    fn card_json_keys_match_types_ts_exactly() {
        let card = build_card(&args(&[]), numbers(), "NVDA", "2026-11-27T10:00:00Z", &[])
            .expect("card builds");
        let keys = |v: &Value| -> Vec<String> {
            let mut ks: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            ks.sort();
            ks
        };
        assert_eq!(
            keys(&card),
            ["answers", "as_of", "headline", "numbers", "ref", "symbol"]
        );
        assert_eq!(
            keys(&card["answers"]),
            ["chance", "gain", "tradeoff", "why_now", "worst"]
        );
        assert_eq!(
            keys(&card["numbers"]),
            [
                "assign_pct",
                "breakeven",
                "cash_needed_usd",
                "days",
                "expiry",
                "max_loss_usd",
                "premium_usd",
                "strike",
                "worst_30_usd",
            ]
        );
        assert_eq!(card["ref"], "NVDA261218P00175000");
        assert_eq!(card["symbol"], "NVDA");
        assert_eq!(card["as_of"], "2026-11-27T10:00:00Z");
        assert_eq!(card["numbers"]["expiry"], "2026-12-18");
    }

    // ---------- number builders ----------

    fn easy_idea() -> EasyIdea {
        EasyIdea {
            occ: "NVDA261218P00175000".into(),
            symbol: "NVDA".into(),
            level: IdeaLevel::Balanced,
            recommended: true,
            strike: 175.0,
            expiry: "2026-12-18".into(),
            days: 21.0,
            premium: 1.83,
            credit: 183.0,
            cost: 17_500.0,
            period_pct: 1.05,
            annual_pct: Some(18.2),
            breakeven: 173.17,
            below_spot_pct: 4.1,
            assign_pct: 18.0,
            cases: EasyCases {
                keep_pct: 82.0,
                between_pct: 10.0,
                loss_pct: 8.0,
            },
            worst_30_usd: -3508.53,
            max_loss_usd: 17_317.0,
            warn: false,
            cash_short: false,
        }
    }

    #[test]
    fn card_numbers_from_easy_maps_every_field() {
        let n = card_numbers_from_easy(&easy_idea());
        assert_eq!(n.strike, 175.0);
        assert_eq!(n.expiry, "2026-12-18");
        assert_eq!(n.days, 21.0);
        assert_eq!(n.premium_usd, 183.0);
        assert_eq!(n.cash_needed_usd, 17_500.0);
        assert_eq!(n.breakeven, 173.17);
        assert_eq!(n.assign_pct, Some(18.0));
        assert_eq!(n.worst_30_usd, Some(-3508.53));
        assert_eq!(n.max_loss_usd, Some(17_317.0));
    }

    fn row(is_call: bool) -> EnrichedRow {
        EnrichedRow {
            q: QuoteRow {
                symbol: if is_call {
                    "NVDA261218C00175000".into()
                } else {
                    "NVDA261218P00175000".into()
                },
                underlying: "NVDA".into(),
                expiry: chrono::NaiveDate::from_ymd_opt(2026, 12, 18).unwrap(),
                option_type: if is_call { OptType::Call } else { OptType::Put },
                strike: 175.0,
                price: 2.0,
                bid: 1.9,
                ask: 2.1,
                volume: 10.0,
                oi: 100.0,
                change: 0.0,
                change_ratio: 0.0,
                src: "API".into(),
            },
            t: 0.1,
            dte: 23.0,
            chg_pct: 0.0,
            vol_oi: None,
            iv: None,
            delta: None,
            gamma: None,
            vega: None,
            theta: None,
            pop: Some(0.8),
            credit: Some(200.0),
            collateral: if is_call { None } else { Some(17_500.0) },
            yld: None,
            ann: Some(9.0),
            be: if is_call { None } else { Some(173.0) },
            cushion: None,
            p_itm: Some(0.18),
            p_touch: None,
            sig_dist: None,
            strike_dist: None,
            day_inc: None,
            if_called: None,
            if_called_ann: None,
            basis: None,
            score: None,
            spread_pct: None,
            prem: if is_call { Some(3.0) } else { None },
            net_cash: None,
            max_loss: None,
            max_profit: None,
            grade: None,
            why: None,
        }
    }

    fn as_of() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 25).unwrap()
    }

    #[test]
    fn card_numbers_from_row_put_hand_checked() {
        let n = card_numbers_from_row(&row(false), 190.0, as_of());
        assert_eq!(n.strike, 175.0);
        assert_eq!(n.expiry, "2026-12-18");
        assert_eq!(n.days, 23.0, "calendar days to expiry");
        assert_eq!(n.premium_usd, 200.0, "premium_usd = credit");
        assert_eq!(n.cash_needed_usd, 17_500.0, "collateral");
        assert_eq!(n.breakeven, 173.0);
        assert_eq!(n.assign_pct, Some(18.0), "p_itm × 100");
        // worst 30%: 0.7×190 = 133 → intrinsic 42/share, credit 2/share,
        // fees ไป-กลับ = (0.55×1.07 + 0.025 + 0.05)×2 = 1.327
        // → 100×(2 − 42) − 1.327 = −4001.33
        assert!((n.worst_30_usd.unwrap() - (-4001.33)).abs() < 1e-9);
        assert_eq!(n.max_loss_usd, Some(17_300.0), "strike×100 − credit");
    }

    #[test]
    fn card_numbers_from_row_call_is_uncapped_and_uses_spot_capital() {
        let n = card_numbers_from_row(&row(true), 190.0, as_of());
        assert_eq!(n.max_loss_usd, None, "a short call has no capped max loss");
        assert_eq!(
            n.cash_needed_usd, 19_000.0,
            "no collateral → 100 shares at spot"
        );
        assert_eq!(
            n.breakeven, 178.0,
            "be missing → strike + premium per share"
        );
        // 0.7×190 = 133 ต่ำกว่า strike → call หมดอายุไร้ค่า: 300 − 1.327
        assert!((n.worst_30_usd.unwrap() - 298.67).abs() < 1e-9);
        // null ต้องไปอยู่ใน JSON ของการ์ดเป็น null จริงๆ ไม่ใช่หายไป
        let card =
            build_card(&args(&[]), n, "NVDA", "2026-11-27T10:00:00Z", &[]).expect("card builds");
        assert_eq!(card["numbers"]["max_loss_usd"], Value::Null);
        assert!(
            card["numbers"].get("max_loss_usd").is_some(),
            "key stays present"
        );
    }
}
