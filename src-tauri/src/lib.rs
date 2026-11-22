// PREMIUM RADAR — Tauri bridge: commands + the REST scan worker wiring the
// radar-core engine to the React frontend. See PLAN.md P4.2-P4.4.

use std::sync::Mutex;

use chrono::Utc;
pub mod scan;
pub mod settings;

use radar_core::engine::{Engine, EngineSnapshot};
use radar_core::screener::{Filters, Mode};
use scan::ScanControl;
use settings::AppSettings;

/// Shared app state managed by Tauri. The engine lives behind an Arc so the
/// scan worker thread can push quotes into the same store the commands read.
pub struct AppState {
    pub engine: std::sync::Arc<Mutex<Engine>>,
    pub settings: Mutex<AppSettings>,
    pub scan: ScanControl,
    pub app_data_dir: String,
}

// ---------- Tauri commands ----------

#[tauri::command]
fn get_snapshot(state: tauri::State<'_, AppState>) -> EngineSnapshot {
    state.engine.lock().unwrap().snapshot()
}

#[tauri::command]
fn set_filters(
    state: tauri::State<'_, AppState>,
    strategy: String,
    risk: String,
    expiry: Option<String>,
    budget: f64,
) -> EngineSnapshot {
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
    engine.snapshot()
}

/// Start (or restart) the REST scan for a symbol. Supersedes any running scan.
#[tauri::command]
fn start_scan(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    symbol: String,
) -> Result<(), String> {
    let symbol = symbol.trim().to_uppercase();
    if symbol.is_empty() {
        return Err("กรุณาระบุ symbol".into());
    }
    let settings = state.settings.lock().unwrap().clone();
    let Some(creds) = settings.webull_credentials() else {
        return Err("ไม่พบ Webull API keys — เปิดหน้า Setup เพื่อตั้งค่าก่อน".into());
    };
    let base_url = if settings.webull_base_url.is_empty() {
        "https://api.webull.co.th".to_string()
    } else {
        settings.webull_base_url.clone()
    };

    state.scan.spawn(app, state.engine.clone(), creds, base_url, symbol.clone());

    // remember the last symbol for the next session
    {
        let mut s = state.settings.lock().unwrap();
        s.last_symbol = symbol;
        let dir = state.app_data_dir.clone();
        let _ = s.save(&dir);
    }
    Ok(())
}

#[tauri::command]
fn stop_scan(state: tauri::State<'_, AppState>) -> bool {
    state.scan.request_stop();
    true
}

#[tauri::command]
fn get_status(state: tauri::State<'_, AppState>) -> String {
    let engine = state.engine.lock().unwrap();
    format!(
        "quotes: {} (live: {}), spot: {:?}, scanning: {}",
        engine.quote_count(),
        engine.live_count(),
        engine.spot(),
        state.scan.is_running(),
    )
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> AppSettings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: tauri::State<'_, AppState>, new_settings: AppSettings) -> Result<(), String> {
    let dir = state.app_data_dir.clone();
    *state.settings.lock().unwrap() = new_settings.clone();
    new_settings.save(&dir)
}

/// Save API credentials from the first-run setup screen.
/// Stores in settings (dev) — production should use OS keyring.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn save_credentials(
    state: tauri::State<'_, AppState>,
    webull_key: String,
    webull_secret: String,
    webull_base_url: Option<String>,
    webull_access_token: Option<String>,
    llm_label: Option<String>,
    llm_base_url: Option<String>,
    llm_model: Option<String>,
    llm_api_key: Option<String>,
) -> Result<AppSettings, String> {
    let dir = state.app_data_dir.clone();
    let mut s = state.settings.lock().unwrap();

    s.webull_app_key = webull_key.trim().to_string();
    s.webull_app_secret = webull_secret.trim().to_string();
    s.webull_access_token = webull_access_token.unwrap_or_default().trim().to_string();
    if let Some(url) = webull_base_url
        && !url.trim().is_empty()
    {
        s.webull_base_url = url.trim().to_string();
    }

    if let (Some(label), Some(base_url), Some(model), Some(key)) =
        (llm_label, llm_base_url, llm_model, llm_api_key)
        && !base_url.is_empty()
        && !model.is_empty()
        && !key.is_empty()
    {
        s.llm = Some(radar_core::copilot::LlmConfig::new(label, base_url, model, key));
    }

    s.save(&dir)?;
    Ok(s.clone())
}

