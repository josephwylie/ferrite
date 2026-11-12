//! The view head (SPEC §5.2): the head row's slot over the board.
//!
//! The history arrows (Claude's and Codex's `‹ ›`, `history`) are not the
//! head's: they stand in the head row right of the traffic lights
//! (`history_arrows_element`), in the sidebar head's empty left slot while
//! the sidebar is open and in the same screen spot over the rail while it
//! is folded, on every board, the empty one and a draft's too: back is most
//! wanted where there is nothing else to do. Where they reach over the
//! board (folded), the head starts clear of them (`view_head_pad_l`).
//!
//! Solo names the focused Thread with T3's breadcrumb (`solo_head`): its
//! Project, a side chat's parent, then its title (a double-click renames it),
//! taking the row's free width, and its ▾ (the Thread's menu, as Claude's
//! title carries one, shown under the pointer); the branch is the panel's,
//! so while the panel is hidden the branch follows the ▾ in the quiet ink
//! (`head_branch`). A Group names itself — its title, `<project> · N
//! threads`, and `New thread`. There is no layout switch and no PR button.
//! The title bar keeps one tool (ADR 0039, `title_tools_element`): the
//! Panel's toggle, at the row's right end at the window's edge, where it
//! stays whatever the Panel holds. Everything else the old tools opened is
//! a Surface of the Panel, opened from its `+` menu or its chord; a Thread
//! beside is the palette's (`open_palette_beside`). The head leaves the
//! toggle its room.
//!
//! The head is an overlay laid after the titlebar's drag strip
//! (the frame's titlebar), over the frame the board column already paints
//! (the plane): it paints nothing and adds no layout. Only
//! its controls occlude, so a press anywhere else in the row still lands on
//! the strip beneath and drags the window (on Windows, the controls take
//! their region out of the strip's `Drag` hit test the same way).

use super::*;
use ferrite_ui::{components, icons, theme};

impl CockpitView {
    /// The head over the board: the view's name when the board shows one
    /// — a Thread in Solo, or a Group.
    pub(super) fn view_head(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut parts = Vec::new();
        if self.factory_view.is_some() {
            parts.extend(self.factory_head());
        } else if !self.empty_board_shown() {
            let named = match self.cockpit.roster().view() {
                View::Solo => self.solo_head(cx),
                View::Group(group) => self.group_view_head(group, cx),
            };
            parts.extend(named.into_iter().flatten());
        }
        // The head runs to the column's edge (or the window's), clear of
        // the tools that stand against it.
        let right = self.view_head_end();
        let menu_thread = matches!(self.cockpit.roster().view(), View::Solo)
            .then(|| self.focused_thread())
            .flatten()
            .filter(|_| !self.empty_board_shown() && self.factory_view.is_none());
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
            .left(px(self.board_left()))
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
        if self.panel_pane().is_none() && ferrite_ui::frame::CUSTOM {
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
        (self.title_tools_right() + theme::ICON_BUTTON + theme::VIEW_HEAD_PAD_X - right)
            .max(theme::VIEW_HEAD_PAD_X)
    }

    /// Where the view head's content starts, from the board's left edge:
    /// `VIEW_HEAD_PAD_X`, or past the history arrows where they reach over
    /// the board (the rail folded, or riding to it).
    fn view_head_pad_l(&self) -> f32 {
        let arrows_end = theme::HISTORY_X + 2.0 * theme::ICON_BUTTON;
        theme::VIEW_HEAD_PAD_X.max(arrows_end + theme::VIEW_HEAD_GAP - self.board_left())
    }

    /// The history arrows in the head row (Claude's and Codex's place),
    /// `HISTORY_X` from the window's left whatever the sidebar does: in the
    /// sidebar head's empty slot between the traffic lights and its bell,
    /// and where the sidebar is folded to its rail, in the same spot over
    /// the title strip. Only the arrows occlude: the row around them still
    /// drags the window.
    pub(super) fn history_arrows_element(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .top_0()
            .left(px(theme::HISTORY_X))
            .h(px(theme::WIN_CHROME_H))
            .flex()
            .items_center()
            .child(self.history_arrows(cx))
            .into_any_element()
    }

    /// Back and forward (`history`), each disabled at its end. Its tooltip
    /// names the chord, as Claude's `Back ⌘[` does.
    fn history_arrows(&self, cx: &mut Context<Self>) -> AnyElement {
        let back = history_arrow(
            "history-back",
            icons::BACK,
            "Back",
            "cockpit::GoBack",
            self.history.can_go_back(),
        )
        .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            view.step_history(-1, window, cx);
        }));
        let forward = history_arrow(
            "history-forward",
            icons::FORWARD,
            "Forward",
            "cockpit::GoForward",
            self.history.can_go_forward(),
        )
        .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            view.step_history(1, window, cx);
        }));
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
        !self.empty_board_shown() && self.factory_view.is_none() && self.focused_thread().is_some()
    }

    /// The Factory's breadcrumb: its Project, `Factory`, then the Task
    /// whose panel is on screen.
    fn factory_head(&self) -> Vec<AnyElement> {
        let Some(view) = self.factory_view.as_ref() else {
            return Vec::new();
        };
        let project = self
            .cockpit
            .registry()
            .project(view.project)
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
            .open_task_now()
            .filter(|_| view.surface == super::workflows::Surface::Board);
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
        let door = crate::nav::icon_button(
            "panel-toggle",
            icons::PANEL_RIGHT,
            theme::NAV_TOGGLE_GLYPH,
            theme::ICON_BUTTON,
            theme::R_CONTROL,
        )
        .occlude()
        .when(self.panel_pane().is_some(), |door| door.bg(theme::paint::SELECTED))
        .tooltip(ferrite_ui::menu::action_tooltip("Toggle panel", "cockpit::TogglePanel"))
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
                .child(door)
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
        let parent = self.side_chats.owner_of(thread);
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
        crate::nav::Measure::Text(cx).ui(self.facts.name(parent).as_ref(), theme::FS_TITLE, theme::W_LABEL)
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
    /// renames it in place, as a Pane head's title does (`pane_title`).
    fn view_title(&self, thread: ThreadId, cx: &mut Context<Self>) -> AnyElement {
        if let Some((RenameTarget::ViewTitle(editing), editor)) = &self.rename {
            if *editing == thread {
                return rename_field(editor.clone())
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
                        view.context_menu = None;
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
            .child(title_menu(thread, shown, cx))
            .into_any_element()
    }

    /// A Group: its title, `<project> · N threads`, a spacer, and `New
    /// thread`, which starts a draft in this Group.
    fn group_view_head(
        &self,
        group: ferrite_core::groups::GroupId,
        cx: &mut Context<Self>,
    ) -> Option<Vec<AnyElement>> {
        let found = self.cockpit.groups().get(group)?;
        let count = found.members.len();
        // The Project, when every member that names one names the same.
        let mut labels = found
            .members
            .iter()
            .filter_map(|member| self.facts.get(*member)?.project_label.clone());
        let project = labels
            .next()
            .filter(|first| labels.all(|label| label == *first));
        let threads = format!("{count} thread{}", if count == 1 { "" } else { "s" });
        let meta = match project {
            Some(project) => format!("{project} \u{b7} {threads}"),
            None => threads,
        };
        let title = div()
            .debug_selector(|| "view-title".into())
            .min_w_0()
            .truncate()
            .text_size(px(theme::FS_TITLE))
            .font_weight(theme::W_LABEL)
            .text_color(rgb(theme::INK_STRONG))
            // The operator's title as typed (SPEC §7), as the bar and nav
            // show it.
            .child(SharedString::from(found.display_title()))
            .into_any_element();
        let new = control(
            components::secondary_button(
                ("group-new-thread", group.get() as usize),
                Some(icons::PLUS),
                "New thread",
                cx,
            )
            .debug_selector(|| "group-new-thread".into())
            .flex_shrink_0()
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.open_draft_with_placement(DraftTarget::Main, DraftPlacement::CurrentGroup, cx);
            })),
        );
        Some(vec![
            title,
            head_meta(meta.into()),
            div().flex_1().into_any_element(),
            new,
        ])
    }
}

