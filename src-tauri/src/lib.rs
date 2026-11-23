// PREMIUM RADAR — Tauri bridge: commands + the REST scan worker wiring the
// radar-core engine to the React frontend. See PLAN.md P4.2-P4.4.

use std::sync::Mutex;

use chrono::Utc;
pub mod scan;
pub mod settings;

use radar_core::engine::{Engine, EngineSnapshot};
use radar_core::screener::{Filters, Mode};
use radar_core::webull::sign::SignatureScheme;
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
    let sel_exp = expiry.and_then(|e| chrono::NaiveDate::parse_from_str(&e, "%Y-%m-%d").ok());
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
    if settings.webull_credentials().is_none() {
        return Err("ไม่พบ Webull API keys — เปิดหน้า Setup เพื่อตั้งค่าก่อน".into());
    }

    state
        .scan
        .spawn(app, state.engine.clone(), settings, symbol.clone());

    // remember the last symbol for the next session (persist goes through
    // the keyring path so the scrubbed file never regains plaintext secrets;
    // fail-closed — on keyring error this preference is simply not saved)
    {
        let mut s = state.settings.lock().unwrap();
        s.last_symbol = symbol;
        let dir = state.app_data_dir.clone();
        if let Err(e) = s.save_with_keyring(&dir) {
            eprintln!("[settings] keyring save ไม่สำเร็จ ข้ามการบันทึก preference ครั้งนี้: {e}");
        }
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
fn save_settings(
    state: tauri::State<'_, AppState>,
    mut new_settings: AppSettings,
) -> Result<(), String> {
    let dir = state.app_data_dir.clone();
    new_settings.save_with_keyring(&dir)?;
    *state.settings.lock().unwrap() = new_settings;
    Ok(())
}

/// Save API credentials from the setup screen — secrets go to the OS keyring
/// (Windows Credential Manager / macOS Keychain). The candidate is staged
/// under the settings mutex and swapped into the shared state only after the
/// save fully succeeds: a keyring failure aborts with an error, leaving both
/// the running credentials and the previous settings file untouched (nothing
/// is ever written to disk as plaintext).
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
    webull_signature_scheme: Option<SignatureScheme>,
) -> Result<AppSettings, String> {
    let dir = state.app_data_dir.clone();
    let mut s = state.settings.lock().unwrap();

    // stage on a copy — the shared state must keep the previously saved
    // (and still working) credentials until this save is fully committed
    let mut candidate = s.clone();
    candidate.webull_app_key = webull_key.trim().to_string();
    candidate.webull_app_secret = webull_secret.trim().to_string();
    candidate.webull_access_token = webull_access_token.unwrap_or_default().trim().to_string();
    if let Some(scheme) = webull_signature_scheme {
        candidate.webull_signature_scheme = scheme;
    }
    if let Some(url) = webull_base_url
        && !url.trim().is_empty()
    {
        candidate.webull_base_url = url.trim().to_string();
    }

    if let (Some(label), Some(base_url), Some(model), Some(key)) =
        (llm_label, llm_base_url, llm_model, llm_api_key)
        && !base_url.is_empty()
        && !model.is_empty()
        && !key.is_empty()
    {
        candidate.llm = Some(radar_core::copilot::LlmConfig::new(
            label, base_url, model, key,
        ));
    }

    candidate.save_with_keyring(&dir)?;
    *s = candidate;
    Ok(s.clone())
}

/// Result of a real Webull REST probe (used by the setup screen).
#[derive(serde::Serialize)]
pub struct ConnectionTestResult {
    pub webull_ok: bool,
    pub detail: String,
    /// Streaming (MQTT) entitlement verdict from a connect-only probe:
    /// "entitled" | "limit" | "none" | "unknown" — None when REST already failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streaming: Option<String>,
    /// Set only when the configured signing scheme was rejected and the other
    /// one worked — the setup screen then saves it together with the keys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_scheme: Option<SignatureScheme>,
}

