//! How a Workflow's Tasks start (ADR 0037, #225): manually, on a
//! schedule, from a GitHub issue or pull request, or from the operator's
//! own script. The settings are part of a Workflow's Definition, so they
//! are saved in its versions; the switch that lets them start Tasks is
//! the Workflow's [`Automation`], apart from saving.
//!
//! A schedule is evaluated against host-local time. GitHub is read with
//! the operator's `gh` login about once a minute, by the Server, off its
//! pump; the Factory only says when to read and what a read found.

use super::{Answer, Command, Effect, Factory, Refusal, RefusalCode, State};
use ferrite_core::factory_board::{
    Actor, GitHubItem, GitHubQuery, GitHubRead, GitHubSource, Missed, Overlap, Schedule, Source,
    Start, StartKind, Switch, Turn, WorkflowId,
};

/// How often an enabled GitHub start is read.
pub const POLL_MS: i64 = 60_000;

/// A run due longer ago than this was missed (the computer slept, or
/// Ferrite was not running), not merely reached by this pass.
pub const MISSED_AFTER_MS: i64 = 120_000;

/// One GitHub start's reads while the Server runs.
#[derive(Debug, Clone, Default)]
pub(super) struct Poll {
    /// The version its reads are of: another drops what is remembered.
    version: u32,
    /// A read is out.
    reading: bool,
    /// When the last read was asked for.
    asked_ms: Option<i64>,
}

impl Factory {
    /// What a command means for the reads kept in memory.
    pub(super) fn heard(&mut self, command: &Command) {
        match command {
            Command::GitHubRead {
                workflow, version, ..
            } => {
                if let Some(poll) = self.polls.get_mut(workflow) {
                    if poll.version == *version {
                        poll.reading = false;
                    }
                }
            }
            Command::Automation {
                workflow,
                turn: Turn::On | Turn::Off,
            } => {
                // A start turned on is read afresh (its tag goes with the
                // rest of its Automation).
                if let Some(poll) = self.polls.get_mut(workflow) {
                    poll.asked_ms = None;
                }
            }
            _ => {}
        }
    }

    /// Ask for a read of each live GitHub start that is due one: about
    /// once a minute, one at a time, and none while a backlog waits for
    /// the operator.
    pub(super) fn poll(&mut self, now_ms: i64) {
        let live: Vec<(WorkflowId, u32, Start, Option<String>)> = self
            .workflows()
            .filter(|workflow| {
                workflow.automation.switch == Switch::On && workflow.automation.backlog.is_empty()
            })
            .filter_map(|workflow| {
                let latest = workflow.latest()?;
                let start = &latest.definition.start;
                start.kind.is_github().then(|| {
                    (
                        workflow.id.clone(),
                        latest.version,
                        start.clone(),
                        workflow.automation.etag.clone(),
                    )
                })
            })
            .collect();
        for (workflow, version, start, etag) in live {
            let poll = self.polls.entry(workflow.clone()).or_default();
            if poll.version != version {
                *poll = Poll {
                    version,
                    ..Poll::default()
                };
            }
            if poll.reading || poll.asked_ms.is_some_and(|asked| now_ms - asked < POLL_MS) {
                continue;
            }
            poll.reading = true;
            poll.asked_ms = Some(now_ms);
            self.pending.push(Effect::ReadGitHub {
                workflow,
                version,
                query: GitHubQuery {
                    kind: start.kind,
                    source: start.github,
                    etag,
                },
            });
        }
    }
}

/// What makes a script's repeated submission the Task it already started.
pub(super) fn script_request(workflow: &str, identity: &str) -> String {
    format!("script:{workflow}:{}", identity.trim())
}

fn unknown_workflow() -> Refusal {
    Refusal::new(RefusalCode::UnknownWorkflow, "no Workflow has that id")
}

