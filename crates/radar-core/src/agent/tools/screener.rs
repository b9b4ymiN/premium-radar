//! Screener tools — the three read-only tools behind
//! `copilot::screener_tools()` re-expressed as ToolSpecs for the agent
//! registry. Names and parameters stay identical so `copilot::execute_tool`
//! keeps working against calls validated here.

use serde_json::json;

use crate::agent::contract::{Mode, Tier, ToolSpec};

pub fn screener_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "top_candidates".into(),
            tier: Tier::Read,
            modes: vec![Mode::Easy, Mode::Pro],
            description: "ดูรายชื่อสัญญาที่ผ่านตัวคัดกรองของแอปในตอนนี้ เรียงจากน่าสนใจที่สุดมาก่อน พร้อมตัวเลขสรุปของแต่ละตัว (ค่าพรีเมียม ผลตอบแทนต่อปี โอกาสสำเร็จ โอกาสโดน assign คือต้องซื้อหรือขายหุ้นจริง)\nใช้เมื่อ: ผู้ใช้อยากรู้ว่าตอนนี้มีสัญญาอะไรน่าสนใจบ้าง หรือขอดูอันดับ top N\nไม่ใช้เมื่อ: อยากดูรายละเอียดครบของสัญญาใดสัญญาหนึ่ง (ใช้ contract_detail) หรืออยากเข้าใจภาพรวมของหุ้นทั้งตัว (ใช้ chain_stats)".into(),
            examples: vec![json!({"count": 5, "type": "put"}), json!({})],
            parameters: json!({
                "type": "object",
                "properties": {
                    "count": {
                        "type": "integer",
                        "description": "จำนวนสัญญาที่ต้องการดู (1-20)",
                        "default": 5,
                        "minimum": 1,
                        "maximum": 20
                    },
                    "type": {
                        "type": "string",
                        "enum": ["all", "put", "call"],
                        "description": "คัดเฉพาะชนิดของสัญญาที่สนใจ",
                        "default": "all"
                    }
                },
                "additionalProperties": false
            }),
            timeout_ms: 5000,
            webull_cost: 0,
        },
        ToolSpec {
            name: "contract_detail".into(),
            tier: Tier::Read,
            modes: vec![Mode::Easy, Mode::Pro],
            description: "ดูรายละเอียดครบทุกด้านของสัญญาหนึ่งตัวจากรหัสสัญญา (OCC symbol) เช่น จุดคุ้มทุน ระยะห่างจากราคาหุ้นปัจจุบัน และเหตุผลของเกรดที่ได้\nใช้เมื่อ: ผู้ใช้ถามเจาะลึกสัญญาที่ระบุชัดเจน หรืออยากได้ตัวเลขที่ไม่ได้อยู่ในรายการสรุปของ top_candidates\nไม่ใช้เมื่อ: ยังไม่รู้ว่าสนใจสัญญาไหน — เรียก top_candidates เพื่อหาสัญญาที่น่าสนใจก่อน".into(),
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
            modes: vec![Mode::Easy, Mode::Pro],
            description: "ดูภาพรวมของหุ้นที่แอปกำลังเปิดดูอยู่ทั้งตัว: ความผันผวนที่ตลาดคาด ช่วงราคาที่คาดว่าจะเคลื่อนไหว และการเทียบสัดส่วนฝั่งพุตกับฝั่งคอล\nใช้เมื่อ: ผู้ใช้อยากเข้าใจบรรยากาศตลาดของหุ้นนี้ก่อนลงรายละเอียดสัญญา\nไม่ใช้เมื่อ: อยากได้ข้อมูลระดับสัญญา (ใช้ top_candidates หรือ contract_detail)".into(),
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
    use crate::agent::registry::Registry;

    #[test]
    fn three_specs_with_names_matching_copilot_screener_tools() {
        let specs = screener_specs();
        assert_eq!(specs.len(), 3);
        let copilot_names: Vec<String> = crate::copilot::screener_tools()
            .iter()
            .map(|t| t.function.name.clone())
            .collect();
        let spec_names: Vec<String> = specs.iter().map(|s| s.name.clone()).collect();
        assert_eq!(copilot_names, spec_names);
    }

    #[test]
    fn specs_follow_the_tool_conventions() {
        for spec in screener_specs() {
            assert_eq!(spec.tier, Tier::Read);
            assert_eq!(spec.modes, vec![Mode::Easy, Mode::Pro]);
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
    fn every_example_passes_validation_in_both_modes() {
        let specs = screener_specs();
        let mut reg = Registry::new();
        for spec in &specs {
            reg.register(spec.clone()).expect("register screener spec");
        }
        for mode in [Mode::Easy, Mode::Pro] {
            for spec in &specs {
                for ex in &spec.examples {
                    let args = ex.to_string();
                    let validated = reg.validate(&spec.name, mode, &args).unwrap_or_else(|e| {
                        panic!("example {args} of {} failed: {}", spec.name, e.message)
                    });
                    assert!(validated.is_object());
                }
            }
        }
    }
}
