//! The setup sheet (ADR 0048) as a view of its own (ADR 0041, 0042): a
//! Project of this computer copied to a Machine for a draft picking that
//! environment. It owns where the copy goes, a folder browsed on the
//! Machine and the name of the copy in it, the entries git ignores and
//! which of them go too, and the copy, which runs on a thread of its own
//! over its own connection (`Cockpit::tree`). Once the folder is there it
//! says so as a `PlacementEvent`, and the Cockpit registers it and chooses
//! it on the draft.

use super::draft_environment::Band;
use super::folder_browser::{FolderBrowser, Moved};
use super::*;
use ferrite_core::chunked::{Cancellation, Progress};
use ferrite_core::placement::{self, Ignored};
use ferrite_core::tree::{Sent, Stopped};
use gpui::component::{checkbox::Checkbox, Disableable};
use gpui::App;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

/// What the sheet copies, and for whom.
#[derive(Clone)]
pub(crate) struct Placing {
    /// The draft that picked the Machine: the copy is chosen on it.
    pub(crate) draft: PaneIdentity,
    pub(crate) title: SharedString,
    pub(crate) root: PathBuf,
    pub(crate) repository: Option<String>,
    pub(crate) machine: ferrite_core::MachineId,
    pub(crate) machine_name: SharedString,
    /// The Machine's projects folder, where the browser opens.
    pub(crate) projects_dir: String,
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
    /// Looking at the destination before anything is sent there.
    Scanning,
    /// `dest` holds something: it takes no copy.
    Occupied {
        dest: String,
    },
    Copying(Progress),
    /// Copied to `dest`; the window is registering it. What could not be
    /// read stays behind and is listed once it is in.
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

/// One entry git ignores, and whether it goes too.
struct Entry {
    ignored: Ignored,
    size: Option<u64>,
    ticked: bool,
}

/// What the thread listing the ignored entries hands back.
enum Listed {
    Entries(Vec<Ignored>),
    Size(usize, u64),
}

/// What the copy's thread hands back.
enum Heard {
    Occupied,
    Progress(Progress),
    Copied(Result<Sent, Stopped>),
}

pub(crate) struct PlacementView {
    model: Entity<Cockpit>,
    pub(crate) placing: Placing,
    /// The folder on the Machine the copy goes into.
    pub(super) dest: Entity<FolderBrowser>,
    /// The copy's own folder in it: the Project's folder name until edited.
    pub(super) name: Entity<Composer>,
    entries: Option<Vec<Entry>>,
    listing: Option<Receiver<Listed>>,
    stage: Stage,
    /// Why the destination takes no copy, once Copy was pressed.
    refusal: Option<&'static str>,
    /// The destination a copy last went to: one into it again finishes
    /// that copy rather than meets what it left.
    copied_into: Option<String>,
    copy: Option<Running>,
    scroll: ScrollHandle,
    /// The draft's band, which the card keeps clear of.
    band: Option<Band>,
}

/// A copy under way: to `dest`, as it was chosen when it began.
struct Running {
    dest: String,
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
        let leaf = placing
            .root
            .file_name()
            .map(|leaf| leaf.to_string_lossy().into_owned())
            .unwrap_or_else(|| placing.title.to_string());
        let dest = cx.new(|cx| {
            FolderBrowser::new(
                model.clone(),
                placing.machine,
                format!("Folder on {}", placing.machine_name).into(),
                format!("{}/", placing.projects_dir.trim_end_matches('/')),
                cx,
            )
        });
        cx.subscribe(&dest, |sheet: &mut Self, _, _: &Moved, cx| sheet.edited(cx))
            .detach();
        let name = cx.new(Composer::new);
        name.update(cx, |name, cx| {
            name.set_role(ferrite_pane::composer::Role::Field(None), cx);
            name.set(leaf, cx);
        });
        cx.subscribe(&name, |sheet: &mut Self, _, _: &Edited, cx| {
            sheet.edited(cx)
        })
        .detach();
        let (found, listing) = ferrite_core::doorbell::channel();
        let root = placing.root.clone();
        std::thread::spawn(move || {
            let ignored = placement::ignored(&root);
            let paths: Vec<String> = ignored.iter().map(|entry| entry.path.clone()).collect();
            if found.send(Listed::Entries(ignored)).is_err() {
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
            entries: None,
            listing: Some(listing),
            stage: Stage::Choosing,
            refusal: None,
            copied_into: None,
            copy: None,
            scroll: ScrollHandle::new(),
            band,
        }
    }

    pub(super) fn focus(&self, cx: &App) -> FocusHandle {
        self.dest.read(cx).focus_handle(cx)
    }

    /// Whether one of the sheet's fields holds the keyboard: the browser's,
    /// or the copy's name.
    pub(super) fn holds_focus(&self, window: &Window, cx: &App) -> bool {
        self.dest.read(cx).focused(window, cx)
            || self.name.read(cx).focus_handle(cx).is_focused(window)
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

    /// Where the copy goes moved: what it was found to hold no longer
    /// counts.
    fn edited(&mut self, cx: &mut Context<Self>) {
        self.refusal = None;
        if matches!(self.stage, Stage::Occupied { .. } | Stage::Failed { .. }) {
            self.enter(Stage::Choosing, cx);
        }
        cx.notify();
    }

    fn busy(&self) -> bool {
        matches!(
            self.stage,
            Stage::Scanning | Stage::Copying(_) | Stage::Registering { .. }
        )
    }

    /// From Copy on, where the copy goes is held: the browser, the copy's
    /// name and the ticks take nothing, unless it stops short (`Occupied`,
    /// `Failed`).
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

    /// Copy to the destination chosen, unless it is no folder to copy into,
    /// or holds something this sheet did not put there.
    fn start(&mut self, cx: &mut Context<Self>) {
        if self.busy() || self.entries.is_none() {
            return;
        }
        let dest = self.destination(cx);
        self.refusal = if self.copy_name(cx).is_empty() {
            Some("Name the copy's folder")
        } else {
            placement::refuse_destination(&dest)
        };
        if self.refusal.is_some() {
            cx.notify();
            return;
        }
        let machine = self.placing.machine;
        let connect = match self.model.update(cx, |cockpit, _| cockpit.tree(machine)) {
            Ok(connect) => connect,
            Err(error) => return self.failed(format!("Not copied: {error}"), cx),
        };
        let skip: Vec<String> = self
            .entries
            .iter()
            .flatten()
            .filter(|entry| !entry.ticked)
            .map(|entry| entry.ignored.path.clone())
            .collect();
        let fresh = self.copied_into.as_deref() != Some(dest.as_str());
        let (source, target) = (self.placing.root.clone(), PathBuf::from(&dest));
        let (heard, hear) = ferrite_core::doorbell::channel();
        let cancel = Cancellation::default();
        let cancelled = cancel.clone();
        std::thread::spawn(move || {
            let last = (|| {
                let mut exchange = connect()?;
                if fresh && placement::occupied(&target, &mut *exchange)? {
                    return Ok(Heard::Occupied);
                }
                let _ = heard.send(Heard::Progress(Progress { sent: 0, total: 0 }));
                let mut progress = |progress| {
                    let _ = heard.send(Heard::Progress(progress));
                };
                placement::copy(
                    &source,
                    &target,
                    &skip,
                    &mut *exchange,
                    &mut progress,
                    &cancelled,
                )
                .map(|sent| Heard::Copied(Ok(sent)))
            })();
            let _ = heard.send(last.unwrap_or_else(|stopped| Heard::Copied(Err(stopped))));
        });
        self.copy = Some(Running { dest, hear, cancel });
        self.enter(Stage::Scanning, cx);
    }

    /// Register what is at the destination as it is, copying nothing.
    fn use_what_is_there(&mut self, cx: &mut Context<Self>) {
        let Stage::Occupied { dest } = &self.stage else {
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

    /// Stop the copy under way; what it sent stays, for the next to finish.
    pub(super) fn stop(&mut self) {
        if let Some(copy) = self.copy.take() {
            copy.cancel.cancel();
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if matches!(self.stage, Stage::Scanning | Stage::Copying(_)) {
            self.stop();
            self.enter(
                Stage::Failed {
                    reason: "Cancelled".into(),
                },
                cx,
            );
        } else {
            cx.emit(PlacementEvent::Closed);
        }
    }

    /// ↵: the footer's own verb, where it has one.
    pub(super) fn confirm(&mut self, cx: &mut Context<Self>) {
        match self.stage {
            Stage::Choosing | Stage::Failed { .. } => self.start(cx),
            Stage::Done { .. } => cx.emit(PlacementEvent::Closed),
            _ => {}
        }
    }

    /// Tab in the browser: go into the first folder its line fits. False
    /// when it does not hold the keyboard.
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
        moved |= self.poll_copy(cx);
        if moved {
            cx.notify();
        }
    }

    fn poll_listing(&mut self) -> bool {
        let Some(listing) = &self.listing else {
            return false;
        };
        let mut moved = false;
        loop {
            match listing.try_recv() {
                Ok(Listed::Entries(ignored)) => {
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
            let dest = copy.dest.clone();
            moved = true;
            match heard {
                Heard::Progress(progress) => {
                    if !matches!(self.stage, Stage::Copying(_)) {
                        self.copied_into = Some(dest);
                    }
                    self.enter(Stage::Copying(progress), cx);
                }
                Heard::Occupied => {
                    self.copy = None;
                    self.enter(Stage::Occupied { dest }, cx);
                    return true;
                }
                Heard::Copied(Ok(sent)) => {
                    let spelled = self.dest.read(cx).spelled_out(&dest);
                    self.copy = None;
                    let skipped = sent.problems.into_iter().map(|problem| problem.path).collect();
                    self.enter(Stage::Registering { dest, skipped }, cx);
                    cx.emit(PlacementEvent::Place { dest: spelled });
                    return true;
                }
                Heard::Copied(Err(stopped)) => {
                    self.failed(format!("Not copied: {}", stopped.error), cx);
                    return true;
                }
            }
        }
    }

    fn tick(&mut self, index: usize, ticked: bool, cx: &mut Context<Self>) {
        if let Some(entry) = self
            .entries
            .as_mut()
            .and_then(|entries| entries.get_mut(index))
        {
            entry.ticked = ticked;
        }
        cx.notify();
    }

    /// How the copy stands, on a line the card keeps in every stage so it
    /// holds its size as the copy moves: cut at its end, whole in its
    /// tooltip.
    fn stage_line(&self) -> gpui::Stateful<Div> {
        use ferrite_ui::theme::{BLOCKED, INK_SUBTLE, LH_UI};
        let machine = &self.placing.machine_name;
        let leaf = |dest: &str| {
            let dest = dest.trim_end_matches(['/', '\\']);
            dest.rsplit(['/', '\\']).next().unwrap_or(dest).to_string()
        };
        let (text, ink, selector): (String, u32, &'static str) = match (self.refusal, &self.stage) {
            (Some(refusal), _) => (refusal.into(), BLOCKED, "placement-refusal"),
            (None, Stage::Choosing) => (String::new(), INK_SUBTLE, "placement-choosing"),
            (None, Stage::Scanning) => ("Checking the folder".into(), INK_SUBTLE, "placement-checking"),
            (None, Stage::Occupied { dest }) => (
                format!("{} is not empty. Nothing there is changed.", leaf(dest)),
                BLOCKED,
                "placement-occupied",
            ),
            (None, &Stage::Copying(Progress { sent, total })) => (
                match total {
                    0 => "Listing files".into(),
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
            .h(px(LH_UI))
            .when(!text.is_empty(), |line| {
                line.tooltip(ferrite_ui::menu::tooltip(text.clone()))
            })
            .child(
                ferrite_ui::components::text_ui()
                    .truncate()
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
            cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(PlacementEvent::Closed);
            }),
        );
        let held = self.held();
        let name: AnyElement = if held {
            ferrite_ui::components::text_ui()
                .truncate()
                .text_color(gpui::rgb(INK_SUBTLE))
                .child(self.copy_name(cx).to_string())
                .into_any_element()
        } else {
            self.name.clone().into_any_element()
        };
        let mut body = project_editor::body()
            .track_scroll(&self.scroll)
            .child(self.dest.clone())
            .child(
                project_editor::prefixed_field(
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
            .child(self.stage_line());
        if let Stage::Done { skipped } = &self.stage {
            if !skipped.is_empty() {
                body = body
                    .child(project_editor::section_label(
                        "Not copied",
                        "In use while copying",
                    ))
                    .child(
                        project_editor::directory_list(
                            skipped.iter().map(|path| note(path.clone())),
                        )
                        .debug_selector(|| "placement-skipped".into()),
                    );
            }
        }
        match &self.entries {
            None => {
                body = body.child(
                    note("Reading what git ignores").debug_selector(|| "placement-listing".into()),
                )
            }
            Some(entries) if !entries.is_empty() => {
                body = body.child(project_editor::section_label(
                    "Ignored by git",
                    "Copied only when ticked",
                ));
                body = body.child(project_editor::directory_list(
                    entries.iter().enumerate().map(|(index, entry)| {
                        let path = &entry.ignored.path;
                        let shown = if entry.ignored.directory {
                            format!("{path}/")
                        } else {
                            path.clone()
                        };
                        let selector = format!("placement-ignored:{path}");
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .h(px(ROW))
                            .gap(px(2.0 * CH))
                            .child(
                                Checkbox::new(("placement-ignored", index))
                                    .debug_selector(move || selector.clone())
                                    .checked(entry.ticked)
                                    .disabled(held)
                                    .accessibility_label(shown.clone())
                                    // A label is drawn one line high, which
                                    // clips an underscore; a child is taller
                                    // than the box, so it is centred on it.
                                    .items_center()
                                    .child(shown)
                                    .on_click(cx.listener(
                                        move |sheet: &mut Self, ticked: &bool, _, cx| {
                                            sheet.tick(index, *ticked, cx)
                                        },
                                    )),
                            )
                            .child(note(entry.size.map(bytes).unwrap_or_default()))
                    }),
                ));
            }
            Some(_) => {}
        }
        let (verb, selector, disabled): (&'static str, &'static str, bool) = match self.stage {
            Stage::Occupied { .. } => ("Use what is there", "placement-use", false),
            Stage::Failed { .. } => ("Retry", "placement-retry", false),
            Stage::Done { .. } => ("Done", "placement-done", false),
            Stage::Registering { .. } | Stage::Scanning | Stage::Copying(_) => {
                ("Copy", "placement-copy", true)
            }
            Stage::Choosing => ("Copy", "placement-copy", self.entries.is_none()),
        };
        let primary = project_editor::primary_button("placement-confirm", verb, disabled, cx)
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
        let card = project_editor::card_over(f32::from(window.viewport_size().height), band_top)
            .id("placement-card")
            .debug_selector(|| "placement-card".into())
            .occlude()
            .child(project_editor::head(
                format!("Copy {} to {machine}", self.placing.title).into(),
                close,
            ))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_shrink(1.)
                    .min_h_0()
                    .child(body)
                    .child(ferrite_ui::components::scrollbar(
                        "placement-scrollbar",
                        &self.scroll,
                    )),
            )
            .child(project_editor::footer(footer_left, footer_right));
        Some(ferrite_ui::sheet::Drawn { card, above: None })
    }
}

/// A quiet line on the card.
fn note(text: impl Into<SharedString>) -> Div {
    ferrite_ui::components::text_ui()
        .flex_shrink_0()
        .text_color(gpui::rgb(ferrite_ui::theme::INK_SUBTLE))
        .child(text.into())
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
