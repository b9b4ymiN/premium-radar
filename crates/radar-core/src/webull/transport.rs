//! Real HTTP transport for Webull — the single place where requests are signed,
//! paced, cached and retried. Enabled behind the `client` feature (reqwest +
//! uuid).
//!
//! Every [`WebullClient`] aimed at one origin shares a process-wide coordinator
//! (pacing + response cache), and on the production host also a cross-process
//! pacing file, so the scanner, the Home/Portfolio commands and the headless
//! recorder spend ONE quota instead of one each.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::rest::{Credentials, FetchError, HttpFetch, SnapCache, Throttle, is_retriable_response};
use super::sign::{MIN_INTERVAL_SEC, SignatureScheme};

const RETRIES: u32 = 2;
const RETRY_DELAY_SEC: f64 = 1.2;
const BASE_URL: &str = "https://api.webull.co.th";

/// Documented Thailand ceilings per endpoint (calls, window), with one call of
/// margin. The Data API pool is 300/60 s; account, order and instrument
/// endpoints are far tighter and a refresh button can reach them.
fn endpoint_budget(path: &str) -> (usize, Duration) {
    let ceiling = |max: usize, secs: u64| (max - 1, Duration::from_secs(secs));
    if path.starts_with("/trading/assets/") || path.ends_with("/accounts/list") {
        ceiling(10, 30)
    } else if path.starts_with("/trading/orders/") {
        ceiling(40, 2)
    } else if path.starts_with("/trading/instruments/") {
        ceiling(60, 60)
    } else {
        ceiling(300, 60)
    }
}

/// How long `path` must still wait so that at most `max` sends fall inside the
/// sliding `window`. Expired timestamps are dropped in place.
fn window_wait(
    stamps: &mut VecDeque<Instant>,
    max: usize,
    window: Duration,
    now: Instant,
) -> Option<Duration> {
    while stamps
        .front()
        .is_some_and(|sent| now.saturating_duration_since(*sent) >= window)
    {
        stamps.pop_front();
    }
    (stamps.len() >= max).then(|| {
        stamps.front().map_or(Duration::ZERO, |sent| {
            (*sent + window).saturating_duration_since(now)
        })
    })
}

/// Pacing + response cache shared by every client of one origin.
struct Coordinator {
    throttle: Throttle,
    cache: SnapCache,
    /// Send timestamps per endpoint path, oldest first.
    sent: HashMap<String, VecDeque<Instant>>,
}

impl Coordinator {
    fn new() -> Self {
        Self {
            throttle: Throttle::from_constants(),
            cache: SnapCache::from_constants(),
            sent: HashMap::new(),
        }
    }

    /// Block until `path` may be sent: at least `MIN_INTERVAL_SEC` after the
    /// previous request of this origin finished, and inside the endpoint's
    /// documented ceiling.
    fn wait_turn(&mut self, path: &str) {
        let wait = self.throttle.wait_duration(self.throttle.last());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
        let (max, window) = endpoint_budget(path);
        let stamps = self.sent.entry(path.to_owned()).or_default();
        while let Some(wait) = window_wait(stamps, max, window, Instant::now()) {
            std::thread::sleep(wait);
        }
    }

    fn record(&mut self, path: &str) {
        self.throttle.mark();
        self.sent
            .entry(path.to_owned())
            .or_default()
            .push_back(Instant::now());
    }
}

static COORDINATORS: LazyLock<Mutex<HashMap<String, Arc<Mutex<Coordinator>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The cache/pacing key a base URL maps to (scheme + host + port).
fn origin_of(base_url: &str) -> String {
    reqwest::Url::parse(base_url)
        .map(|url| url.origin().ascii_serialization())
        .unwrap_or_else(|_| base_url.trim_end_matches('/').to_owned())
}

fn coordinator(base_url: &str) -> Arc<Mutex<Coordinator>> {
    let origin = origin_of(base_url);
    let mut all = COORDINATORS.lock().unwrap_or_else(|e| e.into_inner());
    all.entry(origin)
        .or_insert_with(|| Arc::new(Mutex::new(Coordinator::new())))
        .clone()
}

/// Drop the shared response cache of `base_url`'s origin (credential change):
/// answers fetched with the previous keys must never be served again. Only
/// the cache is emptied, in place: the coordinator itself (pacing windows,
/// throttle) stays the one every client of the origin shares, including a
/// scan that still holds it.
pub fn clear_shared_cache(base_url: &str) {
    let existing = COORDINATORS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&origin_of(base_url))
        .cloned();
    if let Some(coord) = existing {
        coord.lock().unwrap_or_else(|e| e.into_inner()).cache = SnapCache::from_constants();
    }
}

