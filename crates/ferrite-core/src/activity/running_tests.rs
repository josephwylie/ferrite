use super::*;

fn key(native: &str) -> AgentKey {
    AgentKey::new(Provider::Claude, "root", native)
}

fn observe(activity: &mut Activity, event: ActivityEvent) {
    activity.apply(ActivityInput::Observe {
        generation: 7,
        event,
        at: Instant::now(),
    });
}

fn status(activity: &mut Activity, key: &AgentKey, state: AgentStatus) {
    observe(
        activity,
        ActivityEvent::Status {
            key: key.clone(),
            state,
        },
    );
}

fn spawn(activity: &mut Activity, call: &str) -> AgentKey {
    let started = key(call);
    let mut info = AgentInfo::new(started.clone());
    info.parent = Some(Subject::Main);
    info.coverage = TranscriptCoverage::Live;
    observe(activity, ActivityEvent::Discovered(info));
    for alias in [format!("{call}-task"), format!("{call}-agent")] {
        observe(
            activity,
            ActivityEvent::Alias {
                from: key(&alias),
                to: started.clone(),
            },
        );
    }
    status(activity, &started, AgentStatus::Working);
    started
}

fn connected() -> Activity {
    let mut activity = Activity::default();
    activity.apply(ActivityInput::Connect { generation: 7 });
    activity
}

fn running(activity: &Activity) -> Vec<Vec<String>> {
    activity
        .view()
        .running_agents()
        .map(|(mut names, _)| {
            names.sort();
            names
        })
        .collect()
}

/// How long an agent ran is the agent's fact, not its transcript's: it
/// outlives the transcript's eviction and comes back on a replay (a window
/// or Server restart), where a child's content stays on disk until shown.
#[test]
fn a_finished_run_outlives_its_transcript_and_returns_on_replay() {
    let mut activity = connected();
    let agent = spawn(&mut activity, "first");
    let completion = ActivityEvent::CompletionObservation {
        subject: Subject::Subagent(agent.clone()),
        elapsed_ms: 84_250,
        completed_at: "2026-11-23T09:05:00.500+00:00".into(),
        input_tokens: None,
        output_tokens: None,
    };
    status(&mut activity, &agent, AgentStatus::Idle);
    observe(&mut activity, completion.clone());
    let expected = Some(AgentRan {
        took: Duration::from_millis(84_250),
        ended: crate::clock::parse_stamp("2026-11-23T09:05:00.500+00:00"),
    });
    let ran = |activity: &Activity| activity.view().children()[0].ran();
    assert_eq!(ran(&activity), expected);
    activity.apply(ActivityInput::Evict(Subject::Subagent(agent.clone())));
    assert_eq!(ran(&activity), expected, "past its transcript's eviction");

    let mut restarted = Activity::default();
    let mut info = AgentInfo::new(agent.clone());
    info.parent = Some(Subject::Main);
    restarted.apply(ActivityInput::ReplayEvent(ActivityEvent::Discovered(info)));
    restarted.apply(ActivityInput::ReplayEvent(completion));
    restarted.apply(ActivityInput::Evict(Subject::Subagent(agent)));
    assert_eq!(ran(&restarted), expected, "from the log, after a restart");
}

#[test]
fn running_agents_names_each_running_agent_until_it_finishes() {
    let mut activity = connected();
    let first = spawn(&mut activity, "first");
    spawn(&mut activity, "second");
    status(&mut activity, &key("second-agent"), AgentStatus::Idle);
    assert_eq!(running(&activity), [["first", "first-agent", "first-task"]]);

    status(&mut activity, &first, AgentStatus::Waiting);
    assert_eq!(running(&activity).len(), 1);

    status(&mut activity, &first, AgentStatus::Idle);
    assert!(running(&activity).is_empty());
}

#[test]
fn running_agents_reads_no_key_of_a_finished_agent() {
    let reads = |finished: usize| {
        let mut activity = connected();
        for index in 0..finished {
            let done = spawn(&mut activity, &format!("done-{index}"));
            status(&mut activity, &done, AgentStatus::Idle);
        }
        spawn(&mut activity, "live");
        KEY_READS.set(0);
        assert_eq!(running(&activity).len(), 1);
        KEY_READS.get()
    };
    assert_eq!(reads(1), 3);
    assert_eq!(reads(300), 3);
}
