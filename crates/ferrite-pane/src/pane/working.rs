//! A running turn's header and live row, and the task list over it.

use super::*;
use crate::transcript::LineDisclosure;
use std::rc::Rc;

/// What a Thread's running turn draws besides its transcript
/// (`working_tail`), gathered by the Cockpit when it syncs the transcript.
pub struct WorkingFacts<'a> {
    pub transcript: &'a Transcript,
    pub thread: ThreadId,
    /// The Session is starting, nothing streaming yet.
    pub starting: bool,
    pub solo: bool,
    /// The Pane holds the keyboard: only its live row shimmers.
    pub focused: bool,
    pub reduce_motion: bool,
}

/// What the running turn's rows draw besides its blocks: the header under
/// its prompt (`Working for 11m 24s`) and the live row at its tail
/// (`working_tail`). Its `key` changes when what either draws does, never
/// per frame: the header's clock counts on its own (`LiveClock`).
#[derive(Clone, Debug)]
pub struct WorkingTail {
    pub key: u64,
    state: Rc<Working>,
}

impl WorkingTail {
    /// The header's key while a turn runs; `None` with no turn running (a
    /// Session starting, a plan left by a turn cut short).
    pub(crate) fn header_key(&self) -> Option<u64> {
        match &*self.state {
            Working::Turn { header, .. } => Some(hash_of(header)),
            _ => None,
        }
    }

    /// The running turn's header (`turn_header`).
    pub(crate) fn header(&self) -> AnyElement {
        match &*self.state {
            Working::Turn { header, .. } => turn_header(header).into_any_element(),
            _ => div().into_any_element(),
        }
    }

