//! AI Copilot — LLM function calling grounded in live screener data.
//! Provider-agnostic (OpenAI-compatible API): GLM-5.3 primary, OpenAI backup.
//! Streams responses to the frontend via Tauri events.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// LLM provider configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: String, // "glm" | "openai" | custom label
    pub base_url: String, // e.g. "https://api.z.ai" or "https://api.openai.com"
    pub model: String,    // e.g. "glm-5.3" or "gpt-4o"
    pub api_key: String,  // from keyring/env — never logged
    #[serde(default = "default_temperature")]
    pub temperature: f64,
}

fn default_temperature() -> f64 {
    0.3
}

impl LlmConfig {
    pub fn glm(api_key: impl Into<String>) -> Self {
        Self {
            provider: "glm".into(),
            base_url: "https://api.z.ai".into(),
            model: "glm-5.3".into(),
            api_key: api_key.into(),
            temperature: 0.3,
        }
    }

    pub fn openai(api_key: impl Into<String>) -> Self {
        Self {
            provider: "openai".into(),
            base_url: "https://api.openai.com".into(),
            model: "gpt-4o".into(),
            api_key: api_key.into(),
            temperature: 0.3,
        }
    }

    /// From env: GLM_API_KEY (preferred) or OPENAI_API_KEY.
    pub fn from_env() -> Option<Self> {
        if let Some(key) = std::env::var("GLM_API_KEY").ok().filter(|k| !k.is_empty()) {
            return Some(Self::glm(key));
        }
        if let Some(key) = std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
        {
            return Some(Self::openai(key));
        }
        None
    }

    /// Chat completions endpoint.
    pub fn chat_url(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url)
    }
}

/// A chat message in the OpenAI format.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "system" | "user" | "assistant" | "tool"
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

/// Tool definition for function calling (OpenAI-compatible format).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDef {
    pub r#type: String, // always "function"
    pub function: ToolFunction,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON Schema
}

/// The three tools the copilot can call — grounded in real screener data.
pub fn screener_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            r#type: "function".into(),
            function: ToolFunction {
                name: "top_candidates".into(),
                description: "Get the top N option contracts that pass the screener filters. Returns symbol, grade, strike, expiry, premium, annualized return, probability of profit, and assignment risk.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "count": { "type": "integer", "description": "Number of candidates to return (1-20)", "default": 5 },
                        "type": { "type": "string", "enum": ["all", "put", "call"], "description": "Filter by option type", "default": "all" }
                    }
                }),
            },
        },
        ToolDef {
            r#type: "function".into(),
            function: ToolFunction {
                name: "contract_detail".into(),
                description: "Get detailed analysis for a specific option contract by its OCC symbol (e.g. NVDA261218P00175000). Returns all economics fields including breakeven, cushion, greeks, and grade reasoning.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "symbol": { "type": "string", "description": "OCC symbol of the contract" }
                    },
                    "required": ["symbol"]
                }),
            },
        },
        ToolDef {
            r#type: "function".into(),
            function: ToolFunction {
                name: "chain_stats".into(),
                description: "Get chain-level statistics: ATM implied volatility, expected move (±1σ), and put-call skew for the current expiry.".into(),
                parameters: serde_json::json!({ "type": "object", "properties": {} }),
            },
        },
    ]
}

/// System prompt for the copilot — Thai-first, grounded in real data.
pub fn system_prompt() -> String {
    "คุณคือผู้ช่วย AI ของแอป PREMIUM RADAR — เครื่องมือคัดกรองสัญญา options สำหรับขายพรีเมียม (CSP/Covered Call).\n\n\
     กติกาสำคัญ:\n\
     1. ตอบคำถามโดยอ้างอิงข้อมูลจริงจากตารางเสมอ — ใช้ function calling เพื่อดึงข้อมูลก่อนตอบ\n\
     2. อธิบายเป็นภาษาไทยที่เข้าใจง่ายสำหรับคนที่ไม่ใช่นักวิเคราะห์\n\
     3. ไม่แนะนำการซื้อขายโดยตรง — ให้ข้อมูลและอธิบายความเสี่ยง\n\
     4. ระบุที่มาของตัวเลขเสมอ (เช่น 'จากตาราง 8 สัญญาที่ผ่านเงื่อนไข')\n\
     5. ถ้าไม่มีข้อมูลที่ต้องการ ให้บอกตรงๆ ไม่ invent ตัวเลข\n\n\
     เมื่อผู้ใช้ถามเกี่ยวกับสัญญาที่น่าสนใจ ให้เรียก top_candidates ก่อน แล้วอธิบาย top 3 พร้อมเหตุผลว่าทำไมเกรดนั้น\n\
     เมื่อผู้ใช้ถามเรื่องความเสี่ยง ให้อธิบาย cushion, breakeven และโอกาสโดน assign ของสัญญาที่เลือก".into()
}

