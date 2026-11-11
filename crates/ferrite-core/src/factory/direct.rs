//! Where a Task needs the operator, or an agent they chose stands in for
//! them (#222): a Worker's question and its answer, a Provider Decision
//! raised in a Worker's Session, Ready to approve and who decides it,
//! rework with feedback, and one Task's redirect, which never touches the
//! saved Workflow.
//!
//! A redirect supersedes the run it interrupts: that Worker is parked and
//! remembered, so a late report from it is ignored, after a restart too.

use serde::{Deserialize, Serialize};

use super::{
    Actor, Answer, Approver, Destination, Effect, Outcome, Refusal, RefusalCode,
    Resolved, Status, Step, StepId, StepResult, Task, Watch,
};
use crate::ThreadId;

/// What a Task holds of the operator's say, and of who is waiting on it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Oversight {
    /// A Worker's question, waiting for the operator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<Question>,
    /// How many questions were asked: the next one's id.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub asked: u64,
    /// Every question answered, which later Steps are told.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers: Vec<Answered>,
    /// Rework, rejection and redirect feedback, which every later Step is
    /// told.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feedback: Vec<Feedback>,
    /// The operator's direction for this Task only.
    #[serde(
        default,
        rename = "override",
        skip_serializing_if = "Option::is_none"
    )]
    pub direction: Option<Override>,
    /// "Return it to me": the operator approves, whoever the Workflow
    /// names.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub to_me: bool,
    /// The current run decides an approval.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deciding: bool,
    /// A Provider Decision waiting in the Worker's Session, as it reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    /// Workers whose run a redirect superseded: what they report is
    /// ignored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub superseded: Vec<ThreadId>,
}

fn is_zero(count: &u64) -> bool {
    *count == 0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub id: u64,
    pub text: String,
    /// Who asked: the Step's name, or "The agent" for a one-off.
    pub asker: String,
    pub worker: ThreadId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answered {
    pub asker: String,
    pub question: String,
    pub answer: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackKind {
    /// The operator asked for rework.
    Rework,
    /// The deciding agent did not approve.
    Rejected,
    /// The operator's note with a redirect.
    Redirect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feedback {
    pub kind: FeedbackKind,
    pub by: Actor,
    /// Who it was sent to, as the operator reads it.
    pub to: String,
    pub text: String,
    /// How many results there were when it was given: a returning Worker
    /// is told what came after its own last one.
    pub after: usize,
}

/// After a redirect, follow the Workflow's routes or return the Task to
/// the operator to approve.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ThenReturn {
    #[default]
    Routes,
    Me,
}

/// One Task's direction: never the saved Workflow's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Override {
    /// The Step it was sent to.
    pub to: StepId,
    /// Steps a route passes over, as if they had finished.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skip: Vec<StepId>,
    #[serde(default)]
    pub then: ThenReturn,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

fn invalid(message: impl Into<String>) -> Refusal {
    Refusal::new(RefusalCode::Invalid, message)
}

fn not_now(message: impl Into<String>) -> Refusal {
    Refusal::new(RefusalCode::NotNow, message)
}

impl Task {
    /// Who a question or a permission is from: its Step's name.
    fn asker(&self) -> String {
        self.current_step()
            .map(|step| step.name.clone())
            .unwrap_or_else(|| "The agent".into())
    }

    /// Its Worker's run may still be going: a report from it counts.
    pub(super) fn running(&self) -> bool {
        self.status == Status::Working
            || (self.status == Status::NeedsHuman && self.oversight.permission.is_some())
    }

    /// The current Worker's run stops counting: it is parked, and what it
    /// reports later is ignored.
    fn supersede(&mut self, now_ms: i64, effects: &mut Vec<Effect>) {
        let Some(old) = self.worker.take() else {
            return;
        };
        self.workers.retain(|each| each.thread != old);
        if !self.oversight.superseded.contains(&old) {
            self.oversight.superseded.push(old);
        }
        let name = self.asker();
        self.note(
            now_ms,
            Actor::Ferrite,
            format!("{name}'s run was superseded; a late result from it is ignored"),
        );
        effects.push(Effect::Park { thread: old });
    }

    fn label(&self, to: &Destination) -> String {
        match &self.workflow {
            Some(run) => run.definition.route_label("", to),
            None => to.word(),
        }
    }

    /// This Step decides the approval its own route leads to, in the same
    /// run: an agent approves, it is that agent, and nothing returns the
    /// Task to the operator.
    pub(super) fn decides_in_own_run(&self, step: &Step) -> bool {
        let Some(run) = &self.workflow else {
            return false;
        };
        let approval = &run.definition.approval;
        approval.by == Approver::Agent
            && approval.step.as_deref() == Some(step.id.as_str())
            && step.on_finished == Destination::Approval
            && !self.oversight.to_me
            && !self
                .oversight
                .direction
                .as_ref()
                .is_some_and(|direction| direction.then == ThenReturn::Me && direction.to == step.id)
    }

    /// The current run decides an approval, now or in its own report.
    pub(super) fn decides(&self) -> bool {
        self.oversight.deciding
            || self
                .current_step()
                .is_some_and(|step| self.decides_in_own_run(step))
    }
}

// ------------------------------------------------------------ commands

/// A Worker asks the operator: its Task needs them, and nothing after it
/// starts until they answer.
pub(super) fn ask(
    task: &mut Task,
    worker: ThreadId,
    text: String,
    now_ms: i64,
    effects: &mut Vec<Effect>,
) -> Result<Answer, Refusal> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(invalid("say the question"));
    }
    if !task.running() || task.worker != Some(worker) {
        return Ok(Answer::Changed(false));
    }
    let asker = task.asker();
    task.oversight.asked += 1;
    task.oversight.question = Some(Question {
        id: task.oversight.asked,
        text: text.clone(),
        asker: asker.clone(),
        worker,
    });
    task.oversight.permission = None;
    task.note(now_ms, Actor::Agent, format!("{asker} asked: {text}"));
    task.reason = Some(format!("{asker} asks: {text}"));
    task.enter(Status::NeedsHuman, effects);
    Ok(Answer::Changed(true))
}

