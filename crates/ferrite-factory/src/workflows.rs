//! A Project's Workflows in the Factory (ADR 0037, #221): an ordered
//! outline of Steps beside an inspector, the pickers that set a Step's
//! routes, Provider, model, Effort and skills, the draft's problems, and
//! Save, which makes the next version and starts nothing.
//!
//! The editor's own state is a view of its own (ADR 0041), held by the
//! Factory: the Workflow and Step chosen, what the inspector shows,
//! whether the skills search is revealed, the outline's focus and scroll,
//! each text field's Composer, made once for as long as the Factory is
//! open, so what was typed survives a picker, a revealed field, a notice,
//! a scroll and a switch of surface (the draft keeps it across a save and
//! a reopen), and the field that holds the keyboard.
//!
//! Its render is the Workflows surface: the head, the draft's problems,
//! the outline (`outline`) and the inspector (`inspector`), or the empty
//! state, reading the window through a `Source` the window makes over
//! itself, asked for what the editor chose (`WorkflowsQuery`). Which
//! fields the surface shows the window decides from the draft and makes
//! here (`ensure`); what is typed or pressed the editor says as a
//! `WorkflowsEvent`, since every edit is a Server ask.

use std::collections::HashMap;

use ferrite_core::factory_board::{
    Catalog, Definition, Problem, RouteKind, Step, StepChange, StepId, Switch, WorkflowId,
};
use ferrite_core::provider::Provider;
use ferrite_pane::composer::{Composer, Edited, Role};
use ferrite_ui::source::Source;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable,
    MouseDownEvent, Pixels, Point, SharedString, Window,
};

use crate::field::{field_input, Field};
use crate::picker::{Pick, Trigger};
use crate::starts::StartsView;

mod inspector;
mod outline;
pub mod pickers;

/// The outline's width beside the inspector.
const OUTLINE_W: f32 = 440.;
/// How many skills the search offers at once.
const SKILL_ROWS: usize = 8;

/// A Step's row, dragged to a new place in the outline.
#[derive(Clone)]
pub struct StepDrag {
    pub step: StepId,
    name: SharedString,
}

impl StepDrag {
    /// A drag of `step`, its preview saying `name`, as a press on its row
    /// starts one.
    pub fn new(step: StepId, name: SharedString) -> Self {
        Self { step, name }
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
    let (mark, ink) = ferrite_ui::provider::mark(provider);
    ferrite_ui::icons::icon(mark, BAR_MARK, ink).into_any_element()
}

/// A Step's skill, matched by name or description.
fn skill_matches(skill: &ferrite_core::SessionCommand, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    filter.is_empty()
        || skill.name.to_lowercase().contains(&filter)
        || skill.description.to_lowercase().contains(&filter)
        || ferrite_ui::fuzzy::matches(&filter, &skill.name).is_some()
}


/// What the editor asks of the window.
pub enum WorkflowsEvent {
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

/// What the editor asks the window for: the Workflow and the Step it
/// chose, none for the Project's first and the Workflow's first.
pub type WorkflowsQuery = (Option<WorkflowId>, Option<StepId>);

/// The Step `draft` shows: `chosen`, else the first.
pub fn shown_step<'a>(draft: &'a Definition, chosen: Option<&str>) -> Option<&'a Step> {
    chosen
        .and_then(|id| draft.step(id))
        .or_else(|| draft.steps.first())
}

/// What the editor draws from the window: the Workflow shown, the Step
/// shown, the draft's problems, what is known installed and the starts'
/// view.
pub struct WorkflowsInputs {
    pub workflow: WorkflowId,
    /// Its name as its picker says it.
    pub title: String,
    /// Its latest saved version's number, if one was saved.
    pub saved: Option<u32>,
    /// The draft differs from that version, or none was saved.
    pub unsaved: bool,
    pub draft: Definition,
    /// The Step shown: the one chosen, else the first.
    pub step: Option<StepId>,
    /// Its automatic starts' switch, which the start's row says.
    pub switch: Switch,
    pub problems: Vec<Problem>,
    /// What the problems were found against: models as the Providers
    /// announced them, skills as discovered.
    pub catalog: Catalog,
    /// Each Provider's models, as a row names them and a picker offers them.
    pub models: Vec<(Provider, Vec<ferrite_core::ModelInfo>)>,
    /// The skills the shown Step's Provider has installed, once listed.
    pub installed: Option<Vec<ferrite_core::SessionCommand>>,
    /// What the inspector says of them.
    pub skills_note: String,
    /// The start's settings, drawn by their own view.
    pub starts: Entity<StartsView>,
}

impl WorkflowsInputs {
    /// `provider`'s models.
    pub fn models(&self, provider: Provider) -> &[ferrite_core::ModelInfo] {
        self.models
            .iter()
            .find(|(each, _)| *each == provider)
            .map_or(&[], |(_, models)| models)
    }
}

