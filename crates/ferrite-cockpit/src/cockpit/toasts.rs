//! The toast float (FL-18, ADR 0017): the newest standing toast, top right
//! of the transcript's column, over the face of the one before it while
//! that fades away.
//!
//! Its state lives in a view of its own (ADR 0041), `ToastsView`: what the
//! Inbox has heard of each machine, what waits out its delay, the toasts
//! standing and the system notifications posted (`NeedsYou`), and the
//! toast on its way out (`ToastStage`). It hears every machine's Notices
//! itself (`tend`), as the client's model says something `Arrived`, the
//! window notifies, and on a clock of its own for the next alert's delay
//! or toast's end. What it rings it hands the
//! window as a `ToastEvent` (the chime, the system notifications posted and
//! taken back). The float reads its toast's row as it tends, and the
//! window hands it where it hangs as it lays out its columns
//! (`place_toasts`). It redraws only when one of them moved. What a
//! toast's button asks it says as a `ToastEvent` too. The kit's toasts
//! stay the root's.

use gpui::WeakEntity;

use super::*;
use crate::float::{self, FloatPlace};
use crate::notifications::{Alert, QuickAnswer, ToastStage};
use crate::system_notifications::Posting;
use ferrite_core::notifications::{DecisionNoticeId, NoticeId, Notifications};
use ferrite_client::cockpit::OnMachine;
use ferrite_core::MachineId;
use std::time::Instant;

/// What the toasts ask of the window.
pub(crate) enum ToastEvent {
    /// `Allow 1` / `Deny 3` on an approval's toast, on its machine.
    Answer(MachineId, DecisionNoticeId, QuickAnswer),
    /// `Open ⏎`: what the toast is about.
    Open(Verb),
    /// What a `tend` rang and what went, for the window to play and post.
    Rang(Rang),
}

/// What one `tend` leaves to the window: the system notifications to take
/// back (by tag), the chime, and the system notifications to post.
pub(crate) struct Rang {
    withdraw: Vec<String>,
    sound: Option<Sound>,
    post: Vec<Posting>,
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
    /// From the window's right edge to where a toast may stand: the panel's
    /// left while it shows, and left of a Details card in the way.
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
    /// When the alerts next have work of the clock's own, and the timer
    /// that tends them then (`set_clock`).
    clock: Option<(Instant, gpui::Task<()>)>,
    /// The Cockpit's notify and the model's `Arrived`, each of which has
    /// the toasts tend.
    watching: Vec<gpui::Subscription>,
}

impl EventEmitter<ToastEvent> for ToastsView {}

