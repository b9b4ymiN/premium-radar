# PLAN — PREMIUM RADAR: APP v2

**วันที่:** 2026-11-23 · **สถานะ:** ข้อเสนอ Phase 14–20; งานวางแผน ไม่ใช่รายงาน implementation  
**อ้างอิง:** `tmp/codex-consult/BRIEF2.md`, `tmp/codex-consult/p10-probe-apis.json`, `PRODUCT.md`, `PLAN.md`, `PLAN-LAB.md` รวม Appendix C–E, `design/DESIGN.md`  
**เอกสาร API สำหรับ implementation:** https://developer.webull.co.th/apis/llms.txt

---

## 0. ภาพรวม

ขยาย PREMIUM RADAR จากหน้าคัดสัญญาเป็นพื้นที่ทำงานที่เชื่อม **หน้าแรก → รายการติดตาม → กราฟ/ข้อมูลหลักทรัพย์ → Options Monitor / Lab → พอร์ตจริงแบบอ่านอย่างเดียว** ผู้ใช้เห็นข้อมูลที่จ่ายสิทธิ์ไว้ในบริบทที่ช่วยตัดสินใจ และรู้ว่าข้อมูลใดสด ข้อมูลใดเก่า ข้อมูลใดยังไม่มี

“ใช้ให้ครบที่สุด” หมายถึงใช้ความสามารถที่ API และ entitlement รองรับอย่างมีประโยชน์ ไม่ใช่เรียกทุก endpoint ตลอดเวลา หรือเปิด feature ที่ข้อมูลยังไม่พอ โดยเฉพาะ OPRA ไม่ใช่หลักฐานว่า stock depth, fundamentals หรือบริการทุกประเภทได้รับสิทธิ์แล้ว

### ข้อกำหนดกลาง

1. Tauri 2; Rust ทำ financial math, percentage change, sorting/ranking ทางการเงิน, OHLC aggregation, indicators, portfolio totals และ chart series; frontend รับ DTO แล้ว format/render และจัด UI state
2. ข้อมูลตลาด/บัญชีจาก Webull TH API เท่านั้น; watchlist เก็บ local ไม่มี endpoint สมมติ; keys และ token ผ่าน Rust + keyring เดิม ไม่อ่าน `.env` ไม่ส่ง secrets เข้า log/export/frontend
3. อ่านบัญชีและ positions เป็นขอบเขต v2 ที่เจ้าของขอแล้ว จึงขยาย non-goal portfolio tracking ของ v1 เฉพาะส่วน read-only; **ไม่มี create/modify/cancel order, exercise, transfer หรือการซื้อขายอัตโนมัติ** ต้องเป็น change request แยกหากต้องการภายหลัง
4. Thai UI, charcoal V2, rail / left / main / right และ breakpoints เดิม หน้าปัจจุบัน Monitor/Lab ไม่ถูกรื้อ; ใช้ช่อง navigation ที่มีอยู่สำหรับหน้าใหม่
5. shared throttle 1 call / 1.1s รวมทุก REST caller; cache 3s เดิม; stale/as-of/source ทุกส่วน ไม่ใช้ “สด” เพียงเพราะ token ใช้ได้หรือ MQTT connected
6. ไม่เพิ่ม multi-ticker option streaming ในรอบนี้: มีหลาย watchlist quotes ได้ แต่ scanner ทำทีละ symbol ตามเดิม
7. P14+ เป็นเลข phase ของ APP v2 เท่านั้น ไฟล์ชื่อ `p10-probe-apis.json` เป็นหลักฐาน probe ไม่ใช่การเปลี่ยนเลข phase ของ Lab

**การใช้ skills:** ต่อแนวทาง `impeccable` สำหรับหน้าใหม่ภายใต้ระบบเดิม และ `writing-plans` สำหรับ task/DOD ตามที่อ่านไว้ในงานก่อน รูปแบบเอกสารและที่อยู่ไฟล์ใช้ตาม brief ไม่สร้าง artifact หรือแก้ design context นอก write scope

**ขอบเขตตรวจรอบนี้:** อ่าน brief, JSON probe และเอกสารโครงการ; CodeGraph ยังไม่ initialized จึงไม่ได้ยืนยัน current source/navigation handlers; URL llms.txt เปิดไม่สำเร็จทั้ง web tool และ HTTP จากเครื่อง จึงไม่อ้างว่าได้ตรวจ balance path/schema ใหม่แล้ว P14 ระบุงานตรวจที่เหลืออย่างชัดเจน ไม่รัน authenticated probe ในงานวางแผนนี้

---

## 1. หลักฐาน API และ capability gates

Probe timestamp `2026-11-22 23:09:56 UTC` ตรงกับเช้า 23 พ.ย. เวลาไทย ค่า `ms=0` ใน JSON ไม่ใช่ benchmark latency

