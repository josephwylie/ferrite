//! Coordinator Threads on the Server (ADR 0028): the API's coordinator
//! and worker operations and a window's `coordinate`, the lessons read out
//! of a worker's finished turn into its Project's memory, its PR's
//! feedback sent back to it, and its worktree removed once that PR merged.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::Server;
use crate::api::schema::{AgentPromptParams, CoordinatorUpdateParams, WorkerStartParams};
use crate::api::{thread_info, thread_result, ApiError, Code};
use crate::coordinator::{
    self, Brief, BriefChange, Coordination, Coordinators, Feedback, Merged, Need, Worker,
};
use crate::groups::GroupChange;
use crate::protocol::{Done, Op, WireChoice};
use crate::store::Provider;
use crate::transcript::{BlockId, Body};
use crate::workspace::registry::ProjectId;
use crate::workspace::{PrFeedback, PrState, WorkspaceBinding};
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
    /// The followed PRs being read and the merged workers' worktrees
    /// being removed, off the pump thread.
    reading: Option<Receiver<Reading>>,
    read_at: Option<Instant>,
}

/// What one pass off the pump thread found.
struct Reading {
    /// Each followed worker's PR, and for a merged one whether its
    /// checkout is clean: nothing modified, nothing untracked.
    prs: Vec<(ThreadId, PrFeedback, bool)>,
    /// What became of each parked merged worker's worktree.
    cleanups: Vec<(ThreadId, Cleanup)>,
}

/// A parked merged worker's checkout, to be removed.
struct Retiring {
    thread: ThreadId,
    repo: PathBuf,
    /// Its worktree; `None` once it is bound to the main checkout.
    worktree: Option<PathBuf>,
    /// The branch Ferrite made it, deleted while at `head`.
    branch: Option<String>,
    head: String,
}

enum Cleanup {
    /// The worktree is gone, and the branch with it unless named here.
    Removed { kept: Option<String> },
    /// The worktree stays: git refused it, logged.
    Refused,
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
    ApiError::new(
        Code::NotACoordinator,
        "that Thread is no Project's coordinator",
    )
}

impl Server {
    /// The Project `cwd` belongs to, so a start from any directory of a
    /// Project, or from one of its worktrees, finds its one coordinator:
    /// the registered Project holding `cwd` or its repository's main
    /// checkout; else that checkout (`cwd` outside a repository),
    /// registered.
    pub(super) fn project_at(&mut self, cwd: &Path) -> Result<ProjectId, ApiError> {
        let cwd = std::fs::canonicalize(cwd).map_err(failed)?;
        if let Some(project) = self.engine.registry().containing(&cwd) {
            return Ok(project);
        }
        let main = crate::workspace::worktrees(&cwd)
            .ok()
            .and_then(|listed| listed.into_iter().next())
            .map_or(cwd, |main| main.path);
        match self.engine.registry().containing(&main) {
            Some(project) => Ok(project),
            None => self.engine.register_project(&main).map_err(failed),
        }
    }

