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
    actions, deferred, div, point, px, relative, rgb, rgba, AnyElement, ClipboardItem, Context,
    Div, FocusHandle, Focusable, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, ScrollHandle, SharedString, Window,
};

use crate::nav;
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
        ToggleFullscreen,
        ToggleNav,
        ToggleRootSelector,
        SelectorNext,
        SelectorPrevious,
        SelectorPick,
        SelectorDismiss,
        MenuNext,
        MenuPrevious,
        MenuPick,
        MenuDismiss,
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
    /// The Thread cmd-f fullscreened, if any: it takes the whole grid area
    /// at Level::Transcript while every other Session keeps streaming (#20).
    /// Deliberate moves re-aim it through `focus_pane` — cmd-w's survivor
    /// fills the screen like the next browser tab. The Thread is named —
    /// not `fullscreen: bool` over `focused` — so one removed by a path
    /// that never called `focus_pane` reads as *gone* and falls back to the
    /// grid (render's self-heal), instead of a bool silently fullscreening
    /// whichever Pane inherited its index, or an empty cockpit rendering
    /// blank.
    fullscreen: Option<ThreadId>,
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
    /// cmd-b (#21): the nav folded to its 40px LED rail. In memory only —
    /// a preference store is not this ticket.
    nav_collapsed: bool,
    /// The open session-project-root selector, or None (#24). At most one
    /// for the whole cockpit, always on the focused Pane; render self-heals
    /// it shut when the operator leaves that Pane or the header that
    /// anchors it stops rendering.
    selector: Option<pane::RootSelector>,
    /// The popover's place in the focus tree while it is open: its menu
    /// keys bind in the RootSelector key context, exactly as a Decision's
    /// card holds y/n.
    selector_focus: FocusHandle,
    /// The nav's parked rows, cached: each one cost a `Store::peek`, so the
    /// cache is rebuilt on park and revive — never per frame.
    parked_rows: Vec<nav::ParkedRow>,
    /// The open Composer menu — `/` commands or `@` files — or None (#23).
    /// At most one for the whole cockpit, always on the focused Pane's
    /// Composer, and derived from that Composer's own text: every edit
    /// re-syncs it, so backspacing past the trigger closes it by itself.
    menu: Option<ComposerMenu>,
    /// Escape (or a press elsewhere) dismissed the menu: stay shut until
    /// the text moves again, or `sync_menu` would reopen it on the very
    /// text the operator dismissed it over.
    menu_muted: bool,
}

/// Which popover the Composer has open, and everything it shows — rebuilt
/// on each edit of the line, never per frame.
struct ComposerMenu {
    thread: ThreadId,
    kind: MenuKind,
    rows: Vec<pane::MenuRow>,
    selected: usize,
}

enum MenuKind {
    /// `/` — the Session's own commands (Claude's initialize `commands[]`,
    /// Codex's skills/list), straight from core. Nothing static.
    Commands,
    /// `@` — files under the Thread's workspace binding. The walk runs once
    /// when the menu opens and is filtered per keystroke; `token_start` is
    /// where the `@` sits, so a pick knows what to splice out.
    Files {
        files: std::rc::Rc<Vec<String>>,
        token_start: usize,
    },
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
/// having exactly that much room, the Dense 8px above and below the rows in
/// `pane::body` (16 together) — while staying under one 12.5px/1.45 text
/// line (~18px), so a deliberate scroll still detaches.
const TAIL_SLACK: Pixels = px(17.);

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

