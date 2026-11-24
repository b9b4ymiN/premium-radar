//! Read-only Webull Thailand capability probe (`docs/WEBULL_TH_CAPABILITY_MATRIX.md`).
//!
//! Calls every documented read-only endpoint ONCE with the app's saved
//! credentials (settings + OS keyring, loaded in-process) and writes a
//! sanitized JSON report: HTTP status, provider error code, row counts and
//! field names. It never records secrets, tokens, account identifiers or the
//! user's own positions/balances — `sensitive` steps keep counts and field
//! names only. Order placement, watchlist writes and token creation are not
//! probed at all.
//!
//! Run from `src-tauri`:
//!   cargo run --example probe_capabilities [-- <group>...]
//! Groups: auth stocks options fundamentals funds screeners watchlist account
//!         orders token streaming sdk

use std::time::Instant;

use radar_core::webull::rest::{Credentials, FetchError, HttpFetch};
use radar_core::webull::sign::SignatureScheme;
use radar_core::webull::transport::WebullClient;
use serde_json::{Value, json};

const ROOT: &str = "NVDA";
const LEGACY_SNAPSHOT: &str = "/openapi/market-data/stock/snapshot";

/// (id, path, extra query params)
type Endpoint<'a> = (&'a str, &'a str, &'a [(&'a str, &'a str)]);

/// One request to verify. `scheme` overrides the client only for the auth
/// matrix; every other step runs on the scheme the matrix selects.
struct Step<'a> {
    id: &'a str,
    group: &'a str,
    method: &'a str,
    path: &'a str,
    params: Vec<(String, String)>,
    body: Option<Value>,
    scheme: Option<SignatureScheme>,
    /// Counts + field names only — the user's own account data.
    sensitive: bool,
}

impl<'a> Step<'a> {
    fn get(id: &'a str, group: &'a str, path: &'a str, params: &[(&str, &str)]) -> Self {
        Self {
            id,
            group,
            method: "GET",
            path,
            params: params
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            body: None,
            scheme: None,
            sensitive: false,
        }
    }

    fn post(id: &'a str, group: &'a str, path: &'a str, body: Value) -> Self {
        Self {
            method: "POST",
            body: Some(body),
            ..Self::get(id, group, path, &[])
        }
    }

    fn auth(mut self, scheme: SignatureScheme) -> Self {
        self.scheme = Some(scheme);
        self
    }

    fn sensitive(mut self) -> Self {
        self.sensitive = true;
        self
    }
}

/// `symbol` + `category=US_STOCK` + extras — the shape of every fundamentals call.
fn stock_step<'a>(
    id: &'a str,
    group: &'a str,
    path: &'a str,
    symbol: &str,
    extra: &[(&str, &str)],
) -> Step<'a> {
    let mut params: Vec<(String, String)> = vec![
        ("symbol".into(), symbol.into()),
        ("category".into(), "US_STOCK".into()),
    ];
    params.extend(
        extra
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string())),
    );
    Step {
        params,
        ..Step::get(id, group, path, &[])
    }
}

fn run_all(probe: &mut Probe, group: &str, endpoints: &[Endpoint<'_>]) {
    for &(id, path, extra) in endpoints {
        probe.run(Step::get(id, group, path, extra));
    }
}

fn scheme_name(scheme: SignatureScheme) -> &'static str {
    match scheme {
        SignatureScheme::V3 => "v3",
        SignatureScheme::V2 => "v2",
    }
}

struct Outcome {
    status: u16,
    body: Value,
}

struct Probe {
    creds: Credentials,
    scheme: SignatureScheme,
    log: Vec<Value>,
}

impl Probe {
    fn run(&mut self, step: Step<'_>) -> Outcome {
        let started = Instant::now();
        // A fresh, cache-less client per step: the scheme is a client setting
        // and no earlier response may answer for the wire.
        let mut client = WebullClient::new(self.creds.clone())
            .with_signature_scheme(step.scheme.unwrap_or(self.scheme))
            .without_cache();
        let path = step.path.to_string();
        let result = match &step.body {
            Some(body) => client.post_json(&path, &body.to_string()),
            None => client.get(&path, &step.params),
        };
        let ms = started.elapsed().as_millis();
        let outcome = match result {
            Ok((status, body)) => Outcome { status, body },
            Err(FetchError::NonZero(status, body)) => Outcome {
                status,
                body: serde_json::from_str(&body).unwrap_or_else(|_| json!({})),
            },
            Err(e) => Outcome {
                status: 0,
                body: json!({ "message": e.to_string() }),
            },
        };
        let (line, entry) = self.describe(&step, &outcome, ms);
        println!("{line}");
        self.log.push(entry);
        outcome
    }

