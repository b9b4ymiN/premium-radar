//! Replay engine (PLAN-LAB P11) — event-time ย้อนข้อมูล Lab archive
//!
//! หลักการ: decision ใช้เฉพาะข้อมูลที่ available ณ เวลานั้น (no look-ahead)
//! execution = bid-entry/ask-exit จาก quotes ที่สังเกตได้เท่านั้น; แท่งวันปิด
//! เริ่มใช้ได้หลังเวลาปิด (quality::bar_available_at) · costs ต่อเหตุการณ์

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::payoff::{ShortLeg, leg_pnl_at};
use super::portfolio::{LedgerEvent, OpenOption, PortfolioState, apply};
use super::types::{ObsKind, Observation};

/// กติกา exit ของ replay รุ่นแรก
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum ExitRule {
    HoldToExpiry,
}

/// ผลลัพธ์ replay หนึ่งสัญญา
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TradeOutcome {
    pub occ: String,
    pub entry_available: bool,
    /// เหตุผลไทยเมื่อเล่นไม่ได้
    pub blocked_reason: Option<String>,
    pub entry_time: Option<DateTime<Utc>>,
    pub exit_time: Option<DateTime<Utc>>,
    /// P/L สุทธิ (หลัง fee ตามเหตุการณ์ที่เกิด)
    pub pnl: Option<f64>,
    /// S_T ที่ใช้ตัดสิน expiry (ราคา underlying ณ available หลังปิดวันสุดท้าย)
    pub s_t: Option<f64>,
}

/// ข้อมูลที่ decision-time ใช้ได้: observation เดียวของสัญญา + ราคาหุ้น
/// ที่ available ก่อนเวลาตัดสินใจ (look-ahead filter อยู่ที่ caller ส่งชุดที่กรองแล้ว
/// — ฟังก์ชันนี้ตรวจซ้ำด้วย available_at)
pub struct ReplayInput<'a> {
    pub leg: ShortLeg,
    pub occ: String,
    /// สัญญา option observation ณ ตอน entry (available_at ≤ decision_time)
    pub entry_quote: Option<&'a Observation>,
    /// underlying observation ที่ใช้เป็น S_T ตอน expiry (available_at หลังปิดวันสุดท้าย)
    pub expiry_stock: Option<&'a Observation>,
    pub decision_time: DateTime<Utc>,
    pub expiry_close_time: DateTime<Utc>,
    /// fee ต่อเหตุการณ์ (entry / exit)
    pub fee_per_side: f64,
    /// ทุนตั้งต้นของ ledger จำลอง (CSP ต้องมีพอผูก)
    pub cash: f64,
}

