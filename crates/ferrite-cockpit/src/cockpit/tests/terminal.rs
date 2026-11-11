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
    tick(cx);
    assert!(cx.debug_bounds("terminal-pane").is_none());
    assert!(numbers(&view, thread, cx).is_empty(), "its shell ended with it");
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
        view.context_menu
            .as_ref()
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

#[gpui::test]
fn a_selection_offers_attach_and_attaches_as_a_reply_tile(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("terminal-attach", cx);
    opened(&view, cx);
    run(&view, thread, b"echo attach-me\r", cx);
    let shell = view.read_with(cx, |view, _| view.terminal_view(thread)).unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
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
            view.context_menu.as_ref().map(|menu| {
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
        view.context_menu = None;
        cx.notify();
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
        view.context_menu
            .as_ref()
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
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
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
