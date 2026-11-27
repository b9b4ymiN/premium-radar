//! Executors for the `ideas` tool family (W6 P2).

use serde_json::Value;

use super::ToolCtx;

/// `None` = this family does not own `name`.
pub fn execute(_ctx: &ToolCtx, _name: &str, _args: &Value) -> Option<Result<String, String>> {
    None
}
