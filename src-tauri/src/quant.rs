//! Black-Scholes core — ported 1:1 from the Python reference (`Short_Option_Radar_PY.py`)
//! and pinned by golden fixtures in `tests/fixtures/` (prices within 1e-6).

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use chrono_tz::America::New_York;
use libm::erf;

const SQRT2: f64 = std::f64::consts::SQRT_2;
const SQRT2PI: f64 = 2.506_628_274_631_000_502_415_765_284_811;

/// Environment rate pinned in fixtures (`RISK_FREE_RATE`, default 0.043).
pub const RISK_FREE_RATE: f64 = 0.043;
/// Reference dividend yield used by the Python original.
pub const DIV_YIELD: f64 = 0.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OptType {
    Call,
    Put,
}

impl OptType {
    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Call => "C",
            Self::Put => "P",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "C" => Some(Self::Call),
            "P" => Some(Self::Put),
            _ => None,
        }
    }
}

#[inline]
pub fn ncdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / SQRT2))
}

#[inline]
pub fn npdf(x: f64) -> f64 {
    (-0.5 * x * x).exp() / SQRT2PI
}

/// Black-Scholes price. Degenerate inputs (`t <= 0` or `sig <= 0`) return the
/// undiscounted intrinsic value, exactly like the Python original.
pub fn bs_price(cp: OptType, s: f64, k: f64, t: f64, r: f64, q: f64, sig: f64) -> f64 {
    if t <= 0.0 || sig <= 0.0 {
        return match cp {
            OptType::Call => (s - k).max(0.0),
            OptType::Put => (k - s).max(0.0),
        };
    }
    let sq = sig * t.sqrt();
    let d1 = ((s / k).ln() + (r - q + 0.5 * sig * sig) * t) / sq;
    let d2 = d1 - sq;
    match cp {
        OptType::Call => {
            s * (-q * t).exp() * ncdf(d1) - k * (-r * t).exp() * ncdf(d2)
        }
        OptType::Put => k * (-r * t).exp() * ncdf(-d2) - s * (-q * t).exp() * ncdf(-d1),
    }
}

/// Bisection implied volatility. Returns the fraction (0.45 = 45%) or `None`
/// when unsolvable — same rejection paths as Python: non-positive inputs,
/// market price below the *discounted-forward* intrinsic, price above the
/// vol=5 surface, or an IV outside (0.01, 4.99).
pub fn implied_vol(cp: OptType, price: f64, s: f64, k: f64, t: f64, r: f64, q: f64) -> Option<f64> {
    if price <= 0.0 || s <= 0.0 || k <= 0.0 || t <= 0.0 {
        return None;
    }
    let disc_k = k * (-r * t).exp();
    let fwd_s = s * (-q * t).exp();
    let intrinsic = match cp {
        OptType::Call => (fwd_s - disc_k).max(0.0),
        OptType::Put => (disc_k - fwd_s).max(0.0),
    };
    if price < intrinsic - 1e-6 {
        return None;
    }
    let (mut lo, mut hi) = (1e-4_f64, 5.0_f64);
    if bs_price(cp, s, k, t, r, q, hi) < price {
        return None;
    }
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if bs_price(cp, s, k, t, r, q, mid) > price {
            hi = mid;
        } else {
            lo = mid;
        }
        if hi - lo < 1e-6 {
            break;
        }
    }
    let iv = 0.5 * (lo + hi);
    (0.01 < iv && iv < 4.99).then_some(iv)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Greeks {
    pub delta: f64,
    pub gamma: f64,
    /// Per 1 vol point (÷100), as in the Python original.
    pub vega: f64,
    /// Per calendar day (÷365).
    pub theta: f64,
}

