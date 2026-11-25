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
use crate::lab::quality::{bar_available_at, session_date_of};
use crate::lab::storage::{LabStorage, WriteStats};
use crate::lab::types::{CaptureEvent, DiscoveryCoverage};
use crate::webull::discovery::fetch_window;
use crate::webull::quotes::OI_BATCH;
use crate::webull::rest::{
    FetchError, HttpFetch, PATH_OPTION_SNAPSHOT, PATH_STOCK_SNAPSHOT, STRIKE_BAND, WINDOWS,
    extract_list, get_json,
};

/// ขอบเขตการเก็บหนึ่ง run — หน้าต่าง DTE ที่จะ discovery + ความกว้าง
/// strike band รอบ spot ให้ caller แคบกว่าพฤติกรรมเดิมของ scanner ได้
/// (`legacy()` = WINDOWS ครบทั้ง 7 หน้าต่าง + ±30% — ค่าที่ local recorder
/// ใช้อยู่ พฤติกรรม/ผลลัพธ์ต้องไม่เปลี่ยน)
#[derive(Debug, Clone)]
pub struct CaptureScope {
    /// หน้าต่าง (x, y) วันนับจาก session — ไล่ตามลำดับที่ให้
    pub windows: Vec<(i64, i64)>,
    /// ความกว้าง band รอบ spot (0.20 = ±20%)
    pub strike_pct: f64,
}

impl Default for CaptureScope {
    fn default() -> Self {
        Self::legacy()
    }
}

impl CaptureScope {
    /// พฤติกรรมเดิมของ lab_recorder: หน้าต่าง `WINDOWS` ทั้งหมด + band ±30%
    pub fn legacy() -> Self {
        Self {
            windows: WINDOWS.to_vec(),
            strike_pct: STRIKE_BAND,
        }
    }

    /// หน้าต่าง `WINDOWS` ที่ไม่เกิน `max_dte` วัน — คู่ (x, y) ใด y ถูกตัด
    /// ลงเท่า cap และคู่ที่ x เกิน cap ถูกตัดทิ้ง
    /// (60 ⇒ (0,7),(8,14),(15,30),(31,60) · 45 ⇒ …,(31,45))
    pub fn clipped_windows(max_dte: i64) -> Vec<(i64, i64)> {
        WINDOWS
            .iter()
            .filter(|&&(x, _)| x <= max_dte)
            .map(|&(x, y)| (x, y.min(max_dte)))
            .collect()
    }

    /// scope สำหรับ DTE 0..=max_dte + band ±`strike_pct`
    pub fn dte_capped(max_dte: i64, strike_pct: f64) -> Self {
        Self {
            windows: Self::clipped_windows(max_dte),
            strike_pct,
        }
    }

    /// ขอบเขต strike รอบ spot — สูตร/รูปแบบ (`{:.2}`) เดียมกับ
    /// `discovery::strike_band` แต่คุมความกว้างเอง; `legacy()` ให้ค่า
    /// ตรงกันทุกตัวอักษร
    pub fn strike_band(&self, spot: f64) -> (String, String) {
        (
            format!("{:.2}", spot * (1.0 - self.strike_pct)),
            format!("{:.2}", spot * (1.0 + self.strike_pct)),
        )
    }
}

/// พารามิเตอร์การเก็บข้อมูลหนึ่ง run — ใช้ร่วมทุก underlying
#[derive(Debug, Clone)]
pub struct RecordOptions {
    pub session: NaiveDate,
    /// anchor แบบ post-close: [`Anchor::post_close`] ของ `session`
    pub anchor: Anchor,
    pub now: DateTime<Utc>,
    /// true = เก็บซ้ำแม้ session นี้มีข้อมูลแล้ว (dedupe กันซ้ำอยู่ที่ storage)
    pub refresh: bool,
    /// ชื่อกติกาคัด contracts ("near-spot-v1")
    pub selection_rule: &'static str,
    /// ขอบเขตการเก็บ — default/`legacy()` คือพฤติกรรมเดิมทั้งหมด
    pub scope: CaptureScope,
}

