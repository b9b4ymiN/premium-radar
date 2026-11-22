//! AI Copilot — LLM function calling grounded in live screener data.
//! Fully provider-agnostic: any OpenAI-compatible chat completions endpoint
//! works (Z.ai, OpenAI, Anthropic via proxy, Ollama, LM Studio, etc.).
//! Configure via env vars or the settings UI — nothing is hardcoded.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// LLM provider configuration — any OpenAI-compatible endpoint.
///
/// # Env vars (generic, checked in order):
/// 1. `LLM_BASE_URL` + `LLM_API_KEY` + `LLM_MODEL` — full control, any provider
/// 2. `GLM_API_KEY` — shorthand for Z.ai GLM-5.3
/// 3. `OPENAI_API_KEY` — shorthand for OpenAI gpt-4o
///
/// # Settings UI override:
/// The first-run setup screen can set any `base_url` / `model` / `api_key`
/// combination, which takes precedence over env vars.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmConfig {
    /// Human-readable label for the settings UI (e.g. "Z.ai GLM-5.3", "Ollama local")
    #[serde(default)]
    pub label: String,
    /// Base URL of the OpenAI-compatible API (e.g. "https://api.z.ai", "http://localhost:11434")
    pub base_url: String,
    /// Model name (e.g. "glm-5.3", "gpt-4o", "llama3.2", "claude-sonnet-4-5")
    pub model: String,
    /// API key (from keyring/env/settings — never logged)
    pub api_key: String,
    /// Temperature for generation
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    /// Max tokens for response (0 = provider default)
    #[serde(default)]
    pub max_tokens: u32,
}

fn default_temperature() -> f64 {
    0.3
}

impl LlmConfig {
    /// Fully manual — use this when you have all the pieces.
    pub fn new(
        label: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            base_url: base_url.into(),
            model: model.into(),
            api_key: api_key.into(),
            temperature: 0.3,
            max_tokens: 0,
        }
    }

    /// From env vars. Checks generic `LLM_*` first, then provider-specific shorthands.
    pub fn from_env() -> Option<Self> {
        // Full generic override: LLM_BASE_URL + LLM_API_KEY (+ optional LLM_MODEL)
        if let (Some(base_url), Some(api_key)) = (
            std::env::var("LLM_BASE_URL").ok().filter(|v| !v.is_empty()),
            std::env::var("LLM_API_KEY").ok().filter(|v| !v.is_empty()),
        ) {
            let model = std::env::var("LLM_MODEL").unwrap_or_default();
            let label = std::env::var("LLM_LABEL").unwrap_or_else(|_| "Custom".into());
            return Some(Self::new(label, base_url, model, api_key));
        }

        // Shorthand: GLM_API_KEY → Z.ai GLM-5.3
        if let Some(key) = std::env::var("GLM_API_KEY").ok().filter(|k| !k.is_empty()) {
            let model = std::env::var("GLM_MODEL").unwrap_or_else(|_| "glm-5.3".into());
            return Some(Self::new("Z.ai GLM", "https://api.z.ai", model, key));
        }

        // Shorthand: OPENAI_API_KEY → OpenAI
        if let Some(key) = std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
        {
            let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".into());
            return Some(Self::new("OpenAI", "https://api.openai.com", model, key));
        }

        None
    }

    /// Chat completions endpoint (OpenAI-compatible path).
    pub fn chat_url(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url)
    }

    /// Log-safe display — no api_key.
    pub fn masked(&self) -> String {
        format!(
            "LlmConfig(label={}, base_url={}, model={}, api_key=***{}***)",
            self.label,
            self.base_url,
            self.model,
            self.api_key.len()
        )
    }

    /// Check if this config is usable (has base_url, model, and key).
    pub fn is_ready(&self) -> bool {
        !self.base_url.is_empty() && !self.model.is_empty() && !self.api_key.is_empty()
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
    let mut body = serde_json::json!({
        "model": config.model,
        "messages": messages,
        "tools": screener_tools(),
        "temperature": config.temperature,
    });
    if config.max_tokens > 0 {
        body["max_tokens"] = serde_json::json!(config.max_tokens);
    }
    body
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

// Manual Debug impl — api_key never leaks
impl std::fmt::Debug for LlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.masked())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fully_custom_config() {
        let c = LlmConfig::new("Ollama local", "http://localhost:11434", "llama3.2", "");
        assert_eq!(c.label, "Ollama local");
        assert_eq!(c.base_url, "http://localhost:11434");
        assert_eq!(c.model, "llama3.2");
        assert_eq!(c.chat_url(), "http://localhost:11434/v1/chat/completions");
        assert!(!c.is_ready()); // no key
    }

    #[test]
    fn glm_shorthand() {
        let c = LlmConfig::new("Z.ai GLM", "https://api.z.ai", "glm-5.3", "test-key");
        assert!(c.is_ready());
        assert!(c.chat_url().contains("api.z.ai/v1/chat/completions"));
    }

    #[test]
    fn openai_shorthand() {
        let c = LlmConfig::new("OpenAI", "https://api.openai.com", "gpt-4o", "sk-test");
        assert!(c.is_ready());
        assert!(c.chat_url().contains("api.openai.com/v1/chat/completions"));
    }

    #[test]
    fn debug_never_leaks_key() {
        let c = LlmConfig::new("test", "https://example.com", "model", "SUPERSECRET");
        let d = format!("{c:?}");
        assert!(!d.contains("SUPERSECRET"));
        assert!(d.contains("***"));
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
        let body =
            serde_json::json!({"choices": [{"message": {"role": "assistant", "content": "สวัสดี"}}]});
        match parse_response(&body) {
            LlmResponse::Content(c) => assert_eq!(c, "สวัสดี"),
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
    }
}
