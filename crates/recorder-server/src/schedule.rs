//! Scheduler — คำนวณ slot ของแต่ละ job ด้วยเขตเวลา America/New_York
//! (chrono-tz ถูกต้องเรื่อง DST อัตโนมัติ) วันเสาร์–อาทิตย์ไม่มี job
//! และเก็บสถานะ "slot นี้วันนี้รันแล้ว" เพื่อไม่ให้ job ซ้อน/ซ้ำในวันเดียว

use std::collections::HashSet;

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, TimeZone, Utc, Weekday};
use chrono_tz::America::New_York;
use radar_core::lab::recorder::CaptureScope;
use radar_core::webull::quotes::OI_BATCH;
use radar_core::webull::rest::MAX_CONTRACTS;

use crate::config::{Config, TimeOfDay};

/// ชนิดของ job ที่ scheduler สร้างได้
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobKind {
    /// หลังปิดตลาด 1 ครั้ง/วัน — Core ∪ Universe, DTE 0–60, ±20%
    Layer1,
    /// ภาพ intraday ของ Core symbols — DTE 0–45, ±15%
    Layer2Core,
    /// ภาพ intraday ของ Universe — DTE 0–45, ±15%
    Layer2Universe,
}

impl JobKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Layer1 => "layer1",
            Self::Layer2Core => "layer2-core",
            Self::Layer2Universe => "layer2-universe",
        }
    }

    /// เลข layer เพื่อนับใน status (1 หรือ 2)
    pub fn layer(self) -> u8 {
        match self {
            Self::Layer1 => 1,
            Self::Layer2Core | Self::Layer2Universe => 2,
        }
    }

    /// scope ของ job ชนิดนี้ตาม product decision
    pub fn scope(self) -> CaptureScope {
        match self {
            Self::Layer1 => CaptureScope::dte_capped(60, 0.20),
            Self::Layer2Core | Self::Layer2Universe => CaptureScope::dte_capped(45, 0.15),
        }
    }
}

/// slot หนึ่ง job — เวลา wall-clock ของ NY + คีย์กันรันซ้ำต่อวัน
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub kind: JobKind,
    pub at_ny: DateTime<chrono_tz::Tz>,
    /// "2026-11-25/layer1" — unique ต่อ (วัน, kind)
    pub key: String,
}

impl Slot {
    pub fn at_utc(&self) -> DateTime<Utc> {
        self.at_ny.with_timezone(&Utc)
    }
}

/// เวลา wall-clock NY ของ date+TimeOfDay — None เมื่อชน DST gap (เช่น 02:30
/// ตอน spring forward) — slot นั้นถูกข้ามไปวันนั้น
fn local_time(ny_date: NaiveDate, t: TimeOfDay) -> Option<DateTime<chrono_tz::Tz>> {
    let ndt = ny_date.and_hms_opt(t.hour, t.minute, 0)?;
    match New_York.from_local_datetime(&ndt) {
        LocalResult::Single(dt) => Some(dt),
        // ช่วง fall-back มีเวลาซ้ำ 2 รอบ — ยึดรอบแรก ให้ไม่พลาด slot
        LocalResult::Ambiguous(dt, _) => Some(dt),
        LocalResult::None => None,
    }
}

/// slot ทั้งหมดของวัน NY หนึ่งวัน เรียงตามเวลา — สุดสัปดาห์ = ว่าง
pub fn slots_on(ny_date: NaiveDate, cfg: &Config) -> Vec<Slot> {
    if matches!(ny_date.weekday(), Weekday::Sat | Weekday::Sun) {
        return Vec::new();
    }
    let mut slots: Vec<(TimeOfDay, JobKind)> = Vec::new();
    for &t in &cfg.layer2_core_times_ny {
        slots.push((t, JobKind::Layer2Core));
    }
    slots.push((cfg.layer2_universe_time_ny, JobKind::Layer2Universe));
    slots.push((cfg.layer1_time_ny, JobKind::Layer1));
    let mut out: Vec<Slot> = slots
        .into_iter()
        .filter_map(|(t, kind)| {
            local_time(ny_date, t).map(|at_ny| Slot {
                kind,
                at_ny,
                // รวมเวลาของ slot — layer2-core มีได้ 3 slot/วัน ต้องไม่แชร์กัน
                key: format!("{}/{}/{}", ny_date, kind.label(), t.as_str()),
            })
        })
        .collect();
    out.sort_by_key(|s| s.at_ny);
    out
}

