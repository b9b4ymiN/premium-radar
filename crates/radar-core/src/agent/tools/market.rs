//! Agent tools — `market` family (W6 P2): ตลาดวันนี้, สรุปหุ้นตัวเดียว
//! และพฤติกรรมราคาย้อนหลัง ไฟล์นี้เก็บแค่ specs + pure summarisers
//! (domain structs เข้า → compact JSON ออก) — ฝั่ง Tauri เป็นคนดึงข้อมูล

use chrono::NaiveDate;
use serde_json::{Value, json};

use crate::agent::contract::{Mode, Tier, ToolSpec};
use crate::market_history::moving_average;
use crate::quant::realized_vol;
use crate::webull::fundamentals::StockOverview;
use crate::webull::screeners::{MoverRow, SectorRow};
use crate::webull::stocks::{StockBar, StockSnapshot};

/// 1m/3m/6m/1y เป็นจำนวนวันซื้อขาย (21/63/126/252)
pub fn lookback_days(label: &str) -> Option<usize> {
    match label {
        "1m" => Some(21),
        "3m" => Some(63),
        "6m" => Some(126),
        "1y" => Some(252),
        _ => None,
    }
}

/// แท่งราคาที่ต้องขอจาก host: lookback + 50 เผื่อให้ MA50 มีค่าครบ
pub fn lookback_fetch_count(lookback_days: usize) -> u32 {
    (lookback_days + 50) as u32
}

pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "market_pulse".into(),
            tier: Tier::Read,
            modes: vec![Mode::Easy, Mode::Pro],
            description: "ภาพรวมตลาดวันนี้ในครั้งเดียว: หุ้นใน watchlist ที่เปิดใช้อยู่ (ไม่เกิน 10 ตัว), หุ้นที่ราคาขึ้นแรงที่สุด 5 ตัว, ลงแรงที่สุด 5 ตัว และกลุ่มธุรกิจที่วันนี้ไปได้ดีที่สุด/แย่ที่สุดอย่างละ 3 กลุ่ม (พร้อมจำนวนหุ้นในกลุ่มที่ขึ้น/ลง)\nใช้เมื่อ: ผู้ใช้ถามว่า \"ตลาดวันนี้เป็นไง\" หรืออยากรู้บรรยากาศตลาดโดยรวม\nไม่ใช้เมื่อ: อยากดูข้อมูลของหุ้นใดหุ้นหนึ่ง (ใช้ stock_brief) หรือแนวโน้มราคาย้อนหลัง (ใช้ price_behavior)".into(),
            examples: vec![json!({})],
            parameters: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            timeout_ms: 20000,
            webull_cost: 4,
        },
        ToolSpec {
            name: "stock_brief".into(),
            tier: Tier::Read,
            modes: vec![Mode::Easy, Mode::Pro],
            description: "สรุปหุ้นตัวเดียวแบบครบ: ราคาตอนนี้ (รวมช่วงก่อนเปิด/หลังปิดตลาดถ้ามีการซื้อขาย), บริษัทนี้ทำธุรกิจอะไร, จะประกาศผลประกอบการ (earnings) วันไหนและอีกกี่วันถึง, ปันผลรอบถัดไปและรอบล่าสุด, ราคาเป้าหมายที่นักวิเคราะห์ตั้งไว้ และราคาสูง/ต่ำสุดของปีที่ผ่านมา\nใช้เมื่อ: ผู้ใช้ถามถึงหุ้นหนึ่งตัวแบบทั่วไป เช่น \"NVDA เป็นไงบ้าง\" หรือ \"จะประกาศงบวันไหน\"\nไม่ใช้เมื่อ: อยากดูความผันผวนหรือแนวโน้มย้อนหลัง (ใช้ price_behavior) หรือภาพรวมทั้งตลาด (ใช้ market_pulse)".into(),
            examples: vec![json!({"symbol": "NVDA"})],
            parameters: json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "รหัสหุ้น (ticker) ภาษาอังกฤษ เช่น NVDA"
                    }
                },
                "required": ["symbol"],
                "additionalProperties": false
            }),
            timeout_ms: 20000,
            webull_cost: 6,
        },
        ToolSpec {
            name: "price_behavior".into(),
            tier: Tier::Read,
            modes: vec![Mode::Pro],
            description: "วิเคราะห์พฤติกรรมราคาย้อนหลังของหุ้นหนึ่งตัว: ขึ้น/ลงกี่ % ใน 1 สัปดาห์ 1 เดือน และตลอดช่วงที่เลือก, ราคาล่าสุดอยู่ตรงไหนของช่วงสูง-ต่ำ (0 = ต่ำสุด 100 = สูงสุด), ความผันผวนต่อปี, เส้นค่าเฉลี่ยราคา 20/50 วัน (MA20/MA50) และราคาอยู่ห่างจากเส้นเท่าไหร่, ทิศทางรวม (ขึ้น/ลง/ออกข้าง), ร่วงจากจุดสูงสุดของช่วงเท่าไหร่ และขยับเฉลี่ยวันละกี่ %\nใช้เมื่อ: ผู้ใช้ถามว่าหุ้นนี้ \"ขึ้นไปเยอะไหม\" \"ผันผวนแค่ไหน\" หรือกำลังเข้าโหมดไหน\nไม่ใช้เมื่อ: อยากรู้ราคาปัจจุบันหรือวันประกาศงบ (ใช้ stock_brief) — เครื่องมือนี้เป็นมุมมองย้อนหลังเท่านั้น".into(),
            examples: vec![json!({"symbol": "NVDA"}), json!({"symbol": "AAPL", "lookback": "1y"})],
            parameters: json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "รหัสหุ้น (ticker) ภาษาอังกฤษ เช่น NVDA"
                    },
                    "lookback": {
                        "type": "string",
                        "enum": ["1m", "3m", "6m", "1y"],
                        "description": "ช่วงเวลาย้อนหลัง: 1m ≈ เดือน, 3m ≈ ไตรมาส, 6m ≈ ครึ่งปี, 1y ≈ ปี",
                        "default": "3m"
                    }
                },
                "required": ["symbol"],
                "additionalProperties": false
            }),
            timeout_ms: 20000,
            webull_cost: 1,
        },
    ]
}

