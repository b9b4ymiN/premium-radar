//! Option Lab core (PLAN-LAB.md Phase 8) — data contract + quality checks.
//!
//! Pure Rust, no IO: ทุก function deterministic เพื่อ golden-test ได้
//! storage/replay อยู่ใน phase ถัดไป (P8.4 capture, P11 replay)

pub mod capture;
pub mod quality;
pub mod storage;
pub mod types;
