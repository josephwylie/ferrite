//! The setup sheet's list of what git ignores in the Project, each entry
//! a tick and the rest past `ENTRIES_SHOWN` one tick, and the line that
//! says the worktrees in it stay.

use super::*;

/// One entry git ignores, and whether it goes too.
pub(super) struct Entry {
    pub(super) ignored: Ignored,
    pub(super) size: Option<u64>,
    pub(super) ticked: bool,
}

/// At most this many entries git ignores are listed one by one: the rest
/// share the last row, ticked together.
const ENTRIES_SHOWN: usize = 20;

impl PlacementView {
    /// Tick the entry at `index`, or with `rest` every one from it on.
    fn tick(&mut self, index: usize, rest: bool, ticked: bool, cx: &mut Context<Self>) {
        let entries = self.entries.iter_mut().flatten().skip(index);
        for entry in entries.take(if rest { usize::MAX } else { 1 }) {
            entry.ticked = ticked;
        }
        cx.notify();
    }

    /// What git ignores, each entry a tick, the rest past
    /// `ENTRIES_SHOWN` one tick; and the worktrees, which stay.
    pub(super) fn ignored_section(&self, entries: &[Entry], cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let held = self.held();
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
                .gap(px(2.0 * CH))
                .child(
                    Checkbox::new(id)
                        .debug_selector(move || selector.clone())
                        .checked(ticked)
                        .disabled(held)
                        .accessibility_label(label.clone())
                        // A label is drawn one line high, which clips an
                        // underscore; a child is taller than the box, so it
                        // is centred on it.
                        .items_center()
                        .child(label)
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
        let mut section = div().flex().flex_col().flex_shrink_0().gap(px(HALF_ROW));
        if !rows.is_empty() {
            section = section
                .child(project_editor::section_label(
                    "Ignored by git",
                    "Copied only when ticked",
                ))
                .child(project_editor::directory_list(rows));
        }
        if !self.worktrees.is_empty() {
            section = section.child(
                note("Worktrees are not copied.").debug_selector(|| "placement-worktrees".into()),
            );
        }
        section
    }
}
