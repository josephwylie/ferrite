//! The Terminal Surface (`cockpit/tools/terminal.rs`): a shell per Panel
//! tab in the Thread's folder, its menus, and `Attach as context`.
use super::*;
use crate::cockpit::surfaces::SurfaceId;

fn window<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, ThreadId, &'a mut gpui::VisualTestContext) {
    let (core, _fake, _checkout) = bound_cockpit(name, Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, thread, cx)
}

/// How long a real shell gets to answer. A loaded Windows runner has taken
/// longer than 10 seconds to start one.
const SHELL_LOST_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

fn opened(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| view.new_terminal_surface(thread, cx));
    tick(cx);
}

fn numbers(view: &Entity<CockpitView>, thread: ThreadId, cx: &mut gpui::VisualTestContext) -> Vec<u64> {
    view.read_with(cx, |view, _| view.terminal_numbers(thread))
}

/// Run `command` in the shown shell and wait for `needle` to be on screen.
fn run(view: &Entity<CockpitView>, thread: ThreadId, command: &'static [u8], cx: &mut gpui::VisualTestContext) {
    let shell = view.read_with(cx, |view, _| view.terminal_view(thread)).unwrap();
    shell.update(cx, |shell, _| shell.write(command));
}

#[gpui::test]
fn the_door_opens_a_shell_in_a_panel_tab_and_closing_it_ends_it(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("terminal-door", cx);
    opened(&view, cx);
    assert!(cx.debug_bounds("terminal-pane").is_some());
    assert!(cx.debug_bounds("terminal").is_some(), "the grid");
    assert!(cx.debug_bounds("panel-tab-Terminal-1").is_some(), "its tab");
    assert_eq!(numbers(&view, thread, cx), [1]);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Terminal(1)));
    view.update(cx, |view, cx| view.close_surface(thread, &SurfaceId::Terminal(1), cx));
    leap(cx, std::time::Duration::from_secs(1));
    assert!(cx.debug_bounds("terminal-pane").is_none());
    assert!(numbers(&view, thread, cx).is_empty(), "its shell ended with it");
}

#[gpui::test]
fn a_local_shell_with_a_failed_exit_still_closes_its_tab(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("terminal-local-exit", cx);
    opened(&view, cx);
    run(&view, thread, b"exit 7\r", cx);
    let until = std::time::Instant::now() + SHELL_LOST_AFTER;
    while !numbers(&view, thread, cx).is_empty() {
        assert!(std::time::Instant::now() < until, "the local shell's tab never closed");
        std::thread::sleep(std::time::Duration::from_millis(20));
        tick(cx);
    }
    assert!(cx.debug_bounds("terminal-failed").is_none());
}

/// ⌘W with the keyboard in the shell closes that tab, as it does from the
/// Panel's controls, and never parks the Thread.
#[gpui::test]
fn cmd_w_in_the_shell_closes_its_tab_not_the_thread(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("terminal-cmd-w", cx);
    opened(&view, cx);
    let shell = view.read_with(cx, |view, _| view.terminal_view(thread)).unwrap();
    cx.update(|window, cx| window.focus(&shell.focus_handle(cx), cx));
    tick(cx);
    assert!(
        cx.update(|window, cx| view.read(cx).terminal_holds_focus(window, cx)),
        "the shell has the keyboard"
    );
    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    assert!(numbers(&view, thread, cx).is_empty(), "⌘W ended the shell's tab");
    assert!(
        view.read_with(cx, |view, _| view.cockpit.thread(thread).is_some() && view.panes.len() == 1),
        "the Thread stays open"
    );
}

/// A shell that cannot start (its folder is gone) leaves a tab that says so
/// and offers Try again, which starts it once the folder is back.
#[gpui::test]
fn a_shell_that_cannot_start_says_so_and_retries(cx: &mut TestAppContext) {
    let (core, _fake, checkout) = bound_cockpit("terminal-fails", Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let moved = checkout.with_extension("moved");
    std::fs::rename(&checkout, &moved).unwrap();
    opened(&view, cx);
    assert!(cx.debug_bounds("terminal-failed").is_some(), "the tab says it could not start");
    assert!(cx.debug_bounds("terminal").is_none(), "no grid");
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Terminal(1)));
    std::fs::rename(&moved, &checkout).unwrap();
    let at = cx.debug_bounds("terminal-retry").unwrap();
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("terminal-failed").is_none());
    assert!(cx.debug_bounds("terminal").is_some(), "the retry started the shell");
}

