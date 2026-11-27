# PLAN: PREMIUM RADAR Agent (W6) · Tools + Harness + Agentic UI

**วันที่:** 2026-11-27 · **สถานะ:** รอ THP approve · **branch ที่จะใช้:** `feat/agent-harness` (แตกจาก `main` @ `c2086e7`)
**อ้างอิง design:** artifact "Radar Agent Tools" v2 (https://claude.ai/artifact/68oia1JbkviL7QAdsnmuHu) · brain `decisions/decision-premium-radar-north-star`, `concepts/webull-th-ai-resources`

---

## 0. สรุปสำหรับคนทั่วไป

เราจะเปลี่ยนกล่องแชท AI ในแอป จาก "ตอบคำถามจากตารางสแกนได้อย่างเดียว" เป็นผู้ช่วยที่หยิบข้อมูลได้แทบทุกส่วนของแอป พาเปิดหน้าจอ และชี้ให้ดูบนจอได้เอง ทั้งวงกลม ไฮไลต์ และข้อความกะพริบ ส่วนตัวเลขที่ AI พูด ระบบจะตรวจทุกครั้งว่ามาจากข้อมูลจริง

**ตัวอย่างผลลัพธ์:** ภรรยาพิมพ์ว่า "อยากได้เดือนละ $200 มีอะไรดี" แล้ว AI ทำตามลำดับนี้

1. อ่านงบและหุ้นที่เธออยากถือ
2. หาไอเดีย แล้วพาไปหน้าไอเดีย
3. วงกลมตัวเลข "รับเงินทันที $183" และทำให้ "⚠ งบออก 18 ธ.ค." กะพริบ
4. ขึ้นการ์ดที่ตอบครบ 5 ข้อ (ได้เงินเท่าไร / แลกกับอะไร / โอกาสแค่ไหน / ทำไมช่วงนี้ / แย่สุดคืออะไร)
5. ถามว่า "เอาตัวไหน"

ถ้า AI พิมพ์ตัวเลขที่ไม่มีที่มา ระบบจะส่งคำตอบกลับไปให้แก้ก่อนแสดง

**สิ่งที่ไม่ทำในแผนนี้:**
- ส่ง order จริง (รอ W4)
- ต่อ Webull MCP เข้าแอป
- สัญญาณ OI ใหม่ (รอ W5)
- คุมหน้าจอแบบคลิกพิกเซล

## 1. สิ่งที่ตกลงแล้ว

| # | เรื่อง | ข้อสรุป |
|---|---|---|
| โหมดคุมจอ | ข+ค | นำทางทีละขั้นสำหรับคำถามเฉพาะ และทำทั้ง flow เมื่อเป้าหมายกว้าง · ผู้ใช้กดหยุดได้ตลอด |
| ผู้ใช้ | ภรรยา + THP | แยก playbook/น้ำเสียงตาม `ui_mode` (ง่าย / ละเอียด) |
| D1 | harness 7 ชั้น | LLM ไม่คิดเลข · grounding check · `present_idea` แบบมีโครงสร้าง · tools ต่อโหมด 12/17 + playbook · สัญญา tool ที่ชัด · รั้วกันวน · eval set |
| D2 | ข้อมูลพอร์ต | ส่งได้ทั้งหมด **ยกเว้นเลขบัญชี** |
| D3 | งานนาน | tool รอจนเสร็จ (≤ 90 วินาที) + ส่ง progress ระหว่างรอ |
| D4 | library ชี้จอ | `rough-notation` (MIT, ~4KB) · spotlight/กะพริบเขียน CSS เอง · **ต้องได้ OK จาก THP ก่อนติดตั้ง** |
| D5 | eval | Claude ร่าง ~30 ข้อ + THP เติมคำถามจริงของภรรยา 5–10 ข้อ · กัน 1/3 เป็น held-out |
| LLM | provider-agnostic | ยังเป็น OpenAI-compatible (ค่าเริ่มต้น GLM) ทุกกลไกทำใน harness เราเอง ไม่พึ่งฟีเจอร์เฉพาะ vendor |

## 2. โครงสร้างโค้ดที่จะเพิ่ม

```
crates/radar-core/src/agent/
  mod.rs          # public API: AgentRequest, AgentEvent, run_agent()
  contract.rs     # ToolSpec (name, tier, modes, description, examples, schema), Tier, Mode
  registry.rs     # ลงทะเบียน tools, กรองตามโหมด, validate args กับ schema
  runner.rs       # agent loop (แทน chat_with_tool_runner), รอบ/งบ/timeout/กันเรียกซ้ำ/cancel
  grounding.rs    # ดึงตัวเลขจากคำตอบ เทียบกับผล tools
  tools/{market,ideas,mine,ui,present,playbook}.rs
  playbooks/*.md  # include_str! — oi-trader, assignment, five-answers, tone-easy, tone-pro
src-tauri/src/agent_cmd.rs   # agent_ask (ipc::Channel stream), agent_ui_result, agent_confirm, agent_cancel
src/agent/
  bus.ts          # รับ AgentEvent, ทำ ui action, ส่งผลกลับ
  anchors.tsx     # <AgentAnchor id> / useAgentAnchor — ลงทะเบียน anchors ที่แสดงอยู่
  point.ts        # ui_point: rough-notation + CSS (blink/pulse/spotlight/bubble)
  AgentPanel.tsx  # แชท streaming, timeline ขั้นตอน, การ์ดยืนยัน, ปุ่มหยุด/ย้อน
  IdeaCard.tsx    # การ์ด present_idea (ตัวเลขดึงจาก ref)
tests: crates/radar-core/tests/agent_*.rs (wiremock แบบเดียวกับ copilot_runner.rs)
eval:  crates/radar-core/examples/agent_eval.rs + tests/fixtures/agent_eval/*.json
```

`copilot.rs` เก็บ `LlmConfig`/`ChatMessage` ไว้ใช้ต่อ · `ask_copilot` เดิมยังอยู่จน P3 เสร็จแล้วค่อยลบ (P4-T4)

## 3. Phases → Tasks

สัญลักษณ์: 🧵 = ลำดับ (ต้องรอตัวก่อน) · ⚡ = ขนานได้ (worktree แยก) · 🧠 = thought level ของ worker

### P0 · เตรียมงาน (Claude ทำเอง)

| Task | งาน | ไฟล์ | DoD |
|---|---|---|---|
| P0-T1 | `AGENTS.md` กติกาถาวรของลูกน้อง | `AGENTS.md` | commit แล้ว (worktree เห็น) · มี VERIFY, ขอบเขต, รูปแบบ REPORT |
| P0-T2 | branch + บันทึก decision | git, brain | `feat/agent-harness` · brain `decisions/decision-premium-radar-agent-harness` (`commit: false`) |

**DoD ของ Phase:** worker ตัวแรกเริ่มงานได้โดยไม่ต้องอธิบายกติกาซ้ำใน contract

### P1 · แกน harness ใน Rust 🧵

| Task | งาน | ไฟล์ | Tests | DoD | ความเสี่ยง → วิธีลด |
|---|---|---|---|---|---|
| P1-T1 🧠max | `contract.rs` + `registry.rs`: ToolSpec, Tier, Mode, กรองตามโหมด, validate args (type/enum/required) → error ภาษาไทยที่บอกวิธีแก้ | `agent/{mod,contract,registry}.rs`, `lib.rs` | unit: กรองโหมด, validate ผ่าน/ไม่ผ่าน, ToolSpec → OpenAI tools JSON | `cargo test -p radar-core --all-features agent::` เขียว · tool ที่ไม่อยู่ในโหมดเรียกไม่ได้ | schema validator เขียนเองบวม → รองรับแค่ subset ที่ใช้ (object/string/number/integer/boolean/enum/array) |
| P1-T2 🧠max | `runner.rs`: loop + `AgentEvent` (Thinking/ToolStart/ToolEnd/UiAction/NeedConfirm/Answer/Error) ผ่าน callback · max 12 calls · งบ Webull · timeout ต่อ tool · กันเรียกซ้ำ · cancel flag · ปลด engine lock ระหว่าง HTTP | `agent/runner.rs` | wiremock: 2 รอบ, เกินรอบ, เรียกซ้ำ, cancel กลางทาง, lock ไม่ค้าง (เลียน `copilot_runner.rs`) | tests ใหม่ผ่าน + `copilot_runner.rs` เดิมยังผ่าน | blocking reqwest ใน tokio → คงรูปแบบ OS thread แบบเดิม |
| P1-T3 🧠max | `grounding.rs`: ดึงตัวเลข ($, %, จำนวน, วันที่) จากคำตอบ เทียบกับผล tools (ยอมปัดเศษ/หน่วย) · runner ส่งกลับแก้ 1 ครั้ง ถ้ายังไม่ผ่านใส่ flag `unverified` | `agent/grounding.rs`, `runner.rs` | unit ≥ 15 เคส: $183 vs 183.0, 18% vs 0.18, "1 ใน 5", ปี/วันที่ไม่นับ, เลขลำดับ "1)" ไม่นับ | ไม่มี false positive บนชุด 10 คำตอบตัวอย่างที่ถูก · จับตัวเลขแต่งได้ทุกตัวในชุดผิด | false positive ทำให้รำคาญ → whitelist ตัวเลขทั่วไป, วัดบน eval ก่อนเปิดบังคับ |
| P1-T4 🧠high | Tauri `agent_ask` (stream ด้วย `tauri::ipc::Channel<AgentEvent>`), `agent_ui_result`, `agent_confirm`, `agent_cancel` · ส่ง `screen_context` เข้า request | `src-tauri/src/agent_cmd.rs`, `lib.rs` | `cargo clippy` + test ของ wiring ที่ทำได้โดยไม่เปิดหน้าต่าง | `npm run verify` เขียว | ui action รอ ack จากหน้าบ้าน → oneshot + timeout 5 วินาที แล้วคืน `ui_timeout` ให้ agent |

**DoD ของ Phase:** agent loop ใหม่รันได้กับ tools เดิม 3 ตัว (ย้ายเข้า registry) ผ่าน `agent_ask` · `npm run verify` เขียว

### P2 · Read tools (12 ตัว) ⚡ แบ่ง 3 worker หลัง P1

| Task | Tools | ไฟล์ | DoD |
|---|---|---|---|
| P2-T1 ⚡🧠high | `market_pulse`, `stock_brief`, `price_behavior` | `agent/tools/market.rs` | ผลย่อ ≤ ~1.5KB/ครั้ง · มี `as_of`/`source` · `price_behavior` คืนตัวเลขสรุป (ช่วงราคา, ความผันผวนจริง, แนวโน้ม) ไม่ส่งแท่งดิบ |
| P2-T2 ⚡🧠max | `find_ideas`, `scan_chain`, `contract_detail`, `chain_stats`, `what_if` | `agent/tools/ideas.rs` | `scan_chain` รอ ≤ 90 วินาที + ส่ง progress · `what_if` ใช้ `lab_analyze` · ตัวเลขทุกตัวมาจาก `radar_core` |
| P2-T3 ⚡🧠high | `my_portfolio` (ตัดเลขบัญชี), `my_profile`, `my_watchlists`, `read_playbook` + playbook ร่างแรก 5 ไฟล์ | `agent/tools/{mine,playbook}.rs`, `agent/playbooks/*.md` | test ยืนยันว่าไม่มีเลขบัญชี/`account_id` หลุดในผล · playbook ภาษาไทยตาม PRODUCT.md |

**หมายเหตุระหว่างทำ (2026-11-27):**
- โครงสร้างโค้ด: สเปก tool และตัวสรุปผลอยู่ที่ `radar-core/agent/tools/<family>.rs` (ทดสอบด้วย fixture) ส่วนการดึงข้อมูลอยู่ที่ `src-tauri/src/agent_tools/<family>.rs` ผ่าน `ToolCtx` + `dispatch`
- เอา `top_candidates` ออกจาก agent เพราะ `scan_chain` ทำแทนแล้ว (แชทเดิมยังใช้ได้จนถึง P4-T4) · `chain_stats` เหลือแค่โหมดละเอียด
- เลื่อน progress ระหว่างรอ scan ไปทำใน P3-T4 เพราะตอนนี้ UI รู้จาก `ToolStart` อยู่แล้ว
- เลื่อน `agent_ui_result` และ `agent_confirm` จาก P1-T4 ไปทำใน P3-T1 / P4-T3

ทุก tool ต้องมี: คำอธิบายแบบ "ใช้เมื่อ / ไม่ใช้เมื่อ" + ตัวอย่างการเรียก 1–3 แบบ · enum ทุกที่ที่ทำได้ · error ที่บอกวิธีแก้ · unit test ด้วย fixture (ไม่เรียก Webull จริง)

**DoD ของ Phase:** โหมดง่ายเห็น 12 tools, โหมดละเอียดเห็น 17 (นับรวม ui/present) · `cargo test --workspace` เขียว · Claude ตรวจผลย่อของทุก tool ด้วยตัวเอง

### P3 · Agentic UI ⚡ ขนานกับ P2 ได้ (แตะ `src/` เท่านั้น ยกเว้น T1)

| Task | งาน | ไฟล์ | DoD |
|---|---|---|---|
| P3-T1 🧠max | tools ฝั่ง Rust: `ui_go`, `ui_select`, `ui_point` (schema + ตรวจ anchor กับ `screen_context`, คืน `missing` + `hint`) | `agent/tools/ui.rs` | anchor ที่ไม่อยู่บนจอ → คืน hint รายการที่มีจริง |
| P3-T2 ⚡🧠max | `bus.ts` + `anchors.tsx`: ลงทะเบียน/ถอน anchors ตาม mount, ส่ง `screen_context` (โหมด, หน้า, สัญญาที่เลือก, anchors ≤ 60 ตัว) | `src/agent/{bus.ts,anchors.tsx}` | `npm run check` เขียว · anchor หายเมื่อ component unmount |
| P3-T3 ⚡🧠high | `point.ts`: circle/underline/highlight/box (rough-notation), blink/pulse/spotlight/bubble (CSS), ลำดับขั้น, ล้างเมื่อคลิก/เลื่อนจอ, รองรับ reduced-motion | `src/agent/point.ts`, `src/agent/agent.css`, `package.json` | **หลัง THP OK D4** · ทุกเอฟเฟกต์เห็นได้ใน Tauri · ไม่ค้างหลังเปลี่ยนหน้า |
| P3-T4 ⚡🧠max | `AgentPanel.tsx`: streaming, timeline ขั้นตอน, แถบ "AI กำลัง…", ปุ่มหยุด/ย้อน, การ์ดยืนยัน 🟡 · แทนที่ RadarAssistant ทั้งสองโหมด | `src/agent/AgentPanel.tsx`, `src/App.tsx`, `src/easy/EasyApp.tsx` | สลับโหมดแล้วแชทยังใช้ได้ · กดหยุดแล้ว agent หยุดจริง |
| P3-T5 🧠high | ติด anchors ที่หน้าจอหลัก: การ์ดไอเดีย (โหมดง่าย, รายช่อง), WhatIf, ตารางสัญญา, การ์ดพอร์ต, แท็บหลัก | `src/easy/*.tsx`, `src/components/{OptionsTable,OpportunityCards,PortfolioPage}/**` | anchor id ตามรูปแบบ `kind:ref[/field]` · ไม่เปลี่ยนหน้าตา UI |

**DoD ของ Phase:** ใน `npm run tauri dev` สั่งแชทว่า "พาไปดู NVDA แล้ววงกลมโอกาสโดนซื้อ" แล้วจอทำได้จริง (Claude ตรวจด้วย screenshot) · `npm run verify` เขียว

### P4 · คำแนะนำแบบมีโครงสร้าง + โหมด 🧵 หลัง P2, P3

| Task | งาน | ไฟล์ | DoD |
|---|---|---|---|
| P4-T1 🧠max | `present_idea` (ช่องบังคับ 5 ข้อ + `ref`) + `IdeaCard.tsx` ที่ดึงตัวเลขจาก ref เอง | `agent/tools/present.rs`, `src/agent/IdeaCard.tsx` | ตัวเลขบนการ์ดตรงกับ `easy_ideas`/`contract_detail` 100% |
| P4-T2 🧠max | system prompt แยกตามโหมด + โหลด playbook อัตโนมัติ (five-answers สำหรับโหมดง่ายเสมอ) | `agent/runner.rs`, `agent/playbooks/*` | โหมดง่ายไม่มีศัพท์ option ในข้อความหลัก (ตรวจด้วย eval) |
| P4-T3 🧠high | `save_watchlist` (🟡 ต้องยืนยัน) ผ่าน `NeedConfirm` | `agent/tools/mine.rs`, `AgentPanel.tsx` | กด "ไม่" แล้ว agent ได้รับว่า "ถูกปฏิเสธ" และไม่ทำซ้ำ |
| P4-T4 🧠high | ลบ `ask_copilot` และโค้ดเก่า + ย้าย `FiveQuestions.tsx` มาใช้ `agent_ask` | `copilot.rs`, `lib.rs`, `src/easy/FiveQuestions.tsx`, `RadarAssistant/**` | ไม่มีโค้ดตาย · `npm run verify` เขียว |

### P5 · Eval + ปรับจูน 🧵

| Task | งาน | ไฟล์ | DoD |
|---|---|---|---|
| P5-T1 🧠max | runner ของ eval: โหลด fixture (snapshot ข้อมูลตลาดจาก recorder), รันกับ LLM จริงที่ตั้งค่าไว้, ให้คะแนนตามเกณฑ์ที่ตรวจได้, บันทึก transcript + 5 ค่า (ถูก, tool calls, token, เวลา, error) | `examples/agent_eval.rs`, `tests/fixtures/agent_eval/` | `cargo run --example agent_eval --all-features` ได้รายงาน JSON + Markdown |
| P5-T2 | ชุดคำถาม ~30 ข้อ (Claude ร่าง) + 5–10 ข้อจาก THP · แบ่ง dev/held-out | `tests/fixtures/agent_eval/tasks.json` | THP ตรวจรายการแล้ว |
| P5-T3 | ปรับคำอธิบาย tools/playbooks ตามผล (Claude อ่าน transcript เอง) | `agent/**` | held-out ≥ 85% ผ่าน · ไม่มีตัวเลขแต่งหลุดถึงผู้ใช้ · ตัวเลขเป้าหมายนี้ปรับได้หลังเห็น baseline |

**DoD ของงานทั้งหมด:** THP ลองในแอปจริงทั้งสองโหมดแล้วยอมรับ · รายงานสรุปต่อ task · เสนอ log เข้า brain

### P6 · (หลัง W4) `draft_order`: แยกแผนทีหลัง

## 4. ลำดับและการขนาน

```
P0 → P1 (T1→T2→T3, T4 ต่อจาก T2) → ┬─ P2-T1 ⚡ ─┐
                                    ├─ P2-T2 ⚡ ─┤
                                    ├─ P2-T3 ⚡ ─┼→ P4 → P5
                                    ├─ P3-T1 → P3-T2 ⚡ ┐
                                    │          P3-T3 ⚡ ├→ P3-T4 → P3-T5
```

ทำงานพร้อมกันไม่เกิน 3 worker (แผนขอแค่นี้ แม้ cap ของ bridge จะเป็น 5) เพื่อให้ Claude ตรวจทันทุกงาน

## 5. วิธีทำงานกับลูกน้อง (zcrew / ZCode GLM)

**บทบาท:** Claude เป็นเจ้าของงาน (spec, contract, ตรวจ, ตัดสิน pass/fail, รายงาน) · ZCode เป็นคนลงมือ (แก้โค้ด, เขียน test, แก้ error ของตัวเอง) และไม่ตัดสินใจเรื่อง product/architecture และไม่รับรองงานตัวเอง

**ทุก task มี contract** ครบ 7 ส่วน: OBJECTIVE · CONTEXT · SCOPE (`backtick` paths) · DO NOT · ACCEPTANCE CRITERIA (= DoD ข้างบน) · VERIFY (`backtick` commands) · REPORT · กติกาถาวร (VERIFY, ห้ามแตะ, รูปแบบรายงาน) อยู่ใน `AGENTS.md` แล้ว contract จึงเขียนเฉพาะเรื่องของ task นั้น

**รอบการทำงาน:**
1. `git status`: ไม่ทับงานที่ยังไม่ commit ของ THP
2. งานขนาน: `git worktree add ../option2-zc-<task> -b zc/<task>` · ทำ junction `node_modules` ให้ worktree ที่ต้องรัน npm
3. `agent-start` (model/thoughtLevel ตาม 🧠, `workspaceAccess: exclusive`, `mode: build`) → รอผ่าน `[zcrew]` notification ไม่ poll เอง
4. `zcrew gate <runId>` ตรวจ scope และรัน VERIFY บนเครื่องนี้ → Claude อ่าน `git diff` ทุกข้อใน JUDGMENT เอง
5. FAIL → ส่ง correction บน thread เดิม พร้อม error message ตรงตัว (สูงสุด 4 รอบ) · thread ใหญ่เกิน ~150k token หรือแก้ครบ 2 รอบ → เปิด thread ใหม่ด้วย contract ฉบับเต็ม
6. PASS → `zcrew gate --accept` → `agent-close` → merge `zc/<task>` เข้า `feat/agent-harness` ทีละตัว แล้วรัน VERIFY ซ้ำหลังแต่ละ merge
7. ลบ worktree: **ลบ junction `node_modules` ก่อนเสมอ** (`(Get-Item <wt>\node_modules).Delete()`) แล้วค่อย `git worktree remove` / `prune`

**ห้ามลูกน้อง:** commit/push/reset · แตะ `.env`/secrets/keyring · เพิ่ม dependency · ลบหรือ skip test · ขยาย scope เพื่อให้ test ผ่าน · เรียก Webull Trade API

**Claude รายงาน THP ต่อ task:** เปลี่ยนอะไร · หลักฐาน (gate/test ที่รันเอง) · DoD ผ่านกี่ข้อ · ที่ยังค้าง · ดูงาน worker สดได้ด้วย `zcrew watch` หรือ `zcrew dashboard`

**การ commit:** ขออนุญาต THP ให้ Claude commit งานที่ตรวจผ่านแล้วลง `feat/agent-harness` ได้ (แบบเดียวกับ W1) · push/merge `main` เฉพาะเมื่อ THP สั่ง

## 6. ความเสี่ยงรวม

| ความเสี่ยง | ผล | วิธีลด |
|---|---|---|
| GLM เรียก tool/ใส่ค่าแม่นน้อยกว่า Claude | ตอบผิด | eval เทียบโมเดลตั้งแต่ P5 · validate args + error ที่บอกวิธีแก้ |
| grounding check ดักคำตอบที่ถูก | น่ารำคาญ | วัดบน eval ก่อนเปิดโหมดบังคับ · ใช้ flag แทนการบล็อก |
| ข้อมูลพอร์ตไปที่ LLM ภายนอก | privacy | THP ยอมรับแล้ว (ยกเว้นเลขบัญชี) · มี test กันเลขบัญชีหลุด |
| prompt injection จากข้อความภายนอก | agent ถูกหลอก | tools 🟡 ต้องยืนยันทุกครั้ง · ไม่มี tool 🔴 ในแผนนี้ |
| quota Webull ชนกับการสแกน | ข้อมูลไม่มา | งบต่อคำถาม · อ่าน recorder/แคชก่อน |
| แก้ `App.tsx`/`EasyApp.tsx` ชนกันระหว่าง worker | merge conflict | ไฟล์เหล่านี้อยู่ใน task เดียว (P3-T4) · P3-T5 ทำหลัง T4 |
