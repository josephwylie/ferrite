//! Ferrite's plain terminal: the user's login shell on a PTY, emulated by
//! `alacritty_terminal`, drawn as a grid by GPUI in the manner of Zed's
//! `terminal_view`.
//!
//! - [`session`]: one shell — its emulator, its PTY event loop, its size —
//!   with no window attached; closing it (dropping it) ends the shell.
//! - [`keys`]: what a keystroke writes to the PTY.
//! - [`links`]: the address a ⌘-press on the grid opens.
//! - [`palette`]: a cell's colours from the theme's inks.
//! - [`view`]: the grid as an entity — the mono face, the block cursor,
//!   scrollback, selection with copy and paste, and resize with its pane.
//!
//! There is no search and there are no splits.

mod keys;
mod links;
mod palette;
mod session;
mod view;

pub use session::Terminal;
pub use view::{Cursor, Settings, TerminalEvent, TerminalView, KEY_CONTEXT};
pub use view::{Attach, Clear, Copy, Enter, Escape, Paste, SelectAll, ShiftTab, Tab};
