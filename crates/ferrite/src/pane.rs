//! One Pane: the visible cell for one Thread. Header, transcript, Composer,
//! and the three semantic-zoom renderings. Rendering only — everything it
//! shows is folded in core, and every key it answers to belongs to the
//! cockpit above it.
//!
//! L1 is the Soft prototype's Pane, drawn top to bottom: a 32px head, a
//! 24px tasks strip, the transcript body, the Decision card, the 24px
//! changed-files strip and the 58px Composer — all on `--pane`, inside an
//! always-in-layout 1px border that only changes colour, with the focus
//! ring 2px OUTSIDE it so attention and focus are independent channels.
//! Everything inside a Pane is JetBrains Mono; the shell opts in once.
//! L2 (Instruments) and L3 (Wall) keep the metrics they have — the
//! prototype specifies only L1 — and take the new palette and scale.

use ferrite_core::cockpit::{ProviderChoice, ThreadView, ToolTiming};
use ferrite_core::docview::{is_test_run, passed_count, Instruments, Level, Tests};
use ferrite_core::roster::{DraftId, PaneIdentity};
use ferrite_core::store::Provider;
use ferrite_core::transcript::{
    Block, BlockId, Body, Class, Diff, Span, Status, Style, Todos, Token, ToolBlock, ToolState,
    Transcript,
};
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::workspace::WorkspaceBinding;
use ferrite_core::{Decision, ThreadId};
use gpui::prelude::*;
use gpui::{
    canvas, deferred, div, point, px, relative, rgb, rgba, AnyElement, BoxShadow, Context, Div,
    Entity, FocusHandle, FontFeatures, FontWeight, HighlightStyle, PathBuilder, ScrollHandle,
    SharedString, Stateful, Styled, StyledText,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;
#[cfg(test)]
use std::{cell::RefCell, rc::Rc};

use crate::composer::Composer;
use crate::icons::{self, icon};
use crate::pointer::{Pointer, PointerPressed};
use crate::select::SelectionOverlay;
// Every color and metric here is a Soft token (crate::theme) — no literal
// survives in render code, which is #22's grep-able law.
use crate::theme;
use crate::theme::{
    ATTENTION, ATTENTION_EDGE, ATTENTION_WASH, BLOCKED, BLOCKED_WASH, FOCUS, HOVER, IDLE,
    METER_OFF, PANE, RAISED, RUNNING, RUNNING_WASH, SELECTION, SEP, TEXT, TEXT_2, TEXT_MUTED,
    TEXT_STRONG, TRANSPARENT,
};

/// One Pane's view state: what the window owns per Pane. Everything it
/// shows lives in core; this is the keyboard, the scrollback position, and
/// the wall cell's cached strings.
pub struct PaneView {
    /// Which Pane of the roster this is: a live Thread, or a draft still
    /// choosing its provider and CWD (#29) — no Thread, no Session, nothing
    /// durable. The roster's own identity, so the two can never disagree.
    pub identity: PaneIdentity,
    /// The draft's choices, while it is one.
    draft: Option<DraftBinding>,
    /// The Thread's slug name — `thread-NN` until display names exist
    /// (sidebar-and-impl §4.2 #8) — or the draft's placeholder title.
    /// Built once; the wall must not format names per frame.
    pub name: SharedString,
    pub composer: Entity<Composer>,
    pub scroll: ScrollHandle,
    /// A pending Decision takes the keyboard: y and n are answers, not text.
    pub decision_focus: FocusHandle,
    disclosure: ToolDisclosure,
}

struct ToolDisclosure {
    expanded: HashSet<String>,
    target: Option<String>,
    focus: FocusHandle,
    #[cfg(test)]
    bounds: Rc<RefCell<HashMap<String, gpui::Bounds<gpui::Pixels>>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisclosureState {
    Collapsed,
    Expanded,
}

/// A draft Pane's whole choice, ids only — the band renders from this and
/// the first send resolves it through the registry.
pub struct DraftBinding {
    pub provider: ProviderChoice,
    pub project: ProjectId,
    pub target: DraftTarget,
    /// The band chip tab has parked on; None with the keyboard in the
    /// prompt line — the zero-keystroke default path.
    pub band_focus: Option<BandChip>,
    /// A failed bootstrap's words, shown where the band is. The Pane stays
    /// draft and the prompt stays in the Composer.
    pub error: Option<SharedString>,
}

/// The band's three chips, in tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandChip {
    Provider,
    Project,
    Workspace,
}

impl BandChip {
    /// Where tab goes next: across the band, then back to the prompt.
    pub fn next(current: Option<BandChip>) -> Option<BandChip> {
        match current {
            None => Some(BandChip::Provider),
            Some(BandChip::Provider) => Some(BandChip::Project),
            Some(BandChip::Project) => Some(BandChip::Workspace),
            Some(BandChip::Workspace) => None,
        }
    }
}

/// The workspace chip's choice, scoped to the draft's project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftTarget {
    /// The project checkout itself.
    Main,
    /// A registered worktree of the project, named by its branch.
    Existing { branch: SharedString },
    /// A fresh worktree, created at first send.
    New,
}

impl PaneView {
    pub fn new<T: 'static>(thread: ThreadId, cx: &mut Context<T>) -> Self {
        Self {
            identity: PaneIdentity::Thread(thread),
            draft: None,
            name: SharedString::from(format!("thread-{thread:02}")),
            composer: cx.new(Composer::new),
            scroll: ScrollHandle::new(),
            decision_focus: cx.focus_handle(),
            disclosure: ToolDisclosure {
                expanded: HashSet::new(),
                target: None,
                focus: cx.focus_handle(),
                #[cfg(test)]
                bounds: Rc::new(RefCell::new(HashMap::new())),
            },
        }
    }

    /// A draft Pane (#29): cmd-t's answer — a Composer and the pre-prompt
    /// band, and nothing else until the first send bootstraps a Thread.
    pub fn new_draft<T: 'static>(
        draft: DraftId,
        binding: DraftBinding,
        cx: &mut Context<T>,
    ) -> Self {
        Self {
            identity: PaneIdentity::Draft(draft),
            draft: Some(binding),
            name: SharedString::from("new thread"),
            composer: cx.new(Composer::new),
            scroll: ScrollHandle::new(),
            decision_focus: cx.focus_handle(),
            disclosure: ToolDisclosure {
                expanded: HashSet::new(),
                target: None,
                focus: cx.focus_handle(),
                #[cfg(test)]
                bounds: Rc::new(RefCell::new(HashMap::new())),
            },
        }
    }

    /// The Thread this Pane shows, or None while it is still a draft.
    pub fn thread(&self) -> Option<ThreadId> {
        self.identity.thread()
    }

    pub fn draft(&self) -> Option<&DraftBinding> {
        self.draft.as_ref()
    }

    pub fn draft_mut(&mut self) -> Option<&mut DraftBinding> {
        self.draft.as_mut()
    }

    /// The lock's visible half (#29): the first send made a Thread of this
    /// draft, and the band disappears with the Pane's next frame.
    pub fn adopt_thread(&mut self, thread: ThreadId) {
        self.identity = PaneIdentity::Thread(thread);
        self.draft = None;
        self.name = SharedString::from(format!("thread-{thread:02}"));
    }

    pub(crate) fn toggle_tool(&mut self, call: &str) {
        if !self.disclosure.expanded.remove(call) {
            self.disclosure.expanded.insert(call.to_string());
        }
        self.disclosure.target = Some(call.to_string());
    }

    pub(crate) fn tool_state(&self, call: &str) -> DisclosureState {
        if self.disclosure.expanded.contains(call) {
            DisclosureState::Expanded
        } else {
            DisclosureState::Collapsed
        }
    }

    pub(crate) fn tool_targeted(&self, call: &str) -> bool {
        self.disclosure.target.as_deref() == Some(call)
    }

    pub(crate) fn has_tool_target(&self) -> bool {
        self.disclosure.target.is_some()
    }

    pub(crate) fn targeted_tool(&self) -> Option<&str> {
        self.disclosure.target.as_deref()
    }

    pub(crate) fn tool_focus(&self) -> FocusHandle {
        self.disclosure.focus.clone()
    }

    pub(crate) fn cycle_tools(&mut self, calls: &[String], reverse: bool) -> Option<&str> {
        let next = if calls.is_empty() {
            None
        } else if reverse {
            match self
                .disclosure
                .target
                .as_ref()
                .and_then(|target| calls.iter().position(|call| call == target))
            {
                None => calls.last().cloned(),
                Some(0) => None,
                Some(at) => calls.get(at - 1).cloned(),
            }
        } else {
            match self
                .disclosure
                .target
                .as_ref()
                .and_then(|target| calls.iter().position(|call| call == target))
            {
                None => calls.first().cloned(),
                Some(at) if at + 1 == calls.len() => None,
                Some(at) => calls.get(at + 1).cloned(),
            }
        };
        self.disclosure.target = next;
        self.disclosure.target.as_deref()
    }

    pub(crate) fn prune_tools(&mut self, calls: &HashSet<String>) {
        self.disclosure.expanded.retain(|call| calls.contains(call));
        if self
            .disclosure
            .target
            .as_ref()
            .is_some_and(|call| !calls.contains(call))
        {
            self.disclosure.target = None;
        }
    }

    pub(crate) fn clear_tool_target(&mut self) {
        self.disclosure.target = None;
    }

    #[cfg(test)]
    pub(crate) fn tool_expanded(&self, call: &str) -> bool {
        self.disclosure.expanded.contains(call)
    }

    #[cfg(test)]
    pub(crate) fn tool_bounds(&self, call: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.disclosure.bounds.borrow().get(call).copied()
    }

    #[cfg(test)]
    pub(crate) fn tool_bounds_sink(
        &self,
    ) -> Rc<RefCell<HashMap<String, gpui::Bounds<gpui::Pixels>>>> {
        self.disclosure.bounds.clone()
    }
}

/// Everything one Pane draws, as the cockpit reads it for this frame.
/// What a frame knows about a Pane: the Thread's own facts through one
/// core handle — `None` for a Thread the cockpit could not open — beside
/// the four the window alone can answer.
pub struct PaneFacts<'a> {
    pub thread: Option<ThreadView<'a>>,
    /// The actual git checkout of the Thread's cwd (#29), cached by the
    /// cockpit and refreshed on turn end and the watchdog cadence — the L1
    /// header's binding slot. Display-only, never a control: post-lock the
    /// CWD is immutable, and nothing may look otherwise.
    pub branch: Option<SharedString>,
    /// Whether the Composer line is empty — what decides the idle
    /// placeholder, read where the cockpit has a `cx` to read it with.
    pub composer_empty: bool,
    pub history_available: bool,
    pub focused: bool,
    /// The wall cell's folded reading, cached by the cockpit's facts —
    /// everything the L3 recipe needs that is not an O(1) transcript read.
    /// None for a Thread the facts have not met, which draws as empty.
    pub wall: Option<&'a WallCard>,
    /// This frame's selection seam (#27): every text run the transcript
    /// draws goes through it — registered for hit-testing and copy, and
    /// washed where the selection covers it. The cockpit owns the drag;
    /// the Pane only routes its runs.
    pub selection: SelectionOverlay,
}

/// The click-wired elements only the cockpit can build — gpui listeners
/// are made with its own `Context` — and the Pane only places. Each is
/// `None` (or empty) below the level that draws it.
#[derive(Default)]
pub struct PaneWiring {
    /// The open `/` or `@` popover for this Pane's Composer, rows wired to
    /// their picks in the cockpit and hung above the input line here (#23).
    pub menu: Option<AnyElement>,
    /// The Composer's model picker — logomark, model label, chevron —
    /// supplied for **every** L1 Pane, before and after the first-prompt
    /// lock: the prototype draws it in all four Panes (#25).
    pub model_picker: Option<AnyElement>,
    /// The context ring (#22 C12). None when the provider reported no
    /// window, or below L1. No hover card, no token count, no `ctx`: the
    /// ring is the whole affordance.
    pub usage_ring: Option<AnyElement>,
    /// The pending Decision's keycaps, wired to the exact decide verbs the
    /// keys run (#26) — laid into the L1 card or the L2 body. None while
    /// nothing pends, and at the wall, which draws no keycaps.
    pub decide: Option<AnyElement>,
    /// L1 tool chevrons, already wired to the cockpit's shared toggle door.
    pub tool_controls: HashMap<String, AnyElement>,
    /// The head's title cell, wired: the name with a double-click that
    /// opens the rename editor, or the editor itself while renaming. None
    /// draws the plain name (L2, L3, drafts).
    pub title: Option<AnyElement>,
}

/// The wall's state matrix (glance.md §4), selected from O(1) reads plus the
/// folded tests flag. Pure so the matrix is assertable without a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallState {
    Working,
    /// Working with a red test suite: red text, green dot, no ring.
    Failing,
    /// A Decision waits: amber dot, amber ring, two-line alert, no bar.
    Decision,
    /// The Session closed under the Thread: red dot, red ring, alert.
    Blocked,
    /// Turn complete: dimmed cell, green `✓ done`.
    Done,
    Idle,
    /// No transcript in memory at all — the cockpit could not open it.
    Parked,
}

/// glance.md's matrix, one row per state. `finished` is the honest v1 test
/// for "done": an idle Thread whose last turn recorded a cost. The cost is
/// data only — no surface renders a dollar figure; the done cell reads a
/// plain "turn complete".
pub fn wall_state(
    status: Option<Status>,
    pending: bool,
    tests_failing: bool,
    finished: bool,
) -> WallState {
    let Some(status) = status else {
        return WallState::Parked;
    };
    if pending || status == Status::Blocked {
        return WallState::Decision;
    }
    match status {
        Status::Closed => WallState::Blocked,
        Status::Streaming if tests_failing => WallState::Failing,
        Status::Streaming => WallState::Working,
        _ if finished => WallState::Done,
        _ => WallState::Idle,
    }
}

/// The wall cell's folded strings — rebuilt only when the Thread changed,
/// never per frame (the L3 budget: 24 cells × 60fps must not walk Blocks or
/// format strings).
#[derive(Default)]
pub struct WallCard {
    /// The latest test run failed (from `Instruments`, the one O(blocks)
    /// read the wall needs).
    pub tests_failing: bool,
    /// The failing line, with the run's own count where it reported one:
    /// `✗ 2 failing`, else `✗ failing`.
    pub failing: SharedString,
    /// The plan meter's ▰▱ glyph run, empty without a plan (or past the
    /// glyph cap — the status line already carries the fraction).
    pub meter: SharedString,
    /// The working status line: `3/4 · ◐ working` or `◐ working`.
    pub working: SharedString,
    /// An alert cell's context: the Decision's subject on line two, or the
    /// close reason promoted to the red first line (`✗ reason`). Empty when
    /// neither applies.
    pub context: SharedString,
}

/// Fold one Thread's wall reading. The activity phrase stays a status word —
/// naming the running tool at L3 would put `Instruments::of` on every wall
/// cell every rebuild during streaming for a 9px line nobody can read at
/// distance (sidebar-and-impl §4.2 #6 keeps names at L2).
pub fn wall_card(transcript: Option<&Transcript>, decision: Option<&Decision>) -> WallCard {
    let Some(transcript) = transcript else {
        return WallCard::default();
    };
    let todos = transcript.todos();
    let meter = todos
        .map(|todos| SharedString::from(meter_run(todos.done, todos.total)))
        .unwrap_or_default();
    let working = match todos {
        Some(todos) => SharedString::from(format!("{}/{} · ◐ working", todos.done, todos.total)),
        None => SharedString::from("◐ working"),
    };
    let context = match decision {
        Some(decision) => decision_subject(decision),
        // A closed Thread's context is the reason it closed — the last
        // Notice the fold pushed — promoted to the alert line (#22 C14).
        None if transcript.status() == Status::Closed => transcript
            .blocks()
            .iter()
            .rev()
            .find_map(|block| match &block.body {
                Body::Notice(reason) => Some(SharedString::from(format!("✗ {reason}"))),
                _ => None,
            })
            .unwrap_or_else(|| SharedString::from("✗ closed")),
        None => SharedString::default(),
    };
    let tests = Instruments::of(transcript).tests;
    let failing = match tests {
        Some(Tests::Failed { count: Some(count) }) => {
            SharedString::from(format!("✗ {count} failing"))
        }
        _ => SharedString::from("✗ failing"),
    };
    WallCard {
        tests_failing: matches!(tests, Some(Tests::Failed { .. })),
        failing,
        meter,
        working,
        context,
    }
}

