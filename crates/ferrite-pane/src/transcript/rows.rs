//! Stable, owned semantic rows for the transcript virtual list.
//!
//! The Pane supplies its retained block window. This module neither decides
//! history retention nor renders a row; it only preserves the identities a
//! `ListState` needs to reconcile changing transcript content.

use std::{collections::HashMap, hash::Hash, ops::Range, rc::Rc};

use ferrite_core::transcript::outputs::{self, Output};
use ferrite_core::transcript::{tools, Block, BlockId, Body, ToolBlock, ToolState};

/// A stable semantic-row identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RowId {
    /// An adjacent native Markdown answer, named by its original run.
    Markdown(BlockId),
    /// One transcript block: a prompt, a tool call, a note.
    Block(BlockId),
    /// A run of routine calls folded under one summary, named by its first
    /// call's block: the run grows at its tail, so the name holds.
    Group(BlockId),
    /// What a finished turn left to open (`outputs::of_turn`): its cards,
    /// named by the turn's end.
    Outputs(BlockId),
    /// The `note` under the first prompt (`RowShape::note`).
    Note,
    /// The working row while a turn runs (`pane::working_tail`).
    Working,
    /// A long thread's date divider, before the first prompt of a new day,
    /// named by that prompt's block.
    Divider(BlockId),
    /// What a package appends after the last row (the pending Decision).
    Tail,
}

/// What a row is, for the gap table: derived once in `project`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowKind {
    Prompt,
    /// A muted line a row's owner adds that no block says
    /// (`RowShape::note`), standing a line under the prompt it follows.
    Note,
    /// Agent prose. `narration`: more work follows it in its turn, so it
    /// is the agent talking while it works (muted, as the work is); the
    /// turn's last prose, with no work after it, is its answer.
    Answer {
        narration: bool,
    },
    /// One tool call.
    Activity,
    /// Two or more routine calls in a row, folded under one summary
    /// (`Ran 4 commands, read 1 file`). It spaces as a call does.
    Group,
    Reasoning,
    Notice,
    /// A decision record or a revival note: it hangs on an elbow under the
    /// row it answers.
    Meta,
    /// A turn's end: its stamp, or (`hangs`) the elbow note that it was
    /// interrupted or failed.
    TurnEnd {
        hangs: bool,
    },
    /// An agent's visual: its call line and the page. It spaces as a
    /// diffed call does: a line above and below it, so the page never sits
    /// flush against the calls around it.
    Visual,
    /// A finished turn's page and file cards, under its answer: half a
    /// line off whatever it follows.
    Outputs,
    /// The working line, under the newest row while a turn runs: it spaces
    /// as the work does.
    Working,
    /// The tail row (the Decision), a line under the row before it.
    Tail,
    /// A date divider between turns on different days: a turn's gap above
    /// it, a line between it and its day's first prompt.
    Divider,
    /// A turn's wrap-up box. `spent`: a later prompt (or wrap-up) follows
    /// it, so its steps are no longer offered.
    WrapUp {
        spent: bool,
    },
    /// A fallback prose or code block.
    Other,
}

impl RowKind {
    fn of(block: &Block) -> Self {
        match &block.body {
            Body::Prompt(_) => Self::Prompt,
            Body::Tool(_) => Self::Activity,
            Body::Visual(_) => Self::Visual,
            Body::WrapUp(_) => Self::WrapUp { spent: false },
            Body::Thinking(_) => Self::Reasoning,
            Body::Notice(_) => Self::Notice,
            Body::Meta(_) => Self::Meta,
            Body::TurnEnd(end) => Self::TurnEnd {
                hangs: !matches!(end.outcome, ferrite_core::TurnOutcome::Completed),
            },
            // Fallback prose (no Markdown source) is agent prose too.
            Body::Paragraph { .. } | Body::Heading { .. } | Body::Bullet { .. } => {
                Self::Answer { narration: false }
            }
            Body::Code { .. } => Self::Other,
        }
    }
}

/// The space above a row, from the row before it (`None`: the first row),
/// its own kind and the answer size `reading`: the transcript's one table
/// of vertical rhythm (the turn rhythm in `theme.rs`, Claude's measured
/// spacing). A prompt's bubble stands `tx_after_prompt` above whatever
/// answers it and `tx_turn_gap` under the turn before it, each counting the
/// message's actions row laid out at rest between them (the prompt's under
/// its bubble, an answer's under its prose). A turn's calls and
/// groups read as one list, half a line apart; its prose stands
/// `tx_work_prose` from the calls on either side. Every other block (the
/// stamp, a thought, the Decision) stands a line and a half off. A row
/// that answers the one above it (a decision record, an interrupted or
/// failed turn's end) sits flush under it. A first row is flush to the top
/// when it is a prompt and a line down otherwise (both under the body's own
/// top padding, `RowShape::top_pad`).
pub(crate) fn gap_before(previous: Option<RowKind>, kind: RowKind, reading: f32) -> f32 {
    use ferrite_ui::theme;
    use RowKind::*;
    let line = theme::prose_line_height(reading);
    let Some(previous) = previous else {
        return if kind == Prompt { 0. } else { line };
    };
    let call = |kind| matches!(kind, Activity | Group | Visual | Notice);
    let work = |kind| call(kind) || matches!(kind, Working | Answer { narration: true });
    match (previous, kind) {
        (_, TurnEnd { hangs: true } | Meta) => 0.,
        (Divider, _) => line,
        (previous, Divider) => gap_before(Some(previous), Prompt, reading),
        (_, Outputs) => theme::tx_half(line),
        (Prompt, _) => (theme::tx_after_prompt(line) - theme::MESSAGE_ACTION).max(0.),
        (Answer { narration: false }, Prompt) => {
            theme::tx_turn_gap(line) - theme::MESSAGE_ACTIONS_GAP - theme::MESSAGE_ACTION
        }
        (_, Prompt) => theme::tx_turn_gap(line),
        (previous, Answer { .. }) if call(previous) => theme::tx_work_prose(line),
        (Answer { narration: true }, kind) if call(kind) => theme::tx_work_prose(line),
        (previous, kind) if work(previous) && work(kind) => theme::tx_half(line),
        _ => (line * 1.5).round(),
    }
}

