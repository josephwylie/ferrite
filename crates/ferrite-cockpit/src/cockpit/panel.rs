//! The right panel (SPEC §5.6): a fixed column on the frame, right of the
//! board and outside it. Its head sits in the head row over it: the
//! `Changes`, `PR` and `Details` tabs, a `Preview` tab while a preview is
//! open (`preview.rs`), and the toggle that hides it. Under the head, the
//! shown tab's body; under `Changes` and `PR`, the foot's context meter
//! (`Details` reads that meter whole, so it has none). Every tab's
//! sections open with the one section head (`panel_section_head`). The
//! Review, which needs the board's width and takes line comments, still opens beside its Pane inside the board (ADR 0021).
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
//! Its width (`Rest`): hidden, `PANEL_W`, or wide while the Preview tab is
//! the one shown (`preview_w`). Moving between them the column rides
//! (`ride_panel`, `motion::ColumnRide`, the nav's cmd-B ride mirrored):
//! whatever moved it — ⌥⌘B, a preview opening or closing, a tab picked,
//! focus moving to a Pane with another — the board gives up or takes back
//! the width every frame. Coming and going the content fades on its own
//! clock; changing width it does not. The content is laid out at its
//! tab's width at rest (`content_w`) and never re-wraps: the Panel's own
//! width against the window's edge, clipped where the board's edge is, and
//! the wide preview's extra width pinned to the board's edge, so it slides
//! in with it and out under it (`overhang`). A column riding out after its
//! Pane left the focus still draws that Pane's tab (`Panel::last`) — a
//! preview closed as it hid, too, held until the ride ends.
//!
//! One toggle (`panel_toggle_element`) sits at the head row's right end
//! either way, laid by the frame rather than by a head, so it never moves
//! or blinks as the panel comes and goes: the panel head leaves its room
//! while the panel shows, the view head while it is hidden
//! (`view_head_end`). Hiding the panel puts its preview away: a hidden
//! panel holds none.
//!
//! One function per tab draws its body (`panel_changes`, `panel_context`,
//! the PR tab's `panel_pr`, in `pr_tab`, which also reads it, and the
//! preview's `panel_preview`): 7b adds a file's hunks under its head
//! (`panel_file_block`, `hunks`). A tab the operator picks fades its body
//! in (`motion::swap_in`); one shown because focus moved switches at once.
//! Each Thread's tab keeps its own scroll. A tab with nothing to show yet
//! says so in the Panel's one empty state (`panel_empty`).

mod details;
pub(crate) mod hunks;
pub(crate) mod preview;

use super::*;
use ferrite_core::docview::FileChange;
use ferrite_ui::icons::{self, icon};
use gpui::relative;

/// A panel tab. A Thread the panel has not shown yet opens on `Changes`;
/// `Preview` is a Pane's own while its preview is open (`preview.rs`),
/// never remembered as a Thread's tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum PanelTab {
    #[default]
    Changes,
    Pr,
    Details,
    Preview,
}

/// Where the column rests: what the board leaves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rest {
    Hidden,
    Shown,
    /// Its Preview tab shown.
    Wide,
}

/// The panel's own state on the cockpit.
#[derive(Default)]
pub(crate) struct Panel {
    /// The last tab each Thread's panel showed.
    tabs: std::collections::HashMap<ThreadId, PanelTab>,
    /// Each Thread's tabs' scroll, so a tab comes back where it was left
    /// rather than at another tab's offset.
    scrolls: std::collections::HashMap<(ThreadId, PanelTab), ScrollHandle>,
    /// The Changes tab's unfolded files and their hunks.
    hunks: hunks::Hunks,
    /// The column on its way between two widths (see the module doc).
    ride: ferrite_ui::motion::ColumnRide,
    /// Where the column rested at the last frame; `None` before the first,
    /// so a panel shown at launch arrives in place.
    rest: Option<Rest>,
    /// The Pane the panel last showed and its tab, drawn while the column
    /// rides out.
    last: Option<(PaneIdentity, PanelTab)>,
    /// What the board leaves at its right this frame while the column
    /// rides (`board_right`); `None` at rest.
    riding: Option<f32>,
    /// The width a preview reads at in this window (`preview_w`), as of
    /// the last frame.
    wide: f32,
    /// The first frame has been tended: a preview open by then is simply
    /// there (`tend_previews`).
    tended: bool,
    /// A scene's held ride out to a preview's width (`fixture_hold_panel`).
    hold: Option<f32>,
    /// The tab the operator last picked and how many picks there have
    /// been: that tab's body fades in under the count (`motion::swap_in`),
    /// so each pick plays once and a tab merely drawn again does not.
    picked: Option<(ThreadId, PanelTab, u64)>,
}

/// How wide the Panel is while its Preview tab shows, in a window `width`
/// wide (the theme's `PANEL_PREVIEW_*`).
fn preview_w(width: f32) -> f32 {
    use ferrite_ui::theme::*;
    (width * PANEL_PREVIEW_SHARE)
        .clamp(PANEL_PREVIEW_MIN_W, PANEL_PREVIEW_MAX_W)
        .min(width - NAV_WIDTH - PANEL_PREVIEW_BOARD_MIN)
        .max(PANEL_W)
}

