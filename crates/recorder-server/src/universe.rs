//! Universe cache — ไฟล์ `<data_dir>/universe.json`
//!
//! ฝั่งอ่าน (ทุก job): `load_universe` เอาเฉพาะ `symbols[].symbol`
//! ไฟล์หาย/พัง ⇒ universe ว่าง + warning (Core ยังเก็บตามปกติ)
//!
//! ฝั่งเขียน (weekly): `refresh_loop` — เริ่มถ้า cache หาย/เก่า > 7 วัน
//! แล้วทุดาอาทิตย์ 12:00 NY: fetch NASDAQ → Wikipedia fallback → `choose`
//! top 130 → กรอง liquidity จากวันที่เก็บล่าสุด → เขียน top 100

use std::collections::HashSet;
use std::path::Path;

use chrono::{DateTime, Datelike, Duration, Utc, Weekday};
use chrono_tz::America::New_York;
use radar_core::lab::storage::LabStorage;
use radar_core::lab::types::{ObsKind, Observation};
use radar_core::universe::{UniverseSnapshot, UniverseSource};

use crate::config::{Config, TimeOfDay, normalize_symbols};
use crate::schedule::local_time;
use crate::{logerr, logln};

/// fetch top-N ก่อนกรอง liquidity (เผื่อตัวที่ถูกแทนที่ด้วย rank ถัดไป)
pub const REFRESH_TOP_N: usize = 130;
/// เก็บกี่ตัวหลังกรอง liquidity
pub const KEEP_LIQUID_N: usize = 100;

/// รูปร่างไฟล์ตามที่เขียนฝั่ง universe task — เราสนใจแค่ symbol
#[derive(serde::Deserialize)]
struct UniverseFile {
    #[serde(default)]
    #[allow(dead_code)]
    fetched_at: String,
    #[serde(default)]
    #[allow(dead_code)]
    source: String,
    #[serde(default)]
    symbols: Vec<UniverseSymbol>,
}

/// symbol หนึ่งตัวในไฟล์ — เราสนใจแค่ `symbol` เท่านั้น (field อื่น
/// name/sector/market_cap/rank_value อาจเป็น null หรือหายตาม source;
/// field ที่ไม่ประกาศ serde มองไม่เห็นอยู่แล้ว จึงอ่านได้ไม่ว่าค่าจะเป็นอะไร)
#[derive(serde::Deserialize)]
struct UniverseSymbol {
    #[serde(default)]
    symbol: String,
}

/// ผลการอ่าน universe — warning เป็นข้อความให้ log ฝั่ง caller
pub struct UniverseLoad {
    pub symbols: Vec<String>,
    pub warning: Option<String>,
}

/// อ่าน universe.json — ไม่มี network, ไม่ panic
pub fn load_universe(path: &Path) -> UniverseLoad {
    let raw = match std::fs::read_to_string(path) {
        Ok(r) => r,
        Err(_) => {
            return UniverseLoad {
                symbols: Vec::new(),
                warning: Some(format!(
                    "universe: ไม่พบ {} — Layer 1 เก็บเฉพาะ Core symbols",
                    path.display()
                )),
            };
        }
    };
    match serde_json::from_str::<UniverseFile>(&raw) {
        Ok(f) => UniverseLoad {
            symbols: normalize_symbols(
                &f.symbols
                    .iter()
                    .map(|s| s.symbol.clone())
                    .collect::<Vec<_>>(),
            ),
            warning: None,
        },
        Err(e) => UniverseLoad {
            symbols: Vec::new(),
            warning: Some(format!(
                "universe: parse {} ไม่ผ่าน ({e}) — ถือว่าว่าง",
                path.display()
            )),
        },
    }
}

