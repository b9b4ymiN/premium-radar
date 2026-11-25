//! Universe — weekly "top ~100 US stocks by market cap" list สำหรับ Lab
//! recorder server.
//!
//! Source chain (owner-approved):
//! 1. NASDAQ screener JSON (primary) — undocumented endpoint, may break
//! 2. Wikipedia "S&P 100" constituents table (fallback) — แทน iShares
//!    OEF CSV ที่ URL คืน bot-wall HTML แทน CSV (verified live 2026-11-25),
//!    fetch failure เป็น normal fallback path
//! 3. last good cache file (last resort) — อายุ report ใน warnings
//!
//! Wikipedia decision: ตารางไม่มี market cap — เก็บ ranking proxy ใน
//! [`UniverseEntry::rank_value`] (market cap ยืมจาก cache รอบก่อนผ่าน
//! [`rank_from_cache`] ถ้ามี ไม่งั้นเรียงตามลำดับในตาราง) และเว้น
//! `market_cap = None` เพื่อไม่ให้ cache file อ้าง market cap ปลอม;
//! NASDAQ rows เก็บ `market_cap` จริงและไม่มี `rank_value` (serialise
//! เฉพาะตอนมีค่า จึงตรง contract
//! `{"symbol","name","sector","market_cap"}` พอดี ส่วน Wikipedia snapshot
//! จะมี key `rank_value` เพิ่ม — serde consumers ปกติ ignore unknown
//! fields)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

/// หุ้นตัวเดียวใน universe — `market_cap` เป็น cap จริง (NASDAQ) หรือ
/// `None` (Wikipedia) โดยใช้ `rank_value` เป็นตัวเรียงแทน
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniverseEntry {
    pub symbol: String,
    pub name: String,
    pub sector: Option<String>,
    pub market_cap: Option<f64>,
    /// Ranking proxy สำหรับ source ที่ไม่มี market cap จริง (Wikipedia —
    /// ลำดับในตาราง หรือ cap ยืมจาก cache ก่อนหน้า) — serialise เฉพาะเมื่อมีค่า
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank_value: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UniverseSource {
    Nasdaq,
    /// Fallback — เดิมชื่อ `Oef`; cache เก่าที่เขียน `"oef"` ยังโหลดได้
    /// ผ่าน serde alias
    #[serde(alias = "oef")]
    Wikipedia,
    Cache,
}

impl UniverseSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Nasdaq => "nasdaq",
            Self::Wikipedia => "wikipedia",
            Self::Cache => "cache",
        }
    }
}

/// Universe หนึ่งชุดที่ server ใช้ได้ — `fetched_at` เป็นเวลาที่ได้มา
/// (cache path คือเวลาดั้งเดิม ไม่ใช่เวลาที่อ่าน cache)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniverseSnapshot {
    pub fetched_at: DateTime<Utc>,
    pub source: UniverseSource,
    pub symbols: Vec<UniverseEntry>,
}

/// NASDAQ screener (undocumented): `GET .../api/screener/stocks?...`
pub const NASDAQ_SCREENER_URL: &str =
    "https://api.nasdaq.com/api/screener/stocks?tableonly=true&limit=10000&download=true";

/// Wikipedia API ของหน้า "S&P 100" — action=parse คืน HTML ของทั้งหน้าใน
/// `parse.text` (formatversion=2 ทำให้ `text` เป็น string ตรงๆ)
pub const WIKIPEDIA_SP100_API_URL: &str = "https://en.wikipedia.org/w/api.php?action=parse&page=S%26P_100&prop=text&format=json&formatversion=2";

/// UA ตาม Wikimedia policy — ระบุ tool + contact, ไม่มี personal data
pub const WIKIPEDIA_UA: &str =
    "PremiumRadarRecorder/0.1 (+https://github.com/b9b4ymiN/premium-radar)";

/// Generic desktop Chrome UA — ไม่มี personal data
pub const CHROME_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

/// ตัวเรียงอันดับ: market cap จริง ถ้าไม่มีใช้ rank proxy (Wikipedia)
fn rank_key(entry: &UniverseEntry) -> Option<f64> {
    entry.market_cap.or(entry.rank_value)
}

/// แปลง `BRK/B` / `BRK-B` (root 1–6 ตัวอักษร + class ตัวเดียว) เป็น
/// Webull style `BRK.B`; symbol อื่น uppercase ตามเดิม
fn normalise_symbol(raw: &str) -> String {
    let s = raw.trim().to_ascii_uppercase();
    if let Some(pos) = s.find(['/', '-']) {
        let (root, rest) = s.split_at(pos);
        let class = &rest[1..];
        let root_ok =
            !root.is_empty() && root.len() <= 6 && root.chars().all(|c| c.is_ascii_alphabetic());
        let class_ok = class.chars().all(|c| c.is_ascii_alphabetic());
        if root_ok && class_ok {
            return format!("{root}.{class}");
        }
    }
    s
}

