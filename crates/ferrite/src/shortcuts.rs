//! The shortcuts sheet (`?`, FL-8): every key the cockpit binds, read from
//! the bound key table (`keymap::active`, the operator's keys included)
//! through one label table, in the palette's dialog (`palette::dialog`)
//! and over its veil.
//!
//! The palette's input row (the `search` icon, the filter, `esc`), its
//! section headings by where a key acts — `Threads`, `Board`, `Composer`,
//! `Decision`, `Wall`, `Transcript` — rows with the label at the left and
//! the chord at the right, and the palette's footer, `↑↓ scroll · esc`. Typing keeps the rows whose label, section or chord
//! match, as the palette filters. The floats' own keys (the palette's, the
//! list's, this sheet's) and the Review slot's are in their footers, so the
//! sheet leaves them out; the label table still names them, so a key the
//! table gains without a label fails `every_bound_action_has_a_label`.

use std::ops::Range;

use gpui::prelude::*;
use gpui::{
    div, point, px, rgb, Context, Entity, IntoElement, ScrollHandle, StyledText, Subscription,
    Window,
};

use crate::components;
use crate::composer::{Composer, Edited, Role};
use crate::keymap::Binding;
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
            Place::Threads => "Threads",
            Place::Board => "Board",
            Place::Composer => "Composer",
            Place::Decision => "Decision",
            Place::Wall => "Wall",
            Place::Transcript => "Transcript",
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
        "cockpit::NewThread" => Row(Threads, "New thread"),
        "cockpit::NewWorktreeThread" => Row(Threads, "New worktree thread"),
        "cockpit::NewGroup" => Row(Threads, "New group"),
        "palette::OpenGroups" => Row(Threads, "Open a group"),
        "palette::Toggle" => Row(Threads, "Commands"),
        "projects::Toggle" => Row(Threads, "Tasks"),
        "cockpit::CloseThread" => Row(Threads, "Park thread"),
        "cockpit::ReopenThread" => Row(Threads, "Reopen last"),
        "palette::ShowParked" => Row(Threads, "Show parked"),
        "palette::ReviewBranch" => Row(Threads, "Review branch"),
        "cockpit::NextDecision" => Row(Threads, "Next request"),
        "cockpit::ToggleNotifications" => Row(Threads, "Needs you"),
        "cockpit::OpenSettings" => Row(Threads, "Settings"),
        "cockpit::RefreshPrStatus" => Row(Threads, "Refresh PR status"),
        "cockpit::OpenPrInBrowser" => Row(Threads, "Open PR in browser"),
        "ferrite::Quit" => Row(Threads, "Quit"),
        // The board.
        "cockpit::NextPane" => Row(Board, "Next pane"),
        "cockpit::PreviousPane" => Row(Board, "Previous pane"),
        "cockpit::FocusThread1" => Row(Board, "Focus pane 1\u{2013}9"),
        "cockpit::FocusThread2"
        | "cockpit::FocusThread3"
        | "cockpit::FocusThread4"
        | "cockpit::FocusThread5"
        | "cockpit::FocusThread6"
        | "cockpit::FocusThread7"
        | "cockpit::FocusThread8"
        | "cockpit::FocusThread9" => Hidden,
        "cockpit::ToggleFullscreen" => Row(Board, "Fullscreen pane"),
        "cockpit::ToggleNav" => Row(Board, "Toggle sidebar"),
        "cockpit::TogglePanel" => Row(Board, "Toggle panel"),
        "cockpit::TextLarger" => Row(Board, "Text larger"),
        "cockpit::TextSmaller" => Row(Board, "Text smaller"),
        "cockpit::TextReset" => Row(Board, "Text reset"),
        "empty_board::Previous" => Row(Board, "Empty board: previous"),
        "empty_board::Next" => Row(Board, "Empty board: next"),
        "empty_board::Run" => Row(Board, "Empty board: run"),
        // A Decision row (and the wall's quick answers).
        "cockpit::Allow" if wall => Row(Wall, "Allow"),
        "cockpit::Deny" if wall => Row(Wall, "Deny"),
        "cockpit::Always" if wall => Row(Wall, "Always allow"),
        "cockpit::PickOption1" if wall => Row(Wall, "Answer 1\u{2013}3"),
        "cockpit::PickOption2" | "cockpit::PickOption3" if wall => Hidden,
        "cockpit::Allow" => Row(Decision, "Allow"),
        "cockpit::Deny" => Row(Decision, "Deny"),
        "cockpit::Always" => Row(Decision, "Always allow"),
        "cockpit::PickOption1" => Row(Decision, "Pick 1\u{2013}3"),
        "cockpit::PickOption2" | "cockpit::PickOption3" => Hidden,
        "cockpit::PickOption4" => Row(Decision, "Pick 4"),
        "decision::SelectPrevious" => Row(Decision, "Previous option"),
        "decision::SelectNext" => Row(Decision, "Next option"),
        "decision::Confirm" => Row(Decision, "Pick the selected option"),
        "decision::Dismiss" => Row(Decision, "Deny"),
        "decision::Amend" => Row(Decision, "Amend (a note with the answer)"),
        // The Composer.
        "cockpit::Submit" => Row(Composer, "Send"),
        "composer::Newline" => Row(Composer, "New line"),
        "cockpit::Interrupt" => Row(Composer, "Interrupt"),
        "status::CycleMode" => Row(Composer, "Cycle permission mode"),
        "shortcuts::Toggle" => Row(Composer, "Shortcuts"),
        "cockpit::BandCycle" => Row(Composer, "Accept suggestion / next chip"),
        "cockpit::HistoryOlder" => Row(Composer, "Previous prompt"),
        "cockpit::HistoryNewer" => Row(Composer, "Next prompt"),
        "composer::Up" => Row(Composer, "Line up"),
        "composer::Down" => Row(Composer, "Line down"),
        "composer::Paste" => Row(Composer, "Paste"),
        "cockpit::Paste" => Hidden,
        "composer::Backspace" => Row(Composer, "Delete back"),
        "composer::Delete" => Row(Composer, "Delete forward"),
        "composer::Left" => Row(Composer, "Left"),
        "composer::Right" => Row(Composer, "Right"),
        "composer::Home" => Row(Composer, "Line start"),
        "composer::End" => Row(Composer, "Line end"),
        "composer::DeleteWordLeft" => Row(Composer, "Delete word back"),
        "composer::DeleteWordRight" => Row(Composer, "Delete word forward"),
        "composer::DeleteToStart" => Row(Composer, "Delete to line start"),
        "composer::DeleteToEnd" => Row(Composer, "Delete to line end"),
        "composer::WordLeft" => Row(Composer, "Word left"),
        "composer::WordRight" => Row(Composer, "Word right"),
        "composer::SelectLeft" => Row(Composer, "Select left"),
        "composer::SelectRight" => Row(Composer, "Select right"),
        "composer::SelectWordLeft" => Row(Composer, "Select word left"),
        "composer::SelectWordRight" => Row(Composer, "Select word right"),
        "composer::SelectHome" => Row(Composer, "Select to line start"),
        "composer::SelectEnd" => Row(Composer, "Select to line end"),
        "composer::SelectAll" => Row(Composer, "Select all"),
        "composer::Copy" => Row(Composer, "Copy"),
        "composer::Cut" => Row(Composer, "Cut"),
        "composer::Undo" => Row(Composer, "Undo"),
        "composer::Redo" => Row(Composer, "Redo"),
        "cockpit::MenuPrevious" => Row(Composer, "Menu: previous"),
        "cockpit::MenuNext" => Row(Composer, "Menu: next"),
        "cockpit::MenuPick" => Row(Composer, "Menu: pick"),
        "cockpit::MenuDismiss" => Row(Composer, "Menu: close"),
        // The transcript.
        "cockpit::CopySelection" => Row(Transcript, "Copy selection"),
        "cockpit::ToggleTool" => Row(Transcript, "Open or fold a tool call"),
        "cockpit::ToolCyclePrevious" => Row(Transcript, "Previous tool call"),
        "find::Open" => Row(Transcript, "Find"),
        "find::Next" => Row(Transcript, "Find: next"),
        "find::Previous" => Row(Transcript, "Find: previous"),
        "find::Close" => Row(Transcript, "Find: close"),
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
        | "nav::CloseView"
        | "shortcuts::ScrollUp"
        | "shortcuts::ScrollDown"
        | "shortcuts::Dismiss"
        | "zed::NoAction" => Hidden,
        // So do the Review slot's.
        "review::Previous"
        | "review::Next"
        | "review::ExtendPrevious"
        | "review::ExtendNext"
        | "review::Comment"
        | "review::Uncomment"
        | "review::NextScope"
        | "review::Reread"
        | "review::Close"
        | "review::Search"
        | "review::ToggleMarkdown"
        | "review::ShowPullRequest"
        | "review::SaveComment"
        | "review::CancelComment"
        | "review::OpenFound"
        | "review::CloseSearch"
        | "review::AllFiles"
        | "review::PickBase"
        | "review::ChooseBase"
        | "review::CloseBases" => Hidden,
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

