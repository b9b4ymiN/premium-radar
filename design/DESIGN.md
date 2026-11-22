# PREMIUM RADAR — DESIGN.md

**Design system & implementation reference**  
**Version:** V2 — Modern Visual Refinement  
**Source of truth:** `premium_radar_modern_v2(1).html` (ไฟล์ HTML ที่ผู้ใช้แนบในรอบนี้)  
**Source SHA-256:** `6e6c699f93d043bcaffa34fe221b00f6409f26165572c5360185df1b661850ea`  
**Product:** Premium Radar — Webull-inspired Options Trading Workspace  
**Status:** Approved visual direction / **Structure Freeze**  
**Scope:** เอกสารนี้ถอดจาก HTML/CSS/JavaScript ที่มีอยู่จริง ไม่ใช่ข้อเสนอให้สร้าง Layout ใหม่

> **Implementation rule:** รักษาองค์ประกอบ ตำแหน่ง สัดส่วน ลำดับการอ่านและพฤติกรรมหลักของต้นแบบไว้ก่อน การปรับปรุงที่ไม่ผ่านการอนุมัติให้จำกัดเฉพาะ visual styling และ code organization ที่ไม่เปลี่ยนผลลัพธ์บนหน้าจอ ห้ามตีความตัวเลข Demo เป็นข้อมูลตลาดสดหรือโมเดลความเสี่ยงที่ผ่านการพิสูจน์แล้ว

---

## 1. Design intent

### 1.1 เป้าหมาย

- ให้ความรู้สึกเป็น **Professional Trading Terminal**: มีความหนาแน่นของข้อมูลสูงแต่จัดลำดับชัดเจน
- ยึดภาษาออกแบบ **Webull-inspired** โดยไม่อ้างว่าเป็น UI ที่เหมือน Webull อย่างเป็นทางการ
- ใช้ **Charcoal / Neutral Dark** เป็นฐาน และใช้ **Electric Blue** สื่อ selection, focus และ active action
- ใช้สีเขียว/แดงกับค่าการเคลื่อนไหวทางการเงินและสถานะที่มีความหมาย ไม่ใช้เป็นสีตกแต่งโดยไม่จำเป็น
- ลดเงาหนักและ gradient ของการ์ด; ใช้พื้นผิวต่างระดับเล็กน้อยและ **1px dividers** เพื่อแบ่งข้อมูล
- อ่านข้อมูลการซื้อขายได้รวดเร็ว: ตัวเลขชิดขวา, ใช้ tabular numbers, row states เด่นพอดี

### 1.2 สิ่งที่ถือว่าล็อกจาก HTML

1. **4-column application shell:** Icon Rail → Watchlist / Filters → Main Trading Workspace → Contract Analysis / AI
2. **ลำดับส่วนกลาง:** Main Tabs → Quote Header → Chart + Options Statistics → Opportunity Cards → Options Scanner / Chain
3. **Right Panel:** Contract Analysis อยู่เหนือ Radar Assistant
4. **Responsive behavior:** ซ่อน Right Panel ที่หน้าจอแคบ, ซ่อน Left Panel ที่ระดับ mobile ตาม breakpoint เดิม
5. **Interaction model:** Symbol selection, scanner filters, table modes, contract selection, chart timeframes และ rule-based chat demo
6. **Data contract เดิมของ Prototype:** 8 สัญลักษณ์หุ้น / 13 แถว Option จำลอง / 1 สัญญา = 100 หุ้น

หากต้องเพิ่มหน้า Analytics, layout editor, panel ใหม่ หรือแก้โครงสร้างคอลัมน์ ให้ถือเป็น **change request แยกต่างหาก** ไม่อยู่ในสัญญาออกแบบนี้

---

## 2. Actual CSS tokens (ค่าที่มีผลจริง)

ต้นฉบับมี `:root` สองชุด: **base CSS** ก่อน และ `MODERN VISUAL PASS V2 — STRICT STYLE-ONLY OVERRIDE` ภายหลัง ค่าที่แสดงด้านล่างคือ **computed design variables หลังถูก override ตาม cascade** ไม่ใช่สีเก่าชุดแรก

