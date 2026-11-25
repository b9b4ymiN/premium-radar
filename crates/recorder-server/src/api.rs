//! HTTP API แบบ read-only — desktop app ดึงข้อมูล Lab ผ่านตรงนี้แทนไฟล์
//! local (shape ตรงกับ `LabStorage` 100%)
//!
//! - `GET /health` ไม่ต้อง auth — บอกแค่ ok + version
//! - `GET /v1/*` ต้องมี `Authorization: Bearer <token>` (401 ถ้าไม่มี/ผิด)
//!   เทียบ token แบบ constant-time และไม่ log ค่าจริงเด็ดขาด
//! - response ใหญ่ (observations เป็น MB) บีบอัดได้ด้วย gzip เมื่อ client
//!   ส่ง `Accept-Encoding: gzip` (tower-http CompressionLayer)
//! - file IO ทั้งหมดอยู่ใน `spawn_blocking`
//!
//! TLS ไม่ทำใน process (Tailscale/Cloudflare เป็นคนยุติ) และไม่มี
//! write endpoint ใดๆ

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use chrono::{NaiveDate, SecondsFormat, Utc};
use radar_core::lab::storage::LabStorage;
use serde_json::{Value, json};
use tower_http::compression::CompressionLayer;

use crate::config::Config;

/// bind default เมื่อไม่ตั้ง `RECORDER_BIND`
pub const DEFAULT_BIND: &str = "0.0.0.0:8787";

/// state ที่ handler ใช้ — token อยู่ใน `Arc<str>` ไม่ถูก log หรือ serialize
#[derive(Clone)]
pub struct ApiState {
    lab_dir: PathBuf,
    data_dir: PathBuf,
    token: Arc<str>,
}

impl ApiState {
    pub fn new(cfg: &Config, token: String) -> Self {
        Self {
            lab_dir: cfg.lab_dir(),
            data_dir: cfg.data_dir.clone(),
            token: Arc::from(token.as_str()),
        }
    }
}

/// bind address จาก env `RECORDER_BIND` (default `0.0.0.0:8787`)
pub fn bind_addr() -> Result<SocketAddr, String> {
    std::env::var("RECORDER_BIND")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BIND.to_string())
        .parse()
        .map_err(|e| format!("RECORDER_BIND ไม่ใช่ address ที่ใช้ได้: {e}"))
}

/// API token ของ `run`: env `RECORDER_API_TOKEN` ชนะไฟล์
/// `<data_dir>/secrets/api_token` (trim แล้วต้องไม่ว่าง)
pub fn resolve_api_token(cfg: &Config) -> Option<String> {
    if let Some(t) = std::env::var("RECORDER_API_TOKEN")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        return Some(t.trim().to_string());
    }
    std::fs::read_to_string(cfg.api_token_file())
        .ok()
        .map(|raw| raw.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// บอกว่า token มาจากไหน โดยไม่แตะค่าจริง — ใช้ใน `plan`
pub fn api_token_source(cfg: &Config) -> Option<&'static str> {
    if std::env::var("RECORDER_API_TOKEN").is_ok_and(|s| !s.trim().is_empty()) {
        Some("env")
    } else if cfg.api_token_file().exists() {
        Some("file")
    } else {
        None
    }
}

/// เทียบ token แบบ constant-time — วนครบความยาวทางยาวสุดเสมอ ไม่ early-exit
/// ที่ตำแหน่ง mismatch (ผ่าน same-length bytes + เช็คความยาวท้ายรอบเดียว)
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let len_mismatch = a.len() != b.len();
    let mut byte_diff: u8 = 0;
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        byte_diff |= x ^ y;
    }
    !len_mismatch && byte_diff == 0
}

fn error_response(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

/// middleware ที่คุมทุก `/v1/*` — ไม่มี/ผิด token = 401 JSON error
async fn require_bearer(
    State(state): State<ApiState>,
    headers: HeaderMap,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let provided = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    match provided {
        Some(t) if constant_time_eq(t, &state.token) => next.run(req).await,
        _ => error_response(StatusCode::UNAUTHORIZED, "missing or invalid bearer token"),
    }
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }))
}

