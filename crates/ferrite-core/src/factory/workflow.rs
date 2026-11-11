//! A Workflow (ADR 0037, #221): the process the operator authored for a
//! Project, an ordered outline of Steps, saved in numbered versions. A
//! Task keeps the version it started with.
//!
//! Nothing here is prescribed: a Workflow starts Blank or from the
//! "Issue to reviewed PR" template and every value is the operator's.

use serde::{Deserialize, Serialize};

use super::pr::{MergeOutcome, MergeRoutes};
use super::starts::{Automation, Start};
use super::Agent;
use crate::store::Provider;
use crate::workspace::registry::ProjectId;
use crate::ModelInfo;

/// A Workflow's id: `w-<n>`, never given twice by one Server.
pub type WorkflowId = String;
/// A Step's id within its Workflow: `s-<n>`, never given twice by it, so
/// a route to a deleted Step stays a route to that Step.
pub type StepId = String;

/// Where a reported outcome sends a Task. `Next` follows the outline's
/// order, so reordering keeps a chain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Destination {
    /// The Step after this one in the outline.
    Next,
    /// A named Step, by id: it may have been deleted since, and stays.
    Step(StepId),
    /// Ready to approve: the operator, or the agent they chose, decides.
    Approval,
    Complete,
    Human,
    Coordinator,
    /// Stay with the Step: only "Checks pending" keeps waiting (#223).
    Wait,
}

impl Destination {
    /// Its wire word: `next`, `step:<id>`, `approval`, `complete`,
    /// `human`, `coordinator`.
    pub fn word(&self) -> String {
        match self {
            Destination::Next => "next".into(),
            Destination::Step(id) => format!("step:{id}"),
            Destination::Approval => "approval".into(),
            Destination::Complete => "complete".into(),
            Destination::Human => "human".into(),
            Destination::Coordinator => "coordinator".into(),
            Destination::Wait => "wait".into(),
        }
    }
}

impl std::str::FromStr for Destination {
    type Err = String;

    fn from_str(word: &str) -> Result<Self, String> {
        Ok(match word {
            "next" => Destination::Next,
            "approval" => Destination::Approval,
            "complete" => Destination::Complete,
            "human" => Destination::Human,
            "coordinator" => Destination::Coordinator,
            "wait" => Destination::Wait,
            other => match other.strip_prefix("step:") {
                Some(id) if !id.is_empty() => Destination::Step(id.to_string()),
                _ => {
                    return Err(format!(
                        "`{word}` is no route: next, step:<id>, approval, complete, human, coordinator or wait"
                    ))
                }
            },
        })
    }
}

impl Serialize for Destination {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.word())
    }
}

impl<'de> Deserialize<'de> for Destination {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let word = String::deserialize(deserializer)?;
        word.parse().map_err(serde::de::Error::custom)
    }
}

/// One position in a Workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    pub id: StepId,
    pub name: String,
    pub instructions: String,
    /// Skills its Worker is told to use, by name.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Its Provider, model and Effort.
    #[serde(flatten)]
    pub agent: Agent,
    pub on_finished: Destination,
    pub on_unable: Destination,
    /// Start a new Worker on every visit, parking the last one; otherwise
    /// a returning Task continues this Step's Worker.
    #[serde(default)]
    pub fresh: bool,
    /// Routes by merge result, replacing `on_finished` when set (#223).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_routes: Option<MergeRoutes>,
}

impl Step {
    /// Every route it has, in the outline's order: one per merge result
    /// (or "When finished"), then "If unable to finish".
    pub fn routes(&self) -> Vec<(RouteKind, &Destination)> {
        let mut routes = match &self.outcome_routes {
            Some(by) => MergeOutcome::ALL
                .into_iter()
                .map(|outcome| (RouteKind::Merge(outcome), by.get(outcome)))
                .collect(),
            None => vec![(RouteKind::Finished, &self.on_finished)],
        };
        routes.push((RouteKind::Unable, &self.on_unable));
        routes
    }

    /// The route of `kind`, if it has one.
    pub fn route(&self, kind: RouteKind) -> Option<&Destination> {
        match kind {
            RouteKind::Finished => Some(&self.on_finished),
            RouteKind::Unable => Some(&self.on_unable),
            RouteKind::Merge(outcome) => self.outcome_routes.as_ref().map(|by| by.get(outcome)),
        }
    }
}

