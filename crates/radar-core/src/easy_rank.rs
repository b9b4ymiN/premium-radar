//! Easy Mode universe ranking (balanced) — คัด 1–3 ไอเดียขาย put ที่ดีที่สุด
//! จาก observation หนึ่งวันของ ~100 หุ้น US
//!
//! หลักการ BALANCED: premium/cushion/liquidity/vrp ถูกแปลงเป็น **percentile**
//! เทียบกับ candidates ในคืนเดียวกัน (ไม่ใช่ threshold ตายตัว) ตลาดเงียบ
//! ก็ยังได้ "ตัวที่ดีที่สุดของคืนนี้" เสมอ
//!
//! Units (สำคัญ): เงินทุกตัวเป็น USD ต่อ 1 สัญญา (×100) ตาม screener —
//! `credit` = bid×100, `collateral` = strike×100, `dte` เป็นวัน (t×365)
//! premium component = credit ÷ collateral ÷ dte (ผลตอบแทนต่อวัน)
//!
//! Stock observation ใช้ `contract.underlying` เป็น symbol (ดู
//! `lab::capture::stock_observation` — occ = underlying = symbol, multiplier 1)
//! option observation ใช้ occ เป็น key; contract ที่ multiplier ≠ 100 ถูก
//! ตัดทิ้งเพราะ screener คิดเงินแบบ ×100 เสมอ
//!
//! Weight เป็น "ค่าเริ่มต้นที่ยังไม่ได้พิสูจน์" (starting values, not proven)

use std::collections::HashMap;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::easy::{
    EasyIdea, EasyProfile, IdeaLevel, Stance, comfort_cap, fmt_usd, make_idea, stance_for,
};
use crate::lab::types::{ObsKind, Observation};
use crate::quant::OptType;
use crate::screener::{EnrichedRow, MAX_SPREAD_PCT, MIN_CREDIT, QuoteRow, enrich_rows_raw};

// ---------- weights (starting values, not proven) ----------

/// risk 1 (สบายใจน้อย): เน้น fit+cushion — โอกาสโดน assign ต้องต่ำและห่าง spot
const W_RISK1: Weights = Weights {
    fit: 0.25,
    cushion: 0.25,
    liquidity: 0.20,
    premium: 0.15,
    vrp: 0.15,
};
/// risk 2 (กลาง): สมดุลระหว่างผลตอบแทนและความปลอดภัย
const W_RISK2: Weights = Weights {
    fit: 0.20,
    cushion: 0.15,
    liquidity: 0.20,
    premium: 0.25,
    vrp: 0.20,
};
/// risk 3 (สบายใจมาก): ลดน้ำหนัก fit/cushion เพิ่ม premium
const W_RISK3: Weights = Weights {
    fit: 0.15,
    cushion: 0.10,
    liquidity: 0.20,
    premium: 0.35,
    vrp: 0.20,
};

#[derive(Clone, Copy)]
struct Weights {
    fit: f64,
    cushion: f64,
    liquidity: f64,
    premium: f64,
    vrp: f64,
}

impl Weights {
    fn for_risk(risk: u8) -> Self {
        match risk.clamp(1, 3) {
            1 => W_RISK1,
            2 => W_RISK2,
            _ => W_RISK3,
        }
    }
}

/// DTE window ต่อระดับความสบายใจ (วัน, inclusive) — practitioner ขาย
/// cash-secured put ส่วนใหญ่อยู่ราว 30–45 DTE; risk 1–2 ใช้ 14–45 เพื่อไม่รับ
/// สัญญาใกล้หมดอายุที่ราคาแกว่งแรง ส่วน risk 3 ลงไปถึง 10 วันได้เพื่อค่าตอบแทนต่อวันที่สูงกว่า
fn dte_window(risk: u8) -> (f64, f64) {
    match risk.clamp(1, 3) {
        1 | 2 => (14.0, 45.0),
        _ => (10.0, 45.0),
    }
}

/// OI ที่ถือว่า "มีคนถือมากพอ" สำหรับ caution (informational เท่านั้น)
const LOW_OI: f64 = 100.0;
/// Spread ที่เริ่มเป็น caution (%, เขต gate คือ MAX_SPREAD_PCT)
const WIDE_SPREAD_PCT: f64 = 10.0;
/// สัญญาราคาถูก (เซนต์ต่อหุ้น) ช่องว่าง bid-ask เป็น % ดูใหญ่แต่เป็นเงิน
/// น้อยมาก เช่น 0.13/0.18 = 32% แต่ต่างกันแค่ $0.05/หุ้น ($5 ต่อสัญญา) —
/// จึงยอมให้ผ่าน gate ด้วยเกณฑ์เงินสัมบูรณ์นี้คู่กับเกณฑ์ % (การให้คะแนน
/// liquidity ยังคิดจาก % เหมือนเดิม)
const MAX_SPREAD_ABS_USD: f64 = 0.05;

/// Spread gate ของ Easy Mode: ผ่านเมื่อกว้างเป็น % ไม่เกิน MAX_SPREAD_PCT
/// "หรือ" ช่องว่าง ask−bid เป็นเงินไม่เกิน MAX_SPREAD_ABS_USD ต่อหุ้น —
/// ขาด bid/ask ฝั่งใดฝั่งหนึ่ง (spread_pct เป็น None) ผ่านเหมือนเดิม
/// A per-share price for a sentence: whole dollars stay whole ("$10"), any
/// cents are kept ("$21.50", "$28.46") — rounding a strike of 21.5 to "$22"
/// would name a contract that does not exist.
fn fmt_price(v: f64) -> String {
    let cents = (v * 100.0).round() as i64;
    if cents % 100 == 0 {
        fmt_usd(v)
    } else {
        format!(
            "{}.{:02}",
            fmt_usd((cents / 100) as f64),
            (cents % 100).abs()
        )
    }
}

fn spread_ok(row: &EnrichedRow) -> bool {
    row.spread_pct.is_none_or(|sp| sp <= MAX_SPREAD_PCT)
        || row.q.ask - row.q.bid <= MAX_SPREAD_ABS_USD
}

// ---------- public API ----------

/// ข้อมูลเสริมต่อ symbol (stage 2) — ไม่มี ⇒ component vrp เป็น None
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SymbolExtras {
    /// Realised vol 30 วันเป็น fraction (0.36 = 36%/ปี)
    pub realized_30d: Option<f64>,
    pub earnings_date: Option<NaiveDate>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RankInput {
    /// Observation ของ session ที่จะจัดอันดับ
    pub today: Vec<Observation>,
    /// Session ก่อนหน้า (อาจว่าง) — ใช้เฉพาะคำนวณ oi_change_pct เท่านั้น
    pub previous: Vec<Observation>,
    pub extras: HashMap<String, SymbolExtras>,
    /// "API" | "LOCAL"
    pub source: String,
}

/// ส่วนประกอบคะแนนที่ใช้จริงตอนรวม (percentile สำหรับ vrp/premium/cushion/
/// liquidity, fit เป็นค่า absolute) — None = ข้อมูลไม่มี (renormalise ไม่ลงโทษ)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScoreParts {
    pub vrp: Option<f64>,
    pub premium: Option<f64>,
    pub fit: Option<f64>,
    pub cushion: Option<f64>,
    pub liquidity: Option<f64>,
    /// ตัวคูณ 0..1 (1 = ไม่มีเหตุการณ์) จาก earnings ก่อน expiry
    pub event_penalty: f64,
}