/// Connect-only MQTT probe on a dedicated runtime (the command thread is sync).
/// Verdict only — never blocks longer than ~8 s.
fn streaming_verdict(creds: &radar_core::webull::rest::Credentials, base_url: &str) -> String {
    let sid = format!("probe_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let config = radar_core::webull::stream::StreamConfig::for_base_url(base_url, sid);
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return "unknown".into();
    };
    match rt.block_on(radar_core::webull::stream::probe_entitlement(
        config,
        creds.clone(),
    )) {
        radar_core::webull::stream::Entitlement::Entitled => "entitled".into(),
        radar_core::webull::stream::Entitlement::ConnectionLimit => "limit".into(),
        radar_core::webull::stream::Entitlement::NotEntitled => "none".into(),
        radar_core::webull::stream::Entitlement::Unreachable => "unknown".into(),
    }
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
    let scheme = s.webull_signature_scheme;
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
            streaming: None,
            signature_scheme: None,
        };
    }

    let make_client = |scheme: SignatureScheme, with_token: bool| {
        radar_core::webull::transport::WebullClient::new(radar_core::webull::rest::Credentials {
            app_key: key.clone(),
            app_secret: secret.clone(),
            access_token: if with_token { token.clone() } else { None },
        })
        .with_base_url(base_url.clone())
        .with_signature_scheme(scheme)
    };
    let probe = |client: &mut radar_core::webull::transport::WebullClient| {
        radar_core::webull::quotes::fetch_stock_update(client, "AAPL")
            .map(|stock| stock.map(|stock| stock.price))
            .map_err(|e| e.to_string())
    };

    // Token status probe — SIGNING ONLY (no x-access-token attached), and
    // NON-FATAL: an endpoint 401 here must not stop the data probe below,
    // which is the ground truth.
    let mut tok_note = String::new();
    if let Some(tok) = &token {
        tok_note = match radar_core::webull::auth::check_token(&mut make_client(scheme, false), tok)
        {
            Ok(tc) if tc.status == radar_core::webull::auth::TokenStatus::Normal => {
                let exp = tc
                    .expires_ms
                    .filter(|&ms| ms > 0)
                    .and_then(|ms| chrono::DateTime::from_timestamp_millis(ms as i64))
                    .map(|d| d.format("· token NORMAL (หมดอายุ %d %b %Y)").to_string())
                    .unwrap_or_else(|| "· token NORMAL".into());
                format!(" {exp}")
            }
            Ok(tc) => format!(" · token status = {}", tc.status.as_str()),
            Err(_) => " · (เช็ค token status ไม่สำเร็จ)".into(),
        };
    }

    // creds for the MQTT entitlement probe
    let probe_creds = radar_core::webull::rest::Credentials {
        app_key: key.clone(),
        app_secret: secret.clone(),
        access_token: None,
    };

    // The configured signing scheme goes first, then the other one. Each is
    // tried with the token and — because a stale token alone can cause a 401
    // on hosts that do not need one — without it.
    let mut attempts: Vec<(SignatureScheme, bool)> = Vec::new();
    for candidate in [scheme, scheme.other()] {
        attempts.push((candidate, true));
        if token.is_some() {
            attempts.push((candidate, false));
        }
    }
    let mut first_error: Option<String> = None;
    for (candidate, with_token) in attempts {
        let switched = (candidate != scheme).then_some(candidate);
        match probe(&mut make_client(candidate, with_token)) {
            Ok(Some(price)) if token.is_some() && !with_token => {
                // the same scheme passes once the token is removed
                return ConnectionTestResult {
                    webull_ok: false,
                    detail: format!(
                        "ใส่ Access Token แล้วโดน 401 แต่ตัด token ออกแล้วผ่าน (AAPL ${price:.2}) \
                         — token นี้ใช้ไม่ได้: ลบค่าในช่อง Access Token ทิ้งแล้วบันทึกได้เลย"
                    ),
                    streaming: None,
                    signature_scheme: switched,
                };
            }
            Ok(Some(price)) => {
                let scheme_note = if candidate == scheme {
                    String::new()
                } else {
                    format!(" · ใช้ลายเซ็น {}", candidate.x_version())
                };
                return ConnectionTestResult {
                    webull_ok: true,
                    detail: format!("เชื่อมต่อสำเร็จ — AAPL ${price:.2}{tok_note}{scheme_note}"),
                    streaming: Some(streaming_verdict(&probe_creds, &base_url)),
                    signature_scheme: switched,
                };
            }
            Ok(None) => {
                first_error.get_or_insert_with(|| {
                    "ตอบกลับ 200 แต่ไม่พบข้อมูล (เช็คสิทธิ์ดูข้อมูล US ของ app key นี้)".to_string()
                });
            }
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }

    let d = first_error.unwrap_or_default();
    let hint = if d.contains("401")
        || d.contains("INVALID_TOKEN")
        || d.to_lowercase().contains("unauthorized")
    {
        " — Key/Secret ถูกแต่ไม่ผ่าน: สร้าง Access Token ใหม่ด้วยปุ่ม \
         \"สร้าง Access Token ใหม่ (SMS)\" (ยืนยัน SMS ในแอป Webull) \
         หรือลอง Base URL ของ UAT (https://th-api.uat.webullbroker.com) ถ้า key เป็นคู่ทดสอบ"
    } else if d.contains("403") {
        " — key ถูกต้องแต่ไม่มีสิทธิ์เรียก endpoint นี้"
    } else {
        ""
    };
    ConnectionTestResult {
        webull_ok: false,
        detail: format!("{d}{tok_note}{hint}"),
        streaming: None,
        signature_scheme: None,
    }
}

