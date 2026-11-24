//! The watchlists the broker holds, read only (reference family
//! "Watchlists" — both list routes live-verified 2026-11-23 and 2026-11-24).
//! The app's own watchlists live in `radar_core::watchlists`; this module only
//! shows what the broker holds so a symbol can be copied into a local list.
//! The six write routes of that family are not implemented.
//!
//! Both readers are strict about the envelope: the instruments route answers
//! `{watchlist_id, instruments: [...]}`, not the `{data: [...]}` most lists
//! use. A 200 whose shape is not recognised is an error, never an empty list.

use serde_json::Value;

use super::rest::{FetchError, HttpFetch, opt_f64, opt_str};

/// GET `/market-data/watchlists/list`
pub const PATH_WATCHLISTS: &str = "/market-data/watchlists/list";
/// GET `/market-data/watchlists/instruments/list`
pub const PATH_WATCHLIST_INSTRUMENTS: &str = "/market-data/watchlists/instruments/list";

/// The rows of a list body: a bare array, or an object holding them under
/// `key` (the common `data` envelope is accepted too). Anything else is
/// reported by shape only — field names, never values.
pub(super) fn rows_of<'a>(
    data: &'a Value,
    key: &str,
    what: &str,
) -> Result<&'a [Value], FetchError> {
    const DATA: &str = "data";
    match data {
        Value::Array(rows) => Ok(rows),
        Value::Object(fields) => [key, DATA]
            .into_iter()
            .find_map(|name| fields.get(name).and_then(Value::as_array))
            .map(Vec::as_slice)
            .ok_or_else(|| {
                let names: Vec<&str> = fields.keys().map(String::as_str).collect();
                FetchError::Unexpected(format!("{what}: object with fields [{names:?}]"))
            }),
        _ => Err(FetchError::Unexpected(format!("{what}: not a list"))),
    }
}

/// One watchlist as the broker holds it.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct BrokerWatchlist {
    pub id: String,
    pub name: String,
    /// Broker timestamp of the last change, ISO text as returned.
    pub update_time: Option<String>,
}

/// The identifier arrives as text; a number is tolerated.
fn id_of(row: &Value, field: &str) -> Option<String> {
    match row.get(field) {
        Some(Value::String(id)) => (!id.trim().is_empty()).then(|| id.clone()),
        Some(Value::Number(id)) => Some(id.to_string()),
        _ => None,
    }
}

/// Parse the account's watchlists in the broker's order (its `sort` column,
/// stable for rows without one); a list without an id cannot be read and is
/// skipped. A non-empty array that loses every row is a schema mismatch, not
/// "no watchlists".
pub fn parse_watchlists(data: &Value) -> Result<Vec<BrokerWatchlist>, FetchError> {
    let rows = rows_of(data, "watchlists", "watchlists")?;
    let mut lists: Vec<(Option<f64>, BrokerWatchlist)> = Vec::new();
    let mut skipped = 0usize;
    for row in rows {
        match id_of(row, "watchlist_id") {
            Some(id) => lists.push((
                opt_f64(row.get("sort")),
                BrokerWatchlist {
                    id,
                    name: opt_str(row.get("name")).unwrap_or_else(|| "ไม่มีชื่อ".to_string()),
                    update_time: opt_str(row.get("update_time")),
                },
            )),
            None => skipped += 1,
        }
    }
    if !rows.is_empty() && skipped == rows.len() {
        return Err(FetchError::Unexpected(
            "watchlists: every row is missing watchlist_id".into(),
        ));
    }
    lists.sort_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Ok(lists.into_iter().map(|(_, list)| list).collect())
}

/// Every watchlist of the account.
pub fn fetch_watchlists<F: HttpFetch>(f: &mut F) -> Result<Vec<BrokerWatchlist>, FetchError> {
    let (status, data) = f.get(PATH_WATCHLISTS, &[])?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    parse_watchlists(&data)
}

