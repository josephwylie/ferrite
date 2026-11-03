//! The working line under a running turn, and the task list over it.

use super::*;

/// What a Thread's working row reads besides its transcript (`working_tail`),
/// gathered by the Cockpit when it syncs the transcript.
pub struct WorkingFacts<'a> {
    pub transcript: &'a Transcript,
    pub thread: ThreadId,
    pub provider: Option<Provider>,
    /// The Session is starting, nothing streaming yet.
    pub starting: bool,
    pub solo: bool,
    /// The Pane holds the keyboard: only its line animates.
    pub focused: bool,
    /// A turn runs (the cockpit's `turn_running`, what the send circle
    /// reads too) and its Composer is empty: together, whether the send
    /// circle shows stop (`send_is_stop`).
    pub running: bool,
    pub composer_empty: bool,
    pub reduce_motion: bool,
    /// The live reasoning caption's own row is in view.
    pub reasoning_visible: bool,
}

/// The working row's shape: the live line, or the starting shape.
#[derive(Clone, Debug, Hash, PartialEq)]
enum Working {
    Line(WorkingLine),
    Starting { still: bool, solo: bool },
}

/// WP-A · the working row (the prototype's `.comp > .r`): the transcript's
/// last row while a turn runs, so it reads as the next line of the
/// conversation under its newest row. While the transcript streams, the
/// line; while a Session starts with nothing streaming yet, its starting
/// shape; otherwise nothing. Only the Pane holding the keyboard (`focused`)
/// animates it. Where the send circle shows stop (Solo's, with nothing
/// typed — `send_is_stop`) the line spends no words on the key; anywhere
/// else (a draft typed during the turn, a Group Pane's compact box, a
/// Subagent's footer) the focused line names `esc to interrupt`, so a way
/// to stop the turn is always in view. Its key changes with its words and
/// whole seconds, never per frame; the spinner and the shimmer are loops
/// (the loops overlay), so they redraw nothing of the transcript.
pub fn working_tail(facts: WorkingFacts) -> Option<crate::transcript::TranscriptTail> {
    let working = if facts.transcript.status() == Status::Streaming {
        let stop_shown = send_is_stop(facts.running, facts.composer_empty, !facts.solo);
        Working::Line(WorkingLine::of(
            facts.transcript,
            false,
            facts.solo,
            facts.focused,
            facts.focused && !stop_shown,
            facts.reasoning_visible,
            facts.reduce_motion,
            facts.provider,
            facts.thread.get(),
        ))
    } else if facts.starting {
        Working::Starting {
            still: facts.reduce_motion || !facts.focused,
            solo: facts.solo,
        }
    } else {
        return None;
    };
    let key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        working.hash(&mut hasher);
        hasher.finish()
    };
    Some(crate::transcript::TranscriptTail {
        key,
        render: std::rc::Rc::new(move |_, _| {
            let line = match &working {
                Working::Line(line) => line.render(),
                Working::Starting { still, solo } => starting_line(*still, *solo),
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
/// says yet, so Ferrite's own mark holds the gutter — its shards snapping on
/// the focused Pane, assembled and still elsewhere and under reduced motion
/// — and `Starting` in `INK_SUBTLE`, on the working line's own line (Solo's
/// or a Group Pane's), so the line never shifts when the turn's words
/// replace it. Its element id is a constant: the timeline has to survive
/// every re-render of the line.
pub(super) fn starting_line(still: bool, solo: bool) -> Div {
    let mark = if still {
        div()
            .debug_selector(|| "starting-mark-still".into())
            .child(icons::ferrite_icon(theme::GLYPH_BOX))
            .into_any_element()
    } else {
        div()
            .debug_selector(|| "starting-mark-live".into())
            .child(icons::animated_ferrite_icon(
                theme::GLYPH_BOX,
                "starting-progress-indicator",
            ))
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

/// What a focused Group Pane's working line says at its right end.
const INTERRUPT_HINT: &str = "esc to interrupt";

/// The working line's mark: three dots in the provider's colour, a pulse
/// running across them (`components::working_dots`, the Claude app's line);
/// under reduced motion they sit still. Its selector says which.
fn working_mark(ink: u32, live: bool) -> AnyElement {
    div()
        .debug_selector(move || {
            if live {
                "progress-mark-live".into()
            } else {
                "progress-mark-still".into()
            }
        })
        .child(components::working_dots(ink))
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
        false,
        received_reasoning_is_visible,
        reduce_motion,
        None,
        0,
    )
}

/// The working line (SPEC §5.4): one activity row in the app's face, a
/// step under the prose on a Solo (`solo`) or a Group Pane's transcript
/// line, the star spinner in the gutter, the provider's
/// live caption in its colour — the turn's spinner verb (`Reticulating…`,
/// `progress::spinner_verb`, held for the whole turn) unless the provider
/// has something better to say (R16) — then `1m04s · ↑ 4.1k tokens` in
/// `INK_SUBTLE`, and, where no stop button stands for the key (`interrupt`),
/// `esc to interrupt` (`INK_SUBTLE`) at the line's right. Every working line's spinner cycles,
/// on the one shared clock, so all of them show the same frame (the
/// prototype's `.spin`); only the focused Pane's caption shimmers. The seconds
/// are whole (`progress::live_seconds`), tabular, so the text changes once a
/// second. The caption is
/// what truncates; the facts keep
/// their room. L2 (`compact`) draws the same row without the token count.
#[cfg(test)]
pub(super) fn working_line_for(
    transcript: &Transcript,
    compact: bool,
    solo: bool,
    focused: bool,
    interrupt: bool,
    received_reasoning_is_visible: bool,
    reduce_motion: bool,
    provider: Option<Provider>,
    thread: u64,
) -> Div {
    WorkingLine::of(
        transcript,
        compact,
        solo,
        focused,
        interrupt,
        received_reasoning_is_visible,
        reduce_motion,
        provider,
        thread,
    )
    .render()
}

/// What a working line draws, read off its transcript once
/// (`working_line_for`): the transcript's tail row renders it later, from
/// this alone, and its hash is the row's key — it changes when the words or
/// the whole seconds do, never per frame.
#[derive(Clone, Debug, Hash, PartialEq)]
pub(crate) struct WorkingLine {
    ink: u32,
    live: bool,
    spinning: bool,
    interrupt: bool,
    solo: bool,
    /// When the turn began: its clock counts on its own (`LiveClock`), so
    /// the seconds never change this line's key while the clock is hosted.
    started: Option<std::time::Instant>,
    /// The seconds as words, kept only under reduced motion, where no loop
    /// counts them and the line is drawn again as they turn.
    seconds: Option<String>,
    /// The turn's output tokens (`↑ 4.1k tokens`), when it has any.
    tokens: Option<String>,
    /// The caption's selector and its words.
    caption: Option<(String, String)>,
    /// The plan the turn works to, its steps' words and states, drawn as
    /// the task list over the line (`task_list`).
    plan: Vec<(String, TaskState)>,
}

/// A plan step's state, as the task list marks it.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum TaskState {
    Done,
    Current,
    ToDo,
}

impl WorkingLine {
    #[allow(clippy::too_many_arguments)]
    fn of(
        transcript: &Transcript,
        compact: bool,
        solo: bool,
        focused: bool,
        interrupt: bool,
        received_reasoning_is_visible: bool,
        reduce_motion: bool,
        provider: Option<Provider>,
        thread: u64,
    ) -> Self {
        let ink = provider_ink(provider);
        let live = focused && !reduce_motion;
        let spinning = !reduce_motion;
        let started = transcript.turn_started();
        let seconds = transcript
            .turn_elapsed()
            .filter(|_| reduce_motion)
            .map(ferrite_core::progress::live_seconds);
        let tokens = transcript.turn_output_tokens();
        let tokens = (tokens > 0 && !compact)
            .then(|| format!("\u{2191} {} tokens", tokens_label(tokens)));
        let progress = transcript.progress();
        let plan = if progress.has_plan {
            progress
                .plan
                .iter()
                .map(|step| {
                    use ferrite_core::progress::StepStatus;
                    let state = match step.status {
                        StepStatus::Completed => TaskState::Done,
                        StepStatus::InProgress => TaskState::Current,
                        StepStatus::Pending => TaskState::ToDo,
                    };
                    (step.text.clone(), state)
                })
                .collect()
        } else {
            Vec::new()
        };
        let generic = Phase::Working.label();
        let caption = progress
            .caption()
            .map(|caption| {
                if !compact && received_reasoning_is_visible {
                    progress.phase.map(Phase::label).unwrap_or(generic).into()
                } else {
                    caption
                }
            })
            .map(|caption| {
                // The selector names what the line means (`Working`); the words
                // are the turn's verb.
                if caption == generic {
                    (
                        format!("progress-caption-{generic}"),
                        format!(
                            "{}\u{2026}",
                            ferrite_core::progress::spinner_verb(thread, transcript.turn_count())
                        ),
                    )
                } else {
                    // R16: the caption always trails off (`Reticulating…`).
                    let words = if caption.ends_with('\u{2026}') {
                        caption.clone()
                    } else {
                        format!("{caption}\u{2026}")
                    };
                    (format!("progress-caption-{caption}"), words)
                }
            });
        Self {
            ink,
            live,
            spinning,
            interrupt,
            solo,
            started,
            seconds,
            tokens,
            caption,
            plan,
        }
    }

    fn render(&self) -> Div {
        let Self {
            ink,
            live,
            spinning,
            interrupt,
            solo,
            started,
            seconds: _,
            tokens,
            caption,
            plan,
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
            .line_height(px(grid.line));
        if let Some((selector, caption)) = caption {
            // The caption reads in the body's ink; the provider's colour is
            // the dots' (a working glyph, rule 6).
            let caption = SharedString::from(caption);
            let text = if live {
                components::shimmer(caption, INK)
            } else {
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(INK))
                    .child(caption)
                    .into_any_element()
            };
            // The facts after the caption: the turn's clock, counting on its
            // own, then its tokens.
            let rest = tokens.map(|tokens| {
                let text = SharedString::from(format!(" \u{b7} {tokens}"));
                let highlights = separators(&text);
                StyledText::new(text).with_highlights(highlights)
            });
            let has_facts = started.is_some() || rest.is_some();
            row = row
                .debug_selector(move || selector.clone())
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .mr(px(theme::SPACE_2))
                        .h(px(grid.line))
                        .child(working_mark(ink, spinning)),
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
                            .text_color(rgb(INK_SUBTLE))
                            .child("\u{a0}")
                            .children(started.map(live_clock))
                            .children(rest),
                    ))
                })
                // Where no stop button stands for the key, the focused line
                // names it at its right end.
                .when(interrupt, |row| {
                    row.child(div().flex_1().min_w(px(theme::SPACE_6))).child(
                        div()
                            .debug_selector(|| "progress-interrupt".into())
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .text_color(rgb(INK_SUBTLE))
                            .child(INTERRUPT_HINT),
                    )
                });
        }
        div()
            .flex()
            .flex_col()
            .gap(px(theme::SPACE_2))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .children((!plan.is_empty()).then(|| task_list(&plan, grid)))
            .child(row)
    }
}

/// The task list (`TASK_*`): the plan the turn works to, a card on a
/// hairline ring, one row a step — done `check-circle` and its words
/// dimmed, the step in hand the braille spinner (`RUNNING`) and its words
/// strong, a step to do a dashed ring and its words in the body's ink.
fn task_list(plan: &[(String, TaskState)], grid: Grid) -> Div {
    div()
        .debug_selector(|| "task-list".into())
        .flex()
        .flex_col()
        .gap(px(theme::TASK_ROW_GAP))
        .w_full()
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
                TaskState::Current => (components::braille_spinner(RUNNING), INK_STRONG),
                TaskState::ToDo => (
                    icon(icons::CIRCLE_DASHED, theme::TASK_ICON, INK_SUBTLE).into_any_element(),
                    INK,
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

/// `8.0k`, `12k`, `340` — the token count the way Claude Code prints it
/// (`progress::token_label`, which the turn's stamp reads too).
pub(super) fn tokens_label(tokens: u64) -> String {
    ferrite_core::progress::token_label(tokens)
}
