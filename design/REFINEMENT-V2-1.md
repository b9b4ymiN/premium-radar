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
