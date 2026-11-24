//! Every ferrite-core integration test, linked as one program. Cargo links
//! each file directly under `tests/` as its own program on every edit.

#[path = "../../../ferrite-core/src/test_temp.rs"]
mod test_temp;

mod activity_memory;
mod activity;
mod crash_report;
mod edit_drafts;
mod steer_fold;
mod wrap_up;
