//! Live MQTT probe — verifies the whole streaming chain against a real
//! environment before wiring into the app:
//! TLS connect (native certs) → session auth (username = app key) →
//! signed POST subscribe (x-version v2) → protobuf snapshot decode.
//!
//! Run (UAT):
//!   set -a; source ../vcpSystem/trade-service/.env.uat; set +a
//!   cargo run -p radar-core --example mqtt_probe --features "client,stream"
//!
//! Env: APP_KEY / APP_SECRET (+ optional ACCESS_TOKEN, BASE_URL default UAT).
//! Prints statuses only — never key material.

use std::time::Duration;

use radar_core::webull::rest::{Credentials, PATH_STREAMING_SUBSCRIBE};
use radar_core::webull::stream::{StreamClient, StreamConfig, StreamEvent};
use radar_core::webull::transport::WebullClient;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let key = std::env::var("APP_KEY")
        .or(std::env::var("WEBULL_APP_KEY"))
        .unwrap_or_default();
    let secret = std::env::var("APP_SECRET")
        .or(std::env::var("WEBULL_APP_SECRET"))
        .unwrap_or_default();
    let token = std::env::var("ACCESS_TOKEN")
        .or(std::env::var("WEBULL_ACCESS_TOKEN"))
        .ok()
        .filter(|t| !t.is_empty());
    let base_url =
        std::env::var("BASE_URL").unwrap_or_else(|_| "https://th-api.uat.webullbroker.com".into());
    if key.is_empty() || secret.is_empty() {
        eprintln!("ต้องมี APP_KEY/APP_SECRET ใน env");
        std::process::exit(1);
    }
    let creds = Credentials {
        app_key: key,
        app_secret: secret,
        access_token: token,
    };
    println!("base_url = {base_url}");

    let session_id = format!("probe_{}", &uuid_like());
    let mut config = StreamConfig::for_base_url(&base_url, session_id.clone());
    // transport matrix knobs for probing: MQTT_PORT / MQTT_TLS=0|1
    if let Ok(p) = std::env::var("MQTT_PORT") {
        if let Ok(p) = p.parse::<u16>() {
            config.port = p;
        }
    }
    if let Ok(t) = std::env::var("MQTT_TLS") {
        config.use_tls = t != "0";
    }
    println!(
        "mqtt = {}:{} (tls={})",
        config.host, config.port, config.use_tls
    );
    let mut stream = StreamClient::new(config, creds.clone());

    // drive the event loop with an overall deadline
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    let mut got = 0u32;
    loop {
        let remain = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remain.is_zero() {
            println!("[probe] หมดเวลา 45 วิ — ได้ snapshots รวม {got}");
            break;
        }
        let ev = tokio::time::timeout(remain, stream.event_loop().poll()).await;
        let event = match ev {
            Err(_) => {
                println!("[probe] timeout");
                break;
            }
            Ok(Err(e)) => {
                println!("[probe] connection error: {e}");
                break;
            }
            Ok(Ok(e)) => e,
        };
        let Some(se) = stream.handle_event(Ok(event)) else {
            continue;
        };
        match se {
            StreamEvent::Connected => {
                println!("[probe] ✅ MQTT CONNECTED (session {session_id}) — subscribing ...");
                let sub = do_subscribe(&creds, &base_url, &session_id);
                match tokio::task::spawn_blocking(move || sub).await {
                    Ok(Ok(())) => println!("[probe] ✅ subscribe สำเร็จ (stock + options)"),
                    Ok(Err(e)) => {
                        println!("[probe] ❌ subscribe ล้มเหลว: {e}");
                        break;
                    }
                    Err(e) => {
                        println!("[probe] ❌ spawn_blocking: {e}");
                        break;
                    }
                }
            }
            StreamEvent::Snapshot { payload } => {
                let snap = radar_core::webull::pb::decode_snapshot(&payload);
                match snap {
                    Some(s) => {
                        got += 1;
                        if got <= 8 {
                            println!(
                                "[probe] 📨 snapshot #{}: {} price={} bid={} ask={} oi={}",
                                got, s.symbol, s.price, s.bid, s.ask, s.open_interest
                            );
                        }
                        if got >= 5 {
                            println!("[probe] ได้ 5 snapshots แล้ว — ผ่านครบ ✅");
                            break;
                        }
                    }
                    None => println!("[probe] ⚠ decode ล้มเหลว ({} bytes)", payload.len()),
                }
            }
            StreamEvent::Heartbeat => println!("[probe] 💓 echo"),
            StreamEvent::Notice(t) => println!(
                "[probe] 📢 notice: {}",
                &t.chars().take(140).collect::<String>()
            ),
            _ => {}
        }
    }
}

fn do_subscribe(creds: &Credentials, base_url: &str, session_id: &str) -> Result<(), String> {
    let mut http = WebullClient::new(creds.clone()).with_base_url(base_url);
    let stock = serde_json::json!({
        "session_id": session_id, "symbols": ["AAPL"], "category": "US_STOCK",
        "sub_types": ["SNAPSHOT"],
    })
    .to_string();
    let (st, b) = http
        .post_json(PATH_STREAMING_SUBSCRIBE, &stock)
        .map_err(|e| e.to_string())?;
    println!("[probe] subscribe stock -> {st} {}", clip(&b.to_string()));
    if st != 200 {
        return Err(format!("stock {st}"));
    }
    let opts = serde_json::json!({
        "session_id": session_id,
        "symbols": ["AAPL261127C00300000", "AAPL261127P00300000"],
        "category": "US_OPTION", "sub_types": ["SNAPSHOT"],
    })
    .to_string();
    let (st, b) = http
        .post_json(PATH_STREAMING_SUBSCRIBE, &opts)
        .map_err(|e| e.to_string())?;
    println!("[probe] subscribe options -> {st} {}", clip(&b.to_string()));
    if st != 200 {
        return Err(format!("options {st}"));
    }
    Ok(())
}

fn clip(s: &str) -> String {
    s.chars().take(140).collect()
}

fn uuid_like() -> String {
    // cheap unique suffix without pulling uuid into radar-core's dev deps
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("{ms:x}")
}