/// Result of the in-app token creation flow.
#[derive(serde::Serialize)]
pub struct CreateTokenResult {
    pub ok: bool,
    pub status: String,
    pub token: Option<String>,
    pub expires_ms: Option<u64>,
    pub detail: String,
}

/// Start access-token creation (production needs one for data endpoints):
/// POSTs `/openapi/auth/token/create` — the token VALUE arrives immediately
/// as PENDING and the broker SMSes a code to the account's bound phone. The
/// user confirms in the Webull app within 5 minutes; `test_connection` then
/// reports NORMAL.
#[tauri::command]
fn create_access_token(
    state: tauri::State<'_, AppState>,
    webull_key: Option<String>,
    webull_secret: Option<String>,
    webull_base_url: Option<String>,
) -> CreateTokenResult {
    let s = state.settings.lock().unwrap().clone();
    let key = webull_key
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| s.webull_app_key.trim().to_string());
    let secret = webull_secret
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| s.webull_app_secret.trim().to_string());
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
        return CreateTokenResult {
            ok: false,
            status: "NONE".into(),
            token: None,
            expires_ms: None,
            detail: "ยังไม่ได้ใส่ App Key / Secret".into(),
        };
    }
    let creds = radar_core::webull::rest::Credentials {
        app_key: key,
        app_secret: secret,
        access_token: None,
    };
    let mut client = radar_core::webull::transport::WebullClient::new(creds)
        .with_base_url(base_url)
        .with_signature_scheme(s.webull_signature_scheme);
    match radar_core::webull::auth::create_token(&mut client) {
        Ok(c) => CreateTokenResult {
            ok: true,
            status: c.status.as_str().into(),
            token: c.token,
            expires_ms: c.expires_ms,
            detail: "สร้างแล้ว (PENDING) — SMS ส่งไปเบอร์ที่ผูกบัญชีแล้ว \
                     ยืนยันในแอป Webull ภายใน 5 นาที แล้วกด ทดสอบการเชื่อมต่อ"
                .into(),
        },
        Err(e) => CreateTokenResult {
            ok: false,
            status: "NONE".into(),
            token: None,
            expires_ms: None,
            detail: format!("{e}"),
        },
    }
}

