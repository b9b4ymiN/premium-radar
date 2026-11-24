//! Webull request signing — two schemes, selected by [`SignatureScheme`]. Both
//! share one canonical string and differ only in hash, body digest and version.
//!
//! * **V3** — HMAC-SHA256, upper-case SHA-256 body hash, `x-version: v3`, no
//!   app-secret header. This is the current official signature guide and what
//!   the official Python SDK sends (`default_signature_composer` forces
//!   `sha_hmac256_new`). Pinned by `tests/fixtures/webull_sign_v3.json`, which
//!   that SDK composer produced with a test key pair.
//! * **V2** — HMAC-SHA1, upper-case MD5 body hash, `x-version: v2`: the pre-v3
//!   SDK algorithm, ported from the Python reference. Pinned by
//!   `tests/fixtures/webull_sign.json`.

use base64::Engine as _;
use hmac::{Hmac, Mac};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest as _, Sha256};

pub const HOST: &str = "api.webull.co.th";
/// Shared REST throttle (seconds between calls): `60 / (300 × 0.85) ≈ 0.235`
/// — one call at most every 0.235 s per origin, i.e. 85% of the Data API
/// quota of 300/60 s (owner's decision: 80–90%), which every other documented
/// family's window also stays under. Defined once here; `rest.rs` re-exports
/// it and the cross-process pacing file uses the same value.
pub const MIN_INTERVAL_SEC: f64 = 0.235;
/// Snapshot cache TTL, mirrors `CACHE_TTL_SEC`.
pub const CACHE_TTL_SEC: f64 = 3.0;

type HmacSha1 = Hmac<Sha1>;
type HmacSha256 = Hmac<Sha256>;

/// Which signing scheme a request uses. Persisted in settings as `"v3"`/`"v2"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignatureScheme {
    #[default]
    V3,
    V2,
}

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
/// host/app-key/algorithm/nonce/version/timestamp), plus `&` + the upper-case
/// hex body hash when a body is present.
/// `host` MUST be the host actually being called — the server recomputes the
/// signature with the request host and rejects mismatches.
#[allow(clippy::too_many_arguments)]
pub fn canonical_string(
    path: &str,
    host: &str,
    query: &[(String, String)],
    ts: &str,
    nonce: &str,
    app_key: &str,
    algorithm: &str,
    body_hash: Option<&str>,
) -> String {
    let mut allp: Vec<(String, String)> = query.to_vec();
    allp.extend([
        ("host".to_string(), host.to_string()),
        ("x-app-key".to_string(), app_key.to_string()),
        ("x-signature-algorithm".to_string(), algorithm.to_string()),
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
    match body_hash {
        Some(hash) => format!("{path}&{str1}&{hash}"),
        None => format!("{path}&{str1}"),
    }
}

/// Base64 HMAC of `payload`, keyed `"{secret}&"`. `new_from_slice` needs the
/// digest type at compile time, so each scheme expands its own MAC here.
macro_rules! hmac_base64 {
    ($mac:ty, $secret:expr, $payload:expr) => {{
        let mut mac = <$mac>::new_from_slice(format!("{}&", $secret).as_bytes())
            .expect("hmac accepts any key length");
        mac.update($payload.as_bytes());
        base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }};
}

impl SignatureScheme {
    /// `x-signature-algorithm` value — the server validates the name itself.
    pub fn algorithm(self) -> &'static str {
        match self {
            Self::V3 => "HMAC-SHA256",
            Self::V2 => "HMAC-SHA1",
        }
    }

    /// The scheme this one is not — a connection test falls back to it.
    pub fn other(self) -> Self {
        match self {
            Self::V3 => Self::V2,
            Self::V2 => Self::V3,
        }
    }

    /// `x-version` value.
    pub fn x_version(self) -> &'static str {
        match self {
            Self::V3 => "v3",
            Self::V2 => "v2",
        }
    }

    /// Upper-case hex digest of the EXACT body bytes sent; `None` without a body
    /// (the digest part is then omitted from the canonical string entirely).
    pub fn body_hash(self, body: &str) -> Option<String> {
        if body.is_empty() {
            return None;
        }
        let hex: String = match self {
            Self::V3 => Sha256::digest(body.as_bytes())
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect(),
            Self::V2 => Md5::digest(body.as_bytes())
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect(),
        };
        Some(hex)
    }

    /// Base64 signature of the percent-encoded canonical string, keyed with
    /// `"{app_secret}&"`. Key material comes from settings/keyring at runtime;
    /// never from source or logs.
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        self,
        path: &str,
        host: &str,
        query: &[(String, String)],
        ts: &str,
        nonce: &str,
        app_key: &str,
        app_secret: &str,
        body: &str,
    ) -> String {
        let canonical = canonical_string(
            path,
            host,
            query,
            ts,
            nonce,
            app_key,
            self.algorithm(),
            self.body_hash(body).as_deref(),
        );
        let payload = quote_py(&canonical);
        match self {
            Self::V3 => hmac_base64!(HmacSha256, app_secret, payload),
            Self::V2 => hmac_base64!(HmacSha1, app_secret, payload),
        }
    }
}

/// Legacy V2 canonical string.
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
    canonical_string(
        path,
        host,
        query,
        ts,
        nonce,
        app_key,
        SignatureScheme::V2.algorithm(),
        SignatureScheme::V2.body_hash(body).as_deref(),
    )
}

/// Legacy V2 signature.
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
    SignatureScheme::V2.sign(path, host, query, ts, nonce, app_key, app_secret, body)
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

    #[test]
    fn schemes_differ_only_where_expected() {
        assert_eq!(SignatureScheme::V3.algorithm(), "HMAC-SHA256");
        assert_eq!(SignatureScheme::V2.algorithm(), "HMAC-SHA1");
        assert_eq!(SignatureScheme::V3.x_version(), "v3");
        assert_eq!(SignatureScheme::V2.x_version(), "v2");
        assert_eq!(SignatureScheme::default(), SignatureScheme::V3);

        // empty body → no digest segment in either scheme
        let q: Vec<(String, String)> = vec![];
        for scheme in [SignatureScheme::V3, SignatureScheme::V2] {
            assert_eq!(scheme.body_hash(""), None);
            let canonical =
                canonical_string("/p", HOST, &q, "T", "N", "K", scheme.algorithm(), None);
            assert!(!canonical.contains("&&"), "{canonical}");
        }

        // digest lengths: SHA-256 = 64 hex chars, MD5 = 32
        let v3 = SignatureScheme::V3.body_hash("{}").unwrap();
        let v2 = SignatureScheme::V2.body_hash("{}").unwrap();
        assert_eq!(v3.len(), 64);
        assert_eq!(v2.len(), 32);
        assert_eq!(v3, v3.to_uppercase());
        assert_eq!(v2, v2.to_uppercase());
        // md5("{}") = 99914b932bd37a50b983c5e7c90ae93b
        assert_eq!(v2, "99914B932BD37A50B983C5E7C90AE93B");
    }

    #[test]
    fn scheme_roundtrips_through_settings_json() {
        for (scheme, text) in [
            (SignatureScheme::V3, "\"v3\""),
            (SignatureScheme::V2, "\"v2\""),
        ] {
            assert_eq!(serde_json::to_string(&scheme).unwrap(), text);
            let back: SignatureScheme = serde_json::from_str(text).unwrap();
            assert_eq!(back, scheme);
        }
        // an absent field deserializes to the official default
        #[derive(serde::Deserialize)]
        struct Cfg {
            #[serde(default)]
            scheme: SignatureScheme,
        }
        let cfg: Cfg = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg.scheme, SignatureScheme::V3);
    }
}