/// What a Workflow says: its name, its Steps in order, and who approves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    pub steps: Vec<Step>,
    /// How its Tasks start (#225); manually when absent.
    #[serde(default, skip_serializing_if = "Start::is_default")]
    pub start: Start,
    #[serde(default, skip_serializing_if = "Approval::is_default")]
    pub approval: Approval,
}

/// Who decides a result routed to Ready to approve.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Approver {
    #[default]
    Human,
    Agent,
}

/// A Workflow's approval settings (#222).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approval {
    #[serde(default)]
    pub by: Approver,
    /// The Step that decides when `by` is an agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<StepId>,
    /// What the deciding agent verifies, returns, and when it may approve.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub instructions: String,
    /// What runs once approved: Completed or a Step.
    #[serde(default = "complete")]
    pub after: Destination,
    /// Where rework goes unless the operator chooses for one Task: a Step
    /// or the Coordinator.
    #[serde(default = "coordinator")]
    pub rework_to: Destination,
}

fn complete() -> Destination {
    Destination::Complete
}

fn coordinator() -> Destination {
    Destination::Coordinator
}

impl Default for Approval {
    fn default() -> Self {
        Self {
            by: Approver::Human,
            step: None,
            instructions: String::new(),
            after: Destination::Complete,
            rework_to: Destination::Coordinator,
        }
    }
}

impl Approval {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A change to a Workflow's approval: each field given is set. An empty
/// `step` clears the deciding Step.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ApprovalChange {
    /// `human` or `agent`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<Approver>,
    /// The deciding Step's id (`s-1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    /// The decision instructions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// `complete` or `step:<id>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub after: Option<Destination>,
    /// `step:<id>` or `coordinator`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub rework_to: Option<Destination>,
}

impl ApprovalChange {
    pub(super) fn apply(self, approval: &mut Approval) {
        if let Some(by) = self.by {
            approval.by = by;
        }
        if let Some(step) = self.step {
            approval.step = (!step.trim().is_empty()).then_some(step);
        }
        if let Some(instructions) = self.instructions {
            approval.instructions = instructions;
        }
        if let Some(after) = self.after {
            approval.after = after;
        }
        if let Some(rework_to) = self.rework_to {
            approval.rework_to = rework_to;
        }
    }
}

/// One saved version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    /// 1, 2, …
    pub version: u32,
    pub saved_ms: i64,
    pub definition: Definition,
}

/// A Project's Workflow: the draft being edited and its saved versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workflow {
    pub id: WorkflowId,
    pub project: ProjectId,
    pub draft: Definition,
    #[serde(default)]
    pub versions: Vec<Version>,
    /// The next Step's number.
    pub next_step: u64,
    /// Its automatic starts' switch and memory (#225), apart from the
    /// draft and its versions: saving never turns them on.
    #[serde(default, skip_serializing_if = "Automation::is_default")]
    pub automation: Automation,
}

impl Workflow {
    pub fn latest(&self) -> Option<&Version> {
        self.versions.last()
    }

    /// The draft differs from the latest version (or none was saved).
    pub fn unsaved(&self) -> bool {
        self.latest()
            .is_none_or(|version| version.definition != self.draft)
    }

    pub(super) fn new_step_id(&mut self) -> StepId {
        let id = format!("s-{}", self.next_step);
        self.next_step += 1;
        id
    }
}

/// What a new Workflow starts from: `blank`, one Step to fill in, or
/// `issue_to_reviewed_pr`, Triager, Explorer, Implementer, Reviewer, a
/// Merge agent and a Conflict resolver as an example to edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Template {
    Blank,
    IssueToReviewedPr,
}

impl Template {
    pub const ALL: [Template; 2] = [Template::Blank, Template::IssueToReviewedPr];

    pub fn label(self) -> &'static str {
        match self {
            Template::Blank => "Blank",
            Template::IssueToReviewedPr => "Issue to reviewed PR",
        }
    }
}

/// A Step as a new one is: named, everything else to fill in.
pub(super) fn new_step(id: StepId, name: &str, agent: Agent) -> Step {
    Step {
        id,
        name: name.into(),
        instructions: String::new(),
        skills: Vec::new(),
        agent,
        on_finished: Destination::Complete,
        on_unable: Destination::Human,
        fresh: false,
        outcome_routes: None,
    }
}