/// One Pane. A Thread with no open state in core is one the cockpit could
/// not open; it still gets a cell, because a Pane that vanishes hides the
/// problem.
pub fn render_pane(
    view: &PaneView,
    facts: PaneFacts<'_>,
    wiring: PaneWiring,
    level: Level,
) -> impl IntoElement {
    let PaneFacts {
        thread,
        branch,
        composer_empty,
        history_available,
        focused,
        wall,
        selection,
    } = facts;
    let empty = WallCard::default();
    let wall = wall.unwrap_or(&empty);
    let PaneWiring {
        menu,
        model_picker,
        usage_ring,
        mut decide,
        mut tool_controls,
        title,
    } = wiring;
    let transcript = thread.map(|thread| thread.transcript());
    let decision = thread.and_then(|thread| thread.pending());
    let queued = thread.and_then(|thread| thread.queued());
    let workspace = thread.and_then(|thread| thread.workspace());
    let permission_mode = thread.and_then(|thread| thread.permission_mode());
    let timings = thread.map(|thread| thread.tool_timings());
    let status = transcript.map(|t| t.status());
    // `esc interrupt` (§D.7): running **and** focused. The head's dot reads
    // the transcript's own status, not the turn-in-flight flag — a revived
    // Thread mid-turn shows the green dot with `busy` false — so the hint
    // reads the same predicate the dot does, or the Pane looks running and
    // offers no way out. The focus half is here because the prototype's
    // running-but-unfocused Pane draws no hint.
    let running = focused && status == Some(Status::Streaming);
    let state = wall_state(
        status,
        decision.is_some(),
        wall.tests_failing,
        transcript.and_then(|t| t.last_cost()).is_some(),
    );
    // Attention and focus are two independent channels, and they no longer
    // nest (§D.1): the state edge is the Pane's own 1px border — always in
    // layout, only ever recoloured, so nothing reflows when a Decision
    // arrives — and focus is a 2px neutral ring 2px OUTSIDE it, painted by
    // `focus_wrapper` below.
    let edge: gpui::Hsla = if decision.is_some() {
        rgb(ATTENTION).into()
    } else if state == WallState::Blocked {
        rgb(BLOCKED).into()
    } else {
        rgba(TRANSPARENT).into()
    };
    let shell = pane_shell(edge);

    // Far enough away, a Pane is one signal: no header, no transcript,
    // nothing that stops reading at a glance.
    if level == Level::Wall {
        return focus_wrapper(
            shell.child(wall_cell(view, wall, state, focused, title)),
            focused,
        );
    }

    if level == Level::Instruments {
        // An L2 cell keeps its Composer: the operator types into a small
        // Pane as into a big one — a cell too small to read a transcript
        // is not too small to be told what to do next. No menu, picker
        // or band at this size; the keys still work.
        let composer = transcript.map(|transcript| {
            composer_region(
                view,
                Some(transcript),
                ComposerStack {
                    decision,
                    queued,
                    running,
                    empty: composer_empty,
                    history_available,
                    menu: None,
                    mode: permission_mode,
                    model_picker: None,
                    band: None,
                    focused,
                },
            )
        });
        return focus_wrapper(
            shell.child(l2_cell(
                view,
                transcript,
                decision,
                workspace,
                branch.as_ref(),
                state,
                timings,
                decide,
                title,
                composer,
            )),
            focused,
        );
    }

    let mut pane = shell.child(pane_head(view, branch.as_ref(), status, usage_ring, title));
    match transcript {
        Some(transcript) => {
            // The tasks strip sits directly under the header, full width,
            // exactly where the Main comp draws it — meter, the step being
            // worked, and the muted tag (#22 eyeball round).
            if let Some(todos) = transcript.todos() {
                pane = pane.child(tasks_strip(todos, transcript.current_task()));
            }
            pane = pane.child(body(
                view,
                transcript,
                level,
                &selection,
                timings,
                &mut tool_controls,
            ));
            // The Decision card is a **sibling of the body**, not a child
            // of the Composer (§D.5): its `margin: 0 12px 8px` is measured
            // from the Pane's own content box, so nesting it inside the
            // Composer's 12px padding would inset it twice. The child
            // order §D.1 pins is head · tasks · body · decision · changed
            // · composer.
            if let Some(decision) = decision {
                pane = pane.child(decision_card(decision, decide.take()));
            }
            // The CHANGED strip rides above the Composer whenever the
            // Thread has touched files (#22 C11). `Instruments::of` walks
            // every Block, per frame — the same price every L2 cell already
            // pays, and a window shows few L1 Panes; if a wall of L1 Panes
            // ever dips, the fix is the incremental fold docview.rs already
            // names, not a render-side cache.
            let changed = Instruments::of(transcript).changed;
            if !changed.is_empty() {
                pane = pane.child(changed_strip(&changed));
            }
            pane = pane.child(composer_region(
                view,
                Some(transcript),
                ComposerStack {
                    decision,
                    queued,
                    running,
                    empty: composer_empty,
                    history_available,
                    menu,
                    mode: permission_mode,
                    model_picker,
                    band: None,
                    focused,
                },
            ));
        }
        None => {
            pane = pane.child(parked_body());
        }
    }
    focus_wrapper(pane, focused)
}

/// The Pane box (§D.1): `--pane` ground, 8px radius, and a 1px border that
/// is **always in layout** — transparent at rest, amber on a Decision, red
/// when blocked — so a state change reflows nothing. `overflow: hidden`
/// clips the children to the radius. The mono family is declared once
/// here: everything inside a Pane inherits it, everything outside keeps
/// the system sans the root declares.
fn pane_shell(edge: gpui::Hsla) -> Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .min_w_0()
        .bg(rgb(PANE))
        .border_1()
        .border_color(edge)
        .rounded(px(theme::R_SURFACE))
        .font_family(theme::FONT_MONO)
        .overflow_hidden()
}

/// The Pane, plus its focus ring. gpui has no `outline-offset`, and a ring
/// painted inside the shell's `overflow_hidden()` is clipped away
/// entirely — so the ring lives in a **non-clipping** wrapper as an
/// absolute overlay at `inset(-4px)`: 2px wide, 2px outside the border
/// box, radii following the offset (inner 10, outer 12). It reaches 4px
/// into the 8px board gap, which the gap exactly accommodates, and it
/// coexists with the innermost 1px state border rather than nesting inside
/// it.
fn focus_wrapper(shell: Div, focused: bool) -> Div {
    div()
        .relative()
        .flex()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(shell)
        .children(focused.then(|| {
            div()
                .absolute()
                .inset(px(-(theme::FOCUS_RING_OFFSET + theme::FOCUS_RING_W)))
                .rounded(px(theme::R_SURFACE
                    + theme::FOCUS_RING_OFFSET
                    + theme::FOCUS_RING_W))
                .border(px(theme::FOCUS_RING_W))
                .border_color(rgb(FOCUS))
        }))
}

/// The Decision card's `inset 0 0 0 1px` ring — gpui has no inset
/// box-shadow, so it is an absolute full-size overlay that takes no events
/// and no layout. Its radius must match the card it rings.
fn ring_overlay(color: u32, radius: f32) -> Div {
    div()
        .absolute()
        .inset_0()
        .rounded(px(radius))
        .border_1()
        .border_color(rgba(color))
}

// ------------------------------------------------------------------ L3 wall

/// The Wall board's cell recipe: 8px padding, 6px gaps, top-anchored —
/// dot · slug name · 5px bar · one 9px status line; alert states carry a
/// 10px colored first line instead of the bar.
/// Everything a draft Pane draws (#29), assembled in the cockpit where the
/// clicks are wired — the Pane only lays it out.
pub struct DraftState<'a> {
    /// The pre-prompt band: the [provider][project][workspace] chip row.
    pub band: AnyElement,
    /// The open band popover, hung above the Composer like every menu.
    pub menu: Option<AnyElement>,
    pub composer_empty: bool,
    pub focused: bool,
    /// A failed bootstrap's words, shown where the band is.
    pub error: Option<&'a SharedString>,
}

/// A draft Pane (#29): an empty transcript area and the Composer wearing
/// the pre-prompt band. Below L1 a draft is a quiet placeholder cell — the
/// band only exists where a Composer does, and nothing is running that the
/// instruments could show.
pub fn render_draft(view: &PaneView, state: DraftState<'_>, level: Level) -> impl IntoElement {
    let DraftState {
        band,
        menu,
        composer_empty,
        focused,
        error,
    } = state;
    let shell = pane_shell(rgba(TRANSPARENT).into());

    if level != Level::Transcript {
        return focus_wrapper(
            shell.child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .text_size(px(theme::FS_SM))
                    .text_color(rgb(TEXT_MUTED))
                    .child("draft"),
            ),
            focused,
        );
    }

    let mut wrapped = div().flex().flex_col().flex_shrink_0();
    wrapped = wrapped.child(band);
    if let Some(error) = error {
        wrapped = wrapped.child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .pb(px(4.))
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(BLOCKED))
                .child(div().min_w_0().whitespace_normal().child(error.clone())),
        );
    }
    focus_wrapper(
        shell
            .child(pane_head(view, None, None, None, None))
            .child(div().flex().flex_1().min_h_0())
            .child(composer_region(
                view,
                None,
                ComposerStack {
                    decision: None,
                    queued: None,
                    running: false,
                    empty: composer_empty,
                    history_available: false,
                    menu,
                    mode: None,
                    model_picker: None,
                    band: Some(wrapped.into_any_element()),
                    focused,
                },
            )),
        focused,
    )
}

/// The pre-prompt band's own row (#29): chips left, the key path's hint at
/// the right edge — the Composer meta row's grammar, one step above the
/// prompt line.
pub fn draft_band() -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .h(px(theme::TASKS_STRIP_H))
}

/// The band's key-path hint, riding the row's right edge.
pub fn band_hint() -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(theme::FS_MONO))
        .text_color(rgb(TEXT_MUTED))
        .child("⇥ chips · ↵ send")
}

/// One band chip. The prototype draws no draft band (R-09), so the chip is
/// retinted onto the nearest tokens it does define rather than redesigned:
/// the chip recipe's `--raised` ground and `--text-2` ink for the marked
/// slot, `--text-muted` otherwise. The 1px border is always in layout and
/// only changes colour — tab's focus promotes it to `--focus`, because the
/// popover opens on ↵ and the chip must say where ↵ will land.
pub fn band_chip(slot: usize, label: SharedString, accent: bool, focused: bool) -> Stateful<Div> {
    let edge: gpui::Hsla = if focused {
        rgb(FOCUS).into()
    } else {
        rgba(TRANSPARENT).into()
    };
    div()
        .id(("band-chip", slot))
        .flex_shrink_0()
        .text_size(px(theme::FS_MONO))
        .text_color(rgb(if accent { TEXT_2 } else { TEXT_MUTED }))
        .when(accent, |chip| chip.bg(rgb(RAISED)))
        .border_1()
        .border_color(edge)
        .rounded(px(theme::R_CHIP))
        .px(px(theme::CHIP_PAD_X))
        .py(px(theme::CHIP_PAD_Y))
        .hover_control()
        .press_control()
        .child(label)
}

/// A band chip's text: the choice plus the ⌵ that says it answers clicks.
pub fn band_chip_label(choice: &str) -> SharedString {
    SharedString::from(format!("{choice} ⌵"))
}

fn wall_cell(
    view: &PaneView,
    card: &WallCard,
    state: WallState,
    focused: bool,
    title: Option<AnyElement>,
) -> Div {
    let (dot_color, hollow) = match state {
        WallState::Working | WallState::Failing | WallState::Done => (RUNNING, false),
        WallState::Decision => (ATTENTION, false),
        WallState::Blocked => (BLOCKED, false),
        WallState::Idle => (IDLE, false),
        WallState::Parked => (SEP, true),
    };
    let mut cell = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap(px(theme::GRID_GAP))
        .p(px(theme::GRID_PAD))
        .overflow_hidden()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .min_w_0()
                .child(if hollow {
                    hollow_dot(px(theme::LED_WALL))
                } else {
                    led(px(theme::LED_WALL), dot_color)
                })
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_family(theme::FONT_UI)
                        .text_size(px(theme::FS_MONO))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(if focused { TEXT_STRONG } else { TEXT_2 }))
                        .child(match title {
                            Some(title) => title,
                            None => div().truncate().child(view.name.clone()).into_any_element(),
                        }),
                ),
        );
    // The meter survives on working cells only; alert cells trade it for
    // the colored first line (glance.md §3.4). Glyphs, not a bar fill: the
    // slanted ▰▱ run is the one meter language (#22 operator ruling).
    if matches!(state, WallState::Working | WallState::Failing) && !card.meter.is_empty() {
        cell = cell.child(
            div()
                .flex_shrink_0()
                .w_full()
                .truncate()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_2))
                .child(card.meter.clone()),
        );
    }
    // `w_full`, not `min_w_0`: a flex-col child measured at its own width
    // truncates everything to an ellipsis smear (#22 A1).
    let status_line = |text: SharedString, size: f32, color: u32| {
        div()
            .flex_shrink_0()
            .w_full()
            .truncate()
            .text_size(px(size))
            .text_color(rgb(color))
            .child(text)
    };
    match state {
        WallState::Working => {
            cell = cell.child(status_line(
                card.working.clone(),
                theme::FS_MONO,
                TEXT_MUTED,
            ));
        }
        WallState::Failing => {
            cell = cell.child(status_line(card.failing.clone(), theme::FS_MONO, BLOCKED));
        }
        WallState::Decision => {
            cell = cell.child(status_line(
                SharedString::from("⚠ needs you"),
                theme::FS_MONO,
                ATTENTION,
            ));
            if !card.context.is_empty() {
                cell = cell.child(status_line(
                    card.context.clone(),
                    theme::FS_MONO,
                    TEXT_MUTED,
                ));
            }
        }
        WallState::Blocked => {
            // The close reason is the alert; the disposition is the
            // context (#22 C14).
            cell = cell.child(status_line(card.context.clone(), theme::FS_MONO, BLOCKED));
            cell = cell.child(status_line(
                SharedString::from("blocked"),
                theme::FS_MONO,
                TEXT_MUTED,
            ));
        }
        WallState::Done => {
            cell = cell
                .child(status_line(
                    SharedString::from("✓ done"),
                    theme::FS_MONO,
                    RUNNING,
                ))
                .opacity(theme::DONE_WALL_OPACITY);
        }
        WallState::Idle => {
            cell = cell.child(status_line(
                SharedString::from("❯ idle"),
                theme::FS_MONO,
                TEXT_MUTED,
            ));
        }
        WallState::Parked => {
            cell = cell.child(status_line(
                SharedString::from("parked"),
                theme::FS_MONO,
                SEP,
            ));
        }
    }
    cell
}

// ------------------------------------------------------------- L2 cell

/// The Cockpit board's cell grammar: 24px header (LED · title · right meta),
/// then instruments — progress row, badge row, and the bottom-pinned
/// activity line. A pending Decision swaps the body for the y/n card and an
/// idle Thread centers `❯ idle`.
#[allow(clippy::too_many_arguments)]
fn l2_cell(
    view: &PaneView,
    transcript: Option<&Transcript>,
    decision: Option<&Decision>,
    workspace: Option<&WorkspaceBinding>,
    branch: Option<&SharedString>,
    state: WallState,
    timings: Option<&HashMap<String, ToolTiming>>,
    decide: Option<AnyElement>,
    title: Option<AnyElement>,
    composer: Option<Div>,
) -> Div {
    let hot = matches!(
        state,
        WallState::Working | WallState::Failing | WallState::Decision | WallState::Blocked
    );
    let led_color = match state {
        WallState::Decision => ATTENTION,
        WallState::Blocked | WallState::Failing => BLOCKED,
        WallState::Working | WallState::Done => RUNNING,
        WallState::Idle => IDLE,
        WallState::Parked => SEP,
    };
    let mut header = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::CELL_HEADER_H))
        .gap(px(6.))
        .px(px(8.))
        .child(led(px(theme::STATUS_DOT), led_color))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_family(theme::FONT_UI)
                .text_size(px(theme::FS_SM))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(if hot { TEXT_STRONG } else { TEXT_2 }))
                .child(match title {
                    Some(title) => title,
                    None => div().truncate().child(view.name.clone()).into_any_element(),
                }),
        )
        .child(div().flex_1());
    // The amber ring is the chip (#22 D17): a Decision cell's right meta
    // keeps the binding like every other cell.
    header = match state {
        WallState::Done => header.child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(RUNNING))
                .child("done"),
        ),
        // The comp's right-meta slot carries the Thread's id; the name is
        // already the id here, so the slot names the Workspace binding —
        // what an operator running many Threads actually needs.
        _ => header.child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child(binding_label(workspace)),
        ),
    };

    let cell = div().flex().flex_col().flex_1().min_h_0();
    let Some(transcript) = transcript else {
        return cell.child(header).child(parked_body());
    };

    // A Decision's cell body is the card, keyed like the in-Pane card.
    if let Some(decision) = decision {
        return cell.child(header).child(
            l2_decision_body(decision, decide)
                .key_context("Decision")
                .track_focus(&view.decision_focus),
        );
    }

    let read = Instruments::of(transcript);
    let mut body = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .p(px(theme::CELL_PAD))
        .gap(px(8.));

    // The facts the head has no room for at this size: the model serving
    // and the checkout, one muted line — the two things an operator
    // running nine of these asks first.
    let meta: Vec<String> = [
        transcript
            .model()
            .map(ferrite_core::providers::models::display_name),
        branch.map(|branch| branch.to_string()),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect();
    if !meta.is_empty() {
        body = body.child(
            div()
                .w_full()
                .truncate()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child(SharedString::from(meta.join(" · "))),
        );
    }

    if state == WallState::Idle {
        // Idle still shows the tail of the conversation: where the Thread
        // stopped, in its own words, newest at the bottom.
        body = body.child(l2_tail(transcript));
        body = body.child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child("❯ idle — waiting for work"),
        );
        return cell.child(header).child(body).children(composer);
    }

    if state == WallState::Done {
        body = body.child(
            div()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child("turn complete"),
        );
    }

    if let Some(todos) = read.todos {
        // Meter fill follows health: accent while green, secondary while
        // the suite is red (the Cockpit board's two data points). Glyphs,
        // not a bar fill — the ▰▱ run is the one meter language.
        let fill = if state == WallState::Failing {
            TEXT_MUTED
        } else {
            TEXT_2
        };
        body = body.child(
            div()
                .w_full()
                .truncate()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(fill))
                .child(meter(todos.done, todos.total)),
        );
    }

    let mut badges = div().flex().items_center().gap(px(6.));
    let mut any_badge = false;
    match read.tests {
        Some(Tests::Passed { count }) => {
            let label = match count {
                Some(count) => SharedString::from(format!("✓ {count}")),
                None => SharedString::from("✓ tests pass"),
            };
            badges = badges.child(chip(label, RUNNING, rgba(RUNNING_WASH).into()));
            any_badge = true;
        }
        Some(Tests::Failed { count }) => {
            let label = match count {
                Some(count) => SharedString::from(format!("✗ {count} failing")),
                None => SharedString::from("✗ failing"),
            };
            badges = badges.child(chip(label, BLOCKED, rgba(BLOCKED_WASH).into()));
            any_badge = true;
        }
        None => {}
    }
    if read.added > 0 || read.removed > 0 {
        badges = badges.child(
            diff_stat(read.added, read.removed)
                .text_size(px(theme::FS_MONO))
                .bg(rgb(RAISED))
                .rounded(px(theme::R_CHIP))
                .px(px(6.))
                .py(px(1.)),
        );
        any_badge = true;
    }
    if read.files() > 0 {
        badges = badges.child(
            div()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child(SharedString::from(format!(
                    "{} file{}",
                    read.files(),
                    if read.files() == 1 { "" } else { "s" }
                ))),
        );
        any_badge = true;
    }
    if any_badge {
        body = body.child(badges);
    }
    // The tail of the conversation fills what is left: prompts, answers
    // and tool rows in one compact column, newest at the bottom — what
    // the Thread is saying, not only that it is saying something.
    body = body.child(l2_tail(transcript));
    if state == WallState::Done {
        body = body.child(
            div()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child("❯ idle"),
        );
    } else if let Some(activity) = read.activity {
        // The running call's clock rides the line — "◐ Bash cargo check
        // — 8s" — where the cockpit stamped one (#22 amendment).
        let clocked = read
            .running_call
            .as_deref()
            .and_then(|call| timings?.get(call))
            .map(|timing| format!("◐ {activity} — {}", duration_label(timing.elapsed())))
            .unwrap_or_else(|| format!("◐ {activity}"));
        body = body.child(
            div()
                .w_full()
                .truncate()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child(SharedString::from(clocked)),
        );
    }

    let mut content = cell.child(header).child(body).children(composer);
    if state == WallState::Done {
        content = content.opacity(theme::DONE_CELL_OPACITY);
    }
    content
}

