//! Grounding check: every number the model shows a beginner must exist in
//! this run's tool results (or the user's own messages).
//!
//! [`extract_numbers`] finds number tokens in prose — dollars (`$183`,
//! `183 ดอลลาร์`, `$17.5k`), percents (`18%`, `18.5 %`, `18 เปอร์เซ็นต์`)
//! and plain counts — while ignoring things that are not data claims:
//! dates, years, clock times, list markers, ids like OCC symbols, small
//! ordinal counts and the `100 หุ้น` share multiplier. [`collect_facts`]
//! turns the run's tool-result strings and user texts into a pool of raw
//! f64 facts (JSON leaves plus scanned strings — no derived unit copies),
//! and [`ungrounded`] returns the answer tokens that no fact supports:
//! each token kind reads facts in its own unit space (a percent token
//! compares in percent space, so a fraction fact of 0.214 can never
//! ground "0.21%") within small absolute/relative/rounding tolerances.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum NumKind {
    Money,
    Percent,
    Plain,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NumToken {
    pub raw: String,
    pub value: f64,
    pub kind: NumKind,
}

/// Thai unit suffixes, longest first so the raw span stays faithful
/// (เปอร์เซ็นต์ contains เปอร์เซ็น).
const PERCENT_WORDS: [&str; 4] = ["เปอร์เซ็นต์", "เปอร์เซนต์", "เปอร์เซ็น", "เปอร์เซน"];
const DOLLAR_WORD: &str = "ดอลลาร์";
const SHARES_WORD: &str = "หุ้น";
/// Thai month names (short + long): a number directly followed by one is a
/// date, not a data claim.
const THAI_MONTHS: [&str; 24] = [
    "ม.ค.",
    "มกราคม",
    "ก.พ.",
    "กุมภาพันธ์",
    "มี.ค.",
    "มีนาคม",
    "เม.ย.",
    "เมษายน",
    "พ.ค.",
    "พฤษภาคม",
    "มิ.ย.",
    "มิถุนายน",
    "ก.ค.",
    "กรกฎาคม",
    "ส.ค.",
    "สิงหาคม",
    "ก.ย.",
    "กันยายน",
    "ต.ค.",
    "ตุลาคม",
    "พ.ย.",
    "พฤศจิกายน",
    "ธ.ค.",
    "ธันวาคม",
];

fn starts_with_at(chars: &[char], pos: usize, word: &str) -> bool {
    let word: Vec<char> = word.chars().collect();
    if pos + word.len() > chars.len() {
        return false;
    }
    chars[pos..pos + word.len()] == word[..]
}

/// Digits with comma thousand-groups and an optional decimal part.
/// A comma joins the number only when three digits follow, so sentence
/// commas ("1, 2 และ 3") never glue numbers together. Returns the value
/// and the index just past the consumed span.
fn parse_core(chars: &[char], start: usize) -> (f64, usize) {
    let len = chars.len();
    let mut end = start;
    while end < len && chars[end].is_ascii_digit() {
        end += 1;
    }
    while end + 4 <= len
        && chars[end] == ','
        && chars[end + 1].is_ascii_digit()
        && chars[end + 2].is_ascii_digit()
        && chars[end + 3].is_ascii_digit()
    {
        end += 4;
        while end < len && chars[end].is_ascii_digit() {
            end += 1;
        }
    }
    if end + 1 < len && chars[end] == '.' && chars[end + 1].is_ascii_digit() {
        end += 1;
        while end < len && chars[end].is_ascii_digit() {
            end += 1;
        }
    }
    let raw: String = chars[start..end].iter().collect();
    // digit runs always parse; the guard keeps a malformed edge (overflow
    // → inf, never Err) from becoming a panic
    let value = raw.replace(',', "").parse().unwrap_or(f64::NAN);
    (value, end)
}

