# PLAN — PREMIUM RADAR: Option Lab

**วันที่:** 2026-11-22 · **สถานะ:** ข้อเสนอแผน Phase 8–13; ยังไม่ได้ implement หรือ probe API จริง  
**อ้างอิง:** `tmp/codex-consult/BRIEF.md` · `PRODUCT.md` · `PLAN.md` · `design/DESIGN.md` · ภาพ img1/img2/img3 ที่แนบ  
**ขอบเขตงานรอบนี้:** เขียนแผนนี้เท่านั้น ไม่แก้ product truth, UI, source หรือ settings

---

## 0. ภาพรวม

**Option Lab คือพื้นที่ทดลองว่า “เลือกสัญญาแบบไหน คุ้มกับความเสี่ยงและเงินที่ต้องผูกไว้ที่สุด”** สำหรับผู้ขาย CSP / Covered Call ผ่าน Webull Thailand ให้เจ้าของตรวจสัญญาก่อนตัดสินใจ บันทึกคำคาดการณ์ ทดลองกติกาเลือก strike / DTE / exit และกลับมาดูว่าผลจริงสอดคล้องกับที่คาดเพียงใด

“แม่นยำ” หมายถึงโอกาสที่ทำนายสอดคล้องกับผลที่สังเกตได้ภายใต้นิยามเดียวกัน ไม่ใช่แค่ชนะบ่อย ส่วน “ผลตอบแทนสูงสุด” หมายถึงผลตอบแทนสุทธิที่ดีที่สุด **ในชุดกติกา ช่วงข้อมูล และกรอบความเสี่ยงที่ทดสอบ** ไม่ใช่คำรับประกัน หรือการเลือก annualized premium สูงสุด

**เส้นทางใช้งาน:** เลือกสัญญาจาก Radar → เปิด Option Lab → ดู payoff และสมมติฐาน → บันทึกชุดทดลอง → เปรียบเทียบกับกติกาปัจจุบัน → ตรวจผลนอกช่วงปรับแต่ง → บันทึก preset สำหรับนำไปเลือกใช้เองใน Radar

**MVP:** P8–P9 ให้ตรวจสัญญาและเริ่มสะสมข้อมูลได้ทันที; P10 ตรวจความแม่นยำเมื่อมี outcome เพียงพอ; P11 replay ตามข้อมูลที่พิสูจน์ว่ามี; P12 เปรียบเทียบกติกา; P13 จำลองเงินทุนและส่งมอบ ห้ามรอ historical backtest สมบูรณ์จึงเริ่มสร้างประโยชน์

### สิ่งที่เรียนรู้จากภาพ

| ภาพ | สิ่งที่เห็นและนำมาใช้ | ช่องว่างที่ Lab ต้องเติม |
|---|---|---|
| img1 — reference ภายนอก | dashboard 6 กราฟ: จัดอันดับสัญญา, เส้นโอกาสตาม strike, scatter, heatmap ตาม strike/expiry และ payoff ที่มีจุดหัก | ทำให้เห็น trade-off ข้ามสัญญาและเงื่อนไขกำไร; ภาพไม่ได้พิสูจน์สูตรหรือความแม่นยำ จึงใช้เป็นแรงบันดาลใจเท่านั้น |
| img2 — แอป Tkinter ของเรา | sidebar กรอง, ตาราง scanner หนาแน่น, ข้อความอธิบายสัญญาด้านขวา, scatter ความเสี่ยง/ผลตอบแทน และแถบเลือกมุมวิเคราะห์ | มีข้อมูลช่วยเลือกอยู่แล้ว แต่ยังไม่เห็นหลักฐานแบบบันทึกคำคาดการณ์แล้วเทียบผลย้อนหลัง |
| img3 — แอปของเราอีกมุม | โครง scanner เดิม เปลี่ยนกราฟด้านขวาเป็นเส้นและแท่งตามอายุสัญญา | สนับสนุนการเปลี่ยนมุมวิเคราะห์โดยรักษาบริบทสัญญา; ไม่ต้องเปิดทุกกราฟพร้อมกัน |

ตัวหนังสือเล็กในภาพอ่านได้ไม่ครบ จึงไม่ถอดสูตร ชื่อแกน หรือค่าตัวเลขที่คลุมเครือมาเป็น requirement ภาพ Tkinter เป็นหลักฐาน workflow; **V2 ใน DESIGN.md เป็น visual authority** ไม่คัดลอกพื้นกราฟขาวของ img3 หรือกริด 3×2 ของ img1 มาทับหน้าปัจจุบัน

### ข้อกำหนดกลาง

1. Tauri 2; Rust ทำ financial math, aggregation, calibration, simulation และสร้าง chart series ทั้งหมด React/TypeScript + ECharts รับ payload แล้ว format/render เท่านั้น
2. ข้อมูลตลาดมาจาก Webull TH OpenAPI เท่านั้น ไม่มี vendor สำรอง ไม่มีสร้าง historical quotes จากข้อมูลปัจจุบันแล้วอ้างว่าเป็นของจริง
3. Keys/token อยู่ Rust + OS keyring; ไม่อยู่ frontend, log, dataset export, chat หรือ git; auth ใช้ adapter เดิม
4. เพิ่มหน้า **Option Lab** ผ่านตำแหน่ง Strategy Lab ใน main tabs เดิม รักษา shell 4 คอลัมน์และหน้า Options Monitor ทั้งหมด ข้อเสนอนี้เป็นขอบเขตหน้าใหม่หลัง v1 ไม่ใช่อนุญาตให้รื้อ layout
5. ไม่มีส่งคำสั่งซื้อขาย ไม่มีเชื่อมบัญชีถือครอง ไม่มี auto-apply กลยุทธ์; portfolio ใน P13 เป็นสมุดจำลองเท่านั้น
6. การคำนวณเดิมและ golden tests ยังต้องผ่าน; Lab ใช้ model version แยก ไม่แก้คะแนน Radar เดิมโดยเงียบ ๆ
7. Brief ให้ข้อเท็จจริง production ใหม่กว่าแผนเดิม: MQTT v3.1.1 TLS :1883, protobuf, reconnect แล้ว HTTP resubscribe; ไม่ยึด WSS :8883 ใน PLAN เดิม และไม่สร้าง connection เพิ่มเพื่อ Lab โดยไม่จำเป็น

**ข้อจำกัดการตรวจ source รอบนี้:** CodeGraph ตอบว่าโครงการยังไม่ initialized แม้ระบุ projectPath แล้ว จึงไม่ได้ตรวจ symbol/source ผ่าน index และไม่ได้สร้าง index เพราะอยู่นอก write scope รายชื่อไฟล์ใหม่ด้านล่างเป็น proposed boundaries; P8.1 ต้องยืนยันจุดเชื่อมจริงก่อน implement ไม่อ้างว่าโมดูลเหล่านี้มีอยู่แล้ว

---

## 1. Data feasibility — ต้องรู้ก่อนเชื่อผลทดลอง

| แหล่งข้อมูล | ใช้ทำอะไรได้ตาม brief | สิ่งที่ยังสรุปไม่ได้ / gate |
|---|---|---|
| `/market-data/stock/snapshot` | spot ณ เวลารับข้อมูล; บันทึก forward ได้ | ไม่ใช่ประวัติหุ้นย้อนหลัง ไม่ใช่ราคาปิดทางการทุกวันที่ผ่านไป |
| `/market-data/option/snapshot` | quotes, Greeks, OI, volume ปัจจุบัน; probability/payoff และ snapshot archive | ไม่ได้แปลว่ามี historical bid/ask, IV, Greeks, OI |
| `/market-data/option/bars` | มี endpoint historical option bars; เป็นจุดเริ่ม probe | ต้องยืนยันช่วงย้อนหลังจริง interval, pagination, timestamp, timezone, price meaning, volume และ expired contracts; OHLC ไม่ใช่ bid/ask และไม่บอกลำดับแตะราคาในแท่ง |
| `/instrument/option/contracts` | discovery ปัจจุบันตาม 7 DTE windows และ strike band ±30% | ยังไม่ทราบว่าค้น expired/delisted/adjusted contracts ได้หรือไม่; cap 12 expiries × 20 strikes/side ทำให้ universe ไม่ครบ |
| MQTT snapshots | เก็บ quote ที่รับจริงของชุด subscribe เดิมได้ | ไม่ใช่ historical replay service; 96 สัญญาที่เลือกมี selection bias; ขาดช่วงเมื่อแอปปิด/เน็ตหลุด |
| auth + subscribe endpoints | ใช้ lifecycle เดิมและสิทธิ์ที่มี | ไม่เพิ่มความลึกข้อมูลย้อนหลัง; TTL token ราว 15 วันไม่ใช่สัญญาว่าคงที่ |

**Probe matrix P8.2:** NVDA และ SPY; Put/Call; ATM/OTM; active และ expired 1/7/30/90/365 วันเท่าที่ค้นหา ID จริงได้; query lookback 1/7/30/90/365 วัน; ทดสอบเฉพาะ interval และ pagination ที่ TH schema รองรับจริง ห้ามเดาว่าพารามิเตอร์ global API ใช้ได้ เก็บ earliest/latest/count/coverage/latency/status/entitlement พร้อมเวลา probe และ request parameters ที่ตัดข้อมูลลับออก หากไม่มี ID expired ให้รายงานว่า “ค้นหาไม่ได้” แยกจาก “bars ไม่มี”

