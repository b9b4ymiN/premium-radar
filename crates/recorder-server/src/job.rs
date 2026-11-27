//! รัน job จริง (blocking) — token check → Started event → ทุก symbol
//! ผ่าน `record_underlying` → Layer 1 ปิดท้ายด้วย `finish_session`
//! logic การเก็บทั้งหมดอยู่ใน `radar_core::lab::recorder` — ที่นี่เหลือแค่
//! ประกอบส่วนที่ server เป็นเจ้าของ (schedule/symbols/budget/สรุปผล)

use chrono::{DateTime, Utc};
use radar_core::lab::capture::Anchor;
use radar_core::lab::recorder::{
    RecordOptions, RecordOutcome, RecordStage, finish_session, last_closed_session,
    record_underlying,
};
use radar_core::lab::storage::LabStorage;
use radar_core::lab::types::CaptureEvent;
use radar_core::webull::auth::{self, TokenStatus};
use radar_core::webull::transport::WebullClient;

use crate::budget::BudgetedClient;
use crate::config::{Config, Secrets};
use crate::logln;
use crate::schedule::JobKind;
use crate::status::TokenInfo;

/// rule การเลือก contract ของ job ที่ scheduler สั่ง — งบเท่าเดิม (≤ 96 ตัว)
/// แต่กัน 64 ตัวให้ put ลึกถึง ~−20 % ใต้ spot เพื่อให้ Easy หา cash-secured
/// put ที่มีโอกาสถูก assign ต่ำได้ (near-spot-v1 เก็บ put ใกล้ spot อย่างเดียว)
const SELECTION_RULE: &str = "put-depth-v1";

/// job หนึ่งงานตามที่ caller สั่ง
pub struct JobRequest {
    pub kind: JobKind,
    pub symbols: Vec<String>,
    /// slot ที่จองไว้ (RFC3339 ของเวลา NY) — หรือ "(manual)" สำหรับ `once`
    pub scheduled_for_ny: String,
    /// scheduler ตั้ง true เสมอ: หนึ่ง session มีหลาย snapshot ต่อวัน
    /// (L2 10:00/12:45/15:30 + L1 16:30) และ dedupe กันซ้ำอยู่ที่ storage
    /// (`occ|kind|event_time`) — `once` default false ให้ idempotent เหมือน local
    pub refresh: bool,
}

/// สรุป job สำหรับ log บรรทัดเดียว + ปรับ status.json
#[derive(Debug)]
pub struct JobSummary {
    pub kind: JobKind,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub symbols: usize,
    pub recorded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub written: usize,
    pub duplicates: usize,
    pub rejected: usize,
    /// มีเมื่อ Layer 1 จบ (retention)
    pub retention_removed: usize,
    pub captured_days: usize,
    pub session: Option<chrono::NaiveDate>,
    pub token: Option<TokenInfo>,
    /// job-level error (token ใช้ไม่ได้ / เปิด storage ไม่ได้) — job ถูกข้าม
    pub error: Option<String>,
}

impl JobSummary {
    fn new(kind: JobKind, symbols: usize) -> Self {
        Self {
            kind,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            symbols,
            recorded: 0,
            skipped: 0,
            failed: 0,
            written: 0,
            duplicates: 0,
            rejected: 0,
            retention_removed: 0,
            captured_days: 0,
            session: None,
            token: None,
            error: None,
        }
    }

    /// บรรทัดสรุป job สำหรับ log
    pub fn summary_line(&self) -> String {
        let elapsed = (self.finished_at - self.started_at).num_seconds().max(0);
        let base = format!(
            "job={} done session={} symbols={} recorded={} skipped={} failed={} written={} dup={} rej={} elapsed={}s",
            self.kind.label(),
            self.session
                .map(|s| s.to_string())
                .unwrap_or_else(|| "-".into()),
            self.symbols,
            self.recorded,
            self.skipped,
            self.failed,
            self.written,
            self.duplicates,
            self.rejected,
            elapsed,
        );
        if self.kind == JobKind::Layer1 {
            format!(
                "{base} retention_removed={} captured_days={}",
                self.retention_removed, self.captured_days
            )
        } else {
            base
        }
    }
}

fn expires_at(ms: Option<u64>) -> Option<DateTime<Utc>> {
    ms.and_then(|m| DateTime::from_timestamp_millis(m as i64))
}

