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
    /// A decision record or a revival note: it hangs on an elbow under the
    /// row it answers.
    Meta,
    /// A turn's end: its stamp, or (`hangs`) the elbow note that it was
    /// interrupted or failed.
    TurnEnd {
        hangs: bool,
    },
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

/// The space above a row, from the row before it (`None`: the first row,
/// which carries the body's top padding instead), its own kind and the
/// answer size `reading`. The one table of the transcript's vertical rhythm
/// (see the transcript grammar in `theme.rs`).
pub(crate) fn gap_before(previous: Option<RowKind>, kind: RowKind, reading: f32) -> f32 {
    use crate::theme::{reading_step, BODY_PAD_T, GAP_BLOCK, GAP_ROW, GAP_TURN};
    use RowKind::*;
    let Some(previous) = previous else {
        return BODY_PAD_T;
    };
    match (previous, kind) {
        (_, Prompt) => reading_step(GAP_TURN, reading),
        (_, TurnEnd { hangs: true } | Meta)
        | (Activity, Activity)
        | (Answer { commentary: true }, Activity) => GAP_ROW,
        _ => reading_step(GAP_BLOCK, reading),
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
    /// Project the caller-owned retained window into semantic rows, spaced
    /// for answers at `reading` px.
    pub(crate) fn new(blocks: &[Block], turn_diff: Option<&TurnDiff>, reading: f32) -> Self {
        let rows = project(blocks, turn_diff, reading);
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
        turn_diff: Option<&TurnDiff>,
        reading: f32,
    ) -> RowDelta {
        let previous = self.rows.clone();
        let projected = project(blocks, turn_diff, reading);
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

fn project(blocks: &[Block], turn_diff: Option<&TurnDiff>, reading: f32) -> Vec<Rc<TranscriptRow>> {
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
                    matches!(
                        row.kind,
                        RowKind::TurnEnd { .. } | RowKind::Meta | RowKind::Notice
                    )
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
        row.gap = gap_before(previous, row.kind, reading);
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

    /// Answers at the Standard reading size.
    const READING: f32 = crate::theme::FS_PROSE;

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

        let rows = TranscriptRows::new(transcript.blocks(), None, READING);
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
        let mut rows = TranscriptRows::new(transcript.blocks(), None, READING);
        let first = rows.get(0).unwrap().clone();

        text(&mut transcript, "second");
        let delta = rows.reconcile(transcript.blocks(), None, READING);
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
        let mut rows = TranscriptRows::new(transcript.blocks(), None, READING);
        let old = rows.rows().to_vec();

        prompt(&mut transcript, "d");
        text(&mut transcript, "d");
        let delta = rows.reconcile(&transcript.blocks()[2..], None, READING);
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

        let mut rows = TranscriptRows::new(&transcript.blocks()[2..], None, READING);
        let old = rows.rows().to_vec();
        let delta = rows.reconcile(transcript.blocks(), None, READING);
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
        let mut rows = TranscriptRows::new(transcript.blocks(), None, READING);
        assert!(matches!(rows.get(0).unwrap().id(), RowId::Block(_)));

        tool(&mut transcript, "second");
        let delta = rows.reconcile(transcript.blocks(), None, READING);
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
    fn the_gap_table_spaces_turns_blocks_rows_and_stamps() {
        use crate::theme::{BODY_PAD_T, GAP_BLOCK, GAP_ROW, GAP_TURN};
        use RowKind::*;
        let prose = Answer { commentary: false };
        let commentary = Answer { commentary: true };
        let stamp = TurnEnd { hangs: false };
        let hung = TurnEnd { hangs: true };
        for (previous, kind, gap) in [
            (None, Prompt, BODY_PAD_T),
            (None, Activity, BODY_PAD_T),
            (Some(stamp), Prompt, GAP_TURN),
            (Some(hung), Prompt, GAP_TURN),
            (Some(prose), Prompt, GAP_TURN),
            (Some(Prompt), prose, GAP_BLOCK),
            (Some(Prompt), Activity, GAP_BLOCK),
            (Some(Prompt), Reasoning, GAP_BLOCK),
            (Some(Activity), Activity, GAP_ROW),
            (Some(commentary), Activity, GAP_ROW),
            (Some(prose), Activity, GAP_BLOCK),
            (Some(Activity), prose, GAP_BLOCK),
            (Some(Reasoning), Activity, GAP_BLOCK),
            (Some(Activity), Reasoning, GAP_BLOCK),
            // The stamp is a block of its turn; an elbow note hangs on the
            // row it answers.
            (Some(prose), stamp, GAP_BLOCK),
            (Some(Activity), stamp, GAP_BLOCK),
            (Some(prose), hung, GAP_ROW),
            (Some(Activity), hung, GAP_ROW),
            (Some(Activity), Meta, GAP_ROW),
            (Some(prose), Notice, GAP_BLOCK),
            (Some(prose), TurnDiff, GAP_BLOCK),
            (Some(Other), Other, GAP_BLOCK),
        ] {
            assert_eq!(
                gap_before(previous, kind, READING),
                gap,
                "{previous:?} → {kind:?}"
            );
        }
        // Each step is at least twice the one inside it.
        assert!(GAP_TURN >= 2. * GAP_BLOCK && GAP_BLOCK >= 2. * GAP_ROW);
    }

    #[test]
    fn the_turn_and_block_steps_scale_with_the_reading_size_and_rows_do_not() {
        use crate::theme::{
            answer_text_size, reading_step, GAP_BLOCK, GAP_ROW, GAP_TURN, PROSE_GAP,
        };
        use ferrite_core::settings::SoloReadingSize;
        use RowKind::*;
        let prose = Answer { commentary: false };
        let mut previous = None;
        for (size, turn, block) in [
            (SoloReadingSize::Standard, 32., 12.),
            (SoloReadingSize::Comfortable, 37., 14.),
            (SoloReadingSize::Large, 41., 15.),
        ] {
            let reading = answer_text_size(size);
            assert_eq!(gap_before(Some(prose), Prompt, reading), turn, "{size:?}");
            assert_eq!(gap_before(Some(Prompt), prose, reading), block, "{size:?}");
            assert_eq!(gap_before(Some(Activity), Activity, reading), GAP_ROW);
            assert_eq!(reading_step(GAP_TURN, reading), turn);
            // A paragraph gap inside an answer never outgrows the block
            // step between it and the next block.
            assert_eq!(
                reading_step(PROSE_GAP, reading),
                reading_step(GAP_BLOCK, reading)
            );
            assert!(turn >= 2. * block && block >= 2. * GAP_ROW, "{size:?}");
            if let Some((turn_before, block_before)) = previous {
                assert!(turn > turn_before && block > block_before, "{size:?}");
            }
            previous = Some((turn, block));
        }
    }

    #[test]
    fn a_reading_size_change_respaces_every_row_after_the_first() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        text(&mut transcript, "done");
        prompt(&mut transcript, "again");
        let mut rows = TranscriptRows::new(transcript.blocks(), None, READING);
        let first = rows.get(0).unwrap().clone();
        let large = crate::theme::answer_text_size(ferrite_core::settings::SoloReadingSize::Large);
        let delta = rows.reconcile(transcript.blocks(), None, large);
        assert!(delta.splices.is_empty());
        assert_eq!(delta.remeasure, vec![1, 2]);
        assert!(
            Rc::ptr_eq(&first, rows.get(0).unwrap()),
            "the body padding does not scale"
        );
        assert_eq!(rows.get(2).unwrap().gap(), 41.);
    }

    #[test]
    fn a_row_is_classified_and_spaced_by_its_neighbour_at_projection() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "go");
        text(&mut transcript, "Looking first.");
        transcript.apply(Input::Event(SessionEvent::ContentBoundary));
        tool(&mut transcript, "a");
        tool(&mut transcript, "b");
        let rows = TranscriptRows::new(transcript.blocks(), None, READING);
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
                crate::theme::GAP_BLOCK,
                crate::theme::GAP_ROW
            ]
        );
    }

    #[test]
    fn only_the_latest_notice_is_live_and_the_hand_off_changes_both_rows() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Notice("model changed".into()));
        let mut rows = TranscriptRows::new(transcript.blocks(), None, READING);
        assert!(rows.get(0).unwrap().live_notice());
        prompt(&mut transcript, "go");
        transcript.apply(Input::Notice("send failed".into()));
        let delta = rows.reconcile(transcript.blocks(), None, READING);
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
        let rows = TranscriptRows::new(transcript.blocks(), Some(&diff), READING);
        let kinds: Vec<_> = rows.rows().iter().map(|row| row.kind()).collect();
        assert_eq!(
            kinds,
            vec![
                RowKind::Prompt,
                RowKind::Answer { commentary: true },
                RowKind::TurnDiff,
                RowKind::TurnEnd { hangs: true }
            ]
        );
    }

    #[test]
    fn blank_thinking_is_not_a_row() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "   ".into(),
        }));
        assert!(TranscriptRows::new(transcript.blocks(), None, READING).is_empty());
    }
}
