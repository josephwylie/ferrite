//! The cockpit window: every open Pane at once, and the one pump behind them.
//!
//! Rendering and keys only. What each Pane shows — the Blocks, the pending
//! Decision, the held prompt — is folded in core and read from there.

pub(crate) mod beside;
pub(crate) mod decisions;
pub(crate) mod empty_board;
mod leaving;
mod machines;
mod palette;
pub(crate) mod panel;
mod parts;
mod pr_tab;
#[cfg_attr(not(unix), path = "cockpit/projects_unavailable.rs")]
mod projects;
mod server;
pub(crate) mod subagents;
mod transcript_glue;
mod updating;
mod user_commands;
mod view_head;

/// Card triggers' bounds by key, recorded in prepaint.
type FloatTriggers =
    std::rc::Rc<std::cell::RefCell<std::collections::HashMap<SharedString, gpui::Bounds<Pixels>>>>;

use ferrite_ui::components::Tip as _;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "visual-reference")]
#[path = "visual_reference.rs"]
pub mod visual_reference;

use std::time::Duration;

use ferrite_core::cockpit::{CloseError, Cockpit, HistoryDirection, ProviderChoice, ThreadView};
use ferrite_core::docview::{Cell, Level};
use ferrite_core::draft::DraftTarget;
use ferrite_core::followup;
use ferrite_core::groups::{Drag, DropTarget, GroupChange, GroupId, Groups, Plan};
use ferrite_core::layout::{self, Edge, SeamId, Tree, Zone};
use ferrite_core::roster::{PaneIdentity, View};
use ferrite_core::settings::ThreadListOrder;
use ferrite_core::sound::Sound;

/// The loose rows the nav last drew, in order, and under `sort: attention`
/// the state each row was ranked by (C8, "never re-sort under the
/// pointer"): while the pointer is inside the column or a drag is live,
/// the tree keeps this order, and Groups and sections keep the ranks they
/// were sorted by. It re-ranks on pointer-leave, a focus change, or a
/// Thread arriving or leaving — any of which changes `focused` or the row
/// set, so the snapshot no longer applies. Creation order never moves a
/// row, so it has nothing to hold.
struct NavSnapshot {
    rows: Vec<ThreadId>,
    focused: Option<ThreadId>,
    ranks: std::collections::HashMap<ThreadId, Attention>,
}

/// Where the Attention list listed a Thread: its section, and its newest
/// Notice then (`CockpitView::attention_list`).
type Listed = (
    nav::ListSection,
    Option<ferrite_core::notifications::NoticeId>,
);

/// Reorder `rows` by where they sat in `snapshot`, when the snapshot holds
/// exactly the same Threads; `false` (and `rows` untouched) otherwise.
fn hold_order(rows: &mut [nav::ThreadRow], snapshot: &[ThreadId]) -> bool {
    if rows.len() != snapshot.len() {
        return false;
    }
    let position = |row: &nav::ThreadRow| snapshot.iter().position(|held| *held == row.thread);
    if rows.iter().any(|row| position(row).is_none()) {
        return false;
    }
    rows.sort_by_key(position);
    true
}

/// A fold the nav's triangles hold (N-4): a Project section shut to its
/// heading, a Group shut to its row, the Parked section shut to its header.
/// In memory only; a re-render or a view switch keeps it.
pub(crate) use crate::nav::NavFold;

/// The action ⌘`ordinal` is bound to: the shown board's `ordinal`th Pane in
/// head order (R5, `CockpitView::board_order`; Solo's ⌘1 is the Solo Pane).
pub(crate) fn focus_rail_action(ordinal: usize) -> Option<&'static str> {
    [
        "cockpit::FocusThread1",
        "cockpit::FocusThread2",
        "cockpit::FocusThread3",
        "cockpit::FocusThread4",
        "cockpit::FocusThread5",
        "cockpit::FocusThread6",
        "cockpit::FocusThread7",
        "cockpit::FocusThread8",
        "cockpit::FocusThread9",
    ]
    .get(ordinal.checked_sub(1)?)
    .copied()
}

pub(crate) use ferrite_core::Attention;

use ferrite_core::store::Provider;
use ferrite_core::transcript::plain_text;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::workspace::RepoListing;
#[cfg(test)]
use ferrite_core::workspace::WorkspaceChoice;
use ferrite_core::{DecisionAnswer, ThreadId};
use gpui::prelude::*;
use gpui::{
    actions, anchored, deferred, div, px, rgb, rgba, AnyElement, ClickEvent, ClipboardItem,
    Context, Div, Entity, EventEmitter, FocusHandle, Focusable, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollHandle, SharedString,
    Stateful, Window,
};

use crate::facts::Facts;
use crate::nav;
use crate::notifications::{Handle, NeedsYou, Row as NoticeRow, RowTarget, Verb};
use ferrite_pane::composer::{Composer, Edited};
use ferrite_pane::decision;
use ferrite_pane::pane::{self, PaneView};
use ferrite_ui::menu;
// Declared with the Pane, whose Group head dispatches it.
use crate::prefs;
use crate::project_editor;
pub(crate) use ferrite_pane::pane::NextDecision;
use ferrite_pane::select::TranscriptText;
use ferrite_ui::pointer::{Pointer, PointerPressed};

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
        NewThread,
        NewWorktreeThread,
        NewGroup,
        TextLarger,
        TextSmaller,
        TextReset,
        BandCycle,
        ToolCyclePrevious,
        ToggleTool,
        CloseThread,
        ReopenThread,
        CopySelection,
        Paste,
        PickOption1,
        PickOption2,
        PickOption3,
        PickOption4,
        OpenSettings,
        ToggleFullscreen,
        ToggleNav,
        TogglePanel,
        ToggleNotifications,
        MenuNext,
        MenuPrevious,
        MenuPick,
        MenuDismiss,
        HistoryOlder,
        HistoryNewer,
        FocusThread1,
        FocusThread2,
        FocusThread3,
        FocusThread4,
        FocusThread5,
        FocusThread6,
        FocusThread7,
        FocusThread8,
        FocusThread9,
        RefreshPrStatus,
        OpenPrInBrowser,
    ]
);

/// How often the pump drains every Session. One timer for the whole cockpit,
/// not one per Pane: 24 Panes must cost one frame, not 24. 8ms, because 16
/// capped the cockpit at 60fps on a 120Hz display — half the refresh rate,
/// and the operator sees the difference in a scroll. Measured on the perf
/// fixture (release, `--load --panes 4`): 16ms pins at exactly 60.0fps and
/// 42% CPU, 8ms reaches 119fps at 55%, and 4ms also reaches 119 — the
/// display, not the pump, is the ceiling from 8ms down.
fn pump_interval() -> Duration {
    let ms = std::env::var("FERRITE_PUMP_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(PUMP_MS);
    Duration::from_millis(ms)
}

/// The label for a checkout git cannot name — the branch a fresh clone
/// would be sitting on. Never a menu row of its own: the workspace chip
/// offers `main` only where git lists it, like any other branch.
const MAIN_BRANCH: &str = "main";

const PUMP_MS: u64 = 8;

/// A Thread's system notification tag: a newer one for the same Thread
/// replaces it in the notification centre. A Thread on another machine is
/// named by it too, since ids are per Store, whichever machine the window
/// is on: the tag outlives a move.
fn system_tag(machine: &crate::machines::MachineKey, thread: ThreadId) -> String {
    match &machine.0 {
        None => format!("ferrite-thread-{thread}"),
        Some(target) => format!("ferrite-thread-{thread}@{target}"),
    }
}

type FreshNotice = (ferrite_core::notifications::NoticeId, ThreadId, bool);
type LiveRequest = (ferrite_core::notifications::DecisionNoticeId, bool);

/// What a machine's Needs-you state may not have heard yet: its Notices born after
/// `after` (id, Thread, read) and its live requests (id, read).
fn news(
    notifications: &ferrite_core::notifications::Notifications,
    after: Option<ferrite_core::notifications::NoticeId>,
) -> (Vec<FreshNotice>, Vec<LiveRequest>) {
    let fresh = notifications
        .since(after)
        .map(|notice| (notice.id, notice.thread, notice.read))
        .collect();
    let requests = notifications
        .decisions()
        .map(|notice| (notice.id.clone(), notice.read))
        .collect();
    (fresh, requests)
}

/// How many frames a card waits for its geometry (`card_corner`).
/// A Pane laid out for the first time has it by the next frame; a few more
/// cover a layout that settles over two.
const CARD_GEOMETRY_FRAMES: u8 = 3;
const TUNING_BUSY_HINT: &str = "Available when this turn finishes";
const TUNING_PENDING_HINT: &str = "Available once the last change is accepted";

/// Why a Thread's model and effort cannot change now, in the words the
/// pickers show; `None` while they can.
fn tuning_hold(open: &ThreadView) -> Option<&'static str> {
    if open.busy() {
        Some(TUNING_BUSY_HINT)
    } else if open.tuning() {
        Some(TUNING_PENDING_HINT)
    } else {
        None
    }
}

/// One Pane as the Cockpit holds it: its view, and what the Cockpit keeps
/// beside it. Kept with the Pane, so a draft's first send — which makes the
/// Pane a Thread's — keeps it as it was.
pub(crate) struct CockpitPane {
    view: PaneView,
    pub(crate) tab_interaction: subagents::TabInteraction,
    pub(crate) request_forms: subagents::RequestForms,
    /// What the Panel keeps of this Pane's preview (`panel/preview.rs`): its
    /// scroll, its find bar, whether its tab is the one shown.
    pub(crate) preview_tab: panel::preview::PreviewTab,
    /// The Review beside this Pane on the board (`beside.rs`).
    pub(crate) review: beside::ReviewSlot,
}

impl CockpitPane {
    fn new(view: PaneView, cx: &mut gpui::App) -> Self {
        Self {
            view,
            tab_interaction: Default::default(),
            request_forms: Default::default(),
            preview_tab: Default::default(),
            review: beside::ReviewSlot::new(cx),
        }
    }
}

impl std::ops::Deref for CockpitPane {
    type Target = PaneView;
    fn deref(&self) -> &PaneView {
        &self.view
    }
}

impl std::ops::DerefMut for CockpitPane {
    fn deref_mut(&mut self) -> &mut PaneView {
        &mut self.view
    }
}

pub struct CockpitView {
    cockpit: Cockpit,
    /// One view per Pane of the Cockpit's roster, in its order — the
    /// keyboard, the scrollback and the caches the window owns for each.
    /// `sync_panes` is the one door: after every act on the roster the
    /// mirror is reconciled, never edited by hand.
    panes: Vec<CockpitPane>,
    /// The cockpit's own place in the focus tree. Key dispatch walks from the
    /// focused node up to the root, so with nothing focused inside the window
    /// the cockpit's own actions are never reached — at wall range, where no
    /// Pane holds a Composer, this handle is what keeps the keyboard alive.
    focus: FocusHandle,
    perf: Option<Perf>,
    /// When the watchdog last swept, on the executor's clock (the wall
    /// clock in the app, the fake one under test, so a loaded test machine
    /// cannot drop a sweep into the middle of a test). Measurements are
    /// cached from the RSS worker, so a sweep never waits for an
    /// operating-system query.
    swept: std::time::Instant,
    /// The nav and each Pane as cached parts (`parts`), and what notifies
    /// the frame around them alone.
    parts: std::cell::RefCell<parts::Parts>,
    frame_tick: Entity<parts::FrameTick>,
    /// When the next nav age (`2m`) turns over, as of the last draw: the
    /// sweep redraws an otherwise quiet window then.
    ages_turn_at: Option<std::time::SystemTime>,
    /// When the frame's own clock text (the bottom bar's minute, a
    /// notice's age) next turns over, as of the last draw.
    frame_turn_at: Option<std::time::SystemTime>,
    /// Free bytes on the volume holding the Threads' checkouts, as last
    /// read (the bar's `N GB free`); `None` until read, or when unknown.
    free_disk: Option<u64>,
    /// When free disk was last asked for, on the executor's clock: at
    /// launch, then by the sweep once a minute (`read_free_disk`).
    free_disk_asked: Option<std::time::Instant>,
    /// One checkout-label refresh at a time, always off the UI thread.
    branch_refreshing: bool,
    /// A moment that must not be skipped (a turn ended) asked for a
    /// refresh while one was under way: another follows it.
    branch_refresh_queued: bool,
    /// The branch each Project's checkout is on, as git last answered —
    /// what a draft's workspace chip names every frame. Never read from a
    /// frame: a Project the chip has not named yet, every draft's Project
    /// on each branch refresh, and the Project whose band popover opens
    /// are all asked about off the UI thread (`look_up_checkouts`), and the
    /// last answer stands until the new one lands. On a machine, its Server
    /// answers each draft's Project once (`list_repo`).
    checked_out: std::cell::RefCell<std::collections::HashMap<ProjectId, SharedString>>,
    /// Projects whose checkout is being asked about: one lookup each at a
    /// time, however many frames name it meanwhile.
    checking_out: std::cell::RefCell<std::collections::HashSet<ProjectId>>,
    /// What git lists for a draft's chosen repo — its other worktrees and
    /// its local branches — read off the UI thread when the workspace
    /// popover opens and kept by project: the rows show the last answer
    /// at once and take the fresh one when it lands. Never read per frame.
    repo_listing: Option<(ProjectId, RepoListing)>,
    /// On a machine, the listing its Server is reading for the popover or
    /// a draft's chip.
    asking_repo: Option<(
        ProjectId,
        std::sync::mpsc::Receiver<std::io::Result<RepoListing>>,
    )>,
    /// Stable text-run identities; selection itself belongs to GPUI.
    selection: TranscriptText,
    native_copy: Option<String>,
    /// cmd-b (#21): the nav folded to its rail by the operator. In memory
    /// only; the Settings preference is where a launch starts from.
    nav_collapsed: bool,
    /// The window is too narrow for the full column beside the board
    /// (rule 2.7.7, `update_auto_rail`): the nav draws as the rail, and
    /// nothing is saved. Held with `NAV_AUTO_RAIL_HYSTERESIS`.
    nav_auto_rail: std::cell::Cell<bool>,
    /// cmd-B while the window holds the nav folded: the operator's override
    /// of the narrow-window rail, kept for the session (another cmd-B while
    /// narrow gives the rail back). Never saved.
    nav_forced_open: bool,
    /// Measuring what the board would be beside the full column
    /// (`update_auto_rail`): `nav_width` answers the full width meanwhile.
    nav_measure_full: std::cell::Cell<bool>,
    /// The pointer is inside the nav column: its order holds still.
    nav_hovered: bool,
    /// A drag is live this frame: the nav's order holds still.
    nav_drag_live: std::cell::Cell<bool>,
    /// The order the nav last drew, held while the pointer is inside.
    nav_snapshot: std::cell::RefCell<Option<NavSnapshot>>,
    /// The section the Attention list last listed each Thread under, and
    /// the Thread's newest Notice then (`attention_list`): what says the
    /// focused Thread finished and is held under `Done`.
    nav_listed: std::cell::RefCell<std::collections::HashMap<ThreadId, Listed>>,
    /// The column's width on its way between its two (`motion::RESIZE`).
    /// None on launch so a restored preference never performs entrance
    /// choreography; once the operator acts, a flip mid-flight retargets
    /// from the width on screen. `nav_collapsed` stays authoritative.
    nav_tween: Option<ferrite_ui::motion::Tween>,
    /// The column's content opacity on its way between 1 and 0
    /// (`nav::CONTENT_FADE`, 150ms CSS ease), on its own clock apart from the
    /// width's ride. None on launch, like `nav_tween`.
    nav_fade: Option<ferrite_ui::motion::Tween>,
    /// What the nav's folds hold shut (N-4): Projects and Groups. In
    /// memory only.
    nav_folds: std::collections::HashSet<NavFold>,
    /// What the nav and the Pane head say about a Thread beyond an O(1)
    /// read — checkout, Project, a parked row's provider, the L3 card —
    /// refreshed by moment, never per frame.
    facts: Facts,
    /// The one Project filter navigation has (#29), chosen from the ⌘K
    /// palette. `None` is every Project. It narrows the **navigation only**
    /// to that Project's section — which Panes the Cockpit flies is never
    /// touched by it.
    nav_filter: Option<ProjectId>,
    /// The nav tree's scroll, shared with the hand-drawn scrollbar beside
    /// it — gpui 0.2.2 paints none of its own.
    nav_scroll: ScrollHandle,
    /// What the nav shows under its views (§5.1.2): the tree, or a view's
    /// list in its place. The tree on every launch.
    pub(crate) nav_view: nav::NavView,
    /// The keyboard of a view's list other than the Needs-you one (whose
    /// keys are the Needs-you view's, `Floats::notice_focus`): Esc returns to the
    /// tree.
    nav_view_focus: FocusHandle,
    /// The rail's own scroll (cmd-B folded).
    nav_rail_scroll: ScrollHandle,
    /// The window's activation, followed into the Cockpit
    /// (`Cockpit::set_window_active`) once the first frame has a window.
    activation: Option<gpui::Subscription>,
    /// The one popover in the Composer's slot, or None: the `/`/`@` menu
    /// (#23), a picker (#11, #25) or a band chip (#29). Always on the
    /// focused Pane's Composer; render self-heals it shut when the operator
    /// leaves that Pane, zooms below L1, or its offer expires.
    popover: Option<Popover>,
    /// Escape (or a press elsewhere) dismissed the menu: stay shut until
    /// the text moves again, or `sync_menu` would reopen it on the very
    /// text the operator dismissed it over.
    menu_muted: bool,
    draft_commands: Option<DraftCommands>,
    /// A recalled slash/mention prompt is a programmatic edit: consume its
    /// `Edited` event without deriving a menu. The next operator edit clears
    /// the ordinary `menu_muted` latch and derives again.
    suppress_recall_menu_once: bool,
    /// Where the vendors keep session files — discovery's roots, defaulted
    /// to the real homes and aimed at scratch directories by tests. Read
    /// once per picker open, never per frame.
    session_file_roots: Vec<(Provider, std::path::PathBuf)>,
    discovery_request: u64,
    pending_files: Option<PendingFileSearch>,
    pending_discovery: Option<PendingDiscovery>,
    /// The Project new drafts fall back to: the last one the operator was
    /// working in. The launch reads it from the store's most recently used
    /// Thread, and every explicit pick in the Composer's Project chip moves
    /// it, so the next draft opens where the last one ran. Startup only
    /// selects an already registered Project; it must never turn the app's
    /// process directory into user workspace state.
    launch_project: Option<ProjectId>,
    /// The inline title rename in flight, if any: what is being renamed,
    /// and the one-line Composer standing in for its title. At most one —
    /// the title cell *is* the editor, so two at once would need two rows
    /// to be the row you are looking at.
    rename: Option<(RenameTarget, Entity<ferrite_pane::composer::Composer>)>,
    /// The right-click menu, if one is up: what it is about, where it was
    /// summoned, and which destructive row is armed for its second press.
    context_menu: Option<ContextMenu>,
    /// The Solo view's right panel (`panel`): its tab per Thread and
    /// whether the window has room for it.
    panel: panel::Panel,
    /// The panel's PR tab (`pr_tab`): each Thread's last snapshot, its
    /// reads' pacing, and what the operator unfolded.
    pr_tab: pr_tab::PrTab,
    /// Whether the panel's Context tab has its legend open. Kept across
    /// openings: an operator who wants the breakdown wants it every time.
    context_usage_expanded: bool,
    session_controls: Option<(ThreadId, u64)>,
    /// The Composer's mode menu, open on one Thread's Session generation.
    mode_picker: Option<(ThreadId, u64)>,
    session_control_error: Option<(ThreadId, u64, String)>,
    /// The header `ci` mark's checks card, tied to its Thread (#29). The runs it lists are read from the same cached
    /// `BranchStatus` the mark was drawn from, so the card can never
    /// disagree with the mark that opened it.
    context_checks: Option<ThreadId>,
    /// Each bar card trigger's bounds as last laid out (a draft's band
    /// chips), recorded in prepaint: what a bar card hangs from.
    float_triggers: FloatTriggers,
    /// Frames each open card has waited for its trigger's or Pane's
    /// geometry, by trigger key (`card_corner`); none once it has some.
    card_waits: std::cell::RefCell<std::collections::HashMap<String, u8>>,
    /// A seam being dragged: the board, the seam, and the tree as it
    /// stands mid-drag — persisted on release, never per move.
    seam_drag: Option<SeamDrag>,
    /// The board's bounds as the last frame laid it out: what the default
    /// grid of a Group with no stored tree is computed in when an act (a
    /// seam press, a drop) has no window to measure.
    board: std::cell::Cell<layout::Rect>,
    /// The board's one Level (`board_level`), held with 24px of hysteresis
    /// so a resize hovering at a threshold never flickers the tier.
    board_level: std::cell::Cell<Level>,
    /// A Pane (or reader, or nav row) being dragged over a board slot: the
    /// target leaf and what a release there would do, for the preview wash.
    drop_preview: Option<(ThreadId, Zone)>,
    /// The Pane native files were last dragged over. Read only while a drag
    /// is live (`drop_target`), so a drag that leaves the window leaves no
    /// sheet behind.
    file_drop_over: Option<PaneIdentity>,
    /// Boards whose arrangement includes a reader slot, as the operator
    /// left them. Session-only: the persisted Group layout names Threads
    /// alone, so a reader's place is remembered here until quit.
    board_layouts: std::collections::HashMap<Board, Tree>,
    /// Every board slot's size as this frame laid it out, for the ghost a
    /// drag of it shows. Written while the board draws.
    slot_sizes: std::cell::RefCell<std::collections::HashMap<ThreadId, (f32, f32)>>,
    /// The board slot being dragged: it dims in place while its ghost
    /// moves, until the release.
    dragging: Option<ThreadId>,
    /// The nav row being dragged, dimmed in the tree the same way.
    nav_dragging: Option<Drag>,
    /// The operator's settings and where they save; every change saves.
    prefs: Preferences,
    /// The Settings panel is up.
    settings_open: bool,
    /// The Settings page the sidebar has selected; each opening starts on
    /// the first.
    settings_page: prefs::PageKey,
    /// The Settings search field, made with the sheet (it needs a window)
    /// and dropped with it, so each opening starts with an empty query.
    settings_search: Option<Entity<gpui::component::input::InputState>>,
    /// The About path copied last, shown with a check until the sheet
    /// closes.
    settings_copied: Option<&'static str>,
    /// The Project card: creating a Project, or editing the one the nav
    /// filter names. `None` is closed.
    project_editor: Option<ProjectEditor>,
    project_editor_focus: FocusHandle,
    settings_focus: FocusHandle,
    /// Whether the window is maximized, read once per frame in `render`.
    /// The nav's band is drawn without a `Window` in hand, and the two
    /// halves of the titlebar have to agree: a maximized window has no top
    /// resize edge for the drag regions to leave alone (`titlebar.rs`).
    maximized: bool,
    /// The CLIs' versions as `--version` reports them, probed once when the
    /// panel first opens: (claude, codex).
    cli_versions: Option<(SharedString, SharedString)>,
    /// A CLI version probe is in flight (the empty board asks once).
    cli_probing: bool,
    /// The version the empty board's banner names: this build's
    /// (`CARGO_PKG_VERSION`). The parity capture names the release the
    /// prototype's world runs (`0.5.0`).
    pub(crate) shown_version: SharedString,
    /// Where each provider CLI stands against its newest release.
    cli_updates: crate::cli_updates::CliUpdates,
    /// Where Ferrite's own update stands (ADR 0023).
    ferrite_update: crate::ferrite_update::FerriteUpdate,
    /// What the window has said about its Server (ADR 0025).
    server_bands: server::ServerBands,
    /// Whether the window has said `settings.json` could not be used.
    unreadable_told: bool,
    /// The operator's commands running, and what they and the keys have
    /// still to say.
    user_commands: user_commands::UserCommands,
    projects: projects::ProjectsState,
    /// Whether "Quit Ferrite?" is on screen (`leaving`).
    leave_asked: bool,
    group_error: Option<SharedString>,
    /// The Needs-you view: whether its list is down, its cursor, and which Notices
    /// and requests stand as toasts. The Notices themselves are core's.
    pub(crate) needs_you: NeedsYou,
    /// What the toast float drew last: the toast standing and the one
    /// fading out (`toast_float`).
    toast_stage: std::cell::RefCell<crate::notifications::ToastStage>,
    /// What to open once the Cockpit is attached: what the window came
    /// to this machine for (ADR 0026), and until when it waits.
    landing: Option<(Landing, std::time::Instant)>,
    /// The chimes that would have played (`chime`).
    #[cfg(test)]
    pub(crate) chimed: Vec<Sound>,
    /// The Thread the pump last saw focused: focusing another asks for its
    /// PR (`check_pr`).
    pr_focus: Option<ThreadId>,
    /// The PR reads `check_pr` would have sent `gh`: (Thread, manual).
    #[cfg(test)]
    pub(crate) pr_asked: Vec<(ThreadId, bool)>,
    /// The PR tab's snapshot reads, by Thread, fixture or not.
    #[cfg(test)]
    pub(crate) pr_tab_reads: Vec<ThreadId>,
    /// The palette, the shortcuts sheet and the notifications list's focus
    /// (`cockpit::palette`).
    floats: palette::Floats,
    /// Transcript events are detached subscriptions, registered once per
    /// retained Subject entity rather than once per render.
    transcript_entities: std::collections::HashSet<gpui::EntityId>,
    /// WP-F: the Decision cards' way back to this view, and the thread
    /// rules option 2 made (`cockpit::decisions`).
    decisions: decisions::DecisionState,
    // ---- the frame's (F-12 … F-15)
    /// The sidebar column's width this frame while cmd-B rides it (F-12):
    /// the board's left edge reads it every frame. `None` at rest.
    nav_ride: std::cell::Cell<Option<f32>>,
    /// Measuring at the ride's destination (`board_level`), so a Level never
    /// flaps mid-ride — the nav's or the panel's (`board_right`).
    nav_measure_rest: std::cell::Cell<bool>,
    /// A scene holding the ride at a progress (`fixture_hold_nav`).
    nav_hold: Option<f32>,
    /// Where Ferrite was launched: the empty board's path line (F-13).
    launch_dir: Option<std::path::PathBuf>,
    /// The empty board shown while Threads stay open (`show_empty_board`).
    empty_shown: bool,
    /// The empty board's selected row, and the list's own focus.
    empty_cursor: usize,
    empty_focus: FocusHandle,
    /// A Thread opened beside the Solo Pane (`open_beside(Beside::Thread)`):
    /// (the Pane it opened from, the Thread beside it).
    solo_beside: Option<(PaneIdentity, ThreadId)>,
    /// The plan card (`open_plan`), tied to its Thread.
    plan_card: Option<ThreadId>,
    /// The Pane whose preview took the last press: ⌘F finds there. A
    /// press hands the keyboard back to the Pane, so focus cannot say it.
    preview_pressed: Option<PaneIdentity>,
    /// Each Thread's reader slot (its Review) sliding open or shut
    /// (`beside.rs`).
    reader_slides: beside::ReaderSlides,
    /// The Provider an empty board's first draft starts on.
    launch_provider: Provider,
    /// The machines this window can be on (ADR 0026); `None` in a window
    /// on an Engine of its own.
    machines: Option<Entity<crate::machines::Machines>>,
    /// Settings › Machines' field for a new machine's ssh target.
    machine_field: Option<Entity<gpui::component::input::InputState>>,
}

/// The Cockpit asks its window to put it on machine `index` (ADR 0026),
/// and there to open `open`.
pub struct GoToMachine {
    pub index: usize,
    pub open: Option<Landing>,
}

/// What a window goes to a machine for: an alert about one of its
/// Threads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Landing {
    Alert(crate::notifications::Alert),
}

impl EventEmitter<GoToMachine> for CockpitView {}

/// What a card hangs from (`card_corner`): its Pane's Composer (opening
/// up) or head (opening down).
#[derive(Clone, Copy)]
enum CardFrom {
    Composer(PaneIdentity),
    Head(PaneIdentity),
}

/// What an inline rename is aimed at. Both are titles the operator owns:
/// a Group's, and a Thread's own display name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenameTarget {
    Group(GroupId),
    /// A Thread, renamed from its nav row.
    Thread(ThreadId),
    /// A Thread, renamed from its Pane head — the same title, a different
    /// editor site, so the two never draw one editor twice.
    PaneTitle(ThreadId),
    /// A Thread, renamed from the Solo view head (`view_head`).
    ViewTitle(ThreadId),
}

/// What the window is handed at launch beyond the core: the operator's
/// settings, where they are saved, and the Session defaults the spawner
/// reads — shared, so a change here applies to the next Session.
pub struct Preferences {
    pub settings: ferrite_core::settings::Settings,
    pub dir: std::path::PathBuf,
    pub defaults: std::sync::Arc<std::sync::Mutex<ferrite_core::spawner::SessionDefaults>>,
    /// Whether a model writes Thread titles (each Provider's own CLI, the
    /// cheap model, one turn). False in the test and demo constructors,
    /// so no suite ever spawns a real CLI for a name.
    pub titler: bool,
    /// Whether the window checks for newer provider CLIs and installs
    /// them. False in the test and demo constructors, so no suite reaches
    /// the registry or runs an installer.
    pub cli_updates: bool,
    /// Whether the window checks for a newer Ferrite and offers to restart
    /// onto it (`ferrite_update::enabled`). False in the test and demo
    /// constructors, so no suite reaches GitHub or stages an installer.
    pub ferrite_updates: bool,
    /// The release this launch installed, whose notes What's New shows
    /// once (`ferrite_update::at_launch`).
    pub whats_new: Option<ferrite_core::self_update::Staged>,
    /// Why `settings.json` could not be used, when it could not: the
    /// window says so, and saves nothing over the file (`change_settings`).
    pub unreadable: Option<String>,
    /// The keys in `settings.json` the launch refused, a sentence each
    /// (`keymap::with_user_keys`): the window says them once.
    pub keys_refused: Vec<String>,
    /// Whether the board opens and the nav and the bottom bar read it
    /// (ADR 0031). False in the test and demo constructors, so no suite
    /// reads the operator's own.
    pub board: bool,
}

impl Preferences {
    /// Defaults saved nowhere — the test constructor's and the demo's.
    #[cfg_attr(not(any(test, feature = "visual-reference")), allow(dead_code))]
    fn ephemeral() -> Self {
        Self {
            settings: ferrite_core::settings::Settings::default(),
            dir: std::env::temp_dir().join(format!("ferrite-prefs-{}", std::process::id())),
            defaults: std::sync::Arc::new(std::sync::Mutex::new(
                ferrite_core::spawner::SessionDefaults::default(),
            )),
            titler: false,
            cli_updates: false,
            ferrite_updates: false,
            whats_new: None,
            unreadable: None,
            keys_refused: Vec::new(),
            board: false,
        }
    }
}

/// A seam mid-drag.
struct SeamDrag {
    board: Board,
    seam: SeamId,
    tree: Tree,
}

/// A board the Panes lay out on by a split tree: a Group's, or Solo's
/// while its Pane has a reader open beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Board {
    Group(GroupId),
    Solo(PaneIdentity),
}

/// What one leaf of a board's tree stands for: a Pane, or the reader slot
/// a Thread's Pane has open, its Review (one per Pane).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Pane(PaneIdentity),
    Reader(ThreadId),
}

/// A board slot on the move: its head is the handle. Dropped on another
/// slot of the same board it swaps with it (the centre) or splits its
/// slot (an edge). `leaf` is the slot's leaf in the board's tree.
#[derive(Clone, Copy, Debug)]
struct PaneDrag {
    leaf: ThreadId,
}

/// What a dragged slot's ghost draws: its name, the line under it (a
/// Pane's branch, a reader's kind), and the size the slot has on the board.
#[derive(Clone)]
struct GhostFace {
    title: SharedString,
    detail: Option<SharedString>,
    reader: bool,
    size: (f32, f32),
}

/// The longest a ghost gets on either side: a big Pane shrinks to a card
/// the board stays readable around, keeping the slot's proportions.
const GHOST_MAX_W: f32 = 300.0;
const GHOST_MAX_H: f32 = 200.0;
/// A ghost's skeleton lines, as shares of its body's width.
const GHOST_LINES: [f32; 4] = [0.82, 0.64, 0.74, 0.46];

/// The ghost that follows the pointer while a slot is dragged: a scaled,
/// see-through miniature of the slot, held at the point it was grabbed.
struct PaneGhost {
    face: GhostFace,
    /// Where the pointer was in the grabbed handle, which gpui keeps under
    /// the pointer. The miniature is shifted so the same point of the
    /// scaled slot sits there instead.
    grab: gpui::Point<Pixels>,
    /// A nav drag's ghost folds back to its row while the pointer is over
    /// the nav (left of this x), so it never hides the rows it is being
    /// dropped among.
    row_left_of: Option<(f32, NavChip)>,
}

impl PaneGhost {
    fn scale(&self) -> f32 {
        let (w, h) = self.face.size;
        if w <= 0.0 || h <= 0.0 {
            return 1.0;
        }
        (GHOST_MAX_W / w).min(GHOST_MAX_H / h).min(1.0)
    }
}

impl Render for PaneGhost {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use ferrite_ui::theme;
        if let Some((_, chip)) = self
            .row_left_of
            .as_ref()
            .filter(|(edge, _)| f32::from(window.mouse_position().x) < *edge)
        {
            return chip.element(cx);
        }
        let scale = self.scale();
        let (w, h) = self.face.size;
        let (w, h) = ((w * scale).max(160.0), (h * scale).max(90.0));
        let shift = |at: Pixels| px(f32::from(at) * (1.0 - scale));
        // A miniature Pane in the board's own recipe: its head line (the
        // mark in the glyph column, the title in `W_LABEL`, the branch as
        // meta), a hairline, then a few quiet skeleton lines for its body.
        let mut body = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .gap(px(theme::SPACE_2))
            .px(px(theme::PANE_PAD_X))
            .py(px(theme::SPACE_3));
        for share in GHOST_LINES {
            body = body.child(
                div()
                    .h(px(theme::SPACE_1_5))
                    .w(gpui::relative(share))
                    .bg(theme::paint::SELECTED),
            );
        }
        let marker = if self.face.reader {
            ferrite_ui::icons::icon(ferrite_ui::icons::FILE, theme::ROW_ICON, theme::TEXT_DIM)
                .into_any_element()
        } else {
            ferrite_ui::components::status_dot(theme::TEXT_DIM).into_any_element()
        };
        let head = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap(px(theme::SPACE_2))
            .h(px(theme::PANE_HEAD_H))
            .px(px(theme::HEAD_PAD_X))
            .border_b_1()
            .border_color(theme::paint::LINE)
            .font_family(theme::FONT_UI)
            .child(marker)
            .child(
                div()
                    .min_w_0()
                    .flex_shrink(1.)
                    .truncate()
                    .font_weight(theme::W_LABEL)
                    .text_size(px(theme::FS_UI))
                    .line_height(px(theme::LH_UI))
                    .text_color(rgb(theme::TEXT_STRONG))
                    .child(self.face.title.clone()),
            )
            .children(self.face.detail.clone().map(|detail| {
                div()
                    .min_w_0()
                    .flex_shrink(1.)
                    .truncate()
                    .text_size(px(theme::FS_UI))
                    .line_height(px(theme::LH_UI))
                    .text_color(rgb(theme::TEXT_DIM))
                    .child(detail)
            }));
        let card = div()
            .flex()
            .flex_col()
            .w(px(w))
            .h(px(h))
            .overflow_hidden()
            .border_1()
            .border_color(rgba(theme::FOCUS_RING))
            .bg(theme::paint::FLOAT)
            .shadow(ferrite_ui::components::elevation(
                ferrite_ui::components::Elevation::Float,
            ))
            .child(head)
            .child(body);
        // Lifted off the board, the miniature fades up to its see-through
        // ink rather than popping in under the pointer.
        let card = ferrite_ui::motion::fade_in(
            "pane-ghost-in",
            card,
            ferrite_ui::motion::FADE_QUICK,
            theme::DRAG_GHOST_OPACITY,
        );
        div()
            .pl(shift(self.grab.x))
            .pt(shift(self.grab.y))
            .child(card)
    }
}

/// How wide the grab band over a seam is, centred on the gutter: the
/// prototype's 9px band (`.seam{width:9px;margin-left:-4px}`), a pixel past
/// the 8px gutter.
const SEAM_GRAB: f32 = 9.0;

/// A board rect as gpui bounds, in the same window coordinates.
fn window_bounds(rect: layout::Rect) -> gpui::Bounds<Pixels> {
    gpui::Bounds::new(
        gpui::point(px(rect.x), px(rect.y)),
        gpui::size(px(rect.w), px(rect.h)),
    )
}

/// A path as the operator reads it: their home spelled `~`.
fn home_relative(path: &std::path::Path) -> String {
    let shown = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && shown.starts_with(&home) => {
            format!("~{}", &shown[home.len()..])
        }
        _ => shown,
    }
}

/// The open Project card. One surface serves both verbs: `target` names
/// the Project being edited, or `None` while one is being created.
///
/// `staged` is the create card's directory list — a Project does not exist
/// until it is confirmed, so its directories live here until then. The edit
/// card writes through to the registry instead and leaves `staged` empty.
struct ProjectEditor {
    target: Option<ProjectId>,
    /// The name line. Empty means "call it after the main directory".
    name: Entity<ferrite_pane::composer::Composer>,
    staged: Vec<std::path::PathBuf>,
    /// A refusal, shown on the card that caused it.
    error: Option<SharedString>,
    /// The body's scroll, for its overlay scrollbar.
    scroll: ScrollHandle,
    /// On a machine, the path field that stands in for the folder dialog.
    remote: Option<machines::RemotePath>,
}

impl ProjectEditor {
    /// The name to save: what was typed, else the main directory's leaf.
    /// A directory with no leaf name (a drive root) keeps its whole path.
    fn resolved_name(&self, typed: &str) -> Option<String> {
        let typed = typed.trim();
        if !typed.is_empty() {
            return Some(typed.to_string());
        }
        let main = self.staged.first()?;
        Some(
            main.file_name()
                .map(|leaf| leaf.to_string_lossy().into_owned())
                .unwrap_or_else(|| main.display().to_string()),
        )
    }
}

/// What a right-click was on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MenuTarget {
    /// A nav row.
    Thread(ThreadId),
    /// A Pane — the same Thread, but the rename opens in the head.
    Pane(ThreadId),
    Group(GroupId),
    /// A Project's heading in the nav.
    Project(ProjectId),
    /// The Parked section's header at the foot of the nav.
    Parked,
    /// A diff line in the Review beside the Thread's Pane (ADR 0021).
    Review {
        thread: ThreadId,
        line: usize,
    },
}

/// One thing a context-menu row does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MenuVerb {
    Rename,
    Focus,
    Fullscreen,
    /// Unfold or fold the Parked section.
    ToggleParked,
    /// Delete every Thread the Parked section lists — exactly those rows,
    /// so what the menu erases is what the operator can see.
    DeleteAllParked,
    /// Park the Thread in one act, on screen or not: a Group member leaves
    /// its Group and parks (`Cockpit::park_thread`), the same as cmd-w.
    Park,
    /// The transcript menu's own verbs: what is highlighted (in the
    /// Review, the pick), or all of it.
    CopySelection,
    CopyTranscript,
    /// The Review's own verbs: comment on the pick, read the line's file.
    Comment,
    OpenAtLine,
    LeaveGroup,
    EnterGroup,
    DissolveGroup,
    NewThread,
    Reveal,
    CopyPath,
    /// The Thread's PR: ask `gh` again now, or open it in the browser.
    RefreshPr,
    OpenPr,
    /// The work board, new tasks going to the Project.
    OpenTasks,
    Delete,
}

#[derive(Clone, Copy)]
enum DraftPlacement {
    Loose,
    CurrentGroup,
    NewGroupWith(ThreadId),
}

/// About's Version: the package version, and ` · dev` on a development
/// build, so a release is never labelled.
fn version_label(version: &str, dev: bool) -> String {
    if dev {
        format!("{version} · dev")
    } else {
        version.to_string()
    }
}

/// A rename editor with its keys beside it while it edits: `↵ save · esc
/// cancel` in mono `TEXT_DIM`, the `·` in structure ink. The keys live
/// here, where they act, rather than on the menu row that opened it.
fn rename_field(editor: Entity<ferrite_pane::composer::Composer>) -> Div {
    let text = "↵ save · esc cancel";
    let at = text.find(" · ").unwrap_or(0);
    let hint = ferrite_ui::components::text_meta()
        .debug_selector(|| "rename-hint".into())
        .flex_shrink_0()
        .font_family(ferrite_ui::theme::FONT_CODE)
        .child(gpui::StyledText::new(text).with_highlights(vec![(
            at..at + " · ".len(),
            gpui::HighlightStyle {
                color: Some(rgb(ferrite_ui::theme::TEXT_FAINT).into()),
                ..Default::default()
            },
        )]));
    div()
        .flex()
        .items_center()
        .gap(px(ferrite_ui::theme::SPACE_2))
        .min_w_0()
        .flex_1()
        .child(div().min_w_0().flex_1().child(editor))
        .child(hint)
}

/// The context menu up on screen.
struct ContextMenu {
    target: MenuTarget,
    at: Point<Pixels>,
    rows: Vec<Option<(menu::Item, MenuVerb)>>,
    /// The destructive row pressed once, waiting for its confirmation.
    armed: Option<usize>,
}

/// What rides the pointer while a nav row is dragged: that row itself, as
/// the tree drew it — a Thread's row, or the Group's header.
#[derive(Clone)]
enum NavChip {
    Thread(nav::ThreadRow),
    Group(nav::GroupBlock),
}

impl NavChip {
    fn element(&self, cx: &gpui::App) -> Div {
        let measure = nav::Measure::Text(cx);
        nav::drag_row(match self {
            NavChip::Thread(row) => nav::thread_row_with_title(
                row,
                row.name.clone(),
                nav::RowPlace::ROOT,
                None,
                false,
                0,
                &measure,
            ),
            NavChip::Group(group) => nav::group_row_with_title(
                group,
                group.title.clone(),
                nav::group_fold(group),
                0,
                0,
                &measure,
            ),
        })
    }
}

struct NavDragPreview(NavChip);

/// A nav drag, plus the View it started from. The row's own mouse-down
/// fires before the drag does — clicking a Group row enters it — so by the
/// time the drop lands, `self.view` is wherever the *press* took the
/// operator, not where they picked the row up. The drop's meaning belongs
/// to the origin: dragging the second member out of a pair you are looking
/// at is a different act from dragging it out of a Group you are not.
#[derive(Clone, Copy)]
struct NavDrag {
    drag: Drag,
    origin: View,
}

impl Render for NavDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.element(cx)
    }
}

/// What a row says while a drag hovers it: the wash of the answer core
/// already knows — `DROP_WASH`, `TEXT_STRONG` as a ground, where the drop would
/// land, or `BLOCKED_WASH` where core refuses it. The wash alone speaks: a
/// ring would need an edge the rows do not have, and adding one mid-drag
/// would move the row.
fn drop_feedback<E: gpui::InteractiveElement>(element: E, groups: Groups, target: DropTarget) -> E {
    element.drag_over::<NavDrag>(move |style, drag, _, _| {
        if matches!(groups.preview_drop(drag.drag, target), Plan::Refused(_)) {
            style.bg(rgba(ferrite_ui::theme::BLOCKED_WASH))
        } else {
            style.bg(rgba(ferrite_ui::theme::DROP_WASH))
        }
    })
}

/// The one popover the Composer's slot can hold — the `/` or `@` menu
/// (#23), the provider picker (#25) or a draft's band chip (#29) — and
/// everything it shows. At most one for the whole cockpit, always on the
/// focused Pane's Composer: `pane` names that Pane by the roster's own
/// identity. Rows are discovered when it opens and re-derived only by its
/// kind's own rule, never per frame; each row carries what picking it
/// does, so the two can never drift apart.
struct Popover {
    /// The Pane whose Composer holds it.
    pane: PaneIdentity,
    kind: Kind,
    rows: Vec<Row>,
    selected: usize,
    /// The row list's scroll: arrows keep the keyboard cursor in view.
    /// A fresh popover reveals its first cursor once (`revealed`), which
    /// may be a picker's standing choice below the fold.
    scroll: ScrollHandle,
    revealed: std::cell::Cell<bool>,
    /// Why every row is dead for now (`tuning_hold`): a picker built while
    /// its Thread's turn runs. The disabled rows say it in their tooltip.
    hold: Option<&'static str>,
}

/// Which popover holds the slot. The rows, keys, dismissal, heal and paint
/// are shared; the kind decides the row recipe, the footer hint, and the
/// rule that closes it.
enum Kind {
    /// `/` — the Session's own commands (Claude's initialize `commands[]`,
    /// Codex's skills/list), straight from core. Nothing static — except
    /// Ferrite's own local rows riding on top, never sent to the provider:
    /// `provider` (#25) — the picker's door pre-lock, an inert explanation
    /// after — then `import` while the Thread still offers adoption (#11).
    /// Derived from the line's own text: every edit re-syncs it.
    Commands,
    /// `@` — files under the Thread's workspace binding. The walk runs once
    /// when the menu opens and is filtered per keystroke; `token_start` is
    /// where the `@` sits, so a pick knows what to splice out. Text-derived
    /// like `Commands`.
    Files {
        files: std::rc::Rc<Vec<String>>,
        token_start: usize,
        query: String,
        request_id: u64,
    },
    /// #25: re-aim the Thread's provider / model before its first prompt.
    Provider,
    /// The Composer's effort picker: the reasoning ladder the Thread's
    /// model takes, from the provider's own announcement, with the
    /// operator's default on top. Opened by the chip or `/effort`.
    Effort,
    /// #29: one band chip's choices on the focused draft — registry reads,
    /// never a filesystem scan — except the project chip's type-a-path row,
    /// which re-derives from the Composer line per edit.
    Band(pane::BandChip),
}

struct PendingFileSearch {
    pane: PaneIdentity,
    thread: ThreadId,
    generation: u64,
    request_id: u64,
    query: String,
    token_start: usize,
    root: std::path::PathBuf,
    started: std::time::Instant,
    reply: std::sync::mpsc::Receiver<std::io::Result<Vec<ferrite_core::providers::FileSuggestion>>>,
    fallback: Option<std::sync::mpsc::Receiver<Vec<pane::MenuRow>>>,
}
/// The import picker's session listing, under way off the UI thread.
struct PendingDiscovery {
    /// The blank Pane a pick replaces, if any.
    into: Option<PaneIdentity>,
    request_id: u64,
    started: std::time::Instant,
    reply: std::sync::mpsc::Receiver<std::io::Result<Vec<ferrite_core::import::Candidate>>>,
}

static NEXT_FILE_SEARCH: AtomicU64 = AtomicU64::new(1);

impl Kind {
    fn picker_slot(&self) -> Option<(bool, bool)> {
        match self {
            Kind::Provider => Some((false, false)),
            Kind::Effort => Some((true, false)),
            Kind::Band(pane::BandChip::Provider) => Some((false, true)),
            Kind::Band(pane::BandChip::Effort) => Some((true, true)),
            _ => None,
        }
    }

    /// Text-derived popovers follow the line: `sync_menu` rebuilds them on
    /// every edit and escape mutes them until the text moves. The rest are
    /// opened by an act, closed by one, and dismissed by typing a prompt
    /// over them.
    fn follows_text(&self) -> bool {
        matches!(self, Kind::Commands | Kind::Files { .. })
    }

    /// The footer's key hints: each key, then its verb.
    fn hints(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            Kind::Commands => &[("↑↓", "select"), ("↵", "run"), ("esc", "dismiss")],
            Kind::Files { .. } => &[("↑↓", "select"), ("↵", "insert"), ("esc", "dismiss")],
            Kind::Band(pane::BandChip::Project) => &[
                ("type", "a path"),
                ("↑↓", "move"),
                ("↵", "pick"),
                ("esc", "dismiss"),
            ],
            Kind::Band(pane::BandChip::Workspace) => {
                &[("↑↓", "move"), ("↵", "pick"), ("esc", "dismiss")]
            }
            // The pickers are `ChoiceMenu`s: the kit keys them, and the
            // slot's footer never draws for them.
            Kind::Provider
            | Kind::Effort
            | Kind::Band(pane::BandChip::Provider | pane::BandChip::Effort) => &[],
        }
    }
}

/// One row beside its own consequence. The paint half is the Pane's
/// `MenuRow`, reached through `Deref` so a row reads like the line it
/// draws.
struct Row {
    row: pane::MenuRow,
    /// The ✓ — the standing choice on picker and band rows; never on a
    /// menu row.
    active: bool,
    consequence: Consequence,
}

impl Row {
    fn consequence_is_inert(&self) -> bool {
        matches!(self.consequence, Consequence::Inert)
    }
}

impl std::ops::Deref for Row {
    type Target = pane::MenuRow;

    fn deref(&self) -> &pane::MenuRow {
        &self.row
    }
}

/// One draft menu request. Its provider and cwd prevent late replies from
/// leaking commands across project/workspace changes.
struct DraftCommands {
    provider: Provider,
    cwd: std::path::PathBuf,
    pending: Option<ferrite_core::providers::commands::Discovery>,
    commands: Vec<ferrite_core::SessionCommand>,
    error: Option<String>,
}

impl DraftCommands {
    fn poll(&mut self) -> bool {
        let Some(rx) = &self.pending else {
            return false;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(error) => Err(std::io::Error::other(error)),
        };
        self.pending = None;
        match result {
            Ok(commands) => {
                self.commands = commands;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        true
    }
}

/// What picking a row does. A command or a file lands in the line; every
/// other pick is Ferrite's own act, never a prompt.
#[derive(PartialEq)]
enum Consequence {
    /// Replace the active `/filter` with `/name ` — sent later as plain text
    /// on Claude and translated to the typed skill item inside the Codex
    /// Session.
    Command(SharedString),
    /// The local `provider` row pre-lock (#25): clear the line and open
    /// the picker in the menu's place.
    OpenProviderPicker,
    /// The local `effort` row: clear the line and open the effort picker.
    OpenEffortPicker,
    /// An effort-row pick: this level, or None for the operator's default.
    Effort(Option<String>),
    /// The local `import` row (#11): open the import picker — clearing the
    /// line on a Thread, and leaving it alone on a draft, whose typed
    /// command must survive a refused adoption.
    OpenImportPicker,
    /// The locked door's row is an explanation, not an offer: its pick
    /// dismisses and nothing else.
    Inert,
    /// Replace the `@token` with `@rel/path ` and stage the comp's pill
    /// over it, whichever the provider: the wire stays untouched — Claude's
    /// CLI reads the `@path` text itself, Codex's send derives its mention
    /// item — the pick just paints the standing token.
    Mention(SharedString),
    Provision(ProviderChoice),
    Band(BandChoice),
}

/// What picking a band row does to the focused draft.
#[derive(PartialEq)]
enum BandChoice {
    Provider(ProviderChoice),
    /// The effort chip: a rung, or None for the operator's default.
    Effort(Option<String>),
    Project(ProjectId),
    /// The type-a-path row: register the typed path as a project, then
    /// choose it.
    RegisterPath(std::path::PathBuf),
    Target(DraftTarget),
    /// Open the Project card, as the nav's dropdown does.
    AddProject,
}

/// Allow only pixel rounding at the bottom of a native scroll container.
#[cfg(test)]
const TAIL_SLACK: Pixels = px(2.);

/// How often the watchdog sweeps. Leaks grow over seconds, not frames; a
/// sweep per frame would spawn a `ps`/`tasklist` per Session per tick.
const SWEEP_INTERVAL: Duration = Duration::from_secs(2);

/// How often the sweep re-reads free disk for the bar: its minute.
const FREE_DISK_EVERY: Duration = Duration::from_secs(60);

#[cfg(test)]
thread_local! {
    /// Root Cockpit renders on this (test) thread: the unit the idle frame
    /// budget is measured in.
    pub(crate) static RENDERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// What the last frame drew for each Pane and nav row, and how often each
/// was built: the stale-chrome tests read what is on screen, not the model,
/// and the frame budgets count the work each part did. Also how often the
/// Projects board was asked for, by any door.
#[cfg(test)]
pub(crate) mod drawn {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    use ferrite_core::roster::PaneIdentity;
    use ferrite_core::ThreadId;

    /// One Pane as its last build drew it.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub(crate) struct Pane {
        pub focused: bool,
        pub unread: bool,
        pub drop_target: bool,
        pub editing: bool,
        pub reduce_motion: bool,
        pub cell_width: f32,
        /// The working clock its lines read (`12s`).
        pub clock: Option<String>,
        /// Its head holds the rename editor.
        pub renaming: bool,
    }

    thread_local! {
        static PANES: RefCell<HashMap<PaneIdentity, Pane>> = RefCell::new(HashMap::new());
        static PANE_RENDERS: RefCell<HashMap<PaneIdentity, usize>> = RefCell::new(HashMap::new());
        static NAV_RENDERS: Cell<usize> = const { Cell::new(0) };
        static NAV_ROWS: RefCell<HashMap<ThreadId, String>> = RefCell::new(HashMap::new());
        static PROJECTS_TOGGLES: Cell<usize> = const { Cell::new(0) };
    }

    pub(crate) fn toggled_projects() {
        PROJECTS_TOGGLES.with(|toggles| toggles.set(toggles.get() + 1));
    }

    /// How many times the Projects board was toggled on this thread.
    pub(crate) fn projects_toggles() -> usize {
        PROJECTS_TOGGLES.with(Cell::get)
    }

    pub(crate) fn built_pane(identity: PaneIdentity) {
        PANE_RENDERS.with(|renders| *renders.borrow_mut().entry(identity).or_default() += 1);
    }

    pub(crate) fn record_pane(identity: PaneIdentity, pane: Pane) {
        PANES.with(|panes| panes.borrow_mut().insert(identity, pane));
    }

    pub(crate) fn built_nav() {
        NAV_RENDERS.with(|renders| renders.set(renders.get() + 1));
    }

    pub(crate) fn record_row(thread: ThreadId, row: String) {
        NAV_ROWS.with(|rows| rows.borrow_mut().insert(thread, row));
    }

    /// The Pane as last drawn.
    pub(crate) fn pane(identity: PaneIdentity) -> Pane {
        PANES.with(|panes| panes.borrow().get(&identity).cloned().unwrap_or_default())
    }

    /// How many times this Pane has been built on this thread.
    pub(crate) fn pane_renders(identity: PaneIdentity) -> usize {
        PANE_RENDERS.with(|renders| renders.borrow().get(&identity).copied().unwrap_or(0))
    }

    /// How many times the nav has been built on this thread.
    pub(crate) fn nav_renders() -> usize {
        NAV_RENDERS.with(Cell::get)
    }

    /// The nav row for `thread` as last drawn: its status, unread, tail
    /// and name.
    pub(crate) fn row(thread: ThreadId) -> String {
        NAV_ROWS.with(|rows| rows.borrow().get(&thread).cloned().unwrap_or_default())
    }
}

/// The panes24 instrument, kept behind an env var: frames actually painted,
/// and what the process is holding while it paints them.
struct Perf {
    frames: u64,
    since: std::time::Instant,
}

impl CockpitView {
    #[cfg(test)]
    pub fn new(cockpit: Cockpit, cx: &mut Context<Self>) -> Self {
        Self::new_with_provider(cockpit, Provider::Claude, cx)
    }

    /// The test view with the model's thinking shown (the palette's `show
    /// thinking`): what the reasoning rows' own tests draw.
    #[cfg(test)]
    pub fn new_thinking(cockpit: Cockpit, cx: &mut Context<Self>) -> Self {
        let mut prefs = Preferences::ephemeral();
        prefs.settings.show_thinking = true;
        Self::new_with_settings(cockpit, Provider::Claude, prefs, cx)
    }

    #[cfg_attr(not(any(test, feature = "visual-reference")), allow(dead_code))]
    pub fn new_with_provider(
        cockpit: Cockpit,
        launch_provider: Provider,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_with_settings(cockpit, launch_provider, Preferences::ephemeral(), cx)
    }

    pub fn new_with_settings(
        mut cockpit: Cockpit,
        launch_provider: Provider,
        prefs: Preferences,
        cx: &mut Context<Self>,
    ) -> Self {
        ferrite_pane::pane::init_components(cx);
        subagents::init(cx);
        cockpit.settings_changed(&prefs.settings);
        // This window says what it draws from its first frame on, so no
        // Thread streams to it before it is drawn (ADR 0032).
        cockpit.draws_transcripts([]);
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(pump_interval()).await;
            if this.update(cx, |view, cx| view.pump(cx)).is_err() {
                break;
            }
        })
        .detach();
        // Quitting (Cmd-Q, the Dock, a logout) ends the process without
        // dropping this view, so the Sessions' processes are ended here,
        // inside the platform's shutdown budget. Otherwise a Codex
        // app-server outlives Ferrite for a moment holding its thread's
        // writer lock, and the relaunch's resume is refused.
        if prefs.cli_updates {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(crate::cli_updates::FIRST_CHECK_AFTER)
                    .await;
                loop {
                    if this
                        .update(cx, |view, cx| view.tick_cli_updates(cx))
                        .is_err()
                    {
                        break;
                    }
                    cx.background_executor()
                        .timer(crate::cli_updates::TICK)
                        .await;
                }
            })
            .detach();
        }
        if prefs.ferrite_updates {
            updating::schedule_checks(cx);
        }
        let board = prefs.board;
        if board {
            Self::start_sidebar_refresh(cx);
        }
        cx.on_app_quit(|view, _cx| {
            view.cockpit.halt_sessions();
            async {}
        })
        .detach();

        // A packaged app's process directory belongs to the app, not the
        // operator. Start drafts where they were last working, and leave a
        // new installation genuinely empty.
        let launch_project = startup_project(&cockpit);
        // The existing GPUI fixtures assume construction supplies a draft.
        // Keep their synthetic seed out of production; `startup_project` is
        // tested directly against an empty registry below.
        #[cfg(test)]
        let launch_project = launch_project.or_else(|| cockpit.register_project(&here()).ok());
        let mut view = Self {
            cockpit,
            panes: Vec::new(),
            focus: cx.focus_handle(),
            perf: std::env::var("FERRITE_PERF").is_ok().then(|| Perf {
                frames: 0,
                since: std::time::Instant::now(),
            }),
            swept: cx.background_executor().now(),
            parts: Default::default(),
            frame_tick: cx.new(|_| parts::FrameTick),
            ages_turn_at: None,
            frame_turn_at: None,
            free_disk: None,
            free_disk_asked: None,
            branch_refreshing: false,
            branch_refresh_queued: false,
            selection: TranscriptText::default(),
            native_copy: None,
            nav_filter: None,
            nav_folds: std::collections::HashSet::new(),
            nav_listed: Default::default(),
            nav_scroll: ScrollHandle::new(),
            nav_view: nav::NavView::Tree,
            nav_view_focus: cx.focus_handle(),
            nav_rail_scroll: ScrollHandle::new(),
            activation: None,
            popover: None,
            menu_muted: false,
            draft_commands: None,
            suppress_recall_menu_once: false,
            session_file_roots: ferrite_core::import::default_roots(),
            discovery_request: 0,
            pending_files: None,
            pending_discovery: None,
            launch_project,
            checked_out: Default::default(),
            checking_out: Default::default(),
            repo_listing: None,
            asking_repo: None,
            rename: None,
            context_menu: None,
            panel: Default::default(),
            pr_tab: Default::default(),
            context_usage_expanded: false,
            session_controls: None,
            mode_picker: None,
            session_control_error: None,
            context_checks: None,
            float_triggers: FloatTriggers::default(),
            card_waits: Default::default(),
            nav_collapsed: prefs.settings.nav_collapsed,
            nav_auto_rail: std::cell::Cell::new(false),
            nav_forced_open: false,
            nav_measure_full: std::cell::Cell::new(false),
            nav_hovered: false,
            nav_drag_live: std::cell::Cell::new(false),
            nav_snapshot: std::cell::RefCell::new(None),
            nav_tween: None,
            nav_fade: None,
            facts: Facts::with_auto_title(prefs.settings.auto_title),
            seam_drag: None,
            board: std::cell::Cell::new(layout::Rect::default()),
            board_level: std::cell::Cell::new(Level::Transcript),
            drop_preview: None,
            file_drop_over: None,
            board_layouts: std::collections::HashMap::new(),
            slot_sizes: Default::default(),
            dragging: None,
            nav_dragging: None,
            prefs,
            settings_open: false,
            settings_page: prefs::PageKey::default(),
            settings_search: None,
            settings_copied: None,
            project_editor: None,
            project_editor_focus: cx.focus_handle(),
            settings_focus: cx.focus_handle(),
            maximized: false,
            cli_versions: None,
            cli_probing: false,
            shown_version: env!("CARGO_PKG_VERSION").into(),
            cli_updates: Default::default(),
            ferrite_update: Default::default(),
            server_bands: Default::default(),
            unreadable_told: false,
            user_commands: Default::default(),
            projects: projects::ProjectsState::new(board),
            leave_asked: false,
            group_error: None,
            needs_you: NeedsYou::new(),
            toast_stage: Default::default(),
            landing: None,
            #[cfg(test)]
            chimed: Vec::new(),
            pr_focus: None,
            #[cfg(test)]
            pr_asked: Vec::new(),
            #[cfg(test)]
            pr_tab_reads: Vec::new(),
            floats: palette::Floats::new(cx),
            transcript_entities: Default::default(),
            decisions: decisions::DecisionState::new(cx.weak_entity()),
            nav_ride: std::cell::Cell::new(None),
            nav_measure_rest: std::cell::Cell::new(false),
            nav_hold: None,
            // The process's own directory, where the operator launched it
            // (a Dock launch has stood in the newest Thread's by now).
            launch_dir: if cfg!(test) {
                None
            } else {
                std::env::current_dir().ok()
            },
            empty_shown: false,
            empty_cursor: 0,
            empty_focus: cx.focus_handle(),
            solo_beside: None,
            preview_pressed: None,
            reader_slides: Default::default(),
            plan_card: None,
            launch_provider,
            machines: None,
            machine_field: None,
        };
        // A Cockpit notification is the earliest common point after core
        // state changes and before GPUI draws cached transcript children.
        // The render path repeats this cheap key comparison for initial mount
        // and derived display state that did not notify the Cockpit.
        cx.observe_self(|view, cx| view.sync_visible_transcripts(cx))
            .detach();
        // Every Thread the launch opened is on the roster already, and
        // every Thread it did not open is a parked row from the first
        // frame — a launch that opens nothing has no change to notice.
        view.sync_panes(cx);
        view.parked_changed(cx);
        // The bar's free disk from the first frame; the sweep re-reads it
        // each minute.
        view.free_disk_asked = Some(cx.background_executor().now());
        view.read_free_disk(cx);
        // Nothing revived: the cockpit starts as one draft Pane (#29) —
        // nothing spawns before the operator's choice. A machine's Threads
        // arrive once it is connected, and its Projects are its own.
        if view.panes.is_empty() && view.cockpit.machine().is_none() {
            if view.launch_project.is_some() {
                view.open_draft_with_provider(DraftTarget::Main, launch_provider, cx);
            } else {
                view.open_project_creator(cx);
            }
        }
        view
    }

    /// The window's machines (ADR 0026), listed in the nav and Settings.
    pub fn with_machines(mut self, machines: Entity<crate::machines::Machines>) -> Self {
        self.machines = Some(machines);
        self
    }

    /// Open `open` once the Cockpit is attached, if it is by `until`.
    pub fn with_landing(mut self, open: Option<Landing>, until: std::time::Instant) -> Self {
        self.landing = open.map(|open| (open, until));
        self
    }

    /// The Needs-you state of the Cockpit the window left (`NeedsYou::moved`).
    pub fn with_needs_you(mut self, needs_you: NeedsYou) -> Self {
        self.needs_you = needs_you;
        self
    }

    pub(crate) fn machines(&self) -> Option<Entity<crate::machines::Machines>> {
        self.machines.clone()
    }

    pub(crate) fn launch_provider(&self) -> Provider {
        self.launch_provider
    }

    /// The Preferences a Cockpit built in this one's place starts with:
    /// the settings as they are now; what a launch says once is said.
    pub(crate) fn successor_preferences(&self) -> Preferences {
        Preferences {
            settings: self.prefs.settings.clone(),
            dir: self.prefs.dir.clone(),
            defaults: self.prefs.defaults.clone(),
            titler: self.prefs.titler,
            cli_updates: self.prefs.cli_updates,
            ferrite_updates: self.prefs.ferrite_updates,
            whats_new: None,
            unreadable: self.prefs.unreadable.clone(),
            keys_refused: Vec::new(),
            board: self.prefs.board,
        }
    }

    /// Mirror the roster (#28): a view for every Pane the Cockpit shows, in
    /// the roster's order, and none for a Pane it no longer does. The one
    /// way a Thread's Pane joins the grid — built, its Composer watched so
    /// every edit of the line re-syncs the open menu (#23), and its caches
    /// filled here, never per frame. Drafts are made by
    /// `open_draft_with_choice`, which knows their binding; the roster
    /// never holds a draft this window did not open.
    fn sync_panes(&mut self, cx: &mut Context<Self>) {
        let wanted = self.cockpit.roster().panes().to_vec();
        let before = self.panes.len();
        self.panes.retain(|pane| wanted.contains(&pane.identity));
        let mut opened = false;
        for identity in &wanted {
            if self.index_of(*identity).is_some() {
                continue;
            }
            let PaneIdentity::Thread(thread) = *identity else {
                continue;
            };
            let pane = CockpitPane::new(PaneView::new(thread, cx), cx);
            cx.subscribe(&pane.composer, Self::composer_edited).detach();
            self.panes.push(pane);
            self.thread_opened(thread, cx);
            opened = true;
        }
        self.panes
            .sort_by_key(|pane| wanted.iter().position(|shown| *shown == pane.identity));
        let retained_transcript_entities: std::collections::HashSet<_> = self
            .panes
            .iter()
            .flat_map(|pane| pane.transcripts.values())
            .map(|transcript| transcript.entity_id())
            .collect();
        self.transcript_entities
            .retain(|id| retained_transcript_entities.contains(id));
        // A Thread that came or went moved between the grid and the nav's
        // parked rows.
        if opened || self.panes.len() != before {
            self.parked_changed(cx);
        }
        self.refresh_names();
    }

    /// The parked set changed: rebuild the nav's parked rows now, from the
    /// rows the Server published, and fetch what only `git` can say on the
    /// background executor — the rows fill in when it answers.
    fn parked_changed(&mut self, cx: &mut Context<Self>) {
        let lookups = self.facts.parked_changed(&self.cockpit);
        self.look_up(lookups, cx);
    }

    /// A Thread's Pane opened: its facts afresh, what only git can say
    /// read off the UI thread.
    fn thread_opened(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let lookups = self.facts.opened(&self.cockpit, thread);
        self.look_up(lookups, cx);
        self.check_pr(thread, false, cx);
    }

    /// Ask `gh` for the Thread's PR where its checkout is
    /// (`Cockpit::pull_request`, on a machine its Server): at once when
    /// `manual`, else at most once per `pr::AUTO_EVERY`. The row's symbol
    /// turns until the answer lands.
    pub(crate) fn check_pr(&mut self, thread: ThreadId, manual: bool, cx: &mut Context<Self>) {
        let Some(path) = self.thread_path(thread) else {
            return;
        };
        if !self
            .facts
            .ask_pr(thread, cx.background_executor().now(), manual)
        {
            return;
        }
        self.notify_part(parts::Part::Nav, cx);
        #[cfg(test)]
        self.pr_asked.push((thread, manual));
        // Tests and captures never run `gh`: only a fixture world's forge
        // answers them (`Cockpit::fixture_forge`); a test reads what was
        // asked.
        if cfg!(any(test, feature = "visual-reference")) && !self.cockpit.has_forge_fixture(&path) {
            return;
        }
        let answer = self.cockpit.pull_request(&path);
        cx.spawn(async move |this, cx| {
            // A read that failed is `gh` with nothing to say: no label.
            let reading = beside::answered(answer, cx).await.ok().flatten();
            this.update(cx, |view, cx| view.pr_landed(thread, reading, cx))
                .ok();
        })
        .detach();
    }

    /// Adopt what `gh` said about the Thread's PR.
    fn pr_landed(
        &mut self,
        thread: ThreadId,
        reading: Option<ferrite_core::workspace::PullRequest>,
        cx: &mut Context<Self>,
    ) {
        if self.facts.pr_answered(thread, reading) {
            self.notify_part(parts::Part::Nav, cx);
        }
    }

    /// Open the Thread's PR on the forge, in the browser on this computer.
    pub(crate) fn open_pr(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let url = self
            .facts
            .pr(thread)
            .and_then(|pr| pr.url.as_deref())
            .or_else(|| self.pr_tab.url(thread));
        if let Some(url) = url {
            cx.open_url(url);
        }
    }

    /// Run the facts' slow reads in the background and adopt what they
    /// find. On a machine the git half is left out: its checkouts are the
    /// machine's (ADR 0026).
    fn look_up(&mut self, mut lookups: crate::facts::Lookups, cx: &mut Context<Self>) {
        if self.on_machine() {
            lookups.branches.clear();
            lookups.default_branches.clear();
        }
        if lookups.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let answers = cx
                .background_executor()
                .spawn(async move { lookups.run() })
                .await;
            this.update(cx, |view, cx| {
                view.facts.looked_up(answers);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Compare a visible Subject's key every root render, and only then copy
    /// its bounded blocks and timings into its retained entity — with its
    /// banner, its tail row, the call a pending Decision gates, and the
    /// checkout its path targets resolve against (`transcript_glue`).
    fn sync_transcript(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((thread, subject, namespace, focused, selection_scope, disclosure_revision)) =
            self.panes.get(index).and_then(|pane| {
                pane.thread().map(|thread| {
                    (
                        thread,
                        pane.selected.clone(),
                        pane.text_namespace(),
                        index == self.focused(),
                        pane.selection_scope,
                        pane.disclosure_revision(),
                    )
                })
            })
        else {
            return;
        };
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        let Some(subject_view) = open.activity().subject(&subject) else {
            return;
        };
        let transcript = subject_view.transcript();
        let revision = subject_view.presentation_revision();
        let status = pane::transcript_status(subject_view.status(), subject_view.fresh());
        // One reading size for every transcript, on every board.
        let reading_size = self.prefs.settings.reading_size;
        let banner = self.banner_facts(thread, transcript);
        let tail = self.transcript_tail(index, cx);
        // The gated call hides while its Decision waits as the tail row.
        let pending_call = tail
            .as_ref()
            .and_then(|_| open.pending())
            .map(|decision| decision.tool_use_id.clone())
            .filter(|call| !call.is_empty());
        let workspace =
            ferrite_core::workspace::effective_cwd(open.session_project_root(), open.workspace())
                .map(std::path::Path::to_path_buf);
        let solo = self.cockpit.roster().fullscreen().is_some()
            || matches!(self.cockpit.roster().view(), View::Solo);
        let key = ferrite_pane::transcript::TranscriptKey {
            namespace: namespace.clone(),
            content_revision: revision,
            display_revision: disclosure_revision,
            focused,
            signal_status: Some(status),
            reading_size,
            banner: banner.clone(),
            tail: tail.as_ref().map(|tail| tail.key),
            pending_call: pending_call.clone(),
            workspace: workspace.clone(),
            solo,
            thinking: self.prefs.settings.show_thinking,
        };
        let entity = self.panes[index]
            .ensure_transcript(cx)
            .expect("thread Pane has a transcript entity");
        if self.transcript_entities.insert(entity.entity_id()) {
            cx.subscribe(&entity, Self::transcript_event).detach();
        }
        if entity.read(cx).matches(&key) {
            return;
        }
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        let Some(subject_view) = open.activity().subject(&subject) else {
            return;
        };
        let transcript = subject_view.transcript();
        let pane = &self.panes[index];
        let preview = pane.preview.clone();
        let disclosure = pane.transcript_disclosure_snapshot();
        let input = ferrite_pane::transcript::TranscriptInput {
            thread,
            namespace,
            content_revision: revision,
            display_revision: disclosure_revision,
            blocks: pane::rendered_window(transcript.blocks(), Level::Transcript).to_vec(),
            signal_status: Some(status),
            timings: subject_view.timings().clone(),
            provider: Some(open.provider()),
            focused,
            reading_size,
            selection_scope,
            preview,
            expanded: disclosure.0,
            target: disclosure.1,
            disclosure_focus: disclosure.2,
            tail,
            banner,
            pending_call,
            workspace,
            solo,
            thinking: self.prefs.settings.show_thinking,
            settled_at: transcript.settled_at().clone(),
        };
        let selection = self.selection.clone();
        entity.update(cx, |transcript, cx| transcript.sync(input, selection, cx));
    }

    fn sync_visible_transcripts(&mut self, cx: &mut Context<Self>) {
        for index in self.visible_indices() {
            self.sync_transcript(index, cx);
        }
    }

    /// Core's child-retention budget evicted these Subjects. Release only
    /// their retained row trees; normal tab switches retain scroll state.
    fn release_evicted_transcripts(
        &mut self,
        index: usize,
        changed: &[ferrite_core::cockpit::SubjectUpdate],
    ) {
        let Some(thread) = self.panes[index].thread() else {
            return;
        };
        let Some(activity) = self.cockpit.thread(thread).map(|thread| thread.activity()) else {
            return;
        };
        for change in changed {
            let subject = activity.canonical_subject(&change.subject);
            if !activity
                .subject(&subject)
                .is_some_and(|subject| !subject.retained())
            {
                continue;
            }
            if let Some(transcript) = self.panes[index].release_transcript(&subject) {
                self.transcript_entities.remove(&transcript.entity_id());
            }
        }
    }

    fn scroll_transcript_to_bottom(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(transcript) = self.panes[index].transcript() {
            transcript.update(cx, |transcript, cx| transcript.scroll_to_bottom(cx));
        }
    }

    fn transcript_event(
        &mut self,
        entity: Entity<ferrite_pane::transcript::TranscriptView>,
        event: &ferrite_pane::transcript::TranscriptEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.panes.iter().position(|pane| {
            pane.transcripts
                .values()
                .any(|transcript| transcript == &entity)
        }) else {
            return;
        };
        match event {
            ferrite_pane::transcript::TranscriptEvent::ToggleDisclosure(call) => {
                // A click just opens or closes the row; the keyboard ring is
                // Tab's alone, so a click never leaves one behind.
                self.focus_pane(index);
                self.panes[index].clear_tool_target();
                self.panes[index].toggle_tool(call);
                cx.notify();
            }
            ferrite_pane::transcript::TranscriptEvent::OpenReader { path, line } => {
                // ⌘-click on a path: the Thread's preview, in the Panel.
                self.focus_pane(index);
                self.open_preview_from(index, path.clone(), *line, cx);
            }
        }
    }

    /// Every open Pane's name, from the cache — the head, the L2 and L3
    /// cells read `PaneView::name`, and it moves only at a naming moment.
    fn refresh_names(&mut self) {
        for pane in &mut self.panes {
            if let Some(thread) = pane.thread() {
                pane.name = self.facts.name(thread);
            }
        }
    }

    /// Where this Pane sits in the grid.
    fn index_of(&self, identity: PaneIdentity) -> Option<usize> {
        self.panes.iter().position(|pane| pane.identity == identity)
    }

    /// The focused Composer's line moved: unmute and re-derive the menu.
    /// Menus follow the text — typing `/` or `@` opens, backspacing past
    /// the trigger closes, and a pick's own splice closes through here too.
    fn composer_edited(&mut self, composer: Entity<Composer>, _: &Edited, cx: &mut Context<Self>) {
        if let Some(draft) = self
            .panes
            .iter_mut()
            .find(|pane| pane.composer == composer)
            .and_then(|pane| pane.draft_mut())
        {
            draft.error = None;
        }
        let state = composer.read(cx);
        if slash_token(state.text(), state.cursor()).is_none() {
            self.draft_commands = None;
        }
        // The API reports it as the Thread's `draft` (ADR 0030), so no
        // agent types over the operator.
        let unsent = !state.is_empty();
        if let Some(thread) = self
            .panes
            .iter()
            .find(|pane| pane.composer == composer)
            .and_then(|pane| pane.thread())
        {
            self.cockpit.set_composing(thread, unsent);
        }
        // A picker or a band chip is not text-derived (#11, #25, #29):
        // writing a prompt on its line dismisses it — while the clearing
        // splice that opened it leaves the line empty, and keeps it. The
        // project chip is the exception: its rows read the line as the
        // type-a-path row, so edits re-derive it instead.
        let edited = self
            .panes
            .iter()
            .find(|pane| pane.composer == composer)
            .map(|pane| pane.identity);
        let anchored = self
            .popover
            .as_ref()
            .filter(|open| Some(open.pane) == edited && !open.kind.follows_text());
        let resync =
            anchored.is_some_and(|open| matches!(open.kind, Kind::Band(pane::BandChip::Project)));
        let dismiss = anchored.is_some() && !resync;
        if resync {
            self.sync_band_rows(cx);
        } else if dismiss && !composer.read(cx).is_empty() {
            self.popover = None;
        }
        let popover_on = self.popover.as_ref().map(|open| open.pane);
        if self.suppress_recall_menu_once {
            self.suppress_recall_menu_once = false;
            self.close_text_menu();
        } else {
            self.menu_muted = false;
            self.sync_menu(cx);
        }
        // A keystroke redraws its own Pane (its line, its menu); only a
        // menu moving to another Pane redraws more.
        let menu_moved = [popover_on, self.popover.as_ref().map(|open| open.pane)]
            .into_iter()
            .flatten()
            .any(|pane| Some(pane) != edited);
        match edited {
            Some(edited) if !menu_moved => self.notify_part(parts::Part::Pane(edited), cx),
            _ => cx.notify(),
        }
    }

    /// Close the `/`/`@` menu if that is what the slot holds; a picker or
    /// a band chip is not text-derived and stays.
    fn close_text_menu(&mut self) {
        if self
            .popover
            .as_ref()
            .is_some_and(|open| open.kind.follows_text())
        {
            self.popover = None;
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
        // What a stream can move beyond its own Pane and nav row, before it
        // moves: a frame that changes none of it redraws only those.
        let shared = self.parts.borrow().on.then(|| self.shared_chrome());
        self.poll_navigation(cx);
        self.poll_machines(cx);
        let commands_changed = self
            .draft_commands
            .as_mut()
            .is_some_and(DraftCommands::poll);
        if commands_changed {
            self.sync_menu(cx);
        }
        let frame = self.cockpit.pump();
        self.poll_remote_path(cx);
        self.poll_machine_repo(cx);
        let server_moved = self.server_moved();
        // The Server says when its parked Threads' rows moved.
        let parked_moved = self.cockpit.take_parked_changed();
        if parked_moved {
            self.parked_changed(cx);
        }
        self.apply_thread_rules(&frame, cx);
        self.land(cx);
        self.ring_alerts(cx);
        for pane in &self.panes {
            if let Some(thread) = pane.thread() {
                while let Some((held, prepend)) = self.cockpit.take_retrieved_prompt(thread) {
                    pane.composer.update(cx, |composer, cx| {
                        let (current, mut files) =
                            ferrite_core::prompt_files::split(composer.editable());
                        let (held, restored_files) = ferrite_core::prompt_files::split(held);
                        let text = if current.is_empty() {
                            held
                        } else if prepend {
                            format!("{held}\n\n{current}")
                        } else {
                            format!("{current}\n\n{held}")
                        };
                        if prepend {
                            let mut restored_files = restored_files;
                            restored_files.extend(files);
                            files = restored_files;
                        } else {
                            files.extend(restored_files);
                        }
                        composer.set(ferrite_core::prompt_files::compose(&text, &files), cx);
                    });
                    cx.notify();
                }
            }
        }
        let models_changed = self.cockpit.take_models_changed();
        let tuning_availability_changed = self.popover.as_ref().is_some_and(|open| {
            matches!(open.kind, Kind::Provider | Kind::Effort)
                && open.pane.thread().is_some_and(|thread| {
                    self.cockpit
                        .thread(thread)
                        .and_then(|thread| tuning_hold(&thread))
                        != open.hold
                })
        });
        if models_changed || tuning_availability_changed {
            self.refresh_model_picker(cx);
        }
        let completions = self.cockpit.take_bootstrap_results();
        let startup_changed = !completions.is_empty();
        for completion in completions {
            if let Some(draft) = completion.draft {
                self.finish_draft_start(
                    draft,
                    &completion.prompt,
                    completion.result.map(Some).map_err(|e| e.to_string()),
                    cx,
                );
            }
        }
        // The Server can open or park Threads without a window action (the
        // board and other windows do both). Keep this mirror in roster order
        // before using pane indices. Finish bootstraps first so a Draft keeps
        // its existing Composer when it becomes a Thread.
        let panes_changed = !self.panes.iter().map(|pane| pane.identity).eq(self
            .cockpit
            .roster()
            .panes()
            .iter()
            .copied());
        if panes_changed {
            self.sync_panes(cx);
        }
        let focused = self.focused_thread();
        if focused != self.pr_focus {
            self.pr_focus = focused;
            if let Some(thread) = focused {
                self.check_pr(thread, false, cx);
            }
        }
        self.tend_pr_tab(cx);
        let mut restarted = Vec::new();
        // What the sweep found moved: a fact or a Thread's age (everything
        // redraws), a working Thread (its Pane and the nav), the frame's
        // clock (the frame alone).
        let mut swept_change = false;
        let mut swept_busy: Vec<ThreadId> = Vec::new();
        let mut swept_frame = false;
        let now = cx.background_executor().now();
        if now.duration_since(self.swept) >= SWEEP_INTERVAL {
            self.swept = now;
            // The core logs each restart (`Cockpit::sweep`).
            for restart in self.cockpit.sweep() {
                restarted.push(restart.thread);
            }
            // Redraw only for what moved: a fact, a working Thread's clock
            // (under reduced motion the sweep is its only ride), a nav
            // row's age turning over. An idle window stays undrawn.
            // A Thread's facts (its name, Project, last use) are drawn by
            // its nav row and its Pane: they redraw like a working one.
            let moved = self.facts.tick(&self.cockpit);
            swept_busy = self
                .cockpit
                .threads()
                .into_iter()
                .filter(|thread| {
                    self.cockpit
                        .thread(*thread)
                        .is_some_and(|open| open.busy() || open.working_descendants() > 0)
                })
                .chain(moved)
                .collect();
            let wall = ferrite_core::clock::system_time();
            let aged = self.ages_turn_at.is_some_and(|at| wall >= at);
            swept_frame = self.frame_turn_at.is_some_and(|at| wall >= at);
            swept_change = aged;
            self.refresh_branches(cx);
            // Free disk rides the sweep, once a minute: no wake of its own.
            if self
                .free_disk_asked
                .is_none_or(|at| now.duration_since(at) >= FREE_DISK_EVERY)
            {
                self.free_disk_asked = Some(now);
                self.read_free_disk(cx);
            }
        } else if self.cockpit.wants_worktree_listing() {
            // A Main just finished making a worktree: ask git now, not in
            // up to two seconds, so the header follows without a pause.
            self.refresh_branches(cx);
        }
        // A restart writes a Notice even when no Session streamed this frame —
        // and a failed respawn will never stream again, so this notify is that
        // notice's only ride to the screen.
        if frame.is_empty()
            && restarted.is_empty()
            && !swept_change
            && swept_busy.is_empty()
            && !swept_frame
            && !startup_changed
            && !models_changed
            && !commands_changed
            && !server_moved
            && !parked_moved
            && !panes_changed
        {
            return;
        }
        let mut turn_ended = false;
        for update in &frame {
            if let Some(index) = self.pane_for(update.thread) {
                for (from, to) in &update.redirects {
                    self.panes[index].redirect_subject(from, to);
                }
                self.release_evicted_transcripts(index, &update.subjects);
                let selected = self.panes[index].selected.clone();
                let generation = self
                    .cockpit
                    .thread(update.thread)
                    .and_then(|open| open.activity().subject(&selected))
                    .map(|subject| subject.revision())
                    .unwrap_or(0);
                self.panes[index].select_subject(selected.clone(), generation, cx);
                let content_changed = update
                    .subjects
                    .iter()
                    .any(|change| change.subject == selected && change.content_changed)
                    || (self.panes[index].is_main() && !update.dirty.is_empty());
                if update.activity_changed || content_changed || !update.evicted.is_empty() {
                    self.prune_tool_disclosures(update.thread);
                }
                self.prune_request_forms(index);
                self.panes[index].history_error = self
                    .cockpit
                    .subject_history_error(update.thread, &self.panes[index].selected)
                    .map(str::to_string);
                let unretained = self
                    .cockpit
                    .thread(update.thread)
                    .and_then(|thread| thread.activity().subject(&self.panes[index].selected))
                    .is_some_and(|subject| !subject.retained());
                if unretained && self.panes[index].history_error.is_none() {
                    self.retry_subject_history(index, cx);
                }
            }
            // Native progress can change without appending a transcript row.
            turn_ended |= self.facts.streamed(&self.cockpit, update.thread);
        }
        let precise = restarted.is_empty()
            && !swept_change
            && !server_moved
            && !parked_moved
            && !panes_changed
            && !startup_changed
            && !models_changed
            && !commands_changed
            && shared.is_some_and(|shared| shared == self.shared_chrome());
        for thread in restarted {
            self.facts.acted(&self.cockpit, thread);
        }
        if turn_ended {
            // The agent may have moved the checkout during its turn.
            self.refresh_branches_soon(cx);
        }
        if !precise {
            cx.notify();
            return;
        }
        // Streaming: the Panes it reached and the nav redraw, and the frame
        // with them; every other Pane is reused. The transcripts take their
        // new inputs first, as the Cockpit's own notify has them do
        // (`observe_self`), so they draw in this same frame.
        self.sync_visible_transcripts(cx);
        let rows = !frame.is_empty() || !swept_busy.is_empty();
        for thread in frame.iter().map(|update| update.thread).chain(swept_busy) {
            self.notify_thread_pane(thread, cx);
        }
        if rows {
            self.notify_part(parts::Part::Nav, cx);
        }
        self.notify_frame(cx);
    }

    /// What a stream can change that more than its own Pane and nav row
    /// draw: the account's usage windows (every Pane's meter), the Threads
    /// waiting on the operator (every wall tile's answer keys), the unread
    /// notices and attention (the Needs-you view, each
    /// Pane's head).
    fn shared_chrome(&self) -> String {
        let notifications = self.cockpit.notifications();
        let attention: Vec<ThreadId> = self
            .cockpit
            .threads()
            .into_iter()
            .filter(|thread| notifications.attention(*thread))
            .collect();
        format!(
            "{:?} {:?} {:?} {} {:?}",
            self.cockpit.account_limits(Provider::Claude),
            self.cockpit.account_limits(Provider::Codex),
            self.cockpit.needs_you(),
            notifications.unread(),
            attention,
        )
    }

    /// `refresh_branches`, but never skipped: a refresh already under way
    /// may have read git before the moment that asks, so one more follows
    /// it as soon as it lands.
    fn refresh_branches_soon(&mut self, cx: &mut Context<Self>) {
        if self.branch_refreshing {
            self.branch_refresh_queued = true;
        } else {
            self.refresh_branches(cx);
        }
    }

    /// Refresh checkout labels and their branch status without ever waiting
    /// for Git — or `gh` — in GPUI's pump. The same trip asks git which
    /// worktrees each Thread's repo has, and hands the answer to the core:
    /// that is how a binding follows the agent into a worktree it made
    /// (`workspace::follow`), and the labels re-read on the very next pass.
    /// None of it on a machine: no git here can read its checkouts.
    fn refresh_branches(&mut self, cx: &mut Context<Self>) {
        if self.on_machine() {
            return;
        }
        // A draft has no Thread yet, but its workspace chip names its
        // Project's checkout, which the operator may switch under it.
        let drafted: Vec<ProjectId> = self
            .panes
            .iter()
            .filter_map(|pane| pane.draft())
            .map(|draft| draft.binding.project())
            .collect();
        self.look_up_checkouts(drafted, cx);
        if self.branch_refreshing {
            return;
        }
        // Before anything is read: a creation that finishes from here on
        // is judged by the next pass, not by this one.
        let taken_at = std::time::Instant::now();
        let targets: Vec<_> = self
            .cockpit
            .threads()
            .into_iter()
            .filter_map(|thread| {
                let open = self.cockpit.thread(thread)?;
                let repo = open.workspace().map(|binding| binding.repo().to_path_buf());
                let cwd = ferrite_core::workspace::effective_cwd(
                    open.session_project_root(),
                    open.workspace(),
                )?;
                let project = self
                    .cockpit
                    .peek(thread)
                    .ok()
                    .and_then(|meta| meta.project_id)
                    .and_then(|project| self.cockpit.registry().project(project));
                let directories = project
                    .map(|project| {
                        project
                            .directories()
                            .enumerate()
                            .map(|(index, directory)| {
                                let checkout = if index == 0 { cwd } else { directory };
                                let label = directory
                                    .file_name()
                                    .map(|name| name.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| directory.display().to_string());
                                (SharedString::from(label), checkout.to_path_buf())
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Some((thread, cwd.to_path_buf(), directories, repo))
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        self.branch_refreshing = true;
        cx.spawn(async move |this, cx| {
            let (branches, listings) = cx
                .background_executor()
                .spawn(async move {
                    // One `git worktree list` per repo, however many
                    // Threads share it.
                    let mut listed: std::collections::HashMap<
                        std::path::PathBuf,
                        Option<Vec<std::path::PathBuf>>,
                    > = std::collections::HashMap::new();
                    let mut listings = Vec::new();
                    let branches = targets
                        .into_iter()
                        .map(|(thread, cwd, directories, repo)| {
                            if let Some(repo) = repo {
                                let listing = listed
                                    .entry(repo.clone())
                                    .or_insert_with(|| {
                                        ferrite_core::workspace::worktree_paths(&repo).ok()
                                    })
                                    .clone();
                                if let Some(listing) = listing {
                                    listings.push((thread, listing));
                                }
                            }
                            let status = ferrite_core::workspace::branch_status(&cwd);
                            let project_branches = directories
                                .into_iter()
                                .filter_map(|(label, directory)| {
                                    ferrite_core::workspace::checkout_branch(&directory)
                                        .map(|branch| (label, SharedString::from(branch)))
                                })
                                .collect();
                            (thread, status, project_branches)
                        })
                        .collect::<Vec<_>>();
                    (branches, listings)
                })
                .await;
            this.update(cx, |view, cx| {
                view.branch_refreshing = false;
                let labels = view.facts.set_branches(
                    branches
                        .iter()
                        .map(|(thread, status, _)| (*thread, status.clone()))
                        .collect(),
                );
                let roots = view.facts.set_project_branches(
                    branches
                        .into_iter()
                        .map(|(thread, _, project_branches)| (thread, project_branches))
                        .collect(),
                );
                let mut moved = false;
                for (thread, listing) in listings {
                    if view.cockpit.worktrees_listed(thread, listing, taken_at) {
                        moved = true;
                        // Into a worktree: its branch is another PR's.
                        view.check_pr(thread, false, cx);
                    }
                }
                // Git said what it said last time: nothing to redraw. A
                // label that moved is its Thread's: its Pane and its nav
                // row. A binding that moved, everything.
                if moved {
                    cx.notify();
                } else if !labels.is_empty() || !roots.is_empty() {
                    for thread in labels.iter().chain(&roots) {
                        view.notify_thread_pane(*thread, cx);
                    }
                    view.notify_part(parts::Part::Nav, cx);
                    view.notify_frame(cx);
                }
                if moved || std::mem::take(&mut view.branch_refresh_queued) {
                    // The labels above were read for the old cwd, or
                    // before a turn ended; go straight back for the new.
                    view.refresh_branches(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Transcript membership changes only through the pump. Prune stale
    /// expanded call ids here once per changed Thread, never per frame.
    fn prune_tool_disclosures(&mut self, thread: ThreadId) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let mut valid = std::collections::HashSet::new();
        if let Some(transcript) = self
            .cockpit
            .thread(thread)
            .and_then(|open| open.activity().subject(&self.panes[index].selected))
            .map(|subject| subject.transcript())
        {
            for block in transcript.blocks() {
                match &block.body {
                    ferrite_core::transcript::Body::Tool(tool) => {
                        valid.insert(pane::DisclosureId::Tool(tool.call.clone()));
                        valid.insert(pane::DisclosureId::Diff(tool.call.clone()));
                        valid.insert(pane::DisclosureId::Group(tool.call.clone()));
                    }
                    ferrite_core::transcript::Body::Thinking(_) => {
                        valid.insert(pane::DisclosureId::Reasoning(block.id));
                    }
                    _ => {}
                }
            }
        }
        self.panes[index].prune_tools(&valid);
    }

    /// One cell of the board, as the window is right now: the focused
    /// Pane's rect (`pane_rects`), else the first, else the whole board.
    /// Size is the only input semantic zoom takes — there is no mode to
    /// switch, and the nav is simply part of the size: opening it can
    /// legitimately drop Panes a Level (#21).
    fn cell(&self, window: &Window) -> Cell {
        let rects = self.pane_rects(window);
        let focused = self.focused();
        let rect = rects
            .iter()
            .find(|(index, _)| *index == focused)
            .or_else(|| rects.first())
            .map_or_else(|| self.board_bounds(window), |(_, rect)| *rect);
        Cell::new(rect.w.max(0.0), rect.h.max(0.0))
    }

    /// Whether something floats over the cockpit: a menu, a popover or the
    /// Settings panel. The titlebar's drag region reads it — a region that
    /// lies under an open overlay would answer Windows' hit test as the
    /// window frame, and the row under the pointer would never see the
    /// press (`titlebar.rs`).
    fn overlay_open(&self) -> bool {
        self.settings_open
            || self.project_editor.is_some()
            || self.popover.is_some()
            || self.context_checks.is_some()
            || self.plan_card.is_some()
            || self.context_menu.is_some()
            || self.sheet_float_open()
    }

    /// How much of the window the nav holds right now: the full column, or
    /// nothing — folded by cmd-b, or by a window too narrow for the column
    /// (`nav_railed`).
    fn nav_width(&self) -> f32 {
        if self.nav_measure_full.get() {
            return nav::WIDTH;
        }
        if self.nav_railed() {
            nav::FOLDED_WIDTH
        } else {
            nav::WIDTH
        }
    }

    /// The column's width mid-ride (F-12): the cmd-B tween's value this
    /// frame, or a scene's held progress (`fixture_hold_nav`) along the
    /// same curve; `None` at rest.
    fn nav_ride_now(&self, cx: &gpui::App) -> Option<f32> {
        if let Some(progress) = self.nav_hold {
            let (from, to) = match self.nav_tween {
                Some(tween) => (tween.from, tween.to),
                None if self.nav_railed() => (nav::WIDTH, nav::FOLDED_WIDTH),
                None => (nav::FOLDED_WIDTH, nav::WIDTH),
            };
            return Some(ferrite_ui::motion::lerp(
                from,
                to,
                ferrite_ui::motion::RESIZE.progress(progress.clamp(0.0, 1.0)),
            ));
        }
        let now = cx.background_executor().now();
        let reduced = ferrite_ui::motion::reduced_motion(cx);
        self.nav_tween
            .filter(|tween| tween.running(now, reduced))
            .map(|tween| tween.value(now, reduced))
    }

    /// Hold the sidebar's ride at `progress` (0 → 1 along `motion::RESIZE`)
    /// so a scene can capture the board mid-ride; `None` lets it go.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn fixture_hold_nav(&mut self, progress: Option<f32>) {
        self.nav_hold = progress;
    }

    /// Whether the nav draws as the rail: the operator folded it, or the
    /// window is too narrow for the column and cmd-B has not overridden it.
    fn nav_railed(&self) -> bool {
        self.nav_collapsed || (self.nav_auto_rail.get() && !self.nav_forced_open)
    }

    /// Rule 2.7.7, once per frame: fold the nav to the rail — shown, never
    /// saved — while the board beside the full column would be narrower
    /// than `NAV_AUTO_RAIL_BOARD_W`, or a board cell narrower than the tile
    /// floor (`TILE_FLOOR_WIDTH`) that folding would lift it over. It unfolds only once both
    /// clear their threshold by `NAV_AUTO_RAIL_HYSTERESIS`. The operator's
    /// cmd-B override (`nav_forced_open`) outranks it either way. The board
    /// beside the 264px column is the window less the column and what the
    /// board leaves at its right once the panel's ride ends
    /// (`board_right_rest`: the panel, or `BOARD_PAD`), so with the panel
    /// hidden the nav folds under 832px (the card keeps
    /// 560px, as it did beside the 36ch column).
    fn update_auto_rail(&mut self, window: &Window) {
        use ferrite_core::docview::TILE_FLOOR_WIDTH;
        use ferrite_ui::theme::{NAV_AUTO_RAIL_BOARD_W, NAV_AUTO_RAIL_HYSTERESIS};
        let held = self.nav_auto_rail.get();
        let slack = if held { NAV_AUTO_RAIL_HYSTERESIS } else { 0.0 };
        let width = f32::from(window.viewport_size().width);
        let right = self.board_right_rest();
        let board = width - nav::WIDTH - right;
        let narrowest = |view: &Self| {
            view.pane_rects(window)
                .iter()
                .map(|(_, rect)| rect.w)
                .fold(f32::INFINITY, f32::min)
        };
        self.nav_measure_full.set(true);
        let full = narrowest(self);
        self.nav_measure_full.set(false);
        let railed_board = layout::Rect {
            w: width - nav::FOLDED_WIDTH - right,
            ..self.board.get()
        };
        // What the narrowest cell would be beside the rail: the same tree,
        // every column widened by the board's gain.
        let gain = railed_board.w / board.max(1.0);
        let railed = full * gain;
        let cramped = full < TILE_FLOOR_WIDTH + slack && railed >= TILE_FLOOR_WIDTH;
        let rail = board < NAV_AUTO_RAIL_BOARD_W + slack || cramped;
        self.nav_auto_rail.set(rail);
        // `board_bounds` cached the full-column board while measuring.
        self.board_bounds(window);
    }

    /// The level this cockpit is rendering at right now: the board's one
    /// Level (`board_level`) — fullscreen forces Transcript inside it (#20).
    /// Routed here, not in render, so the pointer math (`block_at`) reads
    /// the same level the frame drew.
    fn level_now(&self, window: &Window) -> Level {
        self.board_level(window)
    }

    /// **One Level per board** (rule 2.3.5): the smallest `Level::for_cell`
    /// over every Pane's rect, so a board never mixes tiers and fewer Panes
    /// never draw at a lower tier than more. Fullscreen forces Transcript (a
    /// whole-window cell would pick L1 at any sane size anyway; the force
    /// keeps "fullscreen = L1 regardless" true on a tiny window too).
    ///
    /// Hysteresis: the board steps down the moment any cell falls under a
    /// threshold, but steps up only once every cell clears the next tier's
    /// threshold by `LEVEL_HYSTERESIS` on both axes — a resize that hovers
    /// at the edge never flickers between tiers.
    fn board_level(&self, window: &Window) -> Level {
        if self.projects.coordinator_shown() || self.cockpit.roster().fullscreen().is_some() {
            return Level::Transcript;
        }
        // Mid-ride the board is measured where the ride ends (F-12) — the
        // sidebar's, and every reader's slide: the Level is the
        // destination's from the first frame, never flapping as a width
        // sweeps through a threshold.
        self.nav_measure_rest.set(true);
        let rects = self.pane_rects(window);
        self.nav_measure_rest.set(false);
        self.board_bounds(window);
        let lowest = |slack: f32| {
            rects
                .iter()
                .map(|(_, rect)| Level::for_cell(Cell::new(rect.w - slack, rect.h - slack)))
                .min()
        };
        let Some(plain) = lowest(0.0) else {
            let bounds = self.board_bounds(window);
            return Level::for_cell(Cell::new(bounds.w, bounds.h));
        };
        let held = self.board_level.get();
        let level = if plain <= held {
            plain
        } else {
            held.max(lowest(ferrite_ui::theme::LEVEL_HYSTERESIS).unwrap_or(held))
        };
        self.board_level.set(level);
        level
    }

    /// The board the cards lay out in, in window coordinates (theme WP-C):
    /// flush right of the nav, from under the head row
    /// (`titlebar::board_top`) to `BOARD_PAD` short of the bottom bar and
    /// `board_right` short of the window's right edge (the panel, or the
    /// gutter).
    fn board_bounds(&self, window: &Window) -> layout::Rect {
        use ferrite_ui::theme::{BOARD_PAD, STATUS_BAR_H};
        let viewport = window.viewport_size();
        let top = crate::titlebar::board_top();
        let left = self.board_left();
        let bounds = layout::Rect {
            x: left,
            y: top,
            w: (f32::from(viewport.width) - left - self.board_right()).max(0.0),
            h: (f32::from(viewport.height) - top - STATUS_BAR_H - BOARD_PAD).max(0.0),
        };
        self.board.set(bounds);
        bounds
    }

    /// Where the board's left edge sits: the nav's width this frame (no
    /// seam: the card's edge separates). While cmd-B rides the column the
    /// board rides with it, every frame (F-12).
    fn board_left(&self) -> f32 {
        if self.nav_measure_full.get() {
            nav::WIDTH
        } else if self.nav_measure_rest.get() {
            self.nav_width()
        } else {
            self.nav_ride.get().unwrap_or_else(|| self.nav_width())
        }
    }

    /// A Group's tree in the board as the last frame laid it out: the
    /// operator's stored arrangement, or the default grid (`Tree::grid`).
    #[cfg(test)]
    fn group_layout(&self, group: GroupId) -> Option<Tree> {
        self.cockpit.group_layout(group, self.rest_board())
    }

    /// The board as it is where the sidebar rests: what a default grid's
    /// shape is chosen in, so a ride (F-12) never flips it mid-fold — only
    /// the widths ride.
    fn rest_board(&self) -> layout::Rect {
        let board = self.board.get();
        let rest_left = self.nav_width();
        layout::Rect {
            x: rest_left,
            w: (board.w + board.x - rest_left).max(0.0),
            ..board
        }
    }

    /// The board on screen, when a tree lays it out: the Group being
    /// shown, or Solo — or a fullscreen Pane, its own cell filling the
    /// board — while its one Pane has a reader slot. A plain Solo or
    /// fullscreen Pane has none.
    fn board(&self) -> Option<Board> {
        if let Some(shown) = self.cockpit.roster().fullscreen() {
            return self
                .index_of(shown)
                .filter(|index| self.has_reader(*index))
                .map(|_| Board::Solo(shown));
        }
        if let Some((owner, _)) = self.solo_pair() {
            return Some(Board::Solo(owner));
        }
        match self.cockpit.roster().view() {
            View::Group(group) => Some(Board::Group(group)),
            View::Solo => match self.visible_indices().as_slice() {
                [index] if self.has_reader(*index) => {
                    Some(Board::Solo(self.panes[*index].identity))
                }
                _ => None,
            },
        }
    }

    /// A board's tree as it should draw right now: the one mid-drag, else
    /// the operator's arrangement fitted to what is open — a Group's
    /// members (or the default grid over the board's bounds), a pending
    /// draft spliced in under a stand-in id since a draft is no Thread, and
    /// a reader slot for every visible Pane with a document open or sliding
    /// shut. A draft on a default grid joins the grid rather than splitting
    /// one Pane; a reader the tree has not placed yet opens beside its
    /// Pane, on the right. Every slot is at rest here: `slid_tree` draws a
    /// slide.
    fn board_tree(&self, board: Board) -> Option<Tree> {
        if let Some(drag) = self.seam_drag.as_ref().filter(|drag| drag.board == board) {
            return Some(drag.tree.clone());
        }
        let bounds = self.rest_board();
        let readers: Vec<ThreadId> = self
            .visible_indices()
            .into_iter()
            .filter(|index| self.has_reader(*index))
            .filter_map(|index| self.panes[index].thread())
            .collect();
        let (base, mut panes) = match board {
            Board::Group(group) => {
                let drafts: Vec<ThreadId> = self
                    .cockpit
                    .visible()
                    .into_iter()
                    .filter(|identity| matches!(identity, PaneIdentity::Draft(_)))
                    .map(pane_leaf)
                    .collect();
                // Only a member with a Pane takes a slot: one without
                // would leave a hole in the board, no card to paint it.
                let seated = |member: &ThreadId| self.pane_for(*member).is_some();
                if readers.is_empty()
                    && !drafts.is_empty()
                    && !self.cockpit.groups().has_custom_layout(group)
                {
                    let mut all = self.cockpit.groups().get(group)?.members.clone();
                    all.retain(seated);
                    all.extend(drafts.iter().copied());
                    return Some(Tree::grid(&all, bounds));
                }
                let persisted = self.cockpit.group_layout(group, bounds)?;
                let mut panes = persisted.leaves();
                panes.retain(seated);
                panes.extend(drafts);
                let base = match self.board_layouts.get(&board) {
                    Some(session) if !readers.is_empty() => session.clone(),
                    _ => persisted,
                };
                (base, panes)
            }
            Board::Solo(identity) => {
                let base = self.board_layouts.get(&board).cloned().unwrap_or_default();
                let mut panes = vec![pane_leaf(identity)];
                // A Thread opened beside the Solo Pane takes the right half.
                if let Some((owner, companion)) = self.solo_pair() {
                    if owner == identity {
                        panes.push(pane_leaf(companion));
                    }
                }
                (base, panes)
            }
        };
        let readers: Vec<(ThreadId, ThreadId)> = readers
            .into_iter()
            .map(|owner| (owner, reader_leaf(owner)))
            .filter(|(owner, _)| panes.contains(owner))
            .collect();
        let mut tree = base;
        let wanted: std::collections::BTreeSet<ThreadId> = panes
            .iter()
            .copied()
            .chain(readers.iter().map(|(_, reader)| *reader))
            .collect();
        for stale in tree
            .leaves()
            .into_iter()
            .filter(|leaf| !wanted.contains(leaf))
        {
            tree.remove(stale);
        }
        panes.retain(|leaf| !tree.contains(*leaf));
        for leaf in panes {
            tree.insert(leaf);
        }
        for (owner, reader) in readers {
            if !tree.contains(reader) {
                tree.split_share(owner, Edge::Right, reader, READER_SHARE);
            }
        }
        Some(tree)
    }

    /// Keep an arrangement the operator made on a board. A Group persists
    /// its Threads' part of it (readers and drafts are not members); the
    /// whole tree, reader slots included, is kept for the session while
    /// any reader is in it.
    fn commit_board(&mut self, board: Board, tree: Tree) {
        let slots: Vec<(ThreadId, Slot)> = tree
            .leaves()
            .into_iter()
            .map(|leaf| (leaf, leaf_slot(leaf)))
            .collect();
        if matches!(board, Board::Solo(_))
            || slots
                .iter()
                .any(|(_, slot)| matches!(slot, Slot::Reader(_)))
        {
            self.board_layouts.insert(board, tree.clone());
        } else {
            self.board_layouts.remove(&board);
        }
        if let Board::Group(group) = board {
            let mut threads = tree;
            for (leaf, slot) in slots {
                if !matches!(slot, Slot::Pane(PaneIdentity::Thread(_))) {
                    threads.remove(leaf);
                }
            }
            if let Err(error) = self.cockpit.set_group_layout(group, threads) {
                self.group_error = Some(error.to_string().into());
            }
        }
    }

    /// Whether the board has somewhere to move a slot to — a head is a
    /// drag handle only then.
    fn board_is_movable(&self) -> bool {
        self.board()
            .and_then(|board| self.board_tree(board))
            .is_some_and(|tree| tree.leaves().len() > 1)
    }

    /// Every visible Pane's rect on the board, in window coordinates —
    /// from the board's tree as this frame draws it (or, measuring at rest,
    /// where every reader's slide ends), or the one Solo cell. Reader slots
    /// are no Panes and are left out. Fullscreen is the whole board, split
    /// with its reader.
    fn pane_rects(&self, window: &Window) -> Vec<(usize, layout::Rect)> {
        let bounds = self.board_bounds(window);
        if self.projects.coordinator_shown() {
            return self
                .projects
                .coordinator_thread()
                .and_then(|thread| self.pane_for(thread))
                .map(|index| {
                    vec![(
                        index,
                        layout::Rect {
                            y: bounds.y + projects::TASKS_HEADER_H,
                            h: (bounds.h - projects::TASKS_HEADER_H).max(0.),
                            ..bounds
                        },
                    )]
                })
                .unwrap_or_default();
        }
        if let Some(board) = self.board() {
            let Some(tree) = self.board_tree(board) else {
                return Vec::new();
            };
            let tree = if self.nav_measure_rest.get() || self.nav_measure_full.get() {
                self.rest_tree(tree)
            } else {
                self.slid_tree(tree, bounds)
            };
            return tree
                .rects(bounds, ferrite_ui::theme::BOARD_SEAM)
                .into_iter()
                .filter_map(|(leaf, rect)| match leaf_slot(leaf) {
                    Slot::Pane(identity) => self.index_of(identity).map(|index| (index, rect)),
                    Slot::Reader(_) => None,
                })
                .collect();
        }
        if let Some(identity) = self.cockpit.roster().fullscreen() {
            return self
                .index_of(identity)
                .map(|index| vec![(index, bounds)])
                .unwrap_or_default();
        }
        // Solo is one Pane — or a pending pair (a draft beside its Thread),
        // laid out on the same default grid a Group gets.
        let visible = self.visible_indices();
        let (columns, rows) = layout::grid_shape(visible.len(), self.rest_board());
        let gap = ferrite_ui::theme::BOARD_SEAM;
        let height = (bounds.h - (rows.max(1) - 1) as f32 * gap) / rows.max(1) as f32;
        visible
            .chunks(columns.max(1))
            .enumerate()
            .flat_map(|(row, line)| {
                let width = (bounds.w - (line.len() - 1) as f32 * gap) / line.len() as f32;
                line.iter().enumerate().map(move |(column, index)| {
                    (
                        *index,
                        layout::Rect {
                            x: bounds.x + column as f32 * (width + gap),
                            y: bounds.y + row as f32 * (height + gap),
                            w: width.max(0.0),
                            h: height,
                        },
                    )
                })
            })
            .collect()
    }

    /// The level one Pane draws at: the board's one Level (`board_level`).
    /// `index` is kept so call sites read as the Pane they ask about.
    #[cfg(test)]
    fn level_of(&self, _index: usize, window: &Window) -> Level {
        self.board_level(window)
    }

    /// The palette's `checks` command (F-11): the checks card for the
    /// focused Thread's PR, hung from its Pane's top edge. Nothing when the
    /// PR reports no checks.
    pub(crate) fn open_checks(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        let has_checks = self.facts.pr(thread).is_some_and(|pr| pr.checks.is_some());
        if !has_checks {
            return;
        }
        self.popover = None;
        self.context_menu = None;
        self.plan_card = None;
        self.context_checks = Some(thread);
        cx.notify();
    }

    /// The palette's `plan` command (F-11): the focused Thread's plan as a
    /// card — its steps in order, the done ones checked, the one being
    /// worked marked — hung from its Pane's top edge. Nothing without a plan.
    pub(crate) fn open_plan(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        if self
            .cockpit
            .thread(thread)
            .and_then(|open| open.transcript().todos())
            .is_none()
        {
            return;
        }
        self.popover = None;
        self.context_menu = None;
        self.context_checks = None;
        self.plan_card = Some(thread);
        cx.notify();
    }

    /// The palette's `install CLI updates` command (F-11): every provider
    /// CLI with a newer release ready installs. Nothing while one is already installing.
    pub(crate) fn install_cli_updates(&mut self, cx: &mut Context<Self>) {
        use crate::cli_updates::Badge;
        let (ready, what) = match self.cli_updates.badge() {
            Some(Badge::Ready(what)) => (true, what),
            Some(Badge::Installing(what)) => (false, what),
            None => return,
        };
        if !ready {
            ferrite_core::log!("{what}");
            return;
        }
        for provider in self.cli_updates.ready() {
            self.update_cli(provider, cx);
        }
        cx.notify();
    }

    /// The plan card (`open_plan`): the float grammar's head (`plan · 3/4`),
    /// then one row per step — done `TEXT_DIM` behind a dim check, the
    /// step being worked `TEXT_STRONG` behind the strong `❯`, the rest
    /// `TEXT` behind a faint ring.
    fn plan_card_element(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
        let thread = self.plan_card?;
        if self.focused_thread() != Some(thread) {
            return None;
        }
        let open = self.cockpit.thread(thread)?;
        let transcript = open.transcript();
        let todos = transcript.todos()?;
        let current = transcript.current_task();
        let at = self.card_corner(
            format!("plan-{}", thread.get()),
            CardFrom::Head(PaneIdentity::Thread(thread)),
            window,
        )?;
        let mut card = ferrite_ui::menu::float()
            .min_w(px(CHECKS_CARD_W))
            .max_w(px(CHECKS_CARD_MAX_W))
            .pb(px(FLOAT_PAD))
            .child(
                ferrite_ui::menu::head("Plan").child(SharedString::from(format!(
                    "{}/{}",
                    todos.done, todos.total
                ))),
            );
        let mut marked = false;
        for (index, step) in transcript.progress().plan.iter().enumerate() {
            let done = step.status == ferrite_core::progress::StepStatus::Completed;
            let working = !done && !marked && current == Some(step.text.as_str());
            marked |= working;
            let mark = if done {
                ferrite_ui::components::glyph("\u{2713}", TEXT_DIM).into_any_element()
            } else if working {
                ferrite_ui::components::prompt_mark(TEXT_STRONG)
            } else {
                div()
                    .text_color(rgb(TEXT_FAINT))
                    .child("\u{25cb}")
                    .into_any_element()
            };
            card = card.child(
                div()
                    .debug_selector(move || format!("plan-step-{index}"))
                    .flex()
                    .items_center()
                    .h(px(FLOAT_ROW_H))
                    .px(px(FLOAT_PAD_X))
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .w(px(SPACE_4 + SPACE_1))
                            .child(mark),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(if done {
                                TEXT_DIM
                            } else if working {
                                TEXT_STRONG
                            } else {
                                TEXT
                            }))
                            .child(SharedString::from(step.text.clone())),
                    ),
            );
        }
        let card = card
            .id("plan-card")
            .debug_selector(|| "plan-card".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .on_mouse_down_out(cx.listener(|view, _: &MouseDownEvent, _, cx| {
                if view.plan_card.take().is_some() {
                    cx.notify();
                }
            }));
        Some(
            deferred(
                anchored()
                    .anchor(gpui::Anchor::TopRight)
                    .position(at)
                    .snap_to_window_with_margin(px(GRID_PAD))
                    .child(ferrite_ui::motion::menu_in(
                        "plan-card-in",
                        card,
                        ferrite_ui::motion::Opens::Down,
                    )),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// More than one Pane is on the board and none fills it: every Composer
    /// is the grid's one fixed line (C4).
    fn grid_board(&self) -> bool {
        self.cockpit.roster().fullscreen().is_none() && self.visible_indices().len() > 1
    }

    /// A draft's discard ×, wired to cmd-w's own close, its tooltip naming
    /// the key where one is bound.
    fn draft_discard(
        &self,
        draft: ferrite_core::roster::DraftId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let button = pane::draft_close_button(draft)
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.close_pane(PaneIdentity::Draft(draft), cx);
            }))
            .into_any_element();
        pane::draft_discard(
            draft,
            button,
            ferrite_ui::components::bound_chord("cockpit::CloseThread"),
        )
        .into_any_element()
    }

    /// The Panes on screen, as indices into the mirror — the roster's
    /// `visible`, in its order.
    fn visible_indices(&self) -> Vec<usize> {
        if self.projects.coordinator_shown() {
            return self
                .projects
                .coordinator_thread()
                .and_then(|thread| self.pane_for(thread))
                .into_iter()
                .collect();
        }
        // A Solo pair (`open_beside`) shows both of its Panes.
        if let Some((owner, companion)) = self.solo_pair() {
            return [owner, companion]
                .into_iter()
                .filter_map(|identity| self.index_of(identity))
                .collect();
        }
        self.cockpit
            .visible()
            .into_iter()
            .filter_map(|identity| self.index_of(identity))
            .collect()
    }

    /// Open the inline editor on a title, seeded with the title as it
    /// stands — a rename starts from what is there, not from an empty line.
    /// A Group whose id no longer resolves has no title to edit, and the
    /// press is dropped rather than opening an editor over nothing.
    fn start_rename(&mut self, target: RenameTarget, cx: &mut Context<Self>) {
        let title = match target {
            RenameTarget::Group(group) => self
                .cockpit
                .groups()
                .get(group)
                .map(|group| group.display_title()),
            // A Thread with no stored title shows its generated one, so
            // that is what the editor opens on: the operator edits the name
            // they can see.
            RenameTarget::Thread(thread)
            | RenameTarget::PaneTitle(thread)
            | RenameTarget::ViewTitle(thread) => Some(self.facts.name(thread).to_string()),
        };
        let Some(title) = title else {
            return;
        };
        let editor = cx.new(ferrite_pane::composer::Composer::new);
        editor.update(cx, |editor, cx| {
            editor.set_role(ferrite_pane::composer::Role::Field(None), cx);
            editor.set(title, cx)
        });
        self.rename = Some((target, editor));
        cx.notify();
    }

    /// Close the editor. `save` commits — except on a blank line, which is
    /// not a rename to an empty title but a rename the operator abandoned.
    /// A refusal from the store lands where every other Group error does;
    /// the editor still closes, so a row is never stuck in edit.
    fn finish_rename(&mut self, save: bool, cx: &mut Context<Self>) {
        let Some((target, editor)) = self.rename.take() else {
            return;
        };
        if !save {
            cx.notify();
            return;
        }
        let title = editor.update(cx, |line, cx| line.take(cx));
        if title.trim().is_empty() {
            cx.notify();
            return;
        }
        let result = match target {
            RenameTarget::Group(group) => self
                .cockpit
                .apply_group(GroupChange::Rename { group, title })
                .map(|_| ())
                .map_err(|error| error.to_string()),
            RenameTarget::Thread(thread)
            | RenameTarget::PaneTitle(thread)
            | RenameTarget::ViewTitle(thread) => self
                .cockpit
                .rename_thread(thread, &title)
                .map_err(|error| error.to_string()),
        };
        if let Err(error) = result {
            self.group_error = Some(error.into());
        } else {
            if let RenameTarget::Thread(thread)
            | RenameTarget::PaneTitle(thread)
            | RenameTarget::ViewTitle(thread) = target
            {
                self.facts.renamed(&self.cockpit, thread);
                self.refresh_names();
            }
            self.group_error = None;
        }
        cx.notify();
    }

    /// A Group's title cell: the live editor while this Group is the one
    /// being renamed, otherwise the title with a press that opens it. That
    /// press stops there — the row underneath would otherwise enter the
    /// Group on the way to renaming it.
    fn editable_group_title(
        &self,
        group: GroupId,
        title: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some((RenameTarget::Group(editing), editor)) = &self.rename {
            if *editing == group {
                return rename_field(editor.clone())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
                    )
                    .into_any_element();
            }
        }
        nav::rename_target_group(group, title)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    // A single click belongs to the row under the title —
                    // it enters the Group. Only the second click renames,
                    // and only then is the press worth swallowing.
                    if event.click_count < 2 {
                        return;
                    }
                    cx.stop_propagation();
                    view.start_rename(RenameTarget::Group(group), cx);
                }),
            )
            .into_any_element()
    }

    /// Summon the context menu for `target` at the pointer. Whatever else
    /// was up (a popover, a card, a rename) closes: one floating thing at a
    /// time.
    fn open_context_menu(&mut self, target: MenuTarget, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.popover = None;
        self.context_checks = None;
        if self.rename.is_some() {
            self.finish_rename(false, cx);
        }
        let rows = self.context_rows(target);
        self.context_menu = Some(ContextMenu {
            target,
            at,
            rows,
            armed: None,
        });
        cx.notify();
    }

    /// The rows a target offers. `None` is a gap between groups of rows.
    fn context_rows(&self, target: MenuTarget) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let mut rows: Vec<Option<(menu::Item, MenuVerb)>> = Vec::new();
        let fullscreen =
            ferrite_ui::components::bound_chord("cockpit::ToggleFullscreen").unwrap_or_default();
        match target {
            // The nav row: where a Thread lives in the roster — open it,
            // park it, move it between Groups, delete it.
            MenuTarget::Thread(thread) => {
                let shown = self.pane_for(thread).is_some();
                let live = self.cockpit.thread(thread).is_some();
                let grouped = self.cockpit.groups().of(thread).is_some();
                rows.push(Some((menu::Item::new("Rename"), MenuVerb::Rename)));
                if self.cockpit.roster().focused_thread() != Some(thread) {
                    rows.push(Some((
                        menu::Item::new(if live { "Open" } else { "Resume" }),
                        MenuVerb::Focus,
                    )));
                }
                if shown {
                    rows.push(Some((
                        menu::Item::new("Toggle fullscreen").shortcut(fullscreen.clone()),
                        MenuVerb::Fullscreen,
                    )));
                }
                rows.push(None);
                rows.push(Some((
                    menu::Item::new("New thread in this project").shortcut("cmd-T"),
                    MenuVerb::NewThread,
                )));
                // A machine's path is not in this computer's Finder.
                if !self.on_machine() {
                    rows.push(Some((
                        menu::Item::new("Reveal in Finder"),
                        MenuVerb::Reveal,
                    )));
                }
                rows.push(Some((menu::Item::new("Copy path"), MenuVerb::CopyPath)));
                rows.push(None);
                rows.push(Some((
                    menu::Item::new("Refresh PR status").shortcut(
                        ferrite_ui::components::bound_chord("cockpit::RefreshPrStatus")
                            .unwrap_or_default(),
                    ),
                    MenuVerb::RefreshPr,
                )));
                if self.facts.pr(thread).is_some_and(|pr| pr.url.is_some()) {
                    rows.push(Some((
                        menu::Item::new("Open PR in browser").shortcut(
                            ferrite_ui::components::bound_chord("cockpit::OpenPrInBrowser")
                                .unwrap_or_default(),
                        ),
                        MenuVerb::OpenPr,
                    )));
                }
                rows.push(None);
                if live {
                    rows.push(Some((
                        menu::Item::new("Park thread").shortcut(if shown { "cmd-W" } else { "" }),
                        MenuVerb::Park,
                    )));
                }
                if grouped {
                    rows.push(Some((menu::Item::new("Leave group"), MenuVerb::LeaveGroup)));
                }
                rows.push(Some((
                    menu::Item::new("Delete thread").destructive(),
                    MenuVerb::Delete,
                )));
            }
            // The transcript: what is on screen — copy it, name it, size
            // it, and the Pane's own park/close. Deleting a Thread is the
            // nav's act; the transcript never offers it.
            MenuTarget::Pane(thread) => {
                let grouped = self.cockpit.groups().of(thread).is_some();
                let selected = self.native_copy.is_some();
                rows.push(Some((
                    menu::Item::new("Copy")
                        .shortcut("cmd-C")
                        .disabled(!selected),
                    MenuVerb::CopySelection,
                )));
                rows.push(Some((
                    menu::Item::new("Copy transcript"),
                    MenuVerb::CopyTranscript,
                )));
                rows.push(None);
                rows.push(Some((menu::Item::new("Rename"), MenuVerb::Rename)));
                rows.push(Some((
                    menu::Item::new("Toggle fullscreen").shortcut(fullscreen),
                    MenuVerb::Fullscreen,
                )));
                rows.push(None);
                if !self.on_machine() {
                    rows.push(Some((
                        menu::Item::new("Reveal in Finder"),
                        MenuVerb::Reveal,
                    )));
                }
                rows.push(Some((menu::Item::new("Copy path"), MenuVerb::CopyPath)));
                rows.push(None);
                // cmd-w parks in one act, Group member or not
                // (`Cockpit::park_thread`); leaving without parking is its
                // own row.
                rows.push(Some((
                    menu::Item::new("Park thread").shortcut("cmd-W"),
                    MenuVerb::Park,
                )));
                if grouped {
                    rows.push(Some((menu::Item::new("Leave group"), MenuVerb::LeaveGroup)));
                }
            }
            MenuTarget::Group(group) => {
                rows.push(Some((menu::Item::new("Rename group"), MenuVerb::Rename)));
                if self.cockpit.roster().view() != View::Group(group) {
                    rows.push(Some((menu::Item::new("Open group"), MenuVerb::EnterGroup)));
                }
                rows.push(Some((
                    menu::Item::new("New thread in this group").shortcut("cmd-T"),
                    MenuVerb::NewThread,
                )));
                rows.push(None);
                rows.push(Some((
                    menu::Item::new("Dissolve group").destructive(),
                    MenuVerb::DissolveGroup,
                )));
            }
            // A Project heading: its tasks (the operator's window only).
            MenuTarget::Project(_) => {
                rows.push(Some((
                    menu::Item::new("Open tasks for this project"),
                    MenuVerb::OpenTasks,
                )));
            }
            // The Parked view's row: the view, and the one bulk act the
            // list exists for — clearing the parked history it holds.
            MenuTarget::Parked => {
                rows.push(Some((
                    menu::Item::new(if self.nav_view == nav::NavView::Parked {
                        "Hide parked threads"
                    } else {
                        "Show parked threads"
                    }),
                    MenuVerb::ToggleParked,
                )));
                rows.push(None);
                rows.push(Some((
                    menu::Item::new("Delete parked threads")
                        .destructive()
                        .disabled(self.deletable_parked().is_empty()),
                    MenuVerb::DeleteAllParked,
                )));
            }
            // The Review's diff: the pick — comment on it, read its file,
            // copy it. The reader reads this computer's files only.
            MenuTarget::Review { .. } => {
                rows.push(Some((
                    menu::Item::new("Comment").shortcut("c"),
                    MenuVerb::Comment,
                )));
                rows.push(Some((
                    menu::Item::new("Open file at this line").disabled(self.on_machine()),
                    MenuVerb::OpenAtLine,
                )));
                rows.push(Some((menu::Item::new("Copy"), MenuVerb::CopySelection)));
            }
        }
        rows
    }

    /// The Threads the Parked section lists and counts: every parked
    /// Thread the Project filter admits — a parked Group member too (the
    /// prototype's `parked 3` holds Release notes, an Everything member),
    /// and a parked tile on the wall is still a parked Thread.
    fn parked_threads(&self) -> Vec<ThreadId> {
        self.facts
            .parked()
            .iter()
            .copied()
            .filter(|thread| self.admitted(*thread))
            .collect()
    }

    /// What "Delete parked threads" deletes: the parked Threads in no
    /// Group. A parked member's Group is its place, so the menu that clears
    /// the section can never dissolve a Group behind the operator's back.
    fn deletable_parked(&self) -> Vec<ThreadId> {
        let grouped: std::collections::HashSet<ThreadId> = self
            .cockpit
            .groups()
            .iter()
            .flat_map(|group| group.members.iter().copied())
            .collect();
        self.parked_threads()
            .into_iter()
            .filter(|thread| !grouped.contains(thread))
            .collect()
    }

    /// The Threads some Group claims as home (`Groups::home`), as the nav
    /// sees it: an open Thread that is only ever a Group's guest is still a
    /// loose row under its Project (R1). A member whose leave waits on a
    /// pending draft has already left.
    fn homed_threads(&self) -> std::collections::HashSet<ThreadId> {
        self.cockpit
            .groups()
            .iter()
            .flat_map(|group| {
                let pending_leave = self.cockpit.roster().pending_leave(group.id);
                group.members.iter().copied().filter(move |thread| {
                    Some(*thread) != pending_leave && group.is_home_of(*thread)
                })
            })
            .collect()
    }

    /// Whether any Thread, open or parked, records this Project.
    fn project_in_use(&self, project: ProjectId) -> bool {
        self.cockpit
            .threads()
            .into_iter()
            .chain(self.facts.parked().iter().copied())
            .any(|thread| self.cockpit.project_id(thread) == Some(project))
    }

    /// A Thread's effective cwd. A Group has no cwd.
    fn target_path(&self, target: MenuTarget) -> Option<std::path::PathBuf> {
        match target {
            MenuTarget::Thread(thread) | MenuTarget::Pane(thread) => self.thread_path(thread),
            MenuTarget::Group(_)
            | MenuTarget::Project(_)
            | MenuTarget::Parked
            | MenuTarget::Review { .. } => None,
        }
    }

    fn thread_path(&self, thread: ThreadId) -> Option<std::path::PathBuf> {
        if let Some(open) = self.cockpit.thread(thread) {
            return ferrite_core::workspace::effective_cwd(
                open.session_project_root(),
                open.workspace(),
            )
            .map(std::path::Path::to_path_buf);
        }
        let meta = self.cockpit.peek(thread).ok()?;
        ferrite_core::workspace::effective_cwd(
            meta.session_project_root.as_deref(),
            meta.workspace.as_ref(),
        )
        .map(std::path::Path::to_path_buf)
    }

    /// A row's press: a destructive row arms on the first press and runs
    /// on the second; anything else runs at once. The menu closes after
    /// a verb runs, and stays up while a row is armed.
    fn press_menu_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.context_menu.as_mut() else {
            return;
        };
        let Some(Some((item, verb))) = open.rows.get(index) else {
            return;
        };
        if item.disabled {
            return;
        }
        let verb = *verb;
        // The single-Thread setting is honoured for a single Thread. The
        // bulk delete always asks: one press must never erase a dozen
        // transcripts, whatever the preference says about one.
        let confirm = self.prefs.settings.confirm_delete || verb != MenuVerb::Delete;
        if item.destructive && confirm && open.armed != Some(index) {
            open.armed = Some(index);
            cx.notify();
            return;
        }
        let target = open.target;
        self.context_menu = None;
        self.run_menu_verb(target, verb, window, cx);
        cx.notify();
    }

    fn run_menu_verb(
        &mut self,
        target: MenuTarget,
        verb: MenuVerb,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (target, verb) {
            (MenuTarget::Thread(thread), MenuVerb::Rename) => {
                self.start_rename(RenameTarget::Thread(thread), cx)
            }
            (MenuTarget::Pane(thread), MenuVerb::Rename) => {
                self.start_rename(RenameTarget::PaneTitle(thread), cx)
            }
            (MenuTarget::Group(group), MenuVerb::Rename) => {
                self.start_rename(RenameTarget::Group(group), cx)
            }
            (MenuTarget::Thread(thread) | MenuTarget::Pane(thread), MenuVerb::Focus) => {
                if self.pane_for(thread).is_some() {
                    self.focus_thread(thread, cx);
                } else {
                    self.revive_thread(thread, cx);
                }
            }
            (MenuTarget::Thread(thread) | MenuTarget::Pane(thread), MenuVerb::Fullscreen) => {
                self.focus_thread(thread, cx);
                self.cockpit.toggle_fullscreen();
            }
            // One act: a Group member leaves its Group and parks
            // (`Cockpit::park_thread`), straight into the parked rows.
            (MenuTarget::Thread(thread) | MenuTarget::Pane(thread), MenuVerb::Park) => {
                self.close_pane(PaneIdentity::Thread(thread), cx);
                // A Thread parked from the nav may have had no Pane in
                // this view, so the grid did not change: refresh the rows.
                self.parked_changed(cx);
            }
            (MenuTarget::Pane(_), MenuVerb::CopySelection) => {
                if let Some(text) = self.native_copy.clone() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            (MenuTarget::Review { thread, .. }, MenuVerb::Comment) => {
                if let Some(review) = self.review_of(thread) {
                    review.update(cx, |review, cx| review.open_editor(window, cx));
                }
            }
            (MenuTarget::Review { thread, line }, MenuVerb::OpenAtLine) => {
                let place = self.review_of(thread).and_then(|review| {
                    let (path, number) = review.read(cx).place(line)?;
                    Some((self.thread_path(thread)?.join(path), number))
                });
                if let (Some((path, number)), Some(index)) = (place, self.pane_for(thread)) {
                    self.focus_pane(index);
                    self.open_preview(path, Some(number), window, cx);
                }
            }
            (MenuTarget::Review { thread, .. }, MenuVerb::CopySelection) => {
                if let Some(text) = self
                    .review_of(thread)
                    .and_then(|review| review.read(cx).picked_text())
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            (MenuTarget::Pane(thread), MenuVerb::CopyTranscript) => {
                if let Some(open) = self.cockpit.thread(thread) {
                    let text = plain_text(open.transcript().blocks());
                    if !text.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                }
            }
            (MenuTarget::Thread(thread) | MenuTarget::Pane(thread), MenuVerb::LeaveGroup) => {
                match self.cockpit.apply_group(GroupChange::Leave { thread }) {
                    Ok(_) => self.group_error = None,
                    Err(error) => self.group_error = Some(error.to_string().into()),
                }
                self.sync_panes(cx);
            }
            (MenuTarget::Group(group), MenuVerb::EnterGroup) => self.enter_group(group, cx),
            (MenuTarget::Parked, MenuVerb::ToggleParked) => {
                self.toggle_nav_view(nav::NavView::Parked, None, cx);
            }
            (MenuTarget::Parked, MenuVerb::DeleteAllParked) => {
                self.delete_parked_threads(cx);
            }
            (MenuTarget::Group(group), MenuVerb::DissolveGroup) => {
                // Members leave one by one; the Group dissolves under two.
                let members = self
                    .cockpit
                    .groups()
                    .get(group)
                    .map(|group| group.members.clone())
                    .unwrap_or_default();
                for thread in members {
                    if self.cockpit.groups().get(group).is_none() {
                        break;
                    }
                    if let Err(error) = self.cockpit.apply_group(GroupChange::Leave { thread }) {
                        self.group_error = Some(error.to_string().into());
                        break;
                    }
                }
                self.sync_panes(cx);
            }
            (_, MenuVerb::NewThread) => {
                let project = match target {
                    MenuTarget::Thread(thread) | MenuTarget::Pane(thread) => {
                        self.cockpit.project_id(thread)
                    }
                    MenuTarget::Group(group) => self
                        .cockpit
                        .groups()
                        .get(group)
                        .and_then(|group| group.members.first().copied())
                        .and_then(|thread| self.cockpit.project_id(thread)),
                    MenuTarget::Project(project) => Some(project),
                    MenuTarget::Parked | MenuTarget::Review { .. } => None,
                };
                self.open_draft(DraftTarget::Main, cx);
                if let (Some(project), Some(draft)) = (project, self.focused_draft_mut()) {
                    draft.binding.choose_checkout(project);
                }
            }
            (_, MenuVerb::Reveal) => {
                if let Some(path) = self.target_path(target) {
                    cx.reveal_path(&path);
                }
            }
            (MenuTarget::Thread(thread), MenuVerb::RefreshPr) => self.check_pr(thread, true, cx),
            (MenuTarget::Thread(thread), MenuVerb::OpenPr) => self.open_pr(thread, cx),
            (MenuTarget::Project(project), MenuVerb::OpenTasks) => {
                self.show_tasks_for(project, window, cx)
            }
            (_, MenuVerb::CopyPath) => {
                if let Some(path) = self.target_path(target) {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        path.to_string_lossy().to_string(),
                    ));
                }
            }
            (MenuTarget::Thread(thread) | MenuTarget::Pane(thread), MenuVerb::Delete) => {
                if let Err(error) = self.cockpit.delete(thread) {
                    self.group_error = Some(format!("delete refused: {error}").into());
                } else {
                    self.group_error = None;
                }
                if self
                    .popover
                    .as_ref()
                    .is_some_and(|open| open.pane == PaneIdentity::Thread(thread))
                {
                    self.popover = None;
                }
                self.sync_panes(cx);
                self.parked_changed(cx);
            }
            _ => {}
        }
    }

    /// Delete every Thread the Parked section lists. Each is its own act
    /// against the core, so one that refuses (a dirty legacy worktree, an
    /// unreadable log) does not stop the rest; the refusals are counted
    /// into the nav's banner, with the last reason, and the survivors stay
    /// listed where the operator can deal with them one at a time.
    fn delete_parked_threads(&mut self, cx: &mut Context<Self>) {
        let parked = self.deletable_parked();
        let mut refused = 0usize;
        let mut reason = None;
        for thread in parked {
            if let Err(error) = self.cockpit.delete(thread) {
                refused += 1;
                reason = Some(error.to_string());
            }
        }
        self.group_error = match (refused, reason) {
            (0, _) | (_, None) => None,
            (1, Some(reason)) => Some(format!("1 parked thread not deleted: {reason}").into()),
            (count, Some(reason)) => {
                Some(format!("{count} parked threads not deleted, last: {reason}").into())
            }
        };
        self.sync_panes(cx);
        self.parked_changed(cx);
    }

    /// The context menu, floated at the pointer and clamped inside the
    /// window; `deferred` so nothing later in the tree paints over it. A
    /// press on the menu's own dead space is swallowed.
    fn context_menu_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.context_menu.as_ref()?;
        let mut shell = menu::shell().on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
        );
        for (index, row) in open.rows.iter().enumerate() {
            shell = match row {
                None => shell.child(menu::gap()),
                Some((item, _)) => shell.child(
                    menu::row(index, item, open.armed == Some(index)).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            view.press_menu_row(index, window, cx);
                        }),
                    ),
                ),
            };
        }
        Some(
            deferred(
                anchored()
                    .position(open.at)
                    .snap_to_window_with_margin(px(ferrite_ui::theme::GRID_PAD))
                    .child(ferrite_ui::motion::menu_in(
                        "context-menu-in",
                        shell,
                        ferrite_ui::motion::Opens::Down,
                    )),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// A board: Panes and readers at their tree rects as this frame draws
    /// them (`slid_tree`: a reader sliding takes the room it has so far),
    /// seams over the gaps, the drop preview on top. Coordinates are the
    /// board's own — the frame is `relative`, its origin the nav's right
    /// edge — so the rects the tree computes for the window are shifted
    /// back by that origin.
    fn tree_board(
        &self,
        on: Board,
        tree: Tree,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let bounds = self.board_bounds(window);
        let origin_x = self.board_left();
        let local = |rect: layout::Rect| layout::Rect {
            x: rect.x - origin_x,
            ..rect
        };
        let mut board = div().relative().flex_1().min_w_0().min_h_0();
        // One Level for the whole board: no Pane draws a tier its
        // neighbours do not (rule 2.3.5).
        let level = self.board_level(window);
        let rest: std::collections::HashMap<ThreadId, layout::Rect> = tree
            .rects(bounds, ferrite_ui::theme::BOARD_SEAM)
            .into_iter()
            .collect();
        let tree = self.slid_tree(tree, bounds);
        // A slot slid all the way shut takes no room and draws nothing.
        let rects: Vec<(ThreadId, layout::Rect)> = tree
            .rects(bounds, ferrite_ui::theme::BOARD_SEAM)
            .into_iter()
            .filter(|(_, rect)| rect.w > 0.0 && rect.h > 0.0)
            .collect();
        *self.slot_sizes.borrow_mut() = rects
            .iter()
            .map(|(leaf, rect)| (*leaf, (rect.w, rect.h)))
            .collect();
        // The column's frame, over the cards' corners and under the seams.
        let frame = ferrite_ui::components::frame_around(
            rects.iter().map(|(_, rect)| window_bounds(*rect)).collect(),
        );
        for (leaf, rect) in rects {
            let cell = match leaf_slot(leaf) {
                Slot::Pane(identity) => {
                    let Some(index) = self.index_of(identity) else {
                        continue;
                    };
                    self.pane_slot(index, level, window, cx)
                }
                Slot::Reader(owner) => {
                    let pin = rest
                        .get(&leaf)
                        .and_then(|at_rest| beside::reader_pin(&tree, leaf, rect, *at_rest));
                    let Some(cell) = self.reader_cell(owner, leaf, pin, cx) else {
                        continue;
                    };
                    // A reader is a card beside the Pane, at rest.
                    cell.relative()
                        .child(pane::card_ring(rgba(ferrite_ui::theme::CARD_RING)))
                }
            };
            // The cell lies on its track as laid out; gpui rounds its edges
            // to the pixel where it lands, and its lines from the track's
            // own fraction, as the prototype's browser lays a grid's cells.
            let rect = local(rect);
            // The grabbed slot stays put, dimmed as lifted out of its slot,
            // while its ghost travels: the board keeps its shape until the
            // drop changes it.
            let cell = if self.dragging == Some(leaf) {
                cell.opacity(ferrite_ui::theme::DRAG_SOURCE_OPACITY)
                    .debug_selector(move || format!("pane-drag-source-{}", leaf.get()))
            } else {
                cell
            };
            board = board.child(
                cell.absolute()
                    .left(px(rect.x))
                    .top(px(rect.y))
                    .w(px(rect.w))
                    .h(px(rect.h)),
            );
        }
        board = board.child(frame);
        // The seams (F-18): one line per linked set down the gutter's
        // centre — a default grid's aligned column seams are one line from
        // the board's top to its foot — and a 9px grab band over each seam.
        // At rest the line is nothing: the gutter separates the cards.
        // Under the pointer (any band of the set) and while held, the whole
        // line draws `SEAM_LINE_W` of `SEAM_HOVER`, fading in over
        // `motion::SEAM_FADE`.
        let seams = tree.seams(bounds, ferrite_ui::theme::BOARD_SEAM, SEAM_GRAB);
        let mut drawn: Vec<SeamId> = Vec::new();
        for seam in &seams {
            if drawn.contains(&seam.id) {
                continue;
            }
            let group = tree.linked(&seam.id, bounds, ferrite_ui::theme::BOARD_SEAM);
            let members: Vec<&layout::Seam> = seams
                .iter()
                .filter(|other| group.contains(&other.id))
                .collect();
            drawn.extend(group.iter().cloned());
            let key = SharedString::from(format!(
                "seam-{}",
                seam.id
                    .0
                    .iter()
                    .map(|step| if *step { '1' } else { '0' })
                    .collect::<String>()
            ));
            let dragging = self.seam_drag.as_ref().is_some_and(|drag| {
                drag.board == on && members.iter().any(|member| member.id == drag.seam)
            });
            // The line spans the union of the set's bands, along its axis.
            let line_w = ferrite_ui::theme::SEAM_LINE_W;
            let line = match seam.axis {
                layout::Axis::Row => {
                    let top = members
                        .iter()
                        .map(|member| member.band.y)
                        .fold(f32::INFINITY, f32::min);
                    let bottom = members
                        .iter()
                        .map(|member| member.band.y + member.band.h)
                        .fold(f32::NEG_INFINITY, f32::max);
                    let band = local(seam.band);
                    layout::Rect {
                        x: band.x + (band.w - line_w) / 2.0,
                        y: top,
                        w: line_w,
                        h: bottom - top,
                    }
                }
                layout::Axis::Column => {
                    let left = members
                        .iter()
                        .map(|member| local(member.band).x)
                        .fold(f32::INFINITY, f32::min);
                    let right = members
                        .iter()
                        .map(|member| local(member.band).x + member.band.w)
                        .fold(f32::NEG_INFINITY, f32::max);
                    layout::Rect {
                        x: left,
                        y: seam.band.y + (seam.band.h - line_w) / 2.0,
                        w: right - left,
                        h: line_w,
                    }
                }
            };
            let accent = if dragging {
                rgba(ferrite_ui::theme::SEAM_HOVER).into()
            } else {
                ferrite_ui::motion::hover_blend(
                    &key,
                    rgba(ferrite_ui::theme::TRANSPARENT).into(),
                    rgba(ferrite_ui::theme::SEAM_HOVER).into(),
                )
            };
            board = board.child(
                div()
                    .debug_selector({
                        let key = key.clone();
                        move || format!("board-{key}")
                    })
                    .absolute()
                    .left(px(line.x))
                    .top(px(line.y))
                    .w(px(line.w))
                    .h(px(line.h))
                    .bg(accent),
            );
            for (at, member) in members.iter().enumerate() {
                let band = local(member.band);
                let id = member.id.clone();
                let cursor = match member.axis {
                    layout::Axis::Row => gpui::CursorStyle::ResizeLeftRight,
                    layout::Axis::Column => gpui::CursorStyle::ResizeUpDown,
                };
                board = board.child(
                    div()
                        .id(gpui::ElementId::Name(SharedString::from(format!(
                            "{key}-band-{at}"
                        ))))
                        .debug_selector({
                            let key = key.clone();
                            move || format!("board-{key}-band-{at}")
                        })
                        .absolute()
                        .left(px(band.x))
                        .top(px(band.y))
                        .w(px(band.w))
                        .h(px(band.h))
                        .cursor(cursor)
                        .on_hover(ferrite_ui::motion::hover_listener_with(
                            key.clone(),
                            ferrite_ui::motion::SEAM_FADE,
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                view.begin_seam_drag(on, id.clone(), cx);
                            }),
                        ),
                );
            }
        }
        if let Some((target, zone)) = self.drop_preview {
            if let Some((_, rect)) = tree
                .rects(bounds, ferrite_ui::theme::BOARD_SEAM)
                .into_iter()
                .find(|(leaf, _)| *leaf == target)
            {
                // The slot a release would fill: a white wash at the card's
                // radius with the focus ring's edge, and a raised tag naming
                // the verb.
                let wash = local(layout::zone_rect(rect, zone));
                board = board.child(
                    div()
                        .absolute()
                        .left(px(wash.x))
                        .top(px(wash.y))
                        .w(px(wash.w))
                        .h(px(wash.h))
                        .rounded(px(ferrite_ui::theme::R_PANE))
                        .bg(rgba(ferrite_ui::theme::DROP_WASH))
                        .border_1()
                        .border_color(rgba(ferrite_ui::theme::DROP_EDGE))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .px(px(ferrite_ui::theme::DROP_LABEL_PAD_X))
                                .py(px(ferrite_ui::theme::DROP_LABEL_PAD_Y))
                                .bg(ferrite_ui::theme::FLOAT_GROUND)
                                .rounded(px(ferrite_ui::theme::R_BLOCK))
                                .border_1()
                                .border_color(ferrite_ui::theme::FLOAT_EDGE)
                                .shadow(ferrite_ui::components::float_shadow())
                                .font_family(ferrite_ui::theme::FONT_UI)
                                .text_size(px(ferrite_ui::theme::FS_UI))
                                .line_height(px(ferrite_ui::theme::LH_UI))
                                .text_color(rgb(ferrite_ui::theme::TEXT_STRONG))
                                .child(match zone {
                                    Zone::Swap => "swap",
                                    Zone::Split(Edge::Left) => "split left",
                                    Zone::Split(Edge::Right) => "split right",
                                    Zone::Split(Edge::Top) => "split above",
                                    Zone::Split(Edge::Bottom) => "split below",
                                }),
                        ),
                );
            }
        }
        board
    }

    /// A press on a seam: the drag begins from the persisted tree, and
    /// the moves until release re-derive the ratio from the pointer.
    fn begin_seam_drag(&mut self, board: Board, seam: SeamId, cx: &mut Context<Self>) {
        self.land_reader_slides(cx);
        let Some(tree) = self.board_tree(board) else {
            return;
        };
        self.seam_drag = Some(SeamDrag { board, seam, tree });
        cx.notify();
    }

    /// The pointer moved with a seam held: the seam follows it, and only
    /// the two Panes touching the seam change size — the tree keeps every
    /// other Pane where it was and neither neighbour under its floor.
    fn drag_seam(
        &mut self,
        position: gpui::Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.board_bounds(window);
        let pointer = layout::Point {
            x: f32::from(position.x),
            y: f32::from(position.y),
        };
        let Some(drag) = self.seam_drag.as_mut() else {
            return;
        };
        if drag
            .tree
            .drag_linked(&drag.seam, bounds, pointer, ferrite_ui::theme::BOARD_SEAM)
        {
            cx.notify();
        }
    }

    /// The release: the dragged tree persists, once.
    fn end_seam_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.seam_drag.take() else {
            return;
        };
        self.commit_board(drag.board, drag.tree);
        cx.notify();
    }

    /// A slot dragged over another: what a release here would do, for the
    /// wash — nothing over itself, nothing across boards.
    fn preview_pane_drop(
        &mut self,
        source: ThreadId,
        target: ThreadId,
        position: gpui::Point<Pixels>,
        bounds: gpui::Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let on_board = self
            .board()
            .and_then(|board| self.board_tree(board))
            .is_some_and(|tree| tree.contains(source) && tree.contains(target));
        let preview = (source != target && on_board).then(|| {
            let (pointer, rect) = drop_geometry(position, bounds);
            (target, layout::zone(pointer, rect))
        });
        self.set_drop_preview(preview, cx);
    }

    fn set_drop_preview(&mut self, preview: Option<(ThreadId, Zone)>, cx: &mut Context<Self>) {
        if self.drop_preview != preview {
            self.drop_preview = preview;
            cx.notify();
        }
    }

    /// A nav row dragged over a board slot: it can only split in beside
    /// it (swapping would push the slot off the board), so every point
    /// reads as the nearest edge. A Thread over its own Pane means nothing.
    fn preview_nav_drop(
        &mut self,
        drag: Drag,
        target: ThreadId,
        position: gpui::Point<Pixels>,
        bounds: gpui::Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let preview = match drag {
            Drag::Thread { thread, .. } if thread != target => {
                let (pointer, rect) = drop_geometry(position, bounds);
                Some((target, Zone::Split(layout::nearest_edge(pointer, rect))))
            }
            _ => None,
        };
        self.set_drop_preview(preview, cx);
    }

    /// A nav row released on a board slot: the Thread joins the board's
    /// Group — or, on a Solo board, founds one with the Thread shown — and
    /// lands beside the slot, on the edge the preview named. A member
    /// already on the board just moves there.
    fn drop_nav_on_board(&mut self, drag: Drag, target: ThreadId, cx: &mut Context<Self>) {
        self.nav_dragging = None;
        let preview = self.drop_preview.take();
        let (Some((_, Zone::Split(edge))), Drag::Thread { thread, .. }) =
            (preview.filter(|(previewed, _)| *previewed == target), drag)
        else {
            cx.notify();
            return;
        };
        let view = self.cockpit.roster().view();
        let shown = match leaf_slot(target) {
            Slot::Pane(PaneIdentity::Thread(shown)) | Slot::Reader(shown) => Some(shown),
            _ => None,
        };
        // Groups are non-exclusive (R1): the board's Group is the one being
        // viewed; Solo's is the shown Thread's home.
        let group_of = |view: &Self, thread: ThreadId| {
            view.cockpit.groups().home(thread).map(|group| group.id)
        };
        let holds = |view: &Self, group: GroupId, thread: ThreadId| {
            view.cockpit
                .groups()
                .get(group)
                .is_some_and(|group| group.members.contains(&thread))
        };
        let group = match view {
            View::Group(group) => Some(group),
            View::Solo => shown.and_then(|shown| group_of(self, shown)),
        };
        if group.is_none_or(|group| !holds(self, group, thread)) {
            let joined = match (group, shown) {
                (Some(group), _) => self
                    .cockpit
                    .drop(drag, view, DropTarget::GroupHeader(group)),
                (None, Some(shown)) => self.cockpit.drop(
                    drag,
                    view,
                    DropTarget::ThreadRow {
                        thread: shown,
                        group: None,
                        index: 0,
                    },
                ),
                (None, None) => Ok(()),
            };
            if let Err(error) = joined {
                self.group_error = Some(error.to_string().into());
                self.sync_panes(cx);
                cx.notify();
                return;
            }
            self.group_error = None;
        }
        let Some(group) = group.or_else(|| shown.and_then(|shown| group_of(self, shown))) else {
            self.sync_panes(cx);
            cx.notify();
            return;
        };
        // Entering revives a parked newcomer and shows the Group it joined.
        self.enter_group(group, cx);
        let board = Board::Group(group);
        if let Some(mut tree) = self.board_tree(board) {
            if tree.split(target, edge, thread) {
                self.commit_board(board, tree);
            }
        }
        self.focus_thread(thread, cx);
        cx.notify();
    }

    /// The release of a dragged slot on another: the centre swaps the two
    /// leaves, an edge moves the source beside the target — the tree is
    /// kept either way. Reads the preview the last move computed.
    fn drop_pane(&mut self, source: ThreadId, target: ThreadId, cx: &mut Context<Self>) {
        self.dragging = None;
        let preview = self.drop_preview.take();
        let Some((previewed, zone)) = preview.filter(|(previewed, _)| *previewed == target) else {
            cx.notify();
            return;
        };
        self.apply_pane_drop(source, previewed, zone, cx);
    }

    /// The tree change a drop makes, persisted through the core door.
    fn apply_pane_drop(
        &mut self,
        source: ThreadId,
        target: ThreadId,
        zone: Zone,
        cx: &mut Context<Self>,
    ) {
        let Some(board) = self.board() else {
            return;
        };
        let Some(mut tree) = self.board_tree(board) else {
            return;
        };
        if source == target || !tree.contains(source) || !tree.contains(target) {
            return;
        }
        let changed = match zone {
            Zone::Swap => tree.swap(source, target),
            Zone::Split(edge) => tree.split(target, edge, source),
        };
        if changed {
            self.commit_board(board, tree);
        }
        cx.notify();
    }

    /// cmd-, and the gear: the Settings panel, toggled. Opening it probes
    /// the CLIs' versions once, off the UI thread.
    fn open_settings(&mut self, _: &OpenSettings, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_settings(cx);
    }

    fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = !self.settings_open;
        if self.settings_open {
            self.settings_page = prefs::PageKey::default();
            self.settings_search = None;
            self.settings_copied = None;
            self.project_editor = None;
            self.popover = None;
            self.context_menu = None;
            if self.cli_versions.is_none() {
                self.probe_cli_versions(cx);
            }
        }
        cx.notify();
    }

    /// Ask the CLIs their versions again, off the UI thread.
    fn probe_cli_versions(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let versions = cx
                .background_executor()
                .spawn(async {
                    ferrite_core::providers::discover::rediscover();
                    (cli_version(Provider::Claude), cli_version(Provider::Codex))
                })
                .await;
            this.update(cx, |view, cx| {
                view.cli_versions = Some((versions.0.into(), versions.1.into()));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// One beat of the updater: ask the registry when due, then install
    /// whatever automatic updating may install now.
    fn tick_cli_updates(&mut self, cx: &mut Context<Self>) {
        if !self.cli_updates.begin_check(std::time::Instant::now()) {
            self.auto_update_clis(cx);
            return;
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let results = cx
                .background_executor()
                .spawn(async {
                    [Provider::Claude, Provider::Codex].map(|provider| {
                        (provider, ferrite_core::providers::update::check(provider))
                    })
                })
                .await;
            this.update(cx, |view, cx| {
                let auto = view.prefs.settings.auto_update_clis;
                for (provider, result) in results {
                    view.cli_updates.checked(provider, result, auto);
                }
                view.auto_update_clis(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Install each ready upgrade whose provider has no Session running,
    /// when the operator lets Ferrite update on its own.
    fn auto_update_clis(&mut self, cx: &mut Context<Self>) {
        if !self.prefs.settings.auto_update_clis {
            return;
        }
        for provider in self.cli_updates.ready() {
            if self.cockpit.live_sessions(provider) == 0 {
                self.update_cli(provider, cx);
            }
        }
    }

    /// Install `provider`'s newest CLI now. Once it lands, the model menus
    /// are asked again and every open Thread of that provider restarts on
    /// its own conversation — at once when idle, otherwise when its turn
    /// ends — so nothing keeps running the old release.
    fn update_cli(&mut self, provider: Provider, cx: &mut Context<Self>) {
        let Some(upgrade) = self.cli_updates.begin_update(provider) else {
            return;
        };
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { ferrite_core::providers::update::run(provider, &upgrade) })
                .await;
            this.update(cx, |view, cx| {
                if result.is_ok() {
                    // Open Threads restart on the new CLI as each goes
                    // idle, and the pickers ask for its menu now.
                    view.cockpit.cli_replaced(provider);
                }
                view.cli_updates.updated(provider, result);
                view.probe_cli_versions(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The updater's toasts. Like the Needs-you view's, they wait for render, the
    /// one place with a Window in hand.
    fn present_cli_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::cli_updates::{name, Toast};
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        let toasts = self.cli_updates.take_toasts();
        if toasts.is_empty() {
            // Called every render: a notify here with nothing to show would
            // rebuild the Cockpit on every frame anything else draws.
            return;
        }
        for toast in toasts {
            let notification = match toast {
                Toast::Offer { provider, latest } => {
                    let view = cx.entity().downgrade();
                    Notification::new()
                        .title(format!("{} {latest} is available", name(provider)))
                        .message("Click to update. Newer releases bring newer models.")
                        .with_type(NotificationType::Info)
                        .autohide(false)
                        .on_click(move |_, _, cx| {
                            view.update(cx, |view, cx| view.update_cli(provider, cx))
                                .ok();
                        })
                }
                Toast::Updated { provider, version } => Notification::new()
                    .title(format!("{} updated to {version}", name(provider)))
                    .message("Open Threads restart on it as each finishes its turn.")
                    .with_type(NotificationType::Success)
                    .autohide(true),
                Toast::Failed { provider, detail } => Notification::new()
                    .title(format!("{} update failed", name(provider)))
                    .message(detail)
                    .with_type(NotificationType::Error)
                    .autohide(false),
            };
            window.push_notification(notification, cx);
        }
        cx.notify();
    }

    /// Settings › About's Diagnostics: what a bug report needs, on the
    /// clipboard (`ferrite_core::diagnostics::report`). The report reads
    /// up to the whole log, so it is gathered in the background.
    fn copy_diagnostics(&mut self, cx: &mut Context<Self>) {
        let (claude, codex) = self
            .cli_versions
            .clone()
            .unwrap_or_else(|| ("unknown".into(), "unknown".into()));
        let facts = [
            (
                "Version",
                version_label(env!("CARGO_PKG_VERSION"), crate::titlebar::DEV),
            ),
            (
                "Platform",
                format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            ),
            ("Claude CLI", claude.to_string()),
            ("Codex CLI", codex.to_string()),
            ("Open Threads", self.cockpit.threads().len().to_string()),
        ];
        let dir = self.prefs.dir.clone();
        cx.spawn(async move |this, cx| {
            let report = cx
                .background_executor()
                .spawn(async move { ferrite_core::diagnostics::report(&dir, &facts) })
                .await;
            this.update(cx, |view, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(report));
                view.settings_copied = Some("Diagnostics");
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A `settings.json` that could not be used is said once and kept on
    /// screen: the window runs on the defaults and saves nothing over the
    /// file, so a change made now lasts only until Ferrite quits.
    fn present_unreadable_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        if self.unreadable_told {
            return;
        }
        let Some(reason) = &self.prefs.unreadable else {
            return;
        };
        self.unreadable_told = true;
        let path = self.prefs.dir.join(ferrite_core::settings::Settings::FILE);
        window.push_notification(
            Notification::new()
                .title("Settings could not be read")
                .message(format!(
                    "{}: {reason}. Ferrite is using the defaults and will not save over \
                     the file; fix or remove it, then relaunch.",
                    path.display()
                ))
                .with_type(NotificationType::Warning)
                .autohide(false),
            cx,
        );
    }

    /// What an agent or a script asked of the windows through the API
    /// (ADR 0027), one at a time.
    fn present_asked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for asked in self.cockpit.take_asked() {
            self.present(asked, window, cx);
        }
    }

    /// One ask: to land on a Thread, as its nav row would; a toast, which
    /// fades as a finished turn's does unless important, with its chime.
    fn present(
        &mut self,
        asked: ferrite_core::cockpit::Asked,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use ferrite_core::cockpit::Asked;
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        match asked {
            Asked::Focus(thread) => self.land_on_thread(thread, cx),
            Asked::Notification {
                title,
                body,
                sound,
                important,
            } => {
                let mut toast = Notification::new()
                    .title(title)
                    .with_type(NotificationType::Info)
                    .autohide(!important);
                if let Some(body) = body {
                    toast = toast.message(body);
                }
                window.push_notification(toast, cx);
                // No provider to ask: only the all-sounds switch applies.
                if let Some(sound) = sound.filter(|_| self.prefs.settings.sounds) {
                    self.chime(sound);
                }
            }
        }
    }

    /// A provider's 5-hour or weekly window rising past one of the
    /// operator's thresholds: one toast per crossing, left up — the
    /// allowance is the account's, and every Thread on it draws it down.
    fn present_allowance_warnings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use ferrite_core::providers::limits::LimitWindow;
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        for warning in self.cockpit.take_allowance_warnings() {
            let span = match warning.window {
                LimitWindow::FiveHour => "5-hour",
                LimitWindow::Weekly => "weekly",
            };
            let provider = provider_title(warning.provider);
            window.push_notification(
                Notification::new()
                    .title(format!(
                        "{provider} {span} allowance {:.0}% used",
                        warning.used_fraction * 100.0
                    ))
                    .message(format!(
                        "Past your {}% warning. Every {provider} Thread draws on it.",
                        warning.threshold
                    ))
                    .with_type(NotificationType::Warning)
                    .autohide(false),
                cx,
            );
        }
    }

    /// Every change to the settings goes through here: saved at once
    /// (unless the file on disk could not be read, which is kept as it
    /// is), the Session defaults the spawner reads refreshed, and the facts
    /// that depend on a setting re-derived.
    fn change_settings(
        &mut self,
        change: impl FnOnce(&mut ferrite_core::settings::Settings),
        cx: &mut Context<Self>,
    ) {
        change(&mut self.prefs.settings);
        if self.prefs.unreadable.is_none() {
            if let Err(e) = self.prefs.settings.save(&self.prefs.dir) {
                self.group_error = Some(SharedString::from(format!("settings not saved: {e}")));
            }
        }
        // Saved first: a Server rereads the file.
        self.cockpit.settings_changed(&self.prefs.settings);
        self.machines_changed(cx);
        if let Ok(mut defaults) = self.prefs.defaults.lock() {
            *defaults = ferrite_core::spawner::SessionDefaults::from_settings(&self.prefs.settings);
        }
        ferrite_pane::visual::diagram::set_enabled(
            self.prefs.settings.visuals != ferrite_core::visual::Level::Off,
            cx,
        );
        if self.facts.set_auto_title(self.prefs.settings.auto_title) {
            self.parked_changed(cx);
            for thread in self.cockpit.threads() {
                self.facts.renamed(&self.cockpit, thread);
            }
            self.refresh_names();
        }
        let reading = self.prefs.settings.reading_size;
        for review in self.panes.iter().filter_map(|pane| pane.review.review()) {
            review.update(cx, |review, cx| review.set_reading(reading, cx));
        }
        cx.notify();
    }

    /// Route every toolkit field through the same save and session-default path.
    fn setting_change<T: 'static>(
        &self,
        cx: &Context<Self>,
        write: impl Fn(&mut ferrite_core::settings::Settings, T) + 'static,
    ) -> impl Fn(T, &mut gpui::App) + 'static {
        let view = cx.entity().downgrade();
        move |value, cx| {
            let _ = view.update(cx, |view, cx| {
                view.change_settings(|settings| write(settings, value), cx);
            });
        }
    }

    /// Settings' line for one provider CLI: its installed version, what
    /// the update check found, and the button when there is a release
    /// Ferrite can install.
    fn cli_update_row(
        &self,
        id: &'static str,
        provider: Provider,
        version: SharedString,
        cx: &Context<Self>,
    ) -> prefs::Row {
        let (detail, button) = crate::cli_updates::describe(
            provider,
            self.cli_updates.state(provider),
            self.prefs.settings.auto_update_clis,
            self.cockpit.live_sessions(provider),
        );
        let title = match provider {
            Provider::Claude => "Claude CLI",
            Provider::Codex => "Codex CLI",
        };
        let view = cx.entity().downgrade();
        prefs::action(id, title, detail, version, button, move |cx| {
            view.update(cx, |view, cx| view.update_cli(provider, cx))
                .ok();
        })
    }

    /// Settings' line for one chime: the file it plays (or the built-in),
    /// `Play` to hear it, `Choose…` for a file of the operator's own, and
    /// `Use built-in` to go back.
    fn sound_file_row(
        &self,
        id: &'static str,
        title: &'static str,
        sound: Sound,
        cx: &Context<Self>,
    ) -> prefs::Row {
        let file = self.prefs.settings.sound_file(sound);
        let view = cx.entity().downgrade();
        let play: prefs::Act = std::rc::Rc::new({
            let view = view.clone();
            move |cx| {
                let _ = view.update(cx, |view, _| view.chime(sound));
            }
        });
        let choose: prefs::Act = std::rc::Rc::new({
            move |cx| {
                let _ = view.update(cx, |view, cx| view.choose_sound_file(sound, cx));
            }
        });
        let mut acts = vec![("Play", play), ("Choose\u{2026}", choose)];
        if file.is_some() {
            let reset = self.setting_change(cx, move |s, ()| s.set_sound_file(sound, None));
            acts.push((
                "Use built-in",
                std::rc::Rc::new(move |cx: &mut gpui::App| reset((), cx)),
            ));
        }
        prefs::file_choice(
            id,
            title,
            if cfg!(windows) { "A WAV file" } else { "" },
            file,
            "Built-in",
            acts,
        )
    }

    /// Ask the system for a sound file, and play it from now on.
    fn choose_sound_file(&mut self, sound: Sound, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose sound".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(file) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |view, cx| {
                view.change_settings(|s| s.set_sound_file(sound, Some(file)), cx);
            });
        })
        .detach();
    }

    /// The searchable Settings sheet, drawn above the cockpit's overlays.
    fn settings_element(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.settings_open {
            return None;
        }
        let search = self
            .settings_search
            .get_or_insert_with(|| {
                cx.new(|cx| {
                    gpui::component::input::InputState::new(window, cx).placeholder("Search")
                })
            })
            .clone();
        let machines = self.machine_settings(window, cx);
        let settings = &self.prefs.settings;
        let defaults = vec![prefs::choices(
            "settings-provider",
            "Provider",
            "What a new thread starts on",
            [Provider::Claude, Provider::Codex]
                .into_iter()
                .map(|provider| {
                    (
                        provider_title(provider).into(),
                        settings.default_provider == provider,
                        provider,
                    )
                })
                .collect(),
            self.setting_change(cx, |settings, provider| {
                settings.default_provider = provider
            }),
        )];
        let mut new_thread_groups = vec![prefs::Group::new(None).rows(defaults)];
        for provider in [Provider::Claude, Provider::Codex] {
            let chosen = settings.model_for(provider).map(str::to_string);
            let model = prefs::chooser(
                if provider == Provider::Claude {
                    "settings-claude-model"
                } else {
                    "settings-codex-model"
                },
                // The group names the provider: `Claude › Model`.
                "Model",
                "",
                prefs::model_options(self.cockpit.model_catalog(provider), chosen.as_deref()),
                self.setting_change(cx, move |settings, value| {
                    settings.set_model_for(provider, value)
                }),
            );
            let effort = settings.effort_for(provider).map(str::to_string);
            let mut ladder = ferrite_core::providers::models::efforts_for(
                provider,
                chosen.as_deref(),
                &self.cockpit.announced_models(provider),
            );
            // A saved value remains visible even if the current model's
            // announcement no longer includes it; presentation never rewrites it.
            if let Some(effort) = &effort {
                if !ladder.contains(effort) {
                    ladder.push(effort.clone());
                }
            }
            let effort = prefs::choices(
                if provider == Provider::Claude {
                    "settings-claude-effort"
                } else {
                    "settings-codex-effort"
                },
                "Effort",
                // Said once, in the first group: it holds for both.
                if provider == Provider::Claude {
                    "Each thread can change its own"
                } else {
                    ""
                },
                std::iter::once(None)
                    .chain(ladder.into_iter().map(Some))
                    .map(|value| {
                        let label = value
                            .as_deref()
                            .map(effort_title)
                            .unwrap_or_else(|| prefs::CLI_DEFAULT.into());
                        (label.into(), effort == value, value)
                    })
                    .collect(),
                self.setting_change(cx, move |settings, value| {
                    settings.set_effort_for(provider, value)
                }),
            );
            new_thread_groups.push(provider_group(provider).rows([model, effort]));
        }
        let modes = |options: &[(&str, Option<&str>)], selected: Option<&str>| {
            options
                .iter()
                .map(|(label, value)| {
                    (
                        SharedString::from(label.to_string()),
                        selected == *value,
                        value.map(str::to_string),
                    )
                })
                .collect()
        };
        let claude_permissions = vec![prefs::choices(
            "settings-claude-mode",
            "Permissions",
            "When Claude asks before acting",
            modes(
                &[
                    (prefs::CLI_DEFAULT, None),
                    ("Ask", Some("default")),
                    ("Accept edits", Some("acceptEdits")),
                    ("Plan", Some("plan")),
                    ("Bypass permissions", Some("bypassPermissions")),
                ],
                settings.claude_permission_mode.as_deref(),
            ),
            self.setting_change(cx, |s, v| s.claude_permission_mode = v),
        )];
        let codex_permissions = vec![
            prefs::choices(
                "settings-codex-approval",
                "Approvals",
                "When Codex asks before acting",
                modes(
                    &[
                        ("On request", Some("on-request")),
                        ("Untrusted", Some("untrusted")),
                        ("Never", Some("never")),
                    ],
                    Some(&settings.codex_approval_policy),
                ),
                self.setting_change(cx, |s, v: Option<String>| {
                    s.codex_approval_policy = v.unwrap_or_else(|| "on-request".into())
                }),
            ),
            prefs::choices(
                "settings-codex-sandbox",
                "Sandbox",
                "What Codex may touch",
                modes(
                    &[
                        (prefs::CLI_DEFAULT, None),
                        ("Read only", Some("read-only")),
                        ("Workspace write", Some("workspace-write")),
                        ("Full access", Some("danger-full-access")),
                    ],
                    settings.codex_sandbox.as_deref(),
                ),
                self.setting_change(cx, |s, v| s.codex_sandbox = v),
            ),
        ];
        let behaviour = vec![
            prefs::toggle(
                "settings-auto-title",
                "Name threads automatically",
                "Titled from the first prompt until you rename it",
                settings.auto_title,
                self.setting_change(cx, |s, v| s.auto_title = v),
            ),
            prefs::toggle(
                "settings-placeholder-suggestions",
                "Suggest follow-up prompts",
                "A likely next prompt in an empty composer · tab takes it",
                settings.placeholder_suggestions,
                self.setting_change(cx, |s, v| s.placeholder_suggestions = v),
            ),
            prefs::toggle(
                "settings-confirm-delete",
                "Confirm before deleting a thread",
                "Ask before removing a thread and its transcript",
                settings.confirm_delete,
                self.setting_change(cx, |s, v| s.confirm_delete = v),
            ),
            prefs::toggle(
                "settings-nav-collapsed",
                "Start with the sidebar collapsed",
                "cmd-B toggles it any time",
                settings.nav_collapsed,
                self.setting_change(cx, |s, v| s.nav_collapsed = v),
            ),
        ];
        let size = settings.reading_size;
        let reading = vec![prefs::stepper(
            "settings-text-size",
            "Text size",
            "Transcript text in every Pane, from 12 to 24px",
            format!("{}px", size.px()).into(),
            size > ferrite_core::settings::ReadingSize::SMALLEST,
            size < ferrite_core::settings::ReadingSize::LARGEST,
            {
                let view = cx.entity().downgrade();
                move |delta, cx| {
                    let _ = view.update(cx, |view, cx| view.step_text_size(delta, cx));
                }
            },
        )];
        let sounds = vec![
            prefs::toggle(
                "settings-sounds",
                "Play sounds",
                "A chime when a thread finishes or needs you, unless you are looking at it",
                settings.sounds,
                self.setting_change(cx, |s, v| s.sounds = v),
            ),
            self.sound_file_row("settings-done-sound", "Finished sound", Sound::Done, cx),
            self.sound_file_row(
                "settings-needs-you-sound",
                "Needs-you sound",
                Sound::NeedsYou,
                cx,
            ),
        ];
        let provider_sounds = [Provider::Claude, Provider::Codex].map(|provider| {
            let (id, title) = match provider {
                Provider::Claude => ("settings-claude-sounds", "Sounds for Claude threads"),
                Provider::Codex => ("settings-codex-sounds", "Sounds for Codex threads"),
            };
            prefs::toggle(
                id,
                title,
                "",
                settings.provider_sounds(provider),
                self.setting_change(cx, move |s, v| s.set_provider_sounds(provider, v)),
            )
        });
        let system = vec![prefs::toggle(
            "settings-system-notifications",
            "System notifications",
            "When Ferrite is in the background · clicking one opens its thread",
            settings.system_notifications,
            self.setting_change(cx, |s, v| s.system_notifications = v),
        )];
        let (claude, codex) = self
            .cli_versions
            .clone()
            .unwrap_or_else(|| ("checking…".into(), "checking…".into()));
        let copied = self.settings_copied;
        let copy = |key: &'static str, cx: &Context<Self>| {
            let view = cx.entity().downgrade();
            move |cx: &mut gpui::App| {
                let _ = view.update(cx, |view, cx| {
                    view.settings_copied = Some(key);
                    cx.notify();
                });
            }
        };
        let clis = vec![
            self.cli_update_row("settings-update-claude", Provider::Claude, claude, cx),
            self.cli_update_row("settings-update-codex", Provider::Codex, codex, cx),
            prefs::toggle(
                "settings-auto-update-clis",
                "Update the CLIs automatically",
                "Install a newer Claude or Codex CLI once none of its Threads is running. \
                 New models arrive only with a newer CLI.",
                settings.auto_update_clis,
                self.setting_change(cx, |s, v| s.auto_update_clis = v),
            ),
        ];
        let mut about = vec![
            prefs::fact(
                "Version",
                version_label(env!("CARGO_PKG_VERSION"), crate::titlebar::DEV).into(),
            ),
            prefs::path_fact(
                "Threads",
                self.prefs.dir.join("threads").display().to_string(),
                copied == Some("Threads"),
                copy("Threads", cx),
            ),
            prefs::path_fact(
                "Settings file",
                self.prefs
                    .dir
                    .join(ferrite_core::settings::Settings::FILE)
                    .display()
                    .to_string(),
                copied == Some("Settings file"),
                copy("Settings file", cx),
            ),
            prefs::action(
                "settings-copy-diagnostics",
                "Diagnostics",
                "Version, platform, the CLIs, the newest crash report and the end of \
                 the log, for a bug report",
                ferrite_core::diagnostics::logs(&self.prefs.dir)
                    .display()
                    .to_string()
                    .into(),
                Some(
                    if copied == Some("Diagnostics") {
                        "Copied"
                    } else {
                        "Copy"
                    }
                    .into(),
                ),
                {
                    let view = cx.entity().downgrade();
                    move |cx| {
                        let _ = view.update(cx, |view, cx| view.copy_diagnostics(cx));
                    }
                },
            ),
        ];
        about.splice(1..1, self.ferrite_update_rows(window, cx));
        about.insert(1, self.server_status_row(window, cx));
        let mut about = vec![prefs::Group::new(None).rows(about)];
        let server = self.server_rows(window, cx);
        about.extend(machines);
        about.push(prefs::Group::new(Some("Provider CLIs")).rows(clis));
        let pages = vec![
            prefs::page(prefs::PageKey::NewThreads, new_thread_groups),
            prefs::page(
                prefs::PageKey::Permissions,
                vec![
                    provider_group(Provider::Claude).rows(claude_permissions),
                    provider_group(Provider::Codex).rows(codex_permissions),
                ],
            ),
            prefs::page(
                prefs::PageKey::Behaviour,
                vec![
                    prefs::Group::new(Some("Reading")).rows(reading),
                    prefs::Group::new(Some("Threads and navigation")).rows(behaviour),
                ],
            ),
            prefs::page(
                prefs::PageKey::Notifications,
                vec![
                    prefs::Group::new(Some("Sounds")).rows(sounds),
                    prefs::Group::new(Some("By provider")).rows(provider_sounds),
                    prefs::Group::new(Some("System")).rows(system),
                ],
            ),
            prefs::page(
                prefs::PageKey::Server,
                vec![prefs::Group::new(None).rows(server)],
            ),
            prefs::page(prefs::PageKey::About, about),
        ];

        let select = {
            let view = cx.entity().downgrade();
            let search = search.clone();
            move |page: prefs::PageKey, window: &mut Window, cx: &mut gpui::App| {
                // Choosing a page ends a search: the page shows whole.
                search.update(cx, |search, cx| search.set_value("", window, cx));
                let _ = view.update(cx, |view, cx| {
                    view.settings_page = page;
                    cx.notify();
                });
            }
        };
        let body = prefs::body(pages, self.settings_page, &search, select, window, cx);
        let card = prefs::settings_sheet()
            .id("settings-card")
            .debug_selector(|| "settings-card".into())
            .track_focus(&self.settings_focus)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            // ↑/↓ step the sidebar's pages while the sheet itself holds
            // focus (a field or a control keeps its own arrows).
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, window, cx| {
                let delta = match event.keystroke.key.as_str() {
                    "up" => -1,
                    "down" => 1,
                    _ => return,
                };
                if event.keystroke.modifiers.modified() || !view.settings_focus.is_focused(window) {
                    return;
                }
                cx.stop_propagation();
                view.settings_page = view.settings_page.step(delta);
                cx.notify();
            }))
            .child(prefs::settings_head(
                prefs::sheet_close("settings-close", "Close settings", cx).on_click(cx.listener(
                    |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.settings_open = false;
                        cx.notify();
                    },
                )),
            ))
            .child(body);
        Some(
            deferred(ferrite_ui::motion::veil_in(
                "settings-veil",
                prefs::veil()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.settings_open = false;
                            cx.notify();
                        }),
                    )
                    .child(ferrite_ui::motion::dialog_in("settings-card-in", card)),
            ))
            .with_priority(3)
            .into_any_element(),
        )
    }

    /// Open the card on an existing Project, seeded with its name.
    pub(crate) fn open_project_editor(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let Some(title) = self
            .cockpit
            .registry()
            .project(project)
            .map(|project| project.title.clone())
        else {
            return;
        };
        let name = cx.new(ferrite_pane::composer::Composer::new);
        name.update(cx, |name, cx| {
            name.set_role(ferrite_pane::composer::Role::Field(None), cx);
            name.set(title, cx)
        });
        let remote = self.remote_path(cx);
        self.show_project_editor(
            ProjectEditor {
                target: Some(project),
                name,
                staged: Vec::new(),
                error: None,
                scroll: ScrollHandle::new(),
                remote,
            },
            cx,
        );
    }

    /// Open the card on a Project that does not exist yet: an empty name
    /// and no directories. Nothing is registered until Create.
    fn open_project_creator(&mut self, cx: &mut Context<Self>) {
        let name = cx.new(ferrite_pane::composer::Composer::new);
        name.update(cx, |name, cx| {
            name.set_role(ferrite_pane::composer::Role::Field(None), cx)
        });
        let remote = self.remote_path(cx);
        self.show_project_editor(
            ProjectEditor {
                target: None,
                name,
                staged: Vec::new(),
                error: None,
                scroll: ScrollHandle::new(),
                remote,
            },
            cx,
        );
    }

    /// The card is modal: everything else floating goes down with it.
    fn show_project_editor(&mut self, editor: ProjectEditor, cx: &mut Context<Self>) {
        self.project_editor = Some(editor);
        self.settings_open = false;
        self.popover = None;
        self.context_menu = None;
        self.list_remote_path(cx);
        cx.notify();
    }

    /// Whether the card's confirm can commit — a new Project needs at least
    /// its main directory.
    fn project_editor_ready(&self) -> bool {
        self.project_editor
            .as_ref()
            .is_some_and(|editor| editor.target.is_some() || !editor.staged.is_empty())
    }

    /// Commit the card. Creating registers the staged directories as one
    /// Project — the first is primary — and names it; editing only renames,
    /// because its directory edits already went through. A refusal keeps the
    /// card open with the reason on it, so nothing typed is lost.
    fn confirm_project_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.project_editor.take() else {
            return;
        };
        let typed = editor.name.read(cx).text().to_string();
        let outcome = match editor.target {
            Some(project) => {
                let typed = typed.trim();
                if typed.is_empty() {
                    Ok(project)
                } else {
                    self.cockpit
                        .rename_project(project, typed)
                        .map(|()| project)
                }
            }
            None => {
                let name = editor.resolved_name(&typed);
                self.cockpit
                    .register_project_directories(&editor.staged)
                    .and_then(|project| {
                        match name {
                            Some(name) => self.cockpit.rename_project(project, &name),
                            None => Ok(()),
                        }
                        .map(|()| project)
                    })
            }
        };
        match outcome {
            Ok(project) => {
                // The Project the operator just described is the one they
                // want to look at, and to work in.
                self.launch_project = Some(project);
                self.choose_nav_filter(Some(project), cx);
                if self.panes.is_empty() {
                    self.open_draft_with_provider(
                        DraftTarget::Main,
                        self.prefs.settings.default_provider,
                        cx,
                    );
                }
                self.group_error = None;
                self.refresh_names();
            }
            Err(error) => {
                self.project_editor = Some(ProjectEditor {
                    error: Some(format!("project unchanged · {error}").into()),
                    ..editor
                });
            }
        }
        cx.notify();
    }

    /// One directory of this computer, for the open card (a machine's card
    /// has a path field instead). The picker is single-select
    /// deliberately: a Project's main directory is the first one added, and
    /// a multi-select would leave which-is-which to the file dialog.
    fn browse_for_project_directory(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add directory".into()),
        });
        cx.spawn(async move |this, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => return,
            };
            let Some(path) = path else {
                return;
            };
            this.update(cx, |view, cx| view.adopt_editor_directory(path, cx))
                .ok();
        })
        .detach();
    }

    /// Land a picked directory on the open card. On a new Project it is
    /// staged — and the first one also names the Project, unless a name has
    /// already been typed. On an existing one it is attached at once, so
    /// the card always shows the registry's own list.
    fn adopt_editor_directory(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let Some(editor) = self.project_editor.as_mut() else {
            return;
        };
        match editor.target {
            Some(project) => {
                editor.error = None;
                if let Err(error) = self.cockpit.add_project_directories(project, &[path]) {
                    if let Some(editor) = self.project_editor.as_mut() {
                        editor.error = Some(format!("directory unchanged · {error}").into());
                    }
                }
            }
            None => {
                if editor.staged.contains(&path) {
                    editor.error = Some("that directory is already on this project".into());
                    cx.notify();
                    return;
                }
                let first = editor.staged.is_empty();
                editor.error = None;
                editor.staged.push(path);
                if first {
                    let name = editor.name.clone();
                    let seed = editor.resolved_name("");
                    if name.read(cx).text().trim().is_empty() {
                        if let Some(seed) = seed {
                            name.update(cx, |name, cx| name.set(seed, cx));
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    /// Drop a staged directory from a Project that does not exist yet. If
    /// the main one goes, the next takes its place — the list is ordered,
    /// and the top of it is always primary.
    fn drop_staged_directory(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(editor) = self.project_editor.as_mut() else {
            return;
        };
        if index < editor.staged.len() {
            editor.staged.remove(index);
            editor.error = None;
        }
        cx.notify();
    }

    /// Project mutations stay visible together in one protected surface.
    /// Folder picking may temporarily leave the app, but the card remains
    /// open so the changed directory list is visible on return.
    fn project_editor_element(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editor = self.project_editor.as_ref()?;
        let project = editor
            .target
            .and_then(|project| self.cockpit.registry().project(project));
        // An editing card whose Project was removed underneath it has
        // nothing left to edit.
        if editor.target.is_some() && project.is_none() {
            return None;
        }
        let title = match project {
            Some(project) => SharedString::from(format!("Edit {}", project.title)),
            None => SharedString::from("New project"),
        };
        let directories: Vec<_> = match project {
            Some(project) => std::iter::once(project.root.clone())
                .chain(project.additional_roots.iter().cloned())
                .collect(),
            None => editor.staged.clone(),
        };
        let empty = directories.is_empty();
        let editing = editor.target;

        let close = project_editor::close_button(cx).on_click(cx.listener(
            |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.project_editor = None;
                cx.notify();
            },
        ));
        let mut body = project_editor::body()
            .track_scroll(&editor.scroll)
            .child(project_editor::name_field(
                editor.name.clone(),
                editor.name.read(cx).focus_handle(cx).is_focused(window),
            ))
            .child(project_editor::section_label(
                "Directories",
                "The first is the main directory",
            ));
        let mut rows = Vec::new();
        for (index, directory) in directories.into_iter().enumerate() {
            let mut actions = div().flex().items_center().gap(px(4.));
            // An existing Project's main directory is fixed: Threads,
            // worktrees and titles all hang off it. A staged one is not
            // real yet, so any row can go.
            if editing.is_none() || index > 0 {
                actions = actions.child(
                    project_editor::destructive_button(
                        ("remove-project-directory", index),
                        "Remove",
                        false,
                        cx,
                    )
                    .on_click(cx.listener(
                        move |view, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            let Some(project) = editing else {
                                view.drop_staged_directory(index, cx);
                                return;
                            };
                            let refusal = view
                                .cockpit
                                .remove_project_directory(project, index)
                                .err()
                                .map(|error| {
                                    SharedString::from(format!("directory unchanged · {error}"))
                                });
                            if let Some(editor) = view.project_editor.as_mut() {
                                editor.error = refusal;
                            }
                            cx.notify();
                        },
                    )),
                );
            }
            rows.push(project_editor::directory_row(
                index,
                directory.display().to_string().into(),
                index == 0,
                actions,
            ));
        }
        body = body.child(project_editor::directory_list(rows));
        // On a machine a path field stands in for the folder dialog, which
        // could only browse this computer (ADR 0026).
        body = match &editor.remote {
            Some(remote) => body.child(self.remote_path_element(remote, window, cx)),
            // Its own flex row, so the control hugs its label on the list's edge.
            None => body.child(
                div().flex().child(
                    project_editor::add_directory(
                        if empty {
                            "+ Add main directory"
                        } else {
                            "+ Add directory"
                        },
                        cx,
                    )
                    .on_click(cx.listener(
                        move |view, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            view.browse_for_project_directory(cx);
                        },
                    )),
                ),
            ),
        };
        let mut left = div().flex().items_center();
        if let Some(project) = editing {
            let in_use = self.project_in_use(project);
            left = left.child(
                // The verb stays the verb while it cannot run; why it cannot
                // is the tooltip's to say.
                project_editor::destructive_button("remove-project", "Remove project", in_use, cx)
                    .debug_selector(|| "remove-project".into())
                    .when(in_use, |button| {
                        button.tip("Park or move its threads first")
                    })
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        if view.project_in_use(project) {
                            return;
                        }
                        match view.cockpit.remove_project(project) {
                            Err(error) => {
                                if let Some(editor) = view.project_editor.as_mut() {
                                    editor.error = Some(format!("not removed · {error}").into());
                                }
                            }
                            Ok(()) => {
                                if view.nav_filter == Some(project) {
                                    view.nav_filter = None;
                                }
                                view.group_error = None;
                                view.project_editor = None;
                            }
                        }
                        cx.notify();
                    })),
            );
        }
        let ready = self.project_editor_ready();
        let right = div().flex().items_center().child(
            project_editor::primary_button(
                "confirm-project",
                if editing.is_some() { "Done" } else { "Create" },
                !ready,
                cx,
            )
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                if view.project_editor_ready() {
                    view.confirm_project_editor(cx);
                }
            })),
        );
        let footer = project_editor::footer(left, right);
        if let Some(error) = editor.error.clone() {
            body = body.child(project_editor::error_line(error));
        }

        let card = project_editor::card(f32::from(window.viewport_size().height))
            .id("project-editor-card")
            .debug_selector(|| "project-editor-card".into())
            .track_focus(&self.project_editor_focus)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(project_editor::head(title, close))
            .child(
                // The body scrolls under an overlay bar, never a gutter.
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_shrink(1.)
                    .min_h_0()
                    .child(body)
                    .child(ferrite_ui::components::scrollbar(
                        "project-editor-scrollbar",
                        &editor.scroll,
                    )),
            )
            .child(footer);
        Some(
            deferred(ferrite_ui::motion::veil_in(
                "project-editor-veil",
                project_editor::veil()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.project_editor = None;
                            cx.notify();
                        }),
                    )
                    .child(ferrite_ui::motion::dialog_in(
                        "project-editor-card-in",
                        card,
                    )),
            ))
            .with_priority(3)
            .into_any_element(),
        )
    }

    /// The Pane head's title: the live editor while this Thread is being
    /// renamed from its head, else the name — a double-click opens the
    /// editor, a single click only lands on the Pane. The press stops
    /// there so the cell's own focus does not also clear a selection.
    fn pane_title(&self, index: usize, thread: ThreadId, cx: &mut Context<Self>) -> AnyElement {
        if let Some((RenameTarget::PaneTitle(editing), editor)) = &self.rename {
            if *editing == thread {
                return rename_field(editor.clone())
                    .font_weight(ferrite_ui::theme::W_BODY)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
                    )
                    .into_any_element();
            }
        }
        let face = self.pane_ghost(index);
        let movable = self.board_is_movable();
        pane::head_title(self.panes[index].name.clone())
            .id(("pane-title", thread.get() as usize))
            .debug_selector(move || format!("pane-title-{}", thread.get()))
            .tooltip(ferrite_ui::menu::tooltip("Rename \u{b7} double-click"))
            // On a board the title is a handle too (the L1 head around it
            // is the other): drag it onto another slot to swap or split. It
            // keeps the default cursor — a name, not a button.
            .when(movable, |title| {
                title.on_drag(PaneDrag { leaf: thread }, move |_, grab, _, cx| {
                    let face = face.clone();
                    cx.new(|_| PaneGhost {
                        face,
                        grab,
                        row_left_of: None,
                    })
                })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.focus_pane(index);
                    if event.click_count >= 2 {
                        view.start_rename(RenameTarget::PaneTitle(thread), cx);
                    }
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    /// A Thread's title cell — `editable_group_title`'s twin.
    fn editable_thread_title(
        &self,
        thread: ThreadId,
        title: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some((RenameTarget::Thread(editing), editor)) = &self.rename {
            if *editing == thread {
                return rename_field(editor.clone())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
                    )
                    .into_any_element();
            }
        }
        nav::rename_target_thread(thread, title)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    // As on the Group title, and as on the Pane head: the
                    // first click focuses the Thread, the second renames it.
                    if event.click_count < 2 {
                        return;
                    }
                    cx.stop_propagation();
                    view.start_rename(RenameTarget::Thread(thread), cx);
                }),
            )
            .into_any_element()
    }

    fn pane_for(&self, thread: ThreadId) -> Option<usize> {
        self.panes
            .iter()
            .position(|pane| pane.thread() == Some(thread))
    }

    /// The focused identity's index in this window's mirror. A core act can
    /// receive roster changes before the next pump reconciles the Panes;
    /// another Thread's old index must never receive this one's input.
    fn focused(&self) -> usize {
        self.cockpit
            .roster()
            .focused()
            .and_then(|identity| self.index_of(identity))
            .unwrap_or(0)
    }

    fn focused_thread(&self) -> Option<ThreadId> {
        self.cockpit.roster().focused_thread()
    }

    /// The focused Pane's draft, to change — None on a Thread.
    fn focused_draft_mut(&mut self) -> Option<&mut pane::DraftBinding> {
        let focused = self.focused();
        self.panes
            .get_mut(focused)
            .and_then(|pane| pane.draft_mut())
    }

    fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        // The card's ↵ is its confirm — but in a machine's path field that
        // holds a path, it adds that directory.
        if self.project_editor.is_some() {
            if self.submit_remote_path(window, cx) {
                return;
            }
            if self.project_editor_ready() {
                self.confirm_project_editor(cx);
            }
            return;
        }
        if self.rename.is_some() {
            self.finish_rename(true, cx);
            return;
        }
        // A draft's ↵ (#29): on a band chip it opens that chip's popover;
        // on the prompt line it is the first send — the bootstrap — or a
        // typed-out `/import`.
        if let Some(band_focus) = self
            .panes
            .get(self.focused())
            .and_then(|pane| pane.draft())
            .map(|draft| draft.band_focus)
        {
            match band_focus {
                Some(chip) => self.open_band_popover(chip, cx),
                None if self.typed_import(window, cx) => {}
                None => self.bootstrap_draft(cx),
            }
            return;
        }
        let Some(thread) = self.focused_thread() else {
            return;
        };
        // ⏎ on a parked Group member's tile reopens it in place (F-8):
        // `⏎ reopens it`.
        if self.cockpit.thread(thread).is_none() {
            match self.cockpit.wake(thread) {
                Ok(()) => {
                    self.sync_panes(cx);
                    self.thread_opened(thread, cx);
                    self.parked_changed(cx);
                }
                Err(error) => {
                    ferrite_core::log!("thread {thread} could not be woken: {error:?}")
                }
            }
            cx.notify();
            return;
        }
        if !self.panes[self.focused()].is_main() {
            return;
        }
        // ↵ on an empty line sends a pending question whose every question
        // is answered (digits toggled a multi-select; ↵ sends it).
        if self.panes[self.focused()].composer.read(cx).is_empty() && self.send_ready_question(cx) {
            return;
        }
        if self.typed_import(window, cx) {
            return;
        }
        let composer = self.panes[self.focused()].composer.clone();
        let text = composer.update(cx, |composer, cx| composer.take(cx));
        let text = text.trim().to_string();
        // `/effort max` and `/model sonnet` typed out are Ferrite's own
        // acts, the same as the pickers: the chip stays truthful, Codex
        // (which has no such command) gets them too, and a level or model
        // Ferrite does not know goes to the provider as text.
        if let Some(handled) = self.typed_tuning(thread, &text) {
            match handled {
                Tuning::Effort(effort) => self.pick_effort(thread, effort, cx),
                Tuning::Model(model) => self.pick_provider(
                    thread,
                    ProviderChoice {
                        provider: self
                            .cockpit
                            .thread(thread)
                            .map(|open| open.provider())
                            .unwrap_or(Provider::Claude),
                        model,
                    },
                    cx,
                ),
            }
            return;
        }
        if text.is_empty() {
            // Enter on an empty line takes a held prompt back to edit it.
            if let Some(held) = self.cockpit.unqueue(thread) {
                composer.update(cx, |composer, cx| composer.set(held, cx));
                cx.notify();
            }
            return;
        }
        // Typing does not wait for the agent; sending does.
        if self
            .cockpit
            .thread(thread)
            .is_some_and(|open| open.needs_queue())
        {
            if !self.cockpit.queue(thread, text.clone()) {
                composer.update(cx, |composer, cx| composer.set(text.clone(), cx));
            }
        } else {
            self.cockpit.send(thread, text.clone());
            self.scroll_transcript_to_bottom(self.focused(), cx);
        }
        self.facts.acted(&self.cockpit, thread);
        // A first prompt names an untitled Thread.
        self.facts.renamed(&self.cockpit, thread);
        self.refresh_names();
        if self.is_first_prompt(thread) {
            self.start_titling(thread, text, cx);
        }
        cx.notify();
    }

    /// Backspace on an EMPTY Composer line clears the held prompt — the
    /// `⌫ unqueue` the queued row advertises. With text on the line the
    /// Composer consumes the key first and this never runs.
    fn unqueue_from_backspace(
        &mut self,
        _: &ferrite_pane::composer::Backspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // ⌫ in the palette's line edits the query, never the queue.
        if self.floats.palette.is_some() || !self.panes[self.focused()].is_main() {
            return;
        }
        let Some(thread) = self.focused_thread() else {
            return;
        };
        if !self.panes[self.focused()].composer.read(cx).is_empty() {
            return;
        }
        self.cockpit.cancel_queued(thread, false);
        cx.notify();
    }

    fn interrupt(&mut self, _: &Interrupt, _window: &mut Window, cx: &mut Context<Self>) {
        if self.context_checks.take().is_some() {
            cx.notify();
            return;
        }
        if self.session_controls.take().is_some() {
            cx.notify();
            return;
        }
        if self.context_menu.take().is_some() {
            cx.notify();
            return;
        }
        if self.settings_open {
            self.settings_open = false;
            cx.notify();
            return;
        }
        if self.project_editor.take().is_some() {
            cx.notify();
            return;
        }
        if self.rename.is_some() {
            self.finish_rename(false, cx);
            return;
        }
        if !self.cancel_focused_draft_start() {
            cx.notify();
            return;
        }
        // On a draft, escape returns to the prompt (#29): the band's tab
        // focus clears. (An open band popover holds the ComposerMenu keys,
        // so escape there dismisses through `menu_dismiss` instead.)
        if let Some(draft) = self.focused_draft_mut() {
            draft.band_focus = None;
            cx.notify();
            return;
        }
        // Escape stops what the Pane shows: on Main its turn, never the
        // Subagents and background work it started; on a Subagent's tab
        // that Subagent alone.
        if let Some(thread) = self.focused_thread().filter(|_| !self.machine_lost()) {
            match self.panes[self.focused()].selected.clone() {
                ferrite_core::activity::Subject::Main => self.cockpit.interrupt(thread),
                ferrite_core::activity::Subject::Subagent(key) => self.stop_subagent(thread, &key),
            }
            self.facts.acted(&self.cockpit, thread);
        }
        cx.notify();
    }

    /// Stop the Subagent `key` while it runs. A finished one has nothing to
    /// stop, and its tab's escape must not reach Main instead.
    fn stop_subagent(&mut self, thread: ThreadId, key: &ferrite_core::activity::AgentKey) {
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        let running = open
            .activity()
            .subject(&ferrite_core::activity::Subject::Subagent(key.clone()))
            .is_some_and(|view| {
                matches!(
                    view.status(),
                    ferrite_core::activity::AgentStatus::Pending
                        | ferrite_core::activity::AgentStatus::Working
                        | ferrite_core::activity::AgentStatus::Waiting
                )
            });
        if !running {
            return;
        }
        self.cockpit.stop_subagent(thread, key);
    }

    /// Tab walks a draft's band (#29), or an L1 Thread Pane's rendered tool
    /// disclosures before returning to the Composer.
    /// Tab on an empty Composer that is showing a predicted follow-up puts
    /// that text in the line — editable, unsent. Nothing reaches the provider
    /// without a second, deliberate ↵: ghost text is a guess, and a guess is
    /// not consent to start a turn.
    ///
    /// The condition is `followup::suggest` itself, the same call the idle
    /// line renders from, so the key and the ghost text can never disagree
    /// about whether there is something to accept.
    fn accept_suggestion(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if self.settings_open
            || self.project_editor.is_some()
            || self.rename.is_some()
            || self.popover.is_some()
        {
            return false;
        }
        let Some(thread) = self.focused_thread() else {
            return false;
        };
        let Some(open) = self.cockpit.thread(thread) else {
            return false;
        };
        let offered = followup::suggest(
            open.pending().is_some(),
            Some(open.transcript()),
            open.suggestion(),
        );
        let Some(text) = offered.acceptable().map(str::to_string) else {
            return false;
        };
        let Some(composer) = self
            .panes
            .get(self.focused())
            .map(|pane| pane.composer.clone())
        else {
            return false;
        };
        // Only Tab from the empty input accepts its suggestion. Tab from a
        // transcript action or attachment continues that control's focus walk.
        if !composer.focus_handle(cx).is_focused(window) || !composer.read(cx).is_empty() {
            return false;
        }
        composer.update(cx, |composer, cx| composer.set(text, cx));
        cx.notify();
        true
    }

    fn band_cycle(&mut self, _: &BandCycle, window: &mut Window, cx: &mut Context<Self>) {
        if self.tab_remote_path(window, cx) {
            return;
        }
        if let Some(at) = self
            .popover
            .as_ref()
            .filter(|open| matches!(open.kind, Kind::Commands))
            .map(|open| open.selected)
        {
            self.pick(at, window, cx);
            return;
        }
        if self.accept_suggestion(window, cx) {
            return;
        }
        if self.settings_open
            || self.project_editor.is_some()
            || self
                .focused_thread()
                .and_then(|thread| self.cockpit.thread(thread))
                .is_some_and(|thread| {
                    !thread.activity().children().is_empty()
                        || thread
                            .pending_decisions()
                            .iter()
                            .any(|request| pane::question_of(&request.decision).is_some())
                })
        {
            // Composer is the end of the pane's native tab order. Start its
            // Subject walk at Main instead of wrapping to a sidebar control,
            // which the pane's focus keeper would return to Composer.
            if !self.settings_open && self.project_editor.is_none() {
                if let Some(pane) = self.panes.get(self.focused()) {
                    let has_children = pane
                        .thread()
                        .and_then(|thread| self.cockpit.thread(thread))
                        .is_some_and(|thread| !thread.activity().children().is_empty());
                    if has_children
                        && pane
                            .composer
                            .read(cx)
                            .focus_target(window, cx)
                            .contains_focused(window, cx)
                    {
                        if let Some(focus) = pane.tab_interaction.main_focus() {
                            window.focus(&focus, cx);
                            return;
                        }
                    }
                }
            }
            window.focus_next(cx);
            return;
        }
        let composer = self
            .panes
            .get(self.focused())
            .map(|pane| pane.composer.clone());
        if composer.is_some_and(|composer| {
            composer.update(cx, |composer, cx| composer.cycle_files(false, window, cx))
        }) {
            return;
        }
        let Some(draft) = self.focused_draft_mut() else {
            self.cycle_tools(false, window, cx);
            return;
        };
        draft.band_focus = pane::BandChip::next(draft.band_focus);
        // The popover belongs to the chip it opened from; tab moves on.
        if self
            .popover
            .as_ref()
            .is_some_and(|open| matches!(open.kind, Kind::Band(_)))
        {
            self.popover = None;
        }
        cx.notify();
    }

    fn tool_cycle_previous(
        &mut self,
        _: &ToolCyclePrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_open
            || self.project_editor.is_some()
            || self
                .focused_thread()
                .and_then(|thread| self.cockpit.thread(thread))
                .is_some_and(|thread| {
                    !thread.activity().children().is_empty()
                        || thread
                            .pending_decisions()
                            .iter()
                            .any(|request| pane::question_of(&request.decision).is_some())
                })
        {
            window.focus_prev(cx);
        } else {
            let composer = self
                .panes
                .get(self.focused())
                .map(|pane| pane.composer.clone());
            if !composer.is_some_and(|composer| {
                composer.update(cx, |composer, cx| composer.cycle_files(true, window, cx))
            }) {
                self.cycle_tools(true, window, cx);
            }
        }
    }

    fn cycle_tools(&mut self, reverse: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.level_now(window) != Level::Transcript
            || self.panes[self.focused()].draft().is_some()
        {
            return;
        }
        let focused = self.focused();
        let calls = self.expandable_tools(focused, cx);
        let transcript = self.panes[focused].transcript();
        let tool_focus = self.panes[focused].tool_focus();
        let in_controls = ferrite_pane::rich::code_actions_focused(window);
        let cycle_controls = |from_edge, window: &mut Window, cx: &mut gpui::App| {
            transcript.as_ref().is_some_and(|transcript| {
                transcript.update(cx, |transcript, cx| {
                    transcript.cycle_controls(reverse, from_edge, window, cx)
                })
            })
        };
        // Preserve the existing disclosure walk, then include native actions
        // such as fenced-code Copy/Preview before returning to the composer.
        if in_controls && cycle_controls(false, window, cx) {
            return;
        }
        if reverse
            && !in_controls
            && !tool_focus.is_focused(window)
            && cycle_controls(true, window, cx)
        {
            return;
        }
        let targeted = if in_controls && !reverse {
            false
        } else {
            self.panes[focused].cycle_tools(&calls, reverse).is_some()
        };
        if !targeted && !reverse && !in_controls && cycle_controls(true, window, cx) {
            cx.notify();
            return;
        }
        let focus = if targeted {
            tool_focus
        } else {
            self.panes[focused].composer.focus_handle(cx)
        };
        window.focus(&focus, cx);
        cx.notify();
    }

    fn toggle_tool_action(&mut self, _: &ToggleTool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        let Some(call) = self.panes[self.focused()].targeted_tool().cloned() else {
            return;
        };
        self.toggle_tool(thread, &call, window, cx);
    }

    /// The disclosures the Pane's transcript draws, in order: what the
    /// keyboard walks. The rows decide it — a call folded in a shut group
    /// is not a stop — so it is read from the transcript as last drawn.
    fn expandable_tools(&self, index: usize, cx: &gpui::App) -> Vec<pane::DisclosureId> {
        if self.panes[index].thread().is_none() {
            return Vec::new();
        }
        self.panes[index]
            .transcript()
            .map(|transcript| transcript.read(cx).disclosures())
            .unwrap_or_default()
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
    /// it. At L1 and L2, where a Composer is live; the wall has no line to
    /// be typing into.
    fn answer_or_type(
        &mut self,
        answer: Answer,
        letter: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.panes[self.focused()].is_main() {
            self.answer_subject(answer, cx);
            return;
        }
        if self.level_now(window) == Level::Transcript {
            if let Some(pane) = self.panes.get(self.focused()) {
                // A question is answered in words as often as by a pick,
                // and "no, the second one" starts with the deny key: while
                // a question pends the letters only ever type. Denying a
                // question is the keycap's job.
                let asking = pane
                    .thread()
                    .is_some_and(|thread| self.pending_questions(thread).is_some());
                if asking || !pane.composer.read(cx).is_empty() {
                    pane.composer
                        .clone()
                        .update(cx, |composer, cx| composer.insert(letter, cx));
                    return;
                }
            }
        }
        self.answer(answer, window, cx);
    }

    /// `y` `n` `a` — listed in the shortcuts sheet, not on the row: an
    /// approval takes option 1, 3 or 2 through the one answer path; a
    /// question, form or link takes only the deny.
    fn answer(&mut self, answer: Answer, window: &mut Window, cx: &mut Context<Self>) {
        // The focused Thread if it is the one waiting; otherwise whichever
        // Thread the wall is flagging. Answering from across the room is the
        // point of the badge.
        let thread = match self.focused_thread() {
            Some(thread)
                if self
                    .cockpit
                    .thread(thread)
                    .and_then(|open| open.pending())
                    .is_some() =>
            {
                Some(thread)
            }
            _ => self.cockpit.answer_target(),
        };
        let Some(thread) = thread else {
            return;
        };
        let Some(request) = self.cockpit.thread(thread).and_then(|open| {
            let pending = open.pending()?;
            open.pending_decisions()
                .iter()
                .find(|request| {
                    &request.decision == pending
                        && request.subject == Some(ferrite_core::activity::Subject::Main)
                })
                .cloned()
        }) else {
            return;
        };
        if matches!(request.decision.kind, ferrite_core::DecisionKind::Approval) {
            let choice = match answer {
                Answer::Allow => decision::ApprovalChoice::Allow,
                Answer::Always => decision::ApprovalChoice::AllowForThread,
                Answer::Deny => decision::ApprovalChoice::DenyAndSteer,
            };
            // A denial steers from the Pane the operator is in; one sent
            // across the room leaves the keyboard where it is.
            let steer = self.focused_thread() == Some(thread);
            self.answer_approval_request(
                thread,
                &request,
                choice,
                None,
                steer.then_some(window),
                cx,
            );
        } else {
            // A question is answered by its form, never by a bare "allow" —
            // allowing an unanswered question would send the model nothing.
            self.answer_request(thread, request, answer, cx);
        }
        self.facts.acted(&self.cockpit, thread);
        cx.notify();
    }

    fn next_pane(&mut self, _: &NextPane, _window: &mut Window, cx: &mut Context<Self>) {
        self.board_gives_way();
        self.cockpit.step_focus(1);
        cx.notify();
    }

    fn previous_pane(&mut self, _: &PreviousPane, _window: &mut Window, cx: &mut Context<Self>) {
        self.board_gives_way();
        self.cockpit.step_focus(-1);
        cx.notify();
    }

    /// ⌘⇧⏎ (#20): the focused Pane takes the whole cockpit; ⌘⇧⏎ again
    /// restores the grid. Escape is deliberately not an exit — it stays
    /// Interrupt, and stealing the panic key would make it ambiguous.
    fn toggle_fullscreen(
        &mut self,
        _: &ToggleFullscreen,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cockpit.toggle_fullscreen();
        cx.notify();
    }

    /// A Group Pane head's fill button (SPEC §5.9): its press fills the
    /// board with that Pane — focusing it first, so the one act
    /// (`ToggleFullscreen`, ⌘⇧⏎) fills the board with the Pane pressed.
    fn fill_button(&self, index: usize, thread: ThreadId, cx: &mut Context<Self>) -> AnyElement {
        pane::fill_button(thread.get())
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.focus_pane(index);
                view.cockpit.toggle_fullscreen();
                cx.notify();
            }))
            .into_any_element()
    }

    /// ⌘F: find in the focused Pane's transcript — or in its preview, when
    /// the preview took the last press or holds the keyboard
    /// (`ferrite_pane::find`).
    /// A wall tile draws no transcript to search.
    fn open_find(
        &mut self,
        _: &ferrite_pane::find::Open,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.level_now(window) == Level::Wall {
            return;
        }
        let index = self.focused();
        let Some(pane) = self.panes.get(index) else {
            return;
        };
        let reading =
            self.preview_pressed == Some(pane.identity) || pane.preview.focused(window, cx);
        if reading && self.preview_shown(index) && self.open_preview_find(index, window, cx) {
            return;
        }
        if let Some(transcript) = self.panes.get(index).and_then(|pane| pane.transcript()) {
            transcript.update(cx, |view, cx| view.open_find(window, cx));
        }
    }

    /// cmd-b (#21): fold the nav away, or open it back to the full column.
    /// The width change feeds `cell()`, so Panes may legitimately change
    /// Level — size decides, no special case.
    fn toggle_nav(&mut self, _: &ToggleNav, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_nav_now(cx);
    }

    /// cmd-B, the collapse button: flip what is drawn. While the window
    /// holds the nav folded (`nav_auto_rail`), the flip is an override that
    /// is shown and never saved; otherwise it folds or opens the column.
    fn toggle_nav_now(&mut self, cx: &mut Context<Self>) {
        if !self.nav_collapsed && self.nav_auto_rail.get() {
            let was = self.nav_railed();
            self.nav_forced_open = !self.nav_forced_open;
            self.tween_nav(was, cx);
            return;
        }
        self.set_nav_collapsed(!self.nav_collapsed, cx);
    }

    /// cmd-B rides the column's width between open and folded over
    /// `motion::RESIZE` (Zeron's sidebar), the content fading.
    fn set_nav_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.nav_collapsed == collapsed {
            return;
        }
        let was = self.nav_railed();
        self.nav_collapsed = collapsed;
        self.tween_nav(was, cx);
    }

    /// Ride the column's width from what was drawn to what is drawn now —
    /// and, on its own clock, its content's opacity (1 → 0 folding, 0 → 1
    /// opening, `nav::CONTENT_FADE`). A flip mid-flight turns both around
    /// from where they are on screen.
    fn tween_nav(&mut self, was_railed: bool, cx: &mut Context<Self>) {
        let railed = self.nav_railed();
        if was_railed == railed {
            cx.notify();
            return;
        }
        let now = cx.background_executor().now();
        let reduced = ferrite_ui::motion::reduced_motion(cx);
        let (from, to) = if railed {
            (nav::WIDTH, nav::FOLDED_WIDTH)
        } else {
            (nav::FOLDED_WIDTH, nav::WIDTH)
        };
        self.nav_tween = Some(ferrite_ui::motion::Tween::retarget(
            self.nav_tween,
            from,
            to,
            ferrite_ui::motion::RESIZE,
            now,
            reduced,
        ));
        // Opacity of the content.
        let (from, to) = if railed { (1.0, 0.0) } else { (0.0, 1.0) };
        self.nav_fade = Some(ferrite_ui::motion::Tween::retarget(
            self.nav_fade,
            from,
            to,
            nav::CONTENT_FADE,
            now,
            reduced,
        ));
        cx.notify();
    }

    // --------------------------------------------------- Composer menus (#23)

    /// Re-derive the text-derived popover from the focused line's own text.
    /// Nothing else opens or closes a menu: `/filter` at the caret opens commands,
    /// an `@token` under the caret opens files, anything else closes. A
    /// picker or a band chip holds the slot while it is up and is left
    /// alone here.
    fn sync_menu(&mut self, cx: &mut Context<Self>) {
        if self
            .popover
            .as_ref()
            .is_some_and(|open| !open.kind.follows_text())
        {
            return;
        }
        self.popover = self.derive_menu(cx);
    }

    fn derive_menu(&mut self, cx: &mut Context<Self>) -> Option<Popover> {
        // Muted until the text moves again.
        if self.menu_muted {
            return None;
        }
        let pane = self.panes.get(self.focused())?;
        let thread = pane.thread();
        let (text, cursor) = {
            let composer = pane.composer.read(cx);
            (composer.text().to_string(), composer.cursor())
        };
        let mention = mention_token(&text, cursor);
        // A slash inside an active @path belongs to file completion.
        if let Some((token_start, filter)) =
            slash_token(&text, cursor).filter(|_| mention.is_none())
        {
            // Local actions consume the entire command line. Inside prose,
            // offer provider skills only so a pick cannot discard the draft.
            let standalone = token_start == 0 && cursor == text.len();
            let Some(thread) = thread else {
                let identity = pane.identity;
                let draft = pane.draft()?;
                let provider = draft.binding.provider().provider;
                let cwd = match draft
                    .binding
                    .resolve(self.cockpit.registry(), !self.on_machine())
                {
                    Ok(workspace) => workspace.source_root().to_path_buf(),
                    Err(_) => {
                        // Import remains available when a workspace was removed.
                        // Never reuse commands from the previous valid binding.
                        self.draft_commands = None;
                        if !standalone {
                            return None;
                        }
                        let row =
                            local_row(filter, "import", "Resume a Claude or Codex session", false)?;
                        return Some(Popover {
                            scroll: ScrollHandle::new(),
                            revealed: Default::default(),
                            hold: None,
                            pane: identity,
                            kind: Kind::Commands,
                            rows: vec![Row {
                                row,
                                active: false,
                                consequence: Consequence::OpenImportPicker,
                            }],
                            selected: 0,
                        });
                    }
                };
                if self
                    .draft_commands
                    .as_ref()
                    .is_none_or(|menu| menu.provider != provider || menu.cwd != cwd)
                {
                    let pending = self.cockpit.discover_commands(provider, &cwd);
                    self.draft_commands = Some(DraftCommands {
                        provider,
                        cwd,
                        pending,
                        commands: Vec::new(),
                        error: None,
                    });
                }
                let menu = self.draft_commands.as_mut().expect("initialized above");
                menu.poll();
                let mut rows: Vec<Row> = command_rows(&menu.commands, filter)
                    .into_iter()
                    .map(|row| Row {
                        consequence: Consequence::Command(row.insert.clone()),
                        row,
                        active: false,
                    })
                    .collect();
                if let Some(row) = standalone
                    .then(|| local_row(filter, "import", "Resume a Claude or Codex session", false))
                    .flatten()
                {
                    rows.retain(|existing| existing.row.name != row.name);
                    rows.insert(
                        0,
                        Row {
                            row,
                            active: false,
                            consequence: Consequence::OpenImportPicker,
                        },
                    );
                }
                let status = if menu.pending.is_some() {
                    Some(("Loading commands…", ""))
                } else {
                    menu.error
                        .as_deref()
                        .map(|error| ("Could not load commands", error))
                };
                if let Some((name, detail)) = status {
                    rows.push(Row {
                        row: pane::MenuRow {
                            insert: "".into(),
                            name: name.into(),
                            detail: detail.to_string().into(),
                            matched: Vec::new(),
                            prose_detail: true,
                            inert: true,
                        },
                        active: false,
                        consequence: Consequence::Inert,
                    });
                }
                if rows.is_empty() {
                    return None;
                }
                return Some(Popover {
                    scroll: ScrollHandle::new(),
                    revealed: Default::default(),
                    hold: None,
                    pane: identity,
                    kind: Kind::Commands,
                    rows,
                    selected: 0,
                });
            };
            let open = self.cockpit.thread(thread)?;
            let mut rows: Vec<Row> = command_rows(open.commands(), filter)
                .into_iter()
                .map(|row| Row {
                    consequence: Consequence::Command(row.insert.clone()),
                    row,
                    active: false,
                })
                .collect();
            // Ferrite's local rows ride on top, through the same fuzzy
            // filter and under the same cap as every row. #11: `import`
            // always — in the Thread's place while it is blank, else
            // alongside. #25: `provider` always — live before the first
            // prompt, and kept visible but inert after it, so the door's
            // absence never reads as a bug.
            // A local row replaces a provider command of the same name
            // (Claude lists its own `effort`): Ferrite's opens the picker,
            // and the typed form (`/effort max`) still works — `submit`
            // reads it.
            let push_local = |rows: &mut Vec<Row>, row: Row| {
                rows.retain(|existing| existing.row.name != row.row.name);
                rows.insert(0, row);
                rows.truncate(MENU_ROWS_MAX);
            };
            if standalone {
                if let Some(row) =
                    local_row(filter, "import", "Resume a Claude or Codex session", false)
                {
                    push_local(
                        &mut rows,
                        Row {
                            row,
                            active: false,
                            consequence: Consequence::OpenImportPicker,
                        },
                    );
                }
            }
            // The model can change at any time, and so can the provider:
            // after the first prompt the other one starts a fresh
            // conversation with the earlier one handed over as context,
            // which the picker itself explains.
            let detail = "Switch model or hand over to the other provider";
            if let Some(row) = standalone
                .then(|| local_row(filter, "effort", "Switch reasoning effort", false))
                .flatten()
            {
                push_local(
                    &mut rows,
                    Row {
                        row,
                        active: false,
                        consequence: Consequence::OpenEffortPicker,
                    },
                );
            }
            if let Some(row) = standalone
                .then(|| local_row(filter, "model", detail, false))
                .flatten()
            {
                push_local(
                    &mut rows,
                    Row {
                        row,
                        active: false,
                        consequence: Consequence::OpenProviderPicker,
                    },
                );
            }
            // No match, no popover — there is nothing to pick.
            if rows.is_empty() {
                return None;
            }
            return Some(Popover {
                scroll: ScrollHandle::new(),
                revealed: Default::default(),
                hold: None,
                pane: pane.identity,
                kind: Kind::Commands,
                rows,
                selected: 0,
            });
        }
        let (token_start, filter) = mention?;
        // No binding → nothing to walk → no popover.
        let root = match (thread, pane.draft()) {
            (Some(thread), _) => self
                .cockpit
                .thread(thread)?
                .workspace()?
                .cwd()
                .to_path_buf(),
            (None, Some(draft)) => draft
                .binding
                .resolve(self.cockpit.registry(), !self.on_machine())
                .ok()?
                .source_root()
                .to_path_buf(),
            _ => return None,
        };
        if let Some(thread) = thread {
            if let Ok(receiver) = self.cockpit.search_files(thread, filter) {
                let request_id = NEXT_FILE_SEARCH.fetch_add(1, Ordering::Relaxed);
                let generation = self.cockpit.thread(thread)?.generation();
                self.pending_files = Some(PendingFileSearch {
                    pane: pane.identity,
                    thread,
                    generation,
                    request_id,
                    query: filter.to_string(),
                    token_start,
                    root: root.clone(),
                    started: std::time::Instant::now(),
                    reply: receiver,
                    fallback: None,
                });
                return Some(Popover {
                    scroll: ScrollHandle::new(),
                    revealed: Default::default(),
                    hold: None,
                    pane: pane.identity,
                    kind: Kind::Files {
                        files: std::rc::Rc::new(Vec::new()),
                        token_start,
                        query: filter.to_string(),
                        request_id,
                    },
                    rows: vec![Row {
                        row: pane::MenuRow {
                            insert: SharedString::default(),
                            name: "Searching files…".into(),
                            matched: Vec::new(),
                            detail: SharedString::default(),
                            prose_detail: true,
                            inert: true,
                        },
                        active: false,
                        consequence: Consequence::Inert,
                    }],
                    selected: 0,
                });
            }
        }
        // A machine's files are its Sessions' to search (ADR 0026).
        if self.on_machine() {
            return None;
        }
        // The walk runs once per open menu; keystrokes only re-filter it.
        let walked = match &self.popover {
            Some(open) if open.pane == pane.identity => match &open.kind {
                Kind::Files { files, .. } => Some(files.clone()),
                _ => None,
            },
            _ => None,
        };
        let files = walked.unwrap_or_else(|| {
            std::rc::Rc::new(ferrite_core::workspace::mention_files(
                &root,
                MENTION_FILE_CAP,
            ))
        });
        let rows: Vec<Row> = mention_rows(&files, filter)
            .into_iter()
            .map(|row| Row {
                consequence: Consequence::Mention(row.insert.clone()),
                row,
                active: false,
            })
            .collect();
        if rows.is_empty() {
            return None;
        }
        Some(Popover {
            scroll: ScrollHandle::new(),
            revealed: Default::default(),
            hold: None,
            pane: pane.identity,
            kind: Kind::Files {
                files,
                token_start,
                query: filter.to_string(),
                request_id: 0,
            },
            rows,
            selected: 0,
        })
    }

    /// The menu keys serve whichever popover is up in the Composer's slot:
    /// the picker while it is open (#11, #25), the `/`/`@` menu otherwise
    /// — one key context, no second table.
    fn menu_next(&mut self, _: &MenuNext, _window: &mut Window, cx: &mut Context<Self>) {
        self.step_popover(1, cx);
    }

    fn menu_previous(&mut self, _: &MenuPrevious, _window: &mut Window, cx: &mut Context<Self>) {
        self.step_popover(-1, cx);
    }

    fn history_older(&mut self, _: &HistoryOlder, window: &mut Window, cx: &mut Context<Self>) {
        self.recall_history(HistoryDirection::Older, window, cx);
    }

    fn history_newer(&mut self, _: &HistoryNewer, window: &mut Window, cx: &mut Context<Self>) {
        self.recall_history(HistoryDirection::Newer, window, cx);
    }

    fn recall_history(
        &mut self,
        direction: HistoryDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = self.focused();
        if !self.history_available(index, self.level_now(window)) {
            return;
        }
        let Some(thread) = self.panes[index].thread() else {
            return;
        };
        if self
            .cockpit
            .thread(thread)
            .is_some_and(|open| open.queued().is_some())
        {
            if matches!(direction, HistoryDirection::Older) {
                self.cockpit.edit_queued(thread);
                cx.notify();
            }
            return;
        }
        let composer = self.panes[index].composer.clone();
        let draft = composer.read(cx).editable();
        let Some(text) = self.cockpit.recall_prompt(thread, direction, &draft) else {
            return;
        };
        self.popover = None;
        self.menu_muted = true;
        self.suppress_recall_menu_once = true;
        composer.update(cx, |composer, cx| composer.set(text, cx));
        cx.notify();
    }

    fn history_available(&self, index: usize, level: Level) -> bool {
        if level != Level::Transcript || index != self.focused() {
            return false;
        }
        let Some(thread) = self.panes.get(index).and_then(|pane| pane.thread()) else {
            return false;
        };
        let Some(open) = self.cockpit.thread(thread) else {
            return false;
        };
        !self.panes[index].has_tool_target()
            && self.rename.is_none()
            && self.popover.is_none()
            && ((self.panes[index].is_main() && open.queued().is_some())
                || (open.has_prompt_history()
                    && !open.busy()
                    && open.pending().is_none()
                    && open.queued().is_none()))
    }

    /// Clamp-step the open popover's selection.
    fn step_popover(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(open) = &mut self.popover else {
            return;
        };
        // Inert rows (a picker's section headers) are skipped over: the
        // arrows land only where ↵ would do something.
        let mut stepped = open.selected;
        loop {
            let next = stepped.saturating_add_signed(delta);
            if next >= open.rows.len() || (delta < 0 && stepped == 0) {
                break;
            }
            stepped = next;
            if !open.rows[stepped].inert {
                break;
            }
        }
        if open.rows.get(stepped).is_some_and(|row| row.inert) {
            stepped = open.selected;
        }
        if stepped != open.selected {
            open.selected = stepped;
            open.scroll.scroll_to_item(stepped);
            cx.notify();
        }
    }

    fn menu_pick(&mut self, _: &MenuPick, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = self.popover.as_ref().map(|open| open.selected) {
            self.pick(at, window, cx);
        }
    }

    /// Escape while a popover is up closes it and nothing else — the text
    /// stays, and escape's Interrupt meaning waits for the next press. A
    /// text-derived menu mutes until the text moves; a picker takes no
    /// mute, since nothing reopens it; a band chip's popover also returns
    /// the chip's tab focus to the prompt line.
    fn menu_dismiss(&mut self, _: &MenuDismiss, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.popover.take() else {
            return;
        };
        match open.kind {
            Kind::Band(_) => {
                if let Some(draft) = self.focused_draft_mut() {
                    draft.band_focus = None;
                }
            }
            Kind::Commands | Kind::Files { .. } => self.menu_muted = true,
            Kind::Provider | Kind::Effort => {}
        }
        cx.notify();
    }

    /// The shared tail of ↵ and a row click: the row's own consequence,
    /// dispatched. The popover closes either way — a door it opens (the
    /// pickers) is a fresh popover in its place.
    fn pick(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        // The native menu can outlive the frame in which it opened. A turn
        // that started meanwhile, or a change still waiting on the
        // provider, must not turn a visible choice into a guaranteed
        // refusal in the transcript.
        if let Some(open) = self.popover.as_ref() {
            if matches!(open.kind, Kind::Provider | Kind::Effort)
                && open.pane.thread().is_some_and(|thread| {
                    self.cockpit
                        .thread(thread)
                        .and_then(|thread| tuning_hold(&thread))
                        .is_some()
                })
            {
                self.refresh_model_picker(cx);
                return;
            }
        }
        let Some(open) = self.popover.take() else {
            return;
        };
        let Some(row) = open.rows.get(at) else {
            return;
        };
        let Some(composer) = self
            .index_of(open.pane)
            .map(|index| self.panes[index].composer.clone())
        else {
            return;
        };
        // Local command actions consume a standalone command line.
        let splice_line = |cx: &mut Context<Self>, text: &str| {
            composer.update(cx, |composer, cx| {
                let whole = 0..composer.text().len();
                composer.splice(whole, text, cx);
            });
        };
        match &row.consequence {
            Consequence::Command(name) => {
                composer.update(cx, |composer, cx| {
                    let cursor = composer.cursor();
                    let text = composer.text();
                    let Some((start, _)) = slash_token(text, cursor) else {
                        return;
                    };
                    // Complete the whole token even when editing its middle.
                    let mut end = cursor
                        + text[cursor..]
                            .find(char::is_whitespace)
                            .unwrap_or(text.len() - cursor);
                    // Reuse an existing separator instead of inserting two spaces.
                    if text[end..].starts_with(' ') {
                        end += 1;
                    }
                    composer.splice(start..end, &format!("/{name} "), cx);
                });
            }
            Consequence::Inert => {}
            Consequence::OpenProviderPicker => {
                if let Some(thread) = open.pane.thread() {
                    splice_line(cx, "");
                    self.open_provider_picker(thread, cx);
                }
            }
            Consequence::OpenImportPicker => {
                // A draft's one line is the `/import` its pick replaces; a
                // Thread is replaced only while it is blank.
                let into = match open.pane {
                    PaneIdentity::Draft(_) => Some(open.pane),
                    PaneIdentity::Thread(_) => {
                        splice_line(cx, "");
                        self.import_target(cx)
                    }
                };
                self.open_import_palette(into, window, cx);
            }
            Consequence::Mention(path) => {
                let Kind::Files { token_start, .. } = &open.kind else {
                    return;
                };
                let start = *token_start;
                let token = format!("@{path}");
                composer.update(cx, |composer, cx| {
                    let cursor = composer.cursor();
                    composer.splice(start..cursor, &format!("{token} "), cx);
                    composer.stage_mention(SharedString::from(token), cx);
                });
            }
            Consequence::Provision(choice) => {
                if let Some(thread) = open.pane.thread() {
                    self.pick_provider(thread, choice.clone(), cx);
                }
            }
            Consequence::OpenEffortPicker => {
                if let Some(thread) = open.pane.thread() {
                    splice_line(cx, "");
                    // The model picker carries the effort row (its ladder,
                    // ←/→): `/effort` opens it, as the palette's `effort`.
                    self.open_provider_picker(thread, cx);
                }
            }
            Consequence::Effort(effort) => {
                if let Some(thread) = open.pane.thread() {
                    self.pick_effort(thread, effort.clone(), cx);
                }
            }
            Consequence::Band(choice) => self.pick_band(choice, &composer, cx),
        }
        cx.notify();
    }

    fn poll_navigation(&mut self, cx: &mut Context<Self>) {
        use std::sync::mpsc::TryRecvError;
        if let Some(mut pending) = self.pending_files.take() {
            let current = self.panes.get(self.focused()).is_some_and(|pane| pane.identity == pending.pane)
                && self.cockpit.thread(pending.thread).is_some_and(|live| live.generation() == pending.generation)
                && self.popover.as_ref().is_some_and(|open| open.pane == pending.pane && matches!(&open.kind, Kind::Files { request_id, query, token_start, .. } if *request_id == pending.request_id && query == &pending.query && *token_start == pending.token_start));
            if current {
                let rows = if let Some(fallback) = &pending.fallback {
                    match fallback.try_recv() {
                        Ok(rows) => Some(rows),
                        Err(TryRecvError::Empty) => None,
                        Err(TryRecvError::Disconnected) => Some(Vec::new()),
                    }
                } else {
                    match pending.reply.try_recv() {
                        Ok(Ok(files)) => Some(native_mention_rows(&files)),
                        Err(TryRecvError::Empty)
                            if pending.started.elapsed() < Duration::from_secs(5) =>
                        {
                            None
                        }
                        // A machine's files are walked by none of this
                        // computer's threads (ADR 0026).
                        _ if self.on_machine() => Some(Vec::new()),
                        _ => {
                            let root = pending.root.clone();
                            let query = pending.query.clone();
                            let (tx, rx) = std::sync::mpsc::channel();
                            if std::thread::Builder::new()
                                .name("ferrite-file-fallback".into())
                                .spawn(move || {
                                    let paths = ferrite_core::workspace::mention_files(
                                        &root,
                                        MENTION_FILE_CAP,
                                    );
                                    let _ = tx.send(mention_rows(&paths, &query));
                                })
                                .is_ok()
                            {
                                pending.fallback = Some(rx);
                                None
                            } else {
                                Some(Vec::new())
                            }
                        }
                    }
                };
                if let Some(rows) = rows {
                    if let Some(open) = &mut self.popover {
                        open.rows = rows
                            .into_iter()
                            .map(|row| Row {
                                consequence: Consequence::Mention(row.insert.clone()),
                                row,
                                active: false,
                            })
                            .collect();
                        open.selected = 0;
                    }
                    cx.notify();
                } else {
                    self.pending_files = Some(pending);
                }
            }
        }
        self.poll_session_discovery(cx);
    }

    /// Ferrite's own words to one Pane: a Notice in a Thread's transcript,
    /// or the error line under a draft's band.
    fn say_to(&mut self, pane: PaneIdentity, message: String) {
        match pane {
            PaneIdentity::Thread(thread) => self.cockpit.say(thread, message),
            PaneIdentity::Draft(_) => {
                if let Some(draft) = self
                    .index_of(pane)
                    .and_then(|index| self.panes[index].draft_mut())
                {
                    draft.error = Some(message.into());
                }
            }
        }
    }

    /// The model picker in the Composer slot: one section per Provider —
    /// its logomark row, then its models under the names the Provider's
    /// own menu shows — with the ✓ on what is serving. Claude's rows come
    /// from its handshake; Codex's from the catalog. The catalog stays
    /// inspectable during a turn, with unavailable choices and their reason.
    fn open_provider_picker(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        let current = open.provider();
        let chosen = open.model().map(str::to_string);
        let serving = open.transcript().model().map(str::to_string);
        let locked = open.first_prompt_sent();
        let hold = tuning_hold(&open);
        let (mut rows, selected) = self.provider_rows(
            current,
            chosen.as_deref(),
            serving.as_deref(),
            locked,
            |choice| Consequence::Provision(choice),
        );
        Self::hold_tuning_rows(&mut rows, hold);
        // The `/` menu the pick came through is already closed; a chip
        // click replaces whatever the slot held outright.
        self.popover = Some(Popover {
            scroll: ScrollHandle::new(),
            revealed: Default::default(),
            hold,
            pane: PaneIdentity::Thread(thread),
            kind: Kind::Provider,
            rows,
            selected,
        });
        cx.notify();
    }

    /// Keep the current checkmark and catalog inspectable while choices
    /// cannot be applied: every row disabled, the reason (`Popover::hold`)
    /// in their tooltips (SPEC §5.5).
    fn hold_tuning_rows(rows: &mut [Row], hold: Option<&'static str>) {
        if hold.is_some() {
            for row in rows.iter_mut() {
                row.row.inert = true;
            }
        }
    }

    /// The sectioned rows every model picker shows — the Composer's and
    /// the draft band's alike. `chosen` is the standing choice (None = the
    /// Provider's default), `serving` what the Session's Init named. The
    /// ✓ lands on the chosen row, else on the row serving; the arrows
    /// start there, so a bare ↵ keeps everything as it is.
    fn provider_rows(
        &self,
        current: Provider,
        chosen: Option<&str>,
        serving: Option<&str>,
        locked: bool,
        consequence: impl Fn(ProviderChoice) -> Consequence,
    ) -> (Vec<Row>, usize) {
        let mut rows: Vec<Row> = Vec::new();
        let mut selected = None;
        for provider in [Provider::Claude, Provider::Codex] {
            // After the first prompt the other Provider is a handover, not
            // a swap: its rows stay live and its section says what a pick
            // does — a fresh conversation there, the earlier one carried
            // over as context.
            let handover = locked && provider != current;
            let fixed = false;
            rows.push(Row {
                row: pane::MenuRow {
                    insert: SharedString::default(),
                    name: SharedString::from(provider_title(provider)),
                    matched: Vec::new(),
                    detail: SharedString::from(if handover { "handover" } else { "" }),
                    prose_detail: true,
                    inert: true,
                },
                active: false,
                consequence: Consequence::Inert,
            });
            let mut catalog = self.cockpit.model_catalog(provider);
            if provider == current {
                if let Some(chosen) = chosen {
                    if !catalog.iter().any(|row| row.is(chosen)) {
                        catalog.push(ferrite_core::ModelInfo::bare(chosen));
                    }
                }
            }
            for model in catalog {
                let active = provider == current
                    && match chosen {
                        Some(chosen) => model.value == chosen,
                        None => {
                            model.value == "default"
                                || (serving.is_some_and(|serving| model.is(serving))
                                    && !self
                                        .cockpit
                                        .model_catalog(provider)
                                        .iter()
                                        .any(|row| row.value == "default"))
                        }
                    };
                if active {
                    selected = Some(rows.len());
                }
                // The picker's voice (the prototype's `#picker`): the CLI's
                // own default reads `Default`, a name in sentence case as
                // every model's is (SPEC §7), every description
                // lowercase-first.
                let name = if model.value == "default" {
                    "Default".to_string()
                } else {
                    model.display.clone()
                };
                rows.push(Row {
                    row: pane::MenuRow {
                        insert: SharedString::default(),
                        name: SharedString::from(name),
                        matched: Vec::new(),
                        detail: SharedString::from(ferrite_core::providers::models::groom_detail(
                            &model.detail,
                        )),
                        prose_detail: true,
                        inert: fixed,
                    },
                    active,
                    consequence: if fixed {
                        Consequence::Inert
                    } else {
                        consequence(ProviderChoice {
                            provider,
                            // The Provider's own default is the absence of
                            // a choice, never the word on the wire.
                            model: Some(model.value.clone()).filter(|value| value != "default"),
                        })
                    },
                });
            }
        }
        let selected =
            selected.unwrap_or_else(|| rows.iter().position(|row| !row.inert).unwrap_or(0));
        (rows, selected)
    }

    /// A model-row pick uses the current Provider's live model control;
    /// another Provider is `set_provider`'s pre-prompt swap or handover.
    /// A refusal changed nothing, and the core's own words land in this
    /// Thread's transcript.
    fn pick_provider(&mut self, thread: ThreadId, choice: ProviderChoice, cx: &mut Context<Self>) {
        let same_provider = self
            .cockpit
            .thread(thread)
            .is_some_and(|open| open.provider() == choice.provider);
        let result = if same_provider {
            self.cockpit.set_model(thread, choice.model)
        } else {
            self.cockpit.set_provider(thread, choice)
        };
        if let Err(e) = result {
            self.cockpit.say(thread, format!("model unchanged: {e}"));
        }
        self.facts.acted(&self.cockpit, thread);
        cx.notify();
    }

    /// An import pick (#11): adopt the picked file through the core door —
    /// in place of `into`, the blank Pane the picker was opened for (a
    /// draft becomes the imported Thread and its line clears; a blank
    /// Thread yields its slot), else alongside the Panes shown. A refusal
    /// is the core's readable words, surfaced in that Pane (a toast with
    /// none) — and the door stays open for the next try.
    pub(super) fn adopt_file(
        &mut self,
        into: Option<PaneIdentity>,
        path: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The Pane may have gone while the picker was up.
        let into = into.filter(|pane| self.index_of(*pane).is_some());
        match self.cockpit.adopt_into(into, path) {
            Ok(adopted) => {
                let draft = into.filter(|pane| pane.draft().is_some());
                if let Some(error) = &adopted.not_opened {
                    // Durable but not on screen: the Thread sits in the
                    // nav's parked rows, exactly like a launch-time import
                    // that would not open.
                    ferrite_core::log!(
                        "imported thread {} would not open: {error:?}",
                        adopted.thread
                    );
                } else {
                    // The import has focus: an empty board shown over the
                    // open Threads would hide it.
                    self.empty_shown = false;
                    if let Some(index) = draft.and_then(|draft| self.index_of(draft)) {
                        self.panes[index].composer.update(cx, |composer, cx| {
                            composer.take(cx);
                        });
                        self.panes[index].adopt_thread(adopted.thread);
                    }
                }
                if let Some(error) = &adopted.blank_kept {
                    ferrite_core::log!("the blank thread stayed open: {error}");
                }
                self.sync_panes(cx);
                // A draft that became the import keeps its Pane, so the
                // mirror saw nothing open; a Thread that would not open is
                // a fresh parked row.
                if adopted.not_opened.is_some() {
                    self.parked_changed(cx);
                } else if draft.is_some() {
                    self.thread_opened(adopted.thread, cx);
                }
            }
            Err(e) => {
                let message = format!("cannot import {}: {e}", path.display());
                match into {
                    Some(pane) => self.say_to(pane, message),
                    None => {
                        use gpui::component::notification::{Notification, NotificationType};
                        use gpui::component::WindowExt as _;
                        window.push_notification(
                            Notification::new()
                                .title("Session not imported")
                                .message(message)
                                .with_type(NotificationType::Error)
                                .autohide(false),
                            cx,
                        );
                    }
                }
            }
        }
        cx.notify();
    }

    // ------------------------------------------------ draft Pane + band (#29)

    /// cmd-t's answer (#29): a draft Pane — a Composer, the pre-prompt
    /// band, and nothing durable until the first send bootstraps a Thread.
    /// The provider and the project follow the Pane the operator is on;
    /// `target` is the caller's (cmd-shift-n drafts
    /// straight onto "new worktree").
    fn open_draft(&mut self, target: DraftTarget, cx: &mut Context<Self>) {
        self.open_draft_with_placement(target, DraftPlacement::Loose, cx);
    }

    fn open_draft_in_current_view(&mut self, target: DraftTarget, cx: &mut Context<Self>) {
        self.open_draft_with_placement(target, DraftPlacement::CurrentGroup, cx);
    }

    fn open_draft_with_placement(
        &mut self,
        target: DraftTarget,
        placement: DraftPlacement,
        cx: &mut Context<Self>,
    ) {
        let provider = self
            .panes
            .get(self.focused())
            .map(|pane| match pane.draft() {
                Some(draft) => draft.binding.provider().clone(),
                None => {
                    let open = pane.thread().and_then(|thread| self.cockpit.thread(thread));
                    ProviderChoice {
                        provider: open.map_or(Provider::Claude, |open| open.provider()),
                        model: open.and_then(|open| open.model()).map(str::to_string),
                    }
                }
            })
            .unwrap_or_else(|| self.default_choice());
        self.open_draft_with_choice(target, provider, placement, None, cx);
    }

    /// What a new Thread starts on when nothing on screen says otherwise:
    /// the settings' provider and its chosen model.
    fn default_choice(&self) -> ProviderChoice {
        let provider = self.prefs.settings.default_provider;
        ProviderChoice {
            provider,
            model: self.prefs.settings.model_for(provider).map(str::to_string),
        }
    }

    fn open_draft_with_provider(
        &mut self,
        target: DraftTarget,
        provider: Provider,
        cx: &mut Context<Self>,
    ) {
        self.open_draft_with_choice(
            target,
            ProviderChoice {
                provider,
                model: None,
            },
            DraftPlacement::Loose,
            None,
            cx,
        );
    }

    /// Point navigation at a Project — or back at all of them — and bring
    /// any standing draft with it. The Composer's chip names the Project a
    /// send will run in, and it must not sit there disagreeing with the
    /// Project the operator just chose (the ⌘K palette's filter command,
    /// or a Project just created). Filtered, the tree is that Project's
    /// section. A draft already bootstrapping is on its way out and is left
    /// alone.
    pub(crate) fn choose_nav_filter(&mut self, project: Option<ProjectId>, cx: &mut Context<Self>) {
        self.nav_filter = project;
        if let Some(project) = project {
            let standing = self.panes.iter().position(|pane| {
                pane.identity
                    .draft()
                    .is_some_and(|draft| !self.cockpit.draft_starting(draft))
            });
            if let Some(draft) = standing.and_then(|index| self.panes[index].draft_mut()) {
                // The same move the Project chip makes, so the workspace
                // choice under it cannot outlive the repo it named.
                draft.binding.choose_project(project);
                draft.error = None;
            }
        }
        cx.notify();
    }

    /// How the nav orders the rows inside each Project (N-3): creation
    /// order, or `sort: recent` (last used first) — the ⌘K palette's sort
    /// command. Saved with the settings.
    pub(crate) fn set_thread_order(&mut self, order: ThreadListOrder, cx: &mut Context<Self>) {
        if self.prefs.settings.thread_list_order == order {
            return;
        }
        // A fresh order is drawn at once, never held from the old one.
        self.nav_snapshot.borrow_mut().take();
        self.change_settings(|settings| settings.thread_list_order = order, cx);
    }

    /// Fold or unfold what one of the nav's folds holds (N-4) — a Project
    /// section, the Groups section, a Group — at once. In memory only: the
    /// fold survives re-renders and view switches, and opening a folded
    /// Group leaves it folded.
    pub(crate) fn set_nav_fold(&mut self, fold: NavFold, folded: bool, cx: &mut Context<Self>) {
        if folded {
            self.nav_folds.insert(fold);
        } else {
            self.nav_folds.remove(&fold);
        }
        cx.notify();
    }

    /// Show the parked Threads (N-18): the nav unfolds if it is folded away
    /// and shows the Parked view — the ⌘K palette's "show parked" command.
    pub(crate) fn show_parked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_nav_view(nav::NavView::Parked, Some(window), cx);
    }

    /// Show `view` in the tree's place (§5.1.2): the nav unfolds if it is
    /// folded away, the list starts at its top, and its keys take the
    /// keyboard when there is a window to hand it to — the Needs-you list
    /// the Needs-you view's (`notice_focus`, the cursor on its first row), any other
    /// list Esc's way back to the tree. The tree hands the keyboard back.
    pub(crate) fn show_nav_view(
        &mut self,
        view: nav::NavView,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if view != nav::NavView::Tree && self.nav_railed() {
            self.toggle_nav_now(cx);
        }
        if self.nav_view != view {
            self.nav_scroll.set_offset(gpui::point(px(0.), px(0.)));
        }
        self.nav_view = view;
        if view == nav::NavView::NeedsYou {
            self.needs_you.cursor = 0;
        }
        if let Some(window) = window {
            match view {
                nav::NavView::Tree => {}
                nav::NavView::NeedsYou => window.focus(&self.floats.notice_focus, cx),
                _ => window.focus(&self.nav_view_focus, cx),
            }
        }
        cx.notify();
    }

    /// A view row's press, or the act behind it: `view`, or the tree when
    /// it is already shown (§5.1.2: a click on the active view returns to
    /// the tree).
    pub(crate) fn toggle_nav_view(
        &mut self,
        view: nav::NavView,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let next = if self.nav_view == view {
            nav::NavView::Tree
        } else {
            view
        };
        self.show_nav_view(next, window, cx);
    }

    /// A new Thread in a named Project — the ⌘K palette's "new thread in
    /// <project>". It is a loose draft like every pointer-made one, and it
    /// moves the fallback Project, so the draft after it starts here too.
    pub(crate) fn open_draft_in_project(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        self.launch_project = Some(project);
        let provider = self.default_choice();
        self.open_draft_with_choice(
            DraftTarget::Main,
            provider,
            DraftPlacement::Loose,
            Some(project),
            cx,
        );
    }

    /// `chosen` is a Project the caller is naming outright, which outranks
    /// every guess below — the filter, the current Group, the fallback.
    fn open_draft_with_choice(
        &mut self,
        target: DraftTarget,
        provider: ProviderChoice,
        placement: DraftPlacement,
        chosen: Option<ProjectId>,
        cx: &mut Context<Self>,
    ) {
        self.board_gives_way();
        // The draft starts in the Project of the Thread the operator is in:
        // the focused one, else the one focus last left.
        let current = match placement {
            DraftPlacement::NewGroupWith(thread) => Some(thread),
            _ => self.cockpit.roster().current_thread(),
        }
        .and_then(|thread| self.cockpit.project_id(thread));
        // A second press with a draft already up must not stack another:
        // focus the standing draft and re-aim it at the caller's target.
        // A draft mid-bootstrap is on its way out and does not count.
        if let Some(index) = self.panes.iter().position(|pane| {
            pane.identity
                .draft()
                .is_some_and(|draft| !self.cockpit.draft_starting(draft))
        }) {
            // The standing draft is re-aimed at the Project as well: the
            // chip must not sit there naming the Project of an earlier
            // press while the operator is in another.
            let aim = chosen.or(current);
            let identity = self.panes[index].identity;
            if let Some(draft) = self.panes[index].draft_mut() {
                draft.binding.choose_target(target);
                if let Some(aim) = aim {
                    draft.binding.choose_project(aim);
                    draft.error = None;
                }
            }
            // Re-aim its placement too, not only its target: a standing
            // draft left over from an earlier press must still become the
            // Group the operator is asking for now. Read the view before
            // focusing the draft.
            if let Some(draft) = identity.draft() {
                match placement {
                    DraftPlacement::CurrentGroup => self.cockpit.aim_draft_at_current_view(draft),
                    DraftPlacement::Loose => self.cockpit.aim_draft_loose(draft),
                    DraftPlacement::NewGroupWith(thread) => {
                        self.cockpit.aim_draft_at_new_group(draft, thread)
                    }
                }
            }
            self.cockpit.focus(identity);
            self.sync_panes(cx);
            cx.notify();
            return;
        }
        // A Project the caller named outright wins, then the current
        // Thread's. With no Thread to go by, a chosen nav filter names the
        // Project the operator is looking at, and only then does the draft
        // start where the last one ran.
        let project = chosen
            .or(current)
            .or(self.nav_filter)
            .or(self.launch_project);
        let Some(project) = project else {
            self.open_project_creator(cx);
            return;
        };
        let binding = pane::DraftBinding {
            binding: ferrite_core::draft::DraftBinding::new(provider, project, target),
            band_focus: None,
            error: None,
        };
        // Shortcuts retain an active Group; pointer/menu creation opens a
        // loose Draft and switches the Cockpit to Solo. Both take focus.
        let draft = match placement {
            DraftPlacement::CurrentGroup => self.cockpit.open_draft_in_current_view(),
            DraftPlacement::Loose => self.cockpit.open_draft(),
            DraftPlacement::NewGroupWith(thread) => self.cockpit.open_draft_for_new_group(thread),
        };
        let pane = CockpitPane::new(PaneView::new_draft(draft, binding, cx), cx);
        cx.subscribe(&pane.composer, Self::composer_edited).detach();
        self.panes.push(pane);
        self.sync_panes(cx);
        cx.notify();
    }

    /// Open one chip's popover on the focused draft — the shared tail of a
    /// chip click and ↵ on a tab-focused chip. Toggles shut when the same
    /// chip's popover is already up. Rows are registry reads, discovered at
    /// open — never per frame, never a filesystem scan. Git is asked, off
    /// the UI thread, what the checkout is on now and — for the workspace
    /// chip — what the repo holds; the rows take each answer as it lands.
    fn open_band_popover(&mut self, chip: pane::BandChip, cx: &mut Context<Self>) {
        let Some(pane) = self.panes.get(self.focused()) else {
            return;
        };
        let Some(draft) = pane.draft() else {
            return;
        };
        if self.popover.as_ref().is_some_and(|open| {
            matches!(open.kind, Kind::Band(up) if up == chip) && open.pane == pane.identity
        }) {
            self.popover = None;
            cx.notify();
            return;
        }
        let identity = pane.identity;
        let project = draft.binding.project();
        // The menu answers about the checkout as it stands now; until git
        // says, it names the last answer.
        self.look_up_checkouts([project], cx);
        let rows = self.band_rows(draft, chip, cx);
        // The arrows start on the standing choice — bare ↵ re-picks it.
        let selected = rows.iter().position(|row| row.active).unwrap_or(0);
        self.popover = Some(Popover {
            scroll: ScrollHandle::new(),
            revealed: Default::default(),
            hold: None,
            pane: identity,
            kind: Kind::Band(chip),
            rows,
            selected,
        });
        if chip == pane::BandChip::Workspace {
            self.refresh_repo_listing(project, cx);
        }
        cx.notify();
    }

    /// Ask git, off the UI thread, what `project`'s repo holds for the
    /// workspace popover — its worktrees and its local branches — and fill
    /// the open popover's rows in when the answer lands. One listing per
    /// open, never per frame. On a machine, its Server asks git there.
    fn refresh_repo_listing(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        if self.on_machine() {
            self.ask_machine_repo(project);
            return;
        }
        let Some(root) = self
            .cockpit
            .registry()
            .project(project)
            .map(|project| project.root.clone())
        else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let listing = cx
                .background_executor()
                .spawn(async move { ferrite_core::workspace::repo_listing(&root) })
                .await;
            this.update(cx, |view, cx| {
                view.repo_listing = Some((project, listing));
                view.sync_workspace_rows(cx);
            })
            .ok();
        })
        .detach();
    }

    /// The repo listing landed while the workspace popover is up: rebuild
    /// its rows and keep the highlighted choice by value, as a finished
    /// model discovery does for the provider picker.
    fn sync_workspace_rows(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self.popover.as_ref() else {
            return;
        };
        if !matches!(open.kind, Kind::Band(pane::BandChip::Workspace)) {
            return;
        }
        let Some(draft) = self
            .panes
            .iter()
            .find(|pane| pane.identity == open.pane)
            .and_then(|pane| pane.draft())
        else {
            return;
        };
        let rows = self.band_rows(draft, pane::BandChip::Workspace, cx);
        let Some(open) = &mut self.popover else {
            return;
        };
        let highlighted = open.rows.get(open.selected);
        open.selected = highlighted
            .and_then(|chosen| {
                rows.iter()
                    .position(|row| row.consequence == chosen.consequence)
            })
            .unwrap_or(0);
        open.rows = rows;
        cx.notify();
    }

    /// What `project`'s checkout is on, as git last answered, so the chip
    /// can name it every frame without asking. A Project not named yet is
    /// asked about off the UI thread and reads `main` until the answer
    /// lands — the name a Project git cannot answer for keeps, the branch
    /// a fresh clone would be sitting on.
    fn checked_out_branch(&self, project: ProjectId, cx: &Context<Self>) -> SharedString {
        if let Some(branch) = self.checked_out.borrow().get(&project) {
            return branch.clone();
        }
        self.look_up_checkouts([project], cx);
        SharedString::from(MAIN_BRANCH)
    }

    /// Ask git, off the UI thread, what each of `projects`' checkouts is
    /// on. Callable from a frame: it only starts the lookup, once per
    /// Project at a time, and `checkouts_read` takes the answers. Never on
    /// a machine, whose checkouts no git here can read.
    fn look_up_checkouts(&self, projects: impl IntoIterator<Item = ProjectId>, cx: &Context<Self>) {
        if self.on_machine() {
            return;
        }
        let mut checking = self.checking_out.borrow_mut();
        let mut roots: Vec<(ProjectId, std::path::PathBuf)> = Vec::new();
        for project in projects {
            if checking.contains(&project) || roots.iter().any(|(asked, _)| *asked == project) {
                continue;
            }
            if let Some(found) = self.cockpit.registry().project(project) {
                roots.push((project, found.root.clone()));
            }
        }
        if roots.is_empty() {
            return;
        }
        checking.extend(roots.iter().map(|(project, _)| *project));
        cx.spawn(async move |this, cx| {
            let answers = cx
                .background_executor()
                .spawn(async move {
                    roots
                        .into_iter()
                        .map(|(project, root)| {
                            (project, ferrite_core::workspace::checkout_branch(&root))
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |view, cx| view.checkouts_read(answers, cx))
                .ok();
        })
        .detach();
    }

    /// Checkout answers, back from off the UI thread. Only a changed
    /// answer repaints — the chip, and an open workspace menu's rows.
    fn checkouts_read(
        &mut self,
        answers: Vec<(ProjectId, Option<String>)>,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        {
            let mut checking = self.checking_out.borrow_mut();
            let mut known = self.checked_out.borrow_mut();
            for (project, branch) in answers {
                checking.remove(&project);
                let branch =
                    branch.map_or_else(|| SharedString::from(MAIN_BRANCH), SharedString::from);
                if known.get(&project) != Some(&branch) {
                    known.insert(project, branch);
                    changed = true;
                }
            }
        }
        if changed {
            self.sync_workspace_rows(cx);
            cx.notify();
        }
    }

    /// One chip's rows for the focused draft. The workspace chip is scoped
    /// to the chosen project alone: its checked-out branch, each of the
    /// repo's other worktrees (as git lists them, hand-made ones included),
    /// each other local branch, and the two ways to start a fresh branch —
    /// no global list anywhere. The git half comes from `repo_listing`,
    /// read once at open; until it lands the rows are the checkout and the
    /// fresh starts.
    fn band_rows(
        &self,
        draft: &pane::DraftBinding,
        chip: pane::BandChip,
        cx: &Context<Self>,
    ) -> Vec<Row> {
        match chip {
            pane::BandChip::Provider => {
                let (rows, _) = self.provider_rows(
                    draft.binding.provider().provider,
                    draft.binding.provider().model.as_deref(),
                    None,
                    false,
                    |choice| Consequence::Band(BandChoice::Provider(choice)),
                );
                rows
            }
            pane::BandChip::Effort => {
                let provider = draft.binding.provider().provider;
                let default = self.prefs.settings.effort_for(provider);
                let mut rows = vec![band_row(
                    SharedString::from("Default"),
                    SharedString::from(match default {
                        Some(default) => format!("{} · from Settings", effort_title(default)),
                        None => "The CLI's own choice".to_string(),
                    }),
                    draft.binding.effort().is_none(),
                    BandChoice::Effort(None),
                )];
                for effort in ferrite_core::providers::models::efforts_for(
                    provider,
                    draft.binding.provider().model.as_deref(),
                    &self.cockpit.announced_models(provider),
                ) {
                    rows.push(band_row(
                        SharedString::from(effort_title(&effort)),
                        SharedString::from(effort_detail(&effort)),
                        draft.binding.effort() == Some(effort.as_str()),
                        BandChoice::Effort(Some(effort)),
                    ));
                }
                rows
            }
            pane::BandChip::Project => {
                let mut rows: Vec<Row> = self
                    .cockpit
                    .registry()
                    .projects()
                    .iter()
                    .map(|project| {
                        let detail = if project.additional_roots.is_empty() {
                            project.root.display().to_string()
                        } else {
                            let count = project.additional_roots.len();
                            format!(
                                "{} · +{count} director{}",
                                project.root.display(),
                                if count == 1 { "y" } else { "ies" }
                            )
                        };
                        band_row(
                            SharedString::from(project.title.clone()),
                            SharedString::from(detail),
                            draft.binding.project() == project.id,
                            BandChoice::Project(project.id),
                        )
                    })
                    .collect();
                // Explicit type-a-path grammar. Ordinary drafted prose is
                // never reinterpreted or erased as registry input.
                let typed = self
                    .panes
                    .get(self.focused())
                    .map(|pane| pane.composer.read(cx).text().trim().to_string())
                    .unwrap_or_default();
                if let Some(path) = typed
                    .strip_prefix("path ")
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                {
                    rows.push(band_row(
                        SharedString::from(format!("add {path}")),
                        SharedString::from("register path"),
                        false,
                        // As typed: `~` is the home of the computer the
                        // Server runs on, which spells it out.
                        BandChoice::RegisterPath(path.into()),
                    ));
                }
                // The same verb the nav's dropdown ends on, opening the
                // same card: one way to add a Project, wherever it is
                // asked for.
                rows.push(band_row(
                    SharedString::from("Add project…"),
                    SharedString::from("name it and choose its directories"),
                    false,
                    BandChoice::AddProject,
                ));
                rows
            }
            pane::BandChip::Workspace => {
                // Where the repo already stands, then the fresh starts:
                // the checkout as it is, each other worktree, each other
                // branch, then a new branch — beside the operator, or off
                // in an isolated worktree.
                let project = draft.binding.project();
                let target = draft.binding.target();
                let checked_out = self.checked_out_branch(project, cx);
                let mut rows = vec![band_row(
                    checked_out.clone(),
                    SharedString::from("checked out"),
                    *target == DraftTarget::Main,
                    BandChoice::Target(DraftTarget::Main),
                )];
                let listing = self
                    .repo_listing
                    .as_ref()
                    .filter(|(listed, _)| *listed == project)
                    .map(|(_, listing)| listing);
                // A branch already on offer as the checkout or as a
                // worktree is not offered again as a plain branch: git
                // would refuse to switch the checkout onto it anyway.
                let mut offered: Vec<&str> = vec![checked_out.as_ref()];
                for worktree in listing
                    .map(|listing| listing.worktrees.as_slice())
                    .unwrap_or_default()
                {
                    let directory = worktree
                        .path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| worktree.path.display().to_string());
                    let (name, detail) = match &worktree.branch {
                        Some(branch) => (branch.clone(), format!("worktree · {directory}")),
                        None => (directory, "worktree · detached".to_string()),
                    };
                    if let Some(branch) = &worktree.branch {
                        offered.push(branch);
                    }
                    rows.push(band_row(
                        SharedString::from(name.clone()),
                        SharedString::from(detail),
                        matches!(
                            target,
                            DraftTarget::Existing { path, .. } if *path == worktree.path
                        ),
                        BandChoice::Target(DraftTarget::Existing {
                            branch: name,
                            path: worktree.path.clone(),
                        }),
                    ));
                }
                for branch in listing
                    .map(|listing| listing.branches.as_slice())
                    .unwrap_or_default()
                {
                    if offered.contains(&branch.as_str()) {
                        continue;
                    }
                    rows.push(band_row(
                        SharedString::from(branch.clone()),
                        SharedString::from("existing branch"),
                        matches!(target, DraftTarget::Branch { name } if name == branch),
                        BandChoice::Target(DraftTarget::Branch {
                            name: branch.clone(),
                        }),
                    ));
                }
                rows.push(band_row(
                    SharedString::from("New branch"),
                    SharedString::from("in the project checkout"),
                    *draft.binding.target() == DraftTarget::NewBranch,
                    BandChoice::Target(DraftTarget::NewBranch),
                ));
                rows.push(band_row(
                    SharedString::from("New branch + worktree"),
                    SharedString::from("in an isolated checkout"),
                    *draft.binding.target() == DraftTarget::New,
                    BandChoice::Target(DraftTarget::New),
                ));
                rows
            }
        }
    }

    /// Re-derive the open project popover's rows from the Composer line —
    /// the type-a-path row follows the typing, exactly as the `/` menu's
    /// rows follow theirs.
    fn sync_band_rows(&mut self, cx: &mut Context<Self>) {
        let open = self
            .popover
            .as_ref()
            .is_some_and(|open| matches!(open.kind, Kind::Band(pane::BandChip::Project)));
        if !open {
            return;
        }
        let Some(draft) = self.panes.get(self.focused()).and_then(|pane| pane.draft()) else {
            return;
        };
        let rows = self.band_rows(draft, pane::BandChip::Project, cx);
        if let Some(open) = &mut self.popover {
            open.selected = open.selected.min(rows.len().saturating_sub(1));
            open.rows = rows;
        }
    }

    /// Discovery can finish while a picker is open. Rebuild its rows and
    /// preserve the highlighted choice by value, even if row order changes.
    fn refresh_model_picker(&mut self, cx: &mut Context<Self>) {
        let Some(previous) = self.popover.take() else {
            return;
        };
        match (&previous.kind, previous.pane) {
            (Kind::Provider, PaneIdentity::Thread(thread)) => self.open_provider_picker(thread, cx),
            (Kind::Effort, PaneIdentity::Thread(thread)) => self.open_effort_picker(thread, cx),
            (
                Kind::Band(chip @ (pane::BandChip::Provider | pane::BandChip::Effort)),
                PaneIdentity::Draft(_),
            ) => {
                self.open_band_popover(*chip, cx);
            }
            _ => {
                self.popover = Some(previous);
                return;
            }
        }
        if let (Some(open), Some(selected)) =
            (&mut self.popover, previous.rows.get(previous.selected))
        {
            if let Some(index) = open
                .rows
                .iter()
                .position(|row| row.consequence == selected.consequence)
            {
                open.selected = index;
            }
        }
    }

    /// Stop the submitted startup before Escape or a choice changes its draft.
    fn cancel_focused_draft_start(&mut self) -> bool {
        let Some(id) = self
            .panes
            .get(self.focused())
            .and_then(|pane| pane.identity.draft())
        else {
            return true;
        };
        match self.cockpit.cancel_draft_start(id) {
            Ok(_) => true,
            Err(error) => {
                if let Some(draft) = self.focused_draft_mut() {
                    draft.error = Some(error.to_string().into());
                }
                false
            }
        }
    }

    /// A band row's pick, applied to the focused draft. Changing the
    /// project resets the workspace chip to `main` — the old choice named
    /// another repo's rows.
    fn pick_band(
        &mut self,
        choice: &BandChoice,
        composer: &Entity<Composer>,
        cx: &mut Context<Self>,
    ) {
        if !self.cancel_focused_draft_start() {
            return;
        }
        match choice {
            BandChoice::Provider(provider) => {
                let announced = self.cockpit.announced_models(provider.provider);
                if let Some(draft) = self.focused_draft_mut() {
                    draft.binding.choose_provider(provider.clone(), &announced);
                    draft.error = None;
                }
            }
            BandChoice::Effort(effort) => {
                if let Some(draft) = self.focused_draft_mut() {
                    draft.binding.choose_effort(effort.clone());
                    draft.error = None;
                }
            }
            BandChoice::Project(project) => {
                if let Some(draft) = self.focused_draft_mut() {
                    draft.binding.choose_project(*project);
                    draft.error = None;
                }
                // An explicit pick is the operator saying where they are
                // working now, so the *next* draft starts there too — the
                // same answer a relaunch would read off the store, without
                // waiting for the relaunch.
                self.launch_project = Some(*project);
            }
            BandChoice::RegisterPath(path) => match self.cockpit.register_project(path) {
                Ok(project) => {
                    if let Some(draft) = self.focused_draft_mut() {
                        draft.binding.choose_checkout(project);
                        draft.error = None;
                    }
                    // The typed path was the pick's input, not a prompt:
                    // the line clears for one.
                    composer.update(cx, |composer, cx| {
                        let whole = 0..composer.text().len();
                        composer.splice(whole, "", cx);
                    });
                }
                Err(e) => {
                    if let Some(draft) = self.focused_draft_mut() {
                        draft.error = Some(SharedString::from(format!(
                            "cannot register {}: {e}",
                            path.display()
                        )));
                    }
                }
            },
            BandChoice::Target(target) => {
                if let Some(draft) = self.focused_draft_mut() {
                    draft.binding.choose_target(target.clone());
                    draft.error = None;
                }
            }
            BandChoice::AddProject => self.open_project_creator(cx),
        }
        cx.notify();
    }

    /// The first send (#29): resolve the draft's ids through the registry,
    /// then the core act — create, worktree, spawn, the prompt, and the
    /// Thread taking the draft's own slot; the band is gone for the life of
    /// the Thread. On any failure nothing is half-born: no Thread, the Pane
    /// stays draft, the error shows where the band is, and the prompt stays
    /// in the Composer.
    fn bootstrap_draft(&mut self, cx: &mut Context<Self>) {
        let Some(pane) = self.panes.get(self.focused()) else {
            return;
        };
        let (Some(draft), Some(id)) = (pane.draft(), pane.identity.draft()) else {
            return;
        };
        let composer = pane.composer.clone();
        let text = composer.read(cx).prompt().trim().to_string();
        if text.is_empty() {
            return;
        }
        let provider = draft.binding.provider().clone();
        let effort = draft.binding.effort().map(str::to_owned);
        let resolved = draft
            .binding
            .resolve(self.cockpit.registry(), !self.on_machine())
            .map_err(|e| e.to_string());
        let opened = resolved.and_then(|choice| {
            self.cockpit
                .bootstrap_draft(id, provider, choice, &text, effort.clone())
                .map_err(|e| e.to_string())
        });
        self.finish_draft_start(id, &text, opened, cx);
        cx.notify();
    }

    fn finish_draft_start(
        &mut self,
        id: ferrite_core::roster::DraftId,
        text: &str,
        opened: Result<Option<ferrite_core::cockpit::Bootstrapped>, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .panes
            .iter()
            .position(|pane| pane.identity.draft() == Some(id))
        else {
            return;
        };
        match opened {
            Ok(Some(done)) => {
                let composer = self.panes[index].composer.clone();
                composer.update(cx, |composer, cx| {
                    // Edits made while startup ran belong to the operator.
                    if composer.prompt().trim() == text {
                        composer.take(cx);
                    }
                });
                self.panes[index].adopt_thread(done.thread);
                if done.applied_leave {
                    self.group_error = None;
                }
                if let Some(error) = done.refused_leave {
                    ferrite_core::log!("group change refused: {error}");
                    self.group_error = Some(error.to_string().into());
                }
                if self.popover.as_ref().is_some_and(|open| {
                    open.pane == PaneIdentity::Draft(id) && matches!(open.kind, Kind::Band(_))
                }) {
                    self.popover = None;
                }
                self.sync_panes(cx);
                self.thread_opened(done.thread, cx);
                self.refresh_names();
                self.start_titling(done.thread, text.to_string(), cx);
                self.scroll_transcript_to_bottom(index, cx);
            }
            Ok(None) => {
                if let Some(draft) = self.panes[index].draft_mut() {
                    draft.error = None;
                }
            }
            Err(message) => {
                if let Some(draft) = self.panes[index].draft_mut() {
                    draft.error = Some(SharedString::from(message));
                }
            }
        }
    }

    /// The focused draft's setup chips (#29) — Project and Workspace — for
    /// the Composer's toolbar, wired to their popovers, which hang above
    /// them (`band_menu_element`). A chip click shares `open_band_popover`
    /// with ↵ on a tab-focused chip; the closure re-finds the Pane by its
    /// Composer, a draft's one stable identity.
    fn draft_band_chips(&self, index: usize, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(draft) = self.panes[index].draft() else {
            return Vec::new();
        };
        let composer = self.panes[index].composer.clone();
        let project_title = self
            .cockpit
            .registry()
            .project(draft.binding.project())
            .map(|project| project.title.clone())
            .unwrap_or_else(|| "Project".into());
        let workspace_label = match draft.binding.target() {
            DraftTarget::Main => self.checked_out_branch(draft.binding.project(), cx),
            DraftTarget::Branch { name } => SharedString::from(name.clone()),
            DraftTarget::NewBranch => SharedString::from("New branch"),
            DraftTarget::Existing { branch, .. } => SharedString::from(branch.clone()),
            DraftTarget::New => SharedString::from("New worktree"),
        };
        let chips = [
            (
                pane::BandChip::Project,
                pane::band_chip_label(&project_title),
            ),
            (
                pane::BandChip::Workspace,
                pane::band_chip_label(&workspace_label),
            ),
        ];
        chips
            .into_iter()
            .enumerate()
            .map(|(slot, (chip, label))| {
                let focused = draft.band_focus == Some(chip);
                let chip_composer = composer.clone();
                let chip_element = pane::band_chip(slot, label, focused).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        // The chip is this Pane's: land on it first, then
                        // toggle — and stop the press so the root's dismissal
                        // cannot close what this just opened.
                        cx.stop_propagation();
                        if let Some(at) = view
                            .panes
                            .iter()
                            .position(|pane| pane.composer == chip_composer)
                        {
                            view.focus_pane(at);
                        }
                        view.open_band_popover(chip, cx);
                    }),
                );
                ferrite_ui::components::on_bounds(
                    chip_element,
                    self.record_trigger(band_trigger(chip)),
                )
                .into_any_element()
            })
            .collect()
    }

    /// The focused draft's model and effort chips (#29): the chips a live
    /// Thread's toolbar draws, in the same place, so a new Thread and an
    /// existing one share one silhouette. They open the band's popovers
    /// rather than the Thread's. The effort chip stays put, as the band is
    /// a tab-stop sequence: with no effort resolving it reads the picker's
    /// own `Default` row — the value in force.
    fn draft_model_chips(&self, index: usize, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(draft) = self.panes[index].draft() else {
            return Vec::new();
        };
        let provider = draft.binding.provider().provider;
        // The provider's own word until a model is chosen; the model's name
        // after — one spelling, wherever it shows.
        let model_label = match draft.binding.provider().model.as_deref() {
            Some(model) => SharedString::from(ferrite_core::providers::models::label(
                model,
                &self.cockpit.model_catalog(provider),
            )),
            None => SharedString::from(provider_title(provider)),
        };
        let effort_label = effort_value(
            draft.binding.effort(),
            self.prefs.settings.effort_for(provider),
            provider,
            draft.binding.provider().model.as_deref(),
            &self.cockpit.announced_models(provider),
        )
        .map_or_else(
            || SharedString::from("Default"),
            |label| pane::sentence_case(&label).into(),
        );
        let controls = [
            (
                pane::BandChip::Provider,
                pane::draft_picker(
                    "draft-model-picker",
                    draft.band_focus == Some(pane::BandChip::Provider),
                    pane::model_chip(Some(provider), model_label),
                    cx,
                ),
            ),
            (
                pane::BandChip::Effort,
                pane::draft_picker(
                    "draft-effort-picker",
                    draft.band_focus == Some(pane::BandChip::Effort),
                    pane::effort_chip(effort_label),
                    cx,
                )
                .tip("Reasoning effort"),
            ),
        ];
        controls
            .into_iter()
            .map(|(chip, control)| {
                self.choice_menu(index, Kind::Band(chip), control, cx)
                    .into_any_element()
            })
            .collect()
    }

    /// The Solo Composer's toolbar (SPEC §5.5): the `+` that attaches
    /// files; a draft's Project and Workspace chips, or a Thread's
    /// permission mode; then, at the right, the model and effort chips and
    /// the send circle. None on a board (its compact Composer is input
    /// only, SPEC §5.9) and on a Subagent's tab, whose footer stands in.
    fn composer_toolbar(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pane = &self.panes[index];
        if !pane.is_main() {
            return None;
        }
        // A board Pane's compact Composer (SPEC §5.9) has no toolbar: its
        // one control is the send circle, on the focused Pane only.
        if self.grid_board() {
            return (index == self.focused()).then(|| self.send_button(index, true, cx));
        }
        let attach = pane::attach_button(cx)
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.attach_files(index, cx);
            }))
            .into_any_element();
        let (lead, trail) = match pane.thread() {
            None => (
                self.draft_band_chips(index, cx),
                self.draft_model_chips(index, cx),
            ),
            Some(_) => (
                self.mode_chip(index, cx).into_iter().collect(),
                self.model_chips(index, cx),
            ),
        };
        let send = self.send_button(index, false, cx);
        Some(pane::composer_toolbar(attach, lead, trail, send).into_any_element())
    }

    /// The send circle: Enter's act — or, while Main is busy (a turn runs
    /// or its Session starts) and the line is empty, `stop`, Esc's. The
    /// `compact` Composer's always sends (queued behind a running turn):
    /// its Pane says `Queue a follow-up`.
    fn send_button(&self, index: usize, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let pane = &self.panes[index];
        let running = pane
            .thread()
            .and_then(|thread| self.cockpit.thread(thread))
            .is_some_and(|open| open.busy());
        let stop = !compact && running && pane.composer.read(cx).is_empty();
        pane::send_button(stop, compact, cx)
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                view.focus_pane(index);
                let focus = view.panes[index].composer.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
                match view.panes[index].thread() {
                    Some(thread) if stop => {
                        view.cockpit.interrupt(thread);
                        view.facts.acted(&view.cockpit, thread);
                        cx.notify();
                    }
                    _ => view.submit(&Submit, window, cx),
                }
            }))
            .into_any_element()
    }

    /// The toolbar's `+`: the system's file picker, its picks staged on
    /// this Pane's Composer as a drop or a paste stages them.
    fn attach_files(&mut self, index: usize, cx: &mut Context<Self>) {
        let composer = self.panes[index].composer.clone();
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn(async move |_, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            composer.update(cx, |composer, cx| composer.add_files(&paths, cx));
        })
        .detach();
    }

    /// Test-only: aim the launch project at a scratch repo — production
    /// registers `here()` once at construction, which tests cannot sit in.
    #[cfg(test)]
    fn aim_launch(&mut self, root: &std::path::Path) {
        self.launch_project = Some(
            self.cockpit
                .register_project(root)
                .expect("the scratch repo registers"),
        );
        for pane in &mut self.panes {
            if let Some(draft) = pane.draft_mut() {
                draft
                    .binding
                    .choose_project(self.launch_project.expect("the test aimed it"));
            }
        }
    }

    /// The open popover for this Pane, which hangs it above the line.
    fn popover_element(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self
            .popover
            .as_ref()
            .filter(|open| open.pane == self.panes[index].identity)?;
        // The pickers are `ChoiceMenu`s, and a draft's band menus hang off
        // their chips in the bar (`band_menu_element`).
        if open.kind.picker_slot().is_some() || matches!(open.kind, Kind::Band(_)) {
            return None;
        }
        Some(self.menu_surface(open, cx))
    }

    /// A draft's Project or Workspace menu, hung off its chip in the bar
    /// (ADR 0014): its left edge at the chip's, its foot `FLOAT_OFFSET`
    /// over the bar, kept inside the window's margin. The chip has been
    /// drawn before it can be pressed, so its bounds are always recorded.
    fn band_menu_element(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        use ferrite_ui::theme::{FLOAT_OFFSET, GRID_PAD, SLASH_MENU_W};
        let open = self.popover.as_ref()?;
        let Kind::Band(chip @ (pane::BandChip::Project | pane::BandChip::Workspace)) = open.kind
        else {
            return None;
        };
        let trigger = self
            .float_triggers
            .borrow()
            .get(band_trigger(chip).as_ref())
            .copied()?;
        // Above its chip, in the Composer's toolbar.
        let floor = f32::from(trigger.top()) - FLOAT_OFFSET;
        let width = SLASH_MENU_W.min(f32::from(window.viewport_size().width) - 2. * GRID_PAD);
        Some(
            deferred(
                anchored()
                    .anchor(gpui::Anchor::BottomLeft)
                    .position(gpui::point(trigger.left(), px(floor)))
                    .snap_to_window_with_margin(px(GRID_PAD))
                    .child(div().w(px(width)).child(self.menu_surface(open, cx))),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// The open popover's surface, rows wired to their picks — assembled
    /// here so its clicks land beside every other pointer wire (the root
    /// selector's precedent). Every row is the one menu row: `/`, `@` and
    /// import rows through `pane::menu_row` (slash descriptions in one
    /// aligned column), band rows through `pane::picker_row` with the
    /// strong check on the standing choice; an inert explanation
    /// ("Searching files…", a refusal) is a `menu_note`. The rows scroll
    /// inside the popover's cap; the footer stays.
    fn menu_surface(&self, open: &Popover, cx: &mut Context<Self>) -> AnyElement {
        fn wire<E: InteractiveElement + IntoElement + 'static>(
            drawn: E,
            at: usize,
            cx: &mut Context<CockpitView>,
        ) -> AnyElement {
            drawn
                .debug_selector(move || format!("composer-menu-row-{at}"))
                .on_mouse_move(cx.listener(move |view, _: &gpui::MouseMoveEvent, _, cx| {
                    if let Some(open) = &mut view.popover {
                        if matches!(open.kind, Kind::Commands)
                            && open.selected != at
                            && open.rows.get(at).is_some_and(|row| !row.inert)
                        {
                            open.selected = at;
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        view.pick(at, window, cx);
                    }),
                )
                .into_any_element()
        }
        // Slash commands align their descriptions in one column sized to
        // the longest name shown (clamped, so a long `mcp__…` name cuts).
        let label_w = matches!(open.kind, Kind::Commands).then(|| {
            ferrite_ui::components::code_column_w(
                open.rows
                    .iter()
                    .map(|row| row.name.chars().count())
                    .max()
                    .unwrap_or(0),
            )
        });
        if !open.revealed.replace(true) {
            open.scroll.scroll_to_item(open.selected);
        }
        let mut rows = div()
            .id("composer-menu-rows")
            .debug_selector(|| "composer-menu-rows".into())
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&open.scroll);
        for (at, row) in open.rows.iter().enumerate() {
            let cursor = at == open.selected;
            let explains = row.inert
                && row.consequence_is_inert()
                && row.detail.is_empty()
                && row.matched.is_empty();
            let drawn = match open.kind {
                Kind::Provider | Kind::Band(pane::BandChip::Provider)
                    if row.inert
                        && row.consequence_is_inert()
                        && provider_of_title(&row.name).is_some() =>
                {
                    // A section row carries its own half row of air.
                    wire(
                        pane::picker_section(
                            provider_of_title(&row.name).unwrap(),
                            row.detail.clone(),
                        ),
                        at,
                        cx,
                    )
                }
                _ if explains => wire(ferrite_ui::menu::note(row.name.clone()), at, cx),
                Kind::Commands | Kind::Files { .. } => wire(
                    pane::menu_row(("composer-menu-row", at), &row.row, cursor, label_w),
                    at,
                    cx,
                ),
                Kind::Provider | Kind::Effort | Kind::Band(_) => wire(
                    pane::picker_row(
                        ("composer-menu-row", at),
                        row.name.clone(),
                        row.detail.clone(),
                        cursor,
                        row.active,
                        row.inert,
                    ),
                    at,
                    cx,
                ),
            };
            rows = rows.child(drawn);
        }
        // A press on the popover's own dead space is not a press outside
        // it: swallowed, so the root's dismissal never sees it.
        let popover = pane::menu_popover()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(rows)
            .child(pane::popover_footer(open.kind.hints()));
        ferrite_ui::motion::menu_in("composer-menu-in", popover, ferrite_ui::motion::Opens::Up)
            .into_any_element()
    }

    /// The nav's per-frame state, from caches and O(1) reads only. Nothing
    /// here touches the store, and nothing here is a Pane decision: the
    /// Project filter narrows this list and nothing else.
    ///
    /// The tree (N-2) is one section per Project, in creation order: its
    /// loose rows — open Threads no Group claims as home — then its Groups,
    /// each under the Project of its first member; then `Other`, for
    /// Threads whose Project cannot be read. A Project with nothing to show
    /// draws no heading, and a Thread a second Group includes is listed
    /// under that Group too (R1). Rows keep creation order (N-3) — a Thread
    /// starting or finishing work never moves — unless `sort: recent` asks
    /// for last use within each Project, or `sort: attention` ranks by
    /// state: the loose rows, each Group's members, the Groups in a section
    /// and the sections themselves, each by its roll-up, creation order
    /// among equals. Filtered to a Project, the tree is that Project's
    /// section, holding every Group with a member there.
    ///
    /// The one selection (N-5) is the row of what the board shows: the
    /// Group being viewed (its view or its wall), or the Solo Thread. The
    /// empty board shows neither, so no row is selected.
    fn nav_state(&self) -> nav::NavState {
        let roster = self.cockpit.roster();
        let board_empty = self.empty_board_shown();
        let view = roster.view();
        let solo = match view {
            _ if board_empty => None,
            View::Solo => roster.focused_thread(),
            View::Group(_) => None,
        };
        let project_of = |thread: ThreadId| self.facts.get(thread).and_then(|facts| facts.project);
        let row = |thread: ThreadId| nav::ThreadRow {
            selected: solo == Some(thread),
            ..self.thread_row(thread)
        };
        // The loose rows: open Threads with no home Group, in creation
        // order. Drafts are not rows (#29) — the grid is where a draft
        // lives — and a parked loose Thread waits in the Parked section.
        let homed = self.homed_threads();
        let mut threads: Vec<ThreadId> = self
            .panes
            .iter()
            .filter_map(|pane| pane.thread())
            .filter(|thread| !homed.contains(thread) && self.admitted(*thread))
            .collect();
        threads.sort();
        threads.dedup();
        let mut loose: Vec<nav::ThreadRow> = threads.into_iter().map(row).collect();
        // Stable: Threads used at the same moment, or in the same state,
        // keep creation order.
        let order = self.prefs.settings.thread_list_order;
        match order {
            ThreadListOrder::Created => {}
            ThreadListOrder::Recent => {
                loose.sort_by_key(|row| std::cmp::Reverse(self.last_used(row.thread)))
            }
            ThreadListOrder::Attention => {
                loose.sort_by_key(|row| std::cmp::Reverse(row.status.attention()))
            }
        }
        // Never re-sort under the pointer (C8): while the pointer is inside
        // the column or a drag is live, the rows keep the order they were
        // last drawn in, and the ranks they were sorted by, as long as the
        // same Threads are there and focus has not moved. Anything else
        // re-ranks, and is remembered at the end.
        let focused = roster.focused_thread();
        let frozen = self.nav_hovered || self.nav_drag_live.get();
        let held_ranks = match self.nav_snapshot.borrow().as_ref() {
            Some(held) if frozen && held.focused == focused => {
                hold_order(&mut loose, &held.rows).then(|| held.ranks.clone())
            }
            _ => None,
        };
        let held = held_ranks.is_some();
        let ranks = held_ranks.unwrap_or_default();
        // A row's rank under `sort: attention`, most urgent least: the held
        // one while the tree holds still.
        let rank = |row: &nav::ThreadRow| {
            std::cmp::Reverse(
                ranks
                    .get(&row.thread)
                    .copied()
                    .unwrap_or_else(|| row.status.attention()),
            )
        };
        let attention = order == ThreadListOrder::Attention;

        // The Groups, in the durable order (creation order unless the
        // operator moved one), each under the one Project its members share
        // — or, when they span Projects, in the Groups section, belonging
        // to none of them. Filtered, a Group sits under the filter's Project
        // when any member is there, drawing only those members. A member
        // whose leave is parked on a pending draft has already left, as in
        // `visible_indices`.
        let mut placed: Vec<(Option<Option<ProjectId>>, nav::GroupBlock)> = self
            .cockpit
            .groups()
            .iter()
            .filter_map(|group| {
                let leaving = roster.pending_leave(group.id);
                let members: Vec<ThreadId> = group
                    .members
                    .iter()
                    .copied()
                    .filter(|thread| Some(*thread) != leaving)
                    .collect();
                let mut shown: Vec<nav::ThreadRow> = members
                    .iter()
                    .copied()
                    .filter(|thread| self.admitted(*thread))
                    .map(row)
                    .collect();
                if shown.is_empty() {
                    return None;
                }
                if attention {
                    shown.sort_by_key(rank);
                }
                // `None` is the Groups section; `Some(project)` a Project's.
                let project = match self.nav_filter {
                    Some(filter) => Some(Some(filter)),
                    None => {
                        let mut projects: Vec<ProjectId> = members
                            .iter()
                            .filter_map(|thread| project_of(*thread))
                            .collect();
                        projects.sort();
                        projects.dedup();
                        match projects.as_slice() {
                            [] => Some(None),
                            [only] => Some(Some(*only)),
                            _ => None,
                        }
                    }
                };
                Some((
                    project,
                    nav::GroupBlock {
                        id: group.id,
                        title: group.display_title().into(),
                        members: shown,
                        folded: self.nav_folds.contains(&NavFold::Group(group.id)),
                        selected: !board_empty && view == View::Group(group.id),
                    },
                ))
            })
            .collect();
        if attention {
            placed.sort_by_cached_key(|(_, block)| block.members.iter().map(rank).min());
        }

        // The sections: every Project something is drawn under, in creation
        // order (ids are handed out in registration order), `Other` last.
        // The Groups section, when any Group spans Projects, comes first.
        let mut keys: Vec<Option<ProjectId>> = loose
            .iter()
            .map(|row| project_of(row.thread))
            .chain(placed.iter().filter_map(|(project, _)| *project))
            .collect();
        keys.sort_by_key(|key| (key.is_none(), *key));
        keys.dedup();
        let mut solos: Vec<nav::ThreadRow> = Vec::new();
        let mut groups: Vec<nav::GroupBlock> = Vec::new();
        let mut sections: Vec<nav::ProjectSection> = Vec::new();
        let mixed: Vec<usize> = placed
            .iter()
            .filter(|(project, _)| project.is_none())
            .map(|(_, block)| {
                groups.push(block.clone());
                groups.len() - 1
            })
            .collect();
        if !mixed.is_empty() {
            let rows: Vec<&nav::ThreadRow> = mixed
                .iter()
                .flat_map(|index| groups[*index].members.iter())
                .collect();
            sections.push(nav::ProjectSection {
                project: None,
                mixed: true,
                label: SharedString::from("groups"),
                branch: None,
                drift: None,
                numbers: None,
                folded: self.nav_folds.contains(&NavFold::Groups),
                roll_up: nav::RollUp::of(rows.iter().copied()),
                count: rows.len(),
                solos: Vec::new(),
                groups: mixed,
            });
        }
        for key in keys {
            let mut section_solos = Vec::new();
            for row in loose.iter().filter(|row| project_of(row.thread) == key) {
                section_solos.push(solos.len());
                solos.push(row.clone());
            }
            let mut section_groups = Vec::new();
            for (_, block) in placed.iter().filter(|(project, _)| *project == Some(key)) {
                section_groups.push(groups.len());
                groups.push(block.clone());
            }
            // A section's own Threads, not a Group's guest from another
            // Project: they say its branch and the Space it stands for.
            let own: Vec<ThreadId> = section_solos
                .iter()
                .map(|index| solos[*index].thread)
                .chain(
                    section_groups
                        .iter()
                        .flat_map(|index| groups[*index].members.iter().map(|row| row.thread)),
                )
                .filter(|thread| project_of(*thread) == key)
                .collect();
            // A heading names its rows' shared checkout; `Other` names no
            // Project, so no checkout either. A drift the heading says, the
            // rows leave unsaid.
            let (branch, drift) = match key {
                Some(_) => self.section_checkout(&own),
                None => (None, None),
            };
            if drift.is_some() {
                let own_row = |row: &nav::ThreadRow| own.contains(&row.thread);
                let clear = |row: &mut nav::ThreadRow| {
                    row.details.ahead = 0;
                    row.details.behind = 0;
                };
                for index in &section_solos {
                    clear(&mut solos[*index]);
                }
                for index in &section_groups {
                    groups[*index]
                        .members
                        .iter_mut()
                        .filter(|row| own_row(row))
                        .for_each(clear);
                }
            }
            // A task's own space is named as its row is (ADR 0031), its
            // numbers under the heading.
            let space = key.and_then(|_| self.projects.sidebar.heading(own.iter().copied()));
            let numbers = space
                .map(|space| SharedString::from(space.progress.clone()))
                .filter(|numbers| !numbers.is_empty());
            let rows: Vec<&nav::ThreadRow> = section_solos
                .iter()
                .map(|index| &solos[*index])
                .chain(
                    section_groups
                        .iter()
                        .flat_map(|index| groups[*index].members.iter()),
                )
                .collect();
            let label = self.section_label(
                key,
                space.map(|space| SharedString::from(space.name.clone())),
                rows.iter().copied(),
            );
            let roll_up = nav::RollUp::of(rows.iter().copied());
            let count = rows.len();
            sections.push(nav::ProjectSection {
                project: key,
                mixed: false,
                label,
                branch,
                drift: drift.and_then(|(ahead, behind)| nav::drift_text(ahead, behind)),
                numbers,
                roll_up,
                count,
                folded: key
                    .is_some_and(|project| self.nav_folds.contains(&NavFold::Project(project))),
                solos: section_solos,
                groups: section_groups,
            });
        }

        if attention {
            sections.sort_by_cached_key(|section| {
                section
                    .solos
                    .iter()
                    .map(|index| &solos[*index])
                    .chain(
                        section
                            .groups
                            .iter()
                            .flat_map(|index| groups[*index].members.iter()),
                    )
                    .map(rank)
                    .min()
            });
        }
        if !held {
            let ranks = if attention {
                solos
                    .iter()
                    .chain(groups.iter().flat_map(|group| group.members.iter()))
                    .map(|row| (row.thread, row.status.attention()))
                    .collect()
            } else {
                Default::default()
            };
            *self.nav_snapshot.borrow_mut() = Some(NavSnapshot {
                rows: loose.iter().map(|row| row.thread).collect(),
                focused,
                ranks,
            });
        }

        let parked = self.parked_threads().into_iter().map(row).collect();

        nav::NavState {
            sections,
            groups,
            solos,
            parked,
        }
    }

    /// The nav's Attention list (§5.1.2) from the tree's rows (`state`), so
    /// it lists what the Project filter admits and reads nothing new: every
    /// open Thread that needs the operator, in ⌘D's order (oldest request
    /// first); every one that finished unseen, newest finish first; every
    /// one at work, in creation order. Its folds are the nav's.
    ///
    /// A finish never leaves from under the operator (CONTEXT.md
    /// "Attention list"): the focused Thread is held under `Done` while it
    /// would otherwise leave the list, when it was listed there (opening a
    /// done Thread reads its Notice) or was listed under `Working` and has
    /// a Notice since (it finished while the operator watched, seen at
    /// once). It leaves once focus moves to another. Anything else moves
    /// at once: a Thread whose state takes it to another section, and one
    /// that stopped with no Notice (an interrupt), which leaves.
    pub(crate) fn attention_list(&self, state: &nav::NavState) -> nav::AttentionList {
        use nav::ListSection;
        let roster = self.cockpit.roster();
        let focused = roster.focused_thread();
        // The row stands for what the board shows, as in the tree: the Solo
        // Thread, or the shown Group's focused member.
        let shown = |thread: ThreadId| {
            !self.empty_board_shown()
                && focused == Some(thread)
                && match roster.view() {
                    View::Solo => true,
                    View::Group(group) => self
                        .cockpit
                        .groups()
                        .get(group)
                        .is_some_and(|group| group.members.contains(&thread)),
                }
        };
        let mut rows: Vec<&nav::ThreadRow> = state
            .solos
            .iter()
            .chain(state.groups.iter().flat_map(|group| group.members.iter()))
            .collect();
        rows.sort_by_key(|row| row.thread);
        rows.dedup_by_key(|row| row.thread);

        let notifications = self.cockpit.notifications();
        // The Notices come newest first.
        let newest = |thread: ThreadId| {
            notifications
                .notices()
                .find(|notice| notice.thread == thread)
                .map(|notice| notice.id)
        };
        let mut listed = self.nav_listed.borrow_mut();
        let mut placed: Vec<(ListSection, nav::ThreadRow)> = Vec::new();
        for row in rows {
            let thread = row.thread;
            let held = || {
                let finished = match listed.get(&thread) {
                    Some((ListSection::Done, _)) => true,
                    Some((ListSection::Working, known)) => newest(thread) > *known,
                    _ => false,
                };
                (focused == Some(thread) && finished).then_some(ListSection::Done)
            };
            let Some(section) = ListSection::of(row.status.attention()).or_else(held) else {
                continue;
            };
            let row = nav::ThreadRow {
                selected: shown(thread),
                ..row.clone()
            };
            placed.push((section, row));
        }
        *listed = placed
            .iter()
            .map(|(section, row)| (row.thread, (*section, newest(row.thread))))
            .collect();

        let queue = self.cockpit.needs_you();
        let sections = ListSection::ALL
            .into_iter()
            .filter_map(|section| {
                let mut rows: Vec<nav::ThreadRow> = placed
                    .iter()
                    .filter(|(each, _)| *each == section)
                    .map(|(_, row)| row.clone())
                    .collect();
                // Stable: creation order among equals, and for `Working`.
                match section {
                    ListSection::NeedsYou => rows.sort_by_key(|row| {
                        queue
                            .iter()
                            .position(|thread| *thread == row.thread)
                            .unwrap_or(usize::MAX)
                    }),
                    // The Notices come newest first.
                    ListSection::Done => rows.sort_by_key(|row| {
                        notifications
                            .notices()
                            .position(|notice| notice.thread == row.thread)
                            .unwrap_or(usize::MAX)
                    }),
                    ListSection::Working => {}
                }
                (!rows.is_empty()).then(|| nav::ListBlock {
                    section,
                    rows,
                    folded: self.nav_folds.contains(&NavFold::List(section)),
                })
            })
            .collect();
        nav::AttentionList { sections }
    }

    /// Does the Project filter admit this Thread? `All projects` admits
    /// everything; a chosen Project admits only Threads whose cached
    /// ProjectId is it — so a Thread whose Project is unknown appears under
    /// `All projects` alone, rather than being quietly filed under someone
    /// else's Project.
    fn admitted(&self, thread: ThreadId) -> bool {
        if self.projects.sidebar.owned.contains(&thread) {
            return false;
        }
        let Some(wanted) = self.nav_filter else {
            return true;
        };
        self.facts.get(thread).and_then(|facts| facts.project) == Some(wanted)
    }

    /// One Thread's nav row, entirely from caches: the name core has (there
    /// is no display name), its state, the cached Project and checkout
    /// facts, and its word. Not selected: `nav_state` decides what carries
    /// the selection.
    fn thread_row(&self, thread: ThreadId) -> nav::ThreadRow {
        let facts = self.facts.get(thread);
        let open = self.cockpit.thread(thread);
        let now = ferrite_core::clock::system_time();
        // A Notice not yet seen makes a quiet Thread done.
        let unread = self.cockpit.notifications().attention(thread);
        // The tail reads the Pane's own head slot, so the nav and the Pane
        // never disagree about a Thread (C10).
        let slot =
            open.and_then(|open| pane::thread_word(open, facts.map(|facts| &facts.wall), unread));
        // The mark is squared with that word (`pane::dot_state`): a row whose
        // tail says `failed` never wears an idle dot.
        let status = match open {
            None => nav::RowStatus::Parked,
            Some(open) => {
                let failing = facts.is_some_and(|facts| facts.wall.tests_failing);
                nav::RowStatus::of(
                    pane::dot_state(
                        pane::wall_state(
                            Some(&open.turn()),
                            open.pending_decisions()
                                .iter()
                                .any(|pending| pending.decision.blocks_execution()),
                            failing,
                        ),
                        slot.as_ref(),
                    ),
                    unread,
                    open.background_work(),
                )
            }
        };
        let age = facts
            .and_then(|facts| facts.last_used)
            .map(|at| crate::facts::since_label(at, now))
            .unwrap_or_default();
        // The agent's edits, summed: what this Thread changed (SPEC §8),
        // not a diff against the base.
        let diffstat = facts
            .filter(|facts| !facts.changed_files.is_empty())
            .map(|facts| {
                facts
                    .changed_files
                    .iter()
                    .fold((0, 0), |(added, removed), file| {
                        (added + file.added, removed + file.removed)
                    })
            });
        let mut row = nav::ThreadRow {
            thread,
            name: self.facts.name(thread),
            status,
            project: facts.and_then(|facts| facts.project_label.clone()),
            selected: false,
            branch: facts.and_then(Self::checkout_branch),
            diffstat,
            age: age.clone(),
            details: nav::RowDetails::of(
                facts.and_then(|facts| facts.status.as_ref()),
                facts.and_then(|facts| facts.pr.label()),
            ),
            tail: nav::NavTail::of(status, slot.as_ref(), age),
            task: None,
        };
        // A Thread the board runs is its sidebar entry (ADR 0031): the
        // task's number and title, nothing at the right, its rows under it.
        // Only an open Thread is a live agent: a parked one stays one line.
        let told = open.and(self.projects.sidebar.agents.get(&thread));
        if let Some(told) = told {
            row = nav::ThreadRow {
                name: told.entry.name.clone().into(),
                details: nav::RowDetails::default(),
                tail: nav::NavTail::None,
                task: Some(told.rows()),
                ..row
            };
        }
        #[cfg(test)]
        drawn::record_row(
            thread,
            format!("{:?} {:?} {}", row.status, row.tail, row.name),
        );
        row
    }

    /// The branch a checkout is on: the live status's, else the cached one.
    fn checkout_branch(facts: &crate::facts::ThreadFacts) -> Option<SharedString> {
        facts
            .status
            .as_ref()
            .and_then(|status| status.branch.clone())
            .map(SharedString::from)
            .or_else(|| facts.branch.clone())
    }

    /// What a Project heading says of its rows' checkouts (§5.1.4): the
    /// branch every one of them is on, whether that is the Project's
    /// default, and that branch's drift when every row shares it too. Cache
    /// reads only; the checkouts that say no branch (a parked row may not)
    /// say nothing either way.
    fn section_checkout(&self, rows: &[ThreadId]) -> SectionCheckout {
        let read: Vec<(SharedString, (u32, u32))> = rows
            .iter()
            .filter_map(|thread| self.facts.get(*thread))
            .filter_map(|facts| {
                let drift = facts
                    .status
                    .as_ref()
                    .map_or((0, 0), |status| (status.ahead, status.behind));
                Some((Self::checkout_branch(facts)?, drift))
            })
            .collect();
        let Some((first, drift)) = read.first().cloned() else {
            return (None, None);
        };
        if !read.iter().all(|(branch, _)| *branch == first) {
            return (None, None);
        }
        let default = rows
            .iter()
            .find_map(|thread| {
                self.facts
                    .get(*thread)
                    .and_then(|facts| facts.default_branch.clone())
            })
            .map_or_else(
                || matches!(first.as_ref(), "main" | "master"),
                |default| default == first,
            );
        let shared = read.iter().all(|(_, each)| *each == drift);
        (Some((first, default)), shared.then_some(drift))
    }

    /// Land on a Thread from the nav's other doors — an Attention list or
    /// rail row: the Group on screen when it holds the Thread, else its
    /// home Group's view, else its Pane if open, else revived — exactly as
    /// a press on its own row does.
    fn land_on_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.board_gives_way();
        let target: Option<GroupId> = {
            let groups = self.cockpit.groups();
            let viewed = match self.cockpit.roster().view() {
                View::Group(group)
                    if groups
                        .get(group)
                        .is_some_and(|group| group.members.contains(&thread)) =>
                {
                    Some(group)
                }
                _ => None,
            };
            viewed.or_else(|| groups.home(thread).map(|group| group.id))
        };
        if let Some(group) = target {
            self.enter_group(group, cx);
            self.focus_thread(thread, cx);
        } else if self.pane_for(thread).is_some() {
            self.focus_thread(thread, cx);
        } else {
            self.revive_thread(thread, cx);
        }
    }

    /// ⌘1…⌘9 (R5): focus the shown board's `ordinal`th Pane in head order
    /// (`board_order`); Solo's ⌘1 is the Solo Pane. A digit past the board's
    /// Panes does nothing.
    fn focus_rail(&mut self, ordinal: usize, window: &Window, cx: &mut Context<Self>) {
        let index = ordinal
            .checked_sub(1)
            .and_then(|at| self.board_order(window).get(at).copied());
        if let Some(index) = index {
            self.focus_pane(index);
            cx.notify();
        }
    }

    /// When a Thread was last used, for `sort: recent`. A Thread whose log
    /// cannot be stat'd sorts to the bottom rather than to the top: an
    /// unknown time is not a recent one.
    fn last_used(&self, thread: ThreadId) -> std::time::SystemTime {
        self.facts
            .last_used(thread)
            .unwrap_or(std::time::UNIX_EPOCH)
    }

    /// A running nav row's click: land on that Thread's Pane, in the view
    /// that shows it — the core's one door, so a fullscreened cockpit
    /// re-aims to the clicked Thread like every other deliberate move.
    fn focus_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.empty_shown = false;
        if self.cockpit.focus_thread(thread) {
            self.board_gives_way();
            if let Some(index) = self.pane_for(thread) {
                self.focus_pane(index);
            }
            cx.notify();
        }
    }

    /// Open a Group (#28): the core lays every member's Pane — a parked
    /// one's as its tile, woken by ⏎ (F-8) — and enters the view; the
    /// mirror follows.
    pub fn enter_group(&mut self, group: GroupId, cx: &mut Context<Self>) {
        self.empty_shown = false;
        match self.cockpit.enter_group(group) {
            Ok(()) => {
                self.sync_panes(cx);
            }
            Err(error) => {
                self.group_error = Some(error.to_string().into());
            }
        }
        cx.notify();
    }

    /// A nav drop: the core plans and applies it in the View the drag
    /// started from; a refusal lands in the nav's banner.
    fn apply_drop(&mut self, drag: NavDrag, target: DropTarget, cx: &mut Context<Self>) {
        self.nav_dragging = None;
        match self.cockpit.drop(drag.drag, drag.origin, target) {
            Ok(()) => self.group_error = None,
            Err(error) => {
                ferrite_core::log!("group change refused: {error}");
                self.group_error = Some(error.to_string().into());
            }
        }
        self.sync_panes(cx);
        cx.notify();
    }

    /// Revive one parked Thread into the view that shows it — the shared
    /// tail of cmd-o and a parked nav row's click (#21).
    fn revive_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.empty_shown = false;
        match self.cockpit.reopen(thread) {
            Ok(()) => {
                self.sync_panes(cx);
                cx.notify();
            }
            Err(e) => ferrite_core::log!("thread {thread} could not be reopened: {e:?}"),
        }
    }

    /// Focus by grid index — the pointer's and the tests' spelling of the
    /// core's one door. Every move to a Pane takes a tab's work board out
    /// of the way, as herdr's board tab gives way to the tab you pick.
    fn focus_pane(&mut self, index: usize) {
        if let Some(identity) = self.panes.get(index).map(|pane| pane.identity) {
            if self.projects.coordinator_thread().map(PaneIdentity::Thread) != Some(identity) {
                self.board_gives_way();
            }
            self.cockpit.focus(identity);
        }
    }

    /// cmd-shift-n: the same draft, aimed straight at "new worktree" —
    /// isolation from the operator's checkout is one chip already chosen.
    fn new_worktree_thread(
        &mut self,
        _: &NewWorktreeThread,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_draft_in_current_view(DraftTarget::New, cx);
    }

    /// cmd-t / cmd-n (#29): a draft Pane, not a Thread. The band chooses;
    /// the first send bootstraps.
    fn new_thread(&mut self, _: &NewThread, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_draft_in_current_view(DraftTarget::Main, cx);
    }

    /// cmd-= / cmd-- step every transcript's text size through
    /// `ReadingSize::STEPS` (12–24px), stopping at either end, and cmd-0
    /// returns it to 14: the same saved setting as Settings › Behaviour ›
    /// Text size. `delta` 0 resets.
    fn step_text_size(&mut self, delta: i32, cx: &mut Context<Self>) {
        use ferrite_core::settings::ReadingSize;
        let now = self.prefs.settings.reading_size;
        let next = if delta == 0 {
            ReadingSize::STANDARD
        } else {
            now.step(delta)
        };
        if next != now {
            self.change_settings(|settings| settings.reading_size = next, cx);
            cx.notify();
        }
    }

    /// cmd-g: the titlebar's New Group — a draft that founds a Group with
    /// the focused Thread on its first send. Only a Thread in no Group can
    /// found one, so elsewhere the key does nothing, as the button is not
    /// offered there either.
    fn new_group(&mut self, _: &NewGroup, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self
            .focused_thread()
            .filter(|thread| self.cockpit.groups().of(*thread).is_none())
        else {
            return;
        };
        self.open_draft_with_placement(DraftTarget::Main, DraftPlacement::NewGroupWith(thread), cx);
    }

    /// Reopen the Thread parked most recently — the one the operator just
    /// closed, which is the one they want back (#17); the core walks the
    /// park order, then creation order.
    fn reopen_thread(&mut self, _: &ReopenThread, _window: &mut Window, cx: &mut Context<Self>) {
        match self.cockpit.reopen_last() {
            Some((_, Ok(()))) => {
                self.sync_panes(cx);
                cx.notify();
            }
            Some((thread, Err(e))) => {
                ferrite_core::log!("thread {thread} could not be reopened: {e:?}")
            }
            None => {}
        }
    }

    /// Close the focused Pane — cmd-w. The core decides what that means
    /// (park, leave, defer onto a draft, discard); the mirror follows.
    fn close_thread(&mut self, _: &CloseThread, window: &mut Window, cx: &mut Context<Self>) {
        // ⌘W in the focused Pane's preview or Review closes that, not the
        // Pane (F-14).
        let index = self.focused();
        if let Some(pane) = self.panes.get(index) {
            if self.preview_shown(index) && pane.preview.focused(window, cx) {
                self.close_preview(index, cx);
                return;
            }
            if pane.review.focused(window, cx) {
                self.close_review(index, cx);
                return;
            }
        }
        let Some(identity) = self.cockpit.roster().focused() else {
            return;
        };
        self.close_pane(identity, cx);
    }

    /// Close one Pane through the core act (a Thread always parks, leaving
    /// its Group first), its refusal landing where every other Group error
    /// does — and a park that would not flush is logged, the Pane gone
    /// either way.
    fn close_pane(&mut self, identity: PaneIdentity, cx: &mut Context<Self>) {
        // Whether this close is a Group change at all: a Thread leaving
        // the Group on screen, unless a pair defers the leave onto a
        // pending draft; or a draft discarded with a leave to apply.
        let roster = self.cockpit.roster();
        let changes_group = match (roster.view(), identity) {
            (View::Group(group), PaneIdentity::Thread(_)) => {
                let members = self
                    .cockpit
                    .groups()
                    .get(group)
                    .map_or(0, |group| group.members.len());
                !(members == 2 && roster.pending_draft(group).is_some())
            }
            (_, PaneIdentity::Draft(draft)) => roster
                .draft_scope(draft)
                .is_some_and(|scope| scope.pending_leave.is_some()),
            (View::Solo, PaneIdentity::Thread(_)) => false,
        };
        match self.cockpit.close(identity) {
            Ok(()) => {
                if changes_group {
                    self.group_error = None;
                }
            }
            Err(CloseError::Park(e)) => {
                ferrite_core::log!("pane {identity:?} did not park cleanly: {e}")
            }
            Err(CloseError::Group(error)) => {
                ferrite_core::log!("group change refused: {error}");
                self.group_error = Some(error.to_string().into());
            }
        }
        // A popover on the closed Pane goes with it.
        if self
            .popover
            .as_ref()
            .is_some_and(|open| open.pane == identity)
        {
            self.popover = None;
        }
        self.sync_panes(cx);
        cx.notify();
    }

    /// Jump to the next Thread waiting on the operator — the whole point of
    /// a Group you cannot read all of at once.
    fn next_decision(&mut self, _: &NextDecision, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.focused();
        let mut requests = Vec::new();
        // The answer order (`Cockpit::needs_you`): ⌘D walks the answer
        // queue top to bottom, so its first press lands on the answer target.
        let queue = self.cockpit.needs_you();
        let mut panes: Vec<&CockpitPane> = self.panes.iter().collect();
        panes.sort_by_key(|pane| {
            pane.thread()
                .and_then(|thread| queue.iter().position(|queued| *queued == thread))
                .unwrap_or(usize::MAX)
        });
        for pane in panes {
            if let Some(thread) = pane.thread() {
                if let Some(open) = self.cockpit.thread(thread) {
                    for request in open.pending_decisions() {
                        requests.push((
                            thread,
                            request
                                .subject
                                .clone()
                                .unwrap_or(ferrite_core::activity::Subject::Main),
                        ));
                    }
                }
            }
        }
        let current = self
            .panes
            .get(focused)
            .and_then(|pane| pane.thread().map(|thread| (thread, pane.selected.clone())));
        if let Some((thread, subject)) = subagents::next_request(requests, current.as_ref()) {
            self.select_subject(thread, subject, window, cx);
        }
    }

    /// A left press lands the operator on this Pane. It only moves focus —
    /// through `focus_pane`, the door #21's nav clicks will share, so a
    /// fullscreen re-aims here too: the per-frame snap in render then
    /// carries focus to whatever the Pane holds (Composer or Decision card)
    /// — fighting the snap would regress the dead-keyboard fixes it exists
    /// for. A press inside the transcript body also anchors a selection at
    /// the character under the pointer, at the grain the click count names
    /// (#27) — one on the Pane's chrome (header, Composer) anchors nothing;
    /// the standing selection was already cleared by the root's capture
    /// handler, and the root's bubble handler dismisses any open selector —
    /// this Pane included.
    fn pointer_down(
        &mut self,
        index: usize,
        _event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self.panes.get_mut(index) {
            pane.clear_tool_target();
        }
        self.focus_pane(index);
        cx.notify();
    }

    /// Copy the active native selection. Its retained document keeps logical
    /// transcript fragments available even while their rows are offscreen.
    fn copy_selection(&mut self, _: &CopySelection, window: &mut Window, cx: &mut Context<Self>) {
        let text = gpui::base::TextSelection::selected_text(window, cx);
        let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// ⌘V anywhere in the window lands in the focused Pane's Composer.
    /// The Composer's own Paste runs first while it holds the keyboard;
    /// this is the fallback for when a click in the transcript — a drag
    /// to select, a tool row — took the keyboard away: pasted text or files
    /// go where the operator is about to type, and the keyboard follows.
    fn paste_into_composer(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        // ⌘V in the palette pastes into its line.
        if let Some(palette) = self.floats.palette.clone() {
            if let Some(item) = cx.read_from_clipboard() {
                let input = palette.read(cx).input.clone();
                input.update(cx, |line, cx| line.paste_item(item, cx));
            }
            return;
        }
        if !self.panes[self.focused()].is_main() {
            return;
        }
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let index = self.focused();
        let Some(pane) = self.panes.get(index) else {
            return;
        };
        if self.rename.is_some() {
            return;
        }
        let composer = pane.composer.clone();
        composer.update(cx, |composer, cx| composer.paste_item(item, cx));
        window.focus(&composer.focus_handle(cx), cx);
        cx.notify();
    }
}

/// What a typed `/effort` or `/model` line asked for.
enum Tuning {
    Effort(Option<String>),
    Model(Option<String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
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

/// The slash and filter immediately before the caret, anywhere in a draft.
/// Whitespace ends the filter; a letter must directly follow the slash.
/// A lone leading slash keeps the existing full-command discovery shortcut.
fn slash_token(text: &str, cursor: usize) -> Option<(usize, &str)> {
    let head = text.get(..cursor)?;
    let start = head.rfind('/')?;
    let filter = &head[start + 1..];
    if filter.contains(char::is_whitespace) {
        return None;
    }
    let valid = filter
        .chars()
        .next()
        .map_or(start == 0, char::is_alphabetic);
    valid.then_some((start, filter))
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
                    inert: false,
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
            // A directory ends in `/`: its name is the last segment with
            // that slash, its directory everything before.
            let stem = file.strip_suffix('/').unwrap_or(file);
            let split = stem.rfind('/').map(|at| at + 1).unwrap_or(0);
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
                    inert: false,
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

/// Preserve provider order and matches, while keeping repository metadata
/// and repeated paths from crowding working files out of the bounded menu.
fn native_mention_rows(files: &[ferrite_core::providers::FileSuggestion]) -> Vec<pane::MenuRow> {
    let mut seen = std::collections::HashSet::new();
    files
        .iter()
        .filter(|file| {
            !std::path::Path::new(&file.path)
                .components()
                .any(|part| part.as_os_str() == ".git")
                && seen.insert(file.path.trim_end_matches('/'))
        })
        .take(MENU_ROWS_MAX)
        .map(|file| {
            let mut path = file.path.clone();
            if file.is_directory && !path.ends_with('/') {
                path.push('/');
            }
            let stem = path.strip_suffix('/').unwrap_or(&path);
            let split = stem.rfind('/').map(|at| at + 1).unwrap_or(0);
            let chars: Vec<_> = path.char_indices().collect();
            let matched = file
                .matched
                .iter()
                .filter_map(|index| {
                    let (start, character) = *chars.get(*index)?;
                    (start >= split).then_some(
                        start.saturating_sub(split)
                            ..start.saturating_sub(split) + character.len_utf8(),
                    )
                })
                .collect();
            pane::MenuRow {
                insert: path.clone().into(),
                name: path[split..].to_string().into(),
                matched,
                detail: if split == 0 {
                    "".into()
                } else {
                    path[..split - 1].to_string().into()
                },
                prose_detail: false,
                inert: false,
            }
        })
        .collect()
}

/// How many sessions the import picker lists (#11), newest first: enough
/// history to search, few enough that the listing reads a bounded slice
/// of each file rather than every session ever run.
const IMPORT_ROWS_MAX: usize = 200;

/// One Ferrite-local `/` row (#11's `import`, #25's `provider`) — never
/// the provider's, which its description says out loud. The same fuzzy
/// filter as every row; highlights shifted past the drawn `/`.
fn local_row(
    filter: &str,
    name: &'static str,
    detail: &'static str,
    inert: bool,
) -> Option<pane::MenuRow> {
    let (_, matched) = crate::fuzzy::matches(filter, name)?;
    Some(pane::MenuRow {
        insert: SharedString::from(name),
        name: SharedString::from(format!("/{name}")),
        matched: matched
            .into_iter()
            .map(|range| range.start + 1..range.end + 1)
            .collect(),
        detail: SharedString::from(detail),
        prose_detail: true,
        inert,
    })
}

/// One band row: a label and its detail beside the choice it makes.
fn band_row(label: SharedString, detail: SharedString, active: bool, choice: BandChoice) -> Row {
    Row {
        row: pane::MenuRow {
            insert: SharedString::default(),
            name: label,
            matched: Vec::new(),
            detail,
            prose_detail: false,
            inert: false,
        },
        active,
        consequence: Consequence::Band(choice),
    }
}

/// The provider's lowercase name — the store's own serialized spelling.
fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    }
}

/// The Provider's name as a person says it — the picker's section titles
/// and the chip before any model is known.
fn provider_title(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "Claude",
        Provider::Codex => "Codex",
    }
}

/// A Settings group for one Provider: its name, led by its logomark in
/// brand colour.
fn provider_group(provider: Provider) -> prefs::Group {
    let (mark, ink) = match provider {
        Provider::Claude => (
            ferrite_ui::icons::CLAUDE,
            ferrite_ui::theme::PROVIDER_CLAUDE,
        ),
        Provider::Codex => (ferrite_ui::icons::CODEX, ferrite_ui::theme::PROVIDER_CODEX),
    };
    prefs::Group::new(Some(provider_title(provider))).mark(mark, ink)
}

/// An effort level as a person says it: `xhigh` is "Extra high", the
/// rest capitalized.
fn effort_title(effort: &str) -> String {
    match effort {
        "xhigh" => "Extra high".to_string(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

/// What each rung buys, in the words the providers' own menus use.
/// An effort level as its Composer chip reads it: the title, lowercase
/// (`high`, `extra high`) — the chip is a quiet control, the menu rows keep
/// their titles.
fn effort_chip_label(effort: &str) -> SharedString {
    SharedString::from(effort_title(effort).to_lowercase())
}

/// The key a draft's band chip records its bounds under, for the menu it
/// opens (`band_menu_element`).
fn band_trigger(chip: pane::BandChip) -> SharedString {
    format!("band-{chip:?}").into()
}

/// The effort a chip shows (rule 2.6.5): the Thread's or draft's own
/// choice, else the operator's saved default, else the model row's own
/// default (`medium` for Codex). `None` when nothing resolves: a Thread's
/// chip then hides rather than reading the bare word `effort` (`/effort`
/// still reaches the picker), and a draft's band chip reads `default`.
fn effort_value(
    chosen: Option<&str>,
    saved: Option<&str>,
    provider: Provider,
    model: Option<&str>,
    announced: &[ferrite_core::ModelInfo],
) -> Option<SharedString> {
    if let Some(effort) = chosen.or(saved) {
        return Some(effort_chip_label(effort));
    }
    let rows = ferrite_core::providers::models::catalog(provider, announced);
    let row = match model {
        Some(model) => rows.iter().find(|row| row.is(model)),
        None => rows.first(),
    };
    row.and_then(|row| row.default_effort.as_deref())
        .map(effort_chip_label)
}

fn effort_detail(effort: &str) -> &'static str {
    match effort {
        "minimal" => "the least reasoning the model allows",
        "low" => "fast responses, lighter reasoning",
        "medium" => "balances speed and depth for everyday tasks",
        "high" => "greater depth for complex problems",
        "xhigh" => "extra depth for the hardest problems",
        "max" => "maximum depth, slowest",
        "ultra" => "maximum reasoning with automatic delegation",
        _ => "",
    }
}

fn provider_of_title(title: &str) -> Option<Provider> {
    match title {
        "Claude" => Some(Provider::Claude),
        "Codex" => Some(Provider::Codex),
        _ => None,
    }
}

impl Render for CockpitView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ferrite_ui::motion::hover_frame_start(cx);
        self.tend_reader_slides(cx);
        // Seen is on screen while the window has focus: the Cockpit follows
        // the window's activation from its first frame — a window that opens
        // unfocused sees nothing until it is focused — and the window coming
        // back reads what it shows, which every part then draws.
        if self.activation.is_none() {
            self.cockpit.set_window_active(window.is_window_active());
            self.activation = Some(cx.observe_window_activation(window, |view, window, cx| {
                if view.cockpit.set_window_active(window.is_window_active()) {
                    cx.notify();
                }
            }));
        }
        let root = self.render_cockpit(window, cx);
        // The motion tail: the cockpit is the window's root view, so this
        // runs once per frame, after every hover blend has been read. A
        // hover blend, a column's width (the nav's, the panel's) or a
        // reader's slide mid-flight keeps frames coming; with none, nothing
        // is scheduled (the pulse clock drives loops).
        // Each reads what this frame drew, never the clock again: a ride
        // that ended between the draw and here still asks for the one frame
        // that lands it at rest. A scene's held ride asks for none.
        let column_moving = self.nav_ride.get().is_some() && self.nav_hold.is_none();
        let sliding = self.reader_slides.moving() || self.panel_riding();
        let fading = ferrite_ui::motion::hover_fades_active();
        if column_moving {
            // The nav's width moves the whole board, and the nav with it:
            // every part redraws.
            window.request_animation_frame();
        } else if fading || sliding {
            // A blend is drawn again by the view that paints it alone: a
            // part (a nav row, a Pane's control, a transcript's link), or,
            // for the frame's own controls, the frame around the parts
            // (`motion::frames_through`). A Review's slide and the Panel's
            // ride redraw the frame, which lays the board and the Panel
            // out: only the parts whose cells move are built again (their
            // cache holds their bounds), the rest — the nav — replay.
            let mut views = ferrite_ui::motion::hover_fading_views();
            let frame = ferrite_ui::motion::frames_through(cx.entity_id());
            if sliding && !views.contains(&frame) {
                views.push(frame);
            }
            if views.is_empty() {
                window.request_animation_frame();
            } else {
                window.on_next_frame(move |_, cx| {
                    for view in &views {
                        cx.notify(*view);
                    }
                });
            }
        }
        self.ride_clock_text(cx);
        root
    }
}

impl CockpitView {
    /// The Cockpit's words that change with the clock — a working Thread's
    /// `12s` (its working line, head and tile), a nav row's `2m` — ride the
    /// pulse clock (`motion::ride`) to their next turn: while a loop runs
    /// they change on its grid, as its fixed rate redrew them; otherwise the
    /// sweep brings them up to date.
    fn ride_clock_text(&mut self, cx: &mut Context<Self>) {
        let wall = ferrite_core::clock::system_time();
        // Every age the Cockpit can show: a Thread's (the nav, a parked
        // tile, the palette), a notice's and a waiting request's (the
        // Needs-you view, its toasts).
        let notifications = self.cockpit.notifications();
        let notices = notifications
            .notices()
            .map(|notice| notice.at)
            .chain(notifications.decisions().map(|request| request.at))
            .filter_map(|at| crate::facts::since_label_changes_in(at, wall))
            .min();
        let thread_ages = self.facts.next_age_change(wall);
        // The bottom bar's clock turns over on the minute.
        let minute = crate::facts::next_minute(wall);
        // The sweep redraws a window nothing else draws when an age turns:
        // a Thread's everywhere, the clock's and a notice's in the frame.
        self.ages_turn_at = thread_ages.map(|next| wall + next);
        self.frame_turn_at = Some(wall + notices.map_or(minute, |next| next.min(minute)));
        if ferrite_ui::motion::reduced_motion(cx) {
            return;
        }
        let second = Duration::from_secs(1);
        let rollover = |turn: Option<pane::Turn>| {
            turn.and_then(|turn| turn.elapsed())
                .map(|elapsed| ferrite_core::cadence::next_rollover(elapsed, second))
        };
        let now = cx.background_executor().now();
        let parts = self.parts.borrow().on;
        let mut nav = thread_ages;
        let frame = notices.map_or(minute, |next| next.min(minute));
        let mut everything = nav.into_iter().chain(Some(frame)).min();
        // Each Pane's working clocks: its own part rides them.
        for pane in &self.panes {
            let Some(open) = pane.thread().and_then(|thread| self.cockpit.thread(thread)) else {
                continue;
            };
            let Some(next) = [
                rollover(Some(open.turn())),
                rollover(open.subject_turn(&pane.selected)),
            ]
            .into_iter()
            .flatten()
            .min() else {
                continue;
            };
            everything = Some(everything.map_or(next, |at| at.min(next)));
            if parts {
                let view = self
                    .parts
                    .borrow()
                    .view_id(parts::Part::Pane(pane.identity));
                if let Some(view) = view {
                    ferrite_ui::motion::ride(view, now + next, cx);
                }
            }
        }
        // Every working Thread's nav row reads its clock.
        for thread in self.cockpit.threads() {
            if let Some(next) = rollover(self.cockpit.thread(thread).map(|open| open.turn())) {
                nav = Some(nav.map_or(next, |at| at.min(next)));
                everything = Some(everything.map_or(next, |at| at.min(next)));
            }
        }
        if !parts {
            if let Some(next) = everything {
                ferrite_ui::motion::ride(cx.entity_id(), now + next, cx);
            }
            return;
        }
        let nav_view = self.parts.borrow().view_id(parts::Part::Nav);
        if let (Some(view), Some(next)) = (nav_view, nav) {
            ferrite_ui::motion::ride(view, now + next, cx);
        }
        ferrite_ui::motion::ride(self.frame_tick_id(), now + frame, cx);
    }

    fn render_cockpit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The loops this render lays out go to the loops overlay afresh.
        ferrite_ui::loops_overlay::begin(window, cx.entity_id());
        // Read, so its notify redraws the frame around the parts alone.
        let _ = self.frame_tick.read(cx);
        self.prune_parts();
        self.measure();
        #[cfg(test)]
        RENDERS.with(|renders| renders.set(renders.get() + 1));
        // The board rides the sidebar's width every frame of a cmd-B fold
        // (F-12), and the panel's as it comes and goes; a Solo pair lasts
        // while focus stays on one of its Panes.
        self.nav_ride.set(self.nav_ride_now(cx));
        self.ride_panel(window, cx);
        if self.solo_beside.is_some() && self.solo_pair().is_none() {
            self.solo_beside = None;
        }
        self.update_auto_rail(window);
        self.nav_drag_live.set(cx.has_active_drag());
        self.present_cli_updates(window, cx);
        self.present_ferrite_update(window, cx);
        self.present_server(window, cx);
        self.present_restart(window, cx);
        self.present_unreadable_settings(window, cx);
        self.present_user_commands(window, cx);
        self.present_allowance_warnings(window, cx);
        self.present_asked(window, cx);
        self.present_projects_toasts(window, cx);
        self.maximized = window.is_maximized();
        // The fullscreened Pane, if the roster still shows it: a Pane gone
        // by any path is the roster's to notice, and it falls back to the
        // grid — never a blank cockpit.
        let fullscreen = self
            .cockpit
            .roster()
            .fullscreen()
            .and_then(|shown| self.index_of(shown));
        let level = self.level_now(window);
        // What this frame draws at Transcript level is what the window
        // streams whole; everything else reads its digest (ADR 0032).
        let drawn: Vec<ThreadId> = match (level, fullscreen) {
            (Level::Transcript, Some(index)) => self.panes[index].thread().into_iter().collect(),
            (Level::Transcript, None) => self
                .visible_indices()
                .into_iter()
                .filter_map(|index| self.panes[index].thread())
                .collect(),
            _ => Vec::new(),
        };
        self.cockpit.draws_transcripts(drawn);

        if level == Level::Transcript {
            for index in 0..self.panes.len() {
                let target = self.panes[index].targeted_tool().cloned();
                let target_is_rendered = target
                    .as_ref()
                    .is_none_or(|target| self.expandable_tools(index, cx).contains(target));
                if !target_is_rendered {
                    self.panes[index].clear_tool_target();
                }
            }
        } else {
            // L2/L3 never walk Blocks: only their invisible keyboard target
            // clears. Expanded call ids survive the zoom round-trip.
            for pane in &mut self.panes {
                pane.clear_tool_target();
            }
        }

        // This is intentionally cheap on ordinary frames: `sync_transcript`
        // compares scalar keys first, so Composer edits repaint Cockpit chrome
        // without cloning or notifying any retained transcript entity.
        self.sync_visible_transcripts(cx);

        // The card belongs to the mark that opened it: leaving that Pane,
        // zooming below L1, or the refresh that drops the PR out of the
        // header takes the card with it. A card outliving its mark would be
        // claiming CI for a Pane that is no longer saying anything about it.
        if self.context_checks.is_some_and(|thread| {
            level != Level::Transcript
                || self.focused_thread() != Some(thread)
                || self.settings_open
                || self.project_editor.is_some()
                || self.facts.pr(thread).is_none_or(|pr| pr.checks.is_none())
        }) {
            self.context_checks = None;
        }

        if self.session_controls.is_some_and(|(thread, generation)| {
            level != Level::Transcript
                || self.settings_open
                || self.focused_thread() != Some(thread)
                || self
                    .cockpit
                    .thread(thread)
                    .is_none_or(|open| open.generation() != generation)
        }) {
            self.session_controls = None;
        }

        if self.mode_picker.is_some_and(|(thread, generation)| {
            level != Level::Transcript
                || self.settings_open
                || self.focused_thread() != Some(thread)
                || self
                    .cockpit
                    .thread(thread)
                    .is_none_or(|open| open.generation() != generation)
        }) {
            self.mode_picker = None;
        }

        // The popover belongs to the focused Pane's Composer at L1 (#23):
        // leaving that Pane, or zooming below L1, closes it here — and a
        // picker closes the moment its offer expires (#25): a band chip
        // (#29) goes with the draft becoming a Thread. Render is the one
        // chokepoint every open, pick, dismissal and heal passes.
        if self.popover.as_ref().is_some_and(|open| {
            let focused = self.panes.get(self.focused());
            level != Level::Transcript
                || focused.is_none_or(|pane| pane.identity != open.pane)
                || match &open.kind {
                    // The model picker outlives the first prompt (the
                    // model can always change); only its Thread going
                    // away closes it.
                    Kind::Provider | Kind::Effort => open
                        .pane
                        .thread()
                        .is_none_or(|thread| self.cockpit.thread(thread).is_none()),
                    Kind::Band(_) => focused.is_some_and(|pane| pane.draft().is_none()),
                    Kind::Commands | Kind::Files { .. } => false,
                }
        }) {
            self.popover = None;
        }
        // The open popover widens its Composer's own key context to
        // ComposerMenu: the focused node, where enter and escape can win
        // their tie against Submit and Interrupt.
        let pane_rects = self.pane_rects(window);
        let focused_index = self.focused();
        let float_open = self.overlay_open();
        for (index, pane) in self.panes.iter().enumerate() {
            let row_limit = pane_rects
                .iter()
                .find(|(at, _)| *at == index)
                .map(|(_, rect)| {
                    let queued = pane
                        .thread()
                        .and_then(|thread| self.cockpit.thread(thread))
                        .map_or(0, |thread| thread.queued_all().len());
                    pane::composer_row_limit(rect.h, self.grid_board(), queued)
                })
                .unwrap_or(ferrite_pane::composer::MAX_ROWS);
            // Derive this every frame so answering, leaving fullscreen, or
            // changing Subjects restores the full draft viewport automatically.
            let answering_expanded_question = fullscreen == Some(index)
                && pane
                    .thread()
                    .and_then(|thread| self.cockpit.thread(thread))
                    .is_some_and(|open| {
                        open.pending_decisions().iter().any(|request| {
                            (request.subject.as_ref() == Some(&pane.selected)
                                || (request.subject.is_none() && pane.is_main()))
                                && pane::question_of(&request.decision).is_some()
                        })
                    });
            pane.composer.update(cx, |composer, cx| {
                composer.set_visible_row_limit(
                    if answering_expanded_question {
                        row_limit.min(2)
                    } else {
                        row_limit
                    },
                    cx,
                )
            });
            let open = self
                .popover
                .as_ref()
                .is_some_and(|open| open.pane == pane.identity);
            pane.composer
                .update(cx, |composer, cx| composer.set_menu_open(open, cx));
            // Solo's caret and the focused board Pane's are always the block
            // (FL-12), blinking under an open float; an unfocused board
            // Pane's is the hollow box.
            let lit = index == focused_index;
            pane.composer.update(cx, |composer, cx| {
                composer.set_caret_lit(lit, float_open, cx)
            });
        }
        let history_available: Vec<bool> = (0..self.panes.len())
            .map(|index| self.history_available(index, level))
            .collect();
        for (pane, available) in self.panes.iter().zip(history_available) {
            pane.composer.update(cx, |composer, cx| {
                composer.set_history_available(available, cx)
            });
        }

        // Focus follows the operator, but only onto something this level
        // actually renders: focusing a Composer a wall cell never drew leaves
        // the keyboard pointing at nothing, and every global key stops working.
        // An empty cockpit still needs the keyboard: with nothing focused,
        // dispatch starts above these handlers and cmd-n could never make the
        // first Thread. Fullscreen changes none of this: the fullscreened
        // Pane is the focused Pane (fullscreen follows focus), so the snap
        // lands on the one Pane actually on screen. A machine whose Server
        // went is read-only (ADR 0026): the root holds the keyboard, so no
        // Composer or control on its board takes a key until it is back.
        let lost = self.machine_lost();
        let coordinator_waiting = self.projects.coordinator_shown()
            && self
                .projects
                .coordinator_thread()
                .and_then(|thread| self.pane_for(thread))
                .is_none();
        let wanted = self
            .rename
            .as_ref()
            .map(|(_, editor)| editor.focus_handle(cx))
            // An open Decision note (⇥) holds the keyboard until it closes.
            .or_else(|| self.decision_note_focus(cx))
            // A review beside the focused Pane, or an image in its preview
            // while the Panel shows it, keeps the keyboard it took, from its
            // first frame (a review's comment field's too).
            .or_else(|| {
                let index = self.focused();
                let pane = self.panes.get(index)?;
                pane.review.held_focus(window, cx).or_else(|| {
                    self.preview_shown(index)
                        .then(|| pane.preview.held_focus(window, cx))
                        .flatten()
                })
            })
            .or_else(|| {
                self.panes.get(self.focused()).and_then(|pane| match level {
                    // A parked member's tile draws no Composer: the root holds
                    // the keyboard, where ⏎ wakes it (F-8).
                    _ if pane
                        .thread()
                        .is_some_and(|thread| self.cockpit.thread(thread).is_none()) =>
                    {
                        None
                    }
                    // At L1 the Composer keeps the keyboard even while a
                    // Decision pends: the card is part of its stack and the
                    // input stays live (PromptBox state 04) — y/n/a answer
                    // through the region's own Decision key context (#23).
                    Level::Transcript if pane.has_tool_target() => Some(pane.tool_focus()),
                    // A subagent's tab keeps the keyboard on its body.
                    Level::Transcript if !pane.is_main() => Some(pane.transcript_focus.clone()),
                    Level::Transcript => Some(pane.composer.read(cx).focus_target(window, cx)),
                    _ if pane.thread().is_some_and(|thread| {
                        self.cockpit
                            .thread(thread)
                            .and_then(|open| open.pending())
                            .is_some()
                    }) && level != Level::Wall =>
                    {
                        Some(pane.decision_focus.clone())
                    }
                    _ => None,
                })
            })
            .unwrap_or_else(|| self.focus.clone());
        // The empty board's list holds the keyboard while it shows (F-13):
        // its rows answer ↑ ↓ ⏎ under the `EmptyBoard` context.
        let wanted = if lost || coordinator_waiting {
            self.focus.clone()
        } else if self.rename.is_none() && fullscreen.is_none() && self.empty_board_shown() {
            self.empty_focus.clone()
        } else {
            wanted
        };
        use gpui::component::WindowExt as _;
        let native_text_focused = self
            .panes
            .get(self.focused())
            .is_some_and(|pane| pane.transcript_focus.contains_focused(window, cx))
            && (ferrite_pane::rich::code_actions_focused(window)
                || gpui::base::TextSelection::has_selection(window, cx)
                || self.panes.get(self.focused()).is_some_and(|pane| {
                    pane.rich.output_focused(&pane.text_namespace(), window, cx)
                }));
        let pane_control_focused = !lost
            && self.panes.get(self.focused()).is_some_and(|pane| {
                let controls = pane
                    .thread()
                    .and_then(|id| self.cockpit.thread(id))
                    .is_some_and(|thread| {
                        !thread.activity().children().is_empty()
                            || thread.pending_decisions().len() > 1
                            || thread.pending_decisions().iter().any(|request| {
                                request.subject.is_none()
                                    || pane::question_of(&request.decision).is_some()
                            })
                    });
                pane.agent_menu_open
                    || (controls
                        && pane.controls_focus.contains_focused(window, cx)
                        && !pane.controls_focus.is_focused(window)
                        && (!pane.is_main() || !pane.transcript_focus.is_focused(window)))
            });
        // The focused Pane's preview and Review sit outside its transcript,
        // so their text needs its own exemption or a click to select in it
        // is undone. The focused Pane's only: focus moving to another Pane
        // takes the keyboard with it, and the image viewer's own focus is
        // no text (`Preview::text_focused`).
        let reader_text_focused = self.panes.get(self.focused()).is_some_and(|pane| {
            (self.preview_shown(self.focused()) && pane.preview.text_focused(window, cx))
                || pane.review.inner_focused(window, cx)
        });
        // The find bar's line keeps the keyboard while the operator types.
        let find_focused = self.panes.get(self.focused()).is_some_and(|pane| {
            pane.preview_tab
                .find
                .as_ref()
                .is_some_and(|find| find.holds_focus(window, cx))
                || pane
                    .transcript()
                    .is_some_and(|view| view.read(cx).find_holds_focus(window, cx))
        });
        // A nav view's list keeps the keyboard it was handed until the
        // operator takes it elsewhere: the view stays shown either way.
        let nav_list_focused = match self.nav_view {
            nav::NavView::Tree => false,
            nav::NavView::NeedsYou => self.floats.notice_focus.is_focused(window),
            _ => self.nav_view_focus.is_focused(window),
        };
        if window.has_active_dialog(cx)
            || self.floats_hold_focus()
            || nav_list_focused
            || native_text_focused
            || reader_text_focused
            || find_focused
            || pane_control_focused
        {
            // Native text, dialogs, and the Needs-you view keep their own keyboard focus.
        } else if self.settings_open {
            // The pump must not steal focus from Settings search or controls.
            if !self.settings_focus.contains_focused(window, cx) {
                window.focus(&self.settings_focus, cx);
            }
        } else if self.project_editor.is_some() {
            if !self.project_editor_focus.contains_focused(window, cx) {
                window.focus(&self.project_editor_focus, cx);
            }
        } else if !self
            .popover
            .as_ref()
            .is_some_and(|open| open.kind.picker_slot().is_some())
            && !wanted.is_focused(window)
            && (wanted == self.focus || !wanted.contains_focused(window, cx))
        {
            window.focus(&wanted, cx);
        }

        // The cards start under the head row (`titlebar::board_top`), with
        // `BOARD_PAD` of frame under them and right of them — none at the
        // right beside the panel, whose column is the frame there
        // (`board_bounds`), riding or at rest.
        let board_top = crate::titlebar::board_top();
        let panel_w = self.panel_column_w();
        let frame_right = self.board_right() - panel_w;
        let frame = || {
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .pt(px(board_top))
                .pr(px(frame_right))
                .pb(px(ferrite_ui::theme::BOARD_PAD))
        };
        let visible = self.visible_indices();
        // The empty board names the provider CLIs it found: ask once.
        if self.empty_board_shown()
            && self.cli_versions.is_none()
            && !self.cli_probing
            && !cfg!(test)
        {
            self.cli_probing = true;
            self.probe_cli_versions(cx);
        }
        let board = self.board_bounds(window);
        // The cards the board column's frame leaves holes for: one over
        // the whole board unless a grid or a tree lays out several.
        let mut cards = vec![board];
        let grid = if self.projects.coordinator_shown() {
            frame()
                .flex()
                .flex_col()
                .child(self.coordinator_page(window, cx))
        } else if self.empty_shown && fullscreen.is_none() {
            // Shown on purpose while Threads stay open (`show_empty_board`).
            frame().flex().child(self.empty_board(cx))
        } else if let Some(index) = fullscreen.filter(|index| !self.has_reader(*index)) {
            // The fullscreened Pane takes the whole window (with a reader,
            // its own cell splits: the board below). The other Panes are
            // not laid out at all — hidden siblings would still cost
            // layout — while their Sessions keep streaming through the pump
            // regardless (#20).
            frame()
                .flex()
                .flex_col()
                .child(self.pane_slot(index, level, window, cx))
        } else if let Some((board, tree)) = self
            .board()
            .and_then(|board| self.board_tree(board).map(|tree| (board, tree)))
        {
            // A board is its split tree (SwarmDeck's mosaic): every Pane
            // and reader at the rect the tree gives it, a grab band over
            // every seam, and — while a slot is being dragged — the wash
            // that says what a release would do. Absolute geometry, so a
            // seam drag moves exactly the two sides it sits between.
            cards.clear();
            self.tree_board(board, tree, window, cx)
        } else if visible.is_empty() {
            // Nothing open: the board says how to start instead of lying
            // blank.
            frame().flex().child(self.empty_board(cx))
        } else {
            // Solo's Pane, or its pending pair (any plain grid): cards
            // `GRID_GAP` apart, as `pane_rects` measures them.
            cards = self
                .pane_rects(window)
                .into_iter()
                .map(|(_, rect)| rect)
                .collect();
            let columns = layout::grid_shape(visible.len(), board).0.max(1);
            let gap = px(ferrite_ui::theme::GRID_GAP);
            let mut grid = frame().flex().flex_col().gap(gap);
            for row in visible.chunks(columns) {
                let mut line = div().flex().flex_row().flex_1().min_h_0().gap(gap);
                for index in row {
                    line = line.child(self.pane_slot(*index, level, window, cx));
                }
                grid = grid.child(line);
            }
            grid
        };
        // The board column's frame, over its cards' corners (a tree board
        // lays its own, under its seams).
        let grid = if cards.is_empty() {
            grid
        } else {
            div()
                .relative()
                .flex()
                .flex_1()
                .min_w_0()
                .child(grid)
                .child(ferrite_ui::components::frame_around(
                    cards.into_iter().map(window_bounds).collect(),
                ))
        };

        let grid = if lost {
            machines::held(grid, "machine-lost")
        } else {
            grid
        };

        // A terminal multiplexer's window (theme WP-C): the sidebar and the
        // board side by side over the bottom bar, the 1px seam between
        // them, and the titlebar band laid over their tops. Chrome inherits
        // Geist (`FONT_UI`) with tabular figures; the transcript and code
        // set Geist Mono themselves.
        div()
            .flex()
            .flex_col()
            // `relative`, so the titlebar strip below can lie over the band
            // the board reserves rather than take a row of its own.
            .relative()
            .size_full()
            // The window's root paints nothing on glass (theme rule 4):
            // each region lays its own fill over the blur.
            .bg(ferrite_ui::theme::paint::WINDOW)
            .font_family(ferrite_ui::theme::FONT_UI)
            .map(ferrite_ui::components::tabular)
            .track_focus(&self.focus)
            .key_context(ferrite_ui::keymap::COCKPIT)
            // At wall range no Pane holds a Composer, so the answer keys are
            // not competing with typing: they answer whichever Thread is
            // flagged, without the operator focusing it first.
            .when(level == Level::Wall, |wall| {
                wall.key_context("Ferrite Wall")
            })
            // Nothing that acts on a lost machine's Threads is listened for
            // until it is back (ADR 0026).
            .when(!lost && !coordinator_waiting, |root| {
                root.on_action(cx.listener(Self::submit))
                    .on_action(cx.listener(Self::unqueue_from_backspace))
                    .on_action(cx.listener(Self::allow))
                    .on_action(cx.listener(Self::deny))
                    .on_action(cx.listener(Self::always))
                    .on_action(cx.listener(Self::close_thread))
                    .on_action(cx.listener(Self::reopen_thread))
                    .on_action(cx.listener(Self::paste_into_composer))
                    .on_action(cx.listener(|view, _: &PickOption1, window, cx| {
                        view.pick_or_type(0, "1", window, cx)
                    }))
                    .on_action(cx.listener(|view, _: &PickOption2, window, cx| {
                        view.pick_or_type(1, "2", window, cx)
                    }))
                    .on_action(cx.listener(|view, _: &PickOption3, window, cx| {
                        view.pick_or_type(2, "3", window, cx)
                    }))
                    .on_action(cx.listener(|view, _: &PickOption4, window, cx| {
                        view.pick_or_type(3, "4", window, cx)
                    }))
                    .map(|root| self.register_decision_actions(root, cx))
            })
            .on_action(cx.listener(Self::interrupt))
            .on_action(cx.listener(Self::next_pane))
            .on_action(cx.listener(Self::previous_pane))
            .on_action(cx.listener(Self::next_decision))
            .on_action(cx.listener(Self::new_thread))
            .on_action(cx.listener(Self::new_worktree_thread))
            .on_action(cx.listener(Self::new_group))
            .on_action(cx.listener(|view, _: &TextLarger, _, cx| view.step_text_size(1, cx)))
            .on_action(cx.listener(|view, _: &TextSmaller, _, cx| view.step_text_size(-1, cx)))
            .on_action(cx.listener(|view, _: &TextReset, _, cx| view.step_text_size(0, cx)))
            .on_action(cx.listener(Self::band_cycle))
            .on_action(cx.listener(Self::tool_cycle_previous))
            .on_action(cx.listener(Self::toggle_tool_action))
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::toggle_fullscreen))
            .on_action(cx.listener(Self::open_find))
            .on_action(cx.listener(Self::toggle_nav))
            .on_action(cx.listener(Self::toggle_panel))
            .on_action(
                cx.listener(|view, _: &FocusThread1, window, cx| view.focus_rail(1, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread2, window, cx| view.focus_rail(2, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread3, window, cx| view.focus_rail(3, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread4, window, cx| view.focus_rail(4, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread5, window, cx| view.focus_rail(5, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread6, window, cx| view.focus_rail(6, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread7, window, cx| view.focus_rail(7, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread8, window, cx| view.focus_rail(8, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread9, window, cx| view.focus_rail(9, window, cx)),
            )
            .on_action(cx.listener(Self::toggle_notifications))
            .on_action(cx.listener(|view, _: &RefreshPrStatus, _, cx| {
                if let Some(thread) = view.focused_thread() {
                    view.check_pr(thread, true, cx);
                }
            }))
            .on_action(cx.listener(|view, _: &OpenPrInBrowser, _, cx| {
                if let Some(thread) = view.focused_thread() {
                    view.open_pr(thread, cx);
                }
            }))
            .on_action(cx.listener(Self::menu_next))
            .on_action(cx.listener(Self::menu_previous))
            .on_action(cx.listener(Self::history_older))
            .on_action(cx.listener(Self::history_newer))
            .on_action(cx.listener(Self::menu_pick))
            .on_action(cx.listener(Self::menu_dismiss))
            .map(|root| self.register_palette_actions(root, cx))
            // Every press forgets the preview; one in a preview marks it
            // again as it bubbles (`panel_preview`).
            .capture_any_mouse_down(cx.listener(|view, _: &MouseDownEvent, _, _| {
                view.preview_pressed = None;
            }))
            // The root covers the window, so a release anywhere ends the
            // drag; the selection it made stays until the next press. Moves
            // ride the root too (#27): a sweep keeps extending after the
            // pointer leaves the Pane div, clamped to the origin transcript.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _: &MouseUpEvent, _, cx| {
                    view.end_seam_drag(cx);
                    if view.drop_preview.take().is_some()
                        | view.dragging.take().is_some()
                        | view.nav_dragging.take().is_some()
                    {
                        cx.notify();
                    }
                }),
            )
            // Every drag move reaches every listener, so the root hears a
            // nav drag wherever the pointer is — the tree dims its row.
            .on_drag_move(
                cx.listener(|view, event: &gpui::DragMoveEvent<NavDrag>, _, cx| {
                    let drag = event.drag(cx).drag;
                    if view.nav_dragging != Some(drag) {
                        view.nav_dragging = Some(drag);
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, window, cx| {
                // The pointer inside the nav column holds the tree's order
                // still (C8); leaving it re-ranks.
                let inside = f32::from(event.position.x) < view.nav_width();
                if inside != view.nav_hovered {
                    view.nav_hovered = inside;
                    // Only the nav's order reads it.
                    view.notify_part(parts::Part::Nav, cx);
                }
                if view.seam_drag.is_some() {
                    if event.dragging() {
                        view.drag_seam(event.position, window, cx);
                    } else {
                        view.end_seam_drag(cx);
                    }
                }
            }))
            // A press anywhere the popovers did not swallow dismisses the
            // open Composer menu, picker and band popover — Pane bodies,
            // nav rows that move no focus, the strip, all of it. Bubble
            // phase, deliberately: the chips' toggles and the rows' picks
            // stop propagation first, so this can never close what a deeper
            // handler just opened or eat a pick (#24 review). The menu
            // mutes until the text moves, or the very next frame would
            // reopen it over the same trigger.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _: &MouseDownEvent, _, cx| {
                    let mut dismissed = false;
                    if let Some(open) = view.popover.take() {
                        // The menu mutes until the text moves, or the very
                        // next frame would reopen it over the same trigger.
                        if open.kind.follows_text() {
                            view.menu_muted = true;
                        }
                        dismissed = true;
                    }
                    if view.context_menu.take().is_some() {
                        dismissed = true;
                    }
                    // A press outside the editor abandons the rename — the
                    // operator looked away, which is escape, not enter.
                    // It notifies itself, so it is not counted above.
                    if view.rename.is_some() {
                        view.finish_rename(false, cx);
                    }
                    if dismissed {
                        cx.notify();
                    }
                }),
            )
            // And EVERY press clears the standing selection — and any grip
            // left from a drag whose release the window never saw — capture
            // phase, so it runs before a Pane's own press anchors a fresh
            // one. A press on the nav or the strip deselects exactly like
            // one on a transcript (#27).
            // The nav on the left at its own width, the Cockpit filling the
            // rest on the `--ground`. Fullscreen keeps the nav visible — a
            // deliberate override of sidebar-and-impl.md §3 ("the nav hides
            // entirely"): the fullscreened Pane spans the area right of the
            // nav, so the swarm stays one click away (#21).
            // A right press anywhere no row claimed closes the menu.
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|view, _: &MouseDownEvent, _, cx| {
                    if view.context_menu.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .on_action(cx.listener(Self::open_settings))
            .map(|root| self.register_board_actions(root, cx))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(self.nav(window, cx))
                    // The board lies from its own left edge — the column's
                    // width this frame — so it rides cmd-B with the column,
                    // every frame (F-12).
                    .child(
                        div()
                            .debug_selector(|| "board".into())
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .right(px(panel_w))
                            .left(px(self.board_left()))
                            .flex()
                            .child(grid),
                    ),
            )
            .child(self.bottom_bar(cx))
            // The window's own titlebar, where the platform makes the app
            // draw one. It is the last thing over the band and the first
            // thing under a menu: an overlay that reached into the band
            // would be answering the frame's hit test, not its own rows, so
            // the drag region stands down while one is open.
            .child(crate::titlebar::strip(
                self.board_left(),
                !self.overlay_open(),
                self.maximized,
            ))
            .children(self.view_head(cx))
            // The panel over the strip's right end: its tabs take
            // their presses, the gaps between them still drag. Its toggle
            // over both heads, in one place.
            .children(self.panel_element(window, cx))
            .children(self.panel_toggle_element(cx))
            .children(self.plan_card_element(window, cx))
            .children(self.context_menu_element(cx))
            .children(self.session_controls_element(window, cx))
            .children(self.context_checks_element(window, cx))
            .children(self.band_menu_element(window, cx))
            .children(self.settings_element(window, cx))
            .children(self.project_editor_element(window, cx))
            .children(self.toast_float(window, cx))
            .children(self.sheet_floats(window, cx))
            .children(self.projects_float(window, cx))
            .children(gpui::component::Root::render_dialog_layer(window, cx))
            .children(gpui::component::Root::render_notification_layer(window, cx))
    }
}

impl CockpitView {
    /// One Pane's cell — the click-to-focus and drag plumbing around
    /// `render_pane`. The same cell serves a grid slot and the fullscreen
    /// view; only who lays it out differs.
    fn pane_cell(
        &self,
        index: usize,
        level: Level,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        #[cfg(test)]
        drawn::built_pane(self.panes[index].identity);
        let content = self.pane_content(index, level, window, cx);
        if self.settings_open || self.project_editor.is_some() {
            return content;
        }
        if !self.panes[index].is_main() {
            return content;
        }
        let composer = self.panes[index].composer.clone();
        let identity = self.panes[index].identity;
        let view = cx.entity().downgrade();
        let hovered = view.clone();
        let hover = move |over: bool, _: &mut Window, cx: &mut gpui::App| {
            let _ = hovered.update(cx, |view, cx| {
                let next = match (over, view.file_drop_over) {
                    (true, _) => Some(identity),
                    (false, Some(current)) if current == identity => None,
                    (false, current) => current,
                };
                if view.file_drop_over != next {
                    view.file_drop_over = next;
                    cx.notify();
                }
            });
        };
        ferrite_pane::prompt_drop::target(
            content,
            composer,
            move |_, cx| {
                let _ = view.update(cx, |view, cx| {
                    view.file_drop_over = None;
                    if let Some(index) = view.index_of(identity) {
                        view.focus_pane(index);
                        if level == Level::Wall {
                            view.cockpit.toggle_fullscreen();
                        }
                        if let Some(draft) = view.panes[index].draft_mut() {
                            draft.band_focus = None;
                        }
                        view.popover = None;
                        cx.notify();
                    }
                });
            },
            hover,
        )
    }

    /// Whether native files hover this Pane right now: the last Pane their
    /// drag moved over, while that drag is still live.
    fn drop_target(&self, index: usize, cx: &gpui::App) -> bool {
        cx.has_active_drag() && self.file_drop_over == Some(self.panes[index].identity)
    }

    fn pane_content(
        &self,
        index: usize,
        level: Level,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let pane = &self.panes[index];
        let focused = index == self.focused();
        let cell = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .track_focus(&pane.controls_focus)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                    view.pointer_down(index, event, window, cx)
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                    let Some(thread) = view.panes.get(index).and_then(|pane| pane.thread()) else {
                        return;
                    };
                    cx.stop_propagation();
                    let copied = gpui::base::TextSelection::selected_text(window, cx)
                        .trim_end_matches('\n')
                        .to_string();
                    view.native_copy = (!copied.is_empty()).then_some(copied);
                    view.focus_pane(index);
                    view.open_context_menu(MenuTarget::Pane(thread), event.position, cx);
                }),
            );
        let cell = self.slot_drop_target(cell, pane_leaf(pane.identity), cx);
        // A draft Pane (#29): the Composer and its menu instead of a
        // transcript — nothing in core exists to read yet.
        let Some(thread) = pane.thread() else {
            let draft_id = pane.identity.draft().expect("this is a Draft Pane");
            let draft = pane.draft().expect("a Pane is a Thread or a draft");
            return cell.child(pane::render_draft(
                pane,
                pane::DraftState {
                    attachments: Composer::attachments(&pane.composer, &pane.preview, cx),
                    discard: self.draft_discard(draft_id, cx),
                    menu: (level == Level::Transcript)
                        .then(|| self.popover_element(index, cx))
                        .flatten(),
                    composer_empty: pane.composer.read(cx).is_empty(),
                    composer_files: pane.composer.read(cx).attachment_count(),
                    focused,
                    error: draft.error.as_ref(),
                    reduce_motion: cx.reduce_motion(),
                    drop_target: self.drop_target(index, cx),
                    show_focus: self.grid_board(),
                    toolbar: (level == Level::Transcript)
                        .then(|| self.composer_toolbar(index, cx))
                        .flatten(),
                },
                level,
            ));
        };
        let open = self.cockpit.thread(thread);
        let cached = self.facts.get(thread);
        let retained_transcript = if level == Level::Transcript {
            pane.transcript().map(|transcript| {
                let document = transcript.read(cx).selection_document();
                gpui::base::TextSelectionDocumentOwner::new(document)
                    .child(
                        transcript.cached(
                            gpui::StyleRefinement::default()
                                .flex_1()
                                .min_h_0()
                                .min_w_0()
                                .w_full(),
                        ),
                    )
                    .into_any_element()
            })
        } else {
            None
        };
        let rect = self
            .pane_rects(window)
            .into_iter()
            .find(|(at, _)| *at == index)
            .map(|(_, rect)| rect);
        // A parked Thread's facts, as its tile reads them (F-6, F-8).
        let parked_line = open.is_none().then(|| {
            let age = self
                .facts
                .last_used(thread)
                .map(|at| crate::facts::since_label(at, ferrite_core::clock::system_time()))
                .filter(|age| !age.is_empty())
                .map_or_else(|| "now".to_string(), |age| format!("{age} ago"));
            let turns = self.facts.turns(thread).unwrap_or(0);
            SharedString::from(format!(
                "Parked {age} \u{b7} {turns} turn{}",
                if turns == 1 { "" } else { "s" }
            ))
        });
        let facts = pane::PaneFacts {
            thread: open,
            composer_empty: pane.composer.read(cx).is_empty(),
            composer_files: pane.composer.read(cx).attachment_count(),
            focused,
            unread: self.cockpit.notifications().attention(thread),
            wall: cached.and_then(|facts| facts.wall_for(&pane.selected)),
            reduce_motion: cx.reduce_motion(),
            editing: focused
                && window.is_window_active()
                && pane.composer.read(cx).focus_handle(cx).is_focused(window),
            drop_target: self.drop_target(index, cx),
            show_focus: self.grid_board(),
            // Every head names its provider (theme WP-C): the mark is the
            // one thing a head carries beside the Thread.
            provider_mark: open
                .map(|thread| thread.provider())
                .or_else(|| cached.and_then(|facts| facts.provider)),
            decision_joined: false,
            cell_width: rect.map_or(self.cell(window).width, |rect| rect.w),
            parked_line,
        };
        // Only L1 draws a Composer to hang a popover over (#23), a model
        // picker (#25) or usage meter; the wall answers with keys alone.
        let l1 = level == Level::Transcript;
        let received_reasoning_visible = open
            .and_then(|thread| thread.activity().subject(&pane.selected))
            .and_then(|subject| subject.transcript().progress().caption())
            .is_some_and(|caption| {
                pane.transcript().is_some_and(|transcript| {
                    transcript.read(cx).received_reasoning_is_visible(&caption)
                })
            });
        let attachments = Composer::attachments(&pane.composer, &pane.preview, cx);
        let background = (level != Level::Wall)
            .then(|| self.background_chips(index, cx))
            .flatten();
        // The docked Decision merges into the Composer when that Composer
        // is a live block with nothing floating between them (rule 2.8.1).
        let joins = pane.is_main()
            && attachments.is_none()
            && background.is_none()
            && (!self.grid_board() || focused);
        let activity_decisions = (level != Level::Wall)
            .then(|| self.activity_decisions(index, joins, window, cx))
            .flatten();
        let expand_question = level != Level::Wall && self.question_needs_expansion(index, false);
        let mut facts = facts;
        facts.decision_joined = joins && activity_decisions.is_some();
        #[cfg(test)]
        drawn::record_pane(
            pane.identity,
            drawn::Pane {
                focused,
                unread: facts.unread,
                drop_target: facts.drop_target,
                editing: facts.editing,
                reduce_motion: facts.reduce_motion,
                cell_width: facts.cell_width,
                clock: open
                    .and_then(|open| open.turn().elapsed())
                    .map(ferrite_core::progress::live_seconds),
                renaming: matches!(
                    &self.rename,
                    Some((RenameTarget::PaneTitle(renamed), _)) if *renamed == thread
                ),
            },
        );
        let wiring = pane::PaneWiring {
            transcript: retained_transcript,
            received_reasoning_visible,
            attachments,
            background,
            menu: l1.then(|| self.popover_element(index, cx)).flatten(),
            // The title is the Pane's handle at every size: a drag moves a
            // grouped Pane, a double-click renames it — a tile with no
            // handle could not be rearranged at all.
            title: Some(self.activity_title(index, cx)),
            agents: l1.then(|| self.subject_strip(index, window, cx)).flatten(),
            split: l1.then(|| self.split_beside_button(index, cx)).flatten(),
            activity_decisions,
            expand_question,
            question_measurement: l1
                .then(|| self.activity_question_measurement(index, cx))
                .flatten(),
            child_footer: self.child_footer(index, cx),
            head_drag: self
                .board_is_movable()
                .then(|| head_drag(pane_leaf(pane.identity), self.pane_ghost(index))),
            quick_answers: (level == Level::Wall)
                .then(|| self.quick_answers(index, cx))
                .flatten(),
            toolbar: l1.then(|| self.composer_toolbar(index, cx)).flatten(),
            // The Composer decides whether it draws the queued row.
            unqueue: l1.then(|| {
                pane::queued_close(cx)
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.cockpit.cancel_queued(thread, false);
                        cx.notify();
                    }))
                    .into_any_element()
            }),
            fill: self
                .grid_board()
                .then(|| self.fill_button(index, thread, cx)),
        };
        cell.child(pane::render_pane(pane, facts, wiring, level))
    }

    /// The bottom bar (SPEC §5.7): `1 Solo` always and one tab per Group
    /// in the nav's order, open or not — the current view's selected (none
    /// on the empty board), each with its roll-up dot — a press on `Solo`
    /// showing Solo (the loose Thread used last, else the empty board) and
    /// on a Group entering it; then the vitals.
    fn bottom_bar(&self, cx: &mut Context<Self>) -> Div {
        let view = self.cockpit.roster().view();
        let empty = self.empty_board_shown();
        let mut tabs = Vec::new();
        // Solo's one Thread: the one it shows while it is the view, else
        // the loose Thread used last, where a press on the tab lands. (The
        // Thread written last is not always the one on screen: the current
        // tab's dot must not speak for another, nor its press move off it.)
        let solo = (!empty && view == View::Solo)
            .then(|| self.focused_thread())
            .flatten()
            .or_else(|| {
                self.panes
                    .iter()
                    .filter_map(|pane| pane.thread())
                    .filter(|thread| self.cockpit.groups().of(*thread).is_none())
                    .max_by_key(|thread| self.last_used(*thread))
            });
        // A tab's dot rolls up the Threads its view shows: Solo's one
        // Thread, a Group's members.
        let roll_up = |threads: &mut dyn Iterator<Item = ThreadId>| {
            crate::titlebar::RollUp::of(
                threads.map(|thread| self.thread_row(thread).status.attention()),
            )
        };
        tabs.push(
            crate::titlebar::bar_tab(
                "bottom-bar-tab-solo".into(),
                1,
                "Solo".into(),
                !empty && view == View::Solo,
                roll_up(&mut solo.into_iter()),
            )
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| match solo {
                Some(thread) => {
                    view.empty_shown = false;
                    view.focus_thread(thread, cx);
                }
                None => view.show_empty_board(cx),
            }))
            .into_any_element(),
        );
        for (at, group) in self.cockpit.groups().iter().enumerate() {
            let id = group.id;
            tabs.push(
                crate::titlebar::bar_tab(
                    SharedString::from(format!("bottom-bar-tab-{}", id.get())),
                    at + 2,
                    group.display_title().into(),
                    !empty && view == View::Group(id),
                    roll_up(&mut group.members.iter().copied()),
                )
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.enter_group(id, cx);
                }))
                .into_any_element(),
            );
        }
        let needs_you =
            crate::titlebar::bar_needs_you(self.cockpit.needs_you().len()).map(|count| {
                count
                    .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        view.toggle_notifications(&ToggleNotifications, window, cx);
                    }))
                    .into_any_element()
            });
        let usage = [Provider::Claude, Provider::Codex]
            .into_iter()
            .filter_map(|provider| {
                crate::titlebar::bar_usage(provider, &self.cockpit.account_limits(provider))
            })
            .map(IntoElement::into_any_element);
        let disk = self
            .free_disk
            .and_then(crate::titlebar::bar_free_disk)
            .map(IntoElement::into_any_element);
        let clock = crate::titlebar::bar_clock(ferrite_core::progress::clock_label().into());
        let vitals = needs_you
            .into_iter()
            .chain(usage)
            .chain(disk)
            .chain(Some(clock.into_any_element()))
            .collect();
        let vitals = crate::titlebar::vitals(vitals).into_any_element();
        crate::titlebar::bottom_bar(tabs, vitals)
    }

    /// Read free disk for the bar off the UI thread: the least free of the
    /// volumes holding the Threads' checkouts (SPEC §8), or a fixture
    /// world's figure. Nothing on a machine, whose checkouts are its own;
    /// tests and captures never read the real disk.
    fn read_free_disk(&mut self, cx: &mut Context<Self>) {
        if let Some(bytes) = self.cockpit.free_disk_fixture() {
            self.free_disk_landed(Some(bytes), cx);
            return;
        }
        if self.on_machine() || cfg!(any(test, feature = "visual-reference")) {
            return;
        }
        let mut paths: Vec<std::path::PathBuf> = self
            .cockpit
            .threads()
            .into_iter()
            .filter_map(|thread| self.thread_path(thread))
            .collect();
        paths.sort();
        paths.dedup();
        cx.spawn(async move |this, cx| {
            let free = cx
                .background_executor()
                .spawn(async move {
                    ferrite_core::disk::least_free(paths.iter().map(std::path::PathBuf::as_path))
                })
                .await;
            this.update(cx, |view, cx| view.free_disk_landed(free, cx))
                .ok();
        })
        .detach();
    }

    /// Adopt a free-disk reading; redraw only when the bar's words move.
    fn free_disk_landed(&mut self, free: Option<u64>, cx: &mut Context<Self>) {
        let words = |free: Option<u64>| free.and_then(crate::titlebar::free_disk_words);
        let moved = words(free) != words(self.free_disk);
        self.free_disk = free;
        if moved {
            cx.notify();
        }
    }

    /// A bound action's first keystroke as a shortcut spells it, from this
    /// platform's key table: `cmd-shift-n` → `cmd-shift-N`, which
    /// `components::key_combo` draws tight with glyph modifiers, `⌘⇧N`.
    fn key_label(action: &str) -> Option<SharedString> {
        ferrite_ui::components::bound_chord(action).map(SharedString::from)
    }

    /// A board slot as a drop target, for a slot dragged from the board
    /// and for a Thread dragged from the nav: the moves over it compute
    /// the preview, the release applies it.
    fn slot_drop_target(&self, cell: Div, target: ThreadId, cx: &mut Context<Self>) -> Div {
        // gpui hands every drag move to every listener, not just the one
        // under the pointer: a slot only previews while the pointer is
        // inside it, and drops its own preview once the pointer has left —
        // else the first slot crossed keeps saying "split left" while the
        // drag is somewhere else.
        fn left(view: &mut CockpitView, target: ThreadId, cx: &mut Context<CockpitView>) {
            if view
                .drop_preview
                .is_some_and(|(previewed, _)| previewed == target)
            {
                view.drop_preview = None;
                cx.notify();
            }
        }
        cell.on_drag_move(
            cx.listener(move |view, event: &gpui::DragMoveEvent<PaneDrag>, _, cx| {
                let source = event.drag(cx).leaf;
                if view.dragging != Some(source) {
                    view.dragging = Some(source);
                    cx.notify();
                }
                if event.bounds.contains(&event.event.position) {
                    view.preview_pane_drop(source, target, event.event.position, event.bounds, cx);
                } else {
                    left(view, target, cx);
                }
            }),
        )
        .on_drop(cx.listener(move |view, drag: &PaneDrag, _, cx| {
            view.drop_pane(drag.leaf, target, cx);
        }))
        .on_drag_move(
            cx.listener(move |view, event: &gpui::DragMoveEvent<NavDrag>, _, cx| {
                let drag = event.drag(cx).drag;
                if event.bounds.contains(&event.event.position) {
                    view.preview_nav_drop(drag, target, event.event.position, event.bounds, cx);
                } else {
                    left(view, target, cx);
                }
            }),
        )
        .on_drop(cx.listener(move |view, drag: &NavDrag, _, cx| {
            view.drop_nav_on_board(drag.drag, target, cx);
        }))
    }

    /// The size a slot was laid out at this frame, for its ghost.
    fn slot_size(&self, leaf: ThreadId) -> (f32, f32) {
        self.slot_sizes
            .borrow()
            .get(&leaf)
            .copied()
            .unwrap_or((GHOST_MAX_W, GHOST_MAX_H))
    }

    /// A Pane's ghost: its name, its branch, its size on the board.
    fn pane_ghost(&self, index: usize) -> GhostFace {
        let pane = &self.panes[index];
        GhostFace {
            title: pane.name.clone(),
            detail: pane
                .thread()
                .and_then(|thread| self.facts.get(thread))
                .and_then(|facts| facts.branch.clone()),
            reader: false,
            size: self.slot_size(pane_leaf(pane.identity)),
        }
    }

    /// A nav row's ghost: the Thread as its Pane would look — its size on
    /// the board when it is there, the largest ghost when it is not.
    fn thread_ghost(&self, thread: ThreadId) -> GhostFace {
        GhostFace {
            title: self.facts.name(thread),
            detail: self
                .facts
                .get(thread)
                .and_then(|facts| facts.branch.clone()),
            reader: false,
            size: self.slot_size(thread),
        }
    }

    fn toggle_tool(
        &mut self,
        thread: ThreadId,
        call: &pane::DisclosureId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        self.focus_pane(index);
        gpui::base::TextSelection::clear(window, cx);
        self.panes[index]
            .rich
            .clear_output_selection(&self.panes[index].text_namespace(), cx);
        self.panes[index].toggle_tool(call);
        window.focus(&self.panes[index].tool_focus(), cx);
        cx.notify();
    }

    /// The pending Decision of `thread` when it is a question, borrowed.
    fn pending_questions(
        &self,
        thread: ThreadId,
    ) -> Option<(
        ferrite_core::activity::DecisionHandle,
        &[ferrite_core::questions::Question],
    )> {
        let request = self
            .cockpit
            .thread(thread)?
            .pending_decisions()
            .iter()
            .find(|request| request.subject == Some(ferrite_core::activity::Subject::Main))?;
        let questions = pane::questions_of(&request.decision)?;
        Some((request.handle.clone(), questions))
    }

    /// A digit key (`PickOption1..4`, bound where a Decision holds the
    /// keyboard): on an empty line — or wherever no line is being typed —
    /// it picks row `option` of the request this Pane shows (a single
    /// single-select question then answers at once); with text on the line,
    /// or nothing to pick, it is a digit again.
    fn pick_or_type(
        &mut self,
        option: usize,
        digit: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.level_now(window) == Level::Wall && self.wall_pick(option, window, cx) {
            return;
        }
        let index = self.focused();
        let typing = self.level_now(window) == Level::Transcript
            && self
                .panes
                .get(index)
                .is_some_and(|pane| pane.is_main() && !pane.composer.read(cx).is_empty());
        if !typing && self.pick_request_row(index, option, window, cx) {
            return;
        }
        if let Some(pane) = self.panes.get(index) {
            pane.composer
                .clone()
                .update(cx, |composer, cx| composer.insert(digit, cx));
        }
    }

    /// A typed-out `/import` (the slash menu muted) opens the import
    /// picker, as its menu row does, rather than going to the provider as
    /// text.
    fn typed_import(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.panes.get(self.focused()) else {
            return false;
        };
        let (identity, composer) = (pane.identity, pane.composer.clone());
        if composer.read(cx).prompt().trim() != "/import" {
            return false;
        }
        let into = match identity {
            PaneIdentity::Draft(_) => Some(identity),
            PaneIdentity::Thread(_) => {
                composer.update(cx, |composer, cx| composer.take(cx));
                self.import_target(cx)
            }
        };
        self.open_import_palette(into, window, cx);
        true
    }

    /// A typed `/effort <level>` or `/model <name>` the cockpit itself can
    /// honour: the level is on the model's ladder (or `default`), the name
    /// matches a catalog row by value or display name. Anything else is
    /// None and goes to the provider as text.
    fn typed_tuning(&self, thread: ThreadId, text: &str) -> Option<Tuning> {
        let open = self.cockpit.thread(thread)?;
        let mut words = text.split_whitespace();
        let command = words.next()?;
        let argument = words.next()?;
        if words.next().is_some() {
            return None;
        }
        let argument = argument.to_ascii_lowercase();
        match command {
            "/effort" => {
                if argument == "default" {
                    return Some(Tuning::Effort(None));
                }
                let ladder = ferrite_core::providers::models::efforts_for(
                    open.provider(),
                    open.model(),
                    open.models(),
                );
                ladder
                    .into_iter()
                    .find(|level| level.eq_ignore_ascii_case(&argument))
                    .map(|level| Tuning::Effort(Some(level)))
            }
            "/model" => {
                if argument == "default" {
                    return Some(Tuning::Model(None));
                }
                self.cockpit
                    .model_catalog(open.provider())
                    .into_iter()
                    .find(|row| {
                        row.value.eq_ignore_ascii_case(&argument)
                            || row.display.eq_ignore_ascii_case(&argument)
                            || row.is(&argument)
                    })
                    .map(|row| Tuning::Model(Some(row.value).filter(|value| value != "default")))
            }
            _ => None,
        }
    }

    /// Whether `thread`'s transcript holds exactly one prompt — the moment
    /// a title is asked for.
    fn is_first_prompt(&self, thread: ThreadId) -> bool {
        self.cockpit.thread(thread).is_some_and(|open| {
            open.transcript()
                .blocks()
                .iter()
                .filter(|block| matches!(block.body, ferrite_core::transcript::Body::Prompt(_)))
                .count()
                == 1
        })
    }

    /// Ask the titler for a name, off the UI thread, and adopt it when it
    /// arrives — unless the operator has named the Thread meanwhile, or
    /// turned auto-titles off. No titler configured (tests, the demo)
    /// means the prompt-derived name stands.
    fn start_titling(&mut self, thread: ThreadId, prompt: String, cx: &mut Context<Self>) {
        if !self.prefs.settings.auto_title || !self.prefs.titler {
            return;
        }
        let Some(provider) = self
            .cockpit
            .thread(thread)
            .filter(|open| open.title().is_none())
            .map(|open| open.provider())
        else {
            return;
        };
        // The Thread's own Provider writes its title, through the one form
        // each adapter fills: Claude's `claude -p`, Codex's `codex exec` —
        // the same copy of the CLI the Session runs.
        let program = ferrite_core::providers::discover::program(provider);
        let form = ferrite_core::titler::form_for(
            provider,
            &program,
            &ferrite_core::titler::TitleRequest {
                prompt,
                reply: None,
            },
            &self.cockpit.model_catalog(provider),
        );
        let rx = ferrite_core::titler::spawn(form);
        cx.spawn(async move |this, cx| {
            let title = cx
                .background_executor()
                .spawn(async move { rx.recv().ok().flatten() })
                .await;
            this.update(cx, |view, cx| view.adopt_title(thread, title, cx))
                .ok();
        })
        .detach();
    }

    /// The titler's answer lands only on a Thread still untitled: an
    /// operator's rename in the meantime wins, and a parked Thread keeps
    /// its prompt-derived name.
    fn adopt_title(&mut self, thread: ThreadId, title: Option<String>, cx: &mut Context<Self>) {
        let Some(title) = title else {
            return;
        };
        if self
            .cockpit
            .thread(thread)
            .is_none_or(|open| open.title().is_some())
        {
            return;
        }
        if self.cockpit.rename_thread(thread, &title).is_ok() {
            self.facts.renamed(&self.cockpit, thread);
            self.refresh_names();
            cx.notify();
        }
    }

    /// The Thread `y`/`n`/`a` act on right now: the focused Thread when
    /// it waits, else the answer target (`Cockpit::answer_target`) —
    /// exactly what `answer` picks, so the one cell that shows the keys is
    /// the one they answer.
    pub(crate) fn key_target(&self) -> Option<ThreadId> {
        match self.focused_thread() {
            Some(thread)
                if self
                    .cockpit
                    .thread(thread)
                    .and_then(|open| open.pending())
                    .is_some() =>
            {
                Some(thread)
            }
            _ => self.cockpit.answer_target(),
        }
    }

    /// A wall tile's quick answers while its Thread waits (F-7): an
    /// approval's three — `1 allow`, `2 always`, `3 deny` (R9) — or a
    /// question's options, `1 retry on EEXIST`, `2 isolate per pid`, each a
    /// boxed word wired to its answer, its digit the key that answers it
    /// from the wall (`cockpit::PickOption1-3`). A question with more than
    /// one part answers in its form: its option lands on the Pane, in full.
    fn quick_answers(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.panes.get(index)?.thread()?;
        let open = self.cockpit.thread(thread)?;
        let pending = open.pending_decisions();
        let request = pending
            .iter()
            .find(|request| request.subject == Some(ferrite_core::activity::Subject::Main))
            .or_else(|| pending.first())?
            .clone();
        let key = thread.get();
        let mut buttons = Vec::new();
        match pane::questions_of(&request.decision) {
            Some(questions) => {
                let options = questions.first()?.options.iter().take(3);
                for (at, option) in options.enumerate() {
                    let label = decision::split_recommended(&option.label).0.to_string();
                    let button = pane::quick_answer(
                        SharedString::from(format!("wall-answer-{key}-{at}")),
                        SharedString::from((at + 1).to_string()),
                        label.into(),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            view.wall_answer_question(thread, at, window, cx);
                        }),
                    );
                    buttons.push(button.into_any_element());
                }
            }
            None if matches!(request.decision.kind, ferrite_core::DecisionKind::Approval) => {
                let workspace = open
                    .workspace()
                    .map(|workspace| workspace.cwd().to_path_buf());
                // The same three options as the row: `1 allow` `2 always`
                // `3 deny` (R9), each boxed with the digit that answers it.
                for row in decision::approval_rows(
                    &request.decision,
                    Some(open.provider()),
                    workspace.as_deref(),
                ) {
                    if !row.enabled {
                        continue;
                    }
                    let word = row.word();
                    let choice = row.choice;
                    let button = pane::quick_answer(
                        SharedString::from(format!("wall-answer-{key}-{word}")),
                        row.key.clone(),
                        word.into(),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            // The one answer path the row and a toast share.
                            view.answer_approval(thread, choice, None, window, cx);
                            cx.notify();
                        }),
                    );
                    buttons.push(button.into_any_element());
                }
            }
            None => return None,
        }
        (!buttons.is_empty()).then(|| pane::quick_answers(buttons).into_any_element())
    }

    /// The Thread the wall's digit keys answer (F-7): the focused tile when
    /// it waits, else the one that has waited longest.
    pub(crate) fn wall_answer_target(&self) -> Option<ThreadId> {
        let waits = |thread: ThreadId| {
            self.cockpit
                .thread(thread)
                .is_some_and(|open| !open.pending_decisions().is_empty())
        };
        self.focused_thread()
            .filter(|thread| waits(*thread))
            .or_else(|| self.key_target())
            .or_else(|| {
                self.cockpit
                    .needs_you()
                    .into_iter()
                    .find(|thread| waits(*thread))
            })
    }

    /// A digit at the wall (F-7): `1` allow, `2` allow for this thread, `3`
    /// deny and steer — focus stays where it is — or a question's option.
    /// `false` when no tile waits.
    fn wall_pick(&mut self, option: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(thread) = self.wall_answer_target() else {
            return false;
        };
        let question = self
            .cockpit
            .thread(thread)
            .and_then(|open| {
                open.pending_decisions()
                    .iter()
                    .find(|request| request.subject == Some(ferrite_core::activity::Subject::Main))
                    .map(|request| pane::questions_of(&request.decision).is_some())
            })
            .unwrap_or(false);
        if question {
            self.wall_answer_question(thread, option, window, cx);
            return true;
        }
        let Some(choice) = approval_choice(option) else {
            return false;
        };
        self.answer_approval(thread, choice, None, window, cx);
        cx.notify();
        true
    }

    /// A question's option from the wall: a single-select question answers
    /// at once; anything more opens the Pane in full to answer in its form.
    fn wall_answer_question(
        &mut self,
        thread: ThreadId,
        option: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.answer_question_option(thread, option, window, cx) {
            if let Some(index) = self.pane_for(thread) {
                self.focus_pane(index);
                if self.cockpit.roster().fullscreen().is_none() {
                    self.cockpit.toggle_fullscreen();
                }
            }
        }
        cx.notify();
    }

    /// Records a card trigger's laid-out bounds under `key`.
    fn record_trigger(
        &self,
        key: SharedString,
    ) -> impl FnOnce(gpui::Bounds<Pixels>, &mut Window, &mut gpui::App) + 'static {
        let sink = self.float_triggers.clone();
        move |bounds, _, _| {
            sink.borrow_mut().insert(key, bounds);
        }
    }

    /// Where a card hangs (`FloatPlace::card_corner`). Until its trigger
    /// or Pane has been laid out once, it waits a frame rather than guess —
    /// `CARD_GEOMETRY_FRAMES` of them at most, then hangs from the board's
    /// corner: a Pane with nothing to give (an L3 tile draws no Composer, a
    /// Pane gone from the roster draws nothing) would have the card ask for
    /// every frame for as long as it stayed open.
    fn card_corner(
        &self,
        key: String,
        from: CardFrom,
        window: &mut Window,
    ) -> Option<Point<Pixels>> {
        let up = !matches!(from, CardFrom::Head(_));
        let (CardFrom::Composer(identity) | CardFrom::Head(identity)) = from;
        let corner = self.pane_card_corner(identity, up);
        let mut waits = self.card_waits.borrow_mut();
        if corner.is_some() {
            waits.remove(&key);
            return corner;
        }
        let waited = waits.entry(key).or_default();
        if *waited < CARD_GEOMETRY_FRAMES {
            *waited += 1;
            window.request_animation_frame();
            return None;
        }
        Some(self.board_card_corner(up, window))
    }

    /// A Pane's own card: off its Composer's right edge opening up, its
    /// head's opening down, inside the Pane.
    fn pane_card_corner(&self, identity: PaneIdentity, up: bool) -> Option<Point<Pixels>> {
        let geometry = self.panes[self.index_of(identity)?].geometry.get();
        let card = geometry.card?;
        let limit_right = f32::from(card.right()) - ferrite_ui::theme::PANE_PAD_X;
        let (floor, trigger) = if up {
            let composer = geometry.composer?;
            (f32::from(composer.top()), composer)
        } else {
            (0., card)
        };
        Some(ferrite_ui::components::FloatPlace { floor, limit_right }.card_corner(trigger, up))
    }

    /// The limits a surface summoned from the window's bar rests within
    /// (ADR 0014): its foot over the bar's top, its right edge inside the
    /// window's margin.
    fn bar_place(&self) -> ferrite_ui::components::FloatPlace {
        use ferrite_ui::theme::{BOARD_PAD, GRID_PAD};
        // The board stops `BOARD_PAD` short of the bar and `board_right`
        // short of the window's right edge.
        let board = self.board.get();
        ferrite_ui::components::FloatPlace {
            floor: board.y + board.h + BOARD_PAD,
            limit_right: board.x + board.w + self.board_right() - GRID_PAD,
        }
    }

    /// Where a card hangs with no geometry to hang from: off the board as
    /// though it were the Pane — from its foot (where a Composer would be)
    /// for a card opening up, its head for one opening down.
    fn board_card_corner(&self, up: bool, window: &Window) -> Point<Pixels> {
        let board = self.board_bounds(window);
        let right = board.x + board.w;
        let bottom = board.y + board.h;
        let place = ferrite_ui::components::FloatPlace {
            floor: bottom,
            limit_right: right - ferrite_ui::theme::PANE_PAD_X,
        };
        let edge = if up { bottom } else { board.y };
        place.card_corner(
            gpui::Bounds::new(gpui::point(px(right), px(edge)), gpui::Size::default()),
            up,
        )
    }

    fn run_session_control(
        &mut self,
        thread: ThreadId,
        generation: u64,
        action: ferrite_core::SessionControl,
    ) {
        let current = self
            .cockpit
            .thread(thread)
            .is_some_and(|open| open.generation() == generation);
        if !current {
            return;
        }
        self.session_control_error = self
            .cockpit
            .control(thread, action)
            .err()
            .map(|error| (thread, generation, error.to_string()));
    }

    /// The Composer's permission mode chip (SPEC §5.5, §D.7): the `shield`,
    /// the mode's label from the Session's own list (`permission_modes`),
    /// in `ATTENTION` for bypass or full access. Always shown once the
    /// Thread has a list or a mode, so the mode can be changed, from the
    /// cached answers when no Session runs (ADR 0032). Its menu is that
    /// list, ✓ on the mode in force, a pick sent as the native
    /// `SetPermissionMode` control; ⇧⇥ still cycles (`cycle_mode`). A
    /// Session that cannot switch modes shows the chip without a menu.
    fn mode_chip(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.panes[index].thread()?;
        let open = self.cockpit.thread(thread)?;
        let generation = open.generation();
        let modes = open.permission_modes();
        if modes.is_empty() && open.permission_mode().is_none() {
            return None;
        }
        let mode = open.permission_mode().unwrap_or("default").to_string();
        let label = pane::permission_mode_label(&mode, &modes);
        let key = thread.get() as usize;
        let trigger = pane::toolbar_chip(("mode-picker", key), cx)
            .debug_selector(move || format!("mode-picker-{key}"))
            .tip("Permission mode")
            .child(pane::mode_chip(label, &mode));
        let wrap = |chip: AnyElement| {
            div()
                .debug_selector(move || format!("composer-mode-{key}"))
                .flex_shrink_0()
                .child(chip)
                .into_any_element()
        };
        if modes.is_empty() || !open.supports_control(ferrite_core::ControlKind::SetPermissionMode)
        {
            return Some(wrap(trigger.into_any_element()));
        }
        let choices = modes
            .iter()
            .map(|choice| ferrite_ui::components::Choice {
                label: choice.label.clone().into(),
                checked: choice.value == mode,
                ..Default::default()
            })
            .collect();
        let values: std::rc::Rc<Vec<String>> =
            std::rc::Rc::new(modes.into_iter().map(|choice| choice.value).collect());
        let is_open = self.mode_picker.is_some_and(|(shown, shown_generation)| {
            shown == thread && shown_generation == generation
        });
        let weak = cx.entity().downgrade();
        let picker = weak.clone();
        Some(wrap(
            ferrite_ui::menu::ChoiceMenu {
                head: Some(("Permission mode".into(), SharedString::default())),
                ladder: None,
                hints: vec![
                    ("\u{2191}\u{2193}", "mode"),
                    ("\u{23ce}", "apply"),
                    ("esc", ""),
                ],
                width: None,
                lead: 0.,
                id: format!("mode-picker-{key}").into(),
                trigger,
                choices,
                open: is_open,
                return_focus: self.panes[index].composer.focus_handle(cx),
                on_open: std::rc::Rc::new(move |open, _, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        if open {
                            view.focus_pane(index);
                            view.popover = None;
                            view.context_menu = None;
                            view.session_controls = None;
                            view.mode_picker = Some((thread, generation));
                        } else if view.mode_picker == Some((thread, generation)) {
                            view.mode_picker = None;
                        }
                        cx.notify();
                    });
                }),
                on_pick: std::rc::Rc::new(move |at, _, cx| {
                    let Some(value) = values.get(at).cloned() else {
                        return;
                    };
                    let _ = picker.update(cx, |view, cx| {
                        view.run_session_control(
                            thread,
                            generation,
                            ferrite_core::SessionControl::SetPermissionMode { mode: value },
                        );
                        view.mode_picker = None;
                        cx.notify();
                    });
                }),
                place: self.composer_place(index),
            }
            .into_any_element(),
        ))
    }

    /// The Pane's background shelf: the Session's running background tasks
    /// as chips at the Composer's right edge, each `×` wired to `StopTask`
    /// where the Session can stop one. Main only — a Subagent Subject's
    /// Composer is Main's, but a shelf there would claim the tasks were the
    /// Subagent's own. None while nothing runs, so the shelf leaves with
    /// the last task.
    fn background_chips(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pane = &self.panes[index];
        if !pane.is_main() {
            return None;
        }
        let thread = pane.thread()?;
        let open = self.cockpit.thread(thread)?;
        let generation = open.generation();
        let chips = crate::background_chips::BackgroundChips::new(
            SharedString::from(format!("background-chips-{}", thread.get())),
            open.transcript().progress().background().iter().cloned(),
        );
        if chips.is_empty() {
            return None;
        }
        if !open.supports_control(ferrite_core::ControlKind::StopTask) {
            return Some(chips.into_any_element());
        }
        let view = cx.entity().downgrade();
        Some(
            chips
                .on_stop(move |id, _, cx| {
                    let id = id.to_string();
                    let _ = view.update(cx, |view, cx| {
                        // Only a task this very Session still reports as
                        // working: a chip clicked as its snapshot changes
                        // must not stop whatever took its place.
                        let running =
                            view.cockpit.thread(thread).is_some_and(|open| {
                                open.generation() == generation
                                    && open.transcript().progress().background().iter().any(
                                        |task| {
                                            task.id == id
                                                && task.status
                                                    == ferrite_core::progress::TaskStatus::Working
                                        },
                                    )
                            });
                        if running {
                            view.run_session_control(
                                thread,
                                generation,
                                ferrite_core::SessionControl::StopTask { id },
                            );
                        }
                        cx.notify();
                    });
                })
                .into_any_element(),
        )
    }

    fn session_controls_element(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (thread, generation) = self.session_controls?;
        let at = self.card_corner(
            format!("session-{}", thread.get()),
            CardFrom::Composer(PaneIdentity::Thread(thread)),
            window,
        )?;
        let open = self.cockpit.thread(thread)?;
        if open.generation() != generation {
            return None;
        }
        let transcript = open.transcript();
        // A readout in sections, not a stack of buttons: a quiet section
        // head, then rows — a name at the left, its state and its actions
        // (quiet mono text controls) at the right.
        let text_action = |id: SharedString, verb: &'static str| {
            ferrite_ui::components::faded_button(
                id,
                gpui::rgba(ferrite_ui::theme::TRANSPARENT).into(),
                ferrite_ui::theme::FLOAT_HOVER.into(),
                ferrite_ui::theme::FLOAT_SEL.into(),
                rgb(ferrite_ui::theme::TEXT_DIM).into(),
                cx,
            )
            .h(px(ferrite_ui::theme::TOKEN_H))
            .px(px(ferrite_ui::theme::TOKEN_PAD_X))
            .rounded(px(ferrite_ui::theme::R_SMALL))
            .child(ferrite_ui::components::text_meta().child(verb))
        };
        // A card-wide verb is a whole float row: one row, the rows' inset,
        // `TEXT`, `FLOAT_HOVER` under the pointer.
        let action_row = |id: SharedString, verb: &'static str| {
            ferrite_ui::components::form_button(id, cx)
                .h(px(ferrite_ui::theme::FLOAT_ROW_H))
                .mx(px(ferrite_ui::theme::FLOAT_PAD))
                .px(px(ferrite_ui::theme::MENU_ROW_PAD_X))
                .rounded(px(ferrite_ui::theme::R_MENU_ROW))
                .child(
                    div()
                        .flex()
                        .w_full()
                        .text_size(px(ferrite_ui::theme::FS_UI))
                        .line_height(px(ferrite_ui::theme::LH_UI))
                        .text_color(rgb(ferrite_ui::theme::TEXT))
                        .child(verb),
                )
        };
        // A section head is the float's section heading (theme WP-E): it
        // carries its own air.
        let head = |title: &'static str, _after: bool| ferrite_ui::menu::section(title, None, None);
        let row = || {
            div()
                .flex()
                .items_center()
                .gap(px(ferrite_ui::theme::SPACE_2))
                .min_h(px(ferrite_ui::theme::FLOAT_ROW_H))
                .px(px(ferrite_ui::theme::FLOAT_PAD_X))
                .min_w_0()
        };
        let mut card = ferrite_ui::menu::float()
            .pb(px(ferrite_ui::theme::FLOAT_PAD))
            .id("session-controls-card")
            .debug_selector(|| "session-controls-card".into())
            .w(px(ferrite_ui::theme::SESSION_CARD_W))
            .max_h(px(ferrite_ui::theme::MENU_MAX_H))
            .overflow_y_scroll();
        let error = self
            .session_control_error
            .as_ref()
            .filter(|(shown, shown_generation, _)| {
                *shown == thread && *shown_generation == generation
            });
        let errored = error.is_some();
        if let Some((_, _, error)) = error {
            card = card.child(pane::card_error(error.clone()).id("session-control-error"));
        }
        let modes = open.permission_modes();
        let modes_empty = modes.is_empty();
        if !modes_empty {
            card = card.child(head("Mode", errored));
        }
        let current = open.permission_mode().map(str::to_owned);
        for (index, mode) in modes.into_iter().enumerate() {
            let checked = current.as_deref() == Some(mode.value.as_str());
            let value = mode.value;
            card = card.child(
                ferrite_ui::components::button(SharedString::from(format!(
                    "permission-mode-{index}"
                )))
                .debug_selector(move || format!("permission-mode-{index}"))
                .tab_stop(true)
                .h(px(ferrite_ui::theme::FLOAT_ROW_H))
                .mx(px(ferrite_ui::theme::FLOAT_PAD))
                .px(px(ferrite_ui::theme::MENU_ROW_PAD_X))
                .rounded(px(ferrite_ui::theme::R_MENU_ROW))
                .accessibility_label(mode.label.clone())
                .child(
                    div()
                        .flex()
                        .w_full()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                // The shared button is kit-xsmall; a menu
                                // row reads at the UI size like every menu.
                                .text_size(px(ferrite_ui::theme::FS_UI))
                                .line_height(px(ferrite_ui::theme::LH_UI))
                                .text_color(rgb(if checked {
                                    ferrite_ui::theme::TEXT_STRONG
                                } else {
                                    ferrite_ui::theme::TEXT
                                }))
                                .child(mode.label),
                        )
                        .children(checked.then(|| {
                            ferrite_ui::components::glyph(
                                "\u{2713}",
                                ferrite_ui::theme::TEXT_STRONG,
                            )
                        })),
                )
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.run_session_control(
                        thread,
                        generation,
                        ferrite_core::SessionControl::SetPermissionMode {
                            mode: value.clone(),
                        },
                    );
                    cx.notify();
                })),
            );
        }
        // With no servers the note alone says so; a `MCP servers` head over
        // `No MCP servers reported` would say it twice.
        if transcript.mcp_servers().is_empty() {
            card = card.child(
                ferrite_ui::menu::note("No MCP servers reported")
                    .when(errored || !modes_empty, |note| {
                        note.mt(px(ferrite_ui::theme::FLOAT_SECTION_GAP))
                    }),
            );
        } else {
            card = card.child(head("MCP servers", errored || !modes_empty));
        }
        for (index, server) in transcript.mcp_servers().iter().enumerate() {
            let name = server.name.clone();
            // (selector key, the word drawn, the dot's ink). Connecting is
            // work in progress, not a stop: only a sign-in waits on you.
            let (status, word, dot) = match server.status {
                ferrite_core::McpStatus::Connected => {
                    ("connected", "connected", ferrite_ui::theme::RUNNING)
                }
                ferrite_core::McpStatus::Connecting => {
                    ("connecting", "connecting", ferrite_ui::theme::RUNNING)
                }
                ferrite_core::McpStatus::NeedsAuth => {
                    ("needs-auth", "needs sign-in", ferrite_ui::theme::ATTENTION)
                }
                ferrite_core::McpStatus::Failed => ("failed", "failed", ferrite_ui::theme::BLOCKED),
                ferrite_core::McpStatus::Disabled => {
                    ("disabled", "disabled", ferrite_ui::theme::TEXT_FAINT)
                }
                ferrite_core::McpStatus::Unknown => {
                    ("unknown", "unknown", ferrite_ui::theme::TEXT_FAINT)
                }
            };
            let connected = server.status == ferrite_core::McpStatus::Connected;
            let mut line = row()
                .group(SharedString::from(format!("mcp-row-{index}")))
                .child(ferrite_ui::components::status_dot(dot))
                .child(
                    div()
                        .id(("mcp-server-name", index))
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(ferrite_ui::theme::TEXT))
                        // A long server name gives way to its state and
                        // actions; the whole of it is one hover away.
                        .tooltip(ferrite_ui::menu::tooltip(server.name.clone()))
                        .child(server.name.clone()),
                )
                // A connected server's green dot says so; the word appears
                // only when the state needs reading.
                .when(!connected, |line| {
                    line.child(
                        div()
                            .debug_selector(move || format!("mcp-status-{index}-{status}"))
                            .flex_shrink_0()
                            .text_size(px(ferrite_ui::theme::FS_UI))
                            .text_color(rgb(if server.status == ferrite_core::McpStatus::Failed {
                                ferrite_ui::theme::BLOCKED
                            } else {
                                ferrite_ui::theme::TEXT_DIM
                            }))
                            .child(word),
                    )
                });
            if server.status == ferrite_core::McpStatus::NeedsAuth
                && open.supports_control(ferrite_core::ControlKind::LoginMcp)
            {
                let login_name = server.name.clone();
                line = line.child(
                    text_action(SharedString::from(format!("mcp-login-{index}")), "Sign in")
                        .debug_selector(move || format!("mcp-login-{index}"))
                        .tab_stop(true)
                        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                            let exists = view.cockpit.thread(thread).is_some_and(|open| {
                                open.generation() == generation
                                    && open
                                        .transcript()
                                        .mcp_servers()
                                        .iter()
                                        .any(|server| server.name == login_name)
                            });
                            if exists {
                                view.run_session_control(
                                    thread,
                                    generation,
                                    ferrite_core::SessionControl::LoginMcp {
                                        server: login_name.clone(),
                                    },
                                );
                            }
                            cx.notify();
                        })),
                );
            }
            if let Some(url) = transcript
                .mcp_authorizations()
                .get(&server.name)
                .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
            {
                let url = url.clone();
                line = line.child(
                    text_action(
                        SharedString::from(format!("mcp-authorize-{index}")),
                        "Open sign-in page",
                    )
                    .debug_selector(move || format!("mcp-authorize-{index}"))
                    .tab_stop(true)
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url))),
                );
            }
            if open.supports_control(ferrite_core::ControlKind::ReconnectMcp) {
                line = line.child(
                    text_action(
                        SharedString::from(format!("mcp-reconnect-{index}")),
                        "Reconnect",
                    )
                    .debug_selector(move || format!("mcp-reconnect-{index}"))
                    // A healthy server's reconnect waits under the pointer,
                    // so the name keeps the row; a troubled one shows it.
                    .when(connected, |action| {
                        action
                            .invisible()
                            .group_hover(format!("mcp-row-{index}"), |style| style.visible())
                    })
                    .on_click(cx.listener(
                        move |view, _: &ClickEvent, _, cx| {
                            let exists = view.cockpit.thread(thread).is_some_and(|open| {
                                open.generation() == generation
                                    && open
                                        .transcript()
                                        .mcp_servers()
                                        .iter()
                                        .any(|server| server.name == name)
                            });
                            if exists {
                                view.run_session_control(
                                    thread,
                                    generation,
                                    ferrite_core::SessionControl::ReconnectMcp {
                                        server: name.clone(),
                                    },
                                );
                            }
                            cx.notify();
                        },
                    )),
                );
            }
            card = card.child(line);
            if let Some(error) = server.error.as_ref() {
                card = card.child(
                    div()
                        .pl(px(ferrite_ui::theme::FLOAT_PAD_X
                            + ferrite_ui::theme::STATUS_DOT
                            + ferrite_ui::theme::CH))
                        .pr(px(ferrite_ui::theme::FLOAT_PAD_X))
                        .text_size(px(ferrite_ui::theme::FS_UI))
                        .text_color(rgb(ferrite_ui::theme::TEXT_DIM))
                        .whitespace_normal()
                        .child(error.clone()),
                );
            }
        }
        if open.supports_control(ferrite_core::ControlKind::ReloadMcp) {
            card = card.child(
                action_row("mcp-reload".into(), "Reload MCP servers")
                    .debug_selector(|| "mcp-reload".into())
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.run_session_control(
                            thread,
                            generation,
                            ferrite_core::SessionControl::ReloadMcp,
                        );
                        cx.notify();
                    })),
            );
        }
        let tasks = transcript.progress().background();
        if !tasks.is_empty() || open.supports_control(ferrite_core::ControlKind::BackgroundTasks) {
            card = card.child(head("Background tasks", true));
        }
        for (index, task) in tasks.iter().enumerate() {
            let working = task.status == ferrite_core::progress::TaskStatus::Working;
            let mut line = row()
                .child(ferrite_ui::components::status_dot(if working {
                    ferrite_ui::theme::RUNNING
                } else {
                    ferrite_ui::theme::TEXT_FAINT
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(ferrite_ui::theme::TEXT))
                        .child(task.label.clone()),
                );
            if working && open.supports_control(ferrite_core::ControlKind::StopTask) {
                let id = task.id.clone();
                line = line.child(
                    text_action(
                        SharedString::from(format!("background-stop-{index}")),
                        "Stop",
                    )
                    .debug_selector(move || format!("background-stop-{index}"))
                    .on_click(cx.listener(
                        move |view, _: &ClickEvent, _, cx| {
                            let exists = view.cockpit.thread(thread).is_some_and(|open| {
                                open.generation() == generation
                                    && open.transcript().progress().background().iter().any(
                                        |task| {
                                            task.id == id
                                                && task.status
                                                    == ferrite_core::progress::TaskStatus::Working
                                        },
                                    )
                            });
                            if exists {
                                view.run_session_control(
                                    thread,
                                    generation,
                                    ferrite_core::SessionControl::StopTask { id: id.clone() },
                                );
                            }
                            cx.notify();
                        },
                    )),
                );
            }
            card = card.child(line);
        }
        if open.supports_control(ferrite_core::ControlKind::BackgroundTasks) {
            card = card.child(
                action_row("background-all".into(), "Run tasks in background")
                    .debug_selector(|| "background-all".into())
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.run_session_control(
                            thread,
                            generation,
                            ferrite_core::SessionControl::BackgroundTasks,
                        );
                        cx.notify();
                    })),
            );
        }
        let card = card
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .on_mouse_down_out(cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                if view.session_controls.take().is_some() {
                    cx.notify();
                }
            }));
        Some(
            deferred(
                anchored()
                    .anchor(gpui::Anchor::BottomRight)
                    .position(at)
                    .snap_to_window_with_margin(px(ferrite_ui::theme::GRID_PAD))
                    .child(ferrite_ui::motion::menu_in(
                        "session-controls-in",
                        card,
                        ferrite_ui::motion::Opens::Up,
                    ))
                    .into_any_element(),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// The checks card: every run behind the header's rollup, grouped by
    /// the workflow that owns it, each row opening its own log. Built here
    /// rather than in `pane.rs` because the rows are controls and a
    /// listener needs this view's `Context`.
    fn context_checks_element(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let thread = self.context_checks?;
        let at = self.card_corner(
            format!("ci-{}", thread.get()),
            CardFrom::Head(PaneIdentity::Thread(thread)),
            window,
        )?;
        let pr = self.facts.pr(thread)?;
        let mut card = pane::checks_card().child(pane::checks_head(pr));
        let mut workflow: Option<Option<&str>> = None;
        for (index, run) in pr.runs.iter().enumerate() {
            let group = run.workflow.as_deref();
            if workflow != Some(group) {
                card = card.child(pane::checks_group(group, workflow.is_none()));
                workflow = Some(group);
            }
            let row = pane::check_row(index, run);
            card = card.child(match run.url.clone() {
                // A press opens the run's log where the browser is, and
                // closes the card: the answer is now in the other window.
                Some(url) => row.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        cx.open_url(&url);
                        view.context_checks = None;
                        cx.notify();
                    }),
                ),
                None => row,
            });
        }
        let card = card
            .id("context-checks-card")
            .debug_selector(|| "context-checks-card".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .on_mouse_down_out(cx.listener(|view, _: &MouseDownEvent, _, cx| {
                if view.context_checks.take().is_some() {
                    cx.notify();
                }
            }));
        // The mark rides the Pane head's checkout line: the card drops
        // below it, from its left edge.
        Some(
            deferred(
                anchored()
                    .anchor(gpui::Anchor::TopRight)
                    .position(at)
                    .snap_to_window_with_margin(px(ferrite_ui::theme::GRID_PAD))
                    .child(ferrite_ui::motion::menu_in(
                        "context-checks-in",
                        card,
                        ferrite_ui::motion::Opens::Down,
                    )),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// The Thread's model and effort chips (#25, SPEC §5.5): the provider's
    /// logomark and the model as the picker names it; then the effort in
    /// force, sentence case, when the model takes one. Each opens its own
    /// picker, even while working: the current choice stays visible and
    /// availability follows the Thread's busy state, said in the tooltip.
    /// Both read the cached answers, so they work with no Session.
    fn model_chips(&self, index: usize, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(thread) = self.panes[index].thread() else {
            return Vec::new();
        };
        let Some(open) = self.cockpit.thread(thread) else {
            return Vec::new();
        };
        let provider = open.provider();
        let hold = tuning_hold(&open);
        // The standing choice names the chip; else what the Session's own
        // Init said is serving; until either, the Provider's own name —
        // and always the name a person says, never the id on the wire.
        let label = match open.model().or_else(|| open.transcript().model()) {
            Some(model) => {
                SharedString::from(ferrite_core::providers::models::label(model, open.models()))
            }
            None => SharedString::from(provider_title(provider)),
        };
        let ladder =
            ferrite_core::providers::models::efforts_for(provider, open.model(), open.models());
        let effort = (!ladder.is_empty())
            .then(|| {
                effort_value(
                    open.effort(),
                    self.prefs.settings.effort_for(provider),
                    provider,
                    open.model(),
                    open.models(),
                )
                .or_else(|| {
                    ferrite_core::providers::models::cli_default_effort(provider, open.model())
                        .map(effort_chip_label)
                })
            })
            .flatten();
        let key = thread.get() as usize;
        let mut chips = vec![self
            .choice_menu(
                index,
                Kind::Provider,
                pane::toolbar_chip(("model-picker", key), cx)
                    .debug_selector(move || format!("model-picker-{key}"))
                    .tip(hold.unwrap_or("Model"))
                    .child(pane::model_chip(Some(provider), label)),
                cx,
            )
            .into_any_element()];
        if let Some(effort) = effort {
            chips.push(
                self.choice_menu(
                    index,
                    Kind::Effort,
                    pane::toolbar_chip(("effort-picker", key), cx)
                        .debug_selector(move || format!("effort-picker-{key}"))
                        .tip(hold.unwrap_or("Reasoning effort"))
                        .child(pane::effort_chip(pane::sentence_case(&effort).into())),
                    cx,
                )
                .into_any_element(),
            );
        }
        chips
    }

    /// Where a Composer chip's menu rests (SPEC §5.5): its foot over the
    /// chip's top — the toolbar's chips stand centred in the box's foot
    /// row — and inside the box's right edge. Until the box has been laid
    /// out once, over the bar.
    fn composer_place(&self, index: usize) -> ferrite_ui::components::FloatPlace {
        use ferrite_ui::theme::{COMPOSER_TOOLBAR_H, TOOL_CHIP_H};
        match self
            .panes
            .get(index)
            .and_then(|pane| pane.geometry.get().composer)
        {
            Some(field) => ferrite_ui::components::FloatPlace {
                floor: f32::from(field.bottom()) - (COMPOSER_TOOLBAR_H + TOOL_CHIP_H) / 2.,
                limit_right: f32::from(field.right()),
            },
            None => self.bar_place(),
        }
    }

    fn choice_menu(
        &self,
        index: usize,
        kind: Kind,
        trigger: gpui::component::button::Button,
        cx: &mut Context<Self>,
    ) -> ferrite_ui::menu::ChoiceMenu {
        let identity = self.panes[index].identity;
        let slot = kind.picker_slot().expect("native choice menu");
        let (effort, band) = slot;
        // The Thread's model picker carries the effort ladder under its rows
        // (the prototype's `effort  low  medium  high  max`): ←/→ or a click
        // set the level at once, as the effort menu's own pick does.
        let ladder = (!effort && !band)
            .then(|| identity.thread())
            .flatten()
            .and_then(|thread| {
                let (steps, labels, chosen) = self.effort_ladder(thread)?;
                let view = cx.entity().downgrade();
                Some(ferrite_ui::menu::Ladder {
                    label: "Effort".into(),
                    steps: labels,
                    chosen,
                    on_step: std::rc::Rc::new(move |at, _, cx| {
                        let Some(level) = steps.get(at).cloned() else {
                            return;
                        };
                        let _ =
                            view.update(cx, |view, cx| view.pick_effort(thread, Some(level), cx));
                    }),
                })
            });
        let (head, things) = match (effort, band) {
            (false, false) => (("Model", "/model"), "model"),
            (true, false) => (("Effort", "/effort"), "effort"),
            (false, true) => (("Model", ""), "model"),
            (true, true) => (("Effort", ""), "effort"),
        };
        let mut hints = vec![("\u{2191}\u{2193}", things)];
        if ladder.is_some() {
            hints.push(("\u{2190}\u{2192}", "effort"));
        }
        hints.extend([("\u{23ce}", "apply"), ("esc", "")]);
        let open = self
            .popover
            .as_ref()
            .filter(|open| open.pane == identity && open.kind.picker_slot() == Some(slot));
        let choices = open
            .map(|open| {
                let mut provider = None;
                open.rows
                    .iter()
                    .map(|row| {
                        let section = row.inert
                            && row.consequence_is_inert()
                            && provider_of_title(&row.name).is_some();
                        if section {
                            provider = provider_of_title(&row.name);
                        }
                        ferrite_ui::components::Choice {
                            label: row.name.clone(),
                            checked: row.active,
                            disabled: row.inert,
                            section,
                            // What a row resolves to (`Opus 5.5 (1M) · from
                            // Settings`); a section's handover note too.
                            detail: (!row.detail.is_empty()).then(|| row.detail.clone()),
                            // Why the row is dead for now: its tooltip.
                            why: open
                                .hold
                                .filter(|_| row.inert && !section)
                                .map(SharedString::from),
                            icon: provider.map(|provider| match provider {
                                Provider::Claude => (
                                    ferrite_ui::icons::CLAUDE,
                                    ferrite_ui::theme::PROVIDER_CLAUDE,
                                ),
                                Provider::Codex => {
                                    (ferrite_ui::icons::CODEX, ferrite_ui::theme::PROVIDER_CODEX)
                                }
                            }),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let weak = cx.entity().downgrade();
        let picker = weak.clone();
        let hold = identity.thread().and_then(|thread| {
            self.cockpit
                .thread(thread)
                .and_then(|thread| tuning_hold(&thread))
        });
        ferrite_ui::menu::ChoiceMenu {
            head: Some((head.0.into(), head.1.into())),
            ladder,
            hints,
            // The model picker is the prototype's 66 cells; the rest hug
            // their rows.
            width: (!effort).then_some(ferrite_ui::theme::MODEL_PICKER_W),
            // Its section logo 8px left of the status logo (FL-13).
            lead: if !effort && !band {
                ferrite_ui::theme::MODEL_PICKER_NUDGE
            } else {
                0.
            },
            // Rebuild the retained menu when availability changes.
            id: format!("choice-{identity:?}-{effort}-{}", hold.unwrap_or_default()).into(),
            trigger,
            choices,
            open: open.is_some(),
            return_focus: self.panes[index].composer.focus_handle(cx),
            on_open: std::rc::Rc::new(move |open, _, cx| {
                let _ = weak.update(cx, |view, cx| {
                    if open {
                        if let Some(index) = view.index_of(identity) {
                            view.focus_pane(index);
                        }
                        if band {
                            view.open_band_popover(
                                if effort {
                                    pane::BandChip::Effort
                                } else {
                                    pane::BandChip::Provider
                                },
                                cx,
                            );
                        } else if let Some(thread) = identity.thread() {
                            if effort {
                                view.open_effort_picker(thread, cx);
                            } else {
                                view.open_provider_picker(thread, cx);
                            }
                        }
                    } else if view.popover.as_ref().is_some_and(|open| {
                        open.pane == identity && open.kind.picker_slot() == Some(slot)
                    }) {
                        view.popover = None;
                        cx.notify();
                    }
                });
            }),
            on_pick: std::rc::Rc::new(move |at, window, cx| {
                let _ = picker.update(cx, |view, cx| view.pick(at, window, cx));
            }),
            place: self.composer_place(index),
        }
    }

    /// The model picker's effort row for `thread` (FL-13): the model's
    /// ladder (wire values and the words the tokens print) and the one step
    /// shown inverse — the effort in force (`effort_value`), else the level
    /// the CLI itself runs at (`models::cli_default_effort`). `None` for a
    /// model that takes no effort.
    pub(super) fn effort_ladder(
        &self,
        thread: ThreadId,
    ) -> Option<(Vec<String>, Vec<SharedString>, Option<usize>)> {
        let open = self.cockpit.thread(thread)?;
        let provider = open.provider();
        let steps =
            ferrite_core::providers::models::efforts_for(provider, open.model(), open.models());
        if steps.is_empty() {
            return None;
        }
        let labels: Vec<SharedString> = steps.iter().map(|step| effort_chip_label(step)).collect();
        let current = effort_value(
            open.effort(),
            self.prefs.settings.effort_for(provider),
            provider,
            open.model(),
            open.models(),
        )
        .or_else(|| {
            ferrite_core::providers::models::cli_default_effort(provider, open.model())
                .map(effort_chip_label)
        });
        let chosen = current.and_then(|current| labels.iter().position(|label| *label == current));
        Some((steps, labels, chosen))
    }

    /// The effort picker in the Composer slot: the operator's default for
    /// the provider on top (what it resolves to, named), then the ladder
    /// the Thread's model takes — from the provider's own announcement
    /// (Claude's handshake, Codex's model/list), else the catalog. The ✓
    /// sits on the level in force.
    fn open_effort_picker(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        let provider = open.provider();
        let hold = tuning_hold(&open);
        let chosen = open.effort().map(str::to_string);
        let default = self.prefs.settings.effort_for(provider).map(str::to_string);
        let ladder =
            ferrite_core::providers::models::efforts_for(provider, open.model(), open.models());
        let mut rows: Vec<Row> = Vec::new();
        rows.push(Row {
            row: pane::MenuRow {
                insert: SharedString::default(),
                name: SharedString::from("Default"),
                matched: Vec::new(),
                detail: SharedString::from(match &default {
                    Some(default) => format!("{} · from Settings", effort_title(default)),
                    None => "The CLI's own choice".to_string(),
                }),
                prose_detail: true,
                inert: false,
            },
            active: chosen.is_none(),
            consequence: Consequence::Effort(None),
        });
        for effort in ladder {
            rows.push(Row {
                row: pane::MenuRow {
                    insert: SharedString::default(),
                    name: SharedString::from(effort_title(&effort)),
                    matched: Vec::new(),
                    detail: SharedString::from(effort_detail(&effort)),
                    prose_detail: true,
                    inert: false,
                },
                active: chosen.as_deref() == Some(effort.as_str()),
                consequence: Consequence::Effort(Some(effort)),
            });
        }
        let selected = rows.iter().position(|row| row.active).unwrap_or(0);
        Self::hold_tuning_rows(&mut rows, hold);
        self.popover = Some(Popover {
            scroll: ScrollHandle::new(),
            revealed: Default::default(),
            hold,
            pane: PaneIdentity::Thread(thread),
            kind: Kind::Effort,
            rows,
            selected,
        });
        cx.notify();
    }

    /// An effort-row pick: the core re-aims the Thread (eagerly before the
    /// first prompt, by resuming after it, refused mid-turn — its own
    /// words land in the transcript either way).
    fn pick_effort(&mut self, thread: ThreadId, effort: Option<String>, cx: &mut Context<Self>) {
        if let Err(e) = self.cockpit.set_effort(thread, effort) {
            self.cockpit.say(thread, format!("effort unchanged: {e}"));
        }
        self.facts.acted(&self.cockpit, thread);
        cx.notify();
    }

    /// cmd-i: the Needs-you view, shown or back to the tree
    /// (§5.1.2, FL-16). Showing it closes the floats over it, puts its
    /// cursor on the first row and hands it the keyboard (its keys ride
    /// the `Notifications` context).
    fn toggle_notifications(
        &mut self,
        _: &ToggleNotifications,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.nav_view != nav::NavView::NeedsYou {
            self.popover = None;
            self.context_menu = None;
            self.floats.palette = None;
            self.floats.shortcuts = None;
        }
        self.toggle_nav_view(nav::NavView::NeedsYou, Some(window), cx);
    }

    /// Whether the Needs-you view is shown.
    pub(crate) fn needs_you_shown(&self) -> bool {
        self.nav_view == nav::NavView::NeedsYou
    }

    /// Back to the tree from a view (Esc, or opening what a row stands
    /// for), the keyboard to the board.
    pub(super) fn close_nav_view(&mut self, cx: &mut Context<Self>) {
        if self.nav_view != nav::NavView::Tree {
            self.nav_view = nav::NavView::Tree;
            cx.notify();
        }
    }

    /// Every click or key on the Needs-you view's surfaces — a toast's `⏎ open`, a
    /// row, ⏎ and ⌫ on the list, `mark all read` — lands here and is
    /// answered against core. Opening lands the operator on the Notice's
    /// Pane: the core's one focus door, so a fullscreen re-aims and a
    /// parked Thread revives, and the mirror follows. (A toast's quick
    /// answer needs the window: `answer_from_toast`.)
    pub(crate) fn notice_verb(&mut self, verb: Verb, cx: &mut Context<Self>) {
        match verb {
            Verb::Open(id) => {
                self.needs_you
                    .drop_toast(&crate::notifications::RowTarget::Notice(id));
                if self.cockpit.open_notice(id).is_some() {
                    self.sync_panes(cx);
                }
                self.close_nav_view(cx);
            }
            Verb::Dismiss(id) => {
                self.cockpit.dismiss_notice(id);
            }
            Verb::OpenDecision(id) => {
                self.needs_you
                    .drop_toast(&crate::notifications::RowTarget::Decision(id.clone()));
                let subject = self
                    .cockpit
                    .notifications()
                    .decision(&id)
                    .and_then(|notice| notice.subject.clone())
                    .unwrap_or(ferrite_core::activity::Subject::Main);
                if let Some(thread) = self.cockpit.open_decision_notice(&id) {
                    self.sync_panes(cx);
                    self.select_subject_from_notice(thread, subject, cx);
                }
                self.close_nav_view(cx);
            }
            Verb::DismissDecision(id) => {
                self.cockpit.dismiss_decision_notice(&id);
            }
            // Every row read, none removed: titles drop to plain, the badge
            // clears.
            Verb::MarkAllRead => {
                self.cockpit.mark_notices_read();
                if let Some(machines) = &self.machines {
                    machines.update(cx, |machines, _| machines.mark_notices_read());
                }
            }
            Verb::Answer(..) => {}
            Verb::GoTo(alert) => {
                self.close_nav_view(cx);
                self.go_to_alert(alert, cx);
            }
        }
        cx.notify();
    }

    /// A toast's `1 allow` / `3 deny` (FL-18): the decisions package's
    /// answer to that Thread's waiting approval, and the toast goes.
    fn answer_from_toast(
        &mut self,
        id: ferrite_core::notifications::DecisionNoticeId,
        answer: crate::notifications::QuickAnswer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let choice = match answer {
            crate::notifications::QuickAnswer::Allow => {
                ferrite_pane::decision::ApprovalChoice::Allow
            }
            crate::notifications::QuickAnswer::Deny => {
                ferrite_pane::decision::ApprovalChoice::DenyAndSteer
            }
        };
        self.needs_you
            .drop_toast(&crate::notifications::RowTarget::Decision(id.clone()));
        self.answer_approval(id.thread, choice, None, window, cx);
        cx.notify();
    }

    /// The Needs-you view's verbs, as a closure the surfaces can hold.
    fn notice_handle(&self, cx: &mut Context<Self>) -> Handle {
        let view = cx.entity().downgrade();
        std::rc::Rc::new(move |verb, window, cx| {
            let _ = view.update(cx, |view, cx| {
                // An answered toast goes at once (`ToastStage::dismiss`).
                view.toast_stage.borrow_mut().dismiss();
                match verb {
                    Verb::Answer(id, answer) => view.answer_from_toast(id, answer, window, cx),
                    verb => view.notice_verb(verb, cx),
                }
            });
        })
    }

    /// One completion Notice in the list's words (R7): `✗ failed` with the
    /// turn's failing test summary (a turn whose last test run failed is
    /// failed) or its error; else `✓ done` with the final answer when it is
    /// one line that fits the row, else `worked for <settled duration>`.
    fn notice_row(
        &self,
        notice: &ferrite_core::notifications::Notice,
        now: std::time::SystemTime,
    ) -> NoticeRow {
        use crate::notifications::{Detail, State};
        let title = self.facts.name(notice.thread);
        let finish = self
            .cockpit
            .thread(notice.thread)
            .map(|open| open.digest().finish.clone())
            .unwrap_or_default();
        let (state, detail) = match (&notice.outcome, finish.failing_tests) {
            (_, Some(summary)) => (State::Failed, Detail::Words(summary.into())),
            (ferrite_core::TurnOutcome::Error(error), None) => (
                State::Failed,
                Detail::Words(
                    ferrite_core::progress::one_line(error, ferrite_core::progress::ROW_CHARS)
                        .into(),
                ),
            ),
            _ => {
                let room =
                    crate::notifications::ROW_TEXT_CELLS.saturating_sub(title.chars().count() + 3);
                (
                    State::Done,
                    crate::notifications::done_words(
                        finish.answer.as_deref(),
                        finish.worked_ms.map(Duration::from_millis),
                        room,
                    ),
                )
            }
        };
        NoticeRow {
            away: None,
            target: crate::notifications::RowTarget::Notice(notice.id),
            thread: notice.thread,
            title,
            state,
            detail,
            when: crate::facts::since_label(notice.at, now),
            read: notice.read,
            folded: Vec::new(),
        }
    }

    /// Keep the Needs-you view up with the Notices and requests (FL-18, ADR 0017),
    /// on every pump: hear what arrived here and on the machines off
    /// screen (ADR 0026), and ring what waited out its delay and still
    /// stands — a toast, a sound, and a system notification unless its
    /// Thread's Pane is focused in the frontmost window (focus-notify's
    /// rule, ADR 0005's update).
    ///
    /// The operator is looking at an event, and nothing rings for it — no
    /// system notification either — while the window has focus and its
    /// Thread is on the board. Nobody looks at a machine off screen.
    fn ring_alerts(&mut self, cx: &mut Context<Self>) {
        let now = cx.background_executor().now();
        let entity = self.machines.clone();
        let machines = entity.as_ref().map(|machines| machines.read(cx));
        let notifications = self.cockpit.notifications();
        // Until its Server answers, a Cockpit's Notices are none, not news:
        // hearing them would forget the requests heard of it, and judging
        // its alerts by them would drop the ones the window brought along.
        let here = self.attached().then_some(notifications);
        if let Some(here) = here {
            let (fresh, requests) = news(here, self.needs_you.presented(None));
            self.needs_you.hear(None, fresh, requests, now);
        }
        for away in machines.iter().flat_map(|machines| machines.off_screen()) {
            let (fresh, requests) = news(
                away.notifications,
                self.needs_you.presented(Some(&away.key)),
            );
            self.needs_you.hear(Some(away.key), fresh, requests, now);
        }
        let live = |alert: &crate::notifications::Alert| machines::wanted(alert, here, machines);
        // A system notification the operator has since read, or whose
        // request resolved, is taken back out of the notification centre.
        let stale: Vec<String> = self
            .needs_you
            .posted
            .iter()
            .filter(|(_, alert)| live(alert) == Some(false))
            .map(|(tag, _)| tag.clone())
            .collect();
        let (mut redraw, mut sound, mut posts) = (false, None, Vec::new());
        if self.needs_you.busy() {
            let present = self.cockpit.window_active();
            let on_board: std::collections::HashSet<ThreadId> = self
                .visible_indices()
                .into_iter()
                .filter_map(|index| self.panes[index].thread())
                .collect();
            let in_view = |alert: &crate::notifications::Alert| {
                present && alert.away.is_none() && on_board.contains(&alert.thread)
            };
            let ring = self.needs_you.ring(now, &live, &in_view);
            redraw = ring.redraw;
            // Alerts that ring together chime once, needs-you over done.
            let settings = &self.prefs.settings;
            sound = ring
                .rang
                .iter()
                .filter(|alert| {
                    self.alert_provider(alert, machines)
                        .is_some_and(|provider| settings.sounds_for(provider))
                })
                .map(|alert| {
                    if alert.needs_you() {
                        Sound::NeedsYou
                    } else {
                        Sound::Done
                    }
                })
                .max_by_key(|sound| *sound == Sound::NeedsYou);
            if settings.system_notifications {
                let now = ferrite_core::clock::system_time();
                let focused = self.focused_thread();
                posts = ring
                    .rang
                    .into_iter()
                    .filter(|alert| {
                        let focused = alert.away.is_none() && focused == Some(alert.thread);
                        crate::system_notifications::wanted(focused, present)
                    })
                    .filter_map(|alert| Some((self.system_posting(&alert, machines, now)?, alert)))
                    .collect();
            }
        }
        for tag in stale {
            self.needs_you.posted.remove(&tag);
            crate::system_notifications::withdraw(&tag, cx);
        }
        if let Some(sound) = sound {
            self.chime(sound);
        }
        for (posting, alert) in posts {
            self.needs_you.posted.insert(posting.tag.clone(), alert);
            crate::system_notifications::post(posting, cx);
        }
        if redraw {
            cx.notify();
        }
    }

    /// The provider an alert's Thread runs on, as far as its machine says.
    fn alert_provider(
        &self,
        alert: &crate::notifications::Alert,
        machines: Option<&crate::machines::Machines>,
    ) -> Option<Provider> {
        match &alert.away {
            None => self
                .cockpit
                .thread(alert.thread)
                .map(|open| open.provider()),
            Some(key) => machines?.away(key)?.status.provider(alert.thread),
        }
    }

    /// One alert's system notification, worded as its row and tagged with
    /// its Thread so a newer one replaces it, while its record is there to
    /// word it.
    fn system_posting(
        &self,
        alert: &crate::notifications::Alert,
        machines: Option<&crate::machines::Machines>,
        now: std::time::SystemTime,
    ) -> Option<crate::system_notifications::Posting> {
        let row = self.alert_row(alert, machines, now)?;
        let machine = alert.away.clone().unwrap_or_else(|| {
            machines.map_or(crate::machines::MachineKey(None), |machines| {
                machines.selected_key()
            })
        });
        // The directory is this computer's only for a Thread on screen.
        let cwd = match alert.away {
            None => self
                .cockpit
                .thread(alert.thread)
                .and_then(|open| open.workspace())
                .map(|binding| binding.cwd().to_path_buf()),
            Some(_) => None,
        };
        Some(crate::system_notifications::Posting {
            tag: system_tag(&machine, alert.thread),
            needs_you: alert.needs_you(),
            word: row.word(),
            title: row.title.to_string(),
            detail: row.detail.text(),
            agent: self
                .alert_provider(alert, machines)
                .map(|provider| provider.to_string()),
            cwd,
        })
    }

    /// Play a chime: the operator's own file for it, else the built-in.
    /// Tests and captures keep quiet; a test reads what would have played.
    fn chime(&mut self, sound: Sound) {
        if cfg!(any(test, feature = "visual-reference")) {
            #[cfg(test)]
            self.chimed.push(sound);
            return;
        }
        ferrite_core::sound::play(sound, self.prefs.settings.sound_file(sound));
    }

    /// The operator clicked a system notification: the app comes forward
    /// (the click handler's doing) and lands on what it was about — on its
    /// machine, when that is off screen.
    pub(crate) fn open_system_notification(&mut self, tag: &str, cx: &mut Context<Self>) {
        // Its entry stays: read now, the next pump takes it out of the
        // notification centre with the rest.
        if let Some(alert) = self.needs_you.posted.get(tag).cloned() {
            self.notice_verb(alert.open_verb(), cx);
        }
    }

    /// The row a toast or a system notification shows for `alert`, while
    /// its record is there to word it.
    fn alert_row(
        &self,
        alert: &crate::notifications::Alert,
        machines: Option<&crate::machines::Machines>,
        now: std::time::SystemTime,
    ) -> Option<NoticeRow> {
        if let Some(key) = &alert.away {
            return machines::away_row(&machines?.away(key)?, &alert.target, now);
        }
        let notifications = self.cockpit.notifications();
        Some(match &alert.target {
            RowTarget::Notice(id) => self.notice_row(notifications.get(*id)?, now),
            RowTarget::Decision(id) => self.decision_row(notifications.decision(id)?, now),
        })
    }

    /// A live request in the list's words (R7, `request_detail`); its age
    /// from when it was raised.
    fn decision_row(
        &self,
        notice: &ferrite_core::notifications::DecisionNotice,
        now: std::time::SystemTime,
    ) -> NoticeRow {
        use crate::notifications::{Detail, State};
        let thread = notice.id.thread;
        let open = self.cockpit.thread(thread);
        let detail = open
            .and_then(|open| {
                open.pending_decisions()
                    .iter()
                    .find(|request| request.handle == notice.id.handle)
                    .map(|request| request.decision.clone())
            })
            .map(|decision| {
                // Codex's patch request names no file; its gated call does.
                let gated_path =
                    || {
                        open?.transcript().blocks().iter().rev().find_map(|block| {
                            match &block.body {
                                ferrite_core::transcript::Body::Tool(call)
                                    if !decision.tool_use_id.is_empty()
                                        && ferrite_pane::transcript::gated(
                                            &call.call,
                                            &decision.tool_use_id,
                                        ) =>
                                {
                                    call.diffs
                                        .first()
                                        .map(|diff| diff.path.clone())
                                        .or_else(|| {
                                            (!call.summary.is_empty()).then(|| call.summary.clone())
                                        })
                                }
                                _ => None,
                            }
                        })
                    };
                let cwd = open
                    .and_then(|open| open.workspace())
                    .map(|binding| binding.cwd());
                crate::notifications::request_detail(&decision, notice.kind, cwd, gated_path)
            })
            .unwrap_or(Detail::None);
        NoticeRow {
            away: None,
            target: crate::notifications::RowTarget::Decision(notice.id.clone()),
            thread,
            title: self.facts.name(thread),
            state: State::NeedsYou(notice.kind),
            detail,
            when: crate::facts::since_label(notice.at, now),
            read: notice.read,
            folded: Vec::new(),
        }
    }

    /// The list's rows (FL-16): the live requests first, in the order the
    /// answer keys take them (the Needs-you queue), then the machines off
    /// screen's, then finished turns newest first on every machine, each
    /// Thread folded to its newest.
    pub(super) fn needs_you_rows(&self, cx: &gpui::App) -> Vec<NoticeRow> {
        let now = ferrite_core::clock::system_time();
        let notifications = self.cockpit.notifications();
        let queue = self.cockpit.needs_you();
        let mut rows: Vec<NoticeRow> = notifications
            .decisions()
            .take(50)
            .map(|notice| self.decision_row(notice, now))
            .collect();
        rows.sort_by_key(|row| {
            queue
                .iter()
                .position(|thread| *thread == row.thread)
                .unwrap_or(usize::MAX)
        });
        let (away_requests, away_finished) = self.away_rows(cx);
        rows.extend(away_requests);
        let mut finished: Vec<_> = notifications
            .notices()
            .take(50)
            .map(|notice| (notice.at, self.notice_row(notice, now)))
            .chain(away_finished)
            .collect();
        finished.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        rows.extend(crate::notifications::fold(
            finished.into_iter().map(|(_, row)| row).collect(),
        ));
        rows
    }

    /// The nav's content at its own width (`nav::content`, §5.1): the head
    /// in the head row, the views, the Attention list and the tree — or the
    /// shown view's list in their place — with its scrollbar, and the foot.
    /// The Attention list is built whichever is shown, so what it says it
    /// last drew stays true.
    fn nav_content(&self, state: &nav::NavState, window: &Window, cx: &mut Context<Self>) -> Div {
        #[cfg(test)]
        drawn::built_nav();
        let attention = self.attention_list(state);
        let list = match self.nav_view {
            nav::NavView::Tree => self.nav_tree(state, &attention, cx),
            nav::NavView::NeedsYou => self.needs_you_list(window, cx),
            nav::NavView::Parked => self.parked_list(state, cx),
        };
        let gear = nav::gear_button().on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.toggle_settings(cx);
        }));
        nav::content()
            .child(nav::head(
                crate::titlebar::head_drag(!self.overlay_open(), self.maximized),
                self.nav_toggle(false, cx),
            ))
            .child(self.nav_views(state, cx))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(list)
                    .child(nav::scrollbar(&self.nav_scroll, self.nav_hovered)),
            )
            .child(nav::foot(&self.shown_version, gear))
    }

    /// The sidebar toggle (§5.1.1): in the head, or at the rail's top.
    fn nav_toggle(&self, rail: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        nav::collapse_button(rail).on_mouse_down(
            MouseButton::Left,
            cx.listener(|view, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                view.toggle_nav_now(cx);
            }),
        )
    }

    /// The views (§5.1.2): `Search` (the palette, its chord read from the
    /// keymap), `Needs you` (its pill the blocked Threads), `Parked` (its
    /// count) and `Tasks` while Projects is live (ADR 0031: the board, its
    /// tally). A press shows the view in the tree's place, or the tree
    /// again from the active one.
    fn nav_views(&self, state: &nav::NavState, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::icons;
        use nav::{NavView, Trailing};
        let shown = self.nav_view;
        let mut rows: Vec<AnyElement> = Vec::new();
        let key = "nav-view-search";
        rows.push(
            nav::view_row(
                key,
                nav::view_icon(icons::SEARCH, key, false, false),
                "Search",
                ferrite_ui::components::bound_chord("palette::Toggle")
                    .map_or(Trailing::None, Trailing::Chord),
                false,
                false,
            )
            .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                window.dispatch_action(Box::new(crate::palette::Toggle), cx);
            }))
            .into_any_element(),
        );
        let waiting = self.cockpit.needs_you().len();
        let key = "nav-view-needs-you";
        let active = shown == NavView::NeedsYou;
        rows.push(
            nav::view_row(
                key,
                nav::view_icon(icons::INBOX, key, active, waiting > 0),
                "Needs you",
                Trailing::Pill(waiting),
                active,
                waiting > 0,
            )
            .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                view.toggle_notifications(&ToggleNotifications, window, cx);
            }))
            .into_any_element(),
        );
        let key = "nav-view-parked";
        let active = shown == NavView::Parked;
        rows.push(
            nav::view_row(
                key,
                nav::view_icon(icons::ARCHIVE, key, active, false),
                "Parked",
                Trailing::Count(state.parked.len()),
                active,
                false,
            )
            .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                view.toggle_nav_view(NavView::Parked, Some(window), cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_context_menu(MenuTarget::Parked, event.position, cx);
                }),
            )
            .into_any_element(),
        );
        if self.projects.live {
            let key = "nav-view-tasks";
            let active = self.projects.board_shown();
            rows.push(
                nav::view_row(
                    key,
                    nav::view_icon(icons::BOARD, key, active, false),
                    "Tasks",
                    self.tasks_tally().map_or(Trailing::None, Trailing::Text),
                    active,
                    false,
                )
                .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    view.toggle_projects(window, cx);
                    cx.notify();
                }))
                .into_any_element(),
            );
        }
        nav::views(rows)
    }

    /// The Needs-you view's list in the tree's place (§5.1.2), scrolling as
    /// the tree does and holding its own keys (`needs_you_rows`, its cursor
    /// `needs_you.cursor` while it has the keyboard): the blocked Threads
    /// oldest-first as their two-line rows, a machine off screen's
    /// requests, then `Recent`, the Notices. A row's press opens what it
    /// stands for.
    fn needs_you_list(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let tree = nav::nav_tree(&self.nav_scroll);
        let rows = self.needs_you_rows(cx);
        let cursor = self
            .floats
            .notice_focus
            .is_focused(window)
            .then(|| self.needs_you.cursor.min(rows.len().saturating_sub(1)));
        let mut list = nav::view_list("nav-view-needs-you-list");
        if rows.is_empty() {
            list = list.child(nav::note("nav-needs-you-empty", "Nothing needs you"));
        }
        let mut recent = false;
        for (index, row) in rows.iter().enumerate() {
            let local_request = row.request() && row.away.is_none();
            if !row.request() && !recent {
                recent = true;
                list = list.child(self.recent_heading(cx));
            }
            let verb = row.open_verb();
            let open = cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.notice_verb(verb.clone(), cx);
            });
            let selected = cursor == Some(index);
            let element = if local_request {
                let thread = nav::ThreadRow {
                    selected,
                    ..self.thread_row(row.thread)
                };
                let drawn = nav::thread_row_with_title(
                    &thread,
                    thread.name.clone(),
                    nav::RowPlace::ROOT,
                    None,
                    false,
                    0,
                    &nav::Measure::Text(cx),
                );
                div()
                    .id(("notice-row", index))
                    .debug_selector(move || format!("notice-row-{index}"))
                    .flex_shrink_0()
                    .child(drawn)
                    .on_click(open)
                    .into_any_element()
            } else {
                nav::notice_row(index, row, selected, &nav::Measure::Text(cx))
                    .on_click(open)
                    .into_any_element()
            };
            list = list.child(element);
        }
        if !rows.is_empty() && !recent {
            list = list.child(self.recent_heading(cx));
        }
        tree.track_focus(&self.floats.notice_focus)
            .key_context("Notifications")
            .child(list)
    }

    /// The Parked view's list in the tree's place (§5.1.2), its Threads
    /// grouped as the tree groups them, Esc's way back to the tree.
    fn parked_list(&self, state: &nav::NavState, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut list = nav::view_list("nav-view-parked-list");
        if state.parked.is_empty() {
            list = list.child(nav::note("nav-parked-empty", "Nothing parked"));
        }
        let sections = self.view_tree(state.parked.clone());
        list = list.children(
            self.view_sections(&sections, cx, |view, row, place, ends, cx| {
                view.thread_element(row, None, place, ends, cx)
            }),
        );
        let list = if self.machine_lost() {
            machines::held(list, "machine-lost-parked")
        } else {
            list
        };
        nav::nav_tree(&self.nav_scroll)
            .track_focus(&self.nav_view_focus)
            .key_context("NavView")
            .child(list)
    }

    /// A view's rows grouped as the tree groups them (`nav::ViewSection`):
    /// under their Project, then under their home Group within it. A
    /// Group's fold is the tree's own.
    fn view_tree(&self, rows: Vec<nav::ThreadRow>) -> Vec<nav::ViewSection> {
        let groups = self.cockpit.groups();
        nav::ViewSection::tree(
            rows,
            |row| nav::ViewPlace {
                project: self.facts.get(row.thread).and_then(|facts| facts.project),
                group: groups.home(row.thread).and_then(|group| {
                    Some(nav::HomeGroup {
                        order: groups.iter().position(|other| other.id == group.id)?,
                        id: group.id,
                        title: group.display_title().into(),
                        folded: self.nav_folds.contains(&NavFold::Group(group.id)),
                    })
                }),
            },
            |project, rows| {
                let space = project.and_then(|_| {
                    self.projects
                        .sidebar
                        .heading(rows.iter().map(|row| row.thread))
                        .map(|space| SharedString::from(space.name.clone()))
                });
                self.section_label(project, space, rows.iter().copied())
            },
        )
    }

    /// What heads a Project section, in the tree or a view's list: its
    /// task space's name (ADR 0031), else the Project's title, else a row's
    /// cached Project name, lowercased. `other` names no Project.
    fn section_label<'a>(
        &self,
        key: Option<ProjectId>,
        space: Option<SharedString>,
        mut rows: impl Iterator<Item = &'a nav::ThreadRow>,
    ) -> SharedString {
        let Some(project) = key else {
            return SharedString::from("other");
        };
        space
            .or_else(|| {
                self.cockpit
                    .registry()
                    .project(project)
                    .map(|project| SharedString::from(project.title.to_lowercase()))
            })
            .or_else(|| {
                rows.find_map(|row| row.project.as_deref().map(str::to_lowercase))
                    .map(SharedString::from)
            })
            .unwrap_or_else(|| SharedString::from("other"))
    }

    /// A view's sections drawn as the tree draws its own: each a plain
    /// Project heading over its rows, one level in with the Project's
    /// guide, each Group's row (a press opens it, its chevron folds it as
    /// the tree's does) over its members, one level further with the
    /// Group's guide. `thread` draws a row (its place, and how many of its
    /// guides end at it) and wires its press, which differs by view.
    fn view_sections(
        &self,
        sections: &[nav::ViewSection],
        cx: &mut Context<Self>,
        thread: impl Fn(&Self, &nav::ThreadRow, nav::RowPlace, usize, &mut Context<Self>) -> AnyElement,
    ) -> Vec<AnyElement> {
        sections
            .iter()
            .enumerate()
            .map(|(index, section)| {
                let loose = section.solos.len();
                let mut rows: Vec<AnyElement> = section
                    .solos
                    .iter()
                    .enumerate()
                    .map(|(at, row)| {
                        let ends = usize::from(at + 1 == loose && section.groups.is_empty());
                        thread(self, row, nav::RowPlace::Row(1), ends, cx)
                    })
                    .collect();
                for (at, group) in section.groups.iter().enumerate() {
                    let id = group.id;
                    let last = at + 1 == section.groups.len();
                    let shut = group.folded || group.members.is_empty();
                    let head = nav::group_row_with_title(
                        group,
                        group.title.clone(),
                        self.group_fold(group, cx),
                        1,
                        usize::from(last && shut),
                        &nav::Measure::Text(cx),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                            view.enter_group(id, cx)
                        }),
                    );
                    let mut block = nav::group_block(id).child(head);
                    if !group.folded {
                        let count = group.members.len();
                        let members = group
                            .members
                            .iter()
                            .enumerate()
                            .map(|(at, row)| {
                                let ends = member_ends(last, at, count);
                                thread(self, row, nav::RowPlace::Row(2), ends, cx)
                            })
                            .collect();
                        block = block.child(nav::members(members));
                    }
                    rows.push(block.into_any_element());
                }
                nav::section(index == 0)
                    .child(nav::view_project(
                        index,
                        section.label.clone(),
                        &nav::Measure::Text(cx),
                    ))
                    .child(nav::project_rows(rows))
                    .into_any_element()
            })
            .collect()
    }

    /// The Needs-you list's `Recent` heading, with `Mark all read` at its
    /// right: every row read, none removed.
    fn recent_heading(&self, cx: &mut Context<Self>) -> Div {
        // A text control in the heading's ink, the `+`'s height and its
        // ground under the pointer and pressed, so it reads as one.
        nav::heading("nav-recent", "Recent").child(
            div()
                .id("notifications-mark-read")
                .debug_selector(|| "notifications-mark-read".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .h(px(ferrite_ui::theme::NAV_HEADING_BUTTON))
                .px(px(ferrite_ui::theme::SPACE_1))
                .rounded(px(ferrite_ui::theme::R_SMALL))
                .hover_row("notifications-mark-read")
                .press_row()
                .font_weight(ferrite_ui::theme::W_BODY)
                .child("Mark all read")
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.notice_verb(Verb::MarkAllRead, cx);
                })),
        )
    }

    /// The nav column (§5.1): the head, the views, the tree and the foot
    /// on the frame, from the window's top. cmd-B rides its width down to
    /// the rail over `motion::RESIZE` (the render tail keeps frames coming
    /// while it moves) while its content fades on its own clock (N-15): 1 →
    /// 0 over 150ms folding, 0 → 1 opening, the rail cross-fading the other
    /// way. At rest folded, the column is the rail. The rail and the
    /// uncached rows share one `nav_state` a frame.
    fn nav(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let collapsed = self.nav_railed();
        let now = cx.background_executor().now();
        let reduced = ferrite_ui::motion::reduced_motion(cx);
        // The width the board reads this frame (F-12): the ride's, or a
        // scene's held one.
        let ride = self.nav_ride_now(cx);
        let width = ride.unwrap_or(if collapsed {
            nav::FOLDED_WIDTH
        } else {
            nav::WIDTH
        });
        let frame = div().flex().flex_row().flex_shrink_0().h_full();
        let shell = nav::shell(width).child(nav::body());
        let content = if collapsed && ride.is_none() {
            0.0
        } else {
            self.nav_content_opacity(now, reduced)
        };
        let state = std::cell::OnceCell::new();
        let shell = shell
            .when(content > 0.0, |shell| {
                shell.child(self.nav_rows(content, &state, window, cx))
            })
            .when(content < 1.0, |shell| {
                let state = state.get_or_init(|| self.nav_state());
                shell.child(self.nav_rail(state, window, cx).opacity(1.0 - content))
            });
        frame.child(shell).into_any_element()
    }

    /// The rail (cmd-B folded): every open Thread in the tree's order —
    /// loose rows, then Group members, folds ignored, each Thread once —
    /// with its mark, and the digit of the ⌘1–9 that focuses it where one
    /// does (the board's head order, R5). A press lands on the Thread as
    /// its tree row's does.
    fn nav_rail(
        &self,
        state: &nav::NavState,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let keyed: Vec<Option<ThreadId>> = self
            .board_order(window)
            .into_iter()
            .take(9)
            .map(|index| self.panes[index].thread())
            .collect();
        let focused = self.cockpit.roster().focused_thread();
        let mut rows: Vec<nav::RailRow> = Vec::new();
        let mut listed = std::collections::HashSet::new();
        for section in &state.sections {
            let solos = section
                .solos
                .iter()
                .map(|index| (&state.solos[*index], false));
            let members = section.groups.iter().flat_map(|index| {
                let group = &state.groups[*index];
                group.members.iter().map(|row| (row, group.selected))
            });
            for (row, group_shown) in solos.chain(members) {
                if listed.insert(row.thread) {
                    rows.push(nav::RailRow {
                        key: keyed
                            .iter()
                            .position(|shown| *shown == Some(row.thread))
                            .map(|at| at + 1),
                        thread: row.thread,
                        status: row.status,
                        selected: row.selected || (group_shown && focused == Some(row.thread)),
                    });
                }
            }
        }
        let mut rail = nav::rail(&self.nav_rail_scroll, self.nav_toggle(true, cx));
        for row in &rows {
            let thread = row.thread;
            rail = rail.child(nav::rail_row(row).on_click(
                cx.listener(move |view, _: &ClickEvent, _, cx| view.land_on_thread(thread, cx)),
            ));
        }
        rail
    }

    /// The nav content's opacity this frame (N-15): wherever its own fade
    /// has it while the column rides — a fade that has finished holds its
    /// end — and fully shown at rest.
    fn nav_content_opacity(&self, now: std::time::Instant, reduced: bool) -> f32 {
        // A scene holding the ride (`fixture_hold_nav`) holds the fade where
        // its own clock would be that far into the ride.
        if let Some(progress) = self.nav_hold {
            let elapsed = ferrite_ui::motion::RESIZE
                .duration()
                .mul_f32(progress.clamp(0.0, 1.0));
            let faded = nav::CONTENT_FADE.progress(
                (elapsed.as_secs_f32() / nav::CONTENT_FADE.duration().as_secs_f32()).min(1.0),
            );
            return if self.nav_railed() {
                1.0 - faded
            } else {
                faded
            };
        }
        match (self.nav_tween, self.nav_fade) {
            (Some(ride), Some(fade)) if ride.running(now, reduced) => fade.value(now, reduced),
            _ => 1.0,
        }
    }

    /// The scrolling tree: the Attention list over it (`attention_list`),
    /// then one section per Project (`nav_state`), each its heading, its
    /// loose rows — a `LooseZone`, a place to drop a row to get it out of
    /// its Group — and its Groups, then the ground, which is a `LooseZone`
    /// too.
    fn nav_tree(
        &self,
        state: &nav::NavState,
        attention: &nav::AttentionList,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        // Every drag started from this frame started from this View.
        let origin = self.cockpit.roster().view();
        let mut tree = nav::nav_tree(&self.nav_scroll);
        if let Some(error) = &self.group_error {
            tree = tree.child(nav::notice(error.clone()));
        }
        if !attention.sections.is_empty() {
            tree = tree.child(self.attention_element(attention, cx));
        }
        // With machines (ADR 0026): the one the window is on heads its own
        // tree, which folds under it, and is held while its Server is away;
        // the others follow the tree.
        let machines = self.machine_rows(cx);
        let on = machines
            .as_ref()
            .and_then(|rows| rows.iter().find(|row| row.selected));
        if let Some(on) = on {
            tree = tree.child(self.machine_heading(on, cx));
        }
        let folded = on.is_some_and(|on| on.folded);
        let mut own: Vec<AnyElement> = Vec::new();
        let mut zones = 0usize;
        // `Projects` heads the Project sections, under the Groups section
        // (Groups spanning Projects) where there is one.
        let projects_at = state
            .sections
            .iter()
            .position(|section| !section.mixed)
            .unwrap_or(state.sections.len());
        let mut first = true;
        for (index, section) in state.sections.iter().enumerate().filter(|_| !folded) {
            if index == projects_at {
                own.push(self.projects_heading(cx).into_any_element());
                first = true;
            }
            let mut block = nav::section(first).child(self.project_heading(index, section, cx));
            first = false;
            block = block.children(section.numbers.clone().map(nav::space_numbers));
            if !section.folded {
                // A Project's rows sit one level under its heading; the
                // Groups section has no heading of its own to hang from.
                let depth = usize::from(!section.mixed);
                let mut rows: Vec<AnyElement> = Vec::new();
                if !section.solos.is_empty() {
                    let loose = section.solos.len();
                    let solos = section
                        .solos
                        .iter()
                        .enumerate()
                        .map(|(at, row)| {
                            let last = at + 1 == loose && section.groups.is_empty();
                            self.thread_element(
                                &state.solos[*row],
                                None,
                                nav::RowPlace::Row(depth),
                                usize::from(last),
                                cx,
                            )
                        })
                        .collect();
                    rows.push(self.loose_run(zones, solos, cx));
                    zones += 1;
                }
                for (at, group) in section.groups.iter().enumerate() {
                    let last = at + 1 == section.groups.len();
                    rows.push(self.group_element(&state.groups[*group], depth, last, origin, cx));
                }
                block = if section.mixed {
                    block.children(rows)
                } else {
                    block.child(nav::project_rows(rows))
                };
            }
            own.push(block.into_any_element());
        }
        if projects_at == state.sections.len() && !folded {
            own.push(self.projects_heading(cx).into_any_element());
        }
        // An empty tree says so at its top, before the ground. It names the
        // Project the filter chose.
        if state.sections.is_empty() && !folded {
            let filtered = self
                .nav_filter
                .and_then(|project| self.cockpit.registry().project(project))
                .map(|project| project.title.clone());
            own.push(
                nav::empty_filter(filtered.as_deref(), !state.parked.is_empty()).into_any_element(),
            );
        }
        tree = if self.machine_lost() {
            tree.child(machines::held(
                div().flex().flex_col().flex_shrink_0().children(own),
                "machine-lost-tree",
            ))
        } else {
            tree.children(own)
        };
        if let Some(rows) = &machines {
            tree = tree.children(self.machines_off_screen(rows, cx));
        }
        // The ground under the last row is a drop target too — with every
        // Thread in a Group it is the only place left to drop one to get it
        // out — and it takes the tree's slack, so that ground is never dead.
        tree.child(
            drop_feedback(
                nav::loose_ground(zones),
                self.cockpit.groups().clone(),
                DropTarget::LooseZone,
            )
            .on_drop(cx.listener(|view, drag: &NavDrag, _, cx| {
                view.apply_drop(*drag, DropTarget::LooseZone, cx)
            })),
        )
    }

    /// The Attention list (§5.1.2): each section its head — a press folds
    /// it to the head and its count — and its one-line rows, each wired as
    /// its tree row is (`thread_element`). A row arrives and leaves at
    /// once: the list is the nav's, and a fade would rebuild it a frame at
    /// a time.
    fn attention_element(&self, attention: &nav::AttentionList, cx: &mut Context<Self>) -> Div {
        let sections = attention
            .sections
            .iter()
            .map(|block| {
                let (section, folded) = (block.section, block.folded);
                let head = nav::list_heading(section, block.rows.len()).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        view.set_nav_fold(NavFold::List(section), !folded, cx);
                    }),
                );
                let rows = block
                    .rows
                    .iter()
                    .filter(|_| !folded)
                    .map(|row| self.thread_element(row, None, nav::RowPlace::Line, 0, cx));
                div()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .child(head)
                    .children(rows)
                    .into_any_element()
            })
            .collect();
        nav::attention_list(sections)
    }

    /// The `Projects` heading (§5.1.3), its `+` a new Project.
    fn projects_heading(&self, cx: &mut Context<Self>) -> Div {
        nav::heading("nav-projects-heading", "Projects").child(
            nav::heading_button("nav-new-project", ferrite_ui::icons::PLUS)
                .tooltip(ferrite_ui::menu::tooltip("New project"))
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_project_creator(cx);
                })),
        )
    }

    /// A Project heading (N-12): a plain row whose press folds or unfolds
    /// the section (N-4), and whose right press opens its menu (its tasks,
    /// where a board opens). `Other` names no Project and does neither.
    /// The Groups section's is the `Groups` heading, its press the fold.
    fn project_heading(
        &self,
        index: usize,
        section: &nav::ProjectSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if section.mixed {
            let folded = section.folded;
            return div()
                .id(("nav-project-section", index))
                .child(nav::heading("nav-groups-heading", "Groups"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        view.set_nav_fold(NavFold::Groups, !folded, cx);
                    }),
                )
                .into_any_element();
        }
        let heading = nav::project_section(index, section, &nav::Measure::Text(cx));
        let fold = match section.project {
            _ if section.mixed => NavFold::Groups,
            Some(project) => NavFold::Project(project),
            None => return heading.into_any_element(),
        };
        let folded = section.folded;
        heading
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    view.set_nav_fold(fold, !folded, cx);
                }),
            )
            .when_some(
                section
                    .project
                    .filter(|_| !section.mixed && self.projects.live),
                |heading, project| {
                    heading.on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.open_context_menu(
                                MenuTarget::Project(project),
                                event.position,
                                cx,
                            );
                        }),
                    )
                },
            )
            .into_any_element()
    }

    /// One section's run of loose rows, which is also a `LooseZone`.
    fn loose_run(&self, zone: usize, rows: Vec<AnyElement>, cx: &mut Context<Self>) -> AnyElement {
        drop_feedback(
            nav::solos(zone, rows),
            self.cockpit.groups().clone(),
            DropTarget::LooseZone,
        )
        .on_drop(cx.listener(|view, drag: &NavDrag, _, cx| {
            view.apply_drop(*drag, DropTarget::LooseZone, cx)
        }))
        .into_any_element()
    }

    /// One Group block: the header — the drag handle, the rename target and
    /// the way in, its triangle the fold (N-4) — then, unfolded, the member
    /// rows; and over the header's top edge the "insert before this Group"
    /// drop band, laid last so it is the topmost target there. Its row is
    /// `depth` levels into the tree, its members one further; `last` says
    /// it closes its section, so the section's guide ends at its last row.
    fn group_element(
        &self,
        group: &nav::GroupBlock,
        depth: usize,
        last: bool,
        origin: View,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = group.id;
        let Some(full_index) = self
            .cockpit
            .groups()
            .iter()
            .position(|group| group.id == id)
        else {
            return div().into_any_element();
        };
        let gap = DropTarget::GroupGap(full_index);
        let folded = group.folded;
        let fold = self.group_fold(group, cx);
        let title = self.editable_group_title(id, group.title.clone(), cx);
        let shut = folded || group.members.is_empty();
        let head = nav::group_row_with_title(
            group,
            title,
            fold,
            depth,
            usize::from(last && shut),
            &nav::Measure::Text(cx),
        );
        let chip = NavChip::Group(group.clone());
        let lifted = self.nav_dragging == Some(Drag::Group(id));
        let mut block = nav::group_block(id)
            .when(lifted, |block| {
                block.opacity(ferrite_ui::theme::DRAG_SOURCE_OPACITY)
            })
            .child(
                drop_feedback(
                    head,
                    self.cockpit.groups().clone(),
                    DropTarget::GroupHeader(id),
                )
                .on_drag(
                    NavDrag {
                        drag: Drag::Group(id),
                        origin,
                    },
                    move |_, _, _, cx| {
                        let chip = chip.clone();
                        cx.new(|_| NavDragPreview(chip))
                    },
                )
                .on_drop(cx.listener(move |view, drag: &NavDrag, _, cx| {
                    view.apply_drop(*drag, DropTarget::GroupHeader(id), cx)
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| view.enter_group(id, cx)),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.open_context_menu(MenuTarget::Group(id), event.position, cx);
                    }),
                ),
            );
        if !folded && !group.members.is_empty() {
            // The shown Group's members are one-line rows, the focused
            // Pane's title strong (§5.1.6); any other Group's are two-line
            // rows, nested under it.
            let focused = self.cockpit.roster().focused_thread();
            let count = group.members.len();
            let rows = group
                .members
                .iter()
                .enumerate()
                .map(|(at, row)| {
                    let ends = member_ends(last, at, count);
                    if group.selected {
                        let member = nav::ThreadRow {
                            selected: focused == Some(row.thread),
                            ..row.clone()
                        };
                        let place = nav::RowPlace::Member(depth);
                        self.thread_element(&member, Some(id), place, ends, cx)
                    } else {
                        let place = nav::RowPlace::Row(depth + 1);
                        self.thread_element(row, Some(id), place, ends, cx)
                    }
                })
                .collect();
            let mut members = nav::members(rows);
            // Appending to the Group means dropping past its last row —
            // an index one beyond the end, aimed at the last member.
            if let Some(last) = group.members.last().map(|row| row.thread) {
                let tail = DropTarget::ThreadRow {
                    thread: last,
                    group: Some(id),
                    index: self
                        .cockpit
                        .groups()
                        .get(id)
                        .map_or(0, |group| group.members.len()),
                };
                members = members.child(
                    drop_feedback(nav::member_tail(id), self.cockpit.groups().clone(), tail)
                        .on_drop(cx.listener(move |view, drag: &NavDrag, _, cx| {
                            view.apply_drop(*drag, tail, cx)
                        })),
                );
            }
            block = block.child(members);
        }
        block
            .child(
                drop_feedback(
                    nav::group_gap(full_index),
                    self.cockpit.groups().clone(),
                    gap,
                )
                .on_drop(
                    cx.listener(move |view, drag: &NavDrag, _, cx| view.apply_drop(*drag, gap, cx)),
                ),
            )
            .into_any_element()
    }

    /// A Group row's chevron, wired: the triangle folds and stops there, so
    /// the rest of the row opens the Group, and opening a folded Group
    /// leaves it folded. The tree and the views share the fold.
    fn group_fold(&self, group: &nav::GroupBlock, cx: &mut Context<Self>) -> Stateful<Div> {
        let (id, folded) = (group.id, group.folded);
        nav::group_fold(group).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                view.set_nav_fold(NavFold::Group(id), !folded, cx);
            }),
        )
    }

    /// One Thread row, with the whole drag/drop and click wiring a row has
    /// had since #21. A press on a member row opens its Group (`group`, the
    /// Group it is drawn under) on that Thread; a loose row whose Pane is
    /// open focuses it; a parked one revives. An Attention list row
    /// (`RowPlace::Line`) stands for its tree row: it drags and drops as
    /// that row does, from its home Group, and a press lands on the Thread
    /// (`land_on_thread`); it is renamed from the tree.
    fn thread_element(
        &self,
        row: &nav::ThreadRow,
        group: Option<GroupId>,
        place: nav::RowPlace,
        ends: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let thread = row.thread;
        let line = place == nav::RowPlace::Line;
        let group = if line {
            self.cockpit.groups().home(thread).map(|group| group.id)
        } else {
            group
        };
        // A Project filter hides rows, not members. Drop positions always
        // address the durable order, including the hidden members.
        let index = group
            .and_then(|id| self.cockpit.groups().get(id))
            .and_then(|group| group.members.iter().position(|member| *member == thread))
            .unwrap_or(0);
        let open = self.pane_for(thread).is_some();
        let origin = self.cockpit.roster().view();
        let target = DropTarget::ThreadRow {
            thread,
            group,
            index,
        };
        let title = if line {
            row.name.clone().into_any_element()
        } else {
            self.editable_thread_title(thread, row.name.clone(), cx)
        };
        let editing = !line
            && matches!(
                &self.rename,
                Some((RenameTarget::Thread(editing), _)) if *editing == thread
            );
        let head = nav::thread_row_with_title(
            row,
            title,
            place,
            group,
            editing,
            ends,
            &nav::Measure::Text(cx),
        );
        let face = self.thread_ghost(thread);
        let nav_edge = self.nav_width();
        let chip = NavChip::Thread(row.clone());
        let lifted = matches!(
            self.nav_dragging,
            Some(Drag::Thread { thread: dragged, .. }) if dragged == thread
        );
        drop_feedback(head, self.cockpit.groups().clone(), target)
            // The row being dragged stays in the tree, dimmed, while its
            // copy rides the pointer.
            .when(lifted, |row| {
                row.opacity(ferrite_ui::theme::DRAG_SOURCE_OPACITY)
            })
            .on_drag(
                NavDrag {
                    drag: Drag::Thread { thread, group },
                    origin,
                },
                move |_, grab, _, cx| {
                    let face = face.clone();
                    let chip = chip.clone();
                    cx.new(|_| PaneGhost {
                        face,
                        grab,
                        row_left_of: Some((nav_edge, chip)),
                    })
                },
            )
            .on_drop(
                cx.listener(move |view, drag: &NavDrag, _, cx| view.apply_drop(*drag, target, cx)),
            )
            // A click, not a press: pressing a row to drag it onto the
            // board must leave the board showing, not switch to the row's
            // own view before the drag begins.
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                if line {
                    view.land_on_thread(thread, cx);
                    return;
                }
                if let Some(group) = group {
                    view.enter_group(group, cx);
                    view.focus_thread(thread, cx);
                    return;
                }
                if open {
                    view.focus_thread(thread, cx);
                    return;
                }
                view.revive_thread(thread, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_context_menu(MenuTarget::Thread(thread), event.position, cx);
                }),
            )
            .into_any_element()
    }
}

/// How many of a nav member row's guides end at it (`nav::guides`),
/// innermost first: none above its Group's last member; there its Group's,
/// and its section's too when its Group is the section's last (`last`).
/// The row clamps the count to the levels it has (a Group at the root has
/// no section guide).
fn member_ends(last: bool, at: usize, count: usize) -> usize {
    if at + 1 == count {
        1 + usize::from(last)
    } else {
        0
    }
}

/// What a Project heading says of its rows' checkouts
/// (`CockpitView::section_checkout`): the shared branch and whether it is
/// the default, and its drift (ahead, behind) when that is shared too.
type SectionCheckout = (Option<(SharedString, bool)>, Option<(u32, u32)>);

/// A typed path with `~` spelled out — the type-a-path row accepts what an
/// operator would type at a shell.
/// A slot's leaf in a board's tree. A Thread's Pane is its own id; drafts
/// and reader slots are no Threads and never persist, so stand-in ids
/// nothing real reaches name them: a draft counts down from the top, and a
/// reader slot is its Thread's number under the `READER_LEAF` bit.
const READER_LEAF: u64 = 1 << 62;

/// A Review's share of its Pane's slot when it first opens beside it.
const READER_SHARE: f32 = ferrite_ui::theme::READER_SHARE;

/// The approval a wall tile's digit gives (R9): `1` allow, `2` allow for
/// this thread, `3` deny and steer.
fn approval_choice(option: usize) -> Option<decision::ApprovalChoice> {
    match option {
        0 => Some(decision::ApprovalChoice::Allow),
        1 => Some(decision::ApprovalChoice::AllowForThread),
        2 => Some(decision::ApprovalChoice::DenyAndSteer),
        _ => None,
    }
}

fn pane_leaf(identity: PaneIdentity) -> ThreadId {
    match identity {
        PaneIdentity::Thread(thread) => thread,
        PaneIdentity::Draft(draft) => ThreadId::new(u64::MAX - draft.get()),
    }
}

fn reader_leaf(owner: ThreadId) -> ThreadId {
    ThreadId::new(READER_LEAF | owner.get())
}

fn leaf_slot(leaf: ThreadId) -> Slot {
    let raw = leaf.get();
    if raw > u64::MAX / 2 {
        return Slot::Pane(PaneIdentity::Draft(ferrite_core::roster::DraftId::new(
            u64::MAX - raw,
        )));
    }
    if raw & READER_LEAF == 0 {
        return Slot::Pane(PaneIdentity::Thread(leaf));
    }
    Slot::Reader(ThreadId::new(raw & !READER_LEAF))
}

/// A drag move's pointer and the hovered slot's bounds, in the tree's units.
fn drop_geometry(
    position: gpui::Point<Pixels>,
    bounds: gpui::Bounds<Pixels>,
) -> (layout::Point, layout::Rect) {
    (
        layout::Point {
            x: f32::from(position.x),
            y: f32::from(position.y),
        },
        layout::Rect {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            w: f32::from(bounds.size.width),
            h: f32::from(bounds.size.height),
        },
    )
}

/// A slot's head as its drag handle: the whole band picks the slot up,
/// with its ghost riding the pointer.
fn head_drag(leaf: ThreadId, face: GhostFace) -> pane::HeadDrag {
    Box::new(move |head: Div| {
        head.id(("slot-head", leaf.get() as usize))
            .cursor(gpui::CursorStyle::OpenHand)
            .on_drag(PaneDrag { leaf }, move |_, grab, _, cx| {
                let face = face.clone();
                cx.new(|_| PaneGhost {
                    face,
                    grab,
                    row_left_of: None,
                })
            })
            .into_any_element()
    })
}

/// The copy of a Provider's CLI Ferrite runs — the newest it found — as
/// "version · path", or where it looked when none answered. The About
/// section never guesses.
fn cli_version(provider: Provider) -> String {
    match ferrite_core::providers::discover::located(provider) {
        Some(found) => format!("{} · {}", found.version, found.path.display()),
        None => "not found on PATH or in the usual install directories".to_string(),
    }
}

/// Where Ferrite was started. Demo fixtures use this as their checkout;
/// production startup must not register it as a Project implicitly.
pub(crate) fn here() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| ".".into())
}

/// The Project a launch's draft starts on: where the operator left off.
/// The most recently used Thread's Project is the honest answer — it is
/// the work they were actually doing — and the newest registered Project
/// is the fallback for a store whose Threads name nothing the registry
/// still knows.
fn startup_project(cockpit: &ferrite_core::cockpit::Cockpit) -> Option<ProjectId> {
    cockpit.last_worked_project().or_else(|| {
        cockpit
            .registry()
            .projects()
            .last()
            .map(|project| project.id)
    })
}

/// This process's resident memory, for the perf print.
fn rss_mb() -> f64 {
    ferrite_core::spawner::rss_bytes(std::process::id())
        .map(|bytes| bytes as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests;
