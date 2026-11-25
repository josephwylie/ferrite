//! The Factory's pickers and choices: a chooser whose rows open in the
//! window's menu, two or more choices side by side, and where a picker's
//! trigger was last drawn, so the window's menu hangs under it rather than
//! over it. A view draws the probe inside its trigger and says the
//! `Trigger` with what it asks; the window reads its bounds as it opens
//! the menu.

use std::cell::Cell;
use std::rc::Rc;

use gpui::component::button::Button;
use gpui::prelude::*;
use gpui::{Bounds, ClickEvent, Context, Div, Pixels, Point};

/// Where a picker's trigger was last drawn, so its menu hangs under it
/// rather than over it.
#[derive(Clone, Default)]
pub struct Trigger(Rc<Cell<Option<Bounds<Pixels>>>>);

impl Trigger {
    /// The trigger's child that records where it is drawn; the trigger is
    /// `relative`, so this fills it.
    pub fn probe(&self) -> impl IntoElement {
        let drawn = self.0.clone();
        gpui::canvas(move |bounds, _, _| drawn.set(Some(bounds)), |_, _, _, _| {})
            .absolute()
            .size_full()
    }

    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.0.get()
    }
}

/// The chooser face for any view, `press` given where its rows go: the
/// trigger it records itself in and the pointer.
pub fn picker_button<V: 'static>(
    selector: &'static str,
    text: String,
    flagged: bool,
    press: impl Fn(&mut V, Trigger, Point<Pixels>, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> Button {
    let look = ferrite_ui::controls::Look {
        flagged,
        full_width: true,
        disabled: false,
    };
    let trigger = Trigger::default();
    ferrite_ui::controls::chooser(selector, "", text.into(), look, cx)
        .debug_selector(move || {
            if flagged {
                format!("{selector}-flagged")
            } else {
                selector.to_string()
            }
        })
        .relative()
        .child(trigger.probe())
        .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
            cx.stop_propagation();
            press(view, trigger.clone(), event.position(), cx);
        }))
}

/// Two or more choices side by side, full width: the shared segments, the
/// chosen one selected as `{selector}-{at}-on`.
pub fn segmented<V: 'static, T: Copy + PartialEq + 'static>(
    selector: &'static str,
    chosen: T,
    options: &[(T, &'static str)],
    pick: impl Fn(&mut V, T, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> Div {
    let mut track =
        ferrite_ui::controls::segments(true).debug_selector(move || selector.to_string());
    for (at, (value, word)) in options.iter().enumerate() {
        let value = *value;
        let on = value == chosen;
        let key = format!("{selector}-{at}{}", if on { "-on" } else { "" });
        let pick = pick.clone();
        track = track.child(
            ferrite_ui::controls::segment((selector, at), *word, on, true, cx)
                .debug_selector(move || key.clone())
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| pick(view, value, cx))),
        );
    }
    track
}
