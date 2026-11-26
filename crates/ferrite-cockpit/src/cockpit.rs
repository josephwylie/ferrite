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
mod board_layouts;
mod branch_reads;
mod browser;
#[cfg(feature = "browser-smoke")]
mod browser_smoke;
mod checkouts;
mod cockpit_pane;
mod columns;
mod composer_menu;
mod context_menu;
mod copies;
pub(crate) mod coordinator;
pub(crate) mod decisions;
mod delegates;
mod destination;
mod details;
mod direct;
pub(crate) mod direction;
mod draft_commands;
mod draft_environment;
mod drafts;
#[cfg(test)]
pub(crate) mod drawn;
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
mod nav_keys;
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
mod new_task;
mod project_editor_view;
mod settings_view;
mod setup_sync;
mod sheets;
mod side_chat_view;
mod slot_drag;
mod rename;
mod returns;
use ferrite_ui::components::rename_field;
use rename::RenameTarget;
pub(crate) mod starts;
mod surfaces;
mod sweep;
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
use self::decisions::Answer;
use self::draft_environment::Band;
use self::popups::{CardFrom, Popup};
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

use ferrite_client::cockpit::{Cockpit, ProviderChoice, ThreadView};
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
use ferrite_nav as nav;
use ferrite_nav::notifications::{Row as NoticeRow, RowTarget, Verb};
use ferrite_pane::composer::{Composer, Edited, OpenSource};
use ferrite_pane::link_target::{LinkTarget, Open};
use ferrite_pane::decision;
use ferrite_pane::cell::footer::{effort_chip_label, effort_in_force, tuning_hold, turn_running};
#[cfg(test)]
use ferrite_pane::cell::footer::{plugins, TUNING_BUSY_HINT, TUNING_PENDING_HINT};
use ferrite_pane::cell::{band_trigger, PaneDrag, HERO_TRIGGER};
use ferrite_pane::pane::{self, PaneView};
use ferrite_ui::menu;
use ferrite_pane::select::TranscriptText;
use ferrite_ui::frame::settings;
use ferrite_ui::pointer::{Pointer, PointerPressed};
use ferrite_ui::provider::effort_title;
pub use history::{GoBack, GoForward};
// A Decision card's note answers these, so they are the Pane's.
pub use ferrite_pane::composer::{Interrupt, Submit};
pub use ferrite_pane::decision::{PickOption1, PickOption2, PickOption3, PickOption4};
pub use tools::{GoToFile, ToggleChanges, ToggleFileTree, ToggleFiles, ToggleTerminal};

