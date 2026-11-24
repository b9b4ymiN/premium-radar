# PREMIUM RADAR — UI V2.1 Terminal Surface Refinement
Date: 2026-11-23. Status: applied on branch `zc/premium-radar-ui` (style-only, not yet reviewed).

## Design intent
A more professional dark trading terminal without changing app structure. Use
subtle tonal gradients for global and section headers and restrained alternating
table rows. Keep the structure freeze from design/DESIGN.md and preserve all
financial semantics.

## Adaptation from the review branch
Source: `origin/design/terminal-refinement-api-audit-20261123` (CSS block +
this doc). Changes when porting to the current shell:

- **Dropped the `.main-tabs` rule** — the main-tabs row was removed from the
  JSX (commit `bd91426`, sidebar is the single menu), so the selector styled a
  dead element.
- **Row/chrome colors moved to tokens** in `src/styles/tokens.css`
  (`--chrome-appbar-*`, `--chrome-panel-*`, `--chrome-quote-*`, `--row-*`,
  `--th-label`, `--row-rule`) — values are identical to the reviewed CSS.
- **Added missing legacy aliases** `--divider`, `--muted`, `--accent` — these
  were referenced by the Option Lab / Portfolio / Watchlists / Home modules
  but never declared, so their borders resolved to `currentColor`.

## Applied in src/App.css (additive block at end of file)
- Global app header: charcoal gradient `#1b242f → #111821`
- Panel and table toolbar: `#26313e → #1a222d`
- Quote header: `#18212b → #111820`
- Sticky data-table headers: `#26333f → #1d2631`, label contrast `#c5d0dd`
  (sticky `position/top/z-index` come from the existing base rule, unchanged)
- Rows: `#111820` odd, `#151d26` even, with faint 1px column rules
- Hover: `#243546`; selection: `#203a55` with blue left rail; selected+hover
  `#294661`
- Table status/footer strips: `#171f29`
- No glossy cards, heavy shadows or full-screen decorative gradients

## Constraints
No JSX, navigation, column layout, padding/heights, financial formulas or
Rust/Tauri commands modified. Changes apply to the existing Options Monitor
screener table and the other tables that share the `screener-table` selector
(Home overview, Watchlists, Option Lab payoff table).

## Verification (run on this branch, mock/synthetic browser mode only)
- `npm run check` — see report
- `npm run build` — see report
- `git diff --check` — see report
- Shell widths preserved at 1024: workspace `44px 188px minmax(400px,1fr)`,
  right panel hidden per the existing ≤1150px media query
- Screenshots (before/after, mock mode) under `tmp/v21-css/`:
  monitor at 1440/1366/1024 and Home page (representative second table) at 1440

## Review gates still open
- Inspect hover, selected, sticky scrolling, empty/loading/error states by eye
- Verify contrast of small header typography on low-brightness screens
- Confirm green/red financial values stay distinguishable
- Only merge after independent review and screenshot acceptance

This is a narrow visual amendment to the frozen structure of design/DESIGN.md.

---

# V2.2 — Broker data on the existing surfaces

Date: 2026-11-24. Status: applied on branch `feat/webull-api-expansion`
(commits `88031bb`, `0de8120`, `4007755`, `41d5d1a`), validated against the
live Thailand host.

## What stayed fixed
The four-column shell, the icon rail as the single menu, the scanner workflow
(left panel → scanner table → Contract Analysis → Radar Assistant), the
routing and the Rust/Tauri command contracts. No new navigation model, no tab
row, no new top-level page outside the rail.

## Where each new piece of data went

| Data | Surface | Why there |
|---|---|---|
| Session (regular / pre-market / after-hours / overnight) and the extended-hours trade | Quote header, under the big price | It qualifies the price the header already shows. The big price and its change stay the scanner's regular-session numbers. |
| Earnings window, ex-dividend date, 52-week range, market cap | Quote header, second stat row | The four facts an option seller checks first; they fit beside Open/High/Low/Prev close. The header stays about 75 px tall (two rows instead of one). |
| Company profile, analyst target and rating mix, dividend calendar | Company sheet that unfolds under the header (`ⓘ`) | Needed more room than a header allows, but not a page of its own: it is context for the symbol being scanned, so it opens in place and closes again. |
| Webull's own quote for the selected contract (sizes, open interest, greeks, timestamps) and its last trades | Contract Analysis, above the payoff | It belongs next to the model values it is compared with. Display only. |
| Earnings and ex-dividend risk notes | Contract Analysis, above the Webull quote | A warning about this contract belongs to this contract. |
| Screener ranks and sectors | Market page — the rail slot that was disabled | The prototype already had the slot (`◔`) with the page missing; this fills it. Rows open the stock in the scanner. |
| Movers | Home, a three-column strip | A summary belongs on the landing page; the full lists live on Market. |
| Open orders, balance failure | Portfolio | Same page, one more read-only table; a failed balance call no longer hides positions. |
| Broker watchlists | Watchlists page, below the local list | The page is about lists. The broker's lists are read only; the one action copies a symbol into the local list. |
| Sortable scanner columns | Scanner table header | Ordering only; no value is recomputed. |

## Typography
- Scanner table body 10.8 px → 11.4 px, header 9.5 px → 10 px; chat bubbles
  10.7 → 11.2 px; small print under tables 8.5 px → 10 px.
- Checked at 1920×1080, 1680×950, 1440×900 and 1290×760 (the window's minimum
  is 1290×760). The header's subtitle truncates with an ellipsis at 1290.

## States
- Loading, empty and error states exist for every new surface. An empty
  scanner explains itself once the chain is loaded: when the cheapest contract
  costs more than the budget it says so with both numbers.
- Without the desktop shell (browser preview) the new surfaces say that the
  data is shown in the app; they never show an endless "loading".
- A value the broker did not send is shown as `--`, never as 0.

## Colour
Unchanged rules: green and red for positive and negative values (including the
analyst buy/sell mix), blue for selection and focus, amber for warnings.
The only new coloured elements are the session dot (green regular, amber
extended, grey closed/overnight, red unknown) and the analyst rating bar.

## Screenshots
`tmp/shots/` (git-ignored): `before-*` and `after-*` are the base commit
`f2fee8b` and this branch in browser preview at the four sizes; `live-*` are
the real app against the live host. The portfolio capture has the account
label, the account number and the amounts hidden.
