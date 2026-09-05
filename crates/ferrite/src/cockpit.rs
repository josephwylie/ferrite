//! The cockpit window: every open Pane at once, and the one pump behind them.
//!
//! Rendering and keys only. What each Pane shows — the Blocks, the pending
//! Decision, the held prompt — is folded in core and read from there.

use std::time::Duration;

use ferrite_core::cockpit::{CloseError, Cockpit, HistoryDirection, ProviderChoice};
use ferrite_core::docview::{Cell, Level};
use ferrite_core::draft::DraftTarget;
use ferrite_core::groups::{Drag, DropTarget, GroupChange, GroupId, Groups, Plan};
use ferrite_core::layout::{self, Edge, SeamId, Tree, Zone};
use ferrite_core::roster::{PaneIdentity, View};
use ferrite_core::store::Provider;
use ferrite_core::workspace::registry::ProjectId;
#[cfg(test)]
use ferrite_core::workspace::WorkspaceChoice;
use ferrite_core::{DecisionAnswer, ThreadId};
use gpui::prelude::*;
use gpui::{
    actions, anchored, deferred, div, ease_out_quint, px, rgb, rgba, Animation, AnimationExt,
    AnyElement, ClickEvent, ClipboardItem, Context, Div, Entity, FocusHandle, Focusable,
    FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ScrollHandle, SharedString, Stateful, Window,
};

use crate::composer::{Composer, Edited};
use crate::facts::Facts;
use crate::menu;
use crate::nav;
use crate::pane::{self, PaneView};
use crate::pointer::{Pointer, PointerPressed};
use crate::prefs;
use crate::select::TranscriptText;

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
        MenuNext,
        MenuPrevious,
        MenuPick,
        MenuDismiss,
        HistoryOlder,
        HistoryNewer,
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

const PUMP_MS: u64 = 8;
const NAV_OPEN_MS: u64 = 260;
const NAV_CLOSE_MS: u64 = 190;

pub struct CockpitView {
    cockpit: Cockpit,
    /// One view per Pane of the Cockpit's roster, in its order — the
    /// keyboard, the scrollback and the caches the window owns for each.
    /// `sync_panes` is the one door: after every act on the roster the
    /// mirror is reconciled, never edited by hand.
    panes: Vec<PaneView>,
    /// The cockpit's own place in the focus tree. Key dispatch walks from the
    /// focused node up to the root, so with nothing focused inside the window
    /// the cockpit's own actions are never reached — at wall range, where no
    /// Pane holds a Composer, this handle is what keeps the keyboard alive.
    focus: FocusHandle,
    perf: Option<Perf>,
    /// When the watchdog last swept. Measurements are cached from the RSS
    /// worker, so a sweep never waits for an operating-system query.
    swept: std::time::Instant,
    /// One checkout-label refresh at a time, always off the UI thread.
    branch_refreshing: bool,
    /// Stable text-run identities; selection itself belongs to GPUI.
    selection: TranscriptText,
    native_copy: Option<String>,
    /// cmd-b (#21): the nav folded to its 40px LED rail. In memory only —
    /// a preference store is not this ticket.
    nav_collapsed: bool,
    /// False on launch so a restored preference never performs entrance
    /// choreography. Once the operator acts, the shell may animate between
    /// its two widths; the state itself remains immediately authoritative.
    nav_has_toggled: bool,
    /// What the nav and the Pane head say about a Thread beyond an O(1)
    /// read — checkout, Project, a parked row's provider, the L3 card —
    /// refreshed by moment, never per frame.
    facts: Facts,
    /// The one Project filter navigation has (#29). `None` is
    /// `All Projects`. It filters the **navigation only** — which Panes the
    /// Cockpit flies is never touched by it.
    nav_filter: Option<ProjectId>,
    /// Whether the filter's menu is down. Any press the menu did not
    /// swallow closes it, like every other popover here.
    nav_filter_open: bool,
    /// The nav tree's scroll, shared with the hand-drawn scrollbar beside
    /// it — gpui 0.2.2 paints none of its own.
    nav_scroll: ScrollHandle,
    /// The one popover in the Composer's slot, or None: the `/`/`@` menu
    /// (#23), a picker (#11, #25) or a band chip (#29). Always on the
    /// focused Pane's Composer; render self-heals it shut when the operator
    /// leaves that Pane, zooms below L1, or its offer expires.
    popover: Option<Popover>,
    /// Escape (or a press elsewhere) dismissed the menu: stay shut until
    /// the text moves again, or `sync_menu` would reopen it on the very
    /// text the operator dismissed it over.
    menu_muted: bool,
    /// A recalled slash/mention prompt is a programmatic edit: consume its
    /// `Edited` event without deriving a menu. The next operator edit clears
    /// the ordinary `menu_muted` latch and derives again.
    suppress_recall_menu_once: bool,
    /// Where the vendors keep session files — discovery's roots, defaulted
    /// to the real homes and aimed at scratch directories by tests. Read
    /// once per picker open, never per frame.
    session_file_roots: Vec<(Provider, std::path::PathBuf)>,
    /// The launch directory's registered project (#29) — every draft's
    /// starting choice.
    launch_project: ProjectId,
    /// The inline title rename in flight, if any: what is being renamed,
    /// and the one-line Composer standing in for its title. At most one —
    /// the title cell *is* the editor, so two at once would need two rows
    /// to be the row you are looking at.
    rename: Option<(RenameTarget, Entity<crate::composer::Composer>)>,
    /// The right-click menu, if one is up: what it is about, where it was
    /// summoned, and which destructive row is armed for its second press.
    context_menu: Option<ContextMenu>,
    /// The usage meter's detail card, tied to its Thread and click position.
    context_usage: Option<(ThreadId, gpui::Point<gpui::Pixels>)>,
    /// A seam being dragged: the Group, the seam, and the tree as it
    /// stands mid-drag — persisted on release, never per move.
    seam_drag: Option<SeamDrag>,
    /// A Pane being dragged over another: the target and what a release
    /// there would do, for the preview wash.
    drop_preview: Option<(ThreadId, Zone)>,
    /// The operator's settings and where they save; every change saves.
    prefs: Preferences,
    /// The Settings panel is up.
    settings_open: bool,
    settings_focus: FocusHandle,
    /// Whether the window is maximized, read once per frame in `render`.
    /// The nav's band is drawn without a `Window` in hand, and the two
    /// halves of the titlebar have to agree: a maximized window has no top
    /// resize edge for the drag regions to leave alone (`titlebar.rs`).
    maximized: bool,
    /// The CLIs' versions as `--version` reports them, probed once when the
    /// panel first opens: (claude, codex).
    cli_versions: Option<(SharedString, SharedString)>,
    group_error: Option<SharedString>,
    /// The operator's answers-in-progress to each Thread's question
    /// Decision (Claude's `AskUserQuestion`), keyed to the Decision they
    /// answer so a new question starts clean and a stale draft never
    /// answers the wrong one.
    questions: std::collections::HashMap<ThreadId, QuestionDraft>,
}

/// Where the operator has got to answering one question Decision.
struct QuestionDraft {
    /// The Decision's own id — the draft dies with it.
    decision: String,
    questions: Vec<ferrite_core::questions::Question>,
    answers: Vec<ferrite_core::questions::Answer>,
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
}

/// What the window is handed at launch beyond the core: the operator's
/// settings, where they are saved, and the Session defaults the spawner
/// reads — shared, so a change here applies to the next Session.
pub struct Preferences {
    pub settings: ferrite_core::settings::Settings,
    pub dir: std::path::PathBuf,
    pub defaults: std::sync::Arc<std::sync::Mutex<crate::session::SessionDefaults>>,
    /// Whether a model writes Thread titles (each Provider's own CLI, the
    /// cheap model, one turn). False in the test and demo constructors,
    /// so no suite ever spawns a real CLI for a name.
    pub titler: bool,
}

impl Preferences {
    /// Defaults saved nowhere — the test constructor's and the demo's.
    fn ephemeral() -> Self {
        Self {
            settings: ferrite_core::settings::Settings::default(),
            dir: std::env::temp_dir().join(format!("ferrite-prefs-{}", std::process::id())),
            defaults: std::sync::Arc::new(std::sync::Mutex::new(
                crate::session::SessionDefaults::default(),
            )),
            titler: false,
        }
    }
}

/// A seam mid-drag.
struct SeamDrag {
    group: GroupId,
    seam: SeamId,
    tree: Tree,
}

/// A Pane on the move: its title is the handle. Dropped on another Pane
/// of the same Group it swaps with it (the centre) or splits its slot
/// (an edge).
#[derive(Clone, Copy, Debug)]
struct PaneDrag {
    thread: ThreadId,
}

/// The badge that follows the pointer while a Pane is dragged.
struct PaneDragPreview(SharedString);

impl Render for PaneDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        nav::drag_badge(self.0.clone())
    }
}

/// How wide the grab band over a seam is, centred on the 8px gap.
const SEAM_GRAB: f32 = 10.0;

/// Where a folder the picker returns should land.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BrowseThen {
    /// The focused draft's Project chip.
    Draft,
    /// The nav's Project filter.
    Filter,
}

/// What a right-click was on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MenuTarget {
    /// A nav row.
    Thread(ThreadId),
    /// A Pane — the same Thread, but the rename opens in the head.
    Pane(ThreadId),
    Group(GroupId),
    Project(ProjectId),
}

/// One thing a context-menu row does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MenuVerb {
    Rename,
    Focus,
    Fullscreen,
    Close,
    /// Drop the Session of a Thread that is open but not on screen (Solo
    /// view shows one Pane; the rest still run). `Close` is the on-screen
    /// Pane's own door, with its Group semantics.
    Park,
    /// The transcript menu's own verbs: what is highlighted, or all of it.
    CopySelection,
    CopyTranscript,
    LeaveGroup,
    EnterGroup,
    DissolveGroup,
    NewThread,
    Reveal,
    CopyPath,
    Delete,
    RemoveProject,
}

/// The context menu up on screen.
struct ContextMenu {
    target: MenuTarget,
    at: Point<Pixels>,
    rows: Vec<Option<(menu::Item, MenuVerb)>>,
    /// The destructive row pressed once, waiting for its confirmation.
    armed: Option<usize>,
}

struct NavDragPreview(SharedString);

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
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        nav::drag_badge(self.0.clone())
    }
}

/// What a row says while a drag hovers it: the wash of the answer core
/// already knows. Soft draws the ring version of this as a 1px inset, which
/// a row with no border cannot carry without moving 2px mid-drag, so the
/// wash alone speaks — `--drop-valid` or `--drop-refused`, never both.
fn drop_feedback<E: gpui::InteractiveElement>(element: E, groups: Groups, target: DropTarget) -> E {
    element.drag_over::<NavDrag>(move |style, drag, _, _| {
        if matches!(groups.preview_drop(drag.drag, target), Plan::Refused(_)) {
            style.bg(rgba(crate::theme::BLOCKED_WASH))
        } else {
            style.bg(rgba(crate::theme::RUNNING_WASH))
        }
    })
}

