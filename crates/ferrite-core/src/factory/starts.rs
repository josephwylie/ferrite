//! How a Workflow's Tasks start (ADR 0037, #225): manually, on a
//! schedule, from a GitHub issue or pull request, or from the operator's
//! own script. The settings are part of a Workflow's Definition, so they
//! are saved in its versions; the switch that lets them start Tasks is
//! the Workflow's [`Automation`], apart from saving.
//!
//! A schedule is evaluated against host-local time. GitHub is read with
//! the operator's `gh` login about once a minute, by the Server, off its
//! pump; the Factory only says when to read and what a read found.

use std::collections::BTreeSet;

use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Timelike, Weekday};
use serde::{Deserialize, Serialize};

use super::{
    Actor, Answer, Command, Effect, Factory, Refusal, RefusalCode, State, Status, WorkflowId,
};

/// How often an enabled GitHub start is read.
pub const POLL_MS: i64 = 60_000;

/// A run due longer ago than this was missed (the computer slept, or
/// Ferrite was not running), not merely reached by this pass.
pub const MISSED_AFTER_MS: i64 = 120_000;

/// The "Tasks start" picker's five sources.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StartKind {
    #[default]
    Manual,
    Schedule,
    GithubIssue,
    GithubPr,
    External,
}

impl StartKind {
    pub const ALL: [StartKind; 5] = [
        StartKind::Manual,
        StartKind::Schedule,
        StartKind::GithubIssue,
        StartKind::GithubPr,
        StartKind::External,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StartKind::Manual => "Manually",
            StartKind::Schedule => "On a schedule",
            StartKind::GithubIssue => "From a GitHub issue",
            StartKind::GithubPr => "From a GitHub pull request",
            StartKind::External => "From your own script",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            StartKind::Manual => "You or the coordinator",
            StartKind::Schedule => "At set times",
            StartKind::GithubIssue => "One task per issue",
            StartKind::GithubPr => "One task per PR",
            StartKind::External => "Advanced",
        }
    }

    pub fn is_github(self) -> bool {
        matches!(self, StartKind::GithubIssue | StartKind::GithubPr)
    }

    /// Starts Tasks without the operator, so it has a switch.
    pub fn automatic(self) -> bool {
        self != StartKind::Manual
    }
}

/// How a schedule repeats.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Recurrence {
    #[default]
    Daily,
    /// Monday to Friday.
    Weekdays,
    /// Once a week, on [`Schedule::weekday`].
    Weekly,
    /// Every [`Schedule::interval_hours`] hours, on the hour.
    Interval,
}

impl Recurrence {
    pub const ALL: [Recurrence; 4] = [
        Recurrence::Daily,
        Recurrence::Weekdays,
        Recurrence::Weekly,
        Recurrence::Interval,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Recurrence::Daily => "Every day",
            Recurrence::Weekdays => "Weekdays",
            Recurrence::Weekly => "Once a week",
            Recurrence::Interval => "Every few hours",
        }
    }
}

/// What happens to a run missed while the computer slept.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Missed {
    #[default]
    Skip,
    RunOnce,
}

/// What happens to a run due while the last one still runs.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Overlap {
    #[default]
    Skip,
    Queue,
}

/// The hours an interval schedule offers.
pub const INTERVAL_HOURS: [u32; 7] = [1, 2, 3, 4, 6, 8, 12];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Schedule {
    pub recurrence: Recurrence,
    /// `HH:MM`, host-local; none until chosen. Not used by an interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// 0 is Sunday … 6 Saturday, for a weekly schedule.
    pub weekday: u8,
    pub interval_hours: u32,
    /// What each run's Task is told.
    pub instructions: String,
    pub missed: Missed,
    pub overlap: Overlap,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            recurrence: Recurrence::Daily,
            time: None,
            weekday: 1,
            interval_hours: 4,
            instructions: String::new(),
            missed: Missed::Skip,
            overlap: Overlap::Skip,
        }
    }
}

