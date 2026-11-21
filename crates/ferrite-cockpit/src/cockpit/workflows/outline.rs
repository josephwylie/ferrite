//! The Workflows surface: the surface tabs, the list of Workflows, and
//! the outline of Steps with their routes, which the editor's view draws
//! and whose presses it says as `WorkflowsEvent`s.

use super::*;

impl CockpitView {
    /// The Factory head's surface tabs: Board and Workflows.
    pub(in crate::cockpit) fn factory_surface_tabs(&self, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let Some(view) = self.factory() else {
            return div();
        };
        let board = self.cockpit.factory();
        let counts = [
            (
                Surface::Coordinator,
                "Coordinator",
                board
                    .proposals_of(view.project)
                    .filter(|proposal| {
                        proposal.status == ferrite_core::factory::ProposalStatus::Pending
                    })
                    .count(),
            ),
            (
                Surface::Board,
                "Board",
                board.on_board(view.project).count(),
            ),
            (
                Surface::Workflows,
                "Workflows",
                board.workflows_of(view.project).count(),
            ),
        ];
        let mut tabs = div().flex().items_center().gap(px(SPACE_1));
        for (surface, word, count) in counts {
            let key = format!("factory-tab-{}", word.to_lowercase());
            // The Archive is the board's own view, under none of the tabs.
            let selected =
                view.surface == surface && !(surface == Surface::Board && view.archive_shown);
            tabs = tabs.child(
                div()
                    .id(SharedString::from(key.clone()))
                    .debug_selector(move || key.clone())
                    .flex()
                    .items_center()
                    .gap(px(SPACE_1))
                    .px(px(SPACE_2))
                    .h(px(CONTROL_H))
                    .rounded(px(R_CONTROL))
                    .cursor_pointer()
                    .text_size(px(FS_LABEL))
                    .font_weight(if selected { W_LABEL } else { W_BODY })
                    .text_color(rgb(if selected { INK_STRONG } else { INK_SUBTLE }))
                    .when(selected, |tab| tab.bg(paint::RAISED))
                    .when(!selected, |tab| tab.hover(|tab| tab.bg(paint::HOVER)))
                    .child(word)
                    .child(
                        div()
                            .text_color(rgb(INK_SUBTLE))
                            .child(SharedString::from(count.to_string())),
                    )
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        if surface == Surface::Board
                            && view
                                .factory()
                                .is_some_and(|view| view.archive_shown)
                        {
                            view.close_archive(cx);
                        }
                        view.factory_surface(surface, cx)
                    })),
            );
        }
        tabs
    }

    /// The Workflows surface: its head, the draft's problems, the outline
    /// and the inspector.
    pub(in crate::cockpit) fn workflows_surface(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        use ferrite_ui::theme::*;
        let editor = self.factory().map(|view| view.editor.clone());
        let (Some(editor), Some(workflow)) = (editor, self.shown_workflow(cx)) else {
            return div()
                .debug_selector(|| "workflows-empty".into())
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(SPACE_2))
                .child(
                    div()
                        .font_weight(W_LABEL)
                        .text_color(rgb(INK_STRONG))
                        .child("No workflows yet"),
                )
                .child(
                    div()
                        .text_size(px(FS_META))
                        .text_color(rgb(INK_SUBTLE))
                        .child(
                            "A workflow is your process: agents in order. Start Blank or from \
                             the Issue to reviewed PR template.",
                        ),
                )
                .child(self.new_workflow_button(cx));
        };
        let Some(inputs) = self.workflows_inputs(editor.read(cx)) else {
            return div();
        };
        let problems = &inputs.problems;
        let unsaved = workflow.unsaved();
        let version = match workflow.latest() {
            Some(latest) if unsaved => format!("version {} \u{b7} edited", latest.version),
            Some(latest) => format!("version {}", latest.version),
            None => "not saved".into(),
        };
        let runnable = workflow.latest().is_some() && problems.is_empty();
        let save_disabled = !unsaved || !problems.is_empty();
        let picker = Trigger::default();
        let head = div()
            .debug_selector(|| "workflows-head".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(CONTROL_H + SPACE_4))
            .px(px(SPACE_4))
            .border_b_1()
            .border_color(paint::LINE)
            .child(
                div()
                    .id("workflow-picker")
                    .debug_selector(|| "workflow-picker".into())
                    .flex()
                    .items_center()
                    .gap(px(SPACE_1))
                    .px(px(SPACE_2))
                    .h(px(CONTROL_H))
                    .rounded(px(R_CONTROL))
                    .cursor_pointer()
                    .hover(|picker| picker.bg(paint::HOVER))
                    .font_weight(W_LABEL)
                    .child(SharedString::from(workflow_title(workflow)))
                    .child(ferrite_ui::icons::icon(
                        ferrite_ui::icons::CHEVRON_DOWN,
                        ROW_ICON,
                        INK_SUBTLE,
                    ))
                    .relative()
                    .child(picker.probe())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let picks = view.workflow_picks(cx);
                            view.open_picks(picks, &picker, event.position, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "workflow-version".into())
                    .px(px(SPACE_1_5))
                    .rounded(px(R_CONTROL))
                    .border_1()
                    .border_color(paint::LINE)
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(version)),
            )
            .child(div().flex_1())
            .child(self.new_workflow_button(cx))
            .child(
                ferrite_ui::components::secondary_button(
                    "workflow-run",
                    Some(ferrite_ui::icons::ARROW_RIGHT),
                    "Run task",
                    cx,
                )
                .debug_selector(|| "workflow-run".into())
                .disabled(!runnable)
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    let workflow = view.shown_id(cx);
                    view.open_new_task(cx);
                    view.set_new_task_workflow(workflow, cx);
                })),
            )
            .child(
                crate::project_editor::primary_button(
                    "workflow-save",
                    if unsaved { "Save" } else { "Saved" },
                    save_disabled,
                    cx,
                )
                .debug_selector(|| "workflow-save".into())
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.save_workflow(cx);
                })),
            );

        let listed = (!problems.is_empty()).then(|| {
            let mut list = div()
                .debug_selector(|| "workflow-problems".into())
                .flex()
                .flex_col()
                .flex_shrink_0()
                .gap(px(SPACE_0_5))
                .px(px(SPACE_4))
                .py(px(SPACE_2))
                .border_b_1()
                .border_color(paint::LINE)
                .text_size(px(FS_META))
                .child(div().font_weight(W_LABEL).text_color(rgb(ATTENTION)).child(
                    SharedString::from(match problems.len() {
                        1 => "1 problem: it cannot be saved or run until fixed".to_string(),
                        n => format!("{n} problems: it cannot be saved or run until fixed"),
                    }),
                ));
            for (at, problem) in problems.iter().enumerate() {
                let step = problem.step.clone();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("workflow-problem-{at}")))
                        .debug_selector(move || format!("workflow-problem-{at}"))
                        .text_color(rgb(INK_STRONG))
                        .when(step.is_some(), |row| row.cursor_pointer())
                        .child(SharedString::from(problem.message.clone()))
                        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                            if let Some(step) = step.clone() {
                                view.select_step(step, cx);
                            }
                        })),
                );
            }
            list
        });

        div()
            .debug_selector(|| "workflows".into())
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(head)
            .children(listed)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(editor.update(cx, |editor, cx| editor.outline(&inputs, window, cx)))
                    .child(self.inspector(workflow, window, cx)),
            )
    }

    pub(super) fn new_workflow_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use ferrite_ui::theme::*;
        let trigger = Trigger::default();
        div()
            .id("workflow-new")
            .debug_selector(|| "workflow-new".into())
            .flex()
            .items_center()
            .gap(px(CONTROL_CHIP_GAP))
            .h(px(CONTROL_H))
            .pl(px(CONTROL_CHIP_PAD_X))
            .pr(px(CONTROL_PAD_X))
            .rounded(px(R_CONTROL))
            .bg(paint::RAISED)
            .hover(|button| button.bg(paint::SELECTED))
            .cursor_pointer()
            .text_size(px(FS_LABEL))
            .font_weight(W_LABEL)
            .text_color(rgb(INK_STRONG))
            .child(ferrite_ui::icons::icon(
                ferrite_ui::icons::PLUS,
                CONTROL_CHIP_ICON,
                INK_STRONG,
            ))
            .child("New workflow")
            .relative()
            .child(trigger.probe())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_picks(Self::template_picks(), &trigger, event.position, cx);
                }),
            )
    }
}

