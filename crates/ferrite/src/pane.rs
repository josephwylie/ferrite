//! One Pane: the visible cell for one Thread. Header, transcript, status line,
//! Composer. This is the window half; what it shows lives in `Transcript`.

use std::time::Duration;

use ferrite_core::SessionEvent;
use gpui::prelude::*;
use gpui::{actions, div, px, rgb, Context, Entity, ScrollHandle, SharedString, Window};

use crate::composer::Composer;
use crate::session::Session;
use crate::transcript::{Kind, Status, Transcript};

actions!(pane, [Submit, Interrupt]);

const BG_WINDOW: u32 = 0x050505;
const BG_PANE: u32 = 0x0e0e0e;
const BORDER: u32 = 0x232323;
const HAIRLINE: u32 = 0x1a1a1a;
const TEXT_PRIMARY: u32 = 0xf3f4f7;
const TEXT_SECONDARY: u32 = 0xa7abb4;
const TEXT_MUTED: u32 = 0x7f8187;
const TEXT_THINKING: u32 = 0x5a5d63;
const TEXT_NOTICE: u32 = 0xd9a05b;

const PUMP_MS: u64 = 16;

pub struct Pane {
    session: Option<Session>,
    spawn_error: Option<SharedString>,
    composer: Entity<Composer>,
    title: SharedString,
    provider: SharedString,
    transcript: Transcript,
    scroll: ScrollHandle,
}

impl Pane {
    pub fn new(session: Result<Session, String>, cx: &mut Context<Self>) -> Self {
        let (session, spawn_error) = match session {
            Ok(session) => (Some(session), None),
            Err(message) => (None, Some(SharedString::from(message))),
        };

        if session.is_some() {
            cx.spawn(async move |this, cx| loop {
                cx.background_executor()
                    .timer(Duration::from_millis(PUMP_MS))
                    .await;
                let alive = this.update(cx, |pane, cx| pane.pump(cx));
                if alive.is_err() {
                    break;
                }
            })
            .detach();
        }

        Self {
            session,
            spawn_error,
            composer: cx.new(Composer::new),
            title: "thread-01".into(),
            provider: "claude".into(),
            transcript: Transcript::default(),
            scroll: ScrollHandle::new(),
        }
    }

    pub fn composer(&self) -> &Entity<Composer> {
        &self.composer
    }

    /// Drain whatever the Session produced since the last frame.
    fn pump(&mut self, cx: &mut Context<Self>) {
        let Some(session) = &self.session else {
            return;
        };
        let events: Vec<SessionEvent> = session.events().try_iter().collect();
        if events.is_empty() {
            return;
        }
        for event in events {
            self.transcript.apply(event);
        }
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn submit(&mut self, _: &Submit, _window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.update(cx, |composer, cx| composer.take(cx));
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        match &mut self.session {
            Some(session) => {
                let sent = session.send(&text);
                self.transcript.push_user(text);
                if let Err(e) = sent {
                    self.transcript.push_notice(format!("send failed: {e}"));
                }
            }
            None => {
                self.transcript.push_user(text);
                self.transcript.push_notice("no session".into());
            }
        }
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn interrupt(&mut self, _: &Interrupt, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = &mut self.session {
            if let Err(e) = session.interrupt() {
                self.transcript
                    .push_notice(format!("interrupt failed: {e}"));
            }
        }
        cx.notify();
    }

    fn header(&self) -> impl IntoElement {
        let subtitle = match (self.transcript.model(), self.transcript.session_id()) {
            (Some(model), Some(id)) => {
                let short: String = id.chars().take(8).collect();
                SharedString::from(format!("{model} · {short}"))
            }
            _ => SharedString::from("connecting…"),
        };
        div()
            .flex()
            .flex_shrink_0()
            .justify_between()
            .items_center()
            .px(px(8.))
            .py(px(5.))
            .border_b_1()
            .border_color(rgb(HAIRLINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(TEXT_PRIMARY))
                            .child(self.title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(TEXT_MUTED))
                            .child(self.provider.clone()),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_MUTED))
                    .child(subtitle),
            )
    }

    fn transcript(&self) -> impl IntoElement {
        let mut body = div()
            .id("transcript")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .gap(px(4.))
            .px(px(8.))
            .py(px(6.));

        if let Some(error) = &self.spawn_error {
            return body.justify_center().items_center().child(
                div()
                    .max_w(px(520.))
                    .text_size(px(12.))
                    .text_color(rgb(TEXT_NOTICE))
                    .child(error.clone()),
            );
        }

        for segment in self.transcript.segments() {
            let text = segment.text.trim_end();
            if text.is_empty() {
                continue;
            }
            let (color, body_text) = match segment.kind {
                Kind::User => (TEXT_PRIMARY, format!("❯ {text}")),
                Kind::Assistant => (TEXT_SECONDARY, text.to_string()),
                Kind::Thinking => (TEXT_THINKING, text.to_string()),
                // Placeholder until tool cards land (#4): name the tool, no more.
                Kind::Tool => (TEXT_MUTED, format!("• {text}")),
                Kind::Meta => (TEXT_MUTED, text.to_string()),
                Kind::Notice => (TEXT_NOTICE, text.to_string()),
            };
            body = body.child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(rgb(color))
                    .child(SharedString::from(body_text)),
            );
        }
        body
    }

    fn status_line(&self) -> impl IntoElement {
        let (label, color) = match self.transcript.status() {
            Status::Idle => ("idle", TEXT_MUTED),
            Status::Streaming => ("streaming…", TEXT_SECONDARY),
            Status::Blocked => ("decision needed", TEXT_NOTICE),
            Status::Closed => ("closed", TEXT_NOTICE),
        };
        let cost = self
            .transcript
            .last_cost()
            .map(|cost| SharedString::from(format!("${cost:.4}")))
            .unwrap_or_default();
        div()
            .flex()
            .flex_shrink_0()
            .justify_between()
            .items_center()
            .px(px(8.))
            .py(px(3.))
            .border_t_1()
            .border_color(rgb(HAIRLINE))
            .text_size(px(11.))
            .child(div().text_color(rgb(color)).child(label))
            .child(div().text_color(rgb(TEXT_MUTED)).child(cost))
    }

    fn composer_line(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .py(px(4.))
            .border_t_1()
            .border_color(rgb(HAIRLINE))
            .text_size(px(12.))
            .text_color(rgb(TEXT_PRIMARY))
            .child(div().text_color(rgb(TEXT_MUTED)).child("❯"))
            .child(self.composer.clone())
    }
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut pane = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(rgb(BG_PANE))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded_sm()
            .overflow_hidden()
            .child(self.header())
            .child(self.transcript())
            .child(self.status_line());

        if self.spawn_error.is_none() {
            pane = pane.child(self.composer_line());
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .p(px(8.))
            .bg(rgb(BG_WINDOW))
            .font_family("Menlo")
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::interrupt))
            .child(pane)
    }
}