| Capability | หลักฐานที่มี | ข้อจำกัด / gate ก่อนใช้ |
|---|---|---|
| Stock daily bars | POST `/market-data/stocks/bars/list` ไม่เติม `/openapi`; NVDA 1,200 bars ตั้งแต่ 2022-02-10 ถึง 2026-11-20 | ราว 4.8 ปีในตัวอย่าง ไม่รับประกัน 5 ปีทุกหุ้น; response เรียงใหม่→เก่า ต้อง normalize; adjustment, session labels, batch limits และ pagination ยังต้องยืนยัน |
| Stock intraday | path เดียวกัน M5×100 ได้ 100 bars ของ NVDA ช่วง 19–20 พ.ย. | ไม่ใช่หลักฐานเก็บ M5 ย้อนหลัง 5 ปี หรือมี M15/W/M native; ตรวจ session, partial bars และความลึกจริง |
| Option bars | GET `/openapi/market-data/option/bars` ตอบ 200; ข้อจำกัดตาม Lab probe ก่อนหน้า | JSON รอบนี้ `depth=[]` ไม่เพิ่มหลักฐาน historical depth; stock bars ไม่ทำให้ expired option universe หรือ historical bid/ask กลับมามี |
| Accounts | GET `/trading/accounts/list` 200, count=1 | ต้องรองรับหลายบัญชี/ไม่มีบัญชี/สิทธิ์หมด แม้ live sample มีหนึ่งบัญชี; ไม่เผย account ID ใน report |
| Positions | GET `/trading/assets/positions/list` พร้อม account_id 200, count=0 | พิสูจน์ empty success เท่านั้น; OPTION legs/cost/P&L เป็น schema claim ใน brief ต้องตรวจเอกสารและ fixture ก่อนอ้าง nonempty support |
| Balance | paths ที่เดาตอบ 404 | หา path/method/params จาก account-balance docs แล้ว probe; 404 ไม่แปลว่าเงินเป็นศูนย์ ไม่เดา endpoint ต่อเป็น loop |
| Instrument profiles | GET `/trading/instruments/stocks/profiles/list` 200 ได้ชื่อจริง NVDA และ SPY | JSON ยืนยัน names/count; flags/margin/lot ตาม brief ต้องตรวจ field/enum; ไม่มีหลักฐาน sector, market cap, PE, earnings calendar, financial statements |
| Depth | GET `/market-data/stocks/depths/list` 417 | ต้องตรวจชนิด/ค่าของ depth และ overnight_required ตาม docs; 417 ยังแยก parameter vs entitlement ไม่ได้ ห้ามแสดง order book ปลอมหรือเรียกว่า OPRA ไม่ครอบคลุมโดยยังไม่ตรวจ |

**Capability registry:** แต่ละ feature มี `verified / unverified / unavailable` พร้อม environment, checked_at, evidence และ reason; แยก supported capability จาก runtime health เช่น supported แต่ token หมดอายุ ไม่เปลี่ยนเป็น endpoint ไม่รองรับ

Authentication ของ endpoints ที่ผ่านใช้ระบบเดิมได้ ไม่เขียน signer ใหม่ และไม่ทำ global prefix rewrite: endpoint descriptor ระบุ exact path/method แยก legacy `/openapi/...` กับ paths ใหม่

---

## 2. แผนหน้าและเส้นทางใช้งาน

| หน้า | งานหลักของผู้ใช้ | ข้อมูล / การเชื่อมโยง |
|---|---|---|
| Home | รู้ว่าอะไรควรดูต่อและระบบพร้อมแค่ไหน | quotes ของ active watchlist, ขึ้น/ลงเด่นในรายการนี้, selected-symbol chart, capture/token/feed status, shortcuts |
| Watchlists | เพิ่ม ลบ จัดลำดับ และเลือกรายการหุ้น | local named lists; symbol validation จาก instrument service; เลือกหุ้นแล้ว “ดูกราฟ / เปิด Radar / เปิด Lab” |
| ราคาหุ้นและข้อมูลหลักทรัพย์ | ดูราคาในหลายช่วงเวลาและรู้ว่ากำลังดูอะไร | candles/volume, profile name/type/flags ที่ยืนยันได้, price source; subview ของ selected symbol ใน main เดิม |
| Options Monitor | เลือกสัญญา CSP/CC ทีละหุ้น | workflow เดิม เพิ่มกราฟหุ้นจริงใน chart slot; ไม่ปรับ columns/grade หรือ auto-rescan ทุก watchlist row |
| Option Lab | วิเคราะห์และอ่านผลทดลอง | หน้าเดิม; link ไป capture status/history; Home ไม่เรียกสมมติฐานว่า actual returns |
| Portfolio | ดูสถานะถือครองและเงินของบัญชีที่เลือก | account selector, positions, balance เมื่อยืนยันแล้ว, refresh/as-of, แยก holdings detail; ไม่มี order controls |
| Setup / สถานะระบบ | แก้ credentials/token/feed ที่ผิดปกติ | ใช้หน้าเดิม; แสดง last success/error แบบไม่มี secrets และทางแก้ที่เจาะจง |

**Navigation policy:** rail ใช้ Home / Watchlists / Options / Portfolio ในพื้นที่เดิม; Options Monitor และ Option Lab คง main tabs ภายใน Options; หน้าราคาหุ้นเข้าจาก selected symbol และกลับตำแหน่งเดิมได้ เปลี่ยน route ไม่ reset filters, account หรือ frozen Lab run โดยไม่จำเป็น

Home เป็น default landing หลัง setup สำเร็จ พร้อมปุ่มกลับงานล่าสุด ถ้า credentials ไม่พร้อมให้เปิด setup/สถานะที่แก้ได้โดยตรง ไม่มี dashboard จำลองให้ดูเหมือนข้อมูลจริง

---

## 3. UI contract ภายใต้ V2

รักษา 4 คอลัมน์ base `50 / 225 / main / 320px`, header 43px, status bar 25px และ breakpoint 1420/1150/790 ตาม DESIGN.md; main content ของหน้าใหม่ใช้พื้นที่เดิม ไม่ทำ sidebar ซ้อนหรือเพิ่ม column ที่ห้า

