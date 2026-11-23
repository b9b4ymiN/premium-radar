//! Wiremock integration tests for the real Webull HTTP transport
//! (throttle + cache + retry). Run with `--features client`.
//!
//! The blocking reqwest client panics inside a tokio runtime, so each test
//! spawns a plain OS thread for the client call while wiremock's tokio
//! server runs in the test body.

#![cfg(feature = "client")]

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use radar_core::webull::rest::{Credentials, HttpFetch};
use radar_core::webull::transport::WebullClient;

fn test_creds() -> Credentials {
    Credentials {
        app_key: "TESTKEY".into(),
        app_secret: "TESTSECRET".into(),
        access_token: None,
    }
}

/// Run the blocking client on a plain OS thread (no tokio runtime conflict).
fn call_on_thread<F, T>(uri: String, f: F) -> T
where
    F: FnOnce(&mut WebullClient) -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let mut c = WebullClient::new(test_creds()).with_base_url(uri);
        f(&mut c)
    })
    .join()
    .expect("client thread")
}

#[tokio::test]
async fn basic_get_returns_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/test/path"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [1, 2, 3]})))
        .mount(&server)
        .await;
    let uri = server.uri();
    let (status, body) = call_on_thread(uri, |c| c.get("/test/path", &[]).unwrap());
    assert_eq!(status, 200);
    assert_eq!(body["data"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn rate_limit_429_retries_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/retry"))
        .respond_with(
            ResponseTemplate::new(429).set_body_json(json!({"message": "rate limit exceeded"})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/retry"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;
    let uri = server.uri();
    let (status, body) = call_on_thread(uri, |c| c.get("/retry", &[]).unwrap());
    assert_eq!(status, 200);
    assert_eq!(body["ok"], json!(true));
}

#[tokio::test]
async fn cache_hit_does_not_refetch() {
    let server = MockServer::start().await;
    let mock = Mock::given(method("GET"))
        .and(path("/cached"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"v": 1})))
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let uri = server.uri();
    let result = std::thread::spawn(move || {
        let mut c = WebullClient::new(test_creds()).with_base_url(uri);
        let r1 = c.get("/cached", &[("q".into(), "1".into())]).unwrap();
        let r2 = c.get("/cached", &[("q".into(), "1".into())]).unwrap();
        (r1, r2)
    })
    .join()
    .unwrap();
    let ((s1, b1), (s2, b2)) = result;
    assert_eq!(s1, 200);
    assert_eq!(s2, 200);
    assert_eq!(b1, b2);
    drop(mock);
}

#[tokio::test]
async fn different_params_miss_cache() {
    let server = MockServer::start().await;
    let mock = Mock::given(method("GET"))
        .and(path("/multi"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"v": 1})))
        .expect(2)
        .mount_as_scoped(&server)
        .await;
    let uri = server.uri();
    std::thread::spawn(move || {
        let mut c = WebullClient::new(test_creds()).with_base_url(uri);
        let _ = c.get("/multi", &[("a".into(), "1".into())]).unwrap();
        let _ = c.get("/multi", &[("a".into(), "2".into())]).unwrap();
    })
    .join()
    .unwrap();
    drop(mock);
}

#[tokio::test]
async fn non_200_not_cached() {
    let server = MockServer::start().await;
    let mock = Mock::given(method("GET"))
        .and(path("/err"))
        .respond_with(ResponseTemplate::new(500))
        .expect(2)
        .mount_as_scoped(&server)
        .await;
    let uri = server.uri();
    let (s1, s2) = std::thread::spawn(move || {
        let mut c = WebullClient::new(test_creds()).with_base_url(uri);
        let (a, _) = c.get("/err", &[]).unwrap();
        let (b, _) = c.get("/err", &[]).unwrap();
        (a, b)
    })
    .join()
    .unwrap();
    assert_eq!(s1, 500);
    assert_eq!(s2, 500);
    drop(mock);
}

#[tokio::test]
async fn signing_headers_present_in_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/signed"))
        .and(wiremock::matchers::header_exists("x-app-key"))
        .and(wiremock::matchers::header_exists("x-signature"))
        .and(wiremock::matchers::header_exists("x-timestamp"))
        .and(wiremock::matchers::header_exists("x-signature-nonce"))
        .and(wiremock::matchers::header_exists("x-signature-algorithm"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"signed": true})))
        .mount(&server)
        .await;
    let uri = server.uri();
    let (status, body) = std::thread::spawn(move || {
        let mut c = WebullClient::new(test_creds()).with_base_url(uri);
        c.get("/signed", &[("s".into(), "v".into())]).unwrap()
    })
    .join()
    .unwrap();
    assert_eq!(status, 200);
    assert_eq!(body["signed"], json!(true));
}

