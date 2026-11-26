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
            folded: self.columns.nav_folded(),
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
    /// Pane (`lower_right`), their right edge on its box's and their foot
    /// `TOAST_TOP` over its stack, or over what the operator answers at its
    /// transcript's foot when they would cover it (`toast_foot`). With no
    /// Composer there, or the Factory or the pull requests in the board's
    /// place, the board's lower right corner (`TOAST_FOOT`). A sheet up
    /// holds the keys and the pointer: the row under it waits, and the
    /// toasts stay home rather than rise into its card. So do they for a
    /// row scrolled out of sight, whose bounds are as it was last drawn.
    pub(super) fn toast_home(&self, window: &Window, cx: &gpui::App) -> (Pixels, Pixels) {
        use ferrite_ui::theme::{PANE_HEAD_H, TOAST_BOX_RIGHT, TOAST_FOOT, TOAST_TOP};
        let viewport = window.viewport_size();
        // A Pane the board does not show keeps where it was last drawn.
        let panes_shown =
            self.cockpit.read(cx).environment_available() && self.factory().is_none() && !self.prs_shown();
        let panes = if panes_shown { self.pane_rects(window, cx) } else { Vec::new() };
        let answerable = !self.sheet_up(cx);
        let tall = toasts_tall(window, cx);
        let over_composer = lower_right(panes).and_then(|(index, pane)| {
            let shown = &self.panes[index];
            let geometry = shown.read(cx).geometry.get();
            let (field, stack) = (geometry.composer?, geometry.stack?);
            let in_sight = || shown.read(cx).transcript().is_some_and(|transcript| transcript.read(cx).tail_in_view());
            let answers = geometry.answers;
            let answers = answers
                .cards
                .filter(|_| answerable && in_sight())
                .map(|cards| (cards, answers.buttons.unwrap_or(cards)));
            let head = px(pane.y + PANE_HEAD_H + TOAST_TOP);
            let foot = toast_foot(stack.top() - px(TOAST_TOP), field.right(), tall, head, answers);
            Some((field.right(), foot))
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

/// The board's lower right Pane: the rightmost, and of those whose right
/// edges stand as far right (within a pixel, as a split rounds them), the
/// lowest.
fn lower_right(panes: Vec<(usize, layout::Rect)>) -> Option<(usize, layout::Rect)> {
    const NEAR: f32 = 1.0;
    let right = panes.iter().map(|(_, pane)| pane.x + pane.w).reduce(f32::max)?;
    panes
        .into_iter()
        .filter(|(_, pane)| pane.x + pane.w >= right - NEAR)
        .max_by(|(_, a), (_, b)| (a.y + a.h).total_cmp(&(b.y + b.h)))
}

/// How far up the kit's toasts standing now reach together, each at its
/// tallest (`TOAST_KIT_H`), as many as stand at once.
fn toasts_tall(window: &Window, cx: &gpui::App) -> Pixels {
    use ferrite_ui::theme::{TOAST_GAP, TOAST_KIT_H};
    let most = gpui::component::Theme::global(cx).notification.max_items;
    let standing = window
        .root::<gpui::component::Root>()
        .flatten()
        .map_or(0, |root| root.read(cx).notification.read(cx).notifications().len())
        .min(most);
    px((standing as f32 * (TOAST_KIT_H + TOAST_GAP) - TOAST_GAP).max(0.))
}

/// The foot of toasts `tall` high, their right edge on `right`: at `home`,
/// unless there they would cover what the operator answers (`answers`: the
/// cards, and their buttons). Then just over the cards, though no higher
/// than under the Pane's head (`head`, a tall card's top is covered), and
/// never over the buttons, whatever the head.
fn toast_foot(
    home: Pixels,
    right: Pixels,
    tall: Pixels,
    head: Pixels,
    answers: Option<(gpui::Bounds<Pixels>, gpui::Bounds<Pixels>)>,
) -> Pixels {
    use ferrite_ui::theme::{TOAST_TOP, TOAST_W};
    let toasts = gpui::Bounds::from_corners(gpui::point(right - px(TOAST_W), home - tall), gpui::point(right, home));
    match answers {
        Some((cards, buttons)) if tall > px(0.) && toasts.intersects(&cards) => {
            (cards.top() - px(TOAST_TOP)).max(head + tall).min(buttons.top() - px(TOAST_TOP))
        }
        _ => home,
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

impl CockpitView {
    /// What an agent or a script asked of the windows through the API
    /// (ADR 0027), one at a time.
    pub(super) fn present_asked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let asked = self.cockpit.update(cx, |cockpit, _| cockpit.take_asked());
        if asked.is_empty() {
            return;
        }
        // An act's settle can take an ask before the pump has run: the
        // Panes follow the roster first, so a Focus on a Thread just opened
        // lands on its Pane instead of reopening a Thread that is open.
        self.follow_roster(cx);
        for asked in asked {
            self.present(asked, window, cx);
        }
    }

    /// One ask: to land on a Thread, as its nav row would; a toast, which
    /// fades as a finished turn's does unless important, with its chime.
    pub(super) fn present(
        &mut self,
        asked: ferrite_client::cockpit::Asked,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use ferrite_client::cockpit::Asked;
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        match asked {
            // A fork the side chat asked for goes to its tab, not the board.
            Asked::Focus { thread, fork_of } => {
                let fork_of = fork_of
                    .or_else(|| self.unnamed_fork_owner(thread, cx));
                self.switch_thread(window, cx, |view, _, cx| {
                    if !fork_of.is_some_and(|owner| view.bind_side_thread(owner, thread, cx)) {
                        view.land_on_thread(thread, cx);
                    }
                });
            }
            Asked::Notification {
                title,
                body,
                sound,
                important,
            } => {
                let mut toast = Notification::new()
                    .title(title)
                    .with_type(NotificationType::Info)
                    .autohide(!important);
                if let Some(body) = body {
                    toast = toast.message(body);
                }
                window.push_notification(toast, cx);
                // No provider to ask: only the all-sounds switch applies.
                if let Some(sound) = sound.filter(|_| self.prefs.settings.sounds) {
                    self.chime(sound);
                }
            }
            Asked::TaskAlert {
                project,
                task,
                status,
                title,
                detail,
            } => self.present_task_alert(project, task, status, title, detail, window, cx),
        }
    }

    /// A provider's 5-hour or weekly window rising past one of the
    /// operator's thresholds: one toast per crossing, left up — the
    /// allowance is the account's, and every Thread on it draws it down.
    pub(super) fn present_allowance_warnings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use ferrite_core::allowance::LimitWindow;
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        for warning in self.cockpit.update(cx, |cockpit, _| cockpit.take_allowance_warnings()) {
            let span = match warning.window {
                LimitWindow::FiveHour => "5-hour",
                LimitWindow::Weekly => "weekly",
            };
            let provider = ferrite_ui::provider::title(warning.provider);
            window.push_notification(
                Notification::new()
                    .title(format!(
                        "{provider} {span} allowance {:.0}% used",
                        warning.used_fraction * 100.0
                    ))
                    .message(format!(
                        "Past your {}% warning. Every {provider} Thread draws on it.",
                        warning.threshold
                    ))
                    .with_type(NotificationType::Warning)
                    .autohide(false),
                cx,
            );
        }
    }

    /// ⌘I (and the Bar's needs-you count): the list takes the keys on its
    /// first card (ADR 0040); again, they go back to the board.
    pub(super) fn toggle_notifications(
        &mut self,
        _: &ToggleNotifications,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.nav_view.read(cx).holds_keys(window) {
            self.nav_view.update(cx, |nav, cx| nav.leave(window, cx));
            return;
        }
        self.close_composer_menu(cx);
        self.close_popup(cx);
        self.floats.palette = None;
        self.floats.shortcuts = None;
        let first = self.first_nav_card(cx);
        self.focus_nav_on(first, window, cx);
    }

    /// Every click or key on the Inbox's surfaces — a toast's `⏎ open`, a
    /// row, ⏎ and ⌫ on the list, `mark all read` — lands here and is
    /// answered against core. Opening lands the operator on the Notice's
    /// Pane: the core's one focus door, so a parked Thread revives, and the mirror follows. (A toast's quick
    /// answer needs the window: `answer_from_toast`.)
    pub(crate) fn notice_verb(&mut self, verb: Verb, cx: &mut Context<Self>) {
        match verb {
            Verb::OpenThread(thread) => self.land_on_thread(thread, cx),
            Verb::Open(machine, id) => {
                self.drop_toast(machine, &ferrite_nav::notifications::RowTarget::Notice(id), cx);
                if let Some((_, landed)) = self
                    .cockpit
                    .update(cx, |cockpit, _| cockpit.open_notice(machine, id))
                {
                    self.when_answered(
                        landed,
                        |view, landed, cx| {
                            if landed.is_ok() {
                                view.sync_panes(cx);
                                cx.notify();
                            }
                        },
                        cx,
                    );
                }
                self.nav_view.update(cx, |nav, cx| nav.close_snooze(cx));
            }
            Verb::Dismiss(machine, id) => {
                self.cockpit
                    .update(cx, |cockpit, _| cockpit.dismiss_notice(machine, id));
            }
            Verb::OpenDecision(machine, id) => {
                self.drop_toast(
                    machine,
                    &ferrite_nav::notifications::RowTarget::Decision(id.clone()),
                    cx,
                );
                let notifications = self.cockpit.read(cx).notifications(machine);
                let subject = notifications
                    .decision(&id)
                    .and_then(|notice| notice.subject.clone())
                    .unwrap_or(ferrite_core::activity::Subject::Main);
                let thread = notifications.key(id.thread);
                let pending_subject = self
                    .cockpit
                    .read(cx)
                    .thread(thread)
                    .and_then(|open| {
                        open.pending_decisions()
                            .iter()
                            .find(|pending| pending.handle == id.handle)
                    })
                    .map(|pending| {
                        pending
                            .subject
                            .clone()
                            .unwrap_or(ferrite_core::activity::Subject::Main)
                    });
                // A request whose Notice is gone, or whose Thread did not
                // take the screen, is still landed on while it is pending.
                let pending = move |view: &mut Self, cx: &mut Context<Self>| {
                    if let Some(subject) = pending_subject {
                        view.land_on_thread(thread, cx);
                        view.select_subject_from_notice(thread, subject, cx);
                    }
                };
                match self
                    .cockpit
                    .update(cx, |cockpit, _| cockpit.open_decision_notice(machine, &id))
                {
                    Some((thread, landed)) => self.when_answered(
                        landed,
                        move |view, landed, cx| {
                            if landed.is_ok() {
                                view.sync_panes(cx);
                                view.select_subject_from_notice(thread, subject, cx);
                            } else {
                                pending(view, cx);
                            }
                            cx.notify();
                        },
                        cx,
                    ),
                    None => pending(self, cx),
                }
                self.nav_view.update(cx, |nav, cx| nav.close_snooze(cx));
            }
            Verb::DismissDecision(machine, id) => {
                self.cockpit
                    .update(cx, |cockpit, _| cockpit.dismiss_decision_notice(machine, &id));
            }
            // Every row read on every machine, none removed: titles drop
            // to plain, the badge clears.
            Verb::MarkAllRead => {
                self.cockpit.update(cx, |cockpit, _| cockpit.mark_notices_read());
            }
            Verb::Answer(..) => {}
        }
        cx.notify();
    }

    /// A toast's `1 allow` / `3 deny` (FL-18): the decisions package's
    /// answer to that Thread's waiting approval, and the toast goes.
    pub(super) fn answer_from_toast(
        &mut self,
        machine: ferrite_core::MachineId,
        id: ferrite_core::notifications::DecisionNoticeId,
        answer: ferrite_nav::notifications::QuickAnswer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let choice = match answer {
            ferrite_nav::notifications::QuickAnswer::Allow => {
                ferrite_pane::decision::ApprovalChoice::Allow
            }
            ferrite_nav::notifications::QuickAnswer::Deny => {
                ferrite_pane::decision::ApprovalChoice::DenyAndSteer
            }
        };
        self.drop_toast(
            machine,
            &ferrite_nav::notifications::RowTarget::Decision(id.clone()),
            cx,
        );
        let thread = self.cockpit.read(cx).notifications(machine).key(id.thread);
        self.answer_approval(thread, choice, None, window, cx);
        cx.notify();
    }

    /// One completion Notice in the list's words (R7): `✗ failed` with the
    /// turn's failing test summary (a turn whose last test run failed is
    /// failed) or its error; else `✓ done` with the final answer when it is
    /// one line that fits the row, else `worked for <settled duration>`.
    pub(super) fn notice_row(
        &self,
        key: ThreadKey,
        notice: &ferrite_core::notifications::Notice,
        now: std::time::SystemTime,
        cx: &gpui::App,
    ) -> NoticeRow {
        use ferrite_nav::notifications::{Detail, State};
        let title = self.row_title(key, cx);
        let finish = self
            .cockpit
            .read(cx)
            .thread(key)
            .map(|open| open.digest().finish.clone())
            .unwrap_or_default();
        let (state, detail) = match (&notice.outcome, finish.failing_tests) {
            (_, Some(summary)) => (State::Failed, Detail::Words(summary.into())),
            (ferrite_core::TurnOutcome::Error(error), None) => (
                State::Failed,
                Detail::Words(
                    ferrite_core::progress::one_line(error, ferrite_core::progress::ROW_CHARS)
                        .into(),
                ),
            ),
            _ => {
                let room =
                    ferrite_nav::notifications::ROW_TEXT_CELLS.saturating_sub(title.chars().count() + 3);
                (
                    State::Done,
                    ferrite_nav::notifications::done_words(
                        finish.answer.as_deref(),
                        finish.worked_ms.map(Duration::from_millis),
                        room,
                    ),
                )
            }
        };
        NoticeRow {
            target: ferrite_nav::notifications::RowTarget::Notice(notice.id),
            thread: key,
            title,
            state,
            detail,
            when: ferrite_ui::age::since_label(notice.at, now),
            read: notice.read,
            elsewhere: self.machine_label(key.machine(), cx).is_some(),
        }
    }

    /// The provider an alert's Thread runs on, as far as its machine says.
    pub(super) fn alert_provider(&self, alert: &ferrite_nav::notifications::Alert, cx: &gpui::App) -> Option<Provider> {
        self.cockpit
            .read(cx)
            .thread(alert.thread)
            .map(|open| open.provider())
    }

    /// One alert's system notification, worded as its row and tagged with
    /// its Thread on its machine, so a newer one replaces it, while its
    /// record is there to word it.
    pub(super) fn system_posting(
        &self,
        alert: &ferrite_nav::notifications::Alert,
        now: std::time::SystemTime,
        cx: &gpui::App,
    ) -> Option<ferrite_nav::notifications::Posting> {
        let row = self.alert_row(alert, now, cx)?;
        let cockpit = self.cockpit.read(cx);
        let machine = alert.thread.machine();
        let saved_as = cockpit.saved_as(machine)?.clone();
        // The directory is this computer's only for a Thread on it.
        let cwd = cockpit
            .local(machine)
            .then(|| cockpit.thread(alert.thread))
            .flatten()
            .and_then(|open| open.workspace())
            .map(|binding| binding.cwd().to_path_buf());
        Some(ferrite_nav::notifications::Posting {
            tag: system_tag(&saved_as, alert.thread.thread()),
            needs_you: alert.needs_you(),
            word: row.word(),
            title: row.title.to_string(),
            detail: row.detail.text(),
            agent: self
                .alert_provider(alert, cx)
                .map(|provider| provider.to_string()),
            cwd,
        })
    }

    /// Play a chime: the operator's own file for it, else the built-in.
    /// Tests and captures keep quiet; a test reads what would have played.
    pub(super) fn chime(&mut self, sound: Sound) {
        if cfg!(any(test, feature = "visual-reference")) {
            #[cfg(test)]
            self.chimed.push(sound);
            return;
        }
        ferrite_core::sound::play(sound, self.prefs.settings.sound_file(sound));
    }

    /// The operator clicked a system notification: the app comes forward
    /// (the click handler's doing) and lands on what it was about, in
    /// place, on whichever machine.
    pub(crate) fn open_system_notification(&mut self, tag: &str, cx: &mut Context<Self>) {
        if self.open_task_notification(tag, cx) || self.open_copy_notification(tag, cx) {
            return;
        }
        // Its entry stays: read now, the next pump takes it out of the
        // notification centre with the rest.
        let posted = self.toasts.read(cx).needs_you().posted.get(tag).cloned();
        if let Some(alert) = posted {
            self.notice_verb(alert.open_verb(), cx);
        }
    }

    /// The row a toast or a system notification shows for `alert`, while
    /// its record is there to word it.
    pub(super) fn alert_row(
        &self,
        alert: &ferrite_nav::notifications::Alert,
        now: std::time::SystemTime,
        cx: &gpui::App,
    ) -> Option<NoticeRow> {
        let notifications = self.cockpit.read(cx).notifications(alert.thread.machine());
        Some(match &alert.target {
            RowTarget::Thread(_) => return None,
            RowTarget::Notice(id) => {
                self.notice_row(alert.thread, notifications.get(*id)?, now, cx)
            }
            RowTarget::Decision(id) => {
                self.decision_row(alert.thread, notifications.decision(id)?, now, cx)
            }
        })
    }

    /// One row per open Thread awaiting handling, independent of Notices.
    /// Requests precede other rows, retaining each machine's attention order.
    #[cfg(test)]
    pub(super) fn needs_you_rows(&self, cx: &gpui::App) -> Vec<NoticeRow> {
        self.inbox_rows(&self.nav_state(cx), cx)
    }

    /// The Active section as the Inbox's rows, every machine's, requests
    /// first.
    pub(super) fn inbox_rows(
        &self,
        state: &nav::NavState,
        cx: &gpui::App,
    ) -> Vec<NoticeRow> {
        let now = ferrite_core::clock::system_time();
        let mut rows: Vec<_> = state
            .active
            .iter()
            .map(|entry| entry.key)
            .map(|thread| {
                let notifications = self.cockpit.read(cx).notifications(thread.machine());
                if let Some(decision) = notifications.first_request(thread.thread()) {
                    self.decision_row(thread, decision, now, cx)
                } else if let Some(notice) = notifications
                    .notices()
                    .find(|notice| notice.thread == thread.thread())
                {
                    self.notice_row(thread, notice, now, cx)
                } else {
                    NoticeRow {
                        target: RowTarget::Thread(thread.thread()),
                        thread,
                        title: self.row_title(thread, cx),
                        state: ferrite_nav::notifications::State::Idle,
                        detail: ferrite_nav::notifications::Detail::None,
                        when: "".into(),
                        read: true,
                        elsewhere: self.machine_label(thread.machine(), cx).is_some(),
                    }
                }
            })
            .collect();
        rows.sort_by_key(|row| !row.request());
        let mut seen = std::collections::BTreeSet::new();
        rows.retain(|row| seen.insert(row.thread));
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_ui::theme::{TOAST_TOP, TOAST_W};
    use gpui::{point, size, Bounds};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> layout::Rect {
        layout::Rect { x, y, w, h }
    }

    /// A column split in two whose right edges a split's rounding sets a
    /// hair apart is one column: its lower Pane is the lower right, never
    /// the upper for standing a hair further right.
    #[test]
    fn split_panes_pick_the_lower_right() {
        let left = (0, rect(0., 44., 600., 856.));
        let upper = (1, rect(601., 44., 800., 427.5));
        let lower = (2, rect(601., 472.5, 799.99, 427.5));
        assert_eq!(lower_right(vec![left, upper, lower]).map(|(at, _)| at), Some(2));
        // Side by side, the rightmost, though the left one is lower.
        let tall = (0, rect(0., 44., 600., 900.));
        let short = (1, rect(601., 44., 800., 600.));
        assert_eq!(lower_right(vec![tall, short]).map(|(at, _)| at), Some(1));
        assert_eq!(lower_right(Vec::new()), None);
    }

    /// Where toasts 100 high stand, their right edge at 1000 and their home
    /// foot at 800, under a head at 100, over `cards` with `buttons`.
    fn foot(cards: Bounds<Pixels>, buttons: Bounds<Pixels>) -> Pixels {
        toast_foot(px(800.), px(1000.), px(100.), px(100.), Some((cards, buttons)))
    }

    fn at(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    /// The toasts rise only for what they would cover: a card beside them
    /// or above them keeps them home, one under them sends them just over
    /// it.
    #[test]
    fn toasts_rise_only_over_what_they_would_cover() {
        let beside = at(300., 600., 1000. - TOAST_W - 300., 190.);
        assert_eq!(foot(beside, beside), px(800.), "a card beside them keeps them home");
        let above = at(300., 500., 600., 190.);
        assert_eq!(foot(above, above), px(800.), "a card above them keeps them home");
        let under = at(300., 600., 600., 190.);
        assert_eq!(foot(under, under), px(600. - TOAST_TOP), "over the card");
        assert_eq!(
            toast_foot(px(800.), px(1000.), px(100.), px(100.), None),
            px(800.),
            "nothing answered keeps them home"
        );
        assert_eq!(toast_foot(px(800.), px(1000.), px(0.), px(100.), Some((under, under))), px(800.), "no toasts");
    }

    /// A card too tall to clear under the head is covered at its top, never
    /// at its buttons; a window too short for even that sends the toasts
    /// over the buttons, past the head.
    #[test]
    fn toasts_never_cover_the_buttons() {
        let tall = at(300., 150., 600., 640.);
        let buttons = at(300., 740., 600., 40.);
        assert_eq!(foot(tall, buttons), px(200.), "under the head, over the card's top");
        // A Pane so short its Composer's top is at 245: under its head
        // the toasts would reach down to 200, over the buttons at 170.
        let short = at(300., 150., 600., 90.);
        let low = at(300., 170., 600., 60.);
        let foot = toast_foot(px(245.), px(1000.), px(100.), px(100.), Some((short, low)));
        assert_eq!(foot, px(170. - TOAST_TOP), "over the buttons, past the head");
    }
}
