//! The Panel's controls as keyboard stops: the head's tabs, `+` and
//! maximize, and a pull request's number, primary action, `⋯` and view
//! segments.
//!
//! They form one roving row, in the order they are drawn. ←→ ⇱ ⇲ step along
//! it, ⏎ presses the stop (the button's own key-up click), ⌫ or ⌦ closes a
//! tab, ⇧F10 opens a stop's menu, and ⇥ ⇧⇥ or F6 leave for the next or
//! previous area. Esc hands the keyboard back to the conversation.
//! A menu opened from a stop leaves the keyboard on it.
//!
//! A stop is `PanelControl`; its `Semantics` (name, role, selected) are what
//! the accessibility tree is given and what a test reads, since gpui builds
//! no tree without assistive technology to ask for one. Each frame
//! re-registers the stops it draws, so a closed tab or a hidden view is no
//! longer a stop and cannot hold the keyboard; the last stop focused is
//! kept for each Thread so F6 comes back to it while it is still drawn.
//!
//! A stop is registered on the shared `PanelFocus` by whichever view draws
//! it, the root or a child view rendered after it. So the row is finished
//! only once the whole frame is drawn, and every reader sees the row of the
//! last finished frame, never one half drawn.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ferrite_ui::components::{icon_button, IconSize};
use gpui::component::button::Button;
use gpui::prelude::*;
use gpui::{App, FocusHandle, Role, SharedString, Window};

use super::*;
use crate::cockpit::surfaces::PrTab;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PanelControl {
    Tab(SurfaceId),
    /// A file open in the Files tool, by its place in that tool's tabs.
    FileTab(usize),
    Plus,
    Maximize,
    PrNumber,
    PrPrimary,
    PrMore,
    PrView(PrTab),
}

/// What assistive technology and a test are told of a stop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Semantics {
    pub name: SharedString,
    pub role: Role,
    pub selected: Option<bool>,
}

impl Semantics {
    pub(crate) fn button(name: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            role: Role::Button,
            selected: None,
        }
    }

    pub(crate) fn tab(name: impl Into<SharedString>, selected: bool) -> Self {
        Self {
            name: name.into(),
            role: Role::Tab,
            selected: Some(selected),
        }
    }
}

/// A stop made focusable: its handle, a tab stop that shows the focus ring
/// to the keyboard only, its role, name and selection. A press with the
/// pointer leaves the keyboard where it was, as the head's controls always
/// have.
pub(in crate::cockpit) fn focusable<E: StatefulInteractiveElement>(
    element: E,
    handle: &FocusHandle,
    semantics: &Semantics,
) -> E {
    let element = stop(element, handle)
        .role(semantics.role)
        .aria_label(semantics.name.clone());
    match semantics.selected {
        Some(selected) => element.aria_selected(selected),
        None => element,
    }
}

/// A stop that is the shared icon button, named as `semantics` say; the
/// button gives itself its role and name.
pub(in crate::cockpit) fn icon_stop(
    id: &'static str,
    glyph: &'static str,
    size: IconSize,
    handle: &FocusHandle,
    semantics: &Semantics,
    cx: &App,
) -> Button {
    debug_assert_eq!(semantics.role, Role::Button);
    stop(icon_button(id, glyph, size, semantics.name.clone(), cx), handle)
}

fn stop<E: InteractiveElement>(element: E, handle: &FocusHandle) -> E {
    element
        .track_focus(handle)
        .tab_stop(true)
        .key_context(ferrite_ui::keymap::PANEL_CONTROL)
        .focus_visible(ferrite_ui::components::control_focus)
        .capture_any_mouse_down(|_, window, _| window.prevent_default())
}

/// The Panel's row of stops and the keyboard's memory of them, shared by
/// every view that draws a stop.
#[derive(Default)]
pub(crate) struct PanelFocus {
    handles: RefCell<HashMap<PanelControl, FocusHandle>>,
    /// The stops the last finished frame drew, in order.
    drawn: RefCell<Vec<(PanelControl, Semantics)>>,
    /// The frame drawing the Panel now, until it is finished.
    drawing: RefCell<Option<Drawing>>,
    remembered: RefCell<HashMap<ThreadId, PanelControl>>,
    /// Where the stops that open a menu were last drawn, for it to hang
    /// from.
    bounds: RefCell<HashMap<PanelControl, gpui::Bounds<gpui::Pixels>>>,
}

/// A frame's stops as its views draw them, and the Thread whose Panel it
/// draws: none for a draft's preview or no Panel.
struct Drawing {
    thread: Option<ThreadId>,
    stops: Vec<(PanelControl, Semantics)>,
}