impl CockpitView {
    /// The Pane whose panel shows this frame (see the module doc), if any.
    pub(super) fn panel_pane(&self) -> Option<usize> {
        let index = self.panel_candidate_index()?;
        let pane = &self.panes[index];
        (pane.preview.is_open() || self.prefs.settings.panel_shown && pane.thread().is_some())
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

    /// The tab Pane `index`'s panel shows: its preview's while that is
    /// selected, else its Thread's last.
    pub(super) fn shown_tab(&self, index: usize) -> PanelTab {
        if self.preview_shown(index) {
            return PanelTab::Preview;
        }
        match self.panes[index].thread() {
            Some(thread) => self.panel_tab(thread),
            None => PanelTab::Preview,
        }
    }

    /// How wide the panel is at rest in this window while its Preview tab
    /// shows.
    fn wide_w(&self) -> f32 {
        self.panel.wide.max(ferrite_ui::theme::PANEL_W)
    }

    fn rest_w(&self, rest: Rest) -> f32 {
        match rest {
            Rest::Hidden => ferrite_ui::theme::BOARD_PAD,
            Rest::Shown => ferrite_ui::theme::PANEL_W,
            Rest::Wide => self.wide_w(),
        }
    }

    /// Where the column rests this frame, and what it shows.
    fn panel_rest(&self) -> (Rest, Option<(PaneIdentity, PanelTab)>) {
        match self.panel_pane() {
            Some(index) => {
                let tab = self.shown_tab(index);
                let rest = if tab == PanelTab::Preview {
                    Rest::Wide
                } else {
                    Rest::Shown
                };
                (rest, Some((self.panes[index].identity, tab)))
            }
            None => (Rest::Hidden, None),
        }
    }

    /// Once per frame, before anything measures the board: tend the
    /// previews (`tend_previews`), ride the column when where it rests has
    /// changed since the last frame, for whatever reason, and read where
    /// the ride has the board's edge. A preview closed as the panel hid is
    /// held while the column rides out drawing it, and let go after.
    pub(super) fn ride_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        let reduced = ferrite_ui::motion::reduced_motion(cx);
        self.tend_previews(window);
        self.panel.wide = preview_w(f32::from(window.viewport_size().width));
        let (rest, shown) = self.panel_rest();
        if let Some((identity, tab)) = shown {
            self.panel.last = Some((identity, tab));
            if let Some(thread) = identity.thread().filter(|_| tab != PanelTab::Preview) {
                self.panel.scrolls.entry((thread, tab)).or_default();
            }
        }
        // Until a first Pane is there to show (a machine's window before
        // it connects, an empty board), nothing is recorded: the first
        // panel arrives in place, as one shown at launch does.
        if self.panel.rest.is_some() || rest != Rest::Hidden {
            if let Some(was) = self.panel.rest.replace(rest).filter(|was| *was != rest) {
                let fade = ((was == Rest::Hidden) != (rest == Rest::Hidden))
                    .then_some((rest != Rest::Hidden, crate::nav::CONTENT_FADE));
                let (from, to) = (self.rest_w(was), self.rest_w(rest));
                self.panel.ride.ride(from, to, fade, now, reduced);
            }
        }
        self.panel.riding = match self.panel.hold.filter(|_| rest == Rest::Wide) {
            Some(progress) => Some(ferrite_ui::motion::lerp(
                ferrite_ui::theme::PANEL_W,
                self.wide_w(),
                ferrite_ui::motion::RESIZE.progress(progress.clamp(0.0, 1.0)),
            )),
            None => self
                .panel
                .rest
                .and_then(|_| self.panel.ride.width(now, reduced)),
        };
        let held = match (shown, self.panel.riding, self.panel.last) {
            (None, Some(_), Some((identity, PanelTab::Preview))) => Some(identity),
            _ => None,
        };
        for pane in &self.panes {
            if Some(pane.identity) != held {
                pane.preview.let_go();
            }
        }
    }

    /// Hold the panel's ride out to a preview's width at `progress` (0 → 1
    /// along `motion::RESIZE`) so a scene can capture it mid-ride; `None`
    /// lets it go.
    #[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
    pub(crate) fn fixture_hold_panel(&mut self, progress: Option<f32>) {
        self.panel.hold = progress;
    }

    /// Whether this frame drew the column mid-ride: the frame asks for
    /// another.
    pub(super) fn panel_riding(&self) -> bool {
        self.panel.riding.is_some() && self.panel.hold.is_none()
    }

    /// What the board leaves at its right this frame: the panel's column,
    /// the frame's gutter when the panel is hidden (SPEC §4), or wherever
    /// the column's ride has the board's edge between the two. A measure
    /// taken at the destination (`board_level`'s, `update_auto_rail`'s)
    /// reads where the ride will end.
    pub(super) fn board_right(&self) -> f32 {
        let measuring = self.nav_measure_rest.get() || self.nav_measure_full.get();
        match self.panel.riding {
            Some(right) if !measuring => right,
            _ => self.board_right_rest(),
        }
    }

    /// What the board leaves at its right once the column's ride ends.
    pub(super) fn board_right_rest(&self) -> f32 {
        self.rest_w(self.panel_rest().0)
    }

    /// The panel's column where it comes to rest, 0 when it rests hidden:
    /// what stands over it (a toast) takes one place for the whole ride
    /// rather than following the riding width.
    pub(super) fn panel_rest_column_w(&self) -> f32 {
        match self.panel_rest() {
            (rest, Some(_)) => self.rest_w(rest),
            (_, None) => 0.,
        }
    }

