r"""
tools/dump_fixtures.py - deterministic golden-test fixtures from the REAL Python reference
implementation (C:/Users/bfipa/Downloads/Short_Option_Radar_PY.py, "PREMIUM RADAR").

Regenerate:  python tools/dump_fixtures.py      (writes tests/fixtures/*.json)

Import workarounds (none were ultimately needed as stubs):
  * The module imports tkinter and calls matplotlib.use("TkAgg") at module level, but only
    instantiates Dashboard() under `if __name__ == "__main__":` (line 3505), so importing it
    via importlib with a different module name is headless-safe on this machine. No stubbing.
  * load_settings() at module level reads radar_settings.json next to the source file inside a
    try/except -> returns {} when absent; harmless.
  * The module reads os.getenv("RISK_FREE_RATE") (default "0.043") and os.getenv("PR_THEME")
    at import time. We force both env vars BEFORE import so the dump is deterministic
    regardless of the host environment; the actual RISK_FREE_RATE/DIV_YIELD values used are
    recorded in every fixture file under "meta".

Frozen clock:
  * Reference time is 2026-11-22T17:00:00 America/New_York (= 2026-11-22T22:00:00Z; Nov 22
    2026 is after the US DST end, so NY is EST/UTC-5). This matches the environment's real
    date so generated expiry dates land in realistic windows.
  * We monkeypatch the module-global `datetime` name with a FrozenDateTime subclass whose
    .now(tz) always returns the frozen instant (converted to tz; naive when tz is None).
    This freezes years_to_expiry() (the only clock consumer in the fixture code paths).
    datetime instances constructed by parse_occ()/expiry_utc() are unaffected in value
    (the subclass only overrides now()/utcnow()).

Conventions discovered from the source (Rust port must match):
  * bs_price(cp, S, K, T, r, q, sig)   cp is "C" or "P"; T/r/q/sig are floats (T in years).
  * bs_greeks(cp, S, K, T, r, q, sig)  -> (delta, gamma, vega, theta) tuple; vega is per 1
    VOL POINT (i.e. /100), theta is per CALENDAR DAY (i.e. /365). No T<=0 / sig<=0 guard:
    it raises ZeroDivisionError - those inputs are deliberately excluded from quant_bs.json
    (see "notes" in that file).
  * implied_vol(cp, price, S, K, T, r=RISK_FREE_RATE, q=DIV_YIELD) -> float fraction
    (0.45 == 45%) or None (price<=0, price < discounted-forward intrinsic - 1e-6,
    vol-5.0 price still below market, or solved iv outside (0.01, 4.99)).
  * parse_occ(symbol) -> {"underlying","expiry" (naive datetime),"option_type","strike"}
    or None. Regex ^([A-Z0-9.]{1,6})(\d{2})(\d{2})(\d{2})([CP])(\d{8})$ (dots allowed in
    the root, year is 20YY, strike = int/1000).
  * _enrich_rows_raw(rows, spot) enriches IN PLACE and calls grade_pick itself whenever
    pop is not None; required input keys: symbol, price, bid, ask, oi, volume, change,
    change_ratio (+ OCC meta underlying/expiry/option_type/strike). "price" (not bid) is
    the IV solve input; credit falls back bid -> price.
  * short_candidates(rows, spot, mode, sel_exp, all_exp, dmin, dmax, relax) does NOT sort -
    output preserves input row order. The app additionally filters collateral<=budget and
    sorts by score desc (stable). screener_filters.json captures BOTH orders.
  * chain_stats(rows, spot, sel_exp) -> {"atm_iv","em","skew"}; atm_iv_for -> (iv, T).
    otm_curve(rows, spot) -> sorted [(strike, iv*100)] (only OTM side per type).

All floats are serialized with Python's default json repr (shortest round-trip, full
precision). No NaN/Infinity is produced by these code paths; a guard converts any such
value to null and flags it in "notes" (none trigger today).
"""

import importlib.util
import json
import math
import os
import sys
from datetime import datetime as _RealDateTime, timedelta, timezone

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SOURCE_PATH = r"C:\Users\bfipa\Downloads\Short_Option_Radar_PY.py"
OUT_DIR = os.path.join(ROOT, "tests", "fixtures")

