//! A draft's environment chip (ADR 0048): which environment a new chat
//! runs on, a choice until its first send. A draft's machine is its
//! Project's (ADR 0045), so choosing another environment chooses the
//! Project's counterpart there (`placement::counterpart`), or makes one:
//! a Project of this computer is copied there (`Copies`, chosen on the
//! setup sheet, `PlacementView`), and a machine's Project gets the Project
//! card there.
//!
//! The band's other menus are here too: the Project and Workspace chips'
//! (`Band`), their rows, the repo and checkout reads they show, and a pick.

use super::copies::{self, words, CopyEvent, CopyKey};
use super::menu_rows::Environment;
use super::placement_view::{PlacementEvent, PlacementView, Placing};
use super::*;
use ferrite_client::cockpit::ServerState;
use ferrite_core::machine::Failure;
use ferrite_core::placement::{self, Across, Placement};
use ferrite_core::protocol::Repositories;
use ferrite_pane::band::Band as PaneBand;
use gpui::App;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

/// A pick of another environment for a draft, waiting for both machines
/// to say which repository each of their Projects is.
pub(super) struct Lookup {
    draft: PaneIdentity,
    from: ProjectKey,
    to: MachineId,
    here: Heard,
    there: Heard,
}

/// One machine's repositories: asked, or in hand.
enum Heard {
    Asking(Receiver<std::io::Result<Repositories>>),
    Answered(Repositories),
}

impl Heard {
    fn ask(cockpit: &mut Cockpit, machine: MachineId) -> Self {
        match cockpit.repositories(machine) {
            Ok(answer) => Heard::Asking(answer),
            Err(_) => Heard::Answered(Repositories::new()),
        }
    }

    /// Whether the answer is in hand. One refused, or lost with its link,
    /// knows no repositories: an older Server cannot say.
    fn landed(&mut self) -> bool {
        if let Heard::Asking(answer) = self {
            let repositories = match answer.try_recv() {
                Err(TryRecvError::Empty) => return false,
                Ok(Ok(repositories)) => repositories,
                Ok(Err(_)) | Err(TryRecvError::Disconnected) => Repositories::new(),
            };
            *self = Heard::Answered(repositories);
        }
        true
    }

    fn repositories(&self) -> &Repositories {
        static NONE: Repositories = Repositories::new();
        match self {
            Heard::Answered(repositories) => repositories,
            Heard::Asking(_) => &NONE,
        }
    }
}

/// Why the window cannot run a draft on a machine now, when it cannot.
fn unreached(server: Option<ServerState>) -> Option<SharedString> {
    let Some(ServerState::Detached { attention, .. }) = server else {
        return None;
    };
    Some(
        match attention {
            None => "connecting",
            Some(failure) if failure.needs_sign_in() => "sign in",
            Some(Failure::NotInstalled) => "not set up",
            Some(_) => "offline",
        }
        .into(),
    )
}

impl CockpitView {
    /// Whether the draft in Pane `index` can choose where it runs: more
    /// than one environment is on, or it waits on a copy, whose Machine
    /// may be off by now.
    pub(super) fn environment_chip_shown(&self, index: usize, cx: &App) -> bool {
        self.cockpit.read(cx).machines().nth(1).is_some() || self.waited_on(index, cx).is_some()
    }

    /// What the environment chip and its rows call `machine`.
    pub(super) fn environment_name(&self, machine: MachineId, cx: &App) -> SharedString {
        self.machine_label(machine, cx)
            .map(SharedString::from)
            .unwrap_or_else(|| "Local".into())
    }

    /// Every environment on, as Settings › Connections lists them.
    pub(super) fn environments(&self, cx: &App) -> Vec<Environment> {
        let Some(machines) = self.machines.as_ref() else {
            return Vec::new();
        };
        let cockpit = self.cockpit.read(cx);
        machines
            .rows(cockpit)
            .into_iter()
            .filter_map(|row| {
                let machine = row.held?;
                Some(Environment {
                    machine,
                    name: self.environment_name(machine, cx),
                    unreached: unreached(cockpit.server(machine)),
                })
            })
            .collect()
    }

