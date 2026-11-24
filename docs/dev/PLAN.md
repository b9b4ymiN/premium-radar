# PLAN — PREMIUM RADAR (option2)

**วันที่อนุมัติแผน:** 2026-11-22 · **สถานะ:** รอ approve
**อ้างอิง:** `PRODUCT.md` (product truth) · `design/DESIGN.md` (visual/structure freeze) · `design/premium_radar_modern_v2.html` (visual source of truth) · `Short_Option_Radar_PY.py` (quant logic source of truth) · brain `projects/premium-radar-option2`

---

## 0. ภาพรวม

Port "PREMIUM RADAR" (Tkinter, 3,507 บรรทัด) → **Tauri 2 desktop app**: Rust core ทำงานหนักทั้งหมด (Webull TH API, MQTT streaming, Black-Scholes quant, screener engine, LLM copilot) + React/TypeScript frontend ตามโครง **structure freeze** ของ `premium_radar_modern_v2.html`

**Stack:** Tauri 2 · Rust (tokio, rumqttc, prost, reqwest, chrono-tz, keyring) · React 18 + TypeScript + Vite · ไม่ใช้ UI framework หนัก (CSS เองตาม DESIGN.md tokens) · LLM ผ่าน OpenAI-compatible API (GLM-5.3 หลัก / OpenAI สำรอง)

**แผนผังโค้ดเป้าหมาย:**

```
option2/
├── design/                      # (มีแล้ว) visual source of truth — ห้ามแก้โดยไม่ขอ change request
├── PRODUCT.md · PLAN.md
├── src/                         # React frontend
│   ├── components/              # ตาม DESIGN.md §8.1: AppBar, IconRail, ScannerSidebar,
│   │   │                        #   QuoteHeader, MarketChart, OptionsStats, OpportunityCards,
│   │   │                        #   OptionsTable(Scanner|Chain), ContractAnalysis, RadarAssistant, StatusBar
│   ├── state/                   # Tauri event subscriptions + zustand-lite store
│   ├── styles/tokens.css        # V2 computed palette (DESIGN.md §2)
│   └── App.tsx
└── src-tauri/
    ├── src/
    │   ├── webull/
    │   │   ├── mod.rs · sign.rs · rest.rs      # HMAC-SHA1 + throttle 1.1s + cache 3s + retry 429
    │   │   ├── contracts.rs                    # discovery windows + OCC parse
    │   │   ├── stream.rs                       # MQTT (rumqttc) + HTTP subscribe + reconnect
    │   │   └── pb/                             # prost protobuf defs
    │   ├── quant.rs                            # BS price/IV/greeks — port จาก Python
    │   ├── screener.rs                         # enrich/grade/filters — port จาก Python
    │   ├── engine.rs                           # state store + coalesce + event pump 0.8s
    │   ├── copilot.rs                          # LLM client + function calling tools
    │   └── settings.rs                         # first-run keys (OS keyring) + prefs
    └── tests/                                  # golden fixtures + integration
```

**หลักการกลาง:**
1. ตัวเลขทุกตัวมาจาก Rust — frontend ไม่คำนวณการเงินเอง (แค่ format)
2. Keys (Webull/LLM) อยู่ใน Rust process + OS keyring เท่านั้น — ห้าม log / ห้ามลง frontend bundle / ห้ามลง git
3. Data state ตรงไปตรงมา: `สด (WS) / API (REST) / ราคาปิด (BARS) / DEMO` — ห้าม fake LIVE
4. UI ล็อกตาม DESIGN.md §1.2/§4 — เปลี่ยน layout = change request แยก
5. Financial model แก้/ตรวจแยกจาก UI (DESIGN.md §8) — quant ใช้สูตรจาก Python ต้นฉบับ (แม่นกว่า demo ใน prototype) พร้อมแก้ caveats §7.3

**Git workflow:** branch `feat/<phase>-<task>` เช่น `feat/p1-quant-bs` · Conventional Commits `<type>(<scope>): <subject>` · ก่อน merge: `cargo fmt + clippy + test` และ `tsc + eslint + vite build` ต้องเขียว · ห้าม commit `.env*` (ตั้ง `.gitignore` กันไว้)

---

## Phase 0 — Foundation (scaffold + tooling)

