//! How a follower follows one Delegate (T3 Code's `follow` and
//! `followOnce` in `peer/RemoteDelegation.ts` at 0fd48228), with no I/O: it
//! names the next call to make there, hears how that went, and says what
//! to record. The Server's follower thread makes the calls.
//!
//! It looks at the Thread there first, so the record shows it running or
//! waiting on a Decision, then waits for its turn to end in pieces of at
//! most 50 s, looking again between pieces: a Thread blocked on a Decision
//! is looked at once a piece, never in a loop (G-A2). Once the turn ends,
//! or the Thread there is parked, it reads the result (G-A4, G-A8).
//!
//! A machine that does not answer is asked again, backing off from the
//! first retry to five minutes, and never fails the Delegate. A Thread gone
//! there, a machine removed or turned off here, or another Server answering
//! is final: the Delegate fails with the reason. Answers it cannot read
//! three times running fail it too.

use std::time::Duration;

use ferrite_core::api::Outcome;
use ferrite_core::delegate::Status;
use ferrite_core::Attention;

/// The longest a retry waits (T3's `FOLLOW_MAX_BACKOFF`).
pub const MAX_BACKOFF: Duration = Duration::from_secs(5 * 60);
/// Unreadable answers in a row before the Delegate fails (T3's
/// `FOLLOW_MAX_DEFECTS`).
pub const MAX_DEFECTS: u32 = 3;
/// A Thread there that ended with no reply (T3's `noReply`, of a machine).
pub const NO_REPLY: &str = "(The thread on the linked machine ended without a reply.)";

/// The next call to make there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// `thread.get`: whether it is at work or waits on a Decision.
    Look,
    /// `agent.wait --turn-ended`, for one piece.
    Wait,
    /// `agent.result`: how its turn ended, and its reply.
    Result,
}

/// How a call there went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    Looked(Attention),
    /// Its turn ended.
    TurnEnded,
    /// The piece ran out, or the Thread closed while it ran.
    StillGoing,
    /// It is parked there.
    Parked,
    /// `agent.result`'s answer: no outcome while a turn runs, and none for
    /// a parked Thread whose turn never ended.
    Result {
        outcome: Option<Outcome>,
        reply: Option<String>,
        parked: bool,
    },
    /// Asking again may answer: the machine asleep, a sign-in, a refusal.
    Unanswered(String),
    /// Asking again cannot change it.
    Final(String),
    /// An answer this follower cannot read.
    Unreadable(String),
}

/// What to do with what was heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Then {
    /// Record it running (or waiting) and held up by nothing, then ask.
    Ask { status: Status, ask: Ask },
    /// Record why it is held up, wait out `after`, then ask the same again.
    Retry { problem: String, after: Duration },
    /// It ended there: record its result.
    End { status: Status, result: String },
}

pub struct Follow {
    machine: String,
    thread: String,
    status: Status,
    /// The first retry's wait, doubled for each failure in a row.
    first_retry: Duration,
    failures: u32,
    defects: u32,
}

impl Follow {
    /// Follow a Delegate whose Thread `thread` runs on `machine`, seen
    /// last at `status`, retrying after `first_retry` at first (T3's
    /// 2 s).
    pub fn new(machine: &str, thread: impl ToString, status: Status, first_retry: Duration) -> Self {
        Self {
            machine: machine.to_string(),
            thread: thread.to_string(),
            status,
            first_retry,
            failures: 0,
            defects: 0,
        }
    }

    pub fn first(&self) -> Ask {
        Ask::Look
    }

    /// What `heard`, the answer to `asked`, means.
    pub fn heard(&mut self, asked: Ask, heard: Heard) -> Then {
        let next = match heard {
            Heard::Unanswered(problem) => {
                self.defects = 0;
                return self.retry(problem);
            }
            Heard::Unreadable(problem) => {
                self.defects += 1;
                if self.defects >= MAX_DEFECTS {
                    return Then::End {
                        status: Status::Failed,
                        result: format!(
                            "Following the thread {} on {} failed repeatedly, so it may still run there (task_cancel stops it).",
                            self.thread, self.machine
                        ),
                    };
                }
                return self.retry(problem);
            }
            Heard::Final(reason) => {
                return Then::End {
                    status: Status::Failed,
                    result: reason,
                };
            }
            Heard::Looked(attention) => {
                self.status = match attention {
                    Attention::Blocked => Status::Waiting,
                    _ => Status::Running,
                };
                Ask::Wait
            }
            Heard::StillGoing => Ask::Look,
            Heard::TurnEnded | Heard::Parked => Ask::Result,
            Heard::Result {
                outcome,
                reply,
                parked,
            } => {
                let status = match (outcome, parked) {
                    (Some(Outcome::Completed), _) => Status::Completed,
                    (Some(Outcome::Failed), _) => Status::Failed,
                    (Some(Outcome::Interrupted), _) | (None, true) => Status::Interrupted,
                    // A turn began there since the wait: wait for it.
                    (None, false) => {
                        debug_assert_eq!(asked, Ask::Result);
                        self.ok();
                        return Then::Ask {
                            status: self.status,
                            ask: Ask::Look,
                        };
                    }
                };
                return Then::End {
                    status,
                    result: reply.unwrap_or_else(|| NO_REPLY.into()),
                };
            }
        };
        self.ok();
        Then::Ask {
            status: self.status,
            ask: next,
        }
    }

