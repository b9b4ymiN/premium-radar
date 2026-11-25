//! recorder-server — Lab recorder แบบ headless สำหรับรันใน Docker
//!
//! คำสั่ง: `run` (default, scheduler loop + HTTP API) · `once --layer 1|2
//! [--symbols A,B] [--refresh]` · `plan` (dry run ไม่มี network)
//!
//! ตารางเป็นเวลา New York (DST-correct), สุดสัปดาห์ไม่เก็บ, หนึ่ง job
//! ต่อครั้ง (job ล้นเวลา = slot ถัดไปรันทันทีเมื่อ job เดิมจบ ไม่ซ้อนกัน)
//! slot ที่รันแล้วถูกจดไว้ใน status.json — restart ไม่รันซ้ำของวันเดิม
//!
//! `run` เปิด read-only HTTP API (bearer token — ดู `api::router`) คู่กับ
//! scheduler ใน process เดียวกัน และ spawn task universe refresh รายสัปดาห์

mod api;
mod budget;
mod config;
mod job;
mod schedule;
mod status;
mod universe;

use chrono::Utc;
use chrono_tz::America::New_York;
use config::{Config, Secrets};
use job::{JobRequest, JobSummary};
use schedule::JobKind;
use status::{LastJob, Status};

/// log บรรทัดเดียวพร้อม timestamp ไปที่ stdout (ไม่พึ่ง logging crate)
pub fn logln(msg: impl std::fmt::Display) {
    println!("[{}] {msg}", Utc::now().format("%Y-%m-%dT%H:%M:%SZ"));
}

pub fn logerr(msg: impl std::fmt::Display) {
    eprintln!("[{}] {msg}", Utc::now().format("%Y-%m-%dT%H:%M:%SZ"));
}

const USAGE: &str = "usage: recorder-server [run|once|plan]
  run    (default) scheduler loop — จอง slot ตามเวลา NY แล้วเก็บข้อมูล
  once   ครั้งเดียว: once --layer 1|2 [--symbols A,B] [--refresh]
  plan   dry run: config + 5 job ถัดไป + ประมาณ request (ไม่มี network)";

#[derive(Debug, PartialEq)]
enum Command {
    Run,
    Once {
        layer: u8,
        symbols: Option<Vec<String>>,
        refresh: bool,
    },
    Plan,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let Some(cmd) = args.next() else {
        return Ok(Command::Run);
    };
    match cmd.as_str() {
        "run" => Ok(Command::Run),
        "plan" => Ok(Command::Plan),
        "once" => {
            let (mut layer, mut symbols, mut refresh) = (None, None, false);
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--layer" => {
                        layer = args
                            .next()
                            .and_then(|v| v.parse::<u8>().ok())
                            .filter(|n| *n == 1 || *n == 2);
                        if layer.is_none() {
                            return Err("--layer ต้องเป็น 1 หรือ 2".into());
                        }
                    }
                    "--symbols" => {
                        let Some(raw) = args.next() else {
                            return Err("--symbols ต้องตามด้วย A,B".into());
                        };
                        symbols = Some(
                            raw.split(',')
                                .map(|s| s.trim().to_uppercase())
                                .filter(|s| !s.is_empty())
                                .collect(),
                        );
                    }
                    "--refresh" => refresh = true,
                    other => return Err(format!("flag ไม่รู้จัก: {other}")),
                }
            }
            Ok(Command::Once {
                layer: layer.ok_or("once ต้องมี --layer 1|2")?,
                symbols,
                refresh,
            })
        }
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            std::process::exit(0);
        }
        other => Err(format!("คำสั่งไม่รู้จัก: {other}\n{USAGE}")),
    }
}

/// symbols ของ job แต่ละชนิด — universe.json อ่านสดทุกครั้ง
/// (task universe เขียนได้ตลอดเวลา; หาย/พัง = ว่าง)
fn symbols_for(kind: JobKind, cfg: &Config) -> (Vec<String>, Option<String>) {
    match kind {
        JobKind::Layer1 => {
            let uni = universe::load_universe(&cfg.universe_path());
            (
                universe::merge_symbols(&cfg.core_symbols, &uni.symbols),
                uni.warning,
            )
        }
        JobKind::Layer2Core => (cfg.core_symbols.clone(), None),
        JobKind::Layer2Universe => {
            let uni = universe::load_universe(&cfg.universe_path());
            (uni.symbols, uni.warning)
        }
    }
}