ตรวจวันหยุด, DST, no-trade vs missing, duplicate/out-of-order, string prices, zero volume, crossed/stale quotes และ page overlap แยกการตอบ empty เพราะ entitlement/token/parameter ออกจากไม่มีประวัติ ใช้ shared throttle 1 call/1.1s, TTL cache 3s และ 429 backoff เดิม ให้ live traffic สำคัญกว่า backfill; จำกัดจำนวน request และยกเลิกงานได้

**Historical underlying, dividends, corporate actions และ exchange calendar:** brief ยังไม่ยืนยันแหล่งข้อมูลที่เพียงพอ ต้องตรวจความสามารถ Webull TH ก่อนใช้ ห้ามสมมติว่ามี stock-bars endpoint พร้อมแล้ว; อนุญาต manual scenario inputs ที่ติดป้ายสมมติฐาน แต่ไม่เปลี่ยนให้เป็นหลักฐานย้อนหลัง

### ผล probe ต้องเลือกความสามารถเป็นระดับ

- **A — Historical strategy replay:** มี universe ณ เวลานั้น รวม expired contracts, underlying synchronized, option quotes/ข้อมูลประเมิน fill, metadata และข้อมูลที่ filter ต้องใช้ครบตามช่วงที่รายงาน จึงจำลอง scanner และการจัดอันดับย้อนหลังได้; spread/OI ที่ไม่มี ณ วันเก่าห้ามแทนด้วยค่าปัจจุบัน
- **B — Limited contract replay:** มีเพียง bars ของสัญญาบางตัว ใช้ดู path/ถือถึงหมดอายุภายใต้ข้อมูลที่มี พร้อมราคาซื้อขายสมมติและขอบเขตความคลาดเคลื่อน; ห้ามเรียกว่า backtest scanner ทั้งตลาดหรือใช้รับรอง delta-band strategy
- **C — Forward capture:** ประวัติไม่พอ ให้เก็บ universe + quotes + prediction ตั้งแต่เริ่มใช้ แล้ววัดผลเมื่อครบเวลา; ไม่มีข้อมูลปลอมเติมอดีต

ระดับ A ไม่ใช่คำรับประกัน execution จริง ทุก run ระบุระดับหลักฐานและ fill model แยกกัน หากไม่ผ่าน A ยังส่งมอบ P9/P10 และ P11 สำหรับช่วง forward archive ที่ผ่านเงื่อนไขได้

---

## 2. Architecture และ data contract ที่เสนอ

ใช้ `crates/radar-core/src/lab/` สำหรับ pure math/replay และเรียก Webull/quant services ที่มีจริงหลัง P8.1 ยืนยัน; Tauri adapter จัด job, persistence และ DTO; frontend อยู่ `src/components/OptionLab/` โดยไม่ย้าย component เดิม

| Proposed file / boundary | หน้าที่ |
|---|---|
| `crates/radar-core/src/lab/types.rs` | DatasetManifest, Observation, Prediction, Outcome, ExperimentSpec, RunReport; versioned schemas |
| `lab/quality.rs`, `lab/capture.rs` | ตรวจข้อมูล, as-of timestamps, universe membership และ append-only capture |
| `lab/payoff.rs`, `lab/probability.rs` | CSP/CC economics, model assumptions, probability และ chart series |
| `lab/calibration.rs` | จับ prediction/outcome, Brier score, reliability bins, uncertainty |
| `lab/replay.rs`, `lab/execution.rs` | event-time loop, decision/fill/expiry policy, costs และ missing-data rules |
| `lab/compare.rs`, `lab/portfolio.rs` | walk-forward, parameter sweeps, risk constraints, cash/shares ledger |
| `crates/radar-core/tests/lab_*.rs` | deterministic fixtures, invariants, replay และ leakage tests |
| Tauri command/storage adapter (path ยืนยัน P8.1) | local SQLite ใน app data, migration, jobs/cancel/progress, redacted export |
| `src/components/OptionLab/`, `src/state/lab.ts` | หน้าใหม่และ immutable run selection; format/render ไม่มี financial formulas |

**ข้อมูลขั้นต่ำ:**

- Observation: contract ID/OCC, multiplier/deliverable, underlying, expiry timestamp, event time + received time UTC, source/freshness, bid/ask/trade/bar kind, currency, nullable Greeks/IV/OI/volume, universe ID, quality flags ไม่ใช้ 0 แทน missing
- Prediction: immutable ID, contract, timestamp, horizon, event definition, raw POP, model/version/inputs, cost/entry assumptions, snapshot hash; calibrated POP ถ้ามีเก็บแยก พร้อม training cutoff
- Outcome: prediction ID, resolved/censored/unresolved, endpoint price และ source, outcome event, net P/L ภายใต้ execution model; modeled assignment แยกจาก observed assignment ซึ่งระบบยังไม่มีข้อมูลบัญชี
- ExperimentSpec: frozen universe/dataset hash, date range, strategy parameters, decision time, exit precedence, fill/cost policy, capital/risk limits, model version และ deterministic seed
- RunReport: ID, provenance, exclusions/missing counts, trade ledger, metrics/CI, charts จาก Rust, train/test bounds และ warnings; เปลี่ยน spec ต้องเป็น run ใหม่

**Interface ที่เสนอ:** `analyze_contract(input) -> Result<ContractAnalysis>`, `evaluate_predictions(predictions, outcomes) -> CalibrationReport`, `run_experiment(spec, dataset, cancellation) -> Result<RunReport>`; async bridge `lab_analyze`, `lab_start_run -> job_id`, `lab_cancel(job_id)`, `lab_get_report(run_id)` และ event `lab-progress` ซึ่งมี job/generation ID ป้องกันข้อมูลเก่าปน งาน CPU หนักใช้ worker ฝั่ง Rust ไม่บล็อก live event pump

---

## Phase 8 — Data proof + experiment foundation

| # | Task | ไฟล์ / งาน | DOD |
|---|---|---|---|
| 8.1 | ยืนยัน integration map และ baseline | mapping ในเอกสาร implementation; core/bridge ที่มีจริง | ระบุชื่อ symbol/path ของ quant, bars, throttle, keyring, stream store และ command registration; บันทึกผล regression baseline ตาม test suites จริง แยกจากจำนวนใน brief; ไม่มีการแก้สูตรเดิม |
| 8.2 | Probe Webull historical feasibility | Rust probe ผ่าน adapter เดิม + sanitized report | ครบ matrix §1 หรือมีเหตุผลทุกช่องที่ probe ไม่ได้; ระบุ earliest/latest/granularity/expired coverage และเลือก A/B/C ด้วยหลักฐาน; ไม่มี secrets ใน report; throttle/cancel/429 ตรวจได้; ไม่อ้างจำนวนวันย้อนหลังที่ไม่ได้วัด |
| 8.3 | Dataset manifest + quality checks | `lab/types.rs`, `quality.rs` | fixtures missing/duplicate/string prices/DST/out-of-order ให้ผล deterministic; ทุก observation มี event/received time; run ปฏิเสธ field สำคัญที่ขาดพร้อมเหตุผลไทย; gap ไม่ถูก forward-fill เงียบ ๆ |
| 8.4 | Forward recorder + local storage | `capture.rs`, storage adapter | เก็บ universe ก่อนคัดกรองและ subscribed subset พร้อม caps; restart แล้วไม่ซ้ำ; outage แสดง coverage gap; retention/export/delete ทำงานเฉพาะ Lab data; export schema ไม่มี credentials; app ปิดแล้วไม่แสร้งว่าบันทึกต่อ |

**Phase DOD:** มีรายงานว่าทำ replay แบบใดได้จริง พร้อม dataset ตัวอย่างที่ตรวจ provenance ได้ P11 ถูก gate ตามผล ไม่ gate P9 ด้วยความหวังว่าจะซื้อข้อมูลเพิ่ม

**ความเสี่ยง:** option bars มีอยู่แต่ไม่พอสร้าง historical screener — fallback เป็น forward capture ตั้งแต่ phase นี้

---

## Phase 9 — Contract Lab: เข้าใจสัญญาและ trade-off

| # | Task | ไฟล์ / งาน | DOD |
|---|---|---|---|
| 9.1 | Payoff และทุน CSP/CC | `payoff.rs`, `probability.rs` | tests ต่ำกว่า/ตรง/สูงกว่า strike และ BE; fees, multiplier, missing basis, zero time, invalid IV; CC option leg/combined แยกชัด; invalid input ไม่ออก NaN/Infinity; nonstandard deliverable ที่ยังไม่รองรับถูก block |
| 9.2 | Strike/expiry analytics | pure Rust series + analysis DTO | probability curve, premium heatmap, risk-return scatter ผูก snapshot เดียวกัน; missing cell เป็น “ไม่มีข้อมูล”; chart/table ใช้ค่าชุดเดียวกัน; รูปแบบ premium $, yield % และ annualized มี denominator ทุกแบบ |
| 9.3 | หน้า Option Lab ใน shell เดิม | OptionLab + main-tab binding | เปิดจาก selected contract ได้; shell/Monitor ไม่เปลี่ยน; chart เด่นทีละมุม; detail ตาม contract เดียวกัน; keyboard เลือก heatmap/table ได้; breakpoint checks ตาม §5 ผ่าน |
| 9.4 | Freeze hypothesis | Prediction/ExperimentSpec persistence | บันทึก POP/event/horizon/entry assumptions ณ ตอนตัดสินใจแล้วแก้ย้อนหลังไม่ได้; quote stale มีป้ายและไม่บันทึกเป็น live; เปิดบันทึกกลับมาแล้วตัวเลขไม่ไหลตาม quote ปัจจุบัน |

