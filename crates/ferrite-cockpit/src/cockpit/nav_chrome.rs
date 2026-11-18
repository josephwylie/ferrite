//! The nav's chrome, wired (`nav::chrome`): the search row — the inline
//! field and its results, the Filter menu (a Project to filter by), Add
//! project and New thread — and the foot — Settings, Pull
//! requests, Usage, the undo notice behind ⌘Z, free disk and the update
//! door.

use super::nav_view::{MenuAt, NavEvent, NavInputs, NavView};
use super::*;
use crate::nav::{chrome, NavTooltip};
use ferrite_ui::components::{icon_button, IconSize, MenuPress};
use ferrite_ui::icons;
use gpui::component::button::Button;
use ferrite_pane::composer::{Composer, Edited, Role};
use ferrite_ui::theme::{MOTION_UNDO_NOTICE_MS, NAV_UNDO_SHOWN_MS};

/// What the last Park or Snooze did, which ⌘Z takes back while its notice
/// stands.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NavUndo {
    pub act: UndoAct,
    pub at: std::time::Instant,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum UndoAct {
    Parked(Vec<ThreadId>),
    Snoozed(ThreadId),
}

impl NavUndo {
    /// The notice's words, the key read from the keymap.
    fn words(&self) -> SharedString {
        let done = match self.act {
            UndoAct::Parked(_) => "Parked",
            UndoAct::Snoozed(_) => "Snoozed",
        };
        let key = ferrite_ui::components::bound_chord_in("nav::Undo", Some("NavTree"))
            .map(|keys| ferrite_ui::components::chord_text(&keys).to_uppercase())
            .unwrap_or_default();
        format!("{done}, Undo {key}").trim_end().to_string().into()
    }

    /// The notice's opacity at `now`: in over `UNDO_NOTICE`, out over it
    /// again before it goes.
    fn opacity(&self, now: std::time::Instant) -> f32 {
        let fade = std::time::Duration::from_millis(MOTION_UNDO_NOTICE_MS);
        let shown = std::time::Duration::from_millis(NAV_UNDO_SHOWN_MS);
        let since = now.saturating_duration_since(self.at);
        let left = shown.saturating_sub(since);
        let t = (since.as_secs_f32() / fade.as_secs_f32())
            .min(left.as_secs_f32() / fade.as_secs_f32())
            .clamp(0.0, 1.0);
        ferrite_ui::motion::UNDO_NOTICE.curve.eval(t)
    }
}

/// The search field's line.
pub(super) fn search_line(cx: &mut Context<NavView>) -> Entity<Composer> {
    let line = cx.new(Composer::new);
    line.update(cx, |line, cx| line.set_role(Role::Field(Some("NavSearch")), cx));
    cx.subscribe(&line, |nav, _, _: &Edited, cx| {
        nav.search_pick.set(None);
        nav.reveal.set(true);
        cx.notify();
    })
    .detach();
    line
}

impl CockpitView {
    /// The Filter menu's rows: every Project, the scoped one checked.
    pub(super) fn filter_rows(&self) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let mut rows = vec![Some((
            menu::Item::new("All projects").checked(self.nav_filter.is_none()),
            MenuVerb::FilterProject(None),
        ))];
        rows.extend(self.cockpit.registry().projects().iter().map(|project| {
            Some((
                menu::Item::new(project.title.clone()).checked(self.nav_filter == Some(project.id)),
                MenuVerb::FilterProject(Some(project.id)),
            ))
        }));
        rows
    }
}

impl NavView {
    /// The field holds text: the list gives way to its results.
    pub(super) fn searching(&self, cx: &gpui::App) -> bool {
        !self.search.read(cx).text().trim().is_empty()
    }

