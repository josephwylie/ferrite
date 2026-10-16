//! Other machines on the board (ADR 0031, Seam 3): what upstream's terminal
//! runtime asks them, away from the keys (`board/run/mod.rs`: `poll`). The
//! list of machines is asked for at most once a minute (`remote::machines`:
//! Herdr's machine list, which the adapter answers from `machine.list`, in
//! its order); each machine in view is asked for its tasks over upstream's
//! ssh (`herdr-projects remote list`) whenever the board polls (every 5 s
//! with other machines shown, at once after a change), one that did not
//! answer only after 30 s unless the poll is for now. The board's `update`
//! takes each answer as an event (`update/remotes.rs`).
//!
//! Ferrite runs one Server per machine, so a machine's session is always
//! `default` and the board's header never says `session <name>`.

use sw_board::board::state::State;
use sw_board::board::update::Event;
use sw_board::remote::Machine;

/// How often the list of machines is asked for again.
pub const MACHINES_EVERY_MS: i64 = 60_000;
/// How long a machine that did not answer is left alone, but for a poll for now.
pub const RETRY_AFTER_MS: i64 = 30_000;
/// How long a machine has to answer for its tasks.
const ANSWER_WITHIN_MS: u64 = 20_000;

/// One question for the machines, answered off the foreground.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// Which machines there are.
    Machines,
    /// One machine's tasks, automations and Coordinator.
    List { label: String, machine: Machine },
}

impl Ask {
    /// The answer, as the board's event. Blocks: ssh to another machine.
    pub fn answer(self) -> Event {
        match self {
            Ask::Machines => Event::Machines(sw_board::remote::machines()),
            Ask::List { label, machine } => Event::Remote {
                label,
                answer: sw_board::remote::ask_list(&machine, ANSWER_WITHIN_MS),
            },
        }
    }
}

/// When the list of machines was last asked for.
#[derive(Default)]
pub struct Polling {
    machines_at: i64,
}

impl Polling {
    /// What to ask now for `Effect::Poll(now_too)`: the list of machines when
    /// a minute has passed, and every machine not already being asked, a
    /// failed one only after its wait unless `now_too`. Each one asked is
    /// marked busy until its answer comes in.
    pub fn due(&mut self, s: &mut State, now: i64, now_too: bool) -> Vec<Ask> {
        let mut asks = vec![];
        if now - self.machines_at > MACHINES_EVERY_MS {
            self.machines_at = now;
            asks.push(Ask::Machines);
        }
        for (label, m) in s.remotes.iter_mut() {
            if m.busy || (!now_too && !m.ok && now - m.fail_at < RETRY_AFTER_MS) {
                continue;
            }
            m.busy = true;
            asks.push(Ask::List {
                label: label.clone(),
                machine: m.machine.clone(),
            });
        }
        asks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sw_board::board::data::Remote;

    fn machine(label: &str) -> Machine {
        Machine {
            label: label.into(),
            target: format!("me@{label}"),
            ..Default::default()
        }
    }

    fn state(remotes: &[(&str, bool, i64)]) -> State {
        let mut s = State::new(Default::default(), Default::default());
        for (label, ok, fail_at) in remotes {
            s.remotes.insert(
                label.to_string(),
                Remote {
                    machine: machine(label),
                    ok: *ok,
                    fail_at: *fail_at,
                    ..Default::default()
                },
            );
        }
        s
    }

    fn labels(asks: &[Ask]) -> Vec<&str> {
        asks.iter()
            .map(|a| match a {
                Ask::Machines => "machines",
                Ask::List { label, .. } => label.as_str(),
            })
            .collect()
    }

    #[test]
    fn the_machines_are_listed_at_first_and_then_once_a_minute() {
        let mut p = Polling::default();
        let mut s = state(&[]);
        let start = 1_000_000;
        assert_eq!(labels(&p.due(&mut s, start, true)), ["machines"]);
        assert!(p.due(&mut s, start + MACHINES_EVERY_MS, false).is_empty());
        assert_eq!(
            labels(&p.due(&mut s, start + MACHINES_EVERY_MS + 1, false)),
            ["machines"]
        );
    }

    #[test]
    fn a_machine_is_asked_once_at_a_time_and_one_that_failed_waits_30_s_unless_asked_now() {
        let now = 1_000_000;
        let mut p = Polling { machines_at: now };
        let mut s = state(&[
            ("vm-a", true, 0),
            ("vm-b", false, now - 1_000),
            ("vm-c", false, now - RETRY_AFTER_MS),
        ]);
        assert_eq!(labels(&p.due(&mut s, now, false)), ["vm-a", "vm-c"]);
        assert!(s.remotes["vm-a"].busy && !s.remotes["vm-b"].busy);
        assert!(
            p.due(&mut s, now, false).is_empty(),
            "those asked are still busy"
        );
        assert_eq!(
            labels(&p.due(&mut s, now, true)),
            ["vm-b"],
            "a poll for now asks the failed one too"
        );
    }

    #[test]
    fn an_answer_is_the_boards_event_for_its_machine() {
        let mut p = Polling {
            machines_at: i64::MAX,
        };
        let mut s = state(&[("vm-a", true, 0)]);
        let ask = p.due(&mut s, 0, true).remove(0);
        assert_eq!(
            ask,
            Ask::List {
                label: "vm-a".into(),
                machine: machine("vm-a")
            }
        );
    }
}