**Phase DOD:** ผู้ใช้เลือก CSP และ CC อย่างละหนึ่งสัญญา อธิบายเป็นไทยได้ว่าได้ premium เท่าไร ใช้ทุนอะไร ขาดทุนเมื่อใด และ POP วัดเหตุการณ์อะไร; จับค่ากราฟกับ Rust tests ได้ทุกจุดสำคัญ

---

## Phase 10 — Accuracy journal: ทำนายไว้แล้วตรงเพียงใด

| # | Task | ไฟล์ / งาน | DOD |
|---|---|---|---|
| 10.1 | Outcome resolver | `calibration.rs`, outcome storage | จับผลด้วย event/horizon เดียวกับ prediction; expired missing price = unresolved; ไม่เอา early-exit win ไปเทียบ expiry POP; ไม่อ้างว่า ITM เป็น actual assignment; fixture ทุกสถานะผ่าน |
| 10.2 | Reliability report | `calibration.rs`, calibration view | แสดง Brier, fixed bins, mean predicted vs realized, n, unresolved count และ CI; hand-calculated fixture ให้ค่าตรง; duplicate snapshot ไม่เพิ่ม independent trade count; filtered report ยังเห็น exclusion count |
| 10.3 | Forward protocol + optional recalibration | model version + report | แยก raw model กับ calibrated estimate; fitter ใช้ training เท่านั้น; ทดสอบย้าย outcome ในอนาคตแล้ว prediction เก่าไม่เปลี่ยน; insufficient sample แสดง “ยังสรุปไม่ได้”; ไม่มีแก้ Radar score อัตโนมัติ |

**Phase DOD:** เปิด prediction เก่า → outcome → จุดบน calibration curve ย้อนตรวจข้อมูลได้ครบ การส่ง phase ไม่ขึ้นกับว่ากลยุทธ์ต้องชนะหรือโมเดลต้องดีขึ้น

---

## Phase 11 — Replay ที่ตรวจสอบได้

| # | Task | ไฟล์ / งาน | DOD |
|---|---|---|---|
| 11.1 | Event-time replay + universe reconstruction | `replay.rs` | decision ใช้เฉพาะข้อมูลที่ available ณ เวลานั้น; fill ไม่เกิดบนราคาก่อนตัดสินใจ; fixtures future IV/OI และ expired universe จับ look-ahead/survivorship ได้; report แยก A/B/C ชัด |
| 11.2 | Execution + costs + exit rules | `execution.rs` | bid-entry/ask-exit สำหรับ quote data; bar-only ใช้ assumed fill ติดป้าย; ไม่มี volume/liquidity พอ = no-fill ตาม policy; ทดสอบ target/stop ในแท่งเดียว, gap, stale, fee และ expiry; pessimistic ordering ไม่เลือกลำดับที่กำไรสูงสุด |
| 11.3 | Assignment และ position ledger | `replay.rs`, `portfolio.rs` | put assignment แลก cash กับหุ้น, CC call-away ส่งหุ้นและรับ strike cash; premium ไม่บวกซ้ำ; early assignment เป็น stress scenario; ทดสอบ ledger conservation และ stock loss หลัง assignment |
| 11.4 | Audit report + cancellation | bridge jobs + replay view | rerun dataset/spec/seed เดิมให้ ledger และ metrics เดิม; export ระบุ excluded trades/coverage/fill model; cancel แล้ว report เป็น incomplete และไม่ติดอันดับ; live scanner ยังรับข้อมูลได้ |

**Phase DOD:** มี replay fixture ที่ตรวจมือได้ครบตั้งแต่ entry ถึง cash/shares สุดท้าย และหนึ่ง run บน Webull data เท่าที่ gate อนุญาต ถ้าข้อมูลไม่พอให้ส่งสถานะ blocked พร้อมเหตุผลแทนกราฟผลตอบแทนสมมติที่ดูเหมือน backtest จริง

---

## Phase 12 — เปรียบเทียบกติกาโดยไม่หลอกตัวเอง

| # | Task | ไฟล์ / งาน | DOD |
|---|---|---|---|
| 12.1 | Frozen baseline + bounded sweep | `compare.rs`, experiment controls | baseline = filters/score เดิมที่ freeze; proposed grid ใช้ delta bands เดิม × DTE 7–14/15–30/31–60 × hold-to-expiry/close-at-50%-credit; 18 configurations ก่อน variants อื่น; ระบุ tie-break และหนึ่งตำแหน่งต่อ ticker โดย default; ไม่มี runtime เปลี่ยน baseline |
| 12.2 | Walk-forward validation | `compare.rs` | chronological train/validation/test; purge/embargo อย่างน้อยตาม holding horizon ที่ทับขอบ; test set ไม่ใช้เลือก parameter; leakage fixtures ผ่าน; ทุก config รวมผู้แพ้ปรากฏ ไม่รายงานเฉพาะ best run |
| 12.3 | Risk-constrained comparison | Rust metrics + comparison table | เทียบทุน/เวลา/universe/cost เดียวกัน; แสดง net return, drawdown, tail loss, utilization, assignment scenario และ uncertainty; fail risk limit แล้วไม่อยู่ eligible ranking แม้ premium สูง; แสดง Pareto alternatives เมื่อไม่มีผู้ชนะชัด |
| 12.4 | Preset handoff | versioned preset + Radar adapter | บันทึก preset พร้อม evidence/run ID; ผู้ใช้เลือก apply เอง; unsupported filters แจ้งชัด; ย้อนกลับ preset เดิมได้; ไม่มีส่ง order และไม่มี auto-update ตามผล sweep |

**Phase DOD:** ตอบได้ว่า preset ใดดีกว่า baseline ในช่วงทดสอบใด พร้อมผลเสียและความไม่แน่นอน ถ้า out-of-sample ไม่ดีขึ้นให้สรุปตามจริง ไม่เปลี่ยนเกณฑ์จนได้ผู้ชนะ

---

## Phase 13 — Capital simulation + hardening + handoff

| # | Task | ไฟล์ / งาน | DOD |
|---|---|---|---|
| 13.1 | Cash-secured / covered capital simulator | `portfolio.rs` | reserve K×multiplier ต่อ CSP; CC ต้องมีหุ้นพอและไม่ถูก reserve ซ้ำ; ไม่มี margin/naked call; เปิดซ้ำไม่ได้ถ้าเงิน/หุ้นไม่พอ; re-invest เฉพาะเงินที่ available ตาม settlement policy ที่ระบุ; integer contracts |
| 13.2 | Equity curve + stress + benchmarks | Rust report + UI | equity รวม cash + หุ้น mark-to-market − liability option; assigned/unrealized loss ไม่หาย; เทียบ cash และ buy-and-hold เมื่อมีข้อมูลรองรับ; gap/IV spike/early call-away/fee stress ติดป้าย scenario; ข้อมูล marking ขาดแสดงช่วงไม่ทราบ ไม่สร้าง drawdown เทียม |
| 13.3 | Operational/security/accessibility gates | core/bridge/UI tests | schema migration, corrupted storage, token expiry, 429, disconnect, cancel/restart ผ่าน; ไม่มี key/token ใน exports/log/frontend; keyboard/focus/Thai text/contrast ผ่าน; screenshot checks wide/1366/1024/≤790/short viewport |
| 13.4 | Regression + Thai handoff | test suites + คู่มือ Lab | golden suites เดิมทั้งหมดและ Lab tests ผ่าน; `cargo fmt --check`, clippy, Rust tests, frontend check/build ตาม workspace จริง; installer smoke Windows/macOS; คู่มืออธิบายระดับหลักฐานและ run ทำซ้ำได้; ไม่มีอ้างผลตอบแทนรับประกัน |

**Phase DOD:** สร้าง experiment → รัน/ยกเลิก → เปิดรายงานเก่า → ตรวจ ledger → export sanitized report ได้บนเครื่องติดตั้งจริง โดยหน้า Radar และ key lifecycle เดิมยังทำงาน

---

## 3. Accuracy & math contract

### 3.1 นิยาม payoff และ denominator

ให้ `m` เป็น multiplier ที่ยืนยันแล้ว (รองรับ standard 100 ก่อน), `c` premium ต่อหุ้น, `F` ค่าใช้จ่ายรวมของเส้นทางนั้น, `K` strike, `S_T` ราคาหุ้นเมื่อหมดอายุ และ `B` cost basis ต่อหุ้น:

