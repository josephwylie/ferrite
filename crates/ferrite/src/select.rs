//! Stable identities for native transcript text views. GPUI owns selection,
//! hit testing, highlight painting and copy; Ferrite only identifies runs.
use ferrite_core::{
    transcript::{Block, BlockId},
    ThreadId,
};
use gpui::{HighlightStyle, SharedString};
use std::{cell::RefCell, ops::Range};

#[cfg(test)]
type Registry =
    std::rc::Rc<RefCell<std::collections::HashMap<ThreadId, Vec<(BlockId, u32, bool, String)>>>>;

#[derive(Default)]
pub struct TranscriptText {
    #[cfg(test)]
    fallback: crate::rich::TextCache,
    #[cfg(test)]
    registry: Registry,
}
impl TranscriptText {
    #[cfg(test)]
    pub fn overlay(&self, thread: ThreadId, blocks: &[Block]) -> TextRuns {
        self.overlay_scoped(
            thread,
            thread.get().to_string().into(),
            blocks,
            self.fallback.clone(),
        )
    }
    pub fn overlay_scoped(
        &self,
        _thread: ThreadId,
        namespace: SharedString,
        _: &[Block],
        cache: crate::rich::TextCache,
    ) -> TextRuns {
        #[cfg(test)]
        self.registry.borrow_mut().insert(_thread, Vec::new());
        TextRuns {
            #[cfg(test)]
            thread: _thread,
            namespace,
            cache,
            block: RefCell::new(None),
            next_ordinal: RefCell::new(0),
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
    #[cfg(test)]
    registry: Registry,
}
impl TextRuns {
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
        self.registry
            .borrow_mut()
            .entry(self.thread)
            .or_default()
            .push((block, ordinal, true, text.to_string()));
        crate::rich::Literal {
            id: format!("literal-{}-{block:?}-{ordinal}", self.namespace).into(),
            text,
            highlights,
            cache: self.cache.clone(),
        }
    }
}
