//! Rewind and fork over Codex's app-server (docs/research/edit-rewind.md).
//!
//! Codex names turns, not prompts, so every prompt goes out under
//! Ferrite's id as `clientUserMessageId`, and its `userMessage` item keeps
//! it as `clientId`. A rewind or a fork that names a prompt first finds
//! that prompt's turn by paging `thread/turns/list` newest first, then:
//!
//! - a rewind is `thread/revert` before that turn: the server replaces the
//!   loaded thread's durable history with the prefix, so the cut is in
//!   place. Codex keeps no file checkpoints, so a rewind never restores
//!   files (`ControlKind::RewindFiles` stays unoffered);
//! - a fork is `thread/fork` before that turn (`beforeTurnId`, an
//!   experimental field the handshake's `experimentalApi` opens; the whole
//!   thread for a whole fork), then `thread/unsubscribe` of the new thread,
//!   so its own Session can take it up as the writer.
//!
//! A prompt steered into a turn that was already running shares that turn
//! with the prompt that started it; Codex cannot cut between them, so a
//! rewind to it is refused in words.

use std::io;
use std::sync::mpsc::SyncSender;

use serde_json::{json, Value};

use crate::providers::{Cut, ForkRequest, Forked, RewindRequest, Rewound};

/// Turns per page while a prompt's turn is looked for.
const PAGE_TURNS: u64 = 20;
/// Pages read before the prompt counts as gone: 1,000 turns back.
const MAX_PAGES: usize = 50;

/// The rewind or fork in flight.
#[derive(Default)]
pub(super) struct Rewinds {
    next: u64,
    pending: Option<Pending>,
}

struct Pending {
    /// The request whose answer moves it on.
    request_id: String,
    thread: String,
    ask: Ask,
    stage: Stage,
}

enum Ask {
    Rewind {
        request: RewindRequest,
        reply: SyncSender<io::Result<Rewound>>,
    },
    Fork {
        request: ForkRequest,
        reply: SyncSender<io::Result<Forked>>,
    },
}

enum Stage {
    /// Paging newest first for the prompt's turn.
    Locate {
        pages: usize,
    },
    Revert,
    Fork,
    /// The fork is made; its new thread is being let go.
    Release {
        forked: String,
    },
}

/// What a lookup page says.
enum Located {
    /// The turn the prompt started.
    Started(String),
    /// Steered into a turn another prompt started.
    Joined,
    NotHere,
}

impl Rewinds {
    /// Start a rewind of `thread`: the line to write.
    pub(super) fn rewind(
        &mut self,
        thread: &str,
        request: RewindRequest,
        reply: SyncSender<io::Result<Rewound>>,
    ) -> Value {
        self.begin(thread, Ask::Rewind { request, reply })
    }

    /// Start a fork of `thread`: the line to write.
    pub(super) fn fork(
        &mut self,
        thread: &str,
        request: ForkRequest,
        reply: SyncSender<io::Result<Forked>>,
    ) -> Value {
        self.begin(thread, Ask::Fork { request, reply })
    }

    /// A newer ask takes the place of one still unanswered, whose ack then
    /// reads as never answered.
    fn begin(&mut self, thread: &str, ask: Ask) -> Value {
        let whole = matches!(&ask, Ask::Fork { request, .. } if request.before.is_none());
        let mut pending = Pending {
            request_id: String::new(),
            thread: thread.to_string(),
            ask,
            stage: if whole {
                Stage::Fork
            } else {
                Stage::Locate { pages: 0 }
            },
        };
        let line = if whole {
            self.request(
                &mut pending,
                "thread/fork",
                json!({"threadId": thread, "excludeTurns": true}),
            )
        } else {
            self.page(&mut pending, None)
        };
        self.pending = Some(pending);
        line
    }

