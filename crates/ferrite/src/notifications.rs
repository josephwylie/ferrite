//! The bell: every Thread that finished while the operator looked elsewhere.
//!
//! Drawing and toasts only. What counts as finished is decided headless in
//! `ferrite_core::notifications` — the Notices this module shows are read
//! from there, and the cockpit wires every click back into it through one
//! `Verb`. GPUI Kit owns the moving parts: the toast stack and its
//! auto-hide (`Notification`), the popover's anchoring and outside-click
//! dismissal (`Popover`). Ferrite draws the bell, its count, the panel (the
//! one floating surface) and every toast's body.

use std::collections::BTreeSet;
use std::rc::Rc;

use ferrite_core::notifications::{
    DecisionNotice, DecisionNoticeId, Notice, NoticeId, RequestKind,
};
use ferrite_core::{ThreadId, TurnOutcome};
use gpui::component::button::Button;
use gpui::component::notification::Notification;
use gpui::component::popover::Popover;
use gpui::component::WindowExt as _;
use gpui::prelude::*;
use gpui::{div, px, rgb, Anchor, AnyElement, App, Div, SharedString, Stateful, Window};

use crate::components;
use crate::icons;
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::*;

/// What a click on the bell's surfaces means. The cockpit answers each
/// against the core and repaints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verb {
    /// Land on the Notice's Thread (a toast, a row).
    Open(NoticeId),
    /// Forget one Notice (a row's ×).
    Dismiss(NoticeId),
    /// Land on the exact live request the row represented.
    OpenDecision(DecisionNoticeId),
    /// Hide one live request until its handle changes or ends.
    DismissDecision(DecisionNoticeId),
    /// Forget them all.
    Clear,
}

pub type Handle = Rc<dyn Fn(Verb, &mut Window, &mut App)>;

/// A Bell row has one actionable target, independent of provider wire data.
#[derive(Clone, Debug)]
pub enum RowTarget {
    Notice(NoticeId),
    Decision(DecisionNoticeId),
}

/// The shared presentation kind. A request never invents a turn outcome.
#[derive(Clone, Debug)]
pub enum RowKind {
    Completion(TurnOutcome),
    Request(RequestKind),
}

/// Completion and live-request rows share the same renderer. Core owns the
/// target and kind; the cockpit adds cached title and project words.
#[derive(Clone, Debug)]
pub struct Row {
    pub target: RowTarget,
    pub thread: ThreadId,
    pub title: SharedString,
    pub project: Option<SharedString>,
    pub kind: RowKind,
    pub when: SharedString,
    pub read: bool,
}

impl Row {
    pub fn new(
        notice: &Notice,
        title: SharedString,
        project: Option<SharedString>,
        when: SharedString,
    ) -> Self {
        Self {
            target: RowTarget::Notice(notice.id),
            thread: notice.thread,
            title,
            project,
            kind: RowKind::Completion(notice.outcome.clone()),
            when,
            read: notice.read,
        }
    }

    pub fn decision(
        notice: &DecisionNotice,
        title: SharedString,
        project: Option<SharedString>,
    ) -> Self {
        Self {
            target: RowTarget::Decision(notice.id.clone()),
            thread: notice.id.thread,
            title,
            project,
            kind: RowKind::Request(notice.kind),
            when: "now".into(),
            read: notice.read,
        }
    }

    /// The detail split for drawing: its lexicon lead word, that word's ink
    /// (only a failure or a waiting Decision is coloured), and the rest,
    /// which starts at its first ` · ` seam.
    fn detail_parts(&self) -> (SharedString, u32, SharedString) {
        let lead = self.lead();
        let detail = self.detail();
        let rest = detail.strip_prefix(lead).unwrap_or(&detail).to_string();
        (lead.into(), word_ink(lead), rest.into())
    }

    /// The row's state in the shared lexicon.
    fn lead(&self) -> &'static str {
        match &self.kind {
            RowKind::Completion(TurnOutcome::Error(_)) => words::FAILED,
            RowKind::Completion(TurnOutcome::Interrupted) => words::INTERRUPTED,
            RowKind::Completion(TurnOutcome::Completed) => words::DONE,
            RowKind::Request(_) => words::NEEDS_YOU,
        }
    }

    /// `<state> · <what> · <project>`: the lead word, what a request needs
    /// or the error a turn failed with, then the project.
    fn detail(&self) -> SharedString {
        let mut parts = vec![self.lead().to_string()];
        match &self.kind {
            RowKind::Completion(TurnOutcome::Error(error)) => parts.push(error.clone()),
            RowKind::Completion(_) => {}
            RowKind::Request(RequestKind::Question) => parts.push(words::QUESTION.into()),
            RowKind::Request(RequestKind::Permission) => parts.push(words::APPROVAL.into()),
        }
        if let Some(project) = &self.project {
            parts.push(project.to_string());
        }
        parts.join(" \u{b7} ").into()
    }
}

