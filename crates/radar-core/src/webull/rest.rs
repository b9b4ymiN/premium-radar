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
pub const PATH_AUTH_TOKEN_CREATE: &str = "/openapi/auth/token/create";
pub const PATH_AUTH_TOKEN_CHECK: &str = "/openapi/auth/token/check";
pub const PATH_STREAMING_SUBSCRIBE: &str = "/openapi/market-data/streaming/subscribe";
pub const PATH_STREAMING_UNSUBSCRIBE: &str = "/openapi/market-data/streaming/unsubscribe";

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

/// จำนวนแถวสูงสุดต่อหน้าของ contracts endpoint (P8.2 probe: ชน 1000
/// โดยไม่มี pagination_key กลับ — ใช้ตรวจ possibly_truncated ของ Lab)
pub const PAGE_CAP: usize = 1000;

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
        // allow base URL override for testing (default: https://api.webull.co.th)
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
        // never print the full app_key — first 8 chars identify, rest masked
        let head: String = self.app_key.chars().take(8).collect();
        format!(
            "Credentials(app_key={head}…, secret=***{}***)",
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

// ---------- typed-field readers (the broker sends numbers as strings) ----------

/// Typed number: missing, null, empty, non-numeric or non-finite → `None`.
/// A typed field is never `0` standing in for "unknown".
pub fn opt_f64(v: Option<&Value>) -> Option<f64> {
    let x = to_f64(v.unwrap_or(&Value::Null), f64::NAN);
    x.is_finite().then_some(x)
}

/// Typed integer (epoch milliseconds, fiscal year) from a number or numeric string.
pub fn opt_i64(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Typed string, trimmed; empty or blank-only text counts as missing.
pub fn opt_str(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
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

/// The phrases the broker's messages actually use for quota errors — a phrase
/// match, not a bare "rate" substring ("generate", "separate", "accurate"
/// must not read as quota errors).
fn message_mentions_quota(body: &Value) -> bool {
    body.get("message")
        .and_then(Value::as_str)
        .is_some_and(|m| {
            let m = m.to_ascii_lowercase();
            m.contains("rate limit") || m.contains("too many")
        })
}

/// Python rate-limit classification: HTTP 429 or a body message mentioning
/// rate limiting ("rate limit" / "too many").
pub fn is_rate_limited(status: u16, body: &Value) -> bool {
    status == 429 || message_mentions_quota(body)
}

/// The one retry predicate, shared by the transport and consistent with what
/// [`FetchError::kind`] calls rate limiting: true for HTTP 429, the broker's
/// rate-limit `error_code` (RATE / TOO_MANY — the broker can send it on a
/// non-429 status), a message mentioning "rate limit" / "too many", or a
/// gateway failure (502–504).
pub fn is_retriable_response(status: u16, body: &Value) -> bool {
    if status == 429 || matches!(status, 502..=504) {
        return true;
    }
    let code = body
        .get("error_code")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_uppercase();
    code.contains("RATE") || code.contains("TOO_MANY") || message_mentions_quota(body)
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
    /// Dropped before it was sent because the caller went stale (symbol
    /// changed, scan stopped) — an obsolete request, not an API failure.
    Cancelled,
    /// HTTP 200 with a body the reader does not recognise. Reported, never
    /// shown as "no data": an empty list would look like the truth.
    Unexpected(String),
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
            FetchError::Cancelled => write!(f, "request cancelled (superseded)"),
            FetchError::Unexpected(what) => write!(f, "unexpected response: {what}"),
        }
    }
}
impl std::error::Error for FetchError {}

// ---------- failure classification ----------

/// The broker's machine-readable `error_code` from an error body, if present.
pub fn error_code(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    value
        .get("error_code")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// What kind of failure a call hit — decides whether to retry, what to tell
/// the user and what they can do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// No HTTP response at all (DNS, TLS, timeout).
    Network,
    /// Dropped locally because the caller went stale.
    Cancelled,
    /// Quota exhausted; a later identical call can succeed.
    RateLimited,
    /// Signature or app key rejected.
    Auth,
    /// The access token is missing, invalid or expired.
    Token,
    /// Valid credentials, but the app key lacks the subscription.
    Permission,
    /// The broker rejected the request itself (bad parameter, business rule).
    InvalidRequest,
    /// No such endpoint on this host.
    NotFound,
    /// The broker's side failed.
    Server,
    /// 200 with a body the reader does not recognise.
    Schema,
    Other,
}

/// Classification order: (1) the broker's machine-readable `error_code`
/// decides first, in the priority RateLimited → Token → Auth → Permission
/// codes; (2) then the specific HTTP statuses (429, 401, 403); (3) then the
/// body-text fallbacks; (4) then the generic buckets (404, other 4xx, 5xx).
/// A 403 for a missing subscription
/// and a 401 for a bad token need different next steps, and neither the
/// status nor the message alone can tell them apart (observed codes:
/// 2026-11-23).
fn classify(status: u16, body: &str) -> FailureKind {
    let code = error_code(body).unwrap_or_default().to_ascii_uppercase();
    let text = body.to_ascii_lowercase();
    // (1) the broker's own code outranks the HTTP status (an absent code
    // leaves `code` empty, so every contains() is false)
    if code.contains("RATE") || code.contains("TOO_MANY") {
        return FailureKind::RateLimited;
    }
    if code.contains("TOKEN") || text.contains("x-access-token") {
        return FailureKind::Token;
    }
    if code.contains("UNAUTHORIZED")
        || code.contains("SIGNATURE")
        || code.contains("APP_KEY")
        || code.contains("APPKEY")
        || code.contains("SECRET")
    {
        return FailureKind::Auth;
    }
    if code.contains("SUBSCRIB")
        || code.contains("PERMISSION")
        || code.contains("ENTITLE")
        || code.contains("FORBIDDEN")
    {
        return FailureKind::Permission;
    }
    // (2) HTTP status fallbacks
    match status {
        429 => FailureKind::RateLimited,
        401 => FailureKind::Auth,
        403 => FailureKind::Permission,
        // (3) body-text fallbacks (messages without a machine-readable code)
        // outrank the generic status buckets: a 400 "signature mismatch"
        // means fix the key, not the request
        _ if text.contains("signature") || text.contains("correct environment") => {
            FailureKind::Auth
        }
        404 => FailureKind::NotFound,
        400..=499 => FailureKind::InvalidRequest,
        500..=599 => FailureKind::Server,
        _ => FailureKind::Other,
    }
}

impl FetchError {
    pub fn kind(&self) -> FailureKind {
        match self {
            FetchError::Network(_) => FailureKind::Network,
            FetchError::Cancelled => FailureKind::Cancelled,
            FetchError::NonZero(status, body) => classify(*status, body),
            FetchError::Unexpected(_) => FailureKind::Schema,
        }
    }

    /// Worth sending again unchanged.
    pub fn is_retriable(&self) -> bool {
        matches!(
            self.kind(),
            FailureKind::Network | FailureKind::RateLimited | FailureKind::Server
        )
    }

    /// One line for logs and reports: the status plus the broker's code when
    /// it sent one — never the raw body.
    pub fn summary(&self) -> String {
        match self {
            FetchError::Network(e) => format!("network error: {e}"),
            FetchError::Cancelled => "cancelled".into(),
            FetchError::NonZero(status, body) => match error_code(body) {
                Some(code) => format!("HTTP {status} · {code}"),
                None => format!("HTTP {status}"),
            },
            // the reader's own words about the shape — never the body itself
            FetchError::Unexpected(what) => format!("HTTP 200 · SCHEMA_MISMATCH ({what})"),
        }
    }

    /// One line in Thai for the UI, ending with [`Self::summary`].
    pub fn user_message(&self) -> String {
        let reason = match self.kind() {
            FailureKind::Network => "เชื่อมต่อ Webull ไม่ได้ (เครือข่าย) — ลองใหม่อีกครั้ง",
            FailureKind::Cancelled => "ยกเลิกคำขอเพราะข้อมูลล้าสมัย",
            FailureKind::RateLimited => "เรียก API ถี่เกินไป — รอสักครู่แล้วลองใหม่",
            FailureKind::Auth => "App Key/Secret ไม่ผ่านการยืนยัน — ตรวจค่าในหน้า Setup",
            FailureKind::Token => {
                "Access Token ไม่ถูกต้องหรือหมดอายุ — สร้างใหม่ด้วยปุ่ม \"สร้าง Access Token ใหม่\" ในหน้า Setup"
            }
            FailureKind::Permission => {
                "App key นี้ยังไม่มีสิทธิ์ข้อมูลชุดนี้ (ต้องสมัคร OpenAPI market data เพิ่มที่ Webull)"
            }
            FailureKind::InvalidRequest => {
                "Webull ไม่รับคำขอนี้ (พารามิเตอร์หรือเงื่อนไขไม่ตรง) — ไม่มีข้อมูลชุดนี้ให้แสดง"
            }
            FailureKind::NotFound => "ไม่มี endpoint นี้บน Webull host ที่ตั้งไว้",
            FailureKind::Server => "Webull ตอบกลับผิดพลาดชั่วคราว — ลองใหม่อีกครั้ง",
            FailureKind::Schema => {
                "Webull ตอบกลับมา 200 แต่รูปแบบข้อมูลไม่ตรงกับที่แอปอ่านอยู่ — ไม่แสดงข้อมูลชุดนี้ เพื่อไม่ให้เข้าใจผิดว่าไม่มีข้อมูล"
            }
            FailureKind::Other => "Webull ตอบกลับด้วยสถานะที่ไม่คาดคิด",
        };
        format!("{reason} ({})", self.summary())
    }
}

/// Blocking fetcher — mirrors `_do_call`: GET `path?params`, returns
/// (status, json body). Implementations provide transport (mock / reqwest).
pub trait HttpFetch {
    fn get(&mut self, path: &str, params: &[(String, String)]) -> Result<(u16, Value), FetchError>;

    /// POST JSON แบบเซ็นเหมือนกัน (stocks/bars ใช้) — default คืน error
    fn post_json(&mut self, _path: &str, _body: &str) -> Result<(u16, Value), FetchError> {
        Err(FetchError::Network(
            "post_json not supported by this client".into(),
        ))
    }
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
    fn typed_readers_never_invent_values() {
        let row = json!({
            "a": "12.50", "b": "", "c": null, "d": "junk", "e": -3,
            "f": 1795208309936i64, "g": "1795208309936", "h": " 42 ", "i": "  ", "j": 0,
        });
        assert_eq!(opt_f64(row.get("a")), Some(12.5));
        assert_eq!(opt_f64(row.get("e")), Some(-3.0));
        assert_eq!(opt_f64(row.get("j")), Some(0.0)); // a real zero stays a zero
        for key in ["b", "c", "d", "missing"] {
            assert_eq!(opt_f64(row.get(key)), None, "{key}");
        }
        assert_eq!(opt_i64(row.get("f")), Some(1_795_208_309_936));
        assert_eq!(opt_i64(row.get("g")), Some(1_795_208_309_936));
        assert_eq!(opt_i64(row.get("h")), Some(42));
        // a decimal string is not silently truncated into an integer
        assert_eq!(opt_i64(row.get("a")), None);
        assert_eq!(opt_i64(row.get("d")), None);
        assert_eq!(opt_str(row.get("a")), Some("12.50".into()));
        assert_eq!(opt_str(row.get("i")), None);
        assert_eq!(opt_str(row.get("e")), None);
    }

    /// Error bodies exactly as the Thailand host returned them (2026-11-22/23):
    /// the same status can mean a credential problem or a missing subscription.
    #[test]
    fn failure_kinds_follow_the_broker_code_before_the_status() {
        let cases = [
            (
                429,
                r#"{"message":"rate limit exceeded"}"#,
                FailureKind::RateLimited,
            ),
            (
                401,
                r#"{"error_code":"INVALID_TOKEN","message":"Header x-access-token is missing or invalid."}"#,
                FailureKind::Token,
            ),
            (
                401,
                r#"{"error_code":"UNAUTHORIZED","message":"Invalid credentials ... correct environment"}"#,
                FailureKind::Auth,
            ),
            (
                401,
                r#"{"message":"Header x-signature is invalid."}"#,
                FailureKind::Auth,
            ),
            (
                403,
                r#"{"error_code":"MARKET_DATA_NOT_SUBSCRIBED","message":"Insufficient permission, please subscribe to FOOTPRINT."}"#,
                FailureKind::Permission,
            ),
            (403, "<html>Forbidden</html>", FailureKind::Permission),
            (
                400,
                r#"{"code":400,"message":"Illegal argument"}"#,
                FailureKind::InvalidRequest,
            ),
            (
                417,
                r#"{"error_code":"ILLEGAL_PARAMETER","message":"depth not more than 1"}"#,
                FailureKind::InvalidRequest,
            ),
            (
                417,
                r#"{"error_code":"INVALID_SESSION","message":"Mqtt connection not exist for session:probe_x"}"#,
                FailureKind::InvalidRequest,
            ),
            (404, "404: Not Found", FailureKind::NotFound),
            (
                502,
                "<html><body>Bad Gateway</body></html>",
                FailureKind::Server,
            ),
        ];
        for (status, body, kind) in cases {
            let e = FetchError::NonZero(status, body.to_string());
            assert_eq!(e.kind(), kind, "{status} {body}");
        }
        assert_eq!(
            FetchError::Network("timeout".into()).kind(),
            FailureKind::Network
        );
        assert_eq!(FetchError::Cancelled.kind(), FailureKind::Cancelled);
    }

    #[test]
    fn summaries_and_messages_never_echo_the_body() {
        let denied = FetchError::NonZero(
            403,
            r#"{"error_code":"MARKET_DATA_NOT_SUBSCRIBED","message":"Insufficient permission, please subscribe to FOOTPRINT. Manage your subscriptions here: https://www.webull.co.th/center/mall"}"#
                .into(),
        );
        assert_eq!(denied.summary(), "HTTP 403 · MARKET_DATA_NOT_SUBSCRIBED");
        let message = denied.user_message();
        assert!(
            message.contains("HTTP 403 · MARKET_DATA_NOT_SUBSCRIBED"),
            "{message}"
        );
        assert!(!message.contains("webull.co.th"), "{message}");
        assert_eq!(
            FetchError::NonZero(502, "<html>".into()).summary(),
            "HTTP 502"
        );
        assert_eq!(FetchError::Cancelled.summary(), "cancelled");

        // retry only what a repeat can fix
        assert!(FetchError::Network("x".into()).is_retriable());
        assert!(FetchError::NonZero(429, "".into()).is_retriable());
        assert!(FetchError::NonZero(503, "".into()).is_retriable());
        assert!(!FetchError::Cancelled.is_retriable());
        assert!(
            !FetchError::NonZero(401, r#"{"error_code":"INVALID_TOKEN"}"#.into()).is_retriable()
        );
        assert!(!denied.is_retriable());
    }

    #[test]
    fn rate_limit_classification() {
        assert!(is_rate_limited(429, &json!({})));
        assert!(is_rate_limited(200, &json!({"message": "Rate limited"})));
        assert!(is_rate_limited(200, &json!({"message": "too many, RATE?"})));
        assert!(!is_rate_limited(200, &json!({"message": "ok"})));
        assert!(!is_rate_limited(500, &json!({})));
    }

    /// The broker's `error_code` outranks the HTTP status: 403 +
    /// INVALID_SIGNATURE is a signing problem (Auth), not a missing
    /// subscription (Permission).
    #[test]
    fn signature_codes_outrank_the_403_status() {
        let e = FetchError::NonZero(403, r#"{"error_code":"INVALID_SIGNATURE"}"#.into());
        assert_eq!(e.kind(), FailureKind::Auth);
        let e = FetchError::NonZero(403, r#"{"error_code":"NO_PERMISSION"}"#.into());
        assert_eq!(e.kind(), FailureKind::Permission);
        let e = FetchError::NonZero(401, r#"{"error_code":"INVALID_TOKEN"}"#.into());
        assert_eq!(e.kind(), FailureKind::Token);
        // without a code, the message still beats the generic 4xx bucket
        let e = FetchError::NonZero(400, r#"{"message":"signature mismatch"}"#.into());
        assert_eq!(e.kind(), FailureKind::Auth);
    }

    /// A quota complaint is a phrase ("rate limit" / "too many"), not a bare
    /// "rate" substring: "generate", "separate", "accurate" are not quota
    /// errors and must not trigger retries.
    #[test]
    fn rate_limit_is_a_phrase_not_a_bare_rate_substring() {
        assert!(!is_rate_limited(
            200,
            &json!({"message": "failed to generate report"})
        ));
        assert!(!is_rate_limited(
            200,
            &json!({"message": "cannot separate the symbols"})
        ));
        assert!(!is_rate_limited(200, &json!({"message": "not accurate"})));
        assert!(is_rate_limited(
            200,
            &json!({"message": "Rate limit exceeded"})
        ));
        assert!(is_rate_limited(
            200,
            &json!({"message": "Too many requests"})
        ));
    }

    /// The retry predicate the transport uses: quota signals (status, broker
    /// code or message phrase) and gateway 5xx are retriable; ordinary
    /// rejections are not.
    #[test]
    fn retriable_response_predicate() {
        assert!(is_retriable_response(429, &json!({})));
        assert!(is_retriable_response(
            400,
            &json!({"error_code": "TOO_MANY_REQUESTS"})
        ));
        assert!(is_retriable_response(
            200,
            &json!({"error_code": "RATE_LIMIT_EXCEEDED"})
        ));
        assert!(is_retriable_response(
            200,
            &json!({"message": "rate limit exceeded"})
        ));
        assert!(is_retriable_response(
            200,
            &json!({"message": "Too many requests, slow down"})
        ));
        assert!(is_retriable_response(502, &json!({})));
        assert!(is_retriable_response(503, &json!({})));
        assert!(is_retriable_response(504, &json!({})));
        assert!(!is_retriable_response(
            200,
            &json!({"message": "failed to generate report"})
        ));
        assert!(!is_retriable_response(
            401,
            &json!({"error_code": "INVALID_TOKEN"})
        ));
        assert!(!is_retriable_response(
            400,
            &json!({"message": "bad argument"})
        ));
        assert!(!is_retriable_response(500, &json!({})));
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
