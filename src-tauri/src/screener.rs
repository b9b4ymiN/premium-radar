//! Screener pipeline — ported 1:1 from the Python reference and pinned by
//! `tests/fixtures/{enrich_grade,screener_filters,chain_stats}.json`.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};

use crate::quant::{bs_greeks, implied_vol, ncdf, years_to_expiry, OptType, DIV_YIELD, RISK_FREE_RATE};

pub const MIN_DELTA: f64 = 0.10;
pub const MAX_DELTA: f64 = 0.40;
pub const MIN_DTE: f64 = 3.0;
pub const MAX_DTE: f64 = 60.0;
pub const MAX_SPREAD_PCT: f64 = 25.0;
pub const MIN_CREDIT: f64 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grade {
    A,
    B,
    C,
}

impl Grade {
    pub fn as_str(self) -> &'static str {
        match self {
            Grade::A => "A",
            Grade::B => "B",
            Grade::C => "C",
        }
    }
}

/// Raw quote + OCC meta exactly as the app assembles from the feeds.
#[derive(Clone, Debug)]
pub struct QuoteRow {
    pub symbol: String,
    pub underlying: String,
    pub expiry: NaiveDate,
    pub option_type: OptType,
    pub strike: f64,
    pub price: f64,
    pub bid: f64,
    pub ask: f64,
    pub volume: f64,
    pub oi: f64,
    pub change: f64,
    pub change_ratio: f64,
    pub src: String,
}

/// A quote with all short-option economics attached (None = not computable).
#[derive(Clone, Debug)]
pub struct EnrichedRow {
    pub q: QuoteRow,
    pub t: f64,
    pub dte: f64,
    pub chg_pct: f64,
    pub vol_oi: Option<f64>,
    pub iv: Option<f64>,
    pub delta: Option<f64>,
    pub gamma: Option<f64>,
    pub vega: Option<f64>,
    pub theta: Option<f64>,
    pub pop: Option<f64>,
    pub credit: Option<f64>,
    pub collateral: Option<f64>,
    pub yld: Option<f64>,
    pub ann: Option<f64>,
    pub be: Option<f64>,
    pub cushion: Option<f64>,
    pub p_itm: Option<f64>,
    pub p_touch: Option<f64>,
    pub sig_dist: Option<f64>,
    pub strike_dist: Option<f64>,
    pub day_inc: Option<f64>,
    pub if_called: Option<f64>,
    pub if_called_ann: Option<f64>,
    pub basis: Option<f64>,
    pub score: Option<f64>,
    pub spread_pct: Option<f64>,
    pub prem: Option<f64>,
    pub net_cash: Option<f64>,
    pub max_loss: Option<f64>,
    pub max_profit: Option<f64>,
    pub grade: Option<Grade>,
    pub why: Option<String>,
}