/// อัปเดต `<data_dir>/status.json` หลังจบ job — `slot_id` เป็น Some เมื่อ
/// มาจาก scheduler (จดว่า slot นี้รันแล้ว กันรันซ้ำหลัง restart)
fn update_status(cfg: &Config, sum: &JobSummary, scheduled_for_ny: &str, slot_id: Option<&str>) {
    let path = cfg.status_path();
    let mut st: Status = status::load(&path);
    {
        let stats = if sum.kind.layer() == 1 {
            &mut st.layer1
        } else {
            &mut st.layer2
        };
        stats.jobs += 1;
        stats.symbols_recorded += sum.recorded as u64;
        stats.symbols_skipped += sum.skipped as u64;
        stats.symbols_failed += sum.failed as u64;
    }
    st.last_job = Some(LastJob {
        kind: sum.kind.label().to_string(),
        layer: sum.kind.layer(),
        scheduled_for_ny: scheduled_for_ny.to_string(),
        started_at: sum.started_at,
        finished_at: sum.finished_at,
        symbols: sum.symbols,
        outcome: format!(
            "recorded={} skipped={} failed={} written={} dup={} rej={}",
            sum.recorded, sum.skipped, sum.failed, sum.written, sum.duplicates, sum.rejected
        ),
    });
    if sum.token.is_some() {
        st.token = sum.token.clone();
    }
    st.last_error = sum.error.clone();
    st.updated_at = Some(Utc::now());
    if let Some(id) = slot_id {
        st.mark_slot_done(id, Utc::now());
    }
    if let Err(e) = status::save(&path, &st) {
        logerr(format!("เขียน status.json ไม่ได้: {e}"));
    }
}

/// `plan` — ไม่แตะ network, ไม่ต้องมี credentials
fn run_plan() {
    let (cfg, warnings) = config::load();
    println!("recorder-server plan — dry run (ไม่มี network ไม่อ่าน secret เพิ่ม)");
    for w in &warnings {
        println!("  warning: {w}");
    }
    println!("config: {}", cfg.summary());
    match radar_core::webull::rest::Credentials::from_env() {
        Some(c) => println!("credentials: {}", c.masked()),
        None => println!(
            "credentials: (ยังไม่ตั้ง env — plan ไม่ต้องใช้; run/once ต้องมี WEBULL_APP_KEY/WEBULL_APP_SECRET)"
        ),
    }
    let env_token = std::env::var("WEBULL_ACCESS_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    let token_where = match env_token {
        Some(_) => "env".to_string(),
        None if cfg.token_file().exists() => format!("file {}", cfg.token_file().display()),
        None => "(ยังไม่มี — run/once ต้องมี)".to_string(),
    };
    println!("token source: {token_where} (ค่าจริงไม่ถูกพิมพ์)");

    // API — แสดงเฉพาะ "มี/ไม่มี token" กับ bind ไม่พิมพ์ค่าจริง
    let bind = api::bind_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|e| format!("(ผิด: {e})"));
    let api_token_where = match api::api_token_source(&cfg) {
        Some("env") => "ตั้งแล้ว (env RECORDER_API_TOKEN)".to_string(),
        Some(_) => format!("ตั้งแล้ว (ไฟล์ {})", cfg.api_token_file().display()),
        None => "(ยังไม่มี — run จะปฏิเสธการเปิด API)".to_string(),
    };
    println!("api: bind={bind} · api_token={api_token_where} (ค่าจริงไม่ถูกพิมพ์)");

    let uni = universe::load_universe(&cfg.universe_path());
    if let Some(w) = &uni.warning {
        println!("  warning: {w}");
    }
    let merged = universe::merge_symbols(&cfg.core_symbols, &uni.symbols);
    println!(
        "symbols: core={} universe={} layer1_union={}",
        cfg.core_symbols.len(),
        uni.symbols.len(),
        merged.len()
    );
    println!("core: {:?}", cfg.core_symbols);
    if !uni.symbols.is_empty() {
        let preview: Vec<&str> = uni.symbols.iter().take(8).map(String::as_str).collect();
        println!(
            "universe (8 ตัวแรก): {preview:?}{}",
            if uni.symbols.len() > 8 { " …" } else { "" }
        );
    }

    println!("next {} jobs:", 5);
    for (i, slot) in schedule::next_slots(Utc::now(), &cfg, 5).iter().enumerate() {
        let (symbols, source) = match slot.kind {
            JobKind::Layer1 => (merged.len(), "core∪universe"),
            JobKind::Layer2Core => (cfg.core_symbols.len(), "core"),
            JobKind::Layer2Universe => (uni.symbols.len(), "universe"),
        };
        let windows = slot.kind.scope().windows.len();
        // +1 = token check ก่อน job
        let est = schedule::estimate_requests(symbols, windows) + 1;
        println!(
            "  job {}: {} at {} [NY] = {} · symbols={symbols} ({source}) · windows={windows} · est_requests≈{est}",
            i + 1,
            slot.kind.label(),
            slot.at_ny.format("%Y-%m-%d %H:%M %Z"),
            slot.at_utc()
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        );
    }
}

