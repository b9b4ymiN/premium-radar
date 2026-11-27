//! Easy Mode (CR-001 / W3) — plain-Thai sell-put ideas from real data.
//!
//! Pure functions over the screener's enriched rows: no I/O, no clock reads
//! (`now` is injected). Every JSON shape mirrors `src/types/easy.ts` exactly
//! (snake_case fields, enums as lowercase strings) — the shape test below
//! fails when the two drift apart.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::quant::OptType;
use crate::screener::EnrichedRow;

// ---------- profile (saved in settings, not secret) ----------

/// Whether the user would be happy to own the stock if assigned.
/// Unknown strings load as `want` so a hand-edited file never wipes settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stance {
    #[default]
    Want,
    Avoid,
}

impl Stance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Want => "want",
            Self::Avoid => "avoid",
        }
    }
}

impl<'de> Deserialize<'de> for Stance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "avoid" => Self::Avoid,
            _ => Self::Want,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EasyStock {
    pub symbol: String,
    pub stance: Stance,
}

/// The user's three onboarding answers. `risk` is 1–3 (clamped at use,
/// validated on save).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EasyProfile {
    #[serde(default)]
    pub onboarded: bool,
    #[serde(default)]
    pub stocks: Vec<EasyStock>,
    /// USD the user is ready to set aside.
    #[serde(default)]
    pub cash: f64,
    #[serde(default = "default_risk")]
    pub risk: u8,
}

fn default_risk() -> u8 {
    1
}

impl Default for EasyProfile {
    fn default() -> Self {
        Self {
            onboarded: false,
            stocks: Vec::new(),
            cash: 0.0,
            risk: default_risk(),
        }
    }
}

/// Stance for one symbol: saved answer, or `want` when not in the profile.
pub fn stance_for(profile: &EasyProfile, symbol: &str) -> Stance {
    profile
        .stocks
        .iter()
        .find(|s| s.symbol == symbol)
        .map(|s| s.stance)
        .unwrap_or(Stance::Want)
}

/// Normalise + validate for `save_easy_profile`: symbols are trimmed,
/// uppercased and deduped (first wins, empties dropped); at most 10 stocks;
/// cash must be a finite number ≥ 0; risk must be 1–3.
pub fn normalise_profile(profile: EasyProfile) -> Result<EasyProfile, String> {
    if !profile.cash.is_finite() || profile.cash < 0.0 {
        return Err("เงินที่ตั้งไว้ต้องเป็นตัวเลขตั้งแต่ 0 ขึ้นไป".into());
    }
    if !(1..=3).contains(&profile.risk) {
        return Err("ระดับความสบายใจต้องเป็น 1, 2 หรือ 3".into());
    }
    let mut stocks: Vec<EasyStock> = Vec::new();
    for stock in profile.stocks {
        let symbol = stock.symbol.trim().to_uppercase();
        if symbol.is_empty() || stocks.iter().any(|s| s.symbol == symbol) {
            continue;
        }
        stocks.push(EasyStock {
            symbol,
            stance: stock.stance,
        });
    }
    if stocks.len() > 10 {
        return Err("เก็บรายชื่อหุ้นได้สูงสุด 10 ตัว".into());
    }
    Ok(EasyProfile {
        onboarded: profile.onboarded,
        stocks,
        cash: profile.cash,
        risk: profile.risk,
    })
}

// ---------- ideas (command output) ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdeaLevel {
    Safe,
    Balanced,
    High,
}

impl IdeaLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Balanced => "balanced",
            Self::High => "high",
        }
    }
}

/// The three expiry outcomes, each 0–100 and summing to exactly 100.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EasyCases {
    pub keep_pct: f64,
    pub between_pct: f64,
    pub loss_pct: f64,
}

/// One sell-put idea, all money figures in USD for ONE contract (100 shares).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EasyIdea {
    pub occ: String,
    pub symbol: String,
    pub level: IdeaLevel,
    pub recommended: bool,
    pub strike: f64,
    pub expiry: String,
    pub days: f64,
    /// Premium per share actually used (bid-side credit).
    pub premium: f64,
    /// Money received now: premium × 100.
    pub credit: f64,
    /// Cash that must be set aside: strike × 100.
    pub cost: f64,
    /// credit / cost × 100, for the holding period (not annualised).
    pub period_pct: f64,
    /// Annualised %, shown only as a muted footnote.
    pub annual_pct: Option<f64>,
    /// Break-even price per share: strike − premium.
    pub breakeven: f64,
    /// How far below today's price the strike is, in % (positive = below).
    pub below_spot_pct: f64,
    /// Chance of having to buy the shares, 0–100 (from p_itm).
    pub assign_pct: f64,
    pub cases: EasyCases,
    /// Paper result if the stock falls 30% by expiry (negative = loss).
    pub worst_30_usd: f64,
    /// Theoretical max loss if the stock went to $0: cost − credit.
    pub max_loss_usd: f64,
    /// True when assign_pct ≥ 30 or above the user's comfort.
    pub warn: bool,
    /// Not enough cash for this idea (cost > profile cash).
    pub cash_short: bool,
}

