//! The card's three faces, each a head, what it shows and a footer:
//! choosing where a copy goes and what goes, how far the copy is, and
//! what it did.

use copies::words::{bytes, count, took};
use copies::Reading;
use super::*;
use ferrite_ui::components;
use ferrite_ui::theme::*;

/// A face's head, what it shows, and its footer.
pub(super) type Drawn = (Div, Div, Div);

impl PlacementView {
    /// Where the copy goes and what goes. The browser opens only when asked
    /// (Change…), or when where the copy would go is in doubt.
    pub(super) fn choose(&self, window: &Window, cx: &mut Context<Self>) -> Drawn {
        let head = ferrite_ui::form::head(
            format!("Copy {} to {}", self.placing.title, self.placing.machine_name).into(),
            self.close(cx),
        );
        let removed = self.removed(cx);
        // How a copy that stopped stands leads the card; what is said of
        // where it would go stands under that.
        let (lead, under) = match self.stage_line(cx) {
            Some((line, true)) => (Some(line), None),
            Some((line, false)) => (None, Some(line)),
            None => (None, None),
        };
        let mut content = column().children(lead);
        content = if self.browsing && !removed {
            content.child(self.dest.clone()).child(
                ferrite_ui::field::prefixed_field(
                    "Copies to".into(),
                    self.folder_prefix(cx).into(),
                    self.name.clone(),
                    self.name.read(cx).focus_handle(cx).is_focused(window),
                    "placement-name-focused",
                )
                .debug_selector(|| "placement-name".into()),
            )
        } else {
            let change = components::secondary_button("placement-change", None, "Change…", cx)
                .debug_selector(|| "placement-change".into())
                .h(px(FORM_CONTROL_H - 2.0 - 2.0 * SPACE_1))
                .px(px(FORM_BUTTON_PAD_X))
                .on_click(cx.listener(|sheet: &mut Self, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    sheet.browse(window, cx);
                }));
            // A Machine removed has no folder to choose.
            let change = div().children((!removed).then_some(change));
            content.child(
                ferrite_ui::field::shown_path(
                    self.folder_prefix(cx).into(),
                    self.copy_name(cx).to_string().into(),
                    change,
                )
                .debug_selector(|| "placement-destination".into()),
            )
        };
        content = content.children(under);
        content = match &self.entries {
            None => content.child(
                note("Reading what git ignores").debug_selector(|| "placement-listing".into()),
            ),
            Some(entries) => content.children(self.ignored_section(entries, cx)),
        };
        let left = div().when(self.browsing, |left| {
            left.child(
                components::ghost_button("placement-new-folder", "New folder", cx)
                    .debug_selector(|| "placement-new-folder".into())
                    .on_click(cx.listener(|sheet: &mut Self, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        sheet.dest.update(cx, |dest, cx| dest.new_folder(window, cx));
                    })),
            )
        });
        // A card on a copy that stopped has nothing left to cancel.
        let dismiss = if self.shows_a_copy(cx) { "Close" } else { "Cancel" };
        let right = div()
            .flex()
            .items_center()
            .gap(px(SPACE_2))
            .child(
                components::ghost_button("placement-dismiss", dismiss, cx)
                    .debug_selector(|| "placement-dismiss".into())
                    .on_click(cx.listener(|sheet: &mut Self, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        sheet.dismiss(cx);
                    })),
            )
            .children((!removed).then(|| self.primary_button(cx)));
        (head, content, ferrite_ui::form::footer(left, right))
    }

    /// Choosing's completing button: Copy and what it sends, Resume where
    /// an unfinished copy is, Retry after a failure, or what is there
    /// used as it is. A copy whose Machine was removed has none: only
    /// Close.
    fn primary_button(&self, cx: &mut Context<Self>) -> gpui::component::button::Button {
        let unready = self.entries.is_none() || self.unfit.is_some();
        let phase = self.copy(cx).map(|copy| &copy.phase);
        let copy = || match self.to_send() {
            Some(size) => format!("Copy {}", bytes(size)),
            None => "Copy".into(),
        };
        let (verb, selector, disabled): (String, &'static str, bool) = match phase {
            Some(Phase::Occupied { of: None }) => ("Use what is there".into(), "placement-use", false),
            Some(Phase::Occupied { .. }) => (copy(), "placement-copy", true),
            _ if self.resumes(cx) => ("Resume".into(), "placement-resume", unready),
            Some(Phase::Failed { .. }) => ("Retry".into(), "placement-retry", unready),
            _ => (copy(), "placement-copy", unready),
        };
        ferrite_ui::form::primary_button("placement-confirm", verb, disabled, cx)
            .debug_selector(move || selector.into())
            .on_click(cx.listener(|sheet: &mut Self, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                sheet.primary(cx);
            }))
    }

    /// How the last copy here ended, or why none can go, when there is
    /// anything to say: two lines at most, whole in its tooltip, and
    /// whether it leads the card. A copy that stopped, cancelled or failed
    /// or its Machine back, says it as its toast does
    /// (`copies::Entry::standing`), first and in the ink of what is read
    /// (a failure's red); what is said of the destination (taken, refused,
    /// an unfinished copy there) stands under it.
    fn stage_line(&self, cx: &App) -> Option<(gpui::Stateful<Div>, bool)> {
        let back = self.copies.read(cx).back(&self.key(), cx);
        let standing = self
            .copy(cx)
            .and_then(|copy| Some((copy.standing(back)?, &copy.phase)));
        let (text, ink, selector, leads): (String, u32, &'static str, bool) =
            match (self.refusal.as_ref().or(self.unfit.as_ref()), standing) {
                (Some(refusal), _) => (refusal.to_string(), BLOCKED, "placement-refusal", false),
                (None, Some(((text, _), Phase::Occupied { .. }))) => (text, BLOCKED, "placement-occupied", false),
                (None, Some(((text, failed), phase))) => (
                    text,
                    if failed { BLOCKED } else { INK_STRONG },
                    match phase {
                        Phase::Cancelled { .. } => "placement-cancelled",
                        _ => "placement-failed",
                    },
                    true,
                ),
                _ if self.resumes(cx) => (
                    format!("An unfinished copy of {} is here.", self.placing.title),
                    INK_SUBTLE,
                    "placement-unfinished",
                    false,
                ),
                _ => return None,
            };
        let text = SharedString::from(text);
        let line = div()
            .id("placement-stage")
            .debug_selector(|| "placement-stage".into())
            .flex_shrink_0()
            .min_w_0()
            .tooltip(ferrite_ui::menu::tooltip(text.clone()))
            .child(
                components::text_ui()
                    .line_clamp(2)
                    .text_color(gpui::rgb(ink))
                    .debug_selector(move || selector.into())
                    .child(text),
            );
        Some((line, leads))
    }

    /// How far the copy is: where it goes, the bar, how much of how much
    /// at what pace, and the file it is on.
    pub(super) fn progress(&self, cx: &mut Context<Self>) -> Drawn {
        let machine = self.placing.machine_name.clone();
        let head = ferrite_ui::form::head(format!("Copying to {machine}").into(), self.close(cx));
        let Some(copy) = self.copy(cx) else {
            return (head, column(), div());
        };
        let mut content = column()
            .child(
                note(format!("{} \u{2192} {}", self.placing.title, copy.order.dest))
                    .truncate()
                    .debug_selector(|| "placement-route".into()),
            )
            .child(
                div()
                    .debug_selector(|| "placement-bar".into())
                    .child(components::progress_bar("placement-bar", copy.phase.done())),
            );
        let (numbers, line) = match &copy.phase {
            &Phase::Listing { files } => (
                None,
                match files {
                    0 => "Listing files\u{2026}".to_string(),
                    files => format!("Listing files\u{2026} {}", count(files)),
                },
            ),
            Phase::Copying {
                sent,
                total,
                file,
                reading,
                ..
            } => (
                Some((*sent, *total, *reading)),
                file.as_deref().map(|file| middle(file, FILE_CELLS)).unwrap_or_default(),
            ),
            &Phase::Finishing { bytes, .. } => (
                Some((bytes, bytes, Reading::default())),
                format!("Adding {} on {machine}", split_dest(&copy.order.dest).1),
            ),
            Phase::Stopping { .. } => (None, copies::words::stopping(&machine)),
            _ => (None, String::new()),
        };
        if let Some((sent, total, reading)) = numbers {
            let pace = match (reading.rate, reading.left) {
                (Some(rate), Some(left)) => format!("{}/s \u{b7} {left} left", bytes(rate)),
                (Some(rate), None) => format!("{}/s", bytes(rate)),
                (None, _) => String::new(),
            };
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(2.0 * CH))
                    .child(
                        components::text_ui()
                            .flex_shrink_0()
                            .debug_selector(|| "placement-sent".into())
                            .child(format!("{} of {}", bytes(sent), bytes(total))),
                    )
                    .child(note(pace).min_w_0().truncate().debug_selector(|| "placement-rate".into())),
            );
        }
        let stopping = matches!(copy.phase, Phase::Stopping { .. });
        content = content.child(
            note(line)
                .h(px(LH_UI))
                .truncate()
                .debug_selector(move || if stopping { "placement-stopping" } else { "placement-file" }.into()),
        );
        let cancels = matches!(copy.phase, Phase::Listing { .. } | Phase::Copying { .. });
        let right = div()
            .flex()
            .items_center()
            .gap(px(SPACE_2))
            .when(cancels, |right| {
                right.child(
                    components::ghost_button("placement-cancel", "Cancel", cx)
                        .debug_selector(|| "placement-cancel".into())
                        .on_click(cx.listener(|sheet: &mut Self, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            sheet.stop(cx);
                        })),
                )
            })
            .child(
                components::secondary_button("placement-background", None, "Continue in background", cx)
                    .debug_selector(|| "placement-background".into())
                    .on_click(cx.listener(|sheet: &mut Self, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        sheet.dismiss(cx);
                    })),
            );
        (head, content, ferrite_ui::form::footer(div(), right))
    }

    /// What the copy did, under the check of a finish: where the Project is
    /// now, how much in how long, whether the chat runs there, and what
    /// stayed as it was. A draft that did not move there can start a chat
    /// there.
    pub(super) fn result(&self, cx: &mut Context<Self>) -> Drawn {
        let machine = &self.placing.machine_name;
        let mark = div()
            .debug_selector(|| "placement-done-mark".into())
            .child(ferrite_ui::icons::icon(ferrite_ui::icons::CHECK_CIRCLE, ICON_BUTTON_GLYPH, RUNNING));
        // Named as its toast names it.
        let title = format!("Copied {} to {machine}", self.placing.title);
        let head = ferrite_ui::sheet::sheet_head_marked(mark, title, self.close(cx))
            .debug_selector(|| "placement-done-head".into());
        let mut content = column();
        let mut moved = true;
        if let Some((copy, Phase::Done(summary))) = self.copy(cx).map(|copy| (copy, &copy.phase)) {
            moved = summary.moved;
            content = content
                .child(
                    components::text_ui()
                        .debug_selector(|| "placement-where".into())
                        .child(format!("{} is at {} on {machine}", self.placing.title, copy.order.dest)),
                )
                .child(
                    note(format!("{} in {}", bytes(summary.bytes), took(summary.took)))
                        .debug_selector(|| "placement-copied".into()),
                )
                .when(summary.moved, |content| {
                    content.child(
                        note(format!("This chat now runs on {machine}."))
                            .debug_selector(|| "placement-moved".into()),
                    )
                });
            if !summary.skipped.lines.is_empty() {
                content = content
                    .child(ferrite_ui::form::section_label("Not copied", "Left as it was"))
                    .child(
                        ferrite_ui::form::directory_list(
                            summary.skipped.lines.iter().map(|line| note(line.clone())),
                        )
                        .debug_selector(|| "placement-skipped".into()),
                    );
            }
        }
        let done = ferrite_ui::form::primary_button("placement-confirm", "Done", false, cx)
            .debug_selector(|| "placement-done".into())
            .on_click(cx.listener(|sheet: &mut Self, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                sheet.dismiss(cx);
            }));
        let chat = (!moved).then(|| {
            components::secondary_button("placement-chat", None, "Start a chat there", cx)
                .debug_selector(|| "placement-chat".into())
                .h(px(FORM_CONTROL_H))
                .px(px(FORM_BUTTON_PAD_X))
                .on_click(cx.listener(|sheet: &mut Self, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    sheet.chat(cx);
                }))
        });
        let right = div()
            .flex()
            .items_center()
            .gap(px(SPACE_2))
            .children(chat)
            .child(done);
        (head, content, ferrite_ui::form::footer(div(), right))
    }
}