impl ToastsView {
    pub(super) fn new(
        cockpit: WeakEntity<CockpitView>,
        model: Entity<Cockpit>,
        cx: &mut Context<Self>,
    ) -> Self {
        // The Cockpit is still being made: the float first looks once it
        // is.
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
            clock: None,
            watching: Vec::new(),
        }
    }

    fn watch(&mut self, cx: &mut Context<Self>) {
        let Some(cockpit) = self.cockpit.upgrade() else {
            return;
        };
        self.watching = vec![
            cx.observe(&cockpit, |toasts, _, cx| toasts.tend(cx)),
            cx.subscribe(&self.model, |toasts, _, _: &Arrived, cx| toasts.tend(cx)),
        ];
        self.tend(cx);
    }

    /// Hear what every machine's Notices say, ring what waited out its
    /// delay, take down what went, and set the clock for the next of them;
    /// then look. It runs as the model says something `Arrived`, the window
    /// notifies, and on that clock, never on a pump that brought nothing.
    ///
    /// The operator is looking at an event, and nothing rings for it, no
    /// system notification either, while the window has focus and its
    /// Thread is on the board (focus-notify's rule, ADR 0005's update).
    fn tend(&mut self, cx: &mut Context<Self>) {
        let Some(cockpit) = self.cockpit.upgrade() else {
            return;
        };
        #[cfg(test)]
        super::drawn::heard();
        let now = cx.background_executor().now();
        let view = cockpit.read(cx);
        let needs_you = &mut self.needs_you;
        let model = self.model.read(cx);
        // Until its Server answers, a machine's Notices are none, not news:
        // hearing them would forget the requests heard of it. A machine
        // other than this computer is first heard quietly: what stood
        // before the window held it is not news either.
        for notifications in model.every_notifications() {
            let machine = notifications.machine();
            if !view.attached(machine, cx) {
                continue;
            }
            let (fresh, requests) = news(notifications, needs_you.presented(machine), model);
            needs_you.hear(machine, !model.local(machine), fresh, requests, now);
        }
        let live = |alert: &Alert| super::machines::wanted(alert, view, cx);
        // A system notification the operator has since read, or whose
        // request resolved, is taken back out of the notification centre.
        let withdraw: Vec<String> = needs_you
            .posted
            .iter()
            .filter(|(_, alert)| live(alert) == Some(false))
            .map(|(tag, _)| tag.clone())
            .collect();
        let (mut sound, mut post) = (None, Vec::new());
        if needs_you.busy() {
            let present = model.window_active();
            let on_board: std::collections::HashSet<ThreadKey> = view
                .visible_indices(cx)
                .into_iter()
                .filter_map(|index| view.panes[index].thread())
                .collect();
            let in_view = |alert: &Alert| present && on_board.contains(&alert.thread);
            let ring = needs_you.ring(now, &live, &in_view);
            // Alerts that ring together chime once, needs-you over done.
            let settings = &view.prefs.settings;
            sound = ring
                .rang
                .iter()
                .filter(|alert| {
                    view.alert_provider(alert, cx)
                        .is_some_and(|provider| settings.sounds_for(provider))
                })
                .map(|alert| {
                    if alert.needs_you() {
                        Sound::NeedsYou
                    } else {
                        Sound::Done
                    }
                })
                .max_by_key(|sound| *sound == Sound::NeedsYou);
            if settings.system_notifications {
                let wall = ferrite_core::clock::system_time();
                let focused = view.focused_thread(cx);
                post = ring
                    .rang
                    .into_iter()
                    .filter(|alert| {
                        let focused = focused == Some(alert.thread);
                        crate::system_notifications::wanted(focused, present)
                    })
                    .filter_map(|alert| Some((view.system_posting(&alert, wall, cx)?, alert)))
                    .collect();
            }
        }
        for tag in &withdraw {
            needs_you.posted.remove(tag);
        }
        for (posting, alert) in &post {
            needs_you.posted.insert(posting.tag.clone(), alert.clone());
        }
        self.set_clock(now, cx);
        self.look(cx);
        if !withdraw.is_empty() || sound.is_some() || !post.is_empty() {
            cx.emit(ToastEvent::Rang(Rang {
                withdraw,
                sound,
                post: post.into_iter().map(|(posting, _)| posting).collect(),
            }));
        }
    }

    /// Have `tend` run again when the alerts next have work of the clock's
    /// own (`NeedsYou::next_due`): a delay up, a toast's end. One timer,
    /// set again only when that time moved.
    fn set_clock(&mut self, now: Instant, cx: &mut Context<Self>) {
        let due = self.needs_you.next_due(now);
        if self.clock.as_ref().map(|(at, _)| *at) == due {
            return;
        }
        self.clock = due.map(|at| {
            let timer = cx.spawn(async move |toasts, cx| {
                cx.background_executor()
                    .timer(at.saturating_duration_since(now))
                    .await;
                toasts
                    .update(cx, |toasts, cx| {
                        toasts.clock = None;
                        toasts.tend(cx);
                    })
                    .ok();
            });
            (at, timer)
        });
    }

    /// Read the newest standing toast's row, and redraw when it moved. It
    /// runs as the toasts tend or their toasts change, never in a draw.
    fn look(&mut self, cx: &mut Context<Self>) {
        let Some(cockpit) = self.cockpit.upgrade() else {
            return;
        };
        let view = cockpit.read(cx);
        let row = self.needs_you.toasts().last().and_then(|toast| {
            view.alert_row(&toast.alert, ferrite_core::clock::system_time(), cx)
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

    /// Hear what stands now as heard and toast none of it: a scene's
    /// requests that stood before the window opened. Once the update under
    /// way has ended, since hearing reads the Cockpit.
    #[cfg(feature = "visual-reference")]
    pub(super) fn hush(&mut self, cx: &mut Context<Self>) {
        let toasts = cx.weak_entity();
        cx.defer(move |cx| {
            toasts
                .update(cx, |toasts, cx| {
                    toasts.tend(cx);
                    toasts.needs_you.clear_toasts();
                    toasts.set_clock(cx.background_executor().now(), cx);
                    toasts.look(cx);
                })
                .ok();
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
                    Verb::Answer(machine, id, answer) => ToastEvent::Answer(machine, id, answer),
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
            ToastEvent::Answer(machine, id, answer) => {
                self.answer_from_toast(*machine, id.clone(), *answer, window, cx)
            }
            ToastEvent::Open(verb) => self.notice_verb(verb.clone(), cx),
            ToastEvent::Rang(Rang {
                withdraw,
                sound,
                post,
            }) => {
                for tag in withdraw {
                    crate::system_notifications::withdraw(tag, cx);
                }
                if let Some(sound) = *sound {
                    self.chime(sound);
                }
                for posting in post {
                    crate::system_notifications::post(posting.clone(), cx);
                }
            }
        }
    }

    /// Take one toast of `machine` down (its button, or the list, acted on
    /// it).
    pub(super) fn drop_toast(&mut self, machine: MachineId, target: &RowTarget, cx: &mut Context<Self>) {
        self.toasts.update(cx, |toasts, cx| {
            toasts.needs_you.drop_toast(machine, target);
            toasts.look_again(cx);
        });
    }

    /// Hand the toast float where it hangs this frame, as the columns are
    /// laid out.
    pub(super) fn place_toasts(&self, window: &Window, cx: &mut Context<Self>) {
        let place = Place {
            folded: self.nav_folded(),
            nav_w: self.nav_width(),
            board_right: self.toast_board_right(window, cx),
        };
        // The Cockpit is drawing: the float takes its place as it is.
        self.toasts.update(cx, |toasts, cx| toasts.place(place, cx));
    }

    /// The kit's toasts (the root's), over the sheets and under every menu
    /// (`ferrite_ui::layer`), where `toast_home` puts them: drawn again at
    /// once when the Panes, laid out, moved their home.
    pub(super) fn kit_toasts(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let home = self.toast_home(window, cx);
        let margins = &gpui::component::Theme::global(cx).notification.margins;
        if (margins.right, margins.bottom) != home {
            let margins = &mut gpui::component::Theme::global_mut(cx).notification.margins;
            (margins.right, margins.bottom) = home;
        }
        let layer = gpui::component::Root::render_notification_layer(window, cx)?;
        let view = cx.weak_entity();
        let layer = ferrite_ui::components::on_bounds(div().absolute().inset_0().child(layer), move |_, window, cx| {
            if view.upgrade().is_some_and(|view| view.read(cx).toast_home(window, cx) != home) {
                ferrite_ui::components::redraw_drawn_view(window, cx);
            }
        });
        Some(deferred(layer).with_priority(ferrite_ui::layer::TOAST).into_any_element())
    }

    /// Where the kit's toasts stand, as their margins from the window's
    /// right edge and foot: over the Composer of the board's lower right
    /// Pane, their right edge on its box's and their foot `TOAST_TOP` over
    /// its stack, or over the row the operator answers at its transcript's
    /// foot when they would reach that, though never over the Pane's head.
    /// With no Composer there, or the Factory or the pull requests in the
    /// board's place, the board's lower right corner (`TOAST_FOOT`). A
    /// sheet up holds the keys and the pointer: the row under it waits, and
    /// the toasts stay home rather than rise into its card.
    pub(super) fn toast_home(&self, window: &Window, cx: &gpui::App) -> (Pixels, Pixels) {
        use ferrite_ui::theme::{PANE_HEAD_H, TOAST_BOX_RIGHT, TOAST_FOOT, TOAST_KIT_H, TOAST_REACH, TOAST_TOP};
        let viewport = window.viewport_size();
        let corner = |(_, a): &(usize, layout::Rect), (_, b): &(usize, layout::Rect)| {
            (a.x + a.w, a.y + a.h).partial_cmp(&(b.x + b.w, b.y + b.h)).unwrap_or(std::cmp::Ordering::Equal)
        };
        // A Pane the board does not show keeps where it was last drawn.
        let panes_shown =
            self.cockpit.read(cx).environment_available() && self.factory().is_none() && !self.prs_shown();
        let panes = if panes_shown { self.pane_rects(window, cx) } else { Vec::new() };
        let answerable = !self.sheet_up(cx);
        let over_composer = panes.into_iter().max_by(corner).and_then(|(index, pane)| {
            let geometry = self.panes[index].geometry.get();
            let (field, stack) = (geometry.composer?, geometry.stack?);
            let reach = stack.top() - px(TOAST_TOP + TOAST_REACH);
            let clear = geometry
                .answers
                .filter(|answers| answerable && answers.bottom() > reach)
                .map_or(stack.top(), |answers| answers.top().min(stack.top()));
            let highest = px(pane.y + PANE_HEAD_H + TOAST_TOP + TOAST_KIT_H);
            Some((field.right(), (clear - px(TOAST_TOP)).max(highest)))
        });
        let (right, foot) = over_composer.unwrap_or_else(|| {
            let right = px(self.toast_board_right(window, cx) + TOAST_BOX_RIGHT);
            (viewport.width - right, viewport.height - px(TOAST_FOOT))
        });
        (viewport.width - right, viewport.height - foot)
    }

    /// How far in from the window's right edge the board ends for a toast
    /// in its corner: short of the panel, an open Task's, and the Details
    /// cards standing there.
    fn toast_board_right(&self, window: &Window, cx: &gpui::App) -> f32 {
        self.panel_column_w(cx) + self.task_panel_w(cx) + self.details_toast_inset(window, cx)
    }
}

type FreshNotice = (NoticeId, ThreadKey, bool);
type LiveRequest = (DecisionNoticeId, ThreadKey, bool);

/// What a machine's Needs-you state may not have heard yet: its Notices
/// born after `after` (id, Thread, read) and its live requests (id,
/// Thread, read).
fn news(
    notifications: OnMachine<'_, Notifications>,
    after: Option<NoticeId>,
    model: &Cockpit,
) -> (Vec<FreshNotice>, Vec<LiveRequest>) {
    // An owned Thread's Notices and requests ring nothing: its owner
    // (a Task, a Factory) tells the operator in its own way (ADR 0037).
    // Its Notices still move the watermark, so they are never news later.
    let fresh = notifications
        .since(after)
        .map(|notice| {
            let thread = notifications.key(notice.thread);
            (notice.id, thread, notice.read || !model.operators(thread))
        })
        .collect();
    let requests = notifications
        .decisions()
        .filter_map(|notice| {
            let thread = notifications.key(notice.id.thread);
            model
                .operators(thread)
                .then(|| (notice.id.clone(), thread, notice.read))
        })
        .collect();
    (fresh, requests)
}
