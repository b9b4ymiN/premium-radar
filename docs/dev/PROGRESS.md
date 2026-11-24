# PROGRESS — loop ledger (memory นอก context)

## Webull API expansion & professional UI · 2026-11-23–24

Branch **`feat/webull-api-expansion`** จาก `f2fee8b` (branch เก่า `feat/premium-radar-webull-terminal` เก็บไว้ตามเดิม) · ยังไม่ merge · ไม่ push · ตาม brief "PREMIUM RADAR — Webull API Expansion & Professional UI/UX"

| Phase | สถานะ | Evidence |
|---|---|---|
| 0 Baseline audit | DONE | gate ตอนเริ่ม: `npm run check` พัง (eslint lint โฟลเดอร์ `target/`) + clippy เข้มงวดไม่ผ่าน (lab/, mqtt_probe) → `670f8e8` |
| 1 API research + live probes | DONE | เอกสาร 92 หน้า → 61 operations · **probe จริง 84 call บน `api.webull.co.th` (0×429)** ด้วย `src-tauri/examples/probe_capabilities.rs` (`a355265`, `17be1a3`) · ผลอ่านที่ `docs/WEBULL_TH_CAPABILITY_MATRIX.md` (`4f1a85b`) · รายงาน sanitize อยู่ใน `tmp/` (git-ignored) |
| 2 Reliability & security | DONE | V3 signing (HMAC-SHA256) เป็นค่าเริ่มต้น + ไม่ส่ง `x-app-secret` + coordinator ร่วมทุก request (`52fef8c`) · เลือก scheme ตาม settings + ตรวจอัตโนมัติใน Setup (`b612c6d`) · error classification ตาม error_code ของโบรกเกอร์ (`33b84ee`) |
| 3 Integrations (backend) | DONE | typed clients: stocks/options/fundamentals/screeners/watchlists/trading (`33b84ee`, `590f331`) · คำสั่ง Tauri เพิ่ม 9 ตัว (รวม 31 ตัว) · 17 คำสั่งเป็น async ที่ทำงานหลังฉาก + cache ตามอายุข้อมูล (`0e46e8b`) |
| 3 Integrations (frontend) | DONE | `88031bb` · `0de8120` · `4007755` · `41d5d1a` — รายละเอียดที่ `design/REFINEMENT-V2-1.md` (V2.2) |
| 4 UI/UX | DONE | ก่อน/หลัง ที่ 1920 / 1680 / 1440 / 1290 (browser preview 44 รูป + รูปจากแอปจริง) ใน `tmp/shots/` · ตัวอักษรตาราง 10.8 → 11.4 px · shell/navigation ไม่เปลี่ยน |
| 5 Validation | DONE | independent audit (Codex, read-only, `.cx/audit_20261124190043_b008.md`) → FAIL 11 findings → แก้ด้วย ZCode workers W1–W7 + commander review ทุก diff → re-audit โดย commander → PASS (ตาราง "Phase 5 findings" ล่าง) |

### Gate ล่าสุด (รันจริงที่ `2de9447`, `npm run verify` = 5 คำสั่งนี้)

| คำสั่ง | ผล |
|---|---|
| `npm run check` | ผ่าน — 0 errors · 0 warnings |
| `npm run build` | ผ่าน |
| `cargo fmt --all --check` | ผ่าน |
| `cargo clippy --workspace --all-targets -- -D warnings` | ผ่าน |
| `cargo test --workspace` | **234 passed · 0 failed · 1 ignored** |

### Phase 5 findings (audit 2026-11-24)