/// Name keywords แบบ whole-word (case-insensitive) — ใช้ word boundary
/// ไม่ใช่ substring เพราะ substring "Unit" จะโดน "United Airlines"
const NON_COMMON_NAME_WORDS: &[&str] = &[
    "warrant",
    "warrants",
    "unit",
    "units",
    "right",
    "rights",
    "preferred",
    "preference",
    "depositary",
    "etf",
];

fn passes_name_rules(name: &str) -> bool {
    !name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|w| NON_COMMON_NAME_WORDS.contains(&w.to_ascii_lowercase().as_str()))
}

/// Symbol rules หลัง normalise: ไม่มี `^` (index/preferred), ไม่มี `/`
/// ที่เหลือ (warrant แบบมีวันที่), ไม่มี suffix `.W`/`.U`/`.R`
/// (warrant/unit/right)
fn passes_symbol_rules(symbol: &str) -> bool {
    !symbol.contains('^')
        && !symbol.contains('/')
        && !symbol.contains(".W")
        && !symbol.contains(".U")
        && !symbol.contains(".R")
}

/// กรอง + เรียง + ตัด top n — rule list (tested):
/// 1. ตัด entry ที่ไม่มีค่าเรียง (market_cap หรือ rank_value) หรือค่า
///    ≤ 0 / ไม่ finite — ครอบ marketCap `"0.00"` (warrant) และ `""`
/// 2. normalise symbol เป็น Webull style (`BRK/B`, `BRK-B` → `BRK.B`)
/// 3. ตัด symbol ที่มี `^`, มี `/` ที่เหลือหลัง normalise, หรือมี
///    `.W` / `.U` / `.R`
/// 4. ตัด name ที่มี whole word: warrant(s), unit(s), right(s),
///    preferred, preference, depositary, etf
/// 5. dedupe symbol ที่ซ้ำหลัง normalise (ตัวที่ rank สูงกว่าชนะ)
/// 6. เรียงตาม rank key มาก → น้อย แล้วตัดเหลือ `n` ตัวแรก
///
/// Share-class pairs (GOOGL & GOOG) ไม่ตัด — liquidity filter ที่หลัง
/// เป็นตัวตัดสิน
pub fn top_n(entries: &[UniverseEntry], n: usize) -> Vec<UniverseEntry> {
    let mut kept: Vec<UniverseEntry> = entries
        .iter()
        .filter(|e| rank_key(e).is_some_and(|v| v.is_finite() && v > 0.0))
        .filter(|e| passes_name_rules(&e.name))
        .map(|e| {
            let mut e = e.clone();
            e.symbol = normalise_symbol(&e.symbol);
            e
        })
        .filter(|e| passes_symbol_rules(&e.symbol))
        .collect();
    kept.sort_by(|a, b| {
        rank_key(b)
            .unwrap_or(0.0)
            .total_cmp(&rank_key(a).unwrap_or(0.0))
    });
    let mut seen: HashSet<String> = HashSet::new();
    kept.retain(|e| seen.insert(e.symbol.clone()));
    kept.truncate(n);
    kept
}

/// เลือก universe ตาม chain: NASDAQ → Wikipedia → cache → Err
///
/// Primary ผ่านเมื่อได้ ≥ 80% ของ n หลัง filter; fallback ผ่านเมื่อ parse
/// ได้และมี entry เหลืออย่างน้อย 1 (spec ผูก threshold กับ primary
/// เท่านั้น); cache คืน snapshot เดิมโดยเปลี่ยน source เป็น `Cache`
/// พร้อม warning ที่มีอายุเป็นวัน
pub fn choose(
    primary: Result<Vec<UniverseEntry>, String>,
    fallback: Result<Vec<UniverseEntry>, String>,
    cache: Option<UniverseSnapshot>,
    n: usize,
    now: DateTime<Utc>,
) -> Result<(UniverseSnapshot, Vec<String>), String> {
    let threshold = (n * 8).div_ceil(10);
    let mut reasons: Vec<String> = Vec::new();

    match primary {
        Ok(list) => {
            let top = top_n(&list, n);
            if top.len() >= threshold {
                return Ok((
                    UniverseSnapshot {
                        fetched_at: now,
                        source: UniverseSource::Nasdaq,
                        symbols: top,
                    },
                    Vec::new(),
                ));
            }
            reasons.push(format!(
                "nasdaq yielded only {} of >= {threshold} entries after filter",
                top.len()
            ));
        }
        Err(e) => reasons.push(format!("nasdaq failed: {e}")),
    }

    match fallback {
        Ok(list) => {
            let top = top_n(&list, n);
            if !top.is_empty() {
                let count = top.len();
                return Ok((
                    UniverseSnapshot {
                        fetched_at: now,
                        source: UniverseSource::Wikipedia,
                        symbols: top,
                    },
                    vec![format!(
                        "{}; using Wikipedia S&P 100 fallback ({count} entries)",
                        reasons.join("; ")
                    )],
                ));
            }
            reasons.push("wikipedia yielded 0 entries after filter".to_string());
        }
        Err(e) => reasons.push(format!("wikipedia failed: {e}")),
    }

    if let Some(cache) = cache {
        let age_days = (now - cache.fetched_at).num_days().max(0);
        let original = cache.source;
        let warning = format!(
            "{}; serving cached universe ({} day(s) old, original source {})",
            reasons.join("; "),
            age_days,
            original.as_str()
        );
        let mut served = cache;
        served.source = UniverseSource::Cache;
        return Ok((served, vec![warning]));
    }

    Err(format!("universe unavailable: {}", reasons.join("; ")))
}

