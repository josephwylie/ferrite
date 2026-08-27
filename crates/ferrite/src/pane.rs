//! One Pane: the visible cell for one Thread. Header, transcript, Composer,
//! and the three semantic-zoom renderings. Rendering only — everything it
//! shows is folded in core, and every key it answers to belongs to the
//! cockpit above it.
//!
//! The three levels follow the canon boards: L1 per DirectionDense (dense
//! transcript, 28px merged header, PromptBox composer stack), L2 per the
//! Cockpit board (instrument cell), L3 per the Wall board (dot · slug ·
//! bar · status line, inset attention rings).

use ferrite_core::cockpit::{ProviderChoice, ToolTiming};
use ferrite_core::docview::{is_test_run, passed_count, Instruments, Level, Tests};
use ferrite_core::groups::GroupId;
use ferrite_core::transcript::{
    Block, BlockId, Body, Class, Diff, Span, Status, Style, Todos, Token, ToolBlock, ToolState,
    Transcript, Usage,
};
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::workspace::WorkspaceBinding;
use ferrite_core::{Decision, ThreadId};
use gpui::prelude::*;
use gpui::{
    canvas, deferred, div, point, px, relative, rgb, rgba, AnyElement, BoxShadow, Context, Div,
    Entity, FocusHandle, FontWeight, HighlightStyle, Pixels, ScrollHandle, SharedString, Stateful,
    StyledText,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;
#[cfg(test)]
use std::{cell::RefCell, rc::Rc};

use crate::composer::Composer;
use crate::pointer::{Pointer, PointerPressed};
use crate::select::SelectionOverlay;
// Every color and metric here is an Aperture token (crate::theme) — no
// literal survives in render code, which is #22's grep-able law.
use crate::theme;
use crate::theme::{
    ACCENT, ACCENT_WASH, CODE_KEYWORD, CODE_STR, EDGE, EDGE_STRONG, FAIL, FAIL_WASH, GOOD,
    GOOD_WASH, HAIRLINE, IDLE, INK, INK_FAINT, INK_MUTED, INK_SECONDARY, INK_TERTIARY, INSET,
    RAISED, SURFACE, WAIT, WAIT_EDGE, WAIT_WASH,
};

/// One Pane's view state: what the window owns per Pane. Everything it
/// shows lives in core; this is the keyboard, the scrollback position, and
/// the wall cell's cached strings.
pub struct PaneView {
    /// What the Pane holds (#29): a live Thread, or a draft still choosing
    /// its provider and CWD — no Thread, no Session, nothing durable.
    pub content: PaneContent,
    /// The Thread's slug name — `thread-NN` until display names exist
    /// (sidebar-and-impl §4.2 #8) — or the draft's placeholder title.
    /// Built once; the wall must not format names per frame.
    pub name: SharedString,
    pub composer: Entity<Composer>,
    pub scroll: ScrollHandle,
    /// A pending Decision takes the keyboard: y and n are answers, not text.
    pub decision_focus: FocusHandle,
    disclosure: ToolDisclosure,
    /// The wall cell's folded reading — everything the L3 recipe needs that
    /// is not an O(1) transcript read. The cockpit rebuilds it whenever the
    /// Thread's transcript changes; a frame never walks Blocks at L3.
    pub wall: WallCard,
}

struct ToolDisclosure {
    expanded: HashSet<String>,
    target: Option<String>,
    focus: FocusHandle,
    #[cfg(test)]
    bounds: Rc<RefCell<HashMap<String, gpui::Bounds<Pixels>>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisclosureState {
    Collapsed,
    Expanded,
}

/// A Thread, or the draft that becomes one at first send (#29).
pub enum PaneContent {
    Thread(ThreadId),
    Draft(DraftBinding),
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
    /// Group scope awaiting a real Thread id. Never persisted as a fake id.
    pub pending_group: Option<GroupId>,
    /// A pair member whose durable leave waits for this Draft to become the
    /// replacement. Closing the Draft applies the leave and dissolves.
    pub pending_leave: Option<ThreadId>,
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
            content: PaneContent::Thread(thread),
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
            wall: WallCard::default(),
        }
    }

    /// A draft Pane (#29): cmd-t's answer — a Composer and the pre-prompt
    /// band, and nothing else until the first send bootstraps a Thread.
    pub fn new_draft<T: 'static>(binding: DraftBinding, cx: &mut Context<T>) -> Self {
        Self {
            content: PaneContent::Draft(binding),
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
            wall: WallCard::default(),
        }
    }

    /// The Thread this Pane shows, or None while it is still a draft.
    pub fn thread(&self) -> Option<ThreadId> {
        match &self.content {
            PaneContent::Thread(thread) => Some(*thread),
            PaneContent::Draft(_) => None,
        }
    }

    pub fn draft(&self) -> Option<&DraftBinding> {
        match &self.content {
            PaneContent::Draft(binding) => Some(binding),
            PaneContent::Thread(_) => None,
        }
    }

    pub fn draft_mut(&mut self) -> Option<&mut DraftBinding> {
        match &mut self.content {
            PaneContent::Draft(binding) => Some(binding),
            PaneContent::Thread(_) => None,
        }
    }

    /// The lock's visible half (#29): the first send made a Thread of this
    /// draft, and the band disappears with the Pane's next frame.
    pub fn adopt_thread(&mut self, thread: ThreadId) {
        self.content = PaneContent::Thread(thread);
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
    pub(crate) fn tool_bounds(&self, call: &str) -> Option<gpui::Bounds<Pixels>> {
        self.disclosure.bounds.borrow().get(call).copied()
    }

    #[cfg(test)]
    pub(crate) fn tool_bounds_sink(&self) -> Rc<RefCell<HashMap<String, gpui::Bounds<Pixels>>>> {
        self.disclosure.bounds.clone()
    }
}

/// Everything one Pane draws, as the cockpit reads it for this frame.
pub struct PaneState<'a> {
    pub transcript: Option<&'a Transcript>,
    pub decision: Option<&'a Decision>,
    pub queued: Option<&'a str>,
    pub workspace: Option<&'a WorkspaceBinding>,
    /// The actual git checkout of the Thread's cwd (#29), cached by the
    /// cockpit and refreshed on turn end and the watchdog cadence — the L1
    /// header's binding slot. Display-only, never a control: post-lock the
    /// CWD is immutable, and nothing may look otherwise.
    pub branch: Option<SharedString>,
    /// The open `/` or `@` popover for this Pane's Composer, assembled in the
    /// cockpit exactly like `root_chip` — rows wired to their picks there —
    /// and hung above the input line here (#23). None when no menu is open.
    pub menu: Option<AnyElement>,
    /// Whether the Composer line is empty — what decides the idle
    /// placeholder, read where the cockpit has a `cx` to read it with.
    pub composer_empty: bool,
    pub history_available: bool,
    /// The Session's permission mode, in the provider's own word — the meta
    /// row's mode chip (#23). None (no announcement, or a provider that
    /// makes none) draws no chip; display-only either way.
    pub permission_mode: Option<&'a str>,
    /// The meta row's provider control, assembled in the cockpit exactly
    /// like `root_chip` — the click wired there (#25). Some only before
    /// the Thread's first prompt: after the lock the Pane draws today's
    /// plain muted model label instead.
    pub provider_chip: Option<AnyElement>,
    pub focused: bool,
    /// A turn in flight: the Composer's ❯ becomes ◐ and esc offers interrupt.
    pub running: bool,
    /// This frame's selection seam (#27): every text run the transcript
    /// draws goes through it — registered for hit-testing and copy, and
    /// washed where the selection covers it. The cockpit owns the drag;
    /// the Pane only routes its runs.
    pub selection: SelectionOverlay,
    /// The wall clocks of this Thread's tool calls (`Cockpit::tool_timings`)
    /// — what tool rows and activity lines read their durations from. None
    /// on a Pane whose cockpit kept no clock (tests, parked replays).
    pub timings: Option<&'a HashMap<String, ToolTiming>>,
    /// The context ring plus its hover card, assembled in the cockpit where
    /// the hover state lives — the Pane only places it (#22 C12). None when
    /// the provider reported no window, or below L1.
    pub usage_ring: Option<AnyElement>,
    /// The header's – / ✕ window controls, wired in the cockpit to the
    /// existing park and zoom-back verbs. None below L1.
    pub controls: Option<AnyElement>,
    /// The pending Decision's keycaps, wired in the cockpit to the exact
    /// decide verbs the keys run (#26) — assembled there like `controls`,
    /// laid into the L1 card or the L2 body here. None while nothing
    /// pends, and at the wall, which draws no keycaps.
    pub decide: Option<AnyElement>,
    /// L1 tool chevrons, already wired to the cockpit's shared toggle door.
    pub tool_controls: HashMap<String, AnyElement>,
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

