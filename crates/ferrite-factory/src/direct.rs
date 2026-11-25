//! The Factory's Rework and Direct this task sheets as one view (ADR
//! 0041), made when one opens and dropped once it has gone (ADR 0042).
//! The two share a Task, one text field that holds the keyboard, a card
//! and its close; which of them it is, and what it chooses, is its
//! `Choice`. It draws its Task as the window has it this frame, read
//! through a `Source` it asks with its Project and Task, and what the
//! operator asks of it it says as a `DirectEvent`. Sending is the
//! Cockpit's (`send_rework`, `apply_redirect`), which reads what the
//! sheet holds (`asked`).
//!
//! What was typed and chosen outlives the view: a close leaves it on the
//! Factory, per Task, as a `KeptDirection` that seeds the next sheet.

use std::rc::Rc;

use ferrite_core::draft_content::DraftContent;
use ferrite_core::factory_board::{Destination, Status, StepId, Task, TaskId, ThenReturn};
use ferrite_core::ProjectKey;
use ferrite_pane::composer::{Composer, Role};
use ferrite_ui::source::Source;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseButton,
    MouseDownEvent, Pixels, Point, SharedString, Window,
};

use crate::picker::{picker_button, segmented, Trigger};

/// Which of a Task's two sheets: Rework, or Direct this task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectSheet {
    Rework,
    Redirect,
}

/// What a press of one of the sheets' choosers sets.
#[derive(Clone, Debug, PartialEq)]
pub enum TaskPick {
    ReworkTo(Destination),
    RedirectTo(StepId),
}

/// What the sheet asks the window about: its Project and its Task.
pub type DirectQuery = (ProjectKey, TaskId);

/// What a Rework sheet chooses.
#[derive(Clone, Default)]
pub struct ReworkChoice {
    /// The recipient; none, the Workflow's.
    pub to: Option<Destination>,
}

/// What a Direct this task sheet chooses.
#[derive(Clone, Default)]
pub struct RedirectChoice {
    pub to: Option<StepId>,
    pub skip: Vec<StepId>,
    pub then: ThenReturn,
}

/// Which of the two sheets it is, with what it chooses.
#[derive(Clone)]
pub enum Choice {
    Rework(ReworkChoice),
    Redirect(RedirectChoice),
}

/// One Task's two sheets as they were last closed, kept on its Factory.
/// A refusal, a send tried and the caret are not kept.
#[derive(Default)]
pub struct KeptDirection {
    feedback: DraftContent,
    rework: ReworkChoice,
    note: DraftContent,
    redirect: RedirectChoice,
}

impl KeptDirection {
    /// A send taken: what its sheet held is spent.
    pub fn spend(&mut self, sheet: DirectSheet) {
        match sheet {
            DirectSheet::Rework => {
                self.feedback = DraftContent::default();
                self.rework = ReworkChoice::default();
            }
            DirectSheet::Redirect => {
                self.note = DraftContent::default();
                self.redirect = RedirectChoice::default();
            }
        }
    }
}

pub struct DirectView {
    /// Its Task as the window has it now.
    inputs: Source<Task, DirectQuery>,
    /// The Project whose Factory opened it: the sheet is that Factory's.
    project: ProjectKey,
    task: TaskId,
    choice: Choice,
    /// Its one text field, which holds the keyboard while it is up: the
    /// Rework's feedback, or the redirect's note.
    field: Entity<Composer>,
    /// Its primary action was pressed: say what is missing.
    tried: bool,
}

/// What the sheet asks of the window.
#[derive(Clone)]
pub enum DirectEvent {
    /// Send for rework pressed.
    SendRework,
    /// Apply to this task pressed.
    ApplyRedirect,
    /// A chooser pressed: its rows, for the window's picker under it.
    OpenPicks(Rc<Vec<(ferrite_ui::menu::Item, TaskPick)>>, Trigger, Point<Pixels>),
    /// Its close button or Cancel pressed.
    Closed,
}

impl EventEmitter<DirectEvent> for DirectView {}