    /// The copy the draft in Pane `index` waits on, under way or stopped
    /// short: the draft is sent there once it is in.
    pub(super) fn waited_on<'a>(&self, index: usize, cx: &'a App) -> Option<(&'a CopyKey, &'a copies::Entry)> {
        let pane = self.panes.get(index)?;
        let project = pane.read(cx).draft()?.binding.project();
        self.copies.read(cx).waited_on_by(pane.identity, project)
    }

    /// The environment chip's rows for `draft`, in Pane `pane`: while it
    /// waits on a copy, the copy's Machine is ticked and says how far or
    /// that the copy stopped, off or removed by now or not, and its own
    /// sends it here instead. Then every other copy the window keeps, under
    /// way, done or stopped, this Project's first: a copy is always a pick
    /// away, its toast closed and its draft gone.
    pub(super) fn environment_menu_rows(&self, pane: PaneIdentity, draft: &pane::DraftBinding, cx: &App) -> Vec<Row> {
        let copies = self.copies.read(cx);
        let project = draft.binding.project();
        let waited = copies.waited_on_by(pane, project).map(|(key, copy)| menu_rows::Waited {
            machine: self.cockpit.read(cx).machine_saved_as(&key.machine),
            name: copy.order.placing.machine_name.clone(),
            detail: match copy.phase.done() {
                _ if matches!(copy.phase, copies::Phase::Stopping { .. }) => "cancelling".into(),
                Some(done) if copy.phase.live() => format!("copying {}%", (done * 100.).floor()).into(),
                _ if copy.phase.live() => "copying".into(),
                _ => "copy stopped".into(),
            },
            copy: key.clone(),
        });
        let mut kept: Vec<_> = copies
            .iter()
            .filter(|(key, _)| waited.as_ref().is_none_or(|waited| waited.copy != **key))
            .map(|(key, copy)| {
                let (title, machine) = (&copy.order.placing.title, &copy.order.placing.machine_name);
                let (name, detail) = match (&copy.phase, copy.phase.done()) {
                    (copies::Phase::Done(_), _) => (format!("Copied {title} to {machine}"), "done".into()),
                    (copies::Phase::Stopping { .. }, _) => (format!("Copy of {title} to {machine}"), "cancelling".into()),
                    (phase, Some(done)) if phase.live() => {
                        (format!("Copying {title} to {machine}"), format!("{}%", (done * 100.).floor()))
                    }
                    (phase, None) if phase.live() => (format!("Copying {title} to {machine}"), "listing files".into()),
                    _ => (format!("Copy of {title} to {machine}"), "stopped".into()),
                };
                (key.project != project, menu_rows::KeptCopy {
                    name: name.into(),
                    detail: detail.into(),
                    copy: key.clone(),
                })
            })
            .collect();
        kept.sort_by_key(|(elsewhere, _)| *elsewhere);
        let kept = kept.into_iter().map(|(_, copy)| copy).collect();
        menu_rows::environment_rows(&self.environments(cx), draft, waited, kept)
    }

    /// The focused draft runs on `to` instead: its Project's counterpart
    /// there, once both machines say which repository each Project is.
    /// While it waits on a copy, the copy's Machine brings the copy's card
    /// back, and any other lets the draft go, the copy carrying on.
    pub(super) fn pick_environment(&mut self, to: MachineId, cx: &mut Context<Self>) {
        let index = self.focused(cx);
        let waited_on = self.waited_on(index, cx).map(|(key, _)| key.clone());
        if let Some(key) = waited_on {
            if self.cockpit.read(cx).saved_as(to) == Some(&key.machine) {
                self.show_copy(&key, cx);
                return;
            }
            self.copies.update(cx, |copies, cx| copies.let_draft_go(&key, cx));
        }
        let Some(pane) = self.panes.get(index) else {
            return;
        };
        let Some(from) = pane.read(cx).draft().map(|draft| draft.binding.project()) else {
            return;
        };
        if from.machine() == to {
            return;
        }
        let draft = pane.identity;
        let (here, there) = self.cockpit.update(cx, |cockpit, _| {
            (Heard::ask(cockpit, from.machine()), Heard::ask(cockpit, to))
        });
        self.environment_lookup = Some(Lookup {
            draft,
            from,
            to,
            here,
            there,
        });
        self.poll_environment_lookup(cx);
    }

    /// Answer an environment pick once both machines have said: the
    /// counterpart, else the setup sheet for a Project of this computer,
    /// else the Project card on the machine picked. A draft that went, or
    /// chose another Project meanwhile, is left as it is.
    pub(super) fn poll_environment_lookup(&mut self, cx: &mut Context<Self>) {
        let Some(lookup) = self.environment_lookup.as_mut() else {
            return;
        };
        let here = lookup.here.landed();
        if !(lookup.there.landed() && here) {
            return;
        }
        let Some(lookup) = self.environment_lookup.take() else {
            return;
        };
        let current = self
            .index_of(lookup.draft)
            .and_then(|index| self.panes[index].read(cx).draft())
            .map(|draft| draft.binding.project());
        if current != Some(lookup.from) {
            return;
        }
        match self.counterpart(&lookup, cx) {
            Some(project) => self.rebind_draft(lookup.draft, project, cx),
            None if !self.elsewhere(lookup.from.machine(), cx) => self.open_placement(lookup, cx),
            None => self.open_project_creator_for(lookup.to, lookup.draft, cx),
        }
        cx.notify();
    }

    /// The Project on the machine picked that is the counterpart of the
    /// draft's (`placement::counterpart`, the one lookup).
    fn counterpart(&self, lookup: &Lookup, cx: &App) -> Option<ProjectKey> {
        let cockpit = self.cockpit.read(cx);
        let root = &cockpit.project(lookup.from)?.root;
        let repository = lookup.here.repositories().get(&lookup.from.project());
        let (here, there) = (
            !self.elsewhere(lookup.from.machine(), cx),
            !self.elsewhere(lookup.to, cx),
        );
        let placements = match (here, there) {
            (true, false) => self.placements(lookup.to, cx),
            (false, true) => self.placements(lookup.from.machine(), cx),
            _ => Vec::new(),
        };
        let across = match (here, there) {
            (true, false) => Across::ToMachine(&placements),
            (false, true) => Across::FromMachine(&placements),
            _ => Across::Between,
        };
        let registry = cockpit.registry(lookup.to);
        let projects: Vec<_> = registry
            .reading()
            .projects()
            .iter()
            .map(|project| (project.id, project.root.clone()))
            .collect();
        placement::counterpart(
            root,
            repository.map(String::as_str),
            across,
            &projects,
            lookup.there.repositories(),
        )
        .map(|project| registry.project_key(project))
    }

    /// The band of the draft in Pane `draft`, while it is one.
    pub(super) fn band_of(&self, draft: PaneIdentity, cx: &gpui::App) -> Option<PaneBand> {
        let pane = &self.panes[self.index_of(draft)?];
        pane.read(cx).draft().map(|_| PaneBand::new(pane.read(cx).geometry.clone()))
    }

    /// Where `machine`'s own files are kept here; none for this computer.
    fn machine_dir(&self, machine: MachineId, cx: &App) -> Option<PathBuf> {
        let target = self.cockpit.read(cx).saved_as(machine)?.0.as_ref()?;
        Some(ferrite_core::machine::dir(&self.prefs.dir, target))
    }

    fn placements(&self, machine: MachineId, cx: &App) -> Vec<Placement> {
        self.machine_dir(machine, cx)
            .map(|dir| placement::placements(&dir))
            .unwrap_or_default()
    }

    /// Where a Project copied to `machine` goes unless the operator says
    /// otherwise: its projects folder, as Settings › Connections has it.
    fn projects_dir(&self, machine: MachineId) -> String {
        self.machines
            .as_ref()
            .and_then(|machines| machines.machine(machines.listed_as(machine)?))
            .map(|saved| saved.projects_dir.clone())
            .unwrap_or_else(|| ferrite_core::machine::PROJECTS_DIR.into())
    }

    /// The draft in Pane `draft` works in `project` now: its text, files,
    /// provider and effort stay, and on another machine its files are aimed
    /// there and its previews read there. The next draft starts there too.
    pub(super) fn rebind_draft(
        &mut self,
        draft: PaneIdentity,
        project: ProjectKey,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.index_of(draft) else {
            return;
        };
        let Some(before) = self.panes[index].update_draft(cx, |binding| {
            let before = binding.binding.project().machine();
            binding.binding.choose_project(project);
            binding.error = None;
            before
        }) else {
            return;
        };
        self.launch_project = Some(project);
        let machine = project.machine();
        if machine != before {
            let elsewhere = self.elsewhere(machine, cx);
            self.panes[index].read(cx).preview.set_remote_files(elsewhere);
            let composer = self.panes[index].read(cx).composer.clone();
            self.aim_attachments(&composer, machine, cx);
            self.upload_attachments(&composer, machine, cx);
        }
        cx.notify();
    }

    /// Ask about each draft's checkout on this computer that git has not
    /// named and is not being asked about, as the window hears its binding
    /// change (the root's notify), so the Workspace menu opens already
    /// answered. Never from a frame; a named checkout is read again by the
    /// sweep (`refresh_branches`), a machine's by its Server.
    pub(super) fn ask_draft_checkouts(&self, cx: &Context<Self>) {
        let unnamed: Vec<ProjectKey> = self
            .panes
            .iter()
            .filter_map(|pane| pane.read(cx).draft())
            .filter(|draft| matches!(draft.binding.target(), DraftTarget::Main))
            .map(|draft| draft.binding.project())
            .filter(|project| self.checkouts.branch(*project).is_none() && !self.checkouts.asking(*project))
            .filter(|project| !self.elsewhere(project.machine(), cx))
            .collect();
        self.look_up_checkouts(unnamed, cx);
    }

    /// The setup sheet: copy the draft's Project, of this computer, to the
    /// machine picked; or the copy there under way, one at a time.
    fn open_placement(&mut self, lookup: Lookup, cx: &mut Context<Self>) {
        let Some(project) = self.cockpit.read(cx).project(lookup.from) else {
            return;
        };
        let Some(kept) = self.machine_dir(lookup.to, cx) else {
            return;
        };
        let Some(saved_as) = self.cockpit.read(cx).saved_as(lookup.to).cloned() else {
            return;
        };
        let placing = Placing {
            draft: lookup.draft,
            project: lookup.from,
            title: project.title.clone().into(),
            root: project.root.clone(),
            machine: lookup.to,
            saved_as,
            machine_name: self.environment_name(lookup.to, cx),
            projects_dir: self.projects_dir(lookup.to),
            kept,
        };
        self.open_placing(placing, cx);
    }

    /// The card on the copy of `key`, the copy as it began, on its Machine
    /// as the window holds it now.
    pub(super) fn show_copy(&mut self, key: &CopyKey, cx: &mut Context<Self>) {
        if self.placement_sheet(cx).is_some_and(|sheet| sheet.read(cx).key() == *key) {
            return;
        }
        let Some(mut placing) = self.copies.read(cx).get(key).map(|copy| copy.order.placing.clone()) else {
            return;
        };
        if let Some(machine) = self.cockpit.read(cx).machine_saved_as(&key.machine) {
            placing.machine = machine;
        }
        self.open_placing(placing, cx);
    }

    fn open_placing(&mut self, placing: Placing, cx: &mut Context<Self>) {
        let (model, copies) = (self.cockpit.clone(), self.copies.clone());
        let band = self.band_of(placing.draft, cx);
        let sheet = cx.new(|cx| PlacementView::new(model, copies, placing, band, cx));
        cx.subscribe(&sheet, Self::on_placement_event).detach();
        self.show_sheet(sheets::Sheet::Placement(sheet), cx);
    }

    /// The setup sheet, while it is up.
    pub(super) fn placement_sheet(&self, cx: &App) -> Option<Entity<PlacementView>> {
        self.sheets.read(cx).placement().cloned()
    }

    fn on_placement_event(
        &mut self,
        sheet: Entity<PlacementView>,
        event: &PlacementEvent,
        cx: &mut Context<Self>,
    ) {
        let key = sheet.read(cx).key();
        match event {
            PlacementEvent::Closed => self.placement_closed(&sheet, cx),
            PlacementEvent::Use => {
                if self.copies.update(cx, |copies, cx| copies.use_as_it_is(&key, cx)) {
                    self.place_copy(&key, cx);
                }
            }
            PlacementEvent::Add => self.copy_again(&key, cx),
            PlacementEvent::Chat(project) => self.chat_on_copy(&key, *project, cx),
        }
    }

    /// The card went: how the copy it showed ended has been seen. A copy
    /// done that its draft did not move to stays a moment more while its
    /// toast still offers a chat there.
    pub(super) fn placement_closed(&mut self, sheet: &Entity<PlacementView>, cx: &mut Context<Self>) {
        let key = sheet.read(cx).key();
        let copies = self.copies.read(cx);
        let offers_chat = !copies.lapsed(&key)
            && matches!(
                copies.get(&key).map(|copy| &copy.phase),
                Some(copies::Phase::Done(summary)) if !summary.moved
            );
        if !offers_chat {
            self.copies.update(cx, |copies, cx| copies.forget(&key, cx));
        }
        self.hide_sheet(cx);
    }

    /// Start a chat on `project`, the copy of `key` on its Machine, from
    /// its card or its toast: the copy has been seen.
    pub(super) fn chat_on_copy(&mut self, key: &CopyKey, project: ProjectKey, cx: &mut Context<Self>) {
        if self.placement_sheet(cx).is_some_and(|sheet| sheet.read(cx).key() == *key) {
            self.hide_sheet(cx);
        }
        self.open_draft_in_project(project, cx);
        self.copies.update(cx, |copies, cx| copies.forget(key, cx));
    }

    /// The copy of `key` is on its machine: register it there as a
    /// Project, and the window carries on while its Server answers. A
    /// Project made now takes the name of the one it copies, so the two read
    /// as one; one that was there, however its folder was spelled, keeps
    /// its own.
    pub(super) fn place_copy(&mut self, key: &CopyKey, cx: &mut Context<Self>) {
        let Some(order) = self.copies.read(cx).get(key).map(|copy| copy.order.clone()) else {
            return;
        };
        let Some(machine) = self.cockpit.read(cx).machine_saved_as(&key.machine) else {
            let reason = copies::turned_off(&order.placing.machine_name);
            self.copies.update(cx, |copies, cx| copies.unadded(key, reason, true, cx));
            self.tell_copy(key, cx);
            return;
        };
        let (key, title, dest) = (key.clone(), order.placing.title, order.target);
        let there: std::collections::HashSet<_> = self
            .cockpit
            .read(cx)
            .registry(machine)
            .reading()
            .projects()
            .iter()
            .map(|project| project.id)
            .collect();
        let registered = self
            .cockpit
            .update(cx, |cockpit, _| cockpit.register_project(machine, &dest));
        self.when_answered(
            registered,
            move |view, registered, cx| match registered {
                Ok(project) if !there.contains(&project.project()) => {
                    view.name_copy(key, title, dest, project, cx)
                }
                registered => view.copy_registered(&key, dest, registered, cx),
            },
            cx,
        );
    }

    /// Name the copy's new Project as the Project it copies. Refused, it
    /// keeps its folder's name, and the copy goes on.
    fn name_copy(
        &mut self,
        key: CopyKey,
        title: SharedString,
        dest: PathBuf,
        project: ProjectKey,
        cx: &mut Context<Self>,
    ) {
        let named = self
            .cockpit
            .update(cx, |cockpit, _| cockpit.rename_project(project, &title));
        self.when_answered(
            named,
            move |view, named, cx| {
                if let Err(error) = named {
                    ferrite_core::log!("the copy of {title} kept its folder's name: {error}");
                }
                view.copy_registered(&key, dest, Ok(project), cx)
            },
            cx,
        );
    }

    /// The copy is a Project on its machine: keep where it came from, its
    /// counterpart whatever git says of either, and choose it on the draft
    /// that asked for it, while that waits on it, an unsent draft on the
    /// Project copied. An answer for a copy no longer being added (its
    /// Machine went meanwhile) counts for nothing.
    fn copy_registered(
        &mut self,
        key: &CopyKey,
        dest: PathBuf,
        registered: std::io::Result<ProjectKey>,
        cx: &mut Context<Self>,
    ) {
        if !self.copies.read(cx).finishing(key) {
            return;
        }
        let Some(order) = self.copies.read(cx).get(key).map(|copy| copy.order.clone()) else {
            return;
        };
        let placing = order.placing;
        let project = match registered {
            Ok(project) => project,
            Err(error) => {
                let machine = &placing.machine_name;
                let (reason, when_back) = match self.cockpit.read(cx).server(placing.machine) {
                    None => (copies::turned_off(machine), true),
                    Some(ServerState::Attached { .. }) => (
                        format!(
                            "Copied, but not added on {machine}: {}.",
                            copies::words::plainly(&error.to_string())
                        ),
                        false,
                    ),
                    Some(_) => (format!("The connection to {machine} was lost."), true),
                };
                self.copies.update(cx, |copies, cx| copies.unadded(key, reason, when_back, cx));
                self.tell_copy(key, cx);
                return;
            }
        };
        let remote_root = self
            .cockpit
            .read(cx)
            .project(project)
            .map_or_else(|| dest.clone(), |placed| placed.root.clone());
        let placed = Placement {
            local_root: placing.root.clone(),
            remote_root,
        };
        if let Err(error) = placement::place(&placing.kept, placed) {
            ferrite_core::log!("the placement of {} was not kept: {error}", placing.root.display());
            let reason = format!(
                "Copied to {}, but this computer could not keep where it went: {}.",
                placing.machine_name,
                copies::words::plainly(&error.to_string())
            );
            self.copies.update(cx, |copies, cx| copies.unadded(key, reason, false, cx));
            self.tell_copy(key, cx);
            return;
        }
        // Kept unfinished until now, so a copy never added is resumed.
        if let Err(error) = placement::added_there(&placing.kept, &placing.root, &dest) {
            ferrite_core::log!("the copy of {} is still kept as unfinished: {error}", placing.root.display());
        }
        let moves = order.waits
            && self
                .index_of(placing.draft)
                .and_then(|index| self.panes[index].read(cx).draft())
                .is_some_and(|draft| draft.binding.project() == key.project);
        if moves {
            self.rebind_draft(placing.draft, project, cx);
        }
        if self.copies.update(cx, |copies, cx| copies.done(key, project, moves, cx)) {
            self.tell_copy(key, cx);
        } else if self.placement_sheet(cx).is_some_and(|sheet| sheet.read(cx).key() == *key) {
            // Used as it was: nothing was copied, so the card has nothing
            // to show.
            self.hide_sheet(cx);
        }
    }

    /// The window's copies, drawn again as they move, an environment menu
    /// open with them, and doing what their toasts ask.
    pub(super) fn copies_of_window(cockpit: Entity<Cockpit>, cx: &mut Context<Self>) -> Entity<copies::Copies> {
        let copies = cx.new(|_| copies::Copies::new(cockpit));
        cx.observe(&copies, |view, _, cx| {
            view.environment_menu_follows(cx);
            cx.notify()
        })
        .detach();
        cx.subscribe(&copies, |view, _, event, cx| match event {
            CopyEvent::Show(key) => view.show_copy(key, cx),
            CopyEvent::Again(key) => view.copy_again(key, cx),
            CopyEvent::Chat(key, project) => view.chat_on_copy(key, *project, cx),
        })
        .detach();
        copies
    }

    /// An environment menu open reads the copies as they are now: the row
    /// of each says what its chip and toast say.
    fn environment_menu_follows(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self.band_menu().filter(|open| open.kind.chip == pane::BandChip::Environment) else {
            return;
        };
        let pane = open.pane;
        let Some(draft) = self.panes.iter().find(|at| at.identity == pane).and_then(|at| at.read(cx).draft()) else {
            return;
        };
        let rows = self.environment_menu_rows(pane, draft, cx);
        if let Some(open) = self.band_menu_mut() {
            let chosen = open.rows.get(open.selected).map(|row| row.consequence.clone());
            open.selected = chosen
                .and_then(|chosen| rows.iter().position(|row| row.consequence == chosen))
                .unwrap_or(0);
            open.rows = rows;
        }
    }

    /// Hear the copies, and do what each that moved asks of the window. A
    /// copy whose draft's Pane closed waits for no draft.
    pub(super) fn poll_copies(&mut self, cx: &mut Context<Self>) {
        let open: Vec<PaneIdentity> = self.panes.iter().map(|pane| pane.identity).collect();
        self.copies
            .update(cx, |copies, cx| copies.drafts_closed(|draft| !open.contains(&draft), cx));
        let landed = self.copies.update(cx, |copies, cx| copies.poll(cx));
        for landed in landed {
            self.copy_landed(landed, cx);
        }
    }

    /// Do what a copy that moved asks of the window.
    fn copy_landed(&mut self, landed: copies::Landed, cx: &mut Context<Self>) {
        match landed {
            copies::Landed::Sent(key) => self.place_copy(&key, cx),
            copies::Landed::Ended(key) => self.tell_copy(&key, cx),
        }
    }

    /// The copy of `key` again (`Copies::again`): one whose files all
    /// landed is added at once.
    pub(super) fn copy_again(&mut self, key: &CopyKey, cx: &mut Context<Self>) {
        if let Some(landed) = self.copies.update(cx, |copies, cx| copies.again(key, cx)) {
            self.copy_landed(landed, cx);
        }
    }

    /// The copy of `key` ended: the chime, and with the window in the
    /// background, the system's notification, card or no card.
    pub(super) fn tell_copy(&mut self, key: &CopyKey, cx: &mut Context<Self>) {
        let Some(copy) = self.copies.read(cx).get(key) else {
            return;
        };
        let done = matches!(copy.phase, copies::Phase::Done(_));
        let detail = match &copy.phase {
            copies::Phase::Done(summary) => match summary.skipped.entries {
                0 => format!("{} in {}", words::bytes(summary.bytes), words::took(summary.took)),
                entries => copies::not_copied(entries),
            },
            _ => copy.why().unwrap_or_default(),
        };
        let title = format!("Copy of {} to {}", copy.order.placing.title, copy.order.placing.machine_name);
        if self.prefs.settings.sounds {
            self.chime(if done { Sound::Done } else { Sound::NeedsYou });
        }
        let window_active = self.cockpit.read(cx).window_active();
        if self.prefs.settings.system_notifications && crate::system_notifications::wanted(true, window_active) {
            let posting = ferrite_nav::notifications::Posting {
                tag: key.tag(),
                needs_you: !done,
                word: if done { "done" } else { "failed" },
                title,
                detail,
                agent: None,
                cwd: None,
            };
            crate::system_notifications::post(posting, cx);
            self.copies.update(cx, |copies, _| copies.posted(key));
        }
    }

    /// A click on a copy's system notification: its card.
    pub(super) fn open_copy_notification(&mut self, tag: &str, cx: &mut Context<Self>) -> bool {
        let Some(key) = self.copies.read(cx).tagged(tag) else {
            return false;
        };
        self.show_copy(&key, cx);
        true
    }

    /// Each copy's toast as it stands (`Copies::present`), none for the
    /// copy on the card up.
    pub(super) fn present_copies(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let carded = self.placement_sheet(cx).map(|sheet| sheet.read(cx).key());
        let window_active = self.cockpit.read(cx).window_active();
        self.copies
            .update(cx, |copies, cx| copies.present(carded, window_active, window, cx));
    }

    /// The environment chip of the draft in Pane `index` while it waits
    /// on a copy: the copy's Machine, with how far while it runs, alone
    /// once it stopped.
    pub(super) fn copy_chip(&self, index: usize, cx: &App) -> Option<(pane::ChipMark, SharedString)> {
        let (_, copy) = self.waited_on(index, cx)?;
        let name = copy.order.placing.machine_name.clone();
        Some(match (copy.phase.done(), copy.phase.live()) {
            (Some(done), true) => (pane::ChipMark::Ring(done), format!("{name} {}%", (done * 100.).floor()).into()),
            (None, true) => (pane::ChipMark::Ring(0.), name),
            (_, false) => (pane::ChipMark::Icon(ferrite_ui::icons::SERVER), name),
        })
    }
}