/// Whether a Thread is holding the operator up — the rollup the strip counts
/// and the wall rings: a Decision waiting (amber) or a dead Session (red).
/// Failing tests are noise, not a ring, and are not counted (glance.md §3.1).
/// The strip and the nav both call this, so the two can never disagree.
pub fn needs_operator(pending: bool, status: Option<Status>) -> bool {
    pending || matches!(status, Some(Status::Blocked | Status::Closed))
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

/// One Pane. A Thread with no transcript is one the cockpit could not open;
/// it still gets a cell, because a Pane that vanishes hides the problem.
pub fn render_pane(view: &PaneView, state: PaneState<'_>, level: Level) -> impl IntoElement {
    let PaneState {
        transcript,
        decision,
        queued,
        workspace,
        branch,
        menu,
        composer_empty,
        history_available,
        permission_mode,
        provider_chip,
        focused,
        running,
        selection,
        timings,
        usage_ring,
        controls,
        decide,
        mut tool_controls,
    } = state;
    let status = transcript.map(|t| t.status());
    let state = wall_state(
        status,
        decision.is_some(),
        view.wall.tests_failing,
        transcript.and_then(|t| t.last_cost()).is_some(),
    );
    // Attention and focus are separate channels (#22 D16): the state ring
    // (a Decision's amber everywhere; the wall's red blocker — glance.md §4,
    // L2/L1 blocked renderings are undrawn so red stays at L3) and the
    // ACCENT focus ring coexist, the state ring nesting just inside so a
    // focused amber Pane shows both.
    let state_ring = if decision.is_some() {
        Some(WAIT)
    } else if level == Level::Wall && state == WallState::Blocked {
        Some(FAIL)
    } else {
        None
    };
    let focus_ring = focused.then_some(ACCENT);
    let (outer_ring, inner_ring) = match (focus_ring, state_ring) {
        (Some(focus), Some(state)) => (Some(ring_overlay(focus)), Some(inner_ring_overlay(state))),
        (Some(focus), None) => (Some(ring_overlay(focus)), None),
        (None, Some(state)) => (Some(ring_overlay(state)), None),
        (None, None) => (None, None),
    };
    let shell = div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .bg(rgb(SURFACE))
        .border_1()
        .border_color(rgba(EDGE))
        .overflow_hidden()
        // At the instrument levels the whole cell is a click-to-focus
        // button, and says so (#26): the Cell hover lifts the border it
        // already has — never a wash over the state canvas, and rings keep
        // color to themselves. At L1 the Pane is a workspace surface, not
        // a button, and its cursor stays unset for #27's I-beam.
        .when(level != Level::Transcript, |shell| shell.hover_cell());

    // Far enough away, a Pane is one signal: no header, no transcript,
    // nothing that stops reading at a glance.
    if level == Level::Wall {
        return shell
            .child(wall_cell(view, state, focused))
            .children(outer_ring)
            .children(inner_ring);
    }

    if level == Level::Instruments {
        return shell
            .child(l2_cell(
                view, transcript, decision, workspace, state, timings, decide,
            ))
            .children(outer_ring)
            .children(inner_ring);
    }

    let mut pane = shell.child(dense_header(
        view,
        transcript,
        branch.as_ref(),
        status,
        usage_ring,
        controls,
    ));
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
                    decide,
                    queued,
                    running,
                    empty: composer_empty,
                    history_available,
                    menu,
                    mode: permission_mode,
                    provider_chip,
                    band: None,
                },
            ));
        }
        None => {
            pane = pane.child(parked_body());
        }
    }
    pane.children(outer_ring).children(inner_ring)
}

/// The 1.5px inset attention ring — gpui has no inset box-shadow, so the
/// ring is an absolute full-size overlay quad that takes no events.
fn ring_overlay(color: u32) -> Div {
    div()
        .absolute()
        .inset_0()
        .border(px(theme::RING_W))
        .border_color(rgb(color))
}

/// The state ring nested just inside the focus ring, so both stay visible
/// when a focused Pane also demands attention (#22 D16).
fn inner_ring_overlay(color: u32) -> Div {
    div()
        .absolute()
        .inset(px(theme::RING_INSET))
        .border(px(theme::RING_W))
        .border_color(rgb(color))
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
    /// The header's – / ✕ controls, wired in the cockpit; ✕ discards the
    /// draft — there is nothing to park.
    pub controls: Option<AnyElement>,
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
        controls,
        error,
    } = state;
    let shell = div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .bg(rgb(SURFACE))
        .border_1()
        .border_color(rgba(EDGE))
        .overflow_hidden()
        .when(level != Level::Transcript, |shell| shell.hover_cell());
    let ring = focused.then(|| ring_overlay(ACCENT));

    if level != Level::Transcript {
        return shell
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .text_size(px(theme::TEXT_ROW))
                    .text_color(rgb(INK_MUTED))
                    .child("draft"),
            )
            .children(ring);
    }

    let mut wrapped = div().flex().flex_col().flex_shrink_0();
    wrapped = wrapped.child(band);
    if let Some(error) = error {
        wrapped = wrapped.child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .px(px(theme::COMPOSER_PAD_X))
                .pb(px(4.))
                .text_size(px(theme::TEXT_META))
                .text_color(rgb(FAIL))
                .child(div().min_w_0().whitespace_normal().child(error.clone())),
        );
    }
    shell
        .child(dense_header(view, None, None, None, None, controls))
        .child(div().flex().flex_1().min_h_0())
        .child(composer_region(
            view,
            None,
            ComposerStack {
                decision: None,
                decide: None,
                queued: None,
                running: false,
                empty: composer_empty,
                history_available: false,
                menu,
                mode: None,
                provider_chip: None,
                band: Some(wrapped.into_any_element()),
            },
        ))
        .children(ring)
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
        .px(px(theme::COMPOSER_PAD_X))
        .border_b_1()
        .border_color(rgba(HAIRLINE))
}

/// The band's key-path hint, riding the row's right edge.
pub fn band_hint() -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(theme::TEXT_CHIP))
        .text_color(rgb(INK_MUTED))
        .child("⇥ chips · ↵ send")
}

/// One band chip: the quiet EDGE box of the header chips, the provider's
/// keeping #25's accent tint as the mark of the provider's spot. Tab's
/// focus promotes the border to ACCENT — the popover opens on ↵, so the
/// chip must say where ↵ will land.
pub fn band_chip(slot: usize, label: SharedString, accent: bool, focused: bool) -> Stateful<Div> {
    let edge: gpui::Hsla = if focused {
        rgb(ACCENT).into()
    } else {
        rgba(EDGE).into()
    };
    div()
        .id(("band-chip", slot))
        .flex_shrink_0()
        .text_size(px(theme::TEXT_META))
        .text_color(rgb(if accent { ACCENT } else { INK_TERTIARY }))
        .when(accent, |chip| chip.bg(rgba(theme::ACCENT_WASH)))
        .border_1()
        .border_color(edge)
        .rounded(px(theme::R_CHIP))
        .px(px(6.))
        .py(px(1.))
        .hover_control()
        .press_control()
        .child(label)
}

/// A band chip's text: the choice plus the ⌵ that says it answers clicks.
pub fn band_chip_label(choice: &str) -> SharedString {
    SharedString::from(format!("{choice} ⌵"))
}