| # | Task | ไฟล์ | DOD |
|---|---|---|---|
| 0.1 | Git repo + .gitignore | `.gitignore`, repo init | git init แล้ว commit แรกผ่าน; `.gitignore` ครอบ node_modules/target/dist/.env*/.playwright-mcp; `git log` สะอาด |
| 0.2 | Tauri 2 scaffold (React+TS+Vite) | ทั้งโปรเจค template | `cargo tauri dev` เปิดหน้าต่าง Windows ได้; `cargo tauri build` ผ่าน |
| 0.3 | Rust quality gates | `rustfmt.toml`, clippy lint set | `cargo fmt --check` + `cargo clippy -D warnings` + `cargo test` เขียวบน skeleton |
| 0.4 | Frontend quality gates | `tsconfig` strict, eslint, package.json scripts | `npm run check` (tsc + eslint) + `npm run build` เขียว |

**Phase DOD:** dev loop พร้อมทำงาน — แก้ Rust หรือ TS แล้ว hot-reload ได้ในหน้าต่างเดียว · ทุก gate รันด้วยคำสั่งเดียว (`npm run verify`)

**ความเสี่ยง:** เวอร์ชัน Tauri/Rust บน Windows ต้อง ≥1.90 (updater) — ตรวจตั้งแต่วันแรก · แก้ด้วยการล็อกเวอร์ชันใน `rust-toolchain.toml`

---

## Phase 1 — Quant core (Rust, ไม่แตะ network)

Port จาก Python: `bs_price / implied_vol / bs_greeks / years_to_expiry` · `_enrich_rows_raw / grade_pick / short_candidates / chain_stats / otm_curve`

| # | Task | ไฟล์ | DOD |
|---|---|---|---|
| 1.1 | Contract model + OCC parser | `quant.rs`, `webull/contracts.rs` | parse OCC `^([A-Z0-9.]{1,6})(\d{2})(\d{2})(\d{2})([CP])(\d{8})$` ครบทุก case จาก Python (รวม expiry เป็นวันที่ NY 16:00 ผ่าน chrono-tz); unit test ≥ 15 cases |
| 1.2 | Black-Scholes + IV + Greeks | `quant.rs` | bisection 80 iter bounds 1e-4..5.0 ปฏิเสธค่าต่ำกว่า intrinsic เหมือนต้นฉบับ; golden test เทียบค่าจาก Python **tolerance 1e-6** |
| 1.3 | Enrichment + grading | `screener.rs` | credit/collateral/yld/ann/be/cushion/POP=N(d2@BE)/p_itm/p_touch≈2·p_itm/score=ann×pop + เหตุผลเกรด 10 ข้อภาษาไทย; golden test **grade ตรงเป๊ะ 100%** บน fixture 200+ แถว |
| 1.4 | Filters | `screener.rs` | CSP/CC/ทั้งคู่ · SAFE 0.10–0.20 / BALANCED 0.15–0.30 / AGGR 0.25–0.40 / ALL · DTE 3–60 · spread ≤25% · credit ≥ $0.05 · budget cap (CSP เท่านั้น); golden test เทียบชุด filtered IDs กับ Python |
| 1.5 | Chain stats | `screener.rs` | atm_iv / expected move ±1σ / skew / otm_curve; golden test |
| 1.6 | Golden fixture generator | `tests/fixtures/`, `tools/dump_fixtures.py` | script เล็ก import Python ต้นฉบับ ดึง output จริงเป็น JSON; fixtures committed; CI ไม่ต้องใช้ Python (ค่า freeze แล้ว) |

**Phase DOD:** `cargo test -p app` เขียวทั้งหมด · ตัวเลข quant ตรง Python 100% ภายใน tolerance · ยังไม่มี network code ใดๆ

**ความเสี่ยง:** ความต่าง float ภาษา — แก้ด้วยการเทียบที่ระดับ 1e-6 และ grade เทียบเป็น enum ไม่ใช่ float · Python import อาจต้อง display stub — generator รันที่เครื่องครั้งเดียว ไม่เข้า CI

---

## Phase 2 — Webull REST layer (Rust)