        let mut view = Self {
            cockpit,
            panes: Vec::new(),
            focused: 0,
            fullscreen: None,
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
            nav_collapsed: false,
            selector: None,
            selector_focus: cx.focus_handle(),
            parked_rows: Vec::new(),
            menu: None,
            menu_muted: false,
        };
        for thread in view.cockpit.threads() {
            view.open_pane(thread, cx);
        }
        view.refresh_parked();
        // The first frame's wall cards — every rebuild after rides a change.
        let threads: Vec<ThreadId> = view.panes.iter().map(|pane| pane.thread).collect();
        for thread in threads {
            view.refresh_wall(thread);
        }
        view
    }

    /// The one way a Pane joins the grid: built, and its Composer watched —
    /// every edit of the line re-syncs the open `/`/`@` menu (#23).
    fn open_pane(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let pane = PaneView::new(thread, cx);
        cx.subscribe(&pane.composer, Self::composer_edited).detach();
        self.panes.push(pane);
    }

    /// The focused Composer's line moved: unmute and re-derive the menu.
    /// Menus follow the text — typing `/` or `@` opens, backspacing past
    /// the trigger closes, and a pick's own splice closes through here too.
    fn composer_edited(
        &mut self,
        _: gpui::Entity<crate::composer::Composer>,
        _: &crate::composer::Edited,
        cx: &mut Context<Self>,
    ) {
        self.menu_muted = false;
        self.sync_menu(cx);
        cx.notify();
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
        let mut restarted = Vec::new();
        if self.swept.elapsed() >= SWEEP_INTERVAL {
            self.swept = std::time::Instant::now();
            for restart in self.cockpit.sweep() {
                eprintln!(
                    "ferrite: restarted thread {} after {} bytes resident",
                    restart.thread, restart.rss
                );
                restarted.push(restart.thread);
            }
        }
        // A restart writes a Notice even when no Session streamed this frame —
        // and a failed respawn will never stream again, so this notify is that
        // notice's only ride to the screen.
        if frame.is_empty() && restarted.is_empty() {
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
            // The wall card refolds only when the Thread actually changed —
            // this is the seam that keeps L3 free of per-frame Block walks.
            if !update.dirty.is_empty() || !update.evicted.is_empty() {
                self.refresh_wall(update.thread);
            }
        }
        for thread in restarted {
            self.refresh_wall(thread);
        }
        cx.notify();
    }

    /// Refold one Thread's wall card. Called wherever its transcript can
    /// change — the pump, the operator's own acts — never per frame.
    fn refresh_wall(&mut self, thread: ThreadId) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let card = pane::wall_card(
            self.cockpit.transcript(thread),
            self.cockpit.pending(thread),
        );
        self.panes[index].wall = card;
    }

    /// One cell of the grid, as the window is right now. Size is the only
    /// input semantic zoom takes — there is no mode to switch, and the nav
    /// is simply part of the size: opening it can legitimately drop Panes a
    /// Level (#21).
    fn cell(&self, window: &Window, columns: usize) -> Cell {
        let viewport = window.viewport_size();
        let rows = self.panes.len().div_ceil(columns).max(1);
        // The nav, the strip, the grid's own padding, and the gaps between
        // cells are not the Pane's to render in. (The wall's pinned legend
        // is not subtracted: the Level is decided by width, so the legend
        // can never flip it, and a strip that depends on the Level it is
        // deciding would be circular.)
        let chrome = self.nav_width() + crate::theme::GRID_PAD * 2.0;
        let width = (f32::from(viewport.width) - chrome) / columns as f32 - crate::theme::GRID_GAP;
        let height =
            (f32::from(viewport.height) - crate::theme::STRIP_H - crate::theme::GRID_PAD * 2.0)
                / rows as f32
                - crate::theme::GRID_GAP;
        Cell::new(width.max(0.0), height.max(0.0))
    }

    /// How much of the window the nav holds right now: the 208px column, or
    /// the 40px rail cmd-b folds it to.
    fn nav_width(&self) -> f32 {
        if self.nav_collapsed {
            nav::RAIL_WIDTH
        } else {
            nav::WIDTH
        }
    }

    /// The level this cockpit is rendering at right now — size, with one
    /// exception: fullscreen forces Transcript (#20). A whole-window cell
    /// would pick L1 at any sane size anyway; the force is what keeps
    /// "fullscreen = L1 regardless" true on a tiny window too. Routed here,
    /// not in render, so the pointer math (`block_at`) reads the same level
    /// the frame drew.
    fn level_now(&self, window: &Window) -> Level {
        if self.fullscreen.is_some() {
            return Level::Transcript;
        }
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
        self.refresh_wall(thread);
        cx.notify();
    }

    /// Backspace on an EMPTY Composer line clears the held prompt — the
    /// `⌫ unqueue` the queued row advertises. With text on the line the
    /// Composer consumes the key first and this never runs.
    fn unqueue_from_backspace(
        &mut self,
        _: &crate::composer::Backspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        if !self.panes[self.focused].composer.read(cx).is_empty() {
            return;
        }
        if self.cockpit.unqueue(thread).is_some() {
            cx.notify();
        }
    }

    fn interrupt(&mut self, _: &Interrupt, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(thread) = self.focused_thread() {
            self.cockpit.interrupt(thread);
            self.refresh_wall(thread);
        }
        cx.notify();
    }

    fn allow(&mut self, _: &Allow, window: &mut Window, cx: &mut Context<Self>) {
        self.answer_or_type(Answer::Allow, "y", window, cx);
    }

    fn deny(&mut self, _: &Deny, window: &mut Window, cx: &mut Context<Self>) {
        self.answer_or_type(Answer::Deny, "n", window, cx);
    }

    fn always(&mut self, _: &Always, window: &mut Window, cx: &mut Context<Self>) {
        self.answer_or_type(Answer::Always, "a", window, cx);
    }

    /// The answer keys with the keyboard in the Composer (#23): on an empty
    /// line they are the keycaps' answers; with text on the line they are
    /// letters again — the ⌫-unqueue rule, applied to y/n/a — because an
    /// operator half-way through "not yet…" must be able to finish typing
    /// it. Only at L1, where a Composer is live; the wall and the L2 card
    /// have no line to be typing into.
    fn answer_or_type(
        &mut self,
        answer: Answer,
        letter: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.level_now(window) == Level::Transcript {
            if let Some(pane) = self.panes.get(self.focused) {
                if !pane.composer.read(cx).is_empty() {
                    pane.composer
                        .clone()
                        .update(cx, |composer, cx| composer.insert(letter, cx));
                    return;
                }
            }
        }
        self.answer(answer, cx);
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
        self.refresh_wall(thread);
        cx.notify();
    }

    fn next_pane(&mut self, _: &NextPane, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.panes.is_empty() {
            self.focus_pane((self.focused + 1) % self.panes.len());
            cx.notify();
        }
    }

    fn previous_pane(&mut self, _: &PreviousPane, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.panes.is_empty() {
            self.focus_pane((self.focused + self.panes.len() - 1) % self.panes.len());
            cx.notify();
        }
    }

    /// cmd-f (#20): the focused Pane takes the whole cockpit; cmd-f again
    /// restores the grid. Escape is deliberately not an exit — it stays
    /// Interrupt, and stealing the panic key would make it ambiguous.
    fn toggle_fullscreen(
        &mut self,
        _: &ToggleFullscreen,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.fullscreen = match self.fullscreen {
            Some(_) => None,
            // An empty cockpit has no Pane to fill the screen with.
            None => self.focused_thread(),
        };
        cx.notify();
    }

    /// cmd-b (#21): fold the nav to its 40px LED rail, or open it back to
    /// the 208px column. The width change feeds `cell()`, so Panes may
    /// legitimately change Level — size decides, no special case.
    fn toggle_nav(&mut self, _: &ToggleNav, _window: &mut Window, cx: &mut Context<Self>) {
        self.nav_collapsed = !self.nav_collapsed;
        cx.notify();
    }

    /// cmd-p (#24): the session-project-root selector on the focused Pane.
    fn toggle_root_selector(
        &mut self,
        _: &ToggleRootSelector,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_selector(window, cx);
    }

    /// Open the selector on the focused Pane, or close an open one — the
    /// shared tail of cmd-p and a click on the header chip. Discovery runs
    /// here, once per open, never per frame. A Thread with no binding has
    /// nothing for a root to be inside (the core stores but never prefaces
    /// one), so the selector does not open there; below L1 the header that
    /// anchors the popover is not drawn, so it does not open there either —
    /// focus on an element no frame renders would kill the keyboard.
    fn toggle_selector(&mut self, window: &Window, cx: &mut Context<Self>) {
        if let Some(open) = self.selector.take() {
            // Toggling the Pane it was on closes it; the chip of another
            // Pane (which just took focus) falls through and reopens it
            // there instead of dead-ending on a bare close.
            if Some(open.thread) == self.focused_thread() {
                cx.notify();
                return;
            }
        }
        if self.level_now(window) != Level::Transcript {
            return;
        }
        let Some(thread) = self.focused_thread() else {
            return;
        };
        let Some(binding) = self.cockpit.workspace(thread) else {
            return;
        };
        let checkout = binding.cwd().to_path_buf();
        let mut options = vec![pane::RootOption {
            root: None,
            label: SharedString::from("workspace root"),
        }];
        options.extend(
            ferrite_core::workspace::nested_repositories(&checkout)
                .into_iter()
                .map(|repo| pane::RootOption {
                    label: SharedString::from(pane::root_display(&checkout, &repo)),
                    root: Some(repo),
                }),
        );
        // The arrows start on the Thread's current root — also the ✓ row.
        let current = self.cockpit.session_project_root(thread);
        let active = options
            .iter()
            .position(|option| option.root.as_deref() == current)
            .unwrap_or(0);
        self.selector = Some(pane::RootSelector {
            thread,
            options,
            selected: active,
            active,
        });
        cx.notify();
    }

    fn selector_next(&mut self, _: &SelectorNext, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(selector) = &mut self.selector {
            if selector.selected + 1 < selector.options.len() {
                selector.selected += 1;
                cx.notify();
            }
        }
    }

    fn selector_previous(
        &mut self,
        _: &SelectorPrevious,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(selector) = &mut self.selector {
            if selector.selected > 0 {
                selector.selected -= 1;
                cx.notify();
            }
        }
    }

    fn selector_pick(&mut self, _: &SelectorPick, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = self.selector.as_ref().map(|selector| selector.selected) {
            self.pick_root(at, cx);
        }
    }

    fn selector_dismiss(
        &mut self,
        _: &SelectorDismiss,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selector.take().is_some() {
            cx.notify();
        }
    }

    /// The shared tail of ↵ and a row click: the picked root goes through
    /// the core setter — which ends the Session by design; the next prompt
    /// respawns through `send` wearing the new preface — and the popover
    /// closes. The chip re-reads the core getter next frame, so the chrome
    /// changes with the pick; the store was already durable when the setter
    /// returned.
    fn pick_root(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(selector) = self.selector.take() else {
            return;
        };
        if let Some(option) = selector.options.get(at) {
            if let Err(e) = self
                .cockpit
                .set_session_project_root(selector.thread, option.root.clone())
            {
                // The store refused; the Thread keeps the root it had, and
                // the chip keeps saying so.
                eprintln!(
                    "ferrite: thread {} kept its project root: {e:?}",
                    selector.thread
                );
            }
            self.refresh_wall(selector.thread);
        }
        cx.notify();
    }

    // --------------------------------------------------- Composer menus (#23)

    /// Re-derive the open Composer menu from the focused line's own text.
    /// Nothing else opens or closes a menu: `/` at the start opens commands,
    /// an `@token` under the caret opens files, anything else closes.
    fn sync_menu(&mut self, cx: &mut Context<Self>) {
        self.menu = self.derive_menu(cx);
    }

    fn derive_menu(&mut self, cx: &mut Context<Self>) -> Option<ComposerMenu> {
        // Muted until the text moves again; and never under the root
        // selector, which holds the keyboard while it is up.
        if self.menu_muted || self.selector.is_some() {
            return None;
        }
        let thread = self.focused_thread()?;
        let pane = self.panes.get(self.focused)?;
        let (text, cursor) = {
            let composer = pane.composer.read(cx);
            (composer.text().to_string(), composer.cursor())
        };
        if let Some(filter) = slash_filter(&text) {
            let rows = command_rows(self.cockpit.commands(thread), filter);
            // No wire-backed match, no popover — there is nothing to pick.
            if rows.is_empty() {
                return None;
            }
            return Some(ComposerMenu {
                thread,
                kind: MenuKind::Commands,
                rows,
                selected: 0,
            });
        }
        let (token_start, filter) = mention_token(&text, cursor)?;
        // No binding → nothing to walk → no popover.
        let binding = self.cockpit.workspace(thread)?;
        // The walk runs once per open menu; keystrokes only re-filter it.
        let walked = match &self.menu {
            Some(open) if open.thread == thread => match &open.kind {
                MenuKind::Files { files, .. } => Some(files.clone()),
                MenuKind::Commands => None,
            },
            _ => None,
        };
        let files = walked.unwrap_or_else(|| {
            std::rc::Rc::new(ferrite_core::workspace::mention_files(
                binding.cwd(),
                MENTION_FILE_CAP,
            ))
        });
        let rows = mention_rows(&files, filter);
        if rows.is_empty() {
            return None;
        }
        Some(ComposerMenu {
            thread,
            kind: MenuKind::Files { files, token_start },
            rows,
            selected: 0,
        })
    }

    fn menu_next(&mut self, _: &MenuNext, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.menu {
            if menu.selected + 1 < menu.rows.len() {
                menu.selected += 1;
                cx.notify();
            }
        }
    }

    fn menu_previous(&mut self, _: &MenuPrevious, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.menu {
            if menu.selected > 0 {
                menu.selected -= 1;
                cx.notify();
            }
        }
    }

    fn menu_pick(&mut self, _: &MenuPick, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = self.menu.as_ref().map(|menu| menu.selected) {
            self.pick_menu(at, cx);
        }
    }

    /// Escape while a menu is up closes it and nothing else — the text
    /// stays, and escape's Interrupt meaning waits for the next press.
    fn menu_dismiss(&mut self, _: &MenuDismiss, _window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            self.menu_muted = true;
            cx.notify();
        }
    }

    /// The shared tail of ↵ and a row click: splice the pick into the line.
    /// A command replaces the whole `/filter` with `/name ` — sent later as
    /// plain text on Claude and translated to the typed skill item inside
    /// the Codex Session; a file replaces the `@token` with `@rel/path `
    /// and stages the comp's pill over it, whichever the provider. The
    /// splice's own edit event closes the menu.
    fn pick_menu(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(menu) = self.menu.take() else {
            return;
        };
        let Some(row) = menu.rows.get(at) else {
            return;
        };
        let Some(pane) = self.panes.iter().find(|pane| pane.thread == menu.thread) else {
            return;
        };
        let composer = pane.composer.clone();
        match &menu.kind {
            MenuKind::Commands => {
                let insert = format!("/{} ", row.insert);
                composer.update(cx, |composer, cx| {
                    let whole = 0..composer.text().len();
                    composer.splice(whole, &insert, cx);
                });
            }
            MenuKind::Files { token_start, .. } => {
                let token = format!("@{}", row.insert);
                let start = *token_start;
                composer.update(cx, |composer, cx| {
                    let cursor = composer.cursor();
                    composer.splice(start..cursor, &format!("{token} "), cx);
                    // The pill is the comp's, whoever the provider is: the
                    // wire stays untouched — Claude's CLI reads the `@path`
                    // text itself, Codex's send derives its mention item —
                    // the pick just paints the standing token.
                    composer.stage_mention(SharedString::from(token), cx);
                });
            }
        }
        cx.notify();
    }

    /// The open menu's popover for this Pane, rows wired to their picks —
    /// assembled here so its clicks land beside every other pointer wire
    /// (the root selector's precedent); the Pane hangs it above the line.
    fn composer_menu(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.panes[index].thread;
        let menu = self.menu.as_ref().filter(|menu| menu.thread == thread)?;
        // A press on the popover's own dead space is not a press outside
        // it: swallowed, so the root's dismissal never sees it.
        let mut popover = pane::menu_popover().on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
        );
        for (at, row) in menu.rows.iter().enumerate() {
            popover = popover.child(pane::menu_row(row, at == menu.selected).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.pick_menu(at, cx);
                }),
            ));
        }
        let hints = match menu.kind {
            MenuKind::Commands => "↑↓ select · ↵ run · esc dismiss",
            MenuKind::Files { .. } => "↑↓ select · ↵ insert · esc dismiss",
        };
        popover = popover.child(pane::popover_footer(hints));
        Some(popover.into_any_element())
    }

    /// Rebuild the nav's parked rows. Called on park and revive — never per
    /// frame: each row costs a `Store::peek`, one header line off disk, and
    /// the SharedStrings built here are what every frame after reuses.
    fn refresh_parked(&mut self) {
        let parked = self.cockpit.parked().unwrap_or_default();
        // Stable, append-only order: Threads parked before this launch keep
        // creation order, and this launch's parks append below in park
        // order — a fresh park lands at the bottom of the section instead
        // of re-sorting it.
        let mut ordered: Vec<ThreadId> = parked
            .iter()
            .filter(|thread| !self.park_order.contains(thread))
            .copied()
            .collect();
        ordered.extend(
            self.park_order
                .iter()
                .filter(|thread| parked.contains(thread))
                .copied(),
        );
        self.parked_rows = ordered
            .into_iter()
            .map(|thread| {
                // An unreadable log still gets a row — the Thread exists,
                // and a nav that hides it would hide the problem — it just
                // claims nothing it cannot know.
                let meta = self.cockpit.peek(thread).ok();
                nav::ParkedRow {
                    thread,
                    name: SharedString::from(format!("thread-{thread:02}")),
                    binding: pane::binding_label(
                        meta.as_ref().and_then(|meta| meta.workspace.as_ref()),
                    ),
                    provider: nav::provider_tag(meta.map(|meta| meta.provider)),
                }
            })
            .collect();
    }

    /// The nav's per-frame state, from O(1) reads only — `status()`,
    /// `pending()`, `todos()` — plus small `format!`s, the strip's own
    /// budget. The parked side is the cache; nothing here touches the
    /// store. Render draws exactly this, so tests read it too.
    fn nav_state(&self) -> nav::NavState {
        let running: Vec<nav::RunningRow> = self
            .panes
            .iter()
            .enumerate()
            .map(|(index, pane)| {
                let transcript = self.cockpit.transcript(pane.thread);
                nav::RunningRow {
                    thread: pane.thread,
                    name: SharedString::from(format!("thread-{:02}", pane.thread)),
                    binding: pane::binding_label(self.cockpit.workspace(pane.thread)),
                    provider: nav::provider_tag(self.cockpit.provider(pane.thread)),
                    status: transcript.map(|t| t.status()).unwrap_or_default(),
                    needs_you: self.cockpit.pending(pane.thread).is_some(),
                    todos: transcript.and_then(|t| t.todos()),
                    focused: index == self.focused,
                }
            })
            .collect();
        // The same rollup the strip counts — one function, two surfaces,
        // never a disagreement.
        let waiting = running
            .iter()
            .filter(|row| pane::needs_operator(row.needs_you, Some(row.status)))
            .count();
        nav::NavState {
            running,
            waiting,
            collapsed: self.nav_collapsed,
        }
    }

    /// A running nav row's click: land on that Thread's Pane — through
    /// `focus_pane`, the one door, so a fullscreened cockpit re-aims to the
    /// clicked Thread like every other deliberate move.
    fn focus_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if let Some(index) = self.pane_for(thread) {
            self.focus_pane(index);
            cx.notify();
        }
    }

    /// Revive one parked Thread: a Pane, focus, and the park order and the
    /// nav's cache both forgetting it — cmd-o must not revive it a second
    /// time. The shared tail of cmd-o and a parked nav row's click (#21).
    fn revive_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        match self.cockpit.revive(thread) {
            Ok(()) => {
                self.park_order.retain(|parked| *parked != thread);
                self.open_pane(thread, cx);
                self.focus_pane(self.panes.len() - 1);
                self.refresh_parked();
                self.refresh_wall(thread);
                cx.notify();
            }
            Err(e) => eprintln!("ferrite: thread {thread} could not be reopened: {e:?}"),
        }
    }

    /// The one door to `focused`: every move — keys, clicks, and whatever
    /// #21's nav rows add — lands here, so fullscreen re-aims with focus.
    /// While fullscreen, the Thread the operator lands on is the Thread
    /// that fills the screen (browser-tab muscle memory). Never *enters*
    /// fullscreen, only re-aims it — and with no Thread left to aim at,
    /// falls back to the grid. A writer that bypasses this leaves
    /// fullscreen showing a Thread the operator already left.
    fn focus_pane(&mut self, index: usize) {
        self.focused = index;
        if self.fullscreen.is_some() {
            self.fullscreen = self.focused_thread();
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
                self.open_pane(thread, cx);
                self.focus_pane(self.panes.len() - 1);
                self.refresh_wall(thread);
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
        self.revive_thread(thread, cx);
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
        // The Thread's nav row moves down into the parked section (#21).
        self.refresh_parked();
        // The clamped survivor takes focus — and, while fullscreen, the
        // screen (#20): closing a browser tab shows the next tab, not an
        // overview. Parking the last Thread leaves nothing to aim at, so
        // the setter falls back to the (empty) grid.
        self.focus_pane(self.focused.min(self.panes.len().saturating_sub(1)));
        cx.notify();
    }

    /// Jump to the next Thread waiting on the operator — the whole point of
    /// a wall you cannot read all of at once.
    fn next_decision(&mut self, _: &NextDecision, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(next) = self.cockpit.next_blocked(self.focused_thread()) else {
            return;
        };
        if let Some(pane) = self.pane_for(next) {
            self.focus_pane(pane);
            cx.notify();
        }
    }

    /// A left press lands the operator on this Pane. It only moves focus —
    /// through `focus_pane`, the door #21's nav clicks will share, so a
    /// fullscreen re-aims here too: the per-frame snap in render then
    /// carries focus to whatever the Pane holds (Composer or Decision card)
    /// — fighting the snap would regress the dead-keyboard fixes it exists
    /// for. A press on the transcript also grips a Block, ready to become a
    /// selection if the pointer moves.
    fn pointer_down(
        &mut self,
        index: usize,
        position: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_pane(index);
        // Any press clears the standing selection; only a new drag makes
        // one. The grip was already cleared by the root's capture handler,
        // and the root's bubble handler dismisses any open selector — this
        // Pane included.
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

/// How many rows a Composer menu shows — a dense keyboard menu, not a
/// browser; the fuzzy filter is how the operator reaches the rest.
const MENU_ROWS_MAX: usize = 8;

/// How many files the `@` walk will offer. Bounds one open, not a frame:
/// the walk runs when the menu opens and keystrokes only re-filter it.
const MENTION_FILE_CAP: usize = 2000;

/// The `/` menu's filter: the whole line after a leading `/`, while it is
/// still one token — the first whitespace ends the command and the menu.
fn slash_filter(text: &str) -> Option<&str> {
    let after = text.strip_prefix('/')?;
    (!after.contains(char::is_whitespace)).then_some(after)
}

/// The `@` token the caret sits in: the `@`'s byte offset and the filter
/// typed after it. The `@` must open a token — start of line or after
/// whitespace — so `a@b.example` stays prose, exactly as the wire reads it.
fn mention_token(text: &str, cursor: usize) -> Option<(usize, &str)> {
    let head = text.get(..cursor)?;
    let at = head.rfind('@')?;
    let filter = &head[at + 1..];
    if filter.contains(char::is_whitespace) {
        return None;
    }
    let opens_token = at == 0 || text[..at].ends_with(char::is_whitespace);
    opens_token.then_some((at, filter))
}

/// The `/` menu's rows: the Session's own commands through the fuzzy
/// filter, best first (ties keep the provider's order), capped.
fn command_rows(commands: &[ferrite_core::SessionCommand], filter: &str) -> Vec<pane::MenuRow> {
    let mut scored: Vec<(i64, pane::MenuRow)> = commands
        .iter()
        .filter_map(|command| {
            let (score, matched) = crate::fuzzy::matches(filter, &command.name)?;
            Some((
                score,
                pane::MenuRow {
                    insert: SharedString::from(command.name.clone()),
                    name: SharedString::from(format!("/{}", command.name)),
                    // Shifted past the `/` the row draws in front.
                    matched: matched
                        .into_iter()
                        .map(|range| range.start + 1..range.end + 1)
                        .collect(),
                    detail: SharedString::from(command.description.clone()),
                    prose_detail: true,
                },
            ))
        })
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored
        .into_iter()
        .take(MENU_ROWS_MAX)
        .map(|(_, row)| row)
        .collect()
}

/// The `@` menu's rows: the walked files through the fuzzy filter. The
/// match runs over the whole relative path; the row shows name and
/// directory apart (PromptBox state 03), so highlights are clamped into
/// the name they decorate.
fn mention_rows(files: &[String], filter: &str) -> Vec<pane::MenuRow> {
    let mut scored: Vec<(i64, pane::MenuRow)> = files
        .iter()
        .filter_map(|file| {
            let (score, matched) = crate::fuzzy::matches(filter, file)?;
            let split = file.rfind('/').map(|at| at + 1).unwrap_or(0);
            let matched = matched
                .into_iter()
                .filter_map(|range| {
                    let start = range.start.max(split);
                    (range.end > split).then(|| start - split..range.end - split)
                })
                .collect();
            Some((
                score,
                pane::MenuRow {
                    insert: SharedString::from(file.clone()),
                    name: SharedString::from(file[split..].to_string()),
                    matched,
                    detail: SharedString::from(if split == 0 {
                        String::new()
                    } else {
                        file[..split - 1].to_string()
                    }),
                    prose_detail: false,
                },
            ))
        })
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored
        .into_iter()
        .take(MENU_ROWS_MAX)
        .map(|(_, row)| row)
        .collect()
}

/// Columns for `count` Panes: the boards' own grids are wide, not square —
/// the Cockpit comp lays 6 cells 3×2 and the Wall lays 24 cells 6×4 — so
/// the column count follows a 3:2 grid, never wider than the wall's six.
/// (6×4 is also what makes the wall math work: 24 Panes at the 1440-default
/// window land under the 200px Wall threshold, per sidebar-and-impl §2.)
fn columns(count: usize) -> usize {
    if count <= 1 {
        return 1;
    }
    (count as f64 * 1.5).sqrt().ceil().clamp(1.0, 6.0) as usize
}

impl Render for CockpitView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.measure();
        // A fullscreened Thread whose Pane is gone — removed by a path that
        // bypassed `focus_pane` — falls back to the grid, never a blank
        // cockpit. Render is the one chokepoint every removal passes.
        if self
            .fullscreen
            .is_some_and(|thread| self.pane_for(thread).is_none())
        {
            self.fullscreen = None;
        }
        let fullscreen = self.fullscreen.and_then(|thread| self.pane_for(thread));
        let attention = self.attention();
        let level = self.level_now(window);

        // A selector for a Pane the operator has left — refocused away,
        // parked, or zoomed below the L1 header that anchors it — closes
        // here rather than hanging over the wrong Pane, or holding focus
        // on an element no frame draws (#24).
        if self.selector.as_ref().is_some_and(|selector| {
            self.focused_thread() != Some(selector.thread) || level != Level::Transcript
        }) {
            self.selector = None;
        }
        // And the Composer menu the same way (#23): it belongs to the
        // focused Pane's line at L1, and the root selector outranks it.
        if self.menu.as_ref().is_some_and(|menu| {
            self.focused_thread() != Some(menu.thread)
                || level != Level::Transcript
                || self.selector.is_some()
        }) {
            self.menu = None;
        }
        // The open menu widens its Composer's own key context to
        // ComposerMenu — the focused node, where enter and escape can win
        // their tie against Submit and Interrupt. Render is the one
        // chokepoint every open, pick, dismissal and heal passes.
        let menu_thread = self.menu.as_ref().map(|menu| menu.thread);
        for pane in &self.panes {
            let open = Some(pane.thread) == menu_thread;
            pane.composer
                .update(cx, |composer, cx| composer.set_menu_open(open, cx));
        }

        // Focus follows the operator, but only onto something this level
        // actually renders: focusing a Composer a wall cell never drew leaves
        // the keyboard pointing at nothing, and every global key stops working.
        // An empty cockpit still needs the keyboard: with nothing focused,
        // dispatch starts above these handlers and cmd-n could never make the
        // first Thread. Fullscreen changes none of this: the fullscreened
        // Pane is the focused Pane (fullscreen follows focus), so the snap
        // lands on the one Pane actually on screen.
        let wanted = self
            .panes
            .get(self.focused)
            .and_then(|pane| match level {
                // The open selector holds the keyboard first: the operator
                // opened it, and its keys live in its own context (#24).
                // The heal above already pinned it to this Pane at L1.
                _ if self.selector.is_some() => Some(self.selector_focus.clone()),
                // At L1 the Composer keeps the keyboard even while a
                // Decision pends: the card is part of its stack and the
                // input stays live (PromptBox state 04) — y/n/a answer
                // through the region's own Decision key context (#23).
                Level::Transcript => Some(pane.composer.focus_handle(cx)),
                _ if self.cockpit.pending(pane.thread).is_some() && level != Level::Wall => {
                    Some(pane.decision_focus.clone())
                }
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
            .gap(px(crate::theme::GRID_GAP))
            .p(px(crate::theme::GRID_PAD));
        if let Some(index) = fullscreen {
            // The fullscreened Pane takes the whole content area; the strip
            // above stays as the tether to the rest of the swarm. The other
            // Panes are not laid out at all — hidden siblings would still
            // cost layout — while their Sessions keep streaming through the
            // pump regardless (#20).
            grid = grid.child(self.pane_cell(index, level, cx));
        } else {
            let columns = columns(self.panes.len());
            for (row_at, row) in self.panes.chunks(columns).enumerate() {
                let mut line = div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .gap(px(crate::theme::GRID_GAP));
                for column in 0..row.len() {
                    line = line.child(self.pane_cell(row_at * columns + column, level, cx));
                }
                grid = grid.child(line);
            }
        }

        div()
            .flex()
            .flex_row()
            .size_full()
            .bg(rgb(crate::theme::GROUND))
            .font_family(crate::theme::FONT_MONO)
            .track_focus(&self.focus)
            // At wall range no Pane holds a Composer, so the answer keys are
            // not competing with typing: they answer whichever Thread is
            // flagged, without the operator focusing it first.
            .when(level == Level::Wall, |wall| wall.key_context("Wall"))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::unqueue_from_backspace))
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
            .on_action(cx.listener(Self::toggle_fullscreen))
            .on_action(cx.listener(Self::toggle_nav))
            .on_action(cx.listener(Self::toggle_root_selector))
            .on_action(cx.listener(Self::selector_next))
            .on_action(cx.listener(Self::selector_previous))
            .on_action(cx.listener(Self::selector_pick))
            .on_action(cx.listener(Self::selector_dismiss))
            .on_action(cx.listener(Self::menu_next))
            .on_action(cx.listener(Self::menu_previous))
            .on_action(cx.listener(Self::menu_pick))
            .on_action(cx.listener(Self::menu_dismiss))
            // The root covers the window, so a release anywhere ends the
            // drag; the selection it made stays until the next press.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _: &MouseUpEvent, _, _| view.grip = None),
            )
            // A press anywhere the popovers did not swallow dismisses the
            // open selector and the open Composer menu — Pane bodies, nav
            // rows that move no focus, the strip, all of it. Bubble phase,
            // deliberately: the chip's toggle and the rows' picks stop
            // propagation first, so this can never close what a deeper
            // handler just opened or eat a pick (#24 review). The menu
            // mutes until the text moves, or the very next frame would
            // reopen it over the same trigger.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _: &MouseDownEvent, _, cx| {
                    let mut dismissed = view.selector.take().is_some();
                    if view.menu.take().is_some() {
                        view.menu_muted = true;
                        dismissed = true;
                    }
                    if dismissed {
                        cx.notify();
                    }
                }),
            )
            // And EVERY press kills any grip left over from a drag whose
            // release the window never saw — capture phase, so it runs
            // before a Pane's own press sets a fresh one. Without this, a
            // press outside every transcript would leave a dead anchor a
            // later drag could resume.
            .capture_any_mouse_down(cx.listener(|view, _: &MouseDownEvent, _, _| {
                view.grip = None;
            }))
            // The nav runs the window's full height on the left; the strip
            // and the grid share the rest. Fullscreen keeps it visible — a
            // deliberate override of sidebar-and-impl.md §3 ("the nav hides
            // entirely"): the fullscreened Pane spans the area right of the
            // nav, so the swarm stays one click away (#21).
            .child(self.nav(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(self.strip(attention))
                    .child(grid)
                    // The wall's pinned legend teaches the encoding; the
                    // nearer levels have words and do not need it.
                    .children((level == Level::Wall && fullscreen.is_none()).then(legend)),
            )
    }
}

