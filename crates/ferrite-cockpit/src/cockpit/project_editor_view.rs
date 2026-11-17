//! The Project card as a view of its own (ADR 0041). It owns what only the
//! card reads: the card that is up, the one fading out, its focus, and on a
//! machine its path field and the listings it completes from. It draws the
//! Project it edits from what the Cockpit gives it (`ProjectEditorInputs`),
//! asked for each render, so the Cockpit's own notify still redraws it; and
//! whatever would change the Projects it says as a `ProjectEditorEvent`,
//! which the Cockpit answers once the card's own update has ended.

use super::*;
use ferrite_core::workspace::Listing;
use gpui::{App, WeakEntity};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

pub(crate) struct ProjectEditorView {
    cockpit: WeakEntity<CockpitView>,
    /// The open card. `None` is closed.
    pub(super) editor: Option<ProjectEditor>,
    /// The card the operator closed, and when, drawn from while it fades
    /// out (`leave`).
    leaving: Option<(ProjectEditor, std::time::Instant)>,
    pub(super) focus: FocusHandle,
}

/// The open Project card. One surface serves both verbs: `target` names
/// the Project being edited, or `None` while one is being created.
///
/// `staged` is the create card's directory list — a Project does not exist
/// until it is confirmed, so its directories live here until then. The edit
/// card writes through to the registry instead and leaves `staged` empty.
pub(super) struct ProjectEditor {
    pub(super) target: Option<ProjectId>,
    /// The name line. Empty means "call it after the main directory".
    pub(super) name: Entity<ferrite_pane::composer::Composer>,
    pub(super) staged: Vec<PathBuf>,
    /// A refusal, shown on the card that caused it.
    pub(super) error: Option<SharedString>,
    /// The body's scroll, for its overlay scrollbar.
    scroll: ScrollHandle,
    /// On a machine, the path field that stands in for the folder dialog.
    pub(super) remote: Option<RemotePath>,
}

impl ProjectEditor {
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
}

/// The Project card's path field on a machine (ADR 0026), where no folder
/// dialog can browse: what is typed completes against the directory it
/// names there, listed by the machine's Server (`list_dir`).
pub(super) struct RemotePath {
    pub(super) field: Entity<Composer>,
    /// The listing the completions come from, of the directory as typed:
    /// empty when it could not be read, which is not asked again.
    listed: Option<(String, Listing)>,
    /// A listing asked for and not answered yet, of the directory as typed.
    asking: Option<(String, Receiver<std::io::Result<Listing>>)>,
}

/// Where the field starts: the machine's home, which its Server spells out.
const REMOTE_HOME: &str = "~/";

/// How many completions the card lists under the field.
const REMOTE_COMPLETIONS: usize = 8;

/// `path` as the machine's Server spelled it out in the listing in hand —
/// of it, or of the directory it is in — so `~` stages as the machine's
/// home, named for its leaf, and one directory typed two ways stages
/// once. As typed otherwise: the Server spells `~` out when it registers.
fn spelled_out(remote: &RemotePath, path: &str) -> PathBuf {
    let (directory, leaf) = split_typed(path);
    match &remote.listed {
        Some((listed, listing)) if listed.trim_end_matches('/') == path => listing.path.clone(),
        Some((listed, listing)) if listed == directory => listing.path.join(leaf),
        _ => path.into(),
    }
}

/// A typed path's directory, to list, and the start of the name in it, to
/// complete: `~/src/fe` is `~/src/` and `fe`.
fn split_typed(typed: &str) -> (&str, &str) {
    match typed.rfind('/') {
        Some(at) => typed.split_at(at + 1),
        None => ("", typed),
    }
}

/// What the card draws from, as the Cockpit has it this frame.
pub(crate) struct ProjectEditorInputs {
    /// The Project being edited, as the registry has it: its title and its
    /// directories, the main one first. `None` while creating, or once it
    /// is gone.
    project: Option<(String, Vec<PathBuf>)>,
    /// Some Thread, open or parked, records the Project being edited.
    in_use: bool,
    /// The machine the window is on, for the path field's label.
    machine: String,
}