/// Option Lab — วิเคราะห์สัญญาเดียวครบทุกตัวเลข (PLAN-LAB P9)
/// ค่า input มาจาก engine snapshot ที่ frontend ได้รับอยู่แล้ว —
/// การคำนวณทั้งหมดอยู่ใน Rust ตามข้อกำหนดกลางข้อ 1
#[derive(serde::Deserialize)]
struct LabAnalyzeInput {
    symbol: String,
    /// spot ต้อง > 0 — invalid = คืน error ภาษาไทย ไม่ fallback
    spot: f64,
    strike: f64,
    /// premium ต่อหุ้น (mid/credit)
    premium: f64,
    /// implied volatility ของสัญญา (0..1)
    iv: f64,
    /// วันถึงหมดอายุ (ปี — fractional ตาม engine)
    t: f64,
    is_call: bool,
    /// true = วิเคราะห์แบบ covered call รวมหุ้นที่ถือ (ต้องมี basis)
    covered: bool,
    /// cost basis ต่อหุ้น (แสดงเฉพาะ covered)
    basis: Option<f64>,
}

#[tauri::command]
fn lab_analyze(input: LabAnalyzeInput) -> Result<serde_json::Value, String> {
    use radar_core::lab::payoff::{self, CostInputs};
    use radar_core::lab::probability;
    let r = radar_core::quant::RISK_FREE_RATE;
    let q = radar_core::quant::DIV_YIELD;
    if input.spot <= 0.0 || !input.spot.is_finite() {
        return Err(format!(
            "spot ไม่ valid ({}) — ห้ามวิเคราะห์โดยไม่มีราคาหุ้นจริง",
            input.spot
        ));
    }
    if input.strike <= 0.0 || input.premium < 0.0 || input.t <= 0.0 || input.iv <= 0.0 {
        return Err("strike/premium/DTE/IV ต้องเป็นค่าบวกที่ใช้ได้".into());
    }
    let costs = CostInputs::default();
    let fees_rt = costs.per_side() * 2.0; // เข้า+ออก (ปิดก่อนหมดอายุ)
    let fees_open = costs.per_side();
    let leg = payoff::ShortLeg {
        is_call: input.is_call,
        k: input.strike,
        c: input.premium,
        m: 100.0,
    };
    let leg_open = payoff::analyze_leg(leg, fees_open);
    let leg_rt = payoff::analyze_leg(leg, fees_rt);
    let be = leg_rt.breakeven;
    let pop = if input.is_call {
        be.and_then(|be| probability::short_call_pop(input.spot, be, input.t, r, q, input.iv))
    } else {
        be.and_then(|be| probability::csp_pop(input.spot, be, input.t, r, q, input.iv))
    };
    let itm = probability::p_itm(
        input.spot,
        input.strike,
        input.is_call,
        input.t,
        r,
        q,
        input.iv,
    );
    let touch = itm.map(probability::p_touch);
    let delta = probability::leg_delta(
        input.spot,
        input.strike,
        input.is_call,
        input.t,
        r,
        q,
        input.iv,
    );
    let (cc, cc_analysis) = if input.covered {
        input.basis.filter(|b| *b > 0.0).map(|b| {
            let cc = payoff::CoveredCall {
                leg,
                b,
                s0: input.spot,
            };
            let a = payoff::analyze_cc(cc, fees_rt); // F2: ชุด fee เดียวกับ scenarios
            (Some(cc), Some(a))
        })
    } else {
        None
    }
    .unwrap_or((None, None));
    let scenarios = payoff::scenario_table(input.spot, input.strike, |s| {
        payoff::leg_pnl_at(leg, s, fees_rt)
    });
    let cc_scenarios = cc.map(|cc| {
        payoff::scenario_table(input.spot, input.strike, |s| {
            payoff::cc_pnl_at(cc, s, fees_rt)
        })
    });
    Ok(serde_json::json!({
        "symbol": input.symbol,
        "fees_round_trip": fees_rt,
        "fees_note": "ต่อสัญญา เข้า+ออก (commission+clearing+ORF+VAT · ห้ามรวม slippage)",
        "leg": { "open": leg_open, "round_trip": leg_rt },
        "pop": pop,
        "p_itm": itm,
        "p_touch": touch,
        "delta": delta,
        "covered_call": cc_analysis,
        "call_capital_today": if input.is_call { Some(100.0 * input.spot) } else { None },
        "scenarios": scenarios,
        "cc_scenarios": cc_scenarios,
    }))
}

