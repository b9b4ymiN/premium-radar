// PREMIUM RADAR — Tauri bridge: commands + the REST scan worker wiring the
// radar-core engine to the React frontend. See PLAN.md P4.2-P4.4.

use std::sync::Mutex;

use chrono::Utc;
pub mod scan;
pub mod settings;

use radar_core::engine::{Engine, EngineSnapshot};
use radar_core::screener::{Filters, Mode};
use radar_core::webull::sign::SignatureScheme;
use radar_core::webull::transport::WebullClient;
use scan::ScanControl;
use settings::{AppSettings, SettingsView};

/// Shared app state managed by Tauri. The engine lives behind an Arc so the
/// scan worker thread can push quotes into the same store the commands read.
pub struct AppState {
    pub engine: std::sync::Arc<Mutex<Engine>>,
    pub settings: Mutex<AppSettings>,
    pub scan: ScanControl,
    pub app_data_dir: String,
    /// Access token just issued by [`create_access_token`] — the value stays
    /// native; `save_credentials` picks it up (typed > pending > saved) and
    /// clears it once the save commits.
    pub pending_token: Mutex<Option<String>>,
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

    // Drop everything that belongs to the previously scanned underlying so a
    // new scan never shows the old symbol's stock/option prices; the lock is
    // released before the scan thread (and any network call) starts.
    state.engine.lock().unwrap().begin_symbol(&symbol);

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
fn get_settings(state: tauri::State<'_, AppState>) -> SettingsView {
    settings_of(&state).view()
}

#[tauri::command]
fn save_settings(
    state: tauri::State<'_, AppState>,
    new_settings: AppSettings,
) -> Result<(), String> {
    let dir = state.app_data_dir.clone();
    // never take secrets from JS: every secret field on the incoming struct
    // is overwritten with the current in-memory (keyring) values — a prefs
    // push can neither inject nor move credentials
    let mut new_settings = {
        let current = state
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        settings::copy_saved_secrets(&current, new_settings)
    };
    new_settings.save_with_keyring(&dir)?;
    *state.settings.lock().unwrap() = new_settings;
    Ok(())
}

/// Save API credentials from the setup screen — secrets go to the OS keyring
/// (Windows Credential Manager / macOS Keychain). The candidate is staged
/// under the settings mutex and swapped into the shared state only after the
/// save fully succeeds: a keyring failure aborts with an error, leaving both
/// the running credentials and the previous settings file untouched (nothing
/// is ever written to disk as plaintext). An EMPTY secret/token/LLM key keeps
/// the saved value (the setup screen leaves the field blank when one is
/// stored); the token resolves typed > pending (just issued) > saved. The
/// returned view carries no secret values.
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
) -> Result<SettingsView, String> {
    const URL_ERROR: &str = "Base URL ต้องเป็น https://api.webull.co.th เท่านั้น";
    let typed_url = webull_base_url
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty());
    let base_url = match typed_url.as_deref() {
        Some(typed) => settings::normalise_webull_base_url(typed).ok_or(URL_ERROR)?,
        None => settings::WEBULL_PRODUCTION_BASE_URL.to_string(),
    };
    let pending = state
        .pending_token
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();

    let dir = state.app_data_dir.clone();
    let mut s = state
        .settings
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // stage on a copy — the shared state must keep the previously saved
    // (and still working) credentials until this save is fully committed
    let mut candidate = s.clone();
    candidate.webull_app_key = webull_key.trim().to_string();
    candidate.webull_app_secret = settings::merge_secret(&webull_secret, &s.webull_app_secret);
    candidate.webull_access_token = settings::resolve_access_token(
        webull_access_token.as_deref().unwrap_or(""),
        pending.as_deref(),
        &s.webull_access_token,
    );
    if let Some(scheme) = webull_signature_scheme {
        candidate.webull_signature_scheme = scheme;
    }
    candidate.webull_base_url = base_url;
    candidate.llm = settings::merge_llm(
        s.llm.as_ref(),
        llm_label,
        llm_base_url,
        llm_model,
        llm_api_key,
    );

    candidate.save_with_keyring(&dir)?;
    *s = candidate;
    // answers fetched with the previous credentials must never be served again
    clear_bridge_caches(&s.webull_base_url_or_default());
    *state
        .pending_token
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    Ok(s.view())
}

