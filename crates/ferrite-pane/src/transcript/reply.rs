//! Reply to a selection, as Claude does (its `SelectionTooltip`): a drag
//! that selects a passage of the transcript (anyone's words, the
//! operator's or the agent's) ends with a small dark Reply pill centred
//! over the selection, `SELECTION_REPLY_GAP` above it. A press on it adds
//! the passage to the Composer as a Quote row (`TranscriptEvent::Quote`,
//! `crate::quote`) and clears the selection. It goes when the selection
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

    /// The pill, while its selection stands: centred over it, fading once
    /// a scroll has begun.
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
        let at = gpui::point(reply.at.x, reply.at.y - px(theme::SELECTION_REPLY_GAP));
        let passage = reply.passage.clone();
        let fading = reply.fading.is_some();
        let weak = cx.entity().downgrade();
        let away = weak.clone();
        let id = gpui::ElementId::from(SharedString::from(format!(
            "selection-reply-{}",
            self.input.namespace
        )));
        let key = ferrite_ui::pointer::hover_key(&id);
        let t = motion::hover_t(&key);
        if t > 0. && t < 1. {
            crate::file_links::mark_fading();
        }
        let pill = reply_pill(id, key, t)
            .opacity(opacity)
            .on_mouse_down_out(move |_, _, cx| {
                let _ = away.update(cx, |view, cx| view.dismiss_reply(cx));
            })
            // On the press, not the click: the press clears the selection,
            // and with it this pill, before a release could land on it.
            .when(!fading, |pill| {
                pill.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    gpui::base::TextSelection::clear(window, cx);
                    let passage = passage.clone();
                    let _ = weak.update(cx, |view, cx| {
                        view.reply = None;
                        cx.emit(TranscriptEvent::Quote(passage));
                        cx.notify();
                    });
                })
            });
        // A zero-width column on the point: the pill hangs centred over
        // it whatever its label measures.
        Some(
            gpui::deferred(
                gpui::anchored()
                    .anchor(gpui::Anchor::BottomLeft)
                    .position(at)
                    .child(gpui::div().w(px(0.)).flex().justify_center().child(pill)),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}

/// Claude's pill: "Reply" then its bent-back arrow, `FS_UI` at medium
/// weight in white on `SELECTION_REPLY_FILL` inside a `SELECTION_REPLY_EDGE`
/// hairline, at `SELECTION_REPLY_RADIUS`, lit as the pointer comes over it.
fn reply_pill(id: gpui::ElementId, key: SharedString, t: f32) -> gpui::Stateful<gpui::Div> {
    ferrite_ui::components::text_ui()
        .id(id)
        .debug_selector(|| "selection-reply".into())
        .flex_none()
        .flex()
        .items_center()
        .gap(px(theme::SPACE_1))
        .px(px(theme::SELECTION_REPLY_PAD_X))
        .py(px(theme::SELECTION_REPLY_PAD_Y))
        .rounded(px(theme::SELECTION_REPLY_RADIUS))
        .bg(motion::mix(
            theme::paint::SELECTION_REPLY_FILL.into(),
            theme::paint::SELECTION_REPLY_HOVER.into(),
            t,
        ))
        .border_1()
        .border_color(theme::paint::SELECTION_REPLY_EDGE)
        .shadow(ferrite_ui::components::float_shadow())
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(gpui::rgb(theme::SELECTION_REPLY_INK))
        .whitespace_nowrap()
        .cursor_pointer()
        .occlude()
        .on_hover(motion::hover_listener(key))
        .child("Reply")
        .child(ferrite_ui::icons::icon(
            ferrite_ui::icons::REPLY,
            theme::SELECTION_REPLY_GLYPH,
            theme::SELECTION_REPLY_INK,
        ))
}
