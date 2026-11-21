//! The toast float (FL-18, ADR 0017): the newest standing toast, top right
//! of the transcript's column, over the face of the one before it while
//! that fades away.
//!
//! Its state lives in a view of its own (ADR 0041), `ToastsView`: what the
//! Inbox has heard of each machine, what waits out its delay, the toasts
//! standing and the system notifications posted (`NeedsYou`), and the
//! toast on its way out (`ToastStage`). The root's pump hears the machines
//! and rings it (`ring_alerts`), and plays its chimes and posts its system
//! notifications. The float reads its toast's row itself (`look`), as the
//! client's model says something `Arrived`, the window or its machines
//! notify, or a ring moves its toasts; the window hands it where it hangs
//! as it lays out its columns (`place_toasts`). It redraws only when one
//! of them moved. What a toast's button asks it says as a `ToastEvent`.
//! The kit's toasts stay the root's.

use gpui::WeakEntity;

use super::*;
use crate::float::{self, FloatPlace};
use crate::notifications::{QuickAnswer, ToastStage};
use ferrite_core::notifications::DecisionNoticeId;

/// What a toast's buttons ask of the window.
pub(crate) enum ToastEvent {
    /// `Allow 1` / `Deny 3` on an approval's toast.
    Answer(DecisionNoticeId, QuickAnswer),
    /// `Open ⏎`: what the toast is about.
    Open(Verb),
}

/// What the float draws, as it last read it.
#[derive(Default, PartialEq)]
struct ToastInputs {
    /// The newest standing toast in its row's words: none once its record
    /// is gone.
    row: Option<NoticeRow>,
    place: Place,
}

/// Where the float hangs, as the window laid out its columns this frame.
#[derive(Clone, Copy, Default, PartialEq)]
pub(super) struct Place {
    /// The nav is folded: only then does a toast show.
    pub folded: bool,
    /// The nav's width.
    pub nav_w: f32,
    /// From the window's right edge to the board's: the panel's left while
    /// it shows.
    pub board_right: f32,
}

/// The Inbox's alerts and the toast float that shows them.
pub(crate) struct ToastsView {
    cockpit: WeakEntity<CockpitView>,
    /// The client's model, whose `Arrived` has the float look again.
    model: Entity<Cockpit>,
    /// What was heard, what waits, the toasts standing, what is posted.
    /// Whoever changes its toasts has the float look again (`look_again`).
    pub(crate) needs_you: NeedsYou,
    /// What the float drew last: the toast standing and the one fading out.
    stage: ToastStage,
    /// What the float draws, as it last read it (`look`, `place`). Its
    /// render reads nothing else, so only a change here redraws it.
    inputs: ToastInputs,
    /// The Cockpit's notify, its machines' and the model's `Arrived`, each
    /// of which has the float look again.
    watching: Vec<gpui::Subscription>,
}

impl EventEmitter<ToastEvent> for ToastsView {}

impl ToastsView {
    pub(super) fn new(
        cockpit: WeakEntity<CockpitView>,
        model: Entity<Cockpit>,
        cx: &mut Context<Self>,
    ) -> Self {
        // The Cockpit is still being made, and its machines are handed to
        // it after: the float first looks once it is.
        let toasts = cx.weak_entity();
        cx.defer(move |cx| {
            toasts.update(cx, |toasts, cx| toasts.watch(cx)).ok();
        });
        Self {
            cockpit,
            model,
            needs_you: NeedsYou::new(),
            stage: ToastStage::default(),
            inputs: ToastInputs::default(),
            watching: Vec::new(),
        }
    }

    fn watch(&mut self, cx: &mut Context<Self>) {
        let Some(cockpit) = self.cockpit.upgrade() else {
            return;
        };
        let machines = cockpit.read(cx).machines();
        self.watching = vec![
            cx.observe(&cockpit, |toasts, _, cx| toasts.look(cx)),
            cx.subscribe(&self.model, |toasts, _, _: &Arrived, cx| toasts.look(cx)),
        ];
        self.watching.extend(
            machines.map(|machines| cx.observe(&machines, |toasts, _, cx| toasts.look(cx))),
        );
        self.look(cx);
    }

    /// Read the newest standing toast's row, and redraw when it moved. It
    /// runs as the Cockpit, its machines or the model say something, never
    /// in a draw.
    fn look(&mut self, cx: &mut Context<Self>) {
        let Some(cockpit) = self.cockpit.upgrade() else {
            return;
        };
        let view = cockpit.read(cx);
        let machines = view.machines.as_ref().map(|machines| machines.read(cx));
        let row = self.needs_you.toasts().last().and_then(|toast| {
            view.alert_row(&toast.alert, machines, ferrite_core::clock::system_time(), cx)
        });
        if self.inputs.row != row {
            self.inputs.row = row;
            cx.notify();
        }
    }

    /// The window laid out its columns: where the float hangs this frame.
    pub(super) fn place(&mut self, place: Place, cx: &mut Context<Self>) {
        if self.inputs.place != place {
            self.inputs.place = place;
            cx.notify();
        }
    }

    /// `look`, once the update under way has ended: what the Cockpit's own
    /// code asks as it changes the toasts standing (a ring, a toast taken
    /// down), while the Cockpit may still be borrowed.
    pub(super) fn look_again(&mut self, cx: &mut Context<Self>) {
        let toasts = cx.weak_entity();
        cx.defer(move |cx| {
            toasts.update(cx, |toasts, cx| toasts.look(cx)).ok();
        });
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
        #[cfg(test)]
        super::drawn::built_toasts();
        let Place {
            folded,
            nav_w,
            board_right,
        } = self.inputs.place;
        let standing = self.inputs.row.clone().filter(|_| folded);
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
        self.toasts.update(cx, |toasts, cx| {
            toasts.needs_you.drop_toast(target);
            toasts.look_again(cx);
        });
    }

    /// Hand the toast float where it hangs this frame, as the columns are
    /// laid out, and keep the kit's toast stack (`theme.notification`) on
    /// the transcript's column with it: its right margin follows the
    /// panel, written only when it moves.
    pub(super) fn place_toasts(&self, cx: &mut Context<Self>) {
        let board_right = self.panel_column_w(cx) + self.task_panel_w(cx);
        let place = Place {
            folded: self.nav_folded(),
            nav_w: self.nav_width(),
            board_right,
        };
        // The Cockpit is drawing: the float takes its place as it is.
        self.toasts.update(cx, |toasts, cx| toasts.place(place, cx));
        let right = px(board_right + ferrite_ui::theme::TOAST_BOX_RIGHT);
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
