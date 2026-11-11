//! Stable, owned semantic rows for the transcript virtual list.
//!
//! The Pane supplies its retained block window. This module neither decides
//! history retention nor renders a row; it only preserves the identities a
//! `ListState` needs to reconcile changing transcript content.

use std::{collections::HashMap, hash::Hash, ops::Range, rc::Rc};

use ferrite_core::transcript::outputs::{self, Output};
use ferrite_core::transcript::{tools, Block, BlockId, Body, ToolBlock, ToolState};

use super::turns::{self, Fold};
use crate::pane::DisclosureId;

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
    /// The running turn's header (`Working for 11m 24s`), under its prompt.
    Working,
    /// A settled turn's fold (`Worked for 24m 55s ›`), or a steer's
    /// (`Continued for …`), named by the prompt it stands under.
    TurnFold(BlockId),
    /// The running turn's live row at its tail (`Running sed`, `Thinking`),
    /// or the starting line or a cut-short plan when no turn runs.
    LiveTail,
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
    /// The elbow note that a turn was interrupted or failed, hung under
    /// the row before it. A completed turn's end draws nothing: its fold
    /// says how long it worked.
    TurnEnd,
    /// An agent's visual: its call line and the page. It spaces as a
    /// diffed call does: a line above and below it, so the page never sits
    /// flush against the calls around it.
    Visual,
    /// A finished turn's page and file cards, under its answer: half a
    /// line off whatever it follows.
    Outputs,
    /// The running turn's header, under its prompt (`Working for 11m
    /// 24s`): the row its fold takes once it settles.
    Working,
    /// A settled turn's fold, or a steer's segment's, under its prompt.
    TurnFold,
    /// The live row at a running turn's tail: it spaces as the work does.
    LiveTail,
    /// The tail row (the Decision), a line under the row before it.
    Tail,
    /// A date divider between turns on different days: a turn's gap above
    /// it, a line between it and its day's first prompt.
    Divider,
    /// A context compaction's divider (T3's): never folded, and it breaks
    /// a run of work.
    Compaction,
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
            Body::TurnEnd(_) => Self::TurnEnd,
            Body::Compaction(_) => Self::Compaction,
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
/// of vertical rhythm. A message's own padding comes first, as T3's
/// `TimelineRowContent` gives it (`TX_ROW_PAD`): 16 under a prompt (its
/// actions row counted in) and under a turn's answer (its meta counted in),
/// 4 between two prompts in a row, 8 under narration. A turn's work rows
/// touch, as T3's work log does, and the run stands `WORK_LOG_PAD_B` off
/// what follows it. A turn's fold and header carry their own padding. A
/// prompt after the work stands a turn's gap (`tx_turn_gap`) under it.
/// Every other block (the stamp, the Decision) stands a line and a half
/// off. A row that answers the one above it (a decision record, an
/// interrupted or failed turn's end) sits flush under it. A first row is
/// flush to the top when it is a prompt and a line down otherwise (both
/// under the body's own top padding, `RowShape::top_pad`).
pub(crate) fn gap_before(previous: Option<RowKind>, kind: RowKind, reading: f32) -> f32 {
    use ferrite_ui::theme;
    use RowKind::*;
    let line = theme::prose_line_height(reading);
    let pad = |step| theme::tx_scaled(step, reading);
    let Some(previous) = previous else {
        return if kind == Prompt { 0. } else { line };
    };
    // T3's work log: its rows touch, and the run stands `WORK_LOG_PAD_B`
    // off what follows it.
    let log = |kind| matches!(kind, Activity | Group | Reasoning | Notice | LiveTail);
    match (previous, kind) {
        (previous, kind) if log(previous) && log(kind) => 0.,
        (_, TurnEnd | Meta) => 0.,
        (Divider, _) => line,
        (previous, Divider) => gap_before(Some(previous), Prompt, reading),
        (_, Outputs) => theme::tx_half(line),
        (Prompt, Prompt) => pad(theme::TX_ROW_PAD_JOINED),
        (Prompt, _) | (Answer { narration: false }, _) => pad(theme::TX_ROW_PAD),
        (Answer { narration: true }, _) => pad(theme::TX_ROW_PAD_BARE),
        (Compaction, _) => pad(theme::TX_ROW_PAD),
        (_, Prompt) => theme::tx_turn_gap(line),
        // A turn's header and its fold carry their own bottom padding.
        (Working | TurnFold, _) => theme::TURN_FOLD_PAD_B,
        // A visual stands half a line off the rows around it.
        (Visual, _) | (_, Visual) => theme::tx_half(line),
        (previous, _) if log(previous) => theme::WORK_LOG_PAD_B,
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
    /// The running turn's header key (`Working for …`), while a turn runs.
    pub working: Option<u64>,
    /// The live row's key at the tail: the running turn's live step, the
    /// starting line, or a cut-short plan.
    pub live: Option<u64>,
    /// The running turn's trailing work is opened (`live_anchor`): its rows
    /// show above the live row.
    pub live_open: bool,
    /// The folds opened, by the prompt each stands under
    /// (`DisclosureId::Turn`); every other fold is shut.
    pub open_folds: std::collections::BTreeSet<BlockId>,
    pub tail: Option<u64>,
    pub pending_call: Option<String>,
    /// The model's thinking drawn as `∴` rows; off, it draws nothing.
    pub thinking: bool,
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
    /// A fold row's label and hover time.
    fold: Option<Rc<Fold>>,
    /// A prompt's joins to the prompts right above and below it.
    joins: Joins,
}

/// Whether a prompt stands right under another (`above`) and right over
/// another (`below`): T3 joins two of the operator's messages in a row on
/// tighter corners.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Joins {
    pub above: bool,
    pub below: bool,
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

    /// What a fold row says; `None` for any other row.
    pub(crate) fn fold(&self) -> Option<&Fold> {
        self.fold.as_deref()
    }

    /// A prompt row's joins to its neighbours (`Joins`).
    pub(crate) fn joins(&self) -> Joins {
        self.joins
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
/// `ToolBlock::draws_row`), a completed turn's end (its fold says how long
/// it worked). A group of calls reads across it.
fn draws_nothing(block: &Block, shape: &RowShape) -> bool {
    if block.markdown.is_some() {
        return false;
    }
    match &block.body {
        Body::Thinking(text) => text.trim().is_empty() || !shape.thinking,
        Body::Meta(text) => ferrite_core::transcript::is_decision_record(text),
        Body::Notice(text) => ferrite_core::transcript::is_opened_notice(text),
        Body::Tool(tool) => is_gated(tool, shape) || !tool.draws_row(),
        Body::TurnEnd(end) => end.completed(),
        // A recap alone only updates the Thread's status.
        Body::WrapUp(wrap) => !wrap.shows_box(),
        _ => false,
    }
}

/// What one projection reads besides the shape: the blocks it draws no
/// row for this time — those a shut fold hides, and the running turn's
/// trailing work its live row stands for. A group reads across them as it
/// does across a block that draws nothing.
struct Projection<'a> {
    shape: &'a RowShape,
    skipped: std::collections::HashSet<BlockId>,
}

impl Projection<'_> {
    fn skips(&self, block: &Block) -> bool {
        self.skipped.contains(&block.id) || draws_nothing(block, self.shape)
    }
}