REF_NY_TEXT = "2026-11-22T17:00:00 America/New_York"
REF_UTC = _RealDateTime(2026, 11, 22, 22, 0, 0, tzinfo=timezone.utc)  # 17:00 EST

# ---------------------------------------------------------------- import the module
os.environ["RISK_FREE_RATE"] = "0.043"   # module reads this at import time (default "0.043")
os.environ["PR_THEME"] = "midnight"      # colors only; pinned for determinism anyway

spec = importlib.util.spec_from_file_location("radar_ref", SOURCE_PATH)
R = importlib.util.module_from_spec(spec)
sys.modules["radar_ref"] = R
spec.loader.exec_module(R)


class FrozenDateTime(_RealDateTime):
    """datetime subclass that pins now()/utcnow() to the frozen reference instant."""

    @classmethod
    def now(cls, tz=None):
        if tz is None:
            return REF_UTC.replace(tzinfo=None)
        return REF_UTC.astimezone(tz)

    @classmethod
    def utcnow(cls):
        return REF_UTC.replace(tzinfo=None)


R.datetime = FrozenDateTime  # years_to_expiry() resolves `datetime` through module globals

RF, QY = R.RISK_FREE_RATE, R.DIV_YIELD


# ---------------------------------------------------------------- helpers
def iso_or_none(v):
    return v.isoformat() if isinstance(v, _RealDateTime) else None


def clean(obj):
    """Recursively convert datetimes to ISO strings; map NaN/Inf to None (flagged)."""
    bad = []

    def walk(o):
        if isinstance(o, _RealDateTime):
            return o.isoformat()
        if isinstance(o, float):
            if math.isnan(o) or math.isinf(o):
                bad.append(o)
                return None
            return o
        if isinstance(o, dict):
            return {k: walk(v) for k, v in o.items()}
        if isinstance(o, (list, tuple)):
            return [walk(v) for v in o]
        return o

    out = walk(obj)
    return out, bad


def dump(name, payload):
    os.makedirs(OUT_DIR, exist_ok=True)
    cleaned, bad = clean(payload)
    if bad:
        cleaned.setdefault("notes", []).append(
            f"{len(bad)} non-finite float(s) converted to null: {bad!r}")
    path = os.path.join(OUT_DIR, name)
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(cleaned, fh, ensure_ascii=False, indent=1, allow_nan=False)
    print(f"wrote {name}: {os.path.getsize(path):,} bytes")


def occ_symbol(root, expiry, cp, strike):
    return f"{root}{expiry:%y%m%d}{cp}{int(round(strike * 1000)):08d}"


def round_step(x, step):
    return round(x / step) * step


# frozen reference info shared by every file
META = {
    "reference_time": REF_NY_TEXT,
    "reference_time_utc": REF_UTC.isoformat(),
    "risk_free_rate": RF,
    "div_yield": QY,
    "min_delta": R.MIN_DELTA, "max_delta": R.MAX_DELTA,
    "risk_levels": {k: list(v) for k, v in R.RISK_LEVELS.items()},
    "min_dte": R.MIN_DTE, "max_dte": R.MAX_DTE,
    "max_spread_pct": R.MAX_SPREAD_PCT, "min_credit": R.MIN_CREDIT,
}

