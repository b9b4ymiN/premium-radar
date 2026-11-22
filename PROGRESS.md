# PROGRESS — loop ledger (memory นอก context)

> Method: [[concepts/loop-engineering]] + [[concepts/graph-engineering]] + [[procedural/development/builder-validator-loop]] (จาก brain)
> **กติกาที่บังคับใช้ตั้งแต่ 2026-11-22:**
> - ทุก top-level task: **Builder** (implement จน gates เขียว) → **Independent Validator** (fresh read-only agent ตรวจตาม DOD เอง: อ่านโค้ดเอง รันคำสั่งเอง ไม่เชื่อ builder summary) → **PASS เท่านั้นถึงเปลี่ยนสถานะเป็น DONE** (1 validator ต่อ task)
> - Terminal states ต่อ task: `DONE / BLOCKED / NEEDS_HUMAN / FAILED / NOT_STARTED / IN_PROGRESS` — error ไม่นับเป็น DONE เด็ดขาด
> - Evidence ทุกอย่าง = คำสั่งที่รันจริง + ผล + commit SHA (traceability: "ทำไม task นี้ถึง DONE")
> - ไฟล์นี้คือ **source of truth เดียวของสถานะ** — session ใหม่/agent ใหม่ อ่านไฟล์นี้ + PLAN.md แล้วทำงานต่อได้ทันที ไม่ต้องพึ่ง transcript
> - Budget: validator 1 ตัวต่อ task; แก้ error ย้อนหลังใน same thread สูงสุด 4 rounds ก่อนเข้า FAILED

## Ledger

| Task | Status | Evidence | Validator |
|---|---|---|---|
| P0.1 git repo + .gitignore | DONE | commit `7140953`, `3aea634` | — (ไม่มีโค้ด แค่ docs/config) |
| P0.2 Tauri scaffold | DONE (frontend ส่วน) | commit `0616f2d` — tsc+eslint+vite build เขียว | รอ validator รวมกับ P0 ปิดท้าย |
| P0.3/0.4 quality gates | DONE (frontend) / **BLOCKED (Rust)** | `npm run check`+`build` เขียว; Rust ค้างรอ MSVC | รวมกับข้างบน |
| P0 Rust compile | **NEEDS_HUMAN** | VS Build Tools ติดตั้งผ่าน winget ค้างรอ **UAC prompt บนจอ** (~15 นาทีแล้ว ไม่มี installer เริ่มรัน) — ผู้ใช้ต้องกด Yes หรือแจ้งให้เปลี่ยนวิธี | — |
| P1.6 golden fixtures | DONE | commit `261f894` — 6 ชุด, agent ตรวจซ้ำ 0 diffs, deterministic (sha256 ตรง 2 รอบ) | จะ validate รวมกับ P1 (golden test คือตัวตรวจ L1) |
| P1.1 OCC parser | IN_PROGRESS (โค้ดเสร็จ รอ compile) | `src-tauri/src/webull/contracts.rs` + unit tests | รอ |
| P1.2 BS/IV/greeks | IN_PROGRESS (โค้ดเสร็จ รอ compile) | `src-tauri/src/quant.rs` + `tests/golden_quant.rs` (159+172 cases) | รอ |
| P1.3–1.5 enrich/grade/filters/chain | IN_PROGRESS (โค้ดเสร็จ รอ compile) | `src-tauri/src/screener.rs` + `tests/golden_screener.rs` (402 rows/17 scenarios/8 scenarios) | รอ |
| P2.* Webull REST | NOT_STARTED | — | — |
| P3.* MQTT | NOT_STARTED | — | — |
| P4.* engine+bridge | NOT_STARTED | — | — |
| P5.* frontend shell | NOT_STARTED | — | — |
| P6.* copilot | NOT_STARTED | — | — |
| P7.* packaging | NOT_STARTED | — | — |

## Current blocking (สิ่งเดียวที่ขวางทาง)

**P0 Rust compile รอ VS Build Tools** — winget (PID 11108) รอ UAC:
- ทางออก A: ผู้ใช้เห็น dialog สีน้ำเงิน "Visual Studio Installer" → กด **Yes** → รอติดตั้ง 10-15 นาที → รัน `cargo fmt && cargo clippy -- -D warnings && cargo test` ใน vcvars64 env
- ทางออก B: ไม่มี dialog / ไม่ปรากฏ → kill winget → ดาวน์โหลด vs_BuildTools.exe ตรงจาก Microsoft แล้วรันแบบเห็น progress

## Loop spec ของ task ถัดไป (เมื่อ toolchain พร้อม)

```yaml
task: P1-close
trigger: MSVC linker ใช้ได้ (vcvars64 → cargo build ผ่าน)
goal: cargo fmt --check && cargo clippy -D warnings && cargo test เขียวทั้งหมด ใน src-tauri
verification: L1 (golden fixtures = deterministic ตรงค่า Python 1e-6 / grade exact / why-string exact)
stopping: DONE เมื่อ builder เห็นเขียว + validator agent ยืนยัน PASS ทุก DOD ข้อ || FAILED เมื่อแก้ 4 rounds ไม่ผ่าน || BLOCKED ถ้าเจอ external blocker
memory: อัปเดตตารางข้างบน + commit SHA
```
