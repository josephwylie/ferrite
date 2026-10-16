//! herdr-projects' herdr calls answered by the real Server (ADR 0031): the
//! board's own `Herdr` client, through its Ferrite seam, against this
//! Server's API socket, so a shape the seam reads and the Server answers
//! cannot drift apart unseen.

use super::*;
use sw_core::herdr::Herdr;
use sw_core::json::*;

/// The board keeps its placed panes beside its data, in a folder the
/// process names once: every test here shares it.
fn board_data() -> PathBuf {
    static DATA: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        let dir = scratch("herdr-board-data");
        std::env::set_var("HERDR_PROJECTS_DATA_DIR", &dir);
        dir
    })
    .clone()
}

fn herdr(harness: &Harness) -> Herdr {
    board_data();
    Herdr::new(Some(harness.api.display().to_string()))
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
    let h = herdr(&harness);
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
    let h = herdr(&harness);
    let cwd = harness.dir.display().to_string();
    let made = h.workspace_create(&cwd, "", &[]).unwrap();
    let pane = made.at("root_pane").t("pane_id");
    let started = h.agent_start("t-0002", "codex", &pane, &[], 10000).unwrap();
    let thread = started.t("terminal_id");
    let thread: u64 = thread.split('@').next().unwrap().parse().unwrap();
    h.pane_focus(&pane).unwrap();
    assert_eq!(
        told(&mut window, |message| matches!(
            message,
            ServerMessage::Focus { .. }
        )),
        ServerMessage::Focus {
            thread: ThreadId::new(thread)
        }
    );
}
