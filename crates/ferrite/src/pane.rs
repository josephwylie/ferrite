//! One Pane: the visible cell for one Thread. Header, transcript, status
//! line, Composer. Rendering only — everything it shows is folded in core,
//! and every key it answers to belongs to the cockpit above it.

use ferrite_core::docview::{Instruments, Level, Tests};
use ferrite_core::transcript::{
    Block, Body, Class, Diff, Span, Status, Style, Token, ToolBlock, ToolState, Transcript,
};
use ferrite_core::workspace::WorkspaceBinding;
use ferrite_core::{Decision, ThreadId};
use gpui::prelude::*;
use gpui::{
    div, px, rgb, rgba, AnyElement, Context, Div, Entity, FocusHandle, HighlightStyle,
    ScrollHandle, SharedString, StyledText,
};

use crate::composer::Composer;

pub const BG_WINDOW: u32 = 0x050505;
const BG_PANE: u32 = 0x0e0e0e;
const BG_CODE: u32 = 0x141414;
const BORDER: u32 = 0x232323;
const BORDER_FOCUSED: u32 = 0x3a3a3a;
const LED_RUNNING: u32 = 0x6fa8dc;
const HAIRLINE: u32 = 0x1a1a1a;
const TEXT_PRIMARY: u32 = 0xf3f4f7;
const TEXT_SECONDARY: u32 = 0xa7abb4;
pub const TEXT_MUTED: u32 = 0x7f8187;
const TEXT_THINKING: u32 = 0x5a5d63;
pub const TEXT_NOTICE: u32 = 0xd9a05b;
const TEXT_CODE: u32 = 0xc7ccd6;
const DIFF_ADDED: u32 = 0x7fb069;
const DIFF_REMOVED: u32 = 0xcf6f6f;
const CODE_KEYWORD: u32 = 0x8fa8f0;
const CODE_STRING: u32 = 0x9ec78a;
const CODE_NUMBER: u32 = 0xd0a26a;
const BG_DECISION: u32 = 0x171310;
/// The selection wash. The Composer paints the same value under its own
/// selection (crate::composer reads this const), so selected text reads the
/// same everywhere.
pub const BG_SELECTED: u32 = 0x3f6ea830;
/// One translucent step up whatever sits underneath: the inline-code wash,
/// and the hover shade for rows that have no background of their own.
const BG_HOVER: u32 = 0x2323234d;
/// Hover on the two cards that already paint solid backgrounds — one step up
/// each card's own colour, staying inside the pane's ramp.
const BG_CODE_HOVER: u32 = 0x191919;
const BG_DECISION_HOVER: u32 = 0x1e1813;

/// One Pane's view state: what the window owns per Thread. Everything it
/// shows lives in core; this is the keyboard and the scrollback position.
pub struct PaneView {
    pub thread: ThreadId,
    pub composer: Entity<Composer>,
    pub scroll: ScrollHandle,
    /// A pending Decision takes the keyboard: y and n are answers, not text.
    pub decision_focus: FocusHandle,
}

impl PaneView {
    pub fn new<T: 'static>(thread: ThreadId, cx: &mut Context<T>) -> Self {
        Self {
            thread,
            composer: cx.new(Composer::new),
            scroll: ScrollHandle::new(),
            decision_focus: cx.focus_handle(),
        }
    }
}

/// Everything one Pane draws, as the cockpit reads it for this frame.
pub struct PaneState<'a> {
    pub transcript: Option<&'a Transcript>,
    pub decision: Option<&'a Decision>,
    pub queued: Option<&'a str>,
    pub workspace: Option<&'a WorkspaceBinding>,
    pub focused: bool,
    pub blocked: bool,
    /// The Blocks a drag swept, as indices into the Thread's blocks. The
    /// cockpit owns the drag; the Pane only paints the wash.
    pub selected: Option<std::ops::RangeInclusive<usize>>,
}

