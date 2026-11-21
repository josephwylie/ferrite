//! The Workflows editor's own state, in a view of its own (ADR 0041):
//! the Workflow and Step chosen, what the inspector shows, whether the
//! skills search is revealed, the outline's focus and scroll, each text
//! field's Composer, made once for as long as the Factory is open, and
//! the field that holds the keyboard. The Factory holds it.
//!
//! Its render is the Workflows surface: the head, the draft's problems,
//! the outline (`outline`) and the inspector (`inspector`), or the empty
//! state, with what it reads of the window (`WorkflowsInputs`). Which
//! fields the surface shows the root decides from the draft
//! (`ensure_fields`); what is typed or pressed it says as a
//! `WorkflowsEvent`, since every edit is a Server ask.

use gpui::WeakEntity;

use super::*;

/// What the editor asks of the window.
pub(crate) enum WorkflowsEvent {
    /// A field was typed in: what it holds goes to the draft.
    Edited(Field),
    /// A press on a field or the outline: what holds the keyboard moved.
    Focused,
    /// A Step's row pressed, or ↑ ↓: the inspector shows it.
    Select(StepId),
    /// The approval's row: the inspector shows the approval settings.
    Approval,
    /// The start's row: the inspector shows how Tasks start.
    Start,
    /// ⌥↑ ⌥↓: the Step moves to index `to` of the outline.
    Move { step: StepId, to: usize },
    /// A dragged row let go on the row at `onto`.
    Drop { drag: StepDrag, onto: usize },
    /// A route's picker on a Step's row: its rows open under `trigger`.
    Route {
        step: StepId,
        kind: RouteKind,
        trigger: Trigger,
        at: Point<Pixels>,
    },
    /// A Step's row's `⋯`: its menu opens under `trigger`.
    RowMenu {
        step: StepId,
        trigger: Trigger,
        at: Point<Pixels>,
    },
    /// `+ Add agent`: a new Step at the end.
    Add,
    /// A chooser in the inspector: its rows open under `trigger`.
    Picks {
        picks: Vec<(ferrite_ui::menu::Item, Pick)>,
        trigger: Trigger,
        at: Point<Pixels>,
    },
    /// A choice made in place: a segment.
    Pick(Pick),
    /// A Step's switch: its setting changes.
    EditStep { step: StepId, change: StepChange },
    /// A skill from the search, for the Step shown.
    AddSkill(String),
    /// A skill's chip closed.
    RemoveSkill(String),
    /// `Delete agent`.
    Delete(StepId),
    /// The head's Workflow picker: the Project's Workflows open under
    /// `trigger`.
    Workflows { trigger: Trigger, at: Point<Pixels> },
    /// `Run task`: the New task sheet, on the Workflow shown.
    Run,
    /// `Save`: the draft becomes the next version.
    Save,
}

/// What the editor draws from the window: the Workflow shown, the Step
/// shown, the draft's problems, what is known installed and the starts'
/// view.
pub(crate) struct WorkflowsInputs {
    pub(super) workflow: WorkflowId,
    /// Its name as its picker says it.
    pub(super) title: String,
    /// Its latest saved version's number, if one was saved.
    pub(super) saved: Option<u32>,
    /// The draft differs from that version, or none was saved.
    pub(super) unsaved: bool,
    pub(super) draft: Definition,
    /// The Step shown: the one chosen, else the first.
    pub(super) step: Option<StepId>,
    /// Its automatic starts' switch, which the start's row says.
    pub(super) switch: Switch,
    pub(super) problems: Vec<Problem>,
    /// What the problems were found against: models as the Providers
    /// announced them, skills as discovered.
    pub(super) catalog: Catalog,
    /// Each Provider's models, as a row names them and a picker offers them.
    pub(super) models: Vec<(Provider, Vec<ferrite_core::ModelInfo>)>,
    /// The skills the shown Step's Provider has installed, once listed.
    pub(super) installed: Option<Vec<ferrite_core::SessionCommand>>,
    /// What the inspector says of them.
    pub(super) skills_note: String,
    /// The start's settings, drawn by their own view.
    pub(super) starts: Entity<super::super::starts::StartsView>,
}

impl WorkflowsInputs {
    /// `provider`'s models.
    pub(super) fn models(&self, provider: Provider) -> &[ferrite_core::ModelInfo] {
        self.models
            .iter()
            .find(|(each, _)| *each == provider)
            .map_or(&[], |(_, models)| models)
    }
}

/// What the inspector shows beside the outline, whose row for it is the
/// one chosen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Inspected {
    /// The chosen Step's settings.
    #[default]
    Step,
    /// The approval settings (#222).
    Approval,
    /// How Tasks start (#225).
    Start,
}

