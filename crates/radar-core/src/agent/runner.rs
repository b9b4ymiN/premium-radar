//! Agent runner loop — the copilot loop's replacement for the agent.
//!
//! Differences from `copilot::chat_with_tool_runner`: tools come from the
//! mode-aware [`Registry`] (defs + argument validation with Thai
//! how-to-fix errors), one model reply may carry several `tool_calls`,
//! typed [`AgentEvent`]s stream out through a callback, call/cost/round
//! budgets are enforced, repeated identical calls are answered from
//! cache, and a cancel flag stops the loop before any further HTTP.
//! The executor is the only place caller locks may live — nothing is
//! held across an LLM round-trip (same contract as the copilot loop).

use serde::Serialize;
use serde_json::Value;

use super::contract::{Mode, Tier, ToolSpec};
use crate::copilot::ChatMessage;

#[cfg(any(test, feature = "client"))]
use crate::copilot::{LlmConfig, ToolDef};

/// One agent question: mode, system prompt, UI history and the current
/// screen context the frontend ships along.
pub struct AgentRequest {
    pub mode: Mode,
    pub system_prompt: String,
    /// user/assistant turns from the UI
    pub history: Vec<ChatMessage>,
    pub screen_context: Option<Value>,
}

/// Per-run guard rails. Defaults are the product defaults for one
/// question: at most 12 tool calls, 150 Webull budget units, 8 tool
/// rounds and a 90 s LLM timeout.
pub struct RunLimits {
    pub max_tool_calls: u32,
    pub max_webull_cost: u32,
    pub max_rounds: u32,
    pub llm_timeout_secs: u64,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            max_tool_calls: 12,
            max_webull_cost: 150,
            max_rounds: 8,
            llm_timeout_secs: 90,
        }
    }
}

/// Streamed progress for the chat UI. `unverified` is true when the
/// grounding check found numbers with no supporting tool/user fact even
/// after the single correction round; `ungrounded` lists their raw tokens.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    Thinking {
        round: u32,
    },
    ToolStart {
        call_id: String,
        name: String,
        args: Value,
        tier: Tier,
    },
    /// `summary` is the tool result clipped to 200 characters.
    ToolEnd {
        call_id: String,
        name: String,
        ok: bool,
        summary: String,
        ms: u64,
    },
    /// A ui tool wants the screen to act (`ui_id` pairs the event with the
    /// ack the frontend sends back through the `agent_ui_result` command).
    /// The runner loop itself never emits this — the Tauri executor does.
    UiAction {
        ui_id: String,
        action: Value,
    },
    Answer {
        text: String,
        unverified: bool,
        ungrounded: Vec<String>,
    },
    Error {
        message: String,
    },
    Cancelled,
}

/// Final result of one agent run.
#[derive(Debug, PartialEq)]
pub enum AgentOutcome {
    Answer(String),
    Cancelled,
}

/// Runs one validated tool call. Returns the JSON string sent back to the
/// model, or `Err(thai message)` which the runner wraps as
/// `{"error":"tool_failed","message":...}`.
pub type ToolExecutor<'a> = dyn FnMut(&ToolSpec, &Value) -> Result<String, String> + 'a;

/// System nudge added before the single final request made once the
/// call/round quota is spent (tools are removed from that request).
#[cfg(feature = "client")]
const QUOTA_HINT: &str =
    "ใช้ tool ครบโควตาแล้ว — ตอบจากข้อมูลที่มีอยู่ตอนนี้ และบอกผู้ใช้ตรงๆ ว่าข้อมูลส่วนไหนยังไม่ได้ดู";

/// Correction note of the one grounding retry: the model sees the raw
/// tokens it invented and is told to re-answer from tool data only (or to
/// call the relevant tool first).
#[cfg(feature = "client")]
const GROUNDING_HINT: &str = "ตัวเลขเหล่านี้ไม่มีในข้อมูลจาก tools";

// ---------- pure helpers: compiled for tests and the client loop ----------

