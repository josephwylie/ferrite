//! One Pane: the visible cell for one Thread. Header, transcript, Composer,
//! and the three semantic-zoom renderings. Rendering only — everything it
//! shows is folded in core, and every key it answers to belongs to the
//! cockpit above it.
//!
//! The three levels follow the canon boards: L1 per DirectionDense (dense
//! transcript, 28px merged header, PromptBox composer stack), L2 per the
//! Cockpit board (instrument cell), L3 per the Wall board (dot · slug ·
//! bar · status line, inset attention rings).

use ferrite_core::docview::{Instruments, Level, Tests};
use ferrite_core::transcript::{
    Block, Body, Class, Diff, Span, Status, Style, Todos, Token, ToolBlock, ToolState, Transcript,
};
use ferrite_core::workspace::WorkspaceBinding;
use ferrite_core::{Decision, ThreadId};
use gpui::prelude::*;
use gpui::{
    deferred, div, point, px, relative, rgb, rgba, AnyElement, BoxShadow, Context, Div, Entity,
    FocusHandle, FontWeight, HighlightStyle, ScrollHandle, SharedString, StyledText,
};
use std::path::{Path, PathBuf};

use crate::composer::Composer;
// Every color and metric here is an Aperture token (crate::theme) — no
// literal survives in render code, which is #22's grep-able law.
use crate::theme;
use crate::theme::{
    ACCENT, CODE_KEYWORD, CODE_STR, EDGE, EDGE_STRONG, FAIL, FAIL_WASH, GOOD, GOOD_WASH, HAIRLINE,
    HOVER, IDLE, INK, INK_FAINT, INK_MUTED, INK_SECONDARY, INK_TERTIARY, INSET, RAISED, SELECTION,
    SURFACE, WAIT, WAIT_EDGE, WAIT_WASH,
};

/// One Pane's view state: what the window owns per Thread. Everything it
/// shows lives in core; this is the keyboard, the scrollback position, and
/// the wall cell's cached strings.
pub struct PaneView {
    pub thread: ThreadId,
    /// The Thread's slug name — `thread-NN` until display names exist
    /// (sidebar-and-impl §4.2 #8). Built once; the wall must not format
    /// names per frame.
    pub name: SharedString,
    pub composer: Entity<Composer>,
    pub scroll: ScrollHandle,
    /// A pending Decision takes the keyboard: y and n are answers, not text.
    pub decision_focus: FocusHandle,
    /// The wall cell's folded reading — everything the L3 recipe needs that
    /// is not an O(1) transcript read. The cockpit rebuilds it whenever the
    /// Thread's transcript changes; a frame never walks Blocks at L3.
    pub wall: WallCard,
}

impl PaneView {
    pub fn new<T: 'static>(thread: ThreadId, cx: &mut Context<T>) -> Self {
        Self {
            thread,
            name: SharedString::from(format!("thread-{thread:02}")),
            composer: cx.new(Composer::new),
            scroll: ScrollHandle::new(),
            decision_focus: cx.focus_handle(),
            wall: WallCard::default(),
        }
    }
}

