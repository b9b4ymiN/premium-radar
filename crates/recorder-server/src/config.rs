//! Config ของ recorder-server — JSON file (default `/data/config.json` หรือ
//! env `RECORDER_CONFIG`) สำหรับค่าทั่วไป + env เฉพาะ secrets
//! (`WEBULL_APP_KEY` / `WEBULL_APP_SECRET` / `WEBULL_ACCESS_TOKEN` หรือไฟล์
//! `<data_dir>/secrets/access_token`) — ไฟล์หาย = ใช้ default ทั้งหมด

use std::path::{Path, PathBuf};

use radar_core::webull::rest::Credentials;

pub const DEFAULT_DATA_DIR: &str = "/data";

/// เวลาในหนึ่งวันแบบ "HH:MM" — ใช้เป็น slot ของ scheduler
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeOfDay {
    pub hour: u32,
    pub minute: u32,
}

impl TimeOfDay {
    pub fn parse(s: &str) -> Option<Self> {
        let (h, m) = s.split_once(':')?;
        let hour = h.trim().parse().ok()?;
        let minute = m.trim().parse().ok()?;
        if hour > 23 || minute > 59 {
            return None;
        }
        Some(Self { hour, minute })
    }

    pub fn as_str(self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}

/// Config เต็มของ server — ทุก field มี default ตาม product decision
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub data_dir: PathBuf,
    pub core_symbols: Vec<String>,
    pub retention_days: usize,
    pub layer1_time_ny: TimeOfDay,
    pub layer2_core_times_ny: Vec<TimeOfDay>,
    pub layer2_universe_time_ny: TimeOfDay,
    pub max_requests_per_minute: u32,
    pub webull_base_url: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from(DEFAULT_DATA_DIR),
            core_symbols: vec!["NVDA".into()],
            retention_days: 400,
            layer1_time_ny: TimeOfDay {
                hour: 16,
                minute: 30,
            },
            layer2_core_times_ny: vec![
                TimeOfDay {
                    hour: 10,
                    minute: 0,
                },
                TimeOfDay {
                    hour: 12,
                    minute: 45,
                },
                TimeOfDay {
                    hour: 15,
                    minute: 30,
                },
            ],
            layer2_universe_time_ny: TimeOfDay {
                hour: 12,
                minute: 0,
            },
            max_requests_per_minute: 250,
            webull_base_url: None,
        }
    }
}

/// ตัดช่องว่าง + พิมพ์ใหญ่ + ตัดซ้ำ คงลำดับเดิม
pub fn normalize_symbols(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in raw {
        let up = s.trim().to_uppercase();
        if !up.is_empty() && !out.contains(&up) {
            out.push(up);
        }
    }
    out
}

/// โครง JSON ตรงกับ config file — field ไหนไม่มีใช้ default ของ field นั้น
#[derive(serde::Deserialize)]
struct RawConfig {
    #[serde(default)]
    data_dir: Option<String>,
    #[serde(default)]
    core_symbols: Option<Vec<String>>,
    #[serde(default)]
    retention_days: Option<usize>,
    #[serde(default)]
    layer1_time_ny: Option<String>,
    #[serde(default)]
    layer2_core_times_ny: Option<Vec<String>>,
    #[serde(default)]
    layer2_universe_time_ny: Option<String>,
    #[serde(default)]
    max_requests_per_minute: Option<u32>,
    #[serde(default)]
    webull_base_url: Option<String>,
}

/// อ่านเวลาหลายตัว — ตัวที่พังถูกข้ามพร้อม warning, พังหมด = default
fn parse_times(
    raw: Option<Vec<String>>,
    defaults: Vec<TimeOfDay>,
    field: &str,
    warnings: &mut Vec<String>,
) -> Vec<TimeOfDay> {
    let Some(list) = raw else { return defaults };
    let mut out = Vec::new();
    let mut bad = Vec::new();
    for s in &list {
        match TimeOfDay::parse(s) {
            Some(t) => out.push(t),
            None => bad.push(s.clone()),
        }
    }
    if !bad.is_empty() {
        warnings.push(format!("config {field}: ข้ามเวลาที่อ่านไม่ได้ {bad:?}"));
    }
    if out.is_empty() {
        warnings.push(format!("config {field}: ไม่มีเวลาที่ใช้ได้เลย — กลับไปใช้ default"));
        defaults
    } else {
        out
    }
}