/// Each new terminal is its own tab and shell; ⌘J shows the last one, not
/// another.
#[gpui::test]
fn a_second_terminal_is_a_second_tab_and_cmd_j_shows_the_last(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("terminal-tabs", cx);
    opened(&view, cx);
    opened(&view, cx);
    assert_eq!(numbers(&view, thread, cx), [1, 2]);
    assert!(cx.debug_bounds("panel-tab-Terminal-2").is_some());
    view.update(cx, |view, cx| view.activate_surface(thread, &SurfaceId::Overview, cx));
    tick(cx);
    cx.simulate_keystrokes("cmd-j");
    tick(cx);
    assert_eq!(numbers(&view, thread, cx), [1, 2], "no third shell");
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Terminal(2)));
    view.update(cx, |view, cx| view.close_surface(thread, &SurfaceId::Terminal(2), cx));
    tick(cx);
    assert_eq!(numbers(&view, thread, cx), [1]);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Terminal(1)), "its neighbour");
}

#[gpui::test]
fn the_more_menu_holds_claudes_four_rows(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("terminal-more", cx);
    opened(&view, cx);
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::TerminalMore(thread), gpui::point(px(1700.), px(60.)), cx)
    });
    let rows = view.read_with(cx, |view, _| {
        view.context_menu()
            .unwrap()
            .rows
            .iter()
            .flatten()
            .map(|(item, _)| (item.label.to_string(), item.checked, item.detail.as_ref().map(|d| d.to_string())))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        rows,
        [
            ("Cursor style".into(), false, Some("Block".into())),
            ("Blink cursor".into(), true, None),
            ("Copy on select".into(), false, None),
            ("Scrollback".into(), false, Some("10,000 lines".into())),
        ]
    );
}

/// What the `⋯` menu sets is the window's: the menu shows it the next time
/// it opens, whichever shell it opens over.
#[gpui::test]
fn a_more_menu_choice_shows_when_the_menu_opens_again(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("terminal-more-choice", cx);
    opened(&view, cx);
    let verbs = [
        crate::cockpit::tools::TerminalVerb::ToggleBlink,
        crate::cockpit::tools::TerminalVerb::ToggleCopyOnSelect,
        crate::cockpit::tools::TerminalVerb::Cursor(ferrite_terminal::Cursor::Bar),
        crate::cockpit::tools::TerminalVerb::Scrollback(1_000),
    ];
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            for verb in verbs {
                view.run_menu_verb(MenuTarget::TerminalMore(thread), MenuVerb::Terminal(verb), window, cx);
            }
        })
    });
    opened(&view, cx);
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::TerminalMore(thread), gpui::point(px(1700.), px(60.)), cx)
    });
    let rows = view.read_with(cx, |view, _| {
        view.context_menu()
            .unwrap()
            .rows
            .iter()
            .flatten()
            .map(|(item, _)| (item.label.to_string(), item.checked, item.detail.as_ref().map(|d| d.to_string())))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        rows,
        [
            ("Cursor style".into(), false, Some("Bar".into())),
            ("Blink cursor".into(), false, None),
            ("Copy on select".into(), true, None),
            ("Scrollback".into(), false, Some("1,000 lines".into())),
        ]
    );
}

/// ⌘J back to a shell already open hands it the keyboard again.
#[gpui::test]
fn cmd_j_back_to_an_open_shell_gives_it_the_keyboard(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("terminal-cmd-j-focus", cx);
    opened(&view, cx);
    let shell = view.read_with(cx, |view, _| view.terminal_view(thread)).unwrap();
    view.update(cx, |view, cx| view.activate_surface(thread, &SurfaceId::Overview, cx));
    let composer = view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).focus_handle(cx));
    cx.update(|window, cx| window.focus(&composer, cx));
    tick(cx);
    let shell_focused = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| shell.focus_handle(cx).is_focused(window))
    };
    assert!(!shell_focused(cx), "the Composer has the keyboard");
    cx.simulate_keystrokes("cmd-j");
    tick(cx);
    tick(cx);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Terminal(1)));
    assert!(shell_focused(cx), "the shell took the keyboard");
}

