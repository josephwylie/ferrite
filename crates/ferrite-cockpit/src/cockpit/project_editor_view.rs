//! The Project card as a view of its own (ADR 0041), made when it opens and
//! dropped once it has gone (ADR 0042), so the view is the card. It owns
//! what only the card reads: its name line, a new Project's directories,
//! its refusal, its focus, and on a machine the browser of its folders. It
//! draws the Project it edits from what the Cockpit gives it
//! (`ProjectEditorInputs`), asked for each frame, so the Cockpit's own
//! notify still redraws it; and whatever would change the Projects it says
//! as a `ProjectEditorEvent`, which the Cockpit answers once the card's
//! own update has ended.

use super::folder_browser::FolderBrowser;
use super::*;
use gpui::{App, WeakEntity};
use std::path::PathBuf;

/// The Project card. One surface serves both verbs: `target` names the
/// Project being edited, or `None` while one is being created.
///
/// `staged` is the create card's directory list — a Project does not exist
/// until it is confirmed, so its directories live here until then. The edit
/// card writes through to the registry instead and leaves `staged` empty.
pub(crate) struct ProjectEditorView {
    cockpit: WeakEntity<CockpitView>,
    /// The machine the card's Project is on, or is created on.
    pub(super) machine: ferrite_core::MachineId,
    pub(super) target: Option<ProjectKey>,
    /// The name line. Empty means "call it after the main directory".
    pub(super) name: Entity<ferrite_pane::composer::Composer>,
    pub(super) staged: Vec<PathBuf>,
    /// A refusal, shown on the card that caused it.
    pub(super) error: Option<SharedString>,
    /// The card was committed and its Server has not answered: a second
    /// press commits nothing.
    pub(super) committing: bool,
    /// The body's scroll, for its overlay scrollbar.
    scroll: ScrollHandle,
    /// On a machine, the browser of its folders that stands in for the
    /// folder dialog.
    pub(super) remote: Option<Entity<FolderBrowser>>,
    /// The draft the card was opened for: the Project it creates is chosen
    /// on it.
    pub(super) draft: Option<PaneIdentity>,
    pub(super) focus: FocusHandle,
}

/// Where the browser opens: the machine's home, which its Server spells
/// out.
const REMOTE_HOME: &str = "~/";

/// What the card draws from, as the Cockpit has it this frame.
pub(crate) struct ProjectEditorInputs {
    /// The Project being edited, as the registry has it: its title and its
    /// directories, the main one first. `None` while creating, or once it
    /// is gone.
    project: Option<(String, Vec<PathBuf>)>,
    /// Some Thread, open or parked, records the Project being edited.
    in_use: bool,
}

/// What the card asks of the window.
#[derive(Clone)]
pub(crate) enum ProjectEditorEvent {
    /// A directory picked or typed for the card (`adopt_editor_directory`).
    Adopt(PathBuf),
    /// Remove an existing Project's directory, by its place in the list.
    RemoveDirectory(ProjectKey, usize),
    RemoveProject(ProjectKey),
    /// Create or Done, pressed while the card can commit.
    Confirm,
    /// The close button closed it.
    Closed,
}

impl EventEmitter<ProjectEditorEvent> for ProjectEditorView {}

impl ProjectEditorView {
    /// A card on `target` (`None` creates on `machine`), its name line
    /// seeded with `title`, and on another machine, `browse`, a browser of
    /// its folders labelled for it in place of the folder dialog.
    pub(super) fn new(
        cockpit: WeakEntity<CockpitView>,
        machine: ferrite_core::MachineId,
        target: Option<ProjectKey>,
        title: Option<String>,
        browse: Option<(Entity<Cockpit>, SharedString)>,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(ferrite_pane::composer::Composer::new);
        name.update(cx, |name, cx| {
            name.set_role(ferrite_pane::composer::Role::Field(None), cx);
            if let Some(title) = title {
                name.set(title, cx)
            }
        });
        let remote = browse.map(|(model, label)| {
            cx.new(|cx| FolderBrowser::new(model, machine, label, REMOTE_HOME.into(), cx))
        });
        Self {
            cockpit,
            machine,
            target,
            name,
            staged: Vec::new(),
            error: None,
            committing: false,
            scroll: ScrollHandle::new(),
            remote,
            draft: None,
            focus: cx.focus_handle(),
        }
    }

