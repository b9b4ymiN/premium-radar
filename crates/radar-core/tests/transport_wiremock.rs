//! Wiremock integration tests for the real Webull HTTP transport
//! (signing on the wire, shared pacing + cache, cancel, retry). Run with
//! `--features client`.
//!
//! The blocking reqwest client panics inside a tokio runtime, so each test
//! spawns a plain OS thread for the client call while wiremock's tokio
//! server runs in the test body.

#![cfg(feature = "client")]

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use radar_core::webull::rest::{Credentials, FetchError, HttpFetch};
use radar_core::webull::sign::SignatureScheme;
use radar_core::webull::transport::WebullClient;

fn test_creds() -> Credentials {
    Credentials {
        app_key: "TESTKEY".into(),
        app_secret: "TESTSECRET".into(),
        access_token: None,
    }
}

/// Run the blocking client on a plain OS thread (no tokio runtime conflict).
/// `scheme` is `None` for the default (the one the app ships with).
fn call_on_thread<F, T>(uri: String, scheme: Option<SignatureScheme>, f: F) -> T
where
    F: FnOnce(&mut WebullClient) -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let mut c = WebullClient::new(test_creds()).with_base_url(uri);
        if let Some(scheme) = scheme {
            c = c.with_signature_scheme(scheme);
        }
        f(&mut c)
    })
    .join()
    .expect("client thread")
}

fn header<'a>(req: &'a Request, name: &str) -> &'a str {
    req.headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
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
    let (status, body) = call_on_thread(uri, None, |c| c.get("/test/path", &[]).unwrap());
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
    let (status, body) = call_on_thread(uri, None, |c| c.get("/retry", &[]).unwrap());
    assert_eq!(status, 200);
    assert_eq!(body["ok"], json!(true));
}

/// The broker can report quota exhaustion as a non-429 status carrying a
/// rate-limit `error_code` — the retry predicate must read the code, not
/// just the status.
#[tokio::test]
async fn rate_limit_error_code_on_a_non_429_status_is_retried() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/quota"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error_code": "TOO_MANY_REQUESTS"
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    // takes over once the one-shot failure mock above is exhausted
    Mock::given(method("GET"))
        .and(path("/quota"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;
    let uri = server.uri();
    let (status, body) = call_on_thread(uri, None, |c| c.get("/quota", &[]).unwrap());
    assert_eq!(status, 200);
    assert_eq!(body["ok"], json!(true));
    let reqs = server.received_requests().await.expect("captured requests");
    assert_eq!(reqs.len(), 2, "the 400 was retried once");
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
    let result = call_on_thread(uri, None, |c| {
        let r1 = c.get("/cached", &[("q".into(), "1".into())]).unwrap();
        let r2 = c.get("/cached", &[("q".into(), "1".into())]).unwrap();
        (r1, r2)
    });
    let ((s1, b1), (s2, b2)) = result;
    assert_eq!(s1, 200);
    assert_eq!(s2, 200);
    assert_eq!(b1, b2);
    drop(mock);
}

/// The cache belongs to the origin, not to one client: Home, the scanner and
/// Portfolio each build their own client, and an identical call inside the TTL
/// must not be sent twice.
#[tokio::test]
async fn clients_on_one_origin_share_the_cache() {
    let server = MockServer::start().await;
    let mock = Mock::given(method("GET"))
        .and(path("/shared"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"v": 1})))
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let uri = server.uri();
    call_on_thread(uri.clone(), None, move |a| {
        let mut b = WebullClient::new(test_creds()).with_base_url(uri);
        a.get("/shared", &[]).unwrap();
        b.get("/shared", &[]).unwrap();
    });
    drop(mock);
}