/// #29: a draft's Project or Workspace menu, a popup (`Popup::Band`): one
/// band chip's choices — registry reads, never a filesystem scan — except
/// the Project chip's type-a-path row, which re-derives from the Composer
/// line per edit.
pub(super) struct Band {
    pub(super) chip: pane::BandChip,
    /// Opened from the start hero's project word (`draft_hero`): it drops
    /// below the word rather than standing over the strip's chip.
    pub(super) from_hero: bool,
}

impl Band {
    /// The key the control this menu hangs from records its bounds under.
    pub(super) fn trigger(&self) -> SharedString {
        if self.from_hero {
            HERO_TRIGGER.into()
        } else {
            band_trigger(self.chip)
        }
    }
}

impl CockpitView {
    pub(super) fn band_cycle(&mut self, _: &BandCycle, window: &mut Window, cx: &mut Context<Self>) {
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
            .and_then(|pane| pane.read(cx).draft())
            .is_some_and(|draft| draft.band_focus.is_some());
        let composer = pane.map(|pane| pane.read(cx).composer.clone()).filter(|_| !on_band);
        if composer.is_some_and(|composer| {
            composer.update(cx, |composer, cx| composer.cycle_files(false, window, cx))
        }) {
            return;
        }
        let environment = self.environment_chip_shown(self.focused(cx), cx);
        let tabbed = self.update_focused_draft(cx, |draft| {
            draft.band_focus = pane::BandChip::next(draft.band_focus, environment);
        });
        if tabbed.is_none() {
            self.cycle_tools(false, window, cx);
            return;
        }
        // The menu belongs to the chip it opened from; tab moves on.
        if self.band_menu().is_some() {
            self.close_popup(cx);
        }
        cx.notify();
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
        let Some(draft) = pane.read(cx).draft() else {
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
    pub(super) fn refresh_repo_listing(&mut self, project: ProjectKey, cx: &mut Context<Self>) {
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
    pub(super) fn sync_workspace_rows(&mut self, cx: &mut Context<Self>) {
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
            .and_then(|pane| pane.read(cx).draft())
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

    /// What `project`'s checkout is on, as git last answered, for the
    /// Workspace menu's first row. A Project not named yet is asked about
    /// off the UI thread and reads `main` until the answer lands — the
    /// name a Project git cannot answer for keeps, the branch a fresh
    /// clone would be sitting on.
    pub(super) fn checked_out_branch(&self, project: ProjectKey, cx: &Context<Self>) -> SharedString {
        if let Some(branch) = self.checkouts.branch(project) {
            return branch;
        }
        self.look_up_checkouts([project], cx);
        SharedString::from(MAIN_BRANCH)
    }

    /// Ask, off the UI thread, what each of `projects`' checkouts is on:
    /// git only for one whose `HEAD` moved since it last named the branch
    /// (`git_read::head_of`). It only starts the lookup, once per Project
    /// at a time, and `checkouts_read` takes the answers. Never of another
    /// machine, whose checkouts no git here can read.
    pub(super) fn look_up_checkouts(&self, projects: impl IntoIterator<Item = ProjectKey>, cx: &Context<Self>) {
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
    pub(super) fn checkouts_read(&mut self, answers: Vec<checkouts::Looked>, cx: &mut Context<Self>) {
        if self.checkouts.listed(answers) {
            self.sync_workspace_rows(cx);
            cx.notify();
        }
    }

    /// The project chip's rows for `draft` (`menu_rows::project_rows`):
    /// every machine's Projects, with the focused Composer's line for the
    /// type-a-path row.
    pub(super) fn project_menu_rows(&self, draft: &pane::DraftBinding, cx: &Context<Self>) -> Vec<Row> {
        let typed = self
            .panes
            .get(self.focused(cx))
            .map(|pane| pane.read(cx).composer.read(cx).text().trim().to_string())
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
    pub(super) fn workspace_menu_rows(&self, draft: &pane::DraftBinding, cx: &Context<Self>) -> Vec<Row> {
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
    pub(super) fn sync_band_rows(&mut self, cx: &mut Context<Self>) {
        let open = self
            .band_menu()
            .is_some_and(|open| open.kind.chip == pane::BandChip::Project);
        if !open {
            return;
        }
        let Some(draft) = self.panes.get(self.focused(cx)).and_then(|pane| pane.read(cx).draft()) else {
            return;
        };
        let rows = self.project_menu_rows(draft, cx);
        if let Some(open) = self.band_menu_mut() {
            open.selected = open.selected.min(rows.len().saturating_sub(1));
            open.rows = rows;
        }
    }

    /// A band row's pick, applied to the focused draft. Changing the
    /// project resets the workspace chip to `main` — the old choice named
    /// another repo's rows.
    pub(super) fn pick_band(
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
                    .pane_machine(self.focused(cx), cx)
                    .map(|machine| self.cockpit.read(cx).announced_models(machine, provider.provider))
                    .unwrap_or_default();
                self.update_focused_draft(cx, |draft| {
                    draft.binding.choose_provider(provider.clone(), &announced);
                    draft.error = None;
                });
            }
            BandChoice::Effort(effort) => {
                self.update_focused_draft(cx, |draft| {
                    draft.binding.choose_effort(effort.clone());
                    draft.error = None;
                });
            }
            BandChoice::Project(project) => {
                if let Some(draft) = self.panes.get(self.focused(cx)).map(|pane| pane.identity) {
                    self.rebind_draft(draft, *project, cx);
                }
            }
            BandChoice::RegisterPath(path) => {
                let focused = self.focused(cx);
                if let (Some(machine), Some(pane)) = (
                    self.pane_machine(focused, cx),
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
                            let Some(index) = view
                                .index_of(pane)
                                .filter(|index| view.panes[*index].read(cx).draft().is_some())
                            else {
                                return;
                            };
                            match registered {
                                Ok(project) => {
                                    view.panes[index].update_draft(cx, |draft| {
                                        draft.binding.choose_checkout(project);
                                        draft.error = None;
                                    });
                                    // The typed path was the pick's input,
                                    // not a prompt: the line clears for one.
                                    composer.update(cx, |composer, cx| {
                                        let whole = 0..composer.text().len();
                                        composer.splice(whole, "", cx);
                                    });
                                }
                                Err(e) => {
                                    let error = SharedString::from(format!(
                                        "cannot register {}: {e}",
                                        path.display()
                                    ));
                                    view.panes[index].update_draft(cx, |draft| draft.error = Some(error));
                                }
                            }
                            cx.notify();
                        },
                        cx,
                    );
                }
            }
            BandChoice::Target(target) => {
                self.update_focused_draft(cx, |draft| {
                    draft.binding.choose_target(target.clone());
                    draft.error = None;
                });
            }
            BandChoice::AddProject => {
                let focused = self.panes.get(self.focused(cx));
                let draft = focused.filter(|pane| pane.read(cx).draft().is_some()).map(|pane| pane.identity);
                match (draft, self.pane_machine(self.focused(cx), cx)) {
                    (Some(draft), Some(machine)) => self.open_project_creator_for(machine, draft, cx),
                    _ => self.open_project_creator(cx),
                }
            }
            BandChoice::Environment(machine) => self.pick_environment(*machine, cx),
            BandChoice::Copy(key) => self.show_copy(key, cx),
            BandChoice::Connections => self.open_connections(cx),
        }
        cx.notify();
    }

    /// A draft's Project or Workspace menu, hung off its chip on the strip
    /// over the box (ADR 0014): its left edge at the chip's, its foot
    /// `FLOAT_OFFSET` over the chip — or, opened from the hero's project
    /// word, `FLOAT_OFFSET` under it — kept inside the window's margin. The chip has been
    /// drawn before it can be pressed, so its bounds are always recorded.
    pub(super) fn band_menu_element(
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
}
