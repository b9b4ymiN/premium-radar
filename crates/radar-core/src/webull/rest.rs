//! Webull REST layer — request policy (throttle / cache / retry), response
//! extraction, credentials handling and a network-agnostic fetcher trait so
//! the whole pipeline is testable without HTTP. Ported from the Python
//! reference (`webull_get` / `_do_call` / `extract_list`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::Value;

// ---------- endpoints & limits (mirrors the Python constants) ----------

pub const PATH_STOCK_SNAPSHOT: &str = "/openapi/market-data/stock/snapshot";
pub const PATH_OPTION_SNAPSHOT: &str = "/openapi/market-data/option/snapshot";
pub const PATH_OPTION_BARS: &str = "/openapi/market-data/option/bars";
pub const PATH_OPTION_CONTRACTS: &str = "/openapi/instrument/option/contracts";

/// Shared REST throttle (1 call / 1.1s across all tasks).
pub const MIN_INTERVAL_SEC: f64 = 1.1;
/// Snapshot cache TTL.
pub const CACHE_TTL_SEC: f64 = 3.0;
/// Contracts analyzed through REST snapshots (no WebSocket limit).
pub const MAX_ANALYZE: usize = 480;
/// WebSocket subscribe limit (Webull max 100 incl. the stock).
pub const MAX_CONTRACTS: usize = 96;
/// Snapshot batch size (`OI_BATCH`).
pub const OI_BATCH: usize = 20;
/// Contract-discovery windows in days out (`WINDOWS`).
pub const WINDOWS: [(i64, i64); 7] = [
    (0, 7),
    (8, 14),
    (15, 30),
    (31, 60),
    (61, 120),
    (121, 365),
    (366, 1000),
];
/// Strike band around spot (±30%).
pub const STRIKE_BAND: f64 = 0.30;
/// Distinct expiries kept per symbol.
pub const MAX_EXPIRIES: usize = 12;
/// OTM strikes kept per side per expiry.
pub const STRIKES_EACH_SIDE: usize = 20;

// ---------- credentials ----------

/// App credentials. Key material lives in the OS keyring / env at runtime —
/// never in source, logs, or error messages (use [`Credentials::masked`]).
/// `Debug` is implemented manually so the secret can never leak via `{:?}`.
#[derive(Clone)]
pub struct Credentials {
    pub app_key: String,
    pub app_secret: String,
    pub access_token: Option<String>,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Credentials(app_key={}, secret=***{}***, access_token={})",
            self.app_key,
            self.app_secret.len(),
            self.access_token.is_some()
        )
    }
}

