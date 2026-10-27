//! Stable identities for native transcript text views. GPUI owns selection,
//! hit testing, highlight painting and copy; Ferrite only identifies runs.
use ferrite_core::{
    transcript::{Block, BlockId},
    ThreadId,
};
use gpui::base::{TextSelectionDocument, TextSelectionDocumentMember};
use gpui::{HighlightStyle, SharedString};
use std::{cell::RefCell, ops::Range, rc::Rc};

#[cfg(any(test, feature = "test-support"))]
type Registry =
    std::rc::Rc<RefCell<std::collections::HashMap<ThreadId, Vec<(BlockId, u32, bool, String)>>>>;

#[derive(Clone, Default)]
pub struct TranscriptText {
    #[cfg(any(test, feature = "test-support"))]
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
            #[cfg(any(test, feature = "test-support"))]
            thread: _thread,
            namespace,
            cache,
            block: RefCell::new(None),
            next_ordinal: RefCell::new(0),
            document: None,
            find: None,
            members: RefCell::new(None),
            #[cfg(any(test, feature = "test-support"))]
            registry: self.registry.clone(),
        }
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn registered(&self, thread: ThreadId) -> Vec<(BlockId, u32, bool, String)> {
        self.registry
            .borrow()
            .get(&thread)
            .cloned()
            .unwrap_or_default()
    }
}

/// One native text as copy and find read it: its id (the selection
/// member's key) and its source as drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct RunText {
    pub key: SharedString,
    pub source: SharedString,
    markdown: bool,
}

impl RunText {
    /// The text as rendered: Markdown without its markup, a literal exact.
    pub fn plain(&self) -> String {
        gpui::base::text::TextView::markdown_plain_text(&self.markdown())
    }

    /// The text a find counts in: `plain`, with every run its view paints
    /// on a line of its own. A literal is one run, as written.
    pub fn find_text(&self) -> String {
        if self.markdown {
            gpui::base::text::TextView::markdown_find_text(&self.source)
        } else {
            self.source.to_string()
        }
    }

    fn markdown(&self) -> std::borrow::Cow<'_, str> {
        if self.markdown {
            self.source.as_ref().into()
        } else {
            crate::rich::literal_source(&self.source).into()
        }
    }
}

/// What a find marks in these runs: its query, and the run (by key) and
/// ordinal of the occurrence it is on, which reports where it painted.
pub struct FindMarks {
    pub query: crate::find::Query,
    pub active: Option<(SharedString, usize)>,
    pub report: Rc<gpui::base::text::FindReport>,
}

pub struct TextRuns {
    #[cfg(any(test, feature = "test-support"))]
    thread: ThreadId,
    namespace: SharedString,
    cache: crate::rich::TextCache,
    block: RefCell<Option<BlockId>>,
    next_ordinal: RefCell<u32>,
    document: Option<TextSelectionDocument>,
    find: Option<FindMarks>,
    members: RefCell<Option<(Vec<TextSelectionDocumentMember>, Vec<RunText>)>>,
    #[cfg(any(test, feature = "test-support"))]
    registry: Registry,
}
impl TextRuns {
    pub fn with_document(mut self, document: TextSelectionDocument) -> Self {
        self.document = Some(document);
        self
    }

    pub fn with_find(mut self, find: Option<FindMarks>) -> Self {
        self.find = find;
        self
    }

    /// Reuse the presentation's text identities to describe logical membership.
    /// The caller collects text without constructing or mounting UI elements.
    /// It runs when content or disclosure changes, never for an ordinary frame.
    /// The texts, in order, are what a find searches.
    pub fn capture_members(
        &self,
        build: impl FnOnce(),
    ) -> (Vec<TextSelectionDocumentMember>, Vec<RunText>) {
        assert!(
            self.members.borrow().is_none(),
            "nested transcript text capture"
        );
        #[cfg(any(test, feature = "test-support"))]
        self.registry.borrow_mut().insert(self.thread, Vec::new());
        *self.members.borrow_mut() = Some(Default::default());
        build();
        self.members.borrow_mut().take().unwrap_or_default()
    }

