//! The shortcuts sheet (`?`, FL-8): every key the cockpit binds, read from
//! the key table (`keymap::bindings`) through one label table, in the float
//! grammar at the palette's geometry and over its veil.
//!
//! A head (`shortcuts`, `?` at its right), sections by where a key acts —
//! `threads`, `board`, `composer`, `decision`, `wall`, `transcript` — rows
//! with the label at the left and the chord at the right, and the footer
//! `↑↓ scroll · esc`. The floats' own keys (the palette's, the list's, this
//! sheet's) are in their footers, so the sheet leaves them out; the label
//! table still names them, so a key the table gains without a label fails
//! `every_bound_action_has_a_label`.

use gpui::prelude::*;
use gpui::{
    div, point, px, rgb, Context, FocusHandle, Focusable, IntoElement, ScrollHandle, SharedString,
    Window,
};

use crate::components;
use crate::keymap::{self, Platform};
use crate::theme::*;

gpui::actions!(shortcuts, [Toggle, ScrollUp, ScrollDown, Dismiss]);

/// Where a key acts: the sheet's sections, in the order it draws them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Place {
    Threads,
    Board,
    Composer,
    Decision,
    Wall,
    Transcript,
}

impl Place {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Place::Threads => "threads",
            Place::Board => "board",
            Place::Composer => "composer",
            Place::Decision => "decision",
            Place::Wall => "wall",
            Place::Transcript => "transcript",
        }
    }
}

/// What the label table says about one binding: its row, or that the sheet
/// leaves it out (a float's own key, a second spelling of a row).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Entry {
    Row(Place, &'static str),
    Hidden,
}

