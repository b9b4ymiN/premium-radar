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

// ---------- concurrency: paced starts, capped in-flight ----------

/// Server-side arrival log for the concurrency tests. Wiremock applies a
/// fixed, known delay to every response, so "requests that arrived within
/// one delay of each other" were in flight together — computable from
/// arrivals alone, with no client-side completion race.
#[derive(Default)]
struct Arrivals {
    seen: std::sync::Mutex<Vec<std::time::Instant>>,
}

impl Arrivals {
    fn record(&self) {
        self.seen.lock().unwrap().push(std::time::Instant::now());
    }

    fn sorted(&self) -> Vec<std::time::Instant> {
        let mut v = self.seen.lock().unwrap().clone();
        v.sort();
        v
    }

    /// Most arrivals that overlap inside one response-delay window.
    fn max_in_flight(&self, delay: std::time::Duration) -> usize {
        let starts = self.sorted();
        starts
            .iter()
            .map(|a| {
                starts
                    .iter()
                    .filter(|b| *b >= a && b.duration_since(*a) < delay)
                    .count()
            })
            .max()
            .unwrap_or(0)
    }
}

/// 6 clients of one origin run 6 delayed GETs concurrently: the round-trips
/// overlap (wall time far below 6 serial delays), every pair of request
/// STARTS stays a full MIN_INTERVAL apart, and at most MAX_IN_FLIGHT
/// requests are ever on the wire. The 900 ms delay is deliberate: with
/// 0.235 s pacing, a 4th request only stays off the wire if the in-flight
/// cap actually holds.
#[tokio::test]
async fn concurrent_gets_overlap_round_trips_while_pacing_starts() {
    let server = MockServer::start().await;
    let arrivals = Arc::new(Arrivals::default());
    let delay = std::time::Duration::from_millis(900);
    let responder = {
        let arrivals = arrivals.clone();
        move |_req: &Request| {
            arrivals.record();
            ResponseTemplate::new(200)
                .set_body_json(json!({"ok": true}))
                .set_delay(delay)
        }
    };
    Mock::given(method("GET"))
        .and(path("/overlap"))
        .respond_with(responder)
        .mount(&server)
        .await;
    let uri = server.uri();
    let started = std::time::Instant::now();
    std::thread::spawn(move || {
        let mut threads = Vec::new();
        for i in 0..6 {
            let uri = uri.clone();
            threads.push(std::thread::spawn(move || {
                // one client per thread: like the scan workers, they share
                // only the origin coordinator
                let mut c = WebullClient::new(test_creds()).with_base_url(uri);
                // distinct params -> distinct cache keys: nothing collapses
                let (status, _) = c.get("/overlap", &[("i".into(), i.to_string())]).unwrap();
                assert_eq!(status, 200);
            }));
        }
        for t in threads {
            t.join().unwrap();
        }
    })
    .join()
    .expect("driver thread");
    let elapsed = started.elapsed();

    // overlap: 6 × 900 ms of server work finished far sooner than serially
    assert!(elapsed < delay * 6, "requests did not overlap: {elapsed:?}");
    // starts stay paced one MIN_INTERVAL apart (20 ms arrival jitter budget)
    let min_gap = std::time::Duration::from_secs_f64(radar_core::webull::sign::MIN_INTERVAL_SEC)
        - std::time::Duration::from_millis(20);
    let starts = arrivals.sorted();
    assert_eq!(starts.len(), 6, "every call reached the wire");
    for pair in starts.windows(2) {
        assert!(
            pair[1].duration_since(pair[0]) >= min_gap,
            "starts too close: {pair:?}"
        );
    }
    // and the in-flight ceiling held (the cap, not the pacing, binds here)
    assert_eq!(
        arrivals.max_in_flight(delay),
        3,
        "expected exactly MAX_IN_FLIGHT overlapping requests"
    );
}

/// A 500 response walks every error path in `send_once`; none of them may
/// leak an in-flight permit — after several failures, three delayed calls
/// still run three-wide.
#[tokio::test]
async fn failed_requests_release_in_flight_permits() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fail"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({"boom": true})))
        .mount(&server)
        .await;
    let arrivals = Arc::new(Arrivals::default());
    // 900 ms so three paced starts (0.235 s apart) genuinely coexist on the
    // wire — a shorter delay ends request 0 before request 2 begins.
    let delay = std::time::Duration::from_millis(900);
    let responder = {
        let arrivals = arrivals.clone();
        move |_req: &Request| {
            arrivals.record();
            ResponseTemplate::new(200)
                .set_body_json(json!({"ok": true}))
                .set_delay(delay)
        }
    };
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(responder)
        .mount(&server)
        .await;
    let uri = server.uri();

    let fail_uri = uri.clone();
    std::thread::spawn(move || {
        let mut c = WebullClient::new(test_creds()).with_base_url(fail_uri);
        for i in 0..5 {
            let (status, _) = c.get("/fail", &[("i".into(), i.to_string())]).unwrap();
            assert_eq!(status, 500);
        }
    })
    .join()
    .unwrap_or_else(|_| panic!("failure thread"));

    let started = std::time::Instant::now();
    std::thread::spawn(move || {
        let mut threads = Vec::new();
        for i in 0..3 {
            let uri = uri.clone();
            threads.push(std::thread::spawn(move || {
                let mut c = WebullClient::new(test_creds()).with_base_url(uri);
                let (status, _) = c.get("/slow", &[("i".into(), i.to_string())]).unwrap();
                assert_eq!(status, 200);
            }));
        }
        for t in threads {
            t.join().unwrap();
        }
    })
    .join()
    .expect("driver thread");
    let elapsed = started.elapsed();

    assert_eq!(arrivals.sorted().len(), 3);
    assert_eq!(
        arrivals.max_in_flight(delay),
        3,
        "permits leaked: failures narrowed the lanes"
    );
    assert!(
        elapsed < delay * 3,
        "three calls did not overlap: {elapsed:?}"
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
