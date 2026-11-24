//! A draft's environment chip (ADR 0048): which environment a new chat
//! runs on, a choice until its first send. A draft's machine is its
//! Project's (ADR 0045), so choosing another environment chooses the
//! Project's counterpart there (`placement::counterpart`), or makes one:
//! a Project of this computer is copied there by the setup sheet
//! (`PlacementView`), and a machine's Project gets the Project card there.

use super::menu_rows::Environment;
use super::placement_view::{PlacementEvent, PlacementView, Placing};
use super::*;
use ferrite_client::cockpit::ServerState;
use ferrite_core::machine::Failure;
use ferrite_core::placement::{self, Across, Placement};
use ferrite_core::workspace::Repositories;
use gpui::App;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError};

/// The band over a draft's Composer, where its Pane was last laid out: a
/// sheet opened for the draft keeps its card clear of it
/// (`project_editor::card_over`).
#[derive(Clone)]
pub(super) struct Band(Rc<Cell<pane::PaneGeometry>>);

impl Band {
    /// Where the band's top edge is, once the draft has been laid out.
    pub(super) fn top(&self) -> Option<f32> {
        let composer = self.0.get().composer?;
        Some(f32::from(composer.top()) - ferrite_ui::theme::ENV_STRIP_H)
    }
}

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
    /// Whether a draft can choose where it runs: more than one environment
    /// is on.
    pub(super) fn environment_chip_shown(&self, cx: &App) -> bool {
        self.cockpit.read(cx).machines().nth(1).is_some()
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

    /// The environment chip's rows for `draft`.
    pub(super) fn environment_menu_rows(&self, draft: &pane::DraftBinding, cx: &App) -> Vec<Row> {
        menu_rows::environment_rows(&self.environments(cx), draft)
    }

    /// The focused draft runs on `to` instead: its Project's counterpart
    /// there, once both machines say which repository each Project is.
    pub(super) fn pick_environment(&mut self, to: MachineId, cx: &mut Context<Self>) {
        let Some(pane) = self.panes.get(self.focused(cx)) else {
            return;
        };
        let Some(from) = pane.draft().map(|draft| draft.binding.project()) else {
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
            .and_then(|index| self.panes[index].draft())
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
        placement::counterpart(
            root,
            repository.map(String::as_str),
            across,
            registry.reading(),
            lookup.there.repositories(),
        )
        .map(|project| registry.project_key(project))
    }

    /// The band of the draft in Pane `draft`, while it is one.
    pub(super) fn band_of(&self, draft: PaneIdentity) -> Option<Band> {
        let pane = &self.panes[self.index_of(draft)?];
        pane.draft().map(|_| Band(pane.geometry.clone()))
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
        let Some(binding) = self.panes[index].draft_mut() else {
            return;
        };
        let before = binding.binding.project().machine();
        binding.binding.choose_project(project);
        binding.error = None;
        self.launch_project = Some(project);
        let machine = project.machine();
        if machine != before {
            let elsewhere = self.elsewhere(machine, cx);
            self.panes[index].preview.set_remote_files(elsewhere);
            let composer = self.panes[index].composer.clone();
            self.aim_attachments(&composer, machine, cx);
            self.upload_attachments(&composer, machine, cx);
        }
        cx.notify();
    }

    /// The setup sheet: copy the draft's Project, of this computer, to the
    /// machine picked.
    fn open_placement(&mut self, lookup: Lookup, cx: &mut Context<Self>) {
        let Some(project) = self.cockpit.read(cx).project(lookup.from) else {
            return;
        };
        let Some(kept) = self.machine_dir(lookup.to, cx) else {
            return;
        };
        let placing = Placing {
            draft: lookup.draft,
            title: project.title.clone().into(),
            root: project.root.clone(),
            machine: lookup.to,
            machine_name: self.environment_name(lookup.to, cx),
            projects_dir: self.projects_dir(lookup.to),
            kept,
        };
        let model = self.cockpit.clone();
        let band = self.band_of(lookup.draft);
        let sheet = cx.new(|cx| PlacementView::new(model, placing, band, cx));
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
        match event {
            PlacementEvent::Place { dest } => self.place_copy(&sheet, dest, cx),
            PlacementEvent::Closed => {
                self.hide_sheet(cx);
            }
        }
    }

    /// The sheet's folder is on its machine: register it there as a
    /// Project, and the window carries on while its Server answers. A
    /// Project made now takes the name of the one it copies, so the two read
    /// as one; one that was there, however its folder was spelled, keeps
    /// its own.
    fn place_copy(
        &mut self,
        sheet: &Entity<PlacementView>,
        dest: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        let placing = sheet.read(cx).placing.clone();
        let there: std::collections::HashSet<_> = self
            .cockpit
            .read(cx)
            .registry(placing.machine)
            .reading()
            .projects()
            .iter()
            .map(|project| project.id)
            .collect();
        let registered = self.cockpit.update(cx, |cockpit, _| {
            cockpit.register_project(placing.machine, dest)
        });
        let (sheet, dest) = (sheet.clone(), dest.to_path_buf());
        self.when_answered(
            registered,
            move |view, registered, cx| match registered {
                Ok(project) if !there.contains(&project.project()) => {
                    view.name_copy(sheet, placing, dest, project, cx)
                }
                registered => view.copy_registered(sheet, placing, dest, registered, cx),
            },
            cx,
        );
    }

    /// Name the copy's new Project as the Project it copies. Refused, it
    /// keeps its folder's name, and the copy goes on.
    fn name_copy(
        &mut self,
        sheet: Entity<PlacementView>,
        placing: Placing,
        dest: PathBuf,
        project: ProjectKey,
        cx: &mut Context<Self>,
    ) {
        let named = self
            .cockpit
            .update(cx, |cockpit, _| cockpit.rename_project(project, &placing.title));
        self.when_answered(
            named,
            move |view, named, cx| {
                if let Err(error) = named {
                    ferrite_core::log!("the copy of {} kept its folder's name: {error}", placing.title);
                }
                view.copy_registered(sheet, placing, dest, Ok(project), cx)
            },
            cx,
        );
    }

    /// The copy is a Project on its machine: keep where it came from, its
    /// counterpart whatever git says of either, and choose it on the
    /// draft. The sheet stays up only to list what it left.
    fn copy_registered(
        &mut self,
        sheet: Entity<PlacementView>,
        placing: Placing,
        dest: PathBuf,
        registered: std::io::Result<ProjectKey>,
        cx: &mut Context<Self>,
    ) {
        let project = match registered {
            Ok(project) => project,
            Err(error) => {
                sheet.update(cx, |sheet, cx| {
                    sheet.failed(format!("Not added: {error}"), cx)
                });
                return;
            }
        };
        let remote_root = self
            .cockpit
            .read(cx)
            .project(project)
            .map_or(dest, |placed| placed.root.clone());
        let placed = Placement {
            local_root: placing.root.clone(),
            remote_root,
        };
        if let Err(error) = placement::place(&placing.kept, placed) {
            ferrite_core::log!(
                "the placement of {} was not kept: {error}",
                placing.root.display()
            );
        }
        self.rebind_draft(placing.draft, project, cx);
        let stays = sheet.update(cx, |sheet, cx| sheet.placed(cx));
        if !stays && self.placement_sheet(cx).as_ref() == Some(&sheet) {
            self.hide_sheet(cx);
        }
    }
}