/// "Why now" evidence from real data only; null fields ⇒ the no-data sentence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhyNow {
    /// ATM implied volatility, % per year.
    pub iv_pct: Option<f64>,
    /// Realised volatility of daily closes over the last 30 sessions, %/year.
    pub realized_30d_pct: Option<f64>,
    pub text_th: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EasyIdeas {
    pub symbol: String,
    pub spot: f64,
    /// Where the price came from: "LIVE" | "API" | "CLOSE" | "DEMO".
    pub source: String,
    pub stance: Stance,
    pub agent_say_th: String,
    pub ideas: Vec<EasyIdea>,
    pub skip_reason_th: Option<String>,
    pub why_now: WhyNow,
    pub computed_at: String,
}

/// Map an engine data-source label onto the contract's English tokens.
pub fn source_token(src: &str) -> &'static str {
    match src.trim() {
        "สด" => "LIVE",
        "API" => "API",
        "ราคาปิด" => "CLOSE",
        "DEMO" => "DEMO",
        _ => "API",
    }
}

// ---------- selection ----------

/// Comfort cap on assign_pct, %: "avoid" stays 15 whatever the risk; "want"
/// scales 15 / 25 / 30 with the saved risk. Practitioners selling
/// cash-secured puts mostly work around delta 16–30 at 30–45 DTE (delta ≈
/// chance of being assigned), so risk 3 stops at 30% to stay inside that
/// range — the old risk×10+5 ladder let it reach 35%.
pub fn comfort_cap(stance: Stance, risk: u8) -> f64 {
    match stance {
        Stance::Want => match risk.clamp(1, 3) {
            1 => 15.0,
            2 => 25.0,
            _ => 30.0,
        },
        Stance::Avoid => 15.0,
    }
}

/// Easy-Mode candidate: a put row with usable economics and at least one day
/// to expiry. Independent of the terminal's strategy/risk/budget filters.
fn is_candidate(r: &EnrichedRow) -> bool {
    r.q.option_type == OptType::Put
        && r.q.strike > 0.0
        && r.dte >= 1.0
        && r.credit.is_some_and(|c| c.is_finite() && c > 0.0)
        && r.p_itm.is_some_and(|p| (0.0..=1.0).contains(&p))
        && r.pop.is_some_and(|p| (0.0..=1.0).contains(&p))
        && r.be.is_some_and(|b| b.is_finite())
}

fn p_itm_of(r: &EnrichedRow) -> f64 {
    r.p_itm.unwrap_or(0.0)
}

