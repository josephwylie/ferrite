//! The Factory's pickers: their rows, and what a chosen row sets.

use super::*;

impl CockpitView {
    /// Open a picker of `picks` at the pointer.
    pub(crate) fn open_picks(
        &mut self,
        picks: Vec<(ferrite_ui::menu::Item, Pick)>,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.factory_view.as_mut() else {
            return;
        };
        view.picks = picks;
        self.open_context_menu(context_menu::MenuTarget::Factory, at, cx);
    }

    /// The rows of the open Factory picker.
    pub(in crate::cockpit) fn factory_pick_rows(&self) -> Vec<ferrite_ui::menu::Item> {
        self.factory_view
            .as_ref()
            .map(|view| view.picks.iter().map(|(item, _)| item.clone()).collect())
            .unwrap_or_default()
    }

    /// A picker row pressed.
    pub(crate) fn factory_pick(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(pick) = self
            .factory_view
            .as_ref()
            .and_then(|view| view.picks.get(index))
            .map(|(_, pick)| pick.clone())
        else {
            return;
        };
        self.apply_pick(pick, cx);
    }

    pub(crate) fn apply_pick(&mut self, pick: Pick, cx: &mut Context<Self>) {
        let at = |view: &Self, step: &str| {
            view.shown_workflow()
                .and_then(|workflow| workflow.draft.steps.iter().position(|s| s.id == step))
        };
        match pick {
            Pick::Select(workflow) => self.select_workflow(workflow, cx),
            Pick::Create(template) => self.create_workflow(template, cx),
            Pick::Route { step, kind, to } => {
                self.edit_step(step, StepChange::route(kind, to), cx);
            }
            Pick::Provider(step, provider) => {
                self.discover_skills(provider);
                self.edit_step(
                    step,
                    StepChange {
                        provider: Some(provider),
                        model: Some(String::new()),
                        effort: Some(String::new()),
                        ..Default::default()
                    },
                    cx,
                )
            }
            Pick::Model(step, model) => self.edit_step(
                step,
                StepChange {
                    model: Some(model),
                    ..Default::default()
                },
                cx,
            ),
            Pick::Effort(step, effort) => self.edit_step(
                step,
                StepChange {
                    effort: Some(effort),
                    ..Default::default()
                },
                cx,
            ),
            Pick::MoveUp(step) => {
                if let Some(to) = at(self, &step).and_then(|at| at.checked_sub(1)) {
                    self.move_step(step, to, cx);
                }
            }
            Pick::MoveDown(step) => {
                if let Some(at) = at(self, &step) {
                    self.move_step(step, at + 1, cx);
                }
            }
            Pick::AddAfter(step) => self.add_step(Some(step), cx),
            Pick::Delete(step) => self.delete_step(step, cx),
            Pick::Start(pick) => self.apply_start_pick(pick, cx),
            Pick::Process(workflow) => {
                if let Some(sheet) = self
                    .factory_view
                    .as_mut()
                    .and_then(|view| view.sheet.as_mut())
                {
                    sheet.workflow = workflow;
                }
                cx.notify();
            }
            Pick::Task(pick) => self.apply_task_pick(pick, cx),
            Pick::Coordinator(pick) => self.apply_coordinator_pick(pick, cx),
            Pick::Approver(by) => self.edit_approval(
                ApprovalChange {
                    by: Some(by),
                    ..Default::default()
                },
                cx,
            ),
            Pick::Decider(step) => self.edit_approval(
                ApprovalChange {
                    step: Some(step),
                    ..Default::default()
                },
                cx,
            ),
            Pick::AfterApproval(to) => self.edit_approval(
                ApprovalChange {
                    after: Some(to),
                    ..Default::default()
                },
                cx,
            ),
            Pick::ReworkGoesTo(to) => self.edit_approval(
                ApprovalChange {
                    rework_to: Some(to),
                    ..Default::default()
                },
                cx,
            ),
        }
    }