    /// The Project's coordinator, started in its main checkout and briefed;
    /// or the one it has, reopened when parked and told of a goal or
    /// instructions `change` makes. Either way `change` is made to the
    /// Project's brief. A coordinator that was deleted is replaced: its
    /// successor keeps the brief and the workers.
    pub(super) fn start_coordinator(
        &mut self,
        project: ProjectId,
        provider: Provider,
        change: BriefChange,
    ) -> Result<ThreadId, ApiError> {
        let (root, title) = self
            .engine
            .registry()
            .project(project)
            .map(|entry| (entry.root.clone(), entry.title.clone()))
            .ok_or_else(|| failed("no such Project"))?;
        let kept = self.coordinating.book.of_project(project).cloned();
        if let Some(kept) = &kept {
            let thread = kept.coordinator;
            let open = self.engine.thread(thread).is_some();
            if open || self.engine.parked().map_err(failed)?.contains(&thread) {
                if !open {
                    self.execute(Op::Revive { thread })?;
                }
                let rebriefed = change.goal.as_ref().is_some_and(|goal| *goal != kept.goal)
                    || change
                        .instructions
                        .as_ref()
                        .is_some_and(|instructions| *instructions != kept.instructions);
                self.coordinating
                    .book
                    .update(thread, |coordination| change.apply(coordination))
                    .map_err(failed)?;
                if rebriefed {
                    self.tell_brief(thread, project, &title);
                }
                return Ok(thread);
            }
        }
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

    /// The coordinator is sent its Project's brief as it now stands, by
    /// `agent.prompt`'s rules; one that cannot be sent (a Decision waits)
    /// is logged: the coordinator reads the brief with `coordinator get`.
    fn tell_brief(&mut self, thread: ThreadId, project: ProjectId, title: &str) {
        let Some(coordination) = self.coordinating.book.of_project(project) else {
            return;
        };
        let text = coordinator::brief_changed_prompt(&Brief {
            project: title,
            goal: &coordination.goal,
            instructions: &coordination.instructions,
            memory: &self.coordinating.book.memory(project),
        });
        let told = self.prompt(&AgentPromptParams {
            thread,
            text,
            wait: false,
            timeout_ms: None,
        });
        if let Err(error) = told {
            crate::log!(
                "coordinator {thread}: its new brief was not sent: {}",
                error.message
            );
        }
    }

    /// `coordinator.get`: the brief, the memory, and the workers — the open
    /// ones in the order of what needs the operator, start order among
    /// equals, then the closed ones, the resolved among them.
    pub(super) fn coordinator_result(&self, coordinator: ThreadId) -> Result<Value, ApiError> {
        let book = &self.coordinating.book;
        let coordination = book.led_by(coordinator).ok_or_else(not_a_coordinator)?;
        let mut open: Vec<(Need, Value)> = coordination
            .workers
            .iter()
            .filter(|worker| !worker.resolved())
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
            .filter(|worker| worker.resolved() || self.engine.thread(worker.thread).is_none())
            .map(|worker| {
                json!({ "thread": worker.thread, "task": worker.task, "merged": worker.merged })
            })
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
        if params.task.trim().is_empty() {
            return Err(ApiError::new(Code::InvalidRequest, "a worker needs a task"));
        }
        if params
            .title
            .as_deref()
            .is_some_and(|title| title.trim().is_empty())
        {
            return Err(ApiError::new(
                Code::InvalidRequest,
                "a worker's title cannot be blank",
            ));
        }
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
                merged: None,
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
            if let Err(error) = self.execute(Op::Rename { thread, title }) {
                crate::log!(
                    "coordinator {coordinator}: worker {thread} kept its title: {}",
                    error.message
                );
            }
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
            coordination.follow_prs,
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
    /// from that turn's prompt. Lessons the memory could not take are read
    /// again on the next pass.
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
            let unread = &blocks[blocks.partition_point(|block| Some(block.id) <= from)..];
            let lessons = coordinator::remembered(&crate::transcript::plain_text(unread));
            let next = match self.coordinating.book.remember(project, &lessons) {
                Ok(_) => last,
                Err(error) => {
                    crate::log!("worker {thread}: its lessons wait to be remembered: {error}");
                    from
                }
            };
            read.insert(thread, next);
        }
        self.coordinating.read = read;
    }