/// What the card asks of the window.
#[derive(Clone)]
pub(crate) enum ProjectEditorEvent {
    /// A directory picked or typed for the card (`adopt_editor_directory`).
    Adopt(PathBuf),
    /// Remove an existing Project's directory, by its place in the list.
    RemoveDirectory(ProjectId, usize),
    RemoveProject(ProjectId),
    /// Create or Done, pressed while the card can commit.
    Confirm,
    /// The path field moved: list the directory it names.
    PathEdited,
    /// The close button or a press on the veil closed it.
    Closed,
}

impl EventEmitter<ProjectEditorEvent> for ProjectEditorView {}

impl ProjectEditorView {
    pub(super) fn new(cockpit: WeakEntity<CockpitView>, cx: &mut Context<Self>) -> Self {
        Self {
            cockpit,
            editor: None,
            leaving: None,
            focus: cx.focus_handle(),
        }
    }

    /// Put up a card on `target` (`None` creates), its name line seeded
    /// with `title`, and on a machine a path field for the folder dialog.
    pub(super) fn open(
        &mut self,
        target: Option<ProjectId>,
        title: Option<String>,
        on_machine: bool,
        cx: &mut Context<Self>,
    ) {
        let name = cx.new(ferrite_pane::composer::Composer::new);
        name.update(cx, |name, cx| {
            name.set_role(ferrite_pane::composer::Role::Field(None), cx);
            if let Some(title) = title {
                name.set(title, cx)
            }
        });
        let remote = on_machine.then(|| Self::remote_path(cx));
        self.editor = Some(ProjectEditor {
            target,
            name,
            staged: Vec::new(),
            error: None,
            scroll: ScrollHandle::new(),
            remote,
        });
        cx.notify();
    }

    /// The path field a new card gets on a machine.
    fn remote_path(cx: &mut Context<Self>) -> RemotePath {
        let field = cx.new(Composer::new);
        field.update(cx, |field, cx| {
            field.set_role(ferrite_pane::composer::Role::Field(None), cx);
            field.set(REMOTE_HOME.into(), cx);
        });
        cx.subscribe(&field, |_, _, _: &Edited, cx| {
            cx.emit(ProjectEditorEvent::PathEdited)
        })
        .detach();
        RemotePath {
            field,
            listed: None,
            asking: None,
        }
    }