/// อ่านเวลาเดี่ยว — พัง = default + warning
fn parse_one(
    raw: Option<String>,
    default: TimeOfDay,
    field: &str,
    warnings: &mut Vec<String>,
) -> TimeOfDay {
    match raw {
        None => default,
        Some(s) => match TimeOfDay::parse(&s) {
            Some(t) => t,
            None => {
                warnings.push(format!(
                    "config {field}: '{s}' อ่านไม่ได้ — ใช้ {}",
                    default.as_str()
                ));
                default
            }
        },
    }
}

impl Config {
    /// แปลง JSON ดิบเป็น config — field ที่ parse ไม่ผ่าน (เช่น "9:5x") ทิ้ง
    /// กลับไปใช้ default พร้อม warning ให้เจ้าของเห็นตอนรัน
    fn from_raw(raw: RawConfig, mut warnings: Vec<String>) -> (Self, Vec<String>) {
        let d = Self::default();
        let core = match raw.core_symbols {
            None => d.core_symbols,
            Some(list) => {
                let norm = normalize_symbols(&list);
                if norm.is_empty() {
                    warnings.push("config core_symbols: ว่างหลัง normalize — ใช้ default".into());
                    d.core_symbols
                } else {
                    norm
                }
            }
        };
        let max = raw
            .max_requests_per_minute
            .unwrap_or(d.max_requests_per_minute);
        if max == 0 {
            warnings.push("config max_requests_per_minute: 0 ไม่มีความหมาย — ใช้ default".into());
        }
        (
            Self {
                data_dir: raw
                    .data_dir
                    .filter(|s| !s.trim().is_empty())
                    .map(PathBuf::from)
                    .unwrap_or(d.data_dir),
                core_symbols: core,
                retention_days: raw.retention_days.unwrap_or(d.retention_days),
                layer1_time_ny: parse_one(
                    raw.layer1_time_ny,
                    d.layer1_time_ny,
                    "layer1_time_ny",
                    &mut warnings,
                ),
                layer2_core_times_ny: parse_times(
                    raw.layer2_core_times_ny,
                    d.layer2_core_times_ny,
                    "layer2_core_times_ny",
                    &mut warnings,
                ),
                layer2_universe_time_ny: parse_one(
                    raw.layer2_universe_time_ny,
                    d.layer2_universe_time_ny,
                    "layer2_universe_time_ny",
                    &mut warnings,
                ),
                max_requests_per_minute: if max == 0 {
                    d.max_requests_per_minute
                } else {
                    max
                },
                webull_base_url: raw
                    .webull_base_url
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
            },
            warnings,
        )
    }

    /// อ่าน config จาก path — ไฟล์หาย/พัง = default ทั้งหมด + warning
    pub fn load_from(path: &Path) -> (Self, Vec<String>) {
        match std::fs::read_to_string(path) {
            Ok(raw) => match serde_json::from_str::<RawConfig>(&raw) {
                Ok(parsed) => Self::from_raw(parsed, Vec::new()),
                Err(e) => (
                    Self::default(),
                    vec![format!(
                        "config {}: parse ไม่ผ่าน ({e}) — ใช้ default ทั้งหมด",
                        path.display()
                    )],
                ),
            },
            Err(_) => (
                Self::default(),
                vec![format!(
                    "config {}: ไม่พบไฟล์ — ใช้ default ทั้งหมด",
                    path.display()
                )],
            ),
        }
    }

    /// Path ของ LabStorage คือ `<data_dir>/lab` เสมอ
    pub fn lab_dir(&self) -> PathBuf {
        self.data_dir.join("lab")
    }

    /// Path ของ universe cache ที่ task อื่นเขียน
    pub fn universe_path(&self) -> PathBuf {
        self.data_dir.join("universe.json")
    }

    /// Path ของ status ที่เขียนหลังจบทุก job
    pub fn status_path(&self) -> PathBuf {
        self.data_dir.join("status.json")
    }

    /// Path ของ token file (ไม่บังคับว่าจะมี)
    pub fn token_file(&self) -> PathBuf {
        self.data_dir.join("secrets").join("access_token")
    }

