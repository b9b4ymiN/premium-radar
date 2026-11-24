# PREMIUM RADAR

CSP / Covered Call screener สำหรับหุ้น US ผ่าน Webull Thailand API — desktop app (Tauri 2)

## ติดตั้ง

### สำหรับผู้ใช้ทั่วไป
1. ดาวน์โหลด installer (.msi / .exe) จาก releases
2. เปิดแอปครั้งแรก → หน้า Setup จะขึ้นมา
3. ใส่ Webull App Key + Secret (สมัครจาก Webull Developer Portal)
4. เลือก AI provider (Z.ai / OpenAI / Ollama / custom) หรือข้ามไปก่อน
5. กด "บันทึกและเริ่มใช้งาน"

### สำหรับ developer

```bash
# Clone + install
git clone <repo>
cd option2
npm install

# Browser preview (mock data, no keys, no desktop shell)
npm run dev

# The real app in dev mode (keys entered once in the Setup screen)
npm run tauri dev

# Gates — run from the repo root
npm run check                                                    # TypeScript + ESLint
npm run build                                                    # Vite build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                           # unit, golden, wiremock

# Build the Tauri app (requires MSVC + Rust)
cargo tauri build
```

ตารางว่า API ไหนของ Webull ไทยใช้ได้จริงด้วย key นี้: [`docs/WEBULL_TH_CAPABILITY_MATRIX.md`](docs/WEBULL_TH_CAPABILITY_MATRIX.md) · คู่มือใช้งานภาษาไทย: [`USAGE-TH.md`](USAGE-TH.md)

## API Keys

คีย์กรอกในหน้า **Setup** ของแอปและเก็บใน OS keyring ของเครื่อง (service `premium-radar`) — ไม่เก็บเป็นข้อความธรรมดา และถ้า keyring ใช้ไม่ได้แอปจะแจ้ง error แทนที่จะ fallback

ตัวแปร environment ยังใช้เป็นทางเลือกได้ (ดู `.env.example`)

| Variable | จำเป็น | คำอธิบาย |
|----------|--------|----------|
| `WEBULL_APP_KEY` | ✓ | Webull OpenAPI App Key |
| `WEBULL_APP_SECRET` | ✓ | Webull OpenAPI App Secret |
| `LLM_BASE_URL` | — | LLM endpoint (เช่น `https://api.z.ai`) |
| `LLM_API_KEY` | — | LLM API key |
| `LLM_MODEL` | — | ชื่อโมเดล (เช่น `glm-5.3`, `gpt-4o`, `llama3.2`) |

หรือใช้ shorthand: `GLM_API_KEY` หรือ `OPENAI_API_KEY`

## โครงสร้างโปรเจ็ค

```
crates/radar-core/     — Rust domain logic (framework-free)
  ├── quant.rs         — Black-Scholes / IV / Greeks
  ├── screener.rs      — Enrichment / Grading / Filters
  ├── engine.rs        — Coalescing state store
  ├── copilot.rs       — LLM function calling
  ├── lab/             — Option Lab: payoff, probability, storage, replay, calibration
  └── webull/
      ├── contracts.rs — OCC symbol parser
      ├── sign.rs      — Request signing (V3 HMAC-SHA256 by default, V2 selectable)
      ├── transport.rs — HTTP client + one shared request coordinator
      ├── rest.rs      — Fetcher trait, error classification
      ├── auth.rs      — Access token create / check
      ├── stocks.rs    — Stock snapshots, bars, instrument profiles
      ├── options.rs   — Option quotes (greeks) and trades
      ├── fundamentals.rs — Company profile, earnings window, dividends, analysts
      ├── screeners.rs — Gainers / losers / most active / 52-week / dividend / sectors
      ├── watchlists.rs — Broker watchlists, read only
      ├── trading.rs   — Accounts, balances, positions, open orders (read only)
      ├── discovery.rs — Contract discovery pipeline
      ├── stream.rs    — MQTT streaming (rumqttc)
      └── pb/          — Protobuf decode (prost)

src/                   — React frontend (TypeScript)
src-tauri/             — Tauri bridge (commands + settings + scan worker)
design/                — Design baseline (DESIGN.md + prototype)
docs/                  — Webull Thailand capability matrix
tests/fixtures/        — Golden test fixtures (deterministic)
```

## Design System

อ้างอิงจาก `design/DESIGN.md` + `design/REFINEMENT-V2-1.md`

- โทนสี: Charcoal dark (#0b0d11), 1px borders
- ฟอนต์: Inter + IBM Plex Sans Thai, tabular numerals
- โครง: 4-column shell (icon rail / sidebar / main / right panel) — icon rail เป็นเมนูเดียวของแอป
- Gradient เฉพาะ chrome (แถบหัวแอป หัว panel แถบราคา หัวตาราง) แบบ 2 สีเพื่อนกัน — พื้นที่ข้อมูลเป็นสีเรียบ ไม่มี gloss หรือเงาหนัก
- เขียว/แดง = บวก/ลบ (ค่าการเงิน และสัดส่วนความเห็นซื้อ/ขาย) · ฟ้า = การเลือกและการเน้น · เหลือง = คำเตือน

## Testing

```bash
# Rust: unit, golden (quant / screener / signing) and wiremock tests — 234 passed, 1 ignored
cargo test --workspace

# Frontend
npm run check   # TypeScript + ESLint
npm run build   # Vite build

# Read-only probe of the Thailand host with the saved key (needs the app set up once)
cd src-tauri && cargo run --example probe_capabilities
```
