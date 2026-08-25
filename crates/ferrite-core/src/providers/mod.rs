//! Provider Sessions: one implementation per agent backend, end-to-end
//! (process + protocol + quirks). No shared transport middle layer unless
//! real duplication demands one.

mod claude;
pub use claude::{ClaudeConfig, ClaudeSession, SpawnError, CLAUDE_CLI_MIN_VERSION};
