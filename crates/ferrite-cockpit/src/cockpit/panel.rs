//! The right panel (SPEC §5.6), after Claude's: a fixed column on the
//! frame, right of the board and outside it, holding one scroll of
//! sections with no tabs — Environment, Context, Connectors, Changes
//! (`sections`, `changes`). The pull request is a row in Changes once one
//! exists. The column is the one right pane: while a Thread has a side
//! chat open the column holds that instead (`side_chat`), `SIDE_CHAT_W`
//! wide, and the toggle stands down; closing the chat gives the column
//! back to the sections or the preview. The Review, which needs the board's width and takes line
//! comments, still opens beside its Pane inside the board (ADR 0021).
//!
//! When it shows (`panel_pane`): for the focused Pane, in every view —
//! Solo, a Group (the focused Pane), a fullscreen Pane, with a Review open
//! beside the Pane — while the operator has it on
//! (`Settings::panel_shown`, ⌥⌘B), or while that Pane has a preview open,
//! which shows it whatever the setting. There are no other size rules:
//! hidden, the board takes its width; shown, the board gives it up,
//! however narrow. A draft has no Thread, so no panel but for its preview;
//! the empty board has none at all.
//!
//! Its width: `PANEL_W`, or wide while a preview is the one shown
//! (`preview_w`), or — the preview expanded from its head — all the
//! board can give, down to `PANEL_PREVIEW_BOARD_MIN`. It snaps between them, and open and shut, as Claude's
//! does: no ride, no fade, so nothing lays the board out again frame by
//! frame (`motion-measured.md`).
//!
//! A preview (`preview.rs`) is the one thing that brings a head: while a
//! Thread's Pane has one open, the head holds two tabs, `Overview` (the
//! sections) and the file's name. Otherwise the head row over the panel is
//! bare.
//!
//! The title-bar tools (`view_head`, `tools`) stand against the column's
//! left edge, the rail's door among them, so they ride with it. Beside the
//! rail the column holds the tools' dock (`tools`: Files, Changes, the
//! Terminal), the rail first. Hiding the panel puts its preview away: a
//! hidden panel holds none.

mod background;
mod changes;
pub(crate) mod hunks;
pub(crate) mod preview;
mod sections;

use super::*;
use crate::cockpit::view_head::Column;
use ferrite_ui::icons::{self, icon};

/// The panel's own state on the cockpit.
#[derive(Default)]
pub(crate) struct Panel {
    /// Each Thread's sections' scroll, so a Thread's panel comes back where
    /// it was left.
    scrolls: std::collections::HashMap<ThreadId, ScrollHandle>,
    /// The Changes section's unfolded files and their hunks.
    hunks: hunks::Hunks,
    /// The width a preview reads at in this window (`preview_w`), as of
    /// the last frame.
    wide: f32,
    /// The window's width as of the last frame, what the dock's columns
    /// are sized from (`dock_widths`).
    window_w: f32,
    /// The preview is expanded (its head's expand door) until it closes.
    pub(crate) expanded: bool,
    /// The first frame has been tended: a preview open by then is simply
    /// there (`tend_previews`).
    tended: bool,
    /// The Thread whose Background tasks pane holds the column
    /// (`background`), until it is closed.
    pub(crate) background: Option<ThreadId>,
    /// The background cards turned from how they start (running open,
    /// finished closed), by call.
    pub(crate) background_turned: std::collections::HashSet<String>,
}

/// How wide the Panel is while a preview shows, in a window `width` wide
/// (the theme's `PANEL_PREVIEW_*`); expanded, all but the board's least.
fn preview_w(width: f32, expanded: bool) -> f32 {
    use ferrite_ui::theme::*;
    let most = width - NAV_WIDTH - PANEL_PREVIEW_BOARD_MIN;
    let wide = if expanded {
        most
    } else {
        (width * PANEL_PREVIEW_SHARE)
            .clamp(PANEL_PREVIEW_MIN_W, PANEL_PREVIEW_MAX_W)
            .min(most)
    };
    wide.max(PANEL_W)
}

