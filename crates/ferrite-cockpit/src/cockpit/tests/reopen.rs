//! Closing, reopening and restarting a Panel's tabs: each kind comes back
//! as its lifecycle (`SurfaceId::lifecycle`) says, or says plainly why not.
use super::side_chat::side_tabs;
use super::*;
use crate::cockpit::surfaces::SurfaceId;
use crate::palette::{Act, Command};
use ferrite_core::activity::{
    ActivityEvent, AgentInfo, AgentKey, AgentStatus, ExecutionEvent, Subject, TranscriptCoverage,
};

fn agent_key(name: &str) -> AgentKey {
    AgentKey::new(Provider::Claude, "ui-fixture", name)
}

fn emit(fake: &Fake, event: ActivityEvent) {
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(event))
        .unwrap();
}

fn reopen_closed(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_act(Act::Command(Command::ReopenSurface), window, cx)
        })
    });
    tick(cx);
    tick(cx);
}

fn palette_names(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.palette_command_rows(window, cx)
                .into_iter()
                .map(|row| row.name.to_string())
                .collect()
        })
    })
}

fn click(name: impl Into<String>, cx: &mut gpui::VisualTestContext) {
    let name = name.into();
    let at = debug_bounds(cx, name.clone())
        .unwrap_or_else(|| panic!("{name} is drawn"))
        .center();
    cx.simulate_click(at, gpui::Modifiers::none());
    tick(cx);
}

#[gpui::test]
fn a_closed_agent_tab_reopens_the_same_agent_with_its_transcript(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("reopen-agent", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(800.)));
    let mut info = AgentInfo::new(agent_key("Quillon"));
    info.name = Some("Quillon".into());
    info.parent = Some(Subject::Main);
    info.coverage = TranscriptCoverage::Live;
    emit(&fake, ActivityEvent::Discovered(info));
    emit(
        &fake,
        ActivityEvent::Content {
            key: agent_key("Quillon"),
            id: Some("marker".into()),
            event: ExecutionEvent::Text {
                text: "marker-7f3a quillon\n\n".into(),
            },
        },
    );
    emit(
        &fake,
        ActivityEvent::Status {
            key: agent_key("Quillon"),
            state: AgentStatus::Idle,
        },
    );
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    assert!(
        !palette_names(&view, cx)
            .iter()
            .any(|name| name == "Reopen closed surface"),
        "nothing is closed yet, so nothing is offered"
    );
    view.update(cx, |view, cx| {
        view.open_agent_drill(thread, Subject::Subagent(agent_key("Quillon")), cx)
    });
    tick(cx);
    assert!(cx.debug_bounds("agent-drill").is_some());

    click("panel-tab-Agent-close", cx);
    assert!(cx.debug_bounds("agent-drill").is_none(), "the tab closed");
    assert!(palette_names(&view, cx)
        .iter()
        .any(|name| name == "Reopen closed surface"));

    reopen_closed(&view, cx);
    assert!(
        cx.debug_bounds("agent-drill").is_some(),
        "the agent's view is back"
    );
    assert!(cx.debug_bounds("agent-unavailable").is_none());
    view.read_with(cx, |view, _| {
        let subject = view
            .agent_drills
            .subject(thread)
            .expect("the same agent is held again");
        assert_eq!(*subject, Subject::Subagent(agent_key("Quillon")));
        let held = view.cockpit.thread(thread).unwrap();
        let transcript = held.activity().subject(subject).unwrap();
        assert!(plain_text(transcript.transcript().blocks()).contains("marker-7f3a"));
    });
}

#[gpui::test]
fn a_reopened_agent_the_thread_no_longer_knows_says_so_and_offers_to_close(
    cx: &mut TestAppContext,
) {
    let (core, _fake) = cockpit("reopen-agent-gone", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(800.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        view.upsert_surface(thread, SurfaceId::Agent("gone-agent".into()), cx)
    });
    tick(cx);
    assert!(
        cx.debug_bounds("agent-unavailable").is_some(),
        "a note, not a blank body"
    );
    click("agent-unavailable-close", cx);
    assert!(!panel_tabs(&view, thread, cx)
        .0
        .contains(&SurfaceId::Agent("gone-agent".into())));
}