    /// The live row at the tail, for the running turn's trailing `work`
    /// (`rows::live_start`); `disclosure` opens that work's rows above it.
    pub(crate) fn live(&self, work: &[Block], disclosure: Option<LineDisclosure>) -> AnyElement {
        let row = match &*self.state {
            Working::Turn { live, .. } => live_row(live, work, disclosure),
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
            .child(row)
            .into_any_element()
    }
}

fn hash_of(value: &impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// What the running turn draws, or what stands at the tail with none.
#[derive(Clone, Debug, Hash, PartialEq)]
enum Working {
    Turn { header: Header, live: Live },
    Starting { still: bool, solo: bool },
    /// No turn runs, but the last one was cut short: its plan stays under
    /// its rows, where the turn left it, the step it stopped on blocked.
    Plan {
        plan: Vec<(String, TaskState)>,
        solo: bool,
    },
}

/// The running turn's header facts.
#[derive(Clone, Debug, Hash, PartialEq)]
struct Header {
    /// When the turn began: its clock counts on its own (`LiveClock`), so
    /// the seconds never change this key while the clock is hosted. `None`
    /// before it is known: `Working...`.
    started: Option<std::time::Instant>,
    /// The seconds as words, kept only under reduced motion, where no loop
    /// counts them and the header is drawn again as they turn.
    seconds: Option<String>,
    /// The provider compacts the context: `Compacting for …`.
    compacting: bool,
}

/// The live row's facts besides the turn's trailing work.
#[derive(Clone, Debug, Hash, PartialEq)]
struct Live {
    /// The Pane holds the keyboard and motion is on: the words shimmer.
    shimmer: bool,
    /// Motion is on: the running glyph in the row's gutter turns.
    spinning: bool,
    /// A phase that is not the model's own work (`Retrying · Server
    /// busy`, `Waiting`), which the row says in place of `Thinking`.
    phase: Option<String>,
    /// The plan the turn works to, drawn as the task list over the row.
    plan: Vec<(String, TaskState)>,
}

/// What stands at the transcript's tail and under the running turn's
/// prompt (WP-A): while the transcript streams, the turn's header and live
/// row; while a Session starts with nothing streaming yet, its starting
/// line; a cut-short turn's plan; otherwise nothing. Only the Pane holding
/// the keyboard (`focused`) shimmers. It names no key: the stop button and
/// the placeholder say how to stop.
pub fn working_tail(facts: WorkingFacts) -> Option<WorkingTail> {
    let transcript = facts.transcript;
    // The turn goes on over a Decision that waits, its clock counting,
    // while the operator decides. Alone in its window only: a Group
    // Pane's Decision is an island over a small body, which keeps all its
    // room.
    let working = if transcript.status() == Status::Streaming
        || (transcript.status() == Status::Blocked && facts.solo)
    {
        let progress = transcript.progress();
        let phase = progress
            .phase
            .filter(|phase| matches!(phase, Phase::Retrying | Phase::Waiting))
            .and_then(|_| progress.caption());
        Working::Turn {
            header: Header {
                started: transcript.turn_started(),
                seconds: transcript
                    .turn_elapsed()
                    .filter(|_| facts.reduce_motion)
                    .map(ferrite_core::progress::live_seconds),
                compacting: progress.phase == Some(Phase::Compacting),
            },
            live: Live {
                shimmer: facts.focused && !facts.reduce_motion,
                spinning: !facts.reduce_motion,
                phase,
                plan: plan_of(transcript),
            },
        }
    } else if facts.starting {
        Working::Starting {
            still: facts.reduce_motion || !facts.focused,
            solo: facts.solo,
        }
    } else {
        let plan = plan_of(transcript);
        if plan.is_empty() {
            return None;
        }
        Working::Plan {
            plan,
            solo: facts.solo,
        }
    };
    Some(WorkingTail {
        key: hash_of(&working),
        state: Rc::new(working),
    })
}

/// The words a header or live row wears: `TURN_FOLD_TEXT` on
/// `TURN_FOLD_LINE`, `INK_SUBTLE`, tabular, on a `TURN_FOLD_ROW_H` line.
fn row_words() -> Div {
    components::tabular(
        div()
            .flex()
            .items_center()
            .min_w_0()
            .min_h(px(theme::TURN_FOLD_ROW_H))
            .font_family(theme::FONT_UI)
            .text_size(px(theme::TURN_FOLD_TEXT))
            .line_height(px(theme::TURN_FOLD_LINE))
            .text_color(rgb(INK_SUBTLE)),
    )
}

/// The running turn's header (T3's `WorkingTimelineRow`): `Working for
/// 11m 24s` under its prompt, over the fold's rule (`turn_rule`), its
/// clock ticking once a second on its own. It does not shimmer, and has no
/// mark, count or chevron. Before the turn's start is known it reads
/// `Working...`; while the provider compacts, `Compacting for …` after the
/// minimize mark.
fn turn_header(header: &Header) -> Div {
    let clock = || match (&header.seconds, header.started) {
        (Some(seconds), _) => Some(div().child(SharedString::from(seconds.clone())).into_any_element()),
        (None, Some(started)) => Some(live_clock(started)),
        (None, None) => None,
    };
    let words = match (header.compacting, clock()) {
        (true, Some(clock)) => div()
            .flex()
            .items_center()
            .gap(px(theme::TURN_FOLD_ICON_GAP))
            .child(icon(icons::MINIMIZE, theme::TURN_FOLD_ICON, INK_SUBTLE))
            .child(div().flex().child("Compacting for\u{a0}").child(clock)),
        (true, None) => div()
            .flex()
            .items_center()
            .gap(px(theme::TURN_FOLD_ICON_GAP))
            .child(icon(icons::MINIMIZE, theme::TURN_FOLD_ICON, INK_SUBTLE))
            .child("Compacting\u{2026}"),
        (false, Some(clock)) => div().flex().child("Working for\u{a0}").child(clock),
        (false, None) => div().child("Working..."),
    };
    turn_rule(
        row_words()
            .debug_selector(|| "turn-working".into())
            .h(px(theme::TURN_FOLD_ROW_H))
            .px(px(theme::TURN_FOLD_INSET))
            .whitespace_nowrap()
            .child(words),
    )
}

/// The frame a turn's header and its fold share: `TURN_FOLD_PAD_T` over
/// the row, `TURN_FOLD_TEXT_B` under it to a 1px rule of `TURN_FOLD_RULE`
/// the column's width.
pub(crate) fn turn_rule(row: impl IntoElement) -> Div {
    div()
        .w_full()
        .min_w_0()
        .pt(px(theme::TURN_FOLD_PAD_T))
        .pb(px(theme::TURN_FOLD_TEXT_B))
        .border_b_1()
        .border_color(theme::paint::TURN_FOLD_RULE)
        .child(row)
}

/// What a settled turn's fold row draws (`turn_fold`).
pub(crate) struct FoldFace<'a> {
    /// Names its hover blends: unique in the window.
    pub key: SharedString,
    pub label: &'a str,
    /// The turn's start (its prompt's `sent_at`), its hover time.
    pub started: Option<&'a str>,
    pub open: bool,
    /// The keyboard's target, holding its focus (↵ toggles it): its time
    /// shows, its words brighten.
    pub targeted: Option<&'a gpui::FocusHandle>,
}

/// A settled turn's fold (T3's turn fold, MessagesTimeline.tsx:1621-1652),
/// on the header's frame (`turn_rule`): `Worked for 24m 55s` and its
/// chevron, right while shut and down while open (the glyphs cross-fading,
/// never turning), a press target with no ground, the words not
/// selectable. Under the pointer they go `INK_SUBTLE` → `INK_STRONG` over
/// `motion::FOLD_HOVER`, and the turn's start time (`8:01 pm`) snaps in at
/// the row's end, taking no room until it shows.
pub(crate) fn turn_fold(
    face: FoldFace,
    toggle: Option<Rc<dyn Fn(&mut gpui::Window, &mut gpui::App)>>,
) -> AnyElement {
    use ferrite_ui::motion;
    let row_key = SharedString::from(format!("{}-row", face.key));
    let button_key = SharedString::from(format!("{}-words", face.key));
    let hovered = motion::hover_t(&row_key) > 0.;
    let t = if face.targeted.is_some() {
        1.
    } else {
        motion::hover_t(&button_key)
    };
    let ink = motion::mix(rgb(INK_SUBTLE).into(), rgb(INK_STRONG).into(), t);
    let open = face.open;
    let chevron = motion::settled(
        SharedString::from(format!("{}-chevron", face.key)),
        open,
        motion::CHEVRON,
        move |down| {
            let glyph = |path, opacity: f32| {
                div()
                    .absolute()
                    .inset_0()
                    .opacity(opacity)
                    .child(icon(path, theme::TURN_FOLD_CHEVRON, INK_SUBTLE).text_color(ink))
            };
            div()
                .debug_selector(move || {
                    if open {
                        "turn-fold-open".into()
                    } else {
                        "turn-fold-shut".into()
                    }
                })
                .relative()
                .flex_shrink_0()
                .size(px(theme::TURN_FOLD_CHEVRON))
                .child(glyph(icons::CHEVRON_RIGHT, 1. - down))
                .child(glyph(icons::CHEVRON_DOWN, down))
        },
    );
    let mut words = div()
        .id(button_key.clone())
        .debug_selector(|| "turn-fold-words".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::TURN_FOLD_GAP))
        .px(px(theme::TURN_FOLD_INSET))
        .rounded(px(theme::R_CONTROL))
        .whitespace_nowrap()
        .text_color(ink)
        .child(SharedString::from(face.label.to_owned()))
        .child(chevron)
        .on_hover(motion::hover_listener_with(
            button_key,
            motion::FOLD_HOVER,
            motion::FOLD_HOVER,
        ));
    if let Some(toggle) = toggle {
        words = words
            .cursor_pointer()
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                gpui::base::TextSelection::clear(window, cx);
                toggle(window, cx);
            });
    }
    let time = face
        .started
        .filter(|_| hovered || face.targeted.is_some())
        .map(|stamp| {
            let time = div()
                .id(SharedString::from(format!("{}-time", face.key)))
                .debug_selector(|| "turn-fold-time".into())
                .ml_auto()
                .mr(px(theme::SPACE_1))
                .flex_shrink_0()
                .rounded(px(theme::R_CONTROL))
                .text_size(px(theme::TURN_FOLD_TIME))
                .text_color(rgb(INK_SUBTLE));
            match ferrite_core::clock::parse_local(stamp) {
                Some(at) => time
                    .child(SharedString::from(ferrite_core::clock::label(at)))
                    .tooltip(ferrite_ui::menu::tooltip(ferrite_core::clock::exact_label(at))),
                // A log from before stamps were kept: the time it printed.
                None => time.child(SharedString::from(stamp.to_owned())),
            }
        });
    let mut row = row_words()
        .id(row_key.clone())
        .debug_selector(|| "turn-fold".into())
        .h(px(theme::TURN_FOLD_ROW_H))
        .pr(px(theme::TURN_FOLD_END))
        .gap(px(theme::TURN_FOLD_GAP))
        .on_hover(motion::hover_snap(row_key))
        .child(words)
        .children(time);
    if let Some(focus) = face.targeted {
        row = row
            .bg(theme::paint::RAISED)
            .rounded(px(theme::R_CONTROL))
            .relative()
            .child(
                div()
                    .debug_selector(|| "tool-disclosure-keyboard-target".into())
                    .absolute()
                    .inset_0(),
            )
            .track_focus(focus)
            .key_context("ToolDisclosure");
    }
    turn_rule(row).into_any_element()
}

