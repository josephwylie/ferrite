//! Stable identities for native transcript text views. GPUI owns selection,
//! hit testing, highlight painting and copy; Ferrite only identifies runs.
use ferrite_core::{
    transcript::{Block, BlockId},
    ThreadId,
};
use gpui::base::{TextSelectionDocument, TextSelectionDocumentMember};
use gpui::{HighlightStyle, SharedString};
use std::{cell::RefCell, ops::Range};

#[cfg(test)]
type Registry =
    std::rc::Rc<RefCell<std::collections::HashMap<ThreadId, Vec<(BlockId, u32, bool, String)>>>>;

#[derive(Clone, Default)]
pub struct TranscriptText {
    #[cfg(test)]
    registry: Registry,
}
impl TranscriptText {
    pub fn overlay_scoped(
        &self,
        _thread: ThreadId,
        namespace: SharedString,
        _: &[Block],
        cache: crate::rich::TextCache,
    ) -> TextRuns {
        TextRuns {
            #[cfg(test)]
            thread: _thread,
            namespace,
            cache,
            block: RefCell::new(None),
            next_ordinal: RefCell::new(0),
            document: None,
            members: RefCell::new(None),
            #[cfg(test)]
            registry: self.registry.clone(),
        }
    }
    #[cfg(test)]
    pub fn registered(&self, thread: ThreadId) -> Vec<(BlockId, u32, bool, String)> {
        self.registry
            .borrow()
            .get(&thread)
            .cloned()
            .unwrap_or_default()
    }
}

pub struct TextRuns {
    #[cfg(test)]
    thread: ThreadId,
    namespace: SharedString,
    cache: crate::rich::TextCache,
    block: RefCell<Option<BlockId>>,
    next_ordinal: RefCell<u32>,
    document: Option<TextSelectionDocument>,
    members: RefCell<Option<Vec<TextSelectionDocumentMember>>>,
    #[cfg(test)]
    registry: Registry,
}
impl TextRuns {
    pub fn with_document(mut self, document: TextSelectionDocument) -> Self {
        self.document = Some(document);
        self
    }

    /// Reuse the presentation's text identities to describe logical membership.
    /// This pass constructs elements, but never mounts, parses or lays them out.
    /// It runs when content or disclosure changes, never for an ordinary frame.
    pub fn capture_members(&self, build: impl FnOnce()) -> Vec<TextSelectionDocumentMember> {
        assert!(
            self.members.borrow().is_none(),
            "nested transcript text capture"
        );
        #[cfg(test)]
        self.registry.borrow_mut().insert(self.thread, Vec::new());
        *self.members.borrow_mut() = Some(Vec::new());
        build();
        self.members.borrow_mut().take().unwrap_or_default()
    }

    /// A lazy list may measure the same row more than once in one frame.
    pub fn begin_row(&self) {
        *self.block.borrow_mut() = None;
        *self.next_ordinal.borrow_mut() = 0;
    }

    fn collect(&self, id: &SharedString, source: &str, markdown: bool) {
        if let Some(members) = self.members.borrow_mut().as_mut() {
            let source: SharedString = source.to_owned().into();
            let version = source.clone();
            members.push(
                TextSelectionDocumentMember::new(id.clone(), move |_| {
                    if markdown {
                        gpui::base::text::TextView::markdown_plain_text(&source)
                    } else {
                        gpui::base::text::TextView::markdown_plain_text(
                            &crate::rich::literal_source(&source),
                        )
                    }
                })
                .with_content_version(version),
            );
        }
    }

    pub fn answer(&self, first: BlockId, source: String) -> crate::rich::Markdown {
        #[cfg(test)]
        if self.members.borrow().is_some() {
            self.registry
                .borrow_mut()
                .entry(self.thread)
                .or_default()
                .push((first, 0, true, source.clone()));
        }
        let id: SharedString = format!("markdown-{}-{first:?}", self.namespace).into();
        self.collect(&id, &source, true);
        crate::rich::Markdown::new(id, source, self.cache.clone())
            .selection_document(self.document.clone())
    }

    pub fn output(&self, block: BlockId, part: &str, text: &str) -> crate::rich::Output {
        #[cfg(test)]
        if self.members.borrow().is_some() {
            self.registry
                .borrow_mut()
                .entry(self.thread)
                .or_default()
                .push((block, 0, true, text.to_string()));
        }
        crate::rich::Output {
            id: format!("output-{}-{block:?}-{part}", self.namespace).into(),
            text: text.to_string().into(),
            cache: self.cache.clone(),
        }
    }

    pub fn markdown(&self, block: BlockId, source: String) -> crate::rich::Markdown {
        #[cfg(test)]
        if self.members.borrow().is_some() {
            self.registry
                .borrow_mut()
                .entry(self.thread)
                .or_default()
                .push((block, 0, true, source.clone()));
        }
        let id: SharedString = format!("thinking-{}-{block:?}", self.namespace).into();
        self.collect(&id, &source, true);
        crate::rich::Markdown::new(id, source, self.cache.clone())
            .selection_document(self.document.clone())
    }

    pub fn line(
        &self,
        block: BlockId,
        text: impl Into<SharedString>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    ) -> crate::rich::Literal {
        let text = text.into();
        let mut current = self.block.borrow_mut();
        if *current != Some(block) {
            *current = Some(block);
            *self.next_ordinal.borrow_mut() = 0;
        }
        let ordinal = *self.next_ordinal.borrow();
        *self.next_ordinal.borrow_mut() += 1;
        #[cfg(test)]
        if self.members.borrow().is_some() {
            self.registry
                .borrow_mut()
                .entry(self.thread)
                .or_default()
                .push((block, ordinal, true, text.to_string()));
        }
        let id: SharedString = format!("literal-{}-{block:?}-{ordinal}", self.namespace).into();
        self.collect(&id, &text, false);
        crate::rich::Literal {
            id,
            text,
            highlights,
            cache: self.cache.clone(),
            document: self.document.clone(),
        }
    }
}