/// A GitHub issue's or PR's Task: its title and description, the extra
/// instructions, and the link back; a PR is the one to work on.
fn github_instructions(kind: StartKind, source: &GitHubSource, item: &GitHubItem) -> String {
    let mut text = item.title.trim().to_string();
    if !item.body.trim().is_empty() {
        text.push_str("\n\n");
        text.push_str(item.body.trim());
    }
    if !source.extra_instructions.trim().is_empty() {
        text.push_str("\n\n");
        text.push_str(source.extra_instructions.trim());
    }
    if kind == StartKind::GithubPr {
        text.push_str(&format!(
            "\n\nPull request: {url}\nThis task works on that pull request: check out its \
             branch with `gh pr checkout {number}` in this worktree before changing anything, \
             and push your commits to it.",
            url = item.url,
            number = item.number,
        ));
    } else {
        text.push_str(&format!("\n\nGitHub issue: {}", item.url));
    }
    text
}

impl State {
    /// A Task on `workflow`'s latest version, started by `source`, not by
    /// the operator: `request` makes a repeat the same Task.
    fn start_from(
        &mut self,
        workflow: &str,
        instructions: String,
        request: String,
        source: Source,
        now_ms: i64,
    ) -> Result<String, Refusal> {
        let made = self
            .workflows
            .iter()
            .find(|made| made.id == workflow)
            .ok_or_else(unknown_workflow)?;
        let agent = made
            .latest()
            .and_then(|latest| latest.definition.steps.first())
            .map(|step| step.agent.clone())
            .ok_or_else(|| Refusal::new(RefusalCode::Invalid, "the workflow has no agent"))?;
        // A pull request that starts a Task is its PR (#223).
        let pr = match &source {
            Source::PullRequest { url, .. } => Some(url.clone()),
            _ => None,
        };
        let project = made.project;
        let id = self.start(
            project,
            instructions,
            agent,
            Some(request),
            Some(workflow.to_string()),
            pr,
            now_ms,
        )?;
        let task = self.task_mut(&id)?;
        if task.source.is_none() {
            let said = match &source {
                Source::Schedule { .. } => "Started by the schedule".to_string(),
                Source::Issue { url, .. } => format!("Started from the issue {url}"),
                Source::PullRequest { url, .. } => format!("Started from the pull request {url}"),
                Source::Script { identity } => format!("Submitted by a script as {identity}"),
            };
            if let Some(entry) = task.activity.last_mut() {
                entry.by = Actor::Ferrite;
                entry.text = format!("{said}; {}", entry.text.to_lowercase());
            }
            task.source = Some(source);
        }
        Ok(id)
    }

    /// A GitHub item's Task, once.
    fn take_item(
        &mut self,
        workflow: &str,
        kind: StartKind,
        github: &GitHubSource,
        item: &GitHubItem,
        now_ms: i64,
    ) {
        let repository = github.repository.clone().unwrap_or_default();
        let source = if kind == StartKind::GithubPr {
            Source::PullRequest {
                repository,
                number: item.number,
                url: item.url.clone(),
            }
        } else {
            Source::Issue {
                repository,
                number: item.number,
                url: item.url.clone(),
            }
        };
        let instructions = github_instructions(kind, github, item);
        if let Err(refusal) =
            self.start_from(workflow, instructions, item.url.clone(), source, now_ms)
        {
            ferrite_core::log!("factory: {} did not start a Task: {refusal}", item.url);
        }
        if let Ok(made) = self.workflow_mut(workflow) {
            made.automation.seen.insert(item.url.clone());
        }
    }

    pub(super) fn edit_start(
        &mut self,
        workflow: WorkflowId,
        start: Start,
    ) -> Result<Answer, Refusal> {
        let draft = &mut self.workflow_mut(&workflow)?.draft;
        let changed = draft.start != start;
        draft.start = start;
        Ok(Answer::Changed(changed))
    }

