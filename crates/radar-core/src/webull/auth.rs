//! Webull Thailand access-token endpoints.
//!
//! The broker requires an `x-access-token` on production data endpoints in
//! addition to app-key signing (verified against a live integration in the
//! vcpSystem project: `POST /openapi/auth/token/create` triggers an SMS/2FA
//! confirmation in the Webull app, tokens live ~15 days). This module checks
//! an existing token so the setup screen can tell the user whether theirs is
//! still `NORMAL` before blaming the app key.

use serde_json::Value;

use crate::webull::rest::{FetchError, PATH_AUTH_TOKEN_CHECK, PATH_AUTH_TOKEN_CREATE};
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

/// Result of token creation: the token VALUE exists immediately (status
/// PENDING) but only becomes usable after the SMS/2FA confirmation.
#[derive(Clone, Debug)]
pub struct CreatedToken {
    pub status: TokenStatus,
    pub token: Option<String>,
    pub expires_ms: Option<u64>,
}

/// Shared response shape `{token?, expires?, status?}` — fields are optional
/// on non-NORMAL states (vcpSystem token-manager contract).
fn parse_token_payload(data: &Value) -> (TokenStatus, Option<String>, Option<u64>) {
    let status = match data.get("status").and_then(Value::as_str) {
        Some("NORMAL") => TokenStatus::Normal,
        Some("PENDING") => TokenStatus::Pending,
        Some("INVALID") => TokenStatus::Invalid,
        Some("EXPIRED") => TokenStatus::Expired,
        _ => TokenStatus::None,
    };
    (
        status,
        data.get("token")
            .and_then(Value::as_str)
            .map(str::to_string),
        data.get("expires").and_then(Value::as_u64),
    )
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
    let (st, _, expires_ms) = parse_token_payload(&data);
    Ok(TokenCheck {
        status: st,
        expires_ms,
    })
}

/// POST `/openapi/auth/token/create` with an EMPTY body (no MD5 part in the
/// signature — verified against production 2026-11-22). Returns the new token
/// as PENDING; the user has 5 minutes to confirm the SMS code in the Webull
/// app, after which `check_token` reports NORMAL.
pub fn create_token(client: &mut WebullClient) -> Result<CreatedToken, FetchError> {
    let (status, data) = client.post_json(PATH_AUTH_TOKEN_CREATE, "")?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    let (st, token, expires_ms) = parse_token_payload(&data);
    Ok(CreatedToken {
        status: st,
        token,
        expires_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_pending_with_token_value() {
        let (st, tok, exp) = parse_token_payload(
            &json!({"token": "abc123", "expires": 1796654338228u64, "status": "PENDING"}),
        );
        assert_eq!(st, TokenStatus::Pending);
        assert_eq!(tok.as_deref(), Some("abc123"));
        assert_eq!(exp, Some(1796654338228));
    }

    #[test]
    fn parse_normal_without_optional_fields() {
        let (st, tok, exp) = parse_token_payload(&json!({"status": "NORMAL"}));
        assert_eq!(st, TokenStatus::Normal);
        assert!(tok.is_none());
        assert!(exp.is_none());
    }

    #[test]
    fn parse_invalid_state() {
        let (st, _, _) = parse_token_payload(&json!({"status": "INVALID"}));
        assert_eq!(st, TokenStatus::Invalid);
        let (unknown, _, _) = parse_token_payload(&json!({}));
        assert_eq!(unknown, TokenStatus::None);
    }
}
