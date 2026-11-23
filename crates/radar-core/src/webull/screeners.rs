//! Market discovery: the broker's screener ranks (reference family
//! "Screeners" — all six lists live-verified 2026-11-23). A discovered symbol
//! feeds the existing scanner; nothing here trades.

use serde_json::Value;

use super::rest::{FetchError, HttpFetch, extract_list, opt_f64, opt_str};

/// GET `/market-data/screeners/gainers-losers/list`
pub const PATH_GAINERS_LOSERS: &str = "/market-data/screeners/gainers-losers/list";
/// GET `/market-data/screeners/top-actives/list`
pub const PATH_TOP_ACTIVES: &str = "/market-data/screeners/top-actives/list";
/// GET `/market-data/screeners/week52-high-low/list`
pub const PATH_WEEK_52: &str = "/market-data/screeners/week52-high-low/list";
/// GET `/market-data/screeners/high-dividend-ranks/list`
pub const PATH_HIGH_DIVIDEND: &str = "/market-data/screeners/high-dividend-ranks/list";
/// GET `/market-data/screeners/market-sectors/list`
pub const PATH_SECTORS: &str = "/market-data/screeners/market-sectors/list";
/// GET `/market-data/screeners/market-sectors/get`
pub const PATH_SECTOR_DETAIL: &str = "/market-data/screeners/market-sectors/get";

/// The screener lists the UI offers. Serialized in snake_case for commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoverList {
    Gainers,
    Losers,
    MostActive,
    Week52High,
    Week52Low,
    HighDividend,
}

impl MoverList {
    pub const ALL: [MoverList; 6] = [
        MoverList::Gainers,
        MoverList::Losers,
        MoverList::MostActive,
        MoverList::Week52High,
        MoverList::Week52Low,
        MoverList::HighDividend,
    ];

    /// Endpoint plus the documented query for this list.
    fn request(self) -> (&'static str, Vec<(&'static str, &'static str)>) {
        use MoverList::*;
        match self {
            Gainers => (
                PATH_GAINERS_LOSERS,
                vec![
                    ("rank_type", "DAY_1"),
                    ("category", "US_STOCK"),
                    ("sort_by", "CHANGE_RATIO"),
                    ("direction", "DESC"),
                ],
            ),
            Losers => (
                PATH_GAINERS_LOSERS,
                vec![
                    ("rank_type", "DAY_1"),
                    ("category", "US_STOCK"),
                    ("sort_by", "CHANGE_RATIO"),
                    ("direction", "ASC"),
                ],
            ),
            MostActive => (
                PATH_TOP_ACTIVES,
                vec![
                    ("category", "US_STOCK"),
                    ("rank_type", "VOLUME"),
                    ("sort_by", "VOLUME"),
                    ("direction", "DESC"),
                ],
            ),
            Week52High => (
                PATH_WEEK_52,
                vec![("rank_type", "NEW_HIGH"), ("category", "US_STOCK")],
            ),
            Week52Low => (
                PATH_WEEK_52,
                vec![("rank_type", "NEW_LOW"), ("category", "US_STOCK")],
            ),
            HighDividend => (
                PATH_HIGH_DIVIDEND,
                vec![
                    ("category", "US_STOCK"),
                    ("sort_by", "YIELD"),
                    ("direction", "DESC"),
                ],
            ),
        }
    }
}

/// One ranked stock. Every number is optional — a list only carries the
/// columns it ranks by, and a blank stays `None`.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct MoverRow {
    pub symbol: String,
    pub name: Option<String>,
    /// Last price as the screener reports it.
    pub price: Option<f64>,
    /// Change of the ranking period as a fraction (1.449709 = +144.97 %).
    pub change_ratio: Option<f64>,
    pub volume: Option<f64>,
    pub market_value: Option<f64>,
    pub pe_ttm: Option<f64>,
    pub relative_volume_10d: Option<f64>,
    /// Dividend yield in percent as the screener reports it (9.539 = 9.54 %).
    pub dividend_yield: Option<f64>,
    pub exchange_code: Option<String>,
    /// Broker timestamp of the last trade, ISO text as returned.
    pub last_trade_time: Option<String>,
}

/// Parse one ranked row; rows without a symbol are skipped.
pub fn parse_mover_row(row: &Value) -> Option<MoverRow> {
    let symbol = opt_str(row.get("symbol"))?;
    Some(MoverRow {
        symbol,
        name: opt_str(row.get("name")),
        price: opt_f64(row.get("price")).or_else(|| opt_f64(row.get("close"))),
        change_ratio: opt_f64(row.get("change_ratio")),
        volume: opt_f64(row.get("volume")),
        market_value: opt_f64(row.get("market_value")),
        pe_ttm: opt_f64(row.get("pe_ttm")),
        relative_volume_10d: opt_f64(row.get("relative_volume_10d")),
        dividend_yield: opt_f64(row.get("yield")),
        exchange_code: opt_str(row.get("exchange_code")),
        last_trade_time: opt_str(row.get("last_trade_time")),
    })
}