/// The sheet's rows for a key table, in section order, each action once
/// (its first binding in a place wins: `⌘N` before `⌘T`). The pane digits
/// and the answer digits read as one row each (`⌘1…9`, `1…3`).
pub(crate) fn rows(table: &[Binding]) -> Vec<SheetRow> {
    let mut rows: Vec<SheetRow> = Vec::new();
    for (keys, action, context) in table {
        let Some(Entry::Row(place, label)) = label(action, *context) else {
            continue;
        };
        if rows
            .iter()
            .any(|row| row.place == place && row.label == label)
        {
            continue;
        }
        let mut keys = components::spell_chord(keys);
        if matches!(*action, "cockpit::FocusThread1") {
            keys.push_str("\u{2026}9");
        } else if matches!(*action, "cockpit::PickOption1") {
            keys.push_str("\u{2026}3");
        }
        rows.push(SheetRow { place, label, keys });
    }
    rows.sort_by_key(|row| row.place);
    rows
}

/// The rows `query` keeps, in order, each with its label's lit ranges: the
/// palette's filter over the label, then the section and the chord.
fn filter<'a>(rows: &'a [SheetRow], query: &str) -> Vec<(&'a SheetRow, Vec<Range<usize>>)> {
    rows.iter()
        .filter_map(|row| {
            let context = format!("{} {}", row.place.title(), row.keys);
            crate::fuzzy::palette_match(query, row.label, &context).map(|lit| (row, lit))
        })
        .collect()
}