impl DirectView {
    pub fn new(
        inputs: Source<Task, DirectQuery>,
        project: ProjectKey,
        task: TaskId,
        sheet: DirectSheet,
        kept: &mut KeptDirection,
        cx: &mut Context<Self>,
    ) -> Self {
        let (text, choice) = match sheet {
            DirectSheet::Rework => (
                std::mem::take(&mut kept.feedback),
                Choice::Rework(std::mem::take(&mut kept.rework)),
            ),
            DirectSheet::Redirect => (
                std::mem::take(&mut kept.note),
                Choice::Redirect(std::mem::take(&mut kept.redirect)),
            ),
        };
        let field = cx.new(Composer::new);
        field.update(cx, |field, cx| {
            field.set_role(Role::Field(None), cx);
            field.load(text, cx);
        });
        Self {
            inputs,
            project,
            task,
            choice,
            field,
            tried: false,
        }
    }

    /// What it holds, back on its Task's `kept`.
    pub fn keep(&self, kept: &mut KeptDirection, cx: &App) {
        let text = self.field.read(cx).content();
        match &self.choice {
            Choice::Rework(choice) => {
                kept.feedback = text;
                kept.rework = choice.clone();
            }
            Choice::Redirect(choice) => {
                kept.note = text;
                kept.redirect = choice.clone();
            }
        }
    }

    /// The Project whose Factory the sheet is.
    pub fn project(&self) -> ProjectKey {
        self.project
    }

    /// The Task it acts on.
    pub fn task(&self) -> &TaskId {
        &self.task
    }

    /// What a send takes: its field's text, trimmed, and what it chose.
    pub fn asked(&self, cx: &App) -> (String, Choice) {
        (self.field.read(cx).text().trim().to_string(), self.choice.clone())
    }

    pub fn focus(&self, cx: &App) -> FocusHandle {
        self.field.read(cx).focus_handle(cx)
    }

    fn redirect_mut(&mut self) -> Option<&mut RedirectChoice> {
        match &mut self.choice {
            Choice::Redirect(choice) => Some(choice),
            Choice::Rework(_) => None,
        }
    }

    fn toggle_skip(&mut self, step: StepId, cx: &mut Context<Self>) {
        if let Some(choice) = self.redirect_mut() {
            match choice.skip.iter().position(|each| *each == step) {
                Some(at) => {
                    choice.skip.remove(at);
                }
                None => choice.skip.push(step),
            }
        }
        cx.notify();
    }

    fn set_then(&mut self, then: ThenReturn, cx: &mut Context<Self>) {
        if let Some(choice) = self.redirect_mut() {
            choice.then = then;
        }
        cx.notify();
    }

    /// A recipient or a Step chosen in the window's picker.
    pub fn pick(&mut self, pick: TaskPick, cx: &mut Context<Self>) {
        match (&mut self.choice, pick) {
            (Choice::Rework(choice), TaskPick::ReworkTo(to)) => choice.to = Some(to),
            (Choice::Redirect(choice), TaskPick::RedirectTo(to)) => {
                choice.skip.retain(|each| *each != to);
                choice.to = Some(to);
            }
            _ => {}
        }
        cx.notify();
    }

    fn send(&mut self, cx: &mut Context<Self>) {
        self.tried = true;
        cx.notify();
        cx.emit(match self.choice {
            Choice::Rework(_) => DirectEvent::SendRework,
            Choice::Redirect(_) => DirectEvent::ApplyRedirect,
        });
    }

    /// A chooser whose rows open in the window's picker.
    fn chooser(
        selector: &'static str,
        text: String,
        flagged: bool,
        picks: Vec<(ferrite_ui::menu::Item, TaskPick)>,
        cx: &mut Context<Self>,
    ) -> gpui::component::button::Button {
        let picks = Rc::new(picks);
        picker_button(
            selector,
            text,
            flagged,
            move |_: &mut Self, trigger, at, cx| {
                cx.emit(DirectEvent::OpenPicks(picks.clone(), trigger, at))
            },
            cx,
        )
    }