// ---------- helpers ----------

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// % ต่อวันของ watchlist ที่ home_overview_on สร้างให้แล้ว (×100 อยู่แล้ว)
fn pct_field(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).map(round2)
}

// ---------- market_pulse ----------

/// สรุปตลาดวันนี้ ทุกส่วนเป็น `Ok(rows)` หรือ `Err(เหตุผลสั้นๆ ภาษาไทย)`
/// — ส่วนที่พังจะกลายเป็น field `*_error` แทนรายการ ไม่ทำให้ทั้งเครื่องมือล้ม
///
/// แถว quote ที่โหลดสำเร็จเก็บแค่ symbol/price_usd/change_pct (ไม่พิมพ์
/// status "available" เพื่อประหยัดที่) — แถวที่โหลดไม่ได้เหลือ symbol+status
pub fn summarize_market_pulse(
    as_of: &str,
    watchlist_name: Option<&str>,
    quotes: Result<&[Value], &str>,
    gainers: Result<&[MoverRow], &str>,
    losers: Result<&[MoverRow], &str>,
    sectors: Result<&[SectorRow], &str>,
) -> Value {
    let quote_row = |row: &Value| {
        let symbol = row.get("symbol").and_then(Value::as_str).unwrap_or("");
        let status = row.get("status").and_then(Value::as_str).unwrap_or("");
        if status == "available" {
            json!({
                "symbol": symbol,
                "px_usd": pct_field(row.get("price")),
                "chg_pct": pct_field(row.get("change_pct")),
            })
        } else {
            json!({"symbol": symbol, "status": status})
        }
    };
    let mover_row = |row: &MoverRow| {
        json!({
            "symbol": row.symbol,
            "px_usd": row.price.map(round2),
            "chg_pct": row.change_ratio.map(|r| round2(r * 100.0)),
        })
    };
    let sector_row = |row: &SectorRow, ratio: f64| {
        json!({
            "name": row.name.clone().unwrap_or_else(|| row.id.clone()),
            "chg_pct": round2(ratio * 100.0),
            "up": row.advanced.map(|a| a.round() as i64),
            "down": row.declined.map(|d| d.round() as i64),
        })
    };

    let mut out = json!({
        "as_of": as_of,
        "source": "webull_rest",
        "watchlist_name": watchlist_name,
    });
    match quotes {
        Ok(rows) => {
            let capped: Vec<Value> = rows.iter().take(10).map(quote_row).collect();
            out["quotes"] = json!(capped);
        }
        Err(reason) => {
            out["quotes_error"] = json!(reason);
        }
    }
    for (key, part) in [("gainers", gainers), ("losers", losers)] {
        match part {
            Ok(rows) => {
                let capped: Vec<Value> = rows.iter().take(5).map(mover_row).collect();
                out[key] = json!(capped);
            }
            Err(reason) => {
                out[format!("{key}_error")] = json!(reason);
            }
        }
    }
    match sectors {
        Ok(rows) => {
            // จัดอันดับตาม % เปลี่ยนแปลงของวัน (total_cmp → กับ NaN ก็ deterministic)
            let mut ranked: Vec<(f64, &SectorRow)> = rows
                .iter()
                .filter_map(|row| row.change_ratio.map(|ratio| (ratio, row)))
                .collect();
            ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
            let strongest: Vec<Value> = ranked
                .iter()
                .take(3)
                .map(|(r, row)| sector_row(row, *r))
                .collect();
            let weakest: Vec<Value> = ranked
                .iter()
                .rev()
                .take(3)
                .map(|(r, row)| sector_row(row, *r))
                .collect();
            out["sectors"] = json!({"strongest": strongest, "weakest": weakest});
        }
        Err(reason) => {
            out["sectors_error"] = json!(reason);
        }
    }
    out
}