pub const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// `09:30` as a time of day.
fn time_of(text: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(text.trim(), "%H:%M").ok()
}

impl Schedule {
    /// The first occurrence strictly after `now`, in `now`'s zone; none
    /// while a time is still to be chosen.
    pub fn next_after<Tz: TimeZone>(&self, now: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        let zone = now.timezone();
        if self.recurrence == Recurrence::Interval {
            let every = self.interval_hours.max(1);
            let mut at = now.with_minute(0)?.with_second(0)?.with_nanosecond(0)?;
            loop {
                at += Duration::hours(1);
                if at.hour() % every == 0 && at > *now {
                    return Some(at);
                }
            }
        }
        let time = time_of(self.time.as_deref()?)?;
        let today = now.date_naive();
        (0..=8).find_map(|days| {
            let date = today + Duration::days(days);
            let at = zone.from_local_datetime(&date.and_time(time)).earliest()?;
            if at <= *now {
                return None;
            }
            let weekday = at.weekday();
            let fits = match self.recurrence {
                Recurrence::Daily | Recurrence::Interval => true,
                Recurrence::Weekdays => !matches!(weekday, Weekday::Sat | Weekday::Sun),
                Recurrence::Weekly => weekday.num_days_from_sunday() == u32::from(self.weekday),
            };
            fits.then_some(at)
        })
    }

    /// The next occurrence after `now_ms`, host-local.
    pub fn next_after_ms(&self, now_ms: i64) -> Option<i64> {
        let now = chrono::Local.timestamp_millis_opt(now_ms).single()?;
        self.next_after(&now).map(|at| at.timestamp_millis())
    }

    /// `Weekdays at 09:00`, as the settings say it.
    pub fn describe(&self) -> String {
        let time = self.time.as_deref().unwrap_or("\u{2014}");
        match self.recurrence {
            Recurrence::Daily => format!("Every day at {time}"),
            Recurrence::Weekdays => format!("Weekdays at {time}"),
            Recurrence::Weekly => format!(
                "Every {} at {time}",
                WEEKDAYS[usize::from(self.weekday.min(6))]
            ),
            Recurrence::Interval => {
                let hours = self.interval_hours.max(1);
                let plural = if hours == 1 { "" } else { "s" };
                format!("Every {hours} hour{plural}, on the hour")
            }
        }
    }
}

/// The host's time zone as a schedule's settings name it:
/// `Australia/Adelaide (UTC+10:30)`.
pub fn zone_label(now_ms: i64) -> String {
    let offset = chrono::Local
        .timestamp_millis_opt(now_ms)
        .single()
        .map(|now| now.format("UTC%:z").to_string())
        .unwrap_or_else(|| "UTC".into());
    match iana_time_zone::get_timezone() {
        Ok(name) => format!("{name} ({offset})"),
        Err(_) => offset,
    }
}

/// A host-local time as the settings say it: `Thu 12 Nov, 09:30`.
pub fn when_label(at_ms: i64) -> String {
    chrono::Local
        .timestamp_millis_opt(at_ms)
        .single()
        .map(|at| at.format("%a %-d %b, %H:%M").to_string())
        .unwrap_or_default()
}

/// What a GitHub start waits for.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// Is open and has [`GitHubSource::label`].
    #[default]
    Labelled,
    /// Is opened.
    Opened,
    /// Is a pull request that requests the operator's review.
    ReviewRequested,
}

impl Trigger {
    pub fn label(self) -> &'static str {
        match self {
            Trigger::Labelled => "Has a label",
            Trigger::Opened => "Is opened",
            Trigger::ReviewRequested => "Requests my review",
        }
    }

    /// What a start of `kind` offers.
    pub fn offered(kind: StartKind) -> &'static [Trigger] {
        if kind == StartKind::GithubPr {
            &[Trigger::Labelled, Trigger::Opened, Trigger::ReviewRequested]
        } else {
            &[Trigger::Labelled, Trigger::Opened]
        }
    }
}

