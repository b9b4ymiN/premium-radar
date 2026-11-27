//! Typed tool contract shared by every agent tool.
//!
//! A [`ToolSpec`] is the single source of truth for one tool: schema,
//! description, examples, risk tier and which UI modes expose it. The
//! registry turns specs into the OpenAI-compatible `copilot::ToolDef`
//! list and validates model-supplied arguments against the schema.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::copilot::{ToolDef, ToolFunction};

/// Risk tier — what a tool may do and how the harness treats it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Read-only data access (screener, portfolio, playbooks).
    Read,
    /// Navigate/highlight the UI without changing data.
    Ui,
    /// Persist local changes (watchlists, notes) — needs user confirm.
    WriteLocal,
    /// Live trading — reserved for W4, never registered in this plan.
    Trade,
}

/// UI mode the tool is exposed in (settings `ui_mode`: "easy" | "pro").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Easy,
    Pro,
}

impl Mode {
    /// Settings strings are exactly "easy"/"pro" (see `normalise_ui_mode`
    /// in src-tauri). Anything that is not "pro" falls back to Easy so an
    /// unknown string never hides tools from the simpler mode.
    pub fn from_ui_mode(s: &str) -> Mode {
        if s == "pro" { Mode::Pro } else { Mode::Easy }
    }
}

/// One tool: name, schema, description, examples, tier, modes, budget.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub tier: Tier,
    pub modes: Vec<Mode>,
    pub description: String,
    pub examples: Vec<Value>,
    pub parameters: Value,
    pub timeout_ms: u64,
    pub webull_cost: u32,
}

impl ToolSpec {
    /// Render into the OpenAI-compatible tool definition the model sees.
    ///
    /// The target models are OpenAI-compatible without native
    /// `input_examples` support and pick tools from name+description
    /// alone, so examples are folded into the description text as one
    /// compact JSON line each.
    pub fn to_tool_def(&self) -> ToolDef {
        let mut description = self.description.clone();
        if !self.examples.is_empty() {
            description.push_str("\n\nตัวอย่างการเรียก:\n");
            let lines: Vec<String> = self.examples.iter().map(|ex| ex.to_string()).collect();
            description.push_str(&lines.join("\n"));
        }
        ToolDef {
            r#type: "function".into(),
            function: ToolFunction {
                name: self.name.clone(),
                description,
                parameters: self.parameters.clone(),
            },
        }
    }
}

/// Why a tool call was rejected — machine-readable for the harness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolErrorCode {
    UnknownTool,
    NotInMode,
    BadJson,
    NotObject,
    MissingField,
    UnknownField,
    WrongType,
    NotInEnum,
    OutOfRange,
}

impl ToolErrorCode {
    /// Stable snake_case wire name used in `to_model_json`.
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolErrorCode::UnknownTool => "unknown_tool",
            ToolErrorCode::NotInMode => "not_in_mode",
            ToolErrorCode::BadJson => "bad_json",
            ToolErrorCode::NotObject => "not_object",
            ToolErrorCode::MissingField => "missing_field",
            ToolErrorCode::UnknownField => "unknown_field",
            ToolErrorCode::WrongType => "wrong_type",
            ToolErrorCode::NotInEnum => "not_in_enum",
            ToolErrorCode::OutOfRange => "out_of_range",
        }
    }
}

/// Validation failure for a model tool call — the Thai message always
/// says how to fix the call so the model can retry without user help.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolError {
    pub code: ToolErrorCode,
    pub message: String,
}

impl ToolError {
    /// Compact JSON sent back to the model as the `tool` message payload.
    pub fn to_model_json(&self) -> String {
        serde_json::json!({
            "error": self.code.as_str(),
            "message": self.message,
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(examples: Vec<Value>) -> ToolSpec {
        ToolSpec {
            name: "demo_tool".into(),
            tier: Tier::Read,
            modes: vec![Mode::Easy, Mode::Pro],
            description: "รายละเอียด".into(),
            examples,
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            timeout_ms: 1000,
            webull_cost: 0,
        }
    }

    #[test]
    fn from_ui_mode_maps_pro_and_defaults_everything_else_to_easy() {
        assert_eq!(Mode::from_ui_mode("pro"), Mode::Pro);
        assert_eq!(Mode::from_ui_mode("easy"), Mode::Easy);
        assert_eq!(Mode::from_ui_mode(""), Mode::Easy);
        // case-sensitive, matching the settings normaliser
        assert_eq!(Mode::from_ui_mode("PRO"), Mode::Easy);
        assert_eq!(Mode::from_ui_mode("unknown"), Mode::Easy);
    }

    #[test]
    fn to_tool_def_renders_examples_into_description() {
        let def = spec(vec![
            serde_json::json!({"count": 5, "type": "put"}),
            serde_json::json!({}),
        ])
        .to_tool_def();
        assert_eq!(def.r#type, "function");
        assert_eq!(def.function.name, "demo_tool");
        assert!(def.function.description.starts_with("รายละเอียด"));
        assert!(def.function.description.contains("ตัวอย่างการเรียก:"));
        // one compact JSON line per example, not pretty-printed
        assert!(
            def.function
                .description
                .contains("{\"count\":5,\"type\":\"put\"}")
        );
        assert!(def.function.description.contains("\n{}"));
    }

    #[test]
    fn to_tool_def_omits_examples_block_when_empty() {
        let def = spec(vec![]).to_tool_def();
        assert_eq!(def.function.description, "รายละเอียด");
        assert!(!def.function.description.contains("ตัวอย่างการเรียก"));
    }

    #[test]
    fn tool_error_json_carries_snake_code_and_thai_message() {
        let e = ToolError {
            code: ToolErrorCode::NotInEnum,
            message: "เลือกค่าจาก: a, b".into(),
        };
        let v: Value = serde_json::from_str(&e.to_model_json()).expect("valid json");
        assert_eq!(v["error"], "not_in_enum");
        assert_eq!(v["message"], "เลือกค่าจาก: a, b");
    }

    #[test]
    fn tier_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&Tier::WriteLocal).unwrap_or_default(),
            "\"write_local\""
        );
        assert_eq!(
            serde_json::to_string(&Tier::Ui).unwrap_or_default(),
            "\"ui\""
        );
    }
}