impl Credentials {
    /// Dev/test path: `WEBULL_APP_KEY` / `WEBULL_APP_SECRET` / optional
    /// `WEBULL_ACCESS_TOKEN`. Never logged with values.
    pub fn from_env() -> Option<Self> {
        let key = std::env::var("WEBULL_APP_KEY")
            .ok()
            .filter(|s| !s.is_empty())?;
        let secret = std::env::var("WEBULL_APP_SECRET")
            .ok()
            .filter(|s| !s.is_empty())?;
        let access_token = std::env::var("WEBULL_ACCESS_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());
        Some(Self {
            app_key: key,
            app_secret: secret,
            access_token,
        })
    }

    /// Log-safe representation.
    pub fn masked(&self) -> String {
        format!(
            "Credentials(app_key={}, secret=***{}***)",
            self.app_key,
            self.app_secret.len()
        )
    }
}

// ---------- response helpers (mirror `extract_list` / `to_float`) ----------

/// Pull the payload list out of a Webull response: a bare list, `{"data": []}`
/// or `{"data": {"data": []}}`.
pub fn extract_list(d: &Value) -> Vec<Value> {
    match d {
        Value::Array(a) => a.clone(),
        Value::Object(o) => match o.get("data") {
            Some(Value::Array(a)) => a.clone(),
            Some(Value::Object(inner)) => match inner.get("data") {
                Some(Value::Array(a)) => a.clone(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// `to_float` with Python semantics: None/"null"/""/bad text -> default.
pub fn to_f64(v: &Value, default: f64) -> f64 {
    match v {
        Value::Null => default,
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::Number(n) => n.as_f64().unwrap_or(default),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() || t.eq_ignore_ascii_case("none") || t.eq_ignore_ascii_case("null") {
                default
            } else {
                t.parse::<f64>().unwrap_or(default)
            }
        }
        _ => default,
    }
}

/// Next-page cursor from a contracts response (`pagination_key` at either
/// nesting level); dict-shaped keys terminate paging like Python's check.
pub fn pagination_key(d: &Value) -> Option<String> {
    let level = |o: &serde_json::Map<String, Value>| -> Option<String> {
        match o.get("pagination_key") {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            // dict-shaped key -> hard stop (Python `isinstance(key, dict)`)
            Some(Value::Object(_)) => Some(String::new()),
            _ => None,
        }
    };
    if let Value::Object(o) = d {
        if let Some(k) = level(o) {
            return (!k.is_empty()).then_some(k);
        }
        // falsy ""/missing at top level falls through to data level (Python truthiness)
        if let Some(k) = o.get("data").and_then(Value::as_object).and_then(level) {
            return (!k.is_empty()).then_some(k);
        }
    }
    None
}

/// Python rate-limit classification: HTTP 429 or a body message containing
/// "rate".
pub fn is_rate_limited(status: u16, body: &Value) -> bool {
    if status == 429 {
        return true;
    }
    body.get("message")
        .and_then(Value::as_str)
        .is_some_and(|m| m.to_ascii_lowercase().contains("rate"))
}

// ---------- throttle (pure time math) ----------

/// Shared-channel throttle: every call must be at least `min_interval` after
/// the previous one. The pure math is testable without sleeping.
#[derive(Debug)]
pub struct Throttle {
    pub min_interval: Duration,
    last: Option<Instant>,
}

impl Throttle {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            last: None,
        }
    }

    pub fn from_constants() -> Self {
        Self::new(Duration::from_secs_f64(MIN_INTERVAL_SEC))
    }

    /// How long to wait before the next call, given the recorded last call.
    pub fn wait_duration(&self, last: Option<Instant>) -> Duration {
        match last {
            None => Duration::ZERO,
            Some(t) => self
                .min_interval
                .checked_sub(t.elapsed())
                .unwrap_or(Duration::ZERO),
        }
    }

    /// Record a call as happening *now* (called after the wait).
    pub fn mark(&mut self) {
        self.last = Some(Instant::now());
    }

    pub fn last(&self) -> Option<Instant> {
        self.last
    }
}

// ---------- cache ----------

/// TTL response cache keyed by `path + canonical query` (like `webull_get`).
#[derive(Debug)]
pub struct SnapCache {
    pub ttl: Duration,
    map: HashMap<String, (Instant, (u16, Value))>,
}

impl SnapCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            map: HashMap::new(),
        }
    }

    pub fn from_constants() -> Self {
        Self::new(Duration::from_secs_f64(CACHE_TTL_SEC))
    }

    pub fn get(&self, key: &str, now: Instant) -> Option<&(u16, Value)> {
        self.map
            .get(key)
            .filter(|(t, _)| now.duration_since(*t) < self.ttl)
            .map(|(_, v)| v)
    }

    pub fn put(&mut self, key: String, value: (u16, Value)) {
        self.map.insert(key, (Instant::now(), value));
    }

    /// Cache key identical in spirit to Python's `f"{path}?{json.dumps(params, sort_keys=True)}"`.
    pub fn key(path: &str, params: &[(String, String)]) -> String {
        let mut sorted: Vec<&(String, String)> = params.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let body = sorted
            .iter()
            .map(|(k, v)| format!("{k:?}:{v:?}"))
            .collect::<Vec<_>>()
            .join(",");
        format!("{path}?{{{body}}}")
    }
}

// ---------- fetcher abstraction ----------

#[derive(Debug)]
pub enum FetchError {
    Network(String),
    NonZero(u16, String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Network(e) => write!(f, "network error: {e}"),
            FetchError::NonZero(status, body) => {
                // clip by chars — never slice inside a multi-byte char (Thai endpoints)
                let clipped: String = body.chars().take(300).collect();
                write!(f, "Webull {status}: {clipped}")
            }
        }
    }
}
impl std::error::Error for FetchError {}

/// Blocking fetcher — mirrors `_do_call`: GET `path?params`, returns
/// (status, json body). Implementations provide transport (mock / reqwest).
pub trait HttpFetch {
    fn get(&mut self, path: &str, params: &[(String, String)]) -> Result<(u16, Value), FetchError>;
}

