//! Quality checks — validate / dedupe / gap classification / truncation /
//! spot-based selection (PLAN-LAB.md P8.3 + amendments §2–3)
//!
//! ทุกฟังก์ชัน pure + deterministic; เหตุผลปฏิเสธเป็นภาษาไทยเสมอ

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use chrono_tz::America::New_York;

use super::types::{ContractRef, DataGap, GapKind, ObsKind, Observation};

/// เหตุผลปฏิเสธเป็นไทย — ห้าม panic กับข้อมูลเสีย
#[derive(Debug, Clone, PartialEq)]
pub struct Rejection {
    pub field: &'static str,
    pub reason_th: String,
}

/// วันเทรด NY ของ event time ใดๆ (แท่งปิดรายวัน stamp เที่ยงคืน ET
/// ก็ตกวันเดียวกับ quote กลางวันวันนั้น — DST จัดการโดย chrono-tz)
pub fn session_date_of(event_time: DateTime<Utc>) -> NaiveDate {
    event_time.with_timezone(&New_York).date_naive()
}

/// แท่งวันปิดเริ่มใช้ได้หลังปิด session เท่านั้น (amendments §2:
/// "daily bar session date ไม่ใช่ available-at") — 16:00 NY ของวันนั้น
pub fn bar_available_at(session: NaiveDate) -> DateTime<Utc> {
    let naive = session
        .and_hms_opt(16, 0, 0)
        .expect("16:00 is always valid");
    New_York
        .from_local_datetime(&naive)
        .single()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|| naive.and_utc())
}

/// ตรวจ contract พื้นฐาน: multiplier ต้อง > 0 (ไม่ทราบ = ปฏิเสธ
/// ไม่ใช่เดา 100 เงียบๆ — nonstandard deliverable ต้องถูก block)
pub fn validate_contract(c: &ContractRef) -> Result<(), Rejection> {
    if c.multiplier <= 0.0 || !c.multiplier.is_finite() {
        return Err(Rejection {
            field: "multiplier",
            reason_th: format!(
                "multiplier ไม่ valid ({}) — ต้องยืนยันค่าจริง ห้ามสมมติ",
                c.multiplier
            ),
        });
    }
    if c.strike <= 0.0 || !c.strike.is_finite() {
        return Err(Rejection {
            field: "strike",
            reason_th: format!("strike ไม่ valid ({})", c.strike),
        });
    }
    if c.occ.replace(' ', "").len() < 10 {
        return Err(Rejection {
            field: "occ",
            reason_th: format!("OCC symbol สั้นผิดรูป: {}", c.occ),
        });
    }
    Ok(())
}