/// The operator answers the question waiting: the asker continues with
/// the question and the answer. A second answer finds none waiting.
pub(super) fn answer(
    task: &mut Task,
    text: String,
    question: Option<u64>,
    now_ms: i64,
    effects: &mut Vec<Effect>,
) -> Result<Answer, Refusal> {
    let text = text.trim().to_string();
    let Some(asked) = task.oversight.question.clone() else {
        return Ok(Answer::Changed(false));
    };
    if question.is_some_and(|id| id != asked.id) || task.status != Status::NeedsHuman {
        return Ok(Answer::Changed(false));
    }
    if text.is_empty() {
        return Err(invalid("write the answer"));
    }
    task.oversight.question = None;
    task.oversight.answers.push(Answered {
        asker: asked.asker.clone(),
        question: asked.text.clone(),
        answer: text.clone(),
    });
    task.note(
        now_ms,
        Actor::Operator,
        format!("Answered {}: {text}", asked.asker),
    );
    task.reason = None;
    task.enter(Status::Working, effects);
    task.worker = Some(asked.worker);
    task.watch.prompt = Some(super::answer_prompt(task, &asked.text, &text));
    Ok(Answer::Changed(true))
}

/// The operator approves what waits in Ready to approve.
pub(super) fn approve_command(
    task: &mut Task,
    now_ms: i64,
    effects: &mut Vec<Effect>,
) -> Result<Answer, Refusal> {
    if task.status != Status::Approval {
        return Ok(Answer::Changed(false));
    }
    task.note(now_ms, Actor::Operator, "Approved");
    approved(task, now_ms, effects);
    Ok(Answer::Changed(true))
}