/// Parse a screener list (bare array or `{data: [...]}` envelope).
pub fn parse_movers(data: &Value) -> Vec<MoverRow> {
    extract_list(data)
        .iter()
        .filter_map(parse_mover_row)
        .collect()
}

/// One screener list, in the broker's ranking order.
pub fn fetch_movers<F: HttpFetch>(f: &mut F, list: MoverList) -> Result<Vec<MoverRow>, FetchError> {
    let (path, params) = list.request();
    let params: Vec<(String, String)> = params
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    let (status, data) = f.get(path, &params)?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    Ok(parse_movers(&data))
}

/// One market sector with its breadth counters and leading stock.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct SectorRow {
    pub id: String,
    pub name: Option<String>,
    /// Day change of the sector as a fraction.
    pub change_ratio: Option<f64>,
    pub advanced: Option<f64>,
    pub declined: Option<f64>,
    pub flat: Option<f64>,
    pub volume: Option<f64>,
    pub market_value: Option<f64>,
    /// The sector's leading stock as the screener lists it.
    pub leader: Option<MoverRow>,
}

/// Parse one sector row; the `id` arrives as a string but is tolerated as a number.
pub fn parse_sector_row(row: &Value) -> Option<SectorRow> {
    let id = match row.get("id") {
        Some(Value::String(id)) => id.clone(),
        Some(Value::Number(id)) => id.to_string(),
        _ => return None,
    };
    Some(SectorRow {
        id,
        name: opt_str(row.get("name")),
        change_ratio: opt_f64(row.get("change_ratio")),
        advanced: opt_f64(row.get("advanced")),
        declined: opt_f64(row.get("declined")),
        flat: opt_f64(row.get("flat")),
        volume: opt_f64(row.get("volume")),
        market_value: opt_f64(row.get("market_value")),
        leader: row
            .get("data")
            .and_then(Value::as_array)
            .and_then(|leaders| leaders.first())
            .and_then(parse_mover_row),
    })
}

/// All sectors for the last session, largest market value first.
pub fn fetch_sectors<F: HttpFetch>(f: &mut F) -> Result<Vec<SectorRow>, FetchError> {
    let (status, data) = f.get(
        PATH_SECTORS,
        &[
            ("category".into(), "US_STOCK".into()),
            ("agg_type".into(), "MARKET_VALUE".into()),
            ("period".into(), "D1".into()),
            ("direction".into(), "DESC".into()),
        ],
    )?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    Ok(extract_list(&data)
        .iter()
        .filter_map(parse_sector_row)
        .collect())
}