/// อ่าน cache — None เมื่อไฟล์ไม่มีหรือ parse ไม่ผ่าน (treat เป็น
/// "ไม่มี cache" ไม่ใช่ error)
pub fn read_cache(path: &Path) -> Option<UniverseSnapshot> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// เขียน cache แบบ atomic: เขียน `<path>.tmp` แล้ว rename ทับ
pub fn write_cache(path: &Path, snapshot: &UniverseSnapshot) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let json = serde_json::to_string_pretty(snapshot).map_err(io::Error::other)?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

/// เดินตามลำดับ rank ของ universe เก็บเฉพาะตัวที่ liquid จนครบ n —
/// ตัวที่ไม่ liquid ถูกแทนที่ด้วยชื่อถัดไปใน rank (caller ส่งลิสต์ยาว
/// กว่า n เช่น top 130)
pub fn apply_liquidity(
    ranked: &[UniverseEntry],
    is_liquid: impl Fn(&str) -> bool,
    n: usize,
) -> Vec<UniverseEntry> {
    ranked
        .iter()
        .filter(|e| is_liquid(&e.symbol))
        .take(n)
        .cloned()
        .collect()
}

fn parse_loose_number(s: &str) -> Option<f64> {
    let cleaned: String = s
        .trim()
        .trim_matches('"')
        .chars()
        .filter(|c| *c != ',')
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        cleaned.parse::<f64>().ok()
    }
}

fn parse_market_cap_value(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::String(s) => parse_loose_number(s),
        other => other.as_f64(),
    }
}

/// แปลง NASDAQ screener JSON — ตัดแถวที่ symbol ว่างเงียบๆ (tolerant),
/// Err เมื่อโครงสร้าง `data.rows` หาย
pub fn parse_nasdaq(json: &serde_json::Value) -> Result<Vec<UniverseEntry>, String> {
    let rows = json
        .get("data")
        .and_then(|d| d.get("rows"))
        .and_then(|r| r.as_array())
        .ok_or_else(|| "data.rows missing or not an array".to_string())?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(symbol) = row
            .get("symbol")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let name = row
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let sector = row
            .get("sector")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "--");
        let market_cap = row.get("marketCap").and_then(parse_market_cap_value);
        out.push(UniverseEntry {
            symbol: symbol.to_string(),
            name,
            sector: sector.map(str::to_string),
            market_cap,
            rank_value: None,
        });
    }
    Ok(out)
}

/// ถอด tag ออกจาก HTML fragment เหลือ text เปล่า แล้ว decode entity ที่
/// เจอในตาราง constituents (`&amp;`, `&#39;`, `&quot;`, `&nbsp;`) —
/// `&amp;` ต้องทำเป็นตัวสุดท้าย ไม่งั้น `&amp;quot;` จะถูก decode ซ้ำ
fn cell_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            text.push(c);
        }
    }
    text.replace("&nbsp;", " ")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .trim()
        .to_string()
}

/// เก็บ text ของ cell `<td …>…</td>` ทุกอันในแถว — header row ที่ใช้
/// `<th>` จะได้ 0 cells จึงหลุดไปเอง
fn row_cells(row: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut rest = row;
    while let Some(open) = rest.find("<td") {
        let at = &rest[open..];
        let Some(tag_gt) = at.find('>') else { break };
        let inner = tag_gt + 1;
        let Some(close) = at[inner..].find("</td>") else {
            break;
        };
        cells.push(cell_text(&at[inner..inner + close]));
        rest = &at[inner + close + "</td>".len()..];
    }
    cells
}

/// เก็บ inner HTML ของแถว `<tr …>…</tr>` ทุกแถวใน table — เช็คตัวตาม
/// `<tr` กัน tag อื่นที่ขึ้นต้นเหมือนกัน (เช่น `<track>`)
fn table_rows(table: &str) -> Vec<&str> {
    let mut rows = Vec::new();
    let mut rest = table;
    while let Some(open) = rest.find("<tr") {
        let at = &rest[open..];
        let after = &at[3..];
        if !after.starts_with('>') && !after.starts_with(|c: char| c.is_ascii_whitespace()) {
            rest = after;
            continue;
        }
        let Some(tag_gt) = at.find('>') else { break };
        let inner = tag_gt + 1;
        let Some(close) = at[inner..].find("</tr>") else {
            break;
        };
        rows.push(&at[inner..inner + close]);
        rest = &at[inner + close + "</tr>".len()..];
    }
    rows
}

