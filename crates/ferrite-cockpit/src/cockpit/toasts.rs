//! The toasts' side in the window: what a tend hears and must know of its
//! alerts (`toast_tending`), the row a toast draws, what the toasts ring
//! and what a toast's button asks, and where the float hangs. The float
//! itself, the Inbox's alerts, toasts and posted notifications, is
//! `ferrite_nav::toasts`'s (ADR 0041).

use std::collections::HashSet;

use ferrite_client::cockpit::OnMachine;
use ferrite_core::notifications::{DecisionNoticeId, NoticeId, Notifications};
use ferrite_nav::notifications::Alert;
use ferrite_nav::toasts::{
    Asked, Due, News, Place, Rang, Standing, ToastEvent, ToastSources, Tending, ToastsView,
};
use ferrite_ui::source::Source;

use super::*;

impl CockpitView {
    /// The toast float, reading the window through `toast_tending` and
    /// `alert_row`. Once the Cockpit is made, it tends as the Cockpit
    /// notifies and as the model says something `Arrived`.
    pub(super) fn new_toasts(model: &Entity<Cockpit>, cx: &mut Context<Self>) -> Entity<ToastsView> {
        let root = cx.entity();
        let sources = ToastSources {
            tending: Source::of(&root, |root: &Self, asked: &Asked, cx| Some(root.toast_tending(asked, cx))),
            row: Source::of(&root, |root: &Self, alert: &Alert, cx| {
                root.alert_row(alert, ferrite_core::clock::system_time(), cx)
            }),
        };
        let toasts = cx.new(|_| ToastsView::new(sources));
        let (weak, model) = (toasts.downgrade(), model.clone());
        cx.defer(move |cx| {
            weak.update(cx, |toasts, cx| {
                let wakes = vec![
                    cx.observe(&root, |toasts, _, cx| toasts.tend(cx)),
                    cx.subscribe(&model, |toasts, _, _: &Arrived, cx| toasts.tend(cx)),
                ];
                toasts.watch(wakes, cx);
            })
            .ok();
        });
        toasts
    }

    /// What a tend of the toasts hears and must know: what each machine
    /// the window holds and hears said since `asked`'s watermark, whether
    /// each alert standing still wants the operator and is in view, and
    /// whether each that may ring chimes and is posted.
    pub(super) fn toast_tending(&self, asked: &Asked, cx: &gpui::App) -> Tending {
        #[cfg(test)]
        super::drawn::heard();
        let model = self.cockpit.read(cx);
        // Until its Server answers, a machine's Notices are none, not news:
        // hearing them would forget the requests heard of it. A machine
        // other than this computer is first heard quietly: what stood
        // before the window held it is not news either.
        let news = model
            .every_notifications()
            .filter(|notifications| self.attached(notifications.machine(), cx))
            .map(|notifications| {
                let machine = notifications.machine();
                let (fresh, requests) = news(notifications, asked.presented.get(&machine).copied(), model);
                News {
                    machine,
                    quiet_first: !model.local(machine),
                    fresh,
                    requests,
                }
            })
            .collect();
        let present = model.window_active();
        let on_board: HashSet<ThreadKey> = if asked.standing.is_empty() {
            HashSet::new()
        } else {
            self.visible_indices(cx)
                .into_iter()
                .filter_map(|index| self.panes[index].thread())
                .collect()
        };
        let standing = asked
            .standing
            .iter()
            .map(|alert| Standing {
                live: super::machines::wanted(alert, self, cx),
                in_view: present && on_board.contains(&alert.thread),
            })
            .collect();
        let settings = &self.prefs.settings;
        let (wall, focused) = if asked.due.is_empty() {
            (std::time::SystemTime::UNIX_EPOCH, None)
        } else {
            (ferrite_core::clock::system_time(), self.focused_thread(cx))
        };
        let due = asked
            .due
            .iter()
            .map(|alert| Due {
                chimes: self
                    .alert_provider(alert, cx)
                    .is_some_and(|provider| settings.sounds_for(provider)),
                posting: (settings.system_notifications
                    && crate::system_notifications::wanted(focused == Some(alert.thread), present))
                .then(|| self.system_posting(alert, wall, cx))
                .flatten(),
            })
            .collect();
        Tending {
            news,
            standing,
            due,
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
        self.toasts.update(cx, |toasts, cx| toasts.drop_toast(machine, target, cx));
    }

    /// Hand the toast float where it hangs this frame, as the columns are
    /// laid out, and keep the kit's toast stack (`theme.notification`) on
    /// the transcript's column with it: its right margin follows the
    /// panel, written only when it moves.
    pub(super) fn place_toasts(&self, window: &Window, cx: &mut Context<Self>) {
        let board_right =
            self.panel_column_w(cx) + self.task_panel_w(cx) + self.details_toast_inset(window, cx);
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
