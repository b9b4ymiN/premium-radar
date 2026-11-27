//! ToolSpec definitions grouped by domain. Each family exposes `specs()`
//! plus the pure summarisers that turn domain data into the compact JSON a
//! tool returns to the model (fetching lives in the Tauri crate).

pub mod ideas;
pub mod market;
pub mod mine;
pub mod playbook;
pub mod present;
pub mod screener;
pub mod ui;

pub use screener::screener_specs;