/// One symbol of a broker watchlist.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct BrokerInstrument {
    pub symbol: String,
    pub name: Option<String>,
    pub exchange_code: Option<String>,
}

/// Parse the symbols of one list; rows without a symbol are skipped. A
/// non-empty array that loses every row is a schema mismatch, not an empty
/// list.
pub fn parse_instruments(data: &Value) -> Result<Vec<BrokerInstrument>, FetchError> {
    let rows = rows_of(data, "instruments", "watchlist instruments")?;
    let mut instruments = Vec::new();
    let mut skipped = 0usize;
    for row in rows {
        match opt_str(row.get("symbol")) {
            Some(symbol) => instruments.push(BrokerInstrument {
                symbol,
                name: opt_str(row.get("name")),
                exchange_code: opt_str(row.get("exchange_code")),
            }),
            None => skipped += 1,
        }
    }
    if !rows.is_empty() && skipped == rows.len() {
        return Err(FetchError::Unexpected(
            "watchlist instruments: every row is missing symbol".into(),
        ));
    }
    Ok(instruments)
}

/// The symbols of one watchlist, in the broker's order.
pub fn fetch_watchlist_instruments<F: HttpFetch>(
    f: &mut F,
    watchlist_id: &str,
) -> Result<Vec<BrokerInstrument>, FetchError> {
    let (status, data) = f.get(
        PATH_WATCHLIST_INSTRUMENTS,
        &[("watchlist_id".into(), watchlist_id.to_string())],
    )?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    parse_instruments(&data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webull::rest::FailureKind;
    use serde_json::json;

    /// Field names as the Thailand host returned them on 2026-11-23 and
    /// 2026-11-24 (values replaced).
    fn list_body() -> Value {
        json!([
            {"create_time": "2026-10-02T03:04:05.000+0000", "name": "Second", "sort": 2,
             "update_time": "2026-11-20T03:04:05.000+0000", "watchlist_id": "102"},
            {"create_time": "2026-10-01T03:04:05.000+0000", "name": "First", "sort": 1,
             "update_time": "2026-11-21T03:04:05.000+0000", "watchlist_id": "101"},
            {"name": "No id"},
        ])
    }

    #[test]
    fn lists_are_sorted_by_the_broker_sort_and_keep_their_id() {
        let lists = parse_watchlists(&list_body()).unwrap();
        assert_eq!(
            lists
                .iter()
                .map(|list| list.id.as_str())
                .collect::<Vec<_>>(),
            ["101", "102"]
        );
        assert_eq!(lists[0].name, "First");
        assert_eq!(
            lists[0].update_time.as_deref(),
            Some("2026-11-21T03:04:05.000+0000")
        );
    }

    #[test]
    fn a_missing_name_and_a_numeric_id_are_tolerated() {
        let lists = parse_watchlists(&json!([{"watchlist_id": 7}, {"watchlist_id": ""}])).unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].id, "7");
        assert_eq!(lists[0].name, "ไม่มีชื่อ");
        assert!(parse_watchlists(&json!([])).unwrap().is_empty());
        assert!(parse_watchlists(&json!({"data": []})).unwrap().is_empty());
    }

    #[test]
    fn instruments_keep_symbol_name_and_exchange_and_skip_rows_without_a_symbol() {
        // the envelope the host really sends: the rows sit under `instruments`
        let body = json!({
            "watchlist_id": "101",
            "instruments": [
                {"added_time": "2026-11-01T03:04:05.000+0000", "exchange_code": "NAS",
                 "instrument_id": "913257561", "name": "NVIDIA Corp", "sort": 0, "symbol": "NVDA"},
                {"name": "no symbol", "sort": 1}
            ],
        });
        assert_eq!(
            parse_instruments(&body).unwrap(),
            [BrokerInstrument {
                symbol: "NVDA".into(),
                name: Some("NVIDIA Corp".into()),
                exchange_code: Some("NAS".into()),
            }]
        );
        assert!(
            parse_instruments(&json!({"instruments": []}))
                .unwrap()
                .is_empty()
        );
        assert!(parse_instruments(&json!([])).unwrap().is_empty());
    }

    /// The first in-app check showed an empty list for a list that holds
    /// symbols: the rows were under a key the generic list reader did not
    /// know. An unknown shape must surface, not pass for "no data".
    #[test]
    fn an_unknown_envelope_is_a_schema_mismatch_not_an_empty_list() {
        for body in [
            json!({"data": {"rows": [{"symbol": "AAPL"}]}}),
            json!({"result": [{"symbol": "AAPL"}]}),
            json!("ok"),
            json!(null),
        ] {
            let error = parse_instruments(&body).unwrap_err();
            assert!(matches!(error, FetchError::Unexpected(_)), "{body}");
        }
        let error = parse_watchlists(&json!({"items": []})).unwrap_err();
        assert!(
            matches!(&error, FetchError::Unexpected(what) if what.contains("items")),
            "names the fields it met: {error}"
        );
        assert_eq!(error.kind(), FailureKind::Schema);
        assert!(!error.is_retriable());
        assert!(error.summary().starts_with("HTTP 200 · SCHEMA_MISMATCH"));
        assert!(error.user_message().contains("รูปแบบข้อมูลไม่ตรง"));
    }

    /// A renamed field used to surface as "empty": when a non-empty array
    /// loses every row to a missing required field, that is a schema
    /// mismatch, not a list with nothing in it.
    #[test]
    fn every_row_losing_its_required_field_is_a_schema_mismatch() {
        let error = parse_instruments(&json!({"instruments": [{"ticker": "AAPL"}]})).unwrap_err();
        assert!(
            matches!(&error, FetchError::Unexpected(what) if what.contains("symbol")),
            "names the missing field: {error}"
        );
        let error = parse_watchlists(&json!([{"name": "id field renamed"}])).unwrap_err();
        assert!(
            matches!(&error, FetchError::Unexpected(what) if what.contains("watchlist_id")),
            "names the missing field: {error}"
        );
    }

    /// One queued answer: the call has to hit `path` with exactly `params`.
    struct Answer {
        path: &'static str,
        params: Vec<(&'static str, &'static str)>,
        status: u16,
        body: Value,
    }

    /// Answers queued in call order.
    struct QueueFetch(Vec<Answer>);

    impl HttpFetch for QueueFetch {
        fn get(
            &mut self,
            path: &str,
            params: &[(String, String)],
        ) -> Result<(u16, Value), FetchError> {
            let answer = self.0.remove(0);
            assert_eq!(path, answer.path);
            let got: Vec<(&str, &str)> = params
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect();
            assert_eq!(got, answer.params);
            Ok((answer.status, answer.body))
        }
    }

    #[test]
    fn the_two_routes_carry_no_write_parameters() {
        let mut fetch = QueueFetch(vec![
            Answer {
                path: PATH_WATCHLISTS,
                params: vec![],
                status: 200,
                body: list_body(),
            },
            Answer {
                path: PATH_WATCHLIST_INSTRUMENTS,
                params: vec![("watchlist_id", "101")],
                status: 200,
                body: json!({"instruments": [{"symbol": "AAPL", "name": "Apple"}]}),
            },
        ]);
        assert_eq!(fetch_watchlists(&mut fetch).unwrap().len(), 2);
        let instruments = fetch_watchlist_instruments(&mut fetch, "101").unwrap();
        assert_eq!(instruments[0].symbol, "AAPL");
        assert!(fetch.0.is_empty(), "exactly one request per call");
    }

    #[test]
    fn a_rejected_call_is_an_error_not_an_empty_list() {
        let mut fetch = QueueFetch(vec![Answer {
            path: PATH_WATCHLISTS,
            params: vec![],
            status: 403,
            body: json!({"error_code": "FORBIDDEN", "message": "no"}),
        }]);
        assert!(matches!(
            fetch_watchlists(&mut fetch),
            Err(FetchError::NonZero(403, _))
        ));
    }
}