impl CockpitView {
    /// One Pane's cell — the click-to-focus and drag plumbing around
    /// `render_pane`. The same cell serves a grid slot and the fullscreen
    /// view; only who lays it out differs.
    fn pane_cell(&self, index: usize, level: Level, cx: &mut Context<Self>) -> Div {
        let pane = &self.panes[index];
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
            .on_mouse_move(
                cx.listener(move |view, event: &MouseMoveEvent, window, cx| {
                    view.pointer_drag(index, event, window, cx)
                }),
            )
            .child(pane::render_pane(
                pane,
                pane::PaneState {
                    transcript: self.cockpit.transcript(pane.thread),
                    decision: self.cockpit.pending(pane.thread),
                    queued: self.cockpit.queued(pane.thread),
                    workspace: self.cockpit.workspace(pane.thread),
                    // Only the L1 header draws the chip; the lower levels
                    // must not pay its per-frame strings.
                    root_chip: (level == Level::Transcript)
                        .then(|| self.root_chip(index, cx))
                        .flatten(),
                    // The open `/`/`@` popover — only L1 draws a Composer
                    // to hang it over (#23).
                    menu: (level == Level::Transcript)
                        .then(|| self.composer_menu(index, cx))
                        .flatten(),
                    composer_empty: pane.composer.read(cx).is_empty(),
                    // The meta row's mode chip — only where the meta row
                    // renders.
                    permission_mode: (level == Level::Transcript)
                        .then(|| self.cockpit.permission_mode(pane.thread))
                        .flatten(),
                    focused,
                    running: self.cockpit.busy(pane.thread),
                    selected,
                },
                level,
            ))
    }