# ---------------------------------------------------------------- 1. quant_bs.json
def gen_quant_bs():
    Ts = [0.01, 0.05, 0.1, 0.25, 0.5, 1.0]
    vols = [0.1, 0.3, 0.5, 1.2]
    S = 182.45
    cases = []
    for moneyness, tag in ((1.00, "ATM"), (1.20, "ITM"), (0.80, "OTM")):
        K = round_step(S * moneyness, 0.5)
        for T in Ts:
            for vol in vols:
                for cp in ("C", "P"):
                    price = R.bs_price(cp, S, K, T, RF, QY, vol)
                    d, g, v, th = R.bs_greeks(cp, S, K, T, RF, QY, vol)
                    cases.append({
                        "tag": tag, "cp": cp, "S": S, "K": K, "T": T,
                        "r": RF, "q": QY, "vol": vol,
                        "expected": {"price": price, "delta": d, "gamma": g,
                                     "vega": v, "theta": th},
                    })
    # edge cases: deep ITM/OTM, tiny T, tiny vol, big vol. (bs_greeks raises
    # ZeroDivisionError for T<=0 or vol<=0 -> only bs_price is exercised there.)
    edges = [
        ("deep_ITM_call", "C", S, 5.0, 0.25, 0.3), ("deep_ITM_put", "P", S, 800.0, 0.25, 0.3),
        ("deep_OTM_call", "C", S, 800.0, 0.25, 0.3), ("deep_OTM_put", "P", S, 5.0, 0.25, 0.3),
        ("tiny_T", "C", S, round_step(S, 0.5), 0.0001, 0.3),
        ("tiny_T_put", "P", S, round_step(S, 0.5), 0.0001, 0.3),
        ("tiny_vol", "C", S, 150.0, 0.25, 0.001), ("big_vol", "C", S, 150.0, 0.25, 4.5),
        ("long_T", "P", S, 210.0, 3.0, 0.45), ("one_day_ATM", "C", S, round_step(S, 0.5), 1 / 365.0, 0.5),
        ("zero_T_call", "C", S, 150.0, 0.0, 0.3), ("zero_T_put", "P", S, 150.0, 0.0, 0.3),
        ("zero_T_OTM", "C", S, 200.0, 0.0, 0.3), ("zero_vol", "P", S, 150.0, 0.25, 0.0),
        ("high_r_like", "P", 100.0, 100.0, 0.5, 0.3),
    ]
    for tag, cp, s, k, T, vol in edges:
        price = R.bs_price(cp, s, k, T, RF, QY, vol)
        if T > 0 and vol > 0:
            d, g, v, th = R.bs_greeks(cp, s, k, T, RF, QY, vol)
            greeks = {"delta": d, "gamma": g, "vega": v, "theta": th}
        else:
            greeks = None  # bs_greeks raises ZeroDivisionError here (documented)
        cases.append({"tag": tag, "cp": cp, "S": s, "K": k, "T": T, "r": RF, "q": QY,
                      "vol": vol, "expected": {"price": price, "greeks": greeks}})
    payload = {"meta": dict(META, S_note="grid uses S=182.45, K rounded to 0.5 steps"),
               "conventions": {
                   "cp": "option type: \"C\" call / \"P\" put (first positional arg)",
                   "T": "years to expiry (float)",
                   "vega": "per 1 vol POINT (price change per 0.01 change in vol)",
                   "theta": "per CALENDAR DAY (annual theta / 365)",
                   "zero_guard": "bs_price returns undiscounted spot intrinsic when T<=0 or vol<=0; "
                                 "bs_greeks has NO guard and raises ZeroDivisionError for T<=0 or vol<=0 "
                                 "(expected.greeks is null in those edge cases)",
               },
               "cases": cases}
    return payload, len(cases)


# ---------------------------------------------------------------- 2. quant_iv.json
def gen_quant_iv():
    S = 182.45
    cases = []
    for K in (round_step(S * m, 0.5) for m in (0.8, 0.9, 0.95, 1.0, 1.05, 1.1, 1.2)):
        for T in (0.02, 0.1, 0.5, 1.5):
            for vol in (0.18, 0.42, 0.85):
                for cp in ("C", "P"):
                    p = R.bs_price(cp, S, K, T, RF, QY, vol)
                    cases.append({
                        "cp": cp, "S": S, "K": K, "T": T, "r": RF, "q": QY,
                        "vol_used_for_price": vol, "market_price": p,
                        "expected_iv": R.implied_vol(cp, p, S, K, T, r=RF, q=QY),
                    })
    # pathological: price below discounted-forward intrinsic / zero / absurd / T<=0
    intrinsic_call = max(S * math.exp(-QY * 0.5) - 80.0 * math.exp(-RF * 0.5), 0.0)
    patho = [
        {"cp": "C", "S": S, "K": 80.0, "T": 0.5, "r": RF, "q": QY, "vol_used_for_price": None,
         "market_price": round(intrinsic_call * 0.5, 4), "expected_iv":
             R.implied_vol("C", round(intrinsic_call * 0.5, 4), S, 80.0, 0.5, r=RF, q=QY),
         "note": "price < discounted-forward intrinsic -> None"},
        {"cp": "C", "S": S, "K": 150.0, "T": 0.25, "r": RF, "q": QY, "vol_used_for_price": None,
         "market_price": 0.0, "expected_iv": R.implied_vol("C", 0.0, S, 150.0, 0.25, r=RF, q=QY),
         "note": "zero price -> None"},
        {"cp": "C", "S": S, "K": round_step(S, 0.5), "T": 0.01, "r": RF, "q": QY,
         "vol_used_for_price": None, "market_price": 90.0,
         "expected_iv": R.implied_vol("C", 90.0, S, round_step(S, 0.5), 0.01, r=RF, q=QY),
         "note": "price above the vol=5.0 ceiling -> None"},
        {"cp": "P", "S": S, "K": 150.0, "T": 0.0, "r": RF, "q": QY, "vol_used_for_price": None,
         "market_price": 2.0, "expected_iv": R.implied_vol("P", 2.0, S, 150.0, 0.0, r=RF, q=QY),
         "note": "T<=0 -> None"},
    ]
    cases.extend(patho)
    payload = {"meta": META,
               "conventions": {
                   "return": "implied vol as a float fraction (0.45 == 45%), or null when unsolvable",
                   "solver": "80-iteration bisection on [1e-4, 5.0], tolerance hi-lo < 1e-6; "
                             "rejects iv outside (0.01, 4.99)",
                   "intrinsic_check": "None when market_price < max(fwd_s - disc_k, 0) - 1e-6 "
                                      "(discounted FORWARD intrinsic, not spot intrinsic)",
               },
               "cases": cases}
    return payload, len(cases)


