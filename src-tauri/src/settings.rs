//! Settings persistence + credential management for end users.
//! Non-secret preferences live in `<app_data>/radar_settings.json`; secrets
//! (Webull key/secret/token, LLM API key) live in the OS keyring — saving
//! fails closed when the keyring is unavailable instead of writing secrets
//! to disk as plaintext.

use radar_core::copilot::LlmConfig;
use radar_core::webull::rest::Credentials;
use radar_core::webull::sign::SignatureScheme;
use radar_core::webull::transport::WebullClient;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// OS keyring service name for all Premium Radar secrets.
const KEYRING_SERVICE: &str = "premium-radar";

/// Where the Home screen's Lab capture numbers come from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LabSource {
    /// This machine's own recordings — the default and today's behaviour.
    #[default]
    Local,
    /// A recorder server's read-only API (`/v1/*` with a bearer token).
    Api,
}

/// Unknown values load as Local instead of failing the whole file —
/// `AppSettings::load` falls back to ALL defaults on any serde error, so a
/// hand-edited `lab_source` must not wipe the user's other settings.
impl<'de> Deserialize<'de> for LabSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "api" => Self::Api,
            _ => Self::Local,
        })
    }
}

impl LabSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Api => "api",
        }
    }
}

/// Strict parse for the save command: only the two exact values are accepted.
pub fn parse_lab_source(value: &str) -> Option<LabSource> {
    match value.trim() {
        "local" => Some(LabSource::Local),
        "api" => Some(LabSource::Api),
        _ => None,
    }
}

/// Normalise the recorder-server URL (trim, drop trailing `/`) accepting only
/// `http://` or `https://` with a non-empty host. `None` = invalid.
pub fn normalise_recorder_api_url(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed == "http://" || trimmed == "https://" {
        return None;
    }
    let rest = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))?;
    let host = rest.split(['/', ':', '?']).next().unwrap_or_default();
    (!host.is_empty() && !rest.contains("://")).then(|| trimmed.to_string())
}

fn parse_ipv4(host: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut octets = [0u8; 4];
    for (i, part) in parts.iter().enumerate() {
        octets[i] = part.parse().ok()?;
    }
    Some(octets)
}

/// True when the URL's host is one where plain `http://` is acceptable:
/// localhost names, loopback (v4/v6) or a Tailscale CGNAT 100.64.0.0/10
/// address. The UI consults this only for `http://` URLs to decide whether
/// the "not encrypted" warning is shown; `https` is fine anywhere.
pub fn recorder_host_is_private(url: &str) -> bool {
    let Some((_, rest)) = url.trim().split_once("://") else {
        return false;
    };
    let host = if let Some(after) = rest.strip_prefix('[') {
        after.split(']').next().unwrap_or_default()
    } else {
        rest.split(['/', ':', '?']).next().unwrap_or_default()
    };
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower == "::1" {
        return true;
    }
    let Some([a, b, _, _]) = parse_ipv4(host) else {
        return false;
    };
    a == 127 || (a == 100 && (64..=127).contains(&b))
}

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

    /// Request signing scheme. V3 (HMAC-SHA256) is what the official guide and
    /// SDK send and what the Thailand host accepts; V2 stays selectable for a
    /// host that only takes the pre-v3 algorithm. The connection test detects
    /// and stores the working one, so nobody has to edit this by hand.
    #[serde(default)]
    pub webull_signature_scheme: SignatureScheme,

    // LLM configuration (user-configurable via setup screen)
    #[serde(default)]
    pub llm: Option<LlmConfig>,

    // Lab data source (user-configurable via setup screen)
    #[serde(default)]
    pub lab_source: LabSource,
    #[serde(default)]
    pub recorder_api_url: String,
    /// Recorder-server API token — keyring only (scrubbed from the disk
    /// snapshot exactly like the Webull secrets).
    #[serde(default)]
    pub recorder_api_token: String,
}

fn default_symbol() -> String {
    "NVDA".into()
}
fn default_strategy() -> String {
    "CSP+CC".into()
}
fn default_risk() -> String {
    "BALANCED".into()
}
fn default_webull_url() -> String {
    WEBULL_PRODUCTION_BASE_URL.into()
}