/// A REST client built from the saved settings — the shared in-memory state,
/// not a re-read of the settings file and keyring on every call.
fn data_client(state: &AppState) -> Result<radar_core::webull::transport::WebullClient, String> {
    state
        .settings
        .lock()
        .unwrap()
        .webull_client()
        .ok_or_else(|| "ยังไม่ได้ตั้ง credentials — เปิดหน้า Setup ก่อน".to_string())
}

/// APP v2 — ประวัติราคาหุ้น (PLAN-APP2 P15): bars + MA + summary จาก Rust
#[tauri::command]
fn get_stock_history(
    state: tauri::State<'_, AppState>,
    symbol: String,
    range: String,
) -> Result<serde_json::Value, String> {
    use radar_core::market_history::{moving_average, range_summary};
    use radar_core::webull::stocks::{fetch_stock_bars, timespan_valid};
    let mut client = data_client(&state)?;
    let (timespan, count) = match range.as_str() {
        "1M" => ("D", 30u32),
        "3M" => ("D", 90),
        "6M" => ("D", 180),
        "1Y" => ("D", 365),
        "5Y" => ("D", 1200),
        _ => ("D", 180),
    };
    assert!(timespan_valid(timespan));
    let bars = fetch_stock_bars(&mut client, &[symbol], timespan, count)
        .map_err(|e| format!("ดึงข้อมูลราคาไม่สำเร็จ: {e}"))?;
    let ma5 = moving_average(&bars, 5);
    let ma20 = moving_average(&bars, 20);
    let summary = range_summary(&bars);
    Ok(serde_json::json!({
        "bars": bars, "ma5": ma5, "ma20": ma20,
        "summary": summary, "range": range,
        "note": "ราคาปิดรายวัน · ฐานปรับราคาตามที่ API ส่ง (ยังไม่ยืนยัน adjusted/crude)",
    }))
}

/// APP v2 — ข้อมูลหลักทรัพย์ (เฉพาะ fields ที่ API ให้จริง ไม่มี fundamentals)
#[tauri::command]
fn get_instrument_profile(
    state: tauri::State<'_, AppState>,
    symbol: String,
) -> Result<serde_json::Value, String> {
    use radar_core::webull::stocks::fetch_instrument_profiles;
    let mut client = data_client(&state)?;
    let profiles = fetch_instrument_profiles(&mut client, &[symbol])
        .map_err(|e| format!("ดึงโปรไฟล์ไม่สำเร็จ: {e}"))?;
    serde_json::to_value(profiles.into_iter().next()).map_err(|e| e.to_string())
}