/// แปลง Wikipedia "S&P 100" constituents จาก action=parse API JSON —
/// มือเลือก (ไม่เพิ่ม crate): หาตาราง `id="constituents"`, เก็บ 3 cells
/// แรกของแถว `<td>` เป็น (symbol, name, sector)
///
/// ตารางไม่มี market cap: `market_cap = None` และ `rank_value` เป็นเลข
/// เรียงมาก→น้อยตามลำดับแถว (แถวบนสุด = จำนวนแถว) เพื่อให้ `top_n`
/// เก็บทุกตัวไว้และเรียงตามลำดับในตาราง — จะ override ด้วย market cap
/// จริงจาก cache รอบก่อนได้ที่ [`rank_from_cache`]
pub fn parse_wikipedia_sp100(api_json: &serde_json::Value) -> Result<Vec<UniverseEntry>, String> {
    let text = api_json
        .get("parse")
        .and_then(|p| p.get("text"))
        .and_then(|t| t.as_str())
        .ok_or_else(|| "parse.text missing or not a string".to_string())?;

    let id_pos = text
        .find("id=\"constituents\"")
        .ok_or_else(|| "constituents table not found".to_string())?;
    let table_start = text[..id_pos]
        .rfind("<table")
        .ok_or_else(|| "constituents table missing <table> tag".to_string())?;
    let tag_gt = text[table_start..]
        .find('>')
        .ok_or_else(|| "constituents <table> tag unterminated".to_string())?;
    let open_tag_end = table_start + tag_gt + 1;
    let close_rel = text[id_pos..]
        .find("</table>")
        .ok_or_else(|| "constituents table missing </table>".to_string())?;
    let table_end = id_pos + close_rel;

    let mut entries: Vec<UniverseEntry> = Vec::new();
    for row in table_rows(&text[open_tag_end..table_end]) {
        let cells = row_cells(row);
        if cells.len() < 3 || cells[0].is_empty() {
            continue;
        }
        let sector = cells[2].as_str();
        entries.push(UniverseEntry {
            symbol: cells[0].clone(),
            name: cells[1].clone(),
            sector: if sector.is_empty() {
                None
            } else {
                Some(sector.to_string())
            },
            market_cap: None,
            rank_value: None,
        });
    }
    if entries.is_empty() {
        return Err("constituents table has no data rows".to_string());
    }
    let total = entries.len() as f64;
    for (i, e) in entries.iter_mut().enumerate() {
        e.rank_value = Some(total - i as f64);
    }
    Ok(entries)
}

/// ยืม rank จาก cache รอบก่อนมาเป็น `rank_value` ของ entry ที่ symbol
/// ตรงกัน (cache NASDAQ ให้ `market_cap` จริง, cache Wikipedia รอบก่อน
/// ให้ `rank_value`) — ทำให้ Wikipedia fallback เรียงตาม market cap
/// ล่าสุดที่รู้จักแทนลำดับในตาราง; ตัวที่ไม่มีใน cache (เพิ่งเข้า index)
/// คงอันดับตามตารางไว้ (ค่า table order เล็กกว่า cap จริงจึงตกท้าย)
pub fn rank_from_cache(entries: &mut [UniverseEntry], cache: &[UniverseEntry]) {
    let caps: HashMap<String, f64> = cache
        .iter()
        .filter_map(|e| {
            let v = rank_key(e).filter(|v| v.is_finite() && *v > 0.0)?;
            Some((e.symbol.trim().to_ascii_uppercase(), v))
        })
        .collect();
    for e in entries {
        if let Some(v) = caps.get(e.symbol.trim().to_ascii_uppercase().as_str()) {
            e.rank_value = Some(*v);
        }
    }
}

/// Fetch NASDAQ screener JSON (browser-like headers, 20 s timeout)
#[cfg(feature = "client")]
pub fn fetch_nasdaq() -> Result<serde_json::Value, String> {
    use reqwest::header::{ACCEPT, ORIGIN, REFERER, USER_AGENT};
    let http = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("build http client: {e}"))?;
    let resp = http
        .get(NASDAQ_SCREENER_URL)
        .header(USER_AGENT, CHROME_UA)
        .header(ACCEPT, "application/json")
        .header(ORIGIN, "https://www.nasdaq.com")
        .header(REFERER, "https://www.nasdaq.com/")
        .send()
        .map_err(|e| format!("nasdaq request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("nasdaq screener HTTP {status}"));
    }
    resp.json::<serde_json::Value>()
        .map_err(|e| format!("nasdaq json decode: {e}"))
}

