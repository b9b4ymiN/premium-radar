//! Strategy comparison — 18-config grid + สถานะต่อ config (PLAN-LAB P12 + amendments §5)
//!
//! engine readiness แยกจาก evidence readiness: ทุก config รายงานสถานะ
//! ตามข้อมูลที่มีจริง ไม่ใช่ผลตอบแทน 0 และไม่มี winner badge ก่อนพ้น gate

use serde::Serialize;

/// Delta band ตาม filters เดิมของ Radar (baseline freeze ตาม §5)
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum DeltaBand {
    Safe,
    Balanced,
    Aggressive,
}

/// กติกาออก (exit) ของ grid รุ่นแรก
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum ExitRule {
    HoldToExpiry,
    CloseAt50Credit,
}

/// หนึ่ง configuration ของ sweep รุ่นแรก = 3 band × 3 ช่วง DTE × 2 exit = 18
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExperimentConfig {
    pub band: DeltaBand,
    pub dte_range: (u16, u16),
    pub exit: ExitRule,
}

pub fn grid_v1() -> Vec<ExperimentConfig> {
    let bands = [DeltaBand::Safe, DeltaBand::Balanced, DeltaBand::Aggressive];
    let dtes = [(7u16, 14u16), (15, 30), (31, 60)];
    let mut out = Vec::with_capacity(18);
    for band in bands {
        for dte in dtes {
            for exit in [ExitRule::HoldToExpiry, ExitRule::CloseAt50Credit] {
                out.push(ExperimentConfig {
                    band,
                    dte_range: dte,
                    exit,
                });
            }
        }
    }
    out
}

/// สถานะ evidence ต่อ config — ไม่ใช่ผลตอบแทน (amendments §5)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ConfigStatus {
    /// มีข้อมูลพอเริ่มเห็นผลเบื้องต้น (ยัง "เบื้องต้น ยังไม่ยืนยัน")
    Supported { entries: usize, resolved: usize },
    /// สัญญายังไม่หมดอายุ — รอ outcome ตาม calendar time
    AwaitingOutcomes { entries: usize },
    /// ยังไม่มี/ข้อมูลไม่พอ — ไม่เท่ากับแพ้
    CoverageInsufficient,
    /// กติกาต้องการ field ที่ยังไม่มีในข้อมูล (เช่น path ราคาระหว่างวันของ exit 50%)
    MissingField(&'static str),
}

/// ตัดสินสถานะจากจำนวน entry/resolved ของ config นั้น
pub fn status_for(config: &ExperimentConfig, entries: usize, resolved: usize) -> ConfigStatus {
    if entries == 0 {
        return ConfigStatus::CoverageInsufficient;
    }
    // exit 50% ต้องการการสังเกตราคาระหว่างวัน — ตอนนี้เรามีเฉพาะราคาปิดรายวัน
    if config.exit == ExitRule::CloseAt50Credit {
        return ConfigStatus::MissingField("intraday path — ยังเก็บเฉพาะราคาปิด (post-close)");
    }
    if resolved == 0 {
        return ConfigStatus::AwaitingOutcomes { entries };
    }
    ConfigStatus::Supported { entries, resolved }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_has_18_frozen_configs() {
        let g = grid_v1();
        assert_eq!(g.len(), 18);
        // ไม่มี config ซ้ำ
        let set: std::collections::BTreeSet<String> =
            g.iter().map(|c| format!("{:?}", c)).collect();
        assert_eq!(set.len(), 18);
    }

    #[test]
    fn statuses_honest_no_early_winner() {
        let cfg = &grid_v1()[0];
        assert!(matches!(
            status_for(cfg, 0, 0),
            ConfigStatus::CoverageInsufficient
        ));
        assert!(matches!(
            status_for(cfg, 5, 0),
            ConfigStatus::AwaitingOutcomes { entries: 5 }
        ));
        assert!(matches!(
            status_for(cfg, 5, 3),
            ConfigStatus::Supported { .. }
        ));
        // exit 50% ติด missing-field จนกว่าจะมี intraday
        let cc = grid_v1()
            .into_iter()
            .find(|c| c.exit == ExitRule::CloseAt50Credit)
            .unwrap();
        assert!(matches!(
            status_for(&cc, 50, 40),
            ConfigStatus::MissingField(_)
        ));
    }
}
