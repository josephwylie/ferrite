//! The right panel (SPEC §5.6, ADR 0039): one tabbed host for every side
//! Surface, after T3 Code's right panel. A fixed column on the frame, right
//! of the board and outside it, laid from the top of the title row to the
//! window's foot.
//!
//! Each Thread keeps its own ordered tabs (`surfaces`): Overview (the old
//! sections: Environment, Context, Connectors, Changes), Changes, Files, a
//! File (a preview: `preview.rs`), the Browser, a Terminal, a side chat, an
//! agent, background work, the branch's pull request (`pull_request.rs`). Every way in goes through one door,
//! `upsert_surface`, which brings a tab already open forward and adds one
//! otherwise; the body is one `match` on the active Surface
//! (`surface_body`). The head row (`strip.rs`) holds the tabs, the `+` menu
//! and maximize; with no tab shown the body is "Open a surface".
//!
//! When it shows (`panel_pane`): for the focused Pane, in every view, while
//! its Thread's Panel is open (`ThreadSurfaces::open`, which starts as
//! `Settings::panel_shown`, ⌥⌘B; anything opened in it opens it). A draft
//! has no Thread: its Panel shows for its preview alone, with no tabs. The
//! empty board has none. There are no other size rules: hidden, the board
//! takes its width; shown, the board gives it up, however narrow.
//!
//! Its width is one per Thread, dragged from the column's left edge and
//! never under the shown kind's floor (`floor_w`), which showing it widens
//! the Panel to; maximized, it takes all the content area but
//! `DOCK_THREAD_MIN_W` of the board. It snaps between widths, and open and
//! shut, as T3's and Claude's do: no ride, no fade, so nothing lays the
//! board out again frame by frame (`motion-measured.md`).
//!
//! The title bar keeps one tool, the Panel's toggle (`view_head`), at the
//! window's right edge; the head row ends short of it.

mod background;
mod changes;
pub(crate) mod hunks;
pub(crate) mod preview;
mod pull_request;
mod sections;
mod strip;

use super::surfaces::{SurfaceId, Surfaces, ThreadSurfaces};
use ferrite_ui::icons::{self, icon};
use super::*;

pub(crate) use pull_request::PrAction;
pub(crate) use strip::Opener;
#[cfg(test)]
pub(crate) use strip::tab_scroll_step;

/// The panel's own state on the cockpit.
pub(crate) struct Panel {
    /// Every Thread's tabs (`surfaces`), and whether they changed since
    /// they were last written.
    pub(crate) surfaces: Surfaces,
    dirty: bool,
    /// Each Thread's sections' scroll, so a Thread's Overview comes back
    /// where it was left.
    scrolls: std::collections::HashMap<ThreadId, ScrollHandle>,
    /// The head row's tabs' own horizontal scroll, and its ease to where a
    /// chevron sent it, until the frame that lands it lets it go.
    pub(crate) tabs_scroll: ScrollHandle,
    tabs_tween: std::cell::Cell<Option<ferrite_ui::motion::Tween>>,
    /// The Changes section's unfolded files and their hunks.
    hunks: hunks::Hunks,
    /// The window's width as of the last frame, what the floors and the
    /// maximized width are reckoned from.
    window_w: f32,
    /// The first frame has been tended: a preview open by then is simply
    /// there (`tend_previews`).
    tended: bool,
    /// The Thread whose Panel's left edge is being dragged.
    resizing: Option<ThreadId>,
    /// The tab pressed, until the button goes up.
    tab_press: Option<TabPress>,
    /// "Open a surface"'s highlighted row (`Opener::ALL`'s index).
    pub(crate) empty_highlight: usize,
    /// The background cards turned from how they start (running open,
    /// finished closed), by call.
    pub(crate) background_turned: std::collections::HashSet<String>,
    /// Each Thread's pull request, read whole for its Surface.
    pr_reads: std::collections::HashMap<ThreadId, pull_request::PrRead>,
    /// The Thread whose PR's Merge waits for its second press.
    pr_armed: Option<ThreadId>,
}

