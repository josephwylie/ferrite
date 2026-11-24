//! How a Workflow's Tasks start (ADR 0037, #225), as its Definition keeps
//! it and a window sets it: manually, on a schedule, from a GitHub issue
//! or pull request, or from the operator's own script, and the switch
//! that lets them start ([`Automation`]). When a start is due, and the
//! GitHub reads behind it, are the Factory's (`factory::starts`).

use std::collections::BTreeSet;

use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Timelike, Weekday};
use serde::{Deserialize, Serialize};

use super::Status;

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
    /// The tag of the last GitHub read taken, which the next read is
    /// conditional on: kept with what that read added to `seen`, so a read
    /// dropped or not written leaves the tag before it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
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

    pub(crate) fn unfinished(&self) -> bool {
        !matches!(self.status, Status::Completed | Status::Stopped)
    }
}

impl Automation {
    /// Off, forgetting what only a live start needs; what was seen stays.
    pub(crate) fn turn_off(&mut self) {
        *self = Automation {
            seen: std::mem::take(&mut self.seen),
            ..Automation::default()
        };
    }
}