/// How many Blocks an L2 tail reaches back for.
const L2_TAIL_BLOCKS: usize = 16;
/// How many lines one Block may take in the tail before it is cut.
const L2_TAIL_LINES: usize = 4;

/// The compact tail of a transcript for an L2 cell: the newest Blocks as
/// single runs — a prompt on its raised ground, prose in the reading ink,
/// a tool row as its glyph and call, a Notice in its signal weight — each
/// clamped to a few lines, the column anchored to its bottom and clipped
/// at the top, so whatever height the cell has shows the newest words.
fn l2_tail(transcript: &Transcript) -> Div {
    let blocks = transcript.blocks();
    let tail = &blocks[blocks.len().saturating_sub(L2_TAIL_BLOCKS)..];
    let mut column = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .w_full()
        .justify_end()
        .overflow_hidden()
        .gap(px(4.))
        .text_size(px(theme::FS_MONO))
        .line_height(relative(theme::LINE_BODY));
    for block in tail {
        let line = |text: String, ink: u32| {
            div()
                .w_full()
                .line_clamp(L2_TAIL_LINES)
                .text_color(rgb(ink))
                .child(SharedString::from(text))
        };
        let prose = |spans: &[Span]| -> String {
            spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>()
                .trim()
                .to_string()
        };
        let drawn = match &block.body {
            Body::Prompt(text) => {
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                line(text.to_string(), TEXT_STRONG)
                    .px(px(theme::CHIP_PAD_X))
                    .py(px(theme::CHIP_PAD_Y))
                    .rounded(px(theme::R_CHIP))
                    .bg(rgb(RAISED))
            }
            Body::Paragraph { spans } | Body::Bullet { spans } => {
                let text = prose(spans);
                if text.is_empty() {
                    continue;
                }
                line(text, TEXT_2)
            }
            Body::Heading { spans, .. } => {
                let text = prose(spans);
                if text.is_empty() {
                    continue;
                }
                line(text, TEXT_STRONG).font_weight(FontWeight::SEMIBOLD)
            }
            Body::Code { language, .. } => line(
                format!("```{}", language.as_deref().unwrap_or("")),
                TEXT_MUTED,
            ),
            Body::Tool(tool) => {
                let glyph = if tool.name == "TaskCreate" || tool.name == "TaskUpdate" {
                    "●"
                } else {
                    "▸"
                };
                let ink = match tool.state {
                    ToolState::Failed(_) => BLOCKED,
                    _ => TEXT_MUTED,
                };
                line(format!("{glyph} {} {}", tool.name, tool.summary), ink).line_clamp(1)
            }
            Body::Notice(text) => line(text.clone(), ATTENTION).font_weight(FontWeight::SEMIBOLD),
            Body::Meta(text) => line(text.clone(), TEXT_MUTED),
            Body::Thinking(_) => continue,
        };
        column = column.child(drawn);
    }
    column
}

/// The Cockpit board's Decision cell body: the command, who wants it, and
/// the y/n keycaps — no `a always` at L2. The whole group hangs directly
/// under the header; a spacer here would strand the keycaps on the cell
/// floor with dead black between (#22 A2). The keycaps arrive wired from
/// the cockpit (#26), like every other pointer.
fn l2_decision_body(decision: &Decision, decide: Option<AnyElement>) -> Div {
    let command = decision_subject(decision);
    let wants = decision_wants(decision);
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .p(px(theme::CELL_PAD))
        .gap(px(6.))
        .child(
            div()
                .w_full()
                .truncate()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_STRONG))
                .child(command),
        )
        .child(
            div()
                .w_full()
                .truncate()
                .font_family(theme::FONT_UI)
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child(wants),
        )
        .children(decide)
}

// ---------------------------------------------------------------- L1 pane

/// The Pane head (§D.2): 32px, 12px inline padding, an 8px gap, muted ink.
/// Status dot · Thread id · checkout · the context ring pinned to the
/// trailing edge. No background, no border, and **no rule beneath it** —
/// Soft separates by fill contrast alone. There is no model chip here (the
/// Composer's picker is the only model surface) and no window controls
/// (park and zoom stay on the keyboard).
fn pane_head(
    view: &PaneView,
    branch: Option<&SharedString>,
    status: Option<Status>,
    usage_ring: Option<AnyElement>,
    title: Option<AnyElement>,
) -> Div {
    // The dot's base is the muted ink — the parked look — and each live
    // state takes its own signal colour. The no-dot ruling is scoped to
    // navigation; a Pane head keeps its dot.
    let dot_color = match status {
        Some(Status::Streaming) => RUNNING,
        Some(Status::Blocked) => ATTENTION,
        Some(Status::Closed) => BLOCKED,
        _ => TEXT_MUTED,
    };
    let mut head = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::PANE_HEAD_H))
        .gap(px(theme::EVENT_GAP))
        .px(px(theme::PANE_PAD_X))
        .text_size(px(theme::FS_SM))
        .line_height(relative(theme::LINE_UI))
        .text_color(rgb(TEXT_MUTED))
        .child(led(px(theme::STATUS_DOT), dot_color))
        .child(
            div()
                .min_w_0()
                .flex_shrink()
                .text_size(px(theme::FS_LG))
                .line_height(relative(theme::LINE_UI))
                // gpui seats a run one pixel lower in this 32px head than
                // CSS half-leading does. 2px of bottom padding grows the
                // centred box by two and so lifts the glyphs by one.
                .pb(px(2.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_STRONG))
                .child(match title {
                    Some(title) => title,
                    None => div().truncate().child(view.name.clone()).into_any_element(),
                }),
        );
    // The checkout slot (#29): the branch mark and the cached branch name.
    // Pure text — no hover, no click target: the CWD is immutable once the
    // Thread runs and the head must never look otherwise. No `·` seam
    // before it; the 8px gap is the whole separation.
    if let Some(branch) = branch {
        head = head.child(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(theme::ROW_ICON_GAP))
                .text_size(px(theme::FS_MONO))
                .line_height(relative(theme::LINE_UI))
                .child(icon(icons::BRANCH, theme::ROW_ICON, TEXT_MUTED))
                // Same one-pixel lift as the Thread id; the mark keeps its
                // own centring.
                .child(div().min_w_0().truncate().pb(px(2.)).child(branch.clone())),
        );
    }
    // The ring is hard right and it is the last thing in the head. A
    // growing spacer, not `ml_auto` — see `tasks_strip` for why.
    match usage_ring {
        Some(ring) => head
            .child(div().flex_1().min_w_0())
            .child(div().flex_shrink_0().child(ring)),
        None => head,
    }
}

/// The head's title, saying it can be renamed: the name in its own ink
/// with the hover wash every control wears, truncating. Render-only; the
/// cockpit gives it its id and its double-click.
pub fn head_title(name: SharedString) -> Div {
    div()
        .min_w_0()
        .truncate()
        .px(px(theme::CHIP_PAD_X))
        .mx(px(-theme::CHIP_PAD_X))
        .rounded(px(theme::R_TIGHT))
        .child(name)
        .hover_control()
}

/// The tasks strip (§D.3): 24px, 12px inline padding, a 9px gap — the
/// segment meter, the done/total count in tabular numerals, the task being
/// worked, and the kind label riding the trailing edge. No ground, no
/// border, no separator.
///
/// **This meter is the tasks meter and nothing else.** Context usage is the
/// ring in the head, and never a bar.
fn tasks_strip(todos: Todos, current: Option<&str>) -> Div {
    let done = todos.done.min(todos.total);
    let mut meter = div().flex().flex_shrink_0().gap(px(theme::METER_SEG_GAP));
    for index in 0..todos.total {
        meter = meter.child(
            div()
                .w(px(theme::METER_SEG_W))
                .h(px(theme::METER_SEG_H))
                .rounded(px(theme::METER_SEG_R))
                .bg(if index < done {
                    rgb(TEXT_2).into()
                } else {
                    rgba(METER_OFF)
                }),
        );
    }
    let mut strip = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(9.))
        .h(px(theme::TASKS_STRIP_H))
        .px(px(theme::PANE_PAD_X))
        .text_size(px(theme::FS_SM))
        .line_height(relative(theme::LINE_UI))
        .text_color(rgb(TEXT_MUTED))
        .child(meter)
        .child(tabular(
            div()
                .flex_shrink_0()
                .child(SharedString::from(format!("{done}/{}", todos.total))),
        ));
    if let Some(current) = current {
        strip = strip.child(
            div()
                .min_w_0()
                .truncate()
                .child(SharedString::from(current.to_string())),
        );
    }
    // `margin-inline-start: auto` on the kind label — spelled as a growing
    // spacer, never `ml_auto`: an auto margin on any child makes taffy hand
    // that child the whole of the free space **including the container's
    // own gaps**, and every gap in the row silently collapses to 0. Measured
    // in this build; it is the reason the head, the tasks strip, the event
    // row and the Composer all lay their trailing element out this way.
    strip
        .child(div().flex_1().min_w_0())
        .child(div().flex_shrink_0().child("todo"))
}

/// `▰▰▰▱ 3/4` at the instrument levels. The prototype specifies only the
/// L1 Pane, where the tasks strip now draws real segments — so the glyph
/// meter survives for L2 and L3 alone, which keep the metrics they have.
fn meter(done: usize, total: usize) -> SharedString {
    if total == 0 {
        return SharedString::default();
    }
    let run = meter_run(done, total);
    let done = done.min(total);
    if run.is_empty() {
        return SharedString::from(format!("{done}/{total}"));
    }
    SharedString::from(format!("{run} {done}/{total}"))
}

/// The ▰▱ glyph run alone — the wall's meter, whose status line already
/// carries the fraction. Empty past the cap or without a plan.
fn meter_run(done: usize, total: usize) -> String {
    const GLYPH_CAP: usize = 8;
    if total == 0 || total > GLYPH_CAP {
        return String::new();
    }
    let done = done.min(total);
    let mut run = String::new();
    run.extend(std::iter::repeat_n('▰', done));
    run.extend(std::iter::repeat_n('▱', total - done));
    run
}

/// The name a model shows under, from any spelling the wire uses:
/// `claude-sonnet-4-5` → `Sonnet 4.5`, `gpt-5.4-mini` → `GPT-5.4 Mini`.
/// Public because every chip and row must spell a model exactly one way —
/// one grooming, never two; the catalog's own display names win where a
/// Session announced them (see `providers::models::label`).
pub fn model_label(model: &str) -> SharedString {
    SharedString::from(ferrite_core::providers::models::display_name(model))
}

/// The Composer's model picker (§D.7): a 20px, 4px-radius control on no
/// ground — a 12px logomark in its brand colour, the bare model name in
/// `--text-2`, and a 12px chevron. Hover lifts it to `--hover` / `--text`.
/// Render-only; the cockpit gives it its id and its click.
pub fn model_picker(provider: Option<Provider>, label: SharedString) -> Div {
    let mark = provider.map(|provider| match provider {
        Provider::Codex => icon(icons::CODEX, theme::PROVIDER_MARK_SM, theme::PROVIDER_CODEX),
        Provider::Claude => icon(
            icons::CLAUDE,
            theme::PROVIDER_MARK_SM,
            theme::PROVIDER_CLAUDE,
        ),
    });
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::KEYS_GAP))
        .h(px(theme::CHIP_H))
        .pl(px(theme::PICKER_PAD_L))
        .pr(px(theme::PICKER_PAD_R))
        .rounded(px(theme::R_CHIP))
        .text_size(px(theme::FS_SM))
        .text_color(rgb(TEXT_2))
        .children(mark)
        .child(div().flex_shrink_0().child(label))
        .child(icon(icons::CHEVRON_DOWN, theme::ICON_CHEVRON, TEXT_MUTED))
        .hover_control()
}

/// The rendered tail of a transcript at one level — the window `body`
/// draws and the selection overlay resolves against (#27). One function,
/// two callers, so the wash can never resolve against a different window
/// than is drawn.
pub fn rendered_window(blocks: &[Block], level: Level) -> &[Block] {
    let tail = blocks.len().saturating_sub(level.visible_blocks());
    &blocks[tail..]
}

/// Output-bearing tool rows in exactly the window L1 draws. Disclosure
/// cycling, focus validation, and controls all consume this one eligibility
/// rule so an invisible row can never remain keyboard-addressable.
pub fn rendered_output_tools(blocks: &[Block], level: Level) -> impl Iterator<Item = &ToolBlock> {
    rendered_window(blocks, level)
        .iter()
        .filter_map(|block| match &block.body {
            Body::Tool(tool) if tool.output.is_some() => Some(tool),
            _ => None,
        })
}

fn body(
    view: &PaneView,
    transcript: &Transcript,
    level: Level,
    selection: &SelectionOverlay,
    timings: Option<&HashMap<String, ToolTiming>>,
    tool_controls: &mut HashMap<String, AnyElement>,
) -> impl IntoElement {
    // Only Thread Panes have a transcript body; a draft never lands here.
    let thread = view.thread().map(|thread| thread.get()).unwrap_or(0);
    let mut body = div()
        .id(("transcript", thread as usize))
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(&view.scroll)
        // `padding: 6px 12px 12px` (§D.4). No gap: every block carries its
        // own margin now, so the rhythm is the prototype's, not a uniform
        // stack spacing.
        .px(px(theme::PANE_PAD_X))
        .pt(px(theme::BODY_PAD_T))
        .pb(px(theme::BODY_PAD_B))
        .text_size(px(theme::FS_MD))
        .line_height(relative(theme::LINE_BODY))
        .text_color(rgb(TEXT_2))
        // Characters here are grabbable (#27): the I-beam says so over the
        // whole scrollback, gutters and gaps included, because a press
        // anywhere in it anchors at the nearest character.
        .hover_text();
    // A `.signal` line wears the Pane's own state, so the line and the
    // Pane's border can never disagree.
    let signal = signal_color(Some(transcript.status()));
    let window = rendered_window(transcript.blocks(), level);
    let mut prev_margin_b = 0.;
    for (index, block) in window.iter().enumerate() {
        let next_is_bullet = matches!(
            window.get(index + 1).map(|next| &next.body),
            Some(Body::Bullet { .. })
        );
        let flow = Flow {
            prev_margin_b,
            next_is_bullet,
        };
        prev_margin_b = margin_b(&block.body, next_is_bullet);
        body = body.child(render_block(
            block,
            selection,
            timings,
            view.tool_state(match &block.body {
                Body::Tool(tool) => &tool.call,
                _ => "",
            }) == DisclosureState::Expanded,
            match &block.body {
                Body::Tool(tool) => tool_controls.remove(&tool.call),
                _ => None,
            },
            signal,
            flow,
        ));
    }
    body
}