// ---------- stock_brief ----------

/// สรุปหุ้นตัวเดียว ชิ้นที่หาไม่ได้เป็น null + รวมเหตุผลใน "unavailable"
/// (`overview.unavailable` ของ broker + `unavailable_notes` ที่ฝั่ง cache
/// ส่งมา — สาม path เดียวกันทั้งข้อมูลสดและข้อมูลจาก cache)
///
/// days_until = จำนวนวันจาก `today` ถึงวันเริ่มประกาศงบ (ติดลบปรับเป็น 0
/// = อยู่ในช่วงประกาศแล้ว)
pub fn summarize_stock_brief(
    as_of: &str,
    today: NaiveDate,
    symbol: &str,
    quote: Option<&StockSnapshot>,
    quote_error: Option<&str>,
    overview: Option<&StockOverview>,
    unavailable_notes: &[String],
) -> Value {
    let mut unavailable: Vec<String> = Vec::new();
    if let Some(reason) = quote_error {
        unavailable.push(format!("ราคาหุ้น: {reason}"));
    } else if quote.is_none() {
        unavailable.push("ราคาหุ้น: ไม่มีข้อมูล".into());
    }
    if let Some(overview) = overview {
        for miss in &overview.unavailable {
            unavailable.push(format!("{}: {}", miss.section, miss.reason));
        }
    }
    unavailable.extend(unavailable_notes.iter().cloned());

    let quote_json = quote.map(|q| {
        json!({
            "price_usd": q.price.map(round2),
            "change_pct": q.change_ratio.map(|r| round2(r * 100.0)),
            "extended": q.extended.as_ref().map(|e| json!({
                "price_usd": e.last_price.map(round2),
                "change_pct": e.change_ratio.map(|r| round2(r * 100.0)),
            })),
            "week52_high_usd": q.fifty_two_wk_high.map(round2),
            "week52_low_usd": q.fifty_two_wk_low.map(round2),
        })
    });
    let profile = overview.and_then(|o| o.profile.as_ref()).map(|p| {
        json!({
            "name": p.company_name,
            "industry": (!p.industries.is_empty()).then(|| p.industries.join(", ")),
        })
    });
    let next_earnings = overview.and_then(|o| o.next_earnings.as_ref()).map(|e| {
        let days = NaiveDate::parse_from_str(&e.start_date, "%Y-%m-%d")
            .ok()
            .map(|start| (start - today).num_days().max(0));
        json!({
            "start_date": e.start_date,
            "end_date": e.end_date,
            "days_until": days,
        })
    });
    let dividend = overview
        .and_then(|o| o.dividends.as_ref())
        .filter(|d| d.next_ex_div().is_some() || d.last_paid().is_some())
        .map(|d| {
            json!({
                "next_ex_date": d.next_ex_div().and_then(|e| e.ex_div_date.clone()),
                "next_amount_usd": d.next_ex_div().and_then(|e| e.amount).map(round2),
                "last_ex_date": d.last_paid().and_then(|e| e.ex_div_date.clone()),
                "last_amount_usd": d.last_paid().and_then(|e| e.amount).map(round2),
            })
        });
    let target = overview.and_then(|o| o.analyst_target.as_ref());
    let rating = overview.and_then(|o| o.analyst_rating.as_ref());
    let analysts = json!({
        "target_low_usd": target.and_then(|t| t.low).map(round2),
        "target_mean_usd": target.and_then(|t| t.mean).map(round2),
        "target_high_usd": target.and_then(|t| t.high).map(round2),
        "strong_buy": rating.and_then(|r| r.strong_buy).map(|n| n.round() as i64),
        "buy": rating.and_then(|r| r.buy).map(|n| n.round() as i64),
        "hold": rating.and_then(|r| r.hold).map(|n| n.round() as i64),
        "sell": rating.and_then(|r| r.sell).map(|n| n.round() as i64),
        "bullish_pct": rating.and_then(|r| r.bullish_pct()).map(round2),
    });

    json!({
        "as_of": as_of,
        "source": "webull_rest",
        "symbol": symbol,
        "quote": quote_json,
        "profile": profile,
        "next_earnings": next_earnings,
        "dividend": dividend,
        "analysts": analysts,
        "unavailable": unavailable,
    })
}