```css
:root {
  /* Foundation */
  --bg: #0b0d11;
  --nav: #14171d;
  --side: #11151b;
  --surface: #10141a;
  --surface2: #171c24;
  --surface3: #1c2633;

  /* Delineation */
  --border: #292f39;
  --borderSoft: #222832;

  /* Typography */
  --text: #f0f3f8;
  --muted: #a2adbc;
  --dim: #778394;

  /* Semantic & accent */
  --blue: #4b8cf7;
  --cyan: #7fb4ff;
  --green: #24c99a;
  --red: #f4788c;
  --orange: #f0bd70;

  /* Number family */
  --mono: 'Inter Display', 'Inter', 'Segoe UI', Arial, sans-serif;
}
```

| Role | Value | ใช้กับ |
|---|---|---|
| Base canvas | `#0b0d11` | Background ทั้ง Application |
| Top chrome | `#14171d` | App Bar, Brand area |
| Left / rail base | `#11151b` | Sidebar surfaces |
| Main workspace | `#0d1116` | `.main` (ค่าราย selector) |
| Separator | `#292f39`, `#222832` | ขอบและเส้นแบ่งที่ไม่แย่งความสนใจ |
| Primary text | `#f0f3f8` | ค่าหลักและข้อความสำคัญ |
| Supporting text | `#a2adbc` | Label / caption / secondary |
| Disabled-like / low-emphasis | `#778394` | ข้อมูลที่มีลำดับรองมาก |
| Active blue | `#4b8cf7` | Tabs / focus / selected state |
| Secondary blue | `#7fb4ff` | Supporting accent / links |
| Positive | `#24c99a` | ราคาบวก, Premium, positive indicators |
| Negative | `#f4788c` | ราคาลบ, risk indicator |
| Caution | `#f0bd70` | Warning / DEMO accent |

**Important:** หลายจุดใน HTML ใช้ **hardcoded selector-specific colors** และสีที่วาดลง SVG/JavaScript เช่น `.appbar #14171d`, `.screener-table th #222a35`, กราฟแท่ง `#00b991 / #ef4d72` และ `payoffSvg()` สี `#2da5f5` ดังนั้นเปลี่ยน `:root` อย่างเดียว **จะไม่เปลี่ยนทั้ง UI** ต้องตรวจ style override และ dynamic SVG ด้วย

### 2.1 Color semantics

- **Green / `.pos`** (`!important`): positive stock movement, incoming option premium และค่าที่ UI มองว่าเป็นบวก
- **Red / `.neg`** (`!important`): negative movement, Put label ใน scanner, highlight assignment risk ที่มากกว่า threshold ใน Demo
- **Blue / `.blue`**: accent สำหรับ selected / supporting information
- **Amber / `.amber`**: caution / expected move / disconnected data status
- **Source marker** เป็นจุดสีเล็ก (เช่น `.data-dot`) ไม่ใช่เครื่องหมายยืนยันข้อมูลสด ต้นแบบแสดง `DEMO`
- **Grade A/B/C** ใช้ palette เขียว / ทอง / slate; Grade เป็นคะแนนตัวอย่าง ไม่ใช่ rating ที่ผ่านการรับรอง

ห้ามใช้สีเพียงอย่างเดียวเพื่อสื่อสารผลลัพธ์สำคัญใน Production ต้องมีข้อความหรือสัญลักษณ์ประกอบด้วย

---

## 3. Typography & density

### 3.1 Font stack ที่อยู่ใน HTML จริง

```css
body {
  font-family: 'Inter', 'Inter Display', 'Noto Sans Thai',
               'Segoe UI', sans-serif;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  -webkit-font-smoothing: antialiased;
}
.numeric {
  font-family: var(--mono);
  font-variant-numeric: tabular-nums;
}
```

**ข้อควรรู้:** HTML ไม่มีการโหลด webfont จาก CDN หรือ `@font-face` ดังนั้นหน้าตาจริงขึ้นอยู่กับฟอนต์ที่ติดตั้งในอุปกรณ์; การแสดงภาษาไทยอาจ fallback แตกต่างตาม OS ถ้าต้องการ pixel consistency ให้จัดการ font loading ในขั้น Implementation (โดยไม่เปลี่ยนขนาดและลำดับ UI)

### 3.2 Type hierarchy จากองค์ประกอบเดิม

| องค์ประกอบ | ขนาดที่ปรากฏใน CSS | น้ำหนัก / หมายเหตุ |
|---|---:|---|
| Quote price (`.bigprice`) | `30px` | `730`, tabular numbers |
| Selected ticker (`.symbol`) | `21px` | `780`, tracking `-.045em` |
| Contract metrics (`.ds strong`) | `17px` | `650` |
| Option Statistics value (`.greek strong`) | `15px` | `660` |
| Contract name (`.detailkind h2`) | `15px` | `760` |
| Opportunity contract (`.ocontract`) | `13px` | `720` |
| Brand (`.brandtext`) | `12.2px` | `750`, tracking `.075em` |
| Main body | `12px` | Default |
| Main tabs / table tabs | `11px` | `600` |
| Screener cells | `10.8px` | Numeric family |
| Table headers | `9.5px` | `650` |
| Supporting labels | `9–10.5px` | Low emphasis |