fn wall_cell(view: &PaneView, state: WallState, focused: bool) -> Div {
    let card = &view.wall;
    let (dot_color, hollow) = match state {
        WallState::Working | WallState::Failing | WallState::Done => (GOOD, false),
        WallState::Decision => (WAIT, false),
        WallState::Blocked => (FAIL, false),
        WallState::Idle => (IDLE, false),
        WallState::Parked => (INK_FAINT, true),
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
                        .text_size(px(theme::TEXT_CHIP))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(if focused { INK } else { INK_SECONDARY }))
                        .child(view.name.clone()),
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
                .text_size(px(theme::TEXT_WALL_STATUS))
                .text_color(rgb(ACCENT))
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
                theme::TEXT_WALL_STATUS,
                INK_MUTED,
            ));
        }
        WallState::Failing => {
            cell = cell.child(status_line(
                card.failing.clone(),
                theme::TEXT_WALL_STATUS,
                FAIL,
            ));
        }
        WallState::Decision => {
            cell = cell.child(status_line(
                SharedString::from("⚠ needs you"),
                theme::TEXT_CHIP,
                WAIT,
            ));
            if !card.context.is_empty() {
                cell = cell.child(status_line(
                    card.context.clone(),
                    theme::TEXT_WALL_STATUS,
                    INK_MUTED,
                ));
            }
        }
        WallState::Blocked => {
            // The close reason is the alert; the disposition is the
            // context (#22 C14).
            cell = cell.child(status_line(card.context.clone(), theme::TEXT_CHIP, FAIL));
            cell = cell.child(status_line(
                SharedString::from("blocked"),
                theme::TEXT_WALL_STATUS,
                INK_MUTED,
            ));
        }
        WallState::Done => {
            cell = cell
                .child(status_line(
                    SharedString::from("✓ done"),
                    theme::TEXT_WALL_STATUS,
                    GOOD,
                ))
                .opacity(theme::DONE_WALL_OPACITY);
        }
        WallState::Idle => {
            cell = cell.child(status_line(
                SharedString::from("❯ idle"),
                theme::TEXT_WALL_STATUS,
                INK_MUTED,
            ));
        }
        WallState::Parked => {
            cell = cell.child(status_line(
                SharedString::from("parked"),
                theme::TEXT_WALL_STATUS,
                INK_FAINT,
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
    state: WallState,
    timings: Option<&HashMap<String, ToolTiming>>,
    decide: Option<AnyElement>,
) -> Div {
    let hot = matches!(
        state,
        WallState::Working | WallState::Failing | WallState::Decision | WallState::Blocked
    );
    let led_color = match state {
        WallState::Decision => WAIT,
        WallState::Blocked | WallState::Failing => FAIL,
        WallState::Working | WallState::Done => GOOD,
        WallState::Idle => IDLE,
        WallState::Parked => INK_FAINT,
    };
    let mut header = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::CELL_HEADER_H))
        .gap(px(6.))
        .px(px(8.))
        .border_b_1()
        .border_color(rgba(HAIRLINE))
        .child(led(px(theme::LED), led_color))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_family(theme::FONT_UI)
                .text_size(px(theme::TEXT_TITLE))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(if hot { INK } else { INK_SECONDARY }))
                .child(view.name.clone()),
        )
        .child(div().flex_1());
    // The amber ring is the chip (#22 D17): a Decision cell's right meta
    // keeps the binding like every other cell.
    header = match state {
        WallState::Done => header.child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_CHIP_SM))
                .text_color(rgb(GOOD))
                .child("done"),
        ),
        // The comp's right-meta slot carries the Thread's id; the name is
        // already the id here, so the slot names the Workspace binding —
        // what an operator running many Threads actually needs.
        _ => header.child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_CHIP_SM))
                .text_color(rgb(INK_MUTED))
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

    if state == WallState::Idle {
        body = body.child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_size(px(theme::TEXT_META))
                .text_color(rgb(INK_MUTED))
                .child("❯ idle — waiting for work"),
        );
        return cell.child(header).child(body);
    }

    if state == WallState::Done {
        body = body.child(
            div()
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(INK_TERTIARY))
                .child("turn complete"),
        );
    }

    if let Some(todos) = read.todos {
        // Meter fill follows health: accent while green, secondary while
        // the suite is red (the Cockpit board's two data points). Glyphs,
        // not a bar fill — the ▰▱ run is the one meter language.
        let fill = if state == WallState::Failing {
            INK_SECONDARY
        } else {
            ACCENT
        };
        body = body.child(
            div()
                .w_full()
                .truncate()
                .text_size(px(theme::TEXT_META))
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
            badges = badges.child(chip(label, GOOD, GOOD_WASH));
            any_badge = true;
        }
        Some(Tests::Failed { count }) => {
            let label = match count {
                Some(count) => SharedString::from(format!("✗ {count} failing")),
                None => SharedString::from("✗ failing"),
            };
            badges = badges.child(chip(label, FAIL, FAIL_WASH));
            any_badge = true;
        }
        None => {}
    }
    if read.added > 0 || read.removed > 0 {
        badges = badges.child(
            diff_stat(read.added, read.removed)
                .text_size(px(theme::TEXT_CHIP))
                .bg(rgb(RAISED))
                .rounded(px(theme::R_CHIP))
                .px(px(6.))
                .py(px(1.)),
        );
        any_badge = true;
    }
    if any_badge {
        body = body.child(badges);
    }

    body = body.child(div().flex_1());
    if state == WallState::Done {
        body = body.child(
            div()
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(INK_MUTED))
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
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(INK_TERTIARY))
                .child(SharedString::from(clocked)),
        );
    }

    let mut content = cell.child(header).child(body);
    if state == WallState::Done {
        content = content.opacity(theme::DONE_CELL_OPACITY);
    }
    content
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
                .text_size(px(theme::TEXT_META))
                .text_color(rgb(INK))
                .child(command),
        )
        .child(
            div()
                .w_full()
                .truncate()
                .font_family(theme::FONT_UI)
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(INK_TERTIARY))
                .child(wants),
        )
        .children(decide)
}

// ---------------------------------------------------------------- L1 pane

/// DirectionDense's single 28px header: LED · name · binding · provider
/// chip · spacer · todo meter · context ring · window controls. The Main
/// board's todo strip and context indicator fold into this one line at
/// dense L1.
#[allow(clippy::too_many_arguments)]
fn dense_header(
    view: &PaneView,
    transcript: Option<&Transcript>,
    branch: Option<&SharedString>,
    status: Option<Status>,
    usage_ring: Option<AnyElement>,
    controls: Option<AnyElement>,
) -> Div {
    let led_color = match status {
        Some(Status::Streaming) => GOOD,
        Some(Status::Blocked) => WAIT,
        Some(Status::Closed) => FAIL,
        Some(Status::Idle) => IDLE,
        None => INK_FAINT,
    };
    let mut header = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::HEADER_DENSE_H))
        .gap(px(8.))
        .px(px(10.))
        .border_b_1()
        .border_color(rgba(HAIRLINE))
        .text_size(px(theme::TEXT_ROW))
        .child(led(px(theme::LED), led_color))
        .child(
            div()
                .flex_shrink_0()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(INK))
                .child(view.name.clone()),
        );
    // The binding slot (#29): the actual git checkout of the Thread's cwd
    // — a small branch glyph and the cached branch name. Pure text: no
    // hover, no click target, no re-aim anywhere — the CWD is immutable
    // once the chat runs, and the header must never look otherwise.
    if let Some(branch) = branch {
        header = header
            .child(div().flex_shrink_0().text_color(rgb(INK_FAINT)).child("·"))
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .items_center()
                    .gap(px(4.))
                    .child(branch_icon(INK_TERTIARY))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(INK_TERTIARY))
                            .child(branch.clone()),
                    ),
            );
    }
    // The provider chip — `claude · sonnet-4-5` on the accent wash, the
    // comps' one accent-tinted chip (#22 C12). The raw id no longer crams
    // the composer's bottom-right.
    if let Some(model) = transcript.and_then(|t| t.model()) {
        header = header.child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_META))
                .text_color(rgb(ACCENT))
                .bg(rgba(theme::ACCENT_WASH))
                .rounded(px(theme::R_CHIP))
                .px(px(6.))
                .py(px(1.))
                .child(model_chip_label(model)),
        );
    }
    header = header.child(div().flex_1());
    // The context ring, then the window controls at the far edge (#22
    // amendments) — no cost text and no context label anywhere. The tasks
    // meter lives on its own strip below, per the Main comp.
    if let Some(ring) = usage_ring {
        header = header.child(ring);
    }
    if let Some(controls) = controls {
        header = header.child(controls);
    }
    header
}

/// The header's branch glyph (#29): the bundled fonts carry no git-branch
/// codepoint, so the slot draws its own small two-dot fork — a rail with a
/// dot at each end and a third dot branched off the top, joined by a short
/// arm. Pure chrome: no id, no hover, no events, and it never registers
/// with the selection overlay.
fn branch_icon(color: u32) -> Div {
    let dot = |x: f32, y: f32| {
        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(3.))
            .h(px(3.))
            .rounded_full()
            .bg(rgb(color))
    };
    div()
        .relative()
        .flex_shrink_0()
        .w(px(11.))
        .h(px(11.))
        // The rail between the two left dots.
        .child(
            div()
                .absolute()
                .left(px(2.))
                .top(px(2.5))
                .w(px(1.))
                .h(px(6.))
                .bg(rgb(color)),
        )
        // The arm from the rail out to the branched dot.
        .child(
            div()
                .absolute()
                .left(px(2.5))
                .top(px(2.))
                .w(px(6.))
                .h(px(1.))
                .bg(rgb(color)),
        )
        .child(dot(1., 1.))
        .child(dot(1., 7.))
        .child(dot(7., 1.))
}

/// The tasks strip, the Main comp's own recipe: 28px on the sunken ground
/// under the header — `▰▰▰▱ 3/4` in accent, the step being worked in UI
/// prose, and the muted `todo` tag riding the right edge.
fn tasks_strip(todos: Todos, current: Option<&str>) -> Div {
    let mut strip = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(10.))
        .h(px(theme::TASKS_STRIP_H))
        .px(px(theme::COMPOSER_PAD_X))
        .bg(rgb(INSET))
        .border_b_1()
        .border_color(rgba(HAIRLINE))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_META))
                .text_color(rgb(ACCENT))
                .child(meter(todos.done, todos.total)),
        );
    if let Some(current) = current {
        strip = strip.child(
            div()
                .min_w_0()
                .truncate()
                .font_family(theme::FONT_UI)
                .text_size(px(theme::TEXT_ROW))
                .text_color(rgb(INK_SECONDARY))
                .child(SharedString::from(current.to_string())),
        );
    }
    strip.child(div().flex_1()).child(
        div()
            .flex_shrink_0()
            .text_size(px(theme::TEXT_CHIP))
            .text_color(rgb(INK_MUTED))
            .child("todo"),
    )
}

