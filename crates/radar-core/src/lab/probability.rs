//! Probability พร้อมชื่อเหตุการณ์เสมอ (PLAN-LAB §3.2)
//!
//! BS risk-neutral proxy — ไม่ใช่ empirical ที่พิสูจน์แล้ว, |delta| ห้ามใช้
//! เป็น assignment rate, p_touch เป็น approximation ต้อง clamp และระบุข้อจำกัด

use crate::quant::{OptType, bs_greeks, ncdf};

/// ผลลัพธ์ probability พร้อมชื่อเหตุการณ์ที่วัดจริง
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NamedProb {
    pub event_th: String,
    pub p: f64,
    /// ชื่อโมเดล/สมมติฐาน เช่น "BS risk-neutral · IV ของสัญญา"
    pub model_note: String,
}

fn d2_of(s: f64, k: f64, t: f64, r: f64, q: f64, sig: f64) -> f64 {
    let sig_t = sig * t.sqrt();
    ((s / k).ln() + (r - q - sig * sig / 2.0) * t) / sig_t
}

/// POP ของ CSP ถือถึง expiry = Pr(S_T > net BE) — เหตุการณ์ "กำไร"
/// (BE ใส่ fee ต่อหุ้นแล้ว; t ปี; คืน None เมื่อ input ใช้ไม่ได้)
pub fn csp_pop(spot: f64, breakeven: f64, t: f64, r: f64, q: f64, iv: f64) -> Option<NamedProb> {
    if spot <= 0.0 || breakeven <= 0.0 || t <= 0.0 || iv <= 0.0 {
        return None;
    }
    let d2 = d2_of(spot, breakeven, t, r, q, iv);
    Some(NamedProb {
        event_th: "ราคาหุ้นปิดสูงกว่าจุดคุ้มทุน ณ หมดอายุ (กำไร)".into(),
        p: ncdf(d2),
        model_note: "BS risk-neutral proxy · ยุโรป (ไม่รวม early exercise)".into(),
    })
}

/// POP ของ short call leg = Pr(S_T < leg BE)
pub fn short_call_pop(
    spot: f64,
    breakeven: f64,
    t: f64,
    r: f64,
    q: f64,
    iv: f64,
) -> Option<NamedProb> {
    if spot <= 0.0 || breakeven <= 0.0 || t <= 0.0 || iv <= 0.0 {
        return None;
    }
    let d2 = d2_of(spot, breakeven, t, r, q, iv);
    Some(NamedProb {
        event_th: "ราคาหุ้นปิดต่ำกว่าจุดคุ้มทุน ณ หมดอายุ (กำไร)".into(),
        p: 1.0 - ncdf(d2),
        model_note: "BS risk-neutral proxy · ยุโรป (ไม่รวม early exercise)".into(),
    })
}

/// Terminal ITM probability ของสัญญา — คนละเหตุการณ์กับ POP เสมอ
pub fn p_itm(spot: f64, k: f64, is_call: bool, t: f64, r: f64, q: f64, iv: f64) -> Option<f64> {
    if spot <= 0.0 || k <= 0.0 || t <= 0.0 || iv <= 0.0 {
        return None;
    }
    let d2 = d2_of(spot, k, t, r, q, iv);
    Some(if is_call { ncdf(d2) } else { 1.0 - ncdf(d2) })
}

/// p_touch ≈ 2·p_itm — approximation ตามต้นฉบับ Python, clamp [0,1]
/// พร้อมข้อจำกัดกำกับ (ไม่ใช่ calibrated path probability)
pub fn p_touch(p_itm: f64) -> NamedProb {
    NamedProb {
        event_th: "ราคาแตะระดับ strike ก่อนหมดอายุ (ประมาณ)".into(),
        p: (2.0 * p_itm).clamp(0.0, 1.0),
        model_note: "สูตรประมาณ 2×p_itm ของต้นฉบับ — ไม่ใช่ path probability ที่ calibration แล้ว".into(),
    }
}

/// delta จาก BS (เพื่อแสดงคู่กับ POP — ห้ามใช้แทน assignment rate)
pub fn leg_delta(spot: f64, k: f64, is_call: bool, t: f64, r: f64, q: f64, iv: f64) -> f64 {
    let cp = if is_call { OptType::Call } else { OptType::Put };
    bs_greeks(cp, spot, k, t, r, q, iv).delta
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: f64 = crate::quant::RISK_FREE_RATE;
    const Q: f64 = crate::quant::DIV_YIELD;

    #[test]
    fn deep_otm_put_pop_near_one_and_itm_near_zero() {
        // put ลึกมาก: BE ต่ำกว่า spot มาก → เกือบชนะ; ITM เกือบ 0
        let pop = csp_pop(176.0, 100.0, 0.05, R, Q, 0.5).unwrap();
        assert!(pop.p > 0.95, "p={}", pop.p);
        let itm = p_itm(176.0, 100.0, false, 0.05, R, Q, 0.5).unwrap();
        assert!(itm < 0.05, "itm={itm}");
        // ชื่อเหตุการณ์ต้องระบุ "จุดคุ้มทุน" ไม่ใช่แค่ ITM
        assert!(pop.event_th.contains("จุดคุ้มทุน"));
    }

    #[test]
    fn itm_put_pop_below_one_and_itm_above_zero() {
        let itm = p_itm(176.0, 170.0, false, 0.05, R, Q, 0.5).unwrap();
        assert!(itm > 0.3 && itm < 0.6);
        let pop = csp_pop(176.0, 168.0, 0.05, R, Q, 0.5).unwrap();
        assert!(pop.p > 0.5);
    }

    #[test]
    fn short_call_pop_complements_put_at_same_be() {
        let t = 0.05;
        let put_side = csp_pop(176.0, 180.0, t, R, Q, 0.4).unwrap().p;
        let call_side = short_call_pop(176.0, 180.0, t, R, Q, 0.4).unwrap().p;
        // Pr(S>180) + Pr(S<180) ≈ 1 (ไม่ใช่ 1 เป๊ะเพราะ continuous — ใกล้มาก)
        assert!((put_side + call_side - 1.0).abs() < 1e-9);
    }

    #[test]
    fn p_touch_clamped_and_labeled() {
        let t = p_touch(0.6);
        assert_eq!(t.p, 1.0f64); // 2×0.6 → clamp ที่ 1.0
        assert!(t.model_note.contains("ประมาณ"));
        assert_eq!(p_touch(0.1).p, 0.2);
    }

    #[test]
    fn invalid_inputs_return_none_never_panic() {
        assert!(csp_pop(0.0, 100.0, 0.05, R, Q, 0.5).is_none());
        assert!(csp_pop(176.0, 100.0, 0.0, R, Q, 0.5).is_none());
        assert!(csp_pop(176.0, 100.0, 0.05, R, Q, 0.0).is_none());
        assert!(p_itm(-1.0, 100.0, true, 0.05, R, Q, 0.5).is_none());
    }

    #[test]
    fn delta_consistent_with_itm_for_call() {
        let d = leg_delta(176.0, 170.0, true, 0.05, R, Q, 0.4);
        assert!(d > 0.0 && d < 1.0);
    }
}
