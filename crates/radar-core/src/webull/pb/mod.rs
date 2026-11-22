//! Webull streaming protobuf messages — manually defined with prost derive
//! (no external protoc needed). Based on the Webull Data Streaming API docs:
//! https://developer.webull.com/apis/docs/market-data-api/data-streaming-api
//!
//! Wire format: standard protobuf varint/length-delimited fields.

use prost::Message;

/// Option/stock snapshot — the primary data payload for our screener.
/// Field numbers match the Webull streaming docs.
#[derive(Clone, PartialEq, Message)]
pub struct Snapshot {
    /// Trading symbol (OCC format for options, ticker for stocks)
    #[prost(string, tag = "1")]
    pub symbol: String,
    /// Last traded price
    #[prost(double, tag = "2")]
    pub price: f64,
    /// Total volume
    #[prost(double, tag = "3")]
    pub volume: f64,
    /// Price change
    #[prost(double, tag = "4")]
    pub change: f64,
    /// Percentage change (fraction, e.g. 0.015 = 1.5%)
    #[prost(double, tag = "5")]
    pub change_ratio: f64,
    /// Open interest (options only)
    #[prost(double, tag = "6")]
    pub open_interest: f64,
    /// Bid price
    #[prost(double, tag = "7")]
    pub bid: f64,
    /// Ask price
    #[prost(double, tag = "8")]
    pub ask: f64,
    /// Day open
    #[prost(double, tag = "9")]
    pub open: f64,
    /// Day high
    #[prost(double, tag = "10")]
    pub high: f64,
    /// Day low
    #[prost(double, tag = "11")]
    pub low: f64,
    /// Previous close
    #[prost(double, tag = "12")]
    pub pre_close: f64,
    /// Extended-hours price (if available)
    #[prost(double, tag = "13", optional)]
    pub ext_price: Option<f64>,
    /// Overnight price (if available)
    #[prost(double, tag = "14", optional)]
    pub ovn_price: Option<f64>,
}

/// Order book level (ask or bid).
#[derive(Clone, PartialEq, Message)]
pub struct PriceSize {
    #[prost(double, tag = "1")]
    pub price: f64,
    #[prost(double, tag = "2")]
    pub size: f64,
}

/// Order book quote — bid/ask levels.
#[derive(Clone, PartialEq, Message)]
pub struct Quote {
    #[prost(string, tag = "1")]
    pub symbol: String,
    #[prost(message, repeated, tag = "2")]
    pub asks: Vec<PriceSize>,
    #[prost(message, repeated, tag = "3")]
    pub bids: Vec<PriceSize>,
}

/// Individual trade tick.
#[derive(Clone, PartialEq, Message)]
pub struct Tick {
    /// Trade timestamp (ms since epoch)
    #[prost(int64, tag = "1")]
    pub time: i64,
    /// Trade price
    #[prost(double, tag = "2")]
    pub price: f64,
    /// Trade volume
    #[prost(double, tag = "3")]
    pub volume: f64,
    /// Trade side: "B" = buy, "S" = sell
    #[prost(string, tag = "4")]
    pub side: String,
}

/// Decode a snapshot from protobuf bytes.
pub fn decode_snapshot(data: &[u8]) -> Option<Snapshot> {
    Snapshot::decode(data).ok()
}

/// Decode a quote (order book) from protobuf bytes.
pub fn decode_quote(data: &[u8]) -> Option<Quote> {
    Quote::decode(data).ok()
}

