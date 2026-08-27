//! The cockpit window: every open Pane at once, and the one pump behind them.
//!
//! Rendering and keys only. What each Pane shows — the Blocks, the pending
//! Decision, the held prompt — is folded in core and read from there.

use std::time::Duration;

use ferrite_core::cockpit::{Cockpit, ProviderChoice};
use ferrite_core::docview::{Cell, Level};
use ferrite_core::groups::{
    grid as group_grid, Drag, DropTarget, GroupChange, GroupId, Groups, Plan,
};
use ferrite_core::store::Provider;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::workspace::WorkspaceChoice;
use ferrite_core::{DecisionAnswer, ThreadId};
use gpui::prelude::*;
use gpui::{
    actions, deferred, div, px, relative, rgb, rgba, AnyElement, ClipboardItem, Context, Div,
    Entity, FocusHandle, Focusable, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, ScrollHandle, SharedString, Stateful, Window,
};

use crate::nav;
use crate::pane::{self, PaneView};
use crate::select::TranscriptSelection;

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
        CloseThread,
        ReopenThread,
        CopySelection,
        ToggleFullscreen,
        ToggleNav,
        MenuNext,
        MenuPrevious,
        MenuPick,
        MenuDismiss,
        ToggleGroup,
        MoveToGroup,
        RenameGroup,
        MoveGroupUp,
        MoveGroupDown,
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
    fullscreen: Option<PaneIdentity>,
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
    /// The one live text selection, at character grain (#27). The cockpit
    /// speaks raw positions — begin on press, extend on drag, copied_text
    /// on cmd-c — and select.rs owns every offset behind that seam. A plain
    /// click selects nothing; the next press anywhere clears it.
    selection: TranscriptSelection,
    /// cmd-b (#21): the nav folded to its 40px LED rail. In memory only —
    /// a preference store is not this ticket.
    nav_collapsed: bool,
    /// The Thread whose context ring the pointer is on — its hover card
    /// shows while this is set (#22 C12). Render state only, healed by the
    /// ring not rendering.
    hovered_usage: Option<ThreadId>,
    /// Each open Thread's checkout label (#29): the branch of its cwd,
    /// cached — the header's binding slot is display-only text fed from
    /// here, refreshed on open, turn end and the watchdog cadence, never
    /// per frame. A cwd outside any checkout has no entry and no text.
    branches: std::collections::HashMap<ThreadId, SharedString>,
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
    /// The open Composer-slot picker, or None — the import file-picker
    /// (#11) or the provider picker (#25), one engine. At most one, always
    /// on the Thread that opened it; render self-heals it shut when the
    /// operator leaves that Pane, zooms below L1, or the offer expires
    /// (the Thread stops being adoptable; the first prompt locks the
    /// provider).
    picker: Option<Picker>,
    /// Where the vendors keep session files — discovery's roots, defaulted
    /// to the real homes and aimed at scratch directories by tests. Read
    /// once per picker open, never per frame.
    session_file_roots: Vec<(Provider, std::path::PathBuf)>,
    /// The launch directory's registered project (#29) — every draft's
    /// starting choice.
    launch_project: ProjectId,
    /// The open pre-prompt band popover, or None (#29). At most one, always
    /// on the focused draft Pane — a draft has no ThreadId, so the popover
    /// names its Pane by the Composer entity. Rides the ComposerMenu keys
    /// exactly like the picker; render self-heals it shut when focus leaves
    /// the draft or the draft stops being one.
    band: Option<BandPopover>,
    scope: Scope,
    rename: Option<(GroupId, Entity<crate::composer::Composer>)>,
    group_error: Option<SharedString>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    Wall,
    Group(GroupId),
}

struct NavDragPreview(SharedString);

impl Render for NavDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        nav::drag_badge(self.0.clone())
    }
}

fn drop_feedback<E: gpui::InteractiveElement>(element: E, groups: Groups, target: DropTarget) -> E {
    element.drag_over::<Drag>(move |style, drag, _, _| {
        if matches!(groups.preview_drop(*drag, target), Plan::Refused(_)) {
            style
                .bg(rgba(crate::theme::WAIT_WASH))
                .border_color(rgb(crate::theme::WAIT))
        } else {
            style
                .bg(rgba(crate::theme::ACCENT_WASH))
                .border_color(rgb(crate::theme::ACCENT))
        }
    })
}

/// The open band popover (#29): one chip's rows, discovered when it opened
/// — registry reads, never a filesystem scan — except the project chip's
/// type-a-path row, which re-derives from the Composer line per edit.
struct BandPopover {
    composer: gpui::Entity<crate::composer::Composer>,
    chip: pane::BandChip,
    rows: Vec<BandRow>,
    selected: usize,
}

/// Stable Pane identity across grid reflow.
#[derive(Clone, Debug, PartialEq)]
enum PaneIdentity {
    Thread(ThreadId),
    Draft(Entity<crate::composer::Composer>),
}

/// One band row beside its own consequence.
struct BandRow {
    label: SharedString,
    detail: SharedString,
    /// The ✓ — the draft's standing choice.
    active: bool,
    choice: BandChoice,
}

/// What picking a band row does to the focused draft.
enum BandChoice {
    Provider(ProviderChoice),
    Project(ProjectId),
    /// The type-a-path row: register the typed path as a project, then
    /// choose it.
    RegisterPath(std::path::PathBuf),
    Target(pane::DraftTarget),
}

/// The open Composer-slot picker (#11, #25): everything its popover draws,
/// discovered once when it opened — never per frame. It paints in the
/// Composer-menu slot and rides the ComposerMenu keys; each row carries
/// what picking it does, so the two can never drift apart.
struct Picker {
    thread: Option<ThreadId>,
    composer: Entity<crate::composer::Composer>,
    rows: Vec<PickRow>,
    selected: usize,
    kind: PickKind,
}

/// Which picker owns the slot. The rows, keys and dismissal are shared;
/// only the row recipe, the footer hints and the heal rule differ.
enum PickKind {
    /// #11: adopt a CLI session file into a still-blank Thread.
    ImportFile,
    /// #25: re-aim the Thread's provider / model before its first prompt.
    Provider,
    Group,
}

/// One pickable row beside its own consequence.
struct PickRow {
    row: pane::MenuRow,
    /// The ✓ — the choice this Thread is on right now (Provider rows;
    /// always false for files).
    active: bool,
    choice: Choice,
}

/// What picking a row does — Ferrite's own act either way, never a prompt.
enum Choice {
    Adopt(std::path::PathBuf),
    Provision(ProviderChoice),
    Group(GroupPick),
}

#[derive(Clone, Copy)]
enum GroupPick {
    Existing(GroupId),
    New,
    Solo,
}

/// Which popover the Composer has open, and everything it shows — rebuilt
/// on each edit of the line, never per frame.
struct ComposerMenu {
    thread: Option<ThreadId>,
    composer: Entity<crate::composer::Composer>,
    kind: MenuKind,
    rows: Vec<pane::MenuRow>,
    selected: usize,
}

enum MenuKind {
    /// `/` — the Session's own commands (Claude's initialize `commands[]`,
    /// Codex's skills/list), straight from core. Nothing static — except
    /// Ferrite's own local rows riding on top, never sent to the provider:
    /// `provider` (#25) — the picker's door pre-lock, an inert explanation
    /// after — then `import` while the Thread still offers adoption (#11).
    /// The flags say which of the two actually matched the filter, so the
    /// pick can tell a local row from the provider's.
    Commands { provider: bool, import: bool },
    /// `@` — files under the Thread's workspace binding. The walk runs once
    /// when the menu opens and is filtered per keystroke; `token_start` is
    /// where the `@` sits, so a pick knows what to splice out.
    Files {
        files: std::rc::Rc<Vec<String>>,
        token_start: usize,
    },
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
    #[cfg(test)]
    pub fn new(cockpit: Cockpit, cx: &mut Context<Self>) -> Self {
        Self::new_with_provider(cockpit, Provider::Claude, cx)
    }