# ---------------------------------------------------------------- 3. occ_parse.json
def gen_occ_parse():
    symbols = [
        "NVDA261218P00175000", "NVDA261218C00175000", "TSLA270116C00090000",
        "AAPL261127P00150000", "F270116C00020000",                    # 1-char root
        "AB261127P00005000",                                        # 2-char root
        "SPCX261204C00100000", "MSFT261127C00210000", "META270219P00250000",
        "BRK.B261218C00300000",                                     # dot inside root (regex allows [A-Z0-9.])
        "ALL261127P00010000",                                       # "ALL" + digits, regex-greedy test
        "QQQ270115C00200000", "AMD261127P00097500",                  # half-dollar strike 97.5
        "SPY261127C00060250",                                       # 602.50 with leading zeros
        "XY270116P00012345",                                        # 12.345 fractional strike
        "NVDA260117P00175000",                                      # PAST expiry (2026-01-17) -> clamp path
        "NVDA261231P00175000", "NVDA270130C00175000",
        "T261127C00025000", "GOOG270219P00100000",
        "SPY261127P00000001",                                       # strike 0.001
        "SPY261127P00001234",                                       # strike 12.34
        "MSFT271231C00500000",                                      # dte ~404 (beyond MAX_DTE)
        "BABA261218C00095000",
        # invalid ones -> None
        "nvda261218P00175000",      # lowercase
        "NVDA261218X00175000",      # bad type letter
        "NVDA261218P175000",        # strike not 8 digits
        "NVDA261318P00175000",      # month 13
        "NVDA262318P00175000",      # invalid day 23? no: yy=26 mm=23 invalid month -> None
        "TOOLONG7261218P00175000",  # root > 6 chars
        "NVDA 261218P00175000",     # space (stripped) -> parses as NVDA261218P00175000? test it
        "", None, 12345,
    ]
    cases = []
    for sym in symbols:
        m = R.parse_occ(sym)
        if m is None:
            cases.append({"symbol": sym if isinstance(sym, str) else repr(sym), "expected": None})
            continue
        exp_utc = R.expiry_utc(m["expiry"])
        t = R.years_to_expiry(m["expiry"])
        cases.append({
            "symbol": sym,
            "expected": {
                "underlying": m["underlying"],
                "option_type": m["option_type"],
                "strike": m["strike"],
                "expiry_date": m["expiry"].date().isoformat(),          # naive date from parse_occ
                "expiry_ny_1600_utc": exp_utc.isoformat(),              # instant used by the math
                "t_years": t,                                           # years_to_expiry at frozen ref
                "dte_days": t * 365.0,                                  # what _enrich_rows_raw stores as r["dte"]
                "dte_days_int": int(t * 365.0),
            },
        })
    payload = {"meta": META,
               "conventions": {
                   "regex": r"^([A-Z0-9.]{1,6})(\d{2})(\d{2})(\d{2})([CP])(\d{8})$",
                   "root": "1-6 chars of [A-Z0-9.]; dots allowed; year is 20YY; strike=int/1000",
                   "clamp": "years_to_expiry clamps negative/zero remaining seconds to 3600 "
                            "(so past expiries give t=1/8760, dte~=0.11) - see the 2026-01-17 case",
                   "space": "leading/trailing spaces are stripped before matching",
               },
               "cases": cases}
    return payload, len(cases)


