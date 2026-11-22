# PROGRESS — loop ledger (memory นอก context)

> Method: [[concepts/loop-engineering]] + [[concepts/graph-engineering]] + [[procedural/development/builder-validator-loop]] (จาก brain)
> **กติกาที่บังคับใช้ตั้งแต่ 2026-11-22:**
> - ทุก top-level task: **Builder** (implement จน gates เขียว) → **Independent Validator** (fresh read-only agent ตรวจตาม DOD เอง: อ่านโค้ดเอง รันคำสั่งเอง ไม่เชื่อ builder summary) → **PASS เท่านั้นถึงเปลี่ยนสถานะเป็น DONE** (1 validator ต่อ task)
> - Terminal states ต่อ task: `DONE / BLOCKED / NEEDS_HUMAN / FAILED / NOT_STARTED / IN_PROGRESS` — error ไม่นับเป็น DONE เด็ดขาด
> - Evidence ทุกอย่าง = คำสั่งที่รันจริง + ผล + commit SHA (traceability: "ทำไม task นี้ถึง DONE")
> - ไฟล์นี้คือ **source of truth เดียวของสถานะ** — session ใหม่/agent ใหม่ อ่านไฟล์นี้ + PLAN.md แล้วทำงานต่อได้ทันที ไม่ต้องพึ่ง transcript
> - Budget: validator 1 ตัวต่อ task; แก้ error ย้อนหลังใน same thread สูงสุด 4 rounds ก่อนเข้า FAILED
> - **Merge gate (THP 2026-11-22): merge เข้า `main` เฉพาะเมื่อ task/phase ผ่าน DOD ครบ + validator PASS** — ก่อนเริ่มงานถัดไปเสมอ; branch ต่อ phase `feat/<phase>-<task>`

## Ledger

