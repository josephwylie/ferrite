//! The focused control owns its keys. Every key here goes through the
//! production binding table and a rendered, focused control, so a binding
//! that takes a key from its owner fails the test.
use super::*;
use crate::cockpit::surfaces::SurfaceId;
use ferrite_pane::visual::web::fake::{Call, FakeEngine};
use ferrite_pane::visual::web::Input;

const SENTINEL: &str = "an unsent draft";

fn sentinel_draft(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| composer.set(SENTINEL.into(), cx));
    });
    tick(cx);
}

fn composer_unchanged(view: &Entity<CockpitView>, fake: &Fake, cx: &mut gpui::VisualTestContext) {
    assert_eq!(composer_text(view, cx), SENTINEL, "the draft is untouched");
    assert!(fake.sent.borrow().is_empty(), "nothing was sent to the provider");
}

fn answered(fake: &Fake) -> Vec<(String, DecisionAnswer)> {
    fake.answered.borrow().clone()
}

#[gpui::test]
fn the_launcher_opens_its_highlighted_tool_on_enter(cx: &mut TestAppContext) {
    let (core, fake, _checkout) = bound_cockpit("key-launcher", Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| view.close_surface(view.cockpit.read(cx).thread_key(thread), &SurfaceId::Overview, cx));
    tick(cx);
    sentinel_draft(&view, cx);
    let head = cx.debug_bounds("panel-empty-head").expect("the launcher shows");
    cx.simulate_click(head.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_keystrokes("down enter");
    tick(cx);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Terminal(1)));
    composer_unchanged(&view, &fake, cx);
}

#[gpui::test]
fn approval_words_type_into_the_composer_and_answer_nothing(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("key-approval", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0].send(decision("perm_words")).unwrap();
    tick(cx);
    let pending = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.cockpit.read(cx).thread(thread).and_then(|open| open.pending()).is_some()
        })
    };
    assert!(pending(&view, cx), "the approval is up");
    for word in ["also", "no", "yes"] {
        view.update(cx, |view, cx| {
            view.panes[0].composer.update(cx, |composer, cx| composer.set(String::new(), cx));
        });
        for letter in word.chars() {
            cx.simulate_keystrokes(&letter.to_string());
        }
        tick(cx);
        assert_eq!(composer_text(&view, cx), word, "{word:?} is typed text");
        assert!(pending(&view, cx), "{word:?} answered nothing");
    }
    assert!(fake.sent.borrow().is_empty());
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| composer.set("with a note".into(), cx));
    });
    cx.simulate_keystrokes("cmd-enter");
    cx.run_until_parked();
    assert!(!pending(&view, cx), "the explicit chord still answers");
}

#[gpui::test]
fn the_deny_chord_denies_an_approval_with_text_on_the_line(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("key-deny-chord", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0].send(decision("perm_deny")).unwrap();
    tick(cx);
    cx.simulate_input("a note");
    tick(cx);
    assert!(answered(&fake).is_empty());
    cx.simulate_keystrokes("cmd-.");
    cx.run_until_parked();
    let pending = view.read_with(cx, |view, cx| {
        view.cockpit.read(cx).thread(thread).and_then(|open| open.pending()).is_some()
    });
    assert!(!pending, "the chord answered the approval");
    assert_eq!(answered(&fake).len(), 1);
    assert!(
        matches!(answered(&fake)[0].1, DecisionAnswer::Deny { .. }),
        "and the answer was a denial"
    );
}

