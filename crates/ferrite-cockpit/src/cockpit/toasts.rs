//! The toast float (FL-18, ADR 0017): the newest standing toast, top right
//! of the transcript's column, over the face of the one before it while
//! that fades away.
//!
//! Its state lives in a view of its own (ADR 0041), `ToastsView`: what the
//! Inbox has heard of each machine, what waits out its delay, the toasts
//! standing and the system notifications posted (`NeedsYou`), and the
//! toast on its way out (`ToastStage`). The root's pump hears the machines
//! and rings it (`ring_alerts`), and plays its chimes and posts its system
//! notifications. The float reads its row and its place through
//! `toast_inputs`; what a toast's button asks it says as a `ToastEvent`.
//! The kit's toasts stay the root's (`toast_margins`).

use gpui::WeakEntity;

use super::*;
use crate::float::{self, FloatPlace};
use crate::notifications::{Alert, QuickAnswer, ToastStage};
use ferrite_core::notifications::DecisionNoticeId;

/// What a toast's buttons ask of the window.
pub(crate) enum ToastEvent {
    /// `Allow 1` / `Deny 3` on an approval's toast.
    Answer(DecisionNoticeId, QuickAnswer),
    /// `Open ⏎`: what the toast is about.
    Open(Verb),
}

/// What the float draws from the window.
pub(crate) struct ToastInputs {
    /// The newest standing toast in its row's words: none while the nav is
    /// open, or once its record is gone.
    row: Option<NoticeRow>,
    /// The nav's width.
    nav_w: f32,
    /// From the window's right edge to the board's: the panel's left while
    /// it shows.
    board_right: f32,
}

/// The Inbox's alerts and the toast float that shows them.
pub(crate) struct ToastsView {
    cockpit: WeakEntity<CockpitView>,
    /// What was heard, what waits, the toasts standing, what is posted.
    pub(crate) needs_you: NeedsYou,
    /// What the float drew last: the toast standing and the one fading out.
    stage: ToastStage,
}

impl EventEmitter<ToastEvent> for ToastsView {}

impl ToastsView {
    pub(super) fn new(cockpit: WeakEntity<CockpitView>) -> Self {
        Self {
            cockpit,
            needs_you: NeedsYou::new(),
            stage: ToastStage::default(),
        }
    }

    /// A toast's buttons: an answered or opened toast goes at once
    /// (`ToastStage::dismiss`), and the window hears what was asked.
    fn handle(cx: &mut Context<Self>) -> Handle {
        let this = cx.weak_entity();
        std::rc::Rc::new(move |verb, _, cx| {
            let _ = this.update(cx, |toasts, cx| {
                toasts.stage.dismiss();
                cx.emit(match verb {
                    Verb::Answer(id, answer) => ToastEvent::Answer(id, answer),
                    verb => ToastEvent::Open(verb),
                });
            });
        })
    }
}

impl Render for ToastsView {
    /// The newest standing toast, top right of the transcript's column
    /// where it covers the least (theme `TOAST_*`): under the Pane heads,
    /// clear of every Composer, and never over the right panel. It drops
    /// in when it comes, over the face of the one before it while that
    /// fades away (`notifications::ToastStage`).
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(cockpit) = self.cockpit.upgrade() else {
            return div();
        };
        let standing = self.needs_you.toasts().last().map(|toast| &toast.alert);
        let ToastInputs {
            row: standing,
            nav_w,
            board_right,
        } = cockpit.read(cx).toast_inputs(standing, cx);
        let leaving = self.stage.stage(
            standing.as_ref(),
            cx.background_executor().now(),
            ferrite_ui::motion::reduced_motion(cx),
        );
        use ferrite_ui::theme::{PANE_HEAD_H, TOAST_BOX_RIGHT, TOAST_W, WIN_CHROME_H};
        let viewport = window.viewport_size();
        // What it hangs under, at its width and right edge: the window's
        // head and a Pane's head row, `TOAST_BOX_RIGHT` in from the board's
        // right edge.
        let right = board_right + TOAST_BOX_RIGHT;
        let room = f32::from(viewport.width) - nav_w - right - TOAST_BOX_RIGHT;
        let width = TOAST_W.min(room.max(0.));
        let over = WIN_CHROME_H + PANE_HEAD_H;
        let anchor = gpui::Bounds::new(
            gpui::point(viewport.width - px(right + width), px(0.)),
            gpui::size(px(width), px(over)),
        );
        let hang = |toast: AnyElement| {
            float::hang_element(
                toast,
                anchor,
                FloatPlace::Below {
                    gap: ferrite_ui::theme::TOAST_TOP,
                },
            )
        };
        let leaving = leaving.map(|(row, left)| {
            hang(
                ferrite_ui::motion::toast_out(left, crate::notifications::toast_face(&row, width))
                    .into_any_element(),
            )
        });
        let standing = standing.map(|row| {
            let toast = crate::notifications::toast(&row, Self::handle(cx), width);
            hang(
                ferrite_ui::motion::toast_in(crate::notifications::toast_key(&row), toast)
                    .into_any_element(),
            )
        });
        div().children(leaving.into_iter().chain(standing))
    }
}

impl CockpitView {
    /// What the toast float draws: the newest standing toast's row while
    /// the nav is folded, and what it hangs beside.
    pub(super) fn toast_inputs(&self, standing: Option<&Alert>, cx: &gpui::App) -> ToastInputs {
        let machines = self.machines.as_ref().map(|machines| machines.read(cx));
        ToastInputs {
            row: standing
                .filter(|_| self.nav_folded())
                .and_then(|alert| self.alert_row(alert, machines, ferrite_core::clock::system_time())),
            nav_w: self.nav_width(),
            board_right: self.panel_column_w() + self.task_panel_w(cx),
        }
    }

    /// A toast's button: its quick answer, or what it opens.
    pub(super) fn on_toast_event(
        &mut self,
        _: &Entity<ToastsView>,
        event: &ToastEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ToastEvent::Answer(id, answer) => self.answer_from_toast(id.clone(), *answer, window, cx),
            ToastEvent::Open(verb) => self.notice_verb(verb.clone(), cx),
        }
    }

    /// Take one toast down (its button, or the list, acted on it).
    pub(super) fn drop_toast(&mut self, target: &RowTarget, cx: &mut Context<Self>) {
        self.toasts.update(cx, |toasts, _| toasts.needs_you.drop_toast(target));
    }

    /// Keep the kit's toast stack (`theme.notification`) on the
    /// transcript's column, as the toast float keeps: its right margin
    /// follows the panel, written only when it moves.
    pub(super) fn toast_margins(&self, cx: &mut gpui::App) {
        let right =
            px(self.panel_column_w() + self.task_panel_w(cx) + ferrite_ui::theme::TOAST_BOX_RIGHT);
        if gpui::component::Theme::global(cx)
            .notification
            .margins
            .right
            != right
        {
            gpui::component::Theme::global_mut(cx)
                .notification
                .margins
                .right = right;
        }
    }
}