/// ตรวจ observation ครบข้อบังคับ: เวลา 4 ชั้นต้องสอดคล้อง และ
/// received ต้องไม่ก่อน event; bar ต้อง available หลังปิด session
pub fn validate_observation(o: &Observation) -> Result<(), Rejection> {
    // stock ไม่ใช่ option contract — ตรวจเฉพาะ identity พื้นฐาน ไม่ใช้เกณฑ์ strike/OCC
    match &o.kind {
        ObsKind::StockQuote { .. } => {
            if o.contract.underlying.trim().is_empty() {
                return Err(Rejection {
                    field: "underlying",
                    reason_th: "stock observation ไม่มีชื่อ underlying".to_string(),
                });
            }
            if !o.contract.multiplier.is_finite() || o.contract.multiplier <= 0.0 {
                return Err(Rejection {
                    field: "multiplier",
                    reason_th: "multiplier ไม่ valid — ต้องเป็นจำนวนจริงบวก".to_string(),
                });
            }
        }
        _ => validate_contract(&o.contract)?,
    }
    if o.received_time < o.event_time {
        return Err(Rejection {
            field: "received_time",
            reason_th: format!(
                "received ({}) ก่อน event ({}) — ข้อมูลย้อนเวลาผิดปกติ",
                o.received_time, o.event_time
            ),
        });
    }
    if o.session_date != session_date_of(o.event_time) {
        return Err(Rejection {
            field: "session_date",
            reason_th: format!(
                "session_date {} ไม่ตรงวันเทรด NY ของ event_time {}",
                o.session_date,
                session_date_of(o.event_time)
            ),
        });
    }
    // แท่งวันปิด: ระบบที่ได้ข้อมูลก่อนเวลาปิดจริงคือ look-ahead — ปฏิเสธ
    if matches!(o.kind, ObsKind::OptionBar { .. })
        && o.available_at < bar_available_at(o.session_date)
    {
        return Err(Rejection {
            field: "available_at",
            reason_th: format!(
                "bar ของ session {} มี available_at ก่อนปิดตลาด — ห้ามใช้เป็น feature ก่อนเวลาจริง",
                o.session_date
            ),
        });
    }
    // ตัวเลขที่มาต้องไม่ NaN/Infinity ทุกช่องที่มีค่า
    let numeric_fields: Vec<(String, f64)> = match &o.kind {
        ObsKind::OptionQuote {
            price,
            bid,
            ask,
            iv,
            delta,
            oi,
            volume,
        } => [
            ("price", price),
            ("bid", bid),
            ("ask", ask),
            ("iv", iv),
            ("delta", delta),
            ("oi", oi),
            ("volume", volume),
        ]
        .iter()
        .filter_map(|(n, v)| v.map(|x| (n.to_string(), x)))
        .collect(),
        ObsKind::OptionBar {
            open,
            high,
            low,
            close,
            volume,
        } => [
            ("open", open),
            ("high", high),
            ("low", low),
            ("close", close),
            ("volume", volume),
        ]
        .iter()
        .filter_map(|(n, v)| v.map(|x| (n.to_string(), x)))
        .collect(),
        ObsKind::StockQuote {
            price,
            open,
            high,
            low,
            pre_close,
            volume,
        } => [
            ("price", price),
            ("open", open),
            ("high", high),
            ("low", low),
            ("pre_close", pre_close),
            ("volume", volume),
        ]
        .iter()
        .filter_map(|(n, v)| v.map(|x| (n.to_string(), x)))
        .collect(),
    };
    for (name, v) in numeric_fields {
        if !v.is_finite() {
            return Err(Rejection {
                field: "numeric",
                reason_th: format!("ค่า {} = {} ไม่ใช่ตัวเลขจริง (NaN/Infinity)", name, v),
            });
        }
    }
    Ok(())
}

/// คีย์ dedupe — คนละชนิด/เวลา = คนละจุดข้อมูล
fn dedupe_key(o: &Observation) -> (String, &'static str, DateTime<Utc>) {
    (o.contract.occ.clone(), o.kind.kind_tag(), o.event_time)
}

