//! Blocking read-only client for the recorder server (`crates/recorder-server`).
//!
//! - Every call must run OFF the UI thread (blocking reqwest, same rule as
//!   `with_client_from` — a debug build panics if the client is dropped on an
//!   async thread).
//! - The API token travels only in the `Authorization: Bearer` header to the
//!   configured server. It is never logged, never embedded in an error and
//!   never returned to JS.
//! - Failures are typed and each variant maps to ONE short Thai sentence via
//!   [`RecorderError::message_th`] — no URLs, bodies or tokens leak through.
//! - We send `Accept-Encoding: identity` explicitly: the server compresses
//!   only when asked, and this build of reqwest has no gzip decoder.

use std::time::Duration;

use radar_core::lab::types::Observation;
use serde::Deserialize;

/// Per-request budget — a down server must never hold a command hostage.
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecorderError {
    BadUrl,
    Unreachable,
    Timeout,
    Unauthorized,
    BadResponse,
}

impl RecorderError {
    /// Short plain-Thai message for the UI. Static strings only.
    pub fn message_th(&self) -> &'static str {
        match self {
            Self::BadUrl => "ที่อยู่ server ไม่ถูกต้อง — ต้องขึ้นต้นด้วย http:// หรือ https://",
            Self::Unreachable => "ต่อ server ไม่ได้ — ตรวจที่อยู่หรือ Tailscale",
            Self::Timeout => "server ตอบช้าเกินไป (timeout)",
            Self::Unauthorized => "token ไม่ถูกต้อง",
            Self::BadResponse => "server ตอบกลับมาไม่ตรงรูปแบบที่คาด",
        }
    }
}

/// `GET /health` (no auth) → `{"ok":true,"version":"…"}`
#[derive(Debug, Deserialize, PartialEq)]
pub struct Health {
    pub ok: bool,
    pub version: String,
}

/// An empty/whitespace token never leaves the machine — the verdict is the
/// same one the server would give, without the round-trip.
fn require_token(token: &str) -> Result<&str, RecorderError> {
    let trimmed = token.trim();
    (!trimmed.is_empty())
        .then_some(trimmed)
        .ok_or(RecorderError::Unauthorized)
}

fn client() -> Result<reqwest::blocking::Client, RecorderError> {
    reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|_| RecorderError::Unreachable)
}

fn map_reqwest_err(e: reqwest::Error) -> RecorderError {
    if e.is_timeout() {
        RecorderError::Timeout
    } else if e.is_connect() {
        RecorderError::Unreachable
    } else {
        RecorderError::BadResponse
    }
}

/// GET `{url}{path}` with an optional bearer token; non-2xx maps to a typed
/// error. Rejects URLs reqwest cannot parse up front so garbage stored in the
/// settings file reads as "bad address", not "unreachable".
fn send(
    url: &str,
    path: &str,
    token: Option<&str>,
) -> Result<reqwest::blocking::Response, RecorderError> {
    let full = format!("{url}{path}");
    let parsed = reqwest::Url::parse(&full).map_err(|_| RecorderError::BadUrl)?;
    // Url::parse accepts any scheme (ftp://, file:// …) and an empty host —
    // reqwest only speaks http(s) to a real host
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().unwrap_or("").is_empty() {
        return Err(RecorderError::BadUrl);
    }
    let mut request = client()?.get(&full).header("accept-encoding", "identity");
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request.send().map_err(map_reqwest_err)?;
    match response.status() {
        status if status.is_success() => Ok(response),
        reqwest::StatusCode::UNAUTHORIZED => Err(RecorderError::Unauthorized),
        _ => Err(RecorderError::BadResponse),
    }
}

/// A 2xx body that still fails to decode is a "bad response" — never a panic.
fn decode<T: serde::de::DeserializeOwned>(
    response: reqwest::blocking::Response,
) -> Result<T, RecorderError> {
    response.json().map_err(|_| RecorderError::BadResponse)
}

pub fn health(url: &str) -> Result<Health, RecorderError> {
    decode(send(url, "/health", None)?)
}

/// `GET /v1/status` → status.json contents + `server_time`. Auth required.
pub fn status(url: &str, token: &str) -> Result<serde_json::Value, RecorderError> {
    let token = require_token(token)?;
    decode(send(url, "/v1/status", Some(token))?)
}

/// `GET /v1/dates` → `{"dates":["YYYY-MM-DD",…]}`. The server already sorts;
/// we sort again (ISO dates sort chronologically) so `last()` is honest even
/// against a hand-rolled server.
pub fn dates(url: &str, token: &str) -> Result<Vec<String>, RecorderError> {
    let token = require_token(token)?;
    #[derive(Deserialize)]
    struct DatesBody {
        dates: Vec<String>,
    }
    let body: DatesBody = decode(send(url, "/v1/dates", Some(token))?)?;
    let mut dates = body.dates;
    dates.sort();
    Ok(dates)
}