/// The only Webull REST host credentials may ever be sent to (owner
/// decision: production only — the UAT host is gone from the product).
pub const WEBULL_PRODUCTION_BASE_URL: &str = "https://api.webull.co.th";

/// Normalise `url` (trim, drop trailing `/`) and accept ONLY the production
/// host. Empty means the default. `None` = some other host — the caller must
/// reject it (typed) or fall back to the default (stored).
pub fn normalise_webull_base_url(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Some(default_webull_url());
    }
    (trimmed == WEBULL_PRODUCTION_BASE_URL).then(|| trimmed.to_string())
}

/// Credential merge for the setup screen: an EMPTY incoming secret keeps the
/// saved value (the field is left blank when one is stored), a non-empty one
/// replaces it.
pub(crate) fn merge_secret(incoming: &str, saved: &str) -> String {
    let incoming = incoming.trim();
    if incoming.is_empty() {
        saved.to_string()
    } else {
        incoming.to_string()
    }
}

/// Access-token precedence for a save/probe: typed > pending (just issued,
/// not yet saved) > saved. All inputs trimmed.
pub(crate) fn resolve_access_token(typed: &str, pending: Option<&str>, saved: &str) -> String {
    let typed = typed.trim();
    if !typed.is_empty() {
        return typed.to_string();
    }
    if let Some(pending) = pending.map(str::trim).filter(|p| !p.is_empty()) {
        return pending.to_string();
    }
    saved.trim().to_string()
}

/// The LLM block of [`super::save_credentials`]: typed fields win, blanks
/// fall back to the saved config, and the block only exists once a base URL
/// and model are set (the key may still be blank — `LlmConfig::is_ready`
/// and the `has_api_key` flag keep that honest).
pub(crate) fn merge_llm(
    saved: Option<&LlmConfig>,
    label: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
) -> Option<LlmConfig> {
    let saved = saved.cloned().unwrap_or_else(|| {
        LlmConfig::new(String::new(), String::new(), String::new(), String::new())
    });
    let label = merge_secret(label.as_deref().unwrap_or(""), &saved.label);
    let base_url = merge_secret(base_url.as_deref().unwrap_or(""), &saved.base_url);
    let model = merge_secret(model.as_deref().unwrap_or(""), &saved.model);
    let api_key = merge_secret(api_key.as_deref().unwrap_or(""), &saved.api_key);
    if base_url.is_empty() || model.is_empty() {
        return None;
    }
    Some(LlmConfig {
        label,
        base_url,
        model,
        api_key,
        ..saved
    })
}

/// [`super::save_settings`] guard: the incoming struct may come from JS, so
/// every secret field is overwritten with the current in-memory (keyring)
/// values before anything is persisted — a settings push can never inject or
/// exfiltrate credentials.
pub(crate) fn copy_saved_secrets(current: &AppSettings, mut incoming: AppSettings) -> AppSettings {
    incoming.webull_app_key = current.webull_app_key.clone();
    incoming.webull_app_secret = current.webull_app_secret.clone();
    incoming.webull_access_token = current.webull_access_token.clone();
    incoming.recorder_api_token = current.recorder_api_token.clone();
    if let Some(llm) = incoming.llm.as_mut() {
        llm.api_key = current
            .llm
            .as_ref()
            .map(|c| c.api_key.clone())
            .unwrap_or_default();
    }
    incoming
}

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
            webull_signature_scheme: SignatureScheme::default(),
            llm: None,
            lab_source: LabSource::Local,
            recorder_api_url: String::new(),
            recorder_api_token: String::new(),
        }
    }
}

/// What crosses the Tauri IPC boundary: every preference plus the non-secret
/// Webull app key, with each secret reduced to a `has_*` boolean. No command
/// may return the app secret, access token or LLM api_key values.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LlmView {
    pub label: String,
    pub base_url: String,
    pub model: String,
    pub has_api_key: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SettingsView {
    pub font_size_adjust: i32,
    pub show_math: bool,
    pub last_symbol: String,
    pub last_strategy: String,
    pub last_risk: String,
    pub last_budget: f64,
    pub webull_app_key: String,
    pub webull_base_url: String,
    pub webull_signature_scheme: SignatureScheme,
    pub has_webull_secret: bool,
    pub has_access_token: bool,
    pub llm: Option<LlmView>,
    pub lab_source: LabSource,
    pub recorder_api_url: String,
    pub has_recorder_api_token: bool,
}

