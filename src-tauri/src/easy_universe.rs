//! Easy Mode universe ranking — ประกอบ [`RankInput`] หนึ่งวันจาก recorder
//! (API ตาม settings หรือ local lab storage) แล้ว cache 30 นาที
//!
//! การแบ่งงาน: ส่วน pure (เลือกวันที่, cache, ประกอบ input ผ่าน closure)
//! อยู่ที่นี่และ test ได้โดยไม่แตะ network; `load_rank_input` คือส่วน
//! blocking ที่ผูกกับ recorder/LabStorage/Webull จริง — ต้องเรียกนอก UI
//! thread เท่านั้น (คำสั่ง `easy_universe_ideas` ใน lib.rs ครอบด้วย
//! `off_ui_thread`); `rank_universe` คำนวณใหม่ทุกครั้งจาก input ที่ cache
//! ไว้ งบ/ความเสี่ยง/stance เปลี่ยนจึงเห็นทันทีโดยไม่ fetch ใหม่

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, NaiveDate, Utc};

use radar_core::easy::EasyProfile;
use radar_core::easy_rank::{RankInput, SymbolExtras, shortlist};
use radar_core::lab::storage::LabStorage;
use radar_core::lab::types::{ObsKind, Observation};
use radar_core::quant::realized_vol;
use radar_core::webull::fundamentals::fetch_next_earnings;
use radar_core::webull::stocks::fetch_stock_bars;
use radar_core::webull::transport::WebullClient;

use crate::recorder_api;
use crate::settings::{AppSettings, LabSource};

/// ย้อนหลังหาวันที่มี option quote ได้มากสุดเท่าไร (วันล่าสุดอาจยัง
/// เก็บไม่ครบ — เช่นมีแต่ stock quote ตอน capture เพิ่งเริ่ม)
const LOOKBACK_DATES: usize = 3;
/// อายุ cache ของ RankInput ที่ประกอบแล้ว (observations + extras)
const CACHE_TTL: Duration = Duration::from_secs(30 * 60);

pub const MSG_SETUP: &str = "ยังไม่ได้ตั้งค่า Recorder server — ไปที่ Setting → แหล่งข้อมูล Lab";
pub const MSG_UNREACHABLE: &str = "ติดต่อ Recorder server ไม่ได้ — ลองใหม่อีกครั้ง";
pub const MSG_NO_OPTION_DATA: &str =
    "ยังไม่มีข้อมูล option จาก recorder — ข้อมูลชุดแรกจะมาหลังรอบเก็บเที่ยงคืน (เวลาไทย)";

/// Recorder error → ข้อความไทยสั้นๆ สำหรับ UI: เรื่องการเชื่อมต่อพูดเป็น
/// "ลองใหม่" ประเด็นอื่น (token/URL ผิด, ตอบผิดรูปแบบ) เก็บข้อความเฉพาะ
/// ของตัวเองไว้ — ทั้งสองไม่มีทางมี token ปน
fn recorder_error_th(e: recorder_api::RecorderError) -> String {
    match e {
        recorder_api::RecorderError::Unreachable | recorder_api::RecorderError::Timeout => {
            MSG_UNREACHABLE.to_string()
        }
        other => other.message_th().to_string(),
    }
}

/// "YYYY-MM-DD" → NaiveDate เรียงจากน้อยไปมาก (ของที่ parse ไม่ได้ตัดทิ้ง —
/// defensive เหมือน `recorder_api::dates` ที่เรียงซ้ำเผื่อ server ส่งงั้น)
fn parse_dates(raw: &[String]) -> Vec<NaiveDate> {
    let mut dates: Vec<NaiveDate> = raw
        .iter()
        .filter_map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .collect();
    dates.sort();
    dates
}

/// `today` = วันล่าสุด (ย้อนหลังมากสุด [`LOOKBACK_DATES`] วัน) ที่มี
/// `OptionQuote` ≥ 1 ตัว — วันที่อ่านไม่ได้หรือมีแต่ stock quote ถือว่ายัง
/// ไม่ใช่วันของเรา ข้ามไปดูวันเก่ากว่า
fn select_today<E>(
    dates: &[NaiveDate],
    fetch: &mut impl FnMut(&NaiveDate) -> Result<Vec<Observation>, E>,
) -> Option<(NaiveDate, Vec<Observation>)> {
    for date in dates.iter().rev().take(LOOKBACK_DATES) {
        if let Ok(obs) = fetch(date)
            && obs
                .iter()
                .any(|o| matches!(o.kind, ObsKind::OptionQuote { .. }))
        {
            return Some((*date, obs));
        }
    }
    None
}