- **Home:** context header ระบุ active list + update time; top chart slot 258px เป็น selected-symbol price + stats; summary band 88px เป็นขึ้นเด่น/ลงเด่น/สถานะ capture; lower table เป็น watchlist quotes คลิกแถวเปลี่ยน context; detail ขวาแสดงข้อมูลหลักทรัพย์และ source เหนือ assistant
- **Watchlists:** main header มีเลือก/สร้าง/เปลี่ยนชื่อ list; lower table เป็น editor พร้อม add/search/reorder/remove; top chart แสดงหุ้นที่เลือก เมื่อลิสต์ว่างเป็นคำอธิบายและปุ่มเพิ่มหุ้น ไม่ใช้ sample ticker เติมเงียบ ๆ
- **Portfolio:** context header มีบัญชีที่ mask และ as-of; summary แสดงเฉพาะ totals ที่มีความหมาย; main table แยกหุ้น/option legs พร้อม group underlying; detail ขวาตาม position ที่เลือก ไม่มีกราฟ performance/equity curve ที่อนุมานจาก current positions
- **Company/instrument:** ใช้ detail ของ selected symbol และ subview ใน main เพื่ออ่านบนจอที่ซ่อน right panel; ไม่สร้างหน้าบริษัทที่เต็มด้วยช่องว่าง PE/sector
- **Price chart:** อยู่ใน chart slot จริง ไม่ยัดกราฟลง QuoteHeader ซึ่งเป็นพื้นที่ตัวเลข; action “ดูกราฟเต็มพื้นที่กลาง” เปิด subview ภายใน main โดยรักษา shell และปุ่มกลับ ไม่เพิ่ม floating layout editor หรือรื้อ Monitor

พื้น #0b0d11 / #10141a / #171c24; divider 1px #292f39; text #f0f3f8, muted #a2adbc; blue #4b8cf7 สำหรับเลือก/focus, green #24c99a / red #f4788c / amber #f0bd70 ตาม semantics; Inter/Noto Sans Thai, tabular numbers, ตัวเลขชิดขวา สีคู่ข้อความและเครื่องหมายเสมอ

ECharts เป็น renderer ร่วม: candles, volume, crosshair, zoom และ tooltip ใช้ Rust series; JS ไม่ resample bars หรือคำนวณ MA/% return เอง ทุก chart มีข้อมูลตารางที่อ่านด้วย keyboard ได้ ไม่บังคับทุกหน้ามี chart หากข้อมูลไม่มี

---

## 4. Data / refresh / resource contract

### 4.1 Scheduler เดียวและ request budget

ความสามารถสูงสุดตาม throttle ประมาณ `60/1.1 = 54.5 calls/min` ก่อน latency/backoff จึงไม่วางแผนใช้เต็มเพดาน ตั้ง **automatic background budget เริ่มต้นไม่เกิน 20 calls/min** ขณะ interactive load ไม่ใช้ budget นี้ก็ยังผ่าน shared throttle เดิมทั้งหมด

Priority: auth ที่จำเป็นเพื่อให้คำขอทำงาน → user refresh/selected symbol/scan → visible quotes/portfolio → capture และ background enrichment ตาม deadline; มี bounded fairness เพื่อไม่ให้ capture อดตลอดและไม่ให้ backfill แย่ง scanner รัว ๆ ใช้ request dedupe, in-flight coalescing, generation ID, cancel stale work และ exponential backoff เดิม

| งาน | Policy เริ่มต้น |
|---|---|
| Watchlist quotes | active list ไม่เกิน 20 symbols ในรอบ auto-refresh; ทุก 30s เมื่อหน้าแสดงใน session ตลาดที่ยืนยันได้; batch เมื่อ schema รองรับเท่านั้น ถ้าไม่รองรับใช้ round-robin และแสดงเวลาต่อแถว/รอบจริง ไม่รับประกันครบ 30s |
| Background / hidden lists | ไม่ poll ทุก list; quotes ที่เก็บแล้วอ่านได้พร้อม age; manual refresh เข้า queue ที่แสดง progress |
| Market closed / window inactive | ลด quote polling เป็น 5 นาทีหรือ on-focus ตามสถานะ; ถ้าไม่รู้ holiday/session ห้ามแสร้งรู้ว่าตลาดเปิด/ปิด ใช้ช่วงเวลาคาดการณ์ติดป้ายและ timestamp จริง |
| Stock daily bars | fetch on symbol/range demand; historical cache versioned ตาม adjustment/session; refresh หลังมี session ใหม่ ไม่โหลด 1,200 bars ทุก tick |
| Intraday | เฉพาะ selected visible chart; M5 refresh ทุก 60s เมื่อ foreground; M15 derived จาก M5 ได้เมื่อ coverage ครบ; streaming quote overlay แยกจาก completed candles |
| Profiles | on first selection; metadata store อายุเริ่มต้น 24h โดยแสดง fetched_at; retry/manual refresh ผ่าน scheduler |
| Portfolio | on page entry + ทุก 60s เฉพาะหน้า visible; manual refresh coalesced; ไม่ poll ทุก account พร้อมกัน |
| Depth | on demand เมื่อ capability ผ่าน; REST refresh เริ่ม 5s เฉพาะ panel เปิด จึงกินประมาณ 12 calls/min ต้องลด background quotes ใน budget; ไม่มีเพิ่ม MQTT stock topic โดยยังไม่ทดสอบ |

cache 3s เป็น REST response cache เดิม; historical/metadata storage เป็นอีกชั้นที่มี invalidation และ provenance ไม่เปลี่ยน freshness label ให้ใหม่ตามเวลาที่อ่าน cache

