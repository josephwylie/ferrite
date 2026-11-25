//! The setup sheet (ADR 0048) as a view of its own (ADR 0041, 0042): a
//! Project of this computer copied to a Machine for a draft picking that
//! environment. It chooses where the copy goes, a folder browsed on the
//! Machine and the copy's name in it, and which of the entries git ignores
//! go too. The copy itself is the window's (`Copies`), keyed by Project and
//! Machine: the card is one view of it, as a toast and the draft's chip
//! are, and holds none of its state. While the copy runs the card shows how
//! far it is; it can go and come back without the copy noticing. A copy
//! stopped part way is unfinished (`placement::unfinished`): the sheet
//! opens on it, and only its Resume goes there.

use super::copies::{self, Copies, CopyKey, Order, Phase};
use super::draft_environment::Band;
use super::folder_browser::{FolderBrowser, Moved};
use super::*;
use ferrite_core::placement::{self, Ignored};
use gpui::App;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

/// The most folders the card's browser lists before it scrolls.
const BROWSER_ROWS: usize = 8;

mod card_size;
mod faces;
mod ignored;
use card_size::CardSize;
use ignored::Entry;

/// What the sheet copies, and for whom.
#[derive(Clone)]
pub(crate) struct Placing {
    /// The draft that picked the Machine: the copy is chosen on it.
    pub(crate) draft: PaneIdentity,
    /// The Project copied.
    pub(crate) project: ProjectKey,
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

impl Placing {
    pub(crate) fn key(&self) -> CopyKey {
        CopyKey {
            project: self.project,
            machine: self.machine,
        }
    }
}

/// What the sheet asks of the window.
pub(crate) enum PlacementEvent {
    /// The card goes; a copy it shows carries on.
    Closed,
    /// Add the folder the copy found occupied as it is, copying nothing.
    Use,
}

impl EventEmitter<PlacementEvent> for PlacementView {}

/// What the card shows, by where its copy is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Face {
    /// Where the copy goes, and what goes: before a copy, or after one
    /// stopped short.
    Choose,
    /// How far the copy is.
    Progress,
    /// What the copy did.
    Result,
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
    /// The bytes of all the rest, which always goes.
    Rest(u64),
    Size(usize, u64),
}

pub(crate) struct PlacementView {
    copies: Entity<Copies>,
    pub(crate) placing: Placing,
    /// The folder on the Machine the copy goes into.
    pub(super) dest: Entity<FolderBrowser>,
    /// The copy's own folder in it: the Project's folder name until edited.
    pub(super) name: Entity<Composer>,
    /// The card's own, which holds the keyboard while no field is shown:
    /// keys reach nothing under the sheet.
    focus: FocusHandle,
    entries: Option<Vec<Entry>>,
    /// The bytes of the Project but what git ignores and its worktrees.
    rest: Option<u64>,
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
    /// What the copy the card opened on left here, ticked to match once
    /// the entries git ignores are read.
    kept_back: Option<Vec<String>>,
    /// Why the destination takes no copy, once Copy was pressed.
    refusal: Option<SharedString>,
    /// The folder browser is open: the operator is choosing where.
    browsing: bool,
    /// The folder the card put the browser at. The browser elsewhere with
    /// the operator not browsing went there itself, that folder unread.
    put_at: String,
    scroll: ScrollHandle,
    /// The draft's band, which the card keeps clear of.
    band: Option<Band>,
    size: CardSize,
}

impl PlacementView {
    /// The card for `placing`: on the copy of its Project to its Machine,
    /// where one is, else on choosing one.
    pub(crate) fn new(
        model: Entity<Cockpit>,
        copies: Entity<Copies>,
        placing: Placing,
        band: Option<Band>,
        cx: &mut Context<Self>,
    ) -> Self {
        let order = copies.read(cx).get(placing.key()).map(|copy| copy.order.clone());
        let (folder, leaf) = match &order {
            Some(order) => split_dest(&order.dest),
            None => (
                format!("{}/", placing.projects_dir.trim_end_matches('/')),
                leaf(&placing),
            ),
        };
        let dest = cx.new(|cx| {
            FolderBrowser::new(
                model.clone(),
                placing.machine,
                format!("Folder on {}", placing.machine_name).into(),
                folder.clone(),
                cx,
            )
            .making("The copy makes this folder")
            .fitted(BROWSER_ROWS)
        });
        dest.update(cx, |dest, cx| dest.ask_home(cx));
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
        Self {
            copies,
            placing,
            dest,
            name,
            focus: cx.focus_handle(),
            entries: None,
            rest: None,
            unfit: None,
            worktrees: Vec::new(),
            listing: None,
            unfinished: Vec::new(),
            opening: None,
            kept_back: order.map(|order| order.skip),
            refusal: None,
            browsing: false,
            put_at: folder,
            scroll: ScrollHandle::new(),
            band,
            size: CardSize::default(),
        }
    }

