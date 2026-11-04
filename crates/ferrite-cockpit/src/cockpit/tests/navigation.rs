//! The title bar: back and forward through the UI's history (⌘[ / ⌘]),
//! and the tools at the row's right end.
use super::*;
use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject};

fn shown(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Option<ThreadId> {
    view.read_with(cx, |view, _| view.focused_thread())
}

fn ends(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> (bool, bool) {
    view.read_with(cx, |view, _| {
        (view.history.can_go_back(), view.history.can_go_forward())
    })
}

/// Claude's arrows: ⌘[ goes back to the Thread left, ⌘] forward again,
/// and each is disabled at its end.
#[gpui::test]
fn back_and_forward_walk_the_threads_shown(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("history-threads", 2);
    let threads = core.threads();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let first = shown(&view, cx).expect("a Thread shows");
    let second = *threads.iter().find(|thread| **thread != first).unwrap();
    assert_eq!(ends(&view, cx), (false, false), "nowhere to go yet");

    view.update(cx, |view, cx| view.focus_thread(second, cx));
    tick(cx);
    assert_eq!(ends(&view, cx), (true, false));

    cx.simulate_keystrokes("cmd-[");
    tick(cx);
    assert_eq!(shown(&view, cx), Some(first));
    assert_eq!(ends(&view, cx), (false, true));

    cx.simulate_keystrokes("cmd-]");
    tick(cx);
    assert_eq!(shown(&view, cx), Some(second));
    assert_eq!(ends(&view, cx), (true, false));
}

/// The arrows stand right of the traffic lights in the sidebar's head, before
/// the title, and a click on the live one steps; folding the sidebar leaves
/// them where they were, the title still clear of them; they show on the
/// empty board too, where back is the one thing to do.
#[gpui::test]
fn the_arrows_lead_the_head_and_step_on_a_click(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("history-arrows", 2);
    let threads = core.threads();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    tick(cx);
    let first = shown(&view, cx).unwrap();
    let second = *threads.iter().find(|thread| **thread != first).unwrap();
    view.update(cx, |view, cx| view.focus_thread(second, cx));
    tick(cx);

    let back = cx.debug_bounds("history-back").expect("the back arrow");
    let forward = cx
        .debug_bounds("history-forward")
        .expect("the forward arrow");
    let title = cx.debug_bounds("view-title").expect("the title");
    let bell = cx.debug_bounds("nav-activity").expect("the sidebar's bell");
    assert_eq!(back.left(), px(ferrite_ui::theme::HISTORY_X));
    assert!(
        forward.right() < bell.left(),
        "in the sidebar head, before its icons"
    );
    assert!(back.right() < title.left(), "the arrows come first");
    assert!(back.bottom() <= px(ferrite_ui::theme::WIN_CHROME_H));
    cx.simulate_click(back.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(shown(&view, cx), Some(first));

    view.update(cx, |view, cx| {
        view.nav_forced_open = false;
        view.set_nav_collapsed(true, cx);
    });
    tick(cx);
    assert_eq!(
        cx.debug_bounds("history-back").map(|bounds| bounds.left()),
        Some(back.left()),
        "folded, the same spot"
    );
    let title = cx.debug_bounds("view-title").expect("the title");
    let forward = cx.debug_bounds("history-forward").unwrap();
    assert!(forward.right() < title.left(), "the title clears them");

    view.update(cx, |view, cx| {
        view.empty_shown = true;
        cx.notify();
    });
    tick(cx);
    assert!(
        cx.debug_bounds("history-back").is_some(),
        "on the empty board"
    );
    assert!(cx.debug_bounds("view-title").is_none());
}

/// A parked Thread is somewhere to go back to: the step revives it.
#[gpui::test]
fn back_revives_a_parked_thread(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("history-parked", 2);
    let threads = core.threads();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let first = shown(&view, cx).unwrap();
    let second = *threads.iter().find(|thread| **thread != first).unwrap();
    view.update(cx, |view, cx| view.focus_thread(second, cx));
    tick(cx);
    view.update(cx, |view, cx| {
        view.close_pane(PaneIdentity::Thread(second), cx)
    });
    tick(cx);
    assert_eq!(shown(&view, cx), Some(first), "parking shows the other");

    cx.simulate_keystrokes("cmd-[");
    tick(cx);
    assert_eq!(shown(&view, cx), Some(second), "back opened it again");
}

/// A Group is a place, and back leaves it for the Thread shown before.
#[gpui::test]
fn back_leaves_a_group_for_the_thread_before(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("history-group", 3);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[1],
            second: threads[2],
        })
        .unwrap()
        .group
        .unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.focus_thread(threads[0], cx));
    tick(cx);
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);

    cx.simulate_keystrokes("cmd-[");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.roster().view(), View::Solo);
        assert_eq!(view.focused_thread(), Some(threads[0]));
    });
    cx.simulate_keystrokes("cmd-]");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.roster().view(), View::Group(group))
    });
}

/// Opening a Subagent is a place: ⌘[ goes from it back to Main.
#[gpui::test]
fn back_goes_from_a_subagent_to_main(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("history-subagent", 1);
    let thread = core.threads()[0];
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let agent = AgentKey::new(Provider::Claude, "native", "helper");
    let mut info = AgentInfo::new(agent.clone());
    info.parent = Some(Subject::Main);
    let stream = &fake.streams.borrow()[0];
    stream
        .send(SessionEvent::Activity(ActivityEvent::Discovered(info)))
        .unwrap();
    stream
        .send(SessionEvent::Activity(ActivityEvent::Status {
            key: agent.clone(),
            state: AgentStatus::Working,
        }))
        .unwrap();
    tick(cx);
    let subject = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.panes[0].selected.clone())
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_subject(thread, Subject::Subagent(agent.clone()), window, cx)
        })
    });
    tick(cx);

    cx.simulate_keystrokes("cmd-[");
    tick(cx);
    assert_eq!(subject(&view, cx), Subject::Main);
    cx.simulate_keystrokes("cmd-]");
    tick(cx);
    assert_eq!(subject(&view, cx), Subject::Subagent(agent));
}

/// The tools stand at the row's right end against the panel's left edge,
/// the rail's door after the split, `TITLE_TOOLS_GAP` apart; the head leaves their room.
#[gpui::test]
fn the_title_tools_stand_against_the_panel(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{TITLE_TOOLS_GAP, VIEW_HEAD_PAD_X};
    let (core, _fake) = cockpit("title-tools", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    view.update(cx, |view, cx| {
        view.prefs.settings.panel_shown = true;
        cx.notify();
    });
    tick(cx);
    let panel = cx.debug_bounds("panel").expect("the panel");
    let rail = cx.debug_bounds("panel-toggle").expect("the rail's door");
    let split = cx.debug_bounds("title-split").expect("split beside");
    let tools = cx.debug_bounds("title-tools").expect("the tools");
    assert_eq!(panel.left() - tools.right(), px(VIEW_HEAD_PAD_X));
    assert_eq!(rail.left() - split.right(), px(TITLE_TOOLS_GAP), "Claude's pitch");
    hide_panel(&view, cx);
    tick(cx);
    let head = cx.debug_bounds("view-head").unwrap();
    let title = cx.debug_bounds("view-title").unwrap();
    let split = cx.debug_bounds("title-split").expect("they stay");
    assert!(title.right() < split.left(), "the title runs short of them");
    assert!(head.right() >= cx.debug_bounds("panel-toggle").unwrap().right());
}