# ---------------------------------------------------------------- shared synth chains
MONEYNESS = (-0.30, -0.25, -0.20, -0.15, -0.10, -0.075, -0.05, -0.03, -0.015, 0.0,
             0.015, 0.03, 0.05, 0.075, 0.10, 0.15, 0.20, 0.25, 0.30)
# frozen-ref-relative expiries: dte ~5 / 12 / 34 / 89
EXPIRIES = {
    "E1": _RealDateTime(2026, 11, 27),   # Fri,  ~5d
    "E2": _RealDateTime(2026, 12, 4),    # Fri, ~12d
    "E3": _RealDateTime(2026, 12, 26),   # Sat, ~34d
    "E4": _RealDateTime(2027, 2, 19),    # Fri, ~89d
}


def synth_chain(root, spot, base_vol, expiries, moneyness=MONEYNESS, step=2.5,
                degrade=True, wide_spread=False):
    """Deterministic synthetic chain: prices come from bs_price under a fixed vol surface."""
    strikes = sorted({round_step(spot * (1 + m), step) for m in moneyness})
    rows, i = [], 0
    for exp in expiries:
        for K in strikes:
            for cp in ("C", "P"):
                T = R.years_to_expiry(exp)
                m = math.log(K / spot)
                vol = (base_vol + 0.55 * max(0.0, -m) + 0.12 * max(0.0, m)
                       + 0.10 * max(0.0, 0.25 - T) + (0.015 if cp == "P" else 0.0))
                mid = R.bs_price(cp, spot, K, T, RF, QY, vol)
                sf = (0.30 if wide_spread else 0.04) + 0.20 * abs(m) + 0.10 * max(0.0, 0.1 - T)
                bid = max(round(mid * (1.0 - sf / 2.0), 2), 0.0)
                ask = max(round(mid * (1.0 + sf / 2.0), 2), 0.0)
                price = round(mid, 2)
                oi = float(1200 - 17 * (i % 40))
                volume = float(90 + 13 * (i % 23))
                if degrade:
                    if i % 9 == 4:                       # no quotes -> credit falls back to price
                        bid = ask = 0.0
                    if i % 13 == 5:                      # no last price -> IV unsolvable, credit from bid
                        price = 0.0
                    if i % 11 == 7:                      # zero OI
                        oi = 0.0
                    if i % 17 == 3:                      # zero volume
                        volume = 0.0
                change = round(mid * 0.04 * (1 if i % 2 == 0 else -1), 4)
                row = {
                    "symbol": occ_symbol(root, exp, cp, K),
                    "price": price, "volume": volume, "change": change,
                    "change_ratio": change / max(mid, 0.05),
                    "oi": oi, "bid": bid, "ask": ask, "src": "REST",
                }
                row.update(R.parse_occ(row["symbol"]))   # merge OCC meta exactly like the app
                rows.append(row)
                i += 1
    rows.sort(key=lambda r: (r["expiry"], r["strike"], r["option_type"]))  # app ordering
    return rows


ENRICH_FIELDS = ("T", "dte", "chg_pct", "iv", "delta", "gamma", "vega", "theta", "pop",
                 "credit", "collateral", "yld", "ann", "be", "cushion", "p_itm", "p_touch",
                 "sig_dist", "strike_dist", "day_inc", "if_called", "if_called_ann", "basis",
                 "score", "spread_pct", "prem", "net_cash", "max_loss", "max_profit",
                 "grade", "why", "vol_oi")


def serialize_row(r):
    out = {k: r[k] for k in ("symbol", "option_type", "strike", "price", "bid", "ask",
                             "oi", "volume", "change", "change_ratio", "src", "underlying")}
    out["expiry"] = r["expiry"].date().isoformat()
    for k in ENRICH_FIELDS:
        out[k] = r.get(k)
    return out


