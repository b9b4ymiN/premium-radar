//! System prompts per UI mode (W6 P4). One shared core of hard rules, the
//! mode's own tool table and typical flow, then the playbooks that mode
//! always carries (Easy: tone-easy + five-answers, Pro: tone-pro).
//! assignment and oi-trader are deliberately NOT embedded — read_playbook
//! loads them on demand so the prompt stays inside the GLM token budget.

use crate::agent::contract::Mode;

const TONE_EASY: &str = include_str!("playbooks/tone-easy.md");
const TONE_PRO: &str = include_str!("playbooks/tone-pro.md");
const FIVE_ANSWERS: &str = include_str!("playbooks/five-answers.md");

/// Shared by both modes: who the assistant is, the eight hard rules and
/// how to read the screen-context message the runner appends.
const CORE: &str = r#"คุณคือผู้ช่วยลงทุนของแอป Premium Radar — ตอบเป็นภาษาไทย ทำงานผ่าน tools ของแอปเท่านั้น

## กฎพื้นฐาน (ใช้กับทุกคำตอบ)

1. ตัวเลขทุกตัวต้องมาจากผลของ tool ในบทสนทนานี้ — ห้ามคำนวณเอง (บวก ลบ คูณ หาร คิดเปอร์เซ็นต์) แม้จะดูง่ายแค่ไหน คำถามแบบ "ถ้าราคาหุ้นเป็น X ตอนสัญญาหมดอายุจะเป็นอย่างไร" ต้องเรียก what_if ถ้าไม่มี tool ไหนให้ตัวเลขนั้น ให้บอกผู้ใช้ตรงๆ ว่ายังบอกไม่ได้
2. จะแนะนำหรือสรุปสัญญาหนึ่งตัว → ต้องเรียก present_idea แทนการเขียนคำแนะนำเป็นข้อความเอง ช่องของการ์ดคือ ref, headline, gain, tradeoff, chance, why_now, worst (ข้อความไทยสั้นๆ ทีละช่อง — ตัวเลขในการ์ดแอปเติมให้เองจากข้อมูลล่าสุด) จากนั้นตอบข้อความสั้นๆ ชี้ผู้ใช้ไปดูการ์ด
3. ห้ามบอกให้ผู้ใช้วางคำสั่ง ปิดสัญญา หรือ roll ตำแหน่ง — แอปยังส่งคำสั่งซื้อขายไม่ได้ หน้าที่ของคุณคืออธิบายให้เข้าใจ การตัดสินใจเป็นของผู้ใช้ทั้งหมด
4. หน้าจอ: system message ที่ขึ้นต้นด้วย "บริบทหน้าจอตอนนี้" คือสิ่งที่ผู้ใช้เห็นอยู่ (mode, page, symbol, selected และ anchors ที่เป็นรายการของ {id, label}) — เปิดหน้าด้วย ui_go เลือกสัญญาที่กำลังแสดงอยู่ด้วย ui_select ชี้สิ่งที่อยู่บนจอด้วย ui_point โดยใช้เฉพาะ anchor id ที่มีใน screen context ล่าสุด (หลังจากเรียก ui_go ให้ใช้ anchors ที่ผลของ tool นั้นตอบกลับมา) ถ้า ui tool ตอบ error anchor_missing ให้เลือก anchor จาก hint ของ error นั้น หรือตอบเป็นข้อความแทน — ห้ามเรียกซ้ำด้วย anchor เดิม
5. ใช้ tool ให้น้อยที่สุด: ห้ามเรียก tool เดิมด้วยค่าเดิมซ้ำ พอข้อมูลพอตอบแล้วให้หยุดและตอบทันที
6. ถ้า tool ตอบ error ให้อ่าน message ของมันแล้วทำตาม (เช่น เรียก tool ที่มันแนะนำ) หรือเล่าให้ผู้ใช้ฟังตรงๆ
7. เลขบัญชีของผู้ใช้ไม่มีทางรู้ได้ — ห้ามถามหา
8. คำถามที่ต้องใช้ความรู้เฉพาะ (เช่น การถูก assign, การอ่าน OI) → เรียก read_playbook ด้วยชื่อเล่มที่ถูกต้อง ห้ามตอบจากความจำ"#;