**ตัวอย่าง budget:** 20 symbols แบบ one-call-per-symbol ทุก 30s = 40 calls/min จึงเกิน background budget ต้องยืดรอบหรือใช้ batch ที่พิสูจน์แล้ว ห้ามเปิด loop 20 ตัวอิสระเพื่อหลบ limiter; portfolio/depth/intraday ใช้งบรวมเดียวกัน

**App กับ scheduled recorder:** ทั้งคู่ใช้ key เดียวกันและอาจรันพร้อมกัน ต้องมี cross-process arbitration (เช่น OS lock + shared next-allowed timestamp หรือ local broker เดียว) ไม่ใช่ limiter แยก process จนเกิน 1.1s รวม; graceful cancel/recovery เมื่อ process ตาย และ token refresh เขียน keyring โดยไม่แข่งกัน

### 4.2 DTO และ boundaries ที่เสนอ

ใช้ adapter/command registry จริงที่ยืนยันใน P14.1 ไม่ย้ายโมดูลเดิมเพียงเพื่อให้ตรงผังนี้:

| Boundary / proposed module | หน้าที่ |
|---|---|
| Rust `webull` endpoint adapters | exact paths, typed request/response, nullable numbers, read-only account requests |
| Rust `market_history` | normalized bars, session/adjustment, W/M/M15 aggregation, indicators, gap flags |
| Rust `watchlists` | local CRUD, validation, atomic persistence/migration, quote scheduling |
| Rust `portfolio` | account context, positions/balance reconciliation, signed exposure, source/currency metadata |
| Rust `app_overview` / `capabilities` | Home summary, list movers, feed/token/capture states; แยก transport health กับ data freshness |
| Tauri bridge | typed commands/events, account/symbol generation IDs, validation, no arbitrary URL/trading proxy |
| React page components + existing shell | Home, Watchlists, PriceView, Portfolio, InstrumentDetail; shared charts/tables/states ตาม tokens เดิม |

DTO ทุกชุดมี `source, as_of, received_at, freshness, status, request_generation`; numbers ที่ไม่มีใช้ nullable ไม่ใช่ 0; status แยก loading/ready/empty/stale/error/unsupported/partial

Proposed commands: `get_home_overview`, `get_stock_history`, `get_instrument_profile`, `list_watchlists`, `save_watchlist`, `delete_watchlist`, `list_accounts`, `get_portfolio`, `refresh_portfolio`, `get_depth` โดยชื่อสุดท้ายตรวจ naming จริงใน P14; bridge รับ account handle ที่ resolve ฝั่ง Rust ไม่รับ raw endpoint จาก frontend

---

## Phase 14 — Audit ทุกหน้า + capability contract

| # | Task | ไฟล์ / ขอบเขต | DOD |
|---|---|---|---|
| 14.1 | ทำ current-state inventory | `src/`, Tauri command registry, core adapters, audit document | ทุก route/tab/icon/button/menu/search/chart toggle มีรายการ handler, data source, loading/empty/error state, keyboard behavior และสถานะ implemented/mock/dead/unsupported; ระบุ path/symbol จริง; baseline tests และ UI captures บันทึกตามจริง ไม่ใช้จำนวน command ใน brief แทนการตรวจ |
| 14.2 | Resolve balance / depths / schema | docs + targeted sanitized probe | หา exact balance request จาก account-balance docs; depth ใช้ชนิดและ enum ที่ docs ระบุ; บันทึก 200/schema หรือ reason ที่ยืนยันได้; 417/404 ไม่ถูกตีความว่า empty; fixtures account/positions nonempty/OPTION และ profile fields มี provenance; ไม่ส่ง orders และไม่เขียน signer ใหม่ |
| 14.3 | Contract tests สำหรับ verified APIs | adapter tests + endpoint registry | exact path/method จาก probe ผ่าน mocks; wrong `/openapi` prefix regression ถูกจับ; string/null/empty/error parsing ผ่าน; missing account_id ไม่มี request; empty positions แยก access denied/network error |
| 14.4 | Route/state design และ scope ledger | navigation map, capability matrix | mapping Home/Watchlists/Price/Portfolio/Options/Lab/Setup ชัด; controls ทุกตัวจาก audit ถูก assign ไป task, alias, remove หรือ disabled-with-reason; ไม่มี “ไว้ค่อยทำ” ที่ไม่มี owner/task; read-only portfolio เป็น v2 scope, order execution ยังคง non-goal |

**Phase DOD:** มีรายงาน audit ของ UI จริงและ capability matrix ที่ตรวจสอบได้; endpoint ที่ยังไม่ผ่านถูก gate ราย feature ไม่บล็อก stock charts/watchlist/Home

**ความเสี่ยง:** เอกสารเรียกได้แต่ prod ไม่เหมือน — ใช้ docs เพื่อสร้าง request และใช้ sanitized live evidence เพื่อยืนยัน ไม่เดา schema จากชื่อ endpoint

---

## Phase 15 — Market data foundation + กราฟราคาจริง