impl WorkflowsView {
    /// The outline: the Workflow's name, then each Step's row with its
    /// routes, in order. No box round a row; the chosen row is filled.
    pub(super) fn outline(
        &self,
        inputs: &WorkflowsInputs,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let draft = &inputs.draft;
        let problems = &inputs.problems;
        let approval_shown = self.inspected == Inspected::Approval;
        // While the start or the approval settings show, no Step's row
        // is the chosen one.
        let shown = inputs
            .step
            .clone()
            .filter(|_| self.inspected == Inspected::Step);
        let name_field = Field::WorkflowName(inputs.workflow.clone());
        let mut outline = div()
            .id("outline")
            .debug_selector(|| "outline".into())
            .track_focus(&self.outline_focus)
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(OUTLINE_W))
            .min_h_0()
            .gap(px(SPACE_1))
            .p(px(SPACE_4))
            .border_r_1()
            .border_color(paint::LINE)
            .overflow_y_scroll()
            .track_scroll(&self.outline_scroll)
            .on_key_down(cx.listener(|editor, event: &gpui::KeyDownEvent, _, cx| {
                editor.outline_key(&event.keystroke, cx)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|editor, _: &MouseDownEvent, _, cx| {
                    editor.focused = None;
                    cx.emit(WorkflowsEvent::Focused);
                }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_1))
                    .children(self.input(&name_field, "Workflow name", window, cx))
                    .child(self.start_row(inputs, cx))
                    .child(
                        div()
                            .flex()
                            .gap(px(SPACE_2))
                            .pt(px(SPACE_2))
                            .text_size(px(FS_META))
                            .text_color(rgb(INK_SUBTLE))
                            .child(div().font_weight(W_LABEL).child("Agents, in order"))
                            .child("\u{201c}Next agent\u{201d} follows this order"),
                    ),
            );
        for (at, step) in draft.steps.iter().enumerate() {
            let flagged = problems
                .iter()
                .any(|problem| problem.step.as_deref() == Some(step.id.as_str()));
            outline = outline.child(Self::outline_row(
                inputs,
                step,
                at,
                shown.as_deref() == Some(step.id.as_str()),
                flagged,
                cx,
            ));
        }
        let approval_flagged = problems.iter().any(|problem| problem.approval);
        let approval = &draft.approval;
        let decides = match approval.by {
            Approver::Human => "You approve".to_string(),
            Approver::Agent => match approval.step.as_deref().and_then(|id| draft.step(id))
            {
                Some(step) => format!("{} decides", step.name),
                None => "An agent decides".into(),
            },
        };
        outline
            .child(
                div()
                    .id("outline-approval")
                    .debug_selector(move || {
                        match (approval_shown, approval_flagged) {
                            (true, true) => "outline-approval-selected-flagged",
                            (true, false) => "outline-approval-selected",
                            (false, true) => "outline-approval-flagged",
                            (false, false) => "outline-approval",
                        }
                        .into()
                    })
                    .mt(px(SPACE_2))
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .px(px(SPACE_2))
                    .h(px(ROW + SPACE_2))
                    .rounded(px(R_CONTROL))
                    .cursor_pointer()
                    .when(approval_shown, |row| row.bg(paint::SELECTED))
                    .when(!approval_shown, |row| row.hover(|row| row.bg(paint::HOVER)))
                    .child(div().font_weight(W_LABEL).child("Approval"))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(FS_META))
                            .text_color(rgb(if approval_flagged {
                                BLOCKED
                            } else {
                                INK_SUBTLE
                            }))
                            .child(SharedString::from(decides)),
                    )
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        cx.emit(WorkflowsEvent::Approval)
                    })),
            )
            .child(
                div().pt(px(SPACE_2)).child(
                    ferrite_ui::components::ghost_button("outline-add", "+ Add agent", cx)
                        .debug_selector(|| "outline-add".into())
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            cx.emit(WorkflowsEvent::Add);
                        })),
                ),
            )
            .into_any_element()
    }

    fn outline_row(
        inputs: &WorkflowsInputs,
        step: &Step,
        at: usize,
        selected: bool,
        flagged: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let id = step.id.clone();
        let key = format!("outline-row-{}", step.id);
        let name = if step.name.trim().is_empty() {
            "Untitled agent".to_string()
        } else {
            step.name.clone()
        };
        let provider = step.agent.provider;
        let mut meta = vec![match &step.agent.model {
            Some(model) => model_name(inputs.models(provider), model),
            None => provider.to_string(),
        }];
        if let Some(effort) = &step.agent.effort {
            meta.push(effort.clone());
        }
        match step.skills.len() {
            0 => {}
            1 => meta.push("1 skill".into()),
            n => meta.push(format!("{n} skills")),
        }
        if step.fresh {
            meta.push("starts fresh".into());
        }
        let route = |kind: RouteKind, to: &Destination, cx: &mut Context<Self>| {
            let trigger = Trigger::default();
            let label = inputs.draft.route_label(&step.id, to);
            let broken = inputs.draft.broken(&step.id, to).is_some();
            let which = kind.word();
            let selector = format!(
                "outline-route-{}-{which}{}",
                step.id,
                if broken { "-flagged" } else { "" }
            );
            let step_id = step.id.clone();
            div()
                .flex()
                .items_center()
                .gap(px(SPACE_2))
                .text_size(px(FS_META))
                .child(
                    div()
                        .w(px(132.))
                        .flex_shrink_0()
                        .text_color(rgb(INK_SUBTLE))
                        .child(kind.label()),
                )
                .child(
                    div()
                        .id(SharedString::from(selector.clone()))
                        .debug_selector(move || selector.clone())
                        .flex()
                        .items_center()
                        .gap(px(SPACE_1))
                        .px(px(SPACE_1))
                        .rounded(px(R_CONTROL))
                        .cursor_pointer()
                        .hover(|route| route.bg(paint::HOVER))
                        .text_color(rgb(if broken { BLOCKED } else { INK_STRONG }))
                        .when(broken, |route| {
                            route.child(ferrite_ui::icons::icon(
                                ferrite_ui::icons::WARNING,
                                ROW_ICON,
                                BLOCKED,
                            ))
                        })
                        .child(SharedString::from(label))
                        .child(ferrite_ui::icons::icon(
                            ferrite_ui::icons::CHEVRON_DOWN,
                            ROW_ICON,
                            INK_SUBTLE,
                        ))
                        .relative()
                        .child(trigger.probe())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                cx.emit(WorkflowsEvent::Route {
                                    step: step_id.clone(),
                                    kind,
                                    trigger: trigger.clone(),
                                    at: event.position,
                                });
                            }),
                        ),
                )
        };
        // One route per merge result when it routes by them (#223).
        let routes: Vec<_> = step
            .routes()
            .into_iter()
            .map(|(kind, to)| route(kind, to, cx))
            .collect();
        let more_id = step.id.clone();
        let more_key = format!("{key}-more");
        let more = Trigger::default();
        let drag = StepDrag {
            step: step.id.clone(),
            name: SharedString::from(name.clone()),
        };
        let select = id.clone();
        div()
            .id(SharedString::from(key.clone()))
            .debug_selector({
                let key = if selected {
                    format!("{key}-selected")
                } else {
                    key.clone()
                };
                move || key.clone()
            })
            .flex()
            .gap(px(SPACE_2))
            .px(px(SPACE_2))
            .py(px(SPACE_2))
            .rounded(px(R_CONTROL))
            .cursor_pointer()
            .map(|row| {
                if selected {
                    row.bg(paint::SELECTED)
                } else {
                    row.hover(|row| row.bg(paint::HOVER))
                }
            })
            .on_drag(drag, |drag: &StepDrag, _, _, cx| {
                let name = drag.name.clone();
                cx.new(|_| StepDragPreview(name))
            })
            .drag_over::<StepDrag>(|style, _, _, _| style.bg(paint::SELECTED))
            .on_drop(cx.listener(move |_, drag: &StepDrag, _, cx| {
                cx.emit(WorkflowsEvent::Drop {
                    drag: drag.clone(),
                    onto: at,
                })
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _: &MouseDownEvent, _, cx| {
                    cx.emit(WorkflowsEvent::Select(select.clone()))
                }),
            )
            .child(
                div()
                    .w(px(SPACE_4))
                    .flex_shrink_0()
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from((at + 1).to_string())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_1))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(SPACE_2))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(W_LABEL)
                                    .text_color(rgb(INK_STRONG))
                                    .child(SharedString::from(name)),
                            )
                            .when(flagged, |line| {
                                line.child(ferrite_ui::icons::icon(
                                    ferrite_ui::icons::WARNING,
                                    ROW_ICON,
                                    ATTENTION,
                                ))
                            })
                            .child(
                                div()
                                    .id(SharedString::from(more_key.clone()))
                                    .debug_selector(move || more_key.clone())
                                    .px(px(SPACE_1))
                                    .rounded(px(R_CONTROL))
                                    .hover(|more| more.bg(paint::HOVER))
                                    .child(ferrite_ui::icons::icon(
                                        ferrite_ui::icons::ELLIPSIS,
                                        ROW_ICON,
                                        INK_SUBTLE,
                                    ))
                                    .relative()
                                    .child(more.probe())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |_, event: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            cx.emit(WorkflowsEvent::RowMenu {
                                                step: more_id.clone(),
                                                trigger: more.clone(),
                                                at: event.position,
                                            });
                                        }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(SPACE_1))
                            .text_size(px(FS_META))
                            .text_color(rgb(INK_SUBTLE))
                            .child(provider_mark(step.agent.provider))
                            .child(SharedString::from(meta.join(" \u{b7} "))),
                    )
                    .children(routes),
            )
            .into_any_element()
    }

    /// The outline's row for the start, above the agents.
    fn start_row(&self, inputs: &WorkflowsInputs, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let selected = self.inspected == Inspected::Start;
        let key = if selected {
            "outline-start-selected"
        } else {
            "outline-start"
        };
        let start = &inputs.draft.start;
        let mut said = start.kind.label().to_string();
        if start.kind.automatic() {
            said = format!(
                "{said} \u{b7} automatic starts {}",
                inputs.switch.word().to_lowercase()
            );
        }
        div()
            .id("outline-start")
            .debug_selector(move || key.into())
            .flex()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_2))
            .py(px(SPACE_1_5))
            .rounded(px(R_CONTROL))
            .cursor_pointer()
            .map(|row| {
                if selected {
                    row.bg(paint::SELECTED)
                } else {
                    row.hover(|row| row.bg(paint::HOVER))
                }
            })
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(W_LABEL)
                    .text_color(rgb(INK_STRONG))
                    .child("Tasks start"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(said)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(WorkflowsEvent::Start);
                }),
            )
            .into_any_element()
    }
}

impl CockpitView {
    /// One text field, drawn by the editor.
    pub(in crate::cockpit) fn text_input(
        &self,
        field: &Field,
        label: &'static str,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.factory()?
            .editor
            .update(cx, |editor, cx| editor.input(field, label, window, cx))
    }
}

/// A Workflows field drawn: its label over its Composer, ringed while it
/// has the keyboard; a press on it calls `pressed` first.
pub(crate) fn field_input(
    field: &Field,
    label: &'static str,
    composer: Entity<Composer>,
    focused: bool,
    pressed: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let key = field.key();
    div()
        .id(SharedString::from(key.clone()))
        .debug_selector(move || key.clone())
        .capture_any_mouse_down(pressed)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(if field.prose() {
            crate::project_editor::text_area(label.into(), composer, focused, "factory-field-focused")
        } else {
            crate::project_editor::text_field(label.into(), composer, focused, "factory-field-focused")
        })
        .into_any_element()
}