/// The sheet's footer, word for word.
pub(crate) const FOOTER: &str = "\u{2191}\u{2193} scroll \u{b7} esc";

/// The sheet: its rows, its filter line (which holds the keyboard, its
/// keys in the `Shortcuts` context) and its scroll.
pub(crate) struct Shortcuts {
    rows: Vec<SheetRow>,
    pub input: Entity<Composer>,
    scroll: ScrollHandle,
    _edited: Subscription,
}

impl Shortcuts {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let mut line = Composer::new(cx);
            line.set_role(Role::Field(Some("Shortcuts")), cx);
            line.set_visible_row_limit(1, cx);
            line
        });
        let edited = cx.subscribe(&input, |sheet: &mut Self, _, _: &Edited, cx| {
            sheet.scroll.set_offset(point(px(0.), px(0.)));
            cx.notify();
        });
        Self {
            rows: rows(&crate::keymap::active()),
            input,
            scroll: ScrollHandle::new(),
            _edited: edited,
        }
    }

    /// The rows the filter line keeps — what the tests read.
    #[cfg(test)]
    pub(crate) fn shown(&self, cx: &gpui::App) -> Vec<&SheetRow> {
        filter(&self.rows, self.input.read(cx).text())
            .into_iter()
            .map(|(row, _)| row)
            .collect()
    }

    /// ↑ / ↓: a row at a time, held inside the list.
    pub(crate) fn scroll_by(&mut self, rows: f32, cx: &mut Context<Self>) {
        let offset = self.scroll.offset();
        let max = f32::from(self.scroll.max_offset().y).max(0.0);
        let y = (f32::from(offset.y) - rows * PALETTE_ROW_H).clamp(-max, 0.0);
        self.scroll.set_offset(point(offset.x, px(y)));
        cx.notify();
    }
}

