//! What the Pane holds: either a live Claude Session or the scripted demo
//! stream. Both hand the pump the same `Receiver<SessionEvent>`, so `--demo`
//! exercises the real render path without spawning a CLI.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ferrite_core::providers::{ClaudeSession, CodexSession};
use ferrite_core::{Decision, DecisionAnswer, SessionEvent, TurnOutcome};

pub enum Session {
    Claude(ClaudeSession),
    Codex(CodexSession),
    Demo(DemoSession),
}

impl Session {
    pub fn events(&self) -> &Receiver<SessionEvent> {
        match self {
            Session::Claude(s) => s.events(),
            Session::Codex(s) => s.events(),
            Session::Demo(d) => &d.rx,
        }
    }

    pub fn send(&mut self, text: &str) -> io::Result<()> {
        match self {
            Session::Claude(s) => s.send(text),
            Session::Codex(s) => s.send(text),
            Session::Demo(d) => {
                d.send();
                Ok(())
            }
        }
    }

    pub fn interrupt(&mut self) -> io::Result<()> {
        match self {
            Session::Claude(s) => s.interrupt(),
            Session::Codex(s) => s.interrupt(),
            Session::Demo(d) => {
                d.interrupt();
                Ok(())
            }
        }
    }

    /// Answer a Decision. Both providers take the same shape; only their wire
    /// spelling differs, which is their own business.
    pub fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()> {
        match self {
            Session::Claude(s) => s.respond_to_decision(id, answer),
            Session::Codex(s) => s.respond_to_decision(id, answer),
            Session::Demo(d) => {
                d.respond(answer);
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

    /// The demo's agent does what it was told: allowed, it finishes the write
    /// and carries on; denied, it says so and ends the turn.
    fn respond(&mut self, answer: DecisionAnswer) {
        self.cancel.store(false, Ordering::Relaxed);
        let mut steps = match answer {
            // The demo's agent cannot tell "allow once" from "allow always":
            // the standing answer changes what the provider asks next time,
            // which a script has no next time to show.
            DecisionAnswer::Allow { .. } | DecisionAnswer::AllowAlways { .. } => {
                let mut steps = vec![Step::new(
                    60,
                    SessionEvent::ToolCompleted {
                        id: "toolu_demo".into(),
                        output: "File created".into(),
                        is_error: false,
                        result: ferrite_core::ToolResult::FileEdit {
                            path: "ferrite-perm.txt".into(),
                            hunks: Vec::new(),
                        },
                    },
                )];
                steps.extend(turn(&[], ALLOWED, 0.0124));
                steps
            }
            DecisionAnswer::Deny { .. } => turn(&[], DENIED, 0.0018),
        };
        if let Some(first) = steps.first_mut() {
            first.after = Duration::from_millis(120);
        }
        play(self.tx.clone(), self.cancel.clone(), steps);
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
    holds still while text grows underneath it.\n\n\
    ## What the fold produces\n\
    - headings and bullets, each its own Block\n\
    - inline `code` kept in the run of the sentence\n\
    - fenced blocks handed to the injected highlighter\n\n\
    ```rust\n\
    fn apply(&mut self, input: Input) -> Update {\n\
        // events in, Blocks out\n\
    }\n\
    ```\n\n";

const ALLOWED: &str = "Written. The Decision came back allowed, so the tool ran \
    and the turn carried on from where it stopped.";

const DENIED: &str = "Understood — I will leave that file alone and stop here.";

const REPLY: &str = "Reading the composer path now. The event pump drains the same \
    bounded channel a live Session would write to, so what you are looking at is \
    the shipping render path with a scripted producer behind it.";

const THINKING: &[&str] = &[
    "checking the cockpit binding",
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

    // The second turn opens by asking permission, and stops there: nothing
    // else plays until the operator answers the card.
    steps.push(Step::new(
        400,
        SessionEvent::ToolStarted {
            id: "toolu_demo".into(),
            name: "Write".into(),
            input: serde_json::json!({ "file_path": "ferrite-perm.txt" }),
        },
    ));
    steps.push(Step::new(
        120,
        SessionEvent::DecisionRequested {
            decision: Decision {
                id: "perm_demo".into(),
                tool_use_id: "toolu_demo".into(),
                tool_name: "Write".into(),
                description: "ferrite-perm.txt".into(),
                input: serde_json::json!({ "file_path": "ferrite-perm.txt", "content": "ok" }),
                suggestions: vec![serde_json::json!({
                    "type": "setMode",
                    "mode": "acceptEdits",
                    "destination": "session",
                })],
            },
        },
    ));
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
    // split_inclusive, not split_whitespace: the newlines are what the
    // markdown fold reads, so a pacer that ate them would test nothing.
    for chunk in text.split_inclusive(char::is_whitespace) {
        steps.push(Step::new(
            30,
            SessionEvent::TextDelta {
                text: chunk.to_string(),
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
    use ferrite_core::cockpit::{Cockpit, ThreadId};
    use ferrite_core::transcript::{Body, Input, Status, Transcript};

    #[test]
    fn replaying_the_demo_script_leaves_a_paid_turn_and_a_thread_on_the_operator() {
        let mut transcript = Transcript::default();
        let mut cockpit = Cockpit::default();
        let thread = ThreadId(1);
        for step in script() {
            cockpit.apply(thread, &step.event);
            transcript.apply(Input::Event(step.event));
        }

        assert_eq!(transcript.model(), Some("claude-sonnet-4-5"));
        // The demo ends where a real Thread ends: waiting on a person.
        assert_eq!(transcript.status(), Status::Blocked);

        let costs: Vec<&str> = transcript
            .blocks()
            .iter()
            .filter_map(|block| match &block.body {
                Body::Meta(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(costs, ["$0.0380"]);

        let longest = transcript
            .blocks()
            .iter()
            .filter_map(|block| match &block.body {
                Body::Paragraph { spans } => {
                    Some(spans.iter().map(|s| s.text.chars().count()).sum::<usize>())
                }
                _ => None,
            })
            .max()
            .unwrap();
        assert!(longest > 200, "demo text must wrap; longest was {longest}");

        let pending = cockpit.pending(thread).expect("a Decision to answer");
        assert_eq!(pending.tool_name, "Write");
        assert_eq!(pending.id, "perm_demo");
        assert_eq!(pending.suggestions.len(), 1);
    }

    #[test]
    fn answering_the_demo_decision_plays_the_rest_of_the_turn() {
        let mut demo = DemoSession::start();

        demo.respond(DecisionAnswer::Allow {
            input: serde_json::Value::Null,
        });

        // The scripted answer runs the tool it was blocked on, then finishes.
        let mut seen = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match demo.rx.recv_timeout(Duration::from_millis(200)) {
                Ok(event) => {
                    let done = matches!(event, SessionEvent::TurnEnded { .. });
                    seen.push(event);
                    if done {
                        break;
                    }
                }
                Err(_) => continue,
            }
        }
        assert!(
            seen.iter()
                .any(|event| matches!(event, SessionEvent::ToolCompleted { .. })),
            "the allowed tool must run: {seen:?}"
        );
        assert!(matches!(
            seen.last(),
            Some(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                ..
            })
        ));
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
    fn the_demo_script_opens_a_session_and_stops_on_a_decision() {
        let steps = script();

        assert!(matches!(
            steps.first().unwrap().event,
            SessionEvent::Init { .. }
        ));
        assert!(matches!(
            steps.last().unwrap().event,
            SessionEvent::DecisionRequested { .. }
        ));
    }
}