**Density principle:** เป็น Desktop Data Terminal ไม่ใช่ Consumer Marketing Dashboard อย่าขยายตัวอักษรทุกระดับพร้อมกันจนตารางมีแถวสูงเกินไป หรือบังคับให้เกิด horizontal scroll มากกว่าต้นแบบโดยไม่จำเป็น

---

## 4. Layout specification (Structure Freeze)

### 4.1 Top-level shell

```text
.app                              height: 100dvh; min-height: 690px
├── .appbar                       43px
├── .workspace                    flexible remaining height
│   ├── .iconbar                  50px
│   ├── .leftpanel                225px
│   ├── .main                     minmax(550px, 1fr)
│   └── .rightpanel               320px
└── .statusbar                    25px
```

Base CSS:

```css
.app {
  height: 100dvh;
  min-height: 690px;
  display: grid;
  grid-template-rows: 43px minmax(0, 1fr) 25px;
  overflow: hidden;
}
.workspace {
  display: grid;
  grid-template-columns: 50px 225px minmax(550px, 1fr) 320px;
  min-height: 0;
  min-width: 0;
}
```

### 4.2 Main column — section heights / order

```text
.main
├── .main-tabs                   34px
├── .quoteheader                 min-height: 75px
├── .topgrid                     258px; chart 1fr + snapshot 250px
│   ├── .chart-widget
│   └── .snapshot
├── .opportunities               88px; 3 equal columns
└── .tablemodule                 flex: 1 (remaining height)
    ├── .table-head              37px
    ├── .table-status            min-height: 25px
    ├── .tblscroll               flex: 1; overflow: auto
    └── .table-footer            25px
```

`tablemodule` และ `tblscroll` มี `min-height:0` สำหรับให้ตาราง scroll ภายใน ไม่ขยายพื้นที่ทั้งหน้า `.main` ใช้ `overflow:hidden` ห้ามแก้โครงสร้างเลื่อน Scroll โดยไม่ได้ตรวจผลกระทบ

### 4.3 Left panel

`.leftpanel` วางเรียง:

1. `.panel-heading` — Watchlist header, สูง `32px`
2. `.watch-header` + `#watchlist` — Symbol / Last / Change และ mini sparklines
3. `.watch-bottom` — Watchlist label และ count
4. `.filterhead` — Strategy Scanner heading
5. `#strategyChoices` — All / Put · CSP / Call · CC
6. `#riskChoices` — All / Low / Moderate / High
7. `#expirationChoices` — 2-column expiry buttons
8. `#budget` — Available Cash (USD)
9. `.leftfoot` — Synthetic snapshot / data status

Left panel มี `overflow:auto` เพื่อรองรับความสูงหน้าจอจำกัด โดยไม่ทำให้ main table เปลี่ยนความสูง

### 4.4 Right panel

```text
.rightpanel
├── .detailpanel                flex: 0 0 57%; min-height: 340px
│   ├── .widget-title           Contract Analysis
│   └── #detailBody             overflow: auto
└── .copilot                    flex: 1; min-height: 230px
    ├── .widget-title           Radar Assistant
    ├── #chatlog                scrollable
    ├── .small-ask              quick prompts
    └── #chatForm               input + send
```

Right column ไม่ได้เป็น floating drawer ใน HTML และไม่ได้ collapse เมื่อคลิก — จะ **ถูกซ่อนตาม breakpoint** เท่านั้น

### 4.5 Responsive breakpoints — ตาม source CSS

| Viewport | `.workspace` | Additional changes |
|---|---|---|
| `>1420px` | `50px 225px minmax(550px, 1fr) 320px` | Desktop full terminal |
| `≤1420px` | `44px 198px minmax(490px, 1fr) 292px` | Chart snapshot `220px`, brand `242px`, space กระชับ |
| `≤1150px` | `44px 188px minmax(400px, 1fr)` | `.rightpanel{display:none}`, nav tabs ลำดับ 4 เป็นต้นไปถูกซ่อน |
| `≤790px` | `40px minmax(0, 1fr)` | `.leftpanel{display:none}`, `.snapshot{display:none}`, head quote stats ซ่อน, brand `185px`, search `140px` |