/// ข้อมูล OI ประกอบ (informational, weight 0 — ไม่มีผลกับคะแนน)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OiContext {
    /// Strike ที่มี put OI สูงสุด ณ expiry เดียวกัน (strike ≤ spot)
    pub put_wall_strike: Option<f64>,
    pub put_wall_oi: Option<f64>,
    /// % เปลี่ยน OI ของสัญญาที่เลือก เทียบ session ก่อน (None = ไม่มีข้อมูลเก่า)
    pub oi_change_pct: Option<f64>,
    pub note_th: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UniversePick {
    pub rank: u32,
    pub idea: EasyIdea,
    /// 0..100
    pub score: f64,
    pub parts: ScoreParts,
    pub reasons_th: Vec<String>,
    pub cautions_th: Vec<String>,
    /// ATM IV ของ symbol (%/ปี)
    pub iv_pct: Option<f64>,
    pub realized_30d_pct: Option<f64>,
    pub earnings_date: Option<String>,
    pub oi: OiContext,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BudgetShort {
    /// Collateral ถูกที่สุดในบรรดา candidates ที่ผ่าน gate
    pub min_cash_usd: f64,
    pub cheapest_symbol: String,
    pub text_th: String,
}

/// "เงินที่ต้องมี" ของหุ้นแต่ละตัวที่ user สนใจ (โปรไฟล์ + watchlist) —
/// เงินทุกตัวคิดต่อ 1 สัญญา (×100) เหมือนที่อื่นใน Easy Mode
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WatchCash {
    pub symbol: String,
    pub spot: Option<f64>,
    /// สัญญาพุตที่ถูกที่สุดที่ผ่านทุก quality gate (strike×100) —
    /// None = ไม่มีตัวไหนของ symbol นี้ผ่านเกณฑ์ในรอบนี้
    pub min_cash_usd: Option<f64>,
    /// สัญญาพุตที่ถูกที่สุดที่จับได้ใน DTE window และมี bid>0 (ไม่สน gate อื่น)
    pub any_cash_usd: Option<f64>,
    /// min_cash_usd ≤ profile.cash (false เมื่อยังไม่มีตัวที่ผ่านเกณฑ์)
    pub affordable: bool,
    pub text_th: String,
}

/// จำนวนไอเดียของคืนนี้ต่อระดับความเสี่ยง 1–3 — คำนวณจากข้อมูลและโปรไฟล์
/// ชุดเดียวกัน เปลี่ยนเฉพาะ `risk` (งบเงินเดิม) เพื่อให้ปุ่มสลับโหมดบอกได้
/// ว่าแต่ละโหมด "คืนนี้เหลือกี่ไอเดีย" ก่อนผู้ใช้กดสลับ
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModeCount {
    pub risk: u8,
    /// จำนวน picks ที่โหมดนี้จะแสดง (เหมือน `UniverseIdeas.picks` ของโหมดนั้น)
    pub picks: u32,
    /// จำนวน symbol ที่มีสัญญาผ่านเกณฑ์และจ่ายไหวอย่างน้อย 1 ตัวในโหมดนี้
    pub candidate_symbols: u32,
}

/// ผลตรวจ 5 ข้อของสัญญาที่เกือบผ่าน (เกณฑ์เดียวกับ gate ของ pipeline)
/// `cap_pct` คือเพดานโอกาสต้องซื้อหุ้นของโหมดปัจจุบัน (15/25/30)
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NearMissChecks {
    pub days_ok: bool,
    pub has_buyer: bool,
    pub credit_ok: bool,
    pub liquidity_ok: bool,
    pub assign_ok: bool,
    pub cap_pct: f64,
}

impl NearMissChecks {
    fn failed_count(self) -> u8 {
        (!self.days_ok) as u8
            + (!self.has_buyer) as u8
            + (!self.credit_ok) as u8
            + (!self.liquidity_ok) as u8
            + (!self.assign_ok) as u8
    }
}

/// สัญญาพุตที่ผู้ใช้จ่ายไหวแต่ตกเกณฑ์อย่างน้อย 1 ข้อของคืนนี้ — ใช้อธิบาย
/// ว่า "ทำไม" ยังไม่แนะนำ ไม่ใช่แค่บอกว่าไม่มีไอเดีย (เงินทุกตัวต่อ 1 สัญญา ×100)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NearMiss {
    pub ref_occ: String,
    pub symbol: String,
    pub strike: f64,
    pub expiry: String,
    pub days: f64,
    pub spot: f64,
    /// เงินที่ต้องกัน = strike×100 (แสดงเฉพาะตัวที่จ่ายไหวอยู่แล้ว)
    pub cash_needed_usd: f64,
    /// เงินสดที่ได้ทันทีฝั่ง bid
    pub credit_usd: f64,
    /// โอกาสต้องซื้อหุ้น (%/สัญญา) — ไม่มีข้อมูลคือ 100 (ตกเกณฑ์แน่นอน)
    pub assign_pct: f64,
    /// ask − bid ต่อหุ้น (ช่องว่างราคาเป็นเงิน)
    pub gap_usd: f64,
    pub bid_usd: f64,
    pub ask_usd: f64,
    /// ช่องว่างราคาเป็น % ของราคากลาง (None = ขาดราคาฝั่งใดฝั่งหนึ่ง)
    pub spread_pct: Option<f64>,
    pub checks: NearMissChecks,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UniverseIdeas {
    pub session_date: Option<String>,
    /// RFC3339 ของ option observation ล่าสุด
    pub as_of: Option<String>,
    pub source: String,
    pub scanned_symbols: u32,
    pub candidate_symbols: u32,
    /// จำนวนไอเดียคืนนี้ต่อ risk 1–3 (สำหรับปุ่มสลับโหมดหน้าบ้าน)
    pub mode_counts: Vec<ModeCount>,
    pub market_note_th: String,
    pub picks: Vec<UniversePick>,
    /// สัญญาที่จ่ายไหวแต่ยังไม่ผ่านเกณฑ์ — มีค่าเฉพาะเมื่อ picks ว่าง
    /// (งบไม่พอเลยมี budget_short อธิบายแทน จึงเว้นว่างไว้)
    pub near_misses: Vec<NearMiss>,
    pub budget_short: Option<BudgetShort>,
    /// เงินที่ต้องมีต่อหุ้นที่ user สนใจ (อาจว่างเมื่อไม่มี watch symbol)
    pub watch_cash: Vec<WatchCash>,
    pub agent_say_th: String,
    pub computed_at: String,
}

/// Stage 1: n symbols ที่ดีที่สุดโดยไม่ใช้ extras (vrp = None) สำหรับไปหา
/// realized vol / earnings มาเติม stage 2
pub fn shortlist(
    input: &RankInput,
    profile: &EasyProfile,
    now: DateTime<Utc>,
    n: usize,
) -> Vec<String> {
    let pipe = run_pipeline(input, profile, now, false);
    pipe.best_affordable
        .into_iter()
        .take(n)
        .map(|c| c.symbol)
        .collect()
}

/// Stage 2: จัดอันดับพร้อม extras — watch list คือหุ้นในโปรไฟล์ของ user
pub fn rank_universe(
    input: &RankInput,
    profile: &EasyProfile,
    now: DateTime<Utc>,
) -> UniverseIdeas {
    let watch: Vec<String> = profile.stocks.iter().map(|s| s.symbol.clone()).collect();
    rank_universe_with_watch(input, profile, &watch, now)
}

/// Stage 2 แบบระบุ watch list เอง (หุ้นในโปรไฟล์ + watchlist ของแอป):
/// เหมือน [`rank_universe`] แต่เพิ่ม `watch_cash` — เงินที่ต้องกันต่อหุ้น
/// ที่ user สนใจ (uppercase, dedup, เรียงตามลำดับที่ส่งมา, ไม่เกิน 10 ตัว)
pub fn rank_universe_with_watch(
    input: &RankInput,
    profile: &EasyProfile,
    watch: &[String],
    now: DateTime<Utc>,
) -> UniverseIdeas {
    let pipe = run_pipeline(input, profile, now, true);
    let weights = Weights::for_risk(profile.risk);

    // ---- budget short: ถ้าไม่มีตัวไหนที่สัญลักษณ์นั้นจ่ายไหว ----
    let budget_short = if pipe.best_affordable.is_empty() && pipe.cheapest.is_some() {
        pipe.cheapest
            .as_ref()
            .map(|(min_cash, symbol)| BudgetShort {
                min_cash_usd: *min_cash,
                cheapest_symbol: symbol.clone(),
                // อธิบายกติกา 100 หุ้นด้วย เพราะคนทั่วไปไม่รู้ว่าทำไมงบเท่านี้ไม่พอ
                text_th: format!(
                    "งบ ${} ยังไม่พอสำหรับไอเดียที่ผ่านเกณฑ์คืนนี้ · ขายสัญญา 1 ตัว = \
                     สัญญาว่าจะซื้อหุ้น 100 หุ้น จึงต้องกันเงินไว้ = ราคาที่นัด × 100 · \
                     ตัวที่ใช้เงินน้อยสุดที่ผ่านเกณฑ์คือ {symbol} ต้องกันเงิน ${}",
                    fmt_usd(profile.cash),
                    fmt_usd(*min_cash)
                ),
            })
    } else {
        None
    };

    // ---- picks ----
    let mut picks: Vec<UniversePick> = Vec::new();
    for (ix, cand) in pipe.best_affordable.iter().take(3).enumerate() {
        let rank = u32::try_from(ix + 1).unwrap_or(u32::MAX);
        let stance = stance_for(profile, &cand.symbol);
        let cap = comfort_cap(stance, profile.risk);
        let p_itm = cand.row.p_itm.unwrap_or_default();
        // level ตาม p_itm เทียบสามส่วนของ cap (เอกสารไว้ที่ fn level_for)
        let level = level_for(p_itm * 100.0, cap);
        let mut idea = make_idea(level, &cand.row, &cand.symbol, cand.spot, profile, cap);
        idea.recommended = rank == 1;

        let extras = input.extras.get(&cand.symbol);
        let earnings = extras.and_then(|e| e.earnings_date);
        let realized = extras.and_then(|e| e.realized_30d);

        let parts = ScoreParts {
            vrp: cand.comps.vrp,
            premium: cand.comps.premium,
            fit: cand.comps.fit,
            cushion: cand.comps.cushion,
            liquidity: cand.comps.liquidity,
            event_penalty: cand.event_penalty,
        };
        let reasons_th = reasons_from(&parts, &weights);
        let cautions_th = cautions_for(&cand.row, earnings);

        picks.push(UniversePick {
            rank,
            idea,
            score: cand.score,
            parts,
            reasons_th,
            cautions_th,
            iv_pct: cand.atm_iv.map(|v| v * 100.0),
            realized_30d_pct: realized.map(|v| v * 100.0),
            earnings_date: earnings.map(|d| d.format("%Y-%m-%d").to_string()),
            oi: oi_context(&pipe, cand),
        });
    }

    let market_note_th = market_note(pipe.median_vrp);
    let agent_say_th = match (&budget_short, picks.first()) {
        (Some(short), _) => short.text_th.clone(),
        (None, Some(top)) => format!(
            "คืนนี้คัดได้ {} ไอเดีย น่าสนใจสุดคือ {} เพราะ{}",
            picks.len(),
            top.idea.symbol,
            top.reasons_th
                .first()
                .map(String::as_str)
                .unwrap_or("เป็นตัวที่สมดุลที่สุดของคืนนี้")
        ),
        (None, None) => "คืนนี้ยังไม่มีหุ้นที่ผ่านการคัดกรองให้พิจารณา".to_string(),
    };

    let watch_cash = watch_cash_list(&pipe, profile, watch);

    // mode_counts: จำนวนไอเดียคืนนี้ของแต่ละ risk — โหมดที่กำลังใช้อยู่เอาจาก
    // pipeline หลักโดยตรง, อีกสองโหมดรัน pipeline ใหม่เฉพาะครั้งเดียว
    // (extras ไม่มีผลกับจำนวน เพราะกระทบแค่คะแนนเรียงลำดับ ไม่กระทบว่า
    // symbol ไหนผ่านเกณฑ์และจ่ายไหว)
    let cur_risk = profile.risk.clamp(1, 3);
    let mode_counts = [1u8, 2, 3]
        .map(|r| {
            if r == cur_risk {
                mode_count(r, &pipe.best_affordable)
            } else {
                let mut prof = profile.clone();
                prof.risk = r;
                let other = run_pipeline(input, &prof, now, false);
                mode_count(r, &other.best_affordable)
            }
        })
        .to_vec();

    // near_misses: มีค่าเฉพาะเมื่อไม่มี picks — เรียงตัวที่พลาดน้อยเกณฑ์
    // สุดก่อน แล้วตัวที่โอกาสต้องซื้อหุ้นใกล้เพดานของโหมดสุด (เกือบผ่าน
    // ที่สุดขึ้นหน้า), tie เรียงตาม occ เพื่อให้ deterministic
    let near_misses = if picks.is_empty() {
        let cap = comfort_cap(Stance::Want, cur_risk);
        let mut cands = pipe.near_rows;
        cands.sort_by(|a, b| {
            a.checks
                .failed_count()
                .cmp(&b.checks.failed_count())
                .then_with(|| {
                    (a.assign_pct - cap)
                        .abs()
                        .partial_cmp(&(b.assign_pct - cap).abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.ref_occ.cmp(&b.ref_occ))
        });
        cands.truncate(3);
        cands
    } else {
        Vec::new()
    };

    UniverseIdeas {
        session_date: pipe.session_date.map(|d| d.format("%Y-%m-%d").to_string()),
        as_of: pipe.as_of.map(|t| t.to_rfc3339()),
        source: input.source.clone(),
        scanned_symbols: pipe.scanned_symbols,
        candidate_symbols: pipe.candidate_symbols,
        mode_counts,
        market_note_th,
        picks,
        near_misses,
        budget_short,
        watch_cash,
        agent_say_th,
        computed_at: now.to_rfc3339(),
    }
}

/// picks ของโหมด = min(3, จำนวน symbol ที่ผ่านเกณฑ์และจ่ายไหว) —
/// best_affordable เก็บ symbol ละตัวอยู่แล้ว so len คือจำนวน symbol
fn mode_count(risk: u8, best: &[Cand]) -> ModeCount {
    ModeCount {
        risk,
        picks: u32::try_from(best.len().min(3)).unwrap_or(u32::MAX),
        candidate_symbols: u32::try_from(best.len()).unwrap_or(u32::MAX),
    }
}

/// สร้าง WatchCash ต่อ watch symbol: uppercase/dedup/ตามลำดับ/ไม่เกิน 10 ตัว
fn watch_cash_list(pipe: &PipeOut, profile: &EasyProfile, watch: &[String]) -> Vec<WatchCash> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for raw in watch {
        if out.len() >= 10 {
            break;
        }
        let sym = raw.trim().to_uppercase();
        if sym.is_empty() || !seen.insert(sym.clone()) {
            continue;
        }
        let Some(sc) = pipe.sym_cash.get(&sym) else {
            out.push(WatchCash {
                symbol: sym.clone(),
                spot: None,
                min_cash_usd: None,
                any_cash_usd: None,
                affordable: false,
                text_th: format!("ยังไม่มีข้อมูลของ {sym} ในรอบนี้"),
            });
            continue;
        };
        let affordable = sc.min_gated.is_some_and(|m| m <= profile.cash);
        // ประโยคเดียวที่หน้าบ้านต้องแสดง — เลขทุกตัวคิดใน Rust แล้ว
        let text_th = match (sc.min_gated, sc.min_any) {
            (Some(min), _) => format!(
                "{sym} ราคา ${} · ขาย 1 สัญญาต้องกันเงินราว ${} (ยอมซื้อ 100 หุ้นที่ ${})",
                fmt_price(sc.spot),
                fmt_usd(min),
                fmt_price(min / 100.0)
            ),
            (None, Some(any)) => format!(
                "{sym} ราคา ${} · ยังไม่มีสัญญาที่ปลอดภัยพอในรอบนี้ — ตัวที่ถูกสุดต้องกันเงิน ${}",
                fmt_price(sc.spot),
                fmt_usd(any)
            ),
            (None, None) => format!(
                "{sym} ราคา ${} · ยังไม่มีสัญญาที่ปลอดภัยพอในรอบนี้",
                fmt_price(sc.spot)
            ),
        };
        out.push(WatchCash {
            symbol: sym,
            spot: Some(sc.spot),
            min_cash_usd: sc.min_gated,
            any_cash_usd: sc.min_any,
            affordable,
            text_th,
        });
    }
    out
}

// ---------- pipeline ----------

#[derive(Clone, Copy)]
struct Components {
    vrp: Option<f64>,
    premium: Option<f64>,
    fit: Option<f64>,
    cushion: Option<f64>,
    liquidity: Option<f64>,
}

struct Cand {
    symbol: String,
    occ: String,
    row: EnrichedRow,
    spot: f64,
    atm_iv: Option<f64>,
    /// raw atm_iv/realized ratio (ก่อน map เป็น 0..1) — ใช้ทำ median market note
    vrp_ratio: Option<f64>,
    comps: Components,
    event_penalty: f64,
    score: f64,
}

/// ข้อมูลเงินที่ต้องกันของหุ้นหนึ่งตัวจากข้อมูลคืนนี้ — ใช้ทำ WatchCash
struct SymCash {
    spot: f64,
    /// collateral ต่ำสุดของสัญญาที่ผ่านทุก gate
    min_gated: Option<f64>,
    /// collateral ต่ำสุดของพุตที่มี bid>0 ใน DTE window (ไม่สน gate อื่น)
    min_any: Option<f64>,
}

struct PipeOut {
    /// contract ที่ดีที่สุด "ที่จ่ายไหว" ต่อ symbol เรียง score desc, symbol asc —
    /// ถ้าตัวท็อปของ symbol เกินงบ จะไต่ลงเอาตัวถัดไปของ symbol เดิมที่ผ่านงบ
    best_affordable: Vec<Cand>,
    /// (collateral ต่ำสุด, symbol) ในบรรดา gated contracts ทั้งหมด
    cheapest: Option<(f64, String)>,
    median_vrp: Option<f64>,
    scanned_symbols: u32,
    candidate_symbols: u32,
    session_date: Option<NaiveDate>,
    as_of: Option<DateTime<Utc>>,
    /// latest option quote ต่อ occ (วันนี้) — ใช้ทำ OiContext
    latest: HashMap<String, Observation>,
    /// latest OI ต่อ occ จาก `previous` — ใช้ทำ oi_change_pct เท่านั้น
    prev_oi: HashMap<String, f64>,
    /// ต่อ symbol ที่สแกน (มี spot + quote): เงินที่ต้องกันต่ำสุดแบบผ่าน gate
    /// และแบบไม่สน gate — ใช้ทำ watch_cash
    sym_cash: HashMap<String, SymCash>,
    /// พุตทุกตัวที่ผู้ใช้จ่ายไหว (collateral ≤ งบ) พร้อมผลตรวจ 5 ข้อ —
    /// คัดเหลือ 3 ตัวเป็น near_misses เมื่อไม่มี picks; ตัวที่คำนวณเงินที่
    /// ต้องกันไม่ได้ (ไม่มีราคาเลย) ถูกเว้นเพราะตัดสินความจ่ายไหวไม่ได้
    near_rows: Vec<NearMiss>,
}

fn run_pipeline(
    input: &RankInput,
    profile: &EasyProfile,
    now: DateTime<Utc>,
    use_extras: bool,
) -> PipeOut {
    let risk = profile.risk.clamp(1, 3);
    let (dte_lo, dte_hi) = dte_window(risk);
    let gate_cap_pct = comfort_cap(Stance::Want, risk);
    let fit_target = 0.7 * gate_cap_pct / 100.0;

    // 1. latest ต่อ occ (option) และ spot ต่อ underlying (stock)
    let mut latest: HashMap<String, Observation> = HashMap::new();
    let mut stock: HashMap<&str, Observation> = HashMap::new();
    let mut session_date: Option<NaiveDate> = None;
    let mut as_of: Option<DateTime<Utc>> = None;
    for obs in &input.today {
        session_date = Some(
            session_date
                .unwrap_or(obs.session_date)
                .max(obs.session_date),
        );
        match &obs.kind {
            ObsKind::OptionQuote { .. } => {
                as_of = Some(as_of.unwrap_or(obs.event_time).max(obs.event_time));
                let keep_newer = latest.get(&obs.contract.occ).is_none_or(|cur| {
                    (obs.event_time, obs.received_time) > (cur.event_time, cur.received_time)
                });
                if keep_newer {
                    latest.insert(obs.contract.occ.clone(), obs.clone());
                }
            }
            ObsKind::StockQuote { price, .. } => {
                if price.is_some_and(|p| p.is_finite() && p > 0.0) {
                    let keep_newer = stock
                        .get(obs.contract.underlying.as_str())
                        .is_none_or(|cur| obs.event_time > cur.event_time);
                    if keep_newer {
                        stock.insert(obs.contract.underlying.as_str(), obs.clone());
                    }
                }
            }
            ObsKind::OptionBar { .. } => {}
        }
    }

    // latest OI ต่อ occ จาก previous session — ใช้ทำ oi_change_pct เท่านั้น
    let mut prev_oi: HashMap<String, f64> = HashMap::new();
    let mut prev_time: HashMap<String, DateTime<Utc>> = HashMap::new();
    for obs in &input.previous {
        if let ObsKind::OptionQuote { oi: Some(oi), .. } = &obs.kind {
            let keep_newer = prev_time
                .get(&obs.contract.occ)
                .is_none_or(|t| obs.event_time > *t);
            if keep_newer {
                prev_oi.insert(obs.contract.occ.clone(), *oi);
                prev_time.insert(obs.contract.occ.clone(), obs.event_time);
            }
        }
    }

    // group latest option quotes ต่อ symbol
    let mut by_symbol: HashMap<&str, Vec<&Observation>> = HashMap::new();
    for obs in latest.values() {
        by_symbol
            .entry(obs.contract.underlying.as_str())
            .or_default()
            .push(obs);
    }
    let mut symbols: Vec<&str> = by_symbol
        .keys()
        .copied()
        .filter(|s| stock.contains_key(s))
        .collect();
    symbols.sort_unstable();

    // 2–3. enrich + gates + raw components
    let mut cands: Vec<Cand> = Vec::new();
    let mut cheapest: Option<(f64, String)> = None;
    let mut sym_cash: HashMap<String, SymCash> = HashMap::new();
    let mut near_rows: Vec<NearMiss> = Vec::new();
    for sym in &symbols {
        let spot = match &stock[sym].kind {
            ObsKind::StockQuote { price, .. } => price.expect("spot filtered > 0"),
            _ => unreachable!("stock map holds StockQuote only"),
        };
        let obs_refs = &by_symbol[sym];
        let rows: Vec<QuoteRow> = obs_refs
            .iter()
            .filter(|o| o.contract.option_type == OptType::Put && o.contract.multiplier == 100.0)
            .filter_map(|o| match &o.kind {
                ObsKind::OptionQuote {
                    price,
                    bid,
                    ask,
                    oi,
                    volume,
                    ..
                } => Some(quote_row(
                    &o.contract,
                    *price,
                    *bid,
                    *ask,
                    *oi,
                    *volume,
                    &input.source,
                )),
                _ => None,
            })
            .collect();
        let enriched = enrich_rows_raw(rows, spot, now);
        let atm_iv = atm_iv_for_symbol(&enriched, spot);

        let extras = if use_extras {
            input.extras.get(*sym)
        } else {
            None
        };
        let realized = extras.and_then(|e| e.realized_30d);
        let earnings = extras.and_then(|e| e.earnings_date);
        let vrp_ratio = match (atm_iv, realized) {
            (Some(iv), Some(rv)) if rv > 0.0 => Some(iv / rv),
            _ => None,
        };

        // เงินที่ต้องกันต่ำสุดของ symbol นี้สองแบบ (ผ่าน gate / ไม่สน gate)
        // ใช้ทำ watch_cash — พุตที่ไม่ผ่าน gate ก็ยังบอกได้ว่าถูกสุดกี่ตัง
        let mut min_gated: Option<f64> = None;
        let mut min_any: Option<f64> = None;
        for row in enriched {
            let gated = row.q.option_type == OptType::Put
                && (dte_lo..=dte_hi).contains(&row.dte)
                && row.q.bid > 0.0
                && row.credit.is_some_and(|c| c >= MIN_CREDIT * 100.0)
                && spread_ok(&row)
                && row.iv.is_some()
                && row.p_itm.is_some_and(|p| p * 100.0 <= gate_cap_pct);
            if row.q.bid > 0.0
                && (dte_lo..=dte_hi).contains(&row.dte)
                && let Some(col) = row.collateral
            {
                min_any = Some(min_any.map_or(col, |m| m.min(col)));
            }
            if gated && let Some(col) = row.collateral {
                min_gated = Some(min_gated.map_or(col, |m| m.min(col)));
            }
            if let Some(col) = row.collateral.filter(|c| *c <= profile.cash) {
                near_rows.push(NearMiss {
                    ref_occ: row.q.symbol.clone(),
                    symbol: row.q.underlying.clone(),
                    strike: row.q.strike,
                    expiry: row.q.expiry.format("%Y-%m-%d").to_string(),
                    days: row.dte,
                    spot,
                    cash_needed_usd: col,
                    credit_usd: row.credit.unwrap_or_default(),
                    assign_pct: row.p_itm.map(|p| p * 100.0).unwrap_or(100.0),
                    gap_usd: row.q.ask - row.q.bid,
                    bid_usd: row.q.bid,
                    ask_usd: row.q.ask,
                    spread_pct: row.spread_pct,
                    checks: NearMissChecks {
                        days_ok: (dte_lo..=dte_hi).contains(&row.dte),
                        has_buyer: row.q.bid > 0.0,
                        credit_ok: row.credit.is_some_and(|c| c >= MIN_CREDIT * 100.0),
                        liquidity_ok: spread_ok(&row),
                        assign_ok: row.p_itm.is_some_and(|p| p * 100.0 <= gate_cap_pct),
                        cap_pct: gate_cap_pct,
                    },
                });
            }
            if !gated {
                continue;
            }
            let (credit, collateral, iv, p_itm) = (
                row.credit.unwrap_or_default(),
                row.collateral.unwrap_or_default(),
                row.iv.unwrap_or_default(),
                row.p_itm.unwrap_or_default(),
            );
            let dte = row.dte.max(1.0);
            let premium_raw = credit / collateral / dte;
            let fit = (1.0 - (p_itm - fit_target).abs() / fit_target).clamp(0.0, 1.0);
            let cushion_raw =
                ((spot - row.q.strike) / (spot * iv * (dte / 365.0).sqrt())).clamp(0.0, 2.0);
            let liquidity_raw = row.spread_pct.map(|sp| {
                0.5 * (1.0 - (sp / MAX_SPREAD_PCT).clamp(0.0, 1.0))
                    + 0.5 * ((1.0 + row.q.oi).ln() / 10_001.0f64.ln()).clamp(0.0, 1.0)
            });
            let vrp_mapped = vrp_ratio.map(|r| ((r - 0.8) / 0.7).clamp(0.0, 1.0));

            let event_penalty = match earnings {
                Some(ed) if ed <= row.q.expiry => {
                    if risk == 1 {
                        0.4
                    } else {
                        0.6
                    }
                }
                _ => 1.0,
            };

            // cheapest collateral ในบรรดา gated ทั้งหมด (tie: symbol เรียงตามตัวอักษร)
            let better = match &cheapest {
                None => true,
                Some((c, cs)) => collateral < *c || (collateral == *c && row.q.underlying < *cs),
            };
            if better {
                cheapest = Some((collateral, row.q.underlying.clone()));
            }

            cands.push(Cand {
                symbol: row.q.underlying.clone(),
                occ: row.q.symbol.clone(),
                row,
                spot,
                atm_iv,
                vrp_ratio,
                comps: Components {
                    vrp: vrp_mapped,
                    premium: Some(premium_raw),
                    fit: Some(fit),
                    cushion: Some(cushion_raw),
                    liquidity: liquidity_raw,
                },
                event_penalty,
                score: 0.0,
            });
        }
        sym_cash.insert(
            (*sym).to_string(),
            SymCash {
                spot,
                min_gated,
                min_any,
            },
        );
    }

    let candidate_symbols = u32::try_from(
        cands
            .iter()
            .map(|c| c.symbol.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
    )
    .unwrap_or(u32::MAX);

    // 4. percentile pools ข้ามทุก gated contract (เฉพาะตัวที่มีค่า)
    let prem_pool: Vec<f64> = cands.iter().filter_map(|c| c.comps.premium).collect();
    let cush_pool: Vec<f64> = cands.iter().filter_map(|c| c.comps.cushion).collect();
    let liq_pool: Vec<f64> = cands.iter().filter_map(|c| c.comps.liquidity).collect();
    let vrp_pool: Vec<f64> = cands.iter().filter_map(|c| c.comps.vrp).collect();
    for cand in &mut cands {
        let c = &mut cand.comps;
        c.premium = c.premium.map(|v| pct_le(&prem_pool, v));
        c.cushion = c.cushion.map(|v| pct_le(&cush_pool, v));
        c.liquidity = c.liquidity.map(|v| pct_le(&liq_pool, v));
        c.vrp = c.vrp.map(|v| pct_le(&vrp_pool, v));
    }

    // 5. score
    let weights = Weights::for_risk(risk);
    for cand in &mut cands {
        cand.score = weighted_score(cand.comps, &weights) * cand.event_penalty;
    }

    let median_vrp = median(cands.iter().filter_map(|c| c.vrp_ratio).collect());

    // 6. best ต่อ symbol จากกลุ่มที่จ่ายไหว (score desc, occ asc) —
    // ตัวท็อปของ symbol เกินงบ ไม่ตัดทั้ง symbol: ไต่ลงไปเอาตัวถัดไปที่ผ่านงบ
    // (percentile pools ยังคิดจาก gated ทั้งหมดตามขั้นตอนก่อนหน้า ไม่เปลี่ยน)
    cands.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.occ.cmp(&b.occ))
    });
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut best_affordable: Vec<Cand> = Vec::new();
    for cand in cands {
        if seen.contains(&cand.symbol) {
            continue;
        }
        if cand.row.collateral.unwrap_or(f64::MAX) <= profile.cash {
            seen.insert(cand.symbol.clone());
            best_affordable.push(cand);
        }
    }
    best_affordable.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });

    PipeOut {
        best_affordable,
        cheapest,
        median_vrp,
        scanned_symbols: u32::try_from(symbols.len()).unwrap_or(u32::MAX),
        candidate_symbols,
        session_date,
        as_of,
        latest,
        prev_oi,
        sym_cash,
        near_rows,
    }
}