/// Same directory the Tauri shell keeps `radar_settings.json` in; `None` where
/// the platform has no such directory (cross-process pacing is then skipped).
fn app_data_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Application Support/premium-radar"));
    }
    std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("premium-radar"))
}

fn production_gate_path(base_url: &str) -> Option<PathBuf> {
    let url = reqwest::Url::parse(base_url).ok()?;
    let production = reqwest::Url::parse(BASE_URL).ok()?;
    if url.origin() != production.origin() {
        return None;
    }
    app_data_dir().map(|dir| dir.join("webull-rest-pacing.lock"))
}

/// Coordinate request starts across processes. The file holds only the epoch
/// millis of the last reserved start; the returned guard keeps the OS lock
/// until the request has been sent, so another process waits instead of
/// overlapping.
fn reserve_production_slot(path: &Path) -> io::Result<File> {
    fs::create_dir_all(path.parent().expect("gate file has a parent"))?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.lock()?;
    let mut previous = [0; 8];
    let interval = Duration::from_secs_f64(MIN_INTERVAL_SEC);
    match file.read_exact(&mut previous) {
        Ok(()) => {
            let last = Duration::from_millis(u64::from_le_bytes(previous));
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            let elapsed = now.saturating_sub(last);
            std::thread::sleep(interval.saturating_sub(elapsed));
        }
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            // A fresh file or an interrupted write uses the conservative interval.
            std::thread::sleep(interval);
        }
        Err(error) => return Err(error),
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let timestamp = u64::try_from(now.as_millis()).unwrap_or(u64::MAX);
    file.rewind()?;
    file.write_all(&timestamp.to_le_bytes())?;
    Ok(file)
}

/// Full Webull REST client: shared coordinator (throttle + cache), signing per
/// [`SignatureScheme`], and rate-limited retry (2 retries × 1.2 s).
pub struct WebullClient {
    creds: Credentials,
    http: reqwest::blocking::Client,
    coordinator: Arc<Mutex<Coordinator>>,
    base_url: String,
    production_gate: Option<PathBuf>,
    scheme: SignatureScheme,
    /// Set by long-running workers: once raised, queued requests are dropped.
    cancel: Option<Arc<AtomicBool>>,
    /// Off for probes and tests that must observe the wire, not a cached copy.
    use_cache: bool,
}

/// A blocking client that never follows redirects. reqwest's default policy
/// replays custom headers (app key, access token, signature) on whatever host
/// a 3xx `Location` names — the broker needs no redirects, so a 3xx must
/// surface as-is and hit the existing non-200 handling instead.
fn http_client() -> reqwest::blocking::Client {
    let build = || {
        reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()
    };
    // A default client would follow redirects again, so the fallback retries
    // the same policy before ever settling for the bare default.
    build().unwrap_or_else(|_| build().unwrap_or_default())
}

impl WebullClient {
    pub fn new(creds: Credentials) -> Self {
        Self {
            creds,
            http: http_client(),
            coordinator: coordinator(BASE_URL),
            base_url: BASE_URL.to_string(),
            production_gate: production_gate_path(BASE_URL),
            scheme: SignatureScheme::default(),
            cancel: None,
            use_cache: true,
        }
    }