    /// The card, for the sheet host; none once its Task is gone.
    pub fn card(&self, window: &Window, cx: &mut Context<Self>) -> Option<ferrite_ui::sheet::Drawn> {
        use ferrite_ui::theme::*;
        let id = self.task.clone();
        let task = self.inputs.read(&(self.project, id.clone()), cx)?;
        let definition = &task.workflow.as_ref()?.definition;
        let tried = self.tried;
        let close = ferrite_ui::sheet::sheet_close("direct-close", "Close", cx).on_click(
            cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(DirectEvent::Closed);
            }),
        );
        let cancel = ferrite_ui::components::ghost_button("direct-cancel", "Cancel", cx)
            .debug_selector(|| "direct-cancel".into())
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(DirectEvent::Closed);
            }));
        let label = |text: &'static str| {
            div()
                .text_size(px(FS_META))
                .font_weight(W_LABEL)
                .text_color(rgb(INK_SUBTLE))
                .child(text)
        };
        let (key, label_text) = match self.choice {
            Choice::Rework(_) => ("rework-feedback", "Feedback"),
            Choice::Redirect(_) => ("redirect-note", "Feedback (optional)"),
        };
        let focused = self.field.read(cx).focus_handle(cx).is_focused(window);
        let input = div()
            .id(key)
            .debug_selector(move || key.to_string())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(ferrite_ui::field::text_field(
                label_text.into(),
                self.field.clone(),
                focused,
                "factory-field-focused",
            ));
        let (title, intro, body, primary) = match &self.choice {
            Choice::Rework(choice) => {
                let missing = tried && self.field.read(cx).text().trim().is_empty();
                let to = choice
                    .to
                    .clone()
                    .unwrap_or_else(|| definition.approval.rework_to.clone());
                let picks: Vec<(ferrite_ui::menu::Item, TaskPick)> = definition
                    .steps
                    .iter()
                    .map(|step| Destination::Step(step.id.clone()))
                    .chain([Destination::Coordinator])
                    .map(|each| {
                        (
                            ferrite_ui::menu::Item::new(read(&task, &each)).checked(each == to),
                            TaskPick::ReworkTo(each),
                        )
                    })
                    .collect();
                let mut body = div().flex().flex_col().gap(px(SPACE_3)).child(input);
                if missing {
                    body = body.child(
                        ferrite_ui::form::error_line(
                            "Write the feedback the agent works from.".into(),
                        )
                        .debug_selector(|| "rework-missing".into()),
                    );
                }
                body = body
                    .child(label("Send to"))
                    .child(Self::chooser("rework-to", read(&task, &to), false, picks, cx))
                    .child(meta(format!(
                        "The workflow sends rework to {}. Choosing another recipient affects this \
                         task only.",
                        read(&task, &definition.approval.rework_to)
                    )));
                let send = ferrite_ui::form::primary_button(
                    "rework-send",
                    "Send for rework",
                    false,
                    cx,
                )
                .debug_selector(|| "rework-send".into())
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.send(cx)
                }));
                (
                    format!("Rework task #{id}"),
                    "Your feedback is added to the task's context and travels with every later step.",
                    body,
                    send,
                )
            }
            Choice::Redirect(choice) => {
                let target = choice.to.as_deref().and_then(|step| definition.step(step));
                let picks = definition
                    .steps
                    .iter()
                    .map(|step| {
                        let current = task.step.as_deref() == Some(step.id.as_str());
                        let mut item = ferrite_ui::menu::Item::new(if current {
                            format!("{} \u{b7} current", step.name)
                        } else {
                            step.name.clone()
                        });
                        item = item.checked(target.is_some_and(|target| target.id == step.id));
                        (
                            item,
                            TaskPick::RedirectTo(step.id.clone()),
                        )
                    })
                    .collect();
                let mut body = div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_3))
                    .child(label("Send to"))
                    .child(Self::chooser(
                        "redirect-to",
                        target.map_or_else(
                            || "Choose an agent".to_string(),
                            |step| step.name.clone(),
                        ),
                        tried && target.is_none(),
                        picks,
                        cx,
                    ));
                if tried && target.is_none() {
                    body = body.child(
                        ferrite_ui::form::error_line(
                            "Choose the agent that takes the task.".into(),
                        )
                        .debug_selector(|| "redirect-missing".into()),
                    );
                }
                if task.status == Status::Working {
                    body = body.child(meta(
                        "The current attempt is superseded; any late result from it is ignored.",
                    ));
                }
                let mut skips = div()
                    .debug_selector(|| "redirect-skips".into())
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_1));
                for step in definition
                    .steps
                    .iter()
                    .filter(|step| Some(step.id.as_str()) != choice.to.as_deref())
                {
                    let on = choice.skip.contains(&step.id);
                    let key = format!("redirect-skip-{}{}", step.id, if on { "-on" } else { "" });
                    let id = step.id.clone();
                    skips = skips.child(
                        div()
                            .id(SharedString::from(key.clone()))
                            .debug_selector(move || key.clone())
                            .flex()
                            .items_center()
                            .gap(px(SPACE_2))
                            .cursor_pointer()
                            .child(
                                div()
                                    .size(px(ROW_ICON + SPACE_1))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(R_CONTROL))
                                    .border_1()
                                    .border_color(paint::FIELD_RING)
                                    .when(on, |box_| {
                                        box_.child(ferrite_ui::icons::icon(
                                            ferrite_ui::icons::CHECK,
                                            ROW_ICON,
                                            INK_STRONG,
                                        ))
                                    }),
                            )
                            .child(SharedString::from(step.name.clone()))
                            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                                view.toggle_skip(id.clone(), cx)
                            })),
                    );
                }
                body = body
                    .child(label("Skip for this task"))
                    .child(meta(
                        "When a route reaches a skipped agent, the task moves on as if it had \
                         finished.",
                    ))
                    .child(skips)
                    .child(label("Then"))
                    .child(segmented(
                        "redirect-then",
                        choice.then,
                        &[
                            (ThenReturn::Routes, "Follow the workflow's routes"),
                            (ThenReturn::Me, "Return it to me to approve"),
                        ],
                        |view: &mut Self, then, cx| view.set_then(then, cx),
                        cx,
                    ))
                    .child(input);
                let apply = ferrite_ui::form::primary_button(
                    "redirect-apply",
                    "Apply to this task",
                    false,
                    cx,
                )
                .debug_selector(|| "redirect-apply".into())
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.send(cx)
                }));
                (
                    format!("Direct task #{id}"),
                    "Applies to this task only. The saved workflow and its other tasks are \
                     unchanged.",
                    body,
                    apply,
                )
            }
        };
        let key = match self.choice {
            Choice::Rework(_) => "rework-sheet",
            Choice::Redirect(_) => "redirect-sheet",
        };
        let card = ferrite_ui::form::card(f32::from(window.viewport_size().height))
            .id(key)
            .debug_selector(move || key.to_string())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(ferrite_ui::form::head(title.into(), close))
            .child(ferrite_ui::form::body().child(meta(intro)).child(body))
            .child(ferrite_ui::form::footer(
                div(),
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(cancel)
                    .child(primary),
            ));
        Some(ferrite_ui::sheet::Drawn { card, above: None })
    }
}