หมายเหตุ: `@media(max-width:790px)` ยังตั้ง `.app{min-height:680px}` แทน `690px` จาก base ขณะ mobile ไม่มี mobile filter drawer ที่สามารถเปิดกลับได้ใน HTML รุ่นนี้ **ห้ามอ้างว่ารองรับ workflow เดียวกันครบเมื่อหน้าจอแคบ**

### 4.6 Elevation, borders, radius

- โครงสร้างหลักใช้ **subtle 1px border** และ surface separation ไม่ใช้เงาการ์ดหนัก
- Navigation / filter / selected items มักใช้ background ต่างระดับ + border
- Corner radius อยู่ราว `2–9px` แล้วแต่ component; pill ขนาดใหญ่ไม่ใช่ภาษาหลักของ UI
- เงามีเฉพาะจุดที่เหมาะสม เช่น `.toast` ไม่ใช่ทุก panel
- Grid / Table เน้นเส้นแบ่งและ row hover มากกว่า card tiles

---

## 5. Component inventory & visual contracts

| Component | DOM / CSS selector หลัก | ควรรักษา |
|---|---|---|
| Global header | `.appbar`, `.brandbox`, `.headnav`, `.global-search` | Brand ซ้าย, section tabs, symbol search, DEMO badge, actions ขวา |
| Icon rail | `.iconbar`, `.iconbtn` | Vertical nav, active left indicator, Settings อยู่ด้านล่าง |
| Watchlist | `#watchlist`, `.watch` | Symbol + exchange, sparkline, price + change; selected left border |
| Strategy controls | `#strategyChoices`, `.toggle` | All/CSP/CC แบบ 3 ปุ่มเรียงแนวนอน |
| Risk controls | `#riskChoices`, `.choice` | ลิสต์ตัวเลือกพร้อมค่า Delta ช่วงความเสี่ยง |
| Expiration | `#expirationChoices`, `.exp` | Grid 2 columns, active blue |
| Capital input | `#budget`, `.cashfield` | USD currency prefix, numeric input, short hint |
| Workspace tabs | `.main-tabs`, `.mtab` | Options Monitor / Strategy Lab / Risk Dashboard |
| Quote header | `.quoteheader`, `#priceNow`, `.head-quote-stats` | Ticker, Last, Chg%, Open/High/Low/Prev Close/Volume |
| Candlestick chart | `.chart-widget`, `#marketChart`, `#timeframes` | Chart panel สูง 258px ใน TopGrid; MA5/MA20/Volume legends |
| Options stats | `.snapshot`, `.greek-grid`, `.skewbar` | IV, Expected Move, Put/Call OI, Skew, OI split |
| Opportunity shortlist | `#opportunities`, `.opportunity` | 3 columns: Defensive / Balanced / High Yield |
| Scanner / Chain | `.tablemodule`, `#optionsTable` | Fixed table chrome, sticky header, rows hover/selected, scrollable body |
| Contract analysis | `.detailpanel`, `#detailBody` | Key metrics → probability bar → payoff → explanation → risk |
| Radar Assistant | `.copilot`, `#chatlog`, `#chatForm` | Chat history, quick suggestions, bottom input |
| Status bar | `.statusbar` | OFFLINE, selected symbol, reference date, row count, disconnected feed |
| Toast | `#toast`, `.toast.show` | Temporary centered notification near bottom |

### 5.1 Selection & feedback styles

- **Active top nav:** `.navtab.active` → subtle dark background + blue underline
- **Active workspace tab:** `.mtab.on` → bright text + blue underline
- **Selected watch item:** `.watch.on` → blue left border + blue-tinted background
- **Selected filter / expiry:** `.toggle.on`, `.choice.on`, `.exp.on` → blue-toned fill and border
- **Selected opportunity:** `.opportunity.on` → blue top border + toned background
- **Selected option row:** `tr.selected` → stronger blue row fill; first column has blue inset indicator (Scanner)
- **Hover:** ทำให้ background สว่างขึ้นเล็กน้อย ไม่ยกกล่อง/แอนิเมชันเคลื่อนที่
- **Focus visible:** 2px blue outline + 2px offset จาก V2 override
- **Transitions:** `background-color`, `color`, `border-color`, `box-shadow` ที่ `0.16s ease`; เมื่อ `prefers-reduced-motion:reduce` จะปิด transition ของ button/input

