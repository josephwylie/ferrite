//! Prompts on their way back to a Composer (ADR 0041): one the agent
//! handed back, a held one taken back to edit, and a step through a
//! Thread's prompt history. Each is asked of the Server and put in the
//! Composer when the answer comes; the window never waits for it.

use super::*;
use ferrite_client::cockpit::HistoryDirection;
use ferrite_core::roster::PaneIdentity;
use ferrite_core::ThreadKey;
use std::collections::{HashMap, HashSet, VecDeque};

/// What is asked for and not yet answered, by Thread.
#[derive(Default)]
pub(crate) struct Returning {
    /// The Threads asked for the prompt their agent handed back: the pump
    /// looks every frame, and asks each once at a time.
    handed_back: HashSet<ThreadKey>,
    /// The history steps pressed on each Thread, oldest first: the first
    /// is asked, the rest wait for its answer, since each step starts from
    /// what the one before it put in the Composer.
    steps: HashMap<ThreadKey, VecDeque<HistoryDirection>>,
}

impl CockpitView {
    #[cfg(test)]
    pub(crate) fn returning_steps(&self, thread: ThreadKey) -> usize {
        self.returning.steps.get(&thread).map_or(0, VecDeque::len)
    }

    /// The Composer of the Pane that shows `thread`.
    fn composer_of(&self, thread: ThreadKey, cx: &gpui::App) -> Option<Entity<Composer>> {
        let index = self.index_of(PaneIdentity::Thread(thread))?;
        Some(self.panes[index].read(cx).composer.clone())
    }

    /// Each Thread on screen whose agent handed a prompt back is asked
    /// for it: the pump's, every frame.
    pub(super) fn take_handed_back_prompts(&mut self, cx: &mut Context<Self>) {
        let shown: Vec<ThreadKey> = self.panes.iter().filter_map(|pane| pane.thread()).collect();
        for thread in shown {
            self.take_handed_back(thread, cx);
        }
    }

    fn take_handed_back(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        if self.returning.handed_back.contains(&thread) {
            return;
        }
        let Some(asked) = self.cockpit.update(cx, |cockpit, _| cockpit.take_retrieved_prompt(thread)) else {
            return;
        };
        self.returning.handed_back.insert(thread);
        self.when_answered(
            asked,
            move |view, taken, cx| {
                view.returning.handed_back.remove(&thread);
                let Ok(Some((held, prepend))) = taken else {
                    return;
                };
                match view.composer_of(thread, cx) {
                    Some(composer) => {
                        let content = view.recovered_content(thread, held);
                        composer.update(cx, |composer, cx| composer.restore_content(content, prepend, cx));
                        cx.notify();
                    }
                    None => ferrite_core::log!(
                        "a prompt handed back to thread {} had no Composer to go to",
                        thread.thread()
                    ),
                }
                // A Thread can hold more than one.
                view.take_handed_back(thread, cx);
            },
            cx,
        );
    }

    /// Enter on an empty line takes `thread`'s held prompt back to edit.
    pub(super) fn take_back_held(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let asked = self.cockpit.update(cx, |cockpit, _| cockpit.unqueue(thread));
        self.when_answered(
            asked,
            move |view, held, cx| {
                let (Ok(Some(held)), Some(composer)) = (held, view.composer_of(thread, cx)) else {
                    return;
                };
                let content = view.recovered_content(thread, held);
                composer.update(cx, |composer, cx| composer.restore_content(content, true, cx));
                cx.notify();
            },
            cx,
        );
    }

    /// A step through `thread`'s prompt history: now, or once the steps
    /// pressed before it are answered.
    pub(super) fn step_history_of(&mut self, thread: ThreadKey, direction: HistoryDirection, cx: &mut Context<Self>) {
        let steps = self.returning.steps.entry(thread).or_default();
        steps.push_back(direction);
        if steps.len() == 1 {
            self.ask_next_step(thread, cx);
        }
    }

    /// Ask for `thread`'s oldest step pressed, from what its Composer
    /// holds now.
    fn ask_next_step(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let next = self.returning.steps.get(&thread).and_then(|steps| steps.front().copied());
        let (Some(direction), Some(composer)) = (next, self.composer_of(thread, cx)) else {
            self.returning.steps.remove(&thread);
            return;
        };
        let draft = composer.read(cx).editable();
        self.unsent
            .step_from(thread, draft.clone(), composer.read(cx).content());
        let asked = self.cockpit.update(cx, |cockpit, _| cockpit.recall_prompt(thread, direction, &draft));
        self.when_answered(
            asked,
            move |view, stepped, cx| {
                if let Some(steps) = view.returning.steps.get_mut(&thread) {
                    steps.pop_front();
                }
                if let (Ok(Some(text)), Some(composer)) = (stepped, view.composer_of(thread, cx)) {
                    view.composer_menu.update(cx, |menu, cx| menu.recalled(cx));
                    let content = view.unsent.stepped_to(text);
                    composer.update(cx, |composer, cx| composer.put(content, cx));
                    cx.notify();
                }
                view.ask_next_step(thread, cx);
            },
            cx,
        );
    }
}
