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
