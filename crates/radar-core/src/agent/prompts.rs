//! System prompts per UI mode (W6 P4). Until P4-T2 lands this forwards to
//! the legacy copilot prompt.

use crate::agent::contract::Mode;

pub fn system_prompt(_mode: Mode) -> String {
    crate::copilot::system_prompt()
}