/// วันก่อนหน้า `today` ในลิสต์วันที่เรียงแล้ว (ตัวที่มากสุดในบรรดาที่น้อยกว่า)
fn previous_date(dates: &[NaiveDate], today: NaiveDate) -> Option<NaiveDate> {
    dates.iter().copied().filter(|d| *d < today).max()
}

/// Cache หนึ่ง entry ต่อ (source, today) — profile เปลี่ยนไม่ต้อง
/// invalidate เพราะคำนวณอันดับใหม่จาก input เดิมเสมอ
pub struct UniverseCache {
    ttl: Duration,
    entry: Option<(Instant, String, NaiveDate, RankInput)>,
}

impl UniverseCache {
    fn new(ttl: Duration) -> Self {
        Self { ttl, entry: None }
    }

    fn get(&self, source: &str, today: NaiveDate) -> Option<RankInput> {
        let (at, src, date, input) = self.entry.as_ref()?;
        (at.elapsed() < self.ttl && src == source && *date == today).then(|| input.clone())
    }

    fn put(&mut self, source: &str, today: NaiveDate, input: RankInput) {
        self.entry = Some((Instant::now(), source.to_string(), today, input));
    }
}

static RANK_INPUT_CACHE: LazyLock<Mutex<UniverseCache>> =
    LazyLock::new(|| Mutex::new(UniverseCache::new(CACHE_TTL)));

/// ประกอบ RankInput จากวันที่ที่เรียงแล้ว — pure เทียบกับ closure ที่ส่ง
/// เข้า (ไม่แตะ network/filesystem เอง) จึง test ได้; `None` = ไม่มี
/// option data ใน [`LOOKBACK_DATES`] วันล่าสุด
fn assemble(
    source: &str,
    dates: &[NaiveDate],
    profile: &EasyProfile,
    now: DateTime<Utc>,
    fetch: &mut impl FnMut(&NaiveDate) -> Result<Vec<Observation>, ()>,
    enrich: impl FnOnce(&[String]) -> HashMap<String, SymbolExtras>,
    cache: &Mutex<UniverseCache>,
) -> Option<RankInput> {
    let (today, today_obs) = select_today(dates, fetch)?;
    {
        let guard = crate::lock(cache);
        if let Some(hit) = guard.get(source, today) {
            return Some(hit);
        }
    }
    // previous อ่านไม่ได้ = ไม่มี session ก่อนหน้า ใช้ vec ว่าง
    let previous = previous_date(dates, today)
        .and_then(|d| fetch(&d).ok())
        .unwrap_or_default();
    let mut input = RankInput {
        today: today_obs,
        previous,
        extras: HashMap::new(),
        source: source.to_string(),
    };
    let short = shortlist(&input, profile, now, 15);
    input.extras = enrich(&short);
    crate::lock(cache).put(source, today, input.clone());
    Some(input)
}

/// Stage 2: realized vol + วันงานแรนธิ่งของ shortlist ผ่าน client เดียว —
/// ตัวไหนพังเป็น None ของช่องนั้น ไม่พังทั้งคำสั่ง; ไม่มี client (ไม่มี
/// Webull keys) ⇒ extras ว่างแล้วยังจัดอันดับต่อ (vrp เป็น None)
fn enrich_extras(
    client: Option<&mut WebullClient>,
    symbols: &[String],
) -> HashMap<String, SymbolExtras> {
    let Some(client) = client else {
        return HashMap::new();
    };
    let mut extras = HashMap::new();
    for sym in symbols {
        // bars ไม่มีช่อง symbol — ต้องเรียกทีละตัว (ค่าเดียวกับ easy_ideas)
        let realized = fetch_stock_bars(client, std::slice::from_ref(sym), "D", 45)
            .ok()
            .and_then(|bars| {
                let closes: Vec<f64> = bars.iter().filter_map(|b| b.close).collect();
                realized_vol(&closes, 30)
            });
        let earnings = fetch_next_earnings(client, sym)
            .and_then(|w| NaiveDate::parse_from_str(&w.start_date, "%Y-%m-%d").ok());
        extras.insert(
            sym.clone(),
            SymbolExtras {
                realized_30d: realized,
                earnings_date: earnings,
            },
        );
    }
    extras
}