    /// The Pane the panel draws this frame, its tab and its column's width:
    /// shown, or riding out after the operator hid it or its Pane left the
    /// board's focus.
    fn panel_drawn(&self) -> Option<(PaneIdentity, PanelTab, f32)> {
        match (self.panel_rest(), self.panel.riding) {
            ((rest, Some((identity, tab))), riding) => {
                Some((identity, tab, riding.unwrap_or_else(|| self.rest_w(rest))))
            }
            ((_, None), Some(width)) => {
                let (identity, tab) = self.panel.last?;
                Some((identity, tab, width))
            }
            ((_, None), None) => None,
        }
    }

    /// How much of the window's right edge the panel's column covers this
    /// frame: the board lies left of it, and keeps a gutter of its own
    /// where it covers none.
    pub(super) fn panel_column_w(&self) -> f32 {
        self.panel_drawn().map_or(0., |(_, _, width)| width)
    }

    /// Where the view head ends, in from the window's right edge, and
    /// whether the toggle's room is its last part. Shown, it stops at the
    /// panel, whose head holds the toggle; hidden with a Thread for the
    /// panel, it runs to the toggle and ends in its room. While the column
    /// rides it follows the board's edge, but never over the toggle's room,
    /// so its own controls ride with the board to where they come to rest.
    pub(super) fn view_head_end(&self) -> (f32, bool) {
        use ferrite_ui::theme::{ICON_BUTTON, VIEW_HEAD_GAP};
        let toggle = self.panel_candidate().is_some();
        let inset = panel_toggle_inset();
        if let Some(right) = self.panel.riding {
            let floor = if toggle {
                inset + ICON_BUTTON + VIEW_HEAD_GAP
            } else if ferrite_ui::frame::CUSTOM {
                inset
            } else {
                0.
            };
            return (right.max(floor), false);
        }
        // On Windows the caption buttons keep the row's right end either
        // way.
        if self.panel_pane().is_some() {
            (self.board_right(), false)
        } else if toggle {
            (inset, true)
        } else if ferrite_ui::frame::CUSTOM {
            (inset, false)
        } else {
            (self.board_right(), false)
        }
    }

    /// The tab `thread`'s panel shows.
    pub(crate) fn panel_tab(&self, thread: ThreadId) -> PanelTab {
        self.panel.tabs.get(&thread).copied().unwrap_or_default()
    }

    /// ⌥⌘B, the View menu, the palette and the head's toggle: show or
    /// hide the panel, saved.
    pub(super) fn toggle_panel(&mut self, _: &TogglePanel, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_panel_now(cx);
    }