impl Panel {
    /// The Panels as `dir`'s `panel.json` left them, keeping the Surfaces
    /// `resolves` still finds. A file that cannot be read is no Panels,
    /// written over at the next change: they are conveniences, never worth
    /// refusing to start for.
    pub(crate) fn load(dir: &std::path::Path) -> Self {
        let surfaces = Surfaces::load(dir, survives_restart).unwrap_or_else(|error| {
            eprintln!("ferrite: the panel's tabs were not read: {error}");
            Surfaces::default()
        });
        Self {
            surfaces,
            dirty: false,
            scrolls: Default::default(),
            tabs_scroll: ScrollHandle::new(),
            tabs_tween: Default::default(),
            hunks: Default::default(),
            window_w: 0.,
            tended: false,
            resizing: None,
            tab_press: None,
            empty_highlight: 0,
            background_turned: Default::default(),
            pr_reads: Default::default(),
            pr_armed: None,
        }
    }
}

/// A tab under a held button: where the press began, and whether it has
/// moved far enough to be a drag.
struct TabPress {
    thread: ThreadId,
    id: SurfaceId,
    x: f32,
    dragging: bool,
}

/// Which stored Surfaces still resolve after a restart (the file's
/// `migrate`): the singletons, the branch's pull request (read again as it
/// shows), and a File whose file is still there. A
/// shell, a live page, a side chat's fork and an agent's drill-in end with
/// the window. A deleted Thread's Panel goes when the tabs are next
/// written (`save_surfaces`).
fn survives_restart(_: ThreadId, id: &SurfaceId) -> bool {
    match id {
        SurfaceId::Overview
        | SurfaceId::Changes
        | SurfaceId::Files
        | SurfaceId::Background
        | SurfaceId::PullRequest(_) => true,
        SurfaceId::File(path) => path.exists(),
        SurfaceId::Browser | SurfaceId::Terminal(_) | SurfaceId::SideChat(_) | SurfaceId::Agent(_) => {
            false
        }
    }
}

/// The least width the Panel shows `active` at, in a window `window` wide
/// (ADR 0039's floor per kind): the sections' `PANEL_W`; a reading width
/// for a file, the browser or a pull request; most of the room for Files and Changes; the
/// side chat's for a shell, a side chat or an agent.
fn floor_w(active: Option<&SurfaceId>, window: f32) -> f32 {
    use ferrite_ui::theme::*;
    match active {
        None | Some(SurfaceId::Overview | SurfaceId::Background) => PANEL_W,
        Some(SurfaceId::File(_) | SurfaceId::Browser | SurfaceId::PullRequest(_)) => {
            (window * PANEL_PREVIEW_SHARE).clamp(PANEL_PREVIEW_MIN_W, PANEL_PREVIEW_MAX_W)
        }
        Some(SurfaceId::Files | SurfaceId::Changes) => {
            ((window - NAV_WIDTH).max(0.) * FILES_SHARE).max(FILES_MIN_W)
        }
        Some(SurfaceId::Terminal(_)) => TERMINAL_W,
        Some(SurfaceId::SideChat(_) | SurfaceId::Agent(_)) => SIDE_CHAT_W,
    }
}

/// The most the Panel takes in a window `window` wide: all but the nav and
/// the board's least, and never under `PANEL_W`.
fn most_w(window: f32) -> f32 {
    use ferrite_ui::theme::*;
    (window - NAV_WIDTH - DOCK_THREAD_MIN_W).max(PANEL_W)
}

/// How wide a Thread's Panel draws: maximized, all it can take; else its
/// width, at least its shown kind's floor, at most all it can take.
fn panel_w(state: &ThreadSurfaces, window: f32) -> f32 {
    let most = most_w(window);
    if state.maximized {
        most
    } else {
        state.width.max(floor_w(state.active.as_ref(), window)).min(most)
    }
}