    pub fn new_with_provider(
        mut cockpit: Cockpit,
        launch_provider: Provider,
        cx: &mut Context<Self>,
    ) -> Self {
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
            focused: 0,
            fullscreen: None,
            repo,
            focus: cx.focus_handle(),
            perf: std::env::var("FERRITE_PERF").is_ok().then(|| Perf {
                frames: 0,
                since: std::time::Instant::now(),
            }),
            park_order: Vec::new(),
            swept: std::time::Instant::now(),
            selection: TranscriptSelection::default(),
            nav_collapsed: false,
            hovered_usage: None,
            branches: std::collections::HashMap::new(),
            parked_rows: Vec::new(),
            menu: None,
            menu_muted: false,
            picker: None,
            session_file_roots: default_session_roots(),
            launch_project,
            band: None,
            scope: Scope::Wall,
            rename: None,
            group_error: None,
        };
        for thread in view.cockpit.threads() {
            view.open_pane(thread, cx);
        }
        // Nothing revived: the cockpit starts as one draft Pane (#29) —
        // nothing spawns before the operator's choice.
        if view.panes.is_empty() {
            view.open_draft_with_provider(pane::DraftTarget::Main, launch_provider, cx);
        }
        view.refresh_parked();
        // The first frame's wall cards — every rebuild after rides a change.
        let threads: Vec<ThreadId> = view.panes.iter().filter_map(|pane| pane.thread()).collect();
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
        self.refresh_branch(thread);
    }

    /// Refresh one Thread's cached checkout label (#29): the branch its
    /// effective cwd is actually on, read from git here — on open, on turn
    /// end, and on the watchdog cadence — and nowhere near a frame. The
    /// agent itself may switch branches, which is exactly why the header
    /// reads the repo and not the binding.
    fn refresh_branch(&mut self, thread: ThreadId) {
        let cwd = ferrite_core::workspace::effective_cwd(
            self.cockpit.session_project_root(thread),
            self.cockpit.workspace(thread),
        )
        .map(std::path::Path::to_path_buf);
        match cwd.and_then(|cwd| ferrite_core::workspace::checkout_branch(&cwd)) {
            Some(branch) => {
                self.branches.insert(thread, SharedString::from(branch));
            }
            // Not a checkout: the slot has nothing honest to say.
            None => {
                self.branches.remove(&thread);
            }
        }
    }

    /// The focused Composer's line moved: unmute and re-derive the menu.
    /// Menus follow the text — typing `/` or `@` opens, backspacing past
    /// the trigger closes, and a pick's own splice closes through here too.
    fn composer_edited(
        &mut self,
        composer: gpui::Entity<crate::composer::Composer>,
        _: &crate::composer::Edited,
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = self
            .panes
            .iter_mut()
            .find(|pane| pane.composer == composer)
            .and_then(PaneView::draft_mut)
        {
            draft.error = None;
        }
        // A picker is not text-derived (#11, #25): writing a prompt on its
        // line dismisses it — while the clearing splice that opened it
        // leaves the line empty, and keeps it.
        if self.picker.as_ref().is_some_and(|picker| {
            self.panes
                .iter()
                .any(|pane| pane.thread() == picker.thread && pane.composer == composer)
        }) && !composer.read(cx).is_empty()
        {
            self.picker = None;
        }
        // The band popover (#29) follows the picker's rule — writing a
        // prompt dismisses it — except the project chip's, whose rows read
        // the line as the type-a-path row: edits re-derive it instead.
        if let Some(band) = &self.band {
            if band.composer == composer {
                if band.chip == pane::BandChip::Project {
                    self.sync_band_rows(cx);
                } else if !composer.read(cx).is_empty() {
                    self.band = None;
                }
            }
        }
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
            // The header's checkout labels ride the same slow cadence
            // (#29): the agent may have switched branches under a Pane.
            let threads: Vec<ThreadId> =
                self.panes.iter().filter_map(|pane| pane.thread()).collect();
            for thread in threads {
                self.refresh_branch(thread);
            }
            branch_tick = true;
        }
        // A restart writes a Notice even when no Session streamed this frame —
        // and a failed respawn will never stream again, so this notify is that
        // notice's only ride to the screen.
        if frame.is_empty() && restarted.is_empty() && !branch_tick {
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
                // Turn end is the other stated refresh moment (#29): the
                // turn that just finished may have moved the checkout.
                if !self.cockpit.busy(update.thread) {
                    self.refresh_branch(update.thread);
                }
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
        let rows = self.visible_indices().len().div_ceil(columns).max(1);
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
        Level::for_cell(self.cell(window, self.scope_columns()))
    }

    fn visible_indices(&self) -> Vec<usize> {
        match self.scope {
            Scope::Wall => (0..self.panes.len()).collect(),
            Scope::Group(group) => {
                let Some(group) = self.cockpit.groups().get(group) else {
                    return Vec::new();
                };
                group
                    .members
                    .iter()
                    .filter_map(|thread| self.pane_for(*thread))
                    .chain(self.panes.iter().enumerate().filter_map(|(index, pane)| {
                        pane.draft()
                            .is_some_and(|draft| draft.pending_group == Some(group.id))
                            .then_some(index)
                    }))
                    .collect()
            }
        }
    }

    fn scope_columns(&self) -> usize {
        match self.scope {
            Scope::Wall => columns(self.visible_indices().len()),
            Scope::Group(_) => group_grid(self.visible_indices().len()).1.max(1),
        }
    }

    fn heal_scope(&mut self) {
        if let Scope::Group(group) = self.scope {
            if self.cockpit.groups().get(group).is_none() {
                self.scope = Scope::Wall;
            }
        }
    }

    fn heal_group_focus(&mut self) {
        self.heal_scope();
        let Scope::Group(group) = self.scope else {
            return;
        };
        let focused_is_visible = self.focused_thread().is_some_and(|thread| {
            self.cockpit
                .groups()
                .get(group)
                .is_some_and(|group| group.members.contains(&thread))
        });
        if !focused_is_visible {
            if let Some(next) = self.visible_indices().first().copied() {
                self.focus_pane(next);
            } else {
                self.scope = Scope::Wall;
            }
        }
    }

    fn apply_group_change(&mut self, change: GroupChange) -> Option<ferrite_core::groups::Applied> {
        match self.cockpit.apply_group(change) {
            Ok(applied) => {
                self.group_error = None;
                self.heal_group_focus();
                Some(applied)
            }
            Err(error) => {
                eprintln!("ferrite: group change refused: {error}");
                self.group_error = Some(error.to_string().into());
                None
            }
        }
    }

    fn pane_for(&self, thread: ThreadId) -> Option<usize> {
        self.panes
            .iter()
            .position(|pane| pane.thread() == Some(thread))
    }

    fn focused_thread(&self) -> Option<ThreadId> {
        self.panes.get(self.focused)?.thread()
    }

    fn submit(&mut self, _: &Submit, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some((group, editor)) = self.rename.take() {
            let title = editor.update(cx, |line, cx| line.take(cx));
            if self
                .apply_group_change(GroupChange::Rename {
                    group,
                    title: title.clone(),
                })
                .is_none()
            {
                editor.update(cx, |line, cx| line.set(title, cx));
                self.rename = Some((group, editor));
            }
            cx.notify();
            return;
        }
        // A draft's ↵ (#29): on a band chip it opens that chip's popover;
        // on the prompt line it is the first send — the bootstrap.
        if let Some(draft) = self.panes.get(self.focused).and_then(PaneView::draft) {
            match draft.band_focus {
                Some(chip) => self.open_band_popover(chip, cx),
                None => self.bootstrap_draft(cx),
            }
            return;
        }
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
        if self.rename.take().is_some() {
            cx.notify();
            return;
        }
        // On a draft, escape returns to the prompt (#29): the band's tab
        // focus clears. (An open band popover holds the ComposerMenu keys,
        // so escape there dismisses through `menu_dismiss` instead.)
        if let Some(draft) = self
            .panes
            .get_mut(self.focused)
            .and_then(PaneView::draft_mut)
        {
            draft.band_focus = None;
            cx.notify();
            return;
        }
        if let Some(thread) = self.focused_thread() {
            self.cockpit.interrupt(thread);
            self.refresh_wall(thread);
        }
        cx.notify();
    }

    /// Tab on a draft (#29): walk the band's chips, then back to the
    /// prompt. Anywhere else the key does nothing — no Pane has a second
    /// tab stop.
    fn band_cycle(&mut self, _: &BandCycle, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self
            .panes
            .get_mut(self.focused)
            .and_then(PaneView::draft_mut)
        else {
            return;
        };
        draft.band_focus = pane::BandChip::next(draft.band_focus);
        // The popover belongs to the chip it opened from; tab moves on.
        self.band = None;
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
        let visible = self.visible_indices();
        if !visible.is_empty() {
            let at = visible
                .iter()
                .position(|index| *index == self.focused)
                .unwrap_or(0);
            self.focus_pane(visible[(at + 1) % visible.len()]);
            cx.notify();
        }
    }

    fn previous_pane(&mut self, _: &PreviousPane, _window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.visible_indices();
        if !visible.is_empty() {
            let at = visible
                .iter()
                .position(|index| *index == self.focused)
                .unwrap_or(0);
            self.focus_pane(visible[(at + visible.len() - 1) % visible.len()]);
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
        if self.fullscreen.is_some() {
            self.fullscreen = None;
        } else if let Some(pane) = self.panes.get(self.focused) {
            self.fullscreen = Some(match pane.thread() {
                Some(thread) => PaneIdentity::Thread(thread),
                None => PaneIdentity::Draft(pane.composer.clone()),
            });
        }
        cx.notify();
    }

    /// cmd-b (#21): fold the nav to its 40px LED rail, or open it back to
    /// the 208px column. The width change feeds `cell()`, so Panes may
    /// legitimately change Level — size decides, no special case.
    fn toggle_nav(&mut self, _: &ToggleNav, _window: &mut Window, cx: &mut Context<Self>) {
        self.nav_collapsed = !self.nav_collapsed;
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
        // Muted until the text moves again; and never under an open picker
        // or the band popover, which hold the keyboard while up.
        if self.menu_muted || self.picker.is_some() || self.band.is_some() {
            return None;
        }
        let pane = self.panes.get(self.focused)?;
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
                return Some(ComposerMenu {
                    thread: None,
                    composer: pane.composer.clone(),
                    kind: MenuKind::Commands {
                        provider: false,
                        import: true,
                    },
                    rows: vec![row],
                    selected: 0,
                });
            };
            let mut rows = command_rows(self.cockpit.commands(thread), filter);
            // Ferrite's local rows ride on top, through the same fuzzy
            // filter and under the same cap as every row. #11: `import`
            // while the Thread still offers adoption. #25: `provider`
            // always — live before the first prompt, and kept visible but
            // inert after it, so the door's absence never reads as a bug.
            let push_local = |rows: &mut Vec<pane::MenuRow>, row: pane::MenuRow| {
                rows.insert(0, row);
                rows.truncate(MENU_ROWS_MAX);
            };
            let mut import = false;
            if pane::offers_import(self.cockpit.transcript(thread)) {
                if let Some(row) = local_row(filter, "import", "adopt a CLI session file", false) {
                    push_local(&mut rows, row);
                    import = true;
                }
            }
            let locked = self.cockpit.first_prompt_sent(thread);
            let detail = if locked {
                "locked after first prompt"
            } else {
                "switch provider / model"
            };
            let mut provider = false;
            if let Some(row) = local_row(filter, "provider", detail, locked) {
                push_local(&mut rows, row);
                provider = true;
            }
            // No match, no popover — there is nothing to pick.
            if rows.is_empty() {
                return None;
            }
            return Some(ComposerMenu {
                thread: Some(thread),
                composer: pane.composer.clone(),
                kind: MenuKind::Commands { provider, import },
                rows,
                selected: 0,
            });
        }
        let (token_start, filter) = mention_token(&text, cursor)?;
        // No binding → nothing to walk → no popover.
        let root = match (thread, pane.draft()) {
            (Some(thread), _) => self.cockpit.workspace(thread)?.cwd().to_path_buf(),
            (None, Some(draft)) => self.draft_source_root(draft)?,
            _ => return None,
        };
        // The walk runs once per open menu; keystrokes only re-filter it.
        let walked = match &self.menu {
            Some(open) if open.composer == pane.composer => match &open.kind {
                MenuKind::Files { files, .. } => Some(files.clone()),
                MenuKind::Commands { .. } => None,
            },
            _ => None,
        };
        let files = walked.unwrap_or_else(|| {
            std::rc::Rc::new(ferrite_core::workspace::mention_files(
                &root,
                MENTION_FILE_CAP,
            ))
        });
        let rows = mention_rows(&files, filter);
        if rows.is_empty() {
            return None;
        }
        Some(ComposerMenu {
            thread,
            composer: pane.composer.clone(),
            kind: MenuKind::Files { files, token_start },
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

    /// Clamp-step the open popover's selection — one stepper for both
    /// popovers, with the picker outranking the menu exactly as the pick
    /// and the paint do.
    fn step_popover(&mut self, delta: isize, cx: &mut Context<Self>) {
        let (selected, rows) = if let Some(band) = &mut self.band {
            (&mut band.selected, band.rows.len())
        } else if let Some(picker) = &mut self.picker {
            (&mut picker.selected, picker.rows.len())
        } else if let Some(menu) = &mut self.menu {
            (&mut menu.selected, menu.rows.len())
        } else {
            return;
        };
        let stepped = selected
            .saturating_add_signed(delta)
            .min(rows.saturating_sub(1));
        if stepped != *selected {
            *selected = stepped;
            cx.notify();
        }
    }

    fn menu_pick(&mut self, _: &MenuPick, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = self.band.as_ref().map(|band| band.selected) {
            self.pick_band(at, cx);
            return;
        }
        if let Some(at) = self.picker.as_ref().map(|picker| picker.selected) {
            self.pick_popover(at, cx);
            return;
        }
        if let Some(at) = self.menu.as_ref().map(|menu| menu.selected) {
            self.pick_menu(at, cx);
        }
    }

    /// Escape while a popover is up closes it and nothing else — the text
    /// stays, and escape's Interrupt meaning waits for the next press. The
    /// picker takes no mute: it is not text-derived, so nothing reopens it.
    fn menu_dismiss(&mut self, _: &MenuDismiss, _window: &mut Window, cx: &mut Context<Self>) {
        if self.band.take().is_some() {
            if let Some(draft) = self
                .panes
                .get_mut(self.focused)
                .and_then(PaneView::draft_mut)
            {
                draft.band_focus = None;
            }
            cx.notify();
            return;
        }
        if self.picker.take().is_some() {
            cx.notify();
            return;
        }
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
        let Some(pane) = self
            .panes
            .iter()
            .find(|pane| pane.composer == menu.composer)
        else {
            return;
        };
        let composer = pane.composer.clone();
        match &menu.kind {
            MenuKind::Commands { provider, import } => {
                // Draft imports preserve the typed command until adoption
                // succeeds. A refusal therefore leaves both draft and
                // prompt exactly where the operator put them.
                let Some(thread) = menu.thread else {
                    if *import && at == 0 {
                        self.open_import_picker(None, composer, cx);
                        cx.notify();
                    }
                    return;
                };
                // Every command pick replaces the whole line. The local
                // rows (#25, #11) replace it with nothing — Ferrite's own
                // act, never slash text for the provider — and open their
                // picker in the menu's place. `provider` rides above
                // `import` when both matched the filter.
                let splice = |cx: &mut Context<Self>, text: &str| {
                    composer.update(cx, |composer, cx| {
                        let whole = 0..composer.text().len();
                        composer.splice(whole, text, cx);
                    });
                };
                let mut local = 0;
                if *provider {
                    if at == local {
                        // The locked door's row is an explanation, not an
                        // offer: its pick dismisses and nothing else.
                        if !self.cockpit.first_prompt_sent(thread) {
                            splice(cx, "");
                            self.open_provider_picker(thread, cx);
                        }
                        cx.notify();
                        return;
                    }
                    local += 1;
                }
                if *import && at == local {
                    splice(cx, "");
                    self.open_import_picker(Some(thread), composer.clone(), cx);
                    cx.notify();
                    return;
                }
                splice(cx, &format!("/{} ", row.insert));
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

    /// #11: discovery and the file-pick popover, run once per open — never
    /// per frame. With nothing to list it says so in the transcript instead
    /// of opening an empty popover; the Notice is Ferrite's own out-of-band
    /// line, so the Thread keeps offering import.
    fn open_import_picker(
        &mut self,
        thread: Option<ThreadId>,
        composer: Entity<crate::composer::Composer>,
        cx: &mut Context<Self>,
    ) {
        let candidates = session_file_candidates(&self.session_file_roots, IMPORT_ROWS_MAX);
        if candidates.is_empty() {
            let roots = self
                .session_file_roots
                .iter()
                .map(|(_, root)| root.display().to_string())
                .collect::<Vec<_>>()
                .join(" or ");
            let message = format!("no CLI session files found under {roots}");
            if let Some(thread) = thread {
                self.cockpit
                    .apply_input(thread, ferrite_core::transcript::Input::Notice(message));
            } else if let Some(draft) = self
                .panes
                .iter_mut()
                .find(|pane| pane.composer == composer)
                .and_then(PaneView::draft_mut)
            {
                draft.error = Some(message.into());
            }
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
                    // choice riding beside this row.
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
                PickRow {
                    row,
                    active: false,
                    choice: Choice::Adopt(candidate.path),
                }
            })
            .collect();
        self.picker = Some(Picker {
            thread,
            composer,
            rows,
            selected: 0,
            kind: PickKind::ImportFile,
        });
        cx.notify();
    }

    /// #25: the provider picker in the Composer slot — the two Providers
    /// (✓ on the current one), then the current Provider's announced
    /// models, ✓ on the model actually serving. Discovery is core state
    /// read once at open, never per frame. Refuses to open once the first
    /// prompt has gone out: the choice is locked, and the footer is a
    /// plain label by then anyway.
    fn open_provider_picker(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if self.cockpit.first_prompt_sent(thread) {
            return;
        }
        let Some(current) = self.cockpit.provider(thread) else {
            return;
        };
        let chosen = self.cockpit.model(thread).map(str::to_string);
        // The ✓ marks what is serving: the standing choice where one was
        // picked, otherwise the model the Session's own Init announced.
        let serving = chosen.clone().or_else(|| {
            self.cockpit
                .transcript(thread)
                .and_then(|transcript| transcript.model())
                .map(str::to_string)
        });
        let label = |name: SharedString, detail: SharedString| pane::MenuRow {
            // Nothing lands in the line on ↵, exactly as the import rows.
            insert: SharedString::default(),
            name,
            matched: Vec::new(),
            detail,
            prose_detail: false,
            inert: false,
        };
        let mut rows: Vec<PickRow> = [Provider::Claude, Provider::Codex]
            .into_iter()
            .map(|provider| PickRow {
                row: label(
                    SharedString::from(provider_label(provider)),
                    SharedString::from("provider"),
                ),
                active: current == provider,
                choice: Choice::Provision(ProviderChoice {
                    provider,
                    // The current provider's row IS the standing choice,
                    // so a bare ↵ re-picks it as a true no-op; the other
                    // provider starts on its own default.
                    model: (current == provider).then(|| chosen.clone()).flatten(),
                }),
            })
            .collect();
        // Model rows come only from the Session's announcement — never
        // invented, so the other provider's models are simply absent until
        // a switch lets its Session speak. Their labels ride the chip's
        // own grooming: one spelling per model, wherever it shows.
        let model_detail = SharedString::from(format!("{} model", provider_label(current)));
        for model in self.cockpit.models(thread) {
            rows.push(PickRow {
                row: label(pane::model_chip_label(model), model_detail.clone()),
                active: serving.as_deref() == Some(model.as_str()),
                choice: Choice::Provision(ProviderChoice {
                    provider: current,
                    model: Some(model.clone()),
                }),
            });
        }
        // The arrows start on the current provider's row — bare ↵ keeps
        // everything as it is.
        let selected = rows
            .iter()
            .position(|row| {
                matches!(&row.choice, Choice::Provision(choice) if choice.provider == current
                    && choice.model == chosen)
            })
            .unwrap_or(0);
        // The `/` menu the pick came through is already closed; a chip
        // click replaces it outright.
        self.menu = None;
        self.picker = Some(Picker {
            thread: Some(thread),
            composer: self.panes[self.pane_for(thread).expect("Thread has a Pane")]
                .composer
                .clone(),
            rows,
            selected,
            kind: PickKind::Provider,
        });
        cx.notify();
    }

    /// The shared tail of ↵ and a row click on whichever picker is up:
    /// the row's own choice, dispatched. The picker closes either way.
    fn pick_popover(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        let Some(row) = picker.rows.get(at) else {
            return;
        };
        match &row.choice {
            Choice::Adopt(path) => self.adopt_file(picker.thread, &picker.composer, path, cx),
            Choice::Provision(choice) => {
                if let Some(thread) = picker.thread {
                    self.pick_provider(thread, choice.clone(), cx);
                }
            }
            Choice::Group(choice) => {
                let Some(thread) = picker.thread else {
                    return;
                };
                let current = self.cockpit.groups().of(thread).map(|group| group.id);
                let change = match *choice {
                    GroupPick::Existing(group) if current == Some(group) => {
                        self.group_error = None;
                        cx.notify();
                        return;
                    }
                    GroupPick::Existing(group) => GroupChange::Join {
                        thread,
                        group,
                        index: None,
                    },
                    GroupPick::New => GroupChange::Create {
                        seed: thread,
                        with: None,
                    },
                    GroupPick::Solo => GroupChange::Leave { thread },
                };
                if self.apply_group_change(change).is_none() {
                    self.picker = Some(picker);
                }
            }
        }
    }

    /// A provider-row pick (#25): through the core's one deep setter —
    /// lock check, spawn-new-first, durable header, fresh Transcript —
    /// and the footer relabels when the new Session's Init arrives. A
    /// refusal changed nothing: the old Session kept serving, and the
    /// provider's own words land in this Thread's transcript.
    fn pick_provider(&mut self, thread: ThreadId, choice: ProviderChoice, cx: &mut Context<Self>) {
        if let Err(e) = self.cockpit.set_provider(thread, choice) {
            self.cockpit.apply_input(
                thread,
                ferrite_core::transcript::Input::Notice(format!("provider unchanged: {e}")),
            );
        }
        self.refresh_wall(thread);
        cx.notify();
    }

    /// An import-row pick (#11): adopt the picked file through the core
    /// door. Import creates the Thread; revive opens it — the same
    /// replay-and-resume any parked Thread gets — and it takes focus. The
    /// blank Thread the door was opened from goes with it: deletion is
    /// clean exactly while it is still blank, which the picker's own
    /// invariant guarantees and this re-checks. A refusal is the core's
    /// readable words, surfaced in this Thread's transcript — and the
    /// door stays open for the next try.
    fn adopt_file(
        &mut self,
        blank: Option<ThreadId>,
        composer: &Entity<crate::composer::Composer>,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        match self.cockpit.import(path) {
            Ok(imported) => match self.cockpit.revive(imported) {
                Ok(()) => {
                    if let Some(blank) = blank {
                        self.open_pane(imported, cx);
                        if pane::offers_import(self.cockpit.transcript(blank)) {
                            match self.cockpit.delete(blank) {
                                Ok(()) => self.panes.retain(|pane| pane.thread() != Some(blank)),
                                Err(e) => {
                                    eprintln!("ferrite: the blank thread {blank} stayed open: {e}")
                                }
                            }
                        }
                    } else if let Some(index) = self
                        .panes
                        .iter()
                        .position(|pane| pane.composer == *composer)
                    {
                        composer.update(cx, |composer, cx| {
                            composer.take(cx);
                        });
                        self.panes[index].adopt_thread(imported);
                        self.focus_pane(index);
                        self.refresh_branch(imported);
                    }
                    if let Some(index) = self.pane_for(imported) {
                        self.focus_pane(index);
                    }
                    self.refresh_wall(imported);
                    self.refresh_parked();
                }
                // Durable but not on screen: the Thread sits in the nav's
                // parked rows, exactly like a launch-time import that
                // would not open.
                Err(e) => {
                    eprintln!("ferrite: imported thread {imported} would not open: {e:?}");
                    self.refresh_parked();
                }
            },
            Err(e) => {
                let message = format!("cannot import {}: {e}", path.display());
                if let Some(blank) = blank {
                    self.cockpit
                        .apply_input(blank, ferrite_core::transcript::Input::Notice(message));
                } else if let Some(draft) = self
                    .panes
                    .iter_mut()
                    .find(|pane| pane.composer == *composer)
                    .and_then(PaneView::draft_mut)
                {
                    draft.error = Some(message.into());
                }
            }
        }
        cx.notify();
    }

    // ------------------------------------------------ draft Pane + band (#29)

    /// cmd-t's answer (#29): a draft Pane — a Composer, the pre-prompt
    /// band, and nothing durable until the first send bootstraps a Thread.
    /// The provider follows the Pane the operator is on; the project starts
    /// on the launch project; `target` is the caller's (cmd-shift-n drafts
    /// straight onto "new worktree").
    fn open_draft(&mut self, target: pane::DraftTarget, cx: &mut Context<Self>) {
        let provider = self
            .panes
            .get(self.focused)
            .map(|pane| match &pane.content {
                pane::PaneContent::Thread(thread) => ProviderChoice {
                    provider: self.cockpit.provider(*thread).unwrap_or(Provider::Claude),
                    model: self.cockpit.model(*thread).map(str::to_string),
                },
                pane::PaneContent::Draft(draft) => draft.provider.clone(),
            })
            .unwrap_or(ProviderChoice {
                provider: Provider::Claude,
                model: None,
            });
        self.open_draft_with_choice(target, provider, cx);
    }

    fn open_draft_with_provider(
        &mut self,
        target: pane::DraftTarget,
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
        target: pane::DraftTarget,
        provider: ProviderChoice,
        cx: &mut Context<Self>,
    ) {
        let binding = pane::DraftBinding {
            provider,
            project: match self.scope {
                Scope::Group(group) => self
                    .cockpit
                    .groups()
                    .get(group)
                    .and_then(|group| group.members.first())
                    .and_then(|thread| self.cockpit.project_id(*thread))
                    .unwrap_or(self.launch_project),
                Scope::Wall => self.launch_project,
            },
            target,
            band_focus: None,
            error: None,
            pending_group: match self.scope {
                Scope::Group(group) => Some(group),
                Scope::Wall => None,
            },
        };
        let pane = PaneView::new_draft(binding, cx);
        cx.subscribe(&pane.composer, Self::composer_edited).detach();
        self.panes.push(pane);
        self.focus_pane(self.panes.len() - 1);
        cx.notify();
    }

    /// Open one chip's popover on the focused draft — the shared tail of a
    /// chip click and ↵ on a tab-focused chip. Toggles shut when the same
    /// chip's popover is already up. Rows are registry reads, discovered at
    /// open — never per frame, never a filesystem scan.
    fn open_band_popover(&mut self, chip: pane::BandChip, cx: &mut Context<Self>) {
        let Some(pane) = self.panes.get(self.focused) else {
            return;
        };
        let Some(draft) = pane.draft() else {
            return;
        };
        if self
            .band
            .as_ref()
            .is_some_and(|band| band.chip == chip && band.composer == pane.composer)
        {
            self.band = None;
            cx.notify();
            return;
        }
        let composer = pane.composer.clone();
        let rows = self.band_rows(draft, chip, cx);
        // The arrows start on the standing choice — bare ↵ re-picks it.
        let selected = rows.iter().position(|row| row.active).unwrap_or(0);
        self.band = Some(BandPopover {
            composer,
            chip,
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
    ) -> Vec<BandRow> {
        match chip {
            pane::BandChip::Provider => {
                let mut choices = vec![
                    ProviderChoice {
                        provider: Provider::Claude,
                        model: None,
                    },
                    ProviderChoice {
                        provider: Provider::Codex,
                        model: None,
                    },
                ];
                for model in self.cockpit.announced_models(draft.provider.provider) {
                    choices.push(ProviderChoice {
                        provider: draft.provider.provider,
                        model: Some(model),
                    });
                }
                if draft.provider.model.is_some() && !choices.contains(&draft.provider) {
                    choices.push(draft.provider.clone());
                }
                choices
                    .into_iter()
                    .map(|choice| BandRow {
                        label: SharedString::from(
                            choice
                                .model
                                .clone()
                                .unwrap_or_else(|| provider_label(choice.provider).into()),
                        ),
                        detail: SharedString::from(if choice.model.is_some() {
                            format!("{} model", provider_label(choice.provider))
                        } else {
                            "provider".into()
                        }),
                        active: draft.provider == choice,
                        choice: BandChoice::Provider(choice),
                    })
                    .collect()
            }
            pane::BandChip::Project => {
                let mut rows: Vec<BandRow> = self
                    .cockpit
                    .registry()
                    .projects()
                    .iter()
                    .map(|project| BandRow {
                        label: SharedString::from(project.title.clone()),
                        detail: SharedString::from(project.root.display().to_string()),
                        active: draft.project == project.id,
                        choice: BandChoice::Project(project.id),
                    })
                    .collect();
                // Explicit type-a-path grammar. Ordinary drafted prose is
                // never reinterpreted or erased as registry input.
                let typed = self
                    .panes
                    .get(self.focused)
                    .map(|pane| pane.composer.read(cx).text().trim().to_string())
                    .unwrap_or_default();
                if let Some(path) = typed
                    .strip_prefix("path ")
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                {
                    rows.push(BandRow {
                        label: SharedString::from(format!("add {path}")),
                        detail: SharedString::from("register path"),
                        active: false,
                        choice: BandChoice::RegisterPath(expand_home(path)),
                    });
                }
                rows
            }
            pane::BandChip::Workspace => {
                let mut rows = vec![BandRow {
                    label: SharedString::from("main"),
                    detail: SharedString::from("the project checkout"),
                    active: draft.target == pane::DraftTarget::Main,
                    choice: BandChoice::Target(pane::DraftTarget::Main),
                }];
                for entry in self.cockpit.registry().worktrees(draft.project) {
                    let branch = SharedString::from(entry.branch.clone());
                    rows.push(BandRow {
                        label: branch.clone(),
                        detail: SharedString::from("worktree"),
                        active: matches!(
                            &draft.target,
                            pane::DraftTarget::Existing { branch: chosen } if *chosen == branch
                        ),
                        choice: BandChoice::Target(pane::DraftTarget::Existing { branch }),
                    });
                }
                rows.push(BandRow {
                    label: SharedString::from("new worktree"),
                    detail: SharedString::from("created at first send"),
                    active: draft.target == pane::DraftTarget::New,
                    choice: BandChoice::Target(pane::DraftTarget::New),
                });
                rows
            }
        }
    }

    /// Re-derive the open project popover's rows from the Composer line —
    /// the type-a-path row follows the typing, exactly as the `/` menu's
    /// rows follow theirs.
    fn sync_band_rows(&mut self, cx: &mut Context<Self>) {
        let open = self
            .band
            .as_ref()
            .is_some_and(|band| band.chip == pane::BandChip::Project);
        if !open {
            return;
        }
        let Some(draft) = self.panes.get(self.focused).and_then(PaneView::draft) else {
            return;
        };
        let rows = self.band_rows(draft, pane::BandChip::Project, cx);
        if let Some(band) = &mut self.band {
            band.selected = band.selected.min(rows.len().saturating_sub(1));
            band.rows = rows;
        }
    }

    /// The shared tail of ↵ and a row click on the band popover: the row's
    /// choice, applied to the focused draft. Changing the project resets
    /// the workspace chip to `main` — the old choice named another repo's
    /// rows. The popover closes either way.
    fn pick_band(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(band) = self.band.take() else {
            return;
        };
        let Some(row) = band.rows.get(at) else {
            return;
        };
        match &row.choice {
            BandChoice::Provider(provider) => {
                if let Some(draft) = self
                    .panes
                    .get_mut(self.focused)
                    .and_then(PaneView::draft_mut)
                {
                    draft.provider = provider.clone();
                    draft.error = None;
                }
            }
            BandChoice::Project(project) => {
                if let Some(draft) = self
                    .panes
                    .get_mut(self.focused)
                    .and_then(PaneView::draft_mut)
                {
                    if draft.project != *project {
                        draft.project = *project;
                        draft.target = pane::DraftTarget::Main;
                    }
                    draft.error = None;
                }
            }
            BandChoice::RegisterPath(path) => match self.cockpit.register_project(path) {
                Ok(project) => {
                    if let Some(draft) = self
                        .panes
                        .get_mut(self.focused)
                        .and_then(PaneView::draft_mut)
                    {
                        draft.project = project;
                        draft.target = pane::DraftTarget::Main;
                        draft.error = None;
                    }
                    // The typed path was the pick's input, not a prompt:
                    // the line clears for one.
                    band.composer.update(cx, |composer, cx| {
                        let whole = 0..composer.text().len();
                        composer.splice(whole, "", cx);
                    });
                }
                Err(e) => {
                    if let Some(draft) = self
                        .panes
                        .get_mut(self.focused)
                        .and_then(PaneView::draft_mut)
                    {
                        draft.error = Some(SharedString::from(format!(
                            "cannot register {}: {e}",
                            path.display()
                        )));
                    }
                }
            },
            BandChoice::Target(target) => {
                if let Some(draft) = self
                    .panes
                    .get_mut(self.focused)
                    .and_then(PaneView::draft_mut)
                {
                    draft.target = target.clone();
                    draft.error = None;
                }
            }
        }
        cx.notify();
    }

    /// The first send (#29): resolve the draft's ids through the registry,
    /// bootstrap the Thread — create, worktree, spawn — and only then let
    /// the prompt go; the band is gone for the life of the Thread. On any
    /// failure nothing is half-born: no Thread, the Pane stays draft, the
    /// error shows where the band is, and the prompt stays in the Composer.
    fn bootstrap_draft(&mut self, cx: &mut Context<Self>) {
        let Some(pane) = self.panes.get(self.focused) else {
            return;
        };
        let Some(draft) = pane.draft() else {
            return;
        };
        let composer = pane.composer.clone();
        let text = composer.read(cx).text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let provider = draft.provider.clone();
        let pending_group = draft.pending_group;
        let resolved = self.resolve_target(draft.project, &draft.target);
        let opened = resolved.and_then(|choice| {
            match pending_group {
                Some(group) => self
                    .cockpit
                    .bootstrap_in_group(provider, choice, &text, group),
                None => self.cockpit.bootstrap(provider, choice, &text),
            }
            .map_err(|e| e.to_string())
        });
        match opened {
            Ok(thread) => {
                composer.update(cx, |composer, cx| {
                    composer.take(cx);
                });
                self.panes[self.focused].adopt_thread(thread);
                if self.fullscreen.as_ref().is_some_and(
                    |shown| matches!(shown, PaneIdentity::Draft(open) if *open == composer),
                ) {
                    self.fullscreen = Some(PaneIdentity::Thread(thread));
                }
                self.band = None;
                self.refresh_branch(thread);
                self.panes[self.focused].scroll.scroll_to_bottom();
                self.refresh_wall(thread);
            }
            Err(message) => {
                if let Some(draft) = self
                    .panes
                    .get_mut(self.focused)
                    .and_then(PaneView::draft_mut)
                {
                    draft.error = Some(SharedString::from(message));
                }
            }
        }
        cx.notify();
    }

    /// A draft's ids resolved to the core's choice: the registry answers
    /// the project's root and an existing worktree's path.
    fn resolve_target(
        &self,
        project: ProjectId,
        target: &pane::DraftTarget,
    ) -> Result<WorkspaceChoice, String> {
        let Some(entry) = self.cockpit.registry().project(project) else {
            return Err("the chosen project is no longer registered — re-pick it".into());
        };
        let repo = entry.root.clone();
        Ok(match target {
            pane::DraftTarget::Main => WorkspaceChoice::Main { checkout: repo },
            pane::DraftTarget::New => WorkspaceChoice::NewWorktree { repo },
            pane::DraftTarget::Existing { branch } => {
                let Some(worktree) = self
                    .cockpit
                    .registry()
                    .worktrees(project)
                    .iter()
                    .find(|entry| entry.branch == branch.as_ref())
                else {
                    return Err(format!(
                        "worktree {branch} is no longer registered — re-pick the workspace"
                    ));
                };
                WorkspaceChoice::ExistingWorktree {
                    repo,
                    path: worktree.path.clone(),
                }
            }
        })
    }

    /// The only source-root law for draft file completion: main and a new
    /// worktree both start from the project checkout; an existing choice
    /// walks that registered worktree itself.
    fn draft_source_root(&self, draft: &pane::DraftBinding) -> Option<std::path::PathBuf> {
        let project = self.cockpit.registry().project(draft.project)?;
        match &draft.target {
            pane::DraftTarget::Main | pane::DraftTarget::New => Some(project.root.clone()),
            pane::DraftTarget::Existing { branch } => self
                .cockpit
                .registry()
                .worktrees(draft.project)
                .iter()
                .find(|entry| entry.branch == branch.as_ref())
                .map(|entry| entry.path.clone()),
        }
    }

    /// The focused draft's band — chips wired to their popovers (#29). A
    /// chip click shares `open_band_popover` with ↵ on a tab-focused chip;
    /// the closure re-finds the Pane by its Composer, a draft's one stable
    /// identity.
    fn draft_band_element(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(draft) = self.panes[index].draft() else {
            return div().into_any_element();
        };
        let composer = self.panes[index].composer.clone();
        let project_title = self
            .cockpit
            .registry()
            .project(draft.project)
            .map(|project| project.title.clone())
            .unwrap_or_else(|| "project".into());
        let workspace_label = match &draft.target {
            pane::DraftTarget::Main => SharedString::from("main"),
            pane::DraftTarget::Existing { branch } => branch.clone(),
            pane::DraftTarget::New => SharedString::from("new worktree"),
        };
        let chips = [
            (
                pane::BandChip::Provider,
                pane::provider_chip_label(
                    provider_label(draft.provider.provider),
                    draft.provider.model.as_deref(),
                ),
                true,
            ),
            (
                pane::BandChip::Project,
                pane::band_chip_label(&project_title),
                false,
            ),
            (
                pane::BandChip::Workspace,
                pane::band_chip_label(&workspace_label),
                false,
            ),
        ];
        let mut band = pane::draft_band();
        for (slot, (chip, label, accent)) in chips.into_iter().enumerate() {
            let focused = draft.band_focus == Some(chip);
            let chip_composer = composer.clone();
            band = band.child(pane::band_chip(slot, label, accent, focused).on_mouse_down(
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
        band.child(div().flex_1())
            .child(pane::band_hint())
            .into_any_element()
    }

    /// The open band popover for this draft Pane, rows wired to their
    /// picks — the picker's exact paint, in the Composer-menu slot.
    fn band_popover_element(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let band = self
            .band
            .as_ref()
            .filter(|band| band.composer == self.panes[index].composer)?;
        let mut popover = pane::menu_popover().on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
        );
        for (at, row) in band.rows.iter().enumerate() {
            popover = popover.child(
                pane::picker_row(
                    row.label.clone(),
                    row.detail.clone(),
                    at == band.selected,
                    row.active,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.pick_band(at, cx);
                    }),
                ),
            );
        }
        let footer = if band.chip == pane::BandChip::Project {
            "type path <dir> · ↑↓ move · ↵ pick · esc dismiss"
        } else {
            "↑↓ move · ↵ pick · esc dismiss"
        };
        popover = popover.child(pane::popover_footer(footer));
        Some(popover.into_any_element())
    }

    /// A draft owns the same single Composer popover slot as a Thread. The
    /// band selector outranks text-derived menus and pickers while open.
    fn draft_popover_element(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.band_popover_element(index, cx)
            .or_else(|| self.composer_menu(index, cx))
    }

    /// A draft's – / ✕ controls: ✕ discards the draft — nothing durable
    /// exists to park — and – zooms a fullscreened cockpit back, exactly as
    /// on a Thread Pane.
    fn draft_controls(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let composer = self.panes[index].composer.clone();
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(2.))
            .child(
                pane::control_button(("draft-zoom", index), "–").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if view.fullscreen.take().is_some() {
                            cx.notify();
                        }
                    }),
                ),
            )
            .child(
                pane::control_button(("draft-close", index), "✕").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.discard_draft(&composer, cx);
                    }),
                ),
            )
            .into_any_element()
    }

    /// Test-only: aim the launch project at a scratch repo — production
    /// registers `here()` once at construction, which tests cannot sit in.
    #[cfg(test)]
    fn aim_launch(&mut self, root: &std::path::Path) {
        self.repo = root.to_path_buf();
        self.launch_project = self
            .cockpit
            .register_project(root)
            .expect("the scratch repo registers");
        for pane in &mut self.panes {
            if let Some(draft) = pane.draft_mut() {
                draft.project = self.launch_project;
            }
        }
    }

    /// Discard one draft Pane — cmd-w's draft half and the ✕'s shared
    /// tail. Nothing durable dies with it; the prompt text does, which is
    /// what closing an unsent draft means.
    fn discard_draft(
        &mut self,
        composer: &gpui::Entity<crate::composer::Composer>,
        cx: &mut Context<Self>,
    ) {
        let Some(at) = self
            .panes
            .iter()
            .position(|pane| pane.draft().is_some() && pane.composer == *composer)
        else {
            return;
        };
        self.panes.remove(at);
        self.band = None;
        self.focus_pane(self.focused.min(self.panes.len().saturating_sub(1)));
        cx.notify();
    }

    /// The open menu's popover for this Pane, rows wired to their picks —
    /// assembled here so its clicks land beside every other pointer wire
    /// (the root selector's precedent); the Pane hangs it above the line.
    /// A picker owns the slot while it is up (#11, #25): it opened from
    /// the menu — which closed on the pick — or from the footer chip.
    fn composer_menu(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(picker) = self
            .picker
            .as_ref()
            .filter(|picker| picker.composer == self.panes[index].composer)
        {
            let mut popover = pane::menu_popover().on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            );
            for (at, pick_row) in picker.rows.iter().enumerate() {
                let drawn = match picker.kind {
                    PickKind::ImportFile => pane::menu_row(&pick_row.row, at == picker.selected),
                    // The ✓ grammar — what the Thread is on right now —
                    // is the root selector's row, shared; the muted detail
                    // tags the section ("provider", "claude model").
                    PickKind::Provider => pane::picker_row(
                        pick_row.row.name.clone(),
                        pick_row.row.detail.clone(),
                        at == picker.selected,
                        pick_row.active,
                    ),
                    PickKind::Group => pane::picker_row(
                        pick_row.row.name.clone(),
                        pick_row.row.detail.clone(),
                        at == picker.selected,
                        pick_row.active,
                    ),
                };
                popover = popover.child(drawn.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.pick_popover(at, cx);
                    }),
                ));
            }
            // A model section still empty is said out loud: the list is
            // short because the Session has not spoken, not broken (#25).
            if matches!(picker.kind, PickKind::Provider) && picker.rows.len() == 2 {
                popover = popover.child(pane::picker_hint("models arrive with the handshake"));
            }
            let hints = match picker.kind {
                PickKind::ImportFile => "↑↓ select · ↵ adopt · esc dismiss",
                PickKind::Provider => "↑↓ move · ↵ pick · esc dismiss",
                PickKind::Group => "↑↓ move · ↵ pick · esc dismiss",
            };
            popover = popover.child(pane::popover_footer(hints));
            return Some(popover.into_any_element());
        }
        let menu = self
            .menu
            .as_ref()
            .filter(|menu| menu.composer == self.panes[index].composer)?;
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
            MenuKind::Commands { .. } => "↑↓ select · ↵ run · esc dismiss",
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
        // Drafts are not rows: nothing runs, nothing parks, nothing to aim
        // the nav at (#29) — the grid is where a draft lives.
        let running: Vec<nav::RunningRow> = self
            .panes
            .iter()
            .enumerate()
            .filter_map(|(index, pane)| {
                let thread = pane.thread()?;
                let transcript = self.cockpit.transcript(thread);
                Some(nav::RunningRow {
                    thread,
                    name: SharedString::from(format!("thread-{thread:02}")),
                    binding: pane::binding_label(self.cockpit.workspace(thread)),
                    provider: nav::provider_tag(self.cockpit.provider(thread)),
                    status: transcript.map(|t| t.status()).unwrap_or_default(),
                    needs_you: self.cockpit.pending(thread).is_some(),
                    todos: transcript.and_then(|t| t.todos()),
                    focused: index == self.focused,
                })
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
            self.scope = self
                .cockpit
                .groups()
                .of(thread)
                .map_or(Scope::Wall, |group| Scope::Group(group.id));
            self.focus_pane(index);
            cx.notify();
        }
    }

    fn enter_group(&mut self, group: GroupId, cx: &mut Context<Self>) {
        self.scope = Scope::Group(group);
        if let Some(index) = self.cockpit.groups().get(group).and_then(|group| {
            group
                .members
                .iter()
                .find_map(|thread| self.pane_for(*thread))
        }) {
            self.focus_pane(index);
        }
        cx.notify();
    }

    fn apply_drop(&mut self, drag: Drag, target: DropTarget, cx: &mut Context<Self>) {
        match self.cockpit.plan_group_drop(drag, target) {
            Plan::Change(change) => {
                self.apply_group_change(change);
            }
            Plan::Refused(_) => self.group_error = None,
            Plan::Nothing => self.group_error = None,
        }
        self.heal_group_focus();
        cx.notify();
    }

    /// Revive one parked Thread: a Pane, focus, and the park order and the
    /// nav's cache both forgetting it — cmd-o must not revive it a second
    /// time. The shared tail of cmd-o and a parked nav row's click (#21).
    fn revive_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        match self.cockpit.revive(thread) {
            Ok(()) => {
                self.scope = self
                    .cockpit
                    .groups()
                    .of(thread)
                    .map_or(Scope::Wall, |group| Scope::Group(group.id));
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
            self.fullscreen = self.panes.get(index).map(|pane| match pane.thread() {
                Some(thread) => PaneIdentity::Thread(thread),
                None => PaneIdentity::Draft(pane.composer.clone()),
            });
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
        self.open_draft(pane::DraftTarget::New, cx);
    }

    /// cmd-t / cmd-n (#29): a draft Pane, not a Thread. The band chooses;
    /// the first send bootstraps.
    fn new_thread(&mut self, _: &NewThread, _window: &mut Window, cx: &mut Context<Self>) {
        self.open_draft(pane::DraftTarget::Main, cx);
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
    /// reopening it revives the Thread. A draft has nothing to park and is
    /// simply discarded (#29).
    fn close_thread(&mut self, _: &CloseThread, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get(self.focused) {
            if pane.draft().is_some() {
                let composer = pane.composer.clone();
                self.discard_draft(&composer, cx);
                return;
            }
        }
        let Some(thread) = self.focused_thread() else {
            return;
        };
        self.remove_thread_from_view(thread, cx);
    }

    fn remove_thread_from_view(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if matches!(self.scope, Scope::Group(_)) {
            if let Some(applied) = self.apply_group_change(GroupChange::Leave { thread }) {
                if applied.dissolved.is_some() {
                    self.scope = Scope::Wall;
                }
                let visible = self.visible_indices();
                if let Some(next) = visible.first().copied() {
                    self.focus_pane(next);
                }
                cx.notify();
            }
            return;
        }
        self.park_thread(thread, cx);
    }

    /// Park one Thread — cmd-w's whole body, shared with the header's ✕
    /// control so the pointer and the keyboard close through one door.
    fn park_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if let Err(e) = self.cockpit.park(thread) {
            eprintln!("ferrite: thread {thread} did not park cleanly: {e}");
        }
        // Parked even on a flush error — the Session is gone either way, so
        // cmd-o should still bring this Thread back first.
        self.park_order.push(thread);
        self.panes.retain(|pane| pane.thread() != Some(thread));
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
            self.scope = self
                .cockpit
                .groups()
                .of(next)
                .map_or(Scope::Wall, |group| Scope::Group(group.id));
            self.focus_pane(pane);
            cx.notify();
        }
    }

    fn toggle_group(&mut self, _: &ToggleGroup, _window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.scope, Scope::Group(_)) {
            self.scope = Scope::Wall;
        } else if let Some(thread) = self.focused_thread() {
            let group = self
                .cockpit
                .groups()
                .of(thread)
                .map(|group| group.id)
                .or_else(|| {
                    self.apply_group_change(GroupChange::Create {
                        seed: thread,
                        with: None,
                    })?
                    .group
                });
            if let Some(group) = group {
                self.scope = Scope::Group(group);
            }
        }
        cx.notify();
    }

    fn move_to_group(&mut self, _: &MoveToGroup, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        let current = self.cockpit.groups().of(thread).map(|group| group.id);
        let project = match self.cockpit.try_project_id(thread) {
            Ok(Some(project)) => project,
            Ok(None) => {
                self.group_error = Some("Thread project metadata is missing".into());
                cx.notify();
                return;
            }
            Err(error) => {
                self.group_error = Some(error.to_string().into());
                cx.notify();
                return;
            }
        };
        let row = |name: String, detail: &'static str| pane::MenuRow {
            insert: SharedString::default(),
            name: name.into(),
            matched: Vec::new(),
            detail: detail.into(),
            prose_detail: false,
            inert: false,
        };
        let mut rows = Vec::new();
        for group in self.cockpit.groups().iter() {
            let Some(first) = group.members.first().copied() else {
                continue;
            };
            match self.cockpit.try_project_id(first) {
                Ok(Some(group_project)) if group_project == project => rows.push(PickRow {
                    row: row(group.display_title(), "group"),
                    active: current == Some(group.id),
                    choice: Choice::Group(GroupPick::Existing(group.id)),
                }),
                Ok(Some(_)) => {}
                Ok(None) => {
                    self.group_error = Some("A group has missing project metadata".into());
                    cx.notify();
                    return;
                }
                Err(error) => {
                    self.group_error = Some(error.to_string().into());
                    cx.notify();
                    return;
                }
            }
        }
        rows.push(PickRow {
            row: row("new group".into(), "create"),
            active: false,
            choice: Choice::Group(GroupPick::New),
        });
        rows.push(PickRow {
            row: row("solo".into(), "remove from group"),
            active: current.is_none(),
            choice: Choice::Group(GroupPick::Solo),
        });
        let selected = rows.iter().position(|row| row.active).unwrap_or(0);
        let composer = self.panes[self.focused].composer.clone();
        self.picker = Some(Picker {
            thread: Some(thread),
            composer,
            rows,
            selected,
            kind: PickKind::Group,
        });
        cx.notify();
    }

    fn rename_group(&mut self, _: &RenameGroup, _window: &mut Window, cx: &mut Context<Self>) {
        let Scope::Group(group) = self.scope else {
            return;
        };
        let title = self
            .cockpit
            .groups()
            .get(group)
            .map(|group| group.display_title())
            .unwrap_or_default();
        let editor = cx.new(crate::composer::Composer::new);
        editor.update(cx, |editor, cx| editor.set(title, cx));
        self.rename = Some((group, editor));
        cx.notify();
    }

    fn move_group(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Scope::Group(group) = self.scope else {
            return;
        };
        let at = self
            .cockpit
            .groups()
            .iter()
            .position(|item| item.id == group)
            .unwrap_or(0);
        let last = self.cockpit.groups().iter().count().saturating_sub(1);
        let destination = (at as isize + delta).clamp(0, last as isize) as usize;
        if destination == at {
            self.group_error = None;
            cx.notify();
            return;
        }
        let gap = destination + usize::from(destination > at);
        self.apply_group_change(GroupChange::MoveGroup { group, index: gap });
        cx.notify();
    }
    fn move_group_up(&mut self, _: &MoveGroupUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_group(-1, cx);
    }
    fn move_group_down(&mut self, _: &MoveGroupDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_group(1, cx);
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
    fn pointer_down(&mut self, index: usize, event: &MouseDownEvent, cx: &mut Context<Self>) {
        self.focus_pane(index);
        // A draft has no transcript to select in (#29).
        if let Some(pane) = self.panes.get(index) {
            if let Some(thread) = pane.thread() {
                self.selection.begin(
                    thread,
                    event.position,
                    event.click_count,
                    pane.scroll.bounds(),
                );
            }
        }
        cx.notify();
    }

    /// Dragging with the button held sweeps characters into the selection.
    /// Wired on the root, not the Pane, so the sweep keeps following a
    /// pointer that has left the Pane div — and it aims only at the
    /// gripped Thread's own transcript body, whose rect `extend` clamps
    /// into: leaving through the Composer or the Pane's edge selects to
    /// the boundary, never into chrome or a neighbour.
    fn pointer_drag(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some(thread) = self.selection.gripping_thread() else {
            return;
        };
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let body = self.panes[index].scroll.bounds();
        if self.selection.extend(thread, event.position, body) {
            cx.notify();
        }
    }

    /// Exactly the highlighted text to the clipboard. With nothing visibly
    /// selected — cleared, or every selected row gone from the rendered
    /// window — the clipboard is left alone.
    fn copy_selection(&mut self, _: &CopySelection, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selection.copied_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
}

#[derive(Clone, Copy)]
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

/// Where the vendors write session files, per the layouts the import
/// fixtures capture: `~/.claude/projects/<slug>/<session>.jsonl` and
/// `~/.codex/sessions/<date dirs>/rollout-*.jsonl`. Windows spells the
/// home directory USERPROFILE, not HOME.
fn default_session_roots() -> Vec<(Provider, std::path::PathBuf)> {
    let home = std::path::PathBuf::from(
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".into()),
    );
    vec![
        (Provider::Claude, home.join(".claude").join("projects")),
        (Provider::Codex, home.join(".codex").join("sessions")),
    ]
}

/// One session file discovery found: which vendor's root it was under, and
/// when the vendor last wrote it.
struct ImportCandidate {
    provider: Provider,
    path: std::path::PathBuf,
    modified: Option<std::time::SystemTime>,
}

/// Candidate session files under the vendors' roots: every `.jsonl` in
/// either tree — the layouts differ (per-project slugs vs per-date
/// directories), so the walk recurses instead of assuming a depth, the
/// same read the live import probes use — newest first, capped. A missing
/// root lists nothing: that vendor was simply never run here. Whether a
/// candidate really is an adoptable session stays the import parser's
/// verdict, not the filename's.
fn session_file_candidates(
    roots: &[(Provider, std::path::PathBuf)],
    cap: usize,
) -> Vec<ImportCandidate> {
    fn walk(provider: Provider, dir: &std::path::Path, into: &mut Vec<ImportCandidate>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(provider, &path, into);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
            {
                into.push(ImportCandidate {
                    provider,
                    modified: std::fs::metadata(&path)
                        .and_then(|metadata| metadata.modified())
                        .ok(),
                    path,
                });
            }
        }
    }
    let mut found = Vec::new();
    for (provider, root) in roots {
        walk(*provider, root, &mut found);
    }
    // Newest first; a file with no readable mtime sorts oldest rather than
    // vanishing.
    found.sort_by_key(|candidate| std::cmp::Reverse(candidate.modified));
    found.truncate(cap);
    found
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

/// The provider's lowercase name — the store's own serialized spelling.
fn provider_label(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    }
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
        self.heal_scope();
        // A fullscreened Thread whose Pane is gone — removed by a path that
        // bypassed `focus_pane` — falls back to the grid, never a blank
        // cockpit. Render is the one chokepoint every removal passes.
        let fullscreen = self.fullscreen.as_ref().and_then(|shown| match shown {
            PaneIdentity::Thread(thread) => self.pane_for(*thread),
            PaneIdentity::Draft(composer) => self
                .panes
                .iter()
                .position(|pane| pane.composer == *composer && pane.draft().is_some()),
        });
        if self.fullscreen.is_some() && fullscreen.is_none() {
            self.fullscreen = None;
        }
        let attention = self.attention();
        let level = self.level_now(window);

        // A selection is only real while its rows draw (#27): zooming below
        // L1, parking the Pane, or fullscreening another Thread clears it
        // here rather than leaving invisible clipboard state behind cmd-c.
        // The registries of Threads without a Pane go with it.
        if self.selection.active_thread().is_some_and(|thread| {
            level != Level::Transcript
                || self.pane_for(thread).is_none()
                || self.fullscreen.as_ref().is_some_and(
                    |shown| !matches!(shown, PaneIdentity::Thread(open) if *open == thread),
                )
        }) {
            self.selection.clear();
        }
        let panes = &self.panes;
        self.selection
            .retain_threads(|thread| panes.iter().any(|pane| pane.thread() == Some(thread)));

        // The Composer menu belongs to the focused Pane's line at L1 (#23):
        // leaving that Pane, or zooming below L1, closes it here.
        if self.menu.as_ref().is_some_and(|menu| {
            self.panes
                .get(self.focused)
                .is_none_or(|pane| pane.composer != menu.composer)
                || level != Level::Transcript
        }) {
            self.menu = None;
        }
        // And the picker (#11, #25): it belongs to the Thread that opened
        // it, at L1 — and it closes the moment its offer expires. The
        // import picker's Thread stopping being adoptable means a pick can
        // never delete a Thread that is no longer blank; the provider
        // picker's lock arming means nothing re-aims after the first
        // prompt, however it went out.
        if self.picker.as_ref().is_some_and(|picker| {
            self.panes
                .get(self.focused)
                .is_none_or(|pane| pane.composer != picker.composer)
                || match picker.kind {
                    PickKind::ImportFile => {
                        level != Level::Transcript
                            || picker.thread.is_some_and(|thread| {
                                !pane::offers_import(self.cockpit.transcript(thread))
                            })
                    }
                    PickKind::Provider => {
                        level != Level::Transcript
                            || picker
                                .thread
                                .is_none_or(|thread| self.cockpit.first_prompt_sent(thread))
                    }
                    PickKind::Group => false,
                }
        }) {
            self.picker = None;
        }
        // And the band popover (#29): it belongs to the focused draft Pane
        // at L1 — leaving the draft, or the draft becoming a Thread, closes
        // it here.
        if self.band.as_ref().is_some_and(|band| {
            level != Level::Transcript
                || self
                    .panes
                    .get(self.focused)
                    .map(|pane| pane.draft().is_none() || pane.composer != band.composer)
                    .unwrap_or(true)
        }) {
            self.band = None;
        }
        // The open menu — or the picker or band popover riding the same
        // keys — widens its Composer's own key context to ComposerMenu: the
        // focused node, where enter and escape can win their tie against
        // Submit and Interrupt. Render is the one chokepoint every open,
        // pick, dismissal and heal passes.
        let menu_thread = self.menu.as_ref().and_then(|menu| menu.thread);
        let picker_thread = self.picker.as_ref().and_then(|picker| picker.thread);
        let group_picker_open = self
            .picker
            .as_ref()
            .is_some_and(|picker| matches!(picker.kind, PickKind::Group));
        for pane in &self.panes {
            let thread = pane.thread();
            let open = (thread.is_some() && (thread == menu_thread || thread == picker_thread))
                || self
                    .menu
                    .as_ref()
                    .is_some_and(|menu| menu.composer == pane.composer)
                || self
                    .picker
                    .as_ref()
                    .is_some_and(|picker| picker.composer == pane.composer)
                || self
                    .band
                    .as_ref()
                    .is_some_and(|band| band.composer == pane.composer);
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
            .rename
            .as_ref()
            .map(|(_, editor)| editor.focus_handle(cx))
            .or_else(|| {
                self.panes.get(self.focused).and_then(|pane| match level {
                    // At L1 the Composer keeps the keyboard even while a
                    // Decision pends: the card is part of its stack and the
                    // input stays live (PromptBox state 04) — y/n/a answer
                    // through the region's own Decision key context (#23).
                    Level::Transcript => Some(pane.composer.focus_handle(cx)),
                    _ if pane
                        .thread()
                        .is_some_and(|thread| self.cockpit.pending(thread).is_some())
                        && level != Level::Wall =>
                    {
                        Some(pane.decision_focus.clone())
                    }
                    _ => None,
                })
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
            let visible = self.visible_indices();
            let columns = self.scope_columns();
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
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(crate::theme::GROUND))
            .font_family(crate::theme::FONT_MONO)
            .track_focus(&self.focus)
            // At wall range no Pane holds a Composer, so the answer keys are
            // not competing with typing: they answer whichever Thread is
            // flagged, without the operator focusing it first.
            .when(level == Level::Wall && !group_picker_open, |wall| {
                wall.key_context("Wall")
            })
            .when(group_picker_open, |root| root.key_context("ComposerMenu"))
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
            .on_action(cx.listener(Self::close_thread))
            .on_action(cx.listener(Self::reopen_thread))
            .on_action(cx.listener(Self::copy_selection))
            .on_action(cx.listener(Self::toggle_fullscreen))
            .on_action(cx.listener(Self::toggle_nav))
            .on_action(cx.listener(Self::menu_next))
            .on_action(cx.listener(Self::menu_previous))
            .on_action(cx.listener(Self::menu_pick))
            .on_action(cx.listener(Self::menu_dismiss))
            .on_action(cx.listener(Self::toggle_group))
            .on_action(cx.listener(Self::move_to_group))
            .on_action(cx.listener(Self::rename_group))
            .on_action(cx.listener(Self::move_group_up))
            .on_action(cx.listener(Self::move_group_down))
            // The root covers the window, so a release anywhere ends the
            // drag; the selection it made stays until the next press. Moves
            // ride the root too (#27): a sweep keeps extending after the
            // pointer leaves the Pane div, clamped to the origin transcript.
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _: &MouseUpEvent, _, _| view.selection.release()),
            )
            .on_mouse_move(
                cx.listener(|view, event: &MouseMoveEvent, _, cx| view.pointer_drag(event, cx)),
            )
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
                    if view.menu.take().is_some() {
                        view.menu_muted = true;
                        dismissed = true;
                    }
                    if view.picker.take().is_some() {
                        dismissed = true;
                    }
                    if view.band.take().is_some() {
                        dismissed = true;
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
            .capture_any_mouse_down(cx.listener(|view, _: &MouseDownEvent, _, cx| {
                if view.selection.clear() {
                    cx.notify();
                }
            }))
            // The strip spans the window and owns the blended titlebar band
            // (#22 D24); below it the nav runs the remaining height on the
            // left and the grid takes the rest. Fullscreen keeps the nav
            // visible — a deliberate override of sidebar-and-impl.md §3
            // ("the nav hides entirely"): the fullscreened Pane spans the
            // area right of the nav, so the swarm stays one click away
            // (#21).
            .child(self.strip(attention))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .child(self.nav(cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(grid)
                            // The pinned legend teaches the encoding at the
                            // instrument levels; L1 has words and does not
                            // need it (#22 D18).
                            .children(
                                (level != Level::Transcript && fullscreen.is_none()).then(legend),
                            ),
                    ),
            )
    }
}