/// `GET /v1/status` — status.json ทั้งไฟล์ (หรือ `{}`) + `server_time`
async fn status_file(State(state): State<ApiState>) -> Response {
    let path = state.data_dir.join("status.json");
    let value = tokio::task::spawn_blocking(move || {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .filter(|v| v.is_object())
            .unwrap_or_else(|| json!({}))
    })
    .await
    .unwrap_or_else(|_| json!({}));
    let mut value = value;
    value["server_time"] = json!(Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true));
    Json(value).into_response()
}

/// `GET /v1/dates` — `{"dates":["YYYY-MM-DD",...]}` จาก `captured_dates()`
async fn dates(State(state): State<ApiState>) -> Response {
    let dir = state.lab_dir.clone();
    let res = tokio::task::spawn_blocking(move || {
        LabStorage::open(&dir).and_then(|s| s.captured_dates())
    })
    .await;
    match res {
        Ok(Ok(list)) => Json(json!({
            "dates": list.iter().map(|d| d.to_string()).collect::<Vec<_>>()
        }))
        .into_response(),
        Ok(Err(e)) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("อ่าน dates ไม่ได้: {e}"),
        ),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("worker: {e}")),
    }
}

fn date_param(q: &HashMap<String, String>) -> Result<NaiveDate, String> {
    let raw = q
        .get("date")
        .ok_or("ต้องมี query ?date=YYYY-MM-DD".to_string())?;
    raw.parse::<NaiveDate>()
        .map_err(|e| format!("date '{raw}' อ่านไม่ได้: {e}"))
}

/// `GET /v1/observations?date=` — JSON array ของ `Observation` ตรงตาม
/// `read_observations` คืน (day ใหญ่เป็น MB → บีบ gzip ได้)
async fn observations(
    State(state): State<ApiState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let date = match date_param(&q) {
        Ok(d) => d,
        Err(msg) => return error_response(StatusCode::BAD_REQUEST, &msg),
    };
    let dir = state.lab_dir.clone();
    let res = tokio::task::spawn_blocking(move || {
        LabStorage::open(&dir).and_then(|s| s.read_observations(date))
    })
    .await;
    match res {
        Ok(Ok(obs)) => Json(obs).into_response(),
        Ok(Err(e)) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("อ่าน observations ไม่ได้: {e}"),
        ),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("worker: {e}")),
    }
}

/// `GET /v1/export?date=` — string จาก `export_day` ส่งเป็น JSON string
async fn export_day(
    State(state): State<ApiState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let date = match date_param(&q) {
        Ok(d) => d,
        Err(msg) => return error_response(StatusCode::BAD_REQUEST, &msg),
    };
    let dir = state.lab_dir.clone();
    let res = tokio::task::spawn_blocking(move || {
        LabStorage::open(&dir).and_then(|s| s.export_day(date))
    })
    .await;
    match res {
        Ok(Ok(exported)) => Json(json!(exported)).into_response(),
        Ok(Err(e)) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("export วันที่ {date} ไม่ได้: {e}"),
        ),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("worker: {e}")),
    }
}

/// `GET /v1/universe` — universe.json ทั้งไฟล์ (404 ถ้ายังไม่มี)
async fn universe(State(state): State<ApiState>) -> Response {
    let path = state.data_dir.join("universe.json");
    let res = tokio::task::spawn_blocking(move || std::fs::read_to_string(path)).await;
    match res {
        Ok(Ok(raw)) => match serde_json::from_str::<Value>(&raw) {
            Ok(v) => Json(v).into_response(),
            Err(e) => error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("universe.json parse ไม่ผ่าน: {e}"),
            ),
        },
        Ok(Err(_)) => error_response(StatusCode::NOT_FOUND, "universe.json ยังไม่มี"),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &format!("worker: {e}")),
    }
}