/// Rework with the operator's feedback, to the recipient they chose (the
/// Workflow's when none): it and every later Step are told.
pub(super) fn rework(
    task: &mut Task,
    feedback: String,
    to: Option<Destination>,
    now_ms: i64,
    effects: &mut Vec<Effect>,
) -> Result<Answer, Refusal> {
    let feedback = feedback.trim().to_string();
    if feedback.is_empty() {
        return Err(invalid("write the feedback the agent works from"));
    }
    let Some(run) = &task.workflow else {
        return Err(invalid(
            "a one-off task has no agents to send rework to: tell its agent in the Worker tab",
        ));
    };
    if !matches!(
        task.status,
        Status::Approval | Status::NeedsHuman | Status::Stopped
    ) {
        return Err(not_now(format!(
            "a task that is {} cannot be sent for rework: redirect it instead",
            task.status.word().to_lowercase()
        )));
    }
    let to = to.unwrap_or_else(|| run.definition.approval.rework_to.clone());
    match &to {
        Destination::Coordinator => {}
        Destination::Step(id) if run.definition.step(id).is_some() => {}
        Destination::Step(_) => return Err(invalid("that agent is not in this task's version")),
        _ => return Err(invalid("rework goes to an agent or the Coordinator")),
    }
    if task.oversight.permission.is_some() {
        task.supersede(now_ms, effects);
    }
    let label = task.label(&to);
    task.note(
        now_ms,
        Actor::Operator,
        format!("Requested rework from {label}: {feedback}"),
    );
    task.oversight.feedback.push(Feedback {
        kind: FeedbackKind::Rework,
        by: Actor::Operator,
        to: label,
        text: feedback,
        after: task.results.len(),
    });
    task.oversight.question = None;
    task.oversight.permission = None;
    task.oversight.deciding = false;
    task.oversight.to_me = false;
    task.oversight.direction = None;
    follow(task, None, to, String::new(), now_ms, effects);
    Ok(Answer::Changed(true))
}

/// Send one Task to `to`, skipping `skip`, then following the routes or
/// returning it to the operator. The run it interrupts is superseded.
pub(super) fn redirect(
    task: &mut Task,
    direction: Override,
    now_ms: i64,
    effects: &mut Vec<Effect>,
) -> Result<Answer, Refusal> {
    let Some(run) = &task.workflow else {
        return Err(invalid(
            "a one-off task has one agent: tell it in the Worker tab",
        ));
    };
    if task.status == Status::Completed {
        return Err(not_now("a completed task cannot be redirected"));
    }
    let definition = run.definition.clone();
    let Some(target) = definition.step(&direction.to).cloned() else {
        return Err(Refusal::new(
            RefusalCode::UnknownStep,
            "that agent is not in this task's version",
        ));
    };
    let mut skip = Vec::new();
    for id in direction.skip {
        if definition.step(&id).is_none() {
            return Err(Refusal::new(
                RefusalCode::UnknownStep,
                format!("{id} is not an agent in this task's version"),
            ));
        }
        if id != target.id && !skip.contains(&id) {
            skip.push(id);
        }
    }
    let direction = Override {
        to: target.id.clone(),
        skip,
        then: direction.then,
        note: direction.note.trim().to_string(),
    };
    if matches!(task.status, Status::Working | Status::NeedsHuman) {
        task.supersede(now_ms, effects);
    }
    let mut said = vec![format!("Redirected to {}.", target.name)];
    let skipped: Vec<&str> = direction
        .skip
        .iter()
        .filter_map(|id| definition.step(id).map(|step| step.name.as_str()))
        .collect();
    if !skipped.is_empty() {
        said.push(format!("Skip {} for this task.", skipped.join(", ")));
    }
    said.push(match direction.then {
        ThenReturn::Me => "Then return it to you to approve.".into(),
        ThenReturn::Routes => "Then follow the workflow's routes.".into(),
    });
    if !direction.note.is_empty() {
        said.push(format!("Feedback: {}", direction.note));
        task.oversight.feedback.push(Feedback {
            kind: FeedbackKind::Redirect,
            by: Actor::Operator,
            to: target.name.clone(),
            text: direction.note.clone(),
            after: task.results.len(),
        });
    }
    said.push("The saved workflow is unchanged.".into());
    task.note(now_ms, Actor::Operator, said.join(" "));
    task.oversight.question = None;
    task.oversight.permission = None;
    task.oversight.deciding = false;
    task.oversight.to_me = direction.then == ThenReturn::Me;
    task.oversight.direction = Some(direction);
    task.result = None;
    super::enter_step(task, &target, false, now_ms, effects);
    Ok(Answer::Changed(true))
}