    /// Hiding puts the shown Pane's preview away (a hidden panel holds
    /// none), so a panel shown for a preview alone hides too.
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
        cx.notify();
    }

    /// The panel on `tab` for `thread`, shown if it was hidden. Opening
    /// `Details` asks the Session for a fresh reading, as the bar's meter
    /// did.
    pub(crate) fn open_panel_tab(
        &mut self,
        thread: ThreadId,
        tab: PanelTab,
        cx: &mut Context<Self>,
    ) {
        if !self.prefs.settings.panel_shown {
            self.change_settings(|settings| settings.panel_shown = true, cx);
        }
        self.select_panel_tab(thread, tab, cx);
    }

    /// The head row's one panel toggle (SPEC §5.6), while there is a Thread
    /// for the panel: laid over the row's right end, its right edge
    /// `panel_toggle_inset` in from the window's, over the panel head while
    /// the panel shows and over the view head's end while it is hidden.
    /// The frame lays it after both heads, so it stays put — hover and all
    /// — while the column rides between them.
    pub(super) fn panel_toggle_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.panel_candidate()?;
        Some(
            div()
                .absolute()
                .top_0()
                .right(px(panel_toggle_inset()))
                .h(px(ferrite_ui::theme::WIN_CHROME_H))
                .flex()
                .items_center()
                .child(self.panel_toggle(cx))
                .into_any_element(),
        )
    }

    /// The toggle itself: the nav head's sidebar toggle's twin
    /// (`nav::icon_button`), the `panel-right` glyph, its tooltip and chord
    /// read from the keymap.
    fn panel_toggle(&self, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::{ICON_BUTTON, NAV_TOGGLE_GLYPH, R_TAB};
        crate::nav::icon_button(
            "panel-toggle",
            icons::PANEL_RIGHT,
            NAV_TOGGLE_GLYPH,
            ICON_BUTTON,
            R_TAB,
        )
        .occlude()
        .tooltip(ferrite_ui::menu::action_tooltip(
            "Toggle panel",
            "cockpit::TogglePanel",
        ))
        .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.toggle_panel_now(cx);
        }))
        .into_any_element()
    }

    fn select_panel_tab(&mut self, thread: ThreadId, tab: PanelTab, cx: &mut Context<Self>) {
        let shown = self
            .panel_pane()
            .map(|index| self.shown_tab(index))
            .filter(|_| self.panel_candidate().and_then(|owner| owner.thread()) == Some(thread));
        if shown.is_some_and(|shown| shown != tab) {
            let count = self.panel.picked.map_or(0, |(.., count)| count + 1);
            self.panel.picked = Some((thread, tab, count));
        }
        self.deselect_preview_tab(thread);
        let was = self.panel.tabs.insert(thread, tab);
        if tab == PanelTab::Details && was != Some(PanelTab::Details) {
            self.refresh_context(thread);
        }
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

    /// The panel's column, while it shows or rides: laid over the window's
    /// right edge from the top of the head row to the bar, after the head
    /// row's drag strip, so its tabs take their presses and the gaps
    /// between them still drag the window. The column is as wide as the
    /// board leaves it this frame; its content is laid out at its tab's
    /// width at rest (`content_w`), so a ride clips it and never re-wraps
    /// it: the Panel's own width held against the window's edge, a wide
    /// preview's beyond it pinned to the board's (`overhang`). Coming and
    /// going, it fades on the ride's own clock.
    pub(super) fn panel_element(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
        let (owner, tab, width) = self.panel_drawn()?;
        let opacity = self.panel.ride.opacity(
            cx.background_executor().now(),
            ferrite_ui::motion::reduced_motion(cx),
        );
        let content_w = if tab == PanelTab::Preview {
            self.wide_w()
        } else {
            PANEL_W
        };
        let thread = owner.thread();
        let content = div()
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(-overhang(content_w, width)))
            .w(px(content_w))
            .flex()
            .flex_col()
            .pl(px(PANEL_PAD_L))
            .pr(px(PANEL_PAD_R))
            .pb(px(PANEL_PAD_B))
            .when(opacity < 1.0, |content| content.opacity(opacity))
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .text_color(rgb(TEXT))
            .whitespace_nowrap()
            .child(self.panel_head(owner, tab, cx));
        let content = match (tab, thread) {
            (PanelTab::Preview, _) => content.child(self.panel_preview(owner, cx)),
            (_, Some(thread)) => {
                let body = match tab {
                    PanelTab::Pr => self.panel_pr(thread, window, cx),
                    PanelTab::Details => self.panel_details(thread, window, cx),
                    _ => self.panel_changes(thread, cx),
                };
                let scroll = self
                    .panel
                    .scrolls
                    .get(&(thread, tab))
                    .cloned()
                    .unwrap_or_default();
                let body = panel_body(body, &scroll);
                let body = match self.panel.picked {
                    Some((picked, at, count)) if picked == thread && at == tab => {
                        ferrite_ui::motion::swap_in(
                            SharedString::from(format!("panel-tab-in-{count}")),
                            body,
                        )
                        .into_any_element()
                    }
                    _ => body.into_any_element(),
                };
                // The Details tab reads the same meter whole: no foot under it.
                let foot = (tab != PanelTab::Details)
                    .then(|| self.panel_foot(thread))
                    .flatten();
                content.child(body).children(foot)
            }
            (_, None) => content,
        };
        Some(
            div()
                .debug_selector(|| "panel".into())
                .absolute()
                .top_0()
                .right_0()
                // A hairline short of the bar: the bar's top rule
                // (`BAR_TOP`) lies on the frame's last row, and the panel
                // paints after the bar, so it would cover the rule's run
                // under it. The bar keeps its line the window's width.
                .bottom(px(STATUS_BAR_H + ferrite_ui::theme::HAIRLINE_W))
                .w(px(width))
                .overflow_hidden()
                .bg(paint::PANEL)
                .child(content)
                .into_any_element(),
        )
    }

    /// The head (46, in the head row): a Thread's three tabs and its Pane's
    /// Preview tab while it has a preview, then the toggle's room
    /// (`panel_toggle_element` lays the toggle over it). On Windows it
    /// stops short of the caption buttons, which keep the row's right end.
    fn panel_head(&self, owner: PaneIdentity, shown: PanelTab, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let tabs = div()
            .flex()
            .flex_1()
            .min_w_0()
            .gap(px(SPACE_0_5))
            .when_some(owner.thread(), |tabs, thread| {
                tabs.children(self.thread_tabs(thread, shown, cx))
            })
            .children(
                self.preview_tab_label(owner)
                    .filter(|_| {
                        shown == PanelTab::Preview
                            || self
                                .index_of(owner)
                                .is_some_and(|index| self.panes[index].preview.is_open())
                    })
                    .map(|label| {
                        self.preview_tab_element(owner, label, shown == PanelTab::Preview, cx)
                    }),
            );
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(WIN_CHROME_H))
            .pr(px(panel_toggle_inset() - PANEL_PAD_R))
            .child(tabs)
            .child(toggle_room())
    }

    /// A Thread's tabs in the head: `Changes` with its count, `PR` with its
    /// number, `Details`.
    fn thread_tabs(
        &self,
        thread: ThreadId,
        shown: PanelTab,
        cx: &mut Context<Self>,
    ) -> [gpui::Stateful<Div>; 3] {
        use ferrite_ui::theme::*;
        let facts = self.facts.get(thread);
        let changed = facts.map_or(0, |facts| facts.changed_files.len());
        let pr = self.facts.pr(thread).map(|pr| pr.number);
        let tab = |tab: PanelTab, label: &'static str, count: Option<AnyElement>| {
            let id = gpui::ElementId::from(SharedString::from(format!("panel-tab-{tab:?}")));
            let key = ferrite_ui::pointer::hover_key(&id);
            let active = tab == shown;
            div()
                .id(id)
                .debug_selector(move || format!("panel-tab-{tab:?}"))
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(SPACE_1_5))
                .h(px(PANEL_TAB_H))
                .px(px(PANEL_TAB_PAD_X))
                .rounded(px(R_CONTROL))
                .text_size(px(FS_LABEL))
                .font_weight(W_LABEL)
                .occlude()
                .map(|face| {
                    // The shown tab carries the selection, and steps it up
                    // under the pointer; its blend is the same key either
                    // way, so the tab the pointer left while it was shown
                    // has let its hover go.
                    if active {
                        face.hover_carried(key).text_color(rgb(TEXT_STRONG))
                    } else {
                        face.text_color(rgb(TEXT_MUTED))
                            .hover_raised(key)
                            .press_raised()
                    }
                })
                .child(label)
                .children(count)
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.select_panel_tab(thread, tab, cx);
                }))
        };
        let changes_count = (changed > 0).then(|| {
            div()
                .text_size(px(FS_META))
                .font_weight(W_BODY)
                .text_color(rgb(TEXT_DIM))
                .child(SharedString::from(changed.to_string()))
                .into_any_element()
        });
        let pr_number = pr.map(|number| {
            div()
                .font_family(FONT_CODE)
                .text_size(px(FS_CHROME_CODE))
                .font_weight(W_BODY)
                .text_color(rgb(TEXT_DIM))
                .child(SharedString::from(format!("#{number}")))
                .into_any_element()
        });
        [
            tab(PanelTab::Changes, "Changes", changes_count),
            tab(PanelTab::Pr, "PR", pr_number),
            tab(PanelTab::Details, "Details", None),
        ]
    }

    /// The Changes tab: the branch row, the section line, one block per
    /// file the Thread edited, and `Review changes` — or `No changes yet`.
    /// The files are the agent's own edits (`ThreadFacts::changed_files`,
    /// with their rolled-up `+N −N`), not a git diff against the base
    /// (SPEC §8).
    fn panel_changes(&self, thread: ThreadId, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let Some(facts) = self.facts.get(thread) else {
            return div().into_any_element();
        };
        let branch = facts
            .status
            .as_ref()
            .and_then(|status| status.branch.clone())
            .map(SharedString::from)
            .or_else(|| facts.branch.clone());
        let mut body = div().flex().flex_col().gap(px(SPACE_0_5));
        if let Some(branch) = branch {
            body = body.child(self.panel_branch_row(branch, facts));
        }
        let files = &facts.changed_files;
        if files.is_empty() {
            return body
                .flex_1()
                .child(panel_empty(
                    "panel-no-changes",
                    Some(icons::FILE),
                    "No changes yet",
                    "Files this thread edits show here, each with its diff.",
                ))
                .into_any_element();
        }
        let added = files.iter().map(|file| file.added).sum();
        let removed = files.iter().map(|file| file.removed).sum();
        let count = files.len();
        let noun = if count == 1 { "file" } else { "files" };
        // The section line folds and unfolds every file at once, its
        // chevron on the rows' icon column and its words on their text's.
        // Under the pointer its words blend up to `TEXT`, as the nav's fold
        // headings do (a wash would run past the columns it is laid to); a
        // press lands at `TEXT_STRONG`.
        let section_key = SharedString::from(format!("panel-changes-section-{}", thread.get()));
        let all = self.panel_files_unfolded(thread, files.iter().map(|file| file.path.as_str()));
        let paths: Vec<String> = files.iter().map(|file| file.path.clone()).collect();
        body = body.child(
            div()
                .id("panel-changes-section")
                .debug_selector(|| "panel-changes-section".into())
                .relative()
                .flex()
                .items_center()
                .h(px(PANEL_SECTION_H))
                .mt(px(SPACE_2))
                .pl(px(PANEL_ROW_TEXT_X))
                .pr(px(PANEL_ROW_ICON_X))
                .text_size(px(FS_META))
                .text_color(ferrite_ui::motion::hover_blend(
                    &section_key,
                    rgb(TEXT_DIM).into(),
                    rgb(TEXT).into(),
                ))
                .on_hover(ferrite_ui::motion::hover_listener(section_key))
                .active(|style| style.text_color(rgb(TEXT_STRONG)))
                .cursor_pointer()
                .tooltip(ferrite_ui::menu::tooltip(if all {
                    "Collapse all"
                } else {
                    "Expand all"
                }))
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.toggle_panel_files(thread, paths.clone(), cx);
                }))
                .child(
                    div()
                        .absolute()
                        .left(px(PANEL_ROW_ICON_X))
                        .top(px((PANEL_SECTION_H - PANEL_ROW_ICON) / 2.))
                        .child(fold_chevron(
                            format!("panel-fold-all-{}", thread.get()),
                            all,
                            PANEL_ROW_ICON,
                            TEXT_DIM,
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .font_weight(W_LABEL)
                        .child(SharedString::from(format!("{count} {noun} changed"))),
                )
                .child(diffstat(added, removed)),
        );
        let cwd = self.thread_path(thread);
        for (index, file) in files.iter().enumerate() {
            body = body.child(self.panel_file_block(thread, index, file, cwd.as_deref(), cx));
        }
        let review_id = gpui::ElementId::from("panel-review-changes");
        let review_key = ferrite_ui::pointer::hover_key(&review_id);
        body.child(
            div()
                .id(review_id)
                .debug_selector(|| "panel-review-changes".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .h(px(PANEL_BUTTON_H))
                .mt(px(SPACE_3))
                .rounded(px(R_CONTROL))
                .bg(paint::RAISED)
                .text_size(px(FS_LABEL))
                .font_weight(W_LABEL)
                .text_color(rgb(TEXT_STRONG))
                .hover_control(review_key)
                .press_control()
                .child("Review changes")
                .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                    view.open_beside(
                        beside::Beside::Review(ferrite_reviewr::model::Scope::Uncommitted),
                        window,
                        cx,
                    );
                })),
        )
        .into_any_element()
    }

    /// The branch row (48): the branch, `from <default branch>` under it
    /// (none on the default branch itself: `main from main` says nothing),
    /// and the checkout's drift and dirt at the right when git has said
    /// (`↑2 ↓1 · 3 dirty`).
    fn panel_branch_row(&self, branch: SharedString, facts: &crate::facts::ThreadFacts) -> Div {
        use ferrite_ui::theme::*;
        let base = facts.default_branch.clone().filter(|base| *base != branch);
        let drift = facts.status.as_ref().and_then(|status| {
            let mut words = Vec::new();
            match (status.ahead, status.behind) {
                (0, 0) => {}
                (ahead, 0) => words.push(format!("\u{2191}{ahead}")),
                (0, behind) => words.push(format!("\u{2193}{behind}")),
                (ahead, behind) => words.push(format!("\u{2191}{ahead} \u{2193}{behind}")),
            }
            if status.dirty > 0 {
                words.push(format!("{} dirty", status.dirty));
            }
            (!words.is_empty()).then(|| words.join(" \u{b7} "))
        });
        panel_row(icons::BRANCH, TEXT_DIM, base.is_some())
            .debug_selector(|| "panel-branch".into())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(W_LABEL)
                            .text_color(rgb(TEXT_STRONG))
                            .child(branch),
                    )
                    .children(drift.map(|drift| {
                        div()
                            .flex_shrink_0()
                            .text_size(px(FS_META))
                            .text_color(rgb(TEXT_DIM))
                            .child(SharedString::from(drift))
                    })),
            )
            .children(base.map(|base| {
                div()
                    .truncate()
                    .text_size(px(FS_META))
                    .text_color(rgb(TEXT_DIM))
                    .child(SharedString::from(format!("from {base}")))
            }))
    }

    /// One changed file's block (r8): its head (48, `RAISED`) — the name,
    /// which opens the file in the preview as a file link does, its `+N −N`,
    /// and its directory under it — and, unfolded, its hunks (`hunks`).
    /// The rest of the head folds and unfolds them; under the pointer its
    /// icon turns to the fold's chevron, which turns a quarter with the
    /// fold (`fold_chevron`).
    fn panel_file_block(
        &self,
        thread: ThreadId,
        index: usize,
        file: &FileChange,
        cwd: Option<&std::path::Path>,
        cx: &mut Context<Self>,
    ) -> Div {
        use ferrite_ui::theme::*;
        let raw = std::path::PathBuf::from(&file.path);
        let path = match cwd {
            Some(cwd) if !raw.is_absolute() => cwd.join(&raw),
            _ => raw,
        };
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        // The directory as the operator knows it: under the Thread's
        // checkout, from there; elsewhere, whole.
        let dir = path
            .parent()
            .map(|dir| {
                cwd.and_then(|cwd| dir.strip_prefix(cwd).ok())
                    .unwrap_or(dir)
                    .display()
                    .to_string()
            })
            .filter(|dir| !dir.is_empty());
        let shown_path = match &dir {
            Some(dir) => format!("{dir}/{name}"),
            None => name.clone(),
        };
        let actions = file_actions(index, shown_path, path.clone(), dir.is_some(), cx);
        let unfolded = self.panel_file_unfolded(thread, file, cx);
        let group = SharedString::from(format!("panel-file-head-{index}"));
        let slot = div()
            .debug_selector(move || format!("panel-file-fold-{index}"))
            .relative()
            .size(px(PANEL_ROW_ICON))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .group_hover(group.clone(), |style| style.invisible())
                    .child(icon(icons::FILE, PANEL_ROW_ICON, TEXT_MUTED)),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .invisible()
                    .group_hover(group.clone(), |style| style.visible())
                    .child(fold_chevron(
                        format!("panel-fold-turn-{}-{}", thread.get(), file.path),
                        unfolded,
                        PANEL_ROW_ICON,
                        TEXT_MUTED,
                    )),
            );
        let fold_path = file.path.clone();
        let head = panel_row_with(slot, dir.is_some())
            .id(("panel-file-head", index))
            .group(group.clone())
            .bg(paint::RAISED)
            .cursor_pointer()
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.toggle_panel_file(thread, fold_path.clone(), cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .id(("panel-file", index))
                            .debug_selector(move || format!("panel-file-{index}"))
                            .flex_shrink(1.)
                            .min_w_0()
                            .truncate()
                            .cursor_pointer()
                            .font_weight(W_LABEL)
                            .text_color(rgb(TEXT_STRONG))
                            .child(SharedString::from(name))
                            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                view.open_preview(path.clone(), None, window, cx);
                            })),
                    )
                    .child(div().flex_1())
                    .child(diffstat(file.added, file.removed)),
            )
            .children(dir.map(|dir| {
                div()
                    .truncate()
                    .text_size(px(FS_META))
                    .text_color(rgb(TEXT_DIM))
                    .child(SharedString::from(dir))
            }))
            .child(
                actions
                    .invisible()
                    .group_hover(group, |style| style.visible()),
            );
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .rounded(px(R_BLOCK))
            .overflow_hidden()
            .child(head)
            .child(self.panel_hunks_element(thread, index, &file.path, unfolded))
    }

    /// The foot under every tab but `Details` (which reads it whole): a hairline, `Context` with its
    /// `84k / 200k`, and the context meter. None until the Session reports
    /// its usage: no reading is invented.
    fn panel_foot(&self, thread: ThreadId) -> Option<Div> {
        use ferrite_ui::theme::*;
        let open = self.cockpit.thread(thread)?;
        let usage = open.transcript().usage()?;
        let window = usage.context_window.filter(|window| *window > 0);
        let reading = match window {
            Some(window) => format!(
                "{} / {}",
                pane::compact_count(usage.total_tokens),
                pane::compact_count(window)
            ),
            None => pane::compact_count(usage.total_tokens),
        };
        let meter = window.map(|window| {
            context_meter(
                usage.total_tokens,
                window,
                open.transcript().context_details(),
            )
        });
        Some(
            div()
                .debug_selector(move || format!("panel-foot-{}", usage.total_tokens))
                .flex()
                .flex_col()
                .flex_shrink_0()
                .gap(px(PANEL_FOOT_GAP))
                .pt(px(PANEL_FOOT_PAD_T))
                .pb(px(PANEL_FOOT_PAD_B))
                .px(px(PANEL_ROW_ICON_X))
                .border_t_1()
                .border_color(paint::HAIRLINE)
                .text_size(px(FS_META))
                .child(
                    div()
                        .flex()
                        .text_color(rgb(TEXT_DIM))
                        .child(div().flex_1().font_weight(W_LABEL).child("Context"))
                        .child(
                            div()
                                .text_color(rgb(TEXT_MUTED))
                                .child(SharedString::from(reading)),
                        ),
                )
                .children(meter),
        )
    }
}