    /// Whether the card's confirm can commit — a new Project needs at least
    /// its main directory.
    pub(super) fn ready(&self) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|editor| editor.target.is_some() || !editor.staged.is_empty())
    }

    /// Close the card, if it is open; whether it was. It fades out over
    /// `motion::DIALOG_OUT` (Claude's dialog exit), drawn from the card as
    /// it stood.
    pub(super) fn close(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(editor) = self.editor.take() else {
            return false;
        };
        self.leave(editor, cx);
        true
    }

    /// Drop the card at once, with no exit.
    pub(super) fn discard(&mut self, cx: &mut Context<Self>) {
        self.editor = None;
        cx.notify();
    }

    /// `editor` leaves now: kept for its exit, unless motion is reduced.
    pub(super) fn leave(&mut self, editor: ProjectEditor, cx: &mut Context<Self>) {
        self.leaving = (!ferrite_ui::motion::reduced_motion(cx))
            .then(|| (editor, cx.background_executor().now()));
        cx.notify();
    }

    /// Show a refusal (or clear it) on the open card.
    pub(super) fn set_error(&mut self, error: Option<SharedString>, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_mut() {
            editor.error = error;
        }
        cx.notify();
    }

    /// Stage a picked directory on a Project that does not exist yet. The
    /// first one also names the Project, unless a name has already been
    /// typed.
    pub(super) fn stage(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if editor.staged.contains(&path) {
            editor.error = Some("that directory is already on this project".into());
            cx.notify();
            return;
        }
        let first = editor.staged.is_empty();
        editor.error = None;
        editor.staged.push(path);
        if first {
            let name = editor.name.clone();
            let seed = editor.resolved_name("");
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
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if index < editor.staged.len() {
            editor.staged.remove(index);
            editor.error = None;
        }
        cx.notify();
    }

    /// One directory of this computer, for the open card (a machine's card
    /// has a path field instead). The picker is single-select
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

    /// The open card's path field, on a machine.
    fn remote(&self) -> Option<&RemotePath> {
        self.editor.as_ref()?.remote.as_ref()
    }

    fn remote_mut(&mut self) -> Option<&mut RemotePath> {
        self.editor.as_mut()?.remote.as_mut()
    }

    /// The directory the open card's field names, unless its listing is in
    /// hand or on its way.
    pub(super) fn unlisted_directory(&self, cx: &App) -> Option<String> {
        let remote = self.remote()?;
        let typed = remote.field.read(cx).text();
        let (directory, _) = split_typed(typed);
        let known = |held: Option<&String>| held.is_some_and(|held| held == directory);
        if known(remote.listed.as_ref().map(|(at, _)| at))
            || known(remote.asking.as_ref().map(|(at, _)| at))
        {
            return None;
        }
        Some(directory.to_string())
    }

    /// The listing the Cockpit asked the machine for, on its way.
    pub(super) fn ask(
        &mut self,
        asking: Option<(String, Receiver<std::io::Result<Listing>>)>,
        cx: &mut Context<Self>,
    ) {
        if let Some(remote) = self.remote_mut() {
            remote.asking = asking;
        }
        cx.notify();
    }

    /// Take the machine's answer to the field's listing, once it lands. A
    /// directory that cannot be read lists nothing; one whose answer the
    /// lost connection took is asked again at the next keystroke.
    pub(super) fn poll_remote_path(&mut self, cx: &mut Context<Self>) {
        let Some(remote) = self.remote_mut() else {
            return;
        };
        let Some((directory, answer)) = &remote.asking else {
            return;
        };
        let listing = match answer.try_recv() {
            Err(TryRecvError::Empty) => return,
            Ok(Ok(listing)) => Some(listing),
            Ok(Err(_)) => Some(Listing {
                path: directory.into(),
                entries: Vec::new(),
            }),
            Err(TryRecvError::Disconnected) => None,
        };
        if let Some(listing) = listing {
            remote.listed = Some((directory.clone(), listing));
        }
        remote.asking = None;
        cx.notify();
    }

    /// The directories in the listing whose names start as the field's
    /// last name does; a dot directory only once a dot is typed.
    pub(super) fn remote_completions(&self, cx: &App) -> Vec<String> {
        let Some(remote) = self.remote() else {
            return Vec::new();
        };
        let typed = remote.field.read(cx).text();
        let (directory, start) = split_typed(typed);
        let Some((_, listing)) = remote
            .listed
            .as_ref()
            .filter(|(listed, _)| listed == directory)
        else {
            return Vec::new();
        };
        listing
            .entries
            .iter()
            .filter(|entry| {
                entry.directory
                    && entry.name.starts_with(start)
                    && (start.starts_with('.') || !entry.name.starts_with('.'))
            })
            .take(REMOTE_COMPLETIONS)
            .map(|entry| entry.name.clone())
            .collect()
    }

    /// Complete the field's last name to `name`, a directory: the field
    /// then lists what is in it.
    fn complete_remote_path(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(remote) = self.remote() else {
            return;
        };
        let field = remote.field.clone();
        let typed = field.read(cx).text().to_string();
        let (directory, _) = split_typed(&typed);
        let completed = format!("{directory}{name}/");
        field.update(cx, |field, cx| field.set(completed, cx));
    }

    /// Tab in the path field: complete it to the first directory that
    /// fits. False when the field does not hold the keyboard.
    pub(super) fn tab_remote_path(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if !self.remote_path_focused(window, cx) {
            return false;
        }
        if let Some(first) = self.remote_completions(cx).into_iter().next() {
            self.complete_remote_path(&first, cx);
        }
        true
    }

    /// ↵ in the path field: the directory it names, for the card, and the
    /// field empties, so the next ↵ confirms the card. None when the field
    /// does not hold the keyboard or holds nothing.
    pub(super) fn take_remote_path(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<PathBuf> {
        if !self.remote_path_focused(window, cx) {
            return None;
        }
        let remote = self.remote()?;
        let field = remote.field.clone();
        let typed = field.read(cx).text().trim().to_string();
        let path = match typed.trim_end_matches('/') {
            "" if typed.starts_with('/') => "/",
            "" => return None,
            path => path,
        };
        let path = spelled_out(remote, path);
        field.update(cx, |field, cx| field.set(String::new(), cx));
        Some(path)
    }

    fn remote_path_focused(&self, window: &Window, cx: &App) -> bool {
        self.remote()
            .is_some_and(|remote| remote.field.read(cx).focus_handle(cx).is_focused(window))
    }

    /// The path field and, under it, the directories that complete it —
    /// each a press away.
    fn remote_path_element(
        &self,
        remote: &RemotePath,
        machine: &str,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let focused = remote.field.read(cx).focus_handle(cx).is_focused(window);
        let completions =
            self.remote_completions(cx)
                .into_iter()
                .enumerate()
                .map(|(index, name)| {
                    project_editor::path_completion(index, &name).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.complete_remote_path(&name, cx);
                        }),
                    )
                });
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(project_editor::path_field(
                machine,
                remote.field.clone(),
                focused,
            ))
            .child(project_editor::directory_list(completions.collect()))
    }

    /// Project mutations stay visible together in one protected surface.
    /// Folder picking may temporarily leave the app, but the card remains
    /// open so the changed directory list is visible on return.
    fn card(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        // The open card, or the one that closed while it fades out.
        let (editor, leaving) = match (&self.editor, &self.leaving) {
            (Some(editor), _) => (editor, None),
            (None, Some((editor, left)))
                if cx
                    .background_executor()
                    .now()
                    .saturating_duration_since(*left)
                    < ferrite_ui::motion::DIALOG_OUT.duration() =>
            {
                (editor, Some(*left))
            }
            _ => return None,
        };
        let ProjectEditorInputs {
            project,
            in_use,
            machine,
        } = self
            .cockpit
            .read_with(cx, |cockpit, _| cockpit.project_editor_inputs(editor.target))
            .ok()?;
        // An editing card whose Project was removed underneath it has
        // nothing left to edit.
        if editor.target.is_some() && project.is_none() {
            return None;
        }
        let (title, directories) = match project {
            Some((title, directories)) => {
                (SharedString::from(format!("Edit {title}")), directories)
            }
            None => (SharedString::from("New project"), editor.staged.clone()),
        };
        let empty = directories.is_empty();
        let editing = editor.target;

        let close = project_editor::close_button(cx).on_click(cx.listener(
            |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.close_and_tell(cx);
            },
        ));
        let mut body = project_editor::body()
            .track_scroll(&editor.scroll)
            .child(project_editor::name_field(
                editor.name.clone(),
                editor.name.read(cx).focus_handle(cx).is_focused(window),
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
        // On a machine a path field stands in for the folder dialog, which
        // could only browse this computer (ADR 0026).
        body = match &editor.remote {
            Some(remote) => body.child(self.remote_path_element(remote, &machine, window, cx)),
            // Its own flex row, so the control hugs its label on the list's edge.
            None => body.child(
                div().flex().child(
                    project_editor::add_directory(
                        if empty {
                            "+ Add main directory"
                        } else {
                            "+ Add directory"
                        },
                        cx,
                    )
                    .on_click(cx.listener(
                        move |view, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            view.browse_for_project_directory(cx);
                        },
                    )),
                ),
            ),
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
        if let Some(error) = editor.error.clone() {
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
                        &editor.scroll,
                    )),
            )
            .child(footer);
        let sheet = match leaving {
            Some(left) => ferrite_ui::motion::veil_out(
                left,
                ferrite_ui::motion::VEIL,
                ferrite_ui::sheet::leaving_veil().child(ferrite_ui::motion::dialog_out(left, card)),
            )
            .into_any_element(),
            None => ferrite_ui::motion::veil_in(
                "project-editor-veil",
                ferrite_ui::motion::VEIL,
                project_editor::veil()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            view.close_and_tell(cx);
                        }),
                    )
                    .child(ferrite_ui::motion::dialog_in(
                        "project-editor-card-in",
                        card,
                    )),
            )
            .into_any_element(),
        };
        Some(deferred(sheet).with_priority(3).into_any_element())
    }

    fn close_and_tell(&mut self, cx: &mut Context<Self>) {
        if self.close(cx) {
            cx.emit(ProjectEditorEvent::Closed);
        }
    }
}

