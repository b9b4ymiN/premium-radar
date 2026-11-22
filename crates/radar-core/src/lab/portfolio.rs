//! Capital simulator + position ledger (PLAN-LAB P11.3 + P13.1)
//!
//! สมุดจำลอง cash/shares — conservation ตรวจได้ทุกจุด, integer contracts,
//! ไม่มี margin/naked, equity = cash + หุ้น mark − option liability,
//! drawdown ตามนิยามเจ้าของ 30% (amendments §6)

use serde::Serialize;

/// สถานะพอร์ตจำลอง ณ เวลาหนึ่ง
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
pub struct PortfolioState {
    pub cash: f64,
    /// จำนวนหุ้น (หน่วยหุ้น — รวม fractional ไม่ได้: assignment มาเป็น m ต่อสัญญา)
    pub shares: f64,
    /// ต้นทุนรวมของหุ้นที่ถือ (สำหรับ mark P/L)
    pub stock_cost: f64,
    /// option เปิด: (สัญญา, ชนิด, strike, premium ต่อหุ้น, multiplier)
    pub open_options: usize,
}

/// เหตุการณ์เดียวของ ledger — ทุก event ตรวจ conservation ได้
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum LedgerEvent {
    /// ขาย CSP: รับ premium สุทธิ, ผูก collateral
    SellCsp {
        contracts: usize,
        k: f64,
        m: f64,
        net_credit: f64,
    },
    /// ขาย CC (ต้องมีหุ้นครอบคลุม): รับ premium
    SellCc {
        contracts: usize,
        k: f64,
        m: f64,
        net_credit: f64,
    },
    /// ปิด option (buy-to-close): จ่าย net debit
    CloseOption { cost: f64 },
    /// CSP โดน assigned ที่ expiry: เงินสด → หุ้น
    PutAssigned { contracts: usize, k: f64, m: f64 },
    /// CC โดน called away: หุ้น → เงินสดที่ strike
    CalledAway { contracts: usize, k: f64, m: f64 },
    /// option หมดอายุไร้ค่า: ปลดผูก
    ExpiredWorthless,
    /// mark หุ้น ณ ราคาปิด (สำหรับ equity)
    MarkStock { price: f64 },
}

/// ผลลัพธ์ apply event เดียว + สถานะใหม่
pub fn apply(state: &PortfolioState, ev: &LedgerEvent) -> Result<PortfolioState, String> {
    let mut s = *state;
    match ev {
        LedgerEvent::SellCsp {
            contracts,
            k,
            m,
            net_credit,
        } => {
            if *contracts == 0 {
                return Err("จำนวนสัญญาต้องเป็นจำนวนเต็มบวก".into());
            }
            let collateral = k * m * *contracts as f64;
            if s.cash < collateral {
                return Err(format!(
                    "เงินสด {cash:.2} ไม่พอผูก collateral {collateral:.2} (cash-secured เท่านั้น)",
                    cash = s.cash
                ));
            }
            s.cash += net_credit; // premium เข้า; collateral ยังเป็นเงินสดที่ผูก (ไม่หักสองครั้ง — ตรวจตอน assigned)
            s.open_options += contracts;
        }
        LedgerEvent::SellCc {
            contracts,
            k: _,
            m,
            net_credit,
        } => {
            let needed = m * *contracts as f64;
            if s.shares < needed {
                return Err(format!("หุ้น {sh} ไม่พอคุ้ม CC {needed}", sh = s.shares));
            }
            s.cash += net_credit;
            s.open_options += contracts;
        }
        LedgerEvent::CloseOption { cost } => {
            if s.cash < *cost {
                return Err("เงินสดไม่พอปิดสัญญา".into());
            }
            s.cash -= cost;
        }
        LedgerEvent::PutAssigned { contracts, k, m } => {
            let shares = m * *contracts as f64;
            let pay = k * shares;
            if s.cash < pay {
                return Err("เงินสดผูกไม่พอจ่ายรับหุ้น (conservation เสีย)".into());
            }
            s.cash -= pay;
            s.shares += shares;
            s.stock_cost += pay;
            s.open_options = s.open_options.saturating_sub(*contracts);
        }
        LedgerEvent::CalledAway { contracts, k, m } => {
            let shares = m * *contracts as f64;
            if s.shares < shares {
                return Err("หุ้นไม่พอส่งมอบ".into());
            }
            // ต้นทุนส่วนที่ขายไปหัดออกตามสัดส่วน
            let cost_out = s.stock_cost * (shares / s.shares);
            s.shares -= shares;
            s.stock_cost -= cost_out;
            s.cash += k * shares;
            s.open_options = s.open_options.saturating_sub(*contracts);
        }
        LedgerEvent::ExpiredWorthless => {
            s.open_options = s.open_options.saturating_sub(1);
        }
        LedgerEvent::MarkStock { .. } => {}
    }
    Ok(s)
}

/// Equity = cash + หุ้น mark-to-market (ราคาปัจจุบัน) − liability ของ option เปิด
/// liability ประมาณที่ expiry ด้วย intrinsic ตาม strike ถ่วงน้ำหนัก —
/// ผู้เรียกส่ง open intrinsic รวมมาแทนได้ (optionality ก่อน expiry ไม่อ้าง)
pub fn equity(state: &PortfolioState, stock_price: f64, open_intrinsic: f64) -> f64 {
    state.cash + state.shares * stock_price - open_intrinsic
}

