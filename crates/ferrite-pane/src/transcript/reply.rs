//! Reply to a selection, as Claude does (its `SelectionTooltip`): a drag
//! that selects a passage of the transcript (anyone's words, the
//! operator's or the agent's) ends with a small dark pill centred over the
//! selection, `SELECTION_REPLY_GAP` above it: `Start a side chat`, then
//! `Reply`. A side chat opens the right-docked pane about this passage
//! (`TranscriptEvent::SideChat`). Reply opens the Reply card, a float over
//! the selection (`ReplyCard`): the passage in two lines, a one-line field
//! (`Reply to Claude`) and a send button. Sending does not send a message:
//! it hands the cockpit the passage with the words typed
//! (`TranscriptEvent::Reply`), which becomes a Reply card in the Composer's
//! tray. Esc, or a press anywhere else, closes the card.
//!
//! A press clears the selection (the text selection's capture phase), so
//! the card keeps the passage's rectangles as the pill's press found them
//! and washes them itself while it is open. The pill goes when the
//! selection does (a press anywhere else), and a scroll fades it out over
//! `FADE_QUICK` (Claude's 150ms; Claude first carries it the scroll's first
//! 100px, which the list's offsets cannot report exactly mid-follow) and
//! closes the card at once.

use std::time::Instant;

use gpui::{
    prelude::*, px, Anchor, AnyElement, Bounds, Context, Entity, MouseButton, MouseDownEvent,
    MouseUpEvent, Pixels, Point, SharedString, Size, Window,
};

use super::{TranscriptEvent, TranscriptView};
use crate::composer::{Composer, Role};
use crate::quote::Reply;
use ferrite_ui::{motion, theme};

gpui::actions!(reply, [Send, Close]);

/// The card field's key context: its line's, where ⏎ sends the reply and
/// esc closes the card instead of sending the Composer's prompt or
/// interrupting.
const KEY_CONTEXT: &str = "ReplyCard";

/// The Reply card, open over a passage: the passage, where the selection's
/// first line stood when Reply was pressed (the pill's own place, the
/// card's fallback) and its rectangles, the field the reply is typed in.
pub(super) struct ReplyCard {
    quote: String,
    at: Point<Pixels>,
    rects: Vec<Bounds<Pixels>>,
    field: Entity<Composer>,
}

/// The pill on offer: the passage it quotes, read when the drag ended (a
/// press clears the selection before a click could land), the selection's
/// top centre in window coordinates then, and when a scroll began to fade
/// it.
pub(super) struct SelectionReply {
    passage: String,
    at: Point<Pixels>,
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

    /// A press let go over the transcript: offer the Reply once the
    /// selection it finished has settled, if it selected a passage.
    pub(super) fn released(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let from = self.pressed_at.take().unwrap_or(event.position);
        if !self.input.focused || !self.input.quotes {
            return;
        }
        let at = self.selection_top(from, event.position);
        cx.defer_in(window, move |view, window, cx| {
            let selected = gpui::base::TextSelection::selected_text(window, cx);
            view.reply = crate::quote::passage(&selected).map(|passage| SelectionReply {
                passage,
                at,
                fading: None,
            });
            cx.notify();
        });
    }

    /// The top centre of the selection a drag from `from` to `to` made:
    /// over the run between them on one line, over the column when it
    /// runs across lines (the rows are the column's width).
    fn selection_top(&self, from: Point<Pixels>, to: Point<Pixels>) -> Point<Pixels> {
        let half = px(self.grid().line / 2.);
        let x = if (from.y - to.y).abs() < half {
            (from.x + to.x) / 2.
        } else {
            self.scroll.list_state().viewport_bounds().center().x
        };
        gpui::point(x, from.y.min(to.y) - half)
    }

