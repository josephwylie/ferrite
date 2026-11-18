//! The one place a popup is up (ADR 0043). `Popups` holds the popup that
//! is up, or the one fading out after a close. `show_popup` is every open
//! and replaces whatever was up, at once; `close_popup` is every close.
//! While a popup is up it holds the keyboard at the root, Escape closes
//! it, and so does a press outside it.

use super::context_menu::ContextMenu;
use super::mode_menu::ModeMenu;
use super::*;
use ferrite_ui::motion::{self, FloatMotion, Opens};
use std::time::Instant;

/// A popup the slot can hold. The set is closed: one variant per kind.
#[derive(Clone)]
pub(super) enum Popup {
    /// Also the Factory's picks, the nav's menus and Snooze.
    ContextMenu(ContextMenu),
    /// The permission mode menu.
    Mode(ModeMenu),
    Plan(ThreadId),
    Checks(ThreadId),
    Session { thread: ThreadId, generation: u64 },
}

impl Popup {
    fn motion(&self) -> FloatMotion {
        match self {
            Popup::ContextMenu(_) => motion::MENU,
            _ => motion::POPOVER,
        }
    }

    /// Whether a press at `at`, outside the popup's surface, is still its
    /// own: on the chip that toggles the mode menu, which must shut it
    /// rather than see it closed and open it again, or on a context
    /// menu's submenu, drawn beside the menu.
    fn owns(&self, at: Point<Pixels>, triggers: &FloatTriggers) -> bool {
        let key = match self {
            Popup::Mode(menu) => mode_menu::trigger_key(menu.thread, menu.from_picker),
            Popup::ContextMenu(menu) if menu.sub.is_some() => context_menu::SUBMENU.into(),
            _ => return false,
        };
        triggers
            .borrow()
            .get(&key)
            .is_some_and(|bounds| bounds.contains(&at))
    }
}

enum Shown {
    Up(Popup),
    /// Closed to nothing at this instant, drawn while its exit runs.
    Leaving(Popup, Instant),
}

/// Where the keyboard goes back to as a popup closes.
enum Back {
    /// The nav list held the keys, its cursor here.
    Nav(Option<crate::nav::NavCursor>),
    Focus(FocusHandle),
}

#[derive(Default)]
pub(super) struct Popups {
    shown: Option<Shown>,
    /// What held focus before the popup up took it.
    back: Option<Back>,
}

impl Popups {
    fn up(&self) -> Option<&Popup> {
        match &self.shown {
            Some(Shown::Up(popup)) => Some(popup),
            _ => None,
        }
    }

    fn up_mut(&mut self) -> Option<&mut Popup> {
        match &mut self.shown {
            Some(Shown::Up(popup)) => Some(popup),
            _ => None,
        }
    }

    /// What to draw at `now`: the popup up, or the one leaving with when
    /// it left, until its exit has run.
    fn drawn(&mut self, now: Instant) -> Option<(Popup, Option<Instant>)> {
        if let Some(Shown::Leaving(popup, left)) = &self.shown {
            if now.saturating_duration_since(*left) >= popup.motion().out.duration() {
                self.shown = None;
            }
        }
        match &self.shown {
            Some(Shown::Up(popup)) => Some((popup.clone(), None)),
            Some(Shown::Leaving(popup, left)) => Some((popup.clone(), Some(*left))),
            None => None,
        }
    }
}

/// How a popup's face is drawn this frame: its kind's motion, and when it
/// left if it is leaving.
pub(super) struct Face {
    motion: FloatMotion,
    left: Option<Instant>,
}

impl CockpitView {
    /// A popup is up.
    pub(super) fn popup_up(&self) -> bool {
        self.popup().is_some()
    }

    pub(super) fn popup(&self) -> Option<&Popup> {
        self.popups.up()
    }

