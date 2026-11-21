//! The Factory's Rework and Direct this task sheets as one view (ADR
//! 0041), made when one opens and dropped once it has gone (ADR 0042).
//! The two share a Task, one text field that holds the keyboard, a card
//! and its close; which of them it is, and what it chooses, is its
//! `Choice`. It draws its Task as the Cockpit has it this frame
//! (`direct_inputs`), and what the operator asks of it it says as a
//! `DirectEvent`. Sending is the Cockpit's (`send_rework`,
//! `apply_redirect`).
//!
//! What was typed and chosen outlives the view: a close leaves it on the
//! Factory, per Task, as a `KeptDirection` that seeds the next sheet.

use super::direction::{meta, picker_button, read, segmented, DirectSheet};
use super::workflows::{Pick, Trigger};
use super::*;
use ferrite_core::draft_content::DraftContent;
use ferrite_core::factory::{Destination, Status, StepId, Task, TaskId, ThenReturn};
use ferrite_core::workspace::registry::ProjectId;
use ferrite_pane::composer::{Composer, Role};
use gpui::{App, WeakEntity};
use std::rc::Rc;

/// What a Rework sheet chooses.
#[derive(Clone, Default)]
pub(crate) struct ReworkChoice {
    /// The recipient; none, the Workflow's.
    pub(super) to: Option<Destination>,
}

/// What a Direct this task sheet chooses.
#[derive(Clone, Default)]
pub(crate) struct RedirectChoice {
    pub(super) to: Option<StepId>,
    pub(super) skip: Vec<StepId>,
    pub(super) then: ThenReturn,
}

/// Which of the two sheets it is, with what it chooses.
#[derive(Clone)]
pub(crate) enum Choice {
    Rework(ReworkChoice),
    Redirect(RedirectChoice),
}

/// One Task's two sheets as they were last closed, kept on its Factory.
/// A refusal, a send tried and the caret are not kept.
#[derive(Default)]
pub(crate) struct KeptDirection {
    feedback: DraftContent,
    rework: ReworkChoice,
    note: DraftContent,
    redirect: RedirectChoice,
}

