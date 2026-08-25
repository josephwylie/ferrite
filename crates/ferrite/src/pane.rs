//! One Pane: the visible cell for one Thread. Header, transcript, status line,
//! Composer. Rendering only — the Blocks it draws are folded in core.

use std::time::Duration;

use ferrite_core::transcript::{
    Block, Body, Class, Diff, Input, Span, Status, Style, Token, ToolBlock, ToolState, Transcript,
};
use ferrite_core::SessionEvent;
use gpui::prelude::*;
use gpui::{
    actions, div, px, rgb, rgba, AnyElement, Context, Div, Entity, HighlightStyle, ScrollHandle,
    SharedString, StyledText, Window,
};

use crate::composer::Composer;
use crate::session::Session;

actions!(pane, [Submit, Interrupt]);

const BG_WINDOW: u32 = 0x050505;
const BG_PANE: u32 = 0x0e0e0e;
const BG_CODE: u32 = 0x141414;
const BORDER: u32 = 0x232323;
const HAIRLINE: u32 = 0x1a1a1a;
const TEXT_PRIMARY: u32 = 0xf3f4f7;
const TEXT_SECONDARY: u32 = 0xa7abb4;
const TEXT_MUTED: u32 = 0x7f8187;
const TEXT_THINKING: u32 = 0x5a5d63;
const TEXT_NOTICE: u32 = 0xd9a05b;
const TEXT_CODE: u32 = 0xc7ccd6;
const DIFF_ADDED: u32 = 0x7fb069;
const DIFF_REMOVED: u32 = 0xcf6f6f;

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
            self.transcript.apply(Input::Event(event));
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
                self.transcript.apply(Input::Prompt(text));
                if let Err(e) = sent {
                    self.transcript
                        .apply(Input::Notice(format!("send failed: {e}")));
                }
            }
            None => {
                self.transcript.apply(Input::Prompt(text));
                self.transcript.apply(Input::Notice("no session".into()));
            }
        }
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn interrupt(&mut self, _: &Interrupt, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = &mut self.session {
            if let Err(e) = session.interrupt() {
                self.transcript
                    .apply(Input::Notice(format!("interrupt failed: {e}")));
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
            .gap(px(5.))
            .px(px(8.))
            .py(px(6.))
            .text_size(px(12.));

        if let Some(error) = &self.spawn_error {
            return body.justify_center().items_center().child(
                div()
                    .max_w(px(520.))
                    .text_color(rgb(TEXT_NOTICE))
                    .child(error.clone()),
            );
        }

        for block in self.transcript.blocks() {
            body = body.child(render_block(block));
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

/// One Block, drawn at terminal density: no card chrome anywhere it can be
/// avoided, one hairline where structure genuinely needs a boundary.
pub fn render_block(block: &Block) -> AnyElement {
    let row = div().w_full().flex_shrink_0();
    match &block.body {
        Body::Prompt(line) => row
            .text_color(rgb(TEXT_PRIMARY))
            .child(SharedString::from(format!("❯ {line}")))
            .into_any_element(),
        Body::Paragraph { spans } => row
            .text_color(rgb(TEXT_SECONDARY))
            .child(inline(spans))
            .into_any_element(),
        Body::Heading { level, spans } => row
            .text_color(rgb(TEXT_PRIMARY))
            .text_size(px(if *level <= 2 { 13. } else { 12. }))
            .child(inline(spans))
            .into_any_element(),
        Body::Bullet { spans } => row
            .flex()
            .flex_row()
            .gap(px(6.))
            .text_color(rgb(TEXT_SECONDARY))
            .child(div().flex_shrink_0().text_color(rgb(TEXT_MUTED)).child("•"))
            .child(div().min_w_0().child(inline(spans)))
            .into_any_element(),
        Body::Thinking(thought) => row
            .text_color(rgb(TEXT_THINKING))
            .child(SharedString::from(thought.clone()))
            .into_any_element(),
        Body::Notice(text) => row
            .text_color(rgb(TEXT_NOTICE))
            .child(SharedString::from(text.clone()))
            .into_any_element(),
        Body::Meta(text) => row
            .text_size(px(11.))
            .text_color(rgb(TEXT_MUTED))
            .child(SharedString::from(text.clone()))
            .into_any_element(),
        Body::Code {
            language,
            source,
            tokens,
        } => row
            .flex()
            .flex_col()
            .bg(rgb(BG_CODE))
            .border_l_2()
            .border_color(rgb(BORDER))
            .px(px(6.))
            .py(px(3.))
            .children(language.as_ref().map(|language| {
                div()
                    .text_size(px(10.))
                    .text_color(rgb(TEXT_MUTED))
                    .child(SharedString::from(language.clone()))
            }))
            .child(code(source, tokens.as_deref()))
            .into_any_element(),
        Body::Tool(tool) => render_tool(row, tool),
    }
}

fn render_tool(row: Div, tool: &ToolBlock) -> AnyElement {
    let (marker, marker_color) = match &tool.state {
        ToolState::Running => ("◦", TEXT_MUTED),
        ToolState::Ok => ("•", TEXT_MUTED),
        ToolState::Failed(_) => ("×", TEXT_NOTICE),
    };
    let mut card = row.flex().flex_col().gap(px(2.)).child(
        div()
            .flex()
            .flex_row()
            .gap(px(6.))
            .text_size(px(11.))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(rgb(marker_color))
                    .child(marker),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(rgb(TEXT_SECONDARY))
                    .child(SharedString::from(tool.name.clone())),
            )
            .child(
                div()
                    .min_w_0()
                    .text_color(rgb(TEXT_MUTED))
                    .child(SharedString::from(tool.summary.clone())),
            ),
    );
    if let ToolState::Failed(message) = &tool.state {
        card = card.child(
            div()
                .pl(px(14.))
                .text_size(px(11.))
                .text_color(rgb(TEXT_NOTICE))
                .child(SharedString::from(message.clone())),
        );
    }
    if let Some(diff) = &tool.diff {
        card = card.child(render_diff(diff));
    }
    card.into_any_element()
}

/// A diff card: the path, what it cost in lines, and the change itself.
fn render_diff(diff: &Diff) -> impl IntoElement {
    let mut lines = div().flex().flex_col().px(px(6.)).py(px(2.));
    for line in diff.hunks.iter().flat_map(|hunk| hunk.lines.iter()) {
        let color = match line.chars().next() {
            Some('+') => DIFF_ADDED,
            Some('-') => DIFF_REMOVED,
            _ => TEXT_MUTED,
        };
        lines = lines.child(
            div()
                .w_full()
                .text_color(rgb(color))
                .child(SharedString::from(line.clone())),
        );
    }

    div()
        .flex()
        .flex_col()
        .ml(px(14.))
        .bg(rgb(BG_CODE))
        .border_1()
        .border_color(rgb(BORDER))
        .rounded_sm()
        .overflow_hidden()
        .text_size(px(11.))
        .child(
            div()
                .flex()
                .justify_between()
                .gap(px(8.))
                .px(px(6.))
                .py(px(2.))
                .border_b_1()
                .border_color(rgb(HAIRLINE))
                .child(
                    div()
                        .min_w_0()
                        .text_color(rgb(TEXT_SECONDARY))
                        .child(SharedString::from(diff.path.clone())),
                )
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .gap(px(6.))
                        .child(
                            div()
                                .text_color(rgb(DIFF_ADDED))
                                .child(SharedString::from(format!("+{}", diff.added))),
                        )
                        .child(
                            div()
                                .text_color(rgb(DIFF_REMOVED))
                                .child(SharedString::from(format!("−{}", diff.removed))),
                        ),
                ),
        )
        .child(lines)
}

/// Markdown spans in one wrapping run, so inline code keeps its place in the
/// sentence instead of becoming its own box.
fn inline(spans: &[Span]) -> StyledText {
    let mut text = String::new();
    let mut highlights = Vec::new();
    for span in spans {
        let start = text.len();
        text.push_str(&span.text);
        if span.style == Style::Code {
            highlights.push((
                start..text.len(),
                HighlightStyle {
                    color: Some(rgb(TEXT_CODE).into()),
                    background_color: Some(rgba(0x2323234d).into()),
                    ..Default::default()
                },
            ));
        }
    }
    StyledText::new(text).with_highlights(highlights)
}

/// Highlighted code, or plain code while the highlighter is still thinking.
fn code(source: &str, tokens: Option<&[Token]>) -> StyledText {
    let plain = || StyledText::new(SharedString::from(source.to_string()));
    let Some(tokens) = tokens else {
        return plain();
    };
    let mut highlights = Vec::new();
    let mut at = 0;
    for token in tokens {
        let end = at + token.text.len();
        // A highlighter that disagrees with the source is ignored, not trusted
        // into a panic.
        if end > source.len() {
            return plain();
        }
        let color = match token.class {
            Class::Plain => TEXT_CODE,
            Class::Keyword => 0x8fa8f0,
            Class::Str => 0x9ec78a,
            Class::Comment => TEXT_THINKING,
            Class::Number => 0xd0a26a,
        };
        highlights.push((
            at..end,
            HighlightStyle {
                color: Some(rgb(color).into()),
                ..Default::default()
            },
        ));
        at = end;
    }
    plain().with_highlights(highlights)
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

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::{Hunk, ToolResult, TurnOutcome};
    use gpui::{point, size, TestAppContext};

    /// A transcript holding one of every Block kind the Pane can draw.
    fn every_kind() -> Transcript {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("run the tests".into()));
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "weighing it up".into(),
        }));
        transcript.apply(Input::Event(SessionEvent::TextDelta {
            text: "## Plan\nI will run `cargo test` first.\n- one\n- two\n\n```rust\nfn main() {}\n```\ndone.\n\n"
                .into(),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "toolu_1".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo test" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "toolu_1".into(),
            output: "42 passed".into(),
            is_error: false,
            result: ToolResult::Opaque,
        }));
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "toolu_2".into(),
            name: "Edit".into(),
            input: serde_json::json!({ "file_path": "/workspace/x.txt" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "toolu_2".into(),
            output: "applied".into(),
            is_error: false,
            result: ToolResult::FileEdit {
                path: "/workspace/x.txt".into(),
                hunks: vec![Hunk {
                    old_start: 1,
                    old_lines: 3,
                    new_start: 1,
                    new_lines: 3,
                    lines: vec![" alpha".into(), "-bravo".into(), "+delta".into()],
                }],
            },
        }));
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "toolu_3".into(),
            name: "Read".into(),
            input: serde_json::json!({ "file_path": "/workspace/missing" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "toolu_3".into(),
            output: "No such file or directory".into(),
            is_error: true,
            result: ToolResult::Opaque,
        }));
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(0.038),
        }));
        transcript.apply(Input::Notice("send failed: broken pipe".into()));
        transcript
    }

    struct Blank;

    impl Render for Blank {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    /// The app is thin by design, so its one test is that every Block kind
    /// the core can produce actually lays out and paints in a window.
    #[gpui::test]
    fn every_block_kind_paints(cx: &mut TestAppContext) {
        let transcript = every_kind();
        let blocks: Vec<Block> = transcript.blocks().to_vec();

        let kinds: Vec<&str> = blocks
            .iter()
            .map(|block| match &block.body {
                Body::Prompt(_) => "prompt",
                Body::Paragraph { .. } => "paragraph",
                Body::Heading { .. } => "heading",
                Body::Bullet { .. } => "bullet",
                Body::Code { .. } => "code",
                Body::Tool(tool) => match (&tool.state, &tool.diff) {
                    (_, Some(_)) => "diff",
                    (ToolState::Failed(_), _) => "tool-failed",
                    _ => "tool",
                },
                Body::Thinking(_) => "thinking",
                Body::Notice(_) => "notice",
                Body::Meta(_) => "meta",
            })
            .collect();
        for wanted in [
            "prompt",
            "paragraph",
            "heading",
            "bullet",
            "code",
            "tool",
            "diff",
            "tool-failed",
            "thinking",
            "notice",
            "meta",
        ] {
            assert!(kinds.contains(&wanted), "no {wanted} block in {kinds:?}");
        }

        let (_, cx) = cx.add_window_view(|_, _| Blank);
        cx.draw(
            point(px(0.), px(0.)),
            size(px(900.), px(600.)),
            |_window, _cx| {
                div()
                    .flex()
                    .flex_col()
                    .w(px(900.))
                    .font_family("Menlo")
                    .text_size(px(12.))
                    .children(blocks.iter().map(render_block))
            },
        );
    }
}