/// Decode a tick (trade print) from protobuf bytes.
pub fn decode_tick(data: &[u8]) -> Option<Tick> {
    Tick::decode(data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_roundtrip() {
        let snap = Snapshot {
            symbol: "NVDA261218P00175000".into(),
            price: 4.20,
            volume: 1842.0,
            change: 0.15,
            change_ratio: 0.037,
            open_interest: 3120.0,
            bid: 4.15,
            ask: 4.25,
            open: 4.00,
            high: 4.50,
            low: 3.90,
            pre_close: 4.05,
            ext_price: None,
            ovn_price: None,
        };
        let bytes = snap.encode_to_vec();
        let back = decode_snapshot(&bytes).unwrap();
        assert_eq!(back.symbol, "NVDA261218P00175000");
        assert_eq!(back.price, 4.20);
        assert_eq!(back.open_interest, 3120.0);
    }

    #[test]
    fn quote_roundtrip() {
        let q = Quote {
            symbol: "NVDA".into(),
            asks: vec![
                PriceSize {
                    price: 182.50,
                    size: 100.0,
                },
                PriceSize {
                    price: 182.55,
                    size: 200.0,
                },
            ],
            bids: vec![PriceSize {
                price: 182.45,
                size: 150.0,
            }],
        };
        let bytes = q.encode_to_vec();
        let back = decode_quote(&bytes).unwrap();
        assert_eq!(back.asks.len(), 2);
        assert_eq!(back.bids.len(), 1);
        assert_eq!(back.asks[0].price, 182.50);
    }

    #[test]
    fn tick_roundtrip() {
        let t = Tick {
            time: 1732310400000,
            price: 182.45,
            volume: 100.0,
            side: "B".into(),
        };
        let bytes = t.encode_to_vec();
        let back = decode_tick(&bytes).unwrap();
        assert_eq!(back.price, 182.45);
        assert_eq!(back.side, "B");
    }

    #[test]
    fn decode_empty_bytes_gives_default() {
        // Empty protobuf = all fields at default (0/empty) — valid decode
        let snap = decode_snapshot(&[]).unwrap();
        assert_eq!(snap.price, 0.0);
        assert_eq!(snap.symbol, "");
    }

    #[test]
    fn decode_snapshot_from_partial_bytes() {
        // Just a symbol field (tag 1, length-delimited) should decode
        // tag=0x0A (field 1, wire type 2), len=5, "NVDA"
        let bytes = [0x0A, 0x04, b'N', b'V', b'D', b'A'];
        let snap = decode_snapshot(&bytes).unwrap();
        assert_eq!(snap.symbol, "NVDA");
        assert_eq!(snap.price, 0.0); // other fields default
    }

    #[test]
    fn snapshot_to_engine_quote_update() {
        let snap = Snapshot {
            symbol: "NVDA261218P00175000".into(),
            price: 4.20,
            volume: 1842.0,
            change: 0.15,
            change_ratio: 0.037,
            open_interest: 3120.0,
            bid: 4.15,
            ask: 4.25,
            open: 0.0,
            high: 0.0,
            low: 0.0,
            pre_close: 0.0,
            ext_price: None,
            ovn_price: None,
        };
        // Convert to engine QuoteUpdate
        let update = crate::engine::QuoteUpdate {
            symbol: snap.symbol,
            price: snap.price,
            volume: snap.volume,
            change: snap.change,
            change_ratio: snap.change_ratio,
            oi: snap.open_interest,
            bid: snap.bid,
            ask: snap.ask,
            src: crate::engine::DataSource::Ws,
        };
        assert_eq!(update.price, 4.20);
        assert_eq!(update.oi, 3120.0);
    }

    #[test]
    fn snapshot_roundtrip_prost() {
        // encode → decode must survive the exact bytes a broker push carries
        let snap = Snapshot {
            symbol: "NVDA261218P00175000".into(),
            price: 4.2,
            volume: 1842.0,
            change: 0.1,
            change_ratio: 0.02,
            open_interest: 3120.0,
            bid: 4.15,
            ask: 4.25,
            open: 180.0,
            high: 183.7,
            low: 179.1,
            pre_close: 180.14,
            ext_price: None,
            ovn_price: None,
        };
        let bytes = snap.encode_to_vec();
        let back = decode_snapshot(&bytes).expect("roundtrip decode");
        assert_eq!(back.symbol, "NVDA261218P00175000");
        assert_eq!(back.price, 4.2);
        assert_eq!(back.open_interest, 3120.0);
    }
}