/// `▰▰▰▱ 3/4` while the glyph run stays glanceable; a long plan keeps the
/// fraction alone (an unbounded ▰ run would eat the header).
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

/// The provider chip's text: the model id groomed to the comps' grammar —
/// `claude-sonnet-4-5` → `claude · sonnet-4-5`. An id with no known
/// provider prefix stands verbatim rather than being guessed apart. Public
/// because the picker's model rows must spell a model exactly as the chip
/// does (#25) — one grooming, never two.
pub fn model_chip_label(model: &str) -> SharedString {
    for provider in ["claude", "codex"] {
        if let Some(rest) = model
            .strip_prefix(provider)
            .and_then(|rest| rest.strip_prefix('-'))
        {
            if !rest.is_empty() {
                return SharedString::from(format!("{provider} · {rest}"));
            }
        }
    }
    SharedString::from(model.to_string())
}

/// The pre-lock provider control's text (#25): the groomed model with the
/// ⌵ that says the label answers clicks — and the provider name alone
/// until the Session's Init announces what is actually serving, never an
/// invented model.
pub fn provider_chip_label(provider: &str, model: Option<&str>) -> SharedString {
    match model {
        Some(model) => {
            let groomed = model_chip_label(model);
            let label = if groomed.starts_with(&format!("{provider} · ")) {
                groomed.to_string()
            } else {
                format!("{provider} · {groomed}")
            };
            SharedString::from(format!("{label} ⌵"))
        }
        None => SharedString::from(format!("{provider} ⌵")),
    }
}

/// The control itself (#25): the meta row's accent chip — the same accent
/// tint as the header's provider chip, which is what marks it as the
/// provider's spot — at the meta row's own scale. Render-only; the
/// cockpit wires the click. Carried hover (#26): its accent wash is a
/// stronger ground the achromatic lift would downgrade — the selected-row
/// skip rule, applied to a chip.
pub fn provider_chip(label: SharedString) -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(theme::TEXT_CHIP))
        .text_color(rgb(ACCENT))
        .bg(rgba(theme::ACCENT_WASH))
        .rounded(px(theme::R_CHIP))
        .px(px(6.))
        .py(px(1.))
        .hover_carried()
        .child(label)
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
        .gap(px(theme::TRANSCRIPT_GAP))
        .px(px(theme::TRANSCRIPT_PAD_X))
        .py(px(theme::TRANSCRIPT_PAD_Y))
        .text_size(px(theme::TEXT_BODY))
        .line_height(relative(theme::LINE_TRANSCRIPT))
        // Characters here are grabbable (#27): the I-beam says so over the
        // whole scrollback, gutters and gaps included, because a press
        // anywhere in it anchors at the nearest character.
        .hover_text();
    for block in rendered_window(transcript.blocks(), level) {
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
        ));
    }
    body
}

fn parked_body() -> Div {
    div()
        .flex()
        .flex_1()
        .min_h_0()
        .items_center()
        .justify_center()
        .text_size(px(theme::TEXT_ROW))
        .text_color(rgb(INK_MUTED))
        .child("parked")
}

// --------------------------------------------------------------- Composer

/// The Composer stack's slice of `PaneState`, bundled so `composer_region`
/// stays readable as the states grow.
struct ComposerStack<'a> {
    decision: Option<&'a Decision>,
    /// The Decision's keycaps, wired in the cockpit (#26).
    decide: Option<AnyElement>,
    queued: Option<&'a str>,
    running: bool,
    empty: bool,
    history_available: bool,
    menu: Option<AnyElement>,
    mode: Option<&'a str>,
    /// The meta row's provider control (#25) — Some pre-lock only.
    provider_chip: Option<AnyElement>,
    /// The draft Pane's pre-prompt band (#29) — Some on drafts only, and
    /// gone with the first send.
    band: Option<AnyElement>,
}

/// The PromptBox stack, top to bottom: pre-prompt band (drafts, #29),
/// permission card, queued row, the one growing input line, meta row.
/// Everything stacks above the line and is driven by keys — no send button,
/// no floating box. An open `/` or `@` popover hangs above the whole stack;
/// while a Decision pends the region carries the `Decision` key context so
/// y/n/a answer with the keyboard still in the Composer (#23).
fn composer_region(view: &PaneView, transcript: Option<&Transcript>, stack: ComposerStack) -> Div {
    let ComposerStack {
        decision,
        decide,
        queued,
        running,
        empty,
        history_available,
        menu,
        mode,
        provider_chip,
        band,
    } = stack;
    let is_draft = band.is_some();
    let mut region = div()
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .bg(rgb(theme::COMPOSER))
        .border_t_1()
        .border_color(rgba(EDGE_STRONG))
        .when(decision.is_some(), |region| region.key_context("Decision"));
    let stacked = decision.is_some() || queued.is_some();
    if let Some(band) = band {
        region = region.child(band);
    }
    if let Some(decision) = decision {
        region = region.child(decision_card(decision, decide));
    }
    if let Some(held) = queued {
        region = region.child(queued_line(held));
    }
    // The one line that grows. The idle placeholder overlays it after the
    // block cursor's slot (PromptBox state 01) and disappears the moment
    // there is text or a running turn (§6: hints hide while running). The
    // open menu's ComposerMenu key context lives on the Composer's own
    // focused node — set by the cockpit — where the tie-break works.
    let mut line = div()
        .relative()
        .flex_1()
        .min_w_0()
        .child(view.composer.clone());
    if empty && !running {
        line = line.child(
            div()
                .absolute()
                .left(px(theme::CURSOR_W + 3.))
                .top_0()
                .bottom_0()
                .flex()
                .items_center()
                .text_color(rgb(INK_MUTED))
                .child(placeholder(&view.name, offers_import(transcript))),
        );
    }
    let mut input = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(10.))
        .min_h(px(theme::COMPOSER_H))
        .px(px(theme::COMPOSER_PAD_X))
        .text_size(px(theme::TEXT_INPUT))
        .text_color(rgb(INK))
        // The hairline above the input row appears once something stacks
        // over it (PromptBox state 04).
        .when(stacked, |input| {
            input.border_t_1().border_color(rgba(HAIRLINE))
        })
        .child(if running {
            div()
                .flex_shrink_0()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(ACCENT))
                .child("◐")
        } else {
            div()
                .flex_shrink_0()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(ACCENT))
                .child("❯")
        })
        .child(line);
    if running {
        input = input.child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(INK_MUTED))
                .child("esc interrupt"),
        );
    }
    region = region.child(input);
    // The popover paints above the stack — deferred, so it escapes the
    // Pane's clip and draws over the transcript (the root selector's own
    // recipe, #24).
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

    // The meta row: the Session's mode chip on the left where the comp
    // draws "⏵ auto-edit" — only when the Session actually announced a
    // mode. The model moved to the header's provider chip (#22 C12). Tool
    // durations live on the rows themselves: the transcript's folds stay
    // clockless, and the Cockpit — which already owns IO — stamps each
    // call at ingestion, which is why a replayed log carries none.
    let mut meta = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(10.))
        .h(px(theme::COMPOSER_META_H))
        .px(px(theme::COMPOSER_PAD_X))
        .pb(px(4.));
    if let Some(mode) = mode {
        meta = meta.child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(ACCENT))
                .bg(rgba(theme::ACCENT_WASH))
                .rounded(px(theme::R_CHIP))
                .px(px(6.))
                .py(px(2.))
                .child(mode_chip_label(mode)),
        );
    }
    meta = meta.child(div().flex_1());
    // The footer's right side, per the Main comp: the hints this Composer
    // actually answers (#23's menus and #30's eligible prompt history), then the
    // provider's spot beside them. Before the first prompt that spot is
    // the selector's chip control (#25); after the lock it reverts to
    // today's plain muted label. The header's provider chip stays.
    meta = meta.child(
        div()
            .flex_shrink_0()
            .text_size(px(theme::TEXT_CHIP))
            .text_color(rgb(INK_MUTED))
            .child(composer_hints(is_draft, history_available)),
    );
    if let Some(chip) = provider_chip {
        meta = meta.child(chip);
    } else if let Some(model) = transcript.and_then(Transcript::model) {
        meta = meta.child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(INK_MUTED))
                .child(model_chip_label(model)),
        );
    }
    region.child(meta)
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

/// The idle line's ghost text, PromptBox state 01's pattern verbatim:
/// `message ‹thread-name› — hints`. The hints it advertises are the ones
/// this Composer actually answers — which is why the import hint only
/// appears while the Thread still offers adoption (#11).
fn placeholder(name: &SharedString, offers_import: bool) -> SharedString {
    if name.as_ref() == "new thread" {
        return SharedString::from(
            "message new thread — /import adopt · @ project files · ↵ start",
        );
    }
    let import = if offers_import {
        " · /import adopt a CLI session"
    } else {
        ""
    };
    SharedString::from(format!(
        "message {name} — / commands · @ files · ↵ send{import}"
    ))
}

