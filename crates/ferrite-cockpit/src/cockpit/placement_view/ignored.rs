//! The setup sheet's list of what git ignores in the Project: one tick for
//! them all, none, some or every one ticked, and under it each entry its
//! own tick, the rest past `ENTRIES_SHOWN` one tick; and the line that says
//! the worktrees in it stay.

use super::*;
use copies::words::{bytes, count};
use gpui::component::checkbox::Checkbox;

/// One entry git ignores, and whether it goes too.
pub(super) struct Entry {
    pub(super) ignored: Ignored,
    pub(super) size: Option<u64>,
    pub(super) ticked: bool,
}

/// At most this many entries git ignores are listed one by one: the rest
/// share the last row, ticked together.
const ENTRIES_SHOWN: usize = 20;

/// The header's tick for `entries`: none, some or all of them ticked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ticked {
    None,
    Some,
    All,
}

impl Ticked {
    pub(super) fn of(entries: &[Entry]) -> Self {
        match entries.iter().filter(|entry| entry.ticked).count() {
            0 => Ticked::None,
            ticked if ticked == entries.len() => Ticked::All,
            _ => Ticked::Some,
        }
    }
}

impl PlacementView {
    /// Tick the entry at `index`, or with `rest` every one from it on.
    fn tick(&mut self, index: usize, rest: bool, ticked: bool, cx: &mut Context<Self>) {
        let entries = self.entries.iter_mut().flatten().skip(index);
        for entry in entries.take(if rest { usize::MAX } else { 1 }) {
            entry.ticked = ticked;
        }
        cx.notify();
    }

    /// The header's press: all ticked from none or some, none from all.
    pub(super) fn tick_all(&mut self, cx: &mut Context<Self>) {
        let ticked = self
            .entries
            .as_deref()
            .is_some_and(|entries| Ticked::of(entries) != Ticked::All);
        self.tick(0, true, ticked, cx);
    }

    /// What git ignores under its one tick, each entry a tick, the rest
    /// past `ENTRIES_SHOWN` one tick; and the worktrees, which stay. None
    /// when the Project has neither.
    pub(super) fn ignored_section(&self, entries: &[Entry], cx: &mut Context<Self>) -> Option<Div> {
        use ferrite_ui::theme::*;
        if entries.is_empty() && self.worktrees.is_empty() {
            return None;
        }
        let split = if entries.len() > ENTRIES_SHOWN {
            ENTRIES_SHOWN - 1
        } else {
            entries.len()
        };
        let (each, rest) = entries.split_at(split);
        let row = |id: (&'static str, usize),
                   selector: String,
                   label: String,
                   ticked: bool,
                   size: Option<u64>,
                   rest: bool,
                   cx: &mut Context<Self>| {
            let index = id.1;
            div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(ROW))
                .pl(px(SPACE_4 + SPACE_2))
                .gap(px(2.0 * CH))
                .child(
                    Checkbox::new(id)
                        .debug_selector(move || selector.clone())
                        .checked(ticked)
                        .accessibility_label(label.clone())
                        // A label is drawn one line high, which clips an
                        // underscore; a child is taller than the box, so it
                        // is centred on it. Muted, under the header's tick.
                        .items_center()
                        .child(note(label))
                        .on_click(cx.listener(move |sheet: &mut Self, ticked: &bool, _, cx| {
                            sheet.tick(index, rest, *ticked, cx)
                        })),
                )
                .child(note(size.map(bytes).unwrap_or_default()))
        };
        let mut rows: Vec<Div> = each
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let path = &entry.ignored.path;
                let shown = if entry.ignored.directory {
                    format!("{path}/")
                } else {
                    path.clone()
                };
                let selector = format!("placement-ignored:{path}");
                row(("placement-ignored", index), selector, shown, entry.ticked, entry.size, false, cx)
            })
            .collect();
        if !rest.is_empty() {
            let size = rest.iter().map(|entry| entry.size).sum();
            let ticked = rest.iter().all(|entry| entry.ticked);
            rows.push(row(
                ("placement-ignored", split),
                "placement-ignored-more".into(),
                format!("and {} more", rest.len()),
                ticked,
                size,
                true,
                cx,
            ));
        }
        let mut section = div().flex().flex_col().flex_shrink_0();
        if !entries.is_empty() {
            let ticked = Ticked::of(entries);
            let items = match entries.len() {
                1 => "1 item".to_string(),
                items => format!("{} items", count(items)),
            };
            let total = match entries.iter().map(|entry| entry.size).sum::<Option<u64>>() {
                Some(size) => format!("{items} \u{b7} {}", bytes(size)),
                None => items,
            };
            let label = "Include files git ignores";
            let header = div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(ROW))
                .gap(px(2.0 * CH))
                .child(
                    Checkbox::new("placement-ignored-all")
                        .debug_selector(|| "placement-ignored-all".into())
                        .checked(ticked == Ticked::All)
                        .indeterminate(ticked == Ticked::Some)
                        .accessibility_label(label)
                        .items_center()
                        .child(label)
                        .on_click(cx.listener(|sheet: &mut Self, _: &bool, _, cx| sheet.tick_all(cx))),
                )
                .child(note(total).debug_selector(|| "placement-ignored-total".into()));
            section = section.child(header).child(project_editor::directory_list(rows));
        }
        if !self.worktrees.is_empty() {
            section = section.child(
                note("Worktrees are not copied.")
                    .pt(px(if entries.is_empty() { 0. } else { HALF_ROW }))
                    .debug_selector(|| "placement-worktrees".into()),
            );
        }
        Some(section)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(ticks: &[bool]) -> Vec<Entry> {
        ticks
            .iter()
            .enumerate()
            .map(|(at, &ticked)| Entry {
                ignored: Ignored {
                    path: format!("{at}.log"),
                    directory: false,
                },
                size: None,
                ticked,
            })
            .collect()
    }

    #[test]
    fn the_header_tick_is_none_some_or_all() {
        assert_eq!(Ticked::of(&entries(&[false, false])), Ticked::None);
        assert_eq!(Ticked::of(&entries(&[true, false])), Ticked::Some);
        assert_eq!(Ticked::of(&entries(&[true, true])), Ticked::All);
    }
}
