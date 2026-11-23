//! Company facts an option seller needs around a position: what the company
//! is, when it next reports earnings, when the stock goes ex-dividend and
//! where analysts stand. Reference family "Fundamentals" (live-verified
//! 2026-11-23).
//!
//! Every family is optional by design: an ETF has no earnings, a foreign
//! issuer may lack analyst coverage. One family failing degrades to `None`
//! and is reported in [`StockOverview::unavailable`] — it never aborts the
//! rest.

use serde_json::Value;

use super::quotes::ny_today;
use super::rest::{FailureKind, FetchError, HttpFetch, extract_list, opt_f64, opt_i64, opt_str};
use chrono::NaiveDate;

/// GET `/market-data/fundamentals/company-profiles/get`
pub const PATH_COMPANY_PROFILE: &str = "/market-data/fundamentals/company-profiles/get";
/// GET `/market-data/fundamentals/analysis/target-prices/get`
pub const PATH_ANALYST_TARGETS: &str = "/market-data/fundamentals/analysis/target-prices/get";
/// GET `/market-data/fundamentals/analysis/ratings/get`
pub const PATH_ANALYST_RATINGS: &str = "/market-data/fundamentals/analysis/ratings/get";
/// GET `/market-data/fundamentals/financial-alerts/get`
pub const PATH_FINANCIAL_ALERT: &str = "/market-data/fundamentals/financial-alerts/get";
/// GET `/market-data/fundamentals/dividend-calendars/list`
pub const PATH_DIVIDEND_CALENDAR: &str = "/market-data/fundamentals/dividend-calendars/list";

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct CompanyProfile {
    pub company_name: String,
    pub industries: Vec<String>,
    pub profile: Option<String>,
    pub exhibition_code: Option<String>,
    pub ceo: Option<String>,
    pub employees: Option<f64>,
    pub establish_date: Option<String>,
    pub address: Option<String>,
}

/// Where analysts see the stock; `None` fields mean no consensus value.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct AnalystTarget {
    pub mean: Option<f64>,
    pub low: Option<f64>,
    pub high: Option<f64>,
    pub median: Option<f64>,
    pub currency: Option<String>,
}

/// Analyst recommendation counts.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct AnalystRating {
    /// Number of analysts covered.
    pub number: Option<f64>,
    pub strong_buy: Option<f64>,
    pub buy: Option<f64>,
    pub hold: Option<f64>,
    pub sell: Option<f64>,
    pub under_perform: Option<f64>,
}

impl AnalystRating {
    /// Share of buy-side opinions (strong buy + buy) of all ratings, in percent.
    pub fn bullish_pct(&self) -> Option<f64> {
        let bullish = self.strong_buy? + self.buy?;
        let total = self.number?;
        (total > 0.0).then(|| bullish / total * 100.0)
    }
}

/// The broker's next reporting window — the date an option seller plans
/// around. The broker gives a range until the date is confirmed.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct EarningsWindow {
    pub start_date: String,
    pub end_date: String,
    pub fiscal_year: Option<i64>,
    pub fiscal_period: Option<i64>,
    pub eps_est: Option<f64>,
    pub rev_est: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct DividendEvent {
    pub amount: Option<f64>,
    pub currency: Option<String>,
    pub div_type: Option<String>,
    pub ex_div_date: Option<String>,
    pub record_date: Option<String>,
    pub pay_date: Option<String>,
}

/// The dividend calendar split around "today": what is still ahead of the
/// seller and what was last paid.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct DividendCalendar {
    /// Upcoming (and unconfirmed) distributions, soonest ex-date first.
    pub upcoming: Vec<DividendEvent>,
    /// Past distributions, latest ex-date last.
    pub history: Vec<DividendEvent>,
}

impl DividendCalendar {
    /// The next ex-dividend date on or after `today`.
    pub fn next_ex_div(&self) -> Option<&DividendEvent> {
        self.upcoming.first()
    }
    /// The most recent distribution that already went ex.
    pub fn last_paid(&self) -> Option<&DividendEvent> {
        self.history.last()
    }
}

/// Why a family is missing from an overview.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Unavailable {
    pub section: &'static str,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct StockOverview {
    pub profile: Option<CompanyProfile>,
    pub analyst_target: Option<AnalystTarget>,
    pub analyst_rating: Option<AnalystRating>,
    pub next_earnings: Option<EarningsWindow>,
    pub dividends: Option<DividendCalendar>,
    pub unavailable: Vec<Unavailable>,
}

// ---------- parsers (pure) ----------

