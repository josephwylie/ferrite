//! Stable, owned semantic rows for the transcript virtual list.
//!
//! The Pane supplies its retained block window. This module neither decides
//! history retention nor renders a row; it only preserves the identities a
//! `ListState` needs to reconcile changing transcript content.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    rc::Rc,
};

use ferrite_core::transcript::{Block, BlockId, Body, ToolState};

/// A stable semantic-row identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RowId {
    /// The Thread's banner, the first row of every transcript.
    Banner,
    /// An adjacent native Markdown answer, named by its original run.
    Markdown(BlockId),
    /// One transcript block: a prompt, a tool call, a note.
    Block(BlockId),
    /// What a package appends after the last row (the pending Decision).
    Tail,
}

/// What a row is, for the gap table: derived once in `project`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowKind {
    Banner,
    Prompt,
    /// An agent answer. `commentary` is a lone paragraph, the kind that
    /// introduces the work under it.
    Answer {
        commentary: bool,
    },
    /// One tool call. `diff_shown`: its diff is drawn under it, and the
    /// call after it sits a line below.
    Activity {
        diff_shown: bool,
    },
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
    /// The tail row (the Decision), a line under the row before it.
    Tail,
    /// A fallback prose or code block.
    Other,
}

impl RowKind {
    fn of(block: &Block) -> Self {
        match &block.body {
            Body::Prompt(_) => Self::Prompt,
            Body::Tool(tool) => Self::Activity {
                diff_shown: !tool.diffs.is_empty(),
            },
            Body::Thinking(_) => Self::Reasoning,
            Body::Notice(_) => Self::Notice,
            Body::Meta(_) => Self::Meta,
            Body::TurnEnd(end) => Self::TurnEnd {
                hangs: !matches!(end.outcome, ferrite_core::TurnOutcome::Completed),
            },
            Body::Paragraph { .. }
            | Body::Heading { .. }
            | Body::Bullet { .. }
            | Body::Code { .. } => Self::Other,
        }
    }
}

/// The space above a row, from the row before it (`None`: the first row),
/// its own kind and the answer size `reading`: the transcript's one table
/// of vertical rhythm (the transcript grammar in `theme.rs`). Blocks are
/// one blank line apart, as a terminal prints them: the banner, a turn's
/// prompt band, prose, a run of tool calls, the stamp, the Decision. The
/// calls of one run sit flush — except that a call after a drawn diff sits a
/// line below it — and a row that hangs on an elbow under the row it answers
/// (a decision record, an interrupted or failed turn's end) sits flush under
/// it. A first row is flush to the top when it is a prompt band and a line
/// down otherwise (the banner, first whenever there is one, takes the
/// body's own top padding instead).
pub(crate) fn gap_before(previous: Option<RowKind>, kind: RowKind, reading: f32) -> f32 {
    use RowKind::*;
    let line = crate::theme::prose_line_height(reading);
    let Some(previous) = previous else {
        return if kind == Prompt { 0. } else { line };
    };
    match (previous, kind) {
        (_, Tail) => line,
        (Activity { diff_shown: true }, Activity { .. }) => line,
        (_, TurnEnd { hangs: true } | Meta) | (Activity { .. }, Activity { .. }) => 0.,
        _ => line,
    }
}

/// What a projection reads besides the blocks: the banner and its top
/// padding, the tail row's key, the call a pending Decision gates (hidden
/// while it waits), and the later edits whose folded diff the operator
/// opened.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RowShape {
    pub banner: bool,
    pub banner_pad: f32,
    pub tail: Option<u64>,
    pub pending_call: Option<String>,
    /// The model's thinking drawn as `∴` rows; off, it draws nothing.
    pub thinking: bool,
    pub opened_diffs: HashSet<String>,
}

