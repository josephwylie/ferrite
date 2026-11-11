//! A Project's Workflows in the Factory view (ADR 0037, #221): an ordered
//! outline of Steps beside an inspector, the pickers that set a Step's
//! routes, Provider, model, Effort and skills, the draft's problems, and
//! Save, which makes the next version and starts nothing.
//!
//! The Workflow lives in the Server's Factory: every edit is one command
//! (`Cockpit::workflow`) and the draft the Server publishes is what is
//! drawn. A text field is a Composer this window keeps for as long as the
//! Factory view is open, so what was typed survives a picker, a revealed
//! field, a notice, a scroll and a switch of surface; the draft keeps it
//! across a save and a reopen.

use std::collections::HashMap;

use super::*;
use ferrite_core::factory::{
    self as core_factory, ApprovalChange, Approver, Catalog, Destination, Problem, RouteKind, Step,
    StepChange, StepId, Template, Workflow, WorkflowId,
};
use ferrite_core::protocol::{Done, WorkflowAction};
use ferrite_pane::composer::{Composer, Edited, Role};
use gpui::component::Disableable as _;

/// The outline's width beside the inspector.
const OUTLINE_W: f32 = 440.;
/// How many skills the search offers at once.
const SKILL_ROWS: usize = 8;

/// What the Factory view shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Surface {
    Board,
    Workflows,
}

/// A text field of the editor.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Field {
    WorkflowName(WorkflowId),
    StepName(WorkflowId, StepId),
    Instructions(WorkflowId, StepId),
    /// The skills picker's search.
    SkillSearch,
    /// Agent approval's decision instructions (#222).
    ApprovalInstructions(WorkflowId),
    /// What each scheduled run's Task is told (#225).
    ScheduleInstructions(WorkflowId),
    /// A GitHub start's extra instructions for every Task.
    ExtraInstructions(WorkflowId),
}

impl Field {
    /// Its selector.
    pub(crate) fn key(&self) -> String {
        match self {
            Field::WorkflowName(_) => "workflow-name".into(),
            Field::StepName(..) => "inspector-name".into(),
            Field::Instructions(..) => "inspector-instructions".into(),
            Field::SkillSearch => "skill-search".into(),
            Field::ApprovalInstructions(_) => "approval-instructions".into(),
            Field::ScheduleInstructions(_) => "start-schedule-instructions".into(),
            Field::ExtraInstructions(_) => "start-extra-instructions".into(),
        }
    }
}

/// What a Factory picker's row does.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Pick {
    Select(WorkflowId),
    Create(Template),
    Route {
        step: StepId,
        kind: RouteKind,
        to: Destination,
    },
    Provider(StepId, Provider),
    /// Empty: the Provider's default.
    Model(StepId, String),
    Effort(StepId, String),
    MoveUp(StepId),
    MoveDown(StepId),
    AddAfter(StepId),
    Delete(StepId),
    /// The New task sheet's process: one-off, or a Workflow.
    Process(Option<WorkflowId>),
    /// A choice on a Task's Rework or Direct this task sheet (#222).
    Task(super::direction::TaskPick),
    /// Who approves the shown Workflow's results.
    Approver(Approver),
    /// The Step that decides agent approval.
    Decider(StepId),
    /// What runs once approved.
    AfterApproval(Destination),
    /// Where the Workflow sends rework.
    ReworkGoesTo(Destination),
    /// A "Tasks start" setting (#225).
    Start(super::starts::StartPick),
}

/// Skills found installed for one Provider in the Project.
pub(crate) struct SkillsFound {
    provider: Provider,
    pending: Option<ferrite_core::providers::commands::Discovery>,
    skills: Vec<ferrite_core::SessionCommand>,
    error: Option<String>,
}

/// The Workflows surface's own state.
pub(crate) struct Workflows {
    pub(crate) selected: Option<WorkflowId>,
    pub(crate) step: Option<StepId>,
    pub(crate) fields: HashMap<Field, Entity<Composer>>,
    pub(crate) focused: Option<Field>,
    pub(crate) outline_focus: FocusHandle,
    /// The skills search is revealed.
    pub(crate) skills_open: bool,
    pub(crate) skills: Vec<SkillsFound>,
    /// The inspector shows the approval settings, not a Step.
    pub(crate) approval: bool,
    /// The inspector shows how Tasks start, not a Step (#225).
    pub(crate) start_open: bool,
    /// What the GitHub login lists for a GitHub start's pickers.
    pub(crate) github: super::starts::GitHubLists,
}

