//! Settings persistence — radar_settings.json in the app data directory.
//! Mirrors the Python original's load_settings/save_settings.

use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AppSettings {
    #[serde(default)]
    pub font_size_adjust: i32,
    #[serde(default)]
    pub show_math: bool,
    #[serde(default = "default_symbol")]
    pub last_symbol: String,
    #[serde(default = "default_strategy")]
    pub last_strategy: String,
    #[serde(default = "default_risk")]
    pub last_risk: String,
    #[serde(default)]
    pub last_budget: f64,
    #[serde(default)]
    pub llm_provider: String, // "glm" | "openai" | ""
    #[serde(default)]
    pub llm_api_key: String,  // stored in OS keyring in production; env for dev
}

fn default_symbol() -> String { "NVDA".into() }
fn default_strategy() -> String { "CSP+CC".into() }
fn default_risk() -> String { "BALANCED".into() }

impl AppSettings {
    /// Load from `<app_data>/radar_settings.json`, or defaults if missing.
    pub fn load(app_data_dir: &str) -> Self {
        let path = Self::path(app_data_dir);
        match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save to `<app_data>/radar_settings.json`.
    pub fn save(&self, app_data_dir: &str) -> Result<(), String> {
        let path = Self::path(app_data_dir);
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, raw).map_err(|e| format!("save settings: {e}"))
    }

    fn path(app_data_dir: &str) -> PathBuf {
        PathBuf::from(app_data_dir).join("radar_settings.json")
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
        assert_eq!(s.font_size_adjust, 0);
    }

    #[test]
    fn roundtrip_json() {
        let s = AppSettings {
            last_symbol: "TSLA".into(),
            last_budget: 25000.0,
            llm_provider: "glm".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.last_symbol, "TSLA");
        assert_eq!(back.last_budget, 25000.0);
        assert_eq!(back.llm_provider, "glm");
    }
}