/// How many characters of the file being sent fit on its line: the
/// compact card's width inside its padding and edge.
const FILE_CELLS: usize = ((ferrite_ui::form::COMPACT_WIDTH - 2.0 * MODAL_PAD - 2.0) / (FS_UI * CODE_ADVANCE)) as usize;

/// A face's column: its rows a half row apart.
fn column() -> Div {
    div().flex().flex_col().gap(px(HALF_ROW))
}

/// `text` in at most `cells` characters, its middle cut for an ellipsis:
/// where a path starts and the name it ends in both show.
fn middle(text: &str, cells: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= cells || cells < 2 {
        return text.to_string();
    }
    let kept = cells - 1;
    let start = kept / 2;
    let end = kept - start;
    let mut out: String = chars[..start].iter().collect();
    out.push('\u{2026}');
    out.extend(&chars[chars.len() - end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_path_is_cut_in_the_middle_to_fit() {
        let path = "source/apps/web/src/routes/schedule.tsx";
        assert_eq!(middle(path, 60), path, "it fits");
        let cut = middle(path, 20);
        assert_eq!(cut.chars().count(), 20);
        assert_eq!(cut, "source/ap\u{2026}hedule.tsx");
        assert!(FILE_CELLS > 40, "{FILE_CELLS}");
    }
}
