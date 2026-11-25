//! The one place a popup is up (ADR 0043). `Popups` holds the popup that
//! is up, or the one fading out after a close. `show_popup` is every open
//! and replaces whatever was up, at once; `close_popup` is every close.
//! While a popup is up it says where the keyboard is (`Keys`), Escape
//! closes it, and so does a press outside it.

use super::context_menu::ContextMenu;
use super::mode_menu::ModeMenu;
use super::model_picker::ModelPicker;
use super::*;
use ferrite_ui::motion::{self, FloatMotion, Opens};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

/// A popup the slot can hold. The set is closed: one variant per kind.
pub(super) enum Popup {
    /// Also the Factory's picks, the nav's menus and Snooze.
    ContextMenu(ContextMenu),
    /// The permission mode menu.
    Mode(ModeMenu),
    Plan(ThreadKey),
    Checks(ThreadKey),
    Session { thread: ThreadKey, generation: u64 },
    /// The model picker, with the mode menu its footer opens inside it.
    ModelPicker(ModelPicker),
    /// A draft's Project or Workspace menu.
    Band(Popover<Band>),
    /// The Composer's `+` menu on one Pane, the keyboard on `focus`. The
    /// kit draws it beside its `+` and reads its own keys.
    Attach { pane: PaneIdentity, focus: FocusHandle },
}

/// Where the keyboard is while a popup is up.
pub(super) enum Keys {
    /// The root holds it, so no field under the popup hears a key and the
    /// Cockpit's own keys stand down.
    Root,
    /// A field of the popup's own holds it, and the Cockpit's own keys
    /// stand down.
    Field(FocusHandle),
    /// The popup's Pane's Composer keeps it, and the Cockpit's keys: a
    /// band menu rides the Composer's menu keys and the band's tab walk.
    Composer(PaneIdentity),
}

/// The surface of the mode menu inside the model picker, and the key its
/// bounds are recorded under.
pub(super) const PICKER_MODE: &str = "picker-mode-in";

impl Popup {
    fn keys(&self, cx: &gpui::App) -> Keys {
        match self {
            Popup::ModelPicker(picker) if picker.mode.is_none() => {
                Keys::Field(picker.view.read(cx).search.read(cx).focus_handle(cx))
            }
            Popup::Attach { focus, .. } => Keys::Field(focus.clone()),
            Popup::Band(menu) => Keys::Composer(menu.pane),
            _ => Keys::Root,
        }
    }

    fn motion(&self) -> FloatMotion {
        match self {
            Popup::ContextMenu(_) => motion::MENU,
            _ => motion::POPOVER,
        }
    }

    /// Whether a press at `at`, outside the popup's surface, is still its
    /// own: on the control that toggles it, which must shut it rather than
    /// see it closed and open it again, or on a surface drawn beside it (a
    /// context menu's submenu, the model picker's mode menu).
    fn owns(&self, at: Point<Pixels>, triggers: &Triggers) -> bool {
        let hit = |key: &str| triggers.hit(key, at);
        match self {
            Popup::Mode(menu) => hit(&mode_menu::chip_trigger(menu.thread)),
            Popup::ModelPicker(picker) => {
                hit(&model_picker::chip_trigger(picker.pane))
                    || picker.mode.is_some() && hit(PICKER_MODE)
            }
            Popup::Band(menu) => hit(&menu.kind.trigger()),
            Popup::ContextMenu(menu) => {
                menu.sub.is_some() && hit(context_menu::SUBMENU)
                    || menu.from_door && hit(&menu_button::Door::key(menu.target))
            }
            Popup::Plan(_) | Popup::Checks(_) | Popup::Session { .. } | Popup::Attach { .. } => {
                false
            }
        }
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
    Nav(Option<ferrite_nav::NavCursor>),
    Focus(FocusHandle),
}

#[derive(Default)]
pub(super) struct Popups {
    shown: Option<Shown>,
    /// What held focus before the popups took it, and where they last put
    /// it: the keyboard goes back only while it is still there.
    back: Option<(Back, FocusHandle)>,
    triggers: Triggers,
}

/// Where each control a popup hangs from stood as last laid out, by key,
/// recorded in prepaint. Every control's is kept, not only the open
/// popup's: a popup hangs from its control on its first frame, and a frame
/// that replays a cached view lays none of its controls out again.
#[derive(Clone, Default)]
pub(super) struct Triggers(Rc<RefCell<HashMap<SharedString, gpui::Bounds<Pixels>>>>);

/// One record is the same as itself: what it holds is laid out afresh
/// each frame.
impl PartialEq for Triggers {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Triggers {
    /// Records the laid-out bounds of a control a popup hangs from, under
    /// `key`.
    pub(super) fn record(
        &self,
        key: SharedString,
    ) -> impl FnOnce(gpui::Bounds<Pixels>, &mut Window, &mut gpui::App) + 'static {
        let sink = self.0.clone();
        move |bounds, _, _| {
            sink.borrow_mut().insert(key, bounds);
        }
    }