/// Build the request body for a chat completion call.
pub fn build_request(config: &LlmConfig, messages: &[ChatMessage]) -> Value {
    serde_json::json!({
        "model": config.model,
        "messages": messages,
        "tools": screener_tools(),
        "temperature": config.temperature,
        "stream": false, // non-streaming for tool calls; streaming for final response
    })
}

/// Parse the API response — extract either content or tool_calls.
#[derive(Debug)]
pub enum LlmResponse {
    Content(String),
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    Error(String),
}

pub fn parse_response(body: &Value) -> LlmResponse {
    let Some(choices) = body.get("choices").and_then(Value::as_array) else {
        return LlmResponse::Error("no choices in response".into());
    };
    let Some(choice) = choices.first() else {
        return LlmResponse::Error("empty choices".into());
    };
    let message = choice.get("message").cloned().unwrap_or(Value::Null);

    // Check for tool_calls first
    if let Some(call) = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .and_then(|t| t.first())
    {
        let id = call
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let function = call.get("function").cloned().unwrap_or(Value::Null);
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let arguments = function
            .get("arguments")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if !name.is_empty() {
            return LlmResponse::ToolCall {
                id,
                name,
                arguments,
            };
        }
    }

    // Regular content
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !content.is_empty() {
        return LlmResponse::Content(content.to_string());
    }

    LlmResponse::Error("no content or tool_calls in response".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glm_config_from_env_preferred() {
        // just verify construction works
        let c = LlmConfig::glm("test-key");
        assert_eq!(c.provider, "glm");
        assert_eq!(c.model, "glm-5.3");
        assert!(c.chat_url().contains("api.z.ai"));
    }

    #[test]
    fn openai_config() {
        let c = LlmConfig::openai("test-key");
        assert_eq!(c.provider, "openai");
        assert!(c.chat_url().contains("api.openai.com"));
    }

    #[test]
    fn system_prompt_is_thai() {
        let p = system_prompt();
        assert!(p.contains("PREMIUM RADAR"));
        assert!(p.contains("ภาษาไทย"));
    }

    #[test]
    fn tools_have_three_functions() {
        let tools = screener_tools();
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].function.name, "top_candidates");
        assert_eq!(tools[1].function.name, "contract_detail");
        assert_eq!(tools[2].function.name, "chain_stats");
    }

    #[test]
    fn parse_content_response() {
        let body = serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": "สวัสดีครับ"}}]
        });
        match parse_response(&body) {
            LlmResponse::Content(c) => assert_eq!(c, "สวัสดีครับ"),
            other => panic!("expected Content, got {other:?}"),
        }
    }

    #[test]
    fn parse_tool_call_response() {
        let body = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "function": { "name": "top_candidates", "arguments": "{\"count\": 5}" }
                    }]
                }
            }]
        });
        match parse_response(&body) {
            LlmResponse::ToolCall {
                id,
                name,
                arguments,
            } => {
                assert_eq!(id, "call_abc");
                assert_eq!(name, "top_candidates");
                assert!(arguments.contains("count"));
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn parse_error_response() {
        assert!(matches!(
            parse_response(&serde_json::json!({})),
            LlmResponse::Error(_)
        ));
        assert!(matches!(
            parse_response(&serde_json::json!({"choices": []})),
            LlmResponse::Error(_)
        ));
    }
}
