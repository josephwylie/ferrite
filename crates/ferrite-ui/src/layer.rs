//! The window's layers over its tree, bottom to top: the priorities its
//! `deferred` draws paint in. A sheet sits under the kit's toasts, so no
//! sheet hides one, and every menu sits over them, so no toast hides a menu
//! the operator opened. Draws of one priority paint in the tree's order.

/// A sheet's card and veil, and the palette's veil: over the board.
pub const SHEET: usize = 3;
/// What a sheet floats over its card (Settings' connection form).
pub const SHEET_ABOVE: usize = 4;
/// The kit's toasts (`Root::render_notification_layer`).
pub const TOAST: usize = 5;
/// Every menu, picker, popup and float: a Composer's menus, a chip's, a
/// transcript row's, the palette, the window's own toasts.
pub const MENU: usize = 6;
/// The lightbox: nothing else answers while it is up.
pub const LIGHTBOX: usize = 7;