/// Result of a real Webull REST probe (used by the setup screen).
#[derive(serde::Serialize)]
pub struct ConnectionTestResult {
    pub webull_ok: bool,
    pub detail: String,
}

/// Probe the Webull API with either the given keys (setup screen, unsaved) or
/// the stored settings — one cheap stock-snapshot call, so 401/403/timeout
/// surface before the user starts a full scan.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn test_connection(
    state: tauri::State<'_, AppState>,
    webull_key: Option<String>,
    webull_secret: Option<String>,
    webull_access_token: Option<String>,
    webull_base_url: Option<String>,
) -> ConnectionTestResult {
    let s = state.settings.lock().unwrap().clone();
    let key = webull_key
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .unwrap_or(s.webull_app_key.trim().to_string());
    let secret = webull_secret
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or(s.webull_app_secret.trim().to_string());
    let token = webull_access_token
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| {
            let t = s.webull_access_token.trim();
            (!t.is_empty()).then(|| t.to_string())
        });
    let base_url = webull_base_url
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            if s.webull_base_url.is_empty() {
                "https://api.webull.co.th".to_string()
            } else {
                s.webull_base_url.clone()
            }
        });

    if key.is_empty() || secret.is_empty() {
        return ConnectionTestResult {
            webull_ok: false,
            detail: "ยังไม่ได้ใส่ App Key / Secret".into(),
        };
    }

    let creds = radar_core::webull::rest::Credentials {
        app_key: key.clone(),
        app_secret: secret.clone(),
        access_token: token.clone(),
    };
    let mut client = radar_core::webull::transport::WebullClient::new(creds).with_base_url(base_url.clone());

    // Token status probe — SIGNING ONLY (no x-access-token attached, matching
    // the vcpSystem bootstrap), and NON-FATAL: an endpoint 401 here must not
    // stop the real data probe below, which is the ground truth.
    let mut tok_note = String::new();
    if let Some(tok) = &token {
        let auth_only = radar_core::webull::rest::Credentials {
            app_key: key.clone(),
            app_secret: secret.clone(),
            access_token: None,
        };
        let mut authc =
            radar_core::webull::transport::WebullClient::new(auth_only).with_base_url(base_url.clone());
        tok_note = match radar_core::webull::auth::check_token(&mut authc, tok) {
            Ok(tc) if tc.status == radar_core::webull::auth::TokenStatus::Normal => {
                " · token NORMAL".into()
            }
            Ok(tc) => format!(" · token status = {}", tc.status.as_str()),
            Err(_) => " · (เช็ค token status ไม่สำเร็จ)".into(),
        };
    }

    let probe = |c: &mut radar_core::webull::transport::WebullClient,
                 sym: &str|
     -> Result<Option<f64>, String> {
        match radar_core::webull::quotes::fetch_stock_update(c, sym) {
            Ok(stk) => Ok(stk.map(|s| s.price)),
            Err(e) => Err(format!("{e}")),
        }
    };

    match probe(&mut client, "AAPL") {
        Ok(Some(price)) => ConnectionTestResult {
            webull_ok: true,
            detail: format!("เชื่อมต่อสำเร็จ — AAPL ${price:.2}{tok_note}"),
        },
        Ok(None) => ConnectionTestResult {
            webull_ok: false,
            detail: "ตอบกลับ 200 แต่ไม่พบข้อมูล (เช็คสิทธิ์ดูข้อมูล US ของ app key นี้)".into(),
        },
        Err(with_tok_err) if token.is_some() => {
            // The token may be the problem — retry the same probe WITHOUT it.
            let no_tok = radar_core::webull::rest::Credentials {
                app_key: key,
                app_secret: secret,
                access_token: None,
            };
            let mut bare =
                radar_core::webull::transport::WebullClient::new(no_tok).with_base_url(base_url);
            match probe(&mut bare, "AAPL") {
                Ok(Some(price)) => ConnectionTestResult {
                    webull_ok: false,
                    detail: format!(
                        "ใส่ Access Token แล้วโดน 401 แต่ตัด token ออกแล้วผ่าน (AAPL ${price:.2}) \
                         — token นี้ใช้ไม่ได้: ลบค่าในช่อง Access Token ทิ้งแล้วบันทึกได้เลย"
                    ),
                },
                _ => ConnectionTestResult {
                    webull_ok: false,
                    detail: format!(
                        "{with_tok_err}{tok_note} — ส่งทั้งมีและไม่มี token ก็ 401: \
                         key อาจเป็นของ UAT (ลอง Base URL https://th-api.uat.webullbroker.com) \
                         หรือ app key นี้ไม่มีสิทธิ์ market-data"
                    ),
                },
            }
        }
        Err(e) => {
            let d = e.to_string();
            let hint = if d.contains("401") || d.to_lowercase().contains("unauthorized") {
                " — Key/Secret ถูกแต่ไม่ผ่าน: ลองใส่ Access Token ของ vcpSystem \
                 (.env: WEBULL_ACCESS_TOKEN) หรือลอง Base URL ของ UAT \
                 (https://th-api.uat.webullbroker.com) ถ้า key เป็นคู่ทดสอบ"
            } else if d.contains("403") {
                " — key ถูกต้องแต่ไม่มีสิทธิ์เรียก endpoint นี้"
            } else {
                ""
            };
            ConnectionTestResult {
                webull_ok: false,
                detail: format!("{d}{hint}"),
            }
        }
    }
}