/// One `tool_calls` entry of a model reply.
#[cfg(any(test, feature = "client"))]
#[derive(Clone, Debug, PartialEq)]
struct ParsedToolCall {
    id: String,
    name: String,
    arguments: String,
}

/// A parsed chat-completions reply: plain content, one or more tool
/// calls, or an unusable body.
#[cfg(any(test, feature = "client"))]
#[derive(Debug, PartialEq)]
enum AgentReply {
    Content(String),
    ToolCalls(Vec<ParsedToolCall>),
    Error(String),
}

/// OpenAI-compatible reply parser that keeps ALL `tool_calls` of one
/// message (the copilot parser takes only the first). Entries without a
/// usable `function.name` are skipped; a missing `id` gets a stable
/// per-message synthetic id so the tool replies still pair up.
#[cfg(any(test, feature = "client"))]
fn parse_agent_reply(body: &Value) -> AgentReply {
    let Some(choice) = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
    else {
        return AgentReply::Error("no choices in response".into());
    };
    let message = choice.get("message").cloned().unwrap_or(Value::Null);

    let calls: Vec<ParsedToolCall> = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .enumerate()
                .filter_map(|(index, call)| {
                    let name = call
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if name.is_empty() {
                        return None;
                    }
                    let id = call
                        .get("id")
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty())
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("call_{index}"));
                    let arguments = call
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    Some(ParsedToolCall {
                        id,
                        name: name.to_string(),
                        arguments,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if !calls.is_empty() {
        return AgentReply::ToolCalls(calls);
    }

    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !content.is_empty() {
        return AgentReply::Content(content.to_string());
    }
    AgentReply::Error("no content or tool_calls in response".into())
}

/// system(system_prompt) + optional screen-context system message + the
/// UI history, in that order.
#[cfg(any(test, feature = "client"))]
fn build_messages(req: &AgentRequest) -> Vec<ChatMessage> {
    let mut messages = vec![ChatMessage::system(req.system_prompt.clone())];
    if let Some(context) = &req.screen_context {
        messages.push(ChatMessage::system(format!(
            "บริบทหน้าจอตอนนี้ (JSON): {context}"
        )));
    }
    messages.extend(req.history.iter().cloned());
    messages
}

/// Request body with the same shape as `copilot::build_request`, but the
/// tool list is a parameter (`None` = the final no-tools request) and
/// `max_tokens` only appears when configured.
#[cfg(any(test, feature = "client"))]
fn build_request_body(
    config: &LlmConfig,
    messages: &[ChatMessage],
    tools: Option<Vec<ToolDef>>,
) -> Value {
    let mut body = serde_json::json!({
        "model": config.model,
        "messages": messages,
        "temperature": config.temperature,
    });
    if let Some(tools) = tools {
        body["tools"] = serde_json::json!(tools);
    }
    if config.max_tokens > 0 {
        body["max_tokens"] = serde_json::json!(config.max_tokens);
    }
    body
}

/// The assistant message's `tool_calls` payload for a batch of calls.
#[cfg(feature = "client")]
fn render_tool_calls(calls: &[ParsedToolCall]) -> Value {
    Value::Array(
        calls
            .iter()
            .map(|call| {
                serde_json::json!({
                    "id": call.id,
                    "type": "function",
                    "function": {"name": call.name, "arguments": call.arguments},
                })
            })
            .collect(),
    )
}

/// Canonical compact JSON with object keys sorted at every level — the
/// dedup key must not depend on the order the model emitted fields in
/// (serde_json maps may preserve insertion order, so sort explicitly).
#[cfg(any(test, feature = "client"))]
fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
            keys.sort_unstable();
            let fields: Vec<String> = keys
                .into_iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        Value::String(key.to_string()),
                        canonical_json(&map[key])
                    )
                })
                .collect();
            format!("{{{}}}", fields.join(","))
        }
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        scalar => scalar.to_string(),
    }
}