| # | Task | ไฟล์ / ขอบเขต | DOD |
|---|---|---|---|
| 15.1 | Shared scheduling และ cross-process quota | REST scheduler + recorder integration | mock clock พิสูจน์ spacing ≥1.1s รวม app/recorder; burst/429/cancel/restart ไม่ bypass; in-flight dedupe และ queue priority/fairness ผ่าน; ไม่มี hidden-page polling รั่ว |
| 15.2 | Stock bars adapter/cache | market_history + REST adapter | D และ M5 ใช้ path ที่ผ่าน; sort ascending, dedupe, validate OHLC/volume/null/DST; session label/available-at แยก; latest incomplete bar ติดป้าย; raw-vs-adjusted basis ตรวจจาก docs/probeแล้วเก็บใน cache key ถ้ายังไม่ทราบแสดง “ฐานปรับราคาไม่ยืนยัน” |
| 15.3 | Timeframe/series ใน Rust | market_history tests | resolution D/W/M/M5/M15; W/M aggregate จาก D ด้วย calendar grouping, M15 จาก M5 aligned session เมื่อมีข้อมูลครบ; fixture OHLC first/max/min/last/volume sum ผ่าน; gaps ไม่สร้าง candles และ partial groups ติดป้าย; native interval ใช้ได้เฉพาะเมื่อ probe ผ่าน |
| 15.4 | Chart component + full-main subview | existing chart slot + PriceView | ECharts candles/volume/crosshair/zoom; ranges 1D/5D/1M/3M/1Y/สูงสุดเท่าที่มี; range แยกจาก resolution; unavailable history แจ้งช่วงจริง ไม่ขยายเป็น 5 ปี; switching symbol/range ไม่รับ late response ของตัวเก่า; render-only frontend |
| 15.5 | Chart controls ที่มีประโยชน์ | chart toolbar + Rust indicators | MA5/MA20 และ volume toggle มี handler จริง; MA คำนวณ Rust และ null ก่อน warm-up; drawing/MACD/RSI จาก audit ถ้ายังไม่ implement ให้ซ่อน/disabled พร้อมเหตุผล ไม่ปล่อยปุ่มแกล้งทำงาน; quote overlay ไม่เปลี่ยน OHLC ย้อนหลังอย่างไม่มีหลักฐาน |

**Phase DOD:** NVDA แสดง history ตาม probe ได้และ short-history symbol ไม่หลอกว่ามีครบ; daily และ intraday มี source/session/as-of; Monitor เปลี่ยนจาก placeholder เป็นกราฟจริงโดย shell และ scanner เดิมไม่เสีย

**ขอบเขต Lab:** underlying bars เพิ่มประโยชน์ต่อ outcome/benchmark แต่ไม่ทำให้ daily post-close archive กลายเป็น intraday executable quotes หรือทำให้ P11 historical option universe ผ่าน

---

## Phase 16 — Watchlist management + instrument information

| # | Task | ไฟล์ / ขอบเขต | DOD |
|---|---|---|---|
| 16.1 | Local watchlist model | Rust watchlists + app-data persistence | named lists มี stable IDs/order/symbol refs; add/rename/delete list, add/remove/reorder symbol และ duplicate prevention; migrate default list; atomic write/recovery จาก corrupt file ไม่ลบข้อมูลเงียบ; ไม่มี credentials ใน store |
| 16.2 | Management page และ symbol validation | Watchlists + profile adapter | add ด้วย ticker ที่ validate ผ่าน API; distinguish unknown symbol/permission/offline; offline save ได้เป็น pending verification ไม่ขึ้น valid; reorder ด้วย keyboard ได้, remove มี undo; deleting active item เลือกตัวถัดไปหรือล้าง detail ไม่มี ghost selection |
| 16.3 | Quotes หลายหุ้นภายใต้ quota | watchlist quote service | active list polling/batch fallback ตาม §4; row-level timestamps/source; load มากกว่า auto-refresh cap ยังเก็บรายชื่อได้และบอกขอบเขต refresh; click “เปิด Radar” สแกนเพียง symbol นั้น ไม่มี contract-discovery ทุกแถว |
| 16.4 | Instrument profile ที่ซื่อสัตย์ | InstrumentDetail + Rust mapping | แสดง name/ticker และ type/currency/exchange/lot/flags เฉพาะ fields ที่ยืนยัน; flags แปลไทยตาม enum ไม่เดา; SPY ไม่บังคับเป็น operating company; margin/eligibility flag ไม่ใช่คำแนะนำ leverage; absent field ไม่แสดงเป็น 0 |

**Phase DOD:** สร้างรายการสองชุด เพิ่ม/ย้าย/ลบหุ้น ปิดเปิดแอปแล้ว order เดิมอยู่; quotes ไม่แย่ง scanner; NVDA/SPY แสดงชื่อจาก profile จริง ไม่มี fabricated PE/market cap/sector

**MVP decision:** ไม่เพิ่ม fundamentals provider, company biography ที่ AI แต่ง, news feed หรือ earnings calendar หาก API ไม่มีหลักฐานให้ใช้ หน้านี้ชื่อ “ข้อมูลหลักทรัพย์” และอธิบายขอบเขตข้อมูลสั้น ๆ

---

## Phase 17 — Home ที่นำไปสู่การใช้งานจริง

| # | Task | ไฟล์ / ขอบเขต | DOD |
|---|---|---|---|
| 17.1 | Overview aggregation | Rust app_overview | quotes/movers ของ active watchlist เท่านั้น; daily change ใช้ current comparable session price กับ previous regular close ที่ยืนยัน ถ้า denominator missing/0 = ไม่มีค่า; ไม่เรียก top movers ทั้งตลาด; stale rows ไม่ติดอันดับ fresh โดยไม่ติดป้าย |
| 17.2 | Dashboard + shortcuts | Home in frozen shell | chart/table/summary ตาม §3; selection เชื่อม detail; shortcuts ไป Watchlist/Radar/Lab/Portfolio/Setup ทำงานและรักษา context; loading/empty/partial states มีทางทำต่อ; ไม่มี random cards หรือ sample performance |
| 17.3 | Token/feed/capture health | Rust status adapter + Home status | แยก token valid, transport connected, last quote received, last recorder attempt/success/target session/count/gaps; อ่านผล recorder ที่ตั้งเวลาแล้ว ไม่อ้างว่า app ต้องเปิดตลอด; scheduled-but-missed/error แสดงต่างจาก healthy; ใช้ health state เดิม ไม่ poll token ทุก render |
| 17.4 | Landing + session restoration | route/state persistence | first-run ไป setup, setup เสร็จไป Home; ปุ่มกลับงานล่าสุดไป route/symbol ที่ยัง valid; corrupted/removed context fallback ปลอดภัย; freeze Lab ไม่ถูก replace ด้วย live quote เมื่อกลับ Home แล้วย้อนมา |