    /// Path ของ API token file (`RECORDER_API_TOKEN` env ชนะไฟล์นี้)
    pub fn api_token_file(&self) -> PathBuf {
        self.data_dir.join("secrets").join("api_token")
    }

    /// สรุปแบบ log ได้ (ไม่มี secret อยู่ดี)
    pub fn summary(&self) -> String {
        format!(
            "data_dir={} core={:?} retention={}d L1@{} L2core@{:?} L2univ@{} budget={}req/min base_url={}",
            self.data_dir.display(),
            self.core_symbols,
            self.retention_days,
            self.layer1_time_ny.as_str(),
            self.layer2_core_times_ny
                .iter()
                .map(|t| t.as_str())
                .collect::<Vec<_>>(),
            self.layer2_universe_time_ny.as_str(),
            self.max_requests_per_minute,
            self.webull_base_url.as_deref().unwrap_or("(default)")
        )
    }
}

/// Path ของ config file: env `RECORDER_CONFIG` หรือ `/data/config.json`
pub fn config_path() -> PathBuf {
    std::env::var("RECORDER_CONFIG")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(DEFAULT_DATA_DIR).join("config.json"))
}

/// โหลด config จาก path ที่ resolve แล้ว
pub fn load() -> (Config, Vec<String>) {
    let path = config_path();
    Config::load_from(&path)
}

/// Secrets ที่โหลดพร้อมรันแล้ว — ไม่ log ค่าจริง (ใช้ `masked()`)
#[derive(Clone)]
pub struct Secrets {
    pub creds: Credentials,
    pub access_token: String,
    /// "env" หรือ "file" — บอกว่า token มาจากไหน
    pub token_source: &'static str,
}

/// เลือก access token: env ชนะไฟล์เสมอ; ไฟล์อ่านแล้ว trim
pub fn resolve_access_token(
    env_token: Option<String>,
    token_file: &Path,
) -> Option<(String, &'static str)> {
    if let Some(t) = env_token.filter(|s| !s.trim().is_empty()) {
        return Some((t.trim().to_string(), "env"));
    }
    std::fs::read_to_string(token_file)
        .ok()
        .map(|raw| (raw.trim().to_string(), "file"))
        .filter(|(t, _)| !t.is_empty())
}

