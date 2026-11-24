# Webull Thailand capability matrix

What the official **Thailand** OpenAPI (`https://api.webull.co.th`) gives this
app, as measured — not as documented. Every state below comes from a call made
by this code base with the app's own key, except the rows marked `DOC_ONLY`.

- **Probe:** 84 read-only calls on 2026-11-23, 18:00–19:00 ICT (US pre-market).
- **In the app:** 25 command invocations inside the running desktop app on
  2026-11-24, 06:54–07:20 ICT and 01:10–01:15 ET (US after-hours, then the
  overnight session), plus the option quote and the trade list through the
  UI.
- **Tool:** `cargo run --example probe_capabilities` (in `src-tauri`). It loads
  the saved credentials in-process, calls each route once and writes a report
  with status, row count, field names and a sample of public market data.
  Account and watchlist routes record field names only.
- **Sanitised reports** (git-ignored): `tmp/capability-probe-full.json`,
  `tmp/live-check.json`. No request header, key, token or account number is in
  this document or in those files.

## How to read a state

| State | Meaning |
|---|---|
| `VERIFIED` | HTTP 200 with a body whose fields match the client that reads it. |
| `NO_DATA` | HTTP 200, the list is empty. The route works; the row schema is unconfirmed until an account has a row. |
| `ACCESS_DENIED` | The key is valid but this product is not subscribed (HTTP 403). |
| `INVALID_REQUEST` | The route exists; the request was wrong or came at the wrong time (HTTP 400/417). |
| `NOT_FOUND` | HTTP 404 on the Thailand host. The route exists only in the global SDK. |
| `SCHEMA_MISMATCH` | 200 with a body that does not fit the client. Met once, in the app: the watchlist symbol list (the reader's assumption was wrong, and was fixed). Every other route matched. |
| `DOC_ONLY` | Never called: a write, or something that needs explicit approval. |

## Authentication, measured

Four variants against the same production host, same key:

| Signing | Headers sent | GET snapshot | POST bars |
|---|---|---|---|
| V2 — HMAC-SHA1, MD5 body, `x-version: v2` | with `x-app-secret` | 200 | — |
| V2 — same | without `x-app-secret` | 200 | — |
| V3 — HMAC-SHA256, upper-case SHA-256 body, `x-version: v3` | without `x-app-secret` | 200 | 200 |

Conclusions in the code:

- **V3 is the default** (`SignatureScheme::V3`), which is what the current
  authentication guide and the official Python SDK send.
- **The App Secret is never put in a header.** The generated reference pages
  still list `x-app-secret` as required and name HMAC-SHA1 as the default;
  the live host accepts the request without it under both schemes.
- The connection test tries the configured scheme, then the other one, each
  with and without the access token, and stores the one that works. A host
  that only takes V2 keeps working (`settings.webull_signature_scheme`).
- The worked example in the guide does not reproduce (its intermediate string
  is truncated), so the golden fixtures come from the SDK's own composer
  (`crates/radar-core/tests/fixtures/webull_sign_v3.json`).

## What this key is entitled to

| Product | State | Evidence |
|---|---|---|
| US stock quotes, level 1, real time | yes | `delay_minutes: 0` on bars and option snapshots |
| US stock extended hours (pre-market, after-hours) | yes | `extend_hour_*` fields and `trade_status` when `extend_hour_required=true` |
| US stock overnight session | **no** | 403 `MARKET_DATA_NOT_SUBSCRIBED` — "NIGHT TRADING STOCK QUOTES" |
| Footprint / order flow | **no** | 403 `MARKET_DATA_NOT_SUBSCRIBED` — "FOOTPRINT" |
| US options quotes (OPRA, non-display) with greeks | yes | snapshots carry delta…rho, `imp_vol`, open interest, bid and ask sizes |
| Stock and options streaming (MQTT) | yes | connection test reports `entitled`; the app subscribes 96 contracts near the spot |
| Accounts, balances, positions (read) | yes | `/trading/accounts/list` returns one account |
| Screeners, fundamentals, fund data, watchlists (read) | yes | every row below |

Not subscribed is not the same as not implemented: the app never asks for
overnight data (`overnight_required=false` on every snapshot call, because
`true` is a 403) and has no footprint code.

## Capability matrix

"In the app" names the Rust reader, the Tauri command and the surface.
`—` means verified and deliberately not surfaced.

### Stocks

| Capability | Route | State | Evidence | In the app |
|---|---|---|---|---|
| Snapshot (regular session) | `GET /openapi/market-data/stock/snapshot` | VERIFIED | legacy path, used by the scanner since v1 | `quotes::fetch_stock_update` → scanner spot |
| Snapshot, batch, with extended hours | `GET /market-data/stocks/snapshots/list` | VERIFIED | 6 symbols incl. SPY/QQQ in one call, 41 fields | `stocks::fetch_stock_snapshots` → `get_stock_quote` → quote header session line |
| Snapshot, batch, regular session | same, `extend_hour_required=false` | VERIFIED | `price` equals `close` | same reader → `get_home_overview` → Home table and left panel |
| Snapshot with overnight | same, `overnight_required=true` | ACCESS_DENIED | 403 for `US_STOCK`, `US_ETF` and the legacy path | — |
| Daily / intraday bars | `POST /market-data/stocks/bars/list`, `GET /market-data/stocks/bars/get` | VERIFIED | D and M5; 180 daily bars for the chart | `stocks::fetch_stock_bars` → `get_stock_history` → price chart |
| Bars under `/openapi/…/stocks/bars/list` | — | NOT_FOUND | 404 | — |
| Time and sales | `GET /market-data/stocks/ticks/list` | VERIFIED | 20 rows (`price, side, time, volume, trading_session`) | — |
| Level 1 book | `GET /market-data/stocks/depths/list` | VERIFIED | `depth=1` → asks, bids, `quote_time` | — |
| Book deeper than level 1 | same, `depth=5` | INVALID_REQUEST | 417 "depth not more than 1" | — |
| Footprints | `GET /market-data/stocks/footprints/list` | ACCESS_DENIED | 403 | — |
| Instrument profile | `GET /trading/instruments/stocks/profiles/list` | VERIFIED | 19 fields (lot size, margin flags, `easy_to_borrow`) | `stocks::fetch_instrument_profiles` → `get_instrument_profile` |
| NOII snapshot | `GET /market-data/stocks/noii-snapshots/list` | INVALID_REQUEST | 400 "Parameters not valid" with the documented ones | — |

### Options

| Capability | Route | State | Evidence | In the app |
|---|---|---|---|---|
| Contracts | `GET /openapi/instrument/option/contracts` | VERIFIED | legacy path, 796 rows for one underlying | `discovery::fetch_symbols_flow` → scanner |
| Contracts, new route | `GET /trading/instruments/options/contracts/list` | VERIFIED | 266 rows with date and strike filters | — |
| Contracts already expired | either route, past window | NO_DATA | a past window returns 0 rows; `status=DELISTING` returns 0 rows | — |
| Snapshot with bid/ask sizes, open interest, greeks | `GET /market-data/options/snapshots/list` (and the legacy `/openapi/market-data/option/snapshot`) | VERIFIED | 27 fields, `delay_minutes: 0` | `options::fetch_option_quotes` → `get_option_quote` → Contract Analysis "Webull quote"; `quotes::fetch_option_seeds` → scanner |
| Bars | `GET /market-data/options/bars/list` (and the legacy `/openapi/market-data/option/bars`) | VERIFIED | daily bars since the contract's listing day | `quotes::fetch_bars_seeds` → scanner fallback |
| Time and sales | `GET /market-data/options/ticks/list` | VERIFIED | 20 rows; side codes `B / S / N / NB / NS` seen live | `options::fetch_option_ticks` → `get_option_ticks` → "Recent trades" |
| Streaming subscribe | `POST /openapi/market-data/streaming/subscribe` | VERIFIED in the app | 417 `INVALID_SESSION` when called without a live MQTT session — the order is connect, then subscribe | `stream.rs` → real-time push |

### Fundamentals

| Capability | Route | State | Evidence | In the app |
|---|---|---|---|---|
| Company profile | `GET /market-data/fundamentals/company-profiles/get` | VERIFIED | name, industries, CEO, employees, address, business text | `fundamentals::fetch_stock_overview` → `get_stock_overview` → company sheet |
| Analyst target price | `…/analysis/target-prices/get` | VERIFIED | mean, median, low, high | same → company sheet |
| Analyst rating mix | `…/analysis/ratings/get` | VERIFIED | strong buy / buy / hold / sell counts | same → company sheet |
| Next earnings window | `…/financial-alerts/get` | VERIFIED | start and end date, fiscal period, EPS and revenue estimates | same → quote header "Earnings", risk note in Contract Analysis |
| Dividend calendar | `…/dividend-calendars/list` | VERIFIED | ex-date, record date, pay date, amount | same → quote header "Ex-dividend", risk note |
| Earnings history | `…/earnings-calendars/list` | VERIFIED | 2 rows | — |
| Forecast EPS, capital flows, filings, financial indicators, income statement, balance sheet, cash flow | `…/forecast-eps/get`, `capital-flows/get`, `filings/list`, `indicators/get`, `income-statements/get`, `balance-sheets/get`, `cash-flows/get` | VERIFIED | 1–6 rows each for NVDA | — (a statements page would be a new surface) |
| Industry comparison | `…/industry-comparisons/get` | NO_DATA | 0 rows | — |

### Funds (ETF)

| Capability | Route | State | Evidence | In the app |
|---|---|---|---|---|
| Brief, performance, net values, holdings, dividends, ratings, files, allocations | `GET /market-data/fundamentals/fund-*/get` | VERIFIED | SPY and QQQ | — |
| Fund splits | `…/fund-splits/get` | NO_DATA | 0 rows | — |

### Screeners

| Capability | Route | State | Evidence | In the app |
|---|---|---|---|---|
| Gainers / losers | `GET /market-data/screeners/gainers-losers/list` | VERIFIED | 200 rows each way | `screeners::fetch_movers` → `get_movers` → Market page, Home strip |
| Most active | `GET /market-data/screeners/top-actives/list` | VERIFIED | 200 rows, with `relative_volume_10d` | same |
| 52-week high / low | `GET /market-data/screeners/week52-high-low/list` | VERIFIED | 157–159 rows | same |
| High dividend | `GET /market-data/screeners/high-dividend-ranks/list` | VERIFIED | 200 rows | same |
| Sectors | `GET /market-data/screeners/market-sectors/list`, `…/market-sectors/get` | VERIFIED | 20 sectors; the stocks of one sector | `screeners::fetch_sectors`, `fetch_sector_detail` → `get_sectors`, `get_sector_detail` → Market page |

### Watchlists (broker side)

| Capability | Route | State | Evidence | In the app |
|---|---|---|---|---|
| List the Webull watchlists | `GET /market-data/watchlists/list` | VERIFIED | 7 lists (`watchlist_id, name, create_time, update_time, sort`) | `watchlists::fetch_watchlists` → `get_broker_watchlists` → Watchlists page, read-only module |
| Symbols of one watchlist | `GET /market-data/watchlists/instruments/list` | VERIFIED | 5 rows in one of 7 lists, read in the app on 2026-11-24; two of them are Hong Kong listings (`exchange_code: HKG`) | `watchlists::fetch_watchlist_instruments` → same module; a listing outside the US is shown but cannot be opened or copied |
| Create / rename / delete / add / remove / sort | six routes | DOC_ONLY | writes — never called; they need explicit approval | not implemented |

The app's own watchlists stay local (`watchlists.json` in the app-data
folder). The broker lists are shown for reading and for copying a symbol into
the local list; nothing is written to the broker. The copy button was not
pressed during validation, because it changes the local list; it calls the
same command as the page's own "add" field.