/// `12/18`, `2026-12-18`, `10:30` — spans to consume wholesale.
fn date_time_span(chars: &[char], start: usize) -> Option<usize> {
    let len = chars.len();
    // end index if chars[from..] starts with between min..=max digits
    let digits_at = |from: usize, min: usize, max: usize| -> Option<usize> {
        let mut k = from;
        while k < len && k - from < max && chars[k].is_ascii_digit() {
            k += 1;
        }
        (k - from >= min).then_some(k)
    };
    if let Some(a) = digits_at(start, 4, 4)
        && chars.get(a) == Some(&'-')
        && let Some(b) = digits_at(a + 1, 1, 2)
        && chars.get(b) == Some(&'-')
        && let Some(c) = digits_at(b + 1, 1, 2)
    {
        return Some(c); // 2026-12-18
    }
    if let Some(a) = digits_at(start, 1, 2)
        && chars.get(a) == Some(&':')
        && let Some(b) = digits_at(a + 1, 2, 2)
    {
        if chars.get(b) == Some(&':')
            && let Some(c) = digits_at(b + 1, 2, 2)
        {
            return Some(c); // 10:30:45
        }
        return Some(b); // 10:30
    }
    if let Some(a) = digits_at(start, 1, 2)
        && chars.get(a) == Some(&'/')
        && let Some(b) = digits_at(a + 1, 1, 2)
    {
        if chars.get(b) == Some(&'/')
            && let Some(c) = digits_at(b + 1, 2, 4)
        {
            return Some(c); // 12/18/2026
        }
        return Some(b); // 12/18
    }
    None
}

/// `1)`, `1.`, `- 2.` — a bare integer at a line start (optionally behind a
/// "-" bullet) followed by `)` or `.`.
fn is_list_marker(chars: &[char], core_start: usize, core_end: usize) -> bool {
    let bare_integer = chars[core_start..core_end]
        .iter()
        .all(|c| c.is_ascii_digit());
    if !bare_integer {
        return false;
    }
    if !matches!(chars.get(core_end), Some(')') | Some('.')) {
        return false;
    }
    let mut j = core_start;
    while j > 0 && matches!(chars[j - 1], ' ' | '\t') {
        j -= 1;
    }
    if j > 0 && chars[j - 1] == '-' {
        j -= 1;
        while j > 0 && matches!(chars[j - 1], ' ' | '\t') {
            j -= 1;
        }
    }
    j == 0 || chars[j - 1] == '\n'
}

enum Suffix {
    Kept(NumKind, usize),
    Ignored,
}

/// Unit suffixes and the context ignores that only look rightwards of the
/// number: `k` ×1000, `%` (attached or one space away), ดอลลาร์,
/// เปอร์เซ็นต์, the `100 หุ้น` multiplier, Thai month dates, and digits
/// glued to letters (identifier payloads like `175C`).
fn classify_suffix(chars: &[char], core_end: usize, value: f64, base: NumKind) -> Suffix {
    let len = chars.len();
    if matches!(chars.get(core_end), Some('k') | Some('K')) {
        return Suffix::Kept(NumKind::Money, core_end + 1);
    }
    if chars.get(core_end) == Some(&'%') {
        return Suffix::Kept(NumKind::Percent, core_end + 1);
    }
    // unit words may sit one space away ("18.5 %", "18 เปอร์เซ็นต์")
    let unit_pos = if chars.get(core_end) == Some(&' ') && core_end + 1 < len {
        core_end + 1
    } else {
        core_end
    };
    if chars.get(unit_pos) == Some(&'%') {
        return Suffix::Kept(NumKind::Percent, unit_pos + 1);
    }
    for word in PERCENT_WORDS {
        if starts_with_at(chars, unit_pos, word) {
            return Suffix::Kept(NumKind::Percent, unit_pos + word.chars().count());
        }
    }
    if starts_with_at(chars, unit_pos, DOLLAR_WORD) {
        return Suffix::Kept(NumKind::Money, unit_pos + DOLLAR_WORD.chars().count());
    }
    if value == 100.0 && starts_with_at(chars, unit_pos, SHARES_WORD) {
        return Suffix::Ignored;
    }
    if THAI_MONTHS
        .iter()
        .any(|month| starts_with_at(chars, unit_pos, month))
    {
        return Suffix::Ignored;
    }
    if chars.get(core_end).is_some_and(|c| c.is_alphanumeric()) {
        return Suffix::Ignored;
    }
    Suffix::Kept(base, core_end)
}