/// A Factory's Workflows editor.
pub(crate) struct WorkflowsView {
    cockpit: WeakEntity<CockpitView>,
    /// The Workflow chosen; none, the Project's first.
    pub(crate) selected: Option<WorkflowId>,
    /// The Step chosen; none, the Workflow's first.
    pub(crate) step: Option<StepId>,
    pub(crate) inspected: Inspected,
    /// The skills search is revealed.
    pub(super) skills_open: bool,
    /// What holds the keyboard while no field does.
    pub(crate) outline_focus: FocusHandle,
    /// The outline's scroll: its first child is its head, then one per
    /// Step's row (`outline`).
    pub(crate) outline_scroll: gpui::ScrollHandle,
    /// Each text field's Composer, made once.
    fields: HashMap<Field, Entity<Composer>>,
    /// The field that holds the keyboard; none, the outline.
    pub(crate) focused: Option<Field>,
}

impl EventEmitter<WorkflowsEvent> for WorkflowsView {}

impl WorkflowsView {
    fn new(cockpit: WeakEntity<CockpitView>, cx: &mut Context<Self>) -> Self {
        Self {
            cockpit,
            selected: None,
            step: None,
            inspected: Inspected::Step,
            skills_open: false,
            outline_focus: cx.focus_handle(),
            outline_scroll: gpui::ScrollHandle::new(),
            fields: HashMap::new(),
            focused: None,
        }
    }

    /// Show `workflow`, on its first Step.
    pub(super) fn select_workflow(&mut self, workflow: WorkflowId) {
        self.selected = Some(workflow);
        self.step = None;
        self.inspect(Inspected::Step);
    }

    /// Show `step` in the inspector.
    pub(super) fn select_step(&mut self, step: StepId) {
        self.step = Some(step);
        self.inspect(Inspected::Step);
    }

    /// Show `inspected` in the inspector; no field keeps the keyboard.
    pub(crate) fn inspect(&mut self, inspected: Inspected) {
        self.inspected = inspected;
        self.focused = None;
    }

    /// The Step `draft` shows: the one chosen, else the first.
    pub(super) fn shown_step<'a>(&self, draft: &'a Definition) -> Option<&'a Step> {
        self.step
            .as_deref()
            .and_then(|id| draft.step(id))
            .or_else(|| draft.steps.first())
    }

    /// The outline's keys: ↑ ↓ choose a Step, ⌥↑ ⌥↓ move it, read against
    /// the draft as it is now.
    pub(super) fn outline_key(&mut self, keystroke: &gpui::Keystroke, cx: &mut Context<Self>) {
        let Some(inputs) = self
            .cockpit
            .upgrade()
            .and_then(|cockpit| cockpit.read(cx).workflows_inputs(self))
        else {
            return;
        };
        let steps = &inputs.draft.steps;
        let Some(at) = inputs
            .step
            .as_ref()
            .and_then(|step| steps.iter().position(|each| each.id == *step))
        else {
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
        cx.emit(if keystroke.modifiers.alt {
            WorkflowsEvent::Move {
                step: steps[at].id.clone(),
                to,
            }
        } else {
            WorkflowsEvent::Select(steps[to].id.clone())
        });
        // The chosen row stays in view: the head is the outline's first child.
        self.outline_scroll.scroll_to_item(to + 1);
    }

    /// `field`'s Composer, filled with `text` when it is first made; one
    /// already made keeps what was typed in it.
    pub(super) fn ensure(&mut self, field: Field, text: &str, cx: &mut Context<Self>) {
        if self.fields.contains_key(&field) {
            return;
        }
        let composer = cx.new(Composer::new);
        composer.update(cx, |composer, cx| {
            composer.set_role(Role::Field(None), cx);
            if !text.is_empty() {
                composer.insert(text, cx);
            }
        });
        let key = field.clone();
        cx.subscribe(&composer, move |_, _, _: &Edited, cx| {
            cx.emit(WorkflowsEvent::Edited(key.clone()))
        })
        .detach();
        self.fields.insert(field, composer);
    }

    pub(crate) fn field(&self, field: &Field) -> Option<&Entity<Composer>> {
        self.fields.get(field)
    }

    /// What holds the keyboard: the chosen field, if it is made.
    pub(super) fn focus(&self, cx: &gpui::App) -> Option<FocusHandle> {
        let composer = self.fields.get(self.focused.as_ref()?)?;
        Some(composer.read(cx).focus_handle(cx))
    }

    /// One field: its label over its Composer, ringed while it has the
    /// keyboard. A press on it gives it the keyboard.
    pub(super) fn input(
        &self,
        field: &Field,
        label: &'static str,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let composer = self.fields.get(field)?.clone();
        let focused = composer.read(cx).focus_handle(cx).is_focused(window);
        let chosen = field.clone();
        Some(field_input(
            field,
            label,
            composer,
            focused,
            cx.listener(move |editor, _: &MouseDownEvent, _, cx| {
                editor.focused = Some(chosen.clone());
                cx.emit(WorkflowsEvent::Focused);
            }),
        ))
    }
}

