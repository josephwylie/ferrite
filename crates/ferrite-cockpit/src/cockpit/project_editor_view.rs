//! The Project card in the window (ADR 0041, ADR 0042). The card is
//! `ferrite_settings::project_editor::ProjectEditorView`, the sheet the
//! host holds while it is up. It reads the Project it edits through a
//! `Source` the root makes over itself (`project_editor_inputs`); what
//! would change the Projects it says as a `ProjectEditorEvent`, which the
//! root answers here: registering, naming and removing Projects and their
//! directories on their Servers, and what the window shows after.

use super::*;
use ferrite_settings::project_editor::{ProjectEditorEvent, ProjectEditorInputs, ProjectEditorView};
use ferrite_ui::source::Source;
use gpui::App;
use std::path::PathBuf;

impl CockpitView {
    /// Everything the card draws from, for the card on `target`. It never
    /// reads the card: the card calls it while its own update is running.
    fn project_editor_inputs(&self, target: Option<ProjectKey>, cx: &App) -> ProjectEditorInputs {
        let project = target
            .and_then(|project| self.cockpit.read(cx).project(project))
            .map(|project| {
                (
                    project.title.clone(),
                    std::iter::once(project.root.clone())
                        .chain(project.additional_roots.iter().cloned())
                        .collect(),
                )
            });
        ProjectEditorInputs {
            project,
            in_use: target.is_some_and(|project| self.project_in_use(project, cx)),
        }
    }

    pub(super) fn on_project_editor_event(
        &mut self,
        card: Entity<ProjectEditorView>,
        event: &ProjectEditorEvent,
        cx: &mut Context<Self>,
    ) {
        match event.clone() {
            ProjectEditorEvent::Adopt(path) => self.adopt_editor_directory(path, cx),
            ProjectEditorEvent::RemoveDirectory(project, index) => {
                let removed = self
                    .cockpit
                    .update(cx, |cockpit, _| cockpit.remove_project_directory(project, index));
                self.directory_changed(card, removed, cx);
            }
            ProjectEditorEvent::RemoveProject(project) => {
                if self.project_in_use(project, cx) {
                    return;
                }
                let removed = self
                    .cockpit
                    .update(cx, |cockpit, _| cockpit.remove_project(project));
                self.when_answered(
                    removed,
                    move |view, removed, cx| {
                        match removed {
                            Err(error) => {
                                let error = Some(format!("not removed · {error}").into());
                                card.update(cx, |card, cx| card.set_error(error, cx));
                            }
                            // Only the card that asked is closed.
                            Ok(()) => {
                                if view.nav_filter == Some(project) {
                                    view.nav_filter = None;
                                }
                                view.nav_notice.clear();
                                if view.project_card(cx).as_ref() == Some(&card) {
                                    view.hide_sheet(cx);
                                }
                            }
                        }
                        cx.notify();
                    },
                    cx,
                );
            }
            ProjectEditorEvent::Confirm => self.confirm_project_editor(cx),
            ProjectEditorEvent::Closed => {
                self.hide_sheet(cx);
            }
        }
    }

    /// The Project card, while it is up.
    pub(crate) fn project_card(&self, cx: &App) -> Option<Entity<ProjectEditorView>> {
        self.sheets.read(cx).project_editor().cloned()
    }

    /// Open the card on an existing Project, seeded with its name.
    pub(crate) fn open_project_editor(&mut self, project: ProjectKey, cx: &mut Context<Self>) {
        let Some(title) = self
            .cockpit
            .read(cx)
            .project(project)
            .map(|project| project.title.clone())
        else {
            return;
        };
        self.show_project_editor(project.machine(), Some(project), Some(title), cx);
    }

    /// Open the card on a Project that does not exist yet, on the machine
    /// a new Thread starts on (`start_machine`).
    pub(super) fn open_project_creator(&mut self, cx: &mut Context<Self>) {
        if let Some(machine) = self.start_machine(cx) {
            self.open_project_creator_on(machine, cx);
        }
    }

    /// Open the card on a Project that does not exist yet on `machine`
    /// (the machine pick's row, `add_project`): an empty name and no
    /// directories. Nothing is registered until Create.
    pub(super) fn open_project_creator_on(
        &mut self,
        machine: ferrite_core::MachineId,
        cx: &mut Context<Self>,
    ) {
        self.show_project_editor(machine, None, None, cx);
    }

    /// Open the card on a Project that does not exist yet on `machine`, for
    /// the draft in Pane `draft`, which chooses it once it is created.
    pub(super) fn open_project_creator_for(
        &mut self,
        machine: ferrite_core::MachineId,
        draft: PaneIdentity,
        cx: &mut Context<Self>,
    ) {
        self.open_project_creator_on(machine, cx);
        let band = self.band_of(draft);
        if let Some(card) = self.project_card(cx) {
            card.update(cx, |card, _| {
                card.draft = Some(draft);
                card.band = band;
            });
        }
    }