/// An open menu's keys are the menu's, even over an approval whose empty
/// line binds the same arrows and ⎋, and the Composer has the keyboard
/// back once it closes.
#[gpui::test]
fn an_open_menu_keeps_its_keys_from_a_pending_approval(cx: &mut TestAppContext) {
    use crate::cockpit::context_menu::{MenuCursor, MenuTarget};
    let (core, fake) = cockpit("key-menu-over-decision", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0].send(decision("perm_menu")).unwrap();
    tick(cx);
    let composer = view.read_with(cx, |view, cx| view.panes[0].composer.focus_handle(cx));
    cx.update(|window, cx| window.focus(&composer, cx));
    tick(cx);
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::Thread(keyed(view, thread, cx)), gpui::point(px(300.), px(200.)), cx)
    });
    tick(cx);
    cx.simulate_keystrokes("down");
    tick(cx);
    let cursor = view.read_with(cx, |view, _| view.context_menu().map(|open| open.cursor));
    assert_eq!(cursor, Some(MenuCursor::Row(0)), "down moved the menu's cursor");
    cx.update(|window, cx| window.focus(&composer, cx));
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::Pane(keyed(view, thread, cx)), gpui::point(px(500.), px(400.)), cx)
    });
    tick(cx);
    cx.simulate_keystrokes("down");
    tick(cx);
    let cursor = view.read_with(cx, |view, _| view.context_menu().map(|open| open.cursor));
    assert!(
        matches!(cursor, Some(MenuCursor::Row(_))),
        "a menu opened over a focused field keeps its keys too: {cursor:?}"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.context_menu().is_none()), "escape closed the menu");
    assert!(answered(&fake).is_empty(), "the approval heard none of it");
    assert!(cx.update(|window, _| composer.is_focused(window)), "the Composer has the keyboard back");
}

/// A menu opened from the nav list gives the list its keys and cursor back.
#[gpui::test]
fn a_menu_from_the_nav_list_gives_the_list_back_its_keys(cx: &mut TestAppContext) {
    use crate::cockpit::context_menu::MenuTarget;
    let (core, _fake) = cockpit("key-menu-from-nav", 2);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    view.update_in(cx, |view, window, cx| view.focus_nav(window, cx));
    tick(cx);
    let held = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            let nav = view.read(cx).nav_view.read(cx);
            (nav.holds_keys(window), nav.cursor.clone())
        })
    };
    let (before, cursor) = held(cx);
    assert!(before && cursor.is_some(), "the list holds the keys");
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::Thread(keyed(view, thread, cx)), gpui::point(px(200.), px(200.)), cx)
    });
    tick(cx);
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.context_menu().is_none()));
    assert_eq!(held(cx), (true, cursor), "the list has its keys and its cursor back");
}

#[gpui::test]
fn y_answers_a_composerless_subagent_tab_while_a_composer_types_it(cx: &mut TestAppContext) {
    use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, Subject};
    let (core, fake) = cockpit("key-subagent", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let agent = AgentKey::new(Provider::Claude, "root", "keys-child");
    let mut info = AgentInfo::new(agent.clone());
    info.parent = Some(Subject::Main);
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(ActivityEvent::Discovered(info)))
        .unwrap();
    fake.streams.borrow()[0].send(decision("main_req")).unwrap();
    let SessionEvent::DecisionRequested { decision } = decision("child_req") else {
        unreachable!()
    };
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(ActivityEvent::Decision {
            subject: Some(Subject::Subagent(agent.clone())),
            decision,
        }))
        .unwrap();
    tick(cx);

    cx.simulate_keystrokes("y e s");
    tick(cx);
    assert_eq!(composer_text(&view, cx), "yes", "the composer types it");
    assert!(answered(&fake).is_empty(), "typing answered nothing");

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_subject(thread, Subject::Subagent(agent.clone()), window, cx)
        })
    });
    tick(cx);
    cx.simulate_keystrokes("y");
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(), [(id, DecisionAnswer::Allow { .. })] if id == "child_req"),
        "{:?}",
        answered(&fake)
    );
}

