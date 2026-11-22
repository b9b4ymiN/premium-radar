//! Real HTTP transport for Webull — composes throttle + signing + retry +
//! cache exactly like `_do_call` / `webull_get` in the Python reference.
//! Enabled behind the `client` feature (reqwest + uuid).

use std::time::Duration;

use serde_json::Value;

use super::rest::{Credentials, FetchError, HttpFetch, SnapCache, Throttle, is_rate_limited};
use super::sign::{HOST, X_VERSION, sign};

const RETRIES: u32 = 2;
const RETRY_DELAY_SEC: f64 = 1.2;
const BASE_URL: &str = "https://api.webull.co.th";

/// Full Webull REST client: shared throttle, TTL cache, HMAC-SHA1 signing,
/// and rate-limited retry (2 retries × 1.2 s — mirrors `_do_call`).
pub struct WebullClient {
    creds: Credentials,
    http: reqwest::blocking::Client,
    throttle: Throttle,
    cache: SnapCache,
    base_url: String,
}

impl WebullClient {
    pub fn new(creds: Credentials) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self {
            creds,
            http,
            throttle: Throttle::from_constants(),
            cache: SnapCache::from_constants(),
            base_url: BASE_URL.to_string(),
        }
    }

    /// Override the base URL (wiremock tests).
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// One signed+throttled GET, no cache/retry (mirrors the inner half of `_do_call`).
    fn send_once(
        &mut self,
        path: &str,
        params: &[(String, String)],
    ) -> Result<(u16, Value), FetchError> {
        // throttle: wait until at least MIN_INTERVAL_SEC since the last call
        let wait = self.throttle.wait_duration(self.throttle.last());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
        self.throttle.mark();

        let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let signature = sign(
            path,
            params,
            &ts,
            &nonce,
            &self.creds.app_key,
            &self.creds.app_secret,
            "",
        );

        let url = format!("{}{}", self.base_url, path);
        let mut req = self
            .http
            .get(&url)
            .header("Accept", "application/json")
            .header("x-app-key", &self.creds.app_key)
            .header("x-app-secret", &self.creds.app_secret)
            .header("x-timestamp", &ts)
            .header("x-signature-version", "1.0")
            .header("x-signature-algorithm", "HMAC-SHA1")
            .header("x-signature-nonce", &nonce)
            .header("x-version", X_VERSION)
            .header("x-signature", &signature);
        if let Some(tok) = &self.creds.access_token {
            req = req.header("x-access-token", tok);
        }
        if !params.is_empty() {
            req = req.query(params);
        }

        let resp = req.send().map_err(|e| FetchError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let body: Value = resp.json().unwrap_or(Value::Null);
        Ok((status, body))
    }

    /// Throttled signed GET with rate-limited retry (mirrors `_do_call` fully).
    fn do_get(
        &mut self,
        path: &str,
        params: &[(String, String)],
    ) -> Result<(u16, Value), FetchError> {
        let mut retries = RETRIES;
        loop {
            let (status, body) = self.send_once(path, params)?;
            if is_rate_limited(status, &body) && retries > 0 {
                std::thread::sleep(Duration::from_secs_f64(RETRY_DELAY_SEC));
                retries -= 1;
                continue;
            }
            return Ok((status, body));
        }
    }
}

impl HttpFetch for WebullClient {
    /// Cache check → do_get → cache put on 200 (mirrors `webull_get`).
    fn get(&mut self, path: &str, params: &[(String, String)]) -> Result<(u16, Value), FetchError> {
        let key = SnapCache::key(path, params);
        if let Some(hit) = self.cache.get(&key, std::time::Instant::now()) {
            return Ok(hit.clone());
        }
        let res = self.do_get(path, params)?;
        if res.0 == 200 {
            self.cache.put(key, res.clone());
        }
        Ok(res)
    }
}

// keep HOST referenced so the constant stays discoverable from this module
#[allow(dead_code)]
const _HOST: &str = HOST;