impl PanelFocus {
    /// The stops the last finished frame drew, in order, with what each
    /// says of itself.
    #[cfg(test)]
    pub(crate) fn drawn(&self) -> Vec<(PanelControl, Semantics)> {
        self.drawn.borrow().clone()
    }

    /// The root starts drawing the Panel of `thread`, or none. The row is
    /// finished once the frame is, after every child view has rendered.
    pub(super) fn begin(self: &Rc<Self>, thread: Option<ThreadId>, window: &Window, cx: &mut App) {
        let drawing = Drawing {
            thread,
            stops: Vec::new(),
        };
        // A window drawn twice before its effects run finishes once, with
        // the later draw's row.
        if self.drawing.replace(Some(drawing)).is_none() {
            let focus = self.clone();
            window.defer(cx, move |window, _| focus.finish(window));
        }
    }

    /// Draw a stop: its handle, made if new, and its place in the row.
    pub(in crate::cockpit) fn register(
        &self,
        control: PanelControl,
        semantics: Semantics,
        cx: &mut App,
    ) -> FocusHandle {
        match self.drawing.borrow_mut().as_mut() {
            Some(drawing) => drawing.stops.push((control.clone(), semantics)),
            None => debug_assert!(false, "a stop is drawn outside the Panel's frame"),
        }
        self.handles
            .borrow_mut()
            .entry(control)
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// The frame is drawn: its stops are the row, a stop no longer drawn
    /// lets go of its handle, and the stop in focus is remembered for the
    /// Thread.
    fn finish(&self, window: &Window) {
        let Some(Drawing { thread, stops }) = self.drawing.take() else {
            return;
        };
        let Some(thread) = thread else {
            *self.drawn.borrow_mut() = stops;
            return;
        };
        let drawn = |control: &PanelControl| stops.iter().any(|(shown, _)| shown == control);
        self.handles.borrow_mut().retain(|control, _| drawn(control));
        self.bounds.borrow_mut().retain(|control, _| drawn(control));
        *self.drawn.borrow_mut() = stops;
        if let Some(control) = self.focused(window) {
            self.remembered.borrow_mut().insert(thread, control);
        }
    }

    pub(crate) fn handle(&self, control: &PanelControl) -> Option<FocusHandle> {
        self.handles.borrow().get(control).cloned()
    }

    fn position(&self, control: &PanelControl) -> Option<usize> {
        self.drawn
            .borrow()
            .iter()
            .position(|(drawn, _)| drawn == control)
    }

    fn at(&self, index: usize) -> Option<PanelControl> {
        self.drawn
            .borrow()
            .get(index)
            .map(|(control, _)| control.clone())
    }

    fn count(&self) -> usize {
        self.drawn.borrow().len()
    }

    /// An element's size as `control`'s, to hang its menu from: laid over
    /// the stop, which stands `relative`.
    pub(in crate::cockpit) fn measured(self: &Rc<Self>, control: PanelControl) -> impl IntoElement {
        let focus = self.clone();
        gpui::canvas(
            move |at, _, _| {
                focus.bounds.borrow_mut().insert(control.clone(), at);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }

    /// A deleted Thread's stop is no longer remembered.
    pub(crate) fn retain_threads(&self, keep: impl Fn(ThreadId) -> bool) {
        self.remembered.borrow_mut().retain(|thread, _| keep(*thread));
    }

    /// The stop in `window`'s focus, if one is.
    pub(crate) fn focused(&self, window: &Window) -> Option<PanelControl> {
        self.focused_stop(window).map(|(control, _)| control)
    }

    /// The stop in `window`'s focus and what it says of itself.
    fn focused_stop(&self, window: &Window) -> Option<(PanelControl, Semantics)> {
        let handles = self.handles.borrow();
        self.drawn
            .borrow()
            .iter()
            .find(|(control, _)| {
                handles
                    .get(control)
                    .is_some_and(|handle| handle.is_focused(window))
            })
            .cloned()
    }
}

impl CockpitView {
    pub(crate) fn panel_control_focused(&self, window: &Window) -> bool {
        self.panel_pane().is_some() && self.panel.focus.focused(window).is_some()
    }

    /// What the Cockpit calls the place that holds the keyboard, for a
    /// headless driver to say: the Panel stop by its name in the row.
    pub(crate) fn keyboard_label(&self, window: &Window) -> Option<SharedString> {
        self.panel_pane()?;
        let (_, semantics) = self.panel.focus.focused_stop(window)?;
        Some(semantics.name)
    }

    /// The stop F6 arrives at: the one last in focus while it is drawn,
    /// else the shown tab, else the first stop.
    fn panel_entry(&self, thread: ThreadId) -> Option<PanelControl> {
        let focus = &self.panel.focus;
        let remembered = focus.remembered.borrow().get(&thread).cloned();
        remembered
            .filter(|control| focus.position(control).is_some())
            .or_else(|| {
                let shown = PanelControl::Tab(self.active_surface(thread)?);
                focus.position(&shown).map(|_| shown)
            })
            .or_else(|| focus.at(0))
    }

    fn focus_panel_control(
        &mut self,
        control: &PanelControl,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(handle) = self.panel.focus.handle(control) {
            if let (PanelControl::FileTab(at), Some(thread)) = (control, self.panel_thread()) {
                self.files.update(cx, |files, _| files.reveal_tab(thread, *at));
            }
            window.focus(&handle, cx);
            if matches!(control, PanelControl::Tab(_)) {
                let stop = self.panel.focus.bounds.borrow().get(control).copied();
                if let Some(stop) = stop {
                    self.panel.tabs_scroll.ease_into_view(stop, cx);
                }
            }
            cx.notify();
        }
    }

    /// The Panel takes the keyboard, at the stop it last had. False where
    /// no Panel is shown.
    pub(crate) fn focus_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(entry) = self
            .panel_thread()
            .and_then(|thread| self.panel_entry(thread))
        else {
            return false;
        };
        self.focus_panel_control(&entry, window, cx);
        true
    }

    /// The keyboard goes back to the conversation: render's focus rule
    /// hands it to the Pane's own line.
    pub(crate) fn leave_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// The keyboard leaves a control that reads no letters for the
    /// conversation: a Panel stop, or the transcript row or code action Tab
    /// put it on in the focused Pane. Whether it was on one.
    pub(in crate::cockpit) fn leave_control(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let index = self.focused();
        let in_transcript = self.panes.get(index).is_some_and(|pane| {
            pane.tool_focus().contains_focused(window, cx)
                || (pane.transcript_focus.contains_focused(window, cx)
                    && ferrite_pane::rich::controls_focused(window))
        });
        if !in_transcript && !self.panel_control_focused(window) {
            return false;
        }
        if let Some(pane) = self.panes.get_mut(index) {
            pane.clear_tool_target();
        }
        self.leave_panel(window, cx);
        true
    }

    /// A character no control bound, typed with the keyboard on one, is
    /// text for the conversation: the Composer takes the keyboard and the
    /// character with it.
    pub(in crate::cockpit) fn typing_leaves_control(
        &mut self,
        keystroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = keystroke.modifiers;
        // Space and ⏎ press the stop.
        let Some(text) = keystroke.key_char.as_deref().filter(|text| {
            !text.trim().is_empty()
                && !(modifiers.control || modifiers.alt || modifiers.platform || modifiers.function)
        }) else {
            return;
        };
        if !self.leave_control(window, cx) {
            return;
        }
        // The Composer is not the focus yet, so the platform would hand
        // this character to the control, which takes none.
        if let Some(composer) = self.panes.get(self.focused()).map(|pane| pane.composer.clone()) {
            composer.update(cx, |composer, cx| composer.insert(text, cx));
        }
        cx.stop_propagation();
    }

    /// F6 and ⇧F6: the next or previous area, round the conversation and the
    /// Panel's controls. Typing or reading in a Panel body is neither: its
    /// next area is the controls above it, not a way out of the Panel.
    pub(in crate::cockpit) fn step_area(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel_control_focused(window) {
            self.leave_panel(window, cx);
        } else {
            self.focus_panel(window, cx);
        }
    }

    pub(in crate::cockpit) fn panel_keys_held(&self, window: &Window, cx: &gpui::App) -> bool {
        self.panel_keys.contains_focused(window, cx)
    }

    /// A step along the row from the stop in focus; the ends stop.
    fn step_panel_control(
        &mut self,
        to: impl Fn(usize, usize) -> usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = &self.panel.focus;
        let Some(at) = focus
            .focused(window)
            .and_then(|control| focus.position(&control))
        else {
            return;
        };
        let Some(next) = focus.at(to(at, focus.count())) else {
            return;
        };
        self.focus_panel_control(&next, window, cx);
    }

    pub(in crate::cockpit) fn panel_control_previous(
        &mut self,
        _: &PanelControlPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_panel_control(|at, _| at.saturating_sub(1), window, cx);
    }

    pub(in crate::cockpit) fn panel_control_next(
        &mut self,
        _: &PanelControlNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_panel_control(
            |at, count| (at + 1).min(count.saturating_sub(1)),
            window,
            cx,
        );
    }

    pub(in crate::cockpit) fn panel_control_first(
        &mut self,
        _: &PanelControlFirst,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_panel_control(|_, _| 0, window, cx);
    }

    pub(in crate::cockpit) fn panel_control_last(
        &mut self,
        _: &PanelControlLast,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_panel_control(|_, count| count.saturating_sub(1), window, cx);
    }

    /// ⌘W while the Panel holds the keyboard closes what the keyboard is on:
    /// the tab in focus, else the Surface shown (the empty Panel, the
    /// Panel itself), never the Thread under it.
    pub(in crate::cockpit) fn close_panel_in_focus(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(thread) = self.panel_thread() else {
            return false;
        };
        let held = self.panel_control_focused(window)
            || self.panel_keys_held(window, cx)
            || self.terminal_holds_focus(window, cx)
            || self.side_chat_holds_focus(window, cx);
        if !held {
            return false;
        }
        let on_control = self.panel_control_focused(window);
        let focused = self.panel.focus.focused(window);
        if let Some(PanelControl::FileTab(at)) = focused {
            self.files.update(cx, |files, cx| files.close_tab(thread, at, cx));
            self.focus_after_file_close(thread, at, window, cx);
            return true;
        }
        let closing = match focused {
            Some(PanelControl::Tab(id)) => Some(id),
            _ => self.active_surface(thread),
        };
        match closing {
            Some(id) => {
                self.close_surface(thread, &id, cx);
                if on_control {
                    self.focus_after_close(thread, window, cx);
                }
            }
            None => self.toggle_panel_now(cx),
        }
        true
    }

    /// A tab has closed: the keyboard goes to the tab that takes its place.
    fn focus_after_close(&mut self, thread: ThreadId, window: &mut Window, cx: &mut Context<Self>) {
        let next = self
            .active_surface(thread)
            .map(PanelControl::Tab)
            .filter(|control| self.panel.focus.position(control).is_some())
            .unwrap_or(PanelControl::Plus);
        self.focus_panel_control(&next, window, cx);
    }

    /// A file tab has closed: the keyboard goes to the file that took its
    /// place, else back to the Panel's own tab.
    fn focus_after_file_close(
        &mut self,
        thread: ThreadId,
        closed: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let left = self.files.read(cx).tab_count(thread);
        let next = match left {
            0 => PanelControl::Tab(SurfaceId::Files),
            left => PanelControl::FileTab(closed.min(left - 1)),
        };
        self.focus_panel_control(&next, window, cx);
    }

    /// ⌫ or ⌦ on a tab closes it, and the keyboard goes to the tab that
    /// takes its place.
    pub(in crate::cockpit) fn panel_control_close(
        &mut self,
        _: &PanelControlClose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread) = self.panel_thread() else {
            return;
        };
        match self.panel.focus.focused(window) {
            Some(PanelControl::Tab(id)) => {
                self.close_surface(thread, &id, cx);
                self.focus_after_close(thread, window, cx);
            }
            Some(PanelControl::FileTab(at)) => {
                self.files.update(cx, |files, cx| files.close_tab(thread, at, cx));
                self.focus_after_file_close(thread, at, window, cx);
            }
            _ => {}
        }
    }

    /// ⇧F10 on a stop: its menu opens, where it has one.
    pub(in crate::cockpit) fn panel_control_menu(
        &mut self,
        _: &PanelControlMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread) = self.panel_thread() else {
            return;
        };
        let Some(control) = self.panel.focus.focused(window) else {
            return;
        };
        let target = match &control {
            PanelControl::Tab(id) => {
                let Some(at) = self
                    .panel
                    .surfaces
                    .get(thread)
                    .and_then(|state| state.tabs.iter().position(|tab| tab == id))
                else {
                    return;
                };
                MenuTarget::SurfaceTab { thread, at }
            }
            PanelControl::Plus => return self.toggle_door(super::strip::plus_door(thread), cx),
            PanelControl::PrNumber => MenuTarget::PrLink(thread),
            PanelControl::PrMore => {
                let Some(number) = self.pr_snapshot(thread, cx).map(|pr| pr.number) else {
                    return;
                };
                let target = super::pr_read::PrTarget::thread(thread, number);
                return self.toggle_door(super::pull_request::pr_more_door(target), cx);
            }
            _ => return,
        };
        self.open_panel_menu(control, target, cx);
    }

    /// Open `target`'s menu under the stop `control`, its left edge on the
    /// stop's. The keyboard comes back to the stop as it closes.
    pub(super) fn open_panel_menu(
        &mut self,
        control: PanelControl,
        target: MenuTarget,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.panel.focus.bounds.borrow().get(&control).copied() else {
            return;
        };
        let at = gpui::point(
            bounds.left(),
            bounds.bottom() + px(ferrite_ui::theme::FLOAT_OFFSET),
        );
        self.open_context_menu_at(target, at, gpui::Anchor::TopLeft, None, cx);
    }

}
