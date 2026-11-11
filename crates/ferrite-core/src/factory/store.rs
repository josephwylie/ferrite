//! Where the Factory keeps itself (#226; ADR 0025, 0037). Each Project's
//! part is its own file beside the Store, and its Archive another, so one
//! Project's store failing leaves every other Project's Factory running,
//! and a write costs that one Project's board, never every Task the
//! Factory has held.
//!
//! Persist, then act. A command is applied to a copy of its Project's
//! part, written, and only then taken: a change that cannot be written is
//! not made. The step changes the parts in place and says whether it did;
//! its effects are held until that change is written, and a write that
//! fails is tried again on the next pass, so nothing it decided is lost.
//! An idle pass writes nothing and copies nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    starts, Answer, Board, Command, Coordinator, Effect, Refusal, RefusalCode, Seen, State, Status,
    Task, TaskId, Workflow, WorkflowId, DEFAULT_LIMIT, FILE_NAME, SCHEMA_VERSION,
};
use crate::workspace::registry::ProjectId;
use crate::ThreadId;

/// The directory beside the Store that holds each Project's part.
pub const DIR_NAME: &str = "factory";

/// One Project's archived Tasks, as written.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Archived {
    version: u32,
    tasks: Vec<Task>,
}

/// One Project's part of the Factory.
#[derive(Default)]
struct Part {
    /// Its board: the Tasks not archived, its Workflows, its Coordinator
    /// and proposals.
    state: State,
    /// Its archived Tasks, in the order they were archived.
    archive: Vec<Task>,
    /// Why its files could not be read: it refuses every command, steps
    /// nothing and is never written.
    unreadable: Option<String>,
    /// Why its last write failed; cleared by the next that succeeds.
    failing: Option<String>,
    /// The step changed its board, or its Archive, and it is not written.
    unwritten: bool,
    archive_unwritten: bool,
    /// What the step decided, held until what it changed is written.
    held: Vec<Effect>,
}

/// The next Task's, Workflow's and proposal's numbers, across every
/// Project: an id is never given twice by one Server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ids {
    task: u64,
    workflow: u64,
    proposal: u64,
}

/// Where a command's subject is.
enum Found {
    Board(ProjectId),
    Archive(ProjectId),
}

pub struct Factory {
    parts: BTreeMap<ProjectId, Part>,
    /// Where the parts are written; `None` keeps them in memory (tests).
    dir: Option<PathBuf>,
    ids: Ids,
    revision: u64,
    /// Moves only when an Archive changes.
    archive_revision: u64,
    /// Effects decided by commands, handed out by the next step.
    pub(super) pending: Vec<Effect>,
    /// How many Tasks may be Working at once, across every Project.
    limit: usize,
    /// Each GitHub start's reads, kept only while the Server runs.
    pub(super) polls: BTreeMap<WorkflowId, starts::Poll>,
    /// The Workers that were working when it was loaded and have not been
    /// seen since: lost, it was the restart that lost them (#226).
    restarted: BTreeSet<ThreadId>,
    /// The Tasks that were working with no Worker yet when it was loaded:
    /// the restart came between deciding to start one and starting it.
    unstarted: BTreeSet<TaskId>,
    /// Why the whole store could not be read (an older single file): it
    /// refuses every command, steps nothing and writes nothing.
    trouble: Option<String>,
}

fn fresh_state(ids: Ids) -> State {
    State {
        version: SCHEMA_VERSION,
        next: ids.task,
        next_workflow: ids.workflow,
        next_proposal: ids.proposal,
        ..State::default()
    }
}

fn failed(message: impl Into<String>) -> Refusal {
    Refusal::new(RefusalCode::Failed, message)
}