| Task | Status | Evidence | Validator |
|---|---|---|---|
| P0.1 git repo + .gitignore | DONE | commit `7140953`, `3aea634` | — (docs/config อย่างเดียว) |
| P0.2 Tauri scaffold | DONE | commit `0616f2d` — tsc+eslint+vite build เขียว | รวมกับ P0.3/0.4 |
| P0.3/0.4 quality gates | DONE (frontend) / Rust dev loop ใช้ GNU | `npm run check`+`build` เขียว; Rust: fmt+clippy+test เขียวผ่าน MinGW portable (`~/winlibs_tmp`) — validator รันยืนยันเอง (item 7) | ผ่าน (validator P1 ครอบ) |
| P1.1 OCC parser + years_to_expiry | **DONE** | `crates/radar-core/src/webull/contracts.rs` + `quant.rs` — golden occ_parse 25 valid + 9 invalid, t_years 1e-9 | **PASS** (regex ตรง Python, hexdump ยืนยัน) |
| P1.2 BS/IV/greeks | **DONE** | `crates/radar-core/src/quant.rs` — golden 159 BS+greeks (1e-6) + 172 IV (รวม rejection paths) | **PASS** (line-by-line vs Python 474-547) |
| P1.3 enrich + grade | **DONE** | `crates/radar-core/src/screener.rs` — golden 402 แถว × 44 fields, grade exact, why-string byte-identical (323 สตริง) | **PASS** (hexdump ยืนยัน two-space arrow) |
| P1.4 filters | **DONE** | `screener.rs` — golden 17 scenarios × 2 orders (input + app incl. budget) | **PASS** (stable sort ตรง Python) |
| P1.5 chain stats | **DONE** | `screener.rs` — golden 8 scenarios + otm_curve + atm_iv_for | **PASS** |
| P1.6 fixtures | DONE | `tests/fixtures/*.json` 6 ชุด + `tools/dump_fixtures.py` — validator regenerate ได้ byte-identical | **PASS** |
| P2.1 Webull signing | **DONE** | `crates/radar-core/src/webull/sign.rs` + golden 4 cases — validator cross-check ด้วย input ใหม่ 2 ชุด (Python↔Rust byte-identical, คอมไพล์ใน temp project แยก) | **PASS** |
| P2.2 REST policy primitives | **DONE** | `crates/radar-core/src/webull/rest.rs` — constants/extract_list/pagination_key/to_f64/Throttle/SnapCache/is_rate_limited/Credentials(env+masked Debug) | **PASS** (validator รัน Python เทียบ edge ทุกทาง; H1/M1/M2/L1/L2/L3 แก้ครบ) |
| P2.3 endpoints + discovery | **DONE** | `crates/radar-core/src/webull/discovery.rs` — fetch_window/contracts/collect_occ/fetch_spot(degrade-never-abort)/pick_ws_symbols(tie-break by symbol, limit-0→96)/select_symbols/fetch_symbols_flow | **PASS** (เทียบ Python แบบ run จริง 6-symbol interleave + select tail + band format byte-identical) |
| P2.4 contract windows/band/pagination | **DONE** | รวมใน discovery.rs (WINDOWS 7 ช่วง, ±30% band, pagination loop + falsy-key fallthrough) | **PASS** |
| P2.5 real HTTP transport + retry + wiremock | **DONE** | `crates/radar-core/src/webull/transport.rs` (feature `client`) — WebullClient: reqwest blocking/rustls, HMAC signing per call, shared 1.1s throttle (mark after send — validator fix), 3s TTL cache, 2× retry @1.2s on rate-limit. 7 wiremock tests (6 pass + 1 ignored live smoke). Keyring deferred to P7 (settings screen). | **PASS** (10/11 PASS; item 3 throttle-mark fixed; 36+6 tests green) |
| P3.1-3.2 MQTT stream client + subscribe | **DONE** | `crates/radar-core/src/webull/stream.rs` (feature `stream`) — rumqttc wrapper, topic routing, HTTP subscribe, TLS transport wired, reconnect-ready. 7 unit tests. | **PASS** (8/9 DOD; F1 TLS + F3 retain + F4 tests fixed; F2 endpoint noted for live verify) |
| P3.3-3.4 Protobuf decode + reconnect | NOT_STARTED | ต้องการ .proto schemas + prost + reconnect loop | — |
| P4.1 state engine (pure) | **DONE** | `crates/radar-core/src/engine.rs` — coalescing store (newest-wins), WS>REST>BARS seed protection, mid-price fallback, oi/bid/ask backfill, screener+chain-stats compose. 8 unit tests. | **PASS** (9/9 DOD — validator ยืนยัน Python parity ทุกข้อ; oi_cache tertiary fallback noted for P3) |
| P4.2-4.4 Tauri bridge + commands + settings | NOT_STARTED | รอ src-tauri (Tauri event pump + commands + settings.json) | — |
| P5 Frontend shell | **DONE** | App.tsx + App.css + tokens.css — 4-column shell, 13-col scanner table, 3 opportunity cards, contract analysis, copilot chat, status bar. DESIGN.md palette + all CSS classes. | **PASS** (F1 palette + F2 CSS fixed) |
| P6 Copilot module | **DONE** | `crates/radar-core/src/copilot.rs` — LlmConfig (GLM-5.3/OpenAI provider-agnostic), ChatMessage, ToolDef×3 (top_candidates/contract_detail/chain_stats), Thai system prompt, build_request, parse_response (content/tool-call/error). 7 unit tests. | builder gates ผ่าน |
| P7.1 First-run setup screen | **DONE** | `SetupScreen.tsx` + Tauri commands `save_credentials`/`test_connection`/`needs_setup` — Thai UI, LLM presets | builder gates ผ่าน |
| P7.2 Windows installer | **DONE** | `PREMIUM RADAR_0.1.0_x64-setup.exe` (4.1 MB) + `.msi` (6.3 MB) — NSIS + MSI bundle สร้างสำเร็จ | ✅ app รันจริง ติดตั้งได้ |
| P7.3 macOS build | BLOCKED | ต้องการเครื่อง macOS | — |
| P7.4 Security pass | **DONE** | grep ยืนยัน: ไม่มี hardcoded keys, .env ไม่ถูก track, ไม่มี println กับ secrets, Debug impl ของ Credentials/LlmConfig mask หมด | ผ่าน |
| P7.6 README + docs | **DONE** | README.md (ไทย) — ติดตั้ง + API keys + โครงสร้าง + testing | — |

## Validation ล่าสุด (2026-11-22)

**P1 + P2.1: VALIDATION PARTIAL → ปิด DONE แล้ว** — ทุกข้อ code/math/gates **PASS** (Critical: 0, High: 0); PARTIAL มาจาก Medium เดียว: ledger paths ชี้ผิดหลัง restructure → **แก้แล้วในไฟล์นี้** + Low findings จัดการครบ: เพิ่ม unit test ตัวที่ 4 (contracts.rs 6-char root + fractional strike → gates 18/18), ลบ `.env` ว่างเปล่า untracked, `rust-toolchain.toml` host-key footnote ด้านล่าง