#[gpui::test]
fn files_keeps_the_keys_typed_into_its_editor_and_filter(cx: &mut TestAppContext) {
    let (core, fake, checkout) = bound_cockpit("key-files", Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    sentinel_draft(&view, cx);

    let file = checkout.join("src/lib.rs");
    view.update(cx, |view, cx| view.open_in_files(thread, file.clone(), None, true, cx));
    tick(cx);
    let line = cx.debug_bounds("editor").expect("the editor shows");
    cx.simulate_click(line.origin + gpui::point(px(80.), px(10.)), gpui::Modifiers::none());
    tick(cx);
    for key in ["x", "y", "z"] {
        cx.simulate_keystrokes(key);
        fake.streams.borrow()[0].send(SessionEvent::TextDelta { text: "…".into() }).unwrap();
        tick(cx);
    }
    cx.simulate_keystrokes("cmd-s");
    tick(cx);
    assert!(
        std::fs::read_to_string(&file).unwrap().contains("xyz"),
        "every key stayed in the file"
    );
    composer_unchanged(&view, &fake, cx);

    view.update_in(cx, |view, window, cx| view.go_to_file(thread, window, cx));
    tick(cx);
    for key in ["l", "i", "b"] {
        cx.simulate_keystrokes(key);
        fake.streams.borrow()[0].send(SessionEvent::TextDelta { text: "…".into() }).unwrap();
        tick(cx);
    }
    assert_eq!(view.read_with(cx, |view, cx| view.files.read(cx).filter_text_for_test(view.cockpit.read(cx).thread_key(thread), cx)), "lib");
    composer_unchanged(&view, &fake, cx);
}

#[gpui::test]
fn the_browser_page_gets_the_keys_the_host_binds(cx: &mut TestAppContext) {
    let (core, _, _) = super::machines::browser_core("key-browser");
    bind_production_keys(cx);
    let engine = std::rc::Rc::new(FakeEngine::new());
    cx.update(|cx| ferrite_pane::visual::engine::Visuals::install(engine.clone(), cx));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    until(cx, |cx| engine.live_views() == 1 && cx.debug_bounds("browser-preview").is_some());
    sentinel_draft(&view, cx);
    let page = cx.debug_bounds("browser-preview").unwrap();
    cx.simulate_click(page.center(), gpui::Modifiers::none());
    tick(cx);
    let key_downs = || {
        engine
            .journal()
            .iter()
            .filter(|call| matches!(call, Call::Input(Input::Key { down: true, .. })))
            .count()
    };
    for key in ["enter", "tab", "shift-tab", "escape", "cmd-v", "cmd-c"] {
        let before = key_downs();
        cx.simulate_keystrokes(key);
        tick(cx);
        assert_eq!(key_downs(), before + 1, "{key} reached the page");
    }
    assert_eq!(composer_text(&view, cx), SENTINEL, "the draft is untouched");
}

#[gpui::test]
fn a_subagent_questions_own_answer_types_its_letters(cx: &mut TestAppContext) {
    use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, Subject};
    let (core, fake) = cockpit("key-subagent-answer", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(900.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let agent = AgentKey::new(Provider::Claude, "root", "asking-child");
    let mut info = AgentInfo::new(agent.clone());
    info.parent = Some(Subject::Main);
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(ActivityEvent::Discovered(info)))
        .unwrap();
    let SessionEvent::DecisionRequested { decision } = question("child_q") else {
        unreachable!()
    };
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(ActivityEvent::Decision {
            subject: Some(Subject::Subagent(agent.clone())),
            decision,
        }))
        .unwrap();
    tick(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.select_subject(thread, Subject::Subagent(agent.clone()), window, cx)
        })
    });
    tick(cx);

    cx.simulate_keystrokes("3");
    tick(cx);
    cx.simulate_keystrokes("n a y");
    tick(cx);
    assert!(answered(&fake).is_empty(), "typing answered nothing: {:?}", answered(&fake));
    let serial = view.read_with(cx, |view, cx| {
        view.cockpit.read(cx).thread(thread).unwrap().pending_decisions()[0].handle.serial
    });
    let send = bounds(cx, format!("request-submit-{}-{serial}", thread.get()));
    cx.simulate_click(send.center(), gpui::Modifiers::none());
    tick(cx);
    let answered = answered(&fake);
    let [(id, DecisionAnswer::Questions { answers })] = answered.as_slice() else {
        panic!("the question was answered once: {answered:?}")
    };
    assert_eq!(id, "child_q");
    assert_eq!(answers[0].other.as_deref(), Some("nay"));
}

