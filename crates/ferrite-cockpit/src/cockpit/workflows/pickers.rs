//! The Factory's pickers: the rows the window lists from its own state,
//! and what a chosen row sets.

use super::*;

impl CockpitView {
    /// Open a picker of `picks` under its trigger, else at the pointer.
    pub(crate) fn open_picks(
        &mut self,
        picks: Vec<(ferrite_ui::menu::Item, Pick)>,
        trigger: &Trigger,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.update_factory(cx, |view, _| view.set_picks(picks)).is_none() {
            return;
        }
        let target = context_menu::MenuTarget::Factory;
        match trigger.bounds() {
            Some(under) => self.open_context_menu_below(
                target,
                under,
                crate::cockpit::menu_button::Edge::Left,
                false,
                cx,
            ),
            None => self.open_context_menu(target, at, cx),
        }
    }

    /// The rows of the open Factory picker.
    pub(in crate::cockpit) fn factory_pick_rows(&self, cx: &gpui::App) -> Vec<ferrite_ui::menu::Item> {
        self.factory()
            .map(|view| view.read(cx).picks().iter().map(|(item, _)| item.clone()).collect())
            .unwrap_or_default()
    }

    /// A picker row pressed.
    pub(crate) fn factory_pick(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(pick) = self
            .factory()
            .and_then(|view| view.read(cx).picks().get(index))
            .map(|(_, pick)| pick.clone())
        else {
            return;
        };
        self.apply_pick(pick, cx);
    }

    pub(crate) fn apply_pick(&mut self, pick: Pick, cx: &mut Context<Self>) {
        let at = |view: &Self, step: &str, cx: &gpui::App| {
            view.shown_workflow(cx)
                .and_then(|workflow| workflow.draft.steps.iter().position(|s| s.id == step))
        };
        match pick {
            Pick::Select(workflow) => self.select_workflow(workflow, cx),
            Pick::Create(template) => self.create_workflow(template, cx),
            Pick::Route { step, kind, to } => {
                self.edit_step(step, StepChange::route(kind, to), cx);
            }
            Pick::Provider(step, provider) => {
                self.discover_skills(provider, cx);
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
                if let Some(to) = at(self, &step, cx).and_then(|at| at.checked_sub(1)) {
                    self.move_step(step, to, cx);
                }
            }
            Pick::MoveDown(step) => {
                if let Some(at) = at(self, &step, cx) {
                    self.move_step(step, at + 1, cx);
                }
            }
            Pick::AddAfter(step) => self.add_step(Some(step), cx),
            Pick::Delete(step) => self.delete_step(step, cx),
            Pick::Start(pick) => self.apply_start_pick(pick, cx),
            Pick::Process(workflow) => {
                self.set_new_task_workflow(workflow, cx);
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
        cx: &gpui::App,
    ) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let Some(workflow) = self.shown_workflow(cx) else {
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

    /// A row's menu: a move only where the row can go.
    pub(super) fn row_picks(&self, step: &Step, cx: &gpui::App) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let id = step.id.clone();
        let steps = self
            .shown_workflow(cx)
            .map(|workflow| workflow.draft.steps.as_slice())
            .unwrap_or_default();
        let at = steps.iter().position(|each| each.id == step.id);
        let mut picks = Vec::new();
        if at.is_some_and(|at| at > 0) {
            picks.push((
                ferrite_ui::menu::Item::new("Move up").shortcut("alt-up"),
                Pick::MoveUp(id.clone()),
            ));
        }
        if at.is_some_and(|at| at + 1 < steps.len()) {
            picks.push((
                ferrite_ui::menu::Item::new("Move down").shortcut("alt-down"),
                Pick::MoveDown(id.clone()),
            ));
        }
        picks.extend([
            (
                ferrite_ui::menu::Item::new("Add agent after"),
                Pick::AddAfter(id.clone()),
            ),
            (
                ferrite_ui::menu::Item::new("Delete agent").destructive(),
                Pick::Delete(id),
            ),
        ]);
        picks
    }

    pub(super) fn workflow_picks(&self, cx: &gpui::App) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let Some(project) = self.factory_project() else {
            return Vec::new();
        };
        let shown = self.shown_id(cx);
        self.cockpit
            .read(cx)
            .factory(project.machine())
            .workflows_of(project.project())
            .map(|workflow| {
                (
                    ferrite_ui::menu::Item::new(workflow_title(workflow))
                        .checked(Some(&workflow.id) == shown.as_ref()),
                    Pick::Select(workflow.id.clone()),
                )
            })
            .collect()
    }

    /// The New task sheet's process choices: one-off, and each saved
    /// Workflow at its latest version.
    pub(crate) fn process_picks(&self, cx: &gpui::App) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let Some(project) = self.factory_project() else {
            return Vec::new();
        };
        let chosen = self
            .new_task(cx)
            .and_then(|sheet| sheet.read(cx).workflow().cloned());
        let mut picks = vec![(
            ferrite_ui::menu::Item::new("One-off: a single agent").checked(chosen.is_none()),
            Pick::Process(None),
        )];
        for workflow in self
            .cockpit
            .read(cx)
            .factory(project.machine())
            .workflows_of(project.project())
        {
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
