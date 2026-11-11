//! The Archive (#226): a Completed Task leaves the board 7 days after it
//! completed, or at once with its column's Clear. Archived, it is kept
//! whole (activity, results, Workers) and searched by title and
//! instructions. Its Workers are parked once it completes and they are
//! at rest; they are deleted only with the Task.

use std::collections::BTreeMap;

use super::{Answer, Board, Effect, Seen, State, Status, Task};
use crate::workspace::registry::ProjectId;
use crate::ThreadId;

/// How long a Completed Task stays on the board.
pub const ARCHIVE_AFTER_MS: i64 = 7 * 24 * 3_600_000;

impl Task {
    /// It has left the board for the Archive.
    pub fn archived(&self) -> bool {
        self.keeping.archived_ms.is_some()
    }

    /// Its title or instructions hold `query`, ignoring case; an empty
    /// query matches every Task.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || self.title.to_lowercase().contains(&query)
            || self.instructions.to_lowercase().contains(&query)
    }

    /// Every Worker it had, current and earlier, each once.
    pub fn every_worker(&self) -> Vec<ThreadId> {
        let mut threads = Vec::new();
        for thread in self
            .worker
            .into_iter()
            .chain(self.workers.iter().map(|each| each.thread))
        {
            if !threads.contains(&thread) {
                threads.push(thread);
            }
        }
        threads
    }
}

impl Board {
    /// `project`'s Tasks on the board: all but the archived.
    pub fn on_board(&self, project: ProjectId) -> impl Iterator<Item = &Task> {
        self.of(project).filter(|task| !task.archived())
    }

    /// `project`'s archived Tasks that match `query`, most recently
    /// archived first.
    pub fn archive<'a>(
        &'a self,
        project: ProjectId,
        query: &'a str,
    ) -> impl Iterator<Item = &'a Task> {
        let mut found: Vec<&Task> = self
            .archive
            .iter()
            .flatten()
            .filter(|task| task.project == project && task.matches(query))
            .collect();
        found.sort_by_key(|task| std::cmp::Reverse(task.keeping.archived_ms));
        found.into_iter()
    }
}

impl State {
    /// Clear: `project`'s Completed Tasks go to the Archive now.
    pub(super) fn clear(&mut self, project: ProjectId, now_ms: i64) -> Answer {
        let mut changed = false;
        for task in &mut self.tasks {
            if task.project == project && task.status == Status::Completed && !task.archived() {
                task.keeping.completed_ms.get_or_insert(now_ms);
                task.keeping.archived_ms = Some(now_ms);
                changed = true;
            }
        }
        Answer::Changed(changed)
    }

    /// Each Completed Task: its completion stamped, its Workers parked
    /// once at rest, and archived 7 days on. Whether any changed.
    pub(super) fn step_archive(
        &mut self,
        now_ms: i64,
        workers: &BTreeMap<ThreadId, Seen>,
        effects: &mut Vec<Effect>,
    ) -> bool {
        let mut changed = false;
        for task in &mut self.tasks {
            if task.status != Status::Completed {
                continue;
            }
            if task.keeping.completed_ms.is_none() {
                task.keeping.completed_ms = Some(now_ms);
                changed = true;
            }
            let completed = task.keeping.completed_ms.unwrap_or(now_ms);
            let at_rest = task
                .worker
                .and_then(|thread| workers.get(&thread))
                .is_none_or(|seen| !seen.busy);
            if !task.keeping.parked && at_rest {
                task.keeping.parked = true;
                changed = true;
                for thread in task.every_worker() {
                    effects.push(Effect::Park { thread });
                }
            }
            if !task.archived() && now_ms - completed >= ARCHIVE_AFTER_MS {
                task.keeping.archived_ms = Some(now_ms);
                changed = true;
            }
        }
        changed
    }
}