/// The label table: one entry for every (action, context) the key table
/// binds. `None` is a binding nobody has named yet — the test fails on it.
pub(crate) fn label(action: &str, context: Option<&str>) -> Option<Entry> {
    use Entry::{Hidden, Row};
    use Place::*;
    let wall = context == Some("Wall");
    Some(match action {
        // Threads.
        "cockpit::NewThread" => Row(Threads, "new thread"),
        "cockpit::NewWorktreeThread" => Row(Threads, "new worktree thread"),
        "cockpit::NewGroup" => Row(Threads, "new group"),
        "palette::OpenGroups" => Row(Threads, "open a group"),
        "palette::Toggle" => Row(Threads, "commands"),
        "cockpit::CloseThread" => Row(Threads, "park thread"),
        "cockpit::ReopenThread" => Row(Threads, "reopen last"),
        "palette::ShowParked" => Row(Threads, "show parked"),
        "palette::CompareWithMain" => Row(Threads, "compare with main"),
        "cockpit::NextDecision" => Row(Threads, "next request"),
        "cockpit::ToggleNotifications" => Row(Threads, "notifications"),
        "cockpit::OpenSettings" => Row(Threads, "settings"),
        "ferrite::Quit" => Row(Threads, "quit"),
        // The board.
        "cockpit::NextPane" => Row(Board, "next pane"),
        "cockpit::PreviousPane" => Row(Board, "previous pane"),
        "cockpit::FocusThread1" => Row(Board, "focus pane 1\u{2013}9"),
        "cockpit::FocusThread2"
        | "cockpit::FocusThread3"
        | "cockpit::FocusThread4"
        | "cockpit::FocusThread5"
        | "cockpit::FocusThread6"
        | "cockpit::FocusThread7"
        | "cockpit::FocusThread8"
        | "cockpit::FocusThread9" => Hidden,
        "cockpit::ToggleFullscreen" => Row(Board, "fullscreen pane"),
        "cockpit::ToggleNav" => Row(Board, "toggle sidebar"),
        "cockpit::TextLarger" => Row(Board, "text larger"),
        "cockpit::TextSmaller" => Row(Board, "text smaller"),
        "cockpit::TextReset" => Row(Board, "text reset"),
        "empty_board::Previous" => Row(Board, "empty board: previous"),
        "empty_board::Next" => Row(Board, "empty board: next"),
        "empty_board::Run" => Row(Board, "empty board: run"),
        // A Decision row (and the wall's quick answers).
        "cockpit::Allow" if wall => Row(Wall, "allow"),
        "cockpit::Deny" if wall => Row(Wall, "deny"),
        "cockpit::Always" if wall => Row(Wall, "always allow"),
        "cockpit::PickOption1" if wall => Row(Wall, "answer 1\u{2013}3"),
        "cockpit::PickOption2" | "cockpit::PickOption3" if wall => Hidden,
        "cockpit::Allow" => Row(Decision, "allow"),
        "cockpit::Deny" => Row(Decision, "deny"),
        "cockpit::Always" => Row(Decision, "always allow"),
        "cockpit::PickOption1" => Row(Decision, "pick 1\u{2013}3"),
        "cockpit::PickOption2" | "cockpit::PickOption3" => Hidden,
        "cockpit::PickOption4" => Row(Decision, "pick 4"),
        "decision::SelectPrevious" => Row(Decision, "previous option"),
        "decision::SelectNext" => Row(Decision, "next option"),
        "decision::Confirm" => Row(Decision, "pick the selected option"),
        "decision::Dismiss" => Row(Decision, "deny"),
        "decision::Amend" => Row(Decision, "amend (a note with the answer)"),
        // The Composer.
        "cockpit::Submit" => Row(Composer, "send"),
        "composer::Newline" => Row(Composer, "new line"),
        "cockpit::Interrupt" => Row(Composer, "interrupt"),
        "status::CycleMode" => Row(Composer, "cycle permission mode"),
        "shortcuts::Toggle" => Row(Composer, "shortcuts"),
        "cockpit::BandCycle" => Row(Composer, "accept suggestion / next chip"),
        "cockpit::HistoryOlder" => Row(Composer, "previous prompt"),
        "cockpit::HistoryNewer" => Row(Composer, "next prompt"),
        "composer::Up" => Row(Composer, "line up"),
        "composer::Down" => Row(Composer, "line down"),
        "composer::Paste" => Row(Composer, "paste"),
        "cockpit::Paste" => Hidden,
        "composer::Backspace" => Row(Composer, "delete back"),
        "composer::Delete" => Row(Composer, "delete forward"),
        "composer::Left" => Row(Composer, "left"),
        "composer::Right" => Row(Composer, "right"),
        "composer::Home" => Row(Composer, "line start"),
        "composer::End" => Row(Composer, "line end"),
        "composer::DeleteWordLeft" => Row(Composer, "delete word back"),
        "composer::DeleteWordRight" => Row(Composer, "delete word forward"),
        "composer::DeleteToStart" => Row(Composer, "delete to line start"),
        "composer::DeleteToEnd" => Row(Composer, "delete to line end"),
        "composer::WordLeft" => Row(Composer, "word left"),
        "composer::WordRight" => Row(Composer, "word right"),
        "composer::SelectLeft" => Row(Composer, "select left"),
        "composer::SelectRight" => Row(Composer, "select right"),
        "composer::SelectWordLeft" => Row(Composer, "select word left"),
        "composer::SelectWordRight" => Row(Composer, "select word right"),
        "composer::SelectHome" => Row(Composer, "select to line start"),
        "composer::SelectEnd" => Row(Composer, "select to line end"),
        "composer::SelectAll" => Row(Composer, "select all"),
        "composer::Copy" => Row(Composer, "copy"),
        "composer::Cut" => Row(Composer, "cut"),
        "composer::Undo" => Row(Composer, "undo"),
        "composer::Redo" => Row(Composer, "redo"),
        "cockpit::MenuPrevious" => Row(Composer, "menu: previous"),
        "cockpit::MenuNext" => Row(Composer, "menu: next"),
        "cockpit::MenuPick" => Row(Composer, "menu: pick"),
        "cockpit::MenuDismiss" => Row(Composer, "menu: close"),
        // The transcript.
        "cockpit::CopySelection" => Row(Transcript, "copy selection"),
        "cockpit::ToggleTool" => Row(Transcript, "open or fold a tool call"),
        "cockpit::ToolCyclePrevious" => Row(Transcript, "previous tool call"),
        // The floats' own keys live in their footers.
        "palette::SelectNext"
        | "palette::SelectPrevious"
        | "palette::Confirm"
        | "palette::Preview"
        | "palette::Dismiss"
        | "notifications::SelectNext"
        | "notifications::SelectPrevious"
        | "notifications::Open"
        | "notifications::Dismiss"
        | "notifications::Close"
        | "shortcuts::ScrollUp"
        | "shortcuts::ScrollDown"
        | "shortcuts::Dismiss"
        | "zed::NoAction" => Hidden,
        _ => return None,
    })
}

/// One drawn row: where it acts, its label, its chord as `key_combo` draws
/// it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SheetRow {
    pub place: Place,
    pub label: &'static str,
    pub keys: String,
}

/// The sheet's rows for `platform`, in section order, each action once (its
/// first binding in a place wins: `⌘N` before `⌘T`). The pane digits and
/// the answer digits read as one row each (`⌘1…9`, `1…3`).
pub(crate) fn rows(platform: Platform) -> Vec<SheetRow> {
    let mut rows: Vec<SheetRow> = Vec::new();
    for (keys, action, context) in keymap::bindings(platform) {
        let Some(Entry::Row(place, label)) = label(action, context) else {
            continue;
        };
        if rows
            .iter()
            .any(|row| row.place == place && row.label == label)
        {
            continue;
        }
        let mut keys = components::spell_chord(&keys);
        if matches!(action, "cockpit::FocusThread1") {
            keys.push_str("\u{2026}9");
        } else if matches!(action, "cockpit::PickOption1") {
            keys.push_str("\u{2026}3");
        }
        rows.push(SheetRow { place, label, keys });
    }
    rows.sort_by_key(|row| row.place);
    rows
}

