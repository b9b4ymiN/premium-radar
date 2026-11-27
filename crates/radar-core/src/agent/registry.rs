//! Tool registry: registration, per-mode filtering and argument validation.
//!
//! # Supported schema subset
//!
//! `validate` understands exactly the keywords our specs use:
//! `type` (object / string / number / integer / boolean / array),
//! `properties`, `required`, `enum`, `minimum`, `maximum`, `items` and
//! `additionalProperties: false`. `default` is applied to missing
//! optional top-level fields in the returned value.
//!
//! Unsupported schema keywords (`pattern`, `oneOf`, `format`,
//! `minItems`, …) and unsupported `type` names are ignored rather than
//! rejected — specs are authored against this subset on purpose, and an
//! unknown keyword must not brick a tool call.

use serde_json::{Map, Value};

use super::contract::{Mode, ToolError, ToolErrorCode, ToolSpec};
use crate::copilot::ToolDef;

/// Registry of tool specs, filtered per UI mode for the model.
pub struct Registry {
    specs: Vec<ToolSpec>,
}

impl Registry {
    pub fn new() -> Self {
        Self { specs: Vec::new() }
    }

    /// Register a spec. Rejects duplicate names and names outside
    /// `^[a-z][a-z0-9_]{1,40}$` — the wire contract the model sees must
    /// stay lowercase-ASCII stable.
    pub fn register(&mut self, spec: ToolSpec) -> Result<(), String> {
        if !is_valid_tool_name(&spec.name) {
            return Err(format!(
                "ชื่อ tool \"{}\" ไม่ผ่านรูปแบบ ^[a-z][a-z0-9_]{{1,40}}$ — ใช้ตัวพิมพ์เล็กล้วน ขึ้นต้นด้วยตัวอักษร ยาว 2-41 ตัวอักษร",
                spec.name
            ));
        }
        if self.get(&spec.name).is_some() {
            return Err(format!(
                "ลงทะเบียน tool \"{}\" ซ้ำ — ชื่อนี้มีใน registry อยู่แล้ว",
                spec.name
            ));
        }
        self.specs.push(spec);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&ToolSpec> {
        self.specs.iter().find(|s| s.name == name)
    }

    /// Specs visible in `mode`, in registration order.
    pub fn for_mode(&self, mode: Mode) -> Vec<&ToolSpec> {
        self.specs
            .iter()
            .filter(|s| s.modes.contains(&mode))
            .collect()
    }

    /// OpenAI-compatible tool definitions for `mode`.
    pub fn tool_defs(&self, mode: Mode) -> Vec<ToolDef> {
        self.for_mode(mode)
            .iter()
            .map(|s| s.to_tool_def())
            .collect()
    }

    /// Validate a model-supplied call: name (mode-aware), JSON shape and
    /// schema. Returns the parsed object with top-level defaults applied.
    pub fn validate(&self, name: &str, mode: Mode, arguments: &str) -> Result<Value, ToolError> {
        let Some(spec) = self.get(name) else {
            let available: Vec<&str> = self
                .for_mode(mode)
                .iter()
                .map(|s| s.name.as_str())
                .collect();
            let listing = if available.is_empty() {
                "(ไม่มี tool ให้ใช้ในโหมดนี้เลย)".to_string()
            } else {
                available.join(", ")
            };
            return Err(ToolError {
                code: ToolErrorCode::UnknownTool,
                message: format!(
                    "ไม่รู้จัก tool \"{name}\" — เรียกเฉพาะ tool ที่ใช้ได้ในโหมดนี้: {listing} แล้วลองใหม่"
                ),
            });
        };
        if !spec.modes.contains(&mode) {
            let usable: Vec<&str> = spec.modes.iter().map(|m| mode_name(*m)).collect();
            return Err(ToolError {
                code: ToolErrorCode::NotInMode,
                message: format!(
                    "tool \"{name}\" ใช้ได้เฉพาะในโหมด {} — อย่าเรียก tool นี้ในโหมดปัจจุบัน ให้เลือก tool อื่นที่ใช้ได้ในโหมดนี้แทน",
                    usable.join(" หรือ ")
                ),
            });
        }

        // Models (and OpenAI itself) may send "" for no-argument calls.
        let trimmed = arguments.trim();
        let parsed: Value = if trimmed.is_empty() {
            Value::Object(Map::new())
        } else {
            serde_json::from_str(trimmed).map_err(|e| ToolError {
                code: ToolErrorCode::BadJson,
                message: format!(
                    "arguments ไม่ใช่ JSON ที่ถูกต้อง: {e} — ส่ง JSON object ที่ปิดวงเล็บครบ เช่น {{\"count\": 5}} แล้วเรียกใหม่"
                ),
            })?
        };
        let Value::Object(mut map) = parsed else {
            return Err(ToolError {
                code: ToolErrorCode::NotObject,
                message: format!(
                    "arguments ต้องเป็น JSON object เช่น {{\"count\": 5}} (ส่งมาเป็น {} ค่า {parsed}) — ครอบค่าด้วยเครื่องหมาย {{ }} พร้อมชื่อช่องตาม schema แล้วเรียกใหม่",
                    value_kind(&parsed)
                ),
            });
        };

        validate_value(&Value::Object(map.clone()), &spec.parameters, "")?;

        // Defaults for missing optional top-level fields only — nested
        // defaults are not part of the subset. Required fields missing
        // have already failed validation above.
        if let Some(props) = spec.parameters.get("properties").and_then(Value::as_object) {
            for (key, prop_schema) in props {
                if let Some(default) = prop_schema.get("default") {
                    map.entry(key.clone()).or_insert_with(|| default.clone());
                }
            }
        }
        Ok(Value::Object(map))
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Easy => "easy",
        Mode::Pro => "pro",
    }
}

/// `^[[a-z][a-z0-9_]{1,40}$` — pure ASCII pattern, so byte inspection is
/// exact: any non-ASCII byte simply fails the allowed-character check.
fn is_valid_tool_name(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 2
        && b.len() <= 41
        && b[0].is_ascii_lowercase()
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

/// Validate one value against one schema node, recursing into object
/// fields and array items. `path` is the dotted/indexed field path used
/// in error messages ("" at the root, e.g. `steps[1].anchor` nested).
fn validate_value(value: &Value, schema: &Value, path: &str) -> Result<(), ToolError> {
    let Some(obj) = schema.as_object() else {
        return Ok(()); // nothing constrainable
    };

    if let Some(expected) = obj.get("type").and_then(Value::as_str) {
        check_type(value, expected, path)?;
    }

    if let Some(allowed) = obj.get("enum").and_then(Value::as_array)
        && !allowed.is_empty()
        && !allowed.iter().any(|a| a == value)
    {
        let list: Vec<String> = allowed.iter().map(|a| a.to_string()).collect();
        return Err(ToolError {
            code: ToolErrorCode::NotInEnum,
            message: format!(
                "ค่าของช่อง {} ไม่ตรงกับตัวเลือกที่รองรับ (ส่งมา {value}) — เลือกค่าจาก: {} แล้วเรียกใหม่",
                label(path),
                list.join(", ")
            ),
        });
    }

    if value.is_number() {
        if let Some(min) = obj.get("minimum")
            && value
                .as_f64()
                .is_some_and(|v| v < min.as_f64().unwrap_or(f64::MIN))
        {
            return Err(out_of_range(path, value, "ต้องมากกว่าหรือเท่ากับ", min));
        }
        if let Some(max) = obj.get("maximum")
            && value
                .as_f64()
                .is_some_and(|v| v > max.as_f64().unwrap_or(f64::MAX))
        {
            return Err(out_of_range(path, value, "ต้องน้อยกว่าหรือเท่ากับ", max));
        }
    }

    if let Some(map) = value.as_object() {
        validate_object_fields(map, obj, path)?;
    }

    if let (Some(items), Some(arr)) = (obj.get("items"), value.as_array()) {
        for (i, item) in arr.iter().enumerate() {
            validate_value(item, items, &format!("{path}[{i}]"))?;
        }
    }

    Ok(())
}

/// Object-level checks: unknown fields (when `additionalProperties: false`)
/// and required fields, recursing into known properties.
fn validate_object_fields(
    map: &Map<String, Value>,
    obj: &Map<String, Value>,
    path: &str,
) -> Result<(), ToolError> {
    let props = obj.get("properties").and_then(Value::as_object);
    let reject_extra = obj.get("additionalProperties") == Some(&Value::Bool(false));

    // Check fields before required so a misspelled field reports once with
    // the allowed names, instead of two rounds (missing + unknown).
    for (key, val) in map {
        let child = join_path(path, key);
        match props.and_then(|p| p.get(key)) {
            Some(prop_schema) => validate_value(val, prop_schema, &child)?,
            None if reject_extra => {
                let allowed: Vec<&str> = props
                    .map(|p| p.keys().map(|k| k.as_str()).collect())
                    .unwrap_or_default();
                let listing = if allowed.is_empty() {
                    "tool นี้ไม่มีช่องรับค่า ส่ง {} เปล่าๆ".to_string()
                } else {
                    format!("ใช้เฉพาะช่องที่รองรับ: {}", allowed.join(", "))
                };
                return Err(ToolError {
                    code: ToolErrorCode::UnknownField,
                    message: format!(
                        "ไม่รู้จักช่อง {} — เอาช่องนี้ออก {listing} แล้วเรียกใหม่",
                        label(&child)
                    ),
                });
            }
            None => {} // extra keys tolerated when additionalProperties isn't false
        }
    }

    if let Some(required) = obj.get("required").and_then(Value::as_array) {
        for req in required {
            if let Some(name) = req.as_str()
                && !map.contains_key(name)
            {
                return Err(ToolError {
                    code: ToolErrorCode::MissingField,
                    message: format!(
                        "ขาดช่องที่ต้องมี {} — เพิ่มช่องนี้พร้อมค่าที่ถูกต้องตาม schema แล้วเรียกใหม่",
                        label(&join_path(path, name))
                    ),
                });
            }
        }
    }

    Ok(())
}

fn check_type(value: &Value, expected: &str, path: &str) -> Result<(), ToolError> {
    let matches = match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => is_integer_value(value),
        "boolean" => value.is_boolean(),
        _ => true, // unsupported type name — ignored, see module docs
    };
    if matches {
        return Ok(());
    }
    Err(ToolError {
        code: ToolErrorCode::WrongType,
        message: format!(
            "ช่อง {} ต้องเป็น{} แต่ส่งมาเป็น {} ({value}) — {}",
            label(path),
            expected_thai(expected),
            value_kind(value),
            fix_hint(expected),
        ),
    })
}

/// Whole-number floats count as integers: JSON has no int/float split at
/// the wire and models routinely emit `5.0` for integer fields.
fn is_integer_value(value: &Value) -> bool {
    match value {
        Value::Number(n) => n
            .as_f64()
            .map(|f| f.is_finite() && f.fract() == 0.0)
            .unwrap_or(false),
        _ => false,
    }
}

fn out_of_range(path: &str, value: &Value, rule: &str, bound: &Value) -> ToolError {
    ToolError {
        code: ToolErrorCode::OutOfRange,
        message: format!(
            "ค่าของช่อง {} อยู่นอกช่วงที่รับ (ส่งมา {value} แต่{rule} {bound}) — ปรับค่าให้อยู่ในช่วงที่กำหนดแล้วเรียกใหม่",
            label(path)
        ),
    }
}

fn expected_thai(expected: &str) -> &'static str {
    match expected {
        "object" => "object (ข้อมูลในเครื่องหมาย { })",
        "array" => "array (รายการในเครื่องหมาย [ ])",
        "string" => "string (ข้อความ)",
        "number" => "number (ตัวเลข มีเศษได้ เช่น 5.5)",
        "integer" => "integer (จำนวนเต็ม เช่น 5)",
        "boolean" => "boolean (true หรือ false)",
        // unsupported type names never reach a WrongType error, so the
        // exact label is irrelevant — keep the signature 'static
        _ => "ค่าตามชนิดที่ schema ระบุ",
    }
}