pub fn extract_numbers(text: &str) -> Vec<NumToken> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < len {
        if chars[i] == '$' && matches!(chars.get(i + 1), Some(c) if c.is_ascii_digit()) {
            let (value, core_end) = parse_core(&chars, i + 1);
            // k multiplier and unit words extend the raw span; the money
            // prefix already fixes the kind, so no date/list/plain ignores
            // apply here.
            match classify_suffix(&chars, core_end, value, NumKind::Money) {
                Suffix::Kept(kind, end) => {
                    // ×1000 only when the k suffix was actually consumed
                    let value =
                        if end > core_end && matches!(chars.get(core_end), Some('k') | Some('K')) {
                            value * 1000.0
                        } else {
                            value
                        };
                    tokens.push(NumToken {
                        raw: chars[i..end].iter().collect(),
                        value,
                        kind,
                    });
                    i = end;
                }
                Suffix::Ignored => i = core_end,
            }
            continue;
        }
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        // digits glued to letters/digits belong to an identifier (OCC
        // symbols, tickers, "v2") — not a data claim.
        if i > 0 && chars[i - 1].is_alphanumeric() {
            i += 1;
            continue;
        }
        if let Some(after) = date_time_span(&chars, i) {
            i = after;
            continue;
        }
        let (value, core_end) = parse_core(&chars, i);
        if is_list_marker(&chars, i, core_end) {
            i = core_end;
            continue;
        }
        match classify_suffix(&chars, core_end, value, NumKind::Plain) {
            Suffix::Kept(kind, end) => {
                let mut value = value;
                if kind == NumKind::Money
                    && end > core_end
                    && matches!(chars.get(core_end), Some('k') | Some('K'))
                {
                    value *= 1000.0;
                }
                let bare_integer = chars[i..core_end].iter().all(|c| c.is_ascii_digit());
                let ignored = bare_integer && {
                    let four_digit_year = core_end - i == 4 && (1900.0..=2100.0).contains(&value);
                    let small_count = (0.0..=12.0).contains(&value);
                    kind == NumKind::Plain && (four_digit_year || small_count)
                };
                if !ignored {
                    tokens.push(NumToken {
                        raw: chars[i..end].iter().collect(),
                        value,
                        kind,
                    });
                }
                i = end;
            }
            Suffix::Ignored => i = core_end,
        }
    }
    tokens
}

/// Numbers from tool-result strings and user texts. JSON sources
/// contribute every numeric leaf plus numbers scanned out of string
/// leaves; anything that is not JSON is scanned as text. The pool keeps
/// raw values only — unit readings (fraction→percent, hundred-scale→
/// per-share) are applied per token kind inside matching, because
/// pre-multiplied copies made raw fractions reachable for percent tokens.
/// Facts are deliberately permissive — no ignore rules — so the pool is
/// generous: an extra fact can only keep an answer token out of the
/// report.
pub fn collect_facts(sources: &[&str]) -> Vec<f64> {
    let mut facts = Vec::new();
    for source in sources {
        match serde_json::from_str::<Value>(source) {
            Ok(value) => collect_json(&value, &mut facts),
            Err(_) => scan_text(source, &mut facts),
        }
    }
    facts
}

fn collect_json(value: &Value, facts: &mut Vec<f64>) {
    match value {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                facts.push(f);
            }
        }
        Value::String(s) => scan_text(s, facts),
        Value::Array(items) => items.iter().for_each(|item| collect_json(item, facts)),
        Value::Object(map) => map.values().for_each(|item| collect_json(item, facts)),
        _ => {}
    }
}

/// Bare number scan with no ignore rules: digit runs (commas, decimals,
/// ×1000 for a k suffix) anywhere in the text.
fn scan_text(text: &str, facts: &mut Vec<f64>) {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let (value, end) = parse_core(&chars, i);
        let mut value = value;
        if matches!(chars.get(end), Some('k') | Some('K')) {
            value *= 1000.0;
            facts.push(value);
            i = end + 1;
        } else {
            facts.push(value);
            i = end;
        }
    }
}

/// True when `token` and `candidate` are the same number under the
/// grounding tolerances: absolute/relative closeness, equality with the
/// candidate rounded to 0/1/2 decimals, within 1% for money amounts
/// ≥ 1000 (rounded restatements like "≈ $17,500"), or within 0.5
/// percentage points for percents ("ราว 18%" vs 18.3).
fn close_enough(token: &NumToken, candidate: f64) -> bool {
    let diff = (token.value - candidate).abs();
    if diff <= 0.01_f64.max(0.005 * candidate.abs()) {
        return true;
    }
    for scale in [1.0, 10.0, 100.0] {
        if ((candidate * scale).round() / scale - token.value).abs() <= 1e-9 {
            return true;
        }
    }
    if token.kind == NumKind::Money
        && token.value >= 1000.0
        && candidate != 0.0
        && diff <= 0.01 * candidate.abs()
    {
        return true;
    }
    token.kind == NumKind::Percent && diff <= 0.5
}