/// The one popover the Composer's slot can hold — the `/` or `@` menu
/// (#23), the import file-picker (#11), the provider picker (#25) or a
/// draft's band chip (#29) — and everything it shows. At most one for the
/// whole cockpit, always on the focused Pane's Composer: `pane` names that
/// Pane by the roster's own identity. Rows
/// are discovered when it opens and re-derived only by its kind's own
/// rule, never per frame; each row carries what picking it does, so the
/// two can never drift apart.
struct Popover {
    /// The Pane whose Composer holds it.
    pane: PaneIdentity,
    kind: Kind,
    rows: Vec<Row>,
    selected: usize,
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
    },
    /// #11: adopt a CLI session file into a still-blank Thread.
    ImportFile,
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

    /// The footer's key hints.
    fn hints(&self) -> &'static str {
        match self {
            Kind::Commands => "↑↓ select · ↵ run · esc dismiss",
            Kind::Files { .. } => "↑↓ select · ↵ insert · esc dismiss",
            Kind::ImportFile => "↑↓ select · ↵ adopt · esc dismiss",
            Kind::Band(pane::BandChip::Project) => {
                "type path <dir> · ↑↓ move · ↵ pick · esc dismiss"
            }
            Kind::Provider | Kind::Effort | Kind::Band(_) => "↑↓ move · ↵ pick · esc dismiss",
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

/// What picking a row does. A command or a file lands in the line; every
/// other pick is Ferrite's own act, never a prompt.
#[derive(PartialEq)]
enum Consequence {
    /// Replace the whole `/filter` with `/name ` — sent later as plain text
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
    /// The local `import` row (#11): open the file picker — clearing the
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
    Adopt(std::path::PathBuf),
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
    /// Open the platform's folder picker; the folder picked registers
    /// and becomes the draft's Project.
    Browse,
}

/// Allow only pixel rounding at the bottom of a native scroll container.
#[cfg(test)]
const TAIL_SLACK: Pixels = px(2.);

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
    #[cfg(test)]
    pub fn new(cockpit: Cockpit, cx: &mut Context<Self>) -> Self {
        Self::new_with_provider(cockpit, Provider::Claude, cx)
    }

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
        crate::theme::init_components(cx);
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(pump_interval()).await;
            if this.update(cx, |view, cx| view.pump(cx)).is_err() {
                break;
            }
        })
        .detach();

        let repo = here();

        // The registry's seed (#29): the directory Ferrite launched from is
        // always a project, and every draft's starting choice.
        let launch_project = cockpit
            .register_project(&repo)
            .expect("the launch directory registers as a project");
        let mut view = Self {
            cockpit,
            panes: Vec::new(),
            focus: cx.focus_handle(),
            perf: std::env::var("FERRITE_PERF").is_ok().then(|| Perf {
                frames: 0,
                since: std::time::Instant::now(),
            }),
            swept: std::time::Instant::now(),
            branch_refreshing: false,
            selection: TranscriptText::default(),
            native_copy: None,
            nav_filter: None,
            nav_filter_open: false,
            nav_scroll: ScrollHandle::new(),
            popover: None,
            menu_muted: false,
            suppress_recall_menu_once: false,
            session_file_roots: ferrite_core::import::default_roots(),
            launch_project,
            rename: None,
            context_menu: None,
            context_usage: None,
            nav_collapsed: prefs.settings.nav_collapsed,
            nav_has_toggled: false,
            facts: Facts::with_auto_title(prefs.settings.auto_title),
            seam_drag: None,
            drop_preview: None,
            prefs,
            settings_open: false,
            settings_focus: cx.focus_handle(),
            maximized: false,
            cli_versions: None,
            group_error: None,
            questions: std::collections::HashMap::new(),
        };
        // Every Thread the launch opened is on the roster already, and
        // every Thread it did not open is a parked row from the first
        // frame — a launch that opens nothing has no change to notice.
        view.sync_panes(cx);
        view.facts.parked_changed(&view.cockpit);
        // Nothing revived: the cockpit starts as one draft Pane (#29) —
        // nothing spawns before the operator's choice.
        if view.panes.is_empty() {
            view.open_draft_with_provider(DraftTarget::Main, launch_provider, cx);
        }
        view
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
            let pane = PaneView::new(thread, cx);
            cx.subscribe(&pane.composer, Self::composer_edited).detach();
            self.panes.push(pane);
            self.facts.opened(&self.cockpit, thread);
            opened = true;
        }
        self.panes
            .sort_by_key(|pane| wanted.iter().position(|shown| *shown == pane.identity));
        // A Thread that came or went moved between the grid and the nav's
        // parked rows.
        if opened || self.panes.len() != before {
            self.facts.parked_changed(&self.cockpit);
        }
        self.refresh_names();
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
            .and_then(PaneView::draft_mut)
        {
            draft.error = None;
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
        if self.suppress_recall_menu_once {
            self.suppress_recall_menu_once = false;
            self.close_text_menu();
            cx.notify();
            return;
        }
        self.menu_muted = false;
        self.sync_menu(cx);
        cx.notify();
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
        let frame = self.cockpit.pump();
        let models_changed = self.cockpit.take_models_changed();
        if models_changed {
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
        self.sync_question_drafts();
        let mut restarted = Vec::new();
        let mut branch_tick = false;
        if self.swept.elapsed() >= SWEEP_INTERVAL {
            self.swept = std::time::Instant::now();
            for restart in self.cockpit.sweep() {
                eprintln!(
                    "ferrite: restarted thread {} after {} bytes resident",
                    restart.thread, restart.rss
                );
                restarted.push(restart.thread);
            }
            self.facts.tick(&self.cockpit);
            self.refresh_branches(cx);
            branch_tick = true;
        }
        // A restart writes a Notice even when no Session streamed this frame —
        // and a failed respawn will never stream again, so this notify is that
        // notice's only ride to the screen.
        if frame.is_empty()
            && restarted.is_empty()
            && !branch_tick
            && !startup_changed
            && !models_changed
        {
            return;
        }
        for update in &frame {
            if let Some(pane) = self.pane_for(update.thread) {
                // New content follows the tail; colour arriving late does
                // not, and neither does an operator who scrolled back into
                // history — they reattach by scrolling to the bottom.
                if !update.dirty.is_empty() && self.panes[pane].follow_tail.get() {
                    self.panes[pane].scroll.scroll_to_bottom();
                }
            }
            // The facts refold only when the Thread actually changed.
            if !update.dirty.is_empty() || !update.evicted.is_empty() {
                self.prune_tool_disclosures(update.thread);
                self.facts.streamed(&self.cockpit, update.thread);
            }
        }
        for thread in restarted {
            self.facts.acted(&self.cockpit, thread);
        }
        cx.notify();
    }

    /// Refresh checkout labels without ever waiting for Git in GPUI's pump.
    fn refresh_branches(&mut self, cx: &mut Context<Self>) {
        if self.branch_refreshing {
            return;
        }
        let targets: Vec<_> = self
            .cockpit
            .threads()
            .into_iter()
            .filter_map(|thread| {
                let open = self.cockpit.thread(thread)?;
                let cwd = ferrite_core::workspace::effective_cwd(
                    open.session_project_root(),
                    open.workspace(),
                )?;
                Some((thread, cwd.to_path_buf()))
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        self.branch_refreshing = true;
        cx.spawn(async move |this, cx| {
            let branches = cx
                .background_executor()
                .spawn(async move {
                    targets
                        .into_iter()
                        .map(|(thread, cwd)| {
                            (thread, ferrite_core::workspace::checkout_branch(&cwd))
                        })
                        .collect()
                })
                .await;
            this.update(cx, |view, cx| {
                view.branch_refreshing = false;
                view.facts.set_branches(branches);
                cx.notify();
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
        let valid = self
            .cockpit
            .thread(thread)
            .map(|open| open.transcript())
            .into_iter()
            .flat_map(|transcript| transcript.blocks())
            .filter_map(|block| match &block.body {
                ferrite_core::transcript::Body::Tool(tool)
                    if pane::tool_has_details(tool) || tool.is_shell() =>
                {
                    Some(tool.call.clone())
                }
                _ => None,
            })
            .collect();
        self.panes[index].prune_tools(&valid);
    }

    /// One cell of the grid, as the window is right now. Size is the only
    /// input semantic zoom takes — there is no mode to switch, and the nav
    /// is simply part of the size: opening it can legitimately drop Panes a
    /// Level (#21).
    fn cell(&self, window: &Window) -> Cell {
        let viewport = window.viewport_size();
        let layout = self.cockpit.layout();
        // The nav, the grid's own padding, and the gaps between cells are
        // not the Pane's to render in. Above the board sits the titlebar
        // band (see `board_bounds`).
        let chrome = self.nav_width() + crate::theme::GRID_PAD * 2.0;
        let width =
            (f32::from(viewport.width) - chrome) / layout.columns as f32 - crate::theme::GRID_GAP;
        let height =
            (f32::from(viewport.height) - crate::theme::BOARD_TOP - crate::theme::GRID_PAD)
                / layout.rows as f32
                - crate::theme::GRID_GAP;
        Cell::new(width.max(0.0), height.max(0.0))
    }

    /// Whether something floats over the cockpit: a menu, a popover, the
    /// filter list or the Settings panel. The titlebar's drag region reads
    /// it — a region that lies under an open overlay would answer Windows'
    /// hit test as the window frame, and the row under the pointer would
    /// never see the press (`titlebar.rs`).
    fn overlay_open(&self) -> bool {
        self.settings_open
            || self.nav_filter_open
            || self.popover.is_some()
            || self.context_menu.is_some()
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
        if self.cockpit.roster().fullscreen().is_some() {
            return Level::Transcript;
        }
        self.level_of(self.focused(), window)
    }

    /// The board the Panes lay out in, in window coordinates: right of the
    /// nav, inset by the grid padding.
    /// The board starts under the titlebar band the nav also reserves,
    /// plus its own padding (`BOARD_TOP`): with a transparent macOS titlebar AppKit still
    /// drags the window from that strip, and a Pane head drawn inside it
    /// could not be dragged onto another Pane — the window moved instead.
    fn board_bounds(&self, window: &Window) -> layout::Rect {
        let viewport = window.viewport_size();
        let pad = crate::theme::GRID_PAD;
        let top = crate::theme::BOARD_TOP;
        layout::Rect {
            x: self.nav_width() + pad,
            y: top,
            w: (f32::from(viewport.width) - self.nav_width() - pad * 2.0).max(0.0),
            h: (f32::from(viewport.height) - top - pad).max(0.0),
        }
    }

    /// The Group's tree as it should draw right now: the one mid-drag,
    /// else the persisted one reconciled to the members — with a pending
    /// draft spliced in under a stand-in id, since a draft is no Thread.
    fn group_tree(&self, group: GroupId) -> Option<Tree> {
        let mut tree = match &self.seam_drag {
            Some(drag) if drag.group == group => drag.tree.clone(),
            _ => self.cockpit.group_layout(group)?,
        };
        for identity in self.cockpit.visible() {
            if let PaneIdentity::Draft(draft) = identity {
                tree.insert(draft_leaf(draft));
            }
        }
        Some(tree)
    }

    /// Every visible Pane's rect on the board, in window coordinates —
    /// from the Group's tree, or the one Solo cell. Fullscreen is the
    /// whole board.
    fn pane_rects(&self, window: &Window) -> Vec<(usize, layout::Rect)> {
        let bounds = self.board_bounds(window);
        if let Some(identity) = self.cockpit.roster().fullscreen() {
            return self
                .index_of(identity)
                .map(|index| vec![(index, bounds)])
                .unwrap_or_default();
        }
        match self.cockpit.roster().view() {
            View::Group(group) => {
                let Some(tree) = self.group_tree(group) else {
                    return Vec::new();
                };
                tree.rects(bounds, crate::theme::GRID_GAP)
                    .into_iter()
                    .filter_map(|(leaf, rect)| {
                        let identity = leaf_identity(leaf);
                        self.index_of(identity).map(|index| (index, rect))
                    })
                    .collect()
            }
            View::Solo => self
                .visible_indices()
                .into_iter()
                .map(|index| (index, bounds))
                .collect(),
        }
    }

    /// The level one Pane draws at: its own rect's, or the shared cell's
    /// where no tree governs.
    fn level_of(&self, index: usize, window: &Window) -> Level {
        if self.cockpit.roster().fullscreen().is_some() {
            return Level::Transcript;
        }
        self.pane_rects(window)
            .into_iter()
            .find(|(at, _)| *at == index)
            .map(|(_, rect)| Level::for_cell(Cell::new(rect.w, rect.h)))
            .unwrap_or_else(|| Level::for_cell(self.cell(window)))
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
            RenameTarget::Thread(thread) | RenameTarget::PaneTitle(thread) => {
                Some(self.cockpit.display_title(thread, true))
            }
        };
        let Some(title) = title else {
            return;
        };
        let editor = cx.new(crate::composer::Composer::new);
        editor.update(cx, |editor, cx| editor.set(title, cx));
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
            RenameTarget::Thread(thread) | RenameTarget::PaneTitle(thread) => self
                .cockpit
                .rename_thread(thread, &title)
                .map_err(|error| error.to_string()),
        };
        if let Err(error) = result {
            self.group_error = Some(error.into());
        } else {
            if let RenameTarget::Thread(thread) | RenameTarget::PaneTitle(thread) = target {
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
                return div()
                    .min_w_0()
                    .flex_1()
                    .child(editor.clone())
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
    /// was up (a popover, the filter, a rename) closes: one floating thing
    /// at a time.
    fn open_context_menu(&mut self, target: MenuTarget, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.popover = None;
        self.nav_filter_open = false;
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
        match target {
            // The nav row: where a Thread lives in the roster — open it,
            // park it, move it between Groups, delete it.
            MenuTarget::Thread(thread) => {
                let shown = self.pane_for(thread).is_some();
                let live = self.cockpit.thread(thread).is_some();
                let grouped = self.cockpit.groups().of(thread).is_some();
                rows.push(Some((
                    menu::Item::new("Rename").hint("⏎ save · esc cancel"),
                    MenuVerb::Rename,
                )));
                if self.cockpit.roster().focused_thread() != Some(thread) {
                    rows.push(Some((
                        menu::Item::new(if live { "Open" } else { "Resume" }),
                        MenuVerb::Focus,
                    )));
                }
                if shown {
                    rows.push(Some((
                        menu::Item::new("Toggle Fullscreen").hint("⌘F"),
                        MenuVerb::Fullscreen,
                    )));
                }
                rows.push(None);
                rows.push(Some((
                    menu::Item::new("New Thread in this Project").hint("⌘T"),
                    MenuVerb::NewThread,
                )));
                rows.push(Some((
                    menu::Item::new("Reveal in Finder"),
                    MenuVerb::Reveal,
                )));
                rows.push(Some((menu::Item::new("Copy Path"), MenuVerb::CopyPath)));
                rows.push(None);
                if live {
                    rows.push(Some((
                        menu::Item::new("Park Thread").hint(if shown { "⌘W" } else { "" }),
                        if shown && !grouped {
                            MenuVerb::Close
                        } else {
                            MenuVerb::Park
                        },
                    )));
                }
                if grouped {
                    rows.push(Some((menu::Item::new("Leave Group"), MenuVerb::LeaveGroup)));
                }
                rows.push(Some((
                    menu::Item::new("Delete Thread").destructive(),
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
                    menu::Item::new("Copy").hint("⌘C").disabled(!selected),
                    MenuVerb::CopySelection,
                )));
                rows.push(Some((
                    menu::Item::new("Copy Transcript"),
                    MenuVerb::CopyTranscript,
                )));
                rows.push(None);
                rows.push(Some((
                    menu::Item::new("Rename").hint("⏎ save · esc cancel"),
                    MenuVerb::Rename,
                )));
                rows.push(Some((
                    menu::Item::new("Toggle Fullscreen").hint("⌘F"),
                    MenuVerb::Fullscreen,
                )));
                rows.push(None);
                rows.push(Some((
                    menu::Item::new("Reveal in Finder"),
                    MenuVerb::Reveal,
                )));
                rows.push(Some((menu::Item::new("Copy Path"), MenuVerb::CopyPath)));
                rows.push(None);
                rows.push(Some((
                    menu::Item::new(if grouped { "Close Pane" } else { "Park Thread" }).hint("⌘W"),
                    MenuVerb::Close,
                )));
                if grouped {
                    rows.push(Some((menu::Item::new("Park Thread"), MenuVerb::Park)));
                    rows.push(Some((menu::Item::new("Leave Group"), MenuVerb::LeaveGroup)));
                }
            }
            MenuTarget::Group(group) => {
                rows.push(Some((menu::Item::new("Rename Group"), MenuVerb::Rename)));
                if self.cockpit.roster().view() != View::Group(group) {
                    rows.push(Some((menu::Item::new("Open Group"), MenuVerb::EnterGroup)));
                }
                rows.push(Some((
                    menu::Item::new("New Thread in this Group").hint("⌘T"),
                    MenuVerb::NewThread,
                )));
                rows.push(None);
                rows.push(Some((
                    menu::Item::new("Dissolve Group").destructive(),
                    MenuVerb::DissolveGroup,
                )));
            }
            MenuTarget::Project(project) => {
                rows.push(Some((
                    menu::Item::new("New Thread here").hint("⌘T"),
                    MenuVerb::NewThread,
                )));
                rows.push(Some((
                    menu::Item::new("Reveal in Finder"),
                    MenuVerb::Reveal,
                )));
                rows.push(Some((menu::Item::new("Copy Path"), MenuVerb::CopyPath)));
                rows.push(None);
                let in_use = self.project_in_use(project);
                rows.push(Some((
                    menu::Item::new(if in_use {
                        "Remove Project (has Threads)"
                    } else {
                        "Remove Project"
                    })
                    .destructive()
                    .disabled(in_use),
                    MenuVerb::RemoveProject,
                )));
            }
        }
        rows
    }

    /// Whether any Thread, open or parked, records this Project.
    fn project_in_use(&self, project: ProjectId) -> bool {
        self.cockpit
            .threads()
            .into_iter()
            .chain(self.facts.parked().iter().copied())
            .any(|thread| self.cockpit.project_id(thread) == Some(project))
    }

    /// A Thread's effective cwd or a Project's root. A Group has no cwd.
    fn target_path(&self, target: MenuTarget) -> Option<std::path::PathBuf> {
        match target {
            MenuTarget::Thread(thread) | MenuTarget::Pane(thread) => self.thread_path(thread),
            MenuTarget::Group(_) => None,
            MenuTarget::Project(project) => self
                .cockpit
                .registry()
                .project(project)
                .map(|project| project.root.clone()),
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
    fn press_menu_row(&mut self, index: usize, cx: &mut Context<Self>) {
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
        let confirm = self.prefs.settings.confirm_delete || verb != MenuVerb::Delete;
        if item.destructive && confirm && open.armed != Some(index) {
            open.armed = Some(index);
            cx.notify();
            return;
        }
        let target = open.target;
        self.context_menu = None;
        self.run_menu_verb(target, verb, cx);
        cx.notify();
    }

    fn run_menu_verb(&mut self, target: MenuTarget, verb: MenuVerb, cx: &mut Context<Self>) {
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
            (MenuTarget::Thread(thread) | MenuTarget::Pane(thread), MenuVerb::Close) => {
                self.close_pane(PaneIdentity::Thread(thread), cx);
            }
            (MenuTarget::Thread(thread) | MenuTarget::Pane(thread), MenuVerb::Park) => {
                if let Err(e) = self.cockpit.park(thread) {
                    self.group_error = Some(format!("park refused: {e}").into());
                }
                if self
                    .popover
                    .as_ref()
                    .is_some_and(|open| open.pane == PaneIdentity::Thread(thread))
                {
                    self.popover = None;
                }
                self.sync_panes(cx);
                self.facts.parked_changed(&self.cockpit);
            }
            (MenuTarget::Pane(_), MenuVerb::CopySelection) => {
                if let Some(text) = self.native_copy.clone() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            (MenuTarget::Pane(thread), MenuVerb::CopyTranscript) => {
                if let Some(open) = self.cockpit.thread(thread) {
                    let text = transcript_text(open.transcript().blocks());
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
                    MenuTarget::Project(project) => Some(project),
                    MenuTarget::Thread(thread) | MenuTarget::Pane(thread) => {
                        self.cockpit.project_id(thread)
                    }
                    MenuTarget::Group(group) => self
                        .cockpit
                        .groups()
                        .get(group)
                        .and_then(|group| group.members.first().copied())
                        .and_then(|thread| self.cockpit.project_id(thread)),
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
                self.facts.parked_changed(&self.cockpit);
            }
            (MenuTarget::Project(project), MenuVerb::RemoveProject) => {
                if let Err(error) = self.cockpit.remove_project(project) {
                    self.group_error = Some(format!("remove refused: {error}").into());
                } else {
                    if self.nav_filter == Some(project) {
                        self.nav_filter = None;
                    }
                    self.group_error = None;
                }
            }
            _ => {}
        }
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
                        cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.press_menu_row(index, cx);
                        }),
                    ),
                ),
            };
        }
        Some(
            deferred(
                anchored()
                    .position(open.at)
                    .snap_to_window_with_margin(px(crate::theme::GRID_PAD))
                    .child(shell),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// The Group's board: Panes at their tree rects, seams over the gaps,
    /// the drop preview on top. Coordinates are the board's own — the
    /// frame is `relative`, its origin the nav's right edge — so the rects
    /// the tree computes for the window are shifted back by that origin.
    fn tree_board(
        &self,
        group: GroupId,
        tree: Tree,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let bounds = self.board_bounds(window);
        let origin_x = self.nav_width();
        let local = |rect: layout::Rect| layout::Rect {
            x: rect.x - origin_x,
            ..rect
        };
        let mut board = div().relative().flex_1().min_w_0().min_h_0();
        for (leaf, rect) in tree.rects(bounds, crate::theme::GRID_GAP) {
            let Some(index) = self.index_of(leaf_identity(leaf)) else {
                continue;
            };
            let level = Level::for_cell(Cell::new(rect.w, rect.h));
            let rect = local(rect);
            board = board.child(
                self.pane_cell(index, level, cx)
                    .absolute()
                    .left(px(rect.x))
                    .top(px(rect.y))
                    .w(px(rect.w))
                    .h(px(rect.h)),
            );
        }
        for (at, seam) in tree
            .seams(bounds, crate::theme::GRID_GAP, SEAM_GRAB)
            .into_iter()
            .enumerate()
        {
            let band = local(seam.band);
            let id = seam.id.clone();
            let cursor = match seam.axis {
                layout::Axis::Row => gpui::CursorStyle::ResizeLeftRight,
                layout::Axis::Column => gpui::CursorStyle::ResizeUpDown,
            };
            let dragging = self
                .seam_drag
                .as_ref()
                .is_some_and(|drag| drag.group == group && drag.seam == seam.id);
            let line = match seam.axis {
                layout::Axis::Row => div()
                    .absolute()
                    .left(px((band.w - 2.0) / 2.0))
                    .top_0()
                    .bottom_0()
                    .w(px(2.0)),
                layout::Axis::Column => div()
                    .absolute()
                    .top(px((band.h - 2.0) / 2.0))
                    .left_0()
                    .right_0()
                    .h(px(2.0)),
            };
            board = board.child(
                div()
                    .id(("seam", at))
                    .absolute()
                    .left(px(band.x))
                    .top(px(band.y))
                    .w(px(band.w))
                    .h(px(band.h))
                    .cursor(cursor)
                    // The seam brightens under the pointer and while it is
                    // held: the whole resize affordance.
                    .child(line.rounded(px(1.0)).bg(rgb(if dragging {
                        crate::theme::FOCUS
                    } else {
                        crate::theme::GROUND
                    })))
                    .hover(|style| style.bg(rgba(crate::theme::SEAM_HOVER)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.begin_seam_drag(group, id.clone(), cx);
                        }),
                    ),
            );
        }
        if let Some((target, zone)) = self.drop_preview {
            if let Some((_, rect)) = tree
                .rects(bounds, crate::theme::GRID_GAP)
                .into_iter()
                .find(|(leaf, _)| *leaf == target)
            {
                let wash = local(layout::zone_rect(rect, zone));
                board = board.child(
                    div()
                        .absolute()
                        .left(px(wash.x))
                        .top(px(wash.y))
                        .w(px(wash.w))
                        .h(px(wash.h))
                        .rounded(px(crate::theme::R_SURFACE))
                        .bg(rgba(crate::theme::DROP_WASH))
                        .border_1()
                        .border_color(rgb(crate::theme::DROP_VALID))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .px(px(crate::theme::ROW_PAD_X))
                                .py(px(crate::theme::ROW_PAD_Y))
                                .rounded(px(crate::theme::R_CONTROL))
                                .bg(rgb(crate::theme::MENU))
                                .text_size(px(crate::theme::FS_SM))
                                .text_color(rgb(crate::theme::TEXT))
                                .child(match zone {
                                    Zone::Swap => "⇄ Swap",
                                    Zone::Split(Edge::Left) => "Split left",
                                    Zone::Split(Edge::Right) => "Split right",
                                    Zone::Split(Edge::Top) => "Split above",
                                    Zone::Split(Edge::Bottom) => "Split below",
                                }),
                        ),
                );
            }
        }
        board
    }

    /// A press on a seam: the drag begins from the persisted tree, and
    /// the moves until release re-derive the ratio from the pointer.
    fn begin_seam_drag(&mut self, group: GroupId, seam: SeamId, cx: &mut Context<Self>) {
        let Some(tree) = self.cockpit.group_layout(group) else {
            return;
        };
        self.seam_drag = Some(SeamDrag { group, seam, tree });
        cx.notify();
    }

    /// The pointer moved with a seam held: the ratio follows it, clamped
    /// by the tree to the 20% floor either side.
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
        if let Some(ratio) =
            drag.tree
                .ratio_for(&drag.seam, bounds, pointer, crate::theme::GRID_GAP)
        {
            if drag.tree.set_ratio(&drag.seam, ratio) {
                cx.notify();
            }
        }
    }

    /// The release: the dragged tree persists, once.
    fn end_seam_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.seam_drag.take() else {
            return;
        };
        if let Err(error) = self.cockpit.set_group_layout(drag.group, drag.tree) {
            self.group_error = Some(error.to_string().into());
        }
        cx.notify();
    }

    /// A Pane dragged over another: what a release here would do, for the
    /// wash — nothing over itself, nothing across Groups.
    fn preview_pane_drop(
        &mut self,
        source: ThreadId,
        target: ThreadId,
        position: gpui::Point<Pixels>,
        bounds: gpui::Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let preview = (source != target && self.same_group(source, target)).then(|| {
            let rect = layout::Rect {
                x: f32::from(bounds.origin.x),
                y: f32::from(bounds.origin.y),
                w: f32::from(bounds.size.width),
                h: f32::from(bounds.size.height),
            };
            let pointer = layout::Point {
                x: f32::from(position.x),
                y: f32::from(position.y),
            };
            (target, layout::zone(pointer, rect))
        });
        if self.drop_preview != preview {
            self.drop_preview = preview;
            cx.notify();
        }
    }

    fn same_group(&self, a: ThreadId, b: ThreadId) -> bool {
        match (self.cockpit.groups().of(a), self.cockpit.groups().of(b)) {
            (Some(x), Some(y)) => x.id == y.id,
            _ => false,
        }
    }

    /// The release of a dragged Pane on another: the centre swaps the two
    /// leaves, an edge moves the source beside the target — the tree
    /// persists either way. Reads the preview the last move computed.
    fn drop_pane(&mut self, source: ThreadId, target: ThreadId, cx: &mut Context<Self>) {
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
        let Some(group) = self.cockpit.groups().of(source).map(|group| group.id) else {
            return;
        };
        if !self.same_group(source, target) || source == target {
            return;
        }
        let Some(mut tree) = self.cockpit.group_layout(group) else {
            return;
        };
        let changed = match zone {
            Zone::Swap => tree.swap(source, target),
            Zone::Split(edge) => tree.split(target, edge, source),
        };
        if changed {
            if let Err(error) = self.cockpit.set_group_layout(group, tree) {
                self.group_error = Some(error.to_string().into());
            }
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
            self.popover = None;
            self.context_menu = None;
            self.nav_filter_open = false;
            if self.cli_versions.is_none() {
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
        }
        cx.notify();
    }

    /// Every change to the settings goes through here: saved at once, the
    /// Session defaults the spawner reads refreshed, and the facts that
    /// depend on a setting re-derived.
    fn change_settings(
        &mut self,
        change: impl FnOnce(&mut ferrite_core::settings::Settings),
        cx: &mut Context<Self>,
    ) {
        change(&mut self.prefs.settings);
        if let Err(e) = self.prefs.settings.save(&self.prefs.dir) {
            self.group_error = Some(SharedString::from(format!("settings not saved: {e}")));
        }
        if let Ok(mut defaults) = self.prefs.defaults.lock() {
            *defaults = crate::session::SessionDefaults::from_settings(&self.prefs.settings);
        }
        if self.facts.set_auto_title(self.prefs.settings.auto_title) {
            self.facts.parked_changed(&self.cockpit);
            for thread in self.cockpit.threads() {
                self.facts.renamed(&self.cockpit, thread);
            }
            self.refresh_names();
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

    /// Searchable toolkit Settings, drawn above the cockpit's overlays.
    fn settings_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use gpui::component::setting::SettingGroup;
        if !self.settings_open {
            return None;
        }
        let settings = &self.prefs.settings;
        let mut defaults = vec![prefs::choices(
            "settings-provider",
            "Provider",
            "What a new Thread starts on",
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
        for provider in [Provider::Claude, Provider::Codex] {
            let chosen = settings.model_for(provider).map(str::to_string);
            let mut catalog = self.cockpit.model_catalog(provider);
            if let Some(chosen) = &chosen {
                if !catalog.iter().any(|row| row.is(chosen)) {
                    catalog.push(ferrite_core::ModelInfo::bare(chosen));
                }
            }
            defaults.push(prefs::choices(
                if provider == Provider::Claude {
                    "settings-claude-model"
                } else {
                    "settings-codex-model"
                },
                if provider == Provider::Claude {
                    "Claude model"
                } else {
                    "Codex model"
                },
                "Default uses the CLI's own choice",
                catalog
                    .into_iter()
                    .map(|model| {
                        let value = Some(model.value).filter(|v| v != "default");
                        (model.display.into(), chosen == value, value)
                    })
                    .collect(),
                self.setting_change(cx, move |settings, value| {
                    settings.set_model_for(provider, value)
                }),
            ));
            let effort = settings.effort_for(provider).map(str::to_string);
            let ladder = ferrite_core::providers::models::efforts_for(
                provider,
                chosen.as_deref(),
                &self.cockpit.announced_models(provider),
            );
            defaults.push(prefs::choices(
                if provider == Provider::Claude {
                    "settings-claude-effort"
                } else {
                    "settings-codex-effort"
                },
                if provider == Provider::Claude {
                    "Claude effort"
                } else {
                    "Codex effort"
                },
                "Reasoning depth for new Threads. Each Thread can change its own.",
                std::iter::once(None)
                    .chain(ladder.into_iter().map(Some))
                    .map(|value| {
                        let label = value
                            .as_deref()
                            .map(effort_title)
                            .unwrap_or_else(|| "Default".into());
                        (label.into(), effort == value, value)
                    })
                    .collect(),
                self.setting_change(cx, move |settings, value| {
                    settings.set_effort_for(provider, value)
                }),
            ));
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
        let permissions = vec![
            prefs::choices(
                "settings-claude-mode",
                "Claude permissions",
                "When Claude asks before acting",
                modes(
                    &[
                        ("CLI default", None),
                        ("Ask", Some("default")),
                        ("Accept edits", Some("acceptEdits")),
                        ("Plan", Some("plan")),
                        ("Bypass", Some("bypassPermissions")),
                    ],
                    settings.claude_permission_mode.as_deref(),
                ),
                self.setting_change(cx, |s, v| s.claude_permission_mode = v),
            ),
            prefs::choices(
                "settings-codex-approval",
                "Codex approvals",
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
                "Codex sandbox",
                "What Codex may touch",
                modes(
                    &[
                        ("Codex default", None),
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
            prefs::toggle("settings-auto-title", "Name Threads automatically",
                "Use the first prompt, then a short title from the Thread's Provider. Renaming a Thread keeps your title.",
                settings.auto_title, self.setting_change(cx, |s, v| s.auto_title = v)),
            prefs::toggle("settings-confirm-delete", "Confirm before deleting a Thread", "Ask before removing a Thread and its transcript.",
                settings.confirm_delete, self.setting_change(cx, |s, v| s.confirm_delete = v)),
            prefs::toggle("settings-nav-collapsed", "Start with the sidebar collapsed", "⌘B toggles it any time",
                settings.nav_collapsed, self.setting_change(cx, |s, v| s.nav_collapsed = v)),
        ];
        let (claude, codex) = self
            .cli_versions
            .clone()
            .unwrap_or_else(|| ("checking…".into(), "checking…".into()));
        let about = vec![
            prefs::fact("Claude CLI", claude),
            prefs::fact("Codex CLI", codex),
            prefs::fact(
                "Threads",
                self.prefs.dir.join("threads").display().to_string().into(),
            ),
            prefs::fact(
                "Settings file",
                self.prefs
                    .dir
                    .join(ferrite_core::settings::Settings::FILE)
                    .display()
                    .to_string()
                    .into(),
            ),
        ];
        let groups = vec![
            SettingGroup::new().title("New Threads").items(defaults),
            SettingGroup::new().title("Permissions").items(permissions),
            SettingGroup::new().title("Behaviour").items(behaviour),
            SettingGroup::new().title("About").items(about),
        ];

        let card = prefs::card()
            .id("settings-card")
            .debug_selector(|| "settings-card".into())
            .track_focus(&self.settings_focus)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(prefs::head(prefs::close_button().on_click(cx.listener(
                |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.settings_open = false;
                    cx.notify();
                },
            ))))
            .child(prefs::body(groups));
        Some(
            deferred(
                prefs::veil()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.settings_open = false;
                            cx.notify();
                        }),
                    )
                    .child(card),
            )
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
                return div()
                    .min_w_0()
                    .flex_1()
                    .font_weight(FontWeight::NORMAL)
                    .child(editor.clone())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
                    )
                    .into_any_element();
            }
        }
        let badge = self.panes[index].name.clone();
        let grouped = self.cockpit.groups().of(thread).is_some();
        pane::head_title(self.panes[index].name.clone())
            .id(("pane-title", thread.get() as usize))
            // In a Group the title is the Pane's handle: drag it onto
            // another Pane to swap or split.
            .when(grouped, |title| {
                title.cursor(gpui::CursorStyle::OpenHand).on_drag(
                    PaneDrag { thread },
                    move |_, _, _, cx| {
                        let badge = badge.clone();
                        cx.new(|_| PaneDragPreview(badge))
                    },
                )
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
                return div()
                    .min_w_0()
                    .flex_1()
                    .child(editor.clone())
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

    /// The grid index of the Pane holding the keyboard — the roster's.
    fn focused(&self) -> usize {
        self.cockpit.roster().focused_index()
    }

    fn focused_thread(&self) -> Option<ThreadId> {
        self.cockpit.roster().focused_thread()
    }

    /// The focused Pane's draft, to change — None on a Thread.
    fn focused_draft_mut(&mut self) -> Option<&mut pane::DraftBinding> {
        let focused = self.focused();
        self.panes.get_mut(focused).and_then(PaneView::draft_mut)
    }

    fn submit(&mut self, _: &Submit, _window: &mut Window, cx: &mut Context<Self>) {
        if self.rename.is_some() {
            self.finish_rename(true, cx);
            return;
        }
        // A draft's ↵ (#29): on a band chip it opens that chip's popover;
        // on the prompt line it is the first send — the bootstrap.
        if let Some(draft) = self.panes.get(self.focused()).and_then(PaneView::draft) {
            match draft.band_focus {
                Some(chip) => self.open_band_popover(chip, cx),
                None => self.bootstrap_draft(cx),
            }
            return;
        }
        let Some(thread) = self.focused_thread() else {
            return;
        };
        let composer = self.panes[self.focused()].composer.clone();
        // A question waits: ↵ sends the picks, and whatever is on the
        // line goes with them as the operator's own answer.
        if self.pending_questions(thread).is_some() {
            let text = composer.read(cx).text().trim().to_string();
            let other = Some(text).filter(|text| !text.is_empty());
            if self.submit_questions(thread, other, cx) {
                composer.update(cx, |composer, cx| {
                    composer.take(cx);
                });
            }
            return;
        }
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
        if self.cockpit.thread(thread).is_some_and(|open| open.busy()) {
            self.cockpit.queue(thread, text.clone());
        } else {
            self.cockpit.send(thread, text.clone());
            self.panes[self.focused()].follow_tail.set(true);
            self.panes[self.focused()].scroll.scroll_to_bottom();
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
        _: &crate::composer::Backspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        if !self.panes[self.focused()].composer.read(cx).is_empty() {
            return;
        }
        if self.cockpit.unqueue(thread).is_some() {
            cx.notify();
        }
    }

    fn interrupt(&mut self, _: &Interrupt, _window: &mut Window, cx: &mut Context<Self>) {
        if self.context_usage.take().is_some() {
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
        if let Some(thread) = self.focused_thread() {
            self.cockpit.interrupt(thread);
            self.facts.acted(&self.cockpit, thread);
        }
        cx.notify();
    }

    /// Tab walks a draft's band (#29), or an L1 Thread Pane's rendered tool
    /// disclosures before returning to the Composer.
    fn band_cycle(&mut self, _: &BandCycle, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open {
            window.focus_next(cx);
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
        if self.settings_open {
            window.focus_prev(cx);
        } else {
            self.cycle_tools(true, window, cx);
        }
    }

    fn cycle_tools(&mut self, reverse: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.level_now(window) != Level::Transcript
            || self.panes[self.focused()].draft().is_some()
        {
            return;
        }
        let focused = self.focused();
        let calls = self.expandable_tools(focused, Level::Transcript);
        let targeted = self.panes[focused].cycle_tools(&calls, reverse).is_some();
        let focus = if targeted {
            self.panes[focused].tool_focus()
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
        let Some(call) = self.panes[self.focused()]
            .targeted_tool()
            .map(str::to_string)
        else {
            return;
        };
        self.toggle_tool(thread, &call, window, cx);
    }

    fn expandable_tools(&self, index: usize, level: Level) -> Vec<String> {
        let Some(thread) = self.panes[index].thread() else {
            return Vec::new();
        };
        self.cockpit
            .thread(thread)
            .map(|open| open.transcript())
            .into_iter()
            .flat_map(|transcript| pane::rendered_output_tools(transcript.blocks(), level))
            .map(|tool| tool.call.clone())
            .collect()
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
        self.answer(answer, cx);
    }

    fn answer(&mut self, answer: Answer, cx: &mut Context<Self>) {
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
            _ => self.cockpit.next_blocked(None),
        };
        let Some(thread) = thread else {
            return;
        };
        let Some(decision) = self
            .cockpit
            .thread(thread)
            .and_then(|open| open.pending())
            .cloned()
        else {
            return;
        };
        // A question is answered by its form, never by a bare "allow" —
        // allowing an unanswered question would send the model nothing.
        if pane::question_of(&decision).is_some() && answer != Answer::Deny {
            self.submit_questions(thread, None, cx);
            return;
        }
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
        self.facts.acted(&self.cockpit, thread);
        cx.notify();
    }

    fn next_pane(&mut self, _: &NextPane, _window: &mut Window, cx: &mut Context<Self>) {
        self.cockpit.step_focus(1);
        cx.notify();
    }

    fn previous_pane(&mut self, _: &PreviousPane, _window: &mut Window, cx: &mut Context<Self>) {
        self.cockpit.step_focus(-1);
        cx.notify();
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
        self.cockpit.toggle_fullscreen();
        cx.notify();
    }

    /// cmd-b (#21): fold the nav to its 40px LED rail, or open it back to
    /// the 208px column. The width change feeds `cell()`, so Panes may
    /// legitimately change Level — size decides, no special case.
    fn toggle_nav(&mut self, _: &ToggleNav, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_nav_collapsed(!self.nav_collapsed, cx);
    }

    fn set_nav_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.nav_collapsed == collapsed {
            return;
        }
        self.nav_collapsed = collapsed;
        self.nav_has_toggled = true;
        cx.notify();
    }

    // --------------------------------------------------- Composer menus (#23)

    /// Re-derive the text-derived popover from the focused line's own text.
    /// Nothing else opens or closes a menu: `/` at the start opens commands,
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
        if let Some(filter) = slash_filter(&text) {
            // A draft has one local command: import. It is derived through
            // the same fuzzy slash menu as a live Thread, so `/`, `/im`,
            // and `/import` all reach the picker without ever becoming the
            // draft's first provider prompt.
            let Some(thread) = thread else {
                let row = local_row(filter, "import", "adopt a CLI session file", false)?;
                return Some(Popover {
                    pane: pane.identity,
                    kind: Kind::Commands,
                    rows: vec![Row {
                        row,
                        active: false,
                        consequence: Consequence::OpenImportPicker,
                    }],
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
            // while the Thread still offers adoption. #25: `provider`
            // always — live before the first prompt, and kept visible but
            // inert after it, so the door's absence never reads as a bug.
            // A local row replaces a provider command of the same name
            // (Claude lists its own `effort`): Ferrite's opens the picker,
            // and the typed form (`/effort max`) still works — `submit`
            // reads it.
            let push_local = |rows: &mut Vec<Row>, row: Row| {
                rows.retain(|existing| existing.row.name != row.row.name);
                rows.insert(0, row);
                rows.truncate(MENU_ROWS_MAX);
            };
            if pane::offers_import(Some(open.transcript())) {
                if let Some(row) = local_row(filter, "import", "adopt a CLI session file", false) {
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
            let detail = if open.first_prompt_sent() {
                "switch model · hand over to the other provider"
            } else {
                "switch provider / model"
            };
            if let Some(row) = local_row(filter, "effort", "switch reasoning effort", false) {
                push_local(
                    &mut rows,
                    Row {
                        row,
                        active: false,
                        consequence: Consequence::OpenEffortPicker,
                    },
                );
            }
            if let Some(row) = local_row(filter, "model", detail, false) {
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
                pane: pane.identity,
                kind: Kind::Commands,
                rows,
                selected: 0,
            });
        }
        let (token_start, filter) = mention_token(&text, cursor)?;
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
                .resolve(self.cockpit.registry())
                .ok()?
                .source_root()
                .to_path_buf(),
            _ => return None,
        };
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
            pane: pane.identity,
            kind: Kind::Files { files, token_start },
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
        let composer = self.panes[index].composer.clone();
        let draft = composer.read(cx).text().to_string();
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
        let Some(thread) = self.panes.get(index).and_then(PaneView::thread) else {
            return false;
        };
        let Some(open) = self.cockpit.thread(thread) else {
            return false;
        };
        open.has_prompt_history()
            && !self.panes[index].has_tool_target()
            && self.rename.is_none()
            && !open.busy()
            && open.pending().is_none()
            && open.queued().is_none()
            && self.popover.is_none()
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
            cx.notify();
        }
    }

    fn menu_pick(&mut self, _: &MenuPick, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = self.popover.as_ref().map(|open| open.selected) {
            self.pick(at, cx);
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
            Kind::ImportFile | Kind::Provider | Kind::Effort => {}
        }
        cx.notify();
    }

    /// The shared tail of ↵ and a row click: the row's own consequence,
    /// dispatched. The popover closes either way — a door it opens (the
    /// pickers) is a fresh popover in its place.
    fn pick(&mut self, at: usize, cx: &mut Context<Self>) {
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
        // Every command pick replaces the whole line.
        let splice_line = |cx: &mut Context<Self>, text: &str| {
            composer.update(cx, |composer, cx| {
                let whole = 0..composer.text().len();
                composer.splice(whole, text, cx);
            });
        };
        match &row.consequence {
            Consequence::Command(name) => splice_line(cx, &format!("/{name} ")),
            Consequence::Inert => {}
            Consequence::OpenProviderPicker => {
                if let Some(thread) = open.pane.thread() {
                    splice_line(cx, "");
                    self.open_provider_picker(thread, cx);
                }
            }
            Consequence::OpenImportPicker => {
                if open.pane.thread().is_some() {
                    splice_line(cx, "");
                }
                self.open_import_picker(open.pane, cx);
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
            Consequence::Adopt(path) => self.adopt_file(open.pane, path, cx),
            Consequence::Provision(choice) => {
                if let Some(thread) = open.pane.thread() {
                    self.pick_provider(thread, choice.clone(), cx);
                }
            }
            Consequence::OpenEffortPicker => {
                if let Some(thread) = open.pane.thread() {
                    splice_line(cx, "");
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

    /// #11: discovery and the file-pick popover, run once per open — never
    /// per frame. With nothing to list it says so in the transcript instead
    /// of opening an empty popover; the Notice is Ferrite's own out-of-band
    /// line, so the Thread keeps offering import. On a draft the words land
    /// where the band is.
    fn open_import_picker(&mut self, from: PaneIdentity, cx: &mut Context<Self>) {
        let candidates =
            ferrite_core::import::candidates(&self.session_file_roots, IMPORT_ROWS_MAX);
        if candidates.is_empty() {
            let roots = self
                .session_file_roots
                .iter()
                .map(|(_, root)| root.display().to_string())
                .collect::<Vec<_>>()
                .join(" or ");
            let message = format!("no CLI session files found under {roots}");
            self.say_to(from, message);
            cx.notify();
            return;
        }
        let now = std::time::SystemTime::now();
        let rows = candidates
            .into_iter()
            .map(|candidate| {
                let name = candidate
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let row = pane::MenuRow {
                    // Nothing lands in the line on ↵: the pick reads the
                    // consequence riding beside this row.
                    insert: SharedString::default(),
                    name: SharedString::from(provider_label(candidate.provider)),
                    matched: Vec::new(),
                    detail: SharedString::from(format!(
                        "{name} · {}",
                        age_label(candidate.modified, now)
                    )),
                    prose_detail: false,
                    inert: false,
                };
                Row {
                    row,
                    active: false,
                    consequence: Consequence::Adopt(candidate.path),
                }
            })
            .collect();
        self.popover = Some(Popover {
            pane: from,
            kind: Kind::ImportFile,
            rows,
            selected: 0,
        });
        cx.notify();
    }

    /// Ferrite's own words to one Pane: a Notice in a Thread's transcript,
    /// or the error line under a draft's band.
    fn say_to(&mut self, pane: PaneIdentity, message: String) {
        match pane {
            PaneIdentity::Thread(thread) => self
                .cockpit
                .apply_input(thread, ferrite_core::transcript::Input::Notice(message)),
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
    /// from its handshake; Codex's from the catalog. Opens before and after
    /// the first prompt: the model can always change (the conversation is
    /// resumed under the new one), and once the prompt has gone out the
    /// other Provider's section is drawn inert and says why.
    fn open_provider_picker(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        let current = open.provider();
        let chosen = open.model().map(str::to_string);
        let serving = open.transcript().model().map(str::to_string);
        let locked = open.first_prompt_sent();
        let (rows, selected) = self.provider_rows(
            current,
            chosen.as_deref(),
            serving.as_deref(),
            locked,
            |choice| Consequence::Provision(choice),
        );
        // The `/` menu the pick came through is already closed; a chip
        // click replaces whatever the slot held outright.
        self.popover = Some(Popover {
            pane: PaneIdentity::Thread(thread),
            kind: Kind::Provider,
            rows,
            selected,
        });
        cx.notify();
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
                    detail: SharedString::from(if handover {
                        "hands the conversation over"
                    } else {
                        ""
                    }),
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
                rows.push(Row {
                    row: pane::MenuRow {
                        insert: SharedString::default(),
                        name: SharedString::from(model.display.clone()),
                        matched: Vec::new(),
                        detail: SharedString::from(model.detail.clone()),
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

    /// A model-row pick: the same Provider re-aims the model — before the
    /// first prompt eagerly, after it by resuming the conversation under
    /// the new model; another Provider is `set_provider`'s pre-prompt swap.
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
            self.cockpit.apply_input(
                thread,
                ferrite_core::transcript::Input::Notice(format!("model unchanged: {e}")),
            );
        }
        self.facts.acted(&self.cockpit, thread);
        cx.notify();
    }

    /// An import-row pick (#11): adopt the picked file through the core
    /// door, in place of the Pane it was picked from — a draft becomes the
    /// imported Thread and its line clears; a blank Thread yields its slot.
    /// A refusal is the core's readable words, surfaced in that Pane — and
    /// the door stays open for the next try.
    fn adopt_file(&mut self, from: PaneIdentity, path: &std::path::Path, cx: &mut Context<Self>) {
        match self.cockpit.adopt_into(from, path) {
            Ok(adopted) => {
                if let Some(error) = &adopted.not_opened {
                    // Durable but not on screen: the Thread sits in the
                    // nav's parked rows, exactly like a launch-time import
                    // that would not open.
                    eprintln!(
                        "ferrite: imported thread {} would not open: {error:?}",
                        adopted.thread
                    );
                } else if let Some(index) = from.draft().and_then(|_| self.index_of(from)) {
                    self.panes[index].composer.update(cx, |composer, cx| {
                        composer.take(cx);
                    });
                    self.panes[index].adopt_thread(adopted.thread);
                }
                if let Some(error) = &adopted.blank_kept {
                    eprintln!("ferrite: the blank thread stayed open: {error}");
                }
                self.sync_panes(cx);
                // A draft that became the import keeps its Pane, so the
                // mirror saw nothing open; a Thread that would not open is
                // a fresh parked row.
                if adopted.not_opened.is_some() {
                    self.facts.parked_changed(&self.cockpit);
                } else if from.draft().is_some() {
                    self.facts.opened(&self.cockpit, adopted.thread);
                }
            }
            Err(e) => self.say_to(from, format!("cannot import {}: {e}", path.display())),
        }
        cx.notify();
    }

    // ------------------------------------------------ draft Pane + band (#29)

    /// cmd-t's answer (#29): a draft Pane — a Composer, the pre-prompt
    /// band, and nothing durable until the first send bootstraps a Thread.
    /// The provider follows the Pane the operator is on; the project starts
    /// on the launch project; `target` is the caller's (cmd-shift-n drafts
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
                        provider: open.map_or(Provider::Claude, |open| open.provider()),
                        model: open.and_then(|open| open.model()).map(str::to_string),
                    }
                }
            })
            .unwrap_or_else(|| self.default_choice());
        self.open_draft_with_choice(target, provider, cx);
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
            cx,
        );
    }

    fn open_draft_with_choice(
        &mut self,
        target: DraftTarget,
        provider: ProviderChoice,
        cx: &mut Context<Self>,
    ) {
        // The project starts where the operator is looking: a Group's own
        // Project, or the launch project.
        let project = match self.cockpit.roster().view() {
            View::Group(group) => self
                .cockpit
                .groups()
                .get(group)
                .and_then(|group| group.members.first())
                .and_then(|thread| self.cockpit.project_id(*thread))
                .unwrap_or(self.launch_project),
            View::Solo => self.launch_project,
        };
        let binding = pane::DraftBinding {
            binding: ferrite_core::draft::DraftBinding::new(provider, project, target),
            band_focus: None,
            error: None,
        };
        // The roster scopes the draft to the current view and focuses it.
        let draft = self.cockpit.open_draft();
        let pane = PaneView::new_draft(draft, binding, cx);
        cx.subscribe(&pane.composer, Self::composer_edited).detach();
        self.panes.push(pane);
        self.sync_panes(cx);
        cx.notify();
    }

    /// Open one chip's popover on the focused draft — the shared tail of a
    /// chip click and ↵ on a tab-focused chip. Toggles shut when the same
    /// chip's popover is already up. Rows are registry reads, discovered at
    /// open — never per frame, never a filesystem scan.
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
        let rows = self.band_rows(draft, chip, cx);
        // The arrows start on the standing choice — bare ↵ re-picks it.
        let selected = rows.iter().position(|row| row.active).unwrap_or(0);
        self.popover = Some(Popover {
            pane: identity,
            kind: Kind::Band(chip),
            rows,
            selected,
        });
        cx.notify();
    }

    /// One chip's rows for the focused draft. The workspace chip is scoped
    /// to the chosen project alone: `main`, that project's registered
    /// worktrees, `new worktree` — no global list anywhere.
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
                        None => "the CLI's own choice".to_string(),
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
                        band_row(
                            SharedString::from(project.title.clone()),
                            SharedString::from(project.root.display().to_string()),
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
                        BandChoice::RegisterPath(expand_home(path)),
                    ));
                }
                rows.push(band_row(
                    SharedString::from("Choose folder…"),
                    SharedString::from("add a Project"),
                    false,
                    BandChoice::Browse,
                ));
                rows
            }
            pane::BandChip::Workspace => {
                let mut rows = vec![band_row(
                    SharedString::from("main"),
                    SharedString::from("the project checkout"),
                    *draft.binding.target() == DraftTarget::Main,
                    BandChoice::Target(DraftTarget::Main),
                )];
                for entry in self.cockpit.registry().worktrees(draft.binding.project()) {
                    let branch = entry.branch.clone();
                    rows.push(band_row(
                        SharedString::from(branch.clone()),
                        SharedString::from("worktree"),
                        matches!(
                            draft.binding.target(),
                            DraftTarget::Existing { branch: chosen } if *chosen == branch
                        ),
                        BandChoice::Target(DraftTarget::Existing { branch }),
                    ));
                }
                rows.push(band_row(
                    SharedString::from("new worktree"),
                    SharedString::from("created at first send"),
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
        let Some(draft) = self.panes.get(self.focused()).and_then(PaneView::draft) else {
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
            BandChoice::Browse => self.browse_for_project(BrowseThen::Draft, cx),
        }
        cx.notify();
    }

    /// The platform's folder picker, for a Project not yet registered.
    /// The picker is modal to the window and answers later; the folder it
    /// returns registers through the core door and lands where `then`
    /// says. Cancel changes nothing.
    fn browse_for_project(&mut self, then: BrowseThen, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add Project".into()),
        });
        cx.spawn(async move |this, cx| {
            let picked = match receiver.await {
                Ok(Ok(Some(mut paths))) => paths.pop(),
                _ => None,
            };
            let Some(path) = picked else {
                return;
            };
            this.update(cx, |view, cx| view.adopt_browsed_project(path, then, cx))
                .ok();
        })
        .detach();
    }

    /// A folder the picker returned: registered, then chosen where the
    /// picker was opened from — the draft's Project chip, or the nav's
    /// filter. A refusal lands where that surface shows errors.
    fn adopt_browsed_project(
        &mut self,
        path: std::path::PathBuf,
        then: BrowseThen,
        cx: &mut Context<Self>,
    ) {
        match self.cockpit.register_project(&path) {
            Ok(project) => match then {
                BrowseThen::Draft => {
                    if !self.cancel_focused_draft_start() {
                        return;
                    }
                    if let Some(draft) = self.focused_draft_mut() {
                        draft.binding.choose_checkout(project);
                        draft.error = None;
                    }
                }
                BrowseThen::Filter => {
                    self.nav_filter = Some(project);
                    self.group_error = None;
                }
            },
            Err(e) => {
                let message = SharedString::from(format!("cannot add {}: {e}", path.display()));
                match then {
                    BrowseThen::Draft => {
                        if let Some(draft) = self.focused_draft_mut() {
                            draft.error = Some(message);
                        }
                    }
                    BrowseThen::Filter => self.group_error = Some(message),
                }
            }
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
        let text = composer.read(cx).text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let provider = draft.binding.provider().clone();
        let effort = draft.binding.effort().map(str::to_owned);
        let resolved = draft
            .binding
            .resolve(self.cockpit.registry())
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
                    if composer.text().trim() == text {
                        composer.take(cx);
                    }
                });
                self.panes[index].adopt_thread(done.thread);
                if done.applied_leave {
                    self.group_error = None;
                }
                if let Some(error) = done.refused_leave {
                    eprintln!("ferrite: group change refused: {error}");
                    self.group_error = Some(error.to_string().into());
                }
                if self.popover.as_ref().is_some_and(|open| {
                    open.pane == PaneIdentity::Draft(id) && matches!(open.kind, Kind::Band(_))
                }) {
                    self.popover = None;
                }
                self.sync_panes(cx);
                self.facts.opened(&self.cockpit, done.thread);
                self.refresh_names();
                self.start_titling(done.thread, text.to_string(), cx);
                self.panes[index].follow_tail.set(true);
                self.panes[index].scroll.scroll_to_bottom();
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

    /// The focused draft's setup chips — project and workspace, wired to
    /// their popovers (#29) and riding the left of the controls row. The
    /// model and effort controls sit in the trailing slot instead, where a
    /// live Thread's Composer draws them: `draft_model_picker`.
    ///
    /// A chip click shares `open_band_popover` with ↵ on a tab-focused
    /// chip; the closure re-finds the Pane by its Composer, a draft's one
    /// stable identity.
    fn draft_band_element(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(draft) = self.panes[index].draft() else {
            return div().into_any_element();
        };
        let composer = self.panes[index].composer.clone();
        let project_title = self
            .cockpit
            .registry()
            .project(draft.binding.project())
            .map(|project| project.title.clone())
            .unwrap_or_else(|| "project".into());
        let workspace_label = match draft.binding.target() {
            DraftTarget::Main => SharedString::from("main"),
            DraftTarget::Existing { branch } => SharedString::from(branch.clone()),
            DraftTarget::New => SharedString::from("new worktree"),
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
        let mut band = pane::draft_band();
        for (slot, (chip, label)) in chips.into_iter().enumerate() {
            let focused = draft.band_focus == Some(chip);
            let chip_composer = composer.clone();
            band = band.child(pane::band_chip(slot, label, false, focused).on_mouse_down(
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
            ));
        }
        band.into_any_element()
    }

    /// The focused draft's model and effort controls (#29): the same
    /// pickers a live Thread's Composer draws, in the same trailing slot,
    /// so a new Thread and an existing one share one input silhouette.
    /// They open the band's popovers rather than the Thread's.
    fn draft_model_picker(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(draft) = self.panes[index].draft() else {
            return div().into_any_element();
        };
        let composer = self.panes[index].composer.clone();
        let provider = draft.binding.provider().provider;
        // The provider's own word until a model is chosen; the groomed
        // model name after — one spelling, wherever it shows.
        let model_label = match draft.binding.provider().model.as_deref() {
            Some(model) => SharedString::from(ferrite_core::providers::models::label(
                model,
                &self.cockpit.model_catalog(provider),
            )),
            None => SharedString::from(provider_title(provider)),
        };
        let effort_label = match draft.binding.effort() {
            Some(effort) => SharedString::from(effort_title(effort)),
            None => match self.prefs.settings.effort_for(provider) {
                Some(effort) => SharedString::from(effort_title(effort)),
                None => SharedString::from("effort"),
            },
        };
        let controls = [
            (
                pane::BandChip::Provider,
                pane::draft_picker(
                    "draft-model-picker",
                    draft.band_focus == Some(pane::BandChip::Provider),
                    pane::model_picker(Some(provider), model_label),
                ),
            ),
            (
                pane::BandChip::Effort,
                pane::draft_picker(
                    "draft-effort-picker",
                    draft.band_focus == Some(pane::BandChip::Effort),
                    pane::effort_picker(effort_label),
                ),
            ),
        ];
        let mut row = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(crate::theme::KEYS_GAP));
        for (chip, control) in controls {
            let chip_composer = composer.clone();
            row = row.child(control.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
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
            ));
        }
        row.into_any_element()
    }

    /// Test-only: aim the launch project at a scratch repo — production
    /// registers `here()` once at construction, which tests cannot sit in.
    #[cfg(test)]
    fn aim_launch(&mut self, root: &std::path::Path) {
        self.launch_project = self
            .cockpit
            .register_project(root)
            .expect("the scratch repo registers");
        for pane in &mut self.panes {
            if let Some(draft) = pane.draft_mut() {
                draft.binding.choose_project(self.launch_project);
            }
        }
    }

    /// The open popover for this Pane, rows wired to their picks —
    /// assembled here so its clicks land beside every other pointer wire
    /// (the root selector's precedent); the Pane hangs it above the line.
    /// Menu and import rows draw as `menu_row`; picker and band rows carry
    /// the ✓ grammar — what the Thread or draft is on right now — as
    /// `picker_row`, with the muted detail tagging the section.
    fn popover_element(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self
            .popover
            .as_ref()
            .filter(|open| open.pane == self.panes[index].identity)?;
        if open.kind.picker_slot().is_some() {
            return None;
        }
        // A press on the popover's own dead space is not a press outside
        // it: swallowed, so the root's dismissal never sees it.
        let mut popover = pane::menu_popover().on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
        );
        for (at, row) in open.rows.iter().enumerate() {
            let drawn = match open.kind {
                Kind::Commands | Kind::Files { .. } | Kind::ImportFile => {
                    pane::menu_row(&row.row, at == open.selected)
                }
                Kind::Provider | Kind::Band(pane::BandChip::Provider)
                    if row.inert
                        && row.consequence_is_inert()
                        && provider_of_title(&row.name).is_some() =>
                {
                    pane::picker_section(provider_of_title(&row.name).unwrap(), row.detail.clone())
                }
                Kind::Provider | Kind::Effort | Kind::Band(_) => pane::picker_row(
                    row.name.clone(),
                    row.detail.clone(),
                    at == open.selected,
                    row.active,
                    row.inert,
                ),
            };
            popover = popover.child(drawn.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.pick(at, cx);
                }),
            ));
        }
        popover = popover.child(pane::popover_footer(open.kind.hints()));
        Some(popover.into_any_element())
    }

    /// The nav's per-frame state, from caches and O(1) reads only. Nothing
    /// here touches the store, and nothing here is a Pane decision: the
    /// Project filter narrows this list and nothing else.
    fn nav_state(&self) -> nav::NavState {
        let mut label = SharedString::from("All Projects");
        let mut options = vec![nav::FilterOption {
            project: None,
            label: SharedString::from("All Projects"),
            selected: self.nav_filter.is_none(),
        }];
        for project in self.cockpit.registry().projects() {
            let selected = self.nav_filter == Some(project.id);
            if selected {
                label = SharedString::from(project.title.clone());
            }
            options.push(nav::FilterOption {
                project: Some(project.id),
                label: SharedString::from(project.title.clone()),
                selected,
            });
        }
        let filter = nav::FilterState {
            label,
            open: self.nav_filter_open,
            options,
        };

        // Drafts are not rows: nothing runs, nothing parks, nothing to aim
        // the nav at (#29) — the grid is where a draft lives.
        let focused = self.cockpit.roster().focused_thread();
        let groups: Vec<nav::GroupBlock> = self
            .cockpit
            .groups()
            .iter()
            .filter_map(|group| {
                // A member whose leave is parked on a pending draft has
                // already left as far as the operator is concerned, here
                // exactly as in `visible_indices`.
                let pending_leave = self.cockpit.roster().pending_leave(group.id);
                let members: Vec<nav::ThreadRow> = group
                    .members
                    .iter()
                    .filter(|thread| Some(**thread) != pending_leave)
                    .filter(|thread| self.admitted(**thread))
                    .map(|thread| self.thread_row(*thread))
                    .collect();
                // A Group is shown when any member survives the filter, and
                // then only its surviving members are drawn.
                if members.is_empty() {
                    return None;
                }
                let projects: std::collections::HashSet<_> = group
                    .members
                    .iter()
                    .filter(|thread| Some(**thread) != pending_leave)
                    .filter_map(|thread| self.facts.get(*thread).and_then(|facts| facts.project))
                    .collect();
                let project_summary = match projects.len() {
                    0 => None,
                    1 => members.iter().find_map(|row| row.project.clone()),
                    count => Some(format!("{count} projects").into()),
                };
                Some(nav::GroupBlock {
                    id: group.id,
                    title: group.display_title().into(),
                    // Summarize the whole Group even when the filter hides
                    // some member rows; opening it still shows every Pane.
                    projects: project_summary,
                    current: focused.is_some_and(|thread| group.members.contains(&thread)),
                    members,
                })
            })
            .collect();

        // ...and so it lands in the solos below, which is where a Thread on
        // its way out of a Group belongs.
        let grouped: std::collections::HashSet<ThreadId> = self
            .cockpit
            .groups()
            .iter()
            .flat_map(|group| {
                let pending_leave = self.cockpit.roster().pending_leave(group.id);
                group
                    .members
                    .iter()
                    .copied()
                    .filter(move |thread| Some(*thread) != pending_leave)
            })
            .collect();
        // Open Panes' Threads first, in pane order, then the park order.
        let mut solos: Vec<nav::ThreadRow> = self
            .panes
            .iter()
            .filter_map(PaneView::thread)
            .chain(
                self.facts
                    .parked()
                    .iter()
                    .copied()
                    .filter(|thread| self.pane_for(*thread).is_none()),
            )
            .filter(|thread| !grouped.contains(thread) && self.admitted(*thread))
            .map(|thread| self.thread_row(thread))
            .collect();
        solos.dedup_by_key(|row| row.thread);

        // The nav's default order (#21): most recently used first, across
        // open and parked alike, and across Groups and solo Threads alike —
        // one list, not a Groups shelf above a Threads shelf. A Group is as
        // recent as its most recently used member; its own members keep the
        // operator's order, because that order *is* the Group.
        //
        // `sort_by_key` is stable, so items sharing a second keep the order
        // they were gathered in: Groups in the roster's order, Threads in
        // pane-then-park order.
        let mut order: Vec<(std::time::SystemTime, nav::NavItem)> = groups
            .iter()
            .enumerate()
            .map(|(index, block)| {
                let recency = block
                    .members
                    .iter()
                    .map(|row| self.last_used(row.thread))
                    .max()
                    .unwrap_or(std::time::UNIX_EPOCH);
                (recency, nav::NavItem::Group(index))
            })
            .chain(
                solos
                    .iter()
                    .enumerate()
                    .map(|(index, row)| (self.last_used(row.thread), nav::NavItem::Solo(index))),
            )
            .collect();
        order.sort_by_key(|(recency, _)| std::cmp::Reverse(*recency));
        let order = order.into_iter().map(|(_, item)| item).collect();

        nav::NavState {
            filter,
            groups,
            solos,
            order,
            collapsed: self.nav_collapsed,
        }
    }

    /// Does the Project filter admit this Thread? `All Projects` admits
    /// everything; a chosen Project admits only Threads whose cached
    /// ProjectId is it — so a Thread whose Project is unknown appears under
    /// `All Projects` alone, rather than being quietly filed under someone
    /// else's Project.
    fn admitted(&self, thread: ThreadId) -> bool {
        let Some(wanted) = self.nav_filter else {
            return true;
        };
        self.facts.get(thread).and_then(|facts| facts.project) == Some(wanted)
    }

    /// One Thread's nav row, entirely from caches: the name core has (there
    /// is no display name), the cached Project and checkout, and the
    /// provider — live for an open Thread, the parked cache otherwise.
    fn thread_row(&self, thread: ThreadId) -> nav::ThreadRow {
        let facts = self.facts.get(thread);
        let open = self.cockpit.thread(thread);
        let status = match open {
            None => nav::RowStatus::Parked,
            Some(open) => {
                let failing = facts.is_some_and(|facts| facts.wall.tests_failing);
                match pane::wall_state(Some(open.transcript()), open.pending().is_some(), failing) {
                    pane::WallState::Working => nav::RowStatus::Working,
                    pane::WallState::Failing => nav::RowStatus::Failing,
                    pane::WallState::Decision => nav::RowStatus::Attention,
                    pane::WallState::Blocked => nav::RowStatus::Blocked,
                    pane::WallState::Parked => nav::RowStatus::Parked,
                    pane::WallState::Idle | pane::WallState::Done => nav::RowStatus::Idle,
                }
            }
        };
        let now = std::time::SystemTime::now();
        nav::ThreadRow {
            thread,
            name: self.facts.name(thread),
            status,
            project: facts.and_then(|facts| facts.project_label.clone()),
            branch: facts.and_then(|facts| facts.branch.clone()),
            provider: self
                .cockpit
                .thread(thread)
                .map(|open| open.provider())
                .or_else(|| facts.and_then(|facts| facts.provider)),
            current: self.cockpit.roster().focused_thread() == Some(thread),
            last_used: facts
                .and_then(|facts| facts.last_used)
                .map(|at| crate::facts::since_label(at, now)),
        }
    }

    /// When a Thread was last used, for the nav's default order. A Thread
    /// whose log cannot be stat'd sorts to the bottom rather than to the
    /// top: an unknown time is not a recent one.
    fn last_used(&self, thread: ThreadId) -> std::time::SystemTime {
        self.facts
            .last_used(thread)
            .unwrap_or(std::time::UNIX_EPOCH)
    }

    /// A running nav row's click: land on that Thread's Pane, in the view
    /// that shows it — the core's one door, so a fullscreened cockpit
    /// re-aims to the clicked Thread like every other deliberate move.
    fn focus_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if self.cockpit.focus_thread(thread) {
            if let Some(index) = self.pane_for(thread) {
                self.focus_pane(index);
            }
            cx.notify();
        }
    }

    /// Open a Group (#28): the core revives every member and enters the
    /// view; the mirror follows, and the parked rows forget them.
    pub(crate) fn enter_group(&mut self, group: GroupId, cx: &mut Context<Self>) {
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
        match self.cockpit.drop(drag.drag, drag.origin, target) {
            Ok(()) => self.group_error = None,
            Err(error) => {
                eprintln!("ferrite: group change refused: {error}");
                self.group_error = Some(error.to_string().into());
            }
        }
        self.sync_panes(cx);
        cx.notify();
    }

    /// Revive one parked Thread into the view that shows it — the shared
    /// tail of cmd-o and a parked nav row's click (#21).
    fn revive_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        match self.cockpit.reopen(thread) {
            Ok(()) => {
                self.sync_panes(cx);
                cx.notify();
            }
            Err(e) => eprintln!("ferrite: thread {thread} could not be reopened: {e:?}"),
        }
    }

    /// Focus by grid index — the pointer's and the tests' spelling of the
    /// core's one door.
    fn focus_pane(&mut self, index: usize) {
        if let Some(pane) = self.panes.get(index) {
            self.cockpit.focus(pane.identity);
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
                eprintln!("ferrite: thread {thread} could not be reopened: {e:?}")
            }
            None => {}
        }
    }

    /// Close the focused Pane — cmd-w. The core decides what that means
    /// (park, leave, defer onto a draft, discard); the mirror follows.
    fn close_thread(&mut self, _: &CloseThread, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(identity) = self.cockpit.roster().focused() else {
            return;
        };
        self.close_pane(identity, cx);
    }

    /// Close one Pane through the core act, its refusal landing where every
    /// other Group error does — and a park that would not flush is logged,
    /// the Pane gone either way.
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
                eprintln!("ferrite: pane {identity:?} did not park cleanly: {e}")
            }
            Err(CloseError::Group(error)) => {
                eprintln!("ferrite: group change refused: {error}");
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
    fn next_decision(&mut self, _: &NextDecision, _window: &mut Window, cx: &mut Context<Self>) {
        if self.cockpit.next_decision().is_some() {
            self.focus_pane(self.focused());
            cx.notify();
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

    /// Exactly the highlighted text to the clipboard. With nothing visibly
    /// selected — cleared, or every selected row gone from the rendered
    /// window — the clipboard is left alone.
    fn copy_selection(&mut self, _: &CopySelection, window: &mut Window, cx: &mut Context<Self>) {
        let text = gpui::base::TextSelection::selected_text(window, cx)
            .trim_end_matches('\n')
            .to_string();
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// ⌘V anywhere in the window lands in the focused Pane's Composer.
    /// The Composer's own Paste runs first while it holds the keyboard;
    /// this is the fallback for when a click in the transcript — a drag
    /// to select, a tool row — took the keyboard away: the text goes
    /// where the operator is about to type, and the keyboard follows it.
    fn paste_into_composer(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
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
        composer.update(cx, |composer, cx| composer.insert(&text, cx));
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

/// How many session files the import picker lists (#11) — the same dense
/// keyboard-menu bound as the Composer menus. Newest first is how the
/// operator finds the session they just left.
const IMPORT_ROWS_MAX: usize = MENU_ROWS_MAX;

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

/// A file's age as the operator scans it — the picker row's meta.
fn age_label(modified: Option<std::time::SystemTime>, now: std::time::SystemTime) -> String {
    let Some(modified) = modified else {
        return "age unknown".into();
    };
    // A file from the future (clock skew) is as good as new.
    let Ok(elapsed) = now.duration_since(modified) else {
        return "just now".into();
    };
    let secs = elapsed.as_secs();
    if secs < 60 {
        "just now".into()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
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
        self.measure();
        if self.context_menu.is_none() {
            let copied = gpui::base::TextSelection::selected_text(window, cx)
                .trim_end_matches('\n')
                .to_string();
            self.native_copy = (!copied.is_empty()).then_some(copied);
        }
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

        if level == Level::Transcript {
            for index in 0..self.panes.len() {
                let target = self.panes[index].targeted_tool().map(str::to_string);
                let target_is_rendered = target.as_ref().is_none_or(|target| {
                    self.panes[index]
                        .thread()
                        .and_then(|thread| {
                            self.cockpit.thread(thread).map(|open| open.transcript())
                        })
                        .is_some_and(|transcript| {
                            pane::rendered_output_tools(transcript.blocks(), level)
                                .any(|tool| tool.call == *target)
                        })
                });
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

        if self.context_usage.is_some_and(|(thread, _)| {
            level != Level::Transcript
                || self.focused_thread() != Some(thread)
                || self.settings_open
                || self
                    .cockpit
                    .thread(thread)
                    .and_then(|open| open.transcript().usage())
                    .is_none()
        }) {
            self.context_usage = None;
        }

        // The popover belongs to the focused Pane's Composer at L1 (#23):
        // leaving that Pane, or zooming below L1, closes it here — and a
        // picker closes the moment its offer expires (#11, #25). The
        // import picker's Thread stopping being adoptable means a pick can
        // never delete a Thread that is no longer blank; the provider
        // picker's lock arming means nothing re-aims after the first
        // prompt, however it went out; a band chip (#29) goes with the
        // draft becoming a Thread. Render is the one chokepoint every
        // open, pick, dismissal and heal passes.
        if self.popover.as_ref().is_some_and(|open| {
            let focused = self.panes.get(self.focused());
            level != Level::Transcript
                || focused.is_none_or(|pane| pane.identity != open.pane)
                || match &open.kind {
                    Kind::ImportFile => open.pane.thread().is_some_and(|thread| {
                        !pane::offers_import(
                            self.cockpit.thread(thread).map(|live| live.transcript()),
                        )
                    }),
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
        for pane in &self.panes {
            let open = self
                .popover
                .as_ref()
                .is_some_and(|open| open.pane == pane.identity);
            pane.composer
                .update(cx, |composer, cx| composer.set_menu_open(open, cx));
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
        // lands on the one Pane actually on screen.
        let wanted = self
            .rename
            .as_ref()
            .map(|(_, editor)| editor.focus_handle(cx))
            .or_else(|| {
                self.panes.get(self.focused()).and_then(|pane| match level {
                    // At L1 the Composer keeps the keyboard even while a
                    // Decision pends: the card is part of its stack and the
                    // input stays live (PromptBox state 04) — y/n/a answer
                    // through the region's own Decision key context (#23).
                    Level::Transcript if pane.has_tool_target() => Some(pane.tool_focus()),
                    // An L2 cell draws a Composer too, and the keys go
                    // where the caret is.
                    Level::Transcript | Level::Instruments => Some(pane.composer.focus_handle(cx)),
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
        use gpui::component::WindowExt as _;
        let native_text_focused = gpui::base::TextSelection::has_selection(window, cx)
            && self
                .panes
                .get(self.focused())
                .is_some_and(|pane| pane.transcript_focus.contains_focused(window, cx));
        if window.has_active_dialog(cx) || native_text_focused {
            // Native text and dialogs keep their own keyboard focus.
        } else if self.settings_open {
            // The pump must not steal focus from Settings search or controls.
            if !self.settings_focus.contains_focused(window, cx) {
                window.focus(&self.settings_focus, cx);
            }
        } else if !self
            .popover
            .as_ref()
            .is_some_and(|open| open.kind.picker_slot().is_some())
            && !wanted.is_focused(window)
        {
            window.focus(&wanted, cx);
        }

        let frame = || {
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .gap(px(crate::theme::GRID_GAP))
                .p(px(crate::theme::GRID_PAD))
                // The titlebar band stays the window's own drag strip, and
                // the board keeps its own padding under it (`BOARD_TOP`).
                .pt(px(crate::theme::BOARD_TOP))
        };
        let visible = self.visible_indices();
        let layout = self.cockpit.layout();
        let grid = if let Some(index) = fullscreen {
            // The fullscreened Pane takes the whole window. The other Panes
            // are not laid out at all — hidden siblings would still cost
            // layout — while their Sessions keep streaming through the pump
            // regardless (#20).
            frame()
                .flex()
                .flex_col()
                .child(self.pane_cell(index, level, cx))
        } else if let Some((group, tree)) = match self.cockpit.roster().view() {
            View::Group(group) => self.group_tree(group).map(|tree| (group, tree)),
            View::Solo => None,
        } {
            // A Group's board is its split tree (SwarmDeck's mosaic): every
            // Pane at the rect the tree gives it, a grab band over every
            // seam, and — while a Pane is being dragged — the wash that
            // says what a release would do. Absolute geometry, so a seam
            // drag moves exactly the two sides it sits between.
            self.tree_board(group, tree, window, cx)
        } else {
            let columns = layout.columns;
            let mut grid = frame().flex().flex_col();
            for row in visible.chunks(columns) {
                let mut line = div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .gap(px(crate::theme::GRID_GAP));
                for index in row {
                    line = line.child(self.pane_cell(*index, level, cx));
                }
                for _ in row.len()..columns {
                    line = line.child(div().flex_1().min_w_0().min_h_0());
                }
                grid = grid.child(line);
            }
            grid
        };

        // Two full-height columns and nothing above them: there is no
        // title band at all, so the Pane board starts at y = 0 and its own
        // 10px padding is the only inset (§5 #1). The window's default face
        // is the system sans; the Pane opts into the bundled mono itself.
        div()
            .flex()
            .flex_row()
            // `relative`, so the titlebar strip below can lie over the band
            // the board reserves rather than take a row of its own.
            .relative()
            .size_full()
            .bg(rgb(crate::theme::GROUND))
            .font_family(crate::theme::FONT_UI)
            .track_focus(&self.focus)
            .key_context("Ferrite")
            // At wall range no Pane holds a Composer, so the answer keys are
            // not competing with typing: they answer whichever Thread is
            // flagged, without the operator focusing it first.
            .when(level == Level::Wall, |wall| {
                wall.key_context("Ferrite Wall")
            })
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
            .on_action(cx.listener(Self::band_cycle))
            .on_action(cx.listener(Self::tool_cycle_previous))
            .on_action(cx.listener(Self::toggle_tool_action))
            .on_action(cx.listener(Self::close_thread))
            .on_action(cx.listener(Self::reopen_thread))
            .on_action(cx.listener(Self::copy_selection))
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
            .on_action(cx.listener(Self::toggle_fullscreen))
            .on_action(cx.listener(Self::toggle_nav))
            .on_action(cx.listener(Self::menu_next))
            .on_action(cx.listener(Self::menu_previous))
            .on_action(cx.listener(Self::history_older))
            .on_action(cx.listener(Self::history_newer))
            .on_action(cx.listener(Self::menu_pick))
            .on_action(cx.listener(Self::menu_dismiss))
            // The root covers the window, so a release anywhere ends the
            // drag; the selection it made stays until the next press. Moves
            // ride the root too (#27): a sweep keeps extending after the
            // pointer leaves the Pane div, clamped to the origin transcript.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _: &MouseUpEvent, _, cx| {
                    view.end_seam_drag(cx);
                    if view.drop_preview.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, window, cx| {
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
                    if view.nav_filter_open {
                        view.nav_filter_open = false;
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
            .child(self.nav(cx))
            .child(grid)
            // The window's own titlebar, where the platform makes the app
            // draw one. It is the last thing over the band and the first
            // thing under a menu: an overlay that reached into the band
            // would be answering the frame's hit test, not its own rows, so
            // the drag region stands down while one is open.
            .when(crate::titlebar::CUSTOM, |root| {
                root.child(crate::titlebar::strip(
                    self.nav_width(),
                    !self.overlay_open(),
                    self.maximized,
                ))
            })
            .children(self.context_menu_element(cx))
            .children(self.context_usage_element(cx))
            .children(self.settings_element(cx))
            .children(gpui::component::Root::render_dialog_layer(window, cx))
    }
}

impl CockpitView {
    /// One Pane's cell — the click-to-focus and drag plumbing around
    /// `render_pane`. The same cell serves a grid slot and the fullscreen
    /// view; only who lays it out differs.
    fn pane_cell(&self, index: usize, level: Level, cx: &mut Context<Self>) -> Div {
        let pane = &self.panes[index];
        let focused = index == self.focused();
        let cell = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                    view.pointer_down(index, event, window, cx)
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                    let Some(thread) = view.panes.get(index).and_then(PaneView::thread) else {
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
        // A Thread Pane is a drop target for another Pane of its Group:
        // the moves over it compute the preview, the release applies it.
        let cell = match pane.thread() {
            Some(target) => cell
                .on_drag_move(cx.listener(
                    move |view, event: &gpui::DragMoveEvent<PaneDrag>, _, cx| {
                        // gpui hands every drag move to every listener, not
                        // just the one under the pointer: a Pane only
                        // previews while the pointer is inside it, and
                        // drops its own preview once the pointer has left
                        // — else the first Pane crossed keeps saying
                        // "split left" while the drag is somewhere else.
                        let source = event.drag(cx).thread;
                        if event.bounds.contains(&event.event.position) {
                            view.preview_pane_drop(
                                source,
                                target,
                                event.event.position,
                                event.bounds,
                                cx,
                            );
                        } else if view
                            .drop_preview
                            .is_some_and(|(previewed, _)| previewed == target)
                        {
                            view.drop_preview = None;
                            cx.notify();
                        }
                    },
                ))
                .on_drop(cx.listener(move |view, drag: &PaneDrag, _, cx| {
                    view.drop_pane(drag.thread, target, cx);
                })),
            None => cell,
        };
        // A draft Pane (#29): the band and its popover instead of a
        // transcript — nothing in core exists to read yet.
        let Some(thread) = pane.thread() else {
            let draft = pane.draft().expect("a Pane is a Thread or a draft");
            return cell.child(pane::render_draft(
                pane,
                pane::DraftState {
                    band: self.draft_band_element(index, cx),
                    picker: self.draft_model_picker(index, cx),
                    menu: (level == Level::Transcript)
                        .then(|| self.popover_element(index, cx))
                        .flatten(),
                    composer_empty: pane.composer.read(cx).is_empty(),
                    focused,
                    error: draft.error.as_ref(),
                },
                level,
            ));
        };
        let open = self.cockpit.thread(thread);
        // The frame's selection seam for this Pane (#27), resolved against
        // exactly the rows the body will draw — the shared rendered window,
        // because copy is what you see.
        let selection = {
            let blocks = open.map(|open| open.transcript().blocks()).unwrap_or(&[]);
            self.selection
                .overlay(thread, pane::rendered_window(blocks, level))
        };
        let cached = self.facts.get(thread);
        let facts = pane::PaneFacts {
            thread: open,
            // The cached checkout label (#29) — display-only.
            branch: cached.and_then(|facts| facts.branch.clone()),
            composer_empty: pane.composer.read(cx).is_empty(),
            history_available: self.history_available(index, level),
            focused,
            wall: cached.map(|facts| &facts.wall),
            selection,
        };
        // Only L1 draws a Composer to hang a popover over (#23), a model
        // picker (#25) or usage meter; the wall answers with keys alone.
        let l1 = level == Level::Transcript;
        let wiring = pane::PaneWiring {
            menu: l1.then(|| self.popover_element(index, cx)).flatten(),
            model_picker: l1.then(|| self.model_picker(index, cx)).flatten(),
            usage_meter: l1.then(|| self.usage_meter(index, cx)).flatten(),
            decide: (level != Level::Wall)
                .then(|| self.decide_keycaps(index, level, cx))
                .flatten(),
            tool_controls: self.tool_disclosures(index, thread, level, cx),
            // The title is the Pane's handle at every size: a drag moves a
            // grouped Pane, a double-click renames it — an L2 cell with no
            // handle could not be rearranged at all.
            title: Some(self.pane_title(index, thread, cx)),
            question_form: l1.then(|| self.question_form(index, cx)).flatten(),
        };
        cell.child(pane::render_pane(pane, facts, wiring, level))
    }

    fn tool_disclosures(
        &self,
        index: usize,
        thread: ThreadId,
        level: Level,
        cx: &mut Context<Self>,
    ) -> std::collections::HashMap<String, AnyElement> {
        if level != Level::Transcript {
            return std::collections::HashMap::new();
        }
        let pane = &self.panes[index];
        let Some(open) = self.cockpit.thread(thread) else {
            return std::collections::HashMap::new();
        };
        let transcript = open.transcript();
        let mut controls = std::collections::HashMap::new();
        for tool in pane::rendered_output_tools(transcript.blocks(), level) {
            let call = tool.call.clone();
            let wired = pane::tool_disclosure_control(
                &call,
                pane.tool_state(&call) == pane::DisclosureState::Expanded,
                pane.tool_targeted(&call),
                &pane.tool_focus(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let call = call.clone();
                    move |view, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        view.toggle_tool(thread, &call, window, cx);
                    }
                }),
            );
            #[cfg(test)]
            let wired = {
                let sink = pane.tool_bounds_sink();
                let measured = call.clone();
                div()
                    .child(wired)
                    .on_children_prepainted(move |bounds, _, _| {
                        if let Some(bounds) = bounds.first() {
                            sink.borrow_mut().insert(measured.clone(), *bounds);
                        }
                    })
            };
            controls.insert(call, wired.into_any_element());
        }
        controls
    }

    fn toggle_tool(
        &mut self,
        thread: ThreadId,
        call: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        self.focus_pane(index);
        gpui::base::TextSelection::clear(window, cx);
        self.panes[index].toggle_tool(call);
        window.focus(&self.panes[index].tool_focus(), cx);
        cx.notify();
    }

    /// The pending Decision of `thread` when it is a question — parsed
    /// fresh, which is cheap: a Decision's input is a few hundred bytes.
    fn pending_questions(
        &self,
        thread: ThreadId,
    ) -> Option<(String, Vec<ferrite_core::questions::Question>)> {
        let decision = self.cockpit.thread(thread)?.pending()?;
        let questions = pane::question_of(decision)?;
        Some((decision.id.clone(), questions))
    }

    /// Every open Thread's draft follows its pending Decision: a new
    /// question gets a clean draft, an answered or vanished one loses it.
    fn sync_question_drafts(&mut self) {
        let threads: Vec<ThreadId> = self.cockpit.threads();
        for thread in threads {
            match self.pending_questions(thread) {
                Some((decision, questions)) => {
                    let stale = self
                        .questions
                        .get(&thread)
                        .is_none_or(|draft| draft.decision != decision);
                    if stale {
                        let answers = vec![Default::default(); questions.len()];
                        self.questions.insert(
                            thread,
                            QuestionDraft {
                                decision,
                                questions,
                                answers,
                            },
                        );
                    }
                }
                None => {
                    self.questions.remove(&thread);
                }
            }
        }
        self.questions
            .retain(|thread, _| self.cockpit.thread(*thread).is_some());
    }

    /// A press on an option: a pick-any question toggles it, a pick-one
    /// question moves its mark there.
    fn pick_option(&mut self, thread: ThreadId, question: usize, option: usize) {
        self.sync_question_drafts();
        let Some(draft) = self.questions.get_mut(&thread) else {
            return;
        };
        let (Some(asked), Some(answer)) = (
            draft.questions.get(question),
            draft.answers.get_mut(question),
        ) else {
            return;
        };
        if option >= asked.options.len() {
            return;
        }
        if asked.multi_select {
            match answer.picks.iter().position(|pick| *pick == option) {
                Some(at) => {
                    answer.picks.remove(at);
                }
                None => answer.picks.push(option),
            }
        } else {
            answer.picks = vec![option];
        }
    }

    /// The number keys 1–4 pick on the question the operator is up to —
    /// the first one still unanswered, else the last. With text on the
    /// Composer line they are digits, the y/n/a rule.
    fn pick_or_type(
        &mut self,
        option: usize,
        digit: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        if self.pending_questions(thread).is_none() {
            if let Some(pane) = self.panes.get(self.focused()) {
                pane.composer
                    .clone()
                    .update(cx, |composer, cx| composer.insert(digit, cx));
            }
            return;
        }
        if self.level_now(window) == Level::Transcript {
            if let Some(pane) = self.panes.get(self.focused()) {
                if !pane.composer.read(cx).is_empty() {
                    pane.composer
                        .clone()
                        .update(cx, |composer, cx| composer.insert(digit, cx));
                    return;
                }
            }
        }
        self.sync_question_drafts();
        let Some(draft) = self.questions.get(&thread) else {
            return;
        };
        let question = draft
            .answers
            .iter()
            .position(|answer| answer.picks.is_empty())
            .unwrap_or(draft.questions.len().saturating_sub(1));
        self.pick_option(thread, question, option);
        cx.notify();
    }

    /// Send the form: the picks, plus `other` — the typed line — on the
    /// first question without a pick (else the last). Nothing picked and
    /// nothing typed sends nothing; the question stays up. The answer is
    /// the tool's own allow with the answers folded into its input, which
    /// is exactly how Claude Code's own UI answers it.
    fn submit_questions(
        &mut self,
        thread: ThreadId,
        other: Option<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sync_question_drafts();
        let Some(decision) = self
            .cockpit
            .thread(thread)
            .and_then(|open| open.pending())
            .cloned()
        else {
            return false;
        };
        let Some(draft) = self.questions.get_mut(&thread) else {
            return false;
        };
        if let Some(other) = other {
            let at = draft
                .answers
                .iter()
                .position(|answer| answer.picks.is_empty())
                .unwrap_or(draft.questions.len().saturating_sub(1));
            if let Some(answer) = draft.answers.get_mut(at) {
                answer.other = Some(other);
            }
        }
        let answered = draft.answers.iter().any(|answer| {
            !answer.picks.is_empty() || answer.other.as_deref().is_some_and(|o| !o.is_empty())
        });
        if !answered {
            return false;
        }
        let input = ferrite_core::questions::answered_input(
            &decision.input,
            &draft.answers,
            &draft.questions,
        );
        self.questions.remove(&thread);
        self.cockpit
            .respond(thread, &decision, DecisionAnswer::Allow { input });
        self.facts.acted(&self.cockpit, thread);
        cx.notify();
        true
    }

    /// The question card (L1): one block per question, its options wired
    /// to their picks, and the keys — ↵ sends once something is picked or
    /// typed, n denies.
    fn question_form(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.panes[index].thread()?;
        let (decision_id, questions) = self.pending_questions(thread)?;
        let draft = self
            .questions
            .get(&thread)
            .filter(|draft| draft.decision == decision_id);
        let mut card = pane::question_card();
        for (qi, question) in questions.iter().enumerate() {
            let picks: &[usize] = draft
                .and_then(|draft| draft.answers.get(qi))
                .map(|answer| answer.picks.as_slice())
                .unwrap_or(&[]);
            card = card.child(pane::question_head(
                SharedString::from(question.header.clone()),
                SharedString::from(question.question.clone()),
                question.multi_select,
            ));
            for (oi, option) in question.options.iter().enumerate() {
                let row = pane::question_option(
                    ("question-option", qi * 8 + oi),
                    oi + 1,
                    SharedString::from(option.label.clone()),
                    SharedString::from(option.description.clone()),
                    picks.contains(&oi),
                    question.multi_select,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if let Some(index) = view.pane_for(thread) {
                            view.focus_pane(index);
                        }
                        view.pick_option(thread, qi, oi);
                        cx.notify();
                    }),
                );
                card = card.child(row);
            }
        }
        let answered =
            draft.is_some_and(|draft| draft.answers.iter().any(|answer| !answer.picks.is_empty()));
        let typed = !self.panes[index].composer.read(cx).is_empty();
        let hint = SharedString::from(if answered || typed {
            "↵ sends · type below to answer in your own words"
        } else {
            "1–4 or click to pick · or type an answer below and press ↵"
        });
        let wire = |keycap: Stateful<Div>, answer: Answer, cx: &mut Context<Self>| {
            keycap.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if let Some(index) = view.pane_for(thread) {
                        view.focus_pane(index);
                    }
                    view.answer(answer, cx);
                }),
            )
        };
        let mut keys = pane::decide_row(Level::Transcript);
        if answered || typed {
            keys = keys.child(pane::keycap_send().on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if let Some(index) = view.pane_for(thread) {
                        view.focus_pane(index);
                    }
                    view.submit(&Submit, window, cx);
                }),
            ));
        }
        keys = keys.child(wire(pane::keycap_deny(), Answer::Deny, cx));
        card = card.child(pane::question_footer(hint, Some(keys.into_any_element())));
        Some(card.into_any_element())
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
        let form = ferrite_core::titler::form(
            provider,
            &program,
            &ferrite_core::titler::TitleRequest {
                prompt,
                reply: None,
            },
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

    /// The pending Decision's keycaps (#26), each press wired to the exact
    /// decide verb its key runs — no new semantics, the mouse presses the
    /// keycap it depicts. Assembled here like `pane_controls`; presses land
    /// on the clicked Pane first (the keyboard may be elsewhere) and stop
    /// propagation so the Pane's own press handler cannot re-target them.
    /// L1 draws y/n and, where the request offered a standing answer, a;
    /// the L2 card keeps y/n alone.
    fn decide_keycaps(
        &self,
        index: usize,
        level: Level,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let thread = self.panes[index].thread()?;
        let decision = self.cockpit.thread(thread)?.pending()?;
        let offers_always = level == Level::Transcript && decision.standing_answer().is_some();
        let wire = |keycap: Stateful<Div>, answer: Answer, cx: &mut Context<Self>| {
            keycap.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if let Some(index) = view.pane_for(thread) {
                        view.focus_pane(index);
                    }
                    view.answer(answer, cx);
                }),
            )
        };
        let mut cluster = pane::decide_row(level)
            .child(wire(pane::keycap_allow(), Answer::Allow, cx))
            .child(wire(pane::keycap_deny(), Answer::Deny, cx));
        if offers_always {
            cluster = cluster.child(wire(pane::keycap_always(), Answer::Always, cx));
        }
        Some(cluster.into_any_element())
    }

    /// The Composer meter opens the latest reported usage on click.
    /// No reading is invented when the provider has not reported usage.
    fn usage_meter(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.panes[index].thread()?;
        let usage = self.cockpit.thread(thread)?.transcript().usage()?;
        let fraction = usage
            .context_window
            .filter(|window| *window > 0)
            .map_or(0., |window| usage.total_tokens as f32 / window as f32);
        // Account-wide and remembered across launches, so the meter is
        // not blank until this Thread's first turn happens to report.
        let limits = self
            .cockpit
            .account_limits(self.cockpit.thread(thread)?.provider());
        let was_open = self.context_usage.is_some_and(|(shown, _)| shown == thread);
        Some(
            div()
                .id(("usage-meter", thread.get() as usize))
                .debug_selector(move || format!("usage-meter-{}", thread.get()))
                .rounded(px(crate::theme::R_CHIP))
                .child(pane::usage_lines(fraction, limits))
                .hover_raised()
                .press_raised()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.focus_pane(index);
                        view.popover = None;
                        view.context_menu = None;
                        // Outside-click dismissal runs in capture phase, before this
                        // toggle. Use the state of the meter that received the press.
                        view.context_usage = (!was_open)
                            .then_some((thread, event.position - gpui::point(px(0.), px(12.))));
                        cx.notify();
                    }),
                )
                .into_any_element(),
        )
    }

    fn context_usage_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (thread, at) = self.context_usage?;
        let usage = self.cockpit.thread(thread)?.transcript().usage()?;
        let card = menu::shell()
            .id("context-usage-card")
            .debug_selector(|| "context-usage-card".into())
            .child(pane::context_usage(
                usage,
                self.cockpit
                    .account_limits(self.cockpit.thread(thread)?.provider()),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .on_mouse_down_out(cx.listener(|view, _: &MouseDownEvent, _, cx| {
                if view.context_usage.take().is_some() {
                    cx.notify();
                }
            }));
        Some(
            deferred(
                anchored()
                    .anchor(gpui::Anchor::BottomLeft)
                    .position(at)
                    .snap_to_window_with_margin(px(crate::theme::GRID_PAD))
                    .child(card),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// The Composer's model picker (#25): the provider logomark, the bare
    /// model name, and a chevron — on **every** L1 Pane, not only pre-lock.
    /// Its click still opens the provider picker, which is what the old
    /// provider chip was for; a locked Thread's picker refuses the swap
    /// itself rather than the control vanishing.
    fn model_picker(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.panes[index].thread()?;
        let open = self.cockpit.thread(thread)?;
        let provider = open.provider();
        // The standing choice names the chip; else what the Session's own
        // Init said is serving; until either, the Provider's own name —
        // and always the name a person says, never the id on the wire.
        let label = match open.model().or_else(|| open.transcript().model()) {
            Some(model) => {
                SharedString::from(ferrite_core::providers::models::label(model, open.models()))
            }
            None => SharedString::from(provider_title(provider)),
        };
        let model_chip = self.choice_menu(
            index,
            Kind::Provider,
            crate::components::button(("model-picker", thread.get() as usize))
                .p_0()
                .h_auto()
                .child(pane::model_picker(Some(provider), label)),
            cx,
        );
        // The effort chip beside it — only when the model takes one; a
        // model with no ladder (haiku) draws no chip rather than a dead one.
        let ladder =
            ferrite_core::providers::models::efforts_for(provider, open.model(), open.models());
        let effort_chip = (!ladder.is_empty()).then(|| {
            let label = match open.effort() {
                Some(effort) => SharedString::from(effort_title(effort)),
                None => match self.prefs.settings.effort_for(provider) {
                    Some(effort) => SharedString::from(effort_title(effort)),
                    None => SharedString::from("effort"),
                },
            };
            self.choice_menu(
                index,
                Kind::Effort,
                crate::components::button(("effort-picker", thread.get() as usize))
                    .p_0()
                    .h_auto()
                    .child(pane::effort_picker(label)),
                cx,
            )
        });
        Some(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(crate::theme::KEYS_GAP))
                .child(model_chip)
                .children(effort_chip)
                .into_any_element(),
        )
    }

    fn choice_menu(
        &self,
        index: usize,
        kind: Kind,
        trigger: gpui::component::button::Button,
        cx: &mut Context<Self>,
    ) -> crate::components::ChoiceMenu {
        let identity = self.panes[index].identity;
        let slot = kind.picker_slot().expect("native choice menu");
        let (effort, band) = slot;
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
                        crate::components::Choice {
                            label: row.name.clone(),
                            checked: row.active,
                            disabled: row.inert,
                            section,
                            icon: provider.map(|provider| match provider {
                                Provider::Claude => {
                                    (crate::icons::CLAUDE, crate::theme::PROVIDER_CLAUDE)
                                }
                                Provider::Codex => {
                                    (crate::icons::CODEX, crate::theme::PROVIDER_CODEX)
                                }
                            }),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let weak = cx.entity().downgrade();
        let picker = weak.clone();
        crate::components::ChoiceMenu {
            id: format!("choice-{identity:?}-{effort}").into(),
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
            on_pick: std::rc::Rc::new(move |at, _, cx| {
                let _ = picker.update(cx, |view, cx| view.pick(at, cx));
            }),
        }
    }

    /// The effort chip's click: the root chip's toggle grammar.

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
                    None => "the CLI's own choice".to_string(),
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
        self.popover = Some(Popover {
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
            self.cockpit.apply_input(
                thread,
                ferrite_core::transcript::Input::Notice(format!("effort unchanged: {e}")),
            );
        }
        self.facts.acted(&self.cockpit, thread);
        cx.notify();
    }

    /// The chip's click: close an open provider picker on this Thread, or
    /// open one — the root chip's toggle grammar.

    /// The whole nav column for this frame, rows wired to their Threads
    /// (#21). It paints inside the cockpit's own render — same entity, same
    /// pump, no second timer — and every fact it shows came from
    /// `nav_state`'s O(1) reads or the project/branch/parked caches.
    fn nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.nav_state();
        let gear = prefs::gear_button().on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.toggle_settings(cx);
        }));
        let mut chrome =
            nav::win_chrome(state.collapsed).child(nav::collapse_button().on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.set_nav_collapsed(!view.nav_collapsed, cx);
                }),
            ));
        // The gear sits hard right of the band; folded, it stacks under
        // the collapse button. The stretch between the two is the window's
        // where the app draws its own titlebar: the band reads as a
        // titlebar, so it drags like one (`titlebar.rs`).
        if !state.collapsed {
            chrome = chrome.child(if crate::titlebar::CUSTOM {
                crate::titlebar::drag_region("nav-chrome-drag", self.maximized)
            } else {
                div().flex_1()
            });
        }
        chrome = chrome.child(gear);
        let content = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .h_full()
            .w(px(if state.collapsed {
                nav::RAIL_WIDTH
            } else {
                nav::WIDTH
            }))
            .child(chrome);
        let content = if state.collapsed {
            content.child(self.rail(&state, cx))
        } else {
            content.child(self.nav_head(&state, cx)).child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(self.nav_tree(&state, cx))
                    .child(nav::scrollbar(&self.nav_scroll)),
            )
        };
        if !self.nav_has_toggled {
            return nav::shell(state.collapsed)
                .child(content)
                .into_any_element();
        }
        let (from, to, duration) = if state.collapsed {
            (nav::WIDTH, nav::RAIL_WIDTH, NAV_CLOSE_MS)
        } else {
            (nav::RAIL_WIDTH, nav::WIDTH, NAV_OPEN_MS)
        };
        let content = content.with_animation(
            ("nav-content", usize::from(state.collapsed)),
            Animation::new(Duration::from_millis(duration)).with_easing(ease_out_quint()),
            |content, delta| content.opacity(0.35 + 0.65 * delta),
        );
        nav::shell(state.collapsed)
            .child(content)
            .with_animation(
                ("nav-resize", usize::from(state.collapsed)),
                Animation::new(Duration::from_millis(duration)).with_easing(ease_out_quint()),
                move |column, delta| column.w(px(from + (to - from) * delta)),
            )
            .into_any_element()
    }

    /// The 42px head: the one Project dropdown, and its menu when it is
    /// down. The menu is `deferred` — it is absolutely positioned inside
    /// the head, and the scrolling tree is a later sibling that would
    /// otherwise paint straight over it.
    fn nav_head(&self, state: &nav::NavState, cx: &mut Context<Self>) -> Div {
        let head = nav::nav_head().child(nav::filter_trigger(&state.filter).on_mouse_down(
            MouseButton::Left,
            cx.listener(|view, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                view.nav_filter_open = !view.nav_filter_open;
                cx.notify();
            }),
        ));
        if !state.filter.open {
            return head;
        }
        let mut menu = nav::filter_menu();
        for (index, option) in state.filter.options.iter().enumerate() {
            let project = option.project;
            menu = menu.child(
                nav::filter_option(index, option)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            // The filter narrows navigation and nothing else: no
                            // Pane opens, closes or moves because of it.
                            view.nav_filter = project;
                            view.nav_filter_open = false;
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            if let Some(project) = project {
                                view.open_context_menu(
                                    MenuTarget::Project(project),
                                    event.position,
                                    cx,
                                );
                            }
                        }),
                    ),
            );
        }
        let count = state.filter.options.len();
        menu = menu.child(nav::filter_action(count, "Add Project…").on_mouse_down(
            MouseButton::Left,
            cx.listener(|view, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                view.nav_filter_open = false;
                view.browse_for_project(BrowseThen::Filter, cx);
                cx.notify();
            }),
        ));
        head.child(deferred(menu))
    }

    /// The scrolling tree: Groups with their members and solo Threads in
    /// one order, most recently used first. The 16px band above a Group
    /// block is also its `GroupGap` drop zone, so reordering costs no
    /// layout of its own, and every run of solo rows is a `LooseZone` — a
    /// place to drop a row to get it out of its Group.
    fn nav_tree(&self, state: &nav::NavState, cx: &mut Context<Self>) -> Stateful<Div> {
        // Every drag started from this frame started from this View.
        let origin = self.cockpit.roster().view();
        let mut tree = nav::nav_tree(&self.nav_scroll);
        if let Some(error) = &self.group_error {
            tree = tree.child(
                div()
                    .px(px(crate::theme::ROW_PAD_X))
                    .py(px(crate::theme::ROW_PAD_Y))
                    .rounded(px(crate::theme::R_CONTROL))
                    .text_size(px(crate::theme::FS_SM))
                    .text_color(rgb(crate::theme::ATTENTION))
                    .bg(rgba(crate::theme::ATTENTION_WASH))
                    .child(error.clone()),
            );
        }
        // Solo rows the order puts next to each other are drawn as one run,
        // so the 2px between siblings stays a container's gap rather than a
        // margin on every row.
        let mut zones = 0usize;
        let mut run: Vec<AnyElement> = Vec::new();
        let mut after_group = false;
        for (position, item) in state.order.iter().enumerate() {
            match item {
                nav::NavItem::Solo(index) => {
                    run.push(self.thread_element(&state.solos[*index], None, cx));
                }
                nav::NavItem::Group(index) => {
                    if !run.is_empty() {
                        tree = tree.child(self.loose_run(
                            zones,
                            std::mem::take(&mut run),
                            after_group,
                            cx,
                        ));
                        zones += 1;
                        after_group = false;
                    }
                    tree = tree.child(self.group_element(
                        state,
                        *index,
                        position == 0,
                        after_group,
                        origin,
                        cx,
                    ));
                    after_group = true;
                }
            }
        }
        if !run.is_empty() {
            tree = tree.child(self.loose_run(zones, run, after_group, cx));
            zones += 1;
            after_group = false;
        }
        // The ground under the last row is a drop target too — with every
        // Thread in a Group it is the only place left to drop one to get it
        // out — and it takes the tree's slack, so that ground is never dead.
        let ground = nav::loose_ground(zones)
            .when(after_group, |ground| ground.mt(px(crate::theme::SOLOS_TOP)));
        tree = tree.child(
            drop_feedback(ground, self.cockpit.groups().clone(), DropTarget::LooseZone).on_drop(
                cx.listener(|view, drag: &NavDrag, _, cx| {
                    view.apply_drop(*drag, DropTarget::LooseZone, cx)
                }),
            ),
        );
        if state.order.is_empty() {
            tree = tree.child(nav::empty_filter(&state.filter.label));
        }
        tree
    }

    /// One run of solo rows, which is also a `LooseZone`. `after_group` is
    /// what separates it from the Group above it; a run that opens the tree
    /// starts at the tree's own padding.
    fn loose_run(
        &self,
        zone: usize,
        rows: Vec<AnyElement>,
        after_group: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let run =
            nav::solos(zone, rows).when(after_group, |run| run.mt(px(crate::theme::SOLOS_TOP)));
        drop_feedback(run, self.cockpit.groups().clone(), DropTarget::LooseZone)
            .on_drop(cx.listener(|view, drag: &NavDrag, _, cx| {
                view.apply_drop(*drag, DropTarget::LooseZone, cx)
            }))
            .into_any_element()
    }

    /// One Group block: the 16px band above it when another Group precedes
    /// it — the "insert between these two" drop target — then the header,
    /// which is the drag handle, the rename target and the way in, then the
    /// member rows.
    fn group_element(
        &self,
        state: &nav::NavState,
        index: usize,
        first_in_tree: bool,
        after_group: bool,
        origin: View,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let group = &state.groups[index];
        let id = group.id;
        let full_index = self
            .cockpit
            .groups()
            .iter()
            .position(|group| group.id == id)
            .expect("navigation only shows existing Groups");
        let gap = DropTarget::GroupGap(full_index);
        let head = nav::group_row_with_title(
            group,
            self.editable_group_title(id, group.title.clone(), cx),
        );
        let mut block = nav::group_block()
            // A Group separates itself from whatever is above it: nothing
            // when it opens the tree, the 16px band from another Group —
            // prepended below, because that band is a drop target and not a
            // margin — and the solos' own 24px from a run of rows.
            .when(!first_in_tree && !after_group, |block| {
                block.mt(px(crate::theme::SOLOS_TOP))
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
                    move |_, _, _, cx| cx.new(|_| NavDragPreview("group".into())),
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
        // The first block in the tree has no band above it to drop into, so
        // "insert above this Group" rides its header instead.
        if first_in_tree {
            block = block.child(
                drop_feedback(
                    nav::group_gap_lead(index),
                    self.cockpit.groups().clone(),
                    gap,
                )
                .on_drop(
                    cx.listener(move |view, drag: &NavDrag, _, cx| view.apply_drop(*drag, gap, cx)),
                ),
            );
        }
        if !group.members.is_empty() {
            let rows = group
                .members
                .iter()
                .map(|row| self.thread_element(row, Some(id), cx))
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
                        .expect("Group exists")
                        .members
                        .len(),
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
        if !after_group {
            return block.into_any_element();
        }
        // Two Groups in a row: the band between them, drawn above this one.
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(
                drop_feedback(nav::group_gap(index), self.cockpit.groups().clone(), gap).on_drop(
                    cx.listener(move |view, drag: &NavDrag, _, cx| view.apply_drop(*drag, gap, cx)),
                ),
            )
            .child(block)
            .into_any_element()
    }

    /// One Thread row, with the whole drag/drop and click wiring a row has
    /// had since #21 — retargeted from the old running/parked pair onto the
    /// one row shape. A row whose Pane is open focuses it; a parked one
    /// revives, into the Group it was drawn under.
    fn thread_element(
        &self,
        row: &nav::ThreadRow,
        group: Option<GroupId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let thread = row.thread;
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
        let head = nav::thread_row_with_title(
            row,
            self.editable_thread_title(thread, row.name.clone(), cx),
        );
        let badge = self.facts.name(thread);
        drop_feedback(head, self.cockpit.groups().clone(), target)
            .on_drag(
                NavDrag {
                    drag: Drag::Thread { thread, group },
                    origin,
                },
                move |_, _, _, cx| {
                    let badge = badge.clone();
                    cx.new(|_| NavDragPreview(badge))
                },
            )
            .on_drop(
                cx.listener(move |view, drag: &NavDrag, _, cx| view.apply_drop(*drag, target, cx)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    if open {
                        view.focus_thread(thread, cx);
                        return;
                    }
                    view.revive_thread(thread, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_context_menu(MenuTarget::Thread(thread), event.position, cx);
                }),
            )
            .into_any_element()
    }

    /// The 56px rail cmd-b folds the column to: the filter button, then one
    /// logomark per Thread in the same order the tree draws them. The
    /// filter button unfolds the column and drops the menu — there is one
    /// dropdown, and this is how a 56px column reaches it.
    fn rail(&self, state: &nav::NavState, cx: &mut Context<Self>) -> Div {
        let mut items = nav::rail_items();
        for row in state.ordered_rows() {
            let current = row.current;
            let thread = row.thread;
            let open = self.pane_for(thread).is_some();
            items = items.child(nav::rail_item(row, current).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    if open {
                        view.focus_thread(thread, cx);
                    } else {
                        view.revive_thread(thread, cx);
                    }
                }),
            ));
        }
        nav::rail(self.nav_filter.is_some())
            .child(nav::rail_filter(self.nav_filter.is_some()).on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.set_nav_collapsed(false, cx);
                    view.nav_filter_open = true;
                    cx.notify();
                }),
            ))
            .child(items)
    }
}

