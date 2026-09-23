//! WP-C's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// The head is one 36px row: the checkout, the tasks meter and the PR/CI
/// chip all sit inside it, left to right, and nothing spills past the Pane.
#[gpui::test]
fn the_pane_head_is_one_row(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("head-one-row", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    view.update(cx, |view, cx| {
        view.facts
            .set_branches(vec![(thread, Some(branch_status_with_checks()))]);
        cx.notify();
    });
    for (id, status) in [
        ("1", ferrite_core::progress::StepStatus::Completed),
        ("2", ferrite_core::progress::StepStatus::Pending),
    ] {
        fake.streams.borrow()[0]
            .send(SessionEvent::Progress {
                event: ferrite_core::progress::ProgressEvent::Task {
                    id: id.into(),
                    subject: format!("step {id}"),
                    status: Some(status),
                    deleted: false,
                },
            })
            .unwrap();
    }
    tick(cx);
    let head = cx.debug_bounds("pane-head-1").expect("the head renders");
    assert_eq!(head.size.height, px(crate::theme::PANE_HEAD_H));
    let branch = cx.debug_bounds("project-branch-0").expect("the checkout");
    let meter = cx.debug_bounds("tasks-meter-1").expect("the tasks meter");
    let ci = cx.debug_bounds("ci-mark-1").expect("the PR/CI chip");
    for (name, part) in [("checkout", branch), ("meter", meter), ("ci", ci)] {
        assert!(
            part.top() >= head.top() && part.bottom() <= head.bottom(),
            "the {name} rides the head row: {part:?} / {head:?}"
        );
        assert!(
            part.right() <= head.right(),
            "the {name} stays inside the Pane: {part:?} / {head:?}"
        );
    }
    assert!(branch.right() < meter.left() && meter.right() < ci.left());
}

/// Closing the last Pane leaves a board that says how to start, and the
/// first new Thread takes its place.
#[gpui::test]
fn the_empty_cockpit_says_how_to_start(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("empty-board", 1);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-w", CloseThread, None),
            KeyBinding::new("cmd-n", NewThread, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    assert!(cx.debug_bounds("empty-board").is_none());
    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    view.read_with(cx, |view, _| assert!(view.panes.is_empty()));
    assert!(cx.debug_bounds("empty-board").is_some());
    cx.simulate_keystrokes("cmd-n");
    tick(cx);
    assert!(cx.debug_bounds("empty-board").is_none());
}

/// The empty board's keys come from the platform's own key table.
#[test]
fn the_empty_board_spells_keys_from_the_key_table() {
    let primary = match crate::keymap::PLATFORM {
        crate::keymap::Platform::Mac => "cmd",
        crate::keymap::Platform::Windows => "ctrl",
    };
    assert_eq!(
        CockpitView::key_label("cockpit::NewThread").as_deref(),
        Some(format!("{primary} N").as_str())
    );
    assert_eq!(
        CockpitView::key_label("cockpit::NewWorktreeThread").as_deref(),
        Some(format!("{primary} shift N").as_str())
    );
}
