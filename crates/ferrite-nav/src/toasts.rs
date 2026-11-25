//! The toast float (FL-18, ADR 0017): the newest standing toast, top right
//! of the transcript's column, over the face of the one before it while
//! that fades away.
//!
//! Its state lives in a view of its own (ADR 0041), `ToastsView`: what the
//! Inbox has heard of each machine, what waits out its delay, the toasts
//! standing and the system notifications posted (`NeedsYou`), and the
//! toast on its way out (`ToastStage`). It reads what each machine's
//! Notices say, and what it must know of an alert (whether it still wants
//! the operator, whether it is in view, its chime, its system
//! notification), through a `Source` the window makes over itself
//! (`ToastSources::tending`), and tends itself (`tend`) as often as the
//! window says (`watch`) and on a clock of its own for the next alert's
//! delay or toast's end. What it rings it hands the window as a
//! `ToastEvent` (the chime, the system notifications posted and taken
//! back). The float reads its toast's row through the other `Source` as
//! it tends, and the window hands it where it hangs as it lays out its
//! columns (`place`). It redraws only when one of them moved. What a
//! toast's button asks it says as a `ToastEvent` too. The kit's toasts
//! stay the window's.

use std::collections::HashMap;
use std::time::Instant;

use ferrite_core::notifications::{DecisionNoticeId, NoticeId};
use ferrite_core::sound::Sound;
use ferrite_core::{MachineId, ThreadKey};
use ferrite_ui::float::{self, FloatPlace};
use ferrite_ui::source::Source;
use gpui::prelude::*;
use gpui::{div, px, AnyElement, Context, EventEmitter, Subscription, Window};

use crate::notifications::{
    self, Alert, Handle, NeedsYou, Posting, QuickAnswer, Row, RowTarget, ToastStage, Verb,
};

/// What the toasts ask of the window.
pub enum ToastEvent {
    /// `Allow 1` / `Deny 3` on an approval's toast, on its machine.
    Answer(MachineId, DecisionNoticeId, QuickAnswer),
    /// `Open ⏎`: what the toast is about.
    Open(Verb),
    /// What a `tend` rang and what went, for the window to play and post.
    Rang(Rang),
}

/// What one `tend` leaves to the window: the system notifications to take
/// back (by tag), the chime, and the system notifications to post.
#[derive(Debug, Default, PartialEq)]
pub struct Rang {
    pub withdraw: Vec<String>,
    pub sound: Option<Sound>,
    pub post: Vec<Posting>,
}

/// What a `tend` asks the window.
pub struct Asked {
    /// Each machine's watermark: its Notices at or below it were heard.
    pub presented: HashMap<MachineId, NoticeId>,
    /// The toasts standing, the events waiting and the system
    /// notifications posted (`NeedsYou::standing`).
    pub standing: Vec<Alert>,
    /// The events whose delay is up (`NeedsYou::due`): what may ring.
    pub due: Vec<Alert>,
}

/// What the window answers a `tend`.
pub struct Tending {
    /// What each machine the window holds and hears said since its
    /// watermark, in the window's order of its machines.
    pub news: Vec<News>,
    /// Of each of `Asked::standing`, in its order.
    pub standing: Vec<Standing>,
    /// Of each of `Asked::due`, in its order.
    pub due: Vec<Due>,
}

/// One machine's news: its Notices born after its watermark (id, Thread,
/// read) and its live requests (id, Thread, read), as `NeedsYou::hear`
/// takes them. A machine other than this computer is first heard quietly.
pub struct News {
    pub machine: MachineId,
    pub quiet_first: bool,
    pub fresh: Vec<(NoticeId, ThreadKey, bool)>,
    pub requests: Vec<(DecisionNoticeId, ThreadKey, bool)>,
}

/// What the window says of an alert standing.
#[derive(Clone, Copy)]
pub struct Standing {
    /// Whether its record still wants the operator; `None` while its
    /// machine has not answered.
    pub live: Option<bool>,
    /// The operator is looking at it: the window has focus and its Thread
    /// is on the board.
    pub in_view: bool,
}

/// What the window says of an alert that may ring.
#[derive(Clone)]
pub struct Due {
    /// Its Thread's provider chimes.
    pub chimes: bool,
    /// Its system notification, when one is wanted for it.
    pub posting: Option<Posting>,
}

/// What the toasts read of the window.
pub struct ToastSources {
    /// What a `tend` hears and what it must know of its alerts.
    pub tending: Source<Tending, Asked>,
    /// An alert's row, while its record is there to word it.
    pub row: Source<Row, Alert>,
}

/// What the float draws, as it last read it.
#[derive(Default, PartialEq)]
struct ToastInputs {
    /// The newest standing toast in its row's words: none once its record
    /// is gone.
    row: Option<Row>,
    place: Place,
}

/// Where the float hangs, as the window laid out its columns this frame.
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Place {
    /// The nav is folded: only then does a toast show.
    pub folded: bool,
    /// The nav's width.
    pub nav_w: f32,
    /// From the window's right edge to where a toast may stand: the panel's
    /// left while it shows, and left of a Details card in the way.
    pub board_right: f32,
}

/// The Inbox's alerts and the toast float that shows them.
pub struct ToastsView {
    sources: ToastSources,
    /// What was heard, what waits, the toasts standing, what is posted.
    /// Whoever changes its toasts has the float look again (`look_again`).
    needs_you: NeedsYou,
    /// What the float drew last: the toast standing and the one fading out.
    stage: ToastStage,
    /// What the float draws, as it last read it (`look`, `place`). Its
    /// render reads nothing else, so only a change here redraws it.
    inputs: ToastInputs,
    /// When the alerts next have work of the clock's own, and the timer
    /// that tends them then (`set_clock`).
    clock: Option<(Instant, gpui::Task<()>)>,
    /// What the window has the toasts tend on (`watch`).
    watching: Vec<Subscription>,
}