/// Session NY ที่ปิดล่าสุดจากเวลาปัจจุบัน — ยังไม่ถึงเวลาปิด (16:00 NY
/// เทียบเป็น instant ผ่าน `bar_available_at`) ถือว่าวันก่อนหน้าคือ
/// session ล่าสุดที่ปิดแล้ว (ไม่งั้น `Anchor::post_close` ตั้ง event_time
/// อนาคตแล้ว storage ปฏิเสธทุกแถว) แล้วสุดสัปดาห์ถอยกลับไปศุกร์
/// (ไม่งั้นราคาปิดวันศุกร์ วันหมดอายุสำคัญที่สุดของ options รายสัปดาห์
/// จะไม่ถูกเก็บเลย) — วันหยุดยังไม่รู้ (เหมือนเดิม)
pub fn last_closed_session(now: DateTime<Utc>) -> NaiveDate {
    let mut session = session_date_of(now);
    if now < bar_available_at(session) {
        session -= Duration::days(1);
    }
    while matches!(session.weekday(), Weekday::Sat | Weekday::Sun) {
        session -= Duration::days(1);
    }
    session
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

    // 2) discovery ทีละหน้าต่าง DTE ตาม scope ของ run (band ±scope.strike_pct)
    let (gte, lte) = opts.scope.strike_band(stock.price);
    let mut windows: Vec<WindowCapture> = Vec::new();
    for &(x, y) in &opts.scope.windows {
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
            scope: CaptureScope::legacy(),
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
    fn capture_scope_legacy_matches_scanner_behaviour() {
        let s = CaptureScope::legacy();
        assert_eq!(s.windows, WINDOWS.to_vec());
        assert_eq!(s.strike_pct, STRIKE_BAND);
        // สูตรเดียวกับ discovery::strike_band ทุกตัวอักษร
        assert_eq!(
            s.strike_band(182.45),
            crate::webull::discovery::strike_band(182.45)
        );
        assert_eq!(
            s.strike_band(58.12),
            crate::webull::discovery::strike_band(58.12)
        );
        assert_eq!(
            CaptureScope::default().windows,
            CaptureScope::legacy().windows
        );
    }

    #[test]
    fn capture_scope_clips_windows_to_dte_cap() {
        assert_eq!(
            CaptureScope::clipped_windows(60),
            vec![(0, 7), (8, 14), (15, 30), (31, 60)]
        );
        assert_eq!(
            CaptureScope::clipped_windows(45),
            vec![(0, 7), (8, 14), (15, 30), (31, 45)]
        );
        assert_eq!(
            CaptureScope::clipped_windows(30),
            vec![(0, 7), (8, 14), (15, 30)]
        );
        assert_eq!(CaptureScope::clipped_windows(7), vec![(0, 7)]);
        // cap 0 = เหลือ expiry วันนี้เท่านั้น; cap ติดลบ = ไม่มีหน้าต่างเลย
        assert_eq!(CaptureScope::clipped_windows(0), vec![(0, 0)]);
        assert!(CaptureScope::clipped_windows(-1).is_empty());
    }

    #[test]
    fn capture_scope_strike_band_at_20_and_15_percent() {
        let s20 = CaptureScope::dte_capped(60, 0.20);
        assert_eq!(s20.windows.len(), 4);
        assert_eq!(
            s20.strike_band(100.0),
            ("80.00".to_string(), "120.00".to_string())
        );
        let s15 = CaptureScope::dte_capped(45, 0.15);
        assert_eq!(
            s15.strike_band(200.0),
            ("170.00".to_string(), "230.00".to_string())
        );
    }

    #[test]
    fn narrowed_scope_fetches_only_kept_windows() {
        let storage = tmp_storage("narrow");
        let session = friday_session();
        let mut fake = happy_fake();
        let narrow = RecordOptions {
            scope: CaptureScope::dte_capped(7, 0.20),
            ..opts(session, false)
        };

        let report = record_underlying(&mut fake, &storage, "NVDA", &narrow);

        // เหลือหน้าต่างเดียว (0..7) — discovery ถูกเรียกแค่ครั้งเดียว
        assert_eq!(fake.window_calls, 1);
        assert_eq!(report.coverage.len(), 1);
        assert_eq!(
            report.coverage[0].expiry_range,
            (session, session + Duration::days(7))
        );
        let _ = std::fs::remove_dir_all(storage.root());
    }

    #[test]
    fn last_closed_session_steps_back_before_close_and_over_weekend() {
        // พุธ 25 พ.ย. 2026 06:54 NY (11:54Z — ก่อนปิด 16:00) → อังคาร 24
        let wed_before: DateTime<Utc> = "2026-11-25T11:54:00Z".parse().unwrap();
        assert_eq!(
            last_closed_session(wed_before),
            NaiveDate::from_ymd_opt(2026, 11, 24).unwrap()
        );
        // พุธ 16:30 NY (21:30Z — หลังปิด) → วันเดียวกัน
        let wed_after: DateTime<Utc> = "2026-11-25T21:30:00Z".parse().unwrap();
        assert_eq!(
            last_closed_session(wed_after),
            NaiveDate::from_ymd_opt(2026, 11, 25).unwrap()
        );
        // จันทร์ 09:00 NY ก่อนปิด → ถอยข้ามหางสัปดาห์ไปศุกร์ 20 พ.ย.
        let monday: DateTime<Utc> = "2026-11-23T14:00:00Z".parse().unwrap();
        assert_eq!(last_closed_session(monday), friday_session());
        // เสาร์/อาทิตย์ทุกเวลา → ศุกร์ 20 พ.ย.
        let saturday: DateTime<Utc> = "2026-11-21T12:00:00Z".parse().unwrap();
        assert_eq!(last_closed_session(saturday), friday_session());
        let sunday: DateTime<Utc> = "2026-11-22T12:00:00Z".parse().unwrap();
        assert_eq!(last_closed_session(sunday), friday_session());
    }

    #[test]
    fn last_closed_session_uses_ny_close_instant_across_dst() {
        // พุธ 15 ก.ค. 2026 เป็น EDT — ปิด 16:00 NY = 20:00Z ไม่ใช่ 21:00Z:
        // 20:30Z (16:30 NY) หลังปิดแล้ว → วันเดียวกัน (เช็ค offset ตายตัว
        // 21:00Z จะถอยผิดไปอังคาร)
        let summer_after: DateTime<Utc> = "2026-07-15T20:30:00Z".parse().unwrap();
        assert_eq!(
            last_closed_session(summer_after),
            NaiveDate::from_ymd_opt(2026, 7, 15).unwrap()
        );
        // 10:54Z (06:54 NY) ก่อนปิด → อังคาร 14 ก.ค.
        let summer_before: DateTime<Utc> = "2026-07-15T10:54:00Z".parse().unwrap();
        assert_eq!(
            last_closed_session(summer_before),
            NaiveDate::from_ymd_opt(2026, 7, 14).unwrap()
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