/// Everything one Pane draws, as the cockpit reads it for this frame.
pub struct PaneState<'a> {
    pub transcript: Option<&'a Transcript>,
    pub decision: Option<&'a Decision>,
    pub queued: Option<&'a str>,
    pub workspace: Option<&'a WorkspaceBinding>,
    /// The session-project-root chip for the L1 header — and, while the
    /// selector is open on this Pane, the popover hanging under it (#24).
    /// Assembled in the cockpit, where its clicks are wired beside every
    /// other pointer, exactly as the nav's rows are; None on a Thread with
    /// no binding, which has nothing for a root to be inside.
    pub root_chip: Option<AnyElement>,
    /// The open `/` or `@` popover for this Pane's Composer, assembled in the
    /// cockpit exactly like `root_chip` — rows wired to their picks there —
    /// and hung above the input line here (#23). None when no menu is open.
    pub menu: Option<AnyElement>,
    /// Whether the Composer line is empty — what decides the idle
    /// placeholder, read where the cockpit has a `cx` to read it with.
    pub composer_empty: bool,
    /// The Session's permission mode, in the provider's own word — the meta
    /// row's mode chip (#23). None (no announcement, or a provider that
    /// makes none) draws no chip; display-only either way.
    pub permission_mode: Option<&'a str>,
    pub focused: bool,
    /// A turn in flight: the Composer's ❯ becomes ◐ and esc offers interrupt.
    pub running: bool,
    /// The Blocks a drag swept, as indices into the Thread's blocks. The
    /// cockpit owns the drag; the Pane only paints the wash.
    pub selected: Option<std::ops::RangeInclusive<usize>>,
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
/// for "done": an idle Thread that has a recorded turn cost (the Cockpit
/// board's own done cell reads "turn complete · $0.31").
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
    /// Plan progress, 0..=1, when the Thread has a plan.
    pub fraction: Option<f32>,
    /// The working status line: `3/4 · ◐ working` or `◐ working`.
    pub working: SharedString,
    /// The done line: `✓ done · $0.31`, or `✓ done` before any cost.
    pub done: SharedString,
    /// An alert cell's second line: the Decision's subject, or the close
    /// reason. Empty when neither applies.
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
    let fraction = todos.map(plan_fraction);
    let working = match todos {
        Some(todos) => SharedString::from(format!("{}/{} · ◐ working", todos.done, todos.total)),
        None => SharedString::from("◐ working"),
    };
    let done = match transcript.last_cost() {
        Some(cost) => SharedString::from(format!("✓ done · ${cost:.2}")),
        None => SharedString::from("✓ done"),
    };
    let context = match decision {
        Some(decision) => decision_subject(decision),
        // A closed Thread's context is the reason it closed — the last
        // Notice the fold pushed.
        None if transcript.status() == Status::Closed => transcript
            .blocks()
            .iter()
            .rev()
            .find_map(|block| match &block.body {
                Body::Notice(reason) => Some(SharedString::from(reason.clone())),
                _ => None,
            })
            .unwrap_or_default(),
        None => SharedString::default(),
    };
    WallCard {
        tests_failing: Instruments::of(transcript).tests == Some(Tests::Failed),
        fraction,
        working,
        done,
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
        root_chip,
        menu,
        composer_empty,
        permission_mode,
        focused,
        running,
        selected,
    } = state;
    let status = transcript.map(|t| t.status());
    let state = wall_state(
        status,
        decision.is_some(),
        view.wall.tests_failing,
        transcript.and_then(|t| t.last_cost()).is_some(),
    );
    // The attention ring: a Decision's amber overrides focus everywhere; the
    // red blocker ring is the wall's language (glance.md §4 — L2/L1 blocked
    // renderings are undrawn, so red stays at L3 and the LED carries it up).
    let ring = if decision.is_some() {
        Some(WAIT)
    } else if level == Level::Wall && state == WallState::Blocked {
        Some(FAIL)
    } else if focused {
        Some(ACCENT)
    } else {
        None
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
        .overflow_hidden();

    // Far enough away, a Pane is one signal: no header, no transcript,
    // nothing that stops reading at a glance.
    if level == Level::Wall {
        return shell
            .child(wall_cell(view, state, focused))
            .children(ring.map(ring_overlay));
    }

    if level == Level::Instruments {
        return shell
            .child(l2_cell(view, transcript, decision, workspace, state))
            .children(ring.map(ring_overlay));
    }

    let mut pane = shell.child(dense_header(view, transcript, workspace, status, root_chip));
    match transcript {
        Some(transcript) => {
            pane = pane
                .child(body(view, transcript, level.visible_blocks(), selected))
                .child(composer_region(
                    view,
                    transcript,
                    ComposerStack {
                        decision,
                        queued,
                        running,
                        empty: composer_empty,
                        menu,
                        mode: permission_mode,
                    },
                ));
        }
        None => {
            pane = pane.child(parked_body());
        }
    }
    pane.children(ring.map(ring_overlay))
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

// ------------------------------------------------------------------ L3 wall

/// The Wall board's cell recipe: 8px padding, 6px gaps, top-anchored —
/// dot · slug name · 5px bar · one 9px status line; alert states carry a
/// 10px colored first line instead of the bar.
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
    // The bar survives on working cells only; alert cells trade it for the
    // colored first line (glance.md §3.4).
    if matches!(state, WallState::Working | WallState::Failing) {
        if let Some(fraction) = card.fraction {
            cell = cell.child(bar(px(theme::BAR_H_WALL), fraction, ACCENT));
        }
    }
    let status_line = |text: SharedString, size: f32, color: u32| {
        div()
            .flex_shrink_0()
            .min_w_0()
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
                SharedString::from("✗ failing"),
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
            cell = cell.child(status_line(
                SharedString::from("✗ closed"),
                theme::TEXT_CHIP,
                FAIL,
            ));
            if !card.context.is_empty() {
                cell = cell.child(status_line(
                    card.context.clone(),
                    theme::TEXT_WALL_STATUS,
                    INK_MUTED,
                ));
            }
        }
        WallState::Done => {
            cell = cell
                .child(status_line(
                    card.done.clone(),
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
fn l2_cell(
    view: &PaneView,
    transcript: Option<&Transcript>,
    decision: Option<&Decision>,
    workspace: Option<&WorkspaceBinding>,
    state: WallState,
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
    header = match state {
        WallState::Decision => header.child(needs_you_badge()),
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
            l2_decision_body(decision)
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
        let line = match transcript.last_cost() {
            Some(cost) => SharedString::from(format!("turn complete · ${cost:.2}")),
            None => SharedString::from("turn complete"),
        };
        body = body.child(
            div()
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(INK_TERTIARY))
                .child(line),
        );
    }

    if let Some(todos) = read.todos {
        // Progress fill follows health: accent while green, secondary while
        // the suite is red (the Cockpit board's two data points).
        let fill = if state == WallState::Failing {
            INK_SECONDARY
        } else {
            ACCENT
        };
        let fraction = plan_fraction(todos);
        body = body.child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().flex_1().child(bar(px(theme::BAR_H), fraction, fill)))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(theme::TEXT_META))
                        .text_color(rgb(fill))
                        .child(SharedString::from(format!(
                            "{}/{}",
                            todos.done, todos.total
                        ))),
                ),
        );
    }

    let mut badges = div().flex().items_center().gap(px(6.));
    let mut any_badge = false;
    match read.tests {
        Some(Tests::Passed) => {
            badges = badges.child(chip("✓ tests pass", GOOD, GOOD_WASH));
            any_badge = true;
        }
        Some(Tests::Failed) => {
            badges = badges.child(chip("✗ failing", FAIL, FAIL_WASH));
            any_badge = true;
        }
        None => {}
    }
    if read.added > 0 || read.removed > 0 {
        badges = badges.child(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .text_size(px(theme::TEXT_CHIP))
                .bg(rgb(RAISED))
                .rounded(px(theme::R_CHIP))
                .px(px(6.))
                .py(px(1.))
                .child(
                    div()
                        .text_color(rgb(GOOD))
                        .child(SharedString::from(format!("+{}", read.added))),
                )
                .child(
                    div()
                        .text_color(rgb(FAIL))
                        .child(SharedString::from(format!("−{}", read.removed))),
                ),
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
        body = body.child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(INK_TERTIARY))
                .child(SharedString::from(format!("◐ {activity}"))),
        );
    }

    let mut content = cell.child(header).child(body);
    if state == WallState::Done {
        content = content.opacity(theme::DONE_CELL_OPACITY);
    }
    content
}