/// #11: whether this Thread still offers adopting a CLI session — no
/// conversation yet (nothing in the transcript beyond Ferrite's own notices
/// and bookkeeping) and at rest. One predicate for every surface that opens
/// the door — the placeholder hint, the `/` menu's local entry, and the
/// pick that closes the blank Thread — so no two can disagree.
pub fn offers_import(transcript: Option<&Transcript>) -> bool {
    transcript.is_some_and(|transcript| {
        transcript.status() == Status::Idle
            && transcript
                .blocks()
                .iter()
                .all(|block| matches!(block.body, Body::Notice(_) | Body::Meta(_)))
    })
}

/// The meta row's mode chip text: the comp's own name for acceptEdits
/// ("⏵ auto-edit"); every other mode wears the provider's word verbatim
/// rather than a guessed translation.
fn mode_chip_label(mode: &str) -> SharedString {
    let label = match mode {
        "acceptEdits" => "auto-edit",
        other => other,
    };
    SharedString::from(format!("⏵ {label}"))
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
    /// Matched byte ranges inside `name`, promoted to ACCENT per the comp.
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

/// One 26px menu row. Selection promotes the row exactly as the comp's
/// states 02/03 draw it: EDGE wash, name to ACCENT, matched characters
/// ACCENT (bold only while selected), detail ink one step up; the selected
/// row carries the `↵` hint at its right edge.
pub fn menu_row(row: &MenuRow, selected: bool) -> Div {
    // An inert row never promotes: muted whatever the arrows do, and its
    // matches stay unpainted — the row is an explanation, not an offer.
    let name_ink = match (row.inert, selected) {
        (true, _) => INK_MUTED,
        (false, true) => ACCENT,
        (false, false) => INK_SECONDARY,
    };
    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    if !row.inert {
        for range in &row.matched {
            highlights.push((
                range.clone(),
                HighlightStyle {
                    color: Some(rgb(ACCENT).into()),
                    font_weight: selected.then_some(FontWeight::BOLD),
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
        .rounded(px(theme::R_CHIP))
        .when(selected, |row| row.bg(rgba(EDGE)))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_CODE))
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
        let detail_ink = if selected { INK_TERTIARY } else { INK_MUTED };
        let mut detail = div()
            .min_w_0()
            .truncate()
            .text_color(rgb(detail_ink))
            .child(row.detail.clone());
        detail = if row.prose_detail {
            detail
                .font_family(theme::FONT_UI)
                .text_size(px(theme::TEXT_ROW))
        } else {
            detail.text_size(px(theme::TEXT_META))
        };
        drawn = drawn.child(detail);
    }
    // No ↵ hint on an inert row: enter only dismisses there, and a keycap
    // would advertise an offer the row does not make.
    if selected && !row.inert {
        drawn = drawn.child(div().flex_1()).child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(INK_MUTED))
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
        .gap(px(8.))
        .h(px(theme::CELL_HEADER_H))
        .px(px(theme::COMPOSER_PAD_X))
        .text_size(px(theme::TEXT_TITLE))
        .child(div().flex_shrink_0().text_color(rgb(INK_MUTED)).child("⏳"))
        .child(
            div()
                .min_w_0()
                .truncate()
                .italic()
                .text_color(rgb(INK_TERTIARY))
                .child(SharedString::from(format!("queued — \"{held}\""))),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(INK_MUTED))
                .child("⌫ unqueue"),
        )
}

/// The permission card, exactly as PromptBox state 04 draws it: warning
/// glyph, the command with its subtitle, and the y/n/a keycaps riding the
/// right edge. Kept free of focus and key wiring so it can be drawn — and
/// smoke-rendered — on its own; the keycaps arrive wired from the cockpit
/// (#26). The comp's warning-triangle SVG is stood in by the ⚠ glyph the
/// wall already speaks; gpui here has no asset pipeline to load an icon
/// from.
fn decision_card(decision: &Decision, decide: Option<AnyElement>) -> Div {
    let command = decision_subject(decision);
    let subtitle = decision_wants(decision);
    let card = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(10.))
        .mt(px(8.))
        .mx(px(8.))
        .px(px(10.))
        .py(px(8.))
        .bg(rgba(WAIT_WASH))
        .border_1()
        .border_color(rgba(WAIT_EDGE))
        .rounded(px(theme::R_CARD))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_ALERT_GLYPH))
                .text_color(rgb(WAIT))
                .child("⚠"),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(theme::TEXT_CODE))
                        .text_color(rgb(INK))
                        .truncate()
                        .child(command),
                )
                .child(
                    div()
                        .font_family(theme::FONT_UI)
                        .text_size(px(theme::TEXT_META))
                        .text_color(rgb(INK_TERTIARY))
                        .truncate()
                        .child(subtitle),
                ),
        )
        .child(div().flex_1());
    card.children(decide)
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

/// One keyboard keycap as the comps draw it: mono 10 on RAISED, radius 4,
/// wearing the RAISED-ground hover and press — the mouse presses the key
/// it depicts (#26). The label doubles as the element id the pressed
/// shade tracks; two keycaps never share one in a card. `a always` is
/// de-emphasized by ink and a fainter border, never removed.
fn keycap(label: &'static str, ink: u32, edge: u32) -> Stateful<Div> {
    div()
        .id(label)
        .flex_shrink_0()
        .text_size(px(theme::TEXT_CHIP))
        .text_color(rgb(ink))
        .bg(rgb(RAISED))
        .border_1()
        .border_color(rgba(edge))
        .rounded(px(theme::R_CHIP))
        .px(px(6.))
        .py(px(2.))
        .hover_raised()
        .press_raised()
        .child(label)
}

/// The decide keycaps, one constructor per verb, so the cockpit can wire
/// each press without respelling the keycap grammar (#26).
pub fn keycap_allow() -> Stateful<Div> {
    keycap("y allow", INK, EDGE_STRONG)
}
pub fn keycap_deny() -> Stateful<Div> {
    keycap("n deny", INK_SECONDARY, EDGE_STRONG)
}
pub fn keycap_always() -> Stateful<Div> {
    keycap("a always", INK_MUTED, EDGE)
}

/// The keycaps' cluster: the L1 card seats them on its own 10px pitch, the
/// L2 body packs them at 6 — the two comps' spacings, unchanged.
pub fn decide_row(level: Level) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(if level == Level::Transcript { 10. } else { 6. }))
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
        .border_color(rgb(INK_FAINT))
}

/// The CHANGED strip riding above the Composer: every file this Thread's
/// edits touched, rolled up as bordered chips — `pane.rs +2 −1` (#22 C11).
fn changed_strip(changed: &[ferrite_core::docview::FileChange]) -> Div {
    let mut strip = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .px(px(theme::TRANSCRIPT_PAD_X))
        .py(px(5.))
        .border_t_1()
        .border_color(rgba(HAIRLINE))
        .overflow_hidden()
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(INK_MUTED))
                .child("CHANGED"),
        );
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
                .gap(px(6.))
                .text_size(px(theme::TEXT_META))
                .text_color(rgb(INK_SECONDARY))
                .bg(rgb(RAISED))
                .border_1()
                .border_color(rgba(EDGE))
                .rounded(px(theme::R_CHIP))
                .px(px(7.))
                .py(px(2.))
                .child(SharedString::from(name))
                .child(diff_stat(file.added, file.removed)),
        );
    }
    strip
}

/// `+N −N`, green beside red — the one diff-stat pair every surface (tool
/// rows, L2 badges, CHANGED chips) draws. Text size is the caller's.
fn diff_stat(added: usize, removed: usize) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(4.))
        .child(
            div()
                .text_color(rgb(GOOD))
                .child(SharedString::from(format!("+{added}"))),
        )
        .child(
            div()
                .text_color(rgb(FAIL))
                .child(SharedString::from(format!("−{removed}"))),
        )
}

/// A small status chip: 10px ink on a wash, radius 4.
fn chip(label: impl Into<SharedString>, ink: u32, wash: u32) -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(theme::TEXT_CHIP))
        .text_color(rgb(ink))
        .bg(rgba(wash))
        .rounded(px(theme::R_CHIP))
        .px(px(6.))
        .py(px(1.))
        .child(label.into())
}

/// `8.2s` under ten seconds, `42s` under a minute, `2m14s` beyond — the
/// comps' duration grammar, shared by tool rows and activity lines.
fn duration_label(elapsed: Duration) -> SharedString {
    let secs = elapsed.as_secs_f64();
    if secs < 10.0 {
        SharedString::from(format!("{secs:.1}s"))
    } else if secs < 60.0 {
        SharedString::from(format!("{}s", secs as u64))
    } else {
        let whole = secs as u64;
        SharedString::from(format!("{}m{:02}s", whole / 60, whole % 60))
    }
}