### 5.2 Table presentation

**Scanner** (`.screener-table`) มีคอลัมน์ตามลำดับ:

`Grade | Side | Expiration | Strike | Delta | Assign | Premium | Return | Ann. % | Collateral* | Break even | OI | Source`

**Option Chain** (`.chain-table`) แสดง Call/Put ขนาบ Strike:

`CALLS: IV / Delta / Bid / Ask / OI | STRIKE | PUTS: OI / Bid / Ask / Delta / IV`

- Headers sticky (`position: sticky; top:0`)
- Cell สูง `33px`, header สูง `29px`, text table `10.8px`, header `9.5px`
- Numeric columns ชิดขวา; Grade centered; Side และ Expiration ชิดซ้าย
- Alternating rows ใช้พื้นหลังต่างกันเล็กน้อย; hover/selected ต้องเด่นกว่า zebra
- Selected contract มี background เดียวกันระหว่าง Scanner/Chain เป็นหลัก แต่ Chain ใช้ `tr.selected` ของ strike pair
- ไม่มีการปรับ column order และ field mapping โดยพลการ

**Known behavior ของต้นฉบับ:** Chain ใช้ sample contracts เดิมมาจับคู่ด้วย `expiry + strike` และสัญญา Call/Put ที่มี Strike ไม่ตรงกันจะเห็นช่องฝั่งตรงข้ามเป็น `—`; คลิกหนึ่งแถวจะเลือก `(Put || Call)` ของแถวนั้น ไม่ใช่แยกคลิก Bid/Ask รายฝั่ง

---

## 6. Interactions & state — จาก JavaScript จริง

### 6.1 Client-side state

```js
const state = {
  symbol: 'NVDA',
  risk: 'all',
  strategy: 'all',
  expiry: 'all',
  tab: 'all',
  tableMode: 'scanner',
  sort: 'original',
  budget: 18000,
  selected: 1,
  tf: '3M'
};
```

### 6.2 Interaction matrix

| User action | Actual behavior in HTML |
|---|---|
| Watchlist item click | เปลี่ยน `symbol`; reset `selected=1`; rerender quote, chart, contracts, detail |
| Search ticker + Enter | รับเฉพาะ 8 tickers ที่กำหนด; ไม่รู้จักจะแสดง Toast |
| Strategy All / Put / Call | กรองฝั่ง Option และ reset table tab เป็น All |
| Risk All / Low / Moderate / High | กรองโดย `abs(delta)` ช่วง `.10–.20`, `.15–.30`, `.25–.40` ตามลำดับ |
| Expiration | กรอง `expiry` เท่ากับวันที่เลือก; `All dates` ยกเลิกเงื่อนไข |
| Available Cash input | กรอง Cash-Secured Put ที่ต้องใช้หลักประกันเกินงบ; ไม่ใช้ budget filter กับ Call |
| `All`, `Put · CSP`, `Call · CC` table tabs | กรองแถวในตารางต่อจาก Global Filters |
| Scanner / Option Chain | สลับรูปแบบตาราง โดยไม่ย้าย Panel |
| `APY ↓` | สลับ sort annualized / original และบังคับโหมด Scanner |
| Click option row | อัปเดต `state.selected`, Contract Analysis และ Opportunity selected state |
| Click Opportunity | เปลี่ยน selected contract และ rerender |
| Chart `1D/5D/1M/3M/1Y` | สร้างกราฟจำลองใหม่ตาม timeframe โดยใช้ deterministic pseudo-random seed |
| Top nav / main tabs ที่ไม่ใช่ Options | แสดง Toast ว่ายังไม่ implemented; ไม่ได้เปลี่ยนหน้า |
| AI quick questions / input | ส่งข้อความเข้า Chatlog แล้วตอบด้วย rule-based local logic |
| `Add symbol` | แสดง Toast; ไม่ได้เพิ่ม watchlist จริง |

**Presentational / nonfunctional in source:** Chart foot toggles เช่น MA/VOL/MACD/RSI, Draw Tools, การขยาย widget และบาง icon action เป็นภาพ UI หรือไม่ได้ติด event listener ที่ทำงานจริงใน prototype อย่าระบุว่า feature เหล่านี้ implemented แล้ว

### 6.3 Render/update flow

```text
selectSymbol / filter change / row click
                  ↓
              state update
                  ↓
                render()
                  ├── renderWatchlist()
                  ├── renderQuote()
                  ├── chart()
                  ├── filtered() / selected-contract reconciliation
                  ├── recommendations()
                  ├── renderTable()
                  ├── renderDetail()
                  └── update active states + funnel status
```