/// Home overview (P17.1) — quotes ของ active watchlist + สถานะ Lab capture
#[tauri::command]
fn get_home_overview(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    use radar_core::webull::quotes::fetch_stock_snapshot;
    let mut client = state.settings.lock().unwrap().webull_client();
    let store = load_watchlists(&state.app_data_dir);
    let symbols: Vec<String> = store
        .active_list()
        .map(|l| l.symbols.iter().take(20).cloned().collect())
        .unwrap_or_default();
    let quotes: Vec<_> = symbols
        .iter()
        .map(|symbol| {
            let Some(client) = client.as_mut() else {
                return serde_json::json!({"symbol": symbol, "status": "setup_required"});
            };
            let snapshot = match fetch_stock_snapshot(client, symbol) {
                Ok(Some(row)) => row,
                Ok(None) => return serde_json::json!({"symbol": symbol, "status": "no_data"}),
                Err(_) => return serde_json::json!({"symbol": symbol, "status": "error"}),
            };
            let number = |key| {
                snapshot.get(key).and_then(|value| {
                    value
                        .as_f64()
                        .or_else(|| value.as_str()?.parse().ok())
                        .filter(|n| n.is_finite())
                })
            };
            let Some(price) = number("price").filter(|price| *price > 0.0) else {
                return serde_json::json!({"symbol": symbol, "status": "no_data"});
            };
            let source_timestamp = snapshot
                .get("last_trade_time")
                .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
                .filter(|timestamp| *timestamp > 0)
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|timestamp| timestamp.to_rfc3339());
            serde_json::json!({
                "symbol": symbol,
                "status": "available",
                "price": price,
                "change_pct": number("change_ratio").map(|ratio| ratio * 100.0),
                "volume": number("volume").filter(|volume| *volume >= 0.0),
                "source": "REST",
                "market_session": "regular",
                "source_timestamp": source_timestamp,
                "received_at": chrono::Utc::now().to_rfc3339(),
            })
        })
        .collect();
    // สถานะ Lab capture (อ่านจาก storage จริง ไม่เดา)
    let lab = radar_core::lab::storage::LabStorage::open(
        std::path::Path::new(&state.app_data_dir).join("lab"),
    );
    let (capture_days, last_capture) = match &lab {
        Ok(st) => {
            let dates = st.captured_dates().unwrap_or_default();
            let last = dates.last().map(|d| d.to_string());
            (dates.len(), last)
        }
        Err(_) => (0, None),
    };
    Ok(serde_json::json!({
        "watchlist_name": store.active_list().map(|l| l.name.clone()),
        "quotes": quotes,
        "lab": {"capture_days": capture_days, "last_capture": last_capture},
        "fetched_at": chrono::Utc::now().to_rfc3339(),
    }))
}

/// Watchlists (P16) — persistence ที่ app-data/watchlists.json
fn watchlist_store_path(app_data_dir: &str) -> std::path::PathBuf {
    std::path::Path::new(app_data_dir).join("watchlists.json")
}

fn load_watchlists(app_data_dir: &str) -> radar_core::watchlists::WatchlistStore {
    let path = watchlist_store_path(app_data_dir);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_watchlists(
    app_data_dir: &str,
    store: &radar_core::watchlists::WatchlistStore,
) -> Result<(), String> {
    let path = watchlist_store_path(app_data_dir);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| format!("บันทึก watchlist ไม่สำเร็จ: {e}"))
}

#[tauri::command]
fn list_watchlists(state: tauri::State<'_, AppState>) -> radar_core::watchlists::WatchlistStore {
    load_watchlists(&state.app_data_dir)
}