impl Workflows {
    pub(crate) fn new(cx: &mut Context<CockpitView>) -> Self {
        Self {
            selected: None,
            step: None,
            fields: HashMap::new(),
            focused: None,
            outline_focus: cx.focus_handle(),
            skills_open: false,
            skills: Vec::new(),
            approval: false,
            start_open: false,
            github: Default::default(),
        }
    }
}

/// A Step's row, dragged to a new place in the outline.
#[derive(Clone)]
pub(crate) struct StepDrag {
    pub(crate) step: StepId,
    name: SharedString,
}

impl StepDrag {
    /// A drag of `step`, as a press on its row starts one.
    pub(crate) fn of(step: &Step) -> Self {
        Self {
            step: step.id.clone(),
            name: SharedString::from(step.name.clone()),
        }
    }
}

struct StepDragPreview(SharedString);

impl Render for StepDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        use ferrite_ui::theme::*;
        div()
            .px(px(SPACE_3))
            .py(px(SPACE_1))
            .rounded(px(R_CONTROL))
            .bg(paint::RAISED)
            .text_color(rgb(INK_STRONG))
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .child(self.0.clone())
    }
}

fn provider_mark(provider: Provider) -> AnyElement {
    use ferrite_ui::theme::*;
    ferrite_ui::icons::icon(
        match provider {
            Provider::Claude => ferrite_ui::icons::CLAUDE,
            Provider::Codex => ferrite_ui::icons::CODEX,
        },
        BAR_MARK,
        match provider {
            Provider::Claude => PROVIDER_CLAUDE,
            Provider::Codex => PROVIDER_CODEX,
        },
    )
    .into_any_element()
}

/// A Step's skill, matched by name or description.
fn skill_matches(skill: &ferrite_core::SessionCommand, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    filter.is_empty()
        || skill.name.to_lowercase().contains(&filter)
        || skill.description.to_lowercase().contains(&filter)
        || crate::fuzzy::matches(&filter, &skill.name).is_some()
}

/// A command the provider lists that is a skill.
fn is_skill(command: &ferrite_core::SessionCommand) -> bool {
    matches!(
        command.source,
        Some(ferrite_core::CommandSource::Skill) | Some(ferrite_core::CommandSource::Plugin { .. })
    ) || command.path.is_some()
}

impl CockpitView {
    fn factory_project(&self) -> Option<ProjectId> {
        self.factory_view.as_ref().map(|view| view.project)
    }

    /// Switch the Factory view between its Board and its Workflows.
    pub(crate) fn factory_surface(&mut self, surface: Surface, cx: &mut Context<Self>) {
        let Some(view) = self.factory_view.as_mut() else {
            return;
        };
        view.surface = surface;
        view.workflows.focused = None;
        self.ensure_fields(cx);
        cx.notify();
    }

    /// The Workflow the surface shows: the one chosen, else the first.
    pub(crate) fn shown_workflow(&self) -> Option<&Workflow> {
        let view = self.factory_view.as_ref()?;
        let board = self.cockpit.factory();
        view.workflows
            .selected
            .as_deref()
            .and_then(|id| board.workflow(id))
            .filter(|workflow| workflow.project == view.project)
            .or_else(|| board.workflows_of(view.project).next())
    }

    /// The Step the inspector shows: the one chosen, else the first.
    pub(crate) fn shown_step(&self) -> Option<&Step> {
        let workflow = self.shown_workflow()?;
        let chosen = self.factory_view.as_ref()?.workflows.step.as_deref();
        chosen
            .and_then(|id| workflow.draft.step(id))
            .or_else(|| workflow.draft.steps.first())
    }

    /// One Workflow ask; a refusal is said in the view's note.
    pub(crate) fn workflow_ask(
        &mut self,
        action: WorkflowAction,
        cx: &mut Context<Self>,
    ) -> Option<Done> {
        let done = match self.cockpit.workflow(action) {
            Ok(done) => done,
            Err(error) => {
                if let Some(view) = self.factory_view.as_mut() {
                    view.note = Some(SharedString::from(error.to_string()));
                }
                None
            }
        };
        self.ensure_fields(cx);
        cx.notify();
        done
    }