ระบบเป็น **single self-contained HTML document** ไม่มี framework, bundler, API request หรือ WebSocket connector โค้ด JS อยู่ใน IIFE `(()=>{'use strict'; ... })()` และใช้ `innerHTML` render ตาราง / analytics ที่อยู่ในหน้านี้

---

## 7. Data model & financial formula — describe, do not validate

### 7.1 Dataset ใน HTML

- 8 symbols: `NVDA, TSLA, AAPL, AMD, SPY, QQQ, MSFT, META`
- 13 sample option rows; `NVDA` เป็น reference สำหรับการสเกลข้อมูลหุ้นอื่น
- Expiry choices: `2026-12-04`, `2026-12-18`, `2027-01-15`, `2027-02-19`, `2027-03-19` (บางวัน **ไม่มี sample contracts**)
- วันที่ reference ของ `days()` **hardcoded เป็น 2026-11-22 12:00 UTC**
- Data source แสดง `DEMO DATA`, `Synthetic snapshot`, `SIMULATED`, `OFFLINE`, `DISCONNECTED` ไม่ได้เชื่อมข้อมูลจริง

### 7.2 Calculation ที่สคริปต์ใช้อยู่

```text
stockRatio         = currentSymbolPrice / NVDAReferencePrice
adjustedStrike     = round(originalStrike × stockRatio × 2) / 2
adjustedPremium    = round(originalPremium × stockRatio)
collateral         = strike × 100
Put breakeven      = strike − premium / 100
Call (demo) BE     = strike + premium / 100
DTE                = round(expiration − 2026-11-22)
Simple annualized  = premium / denominator × 365 / max(DTE,1) × 100
  Put denominator  = strike × 100
  Call denominator = current stock price × 100
```

- `Annualized premium` เป็น **simple annualization**, ไม่ใช่ CAGR, risk-adjusted return หรือการรับประกัน APY
- `Assignment risk` (`assign`) และ `win` เก็บเป็นเลข sample **ไม่ได้คำนวณจาก Delta โดยตรง**
- Probability bar ใช้ `win%`, `Near = min(8, 100-win)%`, `ITM = residual%` — เป็นภาพแสดงข้อมูลตัวอย่าง ไม่ใช่ probability distribution จากโมเดลผ่านการทดสอบ
- Recommendation rules:
  - **Defensive:** `abs(delta)` ต่ำสุดในสัญญาที่ผ่าน filters
  - **Balanced:** `abs(delta)` ใกล้ `0.18` มากที่สุด
  - **High Yield:** ค่า simple annualized สูงสุด
- Chart เป็น pseudo-random synthetic candles ที่ reproducible ตาม symbol/timeframe แต่ **ไม่ใช่ historical OHLCV**
- Option Chain สร้าง Bid/Ask เป็น `premium/100 × 0.97` และ `× 1.05`; IV ใน Chain เป็น `48.2%` คงที่ — **ไม่ใช่ market quote**

### 7.3 Financial caveats ที่ต้องรักษาใน Implementation

1. **Covered Call ต้องมี 100 หุ้นอยู่แล้ว**; เงินสดใน `#budget` ไม่ได้บอกว่าผู้ใช้มีหุ้นพอหรือไม่
2. **Payoff graphic สำหรับ Call เป็น option leg only** (`premium − max(0, S_T − strike) × 100`) — **ไม่ใช่ payoff รวมของ Covered Call + หุ้นที่ถืออยู่**
3. **`Call breakeven` ที่คำนวณแบบ `strike + premium/share` เป็นตัวเลขของขา Short Call ใน Demo** ไม่ใช่ต้นทุนคุ้มทุนของหุ้นที่ถือ (ต้องอาศัย cost basis)
4. คอลัมน์ `Return` ใน Scanner ใช้ `premium/(strike×100)` **ทั้ง Put และ Call** ขณะที่คอลัมน์ `Ann. %` ใช้ราคาหุ้นปัจจุบันเป็นฐานสำหรับ Call จึงเป็นคนละ denominator ต้องนิยามให้ชัดก่อน production
5. `assign`, `win`, `grade` ไม่ผ่าน validation และอาจไม่สัมพันธ์กันเชิงความน่าจะเป็น ห้ามใช้กับคำสั่งซื้อขายจริง
6. Date reference hardcoded ทำให้ DTE / annualized ผิดเมื่อเวลาเปลี่ยน ต้องเปลี่ยนเป็น market-calendar-aware date service ก่อน production
7. แสดงค่ากำไร/ขาดทุนก่อน fees, taxes, slippage, early assignment, dividends และผลกระทบอื่น ๆ ของสัญญาจริง
8. โหมดที่ไม่มีผลการกรอง ตาราง/Recommendation จะแสดง empty state แต่ `renderDetail()` ยังคงแสดงสัญญาจาก dataset เดิมได้ จึงต้องแก้ state consistency ก่อน production