    /// A scroll under the pill: it fades out, then goes (at once under
    /// reduced motion).
    pub(super) fn scrolled(&mut self, cx: &mut Context<Self>) {
        self.close_reply_card(cx);
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

    /// Reply was pressed: the pill gives way to the card over `passage`,
    /// whose selection stood at `rects`. Its field, to take the keyboard.
    fn open_reply_card(
        &mut self,
        passage: String,
        at: Point<Pixels>,
        rects: Vec<Bounds<Pixels>>,
        cx: &mut Context<Self>,
    ) -> Entity<Composer> {
        let field = cx.new(|cx| {
            let mut line = Composer::new(cx);
            line.set_role(Role::Field(Some(KEY_CONTEXT)), cx);
            line.set_visible_row_limit(theme::REPLY_CARD_FIELD_ROWS, cx);
            line
        });
        self.reply = None;
        self.reply_card = Some(ReplyCard {
            quote: passage,
            at,
            rects,
            field: field.clone(),
        });
        cx.notify();
        field
    }

    /// Esc, or a press elsewhere: the card goes, the words typed with it.
    pub(super) fn close_reply_card(&mut self, cx: &mut Context<Self>) {
        if self.reply_card.take().is_some() {
            cx.notify();
        }
    }

    /// ⏎ or the send button: the passage and the words typed about it go
    /// to the cockpit as a Reply, and the card closes. Nothing typed, nothing
    /// sent.
    pub(super) fn send_reply(&mut self, cx: &mut Context<Self>) {
        let Some(card) = self.reply_card.as_ref() else {
            return;
        };
        let words = card.field.read(cx).text().trim().to_string();
        if words.is_empty() {
            return;
        }
        let reply = Reply::new(card.quote.clone(), words);
        self.reply_card = None;
        cx.emit(TranscriptEvent::Reply(reply));
        cx.notify();
    }

    /// The card open over `passage`, its selection washed at `rects` (a
    /// reference scene's: no pointer drags one).
    #[cfg(feature = "visual-reference")]
    pub fn fixture_reply_card(
        &mut self,
        passage: &str,
        rects: Vec<Bounds<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let at = rects.first().map_or_else(Point::default, |rect| {
            gpui::point(rect.center().x, rect.top())
        });
        self.open_reply_card(passage.to_string(), at, rects, cx);
    }

    /// Whether the card's field holds the keyboard: the cockpit leaves it
    /// there rather than handing it to the Composer.
    pub fn reply_card_holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.reply_card.as_ref().is_some_and(|card| {
            card.field
                .read(cx)
                .focus_target(window, cx)
                .contains_focused(window, cx)
        })
    }

    /// Whether the Reply card is open — what the tests read.
    #[cfg(any(test, feature = "test-support"))]
    pub fn reply_card_open(&self) -> bool {
        self.reply_card.is_some()
    }

    /// Whether the Reply pill is on offer — what the tests read.
    #[cfg(any(test, feature = "test-support"))]
    pub fn reply_offered(&self) -> bool {
        self.reply.is_some()
    }

