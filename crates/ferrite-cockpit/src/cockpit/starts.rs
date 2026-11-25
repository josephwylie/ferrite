//! A Workflow's "Tasks start" settings in the window (#225, ADR 0037).
//! The start's inspector is `ferrite_factory::starts::StartsView`, which
//! the Factory holds; it reads the shown Workflow's start through a
//! `Source` the root makes over itself (`starts_inputs`) and says a press
//! as a `StartsEvent`. Whether the start shows stays the Workflows
//! editor's (`Inspected::Start`); every edit, asking the login for a list
//! and the automatic starts' switch are Server calls, so they stay here,
//! and the root's pump has the view take the answers.

use super::workflows::{Field, Inspected, Pick};
use super::*;
use ferrite_core::factory_board::{self, Start, Trigger, Turn};
use ferrite_core::protocol::{GitHubAsk, WorkflowAction};
use ferrite_factory::starts::{StartPick, StartsEvent, StartsInputs, StartsView};
use ferrite_ui::source::Source;

impl CockpitView {
    /// Settings' stepper on how many Factory Tasks work at once.
    pub(crate) fn step_factory_limit(&mut self, delta: i32, cx: &mut Context<Self>) {
        let Some(now) = self.factory_limit(cx) else {
            return;
        };
        let next = if delta == 0 {
            factory_board::DEFAULT_LIMIT
        } else {
            factory_board::clamp_limit(now.saturating_add_signed(delta as isize))
        };
        if next != now {
            self.change_defaults(|defaults| defaults.factory_tasks_at_once = next, cx);
            cx.notify();
        }
    }
}

impl CockpitView {
    /// A Factory's starts, heard from their making.
    pub(super) fn new_starts(&mut self, cx: &mut Context<Self>) -> Entity<StartsView> {
        let inputs = Source::of(&cx.entity(), |root: &Self, _: &(), cx| root.starts_inputs(cx));
        let starts = cx.new(|_| StartsView::new(inputs));
        cx.subscribe(&starts, Self::on_starts_event).detach();
        starts
    }

    /// What the start's inspector draws: the shown Workflow's start, while
    /// it is what the inspector shows.
    fn starts_inputs(&self, cx: &gpui::App) -> Option<StartsInputs> {
        let workflow = self.shown_workflow(cx).filter(|_| self.start_shown(cx))?;
        let fields = self.factory()?.read(cx).editor.read(cx);
        let automation = &workflow.automation;
        Some(StartsInputs {
            workflow: workflow.id.clone(),
            start: workflow.draft.start.clone(),
            saved: workflow
                .latest()
                .map(|latest| (latest.version, latest.definition.start.kind)),
            switch: automation.switch,
            due_ms: automation.due_ms,
            waiting: automation.waiting,
            error: automation.error.clone(),
            backlog: automation.backlog.len(),
            schedule_instructions: fields
                .field(&Field::ScheduleInstructions(workflow.id.clone()))
                .cloned(),
            extra_instructions: fields
                .field(&Field::ExtraInstructions(workflow.id.clone()))
                .cloned(),
        })
    }

    /// The start's inspector: a picker opens its rows, a segment, a field
    /// or a switch changes the draft or the automatic starts.
    fn on_starts_event(&mut self, _: Entity<StartsView>, event: &StartsEvent, cx: &mut Context<Self>) {
        match event {
            StartsEvent::Picks { picks, trigger, at } => {
                let picks = picks
                    .iter()
                    .map(|(item, pick)| (item.clone(), Pick::Start(pick.clone())))
                    .collect();
                self.open_picks(picks, trigger, *at, cx)
            }
            StartsEvent::Pick(pick) => self.apply_start_pick(pick.clone(), cx),
            StartsEvent::Field(field) => self.focus_field(field.clone(), cx),
            StartsEvent::Turn(turn) => self.turn_automation(*turn, cx),
            StartsEvent::Backlog(take) => self.answer_backlog(*take, cx),
        }
    }

    /// The start's settings are what the inspector shows.
    pub(crate) fn start_shown(&self, cx: &gpui::App) -> bool {
        self.factory()
            .is_some_and(|view| view.read(cx).editor.read(cx).inspected == Inspected::Start)
    }

    /// Show the shown Workflow's start in the inspector.
    pub(crate) fn open_start(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.factory().map(|view| view.read(cx).editor.clone()) {
            editor.update(cx, |editor, _| editor.inspect(Inspected::Start));
        }
        self.ensure_fields(cx);
        self.ask_github_lists(cx);
        cx.notify();
    }

