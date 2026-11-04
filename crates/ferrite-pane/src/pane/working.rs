//! The working line under a running turn, and the task list over it.

use super::*;
use crate::transcript::LineDisclosure;

/// What a Thread's working row reads besides its transcript (`working_tail`),
/// gathered by the Cockpit when it syncs the transcript.
pub struct WorkingFacts<'a> {
    pub transcript: &'a Transcript,
    pub thread: ThreadId,
    /// The Session is starting, nothing streaming yet.
    pub starting: bool,
    pub solo: bool,
    /// The Pane holds the keyboard: only its line animates.
    pub focused: bool,
    pub reduce_motion: bool,
    /// The live reasoning caption's own row is in view.
    pub reasoning_visible: bool,
    /// The Thread's effort in force (`medium`), for the line's `thinking
    /// with medium effort` while the model thinks.
    pub effort: Option<SharedString>,
}

/// The working row's shape: the live line, or the starting shape.
#[derive(Clone, Debug, Hash, PartialEq)]
enum Working {
    Line(WorkingLine),
    Starting { still: bool, solo: bool },
    /// No turn runs, but the last one's plan stays under its rows, where
    /// the turn left it (a step it stopped on reads blocked).
    Plan {
        plan: Vec<(String, TaskState)>,
        solo: bool,
    },
}

/// WP-A · the working row (the prototype's `.comp > .r`): the transcript's
/// last row while a turn runs, so it reads as the next line of the
/// conversation under its newest row. While the transcript streams, the
/// line; while a Session starts with nothing streaming yet, its starting
/// shape; otherwise nothing. Only the Pane holding the keyboard (`focused`)
/// animates it. It names no key: the stop button and the placeholder say
/// how to stop. Its key changes with its words and whole seconds, never per
/// frame; the mark and the shimmer are loops (the loops overlay), so they
/// redraw nothing of the transcript.
pub fn working_tail(facts: WorkingFacts) -> Option<crate::transcript::WorkingTail> {
    // The line stays over a Decision that waits: the turn goes on being a
    // turn, its clock counting, while the operator decides. Alone in its
    // window only: a Group Pane's Decision is an island over a small body,
    // which keeps all its room.
    let working = if facts.transcript.status() == Status::Streaming
        || (facts.transcript.status() == Status::Blocked && facts.solo)
    {
        Working::Line(WorkingLine::of(
            facts.transcript,
            false,
            facts.solo,
            facts.focused,
            facts.reasoning_visible,
            facts.reduce_motion,
            facts.thread.get(),
            facts.effort.as_deref(),
        ))
    } else if facts.starting {
        Working::Starting {
            still: facts.reduce_motion || !facts.focused,
            solo: facts.solo,
        }
    } else {
        let plan = plan_of(facts.transcript);
        if plan.is_empty() {
            return None;
        }
        Working::Plan {
            plan,
            solo: facts.solo,
        }
    };
    let key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        working.hash(&mut hasher);
        hasher.finish()
    };
    Some(crate::transcript::WorkingTail {
        key,
        render: std::rc::Rc::new(move |disclosure, _, _| {
            let line = match &working {
                Working::Line(line) => line.render(disclosure),
                Working::Starting { still, solo } => starting_line(*still, *solo),
                Working::Plan { plan, solo } => div()
                    .pt(px(theme::TASK_CARD_GAP))
                    .child(task_list(plan, Grid::pane(Default::default(), *solo))),
            };
            div()
                .debug_selector(|| "transcript-progress".into())
                .w_full()
                .min_w_0()
                .flex_shrink_0()
                .child(line)
                .into_any_element()
        }),
    })
}

