//! Settings persistence + credential management for end users.
//! First-run setup screen stores keys here; OS keyring in P7 production.

use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use radar_core::copilot::LlmConfig;
use radar_core::webull::rest::Credentials;

/// Full app settings persisted to `<app_data>/radar_settings.json`.
/// API keys are stored here for dev; production moves them to OS keyring.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
    // UI preferences
    #[serde(default)]
    pub font_size_adjust: i32,
    #[serde(default)]
    pub show_math: bool,

    // Last session state
    #[serde(default = "default_symbol")]
    pub last_symbol: String,
    #[serde(default = "default_strategy")]
    pub last_strategy: String,
    #[serde(default = "default_risk")]
    pub last_risk: String,
    #[serde(default)]
    pub last_budget: f64,

    // Webull credentials (user-configurable via setup screen)
    #[serde(default)]
    pub webull_app_key: String,
    #[serde(default)]
    pub webull_app_secret: String,
    #[serde(default = "default_webull_url")]
    pub webull_base_url: String,

    /// Optional access token (Python `ACCESS_TOKEN`) — some Webull app keys
    /// require the extra `x-access-token` header.
    #[serde(default)]
    pub webull_access_token: String,

    // LLM configuration (user-configurable via setup screen)
    #[serde(default)]
    pub llm: Option<LlmConfig>,
}

fn default_symbol() -> String { "NVDA".into() }
fn default_strategy() -> String { "CSP+CC".into() }
fn default_risk() -> String { "BALANCED".into() }
fn default_webull_url() -> String { "https://api.webull.co.th".into() }

/// Manual Default so `AppSettings::default()` matches the serde field
/// defaults exactly (the derived Default zeroed every field — a fresh install
/// then behaved differently from a loaded-with-missing-keys file).
impl Default for AppSettings {
    fn default() -> Self {
        Self {
            font_size_adjust: 0,
            show_math: false,
            last_symbol: default_symbol(),
            last_strategy: default_strategy(),
            last_risk: default_risk(),
            last_budget: 0.0,
            webull_app_key: String::new(),
            webull_app_secret: String::new(),
            webull_base_url: default_webull_url(),
            webull_access_token: String::new(),
            llm: None,
        }
    }
}

impl AppSettings {
    pub fn load(app_data_dir: &str) -> Self {
        let path = Self::path(app_data_dir);
        match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, app_data_dir: &str) -> Result<(), String> {
        let path = Self::path(app_data_dir);
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, raw).map_err(|e| format!("save settings: {e}"))
    }

    fn path(app_data_dir: &str) -> PathBuf {
        PathBuf::from(app_data_dir).join("radar_settings.json")
    }

    // ---------- credential helpers ----------

    /// Webull credentials from settings, falling back to env vars.
    /// Values are trimmed — paste artifacts are the #1 cause of 401s.
    pub fn webull_credentials(&self) -> Option<Credentials> {
        let key = self.webull_app_key.trim();
        let secret = self.webull_app_secret.trim();
        if !key.is_empty() && !secret.is_empty() {
            return Some(Credentials {
                app_key: key.to_string(),
                app_secret: secret.to_string(),
                access_token: (!self.webull_access_token.trim().is_empty())
                    .then(|| self.webull_access_token.trim().to_string()),
            });
        }
        Credentials::from_env()
    }

    /// LLM config from settings, falling back to env vars.
    pub fn llm_config(&self) -> Option<LlmConfig> {
        if let Some(llm) = &self.llm
            && llm.is_ready()
        {
            return Some(llm.clone());
        }
        LlmConfig::from_env()
    }

    /// Check if the user has completed initial setup (has Webull keys).
    pub fn is_configured(&self) -> bool {
        !self.webull_app_key.is_empty() || std::env::var("WEBULL_APP_KEY").is_ok()
    }

    /// Log-safe summary for status display.
    pub fn status_summary(&self) -> String {
        let webull_ok = self.webull_credentials().is_some();
        let llm = self.llm_config();
        let llm_ok = llm.is_some();
        let llm_name = llm.map(|l| format!("{} ({})", l.label, l.model)).unwrap_or_default();
        format!(
            "Webull: {} | LLM: {}{}",
            if webull_ok { "✓ connected" } else { "✗ not set" },
            if llm_ok { "✓ " } else { "✗ " },
            llm_name,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let s = AppSettings::default();
        assert_eq!(s.last_symbol, "NVDA");
        assert_eq!(s.last_strategy, "CSP+CC");
        assert_eq!(s.last_risk, "BALANCED");
        assert!(!s.is_configured());
        assert!(s.webull_credentials().is_none());
    }

    #[test]
    fn roundtrip_json() {
        let s = AppSettings {
            last_symbol: "TSLA".into(),
            last_budget: 25000.0,
            webull_app_key: "test_key".into(),
            webull_app_secret: "test_secret".into(),
            llm: Some(LlmConfig::new("Test LLM", "https://api.test.com", "test-model", "test-llm-key")),
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.last_symbol, "TSLA");
        assert_eq!(back.webull_app_key, "test_key");
        assert!(back.llm.is_some());
        assert!(back.is_configured());
    }

    #[test]
    fn llm_config_falls_back_to_env() {
        let s = AppSettings::default();
        // no settings LLM → should try env
        let result = s.llm_config();
        // result depends on env vars in test environment; just verify no panic
        let _ = result;
    }

    #[test]
    fn status_summary_no_keys() {
        let s = AppSettings::default();
        let summary = s.status_summary();
        assert!(summary.contains("✗"));
    }
}
