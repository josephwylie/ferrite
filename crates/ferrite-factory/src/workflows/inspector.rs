//! The inspector beside the outline, which the editor's view draws: the
//! chosen Step's settings with the skills search, the approval settings,
//! or the start's, which the starts' own view draws. What a press asks it
//! says as a `WorkflowsEvent`.

use ferrite_core::factory_board::{Approver, Destination, Step, StepChange};
use gpui::prelude::*;
use gpui::{
    div, px, rgb, AnyElement, ClickEvent, Context, MouseButton, MouseDownEvent, SharedString,
    Window,
};

use super::pickers::{effort_picks, model_picks, provider_picks};
use super::{skill_matches, Inspected, WorkflowsEvent, WorkflowsInputs, WorkflowsView, SKILL_ROWS};
use crate::field::Field;
use crate::picker::{Pick, Trigger};

impl WorkflowsView {
    /// The inspector: the chosen Step's name, instructions, skills,
    /// Provider, model, Effort and "Start fresh every time".
    pub(super) fn inspector(
        &self,
        inputs: &WorkflowsInputs,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        match self.inspected {
            Inspected::Start => return inputs.starts.clone().into_any_element(),
            Inspected::Approval => return self.approval_inspector(inputs, window, cx),
            Inspected::Step => {}
        }
        let Some(step) = inputs.step.as_deref().and_then(|id| inputs.draft.step(id)) else {
            return div()
                .debug_selector(|| "inspector-none".into())
                .flex_1()
                .p(px(SPACE_4))
                .text_color(rgb(INK_SUBTLE))
                .child("Add an agent to begin.")
                .into_any_element();
        };
        let catalog = &inputs.catalog;
        let label = |text: &'static str| {
            div()
                .text_size(px(FS_META))
                .font_weight(W_LABEL)
                .text_color(rgb(INK_SUBTLE))
                .child(text)
        };
        let chooser = |selector: &'static str,
                       text: String,
                       flagged: bool,
                       picks: Vec<(ferrite_ui::menu::Item, Pick)>,
                       cx: &mut Context<Self>| {
            let picks = std::rc::Rc::new(picks);
            let trigger = Trigger::default();
            div()
                .id(selector)
                .debug_selector(move || {
                    if flagged {
                        format!("{selector}-flagged")
                    } else {
                        selector.to_string()
                    }
                })
                .flex()
                .items_center()
                .gap(px(SPACE_1))
                .h(px(CONTROL_H))
                .px(px(SPACE_2))
                .rounded(px(R_CONTROL))
                .border_1()
                .border_color(if flagged {
                    gpui::Hsla::from(rgb(BLOCKED))
                } else {
                    paint::FIELD_RING.into()
                })
                .cursor_pointer()
                .text_color(rgb(if flagged { BLOCKED } else { INK_STRONG }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(SharedString::from(text)),
                )
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
                        cx.emit(WorkflowsEvent::Picks {
                            picks: (*picks).clone(),
                            trigger: trigger.clone(),
                            at: event.position,
                        });
                    }),
                )
        };
        let provider = step.agent.provider;
        let model_known = catalog.models(provider).is_some();
        let model_flagged = model_known
            && step
                .agent
                .model
                .as_deref()
                .is_some_and(|model| catalog.model(provider, model).is_none());
        let effort_flagged = step.agent.model.as_deref().is_some_and(|model| {
            catalog.model(provider, model).is_some_and(|info| {
                step.agent
                    .effort
                    .as_ref()
                    .is_some_and(|effort| !info.efforts.contains(effort))
            })
        });
        let model_text = match &step.agent.model {
            Some(model) if model_flagged => format!("{model} \u{b7} unavailable"),
            Some(model) => catalog
                .model(provider, model)
                .map(|info| info.display.clone())
                .unwrap_or_else(|| model.clone()),
            None => "Default".into(),
        };
        let effort_text = match &step.agent.effort {
            Some(effort) if effort_flagged => format!("{effort} \u{b7} unavailable"),
            Some(effort) => effort.clone(),
            None => "Default".into(),
        };

        // Skills: the chosen ones as chips, an unavailable one flagged,
        // and the search that lists what is installed.
        let installed = inputs.installed.as_deref();
        let mut chips = div().flex().flex_wrap().gap(px(SPACE_1));
        for skill in &step.skills {
            let missing = installed.is_some_and(|list| !list.iter().any(|s| &s.name == skill));
            let key = format!(
                "skill-chip-{skill}{}",
                if missing { "-flagged" } else { "" }
            );
            let remove = skill.clone();
            chips = chips.child(
                div()
                    .id(SharedString::from(key.clone()))
                    .debug_selector(move || key.clone())
                    .flex()
                    .items_center()
                    .gap(px(SPACE_1))
                    .px(px(SPACE_1_5))
                    .rounded(px(R_CONTROL))
                    .bg(paint::RAISED)
                    .text_size(px(FS_META))
                    .text_color(rgb(if missing { BLOCKED } else { INK_STRONG }))
                    .child(SharedString::from(if missing {
                        format!("{skill} \u{b7} not installed")
                    } else {
                        skill.clone()
                    }))
                    .child(
                        div()
                            .cursor_pointer()
                            .child(ferrite_ui::icons::icon(
                                ferrite_ui::icons::CLOSE,
                                ROW_ICON,
                                INK_SUBTLE,
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |_, _: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    cx.emit(WorkflowsEvent::RemoveSkill(remove.clone()));
                                }),
                            ),
                    ),
            );
        }
        chips = chips.child(
            div()
                .id("skill-add")
                .debug_selector(|| "skill-add".into())
                .px(px(SPACE_1_5))
                .rounded(px(R_CONTROL))
                .cursor_pointer()
                .hover(|add| add.bg(paint::HOVER))
                .text_size(px(FS_META))
                .text_color(rgb(INK_SUBTLE))
                .child(if self.skills_open {
                    "Done"
                } else {
                    "+ Add skill"
                })
                .on_click(cx.listener(|editor, _: &ClickEvent, _, cx| {
                    editor.skills_open = !editor.skills_open;
                    editor.focused = editor.skills_open.then_some(Field::SkillSearch);
                    cx.emit(WorkflowsEvent::Focused);
                })),
        );
        let search = self.skills_open.then(|| {
            let filter = self
                .field(&Field::SkillSearch)
                .map(|composer| composer.read(cx).text().to_string())
                .unwrap_or_default();
            let mut list = div()
                .debug_selector(|| "skill-options".into())
                .flex()
                .flex_col()
                .gap(px(SPACE_0_5));
            for skill in installed
                .unwrap_or_default()
                .iter()
                .filter(|skill| !step.skills.contains(&skill.name))
                .filter(|skill| skill_matches(skill, &filter))
                .take(SKILL_ROWS)
            {
                let key = format!("skill-option-{}", skill.name);
                let name = skill.name.clone();
                list = list.child(
                    div()
                        .id(SharedString::from(key.clone()))
                        .debug_selector(move || key.clone())
                        .flex()
                        .gap(px(SPACE_2))
                        .px(px(SPACE_1_5))
                        .py(px(SPACE_0_5))
                        .rounded(px(R_CONTROL))
                        .cursor_pointer()
                        .hover(|row| row.bg(paint::HOVER))
                        .child(
                            div()
                                .flex_shrink_0()
                                .font_weight(W_LABEL)
                                .child(SharedString::from(skill.name.clone())),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(FS_META))
                                .text_color(rgb(INK_SUBTLE))
                                .child(SharedString::from(skill.description.clone())),
                        )
                        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                            cx.emit(WorkflowsEvent::AddSkill(name.clone()))
                        })),
                );
            }
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_1))
                .children(self.input(&Field::SkillSearch, "Search skills", window, cx))
                .child(list)
        });

        let fresh = step.fresh;
        let fresh_id = step.id.clone();
        let by_merge = step.outcome_routes.is_some();
        let merge_id = step.id.clone();
        let workflow_id = inputs.workflow.clone();
        let name_field = Field::StepName(workflow_id.clone(), step.id.clone());
        let instructions_field = Field::Instructions(workflow_id, step.id.clone());
        let delete_id = step.id.clone();
        div()
            .id("inspector")
            .debug_selector(|| "inspector".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .p(px(SPACE_4))
            .overflow_y_scroll()
            .child(label("Agent"))
            .children(self.input(&name_field, "Name", window, cx))
            .children(self.input(&instructions_field, "Instructions", window, cx))
            .child(
                div()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(
                        "The agent receives these with the task, the earlier agents' results \
                         and your answers.",
                    ),
            )
            .child(label("Required skills"))
            .child(
                div()
                    .debug_selector(|| "inspector-skills".into())
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_1))
                    .child(chips)
                    .children(search)
                    .child(
                        div()
                            .text_size(px(FS_META))
                            .text_color(rgb(INK_SUBTLE))
                            .child(SharedString::from(inputs.skills_note.clone())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(SPACE_1))
                            .child(label("Provider"))
                            .child(chooser(
                                "inspector-provider",
                                provider.to_string(),
                                false,
                                provider_picks(step),
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(SPACE_1))
                            .child(label("Model"))
                            .child(chooser(
                                "inspector-model",
                                model_text,
                                model_flagged,
                                model_picks(step, inputs.models(provider)),
                                cx,
                            )),
                    ),
            )
            .child(label("Effort"))
            .child(chooser(
                "inspector-effort",
                effort_text,
                effort_flagged,
                effort_picks(step, inputs.models(provider)),
                cx,
            ))
            .child(
                div()
                    .id("inspector-fresh")
                    .debug_selector(move || {
                        if fresh {
                            "inspector-fresh-on".into()
                        } else {
                            "inspector-fresh".into()
                        }
                    })
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .cursor_pointer()
                    .child(ferrite_ui::components::switch(
                        "inspector-fresh-switch",
                        fresh,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child("Start fresh every time")
                            .child(
                                div()
                                    .text_size(px(FS_META))
                                    .text_color(rgb(INK_SUBTLE))
                                    .child(
                                        "A new agent on every visit, so a reviewer is not \
                                         biased by its own earlier pass.",
                                    ),
                            ),
                    )
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(WorkflowsEvent::EditStep {
                            step: fresh_id.clone(),
                            change: StepChange {
                                fresh: Some(!fresh),
                                ..Default::default()
                            },
                        })
                    })),
            )
            .child(
                div()
                    .id("inspector-merge")
                    .debug_selector(move || {
                        if by_merge {
                            "inspector-merge-on".into()
                        } else {
                            "inspector-merge".into()
                        }
                    })
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .cursor_pointer()
                    .child(ferrite_ui::components::switch(
                        "inspector-merge-switch",
                        by_merge,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child("Route by merge result")
                            .child(
                                div()
                                    .text_size(px(FS_META))
                                    .text_color(rgb(INK_SUBTLE))
                                    .child(
                                        "One route per result in place of When finished. Only \
                                         a confirmed merge can complete the task.",
                                    ),
                            ),
                    )
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(WorkflowsEvent::EditStep {
                            step: merge_id.clone(),
                            change: StepChange {
                                by_merge_result: Some(!by_merge),
                                ..Default::default()
                            },
                        })
                    })),
            )
            .child(
                div().pt(px(SPACE_2)).child(
                    ferrite_ui::form::destructive_button(
                        "inspector-delete",
                        "Delete agent",
                        false,
                        cx,
                    )
                    .debug_selector(|| "inspector-delete".into())
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(WorkflowsEvent::Delete(delete_id.clone()))
                    })),
                ),
            )
            .into_any_element()
    }
}

