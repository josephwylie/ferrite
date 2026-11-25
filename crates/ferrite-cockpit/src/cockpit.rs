//! The cockpit window: every open Pane at once, and the one pump behind them.
//!
//! Rendering and keys only. What each Pane shows — the Blocks, the pending
//! Decision, the held prompt — is folded in core and read from there.

pub(crate) mod archive;
mod asks;
mod attachments;
mod background_dialog_view;
mod background_work;
pub(crate) mod beside;
mod browser;
#[cfg(feature = "browser-smoke")]
mod browser_smoke;
mod checkouts;
mod composer_footer;
mod columns;
mod composer_menu;
mod context_menu;
mod copies;
mod copy_toasts;
pub(crate) mod coordinator;
pub(crate) mod decisions;
mod delegates;
mod delivery;
mod destination;
mod details;
mod direct_view;
pub(crate) mod direction;
mod draft_commands;
mod draft_environment;
mod drafts;
mod fields;
mod head_cards;
pub(crate) mod empty_board;
pub(crate) mod environment;
pub(crate) mod factory;
mod history;
mod leaving;
mod lightbox;
mod mode_menu;
mod model_picker;
mod menu_rows;
use self::menu_rows::*;
mod machines;
mod menu_button;
mod connections_view;
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
mod pr_reads;
mod placement_view;
mod pr_watching;
mod prs_view;
mod sends;
mod server;
mod session_card;
mod session_control;
mod new_task_view;
mod project_editor_view;
mod settings_view;
mod setup_sync;
mod sheets;
mod side_chat_view;
mod rename;
mod folder_browser;
mod returns;
use rename::{rename_field, RenameTarget};
mod snooze;
pub(crate) mod starts;
mod surfaces;
mod tab_scroll;
mod toasts;
pub(crate) mod subagents;
pub(crate) mod tools;
mod transcript_glue;
pub(in crate::cockpit) use transcript_glue::Handoff;
mod updating;
mod user_commands;
mod view_head;
pub(crate) mod workflows;

use self::context_menu::{MenuTarget, MenuVerb};
use self::popups::Popup;
use self::model_picker::Column;
use ferrite_ui::components::home_relative;
use ferrite_ui::components::Tip as _;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "visual-reference")]
#[path = "visual_reference.rs"]
pub mod visual_reference;
#[cfg(feature = "headless")]
pub mod changes_frames;

use std::time::Duration;

use ferrite_client::cockpit::{Cockpit, HistoryDirection, ProviderChoice, ThreadView};
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

use ferrite_core::checkout::RepoListing;
use ferrite_core::transcript::plain_text;
use ferrite_core::provider::Provider;
#[cfg(test)]
use ferrite_core::checkout::WorkspaceChoice;
use ferrite_core::{DecisionAnswer, MachineId, ProjectKey, ThreadId, ThreadKey};
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

/// The pump's beat: the most often it runs, however fast things arrive
/// (ADR 0041). One for the whole cockpit, not one per Pane: 24 Panes must
/// cost one frame, not 24. 8ms, because 16
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

/// The longest the pump sleeps with nothing rung and nothing due: what has
/// no bell (a timeout, a retry, what the window tells the Engine) waits at
/// most this long (ADR 0041).
const PUMP_FALLBACK: Duration = Duration::from_millis(250);

/// Which rings wake the pump. gpui's test scheduler (the tests,
/// `--headless`) refuses a wake from another thread, so there the pump
/// hears its own thread's rings and looks for the rest each beat.
fn pump_hears(executor: &gpui::BackgroundExecutor) -> ferrite_core::doorbell::Hears {
    #[cfg(any(test, feature = "headless", feature = "browser-smoke"))]
    if executor.dispatcher().as_test().is_some() {
        return ferrite_core::doorbell::Hears::ThisThread;
    }
    #[cfg(not(any(test, feature = "headless", feature = "browser-smoke")))]
    let _ = executor;
    ferrite_core::doorbell::Hears::AnyThread
}

/// What a Thread's checkouts say, read with no git process: its own
/// checkout whole (`checkout`, its `git_read::checkout_stamp`), and the
/// `HEAD` of each of its Project's directories, whose branch is all the
/// Thread shows of them.
fn checkouts_stamp(checkout: Option<u64>, directories: &[(SharedString, std::path::PathBuf)]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    checkout.hash(&mut hasher);
    for (_, directory) in directories {
        ferrite_core::git_read::head_of(directory).hash(&mut hasher);
    }
    hasher.finish()
}

/// A Thread's system notification tag: a newer one for the same Thread
/// replaces it in the notification centre. A Thread on another machine is
/// named by it too, since ids are per Store, whichever machine the window
/// is on: the tag outlives a move.
fn system_tag(machine: &ferrite_core::MachineKey, thread: ThreadId) -> String {
    match &machine.0 {
        None => format!("ferrite-thread-{thread}"),
        Some(target) => format!("ferrite-thread-{thread}@{target}"),
    }
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
    open.busy() || open.operator_turn()
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
    /// The Review beside this Pane on the board (`beside.rs`).
    pub(crate) review: beside::ReviewSlot,
}

impl CockpitPane {
    fn new(view: PaneView, cx: &mut gpui::App) -> Self {
        Self {
            view,
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
    /// The client's model. On a beat that brought something the pump has
    /// it say `Arrived`, which the views that build from it hear (the
    /// nav). Nothing notifies it.
    pub(crate) cockpit: Entity<Cockpit>,
    browsers: browser::Browsers,
    /// The provider's control of the Browsers' pages and the live
    /// annotation's capture queue, which the pump runs.
    browser_controls: browser::Controls,
    /// Each Thread's Browser chrome: its address field and whether a new
    /// tab is being entered.
    browser_chrome: Entity<browser::BrowserChromeView>,
    browser_dialog: Entity<browser::BrowserDialogView>,
    browser_review: Entity<browser::BrowserReviewView>,
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
    field_watch: fields::FieldWatch,
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
    /// What each Thread's checkouts said (`checkouts_stamp`) when its
    /// labels were last read: a refresh runs git for a Thread only once
    /// they say something else, so an idle window starts no git.
    checkout_stamps: std::collections::HashMap<ThreadKey, u64>,
    /// The files git tracks in each Thread's checkout (by its cwd) though
    /// an ignore rule matches them, as git last listed them
    /// (`git_read::tracked_ignored`): its stamp reads them by name.
    tracked_ignored: std::collections::HashMap<std::path::PathBuf, std::sync::Arc<[std::path::PathBuf]>>,
    /// The machines whose Threads' branches are being read there.
    /// What a draft's workspace chip and its menu read of its Project's
    /// repo, as git or the machine's Server last answered.
    checkouts: checkouts::Checkouts,
    /// Stable text-run identities; selection itself belongs to GPUI.
    selection: TranscriptText,
    /// Every Thread's side chat, a body of the Panel.
    side_chat: Entity<side_chat_view::SideChatView>,
    /// Drafts sent while a Session was starting, kept whole until it takes
    /// the prompt or hands it back.
    held_sends: sends::HeldSends,
    /// The Composer drafts on their way to each Thread (`sends`).
    outbox: sends::Outbox,
    /// What follows each Server answer the window waits for (`asks`).
    awaited: asks::Awaited,
    /// The prompts asked back to a Composer and not yet answered
    /// (`returns`).
    returning: returns::Returning,
    tools: tools::Tools,
    /// Every Thread's Files: its tree, editor tabs and their unsaved edits.
    files: Entity<tools::FilesView>,
    terminals: Entity<tools::TerminalsView>,
    /// Every diff read: a Thread's Changes and each pull request's Code.
    changes: Entity<tools::ChangesView>,
    /// The pull request reader: the Panel's PR Surface and the PRs' detail.
    pull_request: Entity<panel::pull_request::PullRequestView>,
    /// The Inbox's alerts and the toast float that shows them.
    pub(crate) toasts: Entity<toasts::ToastsView>,
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
    nav_filter: Option<ProjectKey>,
    /// The nav, a view of its own (ADR 0041): it owns its list's motion,
    /// scroll, keyboard, search, picks and undo notice, and asks the
    /// Cockpit for the rest through `NavEvent`.
    nav_view: Entity<nav_view::NavView>,
    /// The Cockpit hearing each view that has an event enum of its own
    /// (ADR 0041), from its first frame.
    view_events: Option<Vec<gpui::Subscription>>,
    /// The window's activation, followed into the Cockpit
    /// (`Cockpit::set_window_active`) once the first frame has a window.
    activation: Option<gpui::Subscription>,
    /// The Composer's `/`, `@` and `$` menu (`composer_menu`). Always on
    /// the focused Pane's Composer; render shuts it when the operator
    /// leaves that Pane.
    composer_menu: Entity<composer_menu::ComposerMenuView>,
    /// What a draft's `/` and `$` menus list before it has a Session.
    draft_commands: draft_commands::DraftCommands,
    /// The Project new drafts fall back to: the last one the operator was
    /// working in. The launch reads it from the store's most recently used
    /// Thread, and every explicit pick in the Composer's Project chip moves
    /// it, so the next draft opens where the last one ran. Startup only
    /// selects an already registered Project; it must never turn the app's
    /// process directory into user workspace state.
    launch_project: Option<ProjectKey>,
    /// A draft's environment pick, waiting for its machines to say which
    /// Project is its Project's counterpart.
    environment_lookup: Option<draft_environment::Lookup>,
    /// The inline title rename, in a nav row or the view head.
    rename: rename::Rename,
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
    /// Holds every footer's plan-usage popover open, its ring's own clicks
    /// aside: a visual-reference scene's (`usagepopover`), never the app's.
    usage_popover_held: bool,
    /// Why the last Session control was refused (`session_control`).
    session_control_error: session_control::SessionControlError,
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
    drop_preview: Option<(Leaf, Zone)>,
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
    slot_sizes: std::cell::RefCell<std::collections::HashMap<Leaf, (f32, f32)>>,
    /// The board slot being dragged: it dims in place while its ghost
    /// moves, until the release.
    dragging: Option<Leaf>,
    /// The operator's settings and where they save; every change saves.
    prefs: Preferences,
    /// What each Thread's Composer holds unsent.
    unsent: drafts::Unsent,
    /// Where the quotes of sent prompts were cited from.
    cited_sources: ferrite_core::cited_sources::CitedSources,
    /// The sheet that is up, Settings, the Project card or the
    /// `Background work` dialog (ADR 0042).
    sheets: Entity<sheets::Sheets>,
    /// Every Thread's background work: the stops sent, the cards turned.
    background: Entity<background_work::BackgroundView>,
    /// Every Thread's Overview: the Changes section's folds and hunks, the
    /// sections' scroll.
    overview: Entity<panel::overview::OverviewView>,
    /// What the File Surface keeps of every Pane's preview: each File
    /// tab's document as last read, the scrolls, the find bar, the last
    /// press.
    file_surface: Entity<panel::preview::FileSurfaceView>,
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
    /// The operator's setup kept in sync with each Machine (ADR 0048).
    setup_sync: Entity<setup_sync::SetupSync>,
    /// Every copy of a Project of this computer to a Machine (ADR 0048 §2),
    /// and the toasts that follow them.
    copies: Entity<copies::Copies>,
    copy_toasts: copy_toasts::CopyToasts,
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
    /// The chimes that would have played (`chime`).
    #[cfg(test)]
    pub(crate) chimed: Vec<Sound>,
    /// When the pump asks for a Thread's PR on its own (`pr_reads`).
    pr_reads: pr_reads::PrReads,
    /// The palette, the shortcuts sheet and the notifications list's focus
    /// (`cockpit::palette`).
    floats: palette::Floats,
    /// Transcript events are detached subscriptions, registered once per
    /// retained Subject entity rather than once per render.
    transcript_entities: std::collections::HashSet<gpui::EntityId>,
    /// WP-F: the thread rules option 2 made, and what the Decision cards
    /// press through (`cockpit::decisions`).
    decisions: Entity<decisions::DecisionsView>,
    // ---- the frame's (F-12 … F-15)
    /// Measuring at every reader's slide's destination, so a threshold
    /// never flaps mid-slide.
    nav_measure_rest: std::cell::Cell<bool>,
    /// What stands in the board's place: the Factory or the PRs.
    destination: Option<destination::Destination>,
    /// The PRs destination, kept as it was left while another view shows.
    prs: Entity<prs_view::PrsView>,
    /// The Tasks whose needs-you toast stands, and the state it announced.
    task_toasts: std::collections::HashMap<(MachineId, String), ferrite_core::factory_board::Status>,
    /// The empty board: its selected row and the list's own focus.
    empty_board: Entity<empty_board::EmptyBoardView>,
    /// Each Thread's reader slot (its Review) sliding open or shut
    /// (`beside.rs`).
    reader_slides: beside::ReaderSlides,
    columns: columns::Columns,
    /// The Provider an empty board's first draft starts on.
    launch_provider: Provider,
    /// The machines this window holds (ADR 0026, ADR 0045); `None` in a
    /// window on an Engine of its own.
    machines: Option<crate::machines::Machines>,
}

/// What the model says on a pump beat that brought something: a Session's
/// events, a restart, a sweep that moved a fact or a clock, the Server's
/// state, an ask, parked Threads, the Panes, a start, the models or the
/// commands. A feature that draws from the model subscribes and looks
/// again (`NavView::look`). Nothing notifies the model: every view that
/// reads it while drawing would be rebuilt (vendored gpui,
/// `tracking_reads`).
pub(crate) struct Arrived;

impl EventEmitter<Arrived> for Cockpit {}

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

/// What the window is handed at launch beyond the core: the operator's
/// settings, where they are saved, and the Session defaults the spawner
/// reads — shared, so a change here applies to the next Session.
pub struct Preferences {
    pub settings: ferrite_core::settings::Settings,
    pub dir: std::path::PathBuf,
    pub defaults: std::sync::Arc<std::sync::Mutex<ferrite_core::settings::SessionDefaults>>,
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
    /// This computer's agent setup, which the window keeps in sync with
    /// each Machine's (ADR 0048), or why it keeps none. `None` in the test
    /// and demo constructors, so no suite reads or writes the operator's
    /// setup.
    pub setup_home: Option<Result<ferrite_core::setup_sync::Home, String>>,
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
                ferrite_core::settings::SessionDefaults::default(),
            )),
            cli_updates: false,
            ferrite_updates: false,
            whats_new: None,
            unreadable: None,
            keys_refused: Vec::new(),
            setup_home: None,
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
    Reader(ThreadKey),
}

/// A board slot on the move: its head is the handle. Dropped on another
/// slot of the same board it swaps with it (the centre) or splits its
/// slot (an edge). `leaf` is the slot's leaf in the board's tree.
#[derive(Clone, Copy, Debug)]
struct PaneDrag {
    leaf: Leaf,
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

/// #29: a draft's Project or Workspace menu, a popup (`Popup::Band`): one
/// band chip's choices — registry reads, never a filesystem scan — except
/// the Project chip's type-a-path row, which re-derives from the Composer
/// line per edit.
struct Band {
    chip: pane::BandChip,
    /// Opened from the start hero's project word (`draft_hero`): it drops
    /// below the word rather than standing over the strip's chip.
    from_hero: bool,
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
    use ferrite_core::ThreadKey;

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
        static TOAST_RENDERS: Cell<usize> = const { Cell::new(0) };
        static HEAD_RENDERS: Cell<usize> = const { Cell::new(0) };
        static HEARINGS: Cell<usize> = const { Cell::new(0) };
        static NAV_ROWS: RefCell<HashMap<ThreadKey, String>> = RefCell::new(HashMap::new());
        static PUMPS: Cell<usize> = const { Cell::new(0) };
        static LISTS: Cell<usize> = const { Cell::new(0) };
    }

    pub(crate) fn listed() {
        LISTS.with(|lists| lists.set(lists.get() + 1));
    }

    /// How many times the window has worked out which Threads the nav
    /// lists, and in which section, on this thread.
    pub(crate) fn lists() -> usize {
        LISTS.with(Cell::get)
    }

    pub(crate) fn pumped() {
        PUMPS.with(|pumps| pumps.set(pumps.get() + 1));
    }