    pub(crate) fn create_workflow(&mut self, template: Template, cx: &mut Context<Self>) {
        let Some(project) = self.factory_project() else {
            return;
        };
        if let Some(Done::Workflow { workflow }) =
            self.workflow_ask(WorkflowAction::Create { project, template }, cx)
        {
            self.select_workflow(workflow, cx);
        }
    }

    pub(crate) fn select_workflow(&mut self, workflow: WorkflowId, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_view.as_mut() {
            view.surface = Surface::Workflows;
            view.workflows.selected = Some(workflow);
            view.workflows.step = None;
            view.workflows.focused = None;
            view.workflows.start_open = false;
        }
        self.ensure_fields(cx);
        cx.notify();
    }

    pub(crate) fn select_step(&mut self, step: StepId, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_view.as_mut() {
            view.workflows.step = Some(step);
            view.workflows.focused = None;
            view.workflows.approval = false;
            view.workflows.start_open = false;
        }
        self.ensure_fields(cx);
        cx.notify();
    }

    /// Show the approval settings in the inspector.
    pub(crate) fn select_approval(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_view.as_mut() {
            view.workflows.approval = true;
            view.workflows.focused = None;
        }
        self.ensure_fields(cx);
        cx.notify();
    }

    /// Change the shown Workflow's approval settings.
    pub(crate) fn edit_approval(&mut self, change: ApprovalChange, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_id() else {
            return;
        };
        self.workflow_ask(WorkflowAction::EditApproval { workflow, change }, cx);
    }

    /// The shown Workflow's id.
    fn shown_id(&self) -> Option<WorkflowId> {
        self.shown_workflow().map(|workflow| workflow.id.clone())
    }

    pub(crate) fn add_step(&mut self, after: Option<StepId>, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_id() else {
            return;
        };
        if let Some(Done::Step { step }) =
            self.workflow_ask(WorkflowAction::AddStep { workflow, after }, cx)
        {
            self.select_step(step, cx);
        }
    }

    pub(crate) fn edit_step(&mut self, step: StepId, change: StepChange, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_id() else {
            return;
        };
        self.workflow_ask(
            WorkflowAction::EditStep {
                workflow,
                step,
                change,
            },
            cx,
        );
    }

    /// Move a Step to index `to` of the outline: a drop, or a key.
    pub(crate) fn move_step(&mut self, step: StepId, to: usize, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_id() else {
            return;
        };
        self.workflow_ask(WorkflowAction::MoveStep { workflow, step, to }, cx);
    }

    /// A dragged row let go on the row at `onto`: it takes that place.
    pub(crate) fn drop_step(&mut self, drag: &StepDrag, onto: usize, cx: &mut Context<Self>) {
        self.select_step(drag.step.clone(), cx);
        self.move_step(drag.step.clone(), onto, cx);
    }

    pub(crate) fn delete_step(&mut self, step: StepId, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_id() else {
            return;
        };
        let at = self
            .shown_workflow()
            .and_then(|shown| shown.draft.steps.iter().position(|each| each.id == step));
        self.workflow_ask(WorkflowAction::DeleteStep { workflow, step }, cx);
        // The selection lands on the row that took its place.
        let next = self.shown_workflow().and_then(|shown| {
            let steps = &shown.draft.steps;
            at.and_then(|at| steps.get(at.min(steps.len().saturating_sub(1))))
                .map(|step| step.id.clone())
        });
        if let Some(view) = self.factory_view.as_mut() {
            view.workflows.step = next;
        }
        self.ensure_fields(cx);
        cx.notify();
    }

    /// The outline's keys: ↑ ↓ choose a Step, ⌥↑ ⌥↓ move it.
    pub(crate) fn outline_key(&mut self, keystroke: &gpui::Keystroke, cx: &mut Context<Self>) {
        let (Some(workflow), Some(step)) = (self.shown_workflow(), self.shown_step()) else {
            return;
        };
        let steps: Vec<StepId> = workflow.draft.steps.iter().map(|s| s.id.clone()).collect();
        let step = step.id.clone();
        let Some(at) = steps.iter().position(|each| *each == step) else {
            return;
        };
        let up = match keystroke.key.as_str() {
            "up" => true,
            "down" => false,
            _ => return,
        };
        let to = if up {
            at.checked_sub(1)
        } else {
            (at + 1 < steps.len()).then_some(at + 1)
        };
        let Some(to) = to else {
            return;
        };
        if keystroke.modifiers.alt {
            self.move_step(step, to, cx);
        } else {
            self.select_step(steps[to].clone(), cx);
        }
    }

