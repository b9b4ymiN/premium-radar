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

use chrono::NaiveDate;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use radar_core::webull::discovery::{
    fetch_option_contracts, fetch_option_contracts_par, fetch_symbols_flow_par,
};
use radar_core::webull::rest::{
    Credentials, FetchError, HttpFetch, PATH_OPTION_CONTRACTS, PATH_STOCK_SNAPSHOT,
};
use radar_core::webull::sign::SignatureScheme;
use radar_core::webull::transport::{Priority, WebullClient};

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

// ---------- priority lane + parallel discovery ----------

/// Arrivals tagged with the caller's marker (the `i` query param), in
/// arrival order — server-side evidence of WHO started WHEN.
#[derive(Default)]
struct TaggedArrivals {
    seen: std::sync::Mutex<Vec<(String, std::time::Instant)>>,
}

impl TaggedArrivals {
    fn record(&self, tag: &str) {
        self.seen
            .lock()
            .unwrap()
            .push((tag.to_string(), std::time::Instant::now()));
    }

    fn count(&self) -> usize {
        self.seen.lock().unwrap().len()
    }

    fn order(&self) -> Vec<(String, std::time::Instant)> {
        self.seen.lock().unwrap().clone()
    }
}

/// The coordinator is saturated (3 slow in-flight Background requests, 2
/// Background waiters parked on the condvar); an Interactive request issued
/// LAST must still START before both waiters, and every start keeps the full
/// MIN_INTERVAL spacing. Background-only traffic still drains on its own —
/// that half is `concurrent_gets_overlap_round_trips_while_pacing_starts`
/// above (6 Background callers, no Interactive one, all complete).
#[tokio::test]
async fn interactive_request_starts_before_waiting_background_callers() {
    let server = MockServer::start().await;
    let arrivals = Arc::new(TaggedArrivals::default());
    let delay = std::time::Duration::from_millis(900);
    let responder = {
        let arrivals = arrivals.clone();
        move |req: &Request| {
            let tag = req
                .url
                .query_pairs()
                .find(|(k, _)| k == "i")
                .map(|(_, v)| v.to_string())
                .unwrap_or_default();
            arrivals.record(&tag);
            ResponseTemplate::new(200)
                .set_body_json(json!({"ok": true}))
                .set_delay(delay)
        }
    };
    Mock::given(method("GET"))
        .and(path("/lane"))
        .respond_with(responder)
        .mount(&server)
        .await;
    let uri = server.uri();

    // 5 Background callers race in together: 3 take the in-flight permits,
    // 2 park on the condvar.
    let barrier = Arc::new(std::sync::Barrier::new(5));
    let mut handles = Vec::new();
    for i in 0..5 {
        let uri = uri.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            let mut c = WebullClient::new(test_creds()).with_base_url(uri);
            let (status, _) = c.get("/lane", &[("i".into(), i.to_string())]).unwrap();
            assert_eq!(status, 200);
        }));
    }
    // wait until all three permits are genuinely on the wire, so the
    // Interactive request below can only be a WAITER, never in-flight
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while arrivals.count() < 3 {
        assert!(
            std::time::Instant::now() < deadline,
            "background callers never started"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // issued LAST: after the permits are exhausted and the waiters parked
    let fg_uri = uri.clone();
    handles.push(std::thread::spawn(move || {
        let mut c = WebullClient::new(test_creds())
            .with_base_url(fg_uri)
            .with_priority(Priority::Interactive);
        let (status, _) = c.get("/lane", &[("i".into(), "99".into())]).unwrap();
        assert_eq!(status, 200);
    }));
    for h in handles {
        h.join().expect("lane thread");
    }

    let order = arrivals.order();
    assert_eq!(order.len(), 6, "every caller completed");
    // arrivals 0..3 are the initial in-flight trio (they started before the
    // Interactive caller was even spawned); the Interactive caller must be
    // arrival #4 — ahead of BOTH parked Background waiters
    let fg = order
        .iter()
        .position(|(tag, _)| tag.as_str() == "99")
        .expect("interactive arrival recorded");
    assert_eq!(
        fg, 3,
        "interactive did not jump the waiting background queue: {order:?}"
    );
    // rate guarantees unchanged: starts stay a full MIN_INTERVAL apart even
    // across the priority handoff (20 ms arrival jitter budget)
    let min_gap = std::time::Duration::from_secs_f64(radar_core::webull::sign::MIN_INTERVAL_SEC)
        - std::time::Duration::from_millis(20);
    let mut starts: Vec<std::time::Instant> = order.iter().map(|(_, at)| *at).collect();
    starts.sort();
    for pair in starts.windows(2) {
        assert!(
            pair[1].duration_since(pair[0]) >= min_gap,
            "starts too close: {pair:?}"
        );
    }
}

/// One contracts row, in the shape the discovery scan consumes.
fn contract_row(id: i64, expiry: &str, symbol: &str) -> serde_json::Value {
    json!({"instrument_id": id, "expiration_date": expiry, "symbol": symbol})
}

/// Deterministic per-window contracts responder: window 0 pages twice (its
/// second page carries a PAST expiry), window 1 repeats instrument_id 102
/// from window 0. Keyed by `start_date`, so sequential and parallel runs that
/// issue identical windows get identical responses.
fn contracts_responder() -> impl Fn(&Request) -> ResponseTemplate + Send + Sync {
    move |req: &Request| {
        let params: Vec<(String, String)> = req
            .url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let get = |k: &str| {
            params
                .iter()
                .find(|(kk, _)| kk == k)
                .map(|(_, v)| v.clone())
        };
        let (rows, more) = match (get("start_date").as_deref(), get("pagination_key")) {
            (Some("2026-11-24"), None) => (
                vec![contract_row(101, "2026-11-26", "A261126P00095000")],
                true,
            ),
            (Some("2026-11-24"), Some(_)) => (
                vec![
                    // past expiry: the client-side filter must drop it
                    contract_row(100, "2026-11-20", "A261120P00095000"),
                    contract_row(102, "2026-11-28", "A261128C00105000"),
                ],
                false,
            ),
            (Some("2026-12-02"), _) => (
                vec![
                    // duplicate instrument id: first occurrence wins
                    contract_row(102, "2026-12-04", "A261204P00090000"),
                    contract_row(103, "2026-12-07", "A261207C00100000"),
                ],
                false,
            ),
            (Some("2026-12-09"), _) => (
                vec![
                    contract_row(104, "2026-12-11", "A261211P00095000"),
                    contract_row(105, "2026-12-18", "A261218C00105000"),
                ],
                false,
            ),
            (Some("2026-12-25"), _) => (
                vec![contract_row(106, "2027-01-15", "A270115P00095000")],
                false,
            ),
            (Some("2027-01-24"), _) => (
                vec![contract_row(107, "2027-02-19", "A270219P00095000")],
                false,
            ),
            (Some("2027-03-25"), _) => (
                vec![contract_row(108, "2027-06-18", "A270618P00095000")],
                false,
            ),
            _ => (
                vec![contract_row(109, "2028-06-16", "A280616P00095000")],
                false,
            ),
        };
        let mut body = json!({"data": rows});
        if more {
            body["pagination_key"] = json!("w0-page2");
        }
        ResponseTemplate::new(200).set_body_json(body)
    }
}

fn run_sequential(
    uri: String,
    today: NaiveDate,
    need: Option<usize>,
) -> (Vec<serde_json::Value>, Vec<String>) {
    std::thread::spawn(move || {
        let mut c = WebullClient::new(test_creds())
            .with_base_url(uri)
            .without_cache();
        let mut log = Vec::new();
        let mut sink = |m: String| log.push(m);
        let rows = fetch_option_contracts(
            &mut c,
            "A",
            None,
            None,
            None,
            60,
            need,
            today,
            None,
            Some(&mut sink),
        )
        .unwrap();
        (rows, log)
    })
    .join()
    .expect("sequential thread")
}

fn run_parallel(
    uri: String,
    today: NaiveDate,
    need: Option<usize>,
) -> (Vec<serde_json::Value>, Vec<String>) {
    std::thread::spawn(move || {
        let make = move || {
            Some(
                WebullClient::new(test_creds())
                    .with_base_url(uri.clone())
                    .without_cache(),
            )
        };
        let mut log = Vec::new();
        let mut sink = |m: String| log.push(m);
        let rows = fetch_option_contracts_par(
            &make,
            "A",
            None,
            None,
            None,
            60,
            need,
            today,
            None,
            Some(&mut sink),
        )
        .unwrap();
        (rows, log)
    })
    .join()
    .expect("parallel thread")
}

/// The parallel window scan returns EXACTLY the sequential scan's list (same
/// order) and the same progress messages. The dataset spans all 7 windows
/// (pagination, an overlapping instrument id, a past expiry); the early stop
/// (`need = 6` expiries) lands on window 3 — the middle of the SECOND wave —
/// so wave merging, mid-wave discarding and the stop rule are all exercised.
#[tokio::test]
async fn parallel_discovery_matches_the_sequential_scan() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(PATH_OPTION_CONTRACTS))
        .respond_with(contracts_responder())
        .mount(&server)
        .await;
    let today = NaiveDate::from_ymd_opt(2026, 11, 24).unwrap();
    // expiries per window: 2, 1, 2, 1, ... -> cumulative 2, 3, 5, 6: stop at
    // window 3, i.e. the second window of wave [2, 3, 4]
    let need = Some(6usize);
    let (seq_rows, seq_progress) = run_sequential(server.uri(), today, need);
    let (par_rows, par_progress) = run_parallel(server.uri(), today, need);
    assert_eq!(
        par_rows, seq_rows,
        "parallel merge must equal the sequential list"
    );
    assert_eq!(par_progress, seq_progress);
    // window 0 kept 101+102 (past 100 dropped), window 1 kept only 103 (the
    // repeated id 102 deduped), then 104, 105 from window 2 and 106 from
    // window 3 — where the early stop fired; window 4's rows are discarded
    let ids: Vec<i64> = seq_rows
        .iter()
        .map(|r| r["instrument_id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, vec![101, 102, 103, 104, 105, 106]);
    // the early stop really happened mid-flow (4 progress messages, not 7)
    assert_eq!(seq_progress.len(), 4);
    assert!(server.received_requests().await.unwrap().len() >= 4);
}

/// With a known spot the parallel flow never touches the stock snapshot
/// path — Phase 0's quote is reused instead of fetching the spot again.
#[tokio::test]
async fn known_spot_skips_the_stock_snapshot_call() {
    let server = MockServer::start().await;
    // only the contracts endpoint is mocked: a snapshot call would 404 the
    // whole flow, and the request log below asserts none even happened
    Mock::given(method("GET"))
        .and(path(PATH_OPTION_CONTRACTS))
        .respond_with(|_req: &Request| {
            ResponseTemplate::new(200).set_body_json(json!({"data": [
                {"instrument_id": 1, "expiration_date": "2026-12-18", "symbol": "A261218P00095000"},
                {"instrument_id": 2, "expiration_date": "2026-12-18", "symbol": "A261218C00105000"},
                {"instrument_id": 3, "expiration_date": "2027-01-15", "symbol": "A270115P00095000"},
                {"instrument_id": 4, "expiration_date": "2027-01-15", "symbol": "A270115C00105000"},
            ]}))
        })
        .mount(&server)
        .await;
    let uri = server.uri();
    let (symbols, spot) = std::thread::spawn(move || {
        let make = move || {
            Some(
                WebullClient::new(test_creds())
                    .with_base_url(uri.clone())
                    .without_cache(),
            )
        };
        fetch_symbols_flow_par(
            &make,
            "A",
            Some(100.0),
            NaiveDate::from_ymd_opt(2026, 11, 24).unwrap(),
            None,
            None,
        )
        .unwrap()
    })
    .join()
    .expect("flow thread");
    assert_eq!(spot, 100.0, "the known spot is the flow's spot");
    assert!(!symbols.is_empty());
    let reqs = server.received_requests().await.expect("captured requests");
    assert!(
        reqs.iter().all(|r| r.url.path() != PATH_STOCK_SNAPSHOT),
        "no stock snapshot call may happen with a known spot: {:?}",
        reqs.iter().map(|r| r.url.path()).collect::<Vec<_>>()
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