/// What CSS collapsing needs to know about a Block's neighbours — gpui adds
/// adjacent margins where CSS collapses them, so the two cases the prototype
/// actually shows are carried explicitly: a heading's 12px top margin
/// collapses into whatever the previous block put below it, and a bullet run
/// ends on the `ul`'s 10px rather than the `li`'s 3px.
#[derive(Clone, Copy, Default)]
struct Flow {
    prev_margin_b: f32,
    next_is_bullet: bool,
}

/// The bottom margin a Block actually renders with — the other half of the
/// collapse.
fn margin_b(body: &Body, next_is_bullet: bool) -> f32 {
    match body {
        Body::Bullet { .. } if next_is_bullet => theme::LI_GAP,
        Body::Heading { .. } => theme::H4_MARGIN_B,
        Body::Tool(_) => 0.,
        _ => theme::P_MARGIN_B,
    }
}

fn parked_body() -> Div {
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .items_center()
        .justify_center()
        .text_size(px(theme::FS_SM))
        .text_color(rgb(TEXT_MUTED))
        .child("parked")
}

// --------------------------------------------------------------- Composer

/// The Composer stack's slice of `PaneState`, bundled so `composer_region`
/// stays readable as the states grow.
struct ComposerStack<'a> {
    decision: Option<&'a Decision>,
    queued: Option<&'a str>,
    running: bool,
    empty: bool,
    history_available: bool,
    menu: Option<AnyElement>,
    mode: Option<&'a str>,
    /// The Composer's model picker (#25) — drawn in every Pane.
    model_picker: Option<AnyElement>,
    /// The draft Pane's pre-prompt band (#29) — Some on drafts only, and
    /// gone with the first send.
    band: Option<AnyElement>,
    /// Whether this Pane holds the keyboard. The Composer paints its own
    /// caret when it does; the `›` mark stands in when it does not, and
    /// the two are mutually exclusive (§D.7).
    focused: bool,
}

/// The Composer (§D.7): `--raised` ground, `padding: 7px 12px 8px`, two
/// 20px rows 3px apart — 58px in every Pane. No top border: Soft draws no
/// separators, and the ground change is the whole separation.
///
/// Above the two rows, still inside the region, stack the draft band (#29)
/// and the queued row — neither of which the prototype draws (R-09). The
/// Decision card is **not** here: it is a sibling of the body, drawn by
/// `render_pane`. While a Decision pends this region still carries the
/// `Decision` key context, so y/n/a answer with the keyboard in the
/// Composer (#23).
fn composer_region(view: &PaneView, transcript: Option<&Transcript>, stack: ComposerStack) -> Div {
    let ComposerStack {
        decision,
        queued,
        running,
        empty,
        history_available,
        menu,
        mode,
        model_picker,
        band,
        focused,
    } = stack;
    let is_draft = band.is_some();
    let mut region = div()
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(px(theme::COMPOSER_GAP))
        .min_w_0()
        .bg(rgb(RAISED))
        // gpui's `overflow_hidden()` content mask is an axis-aligned rect, so
        // the shell's 8px radius never clips this ground. The bottom-most
        // child carries the shell's padding-box radius itself: 8 - 1 border.
        .rounded_bl(px(theme::R_SURFACE - 1.))
        .rounded_br(px(theme::R_SURFACE - 1.))
        .pt(px(theme::COMPOSER_PAD_T))
        .px(px(theme::PANE_PAD_X))
        .pb(px(theme::COMPOSER_PAD_B))
        .text_size(px(theme::FS_SM))
        .line_height(relative(theme::LINE_UI))
        .text_color(rgb(TEXT_2))
        .when(decision.is_some(), |region| region.key_context("Decision"));
    if let Some(band) = band {
        region = region.child(band);
    }
    if let Some(held) = queued {
        region = region.child(queued_line(held));
    }
    // The one line that grows. The idle placeholder overlays it in every
    // Pane that does not hold the keyboard — the prototype keeps it under a
    // running turn (§D.7) and shows the focused Pane its caret alone.
    let mut line = div()
        .relative()
        .flex_1()
        .min_w_0()
        .child(view.composer.clone());
    if empty && !focused {
        line = line.child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .flex()
                .items_center()
                .text_color(rgb(TEXT_2))
                .child(placeholder(decision.is_some(), transcript)),
        );
    }
    // `.composer-prompt`: the `›` mark when the Pane is not focused — the
    // Composer paints its own 2 × 14 caret when it is, and the two are
    // mutually exclusive. No `◐`, no `❯`.
    let mut input = div()
        .flex()
        .items_center()
        .gap(px(theme::EVENT_GAP))
        .min_h(px(theme::COMPOSER_ROW_H))
        .min_w_0();
    if !focused {
        input = input.child(
            div()
                .flex_shrink_0()
                .text_color(rgb(TEXT_MUTED))
                .child("\u{203a}"),
        );
    }
    input = input.child(line).child(
        // `.hint`: hard right, nowrap, 10.5px muted. The line beside it is
        // already `flex_1`, so it takes the free space and no auto margin
        // is needed — one would collapse the row's gap.
        div()
            .flex_shrink_0()
            .whitespace_nowrap()
            .text_size(px(theme::FS_MONO))
            .text_color(rgb(TEXT_MUTED))
            .child(composer_hints(is_draft, history_available)),
    );
    region = region.child(input);
    // The popover paints above the stack — deferred, so it escapes the
    // Pane's clip and draws over the transcript (#24).
    if let Some(menu) = menu {
        region = region.child(deferred(
            div()
                .absolute()
                .bottom(relative(1.))
                .left_0()
                .right_0()
                .mb(px(6.))
                .child(menu),
        ));
    }

    // `.composer-controls`: the mode chip, then the esc hint — which does
    // **not** push right here — then the model picker hard right. No cost,
    // no token count, no context text.
    let mut controls = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::EVENT_GAP))
        .h(px(theme::COMPOSER_ROW_H));
    // The chip is the *running* Session's edit mode, so it rides only a
    // Pane that is running and unblocked: a Decision owns the keyboard until
    // it is answered, and a closed Session has no mode to be in. The
    // prototype draws it on its two running Panes and omits it from the
    // Decision and the blocked one.
    if let Some(mode) = mode.filter(|_| running && decision.is_none()) {
        controls = controls.child(mode_chip(mode));
    }
    let escape = if decision.is_some() {
        Some("esc dismiss")
    } else if running {
        Some("esc interrupt")
    } else {
        None
    };
    if let Some(escape) = escape {
        controls = controls.child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child(escape),
        );
    }
    // `margin-inline-start: auto` on the picker. It renders in every Pane,
    // before and after the first-prompt lock — there is no plain-label
    // fallback and no second model surface anywhere.
    if let Some(picker) = model_picker {
        controls = controls
            .child(div().flex_1().min_w_0())
            .child(div().flex_shrink_0().child(picker));
    }
    region.child(controls)
}

/// The Composer's mode chip (§D.7): 20px on `--hover` at rest, 7px inline
/// padding, a 10px pencil and the mode's own word. Hover lifts it to
/// `--fill` / `--text`.
fn mode_chip(mode: &str) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::KEYS_GAP))
        .h(px(theme::CHIP_H))
        .px(px(theme::MODE_CHIP_PAD_X))
        .rounded(px(theme::R_CHIP))
        .bg(rgb(HOVER))
        .text_color(rgb(TEXT_2))
        .child(icon(icons::PENCIL, theme::ICON_PENCIL, TEXT_MUTED))
        .child(mode_chip_label(mode))
        .hover_raised()
}

fn composer_hints(is_draft: bool, history_available: bool) -> &'static str {
    if is_draft {
        "@ project files · /import"
    } else if history_available {
        "↑ history · @ files · / commands"
    } else {
        "@ files · / commands"
    }
}

/// The idle line's ghost text (§D.7): one of the prototype's three, chosen
/// by what the Pane is waiting on — a Decision, a live Thread, or a closed
/// Session. It never names the Thread and never lists the hints; the `.hint`
/// on the same row already does that.
fn placeholder(pending: bool, transcript: Option<&Transcript>) -> SharedString {
    if pending {
        return SharedString::from("Reply to the Decision\u{2026}");
    }
    match transcript.map(|transcript| transcript.status()) {
        Some(Status::Closed) => SharedString::from("Revive and continue\u{2026}"),
        _ => SharedString::from("Steer this Thread\u{2026}"),
    }
}

/// #11: whether this Thread still offers adopting a CLI session — no
/// conversation yet (nothing in the transcript beyond Ferrite's own notices
/// and bookkeeping) and at rest. One predicate for every surface that opens
/// the door — the placeholder hint, the `/` menu's local entry, and the
/// pick that closes the blank Thread — so no two can disagree.
pub fn offers_import(transcript: Option<&Transcript>) -> bool {
    transcript.is_some_and(Transcript::offers_import)
}

/// The meta row's mode chip text: the comp's own name for acceptEdits
/// ("⏵ auto-edit"); every other mode wears the provider's word verbatim
/// rather than a guessed translation.
fn mode_chip_label(mode: &str) -> SharedString {
    let label = match mode {
        "acceptEdits" => "auto-edit",
        other => other,
    };
    SharedString::from(label.to_string())
}

/// One row of the `/` or `@` popover, ready to draw: what a pick inserts,
/// what the row shows, and where the fuzzy filter matched. Prepared by the
/// cockpit when the menu changes — never per frame.
pub struct MenuRow {
    /// What lands in the line on ↵ — a command name, or a file's relative
    /// path.
    pub insert: SharedString,
    /// The row's leading text: `/name`, or the file's name.
    pub name: SharedString,
    /// Matched byte ranges inside `name`, promoted to `--text-strong`.
    pub matched: Vec<std::ops::Range<usize>>,
    /// The dimmer text after it: a command's description, or the file's
    /// directory. Empty draws nothing.
    pub detail: SharedString,
    /// Whether `detail` reads as prose (the comp's ui-face command
    /// descriptions) or as a path (mono, like the rows of state 03).
    pub prose_detail: bool,
    /// A row kept visible but dead (#25's locked provider door): muted ink,
    /// no match highlights, and its pick does nothing but dismiss.
    pub inert: bool,
}

/// The Composer menus' popover shell: the selector's exact surface at the
/// composer's own width (the comps draw slash/@ popovers spanning the box).
pub fn menu_popover() -> Div {
    popover_shell().w_full()
}

/// One 30px menu row, on the filter menu's recipe (R-07). Selection takes
/// the `--hover` ground, promotes the name and its matched characters to
/// `--text-strong` (semibold only while selected) and steps the detail ink
/// up; the selected row carries the `↵` hint at its right edge.
pub fn menu_row(row: &MenuRow, selected: bool) -> Div {
    // An inert row never promotes: muted whatever the arrows do, and its
    // matches stay unpainted — the row is an explanation, not an offer.
    let name_ink = match (row.inert, selected) {
        (true, _) => TEXT_MUTED,
        (false, true) => TEXT_STRONG,
        (false, false) => TEXT_2,
    };
    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    if !row.inert {
        for range in &row.matched {
            highlights.push((
                range.clone(),
                HighlightStyle {
                    color: Some(rgb(TEXT_STRONG).into()),
                    font_weight: selected.then_some(FontWeight::SEMIBOLD),
                    ..Default::default()
                },
            ));
        }
    }
    let mut drawn = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(10.))
        .h(px(theme::MENU_ROW_H))
        .px(px(8.))
        .rounded(px(theme::R_CONTROL))
        .when(selected, |row| row.bg(rgb(HOVER)))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::FS_MD))
                .text_color(rgb(name_ink))
                .child(StyledText::new(row.name.clone()).with_highlights(highlights)),
        );
    // The Row role (#26): the selected row skips the wash — hover would
    // downgrade its EDGE ground — but keeps the cursor; an inert row gets
    // neither, for the same reason it carries no ↵ hint.
    drawn = match (row.inert, selected) {
        (true, _) => drawn,
        (false, true) => drawn.hover_carried(),
        (false, false) => drawn.hover_row(),
    };
    if !row.detail.is_empty() {
        let detail_ink = if selected { TEXT_MUTED } else { TEXT_MUTED };
        let mut detail = div()
            .min_w_0()
            .truncate()
            .text_color(rgb(detail_ink))
            .child(row.detail.clone());
        detail = if row.prose_detail {
            detail
                .font_family(theme::FONT_UI)
                .text_size(px(theme::FS_SM))
        } else {
            detail.text_size(px(theme::FS_MONO))
        };
        drawn = drawn.child(detail);
    }
    // No ↵ hint on an inert row: enter only dismisses there, and a keycap
    // would advertise an offer the row does not make.
    if selected && !row.inert {
        drawn = drawn.child(div().flex_1()).child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child("↵"),
        );
    }
    drawn
}

/// A prompt written while the agent was still working — the ⏳ queued row.
fn queued_line(held: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::EVENT_GAP))
        .h(px(theme::CELL_HEADER_H))
        .text_size(px(theme::FS_SM))
        .child(
            div()
                .flex_shrink_0()
                .text_color(rgb(TEXT_MUTED))
                .child("⏳"),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .italic()
                .text_color(rgb(TEXT_MUTED))
                .child(SharedString::from(format!("queued — \"{held}\""))),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child("⌫ unqueue"),
        )
}

/// The Decision card (§D.5): a sibling of the body, not a child of it.
/// `margin: 0 12px 8px` so it aligns with the body's own inset,
/// `padding: 8px 10px`, a 4px radius on the amber wash, and a 1px amber
/// **inset** ring — an overlay, because it must take no layout. Warning
/// mark, the subject and wants lines, then the keycaps.
///
/// Kept free of focus and key wiring so it can be drawn — and smoke-
/// rendered — on its own; the keycaps arrive wired from the cockpit (#26).
fn decision_card(decision: &Decision, decide: Option<AnyElement>) -> Div {
    let subject = decision_subject(decision);
    let wants = decision_wants(decision);
    div()
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .min_w_0()
        .gap(px(theme::DECISION_GAP))
        .mx(px(theme::DECISION_MARGIN_X))
        .mb(px(theme::DECISION_MARGIN_B))
        .px(px(theme::DECISION_PAD_X))
        .py(px(theme::DECISION_PAD_Y))
        .rounded(px(theme::R_CHIP))
        .bg(rgba(ATTENTION_WASH))
        .child(ring_overlay(ATTENTION_EDGE, theme::R_CHIP))
        .child(icon(icons::WARNING, theme::ICON_WARNING, ATTENTION))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_size(px(theme::FS_MD))
                        .line_height(relative(theme::LINE_UI))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_STRONG))
                        .child(subject),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(theme::FS_SM))
                        .line_height(relative(theme::LINE_UI))
                        .text_color(rgb(TEXT_MUTED))
                        .child(wants),
                ),
        )
        .children(decide)
}

/// The Decision's subject — what it wants to do, tool-prefixed the comps'
/// way: `Bash: gh issue close 212`; the tool's name alone without a
/// description, else the honest unreadable fallback. Every surface that
/// names a Decision (L1 card, L2 cell, wall alert) goes through here.
fn decision_subject(decision: &Decision) -> SharedString {
    match (
        decision.tool_name.is_empty(),
        decision.description.is_empty(),
    ) {
        (false, false) => {
            SharedString::from(format!("{}: {}", decision.tool_name, decision.description))
        }
        (true, false) => SharedString::from(decision.description.clone()),
        (false, true) => SharedString::from(decision.tool_name.clone()),
        (true, true) => SharedString::from("unreadable permission request"),
    }
}

/// A Decision card's subtitle — `Write · wants approval`, carrying the
/// request's cwd when it names one — or the unreadable fallback when the
/// provider named no tool.
fn decision_wants(decision: &Decision) -> SharedString {
    if decision.tool_name.is_empty() {
        return SharedString::from("the provider sent a request Ferrite could not read");
    }
    match decision.input.get("cwd").and_then(|cwd| cwd.as_str()) {
        Some(cwd) => SharedString::from(format!("{} · wants approval · {cwd}", decision.tool_name)),
        None => SharedString::from(format!("{} · wants approval", decision.tool_name)),
    }
}

/// One keycap (§D.5): `padding: 3px 7px`, a 4px radius on `--raised`,
/// 10.5px `--text-2` — and **no border**. The key letter leads in `--text`
/// at weight 600 and the label follows in the cap's own ink — one text run
/// with a highlight over the letter, not two sibling elements, so the cap
/// rounds once rather than once per span and keeps the prototype's width.
/// The label doubles as the element id the pressed shade tracks; two
/// keycaps never share one in a card.
fn keycap(id: &'static str, key: &'static str, label: &'static str, ink: u32) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .text_size(px(theme::FS_MONO))
        .line_height(relative(theme::LINE_UI))
        .text_color(rgb(ink))
        .bg(rgb(RAISED))
        .rounded(px(theme::R_CHIP))
        .px(px(theme::KBD_PAD_X))
        .py(px(theme::KBD_PAD_Y))
        .hover_raised()
        .press_raised()
        .child(
            StyledText::new(SharedString::from(format!("{key}{label}"))).with_highlights(vec![(
                0..key.len(),
                HighlightStyle {
                    color: Some(rgb(TEXT).into()),
                    font_weight: Some(FontWeight::SEMIBOLD),
                    ..Default::default()
                },
            )]),
        )
}