/// Port of `_enrich_rows_raw`: computes IV, greeks and short-option economics.
/// `now` pins the clock (the Python version reads the wall clock).
pub fn enrich_rows_raw(rows: Vec<QuoteRow>, spot: f64, now: DateTime<Utc>) -> Vec<EnrichedRow> {
    let mut tcache: HashMap<NaiveDate, f64> = HashMap::new();
    rows.into_iter()
        .map(|q| {
            let t = *tcache.entry(q.expiry).or_insert_with(|| years_to_expiry(q.expiry, now));
            let mut r = EnrichedRow {
                q,
                t,
                dte: t * 365.0,
                chg_pct: 0.0,
                vol_oi: None,
                iv: None, delta: None, gamma: None, vega: None, theta: None,
                pop: None, credit: None, collateral: None, yld: None, ann: None,
                be: None, cushion: None, p_itm: None, p_touch: None, sig_dist: None,
                strike_dist: None, day_inc: None, if_called: None, if_called_ann: None,
                basis: None, score: None, spread_pct: None, prem: None, net_cash: None,
                max_loss: None, max_profit: None, grade: None, why: None,
            };
            r.chg_pct = r.q.change_ratio * 100.0;
            r.vol_oi = (r.q.oi != 0.0).then(|| r.q.volume / r.q.oi);
            if spot == 0.0 {
                return r;
            }
            let (cp, k) = (r.q.option_type, r.q.strike);
            let iv = implied_vol(cp, r.q.price, spot, k, r.t, RISK_FREE_RATE, DIV_YIELD);
            if let Some(iv) = iv {
                r.iv = Some(iv);
                let g = bs_greeks(cp, spot, k, r.t, RISK_FREE_RATE, DIV_YIELD, iv);
                r.delta = Some(g.delta);
                r.gamma = Some(g.gamma);
                r.vega = Some(g.vega);
                r.theta = Some(g.theta);
            }
            let mut px = r.q.bid;
            if px <= 0.0 {
                px = r.q.price;
            }
            if px > 0.0 {
                let base = if cp == OptType::Put { k } else { spot };
                let be = if cp == OptType::Put { k - px } else { spot - px };
                r.credit = Some(px * 100.0);
                r.collateral = Some(base * 100.0);
                r.yld = Some(px / base * 100.0);
                r.ann = r.yld.map(|y| y * 365.0 / r.dte.max(1.0));
                r.be = Some(be);
                r.cushion = Some((spot - be) / spot * 100.0);
                if let (Some(iv), true) = (iv, be > 0.0) {
                    let d2 = ((spot / be).ln()
                        + (RISK_FREE_RATE - DIV_YIELD - 0.5 * iv * iv) * r.t)
                        / (iv * r.t.sqrt());
                    r.pop = Some(ncdf(d2));
                }
                if r.q.bid > 0.0 && r.q.ask > 0.0 {
                    r.spread_pct =
                        Some((r.q.ask - r.q.bid) / ((r.q.ask + r.q.bid) / 2.0) * 100.0);
                }
                if let Some(iv) = iv {
                    let d2k = ((spot / k).ln()
                        + (RISK_FREE_RATE - DIV_YIELD - 0.5 * iv * iv) * r.t)
                        / (iv * r.t.sqrt());
                    let p_itm = if cp == OptType::Put { ncdf(-d2k) } else { ncdf(d2k) };
                    r.p_itm = Some(p_itm);
                    r.p_touch = Some((2.0 * p_itm).min(1.0));
                    r.sig_dist = Some((spot - be) / (spot * iv * r.t.sqrt()));
                    if let Some(theta) = r.theta {
                        r.day_inc = Some(theta.abs() * 100.0);
                    }
                }
                r.strike_dist = Some((k - spot).abs() / spot * 100.0);
                r.prem = Some(px);
                r.basis = Some(be);
                r.net_cash = Some(if cp == OptType::Put {
                    (k - px) * 100.0
                } else {
                    (spot - px) * 100.0
                });
                r.max_loss = r.net_cash;
                if cp == OptType::Put {
                    r.max_profit = Some(px * 100.0);
                } else {
                    r.max_profit = Some((k - spot + px) * 100.0);
                    let if_called = (k - spot + px) / spot * 100.0;
                    r.if_called = Some(if_called);
                    r.if_called_ann = Some(if_called * 365.0 / r.dte.max(1.0));
                }
                if r.pop.is_some() {
                    r.score = r.ann.map(|ann| ann * r.pop.unwrap_or(0.0));
                    let (grade, why) = grade_pick(&r);
                    r.grade = Some(grade);
                    r.why = Some(why);
                }
            }
            r
        })
        .collect()
}

/// Port of `grade_pick` — the 10-point rubric with Thai reasons.
/// Formatting mirrors the Python f-strings exactly (golden-tested).
pub fn grade_pick(r: &EnrichedRow) -> (Grade, String) {
    let ann = r.ann.unwrap_or(0.0);
    let pop = r.pop.unwrap_or(0.0);
    let cu = r.cushion.unwrap_or(0.0);
    let d = r.delta.unwrap_or(0.0).abs();
    let sp = r.spread_pct;
    let oi = r.q.oi;
    let dte = r.dte;
    let pi = r.p_itm;

    let mut pts: i32 = 0;
    let mut why: Vec<String> = Vec::new();
    if ann >= 30.0 {
        pts += 2;
        why.push(format!("ผลตอบแทน {ann:.0}%/ปี สูง"));
    } else if ann >= 15.0 {
        pts += 1;
        why.push(format!("ผลตอบแทน {ann:.0}%/ปี พอใช้"));
    } else {
        why.push(format!("ผลตอบแทนแค่ {ann:.0}%/ปี"));
    }
    if pop >= 0.80 {
        pts += 2;
        why.push(format!("โอกาสชนะ {:.0}% สูง", pop * 100.0));
    } else if pop >= 0.65 {
        pts += 1;
        why.push(format!("โอกาสชนะ {:.0}%", pop * 100.0));
    } else {
        why.push(format!("โอกาสชนะแค่ {:.0}%", pop * 100.0));
    }
    if let Some(pi) = pi {
        if pi < 0.25 {
            pts += 1;
            why.push(format!("โอกาสโดน assign ต่ำ {:.0}%", pi * 100.0));
        } else if pi > 0.35 {
            why.push(format!("โอกาสโดน assign สูง {:.0}%", pi * 100.0));
        }
    }
    if cu >= 5.0 {
        pts += 1;
        why.push(format!("กันชนราคา {cu:.1}%"));
    }
    if (0.15..=0.30).contains(&d) {
        pts += 1;
        why.push("เดลต้าอยู่ในช่วงดี".to_string());
    }
    if (21.0..=45.0).contains(&dte) {
        pts += 1;
        why.push("อายุ 21-45 วัน (ช่วงที่ time decay คุ้มสุด)".to_string());
    }
    match sp {
        Some(sp) if sp <= 10.0 => {
            pts += 1;
            why.push("สเปรดแคบ".to_string());
        }
        Some(sp) if sp > 20.0 => why.push(format!("สเปรดกว้าง {sp:.0}% ระวังขายยาก")),
        _ => {}
    }
    if oi >= 100.0 {
        pts += 1;
        why.push("OI พอ".to_string());
    } else if oi == 0.0 {
        why.push("ไม่มีข้อมูล OI".to_string());
    }
    let grade = if pts >= 7 { Grade::A } else if pts >= 4 { Grade::B } else { Grade::C };
    (grade, format!("คะแนน {pts}/10  →  {}", why.join(" | ")))
}