/// The sheet's footer, word for word.
pub(crate) const FOOTER: &str = "\u{2191}\u{2193} scroll \u{b7} esc";

/// The sheet: its rows, its scroll and the focus its keys ride
/// (`Shortcuts` context).
pub(crate) struct Shortcuts {
    rows: Vec<SheetRow>,
    scroll: ScrollHandle,
    focus: FocusHandle,
}

impl Shortcuts {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            rows: rows(keymap::PLATFORM),
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
        }
    }

    /// ↑ / ↓: a row at a time, held inside the list.
    pub(crate) fn scroll_by(&mut self, rows: f32, cx: &mut Context<Self>) {
        let offset = self.scroll.offset();
        let max = f32::from(self.scroll.max_offset().y).max(0.0);
        let y = (f32::from(offset.y) - rows * ROW).clamp(-max, 0.0);
        self.scroll.set_offset(point(offset.x, px(y)));
        cx.notify();
    }
}

impl Focusable for Shortcuts {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Shortcuts {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = div()
            .id("shortcuts-rows")
            .flex()
            .flex_col()
            .min_h_0()
            .max_h(px(PALETTE_MAX_H))
            .overflow_y_scroll()
            .track_scroll(&self.scroll);
        let mut place = None;
        for (index, row) in self.rows.iter().enumerate() {
            if place != Some(row.place) {
                place = Some(row.place);
                list = list.child(crate::menu::section(row.place.title(), None, None));
            }
            list = list.child(
                components::text_ui()
                    .debug_selector(move || format!("shortcut-row-{index}"))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .h(px(FLOAT_ROW_H))
                    .pl(px(FLOAT_PAD_X + FLOAT_GUTTER))
                    .pr(px(FLOAT_PAD_X))
                    .whitespace_nowrap()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(TEXT))
                            .child(SharedString::from(row.label)),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .ml(px(FLOAT_DETAIL_GAP))
                            .child(components::key_combo(&row.keys, TEXT_MUTED)),
                    ),
            );
        }
        crate::menu::float()
            .debug_selector(|| "shortcuts-sheet".into())
            .w(px(PALETTE_W))
            .track_focus(&self.focus)
            .key_context("Shortcuts")
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(crate::menu::head("shortcuts").child("?"))
            .child(list)
            .child(crate::menu::footer_line(FOOTER))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key the table binds has a label (or is named hidden), on both
    /// platforms: a new binding without one fails here.
    #[test]
    fn every_bound_action_has_a_label() {
        for platform in [Platform::Mac, Platform::Windows] {
            for (keys, action, context) in keymap::bindings(platform) {
                assert!(
                    label(action, context).is_some(),
                    "{platform:?}: {keys} → {action} ({context:?}) has no label"
                );
            }
        }
    }

    /// The sheet reads the table: its sections in order, each action once,
    /// the digits gathered, the Decision and wall keys in their own places,
    /// the editing keys included.
    #[test]
    fn the_sheet_lists_the_table_by_place() {
        let rows = rows(Platform::Mac);
        let find = |place: Place, label: &str| {
            rows.iter()
                .find(|row| row.place == place && row.label == label)
                .unwrap_or_else(|| panic!("{place:?} {label} is missing"))
                .keys
                .clone()
        };
        assert_eq!(find(Place::Threads, "commands"), "cmd-K");
        assert_eq!(find(Place::Threads, "new thread"), "cmd-N");
        assert_eq!(
            find(Place::Threads, "park thread"),
            "cmd-\u{232b}",
            "the empty line's ⌘⌫ is the first spelling"
        );
        assert_eq!(
            find(Place::Board, "focus pane 1\u{2013}9"),
            "cmd-1\u{2026}9"
        );
        assert_eq!(find(Place::Decision, "allow"), "Y");
        assert_eq!(find(Place::Decision, "pick 1\u{2013}3"), "1\u{2026}3");
        assert_eq!(
            find(Place::Decision, "amend (a note with the answer)"),
            "\u{21e5}"
        );
        assert_eq!(find(Place::Wall, "answer 1\u{2013}3"), "1\u{2026}3");
        assert_eq!(
            find(Place::Composer, "cycle permission mode"),
            "shift-\u{21e5}"
        );
        assert_eq!(find(Place::Composer, "shortcuts"), "?");
        assert_eq!(find(Place::Composer, "delete word back"), "alt-\u{232b}");
        assert_eq!(find(Place::Transcript, "copy selection"), "cmd-C");
        let places: Vec<Place> = rows.iter().map(|row| row.place).collect();
        let mut sorted = places.clone();
        sorted.sort();
        assert_eq!(places, sorted, "sections in order");
        assert!(rows
            .iter()
            .all(|row| !row.label.is_empty() && !row.keys.is_empty()));
    }
}
