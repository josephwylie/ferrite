//! The API's Projects (Herdr's Spaces), worktrees, saved machines
//! and the nav's focus: what herdr-projects asks of Herdr beyond Threads
//! (ADR 0030). A Project is the registry's: keyed by its directories, it
//! is named on the wire by its number as a string (`api::project_id`) and
//! labelled by its title.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::super::Server;
use crate::api::schema::{NavFocusParams, ProjectOpenParams, WorktreeCreateParams};
use crate::api::{self, ApiError, Code};
use crate::protocol::{Done, Op, ProjectAction, ServerMessage};
use crate::settings::Settings;
use crate::workspace;
use crate::workspace::registry::ProjectId;
use crate::ThreadId;

fn failed(error: impl ToString) -> ApiError {
    ApiError::new(Code::Failed, error.to_string())
}

impl Server {
    /// The Project `project.list` names `id`.
    pub(super) fn project(&self, id: &str) -> Result<ProjectId, ApiError> {
        self.engine
            .registry()
            .projects()
            .iter()
            .map(|project| project.id)
            .find(|project| api::project_id(*project) == id)
            .ok_or_else(|| ApiError::new(Code::ProjectNotFound, "no Project has that id"))
    }

    /// `{"project": …}`: what every operation on one Project answers.
    pub(super) fn project_result(&self, project: ProjectId) -> Result<Value, ApiError> {
        let id = api::project_id(project);
        api::project_infos(&self.engine)
            .into_iter()
            .find(|info| info.id == id)
            .map(|info| json!({ "project": info }))
            .ok_or_else(|| ApiError::new(Code::ProjectNotFound, "no Project has that id"))
    }

    /// The directory of `project` holding the canonical `cwd`, or `cwd`
    /// itself when it is a linked worktree of one of them. Anywhere else is
    /// not the Project's.
    pub(super) fn directory_in(&self, project: ProjectId, cwd: &Path) -> Result<PathBuf, ApiError> {
        let entry = self
            .engine
            .registry()
            .project(project)
            .ok_or_else(|| ApiError::new(Code::ProjectNotFound, "no Project has that id"))?;
        if let Some(directory) = entry
            .directories()
            .filter(|directory| cwd.starts_with(directory))
            .max_by_key(|directory| directory.components().count())
        {
            return Ok(directory.to_path_buf());
        }
        let worktree_of_project = workspace::checkout_of(cwd).is_some_and(|checkout| {
            checkout.linked
                && entry
                    .directories()
                    .any(|directory| directory == checkout.repo)
        });
        if worktree_of_project {
            return Ok(cwd.to_path_buf());
        }
        Err(ApiError::new(
            Code::InvalidRequest,
            format!("{} is not in that Project's directories", cwd.display()),
        ))
    }

    /// The Project holding `cwd` — or, for a linked worktree, holding its
    /// repository — else `cwd` registered as one, with `label`.
    pub(super) fn open_project(&mut self, params: ProjectOpenParams) -> Result<Value, ApiError> {
        let cwd = std::fs::canonicalize(&params.cwd).map_err(failed)?;
        let registry = self.engine.registry();
        let found = registry.containing(&cwd).or_else(|| {
            let checkout = workspace::checkout_of(&cwd).filter(|checkout| checkout.linked)?;
            registry.containing(&checkout.repo)
        });
        let project = match found {
            Some(project) => project,
            None => {
                let Some(Done::Project { project }) = self.execute(Op::Project {
                    action: ProjectAction::Register { roots: vec![cwd] },
                })?
                else {
                    return Err(failed("no Project was registered"));
                };
                if let Some(title) = params.label {
                    self.execute(Op::Project {
                        action: ProjectAction::Rename { project, title },
                    })?;
                }
                project
            }
        };
        if params.focus {
            if let Some(&thread) = self.threads_of(project).first() {
                self.say(ServerMessage::Focus { thread });
            }
        }
        self.project_result(project)
    }