    fn get(&self, key: &str) -> Option<gpui::Bounds<Pixels>> {
        self.0.borrow().get(key).copied()
    }

    fn hit(&self, key: &str, at: Point<Pixels>) -> bool {
        self.get(key).is_some_and(|bounds| bounds.contains(&at))
    }
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

    pub(super) fn triggers(&self) -> Triggers {
        self.triggers.clone()
    }

    /// Drop the popup leaving once its exit has run at `now`.
    fn settle(&mut self, now: Instant) {
        if let Some(Shown::Leaving(popup, left)) = &self.shown {
            if now.saturating_duration_since(*left) >= popup.motion().out.duration() {
                self.shown = None;
            }
        }
    }

    /// What to draw: the popup up, or the one leaving with when it left.
    fn drawn(&self) -> Option<(&Popup, Option<Instant>)> {
        match &self.shown {
            Some(Shown::Up(popup)) => Some((popup, None)),
            Some(Shown::Leaving(popup, left)) => Some((popup, Some(*left))),
            None => None,
        }
    }
}

/// How a popup's face is drawn this frame: its kind's motion, and when it
/// left if it is leaving.
#[derive(Clone, Copy)]
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

    /// Where the `+` menu open on `pane` holds the keyboard.
    pub(super) fn attach_menu_focus(&self, pane: PaneIdentity) -> Option<FocusHandle> {
        match self.popups.up() {
            Some(Popup::Attach { pane: on, focus }) if *on == pane => Some(focus.clone()),
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
    /// A rename in progress ends and the Composer's `/`/`@` menu closes.
    pub(super) fn show_popup(&mut self, popup: Popup, cx: &mut Context<Self>) {
        self.close_composer_menu(cx);
        if self.rename.is_open() {
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

    /// Close every menu the board's Panes hold open themselves: a
    /// transcript row's, a Decision's allow menu, a reply's comment and the
    /// Composer's `/`/`@` menu. They draw over the toasts, so over a sheet
    /// too (`ferrite_ui::layer`): what comes up over the board (a sheet,
    /// the palette, the shortcuts) closes them, and none outlives the board
    /// it was opened on.
    pub(super) fn close_board_menus(&mut self, cx: &mut Context<Self>) {
        self.close_composer_menu(cx);
        self.decisions.update(cx, |decisions, cx| decisions.close_menus(cx));
        for pane in &self.panes {
            if let Some(transcript) = pane.transcript() {
                transcript.update(cx, |transcript, cx| transcript.close_menus(cx));
            }
            pane.composer.update(cx, |composer, cx| composer.close_comment(cx));
        }
    }

    /// Escape on the popup up: the mode menu inside the model picker shuts
    /// first, then the popup. Whether anything did.
    pub(super) fn dismiss_popup(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(Popup::ModelPicker(picker)) = self.popups.up_mut() {
            if picker.mode.take().is_some() {
                cx.notify();
                return true;
            }
        }
        self.close_popup(cx)
    }

    /// A popup takes the keyboard whole: the root holds focus while it is
    /// up, or the popup's own field does, so no field under it (a
    /// Decision's, the Composer's) answers its keys. The focus it took
    /// comes back as it closes, unless what it did moved focus elsewhere.
    /// Tended once a frame.
    pub(super) fn tend_popup_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wants = self.popup().map(|popup| match popup.keys(cx) {
            Keys::Root => Some(self.focus.clone()),
            Keys::Field(focus) => Some(focus),
            Keys::Composer(pane) => self
                .index_of(pane)
                .map(|index| self.panes[index].composer.focus_handle(cx)),
        });
        if let Some(wants) = wants.flatten() {
            let back = match self.popups.back.take() {
                Some((back, _)) => back,
                None => {
                    let nav = self.nav_view.read(cx);
                    if nav.holds_keys(window) {
                        Back::Nav(nav.cursor.clone())
                    } else {
                        Back::Focus(window.focused(cx).unwrap_or_else(|| self.focus.clone()))
                    }
                }
            };
            // A press that opened another popup over this one may have
            // focused a field under it.
            if !wants.is_focused(window) {
                window.focus(&wants, cx);
            }
            self.popups.back = Some((back, wants));
        } else if let Some((back, given)) = self.popups.back.take() {
            if given.is_focused(window) || self.focus.is_focused(window) {
                match back {
                    Back::Nav(cursor) => self.focus_nav_on(cursor, window, cx),
                    Back::Focus(handle) => window.focus(&handle, cx),
                }
            }
        }
    }

    /// Whether the popup up takes the Cockpit's own keys: the root's key
    /// context then carries `Popup`, and they stand down.
    pub(super) fn popup_takes_keys(&self, cx: &gpui::App) -> bool {
        self.popup()
            .is_some_and(|popup| !matches!(popup.keys(cx), Keys::Composer(_)))
    }

    /// The model picker up.
    pub(super) fn model_picker(&self) -> Option<&ModelPicker> {
        match self.popups.up() {
            Some(Popup::ModelPicker(picker)) => Some(picker),
            _ => None,
        }
    }

    pub(super) fn model_picker_mut(&mut self) -> Option<&mut ModelPicker> {
        match self.popups.up_mut() {
            Some(Popup::ModelPicker(picker)) => Some(picker),
            _ => None,
        }
    }

    pub(super) fn band_menu(&self) -> Option<&Popover<Band>> {
        match self.popups.up() {
            Some(Popup::Band(menu)) => Some(menu),
            _ => None,
        }
    }

    pub(super) fn band_menu_mut(&mut self) -> Option<&mut Popover<Band>> {
        match self.popups.up_mut() {
            Some(Popup::Band(menu)) => Some(menu),
            _ => None,
        }
    }

    /// Whether `pane` is the focused Pane with no sheet over it: where a
    /// Composer's popup stands.
    fn pane_shown_alone(&self, pane: PaneIdentity, cx: &gpui::App) -> bool {
        self.panes.get(self.focused(cx)).is_some_and(|focused| focused.identity == pane)
            && !self.sheet_up(cx)
    }

    /// A popup closes once it no longer stands rather than hold the
    /// keyboard with nothing drawn, or come back later: a Thread's or a
    /// Pane's popup with it no longer focused or under a sheet, a
    /// Session's with that Session gone, the checks card with no checks,
    /// the plan card with no plan, a draft's band menu or picker with the
    /// draft gone. The mode menu inside the picker goes with its Session.
    /// A page's dialog taking the keyboard closes any.
    pub(super) fn close_popup_with_nothing_to_draw(
        &mut self,
        dialog_took_focus: bool,
        cx: &mut Context<Self>,
    ) {
        let stale_mode = self
            .model_picker()
            .and_then(|picker| picker.mode.as_ref())
            .is_some_and(|menu| {
                self.cockpit
                    .read(cx)
                    .thread(menu.thread)
                    .is_none_or(|open| open.generation() != menu.generation)
            });
        if stale_mode {
            if let Some(picker) = self.model_picker_mut() {
                picker.mode = None;
            }
        }
        let on = |thread: ThreadKey, generation: Option<u64>| {
            self.focused_thread(cx) == Some(thread)
                && !self.sheet_up(cx)
                && self.cockpit.read(cx).thread(thread).is_some_and(|open| {
                    generation.is_none_or(|generation| open.generation() == generation)
                })
        };
        let stands = match self.popups.up() {
            None => return,
            Some(_) if dialog_took_focus => false,
            Some(Popup::ContextMenu(_)) => true,
            Some(Popup::Mode(menu)) => on(menu.thread, Some(menu.generation)),
            Some(Popup::ModelPicker(picker)) => {
                let pane = picker.pane;
                let lives = match pane {
                    PaneIdentity::Thread(thread) => self.cockpit.read(cx).thread(thread).is_some(),
                    PaneIdentity::Draft(_) => self
                        .index_of(pane)
                        .is_some_and(|index| self.panes[index].draft().is_some()),
                };
                self.pane_shown_alone(pane, cx) && lives
            }
            Some(Popup::Plan(thread)) => {
                on(*thread, None)
                    && self
                        .cockpit
                        .read(cx)
                        .thread(*thread)
                        .is_some_and(|open| open.transcript().todos().is_some())
            }
            Some(Popup::Checks(thread)) => {
                on(*thread, None) && self.facts.pr(*thread).is_some_and(|pr| pr.checks.is_some())
            }
            Some(Popup::Session { thread, generation }) => on(*thread, Some(*generation)),
            Some(Popup::Band(menu)) => {
                self.pane_shown_alone(menu.pane, cx)
                    && self.index_of(menu.pane).is_some_and(|index| self.panes[index].draft().is_some())
            }
            Some(Popup::Attach { pane, .. }) => self.pane_shown_alone(*pane, cx),
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
        self.popups.settle(cx.background_executor().now());
        let (popup, left) = self.popups.drawn()?;
        let face = Face {
            motion: popup.motion(),
            left,
        };
        let element = match popup {
            Popup::ContextMenu(menu) => self.context_menu_element(menu, face, window, cx),
            Popup::Mode(menu) => self.mode_menu_element(menu, face, window, cx),
            Popup::Plan(thread) => self.plan_card_element(*thread, face, window, cx),
            Popup::Checks(thread) => self.context_checks_element(*thread, face, window, cx),
            Popup::Session { thread, generation } => {
                self.session_controls_element(*thread, *generation, face, window, cx)
            }
            Popup::ModelPicker(picker) => self.model_picker_element(picker, face, window, cx),
            Popup::Band(menu) => self.band_menu_element(menu, face, window, cx),
            Popup::Attach { .. } => None,
        }?;
        Some(
            deferred(element)
                .with_priority(ferrite_ui::float::FLOAT_PRIORITY)
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
                surface.on_mouse_down_out(cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    view.pressed_outside(id, event.position, cx);
                }))
            });
        motion::float_at(face.motion, face.left, id, surface, opens)
    }

    /// A press at `at` outside the popup's surface `id`: the popup closes
    /// unless the press is its own. Outside the mode menu inside the model
    /// picker, only that menu shuts, unless the press is on the button
    /// that toggles it.
    fn pressed_outside(&mut self, id: &'static str, at: Point<Pixels>, cx: &mut Context<Self>) {
        if id == PICKER_MODE {
            if !self.popups.triggers.hit(model_picker::PICKER_PERMISSIONS, at) {
                if let Some(Popup::ModelPicker(picker)) = self.popups.up_mut() {
                    picker.mode = None;
                    cx.notify();
                }
            }
        } else if self
            .popups
            .up()
            .is_some_and(|popup| !popup.owns(at, &self.popups.triggers))
        {
            self.close_popup(cx);
        }
    }

    /// Where the control `key` stood as last laid out, for a popup to hang
    /// from; none yet asks for the frame that lays it out.
    pub(super) fn hang_from(&self, key: &str, window: &mut Window) -> Option<gpui::Bounds<Pixels>> {
        let trigger = self.popups.triggers.get(key);
        if trigger.is_none() {
            motion::request_frame(window);
        }
        trigger
    }

    /// Where the control `key` stands as last laid out, if it has been.
    pub(super) fn trigger(&self, key: &str) -> Option<gpui::Bounds<Pixels>> {
        self.popups.triggers.get(key)
    }

    /// Records the laid-out bounds of a control a popup hangs from, under
    /// `key`.
    pub(super) fn record_trigger(
        &self,
        key: SharedString,
    ) -> impl FnOnce(gpui::Bounds<Pixels>, &mut Window, &mut gpui::App) + 'static {
        self.popups.triggers.record(key)
    }
}
