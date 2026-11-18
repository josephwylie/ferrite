//! The cockpit window: every open Pane at once, and the one pump behind them.
//!
//! Rendering and keys only. What each Pane shows — the Blocks, the pending
//! Decision, the held prompt — is folded in core and read from there.

mod agent_pane;
pub(crate) mod archive;
mod attachments;
mod background_dialog_view;
mod background_work;
pub(crate) mod beside;
mod browser;
#[cfg(feature = "browser-smoke")]
mod browser_smoke;
mod columns;
mod context_menu;
pub(crate) mod coordinator;
pub(crate) mod decisions;
mod delivery;
mod destination;
mod details;
mod direct_view;
pub(crate) mod direction;
mod drafts;
pub(crate) mod empty_board;
pub(crate) mod environment;
pub(crate) mod factory;
mod history;
mod leaving;
mod lightbox;
mod mode_menu;
mod model_picker;
mod machines;
mod connections;
mod nav_chrome;
mod nav_keys;
mod nav_list;
mod nav_pick;
mod nav_undo;
mod nav_view;
mod palette;
pub(crate) mod panel;
mod parts;
mod popups;
mod prs;
mod sends;
mod server;
mod new_task_view;
mod project_editor_view;
mod settings_view;
mod sheets;
mod side_chat;
mod snooze;
pub(crate) mod starts;
mod surfaces;
mod tab_scroll;
pub(crate) mod subagents;
pub(crate) mod tools;
mod transcript_glue;
pub(in crate::cockpit) use transcript_glue::Handoff;
mod updating;
mod user_commands;
mod view_head;
pub(crate) mod workflows;

/// Card triggers' bounds by key, recorded in prepaint.
type FloatTriggers =
    std::rc::Rc<std::cell::RefCell<std::collections::HashMap<SharedString, gpui::Bounds<Pixels>>>>;

use self::context_menu::{MenuTarget, MenuVerb};
use self::popups::Popup;
use self::model_picker::{Column, ModelPicker};
use ferrite_ui::components::home_relative;
use ferrite_ui::components::Tip as _;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "visual-reference")]
#[path = "visual_reference.rs"]
pub mod visual_reference;
#[cfg(feature = "headless")]
pub mod changes_frames;

use std::time::Duration;

use ferrite_core::cockpit::{Cockpit, HistoryDirection, ProviderChoice, ThreadView};
use ferrite_core::draft::DraftTarget;
use ferrite_core::followup;
use ferrite_core::layout::{self, Edge, SeamId, Tree, Zone};
use ferrite_core::roster::PaneIdentity;
use ferrite_core::sound::Sound;
use ferrite_pane::page::PageSource;
use ferrite_pane::transcript::Page;

/// The action ⌘`ordinal` is bound to: the sidebar's `ordinal`th open Thread
/// (`CockpitView::thread_order`).
pub(crate) fn focus_thread_action(ordinal: usize) -> Option<&'static str> {
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
    Context, Div, Entity, EventEmitter, FocusHandle, Focusable, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollHandle, SharedString, Stateful, Window,
};

use crate::facts::Facts;
use crate::nav;
use crate::notifications::{Handle, NeedsYou, Row as NoticeRow, RowTarget, Verb};
use ferrite_pane::composer::{Composer, Edited, OpenSource};
use ferrite_pane::link_target::{LinkTarget, Open};
use ferrite_pane::decision;
use ferrite_pane::pane::{self, PaneView};
use ferrite_ui::menu;
use crate::project_editor;
use ferrite_pane::select::TranscriptText;
use ferrite_ui::frame::settings;
use ferrite_ui::pointer::{Pointer, PointerPressed};
pub use history::{GoBack, GoForward};
pub use tools::{GoToFile, ToggleChanges, ToggleFileTree, ToggleFiles, ToggleTerminal};