// ---------- price_behavior ----------

/// กฎตัดสินทิศทาง (deterministic — ทดสอบด้วยข้อมูลสังเคราะห์ได้):
/// - "ขึ้น" เมื่อ close ล่าสุด > MA20 และ MA20 ≥ MA50 และ MA20 วันนี้
///   สูงกว่า MA20 เมื่อ 10 แท่งก่อน (เส้นเอียงขึ้น)
/// - "ลง" เมื่อ close ล่าสุด < MA20 และ MA20 ≤ MA50 และ MA20 วันนี้
///   ต่ำกว่า MA20 เมื่อ 10 แท่งก่อน (เส้นเอียงลง)
/// - ทุกกรณีอื่น (รวมกรณี MA ไม่พอคำนวณ) = "ออกข้าง"
fn trend_label(
    last_close: f64,
    ma20: Option<f64>,
    ma50: Option<f64>,
    ma20_ten_bars_ago: Option<f64>,
) -> &'static str {
    let (Some(ma20), Some(ma50)) = (ma20, ma50) else {
        return "ออกข้าง";
    };
    let rising = matches!((ma20, ma20_ten_bars_ago), (now, Some(prev)) if now > prev);
    let falling = matches!((ma20, ma20_ten_bars_ago), (now, Some(prev)) if now < prev);
    if last_close > ma20 && ma20 >= ma50 && rising {
        "ขึ้น"
    } else if last_close < ma20 && ma20 <= ma50 && falling {
        "ลง"
    } else {
        "ออกข้าง"
    }
}

fn change_over(closes: &[f64], days: usize) -> Option<f64> {
    let n = closes.len();
    (n > days).then(|| round2((closes[n - 1] / closes[n - 1 - days] - 1.0) * 100.0))
}

fn dist_from(last: f64, ma: Option<f64>) -> Option<f64> {
    ma.filter(|m| *m > 0.0)
        .map(|m| round2((last / m - 1.0) * 100.0))
}