/// The decide keycaps, one constructor per verb, so the cockpit can wire
/// each press without respelling the keycap grammar (#26).
pub fn keycap_allow() -> Stateful<Div> {
    keycap("y allow", "y", " allow", TEXT_2)
}
pub fn keycap_deny() -> Stateful<Div> {
    keycap("n deny", "n", " deny", TEXT_2)
}
pub fn keycap_always() -> Stateful<Div> {
    keycap("a always", "a", " always", TEXT_2)
}

/// The keycaps' cluster: 5px apart in the L1 card (§D.5), packed at 4 in
/// the L2 body, which the prototype does not specify.
pub fn decide_row(level: Level) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(if level == Level::Transcript {
            theme::KEYS_GAP
        } else {
            4.
        }))
}

// ------------------------------------------------------------ shared bits

fn led(size: gpui::Pixels, color: u32) -> Div {
    div()
        .flex_shrink_0()
        .w(size)
        .h(size)
        .rounded_full()
        .bg(rgb(color))
}

/// A parked LED: the ring without the fill — present, not running.
fn hollow_dot(size: gpui::Pixels) -> Div {
    div()
        .flex_shrink_0()
        .w(size)
        .h(size)
        .rounded_full()
        .border_1()
        .border_color(rgb(SEP))
}

/// The changed-files strip (§D.6): 24px, 12px inline padding, an 8px gap —
/// the word `CHANGED`, then one chip per file carrying its diff stat. No
/// top border: Soft draws no separators.
fn changed_strip(changed: &[ferrite_core::docview::FileChange]) -> Div {
    let mut strip = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .min_w_0()
        .gap(px(theme::EVENT_GAP))
        .h(px(theme::CHANGED_STRIP_H))
        .px(px(theme::PANE_PAD_X))
        .text_size(px(theme::FS_MONO))
        .line_height(relative(theme::LINE_UI))
        .text_color(rgb(TEXT_2))
        .overflow_hidden()
        .child(tracked("CHANGED", TEXT_MUTED));
    for file in changed {
        let name = Path::new(&file.path)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| file.path.clone());
        strip = strip.child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .min_w_0()
                .gap(px(theme::FILE_CHIP_GAP))
                .px(px(theme::CHIP_PAD_X))
                .py(px(theme::CHIP_PAD_Y))
                .rounded(px(theme::R_CHIP))
                .bg(rgb(RAISED))
                .child(div().min_w_0().truncate().child(SharedString::from(name)))
                .child(diff_stat(file.added, file.removed)),
        );
    }
    strip
}

/// `CHANGED` at 0.08em tracking — the only letter-spacing in Soft + Sans.
/// gpui's `TextStyle` has no tracking field, so the label is one element
/// per character on a 0.84px flex gap. CSS also adds a trailing gap after
/// the last character; this row is 0.84px narrower for it, which is below
/// the noise floor on a seven-character string (R-04).
fn tracked(label: &'static str, ink: u32) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .gap(px(theme::CHANGED_TRACKING))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(ink))
        .children(label.chars().map(|glyph| {
            div()
                .flex_shrink_0()
                .w(px(theme::FS_MONO * theme::MONO_ADVANCE))
                .child(glyph.to_string())
        }))
}

/// `+N −N` (§E.12): the added count in `--running`, **a literal space**,
/// then the removed count in `--blocked` with a U+2212 MINUS SIGN — never a
/// hyphen. The space is the gap; there is no flex gap here. One pair, drawn
/// in exactly two places: an event's trail and a changed-strip chip.
fn diff_stat(added: usize, removed: usize) -> Div {
    // ONE text run, not three siblings: gpui rounds every run's advance up
    // to a whole pixel, so `+2`/space/`\u{2212}1` as three elements measures
    // 33px where the prototype measures 31.53px and the chip around it
    // overruns by 2px. The two halves are coloured with highlights instead.
    let plus = format!("+{added}");
    let minus = format!("\u{2212}{removed}");
    let text = format!("{plus} {minus}");
    let removed_at = plus.len() + 1;
    let highlights = vec![
        (
            0..plus.len(),
            HighlightStyle {
                color: Some(rgb(RUNNING).into()),
                ..Default::default()
            },
        ),
        (
            removed_at..removed_at + minus.len(),
            HighlightStyle {
                color: Some(rgb(BLOCKED).into()),
                ..Default::default()
            },
        ),
    ];
    tabular(
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .text_size(px(theme::FS_MONO))
            .child(StyledText::new(SharedString::from(text)).with_highlights(highlights)),
    )
}

/// The one chip recipe the prototype draws (§E.11, `.pass`):
/// `padding: 1px 6px`, a 4px radius, 11px ink on its own ground. The
/// ground arrives resolved because the prototype's own chip sits on a
/// translucent wash while the R-09 stand-ins sit on the opaque `--raised`,
/// and `rgb`/`rgba` read a `u32`'s bytes differently.
fn chip(label: impl Into<SharedString>, ink: u32, ground: gpui::Hsla) -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(theme::FS_SM))
        .text_color(rgb(ink))
        .bg(ground)
        .rounded(px(theme::R_CHIP))
        .px(px(theme::CHIP_PAD_X))
        .py(px(theme::CHIP_PAD_Y))
        .child(label.into())
}

/// `8.2s` under ten seconds, `42s` under a minute, `2m14s` beyond — the
/// comps' duration grammar, shared by tool rows and activity lines. The
/// smallest value the prototype prints is `0.1s`, so a sub-tenth call
/// rounds up into it rather than reading `0.0s`.
fn duration_label(elapsed: Duration) -> SharedString {
    let secs = elapsed.as_secs_f64().max(0.1);
    if secs < 10.0 {
        SharedString::from(format!("{secs:.1}s"))
    } else if secs < 60.0 {
        SharedString::from(format!("{}s", secs as u64))
    } else {
        let whole = secs as u64;
        SharedString::from(format!("{}m{:02}s", whole / 60, whole % 60))
    }
}

/// The context ring (§G.10): a 14px box holding a 5.4px-radius, 2px-stroke
/// circle — a `--meter-off` track under a `--text-2` arc that sweeps
/// clockwise from 12 o'clock with the used fraction of the window.
///
/// **No text, no percentage, no `ctx`, ever.** The ring is the whole
/// affordance: there is no hover card and no token count behind it.
///
/// `PathBuilder::arc_to` draws the real arc — gpui 0.2.2 has an arc
/// primitive, whatever the old comment here claimed.
pub fn usage_ring(fraction: f32) -> Div {
    // A full ring's seam would degenerate the arc; one part in a thousand
    // is invisible at 14px.
    let fraction = fraction.clamp(0.0, 1.0).min(0.999);
    div()
        .relative()
        .flex_shrink_0()
        .w(px(theme::USAGE_RING_D))
        .h(px(theme::USAGE_RING_D))
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    // The circle the prototype draws is `USAGE_RING_R` /
                    // `USAGE_RING_W`; these are what gpui has to be *asked*
                    // for to land on it. lyon's arc approximation pulls the
                    // curve inward by ~0.32px and the stroke rasterises
                    // ~0.5px thin, so the ink measured 12.0px across where
                    // the prototype measures 12.7px. The compensation lives
                    // here, at the rasteriser, and never in theme.rs.
                    const ARC_R: f32 = theme::USAGE_RING_R + 0.15;
                    const ARC_W: f32 = theme::USAGE_RING_W + 0.25;
                    let radius = px(ARC_R);
                    let centre = bounds.center();
                    let sweep = fraction * std::f32::consts::TAU;
                    let start = -std::f32::consts::FRAC_PI_2;
                    let at = |angle: f32| {
                        point(
                            centre.x + radius * angle.cos(),
                            centre.y + radius * angle.sin(),
                        )
                    };
                    // The caps are quads, not paths: they rasterise exactly,
                    // so they sit on the true centreline at the true radius.
                    let cap_at = |angle: f32| {
                        point(
                            centre.x + px(theme::USAGE_RING_R) * angle.cos(),
                            centre.y + px(theme::USAGE_RING_R) * angle.sin(),
                        )
                    };
                    let stroke = |from: f32, to: f32, large: bool| {
                        let mut arc = PathBuilder::stroke(px(ARC_W));
                        arc.move_to(at(from));
                        arc.arc_to(point(radius, radius), px(0.), large, true, at(to));
                        arc.build().ok()
                    };
                    // The unlit track is the same circle as the used arc —
                    // painted, not a bordered box, because gpui rounds a
                    // box's inset to a whole pixel and the ring's radius is
                    // 5.4. Drawn as two halves; a closed circle would
                    // degenerate the arc.
                    if let Some(path) = stroke(start, start + std::f32::consts::PI, false) {
                        window.paint_path(path, rgba(METER_OFF));
                    }
                    if let Some(path) = stroke(
                        start + std::f32::consts::PI,
                        start + std::f32::consts::TAU - 0.001,
                        false,
                    ) {
                        window.paint_path(path, rgba(METER_OFF));
                    }
                    if fraction <= 0.0 {
                        return;
                    }
                    if let Some(path) = stroke(start, start + sweep, fraction > 0.5) {
                        window.paint_path(path, rgb(TEXT_2));
                    }
                    // `.used` carries `stroke-linecap: round`; lyon's
                    // default is butt and gpui 0.2.2 re-exports no
                    // `LineCap`, so each cap is painted as its own disc of
                    // the stroke's radius.
                    let cap = px(theme::USAGE_RING_W / 2.0);
                    for angle in [start, start + sweep] {
                        let end = cap_at(angle);
                        window.paint_quad(
                            gpui::fill(
                                gpui::Bounds::new(
                                    point(end.x - cap, end.y - cap),
                                    gpui::size(cap * 2., cap * 2.),
                                ),
                                rgb(TEXT_2),
                            )
                            .corner_radii(gpui::Corners::all(cap)),
                        );
                    }
                },
            )
            .absolute()
            .inset_0(),
        )
}

/// Which checkout a Thread works in — a worktree's own name, or "main" for
/// the shared one. One line, because an operator running many Threads has to
/// know which of them can trample the others. Shared with the nav's rows
/// (#21), so both surfaces name a binding the same way.
pub fn binding_label(workspace: Option<&WorkspaceBinding>) -> SharedString {
    match workspace {
        Some(WorkspaceBinding::Worktree { path, .. }) => SharedString::from(
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "worktree".into()),
        ),
        Some(WorkspaceBinding::Main { .. }) => SharedString::from("main"),
        None => SharedString::from(""),
    }
}

// The interactive session-project-root selector (#24) is deleted, not
// dormant (#29): an interactive path to change where a Thread works
// post-lock is exactly what must not exist. `binding_label` above survives
// for the nav's rows; the header's binding slot is the display-only branch
// text now.

/// Every popover's shell. The prototype draws exactly one menu — the
/// Project filter — and the Composer's `/` and `@` menus have no Soft
/// form of their own (R-07), so they are restyled onto the filter menu's
/// recipe rather than given a second menu language: the `--menu` ground,
/// a 10px radius, 4px of padding, `--shadow-float`'s **two** layers, and
/// **no border**. Width is the caller's. Rows and footer are the
/// cockpit's to append — their clicks are wired there.
fn popover_shell() -> Div {
    div()
        .flex()
        .flex_col()
        .p(px(theme::MENU_PAD))
        .bg(rgb(theme::MENU))
        .rounded(px(theme::R_MENU))
        .shadow(vec![
            BoxShadow {
                color: rgba(theme::SHADOW_FAR).into(),
                offset: point(px(0.), px(theme::SHADOW_FAR_Y)),
                blur_radius: px(theme::SHADOW_FAR_BLUR),
                spread_radius: px(theme::SHADOW_FAR_SPREAD),
            },
            BoxShadow {
                color: rgba(theme::SHADOW_NEAR).into(),
                offset: point(px(0.), px(theme::SHADOW_NEAR_Y)),
                blur_radius: px(theme::SHADOW_NEAR_BLUR),
                spread_radius: px(0.),
            },
        ])
}

/// The ✓-row recipe the pickers share — the provider picker (#25) and the
/// band popovers (#29) — so "what this Pane is on right now" can never be
/// spelled two ways. `detail` is the muted section tag riding the right
/// edge ("provider", "worktree"); empty draws nothing.
pub fn picker_row(
    label: SharedString,
    detail: SharedString,
    selected: bool,
    active: bool,
    inert: bool,
) -> Div {
    let mut row = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(10.))
        .h(px(theme::MENU_ROW_H))
        .px(px(8.))
        .rounded(px(theme::R_CONTROL))
        .text_size(px(theme::FS_MD))
        .text_color(rgb(if inert {
            TEXT_MUTED
        } else if selected {
            TEXT_STRONG
        } else {
            TEXT_2
        }))
        .child(div().min_w_0().truncate().child(label))
        .child(div().flex_1());
    // The Row role (#26), the menu rows' skip rule: the selected row's
    // EDGE ground outranks the wash, so it keeps only the cursor. An
    // inert row is dead: no wash, no cursor — it explains, it never acts.
    row = if inert {
        row
    } else if selected {
        row.bg(rgb(HOVER)).hover_carried()
    } else {
        row.hover_row()
    };
    if !detail.is_empty() {
        row = row.child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child(detail),
        );
    }
    if active {
        row = row.child(icon(icons::CHECK, theme::ROW_ICON, TEXT));
    }
    row
}

/// A picker's section title: the Provider's logomark in its brand colour
/// and its name, with an optional muted note after it (why the section is
/// fixed). Non-interactive — the arrows skip it, a press does nothing.
pub fn picker_section(provider: Provider, note: SharedString) -> Div {
    let mark = match provider {
        Provider::Codex => icon(icons::CODEX, theme::PROVIDER_MARK_SM, theme::PROVIDER_CODEX),
        Provider::Claude => icon(
            icons::CLAUDE,
            theme::PROVIDER_MARK_SM,
            theme::PROVIDER_CLAUDE,
        ),
    };
    let title = match provider {
        Provider::Claude => "Claude",
        Provider::Codex => "Codex",
    };
    let mut row = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::KEYS_GAP))
        .h(px(theme::MENU_ROW_H))
        .px(px(8.))
        .mt(px(2.))
        .text_size(px(theme::FS_SM))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT))
        .child(mark)
        .child(div().child(title));
    if !note.is_empty() {
        row = row.child(
            div()
                .ml(px(theme::KEYS_GAP))
                .font_weight(FontWeight::NORMAL)
                .text_size(px(theme::FS_MONO))
                .text_color(rgb(TEXT_MUTED))
                .child(note),
        );
    }
    row
}

/// A muted, non-interactive picker line — why a section is short, said out
/// loud (#25: the other rows only arrive with the Session's handshake).
#[allow(dead_code)]
pub fn picker_hint(text: &'static str) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::MENU_ROW_H))
        .px(px(8.))
        .text_size(px(theme::FS_MONO))
        .text_color(rgb(TEXT_MUTED))
        .child(text)
}

/// The popover's key-hint footer — the PromptBox footer grammar, each
/// menu supplying its own verbs.
pub fn popover_footer(hints: &'static str) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::CHIP_H))
        .px(px(8.))
        .mt(px(2.))
        .text_size(px(theme::FS_MONO))
        .text_color(rgb(TEXT_MUTED))
        .child(hints)
}

// ----------------------------------------------------------- Block render

