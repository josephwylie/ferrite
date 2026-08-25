//! The cockpit window: every open Pane at once, and the one pump behind them.
//!
//! Rendering and keys only. What each Pane shows — the Blocks, the pending
//! Decision, the held prompt — is folded in core and read from there.

use std::time::Duration;

use ferrite_core::cockpit::Cockpit;
use ferrite_core::store::Provider;
use ferrite_core::{DecisionAnswer, ThreadId};
use gpui::prelude::*;
use gpui::{actions, div, px, rgb, Context, Focusable, SharedString, Window};

use crate::pane::{self, PaneView};

actions!(
    cockpit,
    [
        Submit,
        Interrupt,
        Allow,
        Deny,
        Always,
        NextPane,
        PreviousPane,
        NextDecision,
        NewThread,
        CloseThread,
        ReopenThread,
    ]
);

/// How often the pump drains every Session. One timer for the whole cockpit,
/// not one per Pane: 24 Panes must cost one frame, not 24. 16ms is a
/// deliberate default — a frame the operator cannot see costs the same as one
/// they can — and the perf run raises it to compare with the spike's 8ms.
fn pump_interval() -> Duration {
    let ms = std::env::var("FERRITE_PUMP_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(PUMP_MS);
    Duration::from_millis(ms)
}

const PUMP_MS: u64 = 16;

pub struct CockpitView {
    cockpit: Cockpit,
    panes: Vec<PaneView>,
    focused: usize,
    perf: Option<Perf>,
    /// When the watchdog last swept. Sweeping costs a `ps` per live Session,
    /// so it runs on its own slow cadence, never per frame.
    swept: std::time::Instant,
}

/// How often the watchdog sweeps. Leaks grow over seconds, not frames; a
/// sweep per frame would spawn a `ps` per Session per tick.
const SWEEP_INTERVAL: Duration = Duration::from_secs(2);

/// The panes24 instrument, kept behind an env var: frames actually painted,
/// and what the process is holding while it paints them.
struct Perf {
    frames: u64,
    since: std::time::Instant,
}

impl CockpitView {
    pub fn new(cockpit: Cockpit, cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(pump_interval()).await;
            if this.update(cx, |view, cx| view.pump(cx)).is_err() {
                break;
            }
        })
        .detach();

        let panes = cockpit
            .threads()
            .into_iter()
            .map(|thread| PaneView::new(thread, cx))
            .collect();
        Self {
            cockpit,
            panes,
            focused: 0,
            perf: std::env::var("FERRITE_PERF").is_ok().then(|| Perf {
                frames: 0,
                since: std::time::Instant::now(),
            }),
            swept: std::time::Instant::now(),
        }
    }

    /// Count this frame and, once a second, say how it is going.
    fn measure(&mut self) {
        let panes = self.panes.len();
        let Some(perf) = &mut self.perf else {
            return;
        };
        perf.frames += 1;
        let elapsed = perf.since.elapsed().as_secs_f64();
        if elapsed < 1.0 {
            return;
        }
        let fps = perf.frames as f64 / elapsed;
        perf.frames = 0;
        perf.since = std::time::Instant::now();
        // stderr, not stdout: an instrument must survive the kill that ends
        // the run, and stdout to a file is block-buffered.
        eprintln!(
            "fps {fps:>6.1} | panes {panes:>3} | rss {:>7.1} MB",
            rss_mb()
        );
    }

    /// One frame for the whole cockpit. Only Panes the pump reports as
    /// changed are worth a repaint; a frame where nothing moved costs nothing.
    fn pump(&mut self, cx: &mut Context<Self>) {
        let frame = self.cockpit.pump();
        let mut restarted = false;
        if self.swept.elapsed() >= SWEEP_INTERVAL {
            self.swept = std::time::Instant::now();
            for restart in self.cockpit.sweep() {
                restarted = true;
                eprintln!(
                    "ferrite: restarted thread {} after {} bytes resident",
                    restart.thread, restart.rss
                );
            }
        }
        // A restart writes a Notice even when no Session streamed this frame —
        // and a failed respawn will never stream again, so this notify is that
        // notice's only ride to the screen.
        if frame.is_empty() && !restarted {
            return;
        }
        for update in &frame {
            if let Some(pane) = self.pane_for(update.thread) {
                // New content follows the tail; colour arriving late does not.
                if !update.dirty.is_empty() {
                    self.panes[pane].scroll.scroll_to_bottom();
                }
            }
        }
        cx.notify();
    }

    fn pane_for(&self, thread: ThreadId) -> Option<usize> {
        self.panes.iter().position(|pane| pane.thread == thread)
    }

    fn focused_thread(&self) -> Option<ThreadId> {
        Some(self.panes.get(self.focused)?.thread)
    }

    fn submit(&mut self, _: &Submit, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        let composer = self.panes[self.focused].composer.clone();
        let text = composer.update(cx, |composer, cx| composer.take(cx));
        let text = text.trim().to_string();
        if text.is_empty() {
            // Enter on an empty line takes a held prompt back to edit it.
            if let Some(held) = self.cockpit.unqueue(thread) {
                composer.update(cx, |composer, cx| composer.set(held, cx));
                cx.notify();
            }
            return;
        }
        // Typing does not wait for the agent; sending does.
        if self.cockpit.busy(thread) {
            self.cockpit.queue(thread, text);
        } else {
            self.cockpit.send(thread, text);
            self.panes[self.focused].scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    fn interrupt(&mut self, _: &Interrupt, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(thread) = self.focused_thread() {
            self.cockpit.interrupt(thread);
        }
        cx.notify();
    }

    fn allow(&mut self, _: &Allow, _window: &mut Window, cx: &mut Context<Self>) {
        self.answer(Answer::Allow, cx);
    }

    fn deny(&mut self, _: &Deny, _window: &mut Window, cx: &mut Context<Self>) {
        self.answer(Answer::Deny, cx);
    }

    fn always(&mut self, _: &Always, _window: &mut Window, cx: &mut Context<Self>) {
        self.answer(Answer::Always, cx);
    }

    fn answer(&mut self, answer: Answer, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        let Some(decision) = self.cockpit.pending(thread).cloned() else {
            return;
        };
        let response = match answer {
            Answer::Allow => DecisionAnswer::Allow {
                input: decision.input.clone(),
            },
            Answer::Deny => DecisionAnswer::Deny {
                message: "The operator denied this tool.".into(),
            },
            // Only where the request itself offered a standing answer; where
            // it did not, the key does nothing rather than quietly allowing.
            Answer::Always => match decision.standing_answer() {
                Some(standing) => DecisionAnswer::AllowAlways {
                    input: decision.input.clone(),
                    suggestion: standing.clone(),
                },
                None => return,
            },
        };
        self.cockpit.respond(thread, &decision, response);
        cx.notify();
    }

    fn next_pane(&mut self, _: &NextPane, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.panes.is_empty() {
            self.focused = (self.focused + 1) % self.panes.len();
            cx.notify();
        }
    }

    fn previous_pane(&mut self, _: &PreviousPane, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.panes.is_empty() {
            self.focused = (self.focused + self.panes.len() - 1) % self.panes.len();
            cx.notify();
        }
    }

    /// Open a Thread. The provider follows the Pane the operator is on, so a
    /// cockpit of Codex Threads keeps growing Codex Threads.
    fn new_thread(&mut self, _: &NewThread, _window: &mut Window, cx: &mut Context<Self>) {
        let provider = self
            .focused_thread()
            .and_then(|thread| self.cockpit.provider(thread))
            .unwrap_or(Provider::Claude);
        match self.cockpit.open(provider) {
            Ok(thread) => {
                self.panes.push(PaneView::new(thread, cx));
                self.focused = self.panes.len() - 1;
                cx.notify();
            }
            Err(e) => eprintln!("ferrite: could not open a thread: {e}"),
        }
    }

    /// Reopen the Thread parked most recently — the one the operator just
    /// closed, which is the one they want back. Choosing among older ones
    /// wants a picker, and that is not this ticket.
    fn reopen_thread(&mut self, _: &ReopenThread, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.cockpit.parked().unwrap_or_default().last().copied() else {
            return;
        };
        match self.cockpit.revive(thread) {
            Ok(()) => {
                self.panes.push(PaneView::new(thread, cx));
                self.focused = self.panes.len() - 1;
                cx.notify();
            }
            Err(e) => eprintln!("ferrite: thread {thread} could not be reopened: {e:?}"),
        }
    }

    /// Close a Pane: the Thread parks — its Session ends, its log stays, and
    /// reopening it revives the Thread.
    fn close_thread(&mut self, _: &CloseThread, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        if let Err(e) = self.cockpit.park(thread) {
            eprintln!("ferrite: thread {thread} did not park cleanly: {e}");
        }
        self.panes.retain(|pane| pane.thread != thread);
        self.focused = self.focused.min(self.panes.len().saturating_sub(1));
        cx.notify();
    }

    /// Jump to the next Thread waiting on the operator — the whole point of
    /// a wall you cannot read all of at once.
    fn next_decision(&mut self, _: &NextDecision, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(next) = self.cockpit.next_blocked(self.focused_thread()) else {
            return;
        };
        if let Some(pane) = self.pane_for(next) {
            self.focused = pane;
            cx.notify();
        }
    }
}

enum Answer {
    Allow,
    Deny,
    Always,
}

/// Columns for `count` Panes: near-square, and never wider than the 24-pane
/// wall's six.
fn columns(count: usize) -> usize {
    (count as f64).sqrt().ceil().clamp(1.0, 6.0) as usize
}

impl Render for CockpitView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.measure();
        let blocked = self.cockpit.blocked();
        let columns = columns(self.panes.len());
        let visible = pane::visible_blocks(self.panes.len());

        // Focus follows the operator: a pending Decision takes the keyboard so
        // y/n are answers, otherwise the focused Pane's Composer has it.
        if let Some(pane) = self.panes.get(self.focused) {
            let wanted = if self.cockpit.pending(pane.thread).is_some() {
                pane.decision_focus.clone()
            } else {
                pane.composer.focus_handle(cx)
            };
            if !wanted.is_focused(window) {
                window.focus(&wanted);
            }
        }

        let mut grid = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(6.))
            .p(px(6.));
        for row in self.panes.chunks(columns) {
            let mut line = div().flex().flex_row().flex_1().min_h_0().gap(px(6.));
            for pane in row {
                let focused = self
                    .focused_thread()
                    .is_some_and(|thread| thread == pane.thread);
                line = line.child(div().flex().flex_col().flex_1().min_w_0().min_h_0().child(
                    pane::render_pane(
                        pane,
                        self.cockpit.transcript(pane.thread),
                        self.cockpit.pending(pane.thread),
                        self.cockpit.queued(pane.thread),
                        focused,
                        blocked.contains(&pane.thread),
                        visible,
                    ),
                ));
            }
            grid = grid.child(line);
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(pane::BG_WINDOW))
            .font_family("Menlo")
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::interrupt))
            .on_action(cx.listener(Self::allow))
            .on_action(cx.listener(Self::deny))
            .on_action(cx.listener(Self::always))
            .on_action(cx.listener(Self::next_pane))
            .on_action(cx.listener(Self::previous_pane))
            .on_action(cx.listener(Self::next_decision))
            .on_action(cx.listener(Self::new_thread))
            .on_action(cx.listener(Self::close_thread))
            .on_action(cx.listener(Self::reopen_thread))
            .child(self.strip(blocked.len()))
            .child(grid)
    }
}