- CSP ถือถึง expiry: `P/L = m × (c − max(K − S_T, 0)) − F`; BE = `K − c + F/m`; collateral denominator = `mK` แบบ gross reserve ไม่ลดด้วย premium เพื่อทำ yield ให้ดูสูง
- Short call leg: `P/L = m × (c − max(S_T − K, 0)) − F`; BE ของขา option = `K + c − F/m`
- Covered call รวมหุ้น: `P/L = m × (S_T − B + c − max(S_T − K, 0)) − F`; ในกรณีปกติ BE = `B − c + F/m` และ max profit = `m(K − B + c) − F`; ค่าติดลบเป็นไปได้ ถ้าต้นทุนสูงกว่าราคาขายสุทธิ ต้องแสดงตามจริง
- CC เปรียบเทียบการใช้ทุนวันนี้ใช้ stock market value ณ entry (`mS_0`); cost-basis P/L แสดงอีกคอลัมน์ ไม่เอาต้นทุนเก่าที่ต่ำมาทำ yield ให้ดูเหนือ strategy อื่น ถ้าไม่มี B ไม่แสดง cost-basis combined result; อนุญาต “สมมติซื้อหุ้นวันนี้” โดยระบุ B = S0 ชัดเจน
- Payoff ข้างต้นเป็น expiry economics สำหรับ standard deliverable; dividends, taxes, financing/FX และ early-close costs แยกใน ledger ห้ามใช้กราฟ expiry แทนมูลค่าก่อนหมดอายุ

กรณี BE ไม่อยู่ใน payoff segment ที่เกี่ยวข้องหรือไม่มีช่วงทำกำไร ให้คำนวณ event จาก payoff inequality จริง ไม่บังคับใช้สูตร BE อย่างเดียว ทดสอบ CC max profit ≤0 และ CSP net credit ≤0

### 3.2 Probability มีชื่อเหตุการณ์เสมอ

ใช้ BS เดิมเป็น baseline แบบ risk-neutral proxy โดยเปิดเผย spot/IV/r/q/T/ราคาเข้าและ cost assumptions ไม่เรียกว่า empirical probability ที่พิสูจน์แล้ว และไม่ถือว่าโมเดล European อธิบาย American early exercise ได้ครบ

- CSP expiry POP = `Pr(S_T > net BE)` ภายใต้สมมติฐานที่ระบุ; short-call-leg POP ใช้ `Pr(S_T < leg BE)`; CC combined POP ต้องแก้จาก combined payoff ไม่ใช้ short-call POP แทน
- Terminal ITM probability, net-profit probability และ touch probability เป็นคนละเหตุการณ์ ห้ามใช้ |delta| เป็น actual assignment rate; `p_touch ≈ 2p_itm` ที่มีเดิมเป็น approximation ต้อง clamp [0,1] และมีคำอธิบายข้อจำกัด ไม่ใช่ calibrated path probability
- หากต้องการ POP ของ “ปิดเมื่อได้ 50% credit” ต้องมี path model และ validation ของ event นี้โดยเฉพาะ ในรุ่นแรกแสดง empirical exit outcomes โดยไม่ยืม expiry POP มาใช้
- กราฟ strike curve แสดง probability ของแต่ละสัญญา/สมมติฐาน IV ของมัน ไม่เชื่อมเป็น terminal density เดียวโดยไม่อธิบาย skew; density จาก IV คงที่ให้ระบุว่าเป็น simplified scenario

### 3.3 วัดความแม่นยำ

`y_i = 1` เมื่อ event ตาม prediction เกิดจริง มิฉะนั้น 0; `Brier = mean((p_i − y_i)^2)` ยิ่งต่ำยิ่งดี ใช้ predictions ที่ matured/resolved เท่านั้น พร้อม coverage และจำนวน unresolved เสมอ

Reliability diagram ใช้ fixed bins 0–10%, …, 90–100% แสดงค่าเฉลี่ย p, observed event rate, n และ 95% Wilson interval; ECE เป็น diagnostic ประกอบ ไม่ใช่คะแนนเดียวสำหรับเลือกโมเดล แสดง Brier เทียบ base-rate predictor ที่เรียนจาก training เท่านั้น

สัญญาหุ้นเดียวกันวันเดียวกันสัมพันธ์กัน: แสดงทั้งจำนวนสัญญาและจำนวน entry-day/expiry clusters ใช้ block bootstrap ตามเวลา/กลุ่มสำหรับ uncertainty ของผลรวมและความต่างกลยุทธ์ ไม่อ้างว่า 96 สัญญาคือ 96 ตัวอย่างอิสระ

**เกณฑ์แสดงผลเบื้องต้นที่เสนอ:** bin ต่ำกว่า 30 resolved observations ติดป้ายตัวอย่างน้อย; ต่ำกว่า 100 resolved observations หรือ 20 entry-day clusters ยังไม่ประกาศว่า calibrated/กลยุทธ์ดีกว่า ตัวเลขนี้เป็น display guardrail ไม่ใช่หลักฐานทางสถิติว่าพอแล้ว; รายงาน CI และ market regimes ต่อไปแม้เกินเกณฑ์

### 3.4 เปรียบเทียบผลตอบแทน

รักษา `annualized × POP` เป็น baseline heuristic ของ Radar **ไม่เรียกว่า expected return** เพราะไม่ได้ชั่งขนาดขาดทุน ตัวชี้วัด Lab หลักคือ net equity return ในช่วงเดียวกัน ควบคู่ maximum drawdown, worst trade, average win/loss, empirical tail loss, assignment scenarios, capital utilization และเวลาถือ

กำหนด risk budget ก่อนรัน เช่น cash reserve, max position/concentration, drawdown ceiling และ liquidity/cost assumptions; กรองชุดที่ไม่ผ่านแล้วจัดอันดับ net return บน validation ก่อนเปิด test สำหรับข้อมูลสั้นแสดง period return และไม่ยก CAGR มาเป็น headline; simple annualized premium ติดป้ายว่าไม่ใช่ CAGR ไม่คูณทบต้นจากดีลเดียว

Costs เป็น versioned inputs: broker/exchange fees, entry/exit slippage, exercise/assignment fee และ stock disposal costs ตามที่เกี่ยวข้อง ต้องมีเจ้าของยืนยันหรือแสดงว่าไม่ทราบ; ห้าม default “net” โดยตั้งค่าที่ไม่ทราบเป็น 0 เงียบ ๆ USD เป็นสกุลหลัก; THB conversion/taxes แสดงเฉพาะเมื่อมี explicit assumptions และแยกออกจากผลตลาด USD

Bid/ask history ไม่พอให้ใช้ conservative scenario ranges ไม่แต่ง executable fill จาก mid; OI ไม่ใช่หลักฐานว่าขายได้จริง ผล backtest คือ simulated outcomes ไม่ใช่ trade P/L จากบัญชี

---

## 4. Protocol ป้องกันผลสวยเกินจริง

1. Freeze hypothesis, universe, entry schedule และ exit priority ก่อนรัน; overlapping delta bands เดิมเป็นคนละ config ไม่แกล้งทำว่าเป็นกลุ่มอิสระ
2. เก็บ as-of/received timestamps และ contract membership; bars ชุดใดใช้เวลาปิดแท่ง ต้องรอปิดแท่งแล้วจึงใช้; future corporate-action corrections มี provenance และไม่ให้ข้อมูลอนาคตเข้า feature
3. Stop/target เกิดในแท่งเดียวแต่ไม่รู้ลำดับให้ pessimistic outcome และ sensitivity report; gap ข้าม stop ใช้ราคาที่ทำได้ตาม model ไม่รับประกัน fill ที่ stop
4. American early assignment และ ex-dividend risk ทดสอบ stress แยก; หากไม่มีประวัติจริงไม่ fit assignment probability ปลอม เมื่อ put assigned ให้เลือก policy ล่วงหน้า: ถือหุ้น/ขายครั้งถัดไป/เข้าสู่ CC cycle และรับ loss ตาม mark-to-market
5. ปรับ parameter ใน train/validation เท่านั้น; purge overlapping holding windows; untouched test เปิดครั้งเดียวต่อ experiment family ถ้านำกลับไปปรับต้องถือว่าใช้หมดแล้วและเก็บ forward evidence ใหม่
6. รายงานทุก configuration/trial count, walk-forward folds, cost sensitivity และ regimes ที่มีจริง; ช่วงตลาดเดียวหรือ universe จำกัดต้องระบุ ผลที่ไม่ดีก็เป็นผลสำเร็จของ Lab ถ้าตรวจสอบได้

---

## 5. UI concept — หนึ่งคำถามต่อหนึ่งมุมมอง

**Mode: Operate.** ใช้ main tab ตำแหน่ง Strategy Lab เดิมแสดง “Option Lab · ทดลองกลยุทธ์” โดยรักษา shell: rail 50px / left 225px / main / right 320px, appbar 43px, status 25px; compact widths ตาม DESIGN.md ทุกค่า ไม่เพิ่มคอลัมน์หรือ floating analysis drawer

ภายในหน้า Lab รักษาลำดับพื้นที่เดิม: main tabs 34px → quote/context header ≥75px → chart + statistics 258px → summary band 88px → table ที่ scroll ภายใน ส่วนขวา Contract Analysis อยู่เหนือ Radar Assistant ตามเดิม การเปลี่ยนเนื้อหาทำเฉพาะหน้าใหม่; หน้าปัจจุบันและ scanner column order ไม่เปลี่ยน