actions!(
    cockpit,
    [
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

pub(crate) use cockpit_pane::CockpitPane;

pub struct CockpitView {
    /// The client's model. On a beat that brought something the pump has
    /// it say `Arrived`, which the views that build from it hear (the
    /// nav). Nothing notifies it.
    pub(crate) cockpit: Entity<Cockpit>,
    /// Every Thread's Browser: its pages, the provider's control of them,
    /// and the views of its chrome, its page's dialog and an annotation's
    /// review.
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
    field_watch: fields::FieldWatch,
    perf: Option<Perf>,
    /// When the watchdog's sweep is next due, what it finds turned over,
    /// and free disk as it last read it (`sweep`).
    sweep: sweep::Sweep,
    /// The nav and each Pane as cached parts (`parts`), and what notifies
    /// the frame around them alone.
    parts: std::cell::RefCell<parts::Parts>,
    /// When the Threads' checkout labels are read from git
    /// (`refresh_branches`).
    branch_reads: branch_reads::BranchReads,
    /// The machines whose Threads' branches are being read there.
    /// What a draft's workspace chip and its menu read of its Project's
    /// repo, as git or the machine's Server last answered.
    checkouts: checkouts::Checkouts,
    /// Stable text-run identities; selection itself belongs to GPUI.
    selection: TranscriptText,
    /// Every Thread's side chat, a body of the Panel.
    side_chat: Entity<ferrite_prs::side_chat::SideChatView>,
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
    /// The tools behind the Panel's Files, Terminal and Changes, and the
    /// Threads whose agent worked since they last looked.
    tools: tools::Tools,
    /// The pull request reader: the Panel's PR Surface and the PRs' detail.
    pull_request: Entity<ferrite_prs::pull_request::PullRequestView>,
    /// The Inbox's alerts and the toast float that shows them.
    pub(crate) toasts: Entity<ferrite_nav::toasts::ToastsView>,
    native_copy: Option<String>,
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
    nav_view: Entity<ferrite_nav::view::NavView>,
    /// The Cockpit hearing its window, from its first frame: the window's
    /// activation, and each view that has an event enum of its own (ADR
    /// 0041) (`hear_window`).
    window_heard: Option<Vec<gpui::Subscription>>,
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
    /// plan, checks or session controls card, and how long a card has
    /// waited for its geometry (ADR 0043).
    popups: popups::Popups,
    /// The Solo view's right panel (`panel`): its tab per Thread, whether
    /// the window has room for it, its focus, and the views of its Overview
    /// and File Surface.
    panel: panel::Panel,
    /// Back and forward through where the operator has been (`history`).
    history: history::History,
    /// Holds every footer's plan-usage popover open, its ring's own clicks
    /// aside: a visual-reference scene's (`usagepopover`), never the app's.
    usage_popover_held: bool,
    /// Why the last Session control was refused (`session_control`).
    session_control_error: session_control::SessionControlError,
    /// The board's bounds as the last frame laid it out: what an act (a
    /// seam press, a drop) measures in when it has no window to measure.
    board: std::cell::Cell<layout::Rect>,
    /// A board slot or native files being dragged: the lifted slot, the
    /// wash a release would fill, the slots' sizes for its ghost, the Pane
    /// the files hover (`slot_drag`).
    slot_drag: slot_drag::SlotDrag,
    /// Each board's arrangement as the operator left it, and the seam
    /// being dragged (`board_layouts`).
    board_layouts: board_layouts::BoardLayouts,
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
    /// Where each provider CLI stands against its newest release.
    cli_updates: crate::cli_updates::CliUpdates,
    /// The machine Settings edits: what its Server has been asked and
    /// said, its CLIs' versions among it.
    environment: environment::Environment,
    /// Where Ferrite's own update stands (ADR 0023).
    ferrite_update: crate::ferrite_update::FerriteUpdate,
    /// What the window has said about its Server (ADR 0025).
    server_bands: server::ServerBands,
    /// The operator's setup kept in sync with each Machine (ADR 0048).
    setup_sync: Entity<setup_sync::SetupSync>,
    /// Every copy of a Project of this computer to a Machine (ADR 0048 §2).
    copies: Entity<copies::Copies>,
    /// Whether the window has said `settings.json` could not be used.
    unreadable_told: bool,
    /// The operator's commands running, and what they and the keys have
    /// still to say.
    user_commands: user_commands::UserCommands,
    /// Whether "Quit Ferrite?" is on screen (`leaving`).
    leave_asked: bool,
    /// What the last nav act has to say (a failed Park, a refused
    /// rename).
    nav_notice: nav_view::Notice,
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
    /// What stands in the board's place: the Factory or the PRs.
    destination: Option<destination::Destination>,
    /// The PRs destination, kept as it was left while another view shows.
    prs: Entity<ferrite_prs::prs::PrsView>,
    /// The Tasks whose needs-you toast stands, and the state it announced.
    task_toasts: std::collections::HashMap<(MachineId, String), ferrite_core::factory_board::Status>,
    /// The empty board: its selected row and the list's own focus.
    empty_board: Entity<ferrite_nav::empty_board::EmptyBoardView>,
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

/// About's Version: the package version, and ` · dev` on a development
/// build, so a release is never labelled.
fn version_label(version: &str, dev: bool) -> String {
    if dev {
        format!("{version} · dev")
    } else {
        version.to_string()
    }
}

/// Allow only pixel rounding at the bottom of a native scroll container.
#[cfg(test)]
const TAIL_SLACK: Pixels = px(2.);

#[cfg(test)]
thread_local! {
    /// Root Cockpit renders on this (test) thread: the unit the idle frame
    /// budget is measured in.
    pub(crate) static RENDERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
        let panel = panel::Panel::load(&prefs.dir, cx);
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
        decision::init(cx);
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
            view.tools.files.update(cx, |files, cx| files.flush_for_quit(cx));
            view.flush_panel_persistence(cx);
            view.cockpit.update(cx, |cockpit, _| cockpit.halt_sessions());
            #[cfg(feature = "cef")]
            let browser = view.browsers.controls.controller.clone();
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
        let cockpit = cx.new(|_| cockpit);
        let setup_sync = cx.new(|_| {
            setup_sync::SetupSync::new(prefs.setup_home.clone(), prefs.dir.clone(), cockpit.clone())
        });
        let copies = Self::copies_of_window(cockpit.clone(), cx);
        let composer_menu = cx.new(|_| composer_menu::ComposerMenuView::default());
        // Whether a Composer's key context is the menu's follows the menu.
        cx.observe(&composer_menu, |view, _, cx| view.sync_composer_floats(cx))
            .detach();
        let sheets = cx.new(|_| sheets::Sheets::default());
        cx.subscribe(&sheets, Self::on_sheets_event).detach();
        let empty_board = Self::new_empty_board(cx);
        let side_chat = Self::new_side_chat(cx);
        let files = Self::new_files(cx);
        let terminals = Self::new_terminals(cx);
        let changes = Self::new_changes(cx);
        // The root draws the diffs into its own frame (`changes_pane`), so
        // each change redraws the window.
        cx.observe(&changes, |_, _, cx| cx.notify()).detach();
        let pull_request = Self::new_pull_request(&changes, cx);
        let prs = Self::new_prs(&changes, &pull_request, cx);
        cx.subscribe(&prs, Self::on_prs_event).detach();
        cx.subscribe(&pull_request, Self::on_pr_event).detach();
        // What the reader shows is drawn by the root's frame too (a tab's
        // state, the PRs' header), so each change redraws the window.
        cx.observe(&pull_request, |_, _, cx| cx.notify()).detach();
        let background = cx.new(|_| background_work::BackgroundView::new());
        cx.subscribe(&background, Self::on_background_event).detach();
        // The root draws it into its own frame (the line, the Overview, the
        // Surface), so each change redraws the window.
        cx.observe(&background, |_, _, cx| cx.notify()).detach();
        // The root draws the Overview into its own frame, so each change
        // redraws the window.
        cx.observe(&panel.overview, |_, _, cx| cx.notify()).detach();
        // So it does the File Surface's card.
        cx.observe(&panel.file_surface, |_, _, cx| cx.notify()).detach();
        let toasts = Self::new_toasts(&cockpit, cx);
        let decisions = cx.new(|_| decisions::DecisionsView::default());
        // A card's pick lives in its Pane's forms, and the root draws the
        // cards into its transcripts, so each change redraws the window.
        cx.observe(&decisions, |_, _, cx| cx.notify()).detach();
        let browsers = browser::Browsers::new(cx);
        // The root draws the chrome into the Browser's body, so each change
        // redraws the window.
        cx.observe(&browsers.chrome, |_, _, cx| cx.notify()).detach();
        // So is a page's dialog, over the page.
        cx.observe(&browsers.dialog, |_, _, cx| cx.notify()).detach();
        // And a retained annotation's review, in the page's place.
        cx.observe(&browsers.review, |_, _, cx| cx.notify()).detach();
        let popups = popups::Popups::default();
        let nav_view = Self::new_nav(&cockpit, popups.triggers(), cx);
        let mut view = Self {
            cockpit: cockpit.clone(),
            browsers,
            columns: columns::Columns::new(prefs.settings.nav_collapsed),
            panes: Vec::new(),
            focus: cx.focus_handle(),
            field_watch: Default::default(),
            perf: std::env::var("FERRITE_PERF").is_ok().then(|| Perf {
                frames: 0,
                since: std::time::Instant::now(),
            }),
            sweep: sweep::Sweep::new(cx.background_executor().now()),
            parts: std::cell::RefCell::new(parts::Parts::new(cx)),
            branch_reads: Default::default(),
            selection: TranscriptText::default(),
            side_chat,
            held_sends: Default::default(),
            outbox: Default::default(),
            awaited: Default::default(),
            returning: Default::default(),
            tools: tools::Tools::new(files, terminals, changes),
            pull_request,
            toasts,
            native_copy: None,
            nav_filter: None,
            nav_view,
            window_heard: None,
            composer_menu,
            draft_commands: Default::default(),
            launch_project,
            environment_lookup: None,
            checkouts: Default::default(),
            rename: Default::default(),
            popups,
            panel,
            history: Default::default(),
            usage_popover_held: false,
            session_control_error: Default::default(),
            head_parent_room: Default::default(),
            facts: Facts::with_auto_title(prefs.settings.auto_title),
            board: std::cell::Cell::new(layout::Rect::default()),
            slot_drag: Default::default(),
            board_layouts: Default::default(),
            unsent: drafts::Unsent::load(&prefs.dir),
            cited_sources,
            prefs,
            sheets,
            background,
            cli_updates: Default::default(),
            environment: Default::default(),
            ferrite_update: Default::default(),
            server_bands: Default::default(),
            setup_sync,
            copies,
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
            destination: None,
            prs,
            task_toasts: Default::default(),
            empty_board,
            reader_slides: Default::default(),
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
            view.ask_draft_checkouts(cx);
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
        if view.sweep.ask_free_disk(cx.background_executor().now()) {
            view.read_free_disk(cx);
        }
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
        self.panel.file_surface
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
            pane.read(cx).preview.set_remote_files(self.elsewhere(thread.machine(), cx));
            self.aim_attachments(&pane.composer(cx), thread.machine(), cx);
            cx.subscribe(&pane.composer(cx), Self::composer_edited).detach();
            cx.subscribe(&pane.composer(cx), Self::composer_opens_source).detach();
            cx.subscribe(&pane.composer(cx), Self::composer_comment_opened).detach();
            self.restore_draft(thread, &pane.read(cx).composer.clone(), cx);
            self.panes.push(pane);
            self.thread_opened(thread, cx);
            opened = true;
        }
        self.panes
            .sort_by_key(|pane| wanted.iter().position(|shown| *shown == pane.identity));
        let retained_transcript_entities: std::collections::HashSet<_> = self
            .panes
            .iter()
            .flat_map(|pane| pane.read(cx).transcripts.values())
            .map(|transcript| transcript.entity_id())
            .collect();
        self.transcript_entities
            .retain(|id| retained_transcript_entities.contains(id));
        // A Thread that came or went moved between the grid and the nav's
        // parked rows.
        if opened || self.panes.len() != before {
            self.parked_changed(cx);
        }
        self.refresh_names(cx);
    }

    /// A Thread's Pane opened: its facts afresh, what only git can say
    /// read off the UI thread.
    fn thread_opened(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let lookups = self.facts.opened(self.cockpit.read(cx), thread);
        self.look_up(lookups, cx);
        self.check_pr(thread, false, cx);
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

    /// Every open Pane's name, from the cache — the head reads
    /// `PaneView::name`, and it moves only at a naming moment.
    fn refresh_names(&mut self, cx: &mut gpui::App) {
        for pane in &mut self.panes {
            if let Some(thread) = pane.thread() {
                let value = self.facts.name(thread);
                pane.update(cx, |pane, _| pane.name = value);
            }
        }
    }

    /// Where this Pane sits in the grid.
    fn index_of(&self, identity: PaneIdentity) -> Option<usize> {
        self.panes.iter().position(|pane| pane.identity == identity)
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
        self.tools.note_work(&frame);
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
        if self.sweep.take(now) {
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
            let aged = self.sweep.ages_turned(wall);
            swept_frame = self.sweep.frame_turned(wall);
            swept_change = aged;
            self.refresh_branches(cx);
            self.follow_workspace(cx);
            // Free disk rides the sweep, once a minute: no wake of its own.
            if self.sweep.ask_free_disk(now) {
                self.read_free_disk(cx);
            }
            // Message times turn over with the minute, on this one timer:
            // each transcript redraws then, and no row arms a wake.
            if self.sweep.minute_turned() {
                let transcripts: Vec<_> = self.panes.iter().filter_map(|pane| pane.read(cx).transcript()).collect();
                for transcript in transcripts {
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
                    self.panes[index].update(cx, |pane, _| pane.redirect_subject(from, to));
                }
                self.release_evicted_transcripts(index, &update.subjects, cx);
                let mut selected = self.panes[index].read(cx).selected.clone();
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
                self.panes[index].update(cx, |pane, cx| pane.select_subject(selected.clone(), generation, cx));
                let content_changed = update
                    .subjects
                    .iter()
                    .any(|change| change.subject == selected && change.content_changed)
                    || (self.panes[index].read(cx).is_main() && !update.dirty.is_empty());
                if update.activity_changed || content_changed || !update.evicted.is_empty() {
                    self.prune_tool_disclosures(update.thread, cx);
                }
                self.prune_request_forms(index, cx);
                let history_error = self
                    .cockpit
                    .read(cx)
                    .subject_history_error(update.thread, &self.panes[index].read(cx).selected)
                    .map(str::to_string);
                self.panes[index].update(cx, |pane, _| pane.history_error = history_error);
                let unretained = self
                    .cockpit
                    .read(cx)
                    .thread(update.thread)
                    .and_then(|thread| thread.activity().subject(&self.panes[index].read(cx).selected))
                    .is_some_and(|subject| !subject.retained());
                if unretained && self.panes[index].read(cx).history_error.is_none() {
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
        PUMP_FALLBACK.min(self.sweep.due_in(now))
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

    /// Ferrite's own words to one Pane: a Notice in a Thread's transcript,
    /// or the error line under a draft's band.
    fn say_to(&mut self, pane: PaneIdentity, message: String, cx: &mut gpui::App) {
        match pane {
            PaneIdentity::Thread(thread) => self.cockpit.update(cx, |cockpit, _| cockpit.say(thread, message)),
            PaneIdentity::Draft(_) => {
                if let Some(index) = self.index_of(pane) {
                    self.panes[index].update_draft(cx, |draft| draft.error = Some(message.into()));
                }
            }
        }
    }

    /// Focus by grid index — the pointer's and the tests' spelling of the
    /// core's one door.
    fn focus_pane(&mut self, index: usize, cx: &mut gpui::App) {
        if let Some(identity) = self.panes.get(index).map(|pane| pane.identity) {
            self.cockpit.update(cx, |cockpit, _| cockpit.focus(identity));
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
    fn pointer_down(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get_mut(index) {
            pane.update(cx, |pane, _| pane.clear_tool_target());
        }
        self.focus_pane(index, cx);
        cx.notify();
    }

}

impl CockpitView {
    /// What the Cockpit hears through its window, from its first frame:
    /// the window's activation, which the Cockpit follows
    /// (`Cockpit::set_window_active`) — a window that opens unfocused sees
    /// nothing until it is focused — and each view's events, since
    /// answering some needs the window.
    fn hear_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.window_heard.is_some() {
            return;
        }
        self.cockpit.update(cx, |cockpit, _| cockpit.set_window_active(window.is_window_active()));
        let activation = cx.observe_window_activation(window, |view, window, cx| {
            if view.cockpit.update(cx, |cockpit, _| cockpit.set_window_active(window.is_window_active())) {
                cx.notify();
            }
        });
        self.watch_lost_focus(window, cx);
        self.window_heard = Some(vec![
            activation,
            cx.subscribe_in(&self.nav_view, window, Self::on_nav_event),
            cx.subscribe_in(&self.side_chat, window, Self::on_side_chat_event),
            cx.subscribe_in(&self.tools.files, window, Self::on_files_event),
            cx.subscribe_in(&self.panel.overview, window, Self::on_overview_event),
            cx.subscribe_in(&self.toasts, window, Self::on_toast_event),
            cx.subscribe_in(&self.panel.file_surface, window, Self::on_file_surface_event),
            cx.subscribe_in(&self.decisions, window, Self::on_decision_event),
            cx.subscribe_in(&self.browsers.chrome, window, Self::on_browser_chrome_event),
            cx.subscribe_in(&self.browsers.dialog, window, Self::on_browser_dialog_event),
            cx.subscribe_in(&self.browsers.review, window, Self::on_browser_review_event),
            cx.subscribe_in(&self.empty_board, window, Self::on_empty_board_event),
            cx.subscribe_in(&self.composer_menu, window, Self::on_composer_menu_event),
        ]);
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
        // Seen is on screen while the window has focus, and the window
        // coming back reads what it shows, which every part then draws.
        self.hear_window(window, cx);
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
            .filter_map(|at| ferrite_ui::age::since_label_changes_in(at, wall))
            .min();
        let thread_ages = self.facts.next_age_change(wall);
        // The sweep redraws a window nothing else draws when an age turns:
        // a Thread's everywhere, a notice's in the frame.
        self.sweep
            .clock_text_turns(thread_ages.map(|next| wall + next), notices.map(|next| wall + next));
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
                rollover(open.subject_turn(&pane.read(cx).selected)),
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
        let _ = self.parts.borrow().frame_tick().read(cx);
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
        if self.columns.nav_folded() {
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
        // Read once a frame: the two halves of the titlebar have to agree,
        // and a maximized window has no top resize edge for the drag
        // regions to leave alone.
        let maximized = window.is_maximized();
        self.leave_destination_if_moved(cx);
        let drawn = self.transcripts_drawn(cx);
        self.cockpit.update(cx, |cockpit, _| cockpit.draws_transcripts(drawn));

        for index in 0..self.panes.len() {
            let target = self.panes[index].read(cx).targeted_tool().cloned();
            let target_is_rendered = target
                .as_ref()
                .is_none_or(|target| self.expandable_tools(index, cx).contains(target));
            if !target_is_rendered {
                self.panes[index].update(cx, |pane, _| pane.clear_tool_target());
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
                        (request.subject.as_ref() == Some(&pane.read(cx).selected)
                            || (request.subject.is_none() && pane.read(cx).is_main()))
                            && pane::question_of(&request.decision).is_some()
                    })
                });
            let row_limit = if answering_question {
                row_limit.min(2)
            } else {
                row_limit
            };
            pane.composer(cx)
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
                    pane.composer(cx).update(cx, |composer, cx| {
                        composer.set_commands(open.commands(), cx)
                    });
                }
            }
        });
        let history_available: Vec<bool> = (0..self.panes.len())
            .map(|index| self.history_available(index, cx))
            .collect();
        for (pane, available) in self.panes.iter().zip(history_available) {
            pane.composer(cx).update(cx, |composer, cx| {
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
            .pane_machine(self.focused(cx), cx)
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
                self.panes[index].read(cx).preview.held_focus(window, cx)
            })
            .or_else(|| {
                let index = self.focused(cx);
                if !self.preview_shown(index, cx) { return None; }
                let pane = self.panes.get(index)?;
                if !pane.read(cx).preview.document().is_some_and(|document| matches!(&document.body, ferrite_pane::preview::Body::Browser)) { return None; }
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
                Some(if pane.read(cx).has_tool_target() {
                    pane.read(cx).tool_focus()
                } else if !pane.read(cx).is_main() {
                    // A subagent's tab keeps the keyboard on its body.
                    pane.read(cx).transcript_focus.clone()
                } else {
                    pane.read(cx).composer.read(cx).focus_target(window, cx)
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
            self.prs.read(cx).focus().clone()
        } else if !self.rename.is_open() && self.empty_board_shown(cx) {
            self.empty_board.read(cx).focus().clone()
        } else {
            wanted
        };
        use gpui::component::WindowExt as _;
        let native_text_focused = self
            .panes
            .get(self.focused(cx))
            .is_some_and(|pane| pane.read(cx).transcript_focus.contains_focused(window, cx))
            && (ferrite_pane::rich::controls_focused(window)
                || gpui::base::TextSelection::has_selection(window, cx)
                || self.panes.get(self.focused(cx)).is_some_and(|pane| {
                    pane.read(cx).rich.output_focused(&pane.read(cx).text_namespace(), window, cx)
                }));
        let pane_control_focused = !lost
            && self.panes.get(self.focused(cx)).is_some_and(|pane| {
                let controls = pane
                    .thread()
                    .and_then(|id| self.cockpit.read(cx).thread(id))
                    .is_some_and(|thread| {
                        !pane.read(cx).is_main()
                            || thread.pending_decisions().len() > 1
                            || thread.pending_decisions().iter().any(|request| {
                                request.subject.is_none() || asks_in_fields(&request.decision)
                            })
                    });
                controls
                    && pane.read(cx).controls_focus.contains_focused(window, cx)
                    && !pane.read(cx).controls_focus.is_focused(window)
                    && (!pane.read(cx).is_main() || !pane.read(cx).transcript_focus.is_focused(window))
            });
        // The focused Pane's preview and Review sit outside its transcript,
        // so their text needs its own exemption or a click to select in it
        // is undone. The focused Pane's only: focus moving to another Pane
        // takes the keyboard with it, and the image viewer's own focus is
        // no text (`Preview::text_focused`).
        let reader_text_focused = self.panes.get(self.focused(cx)).is_some_and(|pane| {
            (self.preview_shown(self.focused(cx), cx) && pane.read(cx).preview.text_focused(window, cx))
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
            self.panel.keys.contains_focused(window, cx) || self.panel_control_focused(window, cx);
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
            && !cfg!(test)
            && self.environment.board_asks()
        {
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
            self.slot_drag.laid_out([]);
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
                        let composer = pane.read(cx).composer.clone();
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
                        if view.panes.get(index).is_some_and(|pane| pane.read(cx).is_main()) {
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
                view.panel.file_surface.update(cx, |files, _| files.forget_press());
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
                    if view.slot_drag.release() {
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
                if view.board_layouts.seam_held() {
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
            .child(crate::frame().titlebar(!self.overlay_open(cx), maximized))
            .child(self.view_head(cx))
            .children(self.history_arrows_element(cx))
            .when(!self.columns.nav_folded(), |root| root.child(self.nav_resize_rail(cx)))
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
            .children(self.kit_toasts(window, cx))
    }
}

impl CockpitView {


    fn board_ground(&self, cx: &gpui::App) -> ferrite_ui::theme::Paint {
        let lone = !self.destination_shown() && !self.empty_board_shown(cx);
        ferrite_ui::theme::Paint::solid(ferrite_ui::theme::tx_ground(lone))
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
