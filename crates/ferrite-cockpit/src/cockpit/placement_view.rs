//! The setup sheet (ADR 0048) as a view of its own (ADR 0041, 0042): a
//! Project of this computer copied to a Machine for a draft picking that
//! environment. It owns where the copy goes, a folder browsed on the
//! Machine and the name of the copy in it, the entries git ignores and
//! which of them go too, and the copy, which runs on a thread of its own
//! over its own connection (`Cockpit::tree`). A copy stopped part way is
//! unfinished (`placement::unfinished`): the sheet opens on it, and only
//! its Resume goes there. Once the folder is there it says so as a
//! `PlacementEvent`, and the Cockpit registers it and chooses it on the
//! draft.

use super::draft_environment::Band;
use super::folder_browser::{FolderBrowser, Moved};
use super::*;
use ferrite_core::chunked::{Cancellation, Progress};
use ferrite_core::placement::{self, Copied, Copying, Ignored};
use ferrite_core::tree::{self, Refusal, Sent, Side, Stopped};
use gpui::component::{checkbox::Checkbox, Disableable};
use gpui::App;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};

mod ignored;
use ignored::Entry;

/// What the sheet copies, and for whom.
#[derive(Clone)]
pub(crate) struct Placing {
    /// The draft that picked the Machine: the copy is chosen on it.
    pub(crate) draft: PaneIdentity,
    pub(crate) title: SharedString,
    pub(crate) root: PathBuf,
    pub(crate) machine: ferrite_core::MachineId,
    pub(crate) machine_name: SharedString,
    /// The Machine's projects folder, where the browser opens.
    pub(crate) projects_dir: String,
    /// The Machine's own folder on this computer (`machine::dir`), which
    /// keeps where its copies went and those unfinished.
    pub(crate) kept: PathBuf,
}

/// What the sheet asks of the window.
pub(crate) enum PlacementEvent {
    /// The folder is at `dest` on the Machine: register it there.
    Place {
        dest: PathBuf,
    },
    Closed,
}

impl EventEmitter<PlacementEvent> for PlacementView {}

/// Where the sheet is. One at a time, so no two can disagree.
enum Stage {
    Choosing,
    /// The Project's files being listed, this many so far, and the
    /// destination looked at, before anything is sent there.
    Listing(usize),
    /// `dest` holds what no copy of this Project put there: it takes none.
    /// `of`, when it is another Project's unfinished copy: that Project's
    /// folder name.
    Occupied {
        dest: String,
        of: Option<String>,
    },
    Copying(Progress),
    /// The operator stopped the copy. What it sent stays, for its Resume.
    Cancelled,
    /// Copied to `dest`; the window is registering it. What could not be
    /// read, and what was changed there, stays as it was and is listed
    /// once it is in, each with why.
    Registering {
        dest: String,
        skipped: Vec<String>,
    },
    Done {
        skipped: Vec<String>,
    },
    Failed {
        reason: SharedString,
    },
}

/// What the thread reading the Project hands back.
enum Listed {
    /// Where its unfinished copies go on the Machine, the last begun last.
    Unfinished(Vec<PathBuf>),
    /// Why it cannot be copied, what git ignores in it, and the worktrees
    /// in it, which stay here.
    Read {
        unfit: Option<String>,
        ignored: Vec<Ignored>,
        worktrees: Vec<String>,
    },
    Size(usize, u64),
}

/// What the copy's thread hands back.
enum Heard {
    /// No connection to the Machine could be made.
    Unreached(io::Error),
    Copying(Copying),
    /// How the copy ended, and where the Project's unfinished copies go
    /// now.
    Copied {
        copied: Result<Copied, Stopped>,
        unfinished: Vec<PathBuf>,
    },
}

pub(crate) struct PlacementView {
    model: Entity<Cockpit>,
    pub(crate) placing: Placing,
    /// The folder on the Machine the copy goes into.
    pub(super) dest: Entity<FolderBrowser>,
    /// The copy's own folder in it: the Project's folder name until edited.
    pub(super) name: Entity<Composer>,
    /// The card's own, which holds the keyboard while where the copy goes
    /// is held: keys reach nothing under the sheet.
    focus: FocusHandle,
    entries: Option<Vec<Entry>>,
    /// Why the Project cannot be copied at all.
    unfit: Option<SharedString>,
    /// The worktrees in the Project, which stay here.
    worktrees: Vec<String>,
    listing: Option<Receiver<Listed>>,
    /// Where the Project's unfinished copies go on the Machine, as each
    /// copy spelled it there.
    unfinished: Vec<PathBuf>,
    /// The unfinished copy the sheet opens on, once the browser can show
    /// it under `~/`.
    opening: Option<PathBuf>,
    stage: Stage,
    /// Why the destination takes no copy, once Copy was pressed.
    refusal: Option<SharedString>,
    copy: Option<Running>,
    /// The copy under way, or the last, resumes an unfinished one.
    resuming: bool,
    scroll: ScrollHandle,
    /// The draft's band, which the card keeps clear of.
    band: Option<Band>,
}

