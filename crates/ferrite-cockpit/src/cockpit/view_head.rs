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
//! Solo names the focused Thread — its title (a double-click renames it),
//! taking the row's free width, and its ▾ (the Thread's menu, as Claude's
//! title carries one); the Project is the nav's and the branch the panel's,
//! so while the panel is hidden the branch follows the ▾ in the quiet ink
//! (`head_branch`). A Group names itself — its title, `<project> · N
//! threads`, and `New thread`. There is no layout switch and no PR button.
//! The title-bar tools (`title_tools_element`, `tools`) stand at the row's
//! right end, against the right column's left edge (or the window's), so
//! they ride with the column as Claude's do: Open a thread beside, then
//! Claude's five — the session rail, Terminal, Changes, Files and `⋯`
//! (the checkout in Finder, its path). The head leaves them their room.
//! Claude's browser has nothing behind it here, so it is not drawn.
//!
//! The head is an overlay laid after the titlebar's drag strip
//! (the frame's titlebar), over the frame the board column already paints
//! (the plane): it paints nothing and adds no layout. Only
//! its controls occlude, so a press anywhere else in the row still lands on
//! the strip beneath and drags the window (on Windows, the controls take
//! their region out of the strip's `Drag` hit test the same way).

use super::tools::Tool;
use super::*;
use ferrite_ui::{components, icons, theme};

impl CockpitView {
    /// The head over the board: the view's name when the board shows one
    /// — a Thread in Solo, or a Group.
    pub(super) fn view_head(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut parts = Vec::new();
        if !self.empty_board_shown() {
            let named = match self.cockpit.roster().view() {
                View::Solo => self.solo_head(cx),
                View::Group(group) => self.group_view_head(group, cx),
            };
            parts.extend(named.into_iter().flatten());
        }
        // The head runs to the column's edge (or the window's), clear of
        // the tools that stand against it.
        let right = self.view_head_end();
        div()
            .debug_selector(|| "view-head".into())
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

    /// How far a column's head row stands below the window's top. The title
    /// tools stand over the window's right edge whatever column is there, so
    /// the head row of the rightmost column (`Column`) stands under them,
    /// not beside them where they would crowd its tabs and doors.
    pub(super) fn head_drop(&self, column: Column) -> f32 {
        let tools = self.title_tools().len();
        let shown = tools > 0 && self.panel_candidate().is_some() && !self.docked_pane_covers();
        let rightmost = match column {
            Column::Terminal => true,
            Column::Files => !self.tool_pressed(Tool::Terminal),
            Column::Rail => !self.dock_shown(self.focused_thread()),
        };
        if rightmost && shown {
            theme::WIN_CHROME_H
        } else {
            0.
        }
    }

    /// The head's padding at its end, which keeps its content clear of
    /// the tools (`right` is where the head ends).
    fn view_head_pad_r(&self, right: f32) -> f32 {
        let tools = self.title_tools().len();
        if tools == 0 {
            return theme::VIEW_HEAD_PAD_X;
        }
        (self.title_tools_right() + tools_width(tools) + theme::VIEW_HEAD_PAD_X - right)
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

    /// The tools the focused Thread has something behind, left to right.
    /// A draft and the empty board have none.
    fn title_tools(&self) -> Vec<TitleTool> {
        if self.empty_board_shown() {
            return Vec::new();
        }
        let Some(thread) = self.focused_thread() else {
            return Vec::new();
        };
        let checkout = self.thread_path(thread).is_some();
        title_tools(checkout, self.on_machine(), self.grid_board())
    }

    /// The title-bar tools, laid by the frame at the row's right end
    /// at the window's edge (`title_tools_right`).
    pub(super) fn title_tools_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let tools = self.title_tools();
        if tools.is_empty() || self.panel_candidate().is_none() || self.docked_pane_covers() {
            return None;
        }
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
                .children(tools.into_iter().map(|tool| self.title_tool(tool, cx)))
                .into_any_element(),
        )
    }

    fn title_tool(&self, tool: TitleTool, cx: &mut Context<Self>) -> AnyElement {
        let (key, glyph, tip, chord) = match tool {
            TitleTool::Split => ("title-split", icons::SPLIT_RIGHT, "Open a thread beside", None),
            TitleTool::Tool(Tool::Rail) => (
                "panel-toggle",
                icons::PANEL_RIGHT,
                "Show session rail",
                Some("cockpit::TogglePanel"),
            ),
            TitleTool::Tool(Tool::Terminal) => (
                "title-terminal",
                icons::TERMINAL,
                "Terminal",
                Some("cockpit::ToggleTerminal"),
            ),
            TitleTool::Tool(Tool::Changes) => (
                "title-changes",
                icons::DIFF,
                "Changes",
                Some("cockpit::ToggleChanges"),
            ),
            TitleTool::Tool(Tool::Files) => (
                "title-files",
                icons::FOLDER,
                "Files",
                Some("cockpit::ToggleFiles"),
            ),
            TitleTool::Browser => ("open-browser", icons::GLOBE, "Open browser", None),
            TitleTool::More => ("title-more", icons::ELLIPSIS, "More", None),
        };
        let pressed = matches!(tool, TitleTool::Tool(tool) if self.tool_pressed(tool));
        let door = crate::nav::icon_button(
            key,
            glyph,
            theme::NAV_TOGGLE_GLYPH,
            theme::ICON_BUTTON,
            theme::R_CONTROL,
        )
        .occlude()
        .when(pressed, |door| door.bg(theme::paint::SELECTED));
        let door = match chord {
            Some(action) => door.tooltip(ferrite_ui::menu::action_tooltip(tip, action)),
            None => door.tooltip(ferrite_ui::menu::tooltip(tip)),
        };
        door.on_click(cx.listener(move |view, event: &ClickEvent, window, cx| {
            cx.stop_propagation();
            match tool {
                TitleTool::Tool(tool) => view.press_tool(tool, cx),
                TitleTool::Split => view.open_palette_beside(window, cx),
                TitleTool::Browser => {
                    if let Some(thread) = view.focused_thread() {
                        view.open_browser(thread, cx);
                    }
                }
                TitleTool::More => {
                    if let Some(thread) = view.focused_thread() {
                        view.open_context_menu(
                            MenuTarget::Tools(thread),
                            event.position(),
                            cx,
                        );
                    }
                }
            }
        }))
        .into_any_element()
    }

