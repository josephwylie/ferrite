//! A turn's shape for the rows: where its work runs, and what its fold
//! (`Worked for 24m 55s ›`) hides once it settles (T3's `deriveTurnFolds`,
//! ADR 0038).
//!
//! A turn opens at a prompt and closes at its `TurnEnd`; a prompt inside an
//! open turn is a steer (`Transcript::apply`, `Input::Prompt` while
//! streaming). A settled turn folds its work under one row at its top,
//! under its prompt, leaving its final answer and what must stay in view;
//! work after a steer folds under a row of its own (`Continued for …`)
//! under the steer's prompt. Everything here reads the blocks alone, so a
//! replayed thread folds as the live one did.

use ferrite_core::transcript::{tools, Block, BlockId, Body, ToolState};
use ferrite_core::TurnOutcome;

/// One fold over a turn's work, or over a steer's segment of it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Fold {
    /// The prompt it stands under, which keys its open state
    /// (`DisclosureId::Turn`): the turn's, or a steer's.
    pub prompt: BlockId,
    /// `Worked for 24m 55s`, `Continued for 3s`, `You stopped after 4.1s`.
    pub label: String,
    /// When its work began (its prompt's `sent_at`): the row's hover time.
    pub started: Option<String>,
    /// The blocks it hides while shut, in order.
    pub hidden: Vec<BlockId>,
}

/// A block that hides under its turn's fold, if it were inside one: work,
/// thoughts, interim prose and notes. The operator's own blocks (prompts),
/// notices, the turn's resource cards (a visual, a wrap-up, review
/// findings), questions put to the operator, and anything still running (a
/// live subagent, background work) stay in view.
fn foldable(block: &Block) -> bool {
    match &block.body {
        Body::Prompt(_) | Body::Notice(_) | Body::Visual(_) | Body::WrapUp(_) => false,
        Body::Tool(tool) => {
            let kind = tools::policy(&tool.name).kind;
            tool.state != ToolState::Running
                && !matches!(kind, tools::ToolKind::Question | tools::ToolKind::Findings)
        }
        Body::TurnEnd(end) => end.outcome == TurnOutcome::Interrupted,
        _ => true,
    }
}

/// Work after the final answer folds only when it finished cleanly: a
/// failed call, or one still running, stays in view under the answer.
fn foldable_after_answer(block: &Block) -> bool {
    match &block.body {
        Body::Tool(tool) => matches!(tool.state, ToolState::Ok | ToolState::Stopped),
        Body::Thinking(_) => true,
        Body::TurnEnd(end) => end.outcome == TurnOutcome::Interrupted,
        _ => false,
    }
}

/// Agent prose: an answer's Markdown, or fallback prose.
fn is_prose(block: &Block) -> bool {
    block.markdown.is_some()
        || matches!(
            block.body,
            Body::Paragraph { .. } | Body::Heading { .. } | Body::Bullet { .. } | Body::Code { .. }
        )
}

/// One turn in the window: its prompt, its steers (each with the index its
/// segment starts at) and its end.
struct Turn {
    prompt: usize,
    steers: Vec<(usize, usize)>,
}

/// The prompt of the turn that is still open at the window's end (no
/// `TurnEnd` yet): the running turn's, where its `Working for` header
/// stands. `None` when the last turn closed, or its prompt was evicted.
pub(crate) fn open_turn(blocks: &[Block]) -> Option<usize> {
    let mut open = None;
    for (index, block) in blocks.iter().enumerate() {
        match block.body {
            Body::Prompt(_) if !block.steer => open = Some(index),
            Body::TurnEnd(_) => open = None,
            _ => {}
        }
    }
    open
}

/// Every fold the window's settled turns draw, in order. `draws`: whether a
/// block draws a row of its own (hidden thinking does not): a fold that
/// would hide nothing that draws is no fold.
pub(crate) fn folds(blocks: &[Block], draws: impl Fn(&Block) -> bool) -> Vec<Fold> {
    let mut turns = Vec::new();
    let mut open: Option<Turn> = None;
    for (index, block) in blocks.iter().enumerate() {
        match &block.body {
            // A turn whose end was never written (an older log) closes on
            // the next turn's prompt, unfolded: its time is not known.
            Body::Prompt(_) => match &mut open {
                Some(turn) if block.steer => {
                    // A steer splits the work once the turn has some: back
                    // to back, the split comes after the last of them.
                    if turn.steers.last().is_some_and(|(_, start)| *start == index) {
                        turn.steers.pop();
                    }
                    if index > turn.prompt + 1 {
                        turn.steers.push((index, index + 1));
                    }
                }
                // A steer whose turn's prompt was evicted heads no fold.
                _ if block.steer => {}
                _ => {
                    open = Some(Turn {
                        prompt: index,
                        steers: Vec::new(),
                    })
                }
            },
            Body::TurnEnd(_) => {
                if let Some(turn) = open.take() {
                    turns.push((turn, index));
                }
            }
            _ => {}
        }
    }
    let latest = turns.len().checked_sub(1);
    turns
        .into_iter()
        .enumerate()
        .flat_map(|(at, (turn, end))| turn_folds(blocks, &turn, end, Some(at) == latest, &draws))
        .collect()
}

