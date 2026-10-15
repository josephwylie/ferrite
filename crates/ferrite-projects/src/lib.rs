//! Ferrite: the crate is a library as well as the binary (ADR 0030, Seam 3),
//! so Ferrite's window reads the same data the binary's commands do.

pub mod adopt;
pub mod agents;
pub mod assign;
pub mod brief;
pub mod cli;
pub mod coordinator;
pub mod doctor;
pub mod grouping;
pub mod herdr;
pub mod inbox;
pub mod keys;
pub mod lifecycle;
pub mod names;
pub mod notify;
pub mod overview;
pub mod paths;
pub mod popup;
pub mod pr;
pub mod profiles;
pub mod progress;
pub mod prompt_box;
pub mod project;
pub mod remote;
pub mod rename;
pub mod routine;
pub mod runner;
pub mod safety;
#[cfg(test)]
mod scenarios;
pub mod settings;
pub mod setup;
pub mod sidebar;
pub mod spaces;
pub mod steps;
pub mod sweep;
pub mod tasks;
pub mod thread;
pub mod threads;
pub mod ticker;
pub mod trust_screen;

/// Crate version plus a build identifier (short git hash and build time), so a
/// rebuilt binary always differs from the one a running ticker was started from.
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("HP_BUILD_ID"));