/// The toast identity: one per Thread, so a Thread that finishes twice
/// before the operator looks replaces its own toast rather than stacking.
struct Finished;
struct Request;

/// The window's side of the bell: whether its panel is down, and which
/// Notices it has toasted already.
pub struct Bell {
    pub open: bool,
    presented: Option<NoticeId>,
    presented_requests: BTreeSet<DecisionNoticeId>,
}

impl Bell {
    pub fn new() -> Self {
        Self {
            open: false,
            presented: None,
            presented_requests: BTreeSet::new(),
        }
    }

    /// The watermark: Notices at or below it have had their toast.
    pub fn presented(&self) -> Option<NoticeId> {
        self.presented
    }

    /// Toast every unread Notice born since the last frame and move the
    /// watermark past all of them. A Notice born read — the operator was
    /// on that Pane — has nothing to shout about.
    pub fn present(
        &mut self,
        rows: impl IntoIterator<Item = Row>,
        handle: &Handle,
        window: &mut Window,
        cx: &mut App,
    ) {
        for row in rows {
            let RowTarget::Notice(id) = &row.target else {
                continue;
            };
            let id = *id;
            self.presented = Some(self.presented.map_or(id, |seen| seen.max(id)));
            if row.read {
                continue;
            }
            window.push_notification(toast(&row, handle.clone()), cx);
        }
    }

    /// Keep live request toasts in lockstep with their generation-scoped
    /// records. Completion uses its monotonic watermark above; requests use
    /// their own opaque identities and vanish as soon as Activity resolves them.
    pub fn present_requests(
        &mut self,
        rows: impl IntoIterator<Item = Row>,
        handle: &Handle,
        window: &mut Window,
        cx: &mut App,
    ) {
        let rows: Vec<_> = rows.into_iter().collect();
        let live: BTreeSet<_> = rows
            .iter()
            .filter_map(|row| match &row.target {
                RowTarget::Decision(id) => Some(id.clone()),
                RowTarget::Notice(_) => None,
            })
            .collect();
        for id in self.presented_requests.difference(&live) {
            window.remove_notification1::<Request>(request_key(id), cx);
        }
        let presented = std::mem::replace(&mut self.presented_requests, live);
        for row in rows {
            let RowTarget::Decision(id) = &row.target else {
                continue;
            };
            if !presented.contains(id) && !row.read {
                window.push_notification(request_toast(&row, handle.clone()), cx);
            }
        }
    }

    /// The bell with its unread count and, when the panel is down, the
    /// panel under it. `rows` are newest first.
    pub fn element(
        &self,
        unread: usize,
        rows: Vec<Row>,
        handle: Handle,
        on_open: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let waiting = rows
            .iter()
            .any(|row| !row.read && matches!(row.kind, RowKind::Request(_)));
        let rows = Rc::new(rows);
        Popover::new("notifications-bell")
            .anchor(Anchor::TopLeft)
            .appearance(false)
            .trigger(trigger(unread, waiting, self.open))
            .open(self.open)
            .on_open_change(move |open, window, cx| on_open(*open, window, cx))
            .content(move |_, _, _| {
                crate::motion::menu_in(
                    "notifications-panel-in",
                    panel(&rows, handle.clone()),
                    crate::motion::Opens::Down,
                )
            })
            .into_any_element()
    }
}

impl Default for Bell {
    fn default() -> Self {
        Self::new()
    }
}

/// The 28×28 bell button in the nav's chrome band, with the unread count
/// riding its corner: `ATTENTION` while a Decision waits among the unread,
/// steel otherwise, hidden at zero.
fn trigger(unread: usize, waiting: bool, open: bool) -> Button {
    let glyph = if open { ACCENT } else { TEXT_MUTED };
    components::button("notifications-bell")
        .debug_selector(|| "notifications-bell".into())
        .relative()
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .tooltip("Notifications")
        .accessibility_label("Notifications")
        .child(icons::icon(icons::BELL, ICON_BUTTON_GLYPH, glyph))
        .when(unread > 0, |bell| bell.child(badge(unread, waiting)))
}