/// A line of meta text under a section or a sheet's intro.
pub fn meta(text: impl Into<SharedString>) -> gpui::Div {
    use ferrite_ui::theme::*;
    div()
        .text_size(px(FS_META))
        .text_color(rgb(INK_SUBTLE))
        .child(text.into())
}

/// How a Destination reads as a rework recipient or what follows approval.
pub fn read(task: &Task, to: &Destination) -> String {
    match &task.workflow {
        Some(run) => run.definition.route_label("", to),
        None => to.word(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use ferrite_ui::testing::{click, mount, record, shows};
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;

    /// The sheet host's part in it: the card, where the host lays it.
    struct Host(Entity<DirectView>);

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

    fn project() -> ProjectKey {
        ProjectKey::fixture(1, ferrite_core::project::ProjectId(4))
    }

    /// A Workflow Task of three Steps as the Server would publish it, at
    /// its second, with `status`: rework goes to the first.
    fn task(id: &str, status: Status) -> Task {
        let step = |id: &str, name: &str| {
            serde_json::json!({
                "id": id, "name": name, "instructions": format!("{name} it"),
                "provider": "claude", "on_finished": "next", "on_unable": "human",
            })
        };
        serde_json::from_value(serde_json::json!({
            "id": id,
            "project": 4,
            "title": "Add a --fast flag",
            "instructions": "Add a --fast flag",
            "status": status,
            "agent": {"provider": "claude", "model": "claude-opus-4"},
            "branch": format!("factory/{id}"),
            "activity": [],
            "created_ms": 1,
            "workflow": {
                "id": "w-1",
                "version": 2,
                "definition": {
                    "name": "Ship it",
                    "steps": [step("s-1", "Build"), step("s-2", "Review"), step("s-3", "Check")],
                    "approval": {"after": "complete", "rework_to": "step:s-1"},
                },
            },
            "step": "s-2",
        }))
        .unwrap()
    }

    /// What the window keeps of the board, as a test keeps it.
    struct Kept {
        tasks: Vec<Task>,
        /// Each Project and Task the sheet asked the window about.
        asked: Rc<RefCell<Vec<DirectQuery>>>,
    }

    /// `sheet` on Task t-1, `status`, from what `kept` held; another Task
    /// is on the board before it.
    fn open<'a>(
        status: Status,
        sheet: DirectSheet,
        kept: &mut KeptDirection,
        cx: &'a mut TestAppContext,
    ) -> (Entity<DirectView>, Entity<Kept>, &'a mut VisualTestContext) {
        let board = cx.new(|_| Kept {
            tasks: vec![task("t-2", Status::Approval), task("t-1", status)],
            asked: Rc::default(),
        });
        let inputs = Source::of(&board, |board: &Kept, query: &DirectQuery, _| {
            board.asked.borrow_mut().push(query.clone());
            board
                .tasks
                .iter()
                .find(|task| task.id == query.1)
                .cloned()
        });
        let (host, cx) = mount(cx, |_, cx| {
            Host(cx.new(|cx| DirectView::new(inputs, project(), "t-1".into(), sheet, kept, cx)))
        });
        let view = host.read_with(cx, |host, _| host.0.clone());
        (view, board, cx)
    }

    /// The window's focus pump: the keyboard to the field the sheet names.
    fn pump(sheet: &Entity<DirectView>, cx: &mut VisualTestContext) {
        cx.update(|window, cx| {
            let focus = sheet.read(cx).focus(cx);
            window.focus(&focus, cx);
        });
        cx.run_until_parked();
    }

    fn text(cx: &mut VisualTestContext, selector: &str) -> String {
        cx.update(|window, _| window.debug_text(selector)).unwrap_or_default().join(" ")
    }

    /// What a sheet's events say: a picker's rows by label, the checked
    /// one marked, and whether it said where its trigger was drawn.
    fn said(event: &DirectEvent) -> String {
        match event {
            DirectEvent::SendRework => "send rework".into(),
            DirectEvent::ApplyRedirect => "apply redirect".into(),
            DirectEvent::Closed => "closed".into(),
            DirectEvent::OpenPicks(picks, trigger, _) => {
                let rows: Vec<String> = picks
                    .iter()
                    .map(|(item, _)| format!("{}{}", item.label, if item.checked { " (checked)" } else { "" }))
                    .collect();
                let under = if trigger.bounds().is_some() { " under it" } else { "" };
                format!("picks{under}: {}", rows.join(", "))
            }
        }
    }

    /// The Rework sheet draws its Task: the Workflow's recipient on its
    /// chooser, and the window is asked about the sheet's own Task.
    #[gpui::test]
    fn the_rework_sheet_draws_its_task_and_the_workflows_recipient(cx: &mut TestAppContext) {
        let (_view, board, cx) = open(Status::Approval, DirectSheet::Rework, &mut KeptDirection::default(), cx);
        for drawn in ["rework-sheet", "rework-feedback", "rework-to", "rework-send", "direct-cancel", "direct-close"] {
            assert!(shows(cx, drawn), "{drawn}");
        }
        assert!(!shows(cx, "redirect-sheet"));
        assert!(text(cx, "rework-to").contains("Build"), "{}", text(cx, "rework-to"));
        assert!(text(cx, "rework-sheet").contains("Rework task #t-1"));
        let asked = board.read_with(cx, |board, _| board.asked.borrow().last().cloned());
        assert_eq!(asked, Some((project(), "t-1".into())));
    }

    /// Send for rework says so each time; with no feedback the sheet says
    /// what is missing until some is typed.
    #[gpui::test]
    fn send_for_rework_says_what_is_missing(cx: &mut TestAppContext) {
        let (view, _board, cx) = open(Status::Approval, DirectSheet::Rework, &mut KeptDirection::default(), cx);
        let heard = record(cx, &view, said);
        assert!(!shows(cx, "rework-missing"));
        click(cx, "rework-send");
        assert!(shows(cx, "rework-missing"), "feedback is needed");
        pump(&view, cx);
        cx.simulate_input("Mention --fast");
        cx.run_until_parked();
        assert!(!shows(cx, "rework-missing"));
        click(cx, "rework-send");
        assert_eq!(heard.take(), ["send rework", "send rework"]);
    }

    /// The recipient chooser offers each Step and the Coordinator, the one
    /// it sends to checked; a pick moves it.
    #[gpui::test]
    fn the_recipient_chooser_offers_each_step_and_the_coordinator(cx: &mut TestAppContext) {
        let (view, _board, cx) = open(Status::Approval, DirectSheet::Rework, &mut KeptDirection::default(), cx);
        let heard = record(cx, &view, said);
        click(cx, "rework-to");
        view.update(cx, |view, cx| view.pick(TaskPick::ReworkTo(Destination::Step("s-2".into())), cx));
        cx.run_until_parked();
        assert!(text(cx, "rework-to").contains("Review"), "{}", text(cx, "rework-to"));
        click(cx, "rework-to");
        let heard = heard.take();
        assert_eq!(heard.len(), 2);
        assert!(heard[0].starts_with("picks under it: Build (checked), Review, Check, "), "{}", heard[0]);
        assert!(heard[1].starts_with("picks under it: Build, Review (checked), Check, "), "{}", heard[1]);
        assert_eq!(heard[0].matches(", ").count(), 3, "and the Coordinator: {}", heard[0]);
        let (_, choice) = view.read_with(cx, |view, cx| view.asked(cx));
        assert!(matches!(choice, Choice::Rework(ReworkChoice { to: Some(Destination::Step(step)) }) if step == "s-2"));
    }

    /// Direct this task needs a Step; the one it goes to is not offered to
    /// skip, a skip toggles, and Then chooses who it returns to.
    #[gpui::test]
    fn the_redirect_sheet_needs_a_step_and_skips_the_others(cx: &mut TestAppContext) {
        let (view, _board, cx) = open(Status::Approval, DirectSheet::Redirect, &mut KeptDirection::default(), cx);
        let heard = record(cx, &view, said);
        assert!(shows(cx, "redirect-sheet"));
        assert!(shows(cx, "redirect-to"));
        click(cx, "redirect-apply");
        assert!(shows(cx, "redirect-missing"), "a Step is needed");
        assert!(shows(cx, "redirect-to-flagged"));
        view.update(cx, |view, cx| view.pick(TaskPick::RedirectTo("s-3".into()), cx));
        cx.run_until_parked();
        assert!(!shows(cx, "redirect-missing"));
        assert!(!shows(cx, "redirect-skip-s-3"), "not the one it goes to");
        assert!(shows(cx, "redirect-skip-s-1") && shows(cx, "redirect-skip-s-2"));
        click(cx, "redirect-skip-s-2");
        assert!(shows(cx, "redirect-skip-s-2-on"));
        assert!(shows(cx, "redirect-skip-s-1"), "and only that one");
        click(cx, "redirect-skip-s-1");
        click(cx, "redirect-skip-s-1-on");
        assert!(shows(cx, "redirect-skip-s-1"), "a second press takes it back");
        assert!(shows(cx, "redirect-then-0-on"));
        click(cx, "redirect-then-1");
        assert!(shows(cx, "redirect-then-1-on"));
        assert_eq!(heard.take(), ["apply redirect"]);
        let (_, choice) = view.read_with(cx, |view, cx| view.asked(cx));
        let Choice::Redirect(choice) = choice else { panic!("a redirect") };
        assert_eq!(
            (choice.to.as_deref(), choice.skip, choice.then),
            (Some("s-3"), vec!["s-2".to_string()], ThenReturn::Me)
        );
    }

    /// Choosing the Step a skip names takes the skip back.
    #[gpui::test]
    fn choosing_a_skipped_step_unskips_it(cx: &mut TestAppContext) {
        let (view, _board, cx) = open(Status::Approval, DirectSheet::Redirect, &mut KeptDirection::default(), cx);
        click(cx, "redirect-skip-s-3");
        assert!(shows(cx, "redirect-skip-s-3-on"));
        view.update(cx, |view, cx| view.pick(TaskPick::RedirectTo("s-3".into()), cx));
        let (_, choice) = view.read_with(cx, |view, cx| view.asked(cx));
        let Choice::Redirect(choice) = choice else { panic!("a redirect") };
        assert!(choice.skip.is_empty(), "{:?}", choice.skip);
    }

    /// Its Step chooser names the Task's current Step, and a Working Task's
    /// sheet says its attempt is superseded, as the board has it now.
    #[gpui::test]
    fn the_redirect_sheet_reads_its_tasks_step_and_status(cx: &mut TestAppContext) {
        let (view, board, cx) = open(Status::Working, DirectSheet::Redirect, &mut KeptDirection::default(), cx);
        let heard = record(cx, &view, said);
        click(cx, "redirect-to");
        assert_eq!(heard.take(), ["picks under it: Build, Review \u{b7} current, Check"]);
        assert!(text(cx, "redirect-sheet").contains("superseded"));
        board.update(cx, |board, cx| {
            board.tasks[1] = task("t-1", Status::Approval);
            cx.notify();
        });
        cx.run_until_parked();
        assert!(!text(cx, "redirect-sheet").contains("superseded"), "as the board has it now");
    }

    /// Its close button and Cancel each say it closed.
    #[gpui::test]
    fn close_and_cancel_say_it_closed(cx: &mut TestAppContext) {
        let (view, _board, cx) = open(Status::Approval, DirectSheet::Rework, &mut KeptDirection::default(), cx);
        let heard = record(cx, &view, said);
        click(cx, "direct-cancel");
        click(cx, "direct-close");
        assert_eq!(heard.take(), ["closed", "closed"]);
    }

    /// The keyboard is its one field's, and a press on its choices leaves
    /// it there: what is typed after one lands in the note.
    #[gpui::test]
    fn a_press_on_its_choices_leaves_the_keyboard_in_its_field(cx: &mut TestAppContext) {
        let (view, _board, cx) = open(Status::Approval, DirectSheet::Redirect, &mut KeptDirection::default(), cx);
        pump(&view, cx);
        assert!(shows(cx, "factory-field-focused"));
        cx.simulate_input("Docs");
        click(cx, "redirect-skip-s-2");
        click(cx, "redirect-then-1");
        pump(&view, cx);
        cx.simulate_input(" only");
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |view, cx| view.asked(cx).0), "Docs only");
    }

    /// What a sheet holds goes back on its Task's keeping, apart from the
    /// other sheet's, and seeds the next; a send spends its own only.
    #[gpui::test]
    fn what_each_sheet_holds_is_kept_apart_until_spent(cx: &mut TestAppContext) {
        let mut kept = KeptDirection::default();
        let (rework, _board, cx) = open(Status::Approval, DirectSheet::Rework, &mut kept, cx);
        pump(&rework, cx);
        cx.simulate_input("Fix the docs");
        rework.update(cx, |view, cx| view.pick(TaskPick::ReworkTo(Destination::Coordinator), cx));
        rework.read_with(cx, |view, cx| view.keep(&mut kept, cx));
        let reopen = |sheet, kept: &mut KeptDirection, cx: &mut VisualTestContext| {
            let inputs = Source::of(&rework, |_: &DirectView, _: &DirectQuery, _| None);
            let view = cx.new(|cx| DirectView::new(inputs, project(), "t-1".into(), sheet, kept, cx));
            view.read_with(cx, |view, cx| view.asked(cx))
        };
        let (note, choice) = reopen(DirectSheet::Redirect, &mut kept, cx);
        assert_eq!(note, "", "the other sheet's own");
        assert!(matches!(choice, Choice::Redirect(_)));
        let (feedback, choice) = reopen(DirectSheet::Rework, &mut kept, cx);
        assert_eq!(feedback, "Fix the docs");
        assert!(matches!(choice, Choice::Rework(ReworkChoice { to: Some(Destination::Coordinator) })));
        rework.read_with(cx, |view, cx| view.keep(&mut kept, cx));
        kept.spend(DirectSheet::Rework);
        let (feedback, choice) = reopen(DirectSheet::Rework, &mut kept, cx);
        assert_eq!(feedback, "", "spent once sent");
        assert!(matches!(choice, Choice::Rework(ReworkChoice { to: None })));
    }

    /// Once its Task runs no Workflow, or leaves the board, the sheet draws
    /// no card.
    #[gpui::test]
    fn with_its_task_gone_it_draws_no_card(cx: &mut TestAppContext) {
        let (_view, board, cx) = open(Status::Approval, DirectSheet::Rework, &mut KeptDirection::default(), cx);
        board.update(cx, |board, cx| {
            board.tasks[1].workflow = None;
            cx.notify();
        });
        cx.run_until_parked();
        assert!(!shows(cx, "rework-sheet"), "no Workflow to direct");
        board.update(cx, |board, cx| {
            board.tasks.pop();
            cx.notify();
        });
        cx.run_until_parked();
        assert!(!shows(cx, "rework-sheet"));
    }
}