/// One Pane. A Thread with no transcript is one the cockpit could not open;
/// it still gets a cell, because a Pane that vanishes hides the problem.
pub fn render_pane(view: &PaneView, state: PaneState<'_>, level: Level) -> impl IntoElement {
    let PaneState {
        transcript,
        decision,
        queued,
        workspace,
        focused,
        blocked,
        selected,
    } = state;
    // One border rule for every level, so focus reads the same at arm's length
    // as it does across the room.
    let border = if focused {
        BORDER_FOCUSED
    } else if blocked {
        TEXT_NOTICE
    } else {
        BORDER
    };
    let shell = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .bg(rgb(if blocked { BG_DECISION } else { BG_PANE }))
        .border_1()
        .border_color(rgb(border))
        .rounded_sm()
        .overflow_hidden();

    // Far enough away, a Pane is one signal: no header, no transcript,
    // nothing that stops reading at a glance.
    if level == Level::Wall {
        return shell.child(wall(view.thread, transcript, blocked));
    }

    let mut pane = shell.child(header(view.thread, transcript, workspace));
    // Mid: what the Thread is doing, above what it said.
    if level == Level::Instruments {
        if let Some(transcript) = transcript {
            pane = pane.child(instruments(transcript));
        }
    }

    match transcript {
        Some(transcript) => {
            pane = pane
                .child(body(view, transcript, level.visible_blocks(), selected))
                .child(status_line(transcript));
        }
        None => {
            pane = pane.child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT_MUTED))
                    .child("parked"),
            );
        }
    }

    // A held prompt is exactly what an operator glancing at instruments wants
    // to see, so it shows wherever there is a Pane to show it in.
    if let Some(held) = queued {
        pane = pane.child(queued_line(held));
    }
    if let Some(decision) = decision {
        pane = pane.child(
            decision_card(decision)
                .key_context("Decision")
                .track_focus(&view.decision_focus),
        );
    }
    // Only the near view has room to answer in.
    if transcript.is_some() && level == Level::Transcript {
        pane = pane.child(composer_line(view, focused));
    }
    pane
}

fn wall(thread: ThreadId, transcript: Option<&Transcript>, blocked: bool) -> Div {
    let (led, label) = match transcript.map(|t| t.status()) {
        None => (TEXT_MUTED, "parked"),
        Some(Status::Streaming) => (LED_RUNNING, "running"),
        Some(Status::Blocked) => (TEXT_NOTICE, "waiting"),
        Some(Status::Closed) => (DIFF_REMOVED, "closed"),
        Some(Status::Idle) => (TEXT_MUTED, "idle"),
    };
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .child(
            div()
                .w(px(14.))
                .h(px(14.))
                .rounded_full()
                .bg(rgb(led))
                .flex_shrink_0(),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(rgb(TEXT_SECONDARY))
                .child(SharedString::from(format!("{thread:02}"))),
        )
        .child(
            div()
                .text_size(px(10.))
                .text_color(rgb(if blocked { TEXT_NOTICE } else { TEXT_MUTED }))
                .child(SharedString::from(label)),
        )
}

/// L2: the Thread's work, in one row that never wraps.
fn instruments(transcript: &Transcript) -> impl IntoElement {
    let read = Instruments::of(transcript);
    let mut row = div()
        .flex()
        .flex_shrink_0()
        .gap(px(10.))
        .px(px(8.))
        .py(px(3.))
        .border_b_1()
        .border_color(rgb(HAIRLINE))
        .text_size(px(11.));

    if let Some(todos) = read.todos {
        row = row.child(
            div()
                .text_color(rgb(TEXT_SECONDARY))
                .child(SharedString::from(format!(
                    "{}/{} done",
                    todos.done, todos.total
                ))),
        );
    }
    row = row.child(match read.tests {
        Some(Tests::Passed) => div()
            .text_color(rgb(DIFF_ADDED))
            .child(SharedString::from("tests pass")),
        Some(Tests::Failed) => div()
            .text_color(rgb(DIFF_REMOVED))
            .child(SharedString::from("tests fail")),
        None => div()
            .text_color(rgb(TEXT_MUTED))
            .child(SharedString::from("no tests run")),
    });
    if read.added > 0 || read.removed > 0 {
        row = row.child(
            div()
                .flex()
                .gap(px(4.))
                .child(
                    div()
                        .text_color(rgb(DIFF_ADDED))
                        .child(SharedString::from(format!("+{}", read.added))),
                )
                .child(
                    div()
                        .text_color(rgb(DIFF_REMOVED))
                        .child(SharedString::from(format!("−{}", read.removed))),
                )
                .child(
                    div()
                        .text_color(rgb(TEXT_MUTED))
                        .child(SharedString::from(format!(
                            "{} file{}",
                            read.files,
                            if read.files == 1 { "" } else { "s" }
                        ))),
                ),
        );
    }
    row
}