    /// Park every open Thread of `project`, as closing their Panes does.
    pub(super) fn close_project(&mut self, project: ProjectId) -> Result<Value, ApiError> {
        let threads = self.threads_of(project);
        for &thread in &threads {
            self.execute(Op::Park { thread })?;
        }
        let mut answer = self.project_result(project)?;
        answer["parked"] = json!(threads);
        Ok(answer)
    }

    /// The open Threads of `project`, in the order they opened.
    fn threads_of(&self, project: ProjectId) -> Vec<ThreadId> {
        self.engine
            .open_threads()
            .iter()
            .copied()
            .filter(|thread| self.engine.project_id(*thread) == Some(project))
            .collect()
    }

    /// A new worktree of `cwd`'s repository on `branch`, from `base`, for
    /// a Thread to start in (`thread.start --cwd` its path), as Herdr's
    /// `worktree create` makes one for its pane.
    pub(super) fn create_worktree(
        &mut self,
        params: WorktreeCreateParams,
    ) -> Result<Value, ApiError> {
        let repo = workspace::checkout_of(&params.cwd)
            .ok_or_else(|| ApiError::new(Code::InvalidRequest, "cwd is not in a git checkout"))?
            .repo;
        let (project, path) = self
            .engine
            .create_worktree(&repo, &params.branch, params.base.as_deref())
            .map_err(failed)?;
        Ok(json!({
            "worktree": { "path": path, "branch": params.branch },
            "project": api::project_id(project),
        }))
    }

    /// Remove the worktree at `path`, as Herdr's `worktree remove` does:
    /// never forced, so a tree with modified or untracked files is refused
    /// and left whole; otherwise every open Thread in it is parked, the
    /// worktree removed, and its Threads bound to the repository's main
    /// checkout. The branch stays: it keeps the work reachable.
    pub(super) fn remove_worktree(&mut self, path: &Path) -> Result<Value, ApiError> {
        let not_found = || {
            ApiError::new(
                Code::WorktreeNotFound,
                format!("no linked worktree is at {}", path.display()),
            )
        };
        let checkout = workspace::checkout_of(path)
            .filter(|checkout| checkout.linked)
            .ok_or_else(not_found)?;
        if !workspace::is_clean(path).map_err(failed)? {
            return Err(ApiError::new(
                Code::DirtyWorktreeRequiresForce,
                format!(
                    "{} has modified or untracked files; nothing was removed",
                    path.display()
                ),
            ));
        }
        let threads = self.engine.threads_in_worktree(path).map_err(failed)?;
        for &thread in &threads {
            if self.engine.thread(thread).is_some() {
                self.execute(Op::Park { thread })?;
            }
        }
        workspace::remove_worktree(&checkout.repo, path).map_err(failed)?;
        for &thread in &threads {
            self.engine.retire_worktree(thread).map_err(failed)?;
        }
        self.engine.forget_worktree(path).map_err(failed)?;
        Ok(json!({ "removed": path, "parked": threads }))
    }

    /// The saved machines a command can ask instead (`--machine`), each by
    /// its name; one turned off is left out, since it is never asked.
    pub(super) fn machines(&self) -> Result<Value, ApiError> {
        let settings = Settings::load(&self.settings_dir).map_err(failed)?;
        let machines: Vec<Value> = settings
            .machines
            .iter()
            .filter(|machine| machine.enabled)
            .map(|machine| {
                json!({ "id": machine.name, "label": machine.name, "target": machine.target })
            })
            .collect();
        Ok(json!({ "machines": machines }))
    }

    /// Every window's nav shows only one Projects project's block, or all
    /// again, until told otherwise: a window that arrives later is told on
    /// its welcome. The slug is Projects' own; the Server passes it on
    /// unread.
    pub(super) fn focus_nav(&mut self, params: NavFocusParams) -> Result<Value, ApiError> {
        self.nav_focus = params.project.clone();
        let windows = self.windows();
        self.say(ServerMessage::NavFocus {
            project: params.project.clone(),
        });
        Ok(json!({ "project": params.project, "windows": windows }))
    }
}
