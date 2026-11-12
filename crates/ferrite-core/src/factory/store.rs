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
//! What the Server reports it did (a Worker started, a read come back) is
//! taken the same way: it already happened, so a failed write cannot
//! refuse it. A part is marked written only by a write that succeeded.
//! An idle pass writes nothing and copies nothing.
//!
//! The next ids are kept in a file of their own, written before any part
//! that holds an id they gave: one Project's unreadable part cannot make
//! another give its ids again.

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
    /// Its board: its Workflows, its Coordinator and proposals, and the
    /// Tasks not in its Archive's file. An archived Task stays here until
    /// that file holds it, so no failed write loses one.
    state: State,
    /// Its archived Tasks as its Archive's file holds them, in the order
    /// they were archived.
    archive: Vec<Task>,
    /// Why its files could not be read: it refuses every command, steps
    /// nothing and is never written.
    unreadable: Option<String>,
    /// Why its last write failed; cleared by the next that succeeds.
    failing: Option<String>,
    /// Its board changed and is not written.
    unwritten: bool,
    /// What the step decided, held until what it changed is written.
    held: Vec<Effect>,
}

/// What a part's write did.
struct Written {
    /// Archived Tasks moved off the board into the Archive.
    moved: bool,
    /// Why the Archive could not be written, if it could not: its Tasks
    /// stay on the board, and the next write tries again.
    archive: Option<String>,
}

/// The next Task's, Workflow's and proposal's numbers, across every
/// Project: an id is never given twice by one Server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Ids {
    task: u64,
    workflow: u64,
    proposal: u64,
}

impl Ids {
    /// What a Project's State gives next, before it makes anything.
    fn lend(self, state: &mut State) {
        state.next = self.task;
        state.next_workflow = self.workflow;
        state.next_proposal = self.proposal;
    }

    /// What follows once `state` has made what it made.
    fn of(state: &State) -> Self {
        Self {
            task: state.next,
            workflow: state.next_workflow,
            proposal: state.next_proposal,
        }
    }