/// Which checkout a Thread works in — a worktree's own name, or "main" for
/// the shared one. One line, because an operator running many Threads has to
/// know which of them can trample the others.
fn binding_label(workspace: Option<&WorkspaceBinding>) -> SharedString {
    match workspace {
        Some(WorkspaceBinding::Worktree { path, .. }) => SharedString::from(
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "worktree".into()),
        ),
        Some(WorkspaceBinding::Main { .. }) => SharedString::from("main"),
        None => SharedString::from(""),
    }
}

fn header(
    thread: ThreadId,
    transcript: Option<&Transcript>,
    workspace: Option<&WorkspaceBinding>,
) -> impl IntoElement {
    let subtitle = match transcript.and_then(|t| Some((t.model()?, t.session_id()?))) {
        Some((model, id)) => {
            let short: String = id.chars().take(8).collect();
            SharedString::from(format!("{model} · {short}"))
        }
        None => SharedString::from("connecting…"),
    };
    div()
        .flex()
        .flex_shrink_0()
        .justify_between()
        .items_center()
        .px(px(8.))
        .py(px(4.))
        .border_b_1()
        .border_color(rgb(HAIRLINE))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(TEXT_PRIMARY))
                        .child(SharedString::from(format!("thread-{thread:02}"))),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(TEXT_MUTED))
                        .child(binding_label(workspace)),
                ),
        )
        .child(
            div()
                .text_size(px(10.))
                .text_color(rgb(TEXT_MUTED))
                .child(subtitle),
        )
}

fn body(
    view: &PaneView,
    transcript: &Transcript,
    visible: usize,
    selected: Option<std::ops::RangeInclusive<usize>>,
) -> impl IntoElement {
    let mut body = div()
        .id(("transcript", view.thread.get() as usize))
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(&view.scroll)
        .gap(px(4.))
        .px(px(8.))
        .py(px(5.))
        .text_size(px(12.));
    let blocks = transcript.blocks();
    let tail = blocks.len().saturating_sub(visible);
    for (offset, block) in blocks[tail..].iter().enumerate() {
        let picked = selected
            .as_ref()
            .is_some_and(|range| range.contains(&(tail + offset)));
        body = body.child(render_block(block, picked));
    }
    body
}

fn status_line(transcript: &Transcript) -> impl IntoElement {
    let (label, color) = match transcript.status() {
        Status::Idle => ("idle", TEXT_MUTED),
        Status::Streaming => ("streaming…", TEXT_SECONDARY),
        Status::Blocked => ("decision needed", TEXT_NOTICE),
        Status::Closed => ("closed", TEXT_NOTICE),
    };
    let mut spend = Vec::new();
    if let Some(usage) = transcript.usage() {
        spend.push(match usage.context_window {
            Some(window) => format!("{}/{}", tokens(usage.total_tokens), tokens(window)),
            None => tokens(usage.total_tokens),
        });
    }
    if let Some(cost) = transcript.last_cost() {
        spend.push(format!("${cost:.4}"));
    }
    div()
        .flex()
        .flex_shrink_0()
        .justify_between()
        .items_center()
        .px(px(8.))
        .py(px(2.))
        .border_t_1()
        .border_color(rgb(HAIRLINE))
        .text_size(px(10.))
        .hover(|line| line.bg(rgba(BG_HOVER)))
        .child(div().text_color(rgb(color)).child(label))
        .child(
            div()
                .text_color(rgb(TEXT_MUTED))
                .child(SharedString::from(spend.join(" · "))),
        )
}

fn composer_line(view: &PaneView, focused: bool) -> impl IntoElement {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .px(px(8.))
        .py(px(3.))
        .border_t_1()
        .border_color(rgb(HAIRLINE))
        .text_size(px(12.))
        .text_color(rgb(TEXT_PRIMARY))
        .child(
            div()
                .text_color(rgb(if focused { TEXT_PRIMARY } else { TEXT_MUTED }))
                .child("❯"),
        )
        .child(view.composer.clone())
}

/// A prompt written while the agent was still working.
fn queued_line(held: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .px(px(8.))
        .py(px(2.))
        .text_size(px(10.))
        .child(div().flex_shrink_0().text_color(rgb(TEXT_MUTED)).child("⋯"))
        .child(
            div()
                .min_w_0()
                .text_color(rgb(TEXT_SECONDARY))
                .child(SharedString::from(held.to_string())),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(rgb(TEXT_MUTED))
                .child("queued"),
        )
}