def build_cases():
    """4-6 synthetic case groups (spot + expiry-set combos) covering data-quality mixes."""
    cases = []
    # A: NVDA-like spot 182.45, all 4 expiries, full moneyness, mild degradation
    rows_a = synth_chain("NVDA", 182.45, 0.42, [EXPIRIES[e] for e in ("E1", "E2", "E3", "E4")])
    cases.append({"name": "A_NVDA_182.45_all4", "spot": 182.45, "rows": rows_a})
    # B: SIRI-like spot 58.12, all 4 expiries, higher vol base, 0.5 strike step
    rows_b = synth_chain("SIRI", 58.12, 0.55, [EXPIRIES[e] for e in ("E1", "E2", "E3", "E4")], step=0.5)
    cases.append({"name": "B_SIRI_58.12_all4", "spot": 58.12, "rows": rows_b})
    # C: degraded data (more fallbacks), spot 182.45, near expiries only
    rows_c = synth_chain("NVDA", 182.45, 0.38, [EXPIRIES["E1"], EXPIRIES["E3"]])
    for j, r in enumerate(rows_c):
        if j % 5 in (2, 3):
            r["price"] = 0.0
        if j % 4 == 1:
            r["bid"] = r["ask"] = 0.0
    cases.append({"name": "C_NVDA_182.45_degraded", "spot": 182.45, "rows": rows_c})
    # D: spot 58.12, single 12d expiry, tight strikes, WIDE spreads (spread_pct > 25 filter)
    rows_d = synth_chain("SIRI", 58.12, 0.50, [EXPIRIES["E2"]],
                         moneyness=(-0.10, -0.075, -0.05, -0.03, -0.015, 0.0, 0.015, 0.03, 0.05, 0.075, 0.10),
                         step=0.5, wide_spread=True)
    cases.append({"name": "D_SIRI_58.12_12d_widesp", "spot": 58.12, "rows": rows_d})
    return cases


# ---------------------------------------------------------------- 4. enrich_grade.json
def gen_enrich_grade(cases):
    out_cases = []
    for c in cases:
        rows = [dict(r) for r in c["rows"]]          # fresh copies: _enrich_rows_raw mutates in place
        R._enrich_rows_raw(rows, c["spot"])          # grade_pick runs inside (when pop is not None)
        out_cases.append({"name": c["name"], "spot": c["spot"],
                          "rows": [serialize_row(r) for r in c["rows"]],
                          "expected": [serialize_row(r) for r in rows]})
    payload = {"meta": META,
               "conventions": {
                   "input_rows": "keys exactly as the app builds them: {symbol,price,volume,change,"
                                 "change_ratio,oi,bid,ask,src} merged with parse_occ() meta "
                                 "{underlying,expiry,option_type,strike}",
                   "in_place": "_enrich_rows_raw mutates rows in place and returns the same list",
                   "iv_input": "IV is solved from r['price'] (last), never from bid; credit falls "
                               "back bid -> price; grade/why set by grade_pick() whenever pop is not None",
                   "grade": "grade_pick returns ('A'|'B'|'C', why-text Thai); A needs >=7 pts, B >=4",
               },
               "cases": out_cases}
    return payload, sum(len(c["rows"]) for c in cases)


# ---------------------------------------------------------------- 5. screener_filters.json
def app_pipeline(rows, spot, mode, sel_exp, dmin, dmax, relax, budget):
    """Exactly what Dashboard._candidates does: short_candidates -> budget -> stable score sort."""
    c = R.short_candidates(rows, spot, mode, sel_exp=sel_exp, all_exp=sel_exp is None,
                           dmin=dmin, dmax=dmax, relax=relax)
    if budget > 0:
        c = [r for r in c if r["collateral"] <= budget]
    c = sorted(c, key=lambda r: r.get("score") or 0, reverse=True)  # list.sort is stable
    return c


def slim(r):
    return {"symbol": r["symbol"], "grade": r.get("grade"), "score": r.get("score"),
            "delta": r.get("delta"), "dte": r.get("dte"), "credit": r.get("credit"),
            "collateral": r.get("collateral"), "ann": r.get("ann"), "pop": r.get("pop"),
            "spread_pct": r.get("spread_pct"), "option_type": r["option_type"]}