/// Where the running turn's trailing work starts (`turns::trailing_work`),
/// while a turn runs: `None` when none does.
fn live_start(blocks: &[Block], shape: &RowShape) -> Option<usize> {
    shape.working?;
    let from = turns::open_turn(blocks).map_or(0, |prompt| prompt + 1);
    let start = turns::trailing_work(blocks, from, |block| !draws_nothing(block, shape));
    Some(start)
}

/// What the live row opens, keying the running turn's trailing work: its
/// first call's group (`DisclosureId::Group`, the group it settles into),
/// or with no call there, its first thought's `DisclosureId::Live`.
pub(crate) fn live_anchor(blocks: &[Block], shape: &RowShape) -> Option<DisclosureId> {
    let work = &blocks[live_start(blocks, shape)?..];
    work.iter()
        .find_map(|block| match &block.body {
            Body::Tool(tool) => Some(DisclosureId::Group(tool.call.clone())),
            _ => None,
        })
        .or_else(|| {
            work.iter()
                .find(|block| matches!(block.body, Body::Thinking(_)))
                .map(|block| DisclosureId::Live(block.id))
        })
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
fn group_at(blocks: &[Block], start: usize, projection: &Projection) -> (Vec<usize>, usize) {
    let run = run_at(&blocks[start]);
    let mut members = vec![start];
    let mut end = start + 1;
    let mut index = end;
    while let Some(block) = blocks.get(index) {
        match run_at(block) {
            _ if projection.skips(block) => index += 1,
            joins if run.is_some() && joins == run => {
                members.push(index);
                index += 1;
                end = index;
            }
            _ => break,
        }
    }
    (members, end)
}

/// The run a block folds into with its neighbours (T3's work groups): a
/// call's (`run_of`) unless it failed, since a failure stands on a row of
/// its own; a shown thought, the routine calls'.
fn run_at(block: &Block) -> Option<tools::Grouping> {
    if block.markdown.is_some() {
        return None;
    }
    match &block.body {
        Body::Tool(tool) if !matches!(tool.state, ToolState::Failed(_)) => run_of(tool),
        Body::Thinking(_) => Some(tools::Grouping::RollsUp),
        _ => None,
    }
}

fn project(blocks: &[Block], shape: &RowShape, reading: f32) -> Vec<Rc<TranscriptRow>> {
    let folds: HashMap<BlockId, Fold> = turns::folds(blocks, |block| !draws_nothing(block, shape))
        .into_iter()
        .map(|fold| (fold.prompt, fold))
        .collect();
    let live = live_start(blocks, shape);
    let mut projection = Projection {
        shape,
        skipped: folds
            .values()
            .filter(|fold| !shape.open_folds.contains(&fold.prompt))
            .flat_map(|fold| fold.hidden.iter().copied())
            .collect(),
    };
    if let Some(live) = live.filter(|_| !shape.live_open) {
        projection
            .skipped
            .extend(blocks[live..].iter().map(|block| block.id));
    }
    let projection = &projection;
    // The running turn's prompt, its header under it.
    let running = shape.working.and(turns::open_turn(blocks));
    let mut rows = Vec::new();
    let row = |id, blocks: &[Block], source: Option<Rc<str>>, kind| TranscriptRow {
        id,
        blocks: blocks.to_vec().into(),
        source,
        kind,
        gap: 0.,
        tail_key: None,
        outputs: Rc::from([]),
        fold: None,
        joins: Joins::default(),
    };
    let header = |key| {
        let mut working = row(RowId::Working, &[], None, RowKind::Working);
        working.tail_key = Some(key);
        working
    };
    // The running turn's prompt was evicted: its header heads the window.
    if let (Some(key), None) = (shape.working, running) {
        rows.push(header(key));
    }
    let struck = struck_failure(blocks, shape);
    let mut day = None;
    let mut index = 0;
    // Where the turn being read began: after its prompt (or the window's
    // first block, its prompt evicted).
    let mut turn_start = 0;
    let mut in_turn = false;
    while index < blocks.len() {
        let block = &blocks[index];
        // A turn's end: its cards first, under its answer.
        if let Body::TurnEnd(_) = &block.body {
            in_turn = false;
            let left = outputs::of_turn(&blocks[turn_start..index]);
            if !left.is_empty() {
                let mut cards = row(RowId::Outputs(block.id), &[], None, RowKind::Outputs);
                cards.outputs = left.into();
                rows.push(cards);
            }
        }
        if projection.skipped.contains(&block.id) {
            index += 1;
            continue;
        }
        if block.markdown.is_some() {
            let first = block.markdown_run.unwrap_or(block.id);
            let start = index;
            let mut source = String::new();
            while let Some(markdown) = blocks
                .get(index)
                .filter(|block| !projection.skipped.contains(&block.id))
                .and_then(|block| block.markdown.as_ref())
            {
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
                // A steer goes on with the turn it folds into.
                if !in_turn {
                    turn_start = index;
                }
                in_turn = true;
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
                if let Some(fold) = folds.get(&block.id) {
                    let mut face = row(RowId::TurnFold(block.id), &[], None, RowKind::TurnFold);
                    face.fold = Some(Rc::new(Fold {
                        hidden: Vec::new(),
                        ..fold.clone()
                    }));
                    rows.push(face);
                }
                if let (Some(key), true) = (shape.working, running == Some(index - 1)) {
                    rows.push(header(key));
                }
            }
            Body::Tool(_) | Body::Thinking(_) if !projection.skips(block) => {
                if run_at(block).is_some() {
                    let (members, end) = group_at(blocks, index - 1, projection);
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
                    RowKind::of(block),
                ));
            }
            // Hidden thinking; an answered request's record (`allowed
            // Bash`), whose call's own row is its account, as the approved
            // transcript draws it; where the Thread works, which the head
            // and the Panel say; and a completed turn's end.
            _ if projection.skips(block) || struck == Some(index - 1) => {}
            _ => rows.push(row(
                RowId::Block(block.id),
                std::slice::from_ref(block),
                None,
                RowKind::of(block),
            )),
        }
    }
    if let Some(key) = shape.live {
        let work = live.map_or(&[][..], |live| &blocks[live..]);
        let mut tail = row(RowId::LiveTail, work, None, RowKind::LiveTail);
        tail.tail_key = Some(key);
        rows.push(tail);
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
            RowKind::TurnEnd => work_after = false,
            RowKind::Answer { .. } => {
                row.kind = RowKind::Answer {
                    narration: work_after,
                }
            }
            _ => {}
        }
    }
    for at in 1..rows.len() {
        if rows[at - 1].kind == RowKind::Prompt && rows[at].kind == RowKind::Prompt {
            rows[at - 1].joins.below = true;
            rows[at].joins.above = true;
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

    /// The running turn's shape: its header under its prompt, its live row
    /// at its tail.
    fn running() -> RowShape {
        RowShape {
            working: Some(1),
            live: Some(2),
            ..shape()
        }
    }

    /// While a turn runs, its header stands under its prompt and its
    /// trailing work is one live row at its tail (the running call is no
    /// longer hidden: the live row is it). Opened, the work shows above the
    /// live row; prose ends the trailing work, and an agent keeps its own
    /// row.
    #[test]
    fn a_running_turn_heads_with_its_header_and_ends_on_its_live_row() {
        use RowKind::*;
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        bash(&mut transcript, "first");
        settle(&mut transcript, "first", false);
        bash(&mut transcript, "run");
        let rows = TranscriptRows::new(transcript.blocks(), &running(), READING);
        let ids: Vec<_> = rows.rows().iter().map(|row| row.id().clone()).collect();
        assert_eq!(ids[1], RowId::Working);
        assert_eq!(ids[2], RowId::LiveTail);
        assert_eq!(kinds(&transcript, &running()), [Prompt, Working, LiveTail]);
        assert_eq!(
            calls(&rows)[2],
            (LiveTail, vec!["first".to_string(), "run".into()]),
            "the live row stands for the trailing work"
        );
        assert_eq!(
            live_anchor(transcript.blocks(), &running()),
            Some(DisclosureId::Group("first".into()))
        );
        let open = RowShape {
            live_open: true,
            ..running()
        };
        assert_eq!(kinds(&transcript, &open), [Prompt, Working, Group, LiveTail]);
        // With no turn running (a Session starting), the work shows.
        let idle = RowShape {
            live: Some(2),
            ..shape()
        };
        assert_eq!(kinds(&transcript, &idle), [Prompt, Group, LiveTail]);
        text(&mut transcript, "Looking.");
        transcript.apply(Input::Event(SessionEvent::ContentBoundary));
        call(&mut transcript, "agent", "Agent");
        let rows = TranscriptRows::new(transcript.blocks(), &running(), READING);
        assert_eq!(
            calls(&rows),
            [
                (Prompt, vec![]),
                (Working, vec![]),
                (Group, vec!["first".to_string(), "run".into()]),
                (Answer { narration: true }, vec![]),
                (Activity, vec!["agent".to_string()]),
                (LiveTail, vec![]),
            ]
        );
    }

    /// The live row's reach: a suite counting its tests keeps its own row
    /// (its bar in view), an earlier turn's work is not the running
    /// turn's, and thinking alone still opens (by its first thought).
    #[test]
    fn the_live_row_stops_at_a_counting_suite_and_an_earlier_turn() {
        use RowKind::*;
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        bash(&mut transcript, "suite");
        transcript.apply(Input::Event(SessionEvent::ToolOutputDelta {
            id: "suite".into(),
            text: "running 4 tests\ntest a ... ok\n".into(),
        }));
        assert_eq!(kinds(&transcript, &running()), [Prompt, Working, Activity, LiveTail]);

        // The running turn's prompt gone from the window, an earlier turn
        // still in it: the live row reaches back to that turn's end, no
        // further.
        let mut earlier = Transcript::default();
        prompt(&mut earlier, "one");
        bash(&mut earlier, "a");
        settle(&mut earlier, "a", false);
        complete(&mut earlier, 2_000);
        prompt(&mut earlier, "two");
        earlier.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "next".into(),
        }));
        let window: Vec<Block> = earlier
            .blocks()
            .iter()
            .filter(|block| !matches!(&block.body, Body::Prompt(text) if text == "two"))
            .cloned()
            .collect();
        let shape = running();
        let rows = TranscriptRows::new(&window, &shape, READING);
        assert_eq!(
            calls(&rows),
            [
                (Working, vec![]),
                (Prompt, vec![]),
                (TurnFold, vec![]),
                (LiveTail, vec![]),
            ]
        );
        let thought = window
            .iter()
            .find(|block| matches!(block.body, Body::Thinking(_)))
            .unwrap()
            .id;
        assert_eq!(
            live_anchor(&window, &shape),
            Some(DisclosureId::Live(thought))
        );
    }

    /// The turn's end as the engine records it: its result, then the time
    /// it took, observed live and kept for replay.
    fn complete(transcript: &mut Transcript, ms: u64) {
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        }));
        transcript.apply(Input::CompletionObservation {
            elapsed_ms: ms,
            completed_at: "2026-11-09T20:01:30+00:00".into(),
            input_tokens: None,
            output_tokens: None,
        });
    }

    fn folds(transcript: &Transcript, shape: &RowShape) -> Vec<String> {
        TranscriptRows::new(transcript.blocks(), shape, READING)
            .rows()
            .iter()
            .filter_map(|row| row.fold().map(|fold| fold.label.clone()))
            .collect()
    }

    /// A settled turn's header becomes its fold, shut: every call, thought
    /// and interim message folds away under it, its final answer stays.
    /// Opened, the work stands in place under it, in its order.
    #[test]
    fn a_settled_turn_folds_its_work_under_its_header() {
        use RowKind::*;
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        bash(&mut transcript, "a");
        settle(&mut transcript, "a", false);
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "checking".into(),
        }));
        text(&mut transcript, "Looking.");
        transcript.apply(Input::Event(SessionEvent::ContentBoundary));
        tool(&mut transcript, "r");
        settle(&mut transcript, "r", false);
        text(&mut transcript, "Done.");
        complete(&mut transcript, 2_000);
        assert_eq!(kinds(&transcript, &shape()), [Prompt, TurnFold, Answer { narration: false }]);
        assert_eq!(folds(&transcript, &shape()), ["Worked for 2s"]);
        let prompt = transcript.blocks()[0].id;
        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        assert_eq!(rows.get(1).unwrap().id(), &RowId::TurnFold(prompt));
        let open = RowShape {
            open_folds: [prompt].into(),
            ..shape()
        };
        assert_eq!(
            kinds(&transcript, &open),
            [
                Prompt,
                TurnFold,
                Group,
                Answer { narration: true },
                Activity,
                Answer { narration: false },
            ]
        );
        // The answer stands the fold's own padding under it.
        let gaps: Vec<_> = rows.rows().iter().map(|row| row.gap()).collect();
        assert_eq!(gaps[2], ferrite_ui::theme::TURN_FOLD_PAD_B);
    }

    /// A compaction breaks the run of work it falls in and stands in view
    /// under its turn's shut fold.
    #[test]
    fn a_compaction_breaks_the_work_and_stays_out_of_the_fold() {
        use RowKind::*;
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        bash(&mut transcript, "a");
        settle(&mut transcript, "a", false);
        transcript.apply(Input::Event(SessionEvent::Progress {
            event: ferrite_core::progress::ProgressEvent::Phase {
                phase: ferrite_core::progress::Phase::Compacting,
                detail: String::new(),
            },
        }));
        bash(&mut transcript, "b");
        settle(&mut transcript, "b", false);
        text(&mut transcript, "Done.");
        complete(&mut transcript, 2_000);
        assert_eq!(
            kinds(&transcript, &shape()),
            [Prompt, TurnFold, Compaction, Answer { narration: false }]
        );
        let prompt = transcript.blocks()[0].id;
        let open = RowShape {
            open_folds: [prompt].into(),
            ..shape()
        };
        assert_eq!(
            kinds(&transcript, &open),
            [Prompt, TurnFold, Activity, Compaction, Activity, Answer { narration: false }]
        );
    }

    /// Any turn folds, however quick (no floor), timed as T3 prints it.
    #[test]
    fn a_quick_turn_folds_too() {
        for (ms, label) in [(2_000, "Worked for 2s"), (4_100, "Worked for 4.1s"), (1_495_000, "Worked for 24m 55s")] {
            let mut transcript = Transcript::default();
            prompt(&mut transcript, "go");
            bash(&mut transcript, "a");
            settle(&mut transcript, "a", false);
            text(&mut transcript, "Done.");
            complete(&mut transcript, ms);
            assert_eq!(folds(&transcript, &shape()), [label]);
        }
    }

    /// No fold where nothing would fold (an answer alone), where the turn
    /// failed (its every step stays in view), or where its end was never
    /// written (an older log): its time is not known.
    #[test]
    fn a_turn_with_nothing_to_fold_failed_or_unended_has_no_fold() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "hi");
        text(&mut transcript, "Hello.");
        complete(&mut transcript, 2_000);
        assert!(folds(&transcript, &shape()).is_empty());

        let mut failed = Transcript::default();
        prompt(&mut failed, "go");
        bash(&mut failed, "a");
        settle(&mut failed, "a", false);
        failed.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Error("overloaded".into()),
            cost_usd: None,
        }));
        assert!(folds(&failed, &shape()).is_empty());
        assert!(kinds(&failed, &shape()).contains(&RowKind::Activity));

        let mut unended = Transcript::default();
        prompt(&mut unended, "go");
        bash(&mut unended, "a");
        settle(&mut unended, "a", false);
        unended.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        }));
        prompt(&mut unended, "again");
        assert!(folds(&unended, &shape()).is_empty());
    }

    /// What must stay in view stays, shut: a notice, a question put to the
    /// operator, work still running past the turn (a background call), and
    /// a call after the answer that failed; one after it that succeeded
    /// folds.
    #[test]
    fn notices_questions_live_work_and_late_failures_stay_in_view() {
        use RowKind::*;
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        bash(&mut transcript, "a");
        settle(&mut transcript, "a", false);
        transcript.apply(Input::Notice("Model switched".into()));
        call(&mut transcript, "q", "AskUserQuestion");
        settle(&mut transcript, "q", false);
        bash(&mut transcript, "bg");
        text(&mut transcript, "Done.");
        transcript.apply(Input::Event(SessionEvent::ContentBoundary));
        bash(&mut transcript, "late-ok");
        settle(&mut transcript, "late-ok", false);
        bash(&mut transcript, "late-bad");
        settle(&mut transcript, "late-bad", true);
        let running_bg = transcript
            .blocks()
            .iter()
            .any(|block| matches!(&block.body, Body::Tool(tool) if tool.call == "bg"));
        assert!(running_bg);
        complete(&mut transcript, 9_000);
        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        let shown: Vec<_> = calls(&rows)
            .into_iter()
            .map(|(kind, calls)| (kind, calls.join(",")))
            .collect();
        // The turn's end retired the foreground call `bg`: it settles to a
        // row the fold hides unless it ran in the background.
        assert!(shown.contains(&(Notice, String::new())), "{shown:?}");
        assert!(shown.contains(&(Activity, "q".into())), "{shown:?}");
        assert!(shown.contains(&(Activity, "late-bad".into())), "{shown:?}");
        assert!(!shown.iter().any(|(_, calls)| calls.contains("late-ok")), "{shown:?}");
        assert!(!shown.iter().any(|(_, calls)| calls == "a"), "{shown:?}");
        assert_eq!(shown[1].0, TurnFold);
    }

    /// A steer splits its turn's work: the top fold keeps the whole turn's
    /// time and folds the work before the steer; the work after it folds
    /// under a fold of its own under the steer's prompt, each opening on
    /// its own.
    #[test]
    fn a_steer_folds_its_own_segment() {
        use RowKind::*;
        let mut transcript = Transcript::default();
        let sent = |transcript: &mut Transcript, text: &str, stamp: &str| {
            prompt(transcript, text);
            transcript.apply(Input::PromptObservation {
                sent_at: stamp.into(),
                prompt_id: None,
            });
        };
        sent(&mut transcript, "go", "2026-11-09T20:00:00+00:00");
        bash(&mut transcript, "a");
        settle(&mut transcript, "a", false);
        sent(&mut transcript, "and the docs", "2026-11-09T20:01:00+00:00");
        bash(&mut transcript, "b");
        settle(&mut transcript, "b", false);
        text(&mut transcript, "Done.");
        complete(&mut transcript, 90_000);
        assert_eq!(
            kinds(&transcript, &shape()),
            [Prompt, TurnFold, Prompt, TurnFold, Answer { narration: false }]
        );
        assert_eq!(folds(&transcript, &shape()), ["Worked for 1m 30s", "Continued for 30s"]);
        let steer = transcript.blocks()[2].id;
        let open = RowShape {
            open_folds: [steer].into(),
            ..shape()
        };
        assert_eq!(
            kinds(&transcript, &open),
            [Prompt, TurnFold, Prompt, TurnFold, Activity, Answer { narration: false }]
        );
    }

    /// The latest turn the operator stopped says so on its fold; an
    /// earlier one reads as worked.
    #[test]
    fn the_latest_stopped_turn_says_you_stopped() {
        let mut transcript = Transcript::default();
        let stop = |transcript: &mut Transcript, ms: u64| {
            transcript.apply(Input::Event(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Interrupted,
                cost_usd: None,
            }));
            transcript.apply(Input::CompletionObservation {
                elapsed_ms: ms,
                completed_at: "2026-11-09T20:01:30+00:00".into(),
                input_tokens: None,
                output_tokens: None,
            });
        };
        prompt(&mut transcript, "go");
        bash(&mut transcript, "a");
        stop(&mut transcript, 4_100);
        assert_eq!(folds(&transcript, &shape()), ["You stopped after 4.1s"]);
        prompt(&mut transcript, "again");
        bash(&mut transcript, "b");
        stop(&mut transcript, 3_000);
        assert_eq!(
            folds(&transcript, &shape()),
            ["Worked for 4.1s", "You stopped after 3s"]
        );
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
        let failed = RowKind::TurnEnd;
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
        // Shown, the thought joins the call after it in one group.
        assert!(kinds(&shape()).contains(&RowKind::Group));
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

    /// Visible rows split groups: prose, a prompt, a turn's end. A shown
    /// thought joins the group beside it (T3's work log), as edits do;
    /// prose with more work after it in its turn narrates.
    #[test]
    fn prose_and_prompts_split_groups_and_thoughts_join_them() {
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
                RowKind::Group,
            ]
        );
        // Hidden thinking draws nothing, so the calls either side of it
        // read as one run.
        assert_eq!(
            kinds(&RowShape::default())[2..],
            [narration, RowKind::Group]
        );
    }

    /// A failed call stands as its own row (T3's), so the routine calls
    /// either side of it read as two runs.
    #[test]
    fn a_failed_call_breaks_its_group() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        for id in ["a", "b"] {
            tool(&mut transcript, id);
            settle(&mut transcript, id, false);
        }
        bash(&mut transcript, "bad");
        settle(&mut transcript, "bad", true);
        for id in ["c", "d"] {
            tool(&mut transcript, id);
            settle(&mut transcript, id, false);
        }
        assert_eq!(
            kinds(&transcript, &shape()),
            [RowKind::Prompt, RowKind::Group, RowKind::Activity, RowKind::Group]
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

    /// T3's padding under a prompt and under a turn's answer.
    fn after_prompt(reading: f32) -> f32 {
        ferrite_ui::theme::tx_scaled(16., reading)
    }

    fn after_answer(reading: f32) -> f32 {
        ferrite_ui::theme::tx_scaled(16., reading)
    }

    /// T3's rhythm (at the Standard 23 line): 16 under a prompt and under
    /// a turn's answer, 4 between two prompts, 8 under narration; a turn's
    /// work rows touch and stand `WORK_LOG_PAD_B` off what follows them;
    /// a prompt after the work stands 56 under it; every other block
    /// stands a line and a half off; a row that answers the one above it
    /// hangs flush; a leading prompt sits flush to the top.
    #[test]
    fn the_gap_table_lists_the_work_and_spaces_the_blocks() {
        use RowKind::*;
        let line = ferrite_ui::theme::prose_line_height(READING);
        assert_eq!(line, 23.);
        let half = ferrite_ui::theme::tx_half(line);
        let block = (line * 1.5).round();
        let answer = Answer { narration: false };
        let narration = Answer { narration: true };
        let hung = TurnEnd;
        let pad_b = ferrite_ui::theme::WORK_LOG_PAD_B;
        for (previous, kind, gap) in [
            (None, Prompt, 0.),
            (None, Activity, line),
            (Some(TurnFold), Prompt, 56.),
            (Some(Prompt), TurnFold, 16.),
            (Some(Prompt), Prompt, 4.),
            (Some(TurnFold), answer, ferrite_ui::theme::TURN_FOLD_PAD_B),
            (Some(Working), Group, ferrite_ui::theme::TURN_FOLD_PAD_B),
            (Some(Activity), LiveTail, 0.),
            (Some(Activity), Prompt, 56.),
            (Some(answer), Prompt, 16.),
            (Some(Prompt), narration, 16.),
            (Some(Prompt), Activity, 16.),
            (Some(Prompt), answer, 16.),
            (Some(Prompt), Working, 16.),
            (Some(Activity), Activity, 0.),
            (Some(Group), Activity, 0.),
            (Some(narration), Activity, 8.),
            (Some(Activity), narration, pad_b),
            (Some(narration), LiveTail, 8.),
            (Some(Visual), Group, half),
            (Some(Activity), answer, pad_b),
            (Some(Activity), Reasoning, 0.),
            (Some(answer), hung, 0.),
            (Some(Activity), Meta, 0.),
            (Some(Activity), Tail, pad_b),
            (Some(answer), Tail, 16.),
            (Some(Meta), Tail, block),
        ] {
            assert_eq!(
                gap_before(previous, kind, READING),
                gap,
                "{previous:?} -> {kind:?}"
            );
        }
    }

    /// T3 joins two of the operator's prompts in a row: 4 apart, the
    /// upper's foot and the lower's head on tighter corners.
    #[test]
    fn two_prompts_in_a_row_join_four_apart() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "one");
        prompt(&mut transcript, "two");
        text(&mut transcript, "done");
        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        let prompts: Vec<_> = rows
            .rows()
            .iter()
            .filter(|row| row.kind() == RowKind::Prompt)
            .map(|row| (row.joins(), row.gap()))
            .collect();
        assert_eq!(
            prompts,
            vec![
                (Joins { above: false, below: true }, 0.),
                (Joins { above: true, below: false }, 4.),
            ]
        );
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
            assert_eq!(gap_before(Some(call), call, reading), 0.);
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
        assert_eq!(
            gaps,
            vec![0., after_prompt(READING), 8.],
            "the narration stands off the prompt, and the work off the narration"
        );
    }

    /// CT-34/41: the first row stands on the body's top padding, the
    /// Decision closes the rows under the answer's padding, and a changed
    /// tail key is a changed row.
    #[test]
    fn the_top_padding_heads_the_rows_and_the_tail_closes_them() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        text(&mut transcript, "answer");
        let line = after_answer(READING);
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