impl Render for WorkflowsView {
    /// The Workflows surface, as the root's state stands now.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let inputs = self
            .cockpit
            .upgrade()
            .and_then(|cockpit| cockpit.read(cx).workflows_inputs(self));
        self.surface(inputs, window, cx)
    }
}

impl CockpitView {
    /// A Factory's Workflows editor, heard from its making.
    pub(in crate::cockpit) fn new_workflows(&mut self, cx: &mut Context<Self>) -> Entity<WorkflowsView> {
        let me = cx.entity().downgrade();
        let editor = cx.new(|cx| WorkflowsView::new(me, cx));
        cx.subscribe(&editor, Self::on_workflows_event).detach();
        editor
    }

    /// What the editor draws, for what `editor` chose: none while the
    /// Project has no Workflow.
    pub(in crate::cockpit) fn workflows_inputs(&self, editor: &WorkflowsView) -> Option<WorkflowsInputs> {
        let workflow = self.chosen_workflow(editor.selected.as_deref())?;
        let view = self.factory()?;
        let step = editor.shown_step(&workflow.draft);
        let provider = step.map(|step| step.agent.provider);
        let catalog = self.factory_catalog();
        Some(WorkflowsInputs {
            workflow: workflow.id.clone(),
            title: workflow_title(workflow),
            saved: workflow.latest().map(|latest| latest.version),
            unsaved: workflow.unsaved(),
            step: step.map(|step| step.id.clone()),
            switch: workflow.automation.switch,
            problems: workflow.draft.problems(&catalog),
            catalog,
            draft: workflow.draft.clone(),
            models: Provider::ALL
                .into_iter()
                .map(|provider| (provider, self.cockpit.model_catalog(provider)))
                .collect(),
            installed: provider
                .and_then(|provider| view.skills.installed(provider))
                .map(<[_]>::to_vec),
            skills_note: provider
                .map(|provider| view.skills.note(provider))
                .unwrap_or_default(),
            starts: view.starts.clone(),
        })
    }

    /// A field typed in goes to the draft; one pressed has the focus pump
    /// hand it the keyboard; a row, a problem or a key chooses what the
    /// inspector shows; a picker opens its rows; every edit and Save ask
    /// the Server; Run task opens the New task sheet.
    fn on_workflows_event(
        &mut self,
        _: Entity<WorkflowsView>,
        event: &WorkflowsEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            WorkflowsEvent::Edited(field) => self.field_edited(field, cx),
            WorkflowsEvent::Focused => cx.notify(),
            WorkflowsEvent::Select(step) => self.select_step(step.clone(), cx),
            WorkflowsEvent::Approval => self.select_approval(cx),
            WorkflowsEvent::Start => self.open_start(cx),
            WorkflowsEvent::Move { step, to } => self.move_step(step.clone(), *to, cx),
            WorkflowsEvent::Drop { drag, onto } => self.drop_step(drag, *onto, cx),
            WorkflowsEvent::Route {
                step,
                kind,
                trigger,
                at,
            } => {
                let Some(step) = self.shown_step_of(step, cx) else {
                    return;
                };
                let picks = self.route_picks(&step, *kind, cx);
                self.open_picks(picks, trigger, *at, cx);
            }
            WorkflowsEvent::RowMenu { step, trigger, at } => {
                let Some(step) = self.shown_step_of(step, cx) else {
                    return;
                };
                let picks = self.row_picks(&step, cx);
                self.open_picks(picks, trigger, *at, cx);
            }
            WorkflowsEvent::Add => self.add_step(None, cx),
            WorkflowsEvent::Picks { picks, trigger, at } => {
                self.open_picks(picks.clone(), trigger, *at, cx)
            }
            WorkflowsEvent::Pick(pick) => self.apply_pick(pick.clone(), cx),
            WorkflowsEvent::EditStep { step, change } => {
                self.edit_step(step.clone(), change.clone(), cx)
            }
            WorkflowsEvent::AddSkill(name) => self.add_skill(name.clone(), cx),
            WorkflowsEvent::RemoveSkill(name) => self.remove_skill(name, cx),
            WorkflowsEvent::Delete(step) => self.delete_step(step.clone(), cx),
            WorkflowsEvent::Workflows { trigger, at } => {
                let picks = self.workflow_picks(cx);
                self.open_picks(picks, trigger, *at, cx)
            }
            WorkflowsEvent::Run => {
                let workflow = self.shown_id(cx);
                self.open_new_task(cx);
                self.set_new_task_workflow(workflow, cx);
            }
            WorkflowsEvent::Save => self.save_workflow(cx),
        }
    }
}