/// A fact grounds a token when the token's kind can read that fact.
/// Percent tokens compare ONLY in percent space: a fraction fact
/// (|f| ≤ 1) is read as f×100 — 0.214 means 21.4%, never 0.21% — while a
/// larger fact is already a percent number. Money and plain tokens read
/// the raw fact and its per-share value (f/100), so "$1.83" grounds
/// against a 183-premium fact.
fn matches_fact(token: &NumToken, fact: f64) -> bool {
    match token.kind {
        NumKind::Percent => {
            let percent = if fact.abs() <= 1.0 {
                fact * 100.0
            } else {
                fact
            };
            close_enough(token, percent)
        }
        NumKind::Money | NumKind::Plain => {
            close_enough(token, fact) || close_enough(token, fact / 100.0)
        }
    }
}

pub fn ungrounded(answer: &str, facts: &[f64]) -> Vec<String> {
    let mut reported: Vec<String> = Vec::new();
    for token in extract_numbers(answer) {
        if facts.iter().any(|fact| matches_fact(&token, *fact)) {
            continue;
        }
        if !reported.contains(&token.raw) {
            reported.push(token.raw);
        }
    }
    reported
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(String, f64, NumKind)> {
        extract_numbers(text)
            .into_iter()
            .map(|t| (t.raw, t.value, t.kind))
            .collect()
    }

    // ---------- extraction ----------

    #[test]
    fn extracts_dollar_money_with_thai_around() {
        assert_eq!(
            kinds("คุณจะได้เงินทันที $183 ต่อสัญญา"),
            vec![("$183".into(), 183.0, NumKind::Money)]
        );
    }

    #[test]
    fn extracts_dollar_money_with_commas_and_cents() {
        assert_eq!(
            kinds("จ่าจริง $1,250.50 ต่อสัญญา"),
            vec![("$1,250.50".into(), 1250.5, NumKind::Money)]
        );
    }

    #[test]
    fn extracts_thai_dollar_word() {
        assert_eq!(
            kinds("รับทันที 183 ดอลลาร์ ทุกสัญญา"),
            vec![("183 ดอลลาร์".into(), 183.0, NumKind::Money)]
        );
    }

    #[test]
    fn extracts_k_suffix_as_thousands() {
        assert_eq!(
            kinds("มูลค่ารวมราว $17.5k"),
            vec![("$17.5k".into(), 17500.0, NumKind::Money)]
        );
        assert_eq!(
            kinds("วันนี้ 17.5K บาทเปลี่ยนมือ"),
            vec![("17.5K".into(), 17500.0, NumKind::Money)]
        );
    }

    #[test]
    fn extracts_attached_percent() {
        assert_eq!(
            kinds("โอกาสโดนซื้อราว 18% ในสัปดาห์นี้"),
            vec![("18%".into(), 18.0, NumKind::Percent)]
        );
    }

    #[test]
    fn extracts_percent_with_space_before_sign() {
        assert_eq!(
            kinds("ผลตอบแทนราว 18.5 % ต่อปี"),
            vec![("18.5 %".into(), 18.5, NumKind::Percent)]
        );
    }

    #[test]
    fn extracts_thai_percent_word() {
        assert_eq!(
            kinds("เพิ่มขึ้น 18 เปอร์เซ็นต์ ต่อปี"),
            vec![("18 เปอร์เซ็นต์".into(), 18.0, NumKind::Percent)]
        );
        assert_eq!(
            kinds("ลดลง 4 เปอร์เซนต์"),
            vec![("4 เปอร์เซนต์".into(), 4.0, NumKind::Percent)]
        );
    }

    #[test]
    fn extracts_plain_numbers_in_thai_sentences() {
        assert_eq!(
            kinds("สัญญานี้มีผู้ถือ 12,500 ราย อายุ 25 วัน"),
            vec![
                ("12,500".into(), 12500.0, NumKind::Plain),
                ("25".into(), 25.0, NumKind::Plain),
            ]
        );
    }

    #[test]
    fn money_and_percent_below_thirteen_still_count() {
        // the 0–12 ignore is for plain counts only; "$8" and "5%" are data
        assert_eq!(
            kinds("ขาดทุนสูงสุด $8 โอกาส 5%"),
            vec![
                ("$8".into(), 8.0, NumKind::Money),
                ("5%".into(), 5.0, NumKind::Percent),
            ]
        );
    }

    // ---------- ignores ----------

    #[test]
    fn ignores_years_between_1900_and_2100() {
        assert_eq!(kinds("งบปี 2026 ออกปี 1999"), vec![]);
        assert_eq!(
            kinds("ตั้งแต่ปี 2026 ถึงปี 2100 ยกเว้น 1899"),
            vec![("1899".into(), 1899.0, NumKind::Plain)]
        );
    }

    #[test]
    fn ignores_thai_month_dates_both_forms() {
        assert_eq!(kinds("งบออก 18 ธ.ค. 2026"), vec![]);
        assert_eq!(kinds("ครบกำหนด 25 มิถุนายน"), vec![]);
        assert_eq!(kinds("วันนัด 5 พ.ย."), vec![]);
    }

    #[test]
    fn ignores_slash_and_iso_dates() {
        assert_eq!(kinds("นัดประชุม 12/18"), vec![]);
        assert_eq!(kinds("งบอก 2026-12-18"), vec![]);
        assert_eq!(
            kinds("วันที่ 2026-12-18 ราคาเท่ากับ 175"),
            vec![("175".into(), 175.0, NumKind::Plain)]
        );
    }

    #[test]
    fn ignores_clock_times() {
        assert_eq!(kinds("เปิดตลาด 10:30 ปิด 16:00 น."), vec![]);
        assert_eq!(kinds("ออกอากาศ 10:30:45"), vec![]);
    }

    #[test]
    fn ignores_list_markers_at_line_start() {
        assert_eq!(kinds("1) ซื้อก่อน\n2) ขายทีหลัง"), vec![]);
        assert_eq!(kinds("1. แรก\n2. สอง"), vec![]);
        assert_eq!(kinds("- 2. ตัวเลือกที่สอง"), vec![]);
        // a decimal at a line start is data, not a marker
        assert_eq!(
            kinds("1.5 เท่าของทุน"),
            vec![("1.5".into(), 1.5, NumKind::Plain)]
        );
    }

    #[test]
    fn ignores_digits_inside_occ_symbols_and_tickers() {
        assert_eq!(kinds("สัญญา NVDA261218P00175000 น่าสนใจ"), vec![]);
        assert_eq!(kinds("ดู Googl2026 ด้วย"), vec![]);
        assert_eq!(kinds("เลือกข้อ 175C"), vec![]);
    }

    #[test]
    fn ignores_small_plain_integers() {
        assert_eq!(kinds("1 ใน 5 ของพอร์ต และมี 3 ตัว"), vec![]);
        assert_eq!(
            kinds("พอร์ตมี 13 ตัว"),
            vec![("13".into(), 13.0, NumKind::Plain)]
        );
    }

    #[test]
    fn ignores_hundred_shares_multiplier_only() {
        assert_eq!(
            kinds("ซื้อ 100 หุ้น ที่ราคา $175"),
            vec![("$175".into(), 175.0, NumKind::Money)]
        );
        assert_eq!(kinds("ซื้อ100หุ้นเลย"), vec![]);
        // other share counts are still data claims
        assert_eq!(
            kinds("ถือ 1000 หุ้น"),
            vec![("1000".into(), 1000.0, NumKind::Plain)]
        );
    }

    #[test]
    fn realistic_answer_extracts_only_data_numbers() {
        assert_eq!(
            kinds(
                "คุณรับเงินทันที $183 แลกกับการพร้อมซื้อ 100 หุ้นที่ $175 โอกาสโดนซื้อราว 18% งบออก 18 ธ.ค. 2026"
            ),
            vec![
                ("$183".into(), 183.0, NumKind::Money),
                ("$175".into(), 175.0, NumKind::Money),
                ("18%".into(), 18.0, NumKind::Percent),
            ]
        );
    }

    // ---------- matching ----------

    #[test]
    fn money_token_matches_exact_fact() {
        assert_eq!(ungrounded("ได้ $183 ต่อสัญญา", &[183.0]), Vec::<String>::new());
    }

    #[test]
    fn percent_token_matches_fraction_fact_from_json() {
        let facts = collect_facts(&[r#"{"prob_itm": 0.18}"#]);
        assert_eq!(ungrounded("โอกาสราว 18%", &facts), Vec::<String>::new());
    }

    #[test]
    fn percent_token_matches_within_half_point() {
        assert_eq!(ungrounded("โอกาสราว 18%", &[18.3]), Vec::<String>::new());
        assert_eq!(
            ungrounded("ราว 19%", &[18.3]),
            vec!["19%".to_string()],
            "0.7 points away must not match"
        );
    }

    #[test]
    fn percent_token_never_reads_a_fraction_fact_as_a_small_percent() {
        // 0.214 is the fraction 21.4%, so "0.21%" is a different number —
        // the 100× unit error must not slip through.
        assert_eq!(
            ungrounded("ผลตอบแทน 0.21% ต่อวัน", &[0.214]),
            vec!["0.21%".to_string()]
        );
    }

    #[test]
    fn percent_token_reads_fraction_facts_in_percent_space() {
        assert_eq!(
            ungrounded("ผลตอบแทนราว 21.4%", &[0.214]),
            Vec::<String>::new()
        );
        // 21.4 rounds to 21 and is within 0.5pp
        assert_eq!(
            ungrounded("ผลตอบแทนราว 21%", &[0.214]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn half_percent_written_against_a_fraction_fact_is_reported() {
        // 0.5 as a fraction is 50%, not 0.5%
        assert_eq!(ungrounded("โอกาส 0.5%", &[0.5]), vec!["0.5%".to_string()]);
    }

    #[test]
    fn percent_token_matches_percent_scale_fact_directly() {
        assert_eq!(ungrounded("ผลตอบแทน 18.2%", &[18.2]), Vec::<String>::new());
    }

    #[test]
    fn money_token_still_reads_hundred_scale_facts_per_share() {
        assert_eq!(
            ungrounded("ได้ $1.83 ต่อสัญญา", &[183.0]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn k_money_matches_full_amount_fact() {
        assert_eq!(
            ungrounded("มูลค่ารวมราว $17.5k", &[17500.0]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn comma_money_matches_decimal_fact() {
        assert_eq!(
            ungrounded("จ่ายจริง $1,250.50", &[1250.5]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn thai_dollar_word_matches_fact() {
        assert_eq!(
            ungrounded("รับ 183 ดอลลาร์ ทันที", &[183.0]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn big_money_allows_one_percent_relative_error() {
        // "≈ $17,500" vs 17650 — within 1%, grounded
        assert_eq!(
            ungrounded("มูลค่าราว $17,500", &[17650.0]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn unmatched_money_is_reported() {
        assert_eq!(
            ungrounded("ได้ $999 จากทุน $183", &[183.0]),
            vec!["$999".to_string()]
        );
    }

    #[test]
    fn duplicates_are_reported_once_in_order() {
        assert_eq!(
            ungrounded("ได้ $999 และอีก $888 แล้วก็ $999", &[183.0]),
            vec!["$999".to_string(), "$888".to_string()]
        );
    }

    // ---------- collect_facts ----------

    #[test]
    fn collect_facts_walks_json_and_scans_string_leaves() {
        let facts = collect_facts(&[
            r#"{"top": {"premium": 183.5}, "note": "ประมาณ $200 ถึง 250", "tags": ["a", 7]}"#,
        ]);
        for expected in [183.5, 200.0, 250.0, 7.0] {
            assert!(facts.contains(&expected), "missing {expected} in {facts:?}");
        }
    }

    #[test]
    fn collect_facts_keeps_raw_values_without_unit_copies() {
        // Unit readings live in matching; pre-multiplied copies in the
        // pool made raw fractions reachable for percent tokens.
        let facts = collect_facts(&[r#"{"p": 0.18, "q": 18.3, "r": 12500.0}"#]);
        assert_eq!(facts.len(), 3, "no ×100 or /100 copies: {facts:?}");
        for expected in [0.18, 18.3, 12500.0] {
            assert!(facts.contains(&expected), "missing {expected}: {facts:?}");
        }
    }

    #[test]
    fn collect_facts_scans_non_json_text_sources() {
        let facts = collect_facts(&["อยากได้เดือนละ $200 มีอะไรดี"]);
        assert!(facts.contains(&200.0), "user text numbers count: {facts:?}");
    }

    #[test]
    fn collect_facts_reads_k_scales_in_text() {
        let facts = collect_facts(&["มูลค่ารวม 17.5k บาท"]);
        assert!(facts.contains(&17500.0), "{facts:?}");
    }

    // ---------- acceptance sets: 6 realistic answers ----------

    const CASES: [(&str, &str); 6] = [
        (
            "คุณรับเงินทันที $183 แลกกับการพร้อมซื้อ 100 หุ้นที่ $175 โอกาสโดนซื้อราว 18% งบออก 18 ธ.ค. 2026",
            r#"{"premium": 183.0, "strike": 175, "prob_itm": 0.18, "earnings_date": "2026-12-18"}"#,
        ),
        (
            "ตอนนี้ราคาหุ้นอยู่ที่ $175.40 แนวรับแถว $168.50 แนวต้านประมาณ $192.25",
            r#"{"price": 175.40, "support": 168.5, "resistance": 192.25}"#,
        ),
        (
            "สัญญานี้มีผู้ถือ 1,250 ราย ซื้อขายวันนี้ 12,500 สัญญา คิดเป็นราว 52% ของค่าเฉลี่ย",
            r#"{"holders": 1250, "volume": 12500, "vs_avg": 0.52}"#,
        ),
        (
            "คุณจ่ายไป $183 ถ้าโดนซื้อจะได้ $175 ต่อสัญญา ขาดทุนสูงสุดคือ $8 ต่อสัญญา",
            r#"{"premium": 183, "strike": 175, "max_loss": 8}"#,
        ),
        (
            "ปันผลราว 0.75 ดอลลาร์ต่อสัญญา คิดเป็นผลตอบแทนประมาณ 4.3 เปอร์เซ็นต์ต่อปี",
            r#"{"dividend": 0.75, "yield_pct": 4.3}"#,
        ),
        (
            "มูลค่าที่ซื้อขายวันนี้รวมราว $17.5k เทียบกับเมื่อวานราว $16.8k",
            r#"{"today_value": 17500, "yesterday_value": 16800}"#,
        ),
    ];

    fn facts_of(tool_json: &str) -> Vec<f64> {
        collect_facts(&[tool_json])
    }

    #[test]
    fn realistic_answers_report_nothing() {
        for (answer, tool_json) in CASES {
            assert_eq!(
                ungrounded(answer, &facts_of(tool_json)),
                Vec::<String>::new(),
                "false positive on: {answer}"
            );
        }
    }

    #[test]
    fn changed_number_is_reported_exactly() {
        let mutated: [(&str, &str, &str); 6] = [
            (
                "คุณรับเงินทันที $199 แลกกับการพร้อมซื้อ 100 หุ้นที่ $175 โอกาสโดนซื้อราว 18% งบออก 18 ธ.ค. 2026",
                CASES[0].1,
                "$199",
            ),
            (
                "ตอนนี้ราคาหุ้นอยู่ที่ $255.40 แนวรับแถว $168.50 แนวต้านประมาณ $192.25",
                CASES[1].1,
                "$255.40",
            ),
            (
                "สัญญานี้มีผู้ถือ 1,250 ราย ซื้อขายวันนี้ 12,500 สัญญา คิดเป็นราว 61% ของค่าเฉลี่ย",
                CASES[2].1,
                "61%",
            ),
            (
                "คุณจ่ายไป $183 ถ้าโดนซื้อจะได้ $175 ต่อสัญญา ขาดทุนสูงสุดคือ $9 ต่อสัญญา",
                CASES[3].1,
                "$9",
            ),
            (
                "ปันผลราว 0.75 ดอลลาร์ต่อสัญญา คิดเป็นผลตอบแทนประมาณ 7.1 เปอร์เซ็นต์ต่อปี",
                CASES[4].1,
                "7.1 เปอร์เซ็นต์",
            ),
            (
                "มูลค่าที่ซื้อขายวันนี้รวมราว $19.9k เทียบกับเมื่อวานราว $16.8k",
                CASES[5].1,
                "$19.9k",
            ),
        ];
        for (answer, tool_json, invented) in mutated {
            assert_eq!(
                ungrounded(answer, &facts_of(tool_json)),
                vec![invented.to_string()],
                "wrong report for: {answer}"
            );
        }
    }
}