def gen_screener(cases):
    # (re)enrich fresh copies per scenario so mutation order can never leak between scenarios
    def enriched(name):
        c = next(x for x in cases if x["name"] == name)
        rows = [dict(r) for r in c["rows"]]
        R._enrich_rows_raw(rows, c["spot"])
        return rows, c["spot"]

    rows_a, spot_a = enriched("A_NVDA_182.45_all4")
    rows_b, spot_b = enriched("B_SIRI_58.12_all4")
    rows_d, spot_d = enriched("D_SIRI_58.12_12d_widesp")

    scenarios = [
        ("A", "CSP", "SAFE", None, 0.0, False),
        ("A", "CSP", "BALANCED", None, 0.0, False),
        ("A", "CSP", "AGGR", None, 0.0, False),
        ("A", "CSP", "ALL", None, 0.0, False),          # app ties relax=True to risk "ALL"
        ("A", "CC", "BALANCED", None, 0.0, False),
        ("A", "CC", "SAFE", None, 0.0, False),
        ("A", "CSP+CC", "BALANCED", None, 0.0, False),
        ("A", "CSP+CC", "AGGR", None, 0.0, False),
        ("A", "CSP", "BALANCED", "E2", 0.0, False),      # filter to one expiry (all_exp=False)
        ("A", "CSP+CC", "BALANCED", "E1", 0.0, False),
        ("A", "CSP", "BALANCED", None, 17250.0, False),  # budget partially filters put collateral (K*100)
        ("B", "CSP", "BALANCED", None, 5400.0, False),   # partial budget on the 58.12 chain
        ("A", "CSP", "BALANCED", None, 0.0, True),       # relax=True with BALANCED deltas (explicit)
        ("B", "CSP+CC", "BALANCED", None, 0.0, False),
        ("B", "CC", "AGGR", "E3", 0.0, False),
        ("D", "CC", "BALANCED", None, 0.0, False),       # wide-spread chain (spread filter bites)
        ("D", "CSP", "BALANCED", None, 0.0, True),       # relax skips the spread/credit/dte limits
    ]
    out = []
    for case_id, mode, risk, exp_key, budget, relax in scenarios:
        rows, spot = {"A": (rows_a, spot_a), "B": (rows_b, spot_b), "D": (rows_d, spot_d)}[case_id]
        dmin, dmax = R.RISK_LEVELS[risk] if risk in R.RISK_LEVELS else (R.MIN_DELTA, R.MAX_DELTA)
        app_relax = relax or risk == "ALL"
        sel_exp = EXPIRIES[exp_key] if exp_key else None
        cand = R.short_candidates(rows, spot, mode, sel_exp=sel_exp, all_exp=sel_exp is None,
                                  dmin=dmin, dmax=dmax, relax=app_relax)
        final = app_pipeline(rows, spot, mode, sel_exp, dmin, dmax, app_relax, budget)
        out.append({
            "case": case_id, "mode": mode, "risk": risk, "dmin": dmin, "dmax": dmax,
            "sel_exp": sel_exp.date().isoformat() if sel_exp else None,
            "all_exp": sel_exp is None, "relax": app_relax, "budget": budget,
            "expected_candidates_input_order": [slim(r) for r in cand],   # short_candidates does NOT sort
            "expected_app_order": [slim(r) for r in final],               # budget filter + stable score desc sort
        })
    payload = {"meta": META,
               "conventions": {
                   "no_sort": "short_candidates() does NOT sort; it preserves input row order. The app "
                              "then filters collateral<=budget and sorts by (score or 0) descending "
                              "(Python stable sort -> ties keep input order)",
                   "filters": "require ann and delta non-None; CSP: put & strike<spot; CC: call & "
                              "strike>spot; dmin<=|delta|<=dmax; MIN_DTE<=dte<=MAX_DTE (0..400 relaxed); "
                              "credit >= MIN_CREDIT*100 = 5.0 (>=1.0 relaxed); spread_pct None or "
                              "<= MAX_SPREAD_PCT 25 (skipped when relaxed)",
                   "relax_binding": "in the app relax == (risk=='ALL'); scenario 13 shows relax=True "
                                    "combined with BALANCED deltas to expose the flag independently",
               },
               "scenarios": out}
    return payload, len(out)


