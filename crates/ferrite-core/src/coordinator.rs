//! Coordinator Threads (ADR 0028), after herdr-projects: one Thread per
//! Project that never does the work itself. It proposes tasks and, on the
//! operator's go-ahead, starts a worker Thread per task, each in a new
//! worktree on its own branch, briefed with the Project's goal, standing
//! instructions and memory. What a worker reports under `## Remember`
//! joins the memory for the next one; its PR's failing checks and review
//! remarks go back to it when the Project follows PRs.
//!
//! This is the bookkeeping and the words, beside the Store and never in a
//! repo: `coordinators.json`, one memory file per Project, the briefs, the
//! lessons read out of an answer, the order a coordinator's Group is read
//! in, and the PR feedback a worker is sent. The Server acts on them.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::workspace::registry::ProjectId;
use crate::workspace::{CheckState, PrFeedback};
use crate::{Attention, ThreadId};

const SCHEMA_VERSION: u32 = 1;
const FILE_NAME: &str = "coordinators.json";

/// One Project's coordination: its coordinator Thread, the brief every
/// worker gets, and the workers it started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coordination {
    pub project: ProjectId,
    pub coordinator: ThreadId,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub goal: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub instructions: String,
    /// Send failing checks and review remarks on a worker's PR back to it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub follow_prs: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workers: Vec<Worker>,
}

/// A change to a Project's brief: each field given replaces its own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BriefChange {
    pub goal: Option<String>,
    pub instructions: Option<String>,
    pub follow_prs: Option<bool>,
}

