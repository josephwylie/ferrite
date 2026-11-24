//! Every ferrite-client integration test, linked as one program. Cargo links
//! each file directly under `tests/` as its own program on every edit.

#[path = "../../../ferrite-core/src/test_temp.rs"]
mod test_temp;

mod contract_controls;
mod notifications;
mod subagent_cockpit;
