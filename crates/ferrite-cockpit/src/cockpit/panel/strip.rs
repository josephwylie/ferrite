//! The Panel's head row and its empty body (ADR 0039; T3's
//! `RightPanelTabs`, `t3-reference/panel-composer.md` §4.1): one strip of
//! Surface tabs, the `+` after the last, and maximize at the row's end.
//!
//! A tab is 24 tall and never wider than 144; the shown one carries the
//! selection, the others read subtle and wash under the pointer. Its
//! leading 16px button holds the kind's icon and, under the pointer, the
//! tab's ×, so the tab never changes width. A middle click closes it; a
//! right click opens its menu (Close, Close others, Close to the right,
//! Close all, and a File's Copy path). Dragged sideways past
//! `SURFACE_TAB_DRAG` and let go over another tab, it moves there; a
//! shorter move is still a click.
//!
//! The `+` menu and "Open a surface" list the same openers (`Opener`), each
//! with its letter; one that cannot run here is dimmed and says why.

use super::*;
use ferrite_ui::icons::{self, icon};

/// One row of the `+` menu and of "Open a surface", in T3's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Opener {
    Browser,
    Terminal,
    Files,
    Changes,
    PullRequest,
    SideChat,
    Background,
    Overview,
}

impl Opener {
    pub(crate) const ALL: [Self; 8] = [
        Self::Browser,
        Self::Terminal,
        Self::Files,
        Self::Changes,
        Self::PullRequest,
        Self::SideChat,
        Self::Background,
        Self::Overview,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Browser => "Browser",
            Self::Terminal => "Terminal",
            Self::Files => "Files",
            Self::Changes => "Changes",
            Self::PullRequest => "Pull request",
            Self::SideChat => "Side chat",
            Self::Background => "Background",
            Self::Overview => "Overview",
        }
    }

    /// The bare letter that picks the row while its list is up.
    pub(crate) fn letter(self) -> &'static str {
        match self {
            Self::Browser => "B",
            Self::Terminal => "T",
            Self::Files => "F",
            Self::Changes => "D",
            Self::PullRequest => "P",
            Self::SideChat => "S",
            Self::Background => "G",
            Self::Overview => "O",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Browser => icons::GLOBE,
            Self::Terminal => icons::TERMINAL,
            Self::Files => icons::FOLDER,
            Self::Changes => icons::DIFF,
            Self::PullRequest => icons::GIT_PULL_REQUEST,
            Self::SideChat => icons::MESSAGES_SQUARE,
            Self::Background => icons::CLOCK,
            Self::Overview => icons::PANEL_RIGHT,
        }
    }
}

/// A Surface's icon in its tab. Overview and Background have no T3
/// counterpart: the Panel's own `PANEL_RIGHT`, and `CLOCK` for work left
/// running.
fn surface_icon(id: &SurfaceId) -> &'static str {
    match id {
        SurfaceId::Overview => icons::PANEL_RIGHT,
        SurfaceId::Changes => icons::DIFF,
        SurfaceId::Files => icons::FOLDER,
        SurfaceId::Background => icons::CLOCK,
        SurfaceId::File(_) => icons::FILE,
        SurfaceId::Browser => icons::GLOBE,
        SurfaceId::Terminal(_) => icons::TERMINAL,
        SurfaceId::SideChat(_) => icons::MESSAGES_SQUARE,
        SurfaceId::Agent(_) => icons::SUBAGENTS,
    }
}