| พื้นที่เดิม | เนื้อหา Lab |
|---|---|
| Left watchlist/filters | ticker และ strategy context เดิม; ค่าที่เปลี่ยนเป็น draft ไม่เปลี่ยน frozen run; advanced experiment controls อยู่ในพื้นที่ table module ของ Lab |
| Context header | ticker/contract, โหมด “วิเคราะห์ปัจจุบัน / ผลทดลองที่บันทึก”, as-of, dataset coverage และ run ID; quote ปัจจุบันไม่ติดป้ายเป็นราคาของ replay |
| Chart + stats | เลือก “กำไรขาดทุน / strike–expiry / ความเสี่ยง–ผลตอบแทน / ความแม่นยำ / ผลทดลอง”; ครั้งละหนึ่ง primary chart + ค่าสำคัญด้านข้าง |
| Summary band 3 ช่อง | ทุนที่ต้องใช้ / ผลตอบแทนสุทธิภายใต้สมมติฐาน / ความเสี่ยงหลัก; รายงานทดลองใช้ baseline / selected / difference พร้อม uncertainty แทน |
| Table module | contracts หรือ experiment runs ตามมุมมอง; sticky header, audit row, filter quality; เลือกแถวผูก chart/detail เดียวกัน |
| Right panel | สัญญา/สมมติฐาน/ข้อจำกัดก่อน แล้ว copilot อธิบายค่าจาก Rust; ถ้ายังไม่มี Lab tools ให้แจ้งว่า assistant ยังอ่าน run นี้ไม่ได้ ไม่ส่ง live numbers มาอธิบาย run เก่า |
| Status bar | feed status แยกจาก job status; progress เช่น “ประมวลผล 4/18 ชุด”; ปุ่มยกเลิกและข้อผิดพลาดไทย |

**Chart semantics:** payoff มี strike/BE/spot และหน่วย USD; heatmap แกน expiry × strike พร้อมตัวเลข/legend และช่อง missing; scatter แกนความเสี่ยงจาก model กับ net yield ที่ระบุฐานทุน สี/รูปทรงแยก Put/Call และ bubble size แสดง liquidity เฉพาะเมื่อมีข้อมูล; calibration ใช้เส้น ideal และ CI; equity curve มีช่วง gap มองเห็น ทุกกราฟมีตารางข้อมูลอ่านแทนได้

**Tokens:** canvas #0b0d11, surface #10141a/#171c24, divider #292f39 1px, text #f0f3f8, muted #a2adbc; blue #4b8cf7 สำหรับ selection/focus, green #24c99a / red #f4788c / amber #f0bd70 ตามความหมายทางการเงิน มี +/- และข้อความ ไม่ใช้สีอย่างเดียว ไม่ใช้ gradient/gloss/heavy shadow ใช้ Inter + Noto Sans Thai และ tabular numerals ตามระบบเดิม

**States:** ยังไม่มีข้อมูล → เริ่มบันทึก; มีข้อมูลแต่ไม่ครบ → ระบุ field/ช่วงที่ขาด; ตลาดปิด → ราคาปิด/as-of; quote stale → หยุดอ้าง live; probe/backfill → progress/cancel; ไม่มีสัญญาตาม filter → ล้าง selection/detail; sample น้อย → ยังสรุปไม่ได้; API error → retry ที่รักษา draft; run incomplete → ไม่เข้าตารางอันดับ

**Responsive DOD:** wide >1420 และ 1366 เห็น panel ตามเดิม; 1024 ซ่อนขวาที่ ≤1150 แต่ main table/รายละเอียดในมุม Lab ยังอ่านสมมติฐานสำคัญได้; Lab อยู่ตำแหน่ง tab ที่ไม่ถูกซ่อนด้วยกฎลำดับ 4+; ≤790 ซ่อน left/snapshot ตามเดิมและไม่อ้างว่าทำ workflow ครบเท่า desktop; short viewport scroll ภายใน ไม่ดันตารางหาย ไม่เพิ่ม mobile drawer

---

## 6. ลำดับส่งมอบและเกณฑ์ตัด scope

`P8 → P9 → P10` และ `P8 data gate → P11 → P12 → P13`; forward collection เริ่ม P8 แล้วสะสมระหว่างพัฒนา phase อื่น การรอ outcome จริงเป็นเวลาปฏิทิน แยกจากเวลาเขียนโค้ด

- ส่งรอบแรก: data report + Contract Lab + capture ที่ใช้จริง แม้ historical replay ไม่ผ่าน
- ส่งรอบสอง: calibration หลังมี matured observations; replay เฉพาะ universe/ช่วงที่รองรับ
- ส่งรอบสาม: parameter comparison และ capital simulation หลัง ledger/execution ผ่านก่อน
- ยังไม่ทำ: multi-ticker simultaneous streaming, strategy marketplace, optimizer อัตโนมัติไม่จำกัด grid, advanced American pricing engine, real-account portfolio sync, order placement

ทุก task ใช้วงจร fixture/acceptance → implementation → verify → review → commit แยกตามงาน เมื่อเข้าสู่ implementation ใช้ test-first กับ math/replay ที่เสี่ยง และใช้ screenshot/interaction checks กับ UI; ชื่อไฟล์เดิมและคำสั่ง workspace จริงต้องยืนยัน P8.1 ก่อนแตกเป็น implementation checklist รายไฟล์

## 7. Risks + คำถามเจ้าของ (เรียงลำดับความสำคัญ สูงสุด 5 ข้อ)

| # | ความเสี่ยง / สิ่งที่ต้องตัดสินใจ | คำถามเจ้าของ | ค่าเริ่มต้นของข้อเสนอ |
|---|---|---|---|
| 1 | “ผลตอบแทนสูงสุด” ไม่มีความหมายถ้าไม่จำกัด downside | ยอมรับ drawdown และการกระจุกในหุ้นตัวเดียวได้เท่าไร และทุนทดลอง USD เท่าไร? | ยังไม่ตั้งตัวเลขแทน; อนุญาต analyze แต่ก่อนจัดอันดับให้กรอก risk budget |
| 2 | ประวัติ Webull ไม่พอและ forward sample ต้องรอ | หาก probe ไม่ผ่าน A ยอมรับ limited replay + เริ่มเก็บ forward เป็นรุ่นแรกหรือไม่? | ส่ง MVP แบบ C พร้อมมุมวิเคราะห์ปัจจุบัน; ไม่ซื้อ vendor อื่น |
| 3 | Assignment ทำให้ premium profit กลายเป็น stock loss | เมื่อ CSP ถูก assigned ต้องการถือหุ้น ขายออก หรือทำ CC ต่อ และ CC ใช้ต้นทุนหุ้นใด? | รายงาน scenario แยกจนเลือก policy; ไม่ roll อัตโนมัติ |
| 4 | Fees/slippage/ภาษี/FX ทำให้ gross ranking ผิด | จะใช้ค่าธรรมเนียมและ slippage สมมติชุดใด และเน้นวัด USD หรือ THB? | USD; costs ที่ไม่ทราบติดป้าย ไม่อ้าง net ที่ครบ; sensitivity ก่อนข้อสรุป |
| 5 | ข้อมูลขาดช่วงและการเลือกดูเฉพาะผู้ชนะทำให้ผลลำเอียง | เปิดแอปเก็บข้อมูลช่วงตลาดได้สม่ำเสมอเพียงใด และยอม freeze กติกาก่อนดู test หรือไม่? | local capture เฉพาะแอปเปิด; append-only hypotheses, trial history และ coverage แสดงเสมอ |

---

**เกณฑ์สำเร็จของ Lab:** ผู้ใช้ตรวจได้ว่าสัญญาหรือกติกาหนึ่งคุ้มกว่าอีกแบบเพราะอะไร ใช้ข้อมูล/ทุน/ความเสี่ยงเท่าไร และส่วนไหนยังพิสูจน์ไม่ได้ โดยทุกตัวเลขย้อนกลับไปหา input และ run ที่สร้างมันได้

---

## Appendix A — ผลการยืนยัน P8.1 (2026-11-22, builder: ZCode)

> P8.1 DOD: ระบุชื่อ symbol/path ของ quant, bars, throttle, keyring, stream store และ command registration · บันทึกผล regression baseline ตาม test suites จริง · ไม่มีการแก้สูตรเดิม — **ผล: DONE**

### A.1 Integration map จริง (ยืนยันจากโค้ด ณ commit d46fd11)