/// A credential change must never serve previously cached answers: clearing
/// the shared cache makes the next identical GET (from a fresh client, like
/// the commands built after a save) hit the server again.
#[tokio::test]
async fn cleared_shared_cache_refetches() {
    let server = MockServer::start().await;
    let mock = Mock::given(method("GET"))
        .and(path("/after-clear"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"v": 1})))
        .expect(2)
        .mount_as_scoped(&server)
        .await;
    let uri = server.uri();
    call_on_thread(uri.clone(), None, |c| {
        c.get("/after-clear", &[]).unwrap();
    });
    radar_core::webull::transport::clear_shared_cache(&uri);
    call_on_thread(uri, None, |c| {
        c.get("/after-clear", &[]).unwrap();
    });
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
    call_on_thread(uri, None, |c| {
        let _ = c.get("/multi", &[("a".into(), "1".into())]).unwrap();
        let _ = c.get("/multi", &[("a".into(), "2".into())]).unwrap();
    });
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
    let (s1, s2) = call_on_thread(uri, None, |c| {
        let (a, _) = c.get("/err", &[]).unwrap();
        let (b, _) = c.get("/err", &[]).unwrap();
        (a, b)
    });
    assert_eq!(s1, 500);
    assert_eq!(s2, 500);
    drop(mock);
}

#[tokio::test]
async fn independent_clients_share_get_and_post_pacing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/first"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/second"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    let uri = server.uri();
    let elapsed = call_on_thread(uri.clone(), None, move |first| {
        let mut second = WebullClient::new(test_creds()).with_base_url(uri);
        let start = std::time::Instant::now();
        assert_eq!(first.get("/first", &[]).unwrap().0, 200);
        assert_eq!(second.post_json("/second", "{}").unwrap().0, 200);
        start.elapsed()
    });
    let interval = std::time::Duration::from_secs_f64(radar_core::webull::sign::MIN_INTERVAL_SEC);
    assert!(
        elapsed >= interval,
        "two clients sent requests in {elapsed:?}"
    );
}

/// Every signing header, for both schemes on GET and POST: the app secret never
/// travels, the signature recomputes from the request's own timestamp/nonce,
/// and the algorithm/version pair is the scheme's.
#[tokio::test]
async fn wire_headers_match_the_client_scheme() {
    for (scheme, algorithm, version) in [
        (SignatureScheme::V3, "HMAC-SHA256", "v3"),
        (SignatureScheme::V2, "HMAC-SHA1", "v2"),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/market/snapshot"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/auth/tokens/check"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "NORMAL"})))
            .mount(&server)
            .await;
        let uri = server.uri();
        let host = radar_core::webull::transport::host_from_base_url(&server.uri());
        let params = vec![
            ("symbols".to_string(), "NVDA,SPY".to_string()),
            ("category".to_string(), "US_STOCK".to_string()),
        ];
        let body = r#"{"token":"abc"}"#;
        let get_params = params.clone();
        let send_body = body.to_string();
        let ((get_status, get_body), (post_status, post_body)) =
            call_on_thread(uri, Some(scheme), move |c| {
                let get = c.get("/market/snapshot", &get_params).unwrap();
                let post = c.post_json("/auth/tokens/check", &send_body).unwrap();
                (get, post)
            });
        // the parsed response JSON is the mocked body, for both verbs
        assert_eq!(get_status, 200, "{scheme:?}");
        assert_eq!(get_body, json!({"ok": true}), "{scheme:?}");
        assert_eq!(post_status, 200, "{scheme:?}");
        assert_eq!(post_body, json!({"status": "NORMAL"}), "{scheme:?}");

        let reqs = server.received_requests().await.expect("captured requests");
        assert_eq!(reqs.len(), 2, "{scheme:?}");
        for req in &reqs {
            assert!(
                req.headers.get("x-app-secret").is_none(),
                "raw app secret must never travel ({scheme:?} {})",
                req.method
            );
            assert_eq!(header(req, "x-app-key"), "TESTKEY");
            assert_eq!(header(req, "x-signature-algorithm"), algorithm);
            assert_eq!(header(req, "x-signature-version"), "1.0");
            assert_eq!(header(req, "x-version"), version);
            assert!(!header(req, "x-timestamp").is_empty());
            assert!(!header(req, "x-signature-nonce").is_empty());
        }
        let [get, post] = [&reqs[0], &reqs[1]];
        assert_eq!(get.method.as_str(), "GET");
        assert_eq!(post.method.as_str(), "POST");
        assert_eq!(header(post, "content-type"), "application/json");
        let signed = |req: &Request, signed_path: &str, query: &[(String, String)], body: &str| {
            scheme.sign(
                signed_path,
                &host,
                query,
                header(req, "x-timestamp"),
                header(req, "x-signature-nonce"),
                "TESTKEY",
                "TESTSECRET",
                body,
            )
        };
        assert_eq!(
            header(get, "x-signature"),
            signed(get, "/market/snapshot", &params, "")
        );
        assert_eq!(
            header(post, "x-signature"),
            signed(post, "/auth/tokens/check", &[], body)
        );
    }
}

