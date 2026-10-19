//! herdr-projects' herdr calls answered by the real Server (ADR 0031): the
//! board's own `Herdr` client, through its Ferrite seam, against this
//! Server's API socket, so a shape the seam reads and the Server answers
//! cannot drift apart unseen.

use super::*;
use sw_core::herdr::Herdr;
use sw_core::json::*;

/// The board keeps its placed panes beside its data, in the folder the
/// process's environment names: each test here names its own, one at a
/// time, so no test's panes are taken for another's Threads.
fn herdr(harness: &Harness) -> (Herdr, std::sync::MutexGuard<'static, ()>) {
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let one = ONE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var("HERDR_PROJECTS_DATA_DIR", harness.dir.join("board-data"));
    (Herdr::new(Some(harness.api.display().to_string())), one)
}

/// The agent in `pane` once `done` holds of it, for at most five seconds.
fn agent_until(h: &Herdr, pane: &str, done: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let agent = h.agent_get(pane).unwrap().expect("an agent in the pane");
        if done(&agent) || Instant::now() >= deadline {
            return agent;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// One turn of the agent in `pane`: prompted, taken, ended. Its agent once
/// the end was counted.
fn turn(h: &Herdr, pane: &str, provider: &Sender<SessionEvent>) -> Value {
    let before = h
        .agent_get(pane)
        .unwrap()
        .unwrap()
        .at("completion_seq")
        .clone();
    let prompted = h.agent_prompt(pane, "go").unwrap();
    assert_eq!(
        prompted.at("agent").s("agent_status"),
        "working",
        "{prompted}"
    );
    provider
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    agent_until(h, pane, |a| {
        a.s("agent_status") == "idle" && a.at("completion_seq") != &before
    })
}

/// A pane placed in a Space of its own, its agent started there, a turn
/// counted, its agent quit and started again in place: the Thread keeps
/// its id and its terminal while the Server runs, and the next turn counts
/// past the one before, as herdr's `state_change_seq` and `completion_seq`
/// do, so the finish line never takes the new turn for the handled one.
#[test]
fn a_pane_s_agent_runs_quits_and_runs_again_on_the_real_server() {
    let harness = start("herdr-revive");
    let (h, _one) = herdr(&harness);
    let cwd = harness.dir.display().to_string();
    let env = [("HERDR_PROJECTS_DATA_DIR".to_string(), "/data".to_string())];
    let made = h.workspace_create(&cwd, "board", &env).unwrap();
    let space = made.at("workspace").t("workspace_id");
    let pane = made.at("root_pane").t("pane_id");
    assert_eq!(made.at("workspace").s("label"), "board", "{made}");
    assert!(
        h.agent_get(&pane).unwrap().is_none(),
        "the pane is at its shell"
    );

    let started = h
        .agent_start("t-0001", "claude", &pane, &[], 10000)
        .unwrap();
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(started.s("pane_id"), pane, "{started}");
    assert_eq!(started.s("workspace_id"), space, "{started}");
    assert_eq!(started.s("agent_status"), "idle", "{started}");
    let terminal = started.t("terminal_id");
    assert!(
        terminal.contains('@'),
        "its Thread in this boot: {terminal}"
    );
    assert!(started.at("completion_seq").is_null(), "no turn ended yet");

    let first = turn(&h, &pane, &provider);
    let handled = first.at("completion_seq").as_u64().expect("a turn ended");
    assert_eq!(first.t("terminal_id"), terminal);
    let tabs = h.tabs(&space).unwrap();
    assert_eq!(tabs.len(), 1, "{tabs:?}");
    assert!(h.pane_read(&pane, "visible", None).is_ok());

    // herdr's `pane quit` then `agent start` in the same pane.
    assert!(h.pane_quit(&pane, 6000));
    assert!(
        h.agent_get(&pane).unwrap().is_none(),
        "the pane is at its shell again"
    );
    let again = h
        .agent_start(
            "t-0001",
            "claude",
            &pane,
            &["--resume".into(), "s-1".into()],
            10000,
        )
        .unwrap();
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(
        again.t("terminal_id"),
        terminal,
        "the same Thread, revived in place"
    );
    assert!(again.at("completion_seq").is_null(), "{again}");
    let second = turn(&h, &pane, &provider);
    let next = second.at("completion_seq").as_u64().unwrap();
    assert!(
        next > handled,
        "the next turn counts past the handled one: {next} after {handled}"
    );
    assert!(second.i("state_change_seq") as u64 >= next);

    // A kind Ferrite has no Provider for is refused before anything starts.
    assert!(h.pane_quit(&pane, 6000));
    let refused = h.agent_start("x", "pi", &pane, &[], 10000).unwrap_err();
    assert_eq!(refused.code, "no_provider", "{}", refused.message);

    h.workspace_close(&space).unwrap();
    assert!(h.workspace_get(&space).is_none(), "gone with its panes");
}

/// `Herdr::pane_focus`, which zooms a pane in herdr, lands every window on
/// the pane's Thread.
#[test]
fn a_pane_focused_lands_the_windows_on_its_thread() {
    let harness = start("herdr-focus");
    let mut window = attach(&harness.address);
    let (h, _one) = herdr(&harness);
    let cwd = harness.dir.display().to_string();
    let made = h.workspace_create(&cwd, "", &[]).unwrap();
    let pane = made.at("root_pane").t("pane_id");
    let started = h.agent_start("t-0002", "codex", &pane, &[], 10000).unwrap();
    h.pane_focus(&pane).unwrap();
    assert_eq!(
        told(&mut window, |message| matches!(
            message,
            ServerMessage::Focus { .. }
        )),
        ServerMessage::Focus {
            thread: ThreadId::new(thread_of(&started))
        }
    );
}

/// A task delegated in a repo, as `delegate/jobs.rs` places it: a worktree
/// Space of its own (`worktree create`), its tab named `Agent` before its
/// agent starts (`delegate/launch.rs`), its agent started in the pane the
/// worktree came with. Its Space and pane, then the agent started.
fn delegated(h: &Herdr, repo: &Path, kind: &str) -> (String, String, Value) {
    let made = h
        .worktree_create(
            &repo.display().to_string(),
            "hp/demo/t-0001",
            "main",
            "demo",
        )
        .unwrap();
    let space = made.at("workspace").t("workspace_id");
    let pane = made.at("root_pane").t("pane_id");
    assert_eq!(made.at("worktree").t("path"), space, "{made}");
    assert_eq!(made.at("tab").t("tab_id"), pane, "a tab is its one pane");
    assert!(h.tab_rename(&pane, "Agent").is_some());
    let started = h
        .agent_start(
            "t-0001",
            kind,
            &pane,
            &["--model".into(), "m".into()],
            10000,
        )
        .unwrap();
    (space, pane, started)
}

/// What the hook keeps of the turn it handled (`flow/hook.rs`): the
/// reading's counts and terminal.
fn handled(agent: &Value) -> Value {
    json!({ "seq": agent.at("state_change_seq"), "comp": agent.at("completion_seq"), "term": agent.at("terminal_id") })
}

/// Whether the finish line takes a reading for one after the turn it
/// handled: upstream's rule (`flow/hook.rs` `after_handled`, private
/// there), on the adapter's answers. Another terminal, or a count past the
/// handled one.
fn after_handled(handled: &Value, agent: &Value) -> bool {
    agent.t("terminal_id") != handled.t("term") || agent.i("state_change_seq") > handled.i("seq")
}

/// The Thread an agent's terminal names (`<thread>@<boot>`).
fn thread_of(agent: &Value) -> u64 {
    let terminal = agent.t("terminal_id");
    terminal
        .split('@')
        .next()
        .unwrap()
        .parse()
        .expect("a Thread's id")
}

fn same_dir(a: &str, b: &Path) -> bool {
    std::fs::canonicalize(a).ok() == std::fs::canonicalize(b).ok()
}

/// A task delegated through the adapter gets a placed pane whose Thread is
/// open in the task's worktree, its tab labelled as the board named it,
/// and `agent list` reports its agent with herdr's counts and its terminal
/// as its Thread in this boot.
#[test]
fn a_delegated_task_runs_in_its_placed_pane_on_the_real_server() {
    let harness = start("herdr-delegate");
    let (h, _one) = herdr(&harness);
    let repo = repo_in(&harness.dir);
    let (space, pane, started) = delegated(&h, &repo, "claude");
    harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();

    let terminal = started.t("terminal_id");
    let (_, boot) = terminal.split_once('@').expect("thread@boot");
    let open = call(&harness).answer(
        "1",
        "thread.get",
        serde_json::json!({ "thread": thread_of(&started) }),
    );
    let info = &open["result"]["thread"];
    assert_eq!(info["boot"], boot, "{open}");
    assert_eq!(info["title"], "t-0001", "{open}");
    assert_eq!(info["branch"], "hp/demo/t-0001", "{open}");
    assert!(
        same_dir(info["cwd"].as_str().unwrap(), Path::new(&space)),
        "{open}"
    );

    assert_eq!(h.tab_get(&pane).unwrap().s("label"), "Agent");
    let space_info = h.workspace_get(&space).expect("its worktree Space");
    assert_eq!(
        space_info.at("worktree").at("is_linked_worktree"),
        &json!(true),
        "{space_info}"
    );
    let agents = h.agents().unwrap();
    let agent = agents
        .iter()
        .find(|a| a.s("pane_id") == pane)
        .unwrap_or_else(|| panic!("listed: {agents:?}"));
    assert_eq!(agent.t("terminal_id"), terminal, "{agent}");
    assert_eq!(agent.s("workspace_id"), space, "{agent}");
    assert_eq!(agent.s("name"), "t-0001", "{agent}");
    assert_eq!(agent.s("agent_status"), "idle", "{agent}");
    assert!(agent.at("state_change_seq").is_u64(), "{agent}");
    assert!(
        agent.at("completion_seq").is_null(),
        "no turn ended yet: {agent}"
    );
    assert!(
        same_dir(agent.s("cwd"), Path::new(&space)),
        "the hook's match on its folder: {agent}"
    );
}

/// A turn ending (its Provider working, then at rest) is one new turn for
/// the finish line, and only one: read again, the same turn is the handled
/// one; the next prompt's turn is after it, by both of herdr's counts.
#[test]
fn a_turn_ended_is_one_new_turn_for_the_finish_line() {
    let harness = start("herdr-turns");
    let (h, _one) = herdr(&harness);
    let repo = repo_in(&harness.dir);
    let (_, pane, started) = delegated(&h, &repo, "claude");
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let armed = handled(&started);

    let first = turn(&h, &pane, &provider);
    assert!(after_handled(&armed, &first), "{first}");
    let done = handled(&first);
    let again = h.agent_get(&pane).unwrap().unwrap();
    assert!(
        !after_handled(&done, &again),
        "the same turn, read again: {again}"
    );
    assert_eq!(again.at("completion_seq"), first.at("completion_seq"));

    let prompted = h.agent_prompt(&pane, "more").unwrap();
    assert!(after_handled(&done, prompted.at("agent")), "{prompted}");
    provider
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    let second = agent_until(&h, &pane, |a| {
        a.at("completion_seq") != first.at("completion_seq")
    });
    assert!(after_handled(&done, &second), "{second}");
    assert!(
        second.i("completion_seq") > done.i("comp"),
        "a missed turn's count is past the handled one: {second}"
    );
    assert_eq!(second.t("terminal_id"), done.t("term"));
}

/// The fallback hand-over (`finish/hand_over.rs`): the agent that cannot go
/// on is ended (`pane_quit`), then the next one starts in the same pane
/// (`agent start`). Between the two the pane answers at its shell, so the
/// hook's reconcile finds the agent exited, never its pane gone; after, the
/// pane names a running Thread, whose readings herdr's own answer confirms
/// as the pane's newest terminal. The next agent of the same kind is the
/// same Thread revived, its count past the handled turn.
#[test]
fn a_fallback_hand_over_keeps_the_task_s_pane_on_the_real_server() {
    let harness = start("herdr-fallback");
    let (h, _one) = herdr(&harness);
    let repo = repo_in(&harness.dir);
    let (space, pane, started) = delegated(&h, &repo, "claude");
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let worker = started.t("terminal_id");
    let done = handled(&turn(&h, &pane, &provider));

    // To codex.
    assert!(h.pane_get(&pane).is_some());
    assert!(h.agent_get(&pane).unwrap().is_some());
    assert!(h.pane_quit(&pane, 6000));
    let shell = h.pane_get(&pane).expect("the pane answers at its shell");
    assert_eq!(shell.s("workspace_id"), space, "{shell}");
    assert_eq!(shell.s("agent_status"), "unknown", "{shell}");
    assert!(h.agent_get(&pane).unwrap().is_none(), "its agent exited");
    assert!(h.tab_rename(&pane, "Agent (codex)").is_some());
    let codex = h
        .agent_start(
            "t-0001",
            "codex",
            &pane,
            &["--model".into(), "c".into()],
            10000,
        )
        .unwrap();
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let next = codex.t("terminal_id");
    assert_ne!(next, worker, "another kind is another Thread");
    assert_eq!(codex.s("agent"), "codex", "{codex}");
    assert_eq!(codex.s("pane_id"), pane, "{codex}");
    let now = h.pane_get(&pane).unwrap();
    assert_eq!(now.t("terminal_id"), next, "the pane names it: {now}");
    assert!(same_dir(now.s("cwd"), Path::new(&space)), "{now}");
    assert_eq!(h.tab_get(&pane).unwrap().s("label"), "Agent (codex)");
    assert!(
        after_handled(&done, &codex),
        "a new terminal re-arms: {codex}"
    );
    let tabs = h.tabs(&space).unwrap();
    assert_eq!(
        tabs.len(),
        1,
        "the old Thread is parked, not a tab: {tabs:?}"
    );
    let done = handled(&turn(&h, &pane, &provider));

    // To codex again, other arguments: the same Thread revived.
    assert!(h.pane_quit(&pane, 6000));
    assert!(h.pane_get(&pane).is_some(), "the pane answers at its shell");
    let revived = h
        .agent_start(
            "t-0001",
            "codex",
            &pane,
            &["--model".into(), "d".into()],
            10000,
        )
        .unwrap();
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(revived.t("terminal_id"), next, "the same Thread: {revived}");
    let after = turn(&h, &pane, &provider);
    assert!(after.i("completion_seq") > done.i("comp"), "{after}");
    assert!(after_handled(&done, &after), "{after}");
}

/// A reviewer per round (`flow/tabs.rs`): a tab of its own in the task's
/// Space, named after its round and renamed once its agent is known, then
/// closed with its Thread; the working agent's Thread is untouched, and the
/// next round's tab is a new one.
#[test]
fn a_reviewer_tab_per_round_leaves_the_agent_s_thread_on_the_real_server() {
    let harness = start("herdr-reviewer");
    let (h, _one) = herdr(&harness);
    let repo = repo_in(&harness.dir);
    let (space, pane, started) = delegated(&h, &repo, "claude");
    let _worker = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let worker = started.t("terminal_id");
    let env = [("HERDR_PROJECTS_DATA_DIR".to_string(), "/data".to_string())];

    let mut seen = vec![pane.clone()];
    for round in 1..=2 {
        let label = format!("Reviewer #{round}");
        let made = h.tab_create(&space, &space, &label, &env).unwrap();
        let tab = made.at("tab").t("tab_id");
        let reviewer = made.at("root_pane").t("pane_id");
        assert!(!seen.contains(&reviewer), "a new tab each round: {made}");
        seen.push(reviewer.clone());
        assert_eq!(made.at("tab").s("label"), label, "{made}");
        let review = h
            .agent_start(
                &format!("t-0001-review-{round}"),
                "claude",
                &reviewer,
                &[],
                10000,
            )
            .unwrap();
        harness
            .sessions
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_ne!(review.t("terminal_id"), worker, "{review}");
        assert!(h.tab_rename(&tab, &format!("{label} (claude)")).is_some());
        let labels: Vec<String> = h
            .tabs(&space)
            .unwrap()
            .iter()
            .map(|t| t.t("label"))
            .collect();
        assert_eq!(labels.len(), 2, "{labels:?}");
        assert!(labels.contains(&"Agent".to_string()), "{labels:?}");
        assert!(labels.contains(&format!("{label} (claude)")), "{labels:?}");

        assert!(h.tab_close(&tab).is_some());
        assert!(h.tab_get(&tab).is_none(), "closed with its pane");
        let parked = call(&harness).answer(
            "1",
            "thread.get",
            serde_json::json!({ "thread": thread_of(&review) }),
        );
        assert_eq!(
            parked["error"]["code"], "unknown_thread",
            "parked: {parked}"
        );
        let agent = h.agent_get(&pane).unwrap().expect("the working agent");
        assert_eq!(agent.t("terminal_id"), worker, "{agent}");
        assert_eq!(h.tab_get(&pane).unwrap().s("label"), "Agent");
        assert_eq!(h.tabs(&space).unwrap().len(), 1);
    }
}
