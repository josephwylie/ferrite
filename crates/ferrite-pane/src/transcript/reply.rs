//! Cite a selection, as T3 does (`AssistantSelectionToolbar`): a drag that
//! selects a passage of the transcript (anyone's words, the operator's or
//! the agent's) ends with a glass button at the pointer, `Cite`, beside
//! `Start a side chat`. `Cite` quotes the passage into the Composer
//! (`TranscriptEvent::Reply`, a Reply card in the Pane's tray); a side chat
//! opens the right-docked pane about this passage
//! (`TranscriptEvent::SideChat`). A passage too long to quote offers a
//! disabled `Shorten selection` instead.
//!
//! The button is placed where the mouse went up, inside the window by
//! `SELECTION_CITE_EDGE`; where the pointer is not on the selection (a
//! keyboard or a far release) it stands at the selection's last line's end,
//! `SELECTION_CITE_BELOW` under it. It shows at once, or
//! `SELECTION_CITE_MULTI_CLICK_MS` after a double or triple click. A press
//! clears the selection (the text selection's capture phase), so the
//! button acts on the press, not the click. It goes when the selection
//! does (a press anywhere else), on Escape, and a scroll fades it out over
//! `FADE_QUICK`.

use std::time::{Duration, Instant};

use gpui::{
    prelude::*, px, AnyElement, Bounds, Context, MouseButton, MouseDownEvent, MouseUpEvent,
    Pixels, Point, SharedString, Size, Window,
};

use super::{TranscriptEvent, TranscriptView};
use crate::quote::Reply;
use ferrite_ui::{icons, motion, theme};

/// The button on offer: the passage it quotes (none when the selection is
/// too long to), read when the drag ended (a press clears the selection
/// before a click could land), where the pointer let go, when it appears,
/// and when a scroll began to fade it.
pub(super) struct SelectionReply {
    passage: Option<String>,
    at: Point<Pixels>,
    appears: Instant,
    fading: Option<Instant>,
}

impl TranscriptView {
    /// A left press over the transcript: where a drag would select from.
    pub(super) fn pressed(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Left {
            self.pressed_at = Some(event.position);
        }
    }

