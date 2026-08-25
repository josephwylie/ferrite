//! The cockpit window: every open Pane at once, and the one pump behind them.
//!
//! Rendering and keys only. What each Pane shows — the Blocks, the pending
//! Decision, the held prompt — is folded in core and read from there.

use std::time::Duration;

use ferrite_core::cockpit::Cockpit;
use ferrite_core::docview::{Cell, Level};
use ferrite_core::store::Provider;
use ferrite_core::transcript::{Block, BlockId};
use ferrite_core::workspace::WorkspaceChoice;
use ferrite_core::{DecisionAnswer, ThreadId};
use gpui::prelude::*;
use gpui::{
    actions, div, point, px, rgb, ClipboardItem, Context, FocusHandle, Focusable, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollHandle, SharedString,
    Window,
};

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
        NewWorktreeThread,
        CloseThread,
        ReopenThread,
        CopySelection,
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
    /// The repo a new Thread binds to — where Ferrite was started.
    repo: std::path::PathBuf,
    /// The cockpit's own place in the focus tree. Key dispatch walks from the
    /// focused node up to the root, so with nothing focused inside the window
    /// the cockpit's own actions are never reached — at wall range, where no
    /// Pane holds a Composer, this handle is what keeps the keyboard alive.
    focus: FocusHandle,
    perf: Option<Perf>,
    /// Threads the operator parked this launch, oldest first — cmd-o pops the
    /// tail, the one just closed. In memory only, deliberately: the store
    /// keeps no park order, so a relaunch forgets it and reopen falls back to
    /// creation order (accepted v1 behavior).
    park_order: Vec<ThreadId>,
    /// When the watchdog last swept. Sweeping costs a `ps`/`tasklist` per
    /// live Session, so it runs on its own slow cadence, never per frame.
    swept: std::time::Instant,
    /// A left press on a transcript, holding the Block it landed on while
    /// the button stays down — the anchor a drag turns into a selection.
    grip: Option<(ThreadId, BlockId)>,
    /// The one live selection. A plain click never makes one; only a drag
    /// does, and the next press anywhere clears it.
    selection: Option<Selection>,
}

/// Whole Blocks swept by a drag across one Pane's transcript — the unit v1
/// selection copies. gpui 0.2.2 has no selection over rendered text (that
/// lives in Zed's own editor element), so character-level selection would
/// need an editor-grade element; Blocks are the honest unit this transcript
/// has. `anchor` and `head` are BlockIds, in drag order, not sorted — never
/// positions: eviction drains old Blocks off the front of the transcript,
/// and a stored position would quietly slide onto Blocks the operator never
/// touched.
#[derive(PartialEq)]
struct Selection {
    thread: ThreadId,
    anchor: BlockId,
    head: BlockId,
}

impl Selection {
    /// Where the selection sits in this transcript now. An endpoint whose
    /// Block was evicted clamps to the window start — eviction eats the
    /// oldest end first, so everything up to the surviving endpoint was
    /// swept too. With both ends gone the selection is gone, rather than
    /// resurrecting on unrelated Blocks.
    fn resolve(&self, blocks: &[Block]) -> Option<std::ops::RangeInclusive<usize>> {
        let anchor = blocks.iter().position(|block| block.id == self.anchor);
        let head = blocks.iter().position(|block| block.id == self.head);
        match (anchor, head) {
            (Some(anchor), Some(head)) => Some(anchor.min(head)..=anchor.max(head)),
            (Some(kept), None) | (None, Some(kept)) => Some(0..=kept),
            (None, None) => None,
        }
    }
}

/// How near the tail still counts as riding it. It must swallow the
/// transcript's own padding — gpui reports a not-yet-overflowing scroll as
/// having exactly that much room, the py(5.) above and below the rows in
/// `pane::body` — while staying under one text line, so a deliberate scroll
/// still detaches.
const TAIL_SLACK: Pixels = px(12.);

/// Whether this scrollback is riding the tail. An operator who wheeled up is
/// reading history: new content must not yank them down until they scroll
/// back to the bottom (the standard terminal contract). The offset runs
/// negative as the view descends, so at the tail it equals -max.
fn follows_tail(scroll: &ScrollHandle) -> bool {
    scroll.max_offset().height + scroll.offset().y <= TAIL_SLACK
}

