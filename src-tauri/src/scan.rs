//! Scan worker — discovery → seed (stock / option snapshots / bars fallback)
//! → MQTT real-time push (WS priority over REST) → REST poll fallback,
//! pushing `scan://progress` and `scan://snapshot` events to the frontend.
//! Port of `Dashboard._stream_worker` + `_rest_fetch` (PLAN.md P3.4).

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::lock;
use crate::settings::AppSettings;
use radar_core::engine::{
    DataSource, Engine, EngineSnapshot, QuoteUpdate, StockUpdate, ws_stock_change,
};
use radar_core::webull::contracts::parse_occ;
use radar_core::webull::discovery::{fetch_symbols_flow, pick_ws_symbols};
use radar_core::webull::pb;
use radar_core::webull::quotes::{self, OI_BATCH};
use radar_core::webull::rest::{MAX_CONTRACTS, PATH_STREAMING_SUBSCRIBE};
use radar_core::webull::stream::{StreamClient, StreamConfig, StreamEvent};

/// REST poll interval when the real-time feed is silent (Python `_rest_tick`:
/// 180 s without WS, 60 s with).
const POLL_IDLE_SEC: u64 = 180;
const POLL_WS_ALIVE_SEC: u64 = 60;
/// WS considered alive if a message (incl. echo) arrived in the last 60 s.
const WS_FRESH_MS: u64 = 60_000;
/// Frontend snapshot emit throttle while WS quotes stream in.
const EMIT_INTERVAL: Duration = Duration::from_millis(800);
/// MQTT inactivity timeout (echo heartbeat arrives every ~30 s keepalive).
const WS_READ_TIMEOUT: Duration = Duration::from_secs(120);
const WS_RECONNECT_DELAY: Duration = Duration::from_secs(5);
/// Option subscribe chunk (Python `CHUNK_SIZE = 50`; client throttle paces calls).
const SUB_CHUNK: usize = 50;
/// Concurrent seed/bars workers. Each carries its own `WebullClient`, but all
/// of them share the origin coordinator, so the wire still sees ONE quota:
/// starts paced one per MIN_INTERVAL, at most `MAX_IN_FLIGHT` requests deep.
const SCAN_WORKERS: usize = 3;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn clip(s: &str) -> String {
    s.chars().take(160).collect()
}

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
        settings: AppSettings,
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
                run(app, engine, stop, running, gen_ctr, settings, root);
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
    let snap: EngineSnapshot = lock(engine).snapshot();
    let _ = app.emit("scan://snapshot", &snap);
}

