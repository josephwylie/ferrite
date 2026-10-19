//! Ferrite headless core: everything below the GPUI shell.
//!
//! This crate must stay UI-free and headless-testable (ADR-0001: the renderer
//! is swappable at exactly this seam). Modules follow the spec: providers,
//! transcript, store, cockpit — added as their tickets land.

pub mod activity;
pub mod api;
mod attention;
pub mod cadence;
pub mod clock;
pub mod cockpit;
pub mod diagnostics;
pub mod docview;
pub mod draft;
pub mod engine;
pub mod followup;
pub mod groups;
pub mod import;
pub mod layout;
pub mod link;
pub mod login_shell;
pub mod machine;
pub mod notifications;
pub mod progress;
pub mod prompt_files;
mod prompt_history;
pub mod protocol;
pub mod providers;
pub mod questions;
pub mod roster;
pub mod self_update;
pub mod server;
pub mod session;
pub mod settings;
pub mod sound;
pub mod spawn;
pub mod spawner;
pub mod store;
pub mod suggest;
pub mod titler;
pub mod transcript;
pub mod user_commands;
pub mod visual;
pub mod workspace;

mod session_event;
mod thread;
pub use attention::Attention;
pub use session_event::{
    validate_form, ContextCategory, ContextDetails, ControlKind, Decision, DecisionAnswer,
    DecisionChoice, DecisionDelivery, DecisionKind, DecisionPolicy, FileEdit, FormChoice,
    FormField, FormFieldKind, Hunk, McpServer, McpStatus, ModelInfo, PermissionModeChoice,
    QueueEvent, QueuedPrompt, RateLimitWindow, RunState, SessionCommand, SessionControl,
    SessionEvent, ToolResult, TurnOutcome, UsageDetails, UsageScope,
};
pub use thread::ThreadId;
