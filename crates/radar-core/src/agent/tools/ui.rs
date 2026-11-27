//! Agent tools — `ui` family (W6 P3): พาผู้ใช้ไปหน้าที่ขอ สั่งเลือกสัญญา
//! และชี้สิ่งที่อยู่บนหน้าจอ ไฟล์นี้เก็บ specs + `plan_ui_call` ซึ่งเป็น
//! pure planner: ตรวจ page/anchor/ข้อจำกัดต่างๆ กับ screen context ที่
//! UI รายงานมา แล้วสร้าง UiAction JSON ตามรูปใน `src/agent/types.ts`
//! — ฝั่ง Tauri เป็นคนสั่ง UI จริงและรอ ack

use serde_json::{Value, json};

use crate::agent::contract::{Mode, Tier, ToolSpec};

/// จำนวน anchor สูงสุดที่ยัดเข้า hint ของ error `anchor_missing`
pub const MAX_HINTS: usize = 20;

/// ขั้นตอน point ต่อการเรียกหนึ่งครั้ง
pub const MAX_POINT_STEPS: usize = 5;

/// ความยาวข้อความ (ตัวอักษร ไม่ใช่ไบต์) ต่อ step
pub const MAX_MESSAGE_CHARS: usize = 120;

const EFFECTS: [&str; 8] = [
    "circle",
    "underline",
    "highlight",
    "box",
    "blink",
    "pulse",
    "spotlight",
    "bubble",
];
const COLORS: [&str; 3] = ["accent", "warn", "danger"];

/// หน้าที่เปิดได้ตามโหมด (UI รายงาน mode มาใน screen context)
fn allowed_pages(mode: &str) -> &'static [&'static str] {
    if mode == "pro" {
        &["home", "market", "monitor", "lab", "portfolio", "watchlist"]
    } else {
        &["home", "s2", "s3", "s4"]
    }
}

