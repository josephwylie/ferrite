//! The Solo panel (SPEC §5.6): when it shows, what hides it, the toggle
//! and its setting, and the tab each Thread remembers.
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
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(width), px(900.)));
    tick(cx);
    (view, cx)
}

/// The Solo card's right edge, as the board laid it out.
fn card_right(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> f32 {
    let card = cx.update(|window, cx| view.read(cx).pane_rects(window))[0].1;
    card.x + card.w
}

/// It shows in Solo from about 1280px and, once shown, holds down to about
/// 1180px; under that it hides and the Pane takes its width.
#[gpui::test]
fn the_panel_shows_where_the_window_has_room(cx: &mut TestAppContext) {
    use crate::theme::{BOARD_PAD, PANEL_W};
    let (view, cx) = solo("panel-room", 1, 1600., cx);
    assert!(cx.debug_bounds("panel").is_some(), "shown at 1600");
    assert_eq!(card_right(&view, cx), 1600. - PANEL_W);
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some(), "held at 1200");
    cx.simulate_resize(gpui::size(px(1150.), px(900.)));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none(), "hidden under 1180");
    assert_eq!(
        card_right(&view, cx),
        1150. - BOARD_PAD,
        "the Pane takes it"
    );
    cx.simulate_resize(gpui::size(px(1250.), px(900.)));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none(), "not back until 1280");
    cx.simulate_resize(gpui::size(px(1300.), px(900.)));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some());
}

/// Never on a Group, and never with the Beside slot open: a reader beside
/// the Pane takes the panel's place, and closing it brings the panel back.
#[gpui::test]
fn the_panel_gives_way_to_a_group_and_to_a_reader(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("panel-group", 2);
    let group = group_all(&mut core);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some(), "Solo shows it");

    let dir = scratch("panel-reader");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("notes.txt");
    std::fs::write(&file, "one\ntwo\n").unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_beside(
                beside::Beside::Reader {
                    path: file.clone(),
                    line: None,
                },
                window,
                cx,
            )
        })
    });
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.panes[0].preview.document().is_some()),
        "the premise: a reader beside the Pane"
    );
    assert!(
        cx.debug_bounds("panel").is_none(),
        "the reader takes its place"
    );
    view.update(cx, |view, cx| view.close_reader(0, cx));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some(), "and gives it back");
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none(), "a Group never shows it");
}

/// ⌥⌘B and the head's button hide and show it, and the choice is the
/// saved setting: it holds across launches.
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
        1600. - crate::theme::BOARD_PAD,
        "the Pane takes its width"
    );
    cx.simulate_keystrokes("cmd-alt-b");
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some(), "and shows it again");
    assert!(saved(&view, cx));
    let hide = cx.debug_bounds("panel-hide").expect("the head's button");
    cx.simulate_click(hide.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none(), "the button hides it");
    assert!(!saved(&view, cx));
}

/// The branch chip's door (`panel_changes_door`): Changes, from any tab or
/// from hidden; on Changes, it hides the panel. Each Thread keeps its
/// last tab; a new one opens on Changes.
#[gpui::test]
fn the_changes_door_and_the_tab_each_thread_keeps(cx: &mut TestAppContext) {
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

    let door = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| view.panel_changes_door(threads[0], cx));
        tick(cx);
    };
    door(&view, cx);
    assert_eq!(tab(&view, cx, threads[0]), PanelTab::Changes);
    assert!(cx.debug_bounds("panel").is_some());
    assert!(
        cx.debug_bounds("panel-no-changes").is_some(),
        "nothing edited yet"
    );
    door(&view, cx);
    assert!(cx.debug_bounds("panel").is_none(), "on Changes it hides");
    door(&view, cx);
    assert!(cx.debug_bounds("panel").is_some(), "and opens again");
    assert_eq!(tab(&view, cx, threads[0]), PanelTab::Changes);
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

/// The Solo head's branch chip is the Changes door: from another tab it
/// opens Changes, on Changes it hides the panel, and once more it shows it.
/// The head stops at the panel's left edge, and takes the row's width once
/// the panel is hidden.
#[gpui::test]
fn the_branch_chip_opens_changes_and_hides_the_panel(cx: &mut TestAppContext) {
    use crate::theme::{BOARD_PAD, PANEL_W};
    let (view, cx) = solo("panel-branch-chip", 1, 1600., cx);
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    view.update(cx, |view, cx| {
        view.facts.set_branches(vec![(
            thread,
            Some(ferrite_core::workspace::BranchStatus {
                branch: Some("feat/panel".into()),
                ..Default::default()
            }),
        )]);
        cx.notify();
    });
    tick(cx);
    let head = cx.debug_bounds("view-head").expect("the Solo head");
    assert_eq!(head.right(), px(1600. - PANEL_W), "it stops at the panel");
    let context = cx.debug_bounds("panel-tab-Context").unwrap();
    cx.simulate_click(context.center(), gpui::Modifiers::none());
    tick(cx);
    let tab = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.panel_tab(thread))
    };
    assert_eq!(tab(&view, cx), PanelTab::Context);
    let click_chip = |cx: &mut gpui::VisualTestContext| {
        let chip = cx.debug_bounds("branch-chip").expect("the branch chip");
        cx.simulate_click(chip.center(), gpui::Modifiers::none());
        tick(cx);
    };
    click_chip(cx);
    assert_eq!(tab(&view, cx), PanelTab::Changes, "the chip opens Changes");
    assert!(cx.debug_bounds("panel").is_some());
    click_chip(cx);
    assert!(cx.debug_bounds("panel").is_none(), "on Changes it hides");
    let head = cx.debug_bounds("view-head").unwrap();
    assert_eq!(
        head.right(),
        px(1600. - BOARD_PAD),
        "the head takes the row"
    );
    click_chip(cx);
    assert!(cx.debug_bounds("panel").is_some(), "and shows it again");
    assert_eq!(tab(&view, cx), PanelTab::Changes);
}
