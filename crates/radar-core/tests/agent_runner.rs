//! Wiremock integration tests for the agent runner loop: validated tool
//! dispatch (defaults applied), multi-call replies, validation errors fed
//! back to the model, duplicate-call suppression, quota enforcement,
//! cancellation, and the "no caller lock held across an LLM HTTP
//! round-trip" proof. Run with `--features client`.
//!
//! `run_agent` uses a blocking reqwest client, which panics inside a
//! tokio runtime, so each test runs the loop on a plain OS thread while
//! wiremock's tokio server runs in the test body (same shape as
//! copilot_runner.rs).

#![cfg(feature = "client")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use radar_core::agent::tools::screener_specs;
use radar_core::agent::{
    AgentEvent, AgentOutcome, AgentRequest, Mode, Registry, RunLimits, Tier, ToolSpec, run_agent,
};
use radar_core::copilot::{ChatMessage, LlmConfig};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn test_registry() -> Registry {
    let mut registry = Registry::new();
    for spec in screener_specs() {
        registry.register(spec).expect("register screener spec");
    }
    registry
}

fn agent_request() -> AgentRequest {
    AgentRequest {
        mode: Mode::Easy,
        system_prompt: "test".into(),
        history: vec![ChatMessage::user("ถาม")],
        screen_context: None,
    }
}

/// Run the blocking agent loop on an OS thread and collect
/// (outcome, events). The cancel flag is shared so executors can flip it.
fn spawn_agent(
    cfg: LlmConfig,
    limits: RunLimits,
    cancel: Arc<AtomicBool>,
    mut exec: impl FnMut(&ToolSpec, &Value) -> Result<String, String> + Send + 'static,
) -> (Result<AgentOutcome, String>, Vec<AgentEvent>) {
    let registry = test_registry();
    let req = agent_request();
    std::thread::spawn(move || {
        let mut events = Vec::new();
        let outcome = run_agent(
            &cfg,
            &registry,
            &req,
            &limits,
            cancel.as_ref(),
            &mut exec,
            &mut |event| events.push(event),
        );
        (outcome, events)
    })
    .join()
    .expect("agent thread")
}

/// A one-shot mock round serving `body` (wiremock matches in mount order,
/// so rounds cascade and the answer mock mounted last catches the rest).
async fn mount_round(server: &MockServer, body: Value) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .up_to_n_times(1)
        .mount(server)
        .await;
}

async fn mount_answer(server: &MockServer, text: &str) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"role": "assistant", "content": text}}]
        })))
        .mount(server)
        .await;
}

/// A reply carrying several tool calls: (id, name, arguments) triples.
fn tool_calls_reply(calls: &[(&str, &str, &str)]) -> Value {
    let tool_calls: Vec<Value> = calls
        .iter()
        .map(|(id, name, args)| {
            json!({
                "id": id,
                "type": "function",
                "function": {"name": name, "arguments": args}
            })
        })
        .collect();
    json!({
        "choices": [{
            "message": {"role": "assistant", "content": null, "tool_calls": tool_calls}
        }]
    })
}

fn request_body(request: &wiremock::Request) -> Value {
    serde_json::from_slice(&request.body).expect("request body is JSON")
}

/// ToolEnd carries wall-clock ms — zero it out for whole-sequence
/// comparison.
fn zero_ms(events: Vec<AgentEvent>) -> Vec<AgentEvent> {
    events
        .into_iter()
        .map(|event| match event {
            AgentEvent::ToolEnd {
                call_id,
                name,
                ok,
                summary,
                ms: _,
            } => AgentEvent::ToolEnd {
                call_id,
                name,
                ok,
                summary,
                ms: 0,
            },
            other => other,
        })
        .collect()
}