/// The working line's shape while a Session starts: nothing the provider
/// says yet, so Ferrite's own mark holds the gutter — the working mark on
/// the focused Pane, assembled and still elsewhere and under reduced motion
/// — and `Starting` in `INK_SUBTLE`, on the working line's own line (Solo's
/// or a Group Pane's), so the line never shifts when the turn's words
/// replace it.
pub(super) fn starting_line(still: bool, solo: bool) -> Div {
    let mark = if still {
        div()
            .debug_selector(|| "starting-mark-still".into())
            .child(icons::ferrite_icon(theme::GLYPH_BOX))
            .into_any_element()
    } else {
        div()
            .debug_selector(|| "starting-mark-live".into())
            .child(components::working_mark(theme::GLYPH_BOX, INK_STRONG))
            .into_any_element()
    };
    let grid = Grid::pane(Default::default(), solo);
    div()
        .debug_selector(|| "transcript-starting".into())
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .h(px(grid.line))
        .font_family(theme::FONT_UI)
        .text_size(px(grid.activity().size))
        .line_height(px(grid.line))
        .text_color(rgb(INK_SUBTLE))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .w(px(grid.gutter()))
                .child(mark),
        )
        .child(div().min_w_0().truncate().child("Starting"))
}

/// The working line's mark: the app's one running glyph
/// (`components::working_mark`), its orbs white; under reduced motion it
/// holds still. Its selector says which.
fn working_mark(live: bool) -> AnyElement {
    div()
        .debug_selector(move || {
            if live {
                "progress-mark-live".into()
            } else {
                "progress-mark-still".into()
            }
        })
        .child(components::working_mark(theme::GLYPH_BOX, INK_STRONG))
        .into_any_element()
}

/// The turn's clock (`1m04s`), counting from `started` on its own: a
/// loop the overlay draws (`loops_overlay::hosted`), woken each whole second
/// by the pulse clock, so the seconds turning redraw nothing of the
/// transcript around it. Under reduced motion it is drawn in place and the
/// line is drawn again as the seconds turn (`WorkingLine::seconds`).
fn live_clock(started: std::time::Instant) -> AnyElement {
    let clock = LiveClock { started };
    ferrite_ui::loops_overlay::hosted(move || clock.into_any_element()).into_any_element()
}

/// A background call's row while its work runs (HD64, Claude's
/// `bg-live-row.png`): `View live output ▸`, brighter than the row's muted
/// words, which opens the Background tasks pane. The row's owner wraps it
/// in the press. It goes when the work is handed back, and the row settles
/// to plain muted text.
pub fn background_link() -> Div {
    div()
        .debug_selector(|| "tool-background-live".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::SPACE_1))
        .ml(px(theme::SPACE_2))
        .cursor_pointer()
        .text_color(rgb(theme::INK_STRONG))
        .child("View live output")
        .child(components::chevron_at(0., theme::ICON_CHEVRON, INK_SUBTLE))
}

/// How long background work has run, counting from its launch (`33s`),
/// muted: the rail's and the pane's clock.
pub fn background_clock(launched: std::time::Instant) -> Div {
    div()
        .debug_selector(|| "background-clock".into())
        .flex_shrink_0()
        .text_color(rgb(INK_SUBTLE))
        .child(live_clock(launched))
}

#[derive(IntoElement, Clone, Copy)]
struct LiveClock {
    started: std::time::Instant,
}

impl RenderOnce for LiveClock {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        // Woken when the next whole second comes round.
        let _ = ferrite_ui::motion::loop_phase_from(
            self.started,
            Duration::from_secs(1),
            ferrite_ui::motion::Motion::Frames(1),
            window.current_view(),
            cx,
        );
        let elapsed = ferrite_core::clock::instant().saturating_duration_since(self.started);
        div()
            .debug_selector(|| "progress-clock".into())
            .child(SharedString::from(ferrite_core::progress::live_seconds(
                elapsed,
            )))
    }
}

/// The working line with no provider known: the spinner and caption in
/// `INK_SUBTLE` (the L2 cell's, retired with it; kept for the tests).
#[cfg(test)]
pub(super) fn working_row(
    transcript: &Transcript,
    compact: bool,
    focused: bool,
    received_reasoning_is_visible: bool,
    reduce_motion: bool,
) -> Div {
    working_line_for(
        transcript,
        compact,
        true,
        focused,
        received_reasoning_is_visible,
        reduce_motion,
        0,
        None,
    )
}