pub fn parse_company_profile(data: &Value) -> Option<CompanyProfile> {
    let row = data.as_object()?;
    Some(CompanyProfile {
        company_name: opt_str(row.get("company_name"))?,
        industries: row
            .get("industries")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default(),
        profile: opt_str(row.get("profile")),
        exhibition_code: opt_str(row.get("exhibition_code")),
        ceo: opt_str(row.get("ceo")),
        employees: opt_f64(row.get("employees")),
        establish_date: opt_str(row.get("establish_date")),
        address: opt_str(row.get("address")),
    })
}

/// `None` when no consensus number exists (an envelope echoing only the symbol).
pub fn parse_analyst_target(data: &Value) -> Option<AnalystTarget> {
    let row = data.as_object()?;
    let target = AnalystTarget {
        mean: opt_f64(row.get("mean")),
        low: opt_f64(row.get("low")),
        high: opt_f64(row.get("high")),
        median: opt_f64(row.get("median")),
        currency: opt_str(row.get("currency")),
    };
    let has_consensus = [target.mean, target.low, target.high, target.median]
        .iter()
        .any(Option::is_some);
    has_consensus.then_some(target)
}

/// `None` when no analyst counts exist.
pub fn parse_analyst_rating(data: &Value) -> Option<AnalystRating> {
    let row = data.as_object()?;
    let rating = AnalystRating {
        number: opt_f64(row.get("number")),
        strong_buy: opt_f64(row.get("strong_buy")),
        buy: opt_f64(row.get("buy")),
        hold: opt_f64(row.get("hold")),
        sell: opt_f64(row.get("sell")),
        under_perform: opt_f64(row.get("under_perform")),
    };
    let has_counts = [
        rating.number,
        rating.strong_buy,
        rating.buy,
        rating.hold,
        rating.sell,
    ]
    .iter()
    .any(Option::is_some);
    has_counts.then_some(rating)
}

/// `None` until the broker publishes a reporting window.
pub fn parse_earnings_window(data: &Value) -> Option<EarningsWindow> {
    let row = data.as_object()?;
    let start_date = opt_str(row.get("start_date"))?;
    Some(EarningsWindow {
        end_date: opt_str(row.get("end_date")).unwrap_or_else(|| start_date.clone()),
        start_date,
        fiscal_year: opt_i64(row.get("fiscal_year")),
        fiscal_period: opt_i64(row.get("fiscal_period")),
        eps_est: opt_f64(row.get("eps_est")),
        rev_est: opt_f64(row.get("rev_est")),
    })
}

fn parse_dividend_event(row: &Value) -> Option<DividendEvent> {
    let ex_div_date = opt_str(row.get("ex_div_date"))?;
    Some(DividendEvent {
        ex_div_date: Some(ex_div_date),
        amount: opt_f64(row.get("amount")),
        currency: opt_str(row.get("currency")),
        div_type: opt_str(row.get("div_type")),
        record_date: opt_str(row.get("record_date")),
        pay_date: opt_str(row.get("pay_date")),
    })
}

/// Split a dividend calendar around `today` (ISO dates compare as strings).
pub fn parse_dividend_calendar(data: &Value, today: NaiveDate) -> Option<DividendCalendar> {
    let mut events: Vec<DividendEvent> = extract_list(data)
        .iter()
        .filter_map(parse_dividend_event)
        .collect();
    events.sort_by(|a, b| a.ex_div_date.cmp(&b.ex_div_date));
    let today_text = today.to_string();
    let split = events
        .iter()
        .position(|event| event.ex_div_date.as_deref() >= Some(today_text.as_str()))
        .unwrap_or(events.len());
    let upcoming = events.split_off(split);
    Some(DividendCalendar {
        upcoming,
        history: events,
    })
}

// ---------- fetch ----------

fn get_object<F: HttpFetch>(f: &mut F, path: &str, symbol: &str) -> Result<Value, FetchError> {
    let (status, data) = f.get(
        path,
        &[
            ("symbol".into(), symbol.to_string()),
            ("category".into(), "US_STOCK".into()),
        ],
    )?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    Ok(data)
}

fn missing(unavailable: &mut Vec<Unavailable>, section: &'static str, reason: &str) {
    unavailable.push(Unavailable {
        section,
        reason: reason.to_string(),
    });
}

/// A rejected credential fails every family the same way — stop asking.
fn fatal(error: &FetchError) -> bool {
    matches!(error.kind(), FailureKind::Auth | FailureKind::Token)
}

/// One family's parsed value, so the loop below can treat them uniformly.
enum Family {
    Profile(CompanyProfile),
    Target(AnalystTarget),
    Rating(AnalystRating),
    Earnings(EarningsWindow),
    Dividends(DividendCalendar),
}

