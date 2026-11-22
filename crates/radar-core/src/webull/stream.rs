//! MQTT streaming layer — rumqttc connection wrapper, topic routing, and a
//! reconnect-ready subscription tracker. Enabled behind the `stream` feature.
//!
//! Wire protocol (verified against the official Python SDK internals,
//! webull.data.internal.quotes_client):
//! - MQTT v3.1.1 over TLS on port 1883 (SDK defaults: transport "tcp" +
//!   tls_set → platform CA roots), ClientId = session_id,
//!   Username = App Key, Password = any value
//! - Subscribe/unsubscribe via signed POST /openapi/market-data/streaming/{subscribe,unsubscribe}
//!   (x-version: v2) with matching session_id — see scan.rs for the caller
//! - Payloads: protobuf for quote/snapshot/tick (no framing), JSON for notice
//! - `echo` topic = heartbeat (null payload)
//! - Max 5 connections/app key, 3 msg/s per connection

use std::time::Duration;

use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet};

use super::rest::Credentials;

/// MQTT brokers (production / UAT).
pub const MQTT_HOST: &str = "data-api.webull.co.th";
pub const MQTT_HOST_UAT: &str = "data-api.uat.webullbroker.com";
/// TLS port used by the SDK (`DataStreamingClient(mqtt_port=1883, tls_enable=True)`).
pub const MQTT_PORT_TLS: u16 = 1883;
/// Documented alternative TLS port.
pub const MQTT_PORT_ALT: u16 = 8883;
/// Connection timeout.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Max messages per second per connection (Webull limit).
pub const MAX_MSG_PER_SEC: usize = 3;

/// A decoded streaming event.
#[derive(Clone, Debug)]
pub enum StreamEvent {
    /// Snapshot push (option contract or stock — decide by symbol: OCC vs root).
    Snapshot { payload: Vec<u8> },
    /// Heartbeat (echo topic).
    Heartbeat,
    /// Server notice (JSON).
    Notice(String),
    /// Connection state change.
    Connected,
    Disconnected,
}

/// Configuration for the MQTT stream.
#[derive(Clone, Debug)]
pub struct StreamConfig {
    pub host: String,
    pub port: u16,
    pub use_tls: bool,
    pub session_id: String,
}

impl StreamConfig {
    pub fn thailand(session_id: impl Into<String>) -> Self {
        Self {
            host: MQTT_HOST.to_string(),
            port: MQTT_PORT_TLS,
            use_tls: true,
            session_id: session_id.into(),
        }
    }

    pub fn uat(session_id: impl Into<String>) -> Self {
        Self {
            host: MQTT_HOST_UAT.to_string(),
            port: MQTT_PORT_TLS,
            use_tls: true,
            session_id: session_id.into(),
        }
    }

    /// Pick the broker that matches the REST base URL environment.
    pub fn for_base_url(base_url: &str, session_id: impl Into<String>) -> Self {
        if base_url.contains("uat") {
            Self::uat(session_id)
        } else {
            Self::thailand(session_id)
        }
    }

    /// Local test (no TLS).
    pub fn local(port: u16, session_id: impl Into<String>) -> Self {
        Self {
            host: "127.0.0.1".into(),
            port,
            use_tls: false,
            session_id: session_id.into(),
        }
    }

    /// rumqttc transport: raw MQTT v3.1.1 over TLS with platform CA roots
    /// (verified against the broker: a raw CONNECT gets a real CONNACK; a
    /// websocket upgrade is refused).
    pub fn rumqttc_transport(&self) -> rumqttc::Transport {
        if self.use_tls {
            rumqttc::Transport::Tls(rumqttc::TlsConfiguration::default())
        } else {
            rumqttc::Transport::Tcp
        }
    }
}

/// Route a raw MQTT message by topic to a typed [`StreamEvent`].
/// Returns `None` for unrecognized/irrelevant messages.
pub fn route_message(topic: &str, payload: &[u8]) -> Option<StreamEvent> {
    match topic {
        "snapshot" => Some(StreamEvent::Snapshot {
            payload: payload.to_vec(),
        }),
        "echo" => Some(StreamEvent::Heartbeat),
        "notice" => {
            let text = String::from_utf8_lossy(payload).to_string();
            Some(StreamEvent::Notice(text))
        }
        // quote (order book) / tick (trade print) feed the stock panels —
        // stock spot already arrives on "snapshot"; skip for now.
        _ => None,
    }
}