/// Maximum drawdown ตามนิยามเจ้าของ: DD_t = 1 − Equity_t/Peak_t
/// คืน (mdd, ว่า risk unverified หรือไม่ — มีช่วง mark ขาด)
pub fn max_drawdown(equity_series: &[Option<f64>]) -> (Option<f64>, bool) {
    let mut peak = f64::NEG_INFINITY;
    let mut mdd = 0.0f64;
    let mut unverified = false;
    let mut saw_any = false;
    for e in equity_series {
        match e {
            Some(v) => {
                saw_any = true;
                peak = peak.max(*v);
                if peak > 0.0 {
                    mdd = mdd.max(1.0 - v / peak);
                }
            }
            None => unverified = true,
        }
    }
    (saw_any.then_some(mdd), unverified)
}

/// เกณฑ์เจ้าของ: 30% equity drawdown ceiling (เกิน = ไม่ผ่าน risk eligibility)
pub const DRAWDOWN_CEILING: f64 = 0.30;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RiskVerdict {
    pub mdd: Option<f64>,
    pub unverified: bool,
    /// None = ยังประเมินไม่ได้ (ไม่ใช่ผ่าน)
    pub passes: Option<bool>,
}

pub fn risk_check(equity_series: &[Option<f64>]) -> RiskVerdict {
    let (mdd, unverified) = max_drawdown(equity_series);
    let passes = match mdd {
        Some(d) if !unverified => Some(d <= DRAWDOWN_CEILING),
        _ => None,
    };
    RiskVerdict {
        mdd,
        unverified,
        passes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: f64 = 100.0;

    #[test]
    fn csp_cycle_win_conserves_cash() {
        let s0 = PortfolioState {
            cash: 20_000.0,
            ..Default::default()
        };
        let s1 = apply(
            &s0,
            &LedgerEvent::SellCsp {
                contracts: 1,
                k: 170.0,
                m: M,
                net_credit: 198.67,
            },
        )
        .unwrap();
        // หมดอายุไร้ค่า → เงิน = ตั้งต้น + credit
        let s2 = apply(&s1, &LedgerEvent::ExpiredWorthless).unwrap();
        assert!((s2.cash - 20_198.67).abs() < 1e-9);
        assert_eq!(s2.shares, 0.0);
    }

    #[test]
    fn put_assignment_converts_cash_to_shares() {
        let s0 = PortfolioState {
            cash: 20_000.0,
            ..Default::default()
        };
        let s1 = apply(
            &s0,
            &LedgerEvent::SellCsp {
                contracts: 1,
                k: 170.0,
                m: M,
                net_credit: 200.0,
            },
        )
        .unwrap();
        let s2 = apply(
            &s1,
            &LedgerEvent::PutAssigned {
                contracts: 1,
                k: 170.0,
                m: M,
            },
        )
        .unwrap();
        assert!((s2.cash - (20_000.0 + 200.0 - 17_000.0)).abs() < 1e-9);
        assert_eq!(s2.shares, 100.0);
        assert!((s2.stock_cost - 17_000.0).abs() < 1e-9);
        // equity ที่ราคาเดิม: cash + หุ้น − liability 0
        let eq = equity(&s2, 170.0, 0.0);
        assert!((eq - (s2.cash + 100.0 * 170.0)).abs() < 1e-9);
    }

    #[test]
    fn called_away_exchanges_shares_for_strike_cash() {
        let s0 = PortfolioState {
            cash: 1_000.0,
            shares: 100.0,
            stock_cost: 17_000.0,
            ..Default::default()
        };
        let s1 = apply(
            &s0,
            &LedgerEvent::SellCc {
                contracts: 1,
                k: 180.0,
                m: M,
                net_credit: 300.0,
            },
        )
        .unwrap();
        let s2 = apply(
            &s1,
            &LedgerEvent::CalledAway {
                contracts: 1,
                k: 180.0,
                m: M,
            },
        )
        .unwrap();
        assert_eq!(s2.shares, 0.0);
        assert!((s2.cash - (1_000.0 + 300.0 + 18_000.0)).abs() < 1e-9);
        assert!((s2.stock_cost - 0.0).abs() < 1e-9);
    }

    #[test]
    fn naked_or_insufficient_blocked_with_thai_reason() {
        let s0 = PortfolioState {
            cash: 5_000.0,
            ..Default::default()
        };
        let err = apply(
            &s0,
            &LedgerEvent::SellCsp {
                contracts: 1,
                k: 170.0,
                m: M,
                net_credit: 200.0,
            },
        )
        .unwrap_err();
        assert!(err.contains("ไม่พอผูก collateral"), "{err}");
        // CC ไม่มีหุ้น = ห้าม
        let err2 = apply(
            &s0,
            &LedgerEvent::SellCc {
                contracts: 1,
                k: 50.0,
                m: M,
                net_credit: 1.0,
            },
        )
        .unwrap_err();
        assert!(err2.contains("ไม่พอคุ้ม"));
    }

    #[test]
    fn drawdown_definition_and_ceiling() {
        // equity 100 → 120 → 85 (DD = 1−85/120 ≈ 0.2917) → ผ่าน; ขอบ 30% เป๊ะ
        // ทดสอบไม่ได้ด้วย FP — amendments §6: เท่ากับ 30% = อยู่ที่ขอบ ไม่มี buffer
        let series = vec![Some(100.0), Some(120.0), Some(85.0)];
        let v = risk_check(&series);
        assert!((v.mdd.unwrap() - (1.0 - 85.0 / 120.0)).abs() < 1e-12);
        assert_eq!(v.passes, Some(true));
        // ลึกกว่า ceiling = ไม่ผ่าน
        let series2 = vec![Some(100.0), Some(120.0), Some(83.0)];
        assert_eq!(risk_check(&series2).passes, Some(false));
    }

    #[test]
    fn missing_marks_make_risk_unverified_not_pass() {
        let series = vec![Some(100.0), None, Some(101.0)];
        let v = risk_check(&series);
        assert!(v.unverified);
        assert_eq!(v.passes, None, "gap ใน marks ห้ามสรุปว่าผ่าน");
    }
}
