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
    registry: Registry,
}
impl TranscriptText {
    pub fn overlay(&self, thread: ThreadId, _: &[Block]) -> TextRuns {
        #[cfg(test)]
        self.registry.borrow_mut().insert(thread, Vec::new());
        TextRuns {
            thread,
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
    thread: ThreadId,
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
            id: format!("literal-{}-{block:?}-{ordinal}", self.thread.get()).into(),
            text,
            highlights,
        }
    }
}