/// How far in from the window's right edge the panel toggle's right edge
/// sits, shown or hidden: the panel's own padding, or on Windows just left
/// of the caption buttons.
pub(super) fn panel_toggle_inset() -> f32 {
    use ferrite_ui::theme::{CAPTION_W, PANEL_PAD_R};
    if ferrite_ui::frame::CUSTOM {
        3. * CAPTION_W
    } else {
        PANEL_PAD_R
    }
}

/// How far content laid out `content_w` wide reaches past the window's
/// edge in a column `width` wide: none while it fits, or for the Panel's
/// own width (held against the edge, the board's edge clipping it); a wide
/// preview's width beyond that is pinned to the board's edge instead, so
/// it slides in and out with it.
fn overhang(content_w: f32, width: f32) -> f32 {
    (content_w - width).clamp(0., content_w - ferrite_ui::theme::PANEL_W)
}

/// A section's head, the Panel's one (30): its name in 12/500 `TEXT_DIM`
/// on the rows' content edge, its reading (a count, `3 connected`) at the
/// right — the PR tab's sections, the Details tab's, and the Context
/// card's own heading (`pane::context_usage`) alike.
pub(super) fn panel_section_head(
    selector: &'static str,
    name: &'static str,
    reading: Option<String>,
) -> Div {
    use ferrite_ui::theme::*;
    div()
        .debug_selector(move || selector.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(PANEL_SECTION_H))
        .px(px(PANEL_ROW_ICON_X))
        .text_size(px(FS_META))
        .text_color(rgb(TEXT_DIM))
        .child(div().flex_1().font_weight(W_LABEL).child(name))
        .children(reading.map(SharedString::from))
}

