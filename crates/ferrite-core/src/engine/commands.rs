//! A client's acts, as protocol commands (ADR 0025): each one the Engine
//! carries out and answers. What only the Server can do — rereading
//! settings, searching a Session's files, stopping or restarting itself —
//! is the Server's own, not here.

use super::*;
use crate::protocol::{
    Done, ErrorCode, NoticeAction, Op, ProjectAction, WireAnswer, WireDirection, WireDissolved,
    WireError, WireHandle,
};

/// What a command came to: a result to reply `ok` with, or why not.
pub type Executed = Result<Option<Done>, WireError>;

fn refused(code: ErrorCode, message: impl ToString) -> WireError {
    WireError {
        code,
        message: message.to_string(),
    }
}

fn failed(error: impl ToString) -> WireError {
    refused(ErrorCode::Failed, error)
}

fn unknown() -> WireError {
    refused(ErrorCode::Unsupported, "this server does not know that")
}

impl Engine {
    pub fn execute(&mut self, op: &Op) -> Executed {
        match op {
            Op::Open {
                provider,
                workspace,
            } => {
                let workspace = workspace.try_into().map_err(|_| unknown())?;
                self.open(*provider, workspace)
                    .map(|thread| Some(Done::Thread { thread }))
                    .map_err(failed)
            }
            Op::Send { thread, text } => {
                self.open_thread(thread)?;
                self.send(*thread, text.clone());
                Ok(None)
            }
            Op::Queue { thread, text } => {
                if self.queue(*thread, text.clone()) {
                    Ok(None)
                } else {
                    Err(failed("the provider did not take the prompt"))
                }
            }
            Op::Unqueue { thread } => Ok(self.unqueue(*thread).map(|text| Done::Text { text })),
            Op::CancelQueued {
                thread,
                restore,
                edit,
            } => {
                if *edit {
                    self.edit_queued(*thread);
                } else {
                    self.cancel_queued(*thread, *restore);
                }
                Ok(None)
            }
            Op::TakeRecovered { thread } => Ok(self
                .take_retrieved_prompt(*thread)
                .map(|(text, prepend)| Done::Recovered { text, prepend })),
            Op::Recall {
                thread,
                direction,
                draft,
            } => {
                let direction = match direction {
                    WireDirection::Older => HistoryDirection::Older,
                    WireDirection::Newer => HistoryDirection::Newer,
                    WireDirection::Unknown => return Err(unknown()),
                };
                Ok(self
                    .recall_prompt(*thread, direction, draft)
                    .map(|text| Done::Text { text }))
            }
            Op::Interrupt { thread } => {
                self.open_thread(thread)?;
                self.interrupt(*thread);
                Ok(None)
            }
            Op::Answer {
                thread,
                handle,
                answer,
            } => self.answer_decision(*thread, handle, answer),
            Op::Control { thread, control } => {
                let control = control.try_into().map_err(|_| unknown())?;
                self.control(*thread, control)
                    .map(|()| None)
                    .map_err(failed)
            }
            Op::SetModel { thread, model } => self
                .set_model(*thread, model.clone())
                .map(|()| None)
                .map_err(failed),
            Op::SetEffort { thread, effort } => self
                .set_effort(*thread, effort.clone())
                .map(|()| None)
                .map_err(failed),
            Op::SetProvider {
                thread,
                provider,
                model,
            } => self
                .set_provider(
                    *thread,
                    ProviderChoice {
                        provider: *provider,
                        model: model.clone(),
                    },
                )
                .map(|()| None)
                .map_err(failed),
            Op::Rename { thread, title } => self
                .rename_thread(*thread, title)
                .map(|()| None)
                .map_err(failed),
            Op::Park { thread } => self.park(*thread).map(|()| None).map_err(failed),
            Op::Revive { thread } => self.revive(*thread).map(|()| None).map_err(failed),
            Op::ReviveGroup { group } => self
                .revive_group(*group)
                .map(|threads| Some(Done::Threads { threads }))
                .map_err(failed),
            Op::Delete { thread } => self.delete(*thread).map(|()| None).map_err(failed),
            Op::Import { path } => self
                .import(path)
                .map(|thread| Some(Done::Thread { thread }))
                .map_err(failed),
            Op::Group { change } => {
                let change = change.try_into().map_err(|_| unknown())?;
                self.apply_group(change)
                    .map(|applied| {
                        Some(Done::Grouped {
                            group: applied.group,
                            dissolved: applied
                                .dissolved
                                .iter()
                                .map(|dissolved| WireDissolved {
                                    group: dissolved.group,
                                    survivor: dissolved.survivor,
                                })
                                .collect(),
                        })
                    })
                    .map_err(failed)
            }
            Op::Layout { group, tree } => match tree {
                Some(tree) => self.set_group_layout(*group, tree.clone()),
                None => self.reset_group_layout(*group),
            }
            .map(|()| None)
            .map_err(failed),
            Op::Project { action } => self.project(action),
            Op::Notice { action } => {
                self.notice(action);
                Ok(None)
            }
            Op::CliReplaced { provider } => {
                self.restart_sessions(*provider);
                self.rediscover_models();
                Ok(None)
            }
            Op::WorktreesListed {
                thread,
                listed,
                age_ms,
            } => {
                let taken_at = crate::clock::instant()
                    .checked_sub(Duration::from_millis(*age_ms))
                    .unwrap_or(self.boot);
                let changed = self.worktrees_listed(*thread, listed.clone(), taken_at);
                Ok(Some(Done::Changed { changed }))
            }
            Op::LoadSubject {
                thread,
                subject,
                retry,
            } => {
                let subject = subject.try_into().map_err(|_| unknown())?;
                if *retry {
                    self.retry_subject_history(*thread, &subject)
                } else {
                    self.ensure_subject_history(*thread, &subject)
                }
                .map(|changed| Some(Done::Changed { changed }))
                .map_err(failed)
            }
            Op::ShowSubject { thread, subject } => {
                let subject = subject.try_into().map_err(|_| unknown())?;
                self.set_visible_subject(*thread, subject);
                Ok(None)
            }
            Op::Say { thread, text } => {
                self.open_thread(thread)?;
                self.apply_input(*thread, Input::Notice(text.clone()));
                Ok(None)
            }
            Op::SettingsChanged
            | Op::Bootstrap { .. }
            | Op::CancelBootstrap { .. }
            | Op::SearchFiles { .. }
            | Op::ListDir { .. }
            | Op::ListRepo { .. }
            | Op::ReviewDiff { .. }
            | Op::ListCommits { .. }
            | Op::SearchRepo { .. }
            | Op::ReadFile { .. }
            | Op::PullRequest { .. }
            | Op::Coordinate { .. }
            | Op::Restart { .. }
            | Op::Stop { .. }
            | Op::Unknown => Err(unknown()),
        }
    }