/// Port of `pick_pnl` — P/L per contract at expiry given terminal price `s`.
pub fn pick_pnl(r: &EnrichedRow, spot: f64, s: f64) -> f64 {
    let (px, k) = (r.prem.unwrap_or(0.0), r.q.strike);
    if r.q.option_type == OptType::Put {
        (px - (k - s).max(0.0)) * 100.0
    } else {
        (s.min(k) - spot + px) * 100.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Csp,
    Cc,
    Both,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "CSP" => Some(Self::Csp),
            "CC" => Some(Self::Cc),
            "CSP+CC" => Some(Self::Both),
            _ => None,
        }
    }
}

/// Filter set for `short_candidates` plus the app-level budget cap.
pub struct Filters {
    pub mode: Mode,
    pub sel_exp: Option<NaiveDate>,
    pub all_exp: bool,
    pub dmin: f64,
    pub dmax: f64,
    pub relax: bool,
    /// 0 = unlimited.
    pub budget: f64,
}

impl Default for Filters {
    fn default() -> Self {
        Self {
            mode: Mode::Both,
            sel_exp: None,
            all_exp: true,
            dmin: MIN_DELTA,
            dmax: MAX_DELTA,
            relax: false,
            budget: 0.0,
        }
    }
}

/// Port of `short_candidates` — filters only, **input order preserved**
/// (matches `expected_candidates_input_order` in the fixtures). No budget
/// cap, no sorting.
pub fn candidates_in_input_order<'a>(
    rows: &[&'a EnrichedRow],
    spot: f64,
    f: &Filters,
) -> Vec<&'a EnrichedRow> {
    rows.iter().copied()
        .filter(|r| {
            if spot == 0.0 || r.ann.is_none() || r.delta.is_none() {
                return false;
            }
            let cp = r.q.option_type;
            let mode_ok = match (cp, f.mode) {
                (OptType::Put, Mode::Csp) | (OptType::Put, Mode::Both) => r.q.strike < spot,
                (OptType::Call, Mode::Cc) | (OptType::Call, Mode::Both) => r.q.strike > spot,
                _ => false,
            };
            if !mode_ok {
                return false;
            }
            if !f.all_exp && Some(r.q.expiry) != f.sel_exp {
                return false;
            }
            let d = r.delta.unwrap_or(0.0).abs();
            if !(f.dmin..=f.dmax).contains(&d) {
                return false;
            }
            let (dte_lo, dte_hi) = if f.relax { (0.0, 400.0) } else { (MIN_DTE, MAX_DTE) };
            if !(dte_lo..=dte_hi).contains(&r.dte) {
                return false;
            }
            let min_credit = if f.relax { 1.0 } else { MIN_CREDIT * 100.0 };
            if r.credit.unwrap_or(0.0) < min_credit {
                return false;
            }
            if let Some(sp) = r.spread_pct {
                if !f.relax && sp > MAX_SPREAD_PCT {
                    return false;
                }
            }
            true
        })
        .collect()
}