impl WorkflowsView {
    /// The approval settings (#222): who approves, the deciding agent and
    /// its instructions, what follows approval and where rework goes,
    /// their problems, and the read-only list of what ends in Completed.
    fn approval_inspector(
        &self,
        inputs: &WorkflowsInputs,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::picker::{picker_button, segmented};
        use ferrite_ui::theme::*;
        let chooser = |selector: &'static str,
                       text: String,
                       flagged: bool,
                       picks: Vec<(ferrite_ui::menu::Item, Pick)>,
                       cx: &mut Context<Self>| {
            let picks = std::rc::Rc::new(picks);
            picker_button(
                selector,
                text,
                flagged,
                move |_, trigger, at, cx: &mut Context<Self>| {
                    cx.emit(WorkflowsEvent::Picks {
                        picks: (*picks).clone(),
                        trigger,
                        at,
                    })
                },
                cx,
            )
        };
        let draft = &inputs.draft;
        let approval = &draft.approval;
        let problems = draft.approval_problems();
        let flagged = |words: &str| {
            problems
                .iter()
                .any(|problem| problem.message.contains(words))
        };
        let label = |text: &'static str| {
            div()
                .text_size(px(FS_META))
                .font_weight(W_LABEL)
                .text_color(rgb(INK_SUBTLE))
                .child(text)
        };
        let meta = |text: &'static str| {
            div()
                .text_size(px(FS_META))
                .text_color(rgb(INK_SUBTLE))
                .child(text)
        };
        let named = |step: &Step| {
            if step.name.trim().is_empty() {
                "Untitled agent".to_string()
            } else {
                step.name.clone()
            }
        };
        let mut body = div()
            .id("approval-settings")
            .debug_selector(|| "approval-settings".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .p(px(SPACE_4))
            .overflow_y_scroll()
            .child(label("Approval"))
            .child(label("Who approves the result"))
            .child(segmented(
                "approval-by",
                approval.by,
                &[(Approver::Human, "You"), (Approver::Agent, "An agent")],
                |_, by, cx: &mut Context<Self>| cx.emit(WorkflowsEvent::Pick(Pick::Approver(by))),
                cx,
            ));
        if approval.by == Approver::Agent {
            let decider = approval.step.as_deref().and_then(|id| draft.step(id));
            let picks = draft
                .steps
                .iter()
                .map(|step| {
                    (
                        ferrite_ui::menu::Item::new(named(step))
                            .checked(decider.is_some_and(|decider| decider.id == step.id)),
                        Pick::Decider(step.id.clone()),
                    )
                })
                .collect();
            body = body
                .child(label("Deciding agent"))
                .child(chooser(
                    "approval-decider",
                    decider.map_or_else(|| "Choose an agent".to_string(), named),
                    flagged("deciding agent") || flagged("agent that decides"),
                    picks,
                    cx,
                ))
                .child(meta(
                    "If this agent's own route leads to approval, it decides in the same run. \
                     Otherwise it runs once more to decide.",
                ))
                .children(self.input(
                    &Field::ApprovalInstructions(inputs.workflow.clone()),
                    "Decision instructions",
                    window,
                    cx,
                ))
                .child(meta(
                    "What to verify, what evidence to return, and when it may approve without you.",
                ));
        }
        let after_picks = std::iter::once(Destination::Complete)
            .chain(
                draft
                    .steps
                    .iter()
                    .map(|step| Destination::Step(step.id.clone())),
            )
            .map(|to| {
                let text = match &to {
                    Destination::Step(id) => draft.step(id).map_or_else(
                        || "Deleted agent".to_string(),
                        |step| format!("Run {}", named(step)),
                    ),
                    _ => "Completed".to_string(),
                };
                (
                    ferrite_ui::menu::Item::new(text).checked(to == approval.after),
                    Pick::AfterApproval(to),
                )
            })
            .collect();
        let rework_picks = draft
            .steps
            .iter()
            .map(|step| Destination::Step(step.id.clone()))
            .chain([Destination::Coordinator])
            .map(|to| {
                (
                    ferrite_ui::menu::Item::new(draft.route_label("", &to))
                        .checked(to == approval.rework_to),
                    Pick::ReworkGoesTo(to),
                )
            })
            .collect();
        let after_text = match &approval.after {
            Destination::Step(id) => draft.step(id).map_or_else(
                || "Deleted agent".to_string(),
                |step| format!("Run {}", named(step)),
            ),
            _ => "Completed".to_string(),
        };
        body = body
            .child(
                div()
                    .flex()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(SPACE_1))
                            .child(label("After approval"))
                            .child(chooser(
                                "approval-after",
                                after_text,
                                flagged("After approval"),
                                after_picks,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(SPACE_1))
                            .child(label("Rework goes to"))
                            .child(chooser(
                                "approval-rework",
                                draft.route_label("", &approval.rework_to),
                                flagged("Rework goes"),
                                rework_picks,
                                cx,
                            )),
                    ),
            )
            .child(meta(
                "You can always rework to another agent from the task itself, for that task only.",
            ));
        for (at, problem) in problems.iter().enumerate() {
            body = body.child(
                div()
                    .debug_selector(move || format!("approval-problem-{at}"))
                    .text_size(px(FS_META))
                    .text_color(rgb(BLOCKED))
                    .child(SharedString::from(problem.message.clone())),
            );
        }
        let paths = draft.completion_paths();
        let mut ends = div()
            .debug_selector(|| "completion-paths".into())
            .flex()
            .flex_col()
            .gap(px(SPACE_1));
        if paths.is_empty() {
            ends = ends.child(
                div()
                    .text_color(rgb(BLOCKED))
                    .child("No route ends in Completed."),
            );
        }
        for (at, path) in paths.into_iter().enumerate() {
            ends = ends.child(
                div()
                    .debug_selector(move || format!("completion-path-{at}"))
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(ferrite_ui::icons::icon(
                        ferrite_ui::icons::CHECK_CIRCLE,
                        ROW_ICON,
                        INK_SUBTLE,
                    ))
                    .child(SharedString::from(path)),
            );
        }
        body.child(
            div()
                .flex()
                .items_center()
                .gap(px(SPACE_2))
                .pt(px(SPACE_2))
                .child(label("Ends in Completed"))
                .child(meta("read only \u{b7} from the routes")),
        )
        .child(ends)
        .child(meta(
            "Change these on the routes of the agents, or with After approval.",
        ))
        .into_any_element()
    }
}