    /// A window's first send, with the scope its Draft gave it, tagged
    /// `tag` so its settling (`take_bootstrap_results`) names it. The
    /// Thread, or `None` while it starts.
    pub fn bootstrap_command(
        &mut self,
        tag: DraftId,
        op: &Op,
    ) -> Result<Option<ThreadId>, WireError> {
        let Op::Bootstrap {
            provider,
            model,
            effort,
            workspace,
            prompt,
            group,
            new_group_with,
        } = op
        else {
            return Err(unknown());
        };
        let workspace = workspace.try_into().map_err(|_| unknown())?;
        let choice = ProviderChoice {
            provider: *provider,
            model: model.clone(),
        };
        let scope = DraftScope {
            group: *group,
            new_group_with: *new_group_with,
            pending_leave: None,
        };
        self.bootstrap_draft(tag, scope, choice, workspace, prompt, effort.clone())
            .map_err(failed)
    }

    fn open_thread(&self, thread: &ThreadId) -> Result<(), WireError> {
        if self.threads.contains_key(thread) {
            Ok(())
        } else {
            Err(refused(ErrorCode::UnknownThread, "that Thread is not open"))
        }
    }

    /// The first valid answer wins: a Decision no longer pending was
    /// answered (or withdrawn) already. A request id is unique only within
    /// one Session, so the handle's generation must be this Session's.
    fn answer_decision(
        &mut self,
        thread: ThreadId,
        handle: &WireHandle,
        answer: &WireAnswer,
    ) -> Executed {
        let answer = answer.try_into().map_err(|_| unknown())?;
        let state = self
            .threads
            .get(&thread)
            .ok_or_else(|| refused(ErrorCode::UnknownThread, "that Thread is not open"))?;
        let handle = DecisionHandle::from(handle);
        let Some(pending) = state
            .activity
            .view()
            .decisions()
            .iter()
            .find(|pending| pending.handle == handle && !pending.submitting)
        else {
            return Err(refused(
                ErrorCode::AlreadyAnswered,
                "this request was already answered",
            ));
        };
        pending
            .decision
            .kind
            .validate_answer(&answer)
            .map_err(|reason| refused(ErrorCode::InvalidAnswer, reason))?;
        match self.respond_decision(thread, &handle, answer) {
            Ok(true) => Ok(None),
            Ok(false) => Err(refused(
                ErrorCode::AlreadyAnswered,
                "this request was already answered",
            )),
            // The provider refused what only it can judge (a choice it never
            // offered); the Decision is still pending.
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
                Err(refused(ErrorCode::InvalidAnswer, error))
            }
            Err(error) => Err(failed(error)),
        }
    }

    fn project(&mut self, action: &ProjectAction) -> Executed {
        let done = |project| Some(Done::Project { project });
        match action {
            ProjectAction::Register { roots } => self.register_project_directories(roots).map(done),
            ProjectAction::AddDirectories { project, roots } => {
                self.add_project_directories(*project, roots).map(|()| None)
            }
            ProjectAction::ReplaceDirectory {
                project,
                index,
                root,
            } => self
                .replace_project_directory(*project, *index, root)
                .map(|()| None),
            ProjectAction::RemoveDirectory { project, index } => self
                .remove_project_directory(*project, *index)
                .map(|()| None),
            ProjectAction::Remove { project } => self.remove_project(*project).map(|()| None),
            ProjectAction::Rename { project, title } => {
                self.rename_project(*project, title).map(|()| None)
            }
            ProjectAction::Unknown => return Err(unknown()),
        }
        .map_err(failed)
    }

    fn notice(&mut self, action: &NoticeAction) {
        let decision = |thread: &ThreadId, handle: &WireHandle| DecisionNoticeId {
            thread: *thread,
            handle: handle.into(),
        };
        match action {
            NoticeAction::Open { notice } => {
                self.open_notice(NoticeId::from_u64(*notice));
            }
            NoticeAction::Dismiss { notice } => {
                self.dismiss_notice(NoticeId::from_u64(*notice));
            }
            NoticeAction::OpenDecision { thread, handle } => {
                self.open_decision_notice(&decision(thread, handle));
            }
            NoticeAction::DismissDecision { thread, handle } => {
                self.dismiss_decision_notice(&decision(thread, handle));
            }
            NoticeAction::Read => self.mark_notices_read(),
            NoticeAction::Clear => self.clear_notices(),
            NoticeAction::Seen { thread } => {
                self.acknowledge_seen(&[*thread]);
            }
            NoticeAction::Unknown => {}
        }
    }
}
