//! The view head (SPEC §5.2): the head row's slot over the board.
//!
//! Solo names the focused Thread with T3's breadcrumb (`solo_head`): its
//! Project, a side chat's parent, then its title (a double-click renames it),
//! taking the row's free width, and its ▾ (the Thread's menu, as Claude's
//! title carries one, shown under the pointer); the branch is the panel's,
//! so while the panel is hidden the branch follows the ▾ in the quiet ink
//! (`head_branch`). There is no layout switch and no PR button.
//! The title bar keeps two tools (ADR 0039, `title_tools_element`): the
//! details card's toggle and the Panel's, at the row's right end at the
//! window's edge, where they stay whatever the Panel holds. Everything
//! else the old tools opened is a Surface of the Panel, opened from its
//! `+` menu or its chord. The head leaves the toggles their room.
//!
//! The head is an overlay laid after the titlebar's drag strip
//! (the frame's titlebar), over the frame the board column already paints
//! (the plane): it paints nothing and adds no layout. Only
//! its controls occlude, so a press anywhere else in the row still lands on
//! the strip beneath and drags the window (on Windows, the controls take
//! their region out of the strip's `Drag` hit test the same way).

use super::*;
use super::menu_button::{Door, Doors, Edge};
use ferrite_ui::components::{bound_chord, icon_button, icon_button_as, IconSize, IconState, Tip};
use ferrite_ui::{icons, theme};