# ---------------------------------------------------------------- 6. chain_stats.json
def gen_chain_stats(cases):
    out = []
    by_name = {c["name"]: c for c in cases}
    combos = [
        ("A_NVDA_182.45_all4", "E1"), ("A_NVDA_182.45_all4", "E2"),
        ("A_NVDA_182.45_all4", "E3"), ("A_NVDA_182.45_all4", "E4"),
        ("B_SIRI_58.12_all4", "E2"), ("B_SIRI_58.12_all4", "E4"),
        ("D_SIRI_58.12_12d_widesp", "E2"),
    ]
    for name, exp_key in combos:
        c = by_name[name]
        rows = [dict(r) for r in c["rows"]]
        R._enrich_rows_raw(rows, c["spot"])
        sel = EXPIRIES[exp_key]
        st = R.chain_stats(rows, c["spot"], sel)
        atm_iv, atm_T = R.atm_iv_for([r for r in rows if r["expiry"] == sel], c["spot"])
        out.append({"case": name, "spot": c["spot"], "sel_exp": sel.date().isoformat(),
                    "expected": {"atm_iv": st["atm_iv"], "em": st["em"], "skew": st["skew"]},
                    "atm_iv_for": {"atm_iv": atm_iv, "T": atm_T},
                    "otm_curve": [[k, v] for k, v in R.otm_curve(
                        [r for r in rows if r["expiry"] == sel], c["spot"])]})
    # empty-iv scenario: rows whose price is 0 -> iv None everywhere -> all-null stats
    c = by_name["C_NVDA_182.45_degraded"]
    rows = [dict(r) for r in c["rows"] if r["expiry"] == EXPIRIES["E1"]]
    for r in rows:
        r["price"] = 0.0
        r["bid"] = r["ask"] = 0.0
    R._enrich_rows_raw(rows, c["spot"])
    st = R.chain_stats(rows, c["spot"], EXPIRIES["E1"])
    atm_iv, atm_T = R.atm_iv_for(rows, c["spot"])
    out.append({"case": "C_NVDA_182.45_degraded_no_iv", "spot": c["spot"],
                "sel_exp": EXPIRIES["E1"].date().isoformat(),
                "expected": {"atm_iv": st["atm_iv"], "em": st["em"], "skew": st["skew"]},
                "atm_iv_for": {"atm_iv": atm_iv, "T": atm_T},
                "otm_curve": [[k, v] for k, v in R.otm_curve(rows, c["spot"])],
                "note": "all iv None -> chain_stats returns all null and atm_iv_for returns (None, None)"})
    payload = {"meta": META,
               "conventions": {
                   "keys": "chain_stats -> {'atm_iv','em','skew'}; em = spot*atm_iv*sqrt(T); "
                           "skew = (mean iv of 3 nearest OTM puts - mean iv of 3 nearest OTM calls)*100; "
                           "null skew when either side is empty",
                   "atm_iv_for": "returns (mean iv over ALL rows at the strike nearest spot, T of the "
                                 "FIRST such row); np.mean over the filtered list order",
                   "otm_curve": "sorted [(strike, iv*100)] restricted to OTM side per type "
                                "(P: strike<=spot, C: strike>=spot); last strike wins on collision",
               },
               "scenarios": out}
    return payload, len(out)


# ---------------------------------------------------------------- main
def main():
    print(f"frozen reference: {REF_NY_TEXT}  ({REF_UTC.isoformat()})")
    print(f"module: RISK_FREE_RATE={RF} DIV_YIELD={QY}")

    bs_payload, bs_n = gen_quant_bs()
    dump("quant_bs.json", bs_payload)

    iv_payload, iv_n = gen_quant_iv()
    dump("quant_iv.json", iv_payload)

    occ_payload, occ_n = gen_occ_parse()
    dump("occ_parse.json", occ_payload)

    cases = build_cases()
    enr_payload, enr_n = gen_enrich_grade(cases)
    dump("enrich_grade.json", enr_payload)

    scr_payload, scr_n = gen_screener(cases)
    dump("screener_filters.json", scr_payload)

    chn_payload, chn_n = gen_chain_stats(cases)
    dump("chain_stats.json", chn_payload)

    print("\ncase counts:")
    print(f"  quant_bs.json          {bs_n} cases")
    print(f"  quant_iv.json          {iv_n} cases")
    print(f"  occ_parse.json         {occ_n} cases")
    print(f"  enrich_grade.json      {enr_n} rows across {len(cases)} case groups")
    print(f"  screener_filters.json  {scr_n} scenarios")
    print(f"  chain_stats.json       {chn_n} scenarios")


if __name__ == "__main__":
    main()