/// สรุปพฤติกรรมราคาจากแท่งรายวัน — คืนแต่ตัวเลขคำนวณแล้ว ไม่มี raw bars
///
/// ราคาสูง/ต่ำของช่วงใช้ high/low ของแท่งในช่วง lookback (ถ้าแท่งไหนไม่มี
/// จะยกเว้นแท่งนั้น; ถ้าไม่มีเลย fallback เป็นราคาปิดสูง/ต่่าสุดของช่วง)
/// ส่วน change/vol/drawdown คำนวณจากราคาปิดที่ไม่ใช่ null เท่านั้น
pub fn summarize_price_behavior(
    as_of: &str,
    symbol: &str,
    lookback: &str,
    bars: &[StockBar],
    lookback_days: usize,
) -> Result<Value, String> {
    let closes: Vec<f64> = bars.iter().filter_map(|b| b.close).collect();
    if closes.len() <= lookback_days {
        return Err(format!(
            "ราคาย้อนหลังของ {symbol} ไม่พอสำหรับช่วง {lookback} (ได้ {} แท่ง ต้องการมากกว่า {lookback_days}) — เรียกใหม่ด้วย lookback สั้นลง เช่น \"1m\"",
            closes.len()
        ));
    }
    let n = closes.len();
    let last = closes[n - 1];
    let window = &closes[n - lookback_days..];

    // สูง/ต่ำของช่วงจาก high/low ของแท่ง (fallback: ราคาปิด)
    let tail_bars = &bars[bars.len() - lookback_days..];
    let period_high = tail_bars
        .iter()
        .filter_map(|b| b.high)
        .fold(None, |acc: Option<f64>, h| {
            Some(acc.map_or(h, |p| p.max(h)))
        })
        .unwrap_or_else(|| window.iter().copied().fold(f64::MIN, f64::max));
    let period_low = tail_bars
        .iter()
        .filter_map(|b| b.low)
        .fold(None, |acc: Option<f64>, l| {
            Some(acc.map_or(l, |p| p.min(l)))
        })
        .unwrap_or_else(|| window.iter().copied().fold(f64::MAX, f64::min));
    let position_pct = (period_high > period_low)
        .then(|| round2((last - period_low) / (period_high - period_low) * 100.0));

    // max drawdown ในช่วง: หลุดจากจุดสูงสุดสะสมมากี่ %
    let mut peak = window[0];
    let mut max_dd = 0.0f64;
    for close in window {
        peak = peak.max(*close);
        max_dd = max_dd.max((peak - *close) / peak);
    }

    // ขยับเฉลี่ยวันละกี่ % (mean |daily return|)
    let avg_move = (window.len() > 1).then(|| {
        let moves: Vec<f64> = window
            .windows(2)
            .map(|pair| ((pair[1] / pair[0]) - 1.0).abs() * 100.0)
            .collect();
        round2(moves.iter().sum::<f64>() / moves.len() as f64)
    });

    let ma20_series = moving_average(bars, 20);
    let ma50_series = moving_average(bars, 50);
    let ma20 = ma20_series.last().copied().flatten();
    let ma50 = ma50_series.last().copied().flatten();
    let ma20_prev10 = ma20_series
        .len()
        .checked_sub(11)
        .and_then(|i| ma20_series[i]);

    Ok(json!({
        "as_of": as_of,
        "source": "webull_rest",
        "symbol": symbol,
        "lookback": lookback,
        "last_close_usd": round2(last),
        "change_pct": {
            "d5": change_over(&closes, 5),
            "d21": change_over(&closes, 21),
            "period": change_over(&closes, lookback_days),
        },
        "period": {
            "high_usd": round2(period_high),
            "low_usd": round2(period_low),
            "position_pct": position_pct,
        },
        "volatility_pct": {
            "d20": realized_vol(&closes, 20).map(|v| round2(v * 100.0)),
            "d60": realized_vol(&closes, 60).map(|v| round2(v * 100.0)),
        },
        "ma": {
            "ma20_usd": ma20.map(round2),
            "ma50_usd": ma50.map(round2),
            "dist_from_ma20_pct": dist_from(last, ma20),
            "dist_from_ma50_pct": dist_from(last, ma50),
        },
        "trend": trend_label(last, ma20, ma50, ma20_prev10),
        "max_drawdown_pct": round2(max_dd * 100.0),
        "avg_daily_move_pct": avg_move,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::registry::Registry;

    fn bar(time: &str, close: f64) -> StockBar {
        StockBar {
            time: time.into(),
            open: Some(close),
            high: Some(close + 1.0),
            low: Some(close - 1.0),
            close: Some(close),
            volume: Some(1000.0),
        }
    }

    fn mover(symbol: &str, price: Option<f64>, ratio: Option<f64>) -> MoverRow {
        MoverRow {
            symbol: symbol.into(),
            price,
            change_ratio: ratio,
            ..Default::default()
        }
    }

    fn sector(name: &str, ratio: f64, up: f64, down: f64) -> SectorRow {
        SectorRow {
            id: "0".into(),
            name: Some(name.into()),
            change_ratio: Some(ratio),
            advanced: Some(up),
            declined: Some(down),
            ..Default::default()
        }
    }

    // ---------- specs ----------

    #[test]
    fn three_specs_with_expected_names_and_modes() {
        let specs = specs();
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["market_pulse", "stock_brief", "price_behavior"]);
        assert_eq!(specs[0].modes, vec![Mode::Easy, Mode::Pro]);
        assert_eq!(specs[1].modes, vec![Mode::Easy, Mode::Pro]);
        assert_eq!(specs[2].modes, vec![Mode::Pro]);
    }

    #[test]
    fn specs_follow_the_tool_conventions() {
        for spec in specs() {
            assert_eq!(spec.tier, Tier::Read, "{}", spec.name);
            assert_eq!(spec.timeout_ms, 20000, "{}", spec.name);
            assert!(spec.webull_cost > 0, "{}", spec.name);
            assert!(!spec.examples.is_empty(), "{} needs examples", spec.name);
            assert!(
                spec.description.contains("ใช้เมื่อ:"),
                "{}: {}",
                spec.name,
                spec.description
            );
            assert!(
                spec.description.contains("ไม่ใช้เมื่อ:"),
                "{}: {}",
                spec.name,
                spec.description
            );
            assert_eq!(
                spec.parameters.get("additionalProperties"),
                Some(&json!(false)),
                "{} must lock its schema",
                spec.name
            );
            assert!(
                spec.to_tool_def()
                    .function
                    .description
                    .contains("ตัวอย่างการเรียก:")
            );
        }
    }

    #[test]
    fn every_example_passes_validation_in_the_modes_it_belongs_to() {
        let mut reg = Registry::new();
        for spec in specs() {
            reg.register(spec).expect("register market spec");
        }
        for spec in specs() {
            for mode in [Mode::Easy, Mode::Pro] {
                for ex in &spec.examples {
                    let args = ex.to_string();
                    let validated = reg.validate(&spec.name, mode, &args);
                    match (mode, spec.name.as_str()) {
                        (Mode::Easy, "price_behavior") => {
                            let err = validated.expect_err("not in Easy mode");
                            assert_eq!(err.code, crate::agent::contract::ToolErrorCode::NotInMode);
                        }
                        _ => {
                            validated.unwrap_or_else(|e| {
                                panic!("example {args} of {} failed: {}", spec.name, e.message)
                            });
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn lookback_mapping_and_fetch_count() {
        assert_eq!(lookback_days("1m"), Some(21));
        assert_eq!(lookback_days("1y"), Some(252));
        assert_eq!(lookback_days("2y"), None);
        assert_eq!(lookback_fetch_count(63), 113);
    }

    // ---------- market_pulse ----------

    #[test]
    fn pulse_caps_lists_and_fits_1500_bytes() {
        let quotes: Vec<Value> = (0..20)
            .map(|i| {
                if i == 7 || i == 15 {
                    json!({"symbol": format!("SYM{i}"), "status": "no_data"})
                } else {
                    json!({
                        "symbol": format!("SYM{i}"),
                        "status": "available",
                        "price": 100.0 + i as f64,
                        "change_pct": 0.5 + i as f64,
                    })
                }
            })
            .collect();
        let gainers: Vec<MoverRow> = (0..20)
            .map(|i| mover(&format!("G{i}"), Some(50.0), Some(0.05)))
            .collect();
        let losers: Vec<MoverRow> = (0..20)
            .map(|i| mover(&format!("L{i}"), Some(30.0), Some(-0.05)))
            .collect();
        let sectors: Vec<SectorRow> = [
            ("Software & IT Services", 0.021, 241.0, 117.0),
            ("Semiconductors", 0.018, 90.0, 60.0),
            ("Energy", 0.015, 200.0, 90.0),
            ("Financials", 0.004, 150.0, 140.0),
            ("Healthcare", -0.001, 120.0, 130.0),
            ("Utilities", -0.004, 40.0, 220.0),
            ("Retailing", -0.008, 70.0, 180.0),
            ("Media", -0.012, 55.0, 160.0),
            ("Industrials", -0.014, 80.0, 170.0),
            ("Consumer Staples", -0.016, 60.0, 190.0),
            ("Real Estate", -0.019, 45.0, 200.0),
        ]
        .iter()
        .map(|(name, ratio, up, down)| sector(name, *ratio, *up, *down))
        .collect();

        let out = summarize_market_pulse(
            "2026-11-27T09:00:00+00:00",
            Some("Core"),
            Ok(&quotes),
            Ok(&gainers),
            Ok(&losers),
            Ok(&sectors),
        );
        assert_eq!(out["as_of"], json!("2026-11-27T09:00:00+00:00"));
        assert_eq!(out["source"], json!("webull_rest"));
        assert_eq!(out["watchlist_name"], json!("Core"));
        assert_eq!(out["quotes"].as_array().unwrap().len(), 10);
        assert_eq!(out["gainers"].as_array().unwrap().len(), 5);
        assert_eq!(out["losers"].as_array().unwrap().len(), 5);
        assert_eq!(out["sectors"]["strongest"].as_array().unwrap().len(), 3);
        assert_eq!(out["sectors"]["weakest"].as_array().unwrap().len(), 3);
        // แรงสุด = เรียงจาก % มากไปน้อย; อ่อนสุด = ใกล้ศูนย์ไปเป็นลบมากสุด
        assert_eq!(
            out["sectors"]["strongest"][0]["name"],
            json!("Software & IT Services")
        );
        assert_eq!(out["sectors"]["weakest"][0]["name"], json!("Real Estate"));
        let size = serde_json::to_string(&out).expect("serializable");
        assert!(
            size.len() <= 1500,
            "pulse payload {} bytes: {size}",
            size.len()
        );
    }

    #[test]
    fn pulse_failed_parts_become_error_fields() {
        let out = summarize_market_pulse(
            "2026-11-27T09:00:00+00:00",
            None,
            Err("ตลาดยังไม่เปิด"),
            Ok(&[]),
            Err("ดึงรายการไม่สำเร็จ"),
            Ok(&[]),
        );
        assert_eq!(out["quotes_error"], json!("ตลาดยังไม่เปิด"));
        assert_eq!(out["losers_error"], json!("ดึงรายการไม่สำเร็จ"));
        assert!(out.get("quotes").is_none());
        assert!(out.get("losers").is_none());
        assert_eq!(out["gainers"], json!([]));
        assert_eq!(out["watchlist_name"], json!(null));
    }

    // ---------- stock_brief ----------

    #[test]
    fn brief_surfaces_every_family_and_missing_pieces() {
        use crate::webull::fundamentals::*;
        use crate::webull::stocks::ExtendedQuote;

        let mut quote = StockSnapshot {
            symbol: "NVDA".into(),
            price: Some(136.5932),
            change_ratio: Some(-0.140088),
            fifty_two_wk_high: Some(243.37),
            fifty_two_wk_low: Some(124.5),
            ..Default::default()
        };
        quote.extended = Some(ExtendedQuote {
            last_price: Some(136.5932),
            change_ratio: Some(0.088132),
            ..Default::default()
        });
        let overview = StockOverview {
            profile: Some(CompanyProfile {
                company_name: "NVIDIA Corp".into(),
                industries: vec!["Semiconductors".into(), "Technology".into()],
                ..Default::default()
            }),
            analyst_target: Some(AnalystTarget {
                low: Some(130.0),
                mean: Some(244.18),
                high: Some(425.0),
                ..Default::default()
            }),
            analyst_rating: Some(AnalystRating {
                number: Some(63.0),
                strong_buy: Some(34.0),
                buy: Some(11.0),
                hold: Some(16.0),
                sell: Some(2.0),
                ..Default::default()
            }),
            next_earnings: Some(EarningsWindow {
                start_date: "2026-12-18".into(),
                end_date: "2026-12-22".into(),
                ..Default::default()
            }),
            dividends: Some(DividendCalendar {
                upcoming: vec![DividendEvent {
                    ex_div_date: Some("2026-12-03".into()),
                    amount: Some(0.01),
                    ..Default::default()
                }],
                history: vec![DividendEvent {
                    ex_div_date: Some("2026-09-04".into()),
                    amount: Some(0.01),
                    ..Default::default()
                }],
            }),
            unavailable: vec![],
        };
        let today = NaiveDate::from_ymd_opt(2026, 11, 27).unwrap();
        let out = summarize_stock_brief(
            "2026-11-27T09:00:00+00:00",
            today,
            "NVDA",
            Some(&quote),
            None,
            Some(&overview),
            &[],
        );
        assert_eq!(out["as_of"], json!("2026-11-27T09:00:00+00:00"));
        assert_eq!(out["source"], json!("webull_rest"));
        assert_eq!(out["quote"]["price_usd"], json!(136.59));
        assert_eq!(out["quote"]["change_pct"], json!(-14.01));
        assert_eq!(out["quote"]["extended"]["change_pct"], json!(8.81));
        assert_eq!(out["quote"]["week52_high_usd"], json!(243.37));
        assert_eq!(out["profile"]["name"], json!("NVIDIA Corp"));
        assert_eq!(
            out["profile"]["industry"],
            json!("Semiconductors, Technology")
        );
        assert_eq!(out["next_earnings"]["start_date"], json!("2026-12-18"));
        assert_eq!(out["next_earnings"]["days_until"], json!(21));
        assert_eq!(out["dividend"]["next_ex_date"], json!("2026-12-03"));
        assert_eq!(out["dividend"]["last_amount_usd"], json!(0.01));
        assert_eq!(out["analysts"]["target_mean_usd"], json!(244.18));
        assert_eq!(out["analysts"]["strong_buy"], json!(34));
        assert_eq!(out["analysts"]["bullish_pct"], json!(71.43));
        assert_eq!(out["unavailable"], json!([]));
    }

    #[test]
    fn brief_nulls_and_unavailable_when_data_is_missing() {
        use crate::webull::fundamentals::Unavailable;
        let today = NaiveDate::from_ymd_opt(2026, 11, 27).unwrap();
        let mut overview = StockOverview::default();
        overview.unavailable.push(Unavailable {
            section: "analyst target",
            reason: "no data".into(),
        });
        let out = summarize_stock_brief(
            "2026-11-27T09:00:00+00:00",
            today,
            "XYZ",
            None,
            Some("HTTP 403"),
            Some(&overview),
            &["dividend calendar: no data".to_string()],
        );
        assert_eq!(out["quote"], json!(null));
        assert_eq!(out["analysts"]["target_mean_usd"], json!(null));
        assert_eq!(
            out["unavailable"],
            json!([
                "ราคาหุ้น: HTTP 403",
                "analyst target: no data",
                "dividend calendar: no data"
            ])
        );
    }

    // ---------- price_behavior ----------

    #[test]
    fn behavior_rejects_too_few_bars_with_a_thai_hint() {
        let bars: Vec<StockBar> = (0..30).map(|i| bar(&format!("d{i}"), 100.0)).collect();
        let err = summarize_price_behavior("t", "XYZ", "3m", &bars, 63).unwrap_err();
        assert!(err.contains("ไม่พอ"), "{err}");
        assert!(err.contains("\"1m\""), "{err}");
    }

    #[test]
    fn behavior_trend_rising_falling_and_flat() {
        let rising: Vec<StockBar> = (0..120)
            .map(|i| bar(&format!("d{i}"), 100.0 + i as f64))
            .collect();
        let out = summarize_price_behavior("t", "UP", "3m", &rising, 63).unwrap();
        assert_eq!(out["trend"], json!("ขึ้น"));

        let falling: Vec<StockBar> = (0..120)
            .map(|i| bar(&format!("d{i}"), 300.0 - i as f64))
            .collect();
        let out = summarize_price_behavior("t", "DOWN", "3m", &falling, 63).unwrap();
        assert_eq!(out["trend"], json!("ลง"));

        let flat: Vec<StockBar> = (0..120).map(|i| bar(&format!("d{i}"), 100.0)).collect();
        let out = summarize_price_behavior("t", "FLAT", "3m", &flat, 63).unwrap();
        assert_eq!(out["trend"], json!("ออกข้าง"));
        // แท่ง flat มี high/low ±1 รอบ close → ราคาอยู่กลางช่วงพอดี
        assert_eq!(out["period"]["position_pct"], json!(50.0));
        assert_eq!(out["max_drawdown_pct"], json!(0.0));
        assert_eq!(out["avg_daily_move_pct"], json!(0.0));
    }

    /// ตรวจตัวเลขกับค่าที่คำนวณมือจาก closes [100, 110, 99, 104] lookback 3:
    /// window = [110, 99, 104], high/low จาก high/low ของแท่ง (+1/-1)
    #[test]
    fn behavior_numbers_match_hand_computation() {
        let closes = [100.0, 110.0, 99.0, 104.0];
        let bars: Vec<StockBar> = (0..4)
            .map(|i| {
                let c = closes[i];
                StockBar {
                    time: format!("d{i}"),
                    open: Some(c),
                    high: Some(c + 1.0),
                    low: Some(c - 1.0),
                    close: Some(c),
                    volume: Some(10.0),
                }
            })
            .collect();
        let out = summarize_price_behavior("t", "XYZ", "1m", &bars, 3).unwrap();
        assert_eq!(out["last_close_usd"], json!(104.0));
        // period change = 104/100 - 1 = 4%
        assert_eq!(out["change_pct"]["period"], json!(4.0));
        assert_eq!(out["change_pct"]["d5"], json!(null)); // มีแค่ 4 แท่ง
        // window bars = 3 แท่งหลัง (110/99/104) → high 111, low 98
        assert_eq!(out["period"]["high_usd"], json!(111.0));
        assert_eq!(out["period"]["low_usd"], json!(98.0));
        assert_eq!(out["period"]["position_pct"], json!(46.15)); // (104-98)/(111-98)
        // drawdown จาก peak 110 → 99 ลงลึกสุด = 11/110 = 10%
        assert_eq!(out["max_drawdown_pct"], json!(10.0));
        // |99/110-1| = 10%, |104/99-1| = 5.0505…% → เฉลี่ย 7.5252… → 7.53
        assert_eq!(out["avg_daily_move_pct"], json!(7.53));
        // MA ไม่พอ → null และ trend ออกข้าง
        assert_eq!(out["ma"]["ma20_usd"], json!(null));
        assert_eq!(out["trend"], json!("ออกข้าง"));
        let raw = serde_json::to_string(&out).unwrap();
        assert!(
            !raw.contains("\"open\"") && !raw.contains("\"volume\""),
            "no raw bars: {raw}"
        );
    }

    /// realized_vol ตรวจกับสูตร (√252 × sample stdev ของ log returns, n−1)
    /// บน 22 closes ที่มี return ต่างกันแค่ 2 ตัว (realized_vol ใช้ 21 ตัวท้าย)
    #[test]
    fn behavior_volatility_matches_the_formula() {
        let mut closes = [100.0; 22];
        closes[20] = 110.0;
        closes[21] = 104.5;
        let bars: Vec<StockBar> = (0..22).map(|i| bar(&format!("d{i}"), closes[i])).collect();
        let out = summarize_price_behavior("t", "XYZ", "1m", &bars, 21).unwrap();

        let tail = &closes[1..];
        let rets: Vec<f64> = tail.windows(2).map(|w| (w[1] / w[0]).ln()).collect();
        let n = rets.len() as f64;
        let mean = rets.iter().sum::<f64>() / n;
        let var = rets.iter().map(|r| (r - mean) * (r - mean)).sum::<f64>() / (n - 1.0);
        let expected = (var.sqrt() * 252.0f64.sqrt() * 100.0 * 100.0).round() / 100.0;
        assert_eq!(out["volatility_pct"]["d20"], json!(expected));
        assert_eq!(out["volatility_pct"]["d60"], json!(null)); // ไม่พบ 61 closes
    }
}