actions!(
    cockpit,
    [
        Submit,
        Interrupt,
        Allow,
        Deny,
        Always,
        // The approval card's chorded buttons: they answer whatever is on
        // the line.
        AllowOnce,
        AlwaysAllow,
        DenyOnce,
        // ⌘U: the `+` menu's `Add files or photos`, on the focused Pane.
        AttachFiles,
        // ⌘D: the next Thread waiting on the operator.
        NextDecision,
        NextThread,
        PreviousThread,
        NewThread,
        NewWorktreeThread,
        TextLarger,
        TextSmaller,
        TextReset,
        BandCycle,
        // F6 and ⇧F6: the next and previous area.
        FocusNextArea,
        FocusPreviousArea,
        // A Panel control's roving row (`panel/controls.rs`).
        PanelControlPrevious,
        PanelControlNext,
        PanelControlFirst,
        PanelControlLast,
        PanelControlClose,
        PanelControlMenu,
        ToolCyclePrevious,
        ToggleTool,
        StopAgent,
        CloseThread,
        ReopenThread,
        CopySelection,
        CiteSelection,
        Paste,
        PickOption1,
        PickOption2,
        PickOption3,
        PickOption4,
        OpenSettings,
        ToggleNav,
        TogglePanel,
        ToggleDetails,
        CloseDetails,
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
        // PageUp, PageDown, Home, End: the focused Pane's transcript, when
        // no text needs the keys.
        TranscriptPageUp,
        TranscriptPageDown,
        TranscriptTop,
        TranscriptBottom,
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
    operators: &dyn Fn(ThreadId) -> bool,
) -> (Vec<FreshNotice>, Vec<LiveRequest>) {
    // An owned Thread's Notices and requests ring nothing: its owner
    // (a Task, a Factory) tells the operator in its own way (ADR 0037).
    // Its Notices still move the watermark, so they are never news later.
    let fresh = notifications
        .since(after)
        .map(|notice| {
            (
                notice.id,
                notice.thread,
                notice.read || !operators(notice.thread),
            )
        })
        .collect();
    let requests = notifications
        .decisions()
        .filter(|notice| operators(notice.id.thread))
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

/// Whether a turn runs on this Thread for the stop affordances: from the
/// moment a prompt is sent — before the provider's first event, which
/// `busy` waits for — through a starting Session to the turn's end. The
/// Composer reads it (`pane::primary_actions`), so a way to stop is in view
/// from the first frame.
fn turn_running(open: &ThreadView) -> bool {
    open.busy() || open.activity().main_operator_turn()
}

/// The id of the latest queued prompt while it can still be taken back:
/// what ↑ and ⌫ act on (the strip's rows each act on their own).
fn queued_to_take_back(open: &ThreadView) -> Option<String> {
    open.queue()
        .first()
        .filter(|item| pane::queued_takes_back(item))
        .map(|item| item.id.clone())
}

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
    pub(crate) cockpit: Cockpit,
    browsers: browser::Browsers,
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
    /// When the frame's own clock text (a notice's age) next turns over,
    /// as of the last draw.
    frame_turn_at: Option<std::time::SystemTime>,
    /// Free bytes on the volume holding the Threads' checkouts, as last
    /// read (the nav foot's `N GB free`); `None` until read, or when unknown.
    free_disk: Option<u64>,
    /// When free disk was last asked for, on the executor's clock: at
    /// launch, then by the sweep once a minute (`read_free_disk`).
    free_disk_asked: Option<std::time::Instant>,
    /// The wall-clock minute the transcripts' message times (`52 minutes
    /// ago`) were last drawn in: the sweep redraws them as it turns over.
    message_minute: i64,
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
    /// Each Thread's side chat, the right pane's other body.
    side_chats: side_chat::SideChats,
    /// Drafts sent while a Session was starting, kept whole until it takes
    /// the prompt or hands it back.
    held_sends: sends::HeldSends,
    tools: tools::Tools,
    agent_drills: agent_pane::AgentDrills,
    native_copy: Option<String>,
    nav_collapsed: bool,
    nav_auto_fold: std::cell::Cell<bool>,
    nav_forced_open: bool,
    nav_measure_full: std::cell::Cell<bool>,
    /// The breadcrumb parent's room, as the last frame laid it out.
    head_parent_room: std::rc::Rc<std::cell::Cell<view_head::ParentRoom>>,
    /// What the nav and the Pane head say about a Thread beyond an O(1)
    /// read — checkout, Project, a parked row's provider, the Wall card —
    /// refreshed by moment, never per frame.
    facts: Facts,
    /// The one Project filter navigation has (#29), chosen from the ⌘K
    /// palette. `None` is every Project. It narrows the **navigation only**
    /// to that Project's section — which Panes the Cockpit flies is never
    /// touched by it.
    nav_filter: Option<ProjectId>,
    /// The nav, a view of its own (ADR 0041): it owns its list's motion,
    /// scroll, keyboard, search, picks and undo notice, and asks the
    /// Cockpit for the rest through `NavEvent`.
    nav_view: Entity<nav_view::NavView>,
    /// The Cockpit hearing the nav's events, from its first frame.
    nav_events: Option<gpui::Subscription>,
    /// The window's activation, followed into the Cockpit
    /// (`Cockpit::set_window_active`) once the first frame has a window.
    activation: Option<gpui::Subscription>,
    /// The one popover in the Composer's slot, or None: the `/`/`@` menu
    /// (#23), a picker (#11, #25) or a band chip (#29). Always on the
    /// focused Pane's Composer; render self-heals it shut when the operator
    /// leaves that Pane or its offer expires.
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
    snooze_editor: Option<snooze::Editor>,
    /// The one popup that is up: the context menu, the mode menu, or the
    /// plan, checks or session controls card (ADR 0043).
    popups: popups::Popups,
    /// The Solo view's right panel (`panel`): its tab per Thread and
    /// whether the window has room for it.
    panel: panel::Panel,
    /// "Open a surface": the Panel's default focus while no tab shows.
    panel_empty_focus: gpui::FocusHandle,
    /// The Panel bodies that take keys of their own (the launcher, Files):
    /// while focus is anywhere inside, the focus pump leaves it be.
    panel_keys: gpui::FocusHandle,
    /// The panel's column while it slides shut, so the keyboard inside it
    /// can be found and taken back (`columns::closing_holds_focus`).
    panel_closing_focus: gpui::FocusHandle,
    /// Back and forward through where the operator has been (`history`).
    history: history::History,
    /// The Composer's `+` menu, open on one Pane.
    attach_menu: Option<ferrite_core::roster::PaneIdentity>,
    /// Holds every footer's plan-usage popover open, its ring's own clicks
    /// aside: a visual-reference scene's (`usagepopover`), never the app's.
    usage_popover_held: bool,
    session_control_error: Option<(ThreadId, u64, String)>,
    /// The Threads the titler is naming right now: their cards draw the
    /// title at `NAV_CARD_RETITLING_OPACITY` until its answer lands.
    titling: std::collections::HashSet<ThreadId>,
    /// Each bar card trigger's bounds as last laid out (a draft's band
    /// chips), recorded in prepaint: what a bar card hangs from.
    float_triggers: FloatTriggers,
    /// The open Project menu came from the start hero's project word
    /// (`draft_hero`), so it drops below the word rather than standing
    /// over the strip's chip (`band_menu_element`).
    band_from_hero: bool,
    /// Frames each open card has waited for its trigger's or Pane's
    /// geometry, by trigger key (`card_corner`); none once it has some.
    card_waits: std::cell::RefCell<std::collections::HashMap<String, u8>>,
    /// A seam being dragged: the board, the seam, and the tree as it
    /// stands mid-drag — persisted on release, never per move.
    seam_drag: Option<SeamDrag>,
    /// The board's bounds as the last frame laid it out: what an act (a
    /// seam press, a drop) measures in when it has no window to measure.
    board: std::cell::Cell<layout::Rect>,
    /// A reader being dragged over a board slot: the
    /// target leaf and what a release there would do, for the preview wash.
    drop_preview: Option<(ThreadId, Zone)>,
    /// The Pane native files were last dragged over. Read only while a drag
    /// is live (`drop_target`), so a drag that leaves the window leaves no
    /// sheet behind.
    file_drop_over: Option<PaneIdentity>,
    /// Boards whose arrangement includes a reader slot, as the operator
    /// left them. Session-only: a reader's place is remembered here until
    /// quit.
    board_layouts: std::collections::HashMap<Board, Tree>,
    /// Every board slot's size as this frame laid it out, for the ghost a
    /// drag of it shows. Written while the board draws.
    slot_sizes: std::cell::RefCell<std::collections::HashMap<ThreadId, (f32, f32)>>,
    /// The board slot being dragged: it dims in place while its ghost
    /// moves, until the release.
    dragging: Option<ThreadId>,
    /// The operator's settings and where they save; every change saves.
    prefs: Preferences,
    /// What each Thread's Composer holds unsent.
    unsent: drafts::Unsent,
    /// Where the quotes of sent prompts were cited from.
    cited_sources: ferrite_core::cited_sources::CitedSources,
    /// The sheet that is up, Settings, the Project card or the
    /// `Background work` dialog (ADR 0042).
    sheets: Entity<sheets::Sheets>,
    /// The work `Stop` on the background-work line was pressed for, by
    /// Thread and Session generation (a provider reuses task ids): the line
    /// reads `Stopping...` until it ends, or `Retry` once one refused.
    background_stops:
        std::collections::HashMap<(ThreadId, u64, background_work::StopTarget), background_work::StopState>,
    /// How many stops `background_stops` has taken, to tell one send's
    /// timer from another's.
    background_stop_sends: u64,
    /// Whether the window is maximized, read once per frame in `render`.
    /// The nav's band is drawn without a `Window` in hand, and the two
    /// halves of the titlebar have to agree: a maximized window has no top
    /// resize edge for the drag regions to leave alone (the frame's titlebar).
    maximized: bool,
    /// The CLIs' versions as `--version` reports them, probed once when the
    /// panel first opens: (claude, codex).
    cli_versions: Option<(SharedString, SharedString)>,
    /// A CLI version probe is in flight (the empty board asks once).
    cli_probing: bool,
    /// Where each provider CLI stands against its newest release.
    cli_updates: crate::cli_updates::CliUpdates,
    /// What the machine's Server has been asked and said.
    environment: environment::Environment,
    /// Where Ferrite's own update stands (ADR 0023).
    ferrite_update: crate::ferrite_update::FerriteUpdate,
    /// What the window has said about its Server (ADR 0025).
    server_bands: server::ServerBands,
    /// Whether the window has said `settings.json` could not be used.
    unreadable_told: bool,
    /// The operator's commands running, and what they and the keys have
    /// still to say.
    user_commands: user_commands::UserCommands,
    /// Whether "Quit Ferrite?" is on screen (`leaving`).
    leave_asked: bool,
    /// What the last nav act has to say (a failed Park, a refused
    /// rename).
    nav_notice: nav_list::Notice,
    /// The Inbox: whether its list is down, its cursor, and which Notices
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
    /// Each Thread's `Transcript::published` as last seen: when it moves,
    /// its PR is read again at once (`refresh_published`, HD27).
    published: std::collections::HashMap<ThreadId, u64>,
    /// The PR reads `check_pr` would have sent `gh`: (Thread, manual).
    #[cfg(test)]
    pub(crate) pr_asked: Vec<(ThreadId, bool)>,
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
    /// Measuring at every reader's slide's destination, so a threshold
    /// never flaps mid-slide.
    nav_measure_rest: std::cell::Cell<bool>,
    /// What stands in the board's place: the Factory or the PRs.
    destination: Option<destination::Destination>,
    /// The PRs destination, kept as it was left while another view shows.
    prs: prs::PrsView,
    /// The Tasks whose needs-you toast stands, and the state it announced.
    task_toasts: std::collections::HashMap<String, ferrite_core::factory::Status>,
    /// The empty board's selected row, and the list's own focus.
    empty_cursor: usize,
    empty_focus: FocusHandle,
    /// The Pane whose preview took the last press: ⌘F finds there. A
    /// press hands the keyboard back to the Pane, so focus cannot say it.
    preview_pressed: Option<PaneIdentity>,
    /// Each Thread's reader slot (its Review) sliding open or shut
    /// (`beside.rs`).
    reader_slides: beside::ReaderSlides,
    columns: columns::Columns,
    /// The Provider an empty board's first draft starts on.
    launch_provider: Provider,
    /// The machines this window can be on (ADR 0026); `None` in a window
    /// on an Engine of its own.
    machines: Option<Entity<crate::machines::Machines>>,
    connections: connections::Connections,
}

/// The Cockpit asks its window to put it on machine `index` (ADR 0026),
/// and there to open `open`. `None` leaves the client without an environment.
pub struct GoToMachine {
    pub index: Option<usize>,
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

/// A read-only body the paging keys move: a scrolled document, or the Files
/// editor's lines.
enum Reader {
    Document(ScrollHandle),
    Code(Entity<tools::Editor>),
}

/// What an inline rename is aimed at: a Thread's own display name, at
/// one of the sites that edit it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenameTarget {
    /// A Thread, renamed from its nav row.
    Thread(ThreadId),
    /// A Thread, renamed from the view head (`view_head`): the same title,
    /// a different editor site, so the two never draw one editor twice.
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
}

impl Preferences {
    /// Defaults saved nowhere — the test constructor's and the demo's —
    /// with the right panel shown: the suites and scenes measure the
    /// window with it, and close it where they mean to (`hide_panel`,
    /// `PANEL_CLOSED`). A launch reads the closed default.
    #[cfg_attr(not(any(test, feature = "visual-reference")), allow(dead_code))]
    fn ephemeral() -> Self {
        Self {
            settings: ferrite_core::settings::Settings {
                panel_shown: true,
                ..Default::default()
            },
            // One directory each: a window's drafts must not reach the
            // next test's window. Process ids recycle, so a name alone
            // would hand a later run the tabs and drafts an earlier left.
            dir: {
                static NEXT: std::sync::atomic::AtomicUsize =
                    std::sync::atomic::AtomicUsize::new(0);
                let dir = std::env::temp_dir().join(format!(
                    "ferrite-prefs-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));
                let _ = std::fs::remove_dir_all(&dir);
                dir
            },
            defaults: std::sync::Arc::new(std::sync::Mutex::new(
                ferrite_core::spawner::SessionDefaults::default(),
            )),
            titler: false,
            cli_updates: false,
            ferrite_updates: false,
            whats_new: None,
            unreadable: None,
            keys_refused: Vec::new(),
        }
    }
}

/// A seam mid-drag.
struct SeamDrag {
    board: Board,
    seam: SeamId,
    tree: Tree,
}

/// The board laid out by a split tree: the shown Pane's, while it has a
/// reader open beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Board(PaneIdentity);

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
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        use ferrite_ui::theme;
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
            ferrite_ui::icons::icon(ferrite_ui::icons::FILE, theme::ROW_ICON, theme::INK_SUBTLE)
                .into_any_element()
        } else {
            ferrite_ui::components::status_dot(theme::INK_SUBTLE).into_any_element()
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
                    .text_color(rgb(theme::INK_STRONG))
                    .child(self.face.title.clone()),
            )
            .children(self.face.detail.clone().map(|detail| {
                div()
                    .min_w_0()
                    .flex_shrink(1.)
                    .truncate()
                    .text_size(px(theme::FS_UI))
                    .line_height(px(theme::LH_UI))
                    .text_color(rgb(theme::INK_SUBTLE))
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
/// cancel` in mono `INK_SUBTLE`, the `·` in structure ink. The keys live
/// here, where they act, rather than on the menu row that opened it.
fn rename_field(editor: Entity<ferrite_pane::composer::Composer>, hint: bool) -> Div {
    let text = "↵ save · esc cancel";
    let at = text.find(" · ").unwrap_or(0);
    let hint_text = ferrite_ui::components::text_meta()
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
        .child(
            div()
                .debug_selector(|| "rename-field".into())
                .min_w_0()
                .flex_1()
                .child(editor),
        )
        .children(hint.then_some(hint_text))
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
/// are shared; the kind decides the row recipe and the rule that closes it.
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
    /// The model picker (`model_picker.rs`): every Provider's models and
    /// the Reasoning column, a Thread's picks re-aiming it and a draft's
    /// making its band choice. Opened by the model chip, `/model` and
    /// `/effort`.
    Provider(Box<ModelPicker>),
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
    /// The model picker's own line holds the keyboard while it is open.
    fn holds_keyboard(&self) -> bool {
        matches!(self, Kind::Provider(_))
    }

    /// Text-derived popovers follow the line: `sync_menu` rebuilds them on
    /// every edit and escape mutes them until the text moves. The rest are
    /// opened by an act, closed by one, and dismissed by typing a prompt
    /// over them.
    fn follows_text(&self) -> bool {
        matches!(self, Kind::Commands | Kind::Files { .. })
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
#[derive(Clone, PartialEq)]
enum Consequence {
    /// Replace the active `/filter` with `/name ` — sent later as plain text
    /// on Claude and translated to the typed skill item inside the Codex
    /// Session.
    Command(SharedString),
    /// Replace the active `$filter` with the skill's `$name ` token, which
    /// goes on the wire as `/name` (`composer::wire_text`).
    Skill(SharedString),
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
#[derive(Clone, PartialEq)]
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

/// How often the sweep re-reads free disk for the nav's foot.
const FREE_DISK_EVERY: Duration = Duration::from_secs(60);

#[cfg(test)]
thread_local! {
    /// Root Cockpit renders on this (test) thread: the unit the idle frame
    /// budget is measured in.
    pub(crate) static RENDERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// What the last frame drew for each Pane and nav row, and how often each
/// was built: the stale-chrome tests read what is on screen, not the model,
/// and the frame budgets count the work each part did.
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
    }

    thread_local! {
        static PANES: RefCell<HashMap<PaneIdentity, Pane>> = RefCell::new(HashMap::new());
        static PANE_RENDERS: RefCell<HashMap<PaneIdentity, usize>> = RefCell::new(HashMap::new());
        static NAV_RENDERS: Cell<usize> = const { Cell::new(0) };
        static NAV_ROWS: RefCell<HashMap<ThreadId, String>> = RefCell::new(HashMap::new());
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
        cockpit: Cockpit,
        launch_provider: Provider,
        prefs: Preferences,
        cx: &mut Context<Self>,
    ) -> Self {
        let panel = panel::Panel::load(&prefs.dir);
        Self::new_with_panel(cockpit, launch_provider, prefs, panel, cx)
    }

    pub(crate) fn new_with_panel(
        mut cockpit: Cockpit,
        launch_provider: Provider,
        prefs: Preferences,
        panel: panel::Panel,
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
        cx.on_release(|view, _| view.flush_panel_persistence()).detach();
        cx.on_app_quit(|view, cx| {
            view.save_drafts(cx);
            view.flush_unsaved_files(cx);
            view.flush_panel_persistence();
            view.cockpit.halt_sessions();
            #[cfg(feature = "cef")]
            let browser = view.browsers.controller.clone();
            let executor = cx.background_executor().clone();
            async move {
                #[cfg(feature = "cef")]
                if let Some(browser) = browser {
                    executor
                        .spawn(async move {
                            browser.shutdown();
                        })
                        .await;
                }
                #[cfg(not(feature = "cef"))]
                let _ = executor;
            }
        })
        .detach();

        // A packaged app's process directory belongs to the app, not the
        // operator. Start drafts where they were last working, and leave a
        // new installation genuinely empty.
        let launch_project = startup_project(&cockpit);
        let cited_sources = ferrite_core::cited_sources::CitedSources::load(&prefs.dir);
        // The existing GPUI fixtures assume construction supplies a draft.
        // Keep their synthetic seed out of production; `startup_project` is
        // tested directly against an empty registry below.
        #[cfg(test)]
        let launch_project = launch_project.or_else(|| cockpit.register_project(&here()).ok());
        cx.bind_keys([
            gpui::KeyBinding::new("enter", browser::Go, Some("BrowserAddress")),
            gpui::KeyBinding::new("escape", browser::CancelAddress, Some("BrowserAddress")),
            gpui::KeyBinding::new("enter", browser::AcceptDialog, Some("BrowserDialog")),
            gpui::KeyBinding::new("escape", browser::DismissDialog, Some("BrowserDialog")),
            gpui::KeyBinding::new("enter", browser::AcceptDialog, Some("BrowserPrompt")),
            gpui::KeyBinding::new("escape", browser::DismissDialog, Some("BrowserPrompt")),
        ]);
        cx.bind_keys([
            gpui::KeyBinding::new("escape", browser::CancelAnnotation, Some("BrowserAnnotationCapture")),
            gpui::KeyBinding::new("escape", ferrite_pane::annotation_editor::Cancel, Some("AnnotationEditor")),
        ]);
        let cockpit_weak = cx.weak_entity();
        let sheets = cx.new(|_| sheets::Sheets::default());
        cx.subscribe(&sheets, Self::on_sheets_event).detach();
        let mut view = Self {
            cockpit,
            browsers: Default::default(),
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
            message_minute: ferrite_core::clock::now().timestamp().div_euclid(60),
            branch_refreshing: false,
            branch_refresh_queued: false,
            selection: TranscriptText::default(),
            side_chats: Default::default(),
            held_sends: Default::default(),
            tools: Default::default(),
            agent_drills: Default::default(),
            native_copy: None,
            nav_filter: None,
            nav_view: cx.new(|cx| nav_view::NavView::new(cockpit_weak, cx)),
            nav_events: None,
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
            snooze_editor: None,
            popups: Default::default(),
            panel,
            panel_empty_focus: cx.focus_handle(),
            panel_keys: cx.focus_handle(),
            panel_closing_focus: cx.focus_handle(),
            history: Default::default(),
            attach_menu: None,
            usage_popover_held: false,
            session_control_error: None,
            titling: std::collections::HashSet::new(),
            float_triggers: FloatTriggers::default(),
            band_from_hero: false,
            card_waits: Default::default(),
            nav_collapsed: prefs.settings.nav_collapsed,
            nav_auto_fold: std::cell::Cell::new(false),
            nav_forced_open: false,
            nav_measure_full: std::cell::Cell::new(false),
            head_parent_room: Default::default(),
            facts: Facts::with_auto_title(prefs.settings.auto_title),
            seam_drag: None,
            board: std::cell::Cell::new(layout::Rect::default()),
            drop_preview: None,
            file_drop_over: None,
            board_layouts: std::collections::HashMap::new(),
            slot_sizes: Default::default(),
            dragging: None,
            unsent: drafts::Unsent::load(&prefs.dir),
            cited_sources,
            prefs,
            sheets,
            background_stops: Default::default(),
            background_stop_sends: 0,
            maximized: false,
            cli_versions: None,
            cli_probing: false,
            cli_updates: Default::default(),
            environment: Default::default(),
            ferrite_update: Default::default(),
            server_bands: Default::default(),
            unreadable_told: false,
            user_commands: Default::default(),
            leave_asked: false,
            nav_notice: Default::default(),
            needs_you: NeedsYou::new(),
            toast_stage: Default::default(),
            landing: None,
            #[cfg(test)]
            chimed: Vec::new(),
            pr_focus: None,
            published: std::collections::HashMap::new(),
            #[cfg(test)]
            pr_asked: Vec::new(),
            floats: palette::Floats::new(),
            transcript_entities: Default::default(),
            decisions: decisions::DecisionState::new(cx.weak_entity()),
            nav_measure_rest: std::cell::Cell::new(false),
            destination: None,
            prs: prs::PrsView::new(cx),
            task_toasts: Default::default(),
            empty_cursor: 0,
            empty_focus: cx.focus_handle(),
            preview_pressed: None,
            reader_slides: Default::default(),
            columns: columns::Columns::new(),
            launch_provider,
            machines: None,
            connections: connections::Connections::default(),
        };
        // A Cockpit notification is the earliest common point after core
        // state changes and before GPUI draws cached transcript children.
        // The render path repeats this cheap key comparison for initial mount
        // and derived display state that did not notify the Cockpit.
        cx.observe_self(|view, cx| {
            view.sync_visible_transcripts(cx);
            view.sync_composer_floats(cx);
        })
        .detach();
        // Every Thread the launch opened is on the roster already, and
        // every Thread it did not open is a parked row from the first
        // frame — a launch that opens nothing has no change to notice.
        view.sync_panes(cx);
        view.reopen_unsent_drafts(cx);
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
            pane.preview.set_remote_files(self.on_machine());
            self.aim_attachments(&pane.composer, cx);
            cx.subscribe(&pane.composer, Self::composer_edited).detach();
            cx.subscribe(&pane.composer, Self::composer_opens_source).detach();
            cx.subscribe(&pane.composer, Self::composer_comment_opened).detach();
            self.restore_draft(thread, &pane.composer.clone(), cx);
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
    /// turns until the answer lands. A manual ask reads the open PR tab
    /// again too: its checks and conversation move without the reading.
    pub(crate) fn check_pr(&mut self, thread: ThreadId, manual: bool, cx: &mut Context<Self>) {
        if manual {
            self.refresh_pr_tab(thread, cx);
        }
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

    /// A Thread whose work may have moved its git or forge state since the
    /// last frame — a commit, a push, a PR, background work handed back
    /// (`Transcript::published`): its PR is read again at once, so Changes
    /// follows (HD27). First sight only notes the count: opening a Thread
    /// asks already.
    fn refresh_published(&mut self, cx: &mut Context<Self>) {
        let moved: Vec<ThreadId> = self
            .cockpit
            .threads()
            .into_iter()
            .filter(|thread| {
                let Some(count) = self
                    .cockpit
                    .thread(*thread)
                    .map(|open| open.transcript().published())
                else {
                    return false;
                };
                self.published
                    .insert(*thread, count)
                    .is_some_and(|seen| seen != count)
            })
            .collect();
        for thread in moved {
            self.check_pr(thread, true, cx);
        }
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
            self.pr_reading_changed(thread, cx);
        }
    }

    /// Open the Thread's PR in its browser preview. An explicit open: the
    /// Thread comes forward first, so the page is the one shown.
    pub(crate) fn open_pr(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let url = self.facts.pr(thread).and_then(|pr| pr.url.clone());
        if let Some(url) = url {
            self.show_thread(thread, cx);
            self.browser_action(thread, ferrite_core::browser::Action::Open { url }, cx);
        }
    }

    /// The Thread's PR as a link target: the one every door to it opens.
    pub(crate) fn pr_target(&self, thread: ThreadId) -> Option<LinkTarget> {
        self.facts.pr(thread).map(|pr| LinkTarget::PullRequest {
            thread,
            number: pr.number,
            url: pr.url.clone(),
        })
    }

    /// An explicit open of the Thread's PR, by the door `how` names: its
    /// native detail, the integrated browser, or the system's. What cannot
    /// open says why.
    pub(crate) fn open_pr_as(
        &mut self,
        thread: ThreadId,
        how: Open,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.pr_target(thread) else {
            return;
        };
        let address = target.address();
        match (how, address) {
            (Open::Primary, _) => {
                self.show_thread(thread, cx);
                self.open_pr_surface(thread, cx);
            }
            (Open::Integrated, Ok(_)) => self.open_pr(thread, cx),
            (Open::System, Ok(url)) => cx.open_url(&url),
            (_, Err(why)) => ferrite_pane::link_target::notify_unavailable(&why, window, cx),
        }
    }

    /// Bring `thread` to the front for an explicit open: its Pane, from
    /// wherever the nav row's own click would land. A Thread already
    /// shown stays put.
    fn show_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if self.focused_thread() != Some(thread) || self.destination_shown() {
            self.open_nav_row(thread, cx);
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
    /// tail row, the call a pending Decision gates, and the
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
        let revision = subject_view.presentation_revision();
        let status = pane::transcript_status(subject_view.status(), subject_view.fresh());
        // One reading size for every transcript, on every board.
        let reading_size = self.prefs.settings.reading_size;
        let tail = self.transcript_tail(index, cx);
        self.wake_for_background_clock(index, cx);
        // The gated call hides while its Decision waits as the tail row.
        let pending_call = tail
            .as_ref()
            .and_then(|_| open.pending())
            .map(|decision| decision.tool_use_id.clone())
            .filter(|call| !call.is_empty());
        let workspace =
            ferrite_core::workspace::effective_cwd(open.session_project_root(), open.workspace())
                .map(std::path::Path::to_path_buf);
        let solo = self.on_plane(thread);
        // While the turn runs, its header under its prompt and its live
        // row at the tail (`pane::working_tail`), over a Decision that
        // waits.
        let working = pane::working_tail(pane::WorkingFacts {
            transcript: subject_view.transcript(),
            thread,
            starting: open.starting(),
            solo,
            focused,
            reduce_motion: cx.reduce_motion(),
        });
        let agents = transcript_glue::agent_faces(open.activity());
        let rewind = transcript_glue::rewind_gate(&open);
        // A subagent's tab draws no Composer to quote into.
        let quotes = self.panes[index].is_main();
        let queued = open.queue().len();
        let earlier = self.earlier_history(thread, subject_view.transcript(), quotes);
        let presentation = ferrite_pane::transcript::TranscriptPresentation {
            thread,
            selection_scope,
            disclosure_focus: self.panes[index].tool_focus(),
            namespace,
            content_revision: revision,
            display_revision: disclosure_revision,
            focused,
            signal_status: Some(status),
            reading_size,
            working,
            tail,
            pending_call,
            workspace,
            solo,
            thinking: self.prefs.settings.show_thinking,
            agents,
            rewind,
            quotes,
            reply_to: open.provider().to_string().into(),
            skills: open.commands().into(),
            cited: self
                .cited_sources
                .of(thread)
                .map(std::borrow::Cow::Borrowed)
                .unwrap_or_default(),
            note: None,
            earlier,
            queued,
        };
        let entity = self.panes[index]
            .ensure_transcript(cx)
            .expect("thread Pane has a transcript entity");
        if self.transcript_entities.insert(entity.entity_id()) {
            cx.subscribe(&entity, Self::transcript_event).detach();
        }
        let transcript = subject_view.transcript();
        let pane = &self.panes[index];
        let selection = self.selection.clone();
        entity.update(cx, |view, cx| {
            view.reconcile(
                presentation,
                || {
                    let disclosure = pane.transcript_disclosure_snapshot();
                    ferrite_pane::transcript::TranscriptBody {
                        blocks: pane::rendered_window(transcript.blocks()).to_vec(),
                        preview: pane.preview.clone(),
                        expanded: disclosure.0,
                        target: disclosure.1,
                    }
                },
                selection,
                cx,
            );
        });
    }

    /// Whether the Thread began before the first row `transcript` draws: its
    /// Blocks were dropped to the limit, or the pane's window is cut from
    /// them. Its whole history can be copied out when the log is readable
    /// here, or when nothing was dropped from the Blocks.
    fn earlier_history(
        &self,
        thread: ThreadId,
        transcript: &ferrite_core::transcript::Transcript,
        main: bool,
    ) -> Option<ferrite_pane::transcript::Earlier> {
        let dropped = transcript.earlier_dropped();
        let cut = pane::rendered_window(transcript.blocks()).len()
            < transcript.blocks().len();
        (dropped || cut).then(|| ferrite_pane::transcript::Earlier {
            copyable: main && (!dropped || self.cockpit.history_log(thread).is_some()),
        })
    }

    /// What the open floats make of each Composer: the open popover widens
    /// its Composer's key context to ComposerMenu (the focused node, where
    /// enter and escape win their tie against Submit and Interrupt), and the
    /// lit caret — Solo's and the focused board Pane's block (FL-12) —
    /// blinks under any open float; an unfocused board Pane's is the hollow
    /// box. Pushed on the Cockpit's own notify (`observe_self`), before the
    /// frame draws, so the Pane a float opens over is built once in that
    /// frame rather than again in the next; render repeats it for state it
    /// derives itself. Each push notifies only on a change.
    fn sync_composer_floats(&self, cx: &mut Context<Self>) {
        let focused_index = self.focused();
        let float_open = self.overlay_open(cx);
        for (index, pane) in self.panes.iter().enumerate() {
            let open = self
                .popover
                .as_ref()
                .is_some_and(|open| open.pane == pane.identity);
            let lit = index == focused_index;
            pane.composer.update(cx, |composer, cx| {
                composer.set_menu_open(open, cx);
                composer.set_caret_lit(lit, float_open, cx);
            });
        }
    }

    /// Whether `thread`'s Pane is the window's plane, on its black ground,
    /// rather than drawn by the Factory, which draws a Worker or a
    /// Coordinator on its own ground.
    fn on_plane(&self, thread: ThreadId) -> bool {
        self.factory_worker_drawn() != Some(thread)
    }

    fn sync_visible_transcripts(&mut self, cx: &mut Context<Self>) {
        let mut shown = self.visible_indices();
        // The Factory draws a Worker or its Coordinator the roster does not
        // show (its Task's Worker tab, the Coordinator surface).
        shown.extend(
            self.factory_worker_drawn()
                .and_then(|thread| self.pane_for(thread))
                .filter(|index| !shown.contains(index)),
        );
        for index in shown {
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

    /// PageUp, PageDown, Home or End on the focused Pane's transcript; the
    /// details card, holding the keyboard, takes them and leaves it be.
    fn page_transcript(
        &mut self,
        page: Page,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.details_hold_focus(window, cx) {
            return;
        }
        if let Some(reader) = self.reader(window, cx) {
            Self::scroll_reader(&reader, page, cx);
            cx.notify();
            return;
        }
        if let Some(transcript) = self
            .panes
            .get(self.focused())
            .and_then(|pane| pane.transcript())
        {
            transcript.update(cx, |transcript, cx| transcript.page(page, cx));
        }
    }

    /// Move a read-only body by `page`, the way the transcript's own paging does.
    fn scroll_reader(reader: &Reader, page: Page, cx: &mut Context<Self>) {
        let scroll = match reader {
            Reader::Code(editor) => return editor.read(cx).scroll_page(page),
            Reader::Document(scroll) => scroll,
        };
        let max = scroll.max_offset().y;
        let step = scroll.bounds().size.height;
        let at = scroll.offset().y;
        let to = match page {
            Page::Top => px(0.),
            Page::Bottom => -max,
            Page::Up => (at + step).min(px(0.)),
            Page::Down => (at - step).max(-max),
        };
        scroll.set_offset(gpui::point(scroll.offset().x, to));
    }

    /// The read-only body the paging keys belong to: the open preview's
    /// document when it holds focus, else the Files tab the panel's keys
    /// are on. Everything else leaves them to the transcript.
    fn reader(&self, window: &Window, cx: &gpui::App) -> Option<Reader> {
        let index = self.focused();
        let pane = self.panes.get(index)?;
        if pane.preview.focused(window, cx) && self.preview_shown(index) {
            let tab = &pane.preview_tab;
            let list = tab.scroll.0.borrow().base_handle.clone();
            return Some(Reader::Document(if tab.markdown_scroll.max_offset().y > px(0.) {
                tab.markdown_scroll.clone()
            } else {
                list
            }));
        }
        let thread = pane.thread()?;
        let on_panel = self.panel_control_focused(window) || self.panel_keys_held(window, cx);
        if !on_panel || self.active_surface(thread) != Some(crate::cockpit::surfaces::SurfaceId::Files) {
            return None;
        }
        self.files_prose_scroll(thread)
            .map(Reader::Document)
            .or_else(|| self.files_editor_of(thread).map(Reader::Code))
    }

    /// A send in Pane `index`: its prompt, once it arrives, scrolls to the
    /// top of the transcript (`TranscriptView::scroll_to_new_prompt`).
    fn scroll_transcript_to_new_prompt(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(transcript) = self.panes[index].transcript() {
            transcript.update(cx, |transcript, cx| transcript.scroll_to_new_prompt(cx));
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
            ferrite_pane::transcript::TranscriptEvent::OpenAgent(call) => {
                self.open_agent_from(index, call.clone(), cx);
            }
            ferrite_pane::transcript::TranscriptEvent::Prompt { block, action, at } => {
                self.prompt_action(index, *block, *action, *at, cx);
            }
            // The reply waits in the Pane's tray as a quote chip, naming its
            // source; the focus pump hands the Composer the keyboard.
            ferrite_pane::transcript::TranscriptEvent::Reply(reply, block, anchor) => {
                let mut reply = reply.clone();
                if let Some((thread, block)) = self.panes[index].thread().zip(*block) {
                    reply = reply.cited_from(ferrite_core::replies::Source { thread, block });
                }
                self.focus_pane(index);
                self.panes[index]
                    .composer
                    .update(cx, |composer, cx| composer.cite_with_comment(reply, anchor.clone(), cx));
                cx.notify();
            }
            ferrite_pane::transcript::TranscriptEvent::Fork { after } => {
                if let Some(thread) = self.panes[index].thread() {
                    self.fork_after(thread, *after, cx);
                }
            }
            ferrite_pane::transcript::TranscriptEvent::CopyFullHistory => {
                if let Some(thread) = self.panes[index].thread() {
                    self.copy_transcript(thread, cx);
                }
            }
            ferrite_pane::transcript::TranscriptEvent::SideChat(passage) => {
                if let Some(thread) = self.panes[index].thread() {
                    self.open_side_chat(thread, Some(passage.clone()), cx);
                }
            }
            // A wrap-up's next step: its words go as the next prompt.
            ferrite_pane::transcript::TranscriptEvent::Send(words) => {
                self.focus_pane(index);
                if let Some(thread) = self.panes[index].thread() {
                    let _ = self.cockpit.send(thread, words.clone());
                }
                cx.notify();
            }
            ferrite_pane::transcript::TranscriptEvent::OpenThread(thread) => {
                self.focus_thread(*thread, cx);
            }
            ferrite_pane::transcript::TranscriptEvent::OpenPullRequest { number, url } => {
                self.focus_pane(index);
                if let Some(thread) = self.panes[index].thread() {
                    self.open_named_pr(thread, *number, url.clone(), cx);
                }
            }
            ferrite_pane::transcript::TranscriptEvent::OpenBackground => {
                self.focus_pane(index);
                if let Some(thread) = self.panes[index].thread() {
                    self.open_background(thread, cx);
                }
            }
            ferrite_pane::transcript::TranscriptEvent::OpenChanges(path) => {
                self.focus_pane(index);
                let Some(thread) = self.panes[index].thread() else {
                    return;
                };
                match path {
                    Some(path) => {
                        let root = self.thread_path(thread);
                        let relative = root
                            .as_deref()
                            .and_then(|root| std::path::Path::new(path).strip_prefix(root).ok())
                            .map_or_else(|| path.clone(), |path| path.display().to_string());
                        self.show_changes_of(thread, &relative, cx);
                    }
                    None => {
                        self.upsert_surface(thread, surfaces::SurfaceId::Changes, cx);
                        cx.notify();
                    }
                }
            }
        }
    }

    /// Every open Pane's name, from the cache — the head reads
    /// `PaneView::name`, and it moves only at a naming moment.
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

    /// A quote chip's Go to source: the Pane's transcript scrolls to the
    /// message it was cited from, or the chip says that message is gone.
    fn composer_comment_opened(&mut self, composer: Entity<Composer>, opened: &ferrite_pane::composer::CommentOpened, cx: &mut Context<Self>) {
        let Some(reply) = composer.read(cx).replies().get(opened.index).cloned() else { return; };
        let Some(source) = reply.source else { return; };
        let Some(transcript) = self.panes.iter().find(|pane| pane.composer == composer && pane.thread() == Some(source.thread)).and_then(|pane| pane.transcript()) else { return; };
        let Some((document, mut keys)) = transcript.read(cx).comment_source(source.block) else { return; };
        let selector = gpui::base::TextSelectionSelector {
            text: reply.quote,
            start: reply.selector.as_ref().map_or(0, |selector| selector.start),
            end: reply.selector.as_ref().map_or(0, |selector| selector.end),
            prefix: reply.selector.as_ref().map_or_else(String::new, |selector| selector.prefix.clone()),
            suffix: reply.selector.as_ref().map_or_else(String::new, |selector| selector.suffix.clone()),
        };
        if let Some(part) = reply.selector.as_ref().map(|selector| selector.part) {
            if part < keys.len() { keys.swap(0, part); }
        }
        let window = opened.window;
        let index = opened.index;
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let anchors = keys.iter().filter_map(|key| document.anchor_text(key, &selector, window, cx)).collect::<Vec<_>>();
                if anchors.len() == 1 {
                    composer.update(cx, |composer, cx| composer.highlight_comment_source(index, anchors.into_iter().next().unwrap(), cx));
                }
            });
        });
    }

    fn composer_opens_source(
        &mut self,
        composer: Entity<Composer>,
        OpenSource { source, quote }: &OpenSource,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.panes.iter().find(|pane| pane.composer == composer) else {
            return;
        };
        let found = pane.thread() == Some(source.thread)
            && pane.transcript().is_some_and(|transcript| {
                transcript.update(cx, |transcript, cx| transcript.reveal_passage(source.block, quote, cx))
            });
        if !found {
            composer.update(cx, |composer, cx| composer.source_missing(*source, cx));
        }
    }

    /// The focused Composer's line moved: unmute and re-derive the menu.
    /// Menus follow the text — typing `/` or `@` opens, backspacing past
    /// the trigger closes, and a pick's own splice closes through here too.
    fn composer_edited(&mut self, composer: Entity<Composer>, _: &Edited, cx: &mut Context<Self>) {
        self.upload_attachments(&composer, cx);
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
        if let Some(identity) = self
            .panes
            .iter()
            .find(|pane| pane.composer == composer)
            .map(|pane| pane.identity)
        {
            if let Some(thread) = identity.thread() {
                self.cockpit.set_composing(thread, unsent);
            }
            self.keep_draft(identity, cx);
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
        self.poll_connections(cx);
        let commands_changed = self
            .draft_commands
            .as_mut()
            .is_some_and(DraftCommands::poll);
        if commands_changed {
            self.sync_menu(cx);
        }
        let frame = self.cockpit.pump();
        self.fetch_attachment_previews(cx);
        self.note_work(&frame);
        self.prune_background_stops();
        self.close_sheet_with_nothing_to_draw(cx);
        self.pump_browsers(cx);
        if let Some(card) = self.project_card(cx) {
            card.update(cx, |card, cx| card.poll_remote_path(cx));
        }
        self.poll_machine_repo(cx);
        let server_moved = self.server_moved();
        // The Server says when its parked Threads' rows moved.
        let parked_moved = self.cockpit.take_parked_changed();
        if parked_moved {
            self.parked_changed(cx);
        }
        if self.cockpit.take_factory_changed() {
            self.factory_changed(cx);
        }
        self.poll_factory_skills(cx);
        self.poll_environment(cx);
        self.apply_thread_rules(&frame, cx);
        self.land(cx);
        self.ring_alerts(cx);
        self.settle_held_sends();
        for index in 0..self.panes.len() {
            if let Some(thread) = self.panes[index].thread() {
                for draft in self.lost_sends(thread) {
                    self.put_back_send(index, draft, true, cx);
                }
                while let Some((held, prepend)) = self.cockpit.take_retrieved_prompt(thread) {
                    let content = self.recovered_content(thread, held);
                    self.panes[index].composer.update(cx, |composer, cx| {
                        composer.restore_content(content, prepend, cx)
                    });
                    cx.notify();
                }
            }
        }
        let models_changed = self.cockpit.take_models_changed();
        let tuning_availability_changed = self.popover.as_ref().is_some_and(|open| {
            matches!(open.kind, Kind::Provider(_))
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
        self.refresh_published(cx);
        let focused = self.focused_thread();
        if focused != self.pr_focus {
            self.pr_focus = focused;
            if let Some(thread) = focused {
                self.check_pr(thread, false, cx);
            }
        }
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
            self.follow_workspace(cx);
            // Free disk rides the sweep, once a minute: no wake of its own.
            if self
                .free_disk_asked
                .is_none_or(|at| now.duration_since(at) >= FREE_DISK_EVERY)
            {
                self.free_disk_asked = Some(now);
                self.read_free_disk(cx);
            }
            // Message times turn over with the minute, on this one timer:
            // each transcript redraws then, and no row arms a wake.
            let minute = ferrite_core::clock::now().timestamp().div_euclid(60);
            if minute != self.message_minute {
                self.message_minute = minute;
                for transcript in self.panes.iter().filter_map(|pane| pane.transcript()) {
                    transcript.update(cx, |_, cx| cx.notify());
                }
            }
        } else if self.cockpit.wants_worktree_listing() {
            // A Main just finished making a worktree: ask git now, not in
            // up to two seconds, so the header follows without a pause.
            self.refresh_branches(cx);
        }
        self.pump_panel_persistence(cx);
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
                let mut selected = self.panes[index].selected.clone();
                let resolves = self
                    .cockpit
                    .thread(update.thread)
                    .is_some_and(|open| open.activity().subject(&selected).is_some());
                // A fresh Session forgets its agents: the Pane they filled
                // has no head to leave by, so it goes back to Main.
                if !resolves {
                    selected = ferrite_core::activity::Subject::Main;
                }
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
            let settled = self.facts.streamed(&self.cockpit, update.thread);
            self.advance_pr_actions(update.thread, settled, cx);
            turn_ended |= settled;
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
        let rows = !swept_busy.is_empty()
            || (!frame.is_empty() && self.nav_view.read(cx).rows_moved(&self.nav_state()));
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
    /// waiting on the operator (the Inbox's needs-you rows), the unread
    /// notices and attention (the Inbox, each
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
            let blocks = transcript.blocks();
            let mut turn_start = 0;
            for (at, block) in blocks.iter().enumerate() {
                match &block.body {
                    ferrite_core::transcript::Body::Tool(tool) => {
                        valid.insert(pane::DisclosureId::Tool(tool.call.clone()));
                        valid.insert(pane::DisclosureId::Group(tool.call.clone()));
                    }
                    ferrite_core::transcript::Body::Thinking(_) => {
                        valid.insert(pane::DisclosureId::Reasoning(block.id));
                        valid.insert(pane::DisclosureId::Live(block.id));
                    }
                    // A turn's fold, or a steer's, by its prompt. A steer
                    // stays inside its turn, whose card spans all of it.
                    ferrite_core::transcript::Body::Prompt(_) => {
                        valid.insert(pane::DisclosureId::Turn(block.id));
                        valid.insert(pane::DisclosureId::Prompt(block.id));
                        if !block.steer {
                            turn_start = at;
                        }
                    }
                    // A changed-files card's Expand all and its folders.
                    ferrite_core::transcript::Body::TurnEnd(_) => {
                        valid.insert(pane::DisclosureId::Changes(block.id));
                        for output in ferrite_core::transcript::outputs::of_turn(&blocks[turn_start..at]) {
                            if let ferrite_core::transcript::outputs::Output::Changes(changes) = output {
                                valid.extend(
                                    ferrite_pane::transcript::changes_folders(&changes, self.thread_path(thread).as_deref())
                                        .into_iter()
                                        .map(|folder| pane::DisclosureId::Folder(block.id, folder)),
                                );
                            }
                        }
                        turn_start = at + 1;
                    }
                    _ => {}
                }
            }
        }
        self.panes[index].prune_tools(&valid);
    }

    /// Whether something floats over the cockpit: a menu, a popover or a
    /// sheet. The titlebar's drag region reads it — a region that
    /// lies under an open overlay would answer Windows' hit test as the
    /// window frame, and the row under the pointer would never see the
    /// press (the frame's titlebar).
    fn overlay_open(&self, cx: &gpui::App) -> bool {
        self.sheet_up(cx)
            || self.popover.is_some()
            || self.popup_up()
            || self.sheet_float_open()
    }

    fn nav_folded(&self) -> bool {
        self.nav_collapsed || (self.nav_auto_fold.get() && !self.nav_forced_open)
    }

    fn update_auto_fold(&mut self, window: &Window) {
        use ferrite_ui::theme::{NAV_AUTO_FOLD_BOARD_W, NAV_AUTO_FOLD_HYSTERESIS, NAV_AUTO_FOLD_PANE_W};
        let held = self.nav_auto_fold.get();
        let slack = if held { NAV_AUTO_FOLD_HYSTERESIS } else { 0.0 };
        let width = f32::from(window.viewport_size().width);
        let right = self.panel_target_w();
        let board = width - self.nav_expanded_width() - right;
        let narrowest = |view: &Self| {
            view.pane_rects(window)
                .iter()
                .map(|(_, rect)| rect.w)
                .fold(f32::INFINITY, f32::min)
        };
        self.nav_measure_full.set(true);
        let full = narrowest(self);
        self.nav_measure_full.set(false);
        let gain = (width - right) / board.max(1.0);
        let folded = full * gain;
        let cramped = full < NAV_AUTO_FOLD_PANE_W + slack && folded >= NAV_AUTO_FOLD_PANE_W;
        let fold = board < NAV_AUTO_FOLD_BOARD_W + slack || cramped;
        self.nav_auto_fold.set(fold);
        // `board_bounds` cached the full-column board while measuring.
        self.board_bounds(window);
    }

    /// The board the cards lay out in, in window coordinates (theme WP-C):
    /// flush right of the nav, from under the head row
    /// (`frame::board_top`) to the window's foot, and `board_right` short
    /// of its right edge (the panel, or the gutter).
    fn board_bounds(&self, window: &Window) -> layout::Rect {
        let viewport = window.viewport_size();
        let top = ferrite_ui::frame::board_top();
        let left = self.nav_width();
        let bounds = layout::Rect {
            x: left,
            y: top,
            w: (f32::from(viewport.width) - left - self.board_right()).max(0.0),
            h: (f32::from(viewport.height) - top).max(0.0),
        };
        self.board.set(bounds);
        bounds
    }

    /// The board on screen, when a tree lays it out: the shown Pane's,
    /// while it has a reader slot. A plain Pane has none.
    fn board(&self) -> Option<Board> {
        match self.visible_indices().as_slice() {
            [index] if self.has_reader(*index) => Some(Board(self.panes[*index].identity)),
            _ => None,
        }
    }

    /// A board's tree as it should draw right now: the one mid-drag, else
    /// the operator's arrangement fitted to what is open — the Pane, and
    /// its reader slot while a document is open or sliding shut. A reader
    /// the tree has not placed yet opens beside its Pane, on the right.
    /// Every slot is at rest here: `slid_tree` draws a slide.
    fn board_tree(&self, board: Board) -> Option<Tree> {
        if let Some(drag) = self.seam_drag.as_ref().filter(|drag| drag.board == board) {
            return Some(drag.tree.clone());
        }
        let readers: Vec<ThreadId> = self
            .visible_indices()
            .into_iter()
            .filter(|index| self.has_reader(*index))
            .filter_map(|index| self.panes[index].thread())
            .collect();
        let base = self.board_layouts.get(&board).cloned().unwrap_or_default();
        let mut panes = vec![pane_leaf(board.0)];
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

    /// Keep an arrangement the operator made on a board, for the session.
    fn commit_board(&mut self, board: Board, tree: Tree) {
        self.board_layouts.insert(board, tree);
    }

    /// The shown Pane's rect on the board, in window coordinates — from the
    /// board's tree as this frame draws it (or, measuring at rest, where
    /// the reader's slide ends), or the whole board. The reader slot is no
    /// Pane and is left out.
    fn pane_rects(&self, window: &Window) -> Vec<(usize, layout::Rect)> {
        let bounds = self.board_bounds(window);
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
        self.visible_indices()
            .first()
            .map(|index| vec![(*index, bounds)])
            .unwrap_or_default()
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
        self.show_popup(Popup::Checks(thread), cx);
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
        self.show_popup(Popup::Plan(thread), cx);
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
    /// then one row per step — done `INK_SUBTLE` behind a dim check, the
    /// step being worked `INK_STRONG` behind the strong `❯`, the rest
    /// `INK_STRONG` behind a faint ring.
    fn plan_card_element(
        &self,
        thread: ThreadId,
        face: popups::Face,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
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
                ferrite_ui::components::glyph("\u{2713}", INK_SUBTLE).into_any_element()
            } else if working {
                ferrite_ui::components::prompt_mark(INK_STRONG)
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
                            .text_color(rgb(if done { INK_SUBTLE } else { INK_STRONG }))
                            .child(SharedString::from(step.text.clone())),
                    ),
            );
        }
        let card = card.id("plan-card").debug_selector(|| "plan-card".into());
        Some(
            anchored()
                .anchor(gpui::Anchor::TopRight)
                .position(at)
                .snap_to_window_with_margin(px(GRID_PAD))
                .child(self.popup_face(
                    face,
                    "plan-card-in",
                    card,
                    ferrite_ui::motion::Opens::Down,
                    cx,
                ))
                .into_any_element(),
        )
    }

    /// The Panes on screen, as indices into the mirror — the roster's
    /// `visible`, in its order.
    fn visible_indices(&self) -> Vec<usize> {
        self.cockpit
            .visible()
            .into_iter()
            .filter_map(|identity| self.index_of(identity))
            .collect()
    }

    /// Open the inline editor on a title, seeded with the title as it
    /// stands — a rename starts from what is there, not from an empty line.
    /// A target that no longer resolves has no title to edit, and the
    /// press is dropped rather than opening an editor over nothing.
    fn start_rename(&mut self, target: RenameTarget, cx: &mut Context<Self>) {
        let title = match target {
            // A Thread with no stored title shows its generated one, so
            // that is what the editor opens on: the operator edits the name
            // they can see.
            RenameTarget::Thread(thread)
            | RenameTarget::ViewTitle(thread) => Some(self.facts.name(thread).to_string()),
        };
        let Some(title) = title else {
            return;
        };
        let editor = cx.new(ferrite_pane::composer::Composer::new);
        editor.update(cx, |editor, cx| {
            editor.set_role(ferrite_pane::composer::Role::Field(Some("NavRename")), cx);
            editor.set(title, cx)
        });
        self.rename = Some((target, editor));
        cx.notify();
    }

    /// Close the editor. `save` commits — except on a blank line, which is
    /// not a rename to an empty title but a rename the operator abandoned.
    /// A refusal from the store lands where every other nav notice does;
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
        let (RenameTarget::Thread(thread) | RenameTarget::ViewTitle(thread)) = target;
        match self.cockpit.rename_thread(thread, &title) {
            Err(error) => self.nav_notice.problem(error.to_string()),
            Ok(_) => {
                self.facts.renamed(&self.cockpit, thread);
                self.refresh_names();
                self.nav_notice.clear();
            }
        }
        cx.notify();
    }

    /// The Threads the Parked shelf lists and counts: every parked
    /// Thread the Project filter admits; a parked Thread on screen is still
    /// a parked Thread.
    fn parked_threads(&self) -> Vec<ThreadId> {
        self.facts
            .parked()
            .iter()
            .copied()
            .filter(|thread| self.admitted(*thread))
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

    /// A Thread's effective cwd.
    fn target_path(&self, target: MenuTarget) -> Option<std::path::PathBuf> {
        match target {
            MenuTarget::Thread(thread)
            | MenuTarget::Pane(thread)
            | MenuTarget::Snooze(thread) => self.thread_path(thread),
            MenuTarget::File(thread) => self.files_pressed(thread),
            MenuTarget::FilesMore(_)
            | MenuTarget::ChangesScope(_)
            | MenuTarget::ChangesMore(_)
            | MenuTarget::ChangesBase(_)
            | MenuTarget::Terminal(_)
            | MenuTarget::TerminalMore(_)
            | MenuTarget::SurfaceMenu(_)
            | MenuTarget::SurfaceTab { .. }
            | MenuTarget::PrMore(_)
            | MenuTarget::PrLink(_)
            | MenuTarget::BrowserPage(_) => None,
            MenuTarget::Details(_) => None,
            MenuTarget::Parked
            | MenuTarget::NavFilter
            | MenuTarget::NavFactory
            | MenuTarget::Review { .. }
            | MenuTarget::Prompt { .. }
            | MenuTarget::Factory
            | MenuTarget::Page(_) => None,
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

    /// Delete every Thread the Parked section lists. Each is its own act
    /// against the core, so one that refuses (a dirty legacy worktree, an
    /// unreadable log) does not stop the rest; the refusals are counted
    /// into the nav's banner, with the last reason, and the survivors stay
    /// listed where the operator can deal with them one at a time.
    fn delete_parked_threads(&mut self, cx: &mut Context<Self>) {
        let parked = self.parked_threads();
        let mut refused = 0usize;
        let mut reason = None;
        for thread in parked {
            if let Err(error) = self.delete_thread(thread) {
                refused += 1;
                reason = Some(error.to_string());
            }
        }
        match (refused, reason) {
            (0, _) | (_, None) => self.nav_notice.clear(),
            (1, Some(reason)) => self
                .nav_notice
                .problem(format!("1 parked thread not deleted: {reason}")),
            (count, Some(reason)) => self
                .nav_notice
                .problem(format!("{count} parked threads not deleted, last: {reason}")),
        }
        self.sync_panes(cx);
        self.parked_changed(cx);
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
        let origin_x = self.nav_width();
        let local = |rect: layout::Rect| layout::Rect {
            x: rect.x - origin_x,
            ..rect
        };
        let mut board = div().relative().flex_1().min_w_0().min_h_0();
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
        for (leaf, rect) in rects {
            let cell = match leaf_slot(leaf) {
                Slot::Pane(identity) => {
                    let Some(index) = self.index_of(identity) else {
                        continue;
                    };
                    self.pane_slot(index, window, cx)
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
                        .child(pane::card_ring(ferrite_ui::theme::paint::CARD_RING))
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
        // The seams (F-18): one line per linked set down the gutter's
        // centre — a default grid's aligned column seams are one line from
        // the board's top to its foot — and a 9px grab band over each seam.
        // At rest the line is nothing: the gutter separates the cards.
        // Under the pointer (any band of the set) and while held, the whole
        // line draws `SEAM_LINE_W` of `SEAM_HOVER`, fading in over
        // `motion::HOVER_FADE`.
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
                        .on_hover(ferrite_ui::motion::hover_listener(key.clone()))
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
                        .rounded(px(ferrite_ui::theme::R_CARD))
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
                                .bg(ferrite_ui::theme::paint::FLOAT)
                                .rounded(px(ferrite_ui::theme::R_CONTROL))
                                .border_1()
                                .border_color(ferrite_ui::theme::paint::FLOAT_EDGE)
                                .shadow(ferrite_ui::components::float_shadow())
                                .font_family(ferrite_ui::theme::FONT_UI)
                                .text_size(px(ferrite_ui::theme::FS_UI))
                                .line_height(px(ferrite_ui::theme::LH_UI))
                                .text_color(rgb(ferrite_ui::theme::INK_STRONG))
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
        if self.settings_open(cx) {
            self.hide_sheet(cx);
            return;
        }
        self.show_settings(cx);
        self.popover = None;
        if self.cli_versions.is_none() {
            self.probe_cli_versions(cx);
        }
        self.ask_server_defaults();
        cx.notify();
    }

    /// One beat of the updater: ask the registry when due, then install
    /// whatever automatic updating may install now.
    fn tick_cli_updates(&mut self, cx: &mut Context<Self>) {
        if !self.cli_updates.begin_check(std::time::Instant::now()) {
            self.auto_update_clis(cx);
            return;
        }
        self.check_clis(cx);
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

    /// Install `provider`'s newest CLI on the machine the window is on, now.
    /// Once it lands, the model menus are asked again and every open Thread
    /// of that provider restarts on its own conversation — at once when
    /// idle, otherwise when its turn ends — so nothing keeps running the old
    /// release. Another machine's Threads are not touched.
    fn update_cli(&mut self, provider: Provider, cx: &mut Context<Self>) {
        if self.cli_updates.begin_update(provider) {
            self.install_cli(provider, cx);
        }
    }

    /// The updater's toasts. Like the Inbox's, they wait for render, the
    /// one place with a Window in hand.
    fn present_cli_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::cli_updates::{name, Toast};
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        let toasts = self.cli_updates.take_toasts();
        let place = self.machine_place();
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
                        .title(format!("{} {latest} is available on {place}", name(provider)))
                        .message("Click to update it there. Newer releases bring newer models.")
                        .with_type(NotificationType::Info)
                        .autohide(false)
                        .on_click(move |_, _, cx| {
                            view.update(cx, |view, cx| view.update_cli(provider, cx))
                                .ok();
                        })
                }
                Toast::Updated { provider, version } => Notification::new()
                    .title(format!("{} updated to {version} on {place}", name(provider)))
                    .message("Its open Threads restart on it as each finishes its turn.")
                    .with_type(NotificationType::Success)
                    .autohide(true),
                Toast::Failed { provider, detail } => Notification::new()
                    .title(format!("{} update failed on {place}", name(provider)))
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
                version_label(env!("CARGO_PKG_VERSION"), crate::DEV),
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
                if let Some(settings) = view.settings_sheet(cx) {
                    settings.update(cx, |settings, cx| {
                        settings.copied = Some("Diagnostics");
                        cx.notify();
                    });
                }
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
            // A fork the side chat asked for goes to its tab, not the board.
            Asked::Focus(thread) => {
                if !self.bind_side_thread(thread, cx) {
                    self.land_on_thread(thread, cx);
                }
            }
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
            Asked::TaskAlert {
                project,
                task,
                status,
                title,
                detail,
            } => self.present_task_alert(project, task, status, title, detail, window, cx),
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
            let provider = ferrite_ui::provider::title(warning.provider);
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
                self.nav_notice.problem(format!("settings not saved: {e}"));
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

    /// The one plane's seams (theme rule 2): a `LINE` hairline closing the
    /// head row from the nav's edge across (Claude's: the sidebar's head
    /// has none), and from it down to the window's foot, one at the nav's
    /// edge and one at the panel's. They
    /// paint over the regions and take no pointer, so a region needs no
    /// edge of its own.
    fn plane_seams(&self, panel_w: f32) -> gpui::Div {
        use ferrite_ui::theme::{paint, HAIRLINE_W, WIN_CHROME_H};
        let line = || div().absolute().bg(paint::LINE);
        let column = |at: gpui::Div| at.top(px(WIN_CHROME_H)).bottom_0().w(px(HAIRLINE_W));
        div()
            .debug_selector(|| "plane-seams".into())
            .absolute()
            .inset_0()
            .child(
                line()
                    .left_0()
                    .right_0()
                    .top(px(WIN_CHROME_H - HAIRLINE_W))
                    .h(px(HAIRLINE_W)),
            )
            .when(panel_w > 0.0, |seams| {
                seams.child(column(line().right(px(panel_w))))
            })
    }

    /// The focused Pane's draft, to change — None on a Thread.
    fn focused_draft_mut(&mut self) -> Option<&mut pane::DraftBinding> {
        let focused = self.focused();
        self.panes
            .get_mut(focused)
            .and_then(|pane| pane.draft_mut())
    }

    fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.get(self.focused()).is_some_and(|pane| !pane.composer.read(cx).comment_valid(cx)) {
            return;
        }
        // The card's ↵ is its confirm — but in a machine's path field that
        // holds a path, it adds that directory.
        if let Some(card) = self.project_card(cx) {
            let typed = card.update(cx, |card, cx| card.take_remote_path(window, cx));
            if let Some(path) = typed {
                self.adopt_editor_directory(path, cx);
                return;
            }
            if card.read(cx).ready() {
                self.confirm_project_editor(cx);
            }
            return;
        }
        if self.snooze_editor.is_some() {
            self.finish_snooze(true, cx);
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
        if !self.admit_files(self.focused(), cx) {
            return;
        }
        let draft = composer.update(cx, |composer, cx| composer.take_draft(cx));
        let text = draft.prompt().trim().to_string();
        let cited = draft.content().replies.clone();
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
                            .unwrap_or(self.prefs.settings.default_provider),
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
                let content = self.recovered_content(thread, held);
                composer.update(cx, |composer, cx| {
                    composer.restore_content(content, true, cx)
                });
                cx.notify();
            }
            return;
        }
        // Typing does not wait for the agent; sending does.
        let focused = self.focused();
        let admitted = if self
            .cockpit
            .thread(thread)
            .is_some_and(|open| open.needs_queue())
        {
            let queued = self.cockpit.queue(thread, text.clone());
            if queued {
                self.hold_queued(thread, draft);
            } else {
                self.put_back_send(focused, draft, true, cx);
            }
            queued
        } else {
            let admission = self.cockpit.send(thread, text.clone());
            let admitted = self.settle_send(focused, thread, draft, admission, cx);
            if admitted {
                self.scroll_transcript_to_new_prompt(focused, cx);
            }
            admitted
        };
        if admitted {
            self.keep_cited_sources(thread, &cited);
        }
        self.facts.acted(&self.cockpit, thread);
        // A first prompt names an untitled Thread.
        self.facts.renamed(&self.cockpit, thread);
        self.refresh_names();
        if admitted && self.is_first_prompt(thread) {
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
        if self.floats.palette.is_some()
            || self.panes.get(self.focused()).is_none_or(|pane| !pane.is_main())
        {
            return;
        }
        let Some(thread) = self.focused_thread() else {
            return;
        };
        if !self.panes[self.focused()].composer.read(cx).is_empty() {
            return;
        }
        let Some(id) = self
            .cockpit
            .thread(thread)
            .and_then(|open| queued_to_take_back(&open))
        else {
            return;
        };
        self.cockpit.remove_queued_item(thread, &id, false);
        cx.notify();
    }

    fn interrupt(&mut self, _: &Interrupt, window: &mut Window, cx: &mut Context<Self>) {
        if self.close_popup(cx) {
            return;
        }
        if let Some(transcript) = self.panes.get(self.focused()).and_then(|pane| pane.transcript()) {
            if transcript.update(cx, |transcript, cx| transcript.dismiss_offer(cx)) {
                return;
            }
        }
        if self.panel_control_focused(window) {
            self.leave_panel(window, cx);
            return;
        }
        if self.settings_open(cx) && self.connections.editor.is_some() {
            self.cancel_connection(window, cx);
            return;
        }
        if self.hide_sheet(cx) {
            return;
        }
        if self.snooze_editor.is_some() {
            self.finish_snooze(false, cx);
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
        // Subagents and background work it started; on a Subagent that
        // Subagent alone — and once it has nothing to stop, it goes back to
        // its parent.
        if let Some(thread) = self.focused_thread().filter(|_| !self.machine_lost()) {
            let selected = self.panes[self.focused()].selected.clone();
            match &selected {
                ferrite_core::activity::Subject::Main => self.cockpit.interrupt(thread),
                ferrite_core::activity::Subject::Subagent(key) => {
                    if !self.stop_subagent(thread, key) {
                        let parent = self
                            .agent_lineage(thread, &selected)
                            .pop()
                            .unwrap_or(ferrite_core::activity::Subject::Main);
                        self.select_subject(thread, parent, window, cx);
                    }
                }
            }
            self.facts.acted(&self.cockpit, thread);
        }
        cx.notify();
    }

    /// Stop the Subagent `key` while it runs; whether it did. A finished
    /// one has nothing to stop, and its escape must not reach Main's turn.
    fn stop_subagent(&mut self, thread: ThreadId, key: &ferrite_core::activity::AgentKey) -> bool {
        let Some(open) = self.cockpit.thread(thread) else {
            return false;
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
        if running {
            self.cockpit.stop_subagent(thread, key);
        }
        running
    }

    /// Tab walks a draft's band (#29), or a Thread Pane's rendered tool
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
        if self.sheet_up(cx)
            || self.rename.is_some()
            || self.snooze_editor.is_some()
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
        if self
            .project_card(cx)
            .is_some_and(|card| card.update(cx, |card, cx| card.tab_remote_path(window, cx)))
        {
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
        if self.sheet_up(cx)
            || self
                .focused_thread()
                .and_then(|thread| self.cockpit.thread(thread))
                .is_some_and(|thread| {
                    thread
                        .pending_decisions()
                        .iter()
                        .any(|request| asks_in_fields(&request.decision))
                })
        {
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
        if self.sheet_up(cx)
            || self
                .focused_thread()
                .and_then(|thread| self.cockpit.thread(thread))
                .is_some_and(|thread| {
                    !thread.activity().children().is_empty()
                        || thread
                            .pending_decisions()
                            .iter()
                            .any(|request| asks_in_fields(&request.decision))
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
        if self.panes[self.focused()].draft().is_some() {
            return;
        }
        let focused = self.focused();
        let calls = self.expandable_tools(focused, cx);
        let transcript = self.panes[focused].transcript();
        let tool_focus = self.panes[focused].tool_focus();
        let in_controls = ferrite_pane::rich::controls_focused(window);
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

    /// ↵ on the keyboard's row: a tool call opens or folds; an agent's row,
    /// which has no details, opens its agent.
    fn toggle_tool_action(&mut self, _: &ToggleTool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        let Some(call) = self.panes[self.focused()].targeted_tool().cloned() else {
            return;
        };
        if let pane::DisclosureId::Tool(agent) = &call {
            if self.targeted_agent(thread, &call).is_some() {
                let index = self.focused();
                self.open_agent_from(index, agent.clone(), cx);
                return;
            }
        }
        self.toggle_tool(thread, &call, window, cx);
    }

    /// ⌫ on the keyboard's agent row: its subagent stops, while it runs
    /// (`stop_subagent`); on any other row, nothing.
    fn stop_agent_action(&mut self, _: &StopAgent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        let Some(call) = self.panes[self.focused()].targeted_tool().cloned() else {
            return;
        };
        if let Some(ferrite_core::activity::Subject::Subagent(key)) =
            self.targeted_agent(thread, &call)
        {
            if self.stop_subagent(thread, &key) {
                cx.notify();
            }
        }
    }

    /// The agent the keyboard's row `target` started, when it is an agent's.
    fn targeted_agent(
        &self,
        thread: ThreadId,
        target: &pane::DisclosureId,
    ) -> Option<ferrite_core::activity::Subject> {
        let pane::DisclosureId::Tool(call) = target else {
            return None;
        };
        self.cockpit
            .thread(thread)?
            .activity()
            .invocations()
            .find(|(native, _)| native == call)
            .map(|(_, agent)| agent.subject())
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
        self.answer_chorded(Answer::Allow, window, cx);
    }

    fn deny(&mut self, _: &Deny, window: &mut Window, cx: &mut Context<Self>) {
        self.answer_chorded(Answer::Deny, window, cx);
    }

    fn always(&mut self, _: &Always, window: &mut Window, cx: &mut Context<Self>) {
        self.answer_chorded(Answer::Always, window, cx);
    }

    /// ⌘⏎ / ⇧⌘⏎, the approval buttons' chords: they answer whether or not
    /// text is on the line, a Subagent's approval as Main's.
    fn answer_chorded(&mut self, answer: Answer, window: &mut Window, cx: &mut Context<Self>) {
        if !self.panes[self.focused()].is_main() {
            self.answer_subject(answer, cx);
        } else {
            self.answer(answer, window, cx);
        }
    }

    /// An approval takes option 1, 3 or 2 through the one answer path; a
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

    fn next_thread(&mut self, _: &NextThread, _window: &mut Window, cx: &mut Context<Self>) {
        self.step_thread(1, cx);
    }

    fn previous_thread(&mut self, _: &PreviousThread, _window: &mut Window, cx: &mut Context<Self>) {
        self.step_thread(-1, cx);
    }

    /// ⌘F: find in the focused Pane's transcript — or in its preview, when
    /// the preview took the last press or holds the keyboard
    /// (`ferrite_pane::find`).
    fn open_find(
        &mut self,
        _: &ferrite_pane::find::Open,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(thread) = self.panel_thread() {
            let on_controls = self.panel_control_focused(window);
            if self.open_files_find(thread, on_controls, window, cx) {
                return;
            }
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
    fn toggle_nav(&mut self, _: &ToggleNav, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_nav_now(cx);
    }

    fn toggle_nav_now(&mut self, cx: &mut Context<Self>) {
        if !self.nav_collapsed && self.nav_auto_fold.get() {
            self.nav_forced_open = !self.nav_forced_open;
            if self.nav_folded() {
                self.snooze_editor = None;
            }
            cx.notify();
            return;
        }
        self.set_nav_collapsed(!self.nav_collapsed, cx);
    }

    fn set_nav_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.nav_collapsed == collapsed {
            return;
        }
        self.nav_collapsed = collapsed;
        if collapsed {
            self.cancel_nav_resize();
            self.snooze_editor = None;
        }
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

    /// The `$` menu (T3's): the skills the Session serving the focused
    /// Pane lists, through the fuzzy filter; while a draft's catalogue
    /// loads, or when none matches, one inert line says so.
    fn skill_menu(&mut self, filter: String, cx: &mut Context<Self>) -> Option<Popover> {
        let pane = self.panes.get(self.focused())?;
        let identity = pane.identity;
        let (skills, loading): (Vec<ferrite_core::SessionCommand>, bool) = match pane.thread() {
            Some(thread) => (self.cockpit.thread(thread)?.commands().to_vec(), false),
            None => {
                let draft = pane.draft()?;
                let provider = draft.binding.provider().provider;
                let cwd = draft
                    .binding
                    .resolve(self.cockpit.registry(), !self.on_machine())
                    .ok()?
                    .source_root()
                    .to_path_buf();
                let line = pane.composer.clone();
                let menu = self.draft_commands(provider, cwd);
                menu.poll();
                if menu.pending.is_none() && menu.error.is_none() {
                    line.update(cx, |composer, cx| composer.set_commands(&menu.commands, cx));
                }
                (menu.commands.clone(), menu.pending.is_some())
            }
        };
        let skills: Vec<_> = skills.into_iter().filter(|command| command.is_skill()).collect();
        let mut rows: Vec<Row> = command_rows(&skills, &filter)
            .into_iter()
            .map(|row| Row {
                consequence: Consequence::Skill(row.insert.clone()),
                row,
                active: false,
            })
            .collect();
        if rows.is_empty() {
            rows.push(note_row(if loading {
                "Searching workspace skills..."
            } else {
                "No skills found. Try / to browse provider commands."
            }));
        }
        Some(Popover {
            scroll: ScrollHandle::new(),
            revealed: Default::default(),
            hold: None,
            pane: identity,
            kind: Kind::Commands,
            rows,
            selected: 0,
        })
    }

    /// The draft's command catalogue for `provider` in `cwd`, asked for
    /// afresh when either changed.
    fn draft_commands(&mut self, provider: Provider, cwd: std::path::PathBuf) -> &mut DraftCommands {
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
        self.draft_commands.as_mut().expect("initialized above")
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
        if mention.is_none() {
            if let Some((_, filter)) = skill_token(&text, cursor) {
                return self.skill_menu(filter.to_string(), cx);
            }
        }
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
                let line = pane.composer.clone();
                let menu = self.draft_commands(provider, cwd);
                menu.poll();
                if menu.pending.is_none() && menu.error.is_none() {
                    line.update(cx, |composer, cx| composer.set_commands(&menu.commands, cx));
                }
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
                    Some(("Searching workspace skills...", ""))
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
                            tag: None,
                        },
                        active: false,
                        consequence: Consequence::Inert,
                    });
                }
                if rows.is_empty() {
                    return standalone.then(|| note_popover(identity, NO_COMMAND));
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
            // A command line with no match says so; a `/word` in prose
            // that names nothing is prose, and opens nothing.
            if rows.is_empty() {
                return standalone.then(|| note_popover(pane.identity, NO_COMMAND));
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
        let root = self.pane_root(self.focused())?;
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
                            name: "Searching workspace files...".into(),
                            matched: Vec::new(),
                            detail: SharedString::default(),
                            prose_detail: true,
                            inert: true,
                            tag: None,
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
            return Some(note_popover(pane.identity, NO_FILES));
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

    fn history_older(&mut self, _: &HistoryOlder, _: &mut Window, cx: &mut Context<Self>) {
        self.recall_history(HistoryDirection::Older, cx);
    }

    fn history_newer(&mut self, _: &HistoryNewer, _: &mut Window, cx: &mut Context<Self>) {
        self.recall_history(HistoryDirection::Newer, cx);
    }

    fn recall_history(&mut self, direction: HistoryDirection, cx: &mut Context<Self>) {
        let index = self.focused();
        if !self.history_available(index) {
            return;
        }
        let Some(thread) = self.panes[index].thread() else {
            return;
        };
        if let Some(id) = self
            .cockpit
            .thread(thread)
            .and_then(|open| queued_to_take_back(&open))
        {
            if matches!(direction, HistoryDirection::Older) {
                self.cockpit.edit_queued_item(thread, &id);
                cx.notify();
            }
            return;
        }
        let composer = self.panes[index].composer.clone();
        let draft = composer.read(cx).editable();
        self.unsent
            .step_from(thread, draft.clone(), composer.read(cx).content());
        let Some(text) = self.cockpit.recall_prompt(thread, direction, &draft) else {
            return;
        };
        self.popover = None;
        self.menu_muted = true;
        self.suppress_recall_menu_once = true;
        let content = self.unsent.stepped_to(text);
        composer.update(cx, |composer, cx| composer.put(content, cx));
        cx.notify();
    }

    fn history_available(&self, index: usize) -> bool {
        if index != self.focused() {
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
            && self.snooze_editor.is_none()
            && self.popover.is_none()
            && ((self.panes[index].is_main() && queued_to_take_back(&open).is_some())
                || (open.has_prompt_history()
                    && !open.busy()
                    && open.pending().is_none()
                    && open.queue().is_empty()))
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
            Kind::Provider(_) => {}
        }
        cx.notify();
    }

    /// The shared tail of ↵ and a row click: the row's own consequence,
    /// dispatched. The popover closes either way — a door it opens (the
    /// pickers) is a fresh popover in its place.
    fn pick(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
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
                    // The pick is a chip, as a mention is; it sends as
                    // the plain `/name` it reads.
                    let token = format!("/{name}");
                    composer.splice(start..end, &format!("{token} "), cx);
                    composer.stage_chip(SharedString::from(token), cx);
                });
            }
            Consequence::Skill(name) => {
                composer.update(cx, |composer, cx| {
                    let cursor = composer.cursor();
                    let text = composer.text();
                    let Some((start, _)) = skill_token(text, cursor) else {
                        return;
                    };
                    let mut end = cursor
                        + text[cursor..]
                            .find(char::is_whitespace)
                            .unwrap_or(text.len() - cursor);
                    if text[end..].starts_with(' ') {
                        end += 1;
                    }
                    let token = format!("${name}");
                    composer.splice(start..end, &format!("{token} "), cx);
                    composer.stage_chip(SharedString::from(token), cx);
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
                    composer.stage_chip(SharedString::from(token), cx);
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
                    // The model picker's Reasoning column, as the
                    // palette's `effort` opens it.
                    self.open_effort_picker(thread, cx);
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
                        if open.rows.is_empty() {
                            open.rows = vec![note_row(NO_FILES)];
                        }
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
        for provider in Provider::ALL {
            // After the first prompt the other Provider is a handover, not
            // a swap: its rows stay live and its section says what a pick
            // does — a fresh conversation there, the earlier one carried
            // over as context.
            let handover = locked && provider != current;
            let fixed = false;
            rows.push(Row {
                row: pane::MenuRow {
                    insert: SharedString::default(),
                    name: SharedString::from(ferrite_ui::provider::title(provider)),
                    matched: Vec::new(),
                    detail: SharedString::from(if handover { "handover" } else { "" }),
                    prose_detail: true,
                    inert: true,
                    tag: None,
                },
                active: false,
                consequence: Consequence::Inert,
            });
            let mut catalog = ferrite_core::providers::models::menu(
                self.cockpit.model_catalog(provider),
                chosen.filter(|_| provider == current),
            );
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
                // own default reads `Default`; every other model is named
                // as the footer names it (`Opus 5.5`, no `(1M)`: one name
                // for one model), the context left to its description;
                // every description lowercase-first.
                let name = if model.value == "default" {
                    "Default".to_string()
                } else {
                    ferrite_core::providers::models::footer_name(
                        provider,
                        Some(&model.value),
                        std::slice::from_ref(&model),
                    )
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
                        tag: None,
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
                } else if let Some(index) = draft.and_then(|draft| self.index_of(draft)) {
                    self.panes[index].composer.update(cx, |composer, cx| {
                        composer.take(cx);
                    });
                    self.panes[index].adopt_thread(adopted.thread);
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
        let provider = self
            .panes
            .get(self.focused())
            .map(|pane| match pane.draft() {
                Some(draft) => draft.binding.provider().clone(),
                None => {
                    let open = pane.thread().and_then(|thread| self.cockpit.thread(thread));
                    ProviderChoice {
                        provider: open
                            .map_or(self.prefs.settings.default_provider, |open| open.provider()),
                        model: open.and_then(|open| open.model()).map(str::to_string),
                    }
                }
            })
            .unwrap_or_else(|| self.default_choice());
        self.open_draft_with_choice(target, provider, None, cx);
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

    /// Pin or unpin a Thread (the row menu's `Pin` / `Unpin`): a pinned
    /// Thread is listed under `Pinned`, newest pin last. Saved with the
    /// settings.
    pub(crate) fn set_pinned(&mut self, thread: ThreadId, pinned: bool, cx: &mut Context<Self>) {
        self.change_settings(
            |settings| {
                settings.pinned_threads.retain(|pin| *pin != thread);
                if pinned {
                    settings.pinned_threads.push(thread);
                }
            },
            cx,
        );
        cx.notify();
    }

    /// Show the parked Threads (N-18): the nav unfolds if it is folded
    /// away, the Parked shelf opens, and the list takes the keys on its
    /// header — the ⌘K palette's "show parked" command.
    pub(crate) fn show_parked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_shelf_open(nav::Shelf::Parked, true, cx);
        self.focus_nav_on(Some(nav::NavCursor::shelf(nav::Shelf::Parked)), window, cx);
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
            Some(project),
            cx,
        );
    }

    /// `chosen` is a Project the caller is naming outright, which outranks
    /// every guess below — the filter, the fallback.
    fn open_draft_with_choice(
        &mut self,
        target: DraftTarget,
        provider: ProviderChoice,
        chosen: Option<ProjectId>,
        cx: &mut Context<Self>,
    ) {
        if !self.cockpit.environment_available() {
            return;
        }
        // The draft starts in the Project of the Thread the operator is in:
        // the focused one, else the one focus last left.
        let current = self
            .cockpit
            .roster()
            .current_thread()
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
        let binding = ferrite_core::draft::DraftBinding::new(provider, project, target);
        self.spawn_draft_pane(binding, cx);
        cx.notify();
    }

    /// A draft Pane aimed at `binding`, focused. The one place a draft Pane
    /// is built, for a press of cmd-t and for a draft kept across a restart.
    fn spawn_draft_pane(
        &mut self,
        binding: ferrite_core::draft::DraftBinding,
        cx: &mut Context<Self>,
    ) -> ferrite_core::roster::DraftId {
        let binding = pane::DraftBinding {
            binding,
            band_focus: None,
            error: None,
        };
        let draft = self.cockpit.open_draft();
        let pane = CockpitPane::new(PaneView::new_draft(draft, binding, cx), cx);
        pane.preview.set_remote_files(self.on_machine());
        self.aim_attachments(&pane.composer, cx);
        cx.subscribe(&pane.composer, Self::composer_edited).detach();
        cx.subscribe(&pane.composer, Self::composer_opens_source).detach();
            cx.subscribe(&pane.composer, Self::composer_comment_opened).detach();
        self.panes.push(pane);
        self.sync_panes(cx);
        draft
    }

    /// Open one chip's popover on the focused draft — the shared tail of a
    /// chip click and ↵ on a tab-focused chip. Toggles shut when the same
    /// chip's popover is already up. Rows are registry reads, discovered at
    /// open — never per frame, never a filesystem scan. Git is asked, off
    /// the UI thread, what the checkout is on now and — for the workspace
    /// chip — what the repo holds; the rows take each answer as it lands.
    fn open_band_popover(&mut self, chip: pane::BandChip, cx: &mut Context<Self>) {
        // Its chip's menu, unless the hero's word asks after this.
        self.band_from_hero = false;
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
        if chip == pane::BandChip::Provider {
            if self.popover.as_ref().is_some_and(|open| {
                open.pane == identity && matches!(open.kind, Kind::Provider(_))
            }) {
                self.popover = None;
                cx.notify();
            } else {
                self.open_model_picker(identity, Column::List, cx);
            }
            return;
        }
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
                    SharedString::from("Current checkout"),
                    checked_out.clone(),
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
                        Some(branch) => (branch.clone(), directory),
                        None => (directory, "detached".to_string()),
                    };
                    if let Some(branch) = &worktree.branch {
                        offered.push(branch);
                    }
                    rows.push(band_row(
                        SharedString::from(format!("Previous worktree ({name})")),
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
                    SharedString::from("New worktree"),
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
        self.rebuild_model_picker(None, cx);
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
        let binding = draft.binding.clone();
        let composer = pane.composer.clone();
        if !self.admit_files(self.focused(), cx) {
            return;
        }
        let text = composer.read(cx).prompt().trim().to_string();
        if text.is_empty() {
            return;
        }
        let provider = binding.provider().clone();
        let effort = binding.effort().map(str::to_owned);
        let resolved = binding
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
                self.promote_draft(id, done.thread, cx);
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
    /// the strip over the Composer (`env_strip`), wired to their popovers, which hang above
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
        let (workspace_mark, workspace_label) = match draft.binding.target() {
            DraftTarget::Main => {
                // The chip no longer names the branch, but its menu's first
                // row does: ask git now so the row opens already answered.
                self.checked_out_branch(draft.binding.project(), cx);
                (ferrite_ui::icons::FOLDER, SharedString::from("Current checkout"))
            }
            DraftTarget::Branch { name } => (ferrite_ui::icons::BRANCH, SharedString::from(name.clone())),
            DraftTarget::NewBranch => (ferrite_ui::icons::BRANCH, SharedString::from("New branch")),
            DraftTarget::Existing { branch, .. } => (
                ferrite_ui::icons::FOLDER_GIT_EXISTING,
                SharedString::from(format!("Previous worktree ({branch})")),
            ),
            DraftTarget::New => (ferrite_ui::icons::FOLDER_GIT, SharedString::from("New worktree")),
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
                let mark = match chip {
                    pane::BandChip::Project => ferrite_ui::icons::FOLDER,
                    _ => workspace_mark,
                };
                let chip_element = pane::band_chip(slot, mark, label, focused).on_mouse_down(
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

    /// The focused draft's model chip (#29), where a live Thread's stands
    /// and as it reads (Claude's new-session `Opus 5.5 Medium`): the
    /// provider's mark, the model, then the effort in force when the model
    /// takes one. It opens the model picker on the draft's band choice.
    fn draft_model_chip(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let draft = self.panes[index].draft()?;
        let provider = draft.binding.provider().provider;
        let model = draft.binding.provider().model.as_deref();
        let announced = self.cockpit.announced_models(provider);
        // The model's name, the provider's default until one is chosen.
        let model_label = SharedString::from(ferrite_core::providers::models::footer_name(
            provider, model, &announced,
        ));
        let effort = (!ferrite_core::providers::models::efforts_for(provider, model, &announced)
            .is_empty())
        .then(|| {
            effort_in_force(
                draft.binding.effort(),
                self.prefs.settings.effort_for(provider),
                provider,
                model,
                &announced,
            )
        })
        .flatten();
        let identity = self.panes[index].identity;
        let group = model_picker::chip_trigger(identity);
        let face = pane::model_effort_chip(pane::ModelChip {
            mark: ferrite_ui::provider::mark(provider),
            model: model_label,
            effort: effort.map(|effort| pane::sentence_case(&effort).into()),
            loading: false,
            open: self.model_picker_open(identity),
            group: group.clone(),
        });
        let control = pane::draft_picker(
            "draft-model-picker",
            draft.band_focus == Some(pane::BandChip::Provider),
            face,
            cx,
        )
        .tip("Model and effort");
        Some(self.model_chip_control(index, control, cx))
    }

    /// A draft's permission word, where a Thread's mode chip stands: the mode
    /// a new Thread starts in (Settings, `permission_mode_for`), read-only
    /// until the Session exists to change it.
    fn draft_mode_chip(&self, index: usize) -> Option<AnyElement> {
        let draft = self.panes[index].draft()?;
        let mode = self
            .prefs
            .settings
            .permission_mode_for(draft.binding.provider().provider)
            .to_string();
        let label = pane::permission_mode_label(&mode, &[]);
        Some(
            div()
                .id(("draft-mode", index))
                .debug_selector(move || format!("draft-mode-{index}"))
                .flex_shrink_0()
                .tooltip(ferrite_ui::menu::tooltip("Permission mode"))
                .child(pane::mode_chip(label, &mode))
                .into_any_element(),
        )
    }

    /// The Solo Composer's controls (SPEC §5.5, Claude's): the `+` that
    /// attaches files and the send circle on the box's row; under the box,
    /// where the work runs at the left (a draft's Project and Workspace
    /// pickers, an empty Thread's Project and branch), and at the right the
    /// model and effort and a Thread's permission mode as plain words, then
    /// the plan-usage ring. A board Pane's compact Composer (SPEC §5.9) has
    /// only the send circle, on the focused Pane; a Subagent's tab has none
    /// (its footer stands in). Either carries the queue strip's acts.
    fn composer_toolbar(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Option<pane::ComposerControls> {
        let pane = &self.panes[index];
        if !pane.is_main() {
            return None;
        }
        // The queue strip's acts, each row's own, aimed at its message by
        // its id: send now sends it next (`send_queued_now`), edit is ↑'s
        // act and remove ⌫'s — neither while Codex steers it.
        let queue: Vec<Vec<AnyElement>> = pane.thread().map_or_else(Vec::new, |thread| {
            let Some(open) = self.cockpit.thread(thread) else {
                return Vec::new();
            };
            let running = turn_running(&open);
            type Act = fn(&mut Cockpit, ThreadId, &str);
            open.queue()
                .iter()
                .enumerate()
                .map(|(at, item)| {
                    let takes_back = pane::queued_takes_back(item);
                    let acts: [(Option<gpui::component::button::Button>, Act); 3] = [
                        (
                            pane::queued_sends_now(item, running)
                                .then(|| pane::queued_send_now(at, cx)),
                            |cockpit, thread, id| cockpit.send_queued_now(thread, id),
                        ),
                        (
                            takes_back.then(|| pane::queued_edit(at, cx)),
                            |cockpit, thread, id| cockpit.edit_queued_item(thread, id),
                        ),
                        (
                            takes_back.then(|| pane::queued_close(at, cx)),
                            |cockpit, thread, id| cockpit.remove_queued_item(thread, id, false),
                        ),
                    ];
                    acts.into_iter()
                        .filter_map(|(button, act)| {
                            let id = item.id.clone();
                            let button = button?.on_click(cx.listener(
                                move |view, _: &ClickEvent, window, cx| {
                                    cx.stop_propagation();
                                    view.focus_pane(index);
                                    let focus =
                                        view.panes[index].composer.read(cx).focus_handle(cx);
                                    window.focus(&focus, cx);
                                    act(&mut view.cockpit, thread, &id);
                                    view.facts.acted(&view.cockpit, thread);
                                    cx.notify();
                                },
                            ));
                            Some(button.into_any_element())
                        })
                        .collect()
                })
                .collect()
        });
        let attach = self.attach_menu(index, cx);
        let (env, mut words, mode, ring) = match pane.thread() {
            None => {
                // A draft has no Session to ask: its mode is the one a new
                // Thread starts in (Settings), and its ring the provider's
                // account limits, as the Thread it becomes will read them.
                let ring = self.panes[index].draft().and_then(|draft| {
                    pane::plan_usage_ring(
                        ("draft-usage-ring", index),
                        self.cockpit
                            .account_limits(draft.binding.provider().provider),
                        None,
                        self.usage_popover_held,
                        cx,
                    )
                });
                (
                    Some(self.draft_band_chips(index, cx)),
                    self.draft_model_chip(index, cx)
                        .into_iter()
                        .collect::<Vec<_>>(),
                    self.draft_mode_chip(index),
                    ring.map(IntoElement::into_any_element),
                )
            }
            Some(thread) => {
                let open = self.cockpit.thread(thread);
                // An empty Thread says where its work runs, as a draft
                // does; its Project and checkout are settled, so as words.
                let empty = open
                    .as_ref()
                    .is_some_and(|open| pane::offers_import(Some(open.transcript())));
                let env = self.facts.get(thread).filter(|_| empty).map(|facts| {
                    [
                        (ferrite_ui::icons::FOLDER, facts.project_label.clone()),
                        (ferrite_ui::icons::BRANCH, facts.branch.clone()),
                    ]
                    .into_iter()
                    .filter_map(|(mark, word)| {
                        word.map(|word| pane::env_word(mark, word).into_any_element())
                    })
                    .collect()
                });
                let ring = open.and_then(|open| {
                    pane::plan_usage_ring(
                        ("plan-usage-ring", thread.get() as usize),
                        self.cockpit.account_limits(open.provider()),
                        open.transcript().plan().map(SharedString::from),
                        self.usage_popover_held,
                        cx,
                    )
                });
                (
                    env,
                    self.model_chip(index, cx).into_iter().collect(),
                    self.mode_chip(index, cx),
                    ring.map(IntoElement::into_any_element),
                )
            }
        };
        // The model's word gives way first; the effort and the mode hold.
        let model = (!words.is_empty()).then(|| words.remove(0));
        words.extend(mode);
        Some(pane::ComposerControls {
            attach: Some(attach),
            stop: self.stop_button(index, cx),
            send: self.send_button(index, cx),
            footer: Some(pane::composer_footer(model, words, ring).into_any_element()),
            env: env
                .filter(|chips: &Vec<AnyElement>| !chips.is_empty())
                .map(|chips| pane::env_strip(chips).into_any_element()),
            queue,
        })
    }

    /// Whether this Pane's turn runs, and whether its line is empty: what
    /// decides its primary actions (`pane::primary_actions`).
    fn running_and_empty(&self, index: usize, cx: &gpui::App) -> (bool, bool) {
        let pane = &self.panes[index];
        let running = pane
            .thread()
            .and_then(|thread| self.cockpit.thread(thread))
            .is_some_and(|open| turn_running(&open));
        (running, pane.composer.read(cx).is_empty())
    }

    /// The send circle: Enter's act, steering a running turn. The box's
    /// stands only once there is something to send (an empty box shows
    /// none).
    fn send_button(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (running, empty) = self.running_and_empty(index, cx);
        if !pane::primary_actions(running, empty).send {
            return None;
        }
        let button = pane::send_button(running, empty, cx)
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                view.focus_pane(index);
                let focus = view.panes[index].composer.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
                view.submit(&Submit, window, cx);
            }))
            .into_any_element();
        Some(button)
    }

    /// Stop, while the turn runs: Esc's act, which keeps what is typed.
    fn stop_button(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (running, empty) = self.running_and_empty(index, cx);
        if !pane::primary_actions(running, empty).stop {
            return None;
        }
        let button = pane::stop_button(cx)
            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                view.focus_pane(index);
                let focus = view.panes[index].composer.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
                if let Some(thread) = view.panes[index].thread() {
                    view.cockpit.interrupt(thread);
                    view.facts.acted(&view.cockpit, thread);
                    cx.notify();
                }
            }))
            .into_any_element();
        Some(button)
    }

    /// The box's `+` and its menu (Claude's): add files or photos, add a
    /// folder, or start a slash command — the `/` menu on the line; then,
    /// for a Thread whose Session reports them, `Connectors ›` and
    /// `Plugins ›` (`attach_submenus`).
    fn attach_menu(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let identity = self.panes[index].identity;
        let choices = [
            ("Add files or photos", ferrite_ui::icons::PAPERCLIP),
            ("Add folder", ferrite_ui::icons::FOLDER),
            ("Slash commands", ferrite_ui::icons::SQUARE_SLASH),
        ]
        .into_iter()
        .enumerate()
        .map(|(at, (label, mark))| ferrite_ui::components::Choice {
            label: label.into(),
            icon: Some((mark, ferrite_ui::theme::INK_STRONG)),
            // The key that does it from anywhere (the menu's digits are
            // the modes'; this one is a chord).
            shortcut: (at == 0)
                .then(|| ferrite_ui::components::bound_chord("cockpit::AttachFiles"))
                .flatten()
                .map(SharedString::from),
            ..Default::default()
        })
        .collect();
        let weak = cx.entity().downgrade();
        let picker = weak.clone();
        ferrite_ui::menu::ChoiceMenu {
            submenus: self.attach_submenus(index, cx),
            width: Some(ferrite_ui::theme::ATTACH_MENU_W),
            hang_left: true,
            id: SharedString::from(format!("attach-menu-{index}")),
            trigger: pane::attach_button(cx),
            choices,
            open: self.attach_menu == Some(identity),
            return_focus: self.panes[index].composer.focus_handle(cx),
            on_open: std::rc::Rc::new(move |open, _, cx| {
                let _ = weak.update(cx, |view, cx| {
                    if open {
                        view.focus_pane(index);
                        view.popover = None;
                        view.close_popup(cx);
                        view.attach_menu = Some(identity);
                    } else if view.attach_menu == Some(identity) {
                        view.attach_menu = None;
                    }
                    cx.notify();
                });
            }),
            on_pick: std::rc::Rc::new(move |at, _, cx| {
                let _ = picker.update(cx, |view, cx| {
                    view.attach_menu = None;
                    match at {
                        0 => view.attach_files(index, false, cx),
                        1 => view.attach_files(index, true, cx),
                        _ => view.panes[index]
                            .composer
                            .clone()
                            .update(cx, |composer, cx| composer.insert("/", cx)),
                    }
                    cx.notify();
                });
            }),
            // Over the box, as the `/` menu stands, hanging from the `+`'s
            // left edge (Claude's), never out over the sidebar.
            place: Some(match self.panes[index].geometry.get().composer {
                Some(field) => ferrite_ui::components::FloatPlace {
                    floor: f32::from(field.top()),
                    limit_right: f32::from(field.right()),
                },
                None => self.bar_place(),
            }),
        }
        .into_any_element()
    }

    /// The `+` menu's submenus, each only when the Pane's Thread has
    /// something to put in it (Claude's rows, as far as Ferrite can back
    /// them):
    ///
    /// - `Connectors ›`: the Session's MCP servers, a troubled one's state
    ///   as its tag; a pick signs in to one that waits on it, reconnects a
    ///   failed one, else opens the panel, where Connectors live — as
    ///   `Manage connectors` does.
    /// - `Plugins ›`: the plugins the Session's commands come from, as the
    ///   provider says (`CommandSource::Plugin`), each opening its own
    ///   commands; a pick starts `/plugin:command ` on the line as a chip.
    ///
    /// Claude's `+` menu reaches nothing else Ferrite could back: a
    /// plugin the CLI loads with no commands has nothing to pick.
    fn attach_submenus(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Vec<ferrite_ui::menu::Submenu> {
        use ferrite_core::{ControlKind, McpStatus, SessionControl};
        let Some(thread) = self.panes[index].thread() else {
            return Vec::new();
        };
        let Some(open) = self.cockpit.thread(thread) else {
            return Vec::new();
        };
        let generation = open.generation();
        let mut submenus = Vec::new();
        let servers = open.transcript().mcp_servers().to_vec();
        if !servers.is_empty() {
            // What a pick on each server does.
            let acts: Vec<Option<SessionControl>> = servers
                .iter()
                .map(|server| match server.status {
                    McpStatus::NeedsAuth if open.supports_control(ControlKind::LoginMcp) => {
                        Some(SessionControl::LoginMcp {
                            server: server.name.clone(),
                        })
                    }
                    McpStatus::Failed if open.supports_control(ControlKind::ReconnectMcp) => {
                        Some(SessionControl::ReconnectMcp {
                            server: server.name.clone(),
                        })
                    }
                    _ => None,
                })
                .collect();
            let choices = servers
                .iter()
                .map(|server| ferrite_ui::components::Choice {
                    label: server.name.clone().into(),
                    badge: match server.status {
                        McpStatus::Connected => None,
                        McpStatus::Connecting => Some("connecting".into()),
                        McpStatus::NeedsAuth => Some("needs sign-in".into()),
                        McpStatus::Failed => Some("failed".into()),
                        McpStatus::Disabled => Some("disabled".into()),
                        McpStatus::Unknown => None,
                    },
                    why: server.error.clone().map(SharedString::from),
                    ..Default::default()
                })
                .chain(std::iter::once(ferrite_ui::components::Choice {
                    label: "Manage connectors".into(),
                    ..Default::default()
                }))
                .collect();
            let view = cx.entity().downgrade();
            submenus.push(ferrite_ui::menu::Submenu {
                icon: Some((ferrite_ui::icons::BLOCKS, ferrite_ui::theme::INK_STRONG)),
                label: "Connectors".into(),
                value: SharedString::default(),
                choices,
                submenus: Vec::new(),
                width: ferrite_ui::theme::ATTACH_MENU_W,
                on_pick: std::rc::Rc::new(move |at, _, cx| {
                    let act = acts.get(at).cloned().flatten();
                    let _ = view.update(cx, |view, cx| {
                        view.attach_menu = None;
                        match act {
                            Some(act) => view.run_session_control(thread, generation, act),
                            None => view.open_panel(thread, cx),
                        }
                        cx.notify();
                    });
                }),
            });
        }
        let plugins = plugins(open.commands());
        if !plugins.is_empty() {
            let view = cx.entity().downgrade();
            let submenus_per_plugin = plugins
                .into_iter()
                .map(|(plugin, commands)| {
                    let names: Vec<String> = commands
                        .iter()
                        .map(|command| command.name.clone())
                        .collect();
                    let choices = commands
                        .iter()
                        .map(|command| ferrite_ui::components::Choice {
                            // The command as the plugin names it: the
                            // submenu already says whose it is.
                            label: command
                                .name
                                .strip_prefix(&format!("{plugin}:"))
                                .unwrap_or(&command.name)
                                .to_string()
                                .into(),
                            why: (!command.description.is_empty())
                                .then(|| command.description.clone().into()),
                            ..Default::default()
                        })
                        .collect();
                    let view = view.clone();
                    ferrite_ui::menu::Submenu {
                        icon: None,
                        label: plugin.into(),
                        value: SharedString::default(),
                        choices,
                        submenus: Vec::new(),
                        width: ferrite_ui::theme::ATTACH_MENU_W,
                        // A pick starts the command on the line, a chip
                        // as the `/` menu's pick is.
                        on_pick: std::rc::Rc::new(move |at, _, cx| {
                            let Some(name) = names.get(at) else {
                                return;
                            };
                            let token = format!("/{name}");
                            let _ = view.update(cx, |view, cx| {
                                view.attach_menu = None;
                                view.panes[index]
                                    .composer
                                    .clone()
                                    .update(cx, |composer, cx| {
                                        composer.insert(&format!("{token} "), cx);
                                        composer.stage_chip(SharedString::from(token), cx);
                                    });
                                cx.notify();
                            });
                        }),
                    }
                })
                .collect();
            submenus.push(ferrite_ui::menu::Submenu {
                icon: Some((ferrite_ui::icons::PLUG, ferrite_ui::theme::INK_STRONG)),
                label: "Plugins".into(),
                value: SharedString::default(),
                choices: Vec::new(),
                submenus: submenus_per_plugin,
                width: ferrite_ui::theme::ATTACH_MENU_W,
                // Its rows are the plugins' submenus: nothing to pick here.
                on_pick: std::rc::Rc::new(|_: usize, _: &mut Window, _: &mut gpui::App| {}),
            });
        }
        submenus
    }

    /// The `+` menu's file or folder picker, its picks staged on this
    /// Pane's Composer as a drop or a paste stages them.
    fn attach_files(&mut self, index: usize, folders: bool, cx: &mut Context<Self>) {
        let composer = self.panes[index].composer.clone();
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: !folders,
            directories: folders,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn(async move |_, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            composer.update(cx, |composer, cx| composer.add_local_files(&paths, cx));
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
        // The model picker hangs over the box (`model_picker_element`), and
        // a draft's band menus off their chips in the bar
        // (`band_menu_element`).
        if matches!(open.kind, Kind::Provider(_) | Kind::Band(_)) {
            return None;
        }
        Some(self.menu_surface(open, cx))
    }

    /// A draft's Project or Workspace menu, hung off its chip on the strip
    /// over the box (ADR 0014): its left edge at the chip's, its foot
    /// `FLOAT_OFFSET` over the chip — or, opened from the hero's project
    /// word, `FLOAT_OFFSET` under it — kept inside the window's margin. The chip has been
    /// drawn before it can be pressed, so its bounds are always recorded.
    fn band_menu_element(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        use ferrite_ui::theme::{
            FLOAT_OFFSET, GRID_PAD, MENU_WINDOW_MARGIN, SLASH_MENU_W, WORKSPACE_MENU_W,
        };
        let open = self.popover.as_ref()?;
        let Kind::Band(chip @ (pane::BandChip::Project | pane::BandChip::Workspace)) = open.kind
        else {
            return None;
        };
        let hero = self.band_from_hero && chip == pane::BandChip::Project;
        let key = if hero {
            HERO_TRIGGER.into()
        } else {
            band_trigger(chip)
        };
        let trigger = self.float_triggers.borrow().get(key.as_ref()).copied()?;
        // Above its chip on the strip over the box; or below the hero's
        // project word, as Codex's switcher drops.
        let (anchor, at) = if hero {
            (
                gpui::Anchor::TopLeft,
                gpui::point(trigger.left(), trigger.bottom() + px(FLOAT_OFFSET)),
            )
        } else {
            (
                gpui::Anchor::BottomLeft,
                gpui::point(trigger.left(), trigger.top() - px(FLOAT_OFFSET)),
            )
        };
        // T3 widens the Workspace menu to 336 once it offers a previous
        // worktree, whose branch rides in the row's name.
        let previous = chip == pane::BandChip::Workspace
            && open.rows.iter().any(|row| {
                matches!(
                    row.consequence,
                    Consequence::Band(BandChoice::Target(DraftTarget::Existing { .. }))
                )
            });
        let width = if previous { WORKSPACE_MENU_W } else { SLASH_MENU_W }
            .min(f32::from(window.viewport_size().width) - 2. * MENU_WINDOW_MARGIN);
        Some(
            deferred(
                anchored()
                    .anchor(anchor)
                    .position(at)
                    .snap_to_window_with_margin(px(GRID_PAD))
                    .child(
                        div()
                            .debug_selector(|| "composer-menu".into())
                            .w(px(width))
                            .child(self.menu_surface(open, cx)),
                    ),
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
    /// ("Searching workspace files...", a refusal) is a `menu_note`. The rows scroll
    /// inside the popover's cap.
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
        // the longest name shown, measured in the row's face (a match
        // draws at `W_LABEL`), so no name is ever cut.
        let label_w = matches!(open.kind, Kind::Commands).then(|| {
            let measure = nav::Measure::Text(cx, self.nav_expanded_width());
            open.rows
                .iter()
                .map(|row| {
                    measure.ui(
                        &row.name,
                        ferrite_ui::theme::FS_UI,
                        ferrite_ui::theme::W_LABEL,
                    )
                })
                .fold(0., f32::max)
                .ceil()
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
                Kind::Band(pane::BandChip::Provider)
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
                Kind::Provider(_) | Kind::Band(_) => wire(
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
        // it: swallowed, so the root's dismissal never sees it. No key-hint
        // footer: the keys act in place (Claude's and Codex's menus).
        // Claude's slash picker explains the row under the cursor in a card
        // at its side (claude/slash-picker.png), so a description cut by the
        // popover's width is still read, by key as well as by pointer. The
        // card stands level with the row, rows being one grid row each.
        let explained = matches!(open.kind, Kind::Commands)
            .then(|| open.rows.get(open.selected))
            .flatten()
            .filter(|row| !row.row.inert && !row.row.detail.is_empty())
            .map(|row| row.row.detail.clone());
        let row_top = open.selected as f32 * ferrite_ui::theme::FLOAT_ROW_H
            + f32::from(open.scroll.offset().y);
        let cap = if open.kind.follows_text() {
            ferrite_ui::theme::SLASH_MENU_MAX_H
        } else {
            ferrite_ui::theme::MENU_MAX_H
        };
        let popover = pane::menu_popover(cap)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(rows);
        div()
            .relative()
            .w_full()
            .child(popover)
            .children(explained.map(|detail| {
                let card = ferrite_ui::menu::float()
                    .debug_selector(|| "composer-menu-tip".into())
                    .w(px(ferrite_ui::theme::TOOLTIP_MAX_W))
                    .px(px(ferrite_ui::theme::TOOLTIP_PAD_X))
                    .py(px(ferrite_ui::theme::TOOLTIP_PAD_Y))
                    .text_size(px(ferrite_ui::theme::FS_META))
                    .line_height(px(ferrite_ui::theme::LH_META))
                    .text_color(rgb(ferrite_ui::theme::INK_STRONG))
                    .child(detail);
                // A drawer as wide as the box has no room on its right.
                crate::float::beside(
                    ferrite_ui::theme::SUBMENU_GAP,
                    row_top.max(0.),
                    ferrite_ui::theme::MENU_WINDOW_MARGIN,
                    card.into_any_element(),
                )
            }))
            .into_any_element()
    }

    /// The nav's per-frame state (ADR 0040), from caches and O(1) reads
    /// only; the Project filter narrows it and nothing else. Every open
    /// Thread the filter admits is in one section, by priority: snoozed,
    /// pinned, working, else active. Pinned keeps pin order; Active puts
    /// pending Decisions first, then the needs-you queue's order (oldest
    /// request first); Working the newest send first; Snoozed the soonest
    /// to wake first; Parked the park order. Drafts are not rows (#29).
    ///
    /// The one selection is the shown Thread. The empty board shows none.
    fn nav_state(&self) -> nav::NavState {
        let roster = self.cockpit.roster();
        let shown = (!self.empty_board_shown())
            .then(|| roster.focused_thread())
            .flatten();
        let row = |thread: ThreadId| nav::ThreadRow {
            selected: shown == Some(thread),
            ..self.thread_row(thread)
        };
        let mut threads: Vec<ThreadId> = self
            .panes
            .iter()
            .filter_map(|pane| pane.thread())
            .filter(|thread| self.cockpit.thread(*thread).is_some() && self.admitted(*thread))
            .collect();
        threads.sort();
        threads.dedup();
        let pins = &self.prefs.settings.pinned_threads;
        let mut state = nav::NavState::default();
        let mut pinned = Vec::new();
        let mut snoozed = Vec::new();
        for thread in threads {
            let open = self.cockpit.thread(thread);
            let snooze = open.and_then(|open| open.snooze()).map(|snooze| snooze.until);
            let working = open.is_some_and(|open| {
                open.pending_decisions().is_empty()
                    && (open.needs_queue() || open.background_work() || !open.queue().is_empty())
            });
            match (snooze, pins.iter().position(|pin| *pin == thread)) {
                (Some(until), _) => snoozed.push((until, row(thread))),
                (None, Some(at)) => pinned.push((at, row(thread))),
                (None, None) if working => state.working.push(row(thread)),
                (None, None) => state.active.push(row(thread)),
            }
        }
        pinned.sort_by_key(|(at, _)| *at);
        state.pinned = pinned.into_iter().map(|(_, row)| row).collect();
        snoozed.sort_by_key(|(until, _)| *until);
        state.snoozed = snoozed.into_iter().map(|(_, row)| row).collect();
        let queue = self.cockpit.needs_you();
        state.active.sort_by_key(|row| {
            (
                self.cockpit
                    .thread(row.thread)
                    .is_none_or(|open| open.pending_decisions().is_empty()),
                queue
                    .iter()
                    .position(|id| *id == row.thread)
                    .unwrap_or(usize::MAX),
            )
        });
        state
            .working
            .sort_by_key(|row| std::cmp::Reverse(row.working_since));
        state.parked = self.parked_threads().into_iter().map(row).collect();
        state
    }

    /// Does the Project filter admit this Thread?    /// Does the Project filter admit this Thread? `All projects` admits
    /// everything; a chosen Project admits only Threads whose cached
    /// ProjectId is it — so a Thread whose Project is unknown appears under
    /// `All projects` alone, rather than being quietly filed under someone
    /// else's Project.
    fn admitted(&self, thread: ThreadId) -> bool {
        // An owned Thread (a Worker, a Coordinator) is its owner's to list.
        if !self.cockpit.operators(thread) {
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
        let working = matches!(
            status,
            nav::RowStatus::Working | nav::RowStatus::Failing
        );
        let row = nav::ThreadRow {
            thread,
            name: self.facts.name(thread),
            status,
            project: facts.and_then(|facts| facts.project_label.clone()),
            selected: false,
            tail: nav::NavTail::of(status, slot.as_ref(), age),
            branch: facts.and_then(Self::checkout_branch),
            worktree: facts.and_then(|facts| facts.worktree.clone()),
            pr: self.facts.pr(thread).map(nav::NavPr::of),
            provider: open
                .map(|open| open.provider())
                .or_else(|| facts.and_then(|facts| facts.provider)),
            activity_at: facts.and_then(|facts| facts.last_used),
            working_since: open
                .filter(|_| working)
                .and_then(|open| open.turn().started),
            pinned: self.prefs.settings.pinned_threads.contains(&thread),
            retitling: self.titling.contains(&thread),
            // The nav wears its own picks (`NavView::card_picked`).
            picked: false,
        };
        #[cfg(test)]
        drawn::record_row(
            thread,
            format!(
                "{:?} {:?} branch={:?} pr={:?} provider={:?} receded={} {}",
                row.status,
                row.tail,
                row.branch,
                row.pr.as_ref().map(|pr| (pr.number, pr.state)),
                row.provider,
                row.receded(),
                row.name,
            ),
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

    fn land_on_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if let Some(owner) = self.cockpit.owner(thread) {
            self.land_on_owned(thread, owner, cx);
            return;
        }
        if self.pane_for(thread).is_some() {
            self.focus_thread(thread, cx);
        } else {
            self.revive_thread(thread, cx);
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
    /// that shows it — the core's one door, like every other deliberate
    /// move.
    fn focus_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.leave_destination();
        if self.cockpit.focus_thread(thread) {
            if let Some(index) = self.pane_for(thread) {
                self.focus_pane(index);
            }
            self.widen_scope_to(thread, cx);
            cx.notify();
        }
    }

    /// The operator went to a Thread the nav's Project scope hides: the
    /// scope widens to all Projects, so the shown Thread has its row in the
    /// list (and the Filter button says why). Owned Threads are never
    /// listed, so nothing here reveals them.
    fn widen_scope_to(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if self.nav_filter.is_some() && self.cockpit.operators(thread) && !self.admitted(thread) {
            self.choose_nav_filter(None, cx);
        }
    }

    /// Revive one parked Thread into the view that shows it: the shared
    /// tail of cmd-o and a parked nav row's click (#21). Whether it came
    /// back; when it did not, the nav says so.
    fn revive_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) -> bool {
        let revived = self.try_revive(thread, cx);
        let outcome = nav_undo::Outcome {
            thread,
            result: revived.as_ref().map(drop).map_err(|e| e.to_string()),
        };
        self.report_failures("reopen", &[outcome]);
        cx.notify();
        revived.is_ok()
    }

    /// `revive_thread` without the words, for a caller with several to
    /// tell. The board's own state gives way only to a Thread that is back.
    fn try_revive(
        &mut self,
        thread: ThreadId,
        cx: &mut Context<Self>,
    ) -> std::io::Result<()> {
        self.cockpit.reopen(thread)?;
        self.leave_destination();
        self.widen_scope_to(thread, cx);
        self.sync_panes(cx);
        cx.notify();
        Ok(())
    }

    /// Focus by grid index — the pointer's and the tests' spelling of the
    /// core's one door.
    fn focus_pane(&mut self, index: usize) {
        if let Some(identity) = self.panes.get(index).map(|pane| pane.identity) {
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
        self.open_draft(DraftTarget::New, cx);
    }

    /// cmd-t / cmd-n (#29): a draft Pane, not a Thread. The band chooses;
    /// the first send bootstraps.
    fn new_thread(&mut self, _: &NewThread, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_draft(DraftTarget::Main, cx);
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

    /// Reopen the Thread parked most recently — the one the operator just
    /// closed, which is the one they want back (#17); the core walks the
    /// park order, then creation order.
    fn reopen_thread(&mut self, _: &ReopenThread, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some((thread, result)) = self.cockpit.reopen_last() {
            let outcome = nav_undo::Outcome {
                thread,
                result: result.map_err(|e| e.to_string()),
            };
            if outcome.result.is_ok() {
                self.widen_scope_to(thread, cx);
                self.sync_panes(cx);
            }
            self.report_failures("reopen", &[outcome]);
            cx.notify();
        }
    }

    /// Close the focused Pane — cmd-w. The core decides what that means
    /// (park, leave, defer onto a draft, discard); the mirror follows.
    fn close_thread(&mut self, _: &CloseThread, window: &mut Window, cx: &mut Context<Self>) {
        if self.nav_view.read(cx).holds_keys(window) {
            self.park_nav_cursor(cx);
            return;
        }
        // ⌘W over the lightbox closes it; in the focused Pane's preview or
        // Review it closes that, not the Pane (F-14).
        if let Some((index, _)) = self.lightbox() {
            self.close_lightbox(index, cx);
            return;
        }
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
        if self.close_panel_in_focus(window, cx) {
            return;
        }
        let Some(identity) = self.cockpit.roster().focused() else {
            return;
        };
        self.close_pane(identity, cx);
    }

    /// Close one Pane through the core act (a Thread always parks), its
    /// refusal landing in the nav's notice — and a park that would not
    /// flush is logged, the Pane gone either way.
    fn close_pane(&mut self, identity: PaneIdentity, cx: &mut Context<Self>) {
        let result = self.cockpit.close(identity);
        let closed = result.is_ok();
        if let Err(error) = result {
            ferrite_core::log!("pane {identity:?} did not close cleanly: {error}");
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
        if let (true, PaneIdentity::Draft(draft)) = (closed, identity) {
            self.discard_draft(draft, cx);
        }
        cx.notify();
    }

    /// Jump to the next Thread waiting on the operator.
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
    /// through `focus_pane`, the door #21's nav clicks share: the per-frame
    /// snap in render then
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

    /// Cite the selection from the keyboard, as the button beside it does.
    fn cite_selection(&mut self, _: &CiteSelection, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(transcript) = self.panes[self.focused()].transcript() {
            transcript.update(cx, |transcript, cx| transcript.cite_selection(window, cx));
        }
    }

    /// ⌘V anywhere in the window lands in the focused Pane's Composer.
    /// The Composer's own Paste runs first while it holds the keyboard;
    /// this is the fallback for when a click in the transcript — a drag
    /// to select, a tool row — took the keyboard away: pasted text or files
    /// go where the operator is about to type, and the keyboard follows.
    fn paste_into_composer(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.snooze_editor.as_ref() {
            if let Some(item) = cx.read_from_clipboard() {
                editor.line.update(cx, |line, cx| line.paste_item(item, cx));
            }
            return;
        }
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

/// T3's empty Composer menus' words.
const NO_COMMAND: &str = "No matching command.";
const NO_FILES: &str = "No matching files or folders.";

/// A Composer menu's one inert line: what it is searching, or that
/// nothing matched.
fn note_row(note: &'static str) -> Row {
    Row {
        row: pane::MenuRow {
            insert: "".into(),
            name: note.into(),
            detail: "".into(),
            matched: Vec::new(),
            prose_detail: true,
            inert: true,
            tag: None,
        },
        active: false,
        consequence: Consequence::Inert,
    }
}

/// A Composer menu holding only `note_row(note)`.
fn note_popover(pane: PaneIdentity, note: &'static str) -> Popover {
    Popover {
        scroll: ScrollHandle::new(),
        revealed: Default::default(),
        hold: None,
        pane,
        kind: Kind::Commands,
        rows: vec![note_row(note)],
        selected: 0,
    }
}

/// The `$` token the caret sits in (T3's skill trigger): the `$`'s byte
/// offset and the filter typed after it. As `@`, the `$` opens a token —
/// start of line or after whitespace — and a digit after it is a price,
/// not a skill (`$5`).
fn skill_token(text: &str, cursor: usize) -> Option<(usize, &str)> {
    let head = text.get(..cursor)?;
    let at = head.rfind('$')?;
    let filter = &head[at + 1..];
    if filter.contains(char::is_whitespace) || filter.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }
    let opens_token = at == 0 || text[..at].ends_with(char::is_whitespace);
    opens_token.then_some((at, filter))
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
/// filter, best first (ties keep the provider's order), capped, each
/// tagged with where it comes from.
fn command_rows(commands: &[ferrite_core::SessionCommand], filter: &str) -> Vec<pane::MenuRow> {
    let mut scored: Vec<(i64, pane::MenuRow)> = commands
        .iter()
        .filter_map(|command| {
            let (score, matched) = crate::fuzzy::matches(filter, &command.name)?;
            Some((
                score,
                pane::MenuRow {
                    insert: SharedString::from(command.name.clone()),
                    // The bare name, as Claude's menu lists it: the `/` is
                    // already on the line.
                    name: SharedString::from(command.name.clone()),
                    matched,
                    detail: SharedString::from(command.description.clone()),
                    prose_detail: true,
                    inert: false,
                    // Where it comes from, as the provider says.
                    tag: command
                        .source
                        .as_ref()
                        .and_then(ferrite_core::CommandSource::tag)
                        .map(SharedString::from),
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
                    tag: None,
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
                tag: None,
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
/// filter as every row.
fn local_row(
    filter: &str,
    name: &'static str,
    detail: &'static str,
    inert: bool,
) -> Option<pane::MenuRow> {
    let (_, matched) = crate::fuzzy::matches(filter, name)?;
    Some(pane::MenuRow {
        insert: SharedString::from(name),
        name: SharedString::from(name),
        matched,
        detail: SharedString::from(detail),
        prose_detail: true,
        inert,
        tag: None,
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
            tag: None,
        },
        active,
        consequence: Consequence::Band(choice),
    }
}

/// The Provider's name as a person says it — the picker's section titles
/// and the chip before any model is known.
/// The plugins a Session's commands come from, each with its commands,
/// in their first command's order: what the provider says is a plugin's
/// (`CommandSource::Plugin`).
fn plugins(
    commands: &[ferrite_core::SessionCommand],
) -> Vec<(String, Vec<&ferrite_core::SessionCommand>)> {
    let mut plugins: Vec<(String, Vec<&ferrite_core::SessionCommand>)> = Vec::new();
    for command in commands {
        let Some(plugin) = command.source.as_ref().and_then(|source| source.plugin()) else {
            continue;
        };
        match plugins.iter_mut().find(|(name, _)| name == plugin) {
            Some((_, listed)) => listed.push(command),
            None => plugins.push((plugin.to_string(), vec![command])),
        }
    }
    plugins
}

/// A Settings group for one Provider: its name, led by its logomark in
/// brand colour.
fn provider_group(provider: Provider) -> settings::Group {
    let (mark, ink) = ferrite_ui::provider::mark(provider);
    settings::Group::new(Some(ferrite_ui::provider::title(provider))).mark(mark, ink)
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
/// The start hero's project word, as a float trigger (`draft_hero`).
const HERO_TRIGGER: &str = "start-hero-project";

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

/// The effort in force (FL-13): the chip's word (`effort_value`), else the
/// level the CLI itself runs at (`models::cli_default_effort`).
fn effort_in_force(
    chosen: Option<&str>,
    saved: Option<&str>,
    provider: Provider,
    model: Option<&str>,
    announced: &[ferrite_core::ModelInfo],
) -> Option<SharedString> {
    effort_value(chosen, saved, provider, model, announced).or_else(|| {
        ferrite_core::providers::models::cli_default_effort(provider, model).map(effort_chip_label)
    })
}

/// A model's effort ladder: its wire values, the words the chip prints,
/// and the step in force (`effort_in_force`). `None` for a model that
/// takes no effort.
fn effort_ladder_of(
    chosen: Option<&str>,
    saved: Option<&str>,
    provider: Provider,
    model: Option<&str>,
    announced: &[ferrite_core::ModelInfo],
) -> Option<(Vec<String>, Vec<SharedString>, Option<usize>)> {
    let steps = ferrite_core::providers::models::efforts_for(provider, model, announced);
    if steps.is_empty() {
        return None;
    }
    let labels: Vec<SharedString> = steps.iter().map(|step| effort_chip_label(step)).collect();
    let current = effort_in_force(chosen, saved, provider, model, announced);
    let chosen = current.and_then(|current| labels.iter().position(|label| *label == current));
    Some((steps, labels, chosen))
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
        if let Some((_, editor)) = &self.rename {
            let line = editor.read(cx).focus_handle(cx);
            if !line.is_focused(window) {
                window.focus(&line, cx);
            }
        }
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
        self.hear_nav(window, cx);
        self.tend_history();
        self.tend_popup_focus(window, cx);
        let root = self.render_cockpit(window, cx);
        // The motion tail: the cockpit is the window's root view, so this
        // runs once per frame, after every hover blend has been read. A
        // hover blend, a press or a reader's slide mid-flight keeps frames
        // coming; with none, nothing is scheduled (the pulse clock drives
        // loops). Each reads what this frame drew, never the clock again: a
        // slide that ended between the draw and here still asks for the one
        // frame that lands it at rest.
        let sliding =
            self.reader_slides.moving() || self.columns.moving() || self.tabs_scrolling(cx);
        let fading = ferrite_ui::motion::hover_fades_active();
        if fading || sliding {
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
    /// `12s` (its working line and head), a nav row's `2m` — ride the
    /// pulse clock (`motion::ride`) to their next turn: while a loop runs
    /// they change on its grid, as its fixed rate redrew them; otherwise the
    /// sweep brings them up to date.
    fn ride_clock_text(&mut self, cx: &mut Context<Self>) {
        let wall = ferrite_core::clock::system_time();
        // Every age the Cockpit can show: a Thread's (the nav, a parked
        // Pane, the palette), a notice's and a waiting request's (the
        // Inbox, its toasts).
        let notifications = self.cockpit.notifications();
        let notices = notifications
            .notices()
            .map(|notice| notice.at)
            .chain(notifications.decisions().map(|request| request.at))
            .filter_map(|at| crate::facts::since_label_changes_in(at, wall))
            .min();
        let thread_ages = self.facts.next_age_change(wall);
        // The sweep redraws a window nothing else draws when an age turns:
        // a Thread's everywhere, a notice's in the frame.
        self.ages_turn_at = thread_ages.map(|next| wall + next);
        self.frame_turn_at = notices.map(|next| wall + next);
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
        let frame = notices;
        let mut everything = nav.into_iter().chain(frame).min();
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
                    .view_id(pane.identity);
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
        if let Some(next) = nav {
            ferrite_ui::motion::ride(self.nav_view.entity_id(), now + next, cx);
        }
        if let Some(frame) = frame {
            ferrite_ui::motion::ride(self.frame_tick_id(), now + frame, cx);
        }
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
        // A checkout's file goes to Files before the Panel tends its tabs,
        // so it never stands as a File tab of its own.
        self.redirect_previews_to_files(cx);
        self.tend_panel(window, cx);
        self.measure_nav_width(window, cx);
        self.update_auto_fold(window);
        self.tend_columns(cx);
        self.tend_details(window, cx);
        if self.nav_folded() {
            self.snooze_editor = None;
        }
        self.present_cli_updates(window, cx);
        self.present_ferrite_update(window, cx);
        self.present_server(window, cx);
        self.present_restart(window, cx);
        self.present_unreadable_settings(window, cx);
        self.present_user_commands(window, cx);
        self.present_allowance_warnings(window, cx);
        self.present_asked(window, cx);
        self.retire_task_toasts(window, cx);
        self.maximized = window.is_maximized();
        // What this frame draws is what the window streams whole;
        // everything else reads its digest (ADR 0032).
        let drawn: Vec<ThreadId> = self
            .visible_indices()
            .into_iter()
            .filter_map(|index| self.panes[index].thread())
            .collect();
        // The open Task's Worker, drawn in its panel, streams whole too.
        self.leave_destination_if_moved();
        let drawn: Vec<ThreadId> = match self.factory_worker_drawn() {
            Some(worker) => vec![worker],
            None => drawn,
        };
        self.cockpit.draws_transcripts(drawn);

        for index in 0..self.panes.len() {
            let target = self.panes[index].targeted_tool().cloned();
            let target_is_rendered = target
                .as_ref()
                .is_none_or(|target| self.expandable_tools(index, cx).contains(target));
            if !target_is_rendered {
                self.panes[index].clear_tool_target();
            }
        }

        // This is intentionally cheap on ordinary frames: `sync_transcript`
        // compares scalar keys first, so Composer edits repaint Cockpit chrome
        // without cloning or notifying any retained transcript entity.
        self.sync_visible_transcripts(cx);
        self.tend_side_chat(window, cx);
        self.tend_terminal(window, cx);
        self.tend_agent_drills(cx);

        // The popover belongs to the focused Pane's Composer (#23):
        // leaving that Pane closes it here — and a
        // picker closes the moment its offer expires (#25): a band chip
        // (#29) goes with the draft becoming a Thread. Render is the one
        // chokepoint every open, pick, dismissal and heal passes.
        if self.popover.as_ref().is_some_and(|open| {
            let focused = self.panes.get(self.focused());
            focused.is_none_or(|pane| pane.identity != open.pane)
                || match &open.kind {
                    // The model picker outlives the first prompt (the
                    // model can always change); only its Thread going
                    // away closes it.
                    Kind::Provider(_) => match open.pane {
                        PaneIdentity::Thread(thread) => self.cockpit.thread(thread).is_none(),
                        PaneIdentity::Draft(_) => focused.is_some_and(|pane| pane.draft().is_none()),
                    },
                    Kind::Band(_) => focused.is_some_and(|pane| pane.draft().is_none()),
                    Kind::Commands | Kind::Files { .. } => false,
                }
        }) {
            self.popover = None;
        }
        let pane_rects = self.pane_rects(window);
        for (index, pane) in self.panes.iter().enumerate() {
            let row_limit = pane_rects
                .iter()
                .find(|(at, _)| *at == index)
                .map(|(_, rect)| {
                    let queued = pane
                        .thread()
                        .and_then(|thread| self.cockpit.thread(thread))
                        .map_or(0, |thread| thread.queue().len());
                    pane::composer_row_limit(rect.h, queued)
                })
                .unwrap_or(ferrite_pane::composer::MAX_ROWS);
            // While a question asks, the Composer keeps to two rows so the
            // question has the Pane; it is derived every frame, so answering
            // or changing Subjects gives the draft its rows back.
            let answering_question = pane
                .thread()
                .and_then(|thread| self.cockpit.thread(thread))
                .is_some_and(|open| {
                    open.pending_decisions().iter().any(|request| {
                        (request.subject.as_ref() == Some(&pane.selected)
                            || (request.subject.is_none() && pane.is_main()))
                            && pane::question_of(&request.decision).is_some()
                    })
                });
            let row_limit = if answering_question {
                row_limit.min(2)
            } else {
                row_limit
            };
            pane.composer
                .update(cx, |composer, cx| composer.set_visible_row_limit(row_limit, cx));
        }
        self.sync_composer_floats(cx);
        // Each Thread's line chips the `/name`s its Session knows, however
        // they got into the text (a draft's line keeps what its `/` menu
        // last discovered: `derive_menu`).
        for pane in &self.panes {
            if let Some(open) = pane.thread().and_then(|thread| self.cockpit.thread(thread)) {
                pane.composer.update(cx, |composer, cx| {
                    composer.set_commands(open.commands(), cx)
                });
            }
        }
        let history_available: Vec<bool> = (0..self.panes.len())
            .map(|index| self.history_available(index))
            .collect();
        for (pane, available) in self.panes.iter().zip(history_available) {
            pane.composer.update(cx, |composer, cx| {
                composer.set_history_available(available, cx)
            });
        }

        // Focus follows the operator, but only onto something the Pane
        // actually renders: focusing a Composer it never drew leaves the
        // keyboard pointing at nothing, and every global key stops working.
        // An empty cockpit still needs the keyboard: with nothing focused,
        // dispatch starts above these handlers and cmd-n could never make the
        // first Thread. A machine whose Server
        // went is read-only (ADR 0026): the root holds the keyboard, so no
        // Composer or control on its board takes a key until it is back.
        let lost = self.machine_lost();
        let wanted = self
            .rename
            .as_ref()
            .map(|(_, editor)| editor.focus_handle(cx))
            .or_else(|| {
                self.snooze_editor
                    .as_ref()
                    .map(|editor| editor.line.focus_handle(cx))
            })
            // The nav's search field keeps the keys it was handed.
            .or_else(|| {
                let nav = self.nav_view.read(cx);
                nav.search_focused(window, cx)
                    .then(|| nav.search.focus_handle(cx))
            })
            // An open Decision note (⇥) holds the keyboard until it closes.
            .or_else(|| self.decision_note_focus(cx))
            // The lightbox, over everything, keeps the keyboard its image
            // took; a review beside the focused Pane keeps its own, from its
            // first frame (a review's comment field's too).
            .or_else(|| {
                let (index, _) = self.lightbox()?;
                self.panes[index].preview.held_focus(window, cx)
            })
            .or_else(|| {
                let index = self.focused();
                if !self.preview_shown(index) { return None; }
                let pane = self.panes.get(index)?;
                if !pane.preview.document().is_some_and(|document| matches!(&document.body, ferrite_pane::preview::Body::Browser)) { return None; }
                self.browser_annotation_focus(pane.thread()?, cx)
            })
            .or_else(|| {
                self.panes
                    .get(self.focused())?
                    .review
                    .held_focus(window, cx)
            })
            .or_else(|| {
                let pane = self.panes.get(self.focused())?;
                // A Thread whose facts have not landed draws no Composer:
                // the root holds the keyboard until they do.
                if pane
                    .thread()
                    .is_some_and(|thread| self.cockpit.thread(thread).is_none())
                {
                    return None;
                }
                // The Composer keeps the keyboard even while a Decision
                // pends: the card is part of its stack and the input stays
                // live (PromptBox state 04).
                Some(if pane.has_tool_target() {
                    pane.tool_focus()
                } else if !pane.is_main() {
                    // A subagent's tab keeps the keyboard on its body.
                    pane.transcript_focus.clone()
                } else {
                    pane.composer.read(cx).focus_target(window, cx)
                })
            })
            .unwrap_or_else(|| self.focus.clone());
        // The empty board's list holds the keyboard while it shows (F-13):
        // its rows answer ↑ ↓ ⏎ under the `EmptyBoard` context.
        let wanted = if lost {
            self.focus.clone()
        } else if let Some(focus) = self.factory_focus(window, cx) {
            focus
        } else if self.rename.is_none() && self.prs_shown() {
            self.prs.focus.clone()
        } else if self.rename.is_none() && self.empty_board_shown() {
            self.empty_focus.clone()
        } else {
            wanted
        };
        use gpui::component::WindowExt as _;
        let native_text_focused = self
            .panes
            .get(self.focused())
            .is_some_and(|pane| pane.transcript_focus.contains_focused(window, cx))
            && (ferrite_pane::rich::controls_focused(window)
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
                        !pane.is_main()
                            || thread.pending_decisions().len() > 1
                            || thread.pending_decisions().iter().any(|request| {
                                request.subject.is_none() || asks_in_fields(&request.decision)
                            })
                    });
                controls
                    && pane.controls_focus.contains_focused(window, cx)
                    && !pane.controls_focus.is_focused(window)
                    && (!pane.is_main() || !pane.transcript_focus.is_focused(window))
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
        // The find bar's line and the Reply card's keep the keyboard while
        // the operator types.
        let find_focused = self.panes.get(self.focused()).is_some_and(|pane| {
            pane.preview_tab
                .find
                .as_ref()
                .is_some_and(|find| find.holds_focus(window, cx))
                || pane.transcript().is_some_and(|view| {
                    let view = view.read(cx);
                    view.find_holds_focus(window, cx)
                })
        });
        // The list keeps the keyboard it was handed until the operator
        // takes it elsewhere.
        let nav_list_focused = self.nav_view.read(cx).holds_keys(window);
        if self.closing_holds_focus(window, cx) {
            window.focus(&wanted, cx);
        }
        // So does a Panel body that reads its own keys: "Open a surface",
        // pressed, or a Files editor or filter being typed in.
        let panel_keys_held =
            self.panel_keys.contains_focused(window, cx) || self.panel_control_focused(window);
        let dialog_took_focus = self.browsers.focus_shown_dialogs(window, cx);
        // Here, before focus is handed out: a page's dialog is modal and
        // keeps the keys, and a popup left up would take them back.
        self.close_popup_with_nothing_to_draw(dialog_took_focus, cx);
        if window.has_active_dialog(cx)
            || dialog_took_focus
            || self.browsers.dialog_holds_focus(window, cx)
            || self.floats_hold_focus()
            || nav_list_focused
            || panel_keys_held
            || self.details_hold_focus(window, cx)
            || native_text_focused
            || reader_text_focused
            || find_focused
            || self.files_find_holds_focus(window, cx)
            || self.side_chat_holds_focus(window, cx)
            || self.terminal_holds_focus(window, cx)
            || pane_control_focused
        {
            // Native text, dialogs, and the Inbox keep their own keyboard focus.
        } else if self.popup_up() {
            // A popup holds the keyboard at the root (`tend_popup_focus`).
        } else if let Some(focus) = self.sheets.read(cx).up().map(|sheet| sheet.focus(cx)) {
            // The pump must not steal focus from a sheet's fields or controls.
            if !focus.contains_focused(window, cx) && !self.connection_holds_focus(window, cx) {
                window.focus(&focus, cx);
            }
        } else if !self
            .popover
            .as_ref()
            .is_some_and(|open| open.kind.holds_keyboard())
            && !wanted.is_focused(window)
            && (wanted == self.focus || !wanted.contains_focused(window, cx))
        {
            window.focus(&wanted, cx);
        }

        // The cards start under the head row (`frame::board_top`), flush to
        // the panel (`board_bounds`), riding or at rest.
        let board_top = ferrite_ui::frame::board_top();
        let panel_w = self.panel_column_w();
        let frame_right = self.board_right() - panel_w;
        let frame = || {
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .pt(px(board_top))
                .pr(px(frame_right))
        };
        let visible = self.visible_indices();
        // The empty board names the provider CLIs it found: ask once.
        if self.cockpit.environment_available()
            && self.empty_board_shown()
            && self.cli_versions.is_none()
            && !self.cli_probing
            && !cfg!(test)
        {
            self.cli_probing = true;
            self.probe_cli_versions(cx);
        }
        let grid = if !self.cockpit.environment_available() {
            frame().flex().child(self.no_environment(cx))
        } else if self.factory().is_some() {
            // A Project's Factory board stands in the board's place.
            frame().flex().child(self.factory_board(window, cx))
        } else if self.prs_shown() {
            frame().flex().child(self.prs_board(cx))
        } else if let Some((board, tree)) = self
            .board()
            .and_then(|board| self.board_tree(board).map(|tree| (board, tree)))
        {
            // A board is its split tree: the Pane and its reader at the
            // rects the tree gives them, a grab band over the seam, and —
            // while a slot is being dragged — the wash that says what a
            // release would do. Absolute geometry, so a seam drag moves
            // exactly the two sides it sits between.
            self.tree_board(board, tree, window, cx)
        } else if visible.is_empty() {
            // Nothing open: the board says how to start instead of lying
            // blank.
            frame().flex().child(self.empty_board(cx))
        } else {
            // The shown Pane, the whole board. No tree lays out slots
            // here, so none of its sizes stands.
            self.slot_sizes.borrow_mut().clear();
            frame()
                .flex()
                .flex_col()
                .child(self.pane_slot(visible[0], window, cx))
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
            .bg(ferrite_ui::theme::paint::WINDOW)
            .font_family(ferrite_ui::theme::FONT_UI)
            .map(ferrite_ui::components::tabular)
            .track_focus(&self.focus)
            // A popup joins the context; the keymap's rows stand down for
            // it.
            .key_context(ferrite_ui::keymap::root_context(self.popup_up()).as_str())
            // Nothing that acts on a lost machine's Threads is listened for
            // until it is back (ADR 0026).
            .when(!lost, |root| {
                root.on_action(cx.listener(Self::submit))
                    .on_action(cx.listener(|view, _: &ferrite_pane::composer::SaveAndSendComment, window, cx| {
                        let composer = view.panes[view.focused()].composer.clone();
                        if composer.update(cx, |composer, cx| composer.save_comment(window, cx)) {
                            view.submit(&Submit, window, cx);
                        }
                    }))
                    .on_action(cx.listener(Self::unqueue_from_backspace))
                    .on_action(cx.listener(Self::allow))
                    .on_action(cx.listener(Self::deny))
                    .on_action(cx.listener(Self::always))
                    .on_action(cx.listener(|view, _: &AttachFiles, _, cx| {
                        let index = view.focused();
                        if view.panes[index].is_main() {
                            view.attach_files(index, false, cx);
                        }
                    }))
                    .on_action(cx.listener(|view, _: &AllowOnce, window, cx| {
                        view.answer_chorded(Answer::Allow, window, cx)
                    }))
                    .on_action(cx.listener(|view, _: &DenyOnce, window, cx| {
                        view.answer_chorded(Answer::Deny, window, cx)
                    }))
                    .on_action(cx.listener(|view, _: &AlwaysAllow, window, cx| {
                        view.answer_chorded(Answer::Always, window, cx)
                    }))
                    .on_action(cx.listener(Self::close_thread))
                    .on_action(cx.listener(|view, _: &TranscriptPageUp, window, cx| {
                        view.page_transcript(Page::Up, window, cx)
                    }))
                    .on_action(cx.listener(|view, _: &TranscriptPageDown, window, cx| {
                        view.page_transcript(Page::Down, window, cx)
                    }))
                    .on_action(cx.listener(|view, _: &TranscriptTop, window, cx| {
                        view.page_transcript(Page::Top, window, cx)
                    }))
                    .on_action(cx.listener(|view, _: &TranscriptBottom, window, cx| {
                        view.page_transcript(Page::Bottom, window, cx)
                    }))
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
            .on_action(cx.listener(Self::next_thread))
            .on_action(cx.listener(Self::previous_thread))
            .on_action(cx.listener(Self::next_decision))
            .on_action(cx.listener(Self::new_thread))
            .on_action(cx.listener(Self::new_worktree_thread))
            .on_action(cx.listener(|view, _: &TextLarger, _, cx| view.step_text_size(1, cx)))
            .on_action(cx.listener(|view, _: &TextSmaller, _, cx| view.step_text_size(-1, cx)))
            .on_action(cx.listener(|view, _: &TextReset, _, cx| view.step_text_size(0, cx)))
            .on_action(cx.listener(Self::band_cycle))
            .on_action(cx.listener(|view, _: &FocusNextArea, window, cx| view.step_area(window, cx)))
            .on_action(cx.listener(|view, _: &FocusPreviousArea, window, cx| view.step_area(window, cx)))
            .on_action(cx.listener(Self::panel_control_previous))
            .on_action(cx.listener(Self::panel_control_next))
            .on_action(cx.listener(Self::panel_control_first))
            .on_action(cx.listener(Self::panel_control_last))
            .on_action(cx.listener(Self::panel_control_close))
            .on_action(cx.listener(Self::panel_control_menu))
            .on_action(cx.listener(Self::tool_cycle_previous))
            .on_action(cx.listener(Self::toggle_tool_action))
            .on_action(cx.listener(Self::stop_agent_action))
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::cite_selection))
            .on_action(cx.listener(Self::open_find))
            .on_action(cx.listener(Self::toggle_nav))
            .on_action(cx.listener(Self::toggle_panel))
            .on_action(cx.listener(Self::toggle_details))
            .on_action(cx.listener(Self::dismiss_details))
            .on_action(cx.listener(Self::go_back))
            .on_action(cx.listener(Self::go_forward))
            .on_action(
                cx.listener(|view, _: &FocusThread1, _, cx| view.go_to_thread(1, cx)),
            )
            .on_action(cx.listener(Self::toggle_terminal))
            .on_action(cx.listener(Self::toggle_changes))
            .on_action(cx.listener(Self::toggle_files))
            .on_action(cx.listener(Self::toggle_file_tree_action))
            .on_action(cx.listener(Self::go_to_file_action))
            .on_action(
                cx.listener(|view, _: &FocusThread2, _, cx| view.go_to_thread(2, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread3, _, cx| view.go_to_thread(3, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread4, _, cx| view.go_to_thread(4, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread5, _, cx| view.go_to_thread(5, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread6, _, cx| view.go_to_thread(6, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread7, _, cx| view.go_to_thread(7, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread8, _, cx| view.go_to_thread(8, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread9, _, cx| view.go_to_thread(9, cx)),
            )
            .on_action(cx.listener(Self::toggle_notifications))
            .on_action(cx.listener(|view, _: &nav::Focus, window, cx| view.focus_nav(window, cx)))
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
            .map(|root| self.register_model_picker_actions(root, cx))
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
                cx.listener(|view, event: &MouseUpEvent, _, cx| {
                    view.end_seam_drag(cx);
                    view.finish_nav_resize(cx);
                    view.panel_pointer_released(event.position, cx);
                    if view.drop_preview.take().is_some() | view.dragging.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, window, cx| {
                // The pointer inside the nav column holds the tree's order
                // still (C8); leaving it re-ranks.
                let inside = f32::from(event.position.x) < view.nav_width();
                view.nav_view
                    .update(cx, |nav, cx| nav.set_hovered(inside, cx));
                view.nav_pointer_moved(event.position, event.dragging(), cx);
                view.panel_pointer_moved(event.position, event.dragging(), cx);
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
            // rest on the `--ground`.
            // While a popup is up the keys are its own (`tend_popup_focus`):
            // Escape closes it, and the context menu's one-letter keys and
            // arrows run its rows before any field under it reads them.
            .capture_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                let key = &event.keystroke;
                if key.key == "escape" && !key.modifiers.modified() && view.close_popup(cx) {
                    cx.stop_propagation();
                } else if let Some(index) = view.menu_mnemonic(key) {
                    cx.stop_propagation();
                    view.press_menu_row(index, window, cx);
                } else if view.menu_navigate(key, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                view.typing_leaves_panel(&event.keystroke, window, cx);
            }))
            .on_action(cx.listener(Self::open_settings))
            .map(|root| self.register_board_actions(root, cx))
            .map(|root| self.register_prs_actions(root, cx))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .children(self.nav())
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
                            .left(px(self.nav_width()))
                            .flex()
                            .child(grid),
                    ),
            )
            // The window's own titlebar, where the platform makes the app
            // draw one. It is the last thing over the band and the first
            // thing under a menu: an overlay that reached into the band
            // would be answering the frame's hit test, not its own rows, so
            // the drag region stands down while one is open.
            .child(crate::frame().titlebar(!self.overlay_open(cx), self.maximized))
            .child(self.view_head(cx))
            .children(self.history_arrows_element(cx))
            .when(!self.nav_folded(), |root| root.child(self.nav_resize_rail(cx)))
            .child(self.nav_edge_tab(cx))
            // The panel over the strip's right end: its tabs take
            // their presses, the gaps between them still drag. Its toggle
            // over both heads, in one place.
            .children(self.panel_element(window, cx))
            .child(self.plane_seams(panel_w))
            .children(self.title_tools_element(cx))
            .children(self.band_menu_element(window, cx))
            .children(self.model_picker_element(window, cx))
            .children(self.popup_element(window, cx))
            .child(self.sheets.clone())
            .children(self.toast_float(window, cx))
            .children(self.sheet_floats(window, cx))
            .children(self.lightbox_element(cx))
            .children(gpui::component::Root::render_dialog_layer(window, cx))
            .children(gpui::component::Root::render_notification_layer(window, cx))
    }
}

impl CockpitView {
    /// One Pane's cell — the click-to-focus and drag plumbing around
    /// `render_pane`. The same cell serves the whole board and a slot of
    /// its tree; only who lays it out differs.
    fn pane_cell(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> Div {
        #[cfg(test)]
        drawn::built_pane(self.panes[index].identity);
        let content = self.pane_content(index, window, cx);
        if self.sheet_up(cx) {
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

    fn pane_content(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> Div {
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
            let draft = pane.draft().expect("a Pane is a Thread or a draft");
            return cell.child(pane::render_draft(
                pane,
                pane::DraftState {
                    start: self.draft_hero(index, cx),
                    attachments: Composer::attachments(&pane.composer, &pane.preview, cx),
                    menu: self.popover_element(index, cx),
                    composer_empty: pane.composer.read(cx).is_empty(),
                    focused,
                    error: draft.error.as_ref(),
                    reduce_motion: cx.reduce_motion(),
                    drop_target: self.drop_target(index, cx),
                    toolbar: self.composer_toolbar(index, cx),
                },
            ));
        };
        // Over a link, the roster can list a Thread a frame before its
        // facts land: the cell stays an empty shell until they do.
        let Some(open) = self.cockpit.thread(thread) else {
            return cell;
        };
        let retained_transcript = pane.transcript().map(|transcript| {
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
        });
        let facts = pane::PaneFacts {
            thread: open,
            composer_empty: pane.composer.read(cx).is_empty(),
            focused,
            unread: self.cockpit.notifications().attention(thread),
            reduce_motion: cx.reduce_motion(),
            editing: focused
                && window.is_window_active()
                && pane.composer.read(cx).focus_handle(cx).is_focused(window),
            drop_target: self.drop_target(index, cx),
            plane: self.on_plane(thread),
            decision_joined: false,
        };
        let attachments = Composer::attachments(&pane.composer, &pane.preview, cx);
        // The docked Decision merges into the Composer when that Composer
        // is a live block with nothing floating between them (rule 2.8.1).
        let joins = pane.is_main() && attachments.is_none();
        let activity_decisions = self.activity_decisions(index, joins, window, cx);
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
                // The slot's width as the board laid it out this frame (a
                // dragged seam included), else the Pane's rect's.
                cell_width: self
                    .slot_sizes
                    .borrow()
                    .get(&thread)
                    .map(|(w, _)| *w)
                    .or_else(|| {
                        self.pane_rects(window)
                            .into_iter()
                            .find(|(at, _)| *at == index)
                            .map(|(_, rect)| rect.w)
                    })
                    .unwrap_or_else(|| self.board_bounds(window).w),
                clock: open.turn().elapsed().map(ferrite_core::progress::live_seconds),
            },
        );
        let wiring = pane::PaneWiring {
            transcript: retained_transcript,
            attachments,
            menu: self.popover_element(index, cx),
            subagent_head: self.subagent_head(index, cx),
            activity_decisions,
            details: self.details_card(index, window, cx),
            details_lane_width: self.details_lane_width(index),
            toolbar: self.composer_toolbar(index, cx),
            resume_offer: self.resume_offer(index, cx),
            // An empty Thread wears the draft's start card until its first
            // prompt.
            start: if pane.is_main() && open.transcript().blocks().is_empty() {
                self.thread_hero(thread)
            } else {
                None
            },
        };
        cell.child(pane::render_pane(pane, facts, wiring))
    }

    /// Read free disk for the nav's foot off the UI thread: the least free of the
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
        let words = |free: Option<u64>| free.and_then(ferrite_ui::frame::free_disk_words);
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

    /// A board slot as a drop target for a reader dragged by its head: the
    /// moves over it compute the preview, the release applies it.
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
    }

    /// The size a slot was laid out at this frame, for its ghost.
    fn slot_size(&self, leaf: ThreadId) -> (f32, f32) {
        self.slot_sizes
            .borrow()
            .get(&leaf)
            .copied()
            .unwrap_or((GHOST_MAX_W, GHOST_MAX_H))
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
        let index = self.focused();
        let typing = self
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
        self.titling.insert(thread);
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
        if self.titling.remove(&thread) {
            self.notify_part(parts::Part::Nav, cx);
        }
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

    /// The offer to go on in a fresh conversation, while the Thread's
    /// provider has none to resume. It says what the way on costs.
    fn resume_offer(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.panes.get(index).filter(|pane| pane.is_main())?.thread()?;
        self.cockpit.thread(thread).filter(|open| open.resume_lost())?;
        let key = thread.get();
        let line = "A fresh conversation starts from the transcript's latest turns, sent as \
                    context with your next prompt. It will not remember the rest.";
        let action = ferrite_ui::components::secondary_button(
            SharedString::from(format!("resume-fresh-{key}")),
            None,
            "Start a fresh conversation",
            cx,
        )
        .debug_selector(move || format!("resume-fresh-{key}"))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                view.start_fresh_conversation(index, thread, window, cx);
            }),
        );
        Some(pane::resume_offer(key, line.into(), action))
    }

    /// Take the offer: the Thread goes on in a fresh conversation, and what
    /// the failed send gave back to the Composer goes out through the same
    /// path ↵ takes.
    fn start_fresh_conversation(
        &mut self,
        index: usize,
        thread: ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.cockpit.thread(thread).is_some_and(|open| open.resume_lost()) {
            return;
        }
        if let Err(error) = self.cockpit.start_fresh_conversation(thread) {
            self.cockpit
                .say(thread, format!("could not start a fresh conversation: {error}"));
            cx.notify();
            return;
        }
        self.focus_pane(index);
        if !self.panes[index].composer.read(cx).is_empty() {
            self.submit(&Submit, window, cx);
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
    /// corner: a Pane with nothing to give (one whose facts have not landed,
    /// one gone from the roster draws nothing) would have the card ask for
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
    /// head's opening down, inside the Pane. A head card hangs under the
    /// Thread details card while that is drawn, since it sits in the same
    /// corner.
    fn pane_card_corner(&self, identity: PaneIdentity, up: bool) -> Option<Point<Pixels>> {
        let pane = &self.panes[self.index_of(identity)?];
        if let Some(details) = pane.details.drawn.get().filter(|_| !up) {
            let limit_right = f32::from(details.right());
            return Some(ferrite_ui::components::FloatPlace { floor: 0., limit_right }.card_corner(details, false));
        }
        let geometry = pane.geometry.get();
        let card = geometry.card?;
        let limit_right = f32::from(card.right()) - ferrite_ui::theme::PANE_PAD_X;
        let (floor, trigger) = if up {
            let composer = geometry.composer?;
            (f32::from(composer.top()), composer)
        } else {
            // A head card hangs from the head's foot, not the card's.
            let head = gpui::Bounds::new(
                card.origin,
                gpui::size(card.size.width, px(ferrite_ui::theme::PANE_HEAD_H)),
            );
            (0., head)
        };
        Some(ferrite_ui::components::FloatPlace { floor, limit_right }.card_corner(trigger, up))
    }

    /// The limits a surface summoned from the window's bar rests within
    /// (ADR 0014): its foot over the bar's top, its right edge inside the
    /// window's margin.
    fn bar_place(&self) -> ferrite_ui::components::FloatPlace {
        use ferrite_ui::theme::GRID_PAD;
        // The board stops at the bar and `board_right` short of the
        // window's right edge.
        let board = self.board.get();
        ferrite_ui::components::FloatPlace {
            floor: board.y + board.h,
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

    /// A pick of permission mode `mode`. A mode that stops asking
    /// (`theme::mode_stops_asking`) is confirmed first, as Codex confirms
    /// Full access; any other goes at once.
    pub(super) fn request_mode(
        &mut self,
        thread: ThreadId,
        generation: u64,
        mode: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let set = ferrite_core::SessionControl::SetPermissionMode { mode: mode.clone() };
        let standing = self
            .cockpit
            .thread(thread)
            .is_some_and(|open| open.permission_mode() == Some(mode.as_str()));
        if standing || !ferrite_ui::theme::mode_stops_asking(&mode) {
            self.run_session_control(thread, generation, set);
            return;
        }
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            "Stop asking before changes?",
            Some("The agent will run commands and edit files without asking you first. Only do this in a folder you trust."),
            &["Turn on", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |view, cx| {
            if answer.await == Ok(0) {
                let _ = view.update(cx, |view, cx| {
                    view.run_session_control(thread, generation, set);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// The Composer's permission mode chip (SPEC §5.5, §D.7): the mode's
    /// label (`pane::permission_mode_label`), in `MODE_ATTENTION_INK` for
    /// `Manual` (`theme::mode_is_manual`). Always shown once the
    /// Thread has a list or a mode, so the mode can be changed, from the
    /// cached answers when no Session runs (ADR 0032). It opens the
    /// permission menu (`mode_menu.rs`); ⇧⇥ still cycles (`cycle_mode`). A
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
        let trigger = trigger.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                view.focus_pane(index);
                view.toggle_mode_menu(thread, generation, false, cx);
            }),
        );
        Some(wrap(
            ferrite_ui::components::on_bounds(
                div().flex().child(trigger),
                self.record_trigger(mode_menu::chip_trigger(thread)),
            )
            .into_any_element(),
        ))
    }

    fn session_controls_element(
        &self,
        thread: ThreadId,
        generation: u64,
        face: popups::Face,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
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
                ferrite_ui::theme::paint::FLOAT_HOVER.into(),
                ferrite_ui::theme::paint::FLOAT_SEL.into(),
                rgb(ferrite_ui::theme::INK_SUBTLE).into(),
                cx,
            )
            .h(px(ferrite_ui::theme::TOKEN_H))
            .px(px(ferrite_ui::theme::TOKEN_PAD_X))
            .rounded(px(ferrite_ui::theme::R_CONTROL))
            .child(ferrite_ui::components::text_meta().child(verb))
        };
        // A card-wide verb is a whole float row: one row, the rows' inset,
        // `INK_STRONG`, `FLOAT_HOVER` under the pointer.
        let action_row = |id: SharedString, verb: &'static str| {
            ferrite_ui::components::form_button(id, cx)
                .h(px(ferrite_ui::theme::FLOAT_ROW_H))
                .mx(px(ferrite_ui::theme::MENU_ROW_INSET_X))
                .px(px(ferrite_ui::theme::MENU_ROW_PAD_X))
                .rounded(px(ferrite_ui::theme::R_CONTROL))
                .child(
                    div()
                        .flex()
                        .w_full()
                        .text_size(px(ferrite_ui::theme::FS_UI))
                        .line_height(px(ferrite_ui::theme::LH_UI))
                        .text_color(rgb(ferrite_ui::theme::INK_STRONG))
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
                .mx(px(ferrite_ui::theme::MENU_ROW_INSET_X))
                .px(px(ferrite_ui::theme::MENU_ROW_PAD_X))
                .rounded(px(ferrite_ui::theme::R_CONTROL))
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
                                .text_color(rgb(ferrite_ui::theme::INK_STRONG))
                                .child(mode.label),
                        )
                        .children(checked.then(|| {
                            ferrite_ui::components::glyph(
                                "\u{2713}",
                                ferrite_ui::theme::MENU_CHECK_INK,
                            )
                        })),
                )
                .on_click(cx.listener(
                    move |view, _: &ClickEvent, window, cx| {
                        view.request_mode(thread, generation, value.clone(), window, cx);
                        cx.notify();
                    },
                )),
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
                        .text_color(rgb(ferrite_ui::theme::INK_STRONG))
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
                                ferrite_ui::theme::INK_SUBTLE
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
                        .text_color(rgb(ferrite_ui::theme::INK_SUBTLE))
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
                        .text_color(rgb(ferrite_ui::theme::INK_STRONG))
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
        Some(
            anchored()
                .anchor(gpui::Anchor::BottomRight)
                .position(at)
                .snap_to_window_with_margin(px(ferrite_ui::theme::GRID_PAD))
                .child(self.popup_face(
                    face,
                    "session-controls-in",
                    card,
                    ferrite_ui::motion::Opens::Up,
                    cx,
                ))
                .into_any_element(),
        )
    }

    /// The checks card: every run behind the header's rollup, grouped by
    /// the workflow that owns it, each row opening its own log. Built here
    /// rather than in `pane.rs` because the rows are controls and a
    /// listener needs this view's `Context`.
    fn context_checks_element(
        &self,
        thread: ThreadId,
        face: popups::Face,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
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
                // Keep the run's log with the Thread's other browser tabs.
                Some(url) => row.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.browser_action(thread, ferrite_core::browser::Action::Open { url: url.clone() }, cx);
                        view.close_popup(cx);
                    }),
                ),
                None => row,
            });
        }
        let card = card
            .id("context-checks-card")
            .debug_selector(|| "context-checks-card".into());
        // The mark rides the Pane head's checkout line: the card drops
        // below it, from its left edge.
        Some(
            anchored()
                .anchor(gpui::Anchor::TopRight)
                .position(at)
                .snap_to_window_with_margin(px(ferrite_ui::theme::GRID_PAD))
                .child(self.popup_face(
                    face,
                    "context-checks-in",
                    card,
                    ferrite_ui::motion::Opens::Down,
                    cx,
                ))
                .into_any_element(),
        )
    }

    /// The Thread's model chip (#25, SPEC §5.5, Claude's `Opus 5.5 Medium`):
    /// the model as the picker names it, then the effort in force, sentence
    /// case, when the model takes one — one control opening the model
    /// picker, its effort a submenu there. It opens even while working: the
    /// current choice stays visible and availability follows the Thread's
    /// busy state, said in the tooltip. It reads the cached answers, so it
    /// works with no Session.
    /// The effort in force on an open Thread (`medium`): its own choice,
    /// else the operator's default, else its model's (`effort_in_force`);
    /// `None` for a model that takes none.
    fn thread_effort(&self, open: &ThreadView) -> Option<SharedString> {
        let provider = open.provider();
        let ladder =
            ferrite_core::providers::models::efforts_for(provider, open.model(), open.models());
        (!ladder.is_empty())
            .then(|| {
                effort_in_force(
                    open.effort(),
                    self.prefs.settings.effort_for(provider),
                    provider,
                    open.model(),
                    open.models(),
                )
            })
            .flatten()
    }

    fn model_chip(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.panes[index].thread()?;
        let open = self.cockpit.thread(thread)?;
        let provider = open.provider();
        let hold = tuning_hold(&open);
        // The standing choice names the chip; else what the Session's own
        // Init said is serving; until either, the provider's default model —
        // never the provider, and never the id on the wire.
        let label = SharedString::from(ferrite_core::providers::models::footer_name(
            provider,
            open.model().or(open.transcript().model()),
            open.models(),
        ));
        let effort = self.thread_effort(&open);
        let key = thread.get() as usize;
        let identity = PaneIdentity::Thread(thread);
        let face = pane::model_effort_chip(pane::ModelChip {
            mark: ferrite_ui::provider::mark(provider),
            model: label,
            effort: effort.map(|effort| pane::sentence_case(&effort).into()),
            loading: open.starting(),
            open: self.model_picker_open(identity),
            group: model_picker::chip_trigger(identity),
        });
        let control = pane::toolbar_chip(("model-picker", key), cx)
            .debug_selector(move || format!("model-picker-{key}"))
            .tip(hold.unwrap_or("Model and effort"))
            .child(face);
        Some(self.model_chip_control(index, control, cx))
    }

    /// The model's effort ladder for `thread` (`effort_ladder_of`).
    pub(super) fn effort_ladder(
        &self,
        thread: ThreadId,
    ) -> Option<(Vec<String>, Vec<SharedString>, Option<usize>)> {
        let open = self.cockpit.thread(thread)?;
        let provider = open.provider();
        effort_ladder_of(
            open.effort(),
            self.prefs.settings.effort_for(provider),
            provider,
            open.model(),
            open.models(),
        )
    }

    /// The effort ladder of the model the draft in Pane `index` would
    /// start on (`effort_ladder_of`).
    fn draft_effort_ladder(
        &self,
        index: usize,
    ) -> Option<(Vec<String>, Vec<SharedString>, Option<usize>)> {
        let draft = self.panes.get(index)?.draft()?;
        let provider = draft.binding.provider().provider;
        effort_ladder_of(
            draft.binding.effort(),
            self.prefs.settings.effort_for(provider),
            provider,
            draft.binding.provider().model.as_deref(),
            &self.cockpit.announced_models(provider),
        )
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

    /// ⌘I (and the Bar's needs-you count): the list takes the keys on its
    /// first card (ADR 0040); again, they go back to the board.
    fn toggle_notifications(
        &mut self,
        _: &ToggleNotifications,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.nav_view.read(cx).holds_keys(window) {
            self.nav_view.update(cx, |nav, cx| nav.leave(window, cx));
            return;
        }
        self.popover = None;
        self.close_popup(cx);
        self.floats.palette = None;
        self.floats.shortcuts = None;
        let first = self.first_nav_card(cx);
        self.focus_nav_on(first, window, cx);
    }

    /// Every click or key on the Inbox's surfaces — a toast's `⏎ open`, a
    /// row, ⏎ and ⌫ on the list, `mark all read` — lands here and is
    /// answered against core. Opening lands the operator on the Notice's
    /// Pane: the core's one focus door, so a parked Thread revives, and the mirror follows. (A toast's quick
    /// answer needs the window: `answer_from_toast`.)
    pub(crate) fn notice_verb(&mut self, verb: Verb, cx: &mut Context<Self>) {
        match verb {
            Verb::OpenThread(thread) => self.land_on_thread(thread, cx),
            Verb::Open(id) => {
                self.needs_you
                    .drop_toast(&crate::notifications::RowTarget::Notice(id));
                if self.cockpit.open_notice(id).is_some() {
                    self.sync_panes(cx);
                }
                self.snooze_editor = None;
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
                let pending_subject = self
                    .cockpit
                    .thread(id.thread)
                    .and_then(|open| {
                        open.pending_decisions()
                            .iter()
                            .find(|pending| pending.handle == id.handle)
                    })
                    .map(|pending| {
                        pending
                            .subject
                            .clone()
                            .unwrap_or(ferrite_core::activity::Subject::Main)
                    });
                if let Some(thread) = self.cockpit.open_decision_notice(&id) {
                    self.sync_panes(cx);
                    self.select_subject_from_notice(thread, subject, cx);
                } else if let Some(subject) = pending_subject {
                    self.land_on_thread(id.thread, cx);
                    self.select_subject_from_notice(id.thread, subject, cx);
                }
                self.snooze_editor = None;
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
                self.snooze_editor = None;
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

    /// The Inbox's verbs, as a closure the surfaces can hold.
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
        }
    }

    /// Keep the Inbox up with the Notices and requests (FL-18, ADR 0017),
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
            let operators = |thread: ThreadId| self.cockpit.operators(thread);
            let (fresh, requests) = news(here, self.needs_you.presented(None), &operators);
            self.needs_you.hear(None, fresh, requests, now);
        }
        for away in machines.iter().flat_map(|machines| machines.off_screen()) {
            let operators = |thread: ThreadId| away.status.operators(thread);
            let (fresh, requests) = news(
                away.notifications,
                self.needs_you.presented(Some(&away.key)),
                &operators,
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
        let machine = match alert.away.clone() {
            Some(machine) => machine,
            None => match machines {
                Some(machines) => machines.selected_key()?,
                None => crate::machines::MachineKey(None),
            },
        };
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
        if self.open_task_notification(tag, cx) {
            return;
        }
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
            RowTarget::Thread(_) => return None,
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
        }
    }

    /// One row per open Thread awaiting handling, independent of Notices.
    /// Requests precede other rows, retaining each machine's attention order.
    #[cfg(test)]
    pub(super) fn needs_you_rows(&self, cx: &gpui::App) -> Vec<NoticeRow> {
        self.inbox_rows(&self.nav_state(), cx)
    }

    /// The Active section as the Inbox's rows: each local Thread's, then a
    /// connected machine's requests and finishes, requests first.
    pub(super) fn inbox_rows(&self, state: &nav::NavState, cx: &gpui::App) -> Vec<NoticeRow> {
        let now = ferrite_core::clock::system_time();
        let notifications = self.cockpit.notifications();
        let mut rows: Vec<_> = state
            .active
            .iter()
            .map(|entry| {
                let thread = entry.thread;
                if let Some(decision) = notifications.first_request(thread) {
                    self.decision_row(decision, now)
                } else if let Some(notice) = notifications
                    .notices()
                    .find(|notice| notice.thread == thread)
                {
                    self.notice_row(notice, now)
                } else {
                    NoticeRow {
                        away: None,
                        target: RowTarget::Thread(thread),
                        thread,
                        title: self.facts.name(thread),
                        state: crate::notifications::State::Idle,
                        detail: crate::notifications::Detail::None,
                        when: "".into(),
                        read: true,
                    }
                }
            })
            .collect();
        let (requests, finished) = self.away_rows(cx);
        rows.extend(requests);
        rows.extend(finished.into_iter().map(|(_, row)| row));
        rows.sort_by_key(|row| !row.request());
        let mut seen = std::collections::BTreeSet::new();
        rows.retain(|row| seen.insert((row.away.clone(), row.thread)));
        rows
    }

    /// The nav column as far in as it is this frame, its face pinned to the
    /// board's edge and clipped as it slides, under a veil that takes the
    /// pointer while it slides out.
    fn nav(&self) -> Option<AnyElement> {
        let drawn = self.nav_width();
        if drawn <= 0.0 {
            return None;
        }
        let width = self.nav_expanded_width();
        let face = nav::column(self.board_ground())
            .w(px(width))
            .absolute()
            .top_0()
            .right_0()
            .child(nav::card(self.nav_rows()).w(px(width)));
        Some(
            div()
                .relative()
                .flex_shrink_0()
                .h_full()
                .w(px(drawn))
                .overflow_hidden()
                .child(face)
                .when(self.nav_folded(), |column| column.child(columns::veil()))
                .into_any_element(),
        )
    }

    fn board_ground(&self) -> ferrite_ui::theme::Paint {
        let lone = !self.destination_shown() && !self.empty_board_shown();
        ferrite_ui::theme::Paint::solid(ferrite_ui::theme::tx_ground(lone))
    }

    fn nav_edge_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::NAV_INSET_Y;
        let open = !self.nav_folded();
        let left = (self.nav_expanded_width() - ferrite_ui::theme::HAIRLINE_W)
            * self.nav_width() / self.nav_expanded_width();
        div()
            .absolute()
            .top(px(nav::CARD_TOP))
            .bottom(px(NAV_INSET_Y))
            .left(px(left))
            .flex()
            .items_center()
            .child(nav::edge_tab(open).on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.toggle_nav_now(cx);
                }),
            ))
            .into_any_element()
    }
}

/// A slot's leaf in a board's tree. A Thread's Pane is its own id; drafts
/// and reader slots are no Threads and never persist, so stand-in ids
/// nothing real reaches name them: a draft counts down from the top, and a
/// reader slot is its Thread's number under the `READER_LEAF` bit.
const READER_LEAF: u64 = 1 << 62;

/// A Review's share of its Pane's slot when it first opens beside it.
const READER_SHARE: f32 = ferrite_ui::theme::READER_SHARE;

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
fn head_drag(leaf: ThreadId, face: GhostFace) -> impl FnOnce(Div) -> AnyElement {
    move |head: Div| {
        head.id(("slot-head", leaf.get() as usize))
            .cursor(gpui::CursorStyle::OpenHand)
            .on_drag(PaneDrag { leaf }, move |_, grab, _, cx| {
                let face = face.clone();
                cx.new(|_| PaneGhost {
                    face,
                    grab,
                })
            })
            .into_any_element()
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

/// Whether a Decision's card asks in fields of its own (questions, or an
/// MCP form): its controls keep the keyboard, and Tab walks them.
fn asks_in_fields(decision: &ferrite_core::Decision) -> bool {
    matches!(
        decision.kind,
        ferrite_core::DecisionKind::Questions(_) | ferrite_core::DecisionKind::Form { .. }
    )
}