    pub(super) fn automation(
        &mut self,
        workflow: WorkflowId,
        turn: Turn,
        now_ms: i64,
    ) -> Result<Answer, Refusal> {
        let made = self.workflow_mut(&workflow)?;
        let start = made.latest().map(|latest| latest.definition.start.clone());
        let automation = &mut made.automation;
        let before = automation.clone();
        let due = |start: &Option<Start>| {
            start
                .as_ref()
                .filter(|start| start.kind == StartKind::Schedule)
                .and_then(|start| start.schedule.next_after_ms(now_ms))
        };
        match turn {
            Turn::On | Turn::Resume if automation.switch == Switch::Paused => {
                automation.switch = Switch::On;
                automation.due_ms = due(&start);
            }
            Turn::On if automation.switch == Switch::Off => {
                if !start.as_ref().is_some_and(|start| start.kind.automatic()) {
                    return Err(Refusal::new(
                        RefusalCode::Invalid,
                        "save a version with an automatic start before turning \
                             automatic starts on",
                    ));
                }
                automation.turn_off();
                automation.switch = Switch::On;
                automation.due_ms = due(&start);
            }
            Turn::Pause if automation.switch == Switch::On => {
                automation.switch = Switch::Paused;
                automation.waiting = false;
            }
            Turn::Off => automation.turn_off(),
            _ => {}
        }
        Ok(Answer::Changed(*automation != before))
    }

    pub(super) fn backlog(
        &mut self,
        workflow: WorkflowId,
        take: bool,
        now_ms: i64,
    ) -> Result<Answer, Refusal> {
        let made = self.workflow_mut(&workflow)?;
        let backlog = std::mem::take(&mut made.automation.backlog);
        if backlog.is_empty() {
            return Ok(Answer::Changed(false));
        }
        let Some(start) = made.latest().map(|latest| latest.definition.start.clone()) else {
            return Ok(Answer::Changed(true));
        };
        for item in &backlog {
            if take {
                self.take_item(&workflow, start.kind, &start.github, item, now_ms);
            } else {
                self.workflow_mut(&workflow)?
                    .automation
                    .seen
                    .insert(item.url.clone());
            }
        }
        Ok(Answer::Changed(true))
    }

    pub(super) fn github_read(
        &mut self,
        workflow: WorkflowId,
        version: u32,
        result: Result<GitHubRead, String>,
        now_ms: i64,
    ) -> Result<Answer, Refusal> {
        let made = self.workflow_mut(&workflow)?;
        let Some(latest) = made.latest().filter(|latest| latest.version == version) else {
            return Ok(Answer::Changed(false));
        };
        let start = latest.definition.start.clone();
        let automation = &mut made.automation;
        if automation.switch != Switch::On || !start.kind.is_github() {
            return Ok(Answer::Changed(false));
        }
        let items = match result {
            Err(why) => {
                let changed = automation.error.as_ref() != Some(&why);
                automation.error = Some(why);
                return Ok(Answer::Changed(changed));
            }
            Ok(GitHubRead::Unchanged) => Vec::new(),
            Ok(GitHubRead::Matches { items, etag }) => {
                automation.etag = etag;
                items
            }
        };
        let mut changed = automation.error.take().is_some();
        let new: Vec<GitHubItem> = items
            .into_iter()
            .filter(|item| !automation.seen.contains(&item.url))
            .collect();
        if !automation.primed {
            automation.primed = true;
            automation.backlog = new;
            return Ok(Answer::Changed(true));
        }
        for item in &new {
            self.take_item(&workflow, start.kind, &start.github, item, now_ms);
            changed = true;
        }
        Ok(Answer::Changed(changed))
    }