    pub(crate) fn key(&self) -> CopyKey {
        self.placing.key()
    }

    /// Read the Project on a thread of its own: where its unfinished copies
    /// are, what git ignores in it and how big each part is. Once, when the
    /// card first chooses: a card on a copy under way needs none of it.
    fn list_project(&mut self) {
        let (found, listing) = ferrite_core::doorbell::channel();
        let (root, kept) = (self.placing.root.clone(), self.placing.kept.clone());
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
            let skipped: Vec<String> = paths.iter().chain(&worktrees).cloned().collect();
            let read = Listed::Read {
                unfit: placement::refuse_source(&root),
                ignored,
                worktrees,
            };
            if found.send(read).is_err() {
                return;
            }
            if found
                .send(Listed::Rest(placement::size_but(&root, &skipped)))
                .is_err()
            {
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
        self.listing = Some(listing);
    }

    /// The copy the card shows, while the window has one.
    fn copy<'a>(&self, cx: &'a App) -> Option<&'a copies::Entry> {
        self.copies.read(cx).get(self.key())
    }

    fn face(&self, cx: &App) -> Face {
        match self.copy(cx).map(|copy| &copy.phase) {
            Some(phase) if phase.live() => Face::Progress,
            Some(Phase::Done(_)) => Face::Result,
            _ => Face::Choose,
        }
    }

    /// Whether the card shows where the copy goes, rather than the copy.
    fn choosing(&self, cx: &App) -> bool {
        self.face(cx) == Face::Choose
    }

    /// Whether the card shows a copy: one under way, or how one ended.
    pub(crate) fn shows_a_copy(&self, cx: &App) -> bool {
        self.copy(cx).is_some()
    }

    /// Where the keyboard goes while the sheet is up: the browser's line
    /// while it is open, else the card itself.
    pub(super) fn focus(&self, cx: &App) -> FocusHandle {
        if self.browsing && self.choosing(cx) {
            self.dest.read(cx).focus_handle(cx)
        } else {
            self.focus.clone()
        }
    }

    /// Whether the sheet holds the keyboard: the browser's, the copy's
    /// name, or the card.
    pub(super) fn holds_focus(&self, window: &Window, cx: &App) -> bool {
        let field = self.dest.read(cx).focused(window, cx)
            || self.name.read(cx).focus_handle(cx).is_focused(window);
        (field && self.browsing && self.choosing(cx)) || self.focus.contains_focused(window, cx)
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
        let target = self.target(cx);
        self.unfinished.contains(&target) || self.copy(cx).is_some_and(|copy| copy.unfinished.contains(&target))
    }

    /// Where the copy goes moved. Moved by the operator from where the last
    /// copy went, what that copy found there and how it ended no longer
    /// count; the browser going home by itself from a folder it cannot
    /// read moves nothing the operator chose.
    fn edited(&mut self, cx: &mut Context<Self>) {
        self.refusal = None;
        let elsewhere = self.copy(cx).is_some_and(|copy| {
            let chosen = !self.dest.read(cx).untouched() || split_dest(&copy.order.dest).1 != self.copy_name(cx);
            chosen && copy.order.dest != self.destination(cx)
        });
        if elsewhere {
            self.let_go(cx);
        }
        cx.notify();
    }

    /// Let go of how the last copy ended, keeping what it found
    /// unfinished.
    fn let_go(&mut self, cx: &mut Context<Self>) {
        let key = self.key();
        if let Some(unfinished) = self.copies.update(cx, |copies, cx| copies.forget(key, cx)) {
            for path in unfinished {
                if !self.unfinished.contains(&path) {
                    self.unfinished.push(path);
                }
            }
        }
    }

    /// The sheet opens on the copy of the Project begun last and left
    /// unfinished, under `~/` as the browser shows paths, unless the
    /// operator has aimed it already.
    fn open_on_unfinished(&mut self, cx: &mut Context<Self>) {
        let Some(last) = &self.opening else {
            return;
        };
        if self.browsing || !self.dest.read(cx).untouched() || self.copy_name(cx) != leaf(&self.placing) {
            self.opening = None;
            return;
        }
        let Some(last) = self.dest.read(cx).tilde(last) else {
            return;
        };
        self.opening = None;
        let (folder, name) = split_dest(&last);
        self.put_at = folder.clone();
        self.dest.update(cx, |dest, cx| dest.open_at(folder, cx));
        self.name.update(cx, |field, cx| field.set(name, cx));
    }

    /// The browser opens by itself when where the copy would go is in
    /// doubt: the folder it was put at could not be read, so it went home,
    /// or the copy's name is taken there by what it would not resume. Not
    /// before the Project is read: until then a folder of the copy's name
    /// may be its own unfinished copy, which the card opens on instead.
    fn browse_if_unsure(&mut self, cx: &mut Context<Self>) {
        if self.browsing || self.opening.is_some() || self.entries.is_none() || !self.choosing(cx) {
            return;
        }
        let elsewhere = self.folder_prefix(cx) != self.put_at;
        let taken = self.dest.read(cx).has(self.copy_name(cx)) == Some(true) && !self.resumes(cx);
        if elsewhere || taken {
            self.browsing = true;
            cx.notify();
        }
    }

    /// Change…: open the browser on where the copy goes.
    fn browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.browsing = true;
        let line = self.dest.read(cx).focus_handle(cx);
        window.focus(&line, cx);
        cx.notify();
    }

    /// What stays here: the entries git ignores left unticked, and the
    /// worktrees.
    fn kept_here(&self) -> Vec<String> {
        self.entries
            .iter()
            .flatten()
            .filter(|entry| !entry.ticked)
            .map(|entry| entry.ignored.path.clone())
            .chain(self.worktrees.iter().cloned())
            .collect()
    }

    /// The bytes the copy sends, once every part is measured.
    fn to_send(&self) -> Option<u64> {
        let entries = self.entries.as_ref()?.iter();
        let ticked: Option<u64> = entries.filter(|entry| entry.ticked).map(|entry| entry.size).sum();
        Some(ticked? + self.rest?)
    }

    /// Copy to the destination chosen, unless the Project cannot be
    /// copied or the destination is no folder to copy into. The copy finds
    /// for itself whether it resumes one, or meets what it did not put
    /// there.
    fn start(&mut self, cx: &mut Context<Self>) {
        if !self.choosing(cx) || self.entries.is_none() {
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
        let resumes = self.resumes(cx);
        self.let_go(cx);
        let order = Order {
            placing: self.placing.clone(),
            target: self.target(cx),
            dest,
            skip: self.kept_here(),
            resumes,
            home: self.dest.read(cx).home(),
        };
        self.browsing = false;
        self.copies.update(cx, |copies, cx| copies.start(order, cx));
        cx.notify();
    }

    /// Stop the copy under way; what it sent stays, for its Resume.
    fn stop(&mut self, cx: &mut Context<Self>) {
        let key = self.key();
        self.copies.update(cx, |copies, cx| copies.cancel(key, cx));
        // The copy may have made its folder by now.
        self.dest.update(cx, |dest, cx| dest.relist(cx));
    }

    /// The close button, esc, a press outside the card and Continue in
    /// background: the card goes, and a copy it shows carries on. Only
    /// Cancel stops one.
    pub(super) fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(PlacementEvent::Closed);
    }

    /// ↵: the footer's own verb.
    pub(super) fn confirm(&mut self, cx: &mut Context<Self>) {
        match self.face(cx) {
            Face::Choose => self.primary(cx),
            Face::Progress | Face::Result => self.dismiss(cx),
        }
    }

    /// The primary button while choosing: what is there used, or a copy.
    fn primary(&mut self, cx: &mut Context<Self>) {
        match self.copy(cx).map(|copy| &copy.phase) {
            Some(Phase::Occupied { of: None }) => cx.emit(PlacementEvent::Use),
            Some(Phase::Occupied { .. }) => {}
            _ => self.start(cx),
        }
    }

    /// Tab in the browser: go into the first folder its last name starts.
    /// False when Tab moves on.
    pub(super) fn tab(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        self.browsing && self.dest.update(cx, |dest, cx| dest.tab(window, cx))
    }

    pub(super) fn poll(&mut self, cx: &mut Context<Self>) {
        self.dest.update(cx, |dest, cx| dest.poll(cx));
        if self.listing.is_none() && self.entries.is_none() && self.choosing(cx) {
            self.list_project();
        }
        let moved = self.poll_listing();
        self.open_on_unfinished(cx);
        self.browse_if_unsure(cx);
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
                    if self.kept_back.is_none() {
                        self.opening = unfinished.last().cloned();
                    }
                    for path in unfinished {
                        if !self.unfinished.contains(&path) {
                            self.unfinished.push(path);
                        }
                    }
                }
                Ok(Listed::Read {
                    unfit,
                    ignored,
                    worktrees,
                }) => {
                    self.unfit = unfit.map(SharedString::from);
                    self.worktrees = worktrees;
                    let kept_back = self.kept_back.take();
                    self.entries = Some(
                        ignored
                            .into_iter()
                            .map(|ignored| Entry {
                                ticked: kept_back
                                    .as_ref()
                                    .is_some_and(|kept| !kept.contains(&ignored.path)),
                                ignored,
                                size: None,
                            })
                            .collect(),
                    );
                }
                Ok(Listed::Rest(size)) => self.rest = Some(size),
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

    /// The card, for the sheet host: as wide as its face, and as tall as
    /// what it shows, easing between the two as the face changes.
    pub(super) fn card(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ferrite_ui::sheet::Drawn> {
        let face = self.face(cx);
        let band_top = self.band.as_ref().and_then(Band::top);
        let (room, lift) = project_editor::room(f32::from(window.viewport_size().height), band_top);
        let width = match face {
            Face::Choose => project_editor::WIDTH,
            Face::Progress | Face::Result => project_editor::COMPACT_WIDTH,
        };
        let (width, height) = self.size.frame(width, room, window, cx);
        let (head, content, footer) = match face {
            Face::Choose => self.choose(window, cx),
            Face::Progress => self.progress(cx),
            Face::Result => self.result(cx),
        };
        let body = project_editor::body()
            .flex_grow(1.)
            .debug_selector(|| "placement-body".into())
            .track_scroll(&self.scroll)
            .p_0()
            .child(
                content
                    .relative()
                    // The content is its own height, which the card is
                    // measured by.
                    .flex_shrink_0()
                    .px(px(ferrite_ui::theme::MODAL_PAD))
                    .pt(px(ferrite_ui::theme::HALF_ROW))
                    .pb(px(ferrite_ui::theme::ROW))
                    .child(self.size.content_probe()),
            );
        let card = match height {
            Some(height) => ferrite_ui::sheet::sheet(width, height),
            None => ferrite_ui::sheet::sheet_fit(width, room),
        }
        .mb(px(lift))
        .relative()
        .id("placement-card")
        .debug_selector(|| "placement-card".into())
        .track_focus(&self.focus)
        .occlude()
        .child(self.size.card_probe())
        .child(head)
        .child(
            div()
                .relative()
                .flex()
                .flex_col()
                .flex_grow(1.)
                .flex_shrink(1.)
                .min_h_0()
                .child(self.size.body_probe())
                .child(ferrite_ui::motion::swap_in(("placement-face", face as usize), body))
                .child(ferrite_ui::components::scrollbar(
                    "placement-scrollbar",
                    &self.scroll,
                )),
        )
        .child(footer);
        Some(ferrite_ui::sheet::Drawn { card, above: None })
    }

    /// The head's close button: the card goes, the copy carries on.
    fn close(&self, cx: &mut Context<Self>) -> gpui::component::button::Button {
        ferrite_ui::sheet::sheet_close("placement-close", "Close", cx).on_click(cx.listener(
            |sheet: &mut Self, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                sheet.dismiss(cx);
            },
        ))
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

/// Where a copy goes, as the folder it goes in, `/` last, and its name.
fn split_dest(dest: &str) -> (String, String) {
    let dest = dest.trim_end_matches(['/', '\\']);
    match dest.rsplit_once(['/', '\\']) {
        Some((folder, name)) => (format!("{folder}/"), name.to_string()),
        None => (String::new(), dest.to_string()),
    }
}

/// A quiet line on the card.
fn note(text: impl Into<SharedString>) -> Div {
    ferrite_ui::components::text_ui()
        .flex_shrink_0()
        .text_color(gpui::rgb(ferrite_ui::theme::INK_SUBTLE))
        .child(text.into())
}