    /// One console line + one report entry, both sanitized.
    fn describe(&self, step: &Step<'_>, outcome: &Outcome, ms: u128) -> (String, Value) {
        let scheme = step.scheme.unwrap_or(self.scheme);
        let code = outcome.body.get("error_code").and_then(Value::as_str);
        // a trading message can echo an account number — the code is enough;
        // any other message is clipped and scrubbed of long digit runs
        let message = if step.path.starts_with("/trading/") {
            None
        } else {
            outcome
                .body
                .get("message")
                .and_then(Value::as_str)
                .map(|m| redact_digits(&m.chars().take(220).collect::<String>()))
        };
        let rows = rows_of(&outcome.body);
        let row_count = rows.as_ref().map_or(0, Vec::len);
        let first = rows.as_ref().and_then(|r| r.first().copied());
        let keys: Vec<String> = first
            .and_then(Value::as_object)
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        // A 200 with no usable row is recorded as NO_DATA, never as proof.
        let verdict = if outcome.status == 200 && first.is_none_or(|row| !has_data(row)) {
            "NO_DATA"
        } else {
            verdict(outcome.status, code)
        };
        let status = outcome.status;
        let scheme_label = scheme_name(scheme);
        let path = step.path;
        let code_note = code.map(|c| format!(" code={c}")).unwrap_or_default();
        let message_note = message
            .as_ref()
            .map(|m| format!(" · {m}"))
            .unwrap_or_default();
        let line = format!(
            "[{ms:>5}ms] {status:>3} {} {path} {scheme_label} — {verdict} rows={row_count}{code_note}{message_note}",
            step.method,
        );
        let mut entry = json!({
            "id": step.id,
            "group": step.group,
            "method": step.method,
            "path": step.path,
            "scheme": scheme_name(scheme),
            "status": outcome.status,
            "ms": ms,
            "verdict": verdict,
            "error_code": code,
            "message": message,
            "rows": row_count,
            "keys": keys,
        });
        if !step.params.is_empty() {
            // values of sensitive steps identify the account — names only
            entry["params"] = if step.sensitive {
                json!(
                    step.params
                        .iter()
                        .map(|(k, _)| k.clone())
                        .collect::<Vec<_>>()
                )
            } else {
                json!(
                    step.params
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect::<Vec<_>>()
                )
            };
        }
        if let (Some(row), false, 200) = (first, step.sensitive, outcome.status) {
            entry["sample"] = clipped(row, 0);
            if row_count <= 5
                && let Some(rows) = &rows
            {
                // small responses (calendars, single objects) are kept whole
                entry["rows_small"] =
                    Value::Array(rows.iter().map(|row| clipped(row, 0)).collect());
            }
        }
        (line, entry)
    }
}

/// The row list of a response, trying the envelope shapes Webull uses.
fn rows_of(body: &Value) -> Option<Vec<&Value>> {
    match body {
        Value::Array(items) => Some(items.iter().collect()),
        Value::Object(map) => {
            for key in ["data", "result", "instruments", "values", "filings"] {
                if let Some(Value::Array(items)) = map.get(key) {
                    return Some(items.iter().collect());
                }
            }
            if let Some(Value::Array(items)) = map.get("data").and_then(|inner| inner.get("data")) {
                return Some(items.iter().collect());
            }
            Some(vec![body])
        }
        _ => None,
    }
}

/// Replace any run of 6 or more digits with `[digits]` — an account or order
/// number must never reach a console line or the report, while short codes
/// ("error 42") stay readable.
fn redact_digits(text: &str) -> String {
    fn emit(out: &mut String, run: &str) {
        if run.len() >= 6 {
            out.push_str("[digits]");
        } else {
            out.push_str(run);
        }
    }
    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            run.push(ch);
        } else {
            emit(&mut out, &run);
            run.clear();
            out.push(ch);
        }
    }
    emit(&mut out, &run);
    out
}