/// สร้าง router เต็ม — `/health` ไม่มี auth, `/v1/*` ต้องมี bearer token
pub fn router(state: ApiState) -> Router {
    let v1 = Router::new()
        .route("/status", get(status_file))
        .route("/dates", get(dates))
        .route("/observations", get(observations))
        .route("/export", get(export_day))
        .route("/universe", get(universe))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_bearer,
        ));
    Router::new()
        .route("/health", get(health))
        .nest("/v1", v1)
        .layer(CompressionLayer::new())
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt;

    /// temp data dir + state ที่ผูก token "test-token"
    fn temp_state(tag: &str) -> (ApiState, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("recorder-server-api-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (
            ApiState {
                lab_dir: dir.join("lab"),
                data_dir: dir.clone(),
                token: Arc::from("test-token"),
            },
            dir,
        )
    }

    fn req(uri: &str, token: Option<&str>) -> Request<Body> {
        let mut b = Request::get(uri);
        if let Some(t) = token {
            b = b.header("authorization", format!("Bearer {t}"));
        }
        b.body(Body::empty()).unwrap()
    }

    async fn body_json(resp: Response) -> Value {
        let bytes = to_bytes(resp.into_body(), 16 * 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn write_observations_file(dir: &std::path::Path, date: &str, lines: &[String]) {
        let day = dir.join("lab").join("v1").join(date);
        std::fs::create_dir_all(&day).unwrap();
        let mut body = lines.join("\n");
        body.push('\n');
        std::fs::write(day.join("observations.jsonl"), body).unwrap();
    }

    /// Observation เป็น JSONL บรรทัด (เหมือนที่ LabStorage เขียน)
    fn obs_line(symbol: &str, i: u32) -> String {
        let t = "2026-11-24T15:00:00Z";
        serde_json::json!({
            "contract": {
                "occ": format!("{symbol}261218C0010000{i}"),
                "underlying": symbol,
                "expiry": "2026-12-18",
                "option_type": "C",
                "strike": 100.0,
                "multiplier": 100.0
            },
            "kind": {
                "OptionQuote": {
                    "price": 1.5, "bid": 1.2, "ask": 1.8, "iv": null,
                    "delta": null, "oi": 500.0, "volume": 12.0
                }
            },
            "event_time": t,
            "received_time": t,
            "session_date": "2026-11-24",
            "available_at": t,
            "source": "API",
            "backfill": false,
            "universe_id": null
        })
        .to_string()
    }

    /// เตรียม lab dir ที่มี 2026-11-24 (12 obs — ใหญ่พอจะโดน gzip) + 2026-11-23
    fn populate_lab(dir: &std::path::Path) {
        let lines: Vec<String> = (0..12).map(|i| obs_line("NVDA", i)).collect();
        write_observations_file(dir, "2026-11-24", &lines);
        write_observations_file(dir, "2026-11-23", &[obs_line("TSLA", 0)]);
    }

    #[tokio::test]
    async fn health_needs_no_auth_and_leaks_nothing() {
        let (state, dir) = temp_state("health");
        let resp = router(state).oneshot(req("/health", None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["ok"], json!(true));
        assert!(body["version"].is_string());
        // ไม่มี field อื่น (ไม่มี data/secrets)
        assert_eq!(body.as_object().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn v1_rejects_missing_and_wrong_token() {
        let (state, dir) = temp_state("auth");
        populate_lab(&dir);
        let app = router(state);

        for uri in ["/v1/dates", "/v1/status", "/v1/universe"] {
            let resp = app.clone().oneshot(req(uri, None)).await.unwrap();
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{uri} no token");
            let body = body_json(resp).await;
            assert!(body["error"].is_string(), "{uri}");

            let resp = app
                .clone()
                .oneshot(req(uri, Some("wrong-token")))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{uri} wrong token");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn dates_and_observations_match_lab_storage_shapes() {
        let (state, dir) = temp_state("shapes");
        populate_lab(&dir);

        // /v1/dates
        let resp = router(state.clone())
            .oneshot(req("/v1/dates", Some("test-token")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["dates"], json!(["2026-11-23", "2026-11-24"]));

        // /v1/observations — array ของ Observation เท่าที่อ่านได้จากไฟล์
        let resp = router(state.clone())
            .oneshot(req("/v1/observations?date=2026-11-24", Some("test-token")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let obs = body_json(resp).await;
        let arr = obs.as_array().unwrap();
        assert_eq!(arr.len(), 12);
        assert_eq!(arr[0]["contract"]["underlying"], json!("NVDA"));
        assert_eq!(arr[0]["kind"]["OptionQuote"]["bid"], json!(1.2));

        // เทียบกับ LabStorage ตรงๆ — shape ต้องเหมือนกัน
        let storage = LabStorage::open(dir.join("lab")).unwrap();
        let want: Value = {
            let list = storage
                .read_observations("2026-11-24".parse().unwrap())
                .unwrap();
            serde_json::to_value(&list).unwrap()
        };
        assert_eq!(obs, want);

        // bad date = 400 พร้อม JSON error
        for bad in ["/v1/observations", "/v1/observations?date=2026-13-99"] {
            let resp = router(state.clone())
                .oneshot(req(bad, Some("test-token")))
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{bad}");
            assert!(body_json(resp).await["error"].is_string());
        }

        // /v1/export — JSON string ของ export_day
        let resp = router(state)
            .oneshot(req("/v1/export?date=2026-11-23", Some("test-token")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let exported = body_json(resp).await;
        assert!(exported.is_string());
        assert!(exported.as_str().unwrap().contains("\"observations\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn status_merges_file_with_server_time() {
        let (state, dir) = temp_state("status");
        std::fs::write(
            dir.join("status.json"),
            r#"{"layer1": {"jobs": 3}, "completed_slots": [{"id": "2026-11-24/layer1/16:30", "finished_at": "2026-11-24T22:00:00Z"}]}"#,
        )
        .unwrap();
        let resp = router(state.clone())
            .oneshot(req("/v1/status", Some("test-token")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["layer1"]["jobs"], json!(3));
        assert!(body["server_time"].is_string());

        // ไฟล์ไม่มี = {} + server_time
        let (empty_state, dir2) = temp_state("status-empty");
        let resp = router(empty_state)
            .oneshot(req("/v1/status", Some("test-token")))
            .await
            .unwrap();
        let body = body_json(resp).await;
        assert_eq!(body.as_object().unwrap().len(), 1);
        assert!(body["server_time"].is_string());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir2);
    }

    #[tokio::test]
    async fn universe_404_when_missing_and_200_when_present() {
        let (state, dir) = temp_state("univ");
        let resp = router(state.clone())
            .oneshot(req("/v1/universe", Some("test-token")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert!(body_json(resp).await["error"].is_string());

        std::fs::write(
            dir.join("universe.json"),
            r#"{"fetched_at": "2026-11-23T23:00:00Z", "source": "nasdaq",
                "symbols": [{"symbol": "AAPL", "name": "Apple", "sector": "Technology", "market_cap": 3e12}]}"#,
        )
        .unwrap();
        let resp = router(state)
            .oneshot(req("/v1/universe", Some("test-token")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["symbols"][0]["symbol"], json!("AAPL"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// client ส่ง Accept-Encoding: gzip → response ใหญ่ต้องถูกบีบอัด
    #[tokio::test]
    async fn observations_gzip_when_requested() {
        let (state, dir) = temp_state("gzip");
        populate_lab(&dir);
        let request = Request::get("/v1/observations?date=2026-11-24")
            .header("authorization", "Bearer test-token")
            .header("accept-encoding", "gzip")
            .body(Body::empty())
            .unwrap();
        let resp = router(state).oneshot(request).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let encoding = resp
            .headers()
            .get("content-encoding")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            encoding.contains("gzip"),
            "content-encoding คือ {encoding:?} — ควรเป็น gzip"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn constant_time_eq_behaves() {
        assert!(constant_time_eq("same", "same"));
        assert!(!constant_time_eq("same", "diff"));
        assert!(!constant_time_eq("same", "sam"));
        assert!(!constant_time_eq("", "x"));
        assert!(constant_time_eq("", ""));
        assert!(!constant_time_eq(
            "prefix-of-longer",
            "prefix-of-longer-extra"
        ));
    }
}
