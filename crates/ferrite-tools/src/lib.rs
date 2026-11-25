//! The tools behind the Panel's Surfaces (ADR 0039), out of the Cockpit:
//! Changes, the Terminal, and the checkout on another machine that Files
//! reads. Each owns its own state, reads what it draws of the window
//! through a `Source` (`ferrite_ui::source`) and says what the operator
//! asks as an event (ADR 0041). Nothing here names the Cockpit, so an edit
//! to one builds and tests without it. Files itself is still the
//! Cockpit's: it registers its tab stops in the Panel's focus row as it
//! draws (ADR 0041's 2026-11-25 amendment).

pub mod changes;
pub mod remote_files;
pub mod terminal;
