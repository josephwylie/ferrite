//! Reply to a selection, as Claude does (its `SelectionTooltip`): a drag
//! that selects a passage of the transcript (anyone's words, the
//! operator's or the agent's) ends with a small dark pill centred over the
//! selection, `SELECTION_REPLY_GAP` above it: `Start a side chat`, then
//! `Reply`. Reply adds the passage to the Composer as a Quote row
//! (`TranscriptEvent::Quote`, `crate::quote`); a side chat is a new Thread
//! forked from this point with the passage quoted in its Composer
//! (`TranscriptEvent::SideChat`) — Claude's is a floating side panel
//! sharing the session, which Ferrite has no seat for (see the ledger).
//! Either clears the selection. The side chat is offered where the agent
//! can fork, faded with why while it cannot now. The pill goes when the selection
//! does (a press anywhere else), and a scroll fades it out over
//! `FADE_QUICK` (Claude's 150ms; Claude first carries it the scroll's first
//! 100px, which the list's offsets cannot report exactly mid-follow).

use std::time::Instant;

use gpui::{
    prelude::*, px, AnyElement, Context, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point,
    SharedString, Window,
};

use super::{TranscriptEvent, TranscriptView};
use ferrite_ui::{motion, theme};

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

    /// Whether the Reply pill is on offer — what the tests read.
    #[cfg(any(test, feature = "test-support"))]
    pub fn reply_offered(&self) -> bool {
        self.reply.is_some()
    }

    /// Where the pill's top-left stands, as Claude's does: by the selection,
    /// never the pointer — centred over its first line, `SELECTION_REPLY_GAP`
    /// above it, or under its last line when the transcript has no room
    /// above; inside the transcript's column either way, so it never covers
    /// the selected text. A selection whose text reports no rectangles (a
    /// code block's) places it over where the drag went (`fallback`, the
    /// top centre it swept).
    fn reply_place(
        &self,
        fallback: Point<Pixels>,
        side_chat: bool,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> Point<Pixels> {
        let column = self.scroll.list_state().viewport_bounds();
        let size = pill_size(side_chat, window);
        let gap = px(theme::SELECTION_REPLY_GAP);
        let rects = gpui::base::TextSelection::painted_bounds(window, cx);
        let top = rects.iter().map(|rect| rect.top()).reduce(Pixels::min);
        let (centre, y) = match top {
            Some(top) => {
                let first = rects
                    .iter()
                    .filter(|rect| (rect.top() - top).abs() < px(1.))
                    .fold(None, |line: Option<gpui::Bounds<Pixels>>, rect| {
                        Some(line.map_or(*rect, |line| line.union(rect)))
                    })
                    .expect("the top line's rectangles");
                let above = top - gap - size.height;
                let y = if above >= column.top() {
                    above
                } else {
                    let bottom = rects
                        .iter()
                        .map(|rect| rect.bottom())
                        .fold(top, Pixels::max);
                    bottom + gap
                };
                (first.center().x, y)
            }
            None => (fallback.x, fallback.y - gap - size.height),
        };
        let x = (centre - size.width / 2.)
            .min(column.right() - size.width)
            .max(column.left());
        gpui::point(x, y)
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
        let place = self.reply_place(reply.at, side_chat_shown, window, cx);
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
        let press = |event: fn(String) -> TranscriptEvent| {
            let weak = weak.clone();
            let passage = passage.clone();
            move |_: &MouseDownEvent, window: &mut Window, cx: &mut gpui::App| {
                cx.stop_propagation();
                gpui::base::TextSelection::clear(window, cx);
                let passage = passage.clone();
                let _ = weak.update(cx, |view, cx| {
                    view.reply = None;
                    cx.emit(event(passage));
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
                        .on_mouse_down(MouseButton::Left, press(TranscriptEvent::SideChat))
                }),
            }
        });
        let reply = pill_action(&namespace, "selection-reply-quote", "Reply")
            .cursor_pointer()
            .when(!fading, |action| {
                action.on_mouse_down(MouseButton::Left, press(TranscriptEvent::Quote))
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