impl Render for Shortcuts {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = crate::palette::input_row("shortcuts-input", self.input.clone()).child(
            div()
                .flex_shrink_0()
                .text_size(px(FS_META))
                .text_color(rgb(TEXT_DIM))
                .child("esc"),
        );
        let query = self.input.read(cx).text().to_string();
        let shown = filter(&self.rows, &query);
        let mut list = div()
            .id("shortcuts-rows")
            .flex()
            .flex_col()
            .min_h_0()
            .max_h(px(PALETTE_MAX_H))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(px(PALETTE_LIST_PAD))
            .pt(px(PALETTE_LIST_PAD))
            .pb(px(PALETTE_LIST_PAD_B));
        let mut place = None;
        for (index, (row, lit)) in shown.iter().enumerate() {
            if place != Some(row.place) {
                let first = place.is_none();
                place = Some(row.place);
                list = list.child(crate::palette::section_heading(row.place.title(), first));
            }
            list = list.child(
                components::text_ui()
                    .debug_selector(move || format!("shortcut-row-{index}"))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .gap(px(PALETTE_GAP))
                    .h(px(PALETTE_ROW_H))
                    .px(px(PALETTE_ROW_PAD_X))
                    .whitespace_nowrap()
                    .child(
                        div().min_w_0().truncate().text_color(rgb(TEXT)).child(
                            StyledText::new(row.label)
                                .with_highlights(components::match_highlights(lit, false)),
                        ),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(FS_META))
                            .child(components::key_combo(&row.keys, TEXT_DIM)),
                    ),
            );
        }
        if shown.is_empty() {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(PALETTE_ROW_H))
                    .px(px(PALETTE_ROW_PAD_X))
                    .text_color(rgb(TEXT_DIM))
                    .child(crate::palette::NO_MATCHES),
            );
        }
        crate::palette::dialog()
            .debug_selector(|| "shortcuts-sheet".into())
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(input)
            .child(list)
            .child(crate::palette::footer_row(FOOTER.into(), String::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{self, Platform};

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
        for (action, context) in keymap::UNBOUND {
            assert!(label(action, context).is_some(), "{action} has no label");
        }
    }

    /// The sheet reads the table: its sections in order, each action once,
    /// the digits gathered, the Decision and wall keys in their own places,
    /// the editing keys included.
    #[test]
    fn the_sheet_lists_the_table_by_place() {
        let rows = rows(&keymap::bindings(Platform::Mac));
        let find = |place: Place, label: &str| {
            rows.iter()
                .find(|row| row.place == place && row.label == label)
                .unwrap_or_else(|| panic!("{place:?} {label} is missing"))
                .keys
                .clone()
        };
        assert_eq!(find(Place::Threads, "Commands"), "cmd-K");
        assert_eq!(find(Place::Threads, "New thread"), "cmd-N");
        assert_eq!(
            find(Place::Threads, "Park thread"),
            "cmd-\u{232b}",
            "the empty line's ⌘⌫ is the first spelling"
        );
        assert_eq!(
            find(Place::Board, "Focus pane 1\u{2013}9"),
            "cmd-1\u{2026}9"
        );
        assert_eq!(find(Place::Decision, "Allow"), "Y");
        assert_eq!(find(Place::Decision, "Pick 1\u{2013}3"), "1\u{2026}3");
        assert_eq!(
            find(Place::Decision, "Amend (a note with the answer)"),
            "\u{21e5}"
        );
        assert_eq!(find(Place::Wall, "Answer 1\u{2013}3"), "1\u{2026}3");
        assert_eq!(
            find(Place::Composer, "Cycle permission mode"),
            "shift-\u{21e5}"
        );
        assert_eq!(find(Place::Composer, "Shortcuts"), "?");
        assert_eq!(find(Place::Composer, "Delete word back"), "alt-\u{232b}");
        assert_eq!(find(Place::Transcript, "Copy selection"), "cmd-C");
        assert_eq!(find(Place::Transcript, "Find"), "cmd-F");
        assert_eq!(find(Place::Transcript, "Find: next"), "\u{23ce}");
        assert_eq!(find(Place::Transcript, "Find: previous"), "shift-\u{23ce}");
        assert_eq!(find(Place::Transcript, "Find: close"), "esc");
        let places: Vec<Place> = rows.iter().map(|row| row.place).collect();
        let mut sorted = places.clone();
        sorted.sort();
        assert_eq!(places, sorted, "sections in order");
        assert!(rows
            .iter()
            .all(|row| !row.label.is_empty() && !row.keys.is_empty()));
    }

    /// The filter keeps the rows whose label, section or chord match, in
    /// order, lighting the label.
    #[test]
    fn the_filter_matches_label_section_and_chord() {
        let rows = rows(&keymap::bindings(Platform::Mac));
        let labels = |query: &str| -> Vec<&str> {
            filter(&rows, query)
                .into_iter()
                .map(|(row, _)| row.label)
                .collect()
        };
        assert_eq!(labels("").len(), rows.len());
        assert!(labels("park").starts_with(&["Park thread"]));
        assert!(
            labels("wall").contains(&"Always allow"),
            "a section matches"
        );
        assert!(labels("cmd-k").contains(&"Commands"), "a chord matches");
        assert!(labels("zzzz").is_empty());
        let (_, lit) = filter(&rows, "park").remove(0);
        assert_eq!(lit, vec![0..4]);
    }

    /// The sheet reads the operator's keys: a rebound action shows its new
    /// chord, an unbound one is gone.
    #[test]
    fn the_sheet_shows_the_operators_keys() {
        let keys = std::collections::BTreeMap::from([
            ("palette::Toggle".to_string(), Some("cmd-p".to_string())),
            ("cockpit::ToggleNav".to_string(), None),
        ]);
        let (table, refused) = keymap::with_user_keys(Platform::Mac, &keys);
        assert!(refused.is_empty(), "{refused:?}");
        let rows = rows(&table);
        let commands = rows.iter().find(|row| row.label == "Commands").unwrap();
        assert_eq!(commands.keys, "cmd-P");
        assert!(rows.iter().all(|row| row.label != "Toggle sidebar"));
    }
}