/// What the inspector shows beside the outline, whose row for it is the
/// one chosen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Inspected {
    /// The chosen Step's settings.
    #[default]
    Step,
    /// The approval settings (#222).
    Approval,
    /// How Tasks start (#225).
    Start,
}

/// A Factory's Workflows editor.
pub struct WorkflowsView {
    /// What the editor draws, as the window has it now.
    inputs: Source<WorkflowsInputs, WorkflowsQuery>,
    /// The Workflow chosen; none, the Project's first.
    selected: Option<WorkflowId>,
    /// The Step chosen; none, the Workflow's first.
    step: Option<StepId>,
    inspected: Inspected,
    /// The skills search is revealed.
    skills_open: bool,
    /// What holds the keyboard while no field does.
    outline_focus: FocusHandle,
    /// The outline's scroll: its first child is its head, then one per
    /// Step's row (`outline`).
    outline_scroll: gpui::ScrollHandle,
    /// Each text field's Composer, made once.
    fields: HashMap<Field, Entity<Composer>>,
    /// The field that holds the keyboard; none, the outline.
    focused: Option<Field>,
}

impl EventEmitter<WorkflowsEvent> for WorkflowsView {}

impl WorkflowsView {
    pub fn new(inputs: Source<WorkflowsInputs, WorkflowsQuery>, cx: &mut Context<Self>) -> Self {
        Self {
            inputs,
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
    pub fn select_workflow(&mut self, workflow: WorkflowId) {
        self.selected = Some(workflow);
        self.step = None;
        self.inspect(Inspected::Step);
    }

    /// Show `step` in the inspector.
    pub fn select_step(&mut self, step: StepId) {
        self.step = Some(step);
        self.inspect(Inspected::Step);
    }

    /// Show `inspected` in the inspector; no field keeps the keyboard.
    pub fn inspect(&mut self, inspected: Inspected) {
        self.inspected = inspected;
        self.focused = None;
    }

    /// The Step `draft` shows: the one chosen, else the first.
    pub fn shown_step<'a>(&self, draft: &'a Definition) -> Option<&'a Step> {
        shown_step(draft, self.step.as_deref())
    }

    /// The Workflow chosen; none, the Project's first.
    pub fn selected(&self) -> Option<&WorkflowId> {
        self.selected.as_ref()
    }

    /// Choose `step`, or none for the Workflow's first.
    pub fn set_step(&mut self, step: Option<StepId>) {
        self.step = step;
    }

    pub fn inspected(&self) -> Inspected {
        self.inspected
    }

    /// The field that holds the keyboard; none, the outline.
    pub fn focused(&self) -> Option<&Field> {
        self.focused.as_ref()
    }

    /// Give `field` the keyboard, or none for the outline.
    pub fn set_focused(&mut self, field: Option<Field>) {
        self.focused = field;
    }

    /// What holds the keyboard while no field does.
    pub fn outline_focus(&self) -> &FocusHandle {
        &self.outline_focus
    }

    /// What the editor draws, for what it chose: none while the Project
    /// has no Workflow.
    fn inputs(&self, cx: &App) -> Option<WorkflowsInputs> {
        self.inputs.read(&(self.selected.clone(), self.step.clone()), cx)
    }

