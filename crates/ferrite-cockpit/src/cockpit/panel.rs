//! The right panel (SPEC §5.6), after Claude's: a fixed column on the
//! frame, right of the board and outside it, holding one scroll of
//! sections with no tabs — Environment, Changes, Context, Connectors
//! (`sections`, `changes`). The pull request is a row in Changes once one
//! exists. The Review, which needs the board's width and takes line
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
//! (`preview_w`). It snaps between them, and open and shut, as Claude's
//! does: no ride, no fade, so nothing lays the board out again frame by
//! frame (`motion-measured.md`).
//!
//! A preview (`preview.rs`) is the one thing that brings a head: while a
//! Thread's Pane has one open, the head holds two tabs, `Overview` (the
//! sections) and the file's name. Otherwise the head row over the panel is
//! bare but for the toggle.
//!
//! One toggle (`panel_toggle_element`) sits at the head row's right end
//! either way, laid by the frame rather than by a head, so it never moves
//! or blinks as the panel comes and goes: the panel head leaves its room
//! while the panel shows, the view head while it is hidden
//! (`view_head_end`). Hiding the panel puts its preview away: a hidden
//! panel holds none.

mod changes;
pub(crate) mod hunks;
pub(crate) mod preview;
mod sections;

use super::*;
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
    /// The first frame has been tended: a preview open by then is simply
    /// there (`tend_previews`).
    tended: bool,
}

/// How wide the Panel is while a preview shows, in a window `width` wide
/// (the theme's `PANEL_PREVIEW_*`).
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

    /// Once per frame, before anything measures the board: tend the
    /// previews (`tend_previews`), read the width a preview takes in this
    /// window, and make sure the shown Thread's sections have their scroll.
    pub(super) fn tend_panel(&mut self, window: &mut Window) {
        self.tend_previews(window);
        self.panel.wide = preview_w(f32::from(window.viewport_size().width));
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
        match self.panel_pane() {
            Some(index) if self.preview_shown(index) => {
                self.panel.wide.max(ferrite_ui::theme::PANEL_W)
            }
            Some(_) => ferrite_ui::theme::PANEL_W,
            None => 0.,
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

    /// Where the view head ends, in from the window's right edge, and
    /// whether the toggle's room is its last part. Shown, it stops at the
    /// panel, whose head holds the toggle; hidden with a Thread for the
    /// panel, it runs to the toggle and ends in its room.
    pub(super) fn view_head_end(&self) -> (f32, bool) {
        let toggle = self.panel_candidate().is_some();
        let inset = panel_toggle_inset();
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

    /// The head row's one panel toggle (SPEC §5.6), while there is a Thread
    /// for the panel: laid over the row's right end, its right edge
    /// `panel_toggle_inset` in from the window's, over the panel head while
    /// the panel shows and over the view head's end while it is hidden.
    /// The frame lays it after both heads, so it stays put, hover and all.
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
        use ferrite_ui::theme::{ICON_BUTTON, NAV_TOGGLE_GLYPH, R_CONTROL};
        crate::nav::icon_button(
            "panel-toggle",
            icons::PANEL_RIGHT,
            NAV_TOGGLE_GLYPH,
            ICON_BUTTON,
            R_CONTROL,
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
        let previewing = self.preview_shown(index);
        let content = div()
            .size_full()
            .flex()
            .flex_col()
            .pl(px(PANEL_PAD_L))
            .pr(px(PANEL_PAD_R))
            .pb(px(PANEL_PAD_B))
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .text_color(rgb(INK))
            .whitespace_nowrap()
            .child(self.panel_head(owner, previewing, cx));
        let content = match owner.thread() {
            _ if previewing => content.child(self.panel_preview(owner, cx)),
            Some(thread) => {
                let scroll = self.panel.scrolls.get(&thread).cloned().unwrap_or_default();
                content.child(panel_body(self.panel_sections(thread, window, cx), &scroll))
            }
            None => content,
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
                .child(content)
                .into_any_element(),
        )
    }

    /// The head (in the head row): bare but for the title tools' and the
    /// toggle's rooms (the frame lays both over it) — unless the Pane
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
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(WIN_CHROME_H))
            .pr(px(panel_toggle_inset() - PANEL_PAD_R))
            .child(tabs.unwrap_or_else(|| div().flex_1()))
            .children(self.title_tools_room(SPACE_1))
            .child(toggle_room())
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
                    face.text_color(rgb(INK)).hover_raised(key).press_raised()
                }
            })
            .child("Overview")
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.deselect_preview_tab(thread);
                cx.notify();
            }))
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

/// The toggle's room at the end of whichever head the toggle lies over
/// (`panel_toggle_element`): the toggle's own box, kept clear.
pub(super) fn toggle_room() -> Div {
    div()
        .flex_shrink_0()
        .size(px(ferrite_ui::theme::ICON_BUTTON))
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

/// A fold's chevron in the panel (a file block's, the Changes line's): the
/// shared one (`components::fold_chevron`).
pub(super) fn fold_chevron(
    key: impl Into<SharedString>,
    unfolded: bool,
    size: f32,
    ink: u32,
) -> impl IntoElement {
    ferrite_ui::components::fold_chevron(key, unfolded, size, ink)
}