#[gpui::test]
fn a_selection_offers_attach_and_attaches_as_a_reply_tile(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("terminal-attach", cx);
    opened(&view, cx);
    run(&view, thread, b"echo attach-me\r", cx);
    let shell = view.read_with(cx, |view, _| view.terminal_view(thread)).unwrap();
    let until = std::time::Instant::now() + SHELL_LOST_AFTER;
    loop {
        tick(cx);
        shell.update(cx, |shell, cx| shell.select_all(cx));
        if shell.read_with(cx, |shell, _| shell.selection().is_some_and(|text| text.contains("\nattach-me"))) {
            break;
        }
        assert!(std::time::Instant::now() < until, "the shell never answered");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // The shell may still be drawing its prompt: select again until the
    // pill, which follows the selection a frame behind, shows.
    for _ in 0..50 {
        tick(cx);
        if cx.debug_bounds("terminal-attach").is_some() {
            break;
        }
        shell.update(cx, |shell, cx| shell.select_all(cx));
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(cx.debug_bounds("terminal-attach").is_some(), "the pill hangs over the selection");
    // The right-click menu: Attach first, then Copy, Paste, Select all, Clear.
    let grid = cx.debug_bounds("terminal").unwrap();
    cx.simulate_mouse_down(grid.center(), gpui::MouseButton::Right, gpui::Modifiers::none());
    tick(cx);
    let names = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.context_menu().map(|menu| {
                menu.rows.iter().flatten().map(|(item, _)| (item.label.to_string(), item.disabled)).collect::<Vec<_>>()
            })
        })
    };
    assert_eq!(
        names(&view, cx).unwrap(),
        [
            ("Attach selection as context".to_string(), false),
            ("Copy".into(), false),
            ("Paste".into(), false),
            ("Select all".into(), false),
            ("Clear terminal".into(), false),
        ]
    );
    view.update(cx, |view, cx| {
        view.close_popup(cx);
    });
    tick(cx);
    let pill = cx.debug_bounds("terminal-attach").unwrap();
    cx.simulate_click(pill.center(), gpui::Modifiers::none());
    tick(cx);
    let replies = view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).replies().len());
    assert_eq!(replies, 1, "a Reply tile in the tray");
    assert!(shell.read_with(cx, |shell, _| shell.selection().is_none()), "the selection lets go");
}

#[gpui::test]
fn with_no_selection_the_menu_has_no_attach_row_and_copy_is_greyed(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("terminal-menu-empty", cx);
    opened(&view, cx);
    let grid = cx.debug_bounds("terminal").unwrap();
    cx.simulate_mouse_down(grid.center(), gpui::MouseButton::Right, gpui::Modifiers::none());
    tick(cx);
    let _ = thread;
    let rows = view.read_with(cx, |view, _| {
        view.context_menu()
            .unwrap()
            .rows
            .iter()
            .flatten()
            .map(|(item, _)| (item.label.to_string(), item.disabled))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        rows,
        [
            ("Copy".to_string(), true),
            ("Paste".into(), false),
            ("Select all".into(), false),
            ("Clear terminal".into(), false),
        ]
    );
}

/// Keys typed in the window reach the shell through the window's own
/// dispatch (the production keymap, the focus the door and a click give),
/// not by calling the terminal: plain letters, Backspace, Enter, and ⌃C.
fn typed_reaches(
    shell: &Entity<ferrite_terminal::TerminalView>,
    needle: &str,
    cx: &mut gpui::VisualTestContext,
) -> bool {
    let until = std::time::Instant::now() + SHELL_LOST_AFTER;
    while std::time::Instant::now() < until {
        tick(cx);
        shell.update(cx, |shell, cx| shell.select_all(cx));
        if shell.read_with(cx, |shell, _| shell.selection().is_some_and(|text| text.contains(needle))) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

/// Whether `needle` is on the shell's screen now, without waiting for it.
fn typed_reaches_in(
    shell: &Entity<ferrite_terminal::TerminalView>,
    needle: &str,
    cx: &mut gpui::VisualTestContext,
) -> bool {
    std::thread::sleep(std::time::Duration::from_millis(300));
    shell.update(cx, |shell, cx| shell.select_all(cx));
    shell.read_with(cx, |shell, _| shell.selection().is_some_and(|text| text.contains(needle)))
}

#[gpui::test]
fn keys_typed_after_the_door_opens_reach_the_shell(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("terminal-keys", cx);
    opened(&view, cx);
    let shell = view.read_with(cx, |view, _| view.terminal_view(thread)).unwrap();
    tick(cx);
    // The `x` is deleted by Backspace; Enter runs the line.
    cx.simulate_keystrokes("e c h o space k e y 4 x backspace 2 enter");
    assert!(typed_reaches(&shell, "\nkey42", cx), "the typed line ran");
    // ⌃C leaves a half-typed line unrun.
    cx.simulate_keystrokes("n o p e ctrl-c");
    cx.simulate_keystrokes("e c h o space d o n e 7 enter");
    assert!(typed_reaches(&shell, "\ndone7", cx), "the shell took more after ⌃C");
    // ⌘V pastes into the shell, not the Composer; ← then a letter edits the line.
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("pasted9".into())));
    cx.simulate_keystrokes("e c h o space cmd-v enter");
    assert!(typed_reaches(&shell, "\npasted9", cx), "⌘V reached the shell");
    cx.simulate_keystrokes("e c h o space o k 1 x x left backspace enter");
    assert!(typed_reaches(&shell, "\nok1x\n", cx), "arrows and Backspace edit the line");
    assert!(
        view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).is_empty()),
        "none of it went to the Composer"
    );
}