/// `GET /v1/observations?date=YYYY-MM-DD` → the server's `Observation` array.
/// Kept for the follow-up work that reads captures from the server.
pub fn observations(url: &str, token: &str, date: &str) -> Result<Vec<Observation>, RecorderError> {
    let token = require_token(token)?;
    decode(send(
        url,
        &format!("/v1/observations?date={date}"),
        Some(token),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// One-shot HTTP server on a throwaway port: hands the (path, had-bearer)
    /// of the single request to `respond` and writes the (status, JSON body)
    /// back. std::net only — no extra crates.
    fn serve_once(respond: impl FnOnce(&str, bool) -> (u16, String) + Send + 'static) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = request
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .split(' ')
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                let authed = request
                    .lines()
                    .any(|l| l.to_ascii_lowercase().starts_with("authorization: bearer "));
                let (status, body) = respond(&path, authed);
                let reason = match status {
                    200 => "OK",
                    401 => "Unauthorized",
                    _ => "Internal Server Error",
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len(),
                );
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn health_parses() {
        let url = serve_once(|path, authed| {
            assert_eq!(path, "/health");
            assert!(!authed, "health must not need auth");
            (200, r#"{"ok":true,"version":"0.1.0"}"#.into())
        });
        let health = health(&url).expect("health ok");
        assert_eq!(
            health,
            Health {
                ok: true,
                version: "0.1.0".into()
            }
        );
    }

    #[test]
    fn dates_parses_sorts_and_sends_bearer() {
        let url = serve_once(|path, authed| {
            assert_eq!(path, "/v1/dates");
            assert!(authed, "dates must carry the bearer token");
            (
                200,
                r#"{"dates":["2026-11-24","2026-11-22","2026-11-23"]}"#.into(),
            )
        });
        let dates = dates(&url, "tok").expect("dates ok");
        assert_eq!(
            dates,
            vec!["2026-11-22", "2026-11-23", "2026-11-24"],
            "client sorts defensively"
        );
    }

    #[test]
    fn status_parses_the_merged_shape() {
        let url = serve_once(|_, _| {
            (
                200,
                r#"{"completed_slots":[],"server_time":"2026-11-24T22:00:00Z"}"#.into(),
            )
        });
        let value = status(&url, "tok").expect("status ok");
        assert_eq!(value["server_time"], "2026-11-24T22:00:00Z");
    }

    #[test]
    fn unauthorized_maps_to_the_token_message() {
        let url = serve_once(|_, _| (401, r#"{"error":"missing or invalid bearer token"}"#.into()));
        let err = dates(&url, "wrong").unwrap_err();
        assert_eq!(err, RecorderError::Unauthorized);
        assert_eq!(err.message_th(), "token ไม่ถูกต้อง");
    }

    #[test]
    fn wrong_shape_is_a_bad_response() {
        let url = serve_once(|_, _| (200, r#"{"unexpected":true}"#.into()));
        assert_eq!(health(&url).unwrap_err(), RecorderError::BadResponse);
        let url = serve_once(|_, _| (200, "not json at all".into()));
        assert_eq!(dates(&url, "tok").unwrap_err(), RecorderError::BadResponse);
    }

    #[test]
    fn empty_token_short_circuits_without_a_request() {
        // an unreachable address would still be beaten by the short-circuit
        assert_eq!(
            dates("http://127.0.0.1:1", "   ").unwrap_err(),
            RecorderError::Unauthorized
        );
        assert_eq!(
            status("http://127.0.0.1:1", "").unwrap_err(),
            RecorderError::Unauthorized
        );
    }

    #[test]
    fn refused_connection_is_unreachable_and_garbage_url_is_bad_url() {
        // bind, grab the port, drop the listener → connections are refused
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        assert_eq!(
            health(&format!("http://{addr}")).unwrap_err(),
            RecorderError::Unreachable
        );
        // "http://" with no host is rejected by normalise_recorder_api_url
        // before it ever reaches a request; here only truly unsendable URLs
        // (no scheme / non-http scheme) must read as BadUrl
        for garbage in ["", "no-scheme", "ftp://host", "file:///etc"] {
            assert_eq!(
                health(garbage).unwrap_err(),
                RecorderError::BadUrl,
                "{garbage:?}"
            );
        }
    }

    #[test]
    fn thai_messages_never_mention_the_token_value() {
        for err in [
            RecorderError::BadUrl,
            RecorderError::Unreachable,
            RecorderError::Timeout,
            RecorderError::Unauthorized,
            RecorderError::BadResponse,
        ] {
            let message = err.message_th();
            assert!(!message.is_empty(), "{err:?}");
            assert!(!message.contains("SECRET"), "{err:?}");
        }
    }
}