/// A tab's empty state, the Panel's one design for "nothing here yet" (no
/// changes, no pull request, no `gh`, no context reading): a badge holding
/// the tab's icon, a title, and a line saying what will show and when,
/// centred in the room the tab leaves it and raised a little
/// (`PANEL_EMPTY_*`). It grows to fill the body, so it is laid as the
/// body's last child.
pub(super) fn panel_empty(
    selector: &'static str,
    glyph: Option<&'static str>,
    title: &'static str,
    detail: impl IntoElement,
) -> Div {
    use ferrite_ui::theme::*;
    div()
        .debug_selector(move || selector.into())
        .flex()
        .flex_col()
        .flex_1()
        .items_center()
        .justify_center()
        .gap(px(PANEL_EMPTY_GAP))
        .py(px(SPACE_6))
        .pb(px(PANEL_EMPTY_LIFT))
        .px(px(SPACE_3))
        .children(glyph.map(|glyph| {
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(PANEL_EMPTY_BADGE))
                .rounded(px(R_PANE))
                .bg(paint::RAISED)
                .child(icon(glyph, PANEL_EMPTY_ICON, TEXT_MUTED))
        }))
        .child(
            div()
                .text_size(px(FS_UI))
                .font_weight(W_LABEL)
                .text_color(rgb(TEXT_STRONG))
                .child(title),
        )
        .child(
            div()
                .max_w(px(PANEL_EMPTY_W))
                .whitespace_normal()
                .text_center()
                .text_size(px(FS_LABEL))
                .line_height(px(PR_BODY_LH))
                .text_color(rgb(TEXT_DIM))
                .child(detail),
        )
}