    /// Solo: the focused Thread's title and its ▾, as Claude's, then the
    /// branch while the panel that says it is hidden, then a spacer. The
    /// Project is the nav's.
    fn solo_head(&self, cx: &mut Context<Self>) -> Option<Vec<AnyElement>> {
        let thread = self.focused_thread()?;
        let branch = self
            .panel_pane()
            .is_none()
            .then(|| self.head_branch(thread))
            .flatten();
        let mut parts = vec![self.view_title(thread, cx), title_menu(thread, cx)];
        parts.extend(branch);
        parts.push(div().flex_1().into_any_element());
        Some(parts)
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

    /// The Thread's title at the view's weight: double-click renames it,
    /// in place, as a Pane head's title does (`pane_title`).
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
        div()
            .id(("view-title", thread.get() as usize))
            .debug_selector(|| "view-title".into())
            .occlude()
            .min_w_0()
            .truncate()
            .text_size(px(theme::FS_TITLE))
            .font_weight(theme::W_LABEL)
            .text_color(rgb(theme::INK_STRONG))
            .child(self.facts.name(thread).clone())
            .tooltip(ferrite_ui::menu::tooltip("Rename \u{b7} double-click"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if event.click_count >= 2 {
                        view.start_rename(RenameTarget::ViewTitle(thread), cx);
                    }
                }),
            )
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

/// A title-bar tool (Claude's, where Ferrite has the surface).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TitleTool {
    /// A Thread beside this one (the old tab strip's split-right button).
    Split,
    /// One of Claude's: the rail, Terminal, Changes or Files.
    Tool(Tool),
    /// The Thread's Conversation browser.
    Browser,
    /// Claude's `⋯`: the checkout in Finder, its path.
    More,
}

/// Which tools show: the rail and the browser for any Thread; Terminal,
/// Changes, Files and `⋯` need a checkout on this computer (a machine's is
/// not on this disk); a board of Panes already has them side by side, so
/// it offers no split.
fn title_tools(checkout: bool, on_machine: bool, grid_board: bool) -> Vec<TitleTool> {
    let mut tools = Vec::new();
    if !grid_board {
        tools.push(TitleTool::Split);
    }
    tools.push(TitleTool::Tool(Tool::Rail));
    let local = checkout && !on_machine;
    if local {
        tools.extend([
            TitleTool::Tool(Tool::Terminal),
            TitleTool::Tool(Tool::Changes),
            TitleTool::Tool(Tool::Files),
        ]);
    }
    tools.push(TitleTool::Browser);
    if local {
        tools.push(TitleTool::More);
    }
    tools
}

/// A column of the right side, in the order they stand: the rail, then the
/// dock's Files or Changes, then its terminal at the window's edge.
#[derive(Clone, Copy)]
pub(super) enum Column {
    Rail,
    Files,
    Terminal,
}

/// `count` tools' doors, `TITLE_TOOLS_GAP` apart.
fn tools_width(count: usize) -> f32 {
    count as f32 * theme::ICON_BUTTON + count.saturating_sub(1) as f32 * theme::TITLE_TOOLS_GAP
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

/// The title's ▾ (Claude's): the Thread's menu (`MenuTarget::Thread`:
/// rename, park, copy its path…) from the head, where the pointer is.
fn title_menu(thread: ThreadId, cx: &mut Context<CockpitView>) -> AnyElement {
    control(
        crate::nav::icon_button(
            "view-title-menu",
            icons::CHEVRON_DOWN,
            theme::NAV_HEADING_GLYPH,
            theme::NAV_HEADING_BUTTON,
            theme::R_CONTROL,
        )
        .debug_selector(|| "view-title-menu".into())
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
    use super::{title_tools, TitleTool};
    use crate::cockpit::tools::Tool as T;

    /// Each tool shows only with something behind it.
    #[test]
    fn a_title_tool_shows_only_with_its_surface() {
        use TitleTool::{Browser, Split, More, Tool};
        let all = [
            Split,
            Tool(T::Rail),
            Tool(T::Terminal),
            Tool(T::Changes),
            Tool(T::Files),
            Browser,
            More,
        ];
        assert_eq!(title_tools(true, false, false), all);
        assert_eq!(
            title_tools(true, true, false),
            [Split, Tool(T::Rail), Browser],
            "a machine's checkout is not on this disk"
        );
        assert_eq!(title_tools(false, false, false), [Split, Tool(T::Rail), Browser]);
        assert_eq!(
            title_tools(true, false, true),
            all[1..],
            "a board already has Panes side by side"
        );
    }
}