/// How wide the dock is in a window `window` wide beside a rail `rail`
/// wide. Opening it shrinks the thread, as Claude's does: Files and Changes
/// take `FILES_SHARE` of the room right of the nav (Claude's capture,
/// `files-code.png`: 1000 of 1666), the Terminal its own `TERMINAL_W`, and
/// together they leave the thread no less than `DOCK_THREAD_MIN_W` (the
/// rail first). Expanded, the Files column takes all of that the Terminal
/// leaves.
fn dock_widths(window: f32, rail: f32, files: bool, terminal: bool, expanded: bool) -> f32 {
    use ferrite_ui::theme::*;
    let room = (window - NAV_WIDTH).max(0.);
    let most = (room - rail - DOCK_THREAD_MIN_W).max(0.);
    let terminal_w = if terminal { TERMINAL_W } else { 0. };
    let files_w = match (files, expanded) {
        (false, _) => 0.,
        (true, true) => most,
        (true, false) => (room * FILES_SHARE).max(FILES_MIN_W),
    };
    (files_w + terminal_w).min(most)
}

impl CockpitView {
    /// The Pane whose panel shows this frame (see the module doc), if any.
    pub(super) fn panel_pane(&self) -> Option<usize> {
        let index = self.panel_candidate_index()?;
        let pane = &self.panes[index];
        (pane.preview.is_open()
            || self.side_chat_shown(index)
            || self.agent_drill_shown(index)
            || self.background_shown(pane.thread())
            || self.dock_shown(pane.thread())
            || self.prefs.settings.panel_shown && pane.thread().is_some())
        .then_some(index)
    }

    /// The Thread whose panel shows this frame, if it shows for one.
    #[cfg(test)]
    pub(super) fn panel_thread(&self) -> Option<ThreadId> {
        self.panes[self.panel_pane()?].thread()
    }

    /// The Pane the panel is for, shown or hidden: the focused Pane's, a
    /// Thread's or a draft's with a preview open. None on the empty board.
    pub(super) fn panel_candidate(&self) -> Option<PaneIdentity> {
        Some(self.panes[self.panel_candidate_index()?].identity)
    }

    fn panel_candidate_index(&self) -> Option<usize> {
        if self.empty_board_shown() {
            return None;
        }
        let index = self.focused();
        let pane = self.panes.get(index)?;
        (pane.thread().is_some() || pane.preview.is_open()).then_some(index)
    }

    /// Once per frame, before anything measures the board: tend the
    /// previews (`tend_previews`), read the width a preview takes in this
    /// window, and make sure the shown Thread's sections have their scroll.
    pub(super) fn tend_panel(&mut self, window: &mut Window) {
        self.tend_previews(window);
        self.panel.window_w = f32::from(window.viewport_size().width);
        self.panel.wide = preview_w(self.panel.window_w, self.panel.expanded);
        if let Some(thread) = self
            .panel_pane()
            .and_then(|index| self.panes[index].thread())
        {
            self.panel.scrolls.entry(thread).or_default();
        }
    }

    /// How much of the window's right edge the panel's column covers this
    /// frame: its own width, a preview's while one is shown, none hidden.
    pub(super) fn panel_column_w(&self) -> f32 {
        use ferrite_ui::theme::*;
        let Some(index) = self.panel_pane() else {
            return 0.;
        };
        if self.side_chat_shown(index) || self.agent_drill_shown(index) {
            return SIDE_CHAT_W;
        }
        let rail = self.rail_w(index);
        rail + self.dock_w(index, rail)
    }