/// A settled turn's folds: none when it failed (every step stays in view,
/// ADR 0007's failures never hidden) or when it hides nothing that draws.
fn turn_folds(
    blocks: &[Block],
    turn: &Turn,
    end: usize,
    latest: bool,
    draws: &impl Fn(&Block) -> bool,
) -> Vec<Fold> {
    let Body::TurnEnd(stamp) = &blocks[end].body else {
        return Vec::new();
    };
    if matches!(stamp.outcome, TurnOutcome::Error(_)) {
        return Vec::new();
    }
    // The final answer: the turn's last run of prose.
    let answer = (turn.prompt + 1..end).rev().find(|at| is_prose(&blocks[*at]));
    let answer_start = answer.map(|last| {
        (turn.prompt + 1..=last)
            .rev()
            .take_while(|at| is_prose(&blocks[*at]))
            .last()
            .unwrap_or(last)
    });
    let hides = |at: usize| {
        let block = &blocks[at];
        match (answer_start, answer) {
            (Some(start), Some(last)) if (start..=last).contains(&at) => false,
            (_, Some(last)) if at > last => foldable_after_answer(block),
            _ => foldable(block),
        }
    };
    let segment = |from: usize, to: usize| -> Vec<BlockId> {
        (from..to)
            .filter(|at| hides(*at))
            .map(|at| blocks[at].id)
            .collect()
    };
    let draws_any = |ids: &[BlockId]| {
        blocks
            .iter()
            .any(|block| ids.contains(&block.id) && draws(block))
    };
    // The segments: the turn's first, up to its first steer; then each
    // steer's, the last running through the turn's end.
    let mut bounds = vec![(turn.prompt, turn.prompt + 1)];
    bounds.extend(turn.steers.iter().copied());
    let segments: Vec<(usize, Vec<BlockId>)> = bounds
        .iter()
        .enumerate()
        .map(|(at, (prompt, from))| {
            let to = bounds.get(at + 1).map_or(end + 1, |(next, _)| *next);
            (*prompt, segment(*from, to))
        })
        .collect();
    let all: Vec<BlockId> = segments.iter().flat_map(|(_, ids)| ids.clone()).collect();
    if !draws_any(&all) {
        return Vec::new();
    }
    let sent = |at: usize| blocks[at].sent_at.clone();
    let duration = stamp
        .elapsed_ms
        .map(|ms| ms as f64)
        .or_else(|| span_ms(sent(turn.prompt).as_deref(), stamp.completed_at.as_deref()))
        .map(ferrite_core::progress::format_duration);
    let label = match (latest && stamp.outcome == TurnOutcome::Interrupted, duration) {
        (true, Some(duration)) => format!("You stopped after {duration}"),
        (true, None) => "You stopped this response".to_owned(),
        (false, Some(duration)) => format!("Worked for {duration}"),
        (false, None) => "Worked".to_owned(),
    };
    let mut folds = Vec::new();
    for (at, (prompt, hidden)) in segments.into_iter().enumerate() {
        if at == 0 {
            folds.push(Fold {
                prompt: blocks[prompt].id,
                label: label.clone(),
                started: sent(prompt),
                hidden,
            });
            continue;
        }
        if !draws_any(&hidden) {
            continue;
        }
        // A segment runs from its steer to the next steer, the last to the
        // turn's end.
        let until = bounds
            .get(at + 1)
            .map_or(stamp.completed_at.clone(), |(next, _)| sent(*next));
        let label = match span_ms(sent(prompt).as_deref(), until.as_deref()) {
            Some(ms) => format!(
                "Continued for {}",
                ferrite_core::progress::format_duration(ms)
            ),
            None => "Continued".to_owned(),
        };
        folds.push(Fold {
            prompt: blocks[prompt].id,
            label,
            started: sent(prompt),
            hidden,
        });
    }
    folds
}

/// The time between two stored stamps, when both were kept (an older log's
/// `7:18 pm` was not): never guessed.
fn span_ms(from: Option<&str>, to: Option<&str>) -> Option<f64> {
    let parse = |stamp: Option<&str>| stamp.and_then(ferrite_core::clock::parse_local);
    let (from, to) = (parse(from)?, parse(to)?);
    Some(to.signed_duration_since(from).num_milliseconds() as f64)
}

/// The running turn's trailing work, which its live row stands for
/// (`Running sed`): the calls and thoughts at its end, back to the first
/// block that is neither — prose, a prompt, a failed call, a question, an
/// agent (its own row runs with it), a suite counting its tests (its bar
/// stays in view, as a shut group keeps it). Blocks that draw nothing are
/// read across. Returns the range's start, `from` the turn's first block after
/// its prompt.
pub(crate) fn trailing_work(blocks: &[Block], from: usize, draws: impl Fn(&Block) -> bool) -> usize {
    let mut start = blocks.len();
    for at in (from..blocks.len()).rev() {
        let block = &blocks[at];
        // An earlier turn's end: its work is not this turn's.
        if matches!(block.body, Body::TurnEnd(_)) {
            break;
        }
        let work = block.markdown.is_none()
            && match &block.body {
                Body::Thinking(_) => true,
                Body::Tool(tool) => {
                    let kind = tools::policy(&tool.name).kind;
                    !matches!(tool.state, ToolState::Failed(_))
                        && tool.progress.is_none()
                        && !matches!(
                            kind,
                            tools::ToolKind::Question | tools::ToolKind::Agent | tools::ToolKind::Todo
                        )
                        && draws(block)
                }
                _ => false,
            };
        if work {
            start = at;
        } else if draws(block) {
            break;
        }
    }
    start
}