    /// Where a float `size` stands by the selection, as Claude's pill does:
    /// by the selection, never the pointer.
    fn reply_place(
        &self,
        fallback: Point<Pixels>,
        size: Size<Pixels>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> Placed {
        let rects = gpui::base::TextSelection::painted_bounds(window, cx);
        placed(
            self.scroll.list_state().viewport_bounds(),
            &rects,
            fallback,
            size,
        )
    }

    /// The pill, while its selection stands: by the selection (not the
    /// pointer, `reply_place`), fading once a scroll has begun.
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
        let opacity = match reply.fading {
            Some(since) => {
                window.request_animation_frame();
                1. - motion::FADE_QUICK.progress_at(now.saturating_duration_since(since))
            }
            None => 1.,
        };
        let side_chat_shown = self.input.rewind.is_some();
        let place = self
            .reply_place(reply.at, pill_size(side_chat_shown, window), window, cx)
            .at;
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
        // and with it this pill, before a release could land on it.
        let press_side_chat = {
            let weak = weak.clone();
            let passage = passage.clone();
            move |_: &MouseDownEvent, window: &mut Window, cx: &mut gpui::App| {
                cx.stop_propagation();
                gpui::base::TextSelection::clear(window, cx);
                let passage = passage.clone();
                let _ = weak.update(cx, |view, cx| {
                    view.reply = None;
                    cx.emit(TranscriptEvent::SideChat(passage));
                    cx.notify();
                });
            }
        };
        let side_chat = self.input.rewind.as_ref().map(|allowed| {
            let action = pill_action(&namespace, "selection-side-chat", "Start a side chat");
            match allowed {
                Err(why) => action
                    .opacity(0.4)
                    .tooltip(ferrite_ui::menu::tooltip(format!(
                        "Start a side chat \u{b7} {why}"
                    ))),
                Ok(()) => action.when(!fading, |action| {
                    action
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, press_side_chat)
                }),
            }
        });
        let at = reply.at;
        let open = weak.clone();
        let reply = pill_action(&namespace, "selection-reply-quote", "Reply")
            .cursor_pointer()
            .when(!fading, |action| {
                action.on_mouse_down(MouseButton::Left, {
                    let passage = passage.clone();
                    move |_: &MouseDownEvent, window: &mut Window, cx: &mut gpui::App| {
                        cx.stop_propagation();
                        // The press cleared the selection already; its
                        // rectangles are as it last painted them.
                        let rects = gpui::base::TextSelection::painted_bounds(window, cx);
                        let passage = passage.clone();
                        let field = open
                            .update(cx, |view, cx| view.open_reply_card(passage, at, rects, cx))
                            .ok();
                        if let Some(field) = field {
                            let focus = field.read(cx).focus_target(window, cx);
                            window.focus(&focus, cx);
                        }
                    }
                })
            });
        let pill = reply_pill(id)
            .opacity(opacity)
            .on_mouse_down_out(move |_, _, cx| {
                let _ = away.update(cx, |view, cx| view.dismiss_reply(cx));
            })
            .children(
                side_chat
                    .map(|side_chat| [side_chat, divider()])
                    .into_iter()
                    .flatten(),
            )
            .child(reply);
        Some(
            gpui::deferred(gpui::anchored().position(place).child(pill))
                .with_priority(3)
                .into_any_element(),
        )
    }
}

