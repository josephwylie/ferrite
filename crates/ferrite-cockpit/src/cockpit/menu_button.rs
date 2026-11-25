//! A button that opens a menu (ADR 0043). Every one is drawn by
//! `Doors::button`, which owns what such a button does: the context menu
//! hangs under it from the edge it names, a press as the pointer goes down
//! or Enter or Space opens it, a second press shuts it, and while it is up
//! the button wears `CHIP_OPEN` and shows no tooltip. Escape closes the
//! menu and the slot gives the keyboard back to what had it.

use super::context_menu::MenuTarget;
use super::popups::Triggers;
use super::*;
use ferrite_ui::components::MenuButton;

/// The button's edge the menu's own edge stands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edge {
    Left,
    Right,
}

/// A button that opens `target`'s menu under itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Door {
    pub target: MenuTarget,
    pub edge: Edge,
}

impl Door {
    pub(crate) fn new(target: MenuTarget, edge: Edge) -> Self {
        Door { target, edge }
    }

    /// The key its bounds are recorded under in the slot.
    pub(super) fn key(target: MenuTarget) -> SharedString {
        format!("menu-door-{target:?}").into()
    }
}

/// What a menu button reads from the slot as it is drawn: the target whose
/// menu is up from its button, and where to record its bounds.
#[derive(Clone, PartialEq)]
pub(crate) struct Doors {
    up: Option<MenuTarget>,
    triggers: Triggers,
}

/// A view that draws menu buttons: what their press asks for and how their
/// tooltip looks there.
pub(crate) trait MenuHost: Sized + 'static {
    fn press_door(&mut self, door: Door, window: &mut Window, cx: &mut Context<Self>);
    fn tip<E: InteractiveElement>(element: E, text: SharedString) -> E;
}

impl Doors {
    /// Whether `door`'s menu is up, opened from its button.
    pub(crate) fn is_up(&self, door: Door) -> bool {
        self.up == Some(door.target)
    }

    /// The button that opens `target`'s menu, for a view of its own crate
    /// that draws it and asks for its menu by an event: whether the menu
    /// is up from it, and where its bounds go.
    pub(crate) fn door(&self, target: MenuTarget) -> ferrite_ui::components::MenuDoor {
        ferrite_ui::components::MenuDoor::new(self.up == Some(target), self.triggers.record(Door::key(target)))
    }

    /// `face` as the button that opens `door`'s menu, `tip` its tooltip.
    pub(crate) fn button<F, H>(
        &self,
        face: F,
        door: Door,
        tip: impl Into<SharedString>,
        cx: &mut Context<H>,
    ) -> F
    where
        F: MenuButton + 'static,
        H: MenuHost,
    {
        let tip = tip.into();
        let press = cx.listener(move |host, _: &(), window, cx| host.press_door(door, window, cx));
        face.menu_button(
            self.is_up(door),
            move |face| H::tip(face, tip),
            self.triggers.record(Door::key(door.target)),
            move |window, cx| press(&(), window, cx),
        )
    }
}

impl MenuHost for CockpitView {
    fn press_door(&mut self, door: Door, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_door(door, cx);
    }

    fn tip<E: InteractiveElement>(mut element: E, text: SharedString) -> E {
        element.interactivity().tooltip(ferrite_ui::menu::tooltip(text));
        element
    }
}

impl CockpitView {
    pub(super) fn doors(&self) -> Doors {
        Doors {
            up: self
                .context_menu()
                .filter(|menu| menu.from_door)
                .map(|menu| menu.target),
            triggers: self.popups.triggers(),
        }
    }

    /// A press on `door`'s button: its menu shuts if it is up, else opens
    /// under the button.
    pub(super) fn toggle_door(&mut self, door: Door, cx: &mut Context<Self>) {
        if self.doors().up == Some(door.target) {
            self.close_popup(cx);
            return;
        }
        let Some(button) = self.trigger(&Door::key(door.target)) else {
            return;
        };
        self.open_context_menu_below(door.target, button, door.edge, true, cx);
    }
}