/// The Cockpit board's Decision cell body: the command, who wants it, and
/// the y/n keycaps — no `a always` at L2.
fn l2_decision_body(decision: &Decision) -> Div {
    let command = decision_subject(decision);
    let wants = decision_wants(decision, "wants approval to run");
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .p(px(theme::CELL_PAD))
        .gap(px(6.))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(theme::TEXT_META))
                .text_color(rgb(INK))
                .child(command),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_family(theme::FONT_UI)
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(INK_TERTIARY))
                .child(wants),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .gap(px(6.))
                .child(keycap("y allow", INK, EDGE_STRONG))
                .child(keycap("n deny", INK_SECONDARY, EDGE_STRONG)),
        )
}

/// The amber `needs you` chip, exactly as the Cockpit board's issue-triage
/// cell draws it.
fn needs_you_badge() -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(theme::TEXT_CHIP_SM))
        .text_color(rgb(WAIT))
        .bg(rgba(WAIT_WASH))
        .rounded(px(theme::R_CHIP))
        .px(px(5.))
        .py(px(1.))
        .child("needs you")
}

// ---------------------------------------------------------------- L1 pane

/// DirectionDense's single 28px header: LED · name · binding · spacer ·
/// todo meter · ctx and cost as text. The todo strip, ctx bar and cost of
/// the Main board fold into this one line at dense L1.
fn dense_header(
    view: &PaneView,
    transcript: Option<&Transcript>,
    workspace: Option<&WorkspaceBinding>,
    status: Option<Status>,
    root_chip: Option<AnyElement>,
) -> Div {
    let led_color = match status {
        Some(Status::Streaming) => GOOD,
        Some(Status::Blocked) => WAIT,
        Some(Status::Closed) => FAIL,
        Some(Status::Idle) => IDLE,
        None => INK_FAINT,
    };
    let binding = binding_label(workspace);
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
    // The root chip sits right after the title (#24's pinned design); the
    // binding meta keeps its place behind it.
    if let Some(chip) = root_chip {
        header = header.child(chip);
    }
    if !binding.is_empty() {
        header = header
            .child(div().flex_shrink_0().text_color(rgb(INK_FAINT)).child("·"))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(INK_TERTIARY))
                    .child(binding),
            );
    }
    header = header.child(div().flex_1());
    if let Some(todos) = transcript.and_then(|t| t.todos()) {
        header = header.child(
            div()
                .flex_shrink_0()
                .text_color(rgb(ACCENT))
                .child(meter(todos.done, todos.total)),
        );
    }
    let spend = transcript.map(spend_label).unwrap_or_default();
    if !spend.is_empty() {
        header = header
            .child(div().flex_shrink_0().text_color(rgb(INK_FAINT)).child("·"))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(rgb(INK_MUTED))
                    .child(spend),
            );
    }
    header
}