    /// The outline's keys: ↑ ↓ choose a Step, ⌥↑ ⌥↓ move it, read against
    /// the draft as it is now.
    fn outline_key(&mut self, keystroke: &gpui::Keystroke, cx: &mut Context<Self>) {
        let Some(inputs) = self.inputs(cx) else {
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
    pub fn ensure(&mut self, field: Field, text: &str, cx: &mut Context<Self>) {
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

    pub fn field(&self, field: &Field) -> Option<&Entity<Composer>> {
        self.fields.get(field)
    }

    /// What holds the keyboard: the chosen field, if it is made.
    pub fn focus(&self, cx: &App) -> Option<FocusHandle> {
        let composer = self.fields.get(self.focused.as_ref()?)?;
        Some(composer.read(cx).focus_handle(cx))
    }

    /// One field: its label over its Composer, ringed while it has the
    /// keyboard. A press on it gives it the keyboard.
    fn input(
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
        let inputs = self.inputs(cx);
        self.surface(inputs, window, cx)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use ferrite_core::factory_board::{Approver, Start};
    use ferrite_core::{ModelInfo, SessionCommand};
    use ferrite_ui::testing::{click, mount, record, shows};
    use gpui::{Modifiers, TestAppContext, VisualTestContext};

    use super::*;
    use crate::starts::StartsInputs;

    /// The window's part in it: the Workflows as the Server published
    /// them, what is known installed, and every query the editor asked.
    struct Held {
        /// Each Workflow by id, the first shown until one is chosen; none
        /// while the Project has no Workflow.
        drafts: Vec<(WorkflowId, Definition)>,
        saved: Option<u32>,
        unsaved: bool,
        problems: Vec<Problem>,
        catalog: Catalog,
        models: Vec<(Provider, Vec<ModelInfo>)>,
        installed: Option<Vec<SessionCommand>>,
        starts: Entity<StartsView>,
        /// What the start's view reads.
        _starting: Entity<Starting>,
        asked: RefCell<Vec<WorkflowsQuery>>,
    }

    impl Held {
        fn inputs(&self, (workflow, step): &WorkflowsQuery) -> Option<WorkflowsInputs> {
            self.asked.borrow_mut().push((workflow.clone(), step.clone()));
            let (id, draft) = match workflow {
                Some(workflow) => self.drafts.iter().find(|(id, _)| id == workflow)?,
                None => self.drafts.first()?,
            }
            .clone();
            Some(WorkflowsInputs {
                workflow: id,
                title: draft.name.clone(),
                saved: self.saved,
                unsaved: self.unsaved,
                step: shown_step(&draft, step.as_deref()).map(|step| step.id.clone()),
                switch: Switch::Off,
                problems: self.problems.clone(),
                catalog: self.catalog.clone(),
                models: self.models.clone(),
                installed: self.installed.clone(),
                skills_note: String::new(),
                starts: self.starts.clone(),
                draft,
            })
        }
    }

    /// The start's own view, on a manual start.
    struct Starting;

    fn step(id: &str, name: &str, extra: serde_json::Value) -> serde_json::Value {
        let mut step = serde_json::json!({
            "id": id, "name": name, "instructions": format!("{name} it"),
            "provider": "claude", "on_finished": "next", "on_unable": "human",
        });
        for (key, value) in extra.as_object().unwrap() {
            step[key] = value.clone();
        }
        step
    }

    /// Three Steps: Build; Review on Codex's GPT-5 at high, with two
    /// skills, fresh each time; Check, whose Needs-a-human route goes to a
    /// deleted Step.
    fn draft() -> Definition {
        serde_json::from_value(serde_json::json!({
            "name": "Ship it",
            "steps": [
                step("s-1", "Build", serde_json::json!({"skills": ["code-review", "retired"]})),
                step("s-2", "Review", serde_json::json!({
                    "provider": "codex", "model": "gpt-5", "effort": "high",
                    "skills": ["a", "b"], "fresh": true,
                })),
                step("s-3", "Check", serde_json::json!({"on_unable": "step:s-9"})),
            ],
        }))
        .unwrap()
    }

    /// Another Workflow, Docs pass: Draft on Codex, then Proofread.
    fn docs() -> Definition {
        serde_json::from_value(serde_json::json!({
            "name": "Docs pass",
            "steps": [
                step("d-1", "Draft", serde_json::json!({"provider": "codex"})),
                step("d-2", "Proofread", serde_json::json!({})),
            ],
        }))
        .unwrap()
    }

    fn model(value: &str, display: &str, efforts: &[&str]) -> ModelInfo {
        ModelInfo {
            value: value.into(),
            display: display.into(),
            detail: String::new(),
            resolved: None,
            efforts: efforts.iter().map(|effort| effort.to_string()).collect(),
            default_effort: None,
        }
    }

    fn skill(name: &str, description: &str) -> SessionCommand {
        SessionCommand {
            name: name.into(),
            description: description.into(),
            path: None,
            source: Some(ferrite_core::CommandSource::Skill),
        }
    }

    /// The editor over `draft`, unsaved, as w-1, with Docs pass beside it
    /// as w-2; tall and wide enough to show every row and setting.
    fn editor(
        draft: Option<Definition>,
        cx: &mut TestAppContext,
    ) -> (Entity<WorkflowsView>, Entity<Held>, &mut VisualTestContext) {
        let starting = cx.new(|_| Starting);
        let held = cx.new(|cx| {
            let start = Source::of(&starting, |_: &Starting, _: &(), _| {
                Some(StartsInputs {
                    workflow: "w-1".into(),
                    start: Start::default(),
                    saved: None,
                    switch: Switch::Off,
                    due_ms: None,
                    waiting: false,
                    error: None,
                    backlog: 0,
                    schedule_instructions: None,
                    extra_instructions: None,
                })
            });
            let claude = vec![model("claude-opus-4", "Opus 4", &["high", "max"])];
            let codex = vec![model("gpt-5", "GPT-5", &["low", "high"])];
            Held {
                drafts: draft
                    .map(|draft| vec![("w-1".into(), draft), ("w-2".into(), docs())])
                    .unwrap_or_default(),
                saved: None,
                unsaved: true,
                problems: Vec::new(),
                catalog: Catalog {
                    models: vec![(Provider::Claude, claude.clone())],
                    skills: Vec::new(),
                },
                models: vec![(Provider::Claude, claude), (Provider::Codex, codex)],
                installed: Some(vec![
                    skill("triage", "Sort incoming issues"),
                    skill("code-review", "Review a diff"),
                    skill("pr", "Open a pull request"),
                ]),
                starts: cx.new(|_| StartsView::new(start)),
                _starting: starting,
                asked: RefCell::new(Vec::new()),
            }
        });
        let inputs = Source::of(&held, |held: &Held, query: &WorkflowsQuery, _| held.inputs(query));
        let (view, cx) = mount(cx, |_, cx| WorkflowsView::new(inputs, cx));
        cx.simulate_resize(gpui::size(px(1400.), px(2400.)));
        cx.run_until_parked();
        (view, held, cx)
    }

    /// The window changes what the editor shows.
    fn change(held: &Entity<Held>, cx: &mut VisualTestContext, change: impl FnOnce(&mut Held)) {
        held.update(cx, |held, cx| {
            change(held);
            cx.notify();
        });
        cx.run_until_parked();
    }

    /// The window answers: a method, then the editor redraws.
    fn answer(view: &Entity<WorkflowsView>, cx: &mut VisualTestContext, answer: impl FnOnce(&mut WorkflowsView, &mut Context<WorkflowsView>)) {
        view.update(cx, |view, cx| {
            answer(view, cx);
            cx.notify();
        });
        cx.run_until_parked();
    }

    fn said(event: &WorkflowsEvent) -> String {
        match event {
            WorkflowsEvent::Edited(field) => format!("edited {}", field.key()),
            WorkflowsEvent::Focused => "focused".into(),
            WorkflowsEvent::Select(step) => format!("select {step}"),
            WorkflowsEvent::Approval => "approval".into(),
            WorkflowsEvent::Start => "start".into(),
            WorkflowsEvent::Move { step, to } => format!("move {step} to {to}"),
            WorkflowsEvent::Drop { drag, onto } => format!("drop {} onto {onto}", drag.step),
            WorkflowsEvent::Route { step, kind, .. } => format!("route {step} {}", kind.word()),
            WorkflowsEvent::RowMenu { step, .. } => format!("row menu {step}"),
            WorkflowsEvent::Add => "add".into(),
            WorkflowsEvent::Picks { picks, .. } => {
                let rows: Vec<String> = picks
                    .iter()
                    .map(|(item, pick)| {
                        let mark = if item.checked { "*" } else { "" };
                        format!("{mark}{} {pick:?}", item.label)
                    })
                    .collect();
                format!("picks {}", rows.join(", "))
            }
            WorkflowsEvent::Pick(pick) => format!("pick {pick:?}"),
            WorkflowsEvent::EditStep { step, change } => format!(
                "edit {step} fresh {:?} merge {:?}",
                change.fresh, change.by_merge_result
            ),
            WorkflowsEvent::AddSkill(name) => format!("add skill {name}"),
            WorkflowsEvent::RemoveSkill(name) => format!("remove skill {name}"),
            WorkflowsEvent::Delete(step) => format!("delete {step}"),
            WorkflowsEvent::Workflows { .. } => "workflows".into(),
            WorkflowsEvent::Run => "run".into(),
            WorkflowsEvent::Save => "save".into(),
        }
    }

    fn text(cx: &mut VisualTestContext, selector: &str) -> String {
        cx.update(|window, _| window.debug_text(selector)).unwrap_or_default().join(" ")
    }

    fn bounds(cx: &mut VisualTestContext, selector: &str) -> gpui::Bounds<Pixels> {
        cx.update(|window, _| window.debug_bounds(selector))
            .unwrap_or_else(|| panic!("nothing drawn as {selector}"))
    }

    /// A row's number, clear of the route choosers in its middle.
    fn number(cx: &mut VisualTestContext, row: &str) -> Point<Pixels> {
        bounds(cx, row).origin + gpui::point(px(12.), px(12.))
    }

    /// With no Workflow, the empty state offers the templates.
    #[gpui::test]
    fn with_no_workflow_it_offers_the_templates(cx: &mut TestAppContext) {
        let (view, _held, cx) = editor(None, cx);
        let heard = record(cx, &view, said);
        assert!(shows(cx, "workflows-empty"));
        assert!(!shows(cx, "outline"));
        click(cx, "workflow-new");
        assert_eq!(
            heard.take(),
            ["picks Blank Create(Blank), Issue to reviewed PR Create(IssueToReviewedPr)"]
        );
    }

    /// The head names the Workflow and its version; Run task waits for a
    /// saved version with no problems and Save for an edit with none; a
    /// listed problem leads to its Step.
    #[gpui::test]
    fn the_head_says_what_can_be_saved_and_run(cx: &mut TestAppContext) {
        let (view, held, cx) = editor(Some(draft()), cx);
        let heard = record(cx, &view, said);
        assert_eq!(text(cx, "workflow-picker"), "Ship it");
        assert_eq!(text(cx, "workflow-version"), "not saved");
        assert!(text(cx, "workflow-save").contains("Save"));
        click(cx, "workflow-run");
        click(cx, "workflow-save");
        click(cx, "workflow-picker");
        assert_eq!(heard.take(), ["save", "workflows"], "nothing saved to run");

        change(&held, cx, |held| {
            held.saved = Some(2);
            held.unsaved = false;
        });
        assert_eq!(text(cx, "workflow-version"), "version 2");
        assert!(text(cx, "workflow-save").contains("Saved"));
        click(cx, "workflow-save");
        click(cx, "workflow-run");
        assert_eq!(heard.take(), ["run"], "nothing to save");

        change(&held, cx, |held| {
            held.unsaved = true;
            held.problems = vec![Problem {
                step: Some("s-3".into()),
                message: "Check: its route goes to a deleted agent.".into(),
                approval: false,
            }];
        });
        assert_eq!(text(cx, "workflow-version"), "version 2 \u{b7} edited");
        assert!(text(cx, "workflow-problems").starts_with("1 problem: it cannot be saved or run"));
        click(cx, "workflow-save");
        click(cx, "workflow-run");
        click(cx, "workflow-problem-0");
        assert_eq!(heard.take(), ["select s-3"], "a problem holds both back");
    }

    /// The outline: the start's row, each Step's row in order with what
    /// runs it and its routes, a route to a deleted Step flagged, and the
    /// approval's row; each press is said.
    #[gpui::test]
    fn the_outline_draws_each_step_and_says_each_press(cx: &mut TestAppContext) {
        let (view, _held, cx) = editor(Some(draft()), cx);
        let heard = record(cx, &view, said);
        assert!(shows(cx, "outline-row-s-1-selected"), "the first, chosen");
        let rows = ["outline-row-s-1-selected", "outline-row-s-2", "outline-row-s-3"];
        let tops: Vec<_> = rows.iter().map(|row| bounds(cx, row).origin.y).collect();
        assert!(tops.windows(2).all(|pair| pair[0] < pair[1]), "in order");
        assert!(text(cx, "outline-row-s-2").contains("GPT-5 \u{b7} high \u{b7} 2 skills \u{b7} starts fresh"));
        assert!(text(cx, "outline-row-s-3").contains("Claude"));
        assert!(text(cx, "outline-route-s-1-finished").contains("Next agent"));
        assert!(shows(cx, "outline-route-s-1-unable"));
        assert!(shows(cx, "outline-route-s-3-unable-flagged"), "to a deleted Step");
        assert!(text(cx, "outline-start").contains("Manually"));
        assert!(text(cx, "outline-approval").contains("You approve"));

        let row = number(cx, "outline-row-s-2");
        cx.simulate_click(row, Modifiers::none());
        cx.run_until_parked();
        click(cx, "outline-route-s-1-finished");
        click(cx, "outline-row-s-3-more");
        click(cx, "outline-add");
        click(cx, "outline-approval");
        click(cx, "outline-start");
        assert_eq!(
            heard.take(),
            [
                "select s-2",
                "focused",
                "route s-1 finished",
                "row menu s-3",
                "focused",
                "add",
                "focused",
                "approval",
                "start"
            ],
            "a press on the outline that a control does not stop takes the keyboard back"
        );
    }

    /// ↑ ↓ choose the Step before or after the one shown, ⌥↑ ⌥↓ move it,
    /// and nothing goes past either end.
    #[gpui::test]
    fn the_outlines_keys_choose_and_move_steps(cx: &mut TestAppContext) {
        let (view, _held, cx) = editor(Some(draft()), cx);
        let heard = record(cx, &view, said);
        cx.update(|window, cx| {
            let focus = view.read(cx).outline_focus().clone();
            window.focus(&focus, cx);
        });
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("up");
        cx.simulate_keystrokes("alt-up");
        answer(&view, cx, |view, _| view.select_step("s-2".into()));
        cx.simulate_keystrokes("alt-up");
        cx.simulate_keystrokes("alt-down");
        answer(&view, cx, |view, _| view.select_step("s-3".into()));
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("a");
        cx.run_until_parked();
        assert_eq!(heard.take(), ["select s-2", "move s-2 to 0", "move s-2 to 2"]);
    }

    /// A row dragged onto another says where it was let go.
    #[gpui::test]
    fn a_row_let_go_on_another_says_where(cx: &mut TestAppContext) {
        let (view, _held, cx) = editor(Some(draft()), cx);
        let heard = record(cx, &view, said);
        let from = number(cx, "outline-row-s-3");
        let onto = number(cx, "outline-row-s-1-selected");
        cx.simulate_mouse_down(from, gpui::MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(onto, Some(gpui::MouseButton::Left), Modifiers::none());
        cx.simulate_mouse_move(onto, Some(gpui::MouseButton::Left), Modifiers::none());
        cx.simulate_mouse_up(onto, gpui::MouseButton::Left, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(heard.take(), ["select s-3", "focused", "drop s-3 onto 0"]);
    }

    /// The inspector shows the Step chosen: its fields, its Provider,
    /// model and Effort pickers with their rows, an unavailable model
    /// flagged, its switches and Delete; each press is said.
    #[gpui::test]
    fn the_inspector_shows_the_step_chosen(cx: &mut TestAppContext) {
        let mut built = draft();
        built.steps[0].agent.model = Some("claude-gone".into());
        let (view, held, cx) = editor(Some(built), cx);
        let heard = record(cx, &view, said);
        assert!(!shows(cx, "inspector-name"), "no field until the window makes it");
        answer(&view, cx, |view, cx| {
            view.ensure(Field::StepName("w-1".into(), "s-1".into()), "Build", cx);
            view.ensure(Field::Instructions("w-1".into(), "s-1".into()), "Build it", cx);
        });
        assert!(shows(cx, "inspector-name") && shows(cx, "inspector-instructions"));
        assert_eq!(text(cx, "inspector-provider"), "Claude");
        assert_eq!(text(cx, "inspector-model-flagged"), "claude-gone \u{b7} unavailable");
        click(cx, "inspector-provider");
        click(cx, "inspector-model-flagged");
        click(cx, "inspector-effort");
        click(cx, "inspector-fresh");
        click(cx, "inspector-merge");
        click(cx, "inspector-delete");
        assert_eq!(
            heard.take(),
            [
                "picks *Claude Provider(\"s-1\", Claude), Codex Provider(\"s-1\", Codex)",
                "picks Default Model(\"s-1\", \"\"), Opus 4 Model(\"s-1\", \"claude-opus-4\")",
                "picks *Default Effort(\"s-1\", \"\"), high Effort(\"s-1\", \"high\"), \
                 max Effort(\"s-1\", \"max\")",
                "edit s-1 fresh Some(true) merge None",
                "edit s-1 fresh None merge Some(true)",
                "delete s-1"
            ]
        );

        answer(&view, cx, |view, _| view.select_step("s-2".into()));
        assert_eq!(
            held.read_with(cx, |held, _| held.asked.borrow().last().cloned()),
            Some((None, Some("s-2".into()))),
            "it asks for the Step it chose"
        );
        assert_eq!(text(cx, "inspector-provider"), "Codex");
        assert!(shows(cx, "inspector-fresh-on"));
        click(cx, "inspector-effort");
        assert_eq!(
            heard.take(),
            ["picks Default Effort(\"s-2\", \"\"), low Effort(\"s-2\", \"low\"), \
              *high Effort(\"s-2\", \"high\")"]
        );
        change(&held, cx, |held| held.drafts[0].1.steps.clear());
        assert!(shows(cx, "inspector-none"));
    }

    /// A Step's skills: each chosen one a chip, one not installed flagged;
    /// the search lists what is installed and not chosen, by name or
    /// description, and says a pick.
    #[gpui::test]
    fn the_skills_search_lists_what_is_installed(cx: &mut TestAppContext) {
        let (view, _held, cx) = editor(Some(draft()), cx);
        let heard = record(cx, &view, said);
        answer(&view, cx, |view, cx| view.ensure(Field::SkillSearch, "", cx));
        assert!(shows(cx, "skill-chip-code-review"));
        assert_eq!(text(cx, "skill-chip-retired-flagged"), "retired \u{b7} not installed");
        let chip = bounds(cx, "skill-chip-code-review");
        let close = gpui::point(chip.right() - px(8.), chip.center().y);
        cx.simulate_click(close, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(heard.take(), ["remove skill code-review"]);

        assert!(!shows(cx, "skill-search"));
        click(cx, "skill-add");
        assert_eq!(heard.take(), ["focused"]);
        assert_eq!(view.read_with(cx, |view, _| view.focused().cloned()), Some(Field::SkillSearch));
        assert!(shows(cx, "skill-search"));
        assert!(shows(cx, "skill-option-triage") && shows(cx, "skill-option-pr"));
        assert!(!shows(cx, "skill-option-code-review"), "already chosen");
        cx.update(|window, cx| {
            let focus = view.read(cx).focus(cx).unwrap();
            window.focus(&focus, cx);
        });
        cx.simulate_input("sort");
        cx.run_until_parked();
        assert!(shows(cx, "skill-option-triage"), "found by its description");
        assert!(!shows(cx, "skill-option-pr"));
        heard.take();
        click(cx, "skill-option-triage");
        assert_eq!(heard.take(), ["add skill triage"]);
        click(cx, "skill-add");
        assert!(!shows(cx, "skill-search"), "Done closes it");
        assert_eq!(view.read_with(cx, |view, _| view.focused().cloned()), None);
    }

    /// The approval settings: who approves, and with an agent the one
    /// that decides; what follows approval and where rework goes, each
    /// picker's rows said; a problem listed; what ends in Completed.
    #[gpui::test]
    fn the_approval_settings_say_each_choice(cx: &mut TestAppContext) {
        let (view, held, cx) = editor(Some(draft()), cx);
        let heard = record(cx, &view, said);
        answer(&view, cx, |view, _| view.inspect(Inspected::Approval));
        assert!(shows(cx, "approval-settings") && !shows(cx, "inspector-name"));
        assert!(shows(cx, "approval-by-0-on"));
        assert!(!shows(cx, "approval-decider"));
        click(cx, "approval-by-1");
        click(cx, "approval-after");
        click(cx, "approval-rework");
        assert_eq!(
            heard.take(),
            [
                "pick Approver(Agent)",
                "picks *Completed AfterApproval(Complete), Run Build AfterApproval(Step(\"s-1\")), \
                 Run Review AfterApproval(Step(\"s-2\")), Run Check AfterApproval(Step(\"s-3\"))",
                "picks Build ReworkGoesTo(Step(\"s-1\")), Review ReworkGoesTo(Step(\"s-2\")), \
                 Check ReworkGoesTo(Step(\"s-3\")), *Coordinator ReworkGoesTo(Coordinator)"
            ]
        );
        assert!(shows(cx, "completion-paths"));

        change(&held, cx, |held| {
            held.drafts[0].1.approval.by = Approver::Agent;
        });
        answer(&view, cx, |view, cx| {
            view.ensure(Field::ApprovalInstructions("w-1".into()), "", cx)
        });
        assert!(shows(cx, "approval-by-1-on") && shows(cx, "approval-instructions"));
        assert!(shows(cx, "approval-decider-flagged"), "no agent chosen");
        assert!(shows(cx, "approval-problem-0"));
        click(cx, "approval-decider-flagged");
        assert_eq!(
            heard.take(),
            ["picks Build Decider(\"s-1\"), Review Decider(\"s-2\"), Check Decider(\"s-3\")"]
        );
    }

    /// V12: a Step's instructions are a text area, taller than a one-line
    /// field, that grows with what is typed.
    #[gpui::test]
    fn step_instructions_are_a_text_area_that_grows(cx: &mut TestAppContext) {
        let (view, _held, cx) = editor(Some(draft()), cx);
        answer(&view, cx, |view, cx| {
            view.ensure(Field::StepName("w-1".into(), "s-1".into()), "Build", cx);
            view.ensure(Field::Instructions("w-1".into(), "s-1".into()), "Build it", cx);
        });
        let name = bounds(cx, "inspector-name");
        let empty = bounds(cx, "inspector-instructions");
        assert!(
            empty.size.height > name.size.height + px(ferrite_ui::theme::LH_UI),
            "an area, not a line: {empty:?} vs {name:?}"
        );
        click(cx, "inspector-instructions");
        let focus = view.read_with(cx, |view, cx| view.focus(cx)).expect("the instructions hold it");
        cx.update(|window, cx| window.focus(&focus, cx));
        for _ in 0..6 {
            cx.simulate_input(
                "Use the triage skill on the incoming issue and ask the owner about anything missing. ",
            );
        }
        cx.run_until_parked();
        let grown = bounds(cx, "inspector-instructions");
        assert!(
            grown.size.height > empty.size.height,
            "it grows: {grown:?} from {empty:?}"
        );
    }

    /// The inspector shows one thing, and only its outline row is chosen:
    /// a Step, the approval or the start, which its own view draws;
    /// choosing a Workflow lands on its first Step, asked for by id.
    #[gpui::test]
    fn the_inspector_shows_one_thing_and_its_row_alone_is_chosen(cx: &mut TestAppContext) {
        let (view, held, cx) = editor(Some(draft()), cx);
        let chosen = |cx: &mut VisualTestContext| {
            [
                ("outline-row-s-1-selected", "step"),
                ("outline-row-s-2-selected", "second"),
                ("outline-row-d-1-selected", "Draft"),
                ("outline-row-d-2-selected", "Proofread"),
                ("outline-approval-selected", "approval"),
                ("outline-start-selected", "start"),
                ("approval-settings", "approval settings"),
                ("inspector-start", "start settings"),
            ]
            .into_iter()
            .filter(|(selector, _)| shows(cx, selector))
            .map(|(_, shown)| shown)
            .collect::<Vec<_>>()
        };
        assert_eq!(chosen(cx), ["step"]);
        answer(&view, cx, |view, _| view.inspect(Inspected::Approval));
        assert_eq!(chosen(cx), ["approval", "approval settings"]);
        answer(&view, cx, |view, _| view.inspect(Inspected::Start));
        assert_eq!(chosen(cx), ["start", "start settings"]);
        assert_eq!(view.read_with(cx, |view, _| view.inspected()), Inspected::Start);
        answer(&view, cx, |view, _| view.select_step("s-2".into()));
        assert_eq!(chosen(cx), ["second"]);
        answer(&view, cx, |view, _| view.inspect(Inspected::Approval));
        assert_eq!(text(cx, "workflow-picker"), "Ship it");
        answer(&view, cx, |view, _| view.select_workflow("w-2".into()));
        assert_eq!(chosen(cx), ["Draft"], "a Workflow chosen shows its own first Step");
        assert_eq!(text(cx, "workflow-picker"), "Docs pass");
        assert_eq!(text(cx, "inspector-provider"), "Codex", "Draft's, not Build's");
        assert_eq!(view.read_with(cx, |view, _| view.selected().cloned()), Some("w-2".into()));
        assert_eq!(
            held.read_with(cx, |held, _| held.asked.borrow().last().cloned()),
            Some((Some("w-2".into()), None)),
            "it asks for the Workflow it chose"
        );
        answer(&view, cx, |view, _| view.set_step(Some("d-2".into())));
        assert_eq!(chosen(cx), ["Proofread"]);
        assert_eq!(text(cx, "inspector-provider"), "Claude");
    }

    /// Each field is made once and keeps what was typed; what is typed is
    /// said; a press on a field gives it the keyboard, a press on the
    /// outline takes it back, and showing something else drops it.
    #[gpui::test]
    fn each_field_is_made_once_and_says_each_edit(cx: &mut TestAppContext) {
        let (view, _held, cx) = editor(Some(draft()), cx);
        let name = Field::WorkflowName("w-1".into());
        let first = view.update(cx, |view, cx| {
            view.ensure(name.clone(), "Ship it", cx);
            view.field(&name).cloned().unwrap()
        });
        answer(&view, cx, |view, cx| view.ensure(name.clone(), "Something else", cx));
        assert_eq!(view.read_with(cx, |view, _| view.field(&name).cloned()), Some(first.clone()));
        assert_eq!(first.read_with(cx, |field, _| field.text().to_string()), "Ship it");
        cx.run_until_parked();
        let heard = record(cx, &view, said);
        assert!(view.read_with(cx, |view, cx| view.focus(cx)).is_none(), "the outline holds it");
        click(cx, "workflow-name");
        assert_eq!(view.read_with(cx, |view, _| view.focused().cloned()), Some(name.clone()));
        let focus = view.read_with(cx, |view, cx| view.focus(cx)).unwrap();
        assert_eq!(focus, first.read_with(cx, |field, cx| field.focus_handle(cx)));
        cx.update(|window, cx| window.focus(&focus, cx));
        cx.simulate_input(" now");
        cx.run_until_parked();
        assert_eq!(first.read_with(cx, |field, _| field.text().to_string()), "Ship it now");
        let outline = bounds(cx, "outline");
        cx.simulate_click(outline.bottom_left() + gpui::point(px(8.), px(-8.)), Modifiers::none());
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |view, _| view.focused().cloned()), None);
        let said = heard.take();
        assert_eq!(said.first().map(String::as_str), Some("focused"));
        assert!(said.contains(&"edited workflow-name".to_string()), "{said:?}");
        assert_eq!(said.last().map(String::as_str), Some("focused"), "{said:?}");
        view.update(cx, |view, _| {
            view.set_focused(Some(name.clone()));
            view.inspect(Inspected::Approval);
            assert_eq!(view.focused(), None, "showing something else drops it");
        });
    }

    #[test]
    fn the_step_shown_is_the_one_chosen_else_the_first() {
        let draft = draft();
        let shown = |chosen| shown_step(&draft, chosen).map(|step| step.id.as_str());
        assert_eq!(shown(Some("s-2")), Some("s-2"));
        assert_eq!(shown(Some("s-9")), Some("s-1"), "a deleted one");
        assert_eq!(shown(None), Some("s-1"));
        let empty = Definition {
            steps: Vec::new(),
            ..draft.clone()
        };
        assert_eq!(shown_step(&empty, None).map(|step| step.id.as_str()), None);
    }
}