/// What a projection reads besides the blocks: the body's top padding, the
/// tail row's key, the call a pending Decision gates (hidden
/// while it waits), and whether thinking draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RowShape {
    /// Room over the first row (`TX_PAD_T` in Solo, `TX_PAD_T_BOARD` on a
    /// board).
    pub top_pad: f32,
    /// The working row's key, while a turn runs.
    pub working: Option<u64>,
    pub tail: Option<u64>,
    pub pending_call: Option<String>,
    /// The model's thinking drawn as `∴` rows; off, it draws nothing.
    pub thinking: bool,
    /// The running call the working line carries (`carried_call`): set by
    /// the projection itself.
    pub carried: Option<String>,
    /// The line's call is opened (`carried_tool`): its row shows in the
    /// line's place.
    pub carried_open: bool,
    /// A muted line under the first prompt (`TranscriptInput::note`).
    pub note: Option<gpui::SharedString>,
}

/// One renderable transcript unit. Its blocks are owned so a list callback
/// does not borrow the transient slice passed to [`TranscriptRows::reconcile`].
///
/// Everything a row draws is in its equality — its gap included — so a row whose neighbour changed its spacing is a changed row,
/// re-measured by reconcile and never per frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptRow {
    id: RowId,
    blocks: Rc<[Block]>,
    source: Option<Rc<str>>,
    kind: RowKind,
    gap: f32,
    /// The tail row's key: a change re-renders it.
    tail_key: Option<u64>,
    /// An outputs row's cards.
    outputs: Rc<[Output]>,
}

impl TranscriptRow {
    /// Stable identity used by list reconciliation and element keys.
    pub(crate) fn id(&self) -> &RowId {
        &self.id
    }

    /// The exact retained blocks belonging to this semantic row.
    pub(crate) fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Joined Markdown source for an answer row, preserving native parser input.
    pub(crate) fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// What the row is, for the gap table and the prompt band.
    pub(crate) fn kind(&self) -> RowKind {
        self.kind
    }

    /// The space above this row (`gap_before`).
    pub(crate) fn gap(&self) -> f32 {
        self.gap
    }

    /// What an outputs row's cards open; empty for any other row.
    pub(crate) fn outputs(&self) -> &[Output] {
        &self.outputs
    }

    #[cfg(test)]
    pub(crate) fn tail_key(&self) -> Option<u64> {
        self.tail_key
    }
}

/// The owned row snapshot a virtual-list callback reads.
#[derive(Clone, Debug, Default)]
pub(crate) struct TranscriptRows {
    rows: Rc<[Rc<TranscriptRow>]>,
    /// The indices of the visual rows, for the per-layout reach pass.
    visuals: Rc<[usize]>,
}

impl TranscriptRows {
    /// Project the caller-owned retained window into semantic rows, spaced
    /// for answers at `reading` px.
    pub(crate) fn new(blocks: &[Block], shape: &RowShape, reading: f32) -> Self {
        let rows = project(blocks, shape, reading);
        Self {
            visuals: visual_indices(&rows),
            rows: rows.into(),
        }
    }

    /// The indices of the rows that are visuals.
    pub(crate) fn visual_rows(&self) -> &[usize] {
        &self.visuals
    }

    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub(crate) fn rows(&self) -> &[Rc<TranscriptRow>] {
        &self.rows
    }

    pub(crate) fn get(&self, index: usize) -> Option<&Rc<TranscriptRow>> {
        self.rows.get(index)
    }

    /// Re-project `blocks`, retaining pointer identity for unchanged rows.
    ///
    /// The returned splices are applied in order to the old list. Existing
    /// rows whose content changed are named in their final indices for lazy
    /// height invalidation.
    pub(crate) fn reconcile(
        &mut self,
        blocks: &[Block],
        shape: &RowShape,
        reading: f32,
    ) -> RowDelta {
        let previous = self.rows.clone();
        let projected = project(blocks, shape, reading);
        let old_by_id: HashMap<_, _> = previous
            .iter()
            .map(|row| (row.id.clone(), row.clone()))
            .collect();
        let next: Vec<_> = projected
            .into_iter()
            .map(|row| match old_by_id.get(&row.id) {
                Some(old) if old.as_ref() == row.as_ref() => old.clone(),
                _ => row,
            })
            .collect();
        let delta = RowDelta::between(&previous, &next);
        self.visuals = visual_indices(&next);
        self.rows = next.into();
        delta
    }
}

fn visual_indices(rows: &[Rc<TranscriptRow>]) -> Rc<[usize]> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.kind == RowKind::Visual)
        .map(|(index, _)| index)
        .collect()
}

/// One structural replacement in a [`gpui::ListState`].
///
/// Apply splices in their stored order. Each `old_range` is relative to the
/// list after preceding splices have run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RowSplice {
    pub(crate) old_range: Range<usize>,
    pub(crate) new_count: usize,
}

/// A list reconciliation plan for one new [`TranscriptRows`] snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RowDelta {
    pub(crate) splices: Vec<RowSplice>,
    /// Final row indices whose existing content changed and need measuring.
    pub(crate) remeasure: Vec<usize>,
}

impl RowDelta {
    fn between(previous: &[Rc<TranscriptRow>], next: &[Rc<TranscriptRow>]) -> Self {
        let previous_ids: Vec<_> = previous.iter().map(|row| row.id.clone()).collect();
        let next_ids: Vec<_> = next.iter().map(|row| row.id.clone()).collect();
        let splices = splices(&previous_ids, &next_ids);

        let previous_by_id: HashMap<_, _> =
            previous.iter().map(|row| (row.id.clone(), row)).collect();
        let remeasure = next
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                previous_by_id
                    .get(&row.id)
                    .is_some_and(|old| old.as_ref() != row.as_ref())
                    .then_some(index)
            })
            .collect();

        Self { splices, remeasure }
    }
}

/// Whether `call` is the call a pending Decision gates: the same id, or the
/// provider's item id inside an attributed key (`["turn","item"]`).
pub fn gated(call: &str, pending: &str) -> bool {
    call == pending || call.ends_with(&format!("\"{pending}\"]"))
}

/// The run a call folds into with the calls beside it, by its policy: the
/// routine run (`RollsUp`: a command, a read, a search, an edit — `Edited 3
/// files` —, an MCP call) or the agents' (`Agents`: Claude's `Ran 2 agents`
/// card). Calls of different runs never share a group. A question, a visual
/// or a checklist call that failed has none: it keeps a row of its own and
/// ends the group before it. Decided by the tool's policy, never by a result
/// still to come.
pub(crate) fn run_of(tool: &ToolBlock) -> Option<tools::Grouping> {
    let grouping = tools::policy(&tool.name).grouping;
    matches!(grouping, tools::Grouping::RollsUp | tools::Grouping::Agents).then_some(grouping)
}

