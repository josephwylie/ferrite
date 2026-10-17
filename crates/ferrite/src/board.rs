//! The work board (ADR 0031, Seam 3): herdr-projects 1.0's board, its
//! vendored state, `update` and view driven from a GPUI view in place of
//! its terminal runtime (`board/run/`). `runtime` owns the board's `State`
//! and feeds it keys, the beat, the loader's reads and the jobs' answers;
//! `keys` says a keystroke as the board names keys; `render` draws the
//! lines `view::screen` lays out on Ferrite's cell grid, an ink per named
//! ink. The view (`board/view/`) stays the authority on what each part
//! says and what gives way when room is short. `mouse` turns GPUI's mouse
//! into the board's wheel and clicks and copies what a drag selects;
//! `links` opens the addresses the view marks; `machines` says when other
//! machines are asked for their tasks.
//!
//! Where it shows is `board.open`: `tab`, the board in the board's place,
//! kept while you are on a Thread; `popup`, a sheet over the board (90% of
//! it; all of it under 110×30 cells) that closes when you open an agent.
//! Ferrite's `projects::Toggle` is herdr's `prefix+a`: it opens the board,
//! and pressed on it, takes you back to where you came from.

mod aim;
#[cfg(any(test, feature = "visual-reference"))]
pub(crate) mod fixture;
mod keys;
mod links;
mod machines;
mod mouse;
#[cfg(test)]
mod panels_tests;
pub(crate) mod render;
mod runtime;
#[cfg(test)]
mod tests;

pub(crate) use aim::Target;
#[cfg(any(test, feature = "visual-reference"))]
pub(crate) use runtime::Board;
pub(crate) use runtime::{BoardEvent, BoardView, Opening};

/// Where `board.open` puts the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Placement {
    /// In the board's place, kept while you are elsewhere.
    Tab,
    /// A sheet over the board, gone once closed.
    Popup,
}

impl Placement {
    /// `board.open` as set (`tab` unless it says `popup`).
    pub(crate) fn read() -> Placement {
        let open = sw_board::store::settings::lookup::setting("board.open", None);
        if open.as_str() == Some("popup") {
            Placement::Popup
        } else {
            Placement::Tab
        }
    }
}

/// Under this many cells (columns, lines) the popup takes the whole board,
/// as upstream's does a whole pane: a margin would cost half the board.
pub(crate) const POPUP_FULL_BELOW: (f32, f32) = (110.0, 30.0);
/// The share of the board the popup covers otherwise.
pub(crate) const POPUP_SHARE: f32 = 0.9;

/// The popup's size on a board `width` by `height` pixels.
pub(crate) fn popup_size(width: f32, height: f32) -> (f32, f32) {
    let (columns, rows) = POPUP_FULL_BELOW;
    if width < columns * crate::theme::CH || height < rows * crate::theme::ROW {
        (width, height)
    } else {
        (width * POPUP_SHARE, height * POPUP_SHARE)
    }
}

/// The key that opens the board as the board writes keys (`cmd+shift+a`),
/// for its "brings you back to the board" lines.
pub(crate) fn board_key() -> String {
    crate::keymap::active()
        .into_iter()
        .find(|(_, action, context)| *action == "projects::Toggle" && context.is_none())
        .map(|(keys, _, _)| keys.replace('-', "+"))
        .unwrap_or_default()
}
