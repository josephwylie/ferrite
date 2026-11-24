//! The Archive (#226) as a window reads it: whether a Task has left the
//! board, the search over it, and a Project's archived Tasks. Moving a
//! Task there is the Factory's (`factory::archive`).

use super::{Board, Task};
use crate::project::ProjectId;
use crate::ThreadId;

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

    /// Every Worker it had, current, earlier and replaced, each once.
    pub fn every_worker(&self) -> Vec<ThreadId> {
        let mut threads = Vec::new();
        for thread in self
            .worker
            .into_iter()
            .chain(self.workers.iter().map(|each| each.thread))
            .chain(self.oversight.superseded.iter().copied())
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