impl BriefChange {
    pub fn apply(self, coordination: &mut Coordination) {
        if let Some(goal) = self.goal {
            coordination.goal = goal;
        }
        if let Some(instructions) = self.instructions {
            coordination.instructions = instructions;
        }
        if let Some(follow_prs) = self.follow_prs {
            coordination.follow_prs = follow_prs;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worker {
    pub thread: ThreadId,
    pub task: String,
    /// The PR feedback already sent to it, by `Feedback::key`: each is
    /// sent once.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub sent: BTreeSet<String>,
}

#[derive(Serialize, Deserialize)]
struct Persisted {
    version: u32,
    projects: Vec<Coordination>,
}

/// Every Project's coordination, persisted beside the Store. Owned by the
/// Server; nothing else writes it.
pub struct Coordinators {
    dir: PathBuf,
    projects: Vec<Coordination>,
}

impl Coordinators {
    /// None yet, kept in `dir`.
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            projects: Vec::new(),
        }
    }

    pub fn load(dir: &Path) -> io::Result<Self> {
        let projects = match fs::read(dir.join(FILE_NAME)) {
            Ok(bytes) => {
                let persisted: Persisted = serde_json::from_slice(&bytes)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                if persisted.version != SCHEMA_VERSION {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "coordinators version {} is not supported",
                            persisted.version
                        ),
                    ));
                }
                persisted.projects
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        Ok(Self {
            projects,
            ..Self::new(dir)
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = &Coordination> {
        self.projects.iter()
    }

    pub fn of_project(&self, project: ProjectId) -> Option<&Coordination> {
        self.projects.iter().find(|entry| entry.project == project)
    }

    pub fn led_by(&self, coordinator: ThreadId) -> Option<&Coordination> {
        self.projects
            .iter()
            .find(|entry| entry.coordinator == coordinator)
    }

    /// Record a Project's coordinator, replacing one it had.
    pub fn insert(&mut self, coordination: Coordination) -> io::Result<()> {
        self.edit(|projects| {
            projects.retain(|entry| entry.project != coordination.project);
            projects.push(coordination);
        })
    }

    /// Change what `coordinator` leads; `false` when it leads nothing.
    pub fn update(
        &mut self,
        coordinator: ThreadId,
        change: impl FnOnce(&mut Coordination),
    ) -> io::Result<bool> {
        let mut found = false;
        self.edit(|projects| {
            if let Some(entry) = projects
                .iter_mut()
                .find(|entry| entry.coordinator == coordinator)
            {
                change(entry);
                found = true;
            }
        })?;
        Ok(found)
    }

    /// Note feedback as sent to `worker`.
    pub fn mark_sent(&mut self, worker: ThreadId, keys: Vec<String>) -> io::Result<()> {
        self.edit(|projects| {
            if let Some(entry) = projects
                .iter_mut()
                .flat_map(|entry| entry.workers.iter_mut())
                .find(|entry| entry.thread == worker)
            {
                entry.sent.extend(keys);
            }
        })
    }

    /// Change the book and write it; on a failed write it stays as it was.
    fn edit(&mut self, change: impl FnOnce(&mut Vec<Coordination>)) -> io::Result<()> {
        let mut projects = self.projects.clone();
        change(&mut projects);
        let persisted = Persisted {
            version: SCHEMA_VERSION,
            projects,
        };
        let bytes = serde_json::to_vec_pretty(&persisted).map_err(io::Error::other)?;
        crate::store::replace_file(&self.dir.join(FILE_NAME), &bytes)?;
        self.projects = persisted.projects;
        Ok(())
    }

    /// The Project's memory: a Markdown file of lessons, one `- ` line
    /// each, that the operator may edit too.
    pub fn memory_path(&self, project: ProjectId) -> PathBuf {
        self.dir
            .join("memory")
            .join(format!("project-{}.md", project.get()))
    }

    /// What the memory says; empty when nothing was remembered yet.
    pub fn memory(&self, project: ProjectId) -> String {
        fs::read_to_string(self.memory_path(project)).unwrap_or_default()
    }

    /// Add the lessons the memory does not already hold; the ones added.
    pub fn remember(&self, project: ProjectId, lessons: &[String]) -> io::Result<Vec<String>> {
        let mut memory = self.memory(project);
        let known: BTreeSet<&str> = memory
            .lines()
            .filter_map(|line| line.trim().strip_prefix("- "))
            .map(str::trim)
            .collect();
        let mut seen = BTreeSet::new();
        let added: Vec<String> = lessons
            .iter()
            .filter(|lesson| !known.contains(lesson.as_str()) && seen.insert(lesson.as_str()))
            .cloned()
            .collect();
        if added.is_empty() {
            return Ok(added);
        }
        if !memory.is_empty() && !memory.ends_with('\n') {
            memory.push('\n');
        }
        for lesson in &added {
            memory.push_str(&format!("- {lesson}\n"));
        }
        let path = self.memory_path(project);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        crate::store::replace_file(&path, memory.as_bytes())?;
        Ok(added)
    }
}

// ------------------------------------------------------------ the order

/// Where a worker stands for the operator, most urgent first: the order a
/// coordinator's Group is read in. A worker that finished — seen or not —
/// is ready for review until it works again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Need {
    NeedsYou,
    ReadyForReview,
    Working,
}

impl Need {
    pub fn of(attention: Attention) -> Self {
        match attention {
            Attention::Blocked => Need::NeedsYou,
            Attention::Done | Attention::Idle => Need::ReadyForReview,
            Attention::Working => Need::Working,
        }
    }
}

// ------------------------------------------------------------ the words

/// What every brief says about the Project.
pub struct Brief<'a> {
    /// The Project's title.
    pub project: &'a str,
    pub goal: &'a str,
    pub instructions: &'a str,
    pub memory: &'a str,
}

impl Brief<'_> {
    fn sections(&self) -> String {
        let or = |text: &str, none: &str| {
            let text = text.trim();
            if text.is_empty() {
                none.to_string()
            } else {
                text.to_string()
            }
        };
        format!(
            "## Goal\n\n{}\n\n## Standing instructions\n\n{}\n\n## Memory\n\n{}",
            or(self.goal, "Not set yet."),
            or(self.instructions, "None."),
            or(self.memory, "Nothing yet."),
        )
    }
}

/// The coordinator's first prompt: its role, how it delegates through the
/// API, and the Project's brief.
pub fn coordinator_prompt(brief: &Brief) -> String {
    let next = if brief.goal.trim().is_empty() {
        "Ask the user what they want to get done."
    } else {
        "Propose the first tasks for this goal."
    };
    format!(
        "You are the coordinator of the project \"{project}\" in Ferrite. You never do \
         the work yourself: do not edit files, run builds or tests, or commit. You plan \
         with the user and delegate every task to a worker Thread, so you are always \
         free to answer them.\n\
         \n\
         How you work (the ferrite skill, `\"$FERRITE_BIN\" --skill`, has the details \
         and the safety rules):\n\
         \n\
         1. Agree the project's goal and standing instructions with the user, and record \
         them: `\"$FERRITE_BIN\" coordinator update --current --goal \"…\" --instructions \
         \"…\"`. Every worker is briefed with them.\n\
         2. Propose the work as a short numbered list of tasks, each one agent's work on \
         one branch. Ask the user to go ahead with your question tool when you have one; \
         otherwise end your turn with the proposal and wait for their answer.\n\
         3. Only on their go-ahead, start one worker per agreed task: `\"$FERRITE_BIN\" \
         worker start --current \"<the task, complete on its own>\" --title \"<a few \
         words>\"`. Each works in its own new worktree and branch.\n\
         4. Follow them with `\"$FERRITE_BIN\" coordinator get --current` (what needs the \
         user first, then what is ready for review, then what is working), `agent wait` \
         and `agent read`, and report to the user. Never answer a worker's Decision \
         yourself.\n\
         \n\
         {sections}\n\
         \n\
         {next}",
        project = brief.project,
        sections = brief.sections(),
    )
}