/// รัน job เต็ม — blocking, เรียกผ่าน `spawn_blocking` จาก async context
pub fn run_job(cfg: &Config, secrets: &Secrets, req: JobRequest) -> JobSummary {
    let mut sum = JobSummary::new(req.kind, req.symbols.len());
    let now = Utc::now();

    let storage = match LabStorage::open(cfg.lab_dir()) {
        Ok(s) => s,
        Err(e) => {
            sum.error = Some(format!(
                "เปิด LabStorage ที่ {} ไม่ได้: {e}",
                cfg.lab_dir().display()
            ));
            sum.finished_at = Utc::now();
            return sum;
        }
    };

    let mut client = WebullClient::new(secrets.creds.clone());
    if let Some(url) = &cfg.webull_base_url {
        client = client.with_base_url(url.clone());
    }

    // token ต้อง NORMAL ก่อนทำงาน — ไม่ผ่าน = ข้าม job (loop ยังอยู่)
    match auth::check_token(&mut client, &secrets.access_token) {
        Ok(check) => {
            sum.token = Some(TokenInfo {
                status: check.status.as_str().to_string(),
                expires_at: expires_at(check.expires_ms),
            });
            if check.status != TokenStatus::Normal {
                sum.error = Some(format!(
                    "token ไม่ผ่าน ({}) — ข้าม job {} (สร้าง token ใหม่แล้ววางที่ env/ไฟล์)",
                    check.status.as_str(),
                    req.kind.label()
                ));
                sum.finished_at = Utc::now();
                return sum;
            }
        }
        Err(e) => {
            sum.token = Some(TokenInfo {
                status: "CHECK_FAILED".to_string(),
                expires_at: None,
            });
            sum.error = Some(format!(
                "ตรวจ token ไม่สำเร็จ: {e} — ข้าม job {}",
                req.kind.label()
            ));
            sum.finished_at = Utc::now();
            return sum;
        }
    }

    // session + anchor ตามชั้น: L1 = ภาพหลังปิดของ session ล่าสุด,
    // L2 = สังเกตสด ณ ตอนนี้
    let (session, anchor) = match req.kind {
        JobKind::Layer1 => {
            let s = last_closed_session(now);
            (s, Anchor::post_close(s))
        }
        JobKind::Layer2Core | JobKind::Layer2Universe => {
            let a = Anchor::at_observation(now);
            (a.session_date, a)
        }
    };
    sum.session = Some(session);
    let scope = req.kind.scope();
    logln(format!(
        "job={} start: slot={} session={} refresh={} scope_windows={} band=±{}% rule={} symbols={}",
        req.kind.label(),
        req.scheduled_for_ny,
        session,
        req.refresh,
        scope.windows.len(),
        (scope.strike_pct * 100.0) as u32,
        SELECTION_RULE,
        req.symbols.len()
    ));
    let _ = storage.log_session_event(session, &CaptureEvent::Started { at: now });

    let opts = RecordOptions {
        session,
        anchor,
        now,
        refresh: req.refresh,
        selection_rule: SELECTION_RULE,
        scope,
    };
    let mut budgeted = BudgetedClient::new(client, cfg.max_requests_per_minute);

    for symbol in &req.symbols {
        let report = record_underlying(&mut budgeted, &storage, symbol, &opts);
        match &report.outcome {
            RecordOutcome::Recorded => sum.recorded += 1,
            RecordOutcome::SkippedAlreadyCaptured { .. } | RecordOutcome::SkippedNoSpot => {
                sum.skipped += 1
            }
            RecordOutcome::Failed { .. } => sum.failed += 1,
        }
        if let Some(stats) = &report.write_stats {
            sum.written += stats.written;
            sum.duplicates += stats.duplicates;
            sum.rejected += stats.rejected.len();
        }
        // หนึ่งบรรทัดต่อ underlying — outcome, rows, duplicates, rejected
        let line = match &report.outcome {
            RecordOutcome::Recorded => format!(
                "sym={symbol} outcome=recorded spot={} windows={} window_errs={} quotes={}/{} written={} dup={} rej={}",
                report
                    .spot
                    .as_ref()
                    .map(|s| format!("{:.2}", s.price))
                    .unwrap_or_else(|| "?".into()),
                report.coverage.len(),
                report.window_errors.len(),
                report.quote_rows,
                report.requested_contracts,
                report.write_stats.as_ref().map(|s| s.written).unwrap_or(0),
                report
                    .write_stats
                    .as_ref()
                    .map(|s| s.duplicates)
                    .unwrap_or(0),
                report
                    .write_stats
                    .as_ref()
                    .map(|s| s.rejected.len())
                    .unwrap_or(0),
            ),
            RecordOutcome::SkippedAlreadyCaptured { existing_rows } => {
                format!("sym={symbol} outcome=skipped_already existing_rows={existing_rows}")
            }
            RecordOutcome::SkippedNoSpot => {
                format!("sym={symbol} outcome=skipped_no_spot (holiday/ไม่มีราคา valid)")
            }
            RecordOutcome::Failed { stage, error } => format!(
                "sym={symbol} outcome=failed stage={} error=\"{error}\"",
                match stage {
                    RecordStage::StockSnapshot => "stock_snapshot",
                    RecordStage::Write => "write",
                }
            ),
        };
        logln(format!("job={} {}", req.kind.label(), line));
    }

    if req.kind == JobKind::Layer1 {
        let finish = finish_session(&storage, session, cfg.retention_days, Utc::now());
        sum.retention_removed = finish.removed_dates.len();
        sum.captured_days = finish.captured_dates.len();
    }

    sum.finished_at = Utc::now();
    sum
}