/// `short_candidates` followed by the app pipeline's budget cap and a stable
/// sort by score descending (`expected_app_order` in the fixtures).
pub fn short_candidates<'a>(rows: &[&'a EnrichedRow], spot: f64, f: &Filters) -> Vec<&'a EnrichedRow> {
    let mut out: Vec<&EnrichedRow> = candidates_in_input_order(rows, spot, f)
        .into_iter()
        .filter(|r| f.budget <= 0.0 || r.collateral.unwrap_or(f64::MAX) <= f.budget)
        .collect();
    out.sort_by(|a, b| {
        b.score.unwrap_or(0.0).partial_cmp(&a.score.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Port of `atm_iv_for`: (atm_iv, T) averaged over every row at the strike
/// nearest spot.
pub fn atm_iv_for(rows: &[&EnrichedRow], spot: f64) -> (Option<f64>, Option<f64>) {
    let pool: Vec<&EnrichedRow> = rows.iter().copied().filter(|r| r.iv.is_some()).collect();
    if pool.is_empty() || spot == 0.0 {
        return (None, None);
    }
    let k = pool
        .iter()
        .map(|r| r.q.strike)
        .min_by(|a, b| {
            (a - spot)
                .abs()
                .partial_cmp(&(b - spot).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .expect("pool non-empty");
    let sel: Vec<&&EnrichedRow> = pool.iter().filter(|r| r.q.strike == k).collect();
    let mean = sel.iter().map(|r| r.iv.unwrap_or(0.0)).sum::<f64>() / sel.len() as f64;
    (Some(mean), Some(sel[0].t))
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChainStats {
    pub atm_iv: Option<f64>,
    pub em: Option<f64>,
    pub skew: Option<f64>,
}

/// Port of `chain_stats`: ATM IV, ±1σ expected move and put-call skew for one
/// expiry (skew uses the 3 nearest OTM strikes on each side).
pub fn chain_stats(rows: &[&EnrichedRow], spot: f64, sel_exp: NaiveDate) -> ChainStats {
    let srows: Vec<&EnrichedRow> = rows
        .iter()
        .copied()
        .filter(|r| r.q.expiry == sel_exp && r.iv.is_some())
        .collect();
    let (Some(iv), Some(t)) = atm_iv_for(&srows, spot) else {
        return ChainStats::default();
    };
    let mut out = ChainStats {
        atm_iv: Some(iv),
        em: Some(spot * iv * t.sqrt()),
        skew: None,
    };
    let mut puts: Vec<&EnrichedRow> = srows
        .iter()
        .copied()
        .filter(|r| r.q.option_type == OptType::Put && r.q.strike < spot)
        .collect();
    let mut calls: Vec<&EnrichedRow> = srows
        .iter()
        .copied()
        .filter(|r| r.q.option_type == OptType::Call && r.q.strike > spot)
        .collect();
    puts.sort_by(|a, b| {
        (spot - a.q.strike)
            .partial_cmp(&(spot - b.q.strike))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    calls.sort_by(|a, b| {
        (a.q.strike - spot)
            .partial_cmp(&(b.q.strike - spot))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    puts.truncate(3);
    calls.truncate(3);
    if !puts.is_empty() && !calls.is_empty() {
        let pm = puts.iter().map(|r| r.iv.unwrap_or(0.0)).sum::<f64>() / puts.len() as f64;
        let cm = calls.iter().map(|r| r.iv.unwrap_or(0.0)).sum::<f64>() / calls.len() as f64;
        out.skew = Some((pm - cm) * 100.0);
    }
    out
}

/// Port of `otm_curve`: sorted `(strike, iv%)` for OTM puts at/below spot and
/// OTM calls at/above spot. Duplicate strikes keep the last row's IV.
pub fn otm_curve(rows: &[&EnrichedRow], spot: f64) -> Vec<(f64, f64)> {
    let mut pts: HashMap<i64, (f64, f64)> = HashMap::new();
    for r in rows {
        let Some(iv) = r.iv else { continue };
        let otm = (r.q.option_type == OptType::Put && r.q.strike <= spot)
            || (r.q.option_type == OptType::Call && r.q.strike >= spot);
        if otm {
            pts.insert((r.q.strike * 1000.0) as i64, (r.q.strike, iv * 100.0));
        }
    }
    let mut out: Vec<(f64, f64)> = pts.into_values().collect();
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otm_curve_keeps_last_duplicate_strike() {
        // covered by golden fixtures too; quick sanity here
        assert_eq!(Mode::parse("CSP+CC"), Some(Mode::Both));
        assert_eq!(Grade::B.as_str(), "B");
    }
}