pub fn replay_single(input: &ReplayInput) -> TradeOutcome {
    let mut out = TradeOutcome {
        occ: input.occ.clone(),
        entry_available: false,
        blocked_reason: None,
        entry_time: None,
        exit_time: None,
        pnl: None,
        s_t: None,
    };
    // look-ahead: entry quote ต้อง available ก่อน decision
    let Some(q) = input.entry_quote else {
        out.blocked_reason = Some("ไม่มี quote ของสัญญา ณ เวลาตัดสินใจ".into());
        return out;
    };
    if q.available_at > input.decision_time {
        out.blocked_reason = Some("quote ยังไม่ available ณ เวลาตัดสินใจ (look-ahead)".into());
        return out;
    }
    // ราคา underlying ตอน expiry ต้อง available หลังปิดวันสุดท้ายจริง
    let Some(st_obs) = input.expiry_stock else {
        out.blocked_reason = Some("ไม่มีราคา underlying หลังปิดวันหมดอายุ — unresolved".into());
        return out;
    };
    if st_obs.available_at < input.expiry_close_time {
        out.blocked_reason = Some("ราคา underlying ยังไม่พร้อมก่อนปิดวันหมดอายุ (look-ahead)".into());
        return out;
    }
    let s_t = match &st_obs.kind {
        ObsKind::StockQuote { price, .. } => *price,
        _ => None,
    };
    let Some(s_t) = s_t else {
        out.blocked_reason = Some("ราคา underlying เป็น missing".into());
        return out;
    };
    out.entry_available = true;
    out.entry_time = Some(input.decision_time);
    out.exit_time = Some(st_obs.available_at);
    out.s_t = Some(s_t);

    // ledger: ขาย → หมดอายุ (hold-to-expiry อย่างเดียวในรุ่นแรก)
    let mut ledger = PortfolioState {
        cash: input.cash,
        ..Default::default()
    };
    let Ok(after_sell) = apply(
        &ledger,
        &LedgerEvent::SellCsp {
            contracts: 1,
            k: input.leg.k,
            m: input.leg.m,
            net_credit: input.leg.c * input.leg.m - input.fee_per_side,
        },
    ) else {
        out.blocked_reason = Some("เงินจำลองไม่พอผูก collateral".into());
        return out;
    };
    ledger = after_sell;
    // P/L ที่ expiry จาก payoff (fee exit: expiry ไร้ค่า = ไม่มีค่า fee ปิด
    // ตามแผน §3.4 — expiry ≠ buy-to-close)
    let pnl = leg_pnl_at(input.leg, s_t, input.fee_per_side);
    // จำลอง outcome ให้ ledger ครบวง (worthless หรือ assigned)
    let ev = if input.leg.is_call {
        if s_t > input.leg.k {
            LedgerEvent::CalledAway {
                contracts: 1,
                k: input.leg.k,
                m: input.leg.m,
            }
        } else {
            LedgerEvent::ExpiredWorthless {
                opt: OpenOption {
                    is_call: true,
                    k: input.leg.k,
                    m: input.leg.m,
                },
            }
        }
    } else if s_t < input.leg.k {
        LedgerEvent::PutAssigned {
            contracts: 1,
            k: input.leg.k,
            m: input.leg.m,
        }
    } else {
        LedgerEvent::ExpiredWorthless {
            opt: OpenOption {
                is_call: false,
                k: input.leg.k,
                m: input.leg.m,
            },
        }
    };
    if let Ok(after) = apply(&ledger, &ev) {
        ledger = after;
    }
    out.pnl = Some(pnl);
    let _ = ledger; // conservation ตรวจใน portfolio tests
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::types::{ContractRef, ObsKind};
    use crate::quant::OptType;
    use chrono::NaiveDate;

    fn contract(occ: &str, is_call: bool) -> ContractRef {
        let m = crate::webull::contracts::parse_occ(occ).unwrap();
        ContractRef {
            occ: occ.into(),
            underlying: m.underlying,
            expiry: m.expiry,
            option_type: if is_call { OptType::Call } else { OptType::Put },
            strike: m.strike,
            multiplier: 100.0,
        }
    }

    fn stock_contract() -> ContractRef {
        ContractRef {
            occ: "NVDA".into(),
            underlying: "NVDA".into(),
            expiry: NaiveDate::from_ymd_opt(2026, 12, 4).unwrap(),
            option_type: OptType::Call,
            strike: 0.0,
            multiplier: 1.0,
        }
    }

    fn obs(contract: ContractRef, price: f64, available: &str) -> Observation {
        Observation {
            contract,
            kind: ObsKind::StockQuote {
                price: Some(price),
                open: None,
                high: None,
                low: None,
                pre_close: None,
                volume: None,
            },
            session_date: NaiveDate::from_ymd_opt(2026, 11, 20).unwrap(),
            event_time: available.parse().unwrap(),
            received_time: available.parse().unwrap(),
            available_at: available.parse().unwrap(),
            source: crate::engine::DataSource::Rest,
            backfill: true,
            universe_id: None,
        }
    }

    fn quote_obs(contract: ContractRef, bid: f64, available: &str) -> Observation {
        Observation {
            contract,
            kind: ObsKind::OptionQuote {
                price: Some(bid),
                bid: Some(bid),
                ask: Some(bid + 0.1),
                iv: None,
                delta: None,
                oi: None,
                volume: None,
            },
            session_date: NaiveDate::from_ymd_opt(2026, 11, 20).unwrap(),
            event_time: available.parse().unwrap(),
            received_time: available.parse().unwrap(),
            available_at: available.parse().unwrap(),
            source: crate::engine::DataSource::Rest,
            backfill: false,
            universe_id: None,
        }
    }

    const FEE: f64 = 0.6635;

    #[test]
    fn csp_win_replay_with_ledger() {
        let leg = ShortLeg {
            is_call: false,
            k: 170.0,
            c: 2.0,
            m: 100.0,
        };
        let c = contract("NVDA261204P00170000", false);
        let q = quote_obs(c.clone(), 2.0, "2026-11-20T21:00:00Z");
        let st = obs(stock_contract(), 175.0, "2026-12-04T21:00:00Z");
        let out = replay_single(&ReplayInput {
            leg,
            occ: c.occ.clone(),
            entry_quote: Some(&q),
            expiry_stock: Some(&st),
            decision_time: "2026-11-20T22:00:00Z".parse().unwrap(),
            expiry_close_time: "2026-12-04T21:00:00Z".parse().unwrap(),
            fee_per_side: FEE,
            cash: 20_000.0,
        });
        assert!(out.blocked_reason.is_none());
        assert_eq!(out.s_t, Some(175.0));
        // ชนะเต็ม: 200 − fee entry
        assert!((out.pnl.unwrap() - (200.0 - FEE)).abs() < 1e-9);
    }

    #[test]
    fn lookahead_entry_blocked_with_thai_reason() {
        let leg = ShortLeg {
            is_call: false,
            k: 170.0,
            c: 2.0,
            m: 100.0,
        };
        let c = contract("NVDA261204P00170000", false);
        // quote available หลัง decision → ต้องปฏิเสธ
        let q = quote_obs(c.clone(), 2.0, "2026-11-21T02:00:00Z");
        let st = obs(stock_contract(), 175.0, "2026-12-04T21:00:00Z");
        let out = replay_single(&ReplayInput {
            leg,
            occ: c.occ.clone(),
            entry_quote: Some(&q),
            expiry_stock: Some(&st),
            decision_time: "2026-11-20T22:00:00Z".parse().unwrap(),
            expiry_close_time: "2026-12-04T21:00:00Z".parse().unwrap(),
            fee_per_side: FEE,
            cash: 20_000.0,
        });
        assert!(
            out.blocked_reason
                .as_deref()
                .unwrap()
                .contains("look-ahead")
        );
        assert!(out.pnl.is_none());
    }

    #[test]
    fn missing_expiry_price_is_unresolved_not_loss() {
        let leg = ShortLeg {
            is_call: false,
            k: 170.0,
            c: 2.0,
            m: 100.0,
        };
        let c = contract("NVDA261204P00170000", false);
        let q = quote_obs(c.clone(), 2.0, "2026-11-20T21:00:00Z");
        let out = replay_single(&ReplayInput {
            leg,
            occ: c.occ.clone(),
            entry_quote: Some(&q),
            expiry_stock: None,
            decision_time: "2026-11-20T22:00:00Z".parse().unwrap(),
            expiry_close_time: "2026-12-04T21:00:00Z".parse().unwrap(),
            fee_per_side: FEE,
            cash: 20_000.0,
        });
        assert!(
            out.blocked_reason
                .as_deref()
                .unwrap()
                .contains("unresolved")
        );
        assert!(out.pnl.is_none(), "ห้ามนับเป็นขาดทุนเมื่อไม่มีข้อมูล");
    }

    #[test]
    fn csp_assigned_replay_pnl_reflects_stock_below_strike() {
        let leg = ShortLeg {
            is_call: false,
            k: 170.0,
            c: 2.0,
            m: 100.0,
        };
        let c = contract("NVDA261204P00170000", false);
        let q = quote_obs(c.clone(), 2.0, "2026-11-20T21:00:00Z");
        let st = obs(stock_contract(), 165.0, "2026-12-04T21:00:00Z");
        let out = replay_single(&ReplayInput {
            leg,
            occ: c.occ.clone(),
            entry_quote: Some(&q),
            expiry_stock: Some(&st),
            decision_time: "2026-11-20T22:00:00Z".parse().unwrap(),
            expiry_close_time: "2026-12-04T21:00:00Z".parse().unwrap(),
            fee_per_side: FEE,
            cash: 20_000.0,
        });
        // intrinsic 5 → −500 + 200 − fee
        assert!((out.pnl.unwrap() - (200.0 - 500.0 - FEE)).abs() < 1e-9);
    }
}