/// A block that projects to no row: blank or hidden thinking, an answered
/// request's record, the `opened in` note, the call a pending Decision
/// gates, a checklist call that did not fail (the plan draws what it did:
/// `ToolBlock::draws_row`), a quick turn's end (`pane::turn_stamp`). A group
/// of calls reads across it.
fn draws_nothing(block: &Block, shape: &RowShape) -> bool {
    if block.markdown.is_some() {
        return false;
    }
    match &block.body {
        Body::Thinking(text) => text.trim().is_empty() || !shape.thinking,
        Body::Meta(text) => ferrite_core::transcript::is_decision_record(text),
        Body::Notice(text) => ferrite_core::transcript::is_opened_notice(text),
        Body::Tool(tool) => {
            is_gated(tool, shape)
                || !tool.draws_row()
                || shape.carried.as_deref() == Some(tool.call.as_str())
        }
        Body::TurnEnd(end) => crate::pane::turn_stamp(end).is_none(),
        // A recap alone only updates the Thread's status.
        Body::WrapUp(wrap) => !wrap.shows_box(),
        _ => false,
    }
}

/// The call a pending Decision gates waits in the Decision itself; it
/// shows once answered.
fn is_gated(tool: &ToolBlock, shape: &RowShape) -> bool {
    tool.state == ToolState::Running
        && shape
            .pending_call
            .as_deref()
            .is_some_and(|pending| gated(&tool.call, pending))
}

/// The call the working line carries while the turn runs: the routine
/// call it names (`pane::running_call`), until the call has something of
/// its own to show (output, a test count). Its row appears once it
/// finishes, as Claude's does, so nothing says it twice; the line's chevron
/// opens it sooner.
pub(crate) fn carried_tool(blocks: &[Block]) -> Option<&ToolBlock> {
    let tool = crate::pane::running_call(blocks)?;
    (tools::policy(&tool.name).grouping == tools::Grouping::RollsUp
        && tool.output.is_none()
        && tool.progress.is_none())
    .then_some(tool)
}

/// The carried call whose row stays hidden: not one the line opened.
fn carried_call(blocks: &[Block], shape: &RowShape) -> Option<String> {
    shape.working?;
    (!shape.carried_open)
        .then(|| carried_tool(blocks))
        .flatten()
        .map(|tool| tool.call.clone())
}

/// The failed turn end the failure strip speaks for: the last turn's,
/// when it failed and the strip stands at the tail, so the failure shows
/// once (`CockpitView::failure_tail`). An earlier failed turn keeps its
/// stamp.
fn struck_failure(blocks: &[Block], shape: &RowShape) -> Option<usize> {
    shape.tail?;
    let at = blocks
        .iter()
        .rposition(|block| matches!(block.body, Body::TurnEnd(_) | Body::Prompt(_)))?;
    matches!(
        &blocks[at].body,
        Body::TurnEnd(end) if matches!(end.outcome, ferrite_core::TurnOutcome::Error(_))
    )
    .then_some(at)
}

/// The day a prompt was sent, when its stamp says (`clock::parse_local`).
fn sent_day(block: &Block) -> Option<impl PartialEq + Copy> {
    block
        .sent_at
        .as_deref()
        .and_then(ferrite_core::clock::parse_local)
        .map(|at| at.date_naive())
}

/// The calls that fold with the one at `blocks[start]`: it and every call
/// of its run (`run_of`) after it up to the first row that is not one,
/// reading across blocks that draw nothing. Returns the members' indices
/// and the index after the run.
fn group_at(blocks: &[Block], start: usize, shape: &RowShape) -> (Vec<usize>, usize) {
    let run = match &blocks[start].body {
        Body::Tool(tool) => run_of(tool),
        _ => None,
    };
    let mut members = vec![start];
    let mut end = start + 1;
    let mut index = end;
    while let Some(block) = blocks.get(index) {
        match &block.body {
            _ if draws_nothing(block, shape) => index += 1,
            Body::Tool(tool)
                if block.markdown.is_none() && run.is_some() && run_of(tool) == run =>
            {
                members.push(index);
                index += 1;
                end = index;
            }
            _ => break,
        }
    }
    (members, end)
}

fn project(blocks: &[Block], shape: &RowShape, reading: f32) -> Vec<Rc<TranscriptRow>> {
    let shape = &RowShape {
        carried: carried_call(blocks, shape),
        ..shape.clone()
    };
    let mut rows = Vec::new();
    let row = |id, blocks: &[Block], source: Option<Rc<str>>, kind| TranscriptRow {
        id,
        blocks: blocks.to_vec().into(),
        source,
        kind,
        gap: 0.,
        tail_key: None,
        outputs: Rc::from([]),
    };
    let struck = struck_failure(blocks, shape);
    let mut day = None;
    let mut index = 0;
    // Where the turn being read began: after its prompt (or the window's
    // first block, its prompt evicted).
    let mut turn_start = 0;
    while index < blocks.len() {
        let block = &blocks[index];
        // A turn's end: its cards first, under its answer.
        if let Body::TurnEnd(_) = &block.body {
            let left = outputs::of_turn(&blocks[turn_start..index]);
            if !left.is_empty() {
                let mut cards = row(RowId::Outputs(block.id), &[], None, RowKind::Outputs);
                cards.outputs = left.into();
                rows.push(cards);
            }
        }
        if block.markdown.is_some() {
            let first = block.markdown_run.unwrap_or(block.id);
            let start = index;
            let mut source = String::new();
            while let Some(markdown) = blocks.get(index).and_then(|block| block.markdown.as_ref()) {
                // A section still open when its item ended (its last, with
                // no newline of its own) closes its paragraph before the next
                // item's prose: two messages read `…hello.` and `The
                // workspace…`, never `hello.The workspace`.
                if !source.is_empty() && !source.ends_with('\n') {
                    source.push_str("\n\n");
                }
                source.push_str(markdown);
                index += 1;
            }
            let run = &blocks[start..index];
            rows.push(row(
                RowId::Markdown(first),
                run,
                Some(source.into()),
                RowKind::Answer { narration: false },
            ));
            continue;
        }
        index += 1;
        match &block.body {
            Body::Prompt(_) => {
                // A turn on another day than the one before it opens
                // under its date.
                if let Some(sent) = sent_day(block) {
                    if day.is_some_and(|day| day != sent) {
                        rows.push(row(
                            RowId::Divider(block.id),
                            std::slice::from_ref(block),
                            None,
                            RowKind::Divider,
                        ));
                    }
                    day = Some(sent);
                }
                turn_start = index;
                let first = rows.iter().all(|row| row.kind != RowKind::Prompt);
                rows.push(row(
                    RowId::Block(block.id),
                    std::slice::from_ref(block),
                    None,
                    RowKind::Prompt,
                ));
                if let Some(note) = shape.note.as_ref().filter(|_| first) {
                    rows.push(row(RowId::Note, &[], Some(note.as_ref().into()), RowKind::Note));
                }
            }
            Body::Tool(tool) => {
                if draws_nothing(block, shape) {
                    continue;
                }
                if run_of(tool).is_some() {
                    let (members, end) = group_at(blocks, index - 1, shape);
                    if members.len() > 1 {
                        let run: Vec<Block> =
                            members.iter().map(|at| blocks[*at].clone()).collect();
                        rows.push(row(RowId::Group(block.id), &run, None, RowKind::Group));
                        index = end;
                        continue;
                    }
                }
                rows.push(row(
                    RowId::Block(block.id),
                    std::slice::from_ref(block),
                    None,
                    RowKind::Activity,
                ));
            }
            // Hidden thinking; an answered request's record (`allowed
            // Bash`), whose call's own row is its account, as the approved
            // transcript draws it; where the Thread works, which the head
            // and the Panel say; and a quick turn's end.
            _ if draws_nothing(block, shape) || struck == Some(index - 1) => {}
            _ => rows.push(row(
                RowId::Block(block.id),
                std::slice::from_ref(block),
                None,
                RowKind::of(block),
            )),
        }
    }
    if let Some(key) = shape.working {
        let mut working = row(RowId::Working, &[], None, RowKind::Working);
        working.tail_key = Some(key);
        rows.push(working);
    }
    if let Some(key) = shape.tail {
        let mut tail = row(RowId::Tail, &[], None, RowKind::Tail);
        tail.tail_key = Some(key);
        rows.push(tail);
    }
    // Prose with more work after it in its turn is narration.
    let mut work_after = false;
    // Whether a later prompt or wrap-up has taken the operator past a box.
    let mut moved_on = false;
    for row in rows.iter_mut().rev() {
        match row.kind {
            RowKind::WrapUp { .. } => {
                row.kind = RowKind::WrapUp { spent: moved_on };
                moved_on = true;
            }
            RowKind::Activity | RowKind::Group | RowKind::Visual => work_after = true,
            RowKind::Prompt => {
                work_after = false;
                moved_on = true;
            }
            RowKind::TurnEnd { .. } => work_after = false,
            RowKind::Answer { .. } => {
                row.kind = RowKind::Answer {
                    narration: work_after,
                }
            }
            _ => {}
        }
    }
    let mut previous = None;
    for row in rows.iter_mut() {
        row.gap = gap_before(previous, row.kind, reading)
            + if previous.is_none() {
                shape.top_pad
            } else {
                0.
            };
        previous = Some(row.kind);
    }
    rows.into_iter().map(Rc::new).collect()
}