    /// The rail's own width in the column: the sections' `PANEL_W`, a
    /// preview's wider reading width, or none while only the dock shows.
    fn rail_w(&self, index: usize) -> f32 {
        let pane = &self.panes[index];
        if self.preview_shown(index) {
            self.panel.wide.max(ferrite_ui::theme::PANEL_W)
        } else if pane.preview.is_open()
            || self.background_shown(pane.thread())
            || self.prefs.settings.panel_shown && pane.thread().is_some()
        {
            ferrite_ui::theme::PANEL_W
        } else {
            0.
        }
    }

    /// The dock's width beside a rail `rail` wide: the Files column at a
    /// reading width and the Terminal's own, as much of that as the board
    /// can spare (`dock_widths`).
    fn dock_w(&self, index: usize, rail: f32) -> f32 {
        let Some(thread) = self.panes[index].thread() else {
            return 0.;
        };
        let (files, terminal) = (
            self.files_column_shown(thread),
            self.terminal_column_shown(thread),
        );
        dock_widths(self.panel.window_w, rail, files, terminal, self.panel.expanded)
    }

    /// What the board leaves at its right: the panel's column, or the
    /// frame's gutter when the panel is hidden (SPEC §4).
    pub(super) fn board_right(&self) -> f32 {
        match self.panel_pane() {
            Some(_) => self.panel_column_w(),
            None => self.board_pad(),
        }
    }

    /// ⌥⌘B, the View menu, the palette and the head's toggle: show or
    /// hide the panel, saved.
    pub(super) fn toggle_panel(&mut self, _: &TogglePanel, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_panel_now(cx);
    }

    /// Hiding puts the shown Pane's preview away (a hidden panel holds
    /// none), so a panel shown for a preview alone hides too. Showing asks
    /// the Thread's Session for a fresh context reading.
    pub(super) fn toggle_panel_now(&mut self, cx: &mut Context<Self>) {
        let shown = match self.panel_pane() {
            Some(index) => {
                self.panes[index].preview.close();
                false
            }
            None => true,
        };
        if self.prefs.settings.panel_shown != shown {
            self.change_settings(|settings| settings.panel_shown = shown, cx);
        }
        if let Some(thread) = self
            .panel_candidate()
            .and_then(|owner| owner.thread())
            .filter(|_| shown)
        {
            self.refresh_context(thread);
        }
        cx.notify();
    }

    /// The panel's sections for `thread`, shown if it was hidden, over any
    /// preview its Pane holds (`Show changes`, a scene).
    pub(crate) fn open_panel(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if !self.prefs.settings.panel_shown {
            self.change_settings(|settings| settings.panel_shown = true, cx);
        }
        self.deselect_preview_tab(thread);
        self.refresh_context(thread);
        cx.notify();
    }

    /// Ask `thread`'s Session to report its context afresh, when it can.
    fn refresh_context(&mut self, thread: ThreadId) {
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        if open.supports_control(ferrite_core::ControlKind::RefreshContext) {
            let generation = open.generation();
            self.run_session_control(
                thread,
                generation,
                ferrite_core::SessionControl::RefreshContext,
            );
        }
    }

    // ------------------------------------------------------------ drawing