    /// A press let go over the transcript: offer the Cite once the
    /// selection it finished has settled, if it selected a passage.
    pub(super) fn released(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pressed_at = None;
        if !self.input.focused || !self.input.quotes {
            return;
        }
        let at = event.position;
        let delay = if event.click_count >= 2 {
            Duration::from_millis(theme::SELECTION_CITE_MULTI_CLICK_MS)
        } else {
            Duration::ZERO
        };
        cx.defer_in(window, move |view, window, cx| {
            let selected = gpui::base::TextSelection::selected_text(window, cx);
            let passage = crate::quote::passage(&selected);
            view.reply = (passage.is_some() || crate::quote::too_long(&selected)).then(|| {
                SelectionReply {
                    passage,
                    at,
                    appears: cx.background_executor().now() + delay,
                    fading: None,
                }
            });
            if !delay.is_zero() {
                cx.spawn(async move |view, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = view.update(cx, |_, cx| cx.notify());
                })
                .detach();
            }
            cx.notify();
        });
    }

    /// A scroll under the button: it fades out, then goes (at once under
    /// reduced motion).
    pub(super) fn scrolled(&mut self, cx: &mut Context<Self>) {
        let Some(reply) = self.reply.as_mut() else {
            return;
        };
        if reply.fading.is_some() {
            return;
        }
        if motion::reduced_motion(cx) {
            self.dismiss_reply(cx);
            return;
        }
        reply.fading = Some(cx.background_executor().now());
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(motion::FADE_QUICK.duration())
                .await;
            let _ = view.update(cx, |view, cx| view.dismiss_reply(cx));
        })
        .detach();
        cx.notify();
    }

    pub(super) fn dismiss_reply(&mut self, cx: &mut Context<Self>) {
        if self.reply.take().is_some() {
            cx.notify();
        }
    }

    /// Escape: the button goes. Whether one was on offer.
    pub fn dismiss_offer(&mut self, cx: &mut Context<Self>) -> bool {
        let offered = self.reply.is_some();
        self.dismiss_reply(cx);
        offered
    }

    /// Whether the Cite button is on offer — what the tests read.
    #[cfg(any(test, feature = "test-support"))]
    pub fn reply_offered(&self) -> bool {
        self.reply.is_some()
    }

    /// The buttons, while their selection stands: at the pointer
    /// (`cite_place`), fading once a scroll has begun, none before they
    /// appear.
    pub(super) fn reply_float(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.reply.is_none() {
            return None;
        }
        if !self.input.focused || !gpui::base::TextSelection::has_selection(window, cx) {
            self.reply = None;
            return None;
        }
        let now = cx.background_executor().now();
        let reply = self.reply.as_ref()?;
        if now < reply.appears {
            return None;
        }
        let opacity = match reply.fading {
            Some(since) => {
                window.request_animation_frame();
                1. - motion::FADE_QUICK.progress_at(now.saturating_duration_since(since))
            }
            None => 1.,
        };
        let side_chat_shown = self.input.rewind.is_some();
        let size = cite_size(reply.passage.is_none(), side_chat_shown, window);
        let rects = gpui::base::TextSelection::painted_bounds(window, cx);
        let place = cite_place(
            self.scroll.list_state().viewport_bounds(),
            &rects,
            reply.at,
            size,
            window.viewport_size(),
        );
        let passage = reply.passage.clone();
        let fading = reply.fading.is_some();
        let weak = cx.entity().downgrade();
        let away = weak.clone();
        let namespace = self.input.namespace.clone();
        let id = gpui::ElementId::from(SharedString::from(format!(
            "selection-reply-{}",
            self.input.namespace
        )));
        // On the press, not the click: the press clears the selection,
        // and with it this button, before a release could land on it.
        let cite = match passage.clone() {
            Some(passage) => glass_button(&namespace, "selection-reply-quote", "Cite", true)
                .cursor_pointer()
                .when(!fading, |button| {
                    let weak = weak.clone();
                    button.on_mouse_down(
                        MouseButton::Left,
                        move |_: &MouseDownEvent, window: &mut Window, cx: &mut gpui::App| {
                            cx.stop_propagation();
                            gpui::base::TextSelection::clear(window, cx);
                            let passage = passage.clone();
                            let _ = weak.update(cx, |view, cx| {
                                view.reply = None;
                                cx.emit(TranscriptEvent::Reply(Reply::new(passage, "")));
                                cx.notify();
                            });
                        },
                    )
                }),
            None => glass_button(&namespace, "selection-reply-quote", "Shorten selection", false)
                .opacity(0.5)
                .tooltip(ferrite_ui::menu::tooltip("Selection is too long to cite")),
        };
        let side_chat = passage.clone().zip(self.input.rewind.as_ref()).map(|(passage, allowed)| {
            let button = glass_button(&namespace, "selection-side-chat", "Start a side chat", false);
            match allowed {
                Err(why) => button
                    .opacity(0.4)
                    .tooltip(ferrite_ui::menu::tooltip(format!(
                        "Start a side chat \u{b7} {why}"
                    ))),
                Ok(()) => button.when(!fading, |button| {
                    let weak = weak.clone();
                    button.cursor_pointer().on_mouse_down(
                        MouseButton::Left,
                        move |_: &MouseDownEvent, window: &mut Window, cx: &mut gpui::App| {
                            cx.stop_propagation();
                            gpui::base::TextSelection::clear(window, cx);
                            let passage = passage.clone();
                            let _ = weak.update(cx, |view, cx| {
                                view.reply = None;
                                cx.emit(TranscriptEvent::SideChat(passage));
                                cx.notify();
                            });
                        },
                    )
                }),
            }
        });
        let row = gpui::div()
            .id(id)
            .debug_selector(|| "selection-reply".into())
            .flex()
            .items_center()
            .gap(px(theme::SELECTION_CITE_GAP))
            .opacity(opacity)
            .occlude()
            .on_mouse_down_out(move |_, _, cx| {
                let _ = away.update(cx, |view, cx| view.dismiss_reply(cx));
            })
            .child(cite)
            .children(side_chat);
        Some(
            gpui::deferred(
                gpui::anchored()
                    .anchor(gpui::Anchor::TopLeft)
                    .position(place)
                    .snap_to_window()
                    .child(row),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}

/// T3's `resolveSelectionActionPosition` then its layout clamp: the button's
/// top-left is the pointer where the pointer is on the selection (within
/// `SELECTION_CITE_NEAR` of a line of it), else the last line's end,
/// `SELECTION_CITE_BELOW` under it; held in `column`, then
/// `SELECTION_CITE_EDGE` inside the window with the button's `size`.
fn cite_place(
    column: Bounds<Pixels>,
    rects: &[Bounds<Pixels>],
    pointer: Point<Pixels>,
    size: Size<Pixels>,
    window: Size<Pixels>,
) -> Point<Pixels> {
    let near = px(theme::SELECTION_CITE_NEAR);
    let on_text = rects.iter().any(|rect| {
        pointer.x >= rect.left() - near
            && pointer.x <= rect.right() + near
            && pointer.y >= rect.top() - near
            && pointer.y <= rect.bottom() + near
    });
    let preferred = match rects.last() {
        Some(last) if !on_text => {
            gpui::point(last.right(), last.bottom() + px(theme::SELECTION_CITE_BELOW))
        }
        _ => pointer,
    };
    let edge = px(theme::SELECTION_CITE_EDGE);
    let held = |value: Pixels, low: Pixels, high: Pixels, limit: Pixels, extent: Pixels| {
        let value = value.max(low).min(high).min(limit - edge);
        value.min(limit - extent - edge).max(edge)
    };
    gpui::point(
        held(preferred.x, column.left(), column.right(), window.width, size.width),
        held(preferred.y, column.top(), column.bottom(), window.height, size.height),
    )
}

/// The buttons' size together: each `SELECTION_CITE_H` tall, its words
/// shaped at `FS_BADGE`, the icon of the first, each side padded
/// `SELECTION_CITE_PAD_X` inside a 1px ring.
fn cite_size(too_long: bool, side_chat: bool, window: &mut Window) -> Size<Pixels> {
    let mut face = window.text_style();
    face.font_family = theme::FONT_UI.into();
    face.font_weight = theme::W_LABEL;
    let shaped = |text: &str| {
        window
            .text_system()
            .shape_line(
                text.to_owned().into(),
                px(theme::FS_BADGE),
                &[face.to_run(text.len())],
                None,
            )
            .width()
    };
    let pad = px(2. * theme::SELECTION_CITE_PAD_X + 2.);
    let first = if too_long { "Shorten selection" } else { "Cite" };
    let mut width = shaped(first) + pad;
    if !too_long {
        width += px(theme::SELECTION_CITE_ICON + theme::SELECTION_CITE_ICON_GAP);
    }
    if side_chat {
        width += px(theme::SELECTION_CITE_GAP) + shaped("Start a side chat") + pad;
    }
    gpui::size(width.ceil(), px(theme::SELECTION_CITE_H))
}

/// T3's `Button size="xs" variant="glass"`: `SELECTION_CITE_H` tall, 12px
/// medium words, fully rounded inside a 1px `FLOAT_EDGE` ring, a small
/// shadow, on `paint::FLOAT` (GPUI has no backdrop blur to give the glass).
/// `quote`: the Quote icon leads it. Lit `FLOAT_HOVER` under the pointer.
fn glass_button(
    namespace: &str,
    selector: &'static str,
    label: &'static str,
    quote: bool,
) -> gpui::Stateful<gpui::Div> {
    let id = gpui::ElementId::from(SharedString::from(format!("{selector}-{namespace}")));
    let key = ferrite_ui::pointer::hover_key(&id);
    let t = motion::hover_t(&key);
    if t > 0. && t < 1. {
        crate::file_links::mark_fading();
    }
    gpui::div()
        .id(id)
        .debug_selector(move || selector.into())
        .flex_none()
        .flex()
        .items_center()
        .gap(px(theme::SELECTION_CITE_ICON_GAP))
        .h(px(theme::SELECTION_CITE_H))
        .px(px(theme::SELECTION_CITE_PAD_X))
        .rounded_full()
        .border_1()
        .border_color(theme::paint::FLOAT_EDGE)
        .shadow(ferrite_ui::components::float_shadow())
        .bg(motion::mix(
            theme::paint::FLOAT.into(),
            theme::paint::FLOAT_HOVER.into(),
            t,
        ))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_BADGE))
        .font_weight(theme::W_LABEL)
        .text_color(gpui::rgb(theme::INK_STRONG))
        .whitespace_nowrap()
        .on_hover(motion::hover_listener(key))
        .when(quote, |button| {
            button.child(icons::icon(
                icons::QUOTE,
                theme::SELECTION_CITE_ICON,
                theme::INK_SUBTLE,
            ))
        })
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(gpui::point(px(x), px(y)), gpui::size(px(w), px(h)))
    }

    fn place(rects: &[Bounds<Pixels>], pointer: (f32, f32)) -> Point<Pixels> {
        cite_place(
            bounds(100., 40., 700., 500.),
            rects,
            gpui::point(px(pointer.0), px(pointer.1)),
            gpui::size(px(80.), px(24.)),
            gpui::size(px(1000.), px(700.)),
        )
    }

    #[test]
    fn the_button_stands_at_the_pointer_on_the_selection() {
        let at = place(&[bounds(200., 100., 60., 20.)], (250., 110.));
        assert_eq!((at.x, at.y), (px(250.), px(110.)));
    }

    #[test]
    fn a_pointer_off_the_selection_puts_it_under_the_last_line_end() {
        let at = place(
            &[bounds(200., 100., 60., 20.), bounds(200., 120., 40., 20.)],
            (600., 400.),
        );
        assert_eq!((at.x, at.y), (px(240.), px(144.)));
    }

    #[test]
    fn it_is_held_in_the_column_and_inside_the_window() {
        let rects = [bounds(200., 100., 60., 20.)];
        let left = place(&rects, (150., 110.));
        assert_eq!(left.x, px(150.), "inside the column, left of the text");
        let far = cite_place(
            bounds(0., 0., 1000., 700.),
            &rects,
            gpui::point(px(990.), px(695.)),
            gpui::size(px(80.), px(24.)),
            gpui::size(px(1000.), px(700.)),
        );
        assert_eq!((far.x, far.y), (px(912.), px(668.)));
        let corner = cite_place(
            bounds(0., 0., 1000., 700.),
            &rects,
            gpui::point(px(0.), px(0.)),
            gpui::size(px(80.), px(24.)),
            gpui::size(px(1000.), px(700.)),
        );
        assert_eq!((corner.x, corner.y), (px(8.), px(8.)));
    }
}