/// The working line (SPEC §5.4), the Claude app's one line, all in
/// `INK_SUBTLE` but its white orbs: the working mark, then what the turn
/// is doing, read off its execution state (`caption_of`), then `1m04s ·
/// ↓ 639 tokens` (the arrow an icon, the count live and rolling:
/// `odometer::rolling_count`) and, while the model thinks, `· thinking with
/// medium effort`. Every line's spinner turns on the one shared clock;
/// only the focused Pane's caption shimmers. The seconds are whole
/// (`progress::live_seconds`), tabular, so the text changes once a second.
/// The caption is what truncates; the facts keep their room. L2
/// (`compact`) draws the same row without the token count.
#[cfg(test)]
pub(super) fn working_line_for(
    transcript: &Transcript,
    compact: bool,
    solo: bool,
    focused: bool,
    received_reasoning_is_visible: bool,
    reduce_motion: bool,
    thread: u64,
    effort: Option<&str>,
) -> Div {
    WorkingLine::of(
        transcript,
        compact,
        solo,
        focused,
        received_reasoning_is_visible,
        reduce_motion,
        thread,
        effort,
    )
    .render(None)
}

/// What a working line draws, read off its transcript once
/// (`working_line_for`): the transcript's tail row renders it later, from
/// this alone, and its hash is the row's key — it changes when the words or
/// the whole seconds do, never per frame.
#[derive(Clone, Debug, Hash, PartialEq)]
pub(crate) struct WorkingLine {
    live: bool,
    spinning: bool,
    solo: bool,
    /// When the turn began: its clock counts on its own (`LiveClock`), so
    /// the seconds never change this line's key while the clock is hosted.
    started: Option<std::time::Instant>,
    /// The seconds as words, kept only under reduced motion, where no loop
    /// counts them and the line is drawn again as they turn.
    seconds: Option<String>,
    /// The turn's output tokens as counted live
    /// (`Transcript::turn_output_live`), when it has any.
    tokens: Option<u64>,
    /// While the model thinks: `thinking with medium effort`.
    thinking: Option<String>,
    /// The caption's selector and its words.
    caption: Option<(String, String)>,
    /// The Thread whose line this is: the key its words roll under.
    thread: u64,
    /// The plan the turn works to, its steps' words and states, drawn as
    /// the task list over the line (`task_list`).
    plan: Vec<(String, TaskState)>,
}

/// A plan step's state, as the task list marks it.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum TaskState {
    Done,
    Current,
    /// The step the turn stopped on (denied, or stopped by the operator).
    Blocked,
    ToDo,
}

/// The plan the turn works to, as the task list draws it. Once the turn
/// has ended on a stop, the step it was on reads blocked.
fn plan_of(transcript: &Transcript) -> Vec<(String, TaskState)> {
    let progress = transcript.progress();
    if !progress.has_plan {
        return Vec::new();
    }
    let stopped = transcript.status() != Status::Streaming
        && matches!(
            transcript.turn_outcome(),
            Some(ferrite_core::TurnOutcome::Interrupted)
        );
    progress
        .plan
        .iter()
        .map(|step| {
            use ferrite_core::progress::StepStatus;
            let state = match step.status {
                StepStatus::Completed => TaskState::Done,
                StepStatus::InProgress if stopped => TaskState::Blocked,
                StepStatus::InProgress => TaskState::Current,
                StepStatus::Pending => TaskState::ToDo,
            };
            (step.text.clone(), state)
        })
        .collect()
}