impl CockpitView {
    /// The head over the board: the shown Thread's name, when the board
    /// shows one.
    pub(super) fn view_head(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut parts = Vec::new();
        if self.factory().is_some() {
            parts.extend(self.factory_head(cx));
        } else if self.prs_shown() {
            let prs = self.prs.read(cx);
            let project = prs
                .project
                .and_then(|project| self.cockpit.registry().project(project))
                .map(|project| SharedString::from(project.title.clone()));
            parts.extend(prs.head(project));
        } else if !self.empty_board_shown() {
            parts.extend(self.solo_head(cx).into_iter().flatten());
        }
        // The head runs to the column's edge (or the window's), clear of
        // the tools that stand against it.
        let right = self.view_head_end();
        let menu_thread = self
            .focused_thread()
            .filter(|_| !self.empty_board_shown() && !self.destination_shown());
        div()
            .debug_selector(|| "view-head".into())
            .when_some(menu_thread, |head, thread| {
                head.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.open_context_menu(MenuTarget::Thread(thread), event.position, cx);
                    }),
                )
            })
            .absolute()
            .top_0()
            .left(px(self.nav_width()))
            .right(px(right))
            .h(px(theme::WIN_CHROME_H))
            .flex()
            .items_center()
            .gap(px(theme::VIEW_HEAD_GAP))
            .pl(px(self.view_head_pad_l()))
            .pr(px(self.view_head_pad_r(right)))
            .font_family(theme::FONT_UI)
            .text_size(px(theme::FS_UI))
            .line_height(px(theme::LH_UI))
            .whitespace_nowrap()
            .children(parts)
            .into_any_element()
    }

    /// Where the view head ends, in from the window's right edge: the
    /// board's edge, or — with no column, on Windows — short of the
    /// caption buttons.
    fn view_head_end(&self) -> f32 {
        if self.panel_drawn_pane().is_none() && ferrite_ui::frame::CUSTOM {
            super::panel::tools_inset()
        } else {
            self.board_right()
        }
    }

    /// How far in from the window's right edge the title tools end: always
    /// the window's own edge (clear of the caption buttons on Windows),
    /// whichever column is open beside the board, as Claude's stay put.
    fn title_tools_right(&self) -> f32 {
        super::panel::tools_inset()
    }

    /// The head's padding at its end, which keeps its content clear of
    /// the tools (`right` is where the head ends).
    fn view_head_pad_r(&self, right: f32) -> f32 {
        if !self.panel_toggle_shown() {
            return theme::VIEW_HEAD_PAD_X;
        }
        (self.title_tools_right() + theme::TITLE_TOOLS_W + theme::VIEW_HEAD_PAD_X - right)
            .max(theme::VIEW_HEAD_PAD_X)
    }

    fn view_head_pad_l(&self) -> f32 {
        let arrows_end = theme::HISTORY_X + 2.0 * theme::ICON_BUTTON;
        theme::VIEW_HEAD_PAD_X.max(arrows_end + theme::VIEW_HEAD_GAP - self.nav_width())
    }

    pub(super) fn history_arrows_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.nav_folded() {
            return Some(
                div()
                    .debug_selector(|| "nav-titlebar".into())
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(self.nav_width()))
                    .h(px(theme::NAV_HEADER_H))
                    .bg(theme::NAV_BACKGROUND)
                    .flex()
                    .items_center()
                    .pl(px(theme::HISTORY_X))
                    .font_family(theme::FONT_UI)
                    .text_color(rgb(theme::INK_STRONG))
                    .child(nav::list::grain(0.0))
                    .child(nav::head(self.history_arrows(cx)))
                    .into_any_element(),
            );
        }
        let arrows = div()
            .absolute()
            .top_0()
            .left(px(theme::HISTORY_X))
            .h(px(theme::WIN_CHROME_H))
            .flex()
            .items_center()
            .child(self.history_arrows(cx))
            .into_any_element();
        Some(arrows)
    }

    /// Back and forward (`history`), each disabled at its end. Its tooltip
    /// names the chord, as Claude's `Back ⌘[` does.
    pub(super) fn history_arrows(&self, cx: &mut Context<Self>) -> AnyElement {
        let back = history_arrow(
            "history-back",
            icons::BACK,
            "Back",
            "cockpit::GoBack",
            self.history.can_go_back(),
            cx.listener(|view, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                view.step_history(-1, window, cx);
            }),
            cx,
        );
        let forward = history_arrow(
            "history-forward",
            icons::FORWARD,
            "Forward",
            "cockpit::GoForward",
            self.history.can_go_forward(),
            cx.listener(|view, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                view.step_history(1, window, cx);
            }),
            cx,
        );
        div()
            .debug_selector(|| "history-arrows".into())
            .flex()
            .flex_shrink_0()
            .occlude()
            .child(back)
            .child(forward)
            .into_any_element()
    }

    /// Whether the title bar shows the Panel's toggle: for a Thread, not a
    /// draft or the empty board.
    fn panel_toggle_shown(&self) -> bool {
        !self.empty_board_shown() && !self.destination_shown() && self.focused_thread().is_some()
    }

    /// The Factory's breadcrumb: its Project, `Factory`, then the Task
    /// whose panel is on screen.
    fn factory_head(&self, cx: &gpui::App) -> Vec<AnyElement> {
        let (Some(project), Some(view)) = (self.factory_project(), self.factory()) else {
            return Vec::new();
        };
        let surface = view.read(cx).surface;
        let project = self
            .cockpit
            .registry()
            .project(project)
            .map(|project| SharedString::from(project.title.clone()))
            .unwrap_or_else(|| "Project".into());
        let crumb = |text: SharedString, current: bool| {
            div()
                .flex_shrink_0()
                .text_size(px(theme::FS_TITLE))
                .when(current, |crumb| crumb.font_weight(theme::W_LABEL))
                .text_color(rgb(if current {
                    theme::INK_STRONG
                } else {
                    theme::INK_SUBTLE
                }))
                .child(text)
        };
        let task = self
            .open_task_now(cx)
            .filter(|_| surface == super::workflows::Surface::Board);
        let mut crumbs = vec![
            crumb(project, false).into_any_element(),
            separator(),
            crumb("Factory".into(), task.is_none())
                .debug_selector(|| "view-head-factory".into())
                .into_any_element(),
        ];
        if let Some(task) = task {
            crumbs.push(separator());
            crumbs.push(
                crumb(task.title.clone().into(), true)
                    .debug_selector(|| "view-head-task".into())
                    .flex_shrink(1.)
                    .min_w_0()
                    .truncate()
                    .into_any_element(),
            );
        }
        vec![div()
            .debug_selector(|| "view-breadcrumb".into())
            .flex()
            .flex_shrink(1.)
            .min_w_0()
            .items_center()
            .gap(px(theme::HEAD_CRUMB_GAP))
            .children(crumbs)
            .into_any_element()]
    }

    /// The title bar's one tool (ADR 0039), laid by the frame at the row's
    /// right end at the window's edge (`title_tools_right`): the Panel's
    /// toggle (T3's "Toggle right panel"), pressed while the Panel shows.
    pub(super) fn title_tools_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.panel_toggle_shown() {
            return None;
        }
        let panel = icon_button_as(
            "panel-toggle",
            icons::PANEL_RIGHT,
            IconSize::Standard,
            "Toggle panel",
            IconState::Toggle(self.panel_pane().is_some()),
            cx,
        )
        .occlude()
        .tip_with_key("Toggle panel", bound_chord("cockpit::TogglePanel"))
        .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.toggle_panel_now(cx);
        }));
        Some(
            div()
                .debug_selector(|| "title-tools".into())
                .absolute()
                .top_0()
                .right(px(self.title_tools_right()))
                .h(px(theme::WIN_CHROME_H))
                .flex()
                .items_center()
                .gap(px(theme::TITLE_TOOLS_GAP))
                .child(self.details_toggle(cx))
                .child(panel)
                .into_any_element(),
        )
    }

    /// Solo: T3's breadcrumb, `/` between its items — the Project (its
    /// badge and name, a click starts a new Thread in it), a side chat's
    /// parent Thread, then the Thread's title with its chevron — then
    /// where a machine runs it (`From` the machine), the branch while the
    /// panel that says it is hidden, and a spacer.
    fn solo_head(&self, cx: &mut Context<Self>) -> Option<Vec<AnyElement>> {
        let thread = self.focused_thread()?;
        let mut crumbs = Vec::new();
        if let Some(project) = self.head_project(thread, cx) {
            crumbs.extend([project, separator()]);
        }
        let parent = self.side_chat.read(cx).owner_of(thread);
        if let Some(parent) = parent {
            crumbs.extend([self.head_parent(parent, cx), separator()]);
        }
        crumbs.push(self.view_title(thread, cx));
        let mut parts = vec![div()
            .debug_selector(|| "view-breadcrumb".into())
            .flex()
            .flex_shrink(1.)
            .min_w_0()
            .items_center()
            .gap(px(theme::HEAD_CRUMB_GAP))
            .children(crumbs)
            .into_any_element()];
        if let Some(machine) = self.cockpit.machine() {
            parts.push(environment_badge(machine.name()));
        }
        let branch = self
            .panel_pane()
            .is_none()
            .then(|| self.head_branch(thread))
            .flatten();
        parts.extend(branch);
        let spare = div().flex_1().h_full();
        parts.push(match parent {
            Some(parent) => spare.child(self.parent_room_probe(parent, cx)).into_any_element(),
            None => spare.into_any_element(),
        });
        Some(parts)
    }

    /// The breadcrumb's Project: its badge at `HEAD_BADGE` then its name,
    /// at most `HEAD_PROJECT_MAX_W`, in `INK_SUBTLE` lifting to
    /// `INK_STRONG` under the pointer. A click starts a new Thread there.
    fn head_project(&self, thread: ThreadId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let facts = self.facts.get(thread)?;
        let (project, name) = (facts.project?, facts.project_label.clone()?);
        let badge = ferrite_ui::project_badge::project_badge(&name)
            .size(px(theme::HEAD_BADGE))
            .rounded(px(theme::HEAD_BADGE / 4.0));
        Some(
            div()
                .id(("view-head-project", thread.get() as usize))
                .debug_selector(|| "view-head-project".into())
                .occlude()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(theme::HEAD_BADGE_GAP))
                .max_w(px(theme::HEAD_PROJECT_MAX_W))
                .cursor_pointer()
                .text_size(px(theme::FS_TITLE))
                .font_weight(theme::W_LABEL)
                .text_color(rgb(theme::INK_SUBTLE))
                .hover(|style| style.text_color(rgb(theme::INK_STRONG)))
                .child(badge)
                .child(div().min_w_0().truncate().child(name.clone()))
                .tooltip(ferrite_ui::menu::tooltip(format!("New thread in {name}")))
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_draft_in_project(project, cx);
                }))
                .into_any_element(),
        )
    }

    /// A side chat's parent Thread, muted: a click opens it. While the row
    /// overflows it collapses to a literal `...` (T3's), its room measured
    /// as the row's width changes (`ParentRoom`); its tooltip keeps the
    /// whole title.
    fn head_parent(&self, parent: ThreadId, cx: &mut Context<Self>) -> AnyElement {
        let title = self.facts.name(parent).clone();
        let mut room = self.head_parent_room.get();
        room.collapsed = room.collapses(self.parent_title_w(parent, cx));
        self.head_parent_room.set(room);
        let shown = if room.collapsed {
            div()
                .debug_selector(|| "view-head-parent-collapsed".into())
                .child("...")
        } else {
            div().min_w_0().truncate().child(title.clone())
        };
        let probe = self.head_parent_room.clone();
        div()
            .id(("view-head-parent", parent.get() as usize))
            .debug_selector(|| "view-head-parent".into())
            .relative()
            .occlude()
            .flex()
            .flex_shrink(1.)
            .min_w_0()
            .cursor_pointer()
            .text_size(px(theme::FS_TITLE))
            .font_weight(theme::W_LABEL)
            .text_color(rgb(theme::INK_SUBTLE))
            .hover(|style| style.text_color(rgb(theme::INK_STRONG)))
            .aria_label(format!("Open parent thread: {title}"))
            .child(shown)
            .child(
                gpui::canvas(
                    move |bounds, _, _| {
                        let mut room = probe.get();
                        room.parent = f32::from(bounds.size.width);
                        probe.set(room);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .tooltip(ferrite_ui::menu::tooltip(title))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.focus_thread(parent, cx);
            }))
            .into_any_element()
    }

    /// The parent's title drawn whole, as wide as the head would draw it.
    fn parent_title_w(&self, parent: ThreadId, cx: &gpui::App) -> f32 {
        crate::nav::Measure::Text(cx, self.nav_expanded_width()).ui(self.facts.name(parent).as_ref(), theme::FS_TITLE, theme::W_LABEL)
    }

    /// The head's spare width after the breadcrumb, laid out: once it says
    /// the parent should show otherwise than it does, the frame is drawn
    /// again.
    fn parent_room_probe(&self, parent: ThreadId, cx: &gpui::App) -> impl IntoElement {
        let need = self.parent_title_w(parent, cx);
        let probe = self.head_parent_room.clone();
        gpui::canvas(
            move |bounds, window, _| {
                let mut room = probe.get();
                room.spare = Some(f32::from(bounds.size.width));
                probe.set(room);
                if room.collapses(need) != room.collapsed {
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .size_full()
    }

    /// The Thread's branch in the quiet ink, for a head whose panel is
    /// hidden. None while its checkout says no branch.
    fn head_branch(&self, thread: ThreadId) -> Option<AnyElement> {
        let branch = Self::checkout_branch(self.facts.get(thread)?)?;
        Some(
            div()
                .debug_selector(|| "view-branch".into())
                .min_w_0()
                .truncate()
                .text_size(px(theme::FS_TITLE))
                .font_weight(theme::W_BODY)
                .text_color(rgb(theme::INK_SUBTLE))
                .child(branch)
                .into_any_element(),
        )
    }

    /// The Thread's title at the view's weight, the breadcrumb's current
    /// item, at least `VIEW_TITLE_MIN_W`, its chevron shown only under the
    /// pointer (T3's). A click opens the Thread's menu; a double-click
    /// renames it in place.
    fn view_title(&self, thread: ThreadId, cx: &mut Context<Self>) -> AnyElement {
        if let Some((RenameTarget::ViewTitle(editing), editor)) = &self.rename {
            if *editing == thread {
                return rename_field(editor.clone(), true)
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
                    )
                    .into_any_element();
            }
        }
        let name = self.facts.name(thread).clone();
        let hover = SharedString::from(format!("view-title-hover-{}", thread.get()));
        let shown = ferrite_ui::motion::hover_t(&hover) > 0.0;
        let title = div()
            .id(("view-title", thread.get() as usize))
            .debug_selector(|| "view-title".into())
            .min_w(px(theme::VIEW_TITLE_MIN_W))
            .truncate()
            .text_size(px(theme::FS_TITLE))
            .font_weight(theme::W_LABEL)
            .text_color(rgb(theme::INK_STRONG))
            .child(name.clone())
            .tooltip(ferrite_ui::menu::tooltip(name))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if event.click_count >= 2 {
                        view.close_popup(cx);
                        view.start_rename(RenameTarget::ViewTitle(thread), cx);
                    } else {
                        view.open_context_menu(MenuTarget::Thread(thread), event.position, cx);
                    }
                }),
            );
        div()
            .id(("view-title-item", thread.get() as usize))
            .occlude()
            .flex()
            .flex_shrink(1.)
            .min_w_0()
            .items_center()
            .gap(px(theme::HEAD_CHEVRON_GAP))
            .on_hover(ferrite_ui::motion::hover_snap(hover))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_context_menu(MenuTarget::Thread(thread), event.position, cx);
                }),
            )
            .child(title)
            .child(title_menu(&self.doors(), thread, shown, cx))
            .into_any_element()
    }
}