impl CockpitView {
    /// The Pane whose panel shows this frame (see the module doc), if any.
    pub(super) fn panel_pane(&self) -> Option<usize> {
        let index = self.panel_candidate_index()?;
        let pane = &self.panes[index];
        match pane.thread() {
            Some(thread) => self.panel_open(thread),
            None => pane.preview.is_open(),
        }
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

    // ---------------------------------------------------------- surfaces

    /// Whether `thread`'s Panel is open: as it was left, or for a Thread
    /// never opened in this window, as the operator last set it.
    pub(super) fn panel_open(&self, thread: ThreadId) -> bool {
        self.panel
            .surfaces
            .get(thread)
            .map_or(self.prefs.settings.panel_shown, |state| state.open)
    }

    /// The Surface `thread`'s Panel shows, open or not: a Thread never
    /// opened in this window shows its Overview.
    pub(crate) fn active_surface(&self, thread: ThreadId) -> Option<SurfaceId> {
        match self.panel.surfaces.get(thread) {
            Some(state) => state.active.clone(),
            None => Some(SurfaceId::Overview),
        }
    }

    /// The Surface Pane `index`'s Panel shows this frame, if it shows one.
    pub(super) fn shown_surface(&self, index: usize) -> Option<SurfaceId> {
        let thread = self.panes.get(index)?.thread()?;
        (self.panel_pane() == Some(index))
            .then(|| self.active_surface(thread))
            .flatten()
    }

    /// `thread`'s tabs, made as a fresh Panel would start, to be changed:
    /// they will be written.
    pub(super) fn surfaces_mut(&mut self, thread: ThreadId) -> &mut ThreadSurfaces {
        self.panel.dirty = true;
        let open = self.prefs.settings.panel_shown;
        self.panel.surfaces.entry(thread, open)
    }

    /// The one door (ADR 0039): show `id` in `thread`'s Panel, its tab
    /// brought forward if it has one, added at the end otherwise, the Panel
    /// opened. What the Surface needs (the Files tree, the Changes read) is
    /// made as it opens.
    pub(crate) fn upsert_surface(&mut self, thread: ThreadId, id: SurfaceId, cx: &mut Context<Self>) {
        self.empty_shown = false;
        match &id {
            SurfaceId::Files => {
                self.ensure_files(thread, cx);
            }
            SurfaceId::Changes => self.ensure_changes(thread, cx),
            SurfaceId::Overview => self.refresh_context(thread),
            SurfaceId::PullRequest(_) => self.read_pr_snapshot(thread, true, cx),
            _ => {}
        }
        self.surfaces_mut(thread).upsert(id);
        cx.notify();
    }

    /// The operator revision of `thread`'s Panel, which an agent reads
    /// before it asks to `reveal_surface`.
    pub(super) fn surface_revision(&self, thread: ThreadId) -> u64 {
        self.panel.surfaces.get(thread).map_or(0, ThreadSurfaces::revision)
    }

    /// An agent's open (a page it opened): `id` shows, unless the operator
    /// has acted in the Panel since `seen`. Says whether it showed.
    pub(super) fn reveal_surface(&mut self, thread: ThreadId, id: SurfaceId, seen: u64) -> bool {
        self.surfaces_mut(thread).reveal(id, seen)
    }

    /// A tab pressed: its Surface shows.
    pub(super) fn activate_surface(&mut self, thread: ThreadId, id: &SurfaceId, cx: &mut Context<Self>) {
        self.surfaces_mut(thread).activate(id);
        cx.notify();
    }

    /// A tab's ×, its middle click, its menu's `Close`: the tab goes, and
    /// what it held with it (a shell ends, a side chat's fork retires, a
    /// preview closes).
    pub(super) fn close_surface(&mut self, thread: ThreadId, id: &SurfaceId, cx: &mut Context<Self>) {
        self.surfaces_mut(thread).close(id);
        self.release_surface(thread, id, cx);
        cx.notify();
    }

    /// Close the tabs of `thread` that `close` takes away, each as
    /// `close_surface` does.
    pub(super) fn close_surfaces(
        &mut self,
        thread: ThreadId,
        close: impl FnOnce(&mut ThreadSurfaces),
        cx: &mut Context<Self>,
    ) {
        let before = self.surfaces_mut(thread).tabs.clone();
        close(self.surfaces_mut(thread));
        let after = self.surfaces_mut(thread).tabs.clone();
        for gone in before.iter().filter(|id| !after.contains(id)) {
            self.release_surface(thread, gone, cx);
        }
        cx.notify();
    }

    /// What a closed tab held goes with it.
    fn release_surface(&mut self, thread: ThreadId, id: &SurfaceId, cx: &mut Context<Self>) {
        match id {
            SurfaceId::Terminal(number) => self.end_terminal(thread, *number, cx),
            SurfaceId::SideChat(tab) => self.close_side_tab(thread, *tab, cx),
            SurfaceId::Agent(key) => self.close_agent_drill(thread, key, cx),
            SurfaceId::File(path) => self.close_preview_of(thread, path),
            SurfaceId::Browser => self.close_preview_of(thread, std::path::Path::new(preview::BROWSER_PATH)),
            SurfaceId::Overview
            | SurfaceId::Changes
            | SurfaceId::Files
            | SurfaceId::Background
            | SurfaceId::PullRequest(_) => {}
        }
    }

    /// A Surface's resource went on its own (a shell exited, a fork was
    /// refused away): its tab goes, not to come back.
    pub(super) fn forget_surface(&mut self, thread: ThreadId, id: &SurfaceId) {
        if self.panel.surfaces.get(thread).is_some_and(|state| state.tabs.contains(id)) {
            self.surfaces_mut(thread).forget(id);
        }
    }

    /// Maximize, or back: `thread`'s Panel takes all the content area but
    /// the board's least.
    pub(super) fn toggle_panel_maximized(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let state = self.surfaces_mut(thread);
        state.maximized = !state.maximized;
        cx.notify();
    }

    /// Write the tabs if they changed (once a frame, or as the window
    /// quits), a deleted Thread's going with it.
    pub(super) fn save_surfaces(&mut self) {
        if !std::mem::take(&mut self.panel.dirty) {
            return;
        }
        let mut known: std::collections::HashSet<ThreadId> =
            self.cockpit.threads().into_iter().collect();
        if let Ok(parked) = self.cockpit.parked() {
            known.extend(parked);
        }
        if !known.is_empty() {
            self.panel.surfaces.retain(|thread| known.contains(&thread));
        }
        if let Err(error) = self.panel.surfaces.save(&self.prefs.dir) {
            eprintln!("ferrite: the panel's tabs were not saved: {error}");
        }
    }

    /// Once per frame, before anything measures the board: tend the
    /// previews (`tend_previews`), widen the shown Panel to its Surface's
    /// floor, give its Surface what it needs to draw, and write the tabs
    /// if they changed.
    pub(super) fn tend_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tend_previews(window, cx);
        self.panel.window_w = f32::from(window.viewport_size().width);
        if let Some(thread) = self
            .panel_pane()
            .and_then(|index| self.panes[index].thread())
        {
            self.panel.scrolls.entry(thread).or_default();
            let window_w = self.panel.window_w;
            let active = self.active_surface(thread);
            let floor = floor_w(active.as_ref(), window_w);
            // A Panel never changed reads its floor without being made.
            if self.panel.surfaces.get(thread).is_some_and(|state| state.width < floor) {
                self.surfaces_mut(thread).width = floor;
            }
            match active {
                Some(SurfaceId::Files) => {
                    self.ensure_files(thread, cx);
                }
                Some(SurfaceId::Changes) => self.ensure_changes(thread, cx),
                Some(SurfaceId::PullRequest(_)) => self.read_pr_snapshot(thread, false, cx),
                _ => {}
            }
        }
        self.save_surfaces();
    }