/// `http_get_json`: fetch + raise on non-200.
pub fn get_json<F: HttpFetch>(
    f: &mut F,
    path: &str,
    params: &[(String, String)],
) -> Result<Value, FetchError> {
    let (status, body) = f.get(path, params)?;
    if status != 200 {
        return Err(FetchError::NonZero(status, body.to_string()));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_list_all_three_shapes() {
        let direct = json!([{"a": 1}]);
        assert_eq!(extract_list(&direct).len(), 1);
        let wrapped = json!({"data": [{"a": 1}, {"a": 2}]});
        assert_eq!(extract_list(&wrapped).len(), 2);
        let nested = json!({"data": {"data": [{"a": 3}]}});
        assert_eq!(extract_list(&nested).len(), 1);
        assert_eq!(extract_list(&json!({})).len(), 0);
        assert_eq!(extract_list(&json!("x")).len(), 0);
    }

    #[test]
    fn pagination_key_levels_and_stops() {
        assert_eq!(
            pagination_key(&json!({"pagination_key": "abc"})).as_deref(),
            Some("abc")
        );
        assert_eq!(
            pagination_key(&json!({"data": {"pagination_key": "p2"}})).as_deref(),
            Some("p2")
        );
        assert_eq!(pagination_key(&json!({"pagination_key": ""})), None);
        // dict-shaped key terminates paging (Python: `isinstance(key, dict)` -> break)
        assert_eq!(pagination_key(&json!({"pagination_key": {"x": 1}})), None);
        assert_eq!(pagination_key(&json!({})), None);
    }

    #[test]
    fn to_f64_python_semantics() {
        assert_eq!(to_f64(&json!(null), 0.0), 0.0);
        assert_eq!(to_f64(&json!(""), 7.0), 7.0);
        assert_eq!(to_f64(&json!("null"), 7.0), 7.0);
        assert_eq!(to_f64(&json!("None"), 7.0), 7.0);
        assert_eq!(to_f64(&json!("12.5"), 0.0), 12.5);
        assert_eq!(to_f64(&json!("junk"), 3.0), 3.0);
        assert_eq!(to_f64(&json!(9), 0.0), 9.0);
    }

    #[test]
    fn rate_limit_classification() {
        assert!(is_rate_limited(429, &json!({})));
        assert!(is_rate_limited(200, &json!({"message": "Rate limited"})));
        assert!(is_rate_limited(200, &json!({"message": "too many, RATE?"})));
        assert!(!is_rate_limited(200, &json!({"message": "ok"})));
        assert!(!is_rate_limited(500, &json!({})));
    }

    #[test]
    fn throttle_math() {
        let t = Throttle::new(Duration::from_secs_f64(1.1));
        assert_eq!(t.wait_duration(None), Duration::ZERO);
        // just-called -> full wait
        let now = Instant::now();
        let w = t.wait_duration(Some(now));
        assert!(w > Duration::from_secs_f64(1.0));
        // long past -> zero
        let old = Instant::now() - Duration::from_secs(5);
        assert_eq!(t.wait_duration(Some(old)), Duration::ZERO);
    }

    #[test]
    fn cache_key_order_insensitive_and_ttl() {
        let a = SnapCache::key("/p", &[("z".into(), "1".into()), ("a".into(), "2".into())]);
        let b = SnapCache::key("/p", &[("a".into(), "2".into()), ("z".into(), "1".into())]);
        assert_eq!(a, b);
        let mut c = SnapCache::new(Duration::from_secs(3));
        c.put("k".into(), (200, json!({"ok": true})));
        assert!(c.get("k", Instant::now()).is_some());
        // expired entry (simulated by ttl 0)
        let mut z = SnapCache::new(Duration::ZERO);
        z.put("k".into(), (200, json!(1)));
        assert!(z.get("k", Instant::now()).is_none());
    }

    #[test]
    fn credentials_never_leak_secret_in_debug() {
        let c = Credentials {
            app_key: "KEY".into(),
            app_secret: "SECRET".into(),
            access_token: None,
        };
        let m = c.masked();
        assert!(m.contains("***") && !m.contains("SECRET"));
        assert_eq!(c.app_key, "KEY");
    }
}