/// One history arrow: an icon button pressing `step` while there is
/// somewhere to go, disabled at its end. The tooltip shows either way, so
/// the chord is learnt at the ends too.
fn history_arrow(
    key: &'static str,
    glyph: &'static str,
    label: &'static str,
    action: &'static str,
    enabled: bool,
    step: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    cx: &gpui::App,
) -> AnyElement {
    icon_button_as(key, glyph, IconSize::Standard, label, IconState::live(enabled), cx)
        .tip_with_key(label, bound_chord(action))
        .on_click(step)
        .into_any_element()
}

/// The title's chevron (T3's, `CHEVRON_DOWN` at 14 in `INK_SUBTLE`):
/// the Thread's menu (`MenuTarget::Thread`: rename, park, copy its
/// path…) under it, its left edge on the chevron's. It keeps its place,
/// seen only while the title is under the pointer (`shown`) or its menu
/// is up.
fn title_menu(doors: &Doors, thread: ThreadId, shown: bool, cx: &mut Context<CockpitView>) -> AnyElement {
    let door = Door::new(MenuTarget::Thread(thread), Edge::Left);
    let shown = shown || doors.is_up(door);
    let chevron = icon_button(
        "view-title-menu",
        icons::CHEVRON_DOWN,
        IconSize::Compact,
        "Thread actions",
        cx,
    )
    .debug_selector(move || {
        if shown {
            "view-title-chevron-shown"
        } else {
            "view-title-menu"
        }
        .into()
    })
    .opacity(if shown { 1.0 } else { 0.0 });
    control(doors.button(chevron, door, "Thread actions", cx))
}