impl CockpitView {
    /// One Pane's cell — the click-to-focus and drag plumbing around
    /// `render_pane`. The same cell serves a grid slot and the fullscreen
    /// view; only who lays it out differs.
    fn pane_cell(&self, index: usize, level: Level, cx: &mut Context<Self>) -> Div {
        let pane = &self.panes[index];
        let focused = index == self.focused;
        let cell = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    view.pointer_down(index, event, cx)
                }),
            );
        // A draft Pane (#29): the band and its popover instead of a
        // transcript — nothing in core exists to read yet.
        let Some(thread) = pane.thread() else {
            let draft = pane.draft().expect("a Pane is a Thread or a draft");
            return cell.child(pane::render_draft(
                pane,
                pane::DraftState {
                    band: self.draft_band_element(index, cx),
                    menu: (level == Level::Transcript)
                        .then(|| self.draft_popover_element(index, cx))
                        .flatten(),
                    composer_empty: pane.composer.read(cx).is_empty(),
                    focused,
                    controls: (level == Level::Transcript).then(|| self.draft_controls(index, cx)),
                    error: draft.error.as_ref(),
                },
                level,
            ));
        };
        // The frame's selection seam for this Pane (#27), resolved against
        // exactly the rows the body will draw — the shared rendered window,
        // because copy is what you see.
        let selection = {
            let blocks = self
                .cockpit
                .transcript(thread)
                .map(|transcript| transcript.blocks())
                .unwrap_or(&[]);
            self.selection
                .overlay(thread, pane::rendered_window(blocks, level))
        };
        let mut rendered = cell.child(pane::render_pane(
            pane,
            pane::PaneState {
                transcript: self.cockpit.transcript(thread),
                decision: self.cockpit.pending(thread),
                queued: self.cockpit.queued(thread),
                workspace: self.cockpit.workspace(thread),
                // The cached checkout label (#29) — display-only, and only
                // where the L1 header draws its binding slot.
                branch: (level == Level::Transcript)
                    .then(|| self.branches.get(&thread).cloned())
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
                    .then(|| self.cockpit.permission_mode(thread))
                    .flatten(),
                // The footer's provider control — pre-lock only, and
                // only where the meta row renders (#25).
                provider_chip: (level == Level::Transcript)
                    .then(|| self.provider_chip(index, cx))
                    .flatten(),
                focused,
                running: self.cockpit.busy(thread),
                selection,
                timings: self.cockpit.tool_timings(thread),
                // The ring and the window controls live on the L1
                // header only, like the root chip.
                usage_ring: (level == Level::Transcript)
                    .then(|| self.usage_ring(index, cx))
                    .flatten(),
                controls: (level == Level::Transcript).then(|| self.pane_controls(thread, cx)),
                // The Decision keycaps, wired where a card can draw
                // them — the wall answers with keys alone.
                decide: (level != Level::Wall)
                    .then(|| self.decide_keycaps(index, level, cx))
                    .flatten(),
            },
            level,
        ));
        if level != Level::Transcript
            && focused
            && self.picker.as_ref().is_some_and(|picker| {
                matches!(picker.kind, PickKind::Group) && picker.composer == pane.composer
            })
        {
            if let Some(menu) = self.composer_menu(index, cx) {
                rendered = rendered.child(deferred(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .mx(px(crate::theme::GRID_GAP))
                        .mb(px(crate::theme::GRID_GAP))
                        .child(menu),
                ));
            }
        }
        rendered
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
        let decision = self.cockpit.pending(thread)?;
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

    /// The header's context ring with its hover card (#22 C12) — assembled
    /// here so the hover state lives beside every other pointer wire; the
    /// Pane only places it. None until the provider reports a window.
    fn usage_ring(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.panes[index].thread()?;
        let usage = self.cockpit.transcript(thread)?.usage()?;
        let window = usage.context_window.filter(|window| *window > 0)?;
        let fraction = usage.total_tokens as f32 / window as f32;
        let mut ring = div()
            .id(("usage-ring", thread.get() as usize))
            .relative()
            .flex_shrink_0()
            .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
                view.hovered_usage = hovered.then_some(thread);
                cx.notify();
            }))
            .child(pane::usage_ring(fraction));
        if self.hovered_usage == Some(thread) {
            // Deferred like every popover, so the card escapes the Pane's
            // clip and draws over whatever is beside the header.
            ring = ring.child(deferred(
                div()
                    .absolute()
                    .top(relative(1.))
                    .right_0()
                    .mt(px(4.))
                    .child(pane::usage_card(usage)),
            ));
        }
        Some(ring.into_any_element())
    }

    /// The header's – / ✕ window controls (#22 amendment), wired to verbs
    /// the keyboard already has: ✕ parks the Thread exactly like cmd-w;
    /// – zooms a fullscreened Pane back to the grid and is quiet otherwise.
    /// Presses stop propagation so the Pane's own press handler cannot
    /// re-focus what was just closed.
    fn pane_controls(&self, thread: ThreadId, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(2.))
            .child(
                pane::control_button(("pane-zoom", thread.get() as usize), "–").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if view.fullscreen.take().is_some() {
                            cx.notify();
                        }
                    }),
                ),
            )
            .child(
                pane::control_button(("pane-close", thread.get() as usize), "✕").on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.remove_thread_from_view(thread, cx);
                    }),
                ),
            )
            .into_any_element()
    }

    /// The composer footer's provider control (#25): pre-lock, the accent
    /// chip whose click opens the provider picker — assembled here so the
    /// click lands beside every other pointer wire, exactly like the
    /// header chips. None once the first prompt has gone out: the Pane
    /// draws today's plain muted label instead.
    fn provider_chip(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let thread = self.panes[index].thread()?;
        if self.cockpit.first_prompt_sent(thread) {
            return None;
        }
        let provider = self.cockpit.provider(thread)?;
        // The Session's own Init names what is serving; until it speaks the
        // chip carries the provider name alone — never an invented model.
        let model = self.cockpit.transcript(thread).and_then(|t| t.model());
        let label = pane::provider_chip_label(provider_label(provider), model);
        Some(
            pane::provider_chip(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                        // The chip is this Pane's: land on it first, then
                        // toggle — and stop the press so the root's
                        // dismissal cannot close what this just opened.
                        cx.stop_propagation();
                        if let Some(index) = view.pane_for(thread) {
                            view.focus_pane(index);
                        }
                        view.toggle_provider_picker(thread, cx);
                    }),
                )
                .into_any_element(),
        )
    }

    /// The chip's click: close an open provider picker on this Thread, or
    /// open one — the root chip's toggle grammar.
    fn toggle_provider_picker(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if let Some(open) = self.picker.take() {
            if open.thread == Some(thread) && matches!(open.kind, PickKind::Provider) {
                cx.notify();
                return;
            }
        }
        self.open_provider_picker(thread, cx);
    }

    /// How many Threads hold the operator up right now — the strip's amber
    /// count, the nav's `waiting`, and the wall's ring census, all through
    /// `pane::needs_operator` so no two surfaces can disagree.
    fn attention(&self) -> usize {
        self.panes
            .iter()
            .filter_map(|pane| pane.thread())
            .filter(|thread| {
                pane::needs_operator(
                    self.cockpit.pending(*thread).is_some(),
                    self.cockpit.transcript(*thread).map(|t| t.status()),
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
        if let Some(error) = &self.group_error {
            rows = rows.child(
                div()
                    .px(px(crate::theme::GRID_PAD))
                    .py(px(crate::theme::POPOVER_PAD))
                    .text_size(px(crate::theme::TEXT_CHIP))
                    .text_color(rgb(crate::theme::WAIT))
                    .bg(rgba(crate::theme::WAIT_WASH))
                    .child(error.clone()),
            );
        }
        let grouped: std::collections::HashSet<ThreadId> = self
            .cockpit
            .groups()
            .iter()
            .flat_map(|group| group.members.iter().copied())
            .collect();
        for (group_index, group) in self.cockpit.groups().iter().enumerate() {
            let id = group.id;
            let gap_target = DropTarget::GroupGap(group_index);
            rows = rows.child(
                drop_feedback(
                    div().h(px(crate::theme::POPOVER_PAD)),
                    self.cockpit.groups().clone(),
                    gap_target,
                )
                .on_drop(cx.listener(move |view, drag: &Drag, _, cx| {
                    view.apply_drop(*drag, gap_target, cx)
                })),
            );
            let header = match &self.rename {
                Some((editing, editor)) if *editing == id => {
                    nav::group_editor(id).child(editor.clone())
                }
                _ => nav::group_header(
                    id,
                    group.display_title().into(),
                    group.members.len(),
                    self.scope == Scope::Group(id),
                ),
            };
            rows = rows.child(
                drop_feedback(
                    header,
                    self.cockpit.groups().clone(),
                    DropTarget::GroupHeader(id),
                )
                .on_drag(Drag::Group(id), move |_, _, _, cx| {
                    cx.new(|_| NavDragPreview("group".into()))
                })
                .on_drop(cx.listener(move |view, drag: &Drag, _, cx| {
                    view.apply_drop(*drag, DropTarget::GroupHeader(id), cx)
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &MouseDownEvent, _, cx| view.enter_group(id, cx)),
                ),
            );
            for (member_index, member) in group.members.iter().enumerate() {
                if let Some(row) = state.running.iter().find(|row| row.thread == *member) {
                    let thread = row.thread;
                    let drawn = if state.collapsed {
                        nav::running_dot(row)
                    } else {
                        nav::running_row(row).ml(px(10.))
                    };
                    let target = DropTarget::ThreadRow {
                        thread,
                        group: Some(id),
                        index: member_index,
                    };
                    rows = rows.child(
                        drop_feedback(drawn, self.cockpit.groups().clone(), target)
                            .on_drag(
                                Drag::Thread {
                                    thread,
                                    group: Some(id),
                                },
                                move |_, _, _, cx| {
                                    cx.new(|_| NavDragPreview(format!("thread-{thread:02}").into()))
                                },
                            )
                            .on_drop(cx.listener(move |view, drag: &Drag, _, cx| {
                                view.apply_drop(*drag, target, cx)
                            }))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                                    view.focus_thread(thread, cx)
                                }),
                            ),
                    );
                } else if let Some(row) = self.parked_rows.iter().find(|row| row.thread == *member)
                {
                    let thread = row.thread;
                    let drawn = if state.collapsed {
                        nav::parked_dot(row)
                    } else {
                        nav::parked_row(row).ml(px(10.))
                    };
                    let target = DropTarget::ThreadRow {
                        thread,
                        group: Some(id),
                        index: member_index,
                    };
                    rows = rows.child(
                        drop_feedback(drawn, self.cockpit.groups().clone(), target)
                            .on_drag(
                                Drag::Thread {
                                    thread,
                                    group: Some(id),
                                },
                                move |_, _, _, cx| {
                                    cx.new(|_| NavDragPreview(format!("thread-{thread:02}").into()))
                                },
                            )
                            .on_drop(cx.listener(move |view, drag: &Drag, _, cx| {
                                view.apply_drop(*drag, target, cx)
                            }))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                                    view.scope = Scope::Group(id);
                                    view.revive_thread(thread, cx)
                                }),
                            ),
                    );
                }
            }
        }
        let terminal_gap = self.cockpit.groups().iter().count();
        let terminal_target = DropTarget::GroupGap(terminal_gap);
        rows = rows.child(
            drop_feedback(
                div().h(px(crate::theme::POPOVER_PAD)),
                self.cockpit.groups().clone(),
                terminal_target,
            )
            .on_drop(cx.listener(move |view, drag: &Drag, _, cx| {
                view.apply_drop(*drag, terminal_target, cx)
            })),
        );
        let mut loose = div().flex().flex_col().flex_1();
        for row in state
            .running
            .iter()
            .filter(|row| !grouped.contains(&row.thread))
        {
            let thread = row.thread;
            let drawn = if state.collapsed {
                nav::running_dot(row)
            } else {
                nav::running_row(row)
            };
            let target = DropTarget::ThreadRow {
                thread,
                group: None,
                index: 0,
            };
            loose = loose.child(
                drop_feedback(drawn, self.cockpit.groups().clone(), target)
                    .on_drag(
                        Drag::Thread {
                            thread,
                            group: None,
                        },
                        move |_, _, _, cx| {
                            cx.new(|_| NavDragPreview(format!("thread-{thread:02}").into()))
                        },
                    )
                    .on_drop(cx.listener(move |view, drag: &Drag, _, cx| {
                        view.apply_drop(*drag, target, cx)
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                            view.focus_thread(thread, cx)
                        }),
                    ),
            );
        }
        let loose_parked: Vec<_> = self
            .parked_rows
            .iter()
            .filter(|row| !grouped.contains(&row.thread))
            .collect();
        if !loose_parked.is_empty() {
            loose = loose.child(nav::parked_header(loose_parked.len(), state.collapsed));
            for row in loose_parked {
                let thread = row.thread;
                let drawn = if state.collapsed {
                    nav::parked_dot(row)
                } else {
                    nav::parked_row(row)
                };
                let target = DropTarget::ThreadRow {
                    thread,
                    group: None,
                    index: 0,
                };
                loose = loose.child(
                    drop_feedback(drawn, self.cockpit.groups().clone(), target)
                        .on_drag(
                            Drag::Thread {
                                thread,
                                group: None,
                            },
                            move |_, _, _, cx| {
                                cx.new(|_| NavDragPreview(format!("thread-{thread:02}").into()))
                            },
                        )
                        .on_drop(cx.listener(move |view, drag: &Drag, _, cx| {
                            view.apply_drop(*drag, target, cx)
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                                view.revive_thread(thread, cx)
                            }),
                        ),
                );
            }
        }
        rows = rows.child(
            drop_feedback(loose, self.cockpit.groups().clone(), DropTarget::LooseZone).on_drop(
                cx.listener(|view, drag: &Drag, _, cx| {
                    view.apply_drop(*drag, DropTarget::LooseZone, cx)
                }),
            ),
        );
        nav::shell(state.collapsed)
            .child(nav::header(
                state.running.len(),
                state.waiting,
                state.collapsed,
            ))
            .child(rows)
    }

    /// The wall header strip: the workspace's own name left — the swarm is
    /// flying a repo, not the product (#22 C15) — and `N panes · M need
    /// you` right, the amber fragment only when someone actually needs the
    /// operator, exactly as the Cockpit and Wall boards draw it.
    fn strip(&self, attention: usize) -> impl IntoElement {
        let panes = self.panes.len();
        let title = self
            .repo
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "workspace".into());
        let mut strip = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(10.))
            .h(px(crate::theme::STRIP_H))
            .pl(px(crate::theme::STRIP_PAD_L))
            .pr(px(12.))
            .border_b_1()
            .border_color(rgba(crate::theme::HAIRLINE))
            // The strip is the window's visible titlebar now — it drags
            // the window (#22 D24).
            .window_control_area(gpui::WindowControlArea::Drag)
            .child(
                div()
                    .font_family(crate::theme::FONT_UI)
                    .text_size(px(crate::theme::TEXT_CODE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(crate::theme::INK_SECONDARY))
                    .child(SharedString::from(title)),
            )
            .children(match self.scope {
                Scope::Wall => None,
                Scope::Group(group) => self.cockpit.groups().get(group).map(|group| {
                    div()
                        .text_size(px(crate::theme::TEXT_ROW))
                        .text_color(rgb(crate::theme::INK_MUTED))
                        .child(SharedString::from(format!("▸ {}", group.display_title())))
                }),
            })
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(crate::theme::TEXT_ROW))
                    .text_color(rgb(crate::theme::INK_MUTED))
                    .child(SharedString::from(pane_count(panes))),
            );
        if attention > 0 {
            let verb = if attention == 1 { "needs" } else { "need" };
            // The `·` is the seam between the counts, not part of the amber
            // fragment (#22 A5).
            strip = strip
                .child(
                    div()
                        .text_size(px(crate::theme::TEXT_ROW))
                        .text_color(rgb(crate::theme::INK_FAINT))
                        .child("·"),
                )
                .child(
                    div()
                        .text_size(px(crate::theme::TEXT_ROW))
                        .text_color(rgb(crate::theme::WAIT))
                        .child(SharedString::from(format!("{attention} {verb} you"))),
                );
        }
        strip
    }
}