// (a) one tool call then answer; executor sees validated args incl. defaults
#[tokio::test]
async fn one_tool_call_then_answer_executor_gets_validated_args_with_defaults() {
    let server = MockServer::start().await;
    mount_round(
        &server,
        tool_calls_reply(&[("call_1", "top_candidates", r#"{"count": 3}"#)]),
    )
    .await;
    mount_answer(&server, "ตอบเสร็จแล้ว").await;

    let calls: Arc<Mutex<Vec<(String, Value)>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = calls.clone();
    let (outcome, _events) = spawn_agent(
        LlmConfig::new("mock", server.uri(), "m", "k"),
        RunLimits::default(),
        Arc::new(AtomicBool::new(false)),
        move |spec, args| {
            recorder
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((spec.name.clone(), args.clone()));
            Ok(json!({"candidates": []}).to_string())
        },
    );

    assert_eq!(outcome, Ok(AgentOutcome::Answer("ตอบเสร็จแล้ว".into())));
    let recorded = calls.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert_eq!(recorded.len(), 1, "executor fires exactly once");
    assert_eq!(recorded[0].0, "top_candidates");
    assert_eq!(
        recorded[0].1,
        json!({"count": 3, "type": "all"}),
        "schema defaults must be filled in before the executor sees the args"
    );
    let reqs = server.received_requests().await.expect("captured");
    assert_eq!(reqs.len(), 2, "exactly two chat rounds");
}

// (b) two tool_calls in one reply → one assistant message + two tool messages
#[tokio::test]
async fn two_tool_calls_in_one_reply_pair_assistant_and_tool_messages() {
    let server = MockServer::start().await;
    mount_round(
        &server,
        tool_calls_reply(&[
            ("call_a", "top_candidates", r#"{"count": 2}"#),
            ("call_b", "chain_stats", "{}"),
        ]),
    )
    .await;
    mount_answer(&server, "สรุปสองตัว").await;

    let fired: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = fired.clone();
    let (outcome, _events) = spawn_agent(
        LlmConfig::new("mock", server.uri(), "m", "k"),
        RunLimits::default(),
        Arc::new(AtomicBool::new(false)),
        move |spec, _args| {
            recorder
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(spec.name.clone());
            Ok(json!({"which": spec.name}).to_string())
        },
    );

    assert_eq!(outcome, Ok(AgentOutcome::Answer("สรุปสองตัว".into())));
    let fired = fired.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert_eq!(fired, vec!["top_candidates", "chain_stats"]);

    let reqs = server.received_requests().await.expect("captured");
    assert_eq!(reqs.len(), 2);
    let messages = request_body(&reqs[1])["messages"]
        .as_array()
        .expect("messages")
        .clone();
    let idx = messages
        .iter()
        .position(|m| m.get("tool_calls").is_some())
        .expect("assistant message carrying tool_calls");
    let tool_calls = messages[idx]["tool_calls"].as_array().expect("two calls");
    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0]["id"], "call_a");
    assert_eq!(tool_calls[0]["function"]["name"], "top_candidates");
    assert_eq!(tool_calls[1]["id"], "call_b");
    assert_eq!(tool_calls[1]["function"]["name"], "chain_stats");

    // exactly two tool messages follow, in the same order with matching ids
    assert_eq!(messages[idx + 1]["role"], "tool");
    assert_eq!(messages[idx + 1]["tool_call_id"], "call_a");
    assert!(
        messages[idx + 1]["content"]
            .as_str()
            .expect("str")
            .contains("top_candidates")
    );
    assert_eq!(messages[idx + 2]["role"], "tool");
    assert_eq!(messages[idx + 2]["tool_call_id"], "call_b");
    assert!(
        messages[idx + 2]["content"]
            .as_str()
            .expect("str")
            .contains("chain_stats")
    );
}

// (c) invalid args → executor not called, model receives the validation error
#[tokio::test]
async fn invalid_args_skip_executor_and_return_validation_error_to_model() {
    let server = MockServer::start().await;
    mount_round(
        &server,
        tool_calls_reply(&[("call_1", "top_candidates", r#"{"count": "many"}"#)]),
    )
    .await;
    mount_answer(&server, "ตอบจากข้อมูลเดิม").await;

    let fired: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = fired.clone();
    let (outcome, events) = spawn_agent(
        LlmConfig::new("mock", server.uri(), "m", "k"),
        RunLimits::default(),
        Arc::new(AtomicBool::new(false)),
        move |spec, args| {
            recorder
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(json!({"name": spec.name, "args": args}));
            Ok("never".into())
        },
    );

    assert_eq!(outcome, Ok(AgentOutcome::Answer("ตอบจากข้อมูลเดิม".into())));
    assert!(
        fired
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty(),
        "executor must not run for an invalid call"
    );
    // the ToolEnd event reports the failure
    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::ToolEnd { ok: false, summary, .. } if summary.contains("wrong_type")
    )));

    let reqs = server.received_requests().await.expect("captured");
    assert_eq!(reqs.len(), 2);
    let messages = request_body(&reqs[1])["messages"]
        .as_array()
        .expect("messages")
        .clone();
    let tool_message = messages
        .iter()
        .rev()
        .find(|m| m["role"] == "tool")
        .expect("tool result message");
    let payload: Value = serde_json::from_str(tool_message["content"].as_str().expect("str"))
        .expect("tool content is JSON");
    assert_eq!(payload["error"], "wrong_type");
    assert!(
        payload["message"].as_str().expect("str").contains("count"),
        "the Thai fix-it message names the field: {}",
        payload["message"]
    );
}

// (d) duplicate call → executor fires once, second tool message carries the note
#[tokio::test]
async fn duplicate_call_replays_previous_result_without_executing() {
    let server = MockServer::start().await;
    mount_round(
        &server,
        tool_calls_reply(&[("call_1", "top_candidates", "{}")]),
    )
    .await;
    mount_round(
        &server,
        tool_calls_reply(&[("call_2", "top_candidates", "{}")]),
    )
    .await;
    mount_answer(&server, "จบแล้ว").await;

    let fired: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
    let counter = fired.clone();
    let (outcome, _events) = spawn_agent(
        LlmConfig::new("mock", server.uri(), "m", "k"),
        RunLimits::default(),
        Arc::new(AtomicBool::new(false)),
        move |_spec, _args| {
            *counter.lock().unwrap_or_else(PoisonError::into_inner) += 1;
            Ok(r#"{"ok": true}"#.into())
        },
    );

    assert_eq!(outcome, Ok(AgentOutcome::Answer("จบแล้ว".into())));
    assert_eq!(
        *fired.lock().unwrap_or_else(PoisonError::into_inner),
        1,
        "the repeat must be served from cache"
    );

    let reqs = server.received_requests().await.expect("captured");
    assert_eq!(reqs.len(), 3);
    let messages = request_body(&reqs[2])["messages"]
        .as_array()
        .expect("messages")
        .clone();
    let tool_messages: Vec<&Value> = messages.iter().filter(|m| m["role"] == "tool").collect();
    assert_eq!(tool_messages.len(), 2);
    let first: Value = serde_json::from_str(tool_messages[0]["content"].as_str().expect("str"))
        .expect("first result JSON");
    assert_eq!(first["ok"], true);
    let second: Value = serde_json::from_str(tool_messages[1]["content"].as_str().expect("str"))
        .expect("dup note JSON");
    assert!(
        second["note"]
            .as_str()
            .expect("str")
            .contains("เรียกซ้ำด้วยค่าเดิม"),
        "got: {second}"
    );
    assert_eq!(second["result"], r#"{"ok": true}"#);
}

// (e) max_tool_calls reached → final request has no tools key, its answer wins
#[tokio::test]
async fn quota_exhausted_sends_one_final_request_without_tools() {
    let server = MockServer::start().await;
    mount_round(
        &server,
        tool_calls_reply(&[("call_1", "chain_stats", "{}")]),
    )
    .await;
    mount_answer(&server, "ตอบจากข้อมูลที่มีอยู่").await;

    let (outcome, _events) = spawn_agent(
        LlmConfig::new("mock", server.uri(), "m", "k"),
        RunLimits {
            max_tool_calls: 1,
            ..Default::default()
        },
        Arc::new(AtomicBool::new(false)),
        |_spec, _args| Ok(json!({"stats": 1}).to_string()),
    );

    assert_eq!(outcome, Ok(AgentOutcome::Answer("ตอบจากข้อมูลที่มีอยู่".into())));
    let reqs = server.received_requests().await.expect("captured");
    assert_eq!(reqs.len(), 2, "one tool round + one final request");
    let final_body = request_body(&reqs[1]);
    assert!(
        final_body.get("tools").is_none(),
        "the final request must not offer tools: {final_body}"
    );
    let messages = final_body["messages"].as_array().expect("messages");
    assert!(
        messages.iter().any(|m| m["role"] == "system"
            && m["content"]
                .as_str()
                .expect("str")
                .contains("ใช้ tool ครบโควตาแล้ว")),
        "the final request must carry the answer-from-what-you-have system nudge"
    );
}

// (e2) a reply with more tool_calls than the remaining quota → the cap is
// enforced mid-batch: extras are refused (never executed) but still paired
#[tokio::test]
async fn batch_larger_than_remaining_quota_refuses_extra_calls_mid_batch() {
    let server = MockServer::start().await;
    mount_round(
        &server,
        tool_calls_reply(&[
            ("call_1", "top_candidates", r#"{"count": 1}"#),
            ("call_2", "top_candidates", r#"{"count": 2}"#),
            ("call_3", "chain_stats", "{}"),
        ]),
    )
    .await;
    mount_answer(&server, "ตอบจากข้อมูลที่มี").await;

    let fired: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = fired.clone();
    let (outcome, _events) = spawn_agent(
        LlmConfig::new("mock", server.uri(), "m", "k"),
        RunLimits {
            max_tool_calls: 2,
            ..Default::default()
        },
        Arc::new(AtomicBool::new(false)),
        move |spec, _args| {
            recorder
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(spec.name.clone());
            Ok(json!({"ok": true}).to_string())
        },
    );

    assert_eq!(outcome, Ok(AgentOutcome::Answer("ตอบจากข้อมูลที่มี".into())));
    let fired = fired.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert_eq!(
        fired,
        vec!["top_candidates", "top_candidates"],
        "only the in-quota calls reach the executor"
    );

    let reqs = server.received_requests().await.expect("captured");
    assert_eq!(reqs.len(), 2, "quota spent in round 1 ⇒ final request next");
    let final_body = request_body(&reqs[1]);
    assert!(
        final_body.get("tools").is_none(),
        "the follow-up request must not offer tools: {final_body}"
    );
    let messages = final_body["messages"].as_array().expect("messages").clone();
    let tool_messages: Vec<&Value> = messages.iter().filter(|m| m["role"] == "tool").collect();
    assert_eq!(
        tool_messages.len(),
        3,
        "every tool_call_id gets a paired reply"
    );
    assert_eq!(tool_messages[0]["tool_call_id"], "call_1");
    assert_eq!(tool_messages[1]["tool_call_id"], "call_2");
    assert_eq!(tool_messages[2]["tool_call_id"], "call_3");
    let third: Value = serde_json::from_str(tool_messages[2]["content"].as_str().expect("str"))
        .expect("refusal payload is JSON");
    assert_eq!(third["error"], "quota_exhausted");
    assert!(
        third["message"]
            .as_str()
            .expect("str")
            .contains("ใช้ tool ครบโควตาของคำถามนี้แล้ว"),
        "got: {third}"
    );
}

// (f) cancel set inside the executor → Cancelled, no further HTTP
#[tokio::test]
async fn cancel_inside_executor_stops_the_loop_without_further_http() {
    let server = MockServer::start().await;
    mount_round(
        &server,
        tool_calls_reply(&[("call_1", "chain_stats", "{}")]),
    )
    .await;
    mount_answer(&server, "should never happen").await;

    let cancel = Arc::new(AtomicBool::new(false));
    let trip = cancel.clone();
    let (outcome, events) = spawn_agent(
        LlmConfig::new("mock", server.uri(), "m", "k"),
        RunLimits::default(),
        cancel,
        move |_spec, _args| {
            trip.store(true, Ordering::SeqCst);
            Ok(json!({"stats": 1}).to_string())
        },
    );

    assert_eq!(outcome, Ok(AgentOutcome::Cancelled));
    assert_eq!(*events.last().expect("events"), AgentEvent::Cancelled);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentEvent::Answer { .. }))
    );
    let reqs = server.received_requests().await.expect("captured");
    assert_eq!(reqs.len(), 1, "no HTTP after the cancel flag fired");
}

// (g) a mutex locked only inside the executor is NOT held while HTTP is in flight
struct LockProbe {
    body: Value,
    guard: Arc<Mutex<()>>,
    lockable_in_flight: Arc<AtomicBool>,
}

impl Respond for LockProbe {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        if self.guard.try_lock().is_ok() {
            self.lockable_in_flight.store(true, Ordering::SeqCst);
        }
        ResponseTemplate::new(200).set_body_json(self.body.clone())
    }
}

#[tokio::test]
async fn executor_lock_is_not_held_while_an_llm_round_is_in_flight() {
    let server = MockServer::start().await;
    let guard: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
    let lockable_in_flight = Arc::new(AtomicBool::new(false));
    let probe = |body: Value| LockProbe {
        body,
        guard: guard.clone(),
        lockable_in_flight: lockable_in_flight.clone(),
    };
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(probe(tool_calls_reply(&[("call_1", "chain_stats", "{}")])))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(probe(json!({
            "choices": [{"message": {"role": "assistant", "content": "เรียบร้อย"}}]
        })))
        .mount(&server)
        .await;

    let engine_lock = guard.clone();
    let (outcome, _events) = spawn_agent(
        LlmConfig::new("mock", server.uri(), "m", "k"),
        RunLimits::default(),
        Arc::new(AtomicBool::new(false)),
        move |_spec, _args| {
            let _held = engine_lock.lock().unwrap_or_else(PoisonError::into_inner);
            Ok(json!({"stats": 1}).to_string())
        },
    );

    assert_eq!(outcome, Ok(AgentOutcome::Answer("เรียบร้อย".into())));
    assert!(
        lockable_in_flight.load(Ordering::SeqCst),
        "the executor mutex must be lockable while an LLM HTTP round is in flight"
    );
}

// (h) exact event sequence for the one-tool-call scenario
#[tokio::test]
async fn event_sequence_for_one_tool_call_scenario_is_exact() {
    let server = MockServer::start().await;
    mount_round(
        &server,
        tool_calls_reply(&[("call_1", "top_candidates", r#"{"count": 3}"#)]),
    )
    .await;
    mount_answer(&server, "ตอบเสร็จแล้ว").await;

    let (_outcome, events) = spawn_agent(
        LlmConfig::new("mock", server.uri(), "m", "k"),
        RunLimits::default(),
        Arc::new(AtomicBool::new(false)),
        |_spec, _args| Ok(json!({"candidates": []}).to_string()),
    );

    assert_eq!(
        zero_ms(events),
        vec![
            AgentEvent::Thinking { round: 1 },
            AgentEvent::ToolStart {
                call_id: "call_1".into(),
                name: "top_candidates".into(),
                args: json!({"count": 3, "type": "all"}),
                tier: Tier::Read,
            },
            AgentEvent::ToolEnd {
                call_id: "call_1".into(),
                name: "top_candidates".into(),
                ok: true,
                summary: "{\"candidates\":[]}".into(),
                ms: 0,
            },
            AgentEvent::Thinking { round: 2 },
            AgentEvent::Answer {
                text: "ตอบเสร็จแล้ว".into(),
                unverified: false,
            },
        ]
    );
}