/// A template's Definition, its Steps numbered from `workflow`'s.
pub(super) fn from_template(template: Template, workflow: &mut Workflow, agent: &Agent) {
    let mut step = |name: &str, instructions: &str, skills: &[&str]| {
        let mut step = new_step(workflow.new_step_id(), name, agent.clone());
        step.instructions = instructions.into();
        step.skills = skills.iter().map(|skill| skill.to_string()).collect();
        step.on_finished = Destination::Next;
        step
    };
    workflow.draft = match template {
        Template::Blank => {
            let mut only = step("Agent", "", &[]);
            only.on_finished = Destination::Complete;
            Definition {
                name: "Untitled workflow".into(),
                steps: vec![only],
                approval: Approval::default(),
                start: Start::default(),
            }
        }
        Template::IssueToReviewedPr => {
            let triage = step(
                "Triager",
                "Use the triage skill on the incoming issue. Ask the owner about any missing \
                 behaviour before handing it on.",
                &["triage"],
            );
            let explore = step(
                "Explorer",
                "Explore the codebase and the issue discussion. Use codebase-design to prepare a \
                 plan and identify the existing test seam.",
                &["codebase-design"],
            );
            let implement = step(
                "Implementer",
                "Implement the agreed plan and any human feedback. Run the relevant project \
                 checks, then use pr to open or update the pull request.",
                &["implement", "pr"],
            );
            let mut review = step(
                "Reviewer",
                "Read the current PR, the issue and all human feedback. Use code-review. Verify \
                 the requested behaviour and the regression evidence. Report the commands you ran \
                 and what they showed.",
                &["code-review"],
            );
            review.on_finished = Destination::Approval;
            review.on_unable = Destination::Step(implement.id.clone());
            review.fresh = true;
            let mut merge = step(
                "Merge agent",
                "Read the PR reference, its current head, the approval and the feedback in the \
                 task context. Use your configured GitHub tools to check the PR and merge it when \
                 eligible. Confirm it actually merged before reporting success. Report conflicts, \
                 pending checks, missing access, new commits since approval or an unconfirmed \
                 result as that outcome.",
                &[],
            );
            let mut resolve = step(
                "Conflict resolver",
                "Resolve only the reported merge conflicts, keep the approved behaviour, push the \
                 result and hand the changed PR back for review.",
                &["implement"],
            );
            merge.on_finished = Destination::Complete;
            merge.on_unable = Destination::Human;
            merge.outcome_routes = Some(MergeRoutes {
                conflict: Destination::Step(resolve.id.clone()),
                ..MergeRoutes::default()
            });
            resolve.on_finished = Destination::Step(review.id.clone());
            Definition {
                name: "Issue to reviewed PR".into(),
                approval: Approval {
                    after: Destination::Step(merge.id.clone()),
                    rework_to: Destination::Step(implement.id.clone()),
                    ..Approval::default()
                },
                steps: vec![triage, explore, implement, review, merge, resolve],
                start: Start::default(),
            }
        }
    };
}

/// A change to one Step: each field given is set. An empty `model` or
/// `effort` is the Provider's default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StepChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<Provider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// `next`, `step:<id>`, `complete`, `human` or `coordinator`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub on_finished: Option<Destination>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub on_unable: Option<Destination>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fresh: Option<bool>,
    /// Route by merge result (one route per result) or not (#223).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_merge_result: Option<bool>,
    /// Merge confirmed: any route, Completed included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub on_merged: Option<Destination>,
    /// Merge conflicts: a Step, `human` or `coordinator`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub on_conflict: Option<Destination>,
    /// Checks pending: `wait`, a Step, `human` or `coordinator`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub on_pending: Option<Destination>,
    /// Access denied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub on_denied: Option<Destination>,
    /// PR changed since approval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub on_head_changed: Option<Destination>,
    /// Result unconfirmed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<String>")]
    pub on_unknown: Option<Destination>,
}