/// Greeks at the given vol. The Python version raises on `t <= 0`/`sig <= 0`;
/// the app only calls this after a successful IV solve, so degenerate inputs
/// return zeroed Greeks instead of panicking.
pub fn bs_greeks(cp: OptType, s: f64, k: f64, t: f64, r: f64, q: f64, sig: f64) -> Greeks {
    let sq = sig * t.sqrt();
    if !(sq > 0.0) || s <= 0.0 {
        return Greeks { delta: 0.0, gamma: 0.0, vega: 0.0, theta: 0.0 };
    }
    let d1 = ((s / k).ln() + (r - q + 0.5 * sig * sig) * t) / sq;
    let d2 = d1 - sq;
    let (eq, er, pdf) = ((-q * t).exp(), (-r * t).exp(), npdf(d1));
    let gamma = eq * pdf / (s * sq);
    let vega = s * eq * pdf * t.sqrt() / 100.0;
    let base = -s * eq * pdf * sig / (2.0 * t.sqrt());
    match cp {
        OptType::Call => Greeks {
            delta: eq * ncdf(d1),
            gamma,
            vega,
            theta: (base - r * k * er * ncdf(d2) + q * s * eq * ncdf(d1)) / 365.0,
        },
        OptType::Put => Greeks {
            delta: eq * (ncdf(d1) - 1.0),
            gamma,
            vega,
            theta: (base + r * k * er * ncdf(-d2) - q * s * eq * ncdf(-d1)) / 365.0,
        },
    }
}

/// 16:00 New York on the expiry date, expressed in UTC (handles DST like the
/// Python `zoneinfo` path). Mirrors the silent 20:00-UTC fallback on failure.
pub fn expiry_utc(expiry: NaiveDate) -> DateTime<Utc> {
    New_York
        .with_ymd_and_hms(expiry.year(), expiry.month(), expiry.day(), 16, 0, 0)
        .single()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| {
            expiry
                .and_hms_opt(20, 0, 0)
                .map(|naive| naive.and_utc())
                .expect("valid naive datetime always converts")
        })
}

/// Years remaining until NY 16:00 expiry, clamped to at least one hour —
/// matches `years_to_expiry` in the Python original.
pub fn years_to_expiry(expiry: NaiveDate, now: DateTime<Utc>) -> f64 {
    let secs = (expiry_utc(expiry) - now).num_milliseconds() as f64 / 1000.0;
    secs.max(3600.0) / (365.0 * 86400.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degenerate_price_returns_intrinsic() {
        assert_eq!(bs_price(OptType::Call, 110.0, 100.0, 0.0, 0.05, 0.0, 0.3), 10.0);
        assert_eq!(bs_price(OptType::Put, 110.0, 100.0, 0.0, 0.05, 0.0, 0.3), 0.0);
        assert_eq!(bs_price(OptType::Put, 90.0, 100.0, 0.01, 0.05, 0.0, 0.0), 10.0);
    }

    #[test]
    fn implied_vol_rejects_below_intrinsic() {
        // deep ITM put priced below the discounted-forward intrinsic
        assert!(implied_vol(OptType::Put, 0.5, 100.0, 150.0, 0.1, RISK_FREE_RATE, DIV_YIELD).is_none());
        assert!(implied_vol(OptType::Call, 0.0, 100.0, 90.0, 0.1, RISK_FREE_RATE, DIV_YIELD).is_none());
    }

    #[test]
    fn iv_round_trip() {
        let (s, k, t, vol) = (182.45_f64, 175.0, 0.1, 0.44);
        let p = bs_price(OptType::Put, s, k, t, RISK_FREE_RATE, DIV_YIELD, vol);
        let iv = implied_vol(OptType::Put, p, s, k, t, RISK_FREE_RATE, DIV_YIELD).unwrap();
        assert!((iv - vol).abs() < 5e-4, "iv={iv}");
    }

    #[test]
    fn put_delta_below_zero_call_above() {
        let g_put = bs_greeks(OptType::Put, 182.45, 175.0, 0.1, RISK_FREE_RATE, DIV_YIELD, 0.4);
        let g_call = bs_greeks(OptType::Call, 182.45, 175.0, 0.1, RISK_FREE_RATE, DIV_YIELD, 0.4);
        assert!(g_put.delta < 0.0 && g_call.delta > 0.0);
        assert!((g_put.delta - g_call.delta - 1.0).abs() < 1e-9);
    }
}
