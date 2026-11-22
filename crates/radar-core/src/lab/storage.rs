//! Lab local storage — append-only JSONL รายวันใน app-data/lab/v1/
//!
//! หลักการ (PLAN-LAB P8.4 + amendments §3):
//! - append-only: ไม่แก้/ไม่ลบบรรทัดเก่า (นอกจาก retention ที่เจ้าของสั่ง)
//! - dedupe-on-write: เขียนซ้ำ (restart, รันซ้ำวันเดียวกัน) ไม่เพิ่มแถว
//! - validate ก่อนเขียน: ข้อมูลเสียถูกปฏิเสธพร้อมเหตุผลไทย ไม่ใช่เขียนไปก่อน
//! - ไม่มี credentials ใน schema โดยโครงสร้าง (export จึงสะอาดเสมอ)

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::Serialize;

use super::quality::validate_observation;
use super::types::{
    CaptureEvent, DiscoveryCoverage, LAB_SCHEMA_VERSION, Observation, StoredPrediction,
};

/// สถิติการเขียนหนึ่ง batch — บันทึกลง session log เสมอ
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct WriteStats {
    pub written: usize,
    pub duplicates: usize,
    pub rejected: Vec<RejectedRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RejectedRecord {
    pub context: String,
    pub field: &'static str,
    pub reason_th: String,
}

/// ที่เก็บข้อมูล Lab ทั้งหมด — root คือ `<app-data>/lab`
pub struct LabStorage {
    root: PathBuf,
}

fn dedupe_key(o: &Observation) -> String {
    format!(
        "{}|{}|{}",
        o.contract.occ,
        o.kind.kind_tag(),
        o.event_time.to_rfc3339()
    )
}

impl LabStorage {
    /// เปิด/สร้างที่เก็บ (`lab/v1/<date>/…`) — idempotent
    pub fn open(root: impl AsRef<Path>) -> std::io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("v1"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn day_dir(&self, date: NaiveDate) -> PathBuf {
        self.root.join("v1").join(date.to_string())
    }

    fn observations_file(&self, date: NaiveDate) -> PathBuf {
        self.day_dir(date).join("observations.jsonl")
    }

    fn sessions_file(&self, date: NaiveDate) -> PathBuf {
        self.day_dir(date).join("sessions.jsonl")
    }

    fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut f = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(f, "{line}")
    }

    /// คีย์ที่มีอยู่แล้วของวันนั้น (ไฟล์วันเดียวไม่ใหญ่ — อ่านทั้งไฟล์ได้สบาย)
    fn existing_keys(&self, date: NaiveDate) -> std::io::Result<BTreeSet<String>> {
        let mut keys = BTreeSet::new();
        let path = self.observations_file(date);
        let Ok(raw) = fs::read_to_string(&path) else {
            return Ok(keys);
        };
        for line in raw.lines().filter(|l| !l.trim().is_empty()) {
            if let Ok(o) = serde_json::from_str::<Observation>(line) {
                keys.insert(dedupe_key(&o));
            }
        }
        Ok(keys)
    }

    /// เขียน observations: validate → dedupe (กับไฟล์ + ใน batch) → append
    /// คืนสถิติเพื่อบันทึกต่อใน session log
    pub fn write_observations(
        &self,
        date: NaiveDate,
        batch: Vec<Observation>,
    ) -> std::io::Result<WriteStats> {
        let mut stats = WriteStats::default();
        let mut keys = self.existing_keys(date)?;
        let mut lines: Vec<String> = Vec::new();
        for o in batch {
            if let Err(rej) = validate_observation(&o) {
                stats.rejected.push(RejectedRecord {
                    context: o.contract.occ.clone(),
                    field: rej.field,
                    reason_th: rej.reason_th,
                });
                continue;
            }
            let k = dedupe_key(&o);
            if keys.contains(&k) {
                stats.duplicates += 1;
                continue;
            }
            keys.insert(k);
            lines.push(serde_json::to_string(&o).map_err(io_err)?);
            stats.written += 1;
        }
        if lines.is_empty() {
            return Ok(stats);
        }
        let path = self.observations_file(date);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
        for line in lines {
            writeln!(f, "{line}")?;
        }
        Ok(stats)
    }

    /// บันทึกเหตุการณ์ session (start/stop/reconnect) + coverage + stats
    pub fn log_session_event(&self, date: NaiveDate, event: &CaptureEvent) -> std::io::Result<()> {
        let line = serde_json::to_string(event).map_err(io_err)?;
        Self::append_line(&self.sessions_file(date), &line)
    }

    pub fn log_coverage(&self, date: NaiveDate, cov: &DiscoveryCoverage) -> std::io::Result<()> {
        let line = serde_json::to_string(cov).map_err(io_err)?;
        Self::append_line(&self.sessions_file(date), &line)
    }

    pub fn log_write_stats(&self, date: NaiveDate, stats: &WriteStats) -> std::io::Result<()> {
        let line = serde_json::to_string(stats).map_err(io_err)?;
        Self::append_line(&self.sessions_file(date), &line)
    }

    /// วันที่ที่มีข้อมูล (เรียงแล้ว) — ใช้ทำ retention และดู coverage
    pub fn captured_dates(&self) -> std::io::Result<Vec<NaiveDate>> {
        let mut dates = Vec::new();
        let v1 = self.root.join("v1");
        let Ok(entries) = fs::read_dir(&v1) else {
            return Ok(dates);
        };
        for e in entries.flatten() {
            let name = e.file_name();
            if let Some(Ok(d)) = name.to_str().map(|s| s.parse::<NaiveDate>()) {
                dates.push(d);
            }
        }
        dates.sort();
        Ok(dates)
    }

    /// Retention: เก็บ `keep_days` วันล่าสุด ลบที่เก่ากว่า — คืนวันที่ที่ถูกลบ
    /// (P8.4: delete ทำงานเฉพาะข้อมูล Lab ไม่แตะอย่างอื่นใน app-data)
    pub fn apply_retention(&self, keep_days: usize) -> std::io::Result<Vec<NaiveDate>> {
        let dates = self.captured_dates()?;
        let keep_from = dates.len().saturating_sub(keep_days);
        let mut removed = Vec::new();
        for &d in &dates[..keep_from] {
            let dir = self.day_dir(d);
            if dir.exists() && fs::remove_dir_all(&dir).is_ok() {
                removed.push(d);
            }
        }
        Ok(removed)
    }

    /// Export วันหนึ่งเป็น JSON รวม (ไม่มี credentials ได้โดยโครงสร้าง —
    /// schema ของ Observation/Coverage/Event ไม่มี field ค่าลับใดๆ)
    pub fn export_day(&self, date: NaiveDate) -> std::io::Result<String> {
        #[derive(Serialize)]
        struct DayExport {
            schema_version: u32,
            date: NaiveDate,
            observations: Vec<Observation>,
            sessions: Vec<serde_json::Value>,
        }
        let observations = self.read_observations(date)?;
        let mut sessions = Vec::new();
        if let Ok(raw) = fs::read_to_string(self.sessions_file(date)) {
            for line in raw.lines().filter(|l| !l.trim().is_empty()) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    sessions.push(v);
                }
            }
        }
        let bundle = DayExport {
            schema_version: LAB_SCHEMA_VERSION,
            date,
            observations,
            sessions,
        };
        serde_json::to_string_pretty(&bundle).map_err(io_err)
    }

    /// คำทำนาย freeze (append-only ทั้งไฟล์เดียว lab/v1/predictions.jsonl)
    pub fn append_prediction(&self, pred: &StoredPrediction) -> std::io::Result<()> {
        let line = serde_json::to_string(pred).map_err(io_err)?;
        let path = self.root.join("v1").join("predictions.jsonl");
        Self::append_line(&path, &line)
    }

    pub fn read_predictions(&self) -> std::io::Result<Vec<StoredPrediction>> {
        let mut out = Vec::new();
        let Ok(raw) = fs::read_to_string(self.root.join("v1").join("predictions.jsonl")) else {
            return Ok(out);
        };
        for line in raw.lines().filter(|l| !l.trim().is_empty()) {
            if let Ok(p) = serde_json::from_str::<StoredPrediction>(line) {
                out.push(p);
            }
        }
        Ok(out)
    }

    pub fn read_observations(&self, date: NaiveDate) -> std::io::Result<Vec<Observation>> {
        let mut out = Vec::new();
        let Ok(raw) = fs::read_to_string(self.observations_file(date)) else {
            return Ok(out);
        };
        for line in raw.lines().filter(|l| !l.trim().is_empty()) {
            if let Ok(o) = serde_json::from_str::<Observation>(line) {
                out.push(o);
            }
        }
        Ok(out)
    }

    /// ลบข้อมูล Lab ทั้งหมด (เจ้าของสั่งเท่านั้น) — คืนจำนวนวันที่ลบ
    pub fn delete_all(&self) -> std::io::Result<usize> {
        let dates = self.captured_dates()?;
        let mut n = 0;
        for d in dates {
            let dir = self.day_dir(d);
            if dir.exists() && fs::remove_dir_all(&dir).is_ok() {
                n += 1;
            }
        }
        // F4: predictions ก็เป็นข้อมูล Lab — ลบพร้อม
        let preds = self.root.join("v1").join("predictions.jsonl");
        if preds.exists() && fs::remove_file(&preds).is_ok() {
            n += 1;
        }
        Ok(n)
    }
}