    /// Whether the card's confirm can commit — a new Project needs at least
    /// its main directory.
    pub(super) fn ready(&self) -> bool {
        self.target.is_some() || !self.staged.is_empty()
    }

    /// The name to save: what was typed, else the main directory's leaf.
    /// A directory with no leaf name (a drive root) keeps its whole path.
    pub(super) fn resolved_name(&self, typed: &str) -> Option<String> {
        let typed = typed.trim();
        if !typed.is_empty() {
            return Some(typed.to_string());
        }
        let main = self.staged.first()?;
        Some(
            main.file_name()
                .map(|leaf| leaf.to_string_lossy().into_owned())
                .unwrap_or_else(|| main.display().to_string()),
        )
    }

    /// Show a refusal (or clear it) on the card.
    pub(super) fn set_error(&mut self, error: Option<SharedString>, cx: &mut Context<Self>) {
        self.error = error;
        cx.notify();
    }

    /// Stage a picked directory on a Project that does not exist yet. The
    /// first one also names the Project, unless a name has already been
    /// typed.
    pub(super) fn stage(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.staged.contains(&path) {
            self.error = Some("that directory is already on this project".into());
            cx.notify();
            return;
        }
        let first = self.staged.is_empty();
        self.error = None;
        self.staged.push(path);
        if first {
            let name = self.name.clone();
            let seed = self.resolved_name("");
            if name.read(cx).text().trim().is_empty() {
                if let Some(seed) = seed {
                    name.update(cx, |name, cx| name.set(seed, cx));
                }
            }
        }
        cx.notify();
    }

    /// Drop a staged directory from a Project that does not exist yet. If
    /// the main one goes, the next takes its place — the list is ordered,
    /// and the top of it is always primary.
    fn drop_staged_directory(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.staged.len() {
            self.staged.remove(index);
            self.error = None;
        }
        cx.notify();
    }

