//! Agent harness foundation: the typed tool contract, the mode-aware
//! registry, and the tool specs the agent exposes to the model.

pub mod contract;
pub mod registry;
pub mod tools;

pub use contract::{Mode, Tier, ToolError, ToolErrorCode, ToolSpec};
pub use registry::Registry;