#[tauri::command]
fn save_watchlist(
    state: tauri::State<'_, AppState>,
    list_id: Option<String>,
    action: String,
    payload: String,
) -> Result<radar_core::watchlists::WatchlistStore, String> {
    use radar_core::watchlists as wl;
    let mut store = load_watchlists(&state.app_data_dir);
    // ลิสต์เป็น default เมื่อยังไม่มีไฟล์ + ไม่เคยสร้าง
    if store.lists.is_empty() {
        store = wl::WatchlistStore::with_default("NVDA");
    }
    let lid = list_id
        .or_else(|| store.active.clone())
        .ok_or("ไม่พบลิสต์ปัจจุบัน")?;
    let list = store
        .lists
        .iter_mut()
        .find(|l| l.id == lid)
        .ok_or_else(|| "ไม่พบลิสต์นี้".to_string())?;
    match action.as_str() {
        "add" => wl::add_symbol(list, &payload).map_err(|e| e.reason_th())?,
        "remove" => wl::remove_symbol(list, &payload),
        "up" => wl::move_symbol(list, &payload, true),
        "down" => wl::move_symbol(list, &payload, false),
        "newlist" => {
            let id = store.create_list(&payload).map_err(|e| e.reason_th())?;
            let _ = store.set_active(&id);
            save_watchlists(&state.app_data_dir, &store)?;
            return Ok(store);
        }
        "deletelist" => {
            store.delete_list(&lid);
            save_watchlists(&state.app_data_dir, &store)?;
            return Ok(store);
        }
        "setactive" => store.set_active(&payload).map_err(|e| e.reason_th())?,
        other => return Err(format!("ไม่รู้จัก action: {other}")),
    }
    save_watchlists(&state.app_data_dir, &store)?;
    Ok(store)
}

/// APP v2 — พอร์ต read-only (P18): accounts → positions + balance
#[tauri::command]
fn get_portfolio(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    use radar_core::webull::trading::{fetch_accounts, fetch_balance, fetch_positions};
    let mut client = data_client(&state)?;
    let accounts = fetch_accounts(&mut client).map_err(|e| format!("ดึงบัญชีไม่สำเร็จ: {e}"))?;
    // v1: บัญชีแรก (docs บอก account_type CASH เดี่ยว) — mask ไม่เผยเต็ม
    let Some(acc) = accounts.first() else {
        return Ok(serde_json::json!({
            "accounts": [], "positions": [], "balance": null,
            "note": "ไม่พบบัญชีในสิทธิ์นี้"
        }));
    };
    let positions = fetch_positions(&mut client, &acc.account_id)
        .map_err(|e| format!("ดึงสถานะถือครองไม่สำเร็จ: {e}"))?;
    let balance = fetch_balance(&mut client, &acc.account_id)
        .map_err(|e| {
            // balance ล้มไม่ทำทั้งหน้าพัง — แสดง positions พร้อมบอกว่ายอดเงินดึงไม่ได้
            let _ = e;
        })
        .ok();
    let head: String = acc.account_number.chars().take(4).collect();
    let tail: String = acc
        .account_number
        .chars()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let masked = format!("{head}…{tail}");
    Ok(serde_json::json!({
        "account_label": acc.account_label,
        "account_masked": masked,
        "as_of": chrono::Utc::now().to_rfc3339(),
        "positions": positions,
        "balance": balance,
    }))
}

/// Option Lab — บันทึกคำทำนาย (freeze) ห้ามแก้ย้อนหลัง (P9.4)
#[derive(serde::Deserialize)]
struct LabFreezeInput {
    occ: String,
    p: f64,
    event_th: String,
    model_note: String,
    spot: f64,
    strike: f64,
    premium: f64,
    iv: f64,
    dte_days: f64,
    expiry: String,
}

