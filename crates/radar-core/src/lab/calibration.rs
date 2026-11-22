//! Accuracy journal — calibration ของ prediction ที่บันทึกไว้ (PLAN-LAB P10 + §3.3)
//!
//! นับเฉพาะ matured/resolved; แสดง unresolved เสมอ; สัญญาพร้อมกันไม่ใช่
//! ตัวอย่างอิสระ (แสดง clusters); guardrails 30/100/20 เป็น display guardrail

use serde::Serialize;

/// Prediction ที่บันทึก (freeze) ไว้รอผล
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Prediction {
    pub id: String,
    /// ความน่าจะเป็นที่โมเดลทำนาย (0..1)
    pub p: f64,
    /// วันที่ทำนาย (ใช้นับ entry-day clusters)
    pub entry_day: chrono::NaiveDate,
    /// สัญญาที่ทำนาย (กลุ่มสัญญาพร้อมวันไม่อิสระ)
    pub occ: String,
}

/// ผลลัพธ์การ resolve: resolved เท่านั้นที่เข้า score; ที่เหลือนับ coverage
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Outcome {
    /// เหตุการณ์เกิด (y=1) / ไม่เกิด (y=0) — ตามนิยาม event เดียวกับ prediction
    Resolved(bool),
    /// หมดอายุแล้วแต่ไม่มีข้อมูลยืนยัน endpoint → ห้ามเดา
    Unresolved,
}

/// Brier = mean((p − y)²) ยิ่งต่ำยิ่งดี — เฉพาะ resolved
pub fn brier_score(predictions: &[Prediction], outcomes: &[Outcome]) -> Option<f64> {
    let pairs: Vec<(f64, f64)> = predictions
        .iter()
        .zip(outcomes)
        .filter_map(|(p, o)| match o {
            Outcome::Resolved(y) => Some((p.p, if *y { 1.0 } else { 0.0 })),
            Outcome::Unresolved => None,
        })
        .collect();
    if pairs.is_empty() {
        return None;
    }
    Some(pairs.iter().map(|(p, y)| (p - y) * (p - y)).sum::<f64>() / pairs.len() as f64)
}

/// Base-rate predictor เทียบ (เรียนจาก resolved เท่านั้น) — สำหรับบอกว่า
/// โมเดลดีกว่าการเดา "อัตราพื้นฐาน" หรือไม่
pub fn brier_base_rate(predictions: &[Prediction], outcomes: &[Outcome]) -> Option<f64> {
    let pairs: Vec<(f64, f64)> = predictions
        .iter()
        .zip(outcomes)
        .filter_map(|(p, o)| match o {
            Outcome::Resolved(y) => Some((p.p, if *y { 1.0 } else { 0.0 })),
            Outcome::Unresolved => None,
        })
        .collect();
    if pairs.is_empty() {
        return None;
    }
    let base = pairs.iter().map(|(_, y)| y).sum::<f64>() / pairs.len() as f64;
    Some(
        pairs
            .iter()
            .map(|(_, y)| (base - y) * (base - y))
            .sum::<f64>()
            / pairs.len() as f64,
    )
}

/// Bin คงที่ 0–10%…90–100% พร้อม Wilson 95% interval
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReliabilityBin {
    pub bin_lo: f64,
    pub bin_hi: f64,
    pub n: usize,
    pub mean_predicted: f64,
    pub observed_rate: f64,
    pub wilson_lo: f64,
    pub wilson_hi: f64,
    pub too_few: bool, // < 30 resolved
}