/// อ่านหนึ่งวันจากแหล่งข้อมูลตาม settings แล้วคืน RankInput ที่ cache แล้ว
/// (blocking — เรียกนอก UI thread เท่านั้น); `Err` เป็นข้อความไทยสำหรับ UI
pub fn load_rank_input(settings: &AppSettings, app_data_dir: &str) -> Result<RankInput, String> {
    let now = Utc::now();
    let profile = settings.easy_profile.clone();
    let mut client = settings.webull_client();
    let assembled = if settings.lab_source == LabSource::Api {
        let url = settings.recorder_api_url.trim();
        let token = settings.recorder_api_token.trim();
        if url.is_empty() || token.is_empty() {
            return Err(MSG_SETUP.to_string());
        }
        let dates = recorder_api::dates(url, token).map_err(recorder_error_th)?;
        let dates = parse_dates(&dates);
        let mut fetch =
            |d: &NaiveDate| recorder_api::observations(url, token, &d.to_string()).map_err(|_| ());
        assemble(
            "API",
            &dates,
            &profile,
            now,
            &mut fetch,
            |short| enrich_extras(client.as_mut(), short),
            &RANK_INPUT_CACHE,
        )
    } else {
        let lab = LabStorage::open(Path::new(app_data_dir).join("lab")).map_err(|e| {
            eprintln!("[easy_universe] เปิด lab storage ไม่ได้: {e}");
            MSG_NO_OPTION_DATA.to_string()
        })?;
        let dates = lab.captured_dates().unwrap_or_else(|e| {
            eprintln!("[easy_universe] อ่านรายการวันที่ไม่ได้: {e}");
            Vec::new()
        });
        let mut fetch = |d: &NaiveDate| lab.read_observations(*d).map_err(|_| ());
        assemble(
            "LOCAL",
            &dates,
            &profile,
            now,
            &mut fetch,
            |short| enrich_extras(client.as_mut(), short),
            &RANK_INPUT_CACHE,
        )
    };
    assembled.ok_or_else(|| MSG_NO_OPTION_DATA.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone};
    use radar_core::easy_rank::rank_universe;
    use radar_core::engine::DataSource;
    use radar_core::lab::quality::session_date_of;
    use radar_core::lab::types::ContractRef;
    use radar_core::quant::OptType;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn at(date: NaiveDate, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(date.year(), date.month(), date.day(), h, 0, 0)
            .unwrap()
    }

    /// universe 6 หุ้นแบบเดียวกับ easy_rank tests (put ~30 DTE + spot)
    fn session(date: NaiveDate) -> Vec<Observation> {
        let (expiry, t) = (d(2026, 12, 24), at(date, 21));
        [
            ("F", 11.0, 10.0, 0.11, 0.13, 1200.0),
            ("GM", 42.0, 38.0, 0.45, 0.49, 3000.0),
            ("INTC", 24.0, 21.0, 0.28, 0.32, 8000.0),
            ("PFE", 33.0, 30.0, 0.30, 0.34, 2500.0),
            ("T", 26.0, 23.0, 0.20, 0.24, 6000.0),
            ("KO", 60.0, 55.0, 0.50, 0.55, 9000.0),
        ]
        .into_iter()
        .flat_map(|(sym, spot, strike, bid, ask, oi)| {
            vec![
                stock(sym, spot, t),
                put(sym, strike, expiry, t, bid, ask, oi),
            ]
        })
        .collect()
    }

    fn put(
        sym: &str,
        strike: f64,
        expiry: NaiveDate,
        event: DateTime<Utc>,
        bid: f64,
        ask: f64,
        oi: f64,
    ) -> Observation {
        let occ = format!("{sym}{}P{strike:0>8.0}", expiry.format("%y%m%d"));
        Observation {
            contract: ContractRef {
                occ,
                underlying: sym.to_string(),
                expiry,
                option_type: OptType::Put,
                strike,
                multiplier: 100.0,
            },
            kind: ObsKind::OptionQuote {
                price: Some((ask + bid) / 2.0),
                bid: Some(bid),
                ask: Some(ask),
                iv: None,
                delta: None,
                oi: Some(oi),
                volume: Some(oi / 2.0),
            },
            event_time: event,
            received_time: event,
            session_date: session_date_of(event),
            available_at: event,
            source: DataSource::Rest,
            backfill: false,
            universe_id: Some("test".into()),
        }
    }

    fn stock(sym: &str, price: f64, event: DateTime<Utc>) -> Observation {
        Observation {
            contract: ContractRef {
                occ: sym.to_string(),
                underlying: sym.to_string(),
                expiry: event.date_naive(),
                option_type: OptType::Call,
                strike: 0.0,
                multiplier: 1.0,
            },
            kind: ObsKind::StockQuote {
                price: Some(price),
                open: None,
                high: None,
                low: None,
                pre_close: None,
                volume: None,
            },
            event_time: event,
            received_time: event,
            session_date: session_date_of(event),
            available_at: event,
            source: DataSource::Rest,
            backfill: false,
            universe_id: Some("test".into()),
        }
    }

    fn profile(cash: f64) -> EasyProfile {
        EasyProfile {
            onboarded: true,
            stocks: Vec::new(),
            cash,
            risk: 2,
        }
    }

    fn now() -> DateTime<Utc> {
        at(d(2026, 11, 25), 22)
    }

    /// วันล่าสุดมีแต่ stock quote (capture เพิ่งเริ่ม) ⇒ ข้ามไปวันก่อนหน้า
    /// ที่มี option quote และหา previous จากวันนั้นได้
    #[test]
    fn selection_skips_latest_date_without_option_quotes() {
        let (d20, d23, d24) = (d(2026, 11, 20), d(2026, 11, 23), d(2026, 11, 24));
        let dates = vec![d20, d23, d24];
        let mut calls = 0;
        let mut fetch = |date: &NaiveDate| -> Result<Vec<Observation>, ()> {
            calls += 1;
            Ok(if *date == d24 {
                vec![stock("F", 11.0, at(d24, 21))]
            } else {
                session(*date)
            })
        };
        let (today, obs) = select_today(&dates, &mut fetch).expect("today = d23");
        assert_eq!(today, d23);
        assert_eq!(calls, 2, "ดู d24 ก่อนแล้วข้ามไป d23");
        assert!(
            obs.iter()
                .any(|o| matches!(o.kind, ObsKind::OptionQuote { .. }))
        );
        assert_eq!(previous_date(&dates, d23), Some(d20));
        assert_eq!(previous_date(&dates, d20), None);
    }

    /// มองย้อนหลังได้มากสุด 3 วัน — วันที่ 4 ที่มีข้อมูลก็ไม่ถูกมอง
    #[test]
    fn selection_looks_back_at_most_three_dates() {
        // 17–24 พ.ย.: มี option quote เฉพาะ d21 ซึ่งเป็นวันที่ 4 จากท้าย
        let dates: Vec<NaiveDate> = (17..=24).map(|day| d(2026, 11, day)).collect();
        let mut calls = 0;
        let mut fetch = |date: &NaiveDate| -> Result<Vec<Observation>, ()> {
            calls += 1;
            Ok(if date.day() == 21 {
                session(*date)
            } else {
                Vec::new()
            })
        };
        assert!(select_today(&dates, &mut fetch).is_none());
        assert_eq!(calls, 3, "ถามแค่ 3 วันล่าสุดแล้วหยุด");
    }

    /// cache ภายใน TTL ต้องคืน RankInput เดิมโดยไม่เรียก enrich ซ้ำ และ
    /// จัดอันดับใหม่จาก input เดิมได้เมื่อ profile (งบ) เปลี่ยน
    #[test]
    fn assemble_uses_cache_within_ttl_and_reranks_per_profile() {
        let dates = vec![d(2026, 11, 23), d(2026, 11, 24)];
        // Cell ให้ closure ยืมแบบ shared จึง assert ระหว่างที่ closure ยังใช้อยู่ได้
        let fetches = std::cell::Cell::new(0);
        let mut fetch = |date: &NaiveDate| -> Result<Vec<Observation>, ()> {
            fetches.set(fetches.get() + 1);
            Ok(session(*date))
        };
        let cache = Mutex::new(UniverseCache::new(Duration::from_secs(60)));
        let mut enrich_calls = 0;
        let first = assemble(
            "LOCAL",
            &dates,
            &profile(50_000.0),
            now(),
            &mut fetch,
            |short: &[String]| {
                enrich_calls += 1;
                short
                    .iter()
                    .map(|s| {
                        (
                            s.clone(),
                            SymbolExtras {
                                realized_30d: Some(0.25),
                                earnings_date: None,
                            },
                        )
                    })
                    .collect()
            },
            &cache,
        )
        .expect("assembles");
        assert_eq!(fetches.get(), 2, "เลือกวัน (d24) + อ่าน previous (d23)");
        assert_eq!(enrich_calls, 1);
        assert_eq!(first.source, "LOCAL");

        // ครั้งที่สอง: cache hit — selection ยังต้องถาม d24 แต่ไม่อ่าน
        // previous และไม่ enrich ซ้ำ
        let cached = assemble(
            "LOCAL",
            &dates,
            &profile(50_000.0),
            now(),
            &mut fetch,
            |_: &[String]| panic!("cache hit ต้องไม่เรียก enrich"),
            &cache,
        )
        .expect("cache hit");
        assert_eq!(cached, first);
        assert_eq!(fetches.get(), 3);
        assert_eq!(enrich_calls, 1);

        // จัดอันดับใหม่จาก input เดิม: งบเล็ก ⇒ budget_short, งบใหญ่ ⇒ picks
        let poor = rank_universe(&cached, &profile(900.0), now());
        assert!(poor.picks.is_empty());
        assert!(poor.budget_short.is_some());
        let rich = rank_universe(&cached, &profile(50_000.0), now());
        assert_eq!(rich.picks.len(), 3);
        assert!(rich.picks.iter().all(|p| p.realized_30d_pct == Some(25.0)));
    }

    /// ไม่มี option data เลย ⇒ None (แปลเป็นข้อความไทยที่คำสั่ง)
    #[test]
    fn assemble_without_option_data_is_none() {
        let dates = vec![d(2026, 11, 24)];
        let mut fetch = |_: &NaiveDate| Ok::<Vec<Observation>, ()>(Vec::new());
        let cache = Mutex::new(UniverseCache::new(Duration::from_secs(60)));
        assert!(
            assemble(
                "LOCAL",
                &dates,
                &profile(50_000.0),
                now(),
                &mut fetch,
                |_: &[String]| HashMap::new(),
                &cache
            )
            .is_none()
        );
    }

    /// extras ที่ fetch พัง (None ทั้งคู่) ต้องยังจัดอันดับได้ — vrp เป็น None
    #[test]
    fn failed_extras_degrade_to_none() {
        let dates = vec![d(2026, 11, 24)];
        let mut fetch = |date: &NaiveDate| Ok::<_, ()>(session(*date));
        let cache = Mutex::new(UniverseCache::new(Duration::from_secs(60)));
        let input = assemble(
            "LOCAL",
            &dates,
            &profile(50_000.0),
            now(),
            &mut fetch,
            |short: &[String]| {
                short
                    .iter()
                    .map(|s| (s.clone(), SymbolExtras::default()))
                    .collect()
            },
            &cache,
        )
        .expect("assembles");
        let ideas = rank_universe(&input, &profile(50_000.0), now());
        assert_eq!(ideas.picks.len(), 3);
        assert!(ideas.picks.iter().all(|p| p.parts.vrp.is_none()));
    }

    /// โหมด API ที่ไม่ได้ใส่ URL/token ⇒ ข้อความตั้งค่า ก่อนถึง network
    #[test]
    fn api_mode_without_url_or_token_returns_setup_message() {
        let mut settings = AppSettings {
            lab_source: LabSource::Api,
            recorder_api_url: String::new(),
            recorder_api_token: String::new(),
            ..AppSettings::default()
        };
        let err = load_rank_input(&settings, "unused").unwrap_err();
        assert_eq!(err, MSG_SETUP);
        // ใส่ URL แต่ไม่ใส่ token ก็ยังเป็นข้อความเดิม
        settings.recorder_api_url = "https://recorder.example.ts.net".into();
        assert_eq!(load_rank_input(&settings, "unused").unwrap_err(), MSG_SETUP);
    }

    /// local LabStorage แบบ end-to-end (temp dir): เขียนสองวันแล้วประกอบ
    /// จนได้ UniverseIdeas source "LOCAL" — ไม่มี Webull keys ⇒ extras ว่าง
    #[test]
    fn local_storage_end_to_end_produces_local_ideas() {
        let dir = std::env::temp_dir().join(format!(
            "premium-radar-easy-universe-{}-local",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        // load_rank_input เปิดที่ <app_data_dir>/lab — เขียนด้วย root เดียวกัน
        let store = LabStorage::open(dir.join("lab")).unwrap();
        store
            .write_observations(d(2026, 11, 23), session(d(2026, 11, 23)))
            .unwrap();
        let stats = store
            .write_observations(d(2026, 11, 24), session(d(2026, 11, 24)))
            .unwrap();
        assert!(
            stats.rejected.is_empty(),
            "ต้องเขียนได้ทุกตัว: {:?}",
            stats.rejected
        );

        let settings = AppSettings {
            lab_source: LabSource::Local,
            easy_profile: profile(50_000.0),
            ..AppSettings::default()
        };
        let input =
            load_rank_input(&settings, dir.to_str().unwrap()).expect("assembles from storage");
        assert_eq!(input.source, "LOCAL");
        assert!(input.extras.is_empty(), "ไม่มี Webull keys ⇒ extras ว่าง");
        let ideas = rank_universe(&input, &settings.easy_profile, now());
        assert_eq!(ideas.source, "LOCAL");
        assert_eq!(ideas.session_date.as_deref(), Some("2026-11-24"));
        assert!(!ideas.picks.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