/// The context ring (#22 C12 amended): a small donut whose ACCENT arc fills
/// clockwise from 12 o'clock with the used fraction of the window, over an
/// EDGE track. The track is a bordered circle; the arc is a sampled annular
/// sector — gpui has no arc primitive, and at this size a 5°-step polygon
/// is indistinguishable from a true arc at 1x and 2x.
pub fn usage_ring(fraction: f32) -> Div {
    // A full ring's seam would degenerate the polygon; one part in a
    // thousand is invisible at 13px.
    let fraction = fraction.clamp(0.0, 1.0).min(0.999);
    div()
        .relative()
        .flex_shrink_0()
        .w(px(theme::USAGE_RING_D))
        .h(px(theme::USAGE_RING_D))
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded_full()
                .border(px(theme::USAGE_RING_W))
                .border_color(rgba(EDGE)),
        )
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    if fraction <= 0.0 {
                        return;
                    }
                    let outer = bounds.size.width.min(bounds.size.height) * 0.5;
                    let inner = outer - px(theme::USAGE_RING_W);
                    window.paint_path(
                        arc_path(bounds.center(), inner, outer, fraction),
                        rgb(ACCENT),
                    );
                },
            )
            .absolute()
            .inset_0(),
        )
}

/// The annular sector from 12 o'clock through `fraction` of the circle,
/// sampled clockwise — one quad contour per step. gpui fills a contour as
/// a fan from its start with no winding rule, so a single outer-then-inner
/// outline would fill the hole; per-segment quads pave exactly the band.
fn arc_path(
    center: gpui::Point<Pixels>,
    inner: Pixels,
    outer: Pixels,
    fraction: f32,
) -> gpui::Path<Pixels> {
    const STEPS_FULL: usize = 72;
    let steps = ((fraction * STEPS_FULL as f32).ceil() as usize).max(2);
    let sweep = fraction * std::f32::consts::TAU;
    let start = -std::f32::consts::FRAC_PI_2;
    let at = |radius: Pixels, angle: f32| {
        point(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        )
    };
    let angle = |step: usize| start + sweep * step as f32 / steps as f32;
    let mut path = gpui::Path::new(at(outer, start));
    for step in 0..steps {
        path.move_to(at(outer, angle(step)));
        path.line_to(at(outer, angle(step + 1)));
        path.line_to(at(inner, angle(step + 1)));
        path.line_to(at(inner, angle(step)));
    }
    path
}

/// The ring's hover card (#22 C12): the full detail — current / maximum
/// tokens — on the popover surface every other overlay uses. The cockpit
/// hangs it under the ring while the pointer is on it.
pub fn usage_card(usage: Usage) -> Div {
    let detail = match usage.context_window {
        Some(window) if window > 0 => {
            format!("{} / {}", tokens(usage.total_tokens), tokens(window))
        }
        _ => tokens(usage.total_tokens),
    };
    popover_shell().child(
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(6.))
            .h(px(theme::MENU_ROW_H))
            .px(px(8.))
            .text_size(px(theme::TEXT_META))
            .text_color(rgb(INK_SECONDARY))
            .child(SharedString::from(detail))
            .child(
                div()
                    .font_family(theme::FONT_UI)
                    .text_color(rgb(INK_MUTED))
                    .child("tokens"),
            ),
    )
}

/// One 23px pane-header window control (#22 amendment): a quiet INK_MUTED
/// glyph on nothing, lifted a step on hover. Wiring is the cockpit's — the
/// verbs are the existing park and zoom-back moves, never new semantics.
pub fn control_button(id: (&'static str, usize), glyph: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(theme::CONTROL_BTN))
        .h(px(theme::CONTROL_BTN))
        .rounded(px(theme::R_CHIP))
        .text_size(px(theme::TEXT_CODE))
        .text_color(rgb(INK_MUTED))
        .hover_control()
        .press_control()
        .child(glyph)
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

/// Every popover's shell, in the comps' one popover language (PromptBox
/// state 02): RAISED surface, EDGE_STRONG border, radius 4, 4px padding,
/// and the three-layer popover elevation. Width is the caller's — the
/// Composer menus span the composer. Rows and footer are the cockpit's to
/// append — their clicks are wired there.
fn popover_shell() -> Div {
    div()
        .flex()
        .flex_col()
        .p(px(theme::POPOVER_PAD))
        .bg(rgb(RAISED))
        .border_1()
        .border_color(rgba(EDGE_STRONG))
        .rounded(px(theme::R_CHIP))
        .shadow(vec![
            BoxShadow {
                color: rgba(theme::RING_FAINT).into(),
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(1.),
            },
            BoxShadow {
                color: rgba(theme::SHADOW_NEAR).into(),
                offset: point(px(0.), px(2.)),
                blur_radius: px(4.),
                spread_radius: px(0.),
            },
            BoxShadow {
                color: rgba(theme::SHADOW_FAR).into(),
                offset: point(px(0.), px(6.)),
                blur_radius: px(16.),
                spread_radius: px(-4.),
            },
        ])
}

/// The ✓-row recipe the pickers share — the provider picker (#25) and the
/// band popovers (#29) — so "what this Pane is on right now" can never be
/// spelled two ways. `detail` is the muted section tag riding the right
/// edge ("provider", "worktree"); empty draws nothing.
pub fn picker_row(label: SharedString, detail: SharedString, selected: bool, active: bool) -> Div {
    let mut row = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(10.))
        .h(px(theme::MENU_ROW_H))
        .px(px(8.))
        .rounded(px(theme::R_CHIP))
        .text_size(px(theme::TEXT_CODE))
        .text_color(rgb(if selected { ACCENT } else { INK_SECONDARY }))
        .child(div().min_w_0().truncate().child(label))
        .child(div().flex_1());
    // The Row role (#26), the menu rows' skip rule: the selected row's
    // EDGE ground outranks the wash, so it keeps only the cursor.
    row = if selected {
        row.bg(rgba(EDGE)).hover_carried()
    } else {
        row.hover_row()
    };
    if !detail.is_empty() {
        row = row.child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_META))
                .text_color(rgb(INK_MUTED))
                .child(detail),
        );
    }
    if active {
        row = row.child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_META))
                .text_color(rgb(ACCENT))
                .child("✓"),
        );
    }
    row
}

/// A muted, non-interactive picker line — why a section is short, said out
/// loud (#25: the other rows only arrive with the Session's handshake).
pub fn picker_hint(text: &'static str) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::MENU_ROW_H))
        .px(px(8.))
        .text_size(px(theme::TEXT_META))
        .text_color(rgb(INK_MUTED))
        .child(text)
}

/// The popover's key-hint footer — the PromptBox footer grammar, each
/// menu supplying its own verbs.
pub fn popover_footer(hints: &'static str) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::POPOVER_FOOTER_H))
        .px(px(8.))
        .mt(px(2.))
        .border_t_1()
        .border_color(rgba(HAIRLINE))
        .text_size(px(theme::TEXT_META))
        .text_color(rgb(INK_MUTED))
        .child(hints)
}

// ----------------------------------------------------------- Block render