    /// The panel's column, while it shows: laid over the window's right
    /// edge from the top of the head row to the bar, after the head row's
    /// drag strip, so its tabs take their presses and the gaps between them
    /// still drag the window.
    pub(super) fn panel_element(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
        let index = self.panel_pane()?;
        let owner = self.panes[index].identity;
        if let Some(thread) = owner.thread().filter(|_| self.agent_drill_shown(index)) {
            return Some(self.agent_drill_column(thread, window, cx));
        }
        if let Some(thread) = owner.thread().filter(|_| self.side_chat_shown(index)) {
            return Some(self.side_chat_column(thread, window, cx));
        }
        let previewing = self.preview_shown(index);
        let background = owner.thread().filter(|thread| self.background_shown(Some(*thread)));
        let rail_w = self.rail_w(index);
        let content = div()
            .w(px(rail_w))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .pl(px(PANEL_PAD_L))
            .pr(px(PANEL_PAD_R))
            .pb(px(PANEL_PAD_B))
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .text_color(rgb(INK_STRONG))
            .whitespace_nowrap();
        let content = match owner.thread() {
            _ if background.is_some() => {
                content.child(self.background_pane(background.expect("just matched"), cx))
            }
            _ if previewing => content
                .child(self.panel_head(owner, previewing, cx))
                .child(self.panel_preview(owner, cx)),
            Some(thread) => {
                let scroll = self.panel.scrolls.get(&thread).cloned().unwrap_or_default();
                content
                    .child(self.panel_head(owner, previewing, cx))
                    .child(panel_body(self.panel_sections(thread, window, cx), &scroll))
            }
            None => content.child(self.panel_head(owner, previewing, cx)),
        };
        Some(
            div()
                .debug_selector(|| "panel".into())
                .absolute()
                .top_0()
                .right_0()
                // A hairline short of the window's foot, as it stood
                // above the old bottom bar's rule.
                .bottom(px(ferrite_ui::theme::HAIRLINE_W))
                .w(px(self.panel_column_w()))
                .overflow_hidden()
                .bg(paint::PANEL)
                .flex()
                .children((rail_w > 0.).then_some(content))
                .children(
                    owner
                        .thread()
                        .filter(|thread| self.dock_shown(Some(*thread)))
                        .map(|thread| self.dock_element(thread, cx)),
                )
                .into_any_element(),
        )
    }

    /// The head (in the head row): bare — unless the Pane
    /// has a preview open, when it holds the preview's tab, after the
    /// Thread's `Overview` tab that goes back to the sections. On Windows
    /// it stops short of the caption buttons, which keep the row's right
    /// end.
    fn panel_head(&self, owner: PaneIdentity, previewing: bool, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let preview = self.preview_tab_label(owner).filter(|_| {
            previewing
                || self
                    .index_of(owner)
                    .is_some_and(|index| self.panes[index].preview.is_open())
        });
        let tabs = preview.map(|label| {
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .gap(px(SPACE_0_5))
                .children(
                    owner
                        .thread()
                        .map(|thread| self.overview_tab(thread, !previewing, cx)),
                )
                .child(self.preview_tab_element(owner, label, previewing, cx))
        });
        // An empty strip has nothing to keep clear of the title tools.
        let drop = if tabs.is_some() { self.head_drop(Column::Rail) } else { 0. };
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(WIN_CHROME_H))
            .pr(px(tools_inset() - PANEL_PAD_R))
            .mt(px(drop))
            .child(tabs.unwrap_or_else(|| div().flex_1()))
            .children(owner.thread().map(|thread| {
                ferrite_pane::preview::head_door("open-browser", "open-browser".into(), icons::GLOBE)
                    .tooltip(ferrite_ui::menu::tooltip("Open browser"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_browser(thread, cx);
                    }))
            }))
    }

    /// The `Overview` tab beside a preview's: the sections, the preview
    /// kept open behind them.
    fn overview_tab(
        &self,
        thread: ThreadId,
        active: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        use ferrite_ui::theme::*;
        let id = gpui::ElementId::from("panel-tab-Overview");
        let key = ferrite_ui::pointer::hover_key(&id);
        div()
            .id(id)
            .debug_selector(|| "panel-tab-Overview".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(PANEL_TAB_H))
            .px(px(PANEL_TAB_PAD_X))
            .rounded(px(R_CONTROL))
            .text_size(px(FS_LABEL))
            .font_weight(W_LABEL)
            .occlude()
            .map(|face| {
                // The shown tab carries the selection, and steps it up
                // under the pointer.
                if active {
                    face.hover_carried(key).text_color(rgb(INK_STRONG))
                } else {
                    face.text_color(rgb(INK_STRONG))
                        .hover_raised(key)
                        .press_raised()
                }
            })
            .child("Overview")
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.deselect_preview_tab(thread);
                cx.notify();
            }))
    }
}

