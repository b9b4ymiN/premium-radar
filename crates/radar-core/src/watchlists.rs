//! Watchlist model (PLAN-APP2 P16.1) — pure CRUD/validation, persistence อยู่ shell

use serde::{Deserialize, Serialize};

/// ลิสต์หนึ่งชุด — id stable, ห้ามซ้ำ symbol
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Watchlist {
    pub id: String,
    pub name: String,
    pub symbols: Vec<String>,
}

/// สร้าง id จากเวลา (nanos) — พอสำหรับ local single-user
pub fn new_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("wl-{now}")
}

/// รูปแบบ symbol อเมริกันอย่างหยาบ (1–6 ตัวอักษร A-Z) — validation จริง
/// เช็คกับ instrument API ตอน add (P16.2)
pub fn normalize_symbol(input: &str) -> Option<String> {
    let s = input.trim().to_ascii_uppercase();
    (1..=6)
        .contains(&s.len())
        .then_some(s)
        .filter(|s| s.chars().all(|c| c.is_ascii_alphabetic()))
}

#[derive(Debug, Clone, PartialEq)]
pub enum WatchlistError {
    DuplicateList,
    ListNotFound,
    DuplicateSymbol,
    InvalidSymbol,
    EmptyName,
}

impl WatchlistError {
    pub fn reason_th(&self) -> String {
        match self {
            Self::DuplicateList => "มีลิสต์ชื่อนี้อยู่แล้ว".into(),
            Self::ListNotFound => "ไม่พบลิสต์นี้".into(),
            Self::DuplicateSymbol => "หุ้นตัวนี้อยู่ในลิสต์แล้ว".into(),
            Self::InvalidSymbol => "รูปแบบสัญลักษณ์หุ้นไม่ถูกต้อง (1–6 ตัวอักษร A–Z)".into(),
            Self::EmptyName => "ชื่อลิสต์ห้ามว่าง".into(),
        }
    }
}

/// เพิ่ม symbol (dedupe + normalize)
pub fn add_symbol(list: &mut Watchlist, input: &str) -> Result<(), WatchlistError> {
    let sym = normalize_symbol(input).ok_or(WatchlistError::InvalidSymbol)?;
    if list.symbols.contains(&sym) {
        return Err(WatchlistError::DuplicateSymbol);
    }
    list.symbols.push(sym);
    Ok(())
}

pub fn remove_symbol(list: &mut Watchlist, sym: &str) {
    list.symbols.retain(|s| s != sym);
}

/// ย้ายขึ้น/ลง 1 ตำแหน่ง (P16.2: reorder ด้วย keyboard ได้)
pub fn move_symbol(list: &mut Watchlist, sym: &str, up: bool) {
    if let Some(i) = list.symbols.iter().position(|s| s == sym) {
        let j = if up {
            i.saturating_sub(1)
        } else {
            (i + 1).min(list.symbols.len() - 1)
        };
        list.symbols.swap(i, j);
    }
}

/// ชุดลิสต์ทั้งหมด + ชื่อ default
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WatchlistStore {
    pub lists: Vec<Watchlist>,
    /// id ของลิสต์ที่กำลังใช้
    pub active: Option<String>,
}

impl WatchlistStore {
    pub fn with_default(first_symbol: &str) -> Self {
        let id = new_id();
        let sym = normalize_symbol(first_symbol).unwrap_or_else(|| "NVDA".into());
        Self {
            lists: vec![Watchlist {
                id: id.clone(),
                name: "รายการหลัก".into(),
                symbols: vec![sym],
            }],
            active: Some(id),
        }
    }

    pub fn active_list(&self) -> Option<&Watchlist> {
        self.lists
            .iter()
            .find(|l| Some(&l.id) == self.active.as_ref())
    }

    pub fn create_list(&mut self, name: &str) -> Result<String, WatchlistError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(WatchlistError::EmptyName);
        }
        if self.lists.iter().any(|l| l.name == name) {
            return Err(WatchlistError::DuplicateList);
        }
        let id = new_id();
        self.lists.push(Watchlist {
            id: id.clone(),
            name: name.into(),
            symbols: Vec::new(),
        });
        Ok(id)
    }

    pub fn delete_list(&mut self, id: &str) {
        self.lists.retain(|l| l.id != id);
        if self.active.as_deref() == Some(id) {
            self.active = self.lists.first().map(|l| l.id.clone());
        }
    }

    pub fn set_active(&mut self, id: &str) -> Result<(), WatchlistError> {
        if self.lists.iter().any(|l| l.id == id) {
            self.active = Some(id.into());
            Ok(())
        } else {
            Err(WatchlistError::ListNotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_normalizes_dedupes_and_rejects_bad() {
        let mut l = Watchlist {
            id: "a".into(),
            name: "x".into(),
            symbols: Vec::new(),
        };
        add_symbol(&mut l, " nvda ").unwrap();
        assert_eq!(l.symbols, vec!["NVDA"]);
        assert!(matches!(
            add_symbol(&mut l, "NVDA"),
            Err(WatchlistError::DuplicateSymbol)
        ));
        assert!(matches!(
            add_symbol(&mut l, "TOOLONG1"),
            Err(WatchlistError::InvalidSymbol)
        ));
        assert!(matches!(
            add_symbol(&mut l, "A1"),
            Err(WatchlistError::InvalidSymbol)
        ));
    }

    #[test]
    fn move_and_remove() {
        let mut l = Watchlist {
            id: "a".into(),
            name: "x".into(),
            symbols: vec!["A".into(), "B".into(), "C".into()],
        };
        move_symbol(&mut l, "C", true);
        assert_eq!(l.symbols, vec!["A", "C", "B"]);
        move_symbol(&mut l, "A", true); // อยู่บนสุดแล้ว — ไม่ขยับ
        assert_eq!(l.symbols, vec!["A", "C", "B"]);
        remove_symbol(&mut l, "C");
        assert_eq!(l.symbols, vec!["A", "B"]);
    }

    #[test]
    fn store_crud_and_active_fallback() {
        let mut s = WatchlistStore::with_default("spy");
        assert_eq!(s.active_list().unwrap().symbols, vec!["SPY"]);
        let id2 = s.create_list("เทรดสั้น").unwrap();
        s.set_active(&id2).unwrap();
        s.delete_list(&id2);
        // ลิสต์ active ถูกลบ → fallback ลิสต์แรก ไม่ dangle
        assert_eq!(s.active.as_deref(), Some(s.lists[0].id.as_str()));
        assert!(matches!(
            s.create_list("รายการหลัก"),
            Err(WatchlistError::DuplicateList)
        ));
    }
}
