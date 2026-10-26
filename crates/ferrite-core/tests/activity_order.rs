//! The order of Main's subagents: working agents first, each group in
//! discovery order, and `order_changed` exactly when that order moves.

use std::time::Instant;

use ferrite_core::activity::{
    Activity, ActivityEvent, ActivityInput, ActivityUpdate, AgentInfo, AgentKey, AgentStatus,
    ExecutionEvent, Subject, TranscriptCoverage,
};
use ferrite_core::store::Provider;
use ferrite_core::transcript::Input;
use ferrite_core::{SessionEvent, TurnOutcome};

fn key(native: &str) -> AgentKey {
    AgentKey::new(Provider::Claude, "root", native)
}
fn connected() -> Activity {
    let mut activity = Activity::default();
    activity.apply(ActivityInput::Connect { generation: 7 });
    activity
}
fn observe(activity: &mut Activity, event: ActivityEvent) -> ActivityUpdate {
    activity.apply(ActivityInput::Observe {
        generation: 7,
        event,
        at: Instant::now(),
    })
}
fn discover_under(activity: &mut Activity, native: &str, parent: Subject) -> ActivityUpdate {
    let mut info = AgentInfo::new(key(native));
    info.parent = Some(parent);
    info.coverage = TranscriptCoverage::Live;
    observe(activity, ActivityEvent::Discovered(info))
}
fn discover(activity: &mut Activity, native: &str) -> AgentKey {
    discover_under(activity, native, Subject::Main);
    key(native)
}
fn status(activity: &mut Activity, key: &AgentKey, state: AgentStatus) -> ActivityUpdate {
    observe(
        activity,
        ActivityEvent::Status {
            key: key.clone(),
            state,
        },
    )
}
fn order(activity: &Activity) -> Vec<String> {
    activity
        .view()
        .children()
        .iter()
        .map(|child| {
            let (_, _, native): (Provider, String, String) =
                serde_json::from_str(child.key().as_str()).expect("agent key is a tuple");
            native
        })
        .collect()
}

#[test]
fn a_new_agent_under_main_joins_the_order_in_discovery_order() {
    let mut activity = connected();
    discover(&mut activity, "a");
    let update = discover_under(&mut activity, "b", Subject::Main);
    assert!(update.order_changed);
    assert_eq!(order(&activity), ["a", "b"]);
}

#[test]
fn rediscovering_an_agent_unchanged_reports_no_order_change() {
    let mut activity = connected();
    discover(&mut activity, "a");
    discover(&mut activity, "b");
    let update = discover_under(&mut activity, "a", Subject::Main);
    assert!(!update.order_changed);
    assert_eq!(order(&activity), ["a", "b"]);
}

#[test]
fn working_flips_move_an_agent_and_report_it_once() {
    let mut activity = connected();
    discover(&mut activity, "a");
    let b = discover(&mut activity, "b");
    assert!(status(&mut activity, &b, AgentStatus::Working).order_changed);
    assert_eq!(order(&activity), ["b", "a"]);
    assert!(!status(&mut activity, &b, AgentStatus::Working).order_changed);
    assert!(status(&mut activity, &b, AgentStatus::Idle).order_changed);
    assert_eq!(order(&activity), ["a", "b"]);
}

#[test]
fn live_content_makes_an_idle_agent_working_and_its_turn_end_settles_it() {
    let mut activity = connected();
    discover(&mut activity, "a");
    let b = discover(&mut activity, "b");
    let content = |activity: &mut Activity, event| {
        observe(
            activity,
            ActivityEvent::Content {
                key: b.clone(),
                id: Some("item".into()),
                event,
            },
        )
    };
    assert!(
        content(
            &mut activity,
            ExecutionEvent::TextDelta {
                text: "hello".into()
            }
        )
        .order_changed
    );
    assert_eq!(order(&activity), ["b", "a"]);
    assert!(
        !content(
            &mut activity,
            ExecutionEvent::TextDelta {
                text: " again".into()
            }
        )
        .order_changed
    );
    assert!(
        content(
            &mut activity,
            ExecutionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            }
        )
        .order_changed
    );
    assert_eq!(order(&activity), ["a", "b"]);
}

#[test]
fn a_stale_working_agent_sorts_with_the_rest_after_disconnect() {
    let mut activity = connected();
    discover(&mut activity, "a");
    let b = discover(&mut activity, "b");
    status(&mut activity, &b, AgentStatus::Working);
    let update = activity.apply(ActivityInput::Disconnect);
    assert!(update.order_changed);
    assert_eq!(order(&activity), ["a", "b"]);
}

