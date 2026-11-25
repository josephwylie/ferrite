//! The tools behind three of the Panel's Surfaces (ADR 0039), out of the
//! Cockpit: Files, Changes and the Terminal. Each owns its own state, reads
//! what it draws of the window through a `Source` (`ferrite_ui::source`)
//! and says what the operator asks as an event (ADR 0041). Nothing here
//! names the Cockpit, so an edit to one builds and tests without it.

pub mod remote_files;
