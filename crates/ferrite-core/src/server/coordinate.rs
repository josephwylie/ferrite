//! Coordinator Threads on the Server (ADR 0028): the API's coordinator
//! and worker operations and a window's `coordinate`, the lessons read out
//! of a worker's finished turn into its Project's memory, and its PR's
//! feedback sent back to it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::Server;
use crate::api::schema::{AgentPromptParams, CoordinatorUpdateParams, WorkerStartParams};
use crate::api::{thread_info, thread_result, ApiError, Code};
use crate::coordinator::{
    self, Brief, BriefChange, Coordination, Coordinators, Feedback, Need, Worker,
};
use crate::protocol::{Done, Op, WireChoice};
use crate::store::Provider;
use crate::transcript::{BlockId, Body};
use crate::workspace::registry::ProjectId;
use crate::workspace::PrFeedback;
use crate::{Attention, ThreadId};

/// How often the followed workers' PRs are read: the Pane header's own
/// pace for `gh` (`workspace::PR_TTL`).
const FOLLOW_EVERY: Duration = Duration::from_secs(60);

/// The Server's side of coordination: the book, and what it watches.
pub(super) struct Coordinating {
    book: Coordinators,
    /// Each open worker's last Block read for lessons (`None`: none yet).
    /// What follows it is read once the worker is at rest.
    read: BTreeMap<ThreadId, Option<BlockId>>,
    /// The followed PRs being read, off the pump thread.
    reading: Option<Receiver<Vec<(ThreadId, PrFeedback)>>>,
    read_at: Option<Instant>,
}

impl Coordinating {
    /// The book beside the Store. One that cannot be read is logged and
    /// started afresh, as unreadable settings are.
    pub(super) fn load(dir: &Path) -> Self {
        let book = Coordinators::load(dir).unwrap_or_else(|error| {
            crate::log!("server: coordinators.json could not be read: {error}");
            Coordinators::new(dir)
        });
        Self {
            book,
            read: BTreeMap::new(),
            reading: None,
            read_at: None,
        }
    }
}

fn failed(error: impl ToString) -> ApiError {
    ApiError::new(Code::Failed, error.to_string())
}

fn not_a_coordinator() -> ApiError {
    ApiError::new(Code::NotACoordinator, "that Thread is no Project's coordinator")
}

impl Server {
    /// The Project's coordinator, started in its main checkout and briefed;
    /// or the one it has, reopened when parked. Either way `change` is made
    /// to the Project's brief. A coordinator that was deleted is replaced:
    /// its successor keeps the brief and the workers.
    pub(super) fn start_coordinator(
        &mut self,
        project: ProjectId,
        provider: Provider,
        change: BriefChange,
    ) -> Result<ThreadId, ApiError> {
        let kept = self.coordinating.book.of_project(project).cloned();
        if let Some(thread) = kept.as_ref().map(|kept| kept.coordinator) {
            let open = self.engine.thread(thread).is_some();
            if open || self.engine.parked().map_err(failed)?.contains(&thread) {
                if !open {
                    self.execute(Op::Revive { thread })?;
                }
                self.coordinating
                    .book
                    .update(thread, |coordination| change.apply(coordination))
                    .map_err(failed)?;
                return Ok(thread);
            }
        }
        let (root, title) = self
            .engine
            .registry()
            .project(project)
            .map(|entry| (entry.root.clone(), entry.title.clone()))
            .ok_or_else(|| failed("no such Project"))?;
        let Some(Done::Thread { thread }) = self.execute(Op::Open {
            provider,
            workspace: WireChoice::Main { checkout: root },
        })?
        else {
            return Err(failed("no Thread was opened"));
        };
        let mut coordination = kept.unwrap_or_else(|| Coordination {
            project,
            coordinator: thread,
            goal: String::new(),
            instructions: String::new(),
            follow_prs: false,
            workers: Vec::new(),
        });
        coordination.coordinator = thread;
        change.apply(&mut coordination);
        let prompt = coordinator::coordinator_prompt(&Brief {
            project: &title,
            goal: &coordination.goal,
            instructions: &coordination.instructions,
            memory: &self.coordinating.book.memory(project),
        });
        self.coordinating
            .book
            .insert(coordination)
            .map_err(failed)?;
        self.execute(Op::Rename {
            thread,
            title: format!("{title} coordinator"),
        })?;
        self.execute(Op::Send {
            thread,
            text: prompt,
        })?;
        Ok(thread)
    }

