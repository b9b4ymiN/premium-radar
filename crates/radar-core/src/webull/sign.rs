//! Webull request signing — port of `_sign` in the Python reference, pinned
//! by `tests/fixtures/webull_sign.json` (values produced by the real Python
//! implementation with a test key pair).

use base64::Engine as _;
use hmac::{Hmac, Mac};
use md5::Digest as _;
use md5::Md5;
use sha1::Sha1;

pub const HOST: &str = "api.webull.co.th";
pub const X_VERSION: &str = "v2";
/// Shared REST throttle (seconds between calls), mirrors `MIN_INTERVAL_SEC`.
pub const MIN_INTERVAL_SEC: f64 = 1.1;
/// Snapshot cache TTL, mirrors `CACHE_TTL_SEC`.
pub const CACHE_TTL_SEC: f64 = 3.0;

type HmacSha1 = Hmac<Sha1>;

/// Percent-encoding compatible with Python `urllib.parse.quote(s, safe="")`
/// (RFC 3986 unreserved set only — note `/` and `&` ARE escaped).
pub fn quote_py(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The pre-hash payload: path + `&` + sorted `k=v` params (query params plus
/// host/app-key/algorithm/nonce/version/timestamp), plus `&` + upper-case
/// MD5 hex of the body when a body is present. Mirrors `_sign` exactly.
/// `host` MUST be the host actually being called — the server recomputes the
/// signature with the request host and rejects mismatches.
#[allow(clippy::too_many_arguments)]
pub fn sign_base(
    path: &str,
    host: &str,
    query: &[(String, String)],
    ts: &str,
    nonce: &str,
    app_key: &str,
    body: &str,
) -> String {
    let mut allp: Vec<(String, String)> = query.to_vec();
    allp.extend([
        ("host".to_string(), host.to_string()),
        ("x-app-key".to_string(), app_key.to_string()),
        ("x-signature-algorithm".to_string(), "HMAC-SHA1".to_string()),
        ("x-signature-nonce".to_string(), nonce.to_string()),
        ("x-signature-version".to_string(), "1.0".to_string()),
        ("x-timestamp".to_string(), ts.to_string()),
    ]);
    allp.sort_by(|a, b| a.0.cmp(&b.0));
    let str1 = allp
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    if body.is_empty() {
        format!("{path}&{str1}")
    } else {
        let mut h = Md5::new();
        h.update(body.as_bytes());
        let md5_hex: String = h.finalize().iter().map(|b| format!("{b:02X}")).collect();
        format!("{path}&{str1}&{md5_hex}")
    }
}

/// HMAC-SHA1 signature (base64) of the percent-encoded payload, keyed with
/// `"{app_secret}&"` — exactly like the Python original. Key material comes
/// from settings/keyring at runtime; never from source or logs.
#[allow(clippy::too_many_arguments)]
pub fn sign(
    path: &str,
    host: &str,
    query: &[(String, String)],
    ts: &str,
    nonce: &str,
    app_key: &str,
    app_secret: &str,
    body: &str,
) -> String {
    let payload = quote_py(&sign_base(path, host, query, ts, nonce, app_key, body));
    let mut mac = HmacSha1::new_from_slice(format!("{app_secret}&").as_bytes())
        .expect("hmac accepts any key length");
    mac.update(payload.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_matches_python_safe_empty() {
        assert_eq!(quote_py("a b"), "a%20b");
        assert_eq!(quote_py("NVDA261218P00175000"), "NVDA261218P00175000");
        assert_eq!(quote_py("a&b=c+d"), "a%26b%3Dc%2Bd");
        assert_eq!(quote_py("/path&x=1"), "%2Fpath%26x%3D1");
    }

    #[test]
    fn params_sorted_before_join() {
        let q = vec![
            ("z".into(), "1".into()),
            ("a".into(), "2".into()),
            ("m".into(), "3".into()),
        ];
        let base = sign_base("/p", HOST, &q, "T", "N", "K", "");
        assert!(base.starts_with("/p&a=2&"), "base = {base}");
        assert!(base.contains("&host=api.webull.co.th"));
        assert!(base.ends_with("&z=1"));
    }

    #[test]
    fn host_is_signed_not_hardcoded() {
        // The signed `host` must follow the actual request host — this is the
        // UAT bug: signing api.webull.co.th while calling th-api.uat… gave
        // "Header x-signature is invalid".
        let q: Vec<(String, String)> = vec![];
        let a = sign_base("/p", "th-api.uat.webullbroker.com", &q, "T", "N", "K", "");
        let b = sign_base("/p", HOST, &q, "T", "N", "K", "");
        assert!(a.contains("host=th-api.uat.webullbroker.com"));
        assert!(b.contains("host=api.webull.co.th"));
        assert_ne!(a, b);
    }
}