impl StepChange {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub(super) fn apply(self, step: &mut Step) {
        let blank = |text: String| (!text.trim().is_empty()).then_some(text);
        if let Some(name) = self.name {
            step.name = name;
        }
        if let Some(instructions) = self.instructions {
            step.instructions = instructions;
        }
        if let Some(skills) = self.skills {
            step.skills = skills;
        }
        if let Some(provider) = self.provider {
            step.agent.provider = provider;
        }
        if let Some(model) = self.model {
            step.agent.model = blank(model);
        }
        if let Some(effort) = self.effort {
            step.agent.effort = blank(effort);
        }
        if let Some(route) = self.on_finished {
            step.on_finished = route;
        }
        if let Some(route) = self.on_unable {
            step.on_unable = route;
        }
        if let Some(fresh) = self.fresh {
            step.fresh = fresh;
        }
        match self.by_merge_result {
            Some(true) if step.outcome_routes.is_none() => {
                step.outcome_routes = Some(MergeRoutes::default())
            }
            Some(false) => step.outcome_routes = None,
            _ => {}
        }
        if let Some(routes) = &mut step.outcome_routes {
            for (outcome, to) in [
                (MergeOutcome::Merged, self.on_merged),
                (MergeOutcome::Conflict, self.on_conflict),
                (MergeOutcome::Pending, self.on_pending),
                (MergeOutcome::Denied, self.on_denied),
                (MergeOutcome::HeadChanged, self.on_head_changed),
                (MergeOutcome::Unknown, self.on_unknown),
            ] {
                if let Some(to) = to {
                    routes.set(outcome, to);
                }
            }
        }
    }

    /// The change that sets the route of `kind`.
    pub fn route(kind: RouteKind, to: Destination) -> Self {
        let mut change = Self::default();
        let slot = match kind {
            RouteKind::Finished => &mut change.on_finished,
            RouteKind::Unable => &mut change.on_unable,
            RouteKind::Merge(MergeOutcome::Merged) => &mut change.on_merged,
            RouteKind::Merge(MergeOutcome::Conflict) => &mut change.on_conflict,
            RouteKind::Merge(MergeOutcome::Pending) => &mut change.on_pending,
            RouteKind::Merge(MergeOutcome::Denied) => &mut change.on_denied,
            RouteKind::Merge(MergeOutcome::HeadChanged) => &mut change.on_head_changed,
            RouteKind::Merge(MergeOutcome::Unknown) => &mut change.on_unknown,
        };
        *slot = Some(to);
        change
    }
}

/// What is installed, as far as it is known: each Provider's models and
/// skills. A Provider with no list is not checked (nothing is known), so
/// nothing is flagged for it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    pub models: Vec<(Provider, Vec<ModelInfo>)>,
    pub skills: Vec<(Provider, Vec<String>)>,
}

impl Catalog {
    pub fn models(&self, provider: Provider) -> Option<&[ModelInfo]> {
        self.models
            .iter()
            .find(|(each, models)| *each == provider && !models.is_empty())
            .map(|(_, models)| models.as_slice())
    }

    pub fn skills(&self, provider: Provider) -> Option<&[String]> {
        self.skills
            .iter()
            .find(|(each, _)| *each == provider)
            .map(|(_, skills)| skills.as_slice())
    }

    /// The model `value` names, when its Provider's list is known.
    pub fn model(&self, provider: Provider, value: &str) -> Option<&ModelInfo> {
        self.models(provider)?
            .iter()
            .find(|model| model.value == value || model.resolved.as_deref() == Some(value))
    }
}

/// One thing wrong with a draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    /// The Step it is about, if one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<StepId>,
    pub message: String,
    /// It is about the approval settings.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub approval: bool,
}

/// A route's two kinds, as the outline labels them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteKind {
    Finished,
    Unable,
    /// One merge result's route (#223).
    Merge(MergeOutcome),
}

impl RouteKind {
    pub fn label(self) -> &'static str {
        match self {
            RouteKind::Finished => "When finished",
            RouteKind::Unable => "If unable to finish",
            RouteKind::Merge(outcome) => outcome.label(),
        }
    }

    /// Its selector word: `finished`, `unable`, or the merge result's.
    pub fn word(self) -> &'static str {
        match self {
            RouteKind::Finished => "finished",
            RouteKind::Unable => "unable",
            RouteKind::Merge(outcome) => outcome.word(),
        }
    }
}

impl Definition {
    pub fn step(&self, id: &str) -> Option<&Step> {
        self.steps.iter().find(|step| step.id == id)
    }

    pub(super) fn step_mut(&mut self, id: &str) -> Option<&mut Step> {
        self.steps.iter_mut().find(|step| step.id == id)
    }

    /// The Step after `id` in the outline.
    pub fn after(&self, id: &str) -> Option<&Step> {
        let at = self.steps.iter().position(|step| step.id == id)?;
        self.steps.get(at + 1)
    }