/// (endpoint, section label, parser) — one fundamentals family.
type FamilyCall = (
    &'static str,
    &'static str,
    fn(&Value, NaiveDate) -> Option<Family>,
);

/// Five paced calls, one per family. Nothing here can fail as a whole: each
/// family either lands or is listed in `unavailable`.
pub fn fetch_stock_overview<F: HttpFetch>(f: &mut F, symbol: &str) -> StockOverview {
    let mut overview = StockOverview::default();
    let today = ny_today();
    let families: [FamilyCall; 5] = [
        (PATH_COMPANY_PROFILE, "company profile", |data, _| {
            parse_company_profile(data).map(Family::Profile)
        }),
        (PATH_ANALYST_TARGETS, "analyst target", |data, _| {
            parse_analyst_target(data).map(Family::Target)
        }),
        (PATH_ANALYST_RATINGS, "analyst rating", |data, _| {
            parse_analyst_rating(data).map(Family::Rating)
        }),
        (PATH_FINANCIAL_ALERT, "earnings date", |data, _| {
            parse_earnings_window(data).map(Family::Earnings)
        }),
        (
            PATH_DIVIDEND_CALENDAR,
            "dividend calendar",
            |data, today| parse_dividend_calendar(data, today).map(Family::Dividends),
        ),
    ];
    let mut stopped: Option<String> = None;
    for (path, section, parse) in families {
        if let Some(reason) = &stopped {
            missing(&mut overview.unavailable, section, reason);
            continue;
        }
        match get_object(f, path, symbol).map(|data| parse(&data, today)) {
            Ok(Some(Family::Profile(value))) => overview.profile = Some(value),
            Ok(Some(Family::Target(value))) => overview.analyst_target = Some(value),
            Ok(Some(Family::Rating(value))) => overview.analyst_rating = Some(value),
            Ok(Some(Family::Earnings(value))) => overview.next_earnings = Some(value),
            Ok(Some(Family::Dividends(value))) => overview.dividends = Some(value),
            Ok(None) => missing(&mut overview.unavailable, section, "no data"),
            Err(error) => {
                if fatal(&error) {
                    stopped = Some(error.summary());
                }
                missing(&mut overview.unavailable, section, &error.summary());
            }
        }
    }
    overview
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::VecDeque;

    /// Bodies exactly as the Thailand host returned them for NVDA on 2026-11-23.
    fn profile_body() -> Value {
        json!({
            "address": "2788 San Tomas Expressway,SANTA CLARA,CA,United States",
            "category": "US_STOCK", "ceo": "Colette M. Kress", "company_name": "NVIDIA Corp",
            "employees": "42000", "establish_date": "1998-02-24", "exhibition_code": "NASDAQ",
            "industries": ["Semiconductors", "Technology"], "profile": "NVIDIA Corporation is an …",
            "symbol": "NVDA",
        })
    }

    #[test]
    fn parses_live_bodies() {
        let profile = parse_company_profile(&profile_body()).unwrap();
        assert_eq!(profile.company_name, "NVIDIA Corp");
        assert_eq!(profile.industries.len(), 2);
        assert_eq!(profile.employees, Some(42000.0));
        assert_eq!(profile.establish_date.as_deref(), Some("1998-02-24"));

        let target = parse_analyst_target(&json!({
            "category": "US_STOCK", "currency": "USD", "high": "425", "low": "130",
            "mean": "244.18286", "median": "235", "symbol": "NVDA",
        }))
        .unwrap();
        assert_eq!(target.mean, Some(244.18286));
        assert_eq!(target.currency.as_deref(), Some("USD"));

        let rating = parse_analyst_rating(&json!({
            "buy": "11", "hold": "16", "number": "63", "sell": "2", "strong_buy": "34",
            "under_perform": "0",
        }))
        .unwrap();
        assert_eq!(rating.bullish_pct(), Some(45.0 / 63.0 * 100.0));
        assert_eq!(AnalystRating::default().bullish_pct(), None);

        let earnings = parse_earnings_window(&json!({
            "currency": "USD", "end_date": "2027-03-01", "eps_est": "2.86275",
            "fiscal_period": 4, "fiscal_year": 2027, "start_date": "2027-02-23",
        }))
        .unwrap();
        assert_eq!(earnings.start_date, "2027-02-23");
        assert_eq!(earnings.end_date, "2027-03-01");
        assert_eq!(earnings.fiscal_year, Some(2027));
    }

    #[test]
    fn envelopes_without_a_consensus_are_no_data() {
        // the broker echoes the request even when it has no answer
        let echo = json!({"symbol": "NVDA", "category": "US_STOCK"});
        assert_eq!(parse_analyst_target(&echo), None);
        assert_eq!(parse_analyst_rating(&echo), None);
        assert_eq!(parse_earnings_window(&echo), None);
        assert_eq!(parse_company_profile(&echo), None);
        assert_eq!(parse_analyst_target(&json!(null)), None);
        assert_eq!(
            parse_dividend_calendar(&json!([]), ny_today()),
            Some(DividendCalendar::default())
        );
    }

    #[test]
    fn dividend_calendar_splits_around_today() {
        let rows = json!([
            {"amount": "0.25", "currency": "USD", "div_type": "CASH_DIVIDEND",
             "ex_div_date": "2026-12-03", "pay_date": "2026-12-26", "record_date": "2026-12-03"},
            {"amount": "0.01", "currency": "USD", "div_type": "CASH_DIVIDEND",
             "ex_div_date": "2026-06-04", "pay_date": "2026-06-26", "record_date": "2026-06-04"},
            {"amount": "0.01", "currency": "USD", "div_type": "CASH_DIVIDEND",
             "ex_div_date": "2026-09-04", "pay_date": "2026-09-26", "record_date": "2026-09-04"},
        ]);
        let today = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        let calendar = parse_dividend_calendar(&rows, today).unwrap();
        assert_eq!(calendar.history.len(), 2);
        assert_eq!(calendar.upcoming.len(), 1);
        assert_eq!(
            calendar.next_ex_div().unwrap().ex_div_date.as_deref(),
            Some("2026-12-03")
        );
        assert_eq!(
            calendar.last_paid().unwrap().ex_div_date.as_deref(),
            Some("2026-09-04")
        );
        // rows without an ex-date cannot be placed on the calendar
        assert!(
            parse_dividend_calendar(&json!([{"amount": "1"}]), today)
                .unwrap()
                .upcoming
                .is_empty()
        );
    }

    /// Pops one queued body per call and remembers the paths it was asked for.
    #[derive(Default)]
    struct QueueFetch {
        bodies: VecDeque<Result<Value, (u16, &'static str)>>,
        paths: Vec<String>,
    }

    impl HttpFetch for QueueFetch {
        fn get(
            &mut self,
            path: &str,
            _params: &[(String, String)],
        ) -> Result<(u16, Value), FetchError> {
            self.paths.push(path.to_string());
            match self.bodies.pop_front() {
                Some(Ok(body)) => Ok((200, body)),
                Some(Err((status, body))) => Err(FetchError::NonZero(status, body.to_string())),
                None => Err(FetchError::Network("queue empty".into())),
            }
        }
    }

    #[test]
    fn overview_degrades_per_family_and_reports_why() {
        let mut f = QueueFetch {
            bodies: VecDeque::from(vec![
                Ok(profile_body()),
                Ok(json!({"symbol": "NVDA", "category": "US_STOCK"})), // no analyst target
                Err((
                    403,
                    r#"{"error_code":"MARKET_DATA_NOT_SUBSCRIBED","message":"x"}"#,
                )),
                Ok(json!({"start_date": "2027-02-23", "end_date": "2027-03-01"})),
                Ok(json!([])),
            ]),
            paths: Vec::new(),
        };
        let overview = fetch_stock_overview(&mut f, "NVDA");
        assert_eq!(f.paths.len(), 5);
        assert_eq!(
            overview.profile.as_ref().unwrap().company_name,
            "NVIDIA Corp"
        );
        assert!(overview.analyst_target.is_none());
        assert!(overview.analyst_rating.is_none());
        assert_eq!(
            overview.next_earnings.as_ref().unwrap().start_date,
            "2027-02-23"
        );
        assert!(overview.dividends.is_some());
        assert_eq!(
            overview.unavailable,
            vec![
                Unavailable {
                    section: "analyst target",
                    reason: "no data".into()
                },
                Unavailable {
                    section: "analyst rating",
                    reason: "HTTP 403 · MARKET_DATA_NOT_SUBSCRIBED".into()
                },
            ]
        );
    }

    #[test]
    fn overview_stops_asking_after_a_rejected_token() {
        let mut f = QueueFetch {
            bodies: VecDeque::from(vec![Err((
                401,
                r#"{"error_code":"INVALID_TOKEN","message":"Header x-access-token is missing or invalid."}"#,
            ))]),
            paths: Vec::new(),
        };
        let overview = fetch_stock_overview(&mut f, "NVDA");
        // one failed call, then nothing else is sent
        assert_eq!(f.paths.len(), 1);
        assert_eq!(overview.unavailable.len(), 5);
        assert!(
            overview
                .unavailable
                .iter()
                .all(|u| u.reason == "HTTP 401 · INVALID_TOKEN")
        );
    }
}