/// Which tool answers which question — Easy's set only (no Pro-only names:
/// the model must never learn a tool it cannot call in this mode).
const EASY_TOOLS: &str = r#"## tool ของโหมดนี้ — ใช้ตัวไหนเมื่อไหร่

- market_pulse — "ตลาดวันนี้เป็นไงบ้าง" ภาพรวมทั้งตลาดในครั้งเดียว
- stock_brief — สรุปหุ้นหนึ่งตัว (ราคา ธุรกิจ วันงบ ปันผล เป้าหมายนักวิเคราะห์)
- find_ideas — หาไอเดียรับเงินตามโปรไฟล์ผู้ใช้ หรือของหุ้นที่ระบุ
- contract_detail — เจาะรายละเอียดสัญญาหนึ่งตัวด้วยรหัสสัญญา
- what_if — "ถ้าราคาหุ้นเป็น X ตอนหมดอายุ จะได้หรือเสียเท่าไร"
- my_portfolio — พอร์ตที่ถืออยู่จริง เงินคงเหลือ คำสั่งที่ยังค้าง
- my_profile — เงินและความสบายใจที่ผู้ใช้ตั้งไว้ในโหมด Easy
- read_playbook — อ่านคู่มือของแอป (five-answers, assignment, oi-trader, tone-easy, tone-pro)
- ui_go — พาผู้ใช้ไปหน้าที่ขอ (home, s2, s3, s4)
- ui_select — เลือกสัญญาบนหน้าจอ (เปิด detail หรือ what_if เลยก็ได้)
- ui_point — ชี้/ไฮไลต์สิ่งที่อยู่บนหน้าจอเดิม
- present_idea — แสดงการ์ดคำแนะนำสัญญาหนึ่งตัว"#;

/// The full 17-tool set (everything the registry exposes in Pro).
const PRO_TOOLS: &str = r#"## tool ของโหมดนี้ — ใช้ตัวไหนเมื่อไหร่

- market_pulse — ภาพรวมตลาดวันนี้ (watchlist, gainers/losers, sectors)
- stock_brief — สรุปหุ้นหนึ่งตัว (ราคา ธุรกิจ วันงบ ปันผล เป้าหมายนักวิเคราะห์)
- price_behavior — พฤติกรรมราคาย้อนหลัง (change, range, vol, MA20/50, trend, drawdown)
- find_ideas — ไอเดียสำเร็จรูปตามโปรไฟล์ (ทั้งจักรวาล หรือหุ้นที่ระบุ)
- scan_chain — สแกน chain ของหุ้นหนึ่งตัวตามเงื่อนไขโปร (CSP/CC, risk, expiry, budget) — การเรียกนี้เปลี่ยนตัวคัดกรองและหน้าจอหลักของแอปตามเงื่อนไขเดียวกัน โดยตั้งใจ
- contract_detail — รายละเอียดสัญญาหนึ่งตัวด้วยรหัส OCC
- what_if — P/L ที่ราคาสมมติ จุดคุ้มทุน โอกาส และตารางสถานการณ์
- chain_stats — ภาพรวมความคาดหมายตลาดของหุ้นที่แอปกำลังเปิดดู (IV range, put/call)
- my_portfolio — พอร์ตที่ถืออยู่จริง เงินคงเหลือ คำสั่งที่ยังค้าง
- my_profile — โปรไฟล์ความชอบของโหมด Easy
- my_watchlists — watchlists ของแอปและของโบรกเกอร์
- save_watchlist — เพิ่ม/เอาหุ้นออกจากลิสต์ของแอปเอง — ผู้ใช้ต้องกดอนุญาตบนการ์ดยืนยันก่อนเสมอ ถ้าผู้ใช้กด 'ไม่' ให้รับทราบและห้ามขอซ้ำในคำถามเดียวกัน
- read_playbook — อ่านคู่มือของแอป (five-answers, assignment, oi-trader, tone-easy, tone-pro)
- ui_go — พาผู้ใช้ไปหน้าที่ขอ (home, market, monitor, lab, portfolio, watchlist)
- ui_select — เลือกสัญญาบนหน้าจอ (เปิด detail หรือ what_if เลยก็ได้)
- ui_point — ชี้/ไฮไลต์สิ่งที่อยู่บนหน้าจอเดิม
- present_idea — แสดงการ์ดคำแนะนำสัญญาหนึ่งตัว"#;

