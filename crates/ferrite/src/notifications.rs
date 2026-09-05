//! The bell: every Thread that finished while the operator looked elsewhere.
//!
//! Drawing and toasts only. What counts as finished is decided headless in
//! `ferrite_core::notifications` — the Notices this module shows are read
//! from there, and the cockpit wires every click back into it through one
//! `Verb`. GPUI Kit owns the moving parts: the toast stack and its
//! auto-hide (`Notification`), the popover's anchoring and outside-click
//! dismissal (`Popover`), the count (`Badge`) and the bell glyph itself.
//! Ferrite supplies the tokens.

use std::rc::Rc;

use ferrite_core::notifications::{Notice, NoticeId};
use ferrite_core::{ThreadId, TurnOutcome};
use gpui::component::badge::Badge;
use gpui::component::button::Button;
use gpui::component::notification::{Notification, NotificationType};
use gpui::component::popover::Popover;
use gpui::component::{Icon, IconName, Sizable, Size, WindowExt as _};
use gpui::prelude::*;
use gpui::{
    div, point, px, rgb, rgba, Anchor, AnyElement, App, BoxShadow, Div, FontWeight, SharedString,
    Stateful, Window,
};

use crate::components;
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::{
    ATTENTION, BLOCKED, FONT_UI, FS_MD, FS_SM, ICON_BUTTON, ICON_BUTTON_GLYPH, LINE_TIGHT, MENU,
    MENU_PAD, MENU_ROW_H, ROW_GAP, ROW_PAD_X, R_CONTROL, R_MENU, SHADOW_FAR, SHADOW_FAR_BLUR,
    SHADOW_FAR_SPREAD, SHADOW_FAR_Y, SHADOW_NEAR, SHADOW_NEAR_BLUR, SHADOW_NEAR_Y, STATUS_DOT,
    TEXT, TEXT_2, TEXT_MUTED, TEXT_STRONG,
};

/// What a click on the bell's surfaces means. The cockpit answers each
/// against the core and repaints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    /// Land on the Notice's Thread (a toast, a row).
    Open(NoticeId),
    /// Forget one Notice (a row's ×).
    Dismiss(NoticeId),
    /// Forget them all.
    Clear,
}

pub type Handle = Rc<dyn Fn(Verb, &mut Window, &mut App)>;

/// One Notice as the panel and a toast read it. Core knows the Thread and
/// the outcome; the cockpit adds the words — a Thread's name and Project
/// are the window's caches, not core facts.
#[derive(Clone, Debug)]
pub struct Row {
    pub id: NoticeId,
    pub thread: ThreadId,
    pub title: SharedString,
    pub project: Option<SharedString>,
    pub outcome: TurnOutcome,
    /// How long ago, in the nav's own shorthand (`now`, `4m`, `2h`).
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
            id: notice.id,
            thread: notice.thread,
            title,
            project,
            outcome: notice.outcome.clone(),
            when,
            read: notice.read,
        }
    }

    fn failed(&self) -> bool {
        matches!(self.outcome, TurnOutcome::Error(_))
    }

    /// `Finished · ferrite`, or the provider's own error in its place.
    fn detail(&self) -> SharedString {
        let outcome = match &self.outcome {
            TurnOutcome::Error(error) => format!("Failed · {error}"),
            _ => "Finished".to_string(),
        };
        match &self.project {
            Some(project) => format!("{outcome} · {project}").into(),
            None => outcome.into(),
        }
    }
}

/// The toast identity: one per Thread, so a Thread that finishes twice
/// before the operator looks replaces its own toast rather than stacking.
struct Finished;

/// The window's side of the bell: whether its panel is down, and which
/// Notices it has toasted already.
pub struct Bell {
    pub open: bool,
    presented: Option<NoticeId>,
}

impl Bell {
    pub fn new() -> Self {
        Self {
            open: false,
            presented: None,
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
            self.presented = Some(self.presented.map_or(row.id, |seen| seen.max(row.id)));
            if row.read {
                continue;
            }
            window.push_notification(toast(&row, handle.clone()), cx);
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
        let rows = Rc::new(rows);
        Popover::new("notifications-bell")
            .anchor(Anchor::TopLeft)
            .appearance(false)
            .trigger(trigger(unread))
            .open(self.open)
            .on_open_change(move |open, window, cx| on_open(*open, window, cx))
            .content(move |_, _, _| panel(&rows, handle.clone()))
            .into_any_element()
    }
}

impl Default for Bell {
    fn default() -> Self {
        Self::new()
    }
}

/// The 28×28 bell button in the nav's chrome band, with the unread count
/// riding its corner. The badge hides itself at zero.
fn trigger(unread: usize) -> Button {
    components::button("notifications-bell")
        .debug_selector(|| "notifications-bell".into())
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .tooltip("Notifications")
        .child(
            Badge::new()
                .count(unread)
                .max(99)
                .with_size(Size::Medium)
                .color(rgb(ATTENTION))
                .child(
                    Icon::new(IconName::Bell)
                        .size(px(ICON_BUTTON_GLYPH))
                        .text_color(rgb(TEXT_MUTED)),
                ),
        )
}

/// One toast, in the kit's own stack: the Thread's name, what became of
/// it, and a click that lands the operator on its Pane.
fn toast(row: &Row, handle: Handle) -> Notification {
    let id = row.id;
    Notification::new()
        .id1::<Finished>(row.thread.get() as usize)
        .title(row.title.clone())
        .message(row.detail())
        .with_type(if row.failed() {
            NotificationType::Error
        } else {
            NotificationType::Success
        })
        .autohide(true)
        .on_click(move |_, window, cx| handle(Verb::Open(id), window, cx))
}

/// The panel under the bell: a head with the clear verb, then the rows
/// newest first, on the same floating-menu surface every other menu here
/// stands on.
fn panel(rows: &Rc<Vec<Row>>, handle: Handle) -> Div {
    let mut panel = surface().child(head(!rows.is_empty(), handle.clone()));
    if rows.is_empty() {
        return panel.child(
            div()
                .px(px(ROW_PAD_X))
                .py(px(ROW_PAD_X))
                .text_size(px(FS_SM))
                .text_color(rgb(TEXT_MUTED))
                .child("No agent has finished yet."),
        );
    }
    for (index, row) in rows.iter().enumerate() {
        panel = panel.child(row_element(index, row, handle.clone()));
    }
    panel
}

fn head(clearable: bool, handle: Handle) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .h(px(MENU_ROW_H))
        .pl(px(ROW_PAD_X))
        .pr(px(MENU_PAD))
        .child(
            div()
                .text_size(px(FS_SM))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(TEXT_2))
                .child("Notifications"),
        )
        .children(clearable.then(|| {
            components::button("notifications-clear")
                .child(
                    div()
                        .text_size(px(FS_SM))
                        .text_color(rgb(TEXT_MUTED))
                        .child("Clear"),
                )
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    handle(Verb::Clear, window, cx)
                })
        }))
}