### Account, read-only

| Capability | Route | State | Evidence | In the app |
|---|---|---|---|---|
| Accounts | `GET /trading/accounts/list` | VERIFIED | 1 account | `trading::fetch_accounts` → `get_portfolio` → Portfolio |
| Balance per currency | `GET /trading/assets/balances/get` | VERIFIED | totals and `account_currency_assets` | `trading::fetch_balance` → Portfolio cards |
| Positions | `GET /trading/assets/positions/list` | NO_DATA | 0 rows — the account holds nothing | `trading::fetch_positions` → Portfolio table |
| Open orders | `GET /trading/orders/open-orders/list` | NO_DATA | 0 rows, HTTP 200 | `trading::fetch_open_orders` → Portfolio "Open orders" |
| Historical orders | `GET /trading/orders/historical-orders/list` | NO_DATA | 0 rows | — |
| Order detail | `GET /trading/orders/get` | DOC_ONLY | needs a client order id this account does not have | — |

The three `NO_DATA` rows parse without error, but their row fields come from
the reference pages and were never confirmed against a live row. Buying power
is shown as buying power, per currency; it is not called collateral anywhere.

### Trading — out of scope

`POST /trading/orders/…` (place, replace, cancel, preview) are `DOC_ONLY` and
have no code path in this repository.

