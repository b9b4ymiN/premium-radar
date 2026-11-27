//! Screener tools for the agent registry. `top_candidates` is no longer an
//! agent tool (the agent's `scan_chain` in the ideas family replaces it —
//! it applies the filters and returns the same formatter's rows); the
//! legacy copilot chat keeps `copilot::screener_tools()` until P4.

use serde_json::json;

use crate::agent::contract::{Mode, Tier, ToolSpec};

pub fn screener_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "contract_detail".into(),
            tier: Tier::Read,
            modes: vec![Mode::Easy, Mode::Pro],
            description: "ดูรายละเอียดครบทุกด้านของสัญญาหนึ่งตัวจากรหัสสัญญา (OCC symbol) เช่น จุดคุ้มทุน ระยะห่างจากราคาหุ้นปัจจุบัน และเหตุผลของเกรดที่ได้\nใช้เมื่อ: ผู้ใช้ถามเจาะลึกสัญญาที่ระบุชัดเจน หรืออยากได้ตัวเลขที่ไม่ได้อยู่ในรายการสรุปของ find_ideas / scan_chain\nไม่ใช้เมื่อ: ยังไม่รู้ว่าสนใจสัญญาไหน — เรียก find_ideas หรือ scan_chain เพื่อหาสัญญาที่น่าสนใจก่อน".into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::contract::ToolErrorCode;
    use crate::agent::registry::Registry;

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
}