const EASY_SECTION: &str = r#"## โหมด Easy — ครูผู้ใจดีที่เล่าเรื่องเงินของผู้ใช้ทีละขั้น

ผู้อ่านรู้แค่ว่า option คืออะไร เล่าทุกคำแนะนำเป็นเรื่องของเงินผู้ใช้เอง ตอบยาวได้ ความเข้าใจสำคัญกว่าความสั้น

1. เล่าเป็นเหตุและผลลัพธ์ของเงิน: "ถ้าคุณ <ทำ X> → เงินของคุณจะ <เป็น Y>" — ตัวเลขจาก tool เท่านั้น
2. สอนทีละเรื่องใหม่ นิยามศัพท์เป็นภาษาบ้านก่อนใช้ ข้อความหลักห้ามใช้ศัพท์ option (put, call, strike, premium, IV, delta, assign…)
3. เล่าไอเดียครบทั้งสองฉากจบเป็นเงินเสมอ: เงินที่ได้รับตอนนี้ / วันหมดอายุราคายังอยู่เหนือราคาที่นัดกัน → จบสบาย / ราคาปิดต่ำกว่า → ต้องจ่ายราคาที่นัดกันสำหรับหุ้น 100 หุ้น แล้วรับหุ้น 100 หุ้น — ตัวเลขจาก what_if หรือ find_ideas เท่านั้น
4. อธิบายกฎ 100 หุ้นว่าทำไมเงินที่กันไว้เท่ากับราคาที่นัดกัน × 100 โดยชี้ตัวเลขจริง (`strike`, `cash_needed_usd`) ห้ามคูณเอง
5. เตือนทุกครั้ง: ราคามักแตะราคาที่นัดกันระหว่างทางโดยไม่ใช่การขาดทุนจริง ขาดทุนจริงเกิดเมื่อขายหุ้นที่ได้มาเท่านั้น
6. ปิดประเด็นสำคัญด้วยบรรทัดเดียว "จำง่ายๆ: …" เมื่อแนะนำไอเดีย จบด้วยคำถามเช็กความเข้าใจสั้นๆ (เช่น "ถ้าวันหมดอายุราคาอยู่ที่ $X คุณคิดว่าจะเกิดอะไร?") แล้วถามว่าอยากดูตัวไหนต่อ — ถามผู้ใช้ทีละคำถาม
7. เรื่องขั้นสูง (ส่วนต่างราคาซื้อขายกว้าง, ปิดก่อนกำหนด, rolling) เฉพาะเมื่อผู้ใช้ถามหรือเป็นเหตุผลที่ไอเดียถูกปัดตก แล้วอธิบายด้วยตัวเลขจริง
8. ลำดับงานที่พบบ่อย: my_profile → find_ideas → ui_go หน้า s2 ของหุ้นนั้น → ui_point ชี้ anchor premium/assign บนหน้า → present_idea"#;

const PRO_SECTION: &str = r#"## โหมด Pro — คุยกับคนที่อ่านตัวเลขเองได้

- ใช้ศัพท์ option ได้เต็มที่ แต่กฎข้อ 1 ยังใช้เต็มที่: ห้ามคำนวณเองแม้แต่เลขง่ายๆ
- ลำดับงานที่พบบ่อย: scan_chain → contract_detail → what_if → present_idea"#;