/// A copy under way: to `dest` as it was chosen when it began, `target`
/// as the Machine spells it.
struct Running {
    dest: String,
    target: PathBuf,
    hear: Receiver<Heard>,
    cancel: Cancellation,
}

impl PlacementView {
    pub(crate) fn new(
        model: Entity<Cockpit>,
        placing: Placing,
        band: Option<Band>,
        cx: &mut Context<Self>,
    ) -> Self {
        let dest = cx.new(|cx| {
            FolderBrowser::new(
                model.clone(),
                placing.machine,
                format!("Folder on {}", placing.machine_name).into(),
                format!("{}/", placing.projects_dir.trim_end_matches('/')),
                cx,
            )
            .making("The copy makes this folder")
        });
        dest.update(cx, |dest, cx| dest.ask_home(cx));
        cx.subscribe(&dest, |sheet: &mut Self, _, _: &Moved, cx| sheet.edited(cx))
            .detach();
        let name = cx.new(Composer::new);
        name.update(cx, |name, cx| {
            name.set_role(ferrite_pane::composer::Role::Field(None), cx);
            name.set(leaf(&placing), cx);
        });
        cx.subscribe(&name, |sheet: &mut Self, _, _: &Edited, cx| {
            sheet.edited(cx)
        })
        .detach();
        let (found, listing) = ferrite_core::doorbell::channel();
        let (root, kept) = (placing.root.clone(), placing.kept.clone());
        std::thread::spawn(move || {
            if found
                .send(Listed::Unfinished(placement::unfinished(&kept, &root)))
                .is_err()
            {
                return;
            }
            let (ignored, worktrees) =
                placement::without_worktrees(placement::ignored(&root), &placement::worktrees(&root));
            let paths: Vec<String> = ignored.iter().map(|entry| entry.path.clone()).collect();
            let read = Listed::Read {
                unfit: placement::refuse_source(&root),
                ignored,
                worktrees,
            };
            if found.send(read).is_err() {
                return;
            }
            for (index, path) in paths.iter().enumerate() {
                if found
                    .send(Listed::Size(index, placement::size(&root, path)))
                    .is_err()
                {
                    return;
                }
            }
        });
        Self {
            model,
            placing,
            dest,
            name,
            focus: cx.focus_handle(),
            entries: None,
            unfit: None,
            worktrees: Vec::new(),
            listing: Some(listing),
            unfinished: Vec::new(),
            opening: None,
            stage: Stage::Choosing,
            refusal: None,
            copy: None,
            resuming: false,
            scroll: ScrollHandle::new(),
            band,
        }
    }

    /// Where the keyboard goes while the sheet is up: the browser's line,
    /// or the card itself while where the copy goes is held.
    pub(super) fn focus(&self, cx: &App) -> FocusHandle {
        if self.held() {
            self.focus.clone()
        } else {
            self.dest.read(cx).focus_handle(cx)
        }
    }

    /// Whether the sheet holds the keyboard: the browser's, the copy's
    /// name, or the card.
    pub(super) fn holds_focus(&self, window: &Window, cx: &App) -> bool {
        let field = self.dest.read(cx).focused(window, cx)
            || self.name.read(cx).focus_handle(cx).is_focused(window);
        (field && !self.held()) || self.focus.contains_focused(window, cx)
    }

