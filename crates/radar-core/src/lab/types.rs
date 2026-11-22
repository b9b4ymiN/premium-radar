//! Lab data contract — versioned schemas (PLAN-LAB.md §2 + amendments §2–3)
//!
//! หลักการ: ค่าที่ไม่ทราบเป็น `None` เสมอ ห้ามใช้ 0 แทน missing
//! เวลาแยก 4 ชั้นตาม amendments: event (ตลาด) / received (เครื่องเราเห็น)
//! / session (วันเทรด NY) / available_at (เริ่มใช้เป็น feature ได้)

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::engine::DataSource;
use crate::quant::OptType;

/// Schema version — เปลี่ยนโครงสร้าง = เพิ่มเลขเวอร์ชัน ไม่แก้ย้อนหลัง
pub const LAB_SCHEMA_VERSION: u32 = 1;

/// Contract identity ที่ผ่าน OCC parse แล้ว + multiplier ที่ยืนยันได้
/// (รองรับ standard 100 ก่อนตามแผน §3.1 — nonstandard ต้องถูก block ไม่ใช่เดา)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractRef {
    /// OCC symbol ไม่มีช่องว่าง เช่น `NVDA261204P00175000`
    pub occ: String,
    pub underlying: String,
    pub expiry: NaiveDate,
    pub option_type: OptType,
    /// Strike ในหน่วยดอลลาร์
    pub strike: f64,
    /// ต่อสัญญา (standard = 100) — 0 = ไม่ทราบ จะถูก validate ปฏิเสธ
    pub multiplier: f64,
}

/// ชนิดข้อมูลตลาด — field ใดไม่มีใน payload เป็น `None`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ObsKind {
    OptionQuote {
        price: Option<f64>,
        bid: Option<f64>,
        ask: Option<f64>,
        iv: Option<f64>,
        delta: Option<f64>,
        oi: Option<f64>,
        volume: Option<f64>,
    },
    OptionBar {
        open: Option<f64>,
        high: Option<f64>,
        low: Option<f64>,
        close: Option<f64>,
        volume: Option<f64>,
    },
    StockQuote {
        price: Option<f64>,
        open: Option<f64>,
        high: Option<f64>,
        low: Option<f64>,
        /// ราคาปิดของ session ก่อน — เชื้อเพลิงตัดสิน outcome วันหมดอายุ
        /// (เก็บเช้าวันรุ่งขึ้นก็ได้ราคาปิดเมื่อวานจาก field นี้)
        pre_close: Option<f64>,
        volume: Option<f64>,
    },
}

impl ObsKind {
    /// discriminant สำหรับ dedupe (คนละชนิด = คนละ observation)
    pub fn kind_tag(&self) -> &'static str {
        match self {
            ObsKind::OptionQuote { .. } => "quote",
            ObsKind::OptionBar { .. } => "bar",
            ObsKind::StockQuote { .. } => "stock",
        }
    }
}

/// จุดข้อมูลตลาดหนึ่งจุดพร้อม provenance ครบ 4 ชั้นเวลา
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub contract: ContractRef,
    pub kind: ObsKind,
    /// เวลาที่ข้อมูลอ้างถึงในตลาด (UTC)
    pub event_time: DateTime<Utc>,
    /// เวลาที่ระบบเราได้เห็นข้อมูล (UTC) — ต้องไม่ก่อน event_time
    pub received_time: DateTime<Utc>,
    /// วันเทรด NY ที่จุดข้อมูลนี้สังกัด
    pub session_date: NaiveDate,
    /// เริ่มนำไปใช้เป็น feature ได้เมื่อไร (แท่งวันปิด = หลังปิด session)
    pub available_at: DateTime<Utc>,
    pub source: DataSource,
    /// ได้มาภายหลังแบบย้อนเวลา (เช่น bar ที่ตามมาหลังเปิดแอปใหม่) —
    /// ห้ามปลอมเป็น quote ที่สังเกตสด
    pub backfill: bool,
    pub universe_id: Option<String>,
}

/// ผลตรวจ discovery หนึ่งหน้าต่าง — แยก coverage ของ universe กับ quote
/// ตาม amendments §3 (อย่าอ้างว่า subscribed = universe ทั้งหมด)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryCoverage {
    /// ช่วงวันหมดอายุที่ query [start, end]
    pub expiry_range: (NaiveDate, NaiveDate),
    /// จำนวนแถวที่ API คืน
    pub rows: usize,
    /// รู้ตัวว่าชน cap หน้า (1000) โดยไม่มี cursor ต่อ → ไม่รับรองว่าครบ
    pub possibly_truncated: bool,
    /// contract ID ที่พบทั้งหมดในหน้าต่างนี้ (ก่อนคัดกรองใดๆ)
    pub discovered_occ: Vec<String>,
    /// สัญญาที่ระบบเลือก subscribe (subset เสมอ)
    pub subscribed_occ: Vec<String>,
    /// กติกา/เวอร์ชันที่ใช้เลือก เช่น "near-spot-v1"
    pub selection_rule: String,
    pub recorded_at: DateTime<Utc>,
}

/// ช่องว่างข้อมูล — taxonomy ตาม amendments §2 (ห้ามเติมค่าแทน)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GapKind {
    /// ช่วงที่รู้ว่าแอป/การเชื่อมต่อไม่ได้เปิด (จาก session log)
    CaptureOutage,
    /// ควรมีข้อมูลแต่ไม่มี และพิสูจน์ไม่ได้ว่าเป็น no-trade
    /// (ไม่มีปฏิทิน exchange — ไม่เดาให้เป็น no-trade ที่ยืนยันแล้ว)
    UnknownGap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataGap {
    pub occ: String,
    pub session_date: NaiveDate,
    pub kind: GapKind,
}

/// คำทำนายที่ freeze ไว้ (P9.4) — append-only แก้ย้อนหลังไม่ได้
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredPrediction {
    /// id เรียงตามเวลาสร้าง (nanos) — ไม่ต้อง uuid
    pub id: String,
    pub occ: String,
    /// ความน่าจะเป็นที่ทำนาย (0..1)
    pub p: f64,
    /// นิยามเหตุการณ์ที่วัด
    pub event_th: String,
    pub model_note: String,
    /// ตัวเลขที่ใช้ตอนตัดสินใจ (freeze ไว้ทั้งชุด)
    pub spot: f64,
    pub strike: f64,
    pub premium: f64,
    pub iv: f64,
    pub dte_days: f64,
    /// วันที่หมดอายุ (NY) — outcome resolver ใช้จับคู่ภายหลัง
    pub expiry: chrono::NaiveDate,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// เหตุการณ์ของ capture session เอง (แยกจากข้อมูลตลาด)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CaptureEvent {
    Started { at: DateTime<Utc> },
    Stopped { at: DateTime<Utc> },
    Reconnected { at: DateTime<Utc> },
}
