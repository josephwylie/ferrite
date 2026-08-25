//! What the Pane holds: either a live Claude Session or the scripted demo
//! stream. Both hand the pump the same `Receiver<SessionEvent>`, so `--demo`
//! exercises the real render path without spawning a CLI.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ferrite_core::providers::ClaudeSession;
use ferrite_core::{SessionEvent, TurnOutcome};

pub enum Session {
    Live(ClaudeSession),
    Demo(DemoSession),
}

impl Session {
    pub fn events(&self) -> &Receiver<SessionEvent> {
        match self {
            Session::Live(s) => s.events(),
            Session::Demo(d) => &d.rx,
        }
    }

    pub fn send(&mut self, text: &str) -> io::Result<()> {
        match self {
            Session::Live(s) => s.send(text),
            Session::Demo(d) => {
                d.send();
                Ok(())
            }
        }
    }

    pub fn interrupt(&mut self) -> io::Result<()> {
        match self {
            Session::Live(s) => s.interrupt(),
            Session::Demo(d) => {
                d.interrupt();
                Ok(())
            }
        }
    }
}

/// A scripted event stream: no process, same channel, same pump.
pub struct DemoSession {
    rx: Receiver<SessionEvent>,
    tx: Sender<SessionEvent>,
    cancel: Arc<AtomicBool>,
}

impl DemoSession {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        play(tx.clone(), cancel.clone(), script());
        Self { rx, tx, cancel }
    }

    fn send(&mut self) {
        self.cancel.store(false, Ordering::Relaxed);
        play(self.tx.clone(), self.cancel.clone(), reply());
    }

    fn interrupt(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// One scripted event and how long to wait before sending it.
pub struct Step {
    pub after: Duration,
    pub event: SessionEvent,
}

impl Step {
    fn new(ms: u64, event: SessionEvent) -> Self {
        Self {
            after: Duration::from_millis(ms),
            event,
        }
    }
}

/// Feed a script down the channel at its own pace, stopping short — and
/// ending the turn Interrupted — as soon as `cancel` is raised.
pub fn play(
    tx: Sender<SessionEvent>,
    cancel: Arc<AtomicBool>,
    steps: Vec<Step>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for step in steps {
            thread::sleep(step.after);
            if cancel.load(Ordering::Relaxed) {
                let _ = tx.send(SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Interrupted,
                    cost_usd: None,
                });
                return;
            }
            if tx.send(step.event).is_err() {
                return;
            }
        }
    })
}

const TURN_ONE: &str = "Ferrite renders whatever the provider streams: no harness, \
    no model calls of its own. This paragraph is long on purpose so the transcript \
    has to wrap inside the Pane and the tail keeps following the newest line as \
    deltas land. Each word arrives as its own TextDelta at roughly thirty \
    milliseconds, which is close enough to a real turn to see whether the layout \
    holds still while text grows underneath it.";

const TURN_TWO: &str = "Second turn, shorter, to prove the transcript keeps its \
    history and the status line drops back to idle with the turn cost beside it.";

const REPLY: &str = "Reading the composer path now. The event pump drains the same \
    bounded channel a live Session would write to, so what you are looking at is \
    the shipping render path with a scripted producer behind it.";

const THINKING: &[&str] = &[
    "checking the workspace binding",
    "the Pane owns view state only",
    "everything durable belongs to core",
];

/// Startup: init, thinking, a long turn, a pause, a shorter turn, then idle.
pub fn script() -> Vec<Step> {
    let mut steps = vec![Step::new(
        250,
        SessionEvent::Init {
            session_id: "4f2a1c9e-7b30-4d18-9c62-1ea55d0b7742".into(),
            model: "claude-sonnet-4-5".into(),
        },
    )];
    steps.extend(turn(THINKING, TURN_ONE, 0.0380));

    let mut second = turn(&[], TURN_TWO, 0.0124);
    if let Some(first) = second.first_mut() {
        first.after = Duration::from_millis(400);
    }
    steps.extend(second);
    steps
}

/// The canned answer to a prompt sent from the Composer.
pub fn reply() -> Vec<Step> {
    turn(&[], REPLY, 0.0091)
}

/// One turn: thinking lines, then word-by-word text, then TurnEnded.
fn turn(thinking: &[&str], text: &str, cost: f64) -> Vec<Step> {
    let mut steps = Vec::new();
    for line in thinking {
        for word in line.split_whitespace() {
            steps.push(Step::new(
                18,
                SessionEvent::ThinkingDelta {
                    text: format!("{word} "),
                },
            ));
        }
        steps.push(Step::new(
            18,
            SessionEvent::ThinkingDelta { text: "\n".into() },
        ));
    }
    for word in text.split_whitespace() {
        steps.push(Step::new(
            30,
            SessionEvent::TextDelta {
                text: format!("{word} "),
            },
        ));
    }
    steps.push(Step::new(
        30,
        SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(cost),
        },
    ));
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{Kind, Status, Transcript};

    #[test]
    fn replaying_the_demo_script_leaves_two_paid_turns_and_text_long_enough_to_wrap() {
        let mut transcript = Transcript::default();
        for step in script() {
            transcript.apply(step.event);
        }

        assert_eq!(transcript.model(), Some("claude-sonnet-4-5"));
        assert_eq!(transcript.status(), Status::Idle);

        let costs: Vec<&str> = transcript
            .segments()
            .iter()
            .filter(|s| s.kind == Kind::Meta)
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(costs, ["$0.0380", "$0.0124"]);

        let longest = transcript
            .segments()
            .iter()
            .filter(|s| s.kind == Kind::Assistant)
            .map(|s| s.text.chars().count())
            .max()
            .unwrap();
        assert!(longest > 200, "demo text must wrap; longest was {longest}");
    }

    #[test]
    fn a_cancelled_playback_ends_the_turn_interrupted() {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(true));

        play(
            tx,
            cancel,
            vec![Step::new(
                0,
                SessionEvent::TextDelta {
                    text: "never sent".into(),
                },
            )],
        )
        .join()
        .unwrap();

        let events: Vec<SessionEvent> = rx.try_iter().collect();
        assert_eq!(
            events,
            vec![SessionEvent::TurnEnded {
                outcome: TurnOutcome::Interrupted,
                cost_usd: None,
            }]
        );
    }

    #[test]
    fn the_demo_script_opens_a_session_and_ends_idle() {
        let steps = script();

        assert!(matches!(
            steps.first().unwrap().event,
            SessionEvent::Init { .. }
        ));
        assert!(matches!(
            steps.last().unwrap().event,
            SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                ..
            }
        ));
    }
}