/// The live row at a running turn's tail (T3's `work-live` and `Thinking`
/// rows): the newest step of its trailing `work` — `Running sed`, the
/// program alone (`text::live_words`), or once it finished `Ran sed` — or,
/// with no call there, `Thinking` (a phase that is not the model's own
/// work, by name). Its width hugs its words, which shimmer on the Pane
/// holding the keyboard. A thought in that work stands over it, its first
/// heading or line, four lines at most, until the work is opened; pressed,
/// it opens the work's rows above it (`disclosure`).
fn live_row(live: &Live, work: &[Block], disclosure: Option<LineDisclosure>) -> Div {
    let open = disclosure.as_ref().is_some_and(|disclosure| disclosure.open);
    let tool = work
        .iter()
        .rev()
        .filter_map(|block| match &block.body {
            Body::Tool(tool) => Some(tool),
            _ => None,
        })
        .min_by_key(|tool| tool.state != ToolState::Running);
    let (selector, words) = match (&live.phase, tool) {
        (Some(phase), _) => ("live-phase", phase.clone()),
        (None, Some(tool)) => ("live-call", text::live_words(tool)),
        (None, None) => ("live-thinking", "Thinking".to_owned()),
    };
    let thought = (!open)
        .then(|| {
            work.iter().rev().find_map(|block| match &block.body {
                Body::Thinking(text) if !text.trim().is_empty() => {
                    Some(ferrite_core::progress::headline(text))
                }
                _ => None,
            })
        })
        .flatten()
        .filter(|thought| !thought.is_empty());
    let face = if live.shimmer {
        components::shimmer(SharedString::from(words), INK_SUBTLE)
    } else {
        div().min_w_0().truncate().child(words).into_any_element()
    };
    let mut row = row_words()
        .debug_selector(move || selector.into())
        .max_w_full()
        .child(working_mark(live.spinning))
        .child(face);
    if let Some(disclosure) = disclosure {
        let toggle = disclosure.toggle;
        row = row
            .cursor_pointer()
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                toggle(window, cx);
            });
    }
    let planned = !live.plan.is_empty();
    div()
        .flex()
        .flex_col()
        .items_start()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .when(planned, |tail| tail.pt(px(theme::TASK_CARD_GAP)).gap(px(theme::TASK_CARD_GAP)))
        .children(planned.then(|| {
            div()
                .w_full()
                .child(task_list(&live.plan, Grid::pane(Default::default(), true)))
        }))
        .children(thought.map(|thought| {
            row_words()
                .debug_selector(|| "live-thought".into())
                .items_start()
                .gap(px(theme::SPACE_2))
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .h(px(theme::TURN_FOLD_LINE))
                        .child(icon(icons::REASONING, theme::ICON_CHEVRON, INK_SUBTLE)),
                )
                .child(
                    div()
                        .min_w_0()
                        .line_clamp(4)
                        .child(SharedString::from(thought)),
                )
        }))
        .child(row)
}