/// Any non-empty value at the top two levels of `row`.
fn has_data(row: &Value) -> bool {
    match row {
        Value::Null => false,
        Value::String(s) => !s.trim().is_empty(),
        Value::Array(items) => items.iter().any(has_data),
        Value::Object(map) => map.values().any(has_data),
        _ => true,
    }
}

/// Sample for the report: market data only (callers pass non-sensitive rows),
/// nested arrays reduced to their length, long strings clipped.
fn clipped(row: &Value, depth: usize) -> Value {
    match row {
        Value::Array(items) => json!(format!("[{} items]", items.len())),
        Value::Object(map) if depth < 2 => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), clipped(v, depth + 1)))
                .collect(),
        ),
        Value::String(s) if s.chars().count() > 80 => Value::String(s.chars().take(80).collect()),
        other => other.clone(),
    }
}

fn verdict(status: u16, code: Option<&str>) -> &'static str {
    match status {
        200 => "VERIFIED",
        401 | 403 => "ACCESS_DENIED",
        404 => "NOT_FOUND",
        429 => "RATE_LIMITED",
        0 => "ERROR",
        400..=499 => match code {
            Some(c)
                if [
                    "PERMISSION",
                    "SUBSCRIPTION",
                    "ENTITLE",
                    "FORBIDDEN",
                    "AUTHORIZ",
                ]
                .iter()
                .any(|needle| c.to_ascii_uppercase().contains(needle)) =>
            {
                "ACCESS_DENIED"
            }
            _ => "INVALID_REQUEST",
        },
        _ => "SERVER_ERROR",
    }
}

