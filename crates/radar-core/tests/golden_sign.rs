//! Golden tests for Webull request signing. Both fixtures use the same test key
//! pair with fixed ts/nonce (fully deterministic):
//! - `webull_sign.json`   — V2 scheme (HMAC-SHA1/MD5), produced by the Python
//!   `_sign` this crate was ported from.
//! - `webull_sign_v3.json` — V3 scheme (HMAC-SHA256/SHA-256), produced by the
//!   official SDK's `default_signature_composer.calc_signature` with the clock
//!   pinned (webull-inc/webull-openapi-python-sdk @ a444472, 2026-11-20).

use std::collections::BTreeMap;

use serde::Deserialize;
use std::path::PathBuf;

use radar_core::webull::sign::{SignatureScheme, sign, sign_base};

/// The fixture is a flat array; each case carries its own host/app_key.
#[derive(Deserialize)]
struct SignCase {
    host: String,
    app_key: String,
    path: String,
    query: BTreeMap<String, String>,
    ts: String,
    nonce: String,
    body: String,
    expected_sig: String,
}

#[test]
fn golden_signatures_match_python() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/webull_sign.json");
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("read fixture: {e}"));
    let cases: Vec<SignCase> =
        serde_json::from_slice(&raw).unwrap_or_else(|e| panic!("parse: {e}"));
    assert!(
        cases.len() >= 4,
        "expected the full case set, got {}",
        cases.len()
    );

    // sanity: the base payload embeds the fixture host before signing
    let first = &cases[0];
    assert_eq!(first.host, "api.webull.co.th");
    let first_q: Vec<(String, String)> = first
        .query
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let base = sign_base(
        &first.path,
        &first.host,
        &first_q,
        &first.ts,
        &first.nonce,
        &first.app_key,
        &first.body,
    );
    assert!(base.contains("host=api.webull.co.th"), "base = {base}");

    for (i, c) in cases.iter().enumerate() {
        let q: Vec<(String, String)> = c
            .query
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let got = sign(
            &c.path,
            &c.host,
            &q,
            &c.ts,
            &c.nonce,
            &c.app_key,
            "TESTSECRET456",
            &c.body,
        );
        assert_eq!(got, c.expected_sig, "case {i}");
    }
}

fn load_cases(file: &str, minimum: usize) -> Vec<SignCase> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(file);
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("read {file}: {e}"));
    let cases: Vec<SignCase> =
        serde_json::from_slice(&raw).unwrap_or_else(|e| panic!("parse {file}: {e}"));
    assert!(
        cases.len() >= minimum,
        "expected the full case set, got {}",
        cases.len()
    );
    cases
}

#[test]
fn golden_v3_signatures_match_official_sdk() {
    // covers: plain GET, comma-joined batch, query needing encoding, no query,
    // POST with a JSON body, the UAT host, and a non-ASCII query value
    let cases = load_cases("webull_sign_v3.json", 8);
    assert!(
        cases.iter().any(|c| !c.body.is_empty()),
        "fixture must include bodies"
    );
    for (i, c) in cases.iter().enumerate() {
        let q: Vec<(String, String)> = c
            .query
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let got = SignatureScheme::V3.sign(
            &c.path,
            &c.host,
            &q,
            &c.ts,
            &c.nonce,
            &c.app_key,
            "TESTSECRET456",
            &c.body,
        );
        assert_eq!(got, c.expected_sig, "case {i}");
    }
}

#[test]
fn golden_v2_signatures_match_python() {
    let cases = load_cases("webull_sign.json", 4);

    // sanity: the base payload embeds the fixture host before signing
    let first = &cases[0];
    assert_eq!(first.host, "api.webull.co.th");
    let first_q: Vec<(String, String)> = first
        .query
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let base = sign_base(
        &first.path,
        &first.host,
        &first_q,
        &first.ts,
        &first.nonce,
        &first.app_key,
        &first.body,
    );
    assert!(base.contains("host=api.webull.co.th"), "base = {base}");

    for (i, c) in cases.iter().enumerate() {
        let q: Vec<(String, String)> = c
            .query
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let got = SignatureScheme::V2.sign(
            &c.path,
            &c.host,
            &q,
            &c.ts,
            &c.nonce,
            &c.app_key,
            "TESTSECRET456",
            &c.body,
        );
        // the free function is the same V2 signature
        assert_eq!(
            got,
            sign(
                &c.path,
                &c.host,
                &q,
                &c.ts,
                &c.nonce,
                &c.app_key,
                "TESTSECRET456",
                &c.body
            ),
            "case {i}"
        );
        assert_eq!(got, c.expected_sig, "case {i}");
    }
}
