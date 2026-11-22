//! Webull Thailand access-token endpoints.
//!
//! The broker requires an `x-access-token` on production data endpoints in
//! addition to app-key signing (verified against a live integration in the
//! vcpSystem project: `POST /openapi/auth/token/create` triggers an SMS/2FA
//! confirmation in the Webull app, tokens live ~15 days). This module checks
//! an existing token so the setup screen can tell the user whether theirs is
//! still `NORMAL` before blaming the app key.

use serde_json::Value;

use crate::webull::rest::{FetchError, PATH_AUTH_TOKEN_CHECK};
use crate::webull::transport::WebullClient;

/// Token states returned by `/openapi/auth/token/check`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenStatus {
    Normal,
    Pending,
    Invalid,
    Expired,
    None,
}

impl TokenStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Pending => "PENDING",
            Self::Invalid => "INVALID",
            Self::Expired => "EXPIRED",
            Self::None => "NONE",
        }
    }
}

/// Result of a token check: status + expiry epoch ms (when provided).
#[derive(Clone, Debug)]
pub struct TokenCheck {
    pub status: TokenStatus,
    pub expires_ms: Option<u64>,
}

/// POST `/openapi/auth/token/check` with `{token}` — mirrors the vcpSystem
/// token-manager contract (`{token, expires, status}`, fields optional on
/// non-NORMAL states).
pub fn check_token(client: &mut WebullClient, token: &str) -> Result<TokenCheck, FetchError> {
    let body = serde_json::json!({ "token": token }).to_string();
    let (status, data) = client.post_json(PATH_AUTH_TOKEN_CHECK, &body)?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    let st = match data.get("status").and_then(Value::as_str) {
        Some("NORMAL") => TokenStatus::Normal,
        Some("PENDING") => TokenStatus::Pending,
        Some("INVALID") => TokenStatus::Invalid,
        Some("EXPIRED") => TokenStatus::Expired,
        _ => TokenStatus::None,
    };
    Ok(TokenCheck {
        status: st,
        expires_ms: data.get("expires").and_then(Value::as_u64),
    })
}
