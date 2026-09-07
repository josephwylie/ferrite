//! Stable, owned semantic rows for the transcript virtual list.
//!
//! The Pane supplies its retained block window. This module neither decides
//! history retention nor renders a row; it only preserves the identities a
//! `ListState` needs to reconcile changing transcript content.

use std::{collections::HashMap, ops::Range, rc::Rc};

use ferrite_core::transcript::{Block, BlockId, Body, ToolActivity};

/// A stable semantic-row identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum RowId {
    /// An adjacent native Markdown answer, named by its original run.
    Markdown(BlockId),
    /// A consecutive tool disclosure, named by the leader's provider call.
    ToolActivity(String),
    /// One non-grouped transcript block.
    Block(BlockId),
}

/// One renderable transcript unit. Its blocks are owned so a list callback
/// does not borrow the transient slice passed to [`TranscriptRows::reconcile`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TranscriptRow {
    id: RowId,
    blocks: Rc<[Block]>,
    source: Option<Rc<str>>,
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
}

/// The owned row snapshot a virtual-list callback reads.
#[derive(Clone, Debug, Default)]
pub(crate) struct TranscriptRows {
    rows: Rc<[Rc<TranscriptRow>]>,
}

impl TranscriptRows {
    /// Project the caller-owned retained window into semantic rows.
    pub(crate) fn new(blocks: &[Block]) -> Self {
        let rows = project(blocks);
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
    pub(crate) fn reconcile(&mut self, blocks: &[Block]) -> RowDelta {
        let previous = self.rows.clone();
        let projected = project(blocks);
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

fn project(blocks: &[Block]) -> Vec<Rc<TranscriptRow>> {
    let mut rows = Vec::new();
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
            rows.push(Rc::new(TranscriptRow {
                id: RowId::Markdown(first),
                blocks: blocks[start..index].to_vec().into(),
                source: Some(source.into()),
            }));
            continue;
        }
        if let Some(activity) = ToolActivity::at_start(&blocks[index..]) {
            let len = activity.blocks.len();
            rows.push(Rc::new(TranscriptRow {
                id: RowId::ToolActivity(activity.leader().call.clone()),
                blocks: blocks[index..index + len].to_vec().into(),
                source: None,
            }));
            index += len;
            continue;
        }
        if !is_blank(block) {
            rows.push(Rc::new(TranscriptRow {
                id: RowId::Block(block.id),
                blocks: vec![block.clone()].into(),
                source: None,
            }));
        }
        index += 1;
    }
    rows
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

        let rows = TranscriptRows::new(transcript.blocks());
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
        let mut rows = TranscriptRows::new(transcript.blocks());
        let first = rows.get(0).unwrap().clone();

        text(&mut transcript, "second");
        let delta = rows.reconcile(transcript.blocks());
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
        let mut rows = TranscriptRows::new(transcript.blocks());
        let old = rows.rows().to_vec();

        prompt(&mut transcript, "d");
        text(&mut transcript, "d");
        let delta = rows.reconcile(&transcript.blocks()[2..]);
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
        assert!(Rc::ptr_eq(&old[2], rows.get(0).unwrap()));
    }

    #[test]
    fn prepend_preserves_the_existing_suffix_rows() {
        let mut transcript = Transcript::default();
        prompt(&mut transcript, "a");
        text(&mut transcript, "a");
        prompt(&mut transcript, "b");
        text(&mut transcript, "b");

        let mut rows = TranscriptRows::new(&transcript.blocks()[2..]);
        let old = rows.get(0).unwrap().clone();
        let delta = rows.reconcile(transcript.blocks());
        assert_eq!(
            delta.splices,
            vec![RowSplice {
                old_range: 0..0,
                new_count: 2
            }]
        );
        assert!(Rc::ptr_eq(&old, rows.get(2).unwrap()));
    }

    #[test]
    fn adjacent_tools_merge_into_the_leaders_stable_activity_row() {
        let mut transcript = Transcript::default();
        tool(&mut transcript, "first");
        let mut rows = TranscriptRows::new(transcript.blocks());
        assert!(matches!(rows.get(0).unwrap().id(), RowId::Block(_)));

        tool(&mut transcript, "second");
        let delta = rows.reconcile(transcript.blocks());
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
    fn blank_thinking_is_not_a_row() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "   ".into(),
        }));
        assert!(TranscriptRows::new(transcript.blocks()).is_empty());
    }
}