#[gpui::test]
fn a_question_mark_typed_on_the_panel_is_text_for_the_composer(cx: &mut TestAppContext) {
    let (core, _fake, _checkout) = bound_cockpit("key-question-panel", Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| view.upsert_surface(view.cockpit.read(cx).thread_key(thread), SurfaceId::Files, cx));
    tick(cx);
    cx.simulate_keystrokes("f6");
    tick(cx);
    assert!(
        super::panel_focus::focused_control(&view, cx).is_some(),
        "the keyboard is on the panel"
    );
    cx.simulate_keystrokes("?");
    cx.simulate_input("why");
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.floats.shortcuts.is_none()),
        "no sheet took the keyboard"
    );
    assert_eq!(composer_text(&view, cx), "?why");
}

/// A settled turn that ran one tool, so the transcript draws its shut
/// `Worked for` fold, the Composer empty and holding the keyboard.
fn settled_tool_turn(cx: &mut TestAppContext) -> (Entity<CockpitView>, &mut gpui::VisualTestContext) {
    let (mut core, fake) = cockpit("key-tool-fold", 1);
    let thread = core.threads()[0];
    let _ = core.send(thread, "prior prompt".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let stream = fake.streams.borrow();
    stream[0]
        .send(SessionEvent::ToolStarted {
            id: "toolu_9".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "echo hi" }),
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::ToolCompleted {
            id: "toolu_9".into(),
            output: "hi".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    drop(stream);
    tick(cx);
    assert!(cx.debug_bounds("turn-fold").is_some(), "the turn's fold is drawn");
    assert_eq!(composer_text(&view, cx), "");
    (view, cx)
}

fn fold_lit(cx: &mut gpui::VisualTestContext) -> bool {
    cx.debug_bounds("tool-disclosure-keyboard-target").is_some()
}

#[gpui::test]
fn escape_on_a_tabbed_fold_gives_the_composer_back_its_typing(cx: &mut TestAppContext) {
    let (view, cx) = settled_tool_turn(cx);
    cx.simulate_keystrokes("tab");
    tick(cx);
    assert!(fold_lit(cx), "Tab lights the fold");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!fold_lit(cx), "Escape puts the fold's light out");
    cx.simulate_input("after");
    tick(cx);
    assert_eq!(composer_text(&view, cx), "after");
}

#[gpui::test]
fn letters_typed_on_a_tabbed_fold_go_to_the_composer(cx: &mut TestAppContext) {
    let (view, cx) = settled_tool_turn(cx);
    cx.simulate_keystrokes("tab");
    tick(cx);
    assert!(fold_lit(cx), "Tab lights the fold");
    cx.simulate_keystrokes("space");
    tick(cx);
    assert!(cx.debug_bounds("turn-fold-open").is_some(), "Space opens the fold");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(cx.debug_bounds("turn-fold-shut").is_some(), "Enter shuts it");
    assert_eq!(composer_text(&view, cx), "", "neither key typed");
    cx.simulate_input("typed");
    tick(cx);
    assert_eq!(composer_text(&view, cx), "typed");
    assert!(!fold_lit(cx), "the keyboard left the fold");
}

#[gpui::test]
fn letters_and_escape_on_a_tabbed_copy_go_to_the_composer(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("key-code-copy", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "```rust\n    first();\n```".into(),
        })
        .unwrap();
    spoken(&fake, 0);
    tick(cx);
    let on_copy = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, _| ferrite_pane::rich::controls_focused(window))
    };
    cx.simulate_keystrokes("shift-tab");
    tick(cx);
    assert!(on_copy(cx), "Shift-Tab reaches the code block's Copy");
    cx.simulate_input("a");
    tick(cx);
    assert!(!on_copy(cx), "typing left Copy");
    cx.simulate_keystrokes("shift-tab");
    tick(cx);
    assert!(on_copy(cx), "Shift-Tab reaches Copy again");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!on_copy(cx), "Escape left Copy");
    cx.simulate_input("b");
    tick(cx);
    assert_eq!(composer_text(&view, cx), "ab");
}
