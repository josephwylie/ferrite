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

/// Each open Thread's Attention as last counted, and the change it was
/// last counted at: Herdr's `state_change_seq`, for telling that something
/// happened between two looks. One count for the whole boot, never reset:
/// a Thread closing is a change, so one revived after its park counts on
/// past every number it carried before.
#[derive(Default)]
pub(super) struct AttentionCount {
    /// The changes counted this boot, every Thread's.
    seq: u64,
    threads: HashMap<ThreadId, Counted>,
}

/// One Thread's count.
#[derive(Clone, Copy)]
pub(super) struct Counted {
    attention: Attention,
    seq: u64,
    /// `seq` when its last turn ended: Herdr's `completion_seq`.
    completed: Option<u64>,
}

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
                // Said late, of a Thread that has closed: nothing to hold.
                if *unsent && self.threads.contains_key(thread) {
                    windows.composing.insert((window, *thread));
                    self.wake(*thread);
                } else {
                    windows.composing.remove(&(window, *thread));
                }
            }
            _ => {}
        }
    }

    /// Start the agent of a Thread the operator is writing to, if it has
    /// none, so it is ready for the prompt (ADR 0032). A start that fails
    /// is left for the send to retry and report.
    fn wake(&mut self, thread: ThreadId) {
        if self
            .threads
            .get(&thread)
            .is_some_and(|state| state.session.is_none())
        {
            if let Err(error) = self.start_session(thread) {
                crate::log!("thread {thread}: its agent could not start early: {error}");
            }
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

    /// A closed Thread's Composers went with its Panes: no window holds a
    /// draft for it, and none is reported when it is revived.
    pub(super) fn forget_closed_composing(&mut self) {
        let threads = &self.threads;
        self.windows
            .composing
            .retain(|(_, thread)| threads.contains_key(thread));
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

    /// The change `thread`'s Attention was last counted at this boot
    /// (`count_attention`): a higher number means something happened since.
    /// A Thread opened since the last count (a revive's answer) is at the
    /// count it will be counted at, past every number it carried.
    pub fn attention_seq(&self, thread: ThreadId) -> u64 {
        let count = &self.attention_count;
        count
            .threads
            .get(&thread)
            .map_or(count.seq, |counted| counted.seq)
    }

    /// `attention_seq` as it was when `thread`'s last turn ended: its
    /// Attention came to rest (idle or done) from work (working or
    /// blocked). `None` until a turn has ended since it last opened.
    pub fn completion_seq(&self, thread: ThreadId) -> Option<u64> {
        self.attention_count
            .threads
            .get(&thread)
            .and_then(|counted| counted.completed)
    }

    /// A Thread closed: a change, its count gone with it, the boot's kept.
    /// Counted as it parks, so a revive before the next count starts anew.
    pub(super) fn count_closed(&mut self, thread: ThreadId) {
        let count = &mut self.attention_count;
        if count.threads.remove(&thread).is_some() {
            count.seq += 1;
        }
    }

    /// Count each open Thread's Attention change since the last count, and
    /// each Thread closed since (`count_closed`). Run with every publish,
    /// so a change any client hears of is counted.
    pub(super) fn count_attention(&mut self) {
        let open: Vec<ThreadId> = self.threads.keys().copied().collect();
        let mut count = std::mem::take(&mut self.attention_count);
        let counted = count.threads.len();
        count.threads.retain(|thread, _| open.contains(thread));
        count.seq += (counted - count.threads.len()) as u64;
        for thread in open {
            let Some(attention) = self.attention(thread) else {
                continue;
            };
            let Some(counted) = count.threads.get_mut(&thread) else {
                count.threads.insert(
                    thread,
                    Counted {
                        attention,
                        seq: count.seq,
                        completed: None,
                    },
                );
                continue;
            };
            if counted.attention == attention {
                continue;
            }
            let at_work = |a| matches!(a, Attention::Working | Attention::Blocked);
            count.seq += 1;
            counted.seq = count.seq;
            if at_work(counted.attention) && !at_work(attention) {
                counted.completed = Some(count.seq);
            }
            counted.attention = attention;
        }
        self.attention_count = count;
    }
}