fn common_prefix<T: Eq>(left: &[T], right: &[T]) -> usize {
    left.iter()
        .zip(right)
        .take_while(|(left, right)| left == right)
        .count()
}

fn common_suffix_after_prefix<T: Eq>(left: &[T], right: &[T], prefix: usize) -> usize {
    left[prefix..]
        .iter()
        .rev()
        .zip(right[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count()
}

/// The splices that turn `previous` into `next`. Past their common start
/// (any rows before the first change), they align on the first previous
/// row `next` still holds: the head before it on either side is one splice
/// — history evicted from the far end, rows prepended, or a group renamed
/// for a new oldest call — and the rest a common prefix, one splice and a
/// common suffix. So the rows between keep their place in the list,
/// whatever changes at both ends in one update.
fn splices<T: Eq + Hash>(previous: &[T], next: &[T]) -> Vec<RowSplice> {
    let start = common_prefix(previous, next);
    let (previous, next) = (&previous[start..], &next[start..]);
    let at_in_next: HashMap<&T, usize> = next.iter().enumerate().map(|(at, id)| (id, at)).collect();
    // With no previous row left, the whole remainder is the head.
    let (kept, at) = previous
        .iter()
        .enumerate()
        .find_map(|(index, id)| Some((index, *at_in_next.get(id)?)))
        .unwrap_or((previous.len(), next.len()));
    let mut splices = Vec::new();
    if kept > 0 || at > 0 {
        splices.push(RowSplice {
            old_range: start..start + kept,
            new_count: at,
        });
    }
    // After the head splice, `previous[kept]` sits at `start + at`.
    let (old, new) = (&previous[kept..], &next[at..]);
    let prefix = common_prefix(old, new);
    let suffix = common_suffix_after_prefix(old, new, prefix);
    let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
    if prefix != old_end || prefix != new_end {
        let from = start + at;
        splices.push(RowSplice {
            old_range: from + prefix..from + old_end,
            new_count: new_end - prefix,
        });
    }
    splices
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::{
        transcript::{Input, Transcript},
        SessionEvent,
    };

    /// Answers at the Standard reading size.
    const READING: f32 = ferrite_ui::theme::FS_PROSE;

    fn shape() -> RowShape {
        RowShape {
            thinking: true,
            ..RowShape::default()
        }
    }

    fn text(transcript: &mut Transcript, text: &str) {
        transcript.apply(Input::Event(SessionEvent::TextDelta { text: text.into() }));
    }

    fn prompt(transcript: &mut Transcript, text: &str) {
        transcript.apply(Input::Prompt(text.into()));
    }

    fn tool(transcript: &mut Transcript, id: &str) {
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: id.into(),
            name: "Read".into(),
            input: serde_json::json!({}),
        }));
    }

    fn edit(transcript: &mut Transcript, id: &str, path: &str) {
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: id.into(),
            name: "Edit".into(),
            input: serde_json::json!({ "file_path": path }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: id.into(),
            output: String::new(),
            is_error: false,
            result: ferrite_core::ToolResult::FileEdit {
                path: path.into(),
                hunks: vec![ferrite_core::Hunk {
                    old_start: 1,
                    old_lines: 1,
                    new_start: 1,
                    new_lines: 1,
                    lines: vec!["-a".into(), "+b".into()],
                    section: None,
                }],
            },
        }));
    }

    fn bash(transcript: &mut Transcript, id: &str) {
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: id.into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo test" }),
        }));
    }

    fn kinds(transcript: &Transcript, shape: &RowShape) -> Vec<RowKind> {
        TranscriptRows::new(transcript.blocks(), shape, READING)
            .rows()
            .iter()
            .map(|row| row.kind())
            .collect()
    }

    /// While the turn runs, its working line carries the running call: its
    /// row appears once it finishes, and an agent keeps its own.
    #[test]
    fn a_running_call_waits_for_its_end_while_the_working_line_carries_it() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        bash(&mut transcript, "run");
        let working = RowShape {
            working: Some(1),
            ..shape()
        };
        assert_eq!(
            kinds(&transcript, &working),
            [RowKind::Prompt, RowKind::Working]
        );
        // With no working line (a Decision at the tail), the row shows.
        assert_eq!(
            kinds(&transcript, &shape()),
            [RowKind::Prompt, RowKind::Activity]
        );
        settle(&mut transcript, "run", false);
        assert_eq!(
            kinds(&transcript, &working),
            [RowKind::Prompt, RowKind::Activity, RowKind::Working]
        );
        call(&mut transcript, "agent", "Agent");
        assert_eq!(
            kinds(&transcript, &working),
            [
                RowKind::Prompt,
                RowKind::Activity,
                RowKind::Activity,
                RowKind::Working
            ]
        );
        // Only the call the line names: one started before it shows.
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        bash(&mut transcript, "first");
        bash(&mut transcript, "second");
        assert_eq!(
            carried_call(transcript.blocks(), &working).as_deref(),
            Some("second")
        );
        // And once the named call streams output, its row shows it.
        transcript.apply(Input::Event(SessionEvent::ToolOutputDelta {
            id: "second".into(),
            text: "compiling\n".into(),
        }));
        assert_eq!(carried_call(transcript.blocks(), &working), None);
    }

    /// A failed last turn says so once, in the failure strip at the tail;
    /// an earlier failed turn keeps its stamp.
    #[test]
    fn a_failure_shows_once_under_its_strip() {
        let mut transcript = Transcript::default();
        let fail = |transcript: &mut Transcript| {
            transcript.apply(Input::Event(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Error("API Error: 529 overloaded".into()),
                cost_usd: None,
            }));
        };
        prompt(&mut transcript, "one");
        text(&mut transcript, "Trying.");
        fail(&mut transcript);
        let strip = RowShape {
            tail: Some(1),
            ..shape()
        };
        let failed = RowKind::TurnEnd { hangs: true };
        assert!(!kinds(&transcript, &strip).contains(&failed));
        assert!(kinds(&transcript, &shape()).contains(&failed));
        prompt(&mut transcript, "two");
        text(&mut transcript, "Trying again.");
        fail(&mut transcript);
        assert_eq!(
            kinds(&transcript, &strip)
                .iter()
                .filter(|kind| **kind == failed)
                .count(),
            1
        );
    }

    /// A turn sent on another day than the one before it opens under its
    /// date; prompts with no stamp (an older log) never divide.
    #[test]
    fn a_new_day_opens_under_its_date() {
        let mut transcript = Transcript::default();
        let sent = |transcript: &mut Transcript, text: &str, stamp: &str| {
            prompt(transcript, text);
            transcript.apply(Input::PromptObservation {
                sent_at: stamp.into(),
                prompt_id: None,
            });
        };
        sent(&mut transcript, "one", "2026-11-03T10:00:00+00:00");
        sent(&mut transcript, "two", "2026-11-03T10:30:00+00:00");
        sent(&mut transcript, "three", "2026-11-05T10:00:00+00:00");
        sent(&mut transcript, "four", "7:18 pm");
        assert_eq!(
            kinds(&transcript, &shape()),
            [
                RowKind::Prompt,
                RowKind::Prompt,
                RowKind::Divider,
                RowKind::Prompt,
                RowKind::Prompt
            ]
        );
    }

    /// The model's thinking, an answered request's record and the
    /// `opened in` note leave no row of their own unless thinking is shown.
    #[test]
    fn thinking_records_and_the_opened_note_draw_no_rows_by_default() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Notice(format!(
            "{}/work/repo",
            ferrite_core::transcript::OPENED_IN
        )));
        prompt(&mut transcript, "run it");
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "The user wants the tests run.".into(),
        }));
        bash(&mut transcript, "b1");
        transcript.apply(Input::Answered {
            allowed: true,
            tool_name: "Bash".into(),
        });
        let kinds = |shape: &RowShape| -> Vec<RowKind> {
            TranscriptRows::new(transcript.blocks(), shape, READING)
                .rows()
                .iter()
                .map(|row| row.kind())
                .collect()
        };
        let hidden = kinds(&RowShape::default());
        assert!(!hidden.contains(&RowKind::Reasoning), "{hidden:?}");
        assert!(!hidden.contains(&RowKind::Notice), "{hidden:?}");
        assert!(!hidden.contains(&RowKind::Meta), "{hidden:?}");
        assert!(kinds(&shape()).contains(&RowKind::Reasoning));
    }

    #[test]
    fn contiguous_markdown_blocks_share_one_native_answer_row() {
        let mut transcript = Transcript::default();
        text(&mut transcript, "first");
        transcript.apply(Input::Event(SessionEvent::ContentBoundary));
        text(&mut transcript, "second");

        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows.get(0).unwrap().id(), RowId::Markdown(_)));
        assert_eq!(rows.get(0).unwrap().blocks().len(), 2);
        // Each item's prose closes its paragraph: `first` and `second`
        // read as two, never `firstsecond`.
        assert_eq!(rows.get(0).unwrap().source(), Some("first\n\nsecond"));
    }

    #[test]
    fn append_keeps_prior_row_rcs_and_splices_the_tail() {
        let mut transcript = Transcript::default();
        text(&mut transcript, "first");
        prompt(&mut transcript, "next");
        let mut rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        let first = rows.get(0).unwrap().clone();

        text(&mut transcript, "second");
        let delta = rows.reconcile(transcript.blocks(), &shape(), READING);
        assert!(Rc::ptr_eq(&first, rows.get(0).unwrap()));
        assert_eq!(
            delta.splices,
            vec![RowSplice {
                old_range: 2..2,
                new_count: 1
            }]
        );
    }

    #[test]
    fn front_eviction_deletes_the_head_before_appending_the_tail() {
        let mut transcript = Transcript::default();
        for text_part in ["a", "b", "c"] {
            prompt(&mut transcript, text_part);
            text(&mut transcript, text_part);
        }
        let mut rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        let old = rows.rows().to_vec();

        prompt(&mut transcript, "d");
        text(&mut transcript, "d");
        let delta = rows.reconcile(&transcript.blocks()[2..], &shape(), READING);
        assert_eq!(
            delta.splices,
            vec![
                RowSplice {
                    old_range: 0..2,
                    new_count: 0
                },
                RowSplice {
                    old_range: 4..4,
                    new_count: 2
                }
            ]
        );
        // The new head is a prompt band flush to the top in place of its
        // blank line: a changed row, re-measured, while the rest keep their
        // Rcs.
        assert!(!Rc::ptr_eq(&old[2], rows.get(0).unwrap()));
        assert_eq!(rows.get(0).unwrap().gap(), 0.);
        assert_eq!(delta.remeasure, vec![0]);
        assert!(Rc::ptr_eq(&old[3], rows.get(1).unwrap()));
    }

    /// A long transcript's oldest row is a group of calls named for its
    /// oldest one: when that call falls off the far end while a row
    /// arrives, the group takes a new name, and only it and the arrival
    /// splice. The rows between keep their place, so the list never redraws
    /// whole.
    #[test]
    fn a_renamed_head_and_an_arrival_splice_only_themselves() {
        let shape = shape();
        let mut transcript = Transcript::default();
        for id in ["r1", "r2", "r3"] {
            tool(&mut transcript, id);
        }
        for text_part in ["a", "b"] {
            prompt(&mut transcript, text_part);
            text(&mut transcript, text_part);
        }
        let mut rows = TranscriptRows::new(transcript.blocks(), &shape, READING);
        let old = rows.rows().to_vec();
        assert_eq!(old[0].kind(), RowKind::Group);

        prompt(&mut transcript, "c");
        let delta = rows.reconcile(&transcript.blocks()[1..], &shape, READING);
        assert_ne!(
            old[0].id(),
            rows.get(0).unwrap().id(),
            "the group took a new name"
        );
        assert_eq!(
            delta.splices,
            vec![
                RowSplice {
                    old_range: 0..1,
                    new_count: 1
                },
                RowSplice {
                    old_range: old.len()..old.len(),
                    new_count: 1
                }
            ]
        );
        assert!(Rc::ptr_eq(&old[1], rows.get(1).unwrap()));
    }

    #[test]
    fn prepend_preserves_the_existing_suffix_rows() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "a");
        text(&mut transcript, "a");
        prompt(&mut transcript, "b");
        text(&mut transcript, "b");

        let mut rows = TranscriptRows::new(&transcript.blocks()[2..], &shape(), READING);
        let old = rows.rows().to_vec();
        let delta = rows.reconcile(transcript.blocks(), &shape(), READING);
        assert_eq!(
            delta.splices,
            vec![RowSplice {
                old_range: 0..0,
                new_count: 2
            }]
        );
        // The old head is now a later turn's prompt: its gap grew from
        // flush to a turn's, so only it is re-measured.
        assert_eq!(rows.get(2).unwrap().gap(), after_answer(READING));
        assert_eq!(delta.remeasure, vec![2]);
        assert!(Rc::ptr_eq(&old[1], rows.get(3).unwrap()));
    }

    fn settle(transcript: &mut Transcript, id: &str, is_error: bool) {
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: id.into(),
            output: "done".into(),
            is_error,
            result: ferrite_core::ToolResult::Opaque,
        }));
    }

    /// Adjacent routine calls fold into one group row, named by its first
    /// call's block, holding every call in order.
    #[test]
    fn adjacent_routine_calls_fold_into_one_group() {
        let mut transcript = Transcript::default();
        tool(&mut transcript, "first");
        bash(&mut transcript, "second");
        tool(&mut transcript, "third");
        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        assert_eq!(rows.len(), 1);
        let group = rows.get(0).unwrap();
        assert_eq!(group.kind(), RowKind::Group);
        assert_eq!(group.id(), &RowId::Group(transcript.blocks()[0].id));
        let calls: Vec<_> = group
            .blocks()
            .iter()
            .map(|block| match &block.body {
                Body::Tool(tool) => tool.call.as_str(),
                _ => "",
            })
            .collect();
        assert_eq!(calls, ["first", "second", "third"]);
    }

    /// A lone call stays its own row; the second turns it into a group
    /// under the same first block, and a third joins it without renaming
    /// the row — a group grows in place.
    #[test]
    fn a_group_forms_at_the_second_call_and_grows_in_place() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        bash(&mut transcript, "a");
        let mut rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        let first = transcript.blocks()[1].id;
        assert_eq!(rows.get(1).unwrap().id(), &RowId::Block(first));
        bash(&mut transcript, "b");
        rows.reconcile(transcript.blocks(), &shape(), READING);
        assert_eq!(rows.get(1).unwrap().id(), &RowId::Group(first));
        bash(&mut transcript, "c");
        let delta = rows.reconcile(transcript.blocks(), &shape(), READING);
        assert!(delta.splices.is_empty(), "{delta:?}");
        assert_eq!(delta.remeasure, vec![1]);
        assert_eq!(rows.get(1).unwrap().blocks().len(), 3);
    }

    /// Visible rows split groups: prose, a prompt, a turn's end, shown
    /// thinking. Edits fold into the group beside them (`Edited 1 file, ran
    /// 2 commands`); prose with more work after it in its turn narrates.
    #[test]
    fn prose_prompts_and_shown_thinking_split_groups() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        tool(&mut transcript, "r1");
        tool(&mut transcript, "r2");
        edit(&mut transcript, "e1", "src/lib.rs");
        bash(&mut transcript, "b1");
        bash(&mut transcript, "b2");
        text(&mut transcript, "Now the tests.");
        transcript.apply(Input::Event(SessionEvent::ContentBoundary));
        bash(&mut transcript, "b3");
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "checking the output".into(),
        }));
        bash(&mut transcript, "b4");
        let kinds = |shape: &RowShape| -> Vec<RowKind> {
            TranscriptRows::new(transcript.blocks(), shape, READING)
                .rows()
                .iter()
                .map(|row| row.kind())
                .collect()
        };
        let narration = RowKind::Answer { narration: true };
        assert_eq!(
            kinds(&shape()),
            vec![
                RowKind::Prompt,
                RowKind::Group,
                narration,
                RowKind::Activity,
                RowKind::Reasoning,
                RowKind::Activity,
            ]
        );
        // Hidden thinking draws nothing, so the calls either side of it
        // read as one run.
        assert_eq!(
            kinds(&RowShape::default())[2..],
            [narration, RowKind::Group]
        );
    }

    /// The turn's last prose — no work after it — is its answer; prose
    /// before more work is narration, and a new prompt starts afresh.
    #[test]
    fn prose_before_more_work_narrates_and_the_last_prose_answers() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        text(&mut transcript, "Looking.");
        transcript.apply(Input::Event(SessionEvent::ContentBoundary));
        tool(&mut transcript, "a");
        text(&mut transcript, "Done.");
        transcript.apply(Input::Event(SessionEvent::ContentBoundary));
        prompt(&mut transcript, "again");
        text(&mut transcript, "Sure.");
        let kinds: Vec<_> = TranscriptRows::new(transcript.blocks(), &shape(), READING)
            .rows()
            .iter()
            .map(|row| row.kind())
            .collect();
        assert_eq!(
            kinds,
            vec![
                RowKind::Prompt,
                RowKind::Answer { narration: true },
                RowKind::Activity,
                RowKind::Answer { narration: false },
                RowKind::Prompt,
                RowKind::Answer { narration: false },
            ]
        );
    }

    /// An answered request's record and the gated call draw nothing, so a
    /// group reads across them; the gated call is no member while it waits.
    #[test]
    fn a_group_reads_across_rows_that_draw_nothing() {
        let mut transcript = Transcript::default();
        bash(&mut transcript, "b1");
        settle(&mut transcript, "b1", false);
        transcript.apply(Input::Answered {
            allowed: true,
            tool_name: "Bash".into(),
        });
        bash(&mut transcript, "b2");
        bash(&mut transcript, "gated");
        let pending = RowShape {
            pending_call: Some("gated".into()),
            ..RowShape::default()
        };
        let rows = TranscriptRows::new(transcript.blocks(), &pending, READING);
        assert_eq!(rows.len(), 1);
        let group = rows.get(0).unwrap();
        assert_eq!(group.kind(), RowKind::Group);
        assert_eq!(
            group.blocks().len(),
            2,
            "the gated call waits in the Decision"
        );
        // A gated call alone beside one call leaves no group.
        let mut lone = Transcript::default();
        bash(&mut lone, "b1");
        bash(&mut lone, "gated");
        let rows = TranscriptRows::new(lone.blocks(), &pending, READING);
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows.get(0).unwrap().id(), RowId::Block(_)));
    }

    /// Edits fold as other calls do (`Edited 2 files`); subagents fold
    /// apart, into their own `Ran 2 agents` group, even running: a call never
    /// leaves a group when its result lands. A lone agent keeps a row.
    #[test]
    fn edits_fold_and_subagents_fold_apart() {
        let mut transcript = Transcript::default();
        for (id, name) in [
            ("w", "Write"),
            ("e", "Edit"),
            ("t", "Task"),
            ("a", "Agent"),
            ("r", "Read"),
            ("lone", "Agent"),
        ] {
            transcript.apply(Input::Event(SessionEvent::ToolStarted {
                id: id.into(),
                name: name.into(),
                input: serde_json::json!({ "file_path": "src/lib.rs" }),
            }));
        }
        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        assert_eq!(
            calls(&rows),
            [
                (RowKind::Group, vec!["w".to_string(), "e".into()]),
                (RowKind::Group, vec!["t".into(), "a".into()]),
                (RowKind::Activity, vec!["r".into()]),
                (RowKind::Activity, vec!["lone".into()]),
            ]
        );
    }

    fn call(transcript: &mut Transcript, id: &str, name: &str) {
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: id.into(),
            name: name.into(),
            input: serde_json::json!({ "subject": "x" }),
        }));
    }

    fn calls(rows: &TranscriptRows) -> Vec<(RowKind, Vec<String>)> {
        rows.rows()
            .iter()
            .map(|row| {
                let calls = row
                    .blocks()
                    .iter()
                    .filter_map(|block| match &block.body {
                        Body::Tool(tool) => Some(tool.call.clone()),
                        _ => None,
                    })
                    .collect();
                (row.kind(), calls)
            })
            .collect()
    }

    /// A checklist call draws no row (the plan shows what it did), and a
    /// group reads across it; one that failed keeps a row of its own and
    /// ends the group before it.
    #[test]
    fn checklist_calls_draw_a_row_only_when_they_fail() {
        let mut transcript = Transcript::default();
        bash(&mut transcript, "a");
        call(&mut transcript, "t1", "TaskCreate");
        settle(&mut transcript, "t1", false);
        bash(&mut transcript, "b");
        call(&mut transcript, "t2", "TodoWrite");
        settle(&mut transcript, "t2", true);
        bash(&mut transcript, "c");
        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        let owned = |kind: RowKind, ids: &[&str]| -> (RowKind, Vec<String>) {
            (kind, ids.iter().map(|id| id.to_string()).collect())
        };
        assert_eq!(
            calls(&rows),
            [
                owned(RowKind::Group, &["a", "b"]),
                owned(RowKind::Activity, &["t2"]),
                owned(RowKind::Activity, &["c"]),
            ]
        );
    }

    /// A question put to the operator stands alone: the group before it
    /// ends, another starts after it.
    #[test]
    fn a_question_breaks_the_group_around_it() {
        let mut transcript = Transcript::default();
        bash(&mut transcript, "a");
        bash(&mut transcript, "b");
        call(&mut transcript, "q", "AskUserQuestion");
        bash(&mut transcript, "c");
        bash(&mut transcript, "d");
        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        let owned = |kind: RowKind, ids: &[&str]| -> (RowKind, Vec<String>) {
            (kind, ids.iter().map(|id| id.to_string()).collect())
        };
        assert_eq!(
            calls(&rows),
            [
                owned(RowKind::Group, &["a", "b"]),
                owned(RowKind::Activity, &["q"]),
                owned(RowKind::Group, &["c", "d"]),
            ]
        );
    }

    /// The gap between a prompt's row and what answers it: the prompt step
    /// less the actions row under its bubble.
    fn after_prompt(reading: f32) -> f32 {
        use ferrite_ui::theme;
        theme::tx_after_prompt(theme::prose_line_height(reading)) - theme::MESSAGE_ACTION
    }

    /// The gap between a finished answer and the next prompt: the turn's
    /// gap less the answer's actions row, which stands under it at rest.
    fn after_answer(reading: f32) -> f32 {
        use ferrite_ui::theme;
        theme::tx_turn_gap(theme::prose_line_height(reading))
            - theme::MESSAGE_ACTIONS_GAP
            - theme::MESSAGE_ACTION
    }

    /// Claude's rhythm (at the 21 line):
    /// a prompt stands 40 over what answers it and 56 under the turn before
    /// (its answer's actions row counted in); a turn's calls read as one
    /// list, half a line apart, its prose 22 from them; every other block
    /// stands a line and a half off; a row that answers the one above it
    /// hangs flush; a leading prompt sits flush to the top.
    #[test]
    fn the_gap_table_lists_the_work_and_spaces_the_blocks() {
        use RowKind::*;
        let line = ferrite_ui::theme::prose_line_height(READING);
        assert_eq!(line, 21.);
        let half = ferrite_ui::theme::tx_half(line);
        let block = (line * 1.5).round();
        let answer = Answer { narration: false };
        let narration = Answer { narration: true };
        let stamp = TurnEnd { hangs: false };
        let hung = TurnEnd { hangs: true };
        assert_eq!(after_answer(READING) + 32., 56.);
        assert_eq!(after_prompt(READING) + 24., 40.);
        for (previous, kind, gap) in [
            (None, Prompt, 0.),
            (None, Activity, line),
            (Some(stamp), Prompt, 56.),
            (Some(Activity), Prompt, 56.),
            (Some(answer), Prompt, after_answer(READING)),
            (Some(Prompt), narration, after_prompt(READING)),
            (Some(Prompt), Activity, after_prompt(READING)),
            (Some(Prompt), answer, after_prompt(READING)),
            (Some(Prompt), Working, after_prompt(READING)),
            (Some(Activity), Activity, half),
            (Some(Group), Activity, half),
            (Some(narration), Activity, 22.),
            (Some(Activity), narration, 22.),
            (Some(narration), Working, half),
            (Some(Visual), Group, half),
            (Some(Activity), answer, 22.),
            (Some(Activity), Reasoning, block),
            (Some(Activity), stamp, block),
            (Some(answer), hung, 0.),
            (Some(Activity), Meta, 0.),
            (Some(Activity), Tail, block),
        ] {
            assert_eq!(
                gap_before(previous, kind, READING),
                gap,
                "{previous:?} → {kind:?}"
            );
        }
    }

    #[test]
    fn the_gaps_scale_with_the_reading_size() {
        use ferrite_core::settings::ReadingSize;
        use ferrite_ui::theme::answer_text_size;
        use RowKind::*;
        let prose = Answer { narration: false };
        let call = Activity;
        for size in [
            ReadingSize::STANDARD,
            ReadingSize::nearest(14),
            ReadingSize::nearest(16),
            ReadingSize::nearest(18),
        ] {
            let reading = answer_text_size(size);
            // The type table's line for the size (`TX_LINE_RATIO`).
            let line = ferrite_ui::theme::prose_line_height(reading);
            assert_eq!(
                gap_before(Some(prose), Prompt, reading),
                after_answer(reading),
                "{size:?}"
            );
            assert_eq!(
                gap_before(Some(Prompt), prose, reading),
                after_prompt(reading),
                "{size:?}"
            );
            assert_eq!(
                gap_before(Some(call), call, reading),
                ferrite_ui::theme::tx_half(line)
            );
            assert_eq!(
                line,
                ferrite_ui::theme::answer_line_height(size),
                "{size:?}"
            );
        }
    }

    #[test]
    fn a_reading_size_change_respaces_every_row_after_the_first() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        text(&mut transcript, "done");
        prompt(&mut transcript, "again");
        let mut rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        let first = rows.get(0).unwrap().clone();
        let large =
            ferrite_ui::theme::answer_text_size(ferrite_core::settings::ReadingSize::nearest(18));
        let delta = rows.reconcile(transcript.blocks(), &shape(), large);
        assert!(delta.splices.is_empty());
        assert_eq!(delta.remeasure, vec![1, 2]);
        assert!(
            Rc::ptr_eq(&first, rows.get(0).unwrap()),
            "the body padding does not scale"
        );
        assert_eq!(rows.get(2).unwrap().gap(), after_answer(large));
    }

    #[test]
    fn a_row_is_classified_and_spaced_by_its_neighbour_at_projection() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        text(&mut transcript, "Looking first.");
        transcript.apply(Input::Event(SessionEvent::ContentBoundary));
        tool(&mut transcript, "a");
        tool(&mut transcript, "b");
        bash(&mut transcript, "t");
        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        let kinds: Vec<_> = rows.rows().iter().map(|row| row.kind()).collect();
        assert_eq!(
            kinds,
            vec![
                RowKind::Prompt,
                RowKind::Answer { narration: true },
                RowKind::Group,
            ]
        );
        let gaps: Vec<_> = rows.rows().iter().map(|row| row.gap()).collect();
        let line = ferrite_ui::theme::prose_line_height(READING);
        assert_eq!(
            gaps,
            vec![
                0.,
                after_prompt(READING),
                ferrite_ui::theme::tx_work_prose(line)
            ],
            "the narration stands off the prompt, and the work off the narration"
        );
    }

    /// CT-34/41: the first row stands on the body's top padding, the
    /// Decision closes the rows a line down, and a changed tail key is a
    /// changed row.
    #[test]
    fn the_top_padding_heads_the_rows_and_the_tail_closes_them() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        text(&mut transcript, "answer");
        let line = (ferrite_ui::theme::prose_line_height(READING) * 1.5).round();
        let solo = RowShape {
            top_pad: ferrite_ui::theme::TX_PAD_T,
            tail: Some(7),
            ..RowShape::default()
        };
        let mut rows = TranscriptRows::new(transcript.blocks(), &solo, READING);
        let ids: Vec<_> = rows.rows().iter().map(|row| row.id().clone()).collect();
        assert_eq!(ids.last(), Some(&RowId::Tail));
        assert_eq!(rows.get(0).unwrap().gap(), ferrite_ui::theme::TX_PAD_T);
        assert_eq!(rows.rows().last().unwrap().gap(), line);
        let board = RowShape {
            top_pad: 0.,
            tail: Some(8),
            ..solo.clone()
        };
        let delta = rows.reconcile(transcript.blocks(), &board, READING);
        assert!(delta.remeasure.contains(&0), "the top padding changed");
        assert!(
            delta.remeasure.contains(&(rows.len() - 1)),
            "the tail's key changed"
        );
        assert_eq!(rows.rows().last().unwrap().tail_key(), Some(8));
        // An empty Thread has only its tail.
        let empty = TranscriptRows::new(&[], &solo, READING);
        assert_eq!(empty.len(), 1);
    }

    /// CT-40: the call a pending Decision gates stays out of the rows until
    /// it is answered.
    #[test]
    fn the_gated_call_hides_while_its_decision_pends() {
        let mut transcript = Transcript::default();
        bash(&mut transcript, "call_gh");
        let pending = RowShape {
            pending_call: Some("call_gh".into()),
            ..RowShape::default()
        };
        assert!(TranscriptRows::new(transcript.blocks(), &pending, READING).is_empty());
        assert!(gated("[\"turn\",\"item_1\"]", "item_1"));
        assert!(!gated("item_10", "item_1"));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "call_gh".into(),
            output: "closed".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        }));
        assert_eq!(
            TranscriptRows::new(transcript.blocks(), &pending, READING).len(),
            1
        );
    }

    #[test]
    fn blank_thinking_is_not_a_row() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "   ".into(),
        }));
        assert!(TranscriptRows::new(transcript.blocks(), &shape(), READING).is_empty());
    }
}