    /// How many times the pump's body has run on this thread.
    pub(crate) fn pumps() -> usize {
        PUMPS.with(Cell::get)
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

    pub(crate) fn built_toasts() {
        TOAST_RENDERS.with(|renders| renders.set(renders.get() + 1));
    }

    pub(crate) fn built_head() {
        HEAD_RENDERS.with(|renders| renders.set(renders.get() + 1));
    }

    /// How many times the view head over the board has been built on this
    /// thread.
    pub(crate) fn head_renders() -> usize {
        HEAD_RENDERS.with(Cell::get)
    }

    pub(crate) fn heard() {
        HEARINGS.with(|heard| heard.set(heard.get() + 1));
    }

    /// How many times the toasts have read the Notices and the machines'
    /// news on this thread.
    pub(crate) fn hearings() -> usize {
        HEARINGS.with(Cell::get)
    }

    /// How many times the toast float has been built on this thread.
    pub(crate) fn toast_renders() -> usize {
        TOAST_RENDERS.with(Cell::get)
    }

    pub(crate) fn record_row(key: ThreadKey, row: String) {
        NAV_ROWS.with(|rows| rows.borrow_mut().insert(key, row));
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

    /// The nav row for `key` as last drawn: its status, unread, tail and
    /// name.
    pub(crate) fn row(key: ThreadKey) -> String {
        NAV_ROWS.with(|rows| rows.borrow().get(&key).cloned().unwrap_or_default())
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
        Self::new_with_machines(cockpit, launch_provider, prefs, None, cx)
    }

    /// A view on every machine `machines` holds, which `cockpit` holds;
    /// `None` is a window on an Engine of its own.
    pub fn new_with_machines(
        cockpit: Cockpit,
        launch_provider: Provider,
        prefs: Preferences,
        machines: Option<crate::machines::Machines>,
        cx: &mut Context<Self>,
    ) -> Self {
        let panel = panel::Panel::load(&prefs.dir);
        Self::new_with_panel(cockpit, launch_provider, prefs, panel, machines, cx)
    }

    /// The view knows its machines before its first Panes open: each
    /// opens with its draft as its machine's (ADR 0045).
    pub(crate) fn new_with_panel(
        mut cockpit: Cockpit,
        launch_provider: Provider,
        prefs: Preferences,
        mut panel: panel::Panel,
        machines: Option<crate::machines::Machines>,
        cx: &mut Context<Self>,
    ) -> Self {
        // A new window holds each machine for the first time: none again.
        let _ = panel.hold(&cockpit);
        ferrite_pane::pane::init_components(cx);
        subagents::init(cx);
        cockpit.settings_changed(&prefs.settings);
        // This window says what it draws from its first frame on, so no
        // Thread streams to it before it is drawn (ADR 0032).
        cockpit.draws_transcripts([]);
        // The pump runs a beat after its last run at the soonest, once the
        // bell rings past what it had seen as that run began, or once its
        // next due time comes (ADR 0041).
        cx.spawn(async move |this, cx| {
            let beat = pump_interval();
            let hears = pump_hears(cx.background_executor());
            let (mut seen, mut due) = (ferrite_core::doorbell::rung(), cx.background_executor().now());
            loop {
                cx.background_executor().timer(beat).await;
                loop {
                    let now = cx.background_executor().now();
                    if ferrite_core::doorbell::rung() > seen || now >= due {
                        break;
                    }
                    let wait = due - now;
                    let wait = match hears {
                        ferrite_core::doorbell::Hears::AnyThread => wait,
                        ferrite_core::doorbell::Hears::ThisThread => wait.min(beat),
                    };
                    futures::future::select(
                        std::pin::pin!(ferrite_core::doorbell::rung_past(seen, hears)),
                        std::pin::pin!(cx.background_executor().timer(wait)),
                    )
                    .await;
                }
                seen = ferrite_core::doorbell::rung();
                match this.update(cx, |view, cx| {
                    let brought = view.pump(cx);
                    cx.background_executor().now() + view.pump_due(brought, cx)
                }) {
                    Ok(next) => due = next,
                    Err(_) => break,
                }
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
        cx.on_release(|view, cx| view.flush_panel_persistence(cx)).detach();
        cx.on_app_quit(|view, cx| {
            view.save_drafts(cx);
            view.files.update(cx, |files, cx| files.flush_for_quit(cx));
            view.flush_panel_persistence(cx);
            view.cockpit.update(cx, |cockpit, _| cockpit.halt_sessions());
            #[cfg(feature = "cef")]
            let browser = view.browser_controls.controller.clone();
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
        let launch_project = launch_project.or_else(|| {
            let machine = cockpit.this_computer()?;
            cockpit.register_project(machine, &here()).now().ok()
        });
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
        let cockpit = cx.new(|_| cockpit);
        let setup_sync = cx.new(|_| {
            setup_sync::SetupSync::new(prefs.setup_home.clone(), prefs.dir.clone(), cockpit.clone())
        });
        let copies = cx.new(|_| copies::Copies::new(cockpit.clone()));
        cx.observe(&copies, |_, _, cx| cx.notify()).detach();
        let composer_menu = cx.new(|_| composer_menu::ComposerMenuView::default());
        // Whether a Composer's key context is the menu's follows the menu.
        cx.observe(&composer_menu, |view, _, cx| view.sync_composer_floats(cx))
            .detach();
        let sheets = cx.new(|_| sheets::Sheets::default());
        cx.subscribe(&sheets, Self::on_sheets_event).detach();
        let prs = cx.new(|cx| prs_view::PrsView::new(cockpit_weak.clone(), cx));
        let empty_board = cx.new(|cx| empty_board::EmptyBoardView::new(cockpit_weak.clone(), cx));
        cx.subscribe(&prs, Self::on_prs_event).detach();
        let side_chat = cx.new(|_| side_chat_view::SideChatView::new(cockpit_weak.clone()));
        let files = cx.new(|_| tools::FilesView::new(cockpit_weak.clone()));
        let terminals = cx.new(|_| tools::TerminalsView::new(cockpit_weak.clone()));
        cx.subscribe(&terminals, Self::on_terminals_event).detach();
        let changes = cx.new(|_| tools::ChangesView::new(cockpit_weak.clone()));
        cx.subscribe(&changes, Self::on_changes_event).detach();
        // The root draws the diffs into its own frame (`changes_pane`), so
        // each change redraws the window.
        cx.observe(&changes, |_, _, cx| cx.notify()).detach();
        let pull_request =
            cx.new(|_| panel::pull_request::PullRequestView::new(cockpit_weak.clone()));
        cx.subscribe(&pull_request, Self::on_pr_event).detach();
        // What the reader shows is drawn by the root's frame too (a tab's
        // state, the PRs' header), so each change redraws the window.
        cx.observe(&pull_request, |_, _, cx| cx.notify()).detach();
        let background = cx.new(|_| background_work::BackgroundView::new());
        cx.subscribe(&background, Self::on_background_event).detach();
        // The root draws it into its own frame (the line, the Overview, the
        // Surface), so each change redraws the window.
        cx.observe(&background, |_, _, cx| cx.notify()).detach();
        let overview = cx.new(|_| panel::overview::OverviewView::default());
        // The root draws it into its own frame, so each change redraws the
        // window.
        cx.observe(&overview, |_, _, cx| cx.notify()).detach();
        let file_surface = cx.new(|_| panel::preview::FileSurfaceView::default());
        // The root draws its card into its own frame, so each change
        // redraws the window.
        cx.observe(&file_surface, |_, _, cx| cx.notify()).detach();
        let toasts = cx.new(|cx| toasts::ToastsView::new(cockpit_weak.clone(), cockpit.clone(), cx));
        let decisions = cx.new(|_| decisions::DecisionsView::default());
        // A card's pick lives in its Pane's forms, and the root draws the
        // cards into its transcripts, so each change redraws the window.
        cx.observe(&decisions, |_, _, cx| cx.notify()).detach();
        let browser_chrome = cx.new(|_| browser::BrowserChromeView::default());
        // The root draws the chrome into the Browser's body, so each change
        // redraws the window.
        cx.observe(&browser_chrome, |_, _, cx| cx.notify()).detach();
        let browser_dialog = cx.new(|_| browser::BrowserDialogView::default());
        // So is a page's dialog, over the page.
        cx.observe(&browser_dialog, |_, _, cx| cx.notify()).detach();
        let browser_review = cx.new(|_| browser::BrowserReviewView::default());
        // And a retained annotation's review, in the page's place.
        cx.observe(&browser_review, |_, _, cx| cx.notify()).detach();
        let mut view = Self {
            cockpit: cockpit.clone(),
            browsers: Default::default(),
            browser_controls: Default::default(),
            browser_chrome,
            browser_dialog,
            browser_review,
            panes: Vec::new(),
            focus: cx.focus_handle(),
            field_watch: Default::default(),
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
            checkout_stamps: Default::default(),
            tracked_ignored: Default::default(),
            selection: TranscriptText::default(),
            side_chat,
            held_sends: Default::default(),
            outbox: Default::default(),
            awaited: Default::default(),
            returning: Default::default(),
            tools: Default::default(),
            files,
            terminals,
            changes,
            pull_request,
            toasts,
            native_copy: None,
            nav_filter: None,
            nav_view: cx.new(|cx| nav_view::NavView::new(cockpit_weak, cockpit, cx)),
            view_events: None,
            activation: None,
            composer_menu,
            draft_commands: Default::default(),
            launch_project,
            environment_lookup: None,
            checkouts: Default::default(),
            rename: Default::default(),
            popups: Default::default(),
            panel,
            panel_empty_focus: cx.focus_handle(),
            panel_keys: cx.focus_handle(),
            panel_closing_focus: cx.focus_handle(),
            history: Default::default(),
            usage_popover_held: false,
            session_control_error: Default::default(),
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
            background,
            overview,
            file_surface,
            maximized: false,
            cli_versions: None,
            cli_probing: false,
            cli_updates: Default::default(),
            environment: Default::default(),
            ferrite_update: Default::default(),
            server_bands: Default::default(),
            setup_sync,
            copies,
            copy_toasts: Default::default(),
            unreadable_told: false,
            user_commands: Default::default(),
            leave_asked: false,
            nav_notice: Default::default(),
            #[cfg(test)]
            chimed: Vec::new(),
            pr_reads: Default::default(),
            floats: palette::Floats::new(),
            transcript_entities: Default::default(),
            decisions,
            nav_measure_rest: std::cell::Cell::new(false),
            destination: None,
            prs,
            task_toasts: Default::default(),
            empty_board,
            reader_slides: Default::default(),
            columns: columns::Columns::new(),
            launch_provider,
            machines,
        };
        // A Cockpit notification is the earliest common point after core
        // state changes and before GPUI draws cached transcript children.
        // The render path repeats this cheap key comparison for initial mount
        // and derived display state that did not notify the Cockpit.
        // It also rings the pump's bell: what a window action left for the
        // pump is read on its next beat.
        cx.observe_self(|view, cx| {
            view.sync_visible_transcripts(cx);
            view.sync_composer_floats(cx);
            ferrite_core::doorbell::ring();
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
        // nothing spawns before the operator's choice. Only this
        // computer's Projects are there before any Server answers.
        if view.panes.is_empty() && view.cockpit.read(cx).this_computer().is_some() {
            if view.launch_project.is_some() {
                view.open_draft_with_provider(DraftTarget::Main, launch_provider, cx);
            } else {
                view.open_project_creator(cx);
            }
        }
        view
    }

    /// `sync_panes` when the Panes are not the roster's, in its order:
    /// whether they moved.
    fn follow_roster(&mut self, cx: &mut Context<Self>) -> bool {
        let moved = !self.panes.iter().map(|pane| pane.identity).eq(self
            .cockpit
            .read(cx)
            .roster()
            .panes()
            .iter()
            .copied());
        if moved {
            self.sync_panes(cx);
        }
        moved
    }

    /// Mirror the roster (#28): a view for every Pane the Cockpit shows, in
    /// the roster's order, and none for a Pane it no longer does. The one
    /// way a Thread's Pane joins the grid — built, its Composer watched so
    /// every edit of the line re-syncs the open menu (#23), and its caches
    /// filled here, never per frame. Drafts are made by
    /// `open_draft_with_choice`, which knows their binding; the roster
    /// never holds a draft this window did not open.
    fn sync_panes(&mut self, cx: &mut Context<Self>) {
        // A side chat's fork has no Pane: one the roster took, as its
        // machine listed it open on being held again, leaves it.
        let forks: Vec<ThreadKey> = self
            .cockpit
            .read(cx)
            .roster()
            .panes()
            .iter()
            .filter_map(|pane| pane.thread())
            .filter(|thread| self.side_chat.read(cx).owns(*thread))
            .collect();
        for fork in forks {
            self.cockpit.update(cx, |cockpit, _| cockpit.set_aside(fork));
        }
        let wanted = self.cockpit.read(cx).roster().panes().to_vec();
        let before = self.panes.len();
        self.panes.retain(|pane| wanted.contains(&pane.identity));
        // What the File Surface kept of a Pane's preview goes with it, as
        // do its Decision forms.
        self.file_surface
            .update(cx, |files, _| files.retain_panes(|pane| wanted.contains(&pane)));
        self.decisions
            .update(cx, |decisions, _| decisions.retain_panes(|pane| wanted.contains(&pane)));
        let mut opened = false;
        for identity in &wanted {
            if self.index_of(*identity).is_some() {
                continue;
            }
            let PaneIdentity::Thread(thread) = *identity else {
                continue;
            };
            let pane = CockpitPane::new(PaneView::new(thread, cx), cx);
            self.decisions
                .update(cx, |decisions, _| decisions.open_pane(*identity));
            pane.preview.set_remote_files(self.elsewhere(thread.machine(), cx));
            self.aim_attachments(&pane.composer, thread.machine(), cx);
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
        let lookups = self.facts.parked_changed(self.cockpit.read(cx));
        self.look_up(lookups, cx);
    }

    /// A Thread's Pane opened: its facts afresh, what only git can say
    /// read off the UI thread.
    fn thread_opened(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let lookups = self.facts.opened(self.cockpit.read(cx), thread);
        self.look_up(lookups, cx);
        self.check_pr(thread, false, cx);
    }

    /// Open the Thread's PR in its browser preview. An explicit open: the
    /// Thread comes forward first, so the page is the one shown.
    pub(crate) fn open_pr(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let url = self.facts.pr(thread).and_then(|pr| pr.url.clone());
        if let Some(url) = url {
            self.show_thread(thread, cx);
            self.browser_action(thread, ferrite_core::browser::Action::Open { url }, cx);
        }
    }

    /// The Thread's PR as a link target: the one every door to it opens.
    pub(crate) fn pr_target(&self, thread: ThreadKey) -> Option<LinkTarget> {
        self.facts.pr(thread).map(|pr| LinkTarget::PullRequest {
            thread: thread.thread(),
            number: pr.number,
            url: pr.url.clone(),
        })
    }

    /// An explicit open of the Thread's PR, by the door `how` names: its
    /// native detail, the integrated browser, or the system's. What cannot
    /// open says why.
    pub(crate) fn open_pr_as(
        &mut self,
        thread: ThreadKey,
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
    fn show_thread(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        if self.focused_thread(cx) != Some(thread) || self.destination_shown() {
            self.open_nav_row(thread, cx);
        }
    }

    /// Run the facts' slow reads in the background and adopt what they
    /// find. Another machine's git half is left out: its checkouts are its
    /// own (ADR 0026).
    fn look_up(&mut self, mut lookups: crate::facts::Lookups, cx: &mut Context<Self>) {
        let cockpit = self.cockpit.read(cx);
        lookups.branches.retain(|(thread, _)| cockpit.local(thread.machine()));
        lookups
            .default_branches
            .retain(|(project, _)| cockpit.local(project.machine()));
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
                        index == self.focused(cx),
                        pane.selection_scope,
                        pane.disclosure_revision(),
                    )
                })
            })
        else {
            return;
        };
        let Some(subject_view) = self
            .cockpit
            .read(cx)
            .thread(thread)
            .and_then(|open| open.activity().subject(&subject))
        else {
            return;
        };
        // What the root's own reads give, before the model is lent to the
        // transcript's update below.
        let earlier = Self::earlier_history(subject_view.transcript(), self.panes[index].is_main());
        let delegates = self.delegate_faces(thread, subject_view.transcript(), cx);
        let parent = self.delegator_link(index, cx);
        let tail = self.transcript_tail(index, cx);
        let solo = self.on_plane(thread, cx);
        self.wake_for_background_clock(index, cx);
        let entity = self.panes[index]
            .ensure_transcript(cx)
            .expect("thread Pane has a transcript entity");
        if self.transcript_entities.insert(entity.entity_id()) {
            cx.subscribe(&entity, Self::transcript_event).detach();
        }
        // One reading size for every transcript, on every board.
        let reading_size = self.prefs.settings.reading_size;
        let reduce_motion = cx.reduce_motion();
        let pane = &self.panes[index];
        let selection = self.selection.clone();
        self.cockpit.update(cx, |model, cx| {
            let Some(open) = model.thread(thread) else {
                return;
            };
            let Some(subject_view) = open.activity().subject(&subject) else {
                return;
            };
            let revision = subject_view.presentation_revision();
            let status = pane::transcript_status(subject_view.status(), subject_view.fresh());
            // The gated call hides while its Decision waits as the tail row.
            let pending_call = tail
                .as_ref()
                .and_then(|_| open.pending())
                .map(|decision| decision.tool_use_id.clone())
                .filter(|call| !call.is_empty());
            let workspace =
                ferrite_core::checkout::effective_cwd(open.session_project_root(), open.workspace())
                    .map(std::path::Path::to_path_buf);
            // While the turn runs, its header under its prompt and its live
            // row at the tail (`pane::working_tail`), over a Decision that
            // waits.
            let working = pane::working_tail(pane::WorkingFacts {
                transcript: subject_view.transcript(),
                thread: thread.thread(),
                starting: open.starting(),
                solo,
                focused,
                reduce_motion,
            });
            let mut agents = transcript_glue::agent_faces(open.activity(), open.provider());
            for (call, face) in delegates {
                agents.insert(call, face);
            }
            let rewind = transcript_glue::rewind_gate(&open);
            // A subagent's tab draws no Composer to quote into.
            let quotes = pane.is_main();
            let queued = open.queue().len();
            let presentation = ferrite_pane::transcript::TranscriptPresentation {
                thread: thread.thread(),
                selection_scope,
                disclosure_focus: pane.tool_focus(),
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
                cited: model
                    .saved(thread)
                    .and_then(|saved| self.cited_sources.of(&saved))
                    .map(std::borrow::Cow::Borrowed)
                    .unwrap_or_default(),
                note: None,
                earlier,
                parent,
                queued,
            };
            let transcript = subject_view.transcript();
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
        });
    }

    /// Whether the Thread began before the first row `transcript` draws: its
    /// Blocks were dropped to the limit, or the pane's window is cut from
    /// them. Main's whole history can be copied out: its Server replays the
    /// log (`copy_transcript`).
    fn earlier_history(
        transcript: &ferrite_core::transcript::Transcript,
        main: bool,
    ) -> Option<ferrite_pane::transcript::Earlier> {
        let dropped = transcript.earlier_dropped();
        let cut = pane::rendered_window(transcript.blocks()).len()
            < transcript.blocks().len();
        (dropped || cut).then_some(ferrite_pane::transcript::Earlier { copyable: main })
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
        let focused_index = self.focused(cx);
        let float_open = self.overlay_open(cx);
        let menu_on = self
            .composer_menu
            .read(cx)
            .pane()
            .or_else(|| self.band_menu().map(|open| open.pane));
        for (index, pane) in self.panes.iter().enumerate() {
            let open = menu_on == Some(pane.identity);
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
    fn on_plane(&self, thread: ThreadKey, cx: &gpui::App) -> bool {
        self.factory_worker_drawn(cx) != Some(thread)
    }

    fn sync_visible_transcripts(&mut self, cx: &mut Context<Self>) {
        let mut shown = self.visible_indices(cx);
        // The Factory draws a Worker or its Coordinator the roster does not
        // show (its Task's Worker tab, the Coordinator surface).
        shown.extend(
            self.factory_worker_drawn(cx)
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
        changed: &[ferrite_client::cockpit::SubjectUpdate],
        cx: &gpui::App,
    ) {
        let Some(thread) = self.panes[index].thread() else {
            return;
        };
        let Some(activity) = self.cockpit.read(cx).thread(thread).map(|thread| thread.activity()) else {
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
            .get(self.focused(cx))
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
        let index = self.focused(cx);
        let pane = self.panes.get(index)?;
        if pane.preview.focused(window, cx) && self.preview_shown(index, cx) {
            if let Some(scroll) = self.file_surface.read(cx).reader_scroll(pane.identity) {
                return Some(Reader::Document(scroll));
            }
        }
        let thread = pane.thread()?;
        let on_panel = self.panel_control_focused(window, cx) || self.panel_keys_held(window, cx);
        if !on_panel || self.active_surface(thread) != Some(crate::cockpit::surfaces::SurfaceId::Files) {
            return None;
        }
        let files = self.files.read(cx);
        files
            .prose_scroll(thread)
            .map(Reader::Document)
            .or_else(|| files.shown_editor(thread).map(Reader::Code))
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
                self.focus_pane(index, cx);
                self.panes[index].clear_tool_target();
                self.panes[index].toggle_tool(call);
                cx.notify();
            }
            ferrite_pane::transcript::TranscriptEvent::OpenReader { path, line } => {
                // ⌘-click on a path: the Thread's preview, in the Panel.
                self.focus_pane(index, cx);
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
                    reply = reply.cited_from(ferrite_core::replies::Source { thread: thread.thread(), block });
                }
                self.focus_pane(index, cx);
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
            ferrite_pane::transcript::TranscriptEvent::OpenParent => {
                if let Some(from) = self.panes[index]
                    .thread()
                    .and_then(|thread| self.facts.get(thread)?.delegated_from.clone())
                {
                    self.open_elsewhere(&from.server, from.thread, cx);
                }
            }
            // A wrap-up's next step: its words go as the next prompt.
            ferrite_pane::transcript::TranscriptEvent::Send(words) => {
                self.focus_pane(index, cx);
                if let Some(thread) = self.panes[index].thread() {
                    let _ = self.cockpit.update(cx, |cockpit, _| cockpit.send(thread, words.clone()));
                }
                cx.notify();
            }
            ferrite_pane::transcript::TranscriptEvent::OpenThread(thread) => {
                if let Some(here) = self.panes[index].thread() {
                    self.focus_thread(here.sibling(*thread), cx);
                }
            }
            ferrite_pane::transcript::TranscriptEvent::OpenPullRequest { number, url } => {
                self.focus_pane(index, cx);
                if let Some(thread) = self.panes[index].thread() {
                    self.open_named_pr(thread, *number, url.clone(), cx);
                }
            }
            ferrite_pane::transcript::TranscriptEvent::OpenBackground => {
                self.focus_pane(index, cx);
                if let Some(thread) = self.panes[index].thread() {
                    self.open_background(thread, cx);
                }
            }
            ferrite_pane::transcript::TranscriptEvent::OpenChanges(path) => {
                self.focus_pane(index, cx);
                let Some(thread) = self.panes[index].thread() else {
                    return;
                };
                match path {
                    Some(path) => {
                        let root = self.thread_path(thread, cx);
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
        let Some(transcript) = self.panes.iter().find(|pane| pane.composer == composer && pane.thread().is_some_and(|key| key.thread() == source.thread)).and_then(|pane| pane.transcript()) else { return; };
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
        let found = pane.thread().is_some_and(|key| key.thread() == source.thread)
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
        let machine = self
            .panes
            .iter()
            .position(|pane| pane.composer == composer)
            .and_then(|index| self.pane_machine(index));
        if let Some(machine) = machine {
            self.upload_attachments(&composer, machine, cx);
        }
        if let Some(draft) = self
            .panes
            .iter_mut()
            .find(|pane| pane.composer == composer)
            .and_then(|pane| pane.draft_mut())
        {
            draft.error = None;
        }
        // The `/` and `$` menus read one listing: only a line with neither
        // at the caret lets it go.
        let state = composer.read(cx);
        if slash_token(state.text(), state.cursor()).is_none()
            && skill_token(state.text(), state.cursor()).is_none()
        {
            self.draft_commands.forget();
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
                self.cockpit.update(cx, |cockpit, _| cockpit.set_composing(thread, unsent));
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
        let band = self
            .band_menu()
            .filter(|open| Some(open.pane) == edited)
            .map(|open| open.kind.chip);
        if band == Some(pane::BandChip::Project) {
            self.sync_band_rows(cx);
        } else if band.is_some() && !composer.read(cx).is_empty() {
            self.close_popup(cx);
        }
        let popover_on = self.composer_menu.read(cx).pane();
        if self.composer_menu.update(cx, |menu, cx| menu.edited(cx)) {
            self.sync_menu(cx);
        }
        // A keystroke redraws its own Pane (its line, its menu); only a
        // menu moving to another Pane redraws more.
        let menu_moved = [popover_on, self.composer_menu.read(cx).pane()]
            .into_iter()
            .flatten()
            .any(|pane| Some(pane) != edited);
        match edited {
            Some(edited) if !menu_moved => self.notify_part(parts::Part::Pane(edited), cx),
            _ => cx.notify(),
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
    /// True when it brought something (what `Arrived` is emitted for).
    fn pump(&mut self, cx: &mut Context<Self>) -> bool {
        #[cfg(test)]
        drawn::pumped();
        // What a stream can move beyond its own Pane and nav row, before it
        // moves: a frame that changes none of it redraws only those.
        let shared = self.parts.borrow().on.then(|| self.shared_chrome(cx));
        self.poll_navigation(cx);
        if let Some(connections) = self.connections(cx) {
            connections.update(cx, |connections, cx| connections.poll(cx));
        }
        let commands_changed = self.draft_commands.listed();
        if commands_changed {
            self.sync_menu(cx);
        }
        let frame = self.cockpit.update(cx, |cockpit, _| cockpit.pump());
        self.settle_side_forks(cx);
        self.fetch_attachment_previews(cx);
        self.note_work(&frame);
        self.prune_background_stops(cx);
        self.close_sheet_with_nothing_to_draw(cx);
        self.pump_browsers(cx);
        if let Some(card) = self.project_card(cx) {
            card.update(cx, |card, cx| card.poll_remote_path(cx));
        }
        if let Some(sheet) = self.placement_sheet(cx) {
            sheet.update(cx, |sheet, cx| sheet.poll(cx));
        }
        self.poll_copies(cx);
        self.poll_environment_lookup(cx);
        self.poll_machine_repo(cx);
        self.tick_setup_sync(cx);
        let server_moved = self.server_moved(cx);
        if server_moved {
            for pane in &self.panes {
                self.notify_part(parts::Part::Pane(pane.identity), cx);
            }
        }
        // A Focus or a toast an API caller asked for is presented by the
        // render, so it needs one even when nothing else moved.
        let asked = self.cockpit.read(cx).has_asked();
        // The Server says when its parked Threads' rows moved.
        let parked_moved = self.cockpit.update(cx, |cockpit, _| cockpit.take_parked_changed());
        if parked_moved {
            self.parked_changed(cx);
        }
        if self.cockpit.update(cx, |cockpit, _| cockpit.take_factory_changed()) {
            self.factory_changed(cx);
        }
        self.poll_factory_skills(cx);
        self.poll_environment(cx);
        self.apply_thread_rules(&frame, cx);
        self.settle_asks(cx);
        self.settle_held_sends(cx);
        for index in 0..self.panes.len() {
            if let Some(thread) = self.panes[index].thread() {
                for draft in self.lost_sends(thread) {
                    self.put_back_send(index, draft, true, cx);
                }
            }
        }
        self.take_handed_back_prompts(cx);
        let models_changed = self.cockpit.update(cx, |cockpit, _| cockpit.take_models_changed());
        let tuning_availability_changed = self.model_picker().is_some_and(|open| {
            open.pane.thread().is_some_and(|thread| {
                self.cockpit
                    .read(cx)
                    .thread(thread)
                    .and_then(|thread| tuning_hold(&thread))
                    != open.view.read(cx).hold
            })
        });
        if models_changed || tuning_availability_changed {
            self.refresh_model_picker(cx);
        }
        let completions = self.cockpit.update(cx, |cockpit, _| cockpit.take_bootstrap_results());
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
        let panes_changed = self.follow_roster(cx);
        self.pace_pr_reads(cx);
        let mut restarted = Vec::new();
        // What the sweep found moved: a fact or a Thread's age (everything
        // redraws), a working Thread (its Pane and the nav), the frame's
        // clock (the frame alone).
        let mut swept_change = false;
        let mut swept_busy: Vec<ThreadKey> = Vec::new();
        let mut swept_frame = false;
        let now = cx.background_executor().now();
        if now.duration_since(self.swept) >= SWEEP_INTERVAL {
            self.swept = now;
            // The core logs each restart (`Cockpit::sweep`).
            for restart in self.cockpit.update(cx, |cockpit, _| cockpit.sweep()) {
                restarted.push(restart.thread);
            }
            // Redraw only for what moved: a fact, a working Thread's clock
            // (under reduced motion the sweep is its only ride), a nav
            // row's age turning over. An idle window stays undrawn.
            // A Thread's facts (its name, Project, last use) are drawn by
            // its nav row and its Pane: they redraw like a working one.
            let moved = self.facts.tick(self.cockpit.read(cx));
            swept_busy = self
                .cockpit
                .read(cx)
                .threads()
                .into_iter()
                .filter(|thread| {
                    self.cockpit
                        .read(cx)
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
        } else if self.cockpit.read(cx).wants_worktree_listing() {
            // A Main just finished making a worktree: ask git now, not in
            // up to two seconds, so the header follows without a pause.
            self.refresh_branches(cx);
        }
        self.pump_panel_persistence(cx);
        // A restart writes a Notice even when no Session streamed this frame —
        // and a failed respawn will never stream again, so this notify is that
        // notice's only ride to the screen.
        let idle = frame.is_empty()
            && restarted.is_empty()
            && !swept_change
            && swept_busy.is_empty()
            && !swept_frame
            && !startup_changed
            && !models_changed
            && !commands_changed
            && !server_moved
            && !asked
            && !parked_moved
            && !panes_changed;
        // Something arrived, or the Server said something no Pane shows
        // (another window's Snooze): what draws from the model looks again
        // once the pump is done (the nav, `NavView::look`).
        let brought = !idle || self.cockpit.update(cx, |cockpit, _| cockpit.take_arrived());
        if brought {
            self.cockpit.update(cx, |_, cx| cx.emit(Arrived));
        }
        if idle {
            return brought;
        }
        let mut turn_ended = false;
        for update in &frame {
            if let Some(index) = self.pane_for(update.thread) {
                for (from, to) in &update.redirects {
                    self.panes[index].redirect_subject(from, to);
                }
                self.release_evicted_transcripts(index, &update.subjects, cx);
                let mut selected = self.panes[index].selected.clone();
                let resolves = self
                    .cockpit
                    .read(cx)
                    .thread(update.thread)
                    .is_some_and(|open| open.activity().subject(&selected).is_some());
                // A fresh Session forgets its agents: the Pane they filled
                // has no head to leave by, so it goes back to Main.
                if !resolves {
                    selected = ferrite_core::activity::Subject::Main;
                }
                let generation = self
                    .cockpit
                    .read(cx)
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
                    self.prune_tool_disclosures(update.thread, cx);
                }
                self.prune_request_forms(index, cx);
                self.panes[index].history_error = self
                    .cockpit
                    .read(cx)
                    .subject_history_error(update.thread, &self.panes[index].selected)
                    .map(str::to_string);
                let unretained = self
                    .cockpit
                    .read(cx)
                    .thread(update.thread)
                    .and_then(|thread| thread.activity().subject(&self.panes[index].selected))
                    .is_some_and(|subject| !subject.retained());
                if unretained && self.panes[index].history_error.is_none() {
                    self.retry_subject_history(index, cx);
                }
            }
            // Native progress can change without appending a transcript row.
            let streamed = self.facts.streamed(self.cockpit.read(cx), update.thread);
            self.advance_pr_actions(update.thread, streamed.settled, cx);
            turn_ended |= streamed.settled;
            if streamed.watch_moved {
                // The watcher saw the PR move: the card follows it.
                self.check_pr(update.thread, true, cx);
            }
        }
        let precise = restarted.is_empty()
            && !swept_change
            && !server_moved
            && !asked
            && !parked_moved
            && !panes_changed
            && !startup_changed
            && !models_changed
            && !commands_changed
            && shared.is_some_and(|shared| shared == self.shared_chrome(cx));
        for thread in restarted {
            self.facts.acted(self.cockpit.read(cx), thread);
        }
        if turn_ended {
            // The agent may have moved the checkout during its turn.
            self.refresh_branches_soon(cx);
        }
        if !precise {
            cx.notify();
            return true;
        }
        // Streaming: the Panes it reached and the nav redraw, and the frame
        // with them; every other Pane is reused. The transcripts take their
        // new inputs first, as the Cockpit's own notify has them do
        // (`observe_self`), so they draw in this same frame.
        self.sync_visible_transcripts(cx);
        // A working row's clock is drawn from the time, not the model: the
        // sweep is its ride under reduced motion, so the nav redraws for it
        // as it is. Its rows the nav finds moved itself.
        let clocks = !swept_busy.is_empty();
        for thread in frame.iter().map(|update| update.thread).chain(swept_busy) {
            self.notify_thread_pane(thread, cx);
        }
        if clocks {
            self.nav_view.update(cx, |_, cx| cx.notify());
        }
        self.notify_frame(cx);
        true
    }

    /// How long until the pump is next due if nothing rings: a beat after
    /// one that brought something (the rest of a full beat, a step that
    /// answers the next one), or while a Browser page is live, since its
    /// pages' events do not ring; else at the sweep, and at the latest
    /// `PUMP_FALLBACK`. An alert's delay and a toast's end are the toasts'
    /// own clock (`ToastsView::tend`).
    fn pump_due(&self, brought: bool, cx: &gpui::App) -> Duration {
        if brought || self.browsers.live() {
            return pump_interval();
        }
        let now = cx.background_executor().now();
        let sweep = SWEEP_INTERVAL.saturating_sub(now.duration_since(self.swept));
        PUMP_FALLBACK.min(sweep)
    }

    /// What a stream can change that more than its own Pane and nav row
    /// draw: the account's usage windows (every Pane's meter), the Threads
    /// waiting on the operator (the Inbox's needs-you rows), the unread
    /// notices and attention (the Inbox, each
    /// Pane's head).
    fn shared_chrome(&self, cx: &gpui::App) -> String {
        let cockpit = self.cockpit.read(cx);
        let attention: Vec<ThreadKey> = cockpit
            .threads()
            .into_iter()
            .filter(|thread| cockpit.notifications(thread.machine()).attention(thread.thread()))
            .collect();
        let limits: Vec<_> = cockpit
            .machines()
            .flat_map(|machine| Provider::ALL.map(|provider| cockpit.account_limits(machine, provider)))
            .collect();
        let unread: Vec<_> = cockpit
            .every_notifications()
            .map(|notifications| notifications.unread())
            .collect();
        format!(
            "{:?} {:?} {:?} {:?}",
            limits,
            cockpit.needs_you(),
            unread,
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
    /// Git runs only for a Thread whose checkouts' files moved since its
    /// labels were read (`checkouts_stamp`): an idle window starts none.
    /// None of it for another machine: no git here can read its checkouts,
    /// and its Server sends each Thread's branch in its digest.
    pub(crate) fn refresh_branches(&mut self, cx: &mut Context<Self>) {
        // A draft has no Thread yet, but its workspace chip names its
        // Project's checkout, which the operator may switch under it.
        let drafted: Vec<ProjectKey> = self
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
        // A worktree a Main just made is named only by a listing taken
        // after it finished, whatever the files said before.
        let listing_wanted = self.cockpit.read(cx).wants_worktree_listing();
        let targets: Vec<_> = self
            .cockpit
            .read(cx)
            .threads()
            .into_iter()
            .filter(|thread| !self.elsewhere(thread.machine(), cx))
            .filter_map(|thread| {
                let open = self.cockpit.read(cx).thread(thread)?;
                let repo = open.workspace().map(|binding| binding.repo().to_path_buf());
                let cwd = ferrite_core::checkout::effective_cwd(
                    open.session_project_root(),
                    open.workspace(),
                )?;
                let project = open
                    .project_id()
                    .and_then(|project| {
                        self.cockpit
                            .read(cx)
                            .registry(thread.machine())
                            .reading()
                            .project(project)
                    });
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
        self.checkout_stamps
            .retain(|thread, _| targets.iter().any(|(open, ..)| open == thread));
        self.tracked_ignored
            .retain(|checkout, _| targets.iter().any(|(_, cwd, ..)| cwd == checkout));
        if targets.is_empty() {
            return;
        }
        let known = self.checkout_stamps.clone();
        let tracked_ignored = self.tracked_ignored.clone();
        self.branch_refreshing = true;
        cx.spawn(async move |this, cx| {
            let (branches, listings, learned) = cx
                .background_executor()
                .spawn(async move {
                    // One `git worktree list` per repo, and one status
                    // per checkout, however many Threads share them.
                    let mut listed: std::collections::HashMap<
                        std::path::PathBuf,
                        Option<Vec<std::path::PathBuf>>,
                    > = std::collections::HashMap::new();
                    let mut walked = std::collections::HashMap::new();
                    let mut statuses = std::collections::HashMap::new();
                    let mut learned = std::collections::HashMap::new();
                    let mut listings = Vec::new();
                    let branches = targets
                        .into_iter()
                        .filter_map(|(thread, cwd, directories, repo)| {
                            // Before git reads: a write from here on moves
                            // the next stamp.
                            let checkout = *walked.entry(cwd.clone()).or_insert_with(|| {
                                let listed = tracked_ignored.get(&cwd).map_or(&[][..], |files| files);
                                ferrite_core::git_read::checkout_stamp(&cwd, listed)
                            });
                            let stamp = checkouts_stamp(checkout, &directories);
                            if !listing_wanted && known.get(&thread) == Some(&stamp) {
                                return None;
                            }
                            if let Some(repo) = repo {
                                let listing = listed
                                    .entry(repo.clone())
                                    .or_insert_with(|| {
                                        ferrite_core::git_read::worktree_paths(&repo).ok()
                                    })
                                    .clone();
                                if let Some(listing) = listing {
                                    listings.push((thread, listing));
                                }
                            }
                            // A list that changed moves the next stamp,
                            // which reads it: git runs once more, then rests.
                            let status = statuses
                                .entry(cwd.clone())
                                .or_insert_with(|| {
                                    let listed = ferrite_core::git_read::tracked_ignored(&cwd);
                                    learned.insert(cwd.clone(), listed.into());
                                    ferrite_core::git_read::branch_status(&cwd)
                                })
                                .clone();
                            let project_branches = directories
                                .into_iter()
                                .filter_map(|(label, directory)| {
                                    ferrite_core::git_read::checkout_branch(&directory)
                                        .map(|branch| (label, SharedString::from(branch)))
                                })
                                .collect();
                            Some((thread, stamp, status, project_branches))
                        })
                        .collect::<Vec<_>>();
                    (branches, listings, learned)
                })
                .await;
            this.update(cx, |view, cx| {
                view.branch_refreshing = false;
                view.checkout_stamps
                    .extend(branches.iter().map(|(thread, stamp, ..)| (*thread, *stamp)));
                view.tracked_ignored.extend(learned);
                let labels = view.facts.set_branches(
                    branches
                        .iter()
                        .map(|(thread, _, status, _)| (*thread, status.clone()))
                        .collect(),
                );
                let roots = view.facts.set_project_branches(
                    branches
                        .into_iter()
                        .map(|(thread, _, _, project_branches)| (thread, project_branches))
                        .collect(),
                );
                let mut moved = false;
                for (thread, listing) in listings {
                    if view.cockpit.update(cx, |cockpit, _| cockpit.worktrees_listed(thread, listing, taken_at)) {
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
    fn prune_tool_disclosures(&mut self, thread: ThreadKey, cx: &gpui::App) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let mut valid = std::collections::HashSet::new();
        if let Some(transcript) = self
            .cockpit
            .read(cx)
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
                                    ferrite_pane::transcript::changes_folders(&changes, self.thread_path(thread, cx).as_deref())
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
            || self.composer_menu.read(cx).is_open()
            || self.popup_up()
            || self.sheet_float_open()
    }

    fn nav_folded(&self) -> bool {
        self.nav_collapsed || (self.nav_auto_fold.get() && !self.nav_forced_open)
    }

    fn update_auto_fold(&mut self, window: &Window, cx: &gpui::App) {
        use ferrite_ui::theme::{NAV_AUTO_FOLD_BOARD_W, NAV_AUTO_FOLD_HYSTERESIS, NAV_AUTO_FOLD_PANE_W};
        let held = self.nav_auto_fold.get();
        let slack = if held { NAV_AUTO_FOLD_HYSTERESIS } else { 0.0 };
        let width = f32::from(window.viewport_size().width);
        let right = self.panel_target_w(cx);
        let board = width - self.nav_expanded_width() - right;
        let narrowest = |view: &Self| {
            view.pane_rects(window, cx)
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
        self.board_bounds(window, cx);
    }

    /// The board the cards lay out in, in window coordinates (theme WP-C):
    /// flush right of the nav, from under the head row
    /// (`frame::board_top`) to the window's foot, and `board_right` short
    /// of its right edge (the panel, or the gutter).
    fn board_bounds(&self, window: &Window, cx: &gpui::App) -> layout::Rect {
        let viewport = window.viewport_size();
        let top = ferrite_ui::frame::board_top();
        let left = self.nav_width();
        let bounds = layout::Rect {
            x: left,
            y: top,
            w: (f32::from(viewport.width) - left - self.board_right(cx)).max(0.0),
            h: (f32::from(viewport.height) - top).max(0.0),
        };
        self.board.set(bounds);
        bounds
    }

    /// The board on screen, when a tree lays it out: the shown Pane's,
    /// while it has a reader slot. A plain Pane has none.
    fn board(&self, cx: &gpui::App) -> Option<Board> {
        match self.visible_indices(cx).as_slice() {
            [index] if self.has_reader(*index) => Some(Board(self.panes[*index].identity)),
            _ => None,
        }
    }

    /// A board's tree as it should draw right now: the one mid-drag, else
    /// the operator's arrangement fitted to what is open — the Pane, and
    /// its reader slot while a document is open or sliding shut. A reader
    /// the tree has not placed yet opens beside its Pane, on the right.
    /// Every slot is at rest here: `slid_tree` draws a slide.
    /// The slot `leaf` stands for on `board`: its Pane, or the reader slot
    /// its Thread has open. None for a leaf of neither. Only the board can
    /// say: a leaf is a bare id, and two machines' Threads share them.
    fn leaf_slot(board: Board, leaf: Leaf) -> Option<Slot> {
        if pane_leaf(board.0) == leaf {
            return Some(Slot::Pane(board.0));
        }
        let PaneIdentity::Thread(owner) = board.0 else {
            return None;
        };
        (reader_leaf(owner) == leaf).then_some(Slot::Reader(owner))
    }

    fn board_tree(&self, board: Board, cx: &gpui::App) -> Option<Tree> {
        if let Some(drag) = self.seam_drag.as_ref().filter(|drag| drag.board == board) {
            return Some(drag.tree.clone());
        }
        let readers: Vec<ThreadKey> = self
            .visible_indices(cx)
            .into_iter()
            .filter(|index| self.has_reader(*index))
            .filter_map(|index| self.panes[index].thread())
            .collect();
        let base = self.board_layouts.get(&board).cloned().unwrap_or_default();
        let mut panes = vec![pane_leaf(board.0)];
        let readers: Vec<(Leaf, Leaf)> = readers
            .into_iter()
            .map(|owner| (pane_leaf(PaneIdentity::Thread(owner)), reader_leaf(owner)))
            .filter(|(owner, _)| panes.contains(owner))
            .collect();
        let mut tree = base;
        let wanted: std::collections::BTreeSet<Leaf> = panes
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
    fn pane_rects(&self, window: &Window, cx: &gpui::App) -> Vec<(usize, layout::Rect)> {
        let bounds = self.board_bounds(window, cx);
        if let Some(board) = self.board(cx) {
            let Some(tree) = self.board_tree(board, cx) else {
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
                .filter_map(|(leaf, rect)| match Self::leaf_slot(board, leaf)? {
                    Slot::Pane(identity) => self.index_of(identity).map(|index| (index, rect)),
                    Slot::Reader(_) => None,
                })
                .collect();
        }
        self.visible_indices(cx)
            .first()
            .map(|index| vec![(*index, bounds)])
            .unwrap_or_default()
    }

    /// The palette's `checks` command (F-11): the checks card for the
    /// focused Thread's PR, hung from its Pane's top edge. Nothing when the
    /// PR reports no checks.
    pub(crate) fn open_checks(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread(cx) else {
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
        let Some(thread) = self.focused_thread(cx) else {
            return;
        };
        if self
            .cockpit
            .read(cx)
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

    /// The plan card (`open_plan`), hung under the Pane head
    /// (`head_cards::PlanCard`).
    fn plan_card_element(
        &self,
        thread: ThreadKey,
        face: popups::Face,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let card = head_cards::PlanCard::of(self.cockpit.read(cx).thread(thread)?.transcript())?;
        let at = self.card_corner(
            format!("plan-{}", thread.thread().get()),
            CardFrom::Head(PaneIdentity::Thread(thread)),
            window,
            cx,
        )?;
        Some(
            anchored()
                .anchor(gpui::Anchor::TopRight)
                .position(at)
                .snap_to_window_with_margin(px(ferrite_ui::theme::GRID_PAD))
                .child(self.popup_face(
                    face,
                    "plan-card-in",
                    card.render(),
                    ferrite_ui::motion::Opens::Down,
                    cx,
                ))
                .into_any_element(),
        )
    }

    /// The Panes on screen, as indices into the mirror — the roster's
    /// `visible`, in its order.
    /// Every Thread whose records this frame draws, which the window
    /// streams whole while it does; every other reads its digest (ADR
    /// 0032, F-A2): the shown Pane's Thread, or the Factory's Worker or
    /// Coordinator in its place, and the fork of the side chat the Panel
    /// shows. A subagent's transcript, the Panel's other Surfaces and a
    /// review beside the Pane read the shown Thread's own records.
    fn transcripts_drawn(&self, cx: &gpui::App) -> Vec<ThreadKey> {
        let board = match self.factory_worker_drawn(cx) {
            Some(worker) => vec![worker],
            None => self
                .visible_indices(cx)
                .into_iter()
                .filter_map(|index| self.panes[index].thread())
                .collect(),
        };
        board.into_iter().chain(self.side_chat_drawn(cx)).collect()
    }

    fn visible_indices(&self, cx: &gpui::App) -> Vec<usize> {
        self.cockpit
            .read(cx)
            .visible()
            .into_iter()
            .filter_map(|identity| self.index_of(identity))
            .collect()
    }

    /// Open the inline editor on a Thread's title. A Thread with no stored
    /// title shows its generated one, so that is what the editor opens on:
    /// the operator edits the name they can see.
    fn start_rename(&mut self, target: RenameTarget, cx: &mut Context<Self>) {
        let (RenameTarget::Thread(thread) | RenameTarget::ViewTitle(thread)) = target;
        let title = self.facts.name(thread).to_string();
        self.rename.open(target, title, cx);
        cx.notify();
    }

    /// Close the editor; `save` commits the title it holds. A refusal from
    /// the store lands where every other nav notice does; the editor still
    /// closes, so a row is never stuck in edit.
    fn finish_rename(&mut self, save: bool, cx: &mut Context<Self>) {
        if !self.rename.is_open() {
            return;
        }
        let asked = if save {
            self.rename.take(cx)
        } else {
            self.rename.close();
            None
        };
        if let Some((thread, title)) = asked {
            let renamed = self.cockpit.update(cx, |cockpit, _| cockpit.rename_thread(thread, &title));
            self.when_answered(
                renamed,
                move |view, renamed, cx| {
                    match renamed {
                        Err(error) => view.nav_notice.problem(error.to_string()),
                        Ok(()) => {
                            view.facts.renamed(view.cockpit.read(cx), thread);
                            view.refresh_names();
                            view.nav_notice.clear();
                        }
                    }
                    cx.notify();
                },
                cx,
            );
        }
        cx.notify();
    }

    /// The Threads the Parked shelf lists and counts: every parked
    /// Thread the Project filter admits; a parked Thread on screen is still
    /// a parked Thread.
    fn parked_threads(&self, cx: &gpui::App) -> Vec<ThreadKey> {
        self.facts
            .parked()
            .iter()
            .copied()
            .filter(|thread| self.admitted(*thread, cx))
            .collect()
    }

    /// Whether any Thread, open or parked, records this Project.
    fn project_in_use(&self, project: ProjectKey, cx: &gpui::App) -> bool {
        self.cockpit
            .read(cx)
            .threads()
            .into_iter()
            .chain(self.facts.parked().iter().copied())
            .any(|thread| {
                self.cockpit.read(cx).project_of(thread) == Some(project)
            })
    }

    /// A Thread's effective cwd.
    fn target_path(&self, target: MenuTarget, cx: &gpui::App) -> Option<std::path::PathBuf> {
        match target {
            MenuTarget::Thread(key)
            | MenuTarget::Pane(key)
            | MenuTarget::Snooze(key) => self.thread_path(key, cx),
            MenuTarget::File(thread) => self.files.read(cx).pressed(thread),
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

    fn thread_path(&self, thread: ThreadKey, cx: &gpui::App) -> Option<std::path::PathBuf> {
        if let Some(open) = self.cockpit.read(cx).thread(thread) {
            return ferrite_core::checkout::effective_cwd(
                open.session_project_root(),
                open.workspace(),
            )
            .map(std::path::Path::to_path_buf);
        }
        let meta = self.cockpit.read(cx).peek(thread).ok()?;
        ferrite_core::checkout::effective_cwd(
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
        let parked = self.parked_threads(cx);
        self.delete_threads(
            parked,
            |view, deleted, cx| {
                let refused: Vec<String> = deleted
                    .into_iter()
                    .filter_map(|(_, deleted)| deleted.err())
                    .map(|error| error.to_string())
                    .collect();
                match (refused.len(), refused.last()) {
                    (_, None) => view.nav_notice.clear(),
                    (1, Some(reason)) => view
                        .nav_notice
                        .problem(format!("1 parked thread not deleted: {reason}")),
                    (count, Some(reason)) => view
                        .nav_notice
                        .problem(format!("{count} parked threads not deleted, last: {reason}")),
                }
                view.sync_panes(cx);
                view.parked_changed(cx);
            },
            cx,
        );
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
        let bounds = self.board_bounds(window, cx);
        let origin_x = self.nav_width();
        let local = |rect: layout::Rect| layout::Rect {
            x: rect.x - origin_x,
            ..rect
        };
        let mut board = div().relative().flex_1().min_w_0().min_h_0();
        let rest: std::collections::HashMap<Leaf, layout::Rect> = tree
            .rects(bounds, ferrite_ui::theme::BOARD_SEAM)
            .into_iter()
            .collect();
        let tree = self.slid_tree(tree, bounds);
        // A slot slid all the way shut takes no room and draws nothing.
        let rects: Vec<(Leaf, layout::Rect)> = tree
            .rects(bounds, ferrite_ui::theme::BOARD_SEAM)
            .into_iter()
            .filter(|(_, rect)| rect.w > 0.0 && rect.h > 0.0)
            .collect();
        *self.slot_sizes.borrow_mut() = rects
            .iter()
            .map(|(leaf, rect)| (*leaf, (rect.w, rect.h)))
            .collect();
        for (leaf, rect) in rects {
            let Some(slot) = Self::leaf_slot(on, leaf) else {
                continue;
            };
            let cell = match slot {
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
        let Some(tree) = self.board_tree(board, cx) else {
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
        let bounds = self.board_bounds(window, cx);
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
        source: Leaf,
        target: Leaf,
        position: gpui::Point<Pixels>,
        bounds: gpui::Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let on_board = self
            .board(cx)
            .and_then(|board| self.board_tree(board, cx))
            .is_some_and(|tree| tree.contains(source) && tree.contains(target));
        let preview = (source != target && on_board).then(|| {
            let (pointer, rect) = drop_geometry(position, bounds);
            (target, layout::zone(pointer, rect))
        });
        self.set_drop_preview(preview, cx);
    }

    fn set_drop_preview(&mut self, preview: Option<(Leaf, Zone)>, cx: &mut Context<Self>) {
        if self.drop_preview != preview {
            self.drop_preview = preview;
            cx.notify();
        }
    }


    /// The release of a dragged slot on another: the centre swaps the two
    /// leaves, an edge moves the source beside the target — the tree is
    /// kept either way. Reads the preview the last move computed.
    fn drop_pane(&mut self, source: Leaf, target: Leaf, cx: &mut Context<Self>) {
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
        source: Leaf,
        target: Leaf,
        zone: Zone,
        cx: &mut Context<Self>,
    ) {
        let Some(board) = self.board(cx) else {
            return;
        };
        let Some(mut tree) = self.board_tree(board, cx) else {
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
        let Some(machine) = self.settings_machine(cx) else {
            return;
        };
        for provider in self.cli_updates.ready() {
            if self.cockpit.read(cx).live_sessions(machine, provider) == 0 {
                self.update_cli(provider, cx);
            }
        }
    }

    /// Install `provider`'s newest CLI on the machine Settings is about, now.
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
        let place = self.machine_place(cx);
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
            ("Open Threads", self.cockpit.read(cx).threads().len().to_string()),
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
        let asked = self.cockpit.update(cx, |cockpit, _| cockpit.take_asked());
        if asked.is_empty() {
            return;
        }
        // An act's settle can take an ask before the pump has run: the
        // Panes follow the roster first, so a Focus on a Thread just opened
        // lands on its Pane instead of reopening a Thread that is open.
        self.follow_roster(cx);
        for asked in asked {
            self.present(asked, window, cx);
        }
    }

    /// One ask: to land on a Thread, as its nav row would; a toast, which
    /// fades as a finished turn's does unless important, with its chime.
    fn present(
        &mut self,
        asked: ferrite_client::cockpit::Asked,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use ferrite_client::cockpit::Asked;
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        match asked {
            // A fork the side chat asked for goes to its tab, not the board.
            Asked::Focus { thread, fork_of } => {
                let fork_of = fork_of
                    .or_else(|| self.unnamed_fork_owner(thread, cx));
                self.switch_thread(window, cx, |view, _, cx| {
                    if !fork_of.is_some_and(|owner| view.bind_side_thread(owner, thread, cx)) {
                        view.land_on_thread(thread, cx);
                    }
                });
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
        use ferrite_core::allowance::LimitWindow;
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        for warning in self.cockpit.update(cx, |cockpit, _| cockpit.take_allowance_warnings()) {
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
        self.cockpit.update(cx, |cockpit, _| cockpit.settings_changed(&self.prefs.settings));
        self.machines_changed(cx);
        if let Ok(mut defaults) = self.prefs.defaults.lock() {
            *defaults = ferrite_core::settings::SessionDefaults::from_settings(&self.prefs.settings);
        }
        ferrite_pane::visual::diagram::set_enabled(
            self.prefs.settings.visuals != ferrite_core::visual::Level::Off,
            cx,
        );
        if self.facts.set_auto_title(self.prefs.settings.auto_title) {
            self.parked_changed(cx);
            for thread in self.cockpit.read(cx).threads() {
                self.facts.renamed(self.cockpit.read(cx), thread);
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

    fn pane_for(&self, thread: ThreadKey) -> Option<usize> {
        self.panes
            .iter()
            .position(|pane| pane.thread() == Some(thread))
    }

    /// The focused identity's index in this window's mirror. A core act can
    /// receive roster changes before the next pump reconciles the Panes;
    /// another Thread's old index must never receive this one's input.
    /// The focused Pane's index; with none focused, one no Pane has.
    fn focused(&self, cx: &gpui::App) -> usize {
        match self.cockpit.read(cx).roster().focused() {
            Some(identity) => self.index_of(identity).unwrap_or(0),
            None => self.panes.len(),
        }
    }

    fn focused_thread(&self, cx: &gpui::App) -> Option<ThreadKey> {
        self.cockpit.read(cx).roster().focused_thread()
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
    fn focused_draft_mut(&mut self, cx: &gpui::App) -> Option<&mut pane::DraftBinding> {
        let focused = self.focused(cx);
        self.panes
            .get_mut(focused)
            .and_then(|pane| pane.draft_mut())
    }

    fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.get(self.focused(cx)).is_some_and(|pane| !pane.composer.read(cx).comment_valid(cx)) {
            return;
        }
        // The card's ↵ is its confirm — but in a machine's folder browser,
        // once it is moved, it adds the folder it shows.
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
        if let Some(sheet) = self.placement_sheet(cx) {
            sheet.update(cx, |sheet, cx| sheet.confirm(cx));
            return;
        }
        if self.nav_view.read(cx).snooze_line().is_some() {
            self.nav_view.update(cx, |nav, cx| nav.finish_snooze(true, cx));
            return;
        }
        if self.rename.is_open() {
            self.finish_rename(true, cx);
            return;
        }
        // A draft's ↵ (#29): on a band chip it opens that chip's popover;
        // on the prompt line it is the first send — the bootstrap — or a
        // typed-out `/import`.
        if let Some(band_focus) = self
            .panes
            .get(self.focused(cx))
            .and_then(|pane| pane.draft())
            .map(|draft| draft.band_focus)
        {
            match band_focus {
                Some(chip) => self.open_band_menu(chip, false, cx),
                None if self.typed_import(window, cx) => {}
                None => self.bootstrap_draft(cx),
            }
            return;
        }
        let Some(thread) = self.focused_thread(cx) else {
            return;
        };
        if !self.panes[self.focused(cx)].is_main() {
            return;
        }
        // ↵ on an empty line sends a pending question whose every question
        // is answered (digits toggled a multi-select; ↵ sends it).
        if self.panes[self.focused(cx)].composer.read(cx).is_empty() && self.send_ready_question(cx) {
            return;
        }
        if self.typed_import(window, cx) {
            return;
        }
        let composer = self.panes[self.focused(cx)].composer.clone();
        if !self.admit_files(self.focused(cx), cx) {
            return;
        }
        let draft = composer.update(cx, |composer, cx| composer.take_draft(cx));
        let text = draft.prompt().trim().to_string();
        let cited = draft.content().replies.clone();
        // `/effort max` and `/model sonnet` typed out are Ferrite's own
        // acts, the same as the pickers: the chip stays truthful, Codex
        // (which has no such command) gets them too, and a level or model
        // Ferrite does not know goes to the provider as text.
        if let Some(handled) = self.typed_tuning(thread, &text, cx) {
            match handled {
                Tuning::Effort(effort) => self.pick_effort(thread, effort, cx),
                Tuning::Model(model) => self.pick_provider(
                    thread,
                    ProviderChoice {
                        provider: self
                            .cockpit
                            .read(cx)
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
            self.take_back_held(thread, cx);
            return;
        }
        // Typing does not wait for the agent; sending does.
        self.send_draft(thread, draft, text, cited, cx);
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
            || self.panes.get(self.focused(cx)).is_none_or(|pane| !pane.is_main())
        {
            return;
        }
        let Some(thread) = self.focused_thread(cx) else {
            return;
        };
        if !self.panes[self.focused(cx)].composer.read(cx).is_empty() {
            return;
        }
        let Some(id) = self
            .cockpit
            .read(cx)
            .thread(thread)
            .and_then(|open| queued_to_take_back(&open))
        else {
            return;
        };
        self.cockpit.update(cx, |cockpit, _| cockpit.remove_queued_item(thread, &id, false));
        cx.notify();
    }

    fn interrupt(&mut self, _: &Interrupt, window: &mut Window, cx: &mut Context<Self>) {
        if self.dismiss_popup(cx) {
            return;
        }
        if let Some(transcript) = self.panes.get(self.focused(cx)).and_then(|pane| pane.transcript()) {
            if transcript.update(cx, |transcript, cx| transcript.dismiss_offer(cx)) {
                return;
            }
        }
        if self.leave_control(window, cx) {
            return;
        }
        if let Some(connections) = self
            .connections(cx)
            .filter(|connections| connections.read(cx).editing())
        {
            connections.update(cx, |connections, cx| connections.cancel(cx));
            return;
        }
        // The setup sheet's esc closes it; a copy it shows carries on.
        if let Some(sheet) = self.placement_sheet(cx) {
            sheet.update(cx, |sheet, cx| sheet.dismiss(cx));
            return;
        }
        if self.hide_sheet(cx) {
            return;
        }
        if self.nav_view.read(cx).snooze_line().is_some() {
            self.nav_view.update(cx, |nav, cx| nav.finish_snooze(false, cx));
            return;
        }
        if self.rename.is_open() {
            self.finish_rename(false, cx);
            return;
        }
        if !self.cancel_focused_draft_start(cx) {
            cx.notify();
            return;
        }
        // On a draft, escape returns to the prompt (#29): the band's tab
        // focus clears. (An open band menu holds the ComposerMenu keys, so
        // escape there dismisses through `menu_dismiss` instead.)
        if let Some(draft) = self.focused_draft_mut(cx) {
            draft.band_focus = None;
            cx.notify();
            return;
        }
        // Escape stops what the Pane shows: on Main its turn while one runs
        // (T3's Stop, which ends the Thread's PR watches with it), never the
        // Subagents and background work it started, and nothing at rest;
        // on a Subagent that Subagent alone — and once it has nothing to
        // stop, it goes back to its parent.
        if let Some(thread) = self
            .focused_thread(cx)
            .filter(|thread| !self.machine_lost(thread.machine(), cx))
        {
            let selected = self.panes[self.focused(cx)].selected.clone();
            match &selected {
                ferrite_core::activity::Subject::Main => {
                    if self.cockpit.read(cx).thread(thread).is_some_and(|open| turn_running(&open)) {
                        self.cockpit.update(cx, |cockpit, _| cockpit.interrupt(thread));
                    }
                }
                ferrite_core::activity::Subject::Subagent(key) => {
                    if !self.stop_subagent(thread, key, cx) {
                        let parent = self
                            .agent_lineage(thread, &selected, cx)
                            .pop()
                            .unwrap_or(ferrite_core::activity::Subject::Main);
                        self.select_subject(thread, parent, window, cx);
                    }
                }
            }
            self.facts.acted(self.cockpit.read(cx), thread);
        }
        cx.notify();
    }

    /// Stop the Subagent `key` while it runs; whether it did. A finished
    /// one has nothing to stop, and its escape must not reach Main's turn.
    fn stop_subagent(&mut self, thread: ThreadKey, key: &ferrite_core::activity::AgentKey, cx: &mut gpui::App) -> bool {
        let Some(open) = self.cockpit.read(cx).thread(thread) else {
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
            self.cockpit.update(cx, |cockpit, _| cockpit.stop_subagent(thread, key));
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
            || self.rename.is_open()
            || self.nav_view.read(cx).snooze_line().is_some()
            || self.composer_menu.read(cx).is_open()
        {
            return false;
        }
        let Some(thread) = self.focused_thread(cx) else {
            return false;
        };
        let Some(open) = self.cockpit.read(cx).thread(thread) else {
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
            .get(self.focused(cx))
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
            || self
                .placement_sheet(cx)
                .is_some_and(|sheet| sheet.update(cx, |sheet, cx| sheet.tab(window, cx)))
        {
            return;
        }
        if let Some(at) = self.composer_menu.read(cx).command_selected() {
            self.composer_menu.update(cx, |menu, cx| menu.pick(at, cx));
            return;
        }
        if self.accept_suggestion(window, cx) {
            return;
        }
        if self.sheet_up(cx)
            || self
                .focused_thread(cx)
                .and_then(|thread| self.cockpit.read(cx).thread(thread))
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
        // A draft's files come first; once the band has the ring, tab walks
        // the band alone, so a file and a chip never hold it at once.
        let pane = self.panes.get(self.focused(cx));
        let on_band = pane
            .and_then(|pane| pane.draft())
            .is_some_and(|draft| draft.band_focus.is_some());
        let composer = pane.map(|pane| pane.composer.clone()).filter(|_| !on_band);
        if composer.is_some_and(|composer| {
            composer.update(cx, |composer, cx| composer.cycle_files(false, window, cx))
        }) {
            return;
        }
        let environment = self.environment_chip_shown(cx);
        let Some(draft) = self.focused_draft_mut(cx) else {
            self.cycle_tools(false, window, cx);
            return;
        };
        draft.band_focus = pane::BandChip::next(draft.band_focus, environment);
        // The menu belongs to the chip it opened from; tab moves on.
        if self.band_menu().is_some() {
            self.close_popup(cx);
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
                .focused_thread(cx)
                .and_then(|thread| self.cockpit.read(cx).thread(thread))
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
                .get(self.focused(cx))
                .map(|pane| pane.composer.clone());
            if !composer.is_some_and(|composer| {
                composer.update(cx, |composer, cx| composer.cycle_files(true, window, cx))
            }) {
                self.cycle_tools(true, window, cx);
            }
        }
    }

    fn cycle_tools(&mut self, reverse: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.get(self.focused(cx)).is_none_or(|pane| pane.draft().is_some()) {
            return;
        }
        let focused = self.focused(cx);
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
        let Some(thread) = self.focused_thread(cx) else {
            return;
        };
        let Some(call) = self.panes[self.focused(cx)].targeted_tool().cloned() else {
            return;
        };
        if let pane::DisclosureId::Tool(agent) = &call {
            if self.targeted_agent(thread, &call, cx).is_some() {
                let index = self.focused(cx);
                self.open_agent_from(index, agent.clone(), cx);
                return;
            }
        }
        self.toggle_tool(thread, &call, window, cx);
    }

    /// ⌫ on the keyboard's agent row: its subagent stops, while it runs
    /// (`stop_subagent`); on any other row, nothing.
    fn stop_agent_action(&mut self, _: &StopAgent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread(cx) else {
            return;
        };
        let Some(call) = self.panes[self.focused(cx)].targeted_tool().cloned() else {
            return;
        };
        if let Some(ferrite_core::activity::Subject::Subagent(key)) =
            self.targeted_agent(thread, &call, cx)
        {
            if self.stop_subagent(thread, &key, cx) {
                cx.notify();
            }
        }
    }

    /// The agent the keyboard's row `target` started, when it is an agent's.
    fn targeted_agent(
        &self,
        thread: ThreadKey,
        target: &pane::DisclosureId,
        cx: &gpui::App,
    ) -> Option<ferrite_core::activity::Subject> {
        let pane::DisclosureId::Tool(call) = target else {
            return None;
        };
        self.cockpit
            .read(cx)
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
        if self.panes.get(self.focused(cx)).is_some_and(|pane| !pane.is_main()) {
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
        let thread = match self.focused_thread(cx) {
            Some(thread)
                if self
                    .cockpit
                    .read(cx)
                    .thread(thread)
                    .and_then(|open| open.pending())
                    .is_some() =>
            {
                Some(thread)
            }
            _ => self.cockpit.read(cx).answer_target(),
        };
        let Some(thread) = thread else {
            return;
        };
        let Some(request) = self.cockpit.read(cx).thread(thread).and_then(|open| {
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
            let steer = self.focused_thread(cx) == Some(thread);
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
        self.facts.acted(self.cockpit.read(cx), thread);
        cx.notify();
    }

    fn next_thread(&mut self, _: &NextThread, window: &mut Window, cx: &mut Context<Self>) {
        self.step_thread(1, window, cx);
    }

    fn previous_thread(&mut self, _: &PreviousThread, window: &mut Window, cx: &mut Context<Self>) {
        self.step_thread(-1, window, cx);
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
        if let Some(thread) = self.panel_thread(cx) {
            let on_controls = self.panel_control_focused(window, cx);
            if self.open_files_find(thread, on_controls, window, cx) {
                return;
            }
        }
        let index = self.focused(cx);
        let Some(pane) = self.panes.get(index) else {
            return;
        };
        let (owner, preview) = (pane.identity, pane.preview.clone());
        let reading =
            self.file_surface.read(cx).pressed() == Some(owner) || preview.focused(window, cx);
        if reading
            && self.preview_shown(index, cx)
            && self
                .file_surface
                .update(cx, |files, cx| files.open_find(owner, &preview, window, cx))
        {
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
                self.nav_view.update(cx, |nav, cx| nav.close_snooze(cx));
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
            self.nav_view.update(cx, |nav, cx| nav.close_snooze(cx));
        }
        cx.notify();
    }

    // --------------------------------------------------- Composer menus (#23)

    /// Re-derive the text-derived popover from the focused line's own text.
    /// Nothing else opens or closes a menu: `/filter` at the caret opens commands,
    /// an `@token` under the caret opens files, anything else closes. A
    /// picker or a band chip holds the slot while it is up and is left
    /// alone here.
    /// The menu keys serve whichever rows menu rides the Composer: the
    /// `/`/`@` menu or a band menu — one key context, no second table.
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
        let index = self.focused(cx);
        if !self.history_available(index, cx) {
            return;
        }
        let Some(thread) = self.panes.get(index).and_then(|pane| pane.thread()) else {
            return;
        };
        if let Some(id) = self
            .cockpit
            .read(cx)
            .thread(thread)
            .and_then(|open| queued_to_take_back(&open))
        {
            if matches!(direction, HistoryDirection::Older) {
                self.cockpit.update(cx, |cockpit, _| cockpit.edit_queued_item(thread, &id));
                cx.notify();
            }
            return;
        }
        self.step_history_of(thread, direction, cx);
    }

    fn history_available(&self, index: usize, cx: &gpui::App) -> bool {
        if index != self.focused(cx) {
            return false;
        }
        let Some(thread) = self.panes.get(index).and_then(|pane| pane.thread()) else {
            return false;
        };
        let Some(open) = self.cockpit.read(cx).thread(thread) else {
            return false;
        };
        !self.panes[index].has_tool_target()
            && !self.rename.is_open()
            && self.nav_view.read(cx).snooze_line().is_none()
            && !self.composer_menu.read(cx).is_open()
            && ((self.panes[index].is_main() && queued_to_take_back(&open).is_some())
                || (open.has_prompt_history()
                    && !open.busy()
                    && open.pending().is_none()
                    && open.queue().is_empty()))
    }

    /// Clamp-step the selection of the rows menu riding the Composer.
    fn step_popover(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.composer_menu.read(cx).is_open() {
            self.composer_menu.update(cx, |menu, cx| menu.step(delta, cx));
        } else if self.band_menu_mut().is_some_and(|open| open.step(delta)) {
            cx.notify();
        }
    }

    fn menu_pick(&mut self, _: &MenuPick, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = self.composer_menu.read(cx).popover().map(|open| open.selected) {
            self.composer_menu.update(cx, |menu, cx| menu.pick(at, cx));
        } else if let Some(at) = self.band_menu().map(|open| open.selected) {
            self.pick(at, window, cx);
        }
    }

    /// Escape while a rows menu rides the Composer closes it and nothing
    /// else — the text stays, and escape's Interrupt meaning waits for the
    /// next press. A text-derived menu mutes until the text moves; a
    /// picker takes no mute, since nothing reopens it; a band menu also
    /// returns the chip's tab focus to the prompt line.
    fn menu_dismiss(&mut self, _: &MenuDismiss, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.composer_menu.update(cx, |menu, cx| menu.dismiss(cx)) && self.band_menu().is_some() {
            self.close_popup(cx);
            if let Some(draft) = self.focused_draft_mut(cx) {
                draft.band_focus = None;
            }
        }
        cx.notify();
    }

    /// ↵ or a press on row `at` of a band menu: its choice, made. The
    /// menu closes first.
    fn pick(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.band_menu() else {
            return;
        };
        let Some(row) = open.rows.get(at) else {
            return;
        };
        let (pane, consequence) = (open.pane, row.consequence.clone());
        self.close_popup(cx);
        self.run_pick(pane, &consequence, window, cx);
    }

    /// What a pick of the band menu or the Composer menu leaves to the
    /// root: a picker opened in the menu's place, a Provider, an effort or
    /// a band choice. A command, a skill or a file the Composer menu put
    /// in the line itself.
    fn run_pick(
        &mut self,
        pane: PaneIdentity,
        consequence: &Consequence,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(composer) = self
            .index_of(pane)
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
        match consequence {
            Consequence::Command(_)
            | Consequence::Skill(_)
            | Consequence::Mention(_)
            | Consequence::Inert => {}
            Consequence::OpenProviderPicker => {
                if let Some(thread) = pane.thread() {
                    splice_line(cx, "");
                    self.open_provider_picker(thread, cx);
                }
            }
            Consequence::OpenImportPicker => {
                // A draft's one line is the `/import` its pick replaces; a
                // Thread is replaced only while it is blank.
                let into = match pane {
                    PaneIdentity::Draft(_) => Some(pane),
                    PaneIdentity::Thread(_) => {
                        splice_line(cx, "");
                        self.import_target(cx)
                    }
                };
                self.open_import_palette(into, window, cx);
            }
            Consequence::Provision(choice) => {
                if let Some(thread) = pane.thread() {
                    self.pick_provider(thread, choice.clone(), cx);
                }
            }
            Consequence::OpenEffortPicker => {
                if let Some(thread) = pane.thread() {
                    splice_line(cx, "");
                    // The model picker's Reasoning column, as the
                    // palette's `effort` opens it.
                    self.open_effort_picker(thread, cx);
                }
            }
            Consequence::Effort(effort) => {
                if let Some(thread) = pane.thread() {
                    self.pick_effort(thread, effort.clone(), cx);
                }
            }
            Consequence::Band(choice) => self.pick_band(choice, &composer, cx),
        }
        cx.notify();
    }

    fn poll_navigation(&mut self, cx: &mut Context<Self>) {
        self.poll_composer_menu(cx);
        self.poll_session_discovery(cx);
    }

    /// Ferrite's own words to one Pane: a Notice in a Thread's transcript,
    /// or the error line under a draft's band.
    fn say_to(&mut self, pane: PaneIdentity, message: String, cx: &mut gpui::App) {
        match pane {
            PaneIdentity::Thread(thread) => self.cockpit.update(cx, |cockpit, _| cockpit.say(thread, message)),
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
        machine: ferrite_core::MachineId,
        current: Provider,
        chosen: Option<&str>,
        serving: Option<&str>,
        locked: bool,
        consequence: impl Fn(ProviderChoice) -> Consequence,
        cx: &gpui::App,
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
            let mut catalog = ferrite_core::models::menu(
                self.cockpit.read(cx).model_catalog(machine, provider),
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
                                        .read(cx)
                                        .model_catalog(machine, provider)
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
                    ferrite_core::models::footer_name(
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
                        detail: SharedString::from(ferrite_core::models::groom_detail(
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
    fn pick_provider(&mut self, thread: ThreadKey, choice: ProviderChoice, cx: &mut Context<Self>) {
        let same_provider = self
            .cockpit
            .read(cx)
            .thread(thread)
            .is_some_and(|open| open.provider() == choice.provider);
        let asked = if same_provider {
            self.cockpit.update(cx, |cockpit, _| cockpit.set_model(thread, choice.model))
        } else {
            self.cockpit.update(cx, |cockpit, _| cockpit.set_provider(thread, choice))
        };
        self.when_tuned(thread, asked, "model", cx);
    }

    /// What follows a model or effort pick's answer: a refusal changed
    /// nothing, and says so in the Thread.
    fn when_tuned(
        &mut self,
        thread: ThreadKey,
        asked: ferrite_client::cockpit::Ask<()>,
        what: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.when_answered(
            asked,
            move |view, answered, cx| {
                if let Err(error) = answered {
                    view.cockpit
                        .update(cx, |cockpit, _| cockpit.say(thread, format!("{what} unchanged: {error}")));
                }
                view.facts.acted(view.cockpit.read(cx), thread);
                cx.notify();
            },
            cx,
        );
    }

    /// The machine an import for `into` lists and adopts on: the blank
    /// Pane's, else where a new Thread starts (`start_machine`).
    pub(super) fn import_machine(
        &self,
        into: Option<PaneIdentity>,
        cx: &gpui::App,
    ) -> Option<ferrite_core::MachineId> {
        into.and_then(|pane| self.index_of(pane))
            .and_then(|index| self.pane_machine(index))
            .or_else(|| self.start_machine(cx))
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
        let Some(machine) = self.import_machine(into, cx) else {
            return;
        };
        match self
            .cockpit
            .update(cx, |cockpit, _| cockpit.adopt_into(machine, into, path))
        {
            Ok(adopted) => {
                let draft = into.filter(|pane| pane.draft().is_some());
                if let Some(error) = &adopted.not_opened {
                    // Durable but not on screen: the Thread sits in the
                    // nav's parked rows, exactly like a launch-time import
                    // that would not open.
                    ferrite_core::log!(
                        "imported thread {} would not open: {error:?}",
                        adopted.thread.thread()
                    );
                } else if let Some(index) = draft.and_then(|draft| self.index_of(draft)) {
                    self.panes[index].composer.update(cx, |composer, cx| {
                        composer.take(cx);
                    });
                    self.adopt_thread(index, adopted.thread, cx);
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
                    Some(pane) => self.say_to(pane, message, cx),
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
            .get(self.focused(cx))
            .map(|pane| match pane.draft() {
                Some(draft) => draft.binding.provider().clone(),
                None => {
                    let open = pane.thread().and_then(|thread| self.cockpit.read(cx).thread(thread));
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
    pub(crate) fn choose_nav_filter(&mut self, project: Option<ProjectKey>, cx: &mut Context<Self>) {
        self.nav_filter = project;
        if let Some(project) = project {
            let standing = self.panes.iter().position(|pane| {
                pane.identity
                    .draft()
                    .is_some_and(|draft| !self.cockpit.read(cx).draft_starting(draft))
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
    /// settings as its machine's Thread (`SavedThread`).
    pub(crate) fn set_pinned(&mut self, key: ThreadKey, pinned: bool, cx: &mut Context<Self>) {
        let Some(pin) = self.cockpit.read(cx).saved(key) else {
            return;
        };
        self.change_settings(
            |settings| {
                settings.pinned_threads.retain(|saved| *saved != pin);
                if pinned {
                    settings.pinned_threads.push(pin);
                }
            },
            cx,
        );
        cx.notify();
    }

    /// The pinned Threads of the machines the window holds, in pin order.
    pub(super) fn pins(&self, cx: &gpui::App) -> Vec<ThreadKey> {
        let cockpit = self.cockpit.read(cx);
        self.prefs
            .settings
            .pinned_threads
            .iter()
            .filter_map(|pin| cockpit.resolve(pin))
            .collect()
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
    pub(crate) fn open_draft_in_project(&mut self, project: ProjectKey, cx: &mut Context<Self>) {
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
        chosen: Option<ProjectKey>,
        cx: &mut Context<Self>,
    ) {
        if !self.cockpit.read(cx).environment_available() {
            return;
        }
        // The draft starts in the Project of the Thread the operator is in:
        // the focused one, else the one focus last left.
        let current = self
            .cockpit
            .read(cx)
            .roster()
            .current_thread()
            .and_then(|thread| self.cockpit.read(cx).project_of(thread));
        // A second press with a draft already up must not stack another:
        // focus the standing draft and re-aim it at the caller's target.
        // A draft mid-bootstrap is on its way out and does not count.
        if let Some(index) = self.panes.iter().position(|pane| {
            pane.identity
                .draft()
                .is_some_and(|draft| !self.cockpit.read(cx).draft_starting(draft))
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
            self.cockpit.update(cx, |cockpit, _| cockpit.focus(identity));
            // The draft may be the Pane focused as the Factory or the PRs
            // opened, so focus never moves to leave them: give the board
            // back here.
            self.leave_destination();
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
        let machine = binding.binding.project().machine();
        let draft = self.cockpit.update(cx, |cockpit, _| cockpit.open_draft());
        let pane = CockpitPane::new(PaneView::new_draft(draft, binding, cx), cx);
        self.decisions
            .update(cx, |decisions, _| decisions.open_pane(pane.identity));
        pane.preview.set_remote_files(self.elsewhere(machine, cx));
        self.aim_attachments(&pane.composer, machine, cx);
        cx.subscribe(&pane.composer, Self::composer_edited).detach();
        cx.subscribe(&pane.composer, Self::composer_opens_source).detach();
            cx.subscribe(&pane.composer, Self::composer_comment_opened).detach();
        self.panes.push(pane);
        self.sync_panes(cx);
        draft
    }

    /// Open one chip's menu on the focused draft — the shared tail of a
    /// chip click, the hero's project word (`from_hero`) and ↵ on a
    /// tab-focused chip. Toggles shut when the same chip's menu is already
    /// up. Rows are registry reads, discovered at open — never per frame,
    /// never a filesystem scan. Git is asked, off the UI thread, what the
    /// checkout is on now and — for the workspace chip — what the repo
    /// holds; the rows take each answer as it lands.
    pub(super) fn open_band_menu(
        &mut self,
        chip: pane::BandChip,
        from_hero: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.panes.get(self.focused(cx)) else {
            return;
        };
        let Some(draft) = pane.draft() else {
            return;
        };
        let identity = pane.identity;
        if self
            .band_menu()
            .is_some_and(|open| open.kind.chip == chip && open.pane == identity)
        {
            self.close_popup(cx);
            return;
        }
        let rows = match chip {
            pane::BandChip::Provider => {
                if self.model_picker_open(identity) {
                    self.close_popup(cx);
                } else {
                    self.open_model_picker(identity, Column::List, cx);
                }
                return;
            }
            pane::BandChip::Environment => self.environment_menu_rows(identity, draft, cx),
            pane::BandChip::Project => self.project_menu_rows(draft, cx),
            pane::BandChip::Workspace => self.workspace_menu_rows(draft, cx),
        };
        let project = draft.binding.project();
        // The menu answers about the checkout as it stands now; until git
        // says, it names the last answer.
        self.look_up_checkouts([project], cx);
        // The arrows start on the standing choice — bare ↵ re-picks it.
        let selected = rows.iter().position(|row| row.active).unwrap_or(0);
        let menu = Popover {
            scroll: ScrollHandle::new(),
            revealed: Default::default(),
            pane: identity,
            kind: Band { chip, from_hero },
            rows,
            selected,
        };
        self.show_popup(Popup::Band(menu), cx);
        if chip == pane::BandChip::Workspace {
            self.refresh_repo_listing(project, cx);
        }
    }

    /// Ask git, off the UI thread, what `project`'s repo holds for the
    /// workspace popover — its worktrees and its local branches — and fill
    /// the open popover's rows in when the answer lands. One listing per
    /// open, never per frame. On another machine, its Server asks git there.
    fn refresh_repo_listing(&mut self, project: ProjectKey, cx: &mut Context<Self>) {
        if self.elsewhere(project.machine(), cx) {
            self.ask_machine_repo(project, cx);
            return;
        }
        let Some(root) = self
            .cockpit
            .read(cx)
            .project(project)
            .map(|project| project.root.clone())
        else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let listing = cx
                .background_executor()
                .spawn(async move { ferrite_core::git_read::repo_listing(&root) })
                .await;
            this.update(cx, |view, cx| {
                view.checkouts.set_listing(project, listing);
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
        let Some(open) = self.band_menu() else {
            return;
        };
        if open.kind.chip != pane::BandChip::Workspace {
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
        let rows = self.workspace_menu_rows(draft, cx);
        let Some(open) = self.band_menu_mut() else {
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
    fn checked_out_branch(&self, project: ProjectKey, cx: &Context<Self>) -> SharedString {
        if let Some(branch) = self.checkouts.branch(project) {
            return branch;
        }
        self.look_up_checkouts([project], cx);
        SharedString::from(MAIN_BRANCH)
    }

    /// Ask, off the UI thread, what each of `projects`' checkouts is on:
    /// git only for one whose `HEAD` moved since it last named the branch
    /// (`git_read::head_of`). Callable from a frame: it only starts the
    /// lookup, once per Project at a time, and `checkouts_read` takes the
    /// answers. Never of another machine, whose checkouts no git here can
    /// read.
    fn look_up_checkouts(&self, projects: impl IntoIterator<Item = ProjectKey>, cx: &Context<Self>) {
        let mut roots: Vec<(ProjectKey, std::path::PathBuf, Option<Option<String>>)> = Vec::new();
        for project in projects {
            if self.elsewhere(project.machine(), cx)
                || self.checkouts.asking(project)
                || roots.iter().any(|(asked, ..)| *asked == project)
            {
                continue;
            }
            if let Some(found) = self.cockpit.read(cx).project(project) {
                roots.push((project, found.root.clone(), self.checkouts.head(project)));
            }
        }
        if roots.is_empty() {
            return;
        }
        self.checkouts.listen(roots.iter().map(|(project, ..)| *project));
        cx.spawn(async move |this, cx| {
            let answers = cx
                .background_executor()
                .spawn(async move {
                    roots
                        .into_iter()
                        .map(|(project, root, known)| {
                            let head = ferrite_core::git_read::head_of(&root);
                            let branch = (known.as_ref() != Some(&head))
                                .then(|| ferrite_core::git_read::checkout_branch(&root));
                            checkouts::Looked { project, head, branch }
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
    fn checkouts_read(&mut self, answers: Vec<checkouts::Looked>, cx: &mut Context<Self>) {
        if self.checkouts.listed(answers) {
            self.sync_workspace_rows(cx);
            cx.notify();
        }
    }

    /// The project chip's rows for `draft` (`menu_rows::project_rows`):
    /// every machine's Projects, with the focused Composer's line for the
    /// type-a-path row.
    fn project_menu_rows(&self, draft: &pane::DraftBinding, cx: &Context<Self>) -> Vec<Row> {
        let typed = self
            .panes
            .get(self.focused(cx))
            .map(|pane| pane.composer.read(cx).text().trim().to_string())
            .unwrap_or_default();
        let cockpit = self.cockpit.read(cx);
        let projects: Vec<_> = cockpit
            .machines()
            .flat_map(|machine| {
                let registry = cockpit.registry(machine);
                let label = self.machine_label(machine, cx);
                registry.reading().projects().iter().map(move |project| {
                    (registry.project_key(project.id), project, label.clone())
                })
            })
            .collect();
        menu_rows::project_rows(&projects, draft, &typed)
    }

    /// The workspace chip's rows for `draft` (`menu_rows::workspace_rows`),
    /// from what git last said of its Project's checkout.
    fn workspace_menu_rows(&self, draft: &pane::DraftBinding, cx: &Context<Self>) -> Vec<Row> {
        let project = draft.binding.project();
        menu_rows::workspace_rows(
            draft,
            self.checked_out_branch(project, cx),
            self.checkouts.listing(project),
        )
    }

    /// Re-derive the open project popover's rows from the Composer line —
    /// the type-a-path row follows the typing, exactly as the `/` menu's
    /// rows follow theirs.
    fn sync_band_rows(&mut self, cx: &mut Context<Self>) {
        let open = self
            .band_menu()
            .is_some_and(|open| open.kind.chip == pane::BandChip::Project);
        if !open {
            return;
        }
        let Some(draft) = self.panes.get(self.focused(cx)).and_then(|pane| pane.draft()) else {
            return;
        };
        let rows = self.project_menu_rows(draft, cx);
        if let Some(open) = self.band_menu_mut() {
            open.selected = open.selected.min(rows.len().saturating_sub(1));
            open.rows = rows;
        }
    }

    /// Discovery can finish while a picker is open. Rebuild its rows and
    /// preserve the highlighted choice by value, even if row order changes.
    fn refresh_model_picker(&mut self, cx: &mut Context<Self>) {
        self.rebuild_model_picker(cx);
    }

    /// Stop the submitted startup before Escape or a choice changes its draft.
    fn cancel_focused_draft_start(&mut self, cx: &mut gpui::App) -> bool {
        let Some(id) = self
            .panes
            .get(self.focused(cx))
            .and_then(|pane| pane.identity.draft())
        else {
            return true;
        };
        match self.cockpit.update(cx, |cockpit, _| cockpit.cancel_draft_start(id)) {
            Ok(_) => true,
            Err(error) => {
                if let Some(draft) = self.focused_draft_mut(cx) {
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
        if !self.cancel_focused_draft_start(cx) {
            return;
        }
        match choice {
            BandChoice::Provider(provider) => {
                let announced = self
                    .pane_machine(self.focused(cx))
                    .map(|machine| self.cockpit.read(cx).announced_models(machine, provider.provider))
                    .unwrap_or_default();
                if let Some(draft) = self.focused_draft_mut(cx) {
                    draft.binding.choose_provider(provider.clone(), &announced);
                    draft.error = None;
                }
            }
            BandChoice::Effort(effort) => {
                if let Some(draft) = self.focused_draft_mut(cx) {
                    draft.binding.choose_effort(effort.clone());
                    draft.error = None;
                }
            }
            BandChoice::Project(project) => {
                if let Some(draft) = self.panes.get(self.focused(cx)).map(|pane| pane.identity) {
                    self.rebind_draft(draft, *project, cx);
                }
            }
            BandChoice::RegisterPath(path) => {
                let focused = self.focused(cx);
                if let (Some(machine), Some(pane)) = (
                    self.pane_machine(focused),
                    self.panes.get(focused).map(|pane| pane.identity),
                ) {
                    let registered = self
                        .cockpit
                        .update(cx, |cockpit, _| cockpit.register_project(machine, path));
                    let (path, composer) = (path.clone(), composer.clone());
                    self.when_answered(
                        registered,
                        move |view, registered, cx| {
                            // The new Pane that asked, if it still stands.
                            let Some(draft) = view
                                .index_of(pane)
                                .and_then(|index| view.panes[index].draft_mut())
                            else {
                                return;
                            };
                            match registered {
                                Ok(project) => {
                                    draft.binding.choose_checkout(project);
                                    draft.error = None;
                                    // The typed path was the pick's input,
                                    // not a prompt: the line clears for one.
                                    composer.update(cx, |composer, cx| {
                                        let whole = 0..composer.text().len();
                                        composer.splice(whole, "", cx);
                                    });
                                }
                                Err(e) => {
                                    draft.error = Some(SharedString::from(format!(
                                        "cannot register {}: {e}",
                                        path.display()
                                    )));
                                }
                            }
                            cx.notify();
                        },
                        cx,
                    );
                }
            }
            BandChoice::Target(target) => {
                if let Some(draft) = self.focused_draft_mut(cx) {
                    draft.binding.choose_target(target.clone());
                    draft.error = None;
                }
            }
            BandChoice::AddProject => {
                let focused = self.panes.get(self.focused(cx));
                let draft = focused.filter(|pane| pane.draft().is_some()).map(|pane| pane.identity);
                match (draft, self.pane_machine(self.focused(cx))) {
                    (Some(draft), Some(machine)) => self.open_project_creator_for(machine, draft, cx),
                    _ => self.open_project_creator(cx),
                }
            }
            BandChoice::Environment(machine) => self.pick_environment(*machine, cx),
            BandChoice::Connections => self.open_connections(cx),
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
        let Some(pane) = self.panes.get(self.focused(cx)) else {
            return;
        };
        let (Some(draft), Some(id)) = (pane.draft(), pane.identity.draft()) else {
            return;
        };
        // It runs where its copy goes, once the copy is in.
        if self.waited_on(self.focused(cx), cx).is_some() {
            return;
        }
        let binding = draft.binding.clone();
        let composer = pane.composer.clone();
        if !self.admit_files(self.focused(cx), cx) {
            return;
        }
        let text = composer.read(cx).prompt().trim().to_string();
        if text.is_empty() {
            return;
        }
        let provider = binding.provider().clone();
        let effort = binding.effort().map(str::to_owned);
        let machine = binding.project().machine();
        self.sync_setup_before_start(machine, cx);
        let resolved = binding
            .resolve(
                self.cockpit.read(cx).project(binding.project()),
                !self.elsewhere(machine, cx),
            )
            .map_err(|e| e.to_string());
        let opened = resolved.and_then(|choice| {
            self.cockpit
                .update(cx, |cockpit, _| {
                    cockpit.bootstrap_draft(machine, id, provider, choice, &text, effort.clone())
                })
                .map_err(|e| e.to_string())
        });
        self.finish_draft_start(id, &text, opened, cx);
        cx.notify();
    }

    fn finish_draft_start(
        &mut self,
        id: ferrite_core::roster::DraftId,
        text: &str,
        opened: Result<Option<ferrite_client::cockpit::Bootstrapped>, String>,
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
                self.adopt_thread(index, done.thread, cx);
                self.promote_draft(id, done.thread, cx);
                self.sync_panes(cx);
                self.thread_opened(done.thread, cx);
                self.refresh_names();
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
            .read(cx)
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
        // Where the draft runs: a chip while there is more than one
        // environment to choose, else a word for a machine.
        let machine = draft.binding.project().machine();
        let waits = self.waited_on(index, cx).is_some();
        let environment = self.environment_chip_shown(cx).then(|| {
            // The copy the draft waits on: where to, and how far.
            if let Some((_, copy)) = self.waited_on(index, cx) {
                let name = copy.order.placing.machine_name.clone();
                let done = copy.phase.done();
                let label = match done {
                    Some(done) => format!("{name} {}%", (done * 100.).floor()).into(),
                    None => name,
                };
                return (pane::BandChip::Environment, 2, pane::ChipMark::Ring(done.unwrap_or_default()), label);
            }
            let mark = if self.elsewhere(machine, cx) {
                ferrite_ui::icons::SERVER
            } else {
                ferrite_ui::icons::LAPTOP
            };
            (pane::BandChip::Environment, 2, pane::ChipMark::Icon(mark), self.environment_name(machine, cx))
        });
        let word = environment
            .is_none()
            .then(|| self.machine_label(machine, cx))
            .flatten()
            .map(|name| pane::env_machine(name.into()).into_any_element());
        let chips = environment.into_iter().chain([
            (
                pane::BandChip::Project,
                0,
                pane::ChipMark::Icon(ferrite_ui::icons::FOLDER),
                pane::band_chip_label(&project_title),
            ),
            (
                pane::BandChip::Workspace,
                1,
                pane::ChipMark::Icon(workspace_mark),
                pane::band_chip_label(&workspace_label),
            ),
        ]);
        let chips = chips
            .map(|(chip, slot, mark, label)| {
                let focused = draft.band_focus == Some(chip);
                let chip_composer = composer.clone();
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
                            view.focus_pane(at, cx);
                        }
                        view.open_band_menu(chip, false, cx);
                    }),
                );
                ferrite_ui::components::on_bounds(
                    chip_element,
                    self.record_trigger(band_trigger(chip)),
                )
                .into_any_element()
            });
        let mut chips: Vec<AnyElement> = word.into_iter().chain(chips).collect();
        if waits {
            // Beside the copy's percentage, why Send is not there.
            chips.insert(1, draft_environment::copy_waits_note().into_any_element());
        }
        chips
    }

    /// The focused draft's model chip (#29), where a live Thread's stands
    /// and as it reads (Claude's new-session `Opus 5.5 Medium`): the
    /// provider's mark, the model, then the effort in force when the model
    /// takes one. It opens the model picker on the draft's band choice.
    fn draft_model_chip(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let draft = self.panes[index].draft()?;
        let provider = draft.binding.provider().provider;
        let model = draft.binding.provider().model.as_deref();
        let announced = self
            .cockpit
            .read(cx)
            .announced_models(draft.binding.project().machine(), provider);
        // The model's name, the provider's default until one is chosen.
        let model_label = SharedString::from(ferrite_core::models::footer_name(
            provider, model, &announced,
        ));
        let effort = (!ferrite_core::models::efforts_for(provider, model, &announced)
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
        let picker_up = self.model_picker_open(identity);
        let face = pane::model_effort_chip(pane::ModelChip {
            mark: ferrite_ui::provider::mark(provider),
            model: model_label,
            effort: effort.map(|effort| pane::sentence_case(&effort).into()),
            loading: false,
            open: picker_up,
            group: group.clone(),
        });
        let control = pane::draft_picker(
            "draft-model-picker",
            draft.band_focus == Some(pane::BandChip::Provider),
            face,
            cx,
        )
        .tip_unless_open(picker_up, "Model and effort");
        Some(self.model_chip_control(index, control, cx))
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
        let (running, line_empty) = self.running_and_empty(index, cx);
        let queue = pane.thread().map_or_else(Vec::new, |thread| {
            composer_footer::queue_acts(index, thread, &self.cockpit, running, cx)
        });
        let open_thread = pane
            .thread()
            .and_then(|thread| Some((thread, self.cockpit.read(cx).thread(thread)?)));
        let attach = composer_footer::AttachMenu::of(
            index,
            pane.identity,
            self.attach_menu_focus(pane.identity),
            pane.composer.focus_handle(cx),
            match pane.geometry.get().composer {
                Some(field) => ferrite_ui::components::FloatPlace {
                    floor: f32::from(field.top()),
                    limit_right: f32::from(field.right()),
                },
                None => self.bar_place(cx),
            },
            open_thread.as_ref().map(|(thread, open)| (*thread, open)),
        );
        let attach = attach.render(cx);
        let (env, mut words, mode, ring) = match pane.thread() {
            None => {
                // A draft has no Session to ask: its mode is the one a new
                // Thread starts in (Settings), and its ring the provider's
                // account limits, as the Thread it becomes will read them.
                let ring = self.panes[index].draft().and_then(|draft| {
                    pane::plan_usage_ring(
                        ("draft-usage-ring", index),
                        self.cockpit.read(cx).account_limits(
                            draft.binding.project().machine(),
                            draft.binding.provider().provider,
                        ),
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
                    self.panes[index].draft().map(|draft| {
                        composer_footer::draft_mode_word(
                            index,
                            self.prefs
                                .settings
                                .permission_mode_for(draft.binding.provider().provider),
                        )
                    }),
                    ring.map(IntoElement::into_any_element),
                )
            }
            Some(thread) => {
                let open = self.cockpit.read(cx).thread(thread);
                let open = open.as_ref();
                // An empty Thread says where its work runs, as a draft
                // does; its Project and checkout are settled, so as words.
                let empty = open.is_some_and(|open| pane::offers_import(Some(open.transcript())));
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
                        ("plan-usage-ring", thread.thread().get() as usize),
                        self.cockpit.read(cx).account_limits(thread.machine(), open.provider()),
                        open.transcript().plan().map(SharedString::from),
                        self.usage_popover_held,
                        cx,
                    )
                });
                let chip = open.and_then(|open| {
                    let menu_up = self.mode_menu().is_some_and(|menu| menu.thread == thread);
                    composer_footer::ModeChip::of(index, thread, open, menu_up)
                });
                (
                    env,
                    self.model_chip(index, cx).into_iter().collect(),
                    chip.map(|chip| {
                        chip.render(self.record_trigger(mode_menu::chip_trigger(thread)), cx)
                    }),
                    ring.map(IntoElement::into_any_element),
                )
            }
        };
        // The model's word gives way first; the effort and the mode hold.
        let model = (!words.is_empty()).then(|| words.remove(0));
        words.extend(mode);
        Some(pane::ComposerControls {
            attach: Some(attach),
            stop: composer_footer::stop_button(index, running, line_empty, cx),
            // A draft waiting on its copy is sent once the copy is in.
            send: if self.waited_on(index, cx).is_some() {
                None
            } else {
                composer_footer::send_button(index, running, line_empty, cx)
            },
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
            .and_then(|thread| self.cockpit.read(cx).thread(thread))
            .is_some_and(|open| turn_running(&open));
        (running, pane.composer.read(cx).is_empty())
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
    fn aim_launch(&mut self, root: &std::path::Path, cx: &mut gpui::App) {
        self.launch_project = Some(
            self.cockpit
                .update(cx, |cockpit, _| {
                    let machine = cockpit.this_computer().expect("this computer is held");
                    cockpit.register_project(machine, root).now()
                })
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
        (index == self.focused(cx)
            && self.composer_menu.read(cx).pane() == Some(self.panes[index].identity))
            .then(|| self.composer_menu.clone().into_any_element())
    }

    /// A draft's Project or Workspace menu, hung off its chip on the strip
    /// over the box (ADR 0014): its left edge at the chip's, its foot
    /// `FLOAT_OFFSET` over the chip — or, opened from the hero's project
    /// word, `FLOAT_OFFSET` under it — kept inside the window's margin. The chip has been
    /// drawn before it can be pressed, so its bounds are always recorded.
    fn band_menu_element(
        &self,
        open: &Popover<Band>,
        face: popups::Face,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use ferrite_ui::theme::{
            FLOAT_OFFSET, GRID_PAD, MENU_WINDOW_MARGIN, SLASH_MENU_W, WORKSPACE_MENU_W,
        };
        let Band { chip, from_hero: hero } = open.kind;
        let trigger = self.trigger(&open.kind.trigger())?;
        // Above its chip on the strip over the box; or below the hero's
        // project word, as Codex's switcher drops.
        let (anchor, at, opens) = if hero {
            (
                gpui::Anchor::TopLeft,
                gpui::point(trigger.left(), trigger.bottom() + px(FLOAT_OFFSET)),
                ferrite_ui::motion::Opens::Down,
            )
        } else {
            (
                gpui::Anchor::BottomLeft,
                gpui::point(trigger.left(), trigger.top() - px(FLOAT_OFFSET)),
                ferrite_ui::motion::Opens::Up,
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
        let surface = div()
            .debug_selector(|| "composer-menu".into())
            .w(px(width))
            .child(menu_surface(open, None, |_, _, _| {}, Self::pick, cx));
        Some(
            anchored()
                .anchor(anchor)
                .position(at)
                .snap_to_window_with_margin(px(GRID_PAD))
                .child(self.popup_face(face, "band-menu-in", surface, opens, cx))
                .into_any_element(),
        )
    }

    /// The nav's rows as it last read them: what it draws.
    #[cfg(test)]
    pub(super) fn nav_state(&self, cx: &gpui::App) -> nav::NavState {
        self.nav_view
            .read(cx)
            .inputs()
            .map(|inputs| inputs.state.clone())
            .unwrap_or_default()
    }

    /// Does the Project filter admit this Thread? `All projects` admits
    /// everything; a chosen Project admits only Threads whose cached
    /// Project is it — so a Thread whose Project is unknown appears under
    /// `All projects` alone, rather than being quietly filed under someone
    /// else's Project.
    fn admitted(&self, thread: ThreadKey, cx: &gpui::App) -> bool {
        // An owned Thread (a Worker, a Coordinator) is its owner's to list.
        if !self.cockpit.read(cx).operators(thread) {
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
    fn thread_row(&self, key: ThreadKey, cx: &gpui::App) -> nav::ThreadRow {
        let facts = self.facts.get(key);
        let open = self.cockpit.read(cx).thread(key);
        let now = ferrite_core::clock::system_time();
        // A Notice not yet seen makes a quiet Thread done.
        let unread = self
            .cockpit
            .read(cx)
            .notifications(key.machine())
            .attention(key.thread());
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
            key,
            name: self.facts.name(key),
            status,
            project: facts.and_then(|facts| facts.project_label.clone()),
            selected: false,
            tail: nav::NavTail::of(status, slot.as_ref(), age),
            branch: facts.and_then(Self::checkout_branch),
            worktree: facts.and_then(|facts| facts.worktree.clone()),
            pr: self.facts.pr(key).map(nav::NavPr::of),
            provider: open
                .map(|open| open.provider())
                .or_else(|| facts.and_then(|facts| facts.provider)),
            activity_at: facts.and_then(|facts| facts.last_used),
            working_since: open
                .filter(|_| working)
                .and_then(|open| open.turn().started),
            pinned: self.pins(cx).contains(&key),
            retitling: open.is_some_and(|open| open.titling()),
            // The nav wears its own picks (`NavView::card_picked`).
            picked: false,
            elsewhere: self.elsewhere_row(key, cx),
        };
        #[cfg(test)]
        drawn::record_row(
            key,
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

    fn land_on_thread(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        if let Some(owner) = self.cockpit.read(cx).owner(thread) {
            self.land_on_owned(thread, owner, cx);
            return;
        }
        if self.pane_for(thread).is_some() {
            self.focus_thread(thread, cx);
        } else {
            self.revive_thread(thread, cx);
        }
    }

    /// When a Thread was last used, for `sort: recent`. A Thread with no
    /// known time sorts to the bottom rather than to the top: an unknown
    /// time is not a recent one.
    fn last_used(&self, thread: ThreadKey) -> std::time::SystemTime {
        self.facts
            .last_used(thread)
            .unwrap_or(std::time::UNIX_EPOCH)
    }

    /// A running nav row's click: land on that Thread's Pane, in the view
    /// that shows it — the core's one door, like every other deliberate
    /// move.
    fn focus_thread(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        self.leave_destination();
        if self.cockpit.update(cx, |cockpit, _| cockpit.focus_thread(thread)) {
            if let Some(index) = self.pane_for(thread) {
                self.focus_pane(index, cx);
            }
            self.widen_scope_to(thread, cx);
            cx.notify();
        }
    }

    /// The operator went to a Thread the nav's Project scope hides: the
    /// scope widens to all Projects, so the shown Thread has its row in the
    /// list (and the Filter button says why). Owned Threads are never
    /// listed, so nothing here reveals them.
    fn widen_scope_to(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        if self.nav_filter.is_some() && self.cockpit.read(cx).operators(thread) && !self.admitted(thread, cx) {
            self.choose_nav_filter(None, cx);
        }
    }

    /// Revive one parked Thread into the view that shows it: the shared
    /// tail of cmd-o and a parked nav row's click (#21). When it does not
    /// come back, the nav says so.
    fn revive_thread(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        self.reopen_threads(vec![thread], "reopen", cx);
    }

    /// `revive_thread` for a caller that follows it with the window in
    /// hand: `then` hears whether the Thread came back.
    fn revive_thread_in(
        &mut self,
        thread: ThreadKey,
        window: &mut Window,
        then: impl FnOnce(&mut Self, bool, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let asked = self.cockpit.update(cx, |cockpit, _| cockpit.reopen(thread));
        self.when_answered_in(
            asked,
            window,
            move |view, answer, window, cx| {
                let back = view.reopened(vec![(thread, answer)], "reopen", cx);
                then(view, back, window, cx);
            },
            cx,
        );
    }

    /// Revive each of `threads`, in order, into the view that shows it.
    /// The nav names each that did not come back, by what `verb` was asked.
    fn reopen_threads(&mut self, threads: Vec<ThreadKey>, verb: &'static str, cx: &mut Context<Self>) {
        let asked = self.cockpit.update(cx, |cockpit, _| {
            threads.iter().map(|thread| cockpit.reopen(*thread)).collect()
        });
        self.when_all_answered(
            asked,
            move |view, answers, cx| {
                view.reopened(threads.into_iter().zip(answers).collect(), verb, cx);
            },
            cx,
        );
    }

    /// The Server answered for Threads asked back: the board's own state
    /// gives way only to a Thread that is back. Whether all came back.
    fn reopened(
        &mut self,
        answers: Vec<(ThreadKey, std::io::Result<()>)>,
        verb: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let outcomes: Vec<nav_undo::Outcome> = answers
            .into_iter()
            .map(|(thread, answer)| nav_undo::Outcome {
                thread,
                result: answer.map_err(|error| error.to_string()),
            })
            .collect();
        for back in outcomes.iter().filter(|outcome| outcome.result.is_ok()) {
            self.leave_destination();
            self.widen_scope_to(back.thread, cx);
        }
        self.sync_panes(cx);
        self.report_failures(verb, &outcomes);
        cx.notify();
        outcomes.iter().all(|outcome| outcome.result.is_ok())
    }

    /// Focus by grid index — the pointer's and the tests' spelling of the
    /// core's one door.
    fn focus_pane(&mut self, index: usize, cx: &mut gpui::App) {
        if let Some(identity) = self.panes.get(index).map(|pane| pane.identity) {
            self.cockpit.update(cx, |cockpit, _| cockpit.focus(identity));
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
    fn reopen_thread(&mut self, _: &ReopenThread, window: &mut Window, cx: &mut Context<Self>) {
        let Some((thread, asked)) = self.cockpit.update(cx, |cockpit, _| cockpit.reopen_last()) else {
            return;
        };
        // What `switch_thread` reads before the switch, read at the press:
        // the Thread is back only once its Server answers.
        let shown = self.focused_thread(cx);
        let on_stop = self.panel_control_focused(window, cx);
        self.when_answered_in(
            asked,
            window,
            move |view, answer, window, cx| {
                let outcome = nav_undo::Outcome {
                    thread,
                    result: answer.map_err(|e| e.to_string()),
                };
                if outcome.result.is_ok() {
                    view.widen_scope_to(thread, cx);
                    view.sync_panes(cx);
                }
                view.report_failures("reopen", &[outcome]);
                if on_stop && view.focused_thread(cx) != shown {
                    window.blur(cx);
                }
                cx.notify();
            },
            cx,
        );
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
        if let Some((index, _)) = self.lightbox(cx) {
            self.close_lightbox(index, cx);
            return;
        }
        let index = self.focused(cx);
        if let Some(pane) = self.panes.get(index) {
            if self.preview_shown(index, cx) && pane.preview.focused(window, cx) {
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
        let Some(identity) = self.cockpit.read(cx).roster().focused() else {
            return;
        };
        self.close_pane(identity, cx);
    }

    /// Close one Pane through the core act (a Thread always parks), its
    /// refusal landing in the nav's notice — and a park that would not
    /// flush is logged, the Pane gone either way.
    fn close_pane(&mut self, identity: PaneIdentity, cx: &mut Context<Self>) {
        let closing = self.cockpit.update(cx, |cockpit, _| cockpit.close(identity));
        // A menu on the closed Pane goes with it.
        if self.composer_menu.read(cx).pane() == Some(identity) {
            self.close_composer_menu(cx);
        }
        self.when_answered(
            closing,
            move |view, closed, cx| {
                if let Err(error) = &closed {
                    ferrite_core::log!("pane {identity:?} did not close cleanly: {error}");
                }
                view.sync_panes(cx);
                if let (true, PaneIdentity::Draft(draft)) = (closed.is_ok(), identity) {
                    view.discard_draft(draft, cx);
                }
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    /// Jump to the next Thread waiting on the operator.
    fn next_decision(&mut self, _: &NextDecision, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.focused(cx);
        let mut requests = Vec::new();
        // The answer order (`Cockpit::needs_you`): ⌘D walks the answer
        // queue top to bottom, so its first press lands on the answer target.
        let queue = self.cockpit.read(cx).needs_you();
        let mut panes: Vec<&CockpitPane> = self.panes.iter().collect();
        panes.sort_by_key(|pane| {
            pane.thread()
                .and_then(|thread| queue.iter().position(|queued| *queued == thread))
                .unwrap_or(usize::MAX)
        });
        for pane in panes {
            if let Some(thread) = pane.thread() {
                if let Some(open) = self.cockpit.read(cx).thread(thread) {
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
        self.focus_pane(index, cx);
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
        if let Some(transcript) = self.panes.get(self.focused(cx)).and_then(|pane| pane.transcript()) {
            transcript.update(cx, |transcript, cx| transcript.cite_selection(window, cx));
        }
    }

    /// ⌘V anywhere in the window lands in the focused Pane's Composer.
    /// The Composer's own Paste runs first while it holds the keyboard;
    /// this is the fallback for when a click in the transcript — a drag
    /// to select, a tool row — took the keyboard away: pasted text or files
    /// go where the operator is about to type, and the keyboard follows.
    fn paste_into_composer(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(line) = self.nav_view.read(cx).snooze_line().cloned() {
            if let Some(item) = cx.read_from_clipboard() {
                line.update(cx, |line, cx| line.paste_item(item, cx));
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
        if self.panes.get(self.focused(cx)).is_none_or(|pane| !pane.is_main()) {
            return;
        }
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let index = self.focused(cx);
        let Some(pane) = self.panes.get(index) else {
            return;
        };
        if self.rename.is_open() {
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

/// How many sessions the import picker lists (#11), newest first: enough
/// history to search, few enough that the listing reads a bounded slice
/// of each file rather than every session ever run.
const IMPORT_ROWS_MAX: usize = 200;

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

impl Band {
    /// The key the control this menu hangs from records its bounds under.
    fn trigger(&self) -> SharedString {
        if self.from_hero {
            HERO_TRIGGER.into()
        } else {
            band_trigger(self.chip)
        }
    }
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
    let rows = ferrite_core::models::catalog(provider, announced);
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
        ferrite_core::models::cli_default_effort(provider, model).map(effort_chip_label)
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
    let steps = ferrite_core::models::efforts_for(provider, model, announced);
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

impl CockpitView {
    /// The nav's, the side chat's, Files', the Overview's and the toasts'
    /// events, heard from the Cockpit's first frame: answering some needs
    /// the window.
    fn hear_views(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.view_events.is_none() {
            self.view_events = Some(vec![
                cx.subscribe_in(&self.nav_view, window, Self::on_nav_event),
                cx.subscribe_in(&self.side_chat, window, Self::on_side_chat_event),
                cx.subscribe_in(&self.files, window, Self::on_files_event),
                cx.subscribe_in(&self.overview, window, Self::on_overview_event),
                cx.subscribe_in(&self.toasts, window, Self::on_toast_event),
                cx.subscribe_in(&self.file_surface, window, Self::on_file_surface_event),
                cx.subscribe_in(&self.decisions, window, Self::on_decision_event),
                cx.subscribe_in(&self.browser_chrome, window, Self::on_browser_chrome_event),
                cx.subscribe_in(&self.browser_dialog, window, Self::on_browser_dialog_event),
                cx.subscribe_in(&self.browser_review, window, Self::on_browser_review_event),
                cx.subscribe_in(&self.empty_board, window, Self::on_empty_board_event),
                cx.subscribe_in(&self.composer_menu, window, Self::on_composer_menu_event),
            ]);
        }
    }
}

impl Render for CockpitView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ferrite_ui::motion::hover_frame_start(cx);
        self.tend_reader_slides(cx);
        if let Some((_, editor)) = self.rename.shown() {
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
            self.cockpit.update(cx, |cockpit, _| cockpit.set_window_active(window.is_window_active()));
            self.activation = Some(cx.observe_window_activation(window, |view, window, cx| {
                if view.cockpit.update(cx, |cockpit, _| cockpit.set_window_active(window.is_window_active())) {
                    cx.notify();
                }
            }));
            self.watch_lost_focus(window, cx);
        }
        self.hear_views(window, cx);
        self.tend_history(cx);
        self.tend_popup_focus(window, cx);
        self.tend_sheet_focus(window, cx);
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
        let notices = self
            .cockpit
            .read(cx)
            .every_notifications()
            .flat_map(|notifications| {
                notifications
                    .reading()
                    .notices()
                    .map(|notice| notice.at)
                    .chain(notifications.reading().decisions().map(|request| request.at))
            })
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
            let Some(open) = pane.thread().and_then(|thread| self.cockpit.read(cx).thread(thread)) else {
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
        for thread in self.cockpit.read(cx).threads() {
            if let Some(next) = rollover(self.cockpit.read(cx).thread(thread).map(|open| open.turn())) {
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
        self.update_auto_fold(window, cx);
        self.tend_columns(cx);
        self.tend_details(window, cx);
        if self.nav_folded() {
            self.nav_view.update(cx, |nav, cx| nav.close_snooze(cx));
        }
        self.present_cli_updates(window, cx);
        self.present_ferrite_update(window, cx);
        self.present_server(window, cx);
        self.present_setup_sync(window, cx);
        self.present_copies(window, cx);
        self.present_restart(window, cx);
        self.present_unreadable_settings(window, cx);
        self.present_user_commands(window, cx);
        self.present_allowance_warnings(window, cx);
        self.present_asked(window, cx);
        self.retire_task_toasts(window, cx);
        self.place_toasts(window, cx);
        self.maximized = window.is_maximized();
        self.leave_destination_if_moved(cx);
        let drawn = self.transcripts_drawn(cx);
        self.cockpit.update(cx, |cockpit, _| cockpit.draws_transcripts(drawn));

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

        // The `/`/`@` menu belongs to the focused Pane's Composer (#23):
        // leaving that Pane closes it here.
        if let Some(on) = self.composer_menu.read(cx).pane() {
            if self
                .panes
                .get(self.focused(cx))
                .is_none_or(|pane| pane.identity != on)
            {
                self.close_composer_menu(cx);
            }
        }
        let pane_rects = self.pane_rects(window, cx);
        for (index, pane) in self.panes.iter().enumerate() {
            let row_limit = pane_rects
                .iter()
                .find(|(at, _)| *at == index)
                .map(|(_, rect)| {
                    let queued = pane
                        .thread()
                        .and_then(|thread| self.cockpit.read(cx).thread(thread))
                        .map_or(0, |thread| thread.queue().len());
                    pane::composer_row_limit(rect.h, queued)
                })
                .unwrap_or(ferrite_pane::composer::MAX_ROWS);
            // While a question asks, the Composer keeps to two rows so the
            // question has the Pane; it is derived every frame, so answering
            // or changing Subjects gives the draft its rows back.
            let answering_question = pane
                .thread()
                .and_then(|thread| self.cockpit.read(cx).thread(thread))
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
        // The model is lent, not written: each Composer copies the commands
        // it chips under its own update.
        self.cockpit.update(cx, |model, cx| {
            for pane in &self.panes {
                if let Some(open) = pane.thread().and_then(|thread| model.thread(thread)) {
                    pane.composer.update(cx, |composer, cx| {
                        composer.set_commands(open.commands(), cx)
                    });
                }
            }
        });
        let history_available: Vec<bool> = (0..self.panes.len())
            .map(|index| self.history_available(index, cx))
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
        // first Thread. A machine whose Server went is read-only (ADR
        // 0026): while the focused Pane is one of its, or with no Pane the
        // machine a new Thread starts on is, the root holds the keyboard,
        // so no Composer or control there takes a key until it is back.
        let lost = self
            .pane_machine(self.focused(cx))
            .or_else(|| self.empty_board_shown(cx).then(|| self.start_machine(cx)).flatten())
            .is_some_and(|machine| self.machine_lost(machine, cx));
        let wanted = self
            .rename
            .shown()
            .map(|(_, editor)| editor.focus_handle(cx))
            .or_else(|| {
                self.nav_view
                    .read(cx)
                    .snooze_line()
                    .map(|line| line.focus_handle(cx))
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
                let (index, _) = self.lightbox(cx)?;
                self.panes[index].preview.held_focus(window, cx)
            })
            .or_else(|| {
                let index = self.focused(cx);
                if !self.preview_shown(index, cx) { return None; }
                let pane = self.panes.get(index)?;
                if !pane.preview.document().is_some_and(|document| matches!(&document.body, ferrite_pane::preview::Body::Browser)) { return None; }
                self.browser_annotation_focus(pane.thread()?, cx)
            })
            .or_else(|| {
                self.panes
                    .get(self.focused(cx))?
                    .review
                    .held_focus(window, cx)
            })
            .or_else(|| {
                let pane = self.panes.get(self.focused(cx))?;
                // A Thread whose facts have not landed draws no Composer:
                // the root holds the keyboard until they do.
                if pane
                    .thread()
                    .is_some_and(|thread| self.cockpit.read(cx).thread(thread).is_none())
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
        } else if !self.rename.is_open() && self.prs_shown() {
            self.prs.read(cx).focus.clone()
        } else if !self.rename.is_open() && self.empty_board_shown(cx) {
            self.empty_board.read(cx).focus.clone()
        } else {
            wanted
        };
        use gpui::component::WindowExt as _;
        let native_text_focused = self
            .panes
            .get(self.focused(cx))
            .is_some_and(|pane| pane.transcript_focus.contains_focused(window, cx))
            && (ferrite_pane::rich::controls_focused(window)
                || gpui::base::TextSelection::has_selection(window, cx)
                || self.panes.get(self.focused(cx)).is_some_and(|pane| {
                    pane.rich.output_focused(&pane.text_namespace(), window, cx)
                }));
        let pane_control_focused = !lost
            && self.panes.get(self.focused(cx)).is_some_and(|pane| {
                let controls = pane
                    .thread()
                    .and_then(|id| self.cockpit.read(cx).thread(id))
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
        let reader_text_focused = self.panes.get(self.focused(cx)).is_some_and(|pane| {
            (self.preview_shown(self.focused(cx), cx) && pane.preview.text_focused(window, cx))
                || pane.review.inner_focused(window, cx)
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
            self.panel_keys.contains_focused(window, cx) || self.panel_control_focused(window, cx);
        let field_holds_keyboard = self.field_holds_keyboard(&wanted, window, cx);
        let dialog_took_focus = self.focus_shown_dialogs(window, cx);
        // Here, before focus is handed out: a page's dialog is modal and
        // keeps the keys, and a popup left up would take them back.
        self.close_popup_with_nothing_to_draw(dialog_took_focus, cx);
        if window.has_active_dialog(cx)
            || dialog_took_focus
            || self.dialog_holds_focus(window, cx)
            || self.floats_hold_focus()
            || nav_list_focused
            || panel_keys_held
            || self.details_hold_focus(window, cx)
            || native_text_focused
            || reader_text_focused
            || self.terminal_holds_focus(window, cx)
            || pane_control_focused
        {
            // Native text, dialogs, and the Inbox keep their own keyboard focus.
        } else if self.popup_up() {
            // A popup puts the keyboard where it says (`tend_popup_focus`).
        } else if let Some(sheet) = self.sheets.read(cx).up().cloned() {
            // The pump must not steal focus from a sheet's fields or controls.
            let connection_focused = self
                .connections(cx)
                .is_some_and(|connections| connections.read(cx).holds_focus(window, cx));
            if !sheet.holds_focus(window, cx) && !connection_focused {
                window.focus(&sheet.focus(cx), cx);
            }
        } else if field_holds_keyboard {
        } else if !wanted.is_focused(window)
            && (wanted == self.focus || !wanted.contains_focused(window, cx))
        {
            window.focus(&wanted, cx);
        }

        // The cards start under the head row (`frame::board_top`), flush to
        // the panel (`board_bounds`), riding or at rest.
        let board_top = ferrite_ui::frame::board_top();
        let panel_w = self.panel_column_w(cx);
        let frame_right = self.board_right(cx) - panel_w;
        let frame = || {
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .pt(px(board_top))
                .pr(px(frame_right))
        };
        let visible = self.visible_indices(cx);
        // The empty board names the provider CLIs it found: ask once.
        if self.cockpit.read(cx).environment_available()
            && self.empty_board_shown(cx)
            && self.cli_versions.is_none()
            && !self.cli_probing
            && !cfg!(test)
        {
            self.cli_probing = true;
            self.probe_cli_versions(cx);
        }
        let grid = if !self.cockpit.read(cx).environment_available() {
            frame().flex().child(self.no_environment(cx))
        } else if self.factory().is_some() {
            // A Project's Factory board stands in the board's place.
            frame().flex().child(self.factory_board(window, cx))
        } else if self.prs_shown() {
            frame().flex().child(self.prs.clone())
        } else if let Some((board, tree)) = self
            .board(cx)
            .and_then(|board| self.board_tree(board, cx).map(|tree| (board, tree)))
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
            frame().flex().child(self.empty_board.clone())
        } else {
            // The shown Pane, the whole board. No tree lays out slots
            // here, so none of its sizes stands.
            self.slot_sizes.borrow_mut().clear();
            frame()
                .flex()
                .flex_col()
                .child(self.pane_slot(visible[0], window, cx))
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
            .key_context(ferrite_ui::keymap::root_context(self.popup_takes_keys(cx)).as_str())
            // Nothing that acts on a lost machine's Threads is listened for
            // until it is back (ADR 0026).
            .when(!lost, |root| {
                root.on_action(cx.listener(Self::submit))
                    .on_action(cx.listener(|view, _: &ferrite_pane::composer::SaveAndSendComment, window, cx| {
                        let Some(pane) = view.panes.get(view.focused(cx)) else {
                            return;
                        };
                        let composer = pane.composer.clone();
                        if composer.update(cx, |composer, cx| composer.save_comment(window, cx)) {
                            view.submit(&Submit, window, cx);
                        }
                    }))
                    .on_action(cx.listener(Self::unqueue_from_backspace))
                    .on_action(cx.listener(Self::allow))
                    .on_action(cx.listener(Self::deny))
                    .on_action(cx.listener(Self::always))
                    .on_action(cx.listener(|view, _: &AttachFiles, _, cx| {
                        let index = view.focused(cx);
                        if view.panes.get(index).is_some_and(|pane| pane.is_main()) {
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
                cx.listener(|view, _: &FocusThread1, window, cx| view.go_to_thread(1, window, cx)),
            )
            .on_action(cx.listener(Self::toggle_terminal))
            .on_action(cx.listener(Self::toggle_changes))
            .on_action(cx.listener(Self::toggle_files))
            .on_action(cx.listener(Self::toggle_file_tree_action))
            .on_action(cx.listener(Self::go_to_file_action))
            .on_action(
                cx.listener(|view, _: &FocusThread2, window, cx| view.go_to_thread(2, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread3, window, cx| view.go_to_thread(3, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread4, window, cx| view.go_to_thread(4, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread5, window, cx| view.go_to_thread(5, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread6, window, cx| view.go_to_thread(6, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread7, window, cx| view.go_to_thread(7, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread8, window, cx| view.go_to_thread(8, window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &FocusThread9, window, cx| view.go_to_thread(9, window, cx)),
            )
            .on_action(cx.listener(Self::toggle_notifications))
            .on_action(cx.listener(|view, _: &nav::Focus, window, cx| view.focus_nav(window, cx)))
            .on_action(cx.listener(|view, _: &RefreshPrStatus, _, cx| {
                if let Some(thread) = view.focused_thread(cx) {
                    view.check_pr(thread, true, cx);
                }
            }))
            .on_action(cx.listener(|view, _: &OpenPrInBrowser, _, cx| {
                if let Some(thread) = view.focused_thread(cx) {
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
            // again as it bubbles (`FileSurfaceView::card`).
            .capture_any_mouse_down(cx.listener(|view, _: &MouseDownEvent, _, cx| {
                view.file_surface.update(cx, |files, _| files.forget_press());
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
                    // The menu mutes until the text moves, or the very
                    // next frame would reopen it over the same trigger.
                    let dismissed = view
                        .composer_menu
                        .update(cx, |menu, cx| menu.dismiss(cx));
                    // A press outside the editor abandons the rename — the
                    // operator looked away, which is escape, not enter.
                    // It notifies itself, so it is not counted above.
                    if view.rename.is_open() {
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
                if key.key == "escape" && !key.modifiers.modified() && view.dismiss_popup(cx) {
                    cx.stop_propagation();
                } else if let Some(index) = view.menu_mnemonic(key) {
                    cx.stop_propagation();
                    view.press_menu_row(index, window, cx);
                } else if view.menu_navigate(key, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                view.typing_leaves_control(&event.keystroke, window, cx);
            }))
            .on_action(cx.listener(Self::open_settings))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .children(self.nav(cx))
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
            .children(self.popup_element(window, cx))
            .child(self.sheets.clone())
            .child(self.toast_float())
            .children(self.sheet_floats(window, cx))
            .children(self.lightbox_element(cx))
            .children(gpui::component::Root::render_dialog_layer(window, cx))
            // The kit's toasts, where the theme hangs them whatever is up
            // (`TOAST_FOOT`).
            .children(gpui::component::Root::render_notification_layer(window, cx).map(|layer| {
                deferred(layer).with_priority(crate::float::FLOAT_PRIORITY)
            }))
    }
}

impl CockpitView {
    /// Pane `index`'s cell — the click-to-focus and drag plumbing around
    /// `render_pane`, for the whole board or a slot of its tree. While its
    /// machine is lost it is dimmed under a cover that takes every press
    /// until the machine is back (ADR 0026).
    fn pane_cell(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let cell = self.live_pane_cell(index, window, cx);
        match self.pane_machine(index) {
            Some(machine) if self.machine_lost(machine, cx) => {
                machines::held(cell, format!("machine-lost-{}", machine.get()))
            }
            _ => cell,
        }
    }

    fn live_pane_cell(&self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> Div {
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
                        view.focus_pane(index, cx);
                        if let Some(draft) = view.panes[index].draft_mut() {
                            draft.band_focus = None;
                        }
                        view.close_composer_menu(cx);
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
        let focused = index == self.focused(cx);
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
                    view.focus_pane(index, cx);
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
                    start: {
                        let hero = self.draft_hero(index, cx);
                        self.under_sheets(hero, cx)
                    },
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
        // One Pane is drawn at a time: this names the Thread the window
        // shows, for a driver to read.
        let cell = cell.debug_selector(move || format!("view-thread-{}", thread.thread().get()));
        // Over a link, the roster can list a Thread a frame before its
        // facts land: the cell stays an empty shell until they do.
        if self.cockpit.read(cx).thread(thread).is_none() {
            return cell;
        }
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
        let attachments = Composer::attachments(&pane.composer, &pane.preview, cx);
        // The docked Decision merges into the Composer when that Composer
        // is a live block with nothing floating between them (rule 2.8.1).
        let joins = pane.is_main() && attachments.is_none();
        let activity_decisions = self.activity_decisions(index, joins, window, cx);
        let menu = self.popover_element(index, cx);
        let details = self.details_card(index, window, cx);
        let toolbar = self.composer_toolbar(index, cx);
        let resume_offer = self.resume_offer(index, cx);
        // The parts above draw with `cx`; the facts read the model after.
        let Some(open) = self.cockpit.read(cx).thread(thread) else {
            return cell;
        };
        let facts = pane::PaneFacts {
            composer_empty: pane.composer.read(cx).is_empty(),
            focused,
            unread: self
                .cockpit
                .read(cx)
                .notifications(thread.machine())
                .attention(thread.thread()),
            reduce_motion: cx.reduce_motion(),
            editing: focused
                && window.is_window_active()
                && pane.composer.read(cx).focus_handle(cx).is_focused(window),
            drop_target: self.drop_target(index, cx),
            plane: self.on_plane(thread, cx),
            decision_joined: joins && activity_decisions.is_some(),
            thread: open,
        };
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
                    .get(&pane_leaf(pane.identity))
                    .map(|(w, _)| *w)
                    .or_else(|| {
                        self.pane_rects(window, cx)
                            .into_iter()
                            .find(|(at, _)| *at == index)
                            .map(|(_, rect)| rect.w)
                    })
                    .unwrap_or_else(|| self.board_bounds(window, cx).w),
                clock: open.turn().elapsed().map(ferrite_core::progress::live_seconds),
            },
        );
        let wiring = pane::PaneWiring {
            transcript: retained_transcript,
            attachments,
            menu,
            activity_decisions,
            details,
            details_lane_width: self.details_lane_width(index),
            toolbar,
            resume_offer,
            // An empty Thread wears the draft's start card until its first
            // prompt.
            start: if pane.is_main() && open.transcript().blocks().is_empty() {
                self.under_sheets(self.thread_hero(thread), cx)
            } else {
                None
            },
        };
        cell.child(pane::render_pane(pane, facts, wiring))
    }

    /// Read free disk for the nav's foot off the UI thread: the least free of the
    /// volumes holding the Threads' checkouts (SPEC §8), or a fixture
    /// world's figure. Never another machine's, whose checkouts are its
    /// own; tests and captures never read the real disk.
    fn read_free_disk(&mut self, cx: &mut Context<Self>) {
        #[cfg(any(test, feature = "demo"))]
        if let Some(bytes) = self.cockpit.read(cx).free_disk_fixture() {
            self.free_disk_landed(Some(bytes), cx);
            return;
        }
        if cfg!(any(test, feature = "visual-reference")) {
            return;
        }
        let mut paths: Vec<std::path::PathBuf> = self
            .cockpit
            .read(cx)
            .threads()
            .into_iter()
            .filter(|thread| !self.elsewhere(thread.machine(), cx))
            .filter_map(|thread| self.thread_path(thread, cx))
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
    fn slot_drop_target(&self, cell: Div, target: Leaf, cx: &mut Context<Self>) -> Div {
        // gpui hands every drag move to every listener, not just the one
        // under the pointer: a slot only previews while the pointer is
        // inside it, and drops its own preview once the pointer has left —
        // else the first slot crossed keeps saying "split left" while the
        // drag is somewhere else.
        fn left(view: &mut CockpitView, target: Leaf, cx: &mut Context<CockpitView>) {
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
    fn slot_size(&self, leaf: Leaf) -> (f32, f32) {
        self.slot_sizes
            .borrow()
            .get(&leaf)
            .copied()
            .unwrap_or((GHOST_MAX_W, GHOST_MAX_H))
    }

    fn toggle_tool(
        &mut self,
        thread: ThreadKey,
        call: &pane::DisclosureId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        self.focus_pane(index, cx);
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
        let index = self.focused(cx);
        let typing = self
            .panes
            .get(index)
            .is_some_and(|pane| pane.is_main() && !pane.composer.read(cx).is_empty());
        if !typing
            && self.decision_key_taken(index, decisions::DecisionKey::Pick(option), window, cx)
        {
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
        let Some(pane) = self.panes.get(self.focused(cx)) else {
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
    fn typed_tuning(&self, thread: ThreadKey, text: &str, cx: &gpui::App) -> Option<Tuning> {
        let open = self.cockpit.read(cx).thread(thread)?;
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
                let ladder = ferrite_core::models::efforts_for(
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
                    .read(cx)
                    .model_catalog(thread.machine(), open.provider())
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


    /// The offer to go on in a fresh conversation, while the Thread's
    /// provider has none to resume. It says what the way on costs.
    fn resume_offer(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.panes.get(index).filter(|pane| pane.is_main())?.thread()?;
        self.cockpit.read(cx).thread(thread).filter(|open| open.resume_lost())?;
        let key = thread.thread().get();
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
        thread: ThreadKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.cockpit.read(cx).thread(thread).is_some_and(|open| open.resume_lost()) {
            return;
        }
        let pane = self.panes[index].identity;
        let asked = self.cockpit.update(cx, |cockpit, _| cockpit.start_fresh_conversation(thread));
        self.when_answered_in(
            asked,
            window,
            move |view, answered, window, cx| {
                cx.notify();
                if let Err(error) = answered {
                    view.cockpit.update(cx, |cockpit, _| {
                        cockpit.say(thread, format!("could not start a fresh conversation: {error}"))
                    });
                    return;
                }
                let Some(index) = view.index_of(pane) else {
                    return;
                };
                view.focus_pane(index, cx);
                if !view.panes[index].composer.read(cx).is_empty() {
                    view.submit(&Submit, window, cx);
                }
            },
            cx,
        );
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
        cx: &gpui::App,
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
        Some(self.board_card_corner(up, window, cx))
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
    fn bar_place(&self, cx: &gpui::App) -> ferrite_ui::components::FloatPlace {
        use ferrite_ui::theme::GRID_PAD;
        // The board stops at the bar and `board_right` short of the
        // window's right edge.
        let board = self.board.get();
        ferrite_ui::components::FloatPlace {
            floor: board.y + board.h,
            limit_right: board.x + board.w + self.board_right(cx) - GRID_PAD,
        }
    }

    /// Where a card hangs with no geometry to hang from: off the board as
    /// though it were the Pane — from its foot (where a Composer would be)
    /// for a card opening up, its head for one opening down.
    fn board_card_corner(&self, up: bool, window: &Window, cx: &gpui::App) -> Point<Pixels> {
        let board = self.board_bounds(window, cx);
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

    /// A pick of permission mode `mode`. A mode that stops asking
    /// (`theme::mode_stops_asking`) is confirmed first, as Codex confirms
    /// Full access; any other goes at once.
    pub(super) fn request_mode(
        &mut self,
        thread: ThreadKey,
        generation: u64,
        mode: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let set = ferrite_core::SessionControl::SetPermissionMode { mode: mode.clone() };
        let standing = self
            .cockpit
            .read(cx)
            .thread(thread)
            .is_some_and(|open| open.permission_mode() == Some(mode.as_str()));
        if standing || !ferrite_ui::theme::mode_stops_asking(&mode) {
            self.run_session_control(thread, generation, set, cx);
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
                    view.run_session_control(thread, generation, set, cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// The Session controls card (`session_card::SessionCard`), hung over
    /// the Composer.
    fn session_controls_element(
        &self,
        thread: ThreadKey,
        generation: u64,
        face: popups::Face,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let at = self.card_corner(
            format!("session-{}", thread.thread().get()),
            CardFrom::Composer(PaneIdentity::Thread(thread)),
            window,
            cx,
        )?;
        let card = session_card::SessionCard::of(
            thread,
            generation,
            &self.cockpit.read(cx).thread(thread)?,
            self.session_control_error.of(thread, generation),
        )?;
        Some(
            anchored()
                .anchor(gpui::Anchor::BottomRight)
                .position(at)
                .snap_to_window_with_margin(px(ferrite_ui::theme::GRID_PAD))
                .child(self.popup_face(
                    face,
                    "session-controls-in",
                    card.render(cx),
                    ferrite_ui::motion::Opens::Up,
                    cx,
                ))
                .into_any_element(),
        )
    }

    /// The checks card (`head_cards::checks_card`), hung under the Pane
    /// head.
    fn context_checks_element(
        &self,
        thread: ThreadKey,
        face: popups::Face,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let at = self.card_corner(
            format!("ci-{}", thread.thread().get()),
            CardFrom::Head(PaneIdentity::Thread(thread)),
            window,
            cx,
        )?;
        let card = head_cards::checks_card(thread, self.facts.pr(thread)?, cx);
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
            ferrite_core::models::efforts_for(provider, open.model(), open.models());
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
        let open = self.cockpit.read(cx).thread(thread)?;
        let provider = open.provider();
        let hold = tuning_hold(&open);
        // The model it runs; until one is known, the provider's default
        // model — never the provider, and never the id on the wire.
        let label = SharedString::from(ferrite_core::models::footer_name(
            provider,
            open.running_model(),
            open.models(),
        ));
        let effort = self.thread_effort(&open);
        let key = thread.thread().get() as usize;
        let identity = PaneIdentity::Thread(thread);
        let picker_up = self.model_picker_open(identity);
        let face = pane::model_effort_chip(pane::ModelChip {
            mark: ferrite_ui::provider::mark(provider),
            model: label,
            effort: effort.map(|effort| pane::sentence_case(&effort).into()),
            loading: open.starting(),
            open: picker_up,
            group: model_picker::chip_trigger(identity),
        });
        let control = pane::toolbar_chip(("model-picker", key), cx)
            .debug_selector(move || format!("model-picker-{key}"))
            .tip_unless_open(picker_up, hold.unwrap_or("Model and effort"))
            .child(face);
        Some(self.model_chip_control(index, control, cx))
    }

    /// The model's effort ladder for `thread` (`effort_ladder_of`).
    pub(super) fn effort_ladder(
        &self,
        thread: ThreadKey,
        cx: &gpui::App,
    ) -> Option<(Vec<String>, Vec<SharedString>, Option<usize>)> {
        let open = self.cockpit.read(cx).thread(thread)?;
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
        cx: &gpui::App,
    ) -> Option<(Vec<String>, Vec<SharedString>, Option<usize>)> {
        let draft = self.panes.get(index)?.draft()?;
        let provider = draft.binding.provider().provider;
        effort_ladder_of(
            draft.binding.effort(),
            self.prefs.settings.effort_for(provider),
            provider,
            draft.binding.provider().model.as_deref(),
            &self
                .cockpit
                .read(cx)
                .announced_models(draft.binding.project().machine(), provider),
        )
    }

    /// An effort-row pick: the core re-aims the Thread (eagerly before the
    /// first prompt, by resuming after it, refused mid-turn — its own
    /// words land in the transcript either way).
    fn pick_effort(&mut self, thread: ThreadKey, effort: Option<String>, cx: &mut Context<Self>) {
        let asked = self.cockpit.update(cx, |cockpit, _| cockpit.set_effort(thread, effort));
        self.when_tuned(thread, asked, "effort", cx);
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
        self.close_composer_menu(cx);
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
            Verb::Open(machine, id) => {
                self.drop_toast(machine, &crate::notifications::RowTarget::Notice(id), cx);
                if let Some((_, landed)) = self
                    .cockpit
                    .update(cx, |cockpit, _| cockpit.open_notice(machine, id))
                {
                    self.when_answered(
                        landed,
                        |view, landed, cx| {
                            if landed.is_ok() {
                                view.sync_panes(cx);
                                cx.notify();
                            }
                        },
                        cx,
                    );
                }
                self.nav_view.update(cx, |nav, cx| nav.close_snooze(cx));
            }
            Verb::Dismiss(machine, id) => {
                self.cockpit
                    .update(cx, |cockpit, _| cockpit.dismiss_notice(machine, id));
            }
            Verb::OpenDecision(machine, id) => {
                self.drop_toast(
                    machine,
                    &crate::notifications::RowTarget::Decision(id.clone()),
                    cx,
                );
                let notifications = self.cockpit.read(cx).notifications(machine);
                let subject = notifications
                    .decision(&id)
                    .and_then(|notice| notice.subject.clone())
                    .unwrap_or(ferrite_core::activity::Subject::Main);
                let thread = notifications.key(id.thread);
                let pending_subject = self
                    .cockpit
                    .read(cx)
                    .thread(thread)
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
                // A request whose Notice is gone, or whose Thread did not
                // take the screen, is still landed on while it is pending.
                let pending = move |view: &mut Self, cx: &mut Context<Self>| {
                    if let Some(subject) = pending_subject {
                        view.land_on_thread(thread, cx);
                        view.select_subject_from_notice(thread, subject, cx);
                    }
                };
                match self
                    .cockpit
                    .update(cx, |cockpit, _| cockpit.open_decision_notice(machine, &id))
                {
                    Some((thread, landed)) => self.when_answered(
                        landed,
                        move |view, landed, cx| {
                            if landed.is_ok() {
                                view.sync_panes(cx);
                                view.select_subject_from_notice(thread, subject, cx);
                            } else {
                                pending(view, cx);
                            }
                            cx.notify();
                        },
                        cx,
                    ),
                    None => pending(self, cx),
                }
                self.nav_view.update(cx, |nav, cx| nav.close_snooze(cx));
            }
            Verb::DismissDecision(machine, id) => {
                self.cockpit
                    .update(cx, |cockpit, _| cockpit.dismiss_decision_notice(machine, &id));
            }
            // Every row read on every machine, none removed: titles drop
            // to plain, the badge clears.
            Verb::MarkAllRead => {
                self.cockpit.update(cx, |cockpit, _| cockpit.mark_notices_read());
            }
            Verb::Answer(..) => {}
        }
        cx.notify();
    }

    /// A toast's `1 allow` / `3 deny` (FL-18): the decisions package's
    /// answer to that Thread's waiting approval, and the toast goes.
    fn answer_from_toast(
        &mut self,
        machine: ferrite_core::MachineId,
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
        self.drop_toast(
            machine,
            &crate::notifications::RowTarget::Decision(id.clone()),
            cx,
        );
        let thread = self.cockpit.read(cx).notifications(machine).key(id.thread);
        self.answer_approval(thread, choice, None, window, cx);
        cx.notify();
    }

    /// One completion Notice in the list's words (R7): `✗ failed` with the
    /// turn's failing test summary (a turn whose last test run failed is
    /// failed) or its error; else `✓ done` with the final answer when it is
    /// one line that fits the row, else `worked for <settled duration>`.
    fn notice_row(
        &self,
        key: ThreadKey,
        notice: &ferrite_core::notifications::Notice,
        now: std::time::SystemTime,
        cx: &gpui::App,
    ) -> NoticeRow {
        use crate::notifications::{Detail, State};
        let title = self.row_title(key, cx);
        let finish = self
            .cockpit
            .read(cx)
            .thread(key)
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
            target: crate::notifications::RowTarget::Notice(notice.id),
            thread: key,
            title,
            state,
            detail,
            when: crate::facts::since_label(notice.at, now),
            read: notice.read,
            elsewhere: self.machine_label(key.machine(), cx).is_some(),
        }
    }

    /// The provider an alert's Thread runs on, as far as its machine says.
    fn alert_provider(&self, alert: &crate::notifications::Alert, cx: &gpui::App) -> Option<Provider> {
        self.cockpit
            .read(cx)
            .thread(alert.thread)
            .map(|open| open.provider())
    }

    /// One alert's system notification, worded as its row and tagged with
    /// its Thread on its machine, so a newer one replaces it, while its
    /// record is there to word it.
    fn system_posting(
        &self,
        alert: &crate::notifications::Alert,
        now: std::time::SystemTime,
        cx: &gpui::App,
    ) -> Option<crate::system_notifications::Posting> {
        let row = self.alert_row(alert, now, cx)?;
        let cockpit = self.cockpit.read(cx);
        let machine = alert.thread.machine();
        let saved_as = cockpit.saved_as(machine)?.clone();
        // The directory is this computer's only for a Thread on it.
        let cwd = cockpit
            .local(machine)
            .then(|| cockpit.thread(alert.thread))
            .flatten()
            .and_then(|open| open.workspace())
            .map(|binding| binding.cwd().to_path_buf());
        Some(crate::system_notifications::Posting {
            tag: system_tag(&saved_as, alert.thread.thread()),
            needs_you: alert.needs_you(),
            word: row.word(),
            title: row.title.to_string(),
            detail: row.detail.text(),
            agent: self
                .alert_provider(alert, cx)
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
    /// (the click handler's doing) and lands on what it was about, in
    /// place, on whichever machine.
    pub(crate) fn open_system_notification(&mut self, tag: &str, cx: &mut Context<Self>) {
        if self.open_task_notification(tag, cx) || self.open_copy_notification(tag, cx) {
            return;
        }
        // Its entry stays: read now, the next pump takes it out of the
        // notification centre with the rest.
        let posted = self.toasts.read(cx).needs_you.posted.get(tag).cloned();
        if let Some(alert) = posted {
            self.notice_verb(alert.open_verb(), cx);
        }
    }

    /// The row a toast or a system notification shows for `alert`, while
    /// its record is there to word it.
    fn alert_row(
        &self,
        alert: &crate::notifications::Alert,
        now: std::time::SystemTime,
        cx: &gpui::App,
    ) -> Option<NoticeRow> {
        let notifications = self.cockpit.read(cx).notifications(alert.thread.machine());
        Some(match &alert.target {
            RowTarget::Thread(_) => return None,
            RowTarget::Notice(id) => {
                self.notice_row(alert.thread, notifications.get(*id)?, now, cx)
            }
            RowTarget::Decision(id) => {
                self.decision_row(alert.thread, notifications.decision(id)?, now, cx)
            }
        })
    }

    /// A live request in the list's words (R7, `request_detail`); its age
    /// from when it was raised.
    fn decision_row(
        &self,
        key: ThreadKey,
        notice: &ferrite_core::notifications::DecisionNotice,
        now: std::time::SystemTime,
        cx: &gpui::App,
    ) -> NoticeRow {
        use crate::notifications::{Detail, State};
        let open = self.cockpit.read(cx).thread(key);
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
            target: crate::notifications::RowTarget::Decision(notice.id.clone()),
            thread: key,
            title: self.row_title(key, cx),
            state: State::NeedsYou(notice.kind),
            detail,
            when: crate::facts::since_label(notice.at, now),
            read: notice.read,
            elsewhere: self.machine_label(key.machine(), cx).is_some(),
        }
    }

    /// One row per open Thread awaiting handling, independent of Notices.
    /// Requests precede other rows, retaining each machine's attention order.
    #[cfg(test)]
    pub(super) fn needs_you_rows(&self, cx: &gpui::App) -> Vec<NoticeRow> {
        self.inbox_rows(&self.nav_state(cx), cx)
    }

    /// The Active section as the Inbox's rows, every machine's, requests
    /// first.
    pub(super) fn inbox_rows(
        &self,
        state: &nav::NavState,
        cx: &gpui::App,
    ) -> Vec<NoticeRow> {
        let now = ferrite_core::clock::system_time();
        let mut rows: Vec<_> = state
            .active
            .iter()
            .map(|entry| entry.key)
            .map(|thread| {
                let notifications = self.cockpit.read(cx).notifications(thread.machine());
                if let Some(decision) = notifications.first_request(thread.thread()) {
                    self.decision_row(thread, decision, now, cx)
                } else if let Some(notice) = notifications
                    .notices()
                    .find(|notice| notice.thread == thread.thread())
                {
                    self.notice_row(thread, notice, now, cx)
                } else {
                    NoticeRow {
                        target: RowTarget::Thread(thread.thread()),
                        thread,
                        title: self.row_title(thread, cx),
                        state: crate::notifications::State::Idle,
                        detail: crate::notifications::Detail::None,
                        when: "".into(),
                        read: true,
                        elsewhere: self.machine_label(thread.machine(), cx).is_some(),
                    }
                }
            })
            .collect();
        rows.sort_by_key(|row| !row.request());
        let mut seen = std::collections::BTreeSet::new();
        rows.retain(|row| seen.insert(row.thread));
        rows
    }

    /// The nav column as far in as it is this frame, its face pinned to the
    /// board's edge and clipped as it slides, under a veil that takes the
    /// pointer while it slides out.
    fn nav(&self, cx: &gpui::App) -> Option<AnyElement> {
        let drawn = self.nav_width();
        if drawn <= 0.0 {
            return None;
        }
        let width = self.nav_expanded_width();
        let face = nav::column(self.board_ground(cx))
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

    fn board_ground(&self, cx: &gpui::App) -> ferrite_ui::theme::Paint {
        let lone = !self.destination_shown() && !self.empty_board_shown(cx);
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
/// reader slot is its Thread's number under the `READER_LEAF` bit. A board
/// holds one Pane and its reader, so a leaf maps back to its slot through
/// the board (`leaf_slot`).
type Leaf = ThreadId;

const READER_LEAF: u64 = 1 << 62;

/// A Review's share of its Pane's slot when it first opens beside it.
const READER_SHARE: f32 = ferrite_ui::theme::READER_SHARE;

fn pane_leaf(identity: PaneIdentity) -> Leaf {
    match identity {
        PaneIdentity::Thread(thread) => thread.thread(),
        PaneIdentity::Draft(draft) => ThreadId::new(u64::MAX - draft.get()),
    }
}

fn reader_leaf(owner: ThreadKey) -> Leaf {
    ThreadId::new(READER_LEAF | owner.thread().get())
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
fn head_drag(leaf: Leaf, face: GhostFace) -> impl FnOnce(Div) -> AnyElement {
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
#[cfg(any(test, feature = "demo"))]
pub(crate) fn here() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| ".".into())
}

/// Whether a PR read of the checkout at `path` goes to its forge. Tests
/// and captures never run `gh`: only a fixture world's forge answers them
/// (`Cockpit::fixture_forge`).
#[cfg(any(test, feature = "visual-reference"))]
pub(crate) fn reads_forge(cockpit: &Cockpit, path: &std::path::Path) -> bool {
    cockpit.has_forge_fixture(path)
}

#[cfg(not(any(test, feature = "visual-reference")))]
pub(crate) fn reads_forge(_: &Cockpit, _: &std::path::Path) -> bool {
    true
}

/// The Project a launch's draft starts on: where the operator left off.
/// The most recently used Thread's Project is the honest answer — it is
/// the work they were actually doing — and the newest registered Project
/// is the fallback for a store whose Threads name nothing the registry
/// still knows.
fn startup_project(cockpit: &ferrite_client::cockpit::Cockpit) -> Option<ProjectKey> {
    let machine = cockpit.start_machine()?;
    cockpit.last_worked_project(machine).or_else(|| {
        let registry = cockpit.registry(machine);
        registry
            .projects()
            .last()
            .map(|project| registry.project_key(project.id))
    })
}

/// This process's resident memory, for the perf print.
fn rss_mb() -> f64 {
    ferrite_core::memory::rss_bytes(std::process::id())
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
