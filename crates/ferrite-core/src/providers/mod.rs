//! Provider Sessions: one implementation per agent backend, end-to-end
//! (process + protocol + quirks). No shared transport middle layer unless
//! real duplication demands one.

mod claude;
pub use claude::{
    Capabilities, ClaudeConfig, ClaudeSession, SpawnError, CLAUDE_CLI_MAX_VERSION_EXCLUSIVE,
    CLAUDE_CLI_MIN_VERSION,
};