    /// Every `FOLLOW_EVERY`, one pass at a time and off the pump thread:
    /// the parked merged workers' worktrees are removed, and the followed
    /// workers' PRs read. When the pass lands, each worker is resolved,
    /// retired or sent its new feedback.
    fn follow_prs(&mut self) {
        if let Some(reading) = &self.coordinating.reading {
            match reading.try_recv() {
                Ok(read) => {
                    self.coordinating.reading = None;
                    self.land(read);
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
            .filter(|worker| !worker.resolved())
            .filter_map(|worker| {
                let cwd = self.engine.thread(worker.thread)?.workspace()?.cwd();
                Some((worker.thread, cwd.to_path_buf()))
            })
            .collect();
        let retiring = self.retiring();
        if followed.is_empty() && retiring.is_empty() {
            return;
        }
        self.coordinating.read_at = Some(Instant::now());
        let (tell, reading) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("ferrite-follow-prs".into())
            .spawn(move || {
                let cleanups = retiring
                    .iter()
                    .map(|retiring| (retiring.thread, clean_up(retiring)))
                    .collect();
                let prs = followed
                    .into_iter()
                    .filter_map(|(thread, cwd)| {
                        let pr = crate::workspace::pr_feedback(&cwd)?;
                        let clean = pr.pr.state == PrState::Merged
                            && crate::workspace::is_clean(&cwd).unwrap_or(false);
                        Some((thread, pr, clean))
                    })
                    .collect();
                let _ = tell.send(Reading { prs, cleanups });
            });
        match spawned {
            Ok(_) => self.coordinating.reading = Some(reading),
            Err(error) => crate::log!("server: the workers' PRs could not be read: {error}"),
        }
    }

    fn land(&mut self, reading: Reading) {
        for (thread, cleanup) in reading.cleanups {
            self.resolve(thread, cleanup);
        }
        let mut feedback = Vec::new();
        for (thread, pr, clean) in reading.prs {
            if pr.pr.state == PrState::Merged {
                self.retire(thread, &pr, clean);
            } else {
                feedback.push((thread, pr));
            }
        }
        self.send_feedback(feedback);
    }

    /// `thread`'s entry in the book while its Project follows PRs.
    fn followed_worker(&self, thread: ThreadId) -> Option<&Worker> {
        self.coordinating
            .book
            .iter()
            .filter(|coordination| coordination.follow_prs)
            .flat_map(|coordination| &coordination.workers)
            .find(|worker| worker.thread == thread)
    }

    /// A followed worker whose PR merged is parked, for its worktree to be
    /// removed on the next pass, once it is at rest: done or idle, nothing
    /// queued. One whose checkout holds uncommitted or untracked work is
    /// left open and logged, and asked again at the next reading.
    fn retire(&mut self, thread: ThreadId, pr: &PrFeedback, clean: bool) {
        if self.followed_worker(thread).is_none() {
            return;
        }
        let at_rest = matches!(
            self.engine.attention(thread),
            Some(Attention::Done | Attention::Idle)
        ) && self
            .engine
            .thread(thread)
            .is_some_and(|view| view.queued().is_none());
        if !at_rest {
            return;
        }
        let number = pr.pr.number;
        if !clean {
            crate::log!(
                "worker {thread}: PR #{number} merged, but its worktree holds uncommitted \
                 or untracked work: left in place"
            );
            return;
        }
        let merged = Merged {
            pr: number,
            head: pr.head.clone(),
            resolved: false,
        };
        let recorded = self
            .coordinating
            .book
            .update_worker(thread, |worker| worker.merged = Some(merged));
        if let Err(error) = recorded {
            crate::log!("worker {thread}: its merged PR was not recorded: {error}");
            return;
        }
        if let Err(error) = self.execute(Op::Park { thread }) {
            crate::log!(
                "worker {thread}: not parked for its merged PR: {}",
                error.message
            );
            return;
        }
        // Its worktree goes on the next pass, at once.
        self.coordinating.read_at = None;
    }

    /// The merged workers parked and not yet resolved, with what to remove.
    /// One opened again since is left to its next reading.
    fn retiring(&self) -> Vec<Retiring> {
        self.coordinating
            .book
            .iter()
            .flat_map(|coordination| &coordination.workers)
            .filter_map(|worker| {
                let merged = worker.merged.as_ref().filter(|merged| !merged.resolved)?;
                let (repo, worktree) = match self.engine.parked_workspace(worker.thread)? {
                    WorkspaceBinding::Worktree { repo, path } => (repo, Some(path)),
                    WorkspaceBinding::Main { checkout } => (checkout, None),
                };
                let branch = worktree
                    .as_deref()
                    .and_then(|path| self.engine.registry().branch_for(path))
                    .map(String::from);
                Some(Retiring {
                    thread: worker.thread,
                    repo,
                    worktree,
                    branch,
                    head: merged.head.clone(),
                })
            })
            .collect()
    }

    /// A merged worker whose worktree is gone is resolved: bound to the
    /// main checkout, out of its coordinator's Group, and its coordinator
    /// told. One opened again meanwhile, or whose binding could not be
    /// moved, is left to the next pass, which finds its worktree gone.
    fn resolve(&mut self, thread: ThreadId, cleanup: Cleanup) {
        let Cleanup::Removed { kept } = cleanup else {
            return;
        };
        if let Err(error) = self.engine.retire_worktree(thread) {
            crate::log!("worker {thread}: not resolved: {error}");
            return;
        }
        let recorded = self.coordinating.book.update_worker(thread, |worker| {
            if let Some(merged) = &mut worker.merged {
                merged.resolved = true;
            }
        });
        if let Err(error) = recorded {
            crate::log!("worker {thread}: its resolution was not recorded: {error}");
            return;
        }
        if self.engine.groups().of(thread).is_some() {
            if let Err(error) = self.engine.apply_group(GroupChange::Leave { thread }) {
                crate::log!("worker {thread}: kept its Group seat: {error}");
            }
        }
        let told = self.coordinating.book.iter().find_map(|coordination| {
            let worker = coordination.workers.iter().find(|w| w.thread == thread)?;
            let pr = worker.merged.as_ref()?.pr;
            let text = coordinator::merged_prompt(thread, &worker.task, pr, kept.as_deref());
            Some((coordination.coordinator, text))
        });
        let Some((coordinator, text)) = told else {
            return;
        };
        let told = self.prompt(&AgentPromptParams {
            thread: coordinator,
            text,
            wait: false,
            timeout_ms: None,
        });
        if let Err(error) = told {
            crate::log!(
                "coordinator {coordinator}: worker {thread}'s merge was not told: {}",
                error.message
            );
        }
    }

    /// Each worker whose PRs are still followed is sent what its PR asks
    /// that it was not sent before, as one prompt; feedback that could not
    /// be sent (a Decision waits, the agent cannot queue yet) is tried
    /// again on the next reading.
    fn send_feedback(&mut self, read: Vec<(ThreadId, PrFeedback)>) {
        for (thread, pr) in read {
            let Some(worker) = self.followed_worker(thread) else {
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
            let keys: Vec<String> = new.iter().map(|item| item.key.clone()).collect();
            let sent = self.prompt(&AgentPromptParams {
                thread,
                text,
                wait: false,
                timeout_ms: None,
            });
            match sent {
                Ok(()) => {
                    let recorded = self
                        .coordinating
                        .book
                        .update_worker(thread, |worker| worker.sent.extend(keys));
                    if let Err(error) = recorded {
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

/// Remove a merged worker's worktree, never forced, then its branch while
/// that is at the PR's head, so nothing on either is lost. Each refusal is
/// logged; a worktree already gone is gone.
fn clean_up(retiring: &Retiring) -> Cleanup {
    let Retiring {
        thread,
        repo,
        worktree,
        branch,
        head,
    } = retiring;
    if let Some(path) = worktree.as_deref().filter(|path| path.exists()) {
        if let Err(error) = crate::workspace::remove_worktree(repo, path) {
            crate::log!("worker {thread}: its worktree stays: {error}");
            return Cleanup::Refused;
        }
    }
    let Some(branch) = branch else {
        return Cleanup::Removed { kept: None };
    };
    let kept = match crate::workspace::delete_branch_at(repo, branch, head) {
        Ok(true) => None,
        Ok(false) => {
            crate::log!("worker {thread}: branch {branch} is kept: it is not at the PR's head");
            Some(branch.clone())
        }
        Err(error) => {
            crate::log!("worker {thread}: branch {branch} is kept: {error}");
            Some(branch.clone())
        }
    };
    Cleanup::Removed { kept }
}

#[cfg(test)]
mod tests;
