//! The nav (ADR 0040) and what stands beside it, out of the Cockpit: the
//! empty board. Each owns its own state, reads what it draws of the window
//! through a `Source` (`ferrite_ui::source`) and says what the operator
//! asks as an event (ADR 0041). Nothing here names the Cockpit, so an edit
//! to one builds and tests without it.

pub mod empty_board;