// ------------------------------------------------------------ routing

/// A Workflow Task's current Worker reported: record it and decide what
/// follows. A deciding run approves or rejects; one sent with "return it
/// to me" comes back to the operator; anything else follows its route.
pub(super) fn reported(
    task: &mut Task,
    outcome: Outcome,
    now_ms: i64,
    effects: &mut Vec<Effect>,
) -> Result<(), Refusal> {
    let Some(step) = task.current_step().cloned() else {
        return Ok(());
    };
    let decides = task.decides();
    match (&outcome, decides) {
        (Outcome::Approved { .. } | Outcome::Rejected { .. }, false) => {
            return Err(invalid(
                "this agent is not deciding an approval: report finished or unable",
            ))
        }
        (Outcome::Finished { .. }, true) if task.oversight.deciding => {
            return Err(invalid(
                "you are deciding the approval: report approved or rejected",
            ))
        }
        _ => {}
    }
    let (said, text) = match &outcome {
        Outcome::Finished { summary } => ("finished", summary.clone()),
        Outcome::Unable { tried } => ("could not finish", tried.clone()),
        Outcome::Approved { summary } => ("approves", summary.clone()),
        Outcome::Rejected { reasons } => ("does not approve", reasons.clone()),
    };
    task.note(
        now_ms,
        Actor::Agent,
        format!("{} {said}: {text}", step.name),
    );
    task.results.push(StepResult {
        step: step.id.clone(),
        name: step.name.clone(),
        outcome: outcome.clone(),
    });
    task.oversight.permission = None;
    let returns_to_me = task
        .oversight
        .direction
        .as_ref()
        .is_some_and(|direction| direction.then == ThenReturn::Me && direction.to == step.id);
    match outcome {
        Outcome::Approved { .. } => approved(task, now_ms, effects),
        Outcome::Rejected { reasons } => {
            let to = task
                .workflow
                .as_ref()
                .map(|run| run.definition.approval.rework_to.clone())
                .unwrap_or(Destination::Human);
            let label = task.label(&to);
            task.oversight.feedback.push(Feedback {
                kind: FeedbackKind::Rejected,
                by: Actor::Agent,
                to: label,
                text: format!("{} did not approve: {reasons}", step.name),
                after: task.results.len(),
            });
            task.oversight.deciding = false;
            task.result = None;
            follow(task, Some(&step.id), to, reasons, now_ms, effects);
        }
        Outcome::Unable { tried } if task.oversight.deciding => {
            // The deciding agent cannot decide: the operator does.
            task.oversight.deciding = false;
            task.oversight.to_me = true;
            task.reason = Some(format!("{} could not decide: {tried}", step.name));
            task.enter(Status::Approval, effects);
        }
        Outcome::Finished { summary } if returns_to_me => {
            task.oversight.to_me = true;
            enter_approval(task, Some(&step.id), summary, now_ms, effects);
        }
        Outcome::Finished { summary } => {
            follow(task, Some(&step.id), step.on_finished, summary, now_ms, effects)
        }
        Outcome::Unable { tried } => follow(task, Some(&step.id), step.on_unable, tried, now_ms, effects),
    }
    Ok(())
}

