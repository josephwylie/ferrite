//! A keyboard stop in the window's row of them, as the Panel's controls are
//! (`ferrite-cockpit`'s `panel/controls.rs`): what a stop says of itself,
//! and an element made one.
//!
//! The row is the window's: it holds each stop's handle and steps along
//! the stops in order. A feature view whose controls are stops in it says
//! which they are, in the order it draws them, as a key of its own and the
//! stop's `Semantics`; the window enters them in its row and hands the
//! view each one's handle to draw it with (ADR 0041's 2026-11-25
//! amendment). The view never writes the row.

use gpui::component::button::Button;
use gpui::prelude::*;
use gpui::{App, FocusHandle, Role, SharedString};

use crate::components::{control_focus, icon_button, IconSize};
use crate::keymap::PANEL_CONTROL;

/// What assistive technology and a test are told of a stop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Semantics {
    pub name: SharedString,
    pub role: Role,
    pub selected: Option<bool>,
}

impl Semantics {
    pub fn button(name: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            role: Role::Button,
            selected: None,
        }
    }

    pub fn tab(name: impl Into<SharedString>, selected: bool) -> Self {
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
pub fn focusable<E: StatefulInteractiveElement>(
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
pub fn icon_stop(
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
        .key_context(PANEL_CONTROL)
        .focus_visible(control_focus)
        .capture_any_mouse_down(|_, window, _| window.prevent_default())
}