| จุดเชื่อม | ตำแหน่งจริง | หมายเหตุสำหรับ Lab |
|---|---|---|
| Quant | `crates/radar-core/src/quant.rs` — `bs_price`, `implied_vol`, `bs_greeks` → `Greeks`, `ncdf`/`npdf`, `expiry_utc`, `years_to_expiry` | ห้ามแก้สูตร — Lab เรียกใช้อย่างเดียว |
| Enrich/grade/filter | `crates/radar-core/src/screener.rs` | baseline ของ P12 คือ filters/score ชุดนี้ |
| Engine/state store | `crates/radar-core/src/engine.rs` — `Engine`, `QuoteUpdate`, `StockUpdate`, `EngineSnapshot`, `set_symbols`, `push_quote`, `push_stock`, `seed_quote`, `snapshot` | stream store ของระบบ — Lab อ่านจาก `EngineSnapshot` ไม่เขียนตรง |
| REST transport | `crates/radar-core/src/webull/transport.rs` — `BASE_URL = https://api.webull.co.th`, signed GET + POST (`post_json`) | adapter เดียวที่ probe P8.2 และ Lab จะใช้ |
| Endpoints (8) | `webull/rest.rs` — `PATH_STOCK_SNAPSHOT`, `PATH_OPTION_SNAPSHOT`, `PATH_OPTION_BARS`, `PATH_OPTION_CONTRACTS`, `PATH_AUTH_TOKEN_CREATE`, `PATH_AUTH_TOKEN_CHECK`, `PATH_STREAMING_SUBSCRIBE`, `PATH_STREAMING_UNSUBSCRIBE` | ครบตาม §1 ของแผน |
| Throttle/cache/limits | `webull/rest.rs` — `MIN_INTERVAL_SEC=1.1`, `CACHE_TTL_SEC=3.0`, `MAX_ANALYZE=480`, `MAX_CONTRACTS=96`, `OI_BATCH=20`, `WINDOWS` (7 ช่วง DTE), `STRIKE_BAND=0.30`, `MAX_EXPIRIES=12`, `STRIKES_EACH_SIDE=20` | probe ต้องใช้ throttle เดียวกัน ห้ามเบี่ยง |
| Option bars fetch | `webull/quotes.rs` — `fetch_bars_seeds` + `bars_to_seeds` (ใช้ `PATH_OPTION_BARS`) | จุดตั้งต้นของ probe P8.2 |
| Contract discovery | `webull/discovery.rs` — วน `WINDOWS`, strike band ±30% ฟอร์แมต `{:.2}`, truncate 12×20/side | universe ปัจจุบันของระบบ |
| Auth/token | `webull/auth.rs` — `check_token` (ต้องไม่ส่ง token), `create_token` (SMS 2FA) | TTL ~15 วัน |
| MQTT stream | `webull/stream.rs` — `MQTT_PORT_TLS=1883`, `StreamConfig`/`for_base_url`, `StreamClient`, `route_message` (snapshot/echo/notice), `track_subscribe` (clean session → ต้อง resubscribe ผ่าน HTTP) | ตรงตามข้อกำหนดกลางข้อ 7 |
| Protobuf | `crates/radar-core/src/webull/pb/` | prost |
| Copilot | `crates/radar-core/src/copilot.rs` — `LlmConfig` (GLM/OpenAI) + function-calling tools | Lab tools เพิ่มภายหลัง ไม่แตะของเดิม |
| Keyring/settings | `src-tauri/src/settings.rs` — `save_with_keyring`, `move_secrets_to_keyring`, `pull_secrets_from_keyring` (OS keyring + settings.json fallback) | Lab export ต้องผ่าน sanitized schema เท่านั้น |
| Scan orchestration | `src-tauri/src/scan.rs` | งาน background จริงเป็นแพตเทิร์นให้ Lab jobs เลียนแบบ |
| Tauri commands (14) | `src-tauri/src/lib.rs` → `invoke_handler`: `get_snapshot`, `set_filters`, `start_scan`, `stop_scan`, `get_status`, `get_settings`, `save_settings`, `save_credentials`, `test_connection`, `create_access_token`, `needs_setup`, `ask_copilot`, `test_llm` | คำสั่ง Lab (`lab_*`) จะลงทะเบียนที่นี่ทีหลัง |
| Frontend shell | `src/App.tsx` + `src/components/*` (4-column shell ตาม DESIGN.md) | `OptionLab` เป็น component ใหม่ ไม่ย้ายของเดิม |

สรุป: ชื่อไฟล์ใหม่ทั้งหมดใน §2 ของแผน (`lab/*.rs`, `src/components/OptionLab/`) เป็นของใหม่จริง ไม่ชนกับของเดิม — proposed boundaries ใช้ได้ตามที่เขียน

### A.2 Regression baseline (บันทึก 2026-11-22)

| Gate | ผล |
|---|---|
| `cargo test --workspace` | **87 passed · 0 failed · 1 ignored** (ignored = live smoke ต้องมี `WEBULL_APP_KEY`/`WEBULL_APP_SECRET` ใน env) — แยก suite: premium_radar_lib 6 · radar_core 68 · golden_quant 3 · golden_screener 3 · golden_sign 1 · transport_wiremock 6 |
| `npm run check` (tsc --noEmit + eslint) | **ผ่าน — 0 errors · 4 warnings** (react-hooks/exhaustive-deps ที่ `src/App.tsx:202` — pre-existing ก่อน Lab ทั้งหมด) |

การกระจาย golden cases: ชุด 159 BS + 172 IV + 402 enrich + 17 filters + 8 chain-stats + 4 signatures อยู่ใน 68+3+3+1 cases ข้างต้นในรูปแบบ parametrize — ตัวเลข "จริงจาก test suites" ตาม DOD คือ 87/0/1

### A.3 สิ่งที่แก้ระหว่างทำ baseline (P8.1 hygiene)

- พบ: `tools/probe-webull.mjs` (Node script จากงาน access-token lifecycle) โดน eslint lint ด้วย globals ของ browser → 30 errors `no-undef` (console/process/fetch/URL/setTimeout) ทำให้ `npm run check` และ `npm run verify` ตก
- แก้: เพิ่ม block `files: ['tools/**/*.mjs']` ใช้ `globals.node` ใน `eslint.config.js` — แก้ที่ config ไม่แตะตัว script · หลังแก้ `npm run check` ผ่าน (0 errors)
- ไม่มีการแก้สูตร quant/screener เดิมแม้แต่บรรทัดเดียว

### A.4 ขั้นต่อไป

P8.2 — probe ความเป็นไปได้ของข้อมูล historical ตาม matrix ใน §1 (NVDA+SPY · Put/Call · ATM/OTM · expired 1/7/30/90/365 วัน · lookback 1/7/30/90/365 วัน) ผ่าน adapter เดิม พร้อม throttle เดิม ออกรายงาน sanitized (ไม่มี secrets) แล้วตัดสินระดับ A/B/C

---

## Appendix B — คำตอบเจ้าของ + ผล P8.2 probe (2026-11-22)

### B.1 คำตอบของเจ้าของ (บันทึกตามที่ตอบ)

| # | คำถาม | คำตอบ | ตีความสำหรับแผน |
|---|---|---|---|
| 1 | Risk budget | **30%** | drawdown ceiling เริ่มที่ 30% สำหรับ P12.3/P13 — ทุนทดลอง USD ยังไม่ได้ระบุ (ใช้ "ต้องกรอกก่อนรัน" ตามข้อเสนอเดิม ไม่ตั้งแทน) |
| 2 | ข้อมูลย้อนหลังไม่พอทำไง | **ข้อมูลจาก API เท่านั้น** | ห้ามซื้อ vendor อื่น ห้ามข้อมูลมือ — ยืนยัน fallback ระดับ C (เก็บเดินหน้าเอง) ถ้า A ไม่ผ่าน และผล probe ออกมาแล้วว่าเป็นจริง (ดู B.3) |
| 3 | เมื่อ CSP โดน assigned ทำอะไรต่อ | **ไม่เข้าใจคำถาม** | อธิบายใหม่พร้อมตัวอย่างแล้ว (รายงานช่องทางแชท) — รอเจ้าของเลือก policy ก่อนถึง P11.3; ระหว่างนี้ Lab รายงาน scenario ทั้งสามแยกกัน |
| 4 | Fees/slippage ใช้ค่าไหน | **ตรวจสอบจาก Webull มาเลย** | ค้นแล้ว (B.4) — ใช้เป็น cost input รุ่นแรก ติดป้ายว่ามาจากเว็บประกาศ ยังไม่ได้ verify กับสลิปจริง |
| 5 | เปิดแอปเก็บข้อมูลสม่ำเสมอแค่ไหน | **ใช้งานจาก API เท่านั้น** | forward capture ต้องออกแบบให้ "ข้อมูลที่มี = ข้อมูลที่ API ให้" — บันทึกทุกครั้งที่แอปเปิด + coverage gap แสดงตรงไปตรงมา ไม่สมมติว่าบันทึกตอนปิด |

### B.2 ผล probe จริง (26 calls · 0 ครั้งโดน 429 · throttle 1.1s ใช้งาน · token ใช้ได้)

รายงานเต็ม: `tmp/codex-consult/p8.2-probe-report.json` (sanitized — ไม่มี key/token)