/// Take `route` from `from` (none for an operator's choice): to a Step
/// (passing over the ones this Task skips), to Ready to approve, or to an
/// end. Moving between Steps is silent; an end is told.
pub(super) fn follow(
    task: &mut Task,
    from: Option<&str>,
    route: Destination,
    text: String,
    now_ms: i64,
    effects: &mut Vec<Effect>,
) {
    let Some(definition) = task.workflow.as_ref().map(|run| run.definition.clone()) else {
        return;
    };
    let mut from = from.map(str::to_string);
    let mut route = route;
    for _ in 0..=definition.steps.len() + 1 {
        let at = from.clone().unwrap_or_default();
        let name = definition
            .step(&at)
            .map_or_else(|| "The task".to_string(), |step| step.name.clone());
        match definition.resolve(&at, &route) {
            Resolved::Step(next) => {
                let skipped = task
                    .oversight
                    .direction
                    .as_ref()
                    .is_some_and(|direction| direction.skip.contains(&next.id));
                if skipped {
                    task.note(
                        now_ms,
                        Actor::Ferrite,
                        format!("Skipped {} for this task", next.name),
                    );
                    from = Some(next.id.clone());
                    route = next.on_finished.clone();
                    continue;
                }
                let next = next.clone();
                super::enter_step(task, &next, false, now_ms, effects);
            }
            Resolved::Approval => enter_approval(task, from.as_deref(), text, now_ms, effects),
            Resolved::Complete => {
                task.oversight.direction = None;
                task.result = Some(text);
                task.reason = None;
                task.enter(Status::Completed, effects);
            }
            Resolved::Human => {
                task.reason = Some(text);
                task.enter(Status::NeedsHuman, effects);
            }
            Resolved::Coordinator => {
                task.reason = Some(if text.is_empty() {
                    "The task routes to the Coordinator, which this Ferrite does not run yet."
                        .to_string()
                } else {
                    format!(
                        "{name} routes to the Coordinator, which this Ferrite does not run yet: \
                         {text}"
                    )
                });
                task.enter(Status::NeedsHuman, effects);
            }
            Resolved::Broken => {
                task.reason = Some(format!(
                    "{name}'s route leads to an agent that is not in this version: {text}"
                ));
                task.enter(Status::NeedsHuman, effects);
            }
        }
        return;
    }
    task.reason = Some("The routes loop without reaching an agent or a decision.".into());
    task.enter(Status::Stopped, effects);
}

/// A result reaches Ready to approve. The operator decides, unless an
/// agent approves and nothing returned the Task to them: then the
/// deciding Step runs (once more, if it was not the one deciding) to
/// decide.
fn enter_approval(
    task: &mut Task,
    from: Option<&str>,
    result: String,
    now_ms: i64,
    effects: &mut Vec<Effect>,
) {
    if task
        .oversight
        .direction
        .take()
        .is_some_and(|direction| direction.then == ThenReturn::Me)
    {
        task.oversight.to_me = true;
    }
    if !result.is_empty() {
        task.result = Some(result);
    }
    task.reason = None;
    let Some(definition) = task.workflow.as_ref().map(|run| run.definition.clone()) else {
        return;
    };
    let approval = &definition.approval;
    if approval.by == Approver::Agent && !task.oversight.to_me {
        match approval.step.as_deref().and_then(|id| definition.step(id)) {
            Some(gate) => {
                let again = from == Some(gate.id.as_str());
                task.note(
                    now_ms,
                    Actor::Ferrite,
                    if again {
                        format!("{} runs once more to decide the approval", gate.name)
                    } else {
                        format!("Asked {} to decide the approval", gate.name)
                    },
                );
                let gate = gate.clone();
                super::enter_step(task, &gate, true, now_ms, effects);
                return;
            }
            None => {
                task.oversight.to_me = true;
                task.note(
                    now_ms,
                    Actor::Ferrite,
                    "The deciding agent is not in this version, so the approval is yours",
                );
            }
        }
    }
    task.note(now_ms, Actor::Ferrite, "Ready to approve");
    task.enter(Status::Approval, effects);
}