impl EventEmitter<ToastEvent> for ToastsView {}

impl ToastsView {
    pub fn new(sources: ToastSources) -> Self {
        Self {
            sources,
            needs_you: NeedsYou::new(),
            stage: ToastStage::default(),
            inputs: ToastInputs::default(),
            clock: None,
            watching: Vec::new(),
        }
    }

    /// Tend as each of `wakes` fires (the window's notify, the model's
    /// `Arrived`), and now.
    pub fn watch(&mut self, wakes: Vec<Subscription>, cx: &mut Context<Self>) {
        self.watching = wakes;
        self.tend(cx);
    }

    /// What was heard, what waits, the toasts standing, what is posted.
    pub fn needs_you(&self) -> &NeedsYou {
        &self.needs_you
    }

    /// Hear what every machine's Notices say, ring what waited out its
    /// delay, take down what went, and set the clock for the next of them;
    /// then look. It runs as the window says (`watch`) and on that clock,
    /// never on a pump that brought nothing.
    ///
    /// The operator is looking at an event, and nothing rings for it, no
    /// system notification either, while the window has focus and its
    /// Thread is on the board (focus-notify's rule, ADR 0005's update).
    pub fn tend(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        let asked = Asked {
            presented: self.needs_you.watermarks(),
            standing: self.needs_you.standing(),
            due: self.needs_you.due(now),
        };
        let Some(Tending {
            news,
            standing,
            due,
        }) = self.sources.tending.read(&asked, cx)
        else {
            return;
        };
        let needs_you = &mut self.needs_you;
        for News {
            machine,
            quiet_first,
            fresh,
            requests,
        } in news
        {
            needs_you.hear(machine, quiet_first, fresh, requests, now);
        }
        // The window answered each asked alert in the order asked; read
        // the answers by the alert, once.
        let mut standing_by: HashMap<&Alert, Standing> = HashMap::new();
        for (alert, answer) in asked.standing.iter().zip(standing) {
            standing_by.entry(alert).or_insert(answer);
        }
        let mut due_by: HashMap<&Alert, Due> = HashMap::new();
        for (alert, answer) in asked.due.iter().zip(due) {
            due_by.entry(alert).or_insert(answer);
        }
        let standing_of = |alert: &Alert| standing_by.get(alert).copied();
        let due_of = |alert: &Alert| due_by.get(alert);
        let live = |alert: &Alert| standing_of(alert).and_then(|standing| standing.live);
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
            let in_view = |alert: &Alert| standing_of(alert).is_some_and(|standing| standing.in_view);
            let ring = needs_you.ring(now, &live, &in_view);
            // Alerts that ring together chime once, needs-you over done.
            sound = ring
                .rang
                .iter()
                .filter(|alert| due_of(alert).is_some_and(|due| due.chimes))
                .map(|alert| {
                    if alert.needs_you() {
                        Sound::NeedsYou
                    } else {
                        Sound::Done
                    }
                })
                .max_by_key(|sound| *sound == Sound::NeedsYou);
            post = ring
                .rang
                .into_iter()
                .filter_map(|alert| Some((due_of(&alert)?.posting.clone()?, alert)))
                .collect();
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
        let row = self
            .needs_you
            .toasts()
            .last()
            .and_then(|toast| self.sources.row.read(&toast.alert, cx));
        if self.inputs.row != row {
            self.inputs.row = row;
            cx.notify();
        }
    }

    /// The window laid out its columns: where the float hangs this frame.
    pub fn place(&mut self, place: Place, cx: &mut Context<Self>) {
        if self.inputs.place != place {
            self.inputs.place = place;
            cx.notify();
        }
    }

    /// `look`, once the update under way has ended: the window may still
    /// be borrowed as it changes the toasts standing.
    fn look_again(&mut self, cx: &mut Context<Self>) {
        let toasts = cx.weak_entity();
        cx.defer(move |cx| {
            toasts.update(cx, |toasts, cx| toasts.look(cx)).ok();
        });
    }

    /// Take one toast of `machine` down (its button, or the list, acted on
    /// it).
    pub fn drop_toast(&mut self, machine: MachineId, target: &RowTarget, cx: &mut Context<Self>) {
        self.needs_you.drop_toast(machine, target);
        self.look_again(cx);
    }

    /// Forget what was heard of each of `machines`: the window let them go.
    pub fn forget(&mut self, machines: &[MachineId], cx: &mut Context<Self>) {
        for machine in machines {
            self.needs_you.forget(*machine);
        }
        self.look_again(cx);
    }

    /// Hear what stands now as heard and toast none of it: a scene's
    /// requests that stood before the window opened. Once the update under
    /// way has ended, since hearing reads the window.
    pub fn hush(&mut self, cx: &mut Context<Self>) {
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
        #[cfg(any(test, feature = "test-support"))]
        crate::testing::built_toasts();
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
                ferrite_ui::motion::toast_out(left, notifications::toast_face(&row, width))
                    .into_any_element(),
            )
        });
        let standing = standing.map(|row| {
            let toast = notifications::toast(&row, Self::handle(cx), width);
            hang(
                ferrite_ui::motion::toast_in(notifications::toast_key(&row), toast)
                    .into_any_element(),
            )
        });
        div().children(leaving.into_iter().chain(standing))
    }
}

#[cfg(test)]
mod tests;