/// One Block in the prototype's transcript vocabulary (§E). The body draws
/// **no gutter at all** for prose: paragraphs, headings and list items sit
/// flush at the content edge, and the only glyphs left are the event row's
/// `▸`/`●` and the result line's `└`, all in `--sep`. Spacing is per-block
/// margins, not a stack gap.
///
/// Every text run routes through the selection overlay (#27) — that is what
/// makes it selectable and copyable; the disc markers, chips, elbows and
/// diff line numbers around the runs are chrome, and stay plain.
fn render_block(
    block: &Block,
    selection: &SelectionOverlay,
    timings: Option<&HashMap<String, ToolTiming>>,
    expanded: bool,
    disclosure: Option<AnyElement>,
    signal: u32,
    flow: Flow,
) -> AnyElement {
    let row = div().w_full().flex_shrink_0();
    match &block.body {
        // The operator's own line stands apart from the answer: a raised,
        // content-sized block in the strong ink — the one surface in the
        // column with a ground of its own, so a glance tells who said
        // what. No `❯`, no gutter: the ground is the whole marker.
        // Laid out exactly like a paragraph (stretched, capped at 68ch —
        // see `paragraph` for why the width is dropped), so the wrap is
        // measured at the width it is painted.
        Body::Prompt(line) => paragraph(row, TEXT_STRONG)
            .px(px(theme::CODE_PAD_X))
            .py(px(theme::PROMPT_PAD_Y))
            .rounded(px(theme::R_CONTROL))
            .bg(rgb(RAISED))
            .child(selection.line(block.id, line.clone(), Vec::new()))
            .into_any_element(),
        Body::Paragraph { spans } => paragraph(row, TEXT_2)
            .child(prose(block.id, spans, selection))
            .into_any_element(),
        // `h4`: `margin: 12px 0 6px`, the same 12px as body text —
        // distinguished by weight and colour only (§E.2).
        Body::Heading { spans, .. } => row
            .mt(px((theme::H4_MARGIN_T - flow.prev_margin_b).max(0.)))
            .mb(px(theme::H4_MARGIN_B))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(TEXT_STRONG))
            .child(prose(block.id, spans, selection))
            .into_any_element(),
        // `li`: a 16px indent, 3px below its siblings — 10px below the last
        // of the run, where the `ul`'s own margin takes over — and an
        // explicit 4px disc, gpui drawing no list markers. The disc sits
        // 15px left of the text and 8.3px below the line box's top (§E.3,
        // pixel-measured).
        Body::Bullet { spans } => row
            .relative()
            .pl(px(theme::UL_INDENT))
            .mb(px(if flow.next_is_bullet {
                theme::LI_GAP
            } else {
                theme::P_MARGIN_B
            }))
            .child(
                div()
                    .absolute()
                    .left(px(theme::UL_INDENT - theme::BULLET_OFFSET))
                    .top(px(8.3))
                    .w(px(theme::BULLET_D))
                    .h(px(theme::BULLET_D))
                    .rounded_full()
                    .bg(rgb(TEXT_2)),
            )
            .child(prose(block.id, spans, selection))
            .into_any_element(),
        // Thinking has no prototype counterpart (R-09): it reads as a
        // `.note` paragraph rather than growing a class of its own.
        // A provider ends a thinking run with a trailing newline; drawing it
        // would add a fourth, empty line box and push the next Block a whole
        // line too far. A `p` has no trailing blank line (§E.1).
        Body::Thinking(thought) => paragraph(row, TEXT_MUTED)
            .child(selection.line(
                block.id,
                thought.trim_end_matches('\n').to_string(),
                Vec::new(),
            ))
            .into_any_element(),
        // A Notice is the prototype's `.signal` line (§E.8): 12px/600,
        // 10px below, coloured by the Pane's own state — muted at rest,
        // amber while a Decision waits, red once the Session closed.
        Body::Notice(text) => row
            .mb(px(theme::P_MARGIN_B))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(signal))
            .child(selection.line(block.id, text.clone(), separators(text)))
            .into_any_element(),
        // Meta, likewise, is a `.note` paragraph (R-09).
        Body::Meta(text) => paragraph(row, TEXT_MUTED)
            .child(selection.line(block.id, text.clone(), Vec::new()))
            .into_any_element(),
        // `.codeblock` (§E.7): 4px radius on `--raised`, a language label
        // at `5px 10px 0`, then the `pre` at `4px 10px 8px`. No border, no
        // language bar, no rule between them.
        Body::Code {
            language,
            source,
            tokens,
        } => row
            .mb(px(theme::P_MARGIN_B))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .bg(rgb(RAISED))
                    .rounded(px(theme::R_CHIP))
                    .overflow_hidden()
                    .text_size(px(theme::FS_MONO))
                    .line_height(relative(theme::LINE_BODY))
                    .children(language.as_ref().map(|language| {
                        div()
                            .px(px(theme::CODE_PAD_X))
                            .pt(px(theme::CODE_LANG_PAD_T))
                            .text_color(rgb(TEXT_MUTED))
                            .child(SharedString::from(language.clone()))
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .px(px(theme::CODE_PAD_X))
                            .pt(px(theme::CODE_PRE_PAD_T))
                            .pb(px(theme::CODE_PRE_PAD_B))
                            .text_color(rgb(TEXT_2))
                            // One child per hard line. Handed the whole
                            // multi-line source, the shaper drops the run of
                            // spaces that opens each inner line and every row
                            // of the block lands flush left; `pre` keeps that
                            // indent, and indentation is code.
                            .children(code_lines(
                                block.id,
                                source,
                                code(source, tokens.as_deref()),
                                selection,
                            )),
                    ),
            )
            .into_any_element(),
        Body::Tool(tool) => render_tool(
            row, block.id, tool, selection, timings, expanded, disclosure,
        ),
    }
}

/// `p` (§E.1): `margin: 0 0 10px`, colour from the caller — `--text-2`
/// for prose, `--text-muted` for a `.note`. The prototype capped prose at
/// 68ch; the operator ruled that out — a wide Pane left half its width
/// empty while tool rows ran the whole column — so prose runs the full
/// content column like everything else in it.
fn paragraph(mut row: Div, ink: u32) -> Div {
    // No width of its own: taffy resolves a flex item's `width: 100%`
    // against the container and hands that figure to the measure function
    // as the item's flex base size, which is fine while nothing clamps it
    // — but a stretched item is the shape every other Block takes, and the
    // width it is measured at is then the width it is painted at.
    row.style().size.width = None;
    row.mb(px(theme::P_MARGIN_B)).text_color(rgb(ink))
}

/// `.signal .sep` (§E.8): the interpunct joining a signal's state to its
/// detail is `--sep` at weight 400, dimmer than the semibold run either
/// side of it. Highlighted in place so the line stays one run and copies
/// back exactly as written.
fn separators(text: &str) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    text.match_indices('\u{b7}')
        .map(|(at, dot)| {
            (
                at..at + dot.len(),
                HighlightStyle {
                    color: Some(rgb(SEP).into()),
                    font_weight: Some(FontWeight::NORMAL),
                    ..Default::default()
                },
            )
        })
        .collect()
}

/// Which colour a `.signal` line wears — the Pane's own state, so the line
/// and the Pane's border can never disagree.
fn signal_color(status: Option<Status>) -> u32 {
    match status {
        Some(Status::Blocked) => ATTENTION,
        Some(Status::Closed) => BLOCKED,
        _ => TEXT_MUTED,
    }
}

/// Tabular numerals, for every count and duration that must not jitter as
/// digits change. JetBrains Mono is monospaced, so this is belt and braces
/// — but the prototype declares it and the token is cheap to honour.
fn tabular<E: Styled>(mut element: E) -> E {
    element
        .text_style()
        .get_or_insert_with(Default::default)
        .font_features = Some(FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])));
    element
}

/// `.event` (§E.9): `▸ Verb (args)` with its `.trail` hard right, then the
/// `└` result line beneath it and the bare hunk under that. Baseline
/// alignment, an 8px gap, 3px of block padding; the glyph column is 9px and
/// the gap 8, so 17px is where a result and a hunk land — under the verb's
/// first character. Keep that relationship, not the number.
///
/// The call composes name, `(`, summary, `)` as overlay pieces of one
/// copied line (#27): flex pieces keep the summary-only truncation, and
/// copy joins them with nothing. The glyph, the chips, the durations and
/// the elbow are chrome and never register.
fn render_tool(
    row: Div,
    block: BlockId,
    tool: &ToolBlock,
    selection: &SelectionOverlay,
    timings: Option<&HashMap<String, ToolTiming>>,
    expanded: bool,
    disclosure: Option<AnyElement>,
) -> AnyElement {
    // A task event wears the `●` glyph and a medium, muted verb; every
    // other tool call wears `▸` and a semibold verb in `--text`.
    let task = matches!(tool.name.as_str(), "TaskCreate" | "TaskUpdate");
    let (glyph, verb_weight, verb_ink) = if task {
        ("●", FontWeight::MEDIUM, TEXT_MUTED)
    } else {
        ("▸", FontWeight::SEMIBOLD, TEXT)
    };
    let mut call = div().flex().min_w_0().items_baseline().gap(px(0.)).child(
        div()
            .flex_shrink_0()
            .font_weight(verb_weight)
            .text_color(rgb(verb_ink))
            .child(selection.line(block, tool.name.clone(), Vec::new())),
    );
    if !tool.summary.is_empty() {
        // The args are one colour and one size — 10.5px muted, wrapped in
        // literal parentheses. There is no file/command split any more.
        let args = |piece: SharedString| {
            div()
                .text_size(px(theme::FS_MONO))
                .line_height(relative(theme::LINE_BODY))
                .child(selection.piece(block, piece, Vec::new()))
        };
        call = call
            .child(
                div().flex_shrink_0().ml(px(theme::EVENT_GAP)).child(
                    div()
                        .text_size(px(theme::FS_MONO))
                        .line_height(relative(theme::LINE_BODY))
                        .child(selection.piece(block, "(", Vec::new())),
                ),
            )
            .child(args(tool.summary.clone().into()).min_w_0().truncate())
            .child(args(")".into()).flex_shrink_0());
    }
    // The prototype draws `▸`/`●` in the glyph column and no chevron
    // anywhere, so the glyph is unconditional — and the disclosure rides on
    // top of it as an invisible hit target rather than replacing it. A tool
    // row with output stays openable by pointer and by keyboard without the
    // Pane drawing one pixel the prototype does not.
    let gutter = div()
        .relative()
        .flex_shrink_0()
        .w(px(theme::GUTTER_W))
        .text_color(rgb(SEP))
        .child(glyph)
        .children(disclosure);
    let mut line = div()
        .flex()
        .flex_row()
        .items_baseline()
        .min_w_0()
        .gap(px(theme::EVENT_GAP))
        .py(px(theme::EVENT_PAD_Y))
        .text_size(px(theme::FS_SM))
        .line_height(relative(theme::LINE_BODY))
        .text_color(rgb(TEXT_MUTED))
        .child(gutter)
        .child(call);
    // A settled call's clock, where the cockpit stamped one; running calls
    // tick on the activity line instead. Only a settled *tool* call carries
    // a time — the prototype ends each non-task trail with one and gives a
    // `.event.task` row no trail at all — and a sub-tenth blip rounds up to
    // `0.1s` in `duration_label` rather than vanishing.
    let settled_clock = if task {
        None
    } else {
        timings
            .and_then(|map| map.get(&tool.call))
            .and_then(|timing| match timing {
                ToolTiming::Done(total) => Some(*total),
                ToolTiming::Running(_) => None,
            })
    };
    // A pass chip that carries the run's own count subsumes the result
    // line it was promoted from; a countless chip keeps the line, which
    // still says more than the chip does.
    let mut promoted = false;
    let mut verdicts: Vec<AnyElement> = tool_verdicts(tool)
        .into_iter()
        .map(|verdict| match verdict {
            ToolVerdict::Diff(added, removed) => diff_stat(added, removed).into_any_element(),
            // `failed` has no prototype form (R-09): the `.pass` chip
            // recipe in the blocked hue, never a new value.
            ToolVerdict::Failed => {
                chip("failed", BLOCKED, rgba(BLOCKED_WASH).into()).into_any_element()
            }
        })
        .collect();
    if verdicts.is_empty() && matches!(tool.name.as_str(), "Bash" | "commandExecution") {
        if matches!(tool.state, ToolState::Ok) {
            let label = if is_test_run(tool) {
                match tool.result_line.as_deref().and_then(passed_count) {
                    Some(count) => {
                        promoted = true;
                        SharedString::from(format!("{count} passed"))
                    }
                    None => SharedString::from("passed"),
                }
            } else {
                SharedString::from("exit 0")
            };
            // The prototype draws exactly one verdict chip, `.pass`. A
            // command that merely exited 0 has no prototype form (R-09):
            // it takes the same chip on the quiet `--raised` ground.
            let (ink, ground) = if is_test_run(tool) {
                (RUNNING, rgba(RUNNING_WASH).into())
            } else {
                (TEXT_MUTED, rgb(RAISED).into())
            };
            verdicts.push(chip(label, ink, ground).into_any_element());
        }
    }
    // `.trail`: `margin-inline-start: auto`, an 8px gap, hard right.
    if !verdicts.is_empty() || settled_clock.is_some() {
        let mut trail = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(theme::EVENT_GAP))
            .children(verdicts);
        if let Some(total) = settled_clock {
            trail = trail.child(tabular(
                div()
                    .flex_shrink_0()
                    .text_size(px(theme::FS_MONO))
                    .line_height(relative(theme::LINE_BODY))
                    .text_color(rgb(TEXT_MUTED))
                    .child(duration_label(total)),
            ));
        }
        line = line.child(div().flex_1().min_w_0()).child(trail);
    }
    let mut card = row.flex().flex_col().child(line);
    if expanded {
        if let Some(output) = &tool.output {
            let ink = if matches!(tool.state, ToolState::Failed(_)) {
                BLOCKED
            } else {
                TEXT_MUTED
            };
            // One row per hard line, each a stretched block under the
            // elbow — the prompt block's lesson (see `paragraph`): a run
            // handed to a flex row is measured at min-content and wraps a
            // character per line.
            card = card.child(output_block(block, &output.text, ink, selection));
            if output.omitted_bytes > 0 {
                card = card.child(result_line(TEXT_MUTED).child(div().min_w_0().child(format!(
                    "… {} bytes omitted from inline view",
                    output.omitted_bytes
                ))));
            }
        }
    } else if !promoted {
        if let Some(line) = &tool.result_line {
            card = card.child(
                result_line(TEXT_MUTED).child(div().min_w_0().truncate().child(selection.line(
                    block,
                    line.clone(),
                    Vec::new(),
                ))),
            );
        }
    }
    if !expanded {
        if let ToolState::Failed(message) = &tool.state {
            card = card.child(result_line(BLOCKED).child(
                div().min_w_0().truncate().child(selection.line(
                    block,
                    message.clone(),
                    Vec::new(),
                )),
            ));
        }
    }
    if let Some(diff) = &tool.diff {
        card = card.child(render_diff(block, diff, selection));
    }
    card.into_any_element()
}

/// An expanded tool's output: the `└` elbow on the first line, then every
/// hard line stretched to the column under it, each wrapping at the
/// column's width. Blank lines keep their height so the shape of the
/// output survives.
fn output_block(block: BlockId, text: &str, ink: u32, selection: &SelectionOverlay) -> Div {
    let mut rows = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .pl(px(theme::INDENT))
        .pt(px(1.))
        .text_size(px(theme::FS_MONO))
        .line_height(relative(theme::LINE_BODY))
        .text_color(rgb(ink));
    for (index, line) in text.split('\n').enumerate() {
        let run = if line.is_empty() {
            selection.line(block, " ", Vec::new())
        } else {
            selection.line(block, line.to_string(), Vec::new())
        };
        let mut row = div().flex().w_full().min_w_0().gap(px(theme::EVENT_GAP));
        row = row.child(
            div()
                .flex_shrink_0()
                .w(px(theme::FS_MONO * theme::MONO_ADVANCE))
                .text_color(rgb(SEP))
                .child(if index == 0 { "└" } else { " " }),
        );
        let mut body = div().flex_1().min_w_0().child(run);
        body.style().size.width = None;
        rows = rows.child(row.child(body));
    }
    rows
}

/// `.result` (§E.10): `padding: 1px 0 3px 17px`, an 8px gap, 10.5px muted
/// — with the `└` elbow in `--sep`. The 17px inset is exactly the event's
/// glyph column plus its gap, so the elbow lands under the verb's first
/// character.
fn result_line(ink: u32) -> Div {
    div()
        .flex()
        .min_w_0()
        .w_full()
        .gap(px(theme::EVENT_GAP))
        .pl(px(theme::INDENT))
        // §E.10 is `1px 0 3px`, but gpui seats this 10.5px/1.55 run about
        // two pixels higher in the box than CSS half-leading does, so the
        // padding is swapped end for end: the 20.275px box — and the 43px
        // event-to-event span — are unchanged, the ink lands 19px under
        // the tool row's.
        .pt(px(theme::RESULT_PAD_T))
        .pb(px(theme::RESULT_PAD_B))
        .text_size(px(theme::FS_MONO))
        .line_height(relative(theme::LINE_BODY))
        .text_color(rgb(ink))
        .child(div().flex_shrink_0().text_color(rgb(SEP)).child("└"))
}

#[derive(Debug, PartialEq, Eq)]
enum ToolVerdict {
    Diff(usize, usize),
    Failed,
}

fn tool_verdicts(tool: &ToolBlock) -> Vec<ToolVerdict> {
    let mut verdicts = Vec::with_capacity(2);
    if let Some(diff) = &tool.diff {
        verdicts.push(ToolVerdict::Diff(diff.added, diff.removed));
    }
    if matches!(tool.state, ToolState::Failed(_)) {
        verdicts.push(ToolVerdict::Failed);
    }
    verdicts
}

/// The only clickable part of a tool row. Its pointer role and pressed
/// treatment make the chevron's hit target honest while the row stays text.
pub fn tool_disclosure_control(
    call: &str,
    expanded: bool,
    targeted: bool,
    focus: &FocusHandle,
) -> Stateful<Div> {
    // Nothing is drawn. The prototype puts `▸` in the glyph column and no
    // chevron anywhere, so the control is the glyph's own square: an
    // invisible hit target laid over the gutter the row already paints.
    // `expanded` therefore turns nothing — the disclosed output below the
    // row is the whole tell, which is what the prototype shows too.
    let _ = expanded;
    div()
        .id(SharedString::from(format!("tool-disclosure-{call}")))
        .absolute()
        .left(px((theme::GUTTER_W - theme::TOOL_DISCLOSURE_HIT) / 2.))
        .top(px(-1.))
        .w(px(theme::TOOL_DISCLOSURE_HIT))
        .h(px(theme::TOOL_DISCLOSURE_HIT))
        .rounded(px(theme::R_TIGHT))
        // Keyboard cycling is the one time the target has to be visible:
        // without a ground the operator cannot see which row `tab` is on.
        // The pointer never triggers it.
        .when(targeted, |control| {
            control
                .bg(rgb(SELECTION))
                .track_focus(focus)
                .key_context("ToolDisclosure")
        })
        .hover_control()
        .press_control()
}