    /// `coordinator.get`: the brief, the memory, and the workers — the open
    /// ones in the order of what needs the operator, start order among
    /// equals, then the closed ones.
    pub(super) fn coordinator_result(&self, coordinator: ThreadId) -> Result<Value, ApiError> {
        let book = &self.coordinating.book;
        let coordination = book.led_by(coordinator).ok_or_else(not_a_coordinator)?;
        let mut open: Vec<(Need, Value)> = coordination
            .workers
            .iter()
            .filter_map(|worker| {
                let info = thread_info(&self.engine, worker.thread)?;
                let need = Need::of(info.attention);
                Some((
                    need,
                    json!({ "need": need, "task": worker.task, "thread": info }),
                ))
            })
            .collect();
        open.sort_by_key(|(need, _)| *need);
        let closed: Vec<Value> = coordination
            .workers
            .iter()
            .filter(|worker| self.engine.thread(worker.thread).is_none())
            .map(|worker| json!({ "thread": worker.thread, "task": worker.task }))
            .collect();
        let project = coordination.project;
        Ok(json!({
            "coordinator": thread_info(&self.engine, coordinator),
            "project": project,
            "goal": coordination.goal,
            "instructions": coordination.instructions,
            "follow_prs": coordination.follow_prs,
            "memory": { "path": book.memory_path(project), "text": book.memory(project) },
            "workers": open.into_iter().map(|(_, worker)| worker).collect::<Vec<_>>(),
            "closed": closed,
        }))
    }

    pub(super) fn update_coordinator(
        &mut self,
        params: CoordinatorUpdateParams,
    ) -> Result<Value, ApiError> {
        let CoordinatorUpdateParams {
            thread,
            goal,
            instructions,
            follow_prs,
        } = params;
        let change = BriefChange {
            goal,
            instructions,
            follow_prs,
        };
        let found = self
            .coordinating
            .book
            .update(thread, |coordination| change.apply(coordination))
            .map_err(failed)?;
        if !found {
            return Err(not_a_coordinator());
        }
        self.coordinator_result(thread)
    }

    /// `worker.start`: a Thread in a new worktree of the Project, seated in
    /// its coordinator's Group and briefed with its task. Once it has
    /// started it is the coordinator's, whatever fails after.
    pub(super) fn start_worker(&mut self, params: WorkerStartParams) -> Result<Value, ApiError> {
        let coordinator = params.thread;
        let coordination = self
            .coordinating
            .book
            .led_by(coordinator)
            .cloned()
            .ok_or_else(not_a_coordinator)?;
        let (root, project_title) = self
            .engine
            .registry()
            .project(coordination.project)
            .map(|entry| (entry.root.clone(), entry.title.clone()))
            .ok_or_else(|| failed("the coordinator's Project is gone"))?;
        let Some(Done::Thread { thread }) = self.execute(Op::Open {
            provider: params.provider.unwrap_or(self.default_provider),
            workspace: WireChoice::NewWorktree { repo: root },
        })?
        else {
            return Err(failed("no Thread was opened"));
        };
        let recorded = self.coordinating.book.update(coordinator, |coordination| {
            coordination.workers.push(Worker {
                thread,
                task: params.task.clone(),
                sent: Default::default(),
            })
        });
        if let Err(error) = recorded {
            crate::log!("coordinator {coordinator}: worker {thread} was not recorded: {error}");
        }
        let group_title = self
            .engine
            .thread(coordinator)
            .and_then(|view| view.title().map(String::from))
            .unwrap_or_else(|| format!("{project_title} coordinator"));
        if let Err(error) = self.engine.enlist(coordinator, thread, &group_title) {
            crate::log!("coordinator {coordinator}: worker {thread} joined no Group: {error}");
        }
        if let Some(title) = params.title {
            self.execute(Op::Rename { thread, title })?;
        }
        let prompt = coordinator::worker_prompt(
            &Brief {
                project: &project_title,
                goal: &coordination.goal,
                instructions: &coordination.instructions,
                memory: &self.coordinating.book.memory(coordination.project),
            },
            coordinator,
            &params.task,
        );
        self.execute(Op::Send {
            thread,
            text: prompt,
        })?;
        thread_result(&self.engine, thread)
    }

    /// One pass: a worker's finished turn is read for its lessons, and the
    /// followed PRs' feedback goes back to their workers.
    pub(super) fn coordinate(&mut self) {
        self.remember_lessons();
        self.follow_prs();
    }