**Phase DOD:** เปิดแอปแล้วรู้ว่าหุ้นในรายการไหนเคลื่อนไหวเด่น ข้อมูลอายุเท่าไร capture ครั้งล่าสุดสำเร็จหรือไม่ และไปหน้าที่ต้องการได้ใน action เดียวโดยไม่อ้างว่าราคาทุกแถวเป็น live

**ข้อจำกัด recorder เดิม:** Appendix D/E มี post-close headless recorder และ known holiday labeling limitation Home ต้องสะท้อนสถานะตามหลักฐานจริง; schedule configured ไม่ใช่ proof ว่าข้อมูลครบ และ snapshot ที่รับหลังปิดตลาดไม่ใช่ quote ณ close โดยอัตโนมัติ

---

## Phase 18 — Portfolio จริงแบบ read-only

| # | Task | ไฟล์ / ขอบเขต | DOD |
|---|---|---|---|
| 18.1 | Account selection และ access handling | Rust account service + bridge | multiple/none/expired permission fixtures; mask account label; switching account invalidate in-flight generation; ไม่มีข้อมูลบัญชีก่อนหน้าปน; account IDs อยู่ Rust mapping ไม่ลง logs/default exports |
| 18.2 | Typed positions + financial semantics | Rust portfolio + fixtures | stocks/options long/short, quantity sign/unit/multiplier/cost basis/market value/unrealized P&L ตรวจจาก schema; nonempty sanitized/documented fixtures ติดป้ายไม่ใช่ live sample; unknown deliverable/missing marks แสดง partial ไม่บังคับ ×100 ทุกสัญญา |
| 18.3 | Balance integration แบบ gated | balance adapter + reconciliation | exact path/schema ผ่าน P14.2 จึงเปิด totals; cash/settled cash/buying power/NAV/currency ใช้ชื่อความหมายตรง schema; positions=[] ไม่แปลว่า balance=0; หาก balance ไม่ผ่าน positions page ยังเปิดได้พร้อม “ยอดเงินยังดึงไม่ได้” ไม่มีรวมยอดที่เดา |
| 18.4 | Portfolio page + refresh | Portfolio table/detail | auto 60s เฉพาะ visible + manual coalesced; empty success แสดงไม่มีสถานะถือครอง; errors เก็บ last-known พร้อม stale badge ไม่แทนด้วย empty; quote/balance timestamps ต่างกันแสดง partial/asynchronous snapshot; ไม่อ้างว่ายอดทั้งหมดเป็น atomic snapshot |
| 18.5 | Link holdings → analysis → Lab | read-only context DTO | เลือก option leg เปิด contract analysis ได้เมื่อ instrument mapping ครบ; เลือกหุ้นไป Radar; “จำลองจากสถานะนี้” เป็น explicit immutable copy มี account-source time/version; ไม่ merge กับ Lab ledger, ไม่อ้าง broker P&L เป็น simulated result และไม่ส่งต่อ account data ให้ LLM อัตโนมัติ |

**Phase DOD:** live empty account ผ่าน end-to-end และ nonempty fixture matrix ผ่าน; absence of live holdings ถูกระบุเป็น validation limit ไม่เติม demo holdings บนบัญชีจริง; balance gate มีผลจริง; read-only boundary มี automated test ว่าไม่มี account-mutating routes/commands

### Financial rules สำหรับพอร์ต

- แสดง broker-reported fields เป็นหลักพร้อมชื่อ/หน่วย; ถ้าคำนวณเองให้ Rust คำนวณและแสดง methodology ไม่บวก derived P&L ซ้ำกับ broker P&L
- market value หุ้นและ option liabilities ต้องใช้ signed quantity และ multiplier/deliverable ที่ยืนยัน; P&L short option ห้ามกลับเครื่องหมายตามสีอย่างเดียว
- cost basis อาจต่อหุ้น/ต่อสัญญา/รวม ต้อง map ตาม schema; zero quantity, fractional stock, expired contract, unknown OCC, adjusted option และ missing bid/ask มี test
- แยก currency totals; ไม่มี FX source ไม่รวม USD กับ THB เป็นเลขเดียว ไม่ใช้ buying power แทนเงินสดพร้อมทำ CSP
- ไม่อนุมานว่า short call covered จากจำนวนหุ้นรวมอย่างเดียวโดยไม่มี reservation/context; ใช้ “ตรวจเบื้องต้น/ข้อมูลไม่พอ” และไม่ส่งคำสั่งแก้ความเสี่ยง
- ไม่มี daily/monthly return, realized P&L history หรือ account equity curve หากไม่มี history/cash flows รองรับ; unrealized P&L วันนี้ไม่ใช่กำไรวันนี้
- Lab 30% drawdown ceiling เป็น risk rule ของ simulation ไม่ใช่ broker account stop และไม่ได้บังคับใช้กับบัญชีจริง

---

## Phase 19 — Depth และปิดช่องว่างทุกหน้า