/// `.hunk` (§E.13): no card, no filename header — the event above already
/// names the file. `margin: 4px 0 10px 17px` so it aligns under the verb, a
/// 4px radius clipping the first and last rows' outer corners, 8px inline
/// padding, a 24px right-aligned number column, a 7px sign column, 10px
/// between columns, and full-bleed washes on the added and removed rows.
///
/// The code cells route through the overlay — their lines copy honestly;
/// the number and sign columns are chrome and never do (#27).
fn render_diff(block: BlockId, diff: &Diff, selection: &SelectionOverlay) -> impl IntoElement {
    let mut lines = div()
        .flex()
        .flex_col()
        .mt(px(theme::HUNK_MARGIN_T))
        .mb(px(theme::P_MARGIN_B))
        .ml(px(theme::INDENT))
        .rounded(px(theme::R_CHIP))
        .overflow_hidden()
        .text_size(px(theme::FS_MONO))
        // Pinned to a whole pixel. At `relative(LINE_HUNK)` each row box is
        // 17.325px, so consecutive rows round their origin and their height
        // independently and the added/removed washes can leave a 1px
        // unpainted seam between them.
        .line_height(px((theme::FS_MONO * theme::LINE_HUNK).round()))
        .text_color(rgb(TEXT_MUTED));
    for hunk in &diff.hunks {
        let mut old = hunk.old_start;
        let mut new = hunk.new_start;
        for line in &hunk.lines {
            // The prototype signs a removal with U+2212 MINUS SIGN, never a
            // hyphen; the source line still carries whatever it carries, so
            // the sign column is drawn and the body is the bare code — the
            // unified-diff marker is consumed here, never redrawn by the
            // code cell. The prototype's cells are flex items, so their
            // leading indent collapses away too and every row's code starts
            // on the same column.
            let (number, sign, sign_color, code_color, wash) = match line.chars().next() {
                Some('+') => {
                    let n = new;
                    new += 1;
                    (n, "+", RUNNING, TEXT_2, Some(RUNNING_WASH))
                }
                Some('-') => {
                    let n = old;
                    old += 1;
                    (n, "\u{2212}", BLOCKED, TEXT_2, Some(BLOCKED_WASH))
                }
                _ => {
                    let n = new;
                    old += 1;
                    new += 1;
                    (n, "", TEXT_MUTED, TEXT_MUTED, None)
                }
            };
            let body = match line.chars().next() {
                Some('+') | Some('-') => line[1..].trim_start(),
                _ => line.trim_start(),
            }
            .to_string();
            let mut row = div()
                .flex()
                .gap(px(theme::DIFF_GAP))
                .px(px(theme::HUNK_PAD_X));
            if let Some(wash) = wash {
                row = row.bg(rgba(wash));
            }
            lines = lines.child(
                row.child(tabular(
                    div()
                        .flex_shrink_0()
                        .w(px(theme::DIFF_NUM_W))
                        .text_right()
                        .text_color(rgb(SEP))
                        .child(SharedString::from(number.to_string())),
                ))
                .child(
                    div()
                        .flex_shrink_0()
                        .w(px(theme::DIFF_SIGN_W))
                        .text_color(rgb(sign_color))
                        .child(sign),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(code_color))
                        .child(selection.line(block, body, Vec::new())),
                ),
            );
        }
    }
    lines
}

/// Markdown spans flattened to one wrapping run — its text and highlight
/// runs, for the selection overlay to wash and register (#27) — so inline
/// code keeps its place in the sentence instead of becoming its own box.
/// Bold and links carry their own styles (#22 C13); links stay inert —
/// paths render, nothing opens.
fn inline(spans: &[Span]) -> (String, Vec<(std::ops::Range<usize>, HighlightStyle)>) {
    let mut text = String::new();
    let mut highlights = Vec::new();
    for span in spans {
        let start = text.len();
        text.push_str(&span.text);
        if let Some(style) = span_style(span.style) {
            highlights.push((start..text.len(), style));
        }
    }
    (text, highlights)
}

/// One span's highlight, or none where the run wears the block's own ink.
fn span_style(style: Style) -> Option<HighlightStyle> {
    match style {
        Style::Plain => None,
        // Inline `code` (§E.4): a `--raised` chip, colour **inherited** —
        // the prototype does not promote it to strong ink. A gpui highlight
        // carries no padding or radius, so the chip's `1px 4px` inset and
        // 3px corners come from `prose` wrapping it in an element instead;
        // this background is the fallback for the runs that stay flat.
        Style::Code => Some(HighlightStyle {
            background_color: Some(rgb(RAISED).into()),
            ..Default::default()
        }),
        // `strong` (§E.5): weight 600 in `--text-strong`.
        Style::Bold => Some(HighlightStyle {
            color: Some(rgb(TEXT_STRONG).into()),
            font_weight: Some(FontWeight::SEMIBOLD),
            ..Default::default()
        }),
        // `a` (§E.6): `--text`, underlined 1px in `--sep`. Inert — paths
        // render, nothing opens.
        Style::Link => Some(HighlightStyle {
            color: Some(rgb(TEXT).into()),
            underline: Some(gpui::UnderlineStyle {
                thickness: px(1.),
                color: Some(rgb(SEP).into()),
                wavy: false,
            }),
            ..Default::default()
        }),
    }
}

/// A prose Block's text (§E.1/E.2/E.3): one wrapping run, so a sentence
/// breaks where the prototype's does.
///
/// Inline `code` is the one span that cannot live inside that run — §E.4
/// gives it `padding: 1px 4px` and a 3px radius, and a gpui highlight has
/// neither. A Block that carries one is composed of flex pieces instead,
/// with the chip as its own padded element; a Block that does not — nearly
/// every one — keeps the single run untouched.
fn prose(block: BlockId, spans: &[Span], selection: &SelectionOverlay) -> AnyElement {
    if !spans.iter().any(|span| span.style == Style::Code) {
        let (text, highlights) = inline(spans);
        return selection.line(block, text, highlights).into_any_element();
    }
    let mut row = div().flex().flex_wrap().items_baseline();
    for (index, span) in spans.iter().enumerate() {
        let text = SharedString::from(span.text.clone());
        let run = if index == 0 {
            selection.line(block, text, Vec::new())
        } else {
            selection.piece(block, text, Vec::new())
        };
        row = match span.style {
            Style::Code => row.child(
                div()
                    .flex_shrink_0()
                    // An inline background box is the font's content area
                    // tall, not the line box — 14px on this 12px face, so
                    // the chip measures the prototype's 16 (§E.4) rather
                    // than the 18.6px reading line it sits on.
                    .line_height(px(14.))
                    .px(px(theme::INLINE_CODE_PAD_X))
                    .py(px(theme::INLINE_CODE_PAD_Y))
                    .rounded(px(theme::R_TIGHT))
                    .bg(rgb(RAISED))
                    .child(run),
            ),
            style => row.child(div().min_w_0().child(match span_style(style) {
                Some(highlight) => run.with_highlights(vec![(0..span.text.len(), highlight)]),
                None => run,
            })),
        };
    }
    row.into_any_element()
}

/// A fenced block's rows, one element per hard line, each carrying that
/// line's slice of the block's highlight runs.
///
/// The indent is drawn as width, not as glyphs. One element per line is not
/// enough on its own: gpui shapes a run of leading U+0020 to zero advance —
/// and U+00A0 in its place shapes to zero too — so every inner line landed
/// flush left however the string was cut. So the leading spaces go into
/// their own box, sized from the mono advance (JetBrains Mono is 600/1000
/// em, the 0.6 below), and the code follows in a second run. The spaces are
/// still emitted as a text fragment inside that box, so a copy takes the
/// line back whole; only its painting is guaranteed by the width.
fn code_lines(
    block: BlockId,
    source: &str,
    highlights: Vec<(std::ops::Range<usize>, HighlightStyle)>,
    selection: &SelectionOverlay,
) -> Vec<Div> {
    /// JetBrains Mono's advance as a fraction of the type size.
    const MONO_ADVANCE: f32 = 0.6;
    let mut rows = Vec::new();
    let mut at = 0usize;
    for line in source.split('\n') {
        let span = at..at + line.len();
        at = span.end + 1;
        let indent = line.len() - line.trim_start_matches(' ').len();
        let runs = |from: usize| -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
            highlights
                .iter()
                .filter_map(|(range, style)| {
                    let start = range.start.max(span.start + from);
                    let end = range.end.min(span.end);
                    (start < end)
                        .then(|| (start - span.start - from..end - span.start - from, *style))
                })
                .collect()
        };
        if indent == 0 {
            rows.push(div().child(selection.line(block, line.to_string(), runs(0))));
            continue;
        }
        rows.push(
            div()
                .flex()
                .child(
                    div()
                        .flex_shrink_0()
                        .w(px((indent as f32 * theme::FS_MONO * MONO_ADVANCE).round()))
                        .child(selection.line(block, line[..indent].to_string(), Vec::new())),
                )
                .child(selection.piece(block, line[indent..].to_string(), runs(indent))),
        );
    }
    rows
}