    fn copy_name<'a>(&self, cx: &'a App) -> &'a str {
        self.name.read(cx).text().trim()
    }

    /// The folder the browser shows, `/` last: what the copy's name goes
    /// after.
    fn folder_prefix(&self, cx: &App) -> String {
        let folder = self.dest.read(cx).folder(cx);
        if folder.ends_with(['/', '\\']) {
            folder
        } else {
            format!("{folder}/")
        }
    }

    /// Where the copy goes: its name in the folder browsed.
    pub(super) fn destination(&self, cx: &App) -> String {
        format!("{}{}", self.folder_prefix(cx), self.copy_name(cx))
    }

    /// Where the copy goes, as the Machine spells it.
    fn target(&self, cx: &App) -> PathBuf {
        self.dest.read(cx).spelled_out(&self.destination(cx))
    }

    /// Whether the copy goes where an unfinished copy of the Project is:
    /// it resumes that one.
    fn resumes(&self, cx: &App) -> bool {
        self.unfinished.contains(&self.target(cx))
    }

    /// Where the copy goes moved: what it was found to hold no longer
    /// counts.
    fn edited(&mut self, cx: &mut Context<Self>) {
        self.refusal = None;
        if matches!(
            self.stage,
            Stage::Occupied { .. } | Stage::Failed { .. } | Stage::Cancelled
        ) {
            self.enter(Stage::Choosing, cx);
        }
        cx.notify();
    }

    /// The sheet opens on the copy of the Project begun last and left
    /// unfinished, under `~/` as the browser shows paths, unless the
    /// operator has aimed it already.
    fn open_on_unfinished(&mut self, cx: &mut Context<Self>) {
        let Some(last) = &self.opening else {
            return;
        };
        if !self.dest.read(cx).untouched() || self.copy_name(cx) != leaf(&self.placing) {
            self.opening = None;
            return;
        }
        let Some(last) = self.dest.read(cx).tilde(last) else {
            return;
        };
        self.opening = None;
        let Some((folder, name)) = last.rsplit_once(['/', '\\']) else {
            return;
        };
        let (folder, name) = (format!("{folder}/"), name.to_string());
        self.dest.update(cx, |dest, cx| dest.open_at(folder, cx));
        self.name.update(cx, |field, cx| field.set(name, cx));
    }

    /// While the copy is under way or being registered.
    pub(super) fn busy(&self) -> bool {
        matches!(
            self.stage,
            Stage::Listing(_) | Stage::Copying(_) | Stage::Registering { .. }
        )
    }

    /// From Copy on, where the copy goes is held: the browser, the copy's
    /// name and the ticks take nothing, unless it stops short (`Occupied`,
    /// `Cancelled`, `Failed`).
    fn held(&self) -> bool {
        self.busy() || matches!(self.stage, Stage::Done { .. })
    }

    /// The sheet moves to `stage`, the browser held or let go with it.
    fn enter(&mut self, stage: Stage, cx: &mut Context<Self>) {
        self.stage = stage;
        let held = self.held();
        self.dest.update(cx, |dest, cx| dest.hold(held, cx));
        cx.notify();
    }

    /// Copy to the destination chosen, unless the Project cannot be
    /// copied or the destination is no folder to copy into. The copy finds
    /// for itself whether it resumes one, or meets what it did not put
    /// there.
    fn start(&mut self, cx: &mut Context<Self>) {
        if self.busy() || self.entries.is_none() {
            return;
        }
        let dest = self.destination(cx);
        self.refusal = self.unfit.clone().or_else(|| {
            if self.copy_name(cx).is_empty() {
                Some("Name the copy's folder".into())
            } else {
                placement::refuse_destination(&dest).map(SharedString::from)
            }
        });
        if self.refusal.is_some() {
            cx.notify();
            return;
        }
        let machine = self.placing.machine;
        let connect = self.model.update(cx, |cockpit, _| cockpit.tree(machine));
        let skip: Vec<String> = self
            .entries
            .iter()
            .flatten()
            .filter(|entry| !entry.ticked)
            .map(|entry| entry.ignored.path.clone())
            .chain(self.worktrees.iter().cloned())
            .collect();
        let target = self.target(cx);
        let (source, kept, to) = (self.placing.root.clone(), self.placing.kept.clone(), target.clone());
        let (heard, hear) = ferrite_core::doorbell::channel();
        let cancel = Cancellation::default();
        let cancelled = cancel.clone();
        std::thread::spawn(move || {
            let mut exchange = match connect.and_then(|connect| connect()) {
                Ok(exchange) => exchange,
                Err(error) => {
                    let _ = heard.send(Heard::Unreached(error));
                    return;
                }
            };
            let job = placement::CopyJob {
                source: &source,
                dest: &to,
                skip: &skip,
                kept: &kept,
            };
            let mut progress = |copying| {
                let _ = heard.send(Heard::Copying(copying));
            };
            let copied = placement::copy(&job, &mut *exchange, &mut progress, &cancelled);
            let unfinished = placement::unfinished(&kept, &source);
            let _ = heard.send(Heard::Copied { copied, unfinished });
        });
        self.resuming = self.resumes(cx);
        self.copy = Some(Running {
            dest,
            target,
            hear,
            cancel,
        });
        self.enter(Stage::Listing(0), cx);
    }

    /// Register what is at the destination as it is, copying nothing.
    fn use_what_is_there(&mut self, cx: &mut Context<Self>) {
        let Stage::Occupied { dest, of: None } = &self.stage else {
            return;
        };
        let dest = dest.clone();
        let spelled = self.dest.read(cx).spelled_out(&dest);
        self.enter(
            Stage::Registering {
                dest,
                skipped: Vec::new(),
            },
            cx,
        );
        cx.emit(PlacementEvent::Place { dest: spelled });
    }

    /// Stop the copy under way; what it sent stays, for its Resume.
    pub(super) fn stop(&mut self) {
        if let Some(copy) = self.copy.take() {
            copy.cancel.cancel();
        }
    }

    /// The close button, Cancel and esc: a copy under way is cancelled,
    /// the sheet staying up to say so; else the sheet closes.
    pub(super) fn cancel(&mut self, cx: &mut Context<Self>) {
        if matches!(self.stage, Stage::Listing(_) | Stage::Copying(_)) {
            self.stop();
            // The copy may have made its folder by now.
            self.dest.update(cx, |dest, cx| dest.relist(cx));
            self.enter(Stage::Cancelled, cx);
        } else {
            cx.emit(PlacementEvent::Closed);
        }
    }

    /// ↵: the footer's own verb, where it has one.
    pub(super) fn confirm(&mut self, cx: &mut Context<Self>) {
        match self.stage {
            Stage::Choosing | Stage::Cancelled | Stage::Failed { .. } => self.start(cx),
            Stage::Done { .. } => cx.emit(PlacementEvent::Closed),
            _ => {}
        }
    }

    /// Tab in the browser: go into the first folder its last name starts.
    /// False when Tab moves on.
    pub(super) fn tab(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        self.dest.update(cx, |dest, cx| dest.tab(window, cx))
    }

    pub(super) fn failed(&mut self, reason: String, cx: &mut Context<Self>) {
        self.copy = None;
        self.enter(
            Stage::Failed {
                reason: reason.into(),
            },
            cx,
        );
    }

    /// What the operator does about a copy that could not go on.
    fn next_step(&self, cx: &App) -> &'static str {
        if self.resumes(cx) {
            "Resume finishes the copy once it is back."
        } else {
            "Try again once it is back."
        }
    }

    /// Why the copy to `target` stopped on `error`, in the operator's
    /// words: the Machine went, or what it could not write and why.
    fn stopped(&self, error: &io::Error, target: &Path, cx: &App) -> String {
        let machine = &self.placing.machine_name;
        let attached = matches!(
            self.model.read(cx).server(self.placing.machine),
            Some(ferrite_client::cockpit::ServerState::Attached { .. })
        );
        if cut_off(error) || !attached {
            return format!("The connection to {machine} was lost. {}", self.next_step(cx));
        }
        // The Machine says `entry: Why (os error N)`, naming where it
        // failed between when that is not the entry: `entry: /where: Why`.
        let said = error.to_string();
        let mut parts: Vec<&str> = said.split(": ").collect();
        let why = plainly(parts.pop().unwrap_or_default());
        let why = if why.is_empty() { "it stopped".into() } else { why };
        let shown = |path: &Path| {
            self.dest
                .read(cx)
                .tilde(path)
                .unwrap_or_else(|| path.display().to_string())
        };
        let absolute = |part: &&&str| part.starts_with(['/', '~']) || part.get(1..3) == Some(":\\");
        match (parts.iter().find(absolute), parts.first().filter(|entry| !entry.is_empty())) {
            (Some(failed), _) => format!("Can't write {}: {why}.", shown(Path::new(failed))),
            (None, Some(entry)) => {
                let written = target.join(entry);
                let folder = written.parent().unwrap_or(target);
                format!("Can't write in {}: {why}.", shown(folder))
            }
            (None, None) => format!("Not copied: {why}."),
        }
    }

    /// The window registered the copy and chose it on the draft. True when
    /// the sheet stays up to list what stayed behind.
    pub(super) fn placed(&mut self, cx: &mut Context<Self>) -> bool {
        let skipped = match &mut self.stage {
            Stage::Registering { skipped, .. } => std::mem::take(skipped),
            _ => Vec::new(),
        };
        let stays = !skipped.is_empty();
        self.enter(Stage::Done { skipped }, cx);
        stays
    }

    pub(super) fn poll(&mut self, cx: &mut Context<Self>) {
        self.dest.update(cx, |dest, cx| dest.poll(cx));
        let mut moved = self.poll_listing();
        self.open_on_unfinished(cx);
        moved |= self.poll_copy(cx);
        if moved {
            cx.notify();
        }
    }

    fn poll_listing(&mut self) -> bool {
        let mut moved = false;
        loop {
            let Some(listing) = &self.listing else {
                return moved;
            };
            match listing.try_recv() {
                Ok(Listed::Unfinished(unfinished)) => {
                    self.opening = unfinished.last().cloned();
                    self.unfinished = unfinished;
                }
                Ok(Listed::Read {
                    unfit,
                    ignored,
                    worktrees,
                }) => {
                    self.unfit = unfit.map(SharedString::from);
                    self.worktrees = worktrees;
                    self.entries = Some(
                        ignored
                            .into_iter()
                            .map(|ignored| Entry {
                                ignored,
                                size: None,
                                ticked: false,
                            })
                            .collect(),
                    );
                }
                Ok(Listed::Size(index, size)) => {
                    if let Some(entry) = self
                        .entries
                        .as_mut()
                        .and_then(|entries| entries.get_mut(index))
                    {
                        entry.size = Some(size);
                    }
                }
                Err(TryRecvError::Empty) => return moved,
                Err(TryRecvError::Disconnected) => {
                    self.listing = None;
                    self.entries.get_or_insert_with(Vec::new);
                    return true;
                }
            }
            moved = true;
        }
    }

    fn poll_copy(&mut self, cx: &mut Context<Self>) -> bool {
        let mut moved = false;
        loop {
            let Some(copy) = &self.copy else {
                return moved;
            };
            let heard = match copy.hear.try_recv() {
                Ok(heard) => heard,
                Err(TryRecvError::Empty) => return moved,
                Err(TryRecvError::Disconnected) => {
                    self.copy = None;
                    if self.busy() {
                        self.failed("The copy stopped".into(), cx);
                    }
                    return true;
                }
            };
            let (dest, target) = (copy.dest.clone(), copy.target.clone());
            moved = true;
            match heard {
                Heard::Copying(Copying::Listing(listed)) => self.enter(Stage::Listing(listed), cx),
                Heard::Copying(Copying::Sending(progress)) => {
                    if !self.unfinished.contains(&target) {
                        self.unfinished.push(target);
                    }
                    self.enter(Stage::Copying(progress), cx);
                }
                Heard::Unreached(error) => {
                    ferrite_core::log!("a copy could not reach {}: {error}", self.placing.machine_name);
                    let reason = format!("Can't reach {}. {}", self.placing.machine_name, self.next_step(cx));
                    self.failed(reason, cx);
                    return true;
                }
                Heard::Copied { copied, unfinished } => {
                    self.copy = None;
                    self.unfinished = unfinished;
                    self.dest.update(cx, |dest, cx| dest.relist(cx));
                    self.copied(copied, dest, target, cx);
                    return true;
                }
            }
        }
    }

    /// The copy to `dest`, `target` as the Machine spells it, ended so.
    fn copied(
        &mut self,
        copied: Result<Copied, Stopped>,
        dest: String,
        target: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let machine = self.placing.machine_name.clone();
        match copied {
            Ok(Copied::Occupied) => self.enter(Stage::Occupied { dest, of: None }, cx),
            Ok(Copied::AnotherCopy { of }) => {
                let of = of.file_name().map(|name| name.to_string_lossy().into_owned());
                let of = Some(of.unwrap_or_else(|| "another Project".into()));
                self.enter(Stage::Occupied { dest, of }, cx);
            }
            Ok(Copied::Sent { sent, changed }) => {
                let skipped = not_copied(&sent, &changed, &machine);
                self.enter(Stage::Registering { dest, skipped }, cx);
                cx.emit(PlacementEvent::Place { dest: target });
            }
            Err(stopped) => {
                ferrite_core::log!("a copy to {} stopped: {}", target.display(), stopped.error);
                let reason = self.stopped(&stopped.error, &target, cx);
                self.failed(reason, cx);
            }
        }
    }

    /// How the copy stands, in a slot the card keeps in every stage so it
    /// holds its size as the copy moves: two lines, whole in its tooltip.
    fn stage_line(&self, cx: &App) -> gpui::Stateful<Div> {
        use ferrite_ui::theme::{BLOCKED, INK_SUBTLE, LH_UI};
        let machine = &self.placing.machine_name;
        let leaf = |dest: &str| {
            let dest = dest.trim_end_matches(['/', '\\']);
            dest.rsplit(['/', '\\']).next().unwrap_or(dest).to_string()
        };
        let (text, ink, selector): (String, u32, &'static str) = match (&self.refusal, &self.stage) {
            (Some(refusal), _) => (refusal.to_string(), BLOCKED, "placement-refusal"),
            (None, Stage::Choosing) => match &self.unfit {
                Some(unfit) => (unfit.to_string(), BLOCKED, "placement-refusal"),
                None if self.resumes(cx) => (
                    format!("An unfinished copy of {} is here.", self.placing.title),
                    INK_SUBTLE,
                    "placement-unfinished",
                ),
                None => (String::new(), INK_SUBTLE, "placement-choosing"),
            },
            (None, &Stage::Listing(listed)) => (
                match listed {
                    0 => "Listing files".into(),
                    listed => format!("Listing files \u{b7} {}", count(listed)),
                },
                INK_SUBTLE,
                "placement-progress",
            ),
            (None, Stage::Occupied { dest, of: None }) => (
                format!("{} is not empty. Nothing there is changed.", leaf(dest)),
                BLOCKED,
                "placement-occupied",
            ),
            (None, Stage::Occupied { dest, of: Some(of) }) => (
                format!("{} holds an unfinished copy of {of}. Nothing there is changed.", leaf(dest)),
                BLOCKED,
                "placement-occupied",
            ),
            (None, &Stage::Copying(Progress { sent, total })) => (
                match total {
                    0 => "Copying".into(),
                    total => format!(
                        "Copying {}% \u{b7} {} of {}",
                        (sent.saturating_mul(100) / total).min(100),
                        bytes(sent),
                        bytes(total)
                    ),
                },
                INK_SUBTLE,
                "placement-progress",
            ),
            (None, Stage::Cancelled) => (
                if self.resumes(cx) {
                    "Copy cancelled. Resume finishes it from where it stopped."
                } else {
                    "Copy cancelled before anything was sent."
                }
                .into(),
                INK_SUBTLE,
                "placement-cancelled",
            ),
            (None, Stage::Registering { dest, .. }) => (
                format!("Adding {} on {machine}", leaf(dest)),
                INK_SUBTLE,
                "placement-adding",
            ),
            (None, Stage::Done { .. }) => (format!("Added on {machine}"), INK_SUBTLE, "placement-added"),
            (None, Stage::Failed { reason }) => (reason.to_string(), BLOCKED, "placement-failed"),
        };
        let text = SharedString::from(text);
        div()
            .id("placement-stage")
            .debug_selector(|| "placement-stage".into())
            .flex_shrink_0()
            .min_w_0()
            .h(px(2.0 * LH_UI))
            .when(!text.is_empty(), |line| {
                line.tooltip(ferrite_ui::menu::tooltip(text.clone()))
            })
            .child(
                ferrite_ui::components::text_ui()
                    .line_clamp(2)
                    .text_color(gpui::rgb(ink))
                    .debug_selector(move || selector.into())
                    .child(text),
            )
    }

    /// The card, for the sheet host.
    pub(super) fn card(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<ferrite_ui::sheet::Drawn> {
        use ferrite_ui::theme::*;
        let machine = &self.placing.machine_name;
        let close = ferrite_ui::sheet::sheet_close("placement-close", "Close", cx).on_click(
            cx.listener(|sheet: &mut Self, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                sheet.cancel(cx);
            }),
        );
        let held = self.held();
        let resumes = self.resumes(cx);
        let name: AnyElement = if held {
            ferrite_ui::components::text_ui()
                .truncate()
                .text_color(gpui::rgb(INK_SUBTLE))
                .child(self.copy_name(cx).to_string())
                .into_any_element()
        } else {
            self.name.clone().into_any_element()
        };
        // The body fills the card, the browser's folders taking what the
        // rest leaves.
        let mut body = ferrite_ui::form::body()
            .flex_grow(1.)
            .debug_selector(|| "placement-body".into())
            .track_scroll(&self.scroll)
            .child(self.dest.clone())
            .child(
                ferrite_ui::field::prefixed_field(
                    if matches!(self.stage, Stage::Done { .. }) {
                        "Copied to"
                    } else {
                        "Copies to"
                    }
                    .into(),
                    self.folder_prefix(cx).into(),
                    name,
                    !held && self.name.read(cx).focus_handle(cx).is_focused(window),
                    "placement-name-focused",
                )
                .debug_selector(|| "placement-name".into()),
            )
            .child(self.stage_line(cx));
        if let Stage::Done { skipped } = &self.stage {
            if !skipped.is_empty() {
                body = body
                    .child(ferrite_ui::form::section_label("Not copied", "Left as it was"))
                    .child(
                        ferrite_ui::form::directory_list(
                            skipped.iter().map(|path| note(path.clone())),
                        )
                        .debug_selector(|| "placement-skipped".into()),
                    );
            }
        }
        body = match &self.entries {
            None => body.child(
                note("Reading what git ignores").debug_selector(|| "placement-listing".into()),
            ),
            Some(entries) => body.child(self.ignored_section(entries, cx)),
        };
        let unready = self.entries.is_none() || self.unfit.is_some();
        let (verb, selector, disabled): (&'static str, &'static str, bool) = match self.stage {
            Stage::Occupied { of: None, .. } => ("Use what is there", "placement-use", false),
            Stage::Occupied { .. } => ("Copy", "placement-copy", true),
            Stage::Done { .. } => ("Done", "placement-done", false),
            Stage::Registering { .. } | Stage::Listing(_) | Stage::Copying(_) if self.resuming => {
                ("Resume", "placement-resume", true)
            }
            Stage::Registering { .. } | Stage::Listing(_) | Stage::Copying(_) => {
                ("Copy", "placement-copy", true)
            }
            _ if resumes => ("Resume", "placement-resume", unready),
            Stage::Failed { .. } => ("Retry", "placement-retry", unready),
            Stage::Choosing | Stage::Cancelled => ("Copy", "placement-copy", unready),
        };
        let primary = ferrite_ui::form::primary_button("placement-confirm", verb, disabled, cx)
            .debug_selector(move || selector.into())
            .on_click(cx.listener(|sheet: &mut Self, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                match sheet.stage {
                    Stage::Occupied { .. } => sheet.use_what_is_there(cx),
                    _ => sheet.confirm(cx),
                }
            }));
        let done = matches!(self.stage, Stage::Done { .. } | Stage::Registering { .. });
        let footer_left = div().when(!held, |left| {
            left.child(
                ferrite_ui::components::ghost_button("placement-new-folder", "New folder", cx)
                    .debug_selector(|| "placement-new-folder".into())
                    .on_click(cx.listener(|sheet: &mut Self, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        sheet
                            .dest
                            .update(cx, |dest, cx| dest.new_folder(window, cx));
                    })),
            )
        });
        let footer_right = div()
            .flex()
            .items_center()
            .gap(px(SPACE_2))
            .when(!done, |right| {
                right.child(
                    ferrite_ui::components::ghost_button("placement-cancel", "Cancel", cx)
                        .debug_selector(|| "placement-cancel".into())
                        .on_click(cx.listener(|sheet: &mut Self, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            sheet.cancel(cx);
                        })),
                )
            })
            .child(primary);
        let band_top = self.band.as_ref().and_then(Band::top);
        let card = ferrite_ui::form::steady_card_over(f32::from(window.viewport_size().height), band_top)
            .id("placement-card")
            .debug_selector(|| "placement-card".into())
            .track_focus(&self.focus)
            .occlude()
            .child(ferrite_ui::form::head(
                format!("Copy {} to {machine}", self.placing.title).into(),
                close,
            ))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_grow(1.)
                    .flex_shrink(1.)
                    .min_h_0()
                    .child(body)
                    .child(ferrite_ui::components::scrollbar(
                        "placement-scrollbar",
                        &self.scroll,
                    )),
            )
            .child(ferrite_ui::form::footer(footer_left, footer_right));
        Some(ferrite_ui::sheet::Drawn { card, above: None })
    }
}