#[gpui::test]
fn a_click_in_the_shell_takes_the_keyboard_back_from_the_composer(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("terminal-click-focus", cx);
    opened(&view, cx);
    let shell = view.read_with(cx, |view, _| view.terminal_view(thread)).unwrap();
    // The Composer takes the keyboard.
    let composer = view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).focus_handle(cx));
    cx.update(|window, cx| window.focus(&composer, cx));
    tick(cx);
    let grid = cx.debug_bounds("terminal").unwrap();
    cx.simulate_click(grid.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_keystrokes("e c h o space c l i c k 5 enter");
    assert!(typed_reaches(&shell, "\nclick5", cx), "the click gave the shell the keyboard");
}

fn frame_after(ms: u64, cx: &mut gpui::VisualTestContext) {
    cx.executor().advance_clock(std::time::Duration::from_millis(ms));
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
}

/// A panel sliding shut over a shell that has the keyboard takes it back:
/// what is typed mid-slide never reaches the shell, and ⌥⌘B opens the panel
/// again at once, with no click first.
#[gpui::test]
fn a_panel_sliding_shut_gives_the_shell_no_keys(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    let (view, thread, cx) = window("terminal-closing", cx);
    opened(&view, cx);
    let shell = view.read_with(cx, |view, _| view.terminal_view(thread)).unwrap();
    tick(cx);
    cx.simulate_keystrokes("e c h o space b e f o r e 1 enter");
    assert!(typed_reaches(&shell, "\nbefore1", cx), "the shell has the keyboard");

    cx.simulate_keystrokes("cmd-alt-b");
    frame_after(20, cx);
    frame_after(20, cx);
    let width = cx.debug_bounds("panel").expect("still sliding").size.width;
    assert!(width > px(0.) && width < px(ferrite_ui::theme::TERMINAL_W), "mid-slide: {width:?}");
    cx.simulate_keystrokes("e c h o space m i d 2 enter");
    frame_after(400, cx);
    frame_after(20, cx);
    assert!(cx.debug_bounds("panel").is_none(), "the panel has gone");
    assert!(
        !typed_reaches_in(&shell, "mid2", cx),
        "nothing typed mid-slide reached the shell"
    );

    cx.simulate_keystrokes("cmd-alt-b");
    frame_after(400, cx);
    assert!(cx.debug_bounds("panel").is_some(), "⌥⌘B reopens it without a click");
}

/// An open the Server refused (here: an in-process window has no browser)
/// is never echoed, so it leaves nothing behind that could later turn away
/// an agent's open as stale.
#[gpui::test]
fn an_agents_open_after_a_refused_one_still_shows_the_browser(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    let (core, _fake, _checkout) = bound_cockpit("terminal-refused-open", Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    opened(&view, cx);
    view.update(cx, |view, cx| {
        view.browser_action(
            thread,
            ferrite_core::browser::Action::Open { url: "http://user:pw@localhost:5173/".into() },
            cx,
        )
    });
    settle(cx);
    back_to_shell(cx);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Terminal(1)));

    hear_own_open_echoed(&view, thread, cx);
    assert_eq!(
        panel_tabs(&view, thread, cx).1,
        Some(SurfaceId::Browser),
        "an agent's open after the operator's last click shows the browser"
    );
}