/// `▰▰▰▱ 3/4` while the glyph run stays glanceable; a long plan keeps the
/// fraction alone (an unbounded ▰ run would eat the header).
fn meter(done: usize, total: usize) -> SharedString {
    const GLYPH_CAP: usize = 8;
    if total == 0 {
        return SharedString::default();
    }
    if total <= GLYPH_CAP {
        let done = done.min(total);
        let mut run = String::new();
        run.extend(std::iter::repeat_n('▰', done));
        run.extend(std::iter::repeat_n('▱', total - done));
        return SharedString::from(format!("{run} {done}/{total}"));
    }
    SharedString::from(format!("{done}/{total}"))
}

/// `ctx 62% · $0.84` — the Dense header's textual instruments. Context
/// falls back to a token count when the provider reports no window.
fn spend_label(transcript: &Transcript) -> SharedString {
    let mut parts = Vec::new();
    if let Some(usage) = transcript.usage() {
        parts.push(match usage.context_window {
            Some(window) if window > 0 => {
                format!("ctx {}%", (usage.total_tokens * 100 / window).min(999))
            }
            _ => format!("ctx {}", tokens(usage.total_tokens)),
        });
    }
    if let Some(cost) = transcript.last_cost() {
        parts.push(format!("${cost:.2}"));
    }
    SharedString::from(parts.join(" · "))
}