/// Fetch Wikipedia "S&P 100" constituents JSON (20 s timeout) — failure
/// เป็น normal fallback path ไม่ใช่ panic
#[cfg(feature = "client")]
pub fn fetch_wikipedia_sp100() -> Result<serde_json::Value, String> {
    use reqwest::header::{ACCEPT, USER_AGENT};
    let http = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("build http client: {e}"))?;
    let resp = http
        .get(WIKIPEDIA_SP100_API_URL)
        .header(USER_AGENT, WIKIPEDIA_UA)
        .header(ACCEPT, "application/json")
        .send()
        .map_err(|e| format!("wikipedia request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("wikipedia HTTP {status}"));
    }
    resp.json::<serde_json::Value>()
        .map_err(|e| format!("wikipedia json decode: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn nasdaq_fixture() -> serde_json::Value {
        serde_json::json!({ "data": { "totalrecords": 14, "rows": [
            {"symbol":"AAPL","name":"Apple Inc.","lastsale":"229.00","marketCap":"3400000000000.00","sector":"Technology","country":"United States","industry":"Computer Manufacturing"},
            {"symbol":"NVDA","name":"NVIDIA Corporation","marketCap":"3300000000000.00","sector":"Technology"},
            {"symbol":"MSFT","name":"Microsoft Corporation","marketCap":"3100000000000.00","sector":"Technology"},
            {"symbol":"GOOGL","name":"Alphabet Inc. Class A","marketCap":"2000000000000.00","sector":"Communication Services"},
            {"symbol":"GOOG","name":"Alphabet Inc. Class C","marketCap":"1980000000000.00","sector":"Communication Services"},
            {"symbol":"TSLA","name":"Tesla, Inc.","marketCap":"1000000000000.00","sector":"Consumer Discretionary"},
            {"symbol":"BRK/B","name":"Berkshire Hathaway Inc.","marketCap":"880000000000.00","sector":"Financials"},
            {"symbol":"UAL","name":"United Airlines Holdings, Inc.","marketCap":"30000000000.00","sector":"Industrials"},
            {"symbol":"ABCD.W","name":"Example Corp Warrant","marketCap":"0.00","sector":"Finance"},
            {"symbol":"EFGH.U","name":"Acme Acquisition Corp Unit","marketCap":"123000.00","sector":"Finance"},
            {"symbol":"IJKL","name":"Global Preferred Holdings","marketCap":"500000000.00","sector":"Finance"},
            {"symbol":"SPY","name":"SPDR S&P 500 ETF Trust","marketCap":"500000000000.00","sector":"--"},
            {"symbol":"NOSECT","name":"No Sector Inc","marketCap":"9000000.00","sector":"--"},
            {"symbol":"EMPT","name":"Empty Cap Inc","marketCap":"","sector":"Healthcare"}
        ]}})
    }

    #[test]
    fn parse_nasdaq_parses_rows_and_caps() {
        let entries = parse_nasdaq(&nasdaq_fixture()).unwrap();
        assert_eq!(entries.len(), 14);
        let aapl = entries.iter().find(|e| e.symbol == "AAPL").unwrap();
        assert_eq!(aapl.name, "Apple Inc.");
        assert_eq!(aapl.sector.as_deref(), Some("Technology"));
        assert_eq!(aapl.market_cap, Some(3_400_000_000_000.0));
        assert_eq!(aapl.rank_value, None);
        // marketCap "" -> None, sector "--" -> None
        let empt = entries.iter().find(|e| e.symbol == "EMPT").unwrap();
        assert_eq!(empt.market_cap, None);
        let nosect = entries.iter().find(|e| e.symbol == "NOSECT").unwrap();
        assert_eq!(nosect.sector, None);
    }

    #[test]
    fn parse_nasdaq_err_without_rows() {
        let err = parse_nasdaq(&serde_json::json!({"data": {}})).unwrap_err();
        assert!(err.contains("data.rows"), "{err}");
        let err = parse_nasdaq(&serde_json::json!({})).unwrap_err();
        assert!(err.contains("data.rows"), "{err}");
    }

    /// ตัดจาก Wikipedia "S&P 100" action=parse response — header `<th>` +
    /// 6 แถวข้อมูล `<td>` (link ครอบ cell, `BRK.B` แบบ dot, GOOG/GOOGL
    /// คู่กัน, ชื่อมี `&amp;`, whitespace ติดใน cell)
    const WIKI_SP100_HTML: &str = r#"<div class="mw-parser-output">
<table class="wikitable sortable" id="constituents">
<tbody>
<tr>
<th>Symbol</th>
<th>Name</th>
<th>Sector</th>
</tr>
<tr>
<td><a href="/wiki/Apple_Inc." title="Apple Inc.">AAPL</a>
</td>
<td><a href="/wiki/Apple_Inc." title="Apple Inc.">Apple Inc.</a>
</td>
<td>Information technology</td>
</tr>
<tr>
<td><a href="/wiki/Alphabet_Inc." title="Alphabet Inc.">GOOGL</a>
</td>
<td><a href="/wiki/Alphabet_Inc." title="Alphabet Inc.">Alphabet</a>
</td>
<td>Communication services</td>
</tr>
<tr>
<td>GOOG</td>
<td>Alphabet
</td>
<td>Communication services</td>
</tr>
<tr>
<td><a href="/wiki/Berkshire_Hathaway" title="Berkshire Hathaway">BRK.B</a>
</td>
<td>Berkshire Hathaway
</td>
<td>Financials</td>
</tr>
<tr>
<td><a href="/wiki/AT%26T" title="AT&amp;T">T</a>
</td>
<td>AT&amp;T Inc.
</td>
<td>Communication services</td>
</tr>
<tr>
<td>MCK</td>
<td>McKesson Corporation</td>
<td>Health care
</td>
</tr>
</tbody></table>
</div>"#;

    fn wikipedia_fixture() -> serde_json::Value {
        serde_json::json!({ "parse": { "title": "S&P 100", "text": WIKI_SP100_HTML } })
    }

    #[test]
    fn parse_wikipedia_sp100_extracts_rows_in_table_order() {
        let entries = parse_wikipedia_sp100(&wikipedia_fixture()).unwrap();
        let symbols: Vec<&str> = entries.iter().map(|e| e.symbol.as_str()).collect();
        assert_eq!(symbols, vec!["AAPL", "GOOGL", "GOOG", "BRK.B", "T", "MCK"]);
        let aapl = &entries[0];
        assert_eq!(aapl.name, "Apple Inc.");
        assert_eq!(aapl.sector.as_deref(), Some("Information technology"));
        assert_eq!(aapl.market_cap, None);
        assert_eq!(entries[3].symbol, "BRK.B"); // คง dot-style ตามตาราง
        assert_eq!(entries[4].name, "AT&T Inc."); // &amp; decoded
        assert_eq!(entries[5].sector.as_deref(), Some("Health care")); // trim
        // rank_value เรียงมาก→น้อยตามลำดับแถว — แถวแรก = จำนวนแถว
        let ranks: Vec<f64> = entries.iter().map(|e| e.rank_value.unwrap()).collect();
        assert_eq!(ranks, vec![6.0, 5.0, 4.0, 3.0, 2.0, 1.0]);
    }

    #[test]
    fn parse_wikipedia_sp100_err_on_missing_table_or_rows() {
        let err = parse_wikipedia_sp100(&serde_json::json!({})).unwrap_err();
        assert!(err.contains("parse.text"), "{err}");
        let err = parse_wikipedia_sp100(&serde_json::json!({"parse": {"text": "<p>no table</p>"}}))
            .unwrap_err();
        assert!(err.contains("constituents table not found"), "{err}");
        // มีตารางแต่เหลือแต่ header `<th>` — ไม่มีแถวข้อมูล
        let err = parse_wikipedia_sp100(&serde_json::json!({
            "parse": {"text": "<table id=\"constituents\"><tr><th>Symbol</th><th>Name</th><th>Sector</th></tr></table>"}
        }))
        .unwrap_err();
        assert!(err.contains("no data rows"), "{err}");
    }

    #[test]
    fn top_n_ranks_wikipedia_by_table_order() {
        let entries = parse_wikipedia_sp100(&wikipedia_fixture()).unwrap();
        let top = top_n(&entries, 3);
        let symbols: Vec<&str> = top.iter().map(|e| e.symbol.as_str()).collect();
        assert_eq!(symbols, vec!["AAPL", "GOOGL", "GOOG"]);
        assert!(top.iter().all(|e| e.market_cap.is_none()));
    }

    #[test]
    fn top_n_filters_ranks_and_normalises() {
        let entries = parse_nasdaq(&nasdaq_fixture()).unwrap();
        let top = top_n(&entries, 5);
        let symbols: Vec<&str> = top.iter().map(|e| e.symbol.as_str()).collect();
        // sorted by cap desc; warrant/unit/preferred/ETF/zero/empty dropped;
        // BRK/B -> BRK.B; UAL kept (whole-word "Unit" rule)
        assert_eq!(symbols, vec!["AAPL", "NVDA", "MSFT", "GOOGL", "GOOG"]);
        let top_all = top_n(&entries, usize::MAX);
        assert!(top_all.iter().any(|e| e.symbol == "BRK.B"));
        assert!(top_all.iter().any(|e| e.symbol == "UAL"));
    }

    #[test]
    fn top_n_keeps_share_class_pair_and_drops_instruments() {
        let entries = parse_nasdaq(&nasdaq_fixture()).unwrap();
        let all = top_n(&entries, usize::MAX);
        let syms: Vec<&str> = all.iter().map(|e| e.symbol.as_str()).collect();
        assert!(
            syms.contains(&"GOOGL") && syms.contains(&"GOOG"),
            "{syms:?}"
        );
        for dropped in ["ABCD.W", "EFGH.U", "IJKL", "SPY", "EMPT"] {
            assert!(
                !syms.contains(&dropped),
                "{dropped} should be filtered: {syms:?}"
            );
        }
        // AAPL NVDA MSFT GOOGL GOOG TSLA BRK.B UAL NOSECT = 9 survivors
        assert_eq!(all.len(), 9);
        // ranked desc by market cap
        let caps: Vec<f64> = all.iter().filter_map(|e| e.market_cap).collect();
        let mut sorted = caps.clone();
        sorted.sort_by(|a, b| b.total_cmp(a));
        assert_eq!(caps, sorted);
    }

    #[test]
    fn top_n_dedupes_exact_symbols() {
        let entries = vec![
            UniverseEntry {
                symbol: "AAPL".into(),
                name: "Apple Inc.".into(),
                sector: None,
                market_cap: Some(1e12),
                rank_value: None,
            },
            UniverseEntry {
                symbol: "aapl".into(),
                name: "Apple Inc. duplicate".into(),
                sector: None,
                market_cap: Some(2e12),
                rank_value: None,
            },
            UniverseEntry {
                symbol: "MSFT".into(),
                name: "Microsoft Corporation".into(),
                sector: None,
                market_cap: Some(1.5e12),
                rank_value: None,
            },
        ];
        let top = top_n(&entries, 3);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].symbol, "AAPL"); // uppercase + deduped, higher cap wins
        assert_eq!(top[0].market_cap, Some(2e12));
        assert_eq!(top[1].symbol, "MSFT");
    }

    #[test]
    fn rank_from_cache_borrows_caps_over_table_order() {
        let mut entries = parse_wikipedia_sp100(&wikipedia_fixture()).unwrap();
        let entry =
            |symbol: &str, market_cap: Option<f64>, rank_value: Option<f64>| UniverseEntry {
                symbol: symbol.into(),
                name: String::new(),
                sector: None,
                market_cap,
                rank_value,
            };
        let cache = vec![
            entry("BRK.B", Some(9e12), None),
            entry("aapl", Some(1e10), None), // match แบบ case-insensitive
            entry("GOOGL", Some(5e12), None),
            // cache รอบก่อนเป็น Wikipedia เอง — rank_value ก็ยืมได้
            entry("T", None, Some(900.0)),
        ];
        rank_from_cache(&mut entries, &cache);
        // ตัวที่ไม่มีใน cache คง table order ไว้
        let goog = entries.iter().find(|e| e.symbol == "GOOG").unwrap();
        assert_eq!(goog.rank_value, Some(4.0));
        // cap จาก cache มาก→น้อย ก่อน แล้วเศษที่ไม่มีใน cache ตาม table order
        let top = top_n(&entries, 6);
        let symbols: Vec<&str> = top.iter().map(|e| e.symbol.as_str()).collect();
        assert_eq!(symbols, vec!["BRK.B", "GOOGL", "AAPL", "T", "GOOG", "MCK"]);
    }

    #[test]
    fn normalise_symbol_maps_class_separators() {
        assert_eq!(normalise_symbol("BRK/B"), "BRK.B");
        assert_eq!(normalise_symbol("brk-b"), "BRK.B");
        assert_eq!(normalise_symbol("BF-B"), "BF.B");
        assert_eq!(normalise_symbol("AAPL"), "AAPL");
        assert_eq!(normalise_symbol("goog"), "GOOG");
    }

    fn n_valid_entries(count: usize) -> Vec<UniverseEntry> {
        (0..count)
            .map(|i| UniverseEntry {
                symbol: format!("SYM{i}"),
                name: format!("Company {i}"),
                sector: Some("Technology".into()),
                market_cap: Some(1e12 - i as f64),
                rank_value: None,
            })
            .collect()
    }

    #[test]
    fn choose_uses_primary_when_enough() {
        let now = Utc.with_ymd_and_hms(2026, 3, 2, 14, 0, 0).unwrap();
        let (snap, warnings) = choose(
            Ok(n_valid_entries(10)),
            Ok(n_valid_entries(10)),
            None,
            10,
            now,
        )
        .unwrap();
        assert_eq!(snap.source, UniverseSource::Nasdaq);
        assert_eq!(snap.fetched_at, now);
        assert_eq!(snap.symbols.len(), 10);
        assert!(warnings.is_empty());
    }

    #[test]
    fn choose_falls_back_to_wikipedia_with_warning() {
        let now = Utc.with_ymd_and_hms(2026, 3, 2, 14, 0, 0).unwrap();
        // 5 of 10 < threshold 8 -> fallback
        let (snap, warnings) = choose(
            Ok(n_valid_entries(5)),
            Ok(n_valid_entries(10)),
            None,
            10,
            now,
        )
        .unwrap();
        assert_eq!(snap.source, UniverseSource::Wikipedia);
        assert_eq!(snap.symbols.len(), 10);
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("Wikipedia S&P 100 fallback"),
            "{}",
            warnings[0]
        );
        assert!(warnings[0].contains("nasdaq"), "{}", warnings[0]);
    }

    #[test]
    fn choose_falls_back_to_cache_with_age_warning() {
        let now = Utc.with_ymd_and_hms(2026, 3, 2, 14, 0, 0).unwrap();
        let cache = UniverseSnapshot {
            fetched_at: now - chrono::Duration::days(12),
            source: UniverseSource::Nasdaq,
            symbols: n_valid_entries(10),
        };
        let (snap, warnings) = choose(
            Err("http 403".into()),
            Err("constituents table not found".into()),
            Some(cache),
            10,
            now,
        )
        .unwrap();
        assert_eq!(snap.source, UniverseSource::Cache);
        assert_eq!(snap.fetched_at, now - chrono::Duration::days(12));
        assert_eq!(snap.symbols.len(), 10);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("12 day"), "{}", warnings[0]);
        assert!(warnings[0].contains("nasdaq failed"), "{}", warnings[0]);
        assert!(warnings[0].contains("wikipedia failed"), "{}", warnings[0]);
    }

    #[test]
    fn choose_errors_when_all_fail() {
        let now = Utc.with_ymd_and_hms(2026, 3, 2, 14, 0, 0).unwrap();
        let err = choose(Err("a".into()), Err("b".into()), None, 10, now).unwrap_err();
        assert!(err.contains("universe unavailable"), "{err}");
        assert!(err.contains("a") && err.contains("b"), "{err}");
        // fallback ที่ filter แล้วว่างก็ถือว่า fail -> cache -> Err
        let empty_ok = Ok(Vec::<UniverseEntry>::new());
        let err = choose(empty_ok, Ok(Vec::new()), None, 10, now).unwrap_err();
        assert!(err.contains("universe unavailable"), "{err}");
    }

    #[test]
    fn cache_round_trip_and_missing_file() {
        let dir = std::env::temp_dir().join(format!("radar-universe-tests-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("universe.json");

        assert_eq!(read_cache(&path), None);

        let snap = UniverseSnapshot {
            fetched_at: Utc.with_ymd_and_hms(2026, 3, 2, 9, 30, 0).unwrap(),
            source: UniverseSource::Nasdaq,
            symbols: vec![UniverseEntry {
                symbol: "AAPL".into(),
                name: "Apple Inc.".into(),
                sector: Some("Technology".into()),
                market_cap: Some(3.4e12),
                rank_value: None,
            }],
        };
        write_cache(&path, &snap).unwrap();
        assert_eq!(read_cache(&path), Some(snap.clone()));
        // no leftover tmp file
        assert!(!dir.join("nested").join("universe.json.tmp").exists());

        // contract: NASDAQ entry has exactly the 4 documented keys
        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(raw["source"], "nasdaq");
        assert!(raw["fetched_at"].is_string());
        let mut keys: Vec<&str> = raw["symbols"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["market_cap", "name", "sector", "symbol"]);

        // Wikipedia snapshot: market_cap null + extra rank_value key
        let wiki_snap = UniverseSnapshot {
            fetched_at: snap.fetched_at,
            source: UniverseSource::Wikipedia,
            symbols: vec![UniverseEntry {
                symbol: "AAPL".into(),
                name: "Apple Inc.".into(),
                sector: Some("Information technology".into()),
                market_cap: None,
                rank_value: Some(101.0),
            }],
        };
        write_cache(&path, &wiki_snap).unwrap();
        assert_eq!(read_cache(&path), Some(wiki_snap.clone()));
        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(raw["source"], "wikipedia");

        // legacy cache สมัย fallback ชื่อ Oef ("source": "oef") ยังโหลดได้
        let legacy = format!(
            "{{\"fetched_at\":\"{}\",\"source\":\"oef\",\"symbols\":[]}}",
            snap.fetched_at.to_rfc3339()
        );
        std::fs::write(&path, legacy).unwrap();
        assert_eq!(read_cache(&path).unwrap().source, UniverseSource::Wikipedia);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_liquidity_promotes_next_ranked() {
        let ranked: Vec<UniverseEntry> = ["A", "B", "C", "D", "E"]
            .iter()
            .enumerate()
            .map(|(i, s)| UniverseEntry {
                symbol: s.to_string(),
                name: format!("Company {s}"),
                sector: None,
                market_cap: Some(100.0 - i as f64),
                rank_value: None,
            })
            .collect();
        let picked = apply_liquidity(&ranked, |s| s != "B" && s != "D", 3);
        let symbols: Vec<&str> = picked.iter().map(|e| e.symbol.as_str()).collect();
        assert_eq!(symbols, vec!["A", "C", "E"]);
        assert_eq!(apply_liquidity(&ranked, |_| false, 3).len(), 0);
    }
}