/// `once` — เก็บครั้งเดียวเดี๋ยวนี้ (exit 0 = รันจบ, 2 = secret ไม่ครบ, 3 = job error)
fn run_once(cfg: &Config, layer: u8, symbols: Option<Vec<String>>, refresh: bool) -> i32 {
    let secrets: Secrets = match config::load_secrets(cfg) {
        Ok(s) => s,
        Err(e) => {
            logerr(format!("{e} — ตั้ง env/ไฟล์ token ก่อนรัน once"));
            return 2;
        }
    };
    logln(format!("creds: {}", secrets.creds.masked()));
    let kind = if layer == 1 {
        JobKind::Layer1
    } else {
        JobKind::Layer2Core
    };
    let (default_syms, warning) = symbols_for(kind, cfg);
    if let Some(w) = warning {
        logln(w);
    }
    let syms = symbols.unwrap_or(default_syms);
    if syms.is_empty() {
        logerr("ไม่มี symbol จะเก็บ (core/universe ว่าง หรือ --symbols ว่าง)");
        return 3;
    }
    let req = JobRequest {
        kind,
        symbols: syms,
        scheduled_for_ny: "(manual once)".into(),
        refresh,
    };
    let sum = job::run_job(cfg, &secrets, req);
    logln(sum.summary_line());
    update_status(cfg, &sum, "(manual once)", None);
    if let Some(e) = &sum.error {
        logerr(format!("once: {e}"));
        3
    } else {
        0
    }
}

/// scheduler loop: จอง slot → sleep → รัน job ถัดไปทีละอัน — slot ที่รัน
/// แล้วถูกจดใน status.json ด้วย (restart ไม่รันซ้ำ slot ค้างของวันเดิม)
async fn scheduler_loop(cfg: Config, secrets: Secrets) {
    // seed จาก status.json — มีเฉพาะ id ของวัน NY ปัจจุบันที่มีผล
    let mut state = schedule::DayState::with_done(
        Utc::now().with_timezone(&New_York).date_naive(),
        status::load(&cfg.status_path()).completed_slot_ids(),
    );
    loop {
        let now = Utc::now();
        let slot = state.next_due(now, &cfg);
        let (symbols, warning) = symbols_for(slot.kind, &cfg);
        if let Some(w) = warning {
            logln(w);
        }
        let scheduled_ny = slot
            .at_ny
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
        if slot.at_utc() > now {
            logln(format!(
                "next: {} ที่ {} [NY] ({} UTC) symbols={}",
                slot.kind.label(),
                slot.at_ny.format("%Y-%m-%d %H:%M %Z"),
                slot.at_utc()
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                symbols.len()
            ));
            let wait = (slot.at_utc() - now).to_std().unwrap_or_default();
            tokio::time::sleep(wait).await;
        } else {
            logln(format!(
                "overdue: {} slot {} [NY] — รันทันที (job ก่อนหน้าล้นเวลา ไม่ซ้อนกัน)",
                slot.kind.label(),
                slot.at_ny.format("%Y-%m-%d %H:%M %Z"),
            ));
        }
        // ทำเครื่องหมายก่อนรัน — กันรัน slot เดิมซ้ำแม้ job พังกลางทาง
        state.mark_done(&slot);

        let job_cfg = cfg.clone();
        let job_secrets = secrets.clone();
        let req = JobRequest {
            kind: slot.kind,
            symbols,
            scheduled_for_ny: scheduled_ny.clone(),
            refresh: true,
        };
        let result =
            tokio::task::spawn_blocking(move || job::run_job(&job_cfg, &job_secrets, req)).await;
        match result {
            Ok(sum) => {
                if let Some(e) = &sum.error {
                    logerr(format!("job {}: {e}", sum.kind.label()));
                }
                logln(sum.summary_line());
                update_status(&cfg, &sum, &scheduled_ny, Some(&slot.key));
            }
            Err(e) => {
                logerr(format!(
                    "job {} panic ใน worker: {e} — ไป slot ถัดไป",
                    slot.kind.label()
                ));
                // slot นี้ "พยายามรัน" แล้ว — จดไว้กันรันซ้ำหลัง restart
                let mut st = status::load(&cfg.status_path());
                st.mark_slot_done(&slot.key, Utc::now());
                if let Err(e2) = status::save(&cfg.status_path(), &st) {
                    logerr(format!("เขียน status.json ไม่ได้: {e2}"));
                }
            }
        }
    }
}

