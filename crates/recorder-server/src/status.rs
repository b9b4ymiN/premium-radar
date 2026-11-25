//! status.json — ไฟล์สถานะเล็ก ๆ ที่ `<data_dir>/status.json` อัปเดตหลัง
//! จบทุก job + บันทึก slot ที่รันแล้ว (กันรันซ้ำหลัง restart) — task
//! ถัดไปจะเปิดอ่านผ่าน HTTP

use std::collections::HashSet;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
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

/// slot ที่รัน (หรือพยายามรัน) ไปแล้ว — id คือ key ของ slot
/// เช่น "2026-11-25/layer2-core/10:00" เก็บเวลาจบไว้เพื่อ prune
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletedSlot {
    pub id: String,
    pub finished_at: DateTime<Utc>,
}

/// เก็บ slot id เท่านั้น ~7 วันล่าสุด (ครบวันจะถูก prune ทิ้ง)
pub const SLOT_RETENTION: Duration = Duration::days(7);

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
    /// slot ที่รันแล้ว (restart ไม่รันซ้ำ) — เก่ากว่า 7 วันถูกตัดตอน mark
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed_slots: Vec<CompletedSlot>,
}

impl Status {
    /// บันทึก slot id (แทนที่อันเดิมถ้าซ้ำ) แล้ว prune เหลือ ~7 วันล่าสุด
    pub fn mark_slot_done(&mut self, id: &str, finished_at: DateTime<Utc>) {
        self.completed_slots.retain(|s| s.id != id);
        self.completed_slots.push(CompletedSlot {
            id: id.to_string(),
            finished_at,
        });
        self.prune_slots(finished_at);
    }

    /// ตัด slot ที่เก่ากว่า [`SLOT_RETENTION`] นับจาก `now`
    pub fn prune_slots(&mut self, now: DateTime<Utc>) {
        let cutoff = now - SLOT_RETENTION;
        self.completed_slots.retain(|s| s.finished_at >= cutoff);
    }

    /// id ของ slot ที่บันทึกไว้ทั้งหมด (สำหรับ seed DayState ตอน startup)
    pub fn completed_slot_ids(&self) -> HashSet<String> {
        self.completed_slots.iter().map(|s| s.id.clone()).collect()
    }
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

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn done(st: &Status, id: &str) -> bool {
        st.completed_slot_ids().contains(id)
    }

    #[test]
    fn slot_done_marks_prunes_and_dedupes() {
        let mut st = Status::default();
        assert!(!done(&st, "2026-11-24/layer1/16:30"));

        let now = t("2026-11-24T22:00:00Z");
        st.mark_slot_done("2026-11-24/layer1/16:30", now);
        st.mark_slot_done("2026-11-24/layer2-core/10:00", now);
        assert!(done(&st, "2026-11-24/layer1/16:30"));
        assert_eq!(st.completed_slots.len(), 2);

        // id เดิม = แทนที่ ไม่เพิ่มซ้ำ
        st.mark_slot_done("2026-11-24/layer1/16:30", now);
        assert_eq!(st.completed_slots.len(), 2);

        // เก่ากว่า 7 วันถูกตัดตอน mark
        st.mark_slot_done("2026-12-01/layer1/16:30", t("2026-12-02T22:00:00Z"));
        assert!(!done(&st, "2026-11-24/layer1/16:30"));
        assert!(done(&st, "2026-12-01/layer1/16:30"));
    }

    /// restart จำลอง: mark slot → save → load → seed DayState ใหม่ →
    /// next_due ต้องข้าม slot ที่บันทึกไว้ (ไม่รันซ้ำวันเดิม)
    #[test]
    fn completed_slots_survive_restart_and_prevent_rerun() {
        let dir =
            std::env::temp_dir().join(format!("recorder-server-restart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("status.json");

        // "เซสชันแรก" — รัน slot แรกของอังคาร 24 พ.ย. แล้วเขียน status
        let tue = chrono::NaiveDate::from_ymd_opt(2026, 11, 24).unwrap();
        let mut st = load(&path);
        let first_slot = crate::schedule::slots_on(tue, &crate::config::Config::default())
            .into_iter()
            .next()
            .unwrap();
        st.mark_slot_done(&first_slot.key, t("2026-11-24T15:05:00Z"));
        save(&path, &st).unwrap();

        // "restart" — โหลดใหม่ตอน 13:00 NY slot ค้างทั้งวัน
        let reloaded = load(&path);
        assert!(done(&reloaded, &first_slot.key));
        let mut state = crate::schedule::DayState::with_done(tue, reloaded.completed_slot_ids());

        // next_due ต้องข้าม slot ที่บันทึกไว้ ไป slot ถัดไป (12:00 universe)
        let late = t("2026-11-24T18:00:00Z"); // 13:00 NY
        let next = state.next_due(late, &crate::config::Config::default());
        assert_ne!(next.key, first_slot.key);
        assert_eq!(next.kind, crate::schedule::JobKind::Layer2Universe);

        // ของเมื่อวาน (เก็บไว้ < 7 วัน) ต้องไม่กัน slot ของวันนี้
        let mut st2 = Status::default();
        st2.mark_slot_done("2026-11-23/layer1/16:30", t("2026-11-23T22:00:00Z"));
        let ids = st2.completed_slot_ids();
        assert!(ids.contains("2026-11-23/layer1/16:30"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
