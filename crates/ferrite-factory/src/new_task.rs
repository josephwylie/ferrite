//! The Factory's New task sheet as a view of its own (ADR 0041), made when
//! it opens and dropped once it has gone (ADR 0042). It owns its two
//! fields, which of them holds the keyboard, the process chosen, the
//! request that makes a second Start the same Task, and a refusal. It draws
//! the process's name from what the window answers for its Project and
//! process (`NewTaskInputs`, through a `Source`), asked for each frame, and
//! what the operator asks of it it says as a `NewTaskEvent`. Starting the
//! Task is the Cockpit's (`start_new_task`), which reads what it sends
//! (`asked`) and hands back a refusal (`refuse`).
//!
//! What was typed outlives the view: a close leaves it on the sheet's
//! Factory as a `KeptNewTask`, plain values that seed the next sheet.

use std::sync::atomic::{AtomicU64, Ordering};

use ferrite_core::draft_content::DraftContent;
use ferrite_core::factory_board::WorkflowId;
use ferrite_core::ProjectKey;
use ferrite_pane::composer::{Composer, Role};
use ferrite_ui::source::Source;
use gpui::prelude::*;
use gpui::{
    div, px, App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseButton,
    MouseDownEvent, Pixels, Point, SharedString, Window,
};

use crate::picker::Trigger;

static REQUESTS: AtomicU64 = AtomicU64::new(1);

/// The New task sheet as it was closed, kept on its Factory for when it
/// opens again. A refusal and which field held the keyboard are not kept:
/// the sheet reopens on its instructions with nothing refused.
pub struct KeptNewTask {
    instructions: DraftContent,
    pr: DraftContent,
    workflow: Option<WorkflowId>,
    request: String,
}

impl KeptNewTask {
    /// Nothing typed yet, and a request of its own.
    pub fn new() -> Self {
        Self {
            instructions: DraftContent::default(),
            pr: DraftContent::default(),
            workflow: None,
            request: format!(
                "window-{}-{}",
                std::process::id(),
                REQUESTS.fetch_add(1, Ordering::Relaxed)
            ),
        }
    }
}

impl Default for KeptNewTask {
    fn default() -> Self {
        Self::new()
    }
}

/// What the sheet asks the window about: its Project, and the process
/// chosen.
pub type NewTaskQuery = (ProjectKey, Option<WorkflowId>);

/// What the sheet draws from, as the window has it this frame. The window
/// answers none once the sheet's Factory no longer shows.
pub struct NewTaskInputs {
    /// The process's name: the Workflow at its latest version, or one-off.
    pub process: SharedString,
}

/// What a Start sends: the instructions and the PR as typed, trimmed, the
/// process chosen and the request that makes a second Start the same Task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    pub project: ProjectKey,
    pub instructions: String,
    /// None when the field is empty.
    pub pr: Option<String>,
    pub request: String,
    pub workflow: Option<WorkflowId>,
}

/// What the sheet asks of the window.
#[derive(Clone)]
pub enum NewTaskEvent {
    /// Start pressed.
    Start,
    /// The Process picker pressed, at the pointer.
    PickProcess(Trigger, Point<Pixels>),
    /// The close button closed it.
    Closed,
}

pub struct NewTaskView {
    inputs: Source<NewTaskInputs, NewTaskQuery>,
    /// The Project whose Factory opened it: the sheet is that Factory's.
    project: ProjectKey,
    instructions: Entity<Composer>,
    /// Its optional pull request, `owner/repository#number` or a URL
    /// (#223).
    pr: Entity<Composer>,
    /// The PR field holds the keyboard, else the instructions do.
    pr_focused: bool,
    /// The process it runs: a Workflow's latest version, else one-off.
    workflow: Option<WorkflowId>,
    /// What a Start sends, so a second press is the same Task.
    request: String,
    error: Option<SharedString>,
}

impl EventEmitter<NewTaskEvent> for NewTaskView {}