/// A premade GitHub start: a repository and a trigger, both picked from
/// what the operator's `gh` login lists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct GitHubSource {
    /// `owner/name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    pub trigger: Trigger,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Added after the issue's or PR's own text in every Task.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub extra_instructions: String,
}

impl GitHubSource {
    /// The one sentence that says exactly what is checked.
    pub fn describe(&self, kind: StartKind) -> String {
        let thing = if kind == StartKind::GithubPr {
            "pull requests"
        } else {
            "issues"
        };
        let which = match self.trigger {
            Trigger::Labelled => format!(
                "open {thing} labelled {}",
                self.label.as_deref().unwrap_or("\u{2026}")
            ),
            Trigger::ReviewRequested => format!("open {thing} that request your review"),
            Trigger::Opened => format!("newly opened {thing}"),
        };
        format!(
            "About once a minute, Ferrite checks {} for {which}, using this computer's GitHub \
             CLI login. Each one starts one task.",
            self.repository.as_deref().unwrap_or("the repository")
        )
    }
}

/// How a Workflow's Tasks start. Every source's settings are kept, so
/// switching between them loses nothing typed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct Start {
    #[serde(rename = "type")]
    pub kind: StartKind,
    pub schedule: Schedule,
    pub github: GitHubSource,
}

impl Start {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// What keeps it from being saved, as the settings say each.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        match self.kind {
            StartKind::Manual | StartKind::External => {}
            StartKind::Schedule => {
                let schedule = &self.schedule;
                if schedule.recurrence != Recurrence::Interval
                    && schedule.time.as_deref().and_then(time_of).is_none()
                {
                    problems.push("Choose the time the schedule runs at.".into());
                }
                if schedule.instructions.trim().is_empty() {
                    problems.push("Write the instructions each scheduled task starts with.".into());
                }
            }
            StartKind::GithubIssue | StartKind::GithubPr => {
                let github = &self.github;
                if github
                    .repository
                    .as_deref()
                    .is_none_or(|repo| repo.trim().is_empty())
                {
                    problems.push("Choose the repository to check.".into());
                }
                if github.trigger == Trigger::Labelled
                    && github
                        .label
                        .as_deref()
                        .is_none_or(|label| label.trim().is_empty())
                {
                    problems.push("Choose the label to check for.".into());
                }
                if !Trigger::offered(self.kind).contains(&github.trigger) {
                    problems.push("Only a pull request can request your review.".into());
                }
            }
        }
        problems
    }
}

/// The switch on a Workflow's automatic starts. Saving never turns it on.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Switch {
    #[default]
    Off,
    On,
    Paused,
}

impl Switch {
    pub fn word(self) -> &'static str {
        match self {
            Switch::Off => "Off",
            Switch::On => "On",
            Switch::Paused => "Paused",
        }
    }
}

/// What the operator asks of the switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Turn {
    On,
    Pause,
    Resume,
    Off,
}

/// A Workflow's automatic starts: the switch and what they remember.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Automation {
    pub switch: Switch,
    /// The last GitHub read failed, and why: retried at the next check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// What already matched when a GitHub start was turned on, held until
    /// the operator says whether to take it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub backlog: Vec<GitHubItem>,
    /// A GitHub start has had its first read since it was turned on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub primed: bool,
    /// Every issue or PR link that has started a Task, or was passed over.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub seen: BTreeSet<String>,
    /// When the schedule next runs, wall-clock milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_ms: Option<i64>,
    /// A scheduled run waits for the last one to finish.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub waiting: bool,
}

impl Automation {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// One open issue or pull request, as a read found it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubItem {
    pub number: u64,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    /// Its page: what makes it the same one when seen again.
    pub url: String,
}

/// What one read of a GitHub start found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHubRead {
    /// Nothing changed since the read `etag` answered.
    Unchanged,
    /// Everything that matches now, and the tag a next read sends.
    Matches {
        etag: Option<String>,
        items: Vec<GitHubItem>,
    },
}