    /// Keep a Composer for each text field the surface shows, filled from
    /// the draft when it is first made; one already made keeps what was
    /// typed in it.
    pub(crate) fn ensure_fields(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.factory_view.as_ref() else {
            return;
        };
        if view.surface != Surface::Workflows {
            return;
        }
        let mut wanted: Vec<(Field, String)> = vec![(Field::SkillSearch, String::new())];
        if let Some(workflow) = self.shown_workflow() {
            wanted.push((
                Field::WorkflowName(workflow.id.clone()),
                workflow.draft.name.clone(),
            ));
            wanted.push((
                Field::ApprovalInstructions(workflow.id.clone()),
                workflow.draft.approval.instructions.clone(),
            ));
            if let Some(step) = self.shown_step() {
                wanted.push((
                    Field::StepName(workflow.id.clone(), step.id.clone()),
                    step.name.clone(),
                ));
                wanted.push((
                    Field::Instructions(workflow.id.clone(), step.id.clone()),
                    step.instructions.clone(),
                ));
            }
        }
        wanted.extend(self.start_fields());
        for (field, text) in wanted {
            let made = self
                .factory_view
                .as_ref()
                .is_some_and(|view| view.workflows.fields.contains_key(&field));
            if made {
                continue;
            }
            let composer = cx.new(Composer::new);
            composer.update(cx, |composer, cx| {
                composer.set_role(Role::Field(None), cx);
                if !text.is_empty() {
                    composer.insert(&text, cx);
                }
            });
            let key = field.clone();
            cx.subscribe(&composer, move |view, _, _: &Edited, cx| {
                view.field_edited(&key, cx)
            })
            .detach();
            if let Some(view) = self.factory_view.as_mut() {
                view.workflows.fields.insert(field, composer);
            }
        }
        if let Some(provider) = self.shown_step().map(|step| step.agent.provider) {
            self.discover_skills(provider);
        }
    }

    /// What was typed in a field goes to the draft at once.
    fn field_edited(&mut self, field: &Field, cx: &mut Context<Self>) {
        let Some(text) = self
            .factory_view
            .as_ref()
            .and_then(|view| view.workflows.fields.get(field))
            .map(|composer| composer.read(cx).text().to_string())
        else {
            return;
        };
        let action = match field.clone() {
            Field::WorkflowName(workflow) => WorkflowAction::Rename {
                workflow,
                name: text,
            },
            Field::StepName(workflow, step) => WorkflowAction::EditStep {
                workflow,
                step,
                change: StepChange {
                    name: Some(text),
                    ..Default::default()
                },
            },
            Field::Instructions(workflow, step) => WorkflowAction::EditStep {
                workflow,
                step,
                change: StepChange {
                    instructions: Some(text),
                    ..Default::default()
                },
            },
            Field::SkillSearch => {
                cx.notify();
                return;
            }
            Field::ApprovalInstructions(workflow) => WorkflowAction::EditApproval {
                workflow,
                change: ApprovalChange {
                    instructions: Some(text),
                    ..Default::default()
                },
            },
            Field::ScheduleInstructions(_) | Field::ExtraInstructions(_) => {
                self.start_field_edited(field, text, cx);
                return;
            }
        };
        self.workflow_ask(action, cx);
    }

    /// Give a text field the keyboard.
    pub(crate) fn focus_field(&mut self, field: Field, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_view.as_mut() {
            view.workflows.focused = Some(field);
        }
        cx.notify();
    }

    /// The text a field holds now.
    pub(crate) fn field_text(&self, field: &Field, cx: &gpui::App) -> Option<String> {
        self.factory_view
            .as_ref()?
            .workflows
            .fields
            .get(field)
            .map(|composer| composer.read(cx).text().to_string())
    }

    /// What holds the keyboard on the Workflows surface: the field
    /// chosen, else the outline.
    pub(super) fn workflows_focus(&self, cx: &gpui::App) -> Option<FocusHandle> {
        let view = self.factory_view.as_ref()?;
        if view.surface != Surface::Workflows {
            return None;
        }
        let workflows = &view.workflows;
        Some(
            workflows
                .focused
                .as_ref()
                .and_then(|field| workflows.fields.get(field))
                .map(|composer| composer.read(cx).focus_handle(cx))
                .unwrap_or_else(|| workflows.outline_focus.clone()),
        )
    }

    // ----------------------------------------------------------- skills