/// How often the watchdog sweeps. Leaks grow over seconds, not frames; a
/// sweep per frame would spawn a `ps`/`tasklist` per Session per tick.
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
            repo: here(),
            focus: cx.focus_handle(),
            perf: std::env::var("FERRITE_PERF").is_ok().then(|| Perf {
                frames: 0,
                since: std::time::Instant::now(),
            }),
            park_order: Vec::new(),
            swept: std::time::Instant::now(),
            grip: None,
            selection: None,
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
                // New content follows the tail; colour arriving late does
                // not, and neither does an operator who scrolled back into
                // history — they reattach by scrolling to the bottom.
                if !update.dirty.is_empty() && follows_tail(&self.panes[pane].scroll) {
                    self.panes[pane].scroll.scroll_to_bottom();
                }
            }
        }
        cx.notify();
    }

    /// One cell of the grid, as the window is right now. Size is the only
    /// input semantic zoom takes — there is no mode to switch.
    fn cell(&self, window: &Window, columns: usize) -> Cell {
        let viewport = window.viewport_size();
        let rows = self.panes.len().div_ceil(columns).max(1);
        // The strip, the grid's own padding, and the gaps between cells are
        // not the Pane's to render in.
        let width = (f32::from(viewport.width) - 12.0) / columns as f32 - 6.0;
        let height = (f32::from(viewport.height) - 34.0) / rows as f32 - 6.0;
        Cell::new(width.max(0.0), height.max(0.0))
    }

    /// The level this cockpit is rendering at right now — size, nothing else.
    fn level_now(&self, window: &Window) -> Level {
        Level::for_cell(self.cell(window, columns(self.panes.len())))
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
        // The focused Thread if it is the one waiting; otherwise whichever
        // Thread the wall is flagging. Answering from across the room is the
        // point of the badge.
        let thread = match self.focused_thread() {
            Some(thread) if self.cockpit.pending(thread).is_some() => Some(thread),
            _ => self.cockpit.next_blocked(None),
        };
        let Some(thread) = thread else {
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

    /// A Thread in its own worktree: isolated from the operator's checkout and
    /// from every other Thread, which is the whole point of the binding.
    fn new_worktree_thread(
        &mut self,
        _: &NewWorktreeThread,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_thread(
            WorkspaceChoice::Worktree {
                repo: self.repo.clone(),
            },
            cx,
        );
    }

    /// Open a Thread. The provider follows the Pane the operator is on, so a
    /// cockpit of Codex Threads keeps growing Codex Threads.
    fn new_thread(&mut self, _: &NewThread, _window: &mut Window, cx: &mut Context<Self>) {
        self.start_thread(
            WorkspaceChoice::Main {
                checkout: self.repo.clone(),
            },
            cx,
        );
    }

    fn start_thread(&mut self, workspace: WorkspaceChoice, cx: &mut Context<Self>) {
        let provider = self
            .focused_thread()
            .and_then(|thread| self.cockpit.provider(thread))
            .unwrap_or(Provider::Claude);
        match self.cockpit.open(provider, workspace) {
            Ok(thread) => {
                self.panes.push(PaneView::new(thread, cx));
                self.focused = self.panes.len() - 1;
                cx.notify();
            }
            // A worktree that git refuses is the operator's to fix; the
            // cockpit says so rather than opening a Thread somewhere else.
            Err(e) => eprintln!("ferrite: could not open a Thread: {e}"),
        }
    }

    /// Reopen the Thread parked most recently — the one the operator just
    /// closed, which is the one they want back. The order is remembered only
    /// for this launch: once it is drained — Threads parked before a relaunch
    /// are never in it — the newest-created parked Thread is next (accepted
    /// v1 behavior). Choosing among older ones wants a picker, and that is
    /// not this ticket.
    fn reopen_thread(&mut self, _: &ReopenThread, _window: &mut Window, cx: &mut Context<Self>) {
        // A Thread whose revive fails below keeps its park but loses its
        // slot in the order: cmd-o moves on rather than jamming on it, and
        // the creation-order fallback still reaches it.
        let Some(thread) = self
            .park_order
            .pop()
            .or_else(|| self.cockpit.parked().unwrap_or_default().last().copied())
        else {
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
        // Parked even on a flush error — the Session is gone either way, so
        // cmd-o should still bring this Thread back first.
        self.park_order.push(thread);
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

    /// A left press lands the operator on this Pane. It only sets `focused`:
    /// the per-frame snap in render then carries focus to whatever the Pane
    /// holds (Composer or Decision card) — fighting the snap would regress
    /// the dead-keyboard fixes it exists for. A press on the transcript also
    /// grips a Block, ready to become a selection if the pointer moves.
    fn pointer_down(
        &mut self,
        index: usize,
        position: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.focused = index;
        // Any press clears the standing selection; only a new drag makes one.
        // The grip was already cleared by the root's capture handler.
        self.selection = None;
        self.grip = self.panes.get(index).and_then(|pane| {
            let at = self.block_at(index, position, window)?;
            let block = self.cockpit.transcript(pane.thread)?.blocks().get(at)?;
            Some((pane.thread, block.id))
        });
        cx.notify();
    }

    /// Dragging with the button held sweeps whole Blocks into the selection.
    /// A drag stays in the Pane it started in; sweeping across a neighbour
    /// freezes it rather than selecting someone else's transcript.
    fn pointer_drag(
        &mut self,
        index: usize,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some((thread, anchor)) = self.grip else {
            return;
        };
        let Some(pane) = self.panes.get(index) else {
            return;
        };
        if pane.thread != thread {
            return;
        }
        // Clamp into the transcript so a drag past its edge still reaches
        // the first or last visible Block instead of going dead.
        let bounds = pane.scroll.bounds();
        if bounds.size.width < px(2.) || bounds.size.height < px(2.) {
            return;
        }
        let position = point(
            event
                .position
                .x
                .clamp(bounds.left(), bounds.right() - px(1.)),
            event
                .position
                .y
                .clamp(bounds.top(), bounds.bottom() - px(1.)),
        );
        let Some(head) = self.block_at(index, position, window).and_then(|at| {
            let blocks = self.cockpit.transcript(thread)?.blocks();
            Some(blocks.get(at)?.id)
        }) else {
            return;
        };
        let next = Some(Selection {
            thread,
            anchor,
            head,
        });
        if self.selection != next {
            self.selection = next;
            cx.notify();
        }
    }

    /// The Block under a window position in one Pane's transcript, as an
    /// index into the Thread's blocks — None outside the transcript, and
    /// None at wall range, where a Pane draws no text to select.
    fn block_at(&self, index: usize, position: Point<Pixels>, window: &Window) -> Option<usize> {
        let visible = self.level_now(window).visible_blocks();
        if visible == 0 {
            return None;
        }
        let pane = self.panes.get(index)?;
        let bounds = pane.scroll.bounds();
        if !bounds.contains(&position) {
            return None;
        }
        let blocks = self.cockpit.transcript(pane.thread)?.blocks().len();
        if blocks == 0 {
            return None;
        }
        let tail = blocks.saturating_sub(visible);
        // Rows are recorded unscrolled: the offset moves the content under a
        // fixed viewport, so the position maps back by subtracting it.
        let y = position.y - pane.scroll.offset().y;
        let shown = blocks - tail;
        let mut child = shown - 1;
        for row in 0..shown {
            match pane.scroll.bounds_for_item(row) {
                // The first row whose bottom edge is below the pointer is the
                // row it is on — a position in a gap belongs to the row after.
                Some(item) if y < item.bottom() => {
                    child = row;
                    break;
                }
                Some(_) => {}
                // Rows the frame has not painted yet: the pointer is past
                // everything drawn, which is the newest Block.
                None => break,
            }
        }
        Some(tail + child)
    }

    /// The selected Blocks' text, one Block per line, to the clipboard. With
    /// nothing selected — or a selection eviction has entirely swept away —
    /// the clipboard is left alone.
    fn copy_selection(&mut self, _: &CopySelection, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(selection) = &self.selection else {
            return;
        };
        let Some(transcript) = self.cockpit.transcript(selection.thread) else {
            return;
        };
        let blocks = transcript.blocks();
        let Some(range) = selection.resolve(blocks) else {
            return;
        };
        let text: Vec<String> = blocks[range].iter().map(pane::block_text).collect();
        cx.write_to_clipboard(ClipboardItem::new_string(text.join("\n")));
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
        let level = self.level_now(window);

        // Focus follows the operator, but only onto something this level
        // actually renders: focusing a Composer a wall cell never drew leaves
        // the keyboard pointing at nothing, and every global key stops working.
        // An empty cockpit still needs the keyboard: with nothing focused,
        // dispatch starts above these handlers and cmd-n could never make the
        // first Thread.
        let wanted = self
            .panes
            .get(self.focused)
            .and_then(|pane| match level {
                _ if self.cockpit.pending(pane.thread).is_some() && level != Level::Wall => {
                    Some(pane.decision_focus.clone())
                }
                Level::Transcript => Some(pane.composer.focus_handle(cx)),
                _ => None,
            })
            .unwrap_or_else(|| self.focus.clone());
        if !wanted.is_focused(window) {
            window.focus(&wanted);
        }

        let mut grid = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(6.))
            .p(px(6.));
        for (row_at, row) in self.panes.chunks(columns).enumerate() {
            let mut line = div().flex().flex_row().flex_1().min_h_0().gap(px(6.));
            for (column, pane) in row.iter().enumerate() {
                let index = row_at * columns + column;
                let focused = self
                    .focused_thread()
                    .is_some_and(|thread| thread == pane.thread);
                let selected = self
                    .selection
                    .as_ref()
                    .filter(|selection| selection.thread == pane.thread)
                    .and_then(|selection| {
                        selection.resolve(self.cockpit.transcript(pane.thread)?.blocks())
                    });
                line = line.child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                                view.pointer_down(index, event.position, window, cx)
                            }),
                        )
                        .on_mouse_move(cx.listener(
                            move |view, event: &MouseMoveEvent, window, cx| {
                                view.pointer_drag(index, event, window, cx)
                            },
                        ))
                        .child(pane::render_pane(
                            pane,
                            pane::PaneState {
                                transcript: self.cockpit.transcript(pane.thread),
                                decision: self.cockpit.pending(pane.thread),
                                queued: self.cockpit.queued(pane.thread),
                                workspace: self.cockpit.workspace(pane.thread),
                                focused,
                                blocked: blocked.contains(&pane.thread),
                                selected,
                            },
                            level,
                        )),
                );
            }
            grid = grid.child(line);
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(pane::BG_WINDOW))
            .font_family(crate::MONO_FONT)
            .track_focus(&self.focus)
            // At wall range no Pane holds a Composer, so the answer keys are
            // not competing with typing: they answer whichever Thread is
            // flagged, without the operator focusing it first.
            .when(level == Level::Wall, |wall| wall.key_context("Wall"))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::interrupt))
            .on_action(cx.listener(Self::allow))
            .on_action(cx.listener(Self::deny))
            .on_action(cx.listener(Self::always))
            .on_action(cx.listener(Self::next_pane))
            .on_action(cx.listener(Self::previous_pane))
            .on_action(cx.listener(Self::next_decision))
            .on_action(cx.listener(Self::new_thread))
            .on_action(cx.listener(Self::new_worktree_thread))
            .on_action(cx.listener(Self::close_thread))
            .on_action(cx.listener(Self::reopen_thread))
            .on_action(cx.listener(Self::copy_selection))
            // The root covers the window, so a release anywhere ends the
            // drag; the selection it made stays until the next press.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _: &MouseUpEvent, _, _| view.grip = None),
            )
            // And EVERY press kills any grip left over from a drag whose
            // release the window never saw — capture phase, so it runs
            // before a Pane's own press sets a fresh one. Without this, a
            // press outside every transcript would leave a dead anchor a
            // later drag could resume.
            .capture_any_mouse_down(cx.listener(|view, _: &MouseDownEvent, _, _| {
                view.grip = None;
            }))
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