/// Where Ferrite was started: the launch project every draft begins on.
pub(crate) fn here() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| ".".into())
}

/// A typed path with `~` spelled out — the type-a-path row accepts what an
/// operator would type at a shell.
/// A draft's stand-in leaf in a Group's tree: drafts are no Threads and
/// never persist, so the high ids stand in for them for one frame.
fn draft_leaf(draft: ferrite_core::roster::DraftId) -> ThreadId {
    ThreadId::new(u64::MAX - draft.get())
}

fn leaf_identity(leaf: ThreadId) -> PaneIdentity {
    if leaf.get() > u64::MAX / 2 {
        PaneIdentity::Draft(ferrite_core::roster::DraftId::new(u64::MAX - leaf.get()))
    } else {
        PaneIdentity::Thread(leaf)
    }
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

fn expand_home(typed: &str) -> std::path::PathBuf {
    if let Some(rest) = typed.strip_prefix("~/") {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".into());
        return std::path::PathBuf::from(home).join(rest);
    }
    std::path::PathBuf::from(typed)
}

/// This process's resident memory, for the perf print.
fn rss_mb() -> f64 {
    crate::session::rss_bytes(std::process::id())
        .map(|bytes| bytes as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0)
}

/// A transcript as plain text, the way the operator would paste it
/// somewhere else: prompts quoted, prose flat, code fenced, tool rows as
/// their one line plus the result line. Thinking stays out — it is not the
/// conversation.
fn transcript_text(blocks: &[ferrite_core::transcript::Block]) -> String {
    use ferrite_core::transcript::Body;
    let flat = |spans: &[ferrite_core::transcript::Span]| -> String {
        spans.iter().map(|span| span.text.as_str()).collect()
    };
    let mut out = String::new();
    for block in blocks {
        let piece = match &block.body {
            Body::Prompt(text) => format!("> {text}"),
            Body::Paragraph { spans } => flat(spans),
            Body::Heading { spans, .. } => format!("## {}", flat(spans)),
            Body::Bullet { spans } => format!("• {}", flat(spans)),
            Body::Code {
                language, source, ..
            } => format!(
                "```{}\n{source}\n```",
                language.as_deref().unwrap_or_default()
            ),
            Body::Tool(tool) => {
                let mut line = format!("● {}", tool.name);
                if !tool.summary.is_empty() {
                    line.push_str(&format!(" ({})", tool.summary));
                }
                if let Some(result) = &tool.result_line {
                    line.push_str(&format!("\n  ⎿ {result}"));
                }
                line
            }
            Body::Thinking(_) => continue,
            Body::Notice(text) | Body::Meta(text) => text.clone(),
        };
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&piece);
    }
    out
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
    use ferrite_core::transcript::Body;
    use ferrite_core::workspace::WorkspaceBinding;
    use ferrite_core::{Decision, SessionEvent};
    use gpui::{KeyBinding, TestAppContext};

    struct Scripted {
        rx: Receiver<SessionEvent>,
        fail_send: Rc<RefCell<bool>>,
        sent: Rc<RefCell<Vec<String>>>,
        answered: Rc<RefCell<Vec<(String, DecisionAnswer)>>>,
    }

    impl Session for Scripted {
        fn set_effort(&mut self, _effort: Option<&str>) -> std::io::Result<()> {
            Ok(())
        }
        fn events(&self) -> &Receiver<SessionEvent> {
            &self.rx
        }
        fn send(&mut self, text: &str) -> std::io::Result<()> {
            if *self.fail_send.borrow() {
                return Err(std::io::Error::other("stub refused first prompt"));
            }
            self.sent.borrow_mut().push(text.to_string());
            Ok(())
        }
        fn interrupt(&mut self) -> std::io::Result<()> {
            Ok(())
        }
        fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> std::io::Result<()> {
            self.answered.borrow_mut().push((id.to_string(), answer));
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct Fake {
        model_discovery: Rc<RefCell<Option<Receiver<(Provider, Vec<ferrite_core::ModelInfo>)>>>>,
        streams: Rc<RefCell<Vec<Sender<SessionEvent>>>>,
        /// Every spawn's choice, in call order — what the provider-picker
        /// tests read back (#25).
        spawned: Rc<RefCell<Vec<ProviderChoice>>>,
        /// While set, spawn refuses — how a test fails a bootstrap (#29).
        fail: Rc<RefCell<bool>>,
        fail_send: Rc<RefCell<bool>>,
        sent: Rc<RefCell<Vec<String>>>,
        /// Every Decision answer that went out, with the Decision's id.
        answered: Rc<RefCell<Vec<(String, DecisionAnswer)>>>,
    }

    impl Spawner for Fake {
        fn discover_models(
            &mut self,
        ) -> Option<Receiver<(Provider, Vec<ferrite_core::ModelInfo>)>> {
            self.model_discovery.borrow_mut().take()
        }

        fn spawn(
            &mut self,
            request: ferrite_core::cockpit::SpawnRequest,
        ) -> std::io::Result<Box<dyn Session>> {
            if *self.fail.borrow() {
                return Err(std::io::Error::other("stub refused to spawn"));
            }
            let (tx, rx) = mpsc::channel();
            self.streams.borrow_mut().push(tx);
            self.spawned.borrow_mut().push(ProviderChoice {
                provider: request.provider,
                model: request.model.map(|model| model.to_string()),
            });
            Ok(Box::new(Scripted {
                rx,
                fail_send: self.fail_send.clone(),
                sent: self.sent.clone(),
                answered: self.answered.clone(),
            }))
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-view-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// Put every open Thread into one Group. A multi-Pane view **is** a Group
    /// (#28), so any test that wants more than one Pane on screen has to say
    /// which Group they share — `View::Solo` shows exactly the focused Pane.
    fn group_all(cockpit: &mut Cockpit) -> GroupId {
        let threads = cockpit.threads();
        let group = cockpit
            .apply_group(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        for thread in &threads[2..] {
            cockpit
                .apply_group(GroupChange::Join {
                    thread: *thread,
                    group,
                    index: None,
                })
                .unwrap();
        }
        group
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

    #[gpui::test]
    fn group_scope_is_one_view_and_cmd_w_makes_the_focused_thread_live_and_solo(
        cx: &mut TestAppContext,
    ) {
        let (mut core, _fake) = cockpit("group-scope-close", 2);
        let threads = core.threads();
        let group = core
            .apply_group(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-w", CloseThread, None)]));
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));

        view.update(cx, |view, cx| view.enter_group(group, cx));
        view.read_with(cx, |view, _| {
            assert!(matches!(view.cockpit.roster().view(), View::Group(_)));
            assert_eq!(view.visible_indices().len(), 2);
        });
        cx.simulate_keystrokes("cmd-w");
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.threads().len(), 2, "leaving never parks");
            assert!(view.cockpit.groups().of(threads[0]).is_none());
            assert_eq!(view.visible_indices().len(), 1);
        });
    }
    #[gpui::test]
    fn a_group_scoped_draft_joins_only_after_its_first_send_succeeds(cx: &mut TestAppContext) {
        let (mut core, _fake) = cockpit("group-draft", 2);
        let threads = core.threads();
        let group = core
            .apply_group(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-t", NewThread, None),
                KeyBinding::new("enter", Submit, None),
            ])
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, cx| view.enter_group(group, cx));
        cx.simulate_keystrokes("cmd-t");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.visible_indices().len(),
                3,
                "the pending draft is visible"
            );
            assert_eq!(
                view.cockpit.groups().iter().next().unwrap().members.len(),
                2,
                "no fake id persisted"
            );
        });
        cx.simulate_input("build it");
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.groups().iter().next().unwrap().members.len(),
                3
            );
            assert!(view.panes.iter().all(|pane| pane.draft().is_none()));
        });
    }

    #[gpui::test]
    fn multi_project_groups_keep_bindings_and_filter_only_navigation(cx: &mut TestAppContext) {
        let (mut core, _) = cockpit("multi-project-group", 2);
        let original = core.threads();
        let first_project = core.project_id(original[0]).unwrap();
        let repo = repo_in(&scratch("multi-project-other"));
        let second_project = core.register_project(&repo).unwrap();
        let group = core
            .apply_group(GroupChange::Create {
                first: original[0],
                second: original[1],
            })
            .unwrap()
            .group
            .unwrap();
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, cx| {
            view.enter_group(group, cx);
            view.open_draft(DraftTarget::Main, cx);
            view.focused_draft_mut()
                .unwrap()
                .binding
                .choose_project(second_project);
            view.panes[view.focused()]
                .composer
                .update(cx, |composer, cx| {
                    composer.set("join from another project".into(), cx)
                });
            view.bootstrap_draft(cx);
            let joined = view
                .focused_thread()
                .expect("different-Project draft joined");
            assert_eq!(
                view.cockpit.groups().get(group).unwrap().members,
                [original[0], original[1], joined]
            );
            assert_eq!(
                view.cockpit
                    .thread(joined)
                    .unwrap()
                    .workspace()
                    .unwrap()
                    .cwd(),
                repo.canonicalize().unwrap()
            );
            assert_eq!(view.cockpit.project_id(original[0]), Some(first_project));
            assert_eq!(view.cockpit.project_id(joined), Some(second_project));
            let nav = view.nav_state();
            assert_eq!(
                nav.groups[0]
                    .projects
                    .as_ref()
                    .map(|label| label.to_string()),
                Some("2 projects".to_string())
            );
            assert_eq!(nav.groups[0].members.len(), 3);
            view.nav_filter = Some(second_project);
            let nav = view.nav_state();
            assert_eq!(
                nav.groups[0]
                    .members
                    .iter()
                    .map(|row| row.thread)
                    .collect::<Vec<_>>(),
                [joined]
            );
            assert_eq!(
                nav.groups[0]
                    .projects
                    .as_ref()
                    .map(|label| label.to_string()),
                Some("2 projects".to_string())
            );
            assert_eq!(
                view.visible_indices().len(),
                3,
                "filter must not shrink the Cockpit"
            );
            view.cockpit.park(joined).unwrap();
            view.sync_panes(cx);
            view.enter_group(group, cx);
            assert_eq!(
                view.visible_indices().len(),
                3,
                "opening the Group revives every member"
            );
            assert_eq!(
                view.cockpit
                    .thread(joined)
                    .unwrap()
                    .workspace()
                    .unwrap()
                    .cwd(),
                repo.canonicalize().unwrap()
            );
        });
    }

    /// The age at the tail of a row's last line hangs under the provider
    /// logomark: one right edge down the row, not two.
    #[gpui::test]
    fn the_age_hangs_under_the_provider_mark(cx: &mut TestAppContext) {
        let (core, _) = cockpit("nav-age-align", 1);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        cx.run_until_parked();
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

        let mark_id: &'static str = format!("nav-mark-{}", thread.get()).leak();
        let age_id: &'static str = format!("nav-since-{}", thread.get()).leak();
        let mark = cx.debug_bounds(mark_id).expect("the row draws a logomark");
        let age = cx.debug_bounds(age_id).expect("the row draws its age");
        assert_eq!(mark.right(), age.right(), "one right edge, not two");
    }

    /// The tree draws one list, not a Groups shelf above a Threads shelf:
    /// Groups and solo Threads sort together by when they were last used,
    /// a Group counting as its most recently used member. Threads created
    /// back to back can share a timestamp, so the assertion is the rule
    /// itself — the order never climbs — plus every item drawn exactly once.
    #[gpui::test]
    fn groups_and_solo_threads_share_one_recency_order(cx: &mut TestAppContext) {
        let (mut core, _) = cockpit("nav-interleave", 4);
        let ids = core.threads();
        core.apply_group(GroupChange::Create {
            first: ids[0],
            second: ids[1],
        })
        .unwrap();
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);

        view.read_with(cx, |view, _| {
            let state = view.nav_state();
            assert_eq!(state.groups.len(), 1);
            assert_eq!(state.solos.len(), 2);
            assert_eq!(
                state.order.len(),
                3,
                "one entry per Group and per solo Thread, and no other"
            );
            let recency = |item: &nav::NavItem| match item {
                nav::NavItem::Group(index) => state.groups[*index]
                    .members
                    .iter()
                    .map(|row| view.last_used(row.thread))
                    .max()
                    .unwrap(),
                nav::NavItem::Solo(index) => view.last_used(state.solos[*index].thread),
            };
            let times: Vec<_> = state.order.iter().map(recency).collect();
            assert!(
                times.windows(2).all(|pair| pair[0] >= pair[1]),
                "most recently used first, whichever kind of item it is"
            );
            let mut seen: Vec<nav::NavItem> = state.order.clone();
            seen.sort_by_key(|item| match item {
                nav::NavItem::Group(index) => (0, *index),
                nav::NavItem::Solo(index) => (1, *index),
            });
            seen.dedup();
            assert_eq!(seen.len(), 3, "nothing is drawn twice and nothing is lost");
            assert_eq!(
                view.nav_state().ordered_rows().len(),
                4,
                "every Thread has a row, member or solo"
            );
        });
    }

    #[gpui::test]
    fn dragging_filtered_members_uses_the_full_group_order(cx: &mut TestAppContext) {
        let (mut core, _) = cockpit("filtered-group-order", 2);
        let repo = repo_in(&scratch("filtered-order-other"));
        let project = core.register_project(&repo).unwrap();
        let third = core
            .open(
                Provider::Claude,
                WorkspaceChoice::Main {
                    checkout: repo.clone(),
                },
            )
            .unwrap();
        let fourth = core
            .open(Provider::Codex, WorkspaceChoice::Main { checkout: repo })
            .unwrap();
        let ids = core.threads();
        let group = core
            .apply_group(GroupChange::Create {
                first: ids[0],
                second: ids[1],
            })
            .unwrap()
            .group
            .unwrap();
        for thread in [third, fourth] {
            core.apply_group(GroupChange::Join {
                thread,
                group,
                index: None,
            })
            .unwrap();
        }
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1200.), px(800.)));
        view.update(cx, |view, cx| {
            view.nav_filter = Some(project);
            view.enter_group(group, cx);
        });
        tick(cx);
        drag_nav(cx, "nav-thread-4", "nav-thread-3");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.groups().get(group).unwrap().members,
                [ids[0], ids[1], fourth, third]
            );
        });
    }

    // Match main: Root mounts the toolkit selection layer, menus and dialogs.
    fn add_cockpit_window(
        cx: &mut TestAppContext,
        build: impl FnOnce(&mut Window, &mut Context<CockpitView>) -> CockpitView,
    ) -> (Entity<CockpitView>, &mut gpui::VisualTestContext) {
        let (root, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| build(window, cx));
            gpui::component::Root::new(view, window, cx)
        });
        let view = root.read_with(cx, |root, _| {
            root.view().clone().downcast::<CockpitView>().unwrap()
        });
        (view, cx)
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

    #[gpui::test]
    fn composer_usage_lines_show_context_and_subscription_windows(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("context-click", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        fake.streams.borrow()[0]
            .send(SessionEvent::RateLimits {
                five_hour: Some(ferrite_core::RateLimitWindow {
                    used_fraction: 0.52,
                    resets_at: Some(11),
                }),
                weekly: Some(ferrite_core::RateLimitWindow {
                    used_fraction: 0.08,
                    resets_at: Some(22),
                }),
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::TokenUsage {
                total_tokens: 124_000,
                input_tokens: 124_000,
                cached_input_tokens: 0,
                output_tokens: 0,
                reasoning_output_tokens: 0,
                context_window: Some(200_000),
            })
            .unwrap();
        tick(cx);
        assert!(cx.debug_bounds("usage-line-context-62").is_some());
        assert!(cx.debug_bounds("usage-line-five-hour-52").is_some());
        assert!(cx.debug_bounds("usage-line-weekly-8").is_some());
        let meter = cx
            .debug_bounds("usage-meter-1")
            .expect("usage lines are visible beside the model");
        cx.simulate_mouse_down(meter.center(), MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("context-usage-current-124000").is_some(),
            "click must reveal current tokens"
        );
        assert!(
            cx.debug_bounds("context-usage-maximum-200000").is_some(),
            "click must reveal maximum tokens"
        );
        assert!(cx.debug_bounds("context-usage-five-hour-52").is_some());
        assert!(cx.debug_bounds("context-usage-weekly-8").is_some());
        fake.streams.borrow()[0]
            .send(SessionEvent::TokenUsage {
                total_tokens: 31_000,
                input_tokens: 31_000,
                cached_input_tokens: 0,
                output_tokens: 0,
                reasoning_output_tokens: 0,
                context_window: None,
            })
            .unwrap();
        tick(cx);
        assert!(
            cx.debug_bounds("context-usage-current-31000").is_some(),
            "open card follows live usage"
        );
        assert!(
            cx.debug_bounds("context-usage-maximum-unknown").is_some(),
            "unknown limit is not invented"
        );
        cx.simulate_mouse_down(meter.center(), MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.context_usage.is_none(), "second meter click closes it")
        });
        cx.simulate_mouse_down(meter.center(), MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.context_usage.is_some()));
        cx.simulate_mouse_down(
            gpui::point(px(500.), px(300.)),
            MouseButton::Left,
            gpui::Modifiers::none(),
        );
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.context_usage.is_none(), "outside click dismisses")
        });
        cx.simulate_mouse_down(meter.center(), MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        cx.simulate_keystrokes("escape");
        view.read_with(cx, |view, _| assert!(view.context_usage.is_none()));
    }

    /// The whole keystroke path in a real window: a blocked Pane, one key, and
    /// the Decision gone because the answer went out.
    #[gpui::test]
    fn one_keystroke_answers_the_card(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("answer", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        fake.streams.borrow()[0].send(decision("perm_01")).unwrap();
        tick(cx);
        view.read_with(cx, |view, _| {
            let thread = view.panes[0].thread().unwrap();
            assert!(
                view.cockpit
                    .thread(thread)
                    .and_then(|open| open.pending())
                    .is_some(),
                "the card should be up before the key"
            );
        });

        cx.simulate_keystrokes("y");

        view.read_with(cx, |view, _| {
            let thread = view.panes[0].thread().unwrap();
            assert!(
                view.cockpit
                    .thread(thread)
                    .and_then(|open| open.pending())
                    .is_none(),
                "y must answer the Decision, not type a letter"
            );
        });
    }

    fn question(id: &str) -> SessionEvent {
        SessionEvent::DecisionRequested {
            decision: Decision {
                id: id.into(),
                tool_use_id: "toolu_q".into(),
                tool_name: "AskUserQuestion".into(),
                description: String::new(),
                input: serde_json::json!({
                    "questions": [{
                        "question": "Which approach?",
                        "header": "Approach",
                        "multiSelect": false,
                        "options": [
                            {"label": "Rewrite", "description": "Start over"},
                            {"label": "Patch", "description": "Smallest change"}
                        ]
                    }]
                }),
                suggestions: vec![],
            },
        }
    }

    /// Claude's `AskUserQuestion` arrives as a Decision whose input is a
    /// form. The Pane draws the form, not the y/n card: a digit picks an
    /// option on the empty line, ↵ sends the pick folded into the tool's
    /// input as `answers` — the shape Claude Code's own UI sends — and a
    /// bare y sends nothing, because an unanswered question is not an
    /// approval.
    #[gpui::test]
    fn a_question_decision_is_answered_by_its_form(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("question-form", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        fake.streams.borrow()[0].send(question("q_01")).unwrap();
        tick(cx);
        view.read_with(cx, |view, _| {
            let draft = view.questions.get(&thread).expect("a draft per question");
            assert_eq!(draft.questions.len(), 1);
            assert!(draft.answers[0].picks.is_empty());
        });

        cx.simulate_keystrokes("y");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit.thread(thread).unwrap().pending().is_some(),
                "y cannot approve an unanswered question"
            );
        });
        // …and while a question pends the letters type: "y" landed on
        // the line, and the digit is a digit with text on it.
        assert_eq!(composer_text(&view, cx), "y");
        cx.simulate_keystrokes("backspace");
        cx.run_until_parked();

        cx.simulate_keystrokes("2");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.questions[&thread].answers[0].picks, vec![1]);
        });

        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit.thread(thread).unwrap().pending().is_none(),
                "↵ sends the answer"
            );
            assert!(
                view.questions.get(&thread).is_none(),
                "the draft dies with it"
            );
        });
        let answered = fake.answered.borrow();
        let (id, answer) = answered.last().expect("the answer went out");
        assert_eq!(id, "q_01");
        let DecisionAnswer::Allow { input } = answer else {
            panic!("a question is answered by allowing with answers: {answer:?}");
        };
        assert_eq!(input["answers"]["Which approach?"], "Patch");
        assert!(input["questions"].is_array(), "the original input is kept");
    }

    /// Text on the line is the operator's own answer: ↵ with "neither,
    /// wait" typed sends it as the answer and clears the line.
    #[gpui::test]
    fn a_typed_line_answers_a_question_in_the_operators_words(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("question-typed", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        fake.streams.borrow()[0].send(question("q_02")).unwrap();
        tick(cx);
        cx.simulate_input("neither, wait 2 days");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.cockpit.thread(thread).unwrap().pending().is_none());
        });
        assert_eq!(
            composer_text(&view, cx),
            "",
            "the line went with the answer"
        );
        let answered = fake.answered.borrow();
        let DecisionAnswer::Allow { input } = &answered.last().unwrap().1 else {
            panic!("allow with answers");
        };
        assert_eq!(input["answers"]["Which approach?"], "neither, wait 2 days");
        assert!(
            fake.sent.borrow().is_empty(),
            "nothing was sent as a prompt"
        );
    }

    /// A model-written title lands only on a Thread nobody has named: the
    /// operator's own rename in the meantime wins, and None changes
    /// nothing.
    #[gpui::test]
    fn a_titlers_answer_names_only_an_untitled_thread(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("titler-adopt", 2);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        let (first, second) = view.read_with(cx, |view, _| {
            let threads = view.cockpit.threads();
            (threads[0], threads[1])
        });
        view.update(cx, |view, cx| {
            view.adopt_title(first, None, cx);
            assert!(view.cockpit.thread(first).unwrap().title().is_none());
            view.adopt_title(first, Some("Wire the titler".into()), cx);
            assert_eq!(
                view.cockpit.thread(first).unwrap().title(),
                Some("Wire the titler")
            );
            assert_eq!(view.facts.name(first).as_ref(), "Wire the titler");

            view.cockpit.rename_thread(second, "Mine").unwrap();
            view.adopt_title(second, Some("Model's idea".into()), cx);
            assert_eq!(view.cockpit.thread(second).unwrap().title(), Some("Mine"));
        });
    }

    /// #26: the mouse presses the keycap it depicts — a real click on the
    /// card's rightmost keycap runs its exact decide verb (`n deny`), even
    /// with text on the Composer line, where the n KEY would type instead.
    /// The sweep hunts the card band so the test does not encode the
    /// keycap's exact position, and the first click that answers must have
    /// denied — allow sits further left.
    #[gpui::test]
    fn clicking_a_keycap_runs_its_own_decide_verb(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("keycap-click", 1);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        fake.streams.borrow()[0].send(decision("perm_01")).unwrap();
        cx.simulate_resize(gpui::size(px(1440.), px(900.)));
        tick(cx);
        cx.simulate_input("not yet");
        view.read_with(cx, |view, cx| {
            assert!(
                view.cockpit
                    .thread(thread)
                    .and_then(|open| open.pending())
                    .is_some(),
                "the premise: a card up"
            );
            assert!(
                !view.panes[0].composer.read(cx).is_empty(),
                "the premise: half-typed text on the line"
            );
        });

        // Sweep the card band right to left until a click answers; misses
        // land on the card's own dead space and change nothing.
        let mut answered = false;
        'sweep: for row in 0..12 {
            let y = 838. - row as f32 * 4.;
            for step in 0..45 {
                let x = 1430. - step as f32 * 6.;
                cx.simulate_click(gpui::point(px(x), px(y)), gpui::Modifiers::none());
                cx.run_until_parked();
                if view.read_with(cx, |view, _| {
                    view.cockpit
                        .thread(thread)
                        .and_then(|open| open.pending())
                        .is_none()
                }) {
                    answered = true;
                    break 'sweep;
                }
            }
        }
        assert!(answered, "the sweep never found a keycap");
        view.read_with(cx, |view, cx| {
            let answered_as = view
                .cockpit
                .thread(thread)
                .map(|open| open.transcript())
                .unwrap()
                .blocks()
                .iter()
                .rev()
                .find_map(|block| match &block.body {
                    Body::Meta(text) => Some(text.clone()),
                    _ => None,
                });
            assert_eq!(
                answered_as.as_deref(),
                Some("denied Write"),
                "the rightmost keycap is `n deny`, and its press runs deny"
            );
            assert_eq!(
                view.panes[0].composer.read(cx).text(),
                "not yet",
                "the press answered the card; it never typed"
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        // A streaming turn makes the Session busy; the next prompt queues.
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: "working".into(),
            })
            .unwrap();
        tick(cx);
        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit.thread(thread).is_some_and(|open| open.busy()),
                "the premise: a turn in flight"
            );
        });
        cx.simulate_input("also this");
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.thread(thread).and_then(|open| open.queued()),
                Some("also this")
            );
        });

        // With text on the line, Backspace edits; the queue is untouched.
        cx.simulate_input("dr");
        cx.simulate_keystrokes("backspace");
        view.read_with(cx, |view, cx| {
            assert!(
                !view.panes[0].composer.read(cx).is_empty(),
                "backspace with text is still an editing key"
            );
            assert_eq!(
                view.cockpit.thread(thread).and_then(|open| open.queued()),
                Some("also this")
            );
        });

        // Emptied, the next Backspace is the advertised ⌫ unqueue.
        cx.simulate_keystrokes("backspace");
        view.read_with(cx, |view, cx| {
            assert!(view.panes[0].composer.read(cx).is_empty());
            assert_eq!(
                view.cockpit.thread(thread).and_then(|open| open.queued()),
                Some("also this")
            );
        });
        cx.simulate_keystrokes("backspace");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.thread(thread).and_then(|open| open.queued()),
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        let closed = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

        cx.simulate_keystrokes("cmd-w");

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1, "the Pane is gone");
            assert!(
                view.cockpit.thread(closed).is_none(),
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        let closed = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        cx.simulate_keystrokes("cmd-w");
        view.read_with(cx, |view, _| assert_eq!(view.panes.len(), 1));

        cx.simulate_keystrokes("cmd-o");

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 2, "the Pane is back");
            assert!(
                view.panes.iter().any(|pane| pane.thread() == Some(closed)),
                "and it is the same Thread, not a new one"
            );
            let blocks = view
                .cockpit
                .thread(closed)
                .map(|open| open.transcript())
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
            let mut ids: Vec<ThreadId> =
                view.panes.iter().filter_map(|pane| pane.thread()).collect();
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        let (a, b) = created(&view, cx);

        view.update(cx, |view, _| view.focus_pane(view.pane_for(b).unwrap()));
        cx.simulate_keystrokes("cmd-w"); // park B
        cx.simulate_keystrokes("cmd-w"); // then A — the most recent park
        cx.simulate_keystrokes("cmd-o");

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1);
            assert_eq!(
                view.panes[0].thread().unwrap(),
                a,
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        let (a, b) = created(&view, cx);

        view.update(cx, |view, _| view.focus_pane(view.pane_for(a).unwrap()));
        cx.simulate_keystrokes("cmd-w"); // park A
        cx.simulate_keystrokes("cmd-w"); // then B

        cx.simulate_keystrokes("cmd-o");
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1);
            assert_eq!(
                view.panes[0].thread().unwrap(),
                b,
                "the last park comes back first"
            );
        });

        cx.simulate_keystrokes("cmd-o");
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 2);
            assert!(
                view.panes.iter().any(|pane| pane.thread() == Some(a)),
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        view.read_with(cx, |view, _| assert_eq!(view.panes.len(), 1));

        cx.simulate_keystrokes("cmd-w"); // park A — this launch's only park
        cx.simulate_keystrokes("cmd-o");
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1);
            assert_eq!(
                view.panes[0].thread().unwrap(),
                a,
                "the just-parked {a} outranks the newer-created {b}"
            );
        });

        cx.simulate_keystrokes("cmd-o"); // the order is drained: creation order
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 2);
            assert!(
                view.panes.iter().any(|pane| pane.thread() == Some(b)),
                "the pre-launch park still comes back, by creation order"
            );
        });
    }

    /// AC4: the wall flags a Thread, and one key answers it from across the
    /// room — the operator never focuses the Pane it belongs to.
    #[gpui::test]
    fn a_wall_flagged_decision_is_answered_without_focusing_its_pane(cx: &mut TestAppContext) {
        let (mut core, fake) = cockpit("wall-answer", 24);
        let group = group_all(&mut core);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("y", Allow, Some("Wall"))]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, cx| view.enter_group(group, cx));
        // A 24-member Group lays out on `groups::grid`, which gives 5 columns
        // here, not the 6 the old global wall's `columns()` gave — so 1440
        // now leaves ~219px cells, one band above the wall. 1200 puts the
        // same 5 columns at ~171px, under the 200px threshold, which is the
        // range this test is about.
        cx.simulate_resize(gpui::size(px(1200.), px(900.)));
        view.update(cx, |view, _| {
            assert_eq!(view.panes.len(), 24);
        });
        cx.update(|window, cx| {
            assert_eq!(view.read(cx).level_now(window), Level::Wall);
        });
        fake.streams.borrow()[7].send(decision("perm_08")).unwrap();
        tick(cx);
        let flagged = view.read_with(cx, |view, _| view.panes[7].thread().unwrap());
        view.read_with(cx, |view, _| {
            assert_eq!(view.focused(), 0, "focus stays where the operator left it");
            assert!(view
                .cockpit
                .thread(flagged)
                .and_then(|open| open.pending())
                .is_some());
        });

        cx.simulate_keystrokes("y");

        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit
                    .thread(flagged)
                    .and_then(|open| open.pending())
                    .is_none(),
                "the flagged Thread is the one that got answered"
            );
            assert_eq!(view.focused(), 0, "and answering did not move the operator");
        });
    }

    /// A Group's grid is near-square with the long edge horizontal, and it
    /// is the *only* grid now: with the global wall gone (#28) there is no
    /// board-specific shape to follow and no six-column ceiling — a Group
    /// is as wide as its own membership makes it. `group_grid` returns
    /// (rows, columns); the cockpit reads the second.
    #[test]
    fn the_grid_follows_the_groups_own_membership() {
        assert_eq!(ferrite_core::groups::grid(1), (1, 1));
        assert_eq!(ferrite_core::groups::grid(2), (1, 2));
        assert_eq!(ferrite_core::groups::grid(6), (2, 3));
        // The prototype's tall-left board is four Panes, laid 2×2 here and
        // overridden to 2×3 by `tall_left_board` in the cockpit itself.
        assert_eq!(ferrite_core::groups::grid(4), (2, 2));
        // Past the old wall's 24, and past its six columns: nothing clamps.
        assert_eq!(ferrite_core::groups::grid(24), (5, 5));
        assert_eq!(ferrite_core::groups::grid(48), (7, 7));
        assert_eq!(ferrite_core::groups::grid(100), (10, 10));
    }

    /// AC1: no mode switch — the same cockpit renders at a different altitude
    /// when the window changes size.
    #[gpui::test]
    fn resizing_the_window_changes_every_panes_level(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("resize", 4);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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

    /// Leg 1, through the draft flow (#29): cmd-shift-n drafts straight
    /// onto "new worktree"; the first send bootstraps, and the Thread
    /// really lands in a worktree of its own — isolation is the whole
    /// point of the binding.
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
            cx.bind_keys([
                KeyBinding::new("cmd-shift-n", NewWorktreeThread, None),
                KeyBinding::new("enter", Submit, None),
            ]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.aim_launch(&repo));
        tick(cx);

        cx.simulate_keystrokes("cmd-shift-n");
        view.read_with(cx, |view, _| {
            let draft = view.panes[view.focused()].draft().expect("a draft Pane");
            assert_eq!(
                *draft.binding.target(),
                DraftTarget::New,
                "aimed at a worktree"
            );
            assert!(fake.streams.borrow().is_empty(), "nothing spawned yet");
        });

        cx.simulate_input("set up the branch");
        cx.simulate_keystrokes("enter");

        view.read_with(cx, |view, _| {
            let thread = view.panes[view.focused()]
                .thread()
                .expect("the first send made a Thread of the draft");
            let binding = view
                .cockpit
                .thread(thread)
                .and_then(|open| open.workspace())
                .expect("a binding");
            assert!(
                matches!(binding, WorkspaceBinding::Worktree { .. }),
                "expected a worktree, got {binding:?}"
            );
            // And it is somewhere of its own, not the operator's checkout.
            assert_ne!(binding.cwd(), repo);
        });
    }

    /// The standing-answer rule holds at wall range too: a request that
    /// offered none is not quietly allowed by the key that means "always".
    #[gpui::test]
    fn always_does_nothing_at_the_wall_when_nothing_was_offered(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("wall-always", 24);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("a", Always, Some("Wall"))]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        // Wall range, as above: the "Wall" key context only exists there.
        cx.simulate_resize(gpui::size(px(1440.), px(900.)));
        // `decision()` offers no standing answer.
        fake.streams.borrow()[3].send(decision("perm_04")).unwrap();
        tick(cx);
        let flagged = view.read_with(cx, |view, _| view.panes[3].thread().unwrap());

        cx.simulate_keystrokes("a");

        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit
                    .thread(flagged)
                    .and_then(|open| open.pending())
                    .is_some(),
                "a Decision with nothing to adopt must still be waiting"
            );
        });
    }

    /// cmd-n drafts onto the checkout the operator is already in — the
    /// plain case, beside cmd-shift-n's worktree — and the zero-keystroke
    /// path holds: typing and sending with the band untouched binds to the
    /// launch project's main checkout.
    #[gpui::test]
    fn a_new_thread_binds_to_the_main_checkout(cx: &mut TestAppContext) {
        let root = scratch("new-main");
        let repo = repo_in(&root);
        let fake = Fake::default();
        let store = Store::open(root.join("threads")).unwrap();
        let core = Cockpit::new(store, Box::new(fake));
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-n", NewThread, None),
                KeyBinding::new("enter", Submit, None),
            ]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.aim_launch(&repo));
        tick(cx);

        cx.simulate_keystrokes("cmd-n");
        cx.simulate_input("hello");
        cx.simulate_keystrokes("enter");

        view.read_with(cx, |view, _| {
            let binding = view
                .cockpit
                .thread(view.panes[view.focused()].thread().unwrap())
                .and_then(|open| open.workspace())
                .expect("a binding");
            assert!(matches!(binding, WorkspaceBinding::Main { .. }));
            // The registry canonicalizes what it registers; the binding is
            // the registered root.
            assert_eq!(binding.cwd(), repo.canonicalize().unwrap());
        });
    }

    /// The band's key path in the ComposerMenu grammar the tests must bind
    /// themselves — production loads it from the keymap table.
    fn bind_band_keys(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("enter", Submit, None),
                KeyBinding::new("tab", BandCycle, None),
                KeyBinding::new("up", MenuPrevious, Some("ComposerMenu")),
                KeyBinding::new("down", MenuNext, Some("ComposerMenu")),
                KeyBinding::new("enter", MenuPick, Some("ComposerMenu")),
                KeyBinding::new("escape", MenuDismiss, Some("ComposerMenu")),
            ]);
        });
    }

    /// AC (#29): the workspace chip is scoped to the chosen project's repo
    /// alone. Driven entirely at the keyboard — tab to the project chip, ↵
    /// opens its popover, arrows pick the other project — and the workspace
    /// popover's rows follow: the second repo's registered worktree is a
    /// row there, and vanishes when the project flips back.
    #[gpui::test]
    fn the_workspace_chip_scopes_to_the_chosen_project(cx: &mut TestAppContext) {
        let base = scratch("band-scope");
        let repo_one = repo_in(&base);
        // A second repo with a distinct leaf name, so the project rows can
        // be told apart.
        let repo_two = base.join("second");
        std::fs::create_dir_all(&repo_two).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "operator@example.invalid"],
            vec!["config", "user.name", "operator"],
            vec!["commit", "-q", "--allow-empty", "-m", "root"],
        ] {
            let out = std::process::Command::new("git")
                .args(&args)
                .current_dir(&repo_two)
                .output()
                .expect("git");
            assert!(out.status.success(), "git {args:?}: {out:?}");
        }
        let fake = Fake::default();
        let store = Store::open(base.join("threads")).unwrap();
        let core = Cockpit::new(store, Box::new(fake));
        bind_band_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, cx| {
            view.aim_launch(&repo_one);
            // The second project holds one registered worktree — made
            // through the core's own bootstrap, then parked away.
            let thread = view
                .cockpit
                .open(
                    Provider::Claude,
                    WorkspaceChoice::NewWorktree {
                        repo: repo_two.clone(),
                    },
                )
                .unwrap();
            view.cockpit.park(thread).unwrap();
            cx.notify();
        });
        tick(cx);
        view.read_with(cx, |view, _| {
            assert!(
                view.panes[view.focused()].draft().is_some(),
                "an empty store launches as a draft Pane"
            );
        });

        // tab tab tab: provider, effort, then the project chip; ↵ opens
        // its popover (bare enter — no popover is up yet, so Submit routes
        // it to the focused chip).
        cx.simulate_keystrokes("tab");
        cx.simulate_keystrokes("tab");
        cx.simulate_keystrokes("tab");
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            let band = view.popover.as_ref().expect("the project popover is open");
            assert!(matches!(band.kind, Kind::Band(pane::BandChip::Project)));
            let labels: Vec<&str> = band.rows.iter().map(|row| row.name.as_ref()).collect();
            assert!(labels.contains(&"repo"), "rows: {labels:?}");
            assert!(labels.contains(&"second"), "rows: {labels:?}");
        });

        // The arrows start on the standing choice (repo); down reaches the
        // project registered after it — "second", the one with a worktree.
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("enter");
        let chosen = view.read_with(cx, |view, _| {
            let draft = view.panes[view.focused()].draft().expect("still a draft");
            assert_eq!(
                *draft.binding.target(),
                DraftTarget::Main,
                "changing the project resets the workspace chip"
            );
            assert_eq!(
                view.cockpit
                    .registry()
                    .project(draft.binding.project())
                    .unwrap()
                    .title,
                "second"
            );
            draft.binding.project()
        });

        // tab: the workspace chip; ↵ opens its popover, scoped to the
        // chosen project only — its one worktree between main and new.
        cx.simulate_keystrokes("tab");
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            let band = view
                .popover
                .as_ref()
                .expect("the workspace popover is open");
            assert!(matches!(band.kind, Kind::Band(pane::BandChip::Workspace)));
            let labels: Vec<&str> = band.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(
                labels,
                vec!["main", "ferrite/wt-1", "new worktree"],
                "scoped to the chosen repo"
            );
        });

        // Flip the project back and the workspace rows follow — no global
        // list anywhere.
        cx.simulate_keystrokes("escape");
        cx.simulate_keystrokes("tab"); // prompt → provider
        cx.simulate_keystrokes("tab"); // → effort
        cx.simulate_keystrokes("tab"); // → project
        cx.simulate_keystrokes("enter");
        cx.simulate_keystrokes("up");
        cx.simulate_keystrokes("enter");
        cx.simulate_keystrokes("tab"); // → workspace
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            let band = view.popover.as_ref().expect("the workspace popover again");
            assert!(matches!(band.kind, Kind::Band(pane::BandChip::Workspace)));
            let draft = view.panes[view.focused()].draft().expect("still a draft");
            assert_ne!(
                draft.binding.project(),
                chosen,
                "the arrows flipped the project"
            );
            let labels: Vec<&str> = band.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(
                labels,
                vec!["main", "new worktree"],
                "no worktree row leaks in from the other repo"
            );
        });
    }

    /// AC (#29): the first send is the lock — the draft becomes a Thread,
    /// the band is gone, the transcript opens with a Notice naming the
    /// checkout, and the prompt follows it.
    #[gpui::test]
    fn the_first_send_bootstraps_the_thread_and_drops_the_band(cx: &mut TestAppContext) {
        let base = scratch("band-lock");
        let repo = repo_in(&base);
        let fake = Fake::default();
        let store = Store::open(base.join("threads")).unwrap();
        let core = Cockpit::new(store, Box::new(fake.clone()));
        bind_band_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.aim_launch(&repo));
        tick(cx);

        cx.simulate_input("run the tests");
        cx.simulate_keystrokes("enter");

        view.read_with(cx, |view, cx| {
            let pane = &view.panes[view.focused()];
            let thread = pane.thread().expect("the draft became a Thread");
            assert!(
                view.cockpit
                    .thread(thread)
                    .is_some_and(|open| open.first_prompt_sent()),
                "the first send armed the lock"
            );
            assert!(
                pane.composer.read(cx).is_empty(),
                "the sent prompt left the line"
            );
            let blocks = view.cockpit.thread(thread).unwrap().transcript().blocks();
            assert!(
                matches!(
                    &blocks[0].body,
                    Body::Notice(line) if line.starts_with("opened in ")
                ),
                "the first line names the checkout: {:?}",
                blocks[0].body
            );
            assert!(
                matches!(&blocks[1].body, Body::Prompt(line) if line == "run the tests"),
                "the prompt follows: {:?}",
                blocks[1].body
            );
        });
    }

    /// Background readiness must settle the submitted Pane, preserving any
    /// later edits and the operator's current focus/picker elsewhere.
    #[gpui::test]
    fn delayed_bootstrap_preserves_edited_composer_focus_and_another_drafts_picker(
        cx: &mut TestAppContext,
    ) {
        use ferrite_core::session::SessionLifecycle;
        use std::sync::{Arc, Mutex};
        struct Ready {
            events: Receiver<SessionEvent>,
            sent: Arc<Mutex<Vec<String>>>,
        }
        impl Session for Ready {
            fn events(&self) -> &Receiver<SessionEvent> {
                &self.events
            }
            fn send(&mut self, text: &str) -> std::io::Result<()> {
                self.sent.lock().unwrap().push(text.into());
                Ok(())
            }
            fn interrupt(&mut self) -> std::io::Result<()> {
                Ok(())
            }
            fn respond_to_decision(&mut self, _: &str, _: DecisionAnswer) -> std::io::Result<()> {
                Ok(())
            }
        }
        struct Delayed {
            gates: std::collections::VecDeque<Receiver<()>>,
            sent: Arc<Mutex<Vec<String>>>,
        }
        impl Spawner for Delayed {
            fn spawn(
                &mut self,
                _: ferrite_core::cockpit::SpawnRequest,
            ) -> std::io::Result<Box<dyn Session>> {
                panic!("use start")
            }
            fn start(
                &mut self,
                _: ferrite_core::cockpit::SpawnRequest,
            ) -> std::io::Result<SessionLifecycle> {
                let gate = self.gates.pop_front().expect("one start per submission");
                let sent = self.sent.clone();
                SessionLifecycle::background(move || {
                    gate.recv().unwrap();
                    let (_, events) = mpsc::channel();
                    Ok(Box::new(Ready { events, sent }) as Box<dyn Session + Send>)
                })
            }
        }
        let base = scratch("delayed-bootstrap-edits");
        let repo = repo_in(&base);
        let sent = Arc::new(Mutex::new(Vec::new()));
        let (first_tx, first_rx) = mpsc::channel();
        let (cancel_tx, cancel_rx) = mpsc::channel();
        let (escape_tx, escape_rx) = mpsc::channel();
        let core = Cockpit::new(
            Store::open(base.join("threads")).unwrap(),
            Box::new(Delayed {
                gates: [first_rx, cancel_rx, escape_rx].into(),
                sent: sent.clone(),
            }),
        );
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        let (submitted, other) = view.update(cx, |view, cx| {
            view.aim_launch(&repo);
            let submitted = view.panes[view.focused()].identity.draft().unwrap();
            view.panes[view.focused()]
                .composer
                .update(cx, |composer, cx| composer.set("first".into(), cx));
            view.bootstrap_draft(cx);
            assert!(view.cockpit.draft_starting(submitted));
            view.panes[view.focused()]
                .composer
                .update(cx, |composer, cx| {
                    composer.set("edited while starting".into(), cx)
                });
            view.open_draft(DraftTarget::Main, cx);
            let other = view.panes[view.focused()].identity.draft().unwrap();
            view.open_band_popover(pane::BandChip::Provider, cx);
            (submitted, other)
        });
        first_tx.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let finished = view.update(cx, |view, cx| {
                view.pump(cx);
                !view.cockpit.draft_starting(submitted)
            });
            if finished {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "startup did not settle"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        view.update(cx, |view, cx| {
            assert_eq!(
                view.cockpit.roster().focused(),
                Some(PaneIdentity::Draft(other))
            );
            assert_eq!(
                view.popover.as_ref().unwrap().pane,
                PaneIdentity::Draft(other)
            );
            let first = view
                .panes
                .iter()
                .find(|pane| pane.thread().is_some())
                .unwrap();
            assert_eq!(first.composer.read(cx).text(), "edited while starting");
            assert_eq!(sent.lock().unwrap().as_slice(), ["first"]);
            // A changed draft choice cancels the submitted startup before
            // applying the new choice, so the old request can never send.
            view.popover = None;
            let composer = view.panes[view.focused()].composer.clone();
            composer.update(cx, |composer, cx| {
                composer.set("cancel on choice".into(), cx)
            });
            view.bootstrap_draft(cx);
            assert!(view.cockpit.draft_starting(other));
            view.pick_band(
                &BandChoice::Provider(ProviderChoice {
                    provider: Provider::Codex,
                    model: None,
                }),
                &composer,
                cx,
            );
            assert!(!view.cockpit.draft_starting(other));
            assert_eq!(composer.read(cx).text(), "cancel on choice");
        });
        cancel_tx.send(()).unwrap();
        view.update(cx, |view, cx| {
            view.bootstrap_draft(cx);
            assert!(view.cockpit.draft_starting(other));
        });
        cx.update(|window, cx| view.update(cx, |view, cx| view.interrupt(&Interrupt, window, cx)));
        view.read_with(cx, |view, cx| {
            assert!(!view.cockpit.draft_starting(other));
            assert_eq!(
                view.panes[view.focused()].composer.read(cx).text(),
                "cancel on choice"
            );
        });
        escape_tx.send(()).unwrap();
        assert_eq!(sent.lock().unwrap().as_slice(), ["first"]);
    }

    /// AC (#29): a failed bootstrap is a no-op with words — no Thread, the
    /// Pane stays draft, the error shows at the band, and the prompt stays
    /// in the Composer for the retry.
    #[gpui::test]
    fn a_failed_bootstrap_keeps_the_draft_and_the_prompt(cx: &mut TestAppContext) {
        let base = scratch("band-refused");
        let repo = repo_in(&base);
        let fake = Fake::default();
        let store = Store::open(base.join("threads")).unwrap();
        let core = Cockpit::new(store, Box::new(fake.clone()));
        bind_band_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.aim_launch(&repo));
        tick(cx);
        *fake.fail_send.borrow_mut() = true;

        cx.simulate_input("precious words");
        cx.simulate_keystrokes("enter");

        view.read_with(cx, |view, cx| {
            let pane = &view.panes[view.focused()];
            let draft = pane.draft().expect("the Pane stays draft");
            assert!(
                draft
                    .error
                    .as_ref()
                    .is_some_and(|error| error.contains("stub refused first prompt")),
                "the failure shows its words: {:?}",
                draft.error
            );
            assert_eq!(
                pane.composer.read(cx).text(),
                "precious words",
                "the prompt is preserved for the retry"
            );
            assert_eq!(
                view.cockpit.parked().unwrap(),
                vec![],
                "no half-born Thread"
            );
        });

        // The retry goes through once the Session accepts the first prompt.
        *fake.fail_send.borrow_mut() = false;
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            assert!(
                view.panes[view.focused()].thread().is_some(),
                "the same draft bootstraps on the retry"
            );
        });
    }

    /// #29: the header's binding slot is fed from the branch cache — the
    /// actual git checkout of the Thread's cwd, read at bootstrap and on
    /// refresh, so an agent that switches branches is reported honestly.
    /// Display-only by construction: nothing here is a control.
    #[gpui::test]
    fn the_locked_pane_caches_the_actual_checkout_branch(cx: &mut TestAppContext) {
        let base = scratch("branch-cache");
        let repo = repo_in(&base);
        let fake = Fake::default();
        let store = Store::open(base.join("threads")).unwrap();
        let core = Cockpit::new(store, Box::new(fake));
        bind_band_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.aim_launch(&repo));
        tick(cx);

        cx.simulate_input("hello");
        cx.simulate_keystrokes("enter");

        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .expect("git");
            assert!(out.status.success(), "git {args:?}: {out:?}");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        let expected = git(&["branch", "--show-current"]);
        let thread = view.read_with(cx, |view, _| {
            let thread = view.panes[view.focused()].thread().expect("locked");
            assert_eq!(
                view.facts
                    .get(thread)
                    .and_then(|facts| facts.branch.as_ref())
                    .map(|branch| branch.to_string()),
                Some(expected.clone()),
                "the bootstrap cached the checkout"
            );
            thread
        });

        // The agent moves the checkout while no Session event arrives. The
        // watchdog cadence still refreshes and repaints the header.
        git(&["checkout", "-q", "-b", "agent-moved"]);
        view.update(cx, |view, cx| {
            view.swept = std::time::Instant::now() - SWEEP_INTERVAL;
            view.pump(cx);
        });
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.facts
                    .get(thread)
                    .and_then(|facts| facts.branch.as_ref())
                    .map(|branch| branch.to_string()),
                Some(expected),
                "the pump returns before the background Git refresh"
            );
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.facts
                    .get(thread)
                    .and_then(|facts| facts.branch.as_ref())
                    .map(|branch| branch.to_string()),
                Some("agent-moved".to_string()),
                "the slot follows the repo, not the binding"
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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
        let (mut core, _fake) = cockpit("click-focus", 2);
        let group = group_all(&mut core);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, cx| view.enter_group(group, cx));
        // Two Panes side by side, each big enough to hold a Composer even
        // with the 208px nav (#21) taken off the left.
        cx.simulate_resize(gpui::size(px(1800.), px(600.)));
        tick(cx);
        view.read_with(cx, |view, _| assert_eq!(view.focused(), 0));

        cx.simulate_click(gpui::point(px(1200.), px(300.)), gpui::Modifiers::none());

        view.read_with(cx, |view, _| {
            assert_eq!(view.focused(), 1, "the click moved the focus ring");
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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
            (scroll.offset().y, scroll.max_offset().y)
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
            let gap = scroll.max_offset().y + scroll.offset().y;
            assert!(
                gap <= TAIL_SLACK,
                "scrolling to the bottom reattaches the tail: {gap:?}"
            );
        });
    }

    /// A Thread opens on its tail. A fresh ScrollHandle sits at the top, so
    /// a reopened Thread with history landed on its oldest line; it must
    /// land on its newest, and keep following the tail from there.
    #[gpui::test]
    fn a_reopened_thread_opens_at_the_bottom_and_keeps_following_the_tail(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("reopen-at-tail", 2);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-w", CloseThread, None),
                KeyBinding::new("cmd-o", ReopenThread, None),
            ]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        // Reopening respawns the Session, so the revived Thread streams on
        // the newest Sender, not the one it was parked with.
        let say = |line: usize| {
            fake.streams
                .borrow()
                .last()
                .expect("a spawned Session")
                .send(SessionEvent::TextDelta {
                    text: format!("history line {line:03}\n\n"),
                })
                .unwrap();
        };
        // Stream into the second-spawned Session: that is `panes[0]`'s only
        // if spawn order matches grid order, so pick the Pane by its stream
        // instead — the last spawn is the last Thread created.
        for line in 0..80 {
            say(line);
        }
        tick(cx);
        let closed = view.read_with(cx, |view, _| {
            view.panes
                .iter()
                .filter_map(|pane| pane.thread())
                .max()
                .unwrap()
        });
        let at = view.read_with(cx, |view, _| {
            view.panes
                .iter()
                .position(|pane| pane.thread() == Some(closed))
                .unwrap()
        });
        view.update(cx, |view, _| view.focus_pane(at));
        cx.simulate_keystrokes("cmd-w");
        view.read_with(cx, |view, _| assert_eq!(view.panes.len(), 1));

        cx.simulate_keystrokes("cmd-o");
        tick(cx);
        let gap = |cx: &mut gpui::VisualTestContext| {
            view.read_with(cx, |view, _| {
                let pane = view
                    .panes
                    .iter()
                    .find(|pane| pane.thread() == Some(closed))
                    .expect("the reopened Pane");
                let scroll = &pane.scroll;
                (
                    scroll.max_offset().y,
                    scroll.max_offset().y + scroll.offset().y,
                )
            })
        };
        let (max, at_open) = gap(cx);
        assert!(max > px(0.), "the transcript must overflow for this test");
        assert!(
            at_open <= TAIL_SLACK,
            "a reopened Thread opens at its bottom, not its top: {at_open:?} of {max:?}"
        );

        // Streaming into the reopened Pane keeps it on the tail.
        for line in 80..100 {
            say(line);
        }
        tick(cx);
        let (later_max, after_stream) = gap(cx);
        assert!(later_max > max, "more history grew the transcript");
        assert!(
            after_stream <= TAIL_SLACK,
            "new Blocks keep a tail-riding reader at the bottom: {after_stream:?}"
        );
    }

    /// #15 AC2 at character grain (#27): a mid-word press sweeps exact
    /// characters across Blocks, and cmd-c puts exactly the highlighted
    /// text on the clipboard — endpoint rows cut at their characters, the
    /// middle row whole, rows joined with honest newlines. A plain click
    /// selects nothing and leaves the clipboard alone.
    #[gpui::test]
    fn a_drag_selects_characters_and_the_copy_key_takes_exactly_them(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("select-copy", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: "alpha\n\nbravo\n\ncharlie\n\n".into(),
            })
            .unwrap();
        tick(cx);
        // "al|pha" down to "char|lie".
        let from = caret(&view, cx, 0, 2);
        let to = caret(&view, cx, 2, 4);

        cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(clipboard(cx).as_deref(), Some("pha\nbravo\nchar"));

        // A plain click clears the selection; copying then changes nothing.
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("kept".into())));
        cx.simulate_click(from, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(clipboard(cx).as_deref(), Some("kept"));
    }

    /// The on-screen position of a byte in one Block's first text run in
    /// the first Pane — where a test aims the mouse (#27). TextLayout
    /// records screen-space geometry, so no scroll math is needed.
    fn caret(
        view: &gpui::Entity<CockpitView>,
        cx: &mut gpui::VisualTestContext,
        block: usize,
        byte: usize,
    ) -> gpui::Point<gpui::Pixels> {
        cx.update(|window, cx| {
            let view = view.read(cx);
            let thread = view.panes[0].thread().unwrap();
            let blocks = view.cockpit.thread(thread).unwrap().transcript().blocks();
            let current = &blocks[block];
            let (id, item, paragraphs, text) = if current.markdown.is_some() {
                let start = (0..=block)
                    .rev()
                    .take_while(|index| blocks[*index].markdown.is_some())
                    .last()
                    .unwrap();
                let text = match &current.body {
                    ferrite_core::transcript::Body::Paragraph { spans } => spans
                        .iter()
                        .map(|span| span.text.as_str())
                        .collect::<String>(),
                    _ => current.markdown.clone().unwrap(),
                };
                (
                    format!(
                        "markdown-{}-{:?}",
                        thread.get(),
                        current.markdown_run.unwrap_or(blocks[start].id)
                    ),
                    block - start,
                    blocks[start..]
                        .iter()
                        .take_while(|block| block.markdown.is_some())
                        .count(),
                    text,
                )
            } else {
                let text = view
                    .selection
                    .registered(thread)
                    .into_iter()
                    .find(|(id, ordinal, _, _)| *id == current.id && *ordinal == 0)
                    .unwrap()
                    .3;
                (
                    format!("literal-{}-{:?}-0", thread.get(), current.id),
                    0,
                    1,
                    text,
                )
            };
            crate::rich::testing::caret(&id, item, paragraphs, &text, byte, window, cx)
                .expect("a rendered native caret")
        })
    }

    fn clipboard(cx: &mut gpui::VisualTestContext) -> Option<String> {
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
    }

    /// A repeated press at the same spot, as the platform reports it: the
    /// second or third press of a multi-click carries its count.
    fn press(cx: &mut gpui::VisualTestContext, position: gpui::Point<Pixels>, count: usize) {
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: gpui::Modifiers::none(),
            click_count: count,
            first_mouse: false,
        });
    }

    /// Native selection: double-click takes a word; triple-click a paragraph.
    #[gpui::test]
    fn double_click_selects_the_word_and_triple_click_the_line(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("select-clicks", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: "make it fast\n\nkeep it honest\n\n".into(),
            })
            .unwrap();
        tick(cx);

        // Double-click lands mid "make" and takes the whole word.
        let on_make = caret(&view, cx, 0, 2);
        cx.simulate_click(on_make, gpui::Modifiers::none());
        press(cx, on_make, 2);
        cx.simulate_mouse_up(on_make, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(clipboard(cx).as_deref(), Some("make"));

        // Triple-click takes the rendered run whole.
        let on_honest = caret(&view, cx, 1, 10);
        press(cx, on_honest, 3);
        cx.simulate_mouse_up(on_honest, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(clipboard(cx).as_deref(), Some("keep it honest"));
    }

    /// #27 fix round: chrome is not selectable ground. A double-click in
    /// the Composer region selects nothing — a press anchors only inside
    /// the transcript body — and a drag that leaves through the bottom
    /// edge clamps into the body, selecting to the last row instead of
    /// freezing short or grabbing chrome.
    #[gpui::test]
    fn presses_on_chrome_select_nothing_and_a_drag_out_the_bottom_clamps(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("select-chrome", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: "alpha\n\nbravo\n\ncharlie\n\n".into(),
            })
            .unwrap();
        tick(cx);
        let body = view.read_with(cx, |view, _| view.panes[0].scroll.bounds());

        // A double-click below the body — the Composer region — must not
        // light up a word in the nearest transcript row.
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("kept".into())));
        let on_chrome = gpui::point(body.center().x, body.bottom() + px(20.));
        cx.simulate_click(on_chrome, gpui::Modifiers::none());
        press(cx, on_chrome, 2);
        cx.simulate_mouse_up(on_chrome, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(clipboard(cx).as_deref(), Some("kept"));

        // A drag that exits through the bottom edge keeps extending —
        // moves ride the root — and clamps to the body's boundary: the
        // sweep reaches the end of the last row, never the Composer.
        let from = caret(&view, cx, 0, 2);
        let out = gpui::point(body.right() - px(10.), body.bottom() + px(60.));
        cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(out, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(out, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(clipboard(cx).as_deref(), Some("pha\nbravo\ncharlie"));
    }

    /// #27: copy is what you see. A sweep across a tool row takes its
    /// composed call pieces joined with nothing and its ⎿ continuation on
    /// its own line — never the ⏺ gutter, the verdict chip, or a duration.
    #[gpui::test]
    fn a_sweep_across_a_tool_row_copies_its_text_and_never_its_chrome(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("select-tool-row", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        let stream = fake.streams.borrow();
        stream[0]
            .send(SessionEvent::TextDelta {
                text: "before\n\n".into(),
            })
            .unwrap();
        stream[0]
            .send(SessionEvent::ToolStarted {
                id: "toolu_9".into(),
                name: "Bash".into(),
                input: serde_json::json!({ "command": "echo hi" }),
            })
            .unwrap();
        stream[0]
            .send(SessionEvent::ToolCompleted {
                id: "toolu_9".into(),
                output: "done".into(),
                is_error: false,
                result: ferrite_core::ToolResult::Opaque,
            })
            .unwrap();
        stream[0]
            .send(SessionEvent::TextDelta {
                text: "after\n\n".into(),
            })
            .unwrap();
        drop(stream);
        tick(cx);

        let from = caret(&view, cx, 0, 0);
        let mut to = caret(&view, cx, 2, "after".len() - 1);
        to.x += px(40.);
        cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(
            clipboard(cx).as_deref(),
            Some("before\n\nBash(echo hi)\n\ndone\n\nafter"),
            "the exit-0 chip and the ⏺ are chrome and must not copy"
        );
    }

    #[gpui::test]
    fn consecutive_shell_calls_share_one_disclosure_and_keep_failures_visible(
        cx: &mut TestAppContext,
    ) {
        let (core, fake) = cockpit("shell-group", 1);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        for (index, command) in ["uname -srm", "date", "pwd", "git status --short"]
            .iter()
            .enumerate()
        {
            fake.streams.borrow()[0]
                .send(SessionEvent::ToolStarted {
                    id: format!("shell-{index}"),
                    name: "commandExecution".into(),
                    input: serde_json::json!({"command": command}),
                })
                .unwrap();
            fake.streams.borrow()[0]
                .send(SessionEvent::ToolCompleted {
                    id: format!("shell-{index}"),
                    output: format!("output {index}"),
                    is_error: index == 2,
                    result: ferrite_core::ToolResult::Opaque,
                })
                .unwrap();
        }
        tick(cx);
        assert!(
            cx.debug_bounds("tool-group-shell-0").is_some(),
            "four calls should render one activity summary"
        );
        assert!(
            cx.debug_bounds("tool-group-failures-shell-0").is_some(),
            "failure remains visible when closed"
        );
        view.read_with(cx, |view, _| {
            let runs = view.selection.registered(thread);
            assert!(
                !runs.iter().any(|(_, _, _, text)| text == "output 0"),
                "successful output starts collapsed"
            );
        });
        let chevron = view.read_with(cx, |view, _| {
            view.panes[0].tool_bounds("shell-0").unwrap().center()
        });
        cx.simulate_click(chevron, gpui::Modifiers::none());
        tick(cx);
        view.read_with(cx, |view, _| {
            let runs = view.selection.registered(thread);
            for index in 0..4 {
                assert!(runs
                    .iter()
                    .any(|(_, _, _, text)| text == &format!("output {index}")));
            }
        });
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolStarted {
                id: "shell-4".into(),
                name: "Bash".into(),
                input: serde_json::json!({"command": "echo next"}),
            })
            .unwrap();
        tick(cx);
        assert!(cx.debug_bounds("tool-group-running-shell-0").is_some());
        view.read_with(cx, |view, _| {
            assert!(
                view.panes[0].tool_expanded("shell-0"),
                "streaming preserves disclosure choice"
            )
        });
    }

    #[gpui::test]
    fn multiline_tool_input_is_compact_and_disclosable_while_running(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("multiline-tool-disclosure", 1);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        let command = "cat <<'EOF'\nlet answer = 42;\nEOF";
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolStarted {
                id: "multiline".into(),
                name: "Bash".into(),
                input: serde_json::json!({ "command": command }),
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolStarted {
                id: "next".into(),
                name: "Read".into(),
                input: serde_json::json!({ "file_path": "next.rs" }),
            })
            .unwrap();
        tick(cx);

        let chevron = view.read_with(cx, |view, cx| {
            let runs = view.selection.registered(thread);
            assert!(runs
                .iter()
                .any(|(_, _, _, text)| text.contains("cat <<'EOF' …")));
            assert!(!runs
                .iter()
                .any(|(_, _, _, text)| text.contains("let answer")));
            let first = runs
                .iter()
                .find(|(_, _, _, text)| text.starts_with("Bash("))
                .unwrap()
                .0;
            let next = runs
                .iter()
                .find(|(_, _, _, text)| text.starts_with("Read("))
                .unwrap()
                .0;
            let y = |block| {
                crate::rich::testing::bounds(
                    &format!("literal-{}-{block:?}-0", thread.get()),
                    0,
                    cx,
                )
                .unwrap()
                .top()
            };
            assert!(
                y(next) - y(first) < px(30.),
                "a collapsed script must stay one row tall"
            );
            view.panes[0].tool_bounds("multiline").unwrap().center()
        });
        cx.simulate_click(chevron, gpui::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.panes[0].tool_expanded("multiline"));
            assert!(view
                .selection
                .registered(thread)
                .iter()
                .any(|(_, _, _, text)| text == "let answer = 42;"));
        });

        // Subsequent events must not prune an input-only disclosure.
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta {
                text: "continuing".into(),
            })
            .unwrap();
        tick(cx);
        view.read_with(cx, |view, _| {
            assert!(view.panes[0].tool_expanded("multiline"))
        });

        let chevron = view.read_with(cx, |view, _| {
            view.panes[0].tool_bounds("multiline").unwrap().center()
        });
        cx.simulate_click(chevron, gpui::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.panes[0].tool_expanded("multiline"));
            let transcript = view.cockpit.thread(thread).unwrap().transcript();
            assert!(transcript
                .blocks()
                .iter()
                .any(|block| matches!(&block.body, Body::Tool(tool) if tool.summary == command)));
        });
    }

    #[gpui::test]
    fn clicking_the_tool_chevron_replaces_the_compact_line_with_selectable_output(
        cx: &mut TestAppContext,
    ) {
        let (core, fake) = cockpit("tool-disclosure-pointer", 1);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        let output = format!("first line\n{}TAIL", "x".repeat(65_540));
        let stream = fake.streams.borrow();
        stream[0]
            .send(SessionEvent::ToolStarted {
                id: "toolu_9".into(),
                name: "Bash".into(),
                input: serde_json::json!({ "command": "echo hi" }),
            })
            .unwrap();
        stream[0]
            .send(SessionEvent::ToolCompleted {
                id: "toolu_9".into(),
                output,
                is_error: false,
                result: ferrite_core::ToolResult::Opaque,
            })
            .unwrap();
        stream[0]
            .send(SessionEvent::ToolStarted {
                id: "toolu_10".into(),
                name: "Read".into(),
                input: serde_json::json!({ "file_path": "/workspace/pending" }),
            })
            .unwrap();
        drop(stream);
        tick(cx);

        view.read_with(cx, |view, cx| {
            let transcript = view.cockpit.thread(thread).unwrap().transcript();
            let id = |call: &str| {
                transcript
                    .blocks()
                    .iter()
                    .find_map(|block| match &block.body {
                        Body::Tool(tool) if tool.call == call => Some(block.id),
                        _ => None,
                    })
                    .unwrap()
            };
            let x = |block| {
                crate::rich::testing::bounds(
                    &format!("literal-{}-{block:?}-0", thread.get()),
                    0,
                    cx,
                )
                .unwrap()
                .left()
            };
            assert_eq!(
                x(id("toolu_9")),
                x(id("toolu_10")),
                "a disclosure must occupy the existing gutter, not shift call text"
            );
        });

        let collapsed = view.read_with(cx, |view, _| view.selection.registered(thread));
        assert!(collapsed.iter().any(|(_, _, _, text)| text == "first line"));
        let chevron = view.read_with(cx, |view, _| {
            let bounds = view.panes[0]
                .tool_bounds("toolu_9")
                .expect("the actual rendered chevron bounds");
            assert_eq!(bounds.size.width, px(crate::theme::TOOL_DISCLOSURE_HIT));
            bounds.center()
        });
        cx.simulate_click(chevron, gpui::Modifiers::none());
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(view.panes[0].tool_expanded("toolu_9"));
            let runs = view.selection.registered(thread);
            // Every hard line of the output is its own run starting a
            // line, so a drag across them copies back with the newlines
            // the output had.
            let tool_block = view
                .cockpit
                .thread(thread)
                .unwrap()
                .transcript()
                .blocks()
                .iter()
                .find_map(|block| match &block.body {
                    Body::Tool(tool) if tool.call == "toolu_9" => Some(block.id),
                    _ => None,
                })
                .unwrap();
            let lines: Vec<&(_, _, _, String)> = runs
                .iter()
                .filter(|(block, _, _, _)| *block == tool_block)
                .collect();
            assert!(lines
                .iter()
                .any(|(_, _, starts, text)| *starts && text == "first line"));
            let total: usize = lines.iter().map(|(_, _, _, text)| text.len() + 1).sum();
            assert!(total >= 64 * 1024, "{total}");
            assert!(!runs
                .iter()
                .any(|(_, _, _, text)| text.contains('▾') || text.contains("bytes omitted")));
        });

        // Expanding long output can scroll the header out of view while
        // following the tail. Bring it back before clicking its disclosure.
        view.update(cx, |view, cx| {
            view.panes[0].follow_tail.set(false);
            view.panes[0].scroll.set_offset(gpui::point(px(0.), px(0.)));
            cx.notify();
        });
        cx.run_until_parked();

        let chevron = view.read_with(cx, |view, _| {
            view.panes[0].tool_bounds("toolu_9").unwrap().center()
        });
        cx.simulate_click(chevron, gpui::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.panes[0].tool_expanded("toolu_9"));
            assert!(view
                .selection
                .registered(thread)
                .iter()
                .any(|(_, _, _, text)| text == "first line"));
        });

        let composer = view.read_with(cx, |view, _| {
            let body = view.panes[0].scroll.bounds();
            gpui::point(body.center().x, body.bottom() + px(20.))
        });
        cx.simulate_click(composer, gpui::Modifiers::none());
        cx.update(|window, cx| {
            view.read_with(cx, |view, cx| {
                assert!(!view.panes[0].tool_targeted("toolu_9"));
                assert!(view.panes[0].composer.focus_handle(cx).is_focused(window));
            });
        });
    }

    #[gpui::test]
    fn keyboard_cycles_tool_controls_and_enter_toggles_without_submitting(cx: &mut TestAppContext) {
        let (mut core, fake) = cockpit("tool-disclosure-keyboard", 1);
        let thread = core.threads()[0];
        core.send(thread, "prior prompt".into());
        bind_production_keys(cx);
        let (root, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| CockpitView::new(core, cx));
            gpui::component::Root::new(view, window, cx)
        });
        let view = root.read_with(cx, |root, _| {
            root.view().clone().downcast::<CockpitView>().unwrap()
        });
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        let stream = fake.streams.borrow();
        stream[0]
            .send(SessionEvent::ToolStarted {
                id: "toolu_9".into(),
                name: "Bash".into(),
                input: serde_json::json!({ "command": "echo hi" }),
            })
            .unwrap();
        stream[0]
            .send(SessionEvent::ToolCompleted {
                id: "toolu_9".into(),
                output: "first line\nsecond line".into(),
                is_error: false,
                result: ferrite_core::ToolResult::Opaque,
            })
            .unwrap();
        stream[0]
            .send(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
        drop(stream);
        tick(cx);
        cx.simulate_input("unsent draft");

        cx.simulate_keystrokes("tab");
        view.read_with(cx, |view, _| {
            assert!(view.panes[0].tool_targeted("toolu_9"));
            assert!(!view.history_available(0, Level::Transcript));
        });
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            assert!(view.panes[0].tool_expanded("toolu_9"));
            assert_eq!(
                fake.sent.borrow().as_slice(),
                ["prior prompt"],
                "Enter must not submit"
            );
        });

        cx.simulate_keystrokes("tab");
        cx.update(|window, cx| {
            view.read_with(cx, |view, cx| {
                assert!(!view.panes[0].tool_targeted("toolu_9"));
                assert!(view.panes[0].composer.focus_handle(cx).is_focused(window));
            });
        });

        cx.simulate_keystrokes("shift-tab");
        view.read_with(
            cx,
            |view, _| assert!(view.panes[0].tool_targeted("toolu_9")),
        );
        for line in 0..200 {
            fake.streams.borrow()[0]
                .send(SessionEvent::TextDelta {
                    text: format!("later block {line}\n\n"),
                })
                .unwrap();
        }
        tick(cx);
        cx.update(|window, cx| {
            view.read_with(cx, |view, cx| {
                assert!(!view.panes[0].tool_targeted("toolu_9"));
                assert!(view.panes[0].composer.focus_handle(cx).is_focused(window));
            });
        });
    }

    /// #15 review, at the rendered window (#27): streaming slides the
    /// window of Blocks a Pane draws, shifting every rendered position — a
    /// selection stored as positions would quietly slide onto rows the
    /// operator never touched. Ids pin it; an endpoint that leaves the
    /// window clamps the copy to the window start; with both ends gone the
    /// selection dies instead of resurrecting elsewhere.
    #[gpui::test]
    fn an_evicted_native_selection_clears_instead_of_sliding_onto_later_blocks(
        cx: &mut TestAppContext,
    ) {
        let (core, fake) = cockpit("select-evict", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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
        // The counts below straddle Level::Transcript's 200-Block rendered
        // window: past 200 total, the window's start slides.
        say(0, 60);
        tick(cx);
        // Wheel to the very top, where Blocks 5..=7 are on screen.
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(500.), px(350.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(100000.))),
            modifiers: gpui::Modifiers::none(),
            touch_phase: gpui::TouchPhase::default(),
        });
        cx.run_until_parked();
        let texts: Vec<String> = (5..=7).map(|line| format!("filler {line:04}")).collect();
        let from = caret(&view, cx, 5, 0);
        let mut to = caret(&view, cx, 7, texts[2].len() - 1);
        // Past the line's right edge: the nearest-index clamp takes the
        // last character too.
        to.x += px(40.);
        cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(clipboard(cx).as_deref(), Some(texts.join("\n").as_str()));

        // A native document that leaves the rendered window loses its
        // selection. Copy must never silently move onto replacement text.
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("kept".into())));
        say(60, 203);
        tick(cx);
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(clipboard(cx).as_deref(), Some("kept"));
        cx.update(|window, cx| assert!(!gpui::base::TextSelection::has_selection(window, cx)));
        say(203, 208);
        tick(cx);
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(
            clipboard(cx).as_deref(),
            Some("kept"),
            "evicted selection must not resurrect on other Blocks"
        );
    }

    /// #15 review: a drag in a scrolled-back transcript selects the
    /// characters under the pointer — TextLayout records screen-space
    /// geometry each frame, so the hit test needs no offset math of its
    /// own (#27), and must not land on the rows at those coordinates in
    /// the unscrolled layout.
    #[gpui::test]
    fn a_drag_in_a_scrolled_back_transcript_selects_the_rows_under_the_pointer(
        cx: &mut TestAppContext,
    ) {
        let (core, fake) = cockpit("select-scrolled", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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
        cx.run_until_parked();
        // The first row fully inside the viewport — nonzero, or the wheel
        // did not actually scroll anything back.
        let viewport = view.read_with(cx, |view, _| view.panes[0].scroll.bounds());
        let row = (0..58)
            .find(|row| caret(&view, cx, *row, 0).y > viewport.top() + px(20.))
            .expect("a paragraph in the viewport");
        assert!(row > 0, "the wheel put earlier rows above the viewport");
        // All 60 Blocks render, so row indices are block indices here.
        let expected: Vec<String> = (row..=row + 2)
            .map(|line| format!("history line {line:02}"))
            .collect();
        let from = caret(&view, cx, row, 0);
        let mut to = caret(&view, cx, row + 2, expected[2].len() - 1);
        to.x += px(40.);

        cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");

        assert_eq!(clipboard(cx).as_deref(), Some(expected.join("\n").as_str()));
    }

    /// AC3: one key walks to whoever is waiting, wherever they are in the grid.
    #[gpui::test]
    fn one_key_jumps_to_the_pane_that_needs_answering(cx: &mut TestAppContext) {
        let (mut core, fake) = cockpit("jump", 4);
        let group = group_all(&mut core);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-d", NextDecision, None),
                KeyBinding::new("cmd-]", NextPane, None),
            ]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, cx| view.enter_group(group, cx));
        fake.streams.borrow()[2].send(decision("perm_03")).unwrap();
        tick(cx);
        view.read_with(cx, |view, _| assert_eq!(view.focused(), 0));

        cx.simulate_keystrokes("cmd-d");

        view.read_with(cx, |view, _| {
            assert_eq!(view.focused(), 2, "focus should land on the blocked Pane");
        });

        // And plain cycling still walks the grid in order.
        cx.simulate_keystrokes("cmd-]");
        view.read_with(cx, |view, _| assert_eq!(view.focused(), 3));
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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
        let (mut core, _fake) = cockpit("fullscreen", 4);
        let group = group_all(&mut core);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-f", ToggleFullscreen, None)]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, cx| view.enter_group(group, cx));
        // Four Panes in this window sit at Instruments (a 2×2 board of
        // ~273px columns beside the nav). An L2 cell keeps its Composer,
        // so the keys already land — the premise is the level, not the
        // absence of a prompt line.
        cx.simulate_resize(gpui::size(px(860.), px(500.)));
        tick(cx);
        cx.simulate_input("kept");
        let typed = view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.take(cx))
        });
        assert_eq!(typed, "kept", "an L2 cell's Composer takes the keys");

        cx.simulate_keystrokes("cmd-f");

        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.roster().fullscreen(),
                Some(PaneIdentity::Thread(view.panes[0].thread().unwrap())),
                "cmd-f fullscreens the focused Pane"
            );
        });
        // One Pane rendered, spanning the whole area right of the nav —
        // a 2-column cell would be under 300px here.
        let width = view.read_with(cx, |view, _| view.panes[0].scroll.bounds().size.width);
        assert!(
            width > px(500.),
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
            assert_eq!(
                view.cockpit.roster().fullscreen(),
                None,
                "cmd-f again restores the grid"
            );
        });
        cx.simulate_input("still");
        let typed = view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.take(cx))
        });
        assert_eq!(
            typed, "still",
            "back on the grid, the L2 Composer still takes the keys"
        );
    }

    #[gpui::test]
    fn cmd_f_reaches_a_draft_from_a_dense_grid(cx: &mut TestAppContext) {
        let (mut core, _fake) = cockpit("draft-fullscreen", 5);
        for thread in core.threads() {
            core.park(thread).unwrap();
        }
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-f", ToggleFullscreen, None)]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, cx| {
            for _ in 0..5 {
                view.open_draft(DraftTarget::Main, cx);
            }
        });
        cx.simulate_resize(gpui::size(px(500.), px(320.)));
        tick(cx);

        cx.simulate_keystrokes("cmd-f");
        cx.simulate_input("reachable");

        view.read_with(cx, |view, cx| {
            assert!(matches!(
                view.cockpit.roster().fullscreen(),
                Some(PaneIdentity::Draft(_))
            ));
            assert_eq!(
                view.panes[view.focused()].composer.read(cx).text(),
                "reachable"
            );
        });
    }

    #[gpui::test]
    fn launch_provider_seeds_the_first_empty_store_draft(cx: &mut TestAppContext) {
        let fake = Fake::default();
        let store = Store::open(scratch("launch-provider-draft")).unwrap();
        let core = Cockpit::new(store, Box::new(fake));
        let (view, cx) = add_cockpit_window(cx, |_, cx| {
            CockpitView::new_with_provider(core, Provider::Codex, cx)
        });

        view.read_with(cx, |view, _| {
            assert_eq!(
                *view.panes[0].draft().unwrap().binding.provider(),
                ProviderChoice {
                    provider: Provider::Codex,
                    model: None,
                }
            );
        });
    }

    /// A launch that opens nothing — the newest Thread would not revive,
    /// or the operator had parked everything — still lists every parked
    /// Thread from the first frame. There is no later change to wait for.
    #[gpui::test]
    fn a_launch_that_opens_nothing_still_lists_the_parked_threads(cx: &mut TestAppContext) {
        let dir = scratch("launch-parked");
        let parked = {
            // An earlier run: one Thread opened, then the process gone.
            let store = Store::open(dir.clone()).unwrap();
            let mut earlier = Cockpit::new(store, Box::new(Fake::default()));
            earlier
                .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
                .unwrap();
            earlier.threads()[0]
        };
        let store = Store::open(dir).unwrap();
        let core = Cockpit::new(store, Box::new(Fake::default()));
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));

        view.read_with(cx, |view, _| {
            assert!(
                view.panes[0].draft().is_some(),
                "nothing revived: the launch is a draft"
            );
            assert_eq!(
                view.facts.parked().to_vec(),
                vec![parked],
                "and the parked Thread has its row"
            );
        });
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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
        let (mut core, _fake) = cockpit("fullscreen-page", 3);
        let group = group_all(&mut core);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-f", ToggleFullscreen, None),
                KeyBinding::new("cmd-]", NextPane, None),
            ]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, cx| view.enter_group(group, cx));
        tick(cx);
        cx.simulate_keystrokes("cmd-f");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.roster().fullscreen(),
                Some(PaneIdentity::Thread(view.panes[0].thread().unwrap()))
            );
        });

        cx.simulate_keystrokes("cmd-]");

        view.read_with(cx, |view, _| {
            assert_eq!(view.focused(), 1, "cmd-] still walks the Threads");
            assert_eq!(
                view.cockpit.roster().fullscreen(),
                Some(PaneIdentity::Thread(view.panes[1].thread().unwrap())),
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        cx.simulate_keystrokes("cmd-f");
        let closed = view.read_with(cx, |view, _| {
            assert!(
                view.cockpit.roster().fullscreen().is_some(),
                "the premise: fullscreen is on"
            );
            view.panes[0].thread().unwrap()
        });

        cx.simulate_keystrokes("cmd-w");

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1, "the Pane is gone");
            assert_eq!(
                view.cockpit.roster().fullscreen(),
                Some(PaneIdentity::Thread(view.panes[0].thread().unwrap())),
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        cx.simulate_keystrokes("cmd-f");
        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit.roster().fullscreen().is_some(),
                "the premise: fullscreen is on"
            );
        });

        cx.simulate_keystrokes("cmd-w");

        view.read_with(cx, |view, _| {
            assert!(view.panes.is_empty(), "the last Pane is gone");
            assert_eq!(
                view.cockpit.roster().fullscreen(),
                None,
                "and so is the fullscreen"
            );
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        cx.simulate_keystrokes("cmd-f");
        let gone = view.read_with(cx, |view, _| {
            assert!(
                view.cockpit.roster().fullscreen().is_some(),
                "the premise: fullscreen is on"
            );
            view.panes[0].thread().unwrap()
        });

        // Park it the way code that never heard of fullscreen would.
        view.update(cx, |view, cx| {
            view.cockpit.park(gone).unwrap();
            view.sync_panes(cx);
            cx.notify();
        });
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.roster().fullscreen(),
                None,
                "a fullscreened Thread that vanished falls back to the grid"
            );
            assert_eq!(view.panes.len(), 1, "with the surviving Thread on it");
        });
    }

    /// #21 AC1: the nav lists every Thread — most recently used first,
    /// open and parked alike — each row naming its Project, its checkout
    /// and its provider. There is no section header between them: one
    /// list, split only by Group membership.
    #[gpui::test]
    fn the_nav_lists_every_thread_most_recently_used_first(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("nav-order", 3);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-w", CloseThread, None)]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        let (grid_order, parked_thread) = view.read_with(cx, |view, _| {
            (
                view.panes
                    .iter()
                    .filter_map(|pane| pane.thread())
                    .collect::<Vec<_>>(),
                view.panes[1].thread().unwrap(),
            )
        });

        view.update(cx, |view, _| view.focus_pane(1));
        cx.simulate_keystrokes("cmd-w");

        view.read_with(cx, |view, _| {
            let state = view.nav_state();
            assert!(state.groups.is_empty(), "no Group claims these Threads");
            let ordered = state.ordered_solos();
            let rows: Vec<ThreadId> = ordered.iter().map(|row| row.thread).collect();
            let mut expected: Vec<ThreadId> = grid_order.to_vec();
            // The nav's default order, and the only one it has: last used
            // first. Parking does not move a row — using it does.
            expected.sort_by_key(|thread| std::cmp::Reverse(view.last_used(*thread)));
            assert_eq!(rows, expected, "most recently used first");
            assert!(
                rows.contains(&parked_thread),
                "a parked Thread is a row like any other"
            );
            assert_eq!(
                ordered[0].name.as_ref(),
                format!("thread-{:02}", expected[0]),
                "rows say what the Pane head says"
            );
            assert!(
                ordered.iter().all(|row| row.last_used.is_some()),
                "every row says how long since it was used"
            );
            assert_eq!(
                ordered[0].provider,
                Some(Provider::Claude),
                "the provider is the logomark's own value, never a `cl` tag"
            );
            let parked: Vec<ThreadId> = view.facts.parked().to_vec();
            assert_eq!(parked, vec![parked_thread], "the parked Thread moved below");
            assert_eq!(
                ordered.last().unwrap().provider,
                Some(Provider::Claude),
                "a parked row still names its provider — peeked, not loaded"
            );
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        view.read_with(cx, |view, _| assert_eq!(view.focused(), 0));

        // Which row is which is the nav's order to decide (last used
        // first), so the test asks it rather than assuming the grid's.
        let row_of = |view: &CockpitView, pane: usize| {
            let thread = view.panes[pane].thread().unwrap();
            view.nav_state()
                .ordered_solos()
                .iter()
                .position(|row| row.thread == thread)
                .expect("every open Thread has a row")
        };
        // Row `n`: the 42px window band, the 42px nav head, the tree's 8px
        // inset, n rows of 56.5px each with the 2px between siblings, then
        // halfway down its own row. No strip, no section header.
        let row_y =
            |n: usize| px(42. + 42. + 8. + n as f32 * (crate::theme::THREAD_ROW_H + 2.) + 28.);
        let (second, first) = view.read_with(cx, |view, _| (row_of(view, 1), row_of(view, 0)));
        cx.simulate_click(
            gpui::point(px(104.), row_y(second)),
            gpui::Modifiers::none(),
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.focused(), 1, "the click moved focus to the row's Pane");
        });

        cx.simulate_keystrokes("cmd-f");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.roster().fullscreen(),
                Some(PaneIdentity::Thread(view.panes[1].thread().unwrap()))
            );
        });
        // And back to the other Pane's row.
        cx.simulate_click(gpui::point(px(104.), row_y(first)), gpui::Modifiers::none());
        view.read_with(cx, |view, _| {
            assert_eq!(view.focused(), 0, "the nav still answers while fullscreen");
            assert_eq!(
                view.cockpit.roster().fullscreen(),
                Some(PaneIdentity::Thread(view.panes[0].thread().unwrap())),
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        let parked = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        cx.simulate_keystrokes("cmd-w");
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1);
            assert_eq!(view.facts.parked().len(), 1, "the parked Thread got a row");
        });

        // The parked row sits second in one undivided list: the 42px window
        // band, the 42px nav head, the tree's 8px inset, one 56.5px row and
        // the 2px between siblings, then halfway down its own row.
        cx.simulate_click(
            gpui::point(
                px(104.),
                px(42. + 42. + 8. + crate::theme::THREAD_ROW_H + 2. + 28.),
            ),
            gpui::Modifiers::none(),
        );

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 2, "the revived Thread got a Pane");
            assert_eq!(
                view.panes[1].thread().unwrap(),
                parked,
                "and it is the same Thread"
            );
            assert_eq!(view.focused(), 1, "focus followed the revival");
            assert!(view.facts.parked().is_empty(), "its nav row moved up");
        });

        // cmd-o must not bring back a Thread the nav already revived.
        cx.simulate_keystrokes("cmd-o");
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 2, "nothing was left parked to reopen");
        });
    }

    /// #21 AC3 is **overruled by the approved prototype**: navigation says
    /// nothing about state. A blocked Thread and a running one are the same
    /// row — no dot, no amber, no count, no chip — and the collapsed rail
    /// says no more. A Decision is visible where it is answered: the Pane's
    /// border, its signal line and its Decision card.
    #[gpui::test]
    fn a_pending_decision_changes_nothing_in_the_nav(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("nav-amber", 2);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-b", ToggleNav, None)]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        let before = view.read_with(cx, |view, _| describe_nav(&view.nav_state()));

        fake.streams.borrow()[1].send(decision("perm_02")).unwrap();
        tick(cx);

        view.read_with(cx, |view, _| {
            assert_eq!(
                describe_nav(&view.nav_state()),
                before,
                "a pending Decision moves no ink in navigation"
            );
        });

        // And the rail says no more than the column did.
        cx.simulate_keystrokes("cmd-b");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let state = view.nav_state();
            assert!(state.collapsed);
            assert_eq!(describe_nav(&state), before);
        });
    }

    /// Everything a nav row can say, as one comparable string. If a state
    /// signal ever creeps back in, this is what catches it.
    fn describe_nav(state: &nav::NavState) -> Vec<String> {
        state
            .ordered_rows()
            .into_iter()
            .map(|row| {
                format!(
                    "{}|{:?}|{:?}|{:?}",
                    row.name, row.project, row.branch, row.provider
                )
            })
            .collect()
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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

    /// The production key table, loaded whole in the mac spelling — so the
    /// popovers' same-depth tie-breaks are tested against exactly the order
    /// launch binds.
    fn bind_production_keys(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let bindings = crate::load_bindings(crate::keymap::Platform::Mac, cx);
            cx.bind_keys(bindings);
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

    #[gpui::test]
    fn history_arrows_only_replace_the_composer_and_restore_the_exact_draft(
        cx: &mut TestAppContext,
    ) {
        let (mut core, fake) = cockpit("prompt-history-view", 1);
        let thread = core.threads()[0];
        core.send(thread, "one".into());
        core.send(thread, "two".into());
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.set("  thr…  ".into(), cx));
        });
        tick(cx);

        for (key, expected) in [("up", "two"), ("up", "one")] {
            cx.simulate_keystrokes(key);
            tick(cx);
            assert_eq!(composer_text(&view, cx), expected);
        }
        view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.set("one edited".into(), cx));
        });
        for (key, expected) in [
            ("up", "one edited"),
            ("down", "two"),
            ("down", "  thr…  "),
            ("down", "  thr…  "),
        ] {
            cx.simulate_keystrokes(key);
            tick(cx);
            assert_eq!(composer_text(&view, cx), expected);
        }
        assert_eq!(fake.sent.borrow().as_slice(), ["one", "two"]);
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit
                    .thread(thread)
                    .map(|open| open.transcript())
                    .unwrap()
                    .blocks()
                    .iter()
                    .filter(|block| matches!(block.body, Body::Prompt(_)))
                    .count(),
                2
            );
        });

        cx.simulate_keystrokes("up");
        cx.simulate_input(" please");
        cx.simulate_keystrokes("enter");
        tick(cx);
        assert_eq!(
            fake.sent.borrow().as_slice(),
            ["one", "two", "two please"],
            "editing a recall uses the ordinary send path exactly once"
        );
    }

    #[gpui::test]
    fn recalling_a_slash_prompt_does_not_open_its_menu_until_a_real_edit(cx: &mut TestAppContext) {
        let (mut core, _) = cockpit("prompt-history-slash", 1);
        let thread = core.threads()[0];
        core.send(thread, "older".into());
        core.send(thread, "/".into());
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);

        cx.simulate_keystrokes("up");
        tick(cx);
        assert_eq!(composer_text(&view, cx), "/");
        view.read_with(cx, |view, _| {
            assert!(
                view.popover.is_none(),
                "recall itself must not derive a menu"
            )
        });
        cx.simulate_keystrokes("up");
        tick(cx);
        assert_eq!(composer_text(&view, cx), "older");

        cx.simulate_keystrokes("down");
        cx.simulate_input("m");
        tick(cx);
        assert_eq!(composer_text(&view, cx), "/m");
        view.read_with(cx, |view, _| {
            assert!(
                view.popover.is_some(),
                "the next real edit restores menu derivation"
            )
        });
    }

    #[gpui::test]
    fn menu_and_decision_states_disarm_history_before_their_arrows_dispatch(
        cx: &mut TestAppContext,
    ) {
        let (mut core, fake) = cockpit("prompt-history-precedence", 1);
        let thread = core.threads()[0];
        core.send(thread, "history".into());
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        cx.update(|window, cx| {
            assert!(view
                .read(cx)
                .history_available(0, view.read(cx).level_now(window)))
        });

        view.update(cx, |view, cx| {
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.set("/".into(), cx));
        });
        tick(cx);
        view.read_with(cx, |view, _| assert!(view.popover.is_some()));
        cx.update(|window, cx| {
            assert!(!view
                .read(cx)
                .history_available(0, view.read(cx).level_now(window)))
        });
        cx.simulate_keystrokes("up");
        assert_eq!(composer_text(&view, cx), "/", "menu owns the arrow");

        view.update(cx, |view, cx| {
            view.popover = None;
            view.panes[0]
                .composer
                .update(cx, |composer, cx| composer.set("draft".into(), cx));
        });
        fake.streams.borrow()[0]
            .send(decision("history-decision"))
            .unwrap();
        tick(cx);
        cx.update(|window, cx| {
            assert!(!view
                .read(cx)
                .history_available(0, view.read(cx).level_now(window)))
        });
        cx.simulate_keystrokes("up");
        assert_eq!(composer_text(&view, cx), "draft", "Decision owns its state");
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

        // A directory row keeps its trailing slash in the name and inserts
        // whole, so `@src/nested/ ` reads as the folder it is.
        let dirs = vec!["src/".to_string(), "src/nested/".to_string()];
        let rows = mention_rows(&dirs, "nest");
        assert_eq!(rows[0].name.as_ref(), "nested/");
        assert_eq!(rows[0].detail.as_ref(), "src");
        assert_eq!(rows[0].insert.as_ref(), "src/nested/");
    }

    /// #23: `/` at the line's start opens the Session's own menu, typing
    /// filters it, ↓/↵ pick — and the pick lands as `/name ` ready for args.
    #[gpui::test]
    fn typing_slash_opens_the_command_menu_and_enter_inserts_the_pick(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("slash-menu", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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
                view.popover.is_none(),
                "nothing opens until the operator types"
            );
        });

        cx.simulate_input("/");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.popover.as_ref().expect("/ opens the menu");
            // Everything the Session listed — plus, on this still-fresh
            // Thread, Ferrite's own model (#25), effort and import (#11)
            // entries on top.
            assert_eq!(menu.rows.len(), 7);
            assert_eq!(menu.selected, 0);
        });

        cx.simulate_input("co");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.popover.as_ref().expect("still open while filtering");
            let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(names, ["/code-review", "/commit", "/compact"]);
        });

        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.popover.as_ref().expect("open").selected, 1);
        });

        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(composer_text(&view, cx), "/commit ");
        view.read_with(cx, |view, _| {
            assert!(view.popover.is_none(), "the pick closed the menu");
        });
    }

    /// Escape closes the menu and only the menu: the text stays, escape's
    /// Interrupt meaning waits for the next press, and more typing reopens.
    #[gpui::test]
    fn escape_dismisses_the_menu_keeps_the_text_and_typing_reopens(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("slash-escape", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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
        view.read_with(cx, |view, _| assert!(view.popover.is_some()));

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.popover.is_none(), "escape dismissed the popover");
        });
        assert_eq!(composer_text(&view, cx), "/c", "and kept the text");

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                view.popover.is_none(),
                "a second escape is Interrupt, not a reopen"
            );
        });

        cx.simulate_input("o");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.popover.is_some(), "typing again reopens the menu");
        });
    }

    /// #23: `@` opens the file menu over the Thread's workspace binding;
    /// the pick lands as `@relative/path ` in the line.
    #[gpui::test]
    fn typing_at_completes_files_from_the_workspace_binding(cx: &mut TestAppContext) {
        let (core, _fake, _checkout) = bound_cockpit("mention-menu", Provider::Claude);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("read ");
        cx.simulate_input("@");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.popover.as_ref().expect("@ opens the file menu");
            let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(
                names,
                ["README.md", "src/", "lib.rs"],
                "the walk, breadth-first, folders too"
            );
        });

        cx.simulate_input("li");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.popover.as_ref().expect("open");
            assert_eq!(menu.rows.len(), 1, "the fuzzy filter narrowed it");
            assert_eq!(menu.rows[0].insert.as_ref(), "src/lib.rs");
        });

        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(composer_text(&view, cx), "read @src/lib.rs ");
        view.read_with(cx, |view, cx| {
            assert!(view.popover.is_none());
            // The pill is provider-agnostic: a Claude pick paints it too —
            // the wire stays plain `@path` text the CLI itself reads.
            assert_eq!(
                view.panes[0].composer.read(cx).mentions(),
                [SharedString::from("@src/lib.rs")],
                "the picked token is staged as the comp's pill"
            );
        });
    }

    #[gpui::test]
    fn draft_mentions_are_scoped_to_the_selected_project(cx: &mut TestAppContext) {
        let project = scratch("draft-mentions-project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("only-here.txt"), "project\n").unwrap();
        let elsewhere = scratch("draft-mentions-elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("not-here.txt"), "elsewhere\n").unwrap();
        let fake = Fake::default();
        let store = Store::open(scratch("draft-mentions-store")).unwrap();
        let core = Cockpit::new(store, Box::new(fake));
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.aim_launch(&project));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("@");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let names: Vec<&str> = view
                .popover
                .as_ref()
                .expect("draft @ menu")
                .rows
                .iter()
                .map(|row| row.name.as_ref())
                .collect();
            assert_eq!(names, ["only-here.txt"]);
        });
    }

    #[gpui::test]
    fn draft_mentions_follow_the_selected_workspace(cx: &mut TestAppContext) {
        let base = scratch("draft-mention-workspace");
        let repo = repo_in(&base);
        std::fs::write(repo.join("main-only.txt"), "main\n").unwrap();
        let fake = Fake::default();
        let store = Store::open(base.join("threads")).unwrap();
        let core = Cockpit::new(store, Box::new(fake));
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| {
            view.aim_launch(&repo);
            let thread = view
                .cockpit
                .open(
                    Provider::Claude,
                    WorkspaceChoice::NewWorktree { repo: repo.clone() },
                )
                .unwrap();
            view.cockpit.park(thread).unwrap();
            let focused = view.focused();
            let entry = view.panes[focused].draft().unwrap().binding.project();
            let worktree = view.cockpit.registry().worktrees(entry)[0].clone();
            std::fs::write(worktree.path.join("worktree-only.txt"), "tree\n").unwrap();
            view.panes[focused]
                .draft_mut()
                .unwrap()
                .binding
                .choose_target(DraftTarget::Existing {
                    branch: worktree.branch,
                });
        });
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("@");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let names: Vec<&str> = view
                .popover
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .map(|r| r.name.as_ref())
                .collect();
            assert!(names.contains(&"worktree-only.txt"), "rows: {names:?}");
        });

        view.update(cx, |view, cx| {
            view.panes[view.focused()]
                .composer
                .update(cx, |composer, cx| composer.set(String::new(), cx));
            let focused = view.focused();
            view.panes[focused]
                .draft_mut()
                .unwrap()
                .binding
                .choose_target(DraftTarget::New);
        });
        cx.simulate_input("@");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let names: Vec<&str> = view
                .popover
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .map(|r| r.name.as_ref())
                .collect();
            assert!(names.contains(&"main-only.txt"), "rows: {names:?}");
            assert!(!names.contains(&"worktree-only.txt"), "rows: {names:?}");
        });

        // Pick the existing worktree through its band row, complete its
        // unique file, and send: the resulting Thread keeps that same
        // directory, not the Project checkout.
        view.update(cx, |view, cx| {
            view.panes[view.focused()]
                .composer
                .update(cx, |composer, cx| composer.set(String::new(), cx));
            view.open_band_popover(pane::BandChip::Workspace, cx);
            view.pick(1, cx);
        });
        cx.simulate_input("@worktree-only");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        assert_eq!(composer_text(&view, cx), "@worktree-only.txt ");
        cx.simulate_keystrokes("enter");
        tick(cx);
        view.read_with(cx, |view, _| {
            let thread = view.panes[view.focused()]
                .thread()
                .expect("first send bound the Thread");
            let cwd = view
                .cockpit
                .thread(thread)
                .unwrap()
                .workspace()
                .unwrap()
                .cwd();
            assert!(cwd.join("worktree-only.txt").is_file());
            assert!(!cwd.join("main-only.txt").exists());
        });
    }

    #[gpui::test]
    fn a_stale_draft_workspace_neither_completes_nor_sends_from_main(cx: &mut TestAppContext) {
        let base = scratch("draft-stale-workspace");
        let repo = repo_in(&base);
        std::fs::write(repo.join("main-only.txt"), "main\n").unwrap();
        let fake = Fake::default();
        let core = Cockpit::new(
            Store::open(base.join("threads")).unwrap(),
            Box::new(fake.clone()),
        );
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| {
            view.aim_launch(&repo);
            view.focused_draft_mut()
                .unwrap()
                .binding
                .choose_target(DraftTarget::Existing {
                    branch: "removed-worktree".into(),
                });
        });
        tick(cx);
        cx.simulate_input("@main-only");
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.popover.is_none()));
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            let draft = view.panes[view.focused()].draft().expect("still a draft");
            assert!(draft.error.as_ref().unwrap().contains("removed-worktree"));
        });
        assert_eq!(composer_text(&view, cx), "@main-only");
        assert!(fake.streams.borrow().is_empty());
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
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("@");
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(view.popover.is_none(), "no binding, no popover");
        });
        assert_eq!(composer_text(&view, cx), "@", "typing was not eaten");
    }

    /// #24's dismissal law holds for the menus: a press the popover did not
    /// swallow closes it, and it stays shut until the text moves.
    #[gpui::test]
    fn a_press_on_the_transcript_dismisses_the_open_menu(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("menu-press-dismiss", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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
        view.read_with(cx, |view, _| assert!(view.popover.is_some()));

        // The middle of the Pane's transcript — nowhere near the popover.
        cx.simulate_mouse_down(
            gpui::point(px(600.), px(200.)),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(view.popover.is_none(), "the press dismissed the popover");
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
        bind_production_keys(cx);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
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
            assert!(
                view.cockpit
                    .thread(thread)
                    .and_then(|open| open.pending())
                    .is_some(),
                "the card is up"
            );
            assert!(
                view.cockpit.thread(thread).is_some_and(|open| open.busy()),
                "the turn is running"
            );
        });

        // The input is still live: typing lands, enter queues behind the
        // turn. (The first key of an empty line is where y/n/a mean their
        // keycaps, so the sentence starts past them.)
        cx.simulate_input("fix the tests too");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.thread(thread).and_then(|open| open.queued()),
                Some("fix the tests too")
            );
            assert!(
                view.cockpit
                    .thread(thread)
                    .and_then(|open| open.pending())
                    .is_some(),
                "typing answered nothing"
            );
        });

        // Emptied, y is the keycap's answer.
        cx.simulate_keystrokes("y");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit
                    .thread(thread)
                    .and_then(|open| open.pending())
                    .is_none(),
                "y on the empty line answered the Decision"
            );
        });
    }

    /// The other half of the y/n/a rule: with text on the line they are
    /// letters — an operator half-way through a word keeps typing it.
    #[gpui::test]
    fn the_answer_keys_stay_letters_while_the_line_holds_text(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("decision-letters", 1);
        bind_production_keys(cx);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
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
                view.cockpit
                    .thread(thread)
                    .and_then(|open| open.pending())
                    .is_some(),
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
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit
                    .thread(thread)
                    .and_then(|open| open.permission_mode()),
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
            assert_eq!(
                view.cockpit
                    .thread(thread)
                    .and_then(|open| open.permission_mode()),
                Some("acceptEdits")
            );
        });
    }

    /// #23: on a Codex Thread a picked file also stages the @-pill — the
    /// send will carry the typed mention item, and the input paints the
    /// token as the comp draws it.
    #[gpui::test]
    fn picking_a_mention_on_a_codex_thread_stages_the_pill(cx: &mut TestAppContext) {
        let (core, _fake, _checkout) = bound_cockpit("mention-codex", Provider::Codex);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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

    // ------------------------------------------------------ Session import (#11)

    /// Fake vendor session roots under a scratch base — the shapes the
    /// vendors write (project slugs, date directories), never a real home.
    fn session_roots(base: &std::path::Path) -> Vec<(Provider, std::path::PathBuf)> {
        vec![
            (Provider::Claude, base.join("claude-projects")),
            (Provider::Codex, base.join("codex-sessions")),
        ]
    }

    /// A session file `age_secs` old: written, then stamped with the mtime
    /// discovery orders by.
    fn write_session_file(path: &std::path::Path, contents: &str, age_secs: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
        let modified = std::time::SystemTime::now() - Duration::from_secs(age_secs);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }

    /// A minimal claude-shaped session body: the import contract's own
    /// cut-down lines, stamped with `session`.
    fn claude_session_body(session: &str) -> String {
        concat!(
            r#"{"type":"user","sessionId":"SESSION","cwd":"/workspace","message":{"role":"user","content":"first question"}}"#,
            "\n",
            r#"{"type":"assistant","sessionId":"SESSION","message":{"model":"claude-haiku-4-5","content":[{"type":"text","text":"first answer"}]}}"#,
            "\n",
        )
        .replace("SESSION", session)
    }

    /// #11: on a fresh Thread `/` lists Ferrite's own `import` entry — the
    /// whole menu before the provider announces commands, the top row
    /// alongside them after — and the first real conversation retires it.
    #[gpui::test]
    fn a_fresh_thread_lists_the_local_import_entry_atop_the_slash_menu(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("import-entry", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        // No provider menu yet: the local entries are the whole list —
        // #25's model door on top, then the import door.
        cx.simulate_input("/");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view
                .popover
                .as_ref()
                .expect("/ offers import on a fresh Thread");
            let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(names, ["/model", "/effort", "/import"]);
            assert_eq!(menu.rows[0].detail.as_ref(), "switch provider / model");
            assert_eq!(menu.rows[2].detail.as_ref(), "adopt a CLI session file");
        });

        // The Session announces its own commands: import rides on top, and
        // the fuzzy filter treats it like any row (`co` is not in "import").
        fake.streams.borrow()[0]
            .send(SessionEvent::Commands {
                commands: menu_commands(),
            })
            .unwrap();
        tick(cx);
        cx.simulate_input("co");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.popover.as_ref().expect("open");
            let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(names, ["/code-review", "/commit", "/compact"]);
        });
        cx.simulate_keystrokes("backspace backspace");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.popover.as_ref().expect("open");
            let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(
                names,
                [
                    "/model",
                    "/effort",
                    "/import",
                    "/code-review",
                    "/commit",
                    "/compact",
                    "/to-tickets"
                ],
                "the local entries ride atop the provider's own menu"
            );
        });

        // A conversation starts: the import door closes; the model row
        // stays live — the model can change any time — and says what it
        // does now that the provider is fixed.
        cx.simulate_keystrokes("backspace");
        cx.simulate_input("hello");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        cx.simulate_input("/");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.popover.as_ref().expect("the model menu still lists");
            assert!(
                menu.rows.iter().all(|row| row.name.as_ref() != "/import"),
                "a Thread with history offers no import"
            );
            assert_eq!(menu.rows[0].name.as_ref(), "/model");
            assert!(!menu.rows[0].inert, "the model door stays open");
            assert_eq!(
                menu.rows[0].detail.as_ref(),
                "switch model · hand over to the other provider"
            );
            assert_eq!(menu.rows[1].name.as_ref(), "/effort");
            assert_eq!(menu.rows.len(), 6);
        });
    }

    /// #11: picking `import` is Ferrite's own act — the line is cleared,
    /// nothing goes near the provider, and the file-pick popover lists both
    /// vendors' session files newest first with provider and age per row.
    #[gpui::test]
    fn picking_import_opens_the_file_picker_and_sends_the_provider_nothing(
        cx: &mut TestAppContext,
    ) {
        let (core, _fake) = cockpit("import-picker", 1);
        let base = scratch("import-picker-roots");
        let roots = session_roots(&base);
        write_session_file(
            &roots[0].1.join("-workspace-alpha").join("aaaa.jsonl"),
            &claude_session_body("aaaa"),
            3 * 60 * 60,
        );
        write_session_file(
            &roots[0].1.join("-workspace-beta").join("bbbb.jsonl"),
            &claude_session_body("bbbb"),
            60,
        );
        write_session_file(
            &roots[1]
                .1
                .join("2026")
                .join("08")
                .join("25")
                .join("rollout-cccc.jsonl"),
            "not read by discovery\n",
            30 * 60,
        );
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.session_file_roots = roots);
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

        // `im` filters to the import door — the provider door (#25) rides
        // above it on a bare `/`.
        cx.simulate_input("/im");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        assert_eq!(
            composer_text(&view, cx),
            "",
            "the pick never lands as slash text"
        );
        view.read_with(cx, |view, _| {
            let picker = view.popover.as_ref().expect("the file picker is open");
            let names: Vec<&str> = picker
                .rows
                .iter()
                .map(|pick| pick.row.name.as_ref())
                .collect();
            assert_eq!(names, ["claude", "codex", "claude"], "newest first");
            let detail = |at: usize| picker.rows[at].row.detail.as_ref();
            assert!(detail(0).contains("bbbb.jsonl"));
            assert!(detail(0).contains("1m ago"));
            assert!(detail(1).contains("rollout-cccc.jsonl"));
            assert!(detail(2).contains("3h ago"));
            // The row and the file it adopts ride together.
            assert!(matches!(
                &picker.rows[0].consequence,
                Consequence::Adopt(path) if path.ends_with("bbbb.jsonl")
            ));
            assert_eq!(picker.selected, 0);
            // Nothing reached the provider: no prompt, no running turn.
            let transcript = view.cockpit.thread(thread).unwrap().transcript();
            assert!(
                !transcript
                    .blocks()
                    .iter()
                    .any(|block| matches!(block.body, Body::Prompt(_))),
                "picking import must not prompt the provider"
            );
            assert!(!view.cockpit.thread(thread).is_some_and(|open| open.busy()));
        });

        // The arrows walk the rows; escape dismisses with the keyboard
        // still in the Composer.
        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.popover.as_ref().expect("open").selected, 1);
        });
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.popover.is_none(), "escape dismissed the picker");
        });
        cx.simulate_input("still typing");
        assert_eq!(
            composer_text(&view, cx),
            "still typing",
            "the keyboard never left the Composer"
        );
    }

    /// #11 AC: enter adopts the picked session through the core door — the
    /// imported Thread opens focused with the conversation replayed and its
    /// resume target set, and the blank Thread the door was opened from is
    /// gone (clean by the picker's own invariant).
    #[gpui::test]
    fn enter_adopts_the_picked_session_and_the_blank_thread_goes(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("import-adopt", 1);
        let base = scratch("import-adopt-roots");
        let roots = session_roots(&base);
        write_session_file(
            &roots[0].1.join("-workspace-alpha").join("adopt-4f2a.jsonl"),
            &claude_session_body("adopt-4f2a"),
            60,
        );
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.session_file_roots = roots);
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let blank = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

        cx.simulate_input("/im");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(view.popover.is_none(), "the pick closed the picker");
            assert_eq!(view.panes.len(), 1, "one Pane: the adopted Thread");
            let adopted = view.panes[0].thread().unwrap();
            assert_ne!(adopted, blank);
            assert_eq!(
                view.focused_thread(),
                Some(adopted),
                "the adopted Thread takes focus"
            );
            let transcript = view.cockpit.thread(adopted).unwrap().transcript();
            assert_eq!(
                transcript.session_id(),
                Some("adopt-4f2a"),
                "the next prompt resumes the file's own session"
            );
            assert!(
                transcript.blocks().iter().any(
                    |block| matches!(&block.body, Body::Prompt(text) if text == "first question")
                ),
                "the conversation replays: {:?}",
                transcript.blocks()
            );
            // The blank Thread is gone entirely — not parked clutter.
            assert!(view.cockpit.thread(blank).is_none());
            assert!(view.cockpit.parked().unwrap().is_empty());
        });
    }

    /// #11 AC: a malformed or foreign pick surfaces the core's readable
    /// refusal in this Thread's transcript — never a crash — and the door
    /// stays open for the next try.
    #[gpui::test]
    fn a_foreign_file_pick_surfaces_the_refusal_in_the_transcript(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("import-refused", 1);
        let base = scratch("import-refused-roots");
        let roots = session_roots(&base);
        write_session_file(
            &roots[1].1.join("2026").join("junk.jsonl"),
            "not a session file at all\n",
            60,
        );
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.session_file_roots = roots);
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

        cx.simulate_input("/im");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(view.popover.is_none(), "the refusal closed the picker");
            assert_eq!(view.panes.len(), 1);
            assert_eq!(view.panes[0].thread().unwrap(), thread, "the Thread stays");
            let transcript = view.cockpit.thread(thread).unwrap().transcript();
            assert!(
                transcript.blocks().iter().any(|block| matches!(
                    &block.body,
                    Body::Notice(line) if line.contains("not an importable session file")
                )),
                "the core's own words reach the transcript: {:?}",
                transcript.blocks()
            );
            assert!(
                view.cockpit.parked().unwrap().is_empty(),
                "no half-imported Thread was left"
            );
        });

        // The refusal is Ferrite speaking, not a conversation: `/` still
        // offers import for the next try.
        cx.simulate_input("/");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.popover.as_ref().expect("the menu reopens");
            assert!(
                menu.rows.iter().any(|row| row.name.as_ref() == "/import"),
                "the import door stays open"
            );
        });
    }

    /// #11: with nothing to list, picking import says so in the transcript
    /// instead of opening an empty popover.
    #[gpui::test]
    fn with_no_session_files_found_the_pick_says_so(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("import-none", 1);
        let base = scratch("import-none-roots");
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.session_file_roots = session_roots(&base));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

        cx.simulate_input("/im");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(view.popover.is_none(), "nothing to pick from");
            let transcript = view.cockpit.thread(thread).unwrap().transcript();
            assert!(
                transcript.blocks().iter().any(|block| matches!(
                    &block.body,
                    Body::Notice(line) if line.contains("no CLI session files found")
                )),
                "the empty discovery is said out loud: {:?}",
                transcript.blocks()
            );
        });
    }

    #[gpui::test]
    fn draft_import_replaces_the_draft_and_focuses_the_imported_thread(cx: &mut TestAppContext) {
        let fake = Fake::default();
        let store = Store::open(scratch("draft-import-store")).unwrap();
        let core = Cockpit::new(store, Box::new(fake));
        let base = scratch("draft-import-roots");
        let roots = session_roots(&base);
        write_session_file(
            &roots[0].1.join("-workspace-alpha").join("draft.jsonl"),
            &claude_session_body("draft-import"),
            1,
        );
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.session_file_roots = roots);
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("/im");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.popover.as_ref().expect("draft slash menu");
            assert_eq!(menu.rows.len(), 1);
            assert_eq!(menu.rows[0].insert.as_ref(), "import");
        });
        view.update(cx, |view, cx| {
            assert!(
                view.popover_element(0, cx).is_some(),
                "the derived Draft menu reaches its one rendered popover slot"
            );
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.popover.is_some()));
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        view.read_with(cx, |view, cx| {
            assert!(view.panes[0].draft().is_none());
            assert!(view.panes[0].thread().is_some());
            assert_eq!(view.panes[0].composer.read(cx).text(), "");
            assert_eq!(view.focused(), 0);
        });
    }

    #[gpui::test]
    fn refused_draft_import_preserves_the_draft_and_command(cx: &mut TestAppContext) {
        let fake = Fake::default();
        let store = Store::open(scratch("draft-import-refused-store")).unwrap();
        let core = Cockpit::new(store, Box::new(fake));
        let base = scratch("draft-import-refused-roots");
        let roots = session_roots(&base);
        write_session_file(
            &roots[0].1.join("-workspace-alpha").join("bad.jsonl"),
            "not a session\n",
            1,
        );
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.session_file_roots = roots);
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("/im");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        view.read_with(cx, |view, cx| {
            let draft = view.panes[0].draft().expect("the draft survives");
            assert!(draft
                .error
                .as_ref()
                .is_some_and(|error| error.contains("cannot import")));
            assert_eq!(view.panes[0].composer.read(cx).text(), "/im");
        });
    }

    /// The dismissal law holds for the picker: a press the popover did not
    /// swallow closes it, exactly like the selector and the menus.
    #[gpui::test]
    fn a_press_on_the_transcript_dismisses_the_open_picker(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("import-press-dismiss", 1);
        let base = scratch("import-press-roots");
        let roots = session_roots(&base);
        write_session_file(
            &roots[0].1.join("-workspace-alpha").join("aaaa.jsonl"),
            &claude_session_body("aaaa"),
            60,
        );
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.session_file_roots = roots);
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("/im");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.popover.is_some()));

        // The middle of the Pane's transcript — nowhere near the popover.
        cx.simulate_mouse_down(
            gpui::point(px(600.), px(200.)),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(view.popover.is_none(), "the press dismissed it");
        });
    }

    // ------------------------------------------------- Provider choice (#25)

    #[gpui::test]
    fn discovery_updates_an_open_draft_picker_without_starting_a_session(cx: &mut TestAppContext) {
        let fake = Fake::default();
        let (tx, rx) = mpsc::channel();
        *fake.model_discovery.borrow_mut() = Some(rx);
        let core = Cockpit::new(
            Store::open(scratch("discovered-models")).unwrap(),
            Box::new(fake.clone()),
        );
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        view.update(cx, |view, cx| {
            view.open_band_popover(pane::BandChip::Provider, cx)
        });
        let models = vec![ferrite_core::ModelInfo {
            value: "gpt-future-model".into(),
            display: "Future Model".into(),
            detail: "Discovered, never bundled".into(),
            resolved: None,
            efforts: vec!["medium".into(), "ultra".into()],
            default_effort: Some("medium".into()),
        }];
        tx.send((Provider::Codex, models.clone())).unwrap();
        tick(cx);
        let index = view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.model_catalog(Provider::Codex), models);
            view.popover
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .position(|row| row.name.as_ref() == "Future Model")
                .expect("the open picker updates when discovery arrives")
        });
        assert!(
            fake.spawned.borrow().is_empty(),
            "discovery creates no Session"
        );
        view.update(cx, |view, cx| view.pick(index, cx));
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.panes[0]
                    .draft()
                    .unwrap()
                    .binding
                    .provider()
                    .model
                    .as_deref(),
                Some("gpt-future-model")
            );
            assert!(view.cockpit.threads().is_empty());
        });
        assert!(
            fake.spawned.borrow().is_empty(),
            "choosing a model creates no Session"
        );
        tx.send((Provider::Codex, Vec::new())).unwrap();
        drop(tx);
        tick(cx);
        view.update(cx, |view, cx| {
            assert_eq!(
                view.cockpit.model_catalog(Provider::Codex),
                models,
                "an empty discovery keeps the last usable menu"
            );
            view.settings_open = true;
            cx.notify();
        });
        tick(cx);
        let choice = cx
            .debug_bounds("settings-codex-model-0")
            .expect("Settings renders the discovered model")
            .center();
        cx.simulate_click(choice, gpui::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.prefs.settings.codex_model.as_deref(),
                Some("gpt-future-model")
            );
        });
    }

    /// #25 AC: the keyboard-only path. `/` lists the local provider row on
    /// top; ↵ opens the picker — the two Providers with the ✓ on the
    /// current one, and no invented model rows before an announcement —
    /// and ↓↵ picks codex: Ferrite's own act, the Session replaced on the
    /// spot, nothing landing as prompt text.
    #[gpui::test]
    fn the_slash_provider_row_opens_the_picker_and_picks_codex(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("provider-pick", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

        cx.simulate_input("/");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        assert_eq!(
            composer_text(&view, cx),
            "",
            "the pick never lands as slash text"
        );
        let first_codex = view.read_with(cx, |view, _| {
            let picker = view.popover.as_ref().expect("the provider picker is open");
            assert!(matches!(picker.kind, Kind::Provider));
            let names: Vec<&str> = picker
                .rows
                .iter()
                .map(|pick| pick.row.name.as_ref())
                .collect();
            // Two sections, each headed by its Provider, each listing the
            // catalog under the names people say — never a wire id.
            assert_eq!(names[0], "Claude");
            assert!(picker.rows[0].inert, "a section title is not a pick");
            let codex = names
                .iter()
                .position(|name| *name == "Codex")
                .expect("a Codex section");
            assert!(
                codex > 1,
                "Claude's models sit between the titles: {names:?}"
            );
            assert!(
                names
                    .iter()
                    .all(|name| !name.contains("claude-") && !name.contains("gpt-5")
                        || name.starts_with("GPT")),
                "{names:?}"
            );
            assert!(picker.rows[1].active, "✓ on the Provider's default");
            assert_eq!(picker.selected, 1, "the arrows start on it");
            codex + 1
        });

        view.update(cx, |view, cx| view.pick(first_codex, cx));
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(view.popover.is_none(), "the pick closed the picker");
            assert_eq!(
                view.cockpit.thread(thread).map(|open| open.provider()),
                Some(Provider::Codex)
            );
            // The switch was Ferrite's own act: no prompt, no running turn.
            let transcript = view.cockpit.thread(thread).unwrap().transcript();
            assert!(transcript.blocks().is_empty());
            assert!(!view.cockpit.thread(thread).is_some_and(|open| open.busy()));
        });
        let spawned = fake.spawned.borrow().last().unwrap().clone();
        assert_eq!(
            spawned.provider,
            Provider::Codex,
            "the choice drives the spawn"
        );
        assert_eq!(
            spawned.model.as_deref(),
            Some(
                ferrite_core::providers::models::fallback(Provider::Codex)[0]
                    .value
                    .as_str()
            ),
            "the first Codex row is its first catalog model"
        );
    }

    /// #25: announced models ride the picker below the providers and a
    /// pick re-aims the model without touching the provider; the fresh
    /// Session's own announcement then lists them again with the ✓ on the
    /// standing choice.
    #[gpui::test]
    fn announced_models_ride_the_picker_and_a_pick_reaims_the_model(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("provider-models", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        fake.streams.borrow()[0]
            .send(SessionEvent::Models {
                models: vec!["sonnet".into(), "opus".into()],
            })
            .unwrap();
        tick(cx);

        cx.simulate_input("/");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let picker = view.popover.as_ref().expect("open");
            let names: Vec<&str> = picker
                .rows
                .iter()
                .map(|pick| pick.row.name.as_ref())
                .collect();
            // The announced list replaces the catalog for its Provider;
            // Codex, which announces none, keeps its catalog.
            assert_eq!(&names[..4], ["Claude", "Sonnet", "Opus", "Codex"]);
            assert_eq!(
                picker.selected, 1,
                "no choice yet: the arrows start on the first model"
            );
        });

        cx.simulate_keystrokes("down enter");
        cx.run_until_parked();

        assert_eq!(
            fake.spawned.borrow().last().unwrap(),
            &ProviderChoice {
                provider: Provider::Claude,
                model: Some("opus".into()),
            },
            "the model rides the spawn; the provider stands"
        );
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.thread(thread).and_then(|open| open.model()),
                Some("opus")
            );
        });

        // The replacement Session announces its own list — reopening the
        // picker shows it with the ✓ on the standing choice.
        fake.streams
            .borrow()
            .last()
            .unwrap()
            .send(SessionEvent::Models {
                models: vec!["sonnet".into(), "opus".into()],
            })
            .unwrap();
        tick(cx);
        cx.simulate_input("/");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let picker = view.popover.as_ref().expect("reopened");
            assert!(picker.rows[2].active, "✓ on the standing model choice");
            assert!(!picker.rows[1].active);
            assert_eq!(picker.selected, 2, "and the arrows start there");
        });
    }

    #[gpui::test]
    fn a_draft_inherits_the_focused_provider_model_and_only_announced_models(
        cx: &mut TestAppContext,
    ) {
        let (core, fake) = cockpit("draft-provider-choice", 2);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        fake.streams.borrow()[1]
            .send(SessionEvent::Models {
                models: vec!["sonnet".into(), "sonnet".into()],
            })
            .unwrap();
        tick(cx);
        view.update(cx, |view, cx| {
            view.cockpit
                .set_provider(
                    thread,
                    ProviderChoice {
                        provider: Provider::Claude,
                        model: Some("opus".into()),
                    },
                )
                .unwrap();
            view.open_draft(DraftTarget::Main, cx);
            view.open_band_popover(pane::BandChip::Provider, cx);
        });

        view.read_with(cx, |view, _| {
            let draft = view.panes[view.focused()].draft().unwrap();
            assert_eq!(draft.binding.provider().model.as_deref(), Some("opus"));
            let labels: Vec<&str> = view
                .popover
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .map(|row| row.name.as_ref())
                .collect();
            // Announced once (deduplicated), the standing choice appended
            // where the list did not name it, then the Codex section.
            assert_eq!(&labels[..4], ["Claude", "Sonnet", "Opus", "Codex"]);
            let rows = &view.popover.as_ref().unwrap().rows;
            assert!(rows[2].active, "✓ on the inherited choice");
            assert_eq!(view.popover.as_ref().unwrap().selected, 2);
        });
        view.update(cx, |view, cx| {
            let selected = view.popover.as_ref().unwrap().selected;
            view.pick(selected, cx);
            assert_eq!(
                view.panes[view.focused()]
                    .draft()
                    .unwrap()
                    .binding
                    .provider()
                    .model
                    .as_deref(),
                Some("opus")
            );
        });
    }

    /// The first prompt fixes the Provider, never the model: the picker
    /// still opens, the other Provider's section is drawn inert and says
    /// why, and the Composer's control stays in every Pane.
    #[gpui::test]
    fn the_first_prompt_fixes_the_provider_but_the_model_picker_stays_open(
        cx: &mut TestAppContext,
    ) {
        let (core, _fake) = cockpit("provider-lock-ui", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        view.update(cx, |view, cx| {
            assert!(
                view.model_picker(0, cx).is_some(),
                "pre-lock the Composer offers the control"
            );
        });

        cx.simulate_input("hello");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        view.update(cx, |view, cx| {
            view.open_provider_picker(thread, cx);
            let picker = view
                .popover
                .as_ref()
                .expect("the picker opens after the first prompt");
            let codex = picker
                .rows
                .iter()
                .position(|row| row.name.as_ref() == "Codex")
                .unwrap();
            // The other Provider is a handover now, not a dead door: its
            // section says so and its rows stay live.
            assert_eq!(
                picker.rows[codex].detail.as_ref(),
                "hands the conversation over"
            );
            assert!(
                picker.rows[codex + 1..].iter().all(|row| !row.inert),
                "the other Provider's rows are live"
            );
            assert!(
                picker.rows[1..codex].iter().all(|row| !row.inert),
                "this Provider's models are live"
            );
            assert!(
                view.model_picker(0, cx).is_some(),
                "and the control itself stays — every Pane draws one"
            );
            view.popover = None;
        });

        // The `/model` row stays live and opens the same picker.
        cx.simulate_input("/");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let row = &view.popover.as_ref().expect("open").rows[0];
            assert_eq!(row.name.as_ref(), "/model");
            assert!(!row.inert);
            assert_eq!(
                row.detail.as_ref(),
                "switch model · hand over to the other provider"
            );
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(matches!(
                view.popover.as_ref().expect("the picker opened").kind,
                Kind::Provider
            ));
        });
        assert_eq!(
            composer_text(&view, cx),
            "",
            "the pick never lands as slash text"
        );
    }

    /// #25 regression: reopening the picker with a standing model choice
    /// and pressing bare ↵ changes nothing — the current provider's row
    /// carries the choice, so the re-pick is a true no-op: no teardown, no
    /// respawn, the model kept.
    #[gpui::test]
    fn reopening_the_picker_and_pressing_enter_keeps_the_standing_choice(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("provider-reopen-noop", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        fake.streams.borrow()[0]
            .send(SessionEvent::Models {
                models: vec!["sonnet".into(), "opus".into()],
            })
            .unwrap();
        tick(cx);

        // Pick opus: claude · opus is now the standing choice.
        cx.simulate_input("/");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        cx.simulate_keystrokes("down enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.thread(thread).and_then(|open| open.model()),
                Some("opus")
            );
        });
        let spawns = fake.streams.borrow().len();

        // Reopen; the arrows start on the standing choice — bare ↵
        // re-picks it whole.
        cx.simulate_input("/");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let picker = view.popover.as_ref().expect("open");
            let standing = picker
                .rows
                .iter()
                .position(|row| row.active)
                .expect("✓ on the choice");
            assert_eq!(
                picker.rows[standing].name.as_ref(),
                "Opus",
                "the learned provider label survives the Session replacement"
            );
            assert_eq!(picker.selected, standing);
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        assert_eq!(
            fake.streams.borrow().len(),
            spawns,
            "a re-pick of the standing choice must not respawn"
        );
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.thread(thread).and_then(|open| open.model()),
                Some("opus"),
                "the model stands"
            );
            assert_eq!(
                view.cockpit.thread(thread).map(|open| open.provider()),
                Some(Provider::Claude)
            );
        });
    }

    /// A right-click on a Pane summons the menu for its Thread; escape
    /// closes it; a destructive row arms on the first press and runs on
    /// the second — a deleted Thread is gone from the grid and the store.
    #[gpui::test]
    fn a_right_click_summons_the_menu_and_delete_takes_two_presses(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("context-menu", 2);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

        // Right-click inside the focused Pane's transcript.
        cx.simulate_mouse_down(
            gpui::point(px(600.), px(300.)),
            gpui::MouseButton::Right,
            gpui::Modifiers::none(),
        );
        cx.run_until_parked();
        let (delete, count) = view.read_with(cx, |view, _| {
            let menu = view
                .context_menu
                .as_ref()
                .expect("a right-click opens the menu");
            assert_eq!(menu.target, MenuTarget::Pane(thread));
            let labels: Vec<&str> = menu
                .rows
                .iter()
                .flatten()
                .map(|(item, _)| item.label.as_ref())
                .collect();
            assert!(labels.contains(&"Rename"), "{labels:?}");
            assert!(labels.contains(&"Park Thread"), "{labels:?}");
            assert!(labels.contains(&"Copy Transcript"), "{labels:?}");
            // The transcript's menu is about what is on screen; deleting
            // a Thread is the nav row's act.
            assert!(!labels.contains(&"Delete Thread"), "{labels:?}");
            let delete = view
                .context_rows(MenuTarget::Thread(thread))
                .iter()
                .position(|row| matches!(row, Some((_, MenuVerb::Delete))))
                .expect("the nav row's menu deletes");
            (
                delete,
                view.cockpit.threads().len() + view.cockpit.parked().unwrap().len(),
            )
        });

        // Escape closes it without running anything.
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.context_menu.is_none()));

        view.update(cx, |view, cx| {
            view.open_context_menu(
                MenuTarget::Thread(thread),
                gpui::point(px(600.), px(300.)),
                cx,
            );
            view.press_menu_row(delete, cx);
            let menu = view.context_menu.as_ref().expect("armed, still up");
            assert_eq!(menu.armed, Some(delete), "the first press only arms");
            view.press_menu_row(delete, cx);
            assert!(
                view.context_menu.is_none(),
                "the second press runs and closes"
            );
            assert!(
                view.cockpit.thread(thread).is_none(),
                "the Thread is gone from the grid"
            );
            assert_eq!(
                view.cockpit.threads().len() + view.cockpit.parked().unwrap().len(),
                count - 1,
                "and from the store"
            );
            assert!(view.panes.iter().all(|pane| pane.thread() != Some(thread)));
        });
    }

    /// cmd-, opens the Settings panel and escape closes it; a chip press
    /// writes the setting and saves it to disk at once, and the Session
    /// defaults the spawner reads follow.
    #[gpui::test]
    fn settings_open_on_cmd_comma_and_every_change_saves(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("settings-panel", 1);
        bind_production_keys(cx);
        let (root, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| CockpitView::new(core, cx));
            gpui::component::Root::new(view, window, cx)
        });
        let view = root.read_with(cx, |root, _| {
            root.view().clone().downcast::<CockpitView>().unwrap()
        });
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_keystrokes("cmd-,");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.settings_open, "cmd-, opens the panel")
        });
        let card = cx.debug_bounds("settings-card").unwrap();
        cx.simulate_click(
            card.origin + gpui::point(px(60.), px(180.)),
            gpui::Modifiers::none(),
        );
        tick(cx);
        let about = cx
            .debug_bounds("settings-fact-Settings file")
            .expect("About navigation reveals stored paths");
        assert!(
            about.bottom() <= card.bottom(),
            "About must scroll into the panel"
        );
        cx.simulate_click(
            card.origin + gpui::point(px(60.), px(90.)),
            gpui::Modifiers::none(),
        );
        tick(cx);
        let codex = cx.debug_bounds("settings-provider-1").unwrap().center();
        cx.simulate_click(codex, gpui::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.prefs.settings.default_provider, Provider::Codex);
            assert_eq!(
                ferrite_core::settings::Settings::load(&view.prefs.dir).default_provider,
                Provider::Codex,
                "the restyled component must save through the real settings action"
            );
        });
        // Click the native search field in the Settings sidebar.
        let card = cx.debug_bounds("settings-card").unwrap();
        cx.simulate_click(
            card.origin + gpui::point(px(80.), px(66.)),
            gpui::Modifiers::none(),
        );
        cx.simulate_input("Confirm before deleting");
        tick(cx);
        let toggle = cx.debug_bounds("settings-confirm-delete").unwrap().center();
        cx.simulate_click(toggle, gpui::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.prefs.settings.confirm_delete);
            assert!(!ferrite_core::settings::Settings::load(&view.prefs.dir).confirm_delete);
        });
        cx.simulate_click(
            card.origin + gpui::point(px(80.), px(66.)),
            gpui::Modifiers::none(),
        );
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.settings_open, "escape from search closes Settings")
        });
        cx.simulate_keystrokes("cmd-,");
        cx.run_until_parked();
        let close = cx.debug_bounds("settings-close").unwrap().center();
        cx.simulate_click(close, gpui::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(!view.settings_open));
        let gear = cx.debug_bounds("settings-gear").unwrap().center();
        cx.simulate_click(gear, gpui::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.settings_open));
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(!view.settings_open, "escape closes it")
        });

        let dir = view.read_with(cx, |view, _| view.prefs.dir.clone());
        let _ = std::fs::remove_dir_all(&dir);
        view.update(cx, |view, cx| {
            view.change_settings(
                |settings| {
                    settings.default_provider = Provider::Codex;
                    settings.claude_permission_mode = Some("acceptEdits".into());
                    settings.confirm_delete = false;
                },
                cx,
            );
        });
        let saved = ferrite_core::settings::Settings::load(&dir);
        assert_eq!(saved.default_provider, Provider::Codex, "saved on change");
        assert_eq!(saved.claude_permission_mode.as_deref(), Some("acceptEdits"));
        view.read_with(cx, |view, _| {
            let defaults = view.prefs.defaults.lock().unwrap();
            assert_eq!(
                defaults.claude_permission_mode.as_deref(),
                Some("acceptEdits"),
                "the spawner's defaults follow"
            );
            assert_eq!(
                view.default_choice().provider,
                Provider::Codex,
                "new drafts follow"
            );
        });

        // With confirmation off, a destructive row runs on its first press.
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        view.update(cx, |view, cx| {
            view.open_context_menu(
                MenuTarget::Thread(thread),
                gpui::point(px(600.), px(300.)),
                cx,
            );
            let delete = view
                .context_menu
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .position(|row| matches!(row, Some((_, MenuVerb::Delete))))
                .unwrap();
            view.press_menu_row(delete, cx);
            assert!(view.context_menu.is_none());
            assert!(view.cockpit.thread(thread).is_none(), "gone on one press");
        });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A Group's board is its split tree: two members sit side by side at
    /// the tree's rects; dragging the seam between them re-shares the
    /// width and persists on release; a Pane dropped on the other's centre
    /// swaps them, on an edge splits — both persisted.
    #[gpui::test]
    fn the_group_board_resizes_by_its_seam_and_reorders_by_drop(cx: &mut TestAppContext) {
        let (mut core, _fake) = cockpit("tree-board", 2);
        let threads = core.threads();
        let group = core
            .apply_group(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1200.), px(700.)));
        view.update(cx, |view, cx| view.enter_group(group, cx));
        tick(cx);

        let rects: Vec<ferrite_core::layout::Rect> = cx.update(|window, cx| {
            view.read(cx)
                .pane_rects(window)
                .into_iter()
                .map(|(_, rect)| rect)
                .collect()
        });
        assert_eq!(rects.len(), 2, "both members have a rect");
        assert!(
            (rects[0].w - rects[1].w).abs() < 1.0,
            "an even split: {rects:?}"
        );
        assert!(
            rects[1].x > rects[0].x + rects[0].w,
            "side by side, a gap between"
        );

        // Grab the seam (the gap between the two rects) and drag it right.
        let seam_x = rects[0].x + rects[0].w + crate::theme::GRID_GAP / 2.0;
        let seam_y = rects[0].y + rects[0].h / 2.0;
        cx.simulate_mouse_down(
            gpui::point(px(seam_x), px(seam_y)),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.seam_drag.is_some(), "the seam is held")
        });
        cx.simulate_mouse_move(
            gpui::point(px(seam_x + 200.), px(seam_y)),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
        cx.run_until_parked();
        cx.simulate_mouse_up(
            gpui::point(px(seam_x + 200.), px(seam_y)),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.seam_drag.is_none(), "released");
            let tree = view.cockpit.group_layout(group).unwrap();
            let ratio = match tree.root.as_ref().unwrap() {
                ferrite_core::layout::Node::Split { ratio, .. } => *ratio,
                _ => panic!("two members make a split"),
            };
            assert!(ratio > 0.6 && ratio < 0.8, "the first side grew: {ratio}");
        });

        // A drop on the centre swaps; on an edge splits the target's slot.
        view.update(cx, |view, cx| {
            view.apply_pane_drop(threads[0], threads[1], Zone::Swap, cx);
            let tree = view.cockpit.group_layout(group).unwrap();
            assert_eq!(tree.leaves(), vec![threads[1], threads[0]], "swapped");
            view.apply_pane_drop(threads[0], threads[1], Zone::Split(Edge::Top), cx);
            let tree = view.cockpit.group_layout(group).unwrap();
            match tree.root.as_ref().unwrap() {
                ferrite_core::layout::Node::Split { axis, .. } => {
                    assert_eq!(*axis, ferrite_core::layout::Axis::Column, "stacked now")
                }
                _ => panic!("still a split"),
            }
            assert_eq!(
                tree.leaves(),
                vec![threads[0], threads[1]],
                "the source sits above"
            );
        });
    }

    /// The effort chip opens a ladder from the provider's own announcement
    /// with the operator's default on top; a pick re-aims the Thread and
    /// the chip names the level. `/effort` opens the same picker.
    #[gpui::test]
    fn the_effort_picker_lists_the_models_ladder_and_a_pick_reaims_it(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("effort-picker", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        fake.streams.borrow()[0]
            .send(SessionEvent::Models {
                models: vec![ferrite_core::ModelInfo {
                    value: "sonnet".into(),
                    display: "Sonnet 5".into(),
                    detail: String::new(),
                    resolved: None,
                    efforts: vec!["low".into(), "high".into()],
                    default_effort: Some("high".into()),
                }],
            })
            .unwrap();
        tick(cx);

        view.update(cx, |view, cx| {
            view.open_effort_picker(thread, cx);
            let picker = view.popover.as_ref().expect("the effort picker opens");
            assert!(matches!(picker.kind, Kind::Effort));
            let names: Vec<&str> = picker.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(names, ["Default", "Low", "High"]);
            assert!(
                picker.rows[0].active,
                "no choice yet: the default is in force"
            );
            view.pick(2, cx);
            assert!(view.popover.is_none(), "a pick closes the picker");
            assert_eq!(view.cockpit.thread(thread).unwrap().effort(), Some("high"));
        });

        cx.simulate_input("/eff");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.popover.as_ref().expect("the / menu");
            assert_eq!(menu.rows[0].name.as_ref(), "/effort");
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let picker = view.popover.as_ref().expect("/effort opens the picker");
            assert!(matches!(picker.kind, Kind::Effort));
            // The pick respawned the Session (pre-lock, eagerly), so the
            // ladder is the catalog's again; the ✓ still sits on High.
            let high = picker
                .rows
                .iter()
                .find(|row| row.name.as_ref() == "High")
                .expect("the ladder has High");
            assert!(high.active, "✓ on the level in force");
        });
        assert_eq!(composer_text(&view, cx), "", "the /effort line is cleared");
    }

    /// Claude lists its own `effort` command; Ferrite's row of that name
    /// replaces it, so the menu shows one `/effort` and it opens the
    /// picker. Typed out with a level, `/effort high` is honoured by the
    /// cockpit itself and never sent as text; an unknown level is.
    #[gpui::test]
    fn ferrites_effort_row_replaces_the_providers_and_a_typed_level_is_honoured(
        cx: &mut TestAppContext,
    ) {
        let (core, fake) = cockpit("effort-typed", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        fake.streams.borrow()[0]
            .send(SessionEvent::Commands {
                commands: vec![ferrite_core::SessionCommand {
                    name: "effort".into(),
                    description: "Set effort level for model usage".into(),
                    path: None,
                }],
            })
            .unwrap();
        tick(cx);
        cx.simulate_input("/eff");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.popover.as_ref().expect("the / menu");
            let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(names, ["/effort"], "one row, Ferrite's");
            assert!(matches!(
                menu.rows[0].consequence,
                Consequence::OpenEffortPicker
            ));
        });
        cx.simulate_input("ort high");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.thread(thread).unwrap().effort(), Some("high"));
        });
        assert!(
            fake.sent.borrow().is_empty(),
            "a known level is not a prompt"
        );

        cx.simulate_input("/effort turbo");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            fake.sent.borrow().last().map(String::as_str),
            Some("/effort turbo"),
            "an unknown level goes to the provider as text"
        );
    }

    /// The draft band's effort chip: its rows are the model's ladder under
    /// the operator's default, a pick names the chip, and the first send
    /// starts the Thread on that effort.
    #[gpui::test]
    fn the_effort_chip_starts_the_thread_on_its_pick(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("draft-effort", 0);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);
        view.update(cx, |view, cx| {
            view.open_band_popover(pane::BandChip::Effort, cx);
            let band = view.popover.as_ref().expect("the effort popover");
            assert!(matches!(band.kind, Kind::Band(pane::BandChip::Effort)));
            let names: Vec<&str> = band.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(names[0], "Default");
            assert!(names.contains(&"High"), "{names:?}");
            let high = names.iter().position(|name| *name == "High").unwrap();
            view.pick(high, cx);
            let draft = view.panes[view.focused()].draft().expect("still a draft");
            assert_eq!(draft.binding.effort(), Some("high"));
        });
        cx.simulate_input("go");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let thread = view.panes[0].thread().expect("bootstrapped");
            assert_eq!(view.cockpit.thread(thread).unwrap().effort(), Some("high"));
        });
    }
    /// #25: the mouse door — a click on the footer chip opens the picker.
    /// The sweep covers the meta row's right side so the test does not
    /// encode the chip's exact position.
    #[gpui::test]
    fn clicking_the_footer_chip_opens_the_provider_picker(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("provider-chip-click", 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        let mut opened = false;
        'sweep: for y in [668., 674., 680., 686.] {
            for x in (0..30).map(|step| 985. - step as f32 * 10.) {
                cx.simulate_mouse_down(
                    gpui::point(px(x), px(y)),
                    gpui::MouseButton::Left,
                    gpui::Modifiers::none(),
                );
                cx.run_until_parked();
                if view.read_with(cx, |view, _| view.popover.is_some()) {
                    opened = true;
                    break 'sweep;
                }
            }
        }
        assert!(opened, "the sweep never found the chip");
        // The sweep runs right to left and the effort chip sits right of
        // the model chip: either picker proves the mouse door.
        view.read_with(cx, |view, _| {
            let picker = view.popover.as_ref().expect("open");
            assert!(matches!(picker.kind, Kind::Provider | Kind::Effort));
        });
    }

    /// The row meta's age, spelled the way an operator scans it.
    #[test]
    fn the_age_label_rounds_to_the_operator_scale() {
        let now = std::time::SystemTime::now();
        let ago = |secs: u64| Some(now - Duration::from_secs(secs));
        assert_eq!(age_label(ago(12), now), "just now");
        assert_eq!(age_label(ago(90), now), "1m ago");
        assert_eq!(age_label(ago(45 * 60), now), "45m ago");
        assert_eq!(age_label(ago(3 * 3600), now), "3h ago");
        assert_eq!(age_label(ago(2 * 86400), now), "2d ago");
        assert_eq!(age_label(None, now), "age unknown");
    }

    fn drag_nav(cx: &mut gpui::VisualTestContext, source: &'static str, target: &'static str) {
        let source = cx.debug_bounds(source).unwrap();
        let target = cx.debug_bounds(target).unwrap();
        let source = gpui::point(source.right() - px(5.), source.center().y);
        cx.simulate_mouse_down(source, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_move(
            gpui::point(source.x - px(30.), source.y),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
        cx.simulate_mouse_move(
            target.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
        cx.simulate_mouse_up(
            target.center(),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
    }

    #[gpui::test]
    fn solo_group_draft_and_close_follow_the_persisted_view_state_machine(cx: &mut TestAppContext) {
        let (mut core, _fake) = cockpit("group-view-state", 3);
        let threads = core.threads();
        let group = core
            .apply_group(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        core.apply_group(GroupChange::Join {
            thread: threads[2],
            group,
            index: None,
        })
        .unwrap();
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-w", CloseThread, None),
                KeyBinding::new("cmd-t", NewThread, None),
                KeyBinding::new("enter", Submit, None),
            ])
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));

        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.roster().view(), View::Solo);
            assert_eq!(view.visible_indices(), [0]);
        });
        view.update(cx, |view, cx| view.enter_group(group, cx));
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.roster().view(), View::Group(group));
            let visible: Vec<_> = view
                .visible_indices()
                .into_iter()
                .filter_map(|index| view.panes[index].thread())
                .collect();
            assert_eq!(visible, threads);
        });

        cx.simulate_keystrokes("cmd-t");
        view.read_with(cx, |view, _| {
            assert_eq!(view.visible_indices().len(), 4, "pending draft appends");
            assert_eq!(view.cockpit.groups().get(group).unwrap().members, threads);
        });
        cx.simulate_input("build it");
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.groups().get(group).unwrap().members.len(), 4);
        });

        cx.simulate_keystrokes("cmd-w");
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.roster().view(), View::Group(group));
            assert_eq!(view.visible_indices().len(), 3);
            assert_eq!(view.cockpit.threads().len(), 4, "leaving never parks");
        });
        cx.simulate_keystrokes("cmd-w cmd-w");
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.roster().view(), View::Solo);
            assert!(view.cockpit.groups().get(group).is_none());
            assert_eq!(view.visible_indices().len(), 1);
            assert_eq!(view.cockpit.threads().len(), 4);
        });
    }

    #[gpui::test]
    fn a_pending_draft_keeps_a_pair_open_until_it_sends_or_closes(cx: &mut TestAppContext) {
        let (mut core, _fake) = cockpit("group-pending-draft", 4);
        let threads = core.threads();
        let sending = core
            .apply_group(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        let closing = core
            .apply_group(GroupChange::Create {
                first: threads[2],
                second: threads[3],
            })
            .unwrap()
            .group
            .unwrap();
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-t", NewThread, None),
                KeyBinding::new("cmd-w", CloseThread, None),
                KeyBinding::new("enter", Submit, None),
            ])
        });
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));

        view.update(cx, |view, cx| view.enter_group(sending, cx));
        cx.simulate_keystrokes("cmd-t");
        let sending_draft =
            view.read_with(cx, |view, _| view.panes[view.focused()].composer.clone());
        view.update(cx, |view, _| {
            view.focus_pane(view.pane_for(threads[0]).unwrap())
        });
        cx.simulate_keystrokes("cmd-w");
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.roster().view(), View::Group(sending));
            assert_eq!(view.visible_indices().len(), 2, "survivor plus Draft");
            assert_eq!(view.cockpit.groups().get(sending).unwrap().members.len(), 2);
        });
        view.update(cx, |view, _| {
            let draft = view
                .panes
                .iter()
                .position(|pane| pane.composer == sending_draft)
                .unwrap();
            view.focus_pane(draft);
        });
        cx.simulate_input("preserve this prompt");
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            let members = &view.cockpit.groups().get(sending).unwrap().members;
            assert_eq!(members.len(), 2);
            assert_eq!(members[0], threads[1]);
            assert!(!members.contains(&threads[0]));
        });

        view.update(cx, |view, cx| view.enter_group(closing, cx));
        cx.simulate_keystrokes("cmd-t");
        let closing_draft =
            view.read_with(cx, |view, _| view.panes[view.focused()].composer.clone());
        cx.simulate_input("discard only when I close");
        view.update(cx, |view, _| {
            view.focus_pane(view.pane_for(threads[2]).unwrap())
        });
        cx.simulate_keystrokes("cmd-w");
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.roster().view(), View::Group(closing));
            assert_eq!(view.visible_indices().len(), 2);
        });
        view.update(cx, |view, _| {
            let draft = view
                .panes
                .iter()
                .position(|pane| pane.composer == closing_draft)
                .unwrap();
            view.focus_pane(draft);
        });
        cx.simulate_keystrokes("cmd-w");
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.roster().view(), View::Solo);
            assert!(view.cockpit.groups().get(closing).is_none());
            assert_eq!(view.focused_thread(), Some(threads[3]));
        });
    }

    #[gpui::test]
    fn pointer_opens_a_group_and_renames_parked_thread_and_group_titles(cx: &mut TestAppContext) {
        let (mut core, _) = cockpit("group-pointer-open-rename", 2);
        let threads = core.threads();
        let group = core
            .apply_group(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        assert_eq!(group.get(), 1);
        core.rename_thread(threads[1], "Existing parked title")
            .unwrap();
        core.park(threads[1]).unwrap();
        cx.update(|cx| cx.bind_keys([KeyBinding::new("enter", Submit, None)]));
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        cx.run_until_parked();

        let thread_title = cx
            .debug_bounds("rename-thread-2")
            .expect("the parked title is rendered");
        // A single click on a title is not a rename — it is the row's own
        // click, which focuses the Thread. Only the second press opens the
        // editor.
        cx.simulate_click(thread_title.center(), gpui::Modifiers::none());
        view.read_with(cx, |view, _| {
            assert!(
                view.rename.is_none(),
                "one click never opens the inline editor"
            );
        });
        press(cx, thread_title.center(), 2);
        view.read_with(cx, |view, cx| {
            let editor = &view.rename.as_ref().expect("inline Thread editor").1;
            assert_eq!(editor.read(cx).text(), "Existing parked title");
        });
        view.update(cx, |view, cx| {
            let editor = view.rename.as_ref().unwrap().1.clone();
            editor.update(cx, |line, cx| line.set("cancel me".into(), cx));
        });
        cx.simulate_click(gpui::point(px(500.), px(350.)), gpui::Modifiers::none());
        view.read_with(cx, |view, _| {
            assert!(view.rename.is_none());
            assert_eq!(
                view.cockpit.thread_title(threads[1]).unwrap().as_deref(),
                Some("Existing parked title")
            );
        });

        let thread_title = cx.debug_bounds("rename-thread-2").unwrap();
        press(cx, thread_title.center(), 2);
        view.update(cx, |view, cx| {
            let editor = view.rename.as_ref().unwrap().1.clone();
            editor.update(cx, |line, cx| line.set("Saved parked title".into(), cx));
        });
        cx.simulate_keystrokes("enter");

        let group_title = cx.debug_bounds("rename-group-1").unwrap();
        cx.simulate_click(group_title.center(), gpui::Modifiers::none());
        view.read_with(cx, |view, _| {
            assert!(
                view.rename.is_none(),
                "a Group title takes two clicks too — one enters the Group"
            );
        });
        press(cx, group_title.center(), 2);
        view.update(cx, |view, cx| {
            let editor = view.rename.as_ref().unwrap().1.clone();
            editor.update(cx, |line, cx| line.set("Saved Group title".into(), cx));
        });
        cx.simulate_keystrokes("enter");

        let header = cx.debug_bounds("nav-group-1").unwrap();
        cx.simulate_click(
            gpui::point(header.right() - px(6.), header.center().y),
            gpui::Modifiers::none(),
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.roster().view(), View::Group(group));
            assert_eq!(view.visible_indices().len(), 2);
            assert_eq!(
                view.cockpit.thread_title(threads[1]).unwrap().as_deref(),
                Some("Saved parked title")
            );
            assert_eq!(
                view.cockpit.groups().get(group).unwrap().display_title(),
                "Saved Group title"
            );
        });
    }

    #[gpui::test]
    fn pointer_drag_joins_then_reorders_to_the_after_last_target(cx: &mut TestAppContext) {
        let (mut core, _) = cockpit("group-pointer-drag", 5);
        let threads = core.threads();
        let group = core
            .apply_group(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        let second_group = core
            .apply_group(GroupChange::Create {
                first: threads[3],
                second: threads[4],
            })
            .unwrap()
            .group
            .unwrap();
        core.rename_thread(threads[2], "Durable drag title")
            .unwrap();
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        cx.run_until_parked();

        drag_nav(cx, "nav-thread-3", "nav-group-1");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.groups().get(group).unwrap().members,
                threads[..3],
                "the real drop dispatched the join plan"
            );
        });

        drag_nav(cx, "nav-thread-1", "member-tail-1");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.cockpit.groups().get(group).unwrap().members,
                [threads[1], threads[2], threads[0]],
                "downward reorder lands after the last member"
            );
        });

        drag_nav(cx, "nav-group-2", "group-gap-0");
        view.read_with(cx, |view, _| {
            let groups: Vec<_> = view.cockpit.groups().iter().map(|group| group.id).collect();
            assert_eq!(groups, [second_group, group]);
        });
    }

    #[gpui::test]
    fn pointer_dragging_from_a_pair_with_a_draft_keeps_the_group_pending(cx: &mut TestAppContext) {
        let (mut core, _) = cockpit("group-pointer-pending-draft", 2);
        let threads = core.threads();
        let group = core
            .apply_group(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-t", NewThread, None)]));
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, cx| view.enter_group(group, cx));
        cx.simulate_keystrokes("cmd-t");
        let draft = view.read_with(cx, |view, _| view.panes[view.focused()].composer.clone());
        cx.simulate_input("preserve this exact prompt");
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        cx.run_until_parked();

        drag_nav(cx, "nav-thread-1", "loose-zone");

        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.roster().view(), View::Solo);
            assert_eq!(view.focused_thread(), Some(threads[0]));
            assert_eq!(
                view.cockpit.groups().get(group).unwrap().members,
                threads,
                "the pending leave has not dissolved the durable pair"
            );
        });
        view.update(cx, |view, cx| view.enter_group(group, cx));
        view.read_with(cx, |view, cx| {
            let visible = view.visible_indices();
            assert_eq!(visible.len(), 2, "survivor plus Draft");
            let visible_threads: Vec<_> = visible
                .iter()
                .filter_map(|index| view.panes[*index].thread())
                .collect();
            assert_eq!(visible_threads, [threads[1]]);
            assert!(visible
                .iter()
                .any(|index| view.panes[*index].composer == draft));
            assert_eq!(draft.read(cx).text(), "preserve this exact prompt");
        });
    }

    #[gpui::test]
    fn a_hundred_member_group_remains_reachable_through_nav_scroll(cx: &mut TestAppContext) {
        let (mut core, _) = cockpit("group-nav-scroll", 100);
        group_all(&mut core);
        let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(320.)));
        cx.run_until_parked();
        let before = cx.debug_bounds("nav-thread-100").unwrap();
        assert!(
            before.center().y > px(320.),
            "the last row starts below the viewport"
        );

        // Past the end and clamped there, rather than a measured distance:
        // a member row is 56.5px tall (#32), so 100 of them are ~5900px of
        // column and any number tuned to a shorter row would stop short.
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(100.), px(200.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-40_000.))),
            modifiers: gpui::Modifiers::none(),
            touch_phase: gpui::TouchPhase::default(),
        });
        cx.run_until_parked();
        let after = cx.debug_bounds("nav-thread-100").unwrap();
        assert!(
            after.center().y < px(320.),
            "scrolling exposes the final member: {after:?}"
        );
    }

    #[gpui::test]
    fn drag_from_a_pair_tracks_the_origin_and_focuses_the_survivor(cx: &mut TestAppContext) {
        let (mut core, _) = cockpit("group-drag-origin", 3);
        let threads = core.threads();
        let group = core
            .apply_group(GroupChange::Create {
                first: threads[0],
                second: threads[1],
            })
            .unwrap()
            .group
            .unwrap();
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, cx| {
            view.enter_group(group, cx);
            view.apply_drop(
                NavDrag {
                    drag: Drag::Thread {
                        thread: threads[0],
                        group: Some(group),
                    },
                    origin: View::Group(group),
                },
                DropTarget::ThreadRow {
                    thread: threads[2],
                    group: None,
                    index: 0,
                },
                cx,
            );
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.roster().view(), View::Solo);
            assert_eq!(view.focused_thread(), Some(threads[1]));
            assert!(view.cockpit.groups().get(group).is_none());
        });
    }

    #[gpui::test]
    fn inline_title_rename_disarms_the_thread_composer_history_context_and_hint(
        cx: &mut TestAppContext,
    ) {
        let (mut core, _) = cockpit("prompt-history-rename", 1);
        let thread = core.threads()[0];
        core.send(thread, "history".into());
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        tick(cx);

        view.update(cx, |view, cx| {
            view.start_rename(RenameTarget::Thread(thread), cx)
        });
        tick(cx);
        view.read_with(cx, |view, _| assert!(view.rename.is_some()));
        cx.update(|window, cx| {
            assert!(
                !view
                    .read(cx)
                    .history_available(0, view.read(cx).level_now(window)),
                "the visible Thread Composer must not advertise or arm history while rename owns focus"
            )
        });
    }
}