    /// Ask the Provider which skills it has installed in the Project, once.
    fn discover_skills(&mut self, provider: Provider) {
        let Some(view) = self.factory_view.as_ref() else {
            return;
        };
        if view
            .workflows
            .skills
            .iter()
            .any(|found| found.provider == provider)
        {
            return;
        }
        let Some(cwd) = self
            .cockpit
            .registry()
            .project(view.project)
            .map(|project| project.root.clone())
        else {
            return;
        };
        let pending = self.cockpit.discover_commands(provider, &cwd);
        let error = pending
            .is_none()
            .then(|| "This window cannot list skills.".to_string());
        if let Some(view) = self.factory_view.as_mut() {
            view.workflows.skills.push(SkillsFound {
                provider,
                pending,
                skills: Vec::new(),
                error,
            });
        }
    }

    /// A discovery that answered: its skills, for the picker and the check.
    pub(super) fn poll_factory_skills(&mut self, cx: &mut Context<Self>) {
        self.poll_start_lists(cx);
        let Some(view) = self.factory_view.as_mut() else {
            return;
        };
        let mut heard = false;
        for found in &mut view.workflows.skills {
            let Some(pending) = &found.pending else {
                continue;
            };
            let answer = match pending.try_recv() {
                Ok(answer) => answer,
                Err(std::sync::mpsc::TryRecvError::Empty) => continue,
                Err(error) => Err(std::io::Error::other(error)),
            };
            found.pending = None;
            heard = true;
            match answer {
                Ok(commands) => found.skills = commands.into_iter().filter(is_skill).collect(),
                Err(error) => found.error = Some(error.to_string()),
            }
        }
        if heard {
            cx.notify();
        }
    }

    /// The skills found for `provider`, once its discovery answered.
    fn skills_for(&self, provider: Provider) -> Option<&[ferrite_core::SessionCommand]> {
        self.factory_view
            .as_ref()?
            .workflows
            .skills
            .iter()
            .find(|found| found.provider == provider && found.pending.is_none())
            .filter(|found| found.error.is_none())
            .map(|found| found.skills.as_slice())
    }

    /// What is known installed: models as the Providers announced them,
    /// skills as discovered.
    pub(crate) fn factory_catalog(&self) -> Catalog {
        Catalog {
            models: Provider::ALL
                .into_iter()
                .map(|provider| (provider, self.cockpit.announced_models(provider)))
                .collect(),
            skills: Provider::ALL
                .into_iter()
                .filter_map(|provider| {
                    let skills = self.skills_for(provider)?;
                    Some((provider, skills.iter().map(|s| s.name.clone()).collect()))
                })
                .collect(),
        }
    }

    /// The shown draft's problems.
    pub(crate) fn draft_problems(&self) -> Vec<Problem> {
        self.shown_workflow()
            .map(|workflow| workflow.draft.problems(&self.factory_catalog()))
            .unwrap_or_default()
    }

    pub(crate) fn add_skill(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(step) = self.shown_step() else {
            return;
        };
        if step.skills.contains(&name) {
            return;
        }
        let mut skills = step.skills.clone();
        skills.push(name);
        let id = step.id.clone();
        self.edit_step(
            id,
            StepChange {
                skills: Some(skills),
                ..Default::default()
            },
            cx,
        );
    }

    pub(crate) fn remove_skill(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(step) = self.shown_step() else {
            return;
        };
        let skills: Vec<String> = step
            .skills
            .iter()
            .filter(|each| *each != name)
            .cloned()
            .collect();
        let id = step.id.clone();
        self.edit_step(
            id,
            StepChange {
                skills: Some(skills),
                ..Default::default()
            },
            cx,
        );
    }

    pub(crate) fn reveal_skills(&mut self, open: bool, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_view.as_mut() {
            view.workflows.skills_open = open;
            view.workflows.focused = open.then_some(Field::SkillSearch);
        }
        cx.notify();
    }

    // ----------------------------------------------------------- save

    /// Save: refused with the problems listed, else the next version.
    /// Nothing starts.
    pub(crate) fn save_workflow(&mut self, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_id() else {
            return;
        };
        let problems = self.draft_problems();
        let note = if !problems.is_empty() {
            Some(SharedString::from("Fix the problems listed before saving."))
        } else {
            let skills = self.factory_catalog().skills;
            match self.workflow_ask(WorkflowAction::Save { workflow, skills }, cx) {
                Some(Done::Version { version }) => {
                    Some(SharedString::from(format!("Saved version {version}.")))
                }
                _ => return,
            }
        };
        if let Some(view) = self.factory_view.as_mut() {
            view.note = note;
        }
        cx.notify();
    }

