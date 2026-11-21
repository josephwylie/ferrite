//! Every ferrite-core integration test, linked as one program. Cargo links
//! each file directly under `tests/` as its own program on every edit.

#[path = "../../../ferrite-core/src/test_temp.rs"]
mod test_temp;

#[cfg(unix)]
mod support;

mod activity_memory;
mod activity;
mod background_handback;
mod browser_annotations;
mod browser_conversations;
mod claude_session;
mod claude_subagents;
mod codex_decision_ids;
mod codex_session;
mod codex_subagent_interrupt;
mod codex_subagents;
mod codex_writer_conflict;
mod contract_catalogs;
mod contract_claude_menu;
mod contract_codex_controls;
mod contract_controls;
mod contract_decisions;
mod contract_diffs;
mod contract_fidelity;
mod contract_file_search;
mod contract_forms;
mod contract_media_inputs;
mod contract_native_controls;
mod contract_plans;
mod contract_rpc;
mod contract_session_discovery;
mod contract_sessions;
mod contract_state;
mod contract_tools;
mod contract_usage;
mod crash_report;
mod edit_drafts;
mod import;
mod live_claude;
mod live_codex;
mod live_import;
mod live_plans;
mod live_progress;
mod notifications;
mod progress_lifecycle;
mod provider_progress;
mod steer_fold;
mod store;
mod subagent_cockpit;
mod visuals;
mod windows_session_tree;
mod wrap_up;