/// The breadcrumb parent's room as the last frame laid it out.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ParentRoom {
    /// The parent's width, and the head's spare width after the row (none
    /// before the row is first laid out).
    parent: f32,
    spare: Option<f32>,
    /// The parent shows as `...`.
    collapsed: bool,
}

impl ParentRoom {
    /// Whether a parent whose whole title is `need` wide shows as `...`:
    /// once the row has no spare width (it overflows), and until the spare
    /// width would hold the whole title with `HEAD_PARENT_ROOM_SLACK` to
    /// spare, so a measure a hair off never sets it flickering.
    fn collapses(self, need: f32) -> bool {
        let Some(spare) = self.spare else {
            return false;
        };
        if self.collapsed {
            self.parent + spare < need + theme::HEAD_PARENT_ROOM_SLACK
        } else {
            spare <= 0.
        }
    }
}

/// The breadcrumb's `/`, in the muted icon ink.
fn separator() -> AnyElement {
    div()
        .debug_selector(|| "view-head-separator".into())
        .flex_shrink_0()
        .text_size(px(theme::FS_TITLE))
        .text_color(rgb(theme::INK_SUBTLE))
        .child("/")
        .into_any_element()
}

/// T3's environment badge after the title: `From` the machine while the
/// window runs a machine's Threads.
fn environment_badge(machine: &str) -> AnyElement {
    div()
        .debug_selector(|| "view-head-environment".into())
        .flex_shrink_0()
        .px(px(theme::SPACE_1_5))
        .rounded(px(theme::R_CONTROL))
        .bg(rgba(theme::HEAD_ENV_GROUND))
        .text_size(px(theme::FS_META))
        .text_color(rgb(theme::BLUE))
        .child(SharedString::from(format!("From {machine}")))
        .into_any_element()
}

/// A control in the head row: it takes its box out of the drag strip
/// beneath (`occlude`), so a press there is the control's alone.
fn control(button: impl IntoElement) -> AnyElement {
    div()
        .flex_shrink_0()
        .occlude()
        .child(button)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::ParentRoom;
    use ferrite_ui::theme::HEAD_PARENT_ROOM_SLACK;

    /// T3's breadcrumb parent: whole while the row has room, a literal
    /// `...` once it overflows, whole again only when the row would hold
    /// the whole title (and its slack), so it never flickers at the edge.
    #[test]
    fn the_breadcrumb_parent_collapses_while_the_row_overflows() {
        let need = 120.;
        let room = |parent: f32, spare: f32, collapsed: bool| ParentRoom {
            parent,
            spare: Some(spare),
            collapsed,
        };
        assert!(!ParentRoom::default().collapses(need), "before any layout: whole");
        assert!(!room(need, 40., false).collapses(need), "room: whole");
        assert!(room(60., 0., false).collapses(need), "overflowing: `...`");
        assert!(room(14., 100., true).collapses(need), "not yet room for it all");
        assert!(
            !room(14., need - 14. + HEAD_PARENT_ROOM_SLACK, true).collapses(need),
            "room again: whole"
        );
    }
}

