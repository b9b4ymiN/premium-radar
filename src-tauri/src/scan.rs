//! REST scan worker — discovery → seed (stock / option snapshots / bars
//! fallback) → poll loop, pushing `scan://progress` and `scan://snapshot`
//! events to the frontend. Port of the REST half of `Dashboard`; MQTT
//! streaming (WS priority) layers on top later (PLAN.md P3.4).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use radar_core::engine::{Engine, EngineSnapshot};
use radar_core::webull::discovery::fetch_symbols_flow;
use radar_core::webull::quotes::{self, OI_BATCH};
use radar_core::webull::rest::Credentials;
use radar_core::webull::transport::WebullClient;

/// Poll interval while no WebSocket feed is connected (Python `_rest_tick`
/// uses 180 s without WS, 60 s with).
const POLL_INTERVAL_SEC: u64 = 180;

/// Shared scan lifecycle: `stop` kills everything, `gen_ctr` invalidates older
/// scans when the symbol changes, `running` reports liveness to the UI.
#[derive(Default)]
pub struct ScanControl {
    stop: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    gen_ctr: Arc<AtomicU64>,
}

impl ScanControl {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Stop any active scan (app exit / explicit stop).
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.gen_ctr.fetch_add(1, Ordering::SeqCst); // invalidate in-flight scans
    }

    /// Launch a scan for `root`; supersedes any running scan.
    pub fn spawn(
        &self,
        app: AppHandle,
        engine: Arc<Mutex<Engine>>,
        creds: Credentials,
        base_url: String,
        root: String,
    ) {
        self.stop.store(false, Ordering::SeqCst);
        let stop = self.stop.clone();
        let running = self.running.clone();
        let gen_ctr = self.gen_ctr.clone();
        running.store(true, Ordering::SeqCst);
        std::thread::Builder::new()
            .name("radar-scan".into())
            .spawn(move || {
                run(app, engine, stop, running, gen_ctr, creds, base_url, root);
            })
            .expect("spawn radar-scan thread");
    }
}

#[derive(Serialize, Clone)]
pub struct ScanProgress {
    /// discovery | seed | bars | live | error
    pub phase: String,
    pub message: String,
}

fn progress(app: &AppHandle, phase: &str, message: impl Into<String>) {
    let _ = app.emit(
        "scan://progress",
        ScanProgress {
            phase: phase.into(),
            message: message.into(),
        },
    );
}

fn emit_snapshot(app: &AppHandle, engine: &Arc<Mutex<Engine>>) {
    let snap: EngineSnapshot = engine.lock().unwrap().snapshot();
    let _ = app.emit("scan://snapshot", &snap);
}

#[allow(clippy::too_many_arguments)]
fn run(
    app: AppHandle,
    engine: Arc<Mutex<Engine>>,
    stop: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    gen_ctr: Arc<AtomicU64>,
    creds: Credentials,
    base_url: String,
    root: String,
) {
    let my_gen = gen_ctr.fetch_add(1, Ordering::SeqCst) + 1;
    let stale = || stop.load(Ordering::Relaxed) || gen_ctr.load(Ordering::Relaxed) != my_gen;
    let finish = |running: &Arc<AtomicBool>| running.store(false, Ordering::SeqCst);

    let mut client = WebullClient::new(creds).with_base_url(base_url);

    // ---- Phase 1: discovery ----
    progress(&app, "discovery", format!("ค้นหา option contracts ของ {root} ..."));
    let mut on_progress = |msg: String| progress(&app, "discovery", msg);
    let flow = fetch_symbols_flow(&mut client, &root, quotes::ny_today(), None, Some(&mut on_progress));
    let (symbols, spot) = match flow {
        Ok(v) => v,
        Err(e) => {
            let mut msg = format!("discovery ล้มเหลว: {e}");
            if msg.contains("401") || msg.to_lowercase().contains("unauthorized") {
                msg.push_str(" — Key/Secret ไม่ถูกต้องหรือขาด Access Token (กดปุ่ม Setup เพื่อแก้ไข)");
            }
            progress(&app, "error", msg);
            return finish(&running);
        }
    };
    if stale() {
        return finish(&running);
    }
    engine
        .lock()
        .unwrap()
        .set_symbols(symbols.clone(), Some(spot));
    progress(
        &app,
        "seed",
        format!("พบ {} contracts · spot {:.2} — กำลังดึงราคา ...", symbols.len(), spot),
    );
    emit_snapshot(&app, &engine);

    // ---- Phase 2+3: seed pass, then poll loop ----
    'outer: loop {
        if let Ok(Some(stk)) = quotes::fetch_stock_update(&mut client, &root) {
            engine.lock().unwrap().push_stock(stk);
        }
        if stale() {
            break 'outer;
        }

        let total = symbols.len();
        let mut empty: Vec<String> = Vec::new();
        for (i, batch) in symbols.chunks(OI_BATCH).enumerate() {
            if stale() {
                break 'outer;
            }
            match quotes::fetch_option_seeds(&mut client, batch) {
                Ok((seeds, miss)) => {
                    {
                        let mut eng = engine.lock().unwrap();
                        for q in seeds {
                            eng.seed_quote(q);
                        }
                    }
                    empty.extend(miss);
                    progress(
                        &app,
                        "seed",
                        format!("ดึงราคา snapshot {}/{}", ((i + 1) * OI_BATCH).min(total), total),
                    );
                    emit_snapshot(&app, &engine);
                }
                Err(_) => empty.extend_from_slice(batch),
            }
        }

        // bars fallback for contracts the snapshot could not price
        if !empty.is_empty() {
            progress(&app, "bars", format!("ดึงราคาปิดย้อนหลัง {} contracts", empty.len()));
        }
        for batch in empty.chunks(OI_BATCH) {
            if stale() {
                break 'outer;
            }
            if let Ok(seeds) = quotes::fetch_bars_seeds(&mut client, batch) {
                let mut eng = engine.lock().unwrap();
                for q in seeds {
                    eng.seed_quote(q);
                }
            }
        }

        emit_snapshot(&app, &engine);
        progress(&app, "live", "อัปเดตแล้ว — รอบถัดไปใน 3 นาที");

        for _ in 0..POLL_INTERVAL_SEC {
            if stale() {
                break 'outer;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    finish(&running);
}