| ช่อง matrix | ผล |
|---|---|
| stock snapshot NVDA/SPY | 200 OK (token/entitlement ผ่าน) · latency 344/1205ms |
| contracts LISTING 8-14d | 200 · NVDA 486 แถว · SPY 1000 แถว (ชน cap หน้า 1000) |
| bars สัญญา active · count 1/7/30/90/365 | 200 ทุกครั้ง · **จำนวนแท่ง = min(count, แท่งที่มีตั้งแต่วัน listed ของสัญญานั้น)** — SPY261130P ได้ 84 แท่ง (2026-06-17→11-20) · SPY261130C-ที่ค้างนาน 5 แท่ง (กระจาย กรกฎาคม→พฤศจิกายน แสดงว่าไม่มี trade = ไม่มีแท่ง) · NVDA สัญญาที่เพิ่ง listed ได้ 4-7 แท่ง |
| bars: สัญญา expired | **ไม่สามารถทดสอบ** — หาสัญญา expired ไม่ได้ (แถวถัดไป) |
| contracts วันหมดอายุในอดีต | status=EXPIRED/DELISTED/UNLISTED → **HTTP 417** · ไม่ส่ง status → 200 แต่ **0 แถว** (NVDA และ SPY เหมือนกัน) |
| pagination บน contracts | หน้าละ 1000 แถว · **ไม่มี pagination_key กลับมา** — หน้าต่าง 7 ช่วง DTE ของ discovery เดิมจึงสำคัญ (แบ่งช่วงเล็กพอที่จะไม่ชน cap) |
| รูปแบบ bar | `time` ISO+0000 (แท่งวันปิด ET เที่ยงคืน=05:00 UTC หน้าหนาว/04:00 UTC หน้าร้อน — ต้อง handle DST ตามแผน P8.3) · มี close/volume ไม่มี bid/ask |

ข้อจำกัดของ probe เอง (บันทึกตรงๆ): field ราคา spot เดาผิด (`last_price`/`close` ไม่ใช่ `price`) ทำให้ spot=0 และ picks ตกไปที่ strike ต่ำสุดแทน ATM — ไม่กระทบข้อสรุป feasibility (พฤติกรรม bars เหมือนกันทุกสัญญาที่ทดสอบ) แต่จำนวนแท่งของ "ATM" ในรายงานอ่านเป็น "สัญญาที่ทดสอบ" เท่านั้น

### B.3 Verdict ตามเกณฑ์ §1 ของแผน — **C หลัก + B จำกัด**

- **A (historical strategy replay): ไม่ผ่าน** — ไม่มีทางสร้าง universe ย้อนหลัง: สัญญา expired ค้นไม่ได้เลย (ไม่มีก็ไม่รู้ว่า "ตอนนั้นเห็นอะไร") + ไม่มี historical quotes/Greeks/OI/spread ย้อนหลัง
- **B (limited contract replay): ผ่านบางส่วน** — bars ของสัญญาที่ยัง LISTING มีจริงตั้งแต่วัน listed (ยาวสุดที่เห็น 5 เดือน/84 แท่ง) ใช้ดู "path ของสัญญาเดี่ยวตั้งแต่ listed" ได้ แต่สัญญา short-DTE ส่วนใหญ่ listed ไม่กี่วัน ประโยชน์จำกัด + แท่งคือ trade days ไม่ใช่ bid/ask
- **C (forward capture): เส้นทางหลัก** — เริ่ม P8.4 ทันที: บันทึก universe ก่อนคัดกรอง + quotes + คำทำนายทุกครั้งที่แอปเปิด ตามคำตอบข้อ 2/5 ของเจ้าของ (API เท่านั้น) — P11 replay จะ gate ไว้เฉพาะข้อมูล forward archive ที่เก็บเอง + ส่วน B สำหรับสัญญา listed นาน

### B.4 Cost inputs รุ่นแรก (จากประกาศ Webull Thailand — ยังไม่ verify กับสลิปจริง)

| รายการ | ค่า (ต่อสัญญา) |
|---|---|
| Options commission | ~$0.55 (ไม่มีขั้นต่ำ; index options ~$0.50) |
| คำสั่งเกิน 500 สัญญา | +$0.10 |
| OCC clearing/settlement | ~$0.025 |
| ORF (Options Regulatory Fee) | ~$0.04–0.06 |
| CAT/SEC/FINRA | เข้าไปในสลิปรวมแล้วตามประกาศ |
| VAT | 7% คิดบนค่าคอมมิชชั่น (ผู้ใช้ไทยรายงานในสลิป) |