/// Deterministic candidate order: p_itm ascending, then annualised descending
/// (missing last), then strike ascending, then OCC ascending.
fn cand_order(a: &&EnrichedRow, b: &&EnrichedRow) -> std::cmp::Ordering {
    p_itm_of(a)
        .partial_cmp(&p_itm_of(b))
        .unwrap_or(std::cmp::Ordering::Equal)
        .then(
            b.ann
                .unwrap_or(f64::NEG_INFINITY)
                .partial_cmp(&a.ann.unwrap_or(f64::NEG_INFINITY))
                .unwrap_or(std::cmp::Ordering::Equal),
        )
        .then(
            a.q.strike
                .partial_cmp(&b.q.strike)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
        .then_with(|| a.q.symbol.cmp(&b.q.symbol))
}

/// Pick the three levels from candidates already sorted by [`cand_order`].
/// Documented rules:
/// - safe: lowest p_itm (first in [`cand_order`]).
/// - balanced: p_itm closest to 0.20 among 0.12–0.30 excluding the safe row;
///   when that band is empty, the next p_itm above safe's; else none.
/// - high: highest annualised among p_itm ≤ 0.45 excluding the other two picks
///   (distinct rows when possible; ann ties keep [`cand_order`]); else none.
fn pick_levels<'a>(cands: &[&'a EnrichedRow]) -> [Option<&'a EnrichedRow>; 3] {
    let safe = cands.first().copied();
    let balanced = safe.and_then(|safe| {
        let closest = cands
            .iter()
            .copied()
            .filter(|r| r.q.symbol != safe.q.symbol && (0.12..=0.30).contains(&p_itm_of(r)))
            .min_by(|a, b| {
                (p_itm_of(a) - 0.20)
                    .abs()
                    .partial_cmp(&(p_itm_of(b) - 0.20).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(cand_order(a, b))
            });
        closest.or_else(|| {
            cands
                .iter()
                .copied()
                .filter(|r| r.q.symbol != safe.q.symbol && p_itm_of(r) > p_itm_of(safe))
                .min_by(cand_order)
        })
    });
    let high = safe.and_then(|safe| {
        cands
            .iter()
            .copied()
            .filter(|r| {
                p_itm_of(r) <= 0.45
                    && r.q.symbol != safe.q.symbol
                    && balanced.is_none_or(|b| r.q.symbol != b.q.symbol)
            })
            .max_by(|a, b| {
                a.ann
                    .unwrap_or(f64::NEG_INFINITY)
                    .partial_cmp(&b.ann.unwrap_or(f64::NEG_INFINITY))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(cand_order(a, b))
            })
    });
    [safe, balanced, high]
}

pub(crate) fn make_idea(
    level: IdeaLevel,
    r: &EnrichedRow,
    symbol: &str,
    spot: f64,
    profile: &EasyProfile,
    cap: f64,
) -> EasyIdea {
    let credit = r.credit.unwrap_or_default();
    let strike = r.q.strike;
    let cost = strike * 100.0;
    let p_itm = r.p_itm.unwrap_or_default();
    let pop = r.pop.unwrap_or_default();
    let assign_pct = (p_itm * 100.0).round();
    // keep and between round independently; loss absorbs the remainder so the
    // three always sum to exactly 100.
    let between_pct = ((pop + p_itm - 1.0).max(0.0) * 100.0).round();
    let mut keep_pct = ((1.0 - p_itm) * 100.0).round();
    let mut loss_pct = 100.0 - keep_pct - between_pct;
    if loss_pct < 0.0 {
        keep_pct = (100.0 - between_pct).max(0.0);
        loss_pct = 0.0;
    }
    EasyIdea {
        occ: r.q.symbol.clone(),
        symbol: symbol.to_string(),
        level,
        recommended: false,
        strike,
        expiry: r.q.expiry.format("%Y-%m-%d").to_string(),
        days: r.dte.round(),
        premium: credit / 100.0,
        credit,
        cost,
        period_pct: credit / cost * 100.0,
        annual_pct: r.ann,
        breakeven: r.be.unwrap_or_default(),
        below_spot_pct: (1.0 - strike / spot) * 100.0,
        assign_pct,
        cases: EasyCases {
            keep_pct,
            between_pct,
            loss_pct,
        },
        worst_30_usd: ((0.7 * spot - strike) * 100.0 + credit).min(credit),
        max_loss_usd: cost - credit,
        warn: assign_pct >= 30.0 || assign_pct > cap,
        cash_short: cost > profile.cash,
    }
}

/// Comma-grouped whole dollars for the Thai sentences ("$18,000").
pub(crate) fn fmt_usd(v: f64) -> String {
    let whole = v.round() as i64;
    let sign = if whole < 0 { "-" } else { "" };
    let digits = whole.unsigned_abs().to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{sign}{grouped}")
}

fn level_th(level: IdeaLevel) -> &'static str {
    match level {
        IdeaLevel::Safe => "ทางปลอดภัย",
        IdeaLevel::Balanced => "ทางสมดุล",
        IdeaLevel::High => "ทางที่ได้เงินมากกว่าแต่เสี่ยงกว่า",
    }
}

fn cheapest_cost(ideas: &[EasyIdea]) -> f64 {
    ideas
        .iter()
        .map(|i| i.cost)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or_default()
}

/// One or two plain-Thai sentences, no option jargon (the no-jargon test
/// below enforces this). Mentions the cash, the comfort level from the
/// profile and the recommended level.
fn agent_say_th(
    symbol: &str,
    ideas: &[EasyIdea],
    recommended: Option<IdeaLevel>,
    profile: &EasyProfile,
    stance: Stance,
) -> String {
    if ideas.is_empty() {
        return format!("{symbol} ยังไม่มีทางเลือกให้พิจารณาในตอนนี้");
    }
    let cash = fmt_usd(profile.cash);
    let comfort = match stance {
        Stance::Want => format!("ยอมให้ต้องซื้อหุ้นได้ราว {} ใน 10 ครั้ง", profile.risk.clamp(1, 3)),
        Stance::Avoid => "ไม่อยากถือหุ้นตัวนี้ จึงเลือกเฉพาะทางที่โอกาสต้องซื้อหุ้นต่ำ ๆ".to_string(),
    };
    match recommended {
        Some(level) => format!(
            "{symbol} มี {} ทางเลือก คุณตั้งเงินไว้ ${cash} และ{comfort} ผมจึงแนะนำ{}",
            ideas.len(),
            level_th(level)
        ),
        None if ideas.iter().all(|i| i.cash_short) => format!(
            "{symbol} มี {} ทางเลือก แต่เงินที่ตั้งไว้ ${cash} ยังไม่พอ \
             ต้องกันไว้อย่างน้อย ${} ต่อ 1 สัญญา",
            ideas.len(),
            fmt_usd(cheapest_cost(ideas))
        ),
        None => format!(
            "{symbol} มี {} ทางเลือก แต่ทุกทางมีโอกาสต้องซื้อหุ้นสูงกว่าที่คุณสบายใจ \
             จึงขอไม่แนะนำอะไรตอนนี้",
            ideas.len()
        ),
    }
}