/// What a running coordinator is told when the operator changes the
/// Project's brief under it.
pub fn brief_changed_prompt(brief: &Brief) -> String {
    format!(
        "The user changed the project's brief. Plan with it from now on; every worker \
         you start is briefed with it.\n\n{}",
        brief.sections()
    )
}

/// A worker's first prompt: the Project's brief and its one task. Where
/// the Project follows PRs, the worker opens one for its branch, so its
/// checks and reviews have somewhere to come from.
pub fn worker_prompt(brief: &Brief, coordinator: ThreadId, task: &str, follow_prs: bool) -> String {
    let deliver = if follow_prs {
        " When your work is ready, push your branch and open a pull request for it \
         (`gh pr create`): its failing checks and review comments will be sent back to you."
    } else {
        ""
    };
    format!(
        "You are a worker in the project \"{project}\" in Ferrite, started by its \
         coordinator (Thread {coordinator}). Work only on your task, in this worktree, on \
         its own branch, and commit your work there.{deliver}\n\
         \n\
         {sections}\n\
         \n\
         ## Your task\n\
         \n\
         {task}\n\
         \n\
         When you are done, end your last message with a short report for the \
         coordinator. Put what the next worker on this project should know under a \
         `## Remember` heading, one lesson per bullet: Ferrite adds them to the \
         project's memory.",
        project = brief.project,
        sections = brief.sections(),
        task = task.trim(),
    )
}

/// The lessons an answer lists under a `## Remember` heading (any level),
/// one per bullet, in order. The section ends at the next heading, or at
/// a prompt or tool row of `transcript::plain_text`.
pub fn remembered(answer: &str) -> Vec<String> {
    let mut lessons = Vec::new();
    let mut inside = false;
    for line in answer.lines().map(str::trim) {
        if let Some(heading) = line.strip_prefix('#') {
            inside = heading
                .trim_start_matches('#')
                .trim()
                .eq_ignore_ascii_case("remember");
        } else if line.starts_with("> ") || line.starts_with("● ") {
            inside = false;
        } else if inside {
            let item = ["• ", "- ", "* "]
                .iter()
                .find_map(|bullet| line.strip_prefix(bullet))
                .map(str::trim)
                .filter(|item| !item.is_empty());
            lessons.extend(item.map(String::from));
        }
    }
    lessons
}

// ------------------------------------------------------- PR feedback

/// One thing a worker's PR asks of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feedback {
    /// Names it across readings, so it is sent once. A rerun check's new
    /// run is new feedback.
    pub key: String,
    pub text: String,
}

/// What an open PR asks of its worker: each failing check, then each
/// review or comment with words in it, short of an approval and of what
/// the PR's own author wrote — the worker's replies, through the
/// operator's `gh`, which would otherwise start it again on its own words.
/// A merged or closed PR asks nothing.
pub fn feedback(pr: &PrFeedback) -> Vec<Feedback> {
    if pr.pr.state != crate::workspace::PrState::Open {
        return Vec::new();
    }
    let checks = pr
        .pr
        .runs
        .iter()
        .filter(|run| run.state == CheckState::Failing)
        .map(|run| Feedback {
            key: format!(
                "check:{}:{}",
                run.name,
                run.url.as_deref().unwrap_or(&run.detail)
            ),
            text: match &run.url {
                Some(url) => format!("Check `{}` failed ({}): {url}", run.name, run.detail),
                None => format!("Check `{}` failed ({}).", run.name, run.detail),
            },
        });
    let remarks = pr
        .remarks
        .iter()
        .filter(|remark| {
            !remark.approves && remark.author != pr.author && !remark.body.trim().is_empty()
        })
        .map(|remark| Feedback {
            key: format!("remark:{}", remark.id),
            text: format!("@{} wrote: {}", remark.author, remark.body.trim()),
        });
    checks.chain(remarks).collect()
}

/// The prompt that sends a PR's new feedback back to its worker.
pub fn feedback_prompt(number: u64, items: &[&Feedback]) -> String {
    let list: Vec<String> = items
        .iter()
        .map(|item| format!("- {}", item.text))
        .collect();
    format!(
        "Your pull request #{number} has new feedback. Fix what it asks for on your \
         branch, push, and report back as before.\n\n{}",
        list.join("\n")
    )
}

#[cfg(test)]
mod tests;