/// Syntax highlight runs for a code Block, or none while the highlighter is
/// still thinking.
fn code(source: &str, tokens: Option<&[Token]>) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let Some(tokens) = tokens else {
        return Vec::new();
    };
    let mut highlights = Vec::new();
    let mut at = 0;
    for token in tokens {
        let end = at + token.text.len();
        // A highlighter that disagrees with the source is ignored, not trusted
        // into a panic.
        if end > source.len() {
            return Vec::new();
        }
        // The prototype's code blocks have exactly one syntax class,
        // `.comment` (§E.7). Everything else is the body's own `--text-2`
        // (R-08 — this is a deliberate loss of colour, not an oversight).
        let color = match token.class {
            Class::Comment => TEXT_MUTED,
            _ => TEXT_2,
        };
        highlights.push((
            at..end,
            HighlightStyle {
                color: Some(rgb(color).into()),
                ..Default::default()
            },
        ));
        at = end;
    }
    highlights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_advertises_history_only_when_the_context_is_armed() {
        assert_eq!(
            composer_hints(false, true),
            "↑ history · @ files · / commands"
        );
        assert_eq!(composer_hints(false, false), "@ files · / commands");
        assert_eq!(
            composer_hints(true, true),
            "@ project files · /import",
            "drafts never advertise Thread history"
        );
    }
    use ferrite_core::transcript::{Input, Lexer, Todos};
    use ferrite_core::{Hunk, SessionEvent, ToolResult, TurnOutcome};
    use gpui::{size, TestAppContext};
    use std::sync::Arc;

    /// A transcript holding one of every Block kind the Pane can draw.
    fn every_kind() -> Transcript {
        let (lexer, answers) = Lexer::new();
        let mut transcript = Transcript::new(Arc::new(lexer));
        transcript.apply(Input::Prompt("run the tests".into()));
        transcript.apply(Input::Event(SessionEvent::ThinkingDelta {
            text: "weighing it up".into(),
        }));
        transcript.apply(Input::Event(SessionEvent::TextDelta {
            text: "## Plan\nI will run `cargo test` first.\n- one\n- two\n\n\
                   ```rust\nfn main() {}\n```\ndone.\n\n"
                .into(),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "toolu_1".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo test" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "toolu_1".into(),
            output: "42 passed".into(),
            is_error: false,
            result: ToolResult::Opaque,
        }));
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "toolu_2".into(),
            name: "Edit".into(),
            input: serde_json::json!({ "file_path": "/workspace/x.txt" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "toolu_2".into(),
            output: "applied".into(),
            is_error: true,
            result: ToolResult::FileEdit {
                path: "/workspace/x.txt".into(),
                hunks: vec![Hunk {
                    old_start: 1,
                    old_lines: 3,
                    new_start: 1,
                    new_lines: 3,
                    lines: vec![" alpha".into(), "-bravo".into(), "+delta".into()],
                }],
            },
        }));
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "toolu_3".into(),
            name: "Read".into(),
            input: serde_json::json!({ "file_path": "/workspace/missing" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "toolu_3".into(),
            output: "No such file or directory".into(),
            is_error: true,
            result: ToolResult::Opaque,
        }));
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(0.038),
        }));
        transcript.apply(Input::Notice("send failed: broken pipe".into()));
        transcript.apply(Input::Revived);
        for answer in answers.try_iter() {
            transcript.apply(answer);
        }
        transcript
    }

    /// Renders Blocks through a real view: hover styles look up the view
    /// they are painting under, which a bare `cx.draw` does not have. Owns
    /// the selection whose overlay every run routes through (#27), so tests
    /// can read what registered and aim carets at it.
    struct ShowsBlocks {
        thread: ThreadId,
        selection: crate::select::TranscriptSelection,
        blocks: Vec<Block>,
        expanded: HashSet<String>,
    }

    impl Render for ShowsBlocks {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut Context<Self>,
        ) -> impl IntoElement {
            let overlay = self.selection.overlay(self.thread, &self.blocks);
            div()
                .flex()
                .flex_col()
                .w(px(900.))
                .font_family(crate::theme::FONT_MONO)
                .text_size(px(12.))
                .children(self.blocks.iter().map(|block| {
                    let expanded = matches!(
                        &block.body,
                        Body::Tool(tool) if self.expanded.contains(&tool.call)
                    );
                    render_block(
                        block,
                        &overlay,
                        None,
                        expanded,
                        None,
                        TEXT_MUTED,
                        Flow::default(),
                    )
                }))
        }
    }

    fn shows_blocks(blocks: Vec<Block>) -> ShowsBlocks {
        ShowsBlocks {
            thread: ThreadId::new(1),
            selection: crate::select::TranscriptSelection::default(),
            blocks,
            expanded: HashSet::new(),
        }
    }

    struct ShowsDecisions {
        decisions: Vec<Decision>,
    }

    impl Render for ShowsDecisions {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut Context<Self>,
        ) -> impl IntoElement {
            div()
                .flex()
                .flex_col()
                .w(px(900.))
                .font_family(crate::theme::FONT_MONO)
                .text_size(px(12.))
                .children(self.decisions.iter().map(|decision| {
                    decision_card(
                        decision,
                        Some(
                            decide_row(Level::Transcript)
                                .child(keycap_allow())
                                .child(keycap_deny())
                                .child(keycap_always())
                                .into_any_element(),
                        ),
                    )
                }))
                .children(self.decisions.iter().map(|decision| {
                    l2_decision_body(
                        decision,
                        Some(
                            decide_row(Level::Instruments)
                                .child(keycap_allow())
                                .child(keycap_deny())
                                .into_any_element(),
                        ),
                    )
                }))
        }
    }

    /// An operator running many Threads has to know which of them share the
    /// checkout and which cannot trample it.
    #[test]
    fn the_chrome_names_the_workspace_a_thread_works_in() {
        assert_eq!(
            binding_label(Some(&WorkspaceBinding::Worktree {
                repo: "/repo".into(),
                path: "/repo/../ferrite-thread-3".into(),
            })),
            "ferrite-thread-3"
        );
        assert_eq!(
            binding_label(Some(&WorkspaceBinding::Main {
                checkout: "/repo".into()
            })),
            "main"
        );
        // A Thread from before bindings existed claims nothing.
        assert_eq!(binding_label(None), "");
    }

    /// #25: the picker shows the bare model name — the provider is the
    /// logomark beside it, so the label never repeats it and never carries
    /// a `·` seam. An id with no known provider prefix stands verbatim
    /// rather than being guessed apart.
    #[test]
    fn the_model_label_strips_the_provider_the_logomark_already_names() {
        assert_eq!(model_label("claude-sonnet-4-5").as_ref(), "Sonnet 4.5");
        assert_eq!(model_label("codex-gpt-5.4-mini").as_ref(), "GPT-5.4 Mini");
        assert_eq!(model_label("gpt-5.6").as_ref(), "GPT-5.6");
        assert_eq!(model_label("claude-fable-5-1").as_ref(), "Fable 5.1");
    }

    /// #26, the Row rule from the pointer's side: rows that answer clicks
    /// advertise it with the cursor — the selected row keeps it while
    /// skipping the wash — and an inert row promises nothing, for the same
    /// reason it draws no ↵ hint. Keycaps are Controls and say so too.
    #[test]
    fn rows_advertise_their_click_with_the_cursor_and_inert_rows_do_not() {
        use gpui::CursorStyle;
        fn cursor(mut drawn: impl Styled) -> Option<CursorStyle> {
            drawn.style().mouse_cursor
        }
        let offer = MenuRow {
            insert: "/import".into(),
            name: "/import".into(),
            matched: vec![],
            detail: "adopt a CLI session".into(),
            prose_detail: true,
            inert: false,
        };
        assert_eq!(
            cursor(menu_row(&offer, false)),
            Some(CursorStyle::PointingHand)
        );
        assert_eq!(
            cursor(menu_row(&offer, true)),
            Some(CursorStyle::PointingHand),
            "the selected row skips the wash, never the cursor"
        );
        let inert = MenuRow {
            inert: true,
            ..offer
        };
        assert_eq!(cursor(menu_row(&inert, false)), None);
        assert_eq!(cursor(menu_row(&inert, true)), None);

        // The ✓-row both selectors share follows the same rule.
        assert_eq!(
            cursor(picker_row(
                "workspace root".into(),
                "".into(),
                false,
                false,
                false
            )),
            Some(CursorStyle::PointingHand)
        );
        assert_eq!(
            cursor(picker_row(
                "workspace root".into(),
                "".into(),
                true,
                true,
                false
            )),
            Some(CursorStyle::PointingHand)
        );

        // The decide keycaps answer the mouse (#26) and say so.
        assert_eq!(cursor(keycap_allow()), Some(CursorStyle::PointingHand));
        assert_eq!(cursor(keycap_deny()), Some(CursorStyle::PointingHand));
        assert_eq!(cursor(keycap_always()), Some(CursorStyle::PointingHand));
    }

    /// The app is thin by design, so its render test is that every Block kind
    /// the core can produce actually lays out and paints in a window.
    #[gpui::test]
    fn every_block_kind_paints(cx: &mut TestAppContext) {
        let transcript = every_kind();
        let failed_edit = transcript
            .blocks()
            .iter()
            .find_map(|block| match &block.body {
                Body::Tool(tool) if tool.call == "toolu_2" => Some(tool),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            tool_verdicts(failed_edit),
            vec![ToolVerdict::Diff(1, 1), ToolVerdict::Failed]
        );
        let blocks: Vec<Block> = transcript.blocks().to_vec();

        let kinds: Vec<&str> = blocks
            .iter()
            .map(|block| match &block.body {
                Body::Prompt(_) => "prompt",
                Body::Paragraph { .. } => "paragraph",
                Body::Heading { .. } => "heading",
                Body::Bullet { .. } => "bullet",
                Body::Code { .. } => "code",
                Body::Tool(tool) => match (&tool.state, &tool.diff) {
                    (_, Some(_)) => "diff",
                    (ToolState::Failed(_), _) => "tool-failed",
                    _ => "tool",
                },
                Body::Thinking(_) => "thinking",
                Body::Notice(_) => "notice",
                Body::Meta(_) => "meta",
            })
            .collect();
        for wanted in [
            "prompt",
            "paragraph",
            "heading",
            "bullet",
            "code",
            "tool",
            "diff",
            "tool-failed",
            "thinking",
            "notice",
            "meta",
        ] {
            assert!(kinds.contains(&wanted), "no {wanted} block in {kinds:?}");
        }

        let thread = ThreadId::new(1);
        let (view, cx) = cx.add_window_view(|_, _| shows_blocks(blocks));
        // A resize forces a real layout-and-paint pass through the view.
        cx.simulate_resize(size(px(900.), px(600.)));
        cx.run_until_parked();

        // And once more with everything selected, so the SELECTION wash
        // paints on every kind (#27): anchor on the first registered
        // character, head past the last.
        view.update(cx, |view, cx| {
            let runs = view.selection.registered(thread);
            let (first, first_ordinal, _, _) = runs.first().expect("registered runs").clone();
            let (last, last_ordinal, _, text) = runs.last().expect("registered runs").clone();
            let from = view
                .selection
                .caret_position(thread, first, first_ordinal, 0)
                .expect("a caret on the first run");
            let mut to = view
                .selection
                .caret_position(thread, last, last_ordinal, text.len())
                .expect("a caret on the last run");
            // Past the right edge: the nearest-index clamp takes the rest.
            to.x += px(40.);
            let everywhere = gpui::Bounds::new(point(px(0.), px(0.)), size(px(900.), px(600.)));
            view.selection.begin(thread, from, 1, everywhere);
            assert!(
                view.selection.extend(thread, to, everywhere),
                "the sweep must take"
            );
            cx.notify();
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                view.selection.copied_text().is_some(),
                "the sweep holds text across every kind"
            );
        });
    }

    /// AC2's copy half, relocated from `block_text` (#27): every Block kind
    /// must register its text with the selection overlay when it renders —
    /// a kind that registers nothing would select and copy as a silent
    /// hole. Chrome — gutter glyphs, bullets, verdict chips, the diff
    /// number column — never registers, so it can never be copied.
    #[gpui::test]
    fn every_block_kind_registers_its_selectable_text(cx: &mut TestAppContext) {
        let transcript = every_kind();
        let instruments = Instruments::of(&transcript);
        let blocks: Vec<Block> = transcript.blocks().to_vec();
        let ids: Vec<ferrite_core::transcript::BlockId> =
            blocks.iter().map(|block| block.id).collect();
        let thread = ThreadId::new(1);
        let (view, cx) = cx.add_window_view(|_, _| shows_blocks(blocks));
        cx.simulate_resize(size(px(900.), px(600.)));
        cx.run_until_parked();

        let runs = view.read_with(cx, |view, _| view.selection.registered(thread));
        for id in &ids {
            assert!(
                runs.iter()
                    .any(|(block, _, _, text)| block == id && !text.trim().is_empty()),
                "no selectable text registered for Block {id:?}"
            );
        }
        // What a whole-transcript copy would assemble: pieces of one visual
        // row join with nothing, rows join with newlines.
        let mut all = String::new();
        for (_, _, starts_line, text) in &runs {
            if *starts_line && !all.is_empty() {
                all.push('\n');
            }
            all.push_str(text);
        }
        assert!(all.contains("run the tests"), "the prompt line: {all}");
        assert!(
            all.contains("fn main() {}"),
            "code registers its source: {all}"
        );
        assert!(
            all.contains("Bash(cargo test)"),
            "tool pieces compose the call: {all}"
        );
        // A hunk registers its code, never its sign column: the `+`/`−` is
        // chrome the prototype draws beside the cell, not text inside it.
        assert!(
            all.contains("delta") && all.contains("bravo"),
            "a diff registers its lines: {all}"
        );
        assert!(
            !all.contains("+delta") && !all.contains("-bravo"),
            "the sign column is chrome and never copies: {all}"
        );
        // The result line registers where it renders (Edit's); Bash's was
        // promoted into its chip, which is chrome — so its count never
        // registers. The `└` elbow is chrome and never joins the run.
        assert!(all.contains("applied"), "the result line: {all}");
        assert!(!all.contains("└"), "the elbow is chrome: {all}");

        view.update(cx, |view, cx| {
            view.expanded.insert("toolu_2".into());
            cx.notify();
        });
        cx.run_until_parked();
        let expanded = view.read_with(cx, |view, _| view.selection.registered(thread));
        assert_eq!(
            expanded
                .iter()
                .filter(|(_, _, _, text)| text == "delta" || text == "bravo")
                .count(),
            2,
            "the edit diff still renders exactly once expanded"
        );
        assert!(expanded.iter().any(|(_, _, _, text)| text == "applied"));
        assert_eq!(instruments.changed.len(), 1);
        assert_eq!((instruments.added, instruments.removed), (1, 1));
        assert!(
            !all.contains("42 passed"),
            "a promoted chip is chrome: {all}"
        );
        // The prototype's body draws two glyphs and one elbow, all chrome;
        // the old ❯/⏺/• gutter glyphs are gone entirely.
        for chrome in ['❯', '⏺', '•', '✓', '▸', '●', '└'] {
            assert!(!all.contains(chrome), "{chrome} is chrome: {all}");
        }
        assert!(
            runs.iter().any(|(_, _, _, text)| text == "delta"),
            "a diff cell is its bare code — no number, no sign: {runs:?}"
        );
    }

    #[gpui::test]
    fn a_blocked_thread_paints_its_decision_card(cx: &mut TestAppContext) {
        let event = crate::demo::script()
            .into_iter()
            .map(|step| step.event)
            .find(|event| matches!(event, SessionEvent::DecisionRequested { .. }))
            .expect("the demo stops on a Decision");
        let SessionEvent::DecisionRequested { decision } = event else {
            unreachable!()
        };
        assert_eq!(decision.tool_name, "Write");

        // A request Ferrite could not read is still a card, or the operator
        // has nothing to deny and the turn hangs.
        let unreadable = Decision {
            tool_name: String::new(),
            description: String::new(),
            ..decision.clone()
        };

        let (_, cx) = cx.add_window_view(|_, _| ShowsDecisions {
            decisions: vec![decision, unreadable],
        });
        cx.simulate_resize(size(px(900.), px(300.)));
        cx.run_until_parked();
    }

    /// glance.md §4's wall matrix, one assertion per row — the selection
    /// logic the wall cell renders from.
    #[test]
    fn the_wall_state_matrix_reads_exactly_as_the_glance_spec() {
        use WallState::*;
        // Working, focused or not, is the streaming Thread.
        assert_eq!(
            wall_state(Some(Status::Streaming), false, false, false),
            Working
        );
        // Failing tests stay a working Thread — red text, not a ring.
        assert_eq!(
            wall_state(Some(Status::Streaming), true, false, false),
            Decision
        );
        assert_eq!(
            wall_state(Some(Status::Streaming), false, true, false),
            Failing
        );
        // A Decision waits: pending flag or Blocked status, either way.
        assert_eq!(
            wall_state(Some(Status::Blocked), false, false, false),
            Decision
        );
        // A closed Session is the red hard-blocker.
        assert_eq!(
            wall_state(Some(Status::Closed), false, false, true),
            Blocked
        );
        // Idle with a recorded turn cost is done; without one, idle.
        assert_eq!(wall_state(Some(Status::Idle), false, false, true), Done);
        assert_eq!(wall_state(Some(Status::Idle), false, false, false), Idle);
        // No transcript at all — the cockpit could not open the Thread.
        assert_eq!(wall_state(None, false, false, false), Parked);
    }

    /// The wall card folds everything the L3 recipe needs that is not an
    /// O(1) read — built on change, never per frame.
    #[test]
    fn the_wall_card_folds_progress_result_and_context_lines() {
        // No transcript: an empty card.
        let empty = wall_card(None, None);
        assert!(!empty.tests_failing);
        assert!(empty.meter.is_empty());

        let mut transcript = Transcript::default();
        for subject in ["a", "b", "c", "d"] {
            transcript.apply(Input::Event(SessionEvent::ToolStarted {
                id: format!("t{subject}"),
                name: "TaskCreate".into(),
                input: serde_json::json!({ "subject": subject }),
            }));
        }
        for task in ["1", "2", "3"] {
            transcript.apply(Input::Event(SessionEvent::ToolStarted {
                id: format!("u{task}"),
                name: "TaskUpdate".into(),
                input: serde_json::json!({ "taskId": task, "status": "completed" }),
            }));
        }
        assert_eq!(transcript.todos(), Some(Todos { done: 3, total: 4 }));
        let card = wall_card(Some(&transcript), None);
        assert_eq!(card.meter.as_ref(), "▰▰▰▱");
        assert_eq!(card.working.as_ref(), "3/4 · ◐ working");

        // A red suite flips the folded flag and folds the failing line —
        // with the run's own count when its output reported one.
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "test1".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo test" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "test1".into(),
            output: "test result: FAILED. 357 passed; 2 failed".into(),
            is_error: true,
            result: ToolResult::Opaque,
        }));
        let red = wall_card(Some(&transcript), None);
        assert!(red.tests_failing);
        assert_eq!(red.failing.as_ref(), "✗ 2 failing");

        // A Decision's subject becomes the alert's second line, wearing the
        // tool prefix every Decision surface shares (#22 C7).
        let decision = Decision {
            id: "perm".into(),
            tool_use_id: "toolu".into(),
            tool_name: "Bash".into(),
            description: "gh issue close 212".into(),
            input: serde_json::Value::Null,
            suggestions: vec![],
        };
        assert_eq!(
            wall_card(Some(&transcript), Some(&decision))
                .context
                .as_ref(),
            "Bash: gh issue close 212"
        );

        // A closed Session's reason is promoted into the alert line itself
        // (#22 C14).
        let mut closed = Transcript::default();
        closed.apply(Input::Event(SessionEvent::Closed {
            reason: "claude CLI exited with code 1".into(),
        }));
        assert_eq!(
            wall_card(Some(&closed), None).context.as_ref(),
            "✗ claude CLI exited with code 1"
        );
    }

    /// One derivation for every surface that names a Decision (L1 card, L2
    /// cell, wall alert) — and the unreadable-request fallback holds even
    /// when the provider names no tool at all.
    #[test]
    fn every_decision_surface_shares_one_subject_derivation() {
        let decision = |tool: &str, description: &str| Decision {
            id: "perm".into(),
            tool_use_id: "toolu".into(),
            tool_name: tool.into(),
            description: description.into(),
            input: serde_json::Value::Null,
            suggestions: vec![],
        };
        let full = decision("Bash", "gh issue close 212");
        assert_eq!(decision_subject(&full).as_ref(), "Bash: gh issue close 212");
        assert_eq!(decision_wants(&full).as_ref(), "Bash · wants approval");
        // A request naming its cwd carries it on the wants line (#22 C7).
        let mut placed = decision("Bash", "gh issue close 212");
        placed.input = serde_json::json!({ "command": "gh issue close 212", "cwd": "/work/api" });
        assert_eq!(
            decision_wants(&placed).as_ref(),
            "Bash · wants approval · /work/api"
        );
        // No description: the tool's name is the subject.
        let bare = decision("Write", "");
        assert_eq!(decision_subject(&bare).as_ref(), "Write");
        // No tool at all: the honest fallback, on both lines.
        let unreadable = decision("", "");
        assert_eq!(
            decision_subject(&unreadable).as_ref(),
            "unreadable permission request"
        );
        assert_eq!(
            decision_wants(&unreadable).as_ref(),
            "the provider sent a request Ferrite could not read"
        );
        // The wall's alert context runs through the same derivation.
        let transcript = Transcript::default();
        assert_eq!(
            wall_card(Some(&transcript), Some(&unreadable))
                .context
                .as_ref(),
            "unreadable permission request"
        );
    }

    /// #23: the mode chip speaks the prototype's name for acceptEdits and
    /// the provider's own word for everything else — never an invented
    /// label, and no `⏵` prefix: the pencil icon is the whole mark.
    #[test]
    fn the_mode_chip_labels_accept_edits_the_prototypes_way_and_the_rest_verbatim() {
        assert_eq!(mode_chip_label("acceptEdits").as_ref(), "auto-edit");
        assert_eq!(
            mode_chip_label("bypassPermissions").as_ref(),
            "bypassPermissions"
        );
        assert_eq!(mode_chip_label("plan").as_ref(), "plan");
        assert_eq!(mode_chip_label("default").as_ref(), "default");
    }

    /// #22 amendment: durations read in the comps' grammar at every scale.
    #[test]
    fn durations_read_at_the_comps_grammar() {
        assert_eq!(duration_label(Duration::from_millis(340)).as_ref(), "0.3s");
        assert_eq!(
            duration_label(Duration::from_millis(8_200)).as_ref(),
            "8.2s"
        );
        assert_eq!(duration_label(Duration::from_secs(42)).as_ref(), "42s");
        assert_eq!(duration_label(Duration::from_secs(134)).as_ref(), "2m14s");
    }

    /// The instrument levels' ▰▱ meter stays glanceable: glyphs for small
    /// plans, the bare fraction for long ones, and done never overshoots.
    /// L1 draws real segments instead; this run is L2/L3's alone.
    #[test]
    fn the_todo_meter_caps_its_glyph_run() {
        assert_eq!(meter(3, 4).as_ref(), "▰▰▰▱ 3/4");
        assert_eq!(meter(0, 2).as_ref(), "▱▱ 0/2");
        assert_eq!(meter(9, 20).as_ref(), "9/20");
        assert_eq!(meter(5, 4).as_ref(), "▰▰▰▰ 4/4");
        assert_eq!(meter(0, 0).as_ref(), "");
        // The wall's bare run: glyphs only, and nothing past the cap — its
        // status line already carries the fraction.
        assert_eq!(meter_run(3, 4), "▰▰▰▱");
        assert_eq!(meter_run(9, 20), "");
        assert_eq!(meter_run(0, 0), "");
    }

    /// #11: import is offered exactly while a Thread has no conversation —
    /// at rest, with nothing in its transcript but Ferrite's own notices
    /// and bookkeeping. The first prompt retires the offer; a refused pick
    /// (a Notice) does not.
    #[test]
    fn import_is_offered_only_while_the_thread_has_no_conversation() {
        assert!(!offers_import(None), "a parked Pane offers nothing");

        let mut fresh = Transcript::default();
        assert!(offers_import(Some(&fresh)));
        fresh.apply(Input::Notice("cannot import x: not a session file".into()));
        fresh.apply(Input::Revived);
        assert!(
            offers_import(Some(&fresh)),
            "Ferrite's own out-of-band lines keep the door open"
        );
        fresh.apply(Input::Prompt("hello".into()));
        assert!(
            !offers_import(Some(&fresh)),
            "the first prompt is a conversation"
        );

        let mut streaming = Transcript::default();
        streaming.apply(Input::Event(SessionEvent::TextDelta { text: "x".into() }));
        assert!(!offers_import(Some(&streaming)), "not at rest");
    }

    /// §D.7: the idle line says what the Pane is waiting on — a Decision, a
    /// live Thread, or a closed Session — and nothing else. It never names
    /// the Thread and never repeats the hints beside it.
    #[test]
    fn the_placeholder_says_what_the_pane_is_waiting_on() {
        let live = Transcript::default();
        assert_eq!(placeholder(false, Some(&live)), "Steer this Thread\u{2026}");
        assert_eq!(
            placeholder(true, Some(&live)),
            "Reply to the Decision\u{2026}"
        );

        let mut closed = Transcript::default();
        closed.apply(Input::Event(SessionEvent::Closed {
            reason: "the CLI exited".into(),
        }));
        assert_eq!(
            placeholder(false, Some(&closed)),
            "Revive and continue\u{2026}"
        );

        for line in [
            placeholder(false, Some(&live)),
            placeholder(true, Some(&live)),
            placeholder(false, Some(&closed)),
        ] {
            assert!(!line.contains("message"), "{line}");
            assert!(!line.contains("commands"), "{line}");
        }
    }
}