    /// Where `route` from `from` leads: a Step to run, or an end.
    pub fn resolve(&self, from: &str, route: &Destination) -> Resolved<'_> {
        match route {
            Destination::Next => match self.after(from) {
                Some(step) => Resolved::Step(step),
                None => Resolved::Broken,
            },
            Destination::Step(id) => match self.step(id) {
                Some(step) => Resolved::Step(step),
                None => Resolved::Broken,
            },
            Destination::Approval => Resolved::Approval,
            Destination::Complete => Resolved::Complete,
            Destination::Human => Resolved::Human,
            Destination::Coordinator => Resolved::Coordinator,
            Destination::Wait => Resolved::Wait,
        }
    }

    /// What the outline's route picker says for `route` from `from`.
    pub fn route_label(&self, from: &str, route: &Destination) -> String {
        let named = |step: &Step| {
            if step.name.trim().is_empty() {
                "Untitled agent".to_string()
            } else {
                step.name.clone()
            }
        };
        match route {
            Destination::Next => match self.after(from) {
                Some(next) => format!("Next agent \u{b7} {}", named(next)),
                None => "Next agent \u{b7} none after this".into(),
            },
            Destination::Step(id) => match self.step(id) {
                Some(step) => named(step),
                None => "Deleted agent".into(),
            },
            Destination::Approval => "Ready to approve".into(),
            Destination::Complete => "Completed".into(),
            Destination::Human => "Needs human".into(),
            Destination::Coordinator => "Coordinator".into(),
            Destination::Wait => "Keep waiting".into(),
        }
    }

    /// What a route picker offers for one of `from`'s routes, in order.
    pub fn route_options(&self, from: &str, kind: RouteKind) -> Vec<Destination> {
        let next = self.after(from).map(|_| Destination::Next).into_iter();
        let others = self
            .steps
            .iter()
            .filter(|step| step.id != from)
            .map(|step| Destination::Step(step.id.clone()));
        match kind {
            RouteKind::Finished | RouteKind::Merge(MergeOutcome::Merged) => next
                .chain(others)
                .chain([
                    Destination::Approval,
                    Destination::Complete,
                    Destination::Human,
                    Destination::Coordinator,
                ])
                .collect(),
            RouteKind::Unable => [Destination::Human, Destination::Coordinator]
                .into_iter()
                .chain(others)
                .collect(),
            // An unsuccessful or uncertain merge never completes the Task.
            RouteKind::Merge(outcome) => (outcome == MergeOutcome::Pending)
                .then_some(Destination::Wait)
                .into_iter()
                .chain(others)
                .chain([Destination::Human, Destination::Coordinator])
                .collect(),
        }
    }

    /// A route that leads nowhere: `next` from the last Step, a deleted
    /// Step, or back to itself.
    pub fn broken(&self, from: &str, route: &Destination) -> Option<&'static str> {
        match route {
            Destination::Next if self.after(from).is_none() => {
                Some("points to the next agent, but none follows it")
            }
            Destination::Step(id) if id == from => Some("points back to the same agent"),
            Destination::Step(id) if self.step(id).is_none() => Some("points to a deleted agent"),
            _ => None,
        }
    }

    /// Every problem, in the outline's order: an empty list may be saved
    /// and run.
    pub fn problems(&self, catalog: &Catalog) -> Vec<Problem> {
        let mut problems = Vec::new();
        let mut add = |step: Option<&Step>, message: String| {
            problems.push(Problem {
                step: step.map(|step| step.id.clone()),
                message,
                approval: false,
            })
        };
        if self.name.trim().is_empty() {
            add(None, "Name the workflow.".into());
        }
        if self.steps.is_empty() {
            add(None, "Add at least one agent.".into());
        }
        for step in &self.steps {
            let who = if step.name.trim().is_empty() {
                add(Some(step), "Name this agent.".into());
                "An unnamed agent".to_string()
            } else {
                step.name.clone()
            };
            if step.instructions.trim().is_empty() {
                add(Some(step), format!("{who} has no instructions."));
            }
            let provider = step.agent.provider;
            if let Some(model) = &step.agent.model {
                if catalog.models(provider).is_some() {
                    match catalog.model(provider, model) {
                        None => add(
                            Some(step),
                            format!(
                                "{who}: {model} is not available from {provider}. Choose a model."
                            ),
                        ),
                        Some(info) => {
                            if let Some(effort) = &step.agent.effort {
                                if !info.efforts.iter().any(|each| each == effort) {
                                    add(
                                        Some(step),
                                        format!("{who}: {model} does not support {effort} effort."),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            if let Some(installed) = catalog.skills(provider) {
                for skill in &step.skills {
                    if !installed.iter().any(|each| each == skill) {
                        add(
                            Some(step),
                            format!("{who}: the skill {skill} is not installed for {provider}."),
                        );
                    }
                }
            }
            for (kind, route) in step.routes() {
                if let Some(what) = self.broken(&step.id, route) {
                    add(Some(step), format!("{who} \u{b7} {} {what}.", kind.label()));
                }
                let merge = |outcome| kind == RouteKind::Merge(outcome);
                if *route == Destination::Complete
                    && matches!(kind, RouteKind::Merge(_))
                    && !merge(MergeOutcome::Merged)
                {
                    add(
                        Some(step),
                        format!(
                            "{who} \u{b7} {} cannot complete the task: only a confirmed merge can.",
                            kind.label()
                        ),
                    );
                }
                if *route == Destination::Wait && !merge(MergeOutcome::Pending) {
                    add(
                        Some(step),
                        format!(
                            "{who} \u{b7} {} cannot keep waiting: only Checks pending waits.",
                            kind.label()
                        ),
                    );
                }
            }
        }
        for message in self.start.problems() {
            add(None, message);
        }
        problems.extend(self.approval_problems());
        if !self.steps.is_empty() && self.completion_paths().is_empty() {
            problems.push(Problem {
                step: None,
                message: "No route ends in Completed. Route an outcome to Completed, or set \
                          After approval to mark the task complete."
                    .into(),
                approval: false,
            });
        }
        problems
    }

    /// What is wrong with the approval settings: agent approval needs a
    /// deciding Step that exists and written instructions; what follows
    /// approval and where rework goes must lead somewhere.
    pub fn approval_problems(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        let mut add = |message: &str| {
            problems.push(Problem {
                step: None,
                message: message.into(),
                approval: true,
            })
        };
        let approval = &self.approval;
        if approval.by == Approver::Agent {
            match approval.step.as_deref() {
                None => add("Agent approval: choose the agent that decides."),
                Some(id) if self.step(id).is_none() => {
                    add("Agent approval: the deciding agent was deleted. Choose another.")
                }
                Some(_) => {}
            }
            if approval.instructions.trim().is_empty() {
                add(
                    "Agent approval: write the decision instructions (what to verify, what \
                     evidence to return, when it may approve without you).",
                );
            }
        }
        match &approval.after {
            Destination::Complete => {}
            Destination::Step(id) if self.step(id).is_some() => {}
            Destination::Step(_) => add("After approval runs a deleted agent. Choose another."),
            _ => add("After approval: choose Completed or an agent."),
        }
        match &approval.rework_to {
            Destination::Coordinator => {}
            Destination::Step(id) if self.step(id).is_some() => {}
            Destination::Step(_) => add("Rework goes to a deleted agent. Choose another."),
            _ => add("Rework goes to: choose an agent or the Coordinator."),
        }
        problems
    }

    /// Some route leads to Ready to approve.
    pub fn reaches_approval(&self) -> bool {
        self.steps.iter().any(|step| {
            step.routes()
                .iter()
                .any(|(_, route)| **route == Destination::Approval)
        })
    }

    /// The routes that end in Completed, said as the operator reads them.
    pub fn completion_paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        if self.reaches_approval() && self.approval.after == Destination::Complete {
            paths.push(match self.approval.by {
                Approver::Human => "Your approval marks the task complete".to_string(),
                Approver::Agent => "Approval by the deciding agent marks the task complete".into(),
            });
        }
        for step in &self.steps {
            for (kind, route) in step.routes() {
                if *route != Destination::Complete {
                    continue;
                }
                paths.push(match kind {
                    RouteKind::Finished => format!("{} reports finished", step.name),
                    RouteKind::Unable => format!("{} reports it could not finish", step.name),
                    RouteKind::Merge(outcome) => {
                        format!("{} reports {}", step.name, outcome.label().to_lowercase())
                    }
                });
            }
        }
        paths
    }
}

/// Where a route leads, resolved against a Definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved<'a> {
    Step(&'a Step),
    Approval,
    Complete,
    Human,
    Coordinator,
    /// Stay with the Step that reported.
    Wait,
    /// Nowhere: the Step it named is gone, or none follows.
    Broken,
}