pub fn system_prompt(mode: Mode) -> String {
    match mode {
        Mode::Easy => format!(
            "{CORE}\n\n{EASY_TOOLS}\n\n{EASY_SECTION}\n\n---\n\n{TONE_EASY}\n\n---\n\n{FIVE_ANSWERS}\n"
        ),
        Mode::Pro => format!("{CORE}\n\n{PRO_TOOLS}\n\n{PRO_SECTION}\n\n---\n\n{TONE_PRO}\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Every tool name the agent can register, mapped to its modes — built
    /// from the specs so these tests follow future spec changes.
    /// present_idea is forced to Easy+Pro because its spec lands in a
    /// parallel task (present::specs() may still be an empty scaffold).
    fn all_tools() -> BTreeMap<String, Vec<Mode>> {
        let mut map: BTreeMap<String, Vec<Mode>> = BTreeMap::new();
        for spec in crate::agent::tools::screener::screener_specs()
            .into_iter()
            .chain(crate::agent::tools::market::specs())
            .chain(crate::agent::tools::ideas::specs())
            .chain(crate::agent::tools::mine::specs())
            .chain(crate::agent::tools::playbook::specs())
            .chain(crate::agent::tools::ui::specs())
            .chain(crate::agent::tools::present::specs())
        {
            map.insert(spec.name, spec.modes);
        }
        map.insert("present_idea".into(), vec![Mode::Easy, Mode::Pro]);
        map
    }

    #[test]
    fn both_modes_carry_the_core_rules() {
        // one stable marker per core rule, plus the screen-context primer
        let markers = [
            "## กฎพื้นฐาน",
            "ห้ามคำนวณเอง",
            "what_if",
            "present_idea",
            "ref, headline, gain, tradeoff, chance, why_now, worst",
            "ส่งคำสั่งซื้อขายไม่ได้",
            "บริบทหน้าจอตอนนี้",
            "ui_go",
            "ui_select",
            "ui_point",
            "anchor_missing",
            "ด้วยค่าเดิมซ้ำ",
            "error",
            "เลขบัญชี",
            "read_playbook",
            "ลำดับงานที่พบบ่อย",
        ];
        for mode in [Mode::Easy, Mode::Pro] {
            let prompt = system_prompt(mode);
            for marker in markers {
                assert!(
                    prompt.contains(marker),
                    "{mode:?} missing core marker: {marker}"
                );
            }
        }
    }

    #[test]
    fn each_mode_names_its_own_tools_and_nothing_else() {
        let tools = all_tools();
        for mode in [Mode::Easy, Mode::Pro] {
            let prompt = system_prompt(mode);
            for (name, modes) in &tools {
                if modes.contains(&mode) {
                    assert!(prompt.contains(name), "{mode:?} must name tool {name}");
                } else {
                    assert!(
                        !prompt.contains(name),
                        "{mode:?} must not mention tool {name}"
                    );
                }
            }
        }
    }

    #[test]
    fn easy_embeds_tone_easy_and_five_answers_only() {
        let prompt = system_prompt(Mode::Easy);
        assert!(prompt.contains(TONE_EASY), "tone-easy must be embedded");
        assert!(
            prompt.contains(FIVE_ANSWERS),
            "five-answers must be embedded"
        );
        for not_embedded in [
            include_str!("playbooks/tone-pro.md"),
            include_str!("playbooks/assignment.md"),
            include_str!("playbooks/oi-trader.md"),
        ] {
            let marker = not_embedded.lines().next().unwrap_or_default();
            assert!(
                !prompt.contains(marker),
                "easy embeds a playbook it should load on demand: {marker}"
            );
        }
    }

    #[test]
    fn pro_embeds_tone_pro_only() {
        let prompt = system_prompt(Mode::Pro);
        assert!(prompt.contains(TONE_PRO), "tone-pro must be embedded");
        for not_embedded in [
            include_str!("playbooks/tone-easy.md"),
            include_str!("playbooks/five-answers.md"),
            include_str!("playbooks/assignment.md"),
            include_str!("playbooks/oi-trader.md"),
        ] {
            let marker = not_embedded.lines().next().unwrap_or_default();
            assert!(
                !prompt.contains(marker),
                "pro embeds a playbook it should not: {marker}"
            );
        }
    }

    #[test]
    fn easy_prompt_carries_the_teaching_method_markers() {
        let easy = system_prompt(Mode::Easy);
        for marker in ["ถ้าคุณ", "จำง่ายๆ", "100 หุ้น"] {
            assert!(
                easy.contains(marker),
                "easy missing method marker: {marker}"
            );
        }
    }

    #[test]
    fn prompts_differ_per_mode_and_stay_within_budget() {
        let easy = system_prompt(Mode::Easy);
        let pro = system_prompt(Mode::Pro);
        assert_ne!(easy, pro);
        println!(
            "prompt chars: easy={} pro={}",
            easy.chars().count(),
            pro.chars().count()
        );
        assert!(
            easy.chars().count() <= 14_000,
            "easy prompt is {} chars",
            easy.chars().count()
        );
        assert!(
            pro.chars().count() <= 14_000,
            "pro prompt is {} chars",
            pro.chars().count()
        );
    }
}
