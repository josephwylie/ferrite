#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryDirection {
    Older,
    Newer,
}

pub(crate) struct PromptHistory {
    entries: Vec<String>,
    cursor: Option<usize>,
    draft: Option<String>,
}

impl PromptHistory {
    pub(crate) fn new(entries: Vec<String>) -> Self {
        Self {
            entries,
            cursor: None,
            draft: None,
        }
    }

    pub(crate) fn has_entries(&self) -> bool {
        !self.entries.is_empty()
    }

    pub(crate) fn recall(
        &mut self,
        direction: HistoryDirection,
        current_draft: &str,
    ) -> Option<String> {
        match direction {
            HistoryDirection::Older => {
                if self.entries.is_empty() {
                    return None;
                }
                let cursor = match self.cursor {
                    Some(0) => return None,
                    Some(cursor) => cursor - 1,
                    None => {
                        self.draft = Some(current_draft.to_string());
                        self.entries.len() - 1
                    }
                };
                self.cursor = Some(cursor);
                Some(self.entries[cursor].clone())
            }
            HistoryDirection::Newer => {
                let cursor = self.cursor?;
                if cursor + 1 < self.entries.len() {
                    let cursor = cursor + 1;
                    self.cursor = Some(cursor);
                    Some(self.entries[cursor].clone())
                } else {
                    self.cursor = None;
                    self.draft.take()
                }
            }
        }
    }

    pub(crate) fn reset(&mut self) {
        self.cursor = None;
        self.draft = None;
    }

    pub(crate) fn append(&mut self, text: String) {
        self.entries.push(text);
        self.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::{HistoryDirection, PromptHistory};

    #[test]
    fn traverses_both_directions_and_restores_the_exact_draft() {
        let mut history = PromptHistory::new(vec!["one".into(), "two".into()]);

        assert_eq!(
            history
                .recall(HistoryDirection::Older, "  thr…\n")
                .as_deref(),
            Some("two")
        );
        assert_eq!(
            history
                .recall(HistoryDirection::Older, "edited recall")
                .as_deref(),
            Some("one")
        );
        assert_eq!(history.recall(HistoryDirection::Older, "one edited"), None);
        assert_eq!(
            history
                .recall(HistoryDirection::Newer, "edited recall")
                .as_deref(),
            Some("two")
        );
        assert_eq!(
            history
                .recall(HistoryDirection::Newer, "edited recall")
                .as_deref(),
            Some("  thr…\n")
        );
        assert_eq!(
            history.recall(HistoryDirection::Newer, "edited recall"),
            None
        );
    }

    #[test]
    fn duplicates_are_distinct_and_reset_forgets_the_saved_draft() {
        let mut history = PromptHistory::new(vec!["same".into(), "same".into()]);
        assert_eq!(
            history.recall(HistoryDirection::Older, "first").as_deref(),
            Some("same")
        );
        assert_eq!(
            history.recall(HistoryDirection::Older, "edited").as_deref(),
            Some("same")
        );

        history.reset();
        assert_eq!(history.recall(HistoryDirection::Newer, "ignored"), None);
        assert_eq!(
            history.recall(HistoryDirection::Older, "second").as_deref(),
            Some("same")
        );
        assert_eq!(
            history.recall(HistoryDirection::Newer, "edited").as_deref(),
            Some("second")
        );
    }
}
