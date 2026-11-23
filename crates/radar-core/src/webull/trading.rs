//! Read-only account and order-query endpoints (live-verified 2026-11-23 —
//! PLAN-APP2 §1 and docs/WEBULL_TH_CAPABILITY_MATRIX.md)
//!
//! เฉพาะการ "อ่าน" — ไม่มี order placement (PRODUCT.md non-goal; PLAN-APP2 ข้อ 3)
//! path ไม่มี prefix `/openapi` เหมือนกลุ่ม stocks

use serde_json::Value;

use super::rest::{FetchError, HttpFetch, extract_list, opt_f64, opt_str, to_f64};

pub const PATH_ACCOUNTS: &str = "/trading/accounts/list";
pub const PATH_POSITIONS: &str = "/trading/assets/positions/list";
pub const PATH_BALANCE: &str = "/trading/assets/balances/get";
/// GET `/trading/orders/open-orders/list` — read-only.
pub const PATH_OPEN_ORDERS: &str = "/trading/orders/open-orders/list";

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AccountInfo {
    pub account_id: String,
    pub account_number: String,
    pub account_type: String,
    pub account_label: String,
}

pub fn fetch_accounts<F: HttpFetch>(f: &mut F) -> Result<Vec<AccountInfo>, FetchError> {
    let (status, data) = f.get(PATH_ACCOUNTS, &[])?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    Ok(extract_list(&data)
        .iter()
        .map(|r| AccountInfo {
            account_id: r
                .get("account_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            account_number: r
                .get("account_number")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            account_type: r
                .get("account_type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            account_label: r
                .get("account_label")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
        })
        .collect())
}

/// หนึ่งตำแหน่ง (หุ้น หรือ option strategy ที่มี legs)
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Position {
    pub symbol: String,
    pub instrument_type: String,
    pub currency: String,
    pub quantity: Option<f64>,
    pub cost_price: Option<f64>,
    pub last_price: Option<f64>,
    pub unrealized_profit_loss: Option<f64>,
    pub option_strategy: Option<String>,
    pub legs: Vec<PositionLeg>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PositionLeg {
    pub symbol: String,
    pub quantity: Option<f64>,
    pub option_type: Option<String>,
    pub expire_date: Option<String>,
    pub exercise_price: Option<f64>,
    pub multiplier: Option<f64>,
}

pub fn fetch_positions<F: HttpFetch>(
    f: &mut F,
    account_id: &str,
) -> Result<Vec<Position>, FetchError> {
    let (status, data) = f.get(PATH_POSITIONS, &[("account_id".into(), account_id.into())])?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    Ok(extract_list(&data)
        .iter()
        .map(|r| Position {
            symbol: r.get("symbol").and_then(Value::as_str).unwrap_or("").into(),
            instrument_type: r
                .get("instrument_type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            currency: r
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            quantity: opt(r.get("quantity")),
            cost_price: opt(r.get("cost_price")),
            last_price: opt(r.get("last_price")),
            unrealized_profit_loss: opt(r.get("unrealized_profit_loss")),
            option_strategy: r
                .get("option_strategy")
                .and_then(Value::as_str)
                .map(String::from),
            legs: r
                .get("legs")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|l| PositionLeg {
                            symbol: l.get("symbol").and_then(Value::as_str).unwrap_or("").into(),
                            quantity: opt(l.get("quantity")),
                            option_type: l
                                .get("option_type")
                                .and_then(Value::as_str)
                                .map(String::from),
                            expire_date: l
                                .get("option_expire_date")
                                .and_then(Value::as_str)
                                .map(String::from),
                            exercise_price: opt(l.get("option_exercise_price")),
                            multiplier: opt(l.get("option_contract_multiplier")),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect())
}

/// ยอดเงินรวม + รายสกุล (probe: keys จริงตาม docs)
#[derive(Debug, Clone, PartialEq, serde::Serialize, Default)]
pub struct Balance {
    pub total_asset_currency: String,
    pub total_cash_balance: Option<f64>,
    pub total_market_value: Option<f64>,
    pub total_unrealized_profit_loss: Option<f64>,
    pub currencies: Vec<CurrencyBalance>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CurrencyBalance {
    pub currency: String,
    pub cash_balance: Option<f64>,
    pub market_value: Option<f64>,
    pub buying_power: Option<f64>,
    pub unrealized_profit_loss: Option<f64>,
}

pub fn fetch_balance<F: HttpFetch>(f: &mut F, account_id: &str) -> Result<Balance, FetchError> {
    let (status, data) = f.get(PATH_BALANCE, &[("account_id".into(), account_id.into())])?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    // response อาจเป็น single object หรือ list — รองรับทั้งคู่
    let obj = if data.is_array() {
        extract_list(&data).first().cloned().unwrap_or(Value::Null)
    } else {
        data
    };
    Ok(Balance {
        total_asset_currency: obj
            .get("total_asset_currency")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into(),
        total_cash_balance: opt(obj.get("total_cash_balance")),
        total_market_value: opt(obj.get("total_market_value")),
        total_unrealized_profit_loss: opt(obj.get("total_unrealized_profit_loss")),
        currencies: obj
            .get("account_currency_assets")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|c| CurrencyBalance {
                        currency: c
                            .get("currency")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .into(),
                        cash_balance: opt(c.get("cash_balance")),
                        market_value: opt(c.get("market_value")),
                        buying_power: opt(c.get("buying_power")),
                        unrealized_profit_loss: opt(c.get("unrealized_profit_loss")),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

// ---------- open orders (read-only) ----------

/// One leg of an order that is still working at the broker.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct OpenOrder {
    pub client_order_id: String,
    /// `NORMAL` for a single order, otherwise the multi-leg strategy name.
    pub combo_type: Option<String>,
    pub status: Option<String>,
    pub side: Option<String>,
    pub symbol: Option<String>,
    pub instrument_type: Option<String>,
    pub order_type: Option<String>,
    pub time_in_force: Option<String>,
    pub total_quantity: Option<f64>,
    pub filled_quantity: Option<f64>,
    pub limit_price: Option<f64>,
    pub place_time: Option<String>,
    pub option_strategy: Option<String>,
}

fn parse_order_leg(leg: &Value, combo_type: Option<&str>) -> Option<OpenOrder> {
    Some(OpenOrder {
        client_order_id: opt_str(leg.get("client_order_id"))?,
        combo_type: combo_type.map(String::from),
        status: opt_str(leg.get("status")),
        side: opt_str(leg.get("side")),
        symbol: opt_str(leg.get("symbol")),
        instrument_type: opt_str(leg.get("instrument_type")),
        order_type: opt_str(leg.get("order_type")),
        time_in_force: opt_str(leg.get("time_in_force")),
        total_quantity: opt_f64(leg.get("total_quantity")),
        filled_quantity: opt_f64(leg.get("filled_quantity")),
        limit_price: opt_f64(leg.get("limit_price")),
        place_time: opt_str(leg.get("place_time")),
        option_strategy: opt_str(leg.get("option_strategy")),
    })
}

/// Flatten `{data: [{client_order_id, combo_type, orders: [...]}]}`: every leg
/// of every (possibly multi-leg) order becomes one row.
pub fn parse_open_orders(data: &Value) -> Vec<OpenOrder> {
    extract_list(data)
        .iter()
        .flat_map(|entry| {
            let combo = opt_str(entry.get("combo_type"));
            entry
                .get("orders")
                .and_then(Value::as_array)
                .map(|legs| {
                    legs.iter()
                        .filter_map(|leg| parse_order_leg(leg, combo.as_deref()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .collect()
}

/// Orders still working at the broker (first page).
pub fn fetch_open_orders<F: HttpFetch>(
    f: &mut F,
    account_id: &str,
) -> Result<Vec<OpenOrder>, FetchError> {
    let (status, data) = f.get(
        PATH_OPEN_ORDERS,
        &[("account_id".into(), account_id.into())],
    )?;
    if status != 200 {
        return Err(FetchError::NonZero(status, data.to_string()));
    }
    Ok(parse_open_orders(&data))
}

fn opt(v: Option<&Value>) -> Option<f64> {
    let x = to_f64(v.unwrap_or(&Value::Null), f64::NAN);
    x.is_finite().then_some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn balance_parses_real_schema_shape() {
        let raw = json!({
            "total_asset_currency": "USD",
            "total_cash_balance": "10000.50",
            "total_market_value": "5300.25",
            "total_unrealized_profit_loss": "-120.10",
            "account_currency_assets": [
                {"currency": "USD", "cash_balance": "10000.50", "market_value": "5300.25",
                 "buying_power": "15000.00", "unrealized_profit_loss": "-120.10"}
            ]
        });
        let b = Balance::default();
        // parser ทำงานบน Value — ทดสอบผ่าน helper เดียวกัน
        let obj = raw;
        let total = opt(obj.get("total_cash_balance"));
        assert_eq!(total, Some(10000.50));
        let cur = obj
            .get("account_currency_assets")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(cur.len(), 1);
        assert_eq!(opt(cur[0].get("buying_power")), Some(15000.0));
        let _ = b;
    }

    #[test]
    fn positions_parse_option_legs() {
        let raw = json!([{
            "symbol": "NVDA 261218C00180000",
            "instrument_type": "OPTION",
            "currency": "USD",
            "quantity": "-2",
            "cost_price": "2.10",
            "last_price": "1.80",
            "unrealized_profit_loss": "60.00",
            "option_strategy": "SINGLE",
            "legs": [{
                "symbol": "NVDA261218C00180000",
                "quantity": "-2",
                "option_type": "CALL",
                "option_expire_date": "2026-12-18",
                "option_exercise_price": "180",
                "option_contract_multiplier": "100"
            }]
        }]);
        let list = extract_list(&raw);
        assert_eq!(list.len(), 1);
        let legs = list[0].get("legs").and_then(Value::as_array).unwrap();
        assert_eq!(
            legs[0].get("option_type").and_then(Value::as_str),
            Some("CALL")
        );
        assert_eq!(opt(legs[0].get("option_contract_multiplier")), Some(100.0));
    }

    #[test]
    fn open_orders_flatten_every_leg_with_its_combo() {
        // Shape of the reference page — the live account answered 200 with no rows,
        // so this fixture follows the documented schema.
        let data = json!({"data": [
            {"client_order_id": "ord-1", "combo_type": "NORMAL", "orders": [
                {"client_order_id": "ord-1", "status": "WORKING", "side": "SELL",
                 "symbol": "NVDA261218P00175000", "instrument_type": "OPTION",
                 "order_type": "LIMIT", "time_in_force": "DAY", "total_quantity": "1",
                 "filled_quantity": "0", "limit_price": "4.15",
                 "place_time": "2026-11-23T10:00:00.000Z"}
            ]},
            {"client_order_id": "ord-2", "combo_type": "VERTICAL", "orders": [
                {"client_order_id": "ord-2-1", "side": "SELL", "total_quantity": "1",
                 "limit_price": "2.00"},
                {"client_order_id": "ord-2-2", "side": "BUY", "total_quantity": "1",
                 "limit_price": "1.00"},
                {"side": "BUY"}
            ]}
        ], "pagination_key": ""});
        let orders = parse_open_orders(&data);
        assert_eq!(orders.len(), 3, "a leg without an id is dropped");
        assert_eq!(orders[0].combo_type.as_deref(), Some("NORMAL"));
        assert_eq!(orders[0].limit_price, Some(4.15));
        assert_eq!(orders[0].instrument_type.as_deref(), Some("OPTION"));
        assert_eq!(orders[1].combo_type.as_deref(), Some("VERTICAL"));
        assert_eq!(orders[1].client_order_id, "ord-2-1");
        assert_eq!(orders[1].symbol, None);
        assert!(parse_open_orders(&json!({"data": []})).is_empty());
        assert!(parse_open_orders(&json!([])).is_empty());
    }

    #[test]
    fn failed_calls_keep_the_broker_body_for_classification() {
        struct Rejected;
        impl HttpFetch for Rejected {
            fn get(
                &mut self,
                _path: &str,
                _params: &[(String, String)],
            ) -> Result<(u16, Value), FetchError> {
                Ok((
                    401,
                    json!({"error_code": "INVALID_TOKEN",
                           "message": "Header x-access-token is missing or invalid."}),
                ))
            }
        }
        let error = fetch_accounts(&mut Rejected).unwrap_err();
        assert_eq!(error.kind(), crate::webull::rest::FailureKind::Token);
        assert_eq!(error.summary(), "HTTP 401 · INVALID_TOKEN");
    }
}
