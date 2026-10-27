//! The right panel (SPEC §5.6): it follows the focused Thread in every
//! view, with no size rules; the one toggle and its setting; the tab each
//! Thread remembers.
use super::*;
use crate::cockpit::panel::PanelTab;

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

/// A Group of `count` Threads at 1600, entered, the nav held open.
fn group<'a>(
    name: &str,
    count: usize,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let (mut core, _fake) = cockpit(name, count);
    let group = group_all(&mut core);
    core.enter_group(group).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    own_settings(&view, name, cx);
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
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
    view.read_with(cx, |view, _| view.panel_thread())
}

/// Focus `thread`'s Pane on the board shown, without changing the view.
fn focus(view: &Entity<CockpitView>, thread: ThreadId, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| {
        let index = view.pane_for(thread).expect("on the board");
        view.focus_pane(index);
        cx.notify();
    });
    tick(cx);
}

/// The Solo card's right edge, as the board laid it out.
fn card_right(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> f32 {
    let card = cx.update(|window, cx| view.read(cx).pane_rects(window))[0].1;
    card.x + card.w
}

/// No size rules: shown, the panel stays at any width down to the window
/// minimum, and the board gives up its column.
#[gpui::test]
fn the_panel_never_hides_for_width(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{PANEL_W, WINDOW_MIN_H, WINDOW_MIN_W};
    let (view, cx) = solo("panel-any-width", 1, 1600., cx);
    for width in [1600., 1150., 900., WINDOW_MIN_W] {
        cx.simulate_resize(gpui::size(px(width), px(900.)));
        tick(cx);
        assert!(cx.debug_bounds("panel").is_some(), "shown at {width}");
        assert_eq!(card_right(&view, cx), width - PANEL_W, "at {width}");
    }
    cx.simulate_resize(gpui::size(px(WINDOW_MIN_W), px(WINDOW_MIN_H)));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some(), "at the window minimum");
}

/// In a Group the panel is the focused Pane's Thread's: focusing another
/// Pane switches it, each Thread keeps its last tab, and a fullscreen Pane
/// keeps it. A draft has no Thread, so no panel and no toggle.
#[gpui::test]
fn the_panel_follows_the_focused_group_pane(cx: &mut TestAppContext) {
    let (view, cx) = group("panel-group", 2, cx);
    let threads = view.read_with(cx, |view, _| view.cockpit.threads());
    focus(&view, threads[0], cx);
    assert!(cx.debug_bounds("panel").is_some(), "a Group shows it");
    assert_eq!(shown(&view, cx), Some(threads[0]));
    let pr = cx.debug_bounds("panel-tab-Pr").expect("the PR tab");
    cx.simulate_click(pr.center(), gpui::Modifiers::none());
    tick(cx);

    focus(&view, threads[1], cx);
    assert_eq!(shown(&view, cx), Some(threads[1]), "focus switches it");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel_tab(threads[1]), PanelTab::Changes, "a new one");
    });
    focus(&view, threads[0], cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.panel_tab(threads[0]), PanelTab::Pr, "remembered");
    });

    cx.simulate_keystrokes("cmd-shift-enter");
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.cockpit.roster().fullscreen().is_some()),
        "the premise: a fullscreen Pane"
    );
    assert_eq!(shown(&view, cx), Some(threads[0]), "fullscreen keeps it");
    let rects = cx.update(|window, cx| view.read(cx).pane_rects(window));
    let card = rects[0].1;
    assert_eq!(card.x + card.w, 1600. - ferrite_ui::theme::PANEL_W);
    cx.simulate_keystrokes("cmd-shift-enter");
    tick(cx);

    view.update(cx, |view, cx| {
        view.open_draft_with_placement(DraftTarget::Main, DraftPlacement::CurrentGroup, cx)
    });
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none(), "a draft has none");
    assert!(cx.debug_bounds("panel-toggle").is_none(), "nor a toggle");
}

/// With the Beside slot open — a reader in the Pane, or another Thread
/// beside it — the panel stays, for whichever Thread holds focus.
#[gpui::test]
fn the_panel_stays_with_the_beside_slot_open(cx: &mut TestAppContext) {
    let (view, cx) = solo("panel-beside", 2, 1600., cx);
    let threads = view.read_with(cx, |view, _| view.cockpit.threads());
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
    view.update(cx, |view, cx| {
        let index = view.pane_for(threads[0]).unwrap();
        view.close_preview(index, cx)
    });
    tick(cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_beside(beside::Beside::Thread(threads[1]), window, cx)
        })
    });
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.visible_indices().len()),
        2,
        "the premise: a Thread beside"
    );
    assert_eq!(shown(&view, cx), Some(threads[0]));
    focus(&view, threads[1], cx);
    assert_eq!(shown(&view, cx), Some(threads[1]), "it follows focus");
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
        1600. - ferrite_ui::theme::BOARD_PAD,
        "the Pane takes its width"
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

