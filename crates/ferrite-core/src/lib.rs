//! Ferrite headless core: everything below the GPUI shell.
//!
//! This crate must stay UI-free and headless-testable (ADR-0001: the renderer
//! is swappable at exactly this seam). Modules follow the spec: providers,
//! transcript, store, cockpit — added as their tickets land.

pub mod cockpit;
pub mod providers;
pub mod transcript;

mod session_event;
pub use session_event::{Decision, DecisionAnswer, Hunk, SessionEvent, ToolResult, TurnOutcome};