    /// How much of the window's right edge the panel's column covers this
    /// frame: its Thread's width, a draft's preview's reading width, none
    /// hidden.
    pub(super) fn panel_column_w(&self) -> f32 {
        let Some(index) = self.panel_pane() else {
            return 0.;
        };
        let window = self.panel.window_w;
        match self.panes[index].thread() {
            Some(thread) => match self.panel.surfaces.get(thread) {
                Some(state) => panel_w(state, window),
                None => floor_w(Some(&SurfaceId::Overview), window).min(most_w(window)),
            },
            None => floor_w(Some(&SurfaceId::Browser), window).min(most_w(window)),
        }
    }

    /// What the board leaves at its right: the panel's column, or the
    /// frame's gutter when the panel is hidden (SPEC §4).
    pub(super) fn board_right(&self) -> f32 {
        match self.panel_pane() {
            Some(_) => self.panel_column_w(),
            None => self.board_pad(),
        }
    }

    /// ⌥⌘B, the View menu, the palette and the title bar's toggle: show or
    /// hide the panel, saved.
    pub(super) fn toggle_panel(&mut self, _: &TogglePanel, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_panel_now(cx);
    }

    /// The focused Thread's Panel opens or shuts, its tabs kept as they
    /// are, and the operator's choice is what a Thread's Panel next starts
    /// as. A draft's Panel, shown for its preview alone, puts the preview
    /// away. Showing asks the Thread's Session for a fresh context reading.
    pub(super) fn toggle_panel_now(&mut self, cx: &mut Context<Self>) {
        let Some(owner) = self.panel_candidate() else {
            return;
        };
        let shown = match owner.thread() {
            Some(thread) => {
                let shown = !self.panel_open(thread);
                self.surfaces_mut(thread).open = shown;
                if shown {
                    self.refresh_context(thread);
                }
                shown
            }
            None => {
                if let Some(index) = self.index_of(owner) {
                    self.panes[index].preview.close();
                }
                false
            }
        };
        if self.prefs.settings.panel_shown != shown {
            self.change_settings(|settings| settings.panel_shown = shown, cx);
        }
        cx.notify();
    }

