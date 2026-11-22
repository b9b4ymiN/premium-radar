//! OCC option symbol parsing — ported 1:1 from `parse_occ` in the Python
//! reference and pinned by `tests/fixtures/occ_parse.json`.

use chrono::NaiveDate;
use regex::Regex;
use std::sync::OnceLock;

use crate::quant::OptType;

#[derive(Clone, Debug, PartialEq)]
pub struct OccMeta {
    pub underlying: String,
    pub expiry: NaiveDate,
    pub option_type: OptType,
    /// Strike in dollars (`int(strike_digits) / 1000`).
    pub strike: f64,
}

/// Parse an OCC symbol such as `NVDA261218P00175000`.
/// Spaces are stripped first; invalid symbols and impossible calendar dates
/// return `None`, mirroring the Python regex + `datetime()` failure paths.
pub fn parse_occ(symbol: &str) -> Option<OccMeta> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^([A-Z0-9.]{1,6})(\d{2})(\d{2})(\d{2})([CP])(\d{8})$").expect("valid regex")
    });
    let s = symbol.trim().replace(' ', "");
    let m = re.captures(&s)?;
    let root = m.get(1)?.as_str().to_string();
    let yy: i32 = 2000 + m.get(2)?.as_str().parse().ok()?;
    let mm: u32 = m.get(3)?.as_str().parse().ok()?;
    let dd: u32 = m.get(4)?.as_str().parse().ok()?;
    let option_type = OptType::parse(m.get(5)?.as_str())?;
    let strike = m.get(6)?.as_str().parse::<f64>().ok()? / 1000.0;
    let expiry = NaiveDate::from_ymd_opt(yy, mm, dd)?;
    Some(OccMeta { underlying: root, expiry, option_type, strike })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_symbol() {
        let m = parse_occ("NVDA261218P00175000").unwrap();
        assert_eq!(m.underlying, "NVDA");
        assert_eq!(m.expiry, NaiveDate::from_ymd_opt(2026, 12, 18).unwrap());
        assert_eq!(m.option_type, OptType::Put);
        assert_eq!(m.strike, 175.0);
    }

    #[test]
    fn strips_spaces_and_short_roots() {
        let m = parse_occ("A 270206C 00250000").unwrap();
        assert_eq!(m.underlying, "A");
        assert_eq!(m.strike, 250.0);
    }

    #[test]
    fn rejects_garbage_and_impossible_dates() {
        assert!(parse_occ("NOTASYMBOL").is_none());
        assert!(parse_occ("NVDA260230P00175000").is_none()); // Feb 30
        assert!(parse_occ("NVDA261318P00175000").is_none()); // month 13
        assert!(parse_occ("NVDA261218P0017500").is_none()); // 7-digit strike
        assert!(parse_occ("nvda261218P00175000").is_none()); // lowercase
    }
}
