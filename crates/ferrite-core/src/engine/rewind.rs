//! Rewind, Edit and Fork (docs/research/edit-rewind.md): back to before a
//! prompt, in place or as a new Thread. The agent is asked; its answer,
//! read on the pump, cuts the transcript and the log, hands an Edit's
//! prompt back to the Composer, or opens the fork.

use super::*;
use crate::providers::{Cut, FilesRestored, ForkAck, Forked, RewindAck, RewindRequest, Rewound};
use crate::store::ResumePoint;

/// How long an agent has to answer a rewind or a fork.
const REWIND_TIMEOUT: Duration = Duration::from_secs(30);

/// A rewind or fork waiting on its agent.
pub(super) struct Rewinding {
    request: RewindRequest,
    /// The prompt's words: an Edit's Composer text, and how a Notice
    /// names it.
    text: String,
    /// How many prompts the cut takes, it among them.
    dropped: u64,
    fork: bool,
    edit: bool,
    /// What the agent was asked, once it could be.
    asked: Option<(Answer, Instant)>,
}

enum Answer {
    Rewind(RewindAck),
    Fork(ForkAck),
}

enum Outcome {
    Rewound(Rewound),
    Forked(Forked),
}

impl Engine {
    /// Go back to before the prompt `prompt` names: it and everything after
    /// it leave the conversation, its files restored first when `files`,
    /// and its words handed back to the Composer when `edit`. Refused, in
    /// words, when it cannot be asked now; the agent's own answer arrives
    /// as the cut or a Notice.
    pub fn rewind(
        &mut self,
        thread: ThreadId,
        prompt: &str,
        files: bool,
        edit: bool,
    ) -> Result<(), String> {
        self.ask_rewind(thread, prompt, files, edit, false)
    }

    /// A new Thread holding `thread`'s conversation up to the prompt
    /// `prompt` names, without it; it opens once the agent says where its
    /// conversation comes from.
    pub fn fork(&mut self, thread: ThreadId, prompt: &str) -> Result<(), String> {
        self.ask_rewind(thread, prompt, false, false, true)
    }

    fn ask_rewind(
        &mut self,
        thread: ThreadId,
        prompt: &str,
        files: bool,
        edit: bool,
        fork: bool,
    ) -> Result<(), String> {
        let state = self
            .threads
            .get(&thread)
            .ok_or_else(|| "that Thread is not open".to_string())?;
        if let Some(why) = refusal(state) {
            return Err(why.into());
        }
        let (text, dropped, latest) = target(state.transcript(), prompt)?;
        let request = RewindRequest {
            prompt: prompt.into(),
            latest,
            files,
            session: state.resume.clone(),
        };
        if !self.start_or_say(thread, if fork { "fork failed" } else { "rewind failed" }) {
            return Err("its agent could not start".into());
        }
        let state = self.threads.get_mut(&thread).expect("checked");
        state.rewinding = Some(Rewinding {
            request,
            text,
            dropped,
            fork,
            edit,
            asked: None,
        });
        Ok(())
    }

    /// Ask the agents that can now take their rewind or fork, and carry out
    /// the answers that came back.
    pub(super) fn settle_rewinds(&mut self) -> Vec<PaneUpdate> {
        let waiting: Vec<ThreadId> = self
            .threads
            .iter()
            .filter(|(_, state)| state.rewinding.is_some())
            .map(|(id, _)| *id)
            .collect();
        let mut settled = Vec::new();
        for id in waiting {
            let Some(outcome) = self.poll_rewind(id) else {
                continue;
            };
            let mut update = PaneUpdate::new(id);
            update.activity_changed = true;
            let state = self.threads.get_mut(&id).expect("listed above");
            let pending = state.rewinding.take().expect("listed above");
            let noun = if pending.fork { "fork" } else { "rewind" };
            match outcome {
                Err(error) => {
                    state.apply(Input::Notice(format!("{noun} failed: {error}")));
                }
                Ok(Outcome::Rewound(rewound)) => {
                    self.finish_rewind(id, pending, rewound, &mut update)
                }
                Ok(Outcome::Forked(forked)) => self.finish_fork(id, pending, forked),
            }
            settled.push(update);
        }
        settled
    }

    /// The answer `thread`'s rewind has come to, if it has: asked of its
    /// agent once that is ready, then read off the ack.
    fn poll_rewind(&mut self, thread: ThreadId) -> Option<io::Result<Outcome>> {
        let state = self.threads.get_mut(&thread).expect("checked by caller");
        let pending = state.rewinding.as_ref().expect("checked by caller");
        if pending.asked.is_none() {
            if state.session.is_none() && state.replacement.is_none() {
                return Some(Err(io::Error::other("its agent stopped")));
            }
            let (request, fork) = (pending.request.clone(), pending.fork);
            let session = state.ready_session()?;
            let asked = if fork {
                session.fork(request).map(Answer::Fork)
            } else {
                session.rewind(request).map(Answer::Rewind)
            };
            match asked {
                Ok(answer) => {
                    let pending = state.rewinding.as_mut().expect("checked by caller");
                    pending.asked = Some((answer, crate::clock::instant()));
                }
                Err(error) => return Some(Err(error)),
            }
        }
        let pending = state.rewinding.as_ref().expect("checked by caller");
        let (answer, asked) = pending.asked.as_ref().expect("asked above");
        let answered = match answer {
            Answer::Rewind(ack) => ack.try_recv().map(|answer| answer.map(Outcome::Rewound)),
            Answer::Fork(ack) => ack.try_recv().map(|answer| answer.map(Outcome::Forked)),
        };
        match answered {
            Ok(answer) => Some(answer),
            Err(std::sync::mpsc::TryRecvError::Empty)
                if crate::clock::instant().duration_since(*asked) < REWIND_TIMEOUT =>
            {
                None
            }
            Err(_) => Some(Err(io::Error::other("its agent did not answer"))),
        }
    }