impl Render for ProjectEditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ferrite_ui::loops_overlay::begin(window, cx.entity_id());
        self.card(window, cx)
            .unwrap_or_else(|| div().into_any_element())
    }
}

impl CockpitView {
    /// Everything the card draws from, for the card on `target`. It never
    /// reads the card: the card calls it while its own update is running.
    fn project_editor_inputs(&self, target: Option<ProjectId>) -> ProjectEditorInputs {
        let project = target
            .and_then(|project| self.cockpit.registry().project(project))
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
            in_use: target.is_some_and(|project| self.project_in_use(project)),
            machine: self
                .cockpit
                .machine()
                .map_or_else(String::new, |bridge| bridge.name().to_string()),
        }
    }

    pub(super) fn on_project_editor_event(
        &mut self,
        _: Entity<ProjectEditorView>,
        event: &ProjectEditorEvent,
        cx: &mut Context<Self>,
    ) {
        match event.clone() {
            ProjectEditorEvent::Adopt(path) => self.adopt_editor_directory(path, cx),
            ProjectEditorEvent::RemoveDirectory(project, index) => {
                let refusal = self
                    .cockpit
                    .remove_project_directory(project, index)
                    .err()
                    .map(|error| SharedString::from(format!("directory unchanged · {error}")));
                self.project_editor_view
                    .update(cx, |editor, cx| editor.set_error(refusal, cx));
                cx.notify();
            }
            ProjectEditorEvent::RemoveProject(project) => {
                if self.project_in_use(project) {
                    return;
                }
                match self.cockpit.remove_project(project) {
                    Err(error) => {
                        let error = Some(format!("not removed · {error}").into());
                        self.project_editor_view
                            .update(cx, |editor, cx| editor.set_error(error, cx));
                    }
                    Ok(()) => {
                        if self.nav_filter == Some(project) {
                            self.nav_filter = None;
                        }
                        self.nav_notice.clear();
                        self.close_project_editor(cx);
                    }
                }
                cx.notify();
            }
            ProjectEditorEvent::Confirm => self.confirm_project_editor(cx),
            ProjectEditorEvent::PathEdited => self.list_remote_path(cx),
            ProjectEditorEvent::Closed => cx.notify(),
        }
    }

    pub(crate) fn project_editor_open(&self, cx: &App) -> bool {
        self.project_editor_view.read(cx).editor.is_some()
    }

    /// Open the card on an existing Project, seeded with its name.
    pub(crate) fn open_project_editor(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let Some(title) = self
            .cockpit
            .registry()
            .project(project)
            .map(|project| project.title.clone())
        else {
            return;
        };
        let on_machine = self.on_machine();
        self.project_editor_view.update(cx, |editor, cx| {
            editor.open(Some(project), Some(title), on_machine, cx)
        });
        self.show_project_editor(cx);
    }

    /// Open the card on a Project that does not exist yet: an empty name
    /// and no directories. Nothing is registered until Create.
    pub(super) fn open_project_creator(&mut self, cx: &mut Context<Self>) {
        let on_machine = self.on_machine();
        self.project_editor_view
            .update(cx, |editor, cx| editor.open(None, None, on_machine, cx));
        self.show_project_editor(cx);
    }

    /// The card is modal: everything else floating goes down with it.
    fn show_project_editor(&mut self, cx: &mut Context<Self>) {
        self.settings_view.update(cx, |settings, cx| settings.close(cx));
        self.connections.editor = None;
        self.popover = None;
        self.context_menu = None;
        self.list_remote_path(cx);
        cx.notify();
    }

    /// Close the Project card, if it is open; whether it was.
    pub(super) fn close_project_editor(&mut self, cx: &mut Context<Self>) -> bool {
        let closed = self
            .project_editor_view
            .update(cx, |editor, cx| editor.close(cx));
        if closed {
            cx.notify();
        }
        closed
    }

    /// Commit the card. Creating registers the staged directories as one
    /// Project — the first is primary — and names it; editing only renames,
    /// because its directory edits already went through. A refusal keeps the
    /// card open with the reason on it, so nothing typed is lost.
    pub(super) fn confirm_project_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self
            .project_editor_view
            .update(cx, |editor, _| editor.editor.take())
        else {
            return;
        };
        let typed = editor.name.read(cx).text().to_string();
        let outcome = match editor.target {
            Some(project) => {
                let typed = typed.trim();
                if typed.is_empty() {
                    Ok(project)
                } else {
                    self.cockpit
                        .rename_project(project, typed)
                        .map(|()| project)
                }
            }
            None => {
                let name = editor.resolved_name(&typed);
                self.cockpit
                    .register_project_directories(&editor.staged)
                    .and_then(|project| {
                        match name {
                            Some(name) => self.cockpit.rename_project(project, &name),
                            None => Ok(()),
                        }
                        .map(|()| project)
                    })
            }
        };
        match outcome {
            Ok(project) => {
                // The Project the operator just described is the one they
                // want to look at, and to work in.
                self.launch_project = Some(project);
                self.choose_nav_filter(Some(project), cx);
                if self.panes.is_empty() {
                    self.open_draft_with_provider(
                        DraftTarget::Main,
                        self.prefs.settings.default_provider,
                        cx,
                    );
                }
                self.nav_notice.clear();
                self.refresh_names();
                self.project_editor_view
                    .update(cx, |view, cx| view.leave(editor, cx));
            }
            Err(error) => {
                let editor = ProjectEditor {
                    error: Some(format!("project unchanged · {error}").into()),
                    ..editor
                };
                self.project_editor_view.update(cx, |view, cx| {
                    view.editor = Some(editor);
                    cx.notify();
                });
            }
        }
        cx.notify();
    }

    /// Land a picked directory on the open card. On a new Project it is
    /// staged; on an existing one it is attached at once, so the card
    /// always shows the registry's own list.
    pub(super) fn adopt_editor_directory(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(target) = self
            .project_editor_view
            .read(cx)
            .editor
            .as_ref()
            .map(|editor| editor.target)
        else {
            return;
        };
        match target {
            Some(project) => {
                let refusal = self
                    .cockpit
                    .add_project_directories(project, &[path])
                    .err()
                    .map(|error| format!("directory unchanged · {error}").into());
                self.project_editor_view
                    .update(cx, |editor, cx| editor.set_error(refusal, cx));
            }
            None => self
                .project_editor_view
                .update(cx, |editor, cx| editor.stage(path, cx)),
        }
        cx.notify();
    }

    /// List the directory the open card's field names — on opening, and
    /// each time it moves — unless that listing is in hand or on its way.
    fn list_remote_path(&mut self, cx: &mut Context<Self>) {
        let Some(directory) = self.project_editor_view.read(cx).unlisted_directory(cx) else {
            return;
        };
        let asking = if directory.is_empty() {
            None
        } else {
            self.cockpit
                .list_dir(std::path::Path::new(&directory))
                .ok()
                .map(|answer| (directory, answer))
        };
        self.project_editor_view
            .update(cx, |editor, cx| editor.ask(asking, cx));
        cx.notify();
    }
}
