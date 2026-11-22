//! Shortcuts that open or jump to threads.
#[allow(unused_imports)]
use super::*;

/// AC3, one Thread at a time: ⌘D puts the Thread that needs answering on
/// screen in place of the one the operator was reading.
#[gpui::test]
fn one_key_shows_the_thread_that_needs_answering(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, fake) = cockpit("jump-solo", 3);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let threads = view.read_with(cx, |view, cx| view.cockpit.read(cx).threads());
    view.update(cx, |view, cx| view.focus_thread(threads[0], cx));
    fake.streams.borrow()[2].send(decision("perm_03")).unwrap();
    tick(cx);
    let shown = |cx: &mut gpui::VisualTestContext| -> Vec<ThreadKey> {
        threads
            .iter()
            .copied()
            .filter(|thread| debug_bounds(cx, format!("pane-root-{}", thread.thread().get())).is_some())
            .collect()
    };
    assert_eq!(shown(cx), [threads[0]]);

    cx.simulate_keystrokes("cmd-d");
    tick(cx);

    assert_eq!(shown(cx), [threads[2]], "the waiting Thread alone is on screen");
}

/// #20: cmd-t is the browser-tab spelling of a new Thread, and cmd-n —
/// the original — still works beside it. Both keys ride the same
/// cockpit::NewThread; the keymap table carries both rows.
#[gpui::test]
fn cmd_t_opens_a_new_thread_and_cmd_n_still_does(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("cmd-t", 1);
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-t", NewThread, None),
            KeyBinding::new("cmd-n", NewThread, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    cx.simulate_keystrokes("cmd-t");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes.len(), 2, "cmd-t opened a draft");
        assert!(view.panes[view.focused(cx)].draft().is_some());
    });

    // cmd-n reaches the same action; with that draft still up it
    // re-focuses it rather than stacking a second.
    cx.simulate_keystrokes("cmd-n");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes.len(), 2, "and cmd-n reuses the standing draft");
        assert!(view.panes[view.focused(cx)].draft().is_some());
    });
}

/// cmd-t on a Thread opens the new draft in that Thread's Project, not
/// wherever the last draft ran: the operator is in that Project now.
#[gpui::test]
fn cmd_t_from_a_thread_starts_in_its_project(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("cmd-t-project", 1);
    let elsewhere = core
        .register_project(&std::env::temp_dir().canonicalize().unwrap())
        .unwrap();
    let here = core.project_of(core.threads()[0]).expect("a bound Thread");
    assert_ne!(here, elsewhere);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-t", NewThread, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.launch_project = Some(elsewhere.project()));
    tick(cx);

    cx.simulate_keystrokes("cmd-t");
    view.read_with(cx, |view, cx| {
        let draft = view.panes[view.focused(cx)]
            .draft()
            .expect("cmd-t opened a draft");
        assert_eq!(draft.binding.project(), here.project());
    });
}

/// The Thread cmd-t is pressed on outranks the nav filter: the operator
/// is in that Thread's Project, whatever the nav is narrowed to.
#[gpui::test]
fn cmd_t_from_a_thread_outranks_the_nav_filter(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("cmd-t-over-filter", 1);
    let here = core.project_of(core.threads()[0]).expect("a bound Thread");
    let filtered = core
        .register_project(&std::env::temp_dir().canonicalize().unwrap())
        .unwrap();
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-t", NewThread, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.nav_filter = Some(filtered.project()));
    tick(cx);

    cx.simulate_keystrokes("cmd-t");
    view.read_with(cx, |view, cx| {
        let draft = view.panes[view.focused(cx)]
            .draft()
            .expect("cmd-t opened a draft");
        assert_eq!(draft.binding.project(), here.project());
    });
}

/// A draft left standing in another Project follows cmd-t to the
/// Thread the operator is in: the one it was pressed on, or, pressed on
/// the draft itself, the Thread focus last left.
#[gpui::test]
fn cmd_t_from_a_thread_brings_the_standing_draft_to_its_project(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("cmd-t-standing-draft", 1);
    let thread = core.threads()[0];
    let here = core.project_of(thread).expect("a bound Thread");
    let elsewhere = core
        .register_project(&std::env::temp_dir().canonicalize().unwrap())
        .unwrap();
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-t", NewThread, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.open_draft_in_project(elsewhere.project(), cx));
    tick(cx);
    let project = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.panes[view.focused(cx)]
                .draft()
                .expect("the draft is focused")
                .binding
                .project()
        })
    };

    cx.simulate_keystrokes("cmd-t");
    assert_eq!(
        project(&view, cx),
        here.project(),
        "pressed on the draft, it follows the Thread focus last left"
    );

    view.update(cx, |view, cx| {
        view.open_draft_in_project(elsewhere.project(), cx);
        view.cockpit.update(cx, |cockpit, _| cockpit.focus(PaneIdentity::Thread(thread)));
    });
    tick(cx);
    cx.simulate_keystrokes("cmd-t");
    assert_eq!(
        project(&view, cx),
        here.project(),
        "pressed on the Thread, it follows"
    );
}

/// What the Composer holds after `hello`, ctrl-a and `x`, with
/// `platform`'s whole key table bound as launch binds it.
fn typed_over_ctrl_a(
    platform: ferrite_ui::keymap::Platform,
    name: &str,
    cx: &mut TestAppContext,
) -> String {
    let (core, _fake) = cockpit(name, 1);
    cx.update(|cx| {
        ferrite_pane::pane::init_components(cx);
        let bindings = ferrite_frame::keys::load(ferrite_ui::keymap::bindings(platform), cx);
        cx.bind_keys(bindings);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    cx.simulate_input("hello");
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("x");
    tick(cx);
    composer_text(&view, cx)
}

/// Windows and Linux: ctrl-a in a text field selects all of it, so what
/// is typed next replaces it.
#[gpui::test]
fn ctrl_a_selects_the_whole_line_off_macos(cx: &mut TestAppContext) {
    let text = typed_over_ctrl_a(ferrite_ui::keymap::Platform::Windows, "ctrl-a-windows", cx);
    assert_eq!(text, "x");
}

/// macOS: ctrl-a is the Emacs line start (⌘A selects all), so what is
/// typed next goes in front.
#[gpui::test]
fn ctrl_a_goes_to_the_line_start_on_macos(cx: &mut TestAppContext) {
    let text = typed_over_ctrl_a(ferrite_ui::keymap::Platform::Mac, "ctrl-a-mac", cx);
    assert_eq!(text, "xhello");
}
