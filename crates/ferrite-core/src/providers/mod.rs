//! Provider Sessions: one implementation per agent backend, end-to-end
//! (process + protocol + quirks). No shared transport middle layer unless
//! real duplication demands one.

mod claude;
mod codex;
pub use claude::{
    ClaudeCapabilities, ClaudeConfig, ClaudeSession, ClaudeSpawnError, CLAUDE_CLI_MAX_VERSION_EXCLUSIVE,
    CLAUDE_CLI_MIN_VERSION,
};
pub use codex::{
    CodexCapabilities, CodexConfig, CodexSession, CodexSpawnError, CODEX_CLI_MAX_VERSION_EXCLUSIVE,
    CODEX_CLI_MIN_VERSION,
};
