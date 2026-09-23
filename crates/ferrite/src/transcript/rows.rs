//! Stable, owned semantic rows for the transcript virtual list.
//!
//! The Pane supplies its retained block window. This module neither decides
//! history retention nor renders a row; it only preserves the identities a
//! `ListState` needs to reconcile changing transcript content.

use std::{collections::HashMap, ops::Range, rc::Rc};

use ferrite_core::transcript::{Block, BlockId, Body, ToolActivity, TurnDiff};

/// A stable semantic-row identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RowId {
    /// An adjacent native Markdown answer, named by its original run.
    Markdown(BlockId),
    /// A consecutive tool disclosure, named by the leader's provider call.
    ToolActivity(String),
    /// One non-grouped transcript block.
    Block(BlockId),
    /// The native turn-wide change summary.
    TurnDiff(String),
}

/// What a row is, for the gap table: derived once in `project`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowKind {
    Prompt,
    /// An agent answer. `commentary` is a lone paragraph, the kind that
    /// introduces the work under it.
    Answer {
        commentary: bool,
    },
    /// A tool call or a group of them.
    Activity,
    Reasoning,
    Notice,
    /// A decision record or a revival note.
    Meta,
    /// A turn's end: its stamp, or the note that it was interrupted or failed.
    TurnEnd,
    TurnDiff,
    /// A fallback prose or code block.
    Other,
}

impl RowKind {
    fn of(block: &Block) -> Self {
        match &block.body {
            Body::Prompt(_) => Self::Prompt,
            Body::Tool(_) => Self::Activity,
            Body::Thinking(_) => Self::Reasoning,
            Body::Notice(_) => Self::Notice,
            Body::Meta(_) => Self::Meta,
            Body::TurnEnd(_) => Self::TurnEnd,
            Body::Paragraph { .. }
            | Body::Heading { .. }
            | Body::Bullet { .. }
            | Body::Code { .. } => Self::Other,
        }
    }
}

/// The space above a row, from the row before it (`None`: the first row,
/// which carries the body's top padding instead) and its own kind. The one
/// table of the transcript's vertical rhythm.
pub(crate) fn gap_before(previous: Option<RowKind>, kind: RowKind) -> f32 {
    use RowKind::*;
    let Some(previous) = previous else {
        return crate::theme::BODY_PAD_T;
    };
    match (previous, kind) {
        (_, Prompt) => crate::theme::GAP_TURN,
        (_, TurnEnd | Meta) => crate::theme::GAP_STAMP,
        (Activity, Activity) | (Answer { commentary: true }, Activity) => crate::theme::GAP_TOOL,
        _ => crate::theme::GAP_SECTION,
    }
}

/// One renderable transcript unit. Its blocks are owned so a list callback
/// does not borrow the transient slice passed to [`TranscriptRows::reconcile`].
///
/// Everything a row draws is in its equality: its gap and whether it is the
/// live notice included, so a row whose neighbour changed its spacing or
/// colour is a changed row, re-measured by reconcile and never per frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptRow {
    id: RowId,
    blocks: Rc<[Block]>,
    source: Option<Rc<str>>,
    turn_diff: Option<TurnDiff>,
    kind: RowKind,
    gap: f32,
    live_notice: bool,
}

impl TranscriptRow {
    /// Stable identity used by list reconciliation and element keys.
    #[cfg(test)]
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

