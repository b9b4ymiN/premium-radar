//! P8.4 — Lab forward-recorder (headless, post-close capture)
//!
//! ออกแบบตามข้อจำกัดจริงของเจ้าของ: เครื่องไม่ได้เปิดช่วงตลาด US
//! (22:30–05:00 เวลงไทย) — บันทึกแบบ "ภาพหลังปิดตลาด" แทน:
//! ตั้งเวลาให้ Windows Task Scheduler ปลุกรันตอนเช้าเวลาไทย
//! (= หลังปิดตลาด US ของคืนก่อน) ก็ได้ภาพราคาปิดของวันครบ
//!
//! ทำงานผ่าน adapter เดิม 100% (transport throttle 1.1s + signing + keyring)
//! เขียนลง `<APPDATA>/premium-radar/lab/` เดียวกับแอป — dedupe-on-write
//! ทำให้รันซ้ำ/แอปเปิดพร้อมกันได้โดยไม่ซ้ำข้อมูล
//!
//! Exit codes: 0 = สำเร็จ · 2 = token หมดอายุ (เปิดแอปสร้างใหม่) · 3 = ข้อผิดพลาด

use std::time::Instant;

use chrono::{Duration, Utc};
use radar_core::engine::{DataSource, parse_stock_update};
use radar_core::lab::capture::{
    Anchor, WindowCapture, option_observations, plan_capture, stock_observation,
};
use radar_core::lab::quality::session_date_of;
use radar_core::lab::storage::LabStorage;
use radar_core::lab::types::CaptureEvent;
use radar_core::webull::auth;
use radar_core::webull::discovery::{fetch_window, strike_band};
use radar_core::webull::quotes::OI_BATCH;
use radar_core::webull::rest::{
    Credentials, PATH_OPTION_SNAPSHOT, PATH_STOCK_SNAPSHOT, WINDOWS, get_json,
};
use serde_json::Value;

#[derive(serde::Deserialize)]
struct LabConfig {
    #[serde(default = "default_underlyings")]
    underlyings: Vec<String>,
    #[serde(default = "default_retention")]
    retention_days: usize,
}

fn default_underlyings() -> Vec<String> {
    vec!["NVDA".into()]
}

fn default_retention() -> usize {
    400
}

fn load_config(app_data: &str, fallback_symbol: &str) -> LabConfig {
    let path = std::path::Path::new(app_data)
        .join("lab")
        .join("config.json");
    if let Ok(raw) = std::fs::read_to_string(&path)
        && let Ok(cfg) = serde_json::from_str::<LabConfig>(&raw)
    {
        return cfg;
    }
    // default: ใช้ symbol ล่าสุดที่แอปสแกน (มี context ของเจ้าของอยู่แล้ว)
    let mut underlyings = default_underlyings();
    if !fallback_symbol.trim().is_empty() {
        underlyings = vec![fallback_symbol.trim().to_string()];
    }
    LabConfig {
        underlyings,
        retention_days: default_retention(),
    }
}