/// The number in an id such as `t-12`: their order is the order they
/// were made in.
fn number(id: &str) -> u64 {
    id.rsplit('-')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn newer(path: &Path, version: u32) -> io::Result<()> {
    if version > SCHEMA_VERSION {
        return Err(io::Error::other(format!(
            "{} is from a newer Ferrite (version {version})",
            path.display()
        )));
    }
    Ok(())
}

fn unreadable(path: &Path, error: impl std::fmt::Display) -> String {
    format!(
        "The Factory's store at {} could not be read, so nothing in it runs and nothing is \
         saved: {error}. Fix or move the file and restart Ferrite.",
        path.display()
    )
}

fn part_path(dir: &Path, project: ProjectId) -> PathBuf {
    dir.join(format!("{}.json", project.get()))
}

fn archive_path(dir: &Path, project: ProjectId) -> PathBuf {
    dir.join(format!("{}.archive.json", project.get()))
}

/// Take the Tasks that have just been archived off the board.
fn sweep(state: &mut State) -> Vec<Task> {
    if !state.tasks.iter().any(Task::archived) {
        return Vec::new();
    }
    let (gone, kept) = std::mem::take(&mut state.tasks)
        .into_iter()
        .partition(Task::archived);
    state.tasks = kept;
    gone
}

impl Part {
    fn write(
        &self,
        dir: Option<&Path>,
        project: ProjectId,
        board: bool,
        archive: bool,
    ) -> Result<(), String> {
        let Some(dir) = dir else {
            return Ok(());
        };
        let write = |path: PathBuf, bytes: serde_json::Result<Vec<u8>>| -> Result<(), String> {
            let bytes = bytes.map_err(|error| error.to_string())?;
            std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
            crate::store::replace_file(&path, &bytes).map_err(|error| error.to_string())
        };
        // The Archive first: a Task written to both is the Archive's.
        if archive {
            let archived = Archived {
                version: SCHEMA_VERSION,
                tasks: self.archive.clone(),
            };
            write(
                archive_path(dir, project),
                serde_json::to_vec_pretty(&archived),
            )?;
        }
        if board {
            write(
                part_path(dir, project),
                serde_json::to_vec_pretty(&self.state),
            )?;
        }
        Ok(())
    }

    fn trouble(&self) -> Option<&String> {
        self.unreadable.as_ref().or(self.failing.as_ref())
    }
}

impl Factory {
    pub fn in_memory() -> Self {
        Self {
            parts: BTreeMap::new(),
            dir: None,
            ids: Ids {
                task: 1,
                workflow: 1,
                proposal: 1,
            },
            revision: 0,
            archive_revision: 0,
            pending: Vec::new(),
            limit: DEFAULT_LIMIT,
            polls: BTreeMap::new(),
            restarted: BTreeSet::new(),
            unstarted: BTreeSet::new(),
            trouble: None,
        }
    }

    /// The Factory kept beside the Store in `store` (a fresh one when
    /// there is none). A Project's part that cannot be read, or that a
    /// newer Ferrite wrote, says why on the board and does nothing: it is
    /// never started afresh over, nor written. Every other Project's part
    /// runs. A Factory still in one older file is moved into parts first.
    pub fn open(store: &Path) -> Self {
        let mut factory = Self::in_memory();
        let dir = store.join(DIR_NAME);
        factory.dir = Some(dir.clone());
        let legacy = store.join(FILE_NAME);
        match read_json::<State>(&legacy).and_then(|state| {
            if let Some(state) = &state {
                newer(&legacy, state.version)?;
            }
            Ok(state)
        }) {
            Ok(None) => {}
            Ok(Some(state)) => {
                if let Err(error) = factory.migrate(state, &legacy) {
                    factory.trouble = Some(unreadable(&legacy, error));
                    return factory;
                }
            }
            Err(error) => {
                factory.trouble = Some(unreadable(&legacy, error));
                return factory;
            }
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return factory,
            Err(error) => {
                factory.trouble = Some(unreadable(&dir, error));
                return factory;
            }
        };
        let mut projects = BTreeSet::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let stem = name
                .strip_suffix(".archive.json")
                .or_else(|| name.strip_suffix(".json"));
            if let Some(id) = stem.and_then(|stem| stem.parse::<u64>().ok()) {
                if let Ok(project) = serde_json::from_value::<ProjectId>(id.into()) {
                    projects.insert(project);
                }
            }
        }
        for project in projects {
            let part = factory.read_part(&dir, project);
            factory.parts.insert(project, part);
        }
        factory.loaded();
        factory
    }

    fn read_part(&self, dir: &Path, project: ProjectId) -> Part {
        let mut part = Part::default();
        let path = part_path(dir, project);
        match read_json::<State>(&path).and_then(|state| {
            state
                .map(|state| newer(&path, state.version).map(|_| state))
                .transpose()
        }) {
            Ok(state) => part.state = state.unwrap_or_else(|| fresh_state(self.ids)),
            Err(error) => {
                part.unreadable = Some(unreadable(&path, error));
                return part;
            }
        }
        let path = archive_path(dir, project);
        match read_json::<Archived>(&path).and_then(|archived| {
            archived
                .map(|archived| newer(&path, archived.version).map(|_| archived))
                .transpose()
        }) {
            Ok(archived) => {
                part.archive = archived.map(|archived| archived.tasks).unwrap_or_default()
            }
            Err(error) => {
                part.state = State::default();
                part.unreadable = Some(unreadable(&path, error));
            }
        }
        part
    }

    /// An older Factory in one file: split into each Project's part and
    /// Archive, written, and the old file set aside.
    fn migrate(&mut self, state: State, legacy: &Path) -> io::Result<()> {
        let ids = Ids {
            task: state.next.max(1),
            workflow: state.next_workflow.max(1),
            proposal: state.next_proposal.max(1),
        };
        let mut parts: BTreeMap<ProjectId, Part> = BTreeMap::new();
        let part = |parts: &mut BTreeMap<ProjectId, Part>, project| {
            parts.entry(project).or_insert_with(|| Part {
                state: fresh_state(ids),
                ..Part::default()
            });
        };
        for task in state.tasks {
            part(&mut parts, task.project);
            parts.get_mut(&task.project).unwrap().state.tasks.push(task);
        }
        for workflow in state.workflows {
            part(&mut parts, workflow.project);
            parts
                .get_mut(&workflow.project)
                .unwrap()
                .state
                .workflows
                .push(workflow);
        }
        for coordinator in state.coordinators {
            part(&mut parts, coordinator.project);
            parts
                .get_mut(&coordinator.project)
                .unwrap()
                .state
                .coordinators
                .push(coordinator);
        }
        for proposal in state.proposals {
            part(&mut parts, proposal.project);
            parts
                .get_mut(&proposal.project)
                .unwrap()
                .state
                .proposals
                .push(proposal);
        }
        let dir = self.dir.clone();
        for (project, part) in &mut parts {
            part.archive = sweep(&mut part.state);
            part.write(dir.as_deref(), *project, true, true)
                .map_err(io::Error::other)?;
        }
        let mut aside = legacy.as_os_str().to_owned();
        aside.push(".migrated");
        std::fs::rename(legacy, PathBuf::from(aside))?;
        Ok(())
    }

    /// After loading: ids continue past every part's, Tasks archived by
    /// an older Ferrite leave the board, and what the restart caught is
    /// remembered for the first step.
    fn loaded(&mut self) {
        for part in self.parts.values_mut() {
            let gone = sweep(&mut part.state);
            for task in gone {
                if !part.archive.iter().any(|kept| kept.id == task.id) {
                    part.archive.push(task);
                }
            }
            // A Task written to both is the Archive's.
            let archived: BTreeSet<TaskId> = part.archive.iter().map(|t| t.id.clone()).collect();
            part.state.tasks.retain(|task| !archived.contains(&task.id));
            self.ids.task = self.ids.task.max(part.state.next);
            self.ids.workflow = self.ids.workflow.max(part.state.next_workflow);
            self.ids.proposal = self.ids.proposal.max(part.state.next_proposal);
            for task in &part.state.tasks {
                if matches!(task.status, Status::Working | Status::NeedsHuman) {
                    self.restarted.extend(task.worker);
                }
                if task.status == Status::Working && task.worker.is_none() {
                    self.unstarted.insert(task.id.clone());
                }
            }
        }
        for task in self.parts.values().flat_map(|part| part.archive.iter()) {
            self.ids.task = self.ids.task.max(number(&task.id) + 1);
        }
    }

    /// The Project whose part holds what `command` is about.
    fn route(&self, command: &Command) -> Result<Found, Refusal> {
        use Command::*;
        let parts = || {
            self.parts
                .iter()
                .filter(|(_, part)| part.unreadable.is_none())
        };
        let on_board = |found: &dyn Fn(&State) -> bool| {
            parts()
                .find(|(_, part)| found(&part.state))
                .map(|(project, _)| *project)
        };
        let archived = |found: &dyn Fn(&Task) -> bool| {
            parts()
                .find(|(_, part)| part.archive.iter().any(found))
                .map(|(project, _)| Found::Archive(*project))
        };
        let task = |id: &str| {
            on_board(&|state: &State| state.tasks.iter().any(|task| task.id == id))
                .map(Found::Board)
                .or_else(|| archived(&|task: &Task| task.id == id))
                .ok_or_else(super::unknown_task)
        };
        let worker = |thread: ThreadId| {
            on_board(&|state: &State| state.tasks.iter().any(|task| task.had_worker(thread)))
                .map(Found::Board)
                .or_else(|| archived(&|task: &Task| task.had_worker(thread)))
                .ok_or_else(super::not_a_worker)
        };
        let workflow = |id: &str| {
            on_board(&|state: &State| state.workflows.iter().any(|made| made.id == id))
                .map(Found::Board)
                .ok_or_else(|| {
                    Refusal::new(RefusalCode::UnknownWorkflow, "no Workflow has that id")
                })
        };
        match command {
            Start { project, .. }
            | CreateWorkflow { project, .. }
            | Clear { project }
            | CoordinatorSay { project, .. }
            | CoordinatorStarted { project, .. }
            | CoordinatorFailed { project, .. }
            | CoordinatorAgent { project, .. } => Ok(Found::Board(*project)),
            RenameWorkflow { workflow: id, .. }
            | AddStep { workflow: id, .. }
            | EditStep { workflow: id, .. }
            | MoveStep { workflow: id, .. }
            | DeleteStep { workflow: id, .. }
            | SaveWorkflow { workflow: id, .. }
            | EditApproval { workflow: id, .. }
            | EditStart { workflow: id, .. }
            | Automation { workflow: id, .. }
            | Backlog { workflow: id, .. }
            | GitHubRead { workflow: id, .. }
            | Submit { workflow: id, .. } => workflow(id),
            WorkerStarted { task: id, .. }
            | WorkerFailed { task: id, .. }
            | Stop { task: id }
            | Resume { task: id }
            | Delete { task: id }
            | AnswerQuestion { task: id, .. }
            | Approve { task: id }
            | Rework { task: id, .. }
            | Redirect { task: id, .. }
            | WorktreeTidied { task: id, .. } => task(id),
            Report { worker: thread, .. }
            | OperatorSaid { worker: thread, .. }
            | Ask { worker: thread, .. }
            | ReportRevision { worker: thread, .. }
            | ReportEvidence { worker: thread, .. } => worker(*thread),
            Propose { coordinator, .. } => on_board(&|state: &State| {
                state
                    .coordinators
                    .iter()
                    .any(|each| each.thread == Some(*coordinator))
            })
            .map(Found::Board)
            .ok_or_else(|| {
                Refusal::new(
                    RefusalCode::NotACoordinator,
                    "this Thread is no Project's Coordinator",
                )
            }),
            AcceptProposal { proposal, .. } | DismissProposal { proposal } => {
                on_board(&|state: &State| state.proposals.iter().any(|each| each.id == *proposal))
                    .map(Found::Board)
                    .ok_or_else(super::coordinator::unknown_proposal)
            }
        }
    }

    /// One change, whoever asks. Its effects come out of the next step.
    pub fn command(&mut self, command: Command, now_ms: i64) -> Result<Answer, Refusal> {
        if let Some(trouble) = &self.trouble {
            return Err(failed(trouble.clone()));
        }
        self.heard(&command);
        let project = match self.route(&command)? {
            Found::Archive(project) => return self.archived(project, command),
            Found::Board(project) => project,
        };
        let ids = self.ids;
        let part = self.parts.entry(project).or_insert_with(|| Part {
            state: fresh_state(ids),
            ..Part::default()
        });
        if let Some(trouble) = &part.unreadable {
            return Err(failed(trouble.clone()));
        }
        part.state.next = ids.task;
        part.state.next_workflow = ids.workflow;
        part.state.next_proposal = ids.proposal;
        let mut next = part.state.clone();
        let mut effects = Vec::new();
        let answer = next.command(command, now_ms, &mut effects)?;
        let gone = sweep(&mut next);
        if next != part.state || !gone.is_empty() {
            let mut written = Part {
                state: next,
                archive: part.archive.clone(),
                ..Part::default()
            };
            written.archive.extend(gone.iter().cloned());
            if let Err(error) = written.write(self.dir.as_deref(), project, true, !gone.is_empty())
            {
                part.failing = Some(format!("The Factory could not be saved: {error}"));
                return Err(failed(format!("the Factory could not be saved: {error}")));
            }
            part.state = written.state;
            if !gone.is_empty() {
                part.archive = written.archive;
                self.archive_revision += 1;
            }
            part.failing = None;
            // What the step held is written now too.
            part.unwritten = false;
            part.archive_unwritten = false;
            self.pending.append(&mut part.held);
            self.revision += 1;
        }
        self.ids = Ids {
            task: part.state.next,
            workflow: part.state.next_workflow,
            proposal: part.state.next_proposal,
        };
        self.pending.extend(effects);
        Ok(answer)
    }

    /// A command about an archived Task: it can be deleted; a Worker of
    /// one reports into the past; nothing else changes it.
    fn archived(&mut self, project: ProjectId, command: Command) -> Result<Answer, Refusal> {
        let part = self.parts.get_mut(&project).expect("routed to it");
        match command {
            Command::Delete { task } => {
                let mut written = Part {
                    archive: part.archive.clone(),
                    ..Part::default()
                };
                let at = written
                    .archive
                    .iter()
                    .position(|each| each.id == task)
                    .ok_or_else(super::unknown_task)?;
                let gone = written.archive.remove(at);
                written
                    .write(self.dir.as_deref(), project, false, true)
                    .map_err(|error| failed(format!("the Factory could not be saved: {error}")))?;
                part.archive = written.archive;
                super::deleted(gone, &mut self.pending);
                self.revision += 1;
                self.archive_revision += 1;
                Ok(Answer::Changed(true))
            }
            Command::Report { .. }
            | Command::OperatorSaid { .. }
            | Command::Ask { .. }
            | Command::ReportRevision { .. }
            | Command::ReportEvidence { .. }
            | Command::WorkerStarted { .. }
            | Command::WorkerFailed { .. }
            | Command::WorktreeTidied { .. }
            | Command::Stop { .. }
            | Command::Resume { .. }
            | Command::Approve { .. }
            | Command::AnswerQuestion { .. } => Ok(Answer::Changed(false)),
            _ => Err(Refusal::new(
                RefusalCode::NotNow,
                "an archived task is kept as it was: it can only be deleted",
            )),
        }
    }

    /// One pass: what to do now, given the time and what each Worker's
    /// Thread is doing. A Worker the Server does not see (no open Thread)
    /// is left as it is.
    pub fn step(&mut self, now_ms: i64, workers: &BTreeMap<ThreadId, Seen>) -> Vec<Effect> {
        if self.trouble.is_some() {
            return Vec::new();
        }
        let mut changed = BTreeSet::new();
        for (project, part) in &mut self.parts {
            if part.unreadable.is_none()
                && part
                    .state
                    .pass_starts(now_ms, &self.unstarted, &mut part.held)
            {
                changed.insert(*project);
            }
        }
        let admit = self.admissions();
        let none = BTreeSet::new();
        for (project, part) in &mut self.parts {
            if part.unreadable.is_some() {
                continue;
            }
            if part.state.pass(
                now_ms,
                workers,
                admit.get(project).unwrap_or(&none),
                &self.restarted,
                &mut part.held,
            ) {
                changed.insert(*project);
            }
            if changed.contains(project) {
                self.revision += 1;
                part.unwritten = true;
                let gone = sweep(&mut part.state);
                if !gone.is_empty() {
                    part.archive.extend(gone);
                    part.archive_unwritten = true;
                    self.archive_revision += 1;
                }
            }
            if part.unwritten || part.archive_unwritten {
                match part.write(
                    self.dir.as_deref(),
                    *project,
                    part.unwritten,
                    part.archive_unwritten,
                ) {
                    Ok(()) => {
                        part.unwritten = false;
                        part.archive_unwritten = false;
                        if part.failing.take().is_some() {
                            self.revision += 1;
                        }
                    }
                    Err(error) => {
                        if part.failing.is_none() {
                            crate::log!("factory: a Project's Factory could not be saved: {error}");
                            self.revision += 1;
                        }
                        part.failing = Some(format!(
                            "The Factory could not be saved, so what it decided waits: {error}. \
                             It tries again on its own."
                        ));
                    }
                }
            }
            if !part.unwritten && !part.archive_unwritten {
                self.pending.append(&mut part.held);
            }
        }
        self.unstarted.clear();
        // Seen once, a Worker lost later was not lost to the restart.
        self.restarted
            .retain(|thread| !workers.contains_key(thread));
        self.poll(now_ms);
        std::mem::take(&mut self.pending)
    }

    /// Which Queued Tasks may start working now, oldest first, while
    /// fewer than the limit work across every Project.
    fn admissions(&self) -> BTreeMap<ProjectId, BTreeSet<TaskId>> {
        let readable = || {
            self.parts
                .iter()
                .filter(|(_, part)| part.unreadable.is_none())
        };
        let working = readable()
            .flat_map(|(_, part)| &part.state.tasks)
            .filter(|task| task.status == Status::Working)
            .count();
        let room = self.limit.saturating_sub(working);
        let mut queued: Vec<(u64, ProjectId, &TaskId)> = readable()
            .flat_map(|(project, part)| part.state.tasks.iter().map(move |task| (project, task)))
            .filter(|(_, task)| task.status == Status::Queued)
            .map(|(project, task)| (number(&task.id), *project, &task.id))
            .collect();
        queued.sort();
        let mut admit: BTreeMap<ProjectId, BTreeSet<TaskId>> = BTreeMap::new();
        for (_, project, task) in queued.into_iter().take(room) {
            admit.entry(project).or_default().insert(task.clone());
        }
        admit
    }

    fn boards(&self) -> impl Iterator<Item = &State> {
        self.parts
            .values()
            .filter(|part| part.unreadable.is_none())
            .map(|part| &part.state)
    }

    /// What the windows draw, the Archive with it.
    pub fn board(&self) -> Board {
        let mut board = self.changes();
        let mut archive: Vec<Task> = self
            .parts
            .values()
            .flat_map(|part| part.archive.iter().cloned())
            .collect();
        archive.sort_by_key(|task| task.keeping.archived_ms);
        board.archive = Some(archive);
        board
    }

    /// The board without the Archive: what a window is told when only the
    /// board changed.
    pub fn changes(&self) -> Board {
        let mut tasks: Vec<Task> = self
            .boards()
            .flat_map(|state| state.tasks.iter().cloned())
            .collect();
        tasks.sort_by_key(|task| number(&task.id));
        let mut workflows: Vec<Workflow> = self
            .boards()
            .flat_map(|state| state.workflows.iter().cloned())
            .collect();
        workflows.sort_by_key(|workflow| number(&workflow.id));
        let mut proposals: Vec<super::Proposal> = self
            .boards()
            .flat_map(|state| state.proposals.iter().cloned())
            .collect();
        proposals.sort_by_key(|proposal| number(&proposal.id));
        Board {
            tasks,
            workflows,
            trouble: self.trouble.clone(),
            troubles: self
                .parts
                .iter()
                .filter_map(|(project, part)| Some((*project, part.trouble()?.clone())))
                .collect(),
            coordinators: self.coordinators().cloned().collect(),
            proposals,
            archive: None,
        }
    }

    pub fn workflow(&self, id: &str) -> Option<&Workflow> {
        self.boards()
            .flat_map(|state| &state.workflows)
            .find(|workflow| workflow.id == id)
    }

    pub(super) fn workflows(&self) -> impl Iterator<Item = &Workflow> {
        self.boards().flat_map(|state| &state.workflows)
    }

    pub fn coordinators(&self) -> impl Iterator<Item = &Coordinator> {
        self.boards().flat_map(|state| &state.coordinators)
    }

    /// The current Worker of each Task on the board: the Threads whose
    /// doing the step is told.
    pub fn watched(&self) -> impl Iterator<Item = ThreadId> + '_ {
        self.boards()
            .flat_map(|state| &state.tasks)
            .filter_map(|task| task.worker)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Moves only when an Archive changed.
    pub fn archive_revision(&self) -> u64 {
        self.archive_revision
    }

    /// How many Tasks may be Working at once.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Set from Settings, within [`super::LIMIT_RANGE`].
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = super::clamp_limit(limit);
    }

    /// A Task on the board or in the Archive.
    pub fn task(&self, id: &str) -> Option<&Task> {
        self.parts
            .values()
            .flat_map(|part| part.state.tasks.iter().chain(&part.archive))
            .find(|task| task.id == id)
    }

    /// The Task `thread` is (or was) a Worker of.
    pub fn task_of(&self, thread: ThreadId) -> Option<&Task> {
        self.parts
            .values()
            .flat_map(|part| part.state.tasks.iter().chain(&part.archive))
            .find(|task| task.had_worker(thread))
    }
}