    /// Override the base URL (wiremock tests, UAT environment).
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self.coordinator = coordinator(&self.base_url);
        self.production_gate = production_gate_path(&self.base_url);
        self
    }

    pub fn with_signature_scheme(mut self, scheme: SignatureScheme) -> Self {
        self.scheme = scheme;
        self
    }

    pub fn signature_scheme(&self) -> SignatureScheme {
        self.scheme
    }

    pub fn with_cancel_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.cancel = Some(flag);
        self
    }

    /// Neither read nor populate the shared response cache.
    pub fn without_cache(mut self) -> Self {
        self.use_cache = false;
        self
    }

    fn is_cancelled(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    }

    fn reserve_shared_slot(&self) -> Result<Option<File>, FetchError> {
        self.production_gate
            .as_deref()
            .map(reserve_production_slot)
            .transpose()
            .map_err(|e| FetchError::Network(format!("Webull request pacing unavailable: {e}")))
    }

    /// Attach the signing headers. The app secret only ever appears here as the
    /// HMAC key: the Thailand host accepts both schemes without an
    /// `x-app-secret` header (live-verified 2026-11-23), so it is never sent.
    fn authorize(
        &self,
        req: reqwest::blocking::RequestBuilder,
        path: &str,
        params: &[(String, String)],
        body: &str,
    ) -> reqwest::blocking::RequestBuilder {
        let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let signature = self.scheme.sign(
            path,
            &host_from_base_url(&self.base_url),
            params,
            &ts,
            &nonce,
            &self.creds.app_key,
            &self.creds.app_secret,
            body,
        );
        let mut req = req
            .header("Accept", "application/json")
            .header("x-app-key", &self.creds.app_key)
            .header("x-timestamp", ts)
            .header("x-signature-version", "1.0")
            .header("x-signature-algorithm", self.scheme.algorithm())
            .header("x-signature-nonce", nonce)
            .header("x-version", self.scheme.x_version())
            .header("x-signature", signature);
        if let Some(token) = &self.creds.access_token {
            req = req.header("x-access-token", token);
        }
        req
    }

    /// One signed, paced request while the caller holds the coordinator lock.
    fn send_locked(
        &self,
        coord: &mut Coordinator,
        path: &str,
        params: &[(String, String)],
        body: Option<&str>,
    ) -> Result<(u16, Value), FetchError> {
        if self.is_cancelled() {
            return Err(FetchError::Cancelled);
        }
        coord.wait_turn(path);
        let process_slot = self.reserve_shared_slot()?;
        let url = format!("{}{}", self.base_url, path);
        let req = if let Some(body) = body {
            self.authorize(self.http.post(&url), path, params, body)
                .header("Content-Type", "application/json")
                .body(body.to_string())
        } else {
            let req = self.authorize(self.http.get(&url), path, params, "");
            if params.is_empty() {
                req
            } else {
                req.query(params)
            }
        };
        let outcome = req.send();
        coord.record(path);
        drop(process_slot);
        let response = outcome.map_err(|e| FetchError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        let text = response
            .text()
            .map_err(|e| FetchError::Network(e.to_string()))?;
        // A non-JSON body (gateway HTML, empty 204) is kept as clipped text so
        // error classification and messages still have something to read.
        let body = serde_json::from_str(&text)
            .unwrap_or_else(|_| Value::String(text.chars().take(300).collect()));
        Ok((status, body))
    }

    /// GET with rate-limited + gateway-error retry. 5xx (502/503/504) happens
    /// on slow environments (UAT) under back-to-back pagination — a short wait
    /// usually clears it, so those get the same retry budget as rate limits.
    fn do_get_locked(
        &self,
        coord: &mut Coordinator,
        path: &str,
        params: &[(String, String)],
    ) -> Result<(u16, Value), FetchError> {
        let mut retries = RETRIES;
        loop {
            let (status, body) = self.send_locked(coord, path, params, None)?;
            let retriable = is_retriable_response(status, &body);
            if retriable && retries > 0 {
                std::thread::sleep(Duration::from_secs_f64(RETRY_DELAY_SEC));
                retries -= 1;
                continue;
            }
            return Ok((status, body));
        }
    }

    /// GET behind the shared cache. The coordinator lock spans both the cache
    /// check and the request, so identical calls made together — even from
    /// different clients — collapse into one request.
    fn get_shared(
        &mut self,
        path: &str,
        params: &[(String, String)],
    ) -> Result<(u16, Value), FetchError> {
        if self.is_cancelled() {
            return Err(FetchError::Cancelled);
        }
        let mut coord = self.coordinator.lock().unwrap_or_else(|e| e.into_inner());
        if !self.use_cache {
            return self.do_get_locked(&mut coord, path, params);
        }
        let key = SnapCache::key(path, params);
        if let Some(hit) = coord.cache.get(&key, Instant::now()) {
            return Ok(hit.clone());
        }
        let response = self.do_get_locked(&mut coord, path, params)?;
        if response.0 == 200 {
            coord.cache.put(key, response.clone());
        }
        Ok(response)
    }

    /// One signed POST with a JSON body (its digest enters the canonical
    /// string). Never cached: token, subscribe and bars calls are side-effectful
    /// or request-specific. No retry — a POST is not idempotent by contract.
    pub fn post_json(&mut self, path: &str, body: &str) -> Result<(u16, Value), FetchError> {
        if self.is_cancelled() {
            return Err(FetchError::Cancelled);
        }
        let mut coord = self.coordinator.lock().unwrap_or_else(|e| e.into_inner());
        self.send_locked(&mut coord, path, &[], Some(body))
    }
}

impl HttpFetch for WebullClient {
    fn post_json(&mut self, path: &str, body: &str) -> Result<(u16, Value), FetchError> {
        WebullClient::post_json(self, path, body)
    }

    fn get(&mut self, path: &str, params: &[(String, String)]) -> Result<(u16, Value), FetchError> {
        self.get_shared(path, params)
    }
}