/// Credential-scoped caches: after a successful save, every layer that may
/// hold broker answers from the previous keys is dropped — the Home overview
/// and movers TTL caches here, plus the transport's shared response cache
/// for the origin.
fn clear_bridge_caches(base_url: &str) {
    if let Ok(mut cache) = OVERVIEW_CACHE.lock() {
        cache.clear();
    }
    if let Ok(mut cache) = MOVERS_CACHE.lock() {
        cache.clear();
    }
    radar_core::webull::transport::clear_shared_cache(base_url);
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
/// surface before the user starts a full scan. A typed base URL that is not
/// the allowed production host is rejected up front with the same message
/// `save_credentials` uses; the probe clients run with caching disabled so a
/// connection test can never be answered from a cached copy.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn test_connection(
    state: tauri::State<'_, AppState>,
    webull_key: Option<String>,
    webull_secret: Option<String>,
    webull_access_token: Option<String>,
    webull_base_url: Option<String>,
) -> Result<ConnectionTestResult, String> {
    let settings = settings_of(&state);
    let pending = state
        .pending_token
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    // several paced calls plus an MQTT probe — off the UI thread
    let result = off_ui_thread(move || {
        Ok(test_connection_with(
            settings,
            pending,
            webull_key,
            webull_secret,
            webull_access_token,
            webull_base_url,
        ))
    })
    .await
    .unwrap_or_else(|e| ConnectionTestResult {
        webull_ok: false,
        detail: e,
        streaming: None,
        signature_scheme: None,
    });
    Ok(result)
}

/// The probe itself; `s` is what typed values fall back to.
#[allow(clippy::too_many_arguments)]
fn test_connection_with(
    s: AppSettings,
    pending_token: Option<String>,
    webull_key: Option<String>,
    webull_secret: Option<String>,
    webull_access_token: Option<String>,
    webull_base_url: Option<String>,
) -> ConnectionTestResult {
    let scheme = s.webull_signature_scheme;
    let key = webull_key
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .unwrap_or(s.webull_app_key.trim().to_string());
    let secret = webull_secret
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or(s.webull_app_secret.trim().to_string());
    let resolved_token = settings::resolve_access_token(
        webull_access_token.as_deref().unwrap_or(""),
        pending_token.as_deref(),
        &s.webull_access_token,
    );
    let token = (!resolved_token.is_empty()).then_some(resolved_token);
    let base_url = match webull_base_url
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        Some(typed) => match settings::normalise_webull_base_url(&typed) {
            Some(url) => url,
            None => {
                return ConnectionTestResult {
                    webull_ok: false,
                    detail: "Base URL ต้องเป็น https://api.webull.co.th เท่านั้น".into(),
                    streaming: None,
                    signature_scheme: None,
                };
            }
        },
        None => s.webull_base_url_or_default(),
    };

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
        // a connection test observes the wire, never a cached answer
        .without_cache()
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
         แล้วกด บันทึก ตามด้วย ทดสอบการเชื่อมต่อ"
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

/// Result of the in-app token creation flow. The token VALUE never crosses
/// the IPC boundary — `token_issued` says one is waiting (PENDING) in
/// `AppState::pending_token` for the next save/probe to pick up.
#[derive(serde::Serialize)]
pub struct CreateTokenResult {
    pub ok: bool,
    pub status: String,
    pub token_issued: bool,
    pub expires_ms: Option<u64>,
    pub detail: String,
}

/// Start access-token creation (production needs one for data endpoints):
/// POSTs `/openapi/auth/token/create` — the token VALUE arrives immediately
/// as PENDING and the broker SMSes a code to the account's bound phone. The
/// value stays native (`AppState::pending_token`); the user confirms in the
/// Webull app within 5 minutes and `test_connection` then reports NORMAL.
#[tauri::command]
async fn create_access_token(
    state: tauri::State<'_, AppState>,
    webull_key: Option<String>,
    webull_secret: Option<String>,
    webull_base_url: Option<String>,
) -> Result<CreateTokenResult, String> {
    let settings = settings_of(&state);
    let (result, issued_token) = off_ui_thread(move || {
        Ok(create_access_token_with(
            settings,
            webull_key,
            webull_secret,
            webull_base_url,
        ))
    })
    .await
    .unwrap_or_else(|e| {
        (
            CreateTokenResult {
                ok: false,
                status: "NONE".into(),
                token_issued: false,
                expires_ms: None,
                detail: e,
            },
            None,
        )
    });
    if let Some(token) = issued_token {
        *state
            .pending_token
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(token);
    }
    Ok(result)
}