fn main() {
    let started = Instant::now();
    let now = Utc::now();

    // target = session ที่ปิดล่าสุด — สุดสัปดาห์ถอยกลับไปศุกร์ ไม่งั้นราคาปิด
    // วันศุกร์ (วันหมดอายุสำคัญที่สุดของ options รายสัปดาห์) จะไม่ถูกเก็บเลย
    let mut session = session_date_of(now);
    while matches!(
        chrono::Datelike::weekday(&session),
        chrono::Weekday::Sat | chrono::Weekday::Sun
    ) {
        session -= chrono::Duration::days(1);
    }
    // anchor แบบ post-close: ภาพราคาปิดของ session นี้ เก็บย้อนหลัง (backfill=true)
    let anchor = Anchor::post_close(session);
    let refresh = std::env::var("LAB_RECORDER_REFRESH").is_ok();
    println!(
        "== Lab recorder — session NY {session} (เก็บ {now} · โหมด post-close{}) ==",
        if refresh { " · REFRESH" } else { "" }
    );

    let app_data = std::env::var("APPDATA")
        .map(|a| format!("{a}/premium-radar"))
        .unwrap_or_else(|_| ".".into());

    let mut settings = premium_radar_lib::settings::AppSettings::load(&app_data);
    settings.pull_secrets_from_keyring();
    let creds: Credentials = settings
        .webull_credentials()
        .expect("ไม่มี credentials — เปิดแอปตั้งค่าก่อน");
    println!("creds: {}", creds.masked());

    // token ต้องมีและ valid (market data บน prod ต้องใช้ x-access-token)
    let Some(token) = creds.access_token.clone() else {
        println!("ไม่มี access token — เปิดแอป → Settings → สร้าง access token (SMS ครั้งเดียว ~15 วัน)");
        std::process::exit(2);
    };
    {
        let mut probe_client = settings
            .webull_client()
            .expect("ไม่มี credentials — เปิดแอปตั้งค่าก่อน");
        let expiry_note = |ms: Option<u64>| {
            ms.and_then(|m| chrono::DateTime::from_timestamp_millis(m as i64))
                .map(|d| format!(" valid ถึง {d}"))
                .unwrap_or_default()
        };
        match auth::check_token(&mut probe_client, &token) {
            Ok(check) if check.status == auth::TokenStatus::Normal => {
                println!("token:{}", expiry_note(check.expires_ms));
            }
            Ok(check) => {
                println!(
                    "token ไม่ผ่าน ({}) — เปิดแอป → Settings → สร้าง access token ใหม่ (SMS)",
                    check.status.as_str()
                );
                std::process::exit(2);
            }
            Err(e) => {
                println!("ตรวจ token ไม่สำเร็จ: {e}");
                std::process::exit(2);
            }
        }
    }

    let cfg = load_config(&app_data, &settings.last_symbol);
    println!(
        "underlyings: {:?} · retention {} วัน",
        cfg.underlyings, cfg.retention_days
    );

    let storage = match LabStorage::open(std::path::Path::new(&app_data).join("lab")) {
        Ok(s) => s,
        Err(e) => {
            println!("เปิดที่เก็บ lab ไม่ได้: {e}");
            std::process::exit(3);
        }
    };
    let _ = storage.log_session_event(session, &CaptureEvent::Started { at: now });

    let mut client = settings
        .webull_client()
        .expect("ไม่มี credentials — เปิดแอปตั้งค่าก่อน");
    let mut grand_written = 0usize;
    let mut grand_dupes = 0usize;
    let mut grand_rejected = 0usize;

    for symbol in &cfg.underlyings {
        println!("\n----- {symbol} -----");
        // หนึ่ง session เก็บครั้งเดียว (idempotent) — REFRESH เพื่อบังคับเก็บซ้ำ
        if !refresh {
            let universe_id = format!("{symbol}-{session}");
            let already = storage
                .read_observations(session)
                .unwrap_or_default()
                .iter()
                .filter(|o| o.universe_id.as_deref() == Some(universe_id.as_str()))
                .count();
            if already > 0 {
                println!(
                    "  มีข้อมูล session นี้อยู่แล้ว {already} แถว — ข้าม (ตั้ง LAB_RECORDER_REFRESH=1 เพื่อเก็บซ้ำ)"
                );
                continue;
            }
        }
        // 1) ราคาหุ้น (ต้อง valid ก่อน — ไม่ valid = บันทึกเหตุการณ์แล้วข้าม)
        let stock = match get_json(
            &mut client,
            PATH_STOCK_SNAPSHOT,
            &[
                ("symbols".into(), symbol.clone()),
                ("category".into(), "US_STOCK".into()),
                ("extend_hour_required".into(), "false".into()),
                ("overnight_required".into(), "false".into()),
            ],
        ) {
            Ok(body) => radar_core::webull::rest::extract_list(&body)
                .first()
                .and_then(|r| parse_stock_update(r, DataSource::Rest)),
            Err(e) => {
                println!("  stock snapshot พลาด: {e} — ข้าม {symbol}");
                continue;
            }
        };
        let Some(stock) = stock else {
            println!("  spot ไม่ valid — ปฏิเสธ {symbol} ตามกติกา (ห้ามเลือกโดยไม่มีราคาจริง)");
            continue;
        };
        println!("  spot {} (pre_close {})", stock.price, stock.pre_close);

        // 2) discovery ทีละหน้าต่าง DTE (band ±30% เหมือน scanner)
        let (gte, lte) = strike_band(stock.price);
        let mut windows: Vec<WindowCapture> = Vec::new();
        for (x, y) in WINDOWS {
            let a = session + Duration::days(x);
            let b = session + Duration::days(y);
            match fetch_window(&mut client, symbol, a, b, Some(&gte), Some(&lte), 3, None) {
                Ok(rows) => windows.push(WindowCapture {
                    expiry_range: (a, b),
                    has_cursor: false,
                    rows,
                }),
                Err(e) => {
                    println!("  discovery {a}..{b} พลาด: {e} — ข้ามหน้าต่างนี้");
                }
            }
        }
        let plan = plan_capture(symbol, stock.price, &windows, "near-spot-v1", now);
        for cov in &plan.coverage {
            println!(
                "  window {}..{}: {} rows{} · discovered {} · quoted {}",
                cov.expiry_range.0,
                cov.expiry_range.1,
                cov.rows,
                if cov.possibly_truncated {
                    " [ชน cap — อาจไม่ครบ]"
                } else {
                    ""
                },
                cov.discovered_occ.len(),
                cov.subscribed_occ.len()
            );
            let _ = storage.log_coverage(session, cov);
        }

        // 3) option snapshots ของ subset (batch 20/call เหมือน scanner)
        let mut quote_rows: Vec<Value> = Vec::new();
        for chunk in plan.subscribed_occ.chunks(OI_BATCH) {
            match get_json(
                &mut client,
                PATH_OPTION_SNAPSHOT,
                &[
                    ("symbols".into(), chunk.join(",")),
                    ("category".into(), "US_OPTION".into()),
                ],
            ) {
                Ok(body) => quote_rows.extend(radar_core::webull::rest::extract_list(&body)),
                Err(e) => println!("  option snapshot batch พลาด: {e}"),
            }
        }
        println!(
            "  quotes: {} แถวจาก {} สัญญา",
            quote_rows.len(),
            plan.subscribed_occ.len()
        );

        // 4) สร้าง + เขียน observations (validate + dedupe ข้างใน)
        let universe_id = format!("{symbol}-{session}");
        let mut batch = vec![stock_observation(
            symbol,
            &stock,
            &anchor,
            now,
            &universe_id,
        )];
        batch.extend(option_observations(&quote_rows, &anchor, now, &universe_id));
        match storage.write_observations(session, batch) {
            Ok(stats) => {
                println!(
                    "  เขียน {} · ซ้ำ {} · ปฏิเสธ {}",
                    stats.written,
                    stats.duplicates,
                    stats.rejected.len()
                );
                for r in &stats.rejected {
                    println!("    ปฏิเสธ {}: {}", r.context, r.reason_th);
                }
                let _ = storage.log_write_stats(session, &stats);
                grand_written += stats.written;
                grand_dupes += stats.duplicates;
                grand_rejected += stats.rejected.len();
            }
            Err(e) => println!("  เขียนล้มเหลว: {e}"),
        }
    }

    let removed = storage
        .apply_retention(cfg.retention_days)
        .unwrap_or_default();
    let _ = storage.log_session_event(session, &CaptureEvent::Stopped { at: Utc::now() });
    let dates = storage.captured_dates().unwrap_or_default();
    println!(
        "\nสรุป: เขียน {grand_written} · ซ้ำ {grand_dupes} · ปฏิเสธ {grand_rejected} · retention ลบ {} วันเก่า · มีข้อมูลรวม {} วัน · ใช้เวลา {:.0}s",
        removed.len(),
        dates.len(),
        started.elapsed().as_secs_f64()
    );
}