| ID | ระดับ | เรื่อง | ผล |
|---|---|---|---|
| F1 | High | secret/token/LLM key ข้าม Tauri IPC (`get_settings` คืนทั้ง struct) — มีมาก่อน branch นี้ | แก้ `af27033` — `SettingsView` (has_* flags), ช่องว่าง = ใช้ค่าเดิม, token ที่ออกใหม่อยู่ใน native |
| F2 | Med | base URL อิสระ ส่ง key ไป host ใดก็ได้ | แก้ `af27033` — รับเฉพาะ `https://api.webull.co.th` (เจ้าของเลือก prod อย่างเดียว) |
| F3 | Med | cache ไม่ผูก credential — ทดสอบ key ใหม่อาจผ่านจาก cache | แก้ `af27033` — test ไม่ใช้ cache, save ล้าง cache โดยไม่เปลี่ยน coordinator |
| F4 | High | หัวราคาโชว์ราคาหุ้นตัวก่อนหลังเปลี่ยน symbol (engine ไม่เคย reset) | แก้ `ea201ec` — `begin_symbol` + symbol tag ที่ UI เช็ค |
| F5 | Low | Debug/probe แสดง app key prefix 8 ตัว + ความยาว secret | แก้ `7cc231d` — prefix 4 ตัว, secret=set/empty, probe ปิดเลขยาว |
| F6 | Med | open orders/watchlist envelope ผิดรูป → "ไม่มีรายการ" | แก้ `4dcc016` — SCHEMA_MISMATCH |
| F7 | Med | 403+INVALID_SIGNATURE = Permission; "generate" ทำให้ retry | แก้ `4dcc016` — code ก่อน status, classifier เดียว |
| F8 | Med | quota ข้าม process (แอป + lab_recorder) | **เลื่อน** — ข้อจำกัดที่รู้แล้ว |
| F9 | Med | copilot ถือ engine lock ระหว่างเรียก LLM; lock poison → panic | แก้ `89a9807` |
| F10 | Med | change ที่ไม่มีโชว์ +0.00 | แก้ `ea201ec` — derive จาก pre_close หรือ `--` |
| F11 | Low | wiremock POST ไม่เช็ค body | แก้ `4dcc016` |
| N1 | Med | redirect พา `x-access-token` ไป host อื่นได้ (เจอตอน re-audit) | แก้ `2de9447` — `Policy::none()` ทั้ง broker และ LLM client |
| — | Low | สี hard-code ใน Setup | แก้ `587d02e` — design tokens |

เหลือ (Low, บันทึกไว้): race สั้นระหว่าง `stale()` กับ push option quote ของ scan เก่า · `#2b6cb8` ปุ่มบันทึกไม่มี token ใกล้ · ล้างค่า LLM ผ่าน Setup ไม่ได้ (ช่องว่าง = คงค่าเดิม)

### สิ่งที่การรันแอปจริงเจอ (unit test ไม่เจอ)

| ที่พบ | แก้ที่ |
|---|---|
| **แอป crash ตอนเปิด** — สร้าง `reqwest::blocking` client บน async thread ของ command (debug build panics) แล้ว lock settings ติด poison ทำให้คำสั่งถัดไปตายหมด | `9bb03da` — สร้าง/ใช้/ทิ้ง client ใน blocking closure อย่างเดียว + อ่าน settings ผ่าน poison ได้ + regression test |
| keyring test ล้มเอง 4/20 ครั้งเมื่อรันขนาน (Windows Credential Manager) | `9bb03da` — test ที่แตะ keyring รันทีละตัว → 0/25 |
| broker ใส่ session หลังปิดตลาดเป็น `ATH` (ไม่มีในเอกสาร) และ `OVN` ช่วง overnight → หัวราคาโชว์รหัสดิบและไม่โชว์ราคานอกเวลา | `4007755`, `41d5d1a` |
| ตารางสแกนว่างเพราะ budget ต่ำกว่า collateral แต่ข้อความบอกแค่ "เชื่อมต่อสดแล้ว" | `4007755` — บอกตัวเลขจริงทั้งสองค่า |
| `get_status` บอก `scanning: false` ขณะ scan ตัวใหม่ยังทำงาน | `f3bba0c` |
| รายการหุ้นใน watchlist ของโบรกเกอร์อยู่ใน key `instruments` ไม่ใช่ `data` → parser แรกอ่านได้ 0 แถวจากที่มี 5 | `590f331` — reader เข้มงวด + `FetchError::Unexpected` (SCHEMA_MISMATCH) |

### หลักฐานจากแอปจริง (2026-11-24, US after-hours + overnight)