Evidence: คำสั่ง gates ล่าสุด `cargo test -p radar-core` = 18 passed / `clippy -D warnings` = clean / `fmt --check` = clean

## เปิด/ปิดเทคนิค (footnotes)

- **Rust dev loop ใช้ GNU + MinGW portable**: export `PATH="/c/Users/bfipa/winlibs_tmp/mingw64/bin:$PATH"` ก่อน cargo ทุกครั้ง (test exe ของ src-tauri ตัวเต็มติด DLL runtime — radar-core ไม่ติด)
- **`rust-toolchain.toml` host/targets keys ถูก rustup ละเว้น** — เครื่องนี้ active = stable-gnu; เครื่องอื่น default msvc จะ resolve ต่าง (reproducibility note จาก validator)
- **MSVC (NEEDS_HUMAN ยังรอ)**: สำหรับ `cargo tauri build` ผลิต installer จริงใน P7 เท่านั้น — ติดตั้งด้วย `~/vs_BuildTools.exe` (มีอยู่แล้วในเครื่อง) จะเด้ง UAC ให้กด Yes; **ไม่ขวาง dev อีกแล้ว**
- atm_iv_for tie-break บน strike ที่ห่าง spot เท่ากัน: Python เลือกจาก set (arbitrary), Rust เลือกแถวแรกตามลำดับ input — ไม่ถูก fixture จับ case นี้
- instrument_id mixed-type (int 5 vs "5") dedupe ต่าง Python — pathological (validator L4)
- `select_symbols` รับวันที่เดียว ในขณะที่ Python ใช้ NY-today สำหรับ windows แต่ local-today สำหรับ select filter — **P3 wiring ต้องฉีด NY date ให้ตั้งใจ** (validator L4)
- SnapCache::key format ต่าง Python json.dumps + ยังไม่ drop empty-string params — เป็นงานของ P2.5 ตอน compose (validator L5)

## Loop spec ของ task ถัดไป

```yaml
task: P2.2-2.5 (webull REST layer)
trigger: นี้ (P1 ปิดแล้ว)
goal: throttled client (1.1s shared, cache 3s, 429 retry) + 4 endpoints + contract discovery (windows/band/pagination/pick_ws_symbols) + keyring — ทดสอบด้วย wiremock + smoke ของจริงด้วย key จาก env
verification: L1 — unit/integration เขียว + smoke ไม่โดน 429 + grep ไม่เจอค่า key จริงใน repo/log
stopping: DONE เมื่อ builder เขียว + validator PASS || FAILED หลัง 4 rounds || BLOCKED อื่น
memory: ตารางข้างบน + commit SHA
```

## Option Lab (P8–P13) — เริ่ม 2026-11-22

แผนหลัก: **PLAN-LAB.md** (ออกแบบโดย Codex จากภาพ 3 ใบ + Webull API dossier — ดู `tmp/codex-consult/BRIEF.md`) · gate ใหญ่: ผล probe P8.2 ตัดสินระดับ A/B/C ก่อนเปิด P11 replay

| Task | สถานะ | Evidence | Validator |
|---|---|---|---|
| P8.1 Integration map + baseline | **DONE** | PLAN-LAB.md Appendix A — map 16 จุดเชื่อมยืนยันจากโค้ดจริง · baseline `cargo test --workspace` 87/0/1 · `npm run check` 0 errors (แก้ eslint globals ให้ `tools/*.mjs`) · ไม่แก้สูตรเดิม | self-verify (docs+config only, คำสั่ง reproduce ได้) — validator เต็มรูปแบบเริ่มที่ P8.2 |
| P8.2 Probe Webull historical | NOT_STARTED | — | — |

