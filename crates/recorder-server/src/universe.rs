//! Universe cache — ไฟล์ `<data_dir>/universe.json` ที่ task อื่นเขียน
//! (ดึงรายชื่อจาก nasdaq/oef) เราอ่านเฉพาะ `symbols[].symbol`
//! ไฟล์หาย/พัง ⇒ universe ว่าง + warning (Core ยังเก็บตามปกติ)

use std::path::Path;

use crate::config::normalize_symbols;

/// รูปร่างไฟล์ตามที่เขียนฝั่ง universe task — เราสนใจแค่ symbol
#[derive(serde::Deserialize)]
struct UniverseFile {
    #[serde(default)]
    #[allow(dead_code)]
    fetched_at: String,
    #[serde(default)]
    #[allow(dead_code)]
    source: String,
    #[serde(default)]
    symbols: Vec<UniverseSymbol>,
}

#[derive(serde::Deserialize)]
struct UniverseSymbol {
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    #[allow(dead_code)]
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    sector: String,
    #[serde(default)]
    #[allow(dead_code)]
    market_cap: Option<f64>,
}

/// ผลการอ่าน universe — warning เป็นข้อความให้ log ฝั่ง caller
pub struct UniverseLoad {
    pub symbols: Vec<String>,
    pub warning: Option<String>,
}

/// อ่าน universe.json — ไม่มี network, ไม่ panic
pub fn load_universe(path: &Path) -> UniverseLoad {
    let raw = match std::fs::read_to_string(path) {
        Ok(r) => r,
        Err(_) => {
            return UniverseLoad {
                symbols: Vec::new(),
                warning: Some(format!(
                    "universe: ไม่พบ {} — Layer 1 เก็บเฉพาะ Core symbols",
                    path.display()
                )),
            };
        }
    };
    match serde_json::from_str::<UniverseFile>(&raw) {
        Ok(f) => UniverseLoad {
            symbols: normalize_symbols(
                &f.symbols
                    .iter()
                    .map(|s| s.symbol.clone())
                    .collect::<Vec<_>>(),
            ),
            warning: None,
        },
        Err(e) => UniverseLoad {
            symbols: Vec::new(),
            warning: Some(format!(
                "universe: parse {} ไม่ผ่าน ({e}) — ถือว่าว่าง",
                path.display()
            )),
        },
    }
}

/// union แบบคงลำดับ: core ก่อน แล้ว universe ที่ยังไม่มี (normalize+dedupe)
pub fn merge_symbols(core: &[String], universe: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in normalize_symbols(core)
        .iter()
        .chain(normalize_symbols(universe).iter())
    {
        if !out.contains(s) {
            out.push(s.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("recorder-server-uni-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn valid_file_extracts_symbols_normalized() {
        let dir = tmp_dir("valid");
        let p = dir.join("universe.json");
        std::fs::write(
            &p,
            r#"{
                "fetched_at": "2026-11-24T23:00:00Z",
                "source": "nasdaq",
                "symbols": [
                    {"symbol": "AAPL", "name": "Apple", "sector": "Tech", "market_cap": 3.0},
                    {"symbol": " msft ", "name": "Microsoft", "sector": "Tech"},
                    {"symbol": "BRK.B"},
                    {"symbol": ""}
                ]
            }"#,
        )
        .unwrap();
        let load = load_universe(&p);
        assert!(load.warning.is_none());
        assert_eq!(load.symbols, vec!["AAPL", "MSFT", "BRK.B"]); // trim+upper, ตัดว่าง
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_means_empty_with_warning() {
        let load = load_universe(Path::new("/nonexistent/universe.json"));
        assert!(load.symbols.is_empty());
        assert!(load.warning.as_deref().unwrap_or_default().contains("ไม่พบ"));
    }

    #[test]
    fn malformed_file_means_empty_with_warning() {
        let dir = tmp_dir("bad");
        let p = dir.join("universe.json");
        std::fs::write(&p, "{not json at all").unwrap();
        let load = load_universe(&p);
        assert!(load.symbols.is_empty());
        assert!(load.warning.as_deref().is_some_and(|w| w.contains("parse")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_dedupes_keeping_core_order_first() {
        let core = vec!["NVDA".to_string(), "AAPL".to_string()];
        let universe = vec!["AAPL".to_string(), "TSLA".to_string(), "nvda".to_string()];
        assert_eq!(
            merge_symbols(&core, &universe),
            vec!["NVDA".to_string(), "AAPL".to_string(), "TSLA".to_string()]
        );
        assert_eq!(merge_symbols(&core, &[]), core);
        assert!(merge_symbols(&[], &[]).is_empty());
    }
}
