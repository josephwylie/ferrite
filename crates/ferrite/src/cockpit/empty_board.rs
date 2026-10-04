//! The empty board's key list (`new thread ⌘N`, `open a group ⌘G`, …).
//!
//! Phase 0 (shared interface; the frame package owns this module): the
//! actions the key table binds in the `EmptyBoard` context, so the table
//! builds before the board that answers them lands.

gpui::actions!(empty_board, [Next, Previous, Run]);