/// slot ถัดไป `max` ตัวที่เวลา strictly หลัง `after_utc` (สำหรับ `plan`)
pub fn next_slots(after_utc: DateTime<Utc>, cfg: &Config, max: usize) -> Vec<Slot> {
    let after_ny = after_utc.with_timezone(&New_York);
    let mut d = after_ny.date_naive();
    let mut out = Vec::new();
    for _ in 0..14 {
        out.extend(slots_on(d, cfg).into_iter().filter(|s| s.at_ny > after_ny));
        d += Duration::days(1);
        if out.len() >= max {
            break;
        }
    }
    out.sort_by_key(|s| s.at_ny);
    out.truncate(max);
    out
}

/// สถานะของวันที่กำลังรัน — slot ที่รันแล้วจะไม่รันซ้ำในวันเดียว
/// (job ล้นเวลาข้าม slot ถัดไป = รัน slot นั้นทันทีเมื่อ job เดิมจบ)
#[derive(Debug)]
pub struct DayState {
    date: NaiveDate,
    done: HashSet<String>,
}

impl DayState {
    pub fn new(date: NaiveDate) -> Self {
        Self {
            date,
            done: HashSet::new(),
        }
    }

    /// เปลี่ยนวัน (เที่ยงคืน NY) → เคลียร์ done
    fn rollover(&mut self, today: NaiveDate) {
        if self.date != today {
            self.date = today;
            self.done.clear();
        }
    }

    /// slot ถัดไปที่ยังไม่รันวันนี้ (เวลาของ slot อาจต่ำกว่า now = overdue
    /// ให้รันทันที) — วันนี้หมดแล้วมองไปวันถัดไปจนกว่าจะเจอ
    pub fn next_due(&mut self, now_utc: DateTime<Utc>, cfg: &Config) -> Slot {
        let now_ny = now_utc.with_timezone(&New_York);
        self.rollover(now_ny.date_naive());
        let mut d = now_ny.date_naive();
        loop {
            let today = d == now_ny.date_naive();
            if let Some(slot) = slots_on(d, cfg)
                .into_iter()
                .find(|s| !(today && self.done.contains(&s.key)))
            {
                return slot;
            }
            d += Duration::days(1);
        }
    }

    pub fn mark_done(&mut self, slot: &Slot) {
        self.rollover(slot.at_ny.date_naive());
        self.done.insert(slot.key.clone());
    }
}