impl AppSettings {
    /// Secret-free projection for the frontend (`get_settings` and friends).
    pub fn view(&self) -> SettingsView {
        SettingsView {
            font_size_adjust: self.font_size_adjust,
            show_math: self.show_math,
            last_symbol: self.last_symbol.clone(),
            last_strategy: self.last_strategy.clone(),
            last_risk: self.last_risk.clone(),
            last_budget: self.last_budget,
            webull_app_key: self.webull_app_key.clone(),
            webull_base_url: self.webull_base_url_or_default(),
            webull_signature_scheme: self.webull_signature_scheme,
            has_webull_secret: !self.webull_app_secret.trim().is_empty(),
            has_access_token: !self.webull_access_token.trim().is_empty(),
            llm: self.llm.as_ref().map(|llm| LlmView {
                label: llm.label.clone(),
                base_url: llm.base_url.clone(),
                model: llm.model.clone(),
                has_api_key: !llm.api_key.trim().is_empty(),
            }),
            lab_source: self.lab_source,
            recorder_api_url: self.recorder_api_url.clone(),
            has_recorder_api_token: !self.recorder_api_token.trim().is_empty(),
        }
    }

    /// The REST base URL in effect: the saved one when it is the allowed
    /// production host (empty or trailing-slash variants normalised), else
    /// production — a legacy file pointing somewhere else must never receive
    /// credentials.
    pub fn webull_base_url_or_default(&self) -> String {
        normalise_webull_base_url(&self.webull_base_url).unwrap_or_else(default_webull_url)
    }

    /// A REST client configured exactly like every other call of the app —
    /// saved credentials, effective base URL and the selected signing scheme —
    /// so all callers share one coordinator and one quota.
    pub fn webull_client(&self) -> Option<WebullClient> {
        let creds = self.webull_credentials()?;
        Some(
            WebullClient::new(creds)
                .with_base_url(self.webull_base_url_or_default())
                .with_signature_scheme(self.webull_signature_scheme),
        )
    }
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

    // ---------- OS keyring (Windows Credential Manager / macOS Keychain) ----------

    /// Persist with secrets in the OS keyring, failing closed: a keyring
    /// error aborts the whole save — no plaintext fallback — and leaves both
    /// the in-memory fields and any legacy on-disk file untouched. On
    /// success the in-memory copy keeps the credentials it just wrote (no
    /// blank/refill round-trip) and only a scrubbed snapshot reaches disk.
    pub fn save_with_keyring(&mut self, app_data_dir: &str) -> Result<(), String> {
        self.save_with_keyring_service(app_data_dir, KEYRING_SERVICE)
    }

    /// Test seam for [`Self::save_with_keyring`]: the keyring service is
    /// injectable so failures (e.g. an invalid service name) are
    /// deterministic and no real credential is ever at risk.
    fn save_with_keyring_service(
        &mut self,
        app_data_dir: &str,
        service: &str,
    ) -> Result<(), String> {
        let mut snapshot = self.clone();
        snapshot.scrub_secrets();
        self.persist_with_keyring(service, || snapshot.save(app_data_dir))
    }

    /// Blank every secret field (used on the disk snapshot only).
    fn scrub_secrets(&mut self) {
        self.webull_app_key = String::new();
        self.webull_app_secret = String::new();
        self.webull_access_token = String::new();
        self.recorder_api_token = String::new();
        if let Some(c) = self.llm.as_mut() {
            c.api_key = String::new();
        }
    }

    /// True when any secret is still stored in plaintext (pre-migration file).
    pub fn has_plaintext_secrets(&self) -> bool {
        !self.webull_app_key.is_empty()
            || !self.webull_app_secret.is_empty()
            || !self.webull_access_token.is_empty()
            || !self.recorder_api_token.is_empty()
            || self.llm.as_ref().is_some_and(|c| !c.api_key.is_empty())
    }