/// Dedupe แบบ deterministic: เก็บตัวที่ received เร็วสุด
/// (tie → ลำดับ input แรกสุด) คืน (ที่เก็บ, ที่ตัด) ไม่ทำลายข้อมูลเงียบๆ
pub fn dedupe_observations(obs: Vec<Observation>) -> (Vec<Observation>, Vec<Observation>) {
    let mut best: BTreeMap<(String, &'static str, DateTime<Utc>), usize> = BTreeMap::new();
    let mut keep: Vec<Observation> = Vec::with_capacity(obs.len());
    let mut dropped = Vec::new();
    for o in obs {
        let k = dedupe_key(&o);
        match best.get(&k) {
            Some(&idx) => {
                let incumbent_is_older = keep[idx].received_time <= o.received_time;
                if incumbent_is_older {
                    dropped.push(o);
                } else {
                    dropped.push(keep[idx].clone());
                    keep[idx] = o;
                }
            }
            None => {
                best.insert(k, keep.len());
                keep.push(o);
            }
        }
    }
    (keep, dropped)
}

/// ตรวจลำดับแท่ง: คืน true เมื่อ input เรียงเวลาถูกต้องแล้ว
/// (Webull bars ต้อง sort เอง — Python ต้นฉบับ sort ด้วย str ของ time)
pub fn bars_sorted_by_time(bars: &[(String, f64)]) -> bool {
    bars.windows(2).all(|w| w[0].0 <= w[1].0)
}

/// จำกัดหน้า discovery: ชน cap โดยไม่มี cursor = ไม่รับรองว่าครบ
/// (probe P8.2 เจอจริง: 1000 แถว ไม่มี pagination_key)
pub fn detect_truncation(rows: usize, page_cap: usize, has_cursor: bool) -> bool {
    rows >= page_cap && !has_cursor
}

/// แยกชนิดช่องว่างข้อมูลรายวัน (amendments §2) — ห้ามเติมราคาแทน:
/// วันที่อยู่ในช่วง outage ของ capture = CaptureOutage,
/// นอกนั้นที่ควรมีแต่ไม่มี = UnknownGap (ไม่มีปฏิทิน exchange
/// จึงไม่อ้างว่าเป็น no-trade ที่ยืนยันแล้ว)
pub fn classify_gaps(
    occ: &str,
    expected_sessions: &[NaiveDate],
    have: &BTreeSet<NaiveDate>,
    outages: &[(DateTime<Utc>, DateTime<Utc>)],
) -> Vec<DataGap> {
    expected_sessions
        .iter()
        .filter(|d| !have.contains(*d))
        .map(|&d| {
            // session เช้า 09:30 NY ของวันนั้นเป็นตัวแทนช่วงเวลาที่ควรมีข้อมูล
            let naive = d.and_hms_opt(9, 30, 0).expect("09:30 is always valid");
            let session_start = New_York
                .from_local_datetime(&naive)
                .single()
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or_else(|| Utc.from_utc_datetime(&naive));
            let in_outage = outages
                .iter()
                .any(|(a, b)| session_start >= *a && session_start <= *b);
            DataGap {
                occ: occ.to_string(),
                session_date: d,
                kind: if in_outage {
                    GapKind::CaptureOutage
                } else {
                    GapKind::UnknownGap
                },
            }
        })
        .collect()
}

/// ผลเลือกสัญญาด้วย spot ที่ valid เท่านั้น
#[derive(Debug, Clone, PartialEq)]
pub struct SpotPicks {
    pub atm_put: ContractRef,
    pub atm_call: ContractRef,
    pub otm_put: ContractRef,
    pub otm_call: ContractRef,
}

/// เลือก ATM/OTM ด้วย moneyness — **spot ต้อง > 0 เสมอ**
/// (probe P8.2 เดา field ผิดได้ spot=0 แล้วตกไป strike ต่ำสุด:
/// amendments §3 สั่งให้เป็น error ไม่ใช่ fallback)
pub fn select_near_spot(
    contracts: &[ContractRef],
    spot: f64,
    otm_target: f64,
) -> Result<SpotPicks, Rejection> {
    if !spot.is_finite() || spot <= 0.0 {
        return Err(Rejection {
            field: "spot",
            reason_th: format!("spot ไม่ valid ({spot}) — ห้ามเลือกสัญญาด้วย moneyness เมื่อไม่มีราคาหุ้นจริง"),
        });
    }
    if !(0.05..=0.95).contains(&otm_target) {
        return Err(Rejection {
            field: "otm_target",
            reason_th: format!("otm_target ควรอยู่ช่วง 0.05–0.95 ของ spot (ได้ {otm_target})"),
        });
    }
    let mut valid = Vec::new();
    for c in contracts {
        if validate_contract(c).is_ok() {
            valid.push(c);
        }
    }
    let nearest = |target: f64, want_put: Option<bool>| -> Option<&ContractRef> {
        valid
            .iter()
            .filter(|c| want_put.is_none_or(|p| (c.option_type == crate::quant::OptType::Put) == p))
            .min_by(|a, b| {
                (a.strike - target)
                    .abs()
                    .partial_cmp(&(b.strike - target).abs())
                    .unwrap()
            })
            .copied()
    };
    match (
        nearest(spot, Some(true)),
        nearest(spot, Some(false)),
        nearest(spot * otm_target, Some(true)),
        // OTM call สมมาตรของ put: put 0.85×spot → call 1.15×spot
        nearest(spot * (2.0 - otm_target), Some(false)),
    ) {
        (Some(atm_put), Some(atm_call), Some(otm_put), Some(otm_call)) => Ok(SpotPicks {
            atm_put: atm_put.clone(),
            atm_call: atm_call.clone(),
            otm_put: otm_put.clone(),
            otm_call: otm_call.clone(),
        }),
        _ => Err(Rejection {
            field: "contracts",
            reason_th: "สัญญาไม่ครบทั้งสี่ตำแหน่ง (ATM/OTM put+call) จาก universe ที่ให้มา".to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::types::{ContractRef, ObsKind, Observation};
    use crate::quant::OptType;

    fn contract(occ: &str, strike: f64, _opt: OptType) -> ContractRef {
        let m = crate::webull::contracts::parse_occ(occ).expect("valid occ in fixture");
        ContractRef {
            occ: occ.to_string(),
            underlying: m.underlying,
            expiry: m.expiry,
            option_type: m.option_type,
            strike,
            multiplier: 100.0,
        }
    }

    fn obs(event: &str, received: &str, kind: ObsKind) -> Observation {
        let event_time: DateTime<Utc> = event.parse().unwrap();
        let session = session_date_of(event_time);
        Observation {
            contract: contract("NVDA261204P00175000", 175.0, OptType::Put),
            kind,
            session_date: session,
            event_time: event_time,
            received_time: received.parse().unwrap(),
            available_at: bar_available_at(session),
            source: crate::engine::DataSource::Rest,
            backfill: false,
            universe_id: None,
        }
    }

    // ---- validate_observation ----

    #[test]
    fn reject_zero_multiplier_with_thai_reason() {
        let mut c = contract("NVDA261204P00175000", 175.0, OptType::Put);
        c.multiplier = 0.0;
        let err = validate_contract(&c).unwrap_err();
        assert!(err.reason_th.contains("multiplier"), "{err:?}");
    }

    #[test]
    fn reject_received_before_event() {
        let o = obs("2026-11-20T22:00:00Z", "2026-11-20T21:00:00Z", stock_kind());
        let err = validate_observation(&o).unwrap_err();
        assert_eq!(err.field, "received_time");
        assert!(err.reason_th.contains("ก่อน event"));
    }

    #[test]
    fn reject_wrong_session_date() {
        let mut o = obs("2026-11-20T22:00:00Z", "2026-11-20T22:01:00Z", stock_kind());
        o.session_date = NaiveDate::from_ymd_opt(2026, 11, 19).unwrap();
        assert_eq!(validate_observation(&o).unwrap_err().field, "session_date");
    }

    #[test]
    fn reject_bar_available_before_session_close() {
        let event: DateTime<Utc> = "2026-11-20T05:00:00Z".parse().unwrap(); // เที่ยงคืน ET 20 พ.ย.
        let session = session_date_of(event);
        let mut o = obs("2026-11-20T05:00:00Z", "2026-11-20T12:00:00Z", bar_kind());
        o.available_at = event; // ก่อนปิดตลาด 21:00 UTC — ต้องโดนปฏิเสธ
        assert!(session == NaiveDate::from_ymd_opt(2026, 11, 20).unwrap());
        assert_eq!(validate_observation(&o).unwrap_err().field, "available_at");
    }

    #[test]
    fn accept_bar_available_at_close() {
        let o = obs("2026-11-20T05:00:00Z", "2026-11-20T22:00:00Z", bar_kind());
        assert!(validate_observation(&o).is_ok());
    }

    #[test]
    fn reject_nan_price() {
        let kind = ObsKind::OptionQuote {
            price: Some(f64::NAN),
            bid: None,
            ask: None,
            iv: None,
            delta: None,
            oi: None,
            volume: None,
        };
        let o = obs("2026-11-20T22:00:00Z", "2026-11-20T22:01:00Z", kind);
        let err = validate_observation(&o).unwrap_err();
        assert!(err.reason_th.contains("NaN"));
    }

    // ---- DST: session_date กลางฤดูร้อน (04:00 UTC) กับหน้าหนาว (05:00 UTC) ----

    #[test]
    fn dst_session_dates_match_ny_calendar() {
        let summer: DateTime<Utc> = "2026-07-15T04:00:00Z".parse().unwrap(); // 00:00 EDT
        let winter: DateTime<Utc> = "2026-11-18T05:00:00Z".parse().unwrap(); // 00:00 EST
        assert_eq!(
            session_date_of(summer),
            NaiveDate::from_ymd_opt(2026, 7, 15).unwrap()
        );
        assert_eq!(
            session_date_of(winter),
            NaiveDate::from_ymd_opt(2026, 11, 18).unwrap()
        );
        // ปิด session ฤดูร้อน = 20:00 UTC, หน้าหนาว = 21:00 UTC
        let s = bar_available_at(NaiveDate::from_ymd_opt(2026, 7, 15).unwrap());
        let w = bar_available_at(NaiveDate::from_ymd_opt(2026, 11, 18).unwrap());
        assert_eq!(s.to_string(), "2026-07-15 20:00:00 UTC");
        assert_eq!(w.to_string(), "2026-11-18 21:00:00 UTC");
    }

    // ---- dedupe ----

    #[test]
    fn dedupe_keeps_earliest_received_and_reports_dropped() {
        let a = obs("2026-11-20T22:00:00Z", "2026-11-20T22:00:10Z", stock_kind());
        let b = obs("2026-11-20T22:00:00Z", "2026-11-20T22:05:00Z", stock_kind());
        let c = obs("2026-11-20T23:00:00Z", "2026-11-20T23:00:10Z", stock_kind());
        let (keep, dropped) = dedupe_observations(vec![a, b, c]);
        assert_eq!(keep.len(), 2);
        assert_eq!(dropped.len(), 1);
        assert_eq!(
            dropped[0].received_time.to_string(),
            "2026-11-20 22:05:00 UTC"
        );
        // คนละชนิดไม่ถูก dedupe รวมกัน
        let d = obs("2026-11-20T22:00:00Z", "2026-11-20T22:00:20Z", bar_kind());
        let (keep2, _) = dedupe_observations(vec![d]);
        assert_eq!(keep2.len(), 1);
    }

    // ---- bars order + truncation ----

    #[test]
    fn bars_sorted_detection() {
        assert!(bars_sorted_by_time(&[
            ("100".into(), 1.0),
            ("200".into(), 2.0)
        ]));
        assert!(!bars_sorted_by_time(&[
            ("200".into(), 2.0),
            ("100".into(), 1.0)
        ]));
    }

    #[test]
    fn truncation_flag_matches_probe_finding() {
        assert!(detect_truncation(1000, 1000, false));
        assert!(!detect_truncation(1000, 1000, true));
        assert!(!detect_truncation(486, 1000, false));
    }

    // ---- gaps ----

    #[test]
    fn gaps_classified_by_outage_window_and_never_filled() {
        let occ = "NVDA261204P00175000";
        let expected: Vec<NaiveDate> = ["2026-11-18", "2026-11-19", "2026-11-20"]
            .iter()
            .map(|s| s.parse().unwrap())
            .collect();
        let have: BTreeSet<NaiveDate> = ["2026-11-18".parse::<NaiveDate>().unwrap()].into();
        // แอปปิดทั้งวันที่ 20 (outage ครอบ session)
        let outages = [(
            "2026-11-20T00:00:00Z".parse::<DateTime<Utc>>().unwrap(),
            "2026-11-20T23:59:59Z".parse::<DateTime<Utc>>().unwrap(),
        )];
        let gaps = classify_gaps(occ, &expected, &have, &outages);
        assert_eq!(gaps.len(), 2);
        let by_date: BTreeMap<_, _> = gaps.iter().map(|g| (g.session_date, &g.kind)).collect();
        assert_eq!(
            by_date[&NaiveDate::from_ymd_opt(2026, 11, 19).unwrap()],
            &GapKind::UnknownGap
        );
        assert_eq!(
            by_date[&NaiveDate::from_ymd_opt(2026, 11, 20).unwrap()],
            &GapKind::CaptureOutage
        );
    }

    // ---- spot-based selection ----

    #[test]
    fn reject_selection_when_spot_invalid_no_fallback() {
        let contracts = vec![
            contract("NVDA261204P00050000", 50.0, OptType::Put),
            contract("NVDA261204C00050000", 50.0, OptType::Call),
        ];
        let err = select_near_spot(&contracts, 0.0, 0.85).unwrap_err();
        assert_eq!(err.field, "spot");
        assert!(err.reason_th.contains("ห้ามเลือกสัญญาด้วย moneyness"));
    }

    #[test]
    fn select_atm_otm_with_valid_spot() {
        let contracts = vec![
            contract("NVDA261204P00170000", 170.0, OptType::Put),
            contract("NVDA261204P00175000", 175.0, OptType::Put),
            contract("NVDA261204C00180000", 180.0, OptType::Call),
            contract("NVDA261204C00195000", 195.0, OptType::Call),
        ];
        let picks = select_near_spot(&contracts, 176.0, 0.85).unwrap();
        assert_eq!(picks.atm_put.strike, 175.0);
        assert_eq!(picks.atm_call.strike, 180.0);
        assert_eq!(picks.otm_put.strike, 170.0);
        assert_eq!(picks.otm_call.strike, 195.0); // ~176*1.15=202 → ใกล้สุดคือ 195
    }

    fn stock_kind() -> ObsKind {
        ObsKind::StockQuote {
            price: Some(176.0),
            open: None,
            high: None,
            low: None,
            pre_close: Some(174.5),
            volume: None,
        }
    }

    fn bar_kind() -> ObsKind {
        ObsKind::OptionBar {
            open: Some(2.0),
            high: Some(2.2),
            low: Some(1.9),
            close: Some(2.1),
            volume: Some(10.0),
        }
    }
}