| # | Task | ไฟล์ / ขอบเขต | DOD |
|---|---|---|---|
| 19.1 | Order book เมื่อ capability ผ่าน | depth adapter + selected-symbol subview | typed params ตาม docs, entitlement/result ผ่าน probe; sorted bid/ask levels และ nullable sizes มี tests; แสดงจำนวน levels ที่ได้รับจริง, REST/as-of; ไม่มี fake ladder จาก last price; หากยังไม่ได้ใช้ disabled พร้อม reason โดยไม่ block release ฟีเจอร์อื่น |
| 19.2 | Navigation และ dead actions | audit ledger จาก 14.1 + UI | Analytics alias ไป Lab เฉพาะเมื่อความหมายตรง หรือซ่อน/disabled-with-reason; risk dashboard, bell, search, export, settings, chart controls และ menu ทุกตัวถูกตรวจจริง; ไม่มี enabled control ที่จบด้วย generic “ยังไม่ทำ” toast |
| 19.3 | Cross-page state integrity | route/state/bridge | Home→Watchlist→Chart→Radar→Lab→Portfolio→กลับ route เดิม ผ่าน; late results, removed ticker, account switch, empty filters และ stale contract ล้าง/รักษา state ถูกต้อง; ไม่มี copilot อ้างข้อมูลของ page/account/run เก่า |
| 19.4 | Missing/error/source copy | shared states + Thai copy | ไทยอ่านรู้เรื่อง, error แยก token/entitlement/network/validation/rate-limit; retry มี scope ไม่ restart ทุก feed; unsupported field ใช้ “API ยังไม่มีข้อมูลนี้” ไม่วาง placeholders ชวนให้เข้าใจว่าใกล้ได้ข้อมูลจริง |

**Phase DOD:** audit ledger ทุก row มี disposition และ acceptance evidence; นับ enabled dead controls = 0; feature ที่ถูก defer มีเหตุผลและ UI disposition ชัด ไม่ถือ unsupported APIs เป็น implementation success ที่ได้ข้อมูลแล้ว

---

## Phase 20 — Integration / security / performance / handoff

| # | Task | ไฟล์ / ขอบเขต | DOD |
|---|---|---|---|
| 20.1 | Reliability + quota soak | scheduler/integration tests | จำลอง foreground scan + quotes + chart + portfolio + recorder overlap อย่างน้อย 15 นาที; spacing/queue/cancel/fairness ไม่มี bypass; token expiry/429/disconnect/resume มีผลที่ UI; ไม่มี auto-retry ไม่จำกัดหรือ listener/timer เพิ่มทุกสลับหน้า |
| 20.2 | Privacy + read-only security review | Rust/bridge/frontend/export | keys/token ไม่ออก logs/bundle/export; account ID mask, sensitive balances ไม่ลง diagnostics; trading write endpoint ไม่อยู่ command allowlist; strings/API payload render safely; default account data อยู่ memory ไม่ persist เกินจำเป็น; logout/key deletion clear account cache |
| 20.3 | Visual/keyboard/resize acceptance | existing + new screens | captures wide >1420, 1366, 1024, ≤790 และ short viewport; shell/order/breakpoints เดิม; table internal scroll, sticky headers, Thai font/focus/contrast; page route ไปได้เมื่อ right panel ซ่อน; narrow state บอกข้อจำกัด ไม่เพิ่ม mobile drawer |
| 20.4 | Regression + packaging | workspace gates + desktop installers | Rust fmt/clippy/tests และ frontend check/build ตาม actual workspace ผ่าน; quant/Lab regression เดิมไม่เปลี่ยน; Windows/macOS smoke; platform ที่ยังตรวจไม่ได้ระบุค้าง ไม่อ้างผ่านจาก Windows อย่างเดียว |
| 20.5 | Thai guide + capability handoff | user guide, release/audit report | คู่มือ Home/watchlist/charts/portfolio + quota/freshness/fee/account semantics; สรุป verified/deferred/unsupported พร้อมหลักฐาน; ผู้ใช้ตั้ง list เปิดกราฟและตรวจพอร์ตได้โดยไม่อ่าน code; ตรวจทุก task DOD พร้อมข้อจำกัด nonempty live validation |

**Phase DOD:** APP v2 ใช้เส้นทางหลักครบจริง ไม่มี mock ที่ดูเหมือน live ไม่มี order placement และมีรายงานชัดว่าค่า OPRA ถูกใช้กับความสามารถใดโดยไม่สัญญาว่าครอบคลุม API ทุกชนิด

---

## 5. Missing-page audit checklist สำหรับผู้ implement

รายการนี้เป็น checklist เพื่อไปตรวจ **ไม่ใช่ข้อกล่าวหาว่าทุก control ยังเสียอยู่ใน source ปัจจุบัน**:

| พื้นที่ | สิ่งที่ต้องไล่ | Disposition เป้าหมาย |
|---|---|---|
| AppBar / rail / tabs | Home, Analytics, Watchlist, Portfolio, Options/Lab, search, bell, settings, shortcuts | real route/action หรือ alias ที่สื่อความหมาย; ซ่อน/disable ตัวที่ไม่มี capability |
| Sidebar | add symbol, rename/delete/reorder, selected ticker, filters, expiry, budget | CRUD จริงและ scan แบบทีละหุ้น; selection ไม่หายหรือปน route |
| Market chart | timeframe, range, MA/VOL/MACD/RSI, draw/expand, tooltip | data-backed chart; unimplemented tools ไม่ enabled |
| Monitor / Chain | sort, selected contract, opportunity card, empty table, stale quotes | regression parity; no fake chain/grade; source labels ตรง |
| Lab | freeze, history, capture status, run controls, report/export | แยก engine readiness/empirical readiness; ไม่มี route placeholder ที่ดูเหมือนผลทดลองจริง |
| Account / Portfolio | account selector, balance, positions, refresh, symbol/option link | read-only, empty/error ต่างกัน; ไม่มี Buy/Sell/Close/Roll action |
| Assistant | page context, loading/error, links, quick prompts | tools ที่มีข้อมูลรองรับเท่านั้น; ไม่ส่ง account payload อัตโนมัติหรือแต่ง company fundamentals |
| Setup / footer / dialogs | token reconnect, source indicator, save/cancel/reset/export, keyboard escape | handlers ครบ; no secret output; status อิงข้อมูลจริง |