#[test]
fn replayed_status_is_not_fresh_and_never_reorders() {
    let mut activity = Activity::default();
    discover_under(&mut activity, "a", Subject::Main);
    let mut info = AgentInfo::new(key("a"));
    info.parent = Some(Subject::Main);
    activity.apply(ActivityInput::ReplayEvent(ActivityEvent::Discovered(info)));
    let mut info = AgentInfo::new(key("b"));
    info.parent = Some(Subject::Main);
    activity.apply(ActivityInput::ReplayEvent(ActivityEvent::Discovered(info)));
    let update = activity.apply(ActivityInput::ReplayEvent(ActivityEvent::Status {
        key: key("b"),
        state: AgentStatus::Working,
    }));
    assert!(!update.order_changed);
    assert_eq!(order(&activity), ["a", "b"]);
}

#[test]
fn a_descendant_joins_when_its_parent_reaches_main_and_leaves_on_detach() {
    let mut activity = connected();
    let update = discover_under(&mut activity, "child", Subject::Subagent(key("parent")));
    assert!(!update.order_changed);
    assert!(order(&activity).is_empty());
    let update = discover_under(&mut activity, "parent", Subject::Main);
    assert!(update.order_changed);
    assert_eq!(order(&activity), ["child", "parent"]);
    let update = observe(
        &mut activity,
        ActivityEvent::Detached { key: key("parent") },
    );
    assert!(update.order_changed);
    assert!(order(&activity).is_empty());
}

#[test]
fn an_agent_outside_main_working_does_not_reorder() {
    let mut activity = connected();
    discover(&mut activity, "a");
    discover_under(&mut activity, "orphan", Subject::Subagent(key("unknown")));
    let update = status(&mut activity, &key("orphan"), AgentStatus::Working);
    assert!(!update.order_changed);
    assert_eq!(order(&activity), ["a"]);
}

#[test]
fn reparenting_an_agent_under_main_moves_its_subtree_in() {
    let mut activity = connected();
    discover(&mut activity, "a");
    discover_under(&mut activity, "b", Subject::Subagent(key("elsewhere")));
    discover_under(&mut activity, "c", Subject::Subagent(key("b")));
    assert_eq!(order(&activity), ["a"]);
    let update = discover_under(&mut activity, "b", Subject::Subagent(key("a")));
    assert!(update.order_changed);
    assert_eq!(order(&activity), ["a", "b", "c"]);
}

#[test]
fn an_alias_takes_the_earlier_discovery_and_the_canonical_key() {
    let mut activity = connected();
    discover(&mut activity, "provisional");
    discover(&mut activity, "middle");
    discover(&mut activity, "durable");
    assert_eq!(order(&activity), ["provisional", "middle", "durable"]);
    let update = observe(
        &mut activity,
        ActivityEvent::Alias {
            from: key("provisional"),
            to: key("durable"),
        },
    );
    assert!(update.order_changed);
    assert_eq!(order(&activity), ["durable", "middle"]);
}

#[test]
fn an_alias_to_an_unknown_key_renames_the_agent_in_place() {
    let mut activity = connected();
    discover(&mut activity, "a");
    discover(&mut activity, "provisional");
    discover(&mut activity, "c");
    let update = observe(
        &mut activity,
        ActivityEvent::Alias {
            from: key("provisional"),
            to: key("durable"),
        },
    );
    assert!(update.order_changed);
    assert_eq!(order(&activity), ["a", "durable", "c"]);
}

#[test]
fn an_alias_carries_a_working_runtime_to_the_target() {
    let mut activity = connected();
    discover(&mut activity, "target");
    discover(&mut activity, "a");
    discover(&mut activity, "source");
    status(&mut activity, &key("source"), AgentStatus::Working);
    assert_eq!(order(&activity), ["source", "target", "a"]);
    let update = observe(
        &mut activity,
        ActivityEvent::Alias {
            from: key("source"),
            to: key("target"),
        },
    );
    assert!(update.order_changed);
    assert_eq!(order(&activity), ["target", "a"]);
    assert!(activity.view().children()[0].fresh());
}

#[test]
fn evicting_and_retaining_transcripts_leaves_the_order_alone() {
    let mut activity = connected();
    discover(&mut activity, "a");
    let b = discover(&mut activity, "b");
    status(&mut activity, &b, AgentStatus::Working);
    let update = activity.apply(ActivityInput::Evict(Subject::Subagent(b.clone())));
    assert!(!update.rejected);
    assert!(!update.order_changed);
    let update = activity.apply(ActivityInput::Retain(Subject::Subagent(b)));
    assert!(!update.order_changed);
    assert_eq!(order(&activity), ["b", "a"]);
}

#[test]
fn a_conversation_reset_empties_the_order() {
    let mut activity = connected();
    discover(&mut activity, "a");
    let update = activity.apply(ActivityInput::Main {
        input: Input::Event(SessionEvent::ConversationReset {
            session_id: "fresh".into(),
        }),
        at: Instant::now(),
    });
    assert!(update.order_changed);
    assert!(order(&activity).is_empty());
    let update = discover_under(&mut activity, "a", Subject::Main);
    assert!(update.order_changed);
    assert_eq!(order(&activity), ["a"]);
}
