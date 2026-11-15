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

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{FocusHandle, Role, SharedString, Window};

use super::*;
use crate::cockpit::surfaces::PrTab;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PanelControl {
    Tab(SurfaceId),
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
pub(super) fn focusable<E: StatefulInteractiveElement>(
    element: E,
    handle: &FocusHandle,
    semantics: &Semantics,
) -> E {
    let element = element
        .track_focus(handle)
        .tab_stop(true)
        .key_context(ferrite_ui::keymap::PANEL_CONTROL)
        .focus_visible(ferrite_ui::components::control_focus)
        .capture_any_mouse_down(|_, window, _| window.prevent_default())
        .role(semantics.role)
        .aria_label(semantics.name.clone());
    match semantics.selected {
        Some(selected) => element.aria_selected(selected),
        None => element,
    }
}

/// The stops drawn this frame and the keyboard's memory of them.
#[derive(Default)]
pub(crate) struct PanelFocus {
    handles: RefCell<HashMap<PanelControl, FocusHandle>>,
    drawn: RefCell<Vec<(PanelControl, Semantics)>>,
    remembered: RefCell<HashMap<ThreadId, PanelControl>>,
    /// Where the stops that open a menu were last drawn, for it to hang
    /// from.
    bounds: Rc<RefCell<HashMap<PanelControl, gpui::Bounds<gpui::Pixels>>>>,
}

impl PanelFocus {
    /// The stops now drawn, in order, with what each says of itself.
    #[cfg(test)]
    pub(crate) fn drawn(&self) -> Vec<(PanelControl, Semantics)> {
        self.drawn.borrow().clone()
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

    /// A deleted Thread's stop is no longer remembered.
    pub(crate) fn retain_threads(&self, keep: impl Fn(ThreadId) -> bool) {
        self.remembered.borrow_mut().retain(|thread, _| keep(*thread));
    }

    /// The stop in `window`'s focus, if one is.
    pub(crate) fn focused(&self, window: &Window) -> Option<PanelControl> {
        let handles = self.handles.borrow();
        self.drawn
            .borrow()
            .iter()
            .map(|(control, _)| control)
            .find(|control| {
                handles
                    .get(*control)
                    .is_some_and(|handle| handle.is_focused(window))
            })
            .cloned()
    }
}

impl CockpitView {
    pub(super) fn begin_panel_controls(&self) {
        self.panel.focus.drawn.borrow_mut().clear();
    }

    /// Draw a stop: its handle, made if new, and its place in the row.
    pub(super) fn panel_control(
        &self,
        control: PanelControl,
        semantics: Semantics,
        cx: &mut Context<Self>,
    ) -> FocusHandle {
        let focus = &self.panel.focus;
        focus.drawn.borrow_mut().push((control.clone(), semantics));
        focus
            .handles
            .borrow_mut()
            .entry(control)
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// The frame's stops are all drawn: a stop no longer drawn lets go of
    /// its handle, and the stop in focus is remembered for `thread`.
    pub(super) fn end_panel_controls(&self, thread: ThreadId, window: &Window) {
        let focus = &self.panel.focus;
        let drawn = focus.drawn.borrow();
        focus
            .handles
            .borrow_mut()
            .retain(|control, _| drawn.iter().any(|(shown, _)| shown == control));
        focus
            .bounds
            .borrow_mut()
            .retain(|control, _| drawn.iter().any(|(shown, _)| shown == control));
        drop(drawn);
        if let Some(control) = focus.focused(window) {
            focus.remembered.borrow_mut().insert(thread, control);
        }
    }

    pub(crate) fn panel_control_focused(&self, window: &Window) -> bool {
        self.panel_pane().is_some() && self.panel.focus.focused(window).is_some()
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
            window.focus(&handle, cx);
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

    /// F6 and ⇧F6: the next or previous area, round the conversation and the
    /// Panel.
    pub(in crate::cockpit) fn step_area(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel_control_focused(window) || self.panel_keys_held(window, cx) {
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
        let closing = match self.panel.focus.focused(window) {
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
        let Some(PanelControl::Tab(id)) = self.panel.focus.focused(window) else {
            return;
        };
        self.close_surface(thread, &id, cx);
        self.focus_after_close(thread, window, cx);
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
            PanelControl::Plus => MenuTarget::SurfaceMenu(thread),
            PanelControl::PrNumber => MenuTarget::PrLink(thread),
            PanelControl::PrMore => MenuTarget::PrMore(thread),
            _ => return,
        };
        self.open_panel_menu(control, target, cx);
    }

    /// Open `target`'s menu under the stop `control`, its left edge on the
    /// stop's. The keyboard stays on the stop.
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

    /// An element's size as `control`'s, to hang its menu from: laid over
    /// the stop, which stands `relative`.
    pub(super) fn measured(&self, control: PanelControl) -> impl IntoElement {
        let bounds = self.panel.focus.bounds.clone();
        gpui::canvas(
            move |at, _, _| {
                bounds.borrow_mut().insert(control.clone(), at);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
    }
}
