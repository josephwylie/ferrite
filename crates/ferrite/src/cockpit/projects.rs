//! Projects' cockpit half (ADR 0030): the snapshot the nav and the bottom
//! bar read, refreshed off the UI thread; the palette's Projects commands
//! (herdr-projects' action menu); the Projects sheet's place on the board
//! and what it asks of the window; and the nav's project blocks.
//!
//! A project's scope is the focused Thread's: the project it heads or
//! works for, where upstream asked which Herdr workspace the pane was in.

use super::*;
use crate::float::{self, FloatPlace};
use crate::palette::{
    Act, Command as PaletteCommand, Row as PaletteRow, Section as PaletteSection,
};
use crate::projects::{self, Ask, Command, Model, Outcome, Place, ProjectsSheet, SheetEvent};
use gpui::component::notification::{Notification, NotificationType};
use gpui::component::WindowExt as _;
use gpui::Subscription;

/// The share of the board the sheet covers, as Herdr's popup pane did.
const SHEET_SHARE: f32 = 0.8;

/// What the cockpit holds for Projects.
#[derive(Default)]
pub(super) struct ProjectsState {
    /// Where the projects live; `None` in the test and demo constructors,
    /// so no suite reads the operator's own.
    pub place: Option<Place>,
    pub model: Model,
    /// The project the nav shows alone (`focus`), coordinator first.
    pub focus: Option<String>,
    pub sheet: Option<Entity<ProjectsSheet>>,
    events: Option<Subscription>,
    /// What a sheet's act said, for the next frame's toast.
    toasts: Vec<(String, String, bool)>,
}

impl ProjectsState {
    pub(super) fn new(place: Option<Place>) -> Self {
        Self {
            place,
            ..Default::default()
        }
    }

    /// Take the sheet down; whether one was up.
    pub(super) fn drop_sheet(&mut self) -> bool {
        self.events = None;
        self.sheet.take().is_some()
    }
}

impl CockpitView {
    /// Read the snapshot every `popup::REFRESH`, off the UI thread; the
    /// window redraws only when it changed.
    pub(super) fn start_projects_refresh(place: Place, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            let reading = place.clone();
            let model = cx
                .background_spawn(async move { Model::load(&reading) })
                .await;
            let alive = this.update(cx, |view, cx| {
                if view.projects.model != model {
                    view.projects.model = model;
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
            cx.background_executor()
                .timer(ferrite_projects::popup::REFRESH)
                .await;
        })
        .detach();
    }

    /// The focused Thread's project, if it has one.
    fn projects_scope(&self) -> Option<String> {
        let thread = self.focused_thread()?;
        self.projects
            .model
            .project_of(thread)
            .map(|project| project.slug.clone())
    }

    /// The projects shortcut: the popup, open or shut.
    pub(super) fn toggle_projects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.projects.sheet.is_some() {
            self.close_projects(cx);
        } else {
            self.run_projects_command(Command::Popup, window, cx);
        }
    }

    pub(super) fn close_projects(&mut self, cx: &mut Context<Self>) {
        if self.projects.drop_sheet() {
            cx.notify();
        }
    }

    /// Put `sheet` over the board, holding the keyboard.
    fn show_projects_sheet(
        &mut self,
        sheet: impl FnOnce(&mut Context<ProjectsSheet>) -> ProjectsSheet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_for_float();
        self.floats.palette = None;
        let sheet = cx.new(sheet);
        self.projects.events = Some(cx.subscribe_in(&sheet, window, Self::on_sheet_event));
        let focus = sheet.read(cx).focus.clone();
        self.projects.sheet = Some(sheet);
        window.focus(&focus, cx);
        cx.notify();
    }

    fn on_sheet_event(
        &mut self,
        _: &Entity<ProjectsSheet>,
        event: &SheetEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.clone() {
            SheetEvent::Close => self.close_projects(cx),
            SheetEvent::Jump(thread) => self.land_on_thread(thread, cx),
            SheetEvent::Open(path) => {
                if ferrite_projects::settings::is_text(&path) {
                    self.close_projects(cx);
                    self.open_beside(beside::Beside::Reader { path, line: None }, window, cx);
                } else {
                    crate::file_links::FileLink {
                        path,
                        location: None,
                    }
                    .open(window, cx);
                }
            }
            SheetEvent::Copy(text) => cx.write_to_clipboard(gpui::ClipboardItem::new_string(text)),
            SheetEvent::Notify { title, body } => {
                self.projects_toast(title, body, false, window, cx)
            }
        }
    }

    fn projects_toast(
        &mut self,
        title: String,
        body: String,
        error: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kind = if error {
            NotificationType::Error
        } else {
            NotificationType::Info
        };
        window.push_notification(
            Notification::new()
                .title(title)
                .message(body)
                .with_type(kind)
                .autohide(!error),
            cx,
        );
    }

