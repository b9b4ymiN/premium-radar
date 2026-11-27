//! Capture builder — เปลี่ยนข้อมูลดิบที่ fetch มาแล้วเป็น Lab records
//! (P8.4: universe ก่อนคัดกรอง + coverage แยกจาก subset ที่ขอ quotes)
//!
//! Pure functions ทั้งหมด — HTTP อยู่ที่ caller (recorder/แอป)

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;

use crate::engine::{DataSource, StockUpdate, parse_quote_update};
use crate::lab::quality::{detect_truncation, session_date_of};
use crate::lab::types::{ContractRef, DiscoveryCoverage, ObsKind, Observation};
use crate::quant::OptType;
use crate::webull::contracts::parse_occ;
use crate::webull::discovery::pick_ws_symbols;
use crate::webull::rest::{MAX_CONTRACTS, PAGE_CAP};

/// งบ put (จำนวน contract) ของ rule `put-depth-v1` — ส่วนที่เหลือจาก
/// [`MAX_CONTRACTS`] ทั้งหมดไปให้ call ใกล้ spot
const PUT_DEPTH_PUT_SHARE: usize = 64;
/// ความลึกของแถบ put ของ rule `put-depth-v1` (สัดส่วนของ spot ใต้ ATM)
const PUT_DEPTH_DEPTH: f64 = 0.20;

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
    /// OCC ที่เลือกขอ quotes ตาม `selection_rule` — near-spot-v1 = ใกล้ spot
    /// ตาม limit เดิมของ scanner ([`pick_ws_symbols`]); put-depth-v1 =
    /// [`pick_put_depth`] (put ลึก, งบเท่ากัน)
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
    // near-spot-v1 = พฤติกรรมเดิมเป๊ะๆ; rule ที่ไม่รู้จัก fall back มาที่
    // near-spot-v1 เพื่อไม่ให้ coverage record อ้าง rule ที่ไม่มี implementation
    let subscribed_occ = match selection_rule {
        "put-depth-v1" => pick_put_depth(
            &all_occ,
            spot,
            MAX_CONTRACTS,
            PUT_DEPTH_PUT_SHARE,
            PUT_DEPTH_DEPTH,
        ),
        _ => pick_ws_symbols(&all_occ, spot, 0),
    };
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

/// ลำดับ index ของ band เรียงตาม "ควรเลือกก่อน → หลัง" แบบที่ prefix ยาว
/// เท่าไรก็ span ทั้งแถบอย่างสม่ำเสมอ: index สุดท้าย (strike ใกล้ spot สุด)
/// → index 0 (strike ลึกสุด) → แล้วแบ่งช่องว่างตาม index ที่กว้างสุดครึ่งๆ
/// ซ้ำๆ (ชนกันเอาช่องแรก = ฝั่งลึก) — deterministic และปรับตัวตามการกระจาย
/// strike ที่หนาแน่นใกล้ spot อัตโนมัติ
fn spanning_indices(n: usize) -> Vec<usize> {
    match n {
        0 => return Vec::new(),
        1 => return vec![0],
        _ => {}
    }
    let mut order = vec![n - 1, 0];
    let mut sorted = vec![0usize, n - 1];
    while sorted.len() < n {
        let mut best = 0;
        let mut best_gap = 0;
        for (k, w) in sorted.windows(2).enumerate() {
            if w[1] - w[0] > best_gap {
                best_gap = w[1] - w[0];
                best = k;
            }
        }
        // กันวนไม่รู้จบ: ยังขาด index = ต้องมี gap ≥ 2 อยู่เสมอ
        if best_gap < 2 {
            break;
        }
        let mid = (sorted[best] + sorted[best + 1]) / 2;
        sorted.insert(best + 1, mid);
        order.push(mid);
    }
    order
}

