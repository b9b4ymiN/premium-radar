//! Agent tools — `playbook` family (W6 P2): the app's Thai how-to
//! playbooks, embedded with `include_str!` so the agent can read the house
//! style before answering. Pure module — no I/O, the clock is injected.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::agent::contract::{Mode, Tier, ToolSpec};

/// Every playbook the `read_playbook` enum accepts, in display order.
pub const NAMES: [&str; 5] = [
    "five-answers",
    "assignment",
    "oi-trader",
    "tone-easy",
    "tone-pro",
];

fn playbook(name: &str) -> Option<(&'static str, &'static str)> {
    match name {
        "five-answers" => Some((
            "อธิบายไอเดียด้วยคำถาม 5 ข้อ",
            include_str!("../playbooks/five-answers.md"),
        )),
        "assignment" => Some((
            "การถูก assign คืออะไร (เวอร์ชันภาษาง่าย)",
            include_str!("../playbooks/assignment.md"),
        )),
        "oi-trader" => Some((
            "อ่าน Open Interest และ Volume เป็นบริบทเท่านั้น",
            include_str!("../playbooks/oi-trader.md"),
        )),
        "tone-easy" => Some(("น้ำเสียงโหมด Easy", include_str!("../playbooks/tone-easy.md"))),
        "tone-pro" => Some(("น้ำเสียงโหมด Pro", include_str!("../playbooks/tone-pro.md"))),
        _ => None,
    }
}

pub fn specs() -> Vec<ToolSpec> {
    let enum_values: Vec<Value> = NAMES.iter().map(|n| json!(n)).collect();
    vec![ToolSpec {
        name: "read_playbook".into(),
        tier: Tier::Read,
        modes: vec![Mode::Easy, Mode::Pro],
        description: "เปิดคู่มือวิธีตอบของแอป (playbook) หนึ่งเล่ม: วิธีอธิบายไอเดียด้วยคำถาม 5 ข้อ (five-answers) ความหมายของการถูก assign (assignment) วิธีอ่าน OI เป็นบริบท (oi-trader) และน้ำเสียงของโหมด Easy / Pro (tone-easy, tone-pro)\nใช้เมื่อ: ก่อนอธิบายไอเดียหรือตอบเรื่องการถูก assign เพื่อทำตามมาตรฐานถ้อยคำของแอป หรือเมื่อผู้ใช้ถามว่าแอปตอบแบบไหน\nไม่ใช้เมื่อ: ต้องการตัวเลขของสัญญาหรือพอร์ต — playbook เป็นคู่มือถ้อยคำ ตัวเลขต้องมาจาก tool ข้อมูลเสมอ".into(),
        examples: vec![json!({"name": "five-answers"}), json!({"name": "tone-easy"})],
        parameters: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "enum": enum_values,
                    "description": "ชื่อ playbook ที่จะอ่าน"
                }
            },
            "required": ["name"],
            "additionalProperties": false
        }),
        timeout_ms: 5000,
        webull_cost: 0,
    }]
}

/// The `read_playbook` answer: the embedded markdown plus its Thai title.
/// Unknown names (impossible after registry validation) still answer with a
/// Thai error listing the valid values.
pub fn summarize_playbook(name: &str, as_of: DateTime<Utc>) -> Result<Value, String> {
    match playbook(name) {
        Some((title, content)) => Ok(json!({
            "name": name,
            "title": title,
            "content": content,
            "as_of": as_of.to_rfc3339(),
            "source": "app",
        })),
        None => Err(format!(
            "ไม่รู้จัก playbook \"{}\" — เลือกค่า name จาก: {} แล้วเรียกใหม่",
            name,
            NAMES.join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::registry::Registry;

    fn as_of() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-27T02:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn spec_follows_the_tool_conventions() {
        let specs = specs();
        assert_eq!(specs.len(), 1);
        let spec = &specs[0];
        assert_eq!(spec.name, "read_playbook");
        assert_eq!(spec.tier, Tier::Read);
        assert_eq!(spec.modes, vec![Mode::Easy, Mode::Pro]);
        assert_eq!(spec.timeout_ms, 5000);
        assert_eq!(spec.webull_cost, 0);
        assert!(spec.description.contains("ใช้เมื่อ:"));
        assert!(spec.description.contains("ไม่ใช้เมื่อ:"));
        assert_eq!(
            spec.parameters.get("additionalProperties"),
            Some(&json!(false))
        );
        let enum_values: Vec<&str> = spec.parameters["properties"]["name"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(enum_values, NAMES.to_vec());
        assert!(
            spec.to_tool_def()
                .function
                .description
                .contains("ตัวอย่างการเรียก:")
        );
    }

    #[test]
    fn every_example_passes_validation_in_both_modes() {
        let mut reg = Registry::new();
        for spec in specs() {
            reg.register(spec).expect("register playbook spec");
        }
        for mode in [Mode::Easy, Mode::Pro] {
            for ex in &specs()[0].examples {
                let args = ex.to_string();
                let validated = reg
                    .validate("read_playbook", mode, &args)
                    .unwrap_or_else(|e| panic!("example {args} failed: {}", e.message));
                assert!(validated.is_object());
            }
        }
        // an off-enum name is rejected with the valid values listed
        let err = reg
            .validate("read_playbook", Mode::Easy, r#"{"name": "nope"}"#)
            .expect_err("off-enum rejected");
        assert_eq!(err.code, crate::agent::ToolErrorCode::NotInEnum);
        assert!(err.message.contains("five-answers"), "{}", err.message);
    }

    #[test]
    fn every_enum_value_maps_to_a_non_empty_playbook() {
        for name in NAMES {
            let out = summarize_playbook(name, as_of()).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(out["name"], name);
            let content = out["content"].as_str().unwrap();
            assert!(!content.trim().is_empty(), "{name} embedded empty");
            assert!(content.lines().count() >= 40, "{name} too short");
            assert!(content.lines().count() <= 120, "{name} too long");
            assert!(!out["title"].as_str().unwrap().is_empty());
            assert_eq!(out["source"], "app");
            // the two hard rules every playbook must restate
            assert!(
                content.contains("ห้ามคำนวณ"),
                "{name} must forbid model-side math"
            );
            assert!(
                content.contains("สั่งซื้อขาย"),
                "{name} must say the app cannot place orders"
            );
        }
    }

    #[test]
    fn unknown_name_answers_with_the_valid_values() {
        let err = summarize_playbook("nope", as_of()).expect_err("unknown name");
        for name in NAMES {
            assert!(err.contains(name), "{name} missing from: {err}");
        }
    }
}