    pub(super) fn submit(
        &mut self,
        workflow: WorkflowId,
        identity: String,
        instructions: String,
        now_ms: i64,
    ) -> Result<Answer, Refusal> {
        let identity = identity.trim().to_string();
        if identity.is_empty() {
            return Err(Refusal::new(
                RefusalCode::Invalid,
                "a submission needs an identity, so a repeat is the same Task",
            ));
        }
        if instructions.trim().is_empty() {
            return Err(Refusal::new(
                RefusalCode::Invalid,
                "a submission needs instructions",
            ));
        }
        let made = self
            .workflows
            .iter()
            .find(|made| made.id == workflow)
            .ok_or_else(unknown_workflow)?;
        let request = script_request(&workflow, &identity);
        if let Some(task) = self
            .tasks
            .iter()
            .find(|task| task.request.as_deref() == Some(request.as_str()))
        {
            return Ok(Answer::Duplicate(task.id.clone()));
        }
        let kind = made.latest().map(|latest| latest.definition.start.kind);
        if kind != Some(StartKind::External) {
            return Err(Refusal::new(
                RefusalCode::Invalid,
                format!(
                    "{} does not start from your own script: save a version that does",
                    made.draft.name
                ),
            ));
        }
        if made.automation.switch != Switch::On {
            return Err(Refusal::new(
                RefusalCode::NotNow,
                format!(
                    "automatic starts for {} are {}",
                    made.draft.name,
                    made.automation.switch.word().to_lowercase()
                ),
            ));
        }
        let id = self.start_from(
            &workflow,
            instructions,
            request,
            Source::Script { identity },
            now_ms,
        )?;
        Ok(Answer::Task(id))
    }

    /// Each live schedule: a run due now starts a Task, unless it was
    /// missed and is skipped, or the last run still goes and it is
    /// skipped or waits. Whether any changed.
    pub(super) fn step_starts(&mut self, now_ms: i64) -> bool {
        let mut changed = false;
        let live: Vec<(String, Schedule)> = self
            .workflows
            .iter()
            .filter(|made| made.automation.switch == Switch::On)
            .filter_map(|made| {
                let start = &made.latest()?.definition.start;
                (start.kind == StartKind::Schedule)
                    .then(|| (made.id.clone(), start.schedule.clone()))
            })
            .collect();
        for (workflow, schedule) in live {
            let mut running = self.tasks.iter().any(|task| {
                task.unfinished()
                    && task.source
                        == Some(Source::Schedule {
                            workflow: workflow.clone(),
                        })
            });
            let Ok(made) = self.workflow_mut(&workflow) else {
                continue;
            };
            let automation = &mut made.automation;
            let before = (automation.waiting, automation.due_ms);
            let mut runs = Vec::new();
            if automation.waiting && !running {
                automation.waiting = false;
                running = true;
                runs.push(now_ms);
            }
            // Never later than the next run after now: a due time made on
            // a clock that ran ahead and was put back is made again.
            let next = schedule.next_after_ms(now_ms);
            if automation.due_ms.is_none()
                || next.is_some_and(|next| automation.due_ms.is_some_and(|due| next < due))
            {
                automation.due_ms = next;
            }
            if let Some(due) = automation.due_ms.filter(|due| now_ms >= *due) {
                automation.due_ms = schedule.next_after_ms(now_ms);
                let missed = now_ms - due > MISSED_AFTER_MS;
                if !(missed && schedule.missed == Missed::Skip) {
                    match (running, schedule.overlap) {
                        (false, _) => runs.push(due),
                        (true, Overlap::Queue) => automation.waiting = true,
                        (true, Overlap::Skip) => {}
                    }
                }
            }
            changed |= before != (automation.waiting, automation.due_ms) || !runs.is_empty();
            for due in runs {
                let source = Source::Schedule {
                    workflow: workflow.clone(),
                };
                let request = format!("schedule:{workflow}:{due}");
                if let Err(refusal) = self.start_from(
                    &workflow,
                    schedule.instructions.clone(),
                    request,
                    source,
                    now_ms,
                ) {
                    ferrite_core::log!("factory: {workflow}'s schedule did not start a Task: {refusal}");
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests;