    /// Take in this ask's answer (`owns`): the next line to write while
    /// the ask goes on, nothing once it is answered.
    pub(super) fn observe(&mut self, frame: &Value) -> Option<Value> {
        if !self.owns(frame) {
            return None;
        }
        let mut pending = self.pending.take()?;
        if let Some(error) = frame.get("error") {
            let message = error["message"].as_str().unwrap_or("Codex refused");
            pending.refuse(message.to_string());
            return None;
        }
        let result = &frame["result"];
        match std::mem::replace(&mut pending.stage, Stage::Locate { pages: 0 }) {
            Stage::Locate { pages } => self.located(pending, result, pages + 1),
            Stage::Revert => {
                pending.answer_rewind(Ok(Rewound {
                    cut: Cut::InPlace,
                    files: None,
                }));
                None
            }
            Stage::Fork => match result["thread"]["id"].as_str() {
                Some(forked) => {
                    let forked = forked.to_string();
                    pending.stage = Stage::Release {
                        forked: forked.clone(),
                    };
                    let line = self.request(
                        &mut pending,
                        "thread/unsubscribe",
                        json!({"threadId": forked}),
                    );
                    self.pending = Some(pending);
                    Some(line)
                }
                None => {
                    pending.fail(io::Error::other("Codex's fork named no thread"));
                    None
                }
            },
            // Let go or not, the fork is made: its Session resumes it.
            Stage::Release { forked } => {
                pending.answer_fork(Ok(Forked::Thread(forked)));
                None
            }
        }
    }

    /// Whether `frame` answers the ask in flight.
    pub(super) fn owns(&self, frame: &Value) -> bool {
        self.pending.as_ref().is_some_and(|pending| {
            frame.get("method").is_none() && frame["id"].as_str() == Some(&pending.request_id)
        })
    }

    /// One lookup page answered: act on the prompt's turn, read on, or
    /// refuse.
    fn located(&mut self, mut pending: Pending, result: &Value, pages: usize) -> Option<Value> {
        let turns = result["data"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        let cursor = result["nextCursor"].as_str();
        let turn = match locate(turns, pending.prompt()) {
            Located::Started(turn) => turn,
            Located::Joined => {
                pending.refuse(
                    "that prompt joined a turn already running; \
                     rewind to the prompt that started it"
                        .into(),
                );
                return None;
            }
            Located::NotHere if cursor.is_some() && pages < MAX_PAGES => {
                pending.stage = Stage::Locate { pages };
                let line = self.page(&mut pending, cursor);
                self.pending = Some(pending);
                return Some(line);
            }
            Located::NotHere => {
                pending.refuse("Codex no longer has that prompt".into());
                return None;
            }
        };
        let thread = pending.thread.clone();
        let line = match &pending.ask {
            Ask::Rewind { .. } => {
                pending.stage = Stage::Revert;
                self.request(
                    &mut pending,
                    "thread/revert",
                    json!({"threadId": thread, "beforeTurnId": turn}),
                )
            }
            Ask::Fork { .. } => {
                pending.stage = Stage::Fork;
                self.request(
                    &mut pending,
                    "thread/fork",
                    json!({"threadId": thread, "beforeTurnId": turn, "excludeTurns": true}),
                )
            }
        };
        self.pending = Some(pending);
        Some(line)
    }

    /// The next page of the thread's turns, newest first, items and all
    /// (a summary may leave out the `clientId`).
    fn page(&mut self, pending: &mut Pending, cursor: Option<&str>) -> Value {
        let mut params = json!({
            "threadId": pending.thread,
            "itemsView": "full",
            "sortDirection": "desc",
            "limit": PAGE_TURNS,
        });
        if let Some(cursor) = cursor {
            params["cursor"] = json!(cursor);
        }
        self.request(pending, "thread/turns/list", params)
    }

    fn request(&mut self, pending: &mut Pending, method: &str, params: Value) -> Value {
        self.next += 1;
        pending.request_id = format!("ferrite-rewind-{}", self.next);
        json!({"jsonrpc": "2.0", "id": pending.request_id, "method": method, "params": params})
    }

    /// The line written for the ask in flight could not be: it fails.
    pub(super) fn write_failed(&mut self, error: io::Error) {
        if let Some(pending) = self.pending.take() {
            pending.fail(error);
        }
    }
}

impl Pending {
    /// The prompt a lookup looks for.
    fn prompt(&self) -> &str {
        match &self.ask {
            Ask::Rewind { request, .. } => &request.prompt,
            Ask::Fork { request, .. } => request.before.as_deref().unwrap_or_default(),
        }
    }