fn main() {
    let groups: Vec<String> = std::env::args().skip(1).collect();
    let wanted = |group: &str| groups.is_empty() || groups.iter().any(|g| g == group);

    let app_data = std::env::var("APPDATA")
        .map(|a| format!("{a}/premium-radar"))
        .unwrap_or_else(|_| ".".into());
    let mut settings = premium_radar_lib::settings::AppSettings::load(&app_data);
    settings.pull_secrets_from_keyring();
    let creds = settings
        .webull_credentials()
        .expect("no creds — run the app setup first");
    if creds.access_token.is_none() {
        eprintln!("no access token saved — production data endpoints will report 401");
    }
    println!("creds: {}", creds.masked());

    let mut probe = Probe {
        creds: creds.clone(),
        scheme: SignatureScheme::V3,
        log: Vec::new(),
    };
    let day = |offset: i64| {
        (chrono::Utc::now() + chrono::Duration::days(offset))
            .format("%Y-%m-%d")
            .to_string()
    };

    // ---- auth matrix: which signing scheme the TH host accepts (neither
    // variant ever sends the app secret as a header) ----
    // A different symbol per variant keeps the shared cache key distinct.
    let stock = |symbols: &str| {
        vec![
            ("symbols".to_string(), symbols.to_string()),
            ("category".to_string(), "US_STOCK".to_string()),
            ("extend_hour_required".to_string(), "false".to_string()),
            ("overnight_required".to_string(), "false".to_string()),
        ]
    };
    let ok = |o: &Outcome| {
        o.status == 200 && rows_of(&o.body).is_some_and(|rows| rows.iter().any(|r| has_data(r)))
    };
    if wanted("auth") {
        let v2 = probe.run(
            Step {
                params: stock("AAPL"),
                ..Step::get("auth.v2", "auth", LEGACY_SNAPSHOT, &[])
            }
            .auth(SignatureScheme::V2),
        );
        let v3 = probe.run(
            Step {
                params: stock("TSLA"),
                ..Step::get("auth.v3", "auth", LEGACY_SNAPSHOT, &[])
            }
            .auth(SignatureScheme::V3),
        );
        let v3_post = probe.run(
            Step::post(
                "auth.v3.post_body",
                "auth",
                "/market-data/stocks/bars/list",
                json!({
                    "symbols": ["AMD"], "category": "US_STOCK", "timespan": "D",
                    "count": 5, "real_time_required": false,
                }),
            )
            .auth(SignatureScheme::V3),
        );
        probe.scheme = if ok(&v3) && ok(&v3_post) {
            SignatureScheme::V3
        } else {
            SignatureScheme::V2
        };
        println!(
            "auth matrix → v2={} v3={} v3-post={} ⇒ rest of the probe uses {}",
            verdict(v2.status, None),
            verdict(v3.status, None),
            verdict(v3_post.status, None),
            scheme_name(probe.scheme),
        );
    }

    // ---- stocks ----
    if wanted("stocks") {
        run_all(
            &mut probe,
            "stocks",
            &[
                (
                    "stock.snapshot.extended",
                    "/market-data/stocks/snapshots/list",
                    &[
                        ("symbols", "NVDA,AAPL,MSFT"),
                        ("category", "US_STOCK"),
                        ("extend_hour_required", "true"),
                        ("overnight_required", "true"),
                    ],
                ),
                (
                    "stock.snapshot.etf",
                    "/market-data/stocks/snapshots/list",
                    &[
                        ("symbols", "SPY,QQQ"),
                        ("category", "US_ETF"),
                        ("extend_hour_required", "true"),
                        ("overnight_required", "true"),
                    ],
                ),
                (
                    "stock.snapshot.legacy",
                    LEGACY_SNAPSHOT,
                    &[
                        ("symbols", "NVDA"),
                        ("category", "US_STOCK"),
                        ("extend_hour_required", "true"),
                        ("overnight_required", "true"),
                    ],
                ),
                (
                    "stock.bars.single",
                    "/market-data/stocks/bars/get",
                    &[
                        ("symbol", "NVDA"),
                        ("category", "US_STOCK"),
                        ("timespan", "D"),
                        ("count", "10"),
                    ],
                ),
                (
                    "stock.tick",
                    "/market-data/stocks/ticks/list",
                    &[
                        ("symbol", "NVDA"),
                        ("category", "US_STOCK"),
                        ("count", "20"),
                        ("trading_sessions", "PRE,RTH"),
                    ],
                ),
                (
                    "stock.depth.l1",
                    "/market-data/stocks/depths/list",
                    &[
                        ("symbol", "NVDA"),
                        ("category", "US_STOCK"),
                        ("depth", "L1"),
                        ("overnight_required", "false"),
                    ],
                ),
                (
                    "stock.depth.1",
                    "/market-data/stocks/depths/list",
                    &[
                        ("symbol", "NVDA"),
                        ("category", "US_STOCK"),
                        ("depth", "1"),
                        ("overnight_required", "false"),
                    ],
                ),
                (
                    "stock.depth.5",
                    "/market-data/stocks/depths/list",
                    &[
                        ("symbol", "NVDA"),
                        ("category", "US_STOCK"),
                        ("depth", "5"),
                        ("overnight_required", "false"),
                    ],
                ),
                (
                    "stock.footprint",
                    "/market-data/stocks/footprints/list",
                    &[
                        ("symbols", "NVDA"),
                        ("category", "US_STOCK"),
                        ("timespan", "M1"),
                        ("count", "5"),
                        ("real_time_required", "false"),
                        ("trading_sessions", "RTH"),
                    ],
                ),
                (
                    "stock.instrument",
                    "/trading/instruments/stocks/profiles/list",
                    &[("category", "US_STOCK"), ("symbols", "NVDA,SPY")],
                ),
            ],
        );
        probe.run(Step::post(
            "stock.bars.m5",
            "stocks",
            "/market-data/stocks/bars/list",
            json!({
                "symbols": ["NVDA"], "category": "US_STOCK", "timespan": "M5",
                "count": 60, "real_time_required": false, "trading_sessions": "RTH",
            }),
        ));
    }

    // ---- options ----
    if wanted("options") {
        let (d10, d24) = (day(10), day(24));
        let contracts = probe.run(Step::get(
            "option.contracts.new",
            "options",
            "/trading/instruments/options/contracts/list",
            &[
                ("category", "US_OPTION"),
                ("underlying_symbols", "NVDA"),
                ("status", "LISTING"),
                ("start_date", &d10),
                ("end_date", &d24),
                ("strike_price_gte", "100"),
                ("strike_price_lte", "200"),
                ("show_deliverables", "true"),
            ],
        ));
        let mut occ: Vec<String> = rows_of(&contracts.body)
            .into_iter()
            .flatten()
            .filter_map(|c| c.get("symbol").and_then(Value::as_str))
            .map(|s| s.replace(' ', ""))
            .take(3)
            .collect();
        let (d0, d7) = (day(0), day(7));
        let legacy = probe.run(Step::get(
            "option.contracts.legacy",
            "options",
            "/openapi/instrument/option/contracts",
            &[
                ("underlying_symbols", "NVDA"),
                ("category", "US_OPTION"),
                ("status", "LISTING"),
                ("start_date", &d0),
                ("end_date", &d7),
                ("count", "20"),
            ],
        ));
        if occ.is_empty() {
            occ = radar_core::webull::rest::extract_list(&legacy.body)
                .iter()
                .filter_map(|c| c.get("symbol").and_then(Value::as_str))
                .map(|s| s.replace(' ', ""))
                .take(3)
                .collect();
        }
        let symbols = occ.join(",");
        let first = occ.first().cloned().unwrap_or_default();
        run_all(
            &mut probe,
            "options",
            &[
                (
                    "option.contracts.delisting",
                    "/trading/instruments/options/contracts/list",
                    &[
                        ("category", "US_OPTION"),
                        ("underlying_symbols", "NVDA"),
                        ("status", "DELISTING"),
                    ],
                ),
                (
                    "option.snapshot.new",
                    "/market-data/options/snapshots/list",
                    &[("symbols", &symbols), ("category", "US_OPTION")],
                ),
                (
                    "option.snapshot.legacy",
                    "/openapi/market-data/option/snapshot",
                    &[("symbols", &symbols), ("category", "US_OPTION")],
                ),
                (
                    "option.bars.new",
                    "/market-data/options/bars/list",
                    &[
                        ("symbols", &symbols),
                        ("category", "US_OPTION"),
                        ("timespan", "D"),
                        ("count", "10"),
                        ("real_time_required", "false"),
                    ],
                ),
                (
                    "option.tick",
                    "/market-data/options/ticks/list",
                    &[
                        ("symbol", &first),
                        ("category", "US_OPTION"),
                        ("count", "20"),
                    ],
                ),
            ],
        );
    }

    // ---- fundamentals (single stock) ----
    if wanted("fundamentals") {
        for (id, path, extra) in [
            (
                "company_profile",
                "/market-data/fundamentals/company-profiles/get",
                &[][..],
            ),
            (
                "earnings_calendar",
                "/market-data/fundamentals/earnings-calendars/list",
                &[][..],
            ),
            (
                "dividend_calendar",
                "/market-data/fundamentals/dividend-calendars/list",
                &[][..],
            ),
            (
                "analyst_target",
                "/market-data/fundamentals/analysis/target-prices/get",
                &[][..],
            ),
            (
                "analyst_rating",
                "/market-data/fundamentals/analysis/ratings/get",
                &[][..],
            ),
            (
                "forecast_eps",
                "/market-data/fundamentals/forecast-eps/get",
                &[][..],
            ),
            (
                "capital_flow",
                "/market-data/fundamentals/capital-flows/get",
                &[("count", "5")][..],
            ),
            ("filings", "/market-data/fundamentals/filings/list", &[][..]),
            (
                "financial_indicators",
                "/market-data/fundamentals/indicators/get",
                &[("type", "QUARTERLY"), ("count", "4")][..],
            ),
            (
                "income_statement",
                "/market-data/fundamentals/income-statements/get",
                &[("type", "QUARTERLY"), ("count", "4")][..],
            ),
            (
                "balance_sheet",
                "/market-data/fundamentals/balance-sheets/get",
                &[("type", "QUARTERLY"), ("count", "2")][..],
            ),
            (
                "cashflow",
                "/market-data/fundamentals/cash-flows/get",
                &[("type", "QUARTERLY"), ("count", "2")][..],
            ),
            (
                "financial_alert",
                "/market-data/fundamentals/financial-alerts/get",
                &[][..],
            ),
            (
                "industry_comparison",
                "/market-data/fundamentals/industry-comparisons/get",
                &[][..],
            ),
        ] {
            probe.run(stock_step(id, "fundamentals", path, ROOT, extra));
        }
    }

    // ---- funds / ETF ----
    if wanted("funds") {
        for (id, path, symbol, extra) in [
            (
                "fund_brief",
                "/market-data/fundamentals/fund-brief/get",
                "SPY",
                &[][..],
            ),
            (
                "fund_performance",
                "/market-data/fundamentals/fund-performances/get",
                "SPY",
                &[][..],
            ),
            (
                "fund_net_values",
                "/market-data/fundamentals/fund-net-values/get",
                "SPY",
                &[("count", "5")][..],
            ),
            (
                "fund_holdings",
                "/market-data/fundamentals/fund-holdings/get",
                "SPY",
                &[][..],
            ),
            (
                "fund_dividends",
                "/market-data/fundamentals/fund-dividends/get",
                "SPY",
                &[][..],
            ),
            (
                "fund_rating",
                "/market-data/fundamentals/fund-ratings/get",
                "SPY",
                &[][..],
            ),
            (
                "fund_splits",
                "/market-data/fundamentals/fund-splits/get",
                "SPY",
                &[][..],
            ),
            (
                "fund_files",
                "/market-data/fundamentals/fund-files/get",
                "SPY",
                &[][..],
            ),
            (
                "fund_allocation",
                "/market-data/fundamentals/fund-allocations/get",
                "SPY",
                &[][..],
            ),
            (
                "fund_brief_qqq",
                "/market-data/fundamentals/fund-brief/get",
                "QQQ",
                &[][..],
            ),
        ] {
            probe.run(stock_step(id, "funds", path, symbol, extra));
        }
    }

    // ---- screeners ----
    if wanted("screeners") {
        let sectors = probe.run(Step::get(
            "screener.sectors",
            "screeners",
            "/market-data/screeners/market-sectors/list",
            &[
                ("category", "US_STOCK"),
                ("agg_type", "MARKET_VALUE"),
                ("period", "D1"),
                ("direction", "DESC"),
            ],
        ));
        let sector_id = rows_of(&sectors.body)
            .and_then(|rows| rows.first().copied())
            .and_then(|sector| sector.get("id"))
            .map(|id| match id {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            });
        if let Some(id) = sector_id {
            probe.run(Step::get(
                "screener.sector_detail",
                "screeners",
                "/market-data/screeners/market-sectors/get",
                &[
                    ("sector_id", &id),
                    ("category", "US_STOCK"),
                    ("period", "D1"),
                ],
            ));
        }
        run_all(
            &mut probe,
            "screeners",
            &[
                (
                    "screener.gainers",
                    "/market-data/screeners/gainers-losers/list",
                    &[
                        ("rank_type", "DAY_1"),
                        ("category", "US_STOCK"),
                        ("sort_by", "CHANGE_RATIO"),
                        ("direction", "DESC"),
                    ],
                ),
                (
                    "screener.losers",
                    "/market-data/screeners/gainers-losers/list",
                    &[
                        ("rank_type", "DAY_1"),
                        ("category", "US_STOCK"),
                        ("sort_by", "CHANGE_RATIO"),
                        ("direction", "ASC"),
                    ],
                ),
                (
                    "screener.top_active",
                    "/market-data/screeners/top-actives/list",
                    &[
                        ("category", "US_STOCK"),
                        ("rank_type", "VOLUME"),
                        ("sort_by", "VOLUME"),
                        ("direction", "DESC"),
                    ],
                ),
                (
                    "screener.high_dividend",
                    "/market-data/screeners/high-dividend-ranks/list",
                    &[
                        ("category", "US_STOCK"),
                        ("sort_by", "YIELD"),
                        ("direction", "DESC"),
                    ],
                ),
                (
                    "screener.week52",
                    "/market-data/screeners/week52-high-low/list",
                    &[("rank_type", "NEW_HIGH"), ("category", "US_STOCK")],
                ),
            ],
        );
    }

    // ---- broker watchlists (read-only) ----
    let mut watchlist_id: Option<String> = None;
    if wanted("watchlist") {
        let watchlists = probe.run(
            Step::get(
                "watchlist.list",
                "watchlist",
                "/market-data/watchlists/list",
                &[],
            )
            .sensitive(),
        );
        watchlist_id = rows_of(&watchlists.body)
            .and_then(|rows| rows.first().copied())
            .and_then(|list| list.get("watchlist_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if let Some(id) = watchlist_id {
        probe.run(
            Step::get(
                "watchlist.instruments",
                "watchlist",
                "/market-data/watchlists/instruments/list",
                &[("watchlist_id", &id)],
            )
            .sensitive(),
        );
    }

    // ---- account / orders (read-only; counts + field names only) ----
    let mut account_id: Option<String> = None;
    if wanted("account") || wanted("orders") {
        let accounts = probe
            .run(Step::get("account.list", "account", "/trading/accounts/list", &[]).sensitive());
        account_id = radar_core::webull::rest::extract_list(&accounts.body)
            .first()
            .and_then(|account| account.get("account_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    if let Some(id) = account_id {
        let mut account_steps: Vec<(&str, &str)> = Vec::new();
        if wanted("account") {
            account_steps.extend([
                ("account.positions", "/trading/assets/positions/list"),
                ("account.balance", "/trading/assets/balances/get"),
            ]);
        }
        if wanted("orders") {
            account_steps.extend([
                ("orders.open", "/trading/orders/open-orders/list"),
                ("orders.history", "/trading/orders/historical-orders/list"),
            ]);
        }
        for (step_id, path) in account_steps {
            probe.run(Step::get(step_id, "account", path, &[("account_id", &id)]).sensitive());
        }
    }

    // ---- token status (signing only — the app's setup test does the same) ----
    if wanted("token")
        && let Some(token) = creds.access_token.clone()
    {
        let saved = probe.creds.clone();
        probe.creds = Credentials {
            access_token: None,
            ..saved.clone()
        };
        for (step_id, path) in [
            ("token.check.legacy", "/openapi/auth/token/check"),
            ("token.check.new", "/auth/tokens/check"),
        ] {
            probe.run(Step::post(step_id, "token", path, json!({ "token": token })).sensitive());
        }
        probe.creds = saved;
    }

    // ---- streaming subscribe/unsubscribe (throwaway session, cleaned up) ----
    if wanted("streaming") {
        let session = format!("probe_{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
        let subscribe = json!({
            "session_id": session,
            "symbols": ["NVDA"],
            "category": "US_STOCK",
            "sub_types": ["QUOTE", "SNAPSHOT", "TICK"],
        });
        probe.run(Step::post(
            "streaming.subscribe",
            "streaming",
            "/openapi/market-data/streaming/subscribe",
            subscribe.clone(),
        ));
        let mut unsubscribe = subscribe;
        unsubscribe["unsubscribe_all"] = json!(true);
        probe.run(Step::post(
            "streaming.unsubscribe",
            "streaming",
            "/openapi/market-data/streaming/unsubscribe",
            unsubscribe,
        ));
    }

    // ---- follow-ups the first pass left open ----
    if wanted("extra") {
        run_all(
            &mut probe,
            "extra",
            &[
                // overnight data needs a subscription this key lacks → never asked for
                (
                    "x.snapshot.extended",
                    "/market-data/stocks/snapshots/list",
                    &[
                        ("symbols", "NVDA,AAPL,MSFT,TSLA,SPY,QQQ"),
                        ("category", "US_STOCK"),
                        ("extend_hour_required", "true"),
                        ("overnight_required", "false"),
                    ],
                ),
                (
                    "x.snapshot.extended.etf",
                    "/market-data/stocks/snapshots/list",
                    &[
                        ("symbols", "SPY,QQQ"),
                        ("category", "US_ETF"),
                        ("extend_hour_required", "true"),
                        ("overnight_required", "false"),
                    ],
                ),
                (
                    "x.snapshot.extended.legacy",
                    LEGACY_SNAPSHOT,
                    &[
                        ("symbols", "NVDA"),
                        ("category", "US_STOCK"),
                        ("extend_hour_required", "true"),
                        ("overnight_required", "false"),
                    ],
                ),
                (
                    "x.tick.rth",
                    "/market-data/stocks/ticks/list",
                    &[
                        ("symbol", "NVDA"),
                        ("category", "US_STOCK"),
                        ("count", "20"),
                        ("trading_sessions", "RTH"),
                    ],
                ),
            ],
        );
        let (from, to) = (day(-30), day(-1));
        probe.run(Step::get(
            "x.option.contracts.past",
            "extra",
            "/trading/instruments/options/contracts/list",
            &[
                ("category", "US_OPTION"),
                ("underlying_symbols", "NVDA"),
                ("start_date", &from),
                ("end_date", &to),
            ],
        ));
        // broker watchlists: read instruments until one list returns rows
        let lists = probe.run(
            Step::get(
                "x.watchlist.list",
                "extra",
                "/market-data/watchlists/list",
                &[],
            )
            .sensitive(),
        );
        let ids: Vec<String> = rows_of(&lists.body)
            .into_iter()
            .flatten()
            .filter_map(|list| list.get("watchlist_id").and_then(Value::as_str))
            .map(str::to_string)
            .collect();
        for id in ids.iter().take(7) {
            let outcome = probe.run(
                Step::get(
                    "x.watchlist.instruments",
                    "extra",
                    "/market-data/watchlists/instruments/list",
                    &[("watchlist_id", id)],
                )
                .sensitive(),
            );
            if rows_of(&outcome.body).is_some_and(|rows| !rows.is_empty()) {
                break;
            }
        }
    }

    // ---- SDK-only endpoints (absent from the Thailand reference) ----
    if wanted("sdk") {
        let instrument_id = rows_of(
            &probe
                .run(Step::get(
                    "sdk.instrument_lookup",
                    "sdk",
                    "/trading/instruments/stocks/profiles/list",
                    &[("category", "US_STOCK"), ("symbols", "NVDA")],
                ))
                .body,
        )
        .and_then(|rows| rows.first().copied())
        .and_then(|profile| profile.get("instrument_id"))
        .and_then(Value::as_str)
        .map(str::to_string);
        if let Some(id) = instrument_id {
            let (from, to) = (day(-365), day(365));
            run_all(
                &mut probe,
                "sdk",
                &[
                    (
                        "sdk.corp_action",
                        "/instrument/corp-action",
                        &[
                            ("instrument_ids", &id),
                            ("start_date", &from),
                            ("end_date", &to),
                            ("page_number", "1"),
                            ("page_size", "20"),
                        ],
                    ),
                    (
                        "sdk.eod_bars",
                        "/market-data/eod-bars",
                        &[("instrument_ids", &id), ("count", "5")],
                    ),
                ],
            );
        }
        run_all(
            &mut probe,
            "sdk",
            &[
                (
                    "sdk.stock_news",
                    "/market-data/news/stock-news/list",
                    &[("symbol", "NVDA"), ("category", "US_STOCK")],
                ),
                (
                    "sdk.news_search",
                    "/market-data/news/search/list",
                    &[("keyword", "NVIDIA")],
                ),
                (
                    "sdk.insider_transactions",
                    "/market-data/fundamentals/insider-transactions/list",
                    &[("symbol", "NVDA"), ("category", "US_STOCK")],
                ),
                (
                    "sdk.insider_holdings",
                    "/market-data/fundamentals/insider-holdings/get",
                    &[("symbol", "NVDA"), ("category", "US_STOCK")],
                ),
                (
                    "sdk.economic_calendar",
                    "/market-data/fundamentals/economic-calendars/list",
                    &[("region", "US")],
                ),
                (
                    "sdk.institution_search",
                    "/market-data/fundamentals/institutions/list",
                    &[("keyword", "Vanguard")],
                ),
                (
                    "sdk.noii",
                    "/market-data/stocks/noii-snapshots/list",
                    &[("symbol", "NVDA"), ("category", "US_STOCK")],
                ),
                (
                    "sdk.fear_greed",
                    "/market-data/screeners/fear-greed-indexes/get",
                    &[("category", "US_STOCK")],
                ),
                (
                    "sdk.unusual_activity",
                    "/market-data/screeners/unusual-activities/list",
                    &[("symbol", "NVDA"), ("category", "US_STOCK"), ("count", "5")],
                ),
            ],
        );
        probe.run(Step::get("sdk.config", "sdk", "/openapi/config", &[]).sensitive());
    }

    let report = json!({
        "probed_at": chrono::Utc::now().to_rfc3339(),
        "host": "api.webull.co.th",
        "scheme_used": scheme_name(probe.scheme),
        "steps": probe.log,
    });
    let out = serde_json::to_string_pretty(&report).expect("ser");
    let path = std::path::Path::new("../tmp/capability-probe.json");
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(path, &out).ok();
    println!("\nreport → {}", path.display());
}

#[cfg(test)]
mod tests {
    use super::redact_digits;

    /// Six or more consecutive digits become `[digits]`; shorter runs (codes,
    /// quantities) stay readable. Runs at both edges of the text are covered.
    #[test]
    fn long_digit_runs_are_redacted_and_short_ones_are_not() {
        assert_eq!(
            redact_digits("account 123456789 not found"),
            "account [digits] not found"
        );
        assert_eq!(redact_digits("code 42"), "code 42");
        assert_eq!(redact_digits("code 12345"), "code 12345");
        assert_eq!(redact_digits("top 123456 tail"), "top [digits] tail");
        assert_eq!(
            redact_digits("two 1234567 and 987654321"),
            "two [digits] and [digits]"
        );
        assert_eq!(redact_digits(""), "");
    }
}