impl TranscriptView {
    /// The Reply card, while it is open: the selection's rectangles washed
    /// as the selection was, and the card by them (`reply_place`), its foot
    /// on the selection's top (`Anchor::BottomLeft`) or its top under
    /// the selection's last line.
    pub(super) fn reply_card_float(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let card = self.reply_card.as_ref()?;
        let max = gpui::size(px(theme::REPLY_CARD_W), px(card_max_h()));
        let column = self.scroll.list_state().viewport_bounds();
        let place = placed(column, &card.rects, card.at, max);
        let (anchor, position) = if place.above {
            (
                Anchor::BottomLeft,
                gpui::point(place.at.x, place.at.y + max.height),
            )
        } else {
            (Anchor::TopLeft, place.at)
        };
        let weak = cx.entity().downgrade();
        let away = weak.clone();
        let namespace = self.input.namespace.clone();
        let empty = card.field.read(cx).text().is_empty();
        let washes = card.rects.iter().map(|rect| {
            gpui::deferred(
                gpui::anchored().position(rect.origin).child(
                    gpui::div()
                        .w(rect.size.width)
                        .h(rect.size.height)
                        .bg(gpui::rgba(theme::TEXT_SELECTION_WASH)),
                ),
            )
            .with_priority(1)
            .into_any_element()
        });
        let field = gpui::div()
            .relative()
            .flex_1()
            .min_w_0()
            .font_family(theme::FONT_UI)
            .line_height(px(theme::LH_UI))
            .text_color(gpui::rgb(theme::INK_STRONG))
            .child(card.field.clone())
            .when(empty, |field| {
                field.child(
                    gpui::div()
                        .debug_selector(|| "reply-card-placeholder".into())
                        .absolute()
                        .top_0()
                        .left_0()
                        .text_color(gpui::rgb(theme::INK_SUBTLE))
                        .child(SharedString::from(format!(
                            "Reply to {}",
                            self.input.reply_to
                        ))),
                )
            });
        let send = gpui::div()
            .id(gpui::ElementId::from(SharedString::from(format!(
                "reply-card-send-{namespace}"
            ))))
            .debug_selector(|| "reply-card-send".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(theme::SEND_CIRCLE_SM))
            .rounded_full()
            .cursor_pointer()
            .hover(|style| style.bg(theme::paint::FLOAT_HOVER))
            .child(ferrite_ui::icons::icon(
                ferrite_ui::icons::REPLY,
                theme::ICON_BUTTON_GLYPH,
                theme::INK_SUBTLE,
            ))
            .tooltip(ferrite_ui::menu::tooltip("Send"))
            .on_click(move |_, _, cx| {
                let _ = weak.update(cx, |view, cx| view.send_reply(cx));
            });
        let body = ferrite_ui::components::text_ui()
            .id(gpui::ElementId::from(SharedString::from(format!(
                "reply-card-{namespace}"
            ))))
            .debug_selector(|| "reply-card".into())
            .w(px(theme::REPLY_CARD_W))
            .flex()
            .flex_col()
            .gap(px(theme::REPLY_CARD_GAP))
            .p(px(theme::REPLY_CARD_PAD))
            .rounded(px(theme::R_FLOAT))
            .bg(theme::paint::FLOAT)
            .border_1()
            .border_color(theme::paint::FLOAT_EDGE)
            .shadow(ferrite_ui::components::float_shadow())
            .occlude()
            .on_mouse_down_out(move |_, _, cx| {
                let _ = away.update(cx, |view, cx| view.close_reply_card(cx));
            })
            .child(
                gpui::div()
                    .debug_selector(|| "reply-card-quote".into())
                    .text_size(px(theme::FS_META))
                    .line_height(px(theme::LH_META))
                    .text_color(gpui::rgb(theme::INK_SUBTLE))
                    .line_clamp(theme::REPLY_CARD_QUOTE_LINES)
                    .text_ellipsis()
                    .child(SharedString::from(card.quote.clone())),
            )
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .gap(px(theme::REPLY_CARD_GAP))
                    .child(field)
                    .child(send),
            );
        Some(
            gpui::div()
                .children(washes)
                .child(
                    gpui::deferred(
                        gpui::anchored()
                            .anchor(anchor)
                            .position(position)
                            .child(body),
                    )
                    .with_priority(3),
                )
                .into_any_element(),
        )
    }
}

/// How tall the card stands to begin with: its padding, its quote at its
/// most lines, the gap, the field's row, its ring. The card's foot is hung
/// on the selection, so as the words wrap it grows upward from there (a
/// card below the selection grows down); only whether it fits above needs
/// this.
fn card_max_h() -> f32 {
    2. * theme::REPLY_CARD_PAD
        + theme::REPLY_CARD_QUOTE_LINES as f32 * theme::LH_META
        + theme::REPLY_CARD_GAP
        + theme::LH_UI
        + 2. * theme::HAIRLINE_W
}

/// Where a float stands by a selection, and which side of it.
struct Placed {
    /// The float's top-left, were it `size` tall.
    at: Point<Pixels>,
    above: bool,
}