fn fix_hint(expected: &str) -> &'static str {
    match expected {
        "integer" => "เปลี่ยนค่าเป็นจำนวนเต็มที่ไม่มีเครื่องหมายคำพูดและไม่มีเศษทศนิยม แล้วเรียกใหม่",
        "number" => "เปลี่ยนค่าเป็นตัวเลขที่ไม่มีเครื่องหมายคำพูด แล้วเรียกใหม่",
        "string" => "ครอบค่าด้วยเครื่องหมายคำพูดสองอัน แล้วเรียกใหม่",
        "boolean" => "ส่ง true หรือ false (ไม่ใช่ \"true\") แล้วเรียกใหม่",
        "array" => "ส่งค่าเป็นรายการในเครื่องหมาง [ ] แล้วเรียกใหม่",
        "object" => "ส่งค่าเป็นข้อมูลในเครื่องหมาง { } แล้วเรียกใหม่",
        _ => "แก้ค่าให้ตรงกับชนิดที่ schema ระบุ แล้วเรียกใหม่",
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn join_path(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

fn label(path: &str) -> String {
    if path.is_empty() {
        "\"arguments\"".to_string()
    } else {
        format!("\"{path}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::contract::Tier;
    use serde_json::json;

    /// Screener specs plus synthetic mode-exclusive tools (the nested
    /// `steps` schema doubles as the recursive-validation fixture).
    fn test_registry() -> Registry {
        let mut reg = Registry::new();
        // The agent no longer ships `top_candidates` (scan_chain replaced
        // it), but these tests exercise defaults/enum/bounds against that
        // exact shape — so they carry their own local copy, registered
        // first to keep the per-mode order assertions stable.
        reg.register(ToolSpec {
            name: "top_candidates".into(),
            tier: Tier::Read,
            modes: vec![Mode::Easy, Mode::Pro],
            description: "ทดสอบ".into(),
            examples: vec![],
            parameters: json!({
                "type": "object",
                "properties": {
                    "count": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 20,
                        "default": 5
                    },
                    "type": {
                        "type": "string",
                        "enum": ["all", "put", "call"],
                        "default": "all"
                    }
                },
                "additionalProperties": false
            }),
            timeout_ms: 1000,
            webull_cost: 0,
        })
        .expect("register top_candidates fixture");
        for spec in crate::agent::tools::screener_specs() {
            reg.register(spec).expect("register screener spec");
        }
        reg.register(ToolSpec {
            name: "easy_only_tool".into(),
            tier: Tier::Read,
            modes: vec![Mode::Easy],
            description: "ทดสอบ".into(),
            examples: vec![],
            parameters: json!({
                "type": "object",
                "properties": {
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "anchor": {"type": "string"}
                            },
                            "required": ["anchor"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["steps"],
                "additionalProperties": false
            }),
            timeout_ms: 1000,
            webull_cost: 0,
        })
        .expect("register easy tool");
        reg.register(ToolSpec {
            name: "pro_only_tool".into(),
            tier: Tier::Read,
            modes: vec![Mode::Pro],
            description: "ทดสอบ".into(),
            examples: vec![],
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            timeout_ms: 1000,
            webull_cost: 0,
        })
        .expect("register pro tool");
        reg
    }

    fn err_of(res: Result<Value, ToolError>) -> ToolError {
        match res {
            Ok(v) => panic!("expected error, got ok: {v}"),
            Err(e) => e,
        }
    }

    #[test]
    fn register_rejects_duplicate_names() {
        let mut reg = Registry::new();
        let spec = crate::agent::tools::screener_specs()
            .into_iter()
            .next()
            .expect("at least one spec");
        reg.register(spec.clone()).expect("first registration ok");
        let msg = reg.register(spec).expect_err("duplicate rejected");
        assert!(msg.contains("ซ้ำ"), "got: {msg}");
    }

    #[test]
    fn register_rejects_invalid_names() {
        let mut reg = Registry::new();
        for bad in [
            "",              // empty
            "a",             // too short (needs 2-41 chars)
            "1abc",          // starts with a digit
            "Bad_Name",      // uppercase
            "a-b",           // hyphen not allowed
            &"a".repeat(42), // too long
        ] {
            let spec = ToolSpec {
                name: bad.into(),
                tier: Tier::Read,
                modes: vec![Mode::Easy],
                description: String::new(),
                examples: vec![],
                parameters: json!({"type": "object"}),
                timeout_ms: 1000,
                webull_cost: 0,
            };
            assert!(
                reg.register(spec).is_err(),
                "name \"{bad}\" should be rejected"
            );
        }
        // boundary lengths pass
        for good in ["ab", &"a".repeat(41)] {
            let spec = ToolSpec {
                name: good.into(),
                tier: Tier::Read,
                modes: vec![Mode::Easy],
                description: String::new(),
                examples: vec![],
                parameters: json!({"type": "object"}),
                timeout_ms: 1000,
                webull_cost: 0,
            };
            assert!(reg.register(spec).is_ok(), "name \"{good}\" should pass");
        }
    }

    #[test]
    fn for_mode_filters_easy_only_and_pro_only_in_registration_order() {
        let reg = test_registry();
        let easy: Vec<&str> = reg
            .for_mode(Mode::Easy)
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        let pro: Vec<&str> = reg
            .for_mode(Mode::Pro)
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(
            easy,
            vec!["top_candidates", "contract_detail", "easy_only_tool"]
        );
        assert_eq!(
            pro,
            vec![
                "top_candidates",
                "contract_detail",
                "chain_stats",
                "pro_only_tool"
            ]
        );
    }

    #[test]
    fn tool_defs_render_per_mode_as_function_type() {
        let reg = test_registry();
        let easy_defs = reg.tool_defs(Mode::Easy);
        let easy: Vec<&str> = easy_defs.iter().map(|d| d.function.name.as_str()).collect();
        assert!(easy.contains(&"easy_only_tool"));
        assert!(!easy.contains(&"pro_only_tool"));
        assert!(
            reg.tool_defs(Mode::Pro)
                .iter()
                .all(|d| d.r#type == "function")
        );
    }

    #[test]
    fn validate_ok_applies_top_level_defaults() {
        let reg = test_registry();
        let out = reg
            .validate("top_candidates", Mode::Easy, "{}")
            .expect("defaults applied");
        assert_eq!(out["count"], json!(5));
        assert_eq!(out["type"], json!("all"));

        // explicit values are kept, missing optional ones filled in
        let partial = reg
            .validate("top_candidates", Mode::Easy, r#"{"count": 3}"#)
            .expect("partial ok");
        assert_eq!(partial["count"], json!(3));
        assert_eq!(partial["type"], json!("all"));
    }

    #[test]
    fn validate_empty_or_whitespace_arguments_mean_empty_object() {
        let reg = test_registry();
        for args in ["", "   ", "\n\t "] {
            let out = reg
                .validate("chain_stats", Mode::Pro, args)
                .expect("empty ok");
            assert_eq!(out, json!({}));
        }
    }

    #[test]
    fn validate_bad_json() {
        let err = err_of(test_registry().validate("top_candidates", Mode::Easy, "{nope"));
        assert_eq!(err.code, ToolErrorCode::BadJson);
        assert!(err.message.contains("JSON"), "got: {}", err.message);
    }

    #[test]
    fn validate_non_object_arguments() {
        let reg = test_registry();
        for args in ["[1, 2]", "\"hello\"", "42", "null"] {
            let err = err_of(reg.validate("top_candidates", Mode::Easy, args));
            assert_eq!(err.code, ToolErrorCode::NotObject, "args: {args}");
            assert!(err.message.contains("object"), "got: {}", err.message);
        }
    }

    #[test]
    fn validate_missing_required_field() {
        let err = err_of(test_registry().validate("contract_detail", Mode::Easy, "{}"));
        assert_eq!(err.code, ToolErrorCode::MissingField);
        assert!(err.message.contains("symbol"), "got: {}", err.message);
    }

    #[test]
    fn validate_unknown_field_lists_allowed_fields() {
        let err = err_of(test_registry().validate(
            "top_candidates",
            Mode::Easy,
            r#"{"count": 5, "typ": "put"}"#,
        ));
        assert_eq!(err.code, ToolErrorCode::UnknownField);
        assert!(err.message.contains("count, type"), "got: {}", err.message);
    }

    #[test]
    fn validate_wrong_type() {
        let err =
            err_of(test_registry().validate("top_candidates", Mode::Easy, r#"{"count": "5"}"#));
        assert_eq!(err.code, ToolErrorCode::WrongType);
        assert!(err.message.contains("count"), "got: {}", err.message);
    }

    #[test]
    fn validate_integer_accepts_whole_float_rejects_fraction() {
        let reg = test_registry();
        let ok = reg
            .validate("top_candidates", Mode::Easy, r#"{"count": 5.0}"#)
            .expect("5.0 is a whole number");
        assert_eq!(ok["count"].as_f64(), Some(5.0));
        let err = err_of(reg.validate("top_candidates", Mode::Easy, r#"{"count": 5.5}"#));
        assert_eq!(err.code, ToolErrorCode::WrongType);
    }

    #[test]
    fn validate_enum_miss_lists_allowed_values() {
        let err = err_of(test_registry().validate(
            "top_candidates",
            Mode::Easy,
            r#"{"type": "strangle"}"#,
        ));
        assert_eq!(err.code, ToolErrorCode::NotInEnum);
        for wanted in ["all", "put", "call"] {
            assert!(err.message.contains(wanted), "got: {}", err.message);
        }
    }

    #[test]
    fn validate_min_and_max_bounds() {
        let reg = test_registry();
        let too_low = err_of(reg.validate("top_candidates", Mode::Easy, r#"{"count": 0}"#));
        assert_eq!(too_low.code, ToolErrorCode::OutOfRange);
        assert!(too_low.message.contains('1'), "got: {}", too_low.message);
        let too_high = err_of(reg.validate("top_candidates", Mode::Easy, r#"{"count": 21}"#));
        assert_eq!(too_high.code, ToolErrorCode::OutOfRange);
        assert!(too_high.message.contains("20"), "got: {}", too_high.message);
    }

    #[test]
    fn validate_nested_array_item_path_appears_in_message() {
        let reg = test_registry();
        let err = err_of(reg.validate(
            "easy_only_tool",
            Mode::Easy,
            r#"{"steps": [{"anchor": "price.card"}, {"anchor": 123}]}"#,
        ));
        assert_eq!(err.code, ToolErrorCode::WrongType);
        assert!(
            err.message.contains("steps[1].anchor"),
            "got: {}",
            err.message
        );

        // missing required inside an array item also carries the full path
        let missing = err_of(reg.validate(
            "easy_only_tool",
            Mode::Easy,
            r#"{"steps": [{"anchor": "price.card"}, {}]}"#,
        ));
        assert_eq!(missing.code, ToolErrorCode::MissingField);
        assert!(
            missing.message.contains("steps[1].anchor"),
            "got: {}",
            missing.message
        );
    }

    #[test]
    fn validate_unknown_tool_lists_available_names_for_mode() {
        let err = err_of(test_registry().validate("nope", Mode::Easy, "{}"));
        assert_eq!(err.code, ToolErrorCode::UnknownTool);
        assert!(
            err.message.contains("top_candidates"),
            "got: {}",
            err.message
        );
        assert!(
            err.message.contains("easy_only_tool"),
            "got: {}",
            err.message
        );
        assert!(
            !err.message.contains("pro_only_tool"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn validate_not_in_mode() {
        let reg = test_registry();
        let err = err_of(reg.validate("pro_only_tool", Mode::Easy, "{}"));
        assert_eq!(err.code, ToolErrorCode::NotInMode);
        assert!(err.message.contains("pro"), "got: {}", err.message);
        let err = err_of(reg.validate("easy_only_tool", Mode::Pro, "{}"));
        assert_eq!(err.code, ToolErrorCode::NotInMode);
        assert!(err.message.contains("easy"), "got: {}", err.message);
    }

    #[test]
    fn validate_ignores_unsupported_schema_keywords() {
        let mut reg = Registry::new();
        reg.register(ToolSpec {
            name: "fancy_tool".into(),
            tier: Tier::Read,
            modes: vec![Mode::Easy, Mode::Pro],
            description: String::new(),
            examples: vec![],
            parameters: json!({
                "type": "object",
                "properties": {
                    "email": {
                        "type": "string",
                        "format": "email",
                        "pattern": "^[^@]+@[^@]+$",
                        "minLength": 3
                    }
                },
                "additionalProperties": false
            }),
            timeout_ms: 1000,
            webull_cost: 0,
        })
        .expect("register");
        reg.validate("fancy_tool", Mode::Easy, r#"{"email": "x"}"#)
            .expect("format/pattern/minLength are ignored, not enforced");
    }
}
