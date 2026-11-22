//! Capture builder — เปลี่ยนข้อมูลดิบที่ fetch มาแล้วเป็น Lab records
//! (P8.4: universe ก่อนคัดกรอง + coverage แยกจาก subset ที่ขอ quotes)
//!
//! Pure functions ทั้งหมด — HTTP อยู่ที่ caller (recorder/แอป)

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::engine::{DataSource, StockUpdate, parse_quote_update};
use crate::lab::quality::{detect_truncation, session_date_of};
use crate::lab::types::{ContractRef, DiscoveryCoverage, ObsKind, Observation};
use crate::quant::OptType;
use crate::webull::contracts::parse_occ;
use crate::webull::discovery::pick_ws_symbols;
use crate::webull::rest::PAGE_CAP;

/// หน้าต่าง discovery หนึ่งช่วงที่ fetch มาแล้ว
pub struct WindowCapture {
    pub expiry_range: (chrono::NaiveDate, chrono::NaiveDate),
    pub rows: Vec<Value>,
    /// pagination_key กลับมาจาก API หรือไม่
    pub has_cursor: bool,
}

/// แผน capture หนึ่ง underlying: coverage รายหน้าต่าง + subset ที่จะขอ quotes
/// (subscribed เป็น subset เสมอ — รายงาน coverage ไม่อ้างว่า universe ครบ
/// เมื่อชน cap โดยไม่มี cursor)
pub struct CapturePlan {
    pub coverage: Vec<DiscoveryCoverage>,
    /// OCC ที่เลือกขอ quotes (pick_ws_symbols: ใกล้ spot, ตาม limit เดิมของ scanner)
    pub subscribed_occ: Vec<String>,
}

pub fn plan_capture(
    _symbol: &str,
    spot: f64,
    windows: &[WindowCapture],
    selection_rule: &str,
    received_at: DateTime<Utc>,
) -> CapturePlan {
    let mut coverage = Vec::new();
    let mut all_occ: Vec<String> = Vec::new();
    for w in windows {
        let mut discovered: Vec<String> = Vec::new();
        for r in &w.rows {
            if let Some(sym) = r.get("symbol").and_then(Value::as_str) {
                let clean = sym.replace(' ', "");
                if parse_occ(&clean).is_some() && !discovered.contains(&clean) {
                    discovered.push(clean);
                }
            }
        }
        let truncated = detect_truncation(w.rows.len(), PAGE_CAP, w.has_cursor);
        all_occ.extend(discovered.iter().cloned());
        coverage.push(DiscoveryCoverage {
            expiry_range: w.expiry_range,
            rows: w.rows.len(),
            possibly_truncated: truncated,
            discovered_occ: discovered,
            subscribed_occ: Vec::new(), // เติมรวมท้ายฟังก์ชัน
            selection_rule: selection_rule.to_string(),
            recorded_at: received_at,
        });
    }
    let subscribed_occ = pick_ws_symbols(&all_occ, spot, 0);
    for c in &mut coverage {
        c.subscribed_occ = subscribed_occ
            .iter()
            .filter(|s| c.discovered_occ.contains(s))
            .cloned()
            .collect();
    }
    CapturePlan {
        coverage,
        subscribed_occ,
    }
}

/// จุดยึดเวลาของ observation — แยก "สังเกตสด" กับ "เก็บหลังปิดตลาด"
/// (post-close: event = เวลาปิด session นั้น, backfill = true ตาม amendments §3)
#[derive(Debug, Clone, Copy)]
pub struct Anchor {
    pub event_time: DateTime<Utc>,
    pub session_date: chrono::NaiveDate,
    pub backfill: bool,
}

impl Anchor {
    /// สังเกตสด ณ เวลานี้ (event = received = t)
    pub fn at_observation(t: DateTime<Utc>) -> Self {
        Self {
            event_time: t,
            session_date: session_date_of(t),
            backfill: false,
        }
    }

    /// เก็บภาพ "หลังปิดตลาด" ของ session ที่ระบุ — event ยึดเวลาปิด
    /// ราคาที่ได้คือราคาปิดของ session นั้น (สุดสัปดาห์เก็บได้ = ศุกร์)
    pub fn post_close(target: chrono::NaiveDate) -> Self {
        Self {
            event_time: crate::lab::quality::bar_available_at(target),
            session_date: target,
            backfill: true,
        }
    }
}