/// Wilson score interval 95%
fn wilson_95(successes: f64, n: f64) -> (f64, f64) {
    if n == 0.0 {
        return (0.0, 1.0);
    }
    let z = 1.959_964;
    let p = successes / n;
    let denom = 1.0 + z * z / n;
    let centre = (p + z * z / (2.0 * n)) / denom;
    let half = z * ((p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt()) / denom;
    ((centre - half).max(0.0), (centre + half).min(1.0))
}

pub fn reliability_bins(predictions: &[Prediction], outcomes: &[Outcome]) -> Vec<ReliabilityBin> {
    let mut bins = Vec::new();
    for i in 0..10 {
        let lo = i as f64 / 10.0;
        let hi = (i + 1) as f64 / 10.0;
        let in_bin: Vec<(f64, bool)> = predictions
            .iter()
            .zip(outcomes)
            .filter_map(|(p, o)| match o {
                Outcome::Resolved(y) => {
                    (p.p >= lo && (p.p < hi || (i == 9 && p.p <= 1.0))).then_some((p.p, *y))
                }
                Outcome::Unresolved => None,
            })
            .collect();
        let n = in_bin.len();
        if n == 0 {
            continue;
        }
        let successes = in_bin.iter().filter(|(_, y)| *y).count() as f64;
        let (wlo, whi) = wilson_95(successes, n as f64);
        bins.push(ReliabilityBin {
            bin_lo: lo,
            bin_hi: hi,
            n,
            mean_predicted: in_bin.iter().map(|(p, _)| p).sum::<f64>() / n as f64,
            observed_rate: successes / n as f64,
            wilson_lo: wlo,
            wilson_hi: whi,
            too_few: n < 30,
        });
    }
    bins
}

/// สรุปพร้อม guardrails (§3.3): ต่ำกว่าเกณฑ์ = "ยังสรุปไม่ได้" ไม่ใช่ผ่าน
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalibrationReport {
    pub resolved: usize,
    pub unresolved: usize,
    pub brier: Option<f64>,
    pub brier_base_rate: Option<f64>,
    pub bins: Vec<ReliabilityBin>,
    pub entry_day_clusters: usize,
    /// ยังสรุปไม่ได้ถ้า resolved < 100 หรือ clusters < 20 (display guardrail)
    pub conclusive: bool,
}

pub fn calibration_report(predictions: &[Prediction], outcomes: &[Outcome]) -> CalibrationReport {
    let resolved = outcomes
        .iter()
        .filter(|o| matches!(o, Outcome::Resolved(_)))
        .count();
    let unresolved = outcomes.len() - resolved;
    let clusters = predictions
        .iter()
        .map(|p| p.entry_day)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    CalibrationReport {
        resolved,
        unresolved,
        brier: brier_score(predictions, outcomes),
        brier_base_rate: brier_base_rate(predictions, outcomes),
        bins: reliability_bins(predictions, outcomes),
        entry_day_clusters: clusters,
        conclusive: resolved >= 100 && clusters >= 20,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pred(i: usize, p: f64, day: u32) -> Prediction {
        Prediction {
            id: format!("p{i}"),
            p,
            entry_day: chrono::NaiveDate::from_ymd_opt(2026, 11, day).unwrap(),
            occ: format!("NVDA261204P{:08}", 17000 + i),
        }
    }

    #[test]
    fn brier_hand_calculated() {
        // p=[0.8,0.4], y=[1,0] → (0.04+0.16)/2 = 0.1
        let preds = vec![pred(0, 0.8, 20), pred(1, 0.4, 20)];
        let outs = vec![Outcome::Resolved(true), Outcome::Resolved(false)];
        assert!((brier_score(&preds, &outs).unwrap() - 0.1).abs() < 1e-12);
    }

    #[test]
    fn unresolved_excluded_but_counted() {
        let preds = vec![pred(0, 0.8, 20), pred(1, 0.4, 21), pred(2, 0.6, 21)];
        let outs = vec![
            Outcome::Resolved(true),
            Outcome::Unresolved,
            Outcome::Resolved(false),
        ];
        let r = calibration_report(&preds, &outs);
        assert_eq!((r.resolved, r.unresolved), (2, 1));
        assert_eq!(r.entry_day_clusters, 2);
        // Brier คิดจาก 2 ตัว resolved เท่านั้น
        assert!(
            (r.brier.unwrap() - ((0.8 - 1.0f64).powi(2) + (0.6 - 0.0f64).powi(2)) / 2.0).abs()
                < 1e-12
        );
    }

    #[test]
    fn base_rate_comparison() {
        // ทำนาย perfect → brier 0 < base-rate
        let preds: Vec<_> = (0..10)
            .map(|i| pred(i, if i < 5 { 0.99 } else { 0.01 }, 20))
            .collect();
        let outs: Vec<_> = (0..10).map(|i| Outcome::Resolved(i < 5)).collect();
        let r = calibration_report(&preds, &outs);
        assert!(r.brier.unwrap() < r.brier_base_rate.unwrap());
    }

    #[test]
    fn guardrail_blocks_small_samples() {
        let preds = vec![pred(0, 0.8, 20)];
        let outs = vec![Outcome::Resolved(true)];
        let r = calibration_report(&preds, &outs);
        assert!(!r.conclusive, "ตัวอย่างน้อยต้องยังสรุปไม่ได้");
        assert_eq!(r.bins.len(), 1);
        assert!(r.bins[0].too_few);
    }

    #[test]
    fn wilson_interval_spans_observed() {
        let (lo, hi) = wilson_95(8.0, 10.0);
        assert!(lo < 0.8 && hi > 0.8);
        assert!(lo >= 0.0 && hi <= 1.0);
    }
}
