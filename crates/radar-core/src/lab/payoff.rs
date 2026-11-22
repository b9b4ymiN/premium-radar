//! CSP / Covered-Call payoff economics (PLAN-LAB §3.1)
//!
//! สูตร expiry สำหรับ standard deliverable (multiplier ยืนยันแล้วเท่านั้น) —
//! fees เป็นรายการทางเดินนั้นๆ (F) ไม่รวมก้อน, dividends/tax/FX แยกที่ ledger
//! ทุกฟังก์ชัน pure — invalid input คืน None/Err ไม่ออก NaN

use serde::Serialize;

/// ต้นทุน-ค่าใช้จ่าย versioned (ต่อขา per side, USD/contract) — Appendix B.4/C.1
/// ค่าที่ยังไม่ทราบ = None → รายงาน "หลังค่าใช้จ่ายที่ทราบ" เท่านั้น ห้ามตั้ง 0 เงียบๆ
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CostInputs {
    pub commission: f64,
    pub clearing: f64,
    pub orf: f64,
    /// เช่น 0.07 = 7% คิดบน commission
    pub vat_on_commission: f64,
    /// assignment/exercise ต่อเหตุการณ์ — None = ไม่ทราบ
    pub assignment_fee: Option<f64>,
}

impl Default for CostInputs {
    /// รุ่นแรกจาก webull.co.th/pricing (Appendix B.4) — provisional
    fn default() -> Self {
        Self {
            commission: 0.55,
            clearing: 0.025,
            orf: 0.05,
            vat_on_commission: 0.07,
            assignment_fee: None,
        }
    }
}

impl CostInputs {
    /// รวมค่าใช้จ่ายต่อสัญญาต่อขา (per side) ที่ทราบแน่
    pub fn per_side(&self) -> f64 {
        self.commission * (1.0 + self.vat_on_commission) + self.clearing + self.orf
    }
}

/// ขาเข้า (sell-to-open) 1 ขา
#[derive(Debug, Clone, Copy)]
pub struct ShortLeg {
    /// C = call (CC), P = put (CSP) — ใช้ OptType จาก quant
    pub is_call: bool,
    /// strike ต่อหุ้น
    pub k: f64,
    /// premium ต่อหุ้นที่ขายได้ (สุทธิต่อหุ้นก่อนค่า fee)
    pub c: f64,
    /// multiplier (ต้อง > 0 — validate แล้ว)
    pub m: f64,
}

pub type Pnl = f64;

/// P/L ที่ expiry ของขา short เดี่ยว (ไม่รวมหุ้น) ตาม S_T
pub fn leg_pnl_at(leg: ShortLeg, s_t: f64, fees: f64) -> Pnl {
    let intrinsic = if leg.is_call {
        (s_t - leg.k).max(0.0)
    } else {
        (leg.k - s_t).max(0.0)
    };
    leg.m * (leg.c - intrinsic) - fees
}

/// Break-even ของขา short จาก payoff inequality (§3.1: ไม่ยึดสูตรเดียว)
/// คืน None เมื่อไม่มีจุดที่กำไรเป็นศูนย์ในช่วงราคาที่เป็นไปได้
pub fn leg_breakeven(leg: ShortLeg, fees: f64) -> Option<f64> {
    let f = fees / leg.m;
    if leg.is_call {
        // กำไรเป็นศูนย์เมื่อ S_T = K + c − F/m และต้องอยู่ฝั่ง S_T > K
        let be = leg.k + leg.c - f;
        (be >= leg.k).then_some(be)
    } else {
        let be = leg.k - leg.c + f;
        (be <= leg.k).then_some(be)
    }
}

/// ผลลัพธ์วิเคราะห์สัญญาเดี่ยวแบบมีสรุปครบ
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LegAnalysis {
    pub max_profit: f64,
    /// CSP: ขาดทุนสูงสุดถ้าหุ้นไป 0 (คิด fee แล้ว) · Call leg เดี่ยว: ไม่มีขอบเขต → None
    pub max_loss: Option<f64>,
    pub breakeven: Option<f64>,
    /// ทุนที่ผูก (denominator): CSP = mK gross, CC option leg เดี่ยว = None
    pub collateral: Option<f64>,
}

