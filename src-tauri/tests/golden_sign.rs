//! Golden tests for Webull request signing against values produced by the
//! real Python `_sign` (test key pair, fixed ts/nonce — fully deterministic).

use serde::Deserialize;
use std::path::PathBuf;

use premium_radar_lib::webull::sign::{sign, sign_base};

#[derive(Deserialize)]
struct SignCase {
    path: String,
    query: Vec<(String, String)>,
    ts: String,
    nonce: String,
    body: String,
    expected_sig: String,
}
#[derive(Deserialize)]
struct SignFile {
    host: String,
    app_key: String,
    cases: Vec<SignCase>,
}

#[test]
fn golden_signatures_match_python() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/webull_sign.json");
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("read fixture: {e}"));
    let f: SignFile = serde_json::from_slice(&raw).unwrap_or_else(|e| panic!("parse: {e}"));
    assert_eq!(f.host, "api.webull.co.th");
    // sanity: the base payload embeds the fixture host before signing
    let base = sign_base(
        &f.cases[0].path,
        &f.cases[0].query,
        &f.cases[0].ts,
        &f.cases[0].nonce,
        &f.app_key,
        &f.cases[0].body,
    );
    assert!(base.contains("host=api.webull.co.th"), "base = {base}");

    for (i, c) in f.cases.iter().enumerate() {
        let got = sign(&c.path, &c.query, &c.ts, &c.nonce, &f.app_key, "TESTSECRET456", &c.body);
        assert_eq!(got, c.expected_sig, "case {i}");
    }
}