    pub(super) fn route_picks(
        &self,
        step: &Step,
        kind: RouteKind,
    ) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let Some(workflow) = self.shown_workflow() else {
            return Vec::new();
        };
        let current = step.route(kind);
        workflow
            .draft
            .route_options(&step.id, kind)
            .into_iter()
            .map(|to| {
                let item = ferrite_ui::menu::Item::new(workflow.draft.route_label(&step.id, &to))
                    .checked(Some(&to) == current);
                (
                    item,
                    Pick::Route {
                        step: step.id.clone(),
                        kind,
                        to,
                    },
                )
            })
            .collect()
    }

    pub(super) fn provider_picks(&self, step: &Step) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        Provider::ALL
            .into_iter()
            .map(|provider| {
                (
                    ferrite_ui::menu::Item::new(provider.to_string())
                        .checked(step.agent.provider == provider),
                    Pick::Provider(step.id.clone(), provider),
                )
            })
            .collect()
    }

    pub(super) fn model_picks(&self, step: &Step) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let mut picks = vec![(
            ferrite_ui::menu::Item::new("Default").checked(step.agent.model.is_none()),
            Pick::Model(step.id.clone(), String::new()),
        )];
        for model in self.cockpit.model_catalog(step.agent.provider) {
            let chosen = step.agent.model.as_deref() == Some(model.value.as_str());
            picks.push((
                ferrite_ui::menu::Item::new(model.display.clone()).checked(chosen),
                Pick::Model(step.id.clone(), model.value.clone()),
            ));
        }
        picks
    }

    pub(super) fn effort_picks(&self, step: &Step) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let models = self.cockpit.model_catalog(step.agent.provider);
        let mut efforts: Vec<String> = match &step.agent.model {
            Some(value) => models
                .iter()
                .find(|model| &model.value == value)
                .map(|model| model.efforts.clone())
                .unwrap_or_default(),
            None => Vec::new(),
        };
        if efforts.is_empty() {
            for model in &models {
                for effort in &model.efforts {
                    if !efforts.contains(effort) {
                        efforts.push(effort.clone());
                    }
                }
            }
        }
        let mut picks = vec![(
            ferrite_ui::menu::Item::new("Default").checked(step.agent.effort.is_none()),
            Pick::Effort(step.id.clone(), String::new()),
        )];
        for effort in efforts {
            let chosen = step.agent.effort.as_deref() == Some(effort.as_str());
            picks.push((
                ferrite_ui::menu::Item::new(effort.clone()).checked(chosen),
                Pick::Effort(step.id.clone(), effort),
            ));
        }
        picks
    }

    pub(super) fn row_picks(&self, step: &Step) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let id = step.id.clone();
        vec![
            (
                ferrite_ui::menu::Item::new("Move up").shortcut("alt-up"),
                Pick::MoveUp(id.clone()),
            ),
            (
                ferrite_ui::menu::Item::new("Move down").shortcut("alt-down"),
                Pick::MoveDown(id.clone()),
            ),
            (
                ferrite_ui::menu::Item::new("Add agent after"),
                Pick::AddAfter(id.clone()),
            ),
            (
                ferrite_ui::menu::Item::new("Delete agent").destructive(),
                Pick::Delete(id),
            ),
        ]
    }

    pub(super) fn workflow_picks(&self) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let Some(project) = self.factory_project() else {
            return Vec::new();
        };
        let shown = self.shown_id();
        self.cockpit
            .factory()
            .workflows_of(project)
            .map(|workflow| {
                (
                    ferrite_ui::menu::Item::new(workflow_title(workflow))
                        .checked(Some(&workflow.id) == shown.as_ref()),
                    Pick::Select(workflow.id.clone()),
                )
            })
            .collect()
    }

    pub(crate) fn template_picks() -> Vec<(ferrite_ui::menu::Item, Pick)> {
        Template::ALL
            .into_iter()
            .map(|template| {
                (
                    ferrite_ui::menu::Item::new(template.label()),
                    Pick::Create(template),
                )
            })
            .collect()
    }

    /// The New task sheet's process choices: one-off, and each saved
    /// Workflow at its latest version.
    pub(crate) fn process_picks(&self) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let Some(project) = self.factory_project() else {
            return Vec::new();
        };
        let chosen = self
            .factory_view
            .as_ref()
            .and_then(|view| view.sheet.as_ref())
            .and_then(|sheet| sheet.workflow.clone());
        let mut picks = vec![(
            ferrite_ui::menu::Item::new("One-off: a single agent").checked(chosen.is_none()),
            Pick::Process(None),
        )];
        for workflow in self.cockpit.factory().workflows_of(project) {
            let Some(latest) = workflow.latest() else {
                continue;
            };
            picks.push((
                ferrite_ui::menu::Item::new(format!(
                    "{} \u{b7} version {}",
                    latest.definition.name, latest.version
                ))
                .checked(chosen.as_ref() == Some(&workflow.id)),
                Pick::Process(Some(workflow.id.clone())),
            ));
        }
        picks
    }
}