| # | Task | ไฟล์ | DOD |
|---|---|---|---|
| 2.1 | Signing + HTTP client | `webull/sign.rs`, `webull/rest.rs` | HMAC-SHA1 ตาม `_sign` ของ Python (sorted params + host + app-key + nonce + ts + MD5 body + urlencode + base64); unit test สร้าง signature จาก input fix ได้ค่าตรงเป๊ะ |
| 2.2 | Throttle + cache + retry | `webull/rest.rs` | 1 call / 1.1s shared ข้าม task; cache TTL 3s; 429 ถอยหลังรอแล้ว retry; ทดสอบด้วย mock server (wiremock) |
| 2.3 | 4 endpoints | `webull/rest.rs` | stock/option snapshot, option bars, option contracts — parse + to_float ปลอดภัย; wiremock fixtures ทุก endpoint |
| 2.4 | Contract discovery | `webull/contracts.rs` | windows 7 ช่วง (0-7…366-1000) + strike band ±30% + pagination_key + MAX_EXPIRIES 12 + STRIKES_EACH_SIDE 20 + เลือก WS symbols ใกล้ spot (pick_ws_symbols); test กับ fixture หลายหน้า |
| 2.5 | Keys handling | `settings.rs` | อ่านจาก OS keyring (keyring crate) / env `WEBULL_APP_KEY` สำหรับ dev; **grep ทั้ง repo ไม่เจอค่า key จริง; log ตัด mask แล้วเท่านั้น** |

**Phase DOD:** integration smoke กับ API จริง (key จาก env ของเครื่อง — ไม่ลง git): โหลด NVDA contracts + snapshots ครบ, throttle ไม่โดน 429 ใน run ปกติ · ทุก unit/integration test เขียว

**ความเสี่ยง:** host ไทย `api.webull.co.th` อาจมีพฤติกรรมต่าง doc global — smoke กับของจริงตั้งแต่ 2.3 · rate limit จริงเข้มกว่า doc — วัดแล้วปรับ interval

---

## Phase 3 — MQTT streaming (Rust)

| # | Task | ไฟล์ | DOD |
|---|---|---|---|
| 3.1 | MQTT connect (WSS) | `webull/stream.rs` | rumqttc: ClientId=session_id, Username=AppKey, wss :8883/mqtt (host th ยืนยันค่าจริงจาก SDK); เชื่อมได้ รับ `echo` heartbeat |
| 3.2 | HTTP subscribe binding | `webull/stream.rs` | subscribe/unsubscribe ผ่าน HTTP API ด้วย session_id เดียวกับ ClientId (reverse จาก Webull Python SDK Apache-2.0); เปลี่ยน symbol แล้ว unsub เก่า |
| 3.3 | Protobuf decode | `webull/pb/` | .proto จาก doc/SDK → prost; decode snapshot/quote/tick ได้ field ครบ; fuzz malformed ไม่ panic |
| 3.4 | Reconnect + resubscribe | `webull/stream.rs` | หลุด→reconnect→resubscribe อัตโนมัติ; zombie session kill ด้วย generation id เหมือนต้นฉบับ; ทดสอบด้วยการตัด network 5 ครั้ง |
| 3.5 | 3-tier merge | `engine.rs` | WS > REST > BARS + source label + ไม่ทับ quote จริงด้วยราคาปิด (เหมือน `seed` logic); unit test ลำดับชั้นครบทุกสามกรณี |

**Phase DOD:** stream จริง NVDA + ~96 contracts ≥ 5 นาทีต่อเนื่อง: ราคาวิ่ง, ไม่มี error, ตัด net แล้วกลับมาเองภายใน 30 วินาที · `LIVE` badge ขึ้นเฉพาะเมื่อ WS และข้อมูลถึงจริง

**ความเสี่ยง (ใหญ่สุดของโปรเจค):** protocol ไทยอาจต่าง — ถ้าสะดุดเกิน 1 วันทำงาน มี **fallback**: โหมด REST polling 30 วิ (ยังคง 3-tier labels) แล้วกลับมาเติม MQTT ทีหลัง

---

## Phase 4 — State engine + Tauri bridge

| # | Task | ไฟล์ | DOD |
|---|---|---|---|
| 4.1 | Store + coalescing | `engine.rs` | latest-per-consect แบบ `_LatestSnapshots`; stale generation ทิ้ง; enrich memoize (3s bucket / spot เคลื่อน 0.3%) เหมือนต้นฉบับ |
| 4.2 | Event pump | `engine.rs`, `src/state/` | emit `screener-update` throttle 800ms; frontend รับแล้ว render ไม่กระตุก (วัดด้วย frame ไม่หาย) |
| 4.3 | Commands | `main.rs` | `load_symbol, set_filters, set_budget, select_contract, start/stop_stream, chat_send`; ทุก command ตอบ Result มีข้อความไทย |
| 4.4 | Settings persistence | `settings.rs` | prefs ลง `radar_settings.json` ใน app data dir (theme/font/budget/last symbol); ปิด-เปิดกลับมาเหมือนเดิม |