/// Where Ferrite was started: the repo a new Thread binds to, either as the
/// main checkout or as the parent of its own worktree.
fn here() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| ".".into())
}

/// This process's resident memory, for the perf print.
fn rss_mb() -> f64 {
    crate::session::rss_bytes(std::process::id())
        .map(|bytes| bytes as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0)
}

/// Adopt CLI sessions started outside Ferrite, before the Cockpit takes the
/// store. Each Thread is durable the moment import returns, so it opens like
/// any parked one. A refusal is the operator's to read: the file is named and
/// the provider's own words are shown, and the run carries on without it.
pub fn adopt(store: &ferrite_core::store::Store, paths: &[String]) -> (Vec<ThreadId>, Vec<String>) {
    let mut adopted = Vec::new();
    let mut refused = Vec::new();
    for path in paths {
        match ferrite_core::import::import(store, std::path::Path::new(path)) {
            Ok(thread) => adopted.push(thread),
            // Reported, not printed: the caller decides where an operator
            // reads it, and a test can read it too.
            Err(e) => refused.push(format!("cannot import {path}: {e}")),
        }
    }
    (adopted, refused)
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
        match cockpit.open(provider, WorkspaceChoice::Main { checkout: here() }) {
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
    use ferrite_core::workspace::WorkspaceBinding;
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
            _cwd: Option<&std::path::Path>,
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
            cockpit
                .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
                .unwrap();
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

    /// The two Threads of a cockpit, in creation order. Pane order follows a
    /// HashMap, so tests about park order must not read it off the grid.
    fn created(
        view: &gpui::Entity<CockpitView>,
        cx: &mut gpui::VisualTestContext,
    ) -> (ThreadId, ThreadId) {
        view.read_with(cx, |view, _| {
            let mut ids: Vec<ThreadId> = view.panes.iter().map(|pane| pane.thread).collect();
            ids.sort();
            (ids[0], ids[1])
        })
    }

    /// #17: cmd-o follows park order, not creation order. Create A then B,
    /// park B then A — the Thread that comes back is A, the one the operator
    /// just closed.
    #[gpui::test]
    fn reopening_revives_the_thread_parked_most_recently(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("park-order", 2);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-w", CloseThread, None),
                KeyBinding::new("cmd-o", ReopenThread, None),
            ]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        let (a, b) = created(&view, cx);

        view.update(cx, |view, _| view.focused = view.pane_for(b).unwrap());
        cx.simulate_keystrokes("cmd-w"); // park B
        cx.simulate_keystrokes("cmd-w"); // then A — the most recent park
        cx.simulate_keystrokes("cmd-o");

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1);
            assert_eq!(
                view.panes[0].thread, a,
                "cmd-o must revive the just-parked {a}, not the newest-created {b}"
            );
        });
    }

    /// Reopening again keeps walking the park order backwards: park A then B,
    /// and two cmd-o bring back B first, then A.
    #[gpui::test]
    fn reopening_again_walks_the_park_order_backwards(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("park-order-again", 2);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-w", CloseThread, None),
                KeyBinding::new("cmd-o", ReopenThread, None),
            ]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        let (a, b) = created(&view, cx);

        view.update(cx, |view, _| view.focused = view.pane_for(a).unwrap());
        cx.simulate_keystrokes("cmd-w"); // park A
        cx.simulate_keystrokes("cmd-w"); // then B

        cx.simulate_keystrokes("cmd-o");
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1);
            assert_eq!(view.panes[0].thread, b, "the last park comes back first");
        });

        cx.simulate_keystrokes("cmd-o");
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 2);
            assert!(
                view.panes.iter().any(|pane| pane.thread == a),
                "and the one before it comes back next"
            );
        });
    }

    /// The park order is memory, not store: a Thread parked before this
    /// launch is not in it. This launch's parks come back first, and only
    /// then does cmd-o fall back to the newest-created parked Thread.
    #[gpui::test]
    fn reopening_falls_back_to_creation_order_for_threads_parked_before_launch(
        cx: &mut TestAppContext,
    ) {
        let fake = Fake::default();
        let store = Store::open(scratch("park-order-fallback")).unwrap();
        let mut core = Cockpit::new(store, Box::new(fake.clone()));
        let a = core
            .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
            .unwrap();
        let b = core
            .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
            .unwrap();
        // Parked before the view exists — a previous launch, as far as the
        // view can know.
        core.park(b).unwrap();
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-w", CloseThread, None),
                KeyBinding::new("cmd-o", ReopenThread, None),
            ]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        view.read_with(cx, |view, _| assert_eq!(view.panes.len(), 1));

        cx.simulate_keystrokes("cmd-w"); // park A — this launch's only park
        cx.simulate_keystrokes("cmd-o");
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1);
            assert_eq!(
                view.panes[0].thread, a,
                "the just-parked {a} outranks the newer-created {b}"
            );
        });

        cx.simulate_keystrokes("cmd-o"); // the order is drained: creation order
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 2);
            assert!(
                view.panes.iter().any(|pane| pane.thread == b),
                "the pre-launch park still comes back, by creation order"
            );
        });
    }

    /// AC4: the wall flags a Thread, and one key answers it from across the
    /// room — the operator never focuses the Pane it belongs to.
    #[gpui::test]
    fn a_wall_flagged_decision_is_answered_without_focusing_its_pane(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("wall-answer", 24);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("y", Allow, Some("Wall"))]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        // 24 Panes in this window is wall range.
        view.update(cx, |view, _| {
            assert_eq!(view.panes.len(), 24);
        });
        fake.streams.borrow()[7].send(decision("perm_08")).unwrap();
        tick(cx);
        let flagged = view.read_with(cx, |view, _| view.panes[7].thread);
        view.read_with(cx, |view, _| {
            assert_eq!(view.focused, 0, "focus stays where the operator left it");
            assert!(view.cockpit.pending(flagged).is_some());
        });

        cx.simulate_keystrokes("y");

        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit.pending(flagged).is_none(),
                "the flagged Thread is the one that got answered"
            );
            assert_eq!(view.focused, 0, "and answering did not move the operator");
        });
    }

    /// AC1: no mode switch — the same cockpit renders at a different altitude
    /// when the window changes size.
    #[gpui::test]
    fn resizing_the_window_changes_every_panes_level(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("resize", 4);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.run_until_parked();

        let wide = cx.update(|window, cx| view.read(cx).level_now(window));
        cx.simulate_resize(gpui::size(gpui::px(360.), gpui::px(280.)));
        let narrow = cx.update(|window, cx| view.read(cx).level_now(window));

        assert!(
            narrow < wide,
            "a smaller window must fall to a lower level: {narrow:?} vs {wide:?}"
        );
    }

    /// A repo with one commit — `git worktree add` needs a commit to branch
    /// from, so a bare init is not enough.
    /// A git repo with one root commit, inside an already-scratched base.
    fn repo_in(base: &std::path::Path) -> std::path::PathBuf {
        let dir = base.join("repo");
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "operator@example.invalid"],
            vec!["config", "user.name", "operator"],
            vec!["commit", "-q", "--allow-empty", "-m", "root"],
        ] {
            let out = std::process::Command::new("git")
                .args(&args)
                .current_dir(&dir)
                .output()
                .expect("git");
            assert!(out.status.success(), "git {args:?}: {out:?}");
        }
        dir
    }

    /// Leg 1: the New-Thread flow offers the worktree, and the Thread really
    /// lands in one — isolation is the whole point of the binding.
    #[gpui::test]
    fn a_thread_can_be_opened_in_its_own_worktree(cx: &mut TestAppContext) {
        // One scratch for both halves: `scratch` wipes its directory, so a
        // second call for the store would delete the repo just made.
        let base = scratch("worktree-key");
        let repo = repo_in(&base);
        let fake = Fake::default();
        let store = Store::open(base.join("threads")).unwrap();
        let core = Cockpit::new(store, Box::new(fake.clone()));
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-shift-n", NewWorktreeThread, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        // The action binds to the repo Ferrite was started in.
        view.update(cx, |view, _| view.repo = repo.clone());
        tick(cx);

        cx.simulate_keystrokes("cmd-shift-n");

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1, "the worktree Thread got a Pane");
            let thread = view.panes[0].thread;
            let binding = view.cockpit.workspace(thread).expect("a binding");
            assert!(
                matches!(binding, WorkspaceBinding::Worktree { .. }),
                "expected a worktree, got {binding:?}"
            );
            // And it is somewhere of its own, not the operator's checkout.
            assert_ne!(binding.cwd(), repo);
        });
    }

    /// Leg 3: a file that is not a session file is refused in the operator's
    /// words, and the cockpit carries on without it.
    #[test]
    fn an_unimportable_file_is_refused_and_adopted_by_nobody() {
        let dir = scratch("import-refusal");
        std::fs::create_dir_all(&dir).unwrap();
        let bogus = dir.join("not-a-session.jsonl");
        std::fs::write(&bogus, "this is not a session file\n").unwrap();
        let store = Store::open(dir.join("threads")).unwrap();

        let (adopted, refused) = adopt(&store, &[bogus.to_string_lossy().to_string()]);

        assert!(adopted.is_empty());
        // The operator is told what was refused and why, in the provider's
        // own words — not left with a launch that quietly did nothing.
        assert_eq!(refused.len(), 1);
        assert!(
            refused[0].contains("not-a-session.jsonl") && refused[0].contains("not an importable"),
            "unhelpful refusal: {}",
            refused[0]
        );
        // Nothing half-made was left in the store either.
        assert!(store.thread_ids().unwrap().is_empty());
    }

    /// The standing-answer rule holds at wall range too: a request that
    /// offered none is not quietly allowed by the key that means "always".
    #[gpui::test]
    fn always_does_nothing_at_the_wall_when_nothing_was_offered(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("wall-always", 24);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("a", Always, Some("Wall"))]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        // `decision()` offers no standing answer.
        fake.streams.borrow()[3].send(decision("perm_04")).unwrap();
        tick(cx);
        let flagged = view.read_with(cx, |view, _| view.panes[3].thread);

        cx.simulate_keystrokes("a");

        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit.pending(flagged).is_some(),
                "a Decision with nothing to adopt must still be waiting"
            );
        });
    }

    /// cmd-n opens on the checkout the operator is already in — the plain
    /// case, beside cmd-shift-n's worktree.
    #[gpui::test]
    fn a_new_thread_binds_to_the_main_checkout(cx: &mut TestAppContext) {
        let root = scratch("new-main");
        let repo = repo_in(&root);
        let fake = Fake::default();
        let store = Store::open(root.join("threads")).unwrap();
        let core = Cockpit::new(store, Box::new(fake));
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-n", NewThread, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.repo = repo.clone());
        tick(cx);

        cx.simulate_keystrokes("cmd-n");

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1);
            let binding = view
                .cockpit
                .workspace(view.panes[0].thread)
                .expect("a binding");
            assert!(matches!(binding, WorkspaceBinding::Main { .. }));
            assert_eq!(binding.cwd(), repo);
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

    /// #15 AC1: a click lands the operator on a Pane — the focus ring moves,
    /// and the keyboard follows it into that Pane's Composer.
    #[gpui::test]
    fn clicking_a_pane_focuses_it_and_the_keyboard_follows(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("click-focus", 2);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        // Two Panes side by side, each big enough to hold a Composer.
        cx.simulate_resize(gpui::size(px(1600.), px(600.)));
        tick(cx);
        view.read_with(cx, |view, _| assert_eq!(view.focused, 0));

        cx.simulate_click(gpui::point(px(1200.), px(300.)), gpui::Modifiers::none());

        view.read_with(cx, |view, _| {
            assert_eq!(view.focused, 1, "the click moved the focus ring");
        });

        cx.simulate_input("hi");

        let (left, right) = view.update(cx, |view, cx| {
            (
                view.panes[0]
                    .composer
                    .update(cx, |composer, cx| composer.take(cx)),
                view.panes[1]
                    .composer
                    .update(cx, |composer, cx| composer.take(cx)),
            )
        });
        assert_eq!(right, "hi", "typing lands in the clicked Pane");
        assert_eq!(left, "", "and nowhere else");
    }

    /// #15 AC4: wheel-scrolling into history detaches from the tail — new
    /// Blocks must not yank the reader down — and scrolling back to the
    /// bottom reattaches tail-follow.
    #[gpui::test]
    fn wheel_scroll_detaches_from_the_tail_and_scrolling_back_reattaches(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("scroll-detach", 1);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        let say = |line: usize| {
            fake.streams.borrow()[0]
                .send(SessionEvent::TextDelta {
                    text: format!("history line {line:03}\n\n"),
                })
                .unwrap();
        };
        for line in 0..80 {
            say(line);
        }
        tick(cx);
        let (offset, max) = view.read_with(cx, |view, _| {
            let scroll = &view.panes[0].scroll;
            (scroll.offset().y, scroll.max_offset().height)
        });
        assert!(max > px(0.), "the transcript must overflow for this test");
        assert!(
            offset + max <= TAIL_SLACK,
            "streaming keeps the tail: {offset:?} against {max:?}"
        );

        // One wheel gesture up: the operator is reading history now.
        let wheel = |cx: &mut gpui::VisualTestContext, dy: f32| {
            cx.simulate_event(gpui::ScrollWheelEvent {
                position: gpui::point(px(500.), px(350.)),
                delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(dy))),
                modifiers: gpui::Modifiers::none(),
                touch_phase: gpui::TouchPhase::default(),
            });
        };
        wheel(cx, 120.);
        let held = view.read_with(cx, |view, _| view.panes[0].scroll.offset().y);
        assert!(held > offset, "wheel up must move the view: {held:?}");

        for line in 80..100 {
            say(line);
        }
        tick(cx);
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.panes[0].scroll.offset().y,
                held,
                "new Blocks must not yank a reader down"
            );
        });

        // Back to the bottom: the tail is theirs again.
        wheel(cx, -100000.);
        for line in 100..110 {
            say(line);
        }
        tick(cx);
        view.read_with(cx, |view, _| {
            let scroll = &view.panes[0].scroll;
            let gap = scroll.max_offset().height + scroll.offset().y;
            assert!(
                gap <= TAIL_SLACK,
                "scrolling to the bottom reattaches the tail: {gap:?}"
            );
        });
    }

    /// #15 AC2, at Block grain: a drag sweeps whole Blocks into a selection
    /// and cmd-c puts their text on the clipboard. A plain click selects
    /// nothing and leaves the clipboard alone.
    #[gpui::test]
    fn a_drag_selects_blocks_and_the_copy_key_takes_their_text(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("select-copy", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: "alpha\n\nbravo\n\ncharlie\n\n".into(),
            })
            .unwrap();
        tick(cx);
        let (first, last) = view.read_with(cx, |view, _| {
            let scroll = &view.panes[0].scroll;
            (
                scroll.bounds_for_item(0).expect("a first row").center(),
                scroll.bounds_for_item(2).expect("a third row").center(),
            )
        });

        cx.simulate_mouse_down(first, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(last, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(last, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");

        let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
        assert_eq!(copied.as_deref(), Some("alpha\nbravo\ncharlie"));

        // A plain click clears the selection; copying then changes nothing.
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("kept".into())));
        cx.simulate_click(first, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");
        let kept = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
        assert_eq!(kept.as_deref(), Some("kept"));
    }

    /// The on-screen center of a rendered transcript row in the first Pane:
    /// row bounds are recorded unscrolled, so the live offset puts them back
    /// on screen.
    fn screen_row(
        view: &gpui::Entity<CockpitView>,
        cx: &mut gpui::VisualTestContext,
        row: usize,
    ) -> gpui::Point<gpui::Pixels> {
        view.read_with(cx, |view, _| {
            let scroll = &view.panes[0].scroll;
            let mut center = scroll
                .bounds_for_item(row)
                .expect("a rendered row")
                .center();
            center.y += scroll.offset().y;
            center
        })
    }

    fn clipboard(cx: &mut gpui::VisualTestContext) -> Option<String> {
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
    }

    /// #15 review: the transcript drops its oldest Blocks past capacity,
    /// shifting every position — a selection stored as positions would
    /// quietly slide onto Blocks the operator never touched. Ids pin it; an
    /// evicted anchor clamps to the window start; a fully evicted selection
    /// dies instead of resurrecting elsewhere.
    #[gpui::test]
    fn a_selection_survives_eviction_instead_of_sliding_onto_later_blocks(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("select-evict", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        let say = |from: usize, to: usize| {
            for line in from..to {
                fake.streams.borrow()[0]
                    .send(SessionEvent::TextDelta {
                        text: format!("filler {line:04}\n\n"),
                    })
                    .unwrap();
            }
        };
        // The counts below straddle the transcript's DEFAULT_CAPACITY of
        // 2000 Blocks (ferrite-core transcript.rs).
        say(0, 60);
        tick(cx);
        // Wheel to the very top, where Blocks 5..=7 are on screen.
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(500.), px(350.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(100000.))),
            modifiers: gpui::Modifiers::none(),
            touch_phase: gpui::TouchPhase::default(),
        });
        let texts = view.read_with(cx, |view, _| {
            let blocks = view
                .cockpit
                .transcript(view.panes[0].thread)
                .expect("a transcript")
                .blocks();
            blocks[5..=7]
                .iter()
                .map(pane::block_text)
                .collect::<Vec<_>>()
        });
        let from = screen_row(&view, cx, 5);
        let to = screen_row(&view, cx, 7);
        cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(clipboard(cx).as_deref(), Some(texts.join("\n").as_str()));

        // 2003 total: three Blocks evicted, every position shifts by three.
        say(60, 2003);
        tick(cx);
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(
            clipboard(cx).as_deref(),
            Some(texts.join("\n").as_str()),
            "eviction shifted positions; the selection must not slide"
        );

        // 2006 total: the anchor (Block 5) is evicted, the head (7) lives —
        // the selection clamps to the window start, which is now Block 6.
        say(2003, 2006);
        tick(cx);
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(
            clipboard(cx).as_deref(),
            Some(texts[1..].join("\n").as_str()),
            "an evicted anchor clamps to the surviving remainder"
        );

        // 2008 total: both endpoints gone — the selection dies, and the
        // clipboard is left alone.
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("kept".into())));
        say(2006, 2008);
        tick(cx);
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(
            clipboard(cx).as_deref(),
            Some("kept"),
            "a fully evicted selection must not resurrect on other Blocks"
        );
    }

    /// #15 review: rows are recorded in unscrolled coordinates, so a drag in
    /// a scrolled-back transcript must map the pointer through the offset —
    /// selecting the Blocks under the pointer, not the Blocks at those
    /// coordinates in the unscrolled layout.
    #[gpui::test]
    fn a_drag_in_a_scrolled_back_transcript_selects_the_rows_under_the_pointer(
        cx: &mut TestAppContext,
    ) {
        let (core, fake) = cockpit("select-scrolled", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        for line in 0..60 {
            fake.streams.borrow()[0]
                .send(SessionEvent::TextDelta {
                    text: format!("history line {line:02}\n\n"),
                })
                .unwrap();
        }
        tick(cx);

        // Wheel well back into history, then drag across on-screen rows.
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(500.), px(350.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(300.))),
            modifiers: gpui::Modifiers::none(),
            touch_phase: gpui::TouchPhase::default(),
        });
        // The first row fully inside the viewport — nonzero, or the wheel
        // did not actually scroll anything back.
        let row = view.read_with(cx, |view, _| {
            let scroll = &view.panes[0].scroll;
            let (bounds, offset) = (scroll.bounds(), scroll.offset().y);
            let mut row = 0;
            loop {
                let item = scroll.bounds_for_item(row).expect("a row in the viewport");
                if item.top() + offset > bounds.top() + px(20.) {
                    return row;
                }
                row += 1;
            }
        });
        assert!(row > 0, "the wheel put earlier rows above the viewport");
        let expected = view.read_with(cx, |view, _| {
            let blocks = view
                .cockpit
                .transcript(view.panes[0].thread)
                .expect("a transcript")
                .blocks();
            // All 60 Blocks render, so row indices are block indices here.
            blocks[row..=row + 2]
                .iter()
                .map(pane::block_text)
                .collect::<Vec<_>>()
                .join("\n")
        });
        let from = screen_row(&view, cx, row);
        let to = screen_row(&view, cx, row + 2);

        cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");

        assert_eq!(clipboard(cx).as_deref(), Some(expected.as_str()));
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