/// The strip's Pane census, in grammatical English — `1 pane`, `24 panes`
/// (#22 A5).
fn pane_count(panes: usize) -> String {
    if panes == 1 {
        "1 pane".into()
    } else {
        format!("{panes} panes")
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

/// Where Ferrite was started: the launch project every draft begins on.
fn here() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| ".".into())
}

/// A typed path with `~` spelled out — the type-a-path row accepts what an
/// operator would type at a shell.
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

/// Revive the newest parked Thread for launch, if the store holds any. An
/// empty store starts as a draft Pane instead (#29): nothing spawns before
/// the operator's choice.
pub fn revive_latest(cockpit: &mut Cockpit) {
    let Some(thread) = cockpit.parked().unwrap_or_default().last().copied() else {
        return;
    };
    if let Err(e) = cockpit.revive(thread) {
        eprintln!("ferrite: thread {thread} could not be revived: {e:?}");
    }
}

/// Fill the cockpit for a multi-pane run (`--panes N`, the perf load):
/// revive the Threads this store already has — newest first, because that
/// is what the operator was last looking at — and open new ones for
/// whatever room is left.
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
    use ferrite_core::transcript::Body;
    use ferrite_core::workspace::WorkspaceBinding;
    use ferrite_core::{Decision, SessionEvent};
    use gpui::{KeyBinding, TestAppContext};

    struct Scripted {
        rx: Receiver<SessionEvent>,
        fail_send: Rc<RefCell<bool>>,
    }

    impl Session for Scripted {
        fn events(&self) -> &Receiver<SessionEvent> {
            &self.rx
        }
        fn send(&mut self, _text: &str) -> std::io::Result<()> {
            if *self.fail_send.borrow() {
                return Err(std::io::Error::other("stub refused first prompt"));
            }
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
        /// Every spawn's choice, in call order — what the provider-picker
        /// tests read back (#25).
        spawned: Rc<RefCell<Vec<ProviderChoice>>>,
        /// While set, spawn refuses — how a test fails a bootstrap (#29).
        fail: Rc<RefCell<bool>>,
        fail_send: Rc<RefCell<bool>>,
    }

    impl Spawner for Fake {
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
            }))
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

    #[gpui::test]
    fn group_scope_is_one_view_and_cmd_w_makes_the_focused_thread_live_and_solo(
        cx: &mut TestAppContext,
    ) {
        let (mut core, _fake) = cockpit("group-scope-close", 2);
        let threads = core.threads();
        core.apply_group(GroupChange::Create {
            seed: threads[0],
            with: Some(threads[1]),
        })
        .unwrap();
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-g", ToggleGroup, None),
                KeyBinding::new("cmd-w", CloseThread, None),
            ])
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));

        cx.simulate_keystrokes("cmd-g");
        view.read_with(cx, |view, _| {
            assert!(matches!(view.scope, Scope::Group(_)));
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
    fn move_to_group_picker_stays_visible_and_operable_at_instrument_level(
        cx: &mut TestAppContext,
    ) {
        let (mut core, _fake) = cockpit("group-picker-instruments", 3);
        let threads = core.threads();
        let group = core
            .apply_group(GroupChange::Create {
                seed: threads[1],
                with: Some(threads[2]),
            })
            .unwrap()
            .group
            .unwrap();
        core.park(threads[1]).unwrap();
        core.park(threads[2]).unwrap();
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-shift-g", MoveToGroup, None),
                KeyBinding::new("up", MenuPrevious, Some("ComposerMenu")),
                KeyBinding::new("enter", MenuPick, Some("ComposerMenu")),
            ]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(560.), px(700.)));
        tick(cx);
        cx.update(|window, cx| {
            assert_eq!(view.read(cx).level_now(window), Level::Instruments);
        });

        cx.simulate_keystrokes("cmd-shift-g");
        tick(cx);
        view.update(cx, |view, cx| {
            assert!(matches!(
                view.picker.as_ref(),
                Some(Picker {
                    kind: PickKind::Group,
                    ..
                })
            ));
            assert!(
                view.composer_menu(0, cx).is_some(),
                "the focused L2 cell has the shared picker popover"
            );
        });

        cx.simulate_keystrokes("up up enter");
        tick(cx);
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.groups().of(threads[0]).unwrap().id, group);
            assert!(view.picker.is_none());
        });
        view.update(cx, |view, cx| {
            view.group_error = Some("old error".into());
            view.apply_drop(Drag::Group(group), DropTarget::LooseZone, cx);
            assert!(
                view.group_error.is_none(),
                "predictable no-drop feedback ends with the drag"
            );
        });
    }

    #[gpui::test]
    fn a_group_scoped_draft_joins_only_after_its_first_send_succeeds(cx: &mut TestAppContext) {
        let (mut core, _fake) = cockpit("group-draft", 2);
        let threads = core.threads();
        core.apply_group(GroupChange::Create {
            seed: threads[0],
            with: Some(threads[1]),
        })
        .unwrap();
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-g", ToggleGroup, None),
                KeyBinding::new("cmd-t", NewThread, None),
                KeyBinding::new("enter", Submit, None),
            ])
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_keystrokes("cmd-g cmd-t");
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

    /// #22 C12: pointing at the context ring opens its hover card; leaving
    /// closes it. The sweep covers the header's right side so the test does
    /// not encode the ring's exact x.
    #[gpui::test]
    fn hovering_the_context_ring_opens_the_token_card(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("usage-hover", 1);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
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
        cx.simulate_resize(gpui::size(px(1440.), px(900.)));
        tick(cx);

        // Sweep the header band right to left until the ring answers.
        let mut hovered = false;
        for at in (0..40).map(|step| 1440. - 20. - step as f32 * 4.) {
            cx.simulate_mouse_move(
                gpui::point(px(at), px(34. + 8. + 14.)),
                None,
                gpui::Modifiers::none(),
            );
            cx.run_until_parked();
            if view.read_with(cx, |view, _| view.hovered_usage == Some(thread)) {
                hovered = true;
                break;
            }
        }
        assert!(hovered, "the sweep never found the ring");

        // Leaving the ring closes the card.
        cx.simulate_mouse_move(
            gpui::point(px(700.), px(450.)),
            None,
            gpui::Modifiers::none(),
        );
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.hovered_usage, None, "the card must close on leave");
        });
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
            let thread = view.panes[0].thread().unwrap();
            assert!(
                view.cockpit.pending(thread).is_some(),
                "the card should be up before the key"
            );
        });

        cx.simulate_keystrokes("y");

        view.read_with(cx, |view, _| {
            let thread = view.panes[0].thread().unwrap();
            assert!(
                view.cockpit.pending(thread).is_none(),
                "y must answer the Decision, not type a letter"
            );
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        fake.streams.borrow()[0].send(decision("perm_01")).unwrap();
        cx.simulate_resize(gpui::size(px(1440.), px(900.)));
        tick(cx);
        cx.simulate_input("not yet");
        view.read_with(cx, |view, cx| {
            assert!(
                view.cockpit.pending(thread).is_some(),
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
                if view.read_with(cx, |view, _| view.cockpit.pending(thread).is_none()) {
                    answered = true;
                    break 'sweep;
                }
            }
        }
        assert!(answered, "the sweep never found a keycap");
        view.read_with(cx, |view, cx| {
            let answered_as = view
                .cockpit
                .transcript(thread)
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
        let closed = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        let (a, b) = created(&view, cx);

        view.update(cx, |view, _| view.focused = view.pane_for(a).unwrap());
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
        let flagged = view.read_with(cx, |view, _| view.panes[7].thread().unwrap());
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.aim_launch(&repo));
        tick(cx);

        cx.simulate_keystrokes("cmd-shift-n");
        view.read_with(cx, |view, _| {
            let draft = view.panes[view.focused].draft().expect("a draft Pane");
            assert_eq!(draft.target, pane::DraftTarget::New, "aimed at a worktree");
            assert!(fake.streams.borrow().is_empty(), "nothing spawned yet");
        });

        cx.simulate_input("set up the branch");
        cx.simulate_keystrokes("enter");

        view.read_with(cx, |view, _| {
            let thread = view.panes[view.focused]
                .thread()
                .expect("the first send made a Thread of the draft");
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
        let flagged = view.read_with(cx, |view, _| view.panes[3].thread().unwrap());

        cx.simulate_keystrokes("a");

        view.read_with(cx, |view, _| {
            assert!(
                view.cockpit.pending(flagged).is_some(),
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.aim_launch(&repo));
        tick(cx);

        cx.simulate_keystrokes("cmd-n");
        cx.simulate_input("hello");
        cx.simulate_keystrokes("enter");

        view.read_with(cx, |view, _| {
            let binding = view
                .cockpit
                .workspace(view.panes[view.focused].thread().unwrap())
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
                view.panes[view.focused].draft().is_some(),
                "an empty store launches as a draft Pane"
            );
        });

        // tab tab: the project chip; ↵ opens its popover (bare enter — no
        // popover is up yet, so Submit routes it to the focused chip).
        cx.simulate_keystrokes("tab");
        cx.simulate_keystrokes("tab");
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            let band = view.band.as_ref().expect("the project popover is open");
            assert_eq!(band.chip, pane::BandChip::Project);
            let labels: Vec<&str> = band.rows.iter().map(|row| row.label.as_ref()).collect();
            assert!(labels.contains(&"repo"), "rows: {labels:?}");
            assert!(labels.contains(&"second"), "rows: {labels:?}");
        });

        // The arrows start on the standing choice (repo); down reaches the
        // project registered after it — "second", the one with a worktree.
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("enter");
        let chosen = view.read_with(cx, |view, _| {
            let draft = view.panes[view.focused].draft().expect("still a draft");
            assert_eq!(
                draft.target,
                pane::DraftTarget::Main,
                "changing the project resets the workspace chip"
            );
            assert_eq!(
                view.cockpit
                    .registry()
                    .project(draft.project)
                    .unwrap()
                    .title,
                "second"
            );
            draft.project
        });

        // tab: the workspace chip; ↵ opens its popover, scoped to the
        // chosen project only — its one worktree between main and new.
        cx.simulate_keystrokes("tab");
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            let band = view.band.as_ref().expect("the workspace popover is open");
            assert_eq!(band.chip, pane::BandChip::Workspace);
            let labels: Vec<&str> = band.rows.iter().map(|row| row.label.as_ref()).collect();
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
        cx.simulate_keystrokes("tab"); // → project
        cx.simulate_keystrokes("enter");
        cx.simulate_keystrokes("up");
        cx.simulate_keystrokes("enter");
        cx.simulate_keystrokes("tab"); // → workspace
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            let band = view.band.as_ref().expect("the workspace popover again");
            assert_eq!(band.chip, pane::BandChip::Workspace);
            let draft = view.panes[view.focused].draft().expect("still a draft");
            assert_ne!(draft.project, chosen, "the arrows flipped the project");
            let labels: Vec<&str> = band.rows.iter().map(|row| row.label.as_ref()).collect();
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.aim_launch(&repo));
        tick(cx);

        cx.simulate_input("run the tests");
        cx.simulate_keystrokes("enter");

        view.read_with(cx, |view, cx| {
            let pane = &view.panes[view.focused];
            let thread = pane.thread().expect("the draft became a Thread");
            assert!(
                view.cockpit.first_prompt_sent(thread),
                "the first send armed the lock"
            );
            assert!(
                pane.composer.read(cx).is_empty(),
                "the sent prompt left the line"
            );
            let blocks = view.cockpit.transcript(thread).unwrap().blocks();
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.aim_launch(&repo));
        tick(cx);
        *fake.fail_send.borrow_mut() = true;

        cx.simulate_input("precious words");
        cx.simulate_keystrokes("enter");

        view.read_with(cx, |view, cx| {
            let pane = &view.panes[view.focused];
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
                view.panes[view.focused].thread().is_some(),
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
            let thread = view.panes[view.focused].thread().expect("locked");
            assert_eq!(
                view.branches.get(&thread).map(|branch| branch.to_string()),
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
                view.branches.get(&thread).map(|branch| branch.to_string()),
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
        view.read_with(cx, |view, _| {
            let thread = view.panes[0].thread().unwrap();
            let id = view
                .cockpit
                .transcript(thread)
                .expect("a transcript")
                .blocks()[block]
                .id;
            view.selection
                .caret_position(thread, id, 0, byte)
                .expect("a rendered caret")
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

    /// #27: double-click takes the word under the pointer, and dragging on
    /// extends word-wise; triple-click takes the whole rendered run.
    #[gpui::test]
    fn double_click_selects_the_word_and_triple_click_the_line(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("select-clicks", 1);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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

        // Held down, the drag extends word-wise: mid "honest" sweeps both
        // whole words and everything between.
        cx.simulate_click(on_make, gpui::Modifiers::none());
        press(cx, on_make, 2);
        let on_honest = caret(&view, cx, 1, 10);
        cx.simulate_mouse_move(on_honest, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_mouse_up(on_honest, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(
            clipboard(cx).as_deref(),
            Some("make it fast\nkeep it honest")
        );

        // Triple-click takes the rendered run whole.
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
            Some("before\nBash(echo hi)\n⎿ done\nafter"),
            "the exit-0 chip and the ⏺ are chrome and must not copy"
        );
    }

    /// #15 review, at the rendered window (#27): streaming slides the
    /// window of Blocks a Pane draws, shifting every rendered position — a
    /// selection stored as positions would quietly slide onto rows the
    /// operator never touched. Ids pin it; an endpoint that leaves the
    /// window clamps the copy to the window start; with both ends gone the
    /// selection dies instead of resurrecting elsewhere.
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

        // 203 total: the window is Blocks 3.., every rendered position
        // shifts by three.
        say(60, 203);
        tick(cx);
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(
            clipboard(cx).as_deref(),
            Some(texts.join("\n").as_str()),
            "the window slid positions; the selection must not slide"
        );

        // 206 total: the anchor (Block 5) left the window, the head (7)
        // lives — the selection clamps to the window start, now Block 6.
        say(203, 206);
        tick(cx);
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(
            clipboard(cx).as_deref(),
            Some(texts[1..].join("\n").as_str()),
            "an evicted anchor clamps to the surviving remainder"
        );

        // 208 total: both endpoints gone — the selection dies, and the
        // clipboard is left alone.
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("kept".into())));
        say(206, 208);
        tick(cx);
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(
            clipboard(cx).as_deref(),
            Some("kept"),
            "a fully evicted selection must not resurrect on other Blocks"
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
        cx.run_until_parked();
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
                Some(PaneIdentity::Thread(view.panes[0].thread().unwrap())),
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

    #[gpui::test]
    fn cmd_f_reaches_a_draft_from_a_dense_grid(cx: &mut TestAppContext) {
        let (mut core, _fake) = cockpit("draft-fullscreen", 5);
        for thread in core.threads() {
            core.park(thread).unwrap();
        }
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("cmd-f", ToggleFullscreen, None)]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, cx| {
            for _ in 0..5 {
                view.open_draft(pane::DraftTarget::Main, cx);
            }
        });
        cx.simulate_resize(gpui::size(px(500.), px(320.)));
        tick(cx);

        cx.simulate_keystrokes("cmd-f");
        cx.simulate_input("reachable");

        view.read_with(cx, |view, cx| {
            assert!(matches!(view.fullscreen, Some(PaneIdentity::Draft(_))));
            assert_eq!(
                view.panes[view.focused].composer.read(cx).text(),
                "reachable"
            );
        });
    }

    #[gpui::test]
    fn launch_provider_seeds_the_first_empty_store_draft(cx: &mut TestAppContext) {
        let fake = Fake::default();
        let store = Store::open(scratch("launch-provider-draft")).unwrap();
        let core = Cockpit::new(store, Box::new(fake));
        let (view, cx) =
            cx.add_window_view(|_, cx| CockpitView::new_with_provider(core, Provider::Codex, cx));

        view.read_with(cx, |view, _| {
            assert_eq!(
                view.panes[0].draft().unwrap().provider,
                ProviderChoice {
                    provider: Provider::Codex,
                    model: None,
                }
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
            assert_eq!(
                view.fullscreen,
                Some(PaneIdentity::Thread(view.panes[0].thread().unwrap()))
            );
        });

        cx.simulate_keystrokes("cmd-]");

        view.read_with(cx, |view, _| {
            assert_eq!(view.focused, 1, "cmd-] still walks the Threads");
            assert_eq!(
                view.fullscreen,
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        cx.simulate_keystrokes("cmd-f");
        let closed = view.read_with(cx, |view, _| {
            assert!(view.fullscreen.is_some(), "the premise: fullscreen is on");
            view.panes[0].thread().unwrap()
        });

        cx.simulate_keystrokes("cmd-w");

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1, "the Pane is gone");
            assert_eq!(
                view.fullscreen,
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
            view.panes[0].thread().unwrap()
        });

        // Park it the way code that never heard of fullscreen would.
        view.update(cx, |view, cx| {
            view.cockpit.park(gone).unwrap();
            view.panes.retain(|pane| pane.thread() != Some(gone));
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
                    .filter_map(|pane| pane.thread())
                    .collect::<Vec<_>>(),
                view.panes[1].thread().unwrap(),
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

        // The second running row: 34px strip above the nav, 34px nav
        // header, 28px rows.
        cx.simulate_click(
            gpui::point(px(104.), px(34. + 34. + 28. + 14.)),
            gpui::Modifiers::none(),
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.focused, 1, "the click moved focus to the row's Pane");
        });

        cx.simulate_keystrokes("cmd-f");
        view.read_with(cx, |view, _| {
            assert_eq!(
                view.fullscreen,
                Some(PaneIdentity::Thread(view.panes[1].thread().unwrap()))
            );
        });
        cx.simulate_click(
            gpui::point(px(104.), px(34. + 34. + 14.)),
            gpui::Modifiers::none(),
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.focused, 0, "the nav still answers while fullscreen");
            assert_eq!(
                view.fullscreen,
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        tick(cx);
        let parked = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        cx.simulate_keystrokes("cmd-w");
        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 1);
            assert_eq!(view.parked_rows.len(), 1, "the parked Thread got a row");
        });

        // The parked row: 34px strip + 34px nav header + one running row +
        // the 22px PARKED divider, then its own 28px row.
        cx.simulate_click(
            gpui::point(px(104.), px(34. + 34. + 28. + 22. + 14.)),
            gpui::Modifiers::none(),
        );

        view.read_with(cx, |view, _| {
            assert_eq!(view.panes.len(), 2, "the revived Thread got a Pane");
            assert_eq!(
                view.panes[1].thread().unwrap(),
                parked,
                "and it is the same Thread"
            );
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
        bind_production_keys(cx);
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
            // Everything the Session listed — plus, on this still-fresh
            // Thread, Ferrite's own provider (#25) and import (#11)
            // entries on top.
            assert_eq!(menu.rows.len(), 6);
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
        bind_production_keys(cx);
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
        bind_production_keys(cx);
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.aim_launch(&project));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("@");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let names: Vec<&str> = view
                .menu
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
            let entry = view.panes[view.focused].draft().unwrap().project;
            let worktree = view.cockpit.registry().worktrees(entry)[0].clone();
            std::fs::write(worktree.path.join("worktree-only.txt"), "tree\n").unwrap();
            view.panes[view.focused].draft_mut().unwrap().target = pane::DraftTarget::Existing {
                branch: SharedString::from(worktree.branch),
            };
        });
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("@");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let names: Vec<&str> = view
                .menu
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .map(|r| r.name.as_ref())
                .collect();
            assert!(names.contains(&"worktree-only.txt"), "rows: {names:?}");
        });

        view.update(cx, |view, cx| {
            view.panes[view.focused]
                .composer
                .update(cx, |composer, cx| composer.set(String::new(), cx));
            view.panes[view.focused].draft_mut().unwrap().target = pane::DraftTarget::New;
        });
        cx.simulate_input("@");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let names: Vec<&str> = view
                .menu
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .map(|r| r.name.as_ref())
                .collect();
            assert!(names.contains(&"main-only.txt"), "rows: {names:?}");
            assert!(!names.contains(&"worktree-only.txt"), "rows: {names:?}");
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
        bind_production_keys(cx);
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
        bind_production_keys(cx);
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
        bind_production_keys(cx);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
        bind_production_keys(cx);
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
        });
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
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
        bind_production_keys(cx);
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        // No provider menu yet: the local entries are the whole list —
        // #25's provider door on top, then the import door.
        cx.simulate_input("/");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view
                .menu
                .as_ref()
                .expect("/ offers import on a fresh Thread");
            let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(names, ["/provider", "/import"]);
            assert_eq!(menu.rows[0].detail.as_ref(), "switch provider / model");
            assert_eq!(menu.rows[1].detail.as_ref(), "adopt a CLI session file");
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
            let menu = view.menu.as_ref().expect("open");
            let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(names, ["/code-review", "/commit", "/compact"]);
        });
        cx.simulate_keystrokes("backspace backspace");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.menu.as_ref().expect("open");
            let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
            assert_eq!(
                names,
                [
                    "/provider",
                    "/import",
                    "/code-review",
                    "/commit",
                    "/compact",
                    "/to-tickets"
                ],
                "the local entries ride atop the provider's own menu"
            );
        });

        // A conversation starts: the import door closes; the provider row
        // stays visible but inert, saying why it no longer opens (#25).
        cx.simulate_keystrokes("backspace");
        cx.simulate_input("hello");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        cx.simulate_input("/");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.menu.as_ref().expect("the provider menu still lists");
            assert!(
                menu.rows.iter().all(|row| row.name.as_ref() != "/import"),
                "a Thread with history offers no import"
            );
            assert_eq!(menu.rows[0].name.as_ref(), "/provider");
            assert!(menu.rows[0].inert, "the locked door is an explanation");
            assert_eq!(menu.rows[0].detail.as_ref(), "locked after first prompt");
            assert_eq!(menu.rows.len(), 5);
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
            let picker = view.picker.as_ref().expect("the file picker is open");
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
                &picker.rows[0].choice,
                Choice::Adopt(path) if path.ends_with("bbbb.jsonl")
            ));
            assert_eq!(picker.selected, 0);
            // Nothing reached the provider: no prompt, no running turn.
            let transcript = view.cockpit.transcript(thread).unwrap();
            assert!(
                !transcript
                    .blocks()
                    .iter()
                    .any(|block| matches!(block.body, Body::Prompt(_))),
                "picking import must not prompt the provider"
            );
            assert!(!view.cockpit.busy(thread));
        });

        // The arrows walk the rows; escape dismisses with the keyboard
        // still in the Composer.
        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.picker.as_ref().expect("open").selected, 1);
        });
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.picker.is_none(), "escape dismissed the picker");
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
            assert!(view.picker.is_none(), "the pick closed the picker");
            assert_eq!(view.panes.len(), 1, "one Pane: the adopted Thread");
            let adopted = view.panes[0].thread().unwrap();
            assert_ne!(adopted, blank);
            assert_eq!(
                view.focused_thread(),
                Some(adopted),
                "the adopted Thread takes focus"
            );
            let transcript = view.cockpit.transcript(adopted).unwrap();
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
            assert!(view.cockpit.transcript(blank).is_none());
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
            assert!(view.picker.is_none(), "the refusal closed the picker");
            assert_eq!(view.panes.len(), 1);
            assert_eq!(view.panes[0].thread().unwrap(), thread, "the Thread stays");
            let transcript = view.cockpit.transcript(thread).unwrap();
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
            let menu = view.menu.as_ref().expect("the menu reopens");
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.session_file_roots = session_roots(&base));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

        cx.simulate_input("/im");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(view.picker.is_none(), "nothing to pick from");
            let transcript = view.cockpit.transcript(thread).unwrap();
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.session_file_roots = roots);
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("/im");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let menu = view.menu.as_ref().expect("draft slash menu");
            assert_eq!(menu.rows.len(), 1);
            assert_eq!(menu.rows[0].insert.as_ref(), "import");
        });
        view.update(cx, |view, cx| {
            assert!(
                view.draft_popover_element(0, cx).is_some(),
                "the derived Draft menu reaches its one rendered popover slot"
            );
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.picker.is_some()));
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        view.read_with(cx, |view, cx| {
            assert!(view.panes[0].draft().is_none());
            assert!(view.panes[0].thread().is_some());
            assert_eq!(view.panes[0].composer.read(cx).text(), "");
            assert_eq!(view.focused, 0);
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        view.update(cx, |view, _| view.session_file_roots = roots);
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);

        cx.simulate_input("/im");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert!(view.picker.is_some()));

        // The middle of the Pane's transcript — nowhere near the popover.
        cx.simulate_mouse_down(
            gpui::point(px(600.), px(200.)),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(view.picker.is_none(), "the press dismissed it");
        });
    }

    /// Discovery is a bounded, ordered walk: both roots, `.jsonl` only,
    /// newest first, capped — and a missing root lists nothing rather than
    /// erroring.
    #[test]
    fn session_file_discovery_walks_both_roots_newest_first_and_capped() {
        let base = scratch("import-discovery");
        let roots = session_roots(&base);
        write_session_file(
            &roots[0].1.join("-workspace-alpha").join("old.jsonl"),
            "x\n",
            3600,
        );
        write_session_file(
            &roots[0].1.join("-workspace-beta").join("new.jsonl"),
            "x\n",
            10,
        );
        write_session_file(
            &roots[1].1.join("2026").join("08").join("rollout-mid.jsonl"),
            "x\n",
            600,
        );
        // Not a session file shape: ignored by extension.
        write_session_file(
            &roots[0].1.join("-workspace-alpha").join("notes.txt"),
            "x\n",
            5,
        );

        let all = session_file_candidates(&roots, 8);
        let names: Vec<String> = all
            .iter()
            .map(|candidate| {
                candidate
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, ["new.jsonl", "rollout-mid.jsonl", "old.jsonl"]);
        assert_eq!(
            all.iter()
                .map(|candidate| candidate.provider)
                .collect::<Vec<_>>(),
            [Provider::Claude, Provider::Codex, Provider::Claude]
        );

        let capped = session_file_candidates(&roots, 2);
        assert_eq!(capped.len(), 2, "the cap holds");
        assert_eq!(
            capped[0].path.file_name().unwrap().to_string_lossy(),
            "new.jsonl"
        );

        let missing = session_roots(&base.join("nowhere"));
        assert!(session_file_candidates(&missing, 8).is_empty());
    }

    // ------------------------------------------------- Provider choice (#25)

    /// #25 AC: the keyboard-only path. `/` lists the local provider row on
    /// top; ↵ opens the picker — the two Providers with the ✓ on the
    /// current one, and no invented model rows before an announcement —
    /// and ↓↵ picks codex: Ferrite's own act, the Session replaced on the
    /// spot, nothing landing as prompt text.
    #[gpui::test]
    fn the_slash_provider_row_opens_the_picker_and_picks_codex(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("provider-pick", 1);
        bind_production_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
        view.read_with(cx, |view, _| {
            let picker = view.picker.as_ref().expect("the provider picker is open");
            assert!(matches!(picker.kind, PickKind::Provider));
            let names: Vec<&str> = picker
                .rows
                .iter()
                .map(|pick| pick.row.name.as_ref())
                .collect();
            assert_eq!(names, ["claude", "codex"], "no model row is invented");
            assert!(picker.rows[0].active, "✓ on the current provider");
            assert!(!picker.rows[1].active);
            assert_eq!(picker.selected, 0, "the arrows start on it");
        });

        cx.simulate_keystrokes("down enter");
        cx.run_until_parked();

        view.read_with(cx, |view, _| {
            assert!(view.picker.is_none(), "the pick closed the picker");
            assert_eq!(view.cockpit.provider(thread), Some(Provider::Codex));
            // The switch was Ferrite's own act: no prompt, no running turn.
            let transcript = view.cockpit.transcript(thread).unwrap();
            assert!(transcript.blocks().is_empty());
            assert!(!view.cockpit.busy(thread));
        });
        assert_eq!(
            fake.spawned.borrow().last().unwrap(),
            &ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
            "the choice drives the spawn"
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
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
            let picker = view.picker.as_ref().expect("open");
            let names: Vec<&str> = picker
                .rows
                .iter()
                .map(|pick| pick.row.name.as_ref())
                .collect();
            assert_eq!(names, ["claude", "codex", "sonnet", "opus"]);
        });

        cx.simulate_keystrokes("down down down enter");
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
            assert_eq!(view.cockpit.model(thread), Some("opus"));
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
            let picker = view.picker.as_ref().expect("reopened");
            assert!(picker.rows[0].active, "✓ on the current provider");
            assert!(picker.rows[3].active, "✓ on the standing model choice");
            assert!(!picker.rows[2].active);
        });
    }

    #[gpui::test]
    fn a_draft_inherits_the_focused_provider_model_and_only_live_models(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("draft-provider-choice", 2);
        bind_production_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
            view.open_draft(pane::DraftTarget::Main, cx);
            view.open_band_popover(pane::BandChip::Provider, cx);
        });

        view.read_with(cx, |view, _| {
            let draft = view.panes[view.focused].draft().unwrap();
            assert_eq!(draft.provider.model.as_deref(), Some("opus"));
            let labels: Vec<&str> = view
                .band
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .map(|row| row.label.as_ref())
                .collect();
            assert_eq!(labels, ["claude", "codex", "sonnet", "opus"]);
            let rows = &view.band.as_ref().unwrap().rows;
            assert_eq!(rows[2].detail.as_ref(), "claude model");
            assert_eq!(rows[3].detail.as_ref(), "claude model");
            assert_eq!(view.band.as_ref().unwrap().selected, 3);
        });
        view.update(cx, |view, cx| {
            let selected = view.band.as_ref().unwrap().selected;
            view.pick_band(selected, cx);
            assert_eq!(
                view.panes[view.focused]
                    .draft()
                    .unwrap()
                    .provider
                    .model
                    .as_deref(),
                Some("opus")
            );
        });
    }

    /// #25: the first prompt locks the door. The picker refuses to open,
    /// and the footer control gives way to the plain label — the chip is
    /// simply not assembled any more.
    #[gpui::test]
    fn the_first_prompt_retires_the_picker_and_the_footer_chip(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("provider-lock-ui", 1);
        bind_production_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        tick(cx);
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        view.update(cx, |view, cx| {
            assert!(
                view.provider_chip(0, cx).is_some(),
                "pre-lock the footer offers the control"
            );
        });

        cx.simulate_input("hello");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        view.update(cx, |view, cx| {
            view.open_provider_picker(thread, cx);
            assert!(view.picker.is_none(), "the picker refuses to open");
            assert!(
                view.provider_chip(0, cx).is_none(),
                "the control reverts to the plain label"
            );
        });

        // The inert `/provider` row's pick dismisses and nothing else.
        cx.simulate_input("/");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.menu.as_ref().expect("open").rows[0].inert);
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.menu.is_none(), "the pick dismissed the menu");
            assert!(view.picker.is_none(), "and opened nothing");
        });
        assert_eq!(composer_text(&view, cx), "/", "the line is left alone");
    }

    /// #25 regression: reopening the picker with a standing model choice
    /// and pressing bare ↵ changes nothing — the current provider's row
    /// carries the choice, so the re-pick is a true no-op: no teardown, no
    /// respawn, the model kept.
    #[gpui::test]
    fn reopening_the_picker_and_pressing_enter_keeps_the_standing_choice(cx: &mut TestAppContext) {
        let (core, fake) = cockpit("provider-reopen-noop", 1);
        bind_production_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
        cx.simulate_keystrokes("down down down enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.model(thread), Some("opus"));
        });
        let spawns = fake.streams.borrow().len();

        // Reopen; the arrows start on the current provider's row, whose
        // choice carries the standing model — bare ↵ re-picks it whole.
        cx.simulate_input("/");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.picker.as_ref().expect("open").selected, 0);
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        assert_eq!(
            fake.streams.borrow().len(),
            spawns,
            "a re-pick of the standing choice must not respawn"
        );
        view.read_with(cx, |view, _| {
            assert_eq!(view.cockpit.model(thread), Some("opus"), "the model stands");
            assert_eq!(view.cockpit.provider(thread), Some(Provider::Claude));
        });
    }

    /// #25: the mouse door — a click on the footer chip opens the picker.
    /// The sweep covers the meta row's right side so the test does not
    /// encode the chip's exact position.
    #[gpui::test]
    fn clicking_the_footer_chip_opens_the_provider_picker(cx: &mut TestAppContext) {
        let (core, _fake) = cockpit("provider-chip-click", 1);
        bind_production_keys(cx);
        let (view, cx) = cx.add_window_view(|_, cx| CockpitView::new(core, cx));
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
                if view.read_with(cx, |view, _| view.picker.is_some()) {
                    opened = true;
                    break 'sweep;
                }
            }
        }
        assert!(opened, "the sweep never found the chip");
        view.read_with(cx, |view, _| {
            let picker = view.picker.as_ref().expect("open");
            assert!(matches!(picker.kind, PickKind::Provider));
        });
    }

    /// The strip counts in grammatical English — never "1 panes" (#22 A5).
    #[test]
    fn the_strip_census_is_singular_for_one_pane() {
        assert_eq!(pane_count(1), "1 pane");
        assert_eq!(pane_count(24), "24 panes");
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
}