#[tauri::command]
fn lab_freeze_prediction(
    state: tauri::State<'_, AppState>,
    input: LabFreezeInput,
) -> Result<String, String> {
    use radar_core::lab::types::StoredPrediction;
    if !(0.0..=1.0).contains(&input.p) {
        return Err("ความน่าจะเป็นต้องอยู่ช่วง 0–1".into());
    }
    let expiry: chrono::NaiveDate = input
        .expiry
        .parse()
        .map_err(|e| format!("รูปแบบวันหมดอายุผิด: {e}"))?;
    let now = chrono::Utc::now();
    let occ = input.occ.clone();
    let pred = StoredPrediction {
        id: format!("pred-{}", now.timestamp_nanos_opt().unwrap_or_default()),
        occ: input.occ.clone(),
        p: input.p,
        event_th: input.event_th,
        model_note: input.model_note,
        spot: input.spot,
        strike: input.strike,
        premium: input.premium,
        iv: input.iv,
        dte_days: input.dte_days,
        expiry,
        created_at: now,
    };
    let root = std::path::Path::new(&state.app_data_dir).join("lab");
    let store = radar_core::lab::storage::LabStorage::open(&root)
        .map_err(|e| format!("เปิดที่เก็บ lab ไม่ได้: {e}"))?;
    // F7: สัญญาเดียววันเดียว = คำทำนายเดียว (double-click ไม่ซ้ำ)
    let today = now.date_naive();
    if let Some(dup) = store
        .read_predictions()
        .unwrap_or_default()
        .iter()
        .find(|p| p.occ == occ && p.created_at.date_naive() == today)
    {
        return Err(format!(
            "บันทึกสัญญานี้วันนี้แล้ว ({}) — หนึ่งสัญญาต่อวันต่อหนึ่งคำทำนาย",
            dup.id
        ));
    }
    store
        .append_prediction(&pred)
        .map_err(|e| format!("บันทึกไม่สำเร็จ: {e}"))?;
    Ok(pred.id)
}

/// Option Lab — คำทำนายทั้งหมดที่ freeze ไว้ (อ่านอย่างเดียว)
#[tauri::command]
fn lab_predictions(state: tauri::State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let root = std::path::Path::new(&state.app_data_dir).join("lab");
    let store = radar_core::lab::storage::LabStorage::open(&root)
        .map_err(|e| format!("เปิดที่เก็บ lab ไม่ได้: {e}"))?;
    Ok(store
        .read_predictions()
        .unwrap_or_default()
        .iter()
        .map(|p| serde_json::to_value(p).unwrap_or_default())
        .collect())
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
    let stored = s
        .llm
        .clone()
        .unwrap_or_else(|| radar_core::copilot::LlmConfig::new("", "", "", ""));
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
            streaming: None,
            signature_scheme: None,
        };
    }
    match radar_core::copilot::ping(&cfg) {
        Ok(reply) => ConnectionTestResult {
            webull_ok: true,
            detail: format!(
                "AI ตอบกลับจริง ({}): {}",
                cfg.model,
                reply.chars().take(80).collect::<String>()
            ),
            streaming: None,
            signature_scheme: None,
        },
        Err(e) => ConnectionTestResult {
            webull_ok: false,
            detail: e,
            streaming: None,
            signature_scheme: None,
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
    // cross-platform app-data dir (macOS build needs HOME, Windows uses APPDATA;
    // keep the same "premium-radar" folder name so a future port keeps one layout)
    let app_data_dir = if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .map(|h| format!("{h}/Library/Application Support/premium-radar"))
            .unwrap_or_else(|_| ".".into())
    } else {
        std::env::var("APPDATA")
            .map(|a| format!("{a}/premium-radar"))
            .unwrap_or_else(|_| ".".into())
    };
    let _ = std::fs::create_dir_all(&app_data_dir);
    let mut app_settings = settings::AppSettings::load(&app_data_dir);
    // One-time migration: plaintext secrets in the file move to the OS
    // keyring and the file is re-saved scrubbed. The save fails closed, so
    // when no keyring backend is available the file is left exactly as it
    // was and remains the source of truth. Either way the in-memory state
    // is refilled to the EFFECTIVE settings below.
    if app_settings.has_plaintext_secrets()
        && let Err(e) = app_settings.save_with_keyring(&app_data_dir)
    {
        eprintln!("[settings] keyring migration ไม่สำเร็จ คงเก็บในไฟล์แทน: {e}");
    }
    app_settings.pull_secrets_from_keyring();

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
            create_access_token,
            needs_setup,
            get_stock_history,
            get_instrument_profile,
            get_home_overview,
            list_watchlists,
            save_watchlist,
            get_portfolio,
            lab_analyze,
            lab_freeze_prediction,
            lab_predictions,
            ask_copilot,
            test_llm,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
