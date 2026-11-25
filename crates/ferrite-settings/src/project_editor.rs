//! The Project card (ADR 0041), made when it opens and dropped once it
//! has gone (ADR 0042), so the view is the card. It owns what only the
//! card reads: its name line, a new Project's directories, its refusal,
//! its focus, and on a machine the browser of its folders. It draws the
//! Project it edits from what the window gives it (`ProjectEditorInputs`),
//! read through a `Source` each frame, so the window's own notify still
//! redraws it; and whatever would change the Projects it says as a
//! `ProjectEditorEvent`, which the window answers once the card's own
//! update has ended (`cockpit/project_editor_view.rs`).

use std::path::PathBuf;

use ferrite_core::roster::PaneIdentity;
use ferrite_core::{MachineId, ProjectKey};
use ferrite_pane::band::Band;
use ferrite_pane::composer::{Composer, Role};
use ferrite_ui::components::{self, home_relative, Tip as _};
use ferrite_ui::pointer::Pointer;
use ferrite_ui::sheet;
use ferrite_ui::source::Source;
use ferrite_ui::theme::*;
use gpui::component::button::Button;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, App, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle, Focusable,
    MouseButton, MouseDownEvent, ScrollHandle, SharedString, Window,
};

use crate::folder_browser::{FolderBrowser, List};

/// The Project card. One surface serves both verbs: `target` names the
/// Project being edited, or `None` while one is being created.
///
/// `staged` is the create card's directory list — a Project does not exist
/// until it is confirmed, so its directories live here until then. The edit
/// card writes through to the registry instead and leaves `staged` empty.
pub struct ProjectEditorView {
    /// The Project the card is on, as the window has it.
    inputs: Source<ProjectEditorInputs, Option<ProjectKey>>,
    /// The machine the card's Project is on, or is created on.
    pub machine: MachineId,
    pub target: Option<ProjectKey>,
    /// The name line. Empty means "call it after the main directory".
    pub name: Entity<Composer>,
    pub staged: Vec<PathBuf>,
    /// A refusal, shown on the card that caused it.
    pub error: Option<SharedString>,
    /// The card was committed and its Server has not answered: a second
    /// press commits nothing.
    pub committing: bool,
    /// The body's scroll, for its overlay scrollbar.
    scroll: ScrollHandle,
    /// On a machine, the browser of its folders that stands in for the
    /// folder dialog.
    pub remote: Option<Entity<FolderBrowser>>,
    /// The draft the card was opened for: the Project it creates is chosen
    /// on it.
    pub draft: Option<PaneIdentity>,
    /// That draft's band, which the card keeps clear of.
    pub band: Option<Band>,
    pub focus: FocusHandle,
}

/// Where the browser opens: the machine's home, which its Server spells
/// out.
const REMOTE_HOME: &str = "~/";

/// What the card draws from, as the window has it this frame.
pub struct ProjectEditorInputs {
    /// The Project being edited, as the registry has it: its title and its
    /// directories, the main one first. `None` while creating, or once it
    /// is gone.
    pub project: Option<(String, Vec<PathBuf>)>,
    /// Some Thread, open or parked, records the Project being edited.
    pub in_use: bool,
}

