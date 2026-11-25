# Product

<!-- impeccable:product-schema 1 -->

## Platform

web (rendered inside a Tauri 2 desktop shell on Windows and macOS)

## Stack

Tauri 2 desktop app — Rust core (Webull TH REST + MQTT streaming, Black-Scholes quant, screener engine, LLM client) + React/TypeScript frontend with ECharts. AI copilot via OpenAI-compatible API: GLM-5.3 primary, OpenAI backup (user holds both keys). Decided 2026-11-22; rationale in brain `decisions/decision-option2-tauri-stack`.

## Users

Updated 2026-11-25 (supersedes the 2026-11-22 entry):

- **Primary user: THP's wife.** She knows only what an option is. She does not know option terms (delta, IV, OI, assign), OI-trader thinking, or how to read a chain. She must be able to decide on her own from what the app shows.
- **Secondary user: THP** and people who already understand options. They use the detailed mode.
- Both are Thai-speaking retail investors selling option premium (CSP / Covered Call) on US stocks through Webull Thailand.

## Product Purpose

North star (THP, 2026-11-25, verbatim): "แอปที่มี AI agent เป็นผู้ช่วยลงทุน option ภาษาไทย ใช้วิธีคิดแบบ OI trader หาโอกาสขาย premium ที่แม่นยำ อธิบายจนคนที่รู้แค่ว่า option คืออะไรตัดสินใจเองได้ อนาคตอาจให้ agent ส่งคำสั่งซื้อขายให้ได้ด้วย โดยเชื่อมกับ Webull ผ่าน MCP และ Skills ของทางการ" Full detail and workstreams: brain `decisions/decision-premium-radar-north-star`.

The engine is still the scanner ported from the Tkinter original `Short_Option_Radar_PY.py`: it scans US option chains, computes premium-selling economics (yield, annualized return, breakeven, cushion, POP, assignment probability, IV/greeks), grades every contract A/B/C with Thai reasons, and streams live prices. On top of it sit OI signals measured from real Webull data (Volume÷OI, daily ΔOI from our own recorder, aggressor side from ticks, IV-implied range, IV vs realized volatility). Each signal must be proven in Option Lab before it affects grades. The app also has an AI agent that proposes opportunities first, without waiting to be asked.

Success = in easy mode, the primary user can answer these 3 questions from the screen alone, without knowing option terms (`design/ux-review-2026-11-25/UX-REVIEW.md` §8):
1. Which NVDA contract is safest for $18,000?
2. What is the worst case if she sells it?
3. What happens to the money if NVDA drops 10% on Friday?

## Positioning

Two things the Python original and generic screeners do not combine: (1) deterministic, fully explainable scoring (10-point rubric → grade A/B/C) where every number is auditable, and (2) an AI copilot that answers questions using only real, live screener data via function-calling tools — it can cite the exact contract it is talking about. Thai plain-language explanations are a first-class product feature, not a translation layer.

## Operating Context

- Users run the app during US market hours from Thailand (evening/night ICT); the app must be readable when the market is closed too (REST snapshot + daily-bars fallback, no fake live data).
- Each user brings their own Webull OpenAPI app key/secret (entered in a first-run setup screen) and optionally their own LLM API key.
- Data source: Webull Thailand OpenAPI — REST (HMAC-SHA1 signed, shared 1.1s throttle) + MQTT v3.1.1 streaming (protobuf payloads; manual integration per official Webull doc; limits: 5 connections/app key, 3 msg/s/connection, manual re-subscribe on reconnect).
- Three-tier data freshness shown honestly on every row: สด (WebSocket) › API (REST snapshot) › ราคาปิด (daily bars).

## Capabilities and Constraints

Confirmed for v1:
- Analyze one ticker at a time (watchlist NVDA, TSLA, AAPL, AMD, SPY, QQQ, MSFT, META; default SPCX as in the original).
- Contract discovery windows 0–7/8–14/15–30/31–60/61–120/121–365/366–1000 DTE, strike band ±30%, OCC symbol parsing.
- Screener economics identical to the original: credit, collateral, yield, annualized, breakeven, cushion, POP = N(d2 at BE), p_itm, p_touch ≈ 2·p_itm, score = ann × pop, 10-point grade rubric with Thai reasons.
- Filters: strategy CSP/CC/both, risk levels by delta band (SAFE 0.10–0.20 / BALANCED 0.15–0.30 / AGGR 0.25–0.40 / ALL), DTE 3–60, max spread 25%, min credit $0.05, budget cap.
- Live price streaming, order-book/tick data for the stock, IV/greeks per contract.
- AI copilot chat panel with function calling (top_candidates, contract_detail, chain_stats) + streaming Thai answers.
- First-run setup screen for Webull keys and LLM key/provider (GLM-5.3 default, OpenAI switchable) — required because the app is distributed to friends/family.
- Thai UI throughout; English ticker/finance terms kept as-is per house style.

