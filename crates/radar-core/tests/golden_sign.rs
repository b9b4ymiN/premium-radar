//! Golden tests for Webull request signing against values produced by the
//! real Python `_sign` (test key pair, fixed ts/nonce — fully deterministic).

use std::collections::BTreeMap;

use serde::Deserialize;
use std::path::PathBuf;

use radar_core::webull::sign::{sign, sign_base};

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
