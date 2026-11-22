//! The right panel (SPEC §5.6): it follows the focused Thread in every
//! view, with no size rules; the one toggle and its setting; one scroll of
//! sections with no tabs.
use super::*;

/// A Solo window of `count` Threads at `width`, the nav held open.
fn solo<'a>(
    name: &str,
    count: usize,
    width: f32,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let (core, _fake) = cockpit(name, count);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    own_settings(&view, name, cx);
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(width), px(900.)));
    tick(cx);
    (view, cx)
}

/// A settings file of the window's own: these tests save the toggle, and
/// the suite's windows otherwise share one file.
fn own_settings(view: &Entity<CockpitView>, name: &str, cx: &mut gpui::VisualTestContext) {
    let dir = scratch(&format!("{name}-settings"));
    std::fs::create_dir_all(&dir).unwrap();
    view.update(cx, |view, _| view.prefs.dir = dir);
}

/// The Thread the panel shows for this frame.
fn shown(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Option<ThreadId> {
    view.read_with(cx, |view, cx| view.panel_thread(cx).map(|thread| thread.thread()))
}

/// The Solo card's right edge, as the board laid it out.
fn card_right(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> f32 {
    let card = cx.update(|window, cx| view.read(cx).pane_rects(window, cx))[0].1;
    card.x + card.w
}

/// No size rules: shown, the panel stays at any width down to the window
/// minimum, and the board gives up its column.
#[gpui::test]
fn the_panel_never_hides_for_width(cx: &mut TestAppContext) {
    use crate::cockpit::panel::least_w;
    use ferrite_ui::theme::{WINDOW_MIN_H, WINDOW_MIN_W};
    let (view, cx) = solo("panel-any-width", 1, 1600., cx);
    for width in [1600., 1150., 900., WINDOW_MIN_W] {
        cx.simulate_resize(gpui::size(px(width), px(900.)));
        tick(cx);
        assert!(cx.debug_bounds("panel").is_some(), "shown at {width}");
        assert_eq!(card_right(&view, cx), width - least_w(), "at {width}");
    }
    cx.simulate_resize(gpui::size(px(WINDOW_MIN_W), px(WINDOW_MIN_H)));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some(), "at the window minimum");
}

/// The panel is the shown Thread's: showing another Thread switches it.
/// A draft has no Thread, so no panel and no toggle.
#[gpui::test]
fn the_panel_follows_the_shown_thread(cx: &mut TestAppContext) {
    let (view, cx) = solo("panel-follows", 2, 1600., cx);
    let threads = view.read_with(cx, |view, cx| view.cockpit.read(cx).threads());
    view.update(cx, |view, cx| view.focus_thread(threads[0], cx));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some(), "a Thread shows it");
    assert_eq!(shown(&view, cx), Some(threads[0]));

    view.update(cx, |view, cx| view.focus_thread(threads[1], cx));
    tick(cx);
    assert_eq!(shown(&view, cx), Some(threads[1]), "it follows the Thread on screen");

    view.update(cx, |view, cx| {
        view.open_draft(DraftTarget::Main, cx)
    });
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none(), "a draft has none");
    assert!(cx.debug_bounds("panel-toggle").is_none(), "nor a toggle");
}

/// With a reader open in the Pane the panel stays, for the Thread on
/// screen.
#[gpui::test]
fn the_panel_stays_with_a_reader_open(cx: &mut TestAppContext) {
    let (view, cx) = solo("panel-beside", 1, 1600., cx);
    let threads = view.read_with(cx, |view, cx| view.cockpit.read(cx).threads());
    view.update(cx, |view, cx| view.focus_thread(threads[0], cx));
    tick(cx);
    let dir = scratch("panel-reader");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("notes.txt");
    std::fs::write(&file, "one\ntwo\n").unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_preview(file.clone(), None, window, cx)
        })
    });
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view
            .pane_for(threads[0])
            .is_some_and(|index| view.panes[index].preview.document().is_some())),
        "the premise: a preview open"
    );
    assert_eq!(shown(&view, cx), Some(threads[0]), "a preview keeps it");
}

/// ⌥⌘B and the toggle hide and show it, and the choice is the saved
/// setting: it holds across launches.
#[gpui::test]
fn toggle_panel_flips_the_saved_setting(cx: &mut TestAppContext) {
    let (view, cx) = solo("panel-toggle", 1, 1600., cx);
    let saved = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            let loaded = ferrite_core::settings::Settings::load(&view.prefs.dir).unwrap();
            assert_eq!(loaded.panel_shown, view.prefs.settings.panel_shown);
            loaded.panel_shown
        })
    };
    cx.simulate_keystrokes("cmd-alt-b");
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none(), "⌥⌘B hides it");
    assert!(!saved(&view, cx), "and saves that");
    assert_eq!(
        card_right(&view, cx),
        1600.,
        "the Pane takes its width, flush to the window's edge"
    );
    cx.simulate_keystrokes("cmd-alt-b");
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some(), "and shows it again");
    assert!(saved(&view, cx));
    let click_toggle = |cx: &mut gpui::VisualTestContext| {
        let toggle = cx.debug_bounds("panel-toggle").expect("the toggle");
        cx.simulate_click(toggle.center(), gpui::Modifiers::none());
        tick(cx);
    };
    click_toggle(cx);
    assert!(cx.debug_bounds("panel").is_none(), "the toggle hides it");
    assert!(!saved(&view, cx));
    click_toggle(cx);
    assert!(cx.debug_bounds("panel").is_some(), "and, hidden, shows it");
    assert!(saved(&view, cx));
}