/// ประมาณ request ต่อ job สำหรับ `plan` — ต่อ symbol:
/// 1 (stock snapshot) + จำนวนหน้าต่าง discovery + quote batches
/// (จำนวนสัญญาที่ subscribe ถูก cap ที่ `MAX_CONTRACTS` = 96 → 5 batch ละ 20)
pub fn estimate_requests(symbols: usize, windows: usize) -> u64 {
    let batches = MAX_CONTRACTS.div_ceil(OI_BATCH);
    symbols as u64 * (1 + windows as u64 + batches as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;
    use chrono_tz::Tz;

    fn cfg() -> Config {
        Config::default()
    }

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    /// mark_done + next_due ต่อ — ลดพิมพ์ซ้ำในเทสลำดับ slot
    fn next_due_after_mark(
        state: &mut DayState,
        just_done: &Slot,
        now: DateTime<Utc>,
        cfg: &Config,
    ) -> Slot {
        state.mark_done(just_done);
        state.next_due(now, cfg)
    }

    #[test]
    fn weekend_days_have_no_slots() {
        let sat = NaiveDate::from_ymd_opt(2026, 11, 28).unwrap(); // เสาร์
        let sun = NaiveDate::from_ymd_opt(2026, 11, 29).unwrap();
        assert!(slots_on(sat, &cfg()).is_empty());
        assert!(slots_on(sun, &cfg()).is_empty());
        // ศุกร์มีครบ 5 slots (3 core + 1 universe + 1 layer1)
        let fri = NaiveDate::from_ymd_opt(2026, 11, 27).unwrap();
        assert_eq!(slots_on(fri, &cfg()).len(), 5);
    }

    #[test]
    fn slots_of_a_day_are_time_ordered_with_expected_utc_offset() {
        // อังคาร 24 พ.ย. 2026 — หลัง DST end (6 พ.ย.) → NY = UTC-5
        let tue = NaiveDate::from_ymd_opt(2026, 11, 24).unwrap();
        let slots = slots_on(tue, &cfg());
        let labels: Vec<&str> = slots.iter().map(|s| s.kind.label()).collect();
        assert_eq!(
            labels,
            vec![
                "layer2-core",
                "layer2-universe",
                "layer2-core",
                "layer2-core",
                "layer1"
            ]
        );
        assert_eq!(slots[0].at_utc(), utc("2026-11-24T15:00:00Z")); // 10:00 NY = 15:00 UTC
        assert_eq!(slots[4].at_utc(), utc("2026-11-24T21:30:00Z")); // 16:30 NY = 21:30 UTC
    }

    #[test]
    fn next_slots_crosses_the_dst_fall_back_weekend_correctly() {
        // เสาร์ 31 ต.ค. 2026 09:00 NY (= 13:00 UTC, ยังเป็น EDT) —
        // DST หมดคืนวันอาทิตย์ 1 พ.ย. → จันทร์ 2 พ.ย. เป็น EST (UTC-5)
        let from = utc("2026-10-31T13:00:00Z");
        let slots = next_slots(from, &cfg(), 5);
        assert_eq!(slots.len(), 5);
        // slot แรก = จันทร์ 2 พ.ย. 10:00 NY → 15:00 UTC (ก่อน DST คือ 14:00)
        assert_eq!(slots[0].kind, JobKind::Layer2Core);
        assert_eq!(slots[0].at_utc(), utc("2026-11-02T15:00:00Z"));
        assert_eq!(
            slots[0].at_ny.format("%Y-%m-%d %H:%M %Z").to_string(),
            "2026-11-02 10:00 EST"
        );
        // slot สุดท้ายของมื้อนั้นคือ Layer 1 16:30 NY = 21:30 UTC
        assert_eq!(slots[4].kind, JobKind::Layer1);
        assert_eq!(slots[4].at_utc(), utc("2026-11-02T21:30:00Z"));
        // ก่อนหน้า DST (ศุกร์ 30 ต.ค.) 16:30 NY ยังเป็น 20:30 UTC
        let fri_slots = slots_on(NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(), &cfg());
        assert_eq!(
            fri_slots.last().unwrap().at_utc(),
            utc("2026-10-30T20:30:00Z")
        );
    }

    #[test]
    fn next_slots_skips_weekend_entirely() {
        // ศุกร์ 27 พ.ย. 17:00 NY — ผ่าน L1 ของวันแล้ว → ตัวถัดไปคือจันทร์ 30
        let from = utc("2026-11-27T22:00:00Z");
        let slots = next_slots(from, &cfg(), 2);
        assert_eq!(
            slots[0].at_ny.date_naive(),
            NaiveDate::from_ymd_opt(2026, 11, 30).unwrap()
        );
        assert_eq!(slots[0].kind, JobKind::Layer2Core);
        assert_eq!(slots[0].at_utc(), utc("2026-11-30T15:00:00Z"));
    }

    #[test]
    fn day_state_runs_overdue_slots_in_order_and_each_once() {
        // server เริ่มสดตอน 13:00 NY — slot 10:00/12:00/12:45 ค้างทั้งหมด
        // policy: รัน slot ที่ค้างทีละอัน "ตามลำดับเวลา" (ไม่ stack, ไม่ข้าม)
        let mut state = DayState::new(NaiveDate::from_ymd_opt(2026, 11, 24).unwrap());
        let late = utc("2026-11-24T18:00:00Z"); // 13:00 NY
        let first = state.next_due(late, &cfg());
        assert_eq!(first.kind, JobKind::Layer2Core);
        assert_eq!(first.at_ny.hour(), 10);
        assert!(first.at_utc() < late); // overdue = รันทันที

        let universe = next_due_after_mark(&mut state, &first, late, &cfg());
        assert_eq!(universe.kind, JobKind::Layer2Universe); // 12:00
        let core1245 = next_due_after_mark(&mut state, &universe, late, &cfg());
        assert_eq!(core1245.kind, JobKind::Layer2Core); // 12:45
        let core1530 = next_due_after_mark(&mut state, &core1245, late, &cfg());
        assert_eq!(core1530.at_ny.hour(), 15);
        let l1 = next_due_after_mark(&mut state, &core1530, late, &cfg());
        assert_eq!(l1.kind, JobKind::Layer1);
        state.mark_done(&l1);

        // หมดวันแล้ว → ข้ามไปวันถัดไป (พุธ 25) slot แรก
        let after = state.next_due(utc("2026-11-24T22:00:00Z"), &cfg());
        assert_eq!(
            after.at_ny.date_naive(),
            NaiveDate::from_ymd_opt(2026, 11, 25).unwrap()
        );
        assert_eq!(after.kind, JobKind::Layer2Core);
        assert_eq!(after.at_ny.hour(), 10);
    }

    #[test]
    fn day_state_rolls_over_at_midnight_ny() {
        // done ของวันจันทร์ต้องไม่กัน slot ของวันอังคาร
        let mon = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let mut state = DayState::new(mon);
        let slot = slots_on(mon, &cfg()).remove(0);
        state.mark_done(&slot);
        let tue_morning = utc("2026-11-24T12:00:00Z"); // 07:00 NY อังคาร
        let next = state.next_due(tue_morning, &cfg());
        assert_eq!(
            next.at_ny.date_naive(),
            NaiveDate::from_ymd_opt(2026, 11, 24).unwrap()
        );
        assert_eq!(next.at_ny.hour(), 10);
    }

    #[test]
    fn estimate_counts_stock_windows_and_quote_batches() {
        // 96 สัญญา / batch 20 → 5 batch → L1 (4 หน้าต่าง) = 1+4+5 = 10/symbol
        assert_eq!(estimate_requests(1, 4), 10);
        assert_eq!(estimate_requests(7, 4), 70);
        // L2 (4 หน้าต่างเช่นกันสำหรับ default cap 45) — 0 symbol = 0 request
        assert_eq!(estimate_requests(0, 4), 0);
        let l1 = JobKind::Layer1.scope();
        assert_eq!(l1.windows.len(), 4);
        let l2 = JobKind::Layer2Core.scope();
        assert_eq!(l2.windows.len(), 4); // (31,45) ตัดสุดท้าย ไม่ใช่ 5
    }

    /// timezone sanity: เลข offset ของ NY ตามที่ chrono-tz รายงาน
    #[test]
    fn ny_offsets_before_and_after_dst() {
        let edt: DateTime<Tz> = utc("2026-10-30T20:30:00Z").with_timezone(&New_York);
        assert_eq!(edt.format("%Z").to_string(), "EDT");
        let est: DateTime<Tz> = utc("2026-11-02T21:30:00Z").with_timezone(&New_York);
        assert_eq!(est.format("%Z").to_string(), "EST");
    }
}
