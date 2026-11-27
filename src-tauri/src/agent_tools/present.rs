//! Executor for the `present` tool family (W6 P4): `present_idea` หาสัญญา
//! จาก ref ใน snapshot ของ engine คำนวณตัวเลขของการ์ดเอง (ยึด EasyIdea
//! ของ build_ideas เมื่อเครื่องง่ายมีไอเดียนั้น ไม่งั้นคำนวณจากแถว),
//! ประกอบการ์ดด้วย pure builder ใน radar-core แล้วส่ง `{kind:"card"}`
//! ออกทาง ui bridge รอ ack 5 วินาที — คืนตัวเลขชุดเดียวกันให้โมเดล
//! เสมอเพื่อให้อ้างอิงต่อได้

use std::time::Duration;

use chrono::Utc;
use radar_core::agent::tools::present::{
    CardNumbers, build_card, card_numbers_from_easy, card_numbers_from_row,
};
use radar_core::easy::{build_ideas, source_token};
use serde_json::{Value, json};

use super::ToolCtx;
use super::ui::{self, AckOutcome};
use crate::lock;

/// งบเวลารอ UI แสดงการ์ด — spec ให้ tool ทั้งหมด 6 วินาที เผื่อการคำนวณ
const CARD_ACK_TIMEOUT: Duration = Duration::from_secs(5);

/// `None` = this family does not own `name`.
pub fn execute(ctx: &ToolCtx, name: &str, args: &Value) -> Option<Result<String, String>> {
    if name != "present_idea" {
        return None;
    }
    Some(run_present_idea(ctx, args))
}

fn run_present_idea(ctx: &ToolCtx, args: &Value) -> Result<String, String> {
    let Some(reference) = args
        .get("ref")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    else {
        return Ok(json!({
            "error": "bad_args",
            "message": "ขาดช่อง ref — ระบุรหัสสัญญา (OCC symbol) เช่น NVDA261218P00175000",
        })
        .to_string());
    };

    let (row, rows, spot, source, atm_iv) = {
        let snap = lock(&ctx.engine).snapshot();
        (
            snap.rows.iter().find(|r| r.q.symbol == reference).cloned(),
            snap.rows,
            snap.spot.unwrap_or(0.0),
            snap.stock
                .as_ref()
                .map(|s| source_token(s.src.as_str()))
                .unwrap_or("API")
                .to_string(),
            snap.stats.as_ref().and_then(|s| s.atm_iv),
        )
    };
    let Some(row) = row else {
        return Ok(json!({
            "error": "ref_not_found",
            "message": format!(
                "ไม่พบสัญญา {reference} ในข้อมูลที่แอปถืออยู่ — เรียก find_ideas หรือ scan_chain ก่อน แล้วค่อย present_idea อีกครั้ง"
            ),
        })
        .to_string());
    };
    if spot <= 0.0 || !spot.is_finite() {
        return Ok(json!({
            "error": "no_spot",
            "message": "ยังไม่มีราคาหุ้นปัจจุบันในระบบ — เรียก find_ideas หรือ scan_chain ก่อน",
        })
        .to_string());
    }

    let symbol = row.q.underlying.clone();
    let profile = ctx.settings.easy_profile.clone();
    // ไม่ยิง network จากที่นี่: realized vol คงเป็น None เหมือนสาย easy ที่
    // ไม่มีข้อมูลแท่งเทียน — ตัวเลขการ์ดไม่เกี่ยวกับมืออยู่แล้ว
    let ideas = build_ideas(
        &symbol,
        spot,
        &source,
        &rows,
        &profile,
        atm_iv,
        None,
        Utc::now(),
    );
    let (numbers, extra_facts): (CardNumbers, Vec<String>) =
        match ideas.ideas.iter().find(|i| i.occ == reference) {
            Some(idea) => {
                let mut extra = vec![
                    json!({
                        "spot": spot,
                        "period_pct": idea.period_pct,
                        "annual_pct": idea.annual_pct,
                        "below_spot_pct": idea.below_spot_pct,
                        "atm_iv": atm_iv,
                    })
                    .to_string(),
                ];
                extra.push(ideas.why_now.text_th.clone());
                (card_numbers_from_easy(idea), extra)
            }
            None => (
                card_numbers_from_row(&row, spot, Utc::now().date_naive()),
                vec![json!({"spot": spot, "atm_iv": atm_iv}).to_string()],
            ),
        };
    let extra_refs: Vec<&str> = extra_facts.iter().map(String::as_str).collect();

    let card = match build_card(
        args,
        numbers,
        &symbol,
        &Utc::now().to_rfc3339(),
        &extra_refs,
    ) {
        Ok(card) => card,
        // error JSON (text_too_long / ungrounded_numbers / bad_args) ส่งต่อ
        // เป็น Ok เพื่อให้โมเดลอ่าน hint แล้วแก้คำตอบของตัวเอง
        Err(error) => return Ok(error.to_string()),
    };
    let numbers_json = card["numbers"].clone();

    if ctx.ui.is_none() {
        return Ok(json!({
            "ok": true,
            "shown": false,
            "numbers": numbers_json,
            "note": "หน้าจอยังไม่พร้อม — เล่าตัวเลขสำคัญจาก numbers ให้ผู้ใช้เอง",
        })
        .to_string());
    }

    let action = json!({"kind": "card", "card": card});
    let shown = match ui::emit_and_wait(ctx, action, CARD_ACK_TIMEOUT)? {
        AckOutcome::Delivered(ack) if ack.ok => true,
        AckOutcome::Delivered(ack) => {
            return Ok(json!({
                "error": "ui_failed",
                "message": ack
                    .detail
                    .unwrap_or_else(|| "หน้าจอแสดงการ์ดไม่สำเร็จ".into()),
            })
            .to_string());
        }
        AckOutcome::TimedOut => false,
        AckOutcome::Cancelled => return Err("หยุดตามที่ผู้ใช้สั่ง".into()),
    };

    let mut out = json!({"ok": true, "shown": shown, "numbers": numbers_json});
    if !shown {
        out["note"] = json!("หน้าจอยังไม่ตอบกลับ การ์ดอาจยังไม่แสดง — เล่าตัวเลขสำคัญจาก numbers ให้ผู้ใช้เอง");
    }
    Ok(out.to_string())
}