/// One renderable transcript unit. Its blocks are owned so a list callback
/// does not borrow the transient slice passed to [`TranscriptRows::reconcile`].
///
/// Everything a row draws is in its equality — its gap and its diff fold
/// included — so a row whose neighbour changed its spacing is a changed row,
/// re-measured by reconcile and never per frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptRow {
    id: RowId,
    blocks: Rc<[Block]>,
    source: Option<Rc<str>>,
    kind: RowKind,
    gap: f32,
    /// The answer wears the Ferrite mark (`AnswerMarks`): it is the first
    /// prose since a prompt or a tool row.
    answer_mark: bool,
    /// A later edit to a file already diffed in its turn: its diff starts
    /// folded behind `+ show diff`.
    diff_folds: bool,
    /// The tail row's key: a change re-renders it.
    tail_key: Option<u64>,
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

    /// This answer wears the Ferrite mark (`AnswerMarks`).
    pub(crate) fn answer_mark(&self) -> bool {
        self.answer_mark
    }

    /// The call's diff folds behind `+ show diff` (a later edit to a file
    /// already diffed in its turn).
    pub(crate) fn diff_folds(&self) -> bool {
        self.diff_folds
    }

    /// The call's diff is drawn.
    pub(crate) fn diff_shown(&self) -> bool {
        matches!(self.kind, RowKind::Activity { diff_shown: true })
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
}

impl TranscriptRows {
    /// Project the caller-owned retained window into semantic rows, spaced
    /// for answers at `reading` px.
    pub(crate) fn new(blocks: &[Block], shape: &RowShape, reading: f32) -> Self {
        let rows = project(blocks, shape, reading);
        Self { rows: rows.into() }
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
        self.rows = next.into();
        delta
    }
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
        let mut splices = Vec::new();

        let prefix = common_prefix(&previous_ids, &next_ids);
        if prefix == 0 {
            // History eviction commonly turns `[a, b, c]` into `[b, c, d]`.
            // Delete the vanished head first, preserving the logical anchor
            // within `b`/`c`, then reconcile the remaining tail.
            let overlap = suffix_prefix_overlap(&previous_ids, &next_ids);
            if overlap > 0 {
                let removed = previous_ids.len() - overlap;
                if removed > 0 {
                    splices.push(RowSplice {
                        old_range: 0..removed,
                        new_count: 0,
                    });
                }
                if overlap < next_ids.len() {
                    splices.push(RowSplice {
                        old_range: overlap..overlap,
                        new_count: next_ids.len() - overlap,
                    });
                }
            } else {
                let suffix = common_suffix_after_prefix(&previous_ids, &next_ids, 0);
                if suffix > 0 {
                    splices.push(RowSplice {
                        old_range: 0..previous_ids.len() - suffix,
                        new_count: next_ids.len() - suffix,
                    });
                } else if previous_ids != next_ids {
                    splices.push(RowSplice {
                        old_range: 0..previous_ids.len(),
                        new_count: next_ids.len(),
                    });
                }
            }
        } else {
            let suffix = common_suffix_after_prefix(&previous_ids, &next_ids, prefix);
            let old_end = previous_ids.len() - suffix;
            let new_end = next_ids.len() - suffix;
            if prefix != old_end || prefix != new_end {
                splices.push(RowSplice {
                    old_range: prefix..old_end,
                    new_count: new_end - prefix,
                });
            }
        }

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
pub(crate) fn gated(call: &str, pending: &str) -> bool {
    call == pending || call.ends_with(&format!("\"{pending}\"]"))
}

fn project(blocks: &[Block], shape: &RowShape, reading: f32) -> Vec<Rc<TranscriptRow>> {
    let mut rows = Vec::new();
    let row = |id, blocks: &[Block], source: Option<Rc<str>>, kind| TranscriptRow {
        id,
        blocks: blocks.to_vec().into(),
        source,
        kind,
        gap: 0.,
        answer_mark: false,
        diff_folds: false,
        tail_key: None,
    };
    if shape.banner {
        rows.push(row(RowId::Banner, &[], None, RowKind::Banner));
    }
    // A file's first diff in a turn is drawn; a later edit to it in the same
    // turn folds behind `+ show diff` until the operator opens it.
    let mut diffed: HashSet<&str> = HashSet::new();
    let mut index = 0;
    while index < blocks.len() {
        let block = &blocks[index];
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
            let commentary = run.len() == 1 && matches!(&run[0].body, Body::Paragraph { .. });
            rows.push(row(
                RowId::Markdown(first),
                run,
                Some(source.into()),
                RowKind::Answer { commentary },
            ));
            continue;
        }
        index += 1;
        match &block.body {
            Body::Prompt(_) => {
                diffed.clear();
                rows.push(row(
                    RowId::Block(block.id),
                    std::slice::from_ref(block),
                    None,
                    RowKind::Prompt,
                ));
            }
            Body::Tool(tool) => {
                // The call a pending Decision gates waits in the Decision
                // itself; it shows once answered.
                if tool.state == ToolState::Running
                    && shape
                        .pending_call
                        .as_deref()
                        .is_some_and(|pending| gated(&tool.call, pending))
                {
                    continue;
                }
                let mut entry = row(
                    RowId::Block(block.id),
                    std::slice::from_ref(block),
                    None,
                    RowKind::of(block),
                );
                if !tool.diffs.is_empty() {
                    let seen = tool
                        .diffs
                        .iter()
                        .any(|diff| diffed.contains(diff.path.as_str()));
                    for diff in &tool.diffs {
                        diffed.insert(diff.path.as_str());
                    }
                    entry.diff_folds = seen;
                    entry.kind = RowKind::Activity {
                        diff_shown: !seen || shape.opened_diffs.contains(&tool.call),
                    };
                }
                rows.push(entry);
            }
            Body::Thinking(text) if text.trim().is_empty() || !shape.thinking => {}
            // An answered request's record (`allowed Bash`): the call's own
            // row is its account, as the approved transcript draws it.
            Body::Meta(text) if ferrite_core::transcript::is_decision_record(text) => {}
            // Where the Thread works: the banner says it.
            Body::Notice(text) if ferrite_core::transcript::is_opened_notice(text) => {}
            _ => rows.push(row(
                RowId::Block(block.id),
                std::slice::from_ref(block),
                None,
                RowKind::of(block),
            )),
        }
    }
    if let Some(key) = shape.tail {
        let mut tail = row(RowId::Tail, &[], None, RowKind::Tail);
        tail.tail_key = Some(key);
        rows.push(tail);
    }
    let mut previous = None;
    let mut marks = crate::transcript::AnswerMarks::default();
    for row in rows.iter_mut() {
        row.gap = if row.kind == RowKind::Banner {
            shape.banner_pad
        } else {
            gap_before(previous, row.kind, reading)
        };
        row.answer_mark = marks.next(row.speaker());
        previous = Some(row.kind);
    }
    rows.into_iter().map(Rc::new).collect()
}