/// One history arrow: the sidebar toggle's door while there is somewhere
/// to go, else its glyph in `HISTORY_ARROW_DISABLED_INK` with no hover.
/// The tooltip shows either way, so the chord is learnt at the ends too.
fn history_arrow(
    key: &'static str,
    glyph: &'static str,
    label: &'static str,
    action: &'static str,
    enabled: bool,
) -> Stateful<Div> {
    let door = if enabled {
        crate::nav::icon_button(
            key,
            glyph,
            theme::NAV_TOGGLE_GLYPH,
            theme::ICON_BUTTON,
            theme::R_CONTROL,
        )
    } else {
        div()
            .id(key)
            .debug_selector(move || key.into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(theme::ICON_BUTTON))
            .child(ferrite_ui::icons::icon(
                glyph,
                theme::NAV_TOGGLE_GLYPH,
                theme::HISTORY_ARROW_DISABLED_INK,
            ))
    };
    door.tooltip(ferrite_ui::menu::action_tooltip(label, action))
}

/// The title's chevron (T3's, `CHEVRON_DOWN` at 14 in `INK_SUBTLE`):
/// the Thread's menu (`MenuTarget::Thread`: rename, park, copy its
/// path…) from the head, where the pointer is. It keeps its place, seen
/// only while the title is under the pointer (`shown`).
fn title_menu(thread: ThreadId, shown: bool, cx: &mut Context<CockpitView>) -> AnyElement {
    control(
        crate::nav::icon_button(
            "view-title-menu",
            icons::CHEVRON_DOWN,
            theme::HEAD_CHEVRON,
            theme::NAV_HEADING_BUTTON,
            theme::R_CONTROL,
        )
        .debug_selector(move || {
            if shown {
                "view-title-chevron-shown"
            } else {
                "view-title-menu"
            }
            .into()
        })
        .opacity(if shown { 1.0 } else { 0.0 })
        .tooltip(ferrite_ui::menu::tooltip("Thread actions"))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                view.open_context_menu(MenuTarget::Thread(thread), event.position, cx);
            }),
        ),
    )
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

/// The head's second word, `INK_SUBTLE`: where the Thread works, or a
/// Group's tally.
fn head_meta(text: SharedString) -> AnyElement {
    div()
        .flex_shrink_0()
        .text_color(rgb(theme::INK_SUBTLE))
        .child(text)
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

