//! The details card's Lineage, read off the window: the Subject's parent,
//! its agents, and the Threads on other machines it delegated to or was
//! delegated from (ADR 0047). `ferrite_pane::details::lineage` draws it.

use super::super::delegates::{Delegator, Serving};
use super::super::subagents::{agent_name, parent_of, state_word};
use super::super::transcript_glue::agent_clock;
use ferrite_core::activity::{ActivityView, AgentStatus, Subject, SubjectView};
use ferrite_core::delegate::{Delegated, Delegating, Status};
use ferrite_pane::details::lineage::{Dot, Leads, Lineage, Relative};
use ferrite_core::server::ServerId;
use ferrite_core::TurnOutcome;
use ferrite_pane::pane::AgentClock;
use std::collections::HashMap;

/// How a parent stands (T3's `activityRunStatus`): at rest, as its last turn
/// ended, so a Thread stopped with Escape does not read Done.
fn parent_status(view: SubjectView<'_>) -> AgentStatus {
    match (view.status(), view.last_outcome()) {
        (AgentStatus::Idle, Some(TurnOutcome::Interrupted)) => AgentStatus::Interrupted,
        (AgentStatus::Idle, Some(TurnOutcome::Error(_))) => AgentStatus::Failed,
        (status, _) => status,
    }
}

/// What Main's Lineage draws of other machines, from the Thread's digest
/// (`ThreadFacts`), and which machine of this window answers as a Server.
pub(super) struct Elsewhere<'a, F: Fn(&ServerId) -> Option<Serving>> {
    pub(super) delegating: &'a Delegating,
    pub(super) delegated_from: Option<Delegator>,
    pub(super) serving: F,
}

/// `subject`'s Lineage in `activity`; Main, its parent, is `main_title`.
pub(super) fn lineage<F: Fn(&ServerId) -> Option<Serving>>(
    activity: ActivityView<'_>,
    subject: &Subject,
    main_title: &str,
    elsewhere: Elsewhere<'_, F>,
) -> Lineage {
    let agents = activity.children();
    let parent = parent_of(activity, subject).and_then(|parent| {
        let (title, view) = match &parent {
            Subject::Main => (main_title.to_owned(), activity.main()),
            Subject::Subagent(_) => (
                agents
                    .iter()
                    .find(|agent| agent.subject() == parent)
                    .map(|agent| agent_name(agent.info()))?,
                activity.subject(&parent)?,
            ),
        };
        let status = parent_status(view);
        Some(Relative {
            meta: state_word(status, view.fresh()).to_owned(),
            timed: false,
            state: Dot::of(status, view.fresh()),
            leads: Leads::Subject(parent),
            title,
            provider: None,
            tooltip: None,
        })
    });
    // An agent's clock is the one its card row reads. Its spawning call comes
    // first: a later `SendMessage` to it is named by the agent too, and that
    // call's own time is not the agent's.
    let mut clocks: HashMap<Subject, Option<AgentClock>> = HashMap::new();
    for (call, agent) in activity.invocations() {
        clocks
            .entry(agent.subject())
            .or_insert_with(|| agent_clock(activity, agent, &call));
    }
    let mut own: Vec<_> = agents
        .into_iter()
        .filter(|agent| parent_of(activity, &agent.subject()).as_ref() == Some(subject))
        .collect();
    own.sort_by_key(|agent| std::cmp::Reverse(agent.discovered()));
    let mut lineage = Lineage {
        parent,
        ..Lineage::default()
    };
    for agent in own {
        let state = Dot::of(agent.status(), agent.fresh());
        let clock = clocks
            .get(&agent.subject())
            .copied()
            .flatten()
            .filter(|_| state != Dot::Failed)
            .map(|clock| ferrite_nav::working_for(clock.elapsed()));
        let row = Relative {
            leads: Leads::Subject(agent.subject()),
            title: agent_name(agent.info()),
            timed: clock.is_some(),
            meta: clock.unwrap_or_else(|| state_word(agent.status(), agent.fresh()).to_owned()),
            state,
            provider: None,
            tooltip: None,
        };
        if agent.running() {
            lineage.running.push(row);
            lineage.running_count += 1;
        } else {
            lineage.previous.push(row);
        }
    }
    if *subject != Subject::Main {
        return lineage;
    }
    lineage.delegated_from = elsewhere.delegated_from.map(delegated_from);
    for delegate in &elsewhere.delegating.delegates {
        let row = delegate_row(delegate, (elsewhere.serving)(&delegate.server).is_some());
        lineage.running_count += usize::from(delegate.status == Status::Running);
        match delegate.status.ended() {
            false => lineage.running.push(row),
            true => lineage.previous.push(row),
        }
    }
    lineage
}