impl TranscriptRow {
    /// Who the row speaks for: an answer is the agent, a tool row a
    /// machine; a lone block its own body's speaker. The banner and the
    /// tail speak for no one.
    fn speaker(&self) -> Option<crate::transcript::Speaker> {
        use crate::transcript::Speaker;
        match self.kind {
            RowKind::Answer { .. } => Some(Speaker::Agent),
            RowKind::Activity { .. } => Some(Speaker::Other),
            RowKind::Banner | RowKind::Tail => None,
            _ => self
                .blocks
                .first()
                .and_then(|block| Speaker::of(&block.body)),
        }
    }
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

fn suffix_prefix_overlap<T: Eq>(left: &[T], right: &[T]) -> usize {
    (1..=left.len().min(right.len()))
        .rev()
        .find(|length| left[left.len() - length..] == right[..*length])
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::{
        transcript::{Input, Transcript},
        SessionEvent,
    };

    /// Answers at the Standard reading size.
    const READING: f32 = crate::theme::FS_PROSE;

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
        // flush to one blank line, so only it is re-measured.
        assert_eq!(rows.get(2).unwrap().gap(), crate::theme::LH_PROSE);
        assert_eq!(delta.remeasure, vec![2]);
        assert!(Rc::ptr_eq(&old[1], rows.get(3).unwrap()));
    }

    /// CT-10: adjacent calls are separate rows, each under its own block id;
    /// nothing folds them into a summary.
    #[test]
    fn adjacent_tools_are_separate_rows() {
        let mut transcript = Transcript::default();
        tool(&mut transcript, "first");
        tool(&mut transcript, "second");
        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        assert_eq!(rows.len(), 2);
        assert!(rows
            .rows()
            .iter()
            .all(|row| matches!(row.id(), RowId::Block(_))));
        assert_eq!(rows.get(1).unwrap().gap(), 0., "calls of one run sit flush");
    }