    /// The agent cut, or will on its next start, or refused: the transcript
    /// and the log follow a cut, recall and the suggestion forget what it
    /// took, and an Edit's prompt goes back to the Composer.
    fn finish_rewind(
        &mut self,
        thread: ThreadId,
        pending: Rewinding,
        rewound: Rewound,
        update: &mut PaneUpdate,
    ) {
        let state = self.threads.get_mut(&thread).expect("checked by caller");
        let files = rewound.files.as_ref().map(files_restored);
        match rewound.cut {
            Cut::Refused(reason) => {
                let kept = files
                    .map(|files| format!("; {files} stay restored"))
                    .unwrap_or_default();
                state.apply(Input::Notice(format!("rewind refused: {reason}{kept}")));
                return;
            }
            Cut::InPlace => {}
            Cut::ResumeAt { session, at } => {
                let point = ResumePoint {
                    session: session.clone(),
                    at: Some(at),
                    fork: false,
                };
                if let Err(error) = state.writer.record_resume_at(&point) {
                    state.apply(Input::Notice(format!("rewind failed: {error}")));
                    return;
                }
                // The next Session takes the conversation up before the
                // prompt; this one still holds all of it.
                state.resume = Some(session);
                state.resume_at = Some(point);
                state.end_session();
            }
        }
        if let Err(error) = state
            .writer
            .record_rewind(&pending.request.prompt, pending.dropped)
        {
            state.report_store_error(error);
        }
        let cut = state.apply(Input::Rewind {
            prompt: pending.request.prompt.clone(),
            dropped: pending.dropped,
        });
        update.dirty.extend(cut.dirty);
        update.evicted.extend(cut.evicted);
        state.prompt_history.forget_latest(pending.dropped as usize);
        state.invalidate_suggestion();
        if pending.edit {
            state
                .native_queue
                .recovered
                .push((pending.text.clone(), false));
        }
        let restored = files
            .map(|files| format!(" · {files} restored"))
            .unwrap_or_default();
        state.apply(Input::Notice(format!(
            "rewound to before \u{201c}{}\u{201d}{restored}",
            title_from_prompt(&pending.text)
        )));
    }

    /// The agent said where the fork's conversation comes from: the new
    /// Thread is written from this one's log and opened.
    fn finish_fork(&mut self, thread: ThreadId, pending: Rewinding, forked: Forked) {
        let native = match forked {
            Forked::Thread(session) => Some(ResumePoint {
                session,
                at: None,
                fork: false,
            }),
            Forked::ResumeAt { session, at } => Some(ResumePoint {
                session,
                at: Some(at),
                fork: true,
            }),
            Forked::Fresh => None,
        };
        let flushed = self
            .threads
            .get_mut(&thread)
            .expect("checked by caller")
            .writer
            .flush();
        let made = flushed
            .map_err(LoadError::Io)
            .and_then(|()| {
                self.store
                    .fork(thread, &pending.request.prompt, native.as_ref())
            })
            .and_then(|fork| self.revive(fork));
        let state = self.threads.get_mut(&thread).expect("checked by caller");
        state.apply(Input::Notice(match made {
            Ok(()) => format!(
                "forked into a new Thread before \u{201c}{}\u{201d}",
                title_from_prompt(&pending.text)
            ),
            Err(error) => format!("fork failed: {error}"),
        }));
    }
}

/// Why `state` cannot be rewound or forked now, if it cannot: the cut
/// never races a turn, a queue or a question.
fn refusal(state: &Thread) -> Option<&'static str> {
    if state.rewinding.is_some() {
        return Some("a rewind is already under way");
    }
    if state.busy() || state.activity.view().main_operator_turn() {
        return Some("a turn is running; stop it first");
    }
    if state.native_queue.pending() || !state.awaiting.prompts.is_empty() {
        return Some("prompts are queued; send or take them back first");
    }
    if !state.activity.view().decisions().is_empty() {
        return Some("a request waits for an answer");
    }
    if state.replacement.is_some() || state.tuning.is_some() {
        return Some("its agent is changing; try again in a moment");
    }
    None
}

/// The prompt `prompt` names in `transcript`: its words, how many prompts
/// a cut there takes, and the id of the latest prompt the operator saw.
fn target(
    transcript: &crate::transcript::Transcript,
    prompt: &str,
) -> Result<(String, u64, Option<String>), String> {
    let blocks = transcript.blocks();
    let at = blocks
        .iter()
        .rposition(|block| block.prompt_id.as_deref() == Some(prompt))
        .ok_or_else(|| "that prompt can no longer be rewound to".to_string())?;
    let crate::transcript::Body::Prompt(text) = &blocks[at].body else {
        return Err("that is not a prompt".into());
    };
    let prompts = || {
        blocks
            .iter()
            .filter(|block| matches!(block.body, crate::transcript::Body::Prompt(_)))
    };
    let dropped = prompts().filter(|block| block.id >= blocks[at].id).count() as u64;
    let latest = prompts().last().and_then(|block| block.prompt_id.clone());
    Ok((text.clone(), dropped, latest))
}

/// A restore as a Notice says it: `2 files, +3 −7`.
fn files_restored(files: &FilesRestored) -> String {
    let noun = if files.files == 1 { "file" } else { "files" };
    format!(
        "{} {noun}, +{} \u{2212}{}",
        files.files, files.insertions, files.deletions
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_restore_reads_as_files_and_lines() {
        let files = FilesRestored {
            files: 2,
            insertions: 3,
            deletions: 7,
        };
        assert_eq!(files_restored(&files), "2 files, +3 \u{2212}7");
    }
}