impl WorkingLine {
    #[allow(clippy::too_many_arguments)]
    fn of(
        transcript: &Transcript,
        compact: bool,
        solo: bool,
        focused: bool,
        received_reasoning_is_visible: bool,
        reduce_motion: bool,
        thread: u64,
        effort: Option<&str>,
    ) -> Self {
        let live = focused && !reduce_motion;
        let spinning = !reduce_motion;
        let started = transcript.turn_started();
        let seconds = transcript
            .turn_elapsed()
            .filter(|_| reduce_motion)
            .map(ferrite_core::progress::live_seconds);
        let tokens = transcript.turn_output_live();
        let tokens = (tokens > 0 && !compact).then_some(tokens);
        let progress = transcript.progress();
        let thinking =
            (progress.phase == Some(Phase::Thinking) && !compact).then(|| match effort {
                Some(effort) => format!("thinking with {effort} effort"),
                None => "thinking".to_owned(),
            });
        let plan = plan_of(transcript);
        let caption = caption_of(
            transcript,
            !compact && received_reasoning_is_visible,
            thread,
        );
        Self {
            live,
            spinning,
            solo,
            started,
            seconds,
            tokens,
            thinking,
            caption,
            plan,
            thread,
        }
    }

    /// `disclosure`: the chevron that opens the call the line carries.
    fn render(&self, disclosure: Option<LineDisclosure>) -> Div {
        let Self {
            live,
            spinning,
            solo,
            started,
            seconds: _,
            tokens,
            thinking,
            caption,
            plan,
            thread,
        } = self.clone();
        // The line stands at the Standard size: the reading keys scale the
        // transcript only.
        let grid = Grid::pane(Default::default(), solo);
        let mut row = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .w_full()
            .min_w_0()
            .h(px(grid.line))
            .font_family(theme::FONT_UI)
            .text_size(px(grid.activity().size))
            .line_height(px(grid.line))
            .text_color(rgb(INK_SUBTLE));
        if let Some((selector, caption)) = caption {
            // The caption reads in the muted ink, as Claude's does. New
            // words roll in over the old (Claude's `cds-morph`,
            // `motion::morph`).
            let face = move |caption: SharedString| {
                if live {
                    components::shimmer(caption, INK_SUBTLE)
                } else {
                    div().min_w_0().truncate().child(caption).into_any_element()
                }
            };
            let text = ferrite_ui::motion::morph(
                ("working-caption", thread as usize),
                SharedString::from(caption),
                face,
            );
            // The facts after the caption: the turn's clock, counting on its
            // own, then its tokens, rolling as they climb, then whether it
            // thinks.
            let seam = |words: &str| {
                let text = SharedString::from(format!(" \u{b7} {words}"));
                let highlights = separators(&text);
                StyledText::new(text).with_highlights(highlights)
            };
            let count = tokens.map(|tokens| {
                div()
                    .debug_selector(|| "progress-tokens".into())
                    .flex()
                    .items_center()
                    .child(seam(""))
                    .child(icon(icons::ARROW_DOWN, theme::WORKING_ARROW, INK_SUBTLE))
                    .child(div().ml(px(theme::SPACE_0_5)).child(
                        ferrite_ui::odometer::rolling_count(
                            format!("working-tokens-{thread}"),
                            tokens,
                        ),
                    ))
                    .child(" tokens")
            });
            let thinking = thinking.map(|thinking| {
                div()
                    .debug_selector(|| "progress-thinking".into())
                    .child(seam(&thinking))
            });
            let has_facts = started.is_some() || count.is_some() || thinking.is_some();
            row = row
                .debug_selector(move || selector.clone())
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .mr(px(theme::SPACE_2))
                        .h(px(grid.line))
                        .child(working_mark(spinning)),
                )
                .child(
                    div()
                        .debug_selector(|| "progress-reasoning".into())
                        .flex()
                        .min_w_0()
                        .flex_shrink(1.)
                        .child(text),
                )
                .when(has_facts, |row| {
                    row.child(components::tabular(
                        div()
                            .debug_selector(|| "progress-metadata".into())
                            .flex()
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .items_center()
                            .child("\u{a0}")
                            .children(started.map(live_clock))
                            .children(count)
                            .children(thinking),
                    ))
                })
                .when_some(disclosure, |row, disclosure| {
                    let toggle = disclosure.toggle;
                    row.child(
                        div()
                            .debug_selector(|| "progress-disclosure".into())
                            .flex_shrink_0()
                            .ml(px(theme::SPACE_1))
                            .child(ferrite_ui::motion::settled(
                                ("working-chevron", thread as usize),
                                disclosure.open,
                                ferrite_ui::motion::CHEVRON,
                                |turn| components::chevron_at(turn, theme::ICON_CHEVRON, INK_SUBTLE),
                            )),
                    )
                    .cursor_pointer()
                    .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        toggle(window, cx);
                    })
                });
        }
        let planned = !plan.is_empty();
        div()
            .flex()
            .flex_col()
            .gap(px(theme::TASK_CARD_GAP))
            .when(planned, |tail| tail.pt(px(theme::TASK_CARD_GAP)))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .children(planned.then(|| task_list(&plan, grid)))
            .child(row)
    }
}

