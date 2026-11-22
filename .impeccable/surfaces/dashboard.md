---
version: 1
slug: "dashboard"
primary_target: "dashboard"
related_targets: []
---

# Surface brief — Main Radar Dashboard (primary surface)

## Scope and visitor mode

Operate. The app's core screen: scan one ticker's option chain, rank CSP/CC candidates, explain the pick in Thai, stream live prices. Used every US trading session, evening/night Thailand time, often in a dim room.

## Audience, job, action, proof, constraints

- Audience: THP + friends/family — Thai retail premium sellers, non-analysts.
- Job: "Which contract should I sell on NVDA tonight, and why am I safe?"
- Action: pick a ticker → read opportunity cards → drill into table row → read contract analysis + payoff → ask Radar Assistant follow-ups.
- Proof: live Webull data (source badge per row), deterministic grades, auditable numbers.
- Constraints: Thai-first copy, honest data freshness, keys invisible, one ticker at a time, dense data must stay scannable.

## Direction contract

THESIS: A professional Webull-inspired trading terminal — four locked columns, information-dense, every pixel answering "which contract and why". Refuses: marketing-dashboard styling (heavy shadows, gradients, glass, glossy bezels, decorative color).

OWN-WORLD: Charcoal neutral-dark surfaces — canvas #0b0d11, chrome #14171d, side #11151b — separated by flat 1px dividers (#292f39 / #222832), never by card shadows. Electric blue #4b8cf7 carries selection/focus/active only; green #24c99a / red #f4788c / amber #f0bd70 carry financial meaning, always paired with text or sign. Inter + Inter Display with tabular numerals; body 12px, quote price 30px/730, table cells 10.8px. Radius 2–9px; rows and dividers, not tiles.

STORY: Trader opens app → quote header (price 30px, OHLC right) → candlestick chart + options statistics (IV, expected move, skew, OI split) → three opportunity cards (Defensive / Balanced / High Yield) → scanner table (13 cols) or option-chain view (Calls|Strike|Puts) → right rail: contract analysis (premium, collateral, breakeven, probability bar, payoff SVG, risk note) over Radar Assistant chat with quick-ask chips. Status bar carries honest data-state (OFFLINE/DEMO until a verified feed exists).

FIRST VIEWPORT (full shell, structure freeze per design/DESIGN.md §4): appbar 43px; workspace grid 50px icon rail / 225px left panel (watchlist w/ sparklines, strategy, risk, expiry, cash, data-mode foot) / flexible main (tabs 34px → quote 75px → topgrid 258px chart+snapshot 250px → opportunities 88px ×3 → tablemodule scroll) / 320px right panel (detail 57% + copilot); statusbar 25px. Breakpoints 1420/1150/790 hide right panel, then left panel, per source CSS.

FORM: premium_radar_modern_v2.html — user-provided approved prototype, **structure freeze** (design/DESIGN.md §1.2 locks shell, section order, right-panel stacking, responsive behavior, interaction model). Seed key: pinned:modern-v2-prototype (user-supplied files dated 2026-11-22; no roll). Layout/anatomy changes require an explicit change request per DESIGN.md §8.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.

## Unresolved decisions

- Thai font final pick at implementation (source has no webfont: Inter/Noto Sans Thai fallback per OS; bundle for pixel consistency without changing sizes).
- Copilot currently rule-based demo — production wires GLM-5.3/OpenAI function calling behind the same chat anatomy.
- Analytics 3×2 grid page (from other prototype) is NOT in this contract — separate change request if wanted.