เอกสารนี้ **ไม่ได้รับรองสูตรหรือโมเดล Options**; บันทึกให้ทีมแยก Presentation Contract ออกจาก Financial Model Contract

---

## 8. Production handoff / suggested implementation boundaries

ส่วนนี้คือ **ข้อเสนอเพื่อพัฒนาต่อ ไม่ใช่พฤติกรรมที่ HTML ทำได้แล้ว** และต้องไม่เปลี่ยนภาพรวม UI โดยไม่ได้อนุมัติ

| Layer | Responsibility | Lock/Change policy |
|---|---|---|
| Design tokens | สี, surface, border, typography, focus, semantic state | อ้างอิง computed V2 CSS เป็นมาตรฐาน |
| Shell & layout | App Bar, Icon Rail, Left, Main, Right, Status | **Locked** — เปลี่ยนต้องขออนุมัติ |
| Components | Watchlist, Filters, Quote, Chart, Stats, Opportunities, Table, Detail, Chat | **Locked** ในเรื่องลำดับ/สัดส่วน/การจัดวาง |
| UI state | selections, tabs, sort, timeframes, active/hover states | คง behavior; อาจ refactor ข้างในได้ |
| Market adapter | quotes, option chain, timestamps, feed status | เพิ่ม service ภายหลัง; อย่าปลอม live status |
| Risk / valuation engine | DTE, probability, P/L, collateral, ranking | **เปลี่ยน/ตรวจสอบได้แยกจาก UI** พร้อม tests และ methodology |
| AI adapter | Context, prompt, citations, market-data grounding | ปัจจุบันแค่ rule-based demo; production ต้องแยก capability |

### 8.1 Source-to-component mapping

ตัวอย่างแนวแบ่ง component ถ้าจะย้ายไป React/Next.js (เป็นคำแนะนำ ไม่ใช่ข้อบังคับ):

```text
AppShell
├── AppBar
├── Workspace
│   ├── IconRail
│   ├── ScannerSidebar
│   │   ├── Watchlist
│   │   └── Filters
│   ├── TradingMain
│   │   ├── WorkspaceTabs
│   │   ├── QuoteHeader
│   │   ├── MarketChart + OptionsStatistics
│   │   ├── OpportunityCards
│   │   └── OptionsTable (Scanner | Chain)
│   └── InsightsSidebar
│       ├── ContractAnalysis
│       └── RadarAssistant
└── StatusBar
```

ไม่ควรเปลี่ยนการจัดวาง component เป็นการออกแบบใหม่ในชื่อ "refactor" — visual parity กับ HTML ที่แนบสำคัญกว่า framework ที่เลือกใช้

### 8.2 Data states ที่ Production ควรเพิ่ม โดยไม่เปลี่ยน layout

- `loading`, `empty`, `disconnected`, `stale`, `partial`, `error`, `ready` ในกรอบ panel เดิม
- Timestamp ตาม timezone และตลาดที่ใช้งานจริง; แสดง source provenance per quote / per row
- Table virtualization เมื่อจำนวนสัญญาสูง โดยรักษา header, selection, keyboard focus และ scroll
- Server-side / client-side filtering contract ที่นิยามตรงกัน
- แยก strategy eligibility ของ CSP กับ CC และ position inventory (หุ้นที่ถือ / จำนวนสัญญาทำได้)
- Accessible label ให้ icon-only controls, semantic table headers, keyboard selection และ contrast validation
- ป้องกัน HTML injection ในข้อมูลตลาด/AI ที่ไม่เชื่อถือ; ห้ามฝัง payload จาก API ลง `innerHTML` ตรง ๆ

---

## 9. Quality checklist / acceptance criteria

### A. Visual parity