    /// Change the shown draft's start, whole.
    pub(crate) fn edit_start(&mut self, change: impl FnOnce(&mut Start), cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_workflow(cx) else {
            return;
        };
        let id = workflow.id.clone();
        let mut start = workflow.draft.start.clone();
        change(&mut start);
        self.workflow_ask(
            WorkflowAction::EditStart {
                workflow: id,
                start,
            },
            cx,
        );
        self.ask_github_lists(cx);
    }

    pub(crate) fn apply_start_pick(&mut self, pick: StartPick, cx: &mut Context<Self>) {
        self.edit_start(
            |start| match pick {
                StartPick::Kind(kind) => {
                    start.kind = kind;
                    if !Trigger::offered(kind).contains(&start.github.trigger) {
                        start.github.trigger = Trigger::Labelled;
                    }
                }
                StartPick::Recurrence(recurrence) => start.schedule.recurrence = recurrence,
                StartPick::Weekday(day) => start.schedule.weekday = day,
                StartPick::Interval(hours) => start.schedule.interval_hours = hours,
                StartPick::Time(time) => start.schedule.time = Some(time),
                StartPick::Repository(repository) => {
                    if start.github.repository.as_deref() != Some(repository.as_str()) {
                        start.github.label = None;
                    }
                    start.github.repository = Some(repository);
                }
                StartPick::Trigger(trigger) => start.github.trigger = trigger,
                StartPick::Label(label) => start.github.label = Some(label),
                StartPick::Missed(missed) => start.schedule.missed = missed,
                StartPick::Overlap(overlap) => start.schedule.overlap = overlap,
            },
            cx,
        );
    }

    /// Turn the shown Workflow's automatic starts on, pause or resume.
    pub(crate) fn turn_automation(&mut self, turn: Turn, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_workflow(cx).map(|shown| shown.id.clone()) else {
            return;
        };
        self.workflow_ask(WorkflowAction::Automation { workflow, turn }, cx);
    }

    /// The operator's word on what already matched.
    pub(crate) fn answer_backlog(&mut self, take: bool, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_workflow(cx).map(|shown| shown.id.clone()) else {
            return;
        };
        self.workflow_ask(WorkflowAction::Backlog { workflow, take }, cx);
    }

    /// The start's text fields, as the draft holds them.
    pub(crate) fn start_fields(&self, cx: &gpui::App) -> Vec<(Field, String)> {
        let Some(workflow) = self.shown_workflow(cx).filter(|_| self.start_shown(cx)) else {
            return Vec::new();
        };
        let start = &workflow.draft.start;
        vec![
            (
                Field::ScheduleInstructions(workflow.id.clone()),
                start.schedule.instructions.clone(),
            ),
            (
                Field::ExtraInstructions(workflow.id.clone()),
                start.github.extra_instructions.clone(),
            ),
        ]
    }

    /// What was typed in one of the start's fields goes to the draft.
    pub(crate) fn start_field_edited(
        &mut self,
        field: &Field,
        text: String,
        cx: &mut Context<Self>,
    ) {
        match field {
            Field::ScheduleInstructions(_) => {
                self.edit_start(|start| start.schedule.instructions = text, cx)
            }
            Field::ExtraInstructions(_) => {
                self.edit_start(|start| start.github.extra_instructions = text, cx)
            }
            _ => {}
        }
    }

    /// Ask the login for what a GitHub start's pickers need: the
    /// repositories once, and the chosen repository's labels. The
    /// Factory's starts wait for each answer.
    fn ask_github_lists(&mut self, cx: &mut Context<Self>) {
        let Some(start) = self
            .shown_workflow(cx)
            .filter(|_| self.start_shown(cx))
            .map(|workflow| workflow.draft.start.clone())
        else {
            return;
        };
        if !start.kind.is_github() {
            return;
        }
        let Some(starts) = self.factory().map(|view| view.read(cx).starts.clone()) else {
            return;
        };
        let labels = start
            .github
            .repository
            .map(|repository| GitHubAsk::Labels { repository });
        let Some(machine) = self.factory_machine() else {
            return;
        };
        for ask in std::iter::once(GitHubAsk::Repositories).chain(labels) {
            if starts.read(cx).unasked(&ask) {
                let asked = self
                    .cockpit
                    .update(cx, |cockpit, _| cockpit.github_names(machine, ask.clone()));
                starts.update(cx, |starts, cx| starts.listen(ask, asked, cx));
            }
        }
    }

    /// A list that answered.
    pub(super) fn poll_start_lists(&mut self, cx: &mut Context<Self>) {
        // The draft a window is told of may name a repository whose labels
        // were never asked for.
        self.ask_github_lists(cx);
        if let Some(starts) = self.factory().map(|view| view.read(cx).starts.clone()) {
            starts.update(cx, |starts, cx| starts.listed(cx));
        }
    }
}