/// Dedup identity of a validated call: tool name + canonical args.
/// `\u{1}` cannot occur in a tool name or as a literal byte inside
/// canonical JSON (control characters are escaped), so the split is
/// unambiguous.
#[cfg(any(test, feature = "client"))]
fn dedup_key(name: &str, validated_args: &Value) -> String {
    format!("{name}\u{1}{}", canonical_json(validated_args))
}

/// Clip to at most `max` characters. `chars()` iterates whole scalar
/// values, so multi-byte Thai text is never split mid-character.
#[cfg(any(test, feature = "client"))]
fn clip_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

// ---------- the loop itself (HTTP needs the client feature) ----------

#[cfg(feature = "client")]
use std::collections::HashMap;

#[cfg(feature = "client")]
use std::sync::atomic::AtomicBool;

#[cfg(feature = "client")]
use std::time::Instant;

#[cfg(feature = "client")]
use super::grounding;
#[cfg(feature = "client")]
use super::registry::Registry;

/// Marker: the cancel flag won before this call could execute.
#[cfg(feature = "client")]
struct CancelSignal;

/// Mutable state of one run: budgets and the duplicate-call cache. The
/// message log lives in `run_agent` — this is only what tool dispatch
/// needs.
#[cfg(feature = "client")]
struct RunState<'a> {
    registry: &'a Registry,
    mode: Mode,
    limits: &'a RunLimits,
    cancel: &'a AtomicBool,
    seen: HashMap<String, String>,
    webull_cost_used: u32,
    tool_calls_used: u32,
}

#[cfg(feature = "client")]
impl RunState<'_> {
    /// Validate, budget-check and execute one model tool call, emitting
    /// `ToolStart`/`ToolEnd`. Returns the tool-result string plus its ok
    /// flag, or `Err(CancelSignal)` when cancellation won before
    /// execution (no `ToolEnd` — there is no result to report).
    fn process_call(
        &mut self,
        call: &ParsedToolCall,
        execute: &mut ToolExecutor<'_>,
        emit: &mut dyn FnMut(AgentEvent),
    ) -> Result<(bool, String), CancelSignal> {
        use std::sync::atomic::Ordering;

        let validated = self
            .registry
            .validate(&call.name, self.mode, &call.arguments);
        let args = match &validated {
            Ok(value) => value.clone(),
            // Invalid payloads still surface to the UI — as the raw string.
            Err(_) => Value::String(call.arguments.clone()),
        };
        let tier = self
            .registry
            .get(&call.name)
            .map(|spec| spec.tier)
            .unwrap_or(Tier::Read);
        emit(AgentEvent::ToolStart {
            call_id: call.id.clone(),
            name: call.name.clone(),
            args,
            tier,
        });
        let started = Instant::now();

        let (ok, result) = match validated {
            Err(tool_error) => (false, tool_error.to_model_json()),
            Ok(validated_args) => {
                let key = dedup_key(&call.name, &validated_args);
                if let Some(previous) = self.seen.get(&key) {
                    (
                        true,
                        serde_json::json!({
                            "note": "เรียกซ้ำด้วยค่าเดิม — นี่คือผลเดิม ไม่ต้องเรียกอีก",
                            "result": previous,
                        })
                        .to_string(),
                    )
                } else if let Some(spec) = self.registry.get(&call.name) {
                    if self.webull_cost_used + spec.webull_cost > self.limits.max_webull_cost {
                        (
                            false,
                            serde_json::json!({
                                "error": "budget_exhausted",
                                "message": "งบการดึงข้อมูลจาก Webull หมดแล้ว — ตอบผู้ใช้จากข้อมูลที่มีอยู่ตอนนี้เท่านั้น และบอกให้ชัดว่าข้อมูลส่วนไหนยังไม่ได้ตรวจสอบ",
                            })
                            .to_string(),
                        )
                    } else {
                        if self.cancel.load(Ordering::SeqCst) {
                            return Err(CancelSignal);
                        }
                        match execute(spec, &validated_args) {
                            Ok(output) => {
                                self.webull_cost_used += spec.webull_cost;
                                self.seen.insert(key, output.clone());
                                (true, output)
                            }
                            Err(message) => (
                                false,
                                serde_json::json!({
                                    "error": "tool_failed",
                                    "message": message,
                                })
                                .to_string(),
                            ),
                        }
                    }
                } else {
                    // validate() only passes registered tools; guard anyway
                    // so a mid-run registry change degrades, not panics.
                    (
                        false,
                        serde_json::json!({
                            "error": "tool_failed",
                            "message": format!("tool \"{}\" หายไปจาก registry ระหว่างรัน", call.name),
                        })
                        .to_string(),
                    )
                }
            }
        };

        let ms = started.elapsed().as_millis() as u64;
        emit(AgentEvent::ToolEnd {
            call_id: call.id.clone(),
            name: call.name.clone(),
            ok,
            summary: clip_chars(&result, 200),
            ms,
        });
        Ok((ok, result))
    }

    /// A call beyond `max_tool_calls`: never validated or executed, but the
    /// `tool_call_id` still must get a reply (providers reject unpaired
    /// ids). Emits ToolStart/ToolEnd and returns the tool-result payload.
    fn quota_refusal(&self, call: &ParsedToolCall, emit: &mut dyn FnMut(AgentEvent)) -> String {
        let tier = self
            .registry
            .get(&call.name)
            .map(|spec| spec.tier)
            .unwrap_or(Tier::Read);
        emit(AgentEvent::ToolStart {
            call_id: call.id.clone(),
            name: call.name.clone(),
            args: Value::String(call.arguments.clone()),
            tier,
        });
        let result = serde_json::json!({
            "error": "quota_exhausted",
            "message": "ใช้ tool ครบโควตาของคำถามนี้แล้ว — ไม่ได้รัน tool นี้ ตอบจากข้อมูลที่มีอยู่",
        })
        .to_string();
        emit(AgentEvent::ToolEnd {
            call_id: call.id.clone(),
            name: call.name.clone(),
            ok: false,
            summary: clip_chars(&result, 200),
            ms: 0,
        });
        result
    }
}

