//! The Factory's views (ADR 0037), out of the Cockpit: each owns its own
//! state, reads what it draws of the window through a `Source`
//! (`ferrite_ui::source`) and says what the operator asks as an event (ADR
//! 0041). Nothing here names the Cockpit, so an edit to one builds and
//! tests without it.

pub mod archive;
pub mod new_task;
pub mod picker;
