//! Lab recorder core — logic capture ราย underlying ที่เคยอยู่ใน example
//! headless (`lab_recorder`) ย้ายมาไว้ที่นี่เพื่อให้ recorder-server
//! ในอนาคตเรียกใช้ร่วมกันได้
//!
//! โมดูลนี้ห้ามพิมพ์/ห้ามออกจาก process/ห้ามแตะ settings หรือ keyring —
//! caller (example, server) ตัดสินใจจาก [`UnderlyingReport`] เพียงอย่างเดียว
//! side effect ทั้งหมดที่ example เดิมทำต่อ underlying (log_coverage,
//! write_observations, log_write_stats) เกิดข้างใน ตามลำดับเดิม

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc, Weekday};
use serde_json::Value;

use crate::engine::{DataSource, StockUpdate, parse_stock_update};
use crate::lab::capture::{
    Anchor, WindowCapture, option_observations, plan_capture, stock_observation,
};
use crate::lab::quality::session_date_of;
use crate::lab::storage::{LabStorage, WriteStats};
use crate::lab::types::{CaptureEvent, DiscoveryCoverage};
use crate::webull::discovery::{fetch_window, strike_band};
use crate::webull::quotes::OI_BATCH;
use crate::webull::rest::{
    FetchError, HttpFetch, PATH_OPTION_SNAPSHOT, PATH_STOCK_SNAPSHOT, WINDOWS, extract_list,
    get_json,
};

/// Session NY ที่ปิดล่าสุดจากเวลาปัจจุบัน — สุดสัปดาห์ถอยกลับไปศุกร์
/// (ไม่งั้นราคาปิดวันศุกร์ วันหมดอายุสำคัญที่สุดของ options รายสัปดาห์
/// จะไม่ถูกเก็บเลย)
pub fn last_closed_session(now: DateTime<Utc>) -> NaiveDate {
    let mut session = session_date_of(now);
    while matches!(session.weekday(), Weekday::Sat | Weekday::Sun) {
        session -= Duration::days(1);
    }
    session
}

/// พารามิเตอร์การเก็บข้อมูลหนึ่ง run — ใช้ร่วมทุก underlying
#[derive(Debug, Clone, Copy)]
pub struct RecordOptions {
    pub session: NaiveDate,
    /// anchor แบบ post-close: [`Anchor::post_close`] ของ `session`
    pub anchor: Anchor,
    pub now: DateTime<Utc>,
    /// true = เก็บซ้ำแม้ session นี้มีข้อมูลแล้ว (dedupe กันซ้ำอยู่ที่ storage)
    pub refresh: bool,
    /// ชื่อกติกาคัด contracts ("near-spot-v1")
    pub selection_rule: &'static str,
}

/// ขั้นตอนที่ทำไม่สำเร็จจนต้องยกเลิก underlying นี้
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordStage {
    /// GET stock snapshot ไม่ผ่าน
    StockSnapshot,
    /// เขียน observations ลง storage ไม่ผ่าน (IO)
    Write,
}