    /// What a command's work said, once it is back: the next frame shows it.
    pub(super) fn present_projects_toasts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (title, body, error) in std::mem::take(&mut self.projects.toasts) {
            self.projects_toast(title, body, error, window, cx);
        }
    }

    /// Run `work` off the UI thread, and toast what it says.
    fn projects_work(
        &mut self,
        work: impl FnOnce() -> (String, String, bool) + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let said = cx.background_spawn(async move { work() }).await;
            this.update(cx, |view, cx| {
                view.projects.toasts.push(said);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// One of the palette's Projects commands.
    pub(super) fn run_projects_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(place) = self.projects.place.clone() else {
            return;
        };
        let scope = self.projects_scope();
        match command {
            Command::Popup => {
                self.show_projects_sheet(|cx| ProjectsSheet::popup(place, scope, cx), window, cx)
            }
            Command::Overview => {
                let slugs = match scope {
                    Some(slug) => vec![slug],
                    None => projects::pickable(&place),
                };
                self.show_projects_sheet(|cx| ProjectsSheet::overview(place, slugs, cx), window, cx)
            }
            Command::New => self.show_projects_sheet(
                |cx| ProjectsSheet::ask(place, Ask::new_project(), cx),
                window,
                cx,
            ),
            Command::Verb(verb) => match scope {
                Some(slug) => {
                    self.projects_work(move || said(projects::run_verb(&place, verb, &slug)), cx)
                }
                None => {
                    let slugs = projects::pickable(&place);
                    match slugs.as_slice() {
                        [] => {
                            let body = format!("there are no projects in {}", place.root.display());
                            self.projects_toast("Projects".into(), body, true, window, cx);
                        }
                        [only] => {
                            let slug = only.clone();
                            self.projects_work(
                                move || said(projects::run_verb(&place, verb, &slug)),
                                cx,
                            )
                        }
                        _ => self.show_projects_sheet(
                            |cx| ProjectsSheet::pick(place, verb, slugs, cx),
                            window,
                            cx,
                        ),
                    }
                }
            },
            Command::Adopt => {
                let Some(thread) = self.focused_thread() else {
                    self.projects_toast(
                        "Projects".into(),
                        "no Thread is focused".into(),
                        true,
                        window,
                        cx,
                    );
                    return;
                };
                if let Some(refusal) = projects::adopt_refusal(&self.projects.model, thread) {
                    self.projects_toast("Projects".into(), refusal, true, window, cx);
                    return;
                }
                let label = self.facts.name(thread).to_string();
                let cwd = self
                    .thread_path(thread)
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
                self.show_projects_sheet(
                    |cx| ProjectsSheet::ask(place, Ask::adopt(thread, label, cwd), cx),
                    window,
                    cx,
                )
            }
            Command::Focus => match scope {
                Some(slug) => self.focus_nav_on_project(Some(slug), cx),
                None => self.projects_toast(
                    "Projects".into(),
                    "this Thread does not belong to a project".into(),
                    true,
                    window,
                    cx,
                ),
            },
            Command::Unfocus => self.focus_nav_on_project(None, cx),
            Command::Configure => self.projects_work(
                move || match place.run(&projects::CONFIGURE) {
                    (true, _) => (
                        projects::CONFIGURED_TITLE.into(),
                        projects::CONFIGURED_BODY.into(),
                        false,
                    ),
                    (false, error) => (projects::CONFIGURE_FAILED_TITLE.into(), error, true),
                },
                cx,
            ),
            Command::Doctor => self.projects_work(
                move || {
                    let (healthy, _) = place.run(&["doctor"]);
                    (
                        projects::DOCTOR_TITLE.into(),
                        projects::doctor_body(healthy).into(),
                        !healthy,
                    )
                },
                cx,
            ),
        }
        cx.notify();
    }

    /// `focus`/`unfocus`: the nav shows only `slug`'s block, coordinator
    /// first, its Threads by need — or everything again.
    pub(crate) fn focus_nav_on_project(&mut self, slug: Option<String>, cx: &mut Context<Self>) {
        if self.projects.focus != slug {
            self.projects.focus = slug;
            cx.notify();
        }
    }

    /// The palette's Projects rows, while there is a place to read.
    pub(super) fn palette_projects_rows(&self) -> Vec<PaletteRow> {
        if self.projects.place.is_none() {
            return Vec::new();
        }
        let in_project = self.projects_scope().is_some();
        Command::ALL
            .into_iter()
            .filter(|command| match command {
                Command::Focus => in_project,
                Command::Unfocus => self.projects.focus.is_some(),
                Command::Adopt => self.focused_thread().is_some() && !in_project,
                _ => true,
            })
            .map(|command| PaletteRow {
                section: PaletteSection::Commands,
                name: command.title().into(),
                context: command.detail().into(),
                status: None,
                detail: None,
                keys: (command == Command::Popup)
                    .then(|| crate::components::bound_chord("projects::Toggle"))
                    .flatten(),
                act: Act::Command(PaletteCommand::Projects(command)),
            })
            .collect()
    }

    /// The sheet over the board: a veil, and the sheet at 80% of the
    /// board, centred, as Herdr hung its popup pane.
    pub(super) fn projects_float(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(sheet) = self.projects.sheet.clone() else {
            return Vec::new();
        };
        let board = self.board_bounds(window);
        let (width, height) = (board.w * SHEET_SHARE, board.h * SHEET_SHARE);
        sheet.update(cx, |sheet, _| sheet.fit(width, height));
        let veil = deferred(
            div()
                .debug_selector(|| "projects-veil".into())
                .absolute()
                .left(px(board.x))
                .top(px(board.y))
                .w(px(board.w))
                .h(px(board.h))
                .bg(rgba(crate::theme::VEIL))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.close_projects(cx);
                    }),
                ),
        )
        .with_priority(float::VEIL_PRIORITY)
        .into_any_element();
        let anchor = gpui::Bounds::new(
            gpui::point(px(board.x), px(board.y)),
            gpui::size(px(board.w), px(board.h)),
        );
        let float = float::hang(
            sheet.into(),
            anchor,
            FloatPlace::BoardTop {
                top: (board.h - height) / 2.0,
                width,
            },
            window,
            cx,
        );
        vec![veil, float]
    }

    /// `projects: N need you`, while the ticker runs and someone does.
    pub(super) fn projects_bar_word(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let word = self.projects.model.needs_you.clone()?;
        Some(
            div()
                .id("bottom-bar-projects")
                .debug_selector(|| "bottom-bar-projects".into())
                .flex_shrink_0()
                .text_color(rgb(crate::theme::YELLOW))
                .cursor_pointer()
                .child(SharedString::from(word))
                .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    view.run_projects_command(Command::Popup, window, cx);
                }))
                .into_any_element(),
        )
    }

    /// The nav's project blocks, from the snapshot, while the ticker runs:
    /// each project with a Thread here, by slug; focused, that one alone.
    pub(super) fn project_blocks(
        &self,
        row: impl Fn(ThreadId) -> nav::ThreadRow,
    ) -> Vec<nav::ProjectBlock> {
        let model = &self.projects.model;
        if !model.ticker {
            return Vec::new();
        }
        let known = |thread: &ThreadId| {
            self.facts.get(*thread).is_some() || self.cockpit.thread(*thread).is_some()
        };
        model
            .projects
            .iter()
            .filter(|project| {
                self.projects
                    .focus
                    .as_ref()
                    .is_none_or(|slug| *slug == project.slug)
            })
            .filter_map(|project| {
                let head = project.coordinator.filter(known).map(&row);
                let rows: Vec<(nav::ThreadRow, Option<SharedString>)> = project
                    .threads
                    .iter()
                    .filter_map(|entry| {
                        let thread = entry.thread.filter(known)?;
                        let sub = Some(SharedString::from(entry.sub.clone()))
                            .filter(|sub| !sub.is_empty());
                        Some((
                            nav::ThreadRow {
                                name: entry.display.clone().into(),
                                ..row(thread)
                            },
                            sub,
                        ))
                    })
                    .collect();
                (head.is_some() || !rows.is_empty()).then(|| nav::ProjectBlock {
                    slug: project.slug.clone().into(),
                    name: project.name.clone().into(),
                    summary: project.summary.clone().into(),
                    head_sub: Some(SharedString::from(project.coordinator_sub.clone()))
                        .filter(|sub| !sub.is_empty()),
                    head,
                    rows,
                })
            })
            .collect()
    }

    /// The project blocks at the top of the tree: each its head (a press
    /// lands on the coordinator) and its Threads' rows, each over its
    /// sub-line.
    pub(super) fn project_blocks_element(
        &self,
        blocks: &[nav::ProjectBlock],
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        blocks
            .iter()
            .enumerate()
            .map(|(index, block)| {
                let head = nav::project_head(index, block);
                let head = match block.head.as_ref().map(|row| row.thread) {
                    Some(thread) => {
                        head.on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                            view.land_on_thread(thread, cx)
                        }))
                    }
                    None => head,
                };
                let mut section = nav::section(index == 0)
                    .child(head)
                    .children(block.head_sub.clone().map(nav::sub_line));
                for (row, sub) in &block.rows {
                    section = section
                        .child(self.thread_element(row, None, nav::RowPlace::Root, cx))
                        .children(sub.clone().map(nav::sub_line));
                }
                section.into_any_element()
            })
            .collect()
    }
}

/// A verb's outcome as a toast: its title, its words, whether it failed.
fn said(outcome: Outcome) -> (String, String, bool) {
    match outcome {
        Outcome::Done { title, body } => (title, body, false),
        Outcome::Refused(error) => ("Projects".into(), error, true),
    }
}

/// Every project's Threads are its own rows: the nav lists them nowhere
/// else.
pub(super) fn claimed(blocks: &[nav::ProjectBlock]) -> std::collections::HashSet<ThreadId> {
    blocks.iter().flat_map(nav::ProjectBlock::threads).collect()
}

/// Whether the nav is focused on a project it can show.
pub(super) fn focused(state: &ProjectsState, blocks: &[nav::ProjectBlock]) -> bool {
    state
        .focus
        .as_ref()
        .is_some_and(|slug| blocks.iter().any(|block| block.slug == slug.as_str()))
}
