//! Projects' cockpit half: the board's sidebar the nav and the bottom bar
//! read (ADR 0031), refreshed off the UI thread; and, from ADR 0030, the
//! palette's Projects commands (herdr-projects' action menu), the Projects
//! sheet's place on the board and what it asks of the window.
//!
//! A project's scope is the focused Thread's: the project it heads or
//! works for, where upstream asked which Herdr workspace the pane was in.

use super::*;
use crate::float::{self, FloatPlace};
use crate::palette::{
    Act, Command as PaletteCommand, Row as PaletteRow, Section as PaletteSection,
};
use crate::projects::{self, Ask, Command, Jump, Model, Outcome, Place, ProjectsSheet, SheetEvent};
use crate::sidebar::{self, Sidebar};
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
    /// What herdr's sidebar would show of the board: the nav's task rows
    /// and the bottom bar's tally.
    pub sidebar: Sidebar,
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

    /// Read the board's sidebar every `sidebar::REFRESH`, off the UI
    /// thread, and at once when an open Thread's Attention changes, as the
    /// upstream writer tells herdr at once; the window redraws only when it
    /// changed. Nothing is read while the window is on another machine,
    /// whose board tells its own.
    pub(super) fn start_sidebar_refresh(cx: &mut Context<Self>) {
        const LOOK: std::time::Duration = std::time::Duration::from_millis(100);
        cx.spawn(async move |this, cx| loop {
            let Ok(pulse) = this.update(cx, |view, cx| view.board_pulse(cx)) else {
                break;
            };
            let here = pulse.is_some();
            let sidebar = cx
                .background_spawn(async move { here.then(sidebar::read).unwrap_or_default() })
                .await;
            let alive = this.update(cx, |view, cx| {
                if view.projects.sidebar != sidebar {
                    view.projects.sidebar = sidebar;
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
            for _ in 0..(sidebar::REFRESH.as_millis() / LOOK.as_millis()) {
                cx.background_executor().timer(LOOK).await;
                let changed = this.update(cx, |view, cx| view.board_pulse(cx) != pulse);
                if changed.unwrap_or(true) {
                    break;
                }
            }
        })
        .detach();
    }

    /// Each open Thread's Attention as the Server reckons it (a Decision
    /// waiting, a turn running, an unseen Notice), from the mirror alone:
    /// what the board's agent list says changes with it. `None` while the
    /// window is on another machine.
    fn board_pulse(&self, cx: &gpui::App) -> Option<Vec<(ThreadId, Attention)>> {
        if self.away_machine(cx).is_some() {
            return None;
        }
        let notices = self.cockpit.notifications();
        let threads = self.cockpit.threads().into_iter();
        Some(
            threads
                .filter_map(|thread| {
                    let view = self.cockpit.thread(thread)?;
                    let attention = if view.pending().is_some() {
                        Attention::Blocked
                    } else if view.needs_queue() || view.background_work() {
                        Attention::Working
                    } else if notices.attention(thread) {
                        Attention::Done
                    } else {
                        Attention::Idle
                    };
                    Some((thread, attention))
                })
                .collect(),
        )
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
        #[cfg(test)]
        super::drawn::toggled_projects();
        if self.projects.sheet.is_some() {
            self.close_projects(cx);
        } else {
            self.run_projects_command(Command::Popup, window, cx);
        }
    }

    /// A reference scene's sheet: open on the section `tabs` along from
    /// threads, fitted to the board as it will draw, every key answered,
    /// and its popup thread stopped (`ProjectsSheet::stop`).
    #[cfg(feature = "visual-reference")]
    pub(super) fn stage_projects_sheet(
        &mut self,
        tabs: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use ferrite_projects::keys::{KeyCode, KeyEvent, KeyModifiers};
        self.toggle_projects(window, cx);
        let Some(sheet) = self.projects.sheet.clone() else {
            return;
        };
        let board = self.board_bounds(window);
        sheet.update(cx, |sheet, cx| {
            sheet.fit(board.w * SHEET_SHARE, board.h * SHEET_SHARE);
            for _ in 0..tabs {
                sheet.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), cx);
            }
            sheet.settle(cx);
            sheet.stop();
        });
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
            SheetEvent::Jump(jump) => self.jump_to(jump, window, cx),
            SheetEvent::Refused(why) => {
                self.projects_toast("Projects".into(), why, true, window, cx)
            }
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

    /// Land on the Thread a popup row jumps to: here, when it runs on the
    /// machine the window is on; else the window goes to its machine and
    /// lands on it there once attached.
    pub(super) fn jump_to(&mut self, jump: Jump, window: &mut Window, cx: &mut Context<Self>) {
        let machines = self.machines.as_ref().map(|machines| machines.read(cx));
        let index = match machines {
            Some(machines) => machines.named(&jump.machine),
            None => jump.machine.is_empty().then_some(0),
        };
        let here = machines.map_or(Some(0), |machines| machines.index(&machines.selected_key()));
        match index {
            Some(index) if Some(index) == here => self.land_on_thread(jump.thread, cx),
            Some(index) => cx.emit(GoToMachine {
                index,
                open: Some(Landing::Thread(jump.thread)),
            }),
            None => {
                let body = format!(
                    "{} runs on `{}`, which is not one of this window's machines",
                    jump.thread, jump.machine
                );
                self.projects_toast("Projects".into(), body, true, window, cx)
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
                // Upstream adopts only a running local agent pane: a Thread
                // on another machine is not this computer's Server's.
                if let Some(machine) = self.away_machine(cx) {
                    let body = format!(
                        "this window is on `{machine}`; only a running local agent pane can be adopted"
                    );
                    self.projects_toast("Projects".into(), body, true, window, cx);
                    return;
                }
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
                self.adopt_if_adoptable(place, thread, window, cx)
            }
            Command::Configure => self.projects_work(move || projects::configure(&place), cx),
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

    /// "Continue this Thread as a project": the adoptable-agent check off
    /// the UI thread, then the dialog — or its refusal, before any input.
    fn adopt_if_adoptable(
        &mut self,
        place: Place,
        thread: ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |view, cx| {
            let checking = place.clone();
            let adoptable = cx
                .background_spawn(async move { projects::adoptable(&checking, thread) })
                .await;
            view.update_in(cx, |view, window, cx| {
                view.adopt_or_refuse(place, thread, adoptable, window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// The adopt dialog once the check passed; else its refusal, and no
    /// dialog.
    pub(super) fn adopt_or_refuse(
        &mut self,
        place: Place,
        thread: ThreadId,
        adoptable: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match adoptable {
            Ok(()) => self.ask_adopt(place, thread, window, cx),
            Err(refusal) => self.projects_toast("Projects".into(), refusal, true, window, cx),
        }
    }

    /// The machine the window is on, by name, when that is not this
    /// computer.
    pub(super) fn away_machine(&self, cx: &gpui::App) -> Option<String> {
        let machines = self.machines.as_ref()?.read(cx);
        let index = machines.index(&machines.selected_key())?;
        machines.machine(index).map(|machine| machine.name.clone())
    }

    /// The adopt dialog for `thread`, its name and directory offered.
    fn ask_adopt(
        &mut self,
        place: Place,
        thread: ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    /// The palette's Projects rows, while there is a place to read.
    pub(super) fn palette_projects_rows(&self) -> Vec<PaletteRow> {
        if self.projects.place.is_none() {
            return Vec::new();
        }
        let in_project = self.projects_scope().is_some();
        Command::ALL
            .into_iter()
            .filter(|command| match command {
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

    /// The board header's tally (`3 need you · 1 ready · 2 working`), while
    /// the board has tasks; a press opens the board.
    pub(super) fn projects_bar_word(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let tally = &self.projects.sidebar.tally;
        #[cfg(test)]
        super::drawn::record_bar_tally((!tally.is_empty()).then(|| tally.clone()));
        (!tally.is_empty()).then(|| {
            div()
                .id("bottom-bar-projects")
                .debug_selector(|| "bottom-bar-projects".into())
                .flex_shrink_0()
                .text_color(rgb(crate::theme::CHROME_MUTED))
                .cursor_pointer()
                .child(SharedString::from(tally.clone()))
                .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    window.dispatch_action(Box::new(projects::Toggle), cx);
                }))
                .into_any_element()
        })
    }
}

/// A verb's outcome as a toast: its title, its words, whether it failed.
fn said(outcome: Outcome) -> (String, String, bool) {
    match outcome {
        Outcome::Done { title, body } => (title, body, false),
        Outcome::Refused(error) => ("Projects".into(), error, true),
    }
}