Audit artifact แต่ละ row ต้องมี: route + control label + source path/handler + current behavior + target behavior + task ID + test/manual evidence + residual limitation ผู้ implement ปิด ledger ไม่ใช่เพียงแนบ screenshot ของหน้าแรก

---

## 6. Dependency / ลำดับส่งมอบ

`P14 → P15 → P16 → P17`; P18 เริ่มหลัง P14 schema gates และ P15 scheduler พร้อม; P19 ปิด audit cross-page; P20 เป็น integration release gate ไม่ต้องรอ balance/depth ถ้า capability ยังไม่ผ่านและ UI ปิดอย่างซื่อสัตย์

- **รอบใช้งานแรก:** stock chart จริง + profile names + watchlist CRUD/polling + Home
- **รอบบัญชี:** accounts/positions read-only; balance เพิ่มเมื่อ exact request ผ่าน; nonempty schema tests เป็น gate แม้บัญชีจริงว่าง
- **รอบครบรายละเอียด:** depth ถ้าสิทธิ์/params ผ่าน, ปิด dead controls และ integration hardening

ไม่กำหนดเวลาเสร็จจากจำนวน endpoint อย่างเดียว: P14 ต้องยืนยัน batch limits, account schema, session/adjustment และ audit size ก่อนประเมินงาน ส่วนที่ API ตอบไม่ได้ไม่แก้ด้วยการดึง vendor อื่นหรือเพิ่ม placeholder

## 7. ความเสี่ยงและข้อจำกัดที่ต้องติดตาม

| ลำดับ | ความเสี่ยง | การควบคุม / ผู้รับผิดชอบ |
|---|---|---|
| 1 | quota ร่วม app/recorder และ multi-symbol UI | P15.1 cross-process scheduler; P20 soak; ไม่เพิ่ม polling loops ตามจำนวน page |
| 2 | empty positions ทำให้คิดว่า nonempty portfolio ผ่านแล้ว | P14 schema fixtures + P18 signed quantity/currency/deliverable tests; live nonempty validation ติดป้ายค้างจนมีข้อมูลจริง |
| 3 | สับสน snapshot ปัจจุบัน, daily bar label และ post-close capture | P15 time/session metadata + P17 recorder health; holiday/partial session limitation เปิดเผยและไม่ใช้ calendar-day heuristic รับรอง close |
| 4 | ค่า entitlement ถูกตีความว่า API ทุกอย่างใช้ได้ | capability registry และ targeted probes; depth/balance เป็น conditional ไม่อ้าง fundamentals ที่ไม่มี |
| 5 | scope ขยายเป็น trading terminal ส่ง order ได้ | P18/P20 read-only allowlist; ไม่มี order UI; read-only portfolio ไม่ยกเลิก non-goal order execution |

**เกณฑ์สำเร็จ:** ผู้ใช้เปิด Home แล้วเลือกหุ้นจาก watchlist ดูกราฟและข้อมูลจริง ไปคัดสัญญา/ทดลองใน Lab และตรวจพอร์ตบัญชีได้ใน shell เดิม ทุกค่าบอกแหล่งที่มาและเวลา ทุก action ที่เปิดให้กดทำงานจริง และข้อมูลที่ API ยังให้ไม่ได้แสดงข้อจำกัดอย่างตรงไปตรงมา

---

## Appendix A — ผล probe รอบ 2 (2026-11-23, P14.2 บางส่วน)

| Capability | สถานะ | หลักฐาน |
|---|---|---|
| Stock bars D | **verified** | `/market-data/stocks/bars/list` POST · NVDA 1,200 แท่ง 2022-02-10→2026-11-20 (~4.8 ปี) · M5×100 ผ่าน |
| Accounts | **verified** | `/trading/accounts/list` 200 · ได้ account_id |
| Positions | **verified (empty-live)** | `/trading/assets/positions/list` 200 · list ว่าง · schema ตาม docs |
| **Balance** | **verified (รอบนี้แก้ path แล้ว)** | **`/trading/assets/balances/get`?account_id 200** · keys จริง: total_cash_balance, total_market_value, total_unrealized_profit_loss, account_currency_assets (ต่อสกุล: cash/market_value/buying_power/unrealized) |
| Instrument | **verified** | `/trading/instruments/stocks/profiles/list` 200 · ชื่อจริง NVIDIA CORPORATION / SPDR |
| Depths | **gated — unresolved** | `depth=10`→417 · `L2`→400 · `5`→417 (แม่ใส่ overnight_required แล้ว) — เปิดตาม P19.1 เมื่อรู้รูป param จริง |
| Fundamentals | **unavailable** | API ไม่มี sector/market cap/PE — แผนไม่ปลอมข้อมูลส่วนนี้ |

รายงานเต็ม: `tmp/codex-consult/p10-probe-apis.json` (sanitized) · probe tool: `src-tauri/examples/probe_apis.rs`
