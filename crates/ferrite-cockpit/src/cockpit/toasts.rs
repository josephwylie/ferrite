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
