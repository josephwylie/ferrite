//! What the API reports of a Thread beyond the Engine's own facts (ADR
//! 0030): what only the windows know — the Thread the operator is looking
//! at, the Composers holding unsent text — and how often each Thread's
//! Attention changed.

use super::*;
use crate::protocol::Op;
use crate::Attention;

/// A window, by the number its Server knows it by (`link::ClientId`).
pub type WindowId = u64;

/// What the windows said of themselves.
#[derive(Default)]
pub(super) struct Windows {
    /// The Thread the focused window's focused Pane shows, and that window.
    focused: Option<(WindowId, ThreadId)>,
    /// Each window's Composers that hold text not yet sent.
    composing: BTreeSet<(WindowId, ThreadId)>,
}

/// Each open Thread's Attention as last counted, and how many changes it
/// has been counted through: Herdr's `state_change_seq`, for telling that
/// something happened between two looks. Counted from this Server's start.
#[derive(Default)]
pub(super) struct AttentionCount(HashMap<ThreadId, (Attention, u64)>);

impl Engine {
    /// A window's word about itself (`Op::Focused`, `Op::Composing`);
    /// anything else is not one, and is passed over.
    pub fn window_says(&mut self, window: WindowId, op: &Op) {
        let windows = &mut self.windows;
        match op {
            Op::Focused {
                thread: Some(thread),
            } => windows.focused = Some((window, *thread)),
            Op::Focused { thread: None } => {
                if windows
                    .focused
                    .is_some_and(|(focused, _)| focused == window)
                {
                    windows.focused = None;
                }
            }
            Op::Composing { thread, unsent } => {
                if *unsent {
                    windows.composing.insert((window, *thread));
                } else {
                    windows.composing.remove(&(window, *thread));
                }
            }
            _ => {}
        }
    }

    /// The window is gone, and what it said with it.
    pub fn window_gone(&mut self, window: WindowId) {
        let windows = &mut self.windows;
        if windows
            .focused
            .is_some_and(|(focused, _)| focused == window)
        {
            windows.focused = None;
        }
        windows.composing.retain(|(said, _)| *said != window);
    }

    /// The Thread the operator is looking at: shown in the focused Pane of
    /// the window that has their focus.
    pub fn focused_thread(&self) -> Option<ThreadId> {
        self.windows.focused.map(|(_, thread)| thread)
    }

    /// Whether some window's Composer for `thread` holds unsent text.
    pub fn composing(&self, thread: ThreadId) -> bool {
        self.windows
            .composing
            .iter()
            .any(|(_, composing)| *composing == thread)
    }

    /// How many times `thread`'s Attention has been seen to change since
    /// this Server started (`count_attention`).
    pub fn attention_seq(&self, thread: ThreadId) -> u64 {
        self.attention_count
            .0
            .get(&thread)
            .map_or(0, |(_, seq)| *seq)
    }

    /// Count each open Thread's Attention change since the last count; a
    /// closed Thread's count goes with it. Run with every publish, so a
    /// change any client hears of is counted.
    pub(super) fn count_attention(&mut self) {
        let open: Vec<ThreadId> = self.threads.keys().copied().collect();
        let mut counted = std::mem::take(&mut self.attention_count.0);
        counted.retain(|thread, _| open.contains(thread));
        for thread in open {
            let Some(attention) = self.attention(thread) else {
                continue;
            };
            counted
                .entry(thread)
                .and_modify(|(was, seq)| {
                    if *was != attention {
                        *was = attention;
                        *seq += 1;
                    }
                })
                .or_insert((attention, 0));
        }
        self.attention_count.0 = counted;
    }
}