    /// Restore previous keyring values if an entry update or disk save fails.
    fn persist_with_keyring(
        &self,
        service: &str,
        save_snapshot: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let fields: [(&str, String); 5] = [
            ("webull_app_key", self.webull_app_key.clone()),
            ("webull_app_secret", self.webull_app_secret.clone()),
            ("webull_access_token", self.webull_access_token.clone()),
            ("recorder_api_token", self.recorder_api_token.clone()),
            (
                "llm_api_key",
                self.llm
                    .as_ref()
                    .map(|c| c.api_key.clone())
                    .unwrap_or_default(),
            ),
        ];
        let mut entries = Vec::with_capacity(fields.len());
        for (account, value) in &fields {
            let entry = keyring::Entry::new(service, account).map_err(|e| e.to_string())?;
            let previous = match entry.get_password() {
                Ok(password) => Some(password),
                Err(keyring::Error::NoEntry) => None,
                Err(e) => return Err(e.to_string()),
            };
            entries.push((
                entry,
                previous,
                (!value.is_empty()).then_some(value.as_str()),
            ));
        }
        let write = |entry: &keyring::Entry, value: Option<&str>| match value {
            Some(value) => entry.set_password(value),
            None => match entry.delete_credential() {
                Err(keyring::Error::NoEntry) => Ok(()),
                result => result,
            },
        };
        let mut changed = Vec::new();
        let result = (|| {
            for (entry, previous, value) in &entries {
                if previous.as_deref() == *value {
                    continue;
                }
                changed.push((entry, previous.as_deref()));
                write(entry, *value).map_err(|e| e.to_string())?;
            }
            save_snapshot()
        })();
        if result.is_err() {
            let mut rollback_failed = false;
            for (entry, previous) in changed.into_iter().rev() {
                rollback_failed |= write(entry, previous).is_err();
            }
            if rollback_failed {
                return Err(
                    "Settings save failed and some keyring entries could not be restored".into(),
                );
            }
        }
        result
    }

    /// Fill EMPTY secret fields from the OS keyring — plaintext in
    /// settings.json always wins, keyring entries are the scrubbed copy.
    pub fn pull_secrets_from_keyring(&mut self) {
        self.pull_secrets_from_keyring_service(KEYRING_SERVICE);
    }