/// The toggle's room at the end of whichever head the toggle lies over
/// (`panel_toggle_element`): the toggle's own box, kept clear.
pub(super) fn toggle_room() -> Div {
    div()
        .flex_shrink_0()
        .size(px(ferrite_ui::theme::ICON_BUTTON))
}

/// The tab's body, scrolling between the head and the foot on the Thread's
/// tab's own `scroll`, its overlay thumb in the column's right padding
/// (`PANEL_SCROLLBAR_INSET`) beside the rows rather than over them.
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

/// A fold's chevron in the panel (a file block's, the PR tab's): the
/// shared one (`components::fold_chevron`).
pub(super) fn fold_chevron(
    key: impl Into<SharedString>,
    unfolded: bool,
    size: f32,
    ink: u32,
) -> impl IntoElement {
    ferrite_ui::components::fold_chevron(key, unfolded, size, ink)
}

/// A panel row (48): its icon at the row's left, its lines (the caller's
/// children) on the text column, centred down the row. The icon sits on
/// the first of two lines, or centred beside one.
fn panel_row(glyph: &'static str, ink: u32, two_lines: bool) -> Div {
    panel_row_with(icon(glyph, ferrite_ui::theme::PANEL_ROW_ICON, ink), two_lines)
}

/// A panel row whose icon slot holds `slot`.
fn panel_row_with(slot: impl IntoElement, two_lines: bool) -> Div {
    use ferrite_ui::theme::*;
    div()
        .relative()
        .flex()
        .flex_col()
        .justify_center()
        .flex_shrink_0()
        .h(px(PANEL_ROW_H))
        .pl(px(PANEL_ROW_TEXT_X))
        .pr(px(PANEL_ROW_ICON_X))
        .rounded(px(R_CONTROL))
        .child(
            div()
                .absolute()
                .left(px(PANEL_ROW_ICON_X))
                .top(px(if two_lines {
                    PANEL_ROW_ICON_Y
                } else {
                    (PANEL_ROW_H - PANEL_ROW_ICON) / 2.
                }))
                .child(slot),
        )
}