pub fn specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "ui_go".into(),
            tier: Tier::Ui,
            modes: vec![Mode::Easy, Mode::Pro],
            description: "พาผู้ใช้ไปหน้าที่ขอ แล้วรอให้ UI เปิดเสร็จก่อนตอบต่อ\nหน้าที่เปิดได้ต่างกันตามโหมด — โหมด Pro: home (หน้าแรก), market (ภาพรวมตลาดและสายตาหุ้นที่ค้นได้), monitor (หน้าจอติดตามหุ้นรายตัว ใส่ symbol ได้), lab (ห้องทดลองทำนายราคา), portfolio (พอร์ตของฉัน), watchlist (รายการหุ้นที่ติดตาม) — โหมด Easy: home (หน้าแรก), s2 (รายการไอเดียของหุ้นตัวหนึ่ง ต้องใส่ symbol), s3 (คำถาม 5 ข้อของไอเดียที่เลือก ต้องใส่ ref คือรหัสสัญญา), s4 (เช็กลิสต์ก่อนตัดสินใจ ต้องใส่ ref คือรหัสสัญญา)\nใช้เมื่อ: ผู้ใช้ขอให้เปิดดูหน้าไหนหน้าหนึ่ง หรือการตอบคำถามต้องเห็นหน้านั้นประกอบ\nไม่ใช้เมื่อ: แค่เล่าข้อมูลให้ฟัง (ไม่ต้องพาไปไหน) หรืออยากชี้สิ่งที่อยู่บนหน้าจอเดิม (ใช้ ui_point)".into(),
            examples: vec![
                json!({"page": "home"}),
                json!({"page": "monitor", "symbol": "NVDA"}),
                json!({"page": "s2", "symbol": "NVDA"}),
            ],
            parameters: json!({
                "type": "object",
                "properties": {
                    "page": {
                        "type": "string",
                        "enum": ["home", "market", "monitor", "lab", "portfolio", "watchlist", "s2", "s3", "s4"],
                        "description": "หน้าที่จะเปิด — บางหน้าใช้ได้เฉพาะโหมด Pro หรือ Easy"
                    },
                    "symbol": {
                        "type": "string",
                        "description": "รหัสหุ้น เช่น NVDA — จำเป็นสำหรับ s2, ใส่ได้กับ monitor"
                    },
                    "ref": {
                        "type": "string",
                        "description": "รหัสสัญญา (OCC symbol) — จำเป็นสำหรับ s3 และ s4"
                    }
                },
                "required": ["page"],
                "additionalProperties": false
            }),
            timeout_ms: 6000,
            webull_cost: 0,
        },
        ToolSpec {
            name: "ui_select".into(),
            tier: Tier::Ui,
            modes: vec![Mode::Easy, Mode::Pro],
            description: "สั่งเลือกสัญญาหนึ่งตัวบนหน้าจอที่เปิดอยู่ (แถวของสัญญานั้นต้องกำลังแสดงอยู่) และเลือกได้ว่าจะเปิดดูรายละเอียดเลย หรือเปิดโหมดลองสมมติราคา (what_if) พร้อมราคาที่จะสมมติ\nใช้เมื่อ: ผู้ใช้บอกว่าสนใจสัญญาตัวไหน และอยากดูสัญญานั้นบนหน้าจอ หรืออยากให้ช่วยลองสมมติราคา\nไม่ใช้เมื่อ: ผู้ใช้อยากได้ตัวเลขของสัญญาเป็นข้อความ (ใช้ contract_detail หรือ find_ideas) หรืออยากให้ชี้ของบนหน้าจอ (ใช้ ui_point)".into(),
            examples: vec![
                json!({"ref": "NVDA261218P00175000"}),
                json!({"ref": "NVDA261218P00175000", "open": "what_if", "price": 120.0}),
            ],
            parameters: json!({
                "type": "object",
                "properties": {
                    "ref": {
                        "type": "string",
                        "description": "รหัสสัญญา (OCC symbol) ของแถวที่กำลังแสดงอยู่บนหน้าจอ เช่น NVDA261218P00175000"
                    },
                    "open": {
                        "type": "string",
                        "enum": ["detail", "what_if"],
                        "description": "detail = เปิดดูรายละเอียด, what_if = เปิดโหมดลองสมมติราคา (ต้องใส่ price ด้วย)"
                    },
                    "price": {
                        "type": "number",
                        "minimum": 0.01,
                        "description": "ราคาหุ้นที่จะสมมติ ใช้เมื่อ open = what_if เท่านั้น"
                    }
                },
                "required": ["ref"],
                "additionalProperties": false
            }),
            timeout_ms: 6000,
            webull_cost: 0,
        },
        ToolSpec {
            name: "ui_point".into(),
            tier: Tier::Ui,
            modes: vec![Mode::Easy, Mode::Pro],
            description: "ชี้สิ่งที่อยู่บนหน้าจอเดิมให้ผู้ใช้เห็น (วาดวงกลม ขีดเส้นใต้ หรือกรอบไฮไลต์) พร้อมข้อความสั้นๆ ภาษาไทยกำกับได้ — ทำทีละขั้นตอนตามลำดับที่ส่ง ไม่เกิน 5 ขั้นต่อครั้ง\nanchor คือ id ของสิ่งที่จะชี้ ต้องเป็นตัวที่มีอยู่จริงบนหน้าจอตอนนี้ (อยู่ใน screen context) เช่น tab:portfolio, whatif:price, idea:NVDA261218P00175000/premium\nใช้เมื่อ: อธิบายอะไรที่มองเห็นบนหน้าจอแล้วอยากให้ผู้ใช้มองตรงจุดนั้นด้วย\nไม่ใช้เมื่อ: ผู้ใช้อยู่หน้าอื่นอยู่ (ใช้ ui_go พาไปก่อน) หรือแค่ตอบเป็นข้อความธรรมดาก็พอ".into(),
            examples: vec![
                json!({"steps": [{"anchor": "tab:portfolio", "effect": "circle", "message": "พอร์ตอยู่ตรงนี้"}]}),
                json!({"steps": [
                    {"anchor": "idea:NVDA261218P00175000/premium", "effect": "highlight", "color": "warn"},
                    {"anchor": "whatif:price", "effect": "underline"}
                ]}),
            ],
            parameters: json!({
                "type": "object",
                "properties": {
                    "steps": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 5,
                        "description": "สิ่งที่จะชี้ 1-5 ขั้นตอน เรียงตามลำดับที่อยากให้ผู้ใช้มอง",
                        "items": {
                            "type": "object",
                            "properties": {
                                "anchor": {
                                    "type": "string",
                                    "description": "id ของสิ่งที่จะชี้ ต้องมีอยู่บนหน้าจอตอนนี้ เช่น tab:portfolio"
                                },
                                "effect": {
                                    "type": "string",
                                    "enum": ["circle", "underline", "highlight", "box", "blink", "pulse", "spotlight", "bubble"],
                                    "description": "รูปแบบการชี้"
                                },
                                "message": {
                                    "type": "string",
                                    "description": "ข้อความไทยสั้นๆ กำกับจุดที่ชี้ ไม่เกิน 120 ตัวอักษร"
                                },
                                "color": {
                                    "type": "string",
                                    "enum": ["accent", "warn", "danger"],
                                    "description": "สีของเอฟเฟกต์"
                                }
                            },
                            "required": ["anchor", "effect"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["steps"],
                "additionalProperties": false
            }),
            timeout_ms: 6000,
            webull_cost: 0,
        },
    ]
}

// ---------- pure planner ----------

/// วางแผนการเรียก ui tool หนึ่งครั้ง: ตรวจ page ที่โหมดอนุญาต anchor ที่มีจริง
/// และข้อจำกัดของ steps — `Ok` คือ UiAction JSON ตาม `src/agent/types.ts`
/// `Err` คือ error JSON (`{"error": code, "message": thai, ...}`) ที่ส่งกลับ
/// ให้โมเดลอ่านเพื่อแก้การเรียก
///
/// `screen` คือ screen context ล่าสุดที่ UI รายงานมา (รวมที่ได้จาก ack
/// ของ ui_action ก่อนหน้า) — `None` หรือ screen ที่ไม่มี mode ที่รู้จัก
/// ถือว่ามองไม่เห็นหน้าจอ (`no_screen`)
pub fn plan_ui_call(name: &str, args: &Value, screen: Option<&Value>) -> Result<Value, Value> {
    let Some(screen) = screen.filter(|s| s.is_object()) else {
        return Err(no_screen_error());
    };
    let Some(mode) = screen
        .get("mode")
        .and_then(Value::as_str)
        .filter(|m| matches!(*m, "easy" | "pro"))
    else {
        return Err(no_screen_error());
    };

    match name {
        "ui_go" => plan_go(args, mode),
        "ui_select" => plan_select(args, screen),
        "ui_point" => plan_point(args, screen),
        _ => Err(bad_args_error(format!(
            "ไม่รู้จัก ui tool \"{name}\" — ใช้ ui_go, ui_select หรือ ui_point"
        ))),
    }
}

fn no_screen_error() -> Value {
    json!({
        "error": "no_screen",
        "message": "ยังไม่รู้ว่าตอนนี้ผู้ใช้เปิดหน้าจอไหนอยู่ (ไม่มี screen context) — อย่าสั่งหน้าจอ ให้ตอบผู้ใช้เป็นข้อความธรรมดาแทน",
    })
}

fn bad_args_error(message: String) -> Value {
    json!({"error": "bad_args", "message": message})
}

/// id ของ anchor ทั้งหมดที่ screen รายงาน (เรียงตามความสำคัญที่ UI ส่งมา)
fn screen_anchor_ids(screen: &Value) -> Vec<String> {
    screen
        .get("anchors")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    row.get("id")
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// hint ของ anchor ที่มีอยู่จริง สูงสุด [`MAX_HINTS`] ตัว — ตัวที่ควรแนะนำ
/// มาก่อนคือตัวที่ใช้ kind เดียวกัน (ส่วนหน้าเครื่องหมาย `:` เช่น `idea:`)
/// แล้วต่อด้วยตัวที่ id มีส่วน ref ของ anchor ที่หายไปอยู่ข้างใน โดยคง
/// ลำดับความสำคัญของ screen ไว้ภายในกลุ่มเดียวกัน
fn anchor_hints(missing: &str, anchors: &[String]) -> Vec<String> {
    let (kind, ref_part) = split_anchor(missing);
    let mut scored: Vec<(u8, &String)> = anchors
        .iter()
        .map(|id| {
            let (id_kind, _) = split_anchor(id);
            let same_kind = kind.is_some() && id_kind == kind;
            let shares_ref = !ref_part.is_empty() && id.contains(ref_part);
            let score = match (same_kind, shares_ref) {
                (true, true) => 0,
                (true, false) => 1,
                (false, true) => 2,
                (false, false) => 3,
            };
            (score, id)
        })
        .collect();
    scored.sort_by_key(|(score, _)| *score); // stable: คงลำดับของ screen
    scored
        .into_iter()
        .take(MAX_HINTS)
        .map(|(_, id)| id.clone())
        .collect()
}

/// แยก anchor id เป็น (kind, ref): `idea:OCC/field` → ("idea", "OCC") —
/// id ที่ไม่มี `:` ได้ kind = None และ ref คือตัวมันเอง
fn split_anchor(id: &str) -> (Option<&str>, &str) {
    match id.split_once(':') {
        Some((kind, rest)) => {
            let reference = rest.split('/').next().unwrap_or(rest);
            (Some(kind), reference)
        }
        None => (None, id),
    }
}

fn anchor_missing_error(missing: &[String], anchors: &[String]) -> Value {
    let hints: Vec<String> = missing
        .iter()
        .flat_map(|one| anchor_hints(one, anchors))
        .collect();
    json!({
        "error": "anchor_missing",
        "message": format!(
            "ไม่พบสิ่งนี้บนหน้าจอที่เปิดอยู่: {} — ดู anchor ที่มีอยู่จริงใน hint แล้วเรียกใหม่ด้วย anchor ตัวใดตัวหนึ่งจากนั้น",
            missing.join(", ")
        ),
        "missing": missing,
        "hint": hints,
    })
}

// ---------- ui_go ----------

fn plan_go(args: &Value, mode: &str) -> Result<Value, Value> {
    let Some(page) = args.get("page").and_then(Value::as_str) else {
        return Err(bad_args_error(
            "ขาดช่อง page — ระบุหน้าที่จะเปิด เช่น {\"page\": \"home\"}".into(),
        ));
    };
    let allowed = allowed_pages(mode);
    if !allowed.contains(&page) {
        return Err(json!({
            "error": "page_not_allowed",
            "message": format!(
                "หน้า \"{page}\" เปิดไม่ได้ในโหมด {mode} ตอนนี้ — เลือกหน้าจาก: {}",
                allowed.join(", ")
            ),
            "page": page,
            "allowed": allowed,
        }));
    }

    let symbol = optional_trimmed(args.get("symbol"));
    let reference = optional_trimmed(args.get("ref"));
    // ข้อกำหนดของแต่ละหน้า: s2 ต้องมีหุ้น, s3/s4 ต้องมีรหัสสัญญา
    match (mode, page) {
        ("easy", "s2") if symbol.is_none() => {
            return Err(bad_args_error(
                "หน้า s2 (รายการไอเดียของหุ้นตัวหนึ่ง) ต้องระบุหุ้น — เพิ่มช่อง symbol เช่น {\"page\": \"s2\", \"symbol\": \"NVDA\"}".into(),
            ));
        }
        ("easy", "s3" | "s4") if reference.is_none() => {
            return Err(bad_args_error(format!(
                "หน้า {page} ต้องระบุรหัสสัญญา — เพิ่มช่อง ref เช่น {{\"page\": \"{page}\", \"ref\": \"NVDA261218P00175000\"}}"
            )));
        }
        _ => {}
    }

    Ok(json!({
        "kind": "go",
        "mode": mode,
        "page": page,
        "symbol": symbol,
        "ref": reference,
    }))
}

/// ค่า string ที่ไม่ใช่ค่าว่าง (trim แล้ว) — เอามาใส่ใน action เฉพาะเมื่อโมเดลส่งมาจริง
fn optional_trimmed(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

// ---------- ui_select ----------

/// `ref` ตรงกับ anchor ตัวหนึ่งเมื่อ id นั้นคือ ref เป๊ะ หรือส่วน ref
/// ของ id (`kind:ref/field` → ส่วนหลัง `:` ตัด `/field` ทิ้ง) เท่ากับ ref —
/// เช่น `chain:NVDA261218P00175000` และ `NVDA261218P00175000` ตรงกัน
fn anchor_ref_matches(id: &str, reference: &str) -> bool {
    if id == reference {
        return true;
    }
    id.split_once(':')
        .is_some_and(|(_, rest)| rest.split('/').next() == Some(reference))
}

fn plan_select(args: &Value, screen: &Value) -> Result<Value, Value> {
    let Some(reference) = optional_trimmed(args.get("ref")) else {
        return Err(bad_args_error(
            "ขาดช่อง ref — ระบุรหัสสัญญา (OCC symbol) ของแถวที่จะเลือก เช่น NVDA261218P00175000".into(),
        ));
    };
    let open = optional_trimmed(args.get("open"));
    if let Some(open) = &open
        && open != "detail"
        && open != "what_if"
    {
        return Err(bad_args_error(format!(
            "ค่าของช่อง open ต้องเป็น detail หรือ what_if (ส่งมา {open}) — แก้ค่าแล้วเรียกใหม่"
        )));
    }
    let mut price = None;
    if let Some(raw) = args.get("price").filter(|p| !p.is_null()) {
        if open.as_deref() != Some("what_if") {
            return Err(bad_args_error(
                "ช่อง price ใช้ได้เฉพาะเมื่อ open = what_if — เพิ่ม open หรือเอา price ออกแล้วเรียกใหม่"
                    .into(),
            ));
        }
        let Some(value) = raw.as_f64() else {
            return Err(bad_args_error("ช่อง price ต้องเป็นตัวเลข เช่น 120.5".into()));
        };
        if value < 0.01 {
            return Err(bad_args_error(
                "ช่อง price ต้องมากกว่าหรือเท่ากับ 0.01 — ปรับค่าแล้วเรียกใหม่".into(),
            ));
        }
        price = Some(value);
    }

    // สัญญาที่จะเลือกต้องกำลังอยู่บนหน้าจอ: เป็น anchor ตัวหนึ่ง หรือเป็น
    // สัญญาที่ selected ไว้แล้วตามที่ screen รายงาน
    let anchors = screen_anchor_ids(screen);
    let already_selected = screen
        .get("selected")
        .and_then(Value::as_str)
        .is_some_and(|s| s == reference);
    if !already_selected && !anchors.iter().any(|id| anchor_ref_matches(id, &reference)) {
        return Err(anchor_missing_error(&[reference], &anchors));
    }

    let mut action = json!({"kind": "select", "ref": reference});
    if let Some(open) = open {
        action["open"] = json!(open);
    }
    if let Some(price) = price {
        action["price"] = json!(price);
    }
    Ok(action)
}

// ---------- ui_point ----------

fn plan_point(args: &Value, screen: &Value) -> Result<Value, Value> {
    let Some(steps) = args.get("steps").and_then(Value::as_array) else {
        return Err(bad_args_error(
            "ขาดช่อง steps หรือไม่ใช่รายการ — ส่ง 1-5 ขั้นตอนของสิ่งที่จะชี้ เช่น {\"steps\": [{\"anchor\": \"tab:portfolio\", \"effect\": \"circle\"}]}".into(),
        ));
    };
    if steps.is_empty() {
        return Err(bad_args_error(
            "ต้องมีอย่างน้อย 1 ขั้นตอน — เพิ่ม step ที่อยากชี้แล้วเรียกใหม่".into(),
        ));
    }
    if steps.len() > MAX_POINT_STEPS {
        return Err(json!({
            "error": "too_many_steps",
            "message": format!(
                "ขั้นตอนการชี้ต้องมี 1-5 ขั้นต่อครั้ง (ส่งมา {} ขั้น) — เลือกเฉพาะจุดที่สำคัญที่สุดแล้วเรียกใหม่",
                steps.len()
            ),
        }));
    }

    let mut planned: Vec<Value> = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        let step_no = index + 1;
        let Some(anchor) = optional_trimmed(step.get("anchor")) else {
            return Err(bad_args_error(format!(
                "step ที่ {step_no} ขาดช่อง anchor — ใส่ id ของสิ่งที่จะชี้ เช่น tab:portfolio"
            )));
        };
        let Some(effect) = step.get("effect").and_then(Value::as_str) else {
            return Err(bad_args_error(format!(
                "step ที่ {step_no} ขาดช่อง effect — เลือกจาก: {}",
                EFFECTS.join(", ")
            )));
        };
        if !EFFECTS.contains(&effect) {
            return Err(bad_args_error(format!(
                "effect ของ step ที่ {step_no} ต้องเป็นค่าใดค่าหนึ่งจาก: {} (ส่งมา {effect})",
                EFFECTS.join(", ")
            )));
        }
        let mut planned_step = json!({"anchor": anchor, "effect": effect});
        if let Some(message) = optional_trimmed(step.get("message")) {
            let length = message.chars().count();
            if length > MAX_MESSAGE_CHARS {
                return Err(json!({
                    "error": "message_too_long",
                    "message": format!(
                        "ข้อความของ step ที่ {step_no} ยาว {length} ตัวอักษร เกินสูงสุด {MAX_MESSAGE_CHARS} ตัวอักษร — สั้นลงแล้วเรียกใหม่"
                    ),
                }));
            }
            planned_step["message"] = json!(message);
        }
        if let Some(color) = optional_trimmed(step.get("color")) {
            if !COLORS.contains(&color.as_str()) {
                return Err(bad_args_error(format!(
                    "color ของ step ที่ {step_no} ต้องเป็น accent, warn หรือ danger (ส่งมา {color})"
                )));
            }
            planned_step["color"] = json!(color);
        }
        planned.push(planned_step);
    }

    // anchor ทุกตัวต้องมีอยู่จริงบนหน้าจอ (id ตรงกันเป๊ะ) — รวมของที่หาย
    // ทั้งชุดไว้ใน error เดียวเพื่อให้โมเดลแก้ครั้งเดียวจบ
    let anchors = screen_anchor_ids(screen);
    let mut missing: Vec<String> = Vec::new();
    for step in &planned {
        let anchor = step["anchor"].as_str().unwrap_or_default();
        if !anchors.iter().any(|id| id == anchor) && !missing.iter().any(|m| m == anchor) {
            missing.push(anchor.to_string());
        }
    }
    if !missing.is_empty() {
        return Err(anchor_missing_error(&missing, &anchors));
    }

    Ok(json!({"kind": "point", "steps": planned}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::contract::ToolErrorCode;
    use crate::agent::registry::Registry;

    fn screen(mode: &str, anchors: &[&str]) -> Value {
        json!({
            "mode": mode,
            "page": "home",
            "symbol": null,
            "selected": null,
            "anchors": anchors
                .iter()
                .map(|id| json!({"id": id, "label": "ป้าย"}))
                .collect::<Vec<_>>(),
        })
    }

    fn err_code(err: &Value) -> &str {
        err.get("error").and_then(Value::as_str).unwrap_or("<none>")
    }

    fn six_steps_args() -> Value {
        json!({"steps": (0..6)
            .map(|i| json!({"anchor": format!("a:{i}"), "effect": "circle"}))
            .collect::<Vec<_>>()})
    }

    // ---------- specs ----------

    #[test]
    fn three_ui_specs_follow_the_conventions() {
        let specs = specs();
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["ui_go", "ui_select", "ui_point"]);
        for spec in &specs {
            assert_eq!(spec.tier, Tier::Ui, "{}", spec.name);
            assert_eq!(spec.modes, vec![Mode::Easy, Mode::Pro], "{}", spec.name);
            assert_eq!(spec.timeout_ms, 6000, "{}", spec.name);
            assert_eq!(spec.webull_cost, 0, "{}", spec.name);
            assert!(
                spec.description.contains("ใช้เมื่อ:"),
                "{}: {}",
                spec.name,
                spec.description
            );
            assert!(
                spec.description.contains("ไม่ใช้เมื่อ:"),
                "{}: {}",
                spec.name,
                spec.description
            );
            assert!(!spec.examples.is_empty(), "{} needs examples", spec.name);
            assert_eq!(
                spec.parameters.get("additionalProperties"),
                Some(&json!(false)),
                "{} must lock its schema",
                spec.name
            );
            assert!(
                spec.to_tool_def()
                    .function
                    .description
                    .contains("ตัวอย่างการเรียก:")
            );
        }
        // คำอธิบายหน้า easy เป็นภาษาบ้านๆ ตามที่ types/แผนกำหนด
        let go = &specs[0];
        for plain in [
            "รายการไอเดียของหุ้นตัวหนึ่ง",
            "คำถาม 5 ข้อของไอเดียที่เลือก",
            "เช็กลิสต์ก่อนตัดสินใจ",
        ] {
            assert!(go.description.contains(plain), "missing: {plain}");
        }
    }

    #[test]
    fn every_example_passes_schema_validation_in_both_modes() {
        let mut reg = Registry::new();
        for spec in specs() {
            reg.register(spec).expect("register ui spec");
        }
        for spec in specs() {
            for mode in [Mode::Easy, Mode::Pro] {
                for ex in &spec.examples {
                    let args = ex.to_string();
                    reg.validate(&spec.name, mode, &args).unwrap_or_else(|e| {
                        panic!("example {args} of {} failed: {}", spec.name, e.message)
                    });
                }
            }
        }
    }

    #[test]
    fn registry_enforces_enums_but_not_the_size_limits() {
        let mut reg = Registry::new();
        for spec in specs() {
            reg.register(spec).expect("register ui spec");
        }
        let err = reg
            .validate("ui_go", Mode::Easy, r#"{"page": "s5"}"#)
            .expect_err("page outside the enum");
        assert_eq!(err.code, ToolErrorCode::NotInEnum);
        let err = reg
            .validate("ui_point", Mode::Easy, r#"{"steps": [{"anchor": "a"}]}"#)
            .expect_err("effect is required per item schema");
        assert_eq!(err.code, ToolErrorCode::MissingField);
        // maxItems/maxLength ไม่อยู่ในชุดที่ registry ตรวจ — pure checker จับเอง
        reg.validate("ui_point", Mode::Easy, &six_steps_args().to_string())
            .expect("registry does not enforce maxItems");
    }

    // ---------- ui_go ----------

    #[test]
    fn go_allowed_pages_per_mode() {
        for page in ["home", "market", "monitor", "lab", "portfolio", "watchlist"] {
            let action = plan_ui_call(
                "ui_go",
                &json!({"page": page}),
                Some(&screen("pro", &["tab:market"])),
            )
            .unwrap_or_else(|e| panic!("{page} must be allowed in pro: {e}"));
            assert_eq!(action["kind"], json!("go"));
            assert_eq!(action["page"], json!(page));
        }
        for page in ["home", "s2", "s3", "s4"] {
            plan_ui_call(
                "ui_go",
                &json!({"page": page, "symbol": "NVDA", "ref": "NVDA261218P00175000"}),
                Some(&screen("easy", &[])),
            )
            .unwrap_or_else(|e| panic!("{page} must be allowed in easy: {e}"));
        }
        // หน้าของอีกโหมดหนึ่งถูกปฏิเสธพร้อมรายชื่อที่ใช้ได้ของโหมดนี้
        for (mode, page, allowed) in [
            ("easy", "market", vec!["home", "s2", "s3", "s4"]),
            (
                "pro",
                "s2",
                vec!["home", "market", "monitor", "lab", "portfolio", "watchlist"],
            ),
        ] {
            let err = plan_ui_call("ui_go", &json!({"page": page}), Some(&screen(mode, &[])))
                .expect_err("cross-mode page must be denied");
            assert_eq!(err_code(&err), "page_not_allowed");
            assert_eq!(err["allowed"], json!(allowed));
            assert!(err["message"].as_str().unwrap().contains(page));
        }
    }

    #[test]
    fn go_exact_action_json() {
        assert_eq!(
            plan_ui_call(
                "ui_go",
                &json!({"page": "home"}),
                Some(&screen("easy", &[]))
            )
            .expect("plan"),
            json!({"kind": "go", "mode": "easy", "page": "home", "symbol": null, "ref": null})
        );
        assert_eq!(
            plan_ui_call(
                "ui_go",
                &json!({"page": "monitor", "symbol": "NVDA"}),
                Some(&screen("pro", &[]))
            )
            .expect("plan"),
            json!({"kind": "go", "mode": "pro", "page": "monitor", "symbol": "NVDA", "ref": null})
        );
        assert_eq!(
            plan_ui_call(
                "ui_go",
                &json!({"page": "s3", "ref": "NVDA261218P00175000"}),
                Some(&screen("easy", &[]))
            )
            .expect("plan"),
            json!({
                "kind": "go", "mode": "easy", "page": "s3",
                "symbol": null, "ref": "NVDA261218P00175000"
            })
        );
    }

    #[test]
    fn go_required_symbol_and_ref_on_easy_pages() {
        let err = plan_ui_call("ui_go", &json!({"page": "s2"}), Some(&screen("easy", &[])))
            .expect_err("s2 needs symbol");
        assert_eq!(err_code(&err), "bad_args");
        assert!(err["message"].as_str().unwrap().contains("symbol"));
        for page in ["s3", "s4"] {
            let err = plan_ui_call(
                "ui_go",
                &json!({"page": page, "symbol": "NVDA"}),
                Some(&screen("easy", &[])),
            )
            .expect_err("s3/s4 need ref");
            assert_eq!(err_code(&err), "bad_args");
            assert!(err["message"].as_str().unwrap().contains("ref"));
        }
        // ค่าว่างๆ (ช่องว่างล้วน) ก็ถือว่าไม่ได้ใส่
        let err = plan_ui_call(
            "ui_go",
            &json!({"page": "s2", "symbol": "  "}),
            Some(&screen("easy", &[])),
        )
        .expect_err("blank symbol is missing");
        assert_eq!(err_code(&err), "bad_args");
    }

    // ---------- ui_select ----------

    #[test]
    fn select_exact_action_json_and_validation() {
        let sc = screen(
            "pro",
            &["chain:NVDA261218P00175000", "chain:TSLA260117C00400000"],
        );
        assert_eq!(
            plan_ui_call(
                "ui_select",
                &json!({"ref": "NVDA261218P00175000"}),
                Some(&sc)
            )
            .expect("plan"),
            json!({"kind": "select", "ref": "NVDA261218P00175000"})
        );
        assert_eq!(
            plan_ui_call(
                "ui_select",
                &json!({"ref": "chain:TSLA260117C00400000", "open": "detail"}),
                Some(&sc)
            )
            .expect("plan"),
            json!({"kind": "select", "ref": "chain:TSLA260117C00400000", "open": "detail"})
        );
        assert_eq!(
            plan_ui_call(
                "ui_select",
                &json!({"ref": "NVDA261218P00175000", "open": "what_if", "price": 120.5}),
                Some(&sc)
            )
            .expect("plan"),
            json!({
                "kind": "select", "ref": "NVDA261218P00175000",
                "open": "what_if", "price": 120.5
            })
        );
        // สัญญาที่ selected ไว้แล้วเลือกซ้ำได้ (เช่นเปิด what_if จากของเดิม)
        let mut selected = screen("easy", &["idea:OTHER2601C00100000"]);
        selected["selected"] = json!("NVDA261218P00175000");
        plan_ui_call(
            "ui_select",
            &json!({"ref": "NVDA261218P00175000", "open": "detail"}),
            Some(&selected),
        )
        .expect("already-selected ref is on screen");
    }

    #[test]
    fn select_bad_args_shapes() {
        let sc = screen("pro", &["chain:NVDA261218P00175000"]);
        for (args, hint) in [
            (json!({}), "ref"),
            (
                json!({"ref": "NVDA261218P00175000", "price": 100.0}),
                "open = what_if",
            ),
            (
                json!({"ref": "NVDA261218P00175000", "open": "detail", "price": 1.0}),
                "open = what_if",
            ),
        ] {
            let err = plan_ui_call("ui_select", &args, Some(&sc)).expect_err("bad call must fail");
            assert_eq!(err_code(&err), "bad_args", "{args}");
            assert!(err["message"].as_str().unwrap().contains(hint), "{args}");
        }
    }

    #[test]
    fn select_missing_anchor_lists_hints() {
        let sc = screen(
            "pro",
            &["chain:NVDA261218P00175000", "chain:TSLA260117C00400000"],
        );
        let err = plan_ui_call(
            "ui_select",
            &json!({"ref": "AAPL260116C00250000"}),
            Some(&sc),
        )
        .expect_err("unknown contract");
        assert_eq!(err_code(&err), "anchor_missing");
        assert_eq!(err["missing"], json!(["AAPL260116C00250000"]));
        assert_eq!(
            err["hint"],
            json!(["chain:NVDA261218P00175000", "chain:TSLA260117C00400000"])
        );
    }

    // ---------- ui_point ----------

    #[test]
    fn point_exact_action_json() {
        let sc = screen("easy", &["tab:home", "whatif:price"]);
        assert_eq!(
            plan_ui_call(
                "ui_point",
                &json!({"steps": [
                    {"anchor": "tab:home", "effect": "circle", "message": "ดูตรงนี้", "color": "warn"},
                    {"anchor": "whatif:price", "effect": "underline"}
                ]}),
                Some(&sc)
            )
            .expect("plan"),
            json!({"kind": "point", "steps": [
                {"anchor": "tab:home", "effect": "circle", "message": "ดูตรงนี้", "color": "warn"},
                {"anchor": "whatif:price", "effect": "underline"}
            ]})
        );
    }

    #[test]
    fn point_six_steps_is_too_many() {
        let sc = screen("easy", &["a:0", "a:1", "a:2", "a:3", "a:4", "a:5"]);
        let err = plan_ui_call("ui_point", &six_steps_args(), Some(&sc)).expect_err("6 steps");
        assert_eq!(err_code(&err), "too_many_steps");
        assert!(err["message"].as_str().unwrap().contains('6'));
        // 5 ขั้นพอดีผ่าน
        let five = json!({"steps": (0..5)
            .map(|i| json!({"anchor": format!("a:{i}"), "effect": "box"}))
            .collect::<Vec<_>>()});
        plan_ui_call("ui_point", &five, Some(&sc)).expect("5 steps is the cap");
    }

    #[test]
    fn point_message_limit_counts_chars_not_bytes() {
        let sc = screen("easy", &["tab:home"]);
        let ok_len: String = "ทด".repeat(60); // 120 ตัวอักษรไทย = ~360 ไบต์
        assert_eq!(ok_len.chars().count(), 120);
        plan_ui_call(
            "ui_point",
            &json!({"steps": [{"anchor": "tab:home", "effect": "bubble", "message": ok_len}]}),
            Some(&sc),
        )
        .expect("120 chars pass even though bytes exceed 120");
        let too_long: String = "ทด".repeat(60) + "x"; // 121 ตัวอักษร
        let err = plan_ui_call(
            "ui_point",
            &json!({"steps": [{"anchor": "tab:home", "effect": "bubble", "message": too_long}]}),
            Some(&sc),
        )
        .expect_err("121 chars");
        assert_eq!(err_code(&err), "message_too_long");
    }

    #[test]
    fn point_anchor_missing_with_ordered_hints() {
        // 25 anchors เกินโควตา hint (20): tab 15 + idea 3 + chart 7 —
        // ตัวที่ใช้ kind เดียวกับที่หาย (idea:) และมี ref ตรงกันต้องขึ้นหัว
        // ก่อน tab/chart ทั้งหมด แล้วตัดที่ 20 ตามลำดับความสำคัญของ screen
        let mut ids: Vec<String> = (0..15).map(|i| format!("tab:x{i}")).collect();
        ids.extend([
            "idea:NVDA261218P00175000/premium".to_string(),
            "idea:NVDA261218P00175000/size".to_string(),
            "idea:AAPL260116C00250000/premium".to_string(),
        ]);
        ids.extend((0..7).map(|i| format!("chart:q{i}")));
        let mut sc = screen("easy", &[]);
        sc["anchors"] = json!(
            ids.iter()
                .map(|id| json!({"id": id, "label": "ป้าย"}))
                .collect::<Vec<_>>()
        );

        let err = plan_ui_call(
            "ui_point",
            &json!({"steps": [{"anchor": "idea:NVDA261218P00175000/strike", "effect": "circle"}]}),
            Some(&sc),
        )
        .expect_err("strike anchor does not exist");
        assert_eq!(err_code(&err), "anchor_missing");
        assert_eq!(err["missing"], json!(["idea:NVDA261218P00175000/strike"]));
        let hint = err["hint"].as_array().expect("hint array");
        assert_eq!(hint.len(), MAX_HINTS, "capped at 20");
        // สามตัวแรก = kind idea: โดยคู่ที่ ref ตรงกัน (NVDA…) มาก่อน AAPL
        assert_eq!(hint[0], json!("idea:NVDA261218P00175000/premium"));
        assert_eq!(hint[1], json!("idea:NVDA261218P00175000/size"));
        assert_eq!(hint[2], json!("idea:AAPL260116C00250000/premium"));
        // ที่เหลือเป็น tab/chart ตามลำดับ screen (ไม่มี idea หลุดมาอีก)
        for h in hint.iter().skip(3) {
            let h = h.as_str().unwrap();
            assert!(
                h.starts_with("tab:") || h.starts_with("chart:"),
                "tail: {h}"
            );
        }
        assert_eq!(hint[3], json!("tab:x0"));
    }

    #[test]
    fn point_collects_all_missing_anchors_without_duplicates() {
        let sc = screen("easy", &["ok:1"]);
        let err = plan_ui_call(
            "ui_point",
            &json!({"steps": [
                {"anchor": "nope:a", "effect": "circle"},
                {"anchor": "nope:b", "effect": "box"},
                {"anchor": "nope:a", "effect": "box"},
            ]}),
            Some(&sc),
        )
        .expect_err("missing anchors");
        assert_eq!(err_code(&err), "anchor_missing");
        assert_eq!(err["missing"], json!(["nope:a", "nope:b"]));
    }

    // ---------- no screen ----------

    #[test]
    fn no_screen_context_or_unknown_mode_answers_no_screen() {
        for name in ["ui_go", "ui_select", "ui_point"] {
            let err = plan_ui_call(name, &json!({}), None).expect_err("no screen");
            assert_eq!(err_code(&err), "no_screen");
            let message = err["message"].as_str().unwrap();
            assert!(
                message.contains("ข้อความ"),
                "tell the model to answer in text: {message}"
            );
        }
        // screen ไม่มี mode (หรือ mode แปลก) = มองไม่เห็นหน้าจอเหมือนกัน
        for broken in [
            json!({"page": "home"}),
            json!({"mode": "wizard", "anchors": []}),
        ] {
            let err = plan_ui_call("ui_go", &json!({"page": "home"}), Some(&broken))
                .expect_err("unusable screen");
            assert_eq!(err_code(&err), "no_screen");
        }
    }

    #[test]
    fn unknown_ui_name_is_bad_args() {
        let err = plan_ui_call("ui_dance", &json!({}), Some(&screen("pro", &[])))
            .expect_err("not a ui tool");
        assert_eq!(err_code(&err), "bad_args");
    }
}
