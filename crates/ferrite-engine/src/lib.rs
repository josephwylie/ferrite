//! Ferrite's Engine: the Store, the providers and their Sessions, the
//! Engine, the Factory, workspaces and the API. A Server runs it; a window
//! never links it (ADR 0049). The types both sides share are
//! `ferrite-core`'s.

#[cfg(test)]
#[path = "../../ferrite-core/src/test_temp.rs"]
mod test_temp;

pub mod api;
pub mod connection;
pub mod engine;
pub mod factory;
pub mod github;
pub mod import;
pub mod pr_watch;
pub mod providers;
pub mod session;
pub mod spawner;
pub mod store;
pub mod suggest;
pub mod titler;
pub mod workspace;