/// The copy's name until the operator gives it one: the Project's folder's.
fn leaf(placing: &Placing) -> String {
    placing
        .root
        .file_name()
        .map(|leaf| leaf.to_string_lossy().into_owned())
        .unwrap_or_else(|| placing.title.to_string())
}

/// How many alike are listed one by one under Not copied before they are
/// told as one line.
const LISTED: usize = 3;

/// What a copy to `machine` left as it was, in the operator's words: each
/// by its path and why, or, past [`LISTED`] of one kind left for one
/// reason, how many and the folder they share.
fn not_copied(sent: &Sent, changed: &[String], machine: &str) -> Vec<String> {
    // Each one left: its path, what it is, and what is said of one and of
    // several.
    let mut left: Vec<(&str, &str, String, String)> = Vec::new();
    for problem in &sent.problems {
        let (one, many) = match problem.side {
            Side::Here => ("in use here".to_string(), "were in use here".to_string()),
            Side::There => (format!("unreadable on {machine}"), format!("could not be read on {machine}")),
        };
        left.push((&problem.path, "files", one, many));
    }
    for unmade in &sent.unmade {
        let what = match unmade.entry {
            tree::Entry::Dir => "folders",
            tree::Entry::Symlink { .. } => "links",
            _ => "files",
        };
        let (one, many) = match &unmade.why {
            Refusal::LeadsOut => (
                format!("a link outside the Project, not made on {machine}"),
                format!("lead outside the Project and were not made on {machine}"),
            ),
            Refusal::LeadsNowhere => (
                format!("a link to nothing on {machine}, not made"),
                format!("lead to nothing on {machine} and were not made"),
            ),
            Refusal::Name => (
                format!("a name {machine} cannot hold"),
                format!("have names {machine} cannot hold"),
            ),
            Refusal::Failed { error } => {
                let why = plainly(error);
                (format!("not made on {machine}: {why}"), format!("could not be made on {machine}: {why}"))
            }
            Refusal::Unknown => (format!("not made on {machine}"), format!("could not be made on {machine}")),
        };
        left.push((&unmade.path, what, one, many));
    }
    for path in changed {
        left.push((path, "files", format!("changed on {machine}"), format!("were changed on {machine}")));
    }
    let mut alike: Vec<((&str, String), Vec<(&str, String)>)> = Vec::new();
    for (path, what, one, many) in left {
        match alike.iter_mut().find(|((kind, said), _)| *kind == what && *said == many) {
            Some((_, these)) => these.push((path, one)),
            None => alike.push(((what, many), vec![(path, one)])),
        }
    }
    alike
        .into_iter()
        .flat_map(|((what, many), these)| {
            if these.len() <= LISTED {
                return these.into_iter().map(|(path, one)| format!("{path} \u{b7} {one}")).collect();
            }
            let folder = shared_folder(these.iter().map(|(path, _)| *path));
            let within = if folder.is_empty() { String::new() } else { format!(" in {folder}") };
            vec![format!("{} {what}{within} {many}", count(these.len()))]
        })
        .collect()
}