/// The host actually being called (the signature embeds it — using the
/// production host while `base_url` points elsewhere yields "x-signature is
/// invalid" from the broker).
pub fn host_from_base_url(base_url: &str) -> String {
    let s = base_url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    s.split('/')
        .next()
        .unwrap_or(s)
        .split(':')
        .next()
        .unwrap_or(s)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_budgets_sit_one_call_below_the_documented_ceiling() {
        let budget = |path: &str| {
            let (max, window) = endpoint_budget(path);
            (max, window.as_secs())
        };
        assert_eq!(budget("/trading/accounts/list"), (9, 30));
        assert_eq!(budget("/trading/assets/balances/get"), (9, 30));
        assert_eq!(budget("/trading/assets/positions/list"), (9, 30));
        assert_eq!(budget("/trading/orders/open-orders/list"), (39, 2));
        assert_eq!(
            budget("/trading/instruments/stocks/profiles/list"),
            (59, 60)
        );
        // everything else sits in the Data API pool, legacy /openapi paths included
        assert_eq!(budget("/openapi/market-data/stock/snapshot"), (299, 60));
        assert_eq!(budget("/market-data/stocks/bars/list"), (299, 60));
        assert_eq!(
            budget("/market-data/fundamentals/company-profiles/get"),
            (299, 60)
        );
    }

    #[test]
    fn window_wait_throttles_only_when_the_ceiling_is_reached() {
        let now = Instant::now();
        let window = Duration::from_secs(30);
        let mut stamps: VecDeque<Instant> = (0..9).map(|_| now - Duration::from_secs(5)).collect();
        // 9 sends inside the window = ceiling reached → wait until the oldest leaves
        let wait = window_wait(&mut stamps, 9, window, now).expect("must wait");
        assert_eq!(wait, Duration::from_secs(25));
        assert_eq!(stamps.len(), 9);
        // 8 sends → free to go
        stamps.pop_back();
        assert_eq!(window_wait(&mut stamps, 9, window, now), None);
        // timestamps at or past the window edge are dropped before counting
        let mut old: VecDeque<Instant> = [now - window, now - Duration::from_secs(31)].into();
        assert_eq!(window_wait(&mut old, 9, window, now), None);
        assert!(old.is_empty());
    }

    #[test]
    fn default_client_uses_the_official_scheme() {
        let client = WebullClient::new(Credentials {
            app_key: "K".into(),
            app_secret: "S".into(),
            access_token: None,
        });
        assert_eq!(client.signature_scheme(), SignatureScheme::V3);
        assert_eq!(
            client
                .with_signature_scheme(SignatureScheme::V2)
                .signature_scheme(),
            SignatureScheme::V2
        );
    }

    #[test]
    fn clear_shared_cache_empties_the_cache_but_keeps_the_coordinator() {
        let url = "https://cache-clear-test.local";
        let shared = coordinator(url);
        let key = SnapCache::key("/p", &[]);
        shared
            .lock()
            .unwrap()
            .cache
            .put(key.clone(), (200, serde_json::json!({"old": true})));
        clear_shared_cache(url);
        assert!(
            Arc::ptr_eq(&shared, &coordinator(url)),
            "pacing must stay on the one shared coordinator"
        );
        assert!(
            shared
                .lock()
                .unwrap()
                .cache
                .get(&key, Instant::now())
                .is_none(),
            "the previous credentials' answers are gone"
        );
        clear_shared_cache("https://never-seen.local"); // unknown origin: no panic
    }

    #[test]
    fn production_gate_worker() {
        let Some(path) = std::env::var_os("RADAR_TEST_GATE_FILE") else {
            return;
        };
        let _slot = reserve_production_slot(Path::new(&path)).unwrap();
    }

    #[test]
    fn production_gate_serializes_separate_processes() {
        let path = std::env::temp_dir().join(format!("radar-gate-{}.lock", uuid::Uuid::new_v4()));
        let parent_slot = reserve_production_slot(&path).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "webull::transport::tests::production_gate_worker",
            ])
            .env("RADAR_TEST_GATE_FILE", &path)
            .spawn()
            .unwrap();
        // Outwait the timestamp cooldown so only the OS lock can keep the child blocked.
        std::thread::sleep(Duration::from_secs_f64(MIN_INTERVAL_SEC + 0.25));
        let blocked = child.try_wait().unwrap().is_none();
        drop(parent_slot);
        let status = child.wait().unwrap();
        fs::remove_file(path).unwrap();
        assert!(blocked);
        assert!(status.success());
    }
}
