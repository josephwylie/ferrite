//! Ferrite headless core: everything below the GPUI shell.
//!
//! This crate must stay UI-free and headless-testable (ADR-0001: the renderer
//! is swappable at exactly this seam). Modules follow the spec: providers,
//! transcript, store, cockpit — added as their tickets land.

#[cfg(test)]
mod test_temp;

pub mod activity;
pub mod allowance;
pub mod api;
pub mod attachment;
pub mod attachment_transfer;
pub mod changes;
mod attention;
pub mod cadence;
pub mod checkout;
pub mod chunked;
pub mod cited_sources;
pub mod cli_update;
pub mod clock;
pub mod connection;
pub mod delegate;
pub mod diagnostics;
pub mod digest;
pub mod disk;
pub mod docview;
pub mod doorbell;
pub mod draft;
pub mod draft_content;
pub mod drafts;
pub mod drive;
pub mod engine;
pub mod factory;
pub mod factory_board;
pub mod folder;
pub mod followup;
pub mod git_read;
pub mod header;
pub mod import;
pub mod layout;
pub mod link;
pub mod login_shell;
pub mod machine;
pub mod memory;
pub mod models;
pub mod notifications;
pub mod pane_update;
pub mod pr_watch;
pub mod progress;
pub mod project;
pub mod prompt_files;
mod file_change;
pub mod file_operations;
pub mod prompt_history;
pub mod prompt_queue;
pub mod protocol;
pub mod provider;
pub mod providers;
pub mod pull_request;
pub mod questions;
pub mod replies;
pub mod roster;
pub mod runtime_mode;
pub mod self_update;
pub mod server;
pub mod session;
pub mod session_file;
pub mod settings;
pub mod setup_sync;
pub mod shell;
pub mod snooze;
pub mod sound;
pub mod spawn;
pub mod spawner;
pub mod store;
pub mod suggest;
pub mod thread_log;
pub mod titler;
pub mod transcript;
pub mod tree;
pub mod update_plan;
pub mod user_commands;
pub mod visual;
pub mod wake;
pub mod window_frame;
pub mod workspace;
pub mod wrap_up;

mod session_event;
mod thread;
pub use attention::Attention;
pub use session_event::{
    validate_form, CommandSource, ContextCategory, ContextDetails, ContextItem, ControlKind,
    Decision, DecisionAnswer, DecisionChoice, DecisionDelivery, DecisionKind, DecisionPolicy,
    Discovery, FileEdit, FormChoice, FormField, FormFieldKind, Hunk, McpServer, McpStatus,
    ModelInfo, PermissionModeChoice, QueueEvent, QueuedPrompt, RateLimitWindow, RunState,
    SessionCommand, SessionControl, SessionEvent, ToolResult, TurnOutcome, UsageDetails,
    UsageScope,
};
pub use thread::{
    DelegatedFrom, MachineId, MachineKey, ProjectKey, SavedThread, ThreadId, ThreadKey,
    ThreadOwner,
};

/// Conversation-owned website tabs and lifecycle.
pub mod browser;
pub mod browser_annotations;