    /// One directory of this computer, for the card (a machine's card
    /// has a browser of its folders instead). The picker is single-select
    /// deliberately: a Project's main directory is the first one added, and
    /// a multi-select would leave which-is-which to the file dialog.
    fn browse_for_project_directory(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add directory".into()),
        });
        cx.spawn(async move |this, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => return,
            };
            let Some(path) = path else {
                return;
            };
            this.update(cx, |_, cx| cx.emit(ProjectEditorEvent::Adopt(path)))
                .ok();
        })
        .detach();
    }

    /// Take the machine's answers to the browser's listings, once they
    /// land.
    pub(super) fn poll_remote_path(&mut self, cx: &mut Context<Self>) {
        if let Some(remote) = &self.remote {
            remote.update(cx, |remote, cx| remote.poll(cx));
        }
    }

    /// Tab in the browser: go into the first folder its line fits. False
    /// when it does not hold the keyboard.
    pub(super) fn tab_remote_path(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        self.remote
            .clone()
            .is_some_and(|remote| remote.update(cx, |remote, cx| remote.tab(window, cx)))
    }

    /// ↵ in the browser: the folder it shows, for the card, and it goes
    /// back to the folder that one is in, so the next ↵ confirms the card.
    /// None when the browser does not hold the keyboard or has not been
    /// moved since.
    pub(super) fn take_remote_path(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<PathBuf> {
        self.remote
            .clone()
            .filter(|remote| {
                let remote = remote.read(cx);
                remote.focused(window, cx) && !remote.untouched()
            })?
            .update(cx, |remote, cx| remote.take(cx))
    }

    /// Project mutations stay visible together in one protected surface.
    /// Folder picking may temporarily leave the app, but the card remains
    /// open so the changed directory list is visible on return.
    pub(super) fn card(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<ferrite_ui::sheet::Drawn> {
        let ProjectEditorInputs { project, in_use } = self
            .cockpit
            .read_with(cx, |cockpit, cx| cockpit.project_editor_inputs(self.target, cx))
            .ok()?;
        // An editing card whose Project was removed underneath it has
        // nothing left to edit.
        if self.target.is_some() && project.is_none() {
            return None;
        }
        let (title, directories) = match project {
            Some((title, directories)) => {
                (SharedString::from(format!("Edit {title}")), directories)
            }
            None => (SharedString::from("New project"), self.staged.clone()),
        };
        let empty = directories.is_empty();
        let editing = self.target;

        let close = project_editor::close_button(cx).on_click(cx.listener(
            |_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(ProjectEditorEvent::Closed);
            },
        ));
        let mut body = project_editor::body()
            .track_scroll(&self.scroll)
            .child(project_editor::name_field(
                self.name.clone(),
                self.name.read(cx).focus_handle(cx).is_focused(window),
            ))
            .child(project_editor::section_label(
                "Directories",
                "The first is the main directory",
            ));
        let mut rows = Vec::new();
        for (index, directory) in directories.into_iter().enumerate() {
            let mut actions = div().flex().items_center().gap(px(4.));
            // An existing Project's main directory is fixed: Threads,
            // worktrees and titles all hang off it. A staged one is not
            // real yet, so any row can go.
            if editing.is_none() || index > 0 {
                actions = actions.child(
                    project_editor::destructive_button(
                        ("remove-project-directory", index),
                        "Remove",
                        false,
                        cx,
                    )
                    .on_click(cx.listener(
                        move |view, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            match editing {
                                None => view.drop_staged_directory(index, cx),
                                Some(project) => {
                                    cx.emit(ProjectEditorEvent::RemoveDirectory(project, index))
                                }
                            }
                        },
                    )),
                );
            }
            rows.push(project_editor::directory_row(
                index,
                directory.display().to_string().into(),
                home_relative(&directory).into(),
                index == 0,
                actions,
            ));
        }
        body = body.child(project_editor::directory_list(rows));
        // On a machine a browser of its folders stands in for the folder
        // dialog, which could only browse this computer (ADR 0026), and the
        // control under it adds the folder it shows.
        let add = project_editor::add_directory(
            if empty {
                "+ Add main directory"
            } else {
                "+ Add directory"
            },
            cx,
        );
        body = match &self.remote {
            Some(remote) => body.child(remote.clone()).child(
                div()
                    .flex()
                    .child(add.debug_selector(|| "project-add-shown".into()).on_click(
                        cx.listener(move |view, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            let shown = view
                                .remote
                                .clone()
                                .and_then(|remote| remote.update(cx, |remote, cx| remote.take(cx)));
                            if let Some(path) = shown {
                                cx.emit(ProjectEditorEvent::Adopt(path));
                            }
                        }),
                    )),
            ),
            // Its own flex row, so the control hugs its label on the list's edge.
            None => body.child(div().flex().child(add.on_click(cx.listener(
                move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.browse_for_project_directory(cx);
                },
            )))),
        };
        let mut left = div().flex().items_center();
        if let Some(project) = editing {
            left = left.child(
                // The verb stays the verb while it cannot run; why it cannot
                // is the tooltip's to say.
                project_editor::destructive_button("remove-project", "Remove project", in_use, cx)
                    .debug_selector(|| "remove-project".into())
                    .when(in_use, |button| {
                        button.tip("Park or move its threads first")
                    })
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        cx.emit(ProjectEditorEvent::RemoveProject(project));
                    })),
            );
        }
        let ready = self.ready();
        let right = div().flex().items_center().child(
            project_editor::primary_button(
                "confirm-project",
                if editing.is_some() { "Done" } else { "Create" },
                !ready,
                cx,
            )
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                if view.ready() {
                    cx.emit(ProjectEditorEvent::Confirm);
                }
            })),
        );
        let footer = project_editor::footer(left, right);
        if let Some(error) = self.error.clone() {
            body = body.child(project_editor::error_line(error));
        }

        let card = project_editor::card(f32::from(window.viewport_size().height))
            .id("project-editor-card")
            .debug_selector(|| "project-editor-card".into())
            .track_focus(&self.focus)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(project_editor::head(title, close))
            .child(
                // The body scrolls under an overlay bar, never a gutter.
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_shrink(1.)
                    .min_h_0()
                    .child(body)
                    .child(ferrite_ui::components::scrollbar(
                        "project-editor-scrollbar",
                        &self.scroll,
                    )),
            )
            .child(footer);
        Some(ferrite_ui::sheet::Drawn { card, above: None })
    }
}

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
        if let Some(card) = self.project_card(cx) {
            card.update(cx, |card, _| card.draft = Some(draft));
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
        let cockpit = cx.weak_entity();
        let browse = self.elsewhere(machine, cx).then(|| {
            let name = self.machine_label(machine, cx).unwrap_or_default();
            (self.cockpit.clone(), format!("Directory on {name}").into())
        });
        let card = cx.new(|cx| {
            ProjectEditorView::new(cockpit, machine, target, title, browse, cx)
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
