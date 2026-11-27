//! Agent tools — `present` family (W6 P4): `present_idea`, the structured
//! recommendation card. Specs + pure card builder; the Tauri crate reads
//! the contract from the engine and emits the card.

use crate::agent::contract::ToolSpec;

pub fn specs() -> Vec<ToolSpec> {
    Vec::new()
}
