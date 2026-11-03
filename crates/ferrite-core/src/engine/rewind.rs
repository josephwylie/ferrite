//! Rewind, Edit and Fork (docs/research/edit-rewind.md): back to before a
//! prompt, in place, or as a new Thread — which may also take the whole
//! conversation. The agent is asked; its answer, read on the pump, cuts the
//! transcript and the log, hands an Edit's prompt back to the Composer, or
//! opens the fork, focused, with its draft in its Composer.

use super::*;
use crate::providers::{
    Cut, FilesRestored, ForkAck, ForkRequest, Forked, RewindAck, RewindRequest, Rewound,
};
use crate::store::ResumePoint;

/// How long an agent has to answer a rewind or a fork.
const REWIND_TIMEOUT: Duration = Duration::from_secs(30);

/// A rewind or fork waiting on its agent.
pub(super) struct Rewinding {
    ask: Ask,
    /// The prompt's words: an Edit's Composer text, and how a Notice
    /// names it; `None` for a fork of the whole conversation.
    text: Option<String>,
    /// How many prompts the cut takes, it among them.
    dropped: u64,
    edit: bool,
    /// What the agent was asked, once it could be.
    asked: Option<(Answer, Instant)>,
}

/// What the agent is to be asked.
#[derive(Clone)]
enum Ask {
    Rewind(RewindRequest),
    /// A fork, with the words its Composer opens holding.
    Fork {
        request: ForkRequest,
        draft: Option<String>,
    },
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
        let state = self.cuttable(thread)?;
        let (text, dropped, latest) = target(state.transcript(), prompt)?;
        let request = RewindRequest {
            prompt: prompt.into(),
            latest,
            files,
            session: state.resume.clone(),
        };
        self.ask(
            thread,
            Rewinding {
                ask: Ask::Rewind(request),
                text: Some(text),
                dropped,
                edit,
                asked: None,
            },
        )
    }

    /// A new Thread holding `thread`'s conversation up to the prompt
    /// `prompt` names, without it — the whole of it for `None`; it opens,
    /// focused, once the agent says where its conversation comes from,
    /// `draft` waiting in its Composer.
    pub fn fork(
        &mut self,
        thread: ThreadId,
        prompt: Option<&str>,
        draft: Option<String>,
    ) -> Result<(), String> {
        let state = self.cuttable(thread)?;
        let (text, dropped) = match prompt {
            Some(prompt) => {
                let (text, dropped, _) = target(state.transcript(), prompt)?;
                (Some(text), dropped)
            }
            None => (None, 0),
        };
        let request = ForkRequest {
            before: prompt.map(str::to_string),
            session: state.resume.clone(),
        };
        self.ask(
            thread,
            Rewinding {
                ask: Ask::Fork { request, draft },
                text,
                dropped,
                edit: false,
                asked: None,
            },
        )
    }

    /// `thread`, when a rewind or a fork can be asked of it now.
    fn cuttable(&self, thread: ThreadId) -> Result<&Thread, String> {
        let state = self
            .threads
            .get(&thread)
            .ok_or_else(|| "that Thread is not open".to_string())?;
        match refusal(state) {
            Some(why) => Err(why.into()),
            None => Ok(state),
        }
    }

    /// Start `thread`'s agent if it must, and leave `pending` for the pump.
    fn ask(&mut self, thread: ThreadId, pending: Rewinding) -> Result<(), String> {
        if !self.start_or_say(thread, &format!("{} failed", pending.noun())) {
            return Err("its agent could not start".into());
        }
        let state = self.threads.get_mut(&thread).expect("checked");
        state.rewinding = Some(pending);
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
            match outcome {
                Err(error) => {
                    state.apply(Input::Notice(format!("{} failed: {error}", pending.noun())));
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
            let ask = pending.ask.clone();
            let session = state.ready_session()?;
            let asked = match ask {
                Ask::Fork { request, .. } => session.fork(request).map(Answer::Fork),
                Ask::Rewind(request) => session.rewind(request).map(Answer::Rewind),
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
        let Ask::Rewind(request) = &pending.ask else {
            unreachable!("a rewind's answer");
        };
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
        if let Err(error) = state.writer.record_rewind(&request.prompt, pending.dropped) {
            state.report_store_error(error);
        }
        let cut = state.apply(Input::Rewind {
            prompt: request.prompt.clone(),
            dropped: pending.dropped,
        });
        update.dirty.extend(cut.dirty);
        update.evicted.extend(cut.evicted);
        state.prompt_history.forget_latest(pending.dropped as usize);
        state.invalidate_suggestion();
        let text = pending.text.unwrap_or_default();
        if pending.edit {
            state.native_queue.recovered.push((text.clone(), false));
        }
        let restored = files
            .map(|files| format!(" · {files} restored"))
            .unwrap_or_default();
        state.apply(Input::Notice(format!(
            "rewound to before \u{201c}{}\u{201d}{restored}",
            title_from_prompt(&text)
        )));
    }

    /// The agent said where the fork's conversation comes from: the new
    /// Thread is written from this one's log and opened, its draft in its
    /// Composer, and the windows land on it (Claude opens a fork).
    fn finish_fork(&mut self, thread: ThreadId, pending: Rewinding, forked: Forked) {
        let Ask::Fork { request, draft } = pending.ask else {
            unreachable!("a fork's answer");
        };
        let native = match forked {
            Forked::Thread(session) => Some(ResumePoint {
                session,
                at: None,
                fork: false,
            }),
            Forked::ResumeAt { session, at } => Some(ResumePoint {
                session,
                at,
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
                    .fork(thread, request.before.as_deref(), native.as_ref())
            })
            .and_then(|fork| self.revive(fork).map(|()| fork));
        if let Ok(fork) = made {
            if let (Some(draft), Some(state)) = (draft, self.threads.get_mut(&fork)) {
                state.native_queue.recovered.push((draft, false));
            }
            self.outbox.push(ServerMessage::Focus { thread: fork });
        }
        let state = self.threads.get_mut(&thread).expect("checked by caller");
        state.apply(Input::Notice(match (made, &pending.text) {
            (Ok(_), Some(text)) => format!(
                "forked into a new Thread before \u{201c}{}\u{201d}",
                title_from_prompt(text)
            ),
            (Ok(_), None) => "forked into a new Thread".to_string(),
            (Err(error), _) => format!("fork failed: {error}"),
        }));
    }
}

impl Rewinding {
    /// How a Notice names this ask.
    fn noun(&self) -> &'static str {
        match self.ask {
            Ask::Rewind(_) => "rewind",
            Ask::Fork { .. } => "fork",
        }
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