    /// Terminal-native rhythm: blocks one blank line apart, the calls of a
    /// run and the rows hung on an elbow flush, a call after a drawn diff a
    /// line down, a leading prompt band flush to the top.
    #[test]
    fn the_gap_table_spaces_blocks_a_line_apart_and_runs_flush() {
        use RowKind::*;
        let line = crate::theme::LH_PROSE;
        let prose = Answer { commentary: false };
        let commentary = Answer { commentary: true };
        let stamp = TurnEnd { hangs: false };
        let hung = TurnEnd { hangs: true };
        let call = Activity { diff_shown: false };
        let diffed = Activity { diff_shown: true };
        for (previous, kind, gap) in [
            (None, Prompt, 0.),
            (None, call, line),
            (Some(Banner), Prompt, line),
            (Some(stamp), Prompt, line),
            (Some(hung), Prompt, line),
            (Some(prose), Prompt, line),
            (Some(Prompt), prose, line),
            (Some(Prompt), call, line),
            (Some(Prompt), Reasoning, line),
            (Some(call), call, 0.),
            (Some(call), diffed, 0.),
            (Some(diffed), call, line),
            (Some(commentary), call, line),
            (Some(prose), call, line),
            (Some(call), prose, line),
            (Some(Reasoning), call, line),
            (Some(call), Reasoning, line),
            // The stamp is a block of its turn; an elbow note hangs on the
            // row it answers.
            (Some(prose), stamp, line),
            (Some(call), stamp, line),
            (Some(prose), hung, 0.),
            (Some(call), hung, 0.),
            (Some(call), Meta, 0.),
            (Some(prose), Notice, line),
            (Some(prose), Tail, line),
            (Some(call), Tail, line),
            (Some(Other), Other, line),
        ] {
            assert_eq!(
                gap_before(previous, kind, READING),
                gap,
                "{previous:?} → {kind:?}"
            );
        }
    }

    #[test]
    fn the_blank_line_scales_with_the_reading_size_and_runs_stay_flush() {
        use crate::theme::answer_text_size;
        use ferrite_core::settings::ReadingSize;
        use RowKind::*;
        let prose = Answer { commentary: false };
        let call = Activity { diff_shown: false };
        for (size, line) in [
            (ReadingSize::STANDARD, 20.),
            (ReadingSize::nearest(14), 21.),
            (ReadingSize::nearest(16), 24.),
            (ReadingSize::nearest(18), 27.),
        ] {
            let reading = answer_text_size(size);
            assert_eq!(gap_before(Some(prose), Prompt, reading), line, "{size:?}");
            assert_eq!(gap_before(Some(Prompt), prose, reading), line, "{size:?}");
            assert_eq!(gap_before(Some(call), call, reading), 0.);
            assert_eq!(line, crate::theme::answer_line_height(size), "{size:?}");
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
            crate::theme::answer_text_size(ferrite_core::settings::ReadingSize::nearest(18));
        let delta = rows.reconcile(transcript.blocks(), &shape(), large);
        assert!(delta.splices.is_empty());
        assert_eq!(delta.remeasure, vec![1, 2]);
        assert!(
            Rc::ptr_eq(&first, rows.get(0).unwrap()),
            "the body padding does not scale"
        );
        assert_eq!(rows.get(2).unwrap().gap(), 27.);
    }

    #[test]
    fn a_row_is_classified_and_spaced_by_its_neighbour_at_projection() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        text(&mut transcript, "Looking first.");
        transcript.apply(Input::Event(SessionEvent::ContentBoundary));
        tool(&mut transcript, "a");
        tool(&mut transcript, "b");
        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        let kinds: Vec<_> = rows.rows().iter().map(|row| row.kind()).collect();
        let call = RowKind::Activity { diff_shown: false };
        assert_eq!(
            kinds,
            vec![
                RowKind::Prompt,
                RowKind::Answer { commentary: true },
                call,
                call
            ]
        );
        let gaps: Vec<_> = rows.rows().iter().map(|row| row.gap()).collect();
        let line = crate::theme::LH_PROSE;
        assert_eq!(gaps, vec![0., line, line, 0.]);
    }