    /// The panel's Overview for `thread`, its Panel opened if it was shut
    /// (the Connectors menu, a scene).
    pub(crate) fn open_panel(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.upsert_surface(thread, SurfaceId::Overview, cx);
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

    /// A press on the column's left edge: the drag that follows sets the
    /// Thread's width.
    fn begin_panel_resize(&mut self, thread: ThreadId) {
        self.panel.resizing = Some(thread);
    }

    /// A press on a tab: a drag past `SURFACE_TAB_DRAG` that follows moves
    /// it; less is still a click.
    pub(super) fn press_surface_tab(&mut self, thread: ThreadId, id: SurfaceId, x: f32) {
        self.panel.tab_press = Some(TabPress {
            thread,
            id,
            x,
            dragging: false,
        });
    }

    /// The pointer moved (the root's every move): with the edge held the
    /// Panel's width follows it, between its Surface's floor and all it
    /// can take; with a tab held, past the drag distance, the tab drags.
    pub(super) fn panel_pointer_moved(&mut self, position: Point<Pixels>, dragging: bool, cx: &mut Context<Self>) {
        if !dragging {
            self.panel.resizing = None;
            self.panel.tab_press = None;
            return;
        }
        if let Some(press) = &mut self.panel.tab_press {
            if (f32::from(position.x) - press.x).abs() >= ferrite_ui::theme::SURFACE_TAB_DRAG {
                press.dragging = true;
            }
        }
        let Some(thread) = self.panel.resizing else {
            return;
        };
        let window = self.panel.window_w;
        let floor = floor_w(self.active_surface(thread).as_ref(), window);
        let width = (window - f32::from(position.x)).clamp(floor, most_w(window).max(floor));
        let state = self.surfaces_mut(thread);
        state.maximized = false;
        state.width = width;
        cx.notify();
    }

    /// A release on tab `at` of `thread`: a tab dragged there moves there.
    pub(super) fn drop_surface_tab(&mut self, thread: ThreadId, at: usize, cx: &mut Context<Self>) {
        let Some(press) = self.panel.tab_press.take() else {
            return;
        };
        if press.dragging && press.thread == thread {
            self.surfaces_mut(thread).move_to(&press.id, at);
            cx.notify();
        }
    }

    /// The button let go anywhere: no edge or tab is held.
    pub(super) fn panel_pointer_released(&mut self, _: Point<Pixels>, cx: &mut Context<Self>) {
        if self.panel.resizing.take().is_some() | self.panel.tab_press.take().is_some() {
            cx.notify();
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
        let column = div()
            .size_full()
            .flex()
            .flex_col()
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .text_color(rgb(INK_STRONG))
            .whitespace_nowrap();
        let column = match owner.thread() {
            Some(thread) => {
                let active = self.active_surface(thread);
                column
                    .child(self.surface_head(thread, cx))
                    .child(self.surface_body(owner, thread, active, window, cx))
            }
            // A draft's preview, alone: no Thread, no tabs.
            None => column.child(div().h(px(WIN_CHROME_H)).flex_shrink_0()).child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .px(px(PANEL_PAD_L))
                    .pb(px(PANEL_PAD_B))
                    .child(self.panel_preview(owner, cx)),
            ),
        };
        let edge = owner.thread().map(|thread| {
            div()
                .id("panel-edge")
                .debug_selector(|| "panel-edge".into())
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(px(SPACE_1))
                .occlude()
                .cursor(gpui::CursorStyle::ResizeLeftRight)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.begin_panel_resize(thread);
                        cx.notify();
                    }),
                )
        });
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
                .child(column)
                .children(edge)
                .into_any_element(),
        )
    }

    /// The body under the head: one `match` on the shown Surface (T3's
    /// `RightPanelSurfaceContent`). Each Surface brings only its body and,
    /// where it has one, its own row of tools.
    fn surface_body(
        &self,
        owner: PaneIdentity,
        thread: ThreadId,
        active: Option<SurfaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let padded = |body: AnyElement| {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .px(px(PANEL_PAD_L))
                .pb(px(PANEL_PAD_B))
                .child(body)
                .into_any_element()
        };
        let filled = |body: AnyElement| {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .child(body)
                .into_any_element()
        };
        match active {
            None => filled(self.surface_empty(thread, cx)),
            Some(SurfaceId::Overview) => {
                let scroll = self.panel.scrolls.get(&thread).cloned().unwrap_or_default();
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .pl(px(PANEL_PAD_L))
                    .pr(px(PANEL_PAD_R))
                    .pb(px(PANEL_PAD_B))
                    .child(panel_body(self.panel_sections(thread, window, cx), &scroll))
                    .into_any_element()
            }
            Some(SurfaceId::Background) => {
                let scroll = self.panel.scrolls.get(&thread).cloned().unwrap_or_default();
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .pl(px(PANEL_PAD_L))
                    .pr(px(PANEL_PAD_R))
                    .pb(px(PANEL_PAD_B))
                    .child(panel_body(self.background_pane(thread, cx).into_any_element(), &scroll))
                    .into_any_element()
            }
            Some(SurfaceId::File(_) | SurfaceId::Browser) => padded(self.panel_preview(owner, cx)),
            Some(SurfaceId::Files) => filled(self.files_pane(thread, cx).into_any_element()),
            Some(SurfaceId::Changes) => filled(self.changes_pane(thread, cx).into_any_element()),
            Some(SurfaceId::Terminal(number)) => {
                filled(self.terminal_pane(thread, number, cx).into_any_element())
            }
            Some(SurfaceId::SideChat(tab)) => filled(self.side_chat_body(thread, tab, cx)),
            Some(SurfaceId::Agent(key)) => filled(self.agent_drill_body(thread, &key)),
            Some(SurfaceId::PullRequest(number)) => filled(self.pr_surface(thread, number, cx)),
        }
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
    use super::*;
    use ferrite_ui::theme::*;

    fn showing(active: SurfaceId, width: f32) -> ThreadSurfaces {
        let mut state = ThreadSurfaces::new(true);
        state.upsert(active);
        state.width = width;
        state
    }

    /// Each kind has its floor; a wider width stands.
    #[test]
    fn the_panel_is_never_under_its_kinds_floor() {
        let window = 1900.;
        assert_eq!(panel_w(&showing(SurfaceId::Overview, 0.), window), PANEL_W);
        assert_eq!(panel_w(&showing(SurfaceId::Terminal(1), 0.), window), TERMINAL_W);
        assert_eq!(
            panel_w(&showing(SurfaceId::File("a".into()), 0.), window),
            (window * PANEL_PREVIEW_SHARE).clamp(PANEL_PREVIEW_MIN_W, PANEL_PREVIEW_MAX_W)
        );
        let files = panel_w(&showing(SurfaceId::Files, 0.), window);
        assert!((files - 0.6 * (window - NAV_WIDTH)).abs() < 1., "{files}");
        assert_eq!(panel_w(&showing(SurfaceId::Overview, 600.), window), 600.);
    }

    /// Maximized, or dragged past it, the board keeps its least.
    #[test]
    fn maximized_leaves_the_board_its_least() {
        let window = 1900.;
        let mut state = showing(SurfaceId::Overview, 0.);
        state.maximized = true;
        assert_eq!(panel_w(&state, window), window - NAV_WIDTH - DOCK_THREAD_MIN_W);
        state.maximized = false;
        state.width = 5000.;
        assert_eq!(panel_w(&state, window), window - NAV_WIDTH - DOCK_THREAD_MIN_W);
    }
}
