//! Agent harness foundation: the typed tool contract, the mode-aware
//! registry, and the tool specs the agent exposes to the model.

pub mod contract;
pub mod registry;
pub mod runner;
pub mod tools;

pub use contract::{Mode, Tier, ToolError, ToolErrorCode, ToolSpec};
pub use registry::Registry;
pub use runner::{AgentEvent, AgentOutcome, AgentRequest, RunLimits, ToolExecutor};

#[cfg(feature = "client")]
pub use runner::run_agent;
