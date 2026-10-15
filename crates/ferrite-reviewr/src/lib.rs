//! herdr-reviewr's review core, vendored for Ferrite's Review slot (README.md): its git scopes,
//! diff model, comment model and formatting, `fff-search` search, and read-only PR reads. Its
//! terminal UI, theming and herdr adapter are dropped; [`ferrite`] is Ferrite's adapter.

// The crate-private helpers only the dropped modules called stay, so upstream merges apply clean.
#![allow(dead_code)]

// First, so every module below has `logln!`.
#[macro_use]
pub mod log;
pub mod azure_devops;
pub mod config;
pub mod diff;
pub mod export;
pub mod ferrite;
pub mod file_list;
pub mod forge;
pub mod git;
pub mod gitlab;
pub mod highlight;
pub mod keymap;
pub mod model;
pub mod proc;
pub mod search;
pub mod snippet;
#[cfg(test)]
mod test_support;
mod text;
pub mod theme;
pub mod turn;
pub mod wake;
mod watch;