    /// Dismiss the view's notice.
    pub(crate) fn dismiss_factory_note(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_view.as_mut() {
            view.note = None;
        }
        cx.notify();
    }

    // ----------------------------------------------------------- pickers

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
    pub(super) fn factory_pick_rows(&self) -> Vec<ferrite_ui::menu::Item> {
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
                let change = match kind {
                    RouteKind::Finished => StepChange {
                        on_finished: Some(to),
                        ..Default::default()
                    },
                    RouteKind::Unable => StepChange {
                        on_unable: Some(to),
                        ..Default::default()
                    },
                };
                self.edit_step(step, change, cx);
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

    fn route_picks(&self, step: &Step, kind: RouteKind) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let Some(workflow) = self.shown_workflow() else {
            return Vec::new();
        };
        let current = match kind {
            RouteKind::Finished => &step.on_finished,
            RouteKind::Unable => &step.on_unable,
        };
        workflow
            .draft
            .route_options(&step.id, kind)
            .into_iter()
            .map(|to| {
                let item = ferrite_ui::menu::Item::new(workflow.draft.route_label(&step.id, &to))
                    .checked(&to == current);
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

    fn provider_picks(&self, step: &Step) -> Vec<(ferrite_ui::menu::Item, Pick)> {
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

    fn model_picks(&self, step: &Step) -> Vec<(ferrite_ui::menu::Item, Pick)> {
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

    fn effort_picks(&self, step: &Step) -> Vec<(ferrite_ui::menu::Item, Pick)> {
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

    fn row_picks(&self, step: &Step) -> Vec<(ferrite_ui::menu::Item, Pick)> {
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

    fn workflow_picks(&self) -> Vec<(ferrite_ui::menu::Item, Pick)> {
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

    // ----------------------------------------------------------- drawing

    /// The Factory head's surface tabs: Board and Workflows.
    pub(super) fn factory_surface_tabs(&self, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let Some(view) = self.factory_view.as_ref() else {
            return div();
        };
        let board = self.cockpit.factory();
        let counts = [
            (Surface::Board, "Board", board.of(view.project).count()),
            (
                Surface::Workflows,
                "Workflows",
                board.workflows_of(view.project).count(),
            ),
        ];
        let mut tabs = div().flex().items_center().gap(px(SPACE_1));
        for (surface, word, count) in counts {
            let key = format!("factory-tab-{}", word.to_lowercase());
            let selected = view.surface == surface;
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
                        view.factory_surface(surface, cx)
                    })),
            );
        }
        tabs
    }

    /// The Workflows surface: its head, the draft's problems, the outline
    /// and the inspector.
    pub(super) fn workflows_surface(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let Some(workflow) = self.shown_workflow() else {
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
        let problems = workflow.draft.problems(&self.factory_catalog());
        let unsaved = workflow.unsaved();
        let version = match workflow.latest() {
            Some(latest) if unsaved => format!("version {} \u{b7} edited", latest.version),
            Some(latest) => format!("version {}", latest.version),
            None => "not saved".into(),
        };
        let runnable = workflow.latest().is_some() && problems.is_empty();
        let save_disabled = !unsaved || !problems.is_empty();
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
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            let picks = view.workflow_picks();
                            view.open_picks(picks, event.position, cx);
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
                    let workflow = view.shown_id();
                    view.open_new_task(cx);
                    if let Some(sheet) = view
                        .factory_view
                        .as_mut()
                        .and_then(|shown| shown.sheet.as_mut())
                    {
                        sheet.workflow = workflow;
                    }
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
                    .child(self.outline(workflow, &problems, window, cx))
                    .child(self.inspector(workflow, window, cx)),
            )
    }

    fn new_workflow_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use ferrite_ui::theme::*;
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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_picks(Self::template_picks(), event.position, cx);
                }),
            )
    }