/// The title tools stay at the window's edge whether the column shows or
/// not (Claude's do not move); the head stops at the panel while it shows
/// and runs under the tools once it is hidden.
fn toggle_sits_in_one_place(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
    which: &str,
) {
    use crate::cockpit::panel::least_w;
    let panel = cx.debug_bounds("panel").expect("shown");
    let tools = cx.debug_bounds("title-tools").expect("the tools");
    assert!(cx.debug_bounds("panel-toggle").is_some(), "{which}: the rail's door");
    assert!(tools.left() > panel.left(), "{which}: over the panel's own corner");
    let head = cx.debug_bounds("view-head").expect("the view head");
    assert_eq!(head.right(), px(1600. - least_w()), "{which}: head stops");
    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none());
    let hidden = cx.debug_bounds("title-tools").expect("the tools stay");
    assert_eq!(hidden, tools, "{which}: they do not move");
    let head = cx.debug_bounds("view-head").unwrap();
    assert!(head.right() >= hidden.right(), "{which}: head runs under them");
    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    tick(cx);
}

#[gpui::test]
fn the_solo_toggle_sits_in_one_place(cx: &mut TestAppContext) {
    let (view, cx) = solo("panel-toggle-solo", 1, 1600., cx);
    toggle_sits_in_one_place(&view, cx, "Solo");
}

/// The palette's `Toggle panel` runs the same act, wherever there is a
/// Thread for the panel.
#[gpui::test]
fn the_palette_toggles_the_panel(cx: &mut TestAppContext) {
    use crate::palette::{Act, Command};
    let (view, cx) = solo("panel-palette", 1, 1600., cx);
    let names = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.palette_command_rows(window, cx)
                .into_iter()
                .map(|row| row.name.to_string())
                .collect::<Vec<_>>()
        })
    });
    assert!(names.iter().any(|name| name == "Toggle panel"), "{names:?}");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_act(Act::Command(Command::TogglePanel), window, cx)
        })
    });
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none(), "the command hides it");
    assert!(view.read_with(cx, |view, _| !view.prefs.settings.panel_shown));
}

/// A Thread's Panel opens on its Overview tab, one scroll of sections:
/// Environment, then Changes and Context once they have something to say
/// (no filler line stands in for them). Opening it from hidden shows it.
#[gpui::test]
fn the_overview_is_one_scroll_of_sections(cx: &mut TestAppContext) {
    let (view, cx) = solo("panel-sections", 1, 1600., cx);
    let thread = view.read_with(cx, |view, cx| view.cockpit.read(cx).threads()[0]);
    for section in ["panel-environment-head", "panel-environment-machine"] {
        assert!(cx.debug_bounds(section).is_some(), "{section}");
    }
    for section in ["panel-changes-head", "panel-context-head"] {
        assert!(cx.debug_bounds(section).is_none(), "empty: {section}");
    }
    assert!(cx.debug_bounds("panel-tab-Overview").is_some(), "its one tab");
    assert!(cx.debug_bounds("panel-pr").is_none(), "no pull request yet");

    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none());
    view.update(cx, |view, cx| view.open_panel(view.cockpit.read(cx).thread_key(thread), cx));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some(), "opening it shows it");
}

/// The pull request is a row in Changes once one exists, and the panel
/// snaps open and shut: its width is its own or nothing, from the first
/// frame.
#[gpui::test]
fn the_pull_request_is_a_row_in_changes(cx: &mut TestAppContext) {
    use crate::cockpit::panel::least_w;
    let (view, cx) = solo("panel-pr-row", 1, 1600., cx);
    let thread = view.read_with(cx, |view, cx| view.cockpit.read(cx).threads()[0]);
    view.update(cx, |view, cx| {
        view.facts.pr_answered(view.cockpit.read(cx).thread_key(thread), Some(pr_with_checks()));
        cx.notify();
    });
    tick(cx);
    let row = cx.debug_bounds("panel-pr").expect("the PR row");
    let changes = cx.debug_bounds("panel-changes-head").unwrap();
    assert!(row.top() > changes.top(), "under Changes");

    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("panel").is_none(), "shut at once");
    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    cx.run_until_parked();
    let panel = cx.debug_bounds("panel").expect("open at once");
    assert_eq!(panel.size.width, px(least_w()));
}