Explicit non-goals v1: order placement, backtesting, portfolio tracking, multi-ticker simultaneous streaming, mobile.

Direction update 2026-11-25:
- **Two modes.** Easy mode is the default. It shows one question per screen, the 5-question card (ได้เงินเท่าไร / แลกกับอะไร / โอกาสแค่ไหน / ทำไมช่วงนี้น่าทำ / แย่สุดคืออะไร) with no option jargon in the main text, and the agent in the centre. Detailed mode is the current terminal. Change request: `design/CR-001-easy-mode.md`.
- **Assign stance per stock.** "อยากถือ" stocks such as NVDA use the framing "get paid to wait for a lower price". "ไม่อยากถือ" stocks such as VLO steer away from assignment and explain what to do if it happens anyway.
- **Recorder server.** A Docker crate in this repo shares `radar-core`. The app gets a setting to choose `local` or `api` as the recorder source.
- **Orders move from non-goal to future goal.** They are gated twice. First, a preview spike must prove that SELL_TO_OPEN works on the Trade API. Second, the user confirms every order. Until both gates pass, the app places no orders.
- Portfolio tracking narrows to the after-sale journey for contracts already sold: current status, what to do if the stock falls, and what happens after assignment.

Hard constraints: APP_KEY/APP_SECRET and LLM keys live only in the Rust process (never in frontend code, logs, or chat output); no fabricated data when a feed is silent — degrade honestly with the source badge.

## Brand Commitments

- Name: PREMIUM RADAR.
- UI language: Thai (plain, non-analyst friendly), technical terms in English.
- Visual + structural baseline pinned by THP 2026-11-22: **`design/premium_radar_modern_v2.html` + `design/DESIGN.md` are the only design source of truth** (V2 Modern Visual Refinement). Webull-inspired professional trading terminal: charcoal neutral-dark surfaces (#0b0d11 canvas), flat 1px dividers (no glossy bezels / gradients / heavy shadows), electric blue #4b8cf7 for selection-focus only, green #24c99a / red #f4788c / amber #f0bd70 for financial semantics, Inter + tabular numerals, locked 4-column shell (icon rail 50px / left panel 225px / main / right panel 320px) with structure freeze per DESIGN.md §1.2 and §4 — layout or anatomy changes require an explicit change request.
- Design work is done through the impeccable skill; financial model contracts are corrected independently of the frozen UI (DESIGN.md §8).
- The frozen 4-column shell above becomes the **detailed mode**. Easy mode has its own shell under CR-001 (approved 2026-11-25, DESIGN.md §4.7) and shares the same tokens and data-source badges.

## Evidence on Hand

- Source-of-truth logic: `C:\Users\bfipa\Downloads\Short_Option_Radar_PY.py` (3,507 lines, read in full 2026-11-22).
- Style reference: Webull app screenshot (analyzed 2026-11-22).
- Layout reference: three PREMIUM RADAR web screenshots (analyzed 2026-11-22).
- Webull manual MQTT integration doc + Apache-2.0 Python SDK repos (researched 2026-11-22; recorded in brain `projects/premium-radar-option2`).
- Real API keys exist (Webull app key/secret; GLM + OpenAI) — never copy into the repo or chat.
- Absences: no logo/icon assets yet, no marketing copy, no real user quotes — future work must not fabricate any of these.

## Product Principles

1. Every number on screen is auditable — deterministic scoring the user can re-derive, with the AI only explaining, never inventing, values.
2. Honest data freshness — always show where a price came from (สด/API/ราคาปิด); silence is displayed as silence.
3. Thai plain language is the product — explanations are written for a non-analyst reader, not shortened translations.
4. One decision per screen. The app answers "which contract should I sell, and why" for one ticker before anything else. In easy mode this is a hard rule.
5. Keys and money stay safe. Credentials never leave the Rust process. The app places no orders until the two gates in "Direction update 2026-11-25" have passed, and even then every order needs the user's confirmation.
6. Risk is shown in money. "แย่สุดคืออะไร" is always a dollar amount tied to a scenario that is easy to understand, never only collateral or breakeven.

## Accessibility & Inclusion

Thai-first typography (bundled Thai fonts like the original's Kanit/Sarabun stack) with font-size adjustment; color-coded up/down always paired with signs/labels so red-green blindness never hides direction; investment language kept at non-analyst level per the owner's standing rule.