    /// What a worker wrote since it was last at rest, once it is at rest
    /// again — every turn in between, queued or blocked on the way — is
    /// read for its `## Remember` lessons, which join the memory. A worker
    /// first seen at rest has nothing new; one first seen mid-turn is read
    /// from that turn's prompt.
    fn remember_lessons(&mut self) {
        let workers: Vec<(ProjectId, ThreadId)> = self
            .coordinating
            .book
            .iter()
            .flat_map(|coordination| {
                coordination
                    .workers
                    .iter()
                    .map(|worker| (coordination.project, worker.thread))
            })
            .collect();
        let mut read = BTreeMap::new();
        for (project, thread) in workers {
            let (Some(view), Some(attention)) =
                (self.engine.thread(thread), self.engine.attention(thread))
            else {
                continue;
            };
            let blocks = view.transcript().blocks();
            let at_rest = matches!(attention, Attention::Done | Attention::Idle);
            let last = blocks.last().map(|block| block.id);
            let Some(&from) = self.coordinating.read.get(&thread) else {
                let from = if at_rest {
                    last
                } else {
                    blocks
                        .iter()
                        .rfind(|block| matches!(block.body, Body::Prompt(_)))
                        .map(|block| block.id)
                };
                read.insert(thread, from);
                continue;
            };
            if !at_rest || last == from {
                read.insert(thread, from);
                continue;
            }
            read.insert(thread, last);
            let unread = &blocks[blocks.partition_point(|block| Some(block.id) <= from)..];
            let lessons = coordinator::remembered(&crate::transcript::plain_text(unread));
            if lessons.is_empty() {
                continue;
            }
            if let Err(error) = self.coordinating.book.remember(project, &lessons) {
                crate::log!("worker {thread}: its lessons were not remembered: {error}");
            }
        }
        self.coordinating.read = read;
    }

    /// Read the followed workers' PRs every `FOLLOW_EVERY`, one reading at
    /// a time, and send each its new feedback when the reading lands.
    fn follow_prs(&mut self) {
        if let Some(reading) = &self.coordinating.reading {
            match reading.try_recv() {
                Ok(read) => {
                    self.coordinating.reading = None;
                    self.send_feedback(read);
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => self.coordinating.reading = None,
            }
        }
        if self
            .coordinating
            .read_at
            .is_some_and(|at| at.elapsed() < FOLLOW_EVERY)
        {
            return;
        }
        let followed: Vec<(ThreadId, PathBuf)> = self
            .coordinating
            .book
            .iter()
            .filter(|coordination| coordination.follow_prs)
            .flat_map(|coordination| &coordination.workers)
            .filter_map(|worker| {
                let cwd = self.engine.thread(worker.thread)?.workspace()?.cwd();
                Some((worker.thread, cwd.to_path_buf()))
            })
            .collect();
        if followed.is_empty() {
            return;
        }
        self.coordinating.read_at = Some(Instant::now());
        let (tell, reading) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("ferrite-follow-prs".into())
            .spawn(move || {
                let read = followed
                    .into_iter()
                    .filter_map(|(thread, cwd)| Some((thread, crate::workspace::pr_feedback(&cwd)?)))
                    .collect();
                let _ = tell.send(read);
            });
        match spawned {
            Ok(_) => self.coordinating.reading = Some(reading),
            Err(error) => crate::log!("server: the workers' PRs could not be read: {error}"),
        }
    }

    /// Each worker whose PRs are still followed is sent what its PR asks
    /// that it was not sent before, as one prompt; feedback that could not
    /// be sent (a Decision waits, the agent cannot queue yet) is tried
    /// again on the next reading.
    fn send_feedback(&mut self, read: Vec<(ThreadId, PrFeedback)>) {
        for (thread, pr) in read {
            let Some(worker) = self
                .coordinating
                .book
                .iter()
                .filter(|coordination| coordination.follow_prs)
                .flat_map(|coordination| &coordination.workers)
                .find(|worker| worker.thread == thread)
            else {
                continue;
            };
            let items = coordinator::feedback(&pr);
            let new: Vec<&Feedback> = items
                .iter()
                .filter(|item| !worker.sent.contains(&item.key))
                .collect();
            if new.is_empty() {
                continue;
            }
            let text = coordinator::feedback_prompt(pr.pr.number, &new);
            let keys = new.iter().map(|item| item.key.clone()).collect();
            let sent = self.prompt(&AgentPromptParams {
                thread,
                text,
                wait: false,
                timeout_ms: None,
            });
            match sent {
                Ok(()) => {
                    if let Err(error) = self.coordinating.book.mark_sent(thread, keys) {
                        crate::log!("worker {thread}: feedback sent was not recorded: {error}");
                    }
                }
                Err(error) => {
                    crate::log!("worker {thread}: PR feedback waits: {}", error.message)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