#[allow(clippy::too_many_arguments)]
fn run(
    app: AppHandle,
    engine: Arc<Mutex<Engine>>,
    stop: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    gen_ctr: Arc<AtomicU64>,
    settings: AppSettings,
    root: String,
) {
    let my_gen = gen_ctr.fetch_add(1, Ordering::SeqCst) + 1;
    let stale = || stop.load(Ordering::Relaxed) || gen_ctr.load(Ordering::Relaxed) != my_gen;
    // A superseded scan exits after its successor has started and must not
    // report "not running" on that successor's behalf; an explicit stop may.
    let finish = |running: &Arc<AtomicBool>| {
        if stop.load(Ordering::SeqCst) || gen_ctr.load(Ordering::SeqCst) == my_gen {
            running.store(false, Ordering::SeqCst);
        }
    };

    let Some(mut client) = settings.webull_client() else {
        progress(
            &app,
            "error",
            "ไม่พบ Webull API keys — เปิดหน้า Setup เพื่อตั้งค่าก่อน",
        );
        return finish(&running);
    };

    let timing_on = std::env::var("RADAR_TIMING").ok().as_deref() == Some("1");
    let t0 = std::time::Instant::now();
    // RADAR_TIMING=1 → one stderr line per phase, milliseconds since scan
    // start. Only the root symbol and counts — never credentials or data.
    let timing = |phase: &str, detail: &str| {
        if timing_on {
            eprintln!(
                "[timing] {root} {phase} +{}ms{detail}",
                t0.elapsed().as_millis()
            );
        }
    };

    // ---- Phase 0: stock quote before discovery ----
    // The header price must not wait for the whole discovery pass; the loop
    // below still refreshes it on every pass.
    if let Ok(Some(mut stk)) = quotes::fetch_stock_update(&mut client, &root) {
        stk.symbol = root.clone();
        lock(&engine).push_stock(stk);
        emit_snapshot(&app, &engine);
        timing("quote-shown", "");
    }

    // ---- Phase 1: discovery ----
    progress(
        &app,
        "discovery",
        format!("ค้นหา option contracts ของ {root} ..."),
    );
    let mut on_progress = |msg: String| progress(&app, "discovery", msg);
    // The stale probe lets the windowed scan stop BETWEEN pages: a symbol
    // change retires the old discovery instead of letting it spend the quota.
    let flow = fetch_symbols_flow(
        &mut client,
        &root,
        quotes::ny_today(),
        Some(&stale as &dyn Fn() -> bool),
        Some(&mut on_progress),
    );
    let (symbols, spot) = match flow {
        Ok(v) => v,
        Err(e) => {
            if stale() {
                return finish(&running);
            }
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
    lock(&engine).set_symbols(symbols.clone(), Some(spot));
    timing("discovery-done", &format!(" contracts={}", symbols.len()));
    progress(
        &app,
        "seed",
        format!(
            "พบ {} contracts · spot {:.2} — กำลังดึงราคา ...",
            symbols.len(),
            spot
        ),
    );
    emit_snapshot(&app, &engine);

    // ---- Phase 2+3: seed pass → start real-time push → poll loop ----
    let ws_alive: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
    let mut first_pass = true;
    'outer: loop {
        if let Ok(Some(mut stk)) = quotes::fetch_stock_update(&mut client, &root) {
            stk.symbol = root.clone();
            lock(&engine).push_stock(stk);
        }
        if stale() {
            break 'outer;
        }

        let total = symbols.len();
        let mut empty: Vec<String> = Vec::new();
        {
            let batches: Vec<&[String]> = symbols.chunks(OI_BATCH).collect();
            // batches are pulled off this cursor; `seeded` counts completed
            // contracts so the n/total progress stays monotonic regardless of
            // which worker finishes first.
            let next_batch = AtomicUsize::new(0);
            let seeded = AtomicUsize::new(0);
            let misses: Mutex<Vec<String>> = Mutex::new(Vec::new());
            let mut workers: Vec<_> = (0..SCAN_WORKERS.min(batches.len()))
                .filter_map(|_| settings.webull_client())
                .collect();
            if workers.is_empty() {
                empty.extend(symbols.iter().cloned());
            } else {
                std::thread::scope(|scope| {
                    for worker in &mut workers {
                        scope.spawn(|| {
                            loop {
                                let i = next_batch.fetch_add(1, Ordering::Relaxed);
                                let Some(batch) = batches.get(i).copied() else {
                                    break;
                                };
                                if stale() {
                                    break;
                                }
                                let count_done = |len: usize| {
                                    (seeded.fetch_add(len, Ordering::Relaxed) + len).min(total)
                                };
                                let fetched = quotes::fetch_option_seeds(worker, batch);
                                // a symbol switch while this batch was in flight:
                                // its quotes must not land in the reset engine
                                if stale() {
                                    break;
                                }
                                match fetched {
                                    Ok((seeds, miss)) => {
                                        {
                                            let mut eng = lock(&engine);
                                            for q in seeds {
                                                eng.seed_quote(q);
                                            }
                                        }
                                        misses
                                            .lock()
                                            .unwrap_or_else(|e| e.into_inner())
                                            .extend(miss);
                                        let n = count_done(batch.len());
                                        progress(
                                            &app,
                                            "seed",
                                            format!("ดึงราคา snapshot {n}/{total}"),
                                        );
                                        emit_snapshot(&app, &engine);
                                        timing("seed-batch", &format!(" {n}/{total}"));
                                    }
                                    Err(_) => {
                                        misses
                                            .lock()
                                            .unwrap_or_else(|e| e.into_inner())
                                            .extend_from_slice(batch);
                                        count_done(batch.len());
                                    }
                                }
                            }
                        });
                    }
                });
                empty = misses.into_inner().unwrap_or_else(|e| e.into_inner());
            }
        }
        if stale() {
            break 'outer;
        }

        // bars fallback for contracts the snapshot could not price
        if !empty.is_empty() {
            progress(
                &app,
                "bars",
                format!("ดึงราคาปิดย้อนหลัง {} contracts", empty.len()),
            );
        }
        {
            let batches: Vec<&[String]> = empty.chunks(OI_BATCH).collect();
            let next_batch = AtomicUsize::new(0);
            let mut workers: Vec<_> = (0..SCAN_WORKERS.min(batches.len()))
                .filter_map(|_| settings.webull_client())
                .collect();
            if !workers.is_empty() {
                std::thread::scope(|scope| {
                    for worker in &mut workers {
                        scope.spawn(|| {
                            loop {
                                let i = next_batch.fetch_add(1, Ordering::Relaxed);
                                let Some(batch) = batches.get(i).copied() else {
                                    break;
                                };
                                if stale() {
                                    break;
                                }
                                let fetched = quotes::fetch_bars_seeds(worker, batch);
                                if stale() {
                                    break;
                                }
                                if let Ok(seeds) = fetched {
                                    let mut eng = lock(&engine);
                                    for q in seeds {
                                        eng.seed_quote(q);
                                    }
                                }
                            }
                        });
                    }
                });
            }
        }

        emit_snapshot(&app, &engine);
        timing("seed-done", &format!(" contracts={total}"));

        // after the first seed pass, layer real-time MQTT push on top
        // (WS quotes win over REST seeds in the engine; REST keeps polling
        // as fallback for the contracts outside the WS window)
        if first_pass {
            first_pass = false;
            let ws_symbols = pick_ws_symbols(&symbols, spot, MAX_CONTRACTS);
            progress(
                &app,
                "live",
                format!(
                    "seed ครบ — เริ่ม real-time push ({} contracts ใกล้ spot) + REST fallback",
                    ws_symbols.len()
                ),
            );
            spawn_ws(
                app.clone(),
                engine.clone(),
                stop.clone(),
                gen_ctr.clone(),
                my_gen,
                settings.clone(),
                root.clone(),
                ws_symbols,
                ws_alive.clone(),
            );
        }

        let ws_fresh = now_ms().saturating_sub(ws_alive.load(Ordering::Relaxed)) < WS_FRESH_MS;
        let interval = if ws_fresh {
            POLL_WS_ALIVE_SEC
        } else {
            POLL_IDLE_SEC
        };
        if !ws_fresh {
            progress(
                &app,
                "live",
                format!("อัปเดต REST แล้ว — รอบถัดไปใน {interval} วิ"),
            );
        }

        for _ in 0..interval {
            if stale() {
                break 'outer;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    finish(&running);
}

// ---------- real-time MQTT push (P3.4 — port of `_stream_worker`) ----------

/// Launch the real-time task on its own OS thread + single-threaded tokio
/// runtime. Shares the scan's generation: a symbol change (or stop) retires
/// this task along with its REST loop.
#[allow(clippy::too_many_arguments)]
fn spawn_ws(
    app: AppHandle,
    engine: Arc<Mutex<Engine>>,
    stop: Arc<AtomicBool>,
    gen_ctr: Arc<AtomicU64>,
    scan_gen: u64,
    settings: AppSettings,
    root: String,
    ws_symbols: Vec<String>,
    ws_alive: Arc<AtomicU64>,
) {
    let _ = std::thread::Builder::new()
        .name("radar-ws".into())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(ws_loop(
                app, engine, stop, gen_ctr, scan_gen, settings, root, ws_symbols, ws_alive,
            ));
        });
}

#[allow(clippy::too_many_arguments)]
async fn ws_loop(
    app: AppHandle,
    engine: Arc<Mutex<Engine>>,
    stop: Arc<AtomicBool>,
    gen_ctr: Arc<AtomicU64>,
    scan_gen: u64,
    settings: AppSettings,
    root: String,
    ws_symbols: Vec<String>,
    ws_alive: Arc<AtomicU64>,
) {
    let stale = || stop.load(Ordering::Relaxed) || gen_ctr.load(Ordering::Relaxed) != scan_gen;
    let base_url = settings.webull_base_url_or_default();
    let Some(creds) = settings.webull_credentials() else {
        return;
    };
    // session id mirrors the Python SDK: "th_" + uuid hex prefix
    let session_id = format!("th_{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);

    let mut attempt: u32 = 0;
    loop {
        if stale() {
            return;
        }
        attempt += 1;
        progress(
            &app,
            "live",
            format!("เชื่อมต่อ real-time (ครั้งที่ {attempt}) ..."),
        );

        let config = StreamConfig::for_base_url(&base_url, session_id.clone());
        let mut stream = StreamClient::new(config, creds.clone());
        let mut subscribed = false;
        let mut last_emit = std::time::Instant::now();

        let run_once = async {
            loop {
                if stale() {
                    return Ok(());
                }
                let ev = tokio::time::timeout(WS_READ_TIMEOUT, stream.event_loop().poll()).await;
                match ev {
                    Err(_) => return Err("120 วิไม่มีข้อความจาก server (รวม heartbeat)".into()),
                    Ok(Err(e)) => return Err(format!("{e}")),
                    Ok(Ok(event)) => {
                        let Some(se) = stream.handle_event(Ok(event)) else {
                            continue;
                        };
                        match se {
                            StreamEvent::Connected if !subscribed => {
                                subscribed = true;
                                if let Err(e) =
                                    do_subscribe(&settings, &session_id, &root, &ws_symbols).await
                                {
                                    return Err(format!("subscribe ล้มเหลว: {e}"));
                                }
                                ws_alive.store(now_ms(), Ordering::Relaxed);
                                progress(
                                    &app,
                                    "realtime",
                                    format!(
                                        "🟢 เชื่อมต่อสดแล้ว — push real-time {} contracts",
                                        ws_symbols.len()
                                    ),
                                );
                            }
                            StreamEvent::Snapshot { payload } => {
                                ws_alive.store(now_ms(), Ordering::Relaxed);
                                let Some(snap) = pb::decode_snapshot(&payload) else {
                                    continue;
                                };
                                let sym = snap.symbol.replace(' ', "");
                                if parse_occ(&sym).is_some() {
                                    // option contract snapshot — WS wins in the engine
                                    lock(&engine).push_quote(QuoteUpdate {
                                        symbol: sym,
                                        price: snap.price,
                                        volume: snap.volume,
                                        change: snap.change,
                                        change_ratio: snap.change_ratio,
                                        oi: snap.open_interest,
                                        bid: snap.bid,
                                        ask: snap.ask,
                                        src: DataSource::Ws,
                                    });
                                } else if sym == root {
                                    // protobuf absent = 0 — derive honestly, never +0.00
                                    let (change, change_ratio) = ws_stock_change(
                                        snap.change,
                                        snap.change_ratio,
                                        snap.price,
                                        snap.pre_close,
                                    );
                                    lock(&engine).push_stock(StockUpdate {
                                        symbol: root.clone(),
                                        price: snap.price,
                                        open: snap.open,
                                        high: snap.high,
                                        low: snap.low,
                                        pre_close: snap.pre_close,
                                        volume: snap.volume,
                                        change,
                                        change_ratio,
                                        src: DataSource::Ws,
                                    });
                                }
                                if last_emit.elapsed() >= EMIT_INTERVAL {
                                    last_emit = std::time::Instant::now();
                                    emit_snapshot(&app, &engine);
                                }
                            }
                            StreamEvent::Heartbeat => {
                                ws_alive.store(now_ms(), Ordering::Relaxed);
                            }
                            StreamEvent::Notice(_)
                            | StreamEvent::Connected
                            | StreamEvent::Disconnected => {}
                        }
                    }
                }
            }
        };

        let outcome = run_once.await;
        if stale() {
            return;
        }
        match outcome {
            Err(e) if e.contains("104") => {
                // Webull custom CONNACK code 104 = "Invalid AppKey": this app
                // key has no streaming entitlement on this environment.
                // Retrying cannot fix it — fall back to REST polling for good.
                progress(
                    &app,
                    "live",
                    "⚠ real-time ไม่พร้อมใช้: AppKey นี้ไม่มีสิทธิ์ streaming (Webull 104: Invalid \
                     AppKey) — ทำงานต่อด้วย REST snapshot (ทุก 3 นาที)",
                );
                return;
            }
            Err(e) if e.contains("105") => {
                // connection limit (max 5/app key) — back off harder
                progress(
                    &app,
                    "live",
                    format!("real-time หลุด ({e}: connection limit) — รอ 60 วิ"),
                );
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
            Err(e) => {
                progress(&app, "live", format!("real-time หลุด ({e}) — ต่อใหม่ใน 5 วิ"));
                tokio::time::sleep(WS_RECONNECT_DELAY).await;
            }
            Ok(()) => return,
        }
    }
}

/// HTTP subscribe (signed POST) — stock root on QUOTE+SNAPSHOT+TICK, option
/// chunks on SNAPSHOT. The shared coordinator paces the calls with everything
/// else; clean-session reconnects must re-run this.
async fn do_subscribe(
    settings: &AppSettings,
    session_id: &str,
    root: &str,
    ws_symbols: &[String],
) -> Result<(), String> {
    let settings = settings.clone();
    let sid = session_id.to_string();
    let root = root.to_string();
    let symbols = ws_symbols.to_vec();
    tokio::task::spawn_blocking(move || {
        let mut http = settings
            .webull_client()
            .ok_or_else(|| "ไม่พบ Webull API keys".to_string())?;
        let stock_body = serde_json::json!({
            "session_id": sid, "symbols": [root], "category": "US_STOCK",
            "sub_types": ["QUOTE", "SNAPSHOT", "TICK"],
        })
        .to_string();
        let (st, b) = http
            .post_json(PATH_STREAMING_SUBSCRIBE, &stock_body)
            .map_err(|e| e.to_string())?;
        if st != 200 {
            return Err(format!("stock subscribe {st}: {}", clip(&b.to_string())));
        }
        for chunk in symbols.chunks(SUB_CHUNK) {
            let body = serde_json::json!({
                "session_id": sid, "symbols": chunk, "category": "US_OPTION",
                "sub_types": ["SNAPSHOT"],
            })
            .to_string();
            let (st, b) = http
                .post_json(PATH_STREAMING_SUBSCRIBE, &body)
                .map_err(|e| e.to_string())?;
            if st != 200 {
                return Err(format!("options subscribe {st}: {}", clip(&b.to_string())));
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