/// `run` — เปิด API (bearer token) + scheduler + universe refresh ใน
/// process เดียว แล้วรอจนกว่าจะ ctrl-c
async fn run_forever(cfg: Config) {
    logln(format!("recorder-server run — {}", cfg.summary()));
    let secrets = match config::load_secrets(&cfg) {
        Ok(s) => s,
        Err(e) => {
            logerr(format!(
                "{e} — ตั้ง env ก่อนรัน (WEBULL_APP_KEY/WEBULL_APP_SECRET/token)"
            ));
            std::process::exit(2);
        }
    };
    logln(format!(
        "creds: {} · token from {}",
        secrets.creds.masked(),
        secrets.token_source
    ));

    // ไม่มี API token = ไม่เปิด API — ไม่เสิร์ฟ Lab data แบบไม่มี auth เด็ดขาด
    let api_token = match api::resolve_api_token(&cfg) {
        Some(t) => t,
        None => {
            logerr(format!(
                "ไม่มี API token — ตั้ง RECORDER_API_TOKEN หรือวางไฟล์ {} แล้วรันใหม่ \
                 (ปฏิเสธการเปิด API แบบไม่มี auth)",
                cfg.api_token_file().display()
            ));
            std::process::exit(2);
        }
    };
    let bind = match api::bind_addr() {
        Ok(b) => b,
        Err(e) => {
            logerr(e);
            std::process::exit(2);
        }
    };
    let listener = match tokio::net::TcpListener::bind(bind).await {
        Ok(l) => l,
        Err(e) => {
            logerr(format!("bind {bind} ไม่ได้: {e}"));
            std::process::exit(2);
        }
    };
    logln(format!(
        "api: http://{bind} — GET /health (no auth) + GET /v1/* (bearer token)"
    ));

    tokio::spawn(universe::refresh_loop(cfg.clone()));
    tokio::spawn(scheduler_loop(cfg.clone(), secrets));

    if let Err(e) = axum::serve(listener, api::router(api::ApiState::new(&cfg, api_token)))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
    {
        logerr(format!("api server error: {e}"));
    }
    logln("ctrl-c — หยุด recorder-server");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = match parse_args(args.into_iter()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            std::process::exit(1);
        }
    };
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("สร้าง tokio runtime ได้");
    rt.block_on(async {
        match cmd {
            Command::Plan => run_plan(),
            Command::Once {
                layer,
                symbols,
                refresh,
            } => {
                let (cfg, warnings) = config::load();
                for w in warnings {
                    logln(format!("warning: {w}"));
                }
                std::process::exit(run_once(&cfg, layer, symbols, refresh));
            }
            Command::Run => {
                let (cfg, warnings) = config::load();
                for w in warnings {
                    logln(format!("warning: {w}"));
                }
                run_forever(cfg).await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Command, String> {
        parse_args(list.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses_run_plan_and_default() {
        assert_eq!(args(&[]), Ok(Command::Run));
        assert_eq!(args(&["run"]), Ok(Command::Run));
        assert_eq!(args(&["plan"]), Ok(Command::Plan));
    }

    #[test]
    fn parses_once_flags() {
        assert_eq!(
            args(&["once", "--layer", "1"]),
            Ok(Command::Once {
                layer: 1,
                symbols: None,
                refresh: false
            })
        );
        assert_eq!(
            args(&[
                "once",
                "--layer",
                "2",
                "--symbols",
                "nvda, tsla",
                "--refresh"
            ]),
            Ok(Command::Once {
                layer: 2,
                symbols: Some(vec!["NVDA".into(), "TSLA".into()]),
                refresh: true
            })
        );
    }

    #[test]
    fn rejects_bad_commands() {
        assert!(args(&["frobnicate"]).is_err());
        assert!(args(&["once"]).is_err()); // ไม่มี --layer
        assert!(args(&["once", "--layer", "3"]).is_err()); // layer 3 ไม่มี
        assert!(args(&["once", "--layer", "1", "--wat"]).is_err());
    }
}