impl KeptDirection {
    /// A send taken: what its sheet held is spent.
    pub(super) fn spend(&mut self, sheet: DirectSheet) {
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

pub(crate) struct DirectView {
    cockpit: WeakEntity<CockpitView>,
    /// The Project whose Factory opened it: the sheet is that Factory's.
    pub(super) project: ProjectId,
    pub(super) task: TaskId,
    pub(super) choice: Choice,
    /// Its one text field, which holds the keyboard while it is up: the
    /// Rework's feedback, or the redirect's note.
    pub(super) field: Entity<Composer>,
    /// Its primary action was pressed: say what is missing.
    tried: bool,
}

/// What the sheet asks of the window.
#[derive(Clone)]
pub(crate) enum DirectEvent {
    /// Send for rework pressed.
    SendRework,
    /// Apply to this task pressed.
    ApplyRedirect,
    /// A chooser pressed: its rows, for the window's picker under it.
    OpenPicks(Rc<Vec<(ferrite_ui::menu::Item, Pick)>>, Trigger, Point<Pixels>),
    /// Its close button or Cancel pressed.
    Closed,
}

impl EventEmitter<DirectEvent> for DirectView {}

impl DirectView {
    fn new(
        cockpit: WeakEntity<CockpitView>,
        project: ProjectId,
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
            cockpit,
            project,
            task,
            choice,
            field,
            tried: false,
        }
    }

    /// What it holds, back on its Task's `kept`.
    fn keep(&self, kept: &mut KeptDirection, cx: &App) {
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

    pub(super) fn focus(&self, cx: &App) -> FocusHandle {
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
    pub(super) fn pick(&mut self, pick: super::direction::TaskPick, cx: &mut Context<Self>) {
        use super::direction::TaskPick;
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
        picks: Vec<(ferrite_ui::menu::Item, Pick)>,
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
    pub(super) fn card(&self, window: &Window, cx: &mut Context<Self>) -> Option<ferrite_ui::sheet::Drawn> {
        use ferrite_ui::theme::*;
        let (project, id) = (self.project, self.task.clone());
        let task = self
            .cockpit
            .read_with(cx, |cockpit, _| cockpit.direct_inputs(project, &id))
            .ok()??;
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
            .child(crate::project_editor::text_field(
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
                let picks: Vec<(ferrite_ui::menu::Item, Pick)> = definition
                    .steps
                    .iter()
                    .map(|step| Destination::Step(step.id.clone()))
                    .chain([Destination::Coordinator])
                    .map(|each| {
                        (
                            ferrite_ui::menu::Item::new(read(&task, &each)).checked(each == to),
                            Pick::Task(super::direction::TaskPick::ReworkTo(each)),
                        )
                    })
                    .collect();
                let mut body = div().flex().flex_col().gap(px(SPACE_3)).child(input);
                if missing {
                    body = body.child(
                        crate::project_editor::error_line(
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
                let send = crate::project_editor::primary_button(
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
                            Pick::Task(super::direction::TaskPick::RedirectTo(step.id.clone())),
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
                        crate::project_editor::error_line(
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
                let apply = crate::project_editor::primary_button(
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
        let card = crate::project_editor::card(f32::from(window.viewport_size().height))
            .id(key)
            .debug_selector(move || key.to_string())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(crate::project_editor::head(title.into(), close))
            .child(crate::project_editor::body().child(meta(intro)).child(body))
            .child(crate::project_editor::footer(
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

impl CockpitView {
    /// The Task the sheet of `project`'s Factory draws, as the board has it
    /// now: none once that Factory no longer shows, or the Task left the
    /// board or runs no Workflow. It never reads the sheet: the sheet calls
    /// it while its own update is running.
    pub(super) fn direct_inputs(&self, project: ProjectId, task: &str) -> Option<Task> {
        if self.factory_project()? != project {
            return None;
        }
        self.cockpit
            .factory()
            .task(task)
            .filter(|task| task.workflow.is_some())
            .cloned()
    }

    fn on_direct_event(&mut self, _: Entity<DirectView>, event: &DirectEvent, cx: &mut Context<Self>) {
        match event.clone() {
            DirectEvent::SendRework => self.send_rework(cx),
            DirectEvent::ApplyRedirect => self.apply_redirect(cx),
            DirectEvent::OpenPicks(picks, trigger, at) => {
                self.open_picks((*picks).clone(), &trigger, at, cx)
            }
            DirectEvent::Closed => {
                self.hide_sheet(cx);
            }
        }
    }

    /// The Rework or Direct this task sheet, while it is up.
    pub(crate) fn direct_sheet(&self, cx: &App) -> Option<Entity<DirectView>> {
        self.sheets.read(cx).direct().cloned()
    }

    /// A sheet on the open Task, in place of any other sheet, with what
    /// was typed and chosen in it before.
    pub(crate) fn open_direct_sheet(&mut self, sheet: DirectSheet, cx: &mut Context<Self>) {
        let Some(task) = self.open_task_now(cx).map(|task| task.id.clone()) else {
            return;
        };
        let (Some(project), Some(view)) = (self.factory_project(), self.factory()) else {
            return;
        };
        let direction = view.read(cx).direction.clone();
        let cockpit = cx.weak_entity();
        let sheet = direction.update(cx, |direction, cx| {
            // The sheet's field takes the keyboard; the answer box does not
            // have it back once the sheet closes.
            direction.answering = false;
            let kept = direction.kept.entry(task.clone()).or_default();
            cx.new(|cx| DirectView::new(cockpit, project, task, sheet, kept, cx))
        });
        cx.subscribe(&sheet, Self::on_direct_event).detach();
        self.show_sheet(sheets::Sheet::Direct(sheet), cx);
    }

    /// The sheet gone, closed or replaced: what it held stays on its
    /// Factory, if that still shows.
    pub(super) fn direct_gone(&mut self, sheet: &Entity<DirectView>, cx: &mut App) {
        let project = sheet.read(cx).project;
        let Some(direction) = self
            .factory()
            .filter(|_| self.factory_project() == Some(project))
            .map(|view| view.read(cx).direction.clone())
        else {
            return;
        };
        direction.update(cx, |direction, cx| {
            let sheet = sheet.read(cx);
            sheet.keep(direction.kept.entry(sheet.task.clone()).or_default(), cx);
        });
    }
}