#[gpui::test]
fn a_closed_side_chat_is_ended_not_reopened_and_no_fork_is_substituted(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("reopen-side-chat", 1);
    *fake.native_controls.borrow_mut() = true;
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "alpha\n\n".into(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.focused_thread().unwrap());
    view.update(cx, |view, cx| {
        view.open_side_chat(thread, Some("marker-91bc".into()), cx)
    });
    tick(cx);
    tick(cx);
    let side = |cx: &mut gpui::VisualTestContext| {
        panel_tabs(&view, thread, cx)
            .0
            .into_iter()
            .filter(|id| matches!(id, SurfaceId::SideChat(_)))
            .count()
    };
    assert_eq!(side(cx), 1);
    assert_eq!(fake.forks.borrow().len(), 1);

    let at = panel_tabs(&view, thread, cx)
        .0
        .iter()
        .position(|id| matches!(id, SurfaceId::SideChat(_)))
        .unwrap();
    let rows = view.read_with(cx, |view, _| view.surface_tab_rows(thread, at));
    assert!(
        rows.iter()
            .flatten()
            .any(|(item, _)| item.label.as_ref() == "End side chat"),
        "the tab's menu names what closing it does"
    );

    let id = side_tabs(&view, thread, cx)[0];
    click(format!("panel-tab-SideChat-{id}-close"), cx);
    assert_eq!(side(cx), 0);
    assert!(
        !palette_names(&view, cx)
            .iter()
            .any(|name| name == "Reopen closed surface"),
        "an ended side chat is not offered back"
    );
    reopen_closed(&view, cx);
    assert_eq!(side(cx), 0, "nothing comes back");
    assert_eq!(
        fake.forks.borrow().len(),
        1,
        "no new fork was made in its place"
    );
}

fn restart_view<'a>(
    name: &str,
    dir: &std::path::Path,
    checkout: &std::path::Path,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    ThreadId,
    &'a mut gpui::VisualTestContext,
) {
    let fake = Fake::default();
    let store = Store::open(scratch(name).join("threads")).unwrap();
    let mut core = Cockpit::new(store, Box::new(fake));
    core.open(
        Provider::Claude,
        WorkspaceChoice::Main {
            checkout: checkout.to_path_buf(),
        },
    )
    .unwrap();
    bind_production_keys(cx);
    let prefs = Preferences {
        dir: dir.to_path_buf(),
        ..Preferences::ephemeral()
    };
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, thread, cx)
}

#[gpui::test]
fn a_restart_restores_the_files_working_set_selection_and_preferences(cx: &mut TestAppContext) {
    let base = scratch("reopen-restart");
    let checkout = checkout_with_files(&base);
    let dir = base.join("prefs");
    {
        let (view, thread, cx) = restart_view("reopen-restart-a", &dir, &checkout, cx);
        view.update(cx, |view, cx| {
            view.open_in_files(thread, checkout.join("src/lib.rs"), None, true, cx);
            view.open_in_files(thread, checkout.join("README.md"), None, true, cx);
            view.open_in_files(thread, checkout.join("src/lib.rs"), None, true, cx);
            view.files.update(cx, |files, cx| files.run_verb(thread, crate::cockpit::tools::FilesVerb::ToggleWrap, cx));
        });
        tick(cx);
        assert_eq!(
            view.read_with(cx, |view, cx| view.files.read(cx).wraps(thread)),
            Some(false)
        );
    }
    cx.quit();
    let (view, thread, cx) = restart_view("reopen-restart-b", &dir, &checkout, cx);
    assert_eq!(
        panel_tabs(&view, thread, cx).1,
        Some(SurfaceId::Files),
        "the shown tool is back"
    );
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, cx| view.files.read(cx).tab_names(thread)),
        [
            ("lib.rs".to_string(), false),
            ("README.md".to_string(), false)
        ],
        "the open files return"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.files.read(cx).active_path(thread)),
        Some(checkout.join("src/lib.rs")),
        "with the one that was shown"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.files.read(cx).wraps(thread)),
        Some(false),
        "and its preferences"
    );
}

#[gpui::test]
fn a_file_missing_after_restart_says_so_and_can_be_tried_again(cx: &mut TestAppContext) {
    let base = scratch("reopen-missing");
    let checkout = checkout_with_files(&base);
    let dir = base.join("prefs");
    let note = checkout.join("note.txt");
    std::fs::write(&note, "n\n").unwrap();
    {
        let (view, thread, cx) = restart_view("reopen-missing-a", &dir, &checkout, cx);
        view.update(cx, |view, cx| {
            view.open_in_files(thread, note.clone(), None, true, cx);
            view.upsert_surface(thread, SurfaceId::File(note.clone()), cx);
        });
        tick(cx);
    }
    cx.quit();
    std::fs::remove_file(&note).unwrap();
    let (view, thread, cx) = restart_view("reopen-missing-b", &dir, &checkout, cx);
    assert!(
        panel_tabs(&view, thread, cx)
            .0
            .contains(&SurfaceId::File(note.clone())),
        "the tab is kept, not dropped"
    );
    until(cx, |cx| cx.debug_bounds("files-unshown").is_some());
    std::fs::write(&note, "n\n").unwrap();
    click("files-retry", cx);
    until(cx, |cx| cx.debug_bounds("files-unshown").is_none());

    view.update(cx, |view, cx| {
        view.upsert_surface(thread, SurfaceId::Files, cx)
    });
    tick(cx);
    std::fs::remove_file(&note).unwrap();
    assert!(view
        .read_with(cx, |view, cx| view.files.read(cx).tab_names(thread))
        .iter()
        .any(|(name, _)| name == "note.txt"));
}