/// The Context section reads the shown Thread's context, live: another
/// Thread's usage is not this panel's, and showing that Thread brings its
/// reading. It is the one context meter: the panel has no foot.
#[gpui::test]
fn the_context_section_reads_the_shown_threads_context(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("panel-context", 2);
    let threads = core.threads().to_vec();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    view.update(cx, |view, cx| view.focus_thread(threads[0], cx));
    tick(cx);
    fake.streams.borrow()[1]
        .send(SessionEvent::TokenUsage {
            total_tokens: 99_000,
            input_tokens: 99_000,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_output_tokens: 0,
            context_window: Some(200_000),
        })
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some());
    assert!(
        cx.debug_bounds("panel-context-head").is_none(),
        "an unshown Thread's usage is not the panel's"
    );
    view.update(cx, |view, cx| view.focus_thread(threads[1], cx));
    tick(cx);
    let head = cx.debug_bounds("panel-context-head").expect("its head");
    let context = cx.debug_bounds("panel-context").expect("its reading");
    let environment = cx.debug_bounds("panel-environment-head").unwrap();
    assert!(environment.top() < head.top() && head.top() < context.top());
}

/// Connectors folds, as Claude's: its head says how many are connected,
/// and a press on it opens the servers' lines, and shuts them again.
#[gpui::test]
fn connectors_fold_under_their_count(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("panel-connectors", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    own_settings(&view, "panel-connectors", cx);
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    let server = |name: &str, status| ferrite_core::McpServer {
        name: name.into(),
        status,
        error: None,
    };
    fake.streams.borrow()[0]
        .send(SessionEvent::McpServers {
            servers: vec![
                server("github", ferrite_core::McpStatus::Connected),
                server("linear", ferrite_core::McpStatus::NeedsAuth),
            ],
        })
        .unwrap();
    tick(cx);
    let head = cx
        .debug_bounds("panel-connectors-head")
        .expect("Connectors, with a server");
    assert!(
        cx.debug_bounds("panel-connector-0").is_none(),
        "folded at first"
    );
    cx.simulate_click(head.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("panel-connector-0").is_some());
    assert!(cx.debug_bounds("panel-connector-1").is_some());
    cx.simulate_click(head.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("panel-connector-0").is_none(), "shut again");
}

/// Connectors eases open and shut over `FOLD`, and a press mid-way turns it
/// round from where it is, not from either end.
#[gpui::test]
fn connectors_ease_and_turn_round_from_where_they_are(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    let (core, fake) = cockpit("panel-connectors-ease", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    own_settings(&view, "panel-connectors-ease", cx);
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    let server = |name: &str| ferrite_core::McpServer {
        name: name.into(),
        status: ferrite_core::McpStatus::Connected,
        error: None,
    };
    fake.streams.borrow()[0]
        .send(SessionEvent::McpServers {
            servers: vec![server("github"), server("linear")],
        })
        .unwrap();
    tick(cx);
    let head = cx.debug_bounds("panel-connectors-head").unwrap();
    let body = |cx: &mut gpui::VisualTestContext| {
        cx.debug_bounds("panel-connectors-body")
            .map_or(0.0, |body| f32::from(body.size.height))
    };
    let step = |ms: u64, cx: &mut gpui::VisualTestContext| {
        cx.executor().advance_clock(Duration::from_millis(ms));
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    };
    cx.simulate_click(head.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    step(20, cx);
    step(20, cx);
    let part = body(cx);
    assert!(part > 0.0, "opening: {part}");
    step(ferrite_ui::theme::MOTION_FOLD_MS, cx);
    let open = body(cx);
    assert!(open > part, "open: {open} after {part}");
    cx.simulate_click(head.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    step(20, cx);
    step(20, cx);
    let closing = body(cx);
    assert!(closing > 0.0 && closing < open, "closing: {closing} of {open}");
    cx.simulate_click(head.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    step(1, cx);
    assert!(
        body(cx) >= closing - 1.0 && body(cx) < open,
        "a reversal starts from where it is: {} from {closing}",
        body(cx)
    );
    step(ferrite_ui::theme::MOTION_FOLD_MS, cx);
    assert_eq!(body(cx), open, "and opens fully");
}

/// The branch is the panel's, so while the panel is hidden the head says
/// it after the title's ▾, and while it shows the head does not.
#[gpui::test]
fn the_head_says_the_branch_while_the_panel_is_hidden(cx: &mut TestAppContext) {
    let (view, cx) = solo("panel-head-branch", 1, 1600., cx);
    view.update(cx, |view, cx| {
        let thread = view.panes[0].thread().unwrap();
        view.facts.set_branches(vec![(
            view.cockpit.read(cx).thread_key(thread),
            Some(ferrite_core::workspace::BranchStatus {
                branch: Some("lane-p".into()),
                ..Default::default()
            }),
        )]);
        cx.notify();
    });
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some(), "the premise: it shows");
    assert!(
        cx.debug_bounds("view-branch").is_none(),
        "the panel says it"
    );

    cx.simulate_keystrokes("cmd-alt-b");
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none());
    let menu = cx.debug_bounds("view-title-menu").expect("the ▾");
    let branch = cx.debug_bounds("view-branch").expect("the head says it");
    assert!(branch.left() >= menu.right(), "after the ▾");
}