    /// The outline: the Workflow's name, then each Step's row with its
    /// routes, in order. No box round a row; the chosen row is filled.
    fn outline(
        &self,
        workflow: &Workflow,
        problems: &[Problem],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let Some(view) = self.factory_view.as_ref() else {
            return div().into_any_element();
        };
        // While the start or the approval settings show, no Step's row
        // is the chosen one.
        let shown = self
            .shown_step()
            .map(|step| step.id.clone())
            .filter(|_| !view.workflows.approval && !self.start_shown());
        let name_field = Field::WorkflowName(workflow.id.clone());
        let mut outline = div()
            .id("outline")
            .debug_selector(|| "outline".into())
            .track_focus(&view.workflows.outline_focus)
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
            .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, _, cx| {
                view.outline_key(&event.keystroke, cx)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _: &MouseDownEvent, _, cx| {
                    if let Some(view) = view.factory_view.as_mut() {
                        view.workflows.focused = None;
                    }
                    cx.notify();
                }),
            )
            .children(self.text_input(&name_field, "Workflow name", window, cx))
            .child(self.start_outline_row(workflow, cx))
            .child(
                div()
                    .flex()
                    .gap(px(SPACE_2))
                    .pt(px(SPACE_2))
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(div().font_weight(W_LABEL).child("Agents, in order"))
                    .child("\u{201c}Next agent\u{201d} follows this order"),
            );
        for (at, step) in workflow.draft.steps.iter().enumerate() {
            let flagged = problems
                .iter()
                .any(|problem| problem.step.as_deref() == Some(step.id.as_str()));
            outline = outline.child(self.outline_row(
                workflow,
                step,
                at,
                shown.as_deref() == Some(step.id.as_str()),
                flagged,
                cx,
            ));
        }
        let approval_flagged = problems.iter().any(|problem| problem.approval);
        let approval_shown = view.workflows.approval;
        let approval = &workflow.draft.approval;
        let decides = match approval.by {
            Approver::Human => "You approve".to_string(),
            Approver::Agent => match approval
                .step
                .as_deref()
                .and_then(|id| workflow.draft.step(id))
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
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.select_approval(cx)
                    })),
            )
            .child(
                div().pt(px(SPACE_2)).child(
                    ferrite_ui::components::ghost_button("outline-add", "+ Add agent", cx)
                        .debug_selector(|| "outline-add".into())
                        .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            view.add_step(None, cx);
                        })),
                ),
            )
            .into_any_element()
    }

    fn outline_row(
        &self,
        workflow: &Workflow,
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
        let mut meta = vec![core_factory::agent_label(&step.agent)
            .split(" \u{b7} ")
            .last()
            .unwrap_or_default()
            .to_string()];
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
            let label = workflow.draft.route_label(&step.id, to);
            let broken = workflow.draft.broken(&step.id, to).is_some();
            let which = match kind {
                RouteKind::Finished => "finished",
                RouteKind::Unable => "unable",
            };
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
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                let Some(step) = view
                                    .shown_workflow()
                                    .and_then(|workflow| workflow.draft.step(&step_id))
                                    .cloned()
                                else {
                                    return;
                                };
                                let picks = view.route_picks(&step, kind);
                                view.open_picks(picks, event.position, cx);
                            }),
                        ),
                )
        };
        let finished = route(RouteKind::Finished, &step.on_finished, cx);
        let unable = route(RouteKind::Unable, &step.on_unable, cx);
        let more_id = step.id.clone();
        let more_key = format!("{key}-more");
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
            .on_drop(cx.listener(move |view, drag: &StepDrag, _, cx| view.drop_step(drag, at, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    view.select_step(select.clone(), cx)
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
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                                            cx.stop_propagation();
                                            let Some(step) = view
                                                .shown_workflow()
                                                .and_then(|workflow| workflow.draft.step(&more_id))
                                                .cloned()
                                            else {
                                                return;
                                            };
                                            let picks = view.row_picks(&step);
                                            view.open_picks(picks, event.position, cx);
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
                    .child(finished)
                    .child(unable),
            )
            .into_any_element()
    }

    /// One text field: its label over its Composer, ringed while it has
    /// the keyboard.
    pub(super) fn text_input(
        &self,
        field: &Field,
        label: &'static str,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let composer = self.factory_view.as_ref()?.workflows.fields.get(field)?;
        let focused = composer.read(cx).focus_handle(cx).is_focused(window);
        let key = field.key();
        let chosen = field.clone();
        Some(
            div()
                .id(SharedString::from(key.clone()))
                .debug_selector(move || key.clone())
                .capture_any_mouse_down(cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    view.focus_field(chosen.clone(), cx)
                }))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(crate::project_editor::text_field(
                    label.into(),
                    composer.clone(),
                    focused,
                    "factory-field-focused",
                ))
                .into_any_element(),
        )
    }

    /// The inspector: the chosen Step's name, instructions, skills,
    /// Provider, model, Effort and "Start fresh every time".
    fn inspector(
        &self,
        workflow: &Workflow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        if self.start_shown() {
            return self.start_inspector(workflow, window, cx);
        }
        if self
            .factory_view
            .as_ref()
            .is_some_and(|view| view.workflows.approval)
        {
            return self.approval_inspector(workflow, window, cx);
        }
        let Some(step) = self.shown_step() else {
            return div()
                .debug_selector(|| "inspector-none".into())
                .flex_1()
                .p(px(SPACE_4))
                .text_color(rgb(INK_SUBTLE))
                .child("Add an agent to begin.")
                .into_any_element();
        };
        let Some(view) = self.factory_view.as_ref() else {
            return div().into_any_element();
        };
        let catalog = self.factory_catalog();
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
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.open_picks((*picks).clone(), event.position, cx);
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
        let installed = self.skills_for(provider);
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
                                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                                    cx.stop_propagation();
                                    view.remove_skill(&remove, cx);
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
                .child(if view.workflows.skills_open {
                    "Done"
                } else {
                    "+ Add skill"
                })
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    let open = view
                        .factory_view
                        .as_ref()
                        .is_some_and(|view| view.workflows.skills_open);
                    view.reveal_skills(!open, cx);
                })),
        );
        let found = view
            .workflows
            .skills
            .iter()
            .find(|found| found.provider == provider);
        let skills_note = match found {
            Some(found) if found.pending.is_some() => "Looking for installed skills\u{2026}".into(),
            Some(found) if found.error.is_some() => format!(
                "Could not list skills: {}",
                found.error.clone().unwrap_or_default()
            ),
            Some(found) => format!("{} installed for {provider}", found.skills.len()),
            None => String::new(),
        };
        let search = view.workflows.skills_open.then(|| {
            let filter = self.field_text(&Field::SkillSearch, cx).unwrap_or_default();
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
                        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                            view.add_skill(name.clone(), cx)
                        })),
                );
            }
            div()
                .flex()
                .flex_col()
                .gap(px(SPACE_1))
                .children(self.text_input(&Field::SkillSearch, "Search skills", window, cx))
                .child(list)
        });

        let fresh = step.fresh;
        let fresh_id = step.id.clone();
        let workflow_id = workflow.id.clone();
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
            .children(self.text_input(&name_field, "Name", window, cx))
            .children(self.text_input(&instructions_field, "Instructions", window, cx))
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
                            .child(SharedString::from(skills_note)),
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
                                self.provider_picks(step),
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
                                self.model_picks(step),
                                cx,
                            )),
                    ),
            )
            .child(label("Effort"))
            .child(chooser(
                "inspector-effort",
                effort_text,
                effort_flagged,
                self.effort_picks(step),
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
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.edit_step(
                            fresh_id.clone(),
                            StepChange {
                                fresh: Some(!fresh),
                                ..Default::default()
                            },
                            cx,
                        )
                    })),
            )
            .child(
                div().pt(px(SPACE_2)).child(
                    crate::project_editor::destructive_button(
                        "inspector-delete",
                        "Delete agent",
                        false,
                        cx,
                    )
                    .debug_selector(|| "inspector-delete".into())
                    .on_click(cx.listener(
                        move |view, _: &ClickEvent, _, cx| view.delete_step(delete_id.clone(), cx),
                    )),
                ),
            )
            .into_any_element()
    }
}

impl CockpitView {
    /// The approval settings (#222): who approves, the deciding agent and
    /// its instructions, what follows approval and where rework goes,
    /// their problems, and the read-only list of what ends in Completed.
    fn approval_inspector(
        &self,
        workflow: &Workflow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use super::direction::{chooser, segmented};
        use ferrite_ui::theme::*;
        let draft = &workflow.draft;
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
                |view, by, cx| view.apply_pick(Pick::Approver(by), cx),
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
                .children(self.text_input(
                    &Field::ApprovalInstructions(workflow.id.clone()),
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

/// A Workflow as its picker names it.
fn workflow_title(workflow: &Workflow) -> String {
    if workflow.draft.name.trim().is_empty() {
        "Untitled workflow".into()
    } else {
        workflow.draft.name.clone()
    }
}