pub fn analyze_leg(leg: ShortLeg, fees: f64) -> LegAnalysis {
    let max_profit = leg.m * leg.c - fees;
    let (max_loss, collateral) = if leg.is_call {
        (None, None)
    } else {
        (
            Some(-(leg.k * leg.m) + leg.m * leg.c - fees),
            Some(leg.k * leg.m),
        )
    };
    LegAnalysis {
        max_profit,
        max_loss,
        breakeven: leg_breakeven(leg, fees),
        collateral,
    }
}

/// Covered Call รวมหุ้น (ต้องมี cost basis B จึงรายงาน — §3.1)
#[derive(Debug, Clone, Copy)]
pub struct CoveredCall {
    pub leg: ShortLeg,
    /// cost basis ต่อหุ้นของหุ้นที่ถืออยู่ (หรือ S_0 กรณี "สมมติซื้อวันนี้")
    pub b: f64,
    /// ราคาหุ้น ณ วันคำนวณ (แสดงเปรียบเทียบทุนวันนี้ = m·S_0)
    pub s0: f64,
}

pub fn cc_pnl_at(cc: CoveredCall, s_t: f64, fees: f64) -> Pnl {
    let intrinsic = (s_t - cc.leg.k).max(0.0);
    cc.leg.m * (s_t - cc.b + cc.leg.c - intrinsic) - fees
}

/// สรุป CC: BE ปกติ = B − c + F/m · max profit = m(K − B + c) − F
/// (ค่าติดลบเป็นไปได้ — แสดงตามจริง)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CcAnalysis {
    pub max_profit: f64,
    pub max_loss: Option<f64>,
    pub breakeven: Option<f64>,
    /// ทุนเทียบวันนี้ (มูลค่าหุ้น ณ entry) — ตัวเทียบ yield ข้ามกลยุทธ์
    pub capital_today: f64,
}

pub fn analyze_cc(cc: CoveredCall, fees: f64) -> CcAnalysis {
    let f = fees / cc.leg.m;
    let max_profit = cc.leg.m * (cc.leg.k - cc.b + cc.leg.c) - fees;
    let max_loss = Some(cc.leg.m * (cc.b - cc.leg.c) - fees); // S_T = 0
    let be = cc.b - cc.leg.c + f;
    CcAnalysis {
        max_profit,
        max_loss,
        breakeven: Some(be),
        capital_today: cc.leg.m * cc.s0,
    }
}

/// ตาราง scenario ณ expiry สำหรับ UI (จุดสำคัญของ S_T)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScenarioRow {
    pub label: String,
    pub s_t: f64,
    pub pnl: Pnl,
}

