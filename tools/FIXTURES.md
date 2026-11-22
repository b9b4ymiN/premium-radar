# Golden-test fixtures

Deterministic fixtures produced by executing the REAL Python reference implementation
`C:\Users\bfipa\Downloads\Short_Option_Radar_PY.py` ("PREMIUM RADAR", 3,506 lines).
A Rust port must reproduce these numbers exactly (prices to 1e-6, grades exactly).

## Regenerate

```
python tools/dump_fixtures.py
```

Writes/replaces everything in `tests/fixtures/`. The run is fully deterministic: no clock,
no randomness, no network (pure-function paths only). Byte-identical output across runs.

## Frozen reference time

**2026-11-22T17:00:00 America/New_York** (= `2026-11-22T22:00:00+00:00` UTC; Nov 22 2026 is
after US DST end, so NY = EST/UTC-5). The dump script monkeypatches the module-global
`datetime` with a subclass whose `now()`/`utcnow()` always return this instant, which pins
`years_to_expiry()` — the only clock consumer in the exercised code paths. All expiries in
the generated data sit 5–89 days after the reference (DTE window 3–120 requested; also
includes one past expiry 2026-01-17 in occ_parse to pin the 3600-second clamp).

Pinned environment at import time (the module reads these via `os.getenv`):
`RISK_FREE_RATE=0.043` (default), `PR_THEME=midnight`. Actual values used are recorded in
every file under `meta` (`risk_free_rate`, `div_yield`, `min_dte`, `max_dte`,
`max_spread_pct`, `min_credit`, `risk_levels`).

## Import workarounds

None needed as stubs. The module imports tkinter and calls `matplotlib.use("TkAgg")` at
module level, but only instantiates `Dashboard()` under `if __name__ == "__main__"`, so a
plain `importlib` import under a different module name is headless-safe. The only other
module-level I/O (`load_settings()` reading `radar_settings.json`) is wrapped in try/except
and returns `{}`.

## Files

| File | Cases | Contents |
|---|---|---|
| `quant_bs.json` | 159 | `bs_price` + `bs_greeks` grid (ATM/ITM/OTM x T {0.01..1.0} x vol {0.1,0.3,0.5,1.2} x C/P) + edge cases (deep ITM/OTM, tiny T, zero T, zero vol, 3y). Expected: price, delta, gamma, vega, theta. |
| `quant_iv.json` | 172 | Round-trips `bs_price` at a known vol through `implied_vol` (7 strikes x 4 T x 3 vols x C/P) + 4 pathological inputs (below intrinsic, zero price, above vol-5 ceiling, T=0) expecting `null`. |
| `occ_parse.json` | 34 | `parse_occ` on real OCC symbols (1–6 char roots incl. dots, C/P, leading-zero strikes) + invalid ones (`null`). Expected: underlying/option_type/strike, expiry date, NY-16:00 UTC instant, `t_years`, `dte_days` (= `t_years*365`, the value `_enrich_rows_raw` stores as `r["dte"]`). |
| `enrich_grade.json` | 402 rows / 4 case groups | `_enrich_rows_raw` (+ `grade_pick`, which it calls internally) on synthetic chains at spots 182.45 and 58.12, strikes ±30%, 4 expiries (DTE ~5/12/34/89), both types, mixed data quality (missing bid/ask, zero price, zero OI/volume). Full enriched row dicts incl. credit, collateral, yld, ann, be, cushion, pop, p_itm, p_touch, score, grade, why, delta, iv, dte. |
| `screener_filters.json` | 17 scenarios | `short_candidates` over the enriched chains: mode CSP/CC/CSP+CC x RISK_LEVELS (SAFE/BALANCED/AGGR/ALL), expiry filter, budget (partial collateral filter), relax flag. Captures BOTH the function's unsorted input-order output and the app pipeline (budget filter + stable sort by score desc). |
| `chain_stats.json` | 8 scenarios | `chain_stats` / `atm_iv_for` / `otm_curve` per (chain, expiry): atm_iv, em, skew, atm T, OTM curve; plus an all-iv-None scenario returning all nulls. |

## JSON rules

Floats at full Python repr precision (shortest round-trip); `None` -> `null`; datetimes ->
ISO strings. No NaN/Infinity appears in these paths; the dumper would convert any to `null`
and flag it in `notes` (never triggered to date).

## Gotchas the Rust port must replicate

- `bs_greeks` vega is per 1 vol **point** (/100) and theta per **calendar day** (/365).
- `bs_greeks` has **no** T<=0 / vol<=0 guard (Python raises ZeroDivisionError); `bs_price`
  returns the undiscounted spot intrinsic there.
- `implied_vol` rejects prices below the **discounted-forward** intrinsic (`fwd_s - disc_k`),
  bisection [1e-4, 5.0] x80 iters, tolerance 1e-6, final bounds (0.01, 4.99).
- IV in `_enrich_rows_raw` is solved from `price` (last), never from bid; credit falls back
  bid -> price; `grade`/`why` only exist when `pop` is not None.
- `short_candidates` does **not** sort — it preserves input row order.
- `years_to_expiry` clamps negative remaining time to 3600 s (t = 1/8760 for past expiries).
- Expiry instant is 16:00 **America/New_York** converted to UTC (21:00Z in EST).