fn body(
    view: &PaneView,
    transcript: &Transcript,
    visible: usize,
    selected: Option<std::ops::RangeInclusive<usize>>,
) -> impl IntoElement {
    let mut body = div()
        .id(("transcript", view.thread.get() as usize))
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
        .line_height(relative(theme::LINE_TRANSCRIPT));
    let blocks = transcript.blocks();
    let tail = blocks.len().saturating_sub(visible);
    for (offset, block) in blocks[tail..].iter().enumerate() {
        let picked = selected
            .as_ref()
            .is_some_and(|range| range.contains(&(tail + offset)));
        body = body.child(render_block(block, picked));
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
    queued: Option<&'a str>,
    running: bool,
    empty: bool,
    menu: Option<AnyElement>,
    mode: Option<&'a str>,
}

/// The PromptBox stack, top to bottom: permission card, queued row, the one
/// growing input line, meta row. Everything stacks above the line and is
/// driven by keys — no send button, no floating box. An open `/` or `@`
/// popover hangs above the whole stack; while a Decision pends the region
/// carries the `Decision` key context so y/n/a answer with the keyboard
/// still in the Composer (#23).
fn composer_region(view: &PaneView, transcript: &Transcript, stack: ComposerStack) -> Div {
    let ComposerStack {
        decision,
        queued,
        running,
        empty,
        menu,
        mode,
    } = stack;
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
    if let Some(decision) = decision {
        region = region.child(decision_card(decision));
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
                .child(placeholder(&view.name)),
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
            div().flex_shrink_0().text_color(rgb(ACCENT)).child("◐")
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
                .text_color(rgb(WAIT))
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
    // mode — and the model on the right where the comps put "fable-5 · max"
    // (run durations wait on a clock core deliberately does not keep —
    // sidebar-and-impl §4.2 #4).
    let mut meta = div()
        .flex()
        .flex_shrink_0()
        .items_center()
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
    if let Some(model) = transcript.model() {
        meta = meta.child(
            div()
                .text_size(px(theme::TEXT_CHIP))
                .text_color(rgb(INK_MUTED))
                .child(SharedString::from(model.to_string())),
        );
    }
    region.child(meta)
}

/// The idle line's ghost text, PromptBox state 01's pattern verbatim:
/// `message ‹thread-name› — hints`. The hints it advertises are the ones
/// this Composer actually answers.
fn placeholder(name: &SharedString) -> SharedString {
    SharedString::from(format!("message {name} — / commands · @ files · ↵ send"))
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
    let name_ink = if selected { ACCENT } else { INK_SECONDARY };
    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
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
    if selected {
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
/// smoke-rendered — on its own. The comp's warning-triangle SVG is stood in
/// by the ⚠ glyph the wall already speaks; gpui here has no asset pipeline
/// to load an icon from.
fn decision_card(decision: &Decision) -> Div {
    let command = decision_subject(decision);
    let subtitle = decision_wants(decision, "wants to run this");
    let mut card = div()
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
                .text_size(px(theme::TEXT_CODE))
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
        .child(div().flex_1())
        .child(keycap("y allow", INK, EDGE_STRONG))
        .child(keycap("n deny", INK_SECONDARY, EDGE_STRONG));
    if decision.standing_answer().is_some() {
        card = card.child(keycap("a always", INK_MUTED, EDGE));
    }
    card
}

/// The Decision's subject — what it wants to do: the description, else the
/// tool's name, else the honest unreadable fallback. Every surface that
/// names a Decision (L1 card, L2 cell, wall alert) goes through here.
fn decision_subject(decision: &Decision) -> SharedString {
    if !decision.description.is_empty() {
        SharedString::from(decision.description.clone())
    } else if !decision.tool_name.is_empty() {
        SharedString::from(decision.tool_name.clone())
    } else {
        SharedString::from("unreadable permission request")
    }
}

/// A Decision card's subtitle — who wants it, in the caller's phrasing —
/// or the unreadable fallback when the provider named no tool.
fn decision_wants(decision: &Decision, wants: &'static str) -> SharedString {
    if decision.tool_name.is_empty() {
        SharedString::from("the provider sent a request Ferrite could not read")
    } else {
        SharedString::from(format!("{} {wants}", decision.tool_name))
    }
}

/// One keyboard keycap as the comps draw it: mono 10 on RAISED, radius 4.
/// `a always` is de-emphasized by ink and a fainter border, never removed.
fn keycap(label: &'static str, ink: u32, edge: u32) -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(theme::TEXT_CHIP))
        .text_color(rgb(ink))
        .bg(rgb(RAISED))
        .border_1()
        .border_color(rgba(edge))
        .rounded(px(theme::R_CHIP))
        .px(px(6.))
        .py(px(2.))
        .child(label)
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

/// A plan's progress as a bar fill, 0..=1 — the one rule every progress
/// pill (wall and L2) shares.
fn plan_fraction(todos: Todos) -> f32 {
    (todos.done as f32 / todos.total.max(1) as f32).clamp(0.0, 1.0)
}

/// A progress pill: EDGE track, colored fill, radius 999.
fn bar(height: gpui::Pixels, fraction: f32, fill: u32) -> Div {
    div()
        .h(height)
        .w_full()
        .rounded_full()
        .bg(rgba(EDGE))
        .overflow_hidden()
        .child(
            div()
                .h_full()
                .w(relative(fraction.clamp(0.0, 1.0)))
                .rounded_full()
                .bg(rgb(fill)),
        )
}

/// A small status chip: 10px ink on a wash, radius 4.
fn chip(label: &'static str, ink: u32, wash: u32) -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(theme::TEXT_CHIP))
        .text_color(rgb(ink))
        .bg(rgba(wash))
        .rounded(px(theme::R_CHIP))
        .px(px(6.))
        .py(px(1.))
        .child(label)
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

// ------------------------------------------------- session project root (#24)

/// The open session-project-root selector: everything its popover draws,
/// discovered once when it opened — never per frame. The cockpit owns the
/// one live selector and its keys; the Pane only paints it.
pub struct RootSelector {
    pub thread: ThreadId,
    /// Row 0 is always the binding itself ("workspace root", clears the
    /// override); the rest are the discovered nested repositories, in
    /// `workspace::nested_repositories` order.
    pub options: Vec<RootOption>,
    /// The row the arrow keys are on.
    pub selected: usize,
    /// The row that was the Thread's root when the popover opened — the ✓.
    pub active: usize,
}

/// One pickable root. `None` is the binding itself: picking it clears the
/// override back to "work in the binding".
pub struct RootOption {
    pub root: Option<PathBuf>,
    pub label: SharedString,
}

/// A root as the operator reads it: relative to the binding wherever
/// possible; one from outside the binding (a hand-edited store) in full
/// rather than pretending. The chip and the popover's rows share this one
/// rule, so the two can never spell the same root differently.
pub fn root_display(binding_cwd: &Path, root: &Path) -> String {
    root.strip_prefix(binding_cwd)
        .unwrap_or(root)
        .display()
        .to_string()
}

/// The header chip naming where inside the binding this Thread's work
/// happens: `⌵ apps/web` under an override, `⌵ workspace` without one.
pub fn root_chip_label(binding: &WorkspaceBinding, root: Option<&Path>) -> SharedString {
    match root {
        Some(root) => SharedString::from(format!("⌵ {}", root_display(binding.cwd(), root))),
        None => SharedString::from("⌵ workspace"),
    }
}

/// The chip itself, per issue #24's pinned design: mono 10.5 in a quiet 1px
/// EDGE box — the accent tint stays on the provider chip, and two accent
/// chips in one header would fight. An override promotes the ink one step
/// so it reads at a glance; the hover wash says the chip answers clicks.
pub fn root_chip(label: SharedString, set: bool) -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(theme::TEXT_META))
        .text_color(rgb(if set { INK_TERTIARY } else { INK_FAINT }))
        .border_1()
        .border_color(rgba(EDGE))
        .rounded(px(theme::R_CHIP))
        .px(px(6.))
        .py(px(1.))
        .hover(|chip| chip.bg(rgba(EDGE)))
        .child(label)
}

/// Every popover's shell, in the comps' one popover language (PromptBox
/// state 02): RAISED surface, EDGE_STRONG border, radius 4, 4px padding,
/// and the three-layer popover elevation. Width is the caller's — the root
/// selector pins its own, the Composer menus span the composer. Rows and
/// footer are the cockpit's to append — their clicks are wired there.
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

/// The session-project-root selector's popover (#24): the shared shell at
/// its pinned width.
pub fn selector_popover() -> Div {
    popover_shell().w(px(theme::POPOVER_W))
}

/// One popover row: mono 12 name — ACCENT on the EDGE wash when the arrows
/// are on it, INK_SECONDARY otherwise — and the ✓ marking the root the
/// Thread is on right now, whichever row the arrows have moved to.
pub fn selector_row(option: &RootOption, selected: bool, active: bool) -> Div {
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
        .child(div().min_w_0().truncate().child(option.label.clone()));
    if selected {
        row = row.bg(rgba(EDGE));
    }
    if active {
        row = row.child(div().flex_1()).child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::TEXT_META))
                .text_color(rgb(ACCENT))
                .child("✓"),
        );
    }
    row
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
/// A selected Block carries the wash; whole Blocks are the selection unit
/// because gpui 0.2.2 has no character-level selection over rendered text.
fn render_block(block: &Block, selected: bool) -> AnyElement {
    let row = div()
        .w_full()
        .flex_shrink_0()
        .when(selected, |row| row.bg(rgba(SELECTION)));
    match &block.body {
        Body::Prompt(line) => gutter_row(row, "❯", ACCENT, true)
            .child(
                div()
                    .min_w_0()
                    .text_color(rgb(INK))
                    .child(SharedString::from(line.clone())),
            )
            .into_any_element(),
        Body::Paragraph { spans } => gutter_row(row, "⏺", INK_TERTIARY, false)
            .child(
                div()
                    .min_w_0()
                    .text_color(rgb(INK_SECONDARY))
                    .child(inline(spans)),
            )
            .into_any_element(),
        Body::Heading { spans, .. } => row
            .flex()
            .pl(px(theme::INDENT))
            .child(
                div()
                    .text_size(px(theme::TEXT_HEADING))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(INK))
                    .border_b_1()
                    .border_color(rgba(EDGE_STRONG))
                    .pb(px(3.))
                    .child(inline(spans)),
            )
            .child(div().flex_1())
            .into_any_element(),
        Body::Bullet { spans } => row
            .flex()
            .flex_row()
            .gap(px(6.))
            .pl(px(theme::INDENT))
            .text_color(rgb(INK_SECONDARY))
            .child(div().flex_shrink_0().text_color(rgb(ACCENT)).child("•"))
            .child(div().min_w_0().child(inline(spans)))
            .into_any_element(),
        Body::Thinking(thought) => row
            .text_color(rgb(INK_FAINT))
            .child(SharedString::from(thought.clone()))
            .into_any_element(),
        Body::Notice(text) => row
            .text_color(rgb(WAIT))
            .child(SharedString::from(text.clone()))
            .into_any_element(),
        Body::Meta(text) => row
            .text_size(px(theme::TEXT_ROW))
            .text_color(rgb(INK_MUTED))
            .child(SharedString::from(text.clone()))
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
                            .child(code(source, tokens.as_deref())),
                    ),
            )
            .into_any_element(),
        Body::Tool(tool) => render_tool(row, tool),
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
/// bold tool name, file args in accent, command args in prose ink.
fn render_tool(row: Div, tool: &ToolBlock) -> AnyElement {
    // Command runners' args read as prose; every other summary is a
    // path-like subject and takes the accent (the comps' file links,
    // rendered inert — opening files is not this pass).
    let arg_color = match tool.name.as_str() {
        "Bash" | "commandExecution" => INK_SECONDARY,
        _ => ACCENT,
    };
    let mut call = div().flex().min_w_0().text_color(rgb(INK_SECONDARY)).child(
        div()
            .flex_shrink_0()
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(INK))
            .child(SharedString::from(tool.name.clone())),
    );
    if !tool.summary.is_empty() {
        call = call
            .child(div().flex_shrink_0().child("("))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(arg_color))
                    .child(SharedString::from(tool.summary.clone())),
            )
            .child(div().flex_shrink_0().child(")"));
    }
    let mut card = row.flex().flex_col().gap(px(theme::TRANSCRIPT_GAP)).child(
        gutter_row(
            div().hover(|row| row.bg(rgba(HOVER))),
            "⏺",
            INK_TERTIARY,
            false,
        )
        .child(call),
    );
    if let Some(line) = &tool.result_line {
        card = card.child(
            div()
                .pl(px(theme::INDENT))
                .min_w_0()
                .truncate()
                .text_color(rgb(INK_MUTED))
                .child(SharedString::from(format!("⎿ {line}"))),
        );
    }
    if let ToolState::Failed(message) = &tool.state {
        card = card.child(
            div()
                .pl(px(theme::INDENT))
                .min_w_0()
                .truncate()
                .text_color(rgb(FAIL))
                .child(SharedString::from(format!("⎿ {message}"))),
        );
    }
    if let Some(diff) = &tool.diff {
        card = card.child(render_diff(diff));
    }
    card.into_any_element()
}