impl CockpitView {
    /// One line across the top: how many Threads, how many want answering.
    fn strip(&self, blocked: usize) -> impl IntoElement {
        let threads = self.panes.len();
        let waiting = if blocked == 0 {
            SharedString::from("all quiet")
        } else {
            SharedString::from(format!("{blocked} waiting on you"))
        };
        div()
            .flex()
            .flex_shrink_0()
            .justify_between()
            .px(px(8.))
            .py(px(4.))
            .text_size(px(11.))
            .child(
                div()
                    .text_color(rgb(pane::TEXT_MUTED))
                    .child(SharedString::from(format!("{threads} threads"))),
            )
            .child(
                div()
                    .text_color(rgb(if blocked == 0 {
                        pane::TEXT_MUTED
                    } else {
                        pane::TEXT_NOTICE
                    }))
                    .child(waiting),
            )
    }
}

/// This process's resident memory, for the perf print.
fn rss_mb() -> f64 {
    crate::session::rss_bytes(std::process::id())
        .map(|bytes| bytes as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0)
}

/// Fill the cockpit: revive the Threads this store already has — newest
/// first, because that is what the operator was last looking at — and open
/// new ones for whatever room is left.
pub fn threads_for(cockpit: &mut Cockpit, wanted: usize, provider: Provider) -> Vec<ThreadId> {
    let mut shown = Vec::new();
    let mut parked = cockpit.parked().unwrap_or_default();
    parked.reverse();
    for thread in parked.into_iter().take(wanted) {
        match cockpit.revive(thread) {
            Ok(()) => shown.push(thread),
            Err(e) => eprintln!("ferrite: thread {thread} could not be revived: {e:?}"),
        }
    }
    while shown.len() < wanted {
        match cockpit.open(provider) {
            Ok(id) => shown.push(id),
            Err(e) => {
                eprintln!("ferrite: could not open a thread: {e}");
                break;
            }
        }
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::mpsc::{self, Receiver, Sender};

    use ferrite_core::cockpit::Spawner;
    use ferrite_core::providers::Session;
    use ferrite_core::store::Store;
    use ferrite_core::{Decision, SessionEvent};
    use gpui::{KeyBinding, TestAppContext};

    struct Scripted {
        rx: Receiver<SessionEvent>,
    }

    impl Session for Scripted {
        fn events(&self) -> &Receiver<SessionEvent> {
            &self.rx
        }
        fn send(&mut self, _text: &str) -> std::io::Result<()> {
            Ok(())
        }
        fn interrupt(&mut self) -> std::io::Result<()> {
            Ok(())
        }
        fn respond_to_decision(
            &mut self,
            _id: &str,
            _answer: DecisionAnswer,
        ) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct Fake {
        streams: Rc<RefCell<Vec<Sender<SessionEvent>>>>,
    }

    impl Spawner for Fake {
        fn spawn(
            &mut self,
            _provider: Provider,
            _resume: Option<&str>,
        ) -> std::io::Result<Box<dyn Session>> {
            let (tx, rx) = mpsc::channel();
            self.streams.borrow_mut().push(tx);
            Ok(Box::new(Scripted { rx }))
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-view-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn cockpit(name: &str, panes: usize) -> (Cockpit, Fake) {
        let fake = Fake::default();
        let store = Store::open(scratch(name)).unwrap();
        let mut cockpit = Cockpit::new(store, Box::new(fake.clone()));
        for _ in 0..panes {
            cockpit.open(Provider::Claude).unwrap();
        }
        (cockpit, fake)
    }

    /// Let the pump's timer fire: the test clock does not move on its own.
    fn tick(cx: &mut gpui::VisualTestContext) {
        cx.executor()
            .advance_clock(Duration::from_millis(PUMP_MS * 4));
        cx.run_until_parked();
    }

    fn decision(id: &str) -> SessionEvent {
        SessionEvent::DecisionRequested {
            decision: Decision {
                id: id.into(),
                tool_use_id: "toolu_1".into(),
                tool_name: "Write".into(),
                description: "ferrite-perm.txt".into(),
                input: serde_json::Value::Null,
                suggestions: vec![],
            },
        }
    }

    /// The whole keystroke path in a real window: a blocked Pane, one key, and
    /// the Decision gone because the answer went out.
    #[gpui::test]
    fn one_keystroke_answers_the_card(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("answer", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        fake.streams.borrow()[0].send(decision("perm_01")).unwrap();
        tick(cx);
        view.read_with(cx, |view, _| {
            let thread = view.panes[0].thread;
            assert!(
                view.cockpit.pending(thread).is_some(),
                "the card should be up before the key"
            );
        });

        cx.simulate_keystrokes("y");

        view.read_with(cx, |view, _| {
            let thread = view.panes[0].thread;
            assert!(
                view.cockpit.pending(thread).is_none(),
                "y must answer the Decision, not type a letter"
            );
        });
    }

    /// AC1 at the keyboard: closing a Pane parks its Thread — the Session
    /// ends, the log stays, and the store still has it to reopen.
    #[gpui::test]
    fn closing_a_pane_parks_the_thread_rather_than_losing_it(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("park", 2);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-w", CloseThread, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        let closed = view.read_with(cx, |view, _| view.panes[0].thread);

        cx.simulate_keystrokes("cmd-w");

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1, "the Pane is gone");
            assert!(
                view.cockpit.transcript(closed).is_none(),
                "and so is its memory"
            );
            assert!(
                view.cockpit.parked().unwrap().contains(&closed),
                "but the Thread is still there to reopen"
            );
        });
    }

    /// AC1's other half: a parked Thread comes back into the running cockpit,
    /// with its history and the marker saying what it is.
    #[gpui::test]
    fn reopening_brings_a_parked_thread_back_with_its_history(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("reopen", 2);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-w", CloseThread, None),
                KeyBinding::new("cmd-o", ReopenThread, None),
            ]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        let closed = view.read_with(cx, |view, _| view.panes[0].thread);
        cx.simulate_keystrokes("cmd-w");
        view.read_with(cx, |view, _| assert_eq!(view.panes.len(), 1));

        cx.simulate_keystrokes("cmd-o");

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 2, "the Pane is back");
            assert!(
                view.panes.iter().any(|pane| pane.thread == closed),
                "and it is the same Thread, not a new one"
            );
            let blocks = view
                .cockpit
                .transcript(closed)
                .expect("its transcript")
                .blocks();
            assert!(
                blocks.iter().any(|block| matches!(
                    &block.body,
                    ferrite_core::transcript::Body::Meta(line)
                        if line.starts_with("revived")
                )),
                "a revived Pane must not pretend it never died: {blocks:?}"
            );
        });
    }

    /// With nothing blocked, the answer keys are letters again.
    #[gpui::test]
    fn the_answer_keys_are_letters_when_nothing_is_blocked(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("letters", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);

        cx.simulate_keystrokes("y");

        let typed = view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.take(cx))
        });
        assert_eq!(typed, "y");
    }

    /// AC3: one key walks to whoever is waiting, wherever they are in the grid.
    #[gpui::test]
    fn one_key_jumps_to_the_pane_that_needs_answering(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("jump", 4);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-d", NextDecision, None),
                KeyBinding::new("cmd-]", NextPane, None),
            ]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        fake.streams.borrow()[2].send(decision("perm_03")).unwrap();
        tick(cx);
        view.read_with(cx, |view, _| assert_eq!(view.focused, 0));

        cx.simulate_keystrokes("cmd-d");

        view.read_with(cx, |view, _| {
            assert_eq!(view.focused, 2, "focus should land on the blocked Pane");
        });

        // And plain cycling still walks the grid in order.
        cx.simulate_keystrokes("cmd-]");
        view.read_with(cx, |view, _| assert_eq!(view.focused, 3));
    }
}