/// The "why now" sentence from ATM IV (fraction) and 30-day realised vol
/// (fraction). The single word "option" is allowed here only.
pub fn why_now(atm_iv: Option<f64>, realized_30d: Option<f64>) -> WhyNow {
    let iv_pct = atm_iv
        .filter(|v| v.is_finite() && *v > 0.0)
        .map(|v| v * 100.0);
    let rv_pct = realized_30d
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v * 100.0);
    let text_th = match (iv_pct, rv_pct) {
        (Some(iv), Some(rv)) if iv > rv * 1.1 => format!(
            "ช่วงนี้คนยอมจ่ายค่าประกันราคาหุ้นแพงกว่าปกติ ราคา option สะท้อนการแกว่ง {:.0}% ต่อปี \
             ขณะที่ 30 วันที่ผ่านมาหุ้นแกว่งจริง {:.0}%",
            iv, rv
        ),
        (Some(_), Some(rv)) if iv_pct.is_some_and(|iv| iv < rv * 0.9) => {
            "ค่าประกันตอนนี้ถูกกว่าการแกว่งจริง ไม่ค่อยคุ้มที่จะขาย".to_string()
        }
        (Some(iv), Some(rv)) => format!(
            "ราคา option สะท้อนการแกว่ง {:.0}% ต่อปี ใกล้เคียงกับที่หุ้นแกว่งจริงในรอบ 30 วัน ({:.0}%) \
             ถือว่าเป็นช่วงราคาปกติ",
            iv, rv
        ),
        _ => "ยังไม่มีข้อมูลพอจะบอกว่าช่วงนี้คุ้มกว่าปกติไหม".to_string(),
    };
    WhyNow {
        iv_pct,
        realized_30d_pct: rv_pct,
        text_th,
    }
}