    /// The header chip naming this Thread's session project root — and,
    /// while the selector is open on it, the popover hanging under the chip
    /// (#24). Assembled here so its clicks land beside every other pointer
    /// wire; the Pane draws whatever it is handed. A Thread with no binding
    /// gets no chip: a root only means anything inside one.
    fn root_chip(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.panes[index].thread;
        let binding = self.cockpit.workspace(thread)?;
        let root = self.cockpit.session_project_root(thread);
        let chip = pane::root_chip(pane::root_chip_label(binding, root), root.is_some())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                    // The chip is this Pane's: land on it first, then
                    // toggle — and stop the press so the Pane's own handler
                    // cannot immediately close what this just opened.
                    cx.stop_propagation();
                    if let Some(index) = view.pane_for(thread) {
                        view.focus_pane(index);
                    }
                    view.toggle_selector(window, cx);
                }),
            );
        let Some(selector) = self
            .selector
            .as_ref()
            .filter(|selector| selector.thread == thread)
        else {
            return Some(div().relative().child(chip).into_any_element());
        };
        let mut popover = pane::selector_popover()
            .key_context("RootSelector")
            .track_focus(&self.selector_focus)
            // A press on the popover's own dead space (padding, footer) is
            // not a press outside it: swallowed, so the root's dismissal
            // handler never sees it. Rows stop the event themselves first.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            );
        for (at, option) in selector.options.iter().enumerate() {
            popover = popover.child(
                pane::selector_row(option, at == selector.selected, at == selector.active)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.pick_root(at, cx);
                        }),
                    ),
            );
        }
        popover = popover.child(pane::popover_footer("↑↓ move · ↵ pick · esc dismiss"));
        Some(
            div()
                .relative()
                .child(chip)
                // Anchored under the chip, right edges aligned (#24's
                // pinned design). Deferred so it paints over the transcript
                // and escapes the Pane's own clip.
                .child(deferred(
                    div()
                        .absolute()
                        .top(relative(1.))
                        .right_0()
                        .mt(px(6.))
                        .child(popover),
                ))
                .into_any_element(),
        )
    }

    /// How many Threads hold the operator up right now — the strip's amber
    /// count, the nav's `waiting`, and the wall's ring census, all through
    /// `pane::needs_operator` so no two surfaces can disagree.
    fn attention(&self) -> usize {
        self.panes
            .iter()
            .filter(|pane| {
                pane::needs_operator(
                    self.cockpit.pending(pane.thread).is_some(),
                    self.cockpit.transcript(pane.thread).map(|t| t.status()),
                )
            })
            .count()
    }

    /// The whole nav column for this frame, rows wired to their Threads
    /// (#21). It paints inside the cockpit's own render — same entity, same
    /// pump, no second timer — and every number it shows came from
    /// `nav_state`'s O(1) reads or the parked cache.
    fn nav(&self, cx: &mut Context<Self>) -> Div {
        let state = self.nav_state();
        let mut rows = div().flex().flex_col().flex_1().min_h_0();
        for row in &state.running {
            let thread = row.thread;
            let drawn = if state.collapsed {
                nav::running_dot(row)
            } else {
                nav::running_row(row)
            };
            rows = rows.child(drawn.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| view.focus_thread(thread, cx)),
            ));
        }
        if !self.parked_rows.is_empty() {
            rows = rows.child(nav::parked_header(self.parked_rows.len(), state.collapsed));
            for row in &self.parked_rows {
                let thread = row.thread;
                let drawn = if state.collapsed {
                    nav::parked_dot(row)
                } else {
                    nav::parked_row(row)
                };
                rows = rows.child(drawn.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        view.revive_thread(thread, cx)
                    }),
                ));
            }
        }
        nav::shell(state.collapsed)
            .child(nav::header(
                state.running.len(),
                state.waiting,
                state.collapsed,
            ))
            .child(rows)
    }

    /// The wall header strip: the product label left, `N panes · M need
    /// you` right — the amber fragment appears only when someone actually
    /// needs the operator, exactly as the Cockpit and Wall boards draw it.
    fn strip(&self, attention: usize) -> impl IntoElement {
        let panes = self.panes.len();
        let mut strip = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(10.))
            .h(px(crate::theme::STRIP_H))
            .px(px(12.))
            .border_b_1()
            .border_color(rgba(crate::theme::HAIRLINE))
            .child(
                div()
                    .font_family(crate::theme::FONT_UI)
                    .text_size(px(crate::theme::TEXT_CODE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(crate::theme::INK_SECONDARY))
                    .child("ferrite"),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(crate::theme::TEXT_ROW))
                    .text_color(rgb(crate::theme::INK_MUTED))
                    .child(SharedString::from(format!("{panes} panes"))),
            );
        if attention > 0 {
            let verb = if attention == 1 { "needs" } else { "need" };
            strip = strip.child(
                div()
                    .text_size(px(crate::theme::TEXT_ROW))
                    .text_color(rgb(crate::theme::WAIT))
                    .child(SharedString::from(format!("· {attention} {verb} you"))),
            );
        }
        strip
    }
}

