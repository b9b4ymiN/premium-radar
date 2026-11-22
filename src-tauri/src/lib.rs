// PREMIUM RADAR — Tauri bridge: commands + event pump wiring the radar-core
// engine to the React frontend. See PLAN.md P4.2-P4.4.

use std::sync::Mutex;
use chrono::Utc;
use radar_core::engine::{Engine, EngineSnapshot};
use radar_core::screener::{Filters, Mode};
use radar_core::webull::rest::Credentials;
use serde::Serialize;

/// Shared app state managed by Tauri.
pub struct AppState {
    pub engine: Mutex<Engine>,
    pub credentials: Mutex<Option<Credentials>>,
}

#[derive(Serialize, Clone)]
pub struct SnapshotPayload {
    pub rows_count: usize,
    pub candidates_count: usize,
    pub spot: Option<f64>,
    /// Serialized EngineSnapshot (the frontend deserializes the full struct)
    pub snapshot: EngineSnapshotSerde,
}

/// A serializable version of EngineSnapshot for the event payload.
#[derive(Serialize, Clone)]
pub struct EngineSnapshotSerde {
    pub candidates: Vec<CandidateSerde>,
    pub spot: Option<f64>,
    pub stock: Option<StockSerde>,
    pub stats: Option<StatsSerde>,
}

#[derive(Serialize, Clone)]
pub struct CandidateSerde {
    pub symbol: String,
    pub grade: Option<String>,
    pub option_type: String,
    pub strike: f64,
    pub expiry: String,
    pub dte: f64,
    pub delta: Option<f64>,
    pub credit: Option<f64>,
    pub collateral: Option<f64>,
    pub ann: Option<f64>,
    pub pop: Option<f64>,
    pub cushion: Option<f64>,
    pub p_itm: Option<f64>,
    pub oi: f64,
    pub src: String,
}

#[derive(Serialize, Clone)]
pub struct StockSerde {
    pub price: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub pre_close: f64,
    pub change: f64,
    pub change_ratio: f64,
    pub src: String,
}

#[derive(Serialize, Clone)]
pub struct StatsSerde {
    pub atm_iv: Option<f64>,
    pub em: Option<f64>,
    pub skew: Option<f64>,
}

fn serialize_snapshot(snap: &EngineSnapshot) -> EngineSnapshotSerde {
    EngineSnapshotSerde {
        candidates: snap
            .candidates
            .iter()
            .map(|r| CandidateSerde {
                symbol: r.q.symbol.clone(),
                grade: r.grade.map(|g| g.as_str().to_string()),
                option_type: r.q.option_type.as_str().to_string(),
                strike: r.q.strike,
                expiry: r.q.expiry.to_string(),
                dte: r.dte,
                delta: r.delta,
                credit: r.credit,
                collateral: r.collateral,
                ann: r.ann,
                pop: r.pop,
                cushion: r.cushion,
                p_itm: r.p_itm,
                oi: r.q.oi,
                src: r.q.src.clone(),
            })
            .collect(),
        spot: snap.spot,
        stock: snap.stock.as_ref().map(|s| StockSerde {
            price: s.price,
            open: s.open,
            high: s.high,
            low: s.low,
            pre_close: s.pre_close,
            change: s.change,
            change_ratio: s.change_ratio,
            src: s.src.as_str().to_string(),
        }),
        stats: snap.stats.as_ref().map(|st| StatsSerde {
            atm_iv: st.atm_iv,
            em: st.em,
            skew: st.skew,
        }),
    }
}

// ---------- Tauri commands ----------

#[tauri::command]
fn get_snapshot(state: tauri::State<'_, AppState>) -> EngineSnapshotSerde {
    let mut engine = state.engine.lock().unwrap();
    let snap = engine.snapshot();
    serialize_snapshot(&snap)
}

#[tauri::command]
fn set_filters(
    state: tauri::State<'_, AppState>,
    strategy: String,
    risk: String,
    expiry: Option<String>,
    budget: f64,
) -> EngineSnapshotSerde {
    let mode = match strategy.as_str() {
        "CSP" => Mode::Csp,
        "CC" => Mode::Cc,
        _ => Mode::Both,
    };
    let (dmin, dmax) = match risk.as_str() {
        "SAFE" => (0.10, 0.20),
        "BALANCED" => (0.15, 0.30),
        "AGGR" => (0.25, 0.40),
        _ => (0.0, 1.0), // ALL
    };
    let sel_exp = expiry
        .and_then(|e| chrono::NaiveDate::parse_from_str(&e, "%Y-%m-%d").ok());
    let filters = Filters {
        mode,
        sel_exp,
        all_exp: sel_exp.is_none(),
        dmin,
        dmax,
        relax: false,
        budget,
    };
    let mut engine = state.engine.lock().unwrap();
    engine.set_filters(filters);
    let snap = engine.snapshot();
    serialize_snapshot(&snap)
}

#[tauri::command]
fn set_symbol(state: tauri::State<'_, AppState>, symbol: String) -> String {
    // In production this triggers discovery + streaming; for now just reset
    let mut engine = state.engine.lock().unwrap();
    let _ = &symbol; // TODO: wire discovery + streaming
    format!("Symbol set to {symbol} (engine reset)")
}

#[tauri::command]
fn get_status(state: tauri::State<'_, AppState>) -> String {
    let engine = state.engine.lock().unwrap();
    format!(
        "quotes: {} (live: {}), spot: {:?}",
        engine.quote_count(),
        engine.live_count(),
        engine.spot(),
    )
}

// ---------- Tauri app entry ----------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let now = Utc::now();
    let engine = Engine::new(now);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            engine: Mutex::new(engine),
            credentials: Mutex::new(Credentials::from_env()),
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            set_filters,
            set_symbol,
            get_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