#[derive(Debug)]
pub enum RecordError {
    Fetch(FetchError),
    Io(std::io::Error),
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordError::Fetch(e) => write!(f, "{e}"),
            RecordError::Io(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for RecordError {}

/// ผลลัพธ์การเก็บหนึ่ง underlying
#[derive(Debug)]
pub enum RecordOutcome {
    Recorded,
    /// มีข้อมูล universe นี้ของ session นี้อยู่แล้ว (idempotent skip)
    SkippedAlreadyCaptured {
        existing_rows: usize,
    },
    /// snapshot กลับมาแต่ราคาไม่ valid — ห้ามเก็บโดยไม่มีราคาจริง
    SkippedNoSpot,
    /// ล้มเหลวตั้งแต่ขั้น `stage` (ข้อมูลก่อนหน้านั้นยังอยู่ใน report)
    Failed {
        stage: RecordStage,
        error: RecordError,
    },
}

/// discovery หน้าต่างหนึ่งพลาด — หน้าต่างอื่นยังเก็บต่อ
#[derive(Debug)]
pub struct WindowError {
    pub expiry_range: (NaiveDate, NaiveDate),
    pub error: FetchError,
}

/// รายงานเต็มของการเก็บหนึ่ง underlying — ให้ caller พิมพ์/ส่งต่อได้
#[derive(Debug)]
pub struct UnderlyingReport {
    pub symbol: String,
    pub outcome: RecordOutcome,
    /// snapshot ที่ parse ได้ (มีเมื่อผ่านขั้น stock snapshot)
    pub spot: Option<StockUpdate>,
    /// coverage รายหน้าต่างที่เก็บสำเร็จ (log ลง session แล้ว)
    pub coverage: Vec<DiscoveryCoverage>,
    pub window_errors: Vec<WindowError>,
    /// แถว quotes ที่ได้ vs จำนวนสัญญาที่ขอ
    pub quote_rows: usize,
    pub requested_contracts: usize,
    /// batch ของ option snapshot ที่พลาด (batch อื่นยังเก็บต่อ)
    pub quote_errors: Vec<FetchError>,
    /// สถิติการเขียน (มีเมื่อ write สำเร็จ)
    pub write_stats: Option<WriteStats>,
}

/// เก็บข้อมูลหนึ่ง underlying: idempotency → stock snapshot → discovery
/// รายหน้าต่าง → plan → option snapshots → เขียน observations
/// ทุก side effect ต่อ storage เกิดที่นี่ตามลำดับเดิมของ example เดิม
pub fn record_underlying<F: HttpFetch>(
    f: &mut F,
    storage: &LabStorage,
    symbol: &str,
    opts: &RecordOptions,
) -> UnderlyingReport {
    let mut report = UnderlyingReport {
        symbol: symbol.to_string(),
        outcome: RecordOutcome::Recorded,
        spot: None,
        coverage: Vec::new(),
        window_errors: Vec::new(),
        quote_rows: 0,
        requested_contracts: 0,
        quote_errors: Vec::new(),
        write_stats: None,
    };

    // หนึ่ง session เก็บครั้งเดียว (idempotent) — refresh เพื่อบังคับเก็บซ้ำ
    let universe_id = format!("{symbol}-{}", opts.session);
    if !opts.refresh {
        let already = storage
            .read_observations(opts.session)
            .unwrap_or_default()
            .iter()
            .filter(|o| o.universe_id.as_deref() == Some(universe_id.as_str()))
            .count();
        if already > 0 {
            report.outcome = RecordOutcome::SkippedAlreadyCaptured {
                existing_rows: already,
            };
            return report;
        }
    }

    // 1) ราคาหุ้น (ต้อง valid ก่อน — ไม่ valid = ข้าม)
    let stock = match get_json(
        f,
        PATH_STOCK_SNAPSHOT,
        &[
            ("symbols".into(), symbol.to_string()),
            ("category".into(), "US_STOCK".into()),
            ("extend_hour_required".into(), "false".into()),
            ("overnight_required".into(), "false".into()),
        ],
    ) {
        Ok(body) => extract_list(&body)
            .first()
            .and_then(|r| parse_stock_update(r, DataSource::Rest)),
        Err(e) => {
            report.outcome = RecordOutcome::Failed {
                stage: RecordStage::StockSnapshot,
                error: RecordError::Fetch(e),
            };
            return report;
        }
    };
    let Some(stock) = stock else {
        report.outcome = RecordOutcome::SkippedNoSpot;
        return report;
    };
    report.spot = Some(stock.clone());

    // 2) discovery ทีละหน้าต่าง DTE (band ±30% เหมือน scanner)
    let (gte, lte) = strike_band(stock.price);
    let mut windows: Vec<WindowCapture> = Vec::new();
    for (x, y) in WINDOWS {
        let a = opts.session + Duration::days(x);
        let b = opts.session + Duration::days(y);
        match fetch_window(f, symbol, a, b, Some(&gte), Some(&lte), 3, None) {
            Ok(rows) => windows.push(WindowCapture {
                expiry_range: (a, b),
                has_cursor: false,
                rows,
            }),
            Err(e) => report.window_errors.push(WindowError {
                expiry_range: (a, b),
                error: e,
            }),
        }
    }

    let plan = plan_capture(symbol, stock.price, &windows, opts.selection_rule, opts.now);
    for cov in &plan.coverage {
        let _ = storage.log_coverage(opts.session, cov);
    }
    report.coverage = plan.coverage;

    // 3) option snapshots ของ subset (batch 20/call เหมือน scanner)
    report.requested_contracts = plan.subscribed_occ.len();
    let mut quote_rows: Vec<Value> = Vec::new();
    for chunk in plan.subscribed_occ.chunks(OI_BATCH) {
        match get_json(
            f,
            PATH_OPTION_SNAPSHOT,
            &[
                ("symbols".into(), chunk.join(",")),
                ("category".into(), "US_OPTION".into()),
            ],
        ) {
            Ok(body) => quote_rows.extend(extract_list(&body)),
            Err(e) => report.quote_errors.push(e),
        }
    }
    report.quote_rows = quote_rows.len();

    // 4) สร้าง + เขียน observations (validate + dedupe ข้างใน)
    let mut batch = vec![stock_observation(
        symbol,
        &stock,
        &opts.anchor,
        opts.now,
        &universe_id,
    )];
    batch.extend(option_observations(
        &quote_rows,
        &opts.anchor,
        opts.now,
        &universe_id,
    ));
    match storage.write_observations(opts.session, batch) {
        Ok(stats) => {
            let _ = storage.log_write_stats(opts.session, &stats);
            report.write_stats = Some(stats);
        }
        Err(e) => {
            report.outcome = RecordOutcome::Failed {
                stage: RecordStage::Write,
                error: RecordError::Io(e),
            };
        }
    }
    report
}

/// รายงานการปิด session — retention + Stopped event + จำนวนวันที่มีข้อมูล
#[derive(Debug)]
pub struct FinishReport {
    pub removed_dates: Vec<NaiveDate>,
    pub captured_dates: Vec<NaiveDate>,
}

/// ปิด run: retention → log Stopped → นับวันที่มีข้อมูลรวม
pub fn finish_session(
    storage: &LabStorage,
    session: NaiveDate,
    retention_days: usize,
    now: DateTime<Utc>,
) -> FinishReport {
    let removed_dates = storage.apply_retention(retention_days).unwrap_or_default();
    let _ = storage.log_session_event(session, &CaptureEvent::Stopped { at: now });
    let captured_dates = storage.captured_dates().unwrap_or_default();
    FinishReport {
        removed_dates,
        captured_dates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    /// fetcher ปลอม — ตอบตาม path ด้วย JSON สำเร็จที่เตรียมไว้ ไม่มี network
    struct FakeFetch {
        stock: Value,
        fail_stock: bool,
        /// แถว discovery รายหน้าต่างตามลำดับ WINDOWS (ที่เหลือ = ว่าง)
        discovery_rows: Vec<Vec<Value>>,
        /// index ใน WINDOWS ที่จะให้พลาด
        fail_windows: Vec<usize>,
        /// ตอบทุกครั้งขอ option snapshot ด้วยแถวชุดนี้
        quote_rows: Vec<Value>,
        window_calls: usize,
    }

    impl HttpFetch for FakeFetch {
        fn get(
            &mut self,
            path: &str,
            _params: &[(String, String)],
        ) -> Result<(u16, Value), FetchError> {
            match path {
                PATH_STOCK_SNAPSHOT => {
                    if self.fail_stock {
                        Err(FetchError::Network("stock endpoint down".into()))
                    } else {
                        Ok((200, self.stock.clone()))
                    }
                }
                crate::webull::rest::PATH_OPTION_CONTRACTS => {
                    let idx = self.window_calls;
                    self.window_calls += 1;
                    if self.fail_windows.contains(&idx) {
                        Err(FetchError::Network("discovery window down".into()))
                    } else {
                        let rows = self.discovery_rows.get(idx).cloned().unwrap_or_default();
                        Ok((200, json!({ "data": rows })))
                    }
                }
                PATH_OPTION_SNAPSHOT => Ok((200, json!({ "data": self.quote_rows.clone() }))),
                _ => Err(FetchError::Unexpected(format!(
                    "fake: unexpected path {path}"
                ))),
            }
        }
    }

    /// เสาร์เช้าเวลาไทย = หลังปิดตลาดศุกร์ — session เป้าหมายคือศุกร์ 20 พ.ย.
    fn friday_session() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 20).unwrap()
    }

    fn received() -> DateTime<Utc> {
        "2026-11-21T02:00:00Z".parse().unwrap()
    }

    fn opts(session: NaiveDate, refresh: bool) -> RecordOptions {
        RecordOptions {
            session,
            anchor: Anchor::post_close(session),
            now: received(),
            refresh,
            selection_rule: "near-spot-v1",
        }
    }

    fn tmp_storage(tag: &str) -> LabStorage {
        let dir = std::env::temp_dir()
            .join("premium-radar-recorder-tests")
            .join(format!("{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        LabStorage::open(&dir).expect("เปิด temp storage ได้")
    }

    fn sessions_log(storage: &LabStorage, session: NaiveDate) -> String {
        std::fs::read_to_string(
            storage
                .root()
                .join("v1")
                .join(session.to_string())
                .join("sessions.jsonl"),
        )
        .unwrap_or_default()
    }

    fn stock_body(price: f64) -> Value {
        json!({ "data": [{
            "symbol": "NVDA", "price": price, "pre_close": 174.5,
            "open": 175.0, "high": 177.0, "low": 174.0, "volume": 1_000_000
        }]})
    }

    /// contract ใกล้ spot 176 — expiry 27 พ.ย. อยู่ในหน้าต่างแรก (0..7 วัน)
    const CALL_175: &str = "NVDA261127C00175000";
    const PUT_175: &str = "NVDA261127P00175000";

    fn discovery_window0() -> Vec<Value> {
        vec![json!({ "symbol": CALL_175 }), json!({ "symbol": PUT_175 })]
    }

    fn quote_rows() -> Vec<Value> {
        vec![
            json!({
                "symbol": CALL_175, "price": 2.15, "bid": 2.10, "ask": 2.20,
                "open_interest": 500, "volume": 20
            }),
            json!({
                "symbol": PUT_175, "price": 1.80, "bid": 1.75, "ask": 1.85,
                "open_interest": 800, "volume": 35
            }),
        ]
    }

    fn happy_fake() -> FakeFetch {
        FakeFetch {
            stock: stock_body(176.0),
            fail_stock: false,
            discovery_rows: vec![discovery_window0()],
            fail_windows: Vec::new(),
            quote_rows: quote_rows(),
            window_calls: 0,
        }
    }

    #[test]
    fn happy_path_writes_stock_and_option_rows_and_logs_coverage() {
        let storage = tmp_storage("happy");
        let session = friday_session();
        let mut fake = happy_fake();

        let report = record_underlying(&mut fake, &storage, "NVDA", &opts(session, false));

        assert!(matches!(report.outcome, RecordOutcome::Recorded));
        let spot = report.spot.as_ref().expect("spot ต้องมีเมื่อ Recorded");
        assert_eq!(spot.price, 176.0);
        // 7 หน้าต่างสำเร็จทั้งหมด (ส่วนใหญ่ว่าง) — หน้าต่างแรกเจอ 2 contracts
        assert_eq!(report.coverage.len(), WINDOWS.len());
        assert!(report.window_errors.is_empty());
        assert_eq!(report.requested_contracts, 2);
        assert_eq!(report.quote_rows, 2);
        let stats = report.write_stats.as_ref().expect("write ต้องสำเร็จ");
        assert_eq!((stats.written, stats.duplicates), (3, 0)); // stock + option 2

        let stored = storage.read_observations(session).unwrap();
        assert_eq!(stored.len(), 3);
        assert!(
            stored
                .iter()
                .all(|o| o.universe_id.as_deref() == Some("NVDA-2026-11-20"))
        );
        // coverage + write stats ถูก log ลง session file
        let log = sessions_log(&storage, session);
        assert!(log.contains("near-spot-v1"));
        assert!(log.contains("\"written\""));
        let _ = std::fs::remove_dir_all(storage.root());
    }

    #[test]
    fn second_call_skips_then_refresh_rewrites_as_duplicates() {
        let storage = tmp_storage("refresh");
        let session = friday_session();
        let mut fake = happy_fake();
        record_underlying(&mut fake, &storage, "NVDA", &opts(session, false));

        // รันซ้ำไม่มี refresh → ข้าม และบอกจำนวนแถวที่มีอยู่
        let mut fake2 = happy_fake();
        let skip = record_underlying(&mut fake2, &storage, "NVDA", &opts(session, false));
        match skip.outcome {
            RecordOutcome::SkippedAlreadyCaptured { existing_rows } => assert_eq!(existing_rows, 3),
            other => panic!("wrong outcome {other:?}"),
        }
        assert!(skip.write_stats.is_none());
        assert_eq!(storage.read_observations(session).unwrap().len(), 3);

        // refresh → เก็บใหม่ dedupe นับเป็นซ้ำทั้งหมด จำนวนแถวไม่โต
        let mut fake3 = happy_fake();
        let again = record_underlying(&mut fake3, &storage, "NVDA", &opts(session, true));
        assert!(matches!(again.outcome, RecordOutcome::Recorded));
        let stats = again.write_stats.as_ref().unwrap();
        assert_eq!((stats.written, stats.duplicates), (0, 3));
        assert_eq!(storage.read_observations(session).unwrap().len(), 3);
        let _ = std::fs::remove_dir_all(storage.root());
    }

    #[test]
    fn missing_or_invalid_spot_skips_and_writes_nothing() {
        let storage = tmp_storage("nospot");
        let session = friday_session();

        // list ว่าง = ไม่มีราคา
        let mut empty = FakeFetch {
            stock: json!({ "data": [] }),
            fail_stock: false,
            discovery_rows: vec![],
            fail_windows: vec![],
            quote_rows: vec![],
            window_calls: 0,
        };
        let r1 = record_underlying(&mut empty, &storage, "NVDA", &opts(session, false));
        assert!(matches!(r1.outcome, RecordOutcome::SkippedNoSpot));
        assert!(r1.spot.is_none());

        // price 0 = invalid ตามกติกา parse_stock_update
        let mut zero = FakeFetch {
            stock: stock_body(0.0),
            ..empty
        };
        zero.window_calls = 0;
        let r2 = record_underlying(&mut zero, &storage, "NVDA", &opts(session, false));
        assert!(matches!(r2.outcome, RecordOutcome::SkippedNoSpot));

        // fetch พลาดเลย = Failed ที่ขั้น stock snapshot
        let mut down = FakeFetch {
            fail_stock: true,
            ..zero
        };
        down.window_calls = 0;
        let r3 = record_underlying(&mut down, &storage, "NVDA", &opts(session, false));
        match r3.outcome {
            RecordOutcome::Failed {
                stage: RecordStage::StockSnapshot,
                ..
            } => {}
            other => panic!("wrong outcome {other:?}"),
        }

        assert!(storage.read_observations(session).unwrap().is_empty());
        assert!(!sessions_log(&storage, session).contains("near-spot-v1"));
        let _ = std::fs::remove_dir_all(storage.root());
    }

    #[test]
    fn failing_discovery_window_is_reported_and_others_still_captured() {
        let storage = tmp_storage("windowfail");
        let session = friday_session();
        let mut fake = FakeFetch {
            fail_windows: vec![0], // หน้าต่างแรก (0..7) พลาด
            ..happy_fake()
        };

        let report = record_underlying(&mut fake, &storage, "NVDA", &opts(session, false));

        assert!(matches!(report.outcome, RecordOutcome::Recorded));
        assert_eq!(report.window_errors.len(), 1);
        // WINDOWS[1] = (8, 14) — หน้าต่างที่พลาดคู่กับ session
        assert_eq!(
            report.window_errors[0].expiry_range,
            (
                session + Duration::days(WINDOWS[0].0),
                session + Duration::days(WINDOWS[0].1)
            )
        );
        // หน้าต่างที่เหลือยังถูก log และเก็บ coverage (แถว discovery อยู่หน้าต่างแรก
        // ซึ่งพลาดไป → ไม่มี option แต่ stock row ยังเขียน)
        assert_eq!(report.coverage.len(), WINDOWS.len() - 1);
        assert_eq!(report.requested_contracts, 0);
        let stats = report.write_stats.as_ref().unwrap();
        assert_eq!((stats.written, stats.duplicates), (1, 0));
        assert!(sessions_log(&storage, session).contains("near-spot-v1"));
        let _ = std::fs::remove_dir_all(storage.root());
    }

    #[test]
    fn last_closed_session_walks_weekend_back_to_friday() {
        // เสาร์ 21 พ.ย. 2026 12:00 UTC (07:00 NY) → session NY = เสาร์ → ศุกร์ 20
        let saturday: DateTime<Utc> = "2026-11-21T12:00:00Z".parse().unwrap();
        assert_eq!(last_closed_session(saturday), friday_session());
        // จันทร์เช้าเวลาไทย 02:00 UTC = อาทิตย์ 21:00 NY → ถอยข้ามเสาร์-อาทิตย์
        let monday: DateTime<Utc> = "2026-11-23T02:00:00Z".parse().unwrap();
        assert_eq!(last_closed_session(monday), friday_session());
        // วันธรรมดาธรรมดา — session = วันเดียวกัน (อังคาร 10:00 NY)
        let tuesday: DateTime<Utc> = "2026-11-24T15:00:00Z".parse().unwrap();
        assert_eq!(
            last_closed_session(tuesday),
            NaiveDate::from_ymd_opt(2026, 11, 24).unwrap()
        );
    }

    #[test]
    fn finish_session_applies_retention_logs_stopped_and_counts_dates() {
        let storage = tmp_storage("finish");
        let session = friday_session();
        let mut fake = happy_fake();
        record_underlying(&mut fake, &storage, "NVDA", &opts(session, false));

        let stopped_at: DateTime<Utc> = "2026-11-21T02:05:00Z".parse().unwrap();
        let finish = finish_session(&storage, session, 400, stopped_at);

        assert!(finish.removed_dates.is_empty());
        assert_eq!(finish.captured_dates, vec![session]);
        let log = sessions_log(&storage, session);
        assert!(log.contains("Stopped"));
        let _ = std::fs::remove_dir_all(storage.root());
    }
}