fn io_err(e: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::types::{ContractRef, ObsKind};
    use crate::quant::OptType;
    use chrono::Utc;

    fn tmp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("premium-radar-lab-tests")
            .join(format!("{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn stock_obs(price: Option<f64>, event: &str) -> Observation {
        let event_time: chrono::DateTime<Utc> = event.parse().unwrap();
        let session = crate::lab::quality::session_date_of(event_time);
        let m = crate::webull::contracts::parse_occ("NVDA261204P00175000").unwrap();
        Observation {
            contract: ContractRef {
                occ: "NVDA261204P00175000".into(),
                underlying: m.underlying,
                expiry: m.expiry,
                option_type: OptType::Put,
                strike: 175.0,
                multiplier: 100.0,
            },
            kind: ObsKind::OptionQuote {
                price,
                bid: Some(1.4),
                ask: Some(1.6),
                iv: None,
                delta: None,
                oi: Some(500.0),
                volume: Some(20.0),
            },
            session_date: session,
            event_time: event_time,
            received_time: event_time,
            available_at: event_time,
            source: crate::engine::DataSource::Rest,
            backfill: false,
            universe_id: Some("test".into()),
        }
    }

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 20).unwrap()
    }

    #[test]
    fn write_dedupe_on_restart_same_day() {
        let root = tmp_root("dedupe");
        let store = LabStorage::open(&root).unwrap();
        let obs = vec![stock_obs(Some(2.1), "2026-11-20T22:00:00Z")];
        let s1 = store.write_observations(date(), obs.clone()).unwrap();
        assert_eq!((s1.written, s1.duplicates), (1, 0));
        // รันซ้ำวันเดียวกัน (restart) — ต้องนับเป็น duplicate ไม่เพิ่มแถว
        let s2 = store.write_observations(date(), obs).unwrap();
        assert_eq!((s2.written, s2.duplicates), (0, 1));
        assert_eq!(store.read_observations(date()).unwrap().len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn invalid_observation_rejected_with_thai_reason_not_written() {
        let root = tmp_root("reject");
        let store = LabStorage::open(&root).unwrap();
        // received ก่อน event → ปฏิเสธ
        let mut o = stock_obs(Some(2.1), "2026-11-20T22:00:00Z");
        o.received_time = "2026-11-20T21:00:00Z".parse().unwrap();
        let s = store.write_observations(date(), vec![o]).unwrap();
        assert_eq!(s.written, 0);
        assert_eq!(s.rejected.len(), 1);
        assert!(s.rejected[0].reason_th.contains("ก่อน event"));
        assert!(store.read_observations(date()).unwrap().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn retention_removes_only_old_days() {
        let root = tmp_root("retention");
        let store = LabStorage::open(&root).unwrap();
        let d1 = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let d3 = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
        for (d, ev) in [
            (d1, "2026-11-01T22:00:00Z"),
            (d2, "2026-11-02T22:00:00Z"),
            (d3, "2026-11-03T22:00:00Z"),
        ] {
            store
                .write_observations(d, vec![stock_obs(Some(1.0), ev)])
                .unwrap();
        }
        let removed = store.apply_retention(2).unwrap();
        assert_eq!(removed, vec![d1]);
        assert_eq!(store.captured_dates().unwrap(), vec![d2, d3]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn export_contains_no_credential_fields() {
        let root = tmp_root("export");
        let store = LabStorage::open(&root).unwrap();
        store
            .write_observations(date(), vec![stock_obs(Some(2.1), "2026-11-20T22:00:00Z")])
            .unwrap();
        let out = store.export_day(date()).unwrap();
        for forbidden in ["app_key", "app_secret", "access_token", "api_key"] {
            assert!(!out.contains(forbidden), "export leaked field {forbidden}");
        }
        assert!(out.contains("NVDA261204P00175000"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn delete_all_removes_every_day() {
        let root = tmp_root("delete");
        let store = LabStorage::open(&root).unwrap();
        store
            .write_observations(date(), vec![stock_obs(Some(2.1), "2026-11-20T22:00:00Z")])
            .unwrap();
        assert_eq!(store.delete_all().unwrap(), 1);
        assert!(store.captured_dates().unwrap().is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
