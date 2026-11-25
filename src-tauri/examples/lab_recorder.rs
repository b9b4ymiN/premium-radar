//! P8.4 — Lab forward-recorder (headless, post-close capture)
//!
//! ออกแบบตามข้อจำกัดจริงของเจ้าของ: เครื่องไม่ได้เปิดช่วงตลาด US
//! (22:30–05:00 เวลงไทย) — บันทึกแบบ "ภาพหลังปิดตลาด" แทน:
//! ตั้งเวลาให้ Windows Task Scheduler ปลุกรันตอนเช้าเวลาไทย
//! (= หลังปิดตลาด US ของคืนก่อน) ก็ได้ภาพราคาปิดของวันครบ
//!
//! logic การเก็บราย underlying อยู่ใน `radar_core::lab::recorder` —
//! ไฟล์นี้เหลือแค่ credentials/token/config + การพิมพ์ผลจาก report
//!
//! ทำงานผ่าน adapter เดิม 100% (transport throttle 1.1s + signing + keyring)
//! เขียนลง `<APPDATA>/premium-radar/lab/` เดียวกับแอป — dedupe-on-write
//! ทำให้รันซ้ำ/แอปเปิดพร้อมกันได้โดยไม่ซ้ำข้อมูล
//!
//! Exit codes: 0 = สำเร็จ · 2 = token หมดอายุ (เปิดแอปสร้างใหม่) · 3 = ข้อผิดพลาด

use std::time::Instant;

use chrono::Utc;
use radar_core::lab::capture::Anchor;
use radar_core::lab::recorder::{
    RecordOptions, RecordOutcome, RecordStage, UnderlyingReport, finish_session,
    last_closed_session, record_underlying,
};
use radar_core::lab::storage::LabStorage;
use radar_core::lab::types::CaptureEvent;
use radar_core::webull::auth;
use radar_core::webull::rest::Credentials;

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

/// พิมพ์รายงานหนึ่ง underlying — ลำดับบรรทัดเหมือนตอน logic อยู่ใน main ทั้งหมด
fn print_report(r: &UnderlyingReport) {
    match &r.outcome {
        RecordOutcome::SkippedAlreadyCaptured { existing_rows } => {
            println!(
                "  มีข้อมูล session นี้อยู่แล้ว {existing_rows} แถว — ข้าม (ตั้ง LAB_RECORDER_REFRESH=1 เพื่อเก็บซ้ำ)"
            );
            return;
        }
        RecordOutcome::Failed {
            stage: RecordStage::StockSnapshot,
            error,
        } => {
            println!("  stock snapshot พลาด: {error} — ข้าม {}", r.symbol);
            return;
        }
        RecordOutcome::SkippedNoSpot => {
            println!(
                "  spot ไม่ valid — ปฏิเสธ {} ตามกติกา (ห้ามเลือกโดยไม่มีราคาจริง)",
                r.symbol
            );
            return;
        }
        _ => {}
    }
    let Some(stock) = &r.spot else {
        return;
    };
    println!("  spot {} (pre_close {})", stock.price, stock.pre_close);
    for w in &r.window_errors {
        println!(
            "  discovery {}..{} พลาด: {} — ข้ามหน้าต่างนี้",
            w.expiry_range.0, w.expiry_range.1, w.error
        );
    }
    for cov in &r.coverage {
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
    }
    for e in &r.quote_errors {
        println!("  option snapshot batch พลาด: {e}");
    }
    println!(
        "  quotes: {} แถวจาก {} สัญญา",
        r.quote_rows, r.requested_contracts
    );
    if let Some(stats) = &r.write_stats {
        println!(
            "  เขียน {} · ซ้ำ {} · ปฏิเสธ {}",
            stats.written,
            stats.duplicates,
            stats.rejected.len()
        );
        for rej in &stats.rejected {
            println!("    ปฏิเสธ {}: {}", rej.context, rej.reason_th);
        }
    }
    if let RecordOutcome::Failed {
        stage: RecordStage::Write,
        error,
    } = &r.outcome
    {
        println!("  เขียนล้มเหลว: {error}");
    }
}

fn main() {
    let started = Instant::now();
    let now = Utc::now();

    // target = session ที่ปิดล่าสุด — สุดสัปดาห์ถอยกลับไปศุกร์ ไม่งั้นราคาปิด
    // วันศุกร์ (วันหมดอายุสำคัญที่สุดของ options รายสัปดาห์) จะไม่ถูกเก็บเลย
    let session = last_closed_session(now);
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
    let opts = RecordOptions {
        session,
        anchor,
        now,
        refresh,
        selection_rule: "near-spot-v1",
    };
    let mut grand_written = 0usize;
    let mut grand_dupes = 0usize;
    let mut grand_rejected = 0usize;

    for symbol in &cfg.underlyings {
        println!("\n----- {symbol} -----");
        let report = record_underlying(&mut client, &storage, symbol, &opts);
        print_report(&report);
        if let Some(stats) = &report.write_stats {
            grand_written += stats.written;
            grand_dupes += stats.duplicates;
            grand_rejected += stats.rejected.len();
        }
    }

    let finish = finish_session(&storage, session, cfg.retention_days, Utc::now());
    println!(
        "\nสรุป: เขียน {grand_written} · ซ้ำ {grand_dupes} · ปฏิเสธ {grand_rejected} · retention ลบ {} วันเก่า · มีข้อมูลรวม {} วัน · ใช้เวลา {:.0}s",
        finish.removed_dates.len(),
        finish.captured_dates.len(),
        started.elapsed().as_secs_f64()
    );
}
