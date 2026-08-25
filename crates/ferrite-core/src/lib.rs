//! Ferrite headless core: everything below the GPUI shell.
//!
//! This crate must stay UI-free and headless-testable (ADR-0001: the renderer
//! is swappable at exactly this seam). Modules follow the spec: providers,
//! transcript, store, cockpit — added as their tickets land.

pub mod cockpit;
pub mod docview;
pub mod import;
pub mod providers;
pub mod store;
pub mod transcript;
pub mod workspace;

mod session_event;
mod thread;
pub use session_event::{Decision, DecisionAnswer, Hunk, SessionEvent, ToolResult, TurnOutcome};
pub use thread::ThreadId;