/// The unread count pill: UI `FS_SM`, tabular, `99+` past two digits.
fn badge(unread: usize, waiting: bool) -> Div {
    let (ground, ink) = badge_inks(waiting);
    let count: SharedString = if unread > 99 {
        "99+".into()
    } else {
        unread.to_string().into()
    };
    components::tabular(
        div()
            .absolute()
            .top(px(BADGE_INSET))
            .right(px(BADGE_INSET))
            .flex()
            .items_center()
            .justify_center()
            .h(px(BADGE_H))
            .min_w(px(BADGE_H))
            .px(px(SPACE_1))
            .rounded_full()
            .bg(rgb(ground))
            .font_family(FONT_UI)
            .text_size(px(FS_SM))
            .line_height(px(BADGE_H))
            .font_weight(W_LABEL)
            .text_color(rgb(ink))
            .child(count),
    )
}

/// The badge's ground and ink: a waiting Decision is attention, plain
/// completions are steel.
fn badge_inks(waiting: bool) -> (u32, u32) {
    if waiting {
        (ATTENTION, GROUND)
    } else {
        (ACCENT_STRONG, ON_ACCENT)
    }
}

/// A row's or toast's status mark: attention while a Decision waits, blocked
/// for a failure, and no colour for a plain finish (green never means
/// finished).
fn mark_ink(row: &Row) -> u32 {
    match &row.kind {
        RowKind::Request(_) => ATTENTION,
        RowKind::Completion(TurnOutcome::Error(_)) => BLOCKED,
        RowKind::Completion(_) => TEXT_MUTED,
    }
}

/// The detail line with only its state word coloured.
fn detail_line(row: &Row) -> Div {
    let (lead, ink, rest) = row.detail_parts();
    components::text_meta()
        .flex()
        .min_w_0()
        .child(div().flex_shrink_0().text_color(rgb(ink)).child(lead))
        .child(div().min_w_0().truncate().child(seamed(rest)))
}

/// A detail's tail with each `·` seam in `TEXT_FAINT`: highlighted in place,
/// so the line stays one run and copies back exactly as written.
fn seamed(text: SharedString) -> gpui::StyledText {
    let seams: Vec<_> = text
        .match_indices('\u{b7}')
        .map(|(at, dot)| {
            (
                at..at + dot.len(),
                gpui::HighlightStyle {
                    color: Some(rgb(TEXT_FAINT).into()),
                    ..Default::default()
                },
            )
        })
        .collect();
    gpui::StyledText::new(text).with_highlights(seams)
}

/// A toast's body in the UI voice: the status mark, the Thread's
/// name, the detail with its state word coloured.
fn toast_body(row: &Row) -> Div {
    let ink = mark_ink(row);
    let title = row.title.clone();
    let thread = row.thread.get();
    div()
        .debug_selector(move || format!("toast-{thread}"))
        .flex()
        .items_start()
        .gap(px(SPACE_2))
        .min_w_0()
        .child(
            div()
                .flex()
                .items_center()
                .h(px(LH_UI))
                .child(components::status_dot(ink)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    components::text_ui()
                        .text_color(rgb(TEXT_STRONG))
                        .truncate()
                        .child(title),
                )
                .child(detail_line(row)),
        )
}

/// One toast, in the kit's own stack: the Thread's name, what became of
/// it, and a click that lands the operator on its Pane.
fn toast(row: &Row, handle: Handle) -> Notification {
    let RowTarget::Notice(id) = row.target else {
        unreachable!("completion toast has a completion target")
    };
    let body = row.clone();
    Notification::new()
        .id1::<Finished>(row.thread.get() as usize)
        .content(move |_, _, _| toast_body(&body).into_any_element())
        .px(px(SPACE_3))
        .py(px(SPACE_3))
        .autohide(true)
        .on_click(move |_, window, cx| handle(Verb::Open(id), window, cx))
}

fn request_key(id: &DecisionNoticeId) -> String {
    format!(
        "{}-{}-{}",
        id.thread.get(),
        id.handle.generation,
        id.handle.serial
    )
}