/// How far in from the window's right edge the title tools' right edge sits
/// with no column to stand against: the panel's own padding, or on Windows
/// just left of the caption buttons.
pub(super) fn tools_inset() -> f32 {
    use ferrite_ui::theme::{CAPTION_W, PANEL_PAD_R};
    if ferrite_ui::frame::CUSTOM {
        3. * CAPTION_W
    } else {
        PANEL_PAD_R
    }
}

/// A section's head, the Panel's one (`PANEL_SECTION_H`): its name in
/// 13/500 `INK_STRONG` on the content edge — Claude's panel heads read as
/// strong as its rows — Environment's, Changes' and Context's alike;
/// Connectors' folds (`sections::connectors`).
pub(super) fn panel_section_head(selector: &'static str, name: &'static str) -> Div {
    use ferrite_ui::theme::*;
    div()
        .debug_selector(move || selector.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(PANEL_SECTION_H))
        .px(px(PANEL_ROW_ICON_X))
        .text_size(px(FS_META))
        .font_weight(W_LABEL)
        .text_color(rgb(INK_STRONG))
        .child(name)
}

/// The sections' scroll, under the head, on the Thread's own `scroll`, its
/// overlay thumb in the column's right padding (`PANEL_SCROLLBAR_INSET`)
/// beside the rows rather than over them.
fn panel_body(body: AnyElement, scroll: &ScrollHandle) -> Div {
    use ferrite_ui::theme::{PANEL_PAD_R, PANEL_SCROLLBAR_INSET};
    div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .child(
            div()
                .id("panel-body")
                .debug_selector(|| "panel-body".into())
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .child(body),
        )
        .child(
            ferrite_ui::components::scrollbar_layer()
                .right(px(-PANEL_PAD_R))
                .child(
                    ferrite_ui::components::scrollbar_bar("panel-scrollbar", scroll)
                        .gutter(px(PANEL_PAD_R), px(PANEL_SCROLLBAR_INSET)),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::dock_widths;
    use ferrite_ui::theme::*;

    #[test]
    fn the_dock_takes_most_of_the_room_and_the_terminals_own() {
        let room = 2600. - NAV_WIDTH;
        assert_eq!(dock_widths(2600., 0., false, false, false), 0.);
        let files = dock_widths(2600., 0., true, false, false);
        assert_eq!(files, room * FILES_SHARE);
        assert_eq!(dock_widths(2600., PANEL_W, false, true, false), TERMINAL_W);
        assert_eq!(dock_widths(2600., 0., true, true, false), files + TERMINAL_W);
    }

    /// Claude's own window (1920): Files 1000 beside a 666 thread, no rail.
    #[test]
    fn files_in_claudes_window_leaves_claudes_thread() {
        let dock = dock_widths(1920., 0., true, false, false);
        assert!((dock - 1000.).abs() < 5., "{dock}");
        assert!(1920. - NAV_WIDTH - dock >= DOCK_THREAD_MIN_W);
    }

    #[test]
    fn the_thread_keeps_its_least() {
        let window = 1440.;
        let most = window - NAV_WIDTH - PANEL_W - DOCK_THREAD_MIN_W;
        assert_eq!(dock_widths(window, PANEL_W, true, true, false), most);
        assert_eq!(dock_widths(window, PANEL_W, true, true, true), most);
        assert_eq!(dock_widths(600., PANEL_W, true, false, false), 0.);
    }

    #[test]
    fn expanded_gives_files_all_the_terminal_leaves() {
        let most = 2600. - NAV_WIDTH - PANEL_W - DOCK_THREAD_MIN_W;
        assert_eq!(dock_widths(2600., PANEL_W, true, false, true), most);
        assert_eq!(dock_widths(2600., PANEL_W, true, true, true), most);
    }
}