/// A bare diff, per DirectionDense: no card, no filename header — the tool
/// row above already names the file. 22px indent, a 30px right-aligned
/// number column, washes for added and removed rows.
fn render_diff(diff: &Diff) -> impl IntoElement {
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
                        .child(SharedString::from(line.clone())),
                ),
            );
        }
    }
    lines
}

/// A Block's text as the clipboard should carry it: what the row shows,
/// without colour or state glyphs. Exhaustive on purpose — a new Body kind
/// must decide what copying it means.
pub fn block_text(block: &Block) -> String {
    fn flat(spans: &[Span]) -> String {
        spans.iter().map(|span| span.text.as_str()).collect()
    }
    match &block.body {
        Body::Prompt(line) => format!("❯ {line}"),
        Body::Paragraph { spans } | Body::Heading { spans, .. } => flat(spans),
        Body::Bullet { spans } => format!("• {}", flat(spans)),
        Body::Thinking(text) | Body::Notice(text) | Body::Meta(text) => text.clone(),
        Body::Code { source, .. } => source.clone(),
        Body::Tool(tool) => {
            let mut lines = vec![format!("{} {}", tool.name, tool.summary)];
            if let Some(line) = &tool.result_line {
                lines.push(format!("⎿ {line}"));
            }
            if let ToolState::Failed(message) = &tool.state {
                lines.push(message.clone());
            }
            if let Some(diff) = &tool.diff {
                lines.push(format!("{} +{} −{}", diff.path, diff.added, diff.removed));
                lines.extend(
                    diff.hunks
                        .iter()
                        .flat_map(|hunk| hunk.lines.iter().cloned()),
                );
            }
            lines.join("\n")
        }
    }
}