/// What the card asks of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectEditorEvent {
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
    /// A card on `target` (`None` creates on `machine`), reading it
    /// through `inputs`, its name line seeded with `title`, and on another
    /// machine, `browse`, a browser of its folders labelled for it in place
    /// of the folder dialog.
    pub fn new(
        inputs: Source<ProjectEditorInputs, Option<ProjectKey>>,
        machine: MachineId,
        target: Option<ProjectKey>,
        title: Option<String>,
        browse: Option<(List, SharedString)>,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(Composer::new);
        name.update(cx, |name, cx| {
            name.set_role(Role::Field(None), cx);
            if let Some(title) = title {
                name.set(title, cx)
            }
        });
        let remote = browse.map(|(list, label)| {
            cx.new(|cx| FolderBrowser::new(list, label, REMOTE_HOME.into(), cx))
        });
        Self {
            inputs,
            machine,
            target,
            name,
            staged: Vec::new(),
            error: None,
            committing: false,
            scroll: ScrollHandle::new(),
            remote,
            draft: None,
            band: None,
            focus: cx.focus_handle(),
        }
    }

    /// Whether the card's confirm can commit — a new Project needs at least
    /// its main directory.
    pub fn ready(&self) -> bool {
        self.target.is_some() || !self.staged.is_empty()
    }

    /// The name to save: what was typed, else the main directory's leaf.
    /// A directory with no leaf name (a drive root) keeps its whole path.
    pub fn resolved_name(&self, typed: &str) -> Option<String> {
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
    pub fn set_error(&mut self, error: Option<SharedString>, cx: &mut Context<Self>) {
        self.error = error;
        cx.notify();
    }

    /// Stage a picked directory on a Project that does not exist yet. The
    /// first one also names the Project, unless a name has already been
    /// typed.
    pub fn stage(&mut self, path: PathBuf, cx: &mut Context<Self>) {
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
    pub fn poll_remote_path(&mut self, cx: &mut Context<Self>) {
        if let Some(remote) = &self.remote {
            remote.update(cx, |remote, cx| remote.poll(cx));
        }
    }

    /// Tab in the browser: go into the first folder its line fits. False
    /// when it does not hold the keyboard.
    pub fn tab_remote_path(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        self.remote
            .clone()
            .is_some_and(|remote| remote.update(cx, |remote, cx| remote.tab(window, cx)))
    }

    /// ↵ in the browser: the folder it shows, for the card, and it goes
    /// back to the folder that one is in, so the next ↵ confirms the card.
    /// None when the browser does not hold the keyboard or has not been
    /// moved since.
    pub fn take_remote_path(
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
    pub fn card(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<ferrite_ui::sheet::Drawn> {
        let ProjectEditorInputs { project, in_use } = self.inputs.read(&self.target, cx)?;
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

        let close = close_button(cx).on_click(cx.listener(
            |_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(ProjectEditorEvent::Closed);
            },
        ));
        let mut body = ferrite_ui::form::body()
            .track_scroll(&self.scroll)
            .child(ferrite_ui::field::text_field(
                "Name".into(),
                self.name.clone(),
                self.name.read(cx).focus_handle(cx).is_focused(window),
                "project-name-focused",
            ))
            .child(ferrite_ui::form::section_label(
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
                    ferrite_ui::form::destructive_button(
                        ("remove-project-directory", index),
                        "Remove",
                        false,
                        cx,
                    )
                    .debug_selector(move || format!("remove-project-directory-{index}"))
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
            rows.push(directory_row(
                index,
                directory.display().to_string().into(),
                home_relative(&directory).into(),
                index == 0,
                actions,
            ));
        }
        body = body.child(ferrite_ui::form::directory_list(rows));
        // On a machine a browser of its folders stands in for the folder
        // dialog, which could only browse this computer (ADR 0026), and the
        // control under it adds the folder it shows.
        let add = add_directory(
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
                ferrite_ui::form::destructive_button("remove-project", "Remove project", in_use, cx)
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
            ferrite_ui::form::primary_button(
                "confirm-project",
                if editing.is_some() { "Done" } else { "Create" },
                !ready,
                cx,
            )
            .debug_selector(|| "project-confirm".into())
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                if view.ready() {
                    cx.emit(ProjectEditorEvent::Confirm);
                }
            })),
        );
        let footer = ferrite_ui::form::footer(left, right);
        if let Some(error) = self.error.clone() {
            body = body.child(ferrite_ui::form::error_line(error));
        }

        let band_top = self.band.as_ref().and_then(Band::top);
        let viewport_h = f32::from(window.viewport_size().height);
        let card = if self.remote.is_some() {
            ferrite_ui::form::steady_card_over(viewport_h, band_top)
        } else {
            ferrite_ui::form::card_over(viewport_h, band_top)
        };
        let card = card
            .id("project-editor-card")
            .debug_selector(|| "project-editor-card".into())
            .track_focus(&self.focus)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(ferrite_ui::form::head(title, close))
            .child(
                // The body scrolls under an overlay bar, never a gutter.
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_grow(1.)
                    .flex_shrink(1.)
                    .min_h_0()
                    .child(body)
                    .child(components::scrollbar(
                        "project-editor-scrollbar",
                        &self.scroll,
                    )),
            )
            .child(footer);
        Some(ferrite_ui::sheet::Drawn { card, above: None })
    }
}