    fn pull_secrets_from_keyring_service(&mut self, service: &str) {
        let fill = |account: &str| {
            keyring::Entry::new(service, account)
                .and_then(|e| e.get_password())
                .unwrap_or_default()
        };
        if self.webull_app_key.is_empty() {
            self.webull_app_key = fill("webull_app_key");
        }
        if self.webull_app_secret.is_empty() {
            self.webull_app_secret = fill("webull_app_secret");
        }
        if self.webull_access_token.is_empty() {
            self.webull_access_token = fill("webull_access_token");
        }
        if self.recorder_api_token.is_empty() {
            self.recorder_api_token = fill("recorder_api_token");
        }
        if self.llm.as_ref().is_some_and(|c| c.api_key.is_empty()) {
            let k = fill("llm_api_key");
            if let Some(c) = self.llm.as_mut() {
                c.api_key = k;
            }
        }
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
        let llm_name = llm
            .map(|l| format!("{} ({})", l.label, l.model))
            .unwrap_or_default();
        format!(
            "Webull: {} | LLM: {}{}",
            if webull_ok {
                "✓ connected"
            } else {
                "✗ not set"
            },
            if llm_ok { "✓ " } else { "✗ " },
            llm_name,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Windows credential store is not consistent under concurrent access
    /// from several tests (a just-written entry read back as missing in 4 of
    /// 20 parallel runs), so the tests that touch it take turns.
    static KEYRING: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn keyring_turn() -> std::sync::MutexGuard<'static, ()> {
        KEYRING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Real OS keyring roundtrip (Windows Credential Manager). Guards the
    /// migration path: if this fails locally, saving credentials fails
    /// closed (legacy plaintext files are left as the source of truth).
    #[test]
    #[cfg(windows)]
    fn keyring_roundtrip_local() {
        let _turn = keyring_turn();
        let e = keyring::Entry::new("premium-radar-test", "probe").expect("Entry::new");
        let _ = e.delete_credential();
        e.set_password("secret-probe").expect("set_password");
        assert_eq!(e.get_password().expect("get_password"), "secret-probe");
        e.delete_credential().expect("delete_credential");
    }

    #[test]
    fn plaintext_secret_detection() {
        let mut s = AppSettings::default();
        assert!(!s.has_plaintext_secrets());
        s.webull_access_token = "tok".into();
        assert!(s.has_plaintext_secrets());
        s.webull_access_token = String::new();
        s.llm = Some(LlmConfig::new("x", "http://x", "m", ""));
        assert!(!s.has_plaintext_secrets(), "empty llm key is not a secret");
        s.llm = Some(LlmConfig::new("x", "http://x", "m", "k"));
        assert!(s.has_plaintext_secrets());
    }

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
            llm: Some(LlmConfig::new(
                "Test LLM",
                "https://api.test.com",
                "test-model",
                "test-llm-key",
            )),
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

    // ---------- W2: secrets stay native ----------

    /// The IPC view carries none of the saved secret values, only `has_*`.
    #[test]
    fn settings_view_never_carries_secret_values() {
        let mut s = fake_secrets(AppSettings::default());
        s.llm = Some(LlmConfig::new(
            "L",
            "https://api.z.ai",
            "m",
            "LLM-SECRET-KEY",
        ));
        let view = s.view();
        assert_eq!(
            view.webull_app_key, "FAKE_TEST_KEY",
            "app key is shown, not hidden"
        );
        assert!(view.has_webull_secret);
        assert!(view.has_access_token);
        assert_eq!(
            view.llm.as_ref().map(|l| l.has_api_key),
            Some(true),
            "llm view flags the key without carrying it"
        );
        let json = serde_json::to_string(&view).expect("view serialises");
        assert!(!json.contains("FAKE_TEST_SECRET"));
        assert!(!json.contains("FAKE_TEST_TOKEN"));
        assert!(!json.contains("LLM-SECRET-KEY"));

        let empty = AppSettings::default().view();
        assert!(!empty.has_webull_secret);
        assert!(!empty.has_access_token);
        assert_eq!(empty.llm.map(|l| l.has_api_key), None);
    }

    #[test]
    fn only_the_production_host_is_allowed() {
        let ok = |url: &str| normalise_webull_base_url(url);
        assert_eq!(
            ok("https://api.webull.co.th").as_deref(),
            Some("https://api.webull.co.th")
        );
        assert_eq!(
            ok("https://api.webull.co.th/").as_deref(),
            Some("https://api.webull.co.th")
        );
        assert_eq!(
            ok("  https://api.webull.co.th  ").as_deref(),
            Some("https://api.webull.co.th")
        );
        assert_eq!(
            ok("").as_deref(),
            Some("https://api.webull.co.th"),
            "empty means the default"
        );
        assert_eq!(ok("http://api.webull.co.th"), None, "not https");
        assert_eq!(ok("https://th-api.uat.webullbroker.com"), None);
        assert_eq!(ok("https://api.webull.co.th.evil.com"), None);
        assert_eq!(ok("https://evil.com/https://api.webull.co.th"), None);
    }

    #[test]
    fn disallowed_stored_host_falls_back_to_production() {
        let mut s = AppSettings {
            webull_base_url: "https://th-api.uat.webullbroker.com".into(),
            ..AppSettings::default()
        };
        assert_eq!(s.webull_base_url_or_default(), WEBULL_PRODUCTION_BASE_URL);
        // allowed variants are normalised, not replaced
        s.webull_base_url = "https://api.webull.co.th/".into();
        assert_eq!(s.webull_base_url_or_default(), "https://api.webull.co.th");
    }

    // ---------- W2: Lab data source (local / recorder server) ----------

    #[test]
    fn recorder_url_normalisation() {
        let ok = |url: &str| normalise_recorder_api_url(url);
        assert_eq!(
            ok("https://recorder.example.ts.net").as_deref(),
            Some("https://recorder.example.ts.net")
        );
        assert_eq!(
            ok("  http://127.0.0.1:8787/ ").as_deref(),
            Some("http://127.0.0.1:8787")
        );
        assert_eq!(ok("https://host/").as_deref(), Some("https://host"));
        assert_eq!(ok("https://host/sub/").as_deref(), Some("https://host/sub"));
        assert_eq!(ok("").as_deref(), None, "empty means not configured");
        assert_eq!(ok("recorder.example.ts.net").as_deref(), None, "no scheme");
        assert_eq!(ok("ftp://host").as_deref(), None);
        assert_eq!(ok("https://").as_deref(), None, "empty host");
        assert_eq!(
            ok("http:///path").as_deref(),
            None,
            "empty host before path"
        );
        assert_eq!(
            ok("https://evil.example/http://host").as_deref(),
            None,
            "embedded second scheme confuses nothing"
        );
    }

    /// Plain http is only tolerated for localhost or Tailscale CGNAT hosts;
    /// the UI shows the encryption warning for everything else.
    #[test]
    fn plain_http_is_private_only_for_loopback_and_tailscale() {
        let private = |url: &str| recorder_host_is_private(url);
        assert!(private("http://localhost:8787"));
        assert!(private("http://LOCALHOST"));
        assert!(private("http://127.0.0.1:8787"));
        assert!(private("http://127.8.9.10"));
        assert!(private("http://[::1]:8787"));
        assert!(private("http://100.64.0.1"));
        assert!(private("http://100.101.102.103"));
        assert!(private("http://100.127.255.255"));
        assert!(!private("http://100.63.0.1"), "below the CGNAT range");
        assert!(!private("http://100.128.0.1"), "above the CGNAT range");
        assert!(!private("http://99.1.1.1"));
        assert!(
            !private("http://192.168.1.10"),
            "LAN is not in the allowed set"
        );
        assert!(!private("http://recorder.example.ts.net"));
        assert!(!private("garbage"));
    }

    /// Old settings files (no lab fields) load as local with nothing
    /// configured; an unknown `lab_source` value also loads as local without
    /// failing the whole file.
    #[test]
    fn old_settings_without_lab_fields_load_as_local() {
        let legacy: AppSettings =
            serde_json::from_str(r#"{"last_symbol":"TSLA","webull_app_key":"k"}"#).unwrap();
        assert_eq!(legacy.lab_source, LabSource::Local);
        assert_eq!(legacy.recorder_api_url, "");
        assert_eq!(legacy.recorder_api_token, "");

        let unknown: AppSettings = serde_json::from_str(r#"{"lab_source":"banana"}"#).unwrap();
        assert_eq!(unknown.lab_source, LabSource::Local);

        let api: AppSettings = serde_json::from_str(r#"{"lab_source":"api"}"#).unwrap();
        assert_eq!(api.lab_source, LabSource::Api);
        assert_eq!(parse_lab_source("api"), Some(LabSource::Api));
        assert_eq!(parse_lab_source(" local "), Some(LabSource::Local));
        assert_eq!(parse_lab_source("cloud"), None);
        assert_eq!(LabSource::Local.as_str(), "local");
    }

    /// The IPC view carries the source and the URL but only a `has_*` flag for
    /// the token — the value never crosses the boundary.
    #[test]
    fn settings_view_carries_lab_fields_but_never_the_token() {
        let s = AppSettings {
            lab_source: LabSource::Api,
            recorder_api_url: "https://recorder.example.ts.net".into(),
            recorder_api_token: "FAKE_RECORDER_TOKEN".into(),
            ..AppSettings::default()
        };
        let view = s.view();
        assert_eq!(view.lab_source, LabSource::Api);
        assert_eq!(view.recorder_api_url, "https://recorder.example.ts.net");
        assert!(view.has_recorder_api_token);
        let json = serde_json::to_string(&view).expect("view serialises");
        assert!(!json.contains("FAKE_RECORDER_TOKEN"));

        let empty = AppSettings::default().view();
        assert_eq!(empty.lab_source, LabSource::Local);
        assert_eq!(empty.recorder_api_url, "");
        assert!(!empty.has_recorder_api_token);
    }

    #[test]
    fn save_settings_never_persists_the_recorder_token_from_js() {
        let current = AppSettings {
            recorder_api_token: "SAVED-TOKEN".into(),
            ..AppSettings::default()
        };
        let incoming = AppSettings {
            recorder_api_token: "JS-TOKEN".into(),
            ..AppSettings::default()
        };
        let guarded = copy_saved_secrets(&current, incoming);
        assert_eq!(guarded.recorder_api_token, "SAVED-TOKEN");
    }

    /// Empty incoming keeps the saved secret; non-empty replaces it; the
    /// access token resolves typed > pending > saved.
    #[test]
    fn credential_merge_empty_keeps_saved_and_token_prefers_typed() {
        assert_eq!(merge_secret("", "SAVED"), "SAVED");
        assert_eq!(merge_secret("   ", "SAVED"), "SAVED");
        assert_eq!(merge_secret("  NEW  ", "SAVED"), "NEW");
        assert_eq!(
            resolve_access_token("TYPED", Some("PENDING"), "SAVED"),
            "TYPED"
        );
        assert_eq!(
            resolve_access_token("", Some("PENDING"), "SAVED"),
            "PENDING"
        );
        assert_eq!(resolve_access_token("", None, "SAVED"), "SAVED");
        assert_eq!(resolve_access_token("", Some("  "), "SAVED"), "SAVED");
        assert_eq!(resolve_access_token("", None, " SAVED "), "SAVED");

        // the LLM block still saves when only the api key is left blank
        let saved = LlmConfig::new("Old", "https://old", "old-model", "OLD-KEY");
        let merged = merge_llm(
            Some(&saved),
            Some("New".into()),
            Some("https://new".into()),
            Some("new-model".into()),
            None,
        )
        .expect("block survives a blank key");
        assert_eq!(merged.api_key, "OLD-KEY");
        assert_eq!(merged.base_url, "https://new");
        assert!(merge_llm(None, None, None, None, None).is_none());
        assert!(merge_llm(None, Some("l".into()), None, None, None).is_none());
    }

    /// save_settings may take a full struct from JS — the secrets on it must
    /// be overwritten with the current in-memory values before persisting.
    #[test]
    fn save_settings_never_persists_secrets_from_js() {
        let current = fake_secrets(AppSettings::default());
        let incoming = AppSettings {
            webull_app_key: "JS-KEY".into(),
            webull_app_secret: "JS-SECRET".into(),
            webull_access_token: "JS-TOKEN".into(),
            llm: Some(LlmConfig::new("j", "https://js", "m", "JS-LLM-KEY")),
            ..AppSettings::default()
        };
        let guarded = copy_saved_secrets(&current, incoming);
        assert_eq!(guarded.webull_app_key, "FAKE_TEST_KEY");
        assert_eq!(guarded.webull_app_secret, "FAKE_TEST_SECRET");
        assert_eq!(guarded.webull_access_token, "FAKE_TEST_TOKEN");
        assert_eq!(
            guarded.llm.as_ref().map(|c| c.api_key.as_str()),
            Some(""),
            "no saved llm → empty key, never the JS one"
        );
    }

    // ---------- keyring migration regression (fake credentials only) ----------

    const KEYRING_ACCOUNTS: [&str; 5] = [
        "webull_app_key",
        "webull_app_secret",
        "webull_access_token",
        "recorder_api_token",
        "llm_api_key",
    ];

    fn fake_secrets(mut s: AppSettings) -> AppSettings {
        s.webull_app_key = "FAKE_TEST_KEY".into();
        s.webull_app_secret = "FAKE_TEST_SECRET".into();
        s.webull_access_token = "FAKE_TEST_TOKEN".into();
        s
    }

    fn temp_dir(name: &str) -> String {
        let dir = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.to_string_lossy().into_owned()
    }

    fn drop_service_entries(service: &str) {
        for acct in KEYRING_ACCOUNTS {
            if let Ok(e) = keyring::Entry::new(service, acct) {
                let _ = e.delete_credential();
            }
        }
    }

    #[test]
    #[cfg(windows)]
    fn keyring_restores_previous_entries_when_snapshot_fails() {
        let _turn = keyring_turn();
        let service = "premium-radar-settings-rollback-test";
        drop_service_entries(service);
        let entry = keyring::Entry::new(service, "webull_app_key").unwrap();
        entry.set_password("OLDER_FAKE_KEY").unwrap();
        let settings = fake_secrets(AppSettings::default());
        let result = settings.persist_with_keyring(service, || Err("snapshot failed".into()));
        let restored = entry.get_password();
        let added = keyring::Entry::new(service, "webull_app_secret")
            .unwrap()
            .get_password();
        drop_service_entries(service);
        assert_eq!(result, Err("snapshot failed".into()));
        assert_eq!(restored.unwrap(), "OLDER_FAKE_KEY");
        assert!(matches!(added, Err(keyring::Error::NoEntry)));
    }

    /// Keyring write fails deterministically (service name far beyond the
    /// credential-manager target-name limit): the save must return a safe
    /// error WITHOUT writing plaintext and WITHOUT discarding the legacy
    /// on-disk file or the in-memory secrets.
    #[test]
    #[cfg(windows)]
    fn keyring_failure_preserves_legacy_and_never_writes_plaintext() {
        let _turn = keyring_turn();
        let dir = temp_dir("radar-keyring-fail-test");
        let mut legacy = fake_secrets(AppSettings::default());
        legacy.last_symbol = "TSLA".into();
        legacy.save(&dir).unwrap(); // pre-migration plaintext file

        let mut s = legacy.clone();
        s.last_symbol = "AAPL".into();
        let broken_service = "x".repeat(100_000);
        let err = s
            .save_with_keyring_service(&dir, &broken_service)
            .unwrap_err();
        assert!(!err.is_empty());
        assert!(
            !err.contains("FAKE_TEST_SECRET"),
            "error must not leak the secret"
        );

        // in-memory secrets survive — runtime credentials not discarded
        assert_eq!(s.webull_app_key, "FAKE_TEST_KEY");
        assert_eq!(s.webull_app_secret, "FAKE_TEST_SECRET");
        assert!(s.webull_credentials().is_some());

        // legacy on-disk file untouched — still the plaintext source
        let disk = AppSettings::load(&dir);
        assert!(disk.has_plaintext_secrets());
        assert_eq!(disk.webull_app_secret, "FAKE_TEST_SECRET");
        assert_eq!(disk.last_symbol, "TSLA", "mutation must not reach disk");

        drop_service_entries(&broken_service);
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Happy-path migration against a dedicated test service: runtime
    /// credentials stay effective in memory while disk holds only a
    /// sanitized snapshot (non-secret preferences kept).
    #[test]
    #[cfg(windows)]
    fn keyring_success_keeps_runtime_creds_and_sanitizes_disk() {
        const SVC: &str = "premium-radar-settings-test";
        let _turn = keyring_turn();
        drop_service_entries(SVC);

        let dir = temp_dir("radar-keyring-ok-test");
        let mut s = fake_secrets(AppSettings::default());
        s.last_symbol = "TSLA".into();
        s.save_with_keyring_service(&dir, SVC).unwrap();

        // runtime credentials still effective after the move
        assert_eq!(s.webull_app_key, "FAKE_TEST_KEY");
        assert_eq!(s.webull_app_secret, "FAKE_TEST_SECRET");
        let creds = s.webull_credentials().expect("runtime creds");
        assert_eq!(creds.app_secret, "FAKE_TEST_SECRET");
        assert_eq!(creds.access_token.as_deref(), Some("FAKE_TEST_TOKEN"));

        // disk snapshot sanitized, non-secret prefs kept
        let disk = AppSettings::load(&dir);
        assert!(!disk.has_plaintext_secrets());
        assert_eq!(disk.last_symbol, "TSLA");

        drop_service_entries(SVC);
        fs::remove_dir_all(&dir).unwrap();
    }
}