- เรียกคำสั่งแบบอ่านอย่างเดียวจากในแอปจริง 25 ครั้ง ผ่านทุกครั้ง · `get_stock_overview` (5 call ต่อเนื่อง) ใช้ 7.3 วิ ระหว่างนั้น UI วาดได้ 437 เฟรม (~60 fps) → UI thread ไม่ถูกบล็อก
- Monitor: 93 candidates, เรียงคอลัมน์ขึ้น/ลงได้, Webull quote + recent trades 12 รายการ, company sheet · Market: 200 แถว × 4 ลิสต์ + sectors 20 · Home movers 15 แถว · Portfolio (label/เลขบัญชี/ยอดเงินถูกปิดในภาพ) · Watchlist ของโบรกเกอร์ 7 ลิสต์ (ลิสต์ที่มีหุ้น = 5, หุ้น HKG แสดงว่าอยู่นอกขอบเขต)
- log ของแอปหลังแก้ `9bb03da`: 0 panics ตลอดการรัน

### ขอบเขตที่ตรวจไม่ได้ / เหลือไว้

- schema รายการของ positions / open orders / historical orders — บัญชีไม่มีรายการตอนตรวจ (HTTP 200, 0 แถว)
- พฤติกรรมหน้าจอใหม่ช่วง Pre-market / ตลาดปกติในแอป (probe ผ่าน HTTP เท่านั้น) · tag `CLOSED` และ tag ของตลาดปกติ ยังไม่เคยเห็น
- host ที่รับได้แค่ V2 (ทดสอบด้วย fixture + wiremock) · UAT
- ปุ่ม "คัดลอก" หุ้นจาก watchlist ของโบรกเกอร์เข้า list ในเครื่อง — ไม่ได้กด เพราะจะแก้ข้อมูลของเจ้าของเครื่อง (ใช้คำสั่งเดียวกับช่องเพิ่มหุ้นเดิม)
- Base URL รับเฉพาะ production — การทดสอบกับ UAT ต้องแก้ allowlist ใน `src-tauri/src/settings.rs` ชั่วคราว

## Webull Thailand and Terminal refinement · 2026-11-23

> ส่วนนี้เป็นบันทึกของ branch `feat/premium-radar-webull-terminal` (งานก่อนหน้า) — งานที่บอกว่า "remain open" ด้านล่าง ปิดแล้วใน section ข้างบน

- Feature branch `feat/premium-radar-webull-terminal` from `cde32a8`. CSS review `633b35e` and keyboard/preview fixes `36aeab4`.
- Security `da792fd` removes the raw secret header, retains v2 signing, and restores prior keyring entries on a failed settings save where the OS permits rollback. `cargo test --workspace` passed 160 tests with 1 ignored. `cargo fmt --all --check` and `git diff --check` passed.
- `npm run check` passed with 4 existing React Hooks warnings. `npm run build` passed. Strict workspace Clippy reported existing lint errors in `mqtt_probe.rs`, `lab/quality.rs` and `lab/storage.rs`.
- Documented GET inventory `3c4bf79` is the first portion of the capability matrix. Guide-tier routes, live Thai entitlement checks, request coordination, and native desktop validation remain open.

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
| P19.2 ปุ่มตายทั้งแอป = 0 | **DONE** | icon rail 4 ปุ่ม wire จริง (Market disabled พร้อมเหตุผล) · navtab Options wire · การ์ด opportunity เลือกสัญญา · pill Option Chain disabled พร้อมเหตุผล | — |
| P20.5 คู่มือไทย | **DONE** | USAGE-TH.md — ติดตั้ง/token/ทุกหน้า/ความหมายตัวเลข/Lab/ขอบเขตที่ตั้งใจไว้ | — |
| P11 replay engine (รุ่นแรก) | **DONE** | lab/replay.rs — no-look-ahead (entry quote + expiry stock available-at gates), hold-to-expiry + ledger จำลอง, unresolved ≠ ขาดทุน · 4 tests (win/assigned/look-ahead/missing) | orchestration จาก archive จริง = รอข้อมูลสะสม |