/// What the working line says, from what the turn is executing (the
/// handoff's §10: derive the line from execution state), as its selector
/// and its words:
///
/// - a phase that is not the model's own work — retrying, compacting,
///   waiting — by name (`Retrying · Server busy`);
/// - else the call running now (its row waits until it finishes: the line
///   carries it), in the provider's description of it where it wrote one
///   (`Waiting 15 seconds then printing done`), else a verb and a short
///   target (`Running cargo test`, `text::working_words`);
/// - else, while the model thinks or writes, the provider's own heading
///   (`Checking build progress`), unless its reasoning row is in view
///   (`heading_shown`);
/// - else the turn's verb (`Reticulating`, `progress::spinner_verb`, held
///   for the whole turn): only when nothing more specific is known.
///
/// None trails off: the running glyph says the work goes on, as the Claude
/// app's line does.
fn caption_of(
    transcript: &Transcript,
    heading_shown: bool,
    thread: u64,
) -> Option<(String, String)> {
    let progress = transcript.progress();
    let generic = Phase::Working.label();
    let working = progress.phase.is_none_or(|phase| phase.label() == generic);
    if working {
        if let Some(call) = running_call(transcript.blocks()) {
            return Some(("progress-call".into(), text::working_words(call)));
        }
    }
    let caption = progress.caption()?;
    let caption = if heading_shown && working {
        generic.to_owned()
    } else {
        caption
    };
    // The selector names what the line means (`Working`); for the generic
    // word, its words are the turn's verb.
    let words = if caption == generic {
        ferrite_core::progress::spinner_verb(thread, transcript.turn_count())
    } else {
        caption.as_str()
    };
    let words = words.trim_end_matches(['\u{2026}', '.']).to_owned();
    Some((format!("progress-caption-{caption}"), words))
}

/// The call the turn runs now: its newest running call, looked for in the
/// current turn only (back to its prompt). Not an agent: its own row in
/// the agents' card runs with it, and the line never says a row twice.
/// The call the working line names: the turn's latest still running,
/// its agents aside (they have their own rows).
pub(crate) fn running_call(blocks: &[Block]) -> Option<&ToolBlock> {
    blocks
        .iter()
        .rev()
        .take_while(|block| !matches!(block.body, Body::Prompt(_)))
        .find_map(|block| match &block.body {
            Body::Tool(tool) if tool.state == ToolState::Running && !super::is_agent(tool) => {
                Some(tool)
            }
            _ => None,
        })
}