/// The live row's gutter: the app's one running glyph
/// (`components::working_mark`), its orbs white, in a `TURN_FOLD_ROW_H`
/// box; under reduced motion it holds still. Its selector says which. (The
/// row's step icons, T3's, take this gutter later.)
fn working_mark(live: bool) -> AnyElement {
    div()
        .debug_selector(move || {
            if live {
                "progress-mark-live".into()
            } else {
                "progress-mark-still".into()
            }
        })
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(theme::TURN_FOLD_ROW_H))
        .child(components::working_mark(theme::GLYPH_BOX, INK_STRONG))
        .into_any_element()
}

/// The tail's shape while a Session starts: nothing the provider says yet,
/// so Ferrite's own mark holds the gutter — the working mark on the
/// focused Pane, assembled and still elsewhere and under reduced motion —
/// and `Starting` in `INK_SUBTLE`, on the live row's own line, so the row
/// never shifts when the turn's live step replaces it.
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
    row_words()
        .debug_selector(|| "transcript-starting".into())
        .w_full()
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

/// The turn's clock (`1m 4s`), counting from `started` on its own: a loop
/// the overlay draws (`loops_overlay::hosted`), woken each whole second by
/// the pulse clock, so the seconds turning redraw nothing of the transcript
/// around it. Under reduced motion the header draws its seconds in place
/// and is drawn again as they turn (`Header::seconds`).
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