/// Emit the terminal error event, then hand the message back for the
/// `Err` return — every failure path goes through here.
#[cfg(feature = "client")]
fn emit_error(emit: &mut dyn FnMut(AgentEvent), message: String) -> String {
    emit(AgentEvent::Error {
        message: message.clone(),
    });
    message
}

/// The agent loop. See the module docs; on every `Err` an
/// [`AgentEvent::Error`] is emitted first, and on cancellation a
/// [`AgentEvent::Cancelled`] is emitted and `Ok(AgentOutcome::Cancelled)`
/// is returned without further HTTP.
#[cfg(feature = "client")]
pub fn run_agent(
    config: &LlmConfig,
    registry: &Registry,
    req: &AgentRequest,
    limits: &RunLimits,
    cancel: &AtomicBool,
    execute: &mut ToolExecutor<'_>,
    emit: &mut dyn FnMut(AgentEvent),
) -> Result<AgentOutcome, String> {
    use std::sync::atomic::Ordering;

    let http = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(limits.llm_timeout_secs))
        .build()
        .map_err(|e| emit_error(emit, format!("build http client: {e}")))?;

    let mut messages = build_messages(req);
    // Grounding sources: every tool result string shown to the model in
    // this run plus the user's own turns — the model may reuse numbers the
    // user themselves provided.
    let mut sources: Vec<String> = req
        .history
        .iter()
        .filter(|message| message.role == "user")
        .map(|message| message.content.clone())
        .collect();
    let mut grounding_retry_used = false;
    let mut state = RunState {
        registry,
        mode: req.mode,
        limits,
        cancel,
        seen: HashMap::new(),
        webull_cost_used: 0,
        tool_calls_used: 0,
    };
    let mut round: u32 = 0;
    let mut quota_hint_sent = false;

    loop {
        if cancel.load(Ordering::SeqCst) {
            emit(AgentEvent::Cancelled);
            return Ok(AgentOutcome::Cancelled);
        }
        // Budgets gate the NEXT request: once the call/round quota is
        // spent, the loop only ever issues the final no-tools request.
        let quota_exhausted =
            state.tool_calls_used >= limits.max_tool_calls || round >= limits.max_rounds;
        round += 1;
        emit(AgentEvent::Thinking { round });

        let body = if quota_exhausted {
            if !quota_hint_sent {
                messages.push(ChatMessage::system(QUOTA_HINT));
                quota_hint_sent = true;
            }
            build_request_body(config, &messages, None)
        } else {
            build_request_body(config, &messages, Some(registry.tool_defs(req.mode)))
        };

        // No caller lock is held across this request: the only lock site
        // is the executor above (same contract as `chat_with_tool_runner`).
        let response = http
            .post(config.chat_url())
            .bearer_auth(&config.api_key)
            .json(&body)
            .send()
            .map_err(|e| emit_error(emit, format!("เรียก LLM ไม่สำเร็จ: {e}")))?;
        let status = response.status().as_u16();
        let raw = response
            .text()
            .map_err(|e| emit_error(emit, format!("อ่าน LLM response ไม่สำเร็จ: {e}")))?;
        if status != 200 {
            return Err(emit_error(
                emit,
                format!("LLM {status}: {}", clip_chars(&raw, 300)),
            ));
        }
        let json: Value = serde_json::from_str(&raw)
            .map_err(|e| emit_error(emit, format!("LLM response ไม่ใช่ JSON: {e}")))?;

        match parse_agent_reply(&json) {
            AgentReply::Content(text) => {
                let facts = grounding::collect_facts(
                    &sources.iter().map(String::as_str).collect::<Vec<&str>>(),
                );
                let ungrounded = grounding::ungrounded(&text, &facts);
                if ungrounded.is_empty() || grounding_retry_used {
                    let unverified = !ungrounded.is_empty();
                    emit(AgentEvent::Answer {
                        text: text.clone(),
                        unverified,
                        ungrounded,
                    });
                    return Ok(AgentOutcome::Answer(text));
                }
                // One correction round: show the model the numbers it
                // invented, then let it answer again (it may also call
                // more tools — normal budgets still apply).
                grounding_retry_used = true;
                messages.push(ChatMessage::assistant(text));
                messages.push(ChatMessage::system(format!(
                    "{GROUNDING_HINT}: {} — ตอบใหม่โดยใช้เฉพาะตัวเลขจากผลของ tools หรือเรียก tool ที่เกี่ยวข้องก่อน ห้ามคำนวณเอง",
                    ungrounded.join(", ")
                )));
            }
            AgentReply::Error(message) => {
                return Err(emit_error(emit, format!("LLM response ผิดรูป: {message}")));
            }
            AgentReply::ToolCalls(_) if quota_exhausted => {
                return Err(emit_error(
                    emit,
                    "AI ยังพยายามเรียก tool ในคำขอสุดท้ายที่ตัด tools ออกแล้ว — ลองถามใหม่ด้วยคำถามที่แคบกว่านี้"
                        .into(),
                ));
            }
            AgentReply::ToolCalls(calls) => {
                messages.push(ChatMessage {
                    role: "assistant".into(),
                    content: String::new(),
                    tool_calls: Some(render_tool_calls(&calls)),
                    tool_call_id: None,
                });
                for call in &calls {
                    // Every call counts toward the quota — valid or not,
                    // duplicate or not, run or refused. Calls past the cap
                    // are refused mid-batch so one oversized reply cannot
                    // blow through max_tool_calls or the Webull budget.
                    state.tool_calls_used += 1;
                    let result = if state.tool_calls_used > limits.max_tool_calls {
                        state.quota_refusal(call, emit)
                    } else {
                        match state.process_call(call, execute, emit) {
                            Ok((_ok, result)) => result,
                            Err(CancelSignal) => {
                                emit(AgentEvent::Cancelled);
                                return Ok(AgentOutcome::Cancelled);
                            }
                        }
                    };
                    sources.push(result.clone());
                    messages.push(ChatMessage {
                        role: "tool".into(),
                        content: result,
                        tool_calls: None,
                        tool_call_id: Some(call.id.clone()),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_keeps_all_tool_calls_in_order() {
        let body = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [
                        {"id": "call_a", "function": {"name": "top_candidates", "arguments": "{\"count\": 3}"}},
                        {"id": "call_b", "function": {"name": "chain_stats", "arguments": "{}"}}
                    ]
                }
            }]
        });
        match parse_agent_reply(&body) {
            AgentReply::ToolCalls(calls) => {
                assert_eq!(calls.len(), 2);
                assert_eq!(calls[0].id, "call_a");
                assert_eq!(calls[0].name, "top_candidates");
                assert_eq!(calls[0].arguments, "{\"count\": 3}");
                assert_eq!(calls[1].id, "call_b");
                assert_eq!(calls[1].name, "chain_stats");
                assert_eq!(calls[1].arguments, "{}");
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    #[test]
    fn parse_missing_id_gets_stable_synthetic_id_and_empty_arguments() {
        let body = json!({
            "choices": [{
                "message": {
                    "tool_calls": [
                        {"function": {"name": "top_candidates"}},
                        {"id": "", "function": {"name": "chain_stats", "arguments": ""}}
                    ]
                }
            }]
        });
        match parse_agent_reply(&body) {
            AgentReply::ToolCalls(calls) => {
                assert_eq!(calls[0].id, "call_0");
                assert_eq!(calls[0].arguments, "");
                assert_eq!(calls[1].id, "call_1");
                assert_eq!(calls[1].arguments, "");
            }
            other => panic!("expected ToolCalls, got {other:?}"),
        }
    }

    #[test]
    fn parse_content_only_reply() {
        let body = json!({
            "choices": [{"message": {"role": "assistant", "content": "ตอบตรงนี้เลย"}}]
        });
        assert_eq!(
            parse_agent_reply(&body),
            AgentReply::Content("ตอบตรงนี้เลย".into())
        );
    }

    #[test]
    fn parse_error_shapes() {
        for body in [
            json!({}),
            json!({"choices": []}),
            json!({"choices": [{"message": {"role": "assistant", "content": null}}]}),
            json!({"choices": [{"message": {"tool_calls": []}}]}),
            json!({"choices": [{"message": {"tool_calls": [{"id": "x", "function": {}}]}}]}),
        ] {
            assert!(
                matches!(parse_agent_reply(&body), AgentReply::Error(_)),
                "expected Error for {body}"
            );
        }
    }

    #[test]
    fn dedup_key_ignores_key_order_including_nested() {
        let a: Value =
            serde_json::from_str(r#"{"count": 3, "type": "all", "filter": {"z": 1, "a": [2, 1]}}"#)
                .expect("json a");
        let b: Value =
            serde_json::from_str(r#"{"filter": {"a": [2, 1], "z": 1}, "type": "all", "count": 3}"#)
                .expect("json b");
        assert_eq!(
            dedup_key("top_candidates", &a),
            dedup_key("top_candidates", &b)
        );
        // different name or different value ⇒ different key
        assert_ne!(
            dedup_key("chain_stats", &a),
            dedup_key("top_candidates", &a)
        );
        assert_ne!(
            dedup_key("top_candidates", &json!({"count": 3})),
            dedup_key("top_candidates", &json!({"count": 4}))
        );
    }

    #[test]
    fn clip_chars_is_char_safe_for_thai_text() {
        let long = "ทดสอบตัดข้อความ".repeat(40); // 640 Thai chars, all multi-byte
        let clipped = clip_chars(&long, 200);
        assert_eq!(clipped.chars().count(), 200);
        assert!(long.starts_with(&clipped));
        // short strings pass through untouched
        assert_eq!(clip_chars("สั้น", 200), "สั้น");
        // combining marks are separate chars and are never half-split
        let with_mark = "ก้".repeat(150);
        let clipped_mark = clip_chars(&with_mark, 200);
        assert_eq!(clipped_mark.chars().count(), 200);
        assert!(with_mark.starts_with(&clipped_mark));
    }

    #[test]
    fn build_messages_assembles_system_context_history() {
        let req = AgentRequest {
            mode: Mode::Easy,
            system_prompt: "คุณคือผู้ช่วย".into(),
            history: vec![ChatMessage::user("สวัสดี"), ChatMessage::assistant("ยินดี")],
            screen_context: Some(json!({"page": "ideas", "anchors": ["a", "b"]})),
        };
        let messages = build_messages(&req);
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content, "คุณคือผู้ช่วย");
        assert_eq!(messages[1].role, "system");
        assert!(messages[1].content.starts_with("บริบทหน้าจอตอนนี้ (JSON): "));
        assert!(messages[1].content.contains("\"page\":\"ideas\""));
        assert_eq!(messages[2].content, "สวัสดี");
        assert_eq!(messages[3].content, "ยินดี");

        let no_context = AgentRequest {
            mode: Mode::Easy,
            system_prompt: "คุณคือผู้ช่วย".into(),
            history: vec![ChatMessage::user("สวัสดี")],
            screen_context: None,
        };
        assert_eq!(build_messages(&no_context).len(), 2);
    }

    #[test]
    fn build_request_body_omits_tools_and_max_tokens_conditionally() {
        let cfg = LlmConfig::new("m", "http://x", "model", "k");
        let messages = vec![ChatMessage::user("hi")];

        let bare = build_request_body(&cfg, &messages, None);
        assert!(bare.get("tools").is_none());
        assert!(bare.get("max_tokens").is_none());
        assert_eq!(bare["model"], "model");
        assert_eq!(bare["messages"].as_array().map(Vec::len), Some(1));

        let mut with_tokens = cfg.clone();
        with_tokens.max_tokens = 512;
        let body = build_request_body(
            &with_tokens,
            &messages,
            Some(vec![ToolDef {
                r#type: "function".into(),
                function: crate::copilot::ToolFunction {
                    name: "top_candidates".into(),
                    description: "d".into(),
                    parameters: json!({"type": "object"}),
                },
            }]),
        );
        assert!(body["tools"].is_array());
        assert_eq!(body["max_tokens"], 512);
    }

    #[test]
    fn agent_event_serializes_tagged_snake_case() {
        assert_eq!(
            serde_json::to_value(AgentEvent::Cancelled).expect("serialize"),
            json!({"type": "cancelled"})
        );
        assert_eq!(
            serde_json::to_value(AgentEvent::Thinking { round: 2 }).expect("serialize"),
            json!({"type": "thinking", "round": 2})
        );
        assert_eq!(
            serde_json::to_value(AgentEvent::ToolStart {
                call_id: "c1".into(),
                name: "top_candidates".into(),
                args: json!({"count": 5}),
                tier: Tier::Read,
            })
            .expect("serialize"),
            json!({"type": "tool_start", "call_id": "c1", "name": "top_candidates", "args": {"count": 5}, "tier": "read"})
        );
        assert_eq!(
            serde_json::to_value(AgentEvent::Answer {
                text: "ตอบ".into(),
                unverified: true,
                ungrounded: vec!["$999".into()],
            })
            .expect("serialize"),
            json!({"type": "answer", "text": "ตอบ", "unverified": true, "ungrounded": ["$999"]})
        );
        assert_eq!(
            serde_json::to_value(AgentEvent::UiAction {
                ui_id: "ui-1".into(),
                action: json!({"kind": "go", "mode": "pro", "page": "home"}),
            })
            .expect("serialize"),
            json!({
                "type": "ui_action",
                "ui_id": "ui-1",
                "action": {"kind": "go", "mode": "pro", "page": "home"}
            })
        );
    }

    #[test]
    fn run_limits_defaults_match_the_product_numbers() {
        let limits = RunLimits::default();
        assert_eq!(limits.max_tool_calls, 12);
        assert_eq!(limits.max_webull_cost, 150);
        assert_eq!(limits.max_rounds, 8);
        assert_eq!(limits.llm_timeout_secs, 90);
    }
}