    fn ok(&mut self) {
        self.failures = 0;
        self.defects = 0;
    }

    fn retry(&mut self, problem: String) -> Then {
        let after = backoff(self.first_retry, self.failures);
        self.failures += 1;
        Then::Retry { problem, after }
    }
}

/// The wait before retry `n` (from 0): `first` doubled `n` times, at most
/// [`MAX_BACKOFF`] (T3's `Schedule.exponential` capped).
pub fn backoff(first: Duration, n: u32) -> Duration {
    first
        .checked_mul(1u32.checked_shl(n.min(31)).unwrap_or(u32::MAX))
        .unwrap_or(MAX_BACKOFF)
        .min(MAX_BACKOFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RETRY: Duration = Duration::from_secs(2);

    fn follow() -> Follow {
        Follow::new("pc", 4, Status::Starting, RETRY)
    }

    fn asks(status: Status, ask: Ask) -> Then {
        Then::Ask { status, ask }
    }

    #[test]
    fn it_looks_then_waits_and_looks_between_pieces() {
        let mut follow = follow();
        assert_eq!(follow.first(), Ask::Look);
        assert_eq!(
            follow.heard(Ask::Look, Heard::Looked(Attention::Working)),
            asks(Status::Running, Ask::Wait)
        );
        assert_eq!(follow.heard(Ask::Wait, Heard::StillGoing), asks(Status::Running, Ask::Look));
        // Blocked is seen once a piece, and the wait goes on.
        assert_eq!(
            follow.heard(Ask::Look, Heard::Looked(Attention::Blocked)),
            asks(Status::Waiting, Ask::Wait)
        );
        assert_eq!(follow.heard(Ask::Wait, Heard::TurnEnded), asks(Status::Waiting, Ask::Result));
    }

    #[test]
    fn the_result_ends_it_with_its_turns_outcome() {
        let ended = |outcome, reply: Option<&str>, parked| {
            follow().heard(
                Ask::Result,
                Heard::Result {
                    outcome,
                    reply: reply.map(str::to_string),
                    parked,
                },
            )
        };
        assert_eq!(
            ended(Some(Outcome::Completed), Some("Done."), false),
            Then::End {
                status: Status::Completed,
                result: "Done.".into()
            }
        );
        assert_eq!(
            ended(Some(Outcome::Failed), Some("API error"), false),
            Then::End {
                status: Status::Failed,
                result: "API error".into()
            }
        );
        assert_eq!(
            ended(None, None, true),
            Then::End {
                status: Status::Interrupted,
                result: NO_REPLY.into()
            }
        );
        // A turn that began since the wait is waited for.
        assert_eq!(ended(None, None, false), asks(Status::Starting, Ask::Look));
    }

    #[test]
    fn a_parked_thread_there_is_read_not_failed() {
        let mut follow = follow();
        assert_eq!(follow.heard(Ask::Wait, Heard::Parked), asks(Status::Starting, Ask::Result));
        assert_eq!(follow.heard(Ask::Look, Heard::Parked), asks(Status::Starting, Ask::Result));
    }

    #[test]
    fn an_unanswered_call_backs_off_to_five_minutes_and_never_fails() {
        let mut follow = follow();
        let mut waits = Vec::new();
        for _ in 0..10 {
            match follow.heard(Ask::Wait, Heard::Unanswered("asleep".into())) {
                Then::Retry { problem, after } => {
                    assert_eq!(problem, "asleep");
                    waits.push(after.as_secs());
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(waits, vec![2, 4, 8, 16, 32, 64, 128, 256, 300, 300]);
        // An answer starts the count again.
        follow.heard(Ask::Wait, Heard::StillGoing);
        assert_eq!(
            follow.heard(Ask::Look, Heard::Unanswered("asleep".into())),
            Then::Retry {
                problem: "asleep".into(),
                after: RETRY
            }
        );
        assert_eq!(backoff(RETRY, 40), MAX_BACKOFF);
    }

    #[test]
    fn a_final_answer_fails_it_with_the_reason() {
        assert_eq!(
            follow().heard(Ask::Wait, Heard::Final("unknown thread".into())),
            Then::End {
                status: Status::Failed,
                result: "unknown thread".into()
            }
        );
    }

    #[test]
    fn three_unreadable_answers_in_a_row_fail_it() {
        let mut follow = follow();
        for _ in 0..2 {
            assert!(matches!(
                follow.heard(Ask::Result, Heard::Unreadable("garbled".into())),
                Then::Retry { .. }
            ));
        }
        assert_eq!(
            follow.heard(Ask::Result, Heard::Unreadable("garbled".into())),
            Then::End {
                status: Status::Failed,
                result: "Following the thread 4 on pc failed repeatedly, so it may still run there (task_cancel stops it).".into()
            }
        );
    }
}