- [ ] ใช้ **V2 override palette** ไม่ใช่ base palette จากช่วงต้นไฟล์
- [ ] 4 columns คงลำดับและสัดส่วนตาม breakpoint
- [ ] Header สูง 43px, Footer 25px, Main Tabs 34px, Chart TopGrid 258px, Opportunity band 88px
- [ ] Watchlist, Filters, Scanner/Chain, Detail, Chat อยู่ตำแหน่งตาม HTML เดิม
- [ ] Scanner header sticky และ table เลื่อนภายใน `.tblscroll`
- [ ] Active, hover, selected, focus-visible ใช้สีและความเข้มแบบ V2
- [ ] Data density, row height, typography hierarchy และ tabular numerals ใกล้เคียงต้นแบบ
- [ ] สีกราฟที่ render ผ่าน JS/SVG สอดคล้องกับ design palette ด้วย (ไม่พึ่ง `:root` อย่างเดียว)

### B. Interaction parity

- [ ] เลือก symbol แล้ว Quote / Chart / Contracts / Detail เปลี่ยนสอดคล้องกัน
- [ ] Strategy, Risk, Expiry, Budget filter รวมกันได้ตามตรรกะเดิม
- [ ] Table tabs, Scanner/Chain, APY sort ทำงานตามที่ source กำหนด
- [ ] กด Opportunity หรือ Table row แล้ว Contract Analysis อัปเดต
- [ ] Chart timeframes แสดง selected state ถูกต้อง
- [ ] Demo-only actions ยังคงแสดงว่าไม่ implemented ไม่สร้างความเข้าใจผิดว่ารันจริง
- [ ] Empty-filter state ไม่ทิ้งข้อมูล Contract Analysis ที่ทำให้เข้าใจผิด (Production fix)

### C. Financial integrity / transparency

- [ ] ทุกแหล่งข้อมูลกำกับ **Demo / Delayed / Live / Disconnected** ตรงสถานะจริง
- [ ] Formula และ input/output ของ CSP / CC มี unit tests แยกจาก UI
- [ ] DTE ใช้วันที่จริงและ exchange calendar ไม่ใช้ demo reference date
- [ ] Assignment probabilities ไม่ถูกอ้างจาก |Delta| แบบตรง ๆ โดยไม่มี methodology
- [ ] CC payoff / breakeven แสดงทั้ง Option Leg และ Combined Position อย่างถูกต้องเมื่อพัฒนาจริง
- [ ] ค่าตอบแทนรายปีมีชื่อ denominator และข้อจำกัดชัดเจน

### D. Viewports to review

- [ ] Wide desktop — `>1420px`
- [ ] Compact desktop — `1366px` (Right Panel ยังแสดง)
- [ ] Laptop / tablet landscape — `1024px` (Right Panel ซ่อน)
- [ ] Narrow/mobile — `≤790px` (Left Panel และ Snapshot ซ่อนตามต้นฉบับ)
- [ ] Short viewport — ตรวจ min-height / panel scroll, ไม่ทำให้ table หาย

---

## 10. Source limitations & change log

### สิ่งที่ยืนยันได้จาก HTML นี้

- เป็น **single-file self-contained prototype** ที่มี inline CSS และ inline JavaScript
- V2 เป็น **style-only override** ตาม comment ใน source โดยไม่แก้ base layout/DOM/JS ใน pass นั้น
- มี Dashboard ลักษณะ Options Trading Terminal พร้อม Scanner, Chain, Contract Analysis, Chart และ local assistant demo
- ใช้ Synthetic sample snapshot; ไม่มีการเชื่อม WebSocket / Webull API หรือ model AI ในไฟล์นี้

### สิ่งที่เอกสารไม่สมมติแทนต้นฉบับ

- ไม่มี design library / Figma / production token package ที่แนบมา
- ไม่มี backend schema, auth, production API contract หรือการยืนยันความแม่นยำทางการเงิน
- ไม่มีหน้า Analytics 6 กราฟตาม prototype คนละไฟล์; เอกสารนี้อ้างอิง **เฉพาะ HTML ที่แนบในรอบนี้**
- ไม่มีการตรวจความเหมือนแบบ pixel-perfect กับ Webull หรือการทดสอบบนอุปกรณ์ทุกประเภท

### Change log

- **DESIGN.md / 1.0:** Extracted directly from `premium_radar_modern_v2(1).html`; documented effective V2 tokens, layout, interactive state, demo data/financial caveats, and structure-preserving implementation contract. No modifications made to the source HTML.

---

**Design principle for future changes:** **Preserve layout. Preserve information hierarchy. Improve implementation without redesigning the product.**