/// The request itself; `s` is what typed values fall back to. Returns the
/// result plus the raw issued token for the caller to keep native — it never
/// reaches the IPC result.
fn create_access_token_with(
    s: AppSettings,
    webull_key: Option<String>,
    webull_secret: Option<String>,
    webull_base_url: Option<String>,
) -> (CreateTokenResult, Option<String>) {
    let key = webull_key
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| s.webull_app_key.trim().to_string());
    let secret = webull_secret
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| s.webull_app_secret.trim().to_string());
    let base_url = match webull_base_url
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        Some(typed) => match settings::normalise_webull_base_url(&typed) {
            Some(url) => url,
            None => {
                return (
                    CreateTokenResult {
                        ok: false,
                        status: "NONE".into(),
                        token_issued: false,
                        expires_ms: None,
                        detail: "Base URL ต้องเป็น https://api.webull.co.th เท่านั้น".into(),
                    },
                    None,
                );
            }
        },
        None => s.webull_base_url_or_default(),
    };
    if key.is_empty() || secret.is_empty() {
        return (
            CreateTokenResult {
                ok: false,
                status: "NONE".into(),
                token_issued: false,
                expires_ms: None,
                detail: "ยังไม่ได้ใส่ App Key / Secret".into(),
            },
            None,
        );
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
        Ok(c) => {
            let token_issued = c.token.is_some();
            (
                CreateTokenResult {
                    ok: true,
                    status: c.status.as_str().into(),
                    token_issued,
                    expires_ms: c.expires_ms,
                    detail: "สร้างแล้ว (PENDING) — SMS ส่งไปเบอร์ที่ผูกบัญชีแล้ว \
                             ยืนยันในแอป Webull ภายใน 5 นาที แล้วกด บันทึก ตามด้วย ทดสอบการเชื่อมต่อ"
                        .into(),
                },
                c.token,
            )
        }
        Err(e) => (
            CreateTokenResult {
                ok: false,
                status: "NONE".into(),
                token_issued: false,
                expires_ms: None,
                detail: format!("{e}"),
            },
            None,
        ),
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

/// The saved settings. The lock guards plain data, so a poisoned lock (another
/// thread panicked while holding it) still hands out a usable copy instead of
/// taking every later command down with it.
fn settings_of(state: &AppState) -> AppSettings {
    state
        .settings
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// Run blocking REST work off the UI thread. A plain `fn` command runs on the
/// main thread in Tauri, so every paced call (≥ 1.1 s each) would freeze the
/// window.
async fn off_ui_thread<T, F>(work: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| format!("background task failed: {e}"))?
}

/// Blocking REST work with a client built from `settings`, off the UI thread.
/// The client wraps a blocking HTTP stack that may not be built, used or
/// dropped on an async thread — a debug build panics outright ("Cannot drop a
/// runtime in a context where blocking is not allowed") — so all three happen
/// inside this closure. A command never builds the client itself.
async fn with_client_from<T, F>(settings: AppSettings, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&mut WebullClient) -> Result<T, String> + Send + 'static,
{
    off_ui_thread(move || {
        let mut client = settings
            .webull_client()
            .ok_or_else(|| "ยังไม่ได้ตั้ง credentials — เปิดหน้า Setup ก่อน".to_string())?;
        work(&mut client)
    })
    .await
}

/// `with_client_from` with the saved settings.
async fn with_client<T, F>(state: &AppState, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&mut WebullClient) -> Result<T, String> + Send + 'static,
{
    with_client_from(settings_of(state), work).await
}

/// Per-key TTL cache for answers that cost several paced calls or change by
/// the minute. Values keep their original `fetched_at`, so a hit is honest
/// about its age.
struct TtlCache<T> {
    ttl: std::time::Duration,
    entries: std::collections::HashMap<String, (std::time::Instant, T)>,
}

impl<T: Clone> TtlCache<T> {
    fn new(ttl: std::time::Duration) -> Self {
        Self {
            ttl,
            entries: std::collections::HashMap::new(),
        }
    }

    fn get(&self, key: &str) -> Option<T> {
        self.entries
            .get(key)
            .filter(|(at, _)| at.elapsed() < self.ttl)
            .map(|(_, value)| value.clone())
    }

    fn put(&mut self, key: &str, value: T) {
        if self.entries.len() >= 32 {
            self.entries.retain(|_, (at, _)| at.elapsed() < self.ttl);
        }
        self.entries
            .insert(key.to_string(), (std::time::Instant::now(), value));
    }

    /// Drop everything (credential change — a hit must never outlive the keys
    /// that fetched it).
    fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Company facts change slowly; screener ranks move by the minute.
static OVERVIEW_CACHE: std::sync::LazyLock<std::sync::Mutex<TtlCache<serde_json::Value>>> =
    std::sync::LazyLock::new(|| {
        std::sync::Mutex::new(TtlCache::new(std::time::Duration::from_secs(600)))
    });
static MOVERS_CACHE: std::sync::LazyLock<std::sync::Mutex<TtlCache<serde_json::Value>>> =
    std::sync::LazyLock::new(|| {
        std::sync::Mutex::new(TtlCache::new(std::time::Duration::from_secs(60)))
    });

/// APP v2 — ประวัติราคาหุ้น (PLAN-APP2 P15): bars + MA + summary จาก Rust
#[tauri::command]
async fn get_stock_history(
    state: tauri::State<'_, AppState>,
    symbol: String,
    range: String,
) -> Result<serde_json::Value, String> {
    use radar_core::market_history::{moving_average, range_summary};
    use radar_core::webull::stocks::{fetch_stock_bars, timespan_valid};
    with_client(&state, move |client| {
        let (timespan, count) = match range.as_str() {
            "1M" => ("D", 30u32),
            "3M" => ("D", 90),
            "6M" => ("D", 180),
            "1Y" => ("D", 365),
            "5Y" => ("D", 1200),
            _ => ("D", 180),
        };
        assert!(timespan_valid(timespan));
        let bars = fetch_stock_bars(client, &[symbol], timespan, count)
            .map_err(|e| format!("ดึงข้อมูลราคาไม่สำเร็จ: {}", e.user_message()))?;
        let ma5 = moving_average(&bars, 5);
        let ma20 = moving_average(&bars, 20);
        let summary = range_summary(&bars);
        Ok(serde_json::json!({
            "bars": bars, "ma5": ma5, "ma20": ma20,
            "summary": summary, "range": range,
            "note": "ราคาปิดรายวัน · ฐานปรับราคาตามที่ API ส่ง (ยังไม่ยืนยัน adjusted/crude)",
        }))
    })
    .await
}

/// APP v2 — ข้อมูลหลักทรัพย์ (เฉพาะ fields ที่ API ให้จริง)
#[tauri::command]
async fn get_instrument_profile(
    state: tauri::State<'_, AppState>,
    symbol: String,
) -> Result<serde_json::Value, String> {
    use radar_core::webull::stocks::fetch_instrument_profiles;
    with_client(&state, move |client| {
        let profiles = fetch_instrument_profiles(client, &[symbol])
            .map_err(|e| format!("ดึงโปรไฟล์ไม่สำเร็จ: {}", e.user_message()))?;
        serde_json::to_value(profiles.into_iter().next()).map_err(|e| e.to_string())
    })
    .await
}

/// Epoch milliseconds → RFC 3339; zero and missing stay `None`.
fn epoch_ms(ms: Option<i64>) -> Option<String> {
    ms.filter(|ms| *ms > 0)
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|time| time.to_rfc3339())
}

/// One watchlist quote row. Only an available row carries numbers; any other
/// row says why it has none.
fn quote_row(
    symbol: &str,
    snapshot: Option<&radar_core::webull::stocks::StockSnapshot>,
    reason: Option<&str>,
    received_at: &str,
) -> serde_json::Value {
    match snapshot {
        Some(snapshot) if snapshot.price.unwrap_or(0.0) > 0.0 => serde_json::json!({
            "symbol": symbol,
            "status": "available",
            "price": snapshot.price,
            "change_pct": snapshot.change_ratio.map(|ratio| ratio * 100.0),
            "volume": snapshot.volume,
            "source": "REST",
            // a regular-session quote: the list never asks for extended hours
            "market_session": "regular",
            "source_timestamp": epoch_ms(snapshot.last_trade_time_ms),
            "received_at": received_at,
        }),
        Some(_) => serde_json::json!({"symbol": symbol, "status": "no_data"}),
        None => serde_json::json!({"symbol": symbol, "status": "error", "reason": reason}),
    }
}

/// Every symbol of the watchlist in ONE batched request (it used to be one
/// paced call per symbol), each row stating its own availability. Returns the
/// rows and, when the whole call failed, the reason.
fn home_overview_on(
    mut client: Option<WebullClient>,
    symbols: &[String],
) -> (Vec<serde_json::Value>, Option<String>) {
    use radar_core::webull::stocks::fetch_stock_snapshots;
    if symbols.is_empty() {
        return (Vec::new(), None);
    }
    let Some(client) = client.as_mut() else {
        let rows = symbols
            .iter()
            .map(|symbol| serde_json::json!({"symbol": symbol, "status": "setup_required"}))
            .collect();
        return (rows, None);
    };
    let received_at = chrono::Utc::now().to_rfc3339();
    match fetch_stock_snapshots(client, symbols, false) {
        Ok(snapshots) => {
            // keep the watchlist order; a symbol the broker did not price is "no data"
            let rows = symbols
                .iter()
                .map(|symbol| {
                    let snapshot = snapshots.iter().find(|snapshot| snapshot.symbol == *symbol);
                    quote_row(symbol, snapshot, None, &received_at)
                })
                .collect();
            (rows, None)
        }
        Err(error) => {
            let reason = error.user_message();
            let rows = symbols
                .iter()
                .map(|symbol| {
                    serde_json::json!({"symbol": symbol, "status": "error", "reason": reason})
                })
                .collect();
            (rows, Some(reason))
        }
    }
}

/// Home overview (P17.1) — quotes ของ active watchlist + สถานะ Lab capture
#[tauri::command]
async fn get_home_overview(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let settings = settings_of(&state);
    let store = load_watchlists(&state.app_data_dir);
    let watchlist_name = store.active_list().map(|list| list.name.clone());
    let symbols: Vec<String> = store
        .active_list()
        .map(|list| list.symbols.iter().take(20).cloned().collect())
        .unwrap_or_default();
    // สถานะ Lab capture (อ่านจาก storage จริง ไม่เดา)
    let lab = radar_core::lab::storage::LabStorage::open(
        std::path::Path::new(&state.app_data_dir).join("lab"),
    );
    let (capture_days, last_capture) = match &lab {
        Ok(storage) => {
            let dates = storage.captured_dates().unwrap_or_default();
            let last = dates.last().map(|date| date.to_string());
            (dates.len(), last)
        }
        Err(_) => (0, None),
    };
    let (quotes, error) = off_ui_thread(move || {
        // the client is optional on purpose: without credentials every row says so
        Ok(home_overview_on(settings.webull_client(), &symbols))
    })
    .await?;
    Ok(serde_json::json!({
        "watchlist_name": watchlist_name,
        "quotes": quotes,
        "error": error,
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

/// Show only the first 4 and last 3 characters of an account number.
fn mask_account(number: &str) -> String {
    let head: String = number.chars().take(4).collect();
    let tail: String = number
        .chars()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}…{tail}")
}

/// APP v2 — พอร์ต read-only (P18): accounts → positions + balance + open orders
#[tauri::command]
async fn get_portfolio(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    use radar_core::webull::trading::{
        fetch_accounts, fetch_balance, fetch_open_orders, fetch_positions,
    };
    with_client(&state, move |client| {
        let accounts =
            fetch_accounts(client).map_err(|e| format!("ดึงบัญชีไม่สำเร็จ: {}", e.user_message()))?;
        // v1: บัญชีแรก (docs บอก account_type CASH เดี่ยว) — mask ไม่เผยเต็ม
        let Some(acc) = accounts.first() else {
            return Ok(serde_json::json!({
                "accounts": [], "positions": [], "balance": null, "open_orders": [],
                "note": "ไม่พบบัญชีในสิทธิ์นี้"
            }));
        };
        let positions = fetch_positions(client, &acc.account_id)
            .map_err(|e| format!("ดึงสถานะถือครองไม่สำเร็จ: {}", e.user_message()))?;
        // balance/orders ล้มไม่ทำทั้งหน้าพัง — แสดง positions พร้อมบอกว่าส่วนไหนดึงไม่ได้
        let (balance, balance_error) = match fetch_balance(client, &acc.account_id) {
            Ok(balance) => (Some(balance), None),
            Err(e) => (None, Some(e.user_message())),
        };
        let (open_orders, open_orders_error) = match fetch_open_orders(client, &acc.account_id) {
            Ok(orders) => (orders, None),
            Err(e) => (Vec::new(), Some(e.user_message())),
        };
        Ok(serde_json::json!({
            "account_label": acc.account_label,
            "account_masked": mask_account(&acc.account_number),
            "as_of": chrono::Utc::now().to_rfc3339(),
            "positions": positions,
            "balance": balance,
            "balance_error": balance_error,
            "open_orders": open_orders,
            "open_orders_error": open_orders_error,
        }))
    })
    .await
}

/// Quote-header context: the stock WITH its extended-hours block and the
/// broker's own stats. The scanner's stock price stays regular-session; this
/// call exists so a pre/after-hours move is visible next to it, labelled.
#[tauri::command]
async fn get_stock_quote(
    state: tauri::State<'_, AppState>,
    symbol: String,
) -> Result<serde_json::Value, String> {
    use radar_core::webull::stocks::fetch_stock_snapshots;
    let symbol = symbol.trim().to_uppercase();
    with_client(&state, move |client| {
        let fetched_at = chrono::Utc::now().to_rfc3339();
        let snapshots = fetch_stock_snapshots(client, std::slice::from_ref(&symbol), true)
            .map_err(|e| e.user_message())?;
        let quote = snapshots
            .into_iter()
            .next()
            .filter(|snapshot| snapshot.price.unwrap_or(0.0) > 0.0);
        Ok(serde_json::json!({
            "symbol": symbol, "quote": quote, "fetched_at": fetched_at, "source": "REST",
        }))
    })
    .await
}

/// What the company is, when it reports and goes ex-dividend, and where
/// analysts stand. Five paced calls, so an answer that carries something is
/// reused for ten minutes; `refresh` asks again.
#[tauri::command]
async fn get_stock_overview(
    state: tauri::State<'_, AppState>,
    symbol: String,
    refresh: Option<bool>,
) -> Result<serde_json::Value, String> {
    use radar_core::webull::fundamentals::fetch_stock_overview;
    let symbol = symbol.trim().to_uppercase();
    if !refresh.unwrap_or(false)
        && let Some(cached) = OVERVIEW_CACHE.lock().unwrap().get(&symbol)
    {
        return Ok(cached);
    }
    let cache_key = symbol.clone();
    let value = with_client(&state, move |client| {
        let overview = fetch_stock_overview(client, &symbol);
        Ok(serde_json::json!({
            "symbol": symbol, "overview": overview, "fetched_at": chrono::Utc::now().to_rfc3339(),
        }))
    })
    .await?;
    let landed =
        value["overview"]["profile"].is_object() || value["overview"]["next_earnings"].is_object();
    if landed {
        OVERVIEW_CACHE
            .lock()
            .unwrap()
            .put(&cache_key, value.clone());
    }
    Ok(value)
}

/// One screener list in the broker's ranking order, reused for a minute.
#[tauri::command]
async fn get_movers(
    state: tauri::State<'_, AppState>,
    list: radar_core::webull::screeners::MoverList,
    refresh: Option<bool>,
) -> Result<serde_json::Value, String> {
    use radar_core::webull::screeners::fetch_movers;
    let key = format!("movers:{list:?}");
    if !refresh.unwrap_or(false)
        && let Some(cached) = MOVERS_CACHE.lock().unwrap().get(&key)
    {
        return Ok(cached);
    }
    let value = with_client(&state, move |client| {
        let rows = fetch_movers(client, list).map_err(|e| e.user_message())?;
        Ok(serde_json::json!({
            "list": list, "rows": rows,
            "fetched_at": chrono::Utc::now().to_rfc3339(), "source": "REST",
        }))
    })
    .await?;
    MOVERS_CACHE.lock().unwrap().put(&key, value.clone());
    Ok(value)
}

/// Market sectors with breadth and leading stock, reused for a minute.
#[tauri::command]
async fn get_sectors(
    state: tauri::State<'_, AppState>,
    refresh: Option<bool>,
) -> Result<serde_json::Value, String> {
    use radar_core::webull::screeners::fetch_sectors;
    let key = "sectors";
    if !refresh.unwrap_or(false)
        && let Some(cached) = MOVERS_CACHE.lock().unwrap().get(key)
    {
        return Ok(cached);
    }
    let value = with_client(&state, move |client| {
        let rows = fetch_sectors(client).map_err(|e| e.user_message())?;
        Ok(serde_json::json!({
            "rows": rows, "fetched_at": chrono::Utc::now().to_rfc3339(), "source": "REST",
        }))
    })
    .await?;
    MOVERS_CACHE.lock().unwrap().put(key, value.clone());
    Ok(value)
}

/// The stocks of one sector, reused for a minute.
#[tauri::command]
async fn get_sector_detail(
    state: tauri::State<'_, AppState>,
    sector_id: String,
    refresh: Option<bool>,
) -> Result<serde_json::Value, String> {
    use radar_core::webull::screeners::fetch_sector_detail;
    let key = format!("sector:{sector_id}");
    if !refresh.unwrap_or(false)
        && let Some(cached) = MOVERS_CACHE.lock().unwrap().get(&key)
    {
        return Ok(cached);
    }
    let value = with_client(&state, move |client| {
        let rows = fetch_sector_detail(client, &sector_id).map_err(|e| e.user_message())?;
        Ok(serde_json::json!({
            "sector_id": sector_id, "rows": rows,
            "fetched_at": chrono::Utc::now().to_rfc3339(), "source": "REST",
        }))
    })
    .await?;
    MOVERS_CACHE.lock().unwrap().put(&key, value.clone());
    Ok(value)
}

/// The watchlists the broker holds — read only, reused for a minute. The
/// app's own lists are local; nothing here writes to the broker.
#[tauri::command]
async fn get_broker_watchlists(
    state: tauri::State<'_, AppState>,
    refresh: Option<bool>,
) -> Result<serde_json::Value, String> {
    use radar_core::webull::watchlists::fetch_watchlists;
    let key = "broker-watchlists";
    if !refresh.unwrap_or(false)
        && let Some(cached) = MOVERS_CACHE.lock().unwrap().get(key)
    {
        return Ok(cached);
    }
    let value = with_client(&state, move |client| {
        let lists = fetch_watchlists(client).map_err(|e| e.user_message())?;
        Ok(serde_json::json!({
            "lists": lists, "fetched_at": chrono::Utc::now().to_rfc3339(), "source": "REST",
        }))
    })
    .await?;
    MOVERS_CACHE.lock().unwrap().put(key, value.clone());
    Ok(value)
}

/// The symbols of one broker watchlist — read only.
#[tauri::command]
async fn get_broker_watchlist(
    state: tauri::State<'_, AppState>,
    watchlist_id: String,
    refresh: Option<bool>,
) -> Result<serde_json::Value, String> {
    use radar_core::webull::watchlists::fetch_watchlist_instruments;
    let key = format!("broker-watchlist:{watchlist_id}");
    if !refresh.unwrap_or(false)
        && let Some(cached) = MOVERS_CACHE.lock().unwrap().get(&key)
    {
        return Ok(cached);
    }
    let value = with_client(&state, move |client| {
        let instruments =
            fetch_watchlist_instruments(client, &watchlist_id).map_err(|e| e.user_message())?;
        Ok(serde_json::json!({
            "watchlist_id": watchlist_id, "instruments": instruments,
            "fetched_at": chrono::Utc::now().to_rfc3339(), "source": "REST",
        }))
    })
    .await?;
    MOVERS_CACHE.lock().unwrap().put(&key, value.clone());
    Ok(value)
}

/// What the broker itself quotes for one contract: sizes, open interest,
/// greeks, implied volatility and the quote's own timestamps.
#[tauri::command]
async fn get_option_quote(
    state: tauri::State<'_, AppState>,
    symbol: String,
) -> Result<serde_json::Value, String> {
    use radar_core::webull::options::fetch_option_quotes;
    let symbol = symbol.trim().replace(' ', "");
    with_client(&state, move |client| {
        let fetched_at = chrono::Utc::now().to_rfc3339();
        let quotes = fetch_option_quotes(client, std::slice::from_ref(&symbol))
            .map_err(|e| e.user_message())?;
        Ok(serde_json::json!({
            "symbol": symbol, "quote": quotes.into_iter().next(),
            "fetched_at": fetched_at, "source": "REST",
        }))
    })
    .await
}

/// The last trades of one contract, latest first.
#[tauri::command]
async fn get_option_ticks(
    state: tauri::State<'_, AppState>,
    symbol: String,
    count: Option<u32>,
) -> Result<serde_json::Value, String> {
    use radar_core::webull::options::fetch_option_ticks;
    let symbol = symbol.trim().replace(' ', "");
    let count = count.unwrap_or(15).min(50);
    with_client(&state, move |client| {
        let fetched_at = chrono::Utc::now().to_rfc3339();
        let ticks = fetch_option_ticks(client, &symbol, count).map_err(|e| e.user_message())?;
        Ok(serde_json::json!({
            "symbol": symbol, "ticks": ticks, "fetched_at": fetched_at, "source": "REST",
        }))
    })
    .await
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
async fn ask_copilot(
    state: tauri::State<'_, AppState>,
    messages: Vec<radar_core::copilot::ChatMessage>,
) -> Result<String, String> {
    let cfg = state
        .settings
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .llm_config()
        .ok_or("ยังไม่ได้ตั้งค่า AI — เปิดหน้า Setup (ไอคอน ⚙) เพื่อใส่ LLM")?;
    // the model call takes seconds to a minute — off the UI thread
    let engine = state.engine.clone();
    off_ui_thread(move || {
        let mut engine = engine.lock().unwrap();
        radar_core::copilot::chat_with_tools(&cfg, &messages, &mut engine)
    })
    .await
}

/// Probe the LLM endpoint with either the given values (setup screen,
/// unsaved) or the stored settings — one tiny chat completion.
#[tauri::command]
async fn test_llm(
    state: tauri::State<'_, AppState>,
    llm_label: Option<String>,
    llm_base_url: Option<String>,
    llm_model: Option<String>,
    llm_api_key: Option<String>,
) -> Result<ConnectionTestResult, String> {
    let settings = settings_of(&state);
    let result = off_ui_thread(move || {
        Ok(test_llm_with(
            settings,
            llm_label,
            llm_base_url,
            llm_model,
            llm_api_key,
        ))
    })
    .await
    .unwrap_or_else(|e| ConnectionTestResult {
        webull_ok: false,
        detail: e,
        streaming: None,
        signature_scheme: None,
    });
    Ok(result)
}

/// The probe itself; `s` is what typed values fall back to.
fn test_llm_with(
    s: AppSettings,
    llm_label: Option<String>,
    llm_base_url: Option<String>,
    llm_model: Option<String>,
    llm_api_key: Option<String>,
) -> ConnectionTestResult {
    let pick = |value: Option<String>, current: &str| {
        value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| current.trim().to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// TtlCache: fresh hits only, and an entry expires by its own age.
    #[test]
    fn ttl_cache_expires() {
        let mut cache = TtlCache::new(std::time::Duration::from_millis(40));
        cache.put("a", json!(1));
        assert_eq!(cache.get("a"), Some(json!(1)));
        assert_eq!(cache.get("missing"), None);
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert_eq!(cache.get("a"), None);
    }

    #[test]
    fn account_number_is_masked() {
        assert_eq!(mask_account("1234567890"), "1234…890");
        assert_eq!(mask_account("12"), "12…12");
    }

    /// A watchlist row states its own availability; numbers only when priced.
    #[test]
    fn quote_row_reports_availability() {
        use radar_core::webull::stocks::StockSnapshot;
        let mut snapshot = StockSnapshot {
            price: Some(101.5),
            change_ratio: Some(0.0125),
            last_trade_time_ms: Some(1_795_208_309_936),
            ..Default::default()
        };
        let row = quote_row("MSFT", Some(&snapshot), None, "t0");
        assert_eq!(row["status"], "available");
        assert_eq!(row["price"], json!(101.5));
        assert_eq!(row["change_pct"], json!(1.25));
        assert_eq!(row["market_session"], "regular");
        let stamped = chrono::DateTime::from_timestamp_millis(1_795_208_309_936)
            .map(|time| time.to_rfc3339())
            .expect("valid epoch");
        assert_eq!(row["source_timestamp"], json!(stamped));

        snapshot.price = Some(0.0);
        assert_eq!(
            quote_row("MSFT", Some(&snapshot), None, "t0")["status"],
            "no_data"
        );
        let failed = quote_row("MSFT", None, Some("why"), "t0");
        assert_eq!(failed["status"], "error");
        assert_eq!(failed["reason"], "why");
        assert!(failed.get("price").is_none());
    }

    /// Blocking work really leaves the UI thread, and a nested current-thread
    /// runtime (the MQTT probe) is legal on that worker.
    #[tokio::test]
    async fn blocking_work_runs_off_the_async_thread() {
        let result = off_ui_thread(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())?;
            let answer = runtime.block_on(async {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                42
            });
            Ok(answer)
        })
        .await;
        assert_eq!(result, Ok(42));
    }

    /// The first real launch of this layer crashed on its first command: the
    /// blocking client was built on the async thread, and a debug build panics
    /// there ("Cannot drop a runtime in a context where blocking is not
    /// allowed"). This awaits the helper from an async thread — the exact
    /// situation — so building the client anywhere but inside the closure
    /// fails here.
    #[tokio::test]
    async fn client_is_built_on_the_blocking_pool() {
        let settings = AppSettings {
            webull_app_key: "key".into(),
            webull_app_secret: "secret".into(),
            ..AppSettings::default()
        };
        let scheme = with_client_from(settings, |client| Ok(client.signature_scheme())).await;
        assert_eq!(scheme, Ok(SignatureScheme::V3));

        let missing = with_client_from(AppSettings::default(), |_| Ok(())).await;
        assert!(missing.unwrap_err().contains("credentials"));
    }

    /// A panic in one command must not take every later command down: the
    /// settings lock is still readable after it is poisoned.
    #[test]
    fn settings_survive_a_poisoned_lock() {
        const WHY: &str = "a command died while holding the lock";
        std::panic::set_hook(Box::new(|info| {
            let payload = info.payload();
            let text = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("");
            if !text.contains(WHY) {
                eprintln!("unexpected panic: {text}");
            }
        }));
        let state = AppState {
            engine: std::sync::Arc::new(Mutex::new(Engine::new(Utc::now()))),
            settings: Mutex::new(AppSettings::default()),
            scan: ScanControl::default(),
            app_data_dir: String::new(),
            pending_token: Mutex::new(None),
        };
        let lock = &state.settings;
        std::thread::scope(|scope| {
            let poisoned = scope.spawn(move || {
                let _guard = lock.lock().unwrap();
                panic!("{WHY}");
            });
            assert!(poisoned.join().is_err(), "the thread should have panicked");
        });
        let _ = std::panic::take_hook();

        assert!(
            state.settings.lock().is_err(),
            "the lock should be poisoned"
        );
        assert_eq!(settings_of(&state).last_symbol, "NVDA");
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
            pending_token: Mutex::new(None),
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
            get_stock_quote,
            get_stock_overview,
            get_option_quote,
            get_option_ticks,
            get_movers,
            get_sectors,
            get_sector_detail,
            get_broker_watchlists,
            get_broker_watchlist,
            lab_analyze,
            lab_freeze_prediction,
            lab_predictions,
            ask_copilot,
            test_llm,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