/// A live request's toast: the attention mark and word.
fn request_toast(row: &Row, handle: Handle) -> Notification {
    let RowTarget::Decision(id) = &row.target else {
        unreachable!("request toast has a request target")
    };
    let id = id.clone();
    let body = row.clone();
    Notification::new()
        .id1::<Request>(request_key(&id))
        .content(move |_, _, _| toast_body(&body).into_any_element())
        .px(px(SPACE_3))
        .py(px(SPACE_3))
        .autohide(true)
        .on_click(move |_, window, cx| handle(Verb::OpenDecision(id.clone()), window, cx))
}

/// The panel under the bell: a head with the clear verb, then the rows
/// newest first, on the one floating surface every menu stands on.
fn panel(rows: &Rc<Vec<Row>>, handle: Handle) -> Div {
    let unread = rows.iter().filter(|row| !row.read).count();
    let panel = components::floating_surface()
        .w(px(NOTICE_PANEL_W))
        .max_h(px(MENU_MAX_H))
        .child(head(!rows.is_empty(), unread, handle.clone()));
    if rows.is_empty() {
        return panel.child(div().py(px(SPACE_6)).child(components::empty_state(
            "No notifications",
            Some("Finished turns and waiting Decisions land here.".into()),
        )));
    }
    panel.child(
        div()
            .id("notifications-list")
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_y_scroll()
            .children(
                rows.iter()
                    .enumerate()
                    .map(|(index, row)| row_element(index, row, handle.clone())),
            ),
    )
}

/// The panel's head: its title on the rows' leading edge (the status
/// dots' column), the count tabular, and `Clear all` hung so its word ends
/// where each row's dismiss ends. No rule under it: `NOTICE_HEAD_GAP` of
/// space sets it off from the rows.
fn head(clearable: bool, unread: usize, handle: Handle) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .h(px(MENU_ROW_H))
        .flex_shrink_0()
        .px(px(MENU_ROW_PAD_X))
        .mb(px(NOTICE_HEAD_GAP))
        .child(
            components::text_meta()
                .flex()
                .gap(px(SPACE_1))
                .child(
                    div()
                        .font_weight(W_LABEL)
                        .text_color(rgb(TEXT_2))
                        .child("Notifications"),
                )
                .when(unread > 0, |title| {
                    title.child(components::tabular(
                        div().child(format!("· {unread} unread")),
                    ))
                }),
        )
        .children(clearable.then(|| {
            components::button("notifications-clear")
                .debug_selector(|| "notifications-clear".into())
                .px(px(SPACE_1_5))
                .mr(px(-SPACE_1_5))
                .child(components::text_meta().child("Clear all"))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    handle(Verb::Clear, window, cx)
                })
        }))
}

fn row_element(index: usize, row: &Row, handle: Handle) -> Stateful<Div> {
    let target = row.target.clone();
    let open = handle.clone();
    let dismiss = row.target.clone();
    let group: SharedString = format!("notice-row-{index}").into();
    div()
        .id(("notice-row", index))
        .debug_selector(move || format!("notice-row-{index}"))
        .group(group.clone())
        .flex()
        .items_center()
        .w_full()
        .flex_shrink_0()
        .min_h(px(NOTICE_ROW_H))
        .px(px(MENU_ROW_PAD_X))
        .py(px(SPACE_1_5))
        .gap(px(SPACE_2))
        .rounded(px(R_MENU_ROW))
        .hover_raised()
        .press_raised()
        // Title and detail truncate at the panel's width; the whole of both
        // stays one hover away.
        .tooltip(crate::menu::tooltip(format!(
            "{}\n{}",
            row.title,
            row.detail()
        )))
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            open(target_verb(&target), window, cx)
        })
        // The mark's column stays when the row is read, so titles align.
        .child(
            div()
                .flex_shrink_0()
                .w(px(STATUS_DOT))
                .when(!row.read, |slot| {
                    slot.child(components::status_dot(mark_ink(row)))
                }),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    components::text_ui()
                        .text_color(rgb(if row.read { TEXT_2 } else { TEXT_STRONG }))
                        .truncate()
                        .child(row.title.clone()),
                )
                .child(detail_line(row)),
        )
        .child(components::tabular(
            components::text_meta()
                .flex_shrink_0()
                .child(row.when.clone()),
        ))
        // The dismiss × keeps its width at rest, so the age never moves; it
        // shows under the pointer.
        .child(
            components::button(("notice-dismiss", index))
                .p_0()
                .w(px(ICON_BUTTON_GLYPH))
                .h(px(ICON_BUTTON_GLYPH))
                .opacity(0.)
                .group_hover(group, |style| style.opacity(1.))
                .tooltip("Dismiss")
                .accessibility_label("Dismiss")
                .child(icons::icon(icons::CLOSE, ROW_ICON, TEXT_MUTED))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    handle(dismiss_verb(&dismiss), window, cx)
                }),
        )
}