/// One toggle, in one place: shown, it is the panel head's last control;
/// hidden, the view head's, at the same spot. The head stops at the
/// panel's left edge, and runs to the toggle once the panel is hidden.
fn toggle_sits_in_one_place(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
    which: &str,
) {
    use ferrite_ui::theme::PANEL_W;
    let panel = cx.debug_bounds("panel").expect("shown");
    let shown = cx.debug_bounds("panel-toggle").expect("the panel's toggle");
    assert!(
        panel.contains(&shown.center()),
        "{which}: in the panel head"
    );
    let head = cx.debug_bounds("view-head").expect("the view head");
    assert_eq!(head.right(), px(1600. - PANEL_W), "{which}: head stops");
    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none());
    let hidden = cx.debug_bounds("panel-toggle").expect("the head's toggle");
    assert_eq!(hidden, shown, "{which}: the same place");
    let head = cx.debug_bounds("view-head").unwrap();
    assert!(head.contains(&hidden.center()), "{which}: in the view head");
    assert_eq!(head.right(), hidden.right(), "{which}: at its right end");
    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    tick(cx);
}

#[gpui::test]
fn the_solo_toggle_sits_in_one_place(cx: &mut TestAppContext) {
    let (view, cx) = solo("panel-toggle-solo", 1, 1600., cx);
    toggle_sits_in_one_place(&view, cx, "Solo");
}

#[gpui::test]
fn the_group_toggle_sits_in_one_place(cx: &mut TestAppContext) {
    let (view, cx) = group("panel-toggle-group", 2, cx);
    toggle_sits_in_one_place(&view, cx, "Group");
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
            view.run_palette_act(Act::Command(Command::TogglePanel), false, window, cx)
        })
    });
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none(), "the command hides it");
    assert!(view.read_with(cx, |view, _| !view.prefs.settings.panel_shown));
}

/// Each Thread keeps its last tab; a new one opens on Changes; opening a
/// tab from hidden shows the panel on it.
#[gpui::test]
fn the_tab_each_thread_keeps(cx: &mut TestAppContext) {
    let (view, cx) = solo("panel-tabs", 2, 1600., cx);
    let threads = view.read_with(cx, |view, _| view.cockpit.threads());
    view.update(cx, |view, cx| view.focus_thread(threads[0], cx));
    tick(cx);
    let tab = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext, thread| {
        view.read_with(cx, |view, _| view.panel_tab(thread))
    };
    assert_eq!(tab(&view, cx, threads[0]), PanelTab::Changes);
    let pr = cx.debug_bounds("panel-tab-Pr").expect("the PR tab");
    cx.simulate_click(pr.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(tab(&view, cx, threads[0]), PanelTab::Pr);
    assert!(
        cx.debug_bounds("panel-no-pr").is_some(),
        "no pull request for the branch"
    );
    view.update(cx, |view, cx| view.focus_thread(threads[1], cx));
    tick(cx);
    assert_eq!(tab(&view, cx, threads[1]), PanelTab::Changes, "a new one");
    view.update(cx, |view, cx| view.focus_thread(threads[0], cx));
    tick(cx);
    assert_eq!(tab(&view, cx, threads[0]), PanelTab::Pr, "remembered");

    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none());
    view.update(cx, |view, cx| {
        view.open_panel_tab(threads[0], PanelTab::Changes, cx)
    });
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some(), "opening a tab shows it");
    assert_eq!(tab(&view, cx, threads[0]), PanelTab::Changes);
    assert!(
        cx.debug_bounds("panel-no-changes").is_some(),
        "nothing edited yet"
    );
}

/// The foot reads the shown Thread's context, live: another Thread's usage
/// is not this panel's, and showing that Thread brings its reading.
#[gpui::test]
fn the_foot_reads_the_shown_threads_context(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("panel-foot", 2);
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
        cx.debug_bounds("panel-foot-99000").is_none(),
        "an unshown Thread's usage is not the panel's"
    );
    view.update(cx, |view, cx| view.focus_thread(threads[1], cx));
    tick(cx);
    assert!(cx.debug_bounds("panel-foot-99000").is_some());
}
