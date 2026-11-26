//! The focus pump's one rule for text fields (`render`): it never takes the
//! keyboard from a field the operator is typing in. A new field needs no
//! word in the pump; being a Composer is enough.

use super::*;

/// What the rule remembers from the frame before. Private to this module,
/// so the pump's one call a frame is the only thing that moves it.
#[derive(Default)]
pub(super) struct FieldWatch {
    /// What held the keyboard when the pump last looked.
    seen: Option<FocusHandle>,
    /// What the pump wanted the keyboard on then.
    handed: Option<FocusHandle>,
}

impl CockpitView {
    /// Whether a text field holds the keyboard the pump must leave alone:
    /// the focused node sits inside a Composer the last frame drew, and that
    /// Composer is none of the pump's own, a Pane's prompt or the field it
    /// `wanted` the keyboard on last frame (the Factory's answer box, a
    /// rename), which it takes back once it wants another; nor is it inside
    /// a Pane the operator has since moved away from.
    ///
    /// The last frame is all there is to read, and a field opened by a key
    /// (⌘F's bar) takes the keyboard before it is ever drawn. So a focus
    /// that is new since the pump last looked, on something not yet drawn,
    /// is left alone for the one frame that draws it, and judged on the next.
    ///
    /// Called once a frame, by the pump: it moves `FieldWatch` on.
    pub(super) fn field_holds_keyboard(
        &mut self,
        wanted: &FocusHandle,
        window: &Window,
        cx: &gpui::App,
    ) -> bool {
        let focus = window.focused(cx);
        let fresh = focus != self.field_watch.seen;
        self.field_watch.seen = focus.clone();
        let handed = self
            .field_watch
            .handed
            .replace(wanted.clone())
            .filter(|handed| *handed != self.focus);
        let Some(focus) = focus else {
            return false;
        };
        let prompt = self
            .panes
            .iter()
            .any(|pane| pane.read(cx).composer.read(cx).focus_handle(cx).contains_focused(window, cx));
        let pumped = handed.is_some_and(|handed| handed.contains_focused(window, cx));
        if prompt || pumped || focus == self.focus {
            return false;
        }
        if !self.focus.contains_focused(window, cx) {
            return fresh;
        }
        let focused = self.focused(cx);
        let in_composer = window
            .context_stack()
            .iter()
            .any(|context| context.contains(ferrite_pane::composer::KEY_CONTEXT));
        in_composer
            && !self.panes.iter().enumerate().any(|(index, pane)| {
                index != focused && pane.read(cx).controls_focus.contains_focused(window, cx)
            })
    }

    /// A field that stops being drawn while it holds the keyboard (a find
    /// bar closed by Esc) leaves it on nothing drawn, and no frame follows
    /// to take it back: the window asks for one. The notify waits for the
    /// draw to end, since one inside it marks nothing dirty.
    pub(super) fn watch_lost_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.on_focus_lost(window, |_, window, cx| cx.defer_in(window, |_, _, cx| cx.notify()))
            .detach();
    }
}