    pub(crate) fn turn_diff(&self) -> Option<&TurnDiff> {
        self.turn_diff.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn kind(&self) -> RowKind {
        self.kind
    }

    /// The space above this row (`gap_before`).
    pub(crate) fn gap(&self) -> f32 {
        self.gap
    }

    /// The transcript's most recent Notice: the only one that wears the
    /// Pane's state colour. Older notices are history and stay neutral.
    pub(crate) fn live_notice(&self) -> bool {
        self.live_notice
    }
}

/// The owned row snapshot a virtual-list callback reads.
#[derive(Clone, Debug, Default)]
pub(crate) struct TranscriptRows {
    rows: Rc<[Rc<TranscriptRow>]>,
}

impl TranscriptRows {
    /// Project the caller-owned retained window into semantic rows.
    pub(crate) fn new(blocks: &[Block], turn_diff: Option<&TurnDiff>) -> Self {
        let rows = project(blocks, turn_diff);
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
    pub(crate) fn reconcile(&mut self, blocks: &[Block], turn_diff: Option<&TurnDiff>) -> RowDelta {
        let previous = self.rows.clone();
        let projected = project(blocks, turn_diff);
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

fn project(blocks: &[Block], turn_diff: Option<&TurnDiff>) -> Vec<Rc<TranscriptRow>> {
    let mut rows = Vec::new();
    let row = |id, blocks: &[Block], source: Option<Rc<str>>, kind| TranscriptRow {
        id,
        blocks: blocks.to_vec().into(),
        source,
        turn_diff: None,
        kind,
        gap: 0.,
        live_notice: false,
    };
    let mut index = 0;
    while index < blocks.len() {
        let block = &blocks[index];
        if block.markdown.is_some() {
            let first = block.markdown_run.unwrap_or(block.id);
            let start = index;
            let mut source = String::new();
            while let Some(markdown) = blocks.get(index).and_then(|block| block.markdown.as_ref()) {
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
        if let Some(activity) = ToolActivity::at_start(&blocks[index..]) {
            let len = activity.blocks.len();
            rows.push(row(
                RowId::ToolActivity(activity.leader().call.clone()),
                &blocks[index..index + len],
                None,
                RowKind::Activity,
            ));
            index += len;
            continue;
        }
        if !is_blank(block) {
            rows.push(row(
                RowId::Block(block.id),
                std::slice::from_ref(block),
                None,
                RowKind::of(block),
            ));
        }
        index += 1;
    }
    if let Some(turn_diff) = turn_diff {
        // The turn's changes precede the rows that close the turn (its stamp,
        // a decision record, a notice), so the stamp stays the turn's last
        // word.
        let at = rows.len()
            - rows
                .iter()
                .rev()
                .take_while(|row| {
                    matches!(row.kind, RowKind::TurnEnd | RowKind::Meta | RowKind::Notice)
                })
                .count();
        rows.insert(
            at,
            TranscriptRow {
                id: RowId::TurnDiff(turn_diff.turn_id.clone()),
                blocks: Vec::new().into(),
                source: None,
                turn_diff: Some(turn_diff.clone()),
                kind: RowKind::TurnDiff,
                gap: 0.,
                live_notice: false,
            },
        );
    }
    let live = rows.iter().rposition(|row| row.kind == RowKind::Notice);
    let mut previous = None;
    for (index, row) in rows.iter_mut().enumerate() {
        row.gap = gap_before(previous, row.kind);
        row.live_notice = live == Some(index);
        previous = Some(row.kind);
    }
    rows.into_iter().map(Rc::new).collect()
}

fn is_blank(block: &Block) -> bool {
    matches!(&block.body, Body::Thinking(text) if text.trim().is_empty())
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

    #[test]
    fn contiguous_markdown_blocks_share_one_native_answer_row() {
        let mut transcript = Transcript::default();
        text(&mut transcript, "first");
        transcript.apply(Input::Event(SessionEvent::ContentBoundary));
        text(&mut transcript, "second");

        let rows = TranscriptRows::new(transcript.blocks(), None);
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows.get(0).unwrap().id(), RowId::Markdown(_)));
        assert_eq!(rows.get(0).unwrap().blocks().len(), 2);
        assert_eq!(rows.get(0).unwrap().source(), Some("firstsecond"));
    }

    #[test]
    fn append_keeps_prior_row_rcs_and_splices_the_tail() {
        let mut transcript = Transcript::default();
        text(&mut transcript, "first");
        prompt(&mut transcript, "next");
        let mut rows = TranscriptRows::new(transcript.blocks(), None);
        let first = rows.get(0).unwrap().clone();

        text(&mut transcript, "second");
        let delta = rows.reconcile(transcript.blocks(), None);
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
        let mut rows = TranscriptRows::new(transcript.blocks(), None);
        let old = rows.rows().to_vec();

        prompt(&mut transcript, "d");
        text(&mut transcript, "d");
        let delta = rows.reconcile(&transcript.blocks()[2..], None);
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
        // The new head takes the body's top padding in place of its turn
        // gap: a changed row, re-measured, while the rest keep their Rcs.
        assert!(!Rc::ptr_eq(&old[2], rows.get(0).unwrap()));
        assert_eq!(rows.get(0).unwrap().gap(), crate::theme::BODY_PAD_T);
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

        let mut rows = TranscriptRows::new(&transcript.blocks()[2..], None);
        let old = rows.rows().to_vec();
        let delta = rows.reconcile(transcript.blocks(), None);
        assert_eq!(
            delta.splices,
            vec![RowSplice {
                old_range: 0..0,
                new_count: 2
            }]
        );
        // The old head is now a later turn's prompt: its gap grew from the
        // body padding to the turn gap, so only it is re-measured.
        assert_eq!(rows.get(2).unwrap().gap(), crate::theme::GAP_TURN);
        assert_eq!(delta.remeasure, vec![2]);
        assert!(Rc::ptr_eq(&old[1], rows.get(3).unwrap()));
    }

    #[test]
    fn adjacent_tools_merge_into_the_leaders_stable_activity_row() {
        let mut transcript = Transcript::default();
        tool(&mut transcript, "first");
        let mut rows = TranscriptRows::new(transcript.blocks(), None);
        assert!(matches!(rows.get(0).unwrap().id(), RowId::Block(_)));

        tool(&mut transcript, "second");
        let delta = rows.reconcile(transcript.blocks(), None);
        assert!(matches!(
            rows.get(0).unwrap().id(),
            RowId::ToolActivity(call) if call == "first"
        ));
        assert_eq!(
            delta.splices,
            vec![RowSplice {
                old_range: 0..1,
                new_count: 1
            }]
        );
    }

    #[test]
    fn the_gap_table_spaces_turns_sections_tool_runs_and_stamps() {
        use crate::theme::{BODY_PAD_T, GAP_SECTION, GAP_STAMP, GAP_TOOL, GAP_TURN};
        use RowKind::*;
        let prose = Answer { commentary: false };
        let commentary = Answer { commentary: true };
        for (previous, kind, gap) in [
            (None, Prompt, BODY_PAD_T),
            (None, Activity, BODY_PAD_T),
            (Some(TurnEnd), Prompt, GAP_TURN),
            (Some(prose), Prompt, GAP_TURN),
            (Some(Prompt), prose, GAP_SECTION),
            (Some(Prompt), Activity, GAP_SECTION),
            (Some(Prompt), Reasoning, GAP_SECTION),
            (Some(Activity), Activity, GAP_TOOL),
            (Some(commentary), Activity, GAP_TOOL),
            (Some(prose), Activity, GAP_SECTION),
            (Some(Activity), prose, GAP_SECTION),
            (Some(Reasoning), Activity, GAP_SECTION),
            (Some(Activity), Reasoning, GAP_SECTION),
            (Some(prose), TurnEnd, GAP_STAMP),
            (Some(Activity), TurnEnd, GAP_STAMP),
            (Some(Activity), Meta, GAP_STAMP),
            (Some(prose), Notice, GAP_SECTION),
            (Some(prose), TurnDiff, GAP_SECTION),
            (Some(Other), Other, GAP_SECTION),
        ] {
            assert_eq!(gap_before(previous, kind), gap, "{previous:?} → {kind:?}");
        }
    }

    #[test]
    fn a_row_is_classified_and_spaced_by_its_neighbour_at_projection() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        text(&mut transcript, "Looking first.");
        transcript.apply(Input::Event(SessionEvent::ContentBoundary));
        tool(&mut transcript, "a");
        tool(&mut transcript, "b");
        let rows = TranscriptRows::new(transcript.blocks(), None);
        let kinds: Vec<_> = rows.rows().iter().map(|row| row.kind()).collect();
        assert_eq!(
            kinds,
            vec![
                RowKind::Prompt,
                RowKind::Answer { commentary: true },
                RowKind::Activity
            ]
        );
        let gaps: Vec<_> = rows.rows().iter().map(|row| row.gap()).collect();
        assert_eq!(
            gaps,
            vec![
                crate::theme::BODY_PAD_T,
                crate::theme::GAP_SECTION,
                crate::theme::GAP_TOOL
            ]
        );
    }

    #[test]
    fn only_the_latest_notice_is_live_and_the_hand_off_changes_both_rows() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Notice("model changed".into()));
        let mut rows = TranscriptRows::new(transcript.blocks(), None);
        assert!(rows.get(0).unwrap().live_notice());
        prompt(&mut transcript, "go");
        transcript.apply(Input::Notice("send failed".into()));
        let delta = rows.reconcile(transcript.blocks(), None);
        let live: Vec<_> = rows.rows().iter().map(|row| row.live_notice()).collect();
        assert_eq!(live, vec![false, false, true]);
        assert!(
            delta.remeasure.contains(&0),
            "the old notice is a changed row, so its colour is redrawn"
        );
    }

    #[test]
    fn the_turns_changes_precede_the_rows_that_close_the_turn() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        text(&mut transcript, "done");
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Interrupted,
            cost_usd: None,
        }));
        let diff = TurnDiff {
            turn_id: "t".into(),
            diff: "+x".into(),
            omitted_bytes: 0,
        };
        let rows = TranscriptRows::new(transcript.blocks(), Some(&diff));
        let kinds: Vec<_> = rows.rows().iter().map(|row| row.kind()).collect();
        assert_eq!(
            kinds,
            vec![
                RowKind::Prompt,
                RowKind::Answer { commentary: true },
                RowKind::TurnDiff,
                RowKind::TurnEnd
            ]
        );
    }

    #[test]
    fn blank_thinking_is_not_a_row() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "   ".into(),
        }));
        assert!(TranscriptRows::new(transcript.blocks(), None).is_empty());
    }
}