**Phase DOD:** สลับ symbol (NVDA→TSLA) แล้วข้อมูลใหม่ถูกต้อง ไม่ปนของเก่า · filter จน 0 ผล → empty state ชัด + analysis panel ไม่แสดงของเก่าหลอก (แก้ caveat §7.3 #8)

---

## Phase 5 — Frontend shell (React + TS)

ตาม DESIGN.md §8.1 component mapping + §4 measurements + §9 checklist

| # | Task | ไฟล์ (src/) | DOD |
|---|---|---|---|
| 5.1 | Design tokens CSS | `styles/tokens.css` | V2 computed palette จาก §2 ครบทุก token; focus-visible 2px #4b8cf7; scrollbar/selection ตาม source |
| 5.2 | AppShell 4 คอลัมน์ | `components/shell/` | 43/50/225/1fr/320/25px + breakpoints 1420/1150/790 ซ่อน/แสดงตาม §4.5; **ไม่มี horizontal scroll** ที่ทุก breakpoint |
| 5.3 | Left panel | `components/ScannerSidebar/` | watchlist+sparkline+price/chg, strategy 3-toggle, risk 4 choice, expiry grid, cashfield, data-mode foot — ผูก command จริงครบ |
| 5.4 | Main: quote + topgrid | `components/QuoteHeader/`, `MarketChart/`, `OptionsStats/` | quote 30px tabular; แท่งเทียน (klinecharts หรือ SVG เอง — เลือกตอน 5.4 ตามขนาด bundle); IV/EM/skew/OI split จากข้อมูลจริง |
| 5.5 | Opportunity cards + Scanner table | `components/OpportunityCards/`, `OptionsTable/` | 3 การ์ด (Defensive/Balanced/High Yield ตามต้นฉบับ Python); ตาราง 13 คอลัมน์ sticky header, row 33px, sort, hover/selected ตาม §5.2 |
| 5.6 | Right panel | `components/ContractAnalysis/`, `RadarAssistant/` | stats 4 ช่อง, probability bar 3 ส่วน, payoff SVG (render จาก pick_pnl จริง), คำอธิบายไทยจาก grade_pick, risk note; chat UI (rule-based stub ยังไม่ต่อ LLM) |
| 5.7 | Status bar + states | `components/StatusBar/` | OFFLINE/CONNECTING/LIVE + row count + feed status; loading/empty/error ทุก panel ตาม §8.2 |
| 5.8 | Option Chain mode | `OptionsTable/Chain` | Calls\|Strike\|Puts mirror + zebra + selected pair ตาม §5.2 known behavior (คลิกเลือก pair) |

**Phase DOD:** **Checklist §9 A (visual parity) + B (interaction parity)** ผ่านทั้งหมด ที่ 4 breakpoints (>1420 / 1366 / 1024 / ≤790) — ยืนยันด้วย screenshot capture เทียบต้นแบบ · ข้อมูลจริงจาก Webull ไหลทั้งหน้า · ตัวเลขทุกตัว format จาก Rust payloads

**ความเสี่ยง:** หลงไปทำ "สวยกว่าต้นแบบ" — กติกา: parity ชนะความคิดเห็น · แท่งเทียนถ้า lib หนักเกิน เขียน SVG เอง (มี pattern ใน prototype อยู่แล้ว)

---

## Phase 6 — Copilot (GLM-5.3)

| # | Task | ไฟล์ | DOD |
|---|---|---|---|
| 6.1 | LLM client | `copilot.rs` | OpenAI-compatible: base_url สลับ GLM-5.3 (docs.z.ai) ↔ OpenAI; streaming SSE → event `copilot-delta`; timeout + error ไทย; key จาก keyring/env |
| 6.2 | Function calling tools | `copilot.rs` | `top_candidates(filters)`, `contract_detail(id)`, `chain_stats()` อ่านจาก engine จริง; ผลลัพธ์ cite เป็น chip คลิกเลือกสัญญาได้; system prompt ไทย + กติกา "ตอบจากตัวเลขที่มีเท่านั้น ไม่แนะนำซื้อขาย" |
| 6.3 | Chat wiring | `RadarAssistant/` | streaming bubbles, quick-ask 3 ปุ่ม (ต่อยอดจาก prototype), คำตอบอ้างสัญญาจริง; ถ้า LLM ว่าง → แจ้งชัดไม่เงียบ |

**Phase DOD:** ถาม "มีเงิน 18,000 ตัวไหนน่าทำ" → คำตอบอ้าง contract เดียวกับที่อยู่ในตาราง ตัวเลขตรงเป๊ะ (spot-check 3 คำถาม) · **keys ไม่ปรากฏใน frontend bundle / log / git — ตรวจด้วย grep หลัง build release**

---

## Phase 7 — Packaging + hardening + handoff

| # | Task | DOD |
|---|---|---|
| 7.1 | First-run setup screen | ฟอร์มใส่ Webull key/secret + เลือก provider (GLM-5.3/OpenAI) + key → เก็บ keyring; ทดสอบบันทึก-ลบ-แก้; ของเก่าอัปเกรดไม่พัง |
| 7.2 | Windows installer | `tauri build` → NSIS/MSI ติดตั้งเครื่องสะอาด (VM/เครื่องจริง) เปิดใช้ได้ครบทุกฟีเจอร์ |
| 7.3 | macOS build | .dmg บิลด์ผ่าน + THP ทดสอบบนเครื่อง Mac จริง (ส่ง artifact ให้ทดสอบ) |
| 7.4 | Security pass | สแกน: ไม่มี secret ใน bundle/log; sanitize ทุก dynamic string ก่อน render (ไม่ใช้ innerHTML กับ payload); CSP เข้ม; error ไม่กลืนเงียบ (log + propagate) |
| 7.5 | Financial integrity pass | ปิด caveat §7.3 ครบ 8 ข้อ: DTE จริง (ไม่ hardcode), CC payoff แสดง option-leg + combined, denominator ป้ายชัด, `assign/win` จากโมเดลมี methodology, empty-filter consistency, gross-vs-net note, event risk (งบ) flag พื้นฐาน |
| 7.6 | Docs + final report | README ติดตั้ง/ใช้งาน (TH) + รายงานสรุปต่อ Task ตาม DOD จริง (ผ่าน/ตก/แก้ยังไง) ตามขั้นตอนที่ 5 ของ workflow |

**Phase DOD:** เครื่องสะอาด Windows + macOS: ติดตั้ง → ใส่ key → สแกน NVDA → คุยกับ copilot ได้ ภายใน 5 นาทีโดยไม่ต้องอ่านโค้ด · checklist §9 A–D เขียวทั้งหมด

---

## สิ่งที่ตัดออกจาก v1 (non-goals — ยืนยันจาก PRODUCT.md)

Order placement · backtest · portfolio tracking · multi-ticker พร้อมกัน · mobile UI (จอ ≤790 ซ่อน panels ตามต้นแบบ ไม่ใช่ mobile app) · Analytics 3×2 grid page (ไม่อยู่ใน structure freeze — ถ้าอยากได้ เปิด change request หลัง v1) · auto-update

## Risk register

| ความเสี่ยง | โอกาส | แผนรับมือ |
|---|---|---|
| MQTT host/protocol ไทยต่างจาก doc | กลาง | reverse จาก SDK source; **fallback REST polling** ทำงานได้ก่อน แล้วเติมเมืองหลัง |
| Rate limit จริงเข้มกว่า 1.1s | กลาง | วัดจริง Phase 2 แล้วปรับ interval + เพิ่ม cache |
| Rust float ต่าง Python | ต่ำ | golden fixtures tolerance 1e-6, grade เทียบ enum |
| GLM tool-calling พฤติกรรมต่าง doc | ต่ำ | provider-agnostic + OpenAI สำรองสลับได้ใน settings |
| ขอบเขตบวม (scope creep UI) | กลาง | structure freeze + ทุกการเปลี่ยน layout เป็น change request |
| macOS build ไม่มีเครื่องทดสอบ | กลาง | ส่ง artifact ให้ THP ทดสอบเป็น gate สุดท้ายของ 7.3 |

## ลำดับ + ประมาณการ

P0→P1→P2→P3→P4→P5→P6→P7 (P1 ทำขนานกับ P2 ได้ — quant ไม่พึ่ง network) · ประมาณการหยาบต่อ phase ทำงานจริง: P0 0.5 วัน · P1 2 วัน · P2 1.5 วัน · P3 2–4 วัน (มี fallback ตัดเหลือ 1) · P4 1 วัน · P5 3 วัน · P6 1.5 วัน · P7 2 วัน — รวม ~13–16 วันทำงาน

---

**ขอ approve แผนนี้เพื่อเริ่ม Phase 0–1** (หรือแจ้งปรับก่อน — หลัง approve จะลงมือตาม Task ทีละตัว ตรวจ DOD ทุกตัว และรายงานตามขั้นตอนที่ 4–5 ของ workflow)
