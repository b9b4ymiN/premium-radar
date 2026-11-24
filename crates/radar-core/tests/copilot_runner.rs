//! Wiremock integration tests for the copilot tool-runner loop: the runner
//! closure fires exactly once per model tool call (with the model's name and
//! arguments) and — the reason the variant exists — the engine mutex the
//! `ask_copilot` wiring locks is provably NOT held while an LLM HTTP
//! round-trip is in flight. Run with `--features client`.
//!
//! The copilot uses a blocking reqwest client, which panics inside a tokio
//! runtime, so each test spawns a plain OS thread for the chat call while
//! wiremock's tokio server runs in the test body (same shape as
//! transport_wiremock.rs).

#![cfg(feature = "client")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use radar_core::copilot::{ChatMessage, LlmConfig, chat_with_tool_runner};
use radar_core::engine::Engine;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Mount the two-round chat: round 1 asks for one `top_candidates` tool call
/// (one-shot), round 2 answers with text.
async fn mount_two_round_chat(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "function": {
                            "name": "top_candidates",
                            "arguments": "{\"count\": 3}"
                        }
                    }]
                }
            }]
        })))
        .up_to_n_times(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {"role": "assistant", "content": "ตอบเสร็จแล้ว"}
            }]
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn runner_variant_executes_one_tool_and_returns_final_text() {
    let server = MockServer::start().await;
    mount_two_round_chat(&server).await;

    let cfg = LlmConfig::new("mock", server.uri(), "m", "k");
    let history = vec![ChatMessage::user("ทดสอบ")];

    let (answer, calls) = std::thread::spawn(move || {
        let mut calls: Vec<(String, String)> = Vec::new();
        let answer = chat_with_tool_runner(&cfg, &history, &mut |name, arguments| {
            calls.push((name.to_string(), arguments.to_string()));
            json!({"candidates": []}).to_string()
        });
        (answer, calls)
    })
    .join()
    .expect("copilot thread");

    assert_eq!(answer, Ok("ตอบเสร็จแล้ว".to_string()));
    assert_eq!(
        calls,
        vec![("top_candidates".to_string(), "{\"count\": 3}".to_string())],
        "the runner must fire exactly once with the model's name+arguments"
    );
    let reqs = server.received_requests().await.expect("captured requests");
    assert_eq!(reqs.len(), 2, "exactly two chat rounds");
}

/// Serves `body` and records whether the shared engine mutex was lockable at
/// the moment the response was produced — i.e. while a chat request was in
/// flight.
struct LockProbe {
    body: Value,
    engine: Arc<Mutex<Engine>>,
    lockable_in_flight: Arc<AtomicBool>,
}

impl Respond for LockProbe {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        if self.engine.try_lock().is_ok() {
            self.lockable_in_flight.store(true, Ordering::SeqCst);
        }
        ResponseTemplate::new(200).set_body_json(self.body.clone())
    }
}

/// The `ask_copilot` wiring: lock the engine only inside the tool runner.
/// With the old "lock across the whole conversation" shape the try_lock in
/// [`LockProbe`] deterministically fails, so this test fails.
#[tokio::test]
async fn engine_lock_is_not_held_while_a_chat_round_is_in_flight() {
    let server = MockServer::start().await;
    let engine = Arc::new(Mutex::new(Engine::new(chrono::Utc::now())));
    let lockable_in_flight = Arc::new(AtomicBool::new(false));
    let probe = |body: Value| LockProbe {
        body,
        engine: engine.clone(),
        lockable_in_flight: lockable_in_flight.clone(),
    };
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(probe(json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "function": {
                            "name": "chain_stats",
                            "arguments": "{}"
                        }
                    }]
                }
            }]
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(probe(json!({
            "choices": [{
                "message": {"role": "assistant", "content": "เรียบร้อย"}
            }]
        })))
        .mount(&server)
        .await;

    let cfg = LlmConfig::new("mock", server.uri(), "m", "k");
    let history = vec![ChatMessage::user("สถิติ chain")];
    let chat_engine = engine.clone();
    let answer = std::thread::spawn(move || {
        chat_with_tool_runner(&cfg, &history, &mut |name, arguments| {
            let mut engine = chat_engine
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            radar_core::copilot::execute_tool(name, arguments, &mut engine)
        })
    })
    .join()
    .expect("copilot thread");

    assert_eq!(answer, Ok("เรียบร้อย".to_string()));
    assert!(
        lockable_in_flight.load(Ordering::SeqCst),
        "the engine mutex must be lockable while an LLM HTTP round is in flight"
    );
}
