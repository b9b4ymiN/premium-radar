# Webull Thailand capability matrix

This inventory extracts GET routes from the [Thailand OpenAPI index](https://developer.webull.co.th/apis/llms.txt). All entries are **DOC_ONLY** for this validation run. No broker calls were made. A request may require an approved App Key and token, plus endpoint-specific entitlements. The reference examples may use UAT.

| Reference | Method and path | Query parameters | Published response schema | State |
|---|---|---|---|---|
| [tick](https://developer.webull.co.th/apis/docs/reference/trade-api/tick.md) | `GET /market-data/stocks/ticks/list` | symbol (string, required), category (string, required), count (string, required), trading_sessions (string, required) | object: symbol (string), instrument_id (string), result (array) | DOC_ONLY |
| [snapshot](https://developer.webull.co.th/apis/docs/reference/trade-api/snapshot.md) | `GET /market-data/stocks/snapshots/list` | symbols (string, required), category (string, required), extend_hour_required (string, required), overnight_required (string, required) | array of object: instrument_id (string), pre_close (string), change_ratio (string), symbol (string), last_trade_time (integer), price (string), open (string), close (string), high (string), low (string), volume (string), change (string), ask (string), ask_size (string), bid (string), bid_size (string), extend_hour_last_price (string), extend_hour_high (string), extend_hour_low (string), extend_hour_change (string), extend_hour_change_ratio (string), extend_hour_volume (string), extend_hour_last_trade_time (integer), ovn_price (string), ovn_high (string), ovn_low (string), ovn_volume (string), ovn_change (string), ovn_change_ratio (string), ovn_last_trade_time (integer), ovn_ask (string), ovn_ask_size (string), ovn_bid (string), ovn_bid_size (string) | DOC_ONLY |
| [quotes](https://developer.webull.co.th/apis/docs/reference/trade-api/quotes.md) | `GET /market-data/stocks/depths/list` | symbol (string, required), category (string, required), depth (string, required), overnight_required (string, required) | object: symbol (string), instrument_id (string), quote_time (string), asks (array), bids (array) | DOC_ONLY |
| [footprint](https://developer.webull.co.th/apis/docs/reference/trade-api/footprint.md) | `GET /market-data/stocks/footprints/list` | symbols (string, required), category (string, required), timespan (string, required), count (string), real_time_required (string, required), trading_sessions (string) | array of object: symbol (string), instrument_id (string), result (array) | DOC_ONLY |
| [instrument-list](https://developer.webull.co.th/apis/docs/reference/trade-api/instrument-list.md) | `GET /trading/instruments/stocks/profiles/list` | category (string, required), symbols (string), status (string), sub_category (string), pagination_key (string) | object: data (array), pagination_key (string) | DOC_ONLY |
| [account-list](https://developer.webull.co.th/apis/docs/reference/trade-api/account-list.md) | `GET /trading/accounts/list` | None listed | array of object: user_id (string), account_id (string), account_number (string), account_type (string), account_class (string), account_label (string) | DOC_ONLY |
| [account-balance](https://developer.webull.co.th/apis/docs/reference/trade-api/account-balance.md) | `GET /trading/assets/balances/get` | account_id (String, required) | object: total_asset_currency (string), total_cash_balance (string), total_market_value (string), total_unrealized_profit_loss (string), account_currency_assets (array) | DOC_ONLY |
| [account-position](https://developer.webull.co.th/apis/docs/reference/trade-api/account-position.md) | `GET /trading/assets/positions/list` | account_id (String, required) | array of object: position_id (string), currency (string), quantity (string), symbol (string), option_strategy (string), instrument_type (string), last_price (string), cost_price (string), unrealized_profit_loss (string), legs (array) | DOC_ONLY |
| [order-history](https://developer.webull.co.th/apis/docs/reference/trade-api/order-history.md) | `GET /trading/orders/historical-orders/list` | account_id (String, required), start_time (String), end_time (String), pagination_key (String) | object: data (array), pagination_key (string) | DOC_ONLY |
| [order-open](https://developer.webull.co.th/apis/docs/reference/trade-api/order-open.md) | `GET /trading/orders/open-orders/list` | account_id (String, required), pagination_key (String) | object: data (array), pagination_key (string) | DOC_ONLY |
| [order-detail](https://developer.webull.co.th/apis/docs/reference/trade-api/order-detail.md) | `GET /trading/orders/get` | account_id (String, required), client_order_id (String, required) | object: client_order_id (string), combo_type (string), orders (array) | DOC_ONLY |

## Reference gaps

These indexed references could not be parsed as OpenAPI JSON. Their routes and schemas require manual confirmation.

- [bars](https://developer.webull.co.th/apis/docs/reference/trade-api/bars.md) · HTTP Error 404: Not Found
- [subscribe-trade-events](https://developer.webull.co.th/apis/docs/reference/custom/subscribe-trade-events.md) · No OpenAPI JSON

## Integration priorities

- P1 · Reuse stock snapshots and bars in Market Overview. Expose source, timestamp and availability. Only show ticks and depth once entitled data is proven.
- P1 · Reuse the existing Rust option discovery, snapshot and bars clients. Keep MQTT entitlement and option field availability separate from stock data.
- P2 · Reuse Rust account list, balance and positions. Use currency-aware formatting. Open orders, history and detail require a read-only smoke test before surfacing.
- P3 · Screeners, watchlists, fundamentals, corporate events and ETF data require a separate per-route review of the Thailand guide tier. The CLI and SDK may cover multiple regions. Their presence alone does not establish Thai host entitlement.

The [authentication guide](https://developer.webull.co.th/apis/docs/authentication/overview/) and [Python SDK](https://github.com/webull-inc/webull-openapi-python-sdk) keep the secret local. Some generated reference pages advertise an App Secret header. Preserve legacy signing until read-only live verification resolves regional compatibility. Quotas must be confirmed for the specific account and endpoint.

On future live checks use VERIFIED for useful schema-matching data, ACCESS_DENIED for explicit permission rejection, INVALID_REQUEST for rejected input, NO_DATA for valid empty responses and SCHEMA_MISMATCH for unexpected payloads. Never publish request headers or raw account data.