/// Build the Easy Mode answer for one symbol from the screener's enriched
/// rows (ALL rows — the terminal's filters do not apply here). `atm_iv` and
/// `realized_30d` are fractions (0.48 = 48 %/year); either may be `None`.
#[allow(clippy::too_many_arguments)]
pub fn build_ideas(
    symbol: &str,
    spot: f64,
    source: &str,
    rows: &[EnrichedRow],
    profile: &EasyProfile,
    atm_iv: Option<f64>,
    realized_30d: Option<f64>,
    now: DateTime<Utc>,
) -> EasyIdeas {
    let stance = stance_for(profile, symbol);
    let cap = comfort_cap(stance, profile.risk);
    let mut cands: Vec<&EnrichedRow> = rows
        .iter()
        .filter(|r| r.q.underlying == symbol && is_candidate(r))
        .collect();
    cands.sort_by(cand_order);

    let mut ideas: Vec<EasyIdea> = Vec::new();
    if spot > 0.0 {
        let picks = pick_levels(&cands);
        for (level, pick) in [
            (IdeaLevel::Safe, 0),
            (IdeaLevel::Balanced, 1),
            (IdeaLevel::High, 2),
        ] {
            if let Some(row) = picks[pick] {
                ideas.push(make_idea(level, row, symbol, spot, profile, cap));
            }
        }
    }

    // Highest-paying affordable level within the comfort cap, preferring
    // high → balanced → safe; fall back to safe whenever it is affordable.
    let recommended = [IdeaLevel::High, IdeaLevel::Balanced, IdeaLevel::Safe]
        .into_iter()
        .find_map(|level| {
            ideas
                .iter()
                .position(|i| i.level == level && i.assign_pct <= cap && !i.cash_short)
        })
        .or_else(|| {
            ideas
                .iter()
                .position(|i| i.level == IdeaLevel::Safe && !i.cash_short)
        })
        .map(|ix| {
            ideas[ix].recommended = true;
            ideas[ix].level
        });

    let skip_reason_th = if ideas.is_empty() {
        Some(format!("ยังไม่มีข้อมูลของ {symbol} พอจะคำนวณทางเลือกได้"))
    } else if ideas.iter().all(|i| i.cash_short) {
        Some(format!(
            "เงินที่ตั้งไว้ ${} ยังไม่พอ ต้องกันไว้อย่างน้อย ${} ต่อ 1 สัญญา",
            fmt_usd(profile.cash),
            fmt_usd(cheapest_cost(&ideas))
        ))
    } else {
        None
    };

    EasyIdeas {
        symbol: symbol.to_string(),
        spot,
        source: source.to_string(),
        stance,
        agent_say_th: agent_say_th(symbol, &ideas, recommended, profile, stance),
        ideas,
        skip_reason_th,
        why_now: why_now(atm_iv, realized_30d),
        computed_at: now.to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screener::QuoteRow;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-25T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    /// Minimal put row with the fields Easy Mode reads; everything else None.
    fn put_row(strike: f64, p_itm: f64, pop: f64, credit: f64, ann: Option<f64>) -> EnrichedRow {
        let premium = credit / 100.0;
        EnrichedRow {
            q: QuoteRow {
                symbol: format!("NVDA261218P{strike:010.3}"),
                underlying: "NVDA".into(),
                expiry: chrono::NaiveDate::from_ymd_opt(2026, 12, 18).unwrap(),
                option_type: OptType::Put,
                strike,
                price: premium,
                bid: premium,
                ask: premium + 0.1,
                volume: 10.0,
                oi: 100.0,
                change: 0.0,
                change_ratio: 0.0,
                src: "API".into(),
            },
            t: 0.1,
            dte: 23.0,
            chg_pct: 0.0,
            vol_oi: None,
            iv: None,
            delta: None,
            gamma: None,
            vega: None,
            theta: None,
            pop: Some(pop),
            credit: Some(credit),
            collateral: Some(strike * 100.0),
            yld: None,
            ann,
            be: Some(strike - premium),
            cushion: None,
            p_itm: Some(p_itm),
            p_touch: None,
            sig_dist: None,
            strike_dist: None,
            day_inc: None,
            if_called: None,
            if_called_ann: None,
            basis: None,
            score: None,
            spread_pct: None,
            prem: None,
            net_cash: None,
            max_loss: None,
            max_profit: None,
            grade: None,
            why: None,
        }
    }

    fn profile(cash: f64, risk: u8, stance: Stance) -> EasyProfile {
        EasyProfile {
            onboarded: true,
            stocks: vec![EasyStock {
                symbol: "NVDA".into(),
                stance,
            }],
            cash,
            risk,
        }
    }

    fn build(rows: &[EnrichedRow], profile: &EasyProfile) -> EasyIdeas {
        build_ideas(
            "NVDA",
            182.45,
            "API",
            rows,
            profile,
            Some(0.48),
            Some(0.36),
            now(),
        )
    }

    fn find(ideas: &EasyIdeas, level: IdeaLevel) -> &EasyIdea {
        ideas
            .ideas
            .iter()
            .find(|i| i.level == level)
            .unwrap_or_else(|| panic!("{:?} missing", level))
    }

    /// p_itm 0.05 / 0.21 / 0.28 / 0.42 / 0.50 ⇒ safe 0.05, balanced 0.21
    /// (closest to 0.20), high 0.42 (best ann within p_itm ≤ 0.45); the 0.50
    /// row is too deep to ever qualify as high.
    fn selection_rows() -> Vec<EnrichedRow> {
        vec![
            put_row(150.0, 0.05, 0.98, 55.0, Some(3.0)),
            put_row(160.0, 0.21, 0.80, 180.0, Some(9.0)),
            put_row(162.0, 0.28, 0.74, 205.0, Some(10.0)),
            put_row(168.0, 0.42, 0.62, 320.0, Some(17.0)),
            put_row(175.0, 0.50, 0.55, 480.0, Some(40.0)),
        ]
    }

    #[test]
    fn level_selection_safe_balanced_high() {
        let ideas = build(&selection_rows(), &profile(20_000.0, 2, Stance::Want));
        assert_eq!(find(&ideas, IdeaLevel::Safe).strike, 150.0);
        assert_eq!(find(&ideas, IdeaLevel::Balanced).strike, 160.0);
        assert_eq!(find(&ideas, IdeaLevel::High).strike, 168.0);
        assert_eq!(ideas.ideas.len(), 3);
    }

    #[test]
    fn balanced_falls_back_to_next_above_safe_when_band_empty() {
        let rows = vec![
            put_row(150.0, 0.05, 0.98, 55.0, Some(3.0)),
            put_row(158.0, 0.10, 0.86, 110.0, Some(6.0)),
        ];
        let ideas = build(&rows, &profile(20_000.0, 2, Stance::Want));
        assert_eq!(find(&ideas, IdeaLevel::Safe).strike, 150.0);
        assert_eq!(find(&ideas, IdeaLevel::Balanced).strike, 158.0);
        assert_eq!(ideas.ideas.len(), 2, "no distinct high candidate");
    }

    #[test]
    fn single_candidate_gives_only_safe() {
        let rows = vec![put_row(150.0, 0.08, 0.90, 80.0, Some(5.0))];
        let ideas = build(&rows, &profile(20_000.0, 2, Stance::Want));
        assert_eq!(ideas.ideas.len(), 1);
        assert_eq!(ideas.ideas[0].level, IdeaLevel::Safe);
        assert_eq!(
            ideas.skip_reason_th, None,
            "an affordable idea means no skip reason"
        );
    }

    #[test]
    fn calls_and_unusable_rows_are_not_candidates() {
        let mut rows = selection_rows();
        let mut call = put_row(190.0, 0.30, 0.70, 300.0, Some(20.0));
        call.q.option_type = OptType::Call;
        rows.push(call);
        rows.push(put_row(155.0, 0.30, 0.70, 0.0, Some(20.0))); // zero credit
        rows.push(put_row(155.0, 0.30, f64::NAN, 100.0, Some(20.0))); // pop NaN
        rows.push(put_row(155.0, 0.30, 0.70, 100.0, Some(20.0))); // dte < 1
        rows.last_mut().unwrap().dte = 0.5;
        let ideas = build(&rows, &profile(20_000.0, 2, Stance::Want));
        assert_eq!(ideas.ideas.len(), 3);
        assert!(ideas.ideas.iter().all(|i| i.strike != 190.0));
        assert!(ideas.ideas.iter().all(|i| i.strike != 155.0));
    }

    #[test]
    fn recommendation_follows_comfort_cap_by_stance_and_risk() {
        let rows = vec![
            put_row(150.0, 0.05, 0.98, 55.0, Some(3.0)),
            put_row(160.0, 0.20, 0.81, 175.0, Some(10.0)),
            put_row(165.0, 0.29, 0.69, 260.0, Some(16.0)),
        ];
        // want: cap 15/25/30 (risk 3 capped at 30 to stay inside the
        // practitioner delta ≈16–30 range; the old ladder reached 35)
        let want = |risk: u8| build(&rows, &profile(20_000.0, risk, Stance::Want));
        assert!(find(&want(1), IdeaLevel::Safe).recommended);
        assert!(!find(&want(1), IdeaLevel::Balanced).recommended);
        assert!(
            find(&want(2), IdeaLevel::Balanced).recommended,
            "assign 20 ≤ cap 25 beats safe"
        );
        assert!(!find(&want(2), IdeaLevel::High).recommended, "29 > 25");
        assert!(find(&want(3), IdeaLevel::High).recommended, "29 ≤ cap 30");
        // avoid: cap 15 regardless of risk
        let avoid = build(&rows, &profile(20_000.0, 3, Stance::Avoid));
        assert!(find(&avoid, IdeaLevel::Safe).recommended);
        assert!(!find(&avoid, IdeaLevel::High).recommended);
        assert_eq!(avoid.stance, Stance::Avoid);
        // symbol not in the profile ⇒ want
        let plain = EasyProfile {
            cash: 20_000.0,
            risk: 3,
            ..EasyProfile::default()
        };
        assert_eq!(
            build(&rows, &plain).stance,
            Stance::Want,
            "unknown symbol defaults to want"
        );
    }

    #[test]
    fn cash_short_marks_flags_skip_reason_and_no_recommendation() {
        let ideas = build(&selection_rows(), &profile(5_000.0, 2, Stance::Want));
        assert!(ideas.ideas.iter().all(|i| i.cash_short));
        assert!(ideas.ideas.iter().all(|i| !i.recommended));
        let skip = ideas.skip_reason_th.expect("cash-short skip reason");
        assert!(
            skip.contains("$5,000") && skip.contains("$15,000"),
            "mentions saved cash and cheapest collateral: {skip}"
        );
        assert!(ideas.agent_say_th.contains("ยังไม่พอ"));
    }

    #[test]
    fn partial_cash_short_recommends_what_fits() {
        // safe costs $15,000, balanced $16,000, high $16,800: with $15,500
        // only safe fits, and it is within the cap ⇒ recommended.
        let ideas = build(&selection_rows(), &profile(15_500.0, 2, Stance::Want));
        assert!(!find(&ideas, IdeaLevel::Safe).cash_short);
        assert!(find(&ideas, IdeaLevel::Balanced).cash_short);
        assert!(find(&ideas, IdeaLevel::High).cash_short);
        assert!(find(&ideas, IdeaLevel::Safe).recommended);
        assert_eq!(ideas.skip_reason_th, None);
    }

    #[test]
    fn warn_flags_thirty_percent_or_above_cap() {
        let ideas = build(&selection_rows(), &profile(20_000.0, 2, Stance::Want));
        assert!(!find(&ideas, IdeaLevel::Safe).warn, "5% under cap 25");
        assert!(
            find(&ideas, IdeaLevel::High).warn,
            "42% above cap 25 and above 30"
        );
        let strict = build(&selection_rows(), &profile(20_000.0, 1, Stance::Want));
        assert!(
            find(&strict, IdeaLevel::Balanced).warn,
            "21% above cap 15 even though under 30"
        );
    }

    #[test]
    fn cases_sum_to_exactly_100() {
        for risk in [1u8, 2, 3] {
            let ideas = build(&selection_rows(), &profile(20_000.0, risk, Stance::Want));
            for idea in &ideas.ideas {
                let sum = idea.cases.keep_pct + idea.cases.between_pct + idea.cases.loss_pct;
                assert_eq!(sum, 100.0, "{:?} risk {risk}", idea.level);
                assert!(idea.cases.keep_pct >= 0.0);
                assert!(idea.cases.between_pct >= 0.0);
                assert!(idea.cases.loss_pct >= 0.0);
            }
            // pop > p_itm leaves "between" room on these rows
            let safe = find(&ideas, IdeaLevel::Safe);
            assert_eq!(safe.cases.keep_pct, 95.0);
            assert_eq!(safe.cases.between_pct, 3.0);
            assert_eq!(safe.cases.loss_pct, 2.0);
        }
    }

    #[test]
    fn worst_30_matches_the_reference_numbers() {
        // strike 165 / spot 182.45 / credit 220 ⇒ 0.7×182.45−165 = −37.285
        // per share ⇒ −3,728.5 + 220 = −3,508.5 (paper shows −3,509).
        let rows = vec![put_row(165.0, 0.25, 0.78, 220.0, Some(12.0))];
        let ideas = build(&rows, &profile(20_000.0, 2, Stance::Want));
        let idea = &ideas.ideas[0];
        assert!((idea.worst_30_usd - (-3508.5)).abs() < 1e-6);
        assert_eq!(idea.worst_30_usd.round(), -3509.0, "paper display");
        assert_eq!(idea.max_loss_usd, 165.0 * 100.0 - 220.0);
        assert_eq!(idea.premium, 2.2);
        assert_eq!(idea.credit, 220.0);
        assert_eq!(idea.cost, 16_500.0);
        assert!((idea.period_pct - (220.0 / 16_500.0 * 100.0)).abs() < 1e-9);
        assert_eq!(idea.annual_pct, Some(12.0));
        assert_eq!(idea.breakeven, 162.8);
        assert!((idea.below_spot_pct - (1.0 - 165.0 / 182.45) * 100.0).abs() < 1e-9);
        assert_eq!(idea.assign_pct, 25.0);
        assert_eq!(idea.days, 23.0);
        assert_eq!(idea.expiry, "2026-12-18");
    }

    #[test]
    fn deep_strike_below_thirty_percent_floor_keeps_the_credit() {
        // strike 100 is far below 0.7×182.45 ⇒ worst case keeps the credit.
        let rows = vec![put_row(100.0, 0.01, 0.97, 20.0, Some(2.0))];
        let ideas = build(&rows, &profile(20_000.0, 2, Stance::Want));
        assert_eq!(ideas.ideas[0].worst_30_usd, 20.0);
    }

    #[test]
    fn json_shape_matches_the_contract() {
        let ideas = build(&selection_rows(), &profile(18_000.0, 1, Stance::Want));
        let value = serde_json::to_value(&ideas).expect("serialises");
        for key in [
            "symbol",
            "spot",
            "source",
            "stance",
            "agent_say_th",
            "ideas",
            "skip_reason_th",
            "why_now",
            "computed_at",
        ] {
            assert!(value.get(key).is_some(), "EasyIdeas.{key} missing");
        }
        assert_eq!(value["stance"], "want");
        assert_eq!(value["source"], "API");
        assert_eq!(value["computed_at"], "2026-11-25T10:00:00+00:00");
        for key in ["iv_pct", "realized_30d_pct", "text_th"] {
            assert!(value["why_now"].get(key).is_some(), "WhyNow.{key} missing");
        }
        assert_eq!(value["why_now"]["iv_pct"], 48.0);
        assert_eq!(value["why_now"]["realized_30d_pct"], 36.0);
        for idea in value["ideas"].as_array().unwrap() {
            for key in [
                "occ",
                "symbol",
                "level",
                "recommended",
                "strike",
                "expiry",
                "days",
                "premium",
                "credit",
                "cost",
                "period_pct",
                "annual_pct",
                "breakeven",
                "below_spot_pct",
                "assign_pct",
                "cases",
                "worst_30_usd",
                "max_loss_usd",
                "warn",
                "cash_short",
            ] {
                assert!(idea.get(key).is_some(), "EasyIdea.{key} missing");
            }
            assert!(
                ["safe", "balanced", "high"].contains(&idea["level"].as_str().unwrap()),
                "level enum string"
            );
            for key in ["keep_pct", "between_pct", "loss_pct"] {
                assert!(idea["cases"].get(key).is_some(), "cases.{key} missing");
            }
            assert_eq!(idea["expiry"], "2026-12-18");
        }
        // a row without ann serialises annual_pct as null, not 0
        let rows = vec![put_row(150.0, 0.05, 0.98, 55.0, None)];
        let value =
            serde_json::to_value(build(&rows, &profile(18_000.0, 1, Stance::Want))).unwrap();
        assert!(value["ideas"][0]["annual_pct"].is_null());
    }

    #[test]
    fn agent_say_contains_no_option_jargon() {
        let forbidden = ["put", "strike", "delta", "iv", "premium", "option"];
        let mut texts = Vec::new();
        for stance in [Stance::Want, Stance::Avoid] {
            for risk in [1u8, 2, 3] {
                texts.push(build(&selection_rows(), &profile(18_000.0, risk, stance)).agent_say_th);
                texts.push(build(&selection_rows(), &profile(500.0, risk, stance)).agent_say_th);
            }
        }
        texts.push(build(&[], &profile(18_000.0, 1, Stance::Want)).agent_say_th);
        for text in texts {
            let words: Vec<String> = text
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|w| !w.is_empty())
                .map(str::to_ascii_lowercase)
                .collect();
            for word in forbidden {
                assert!(
                    !words.contains(&word.to_string()),
                    "jargon '{word}' in: {text}"
                );
            }
        }
    }

    #[test]
    fn agent_say_mentions_cash_comfort_and_level() {
        let ideas = build(&selection_rows(), &profile(18_000.0, 2, Stance::Want));
        let say = &ideas.agent_say_th;
        assert!(say.contains("NVDA") && say.contains("3 ทางเลือก"));
        assert!(say.contains("$18,000"), "cash: {say}");
        assert!(say.contains("2 ใน 10 ครั้ง"), "comfort: {say}");
        assert!(say.contains("แนะนำ"), "recommendation: {say}");

        let avoid = build(&selection_rows(), &profile(18_000.0, 3, Stance::Avoid));
        assert!(avoid.agent_say_th.contains("ไม่อยากถือหุ้น"));
    }

    #[test]
    fn why_now_branches() {
        let expensive = why_now(Some(0.48), Some(0.36));
        assert_eq!(expensive.iv_pct, Some(48.0));
        assert_eq!(expensive.realized_30d_pct, Some(36.0));
        assert!(expensive.text_th.contains("แพงกว่าปกติ"));
        assert!(expensive.text_th.contains("48%") && expensive.text_th.contains("36%"));

        let cheap = why_now(Some(0.30), Some(0.50));
        assert_eq!(cheap.text_th, "ค่าประกันตอนนี้ถูกกว่าการแกว่งจริง ไม่ค่อยคุ้มที่จะขาย");

        let neutral = why_now(Some(0.40), Some(0.40));
        assert!(neutral.text_th.contains("ใกล้เคียงกับ"));

        for missing in [
            why_now(None, Some(0.4)),
            why_now(Some(0.4), None),
            why_now(None, None),
        ] {
            assert_eq!(missing.text_th, "ยังไม่มีข้อมูลพอจะบอกว่าช่วงนี้คุ้มกว่าปกติไหม");
            assert!(missing.iv_pct.is_none() || missing.realized_30d_pct.is_none());
        }
    }

    #[test]
    fn source_token_maps_engine_labels() {
        assert_eq!(source_token("สด"), "LIVE");
        assert_eq!(source_token("API"), "API");
        assert_eq!(source_token("ราคาปิด"), "CLOSE");
        assert_eq!(source_token("DEMO"), "DEMO");
        assert_eq!(source_token("weird"), "API");
    }

    #[test]
    fn profile_normalisation_and_stance_lookup() {
        let raw = EasyProfile {
            onboarded: true,
            stocks: vec![
                EasyStock {
                    symbol: " nvda ".into(),
                    stance: Stance::Avoid,
                },
                EasyStock {
                    symbol: "NVDA".into(),
                    stance: Stance::Want,
                },
                EasyStock {
                    symbol: "  ".into(),
                    stance: Stance::Want,
                },
                EasyStock {
                    symbol: "tsla".into(),
                    stance: Stance::Want,
                },
            ],
            cash: 12_345.0,
            risk: 2,
        };
        let ok = normalise_profile(raw).expect("valid");
        assert_eq!(ok.stocks.len(), 2, "empty dropped, dup merged");
        assert_eq!(ok.stocks[0].symbol, "NVDA");
        assert_eq!(ok.stocks[0].stance, Stance::Avoid, "first occurrence wins");
        assert_eq!(stance_for(&ok, "NVDA"), Stance::Avoid);
        assert_eq!(stance_for(&ok, "AAPL"), Stance::Want);

        let bad_cash = EasyProfile {
            cash: -1.0,
            ..EasyProfile::default()
        };
        assert!(normalise_profile(bad_cash).is_err());
        let bad_risk = EasyProfile {
            risk: 4,
            ..EasyProfile::default()
        };
        assert!(normalise_profile(bad_risk).is_err());
        let too_many = EasyProfile {
            stocks: (0..11)
                .map(|i| EasyStock {
                    symbol: format!("S{i}"),
                    stance: Stance::Want,
                })
                .collect(),
            ..EasyProfile::default()
        };
        assert!(normalise_profile(too_many).is_err());
    }

    #[test]
    fn profile_serde_defaults_and_lenient_stance() {
        let loaded: EasyProfile = serde_json::from_str("{}").unwrap();
        assert_eq!(loaded, EasyProfile::default());
        assert_eq!(loaded.risk, 1);
        let weird: EasyProfile = serde_json::from_str(
            r#"{"onboarded":true,"stocks":[{"symbol":"NVDA","stance":"maybe"}],"cash":5,"risk":3}"#,
        )
        .unwrap();
        assert_eq!(weird.stocks[0].stance, Stance::Want, "unknown ⇒ want");
        let json = serde_json::to_string(&weird).unwrap();
        assert!(json.contains(r#""stance":"want""#));
    }
}