fn cmd_click(cx: &mut gpui::VisualTestContext, row: usize, column: usize) {
    let at = cell(cx, row, column);
    cx.simulate_click(at, gpui::Modifiers::secondary_key());
    settle(cx);
}

/// One click on the shell's own tab, as the operator would make it.
fn back_to_shell(cx: &mut gpui::VisualTestContext) {
    settle(cx);
    let at = cx.debug_bounds("panel-tab-Terminal-1").expect("a tab").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    settle(cx);
}

/// What a Server linked to this window sends back for the open it was asked:
/// the same action, read a frame later than the operator's next click.
fn hear_own_open_echoed(view: &gpui::Entity<CockpitView>, thread: ThreadId, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, _| {
        view.cockpit.fixture_browser_message(ferrite_core::protocol::ServerMessage::Browser {
            thread,
            owner: None,
            state: Default::default(),
            action: Some(ferrite_core::browser::Action::Open { url: "http://localhost:5173/app".into() }),
            owned: false,
        })
    });
    settle(cx);
}

/// What the operator sees once the panel has stopped moving.
fn settle(cx: &mut gpui::VisualTestContext) {
    frame_after(400, cx);
    frame_after(20, cx);
}

/// The point on the grid's row `row`, `column` cells in, as the face lays
/// it out.
fn cell(cx: &mut gpui::VisualTestContext, row: usize, column: usize) -> gpui::Point<gpui::Pixels> {
    let grid = cx.debug_bounds("terminal").expect("the grid");
    gpui::point(
        grid.left() + px(ferrite_ui::theme::TERMINAL_PAD_X + (column as f32 + 0.5) * ferrite_ui::theme::FS_TERMINAL * ferrite_ui::theme::CODE_ADVANCE),
        grid.top() + px(ferrite_ui::theme::TERMINAL_PAD_Y + (row as f32 + 0.5) * ferrite_ui::theme::LH_TERMINAL),
    )
}

/// ⌘-click on what the shell printed: a local address and a web address
/// open Ferrite's browser, a relative path with a line opens the editor on
/// that line, resolved against the shell's folder. A plain click opens
/// nothing.
#[gpui::test]
fn cmd_click_opens_what_the_shell_printed(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    let (core, _fake, checkout) = bound_cockpit("terminal-links", Provider::Claude);
    std::fs::write(checkout.join("src/main.rs"), "one\ntwo\nthree\nfour\n").unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.cockpit.fixture_browser_accepts());
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    opened(&view, cx);
    // The platform shell: the login shell on Unix, PowerShell on Windows.
    #[cfg(unix)]
    let print = b"printf '\\033[2J\\033[Hhttp://localhost:5173/app\\nhttps://example.com/docs\\nsrc/main.rs:3\\n'\r";
    #[cfg(windows)]
    let print = b"Clear-Host; 'http://localhost:5173/app', 'https://example.com/docs', 'src/main.rs:3'\r";
    run(&view, thread, print, cx);
    let shell = view.read_with(cx, |view, _| view.terminal_view(thread)).unwrap();
    assert!(typed_reaches(&shell, "docs\nsrc/main.rs:3", cx), "the shell printed the three");
    let at = cell(cx, 0, 12);
    cx.simulate_click(at, gpui::Modifiers::none());
    tick(cx);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Terminal(1)), "a plain click opens nothing");

    cmd_click(cx, 0, 12);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Browser));

    back_to_shell(cx);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Terminal(1)), "one click returns to the shell");
    hear_own_open_echoed(&view, thread, cx);
    assert_eq!(
        panel_tabs(&view, thread, cx).1,
        Some(SurfaceId::Terminal(1)),
        "the Server's echo of the window's own open does not take the panel back"
    );
    cmd_click(cx, 1, 12);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Browser));

    back_to_shell(cx);
    cmd_click(cx, 2, 5);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Files));
    assert!(view.read_with(cx, |view, _| view.files_tab_names(thread)).contains(&("main.rs".to_string(), true)));
    let editor = view.read_with(cx, |view, _| view.files_editor_of(thread).expect("the editor"));
    let (text, caret) = editor.read_with(cx, |editor, _| (editor.text().to_string(), editor.caret_for_test()));
    assert_eq!(text[..caret].matches('\n').count(), 2, "the caret is on line 3");
}