/// The deepest folder that holds every one of `paths`: `""` for the
/// Project's own.
fn shared_folder<'a>(paths: impl Iterator<Item = &'a str>) -> String {
    let mut shared: Option<Vec<&str>> = None;
    for path in paths {
        let mut folder: Vec<&str> = path.split('/').collect();
        folder.pop();
        shared = Some(match shared {
            None => folder,
            Some(shared) => shared.into_iter().zip(folder).take_while(|(one, other)| one == other).map(|(one, _)| one).collect(),
        });
    }
    shared.unwrap_or_default().join("/")
}

/// What a Server said went wrong, as a clause: `Access is denied. (os
/// error 5)` reads `access is denied`.
fn plainly(said: &str) -> String {
    let why = said.split(" (os error").next().unwrap_or(said).trim_end_matches('.');
    let mut letters = why.chars();
    match letters.next() {
        Some(first) => first.to_lowercase().chain(letters).collect(),
        None => String::new(),
    }
}

/// Whether `error` is the connection to the Machine ending under the copy.
fn cut_off(error: &io::Error) -> bool {
    use io::ErrorKind::*;
    matches!(
        error.kind(),
        BrokenPipe | ConnectionReset | ConnectionAborted | NotConnected | UnexpectedEof | TimedOut
    )
}