fn target_verb(target: &RowTarget) -> Verb {
    match target {
        RowTarget::Notice(id) => Verb::Open(*id),
        RowTarget::Decision(id) => Verb::OpenDecision(id.clone()),
    }
}

fn dismiss_verb(target: &RowTarget) -> Verb {
    match target {
        RowTarget::Notice(id) => Verb::Dismiss(*id),
        RowTarget::Decision(id) => Verb::DismissDecision(id.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(outcome: TurnOutcome, project: Option<&str>) -> Row {
        Row {
            target: RowTarget::Notice(NoticeId::from_u64(1)),
            thread: ThreadId::new(3),
            title: "fix the bell".into(),
            project: project.map(SharedString::from),
            kind: RowKind::Completion(outcome),
            when: "now".into(),
            read: false,
        }
    }

    #[test]
    fn only_the_state_word_takes_a_colour() {
        let failed = row(TurnOutcome::Error("rate limited".into()), Some("ferrite"));
        assert_eq!(
            failed.detail_parts(),
            (
                "failed".into(),
                BLOCKED,
                " \u{b7} rate limited \u{b7} ferrite".into()
            )
        );
        let done = row(TurnOutcome::Completed, None);
        assert_eq!(done.detail_parts(), ("done".into(), TEXT_MUTED, "".into()));
        assert_eq!(mark_ink(&done), TEXT_MUTED, "green never means finished");
        assert_eq!(mark_ink(&failed), BLOCKED);
        let waiting = Row {
            kind: RowKind::Request(RequestKind::Permission),
            ..done
        };
        assert_eq!(
            waiting.detail_parts(),
            ("needs you".into(), ATTENTION, " \u{b7} approval".into())
        );
        assert_eq!(mark_ink(&waiting), ATTENTION);
        assert_eq!(badge_inks(true), (ATTENTION, GROUND));
        assert_eq!(badge_inks(false), (ACCENT_STRONG, ON_ACCENT));
    }

    #[test]
    fn a_rows_detail_names_the_outcome_and_the_project() {
        let done = row(TurnOutcome::Completed, Some("ferrite"));
        assert_eq!(done.detail(), SharedString::from("done \u{b7} ferrite"));
        let failed = row(TurnOutcome::Error("rate limited".into()), None);
        assert_eq!(
            failed.detail(),
            SharedString::from("failed \u{b7} rate limited")
        );
        let stopped = row(TurnOutcome::Interrupted, Some("ferrite"));
        assert_eq!(
            stopped.detail(),
            SharedString::from("interrupted \u{b7} ferrite")
        );
        let approval = Row {
            kind: RowKind::Request(RequestKind::Permission),
            ..row(TurnOutcome::Completed, Some("ferrite"))
        };
        assert_eq!(
            approval.detail(),
            SharedString::from("needs you \u{b7} approval \u{b7} ferrite")
        );
        let question = Row {
            kind: RowKind::Request(RequestKind::Question),
            ..row(TurnOutcome::Completed, Some("ferrite"))
        };
        assert_eq!(
            question.detail(),
            SharedString::from("needs you \u{b7} question \u{b7} ferrite")
        );
    }

    /// The lead words are the shared lexicon's, never a local literal.
    #[test]
    fn the_lead_words_are_the_lexicon() {
        let lead = |kind: RowKind| {
            Row {
                kind,
                ..row(TurnOutcome::Completed, Some("ferrite"))
            }
            .detail_parts()
            .0
        };
        assert_eq!(
            lead(RowKind::Completion(TurnOutcome::Completed)),
            SharedString::from(words::DONE)
        );
        assert_eq!(
            lead(RowKind::Completion(TurnOutcome::Interrupted)),
            SharedString::from(words::INTERRUPTED)
        );
        assert_eq!(
            lead(RowKind::Completion(TurnOutcome::Error("x".into()))),
            SharedString::from(words::FAILED)
        );
        for kind in [RequestKind::Permission, RequestKind::Question] {
            assert_eq!(
                lead(RowKind::Request(kind)),
                SharedString::from(words::NEEDS_YOU)
            );
        }
    }
}