/// T3's `RemoteSubagentRow`: its title (else `Delegate`, where T3 says
/// `Subagent`), and `on pc · Running`, its dot in T3's status inks. One
/// held up says why under the pointer, in its Server's words: following
/// it, or a stop that has not reached it.
fn delegate_row(delegate: &Delegated, reached: bool) -> Relative {
    let title = delegate.title.clone().unwrap_or_else(|| "Delegate".to_owned());
    // T3's hint, pressable or not: this window has no read of a Thread
    // through another machine to offer instead (T3's `Read the thread on`).
    let hint = format!("Open on {}", delegate.machine);
    let problem = delegate.unstopped().or_else(|| delegate.problem.clone());
    Relative {
        tooltip: Some(
            [Some(title.clone()), Some(hint), problem]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        meta: format!("on {} \u{b7} {}", delegate.machine, status_label(delegate.status)),
        timed: false,
        state: match delegate.status {
            Status::Starting | Status::Running | Status::Waiting => Dot::Running,
            Status::Failed => Dot::Failed,
            Status::Completed => Dot::Done,
            Status::Cancelled | Status::Interrupted => Dot::Idle,
        },
        leads: Leads::Elsewhere {
            server: delegate.server.clone(),
            thread: delegate.thread,
            reached,
            cancel: (!delegate.status.ended()).then_some(delegate.id),
        },
        provider: delegate.provider,
        title,
    }
}

/// T3's `DelegatedFromRow`: the delegating Thread's title, and `on mac`
/// (`Delegator`).
fn delegated_from(from: Delegator) -> Relative {
    let hint = match from.reached {
        true => format!("Open parent agent on {}", from.machine),
        false => format!(
            "Delegated from {}. Connect to it to open the parent thread.",
            from.machine
        ),
    };
    Relative {
        tooltip: Some(format!("{}\n{hint}", from.title)),
        meta: format!("on {}", from.machine),
        timed: false,
        state: Dot::Idle,
        leads: Leads::Delegator {
            server: from.server,
            thread: from.thread,
            reached: from.reached,
        },
        provider: None,
        title: from.title,
    }
}

/// T3's `threadRelationshipStatusLabel` for a delegated task's status.
fn status_label(status: Status) -> &'static str {
    match status {
        Status::Starting => "Starting",
        Status::Running => "Running",
        Status::Waiting => "Waiting",
        Status::Completed => "Done",
        Status::Failed => "Failed",
        Status::Cancelled | Status::Interrupted => "Stopped",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::activity::{Activity, ActivityInput};
    use ferrite_core::delegate::DelegateId;
    use ferrite_core::provider::Provider;
    use ferrite_core::ThreadId;

    fn delegated(id: u32, title: Option<&str>, status: Status) -> Delegated {
        Delegated {
            id: DelegateId::new(id),
            server: ServerId::new("s-pc"),
            machine: "pc".into(),
            thread: ThreadId::new(7 + u64::from(id)),
            title: title.map(str::to_owned),
            provider: Some(Provider::Codex),
            status,
            problem: None,
            stop_owed: false,
            result: None,
            started_at: None,
            ended_at: None,
        }
    }

    /// `pc`, off screen, as this window saved it.
    fn pc() -> Serving {
        Serving {
            name: Some("pc (box)".into()),
            machine: ferrite_core::ThreadKey::fixture(1, 1).machine(),
        }
    }

    /// Main's Lineage with these Delegates, `pc` reached off screen as
    /// `pc (box)` when `reached`, delegated `from` another Thread.
    fn main_lineage(delegates: Vec<Delegated>, from: Option<Delegator>, reached: bool) -> Lineage {
        let activity = Activity::default();
        let delegating = Delegating {
            delegates,
            holds: true,
        };
        let serving = |_: &ServerId| reached.then(pc);
        lineage(
            activity.view(),
            &Subject::Main,
            "Main",
            Elsewhere {
                delegating: &delegating,
                delegated_from: from,
                serving,
            },
        )
    }

    /// A Server restart leaves every agent not fresh: how one ended still
    /// reads as it did, and what one was doing no longer claims to go on.
    #[test]
    fn how_an_agent_ended_outlives_a_restart() {
        let read = |status| (state_word(status, false), Dot::of(status, false));
        assert_eq!(read(AgentStatus::Interrupted), ("Stopped", Dot::Idle));
        assert_eq!(read(AgentStatus::Failed), ("Failed", Dot::Failed));
        assert_eq!(read(AgentStatus::Idle), ("Done", Dot::Done));
        assert_eq!(read(AgentStatus::Working), ("Unavailable", Dot::Idle));
    }

    /// A `SendMessage` is named by the agent it reached, and takes no time
    /// of the agent's: the row keeps the run its card row reads.
    #[test]
    fn a_message_to_an_agent_leaves_its_clock() {
        use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, TranscriptCoverage};
        use ferrite_core::transcript::Input;
        use ferrite_core::{SessionEvent, ToolResult};
        let key = |native| AgentKey::new(Provider::Claude, "root", native);
        let at = std::time::Instant::now();
        let mut activity = Activity::default();
        activity.apply(ActivityInput::Connect { generation: 1 });
        let mut info = AgentInfo::new(key("Atlas"));
        info.name = Some("Atlas".into());
        info.parent = Some(Subject::Main);
        info.coverage = TranscriptCoverage::Live;
        for event in [
            ActivityEvent::Discovered(info),
            ActivityEvent::Status { key: key("Atlas"), state: AgentStatus::Interrupted },
            ActivityEvent::CompletionObservation {
                subject: Subject::Subagent(key("Atlas")),
                elapsed_ms: 57_000,
                completed_at: "2026-11-24T10:00:57Z".into(),
                input_tokens: None,
                output_tokens: None,
            },
            ActivityEvent::Alias { from: key("Send"), to: key("Atlas") },
        ] {
            activity.apply(ActivityInput::Observe { generation: 1, event, at });
        }
        for event in [
            SessionEvent::ToolStarted {
                id: "Send".into(),
                name: "SendMessage".into(),
                input: serde_json::json!({}),
            },
            SessionEvent::ToolCompleted {
                id: "Send".into(),
                output: "refused".into(),
                is_error: true,
                result: ToolResult::Opaque,
            },
        ] {
            activity.apply(ActivityInput::Main { input: Input::Event(event), at });
        }
        let delegating = Delegating { delegates: Vec::new(), holds: true };
        let read = lineage(
            activity.view(),
            &Subject::Main,
            "Main",
            Elsewhere {
                delegating: &delegating,
                delegated_from: None,
                serving: |_: &ServerId| None,
            },
        );
        assert_eq!(read.previous[0].meta, "57s");
    }

    /// A parent at rest reads as its last turn ended: stopped, not Done.
    #[test]
    fn a_parent_stopped_with_escape_reads_stopped() {
        let ended = |outcome| {
            let mut activity = Activity::default();
            activity.apply(ActivityInput::Connect { generation: 1 });
            activity.apply(ActivityInput::Main {
                input: ferrite_core::transcript::Input::Event(
                    ferrite_core::SessionEvent::TurnEnded { outcome, cost_usd: None },
                ),
                at: std::time::Instant::now(),
            });
            parent_status(activity.view().main())
        };
        assert_eq!(ended(TurnOutcome::Interrupted), AgentStatus::Interrupted);
        assert_eq!(ended(TurnOutcome::Error("x".into())), AgentStatus::Failed);
        assert_eq!(ended(TurnOutcome::Completed), AgentStatus::Idle);
    }

    /// T3's `RemoteSubagentRow`: the title (`Delegate` when it has none),
    /// `on pc · Running` in T3's status words, open ones after the agents
    /// and ended ones under `Previous agents`, each in the order they
    /// started; the heading counts the running ones alone.
    #[test]
    fn a_delegate_is_t3s_remote_row_in_t3s_words() {
        let lineage = main_lineage(
            vec![
                delegated(1, Some("Fix the tests"), Status::Running),
                delegated(2, None, Status::Waiting),
                delegated(3, Some("Lint"), Status::Completed),
                delegated(4, Some("Docs"), Status::Interrupted),
                delegated(5, Some("Bench"), Status::Failed),
            ],
            None,
            true,
        );
        let rows = |rows: &[Relative]| {
            rows.iter()
                .map(|row| (row.title.clone(), row.meta.clone(), row.state))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rows(&lineage.running),
            [
                ("Fix the tests".to_owned(), "on pc \u{b7} Running".to_owned(), Dot::Running),
                ("Delegate".to_owned(), "on pc \u{b7} Waiting".to_owned(), Dot::Running),
            ]
        );
        assert_eq!(
            rows(&lineage.previous),
            [
                ("Lint".to_owned(), "on pc \u{b7} Done".to_owned(), Dot::Done),
                ("Docs".to_owned(), "on pc \u{b7} Stopped".to_owned(), Dot::Idle),
                ("Bench".to_owned(), "on pc \u{b7} Failed".to_owned(), Dot::Failed),
            ]
        );
        assert_eq!(lineage.running_count, 1, "T3 counts the running alone");
        let open = &lineage.running[0];
        assert_eq!(open.provider, Some(Provider::Codex));
        assert_eq!(
            open.leads,
            Leads::Elsewhere {
                server: ServerId::new("s-pc"),
                thread: ThreadId::new(8),
                reached: true,
                cancel: Some(DelegateId::new(1)),
            }
        );
        assert_eq!(open.tooltip.as_deref(), Some("Fix the tests\nOpen on pc"));
        assert!(
            matches!(lineage.previous[0].leads, Leads::Elsewhere { cancel: None, .. }),
            "an ended one has nothing to stop"
        );
        assert_eq!(
            lineage.previous[0].tooltip.as_deref(),
            Some("Lint\nOpen on pc"),
            "a problem is said only where there is one"
        );
    }

    /// A machine this window does not reach draws its row unpressable, as
    /// T3 disables it; what holds one up is said under the pointer in its
    /// Server's words, and a stop owed there in T3's cancel words.
    #[test]
    fn an_unreached_or_held_up_delegate_says_why() {
        let mut followed = delegated(1, Some("Fix the tests"), Status::Running);
        followed.problem = Some("pc did not answer: no route to host".into());
        let mut unstopped = delegated(2, Some("Lint"), Status::Cancelled);
        unstopped.problem = Some("pc did not answer".into());
        unstopped.stop_owed = true;
        let lineage = main_lineage(vec![followed, unstopped], None, false);
        let open = &lineage.running[0];
        assert!(matches!(open.leads, Leads::Elsewhere { reached: false, .. }));
        assert_eq!(
            open.tooltip.as_deref(),
            Some("Fix the tests\nOpen on pc\npc did not answer: no route to host")
        );
        assert_eq!(open.state, Dot::Running, "T3's dot is its status's");
        assert_eq!(
            lineage.previous[0].tooltip.as_deref(),
            Some("Lint\nOpen on pc\nIt could not be stopped on pc and may still run there: pc did not answer")
        );
    }

    /// T3's `DelegatedFromRow`: the delegating Thread's title as its
    /// machine says it now, else as it was sent, `on` the name this window
    /// saved that machine by, else the label its Server sent: this
    /// computer is not `local` to the Thread it delegated.
    #[test]
    fn a_delegated_thread_names_whose_it_is() {
        use ferrite_core::DelegatedFrom;
        let from = DelegatedFrom {
            server: ServerId::new("s-mac"),
            label: "mac".into(),
            thread: ThreadId::new(3),
            title: "Ship it".into(),
        };
        let lineage = |delegator| main_lineage(vec![], Some(delegator), false).delegated_from.unwrap();
        let reached = lineage(Delegator::of(&from, Some(pc()), Some("Ship it today")));
        assert_eq!(
            (reached.title.as_str(), reached.meta.as_str()),
            ("Ship it today", "on pc (box)")
        );
        assert_eq!(
            reached.tooltip.as_deref(),
            Some("Ship it today\nOpen parent agent on pc (box)")
        );
        assert_eq!(
            reached.leads,
            Leads::Delegator {
                server: ServerId::new("s-mac"),
                thread: ThreadId::new(3),
                reached: true,
            }
        );
        let home = Serving {
            name: None,
            machine: ferrite_core::ThreadKey::fixture(0, 1).machine(),
        };
        assert_eq!(lineage(Delegator::of(&from, Some(home), None)).meta, "on mac");
        let alone = lineage(Delegator::of(&from, None, None));
        assert_eq!((alone.title.as_str(), alone.meta.as_str()), ("Ship it", "on mac"));
        assert_eq!(
            alone.tooltip.as_deref(),
            Some("Ship it\nDelegated from mac. Connect to it to open the parent thread.")
        );
        assert!(matches!(alone.leads, Leads::Delegator { reached: false, .. }));
    }
}