รวม **~$0.6635 ต่อสัญญาต่อขา (per side)** (commission 0.55 + clearing 0.025 + ORF 0.05 + VAT 7% บนคอม) — round trip เข้า+ออก ≈ **$1.327** ก่อน slippage (แก้กำกวม "ต่อรอบ" ตาม amendments C.1) — บันทึกเป็น versioned cost input เริ่มต้น: `commission=0.55, clearing=0.025, orf=0.05, vat=7% บน commission, assignment/exercise fee=ไม่ทราบ (ต้องเช็คสลิปจริง)` ทุกตัวติดป้าย "ไม่ทราบ" ห้ามตั้ง 0 เงียบๆ ตามแผน §3.4 · ที่มา: [webull.co.th/pricing](https://www.webull.co.th/pricing) + ประกาศทางการของ Webull Thailand

### B.5 Security findings ระหว่าง P8.2

1. **`Credentials::masked()` พิมพ์ `app_key` เต็ม** (แก้แล้ว: เหลือ 8 ตัวแรก + …) — app_key โผล่ใน stdout ของ probe 1 ครั้งก่อนแก้; app_key อย่างเดียวใช้ sign ไม่ได้ (ต้องมี secret คู่กัน) จึงเสี่ยงต่ำ แจ้งไว้ตรงๆ — ถ้าอยากชัวร์ 100% ขอ rotate app_key กับ Webull ได้
2. รายงาน probe ไม่มี key/token ใดๆ; credentials โหลดจาก keyring เหมือนแอปจริง ไม่แตะ .env

### B.6 สุขภาพ gates (บันทึกเพิ่มจาก Appendix A)

- `cargo test -p radar-core --features client,stream` = 81 ผ่าน 0 ตก (+ `cargo test --workspace` รวม 87 ผ่าน) หลังแก้ masked()
- **fmt drift ของเก่า**: `cargo fmt --check` ตก ~60 จุดใน 12 ไฟล์ที่ commit ไปแล้ว (น่าจะจาก rustfmt เปลี่ยนเวอร์ชันตอนสลับ toolchain GNU→MSVC) — ไฟล์ใหม่ของ Lab ทุกไฟล์ fmt-clean; ขอเจ้าของอนุมัติ `cargo fmt` ทั้ง repo เป็น commit แยก (chore) เพราะ diff ใหญ่แต่เชิงกลไกล้วน

---

## Appendix C — Codex amendments + ผล P8.3 (2026-11-22)

### C.1 Amendments จาก Codex (หลังรับ clarifications + ผล P8.2)

สรุปเต็ม: `tmp/codex-consult/plan-amendments.md` (Codex เขียน) — ประเด็นที่ถูกนำมาใช้:
- **ภาพ 3 ใบ = ไอเดีย/inspiration เท่านั้น** (เจ้าของยืนยัน) — charcoal V2 + 4-column shell เป็น authority; กราฟใน P9 เป็นข้อเสนอ ไม่ใช่ requirement
- ความแม่นของ probe เพิ่มขึ้น: expired bars "ยังทดสอบไม่ได้" ≠ "ไม่มี"; 84 แท่ง ≠ retention guarantee; correction: OCC ใน B.2 ที่ถูกคือ `SPY261130C` (ข้อความเดิมพิมพ์ SPY261230C ผิด)
- P8.3 เพิ่ม DOD: valid-spot gate (ห้าม fallback), truncation flag, time semantics 4 ชั้น (session/event/received/available-at), gap taxonomy ห้ามเติมค่า
- P8.4 เพิ่ม DOD: แยก discovery coverage กับ quote coverage, capture universe ก่อนคัดกรอง, pinned trial contracts ไม่หายเมื่อหลุด top-96, บันทึก start/stop/reconnect/gap, ข้อมูล underlying ด้วย
- P11 primary dataset = forward archive; P12 แยก engine readiness ออกจาก evidence readiness (ranking มี gate รอหลักฐาน); P13 drawdown 30% = equity รวมทั้งพอร์ต `DD_t = 1 − Equity_t/Peak_t`
- Fees: ระบุ basis ให้ชัด — **ต่อขา (per side)** คือ ~$0.6635/สัญญา (0.55+0.025+0.05+VAT 7% บนคอม); round trip entry+exit ≈ **$1.327** (ก่อน slippage) — B.4 เดิมเขียนกำกวม "ต่อรอบเข้า/ออก" ให้ถือค่านี้แทน; assignment/exercise fee = unknown แสดง "หลังค่าใช้จ่ายที่ทราบ"

### C.2 P8.3 — Dataset manifest + quality checks: DONE

ไฟล์ใหม่: `crates/radar-core/src/lab/{mod,types,quality}.rs` (schema v1) + tests ในตัว 13 เคส

| สิ่งที่ส่งมอบ | หลักฐาน |
|---|---|
| `ContractRef` / `ObsKind` / `Observation` (เวลา 4 ชั้น + backfill flag + Option ทุกตัวเลข ห้าม 0 แทน missing) | `lab/types.rs` + `LAB_SCHEMA_VERSION=1` |
| `validate_observation` ปฏิเสธพร้อมเหตุผลไทย: multiplier ไม่ valid, received ก่อน event, session_date ผิด, bar available ก่อนปิดตลาด (look-ahead), NaN/Infinity | tests `reject_*` 5 เคส |
| `session_date_of` / `bar_available_at` — DST fixtures ผ่าน (Jul 04:00 UTC = 20:00 UTC ปิด; Nov 05:00 UTC = 21:00 UTC ปิด) | test `dst_session_dates_match_ny_calendar` |
| `dedupe_observations` deterministic (เก็บ received เร็วสุด, แยกตามชนิด, รายงานตัวที่ตัด) | test `dedupe_keeps_earliest_received...` |
| `classify_gaps` — CaptureOutage (อิง session log) / UnknownGap; **ไม่มีการเติมค่าใดๆ**; ไม่อ้าง no-trade โดยไม่มีปฏิทิน exchange | test `gaps_classified_by_outage_window...` |
| `detect_truncation` — ชน cap โดยไม่มี cursor = possibly_truncated (ตรงผล P8.2: 1000 แถวไม่มี pagination_key) | test `truncation_flag_matches_probe_finding` |
| `select_near_spot` — **spot ≤ 0 = error ภาษาไทย ห้าม fallback** (แก้รากเหตุที่ P8.2 เดา field ผิด) + เลือก ATM/OTM 4 ตำแหน่ง | tests `reject_selection_when_spot_invalid...` + `select_atm_otm_with_valid_spot` |

### C.3 Targeted smoke ด้วย spot ที่ valid (`tmp/codex-consult/p8.3-smoke-report.json`)

- spot จาก field `price` จริง: NVDA **125.53** → ATM K=126 ทั้ง put/call · SPY **689.81** → ATM K=690 — picks เป็น ATM จริงแล้ว (ไม่ตก strike ต่ำสุดเหมือน P8.2)
- หลักฐานระดับ B แม่นขึ้นด้วย ATM จริง: **SPY261130 ATM put K=690 มี 106 แท่ง (2026-05-26→11-20, ~6 เดือน)** · ATM call ladder 35 แท่ง (ตั้งแต่ 2026-06-02) · OTM put 586: 14 แท่ง · NVDA ATM (หมดอายุ 30 พ.ย.): 1 แท่ง — ยืนยัน **ความลึก = อายุตั้งแต่สัญญา listed** ไม่ใช่หน้าต่างเวลาคงที่ · count param ทำงานเป็น cap จริง (count=30→30 แท่ง, count=365→35 เพราะมีแค่นั้น)

**P8.3 verdict: DONE — DOD ต้นแผน + ทุกข้อเพิ่มจาก amendments ผ่านครบ**

---

## Appendix D — ดีไซน์ post-close capture + ผล P8.4 (2026-11-22)

### D.1 เหตุผลของดีไซน์ (คำถามสำคัญของเจ้าของ)

เจ้าของถามก่อนลงมือ: "ฐานข้อมูลจะถูกเลี้ยงโดยแอปเลยใช่ไหม แล้วต้องเปิดแอปตลอดหรือเปล่า?" + ยืนยันว่า **เครื่องไม่ได้เปิดช่วงตลาด US (22:30–05:00 เวลาไทย)** — คำตอบคือไม่ต้องเปิดแอปตลอด:

- หลักฐานของ Lab ต้องการ "ภาพราคาปิดรายวัน" ไม่ใช่ tick ต่อเนื่อง
- ตัวบันทึกเป็นโปรแกรม headless แยกจากแอป (ใช้ engine/keyring/token เดิม 100%) ตั้งเวลาให้ Windows รันเช้าเวลาไทย = หลังปิดตลาด US เสมอ
- **target = session ที่ปิดล่าสุด** วันเสาร์/อาทิตย์/จันทร์เช้า ถอยไปเก็บ "ราคาปิดศุกร์" (วันหมดอายุสำคัญที่สุด) — ไม่พลาดข้อมูลศุกร์
- semantics: `Anchor::post_close` — event_time = เวลาปิด session นั้น, received = ตอนเก็บ, `backfill=true` ตรงตาม amendments §3 ("รับข้อมูลย้อนหลังหลังเปิดแอปให้ระบุว่า backfill")
- ยอมแลก: ยังไม่เห็น path ระหว่างวัน (จำเป็นภายหลังเฉพาะกติกา "ปิดก่อนเมื่อได้ 50%" — ถ้าถึงตอนนั้นค่อยพิจารณาเครื่องเปิดตลอด)

### D.2 สิ่งที่ส่งมอบ

| ชิ้น | รายละเอียด |
|---|---|
| `lab/storage.rs` | append-only JSONL รายวัน `app-data/lab/v1/<NY-session>/` · validate ก่อนเขียน (เหตุผลไทย) · dedupe-on-write · retention · export sanitized · delete — tests 5 เคส |
| `lab/capture.rs` | `plan_capture` (แยก discovered universe กับ subscribed subset + truncation flag) · `Anchor{at_observation, post_close}` · `option_observations` (0→None ไม่ใช่ศูนย์จริง) · `stock_observation` (เก็บ `pre_close` เพื่อ outcome resolution) — tests 6 เคส |
| `src-tauri/examples/lab_recorder.rs` | headless recorder: token check (exit 2 พร้อมวิธีแก้ภาษาไทย) → stock (spot invalid = ปฏิเสธ ไม่ fallback) → discovery 7 หน้าต่าง ±30% → quotes subset 96 → เขียน + สรุปไทย · **idempotent**: session นั้นเก็บแล้ว = ข้าม (`LAB_RECORDER_REFRESH=1` เก็บซ้ำ) |
| config | `app-data/lab/config.json` `{underlyings, retention_days=400}` (default = symbol ล่าสุดของแอป) |

### D.3 ผลรันจริงครั้งแรก (2026-11-22 เสาร์เย็น → เก็บปิดศุกร์ 20 พ.ย.)

- session 2026-11-20 · 7 windows: discovered 1,284 สัญญา (band ±30%) · quoted 96 · **เขียน 97 records (1 หุ้น + 96 option) · ซ้ำ 0 · ปฏิเสธ 0 · 19 วินาที**
- ข้อมูลจับ event ใหญ่ได้จริง: NVDA ปิดศุกร์ 125.53 (pre_close 145.98 = ร่วง ~14%)
- รันซ้ำทันที = "มีข้อมูล session นี้อยู่แล้ว 97 แถว — ข้าม" ✓ idempotent
- ตรวจไฟล์ข้อมูลจริง: `backfill=true` ทุกแถว · session=2026-11-20 · universe_id=`NVDA-2026-11-20` ครบ · **ไม่มีชื่อ field ค่าลับใดๆ ในไฟล์** ✓

### D.4 วิธีตั้งเวลา (เจ้าของสั่งได้เลย — คำสั่งเดียว)

```
schtasks /Create /TN "PremiumRadar Lab Recorder" /TR "C:\Users\bfipa\.cargo-target\release\examples\lab_recorder.exe" /SC DAILY /ST 08:00 /F
```

เช้า 08:00 เวลาไทยทุกวัน = หลังปิดตลาด US เสมอ · ถ้าเครื่องหลับ/ปิดวันไหน = coverage gap วันนั้นแสดงตามจริง (ไม่มีการเติมข้อมูลปลอม) · token หมดอายุ ~15 วัน → เปิดแอปสร้างใหม่ครั้งเดียว (recorder จะออก exit 2 พร้อมข้อความบอกวิธีแก้ ให้อ่านจาก Task Scheduler history หรือรัน manual เช็คได้)

### D.5 สิ่งที่ยังค้าง (ตาม DOD เดิม)

- capture จากในแอป (โหมด bonus ตอนแอปเปิดระหว่างตลาด) — โครง `Anchor::at_observation` รองรับแล้ว เหลือ hook ใน scan.rs (งานถัดไป ไม่บล็อกการสะสมข้อมูล)
- validator อิสระ (ตาม loop ของโปรเจค) — โค้ด P8.3/P8.4 ทั้งหมดรอ validator รอบเดียว

**P8.4 verdict: DONE (capture เริ่มทำงานจริงแล้ว — ข้อมูลวันแรกถูกเก็บเรียบร้อย) · Phase 8 DOD ผ่าน: probe A/B/C ตัดสินแล้ว + dataset ตัวอย่างตรวจ provenance ได้**

---

## Appendix E — P9 ส่งมอบ + validator + ปิด session กลางคืน (2026-11-23)

- P9 UI: OptionLab บนแท็บใหม่ (shell เดิม) · Rust คำนวณทั้งหมดผ่าน `lab_analyze` · freeze prediction (P9.4) append-only 1 สัญญา/วัน
- P10/P12/P13 engine cores ส่งมอributed พร้อม tests · empirical = รอข้อมูล (โดยดีไซน์)
- **Validator อิสระ: PASS-WITH-NOTES → แก้ครบ 7 จุด (F1 CC-BE segment, F2 fee set เดียว, F3 reserve ซ้ำ, F4 delete predictions, F6 UI sort/dead-code, F7 freeze dedupe; F5 holiday-mislabel จดเป็น known limitation ตามข้อจำกัดไม่มีปฏิทิน exchange)**
- schtasks ตั้งแล้ว + ทดสอบ /Run exit 0 · commits: 6474234 → 41a411b → 4e3433b → f8220f4
- เหลือรอบถัดไป: replay engine เต็มรูปแบบ (P11 orchestration), walk-forward จริงเมื่อข้อมูลพอ, คู่มือไทย (P13.4)