/// เลือก contract ให้ put ลึกพอสำหรับ cash-secured put screening (rule
/// `put-depth-v1`): งบรวมเท่ากับ near-spot-v1 แต่กัน `put_share` ตัวให้
/// put แทนการเอาใกล้ spot ทั้งหมด
///
/// กติกา:
/// - put กระจายงบ `put_share` ข้าม expiry แบบ round-robin (expiry ใกล้สุด
///   ก่อน — รวม nearest ITM put หนึ่งตัวต่อ expiry อยู่ในงบเดียวกัน เพื่อให้
///   บริเวณใกล้ spot ยังถูกเก็บ) ลำดับที่เลือกต่อ expiry: nearest ITM put →
///   band strike ใกล้ spot สุด → band strike ลึกสุด → เติมระหว่างกลางแบบ
///   spacing สม่ำเสมอ ([`spanning_indices`]) — ไม่ได้เอาเฉพาะใกล้ spot
/// - band ของ put = OTM/ATM ที่ `spot*(1-depth) <= strike <= spot`
/// - call = งบที่เหลือ (`total` − จำนวน put ที่เลือกได้) ตาม ranking เดียวกับ
///   [`pick_ws_symbols`] จำกัดเฉพาะฝั่ง call
/// - ผลลัพธ์ไม่เกิน `total` ตัว ไม่ซ้ำ และเรียงตาม (expiry, type, strike)
///   เหมือน sort ของ `select_symbols`
pub fn pick_put_depth(
    syms: &[String],
    spot: f64,
    total: usize,
    put_share: usize,
    depth: f64,
) -> Vec<String> {
    // (expiry → [(strike, symbol)]) เฉพาะ put — BTreeMap เรียง expiry ใกล้สุดก่อน
    let mut puts: BTreeMap<NaiveDate, Vec<(f64, &String)>> = BTreeMap::new();
    let mut call_syms: Vec<String> = Vec::new();
    for sy in syms {
        let Some(m) = parse_occ(sy) else { continue };
        match m.option_type {
            OptType::Put => puts.entry(m.expiry).or_default().push((m.strike, sy)),
            OptType::Call => call_syms.push(sy.clone()),
        }
    }

    let lo = spot * (1.0 - depth);
    let expiry_seqs: Vec<Vec<&String>> = puts
        .values()
        .map(|raw| {
            // เรียง strike น้อย→มาก; symbol ต่างกันที่ชน (expiry, strike) เดียวกัน
            // เก็บตัวที่เรียง dictionary น้อยกว่าเพื่อ deterministic
            let mut by_strike = raw.clone();
            by_strike.sort_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.1.cmp(b.1))
            });
            by_strike.dedup_by(|a, b| a.0 == b.0);

            let mut seq: Vec<&String> = Vec::new();
            // nearest ITM put ตัวเดียว: strike แรกที่มากกว่า spot (เรียง ascending)
            if let Some((_, s)) = by_strike.iter().find(|(k, _)| *k > spot) {
                seq.push(*s);
            }
            let band: Vec<&String> = by_strike
                .iter()
                .filter(|(k, _)| *k >= lo && *k <= spot)
                .map(|(_, s)| *s)
                .collect();
            seq.extend(spanning_indices(band.len()).into_iter().map(|i| band[i]));
            seq
        })
        .collect();

    // round-robin ตาม expiry (ใกล้สุดก่อน) จนกว่างบ put หรือ candidate หมด —
    // เศษงบที่เหลือไหลไปให้ call ท้ายฟังก์ชัน
    let put_cap = put_share.min(total);
    let mut chosen_puts: Vec<&String> = Vec::new();
    let mut cursors = vec![0usize; expiry_seqs.len()];
    'spread: loop {
        let mut advanced = false;
        for (i, seq) in expiry_seqs.iter().enumerate() {
            if chosen_puts.len() >= put_cap {
                break 'spread;
            }
            if let Some(&s) = seq.get(cursors[i]) {
                chosen_puts.push(s);
                cursors[i] += 1;
                advanced = true;
            }
        }
        if !advanced {
            break 'spread;
        }
    }

    // 0 มีค่าพิเศษใน pick_ws_symbols (= MAX_CONTRACTS) — remaining เป็น 0
    // ต้องไม่ส่งเข้าไป
    let remaining = total.saturating_sub(chosen_puts.len());
    let calls = if remaining > 0 {
        pick_ws_symbols(&call_syms, spot, remaining)
    } else {
        Vec::new()
    };

    let mut keyed: Vec<(NaiveDate, &str, f64, String)> =
        Vec::with_capacity(chosen_puts.len() + calls.len());
    for s in chosen_puts.into_iter().cloned().chain(calls) {
        if let Some(m) = parse_occ(&s) {
            keyed.push((m.expiry, m.option_type.as_str(), m.strike, s));
        }
    }
    keyed.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(b.1))
            .then_with(|| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.3.cmp(&b.3))
    });
    let mut out: Vec<String> = keyed.into_iter().map(|(_, _, _, s)| s).collect();
    out.dedup();
    out
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
            symbol: "NVDA".into(),
            price: 176.0,
            open: 0.0,
            high: 0.0,
            low: 0.0,
            pre_close: 174.5,
            volume: 0.0,
            change: Some(0.0),
            change_ratio: Some(0.0),
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
            symbol: "NVDA".into(),
            price: 176.0,
            open: 175.0,
            high: 177.0,
            low: 174.0,
            pre_close: 174.5,
            volume: 1_000_000.0,
            change: Some(1.5),
            change_ratio: Some(0.0086),
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

    // ---------- put-depth-v1 ----------

    const TEST_EXPIRIES: [&str; 4] = ["2026-12-04", "2026-12-11", "2026-12-18", "2027-01-15"];

    /// chain สังเคราะห์: 4 expiry × strikes 70..=130 (step 1) ทั้ง call/put
    fn synthetic_chain(expiries: &[&str]) -> Vec<String> {
        let mut syms = Vec::new();
        for exp in expiries {
            let ymd = &exp.replace('-', "")[2..];
            for k in 70..=130i64 {
                for t in ["C", "P"] {
                    syms.push(format!("A{ymd}{t}{:08}", k * 1000));
                }
            }
        }
        syms
    }

    fn exp_date(s: &str) -> chrono::NaiveDate {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn put_depth_spans_band_within_budget() {
        let syms = synthetic_chain(&TEST_EXPIRIES);
        let picked = pick_put_depth(&syms, 100.0, 96, 64, 0.20);
        assert!(picked.len() <= 96);
        // deterministic: เรียกซ้ำได้ลำดับเดิม
        assert_eq!(picked, pick_put_depth(&syms, 100.0, 96, 64, 0.20));
        // ไม่มีตัวซ้ำ
        let mut uniq = picked.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), picked.len());

        let metas: Vec<_> = picked.iter().filter_map(|s| parse_occ(s)).collect();
        let n_puts = metas
            .iter()
            .filter(|m| m.option_type == OptType::Put)
            .count();
        let n_calls = metas
            .iter()
            .filter(|m| m.option_type == OptType::Call)
            .count();
        assert_eq!(n_puts, 64); // งบ put เต็ม (4 expiry × 16 ตัว)
        assert!(n_calls <= 96 - n_puts);

        for exp in TEST_EXPIRIES {
            let put_strikes: Vec<f64> = metas
                .iter()
                .filter(|m| m.option_type == OptType::Put && m.expiry == exp_date(exp))
                .map(|m| m.strike)
                .collect();
            assert_eq!(put_strikes.len(), 16, "expiry {exp}");
            // ทุก expiry ลึกถึง ≥ 18 % ใต้ spot (strike ≤ 82) และมี put
            // ใกล้ spot ที่สุด (ATM 100) ครบ
            assert!(
                put_strikes.iter().copied().fold(f64::MAX, f64::min) <= 82.0,
                "expiry {exp}"
            );
            assert!(put_strikes.contains(&100.0), "expiry {exp}");
        }

        // call ใช้ ranking เดียวกับ pick_ws_symbols จำกัดเฉพาะฝั่ง call
        let call_only: Vec<String> = syms
            .iter()
            .filter(|s| parse_occ(s).is_some_and(|m| m.option_type == OptType::Call))
            .cloned()
            .collect();
        let mut expected_calls = pick_ws_symbols(&call_only, 100.0, 96 - n_puts);
        expected_calls.sort();
        let mut got_calls: Vec<String> = picked
            .iter()
            .filter(|s| parse_occ(s).is_some_and(|m| m.option_type == OptType::Call))
            .cloned()
            .collect();
        got_calls.sort();
        assert_eq!(got_calls, expected_calls);
    }

    #[test]
    fn plan_capture_dispatches_on_selection_rule() {
        let syms = synthetic_chain(&TEST_EXPIRIES);
        // หนึ่ง window ต่อ expiry เพื่อให้ all_occ ตามลำดับที่แน่นอน
        let windows: Vec<WindowCapture> = TEST_EXPIRIES
            .iter()
            .map(|exp| {
                let ymd = &exp.replace('-', "")[2..];
                let rows: Vec<Value> = syms
                    .iter()
                    .filter(|s| s.starts_with('A') && &s[1..7] == ymd)
                    .map(|s| json!({"symbol": s}))
                    .collect();
                WindowCapture {
                    expiry_range: (exp_date(exp), exp_date(exp).succ_opt().unwrap()),
                    rows,
                    has_cursor: false,
                }
            })
            .collect();

        let near = plan_capture("A", 100.0, &windows, "near-spot-v1", received());
        // near-spot-v1 = pick_ws_symbols ตรงๆ (output เดิมไม่เปลี่ยน)
        let mut all_occ: Vec<String> = Vec::new();
        for w in &windows {
            for r in &w.rows {
                if let Some(sym) = r.get("symbol").and_then(Value::as_str) {
                    let clean = sym.replace(' ', "");
                    if parse_occ(&clean).is_some() && !all_occ.contains(&clean) {
                        all_occ.push(clean);
                    }
                }
            }
        }
        assert_eq!(near.subscribed_occ, pick_ws_symbols(&all_occ, 100.0, 0));
        // rule ที่ไม่รู้จัก fall back เป็น near-spot-v1
        let unknown = plan_capture("A", 100.0, &windows, "no-such-rule", received());
        assert_eq!(unknown.subscribed_occ, near.subscribed_occ);

        // put-depth-v1: จำนวน contract เท่าเดิม (≤ 96 → batch 20/ครั้งเท่าเดิม)
        // แต่ put ลึกกว่า near-spot
        let deep = plan_capture("A", 100.0, &windows, "put-depth-v1", received());
        assert_eq!(deep.subscribed_occ.len(), 96);
        assert_eq!(deep.subscribed_occ.len().div_ceil(20), 5);
        assert_eq!(near.subscribed_occ.len().div_ceil(20), 5);
        let deepest_of = |plan: &CapturePlan| {
            plan.subscribed_occ
                .iter()
                .filter_map(|s| parse_occ(s))
                .filter(|m| m.option_type == OptType::Put)
                .map(|m| m.strike)
                .fold(f64::MAX, f64::min)
        };
        assert!(deepest_of(&deep) <= 82.0);
        assert!(deepest_of(&deep) < deepest_of(&near));
        // coverage ยังกรอง subscribed ให้อยู่ใน discovered ของหน้าต่างตัวเอง
        for c in &deep.coverage {
            assert!(!c.subscribed_occ.is_empty());
            for s in &c.subscribed_occ {
                assert!(c.discovered_occ.contains(s));
            }
        }
    }
}