fn contract_ref(occ: &str) -> Option<ContractRef> {
    let m = parse_occ(occ)?;
    Some(ContractRef {
        occ: occ.to_string(),
        underlying: m.underlying,
        expiry: m.expiry,
        option_type: m.option_type,
        strike: m.strike,
        // standard US equity option — nonstandard deliverable จะถูก
        // validate_contract ปฏิเสธเมื่อระบุค่าจริงต่างออกไป
        multiplier: 100.0,
    })
}

fn pos(v: f64) -> Option<f64> {
    (v.is_finite() && v > 0.0).then_some(v)
}

/// แปลง option snapshot rows → Observations (เวลา = เวลาที่สังเกตจริง;
/// ตลาดปิด = ภาพราคาปิดของ session ล่าสุด — as-of ถูกบันทึกครบในตัวข้อมูล)
pub fn option_observations(
    quote_rows: &[Value],
    anchor: &Anchor,
    received_at: DateTime<Utc>,
    universe_id: &str,
) -> Vec<Observation> {
    let session = anchor.session_date;
    quote_rows
        .iter()
        .filter_map(|v| {
            let q = parse_quote_update(v, DataSource::Rest)?;
            let contract = contract_ref(&q.symbol)?;
            Some(Observation {
                contract,
                kind: ObsKind::OptionQuote {
                    price: pos(q.price),
                    bid: pos(q.bid),
                    ask: pos(q.ask),
                    iv: None,
                    delta: None,
                    oi: pos(q.oi),
                    volume: pos(q.volume),
                },
                session_date: session,
                event_time: anchor.event_time,
                received_time: received_at,
                available_at: anchor.event_time,
                source: DataSource::Rest,
                backfill: anchor.backfill,
                universe_id: Some(universe_id.to_string()),
            })
        })
        .collect()
}