/// The task list (`TASK_*`): the plan the turn works to, a card on a
/// hairline ring, one row a step — done `check-circle` and its words
/// dimmed, the step in hand the working mark and its words
/// strong, a step to do a dashed ring and its words in the body's ink.
fn task_list(plan: &[(String, TaskState)], grid: Grid) -> Div {
    div()
        .debug_selector(|| "task-list".into())
        .flex()
        .flex_col()
        .gap(px(theme::TASK_ROW_GAP))
        // Stretched, hanging `CARD_OUTSET` past the column (one left edge).
        .ml(px(-theme::CARD_OUTSET))
        .min_w_0()
        .p(px(theme::TASK_CARD_PAD))
        .rounded(px(theme::R_CARD))
        .border_1()
        .border_color(theme::paint::CARD_RING)
        .font_family(theme::FONT_UI)
        .text_size(px(grid.activity().size))
        .line_height(px(grid.line))
        .children(plan.iter().map(|(words, state)| {
            let (mark, ink) = match state {
                TaskState::Done => (
                    icon(icons::CHECK_CIRCLE, theme::TASK_ICON, INK_SUBTLE).into_any_element(),
                    INK_SUBTLE,
                ),
                TaskState::Current => (
                    components::working_mark(theme::TASK_ICON, INK_STRONG),
                    INK_STRONG,
                ),
                TaskState::Blocked => (
                    icon(icons::CIRCLE_MINUS, theme::TASK_ICON, INK_SUBTLE).into_any_element(),
                    INK_STRONG,
                ),
                TaskState::ToDo => (
                    icon(icons::CIRCLE_DASHED, theme::TASK_ICON, INK_SUBTLE).into_any_element(),
                    INK_STRONG,
                ),
            };
            div()
                .flex()
                .items_center()
                .gap(px(theme::SPACE_2))
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .size(px(theme::TASK_ICON))
                        .child(mark),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(ink))
                        .child(SharedString::from(words.clone())),
                )
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::transcript::{Input, Lexer};
    use ferrite_core::SessionEvent;
    use std::sync::Arc;

    /// With no call running, the line keeps the thinking heading, and with
    /// nothing better known, the turn's verb.
    #[test]
    fn the_verb_holds_the_line_only_while_nothing_runs() {
        let (lexer, _) = Lexer::new();
        let mut transcript = Transcript::new(Arc::new(lexer));
        transcript.apply(Input::Prompt("Build".into()));
        transcript.apply(Input::Event(SessionEvent::ReasoningSummaryDelta {
            text: "**Checking build progress**".into(),
            summary_index: 0,
        }));
        let line = |transcript: &Transcript, heading_shown| {
            WorkingLine::of(transcript, false, true, false, heading_shown, true, 7, None)
                .caption
                .map(|(selector, _)| selector)
        };
        assert_eq!(
            line(&transcript, false).as_deref(),
            Some("progress-caption-Checking build progress")
        );
        // The heading's own row in view: the line says the verb.
        assert_eq!(
            line(&transcript, true).as_deref(),
            Some("progress-caption-Working")
        );
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "build".into(),
            name: "commandExecution".into(),
            input: serde_json::json!({"command": "cargo build --release"}),
        }));
        assert_eq!(line(&transcript, true).as_deref(), Some("progress-call"));
    }

    /// The count climbs with the thinking estimate, snaps up to the usage,
    /// counts on from there as the next message thinks, and never falls
    /// back when the turn's own (smaller) figure lands; while the model
    /// thinks, the line says so, with the Thread's effort.
    #[test]
    fn the_token_count_climbs_and_says_the_model_thinks() {
        let (lexer, _) = Lexer::new();
        let mut transcript = Transcript::new(Arc::new(lexer));
        transcript.apply(Input::Prompt("Plan it".into()));
        let line = |transcript: &Transcript| {
            WorkingLine::of(
                transcript,
                false,
                true,
                true,
                false,
                false,
                7,
                Some("medium"),
            )
        };
        assert_eq!(line(&transcript).tokens, None);
        let think = |transcript: &mut Transcript, estimated| {
            transcript.apply(Input::Event(SessionEvent::ThinkingTokens { estimated }));
            transcript.apply(Input::Event(SessionEvent::Progress {
                event: ferrite_core::progress::ProgressEvent::Phase {
                    phase: Phase::Thinking,
                    detail: String::new(),
                },
            }));
        };
        think(&mut transcript, 5);
        assert_eq!(line(&transcript).tokens, Some(5));
        assert_eq!(
            line(&transcript).thinking.as_deref(),
            Some("thinking with medium effort")
        );
        think(&mut transcript, 1400);
        assert_eq!(line(&transcript).tokens, Some(1400));
        transcript.apply(Input::Event(SessionEvent::TokenUsage {
            total_tokens: 2000,
            input_tokens: 0,
            cached_input_tokens: 0,
            output_tokens: 1500,
            reasoning_output_tokens: 0,
            context_window: None,
        }));
        assert_eq!(line(&transcript).tokens, Some(1500));
        // The next message thinks on top of the first's report.
        think(&mut transcript, 600);
        assert_eq!(line(&transcript).tokens, Some(2100));
        transcript.apply(Input::Event(SessionEvent::UsageDetails {
            details: ferrite_core::UsageDetails {
                scope: ferrite_core::UsageScope::Turn,
                input_tokens: 10,
                cached_input_tokens: 0,
                output_tokens: 1800,
                reasoning_output_tokens: 0,
            },
        }));
        assert_eq!(line(&transcript).tokens, Some(2100));
        // Writing, the model no longer thinks.
        transcript.apply(Input::Event(SessionEvent::TextDelta {
            text: "Done.".into(),
        }));
        assert_eq!(line(&transcript).thinking, None);
    }

    /// While a call runs the line carries it in its description.
    #[test]
    fn the_line_carries_the_running_call_in_its_own_words() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("Wait".into()));
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "sleep".into(),
            name: "Bash".into(),
            input: serde_json::json!({"command": "sleep 15 && echo done", "description": "Waiting 15 seconds then printing done"}),
        }));
        let line = WorkingLine::of(&transcript, false, true, false, false, true, 7, None);
        assert_eq!(
            line.caption,
            Some((
                "progress-call".to_owned(),
                "Waiting 15 seconds then printing done".to_owned()
            ))
        );
    }

    fn planned(transcript: &mut Transcript) {
        use ferrite_core::progress::{PlanStep, ProgressEvent, StepStatus};
        let step = |text: &str, status| PlanStep {
            text: text.into(),
            status,
        };
        transcript.apply(Input::Event(SessionEvent::Progress {
            event: ProgressEvent::Plan {
                steps: vec![
                    step("Read the config", StepStatus::Completed),
                    step("Delete the build", StepStatus::InProgress),
                    step("Rebuild", StepStatus::Pending),
                ],
                explanation: String::new(),
            },
        }));
    }

    /// The plan is the turn's: it stays under the turn's rows once the turn
    /// is over, and the step a stop landed on reads blocked.
    #[test]
    fn the_plan_stays_after_the_turn_and_blocks_where_it_stopped() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("Clean up".into()));
        planned(&mut transcript);
        let states = |transcript: &Transcript| {
            plan_of(transcript)
                .into_iter()
                .map(|(_, state)| state)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            states(&transcript),
            [TaskState::Done, TaskState::Current, TaskState::ToDo]
        );
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Interrupted,
            cost_usd: None,
        }));
        assert_eq!(
            states(&transcript),
            [TaskState::Done, TaskState::Blocked, TaskState::ToDo]
        );
        let facts = || WorkingFacts {
            transcript: &transcript,
            thread: ThreadId::new(1),
            starting: false,
            solo: true,
            focused: false,
            reduce_motion: true,
            reasoning_visible: false,
            effort: None,
        };
        assert!(
            working_tail(facts()).is_some(),
            "the idle thread still draws its plan"
        );
        let finished = Transcript::default();
        assert!(working_tail(WorkingFacts {
            transcript: &finished,
            ..facts()
        })
        .is_none());
    }
}