/// จุด scenario มาตรฐาน: spot ±%, BE, strike
pub fn scenario_table(spot: f64, strike: f64, pnl_at: impl Fn(f64) -> Pnl) -> Vec<ScenarioRow> {
    let mut rows = Vec::new();
    for pct in [-10.0, -5.0, 0.0, 5.0, 10.0] {
        let s = spot * (1.0 + pct / 100.0);
        rows.push(ScenarioRow {
            label: format!("หุ้น {pct:+.0}% → {s:.2}"),
            s_t: s,
            pnl: pnl_at(s),
        });
    }
    rows.push(ScenarioRow {
        label: format!("ถึง strike {strike:.2}"),
        s_t: strike,
        pnl: pnl_at(strike),
    });
    rows.push(ScenarioRow {
        label: "หุ้น = 0".into(),
        s_t: 0.0,
        pnl: pnl_at(0.0),
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEES: f64 = 1.327; // round trip รุ่นแรก (Appendix C.1)

    #[test]
    fn csp_above_be_keeps_premium() {
        let leg = ShortLeg {
            is_call: false,
            k: 170.0,
            c: 2.0,
            m: 100.0,
        };
        // S_T = 175: ไม่ถูก exercise → กำไร = 200 − 1.327
        assert!((leg_pnl_at(leg, 175.0, FEES) - (200.0 - FEES)).abs() < 1e-9);
    }

    #[test]
    fn csp_below_strike_loses_linearly_and_be_matches_inequality() {
        let leg = ShortLeg {
            is_call: false,
            k: 170.0,
            c: 2.0,
            m: 100.0,
        };
        // S_T = 165: intrinsic 5 → −500 + 200 − fees
        assert!((leg_pnl_at(leg, 165.0, FEES) - (200.0 - 500.0 - FEES)).abs() < 1e-9);
        let be = leg_breakeven(leg, FEES).unwrap();
        assert!((be - (170.0 - 2.0 + FEES / 100.0)).abs() < 1e-9);
        // ที่ BE จริง P/L = 0 (ตรวจจาก inequality ไม่ใช่แค่สูตร)
        assert!(leg_pnl_at(leg, be, FEES).abs() < 1e-9);
    }

    #[test]
    fn csp_max_loss_and_gross_collateral() {
        let leg = ShortLeg {
            is_call: false,
            k: 170.0,
            c: 2.0,
            m: 100.0,
        };
        let a = analyze_leg(leg, FEES);
        assert_eq!(a.collateral, Some(17_000.0)); // mK gross ไม่หัก premium
        assert_eq!(a.max_profit, 200.0 - FEES);
        let Some(loss) = a.max_loss else { panic!() };
        assert!((loss - (-17_000.0 + 200.0 - FEES)).abs() < 1e-9);
        assert_eq!(leg_pnl_at(leg, 0.0, FEES), loss);
    }

    #[test]
    fn short_call_leg_be_above_strike() {
        let leg = ShortLeg {
            is_call: true,
            k: 50.0,
            c: 86.04,
            m: 100.0,
        };
        let be = leg_breakeven(leg, FEES).unwrap();
        assert!(be > 50.0);
        assert!(leg_pnl_at(leg, be, FEES).abs() < 1e-9);
        // S_T สูงมาก → ขาดทุนไม่มีขอบ (analyze รายงาน None)
        assert!(analyze_leg(leg, FEES).max_loss.is_none());
    }

    #[test]
    fn cc_combined_be_and_capped_profit() {
        let cc = CoveredCall {
            leg: ShortLeg {
                is_call: true,
                k: 180.0,
                c: 3.0,
                m: 100.0,
            },
            b: 170.0,
            s0: 176.0,
        };
        let a = analyze_cc(cc, FEES);
        assert_eq!(a.max_profit, 100.0 * (180.0 - 170.0 + 3.0) - FEES);
        assert_eq!(a.capital_today, 17_600.0);
        // BE: กรณีปกติ B − c + F/m; ตรวจ P(L)=0 จริงที่ BE (S_T < K segment)
        let be = a.breakeven.unwrap();
        assert!((be - (170.0 - 3.0 + FEES / 100.0)).abs() < 1e-9);
        assert!(cc_pnl_at(cc, be, FEES).abs() < 1e-9);
        // เหนือ strike: กำไรล็อกที่ max profit
        assert!((cc_pnl_at(cc, 200.0, FEES) - a.max_profit).abs() < 1e-9);
    }

    #[test]
    fn cc_negative_max_profit_shown_honestly() {
        // ซื้อหุ้นแพงกว่า strike+credit → สูตรให้ค่าติดลบ แสดงตามจริง
        let cc = CoveredCall {
            leg: ShortLeg {
                is_call: true,
                k: 100.0,
                c: 1.0,
                m: 100.0,
            },
            b: 150.0,
            s0: 150.0,
        };
        assert!(analyze_cc(cc, FEES).max_profit < 0.0);
    }

    #[test]
    fn scenario_table_covers_strike_and_zero() {
        let leg = ShortLeg {
            is_call: false,
            k: 170.0,
            c: 2.0,
            m: 100.0,
        };
        let rows = scenario_table(175.0, 170.0, |s| leg_pnl_at(leg, s, FEES));
        assert!(rows.iter().any(|r| r.s_t == 170.0));
        assert!(rows.iter().any(|r| r.s_t == 0.0));
        assert_eq!(rows.len(), 7);
    }

    #[test]
    fn cost_inputs_v1_per_side() {
        // 0.55×1.07 + 0.025 + 0.05 = 0.6635 (Appendix C.1)
        assert!((CostInputs::default().per_side() - 0.6635).abs() < 1e-9);
        assert!(CostInputs::default().assignment_fee.is_none());
    }
}