/// The streaming client — wraps rumqttc; recreate on reconnect (clean session
/// means the broker drops subscription state, so HTTP subscribe must re-run).
pub struct StreamClient {
    config: StreamConfig,
    client: AsyncClient,
    event_loop: EventLoop,
    subscribed_topics: Vec<(Vec<String>, String)>, // (symbols, topic)
}

impl StreamClient {
    /// Create a new stream client (does NOT connect yet).
    pub fn new(config: StreamConfig, creds: Credentials) -> Self {
        let mut opts = MqttOptions::new(&config.session_id, &config.host, config.port);
        opts.set_keep_alive(Duration::from_secs(30));
        // Webull: username = App Key, password = any value (SDK uses a uuid)
        opts.set_credentials(&creds.app_key, "webull");
        opts.set_transport(config.rumqttc_transport());

        let (client, event_loop) = AsyncClient::new(opts, 64);
        Self {
            config,
            client,
            event_loop,
            subscribed_topics: Vec::new(),
        }
    }

    /// Track a subscription for auto-resubscribe on reconnect.
    pub fn track_subscribe(&mut self, symbols: Vec<String>, topic: String) {
        // remove old entries for this topic, then push the replacement
        self.subscribed_topics.retain(|(_, t)| t != &topic);
        self.subscribed_topics.push((symbols, topic));
    }

    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    pub fn client(&self) -> &AsyncClient {
        &self.client
    }

    pub fn event_loop(&mut self) -> &mut EventLoop {
        &mut self.event_loop
    }

    pub fn subscriptions(&self) -> &[(Vec<String>, String)] {
        &self.subscribed_topics
    }

    /// Process one MQTT event into a StreamEvent (or None for irrelevant packets).
    /// Disconnection surfaces as `Err` from `event_loop.poll()`.
    pub fn handle_event(
        &mut self,
        event: Result<Event, rumqttc::ConnectionError>,
    ) -> Option<StreamEvent> {
        match event {
            Ok(Event::Incoming(Packet::ConnAck(_))) => Some(StreamEvent::Connected),
            Ok(Event::Incoming(Packet::Publish(msg))) => {
                let topic = msg.topic.clone();
                let payload = msg.payload.to_vec();
                route_message(topic.as_str(), &payload)
            }
            Err(_) => Some(StreamEvent::Disconnected),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_thailand_defaults() {
        let c = StreamConfig::thailand("test_session");
        assert_eq!(c.host, MQTT_HOST);
        assert_eq!(c.port, 1883);
        assert!(c.use_tls);
        assert_eq!(c.session_id, "test_session");
    }

    #[test]
    fn config_follows_base_url_environment() {
        let uat = StreamConfig::for_base_url("https://th-api.uat.webullbroker.com", "s");
        assert_eq!(uat.host, MQTT_HOST_UAT);
        let prod = StreamConfig::for_base_url("https://api.webull.co.th", "s");
        assert_eq!(prod.host, MQTT_HOST);
    }

    #[test]
    fn config_local_no_tls() {
        let c = StreamConfig::local(1883, "local_test");
        assert_eq!(c.host, "127.0.0.1");
        assert_eq!(c.port, 1883);
        assert!(!c.use_tls);
    }

    #[test]
    fn route_echo_heartbeat() {
        assert!(matches!(route_message("echo", &[]), Some(StreamEvent::Heartbeat)));
    }

    #[test]
    fn route_notice_json() {
        let n = route_message("notice", br#"{"type":"status","rtt":100}"#);
        assert!(matches!(n, Some(StreamEvent::Notice(ref t)) if t.contains("status")));
    }

    #[test]
    fn route_unknown_topic_none() {
        assert!(route_message("garbage", &[]).is_none());
    }

    #[test]
    fn track_subscribe_replaces_old_topic_entries() {
        let mut c = StreamClient::new(
            StreamConfig::local(1883, "s"),
            Credentials {
                app_key: "K".into(),
                app_secret: "S".into(),
                access_token: None,
            },
        );
        c.track_subscribe(vec!["A".into(), "B".into()], "snapshot".into());
        c.track_subscribe(vec!["C".into()], "snapshot".into()); // replaces, not appends
        assert_eq!(c.subscriptions().len(), 1);
        assert_eq!(c.subscriptions()[0].0, vec!["C".to_string()]);
        // different topic coexists
        c.track_subscribe(vec!["X".into()], "tick".into());
        assert_eq!(c.subscriptions().len(), 2);
    }

    #[test]
    fn route_snapshot_produces_event() {
        let e = route_message("snapshot", &[1, 2, 3]);
        assert!(matches!(e, Some(StreamEvent::Snapshot { .. })));
    }
}