/// Where a float `size` stands by the selection whose rectangles are
/// `rects`, inside `column`, as Claude's pill does: by the selection, never
/// the pointer — centred over its first line, `SELECTION_REPLY_GAP` above
/// it, or under its last line when the transcript has no room above; inside
/// the column either way, so it never covers the selected text. A selection
/// that reports no rectangles (a code block's) places it over where the
/// drag went (`fallback`, the top centre it swept).
fn placed(
    column: Bounds<Pixels>,
    rects: &[Bounds<Pixels>],
    fallback: Point<Pixels>,
    size: Size<Pixels>,
) -> Placed {
    let gap = px(theme::SELECTION_REPLY_GAP);
    let top = rects.iter().map(|rect| rect.top()).reduce(Pixels::min);
    let (centre, y, above) = match top {
        Some(top) => {
            let first = rects
                .iter()
                .filter(|rect| (rect.top() - top).abs() < px(1.))
                .fold(None, |line: Option<Bounds<Pixels>>, rect| {
                    Some(line.map_or(*rect, |line| line.union(rect)))
                })
                .expect("the top line's rectangles");
            let above = top - gap - size.height;
            if above >= column.top() {
                (first.center().x, above, true)
            } else {
                let bottom = rects
                    .iter()
                    .map(|rect| rect.bottom())
                    .fold(top, Pixels::max);
                (first.center().x, bottom + gap, false)
            }
        }
        None => (fallback.x, fallback.y - gap - size.height, true),
    };
    let x = (centre - size.width / 2.)
        .min(column.right() - size.width)
        .max(column.left());
    Placed {
        at: gpui::point(x, y),
        above,
    }
}

/// Claude's pill: its actions side by side, `FS_UI` at body weight in
/// `INK_STRONG`, a float on `paint::FLOAT` inside its `FLOAT_EDGE`
/// hairline under the float shadow, at `SELECTION_REPLY_RADIUS`.
fn reply_pill(id: gpui::ElementId) -> gpui::Stateful<gpui::Div> {
    ferrite_ui::components::text_ui()
        .id(id)
        .debug_selector(|| "selection-reply".into())
        .flex_none()
        .flex()
        .items_center()
        .rounded(px(theme::SELECTION_REPLY_RADIUS))
        .bg(theme::paint::FLOAT)
        .border_1()
        .border_color(theme::paint::FLOAT_EDGE)
        .shadow(ferrite_ui::components::float_shadow())
        .font_weight(theme::W_BODY)
        .text_color(gpui::rgb(theme::INK_STRONG))
        .whitespace_nowrap()
        .overflow_hidden()
        .occlude()
}

/// The pill's size: its words shaped at `FS_UI`, each action's padding,
/// the divider, its hairline ring.
fn pill_size(side_chat: bool, window: &mut Window) -> gpui::Size<Pixels> {
    let mut face = window.text_style();
    face.font_family = theme::FONT_UI.into();
    face.font_weight = theme::W_BODY;
    let shaped = |text: &'static str| {
        window
            .text_system()
            .shape_line(
                text.into(),
                px(theme::FS_UI),
                &[face.to_run(text.len())],
                None,
            )
            .width()
    };
    let pad = px(2. * theme::SELECTION_REPLY_PAD_X);
    let mut width = shaped("Reply") + pad + px(2.);
    if side_chat {
        width += shaped("Start a side chat") + pad + px(1.);
    }
    gpui::size(
        width.ceil(),
        px(theme::LH_UI + 2. * theme::SELECTION_REPLY_PAD_Y + 2.),
    )
}

/// The hairline between the pill's actions (Claude's `|`).
fn divider() -> gpui::Stateful<gpui::Div> {
    gpui::div()
        .id("selection-reply-divider")
        .flex_none()
        .w(px(1.))
        .h(px(theme::SELECTION_REPLY_DIVIDER_H))
        .bg(theme::paint::FLOAT_EDGE)
}

/// One of the pill's actions: its word, `SELECTION_REPLY_PAD_X` either
/// side, lit as the pointer comes over it (`FLOAT_HOVER`, over
/// `motion::hover_t`).
fn pill_action(
    namespace: &str,
    selector: &'static str,
    label: &'static str,
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
        .flex()
        .items_center()
        .px(px(theme::SELECTION_REPLY_PAD_X))
        .py(px(theme::SELECTION_REPLY_PAD_Y))
        .bg(motion::mix(
            theme::paint::FLOAT.into(),
            theme::paint::FLOAT_HOVER.into(),
            t,
        ))
        .on_hover(motion::hover_listener(key))
        .child(label)
}