/// โหลด credentials จาก env + token จาก env/ไฟล์ — ใช้ตอน `run`/`once`
pub fn load_secrets(cfg: &Config) -> Result<Secrets, String> {
    let key = std::env::var("WEBULL_APP_KEY")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or("ไม่มี WEBULL_APP_KEY (env)")?;
    let secret = std::env::var("WEBULL_APP_SECRET")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or("ไม่มี WEBULL_APP_SECRET (env)")?;
    let env_token = std::env::var("WEBULL_ACCESS_TOKEN").ok();
    let Some((token, source)) = resolve_access_token(env_token, &cfg.token_file()) else {
        return Err(format!(
            "ไม่มี access token — ตั้ง WEBULL_ACCESS_TOKEN หรือวางไฟล์ {}",
            cfg.token_file().display()
        ));
    };
    Ok(Secrets {
        creds: Credentials {
            app_key: key,
            app_secret: secret,
            access_token: Some(token.clone()),
        },
        access_token: token,
        token_source: source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_cfg(dir: &std::path::Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn time_of_day_parse_and_rejects() {
        assert_eq!(
            TimeOfDay::parse("16:30"),
            Some(TimeOfDay {
                hour: 16,
                minute: 30
            })
        );
        assert_eq!(
            TimeOfDay::parse(" 9:05 "),
            Some(TimeOfDay { hour: 9, minute: 5 })
        );
        assert_eq!(TimeOfDay::parse("24:00"), None);
        assert_eq!(TimeOfDay::parse("12:60"), None);
        assert_eq!(TimeOfDay::parse("1230"), None);
        assert_eq!(TimeOfDay::parse(""), None);
    }

    #[test]
    fn defaults_match_product_decision() {
        let d = Config::default();
        assert_eq!(d.data_dir, PathBuf::from("/data"));
        assert_eq!(d.core_symbols, vec!["NVDA"]);
        assert_eq!(d.retention_days, 400);
        assert_eq!(d.layer1_time_ny.as_str(), "16:30");
        assert_eq!(
            d.layer2_core_times_ny
                .iter()
                .map(|t| t.as_str())
                .collect::<Vec<_>>(),
            vec!["10:00", "12:45", "15:30"]
        );
        assert_eq!(d.layer2_universe_time_ny.as_str(), "12:00");
        assert_eq!(d.max_requests_per_minute, 250);
        assert!(d.webull_base_url.is_none());
        assert_eq!(d.lab_dir(), PathBuf::from("/data/lab"));
        assert_eq!(d.universe_path(), PathBuf::from("/data/universe.json"));
        assert_eq!(d.status_path(), PathBuf::from("/data/status.json"));
    }

    #[test]
    fn missing_file_gives_defaults_with_warning() {
        let (cfg, warnings) = Config::load_from(Path::new("/nonexistent/config.json"));
        assert_eq!(cfg, Config::default());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("ไม่พบไฟล์"));
    }

    #[test]
    fn json_overrides_and_partial_fallback() {
        let tmp = std::env::temp_dir().join(format!("recorder-server-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let full = write_cfg(
            &tmp,
            "full.json",
            r#"{
                "data_dir": "D:/labdata",
                "core_symbols": ["nvda ", "TSLA", "nvda"],
                "retention_days": 10,
                "layer1_time_ny": "17:00",
                "layer2_core_times_ny": ["09:30", "14:00"],
                "layer2_universe_time_ny": "11:00",
                "max_requests_per_minute": 100,
                "webull_base_url": "https://uat.example"
            }"#,
        );
        let (cfg, warnings) = Config::load_from(&full);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(cfg.data_dir, PathBuf::from("D:/labdata"));
        assert_eq!(cfg.core_symbols, vec!["NVDA", "TSLA"]); // trim + dedupe
        assert_eq!(cfg.retention_days, 10);
        assert_eq!(cfg.layer1_time_ny.as_str(), "17:00");
        assert_eq!(
            cfg.layer2_core_times_ny
                .iter()
                .map(|t| t.as_str())
                .collect::<Vec<_>>(),
            vec!["09:30", "14:00"]
        );
        assert_eq!(cfg.layer2_universe_time_ny.as_str(), "11:00");
        assert_eq!(cfg.max_requests_per_minute, 100);
        assert_eq!(cfg.webull_base_url.as_deref(), Some("https://uat.example"));
        assert_eq!(
            cfg.universe_path(),
            PathBuf::from("D:/labdata/universe.json")
        );

        // partial: field ที่ไม่มีคง default
        let partial = write_cfg(&tmp, "partial.json", r#"{"retention_days": 7}"#);
        let (cfg2, warnings2) = Config::load_from(&partial);
        assert!(warnings2.is_empty());
        assert_eq!(cfg2.retention_days, 7);
        assert_eq!(cfg2.core_symbols, vec!["NVDA"]);
        assert_eq!(cfg2.layer1_time_ny.as_str(), "16:30");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn malformed_time_falls_back_with_warning() {
        let tmp =
            std::env::temp_dir().join(format!("recorder-server-cfgbad-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let p = write_cfg(
            &tmp,
            "bad.json",
            r#"{"layer1_time_ny": "bananas", "layer2_core_times_ny": ["ok:00" ]}"#,
        );
        let (cfg, warnings) = Config::load_from(&p);
        assert_eq!(cfg.layer1_time_ny.as_str(), "16:30"); // default
        assert!(warnings.iter().any(|w| w.contains("layer1_time_ny")));
        assert_eq!(
            cfg.layer2_core_times_ny,
            Config::default().layer2_core_times_ny
        );
        assert!(warnings.iter().any(|w| w.contains("layer2_core_times_ny")));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn token_env_beats_file_and_file_is_trimmed() {
        let tmp =
            std::env::temp_dir().join(format!("recorder-server-token-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("access_token");
        std::fs::write(&file, "  from-file-token \n").unwrap();

        // ไม่มี env → ไฟล์ + trim
        let from_file = resolve_access_token(None, &file).unwrap();
        assert_eq!(from_file, ("from-file-token".to_string(), "file"));

        // มี env → env ชนะ
        let from_env = resolve_access_token(Some("from-env".into()), &file).unwrap();
        assert_eq!(from_env, ("from-env".to_string(), "env"));

        // ไม่มีทั้งคู่ → None
        assert!(resolve_access_token(None, &tmp.join("nope")).is_none());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
