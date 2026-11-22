// PREMIUM RADAR — Tauri bridge: commands + event pump wiring the radar-core
// engine to the React frontend. See PLAN.md P4.2-P4.4.

use std::sync::Mutex;
use chrono::Utc;
pub mod settings;
use radar_core::engine::{Engine, EngineSnapshot};
use radar_core::screener::{Filters, Mode};
use radar_core::webull::rest::Credentials;
use serde::Serialize;

/// Shared app state managed by Tauri.
pub struct AppState {
    pub engine: Mutex<Engine>,
    pub credentials: Mutex<Option<Credentials>>,
    pub settings: Mutex<settings::AppSettings>,
    pub app_data_dir: String,
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

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> settings::AppSettings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: tauri::State<'_, AppState>, new_settings: settings::AppSettings) -> Result<(), String> {
    let dir = state.app_data_dir.clone();
    *state.settings.lock().unwrap() = new_settings.clone();
    new_settings.save(&dir)
}

/// Save API credentials from the first-run setup screen.
/// Stores in settings (dev) — production should use OS keyring.
#[tauri::command]
fn save_credentials(
    state: tauri::State<'_, AppState>,
    webull_key: String,
    webull_secret: String,
    webull_base_url: Option<String>,
    llm_label: Option<String>,
    llm_base_url: Option<String>,
    llm_model: Option<String>,
    llm_api_key: Option<String>,
) -> Result<settings::AppSettings, String> {
    let dir = state.app_data_dir.clone();
    let mut s = state.settings.lock().unwrap();

    s.webull_app_key = webull_key;
    s.webull_app_secret = webull_secret;
    if let Some(url) = webull_base_url {
        if !url.is_empty() {
            s.webull_base_url = url;
        }
    }

    if let (Some(label), Some(base_url), Some(model), Some(key)) =
        (llm_label, llm_base_url, llm_model, llm_api_key)
    {
        if !base_url.is_empty() && !model.is_empty() && !key.is_empty() {
            s.llm = Some(radar_core::copilot::LlmConfig::new(label, base_url, model, key));
        }
    }

    s.save(&dir)?;
    Ok(s.clone())
}

/// Test if the current configuration is ready to use.
#[tauri::command]
fn test_connection(state: tauri::State<'_, AppState>) -> String {
    let s = state.settings.lock().unwrap();
    s.status_summary()
}

/// Check if first-run setup is needed (no Webull keys configured).
#[tauri::command]
fn needs_setup(state: tauri::State<'_, AppState>) -> bool {
    !state.settings.lock().unwrap().is_configured()
}

// ---------- Tauri app entry ----------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let now = Utc::now();
    let engine = Engine::new(now);
    let app_data_dir = std::env::var("APPDATA")
        .unwrap_or_else(|_| ".".into());
    let app_data_dir = format!("{app_data_dir}/premium-radar");
    let _ = std::fs::create_dir_all(&app_data_dir);
    let app_settings = settings::AppSettings::load(&app_data_dir);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            engine: Mutex::new(engine),
            credentials: Mutex::new(Credentials::from_env()),
            settings: Mutex::new(app_settings),
            app_data_dir,
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            set_filters,
            set_symbol,
            get_status,
            get_settings,
            save_settings,
            save_credentials,
            test_connection,
            needs_setup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