// ---------- raw app-secret must never travel, signature must stay valid ----------

fn header<'a>(req: &'a wiremock::Request, name: &str) -> &'a str {
    req.headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
}

/// GET: no `x-app-secret` header on the wire, and the `x-signature` still
/// recomputes with the secret as the HMAC key — the secret authenticates
/// without being disclosed.
#[tokio::test]
async fn get_omits_raw_secret_but_keeps_signature() {
    let server = MockServer::start().await;
    let params = vec![
        ("symbols".to_string(), "NVDA/US".to_string()),
        ("category".to_string(), "US_STOCK".to_string()),
    ];
    Mock::given(method("GET"))
        .and(path("/market/snapshot"))
        .and(wiremock::matchers::header_exists("x-signature"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;
    let uri = server.uri();
    let send_params = params.clone();
    let (status, _body) = call_on_thread(uri, move |c| {
        c.get("/market/snapshot", &send_params).unwrap()
    });
    assert_eq!(status, 200);

    let reqs = server.received_requests().await.expect("captured requests");
    let req = reqs.last().expect("one request captured");
    assert!(
        req.headers.get("x-app-secret").is_none(),
        "raw app secret must not travel in GET"
    );
    let expected = radar_core::webull::sign::sign(
        "/market/snapshot",
        &radar_core::webull::transport::host_from_base_url(&server.uri()),
        &params,
        header(req, "x-timestamp"),
        header(req, "x-signature-nonce"),
        "TESTKEY",
        "TESTSECRET",
        "",
    );
    assert_eq!(header(req, "x-signature"), expected);
}

/// POST (token endpoint shape): no `x-app-secret` header on the wire, and
/// the signature still covers the MD5 body hash with the secret as key.
#[tokio::test]
async fn post_omits_raw_secret_but_keeps_signature() {
    let server = MockServer::start().await;
    let body = r#"{"account":"acct","passwd":"pwd"}"#;
    Mock::given(method("POST"))
        .and(path("/auth/token"))
        .and(wiremock::matchers::header_exists("x-signature"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"accessToken": "t"})))
        .mount(&server)
        .await;
    let uri = server.uri();
    let body_owned = body.to_string();
    let (status, resp) = call_on_thread(uri, move |c| {
        c.post_json("/auth/token", &body_owned).unwrap()
    });
    assert_eq!(status, 200);
    assert_eq!(resp["accessToken"], json!("t"));

    let reqs = server.received_requests().await.expect("captured requests");
    let req = reqs.last().expect("one request captured");
    assert!(
        req.headers.get("x-app-secret").is_none(),
        "raw app secret must not travel in POST"
    );
    let expected = radar_core::webull::sign::sign(
        "/auth/token",
        &radar_core::webull::transport::host_from_base_url(&server.uri()),
        &[],
        header(req, "x-timestamp"),
        header(req, "x-signature-nonce"),
        "TESTKEY",
        "TESTSECRET",
        body,
    );
    assert_eq!(header(req, "x-signature"), expected);
}

// ---------- live smoke (ignored by default; run with -- --ignored) ----------

#[tokio::test]
#[ignore = "live smoke — needs WEBULL_APP_KEY + WEBULL_APP_SECRET in env"]
async fn live_smoke_fetch_spot() {
    let creds = Credentials::from_env().expect("WEBULL_APP_KEY / WEBULL_APP_SECRET must be set");
    let (status, body) = std::thread::spawn(move || {
        let mut c = WebullClient::new(creds);
        c.get(
            "/openapi/market-data/stock/snapshot",
            &[
                ("symbols".into(), "NVDA".into()),
                ("category".into(), "US_STOCK".into()),
            ],
        )
        .expect("live call")
    })
    .join()
    .unwrap();
    assert_eq!(status, 200, "body: {body}");
    let list = radar_core::webull::rest::extract_list(&body);
    assert!(!list.is_empty(), "expected at least one row for NVDA");
}