### Tokens and streaming

| Capability | Route | State | Evidence | In the app |
|---|---|---|---|---|
| Token check | `POST /openapi/auth/token/check`, `POST /auth/tokens/check` | VERIFIED | both paths; `status` and expiry | `auth::check_token` → connection test |
| Token create (SMS confirmation) | `POST /openapi/auth/token/create` | DOC_ONLY in this validation | triggers an SMS to the account's phone; the Setup screen calls it only when the user presses the button | `auth::create_token` |
| Server config | `GET /openapi/config` | VERIFIED | `token_check_enabled` | — |
| MQTT push | TLS `:1883`, MQTT 3.1.1, protobuf payloads | VERIFIED in the app | entitled; 96 contracts near the spot subscribed | `stream.rs` |

### In the global SDK, not on the Thailand host

All 404: `/instrument/corp-action`, `/market-data/eod-bars`,
`/market-data/news/stock-news/list`, `/market-data/news/search/list`,
`/market-data/fundamentals/insider-transactions/list`, `…/insider-holdings/get`,
`…/economic-calendars/list`, `…/institutions/list`,
`/market-data/screeners/fear-greed-indexes/get`, `…/unusual-activities/list`.
Their presence in the SDK says nothing about this region.

## Rate limits and how the app stays under them

Published for the Thailand host, per app key:

| Family | Limit | The coordinator allows |
|---|---|---|
| Data API (quotes, bars, screeners, fundamentals) | 300 requests / 60 s | 255 |
| Instrument profiles | 60 / 60 s | 51 |
| Accounts, balance, positions | 10 / 30 s each | 8 each |
| Order queries | 40 / 2 s | 34 |
| Streaming subscribe | unlimited | — |

One process-wide `Coordinator` owns every request (`transport.rs`): at least
0.235 s between two requests — `60 / (300 × 0.85)`, i.e. 85% of the Data API
quota (the owner's chosen 80–90% band) — a 3 s response cache, and a sliding
window per family taken at 85% of each documented ceiling: shared pools (Data
API, instrument profiles, order queries) count every path in one family
window, while the "each"-limited account/balance/position endpoints keep a
window per path. On the production host a lock file in the app-data folder
paces several app processes against the same quota, using the same interval.
No probe step returned 429.

## What the payloads actually look like

These are the places the reference pages and the live data disagree, or where
the data needs care. Each one is handled by a parser and covered by a test.

- **Numbers are strings.** `"361.51"`, `"2385"`. A missing value is `null`
  after parsing, never `0`.
- **`trade_status` is not in the reference page.** Seen: `"PRE"` during
  pre-market, `"ATH"` after hours, `"OVN"` overnight (01:10 ET). The header
  labels a tag it knows and shows an unknown tag as returned.
- **`price` depends on the request.** With `extend_hour_required=false`,
  `price` equals `close`, the regular-session close. With `true`, `price` is
  the extended-hours trade, `close` stays the regular close, `change` and
  `change_ratio` stay the regular-session change, and the extended move is in
  the flat `extend_hour_*` fields. The app reads extended data for the quote
  header only, and labels it.
- **Watchlist symbols come as `{watchlist_id, instruments: [...]}`**, not in
  the `{data: [...]}` envelope most lists use. The first reader did not know
  that key and showed a list with five symbols as empty. The watchlist
  readers are now strict: a 200 with an unrecognised shape is reported as
  `SCHEMA_MISMATCH`, never drawn as "no data". The other list readers still
  accept the envelopes they were verified against.
- **Bar timestamps are New York midnight written with a UTC suffix**:
  `2026-03-10T04:00:00.000+0000` in summer, `2026-11-23T05:00:00.000+0000` in
  winter. Reading them as UTC moves the session date.
- **The screener's dividend yield is a percent** (`"9.5390"`); the snapshot's
  is a fraction (`"0.007966"`). Each client converts on the way in.
- **`relative_volume_10d` is only filled for the most-active list.**
- **52-week screener `price_1w` / `price_52w`** do not relate to `price` in any
  unit that was confirmed; they are not read.
- **Expired contracts cannot be listed**: a past window returns no rows, so
  history cannot be rebuilt from the chain.
- **Option bars start at the contract's listing day**, trading days only,
  without bid or ask.
- **Contract lists are capped at 1000 rows** with no usable pagination key;
  discovery pages by expiry window instead.

## Not verified, and why

- **Row schemas of positions, open orders and historical orders.** The
  account had none during validation.
- **Pre-market and regular-session behaviour of the new surfaces in the
  app.** The in-app runs happened after hours and overnight; the probe
  covered pre-market at the HTTP level only.
- **The `CLOSED` tag and the regular-session tag** of `trade_status` (`PRE`,
  `ATH` and `OVN` were observed).
- **Copying a symbol from a broker list into the local list.** Not pressed,
  to leave the local list as it was.
- **A host that only accepts V2.** The fallback is tested with fixtures and a
  wiremock, not against a live host.
- **UAT.** Everything here is production.
- **Writes of any kind.** Never called.

## To reproduce

```bash
cd src-tauri
cargo run --example probe_capabilities            # every group
cargo run --example probe_capabilities -- stocks  # one group
# report: ../tmp/capability-probe.json (field names, counts and public samples only)
```

The probe is read-only by construction: it has no route for an order, a
watchlist change or a token write.