/// Token counts read at a glance, not to the digit.
fn tokens(count: u64) -> String {
    match count {
        0..=999 => count.to_string(),
        1_000..=999_999 => format!("{:.1}k", count as f64 / 1_000.0),
        _ => format!("{:.1}m", count as f64 / 1_000_000.0),
    }
}

/// Markdown spans in one wrapping run, so inline code keeps its place in the
/// sentence instead of becoming its own box.
fn inline(spans: &[Span]) -> StyledText {
    let mut text = String::new();
    let mut highlights = Vec::new();
    for span in spans {
        let start = text.len();
        text.push_str(&span.text);
        if span.style == Style::Code {
            highlights.push((
                start..text.len(),
                HighlightStyle {
                    // The comps' inline-code chip: primary ink on RAISED.
                    color: Some(rgb(INK).into()),
                    background_color: Some(rgb(RAISED).into()),
                    ..Default::default()
                },
            ));
        }
    }
    StyledText::new(text).with_highlights(highlights)
}

/// Highlighted code, or plain code while the highlighter is still thinking.
fn code(source: &str, tokens: Option<&[Token]>) -> StyledText {
    let plain = || StyledText::new(SharedString::from(source.to_string()));
    let Some(tokens) = tokens else {
        return plain();
    };
    let mut highlights = Vec::new();
    let mut at = 0;
    for token in tokens {
        let end = at + token.text.len();
        // A highlighter that disagrees with the source is ignored, not trusted
        // into a panic.
        if end > source.len() {
            return plain();
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
    plain().with_highlights(highlights)
}

#[cfg(test)]
mod tests {
    use super::*;
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
            is_error: false,
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
    /// they are painting under, which a bare `cx.draw` does not have.
    struct ShowsBlocks {
        blocks: Vec<Block>,
    }

    impl Render for ShowsBlocks {
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
                .children(self.blocks.iter().map(|block| render_block(block, false)))
                // And once more selected, so the wash paints on every kind.
                .children(self.blocks.iter().map(|block| render_block(block, true)))
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
                .children(self.decisions.iter().map(decision_card))
                .children(self.decisions.iter().map(l2_decision_body))
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

    /// #24: the chip names the root relative to the binding — `⌵ apps/web`
    /// — and `⌵ workspace` when no override is set. A root from outside the
    /// binding (a hand-edited store) shows in full rather than pretending.
    #[test]
    fn the_root_chip_names_the_root_relative_to_the_binding() {
        let binding = WorkspaceBinding::Main {
            checkout: "/repo".into(),
        };
        assert_eq!(root_chip_label(&binding, None).as_ref(), "⌵ workspace");
        assert_eq!(
            root_chip_label(&binding, Some(Path::new("/repo/apps/web"))).as_ref(),
            "⌵ apps/web"
        );
        assert_eq!(
            root_chip_label(&binding, Some(Path::new("/elsewhere/api"))).as_ref(),
            "⌵ /elsewhere/api"
        );
    }

    /// The app is thin by design, so its render test is that every Block kind
    /// the core can produce actually lays out and paints in a window.
    #[gpui::test]
    fn every_block_kind_paints(cx: &mut TestAppContext) {
        let transcript = every_kind();
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

        let (_, cx) = cx.add_window_view(|_, _| ShowsBlocks { blocks });
        // A resize forces a real layout-and-paint pass through the view.
        cx.simulate_resize(size(px(900.), px(600.)));
        cx.run_until_parked();
    }

    /// AC2's copy half needs every Block kind to say what it is as text —
    /// an empty string here would copy as a silent hole.
    #[test]
    fn every_block_kind_has_clipboard_text() {
        let transcript = every_kind();
        for block in transcript.blocks() {
            assert!(
                !block_text(block).trim().is_empty(),
                "no clipboard text for {block:?}"
            );
        }
        let by_kind: Vec<String> = transcript.blocks().iter().map(block_text).collect();
        let all = by_kind.join("\n");
        assert!(all.contains("❯ run the tests"), "the prompt line: {all}");
        assert!(
            all.contains("fn main() {}"),
            "code copies its source: {all}"
        );
        assert!(
            all.contains("+delta") && all.contains("-bravo"),
            "a diff copies its lines: {all}"
        );
        // The ⎿ continuation the fold keeps copies too.
        assert!(all.contains("⎿ 42 passed"), "the result line: {all}");
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
        assert_eq!(empty.fraction, None);

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
        assert_eq!(card.fraction, Some(0.75));
        assert_eq!(card.working.as_ref(), "3/4 · ◐ working");

        // A red suite flips the folded flag.
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "test1".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo test" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "test1".into(),
            output: String::new(),
            is_error: true,
            result: ToolResult::Opaque,
        }));
        assert!(wall_card(Some(&transcript), None).tests_failing);

        // A finished turn's cost reaches the done line.
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(0.31),
        }));
        assert_eq!(
            wall_card(Some(&transcript), None).done.as_ref(),
            "✓ done · $0.31"
        );

        // A Decision's subject becomes the alert's second line.
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
            "gh issue close 212"
        );

        // A closed Session's reason becomes the blocked context line.
        let mut closed = Transcript::default();
        closed.apply(Input::Event(SessionEvent::Closed {
            reason: "claude CLI exited with code 1".into(),
        }));
        assert_eq!(
            wall_card(Some(&closed), None).context.as_ref(),
            "claude CLI exited with code 1"
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
        assert_eq!(decision_subject(&full).as_ref(), "gh issue close 212");
        assert_eq!(
            decision_wants(&full, "wants to run this").as_ref(),
            "Bash wants to run this"
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
            decision_wants(&unreadable, "wants approval to run").as_ref(),
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

    /// The Dense header's ▰▱ meter stays glanceable: glyphs for small plans,
    /// the bare fraction for long ones, and done never overshoots.
    #[test]
    fn the_todo_meter_caps_its_glyph_run() {
        assert_eq!(meter(3, 4).as_ref(), "▰▰▰▱ 3/4");
        assert_eq!(meter(0, 2).as_ref(), "▱▱ 0/2");
        assert_eq!(meter(9, 20).as_ref(), "9/20");
        assert_eq!(meter(5, 4).as_ref(), "▰▰▰▰ 4/4");
        assert_eq!(meter(0, 0).as_ref(), "");
    }
}