/// Ask the AI copilot a question, grounded in the live engine data via
/// function calling (top_candidates / contract_detail / chain_stats).
#[tauri::command]
fn ask_copilot(
    state: tauri::State<'_, AppState>,
    messages: Vec<radar_core::copilot::ChatMessage>,
) -> Result<String, String> {
    let cfg = state
        .settings
        .lock()
        .unwrap()
        .llm_config()
        .ok_or("ยังไม่ได้ตั้งค่า AI — เปิดหน้า Setup (ไอคอน ⚙) เพื่อใส่ LLM")?;
    let mut engine = state.engine.lock().unwrap();
    radar_core::copilot::chat_with_tools(&cfg, &messages, &mut engine)
}

/// Probe the LLM endpoint with either the given values (setup screen,
/// unsaved) or the stored settings — one tiny chat completion.
#[tauri::command]
fn test_llm(
    state: tauri::State<'_, AppState>,
    llm_label: Option<String>,
    llm_base_url: Option<String>,
    llm_model: Option<String>,
    llm_api_key: Option<String>,
) -> ConnectionTestResult {
    let s = state.settings.lock().unwrap().clone();
    let pick = |o: Option<String>, cur: &str| {
        o.map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| cur.trim().to_string())
    };
    let stored = s.llm.clone().unwrap_or_else(|| {
        radar_core::copilot::LlmConfig::new("", "", "", "")
    });
    let cfg = radar_core::copilot::LlmConfig {
        label: pick(llm_label, &stored.label),
        base_url: pick(llm_base_url, &stored.base_url),
        model: pick(llm_model, &stored.model),
        api_key: pick(llm_api_key, &stored.api_key),
        temperature: 0.0,
        max_tokens: 0,
    };
    if !cfg.is_ready() {
        return ConnectionTestResult {
            webull_ok: false,
            detail: "ยังไม่ได้ใส่ Base URL / Model / API Key ของ AI".into(),
        };
    }
    match radar_core::copilot::ping(&cfg) {
        Ok(reply) => ConnectionTestResult {
            webull_ok: true,
            detail: format!("AI ตอบกลับจริง ({}): {}", cfg.model, reply.chars().take(80).collect::<String>()),
        },
        Err(e) => ConnectionTestResult {
            webull_ok: false,
            detail: e,
        },
    }
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
            engine: std::sync::Arc::new(Mutex::new(engine)),
            settings: Mutex::new(app_settings),
            scan: ScanControl::default(),
            app_data_dir,
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            set_filters,
            start_scan,
            stop_scan,
            get_status,
            get_settings,
            save_settings,
            save_credentials,
            test_connection,
            needs_setup,
            ask_copilot,
            test_llm,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