    /// Codex said no, in its words: a rewind's refusal, a fork's failure.
    fn refuse(self, reason: String) {
        match self.ask {
            Ask::Rewind { reply, .. } => {
                let _ = reply.send(Ok(Rewound {
                    cut: Cut::Refused(reason),
                    files: None,
                }));
            }
            Ask::Fork { reply, .. } => {
                let _ = reply.send(Err(io::Error::other(reason)));
            }
        }
    }

    fn fail(self, error: io::Error) {
        match self.ask {
            Ask::Rewind { reply, .. } => {
                let _ = reply.send(Err(error));
            }
            Ask::Fork { reply, .. } => {
                let _ = reply.send(Err(error));
            }
        }
    }

    fn answer_rewind(self, answer: io::Result<Rewound>) {
        if let Ask::Rewind { reply, .. } = self.ask {
            let _ = reply.send(answer);
        }
    }

    fn answer_fork(self, answer: io::Result<Forked>) {
        if let Ask::Fork { reply, .. } = self.ask {
            let _ = reply.send(answer);
        }
    }
}

/// Where `prompt` is among `turns`: the turn whose first `userMessage`
/// carries it as `clientId`.
fn locate(turns: &[Value], prompt: &str) -> Located {
    for turn in turns {
        let mut messages = turn["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| item["type"] == "userMessage");
        let Some(first) = messages.next() else {
            continue;
        };
        if first["clientId"] == prompt {
            return match turn["id"].as_str() {
                Some(id) => Located::Started(id.to_string()),
                None => Located::NotHere,
            };
        }
        if messages.any(|item| item["clientId"] == prompt) {
            return Located::Joined;
        }
    }
    Located::NotHere
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{sync_channel, Receiver};

    /// The answers as the 0.166.0 app-server's JSON Schema shapes them
    /// (`codex app-server generate-json-schema`: `thread/turns/list`,
    /// `thread/revert`, `thread/fork`, `thread/unsubscribe`, and a refusal).
    /// Written from the schema; no thread was rewound to capture them.
    const FIXTURE: &str = include_str!("../../../tests/fixtures/codex-rewind-0.166.0.jsonl");

    /// Fixture line `index`, answering request `id`.
    fn answer(index: usize, id: &Value) -> Value {
        let mut frame: Value = serde_json::from_str(FIXTURE.lines().nth(index).unwrap()).unwrap();
        frame["id"] = id.clone();
        frame
    }

    fn rewind(prompt: &str) -> (Rewinds, Value, Receiver<io::Result<Rewound>>) {
        let mut rewinds = Rewinds::default();
        let (reply, ack) = sync_channel(1);
        let request = RewindRequest {
            prompt: prompt.into(),
            latest: None,
            files: false,
            session: Some("main".into()),
        };
        let line = rewinds.rewind("main", request, reply);
        (rewinds, line, ack)
    }

    fn fork(before: Option<&str>) -> (Rewinds, Value, Receiver<io::Result<Forked>>) {
        let mut rewinds = Rewinds::default();
        let (reply, ack) = sync_channel(1);
        let request = ForkRequest {
            before: before.map(str::to_string),
            session: Some("main".into()),
        };
        let line = rewinds.fork("main", request, reply);
        (rewinds, line, ack)
    }

    #[test]
    fn a_rewind_finds_its_turn_newest_first_then_reverts_before_it() {
        let (mut rewinds, line, ack) = rewind("p2");
        assert_eq!(line["method"], "thread/turns/list");
        assert_eq!(line["params"]["sortDirection"], "desc");
        assert_eq!(line["params"]["itemsView"], "full");
        let revert = rewinds.observe(&answer(0, &line["id"])).unwrap();
        assert_eq!(revert["method"], "thread/revert");
        assert_eq!(revert["params"]["threadId"], "main");
        assert_eq!(revert["params"]["beforeTurnId"], "t2");
        // The server's notice of it is no answer.
        assert!(!rewinds.owns(&answer(3, &revert["id"])));
        assert!(rewinds.observe(&answer(2, &revert["id"])).is_none());
        assert_eq!(ack.try_recv().unwrap().unwrap().cut, Cut::InPlace);
    }

    #[test]
    fn an_older_prompt_reads_the_next_page() {
        let (mut rewinds, line, ack) = rewind("p1");
        let page = rewinds.observe(&answer(0, &line["id"])).unwrap();
        assert_eq!(page["method"], "thread/turns/list");
        assert_eq!(page["params"]["cursor"], "c2");
        let revert = rewinds.observe(&answer(1, &page["id"])).unwrap();
        assert_eq!(revert["params"]["beforeTurnId"], "t1");
        assert!(ack.try_recv().is_err());
    }

    #[test]
    fn a_steered_prompt_and_a_lost_one_are_refused() {
        let (mut rewinds, line, ack) = rewind("p2b");
        assert!(rewinds.observe(&answer(0, &line["id"])).is_none());
        assert!(matches!(
            ack.try_recv().unwrap().unwrap().cut,
            Cut::Refused(reason) if reason.contains("joined a turn")
        ));
        let (mut rewinds, line, ack) = rewind("gone");
        let page = rewinds.observe(&answer(0, &line["id"])).unwrap();
        assert!(rewinds.observe(&answer(1, &page["id"])).is_none());
        assert_eq!(
            ack.try_recv().unwrap().unwrap().cut,
            Cut::Refused("Codex no longer has that prompt".into())
        );
    }

    #[test]
    fn a_refusal_reads_in_codexs_words() {
        let (mut rewinds, line, ack) = rewind("p3");
        let revert = rewinds.observe(&answer(0, &line["id"])).unwrap();
        assert_eq!(revert["params"]["beforeTurnId"], "t3");
        assert!(rewinds.observe(&answer(6, &revert["id"])).is_none());
        assert_eq!(
            ack.try_recv().unwrap().unwrap().cut,
            Cut::Refused("thread main has a turn in progress".into())
        );
    }

    /// A fork before a prompt forks before its turn, and lets the new
    /// thread go before the fork is answered; a whole fork asks at once.
    #[test]
    fn a_fork_forks_before_the_turn_and_lets_the_new_thread_go() {
        let (mut rewinds, line, ack) = fork(Some("p1"));
        let page = rewinds.observe(&answer(0, &line["id"])).unwrap();
        let made = rewinds.observe(&answer(1, &page["id"])).unwrap();
        assert_eq!(made["method"], "thread/fork");
        assert_eq!(
            made["params"],
            json!({"threadId": "main", "beforeTurnId": "t1", "excludeTurns": true})
        );
        let release = rewinds.observe(&answer(4, &made["id"])).unwrap();
        assert_eq!(release["method"], "thread/unsubscribe");
        assert_eq!(release["params"]["threadId"], "fork");
        assert!(ack.try_recv().is_err());
        assert!(rewinds.observe(&answer(5, &release["id"])).is_none());
        assert_eq!(
            ack.try_recv().unwrap().unwrap(),
            Forked::Thread("fork".into())
        );

        let (_, line, _) = fork(None);
        assert_eq!(line["method"], "thread/fork");
        assert_eq!(
            line["params"],
            json!({"threadId": "main", "excludeTurns": true})
        );
    }

    #[test]
    fn a_write_that_fails_fails_the_ask() {
        let (mut rewinds, _line, ack) = fork(Some("p3"));
        rewinds.write_failed(io::Error::other("pipe closed"));
        assert!(ack.try_recv().unwrap().is_err());
    }
}