/// A changed file's actions, laid over its head's diffstat and shown while
/// the pointer is on the head (`PANEL_FILE_ACTION`): copy the path as the
/// block reads it, and open the file in the app the system gives it. A
/// click on the name previews it; the rest of the head folds its hunks.
fn file_actions(
    index: usize,
    shown_path: String,
    path: std::path::PathBuf,
    two_lines: bool,
    cx: &mut Context<CockpitView>,
) -> Div {
    use ferrite_ui::theme::*;
    let door = |id: &'static str, glyph: &'static str, tooltip: &'static str| {
        let element = gpui::ElementId::from((id, index));
        let key = ferrite_ui::pointer::hover_key(&element);
        div()
            .id(element)
            .debug_selector(move || format!("{id}-{index}"))
            .flex()
            .items_center()
            .justify_center()
            .size(px(PANEL_FILE_ACTION))
            .rounded(px(R_SMALL))
            .cursor_pointer()
            .bg(ferrite_ui::motion::hover_blend(
                &key,
                paint::RAISED.into(),
                paint::SELECTED.into(),
            ))
            .on_hover(ferrite_ui::motion::hover_listener(key))
            .press_control()
            .tooltip(ferrite_ui::menu::tooltip(tooltip))
            .child(icon(glyph, PANEL_FILE_ACTION_GLYPH, TEXT_MUTED))
    };
    div()
        .absolute()
        // Centred on the name's line.
        .top(px((PANEL_ROW_H - LH_UI - if two_lines { LH_META } else { 0. }) / 2.
            + (LH_UI - PANEL_FILE_ACTION) / 2.))
        .right(px(SPACE_1_5))
        .flex()
        .gap(px(SPACE_0_5))
        .pl(px(SPACE_1))
        .bg(paint::RAISED)
        .child(
            div()
                .debug_selector(move || format!("panel-file-copy-{index}"))
                .child(
                    ferrite_ui::components::copy_button(("panel-file-copy", index), shown_path)
                        .tooltip("Copy path")
                        .door(PANEL_FILE_ACTION, PANEL_FILE_ACTION_GLYPH, R_SMALL)
                        .on_raised(),
                ),
        )
        .child(
            door("panel-file-open", icons::EXTERNAL, "Open in its app").on_click(
                cx.listener(move |_, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    ferrite_pane::preview::open_original(&path, window, cx);
                }),
            ),
        )
}

/// A file count's `+N −N` in the chrome's code face, the signs in the diff
/// hues (`pane::trail_diff_stat`).
fn diffstat(added: usize, removed: usize) -> Div {
    use ferrite_ui::theme::*;
    div()
        .flex_shrink_0()
        .font_family(FONT_CODE)
        .text_size(px(FS_CHROME_CODE))
        .child(pane::trail_diff_stat(added, removed))
}

/// The foot's meter: `pane::context_meter_segments` (the Details tab's bar
/// draws the same), 2px apart with rounded ends, then the empty track.
fn context_meter(total: u64, window: u64, details: Option<&ferrite_core::ContextDetails>) -> Div {
    use ferrite_ui::theme::*;
    let segments = pane::context_meter_segments(total, window, details);
    let segment = |ink: u32| div().h_full().rounded(px(R_TIGHT)).bg(rgb(ink));
    div()
        .debug_selector(|| "panel-meter".into())
        .flex()
        .gap(px(SPACE_0_5))
        .h(px(PANEL_METER_H))
        .children(segments.into_iter().map(|(ink, share)| {
            segment(ink)
                .flex_shrink_0()
                .w(relative(share.clamp(0., 1.)))
        }))
        .child(segment(PANEL_METER_TRACK).flex_1())
}