    pub(super) fn context_menu(&self) -> Option<&ContextMenu> {
        match self.popups.up() {
            Some(Popup::ContextMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(super) fn context_menu_mut(&mut self) -> Option<&mut ContextMenu> {
        match self.popups.up_mut() {
            Some(Popup::ContextMenu(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(super) fn mode_menu(&self) -> Option<&ModeMenu> {
        match self.popups.up() {
            Some(Popup::Mode(menu)) => Some(menu),
            _ => None,
        }
    }

    /// The mode the mode menu up previews.
    pub(super) fn mode_preview_mut(&mut self) -> Option<&mut Option<String>> {
        match self.popups.up_mut() {
            Some(Popup::Mode(menu)) => Some(&mut menu.preview),
            _ => None,
        }
    }

    /// Put `popup` up in place of whatever was, with no exit for that one.
    /// A rename in progress ends and the Composer's popover closes, apart
    /// from the model picker a mode menu opened from it hangs from.
    pub(super) fn show_popup(&mut self, popup: Popup, cx: &mut Context<Self>) {
        if !matches!(&popup, Popup::Mode(menu) if menu.from_picker) {
            self.popover = None;
        }
        if self.rename.is_some() {
            self.finish_rename(false, cx);
        }
        self.popups.shown = Some(Shown::Up(popup));
        cx.notify();
    }

    /// Close the popup up, if one is; whether one was. Every close comes
    /// here. It fades out unless motion is reduced.
    pub(super) fn close_popup(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(Shown::Up(popup)) = self
            .popups
            .shown
            .take_if(|shown| matches!(shown, Shown::Up(_)))
        else {
            return false;
        };
        if !motion::reduced_motion(cx) {
            self.popups.shown = Some(Shown::Leaving(popup, cx.background_executor().now()));
        }
        cx.notify();
        true
    }

    /// A popup takes the keyboard whole: the root holds focus while it is
    /// up, so no field's bindings (a Decision's, the Composer's) answer
    /// its keys. The focus it took comes back as it closes, unless what it
    /// did moved focus elsewhere. Tended once a frame.
    pub(super) fn tend_popup_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup_up() {
            if self.popups.back.is_none() {
                let nav = self.nav_view.read(cx);
                self.popups.back = Some(if nav.holds_keys(window) {
                    Back::Nav(nav.cursor.clone())
                } else {
                    Back::Focus(window.focused(cx).unwrap_or_else(|| self.focus.clone()))
                });
            }
            // A press that opened another popup over this one may have
            // focused a field under it.
            if !self.focus.is_focused(window) {
                window.focus(&self.focus, cx);
            }
        } else if let Some(back) = self.popups.back.take() {
            if self.focus.is_focused(window) {
                match back {
                    Back::Nav(cursor) => self.focus_nav_on(cursor, window, cx),
                    Back::Focus(handle) => window.focus(&handle, cx),
                }
            }
        }
    }

    /// A popup closes once it no longer stands rather than hold the
    /// keyboard with nothing drawn, or come back later: a Thread's popup
    /// with its Thread no longer focused or under a sheet, a Session's
    /// with that Session gone, the checks card with no checks, the plan
    /// card with no plan, a mode menu from the model picker with the
    /// picker shut. A page's dialog taking the keyboard closes any.
    pub(super) fn close_popup_with_nothing_to_draw(
        &mut self,
        dialog_took_focus: bool,
        cx: &mut Context<Self>,
    ) {
        let on = |thread: ThreadId, generation: Option<u64>| {
            self.focused_thread() == Some(thread)
                && !self.sheet_up(cx)
                && self.cockpit.thread(thread).is_some_and(|open| {
                    generation.is_none_or(|generation| open.generation() == generation)
                })
        };
        let stands = match self.popups.up() {
            None => return,
            Some(_) if dialog_took_focus => false,
            Some(Popup::ContextMenu(_)) => true,
            Some(Popup::Mode(menu)) => {
                let picker_up = self.model_picker_open(PaneIdentity::Thread(menu.thread));
                on(menu.thread, Some(menu.generation)) && (!menu.from_picker || picker_up)
            }
            Some(Popup::Plan(thread)) => {
                on(*thread, None)
                    && self
                        .cockpit
                        .thread(*thread)
                        .is_some_and(|open| open.transcript().todos().is_some())
            }
            Some(Popup::Checks(thread)) => {
                on(*thread, None) && self.facts.pr(*thread).is_some_and(|pr| pr.checks.is_some())
            }
            Some(Popup::Session { thread, generation }) => on(*thread, Some(*generation)),
        };
        if !stands {
            self.close_popup(cx);
        }
    }

    /// The popup up or leaving, drawn over everything but the lightbox:
    /// over the sheets too, whose pickers open the context menu.
    pub(super) fn popup_element(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (popup, left) = self.popups.drawn(cx.background_executor().now())?;
        let face = Face {
            motion: popup.motion(),
            left,
        };
        let element = match popup {
            Popup::ContextMenu(menu) => self.context_menu_element(&menu, face, window, cx),
            Popup::Mode(menu) => self.mode_menu_element(menu, face, window, cx),
            Popup::Plan(thread) => self.plan_card_element(thread, face, window, cx),
            Popup::Checks(thread) => self.context_checks_element(thread, face, window, cx),
            Popup::Session { thread, generation } => {
                self.session_controls_element(thread, generation, face, window, cx)
            }
        }?;
        Some(
            deferred(element)
                .with_priority(crate::float::FLOAT_PRIORITY)
                .into_any_element(),
        )
    }

    /// A popup's surface as drawn: a press on it is its own, a press
    /// outside it closes it while it is up, and it comes in or leaves
    /// with its kind's motion. Capture phase, so no handler under the
    /// press can keep it up.
    pub(super) fn popup_face<E>(
        &self,
        face: Face,
        id: &'static str,
        surface: E,
        opens: Opens,
        cx: &mut Context<Self>,
    ) -> AnyElement
    where
        E: Styled + InteractiveElement + IntoElement + 'static,
    {
        let surface = surface
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .when(face.left.is_none(), |surface| {
                surface.on_mouse_down_out(cx.listener(|view, event: &MouseDownEvent, _, cx| {
                    if !view
                        .popups
                        .up()
                        .is_some_and(|popup| popup.owns(event.position, &view.float_triggers))
                    {
                        view.close_popup(cx);
                    }
                }))
            });
        motion::float_at(face.motion, face.left, id, surface, opens)
    }
}