    /// The Threads whose title holds the field's words, case aside: the
    /// list's, in its order, then the parked ones.
    pub(super) fn search_matches<'a>(
        &self,
        state: &'a nav::NavState,
        cx: &gpui::App,
    ) -> Vec<&'a nav::ThreadRow> {
        let words = self.search.read(cx).text().trim().to_lowercase();
        state
            .open_rows()
            .chain(&state.parked)
            .filter(|row| row.name.to_lowercase().contains(&words))
            .collect()
    }

    /// Which of `matches` is highlighted, and so which ⏎ opens: the picked
    /// Thread's place now, wherever live updates have moved it. A Thread
    /// that has left the results, or none picked yet, falls to the first
    /// result, and that result is then the pick.
    fn search_cursor(&self, matches: &[&nav::ThreadRow]) -> Option<usize> {
        let at = self
            .search_pick
            .get()
            .and_then(|pick| matches.iter().position(|row| row.thread == pick))
            .or((!matches.is_empty()).then_some(0))?;
        self.search_pick.set(Some(matches[at].thread));
        Some(at)
    }

    /// The row under the head: the field, then Filter, Add project and
    /// New thread.
    pub(super) fn search_row(&self, inputs: &NavInputs, window: &Window, cx: &mut Context<Self>) -> Div {
        let empty = !self.searching(cx);
        let focused = self.search_focused(window, cx);
        let clear = (!self.search.read(cx).text().is_empty()).then(|| {
            chrome::search_clear(cx)
                .on_click(cx.listener(|nav, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    nav.clear_search(cx);
                }))
                .into_any_element()
        });
        let line = self.search.clone();
        let field = chrome::search_field(self.search.clone(), empty, focused, clear)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _: &MouseDownEvent, window, cx| {
                    cx.emit(NavEvent::DismissSnooze);
                    window.focus(&line.focus_handle(cx), cx);
                }),
            )
            .on_action(cx.listener(|nav, _: &nav::SearchNext, _, cx| nav.step_search(1, cx)))
            .on_action(cx.listener(|nav, _: &nav::SearchPrevious, _, cx| nav.step_search(-1, cx)))
            .on_action(cx.listener(|nav, _: &nav::Open, _, cx| nav.open_search_cursor(cx)))
            .on_action(cx.listener(|nav, _: &nav::CloseView, window, cx| {
                nav.clear_search(cx);
                window.blur(cx);
            }));
        let filter = chrome::filter_button(inputs.filtered.as_deref(), cx);
        let filter = self.menu_button(filter, MenuTarget::NavFilter, gpui::Anchor::TopRight, cx);
        let factory = inputs.factory.map(|needs_you| {
            let open = inputs.menu == Some(MenuTarget::NavFactory);
            self.menu_button(
                chrome::factory_button(needs_you, open, cx),
                MenuTarget::NavFactory,
                gpui::Anchor::TopLeft,
                cx,
            )
        });
        let add = icon_button("nav-add-project", icons::FOLDER_PLUS, IconSize::Standard, "Add project", cx)
            .nav_tooltip("Add project")
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(NavEvent::AddProject);
            }));
        let new = icon_button("nav-new-thread", icons::SQUARE_PEN, IconSize::Standard, "New thread", cx)
            .nav_action_tooltip("New thread", "cockpit::NewThread")
            .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                window.dispatch_action(Box::new(NewThread), cx);
            }));
        chrome::search_row(
            field,
            factory
                .into_iter()
                .chain([filter, add.into_any_element(), new.into_any_element()])
                .collect(),
        )
    }

    /// A head button that opens `target`'s menu under itself, the menu's
    /// `corner` on the button's matching edge.
    fn menu_button(
        &self,
        button: Button,
        target: MenuTarget,
        corner: gpui::Anchor,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let drawn = std::rc::Rc::new(std::cell::Cell::new(None::<gpui::Bounds<Pixels>>));
        button
            .child({
                let drawn = drawn.clone();
                gpui::canvas(move |bounds, _, _| drawn.set(Some(bounds)), |_, _, _, _| {})
                    .absolute()
                    .size_full()
            })
            .on_menu_press(cx.listener(move |_, at: &Point<Pixels>, _, cx| {
                cx.stop_propagation();
                let at = match drawn.get() {
                    Some(button) => MenuAt::Below(button, corner),
                    None => MenuAt::Point(*at),
                };
                cx.emit(NavEvent::Menu(target, at));
            }))
            .into_any_element()
    }

    /// The results in the list's place: a row per match, the highlighted
    /// one lit, or `No threads found`.
    pub(super) fn search_results(
        &self,
        state: &nav::NavState,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let matches = self.search_matches(state, cx);
        let wall = ferrite_core::clock::system_time();
        let cursor = self.search_cursor(&matches);
        let rows: Vec<AnyElement> = matches
            .iter()
            .enumerate()
            .map(|(at, row)| {
                let thread = row.thread;
                let age = row
                    .activity_at
                    .map(|at| nav::card_age(at, wall))
                    .unwrap_or_default();
                let lit = cursor == Some(at);
                chrome::result_row(row, age, lit)
                    .children(lit.then(|| self.search_reveal()))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(NavEvent::OpenResult(thread));
                    }))
                    .into_any_element()
            })
            .collect();
        let body = if rows.is_empty() {
            crate::nav::list::empty("nav-no-results", "No threads found", None)
                .into_any_element()
        } else {
            chrome::results(rows).into_any_element()
        };
        nav::list_scroll(&self.scroll).child(body)
    }

    /// ↑ / ↓ in the field: the highlight moves, stopping at the ends.
    pub(super) fn step_search(&mut self, by: isize, cx: &mut Context<Self>) {
        let Some(inputs) = self.inputs(cx) else {
            return;
        };
        let matches = self.search_matches(&inputs.state, cx);
        let Some(at) = self.search_cursor(&matches) else {
            return;
        };
        let to = at.saturating_add_signed(by).min(matches.len() - 1);
        self.search_pick.set(Some(matches[to].thread));
        self.reveal.set(true);
        cx.notify();
    }

    /// ⏎ in the field: the highlighted result opens.
    pub(super) fn open_search_cursor(&mut self, cx: &mut Context<Self>) {
        let Some(inputs) = self.inputs(cx) else {
            return;
        };
        let matches = self.search_matches(&inputs.state, cx);
        if let Some(at) = self.search_cursor(&matches) {
            cx.emit(NavEvent::OpenResult(matches[at].thread));
        }
    }

    /// Esc in the field, or its clear button: the field empties and the
    /// list comes back.
    pub(super) fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.search.update(cx, |line, cx| line.set(String::new(), cx));
        self.search_pick.set(None);
        cx.notify();
    }

    /// Whether the search field has the keyboard.
    pub(super) fn search_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.search.focus_handle(cx).is_focused(window)
    }

    // ------------------------------------------------------------ the foot

    /// The foot: Settings, Pull requests (the shown Thread's PR in the
    /// Panel, else the destination), Usage (the shown Provider's plan), the
    /// undo notice, free disk and the update door.
    pub(super) fn foot(&self, inputs: &NavInputs, window: &Window, cx: &mut Context<Self>) -> Div {
        let settings = icon_button("nav-settings", icons::GEAR, IconSize::Large, "Settings", cx)
            .nav_action_tooltip("Settings", "cockpit::OpenSettings")
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(NavEvent::Settings);
            }));
        let pulls = icon_button(
            "nav-pull-requests",
            icons::GIT_PULL_REQUEST,
            IconSize::Large,
            "Pull requests",
            cx,
        )
            .nav_tooltip("Pull requests")
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(NavEvent::PullRequests);
            }));
        let usage = ferrite_pane::pane::plan_usage_popover(
            "nav-usage",
            // The popover opens it from the pointer only, so Tab passes it.
            icon_button("nav-usage", icons::CHART_NO_AXES_COLUMN, IconSize::Large, "Usage", cx)
                .tab_stop(false)
                .nav_tooltip("Usage"),
            inputs.limits.clone(),
        );
        let now = cx.background_executor().now();
        let notice = self.undo.as_ref().map(|undo| {
            if undo.opacity(now) < 1.0 {
                ferrite_ui::motion::request_frame(window);
            }
            chrome::undo_notice(undo.words(), undo.opacity(now)).into_any_element()
        });
        let mut end = Vec::new();
        if let Some((words, ink)) = inputs.free_disk.and_then(ferrite_ui::frame::free_disk_words) {
            end.push(chrome::free_disk(words, ink).into_any_element());
        }
        if let Some(tip) = inputs.update.clone() {
            end.push(update_door(tip, cx));
        }
        chrome::foot(
            vec![
                settings.into_any_element(),
                pulls.into_any_element(),
                usage.into_any_element(),
            ],
            notice,
            end,
        )
    }

    /// Stand the undo notice for `act`, fading out after
    /// `NAV_UNDO_SHOWN_MS`.
    pub(super) fn offer_undo(&mut self, act: UndoAct, cx: &mut Context<Self>) {
        let at = cx.background_executor().now();
        let undo = NavUndo { act, at };
        self.undo = Some(undo.clone());
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(NAV_UNDO_SHOWN_MS))
                .await;
            this.update(cx, |nav, cx| {
                if nav.undo.as_ref() == Some(&undo) {
                    nav.undo = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// ⌘Z while the list holds the keys: the last Park revives, the last
    /// Snooze wakes, and the notice goes.
    pub(super) fn undo_nav(&mut self, cx: &mut Context<Self>) {
        let Some(undo) = self.undo.take() else {
            return;
        };
        cx.emit(NavEvent::Undo(undo.act));
        cx.notify();
    }
}

/// The update door, at the foot's right while a release is downloaded or
/// waiting: its press restarts onto it, as Settings' button does.
fn update_door(tip: SharedString, cx: &mut Context<NavView>) -> AnyElement {
    icon_button("nav-update", icons::ARROW_UP, IconSize::Large, tip.clone(), cx)
        .nav_tooltip(tip)
        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            cx.emit(NavEvent::Update);
        }))
        .into_any_element()
}