    /// Q2: the mark goes on the first prose after a prompt or a tool row;
    /// prose after reasoning (no speaker) is the same answer and wears none.
    #[test]
    fn the_answer_mark_is_drawn_once_per_speaker_change_to_the_agent() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        text(&mut transcript, "first");
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "weighing it".into(),
        }));
        text(&mut transcript, "still the agent");
        tool(&mut transcript, "t1");
        text(&mut transcript, "after the tool");
        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        let marks: Vec<_> = rows
            .rows()
            .iter()
            .map(|row| (row.kind(), row.answer_mark()))
            .collect();
        assert_eq!(
            marks,
            vec![
                (RowKind::Prompt, false),
                (RowKind::Answer { commentary: true }, true),
                (RowKind::Reasoning, false),
                (RowKind::Answer { commentary: true }, false),
                (RowKind::Activity { diff_shown: false }, false),
                (RowKind::Answer { commentary: true }, true),
            ]
        );
    }

    /// CT-34/41: the banner heads the rows on the body's padding, the
    /// Decision closes them a line down, and a changed tail key is a
    /// changed row.
    #[test]
    fn the_banner_heads_the_rows_and_the_tail_closes_them() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        text(&mut transcript, "answer");
        let line = crate::theme::LH_PROSE;
        let solo = RowShape {
            banner: true,
            banner_pad: crate::theme::BANNER_PAD_T,
            tail: Some(7),
            ..RowShape::default()
        };
        let mut rows = TranscriptRows::new(transcript.blocks(), &solo, READING);
        let ids: Vec<_> = rows.rows().iter().map(|row| row.id().clone()).collect();
        assert_eq!(ids.first(), Some(&RowId::Banner));
        assert_eq!(ids.last(), Some(&RowId::Tail));
        assert_eq!(rows.get(0).unwrap().gap(), crate::theme::BANNER_PAD_T);
        assert_eq!(
            rows.get(1).unwrap().gap(),
            line,
            "the first band a line down"
        );
        assert_eq!(rows.rows().last().unwrap().gap(), line);
        let board = RowShape {
            banner_pad: 0.,
            tail: Some(8),
            ..solo.clone()
        };
        let delta = rows.reconcile(transcript.blocks(), &board, READING);
        assert!(delta.remeasure.contains(&0), "the banner's padding changed");
        assert!(
            delta.remeasure.contains(&(rows.len() - 1)),
            "the tail's key changed"
        );
        assert_eq!(rows.rows().last().unwrap().tail_key(), Some(8));
        // An empty Thread still has its banner.
        let empty = TranscriptRows::new(&[], &solo, READING);
        assert_eq!(empty.len(), 2);
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

    /// CT-17/18: a file's first diff in a turn shows; a later edit to it
    /// folds until opened, and only a drawn diff spaces the next call.
    #[test]
    fn a_later_edit_to_a_diffed_file_folds_its_diff() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "fix it");
        edit(&mut transcript, "e1", "crates/ferrite/src/nav.rs");
        bash(&mut transcript, "t1");
        edit(&mut transcript, "e2", "crates/ferrite/src/nav.rs");
        bash(&mut transcript, "t2");
        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        let calls: Vec<_> = rows
            .rows()
            .iter()
            .filter(|row| matches!(row.kind(), RowKind::Activity { .. }))
            .map(|row| (row.diff_shown(), row.diff_folds(), row.gap()))
            .collect();
        let line = crate::theme::LH_PROSE;
        assert_eq!(
            calls,
            vec![
                (true, false, line),
                (false, false, line),
                (false, true, 0.),
                (false, false, 0.),
            ]
        );
        let opened = RowShape {
            opened_diffs: HashSet::from(["e2".to_string()]),
            ..RowShape::default()
        };
        let rows = TranscriptRows::new(transcript.blocks(), &opened, READING);
        let last = rows.rows().last().unwrap();
        assert_eq!(last.gap(), line, "an opened diff spaces the next call too");
        // A new turn diffs the file afresh.
        prompt(&mut transcript, "again");
        edit(&mut transcript, "e3", "crates/ferrite/src/nav.rs");
        let rows = TranscriptRows::new(transcript.blocks(), &shape(), READING);
        assert!(rows.rows().last().unwrap().diff_shown());
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
