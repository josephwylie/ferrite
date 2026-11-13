//! The nav's chrome, wired (`nav::chrome`): the search row — the inline
//! field and its results, the Filter menu (a Project to filter by, a Group
//! to open), Add project and New thread — and the foot — Settings, Pull
//! requests, Usage, the undo notice behind ⌘Z, free disk and the update
//! door.

use super::*;
use crate::nav::{chrome, NavTooltip};
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
pub(super) fn search_line(cx: &mut Context<CockpitView>) -> Entity<Composer> {
    let line = cx.new(Composer::new);
    line.update(cx, |line, cx| line.set_role(Role::Field(Some("NavSearch")), cx));
    cx.subscribe(&line, |view, _, _: &Edited, cx| {
        view.nav_search_cursor = 0;
        cx.notify();
    })
    .detach();
    line
}

impl CockpitView {
    /// The field holds text: the list gives way to its results.
    pub(super) fn nav_searching(&self, cx: &gpui::App) -> bool {
        !self.nav_search.read(cx).text().trim().is_empty()
    }

    /// The Threads whose title holds the field's words, case aside: the
    /// list's, in its order, then the parked ones.
    pub(super) fn search_matches<'a>(
        &self,
        state: &'a nav::NavState,
        cx: &gpui::App,
    ) -> Vec<&'a nav::ThreadRow> {
        let words = self.nav_search.read(cx).text().trim().to_lowercase();
        state
            .open_rows()
            .chain(&state.parked)
            .filter(|row| row.name.to_lowercase().contains(&words))
            .collect()
    }

    /// The row under the head: the field, then Filter, Add project and
    /// New thread.
    pub(super) fn search_row(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let empty = !self.nav_searching(cx);
        let focused = self.nav_search.focus_handle(cx).is_focused(window);
        let clear = (!self.nav_search.read(cx).text().is_empty()).then(|| {
            chrome::search_clear()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.clear_search(cx);
                    }),
                )
                .into_any_element()
        });
        let line = self.nav_search.clone();
        let field = chrome::search_field(self.nav_search.clone(), empty, focused, clear)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                    view.snooze_editor = None;
                    window.focus(&line.focus_handle(cx), cx);
                }),
            )
            .on_action(cx.listener(|view, _: &nav::SearchNext, _, cx| view.step_search(1, cx)))
            .on_action(
                cx.listener(|view, _: &nav::SearchPrevious, _, cx| view.step_search(-1, cx)),
            )
            .on_action(cx.listener(|view, _: &nav::Open, window, cx| {
                view.open_search_cursor(window, cx)
            }))
            .on_action(cx.listener(|view, _: &nav::CloseView, window, cx| {
                view.clear_search(cx);
                window.blur(cx);
            }));
        let filtered = self
            .nav_filter
            .and_then(|project| self.cockpit.registry().project(project))
            .map(|project| project.title.clone());
        let filter = chrome::filter_button(filtered.as_deref());
        let filter = self.menu_button(filter, MenuTarget::NavFilter, gpui::Anchor::TopRight, cx);
        let projects = self.factory_nav_projects(&self.nav_state());
        let factory = (!projects.is_empty()).then(|| {
            let needs_you = projects
                .iter()
                .any(|project| self.cockpit.factory().needs_you(*project) > 0);
            let open = self
                .context_menu
                .as_ref()
                .is_some_and(|menu| menu.target == MenuTarget::NavFactory);
            self.menu_button(
                chrome::factory_button(needs_you, open),
                MenuTarget::NavFactory,
                gpui::Anchor::TopLeft,
                cx,
            )
        });
        let add = chrome::head_button("nav-add-project", ferrite_ui::icons::FOLDER_PLUS)
            .nav_tooltip("Add project")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_project_creator(cx);
                }),
            );
        let new = chrome::head_button("nav-new-thread", ferrite_ui::icons::SQUARE_PEN)
            .nav_action_tooltip("New thread", "cockpit::NewThread")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.dispatch_action(Box::new(NewThread), cx);
                }),
            );
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
        button: Stateful<Div>,
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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    match drawn.get() {
                        Some(button) => view.open_context_menu_below(target, button, corner, cx),
                        None => view.open_context_menu(target, event.position, cx),
                    }
                }),
            )
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
        let cursor = self.nav_search_cursor.min(matches.len().saturating_sub(1));
        let rows: Vec<AnyElement> = matches
            .iter()
            .enumerate()
            .map(|(at, row)| {
                let thread = row.thread;
                let age = row
                    .activity_at
                    .map(|at| nav::card_age(at, wall))
                    .unwrap_or_default();
                chrome::result_row(row, age, at == cursor)
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        view.open_search_result(thread, window, cx);
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
        nav::list_scroll(&self.nav_scroll).child(body)
    }

    /// ↑ / ↓ in the field: the highlight moves, stopping at the ends.
    pub(super) fn step_search(&mut self, by: isize, cx: &mut Context<Self>) {
        let state = self.nav_state();
        let count = self.search_matches(&state, cx).len();
        self.nav_search_cursor = self
            .nav_search_cursor
            .saturating_add_signed(by)
            .min(count.saturating_sub(1));
        cx.notify();
    }

    /// ⏎ in the field: the highlighted result opens.
    pub(super) fn open_search_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.nav_state();
        let thread = self
            .search_matches(&state, cx)
            .get(self.nav_search_cursor)
            .map(|row| row.thread);
        if let Some(thread) = thread {
            self.open_search_result(thread, window, cx);
        }
    }

    /// A result opens its Thread; the search clears and the keys go back
    /// to the board.
    fn open_search_result(&mut self, thread: ThreadId, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_search(cx);
        self.open_nav_row(thread, cx);
        window.blur(cx);
    }

    /// Esc in the field, or its clear button: the field empties and the
    /// list comes back.
    pub(super) fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.nav_search.update(cx, |line, cx| line.set(String::new(), cx));
        self.nav_search_cursor = 0;
        cx.notify();
    }

    /// Whether the search field has the keyboard.
    pub(super) fn search_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.nav_search.focus_handle(cx).is_focused(window)
    }

    /// The Filter menu's rows: every Project (the scoped one checked), and
    /// the Groups, which open from here.
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
        let groups: Vec<_> = self
            .cockpit
            .groups()
            .iter()
            .map(|group| {
                Some((
                    menu::Item::new(group.display_title())
                        .leading(ferrite_ui::icons::BOARD, ferrite_ui::theme::INK_SUBTLE),
                    MenuVerb::OpenGroup(group.id),
                ))
            })
            .collect();
        if !groups.is_empty() {
            rows.push(None);
            rows.extend(groups);
        }
        rows
    }

    // ------------------------------------------------------------ the foot

    /// The foot: Settings, Pull requests (the shown Thread's, in the
    /// Panel), Usage (the shown Provider's plan), the undo notice, free
    /// disk and the update door.
    pub(super) fn nav_foot(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let settings = chrome::foot_button("nav-settings", ferrite_ui::icons::GEAR)
            .nav_action_tooltip("Settings", "cockpit::OpenSettings")
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.toggle_settings(cx);
            }));
        let shown = self.focused_thread();
        let pr = shown.filter(|thread| self.facts.pr(*thread).is_some());
        let pulls = chrome::foot_button("nav-pull-requests", ferrite_ui::icons::GIT_PULL_REQUEST)
            .nav_tooltip(if pr.is_some() {
                "Pull request"
            } else {
                "No pull request for this thread"
            })
            .when(pr.is_none(), |button| {
                button.opacity(ferrite_ui::theme::NAV_FOOT_DISABLED_OPACITY)
            })
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                if let Some(thread) = pr {
                    view.open_pr(thread, cx);
                }
            }));
        let provider = shown
            .and_then(|thread| self.cockpit.thread(thread))
            .map(|open| open.provider())
            .unwrap_or(self.launch_provider);
        let usage = ferrite_pane::pane::plan_usage_popover(
            "nav-usage",
            chrome::foot_button("nav-usage", ferrite_ui::icons::CHART_NO_AXES_COLUMN)
                .nav_tooltip("Usage"),
            self.cockpit.account_limits(provider),
        );
        let now = cx.background_executor().now();
        let notice = self.nav_undo.as_ref().map(|undo| {
            if undo.opacity(now) < 1.0 {
                ferrite_ui::motion::request_frame(window);
            }
            chrome::undo_notice(undo.words(), undo.opacity(now)).into_any_element()
        });
        let mut end = Vec::new();
        if let Some((words, ink)) = self.free_disk.and_then(ferrite_ui::frame::free_disk_words) {
            end.push(chrome::free_disk(words, ink).into_any_element());
        }
        if let Some(door) = self.update_door(cx) {
            end.push(door);
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

    /// The update door, at the foot's right while a release is downloaded
    /// or waiting: its press restarts onto it, as Settings' button does.
    fn update_door(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::ferrite_update::State;
        let tip = match self.ferrite_update.state() {
            State::Ready { version, .. } => format!("Restart to update to {version}"),
            State::HandedOver { version } => format!("Install Ferrite {version}"),
            _ => return None,
        };
        Some(
            chrome::foot_button("nav-update", ferrite_ui::icons::ARROW_UP)
                .nav_tooltip(tip)
                .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    view.press_ferrite_update(window, cx);
                }))
                .into_any_element(),
        )
    }

    /// Stand the undo notice for `act`, fading out after
    /// `NAV_UNDO_SHOWN_MS`.
    pub(super) fn offer_undo(&mut self, act: UndoAct, cx: &mut Context<Self>) {
        let at = cx.background_executor().now();
        let undo = NavUndo { act, at };
        self.nav_undo = Some(undo.clone());
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(NAV_UNDO_SHOWN_MS))
                .await;
            this.update(cx, |view, cx| {
                if view.nav_undo.as_ref() == Some(&undo) {
                    view.nav_undo = None;
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
        let Some(undo) = self.nav_undo.take() else {
            return;
        };
        match undo.act {
            UndoAct::Parked(threads) => {
                for thread in threads {
                    self.revive_thread(thread, cx);
                }
            }
            UndoAct::Snoozed(thread) => self.snooze_thread(thread, None, cx),
        }
        cx.notify();
    }
}