/// One Block at DirectionDense density: 14px glyph gutter for prompt and
/// agent rows, 22px indent for the agent's long-form (headings, bullets,
/// code) and for `⎿` continuations and bare diffs under tool rows.
/// Every text run routes through the selection overlay (#27) — that is
/// what makes it selectable and copyable; the gutter glyphs, chips and
/// diff line numbers around the runs are chrome, and stay plain.
fn render_block(
    block: &Block,
    selection: &SelectionOverlay,
    timings: Option<&HashMap<String, ToolTiming>>,
    expanded: bool,
    disclosure: Option<AnyElement>,
) -> AnyElement {
    let row = div().w_full().flex_shrink_0();
    match &block.body {
        Body::Prompt(line) => gutter_row(row, "❯", ACCENT, true)
            .child(div().min_w_0().text_color(rgb(INK)).child(selection.line(
                block.id,
                line.clone(),
                Vec::new(),
            )))
            .into_any_element(),
        Body::Paragraph { spans } => {
            let (text, highlights) = inline(spans);
            gutter_row(row, "⏺", INK_TERTIARY, false)
                .child(
                    div()
                        .min_w_0()
                        .text_color(rgb(INK_SECONDARY))
                        .child(selection.line(block.id, text, highlights)),
                )
                .into_any_element()
        }
        Body::Heading { spans, .. } => {
            let (text, highlights) = inline(spans);
            row.flex()
                .pl(px(theme::INDENT))
                .child(
                    div()
                        .text_size(px(theme::TEXT_HEADING))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(INK))
                        .child(selection.line(block.id, text, highlights)),
                )
                .child(div().flex_1())
                .into_any_element()
        }
        Body::Bullet { spans } => {
            let (text, highlights) = inline(spans);
            row.flex()
                .flex_row()
                .gap(px(6.))
                .pl(px(theme::INDENT))
                .text_color(rgb(INK_SECONDARY))
                .child(div().flex_shrink_0().text_color(rgb(ACCENT)).child("•"))
                .child(
                    div()
                        .min_w_0()
                        .child(selection.line(block.id, text, highlights)),
                )
                .into_any_element()
        }
        // Out-of-band lines share the text column, not the gutter — one
        // left edge for everything that reads as prose (#22 D22).
        Body::Thinking(thought) => row
            .pl(px(theme::INDENT))
            .text_color(rgb(INK_FAINT))
            .child(selection.line(block.id, thought.clone(), Vec::new()))
            .into_any_element(),
        Body::Notice(text) => row
            .pl(px(theme::INDENT))
            .text_color(rgb(WAIT))
            .child(selection.line(block.id, text.clone(), Vec::new()))
            .into_any_element(),
        Body::Meta(text) => row
            .pl(px(theme::INDENT))
            .text_size(px(theme::TEXT_ROW))
            .text_color(rgb(INK_MUTED))
            .child(selection.line(block.id, text.clone(), Vec::new()))
            .into_any_element(),
        Body::Code {
            language,
            source,
            tokens,
        } => row
            .pl(px(theme::INDENT))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .bg(rgb(INSET))
                    .border_1()
                    .border_color(rgba(EDGE))
                    .rounded(px(theme::R_TIGHT))
                    .overflow_hidden()
                    .children(language.as_ref().map(|language| {
                        div()
                            .flex()
                            .items_center()
                            .h(px(20.))
                            .px(px(8.))
                            .border_b_1()
                            .border_color(rgba(HAIRLINE))
                            .text_size(px(theme::TEXT_CHIP))
                            .text_color(rgb(INK_FAINT))
                            .child(SharedString::from(language.clone()))
                    }))
                    .child(
                        div()
                            .px(px(8.))
                            .py(px(6.))
                            .text_size(px(theme::TEXT_CODE))
                            .line_height(relative(theme::LINE_CODE))
                            .child(selection.line(
                                block.id,
                                source.clone(),
                                code(source, tokens.as_deref()),
                            )),
                    ),
            )
            .into_any_element(),
        Body::Tool(tool) => render_tool(
            row, block.id, tool, selection, timings, expanded, disclosure,
        ),
    }
}

/// A `.row` with the 14px glyph gutter — ❯ for the operator, ⏺ for the agent.
fn gutter_row(row: Div, glyph: &'static str, color: u32, bold: bool) -> Div {
    row.flex().flex_row().gap(px(8.)).child(
        div()
            .flex_shrink_0()
            .w(px(theme::GUTTER_W))
            .text_color(rgb(color))
            .when(bold, |glyph| glyph.font_weight(FontWeight::BOLD))
            .child(glyph),
    )
}

/// `⏺ Name(arg)` with its `⎿` continuation and bare diff, per DirectionDense:
/// bold tool name, file args in accent, command args in prose ink. The row's
/// right edge carries its verdict — the diff stat for an edit, the pass/exit
/// chip for a command that succeeded (#22 C9/C10) — and the call's measured
/// duration where the cockpit clocked it. The call composes name, `(`,
/// summary, `)` as overlay pieces of one copied line (#27): flex pieces keep
/// the summary-only truncation, and copy joins them with nothing. The chip,
/// the duration and the ⏺ are chrome and never register.
fn render_tool(
    row: Div,
    block: BlockId,
    tool: &ToolBlock,
    selection: &SelectionOverlay,
    timings: Option<&HashMap<String, ToolTiming>>,
    expanded: bool,
    disclosure: Option<AnyElement>,
) -> AnyElement {
    // Command runners' args read as prose; every other summary is a
    // path-like subject and takes the accent (the comps' file links,
    // rendered inert — opening files is not this pass).
    let command_runner = matches!(tool.name.as_str(), "Bash" | "commandExecution");
    let arg_color = if command_runner {
        INK_SECONDARY
    } else {
        ACCENT
    };
    let mut call = div().flex().min_w_0().text_color(rgb(INK_SECONDARY)).child(
        div()
            .flex_shrink_0()
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(INK))
            .child(selection.line(block, tool.name.clone(), Vec::new())),
    );
    if !tool.summary.is_empty() {
        call = call
            .child(
                div()
                    .flex_shrink_0()
                    .child(selection.piece(block, "(", Vec::new())),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(arg_color))
                    .child(selection.piece(block, tool.summary.clone(), Vec::new())),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .child(selection.piece(block, ")", Vec::new())),
            );
    }
    let has_disclosure = disclosure.is_some();
    let gutter = div()
        .relative()
        .flex_shrink_0()
        .w(px(theme::GUTTER_W))
        .children(disclosure)
        .when(!has_disclosure, |gutter| {
            gutter.text_color(rgb(INK_TERTIARY)).child("⏺")
        });
    let mut line = div()
        .flex()
        .flex_row()
        .gap(px(8.))
        .child(gutter)
        .child(call);
    // A settled call's clock, where the cockpit stamped one; running calls
    // tick on the activity line instead. Sub-tenth blips render nothing —
    // a column of 0.0s is noise, not an instrument.
    let settled_clock = timings
        .and_then(|map| map.get(&tool.call))
        .and_then(|timing| match timing {
            ToolTiming::Done(total) => Some(*total),
            ToolTiming::Running(_) => None,
        })
        .filter(|total| *total >= Duration::from_millis(100));
    // A pass chip that carries the run's own count subsumes the ⎿ line it
    // was promoted from; a countless chip keeps the line, which still says
    // more than the chip does.
    let mut promoted = false;
    let mut verdicts: Vec<AnyElement> = tool_verdicts(tool)
        .into_iter()
        .map(|verdict| match verdict {
            ToolVerdict::Diff(added, removed) => diff_stat(added, removed)
                .text_size(px(theme::TEXT_META))
                .into_any_element(),
            ToolVerdict::Failed => chip("failed", FAIL, FAIL_WASH).into_any_element(),
        })
        .collect();
    if verdicts.is_empty() && command_runner && matches!(tool.state, ToolState::Ok) {
        // A command runner that settled without an error exited 0 — that
        // is exactly what `is_error` carries for one; a test run reads its
        // count off its own result line, or stays honestly countless.
        let label = if is_test_run(tool) {
            match tool.result_line.as_deref().and_then(passed_count) {
                Some(count) => {
                    promoted = true;
                    SharedString::from(format!("✓ {count} passed"))
                }
                None => SharedString::from("✓ passed"),
            }
        } else {
            SharedString::from("exit 0")
        };
        verdicts.push(chip(label, GOOD, GOOD_WASH).into_any_element());
    }
    if !verdicts.is_empty() || settled_clock.is_some() {
        line = line.child(div().flex_1());
    }
    line = line.children(verdicts);
    if let Some(total) = settled_clock {
        line = line.child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_META))
                .text_color(rgb(INK_MUTED))
                .child(duration_label(total)),
        );
    }
    let mut card = row
        .flex()
        .flex_col()
        .gap(px(theme::TRANSCRIPT_GAP))
        .child(line);
    if expanded {
        if let Some(output) = &tool.output {
            let color = if matches!(tool.state, ToolState::Failed(_)) {
                FAIL
            } else {
                INK_MUTED
            };
            card = card.child(
                div()
                    .pl(px(theme::INDENT))
                    .w_full()
                    .text_color(rgb(color))
                    .child(selection.line(block, output.text.clone(), Vec::new())),
            );
            if output.omitted_bytes > 0 {
                card = card.child(
                    div()
                        .pl(px(theme::INDENT))
                        .text_size(px(theme::TEXT_META))
                        .text_color(rgb(INK_MUTED))
                        .child(format!(
                            "… {} bytes omitted from inline view",
                            output.omitted_bytes
                        )),
                );
            }
        }
    } else if !promoted {
        if let Some(line) = &tool.result_line {
            card = card.child(
                div()
                    .pl(px(theme::INDENT))
                    .w_full()
                    .truncate()
                    .text_color(rgb(INK_MUTED))
                    .child(selection.line(block, format!("⎿ {line}"), Vec::new())),
            );
        }
    }
    if !expanded {
        if let ToolState::Failed(message) = &tool.state {
            card = card.child(
                div()
                    .pl(px(theme::INDENT))
                    .w_full()
                    .truncate()
                    .text_color(rgb(FAIL))
                    .child(selection.line(block, format!("⎿ {message}"), Vec::new())),
            );
        }
    }
    if let Some(diff) = &tool.diff {
        card = card.child(render_diff(block, diff, selection));
    }
    card.into_any_element()
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
    div()
        .id(SharedString::from(format!("tool-disclosure-{call}")))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .absolute()
        .left(px((theme::GUTTER_W - theme::TOOL_DISCLOSURE_HIT) / 2.))
        .top(px(-1.))
        .w(px(theme::TOOL_DISCLOSURE_HIT))
        .h(px(theme::TOOL_DISCLOSURE_HIT))
        .rounded(px(theme::R_TIGHT))
        .text_color(rgb(if targeted { ACCENT } else { INK_MUTED }))
        .when(targeted, |control| {
            control
                .bg(rgba(ACCENT_WASH))
                .track_focus(focus)
                .key_context("ToolDisclosure")
        })
        .child(if expanded { "▾" } else { "▸" })
        .hover_control()
        .press_control()
}