/// union แบบคงลำดับ: core ก่อน แล้ว universe ที่ยังไม่มี (normalize+dedupe)
pub fn merge_symbols(core: &[String], universe: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in normalize_symbols(core)
        .iter()
        .chain(normalize_symbols(universe).iter())
    {
        if !out.contains(s) {
            out.push(s.clone());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Weekly refresh + liquidity
// ---------------------------------------------------------------------------

/// สรุปความ liquid ของ symbols จาก observations ของ "วันเดียว"
/// (caller ส่งวันที่เก็บล่าสุด) — pure เพื่อ unit-test ได้
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LiquidityInfo {
    /// symbol ที่มี option observation (quote หรือ bar) วันนั้น
    pub seen: HashSet<String>,
    /// เหล่า seen ที่มี quote ซึ่ง bid > 0 และ OI >= 100 อย่างน้อยหนึ่งรายการ
    pub liquid: HashSet<String>,
}

/// liquid = มี quote อย่างน้อยหนึ่งรายการที่ `bid > 0` และ `oi >= 100`
/// (ค่า None ไม่ถือว่าผ่าน — ห้ามใช้ 0 แทน missing)
pub fn liquidity_sets(obs: &[Observation]) -> LiquidityInfo {
    let mut info = LiquidityInfo::default();
    for o in obs {
        if let ObsKind::OptionQuote { bid, oi, .. } = &o.kind {
            info.seen.insert(o.contract.underlying.clone());
            if bid.is_some_and(|b| b > 0.0) && oi.is_some_and(|v| v >= 100.0) {
                info.liquid.insert(o.contract.underlying.clone());
            }
        } else if matches!(o.kind, ObsKind::OptionBar { .. }) {
            info.seen.insert(o.contract.underlying.clone());
        }
    }
    info
}

/// predicate สำหรับ `apply_liquidity`: symbol ที่ยังไม่มีข้อมูลเลย
/// (= ยังไม่เคยเก็บ) นับเป็น liquid — ตัวที่มีข้อมูลต้องผ่านเกณฑ์จริง
pub fn is_liquid(info: &LiquidityInfo, symbol: &str) -> bool {
    !info.seen.contains(symbol) || info.liquid.contains(symbol)
}

/// ต้อง refresh ไหม: cache หาย หรือเก่า >= 7 วัน (pure — test ได้)
pub fn refresh_needed(cache: Option<&UniverseSnapshot>, now: DateTime<Utc>) -> bool {
    match cache {
        None => true,
        Some(c) => now.signed_duration_since(c.fetched_at) >= Duration::days(7),
    }
}

/// อาทิตย์ 12:00 NY ครั้งถัดไปที่ strictly หลัง `now_utc` (pure — test ได้)
/// 12:00 ไม่มีทางชน DST gap จึงไม่มี case หาย
pub fn next_sunday_noon_ny(now_utc: DateTime<Utc>) -> DateTime<chrono_tz::Tz> {
    let now_ny = now_utc.with_timezone(&New_York);
    let noon = TimeOfDay {
        hour: 12,
        minute: 0,
    };
    let mut d = now_ny.date_naive();
    loop {
        if d.weekday() == Weekday::Sun
            && let Some(at) = local_time(d, noon)
            && at > now_ny
        {
            return at;
        }
        d += Duration::days(1);
    }
}

/// อ่าน liquidity จาก "วันที่เก็บล่าสุด" ใน Lab storage — ยังไม่มีวันไหน
/// เลย = ทุกตัว liquid (universe เพิ่งเริ่มเก็บ)
fn latest_liquidity(cfg: &Config) -> Result<LiquidityInfo, String> {
    let storage =
        LabStorage::open(cfg.lab_dir()).map_err(|e| format!("เปิด lab storage ไม่ได้: {e}"))?;
    let mut dates = storage
        .captured_dates()
        .map_err(|e| format!("อ่าน captured dates ไม่ได้: {e}"))?;
    let Some(latest) = dates.pop() else {
        return Ok(LiquidityInfo::default());
    };
    let obs = storage
        .read_observations(latest)
        .map_err(|e| format!("อ่าน observations ของ {latest} ไม่ได้: {e}"))?;
    Ok(liquidity_sets(&obs))
}

/// รัน refresh หนึ่งครั้ง (blocking — เรียกใน `spawn_blocking`):
/// fetch NASDAQ → Wikipedia fallback → choose top 130 → กรอง liquidity → เขียน 100
///
/// กรณีทั้งสอง source พังและ `choose` เสิร์ฟ cache — ไม่เขียนทับ
/// (fetched_at เดิมทำให้ `refresh_needed` ยัง true → retry รอบถัดไปเอง)
pub fn refresh_universe(cfg: &Config) -> Result<(&'static str, usize), String> {
    let now = Utc::now();
    let primary = radar_core::universe::fetch_nasdaq()
        .and_then(|json| radar_core::universe::parse_nasdaq(&json));
    let cache = radar_core::universe::read_cache(&cfg.universe_path());
    // Wikipedia ไม่มี market cap — ยืม rank จาก cache รอบก่อน (ถ้ามี)
    // ให้เรียงตาม cap ล่าสุดที่รู้จักแทนลำดับในตาราง
    let fallback = radar_core::universe::fetch_wikipedia_sp100()
        .and_then(|json| radar_core::universe::parse_wikipedia_sp100(&json))
        .map(|mut entries| {
            if let Some(c) = cache.as_ref() {
                radar_core::universe::rank_from_cache(&mut entries, &c.symbols);
            }
            entries
        });
    let (snap, warnings) =
        radar_core::universe::choose(primary, fallback, cache, REFRESH_TOP_N, now)
            .map_err(|e| format!("universe unavailable: {e}"))?;
    for w in &warnings {
        logln(format!("universe: {w}"));
    }
    if snap.source == UniverseSource::Cache {
        return Ok((snap.source.as_str(), snap.symbols.len()));
    }
    let liquidity = latest_liquidity(cfg)?;
    let kept = radar_core::universe::apply_liquidity(
        &snap.symbols,
        |s| is_liquid(&liquidity, s),
        KEEP_LIQUID_N,
    );
    let n = kept.len();
    let out = UniverseSnapshot {
        fetched_at: snap.fetched_at,
        source: snap.source,
        symbols: kept,
    };
    radar_core::universe::write_cache(&cfg.universe_path(), &out)
        .map_err(|e| format!("เขียน universe.json ไม่ได้: {e}"))?;
    Ok((out.source.as_str(), n))
}

/// loop ของ task universe (spawn คู่กับ scheduler):
/// - startup: cache หาย/เก่า ≥ 7 วัน → refresh เลย
/// - cache สด → รอถึงอาทิตย์ 12:00 NY แล้ว refresh
/// - refresh พัง → ลองใหม่ทุก 1 ชม. (cache ยัง stale/missing อยู่)
pub async fn refresh_loop(cfg: Config) {
    loop {
        let now = Utc::now();
        let path = cfg.universe_path();
        let cache = tokio::task::spawn_blocking(move || radar_core::universe::read_cache(&path))
            .await
            .ok()
            .flatten();
        if refresh_needed(cache.as_ref(), now) {
            if attempt_refresh(&cfg).await {
                sleep_until_next_sunday().await;
            } else {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        } else {
            let next = next_sunday_noon_ny(now).with_timezone(&Utc);
            let wait = (next - Utc::now()).to_std().unwrap_or_default();
            logln(format!(
                "universe: cache สด — รอ refresh อาทิตย์ 12:00 NY ({})",
                next.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            ));
            tokio::time::sleep(wait).await;
            // ถึงเที่ยงวันอาทิตย์แล้ว — refresh ตามตารางแม้ cache ยังไม่ครบ 7 วัน
            if !attempt_refresh(&cfg).await {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        }
    }
}

async fn sleep_until_next_sunday() {
    let next = next_sunday_noon_ny(Utc::now()).with_timezone(&Utc);
    let wait = (next - Utc::now()).to_std().unwrap_or_default();
    tokio::time::sleep(wait).await;
}

async fn attempt_refresh(cfg: &Config) -> bool {
    let c = cfg.clone();
    match tokio::task::spawn_blocking(move || refresh_universe(&c)).await {
        Ok(Ok((source, kept))) => {
            logln(format!(
                "universe refresh: source={source} kept={kept} symbols"
            ));
            true
        }
        Ok(Err(e)) => {
            logerr(format!("universe refresh ไม่สำเร็จ: {e} — ลองใหม่ใน 1 ชม."));
            false
        }
        Err(e) => {
            logerr(format!(
                "universe refresh worker panic: {e} — ลองใหม่ใน 1 ชม."
            ));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("recorder-server-uni-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn valid_file_extracts_symbols_normalized() {
        let dir = tmp_dir("valid");
        let p = dir.join("universe.json");
        std::fs::write(
            &p,
            r#"{
                "fetched_at": "2026-11-24T23:00:00Z",
                "source": "nasdaq",
                "symbols": [
                    {"symbol": "AAPL", "name": "Apple", "sector": "Tech", "market_cap": 3.0},
                    {"symbol": " msft ", "name": "Microsoft", "sector": "Tech"},
                    {"symbol": "BRK.B"},
                    {"symbol": ""}
                ]
            }"#,
        )
        .unwrap();
        let load = load_universe(&p);
        assert!(load.warning.is_none());
        assert_eq!(load.symbols, vec!["AAPL", "MSFT", "BRK.B"]); // trim+upper, ตัดว่าง
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn null_and_unknown_symbol_fields_still_load_all_symbols() {
        let dir = tmp_dir("nulls");
        let p = dir.join("universe.json");
        // cache จริง (write_cache ฝั่ง radar-core) เขียน sector/market_cap
        // เป็น null ได้ มี rank_value เพิ่ม และบาง entry อาจไม่มี name เลย
        std::fs::write(
            &p,
            r#"{
                "fetched_at": "2026-11-24T23:00:00Z",
                "source": "wikipedia",
                "symbols": [
                    {"symbol": "AAPL", "name": "Apple", "sector": null, "market_cap": null},
                    {"symbol": "MSFT", "sector": null, "rank_value": 42.0},
                    {"symbol": "BRK.B", "name": null}
                ]
            }"#,
        )
        .unwrap();
        let load = load_universe(&p);
        assert!(load.warning.is_none());
        assert_eq!(load.symbols, vec!["AAPL", "MSFT", "BRK.B"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_means_empty_with_warning() {
        let load = load_universe(Path::new("/nonexistent/universe.json"));
        assert!(load.symbols.is_empty());
        assert!(load.warning.as_deref().unwrap_or_default().contains("ไม่พบ"));
    }

    #[test]
    fn malformed_file_means_empty_with_warning() {
        let dir = tmp_dir("bad");
        let p = dir.join("universe.json");
        std::fs::write(&p, "{not json at all").unwrap();
        let load = load_universe(&p);
        assert!(load.symbols.is_empty());
        assert!(load.warning.as_deref().is_some_and(|w| w.contains("parse")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_dedupes_keeping_core_order_first() {
        let core = vec!["NVDA".to_string(), "AAPL".to_string()];
        let universe = vec!["AAPL".to_string(), "TSLA".to_string(), "nvda".to_string()];
        assert_eq!(
            merge_symbols(&core, &universe),
            vec!["NVDA".to_string(), "AAPL".to_string(), "TSLA".to_string()]
        );
        assert_eq!(merge_symbols(&core, &[]), core);
        assert!(merge_symbols(&[], &[]).is_empty());
    }

    // -----------------------------------------------------------------
    // liquidity + refresh decision
    // -----------------------------------------------------------------

    use chrono::TimeZone;
    use radar_core::engine::DataSource;
    use radar_core::lab::types::{ContractRef, Observation};
    use radar_core::quant::OptType;
    use radar_core::universe::UniverseEntry;

    fn quote(symbol: &str, bid: Option<f64>, oi: Option<f64>) -> Observation {
        let t = Utc.with_ymd_and_hms(2026, 11, 24, 15, 0, 0).unwrap();
        Observation {
            contract: ContractRef {
                occ: format!("{symbol}261218C00100000"),
                underlying: symbol.to_string(),
                expiry: Utc
                    .with_ymd_and_hms(2026, 12, 18, 0, 0, 0)
                    .unwrap()
                    .date_naive(),
                option_type: OptType::Call,
                strike: 100.0,
                multiplier: 100.0,
            },
            kind: radar_core::lab::types::ObsKind::OptionQuote {
                price: Some(1.0),
                bid,
                ask: None,
                iv: None,
                delta: None,
                oi,
                volume: None,
            },
            event_time: t,
            received_time: t,
            session_date: t.date_naive(),
            available_at: t,
            source: DataSource::Rest,
            backfill: false,
            universe_id: None,
        }
    }

    #[test]
    fn liquidity_sets_classifies_bid_oi_and_missing() {
        let obs = vec![
            quote("GOOD", Some(1.2), Some(500.0)),  // ผ่าน
            quote("GOOD", Some(0.0), Some(900.0)),  // ไม่ผ่าน แต่ตัวอื่นผ่านแล้ว
            quote("NOBID", Some(0.0), Some(500.0)), // bid ไม่ > 0
            quote("LOWOI", Some(1.0), Some(99.0)),  // OI < 100
            quote("MISSINGF", None, Some(500.0)),   // ค่า None = ไม่ผ่าน
        ];
        let info = liquidity_sets(&obs);
        assert_eq!(
            info.seen,
            ["GOOD", "NOBID", "LOWOI", "MISSINGF"]
                .iter()
                .map(|s| s.to_string())
                .collect::<std::collections::HashSet<_>>()
        );
        assert_eq!(
            info.liquid,
            ["GOOD"].iter().map(|s| s.to_string()).collect()
        );
    }

    #[test]
    fn liquidity_predicate_no_data_counts_as_liquid() {
        let info = liquidity_sets(&[quote("SEEN", Some(0.0), None)]);
        assert!(!is_liquid(&info, "SEEN")); // มีข้อมูลแต่ไม่ผ่านเกณฑ์
        assert!(is_liquid(&info, "NEWDATA")); // ยังไม่มีข้อมูล = liquid
        let empty = LiquidityInfo::default();
        assert!(is_liquid(&empty, "ANYTHING"));
    }

    fn snap_at(days_ago: i64) -> UniverseSnapshot {
        let now = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        UniverseSnapshot {
            fetched_at: now - Duration::days(days_ago),
            source: radar_core::universe::UniverseSource::Nasdaq,
            symbols: vec![UniverseEntry {
                symbol: "AAPL".into(),
                name: "Apple Inc.".into(),
                sector: None,
                market_cap: Some(1e12),
                rank_value: None,
            }],
        }
    }

    #[test]
    fn refresh_needed_missing_stale_and_fresh() {
        let now = Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap();
        assert!(refresh_needed(None, now)); // ไฟล์ไม่มี
        assert!(refresh_needed(Some(&snap_at(7)), now)); // ครบ 7 วัน = stale
        assert!(!refresh_needed(Some(&snap_at(6)), now)); // ยังสด
        assert!(!refresh_needed(Some(&snap_at(-1)), now)); // fetched_at อนาคต (นาฬิกาเดี้ยง) = ไม่ refresh
    }

    #[test]
    fn next_sunday_noon_from_weekday_and_same_sunday() {
        // เสาร์ 28 พ.ย. 2026 18:00 NY → อาทิตย์ 29 12:00 NY (EST = 17:00 UTC)
        let sat = New_York.with_ymd_and_hms(2026, 11, 28, 18, 0, 0).unwrap();
        let next = next_sunday_noon_ny(sat.with_timezone(&Utc));
        assert_eq!(
            next,
            New_York.with_ymd_and_hms(2026, 11, 29, 12, 0, 0).unwrap()
        );

        // อาทิตย์ 11:00 NY → เที่ยงวันนี้
        let sun_morning = New_York.with_ymd_and_hms(2026, 11, 29, 11, 0, 0).unwrap();
        assert_eq!(
            next_sunday_noon_ny(sun_morning.with_timezone(&Utc)),
            New_York.with_ymd_and_hms(2026, 11, 29, 12, 0, 0).unwrap()
        );

        // อาทิตย์ 13:00 NY → อาทิตย์ถัดไป (6 ธ.ค.)
        let sun_afternoon = New_York.with_ymd_and_hms(2026, 11, 29, 13, 0, 0).unwrap();
        assert_eq!(
            next_sunday_noon_ny(sun_afternoon.with_timezone(&Utc)),
            New_York.with_ymd_and_hms(2026, 12, 6, 12, 0, 0).unwrap()
        );
    }
}