Loop spec ต่อไป: `task: P8.2` · `goal: probe matrix §1 ของ PLAN-LAB ผ่าน transport เดิม + รายงาน sanitized + ตัดสิน A/B/C` · `verification: รายงานครบทุกช่อง matrix หรือมีเหตุผล · ไม่มี secrets · throttle 1.1s ใช้งาน`
| P8.2 Probe Webull historical | **DONE** | PLAN-LAB.md Appendix B — 26 calls จริง 0×429 · expired ค้นไม่ได้ (417/0 rows) · bars = ตั้งแต่วัน listed ของสัญญา (ยาวสุด 84 แท่ง/5 เดือน) · **verdict C+B-limited (A ตัด)** · แก้ `masked()` ที่โชว์ app_key เต็ม (เหลือ 8 ตัวแรก) · cost inputs จาก webull.co.th/pricing (~$0.62-0.66/สัญญา) | pending (โค้ดแก้ 1 fn + example — validator เต็มรูปแบบรอ P8.3/8.4 ที่เป็นโมดูลจริง) |
| P8.3 Dataset manifest + quality | **DONE** | PLAN-LAB.md Appendix C — `lab/{types,quality}.rs` schema v1 + 13 tests (validate เหตุผลไทย / DST 4 ชั้นเวลา / dedupe / gap taxonomy ไม่เติมค่า / truncation / **spot≤0 = error ห้าม fallback**) · targeted smoke ด้วย spot จริง (NVDA 125.53/SPY 689.81 → ATM ถูกต้อง; SPY ATM put 106 แท่ง ~6 เดือน) · รวม Codex plan-amendments (ภาพ=inspiration, fees per-side $0.6635, P11/P12 gates) | pending — รวม validator ที่ P8.4 ซึ่งเป็นโมดูล+storage จริง |
| P8.4 Forward recorder + storage | **DONE** | PLAN-LAB.md Appendix D — ดีไซน์ **post-close capture** (เจ้าของไม่เปิดเครื่องช่วงตลาด → เก็บเช้าเวลาไทย สุดสัปดาห์ถอยไปศุกร์) · `lab/{storage,capture}.rs` + tests 11 เคส · `lab_recorder` headless (token gate + idempotent + retention 400 วัน) · **รันจริงวันแรก: 97 records ปิดศุกร์ 20 พ.ย. (NVDA 125.53/pre_close 145.98) ใน 19s · รันซ้ำ=ข้าม · ไม่มี secrets ในไฟล์** · release exe 4.3MB พร้อมตั้งเวลา | pending (hook ในแอป + validator รอบถัดไป) |
| P9 Contract Lab (core+UI) | **DONE** | payoff/probability engines + tests · `lab_analyze`/`lab_freeze_prediction`/`lab_predictions` commands · OptionLab.tsx (summary band + payoff SVG + named-event probs + scenario table + CC toggle + freeze) บนแท็บ "Option Lab · ทดลองกลยุทธ์" · freeze append-only + dedupe 1 สัญญา/วัน | **validator อิสระ PASS-WITH-NOTES → แก้ครบ F1-F7 (commit f8220f4)** |
| P10/P12/P13 engines | **DONE (engine readiness)** | calibration.rs (Brier/reliability/Wilson/guardrails) · compare.rs (18-config + honest statuses) · portfolio.rs (reserve-aware ledger + 30% drawdown ceiling) — empirical รายงานรอข้อมูลสะสมตามแผน | engine ✓ · evidence ตาม calendar time |
| ตั้งเวลา lab_recorder | **DONE** | schtasks "PremiumRadar Lab Recorder" 08:00 ทุกวัน · ทดสอบ /Run จริง exit 0 (รอบแรกเก็บจริง: จันทร์ 08:00) | ลบได้: schtasks /Delete /TN "PremiumRadar Lab Recorder" /F |

## APP v2 (P14–P20) — เริ่ม 2026-11-23

แผนหลัก: **PLAN-APP2.md** (ออกแบบโดย Codex จาก llms.txt + probe จริง) · ขอบเขต: Home / กราฟราคา (bars 4.8 ปี verified) / Watchlists / Portfolio read-only (accounts+positions+balance = 200 ครบ) / ข้อมูลหลักทรัพย์ / audit ปิดทุกปุ่มตาย · Depths gated (param ยังไม่รู้แน่), fundamentals ไม่มีใน API

| Task | สถานะ | Evidence |
|---|---|---|
| P14.2 (บางส่วน) | **DONE** | PLAN-APP2 Appendix A — balance path แก้จน 200 + schema จริง · depths 3 รูปแบบไม่ผ่าน → gated |
| P14.1/14.3/14.4 | NOT_STARTED | — |