fn close_button(cx: &App) -> Button {
    sheet::sheet_close("project-editor-close", "Close project editor", cx)
}

/// The quiet text control under the list that adds a directory (`+ Add
/// directory`, or `+ Add main directory` while there is none): `INK_SUBTLE`,
/// `INK_STRONG` on `paint::RAISED` under the pointer, on the text's own edge.
fn add_directory(label: &'static str, cx: &App) -> Button {
    components::form_button("add-project-directory", cx)
        .debug_selector(|| "project-add-directory".into())
        .group(ADD_GROUP)
        .h(px(ROW))
        .ml(px(-CH))
        .px(px(CH))
        .child(
            components::text_ui()
                .text_color(rgb(INK_SUBTLE))
                .group_hover(ADD_GROUP, |style| style.text_color(rgb(INK_STRONG)))
                .child(label),
        )
}

const ADD_GROUP: &str = "project-add-directory";

/// One directory: its name, `main` beside the first, and its path under
/// it as the operator reads it (`shown`, home spelled `~`) in `INK_SUBTLE`,
/// one line cut from its start — a path's end names it — and whole in the
/// tooltip. Under the pointer the row takes `paint::RAISED`, hung a cell
/// outside the text's edge, so the text stays on the sheet's column.
fn directory_row(
    index: usize,
    path: SharedString,
    shown: SharedString,
    main: bool,
    actions: impl IntoElement,
) -> gpui::Stateful<Div> {
    // The final directory component distinguishes neighboring project roots;
    // a shared parent prefix does not. Native Path semantics also preserve
    // Windows drive/UNC roots, which have no file name, through the fallback.
    let name: SharedString = std::path::Path::new(path.as_ref())
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path.as_ref())
        .to_string()
        .into();
    let selector: SharedString = format!("project-directory:{path}").into();
    div()
        .id(("project-directory-row", index))
        .flex()
        .items_center()
        .flex_shrink_0()
        .ml(px(-CH))
        .px(px(CH))
        .py(px(SPACE_1))
        .gap(px(2.0 * CH))
        .hover_raised(format!("project-directory-row-{index}"))
        .tooltip(ferrite_ui::menu::tooltip(path.clone()))
        .cursor_default()
        .child(
            div()
                .id(selector.clone())
                .debug_selector(move || selector.to_string())
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(2.0 * CH))
                        .child(
                            components::text_ui()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(INK_STRONG))
                                .child(name),
                        )
                        .when(main, |line| {
                            line.child(
                                components::text_ui()
                                    .flex_shrink_0()
                                    .text_color(rgb(INK_SUBTLE))
                                    .debug_selector(|| "project-directory-main".into())
                                    .child("main"),
                            )
                        }),
                )
                .child(
                    components::text_ui()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis_start()
                        .text_color(rgb(INK_SUBTLE))
                        .child(shown),
                ),
        )
        .child(div().flex_shrink_0().child(actions))
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::mpsc::channel;

    use ferrite_core::folder::{DirEntry, Listing};
    use ferrite_core::project::ProjectId;
    use ferrite_ui::testing::{click, mount, record, shows};
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;

    /// The sheet host's part in it: the card, where the host lays it, and
    /// each press that reached the window behind it.
    struct Host(Entity<ProjectEditorView>, Rc<Cell<usize>>);

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let drawn = self.0.update(cx, |card, cx| card.card(window, cx));
            let behind = self.1.clone();
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, move |_, _, _| behind.set(behind.get() + 1))
                .children(drawn.map(|drawn| drawn.card))
        }
    }

    /// What the window keeps of its Projects, as a test keeps it: each
    /// one's title and directories, whether a Thread records it, each
    /// Project the card asked about, and the presses that reached it.
    struct Kept {
        projects: HashMap<ProjectKey, (String, Vec<PathBuf>)>,
        in_use: bool,
        asked: Rc<RefCell<Vec<Option<ProjectKey>>>>,
        behind: Rc<Cell<usize>>,
    }

    fn app() -> ProjectKey {
        ProjectKey::fixture(1, ProjectId(4))
    }

    fn kept() -> Kept {
        Kept {
            projects: HashMap::from([(
                app(),
                ("app".to_string(), vec![PathBuf::from("/src/app"), PathBuf::from("/src/app-docs")]),
            )]),
            in_use: false,
            asked: Rc::default(),
            behind: Rc::default(),
        }
    }

    /// The card on `target` (`None` creates one) in a window of its own,
    /// drawn as the sheet host draws it, reading `kept` as the Cockpit
    /// reads its registry; on a machine when `browse` is given.
    fn open(
        kept: Kept,
        target: Option<ProjectKey>,
        browse: Option<(List, SharedString)>,
        cx: &mut TestAppContext,
    ) -> (Entity<ProjectEditorView>, Entity<Kept>, &mut VisualTestContext) {
        let behind = kept.behind.clone();
        let kept = cx.new(|_| kept);
        let inputs = Source::of(&kept, |kept: &Kept, target: &Option<ProjectKey>, _| {
            kept.asked.borrow_mut().push(*target);
            Some(ProjectEditorInputs {
                project: target.and_then(|target| kept.projects.get(&target).cloned()),
                in_use: kept.in_use,
            })
        });
        let title = target.map(|_| "app".to_string());
        let (host, cx) = mount(cx, move |_, cx| {
            let card = cx.new(|cx| ProjectEditorView::new(inputs, app().machine(), target, title, browse, cx));
            Host(card, behind)
        });
        let card = host.read_with(cx, |host, _| host.0.clone());
        cx.run_until_parked();
        (card, kept, cx)
    }

    fn card_text(cx: &mut VisualTestContext) -> String {
        cx.update(|window, _| window.debug_text("project-editor-card"))
            .unwrap_or_default()
            .join("\n")
    }

    fn redraw(card: &Entity<ProjectEditorView>, cx: &mut VisualTestContext) {
        card.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
    }

    /// A new Project's card has no directory, so it cannot be created:
    /// its control offers the main one. Staged, Create says so.
    #[gpui::test]
    fn a_new_project_is_created_once_it_has_a_directory(cx: &mut TestAppContext) {
        let (card, _kept, cx) = open(kept(), None, None, cx);
        let heard = record(cx, &card, ProjectEditorEvent::clone);
        let text = card_text(cx);
        assert!(text.contains("New project"), "{text}");
        assert!(text.contains("+ Add main directory"), "{text}");
        click(cx, "project-confirm");
        assert_eq!(heard.take(), [], "nothing to create");

        card.update(cx, |card, cx| card.stage("/src/primary".into(), cx));
        cx.run_until_parked();
        assert!(shows(cx, "project-directory:/src/primary"));
        assert!(shows(cx, "project-directory-main"));
        assert!(card_text(cx).contains("+ Add directory"));
        click(cx, "project-confirm");
        assert_eq!(heard.take(), [ProjectEditorEvent::Confirm]);
    }

    /// The first directory names the Project until a name is typed; one
    /// already on it is refused on the card. A drive root, with no leaf,
    /// names it whole.
    #[gpui::test]
    fn staging_names_the_project_until_one_is_typed(cx: &mut TestAppContext) {
        let (card, _kept, cx) = open(kept(), None, None, cx);
        let name = |cx: &mut VisualTestContext| {
            card.read_with(cx, |card, cx| card.name.read(cx).text().to_string())
        };
        card.update(cx, |card, cx| card.stage("/src/primary".into(), cx));
        assert_eq!(name(cx), "primary");
        card.update(cx, |card, cx| card.stage("/src/primary".into(), cx));
        cx.run_until_parked();
        assert!(card_text(cx).contains("that directory is already on this project"));
        card.update(cx, |card, cx| card.stage("/src/extra".into(), cx));
        cx.run_until_parked();
        assert!(!card_text(cx).contains("already on this project"), "a new directory clears it");
        assert_eq!(card.read_with(cx, |card, _| card.staged.clone()), [PathBuf::from("/src/primary"), "/src/extra".into()]);

        let (typed, _kept, cx) = open(kept(), None, None, cx);
        typed.update(cx, |card, cx| card.name.update(cx, |name, cx| name.set("Mine".into(), cx)));
        typed.update(cx, |card, cx| card.stage("/src/primary".into(), cx));
        typed.read_with(cx, |card, cx| {
            assert_eq!(card.name.read(cx).text(), "Mine", "a typed name stays");
            assert_eq!(card.resolved_name("  "), Some("primary".into()));
            assert_eq!(card.resolved_name(" Mine "), Some("Mine".into()));
        });
        let (root, _kept, cx) = open(kept(), None, None, cx);
        root.update(cx, |card, cx| card.stage("/".into(), cx));
        assert_eq!(root.read_with(cx, |card, _| card.resolved_name("")), Some("/".into()));
    }

    /// A staged directory goes at its Remove, the card's own; the next is
    /// main then.
    #[gpui::test]
    fn a_staged_directory_is_removed_on_the_card(cx: &mut TestAppContext) {
        let (card, _kept, cx) = open(kept(), None, None, cx);
        let heard = record(cx, &card, ProjectEditorEvent::clone);
        card.update(cx, |card, cx| {
            card.stage("/src/primary".into(), cx);
            card.stage("/src/extra".into(), cx);
        });
        cx.run_until_parked();
        click(cx, "remove-project-directory-0");
        assert_eq!(card.read_with(cx, |card, _| card.staged.clone()), [PathBuf::from("/src/extra")]);
        assert!(!shows(cx, "project-directory:/src/primary"));
        assert!(shows(cx, "project-directory:/src/extra"));
        assert!(shows(cx, "project-directory-main"), "the next is main");
        assert_eq!(heard.take(), [], "nothing is asked of the window");
    }

    /// An existing Project's card draws its directories as the window has
    /// them; its main one stays, another's Remove and Remove project are
    /// asked of the window, and Done confirms.
    #[gpui::test]
    fn an_existing_projects_card_asks_the_window(cx: &mut TestAppContext) {
        let (card, kept, cx) = open(kept(), Some(app()), None, cx);
        let heard = record(cx, &card, ProjectEditorEvent::clone);
        assert!(card_text(cx).contains("Edit app"));
        assert!(shows(cx, "project-directory:/src/app"));
        assert!(shows(cx, "project-directory:/src/app-docs"));
        assert!(!shows(cx, "remove-project-directory-0"), "the main directory is fixed");
        assert_eq!(kept.read_with(cx, |kept, _| kept.asked.borrow().last().copied()), Some(Some(app())));
        click(cx, "remove-project-directory-1");
        click(cx, "remove-project");
        click(cx, "project-confirm");
        assert_eq!(
            heard.take(),
            [
                ProjectEditorEvent::RemoveDirectory(app(), 1),
                ProjectEditorEvent::RemoveProject(app()),
                ProjectEditorEvent::Confirm,
            ]
        );
        assert!(card_text(cx).contains("Done"));
    }

    /// The window's notify redraws the card from what it now keeps.
    #[gpui::test]
    fn the_windows_notify_redraws_the_card(cx: &mut TestAppContext) {
        let (_card, kept, cx) = open(kept(), Some(app()), None, cx);
        kept.update(cx, |kept, cx| {
            kept.projects.get_mut(&app()).unwrap().1.push("/src/app-site".into());
            cx.notify();
        });
        cx.run_until_parked();
        assert!(shows(cx, "project-directory:/src/app-site"));
    }

    /// A card whose Project went, or whose window went, draws nothing.
    #[gpui::test]
    fn a_card_whose_project_or_window_went_draws_nothing(cx: &mut TestAppContext) {
        let (card, kept, cx) = open(kept(), Some(app()), None, cx);
        assert!(shows(cx, "project-editor-card"));
        kept.update(cx, |kept, cx| {
            kept.projects.clear();
            cx.notify();
        });
        cx.run_until_parked();
        assert!(!shows(cx, "project-editor-card"), "the Project went");

        let (creating, kept, cx) = open(self::kept(), None, None, cx);
        assert!(shows(cx, "project-editor-card"));
        drop(kept);
        redraw(&creating, cx);
        assert!(!shows(cx, "project-editor-card"), "the window went");
        let _ = card;
    }

    /// While a Thread records the Project, Remove project is there but
    /// takes no press.
    #[gpui::test]
    fn remove_project_takes_no_press_while_the_project_is_in_use(cx: &mut TestAppContext) {
        let (card, _kept, cx) = open(Kept { in_use: true, ..kept() }, Some(app()), None, cx);
        let heard = record(cx, &card, ProjectEditorEvent::clone);
        assert!(shows(cx, "remove-project"));
        click(cx, "remove-project");
        assert_eq!(heard.take(), []);
    }

    /// The close button closes it.
    #[gpui::test]
    fn the_close_button_closes_it(cx: &mut TestAppContext) {
        let (card, _kept, cx) = open(kept(), None, None, cx);
        let heard = record(cx, &card, ProjectEditorEvent::clone);
        click(cx, "project-editor-close");
        assert_eq!(heard.take(), [ProjectEditorEvent::Closed]);
    }

    /// The window's refusal is said on the card, until it is cleared.
    #[gpui::test]
    fn a_refusal_is_said_on_the_card(cx: &mut TestAppContext) {
        let (card, _kept, cx) = open(kept(), Some(app()), None, cx);
        card.update(cx, |card, cx| card.set_error(Some("not removed · busy".into()), cx));
        cx.run_until_parked();
        assert!(card_text(cx).contains("not removed · busy"));
        card.update(cx, |card, cx| card.set_error(None, cx));
        cx.run_until_parked();
        assert!(!card_text(cx).contains("not removed"));
    }

    /// The name field wears the focus ink while the keyboard is in it, and
    /// its resting edge otherwise.
    #[gpui::test]
    fn the_name_field_shows_keyboard_focus(cx: &mut TestAppContext) {
        let (card, _kept, cx) = open(kept(), None, None, cx);
        assert!(!shows(cx, "project-name-focused"));
        cx.update(|window, cx| {
            let name = card.read(cx).name.read(cx).focus_handle(cx);
            window.focus(&name, cx);
        });
        cx.run_until_parked();
        assert!(shows(cx, "project-name-focused"), "focused: the focus ink");
        cx.update(|window, cx| {
            let elsewhere = card.read(cx).focus.clone();
            window.focus(&elsewhere, cx);
        });
        cx.run_until_parked();
        assert!(!shows(cx, "project-name-focused"), "elsewhere: the resting edge");
    }

    /// On this computer its control asks the folder dialog for one
    /// directory, and the one picked is asked of the window; a dialog
    /// dismissed asks nothing.
    #[gpui::test]
    fn the_folder_dialog_adds_the_directory_picked(cx: &mut TestAppContext) {
        let (card, _kept, cx) = open(kept(), None, None, cx);
        let heard = record(cx, &card, ProjectEditorEvent::clone);
        click(cx, "project-add-directory");
        assert!(cx.did_prompt_for_paths(), "the folder dialog is up");
        cx.simulate_path_prompt_response(|options| {
            assert!(options.directories && !options.files && !options.multiple, "one directory");
            Some(vec!["/src/picked".into()])
        });
        cx.run_until_parked();
        assert_eq!(heard.take(), [ProjectEditorEvent::Adopt("/src/picked".into())]);

        click(cx, "project-add-directory");
        cx.simulate_path_prompt_response(|_| None);
        cx.run_until_parked();
        assert_eq!(heard.take(), [], "dismissed");
    }

    /// A press on the card stays on it; one beside it reaches the window.
    #[gpui::test]
    fn a_press_on_the_card_stays_on_it(cx: &mut TestAppContext) {
        let kept = kept();
        let behind = kept.behind.clone();
        let (_card, _kept, cx) = open(kept, None, None, cx);
        let card = cx.update(|window, _| window.debug_bounds("project-editor-card")).expect("the card");
        cx.simulate_click(card.origin + gpui::point(px(4.), px(4.)), gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(behind.get(), 0, "on the card");
        cx.simulate_click(gpui::point(px(2.), px(2.)), gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(behind.get(), 1, "beside it");
    }

    /// The card is a card, not a title bar: its body is inside it.
    #[gpui::test]
    fn the_card_draws_its_body(cx: &mut TestAppContext) {
        let (_card, _kept, cx) = open(kept(), None, None, cx);
        let (card, add) = cx.update(|window, _| {
            (window.debug_bounds("project-editor-card"), window.debug_bounds("project-add-directory"))
        });
        let (card, add) = (card.expect("the card"), add.expect("the add control"));
        assert!(add.size.height > px(0.) && card.contains(&add.origin), "{add:?} in {card:?}");
    }

    /// On a machine its browser stands in for the folder dialog: the
    /// control under it adds the folder it shows, and ↵ in it takes the
    /// folder once the operator has moved it.
    #[gpui::test]
    fn on_a_machine_the_browser_adds_the_folder_it_shows(cx: &mut TestAppContext) {
        let folders = HashMap::from([
            ("~/".to_string(), vec![("src", true)]),
            ("~/src/".to_string(), vec![("repo", true)]),
            ("~/src/repo/".to_string(), vec![]),
        ]);
        let asked = Rc::new(RefCell::new(Vec::new()));
        let heard_asks = asked.clone();
        let list: List = Rc::new(move |path, _| {
            let folder = path.display().to_string();
            heard_asks.borrow_mut().push(folder.clone());
            let (answer, answered) = channel();
            let entries = folders.get(&folder).cloned().unwrap_or_default();
            let listing = Listing {
                path: folder.replace('~', "/home/me").trim_end_matches('/').to_string().into(),
                entries: entries
                    .into_iter()
                    .map(|(name, directory)| DirEntry { name: name.into(), directory })
                    .collect(),
            };
            answer.send(Ok(listing)).unwrap();
            Ok(answered)
        });
        let (card, _kept, cx) = open(kept(), None, Some((list, "Directory on box".into())), cx);
        let heard = record(cx, &card, ProjectEditorEvent::clone);
        let settle = |cx: &mut VisualTestContext| {
            card.update(cx, |card, cx| card.poll_remote_path(cx));
            cx.run_until_parked();
        };
        settle(cx);
        assert_eq!(asked.borrow().first().map(String::as_str), Some("~/"), "it opens on the machine's home");
        assert!(shows(cx, "folder-row:src"));
        assert!(card_text(cx).contains("Directory on box"));
        let taken = cx.update(|window, cx| card.update(cx, |card, cx| card.take_remote_path(window, cx)));
        assert_eq!(taken, None, "the browser does not hold the keyboard");

        click(cx, "folder-row:src");
        settle(cx);
        click(cx, "project-add-shown");
        settle(cx);
        assert_eq!(heard.take(), [ProjectEditorEvent::Adopt("/home/me/src".into())]);

        let browser = card.read_with(cx, |card, _| card.remote.clone().unwrap());
        let line = browser.read_with(cx, |browser, _| browser.field.clone());
        cx.update(|window, cx| window.focus(&line.read(cx).focus_handle(cx), cx));
        line.update(cx, |line, cx| line.set("~/src/re".into(), cx));
        settle(cx);
        let tabbed = cx.update(|window, cx| card.update(cx, |card, cx| card.tab_remote_path(window, cx)));
        assert!(tabbed, "Tab finished the name");
        settle(cx);
        let taken = cx.update(|window, cx| card.update(cx, |card, cx| card.take_remote_path(window, cx)));
        assert_eq!(taken, Some("/home/me/src/repo".into()));
    }
}