/// A bare diff, per DirectionDense: no card, no filename header — the tool
/// row above already names the file. 22px indent, a 30px right-aligned
/// number column, washes for added and removed rows. The code cells route
/// through the overlay — their `+`/`-` signs are text and copy honestly;
/// the number column is chrome and never does (#27).
fn render_diff(block: BlockId, diff: &Diff, selection: &SelectionOverlay) -> impl IntoElement {
    let mut lines = div()
        .flex()
        .flex_col()
        .ml(px(theme::INDENT))
        .text_size(px(theme::TEXT_CODE));
    for hunk in &diff.hunks {
        let mut old = hunk.old_start;
        let mut new = hunk.new_start;
        for line in &hunk.lines {
            let (number, number_color, code_color, wash) = match line.chars().next() {
                Some('+') => {
                    let n = new;
                    new += 1;
                    (n, GOOD, INK, Some(GOOD_WASH))
                }
                Some('-') => {
                    let n = old;
                    old += 1;
                    (n, FAIL, INK, Some(FAIL_WASH))
                }
                _ => {
                    let n = new;
                    old += 1;
                    new += 1;
                    (n, INK_FAINT, INK_TERTIARY, None)
                }
            };
            let mut row = div().flex().gap(px(10.)).px(px(6.));
            if let Some(wash) = wash {
                row = row.bg(rgba(wash));
            }
            lines = lines.child(
                row.child(
                    div()
                        .flex_shrink_0()
                        .w(px(theme::DIFF_NUM_W))
                        .text_right()
                        .text_color(rgb(number_color))
                        .child(SharedString::from(number.to_string())),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(code_color))
                        .child(selection.line(block, line.clone(), Vec::new())),
                ),
            );
        }
    }
    lines
}

/// Token counts read at a glance, not to the digit — `412k`, `1M`, `1.5M`.
fn tokens(count: u64) -> String {
    fn scaled(value: f64, suffix: &str) -> String {
        if (value - value.round()).abs() < 0.05 {
            format!("{}{suffix}", value.round() as u64)
        } else {
            format!("{value:.1}{suffix}")
        }
    }
    match count {
        0..=999 => count.to_string(),
        1_000..=999_999 => scaled(count as f64 / 1_000.0, "k"),
        _ => scaled(count as f64 / 1_000_000.0, "M"),
    }
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
        let style = match span.style {
            Style::Plain => None,
            Style::Code => Some(HighlightStyle {
                // The comps' inline-code chip: primary ink on RAISED.
                color: Some(rgb(INK).into()),
                background_color: Some(rgb(RAISED).into()),
                ..Default::default()
            }),
            Style::Bold => Some(HighlightStyle {
                color: Some(rgb(INK).into()),
                font_weight: Some(FontWeight::BOLD),
                ..Default::default()
            }),
            Style::Link => Some(HighlightStyle {
                color: Some(rgb(ACCENT).into()),
                underline: Some(gpui::UnderlineStyle {
                    thickness: px(1.),
                    color: Some(rgba(theme::ACCENT_UNDERLINE).into()),
                    wavy: false,
                }),
                ..Default::default()
            }),
        };
        if let Some(style) = style {
            highlights.push((start..text.len(), style));
        }
    }
    (text, highlights)
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
        let color = match token.class {
            Class::Plain => ACCENT,
            Class::Keyword => CODE_KEYWORD,
            Class::Str => CODE_STR,
            Class::Comment => INK_FAINT,
            Class::Number => WAIT,
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
                    render_block(block, &overlay, None, expanded, None)
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

    /// #25: the pre-lock control reuses the model grooming with the ⌵ that
    /// says it answers clicks — and carries the provider name alone until
    /// the Session's Init names what is serving.
    #[test]
    fn the_provider_chip_label_grooms_the_model_or_names_the_provider_alone() {
        assert_eq!(provider_chip_label("claude", None).as_ref(), "claude ⌵");
        assert_eq!(
            provider_chip_label("claude", Some("claude-sonnet-4-5")).as_ref(),
            "claude · sonnet-4-5 ⌵"
        );
        assert_eq!(
            provider_chip_label("codex", Some("gpt-5.4-mini")).as_ref(),
            "codex · gpt-5.4-mini ⌵"
        );
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
            cursor(picker_row("workspace root".into(), "".into(), false, false)),
            Some(CursorStyle::PointingHand)
        );
        assert_eq!(
            cursor(picker_row("workspace root".into(), "".into(), true, true)),
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
        assert!(
            all.contains("+delta") && all.contains("-bravo"),
            "a diff registers its lines: {all}"
        );
        // The ⎿ continuation registers where it renders (Edit's result);
        // Bash's was promoted into its chip, which is chrome — so its count
        // never registers.
        assert!(all.contains("⎿ applied"), "the result line: {all}");

        view.update(cx, |view, cx| {
            view.expanded.insert("toolu_2".into());
            cx.notify();
        });
        cx.run_until_parked();
        let expanded = view.read_with(cx, |view, _| view.selection.registered(thread));
        assert_eq!(
            expanded
                .iter()
                .filter(|(_, _, _, text)| text == "+delta" || text == "-bravo")
                .count(),
            2,
            "the edit diff still renders exactly once expanded"
        );
        assert!(expanded.iter().any(|(_, _, _, text)| text == "applied"));
        assert!(!expanded.iter().any(|(_, _, _, text)| text == "⎿ applied"));
        assert_eq!(instruments.changed.len(), 1);
        assert_eq!((instruments.added, instruments.removed), (1, 1));
        assert!(
            !all.contains("42 passed"),
            "a promoted chip is chrome: {all}"
        );
        for chrome in ['❯', '⏺', '•', '✓'] {
            assert!(!all.contains(chrome), "{chrome} is chrome: {all}");
        }
        assert!(
            runs.iter().any(|(_, _, _, text)| text == "+delta"),
            "a diff cell is its bare line — no number column: {runs:?}"
        );
    }

    #[gpui::test]
    fn a_blocked_thread_paints_its_decision_card(cx: &mut TestAppContext) {
        let event = crate::session::script()
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

    /// The rollup rule: rings (Decision amber, blocker red) count; failing
    /// tests do not. The strip and the nav both read this one function.
    #[test]
    fn needs_operator_counts_rings_and_never_failing_tests() {
        assert!(needs_operator(true, Some(Status::Streaming)));
        assert!(needs_operator(false, Some(Status::Blocked)));
        assert!(needs_operator(false, Some(Status::Closed)));
        assert!(!needs_operator(false, Some(Status::Streaming)));
        assert!(!needs_operator(false, Some(Status::Idle)));
        assert!(!needs_operator(false, None));
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

    /// #23: the mode chip speaks the comp's name for acceptEdits and the
    /// provider's own word for everything else — never an invented label.
    #[test]
    fn the_mode_chip_labels_accept_edits_the_comp_way_and_the_rest_verbatim() {
        assert_eq!(mode_chip_label("acceptEdits").as_ref(), "⏵ auto-edit");
        assert_eq!(
            mode_chip_label("bypassPermissions").as_ref(),
            "⏵ bypassPermissions"
        );
        assert_eq!(mode_chip_label("plan").as_ref(), "⏵ plan");
        assert_eq!(mode_chip_label("default").as_ref(), "⏵ default");
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

    /// The hover card's numbers read at a glance — `412k / 1M`, never a
    /// trailing `.0`.
    #[test]
    fn token_counts_read_at_a_glance() {
        assert_eq!(tokens(412), "412");
        assert_eq!(tokens(124_000), "124k");
        assert_eq!(tokens(412_000), "412k");
        assert_eq!(tokens(1_000_000), "1M");
        assert_eq!(tokens(1_530_000), "1.5M");
    }

    /// The Dense header's ▰▱ meter stays glanceable: glyphs for small plans,
    /// the bare fraction for long ones, and done never overshoots.
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

    /// #11: the idle line's second hint — only where the door is open, and
    /// never displacing the hints every Composer answers.
    #[test]
    fn the_placeholder_advertises_import_only_on_a_fresh_thread() {
        let name = SharedString::from("thread-01");
        let plain = placeholder(&name, false);
        let offering = placeholder(&name, true);
        assert!(!plain.contains("/import"), "{plain}");
        assert!(offering.contains("/import"), "{offering}");
        for hints in [&plain, &offering] {
            assert!(hints.contains("/ commands"), "{hints}");
            assert!(hints.contains("↵ send"), "{hints}");
        }
    }
}