impl NewTaskView {
    pub fn new(
        inputs: Source<NewTaskInputs, NewTaskQuery>,
        project: ProjectKey,
        kept: KeptNewTask,
        cx: &mut Context<Self>,
    ) -> Self {
        let field = |content: DraftContent, cx: &mut Context<Self>| {
            let field = cx.new(Composer::new);
            field.update(cx, |field, cx| {
                field.set_role(Role::Field(None), cx);
                field.load(content, cx);
            });
            field
        };
        Self {
            inputs,
            project,
            instructions: field(kept.instructions, cx),
            pr: field(kept.pr, cx),
            pr_focused: false,
            workflow: kept.workflow,
            request: kept.request,
            error: None,
        }
    }

    /// The Project whose Factory the sheet is.
    pub fn project(&self) -> ProjectKey {
        self.project
    }

    /// The process chosen: a Workflow, else one-off.
    pub fn workflow(&self) -> Option<&WorkflowId> {
        self.workflow.as_ref()
    }

    /// The refusal the sheet shows, if any.
    pub fn error(&self) -> Option<&SharedString> {
        self.error.as_ref()
    }

    /// What it holds, for its Factory to keep once it closes.
    pub fn kept(&self, cx: &App) -> KeptNewTask {
        KeptNewTask {
            instructions: self.instructions.read(cx).content(),
            pr: self.pr.read(cx).content(),
            workflow: self.workflow.clone(),
            request: self.request.clone(),
        }
    }

    /// What a Start sends, as the sheet holds it now.
    pub fn asked(&self, cx: &App) -> Asked {
        Asked {
            project: self.project,
            instructions: self.instructions.read(cx).text().trim().to_string(),
            pr: Some(self.pr.read(cx).text().trim().to_string()).filter(|pr| !pr.is_empty()),
            request: self.request.clone(),
            workflow: self.workflow.clone(),
        }
    }

    /// What its two fields hold, as typed: the instructions and the PR.
    pub fn typed(&self, cx: &App) -> (String, String) {
        (self.instructions.read(cx).text().to_string(), self.pr.read(cx).text().to_string())
    }

    /// The process it runs, chosen in the window's picker.
    pub fn set_workflow(&mut self, workflow: Option<WorkflowId>, cx: &mut Context<Self>) {
        self.workflow = workflow;
        cx.notify();
    }

    /// What the window said of a Start: a refusal, or none.
    pub fn refuse(&mut self, error: Option<SharedString>, cx: &mut Context<Self>) {
        self.error = error;
        cx.notify();
    }

    /// The field that holds the keyboard while the sheet is up.
    pub fn focus(&self, cx: &App) -> FocusHandle {
        let field = if self.pr_focused {
            &self.pr
        } else {
            &self.instructions
        };
        field.read(cx).focus_handle(cx)
    }

    fn focus_pr(&mut self, pr: bool, cx: &mut Context<Self>) {
        self.pr_focused = pr;
        cx.notify();
    }