/// The default client signs with the official V3 scheme.
#[tokio::test]
async fn default_scheme_is_v3() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/probe"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    let uri = server.uri();
    call_on_thread(uri, None, |c| {
        c.get("/probe", &[]).unwrap();
    });
    let reqs = server.received_requests().await.expect("captured requests");
    let req = reqs.last().expect("one request captured");
    assert_eq!(header(req, "x-signature-algorithm"), "HMAC-SHA256");
    assert_eq!(header(req, "x-version"), "v3");
}

/// A raised cancel flag drops the request before anything reaches the wire.
#[tokio::test]
async fn cancelled_request_never_reaches_the_wire() {
    let server = MockServer::start().await;
    let uri = server.uri();
    let result = std::thread::spawn(move || {
        let flag = Arc::new(AtomicBool::new(true));
        let mut c = WebullClient::new(test_creds())
            .with_base_url(uri)
            .with_cancel_flag(flag);
        c.get("/never", &[]).map(|_| ())
    })
    .join()
    .unwrap();
    assert!(matches!(result, Err(FetchError::Cancelled)));
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A non-JSON error body (gateway HTML) is kept as clipped text instead of
/// being swallowed into `null`.
#[tokio::test]
async fn non_json_body_is_kept_as_text() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/gateway"))
        .respond_with(
            ResponseTemplate::new(502).set_body_string("<html><body>Bad Gateway</body></html>"),
        )
        .up_to_n_times(3)
        .mount(&server)
        .await;
    let uri = server.uri();
    let (status, body) = call_on_thread(uri, None, |c| c.get("/gateway", &[]).unwrap());
    assert_eq!(status, 502);
    assert_eq!(
        body,
        json!("<html><body>Bad Gateway</body></html>".to_string())
    );
}

/// reqwest's default redirect policy replays custom headers (app key, access
/// token, signature) on whatever host a 3xx `Location` names. The client must
/// return the 3xx as-is instead, so the redirect target sees zero requests.
#[tokio::test]
async fn redirect_never_leaks_credentials_to_another_host() {
    let a = MockServer::start().await;
    let b = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/r"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("Location", format!("{}/leak", b.uri())),
        )
        .mount(&a)
        .await;
    let uri = a.uri();
    let status = std::thread::spawn(move || {
        let mut c = WebullClient::new(Credentials {
            access_token: Some("ACCESS-TOKEN".into()),
            ..test_creds()
        })
        .with_base_url(uri);
        c.get("/r", &[]).unwrap().0
    })
    .join()
    .expect("client thread");
    assert_eq!(
        status, 302,
        "the 3xx must surface instead of being followed"
    );
    assert_eq!(
        a.received_requests().await.unwrap().len(),
        1,
        "a 3xx is terminal — no retry"
    );
    assert_eq!(
        b.received_requests().await.unwrap().len(),
        0,
        "credential headers must never reach the redirect target"
    );
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