/// One Block, drawn at terminal density: no card chrome anywhere it can be
/// avoided, one hairline where structure genuinely needs a boundary.
/// A selected Block carries the wash; whole Blocks are the selection unit
/// because gpui 0.2.2 has no character-level selection over rendered text.
fn render_block(block: &Block, selected: bool) -> AnyElement {
    let row = div()
        .w_full()
        .flex_shrink_0()
        .when(selected, |row| row.bg(rgba(BG_SELECTED)));
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
            // The row the pointer is on, findable in a dense transcript.
            .hover(|row| row.bg(rgba(BG_HOVER)))
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
        .hover(|card| card.bg(rgb(BG_CODE_HOVER)))
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

/// What a blocked Thread shows above its Composer. Kept free of focus and
/// key wiring so it can be drawn — and smoke-rendered — on its own.
fn decision_card(decision: &Decision) -> Div {
    let subject = if decision.tool_name.is_empty() {
        SharedString::from("unreadable permission request")
    } else if decision.description.is_empty() {
        SharedString::from(decision.tool_name.clone())
    } else {
        SharedString::from(format!("{} · {}", decision.tool_name, decision.description))
    };
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .px(px(8.))
        .py(px(4.))
        .gap(px(2.))
        .border_t_1()
        .border_color(rgb(TEXT_NOTICE))
        .bg(rgb(BG_DECISION))
        .hover(|card| card.bg(rgb(BG_DECISION_HOVER)))
        .child(
            div()
                .flex()
                .gap(px(6.))
                .text_size(px(12.))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(rgb(TEXT_NOTICE))
                        .child("◆"),
                )
                .child(div().min_w_0().text_color(rgb(TEXT_PRIMARY)).child(subject)),
        )
        .child(div().text_size(px(11.)).text_color(rgb(TEXT_MUTED)).child(
            if decision.standing_answer().is_some() {
                "y allow · n deny · a always"
            } else {
                "y allow · n deny"
            },
        ))
}

/// A Block's text as the clipboard should carry it: what the row shows,
/// without colour or state glyphs. Exhaustive on purpose — a new Body kind
/// must decide what copying it means.
pub fn block_text(block: &Block) -> String {
    fn flat(spans: &[Span]) -> String {
        spans.iter().map(|span| span.text.as_str()).collect()
    }
    match &block.body {
        Body::Prompt(line) => format!("❯ {line}"),
        Body::Paragraph { spans } | Body::Heading { spans, .. } => flat(spans),
        Body::Bullet { spans } => format!("• {}", flat(spans)),
        Body::Thinking(text) | Body::Notice(text) | Body::Meta(text) => text.clone(),
        Body::Code { source, .. } => source.clone(),
        Body::Tool(tool) => {
            let mut lines = vec![format!("{} {}", tool.name, tool.summary)];
            if let ToolState::Failed(message) = &tool.state {
                lines.push(message.clone());
            }
            if let Some(diff) = &tool.diff {
                lines.push(format!("{} +{} −{}", diff.path, diff.added, diff.removed));
                lines.extend(
                    diff.hunks
                        .iter()
                        .flat_map(|hunk| hunk.lines.iter().cloned()),
                );
            }
            lines.join("\n")
        }
    }
}