/// A quiet line on the card.
fn note(text: impl Into<SharedString>) -> Div {
    ferrite_ui::components::text_ui()
        .flex_shrink_0()
        .text_color(gpui::rgb(ferrite_ui::theme::INK_SUBTLE))
        .child(text.into())
}

/// A count as the operator reads it: `12,500`.
fn count(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A size as the operator reads it: `340 KB`, `1.2 GB`.
fn bytes(size: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if size < 1000 {
        return format!("{size} B");
    }
    let mut value = size as f64 / 1000.0;
    let mut unit = 0;
    while value >= 1000.0 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::tree::{Problem, Unmade};

    fn unmade(path: &str, entry: tree::Entry, why: Refusal) -> Unmade {
        Unmade {
            path: path.into(),
            entry,
            why,
        }
    }

    fn link() -> tree::Entry {
        tree::Entry::Symlink { target: "../x".into() }
    }

    /// A few left are each named by path; many of one kind left for one
    /// reason are one line, with how many and the folder they share.
    #[test]
    fn what_a_copy_left_is_said_plainly_and_many_alike_as_one_line() {
        let mut sent = Sent::default();
        sent.problems.push(Problem {
            path: "server.log".into(),
            side: Side::Here,
            error: "The process cannot access the file. (os error 32)".into(),
        });
        for at in 0..312 {
            let path = format!("apps/web/node_modules/@scope/name-{at}");
            sent.unmade.push(unmade(&path, link(), Refusal::LeadsOut));
        }
        sent.unmade.push(unmade("apps/docs/node_modules/x", link(), Refusal::LeadsOut));
        sent.unmade.push(unmade(".env", link(), Refusal::LeadsNowhere));
        sent.unmade.push(unmade("src/aux", tree::Entry::Dir, Refusal::Name));
        let failed = Refusal::Failed {
            error: "Access is denied. (os error 5)".into(),
        };
        sent.unmade.push(unmade("dist/app.js", tree::Entry::File { size: 1, sha256: String::new(), mode: None, modified: None, text: None }, failed));
        assert_eq!(
            not_copied(&sent, &["notes.md".into()], "josephpc"),
            [
                "server.log \u{b7} in use here",
                "313 links in apps lead outside the Project and were not made on josephpc",
                ".env \u{b7} a link to nothing on josephpc, not made",
                "src/aux \u{b7} a name josephpc cannot hold",
                "dist/app.js \u{b7} not made on josephpc: access is denied",
                "notes.md \u{b7} changed on josephpc",
            ]
        );
        sent.unmade.truncate(312);
        assert_eq!(
            not_copied(&Sent { problems: Vec::new(), ..sent }, &[], "josephpc"),
            ["312 links in apps/web/node_modules/@scope lead outside the Project and were not made on josephpc"]
        );
    }
}
