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
    "https://api.webull.co.th".into()
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
        }
    }
}

impl AppSettings {
    /// The REST base URL in effect: the saved one, else production.
    pub fn webull_base_url_or_default(&self) -> String {
        let url = self.webull_base_url.trim();
        if url.is_empty() {
            default_webull_url()
        } else {
            url.to_string()
        }
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
        if let Some(c) = self.llm.as_mut() {
            c.api_key = String::new();
        }
    }

    /// True when any secret is still stored in plaintext (pre-migration file).
    pub fn has_plaintext_secrets(&self) -> bool {
        !self.webull_app_key.is_empty()
            || !self.webull_app_secret.is_empty()
            || !self.webull_access_token.is_empty()
            || self.llm.as_ref().is_some_and(|c| !c.api_key.is_empty())
    }

    /// Restore previous keyring values if an entry update or disk save fails.
    fn persist_with_keyring(
        &self,
        service: &str,
        save_snapshot: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let fields: [(&str, String); 4] = [
            ("webull_app_key", self.webull_app_key.clone()),
            ("webull_app_secret", self.webull_app_secret.clone()),
            ("webull_access_token", self.webull_access_token.clone()),
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

    /// Real OS keyring roundtrip (Windows Credential Manager). Guards the
    /// migration path: if this fails locally, saving credentials fails
    /// closed (legacy plaintext files are left as the source of truth).
    #[test]
    #[cfg(windows)]
    fn keyring_roundtrip_local() {
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

    // ---------- keyring migration regression (fake credentials only) ----------

    const KEYRING_ACCOUNTS: [&str; 4] = [
        "webull_app_key",
        "webull_app_secret",
        "webull_access_token",
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