/// Approved, by the operator or the deciding agent: what follows approval
/// runs (a Step, or Completed).
fn approved(task: &mut Task, now_ms: i64, effects: &mut Vec<Effect>) {
    task.oversight.to_me = false;
    task.oversight.deciding = false;
    task.oversight.direction = None;
    let after = task
        .workflow
        .as_ref()
        .map_or(Destination::Complete, |run| run.definition.approval.after.clone());
    let result = task.result.clone().unwrap_or_default();
    match after {
        Destination::Step(_) => follow(task, None, after, result, now_ms, effects),
        _ => {
            task.reason = None;
            task.result = Some(result).filter(|result| !result.is_empty());
            task.enter(Status::Completed, effects);
        }
    }
}

/// A Provider Decision waits in the Worker's Session: the Task needs the
/// operator, who answers it in the Task. Answered, it works again.
pub(super) fn permission(
    task: &mut Task,
    waiting: Option<&str>,
    turns: u64,
    now_ms: i64,
    effects: &mut Vec<Effect>,
) -> bool {
    match (waiting, task.oversight.permission.is_some()) {
        (Some(what), false) if task.status == Status::Working => {
            let asker = task.asker();
            task.oversight.permission = Some(what.to_string());
            task.note(
                now_ms,
                Actor::Agent,
                format!("{asker} asks permission: {what}"),
            );
            task.reason = Some(format!("{asker} asks permission: {what}"));
            task.enter(Status::NeedsHuman, effects);
            true
        }
        (None, true) if task.status == Status::NeedsHuman => {
            task.oversight.permission = None;
            task.reason = None;
            task.note(now_ms, Actor::Ferrite, "Permission answered; the agent continues");
            task.enter(Status::Working, effects);
            task.watch = Watch {
                baseline: turns,
                ..Watch::default()
            };
            true
        }
        _ => false,
    }
}

/// What the deciding agent is told: the instructions the operator wrote,
/// what is being approved, and how to say its decision.
pub(super) fn decision_section(task: &Task) -> String {
    let instructions = task
        .workflow
        .as_ref()
        .map(|run| run.definition.approval.instructions.trim().to_string())
        .unwrap_or_default();
    let what = task
        .result
        .clone()
        .or_else(|| {
            task.results.iter().rev().find_map(|result| match &result.outcome {
                Outcome::Finished { summary } => Some(summary.clone()),
                _ => None,
            })
        })
        .unwrap_or_else(|| "this task's current result".into());
    format!(
        "## Approval decision\n\nYou decide whether this task's result is approved, as the \
         operator would.\n\n{instructions}\n\nWhat is being approved: {what}\n\nReport your \
         decision with one command:\n\n    {report} approved \"<what you verified>\"\n\nor, if \
         it should not be approved:\n\n    {report} rejected \"<your reasons>\"\n\nIf only the \
         operator can decide, ask them:\n\n    {question} \"<your question>\"\n",
        report = super::REPORT,
        question = super::QUESTION,
    )
}

/// What a fresh or returning Worker is told the operator said: the
/// questions answered and the feedback given since `after` results.
pub(super) fn said_since(task: &Task, after: usize, answers: bool) -> String {
    let mut out = String::new();
    if answers && !task.oversight.answers.is_empty() {
        let lines: Vec<String> = task
            .oversight
            .answers
            .iter()
            .map(|each| {
                format!(
                    "- {} asked: {}\n  The operator answered: {}",
                    each.asker, each.question, each.answer
                )
            })
            .collect();
        out.push_str(&format!("\n## Questions answered\n\n{}\n", lines.join("\n")));
    }
    let feedback: Vec<String> = task
        .oversight
        .feedback
        .iter()
        .filter(|each| each.after >= after)
        .map(|each| match each.kind {
            FeedbackKind::Rejected => format!("- {}", each.text),
            FeedbackKind::Rework => format!("- The operator asked {} for rework: {}", each.to, each.text),
            FeedbackKind::Redirect => {
                format!("- The operator sent the task to {}: {}", each.to, each.text)
            }
        })
        .collect();
    if !feedback.is_empty() {
        out.push_str(&format!("\n## Feedback\n\n{}\n", feedback.join("\n")));
    }
    out
}