    fn max(self, other: Self) -> Self {
        Self {
            task: self.task.max(other.task),
            workflow: self.workflow.max(other.workflow),
            proposal: self.proposal.max(other.proposal),
        }
    }
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
    /// What the ids file holds.
    ids_written: Ids,
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

fn ids_path(dir: &Path) -> PathBuf {
    dir.join("ids.json")
}

fn write_file(dir: &Path, path: &Path, bytes: serde_json::Result<Vec<u8>>) -> Result<(), String> {
    let bytes = bytes.map_err(|error| error.to_string())?;
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    crate::store::replace_file(path, &bytes).map_err(|error| error.to_string())
}

/// The next ids, written when they moved: before any part that holds one.
fn write_ids(dir: Option<&Path>, written: &mut Ids, ids: Ids) -> Result<(), String> {
    let Some(dir) = dir.filter(|_| ids != *written) else {
        return Ok(());
    };
    write_file(dir, &ids_path(dir), serde_json::to_vec_pretty(&ids))?;
    *written = ids;
    Ok(())
}

/// The archived Task a script's repeated submission already started.
fn archived_submission(command: &Command, archive: &[Task]) -> Option<TaskId> {
    let Command::Submit {
        workflow, identity, ..
    } = command
    else {
        return None;
    };
    let request = starts::script_request(workflow, identity);
    archive
        .iter()
        .find(|task| task.request.as_deref() == Some(request.as_str()))
        .map(|task| task.id.clone())
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
    /// Its archived Tasks into the Archive's file, then (when `board`, or
    /// when a Task left it) its board into its own: a Task leaves the
    /// board's file only once the Archive's holds it. A failed Archive
    /// write leaves them on the board and is not the board's failure.
    fn write(
        &mut self,
        dir: Option<&Path>,
        project: ProjectId,
        board: bool,
    ) -> Result<Written, String> {
        let mut written = Written {
            moved: false,
            archive: None,
        };
        if self.state.tasks.iter().any(Task::archived) {
            let moving: Vec<&Task> = self.state.tasks.iter().filter(|t| t.archived()).collect();
            // One the Archive's file already holds (a board write failed
            // after it) is replaced, not held twice.
            let mut archive: Vec<Task> = self
                .archive
                .iter()
                .filter(|kept| !moving.iter().any(|task| task.id == kept.id))
                .cloned()
                .collect();
            archive.extend(moving.into_iter().cloned());
            let saved = match dir {
                None => Ok(()),
                Some(dir) => write_file(
                    dir,
                    &archive_path(dir, project),
                    serde_json::to_vec_pretty(&Archived {
                        version: SCHEMA_VERSION,
                        tasks: archive.clone(),
                    }),
                ),
            };
            match saved {
                Ok(()) => {
                    sweep(&mut self.state);
                    self.archive = archive;
                    written.moved = true;
                }
                Err(error) => written.archive = Some(error),
            }
        }
        if let Some(dir) = dir.filter(|_| board || written.moved) {
            write_file(
                dir,
                &part_path(dir, project),
                serde_json::to_vec_pretty(&self.state),
            )?;
        }
        Ok(written)
    }

    /// Its Archive's file is behind: archived Tasks wait on the board.
    fn archive_behind(&self) -> bool {
        self.state.tasks.iter().any(Task::archived)
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
            ids_written: Ids {
                task: 0,
                workflow: 0,
                proposal: 0,
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
        let ids = ids_path(&dir);
        match read_json::<Ids>(&ids) {
            Ok(None) => {}
            Ok(Some(written)) => {
                factory.ids = factory.ids.max(written);
                factory.ids_written = written;
            }
            Err(error) => {
                factory.trouble = Some(unreadable(&ids, error));
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
            let written = part
                .write(dir.as_deref(), *project, true)
                .map_err(io::Error::other)?;
            if let Some(error) = written.archive {
                return Err(io::Error::other(error));
            }
        }
        let mut aside = legacy.as_os_str().to_owned();
        aside.push(".migrated");
        std::fs::rename(legacy, PathBuf::from(aside))?;
        Ok(())
    }

    /// After loading: ids continue past every part's, and what the
    /// restart caught is remembered for the first step. An archived Task
    /// still on the board goes to the Archive at the first write.
    fn loaded(&mut self) {
        for part in self.parts.values_mut() {
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
            | WorktreeTidied { task: id, .. }
            | GitHubHead { task: id, .. } => task(id),
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
        if let Some(task) = archived_submission(&command, &part.archive) {
            return Ok(Answer::Duplicate(task));
        }
        let fact = command.reports_effect();
        ids.lend(&mut part.state);
        let mut next = part.state.clone();
        let mut effects = Vec::new();
        let answer = next.command(command, now_ms, &mut effects)?;
        if next == part.state {
            self.pending.extend(effects);
            return Ok(answer);
        }
        let made = Ids::of(&next);
        let mut written = Part {
            state: next,
            archive: std::mem::take(&mut part.archive),
            ..Part::default()
        };
        let dir = self.dir.as_deref();
        let result = write_ids(dir, &mut self.ids_written, made)
            .and_then(|()| written.write(dir, project, true));
        part.archive = std::mem::take(&mut written.archive);
        let failing = match &result {
            Err(error) => Some(error),
            Ok(done) => done.archive.as_ref(),
        }
        .map(|error| format!("The Factory could not be saved: {error}"));
        if part.failing != failing {
            part.failing = failing;
            self.revision += 1;
        }
        match &result {
            Err(error) if !fact => {
                return Err(failed(format!("the Factory could not be saved: {error}")))
            }
            Ok(done) => {
                if done.moved {
                    self.archive_revision += 1;
                }
                // What the step held is written now too.
                part.unwritten = false;
                self.pending.append(&mut part.held);
                self.pending.extend(effects);
            }
            // A fact is taken as the step's changes are: in place, its
            // effects held until a later write succeeds.
            Err(_) => {
                part.unwritten = true;
                part.held.extend(effects);
            }
        }
        part.state = written.state;
        self.revision += 1;
        self.ids = made;
        Ok(answer)
    }

    /// A command about an archived Task: it can be deleted; a Worker of
    /// one reports into the past; nothing else changes it.
    fn archived(&mut self, project: ProjectId, command: Command) -> Result<Answer, Refusal> {
        let part = self.parts.get_mut(&project).expect("routed to it");
        match command {
            Command::Delete { task } => {
                let mut archive = part.archive.clone();
                let at = archive
                    .iter()
                    .position(|each| each.id == task)
                    .ok_or_else(super::unknown_task)?;
                let gone = archive.remove(at);
                if let Some(dir) = self.dir.as_deref() {
                    let archived = Archived {
                        version: SCHEMA_VERSION,
                        tasks: archive,
                    };
                    write_file(
                        dir,
                        &archive_path(dir, project),
                        serde_json::to_vec_pretty(&archived),
                    )
                    .map_err(|error| failed(format!("the Factory could not be saved: {error}")))?;
                    archive = archived.tasks;
                }
                part.archive = archive;
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
            | Command::GitHubHead { .. }
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
    /// Thread is doing. A Worker the Server says nothing of is left as it
    /// is.
    pub fn step(&mut self, now_ms: i64, workers: &BTreeMap<ThreadId, Seen>) -> Vec<Effect> {
        if self.trouble.is_some() {
            return Vec::new();
        }
        let mut changed = BTreeSet::new();
        for (project, part) in &mut self.parts {
            if part.unreadable.is_some() {
                continue;
            }
            // A schedule's or GitHub's Task takes the next id across
            // every Project, as a command's does.
            self.ids.lend(&mut part.state);
            if part
                .state
                .pass_starts(now_ms, &self.unstarted, &mut part.held)
            {
                changed.insert(*project);
            }
            self.ids = Ids::of(&part.state);
        }
        // A part holding an id the ids file has not given waits for it.
        let ids = write_ids(self.dir.as_deref(), &mut self.ids_written, self.ids);
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
            }
            if part.unwritten || part.archive_behind() {
                let board = part.unwritten;
                let result = ids
                    .clone()
                    .and_then(|()| part.write(self.dir.as_deref(), *project, board));
                if let Ok(done) = &result {
                    part.unwritten = false;
                    if done.moved {
                        self.revision += 1;
                        self.archive_revision += 1;
                    }
                }
                let failing = match &result {
                    Err(error) => Some(error),
                    Ok(done) => done.archive.as_ref(),
                };
                match failing {
                    None => {
                        if part.failing.take().is_some() {
                            self.revision += 1;
                        }
                    }
                    Some(error) => {
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
            if !part.unwritten {
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
    /// fewer than the limit hold a place across every Project: a Task
    /// waiting on a Provider permission keeps its own, since answering
    /// it lets its Worker go on at once.
    fn admissions(&self) -> BTreeMap<ProjectId, BTreeSet<TaskId>> {
        let readable = || {
            self.parts
                .iter()
                .filter(|(_, part)| part.unreadable.is_none())
        };
        let working = readable()
            .flat_map(|(_, part)| &part.state.tasks)
            .filter(|task| task.running())
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

    pub fn proposal(&self, id: &str) -> Option<&super::Proposal> {
        self.boards()
            .flat_map(|state| &state.proposals)
            .find(|proposal| proposal.id == id)
    }

    pub(super) fn workflows(&self) -> impl Iterator<Item = &Workflow> {
        self.boards().flat_map(|state| &state.workflows)
    }

    pub fn coordinators(&self) -> impl Iterator<Item = &Coordinator> {
        self.boards().flat_map(|state| &state.coordinators)
    }

    /// `thread` is a Project's Coordinator.
    pub fn coordinates(&self, thread: ThreadId) -> bool {
        self.coordinators()
            .any(|coordinator| coordinator.thread == Some(thread))
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
