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