/// A plan step's state, as the task list marks it.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum TaskState {
    Done,
    Current,
    /// The step the turn stopped on (stopped by the operator, or failed).
    Blocked,
    ToDo,
}

/// The plan the turn works to, as the task list draws it, as Claude
/// Desktop's turn box shows it (§12; `craft/claude/parity/plan-*.png`):
/// only a plan this turn made or moved, and once the turn has ended only
/// one it was cut short in (stopped, or failed), the step it stopped on
/// reading blocked. A turn that ends in its answer takes its plan with it,
/// finished or not: nothing is left to look live.
fn plan_of(transcript: &Transcript) -> Vec<(String, TaskState)> {
    use ferrite_core::progress::StepStatus;
    use ferrite_core::TurnOutcome;
    let progress = transcript.progress();
    if !progress.has_plan || !progress.plan_touched {
        return Vec::new();
    }
    let running = matches!(transcript.status(), Status::Streaming | Status::Blocked);
    let cut_short = match transcript.turn_outcome() {
        None if running => false,
        Some(TurnOutcome::Interrupted | TurnOutcome::Error(_)) => true,
        None | Some(TurnOutcome::Completed) => return Vec::new(),
    };
    progress
        .plan
        .iter()
        .map(|step| {
            let state = match step.status {
                StepStatus::Completed => TaskState::Done,
                StepStatus::InProgress if cut_short => TaskState::Blocked,
                StepStatus::InProgress => TaskState::Current,
                StepStatus::Pending => TaskState::ToDo,
            };
            (step.text.clone(), state)
        })
        .collect()
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

    fn tail(transcript: &Transcript) -> Working {
        let tail = working_tail(WorkingFacts {
            transcript,
            thread: ThreadId::new(1),
            starting: false,
            solo: true,
            focused: true,
            reduce_motion: false,
        })
        .expect("a running turn");
        (*tail.state).clone()
    }

    /// The header counts from the turn's start and says when the provider
    /// compacts; the live row names a phase that is not the model's own
    /// work in place of `Thinking`. Neither counts tokens or names effort.
    #[test]
    fn the_header_counts_and_says_it_compacts() {
        let (lexer, _) = Lexer::new();
        let mut transcript = Transcript::new(Arc::new(lexer));
        transcript.apply(Input::Prompt("Build".into()));
        let phase = |transcript: &mut Transcript, phase| {
            transcript.apply(Input::Event(SessionEvent::Progress {
                event: ferrite_core::progress::ProgressEvent::Phase {
                    phase,
                    detail: String::new(),
                },
            }));
        };
        let Working::Turn { header, live } = tail(&transcript) else {
            panic!("a turn runs");
        };
        assert!(header.started.is_some());
        assert!(!header.compacting);
        assert_eq!(live.phase, None);
        phase(&mut transcript, Phase::Compacting);
        let Working::Turn { header, .. } = tail(&transcript) else {
            panic!("a turn runs");
        };
        assert!(header.compacting);
        phase(&mut transcript, Phase::Retrying);
        let Working::Turn { header, live } = tail(&transcript) else {
            panic!("a turn runs");
        };
        assert!(!header.compacting);
        assert_eq!(live.phase.as_deref(), Some("Retrying"));
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

    fn states(transcript: &Transcript) -> Vec<TaskState> {
        plan_of(transcript)
            .into_iter()
            .map(|(_, state)| state)
            .collect()
    }

    fn ended(transcript: &mut Transcript, outcome: ferrite_core::TurnOutcome) {
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome,
            cost_usd: None,
        }));
    }

    /// A turn cut short keeps its plan under its rows, where the turn left
    /// it, the step it stopped on blocked — never live
    /// (`craft/claude/parity/plan-interrupted.png`).
    #[test]
    fn a_plan_cut_short_stays_with_the_step_it_stopped_on_blocked() {
        for outcome in [
            ferrite_core::TurnOutcome::Interrupted,
            ferrite_core::TurnOutcome::Error("overloaded".into()),
        ] {
            let mut transcript = Transcript::default();
            transcript.apply(Input::Prompt("Clean up".into()));
            planned(&mut transcript);
            assert_eq!(
                states(&transcript),
                [TaskState::Done, TaskState::Current, TaskState::ToDo]
            );
            ended(&mut transcript, outcome);
            assert_eq!(
                states(&transcript),
                [TaskState::Done, TaskState::Blocked, TaskState::ToDo]
            );
            let facts = WorkingFacts {
                transcript: &transcript,
                thread: ThreadId::new(1),
                starting: false,
                solo: true,
                focused: true,
                reduce_motion: false,
            };
            assert!(
                working_tail(facts).is_some(),
                "the idle thread still draws its plan"
            );
        }
    }

    /// A turn that ends in its answer takes its plan with it, finished or
    /// not, as Claude Desktop's does (`plan-ended-unfinished.png`,
    /// `plan-finished.png`, `plan-wrapped-up.png`): no step is left to
    /// spin under an answer that is done. The owner's InvoiceMate thread
    /// planned four searches, ran them, answered, and its first step spun on.
    #[test]
    fn a_turn_that_ends_in_its_answer_takes_its_plan_with_it() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("Search the forums".into()));
        planned(&mut transcript);
        ended(&mut transcript, ferrite_core::TurnOutcome::Completed);
        assert!(states(&transcript).is_empty());
        let facts = WorkingFacts {
            transcript: &transcript,
            thread: ThreadId::new(1),
            starting: false,
            solo: true,
            focused: true,
            reduce_motion: false,
        };
        assert!(working_tail(facts).is_none(), "nothing under the answer");
    }

    /// The plan is the turn's that made it: the next turn shows none until
    /// it plans itself (`plan-next-turn.png`), and a steer folded into the
    /// running turn keeps it.
    #[test]
    fn a_later_turn_shows_no_earlier_plan() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("Clean up".into()));
        planned(&mut transcript);
        transcript.apply(Input::Prompt("and the cache too".into()));
        assert_eq!(states(&transcript).len(), 3, "a steer keeps the plan");
        ended(&mut transcript, ferrite_core::TurnOutcome::Interrupted);
        transcript.apply(Input::Prompt("Say hi".into()));
        assert!(
            states(&transcript).is_empty(),
            "the stopped plan is not this turn's"
        );
        planned(&mut transcript);
        assert_eq!(
            states(&transcript),
            [TaskState::Done, TaskState::Current, TaskState::ToDo]
        );
    }
}