/// ราคาหุ้น underlying → Observation (pre_close เก็บด้วยเพื่อ outcome resolution)
pub fn stock_observation(
    symbol: &str,
    update: &StockUpdate,
    anchor: &Anchor,
    received_at: DateTime<Utc>,
    universe_id: &str,
) -> Observation {
    let contract = ContractRef {
        occ: symbol.to_string(),
        underlying: symbol.to_string(),
        expiry: received_at.date_naive(),
        option_type: OptType::Call, // ไม่ใช้ — stock ไม่ใช่ option
        strike: 0.0,
        multiplier: 1.0,
    };
    Observation {
        contract,
        kind: ObsKind::StockQuote {
            price: pos(update.price),
            open: pos(update.open),
            high: pos(update.high),
            low: pos(update.low),
            pre_close: pos(update.pre_close),
            volume: pos(update.volume),
        },
        session_date: anchor.session_date,
        event_time: anchor.event_time,
        received_time: received_at,
        available_at: anchor.event_time,
        source: update.src,
        backfill: anchor.backfill,
        universe_id: Some(universe_id.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn received() -> DateTime<Utc> {
        "2026-11-21T02:00:00Z".parse().unwrap() // 09:00 ไทย = หลังปิดตลาด US
    }

    #[test]
    fn plan_separates_discovered_universe_from_subscribed_subset() {
        let rows: Vec<Value> = [
            "NVDA261204C00050000",
            "NVDA261204P00175000",
            "NVDA261204C00180000",
        ]
        .iter()
        .map(|s| json!({"symbol": s}))
        .collect();
        let windows = [WindowCapture {
            expiry_range: (
                chrono::NaiveDate::from_ymd_opt(2026, 12, 4).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 12, 10).unwrap(),
            ),
            rows,
            has_cursor: false,
        }];
        let plan = plan_capture("NVDA", 176.0, &windows, "near-spot-v1", received());
        let cov = &plan.coverage[0];
        assert_eq!(cov.discovered_occ.len(), 3);
        assert_eq!(cov.rows, 3);
        assert!(!cov.possibly_truncated);
        // subscribed = subset ใกล้ spot เท่านั้น ไม่ใช่ทั้ง universe
        assert!(plan.subscribed_occ.len() <= 3);
        assert!(
            plan.subscribed_occ
                .contains(&"NVDA261204P00175000".to_string())
        );
        // coverage ของหน้าต่างนี้กรองไว้เฉพาะที่ค้นพบในหน้าต่างนี้
        for s in &cov.subscribed_occ {
            assert!(cov.discovered_occ.contains(s));
        }
    }

    #[test]
    fn plan_flags_truncated_window_at_page_cap() {
        let rows: Vec<Value> = (0..1000)
            .map(|i| json!({"symbol": format!("NVDA261204C{:08}", 100 + i)}))
            .collect();
        let windows = [WindowCapture {
            expiry_range: (
                chrono::NaiveDate::from_ymd_opt(2026, 12, 4).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 12, 10).unwrap(),
            ),
            rows,
            has_cursor: false,
        }];
        let plan = plan_capture("NVDA", 176.0, &windows, "near-spot-v1", received());
        assert!(plan.coverage[0].possibly_truncated);
        assert_eq!(plan.coverage[0].rows, 1000);
    }

    #[test]
    fn option_observations_map_zero_fields_to_none() {
        let rows = [json!({
            "symbol": "NVDA261204P00175000",
            "price": 2.15, "bid": 2.10, "ask": 2.20, "open_interest": 500, "volume": 0.0
        })];
        let obs = option_observations(&rows, &Anchor::at_observation(received()), received(), "u1");
        assert_eq!(obs.len(), 1);
        match &obs[0].kind {
            ObsKind::OptionQuote {
                price,
                bid,
                ask,
                oi,
                volume,
                ..
            } => {
                assert_eq!(*price, Some(2.15));
                assert_eq!(*oi, Some(500.0));
                // volume 0 จาก API = ไม่ทราบ ไม่ใช่ศูนย์จริง
                assert_eq!(*volume, None);
                assert_eq!(*bid, Some(2.10));
                assert_eq!(*ask, Some(2.20));
            }
            other => panic!("wrong kind {other:?}"),
        }
        assert_eq!(obs[0].universe_id.as_deref(), Some("u1"));
        // ตลาดปิด (02:00 UTC = 21:00 ET ก่อนหน้า) → session ของเช้าเอเชีย
        // คือวันที่ 20 (session ที่เพิ่งปิดไป)
        assert_eq!(
            obs[0].session_date,
            chrono::NaiveDate::from_ymd_opt(2026, 11, 20).unwrap()
        );
    }

    #[test]
    fn post_close_anchor_marks_backfill_and_friday_session() {
        // เสาร์เช้าเวลาไทยเก็บ "ราคาปิดศุกร์" — session = ศุกร์ backfill = true
        let friday = chrono::NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
        let anchor = Anchor::post_close(friday);
        let saturday_received: DateTime<Utc> = "2026-11-21T02:00:00Z".parse().unwrap();
        assert!(anchor.backfill);
        assert_eq!(anchor.session_date, friday);
        // event = ปิดตลาดศุกร์ 21:00 UTC · received หลังจากนั้น → ผ่าน validator
        assert!(saturday_received > anchor.event_time);

        let update = StockUpdate {
            price: 176.0,
            open: 0.0,
            high: 0.0,
            low: 0.0,
            pre_close: 174.5,
            volume: 0.0,
            change: 0.0,
            change_ratio: 0.0,
            src: DataSource::Rest,
        };
        let o = stock_observation("NVDA", &update, &anchor, saturday_received, "u1");
        assert!(crate::lab::quality::validate_observation(&o).is_ok());
        assert!(o.backfill);
        assert_eq!(o.session_date, friday);
    }

    #[test]
    fn stock_observation_keeps_pre_close() {
        let update = StockUpdate {
            price: 176.0,
            open: 175.0,
            high: 177.0,
            low: 174.0,
            pre_close: 174.5,
            volume: 1_000_000.0,
            change: 1.5,
            change_ratio: 0.0086,
            src: DataSource::Rest,
        };
        let o = stock_observation(
            "NVDA",
            &update,
            &Anchor::at_observation(received()),
            received(),
            "u1",
        );
        match &o.kind {
            ObsKind::StockQuote {
                price, pre_close, ..
            } => {
                assert_eq!(*price, Some(176.0));
                assert_eq!(*pre_close, Some(174.5));
            }
            other => panic!("wrong kind {other:?}"),
        }
    }
}