/// A tab's debug name: its kind, and for a resource what it shows.
pub(crate) fn surface_selector(id: &SurfaceId) -> String {
    match id {
        SurfaceId::Overview => "panel-tab-Overview".into(),
        SurfaceId::Changes => "panel-tab-Changes".into(),
        SurfaceId::Files => "panel-tab-Files".into(),
        SurfaceId::Background => "panel-tab-Background".into(),
        SurfaceId::Browser => "panel-tab-Browser".into(),
        SurfaceId::File(path) => format!(
            "panel-tab-File-{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
        SurfaceId::Terminal(number) => format!("panel-tab-Terminal-{number}"),
        SurfaceId::SideChat(tab) => format!("panel-tab-SideChat-{tab}"),
        SurfaceId::Agent(_) => "panel-tab-Agent".into(),
    }
}

impl CockpitView {
    /// Why `opener` cannot run for `thread` now (the row stays, dimmed,
    /// saying so), or None.
    pub(crate) fn opener_unavailable(&self, thread: ThreadId, opener: Opener) -> Option<&'static str> {
        let local = self.thread_path(thread).is_some() && !self.on_machine();
        match opener {
            Opener::Terminal | Opener::Changes | Opener::Files if !local => {
                Some("Needs a checkout on this computer")
            }
            Opener::PullRequest if self.facts.pr(thread).is_none() => {
                Some("No pull request on this branch")
            }
            Opener::SideChat
                if self
                    .cockpit
                    .thread(thread)
                    .is_none_or(|open| transcript_glue::rewind_gate(&open).is_none()) =>
            {
                Some("Its agent cannot start a side chat")
            }
            _ => None,
        }
    }

    /// A row of the `+` menu or of "Open a surface": its Surface opens in
    /// `thread`'s Panel. Nothing for a row that cannot run.
    pub(crate) fn run_opener(&mut self, thread: ThreadId, opener: Opener, cx: &mut Context<Self>) {
        if self.opener_unavailable(thread, opener).is_some() {
            return;
        }
        match opener {
            Opener::Browser => self.open_browser(thread, cx),
            Opener::Terminal => self.new_terminal_surface(thread, cx),
            Opener::Files => self.upsert_surface(thread, SurfaceId::Files, cx),
            Opener::Changes => self.upsert_surface(thread, SurfaceId::Changes, cx),
            Opener::PullRequest => self.open_pr(thread, cx),
            Opener::SideChat => self.open_side_chat(thread, None, cx),
            Opener::Background => self.open_background(thread, cx),
            Opener::Overview => self.open_panel(thread, cx),
        }
    }

    /// The `+` menu's rows: every opener, its letter at the row's end, an
    /// unavailable one disabled with its reason.
    pub(in crate::cockpit) fn surface_menu_rows(&self, thread: ThreadId) -> Vec<Option<(menu::Item, MenuVerb)>> {
        Opener::ALL
            .into_iter()
            .map(|opener| {
                let mut item = menu::Item::new(opener.label()).shortcut(opener.letter());
                item.leading = Some((opener.icon(), ferrite_ui::theme::INK_SUBTLE));
                if let Some(why) = self.opener_unavailable(thread, opener) {
                    item.disabled = true;
                    item.why = Some(why.into());
                }
                Some((item, MenuVerb::OpenSurface(opener)))
            })
            .collect()
    }

    /// A tab's right-click menu: its closes, each disabled where it would
    /// close nothing, a File's path first.
    pub(in crate::cockpit) fn surface_tab_rows(&self, thread: ThreadId, at: usize) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let Some(state) = self.panel.surfaces.get(thread) else {
            return Vec::new();
        };
        let count = state.tabs.len();
        let row = |label: &'static str, verb: MenuVerb, enabled: bool| {
            let mut item = menu::Item::new(label);
            item.disabled = !enabled;
            Some((item, verb))
        };
        let mut rows = Vec::new();
        if let Some(SurfaceId::File(_)) = state.tabs.get(at) {
            rows.push(row("Copy path", MenuVerb::CopyPath, true));
            rows.push(None);
        }
        rows.extend([
            row("Close", MenuVerb::CloseSurface, at < count),
            row("Close others", MenuVerb::CloseOtherSurfaces, count > 1),
            row("Close to the right", MenuVerb::CloseSurfacesRight, at + 1 < count),
            row("Close all", MenuVerb::CloseAllSurfaces, count > 0),
        ]);
        rows
    }

    /// A tab menu's verb, on the tab it opened on.
    pub(in crate::cockpit) fn run_surface_tab_verb(
        &mut self,
        thread: ThreadId,
        at: usize,
        verb: MenuVerb,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self
            .panel
            .surfaces
            .get(thread)
            .and_then(|state| state.tabs.get(at))
            .cloned()
        else {
            return;
        };
        match verb {
            MenuVerb::CloseSurface => self.close_surface(thread, &id, cx),
            MenuVerb::CloseOtherSurfaces => self.close_surfaces(thread, |state| state.close_others(&id), cx),
            MenuVerb::CloseSurfacesRight => self.close_surfaces(thread, |state| state.close_right(&id), cx),
            MenuVerb::CloseAllSurfaces => self.close_surfaces(thread, ThreadSurfaces::close_all, cx),
            MenuVerb::CopyPath => {
                if let SurfaceId::File(path) = &id {
                    cx.write_to_clipboard(ClipboardItem::new_string(path.display().to_string()));
                }
            }
            _ => {}
        }
    }

    /// A tab's title (T3's `surfaceTitle`): the singletons' names, a file's
    /// name (a command's own), the shell's `Terminal N`, the side chat's
    /// title, the agent's task.
    fn surface_title(&self, thread: ThreadId, id: &SurfaceId) -> SharedString {
        match id {
            SurfaceId::Overview => "Overview".into(),
            SurfaceId::Changes => "Changes".into(),
            SurfaceId::Files => "Files".into(),
            SurfaceId::Background => "Background".into(),
            SurfaceId::Browser => "Browser".into(),
            SurfaceId::File(path) => self.file_surface_title(thread, path),
            SurfaceId::Terminal(number) => format!("Terminal {number}").into(),
            SurfaceId::SideChat(tab) => self.side_chat_title(thread, *tab),
            SurfaceId::Agent(key) => self.agent_title(thread, key),
        }
    }

    /// The head row: the tabs and `+` in a strip that scrolls sideways,
    /// then maximize, short of the title bar's Panel toggle at the
    /// window's edge. The gaps drag the window.
    pub(super) fn surface_head(&self, thread: ThreadId, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let state = self.panel.surfaces.get(thread);
        let tabs: Vec<SurfaceId> = match state {
            Some(state) => state.tabs.clone(),
            None => vec![SurfaceId::Overview],
        };
        let active = self.active_surface(thread);
        let maximized = state.is_some_and(|state| state.maximized);
        let strip = div()
            .debug_selector(|| "panel-tabs".into())
            .flex()
            .items_center()
            .gap(px(SURFACE_HEAD_GAP))
            .min_w_full()
            .children(tabs.iter().enumerate().map(|(at, id)| {
                self.surface_tab(thread, at, id, active.as_ref() == Some(id), cx)
            }))
            .child(self.surface_plus(thread, cx));
        let maximize = ferrite_pane::preview::head_door(
            "panel-maximize",
            "panel-maximize".into(),
            if maximized { icons::MINIMIZE } else { icons::MAXIMIZE },
        )
        .occlude()
        .tooltip(ferrite_ui::menu::tooltip(if maximized {
            "Restore panel size"
        } else {
            "Maximize panel"
        }))
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.toggle_panel_maximized(thread, cx);
        }));
        // The title bar's Panel toggle keeps the row's right end.
        let toggle = ICON_BUTTON + TITLE_TOOLS_GAP;
        div()
            .debug_selector(|| "panel-head".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SURFACE_HEAD_GAP))
            .h(px(WIN_CHROME_H))
            .pl(px(SURFACE_HEAD_PAD_L))
            .pr(px(super::tools_inset() + toggle))
            .child(
                div()
                    .id("panel-tab-scroll")
                    .flex_1()
                    .min_w_0()
                    .overflow_x_scroll()
                    .track_scroll(&self.panel.tabs_scroll)
                    .child(strip),
            )
            .child(maximize)
    }

    /// One tab (T3's `group/tab`, see the module doc).
    fn surface_tab(
        &self,
        thread: ThreadId,
        at: usize,
        id: &SurfaceId,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let element = gpui::ElementId::from(SharedString::from(format!("panel-tab-{at}")));
        let key = ferrite_ui::pointer::hover_key(&element);
        let group = SharedString::from(format!("panel-tab-group-{at}"));
        let selector = surface_selector(id);
        let close_id = id.clone();
        let close = div()
            .id(SharedString::from(format!("panel-tab-close-{at}")))
            .debug_selector({
                let selector = selector.clone();
                move || format!("{selector}-close")
            })
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(SURFACE_TAB_CLOSE))
            .rounded(px(SURFACE_TAB_CLOSE_R))
            .hover(|close| close.bg(paint::HOVER))
            .child(
                div()
                    .flex()
                    .group_hover(group.clone(), |icon| icon.hidden())
                    .child(icon(surface_icon(id), SURFACE_TAB_ICON, if active { INK_STRONG } else { INK_SUBTLE })),
            )
            .child(
                div()
                    .debug_selector({
                        let selector = selector.clone();
                        move || format!("{selector}-x")
                    })
                    .hidden()
                    .group_hover(group.clone(), |x| x.flex())
                    .child(icon(icons::CLOSE, SURFACE_TAB_ICON, INK_STRONG)),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.close_surface(thread, &close_id, cx);
            }));
        let pressed = id.clone();
        let middle = id.clone();
        let held = id.clone();
        div()
            .id(element)
            .group(group)
            .debug_selector(move || selector.clone())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SURFACE_TAB_GAP))
            .h(px(SURFACE_TAB_H))
            .max_w(px(SURFACE_TAB_MAX_W))
            .pl(px(SURFACE_TAB_PAD_L))
            .pr(px(SURFACE_TAB_PAD_R))
            .rounded(px(R_CONTROL))
            .text_size(px(FS_BADGE))
            .font_weight(W_LABEL)
            .occlude()
            .map(|face| {
                // The shown tab carries the selection, and steps it up
                // under the pointer; the rest wash.
                if active {
                    face.hover_carried(key).text_color(rgb(INK_STRONG))
                } else {
                    face.text_color(rgb(INK_SUBTLE))
                        .hover(|face| face.bg(paint::HOVER).text_color(rgb(INK_STRONG)))
                }
            })
            .child(close)
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(self.surface_title(thread, id)),
            )
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.activate_surface(thread, &pressed, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, _, _| {
                    view.press_surface_tab(thread, held.clone(), f32::from(event.position.x));
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseUpEvent, _, cx| {
                    view.drop_surface_tab(thread, at, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.close_surface(thread, &middle, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_context_menu(MenuTarget::SurfaceTab { thread, at }, event.position, cx);
                }),
            )
            .into_any_element()
    }

    /// The `+` after the last tab: the opener menu under it.
    fn surface_plus(&self, thread: ThreadId, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let id = gpui::ElementId::from("panel-plus");
        let key = ferrite_ui::pointer::hover_key(&id);
        let drawn: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<Pixels>>>> = Default::default();
        let measured = drawn.clone();
        div()
            .id(id)
            .debug_selector(|| "panel-plus".into())
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(SURFACE_PLUS))
            .rounded(px(R_CONTROL))
            .occlude()
            .hover_raised(key)
            .child(
                gpui::canvas(move |bounds, _, _| measured.set(Some(bounds)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(icon(icons::PLUS, SURFACE_PLUS_GLYPH, INK_SUBTLE))
            .tooltip(ferrite_ui::menu::tooltip("Open a surface"))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
                cx.stop_propagation();
                // Under the `+`, its left edge on the button's.
                let at = drawn.get().map_or(event.position(), |plus| {
                    gpui::point(plus.left(), plus.bottom() + px(FLOAT_OFFSET))
                });
                view.open_context_menu_at(
                    MenuTarget::SurfaceMenu(thread),
                    at,
                    gpui::Anchor::TopLeft,
                    None,
                    cx,
                );
            }))
            .into_any_element()
    }

    /// The `+ menu` command: the focused Thread's Panel opens with its
    /// opener menu up under the head's left end.
    pub(in crate::cockpit) fn open_surface_menu(&mut self, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        if !self.panel_open(thread) {
            self.surfaces_mut(thread).open = true;
        }
        let window = self.panel.window_w;
        let left = window - self.panel_column_w() + ferrite_ui::theme::SURFACE_HEAD_PAD_L;
        let at = gpui::point(px(left), px(ferrite_ui::theme::WIN_CHROME_H));
        self.open_context_menu_at(MenuTarget::SurfaceMenu(thread), at, gpui::Anchor::TopLeft, None, cx);
    }

    /// "Open a surface": the Panel with no tab shown (T3's empty panel).
    /// Its rows are the `+` menu's; a bare letter, the arrows and ⏎ work
    /// while it holds the keyboard.
    pub(super) fn surface_empty(&self, thread: ThreadId, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let highlight = self.panel.empty_highlight;
        let rows = Opener::ALL.into_iter().enumerate().map(|(at, opener)| {
            let why = self.opener_unavailable(thread, opener);
            let id = SharedString::from(format!("panel-open-{}", opener.letter()));
            let selector = id.to_string();
            let key = ferrite_ui::pointer::hover_key(&gpui::ElementId::from(id.clone()));
            div()
                .id(id)
                .debug_selector(move || selector.clone())
                .flex()
                .items_center()
                .gap(px(SURFACE_EMPTY_ROW_PAD_X))
                .w_full()
                .h(px(SURFACE_EMPTY_ROW_H))
                .px(px(SURFACE_EMPTY_ROW_PAD_X))
                .rounded(px(SURFACE_EMPTY_ROW_R))
                .text_size(px(FS_UI))
                .when(why.is_some(), |row| row.opacity(0.5))
                .when(highlight == at, |row| row.bg(paint::HOVER))
                .when(why.is_none(), |row| row.hover_raised(key))
                .when_some(why, |row, why| row.tooltip(ferrite_ui::menu::tooltip(why)))
                .child(icon(opener.icon(), ICON_BUTTON_GLYPH, INK_SUBTLE))
                .child(div().flex_1().min_w_0().truncate().child(opener.label()))
                .child(soft_key_cap(opener.letter()))
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.run_opener(thread, opener, cx);
                }))
        });
        div()
            .id("panel-empty")
            .debug_selector(|| "panel-empty".into())
            .track_focus(&self.panel_empty_focus)
            .flex()
            .flex_col()
            .items_center()
            .flex_1()
            .min_h_0()
            .on_key_down(cx.listener(move |view, event: &gpui::KeyDownEvent, _, cx| {
                if view.surface_empty_key(thread, &event.keystroke, cx) {
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .max_w(px(SURFACE_EMPTY_W))
                    .py(px(SPACE_6))
                    .child(
                        div()
                            .debug_selector(|| "panel-empty-head".into())
                            .mb(px(SPACE_3))
                            .text_center()
                            .text_size(px(FS_UI))
                            .font_weight(W_LABEL)
                            .text_color(rgb(INK_STRONG))
                            .child("Open a surface"),
                    )
                    .child(div().flex().flex_col().gap(px(SPACE_0_5)).children(rows)),
            )
            .into_any_element()
    }

    /// A key in "Open a surface": a letter fires its row (an unavailable
    /// one not), ↑↓←→ move the highlight round, ⏎ fires it.
    pub(in crate::cockpit) fn surface_empty_key(
        &mut self,
        thread: ThreadId,
        keystroke: &gpui::Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        if keystroke.modifiers.modified() {
            return false;
        }
        let count = Opener::ALL.len();
        let at = self.panel.empty_highlight;
        match keystroke.key.as_str() {
            "down" | "right" => self.panel.empty_highlight = (at + 1) % count,
            "up" | "left" => self.panel.empty_highlight = (at + count - 1) % count,
            "enter" => self.run_opener(thread, Opener::ALL[at % count], cx),
            key => {
                let Some(opener) = Opener::ALL
                    .into_iter()
                    .find(|opener| opener.letter().eq_ignore_ascii_case(key))
                else {
                    return false;
                };
                self.run_opener(thread, opener, cx);
            }
        }
        cx.notify();
        true
    }
}

/// T3's soft key cap (`kbd`, `h-5 min-w-5 rounded px-1 text-xs`, white 7%,
/// no border): the empty Panel's letters.
pub(crate) fn soft_key_cap(key: &'static str) -> Div {
    use ferrite_ui::theme::*;
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .min_w(px(KEY_CAP_SOFT))
        .h(px(KEY_CAP_SOFT))
        .px(px(SPACE_1))
        .rounded(px(KEY_CAP_SOFT_R))
        .bg(rgba(KEY_CAP_SOFT_GROUND))
        .text_size(px(FS_BADGE))
        .font_weight(W_LABEL)
        .text_color(rgb(INK_SUBTLE))
        .child(key)
}