    /// The card is modal: everything else floating goes down with it.
    fn show_project_editor(
        &mut self,
        machine: ferrite_core::MachineId,
        target: Option<ProjectKey>,
        title: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let inputs = Source::of(&cx.entity(), |root: &Self, target: &Option<ProjectKey>, cx| {
            Some(root.project_editor_inputs(*target, cx))
        });
        let browse = self.elsewhere(machine, cx).then(|| {
            let name = self.machine_label(machine, cx).unwrap_or_default();
            let list = super::machines::folders_on(self.cockpit.clone(), machine);
            (list, format!("Directory on {name}").into())
        });
        let card = cx.new(|cx| {
            ProjectEditorView::new(inputs, machine, target, title, browse, cx)
        });
        cx.subscribe(&card, Self::on_project_editor_event).detach();
        self.show_sheet(sheets::Sheet::ProjectEditor(card), cx);
        self.close_composer_menu(cx);
        cx.notify();
    }

    /// Commit the card. Creating registers the staged directories as one
    /// Project — the first is primary — and names it; editing only renames,
    /// because its directory edits already went through. A refusal keeps the
    /// card open with the reason on it, so nothing typed is lost.
    pub(super) fn confirm_project_editor(&mut self, cx: &mut Context<Self>) {
        let Some(card) = self.project_card(cx) else {
            return;
        };
        let editor = card.read(cx);
        if editor.committing {
            return;
        }
        let machine = editor.machine;
        let target = editor.target;
        let typed = editor.name.read(cx).text().to_string();
        let name = editor.resolved_name(&typed);
        let staged = editor.staged.clone();
        card.update(cx, |card, _| card.committing = true);
        match target {
            Some(project) => {
                let typed = typed.trim();
                if typed.is_empty() {
                    self.project_committed(card, Ok(project), cx);
                } else {
                    self.name_project(card, project, typed, cx);
                }
            }
            None => {
                let registered = self
                    .cockpit
                    .update(cx, |cockpit, _| cockpit.register_project_directories(machine, &staged));
                self.when_answered(
                    registered,
                    move |view, registered, cx| match (registered, name) {
                        (Ok(project), Some(name)) => view.name_project(card, project, &name, cx),
                        (registered, _) => view.project_committed(card, registered, cx),
                    },
                    cx,
                );
            }
        }
        cx.notify();
    }

    /// Name `project` as the card has it, the last act of committing it.
    fn name_project(
        &mut self,
        card: Entity<ProjectEditorView>,
        project: ProjectKey,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        let named = self
            .cockpit
            .update(cx, |cockpit, _| cockpit.rename_project(project, name));
        self.when_answered(
            named,
            move |view, named, cx| view.project_committed(card, named.map(|()| project), cx),
            cx,
        );
    }

    /// The Server answered the card's commit: the Project it describes
    /// is shown, or the card says why it is unchanged.
    fn project_committed(
        &mut self,
        card: Entity<ProjectEditorView>,
        outcome: std::io::Result<ProjectKey>,
        cx: &mut Context<Self>,
    ) {
        let draft = card.update(cx, |card, _| {
            card.committing = false;
            card.draft
        });
        match outcome {
            Ok(project) => {
                if self.project_card(cx).as_ref() == Some(&card) {
                    self.hide_sheet(cx);
                }
                // The Project the operator just described is the one they
                // want to look at, and to work in.
                self.launch_project = Some(project);
                self.choose_nav_filter(Some(project), cx);
                if self.empty_board_shown(cx) {
                    self.open_draft_with_provider(
                        DraftTarget::Main,
                        self.prefs.settings.default_provider,
                        cx,
                    );
                }
                if let Some(draft) = draft {
                    self.rebind_draft(draft, project, cx);
                }
                self.nav_notice.clear();
                self.refresh_names();
            }
            Err(error) => {
                let error = Some(format!("project unchanged · {error}").into());
                card.update(cx, |card, cx| card.set_error(error, cx));
            }
        }
        cx.notify();
    }

    /// A directory of the card's Project was asked added or removed: the
    /// card says a refusal when the answer comes.
    fn directory_changed(
        &mut self,
        card: Entity<ProjectEditorView>,
        changed: ferrite_client::cockpit::Ask<()>,
        cx: &mut Context<Self>,
    ) {
        self.when_answered(
            changed,
            move |_, changed, cx| {
                let refusal = changed
                    .err()
                    .map(|error| SharedString::from(format!("directory unchanged · {error}")));
                card.update(cx, |card, cx| card.set_error(refusal, cx));
                cx.notify();
            },
            cx,
        );
    }

    /// Land a picked directory on the card. On a new Project it is staged;
    /// on an existing one it is attached at once, so the card always shows
    /// the registry's own list.
    pub(super) fn adopt_editor_directory(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(card) = self.project_card(cx) else {
            return;
        };
        match card.read(cx).target {
            Some(project) => {
                let added = self
                    .cockpit
                    .update(cx, |cockpit, _| cockpit.add_project_directories(project, &[path]));
                self.directory_changed(card, added, cx);
            }
            None => card.update(cx, |card, cx| card.stage(path, cx)),
        }
        cx.notify();
    }
}