/// The stocks of one sector, ranked by the sector's change column.
pub fn fetch_sector_detail<F: HttpFetch>(
    f: &mut F,
    sector_id: &str,
) -> Result<Vec<MoverRow>, FetchError> {
    let (status, data) = f.get(
        PATH_SECTOR_DETAIL,
        &[
            ("sector_id".into(), sector_id.to_string()),
            ("category".into(), "US_STOCK".into()),
            ("period".into(), "D1".into()),
        ],
    )?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    Ok(parse_movers(&data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Rows exactly as the Thailand host returned them on 2026-11-23.
    fn gainer_row() -> Value {
        json!({
            "amplitude": "1.647295", "change": "0.3243", "change_ratio": "1.449709",
            "close": "0.548", "currency": "USD", "currency_code": "USD",
            "exchange_code": "NAS", "high": "0.82", "instrument_id": "950983869",
            "last_trade_time": "2026-11-23T11:05:33.934+0000", "low": "0.4515",
            "market_value": "1773638.716", "name": "Nexus Advanced Technologies",
            "open": "0.6439", "pe_ttm": "-0.078283", "pre_close": "0.548", "price": "0.548",
            "relative_volume_10d": "0.5421", "symbol": "NXAT", "turnover_rate": "108.706495",
            "volume": "184732",
        })
    }

    #[test]
    fn mover_row_keeps_the_ranked_columns_and_blanks_the_rest() {
        let row = parse_mover_row(&gainer_row()).unwrap();
        assert_eq!(row.symbol, "NXAT");
        assert_eq!(row.name.as_deref(), Some("Nexus Advanced Technologies"));
        assert_eq!(row.price, Some(0.548));
        assert_eq!(row.change_ratio, Some(1.449709));
        assert_eq!(row.volume, Some(184_732.0));
        assert_eq!(row.pe_ttm, Some(-0.078283));
        // the gainers list carries no dividend column
        assert_eq!(row.dividend_yield, None);

        let dividend = parse_mover_row(&json!({
            "symbol": "GMEX", "price": "0.1920", "yield": "9.5390", "ex_date": "2026-09-28",
        }))
        .unwrap();
        assert_eq!(dividend.dividend_yield, Some(9.539));
        assert_eq!(dividend.change_ratio, None);
        assert_eq!(parse_mover_row(&json!({"price": "1"})), None);
        // `price` missing → the close stands in
        let close_only = parse_mover_row(&json!({"symbol": "X", "close": "2.5"})).unwrap();
        assert_eq!(close_only.price, Some(2.5));
    }

    #[test]
    fn every_list_targets_its_documented_endpoint() {
        for list in MoverList::ALL {
            let (path, params) = list.request();
            let value = |key: &str| params.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
            assert_eq!(value("category"), Some("US_STOCK"), "{list:?}");
            match list {
                MoverList::Gainers => {
                    assert_eq!(path, PATH_GAINERS_LOSERS);
                    assert_eq!(value("rank_type"), Some("DAY_1"));
                    assert_eq!(value("direction"), Some("DESC"));
                }
                MoverList::Losers => assert_eq!(value("direction"), Some("ASC")),
                MoverList::MostActive => assert_eq!(path, PATH_TOP_ACTIVES),
                MoverList::Week52High => {
                    assert_eq!(path, PATH_WEEK_52);
                    assert_eq!(value("rank_type"), Some("NEW_HIGH"));
                }
                MoverList::Week52Low => assert_eq!(value("rank_type"), Some("NEW_LOW")),
                MoverList::HighDividend => assert_eq!(path, PATH_HIGH_DIVIDEND),
            }
        }
    }

    #[test]
    fn movers_parse_bare_array_and_data_envelope() {
        assert_eq!(
            parse_movers(&json!([gainer_row(), {"symbol": ""}])).len(),
            1
        );
        assert_eq!(parse_movers(&json!({"data": [gainer_row()]})).len(), 1);
        assert!(parse_movers(&json!({"data": {"data": []}})).is_empty());
    }

    #[test]
    fn sector_row_carries_breadth_and_its_leading_stock() {
        let sector = parse_sector_row(&json!({
            "advanced": "21", "change_ratio": "-0.0393", "data": [{"symbol": "MSFT",
                "name": "Microsoft", "price": "400"}],
            "declined": "146", "flat": "427", "id": "7324",
            "market_value": "19975263748096", "name": "Software & IT Services",
            "volume": "397042095",
        }))
        .unwrap();
        assert_eq!(sector.id, "7324");
        assert_eq!(sector.name.as_deref(), Some("Software & IT Services"));
        assert_eq!(sector.change_ratio, Some(-0.0393));
        assert_eq!(sector.advanced, Some(21.0));
        assert_eq!(sector.declined, Some(146.0));
        assert_eq!(sector.leader.as_ref().unwrap().symbol, "MSFT");
        // a numeric id and a sector without a listed leader both parse
        assert_eq!(parse_sector_row(&json!({"id": 6391})).unwrap().id, "6391");
        assert!(parse_sector_row(&json!({"name": "no id"})).is_none());
    }

    /// Records the request it was asked for and answers with one row.
    #[derive(Default)]
    struct RecordingFetch {
        path: String,
        params: Vec<(String, String)>,
        body: Value,
    }

    impl HttpFetch for RecordingFetch {
        fn get(
            &mut self,
            path: &str,
            params: &[(String, String)],
        ) -> Result<(u16, Value), FetchError> {
            self.path = path.to_string();
            self.params = params.to_vec();
            Ok((200, self.body.clone()))
        }
    }

    #[test]
    fn sector_calls_send_the_documented_parameters() {
        let mut f = RecordingFetch {
            body: json!({"data": [{"id": "7324", "name": "Software"}]}),
            ..Default::default()
        };
        assert_eq!(fetch_sectors(&mut f).unwrap().len(), 1);
        assert_eq!(f.path, PATH_SECTORS);
        assert!(f.params.contains(&("period".to_string(), "D1".to_string())));

        f.body = json!([gainer_row()]);
        assert_eq!(fetch_sector_detail(&mut f, "7324").unwrap().len(), 1);
        assert_eq!(f.path, PATH_SECTOR_DETAIL);
        assert_eq!(
            f.params.first(),
            Some(&("sector_id".to_string(), "7324".to_string()))
        );

        let mut f = RecordingFetch {
            body: json!([gainer_row()]),
            ..Default::default()
        };
        assert_eq!(fetch_movers(&mut f, MoverList::Losers).unwrap().len(), 1);
        assert_eq!(f.path, PATH_GAINERS_LOSERS);
        assert!(
            f.params
                .contains(&("direction".to_string(), "ASC".to_string()))
        );
    }

    #[test]
    fn broker_errors_surface_as_non_zero() {
        let mut f = QueueFail;
        let error = fetch_movers(&mut f, MoverList::Gainers).unwrap_err();
        assert!(matches!(error, FetchError::NonZero(403, _)));
        assert_eq!(error.summary(), "HTTP 403");
    }

    struct QueueFail;

    impl HttpFetch for QueueFail {
        fn get(
            &mut self,
            _path: &str,
            _params: &[(String, String)],
        ) -> Result<(u16, Value), FetchError> {
            Err(FetchError::NonZero(403, "forbidden".into()))
        }
    }
}
