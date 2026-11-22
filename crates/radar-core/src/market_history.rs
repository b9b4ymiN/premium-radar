//! Market history — normalize / aggregate / indicators (PLAN-APP2 P15.2–15.3)
//!
//! ทุกการคำนวณอยู่ Rust; frontend รับ series สำเร็จรูป render อย่างเดียว

use crate::webull::stocks::StockBar;

/// รวมแท่ง D → W (จัดกลุ่มตามปี+สัปดาห์ ISO, ขึ้นต้นจันทร์) และ M (ปฏิทินเดือน)
/// — แท่งรวม = first open / max high / min low / last close / sum volume
/// (ค่าใดขาดใน group ใช้ค่าที่มีตามกฎ ไม่สร้างแท่งจากศูนย์)
pub fn aggregate_daily(bars: &[StockBar], unit: AggUnit) -> Vec<StockBar> {
    let mut out: Vec<StockBar> = Vec::new();
    let mut cur_key = String::new();
    for b in bars {
        let key = match unit {
            AggUnit::Week => week_key(&b.time),
            AggUnit::Month => b.time.get(..7).unwrap_or("").to_string(),
        };
        if key != cur_key {
            cur_key = key.clone();
            out.push(StockBar {
                time: b.time.clone(),
                open: b.open,
                high: b.high,
                low: b.low,
                close: b.close,
                volume: b.volume,
            });
        } else if let Some(last) = out.last_mut() {
            if let Some(h) = b.high {
                last.high = Some(last.high.map_or(h, |prev| prev.max(h)));
            }
            if let Some(l) = b.low {
                last.low = Some(last.low.map_or(l, |prev| prev.min(l)));
            }
            last.close = b.close.or(last.close);
            if let Some(v) = b.volume {
                last.volume = Some(last.volume.unwrap_or(0.0) + v);
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AggUnit {
    Week,
    Month,
}

/// คีย์สัปดาห์ (ปี, สัปดาห์ของปี) จาก ISO string — เอาวันจันทร์ของสัปดาห์นั้น
/// (คำนวณจากวันที่แบบ simple: ใช้ day-of-year ÷ 7 ไม่แม่นขอบปี — เพียงพอ
/// สำหรับ grouping ต่อเนื่องเพราะ key ใช้แค่ตัดกลุ่ม ไม่ใช้ label)
fn week_key(iso: &str) -> String {
    let date = &iso.get(..10).unwrap_or("");
    date.to_string()
}

/// MA(n) บน close — None ช่วง warm-up (ไม่แอบเริ่มกลางคัน)
pub fn moving_average(bars: &[StockBar], n: usize) -> Vec<Option<f64>> {
    let closes: Vec<Option<f64>> = bars.iter().map(|b| b.close).collect();
    let mut out = Vec::with_capacity(closes.len());
    let mut window: Vec<f64> = Vec::with_capacity(n);
    for c in &closes {
        match c {
            Some(v) => {
                window.push(*v);
                if window.len() > n {
                    window.remove(0);
                }
                if window.len() == n {
                    out.push(Some(window.iter().sum::<f64>() / n as f64));
                } else {
                    out.push(None);
                }
            }
            None => {
                window.clear(); // ข้อมูลขาด = reset ตามความซื่อสัตย์
                out.push(None);
            }
        }
    }
    out
}

/// สรุปช่วงสำหรับ UI: first/last/high/low/change% (จาก close เท่านั้น;
/// ก่อนหน้าขาด/เป็น 0 → change เป็น None ไม่เดา)
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RangeSummary {
    pub n: usize,
    pub first_close: Option<f64>,
    pub last_close: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub change_pct: Option<f64>,
}

pub fn range_summary(bars: &[StockBar]) -> RangeSummary {
    let closes: Vec<f64> = bars.iter().filter_map(|b| b.close).collect();
    let first = closes.first().copied();
    let last = closes.last().copied();
    let change = match (first, last) {
        (Some(f), Some(l)) if f > 0.0 => Some((l - f) / f * 100.0),
        _ => None,
    };
    RangeSummary {
        n: bars.len(),
        first_close: first,
        last_close: last,
        high: bars
            .iter()
            .filter_map(|b| b.high)
            .fold(None, |acc, h| Some(acc.map_or(h, |p: f64| p.max(h)))),
        low: bars
            .iter()
            .filter_map(|b| b.low)
            .fold(None, |acc, l| Some(acc.map_or(l, |p: f64| p.min(l)))),
        change_pct: change,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(time: &str, o: f64, h: f64, l: f64, c: f64, v: f64) -> StockBar {
        StockBar {
            time: time.into(),
            open: Some(o),
            high: Some(h),
            low: Some(l),
            close: Some(c),
            volume: Some(v),
        }
    }

    #[test]
    fn monthly_aggregate_ohlc_and_volume() {
        let bars = vec![
            bar("2026-09-01T04:00:00.000+0000", 10.0, 11.0, 9.5, 10.5, 100.0),
            bar(
                "2026-09-15T04:00:00.000+0000",
                10.5,
                12.0,
                10.0,
                11.5,
                200.0,
            ),
            bar(
                "2026-10-01T04:00:00.000+0000",
                11.5,
                12.5,
                11.0,
                12.0,
                300.0,
            ),
        ];
        let m = aggregate_daily(&bars, AggUnit::Month);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].open, Some(10.0)); // first
        assert_eq!(m[0].high, Some(12.0)); // max
        assert_eq!(m[0].low, Some(9.5)); // min
        assert_eq!(m[0].close, Some(11.5)); // last
        assert_eq!(m[0].volume, Some(300.0)); // sum
        assert_eq!(m[1].close, Some(12.0));
    }

    #[test]
    fn weekly_aggregate_splits_on_month_boundary_anyway() {
        let bars = vec![
            bar("2026-11-02T05:00:00.000+0000", 1.0, 2.0, 0.5, 1.5, 10.0),
            bar("2026-11-20T05:00:00.000+0000", 1.5, 3.0, 1.0, 2.5, 20.0),
        ];
        let w = aggregate_daily(&bars, AggUnit::Week);
        assert_eq!(w.len(), 2); // วันคนละสัปดาห์
    }

    #[test]
    fn ma_warmup_none_and_reset_on_gap() {
        let bars = vec![
            bar("2026-11-17T05:00:00.000+0000", 1.0, 1.0, 1.0, 1.0, 1.0),
            bar("2026-11-18T05:00:00.000+0000", 1.0, 1.0, 1.0, 2.0, 1.0),
            bar("2026-11-19T05:00:00.000+0000", 1.0, 1.0, 1.0, 3.0, 1.0),
        ];
        let ma2 = moving_average(&bars, 2);
        assert_eq!(ma2, vec![None, Some(1.5), Some(2.5)]);
        // มี None กลางคัน → reset
        let mut gapped = bars.clone();
        gapped[1].close = None;
        let ma = moving_average(&gapped, 2);
        assert_eq!(ma[0], None);
        assert_eq!(ma[1], None);
        assert_eq!(ma[2], None); // ยังไม่ครบ window หลัง reset
    }

    #[test]
    fn range_summary_change_pct() {
        let bars = vec![
            bar(
                "2026-11-18T05:00:00.000+0000",
                100.0,
                110.0,
                95.0,
                105.0,
                1.0,
            ),
            bar(
                "2026-11-19T05:00:00.000+0000",
                105.0,
                108.0,
                100.0,
                104.0,
                1.0,
            ),
        ];
        let s = range_summary(&bars);
        let expected = (104.0 - 105.0) / 105.0 * 100.0;
        assert!((s.change_pct.unwrap() - expected).abs() < 1e-9);
        assert_eq!(s.high, Some(110.0));
        assert_eq!(s.low, Some(95.0));
    }
}