fn row_element(index: usize, row: &Row, handle: Handle) -> Stateful<Div> {
    let id = row.id;
    let open = handle.clone();
    let dot = div()
        .flex_shrink_0()
        .w(px(STATUS_DOT))
        .h(px(STATUS_DOT))
        .rounded_full()
        .bg(if row.read {
            rgba(0)
        } else if row.failed() {
            rgb(BLOCKED)
        } else {
            rgb(ATTENTION)
        });
    div()
        .id(("notice-row", index))
        .flex()
        .items_center()
        .w_full()
        .min_h(px(MENU_ROW_H))
        .px(px(ROW_PAD_X))
        .py(px(MENU_PAD))
        .gap(px(ROW_PAD_X))
        .rounded(px(R_CONTROL))
        .hover_row()
        .press_row()
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            open(Verb::Open(id), window, cx)
        })
        .child(dot)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(FS_MD))
                        .line_height(gpui::relative(LINE_TIGHT))
                        .text_color(rgb(if row.read { TEXT_2 } else { TEXT_STRONG }))
                        .when(!row.read, |title| title.font_weight(FontWeight::MEDIUM))
                        .truncate()
                        .child(row.title.clone()),
                )
                .child(
                    div()
                        .text_size(px(FS_SM))
                        .line_height(gpui::relative(LINE_TIGHT))
                        .text_color(rgb(TEXT_MUTED))
                        .truncate()
                        .child(row.detail()),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(FS_SM))
                .text_color(rgb(TEXT_MUTED))
                .child(row.when.clone()),
        )
        .child(
            components::button(("notice-dismiss", index))
                .p_0()
                .w(px(ICON_BUTTON_GLYPH))
                .h(px(ICON_BUTTON_GLYPH))
                .child(
                    Icon::new(IconName::Close)
                        .size(px(FS_SM))
                        .text_color(rgb(TEXT_MUTED)),
                )
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    handle(Verb::Dismiss(id), window, cx)
                }),
        )
}

/// The floating-menu ground every popover here stands on: `--menu`, the
/// 10px radius, the far and near shadows. 340px wide, so a row holds a
/// title, a detail line and its age without wrapping.
fn surface() -> Div {
    div()
        .flex()
        .flex_col()
        .w(px(340.))
        .max_h(px(420.))
        .gap(px(ROW_GAP))
        .p(px(MENU_PAD))
        .rounded(px(R_MENU))
        .bg(rgb(MENU))
        .font_family(FONT_UI)
        .text_color(rgb(TEXT))
        .shadow(vec![
            BoxShadow {
                inset: false,
                color: rgba(SHADOW_FAR).into(),
                offset: point(px(0.), px(SHADOW_FAR_Y)),
                blur_radius: px(SHADOW_FAR_BLUR),
                spread_radius: px(SHADOW_FAR_SPREAD),
            },
            BoxShadow {
                inset: false,
                color: rgba(SHADOW_NEAR).into(),
                offset: point(px(0.), px(SHADOW_NEAR_Y)),
                blur_radius: px(SHADOW_NEAR_BLUR),
                spread_radius: px(0.),
            },
        ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(outcome: TurnOutcome, project: Option<&str>) -> Row {
        Row {
            id: NoticeId::from_u64(1),
            thread: ThreadId::new(3),
            title: "fix the bell".into(),
            project: project.map(SharedString::from),
            outcome,
            when: "now".into(),
            read: false,
        }
    }

    #[test]
    fn a_rows_detail_names_the_outcome_and_the_project() {
        let done = row(TurnOutcome::Completed, Some("ferrite"));
        assert_eq!(done.detail(), SharedString::from("Finished · ferrite"));
        assert!(!done.failed());
        let failed = row(TurnOutcome::Error("rate limited".into()), None);
        assert_eq!(failed.detail(), SharedString::from("Failed · rate limited"));
        assert!(failed.failed());
    }
}
