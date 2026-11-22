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

# Dev mode (mock data, no keys needed)
npm run dev

# Run tests
export PATH="/c/Users/bfipa/winlibs_tmp/mingw64/bin:$PATH"  # MinGW portable
cargo test -p radar-core --features "client stream"

# Build frontend
npm run build

# Build Tauri app (requires MSVC + Rust)
cargo tauri build
```

## API Keys

ดู `.env.example` สำหรับตัวอย่างการตั้งค่า

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
  └── webull/
      ├── contracts.rs — OCC symbol parser
      ├── sign.rs      — HMAC-SHA1 request signing
      ├── rest.rs      — REST policy (throttle/cache)
      ├── discovery.rs — Contract discovery pipeline
      ├── transport.rs — Real HTTP client (reqwest)
      ├── stream.rs    — MQTT streaming (rumqttc)
      └── pb/          — Protobuf decode (prost)

src/                   — React frontend (TypeScript)
src-tauri/             — Tauri bridge (commands + settings)
design/                — Design baseline (DESIGN.md + prototype)
tests/fixtures/        — Golden test fixtures (deterministic)
```

## Design System

อ้างอิงจาก `design/DESIGN.md` — V2 Modern Visual Refinement

- โทนสี: Charcoal dark (#0b0d11), flat 1px borders
- ฟอนต์: Inter + IBM Plex Sans Thai, tabular numerals
- โครง: 4-column shell (icon rail / sidebar / main / right panel)
- ไม่ใช้ gloss, gradients, heavy shadows

## Testing

```bash
# Unit + golden tests (66 tests)
cargo test -p radar-core --features "client stream"

# Frontend
npm run check   # TypeScript + ESLint
npm run build   # Vite build

# Wiremock integration (requires `client` feature)
cargo test -p radar-core --features client --test transport_wiremock

# Live smoke (requires real API keys)
cargo test -p radar-core --features client --test transport_wiremock -- --ignored
```