    /// The card, for the sheet host.
    pub fn card(&self, window: &Window, cx: &mut Context<Self>) -> Option<ferrite_ui::sheet::Drawn> {
        let NewTaskInputs { process } = self.inputs.read(&(self.project, self.workflow.clone()), cx)?;
        let close = ferrite_ui::sheet::sheet_close("new-task-close", "Close New task", cx)
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(NewTaskEvent::Closed);
            }));
        let focused = self.instructions.read(cx).focus_handle(cx).is_focused(window);
        let pr_focused = self.pr.read(cx).focus_handle(cx).is_focused(window);
        let process_trigger = Trigger::default();
        let mut body = ferrite_ui::form::body()
            .child(
                div()
                    .id("new-task-instructions")
                    .debug_selector(|| "new-task-instructions".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| view.focus_pr(false, cx)),
                    )
                    .child(ferrite_ui::field::text_area(
                        "Instructions".into(),
                        self.instructions.clone(),
                        focused,
                        "new-task-instructions-focused",
                    )),
            )
            .child(
                div()
                    .id("new-task-pr")
                    .debug_selector(|| "new-task-pr".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| view.focus_pr(true, cx)),
                    )
                    .child(ferrite_ui::field::text_field(
                        "Pull request (optional): owner/repository#number or its URL".into(),
                        self.pr.clone(),
                        pr_focused,
                        "new-task-pr-focused",
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(ferrite_ui::theme::SPACE_1))
                    .child(ferrite_ui::components::section_label("Process"))
                    .child(
                        div()
                            .id("new-task-process")
                            .debug_selector(|| "new-task-process".into())
                            .flex()
                            .items_center()
                            .gap(px(ferrite_ui::theme::SPACE_1))
                            .h(px(ferrite_ui::theme::CONTROL_H))
                            .px(px(ferrite_ui::theme::SPACE_2))
                            .rounded(px(ferrite_ui::theme::R_CONTROL))
                            .border_1()
                            .border_color(ferrite_ui::theme::paint::FIELD_RING)
                            .cursor_pointer()
                            .child(div().flex_1().child(process))
                            .child(ferrite_ui::icons::icon(
                                ferrite_ui::icons::CHEVRON_DOWN,
                                ferrite_ui::theme::ROW_ICON,
                                ferrite_ui::theme::INK_SUBTLE,
                            ))
                            .relative()
                            .child(process_trigger.probe())
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    cx.emit(NewTaskEvent::PickProcess(
                                        process_trigger.clone(),
                                        event.position,
                                    ));
                                }),
                            ),
                    ),
            );
        if let Some(error) = self.error.clone() {
            body = body.child(
                ferrite_ui::form::error_line(error).debug_selector(|| "new-task-error".into()),
            );
        }
        let start = ferrite_ui::form::primary_button("new-task-start", "Start", false, cx)
            .debug_selector(|| "new-task-start".into())
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(NewTaskEvent::Start);
            }));
        let card = ferrite_ui::form::card(f32::from(window.viewport_size().height))
            .id("new-task-card")
            .debug_selector(|| "new-task-card".into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(ferrite_ui::form::head("New task".into(), close))
            .child(body)
            .child(ferrite_ui::form::footer(
                div().child(
                    ferrite_ui::components::text_ui()
                        .text_color(gpui::rgb(ferrite_ui::theme::INK_SUBTLE))
                        .child("Runs in its own worktree and branch."),
                ),
                div().flex().items_center().child(start),
            ));
        Some(ferrite_ui::sheet::Drawn { card, above: None })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use ferrite_ui::testing::{click, mount, record, shows};
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;

    /// The sheet host's part in it: the card, where the host lays it.
    struct Host(Entity<NewTaskView>);

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let drawn = self.0.update(cx, |sheet, cx| sheet.card(window, cx));
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .children(drawn.map(|drawn| drawn.card))
        }
    }

    /// What the window keeps of the Factory the sheet is on, as a test
    /// keeps it: whether it still shows, and its Workflows by id.
    struct Kept {
        showing: bool,
        workflows: Vec<(&'static str, &'static str)>,
        /// Each Project and process the sheet asked the window about.
        asked: Rc<RefCell<Vec<NewTaskQuery>>>,
    }

    fn kept() -> Kept {
        Kept {
            showing: true,
            workflows: vec![("w-1", "Fix flakes"), ("w-2", "Release")],
            asked: Rc::default(),
        }
    }

    fn project() -> ProjectKey {
        ProjectKey::fixture(1, ferrite_core::project::ProjectId(4))
    }

    /// The sheet on `kept`'s Factory, as the sheet host draws it, from
    /// `seed`; the window names a Workflow's process by its name.
    fn open(
        kept: Kept,
        seed: KeptNewTask,
        cx: &mut TestAppContext,
    ) -> (Entity<NewTaskView>, Entity<Kept>, &mut VisualTestContext) {
        let kept = cx.new(|_| kept);
        let inputs = Source::of(&kept, |kept: &Kept, query: &NewTaskQuery, _| {
            kept.asked.borrow_mut().push(query.clone());
            kept.showing.then(|| {
                let named = query.1.as_deref().and_then(|id| {
                    kept.workflows.iter().find(|(each, _)| *each == id).map(|(_, name)| *name)
                });
                NewTaskInputs {
                    process: named.unwrap_or("One-off: a single agent").into(),
                }
            })
        });
        let (host, cx) = mount(cx, |_, cx| {
            Host(cx.new(|cx| NewTaskView::new(inputs, project(), seed, cx)))
        });
        let sheet = host.read_with(cx, |host, _| host.0.clone());
        (sheet, kept, cx)
    }

    /// The window's focus pump: the keyboard to the field the sheet names.
    fn pump(sheet: &Entity<NewTaskView>, cx: &mut VisualTestContext) {
        cx.update(|window, cx| {
            let focus = sheet.read(cx).focus(cx);
            window.focus(&focus, cx);
        });
        cx.run_until_parked();
    }

    fn process(cx: &mut VisualTestContext) -> Vec<SharedString> {
        cx.update(|window, _| window.debug_text("new-task-process")).unwrap_or_default()
    }

    /// The card: its two fields, the process and its buttons.
    #[gpui::test]
    fn the_card_draws_its_fields_the_process_and_its_buttons(cx: &mut TestAppContext) {
        let (_sheet, _kept, cx) = open(kept(), KeptNewTask::new(), cx);
        for drawn in [
            "new-task-card",
            "new-task-instructions",
            "new-task-pr",
            "new-task-process",
            "new-task-start",
            "new-task-close",
        ] {
            assert!(shows(cx, drawn), "{drawn}");
        }
        assert!(!shows(cx, "new-task-error"), "nothing refused");
    }

    /// The window is asked about the sheet's Project and the process
    /// chosen, and the process is drawn as it answers.
    #[gpui::test]
    fn the_process_is_drawn_as_the_window_names_the_one_chosen(cx: &mut TestAppContext) {
        let (sheet, kept, cx) = open(kept(), KeptNewTask::new(), cx);
        assert!(process(cx).iter().any(|line| line.contains("One-off")));
        sheet.update(cx, |sheet, cx| sheet.set_workflow(Some("w-2".into()), cx));
        cx.run_until_parked();
        let asked = kept.read_with(cx, |kept, _| kept.asked.borrow().last().cloned());
        assert_eq!(asked, Some((project(), Some("w-2".into()))));
        assert!(process(cx).iter().any(|line| line.contains("Release")), "{:?}", process(cx));
        assert_eq!(sheet.read_with(cx, |sheet, _| sheet.workflow().cloned()).as_deref(), Some("w-2"));
    }

    /// The instructions hold the keyboard as it opens; a press on the PR
    /// field takes it there and one on the instructions brings it back,
    /// and what is typed lands in the field that holds it.
    #[gpui::test]
    fn a_press_gives_the_keyboard_to_the_field_pressed(cx: &mut TestAppContext) {
        let (sheet, _kept, cx) = open(kept(), KeptNewTask::new(), cx);
        pump(&sheet, cx);
        assert!(shows(cx, "new-task-instructions-focused"), "the instructions, on open");
        cx.simulate_input("Fix");
        click(cx, "new-task-pr");
        pump(&sheet, cx);
        assert!(shows(cx, "new-task-pr-focused"), "the PR field, pressed");
        assert!(!shows(cx, "new-task-instructions-focused"));
        cx.simulate_input("acme/widgets#1");
        click(cx, "new-task-instructions");
        pump(&sheet, cx);
        assert!(shows(cx, "new-task-instructions-focused"), "back to the instructions");
        cx.simulate_input(" it");
        cx.run_until_parked();
        let asked = sheet.read_with(cx, |sheet, cx| sheet.asked(cx));
        assert_eq!(asked.instructions, "Fix it");
        assert_eq!(asked.pr.as_deref(), Some("acme/widgets#1"));
    }

    /// Start, the Process picker and the close button each say what was
    /// pressed; the picker says where its trigger was drawn.
    #[gpui::test]
    fn start_the_process_and_close_say_what_was_pressed(cx: &mut TestAppContext) {
        let (sheet, _kept, cx) = open(kept(), KeptNewTask::new(), cx);
        let heard = record(cx, &sheet, |event: &NewTaskEvent| match event {
            NewTaskEvent::Start => "start",
            NewTaskEvent::PickProcess(trigger, _) if trigger.bounds().is_some() => "process, under its trigger",
            NewTaskEvent::PickProcess(..) => "process, nowhere",
            NewTaskEvent::Closed => "closed",
        });
        click(cx, "new-task-start");
        click(cx, "new-task-process");
        click(cx, "new-task-close");
        assert_eq!(heard.take(), ["start", "process, under its trigger", "closed"]);
    }

    /// What a Start sends: the words trimmed, no PR for an empty field, the
    /// process chosen and the sheet's request.
    #[gpui::test]
    fn what_start_sends_is_what_the_sheet_holds(cx: &mut TestAppContext) {
        let seed = KeptNewTask::new();
        let request = seed.request.clone();
        let (sheet, _kept, cx) = open(kept(), seed, cx);
        pump(&sheet, cx);
        cx.simulate_input("  Write the notes \n");
        sheet.update(cx, |sheet, cx| sheet.set_workflow(Some("w-1".into()), cx));
        cx.run_until_parked();
        assert_eq!(
            sheet.read_with(cx, |sheet, cx| sheet.asked(cx)),
            Asked {
                project: project(),
                instructions: "Write the notes".into(),
                pr: None,
                request,
                workflow: Some("w-1".into()),
            }
        );
    }

    /// What it keeps seeds the next sheet: its words, its PR, its process
    /// and its request, with no refusal. A new sheet has a request of its
    /// own.
    #[gpui::test]
    fn what_it_keeps_seeds_the_next_sheet(cx: &mut TestAppContext) {
        let (sheet, factory, cx) = open(kept(), KeptNewTask::new(), cx);
        pump(&sheet, cx);
        cx.simulate_input("Fix the flake ");
        click(cx, "new-task-pr");
        pump(&sheet, cx);
        cx.simulate_input("acme/widgets#7");
        sheet.update(cx, |sheet, cx| {
            sheet.set_workflow(Some("w-1".into()), cx);
            sheet.refuse(Some("No.".into()), cx);
        });
        let (was, seed) = sheet.read_with(cx, |sheet, cx| (sheet.asked(cx), sheet.kept(cx)));
        let inputs = Source::of(&factory, |_: &Kept, _: &NewTaskQuery, _| None);
        let next = cx.new(|cx| NewTaskView::new(inputs, project(), seed, cx));
        next.read_with(cx, |next, cx| {
            assert_eq!(next.asked(cx), was, "the same words, process and request");
            assert_eq!(next.typed(cx), ("Fix the flake ".into(), "acme/widgets#7".into()), "as typed");
            assert_eq!(next.error(), None, "the refusal is not kept");
        });
        assert_ne!(KeptNewTask::new().request, KeptNewTask::new().request);
    }

    /// A refusal is drawn on the sheet until the window takes it back.
    #[gpui::test]
    fn a_refusal_is_drawn_on_the_sheet(cx: &mut TestAppContext) {
        let (sheet, _kept, cx) = open(kept(), KeptNewTask::new(), cx);
        sheet.update(cx, |sheet, cx| sheet.refuse(Some("Say what the task is.".into()), cx));
        cx.run_until_parked();
        let said = cx.update(|window, _| window.debug_text("new-task-error"));
        assert_eq!(said, Some(vec!["Say what the task is.".into()]));
        sheet.update(cx, |sheet, cx| sheet.refuse(None, cx));
        cx.run_until_parked();
        assert!(!shows(cx, "new-task-error"));
    }

    /// Once its Factory no longer shows, the sheet draws no card.
    #[gpui::test]
    fn with_its_factory_gone_it_draws_no_card(cx: &mut TestAppContext) {
        let (_sheet, kept, cx) = open(kept(), KeptNewTask::new(), cx);
        assert!(shows(cx, "new-task-card"));
        kept.update(cx, |kept, cx| {
            kept.showing = false;
            cx.notify();
        });
        cx.run_until_parked();
        assert!(!shows(cx, "new-task-card"));
    }
}