/// One read for the Server to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubQuery {
    pub kind: StartKind,
    pub source: GitHubSource,
    /// The tag of the last read that found something.
    pub etag: Option<String>,
}

/// What started a Task without the operator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// A run of `workflow`'s schedule.
    Schedule { workflow: String },
    Issue {
        repository: String,
        number: u64,
        url: String,
    },
    /// The pull request is the Task's PR.
    PullRequest {
        repository: String,
        number: u64,
        url: String,
    },
    /// The operator's own script, by the identity it sent.
    Script { identity: String },
}

impl Source {
    /// The link back to where it came from, if it has one.
    pub fn link(&self) -> Option<&str> {
        match self {
            Source::Issue { url, .. } | Source::PullRequest { url, .. } => Some(url),
            Source::Schedule { .. } | Source::Script { .. } => None,
        }
    }

    /// `acme/app#7`, `Schedule`, `Script JIRA-1`: what a card says.
    pub fn label(&self) -> String {
        match self {
            Source::Schedule { .. } => "Schedule".into(),
            Source::Issue {
                repository, number, ..
            }
            | Source::PullRequest {
                repository, number, ..
            } => format!("{repository}#{number}"),
            Source::Script { identity } => format!("Script {identity}"),
        }
    }
}

impl super::Task {
    /// The pull request it works on: the one that started it.
    pub fn pull_request(&self) -> Option<&str> {
        match &self.source {
            Some(Source::PullRequest { url, .. }) => Some(url),
            _ => None,
        }
    }

    fn unfinished(&self) -> bool {
        !matches!(self.status, Status::Completed | Status::Stopped)
    }
}

impl Automation {
    /// Off, forgetting what only a live start needs; what was seen stays.
    pub(super) fn turn_off(&mut self) {
        *self = Automation {
            seen: std::mem::take(&mut self.seen),
            ..Automation::default()
        };
    }
}

/// One GitHub start's reads while the Server runs.
#[derive(Debug, Clone, Default)]
pub(super) struct Poll {
    /// The version its reads are of: another drops what is remembered.
    version: u32,
    /// A read is out.
    reading: bool,
    /// When the last read was asked for.
    asked_ms: Option<i64>,
    etag: Option<String>,
}

impl Factory {
    /// What a command means for the reads kept in memory.
    pub(super) fn heard(&mut self, command: &Command) {
        match command {
            Command::GitHubRead {
                workflow,
                version,
                result,
            } => {
                if let Some(poll) = self.polls.get_mut(workflow) {
                    if poll.version == *version {
                        poll.reading = false;
                        if let Ok(GitHubRead::Matches { etag, .. }) = result {
                            poll.etag = etag.clone();
                        }
                    }
                }
            }
            Command::Automation {
                workflow,
                turn: Turn::On | Turn::Off,
            } => {
                // A start turned on is read afresh, in full.
                if let Some(poll) = self.polls.get_mut(workflow) {
                    poll.etag = None;
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
        let live: Vec<(WorkflowId, u32, Start)> = self
            .workflows()
            .filter(|workflow| {
                workflow.automation.switch == Switch::On && workflow.automation.backlog.is_empty()
            })
            .filter_map(|workflow| {
                let latest = workflow.latest()?;
                let start = &latest.definition.start;
                start
                    .kind
                    .is_github()
                    .then(|| (workflow.id.clone(), latest.version, start.clone()))
            })
            .collect();
        for (workflow, version, start) in live {
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
            let etag = poll.etag.clone();
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
            crate::log!("factory: {} did not start a Task: {refusal}", item.url);
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
            Ok(GitHubRead::Matches { items, .. }) => items,
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
        let request = format!("script:{workflow}:{identity}");
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
            if automation.due_ms.is_none() {
                automation.due_ms = schedule.next_after_ms(now_ms);
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
                    crate::log!("factory: {workflow}'s schedule did not start a Task: {refusal}");
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests;