/// QuoteRow จาก observation: price = price หรือ mid ของ bid/ask, bid/ask หาย = 0
/// (screener จะ fallback เอง; spread_pct จะเป็น None เมื่อขาดฝั่งใดฝั่งหนึ่ง)
fn quote_row(
    contract: &crate::lab::types::ContractRef,
    price: Option<f64>,
    bid: Option<f64>,
    ask: Option<f64>,
    oi: Option<f64>,
    volume: Option<f64>,
    src: &str,
) -> QuoteRow {
    let mid = match (bid, ask) {
        (Some(b), Some(a)) if b > 0.0 && a > 0.0 => Some((a + b) / 2.0),
        _ => None,
    };
    QuoteRow {
        symbol: contract.occ.clone(),
        underlying: contract.underlying.clone(),
        expiry: contract.expiry,
        option_type: contract.option_type,
        strike: contract.strike,
        price: price.or(mid).unwrap_or(0.0),
        bid: bid.unwrap_or(0.0),
        ask: ask.unwrap_or(0.0),
        volume: volume.unwrap_or(0.0),
        oi: oi.unwrap_or(0.0),
        change: 0.0,
        change_ratio: 0.0,
        src: src.to_string(),
    }
}

/// ATM IV ของ symbol: expiry ที่ dte ใกล้ 30 วันที่สุด (tie: expiry เร็วกว่า)
/// แล้วเอา iv ของ put ที่ strike ใกล้ spot ที่สุด (tie: strike ต่ำกว่า)
/// — เก็บง่ายและ deterministic ตามที่ spec อนุญาต ("keep simple, document it")
fn atm_iv_for_symbol(rows: &[EnrichedRow], spot: f64) -> Option<f64> {
    let mut puts: Vec<&EnrichedRow> = rows
        .iter()
        .filter(|r| r.q.option_type == OptType::Put && r.iv.is_some())
        .collect();
    if puts.is_empty() {
        return None;
    }
    puts.sort_by(|a, b| {
        // ใกล้ 30 วันที่สุดก่อน (dte มาจาก years_to_expiry ของ row เอง)
        ((a.dte - 30.0).abs())
            .partial_cmp(&(b.dte - 30.0).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.q.expiry.cmp(&b.q.expiry))
    });
    let chosen_expiry = puts[0].q.expiry;
    puts.iter()
        .filter(|r| r.q.expiry == chosen_expiry)
        .min_by(|a, b| {
            (a.q.strike - spot)
                .abs()
                .partial_cmp(&(b.q.strike - spot).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    a.q.strike
                        .partial_cmp(&b.q.strike)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        })
        .and_then(|r| r.iv)
}

/// percentile rank: สัดส่วนของค่าใน pool ที่ ≤ x (pool ไม่ว่าง)
fn pct_le(pool: &[f64], x: f64) -> f64 {
    if pool.is_empty() {
        return 0.0;
    }
    let below = pool.iter().filter(|v| **v <= x).count() as f64;
    below / pool.len() as f64
}

fn median(mut vals: Vec<f64>) -> Option<f64> {
    if vals.is_empty() {
        return None;
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = vals.len() / 2;
    Some(if vals.len() % 2 == 1 {
        vals[mid]
    } else {
        (vals[mid - 1] + vals[mid]) / 2.0
    })
}

fn weighted_score(comps: Components, w: &Weights) -> f64 {
    let pairs = [
        (comps.fit, w.fit),
        (comps.cushion, w.cushion),
        (comps.liquidity, w.liquidity),
        (comps.premium, w.premium),
        (comps.vrp, w.vrp),
    ];
    let mut sum = 0.0;
    let mut wsum = 0.0;
    for (val, weight) in pairs {
        if let Some(v) = val {
            sum += weight * v;
            wsum += weight;
        }
    }
    if wsum == 0.0 {
        return 0.0;
    }
    100.0 * sum / wsum
}

/// level ตาม p_itm เทียบ cap สามส่วน: ≤ cap/3 = Safe, ≤ 2cap/3 = Balanced,
/// เกินนั้น = High (cap มาจาก stance จริงของ user ต่อ symbol นั้น)
fn level_for(p_itm_pct: f64, cap: f64) -> IdeaLevel {
    if p_itm_pct <= cap / 3.0 {
        IdeaLevel::Safe
    } else if p_itm_pct <= cap * 2.0 / 3.0 {
        IdeaLevel::Balanced
    } else {
        IdeaLevel::High
    }
}

/// เหตุผล 2–4 ข้อจาก component ที่มี contribution (weight×ค่า) สูงสุด
fn reasons_from(parts: &ScoreParts, w: &Weights) -> Vec<String> {
    let mut items: Vec<(f64, &str)> = Vec::new();
    if let Some(v) = parts.vrp {
        items.push((w.vrp * v, "ค่าประกันแพงกว่าการแกว่งจริงของหุ้น"));
    }
    if let Some(v) = parts.premium {
        items.push((w.premium * v, "ได้เงินคุ้มกับเงินที่ต้องกัน"));
    }
    if let Some(v) = parts.fit {
        items.push((w.fit * v, "โอกาสต้องซื้อหุ้นใกล้ระดับที่คุณสบายใจ"));
    }
    if let Some(v) = parts.cushion {
        items.push((w.cushion * v, "ราคาที่ยอมซื้ออยู่ไกลกว่ากรอบที่หุ้นมักแกว่ง"));
    }
    if let Some(v) = parts.liquidity {
        items.push((w.liquidity * v, "ซื้อขายคล่อง"));
    }
    items.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    items
        .into_iter()
        .take(3)
        .map(|(_, s)| s.to_string())
        .collect()
}

fn cautions_for(row: &EnrichedRow, earnings: Option<NaiveDate>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(ed) = earnings.filter(|ed| *ed <= row.q.expiry) {
        out.push(format!(
            "จะแจ้งผลประกอบการวันที่ {} ก่อนสัญญาหมดอายุ ราคาอาจแกว่งแรง",
            thai_date(ed)
        ));
    }
    if row.spread_pct.is_some_and(|sp| sp > WIDE_SPREAD_PCT) {
        out.push(format!(
            "ช่องว่างราคาซื้อ-ขายกว้าง {:.0}% อาจปิดสถานะยาก",
            row.spread_pct.unwrap_or_default()
        ));
    }
    if row.q.oi < LOW_OI {
        out.push("คนถือสัญญานี้น้อย ซื้อขายอาจลึก".to_string());
    }
    out
}

const THAI_MONTHS: [&str; 12] = [
    "ม.ค.",
    "ก.พ.",
    "มี.ค.",
    "เม.ย.",
    "พ.ค.",
    "มิ.ย.",
    "ก.ค.",
    "ส.ค.",
    "ก.ย.",
    "ต.ค.",
    "พ.ย.",
    "ธ.ค.",
];

fn thai_date(d: NaiveDate) -> String {
    format!(
        "{} {} {}",
        d.day(),
        THAI_MONTHS[d.month0() as usize],
        d.year() + 543
    )
}

fn market_note(median_vrp: Option<f64>) -> String {
    match median_vrp {
        None => "ยังไม่มีข้อมูลความผันผวนย้อนหลังพอจะบอกว่าคืนนี้ค่าประกันแพงหรือถูกกว่าปกติ".to_string(),
        Some(v) if v < 0.95 => "คืนนี้ค่าประกันโดยรวมถูกกว่าช่วงปกติ จึงคัดจากตัวที่ดีที่สุดของคืนนี้เท่านั้น".to_string(),
        Some(v) if v > 1.15 => "คืนนี้ค่าประกันโดยรวมแพงกว่าช่วงปกติ เป็นช่วงที่ขายค่าประกันได้ราคาดี".to_string(),
        Some(_) => "ระดับราคาค่าประกันคืนนี้ใกล้เคียงช่วงปกติ".to_string(),
    }
}

/// OiContext ของสัญญาที่เลือก — informational ล้วน (weight 0 ไม่แตะคะแนน)
fn oi_context(pipe: &PipeOut, cand: &Cand) -> OiContext {
    // put wall: strike ที่ put OI สูงสุด ณ expiry เดียวกัน, strike ≤ spot
    let mut wall: Option<(f64, f64)> = None; // (oi, strike)
    for obs in pipe.latest.values() {
        if obs.contract.underlying != cand.symbol
            || obs.contract.expiry != cand.row.q.expiry
            || obs.contract.option_type != OptType::Put
            || obs.contract.strike > cand.spot
        {
            continue;
        }
        if let ObsKind::OptionQuote { oi: Some(oi), .. } = &obs.kind {
            let better = wall.is_none_or(|(cur_oi, _)| {
                *oi > cur_oi || (*oi == cur_oi && obs.contract.strike > wall.expect("some").1)
            });
            if better {
                wall = Some((*oi, obs.contract.strike));
            }
        }
    }
    let (put_wall_strike, put_wall_oi) = match wall {
        Some((oi, strike)) => (Some(strike), Some(oi)),
        None => (None, None),
    };

    // oi change เทียบ previous session (latest OI ต่อ occ เดิม)
    let oi_change_pct = pipe.prev_oi.get(&cand.occ).and_then(|prev| {
        (prev.is_finite() && *prev > 0.0)
            .then(|| (cand.row.q.oi - prev) / prev * 100.0)
            .filter(|v| v.is_finite())
    });

    OiContext {
        put_wall_strike,
        put_wall_oi,
        oi_change_pct,
        note_th: "ข้อมูล OI เป็นข้อมูลประกอบ ยังไม่ได้พิสูจน์ว่าช่วยทำนายได้".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::DataSource;
    use crate::lab::types::ContractRef;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 25, 22, 0, 0).unwrap()
    }

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 25, h, m, 0).unwrap()
    }

    fn prev_day(h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 24, h, 0, 0).unwrap()
    }

    fn put(
        sym: &str,
        strike: f64,
        expiry: NaiveDate,
        event: DateTime<Utc>,
        bid: f64,
        ask: f64,
        oi: f64,
    ) -> Observation {
        let occ = format!("{sym}{}P{strike:0>8.0}", expiry.format("%y%m%d"));
        Observation {
            contract: ContractRef {
                occ,
                underlying: sym.to_string(),
                expiry,
                option_type: OptType::Put,
                strike,
                multiplier: 100.0,
            },
            kind: ObsKind::OptionQuote {
                price: Some((ask + bid) / 2.0),
                bid: Some(bid),
                ask: Some(ask),
                iv: None,
                delta: None,
                oi: Some(oi),
                volume: Some(oi / 2.0),
            },
            event_time: event,
            received_time: event,
            session_date: event.date_naive(),
            available_at: event,
            source: DataSource::Rest,
            backfill: false,
            universe_id: Some("test".into()),
        }
    }

    fn stock(sym: &str, price: f64, event: DateTime<Utc>) -> Observation {
        Observation {
            contract: ContractRef {
                occ: sym.to_string(),
                underlying: sym.to_string(),
                expiry: event.date_naive(),
                option_type: OptType::Call,
                strike: 0.0,
                multiplier: 1.0,
            },
            kind: ObsKind::StockQuote {
                price: Some(price),
                open: None,
                high: None,
                low: None,
                pre_close: None,
                volume: None,
            },
            event_time: event,
            received_time: event,
            session_date: event.date_naive(),
            available_at: event,
            source: DataSource::Rest,
            backfill: false,
            universe_id: Some("test".into()),
        }
    }

    fn profile(cash: f64, risk: u8) -> EasyProfile {
        EasyProfile {
            onboarded: true,
            stocks: Vec::new(),
            cash,
            risk,
        }
    }

    fn input(today: Vec<Observation>) -> RankInput {
        RankInput {
            today,
            previous: Vec::new(),
            extras: HashMap::new(),
            source: "API".into(),
        }
    }

    fn exp_dec24() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 12, 24).unwrap()
    }

    /// 6 หุ้น ทุกตัว ~30 DTE (เอาไว้ทดสอบ a/d/f/g/h/i)
    fn base_today() -> Vec<Observation> {
        let (e, t) = (exp_dec24(), at(21, 0));
        [
            // (sym, spot, strike, bid, ask, oi)
            ("F", 11.0, 10.0, 0.11, 0.13, 1200.0),
            ("GM", 42.0, 38.0, 0.45, 0.49, 3000.0),
            ("INTC", 24.0, 21.0, 0.28, 0.32, 8000.0),
            ("PFE", 33.0, 30.0, 0.30, 0.34, 2500.0),
            ("T", 26.0, 23.0, 0.20, 0.24, 6000.0),
            ("KO", 60.0, 55.0, 0.50, 0.55, 9000.0),
        ]
        .into_iter()
        .flat_map(|(sym, spot, strike, bid, ask, oi)| {
            vec![stock(sym, spot, t), put(sym, strike, e, t, bid, ask, oi)]
        })
        .collect()
    }

    /// ตลาดเงียบ: premium ต่ำทุกตัวแต่ยังได้ picks (percentile ไม่ใช่ threshold)
    #[test]
    fn quiet_market_still_picks() {
        let (e, t) = (exp_dec24(), at(21, 0));
        let today: Vec<Observation> = [
            ("F", 11.0, 10.5, 0.05, 0.06, 900.0),
            ("GM", 42.0, 40.5, 0.11, 0.12, 1500.0),
            ("INTC", 24.0, 23.0, 0.06, 0.07, 2000.0),
            ("PFE", 33.0, 32.0, 0.09, 0.10, 1200.0),
            ("T", 26.0, 25.0, 0.06, 0.07, 1800.0),
            ("KO", 60.0, 58.0, 0.13, 0.14, 3000.0),
        ]
        .into_iter()
        .flat_map(|(sym, spot, strike, bid, ask, oi)| {
            vec![stock(sym, spot, t), put(sym, strike, e, t, bid, ask, oi)]
        })
        .collect();
        let out = rank_universe(&input(today), &profile(50_000.0, 2), now());
        assert_eq!(out.candidate_symbols, 6, "ทุก symbol ผ่าน gate แม้ premium ต่ำ");
        assert!(!out.picks.is_empty(), "quiet market ยังต้องได้ picks");
        assert!(out.picks.iter().all(|p| p.score > 0.0));
    }

    /// เงินไม่พอ: picks ว่าง + budget_short บอก collateral ถูกสุด
    #[test]
    fn cash_too_small_gives_budget_short() {
        let out = rank_universe(&input(base_today()), &profile(900.0, 2), now());
        assert!(out.picks.is_empty());
        let short = out.budget_short.expect("budget short present");
        assert_eq!(short.cheapest_symbol, "F");
        assert_eq!(short.min_cash_usd, 1000.0, "F strike 10 × 100");
        assert!(short.text_th.contains("$1,000"), "text: {}", short.text_th);
        assert!(short.text_th.contains("F"));
        assert!(short.text_th.contains("100 หุ้น"), "ต้องอธิบายกติกา 100 หุ้น");
        assert_eq!(out.agent_say_th, short.text_th);
        assert_eq!(out.candidate_symbols, 6, "gates ผ่าน เฉพาะงบที่ไม่พอ");
    }

    /// risk1 (dte 14-45, cap 15%) vs risk3 (dte 10-45, cap 30%): AAA ตกเพราะ
    /// dte 12, BBB ตกเพราะ p_itm ~21% — risk1 เหลือ CCC อย่างเดียว,
    /// risk3 ได้ครบและตัวท็อปเปลี่ยน
    fn risk_today() -> Vec<Observation> {
        let (t, near, far) = (
            at(21, 0),
            NaiveDate::from_ymd_opt(2026, 12, 7).unwrap(),
            exp_dec24(),
        );
        vec![
            stock("AAA", 50.0, t),
            put("AAA", 45.0, near, t, 0.92, 0.98, 4000.0),
            stock("BBB", 50.0, t),
            put("BBB", 45.0, far, t, 0.63, 0.67, 4000.0),
            stock("CCC", 50.0, t),
            put("CCC", 45.0, far, t, 0.20, 0.24, 4000.0),
        ]
    }

    #[test]
    fn risk_window_changes_candidates_and_order() {
        let inp = input(risk_today());
        let r1 = rank_universe(&inp, &profile(50_000.0, 1), now());
        let r3 = rank_universe(&inp, &profile(50_000.0, 3), now());
        assert_eq!(
            r1.picks
                .iter()
                .map(|p| p.idea.symbol.clone())
                .collect::<Vec<_>>(),
            vec!["CCC".to_string()],
            "risk1: AAA ตกที่ dte, BBB ตกที่ p_itm"
        );
        assert_eq!(r3.picks.len(), 3);
        assert_ne!(
            r3.picks[0].idea.symbol, "CCC",
            "risk3 ตัวท็อปต้องเปลี่ยนไปตัวที่ premium สูงกว่า"
        );
        // หมายเหตุ: score ของ r1 อาจสูงเพราะ pool เดี่ยว percentile เป็น 1 เสมอ —
        // สิ่งที่สำคัญคือลำดับ/ตัวที่เลือกเปลี่ยนตาม risk
    }

    /// หนึ่ง pick ต่อ symbol, ไม่เกิน 3, และ shortlist ตัดตาม n
    #[test]
    fn at_most_one_pick_per_symbol_and_max_three() {
        let inp = input(base_today());
        let out = rank_universe(&inp, &profile(50_000.0, 2), now());
        assert_eq!(out.picks.len(), 3, "6 symbols ได้ 3 picks");
        let mut syms: Vec<&str> = out.picks.iter().map(|p| p.idea.symbol.as_str()).collect();
        let n_unique = syms.iter().collect::<std::collections::BTreeSet<_>>().len();
        assert_eq!(n_unique, syms.len(), "ไม่ซ้ำ symbol");
        assert_eq!(out.picks[0].rank, 1);
        assert!(out.picks[0].idea.recommended);
        assert!(out.picks[1..].iter().all(|p| !p.idea.recommended));
        syms.sort_unstable();
        assert_eq!(
            syms,
            vec!["GM", "INTC", "KO"],
            "top-3 ของ universe นี้ (ไม่มี extras)"
        );

        let sl = shortlist(&inp, &profile(50_000.0, 2), now(), 5);
        assert_eq!(sl.len(), 5);
        assert!(
            sl.iter()
                .all(|s| ["F", "GM", "INTC", "PFE", "T", "KO"].contains(&s.as_str()))
        );
        assert_eq!(
            sl.iter().collect::<std::collections::BTreeSet<_>>().len(),
            5
        );
    }

    /// หลาย snapshot ต่อ occ: event_time ล่าสุดชนะ
    #[test]
    fn latest_snapshot_wins_per_occ() {
        let (e, t) = (exp_dec24(), at(21, 0));
        let today = vec![
            stock("F", 11.0, t),
            put("F", 10.0, e, at(20, 30), 0.08, 0.09, 1200.0),
            put("F", 10.0, e, t, 0.11, 0.13, 1200.0),
            stock("GM", 42.0, t),
            put("GM", 38.0, e, t, 0.45, 0.49, 3000.0),
        ];
        let out = rank_universe(&input(today), &profile(50_000.0, 2), now());
        let f = out
            .picks
            .iter()
            .find(|p| p.idea.symbol == "F")
            .expect("F อยู่ใน 2 picks");
        assert_eq!(f.idea.credit, 11.0, "bid ล่าสุด 0.11 × 100");
        assert_eq!(f.idea.premium, 0.11);
    }

    /// earnings ก่อน expiry ลดคะแนน (event_penalty) เทียบตัวเหมือนกันแต่ไม่มี earnings
    #[test]
    fn earnings_before_expiry_lowers_score() {
        let (e, t) = (exp_dec24(), at(21, 0));
        let today = vec![
            stock("UU1", 40.0, t),
            put("UU1", 36.0, e, t, 0.33, 0.37, 2000.0),
            stock("UU2", 40.0, t),
            put("UU2", 36.0, e, t, 0.33, 0.37, 2000.0),
        ];
        let mut inp = input(today);
        for sym in ["UU1", "UU2"] {
            inp.extras.insert(
                sym.into(),
                SymbolExtras {
                    realized_30d: Some(0.25),
                    earnings_date: None,
                },
            );
        }
        inp.extras.insert(
            "UU2".into(),
            SymbolExtras {
                realized_30d: Some(0.25),
                earnings_date: Some(NaiveDate::from_ymd_opt(2026, 12, 23).unwrap()),
            },
        );
        let out = rank_universe(&inp, &profile(50_000.0, 2), now());
        let (u1, u2) = (&out.picks[0], &out.picks[1]);
        let (no_earn, earn) = if u1.idea.symbol == "UU1" {
            (u1, u2)
        } else {
            (u2, u1)
        };
        assert!(
            earn.score < no_earn.score,
            "{} < {}",
            earn.score,
            no_earn.score
        );
        assert_eq!(no_earn.rank, 1);
        assert_eq!(earn.parts.event_penalty, 0.6);
        assert_eq!(no_earn.parts.event_penalty, 1.0);
        assert!(
            earn.cautions_th.iter().any(|c| c.contains("ผลประกอบการ")),
            "cautions: {:?}",
            earn.cautions_th
        );
        assert_eq!(earn.earnings_date.as_deref(), Some("2026-12-23"));
    }

    /// OI ประกอบ (previous-day OI + put wall) ต้องไม่แตะคะแนน/ลำดับ
    #[test]
    fn oi_context_and_previous_day_oi_do_not_change_scores() {
        let base = input(base_today());
        let a = rank_universe(&base, &profile(50_000.0, 2), now());

        // previous-day OI ต่างจากวันนี้ — คะแนนและลำดับเหมือนเดิม
        let mut with_prev = input(base_today());
        with_prev.previous = base_today()
            .into_iter()
            .filter(|o| matches!(o.kind, ObsKind::OptionQuote { .. }))
            .map(|mut o| {
                o.event_time = prev_day(21);
                o.received_time = prev_day(21);
                o.session_date = prev_day(21).date_naive();
                if let ObsKind::OptionQuote { oi, .. } = &mut o.kind {
                    *oi = oi.map(|v| v * 3.0);
                }
                o
            })
            .collect();
        let b = rank_universe(&with_prev, &profile(50_000.0, 2), now());
        let sig = |o: &UniverseIdeas| {
            o.picks
                .iter()
                .map(|p| (p.idea.symbol.clone(), p.score))
                .collect::<Vec<_>>()
        };
        assert_eq!(sig(&a), sig(&b), "คะแนน/ลำดับเหมือนเดิมทุกตัว");
        assert!(a.picks[0].oi.oi_change_pct.is_none(), "ไม่มี previous ⇒ None");
        let change = b.picks[0].oi.oi_change_pct.expect("มี previous ⇒ Some");
        assert!(change < 0.0, "OI เมื่อวานสูงกว่า 3 เท่า ⇒ ลดลง: {change}");

        // put wall เปลี่ยนได้โดยคะแนนไม่ขยับ: เพิ่ม put deep-ITM (ถูก gate ตัดออก)
        let mut today = base_today();
        today.push(put(
            "INTC",
            15.0,
            exp_dec24(),
            at(21, 0),
            8.80,
            8.90,
            99_999.0,
        ));
        let c = rank_universe(&input(today), &profile(50_000.0, 2), now());
        assert_eq!(sig(&a), sig(&c), "wall เพิ่มมาแล้วคะแนนไม่ขยับ");
        assert_eq!(a.picks[0].oi.put_wall_strike, Some(21.0));
        assert_eq!(
            c.picks[0].oi.put_wall_strike,
            Some(15.0),
            "wall ย้ายไป strike ที่ OI ใหญ่สุด"
        );
        assert_eq!(c.picks[0].oi.put_wall_oi, Some(99_999.0));
        assert!(c.picks[0].oi.note_th.contains("ยังไม่ได้พิสูจน์"));
    }

    /// ไม่มี extras: vrp เป็น None แต่ยังต้องมีคะแนน (renormalise ไม่ลงโทษ)
    #[test]
    fn missing_extras_gives_vrp_none() {
        let out = rank_universe(&input(base_today()), &profile(50_000.0, 2), now());
        assert!(!out.picks.is_empty());
        for p in &out.picks {
            assert!(p.parts.vrp.is_none());
            assert!(p.score > 0.0);
            assert!(p.iv_pct.is_some(), "ATM iv มาจาก quote เอง ไม่เกี่ยว extras");
            assert!(p.realized_30d_pct.is_none());
        }
        assert!(out.market_note_th.contains("ยังไม่มีข้อมูล"));
    }

    /// serde JSON round-trip + snake_case keys
    #[test]
    fn serde_round_trip_snake_case() {
        let out = rank_universe(&input(base_today()), &profile(900.0, 2), now());
        let value = serde_json::to_value(&out).expect("serialises");
        for key in [
            "session_date",
            "as_of",
            "source",
            "scanned_symbols",
            "candidate_symbols",
            "mode_counts",
            "market_note_th",
            "picks",
            "near_misses",
            "budget_short",
            "watch_cash",
            "agent_say_th",
            "computed_at",
        ] {
            assert!(value.get(key).is_some(), "UniverseIdeas.{key} missing");
        }
        assert!(value["budget_short"]["min_cash_usd"].is_number());
        let rich = rank_universe(&input(base_today()), &profile(50_000.0, 2), now());
        let value = serde_json::to_value(&rich).unwrap();
        let pick = &value["picks"][0];
        for key in [
            "rank",
            "idea",
            "score",
            "parts",
            "reasons_th",
            "cautions_th",
            "iv_pct",
            "realized_30d_pct",
            "earnings_date",
            "oi",
        ] {
            assert!(pick.get(key).is_some(), "UniversePick.{key} missing");
        }
        for key in [
            "vrp",
            "premium",
            "fit",
            "cushion",
            "liquidity",
            "event_penalty",
        ] {
            assert!(pick["parts"].get(key).is_some(), "ScoreParts.{key} missing");
        }
        for key in ["put_wall_strike", "put_wall_oi", "oi_change_pct", "note_th"] {
            assert!(pick["oi"].get(key).is_some(), "OiContext.{key} missing");
        }
        assert_eq!(value["as_of"], "2026-11-25T21:00:00+00:00");
        let back: UniverseIdeas = serde_json::from_value(value).expect("deserialises");
        assert_eq!(back, rich);
    }

    /// symbol ที่ไม่มี spot (หรือไม่มี option quote) ถูกข้าม
    #[test]
    fn symbol_without_spot_is_skipped() {
        let (e, t) = (exp_dec24(), at(21, 0));
        let mut today = base_today();
        today.push(put("ZAP", 90.0, e, t, 0.50, 0.55, 5000.0)); // ไม่มีราคาหุ้น
        today.push(stock("NOSTRIKE", 100.0, t)); // ไม่มี option quote
        let out = rank_universe(&input(today), &profile(50_000.0, 2), now());
        assert_eq!(out.scanned_symbols, 6, "นับเฉพาะที่มี spot + quote");
        assert!(out.picks.iter().all(|p| p.idea.symbol != "ZAP"));
        assert!(out.picks.iter().all(|p| p.idea.symbol != "NOSTRIKE"));
    }

    /// ตัวท็อปของ symbol เกินงบ ไม่ตัดทั้ง symbol — ต้องได้ strike ที่จ่ายไหวแทน
    /// (งบเล็กเป็นเคสหลักของ Easy Mode) — risk 3 เพื่อให้ put ตื้นของ MIX
    /// (p_itm ~28% ≤ cap 30) ผ่าน gate และเป็นตัวท็อปด้วยน้ำหนัก premium
    /// (spot 34 แทน 32: cap 30 ของ risk3 แคบลงจาก 35 ดังนั้นเลื่อน spot
    /// ให้พ้นเพดานเดิมที่ ~31%)
    #[test]
    fn symbol_falls_back_to_affordable_strike_when_best_is_too_rich() {
        let (e, t) = (exp_dec24(), at(21, 0));
        let today = vec![
            stock("MIX", 34.0, t),
            // ตัวท็อปของ MIX: premium แพงกว่า + spread แคบกว่า ⇒ คะแนนสูงสุด
            // แต่กันเงิน $3,100 เกินงบ $3,000
            put("MIX", 31.0, e, t, 0.36, 0.40, 4000.0),
            // strike ต่ำกว่า: คะแนนรองลงมาแต่กันเงินแค่ $2,900
            put("MIX", 29.0, e, t, 0.13, 0.165, 4000.0),
        ];
        let inp = input(today);

        let tight = rank_universe(&inp, &profile(3_000.0, 3), now());
        assert_eq!(tight.picks.len(), 1, "MIX ยังต้องอยู่ใน picks");
        assert_eq!(tight.picks[0].idea.symbol, "MIX");
        assert_eq!(tight.picks[0].idea.strike, 29.0, "ใช้ strike ที่จ่ายไหว");
        assert_eq!(tight.picks[0].idea.cost, 2_900.0);
        assert!(!tight.picks[0].idea.cash_short);
        assert!(tight.budget_short.is_none());

        // งบใหญ่พอต้องได้ตัวท็อปจริง (พิสูจน์ว่า 31 คือ higher-score จริง)
        let rich = rank_universe(&inp, &profile(10_000.0, 3), now());
        assert_eq!(rich.picks[0].idea.strike, 31.0);
        assert!(rich.picks[0].score > tight.picks[0].score);
    }

    // ---------- spread gate (percent OR absolute gap) ----------

    /// EnrichedRow เดี่ยวจาก bid/ask เพื่อเทส spread_ok ตรง ๆ
    fn row_with_spread(bid: f64, ask: f64) -> EnrichedRow {
        let rows = vec![QuoteRow {
            symbol: "TEST261224P00100000".into(),
            underlying: "TEST".into(),
            expiry: exp_dec24(),
            option_type: OptType::Put,
            strike: 100.0,
            price: (ask + bid) / 2.0,
            bid,
            ask,
            volume: 1000.0,
            oi: 1000.0,
            change: 0.0,
            change_ratio: 0.0,
            src: "TEST".into(),
        }];
        enrich_rows_raw(rows, 100.0, now()).remove(0)
    }

    #[test]
    fn fmt_price_keeps_cents_and_drops_zero_cents() {
        assert_eq!(fmt_price(21.5), "21.50");
        assert_eq!(fmt_price(28.46), "28.46");
        assert_eq!(fmt_price(26.0), "26");
        assert_eq!(fmt_price(1250.0), "1,250");
    }

    #[test]
    fn spread_ok_passes_wide_percent_with_tiny_absolute_gap() {
        // PFE จริงจาก recorder: 0.13/0.18 = 32% แต่ต่างกันแค่ $0.05/หุ้น
        let row = row_with_spread(0.13, 0.18);
        assert!(
            row.spread_pct.is_some_and(|sp| sp > MAX_SPREAD_PCT),
            "fixture ต้องกว้างเป็น % ก่อน"
        );
        assert!(spread_ok(&row), "32% แต่ 5 เซนต์ ต้องผ่าน");
    }

    #[test]
    fn spread_ok_fails_wide_percent_with_big_absolute_gap() {
        let row = row_with_spread(0.30, 0.40);
        let sp = row.spread_pct.expect("มีทั้งสองฝั่ง");
        assert!(sp > MAX_SPREAD_PCT && sp < 30.5, "≈28.6%: {sp}");
        assert!(!spread_ok(&row), "28.6% และ $0.10 ต้องไม่ผ่าน");
    }

    #[test]
    fn spread_ok_passes_narrow_percent_even_with_big_gap() {
        let row = row_with_spread(1.00, 1.20); // 18% ≤ 25 แม้ช่องว่าง $0.20
        assert!(spread_ok(&row));
    }

    #[test]
    fn spread_ok_missing_spread_still_passes() {
        let row = row_with_spread(0.10, 0.0); // ขาด ask ⇒ spread_pct None
        assert!(row.spread_pct.is_none());
        assert!(spread_ok(&row));
    }

    /// Regression ของ session 2026-11-26: PFE spot 28.46, K=26 (2026-12-24,
    /// DTE 28) bid .13 ask .18, IV ≈ 31% — เดิมตายที่ spread 32% > 25% ทั้ง
    /// ที่ช่องว่างแค่ $5 ต่อสัญญา ตอนนี้งบ $3,000 / risk 1 ต้องได้ pick
    #[test]
    fn pfe_k26_regression_becomes_pick_at_3000_risk1() {
        let now = Utc.with_ymd_and_hms(2026, 11, 26, 22, 0, 0).unwrap();
        let t = Utc.with_ymd_and_hms(2026, 11, 26, 21, 0, 0).unwrap();
        let today = vec![
            stock("PFE", 28.46, t),
            put("PFE", 26.0, exp_dec24(), t, 0.13, 0.18, 5_000.0),
        ];
        let out = rank_universe(&input(today), &profile(3_000.0, 1), now);
        assert!(out.budget_short.is_none(), "ต้องมีตัวที่จ่ายไหวแล้ว");
        assert_eq!(out.picks.len(), 1);
        let pick = &out.picks[0];
        assert_eq!(pick.idea.symbol, "PFE");
        assert_eq!(pick.idea.strike, 26.0);
        assert!(pick.idea.cost <= 3_000.0, "cost = {}", pick.idea.cost);
        assert_eq!(pick.idea.cost, 2_600.0, "26 × 100");
        assert!(!pick.idea.cash_short);
    }

    // ---------- watch_cash ----------

    /// สาขาข้อความครบ: ไม่มีข้อมูล / ผ่านเกณฑ์ / ไม่ผ่านเกณฑ์แต่มีตัวถูก /
    /// มี spot แต่ไม่มีพุตใน DTE window + ธง affordable — risk 2 เพราะ
    /// ทุก branch ของ fixture นี้ตัดสินที่ gate p_itm/spread ไม่ใช่หน้าต่าง dte
    #[test]
    fn watch_cash_text_branches_and_affordability() {
        let (t, near) = (
            at(21, 0),
            NaiveDate::from_ymd_opt(2026, 11, 28).unwrap(), // dte 3 นอกหน้าต่าง
        );
        let mut today = base_today(); // F ผ่าน gate ทั้งหมด (strike 10)
        today.push(stock("GATE", 50.0, t)); // ATM put ไม่ผ่าน p_itm แต่มี bid
        today.push(put("GATE", 50.0, exp_dec24(), t, 0.92, 0.98, 4_000.0));
        today.push(stock("NEAR", 50.0, t)); // พุตใกล้เกินไป ⇒ ไม่มีตัวใน window
        today.push(put("NEAR", 45.0, near, t, 0.50, 0.55, 4_000.0));

        let watch = ["f", "GATE", "NEAR", "NVDA"];
        let out = rank_universe_with_watch(
            &input(today.clone()),
            &profile(5_000.0, 2),
            &watch.map(String::from),
            now(),
        );
        let by: HashMap<&str, &WatchCash> = out
            .watch_cash
            .iter()
            .map(|w| (w.symbol.as_str(), w))
            .collect();
        assert_eq!(out.watch_cash.len(), 4);

        let f = by["F"];
        assert_eq!(f.spot, Some(11.0));
        assert_eq!(f.min_cash_usd, Some(1_000.0), "F strike 10 × 100");
        assert!(f.affordable, "1,000 ≤ งบ 5,000");
        assert_eq!(
            f.text_th,
            "F ราคา $11 · ขาย 1 สัญญาต้องกันเงินราว $1,000 (ยอมซื้อ 100 หุ้นที่ $10)"
        );

        let gate = by["GATE"];
        assert_eq!(gate.min_cash_usd, None, "ATM put ไม่ผ่าน p_itm gate");
        assert_eq!(gate.any_cash_usd, Some(5_000.0), "ตัวถูกสุดที่จับได้");
        assert!(!gate.affordable);
        assert_eq!(
            gate.text_th,
            "GATE ราคา $50 · ยังไม่มีสัญญาที่ปลอดภัยพอในรอบนี้ — ตัวที่ถูกสุดต้องกันเงิน $5,000"
        );

        let near = by["NEAR"];
        assert_eq!(near.spot, Some(50.0));
        assert_eq!(near.min_cash_usd, None);
        assert_eq!(near.any_cash_usd, None, "พุตเดียวอยู่นอก DTE window");
        assert_eq!(near.text_th, "NEAR ราคา $50 · ยังไม่มีสัญญาที่ปลอดภัยพอในรอบนี้");

        let nvda = by["NVDA"];
        assert_eq!(nvda.spot, None);
        assert_eq!(nvda.min_cash_usd, None);
        assert_eq!(nvda.any_cash_usd, None);
        assert_eq!(nvda.text_th, "ยังไม่มีข้อมูลของ NVDA ในรอบนี้");

        // KO (base_today) ผ่านเกณฑ์ที่ $5,500 — เกินงบ $5,000 ⇒ ไม่ affordable
        let rich = rank_universe_with_watch(
            &input(today.clone()),
            &profile(5_000.0, 2),
            &["KO".to_string()],
            now(),
        );
        let ko = &rich.watch_cash[0];
        assert_eq!(ko.min_cash_usd, Some(5_500.0));
        assert!(!ko.affordable, "5,500 > งบ 5,000");
    }

    /// uppercase + dedup (เก็บตัวแรก) + คงลำดับ + cap 10 หลัง dedup
    #[test]
    fn watch_cash_uppercases_dedups_and_caps_at_ten() {
        let mut watch: Vec<String> = ["f", "F", " ko ", "GM"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        for i in 0..12 {
            watch.push(format!("S{i:02}")); // symbol ต่างกัน 12 ตัว ไม่มีในข้อมูล
        }
        let out =
            rank_universe_with_watch(&input(base_today()), &profile(50_000.0, 2), &watch, now());
        assert_eq!(out.watch_cash.len(), 10, "dedup เหลือ 4 + 12 ใหม่ ⇒ cap 10");
        assert_eq!(out.watch_cash[0].symbol, "F", "uppercase + เก็บตัวแรก");
        assert_eq!(out.watch_cash[1].symbol, "KO", "trim + uppercase");
        assert_eq!(out.watch_cash[2].symbol, "GM");
        assert_eq!(out.watch_cash[3].symbol, "S00", "ตัวถัดไปตามลำดับที่ส่งมา");
        assert_eq!(out.watch_cash[9].symbol, "S06", "ตัดที่ 10 ตัวแรก");
    }

    /// rank_universe ใช้ profile.stocks เป็น watch list อัตโนมัติ
    #[test]
    fn rank_universe_defaults_watch_to_profile_stocks() {
        let mut prof = profile(50_000.0, 2);
        prof.stocks = vec![
            crate::easy::EasyStock {
                symbol: "f".into(),
                stance: Stance::Want,
            },
            crate::easy::EasyStock {
                symbol: "NOPE".into(),
                stance: Stance::Avoid,
            },
        ];
        let out = rank_universe(&input(base_today()), &prof, now());
        let syms: Vec<&str> = out.watch_cash.iter().map(|w| w.symbol.as_str()).collect();
        assert_eq!(syms, ["F", "NOPE"], "uppercase ตามลำดับ profile");
    }

    // ---------- mode_counts / near_misses ----------

    /// mode_counts บอกจำนวนไอเดียของคืนนี้ต่อ risk 1–3 จากข้อมูลชุดเดียวกัน:
    /// AAA (dte 12) เข้าได้เฉพาะ risk3 ที่หน้าต่างเริ่ม 10 วัน, BBB (p_itm ~21%)
    /// เข้าเมื่อ cap 25/30, CCC เข้าทุกโหมด — และโหมดที่ใช้อยู่ต้องตรงกับ picks
    #[test]
    fn mode_counts_report_each_risk_from_same_data() {
        let inp = input(risk_today());
        let out = rank_universe(&inp, &profile(50_000.0, 1), now());
        assert_eq!(
            out.mode_counts.iter().map(|m| m.risk).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        let picks_of = |out: &UniverseIdeas, r: u8| {
            out.mode_counts.iter().find(|m| m.risk == r).unwrap().picks
        };
        assert_eq!(picks_of(&out, 1), 1, "risk1: CCC เท่านั้น");
        assert_eq!(picks_of(&out, 2), 2, "risk2: BBB+CCC");
        assert_eq!(
            picks_of(&out, 3),
            3,
            "risk3: ครบ (AAA เข้าเมื่อหน้าต่างเริ่ม 10 วัน)"
        );
        assert_eq!(picks_of(&out, 1), out.picks.len() as u32, "โหมดปัจจุบัน = จอ");

        let out3 = rank_universe(&inp, &profile(50_000.0, 3), now());
        assert_eq!(out3.mode_counts[2].picks, out3.picks.len() as u32);
        assert_eq!(out3.mode_counts[2].candidate_symbols, 3);
    }

    /// ไม่มี picks เพราะตัวเดียวที่จ่ายไหวตกแค่เกณฑ์ความคล่องการซื้อขาย —
    /// near_miss ต้องพาตัวเลขจริง (เงินที่ต้องกัน, เงินที่ได้, ช่องว่างราคา)
    /// ไปให้หน้าบ้านอธิบายเป็นประโยค
    #[test]
    fn near_miss_when_only_affordable_row_fails_only_liquidity() {
        let now = Utc.with_ymd_and_hms(2026, 11, 26, 22, 0, 0).unwrap();
        let t = Utc.with_ymd_and_hms(2026, 11, 26, 21, 0, 0).unwrap();
        let today = vec![
            stock("PFE", 28.46, t),
            // bid .06 ask .18: ช่องว่าง $0.12/หุ้น > $0.05 และ ~100% > 25% ⇒ ตก liquidity
            put("PFE", 26.0, exp_dec24(), t, 0.06, 0.18, 5_000.0),
        ];
        let out = rank_universe(&input(today), &profile(3_000.0, 1), now);
        assert!(out.picks.is_empty());
        assert!(out.budget_short.is_none());
        assert_eq!(out.near_misses.len(), 1);
        let nm = &out.near_misses[0];
        assert_eq!(nm.symbol, "PFE");
        assert_eq!(nm.strike, 26.0);
        assert_eq!(nm.cash_needed_usd, 2_600.0, "26 × 100 ≤ งบ 3,000");
        assert_eq!(nm.credit_usd, 6.0, "bid 0.06 × 100 ยัง ≥ ขั้นต่ำ $5");
        assert!((nm.gap_usd - 0.12).abs() < 1e-9, "ask − bid ต่อหุ้น");
        assert_eq!(nm.bid_usd, 0.06);
        assert_eq!(nm.ask_usd, 0.18);
        assert!(nm.spread_pct.is_some_and(|sp| sp > 25.0));
        assert_eq!(nm.expiry, "2026-12-24");
        assert_eq!(nm.checks.cap_pct, 15.0, "risk 1");
        assert!(
            nm.checks.days_ok && nm.checks.has_buyer && nm.checks.credit_ok && nm.checks.assign_ok
        );
        assert!(!nm.checks.liquidity_ok, "ตกแค่ liquidity ข้อเดียว");
        assert!(nm.assign_pct <= 15.0, "assign ผ่าน: {}", nm.assign_pct);
    }

    /// เรียงตามจำนวนเกณฑ์ที่พลาด แล้วตามโอกาสต้องซื้อหุ้นใกล้เพดานของโหมด —
    /// ไม่เกิน 3 ตัว, ว่างเมื่อมี picks, และว่างเมื่อไม่มีพุตที่จ่ายไหวเลย
    /// (budget_short อธิบายแทน)
    #[test]
    fn near_misses_rank_by_fewest_failures_then_cap_distance() {
        let (t, near) = (at(21, 0), NaiveDate::from_ymd_opt(2026, 11, 28).unwrap());
        let today = vec![
            stock("WIDE", 28.46, t),
            put("WIDE", 26.0, exp_dec24(), t, 0.06, 0.18, 5_000.0), // liquidity (1)
            stock("DEEP", 50.0, t),
            put("DEEP", 10.0, exp_dec24(), t, 0.02, 0.03, 5_000.0), // credit (1, p_itm ไกลเพดาน)
            stock("ATM", 50.0, t),
            put("ATM", 50.0, near, t, 0.92, 0.98, 5_000.0), // days + assign (2)
            stock("ZERO", 28.46, t),
            put("ZERO", 26.0, near, t, 0.02, 0.18, 5_000.0), // 4 ข้อ และถูกตัดที่ 3 ตัว
        ];
        let out = rank_universe(&input(today), &profile(6_000.0, 1), now());
        assert!(out.picks.is_empty());
        assert_eq!(
            out.near_misses
                .iter()
                .map(|n| n.symbol.as_str())
                .collect::<Vec<_>>(),
            vec!["WIDE", "DEEP", "ATM"],
            "พลาดน้อยก่อน แล้วใกล้เพดาน cap ก่อน; ZERO ตัดที่ cap 3 ตัว"
        );
        assert!(out.near_misses.iter().all(|n| n.cash_needed_usd <= 6_000.0));

        // มี picks ⇒ near_misses ว่าง
        let rich = rank_universe(&input(base_today()), &profile(50_000.0, 2), now());
        assert!(!rich.picks.is_empty());
        assert!(rich.near_misses.is_empty());

        // ไม่มีพุตที่จ่ายไหวเลย ⇒ budget_short อธิบายแทน
        let broke = rank_universe(&input(base_today()), &profile(900.0, 2), now());
        assert!(broke.picks.is_empty());
        assert!(broke.budget_short.is_some());
        assert!(broke.near_misses.is_empty());
    }
}