/// Token counts read at a glance, not to the digit.
fn tokens(count: u64) -> String {
    match count {
        0..=999 => count.to_string(),
        1_000..=999_999 => format!("{:.1}k", count as f64 / 1_000.0),
        _ => format!("{:.1}m", count as f64 / 1_000_000.0),
    }
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
                    background_color: Some(rgba(BG_HOVER).into()),
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
            Class::Keyword => CODE_KEYWORD,
            Class::Str => CODE_STRING,
            Class::Comment => TEXT_THINKING,
            Class::Number => CODE_NUMBER,
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

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::transcript::{Input, Lexer};
    use ferrite_core::{Hunk, SessionEvent, ToolResult, TurnOutcome};
    use gpui::{size, TestAppContext};
    use std::sync::Arc;

    /// A transcript holding one of every Block kind the Pane can draw.
    fn every_kind() -> Transcript {
        let (lexer, answers) = Lexer::new();
        let mut transcript = Transcript::new(Arc::new(lexer));
        transcript.apply(Input::Prompt("run the tests".into()));
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "weighing it up".into(),
        }));
        transcript.apply(Input::Event(SessionEvent::TextDelta {
            text: "## Plan\nI will run `cargo test` first.\n- one\n- two\n\n\
                   ```rust\nfn main() {}\n```\ndone.\n\n"
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
        transcript.apply(Input::Revived);
        for answer in answers.try_iter() {
            transcript.apply(answer);
        }
        transcript
    }

    /// Renders Blocks through a real view: hover styles look up the view
    /// they are painting under, which a bare `cx.draw` does not have.
    struct ShowsBlocks {
        blocks: Vec<Block>,
    }

    impl Render for ShowsBlocks {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut Context<Self>,
        ) -> impl IntoElement {
            div()
                .flex()
                .flex_col()
                .w(px(900.))
                .font_family(crate::MONO_FONT)
                .text_size(px(12.))
                .children(self.blocks.iter().map(|block| render_block(block, false)))
                // And once more selected, so the wash paints on every kind.
                .children(self.blocks.iter().map(|block| render_block(block, true)))
        }
    }

    struct ShowsDecisions {
        decisions: Vec<Decision>,
    }

    impl Render for ShowsDecisions {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut Context<Self>,
        ) -> impl IntoElement {
            div()
                .flex()
                .flex_col()
                .w(px(900.))
                .font_family(crate::MONO_FONT)
                .text_size(px(12.))
                .children(self.decisions.iter().map(decision_card))
        }
    }

    /// An operator running many Threads has to know which of them share the
    /// checkout and which cannot trample it.
    #[test]
    fn the_chrome_names_the_workspace_a_thread_works_in() {
        assert_eq!(
            binding_label(Some(&WorkspaceBinding::Worktree {
                repo: "/repo".into(),
                path: "/repo/../ferrite-thread-3".into(),
            })),
            "ferrite-thread-3"
        );
        assert_eq!(
            binding_label(Some(&WorkspaceBinding::Main {
                checkout: "/repo".into()
            })),
            "main"
        );
        // A Thread from before bindings existed claims nothing.
        assert_eq!(binding_label(None), "");
    }

    /// The app is thin by design, so its render test is that every Block kind
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

        let (_, cx) = cx.add_window_view(|_, _| ShowsBlocks { blocks });
        // A resize forces a real layout-and-paint pass through the view.
        cx.simulate_resize(size(px(900.), px(600.)));
        cx.run_until_parked();
    }

    /// AC2's copy half needs every Block kind to say what it is as text —
    /// an empty string here would copy as a silent hole.
    #[test]
    fn every_block_kind_has_clipboard_text() {
        let transcript = every_kind();
        for block in transcript.blocks() {
            assert!(
                !block_text(block).trim().is_empty(),
                "no clipboard text for {block:?}"
            );
        }
        let by_kind: Vec<String> = transcript.blocks().iter().map(block_text).collect();
        let all = by_kind.join("\n");
        assert!(all.contains("❯ run the tests"), "the prompt line: {all}");
        assert!(
            all.contains("fn main() {}"),
            "code copies its source: {all}"
        );
        assert!(
            all.contains("+delta") && all.contains("-bravo"),
            "a diff copies its lines: {all}"
        );
    }

    #[gpui::test]
    fn a_blocked_thread_paints_its_decision_card(cx: &mut TestAppContext) {
        let event = crate::session::script()
            .into_iter()
            .map(|step| step.event)
            .find(|event| matches!(event, SessionEvent::DecisionRequested { .. }))
            .expect("the demo stops on a Decision");
        let SessionEvent::DecisionRequested { decision } = event else {
            unreachable!()
        };
        assert_eq!(decision.tool_name, "Write");

        // A request Ferrite could not read is still a card, or the operator
        // has nothing to deny and the turn hangs.
        let unreadable = Decision {
            tool_name: String::new(),
            description: String::new(),
            ..decision.clone()
        };

        let (_, cx) = cx.add_window_view(|_, _| ShowsDecisions {
            decisions: vec![decision, unreadable],
        });
        cx.simulate_resize(size(px(900.), px(300.)));
        cx.run_until_parked();
    }
}
