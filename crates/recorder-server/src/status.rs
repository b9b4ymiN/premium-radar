//! status.json — ไฟล์สถานะเล็ก ๆ ที่ `<data_dir>/status.json` อัปเดตหลัง
//! จบทุก job (task ถัดไปจะเปิดอ่านผ่าน HTTP)

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LayerStats {
    #[serde(default)]
    pub jobs: u64,
    #[serde(default)]
    pub symbols_recorded: u64,
    #[serde(default)]
    pub symbols_skipped: u64,
    #[serde(default)]
    pub symbols_failed: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastJob {
    pub kind: String,
    pub layer: u8,
    pub scheduled_for_ny: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub symbols: usize,
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenInfo {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_job: Option<LastJob>,
    #[serde(default)]
    pub layer1: LayerStats,
    #[serde(default)]
    pub layer2: LayerStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<TokenInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

/// อ่าน status เดิม — ไฟล์หาย/พัง = เริ่มนับใหม่ (ไฟล์นี้ไม่ใช่ source of truth)
pub fn load(path: &Path) -> Status {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// เขียน status แบบ pretty — สร้าง parent dir ให้ถ้ายังไม่มี
pub fn save(path: &Path, status: &Status) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(
        path,
        serde_json::to_string_pretty(status).unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_keeps_counts_and_token() {
        let dir =
            std::env::temp_dir().join(format!("recorder-server-status-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("status.json");
        let _ = std::fs::remove_file(&path);

        // ไฟล์ยังไม่มี = default
        assert_eq!(load(&path).layer1.jobs, 0);

        let mut st = load(&path);
        st.layer1.jobs = 3;
        st.layer1.symbols_recorded = 12;
        st.token = Some(TokenInfo {
            status: "NORMAL".into(),
            expires_at: Some("2026-12-05T00:00:00Z".parse().unwrap()),
        });
        st.last_job = Some(LastJob {
            kind: "layer1".into(),
            layer: 1,
            scheduled_for_ny: "2026-11-24T16:30:00-05:00".into(),
            started_at: "2026-11-24T21:30:00Z".parse().unwrap(),
            finished_at: "2026-11-24T21:41:00Z".parse().unwrap(),
            symbols: 4,
            outcome: "recorded=3 skipped=0 failed=1".into(),
        });
        save(&path, &st).unwrap();

        let back = load(&path);
        assert_eq!(back.layer1.jobs, 3);
        assert_eq!(back.layer1.symbols_recorded, 12);
        assert_eq!(back.token.as_ref().unwrap().status, "NORMAL");
        assert_eq!(back.last_job.as_ref().unwrap().kind, "layer1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_resets_to_default() {
        let dir =
            std::env::temp_dir().join(format!("recorder-server-statusbad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("status.json");
        std::fs::write(&path, "}{ broken").unwrap();
        assert_eq!(load(&path), Status::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