/// The wall's pinned legend, verbatim from the Wall board: the five state
/// swatches and the ring key.
fn legend() -> Div {
    let item = |swatch: u32, label: &'static str| {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(4.))
            .child(div().text_color(rgb(swatch)).child("●"))
            .child(div().child(label))
    };
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(14.))
        .h(px(crate::theme::LEGEND_H))
        .px(px(12.))
        .border_t_1()
        .border_color(rgba(crate::theme::HAIRLINE))
        .text_size(px(crate::theme::TEXT_CHIP_SM))
        .text_color(rgb(crate::theme::INK_MUTED))
        .child(item(crate::theme::GOOD, "working"))
        .child(item(crate::theme::WAIT, "needs you"))
        .child(item(crate::theme::FAIL, "blocked / failing"))
        .child(item(crate::theme::GOOD, "done (dimmed)").opacity(0.7))
        .child(item(crate::theme::IDLE, "idle"))
        .child(div().flex_1())
        .child(div().child("ring = focused · amber ring = decision · red ring = blocker"))
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

    /// The queued row's `⌫ unqueue` hint is a real key: Backspace on an
    /// empty Composer line clears the held prompt, while with text on the
    /// line it stays an editing key and the queue survives.
    #[gpui::test]
    fn backspace_on_an_empty_line_unqueues_the_held_prompt(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("unqueue-key", 1);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("enter", Submit, None),
                KeyBinding::new("backspace", crate::composer::Backspace, None),
            ]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread);
        // A streaming turn makes the Session busy; the next prompt queues.
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: "working".into(),
            })
            .unwrap();
        tick(cx);
        view.read_with(cx, |view, _| {
            assert!(view.cockpit.busy(thread), "the premise: a turn in flight");
        });
        cx.simulate_input("also this");
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.queued(thread), Some("also this"));
        });

        // With text on the line, Backspace edits; the queue is untouched.
        cx.simulate_input("dr");
        cx.simulate_keystrokes("backspace");
        view.read_with(cx, |view, cx| {
            assert!(
                !view.panes[0].composer.read(cx).is_empty(),
                "backspace with text is still an editing key"
            );
            assert_eq!(view.cockpit.queued(thread), Some("also this"));
        });

        // Emptied, the next Backspace is the advertised ⌫ unqueue.
        cx.simulate_keystrokes("backspace");
        view.read_with(cx, |view, cx| {
            assert!(view.panes[0].composer.read(cx).is_empty());
            assert_eq!(view.cockpit.queued(thread), Some("also this"));
        });
        cx.simulate_keystrokes("backspace");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.queued(thread),
                None,
                "backspace on the empty line unqueues the held prompt"
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
        // 24 Panes at the app's default 1440×900 is wall range: 6 columns
        // of ~197px cells, under the 200px threshold.
        cx.simulate_resize(gpui::size(px(1440.), px(900.)));
        view.update(cx, |view, _| {
            assert_eq!(view.panes.len(), 24);
        });
        cx.update(|window, cx| {
            assert_eq!(view.read(cx).level_now(window), Level::Wall);
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

    /// The grids the boards draw: 6 cells lay 3×2 (Cockpit board) and 24
    /// lay 6×4 (Wall board); one Pane keeps the whole width.
    #[test]
    fn the_grid_follows_the_boards_wide_shape() {
        assert_eq!(columns(1), 1);
        assert_eq!(columns(2), 2);
        assert_eq!(columns(6), 3);
        assert_eq!(columns(24), 6);
        // Never wider than the wall's six, whatever the count.
        assert_eq!(columns(48), 6);
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
        // Wall range, as above: the "Wall" key context only exists there.
        cx.simulate_resize(gpui::size(px(1440.), px(900.)));
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
        // Two Panes side by side, each big enough to hold a Composer even
        // with the 208px nav (#21) taken off the left.
        cx.simulate_resize(gpui::size(px(1800.), px(600.)));
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

    /// #20: cmd-t is the browser-tab spelling of a new Thread, and cmd-n —
    /// the original — still works beside it. Both keys ride the same
    /// cockpit::NewThread; the keymap table carries both rows.
    #[gpui::test]
    fn cmd_t_opens_a_new_thread_and_cmd_n_still_does(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("cmd-t", 1);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-t", NewThread, None),
                KeyBinding::new("cmd-n", NewThread, None),
            ]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);

        cx.simulate_keystrokes("cmd-t");
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 2, "cmd-t opened a Thread");
        });

        cx.simulate_keystrokes("cmd-n");
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 3, "and cmd-n still does");
        });
    }

    /// #20: cmd-f gives the focused Pane the whole cockpit at L1, and cmd-f
    /// again restores the grid. The proof of L1 is the keyboard: only a
    /// Transcript-level Pane renders a Composer, so typing landing there is
    /// the level made observable — and the focus snap holding in fullscreen.
    #[gpui::test]
    fn cmd_f_fullscreens_the_focused_pane_and_toggles_back(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("fullscreen", 4);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-f", ToggleFullscreen, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        // Four Panes in this window sit at Instruments (three ~246px
        // columns beside the nav): no Composer anywhere.
        cx.simulate_resize(gpui::size(px(980.), px(700.)));
        tick(cx);
        cx.simulate_input("lost");
        let typed = view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.take(cx))
        });
        assert_eq!(typed, "", "the premise: no Composer at grid level");

        cx.simulate_keystrokes("cmd-f");

        view.read_with(cx, |view, _| {
            assert_eq!(
                view.fullscreen,
                Some(view.panes[0].thread),
                "cmd-f fullscreens the focused Pane"
            );
        });
        // One Pane rendered, spanning the whole area right of the nav —
        // a 2-column cell would be under 400px here.
        let width = view.read_with(cx, |view, _| view.panes[0].scroll.bounds().size.width);
        assert!(
            width > px(700.),
            "the fullscreened Pane takes the whole cockpit: {width:?}"
        );
        cx.simulate_input("hi");
        let typed = view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.take(cx))
        });
        assert_eq!(typed, "hi", "fullscreen renders at Transcript level");

        cx.simulate_keystrokes("cmd-f");

        view.read_with(cx, |view, _| {
            assert_eq!(view.fullscreen, None, "cmd-f again restores the grid");
        });
        cx.simulate_input("gone");
        let typed = view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.take(cx))
        });
        assert_eq!(typed, "", "back on the grid, back at Instruments");
    }

    /// #20: fullscreen is L1 *regardless* — a window too small for any cell
    /// to earn Transcript still renders the fullscreened Pane at Transcript,
    /// Composer and all. Size stops deciding; the mode does.
    #[gpui::test]
    fn fullscreen_forces_transcript_level_even_in_a_tiny_window(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("fullscreen-tiny", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-f", ToggleFullscreen, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(240.), px(200.)));
        tick(cx);
        let natural = cx.update(|window, cx| view.read(cx).level_now(window));
        assert!(
            natural < Level::Transcript,
            "the premise: this window cannot earn L1 by size ({natural:?})"
        );

        cx.simulate_keystrokes("cmd-f");
        cx.simulate_input("hi");

        let typed = view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.take(cx))
        });
        assert_eq!(typed, "hi", "forced L1: the Composer holds the keyboard");
    }

    /// #20: cmd-] while fullscreen pages the fullscreen to the next Thread —
    /// browser-tab muscle memory — rather than exiting, or going stale on
    /// the Thread the operator just left.
    #[gpui::test]
    fn paging_while_fullscreen_moves_the_fullscreen_to_the_next_thread(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("fullscreen-page", 3);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-f", ToggleFullscreen, None),
                KeyBinding::new("cmd-]", NextPane, None),
            ]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        cx.simulate_keystrokes("cmd-f");
        view.read_with(cx, |view, _| {
            assert_eq!(view.fullscreen, Some(view.panes[0].thread));
        });

        cx.simulate_keystrokes("cmd-]");

        view.read_with(cx, |view, _| {
            assert_eq!(view.focused, 1, "cmd-] still walks the Threads");
            assert_eq!(
                view.fullscreen,
                Some(view.panes[1].thread),
                "and the next Thread is the fullscreened one now"
            );
        });
    }

    /// #20: cmd-w while fullscreen parks the fullscreened Thread and the
    /// survivor fills the screen — closing a browser tab shows the next
    /// tab, not an overview.
    #[gpui::test]
    fn closing_the_fullscreened_thread_fullscreens_the_survivor(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("fullscreen-close", 2);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-f", ToggleFullscreen, None),
                KeyBinding::new("cmd-w", CloseThread, None),
            ]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        cx.simulate_keystrokes("cmd-f");
        let closed = view.read_with(cx, |view, _| {
            assert!(view.fullscreen.is_some(), "the premise: fullscreen is on");
            view.panes[0].thread
        });

        cx.simulate_keystrokes("cmd-w");

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1, "the Pane is gone");
            assert_eq!(
                view.fullscreen,
                Some(view.panes[0].thread),
                "the surviving Thread fills the screen, like the next tab"
            );
            assert!(
                view.cockpit.parked().unwrap().contains(&closed),
                "and cmd-w still parks, exactly as before"
            );
        });
    }

    /// #20: parking the last Thread while fullscreen has nothing left to
    /// fullscreen — the cockpit falls back to the (empty) grid rather than
    /// rendering a blank fullscreen.
    #[gpui::test]
    fn parking_the_last_fullscreened_thread_falls_back_to_the_grid(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("fullscreen-last", 1);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-f", ToggleFullscreen, None),
                KeyBinding::new("cmd-w", CloseThread, None),
            ]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        cx.simulate_keystrokes("cmd-f");
        view.read_with(cx, |view, _| {
            assert!(view.fullscreen.is_some(), "the premise: fullscreen is on");
        });

        cx.simulate_keystrokes("cmd-w");

        view.read_with(cx, |view, _| {
            assert!(view.panes.is_empty(), "the last Pane is gone");
            assert_eq!(view.fullscreen, None, "and so is the fullscreen");
        });
    }

    /// #20 edge: the fullscreened Thread parked by a path that knows nothing
    /// about fullscreen (a future nav click, the watchdog). The next frame
    /// falls back to the grid — never a blank cockpit, never fullscreen on a
    /// Thread the operator did not pick.
    #[gpui::test]
    fn a_fullscreened_thread_parked_externally_falls_back_to_the_grid(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("fullscreen-external", 2);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-f", ToggleFullscreen, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        cx.simulate_keystrokes("cmd-f");
        let gone = view.read_with(cx, |view, _| {
            assert!(view.fullscreen.is_some(), "the premise: fullscreen is on");
            view.panes[0].thread
        });

        // Park it the way code that never heard of fullscreen would.
        view.update(cx, |view, cx| {
            view.cockpit.park(gone).unwrap();
            view.panes.retain(|pane| pane.thread != gone);
            view.focused = 0;
            cx.notify();
        });
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert_eq!(
                view.fullscreen, None,
                "a fullscreened Thread that vanished falls back to the grid"
            );
            assert_eq!(view.panes.len(), 1, "with the surviving Thread on it");
        });
    }

    /// #21 AC1: the nav lists every Thread — running first in grid order,
    /// then parked below — with the binding and provider a glance needs.
    #[gpui::test]
    fn the_nav_lists_running_threads_in_grid_order_and_parked_below(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("nav-order", 3);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-w", CloseThread, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        let (grid_order, parked_thread) = view.read_with(cx, |view, _| {
            (
                view.panes
                    .iter()
                    .map(|pane| pane.thread)
                    .collect::<Vec<_>>(),
                view.panes[1].thread,
            )
        });

        view.update(cx, |view, _| view.focused = 1);
        cx.simulate_keystrokes("cmd-w");

        view.read_with(cx, |view, _| {
            let state = view.nav_state();
            let running: Vec<ThreadId> = state.running.iter().map(|row| row.thread).collect();
            let expected: Vec<ThreadId> = grid_order
                .iter()
                .copied()
                .filter(|thread| *thread != parked_thread)
                .collect();
            assert_eq!(running, expected, "running rows follow the grid order");
            assert_eq!(
                state.running[0].name.as_ref(),
                format!("thread-{:02}", expected[0]),
                "rows say what the Pane header says"
            );
            assert_eq!(state.running[0].binding.as_ref(), "main");
            assert_eq!(state.running[0].provider, "cl");
            let parked: Vec<ThreadId> = view.parked_rows.iter().map(|row| row.thread).collect();
            assert_eq!(parked, vec![parked_thread], "the parked Thread moved below");
            assert_eq!(
                view.parked_rows[0].binding.as_ref(),
                "main",
                "a parked row still names its binding — peeked, not loaded"
            );
            assert_eq!(view.parked_rows[0].provider, "cl");
        });
    }

    /// #21 AC2: clicking a running nav row lands the operator on that Pane —
    /// through `focus_pane`, so a fullscreened cockpit re-aims to the
    /// clicked Thread instead of going stale on the one they left.
    #[gpui::test]
    fn clicking_a_running_nav_row_focuses_its_pane_and_reaims_fullscreen(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("nav-click", 2);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-f", ToggleFullscreen, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        view.read_with(cx, |view, _| assert_eq!(view.focused, 0));

        // The second running row: 34px nav header, 28px rows.
        cx.simulate_click(
            gpui::point(px(104.), px(34. + 28. + 14.)),
            gpui::Modifiers::none(),
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.focused, 1, "the click moved focus to the row's Pane");
        });

        cx.simulate_keystrokes("cmd-f");
        view.read_with(cx, |view, _| {
            assert_eq!(view.fullscreen, Some(view.panes[1].thread));
        });
        cx.simulate_click(
            gpui::point(px(104.), px(34. + 14.)),
            gpui::Modifiers::none(),
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.focused, 0, "the nav still answers while fullscreen");
            assert_eq!(
                view.fullscreen,
                Some(view.panes[0].thread),
                "and the fullscreen re-aims with focus — the one door"
            );
        });
    }

    /// #21 AC2: clicking a parked nav row revives that Thread — a Pane,
    /// focus, and the park order forgetting it so cmd-o cannot revive it a
    /// second time.
    #[gpui::test]
    fn clicking_a_parked_nav_row_revives_that_thread(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("nav-revive", 2);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-w", CloseThread, None),
                KeyBinding::new("cmd-o", ReopenThread, None),
            ]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        let parked = view.read_with(cx, |view, _| view.panes[0].thread);
        cx.simulate_keystrokes("cmd-w");
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1);
            assert_eq!(view.parked_rows.len(), 1, "the parked Thread got a row");
        });

        // The parked row: 34px header + one running row + the 22px PARKED
        // divider, then its own 28px row.
        cx.simulate_click(
            gpui::point(px(104.), px(34. + 28. + 22. + 14.)),
            gpui::Modifiers::none(),
        );

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 2, "the revived Thread got a Pane");
            assert_eq!(view.panes[1].thread, parked, "and it is the same Thread");
            assert_eq!(view.focused, 1, "focus followed the revival");
            assert!(view.parked_rows.is_empty(), "its nav row moved up");
        });

        // cmd-o must not bring back a Thread the nav already revived.
        cx.simulate_keystrokes("cmd-o");
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 2, "nothing was left parked to reopen");
        });
    }

    /// #21 AC3: a pending Decision is visible in the nav — the blocked row
    /// wears the amber, the header counts it, and the collapsed rail keeps
    /// saying so.
    #[gpui::test]
    fn a_pending_decision_lights_the_nav_row_amber(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("nav-amber", 2);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-b", ToggleNav, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        view.read_with(cx, |view, _| {
            let state = view.nav_state();
            assert!(state.running.iter().all(|row| !row.needs_you));
            assert_eq!(state.waiting, 0);
        });

        fake.streams.borrow()[1].send(decision("perm_02")).unwrap();
        tick(cx);

        view.read_with(cx, |view, _| {
            let state = view.nav_state();
            assert!(state.running[1].needs_you, "the blocked row wears amber");
            assert!(!state.running[0].needs_you, "and nobody else does");
            assert_eq!(state.waiting, 1, "the header counts the wait");
        });

        // Collapsed, the same state feeds the rail's halo — and the frame
        // after the toggle actually paints it.
        cx.simulate_keystrokes("cmd-b");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let state = view.nav_state();
            assert!(state.collapsed);
            assert!(state.running[1].needs_you);
        });
    }

    /// #21: the nav's width is part of the zoom input — cmd-b folding it to
    /// the 40px rail hands the cells 168px back, so a Pane that could not
    /// hold a transcript beside the full nav can beside the rail. cmd-b
    /// again takes the width back.
    #[gpui::test]
    fn cmd_b_collapses_the_nav_and_the_cells_grow_a_level(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("nav-toggle", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-b", ToggleNav, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        // Sized so the Transcript threshold sits between the two nav
        // widths: Instruments beside the 208px column (330px cell),
        // Transcript beside the 40px rail (498px cell).
        cx.simulate_resize(gpui::size(px(560.), px(700.)));
        tick(cx);
        let expanded = cx.update(|window, cx| view.read(cx).level_now(window));
        assert_eq!(
            expanded,
            Level::Instruments,
            "the premise: the full nav costs this cell its transcript"
        );

        cx.simulate_keystrokes("cmd-b");
        let collapsed = cx.update(|window, cx| view.read(cx).level_now(window));
        assert_eq!(collapsed, Level::Transcript, "the rail hands width back");

        cx.simulate_keystrokes("cmd-b");
        let reopened = cx.update(|window, cx| view.read(cx).level_now(window));
        assert_eq!(reopened, Level::Instruments, "cmd-b toggles back");
    }

    // ------------------------------------------------- root selector (#24)

    /// A binding checkout holding two nested repositories — `.git`
    /// DIRECTORIES, which is what discovery counts — in a scratched base.
    fn checkout_with_nested(base: &std::path::Path) -> std::path::PathBuf {
        let checkout = base.join("checkout");
        for nested in ["apps/web/.git", "libs/core/.git"] {
            std::fs::create_dir_all(checkout.join(nested)).unwrap();
        }
        // And one linked worktree — a `.git` FILE — in a `.worktrees/`
        // nest: the operator's literal ask (#24).
        let worktree = checkout.join(".worktrees/T3-code");
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(worktree.join(".git"), "gitdir: elsewhere\n").unwrap();
        checkout
    }

    /// The production key table, loaded whole in the mac spelling — so the
    /// popover's same-depth tie-breaks are tested against exactly the order
    /// launch binds.
    fn bind_selector_keys(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let bindings = crate::load_bindings(crate::keymap::Platform::Mac, cx);
            cx.bind_keys(bindings);
        });
    }

    /// One Thread bound to a checkout with two nested repos and a linked
    /// worktree to discover.
    fn selector_cockpit(name: &str) -> (Cockpit, Fake, std::path::PathBuf) {
        let base = scratch(name);
        let checkout = checkout_with_nested(&base);
        let fake = Fake::default();
        let store = Store::open(base.join("threads")).unwrap();
        let mut cockpit = Cockpit::new(store, Box::new(fake.clone()));
        cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::Main {
                    checkout: checkout.clone(),
                },
            )
            .unwrap();
        (cockpit, fake, checkout)
    }

    /// #24: cmd-p opens the selector on the focused Pane — row 0 the
    /// binding itself, then the discovered nested repositories — cmd-p
    /// again closes it, and escape closes it with the keyboard back in the
    /// Composer (the focus-snap invariant).
    #[gpui::test]
    fn cmd_p_opens_the_root_selector_and_escape_returns_the_keyboard(cx: &mut TestAppContext) {
        let (core, _fake, _checkout) = selector_cockpit("selector-open");
        bind_selector_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_keystrokes("cmd-p");
        view.read_with(cx, |view, _| {
            let selector = view.selector.as_ref().expect("cmd-p opens the selector");
            let labels: Vec<&str> = selector
                .options
                .iter()
                .map(|option| option.label.as_ref())
                .collect();
            assert_eq!(
                labels,
                [
                    "workspace root",
                    ".worktrees/T3-code",
                    "apps/web",
                    "libs/core"
                ],
                "the binding first, then worktrees and repos alike"
            );
            assert_eq!(selector.selected, 0, "the arrows start on the current root");
        });

        cx.simulate_keystrokes("cmd-p");
        view.read_with(cx, |view, _| {
            assert!(view.selector.is_none(), "cmd-p again closes it");
        });

        cx.simulate_keystrokes("cmd-p");
        cx.simulate_keystrokes("escape");
        view.read_with(cx, |view, _| {
            assert!(view.selector.is_none(), "escape dismisses the popover");
        });
        cx.simulate_input("hi");
        let typed = view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.take(cx))
        });
        assert_eq!(typed, "hi", "the keyboard is the Composer's again");
    }

    /// #24 review: a press on chrome that moves no focus — the focused
    /// Thread's own nav row here — still dismisses the popover. Dismissal
    /// rides the root's bubble handler, so it runs for every press the
    /// popover's own surface did not swallow.
    #[gpui::test]
    fn a_press_on_nav_chrome_dismisses_the_selector(cx: &mut TestAppContext) {
        let (core, _fake, _checkout) = selector_cockpit("selector-nav-dismiss");
        bind_selector_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        cx.simulate_keystrokes("cmd-p");
        view.read_with(cx, |view, _| {
            assert!(view.selector.is_some(), "the premise: the popover is open");
        });

        // The focused Thread's own nav row: 34px nav header, first 28px row.
        cx.simulate_click(
            gpui::point(px(104.), px(34. + 14.)),
            gpui::Modifiers::none(),
        );

        view.read_with(cx, |view, _| {
            assert_eq!(view.focused, 0, "the row was the focused Thread's own");
            assert!(
                view.selector.is_none(),
                "the press still dismissed the popover"
            );
        });
    }

    /// #24: ↓ then ↵ picks a linked worktree through the core setter — the
    /// operator's literal "worktree selector" — observable through the
    /// getter, in the chrome's own label, and in the store header a
    /// relaunch loads. The pick ends the Session by design: the next prompt
    /// respawns a fresh one.
    #[gpui::test]
    fn arrows_and_enter_pick_a_worktree_and_the_chrome_follows(cx: &mut TestAppContext) {
        let (core, fake, checkout) = selector_cockpit("selector-pick");
        bind_selector_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread);
        assert_eq!(fake.streams.borrow().len(), 1, "the premise: one Session");

        cx.simulate_keystrokes("cmd-p");
        cx.simulate_keystrokes("down");
        view.read_with(cx, |view, _| {
            assert_eq!(view.selector.as_ref().expect("open").selected, 1);
        });
        cx.simulate_keystrokes("enter");

        let expected = checkout.join(".worktrees/T3-code");
        view.read_with(cx, |view, _| {
            assert!(view.selector.is_none(), "picking closes the popover");
            let root = view.cockpit.session_project_root(thread);
            assert_eq!(root, Some(expected.as_path()), "the core setter ran");
            let binding = view.cockpit.workspace(thread).expect("a binding");
            assert_eq!(
                pane::root_chip_label(binding, root).as_ref(),
                "⌵ .worktrees/T3-code",
                "the chrome names the new root at once"
            );
            assert_eq!(
                view.cockpit.peek(thread).unwrap().session_project_root,
                Some(expected.clone()),
                "the store header a relaunch loads already carries it"
            );
        });

        // The pick ended the Session (the designed behavior); the next
        // prompt respawns a fresh one and lands raw in the transcript.
        cx.simulate_input("go");
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            let blocks = view
                .cockpit
                .transcript(thread)
                .expect("a transcript")
                .blocks();
            assert!(
                blocks.iter().any(|block| matches!(
                    &block.body,
                    ferrite_core::transcript::Body::Prompt(line) if line == "go"
                )),
                "the prompt went out after the pick: {blocks:?}"
            );
        });
        assert_eq!(
            fake.streams.borrow().len(),
            2,
            "a fresh Session replaced the one the pick ended"
        );
    }

    /// #24: reopened, the selector stands on the active root — and picking
    /// "workspace root" clears the override back to the binding itself.
    #[gpui::test]
    fn picking_workspace_root_clears_the_override(cx: &mut TestAppContext) {
        let (core, _fake, _checkout) = selector_cockpit("selector-clear");
        bind_selector_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread);
        cx.simulate_keystrokes("cmd-p");
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit.session_project_root(thread).is_some(),
                "the premise: an override is set"
            );
        });

        cx.simulate_keystrokes("cmd-p");
        view.read_with(cx, |view, _| {
            let selector = view.selector.as_ref().expect("open again");
            assert_eq!(selector.active, 1, "the ✓ sits on the active root");
            assert_eq!(selector.selected, 1, "and the arrows start there");
        });
        cx.simulate_keystrokes("up");
        cx.simulate_keystrokes("enter");

        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.session_project_root(thread), None);
            let binding = view.cockpit.workspace(thread).expect("a binding");
            assert_eq!(
                pane::root_chip_label(binding, None).as_ref(),
                "⌵ workspace",
                "the chrome says the override is gone"
            );
            assert_eq!(
                view.cockpit.peek(thread).unwrap().session_project_root,
                None,
                "cleared on disk too"
            );
        });
    }

    // ------------------------------------------------- Composer menus (#23)

    /// The comp's own slash-menu rows (PromptBox state 02), as a Session
    /// would announce them.
    fn menu_commands() -> Vec<ferrite_core::SessionCommand> {
        [
            ("code-review", "review branch vs main"),
            ("commit", "stage + commit this pane's diff"),
            ("compact", "summarize context"),
            ("to-tickets", "plan → GitHub issues"),
        ]
        .into_iter()
        .map(|(name, description)| ferrite_core::SessionCommand {
            name: name.into(),
            description: description.into(),
            path: None,
        })
        .collect()
    }

    /// A binding checkout holding a couple of plain files for the `@` menu.
    fn checkout_with_files(base: &std::path::Path) -> std::path::PathBuf {
        let checkout = base.join("checkout");
        std::fs::create_dir_all(checkout.join("src")).unwrap();
        std::fs::write(checkout.join("README.md"), "r\n").unwrap();
        std::fs::write(checkout.join("src").join("lib.rs"), "l\n").unwrap();
        checkout
    }

    /// One Thread of `provider` bound to a checkout with files to mention.
    fn bound_cockpit(name: &str, provider: Provider) -> (Cockpit, Fake, std::path::PathBuf) {
        let base = scratch(name);
        let checkout = checkout_with_files(&base);
        let fake = Fake::default();
        let store = Store::open(base.join("threads")).unwrap();
        let mut cockpit = Cockpit::new(store, Box::new(fake.clone()));
        cockpit
            .open(
                provider,
                WorkspaceChoice::Main {
                    checkout: checkout.clone(),
                },
            )
            .unwrap();
        (cockpit, fake, checkout)
    }

    fn composer_text(view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> String {
        view.read_with(cx, |view, cx| {
            view.panes[0].composer.read(cx).text().to_string()
        })
    }

    /// The line's triggers, parsed exactly as the wire reads them: `/` only
    /// as a leading single token, `@` only opening a token under the caret.
    #[test]
    fn the_slash_and_mention_triggers_parse_the_line() {
        assert_eq!(slash_filter("/"), Some(""));
        assert_eq!(slash_filter("/co"), Some("co"));
        assert_eq!(
            slash_filter("/compact now"),
            None,
            "a space ends the command"
        );
        assert_eq!(slash_filter("say /compact"), None, "leading token only");

        assert_eq!(mention_token("@", 1), Some((0, "")));
        assert_eq!(mention_token("fix @Xte", 8), Some((4, "Xte")));
        assert_eq!(
            mention_token("fix @Xte now", 12),
            None,
            "the caret left the token"
        );
        assert_eq!(
            mention_token("mail a@b.example", 16),
            None,
            "interior @ is prose"
        );
        assert_eq!(mention_token("no token here", 13), None);
    }

    /// The `/` rows: fuzzy-filtered, best first, highlights shifted past the
    /// drawn `/`, the description riding as prose detail.
    #[test]
    #[allow(clippy::single_range_in_vec_init)] // assertions compare literal ranges
    fn command_rows_filter_and_highlight_by_fuzzy_match() {
        let commands = menu_commands();
        let all = command_rows(&commands, "");
        assert_eq!(all.len(), 4, "an empty filter lists everything");

        let rows = command_rows(&commands, "co");
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_ref()).collect();
        assert_eq!(
            names,
            ["/code-review", "/commit", "/compact"],
            "to-tickets has no `co` subsequence"
        );
        assert_eq!(rows[0].matched, [1..3], "highlights sit past the drawn /");
        assert_eq!(rows[0].insert.as_ref(), "code-review");
        assert!(rows[0].prose_detail);
        assert!(command_rows(&commands, "zzz").is_empty());
    }

    /// The `@` rows: name and directory split apart, the path the pick
    /// inserts kept whole.
    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn mention_rows_split_name_from_directory() {
        let files = vec!["README.md".to_string(), "src/lib.rs".to_string()];
        let rows = mention_rows(&files, "");
        assert_eq!(rows[0].name.as_ref(), "README.md");
        assert_eq!(rows[0].detail.as_ref(), "");
        assert_eq!(rows[1].name.as_ref(), "lib.rs");
        assert_eq!(rows[1].detail.as_ref(), "src");
        assert!(!rows[1].prose_detail);

        let rows = mention_rows(&files, "lib");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].insert.as_ref(), "src/lib.rs");
        assert_eq!(rows[0].matched, [0..3], "highlights land inside the name");
    }

    /// #23: `/` at the line's start opens the Session's own menu, typing
    /// filters it, ↓/↵ pick — and the pick lands as `/name ` ready for args.
    #[gpui::test]
    fn typing_slash_opens_the_command_menu_and_enter_inserts_the_pick(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("slash-menu", 1);
        bind_selector_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        // The Session announces its menu — the popover's only source.
        fake.streams.borrow()[0]
            .send(SessionEvent::Commands {
                commands: menu_commands(),
            })
            .unwrap();
        tick(cx);
        view.read_with(cx, |view, _| {
            assert!(
                view.menu.is_none(),
                "nothing opens until the operator types"
            );
        });

        cx.simulate_input("/");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.menu.as_ref().expect("/ opens the menu");
            assert_eq!(menu.rows.len(), 4, "everything the Session listed");
            assert_eq!(menu.selected, 0);
        });

        cx.simulate_input("co");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.menu.as_ref().expect("still open while filtering");
            let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(names, ["/code-review", "/commit", "/compact"]);
        });

        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.menu.as_ref().expect("open").selected, 1);
        });

        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(composer_text(&view, cx), "/commit ");
        view.read_with(cx, |view, _| {
            assert!(view.menu.is_none(), "the pick closed the menu");
        });
    }

    /// Escape closes the menu and only the menu: the text stays, escape's
    /// Interrupt meaning waits for the next press, and more typing reopens.
    #[gpui::test]
    fn escape_dismisses_the_menu_keeps_the_text_and_typing_reopens(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("slash-escape", 1);
        bind_selector_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        fake.streams.borrow()[0]
            .send(SessionEvent::Commands {
                commands: menu_commands(),
            })
            .unwrap();
        tick(cx);

        cx.simulate_input("/c");
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.menu.is_some()));

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.menu.is_none(), "escape dismissed the popover");
        });
        assert_eq!(composer_text(&view, cx), "/c", "and kept the text");

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                view.menu.is_none(),
                "a second escape is Interrupt, not a reopen"
            );
        });

        cx.simulate_input("o");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.menu.is_some(), "typing again reopens the menu");
        });
    }

    /// #23: `@` opens the file menu over the Thread's workspace binding;
    /// the pick lands as `@relative/path ` in the line.
    #[gpui::test]
    fn typing_at_completes_files_from_the_workspace_binding(cx: &mut TestAppContext) {
        let (core, _fake, _checkout) = bound_cockpit("mention-menu", Provider::Claude);
        bind_selector_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("read ");
        cx.simulate_input("@");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.menu.as_ref().expect("@ opens the file menu");
            let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(names, ["README.md", "lib.rs"], "the walk, in order");
        });

        cx.simulate_input("li");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.menu.as_ref().expect("open");
            assert_eq!(menu.rows.len(), 1, "the fuzzy filter narrowed it");
            assert_eq!(menu.rows[0].insert.as_ref(), "src/lib.rs");
        });

        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(composer_text(&view, cx), "read @src/lib.rs ");
        view.read_with(cx, |view, cx| {
            assert!(view.menu.is_none());
            // The pill is provider-agnostic: a Claude pick paints it too —
            // the wire stays plain `@path` text the CLI itself reads.
            assert_eq!(
                view.panes[0].composer.read(cx).mentions(),
                [SharedString::from("@src/lib.rs")],
                "the picked token is staged as the comp's pill"
            );
        });
    }

    /// A Thread with no binding has nothing to walk: `@` opens nothing and
    /// typing carries on.
    #[gpui::test]
    fn a_thread_without_a_binding_opens_no_file_menu(cx: &mut TestAppContext) {
        let dir = scratch("mention-unbound");
        let thread_dir = dir.join("9");
        std::fs::create_dir_all(&thread_dir).unwrap();
        std::fs::write(
            thread_dir.join("log.jsonl"),
            concat!(
                r#"{"schema":2,"provider":"claude"}"#,
                "\n",
                r#"{"type":"prompt","text":"hello"}"#,
                "\n",
            ),
        )
        .unwrap();
        let fake = Fake::default();
        let store = Store::open(&dir).unwrap();
        let mut core = Cockpit::new(store, Box::new(fake));
        core.revive(ThreadId::new(9)).unwrap();
        bind_selector_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("@");
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(view.menu.is_none(), "no binding, no popover");
        });
        assert_eq!(composer_text(&view, cx), "@", "typing was not eaten");
    }

    /// #24's dismissal law holds for the menus: a press the popover did not
    /// swallow closes it, and it stays shut until the text moves.
    #[gpui::test]
    fn a_press_on_the_transcript_dismisses_the_open_menu(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("menu-press-dismiss", 1);
        bind_selector_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        fake.streams.borrow()[0]
            .send(SessionEvent::Commands {
                commands: menu_commands(),
            })
            .unwrap();
        tick(cx);
        cx.simulate_input("/");
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.menu.is_some()));

        // The middle of the Pane's transcript — nowhere near the popover.
        cx.simulate_mouse_down(
            gpui::point(px(600.), px(200.)),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(view.menu.is_none(), "the press dismissed the popover");
        });
        assert_eq!(composer_text(&view, cx), "/", "the text survived the press");
    }

    /// #23: while a Decision pends at L1 the keyboard stays in the Composer
    /// — the input is live (typing queues, since the turn is running) and
    /// the empty line makes y the keycap's answer.
    #[gpui::test]
    fn a_pending_decision_keeps_the_composer_live_and_an_empty_line_answers(
        cx: &mut TestAppContext,
    ) {
        let (core, fake) = cockpit("decision-live", 1);
        bind_selector_keys(cx);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread);
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolStarted {
                id: "toolu_1".into(),
                name: "Write".into(),
                input: serde_json::json!({ "file_path": "ferrite-perm.txt" }),
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(decision("perm_live"))
            .unwrap();
        tick(cx);
        view.read_with(cx, |view, _| {
            assert!(view.cockpit.pending(thread).is_some(), "the card is up");
            assert!(view.cockpit.busy(thread), "the turn is running");
        });

        // The input is still live: typing lands, enter queues behind the
        // turn. (The first key of an empty line is where y/n/a mean their
        // keycaps, so the sentence starts past them.)
        cx.simulate_input("fix the tests too");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.queued(thread), Some("fix the tests too"));
            assert!(
                view.cockpit.pending(thread).is_some(),
                "typing answered nothing"
            );
        });

        // Emptied, y is the keycap's answer.
        cx.simulate_keystrokes("y");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit.pending(thread).is_none(),
                "y on the empty line answered the Decision"
            );
        });
    }

    /// The other half of the y/n/a rule: with text on the line they are
    /// letters — an operator half-way through a word keeps typing it.
    #[gpui::test]
    fn the_answer_keys_stay_letters_while_the_line_holds_text(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("decision-letters", 1);
        bind_selector_keys(cx);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread);
        fake.streams.borrow()[0]
            .send(decision("perm_type"))
            .unwrap();
        tick(cx);

        cx.simulate_input("wait");
        cx.simulate_keystrokes("y");
        cx.run_until_parked();

        assert_eq!(
            composer_text(&view, cx),
            "waity",
            "y typed instead of answering"
        );
        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit.pending(thread).is_some(),
                "the Decision is still waiting"
            );
        });
    }

    /// #23: the Session's announced permission mode becomes the meta row's
    /// chip state — display-only, absent until announced, and rendered
    /// through the same frame the assertions ride.
    #[gpui::test]
    fn the_announced_permission_mode_reaches_the_meta_row_state(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("mode-chip", 1);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread);
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.permission_mode(thread),
                None,
                "no chip is invented before the Session speaks"
            );
        });

        fake.streams.borrow()[0]
            .send(SessionEvent::PermissionMode {
                mode: "acceptEdits".into(),
            })
            .unwrap();
        tick(cx);

        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.permission_mode(thread), Some("acceptEdits"));
        });
    }

    /// #23: on a Codex Thread a picked file also stages the @-pill — the
    /// send will carry the typed mention item, and the input paints the
    /// token as the comp draws it.
    #[gpui::test]
    fn picking_a_mention_on_a_codex_thread_stages_the_pill(cx: &mut TestAppContext) {
        let (core, _fake, _checkout) = bound_cockpit("mention-codex", Provider::Codex);
        bind_selector_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("@li");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        assert_eq!(composer_text(&view, cx), "@src/lib.rs ");
        view.read_with(cx, |view, cx| {
            assert_eq!(
                view.panes[0].composer.read(cx).mentions(),
                [SharedString::from("@src/lib.rs")],
                "the pill token is staged for the paint"
            );
        });
    }

    /// #24: a Thread from before bindings were recorded has nothing for a
    /// root to be inside — cmd-p opens nothing, and the keyboard stays with
    /// the Composer.
    #[gpui::test]
    fn a_thread_without_a_binding_ignores_the_selector_key(cx: &mut TestAppContext) {
        let dir = scratch("selector-unbound");
        let thread_dir = dir.join("9");
        std::fs::create_dir_all(&thread_dir).unwrap();
        // A frozen v2-era log: no workspace binding, exactly what old
        // stores still hold (the store's own frozen-contract fixtures).
        std::fs::write(
            thread_dir.join("log.jsonl"),
            concat!(
                r#"{"schema":2,"provider":"claude"}"#,
                "\n",
                r#"{"type":"prompt","text":"hello"}"#,
                "\n",
            ),
        )
        .unwrap();
        let fake = Fake::default();
        let store = Store::open(&dir).unwrap();
        let mut core = Cockpit::new(store, Box::new(fake));
        core.revive(ThreadId::new(9)).unwrap();
        bind_selector_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit.workspace(view.panes[0].thread).is_none(),
                "the premise: no binding"
            );
        });

        cx.simulate_keystrokes("cmd-p");

        view.read_with(cx, |view, _| {
            assert!(view.selector.is_none(), "no binding, no selector");
        });
        cx.simulate_input("still typing");
        let typed = view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.take(cx))
        });
        assert_eq!(
            typed, "still typing",
            "the keyboard never left the Composer"
        );
    }
}