    /// How many texts the capture in progress holds.
    pub fn captured(&self) -> usize {
        self.members
            .borrow()
            .as_ref()
            .map_or(0, |(_, texts)| texts.len())
    }

    /// A lazy list may measure the same row more than once in one frame.
    pub fn begin_row(&self) {
        *self.block.borrow_mut() = None;
        *self.next_ordinal.borrow_mut() = 0;
    }

    /// Note a native text for copy (a member of the selection document)
    /// and find, in order. A bounded output viewer owns its selection, so
    /// only find notes it (`copied` false).
    fn collect(&self, id: &SharedString, source: &str, markdown: bool, copied: bool) {
        if let Some((members, texts)) = self.members.borrow_mut().as_mut() {
            let text = RunText {
                key: id.clone(),
                source: source.to_owned().into(),
                markdown,
            };
            if copied {
                let version = text.source.clone();
                let copy = text.clone();
                members.push(
                    TextSelectionDocumentMember::new(id.clone(), move |_| copy.plain())
                        .with_content_version(version),
                );
            }
            texts.push(text);
        }
    }

    fn marks(&self, id: &SharedString) -> Option<gpui::base::text::TextFind> {
        let find = self.find.as_ref()?;
        let active = find
            .active
            .as_ref()
            .filter(|(key, _)| key == id)
            .map(|(_, ordinal)| *ordinal);
        crate::find::marks(&find.query, active, &find.report)
    }

    pub fn answer(&self, first: BlockId, source: String) -> crate::rich::Markdown {
        #[cfg(any(test, feature = "test-support"))]
        if self.members.borrow().is_some() {
            self.registry
                .borrow_mut()
                .entry(self.thread)
                .or_default()
                .push((first, 0, true, source.clone()));
        }
        let id: SharedString = format!("markdown-{}-{first:?}", self.namespace).into();
        self.collect(&id, &source, true, true);
        let find = self.marks(&id);
        crate::rich::Markdown::new(id, source, self.cache.clone())
            .selection_document(self.document.clone())
            .find(find)
    }

    pub fn output(&self, block: BlockId, part: &str, text: &str) -> crate::rich::Output {
        #[cfg(any(test, feature = "test-support"))]
        if self.members.borrow().is_some() {
            self.registry
                .borrow_mut()
                .entry(self.thread)
                .or_default()
                .push((block, 0, true, text.to_string()));
        }
        let id: SharedString = format!("output-{}-{block:?}-{part}", self.namespace).into();
        self.collect(&id, text, false, false);
        crate::rich::Output {
            find: self.marks(&id),
            id,
            text: text.to_string().into(),
            cache: self.cache.clone(),
            aria_label: "Tool output".into(),
            fill: false,
        }
    }

    pub fn markdown(&self, block: BlockId, source: String) -> crate::rich::Markdown {
        #[cfg(any(test, feature = "test-support"))]
        if self.members.borrow().is_some() {
            self.registry
                .borrow_mut()
                .entry(self.thread)
                .or_default()
                .push((block, 0, true, source.clone()));
        }
        let id: SharedString = format!("thinking-{}-{block:?}", self.namespace).into();
        self.collect(&id, &source, true, true);
        let find = self.marks(&id);
        crate::rich::Markdown::new(id, source, self.cache.clone())
            .selection_document(self.document.clone())
            .find(find)
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
        #[cfg(any(test, feature = "test-support"))]
        if self.members.borrow().is_some() {
            self.registry
                .borrow_mut()
                .entry(self.thread)
                .or_default()
                .push((block, ordinal, true, text.to_string()));
        }
        let id: SharedString = format!("literal-{}-{block:?}-{ordinal}", self.namespace).into();
        self.collect(&id, &text, false, true);
        crate::rich::Literal {
            find: self.marks(&id),
            id,
            text,
            highlights,
            cache: self.cache.clone(),
            document: self.document.clone(),
        }
    }
}
