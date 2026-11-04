//! The title-bar tools (`cockpit/tools.rs`): each door opens its surface in
//! the right column for the focused Thread, a door pressed again puts it
//! away, and the column makes room for the dock beside the rail.
use super::*;
use crate::cockpit::tools::Tool;
use ferrite_ui::theme::{PANEL_W, TERMINAL_W};

fn solo<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let (core, _fake) = cockpit(name, 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let dir = scratch(&format!("{name}-settings"));
    std::fs::create_dir_all(&dir).unwrap();
    view.update(cx, |view, _| view.prefs.dir = dir);
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    (view, cx)
}

fn press(view: &Entity<CockpitView>, tool: Tool, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| view.press_tool(tool, cx));
    tick(cx);
}

fn pressed(view: &Entity<CockpitView>, tool: Tool, cx: &mut gpui::VisualTestContext) -> bool {
    view.read_with(cx, |view, _| view.tool_pressed(tool))
}

#[gpui::test]
fn a_door_opens_its_tool_and_pressed_again_puts_it_away(cx: &mut TestAppContext) {
    let (view, cx) = solo("tools-doors", cx);
    for tool in [Tool::Files, Tool::Changes, Tool::Terminal] {
        assert!(!pressed(&view, tool, cx));
        press(&view, tool, cx);
        assert!(pressed(&view, tool, cx), "{tool:?} wears the pressed ground");
        press(&view, tool, cx);
        assert!(!pressed(&view, tool, cx), "{tool:?} put away");
    }
    assert!(cx.debug_bounds("dock").is_none(), "nothing open, no dock");
}

#[gpui::test]
fn the_chords_open_changes_and_files(cx: &mut TestAppContext) {
    let (view, cx) = solo("tools-chords", cx);
    cx.simulate_keystrokes("cmd-shift-d");
    tick(cx);
    assert!(pressed(&view, Tool::Changes, cx), "⇧⌘D");
    cx.simulate_keystrokes("cmd-shift-f");
    tick(cx);
    assert!(pressed(&view, Tool::Files, cx), "⇧⌘F");
    cx.simulate_keystrokes("cmd-j");
    tick(cx);
    assert!(pressed(&view, Tool::Terminal, cx), "⌘J");
}

#[gpui::test]
fn the_dock_sits_beside_the_rail_and_the_terminal_has_its_own_column(cx: &mut TestAppContext) {
    let (view, cx) = solo("tools-columns", cx);
    let rail = cx.debug_bounds("panel").expect("the rail").size.width;
    assert_eq!(rail, px(PANEL_W));
    press(&view, Tool::Files, cx);
    let wide = cx.debug_bounds("panel").unwrap().size.width;
    assert!(wide > rail, "the column grew for the dock");
    assert!(cx.debug_bounds("dock-files").is_some());
    press(&view, Tool::Terminal, cx);
    let terminal = cx.debug_bounds("dock-terminal").expect("its own column");
    assert_eq!(terminal.size.width, px(TERMINAL_W));
    assert!(cx.debug_bounds("panel-sections").is_some(), "the rail stays");
    press(&view, Tool::Files, cx);
    assert!(cx.debug_bounds("dock-files").is_none());
    assert!(cx.debug_bounds("dock-terminal").is_some());
}

#[gpui::test]
fn the_dock_shows_with_the_rail_hidden(cx: &mut TestAppContext) {
    let (view, cx) = solo("tools-alone", cx);
    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none());
    press(&view, Tool::Changes, cx);
    assert!(cx.debug_bounds("panel").is_some(), "the dock brings the column");
    assert!(cx.debug_bounds("panel-sections").is_none(), "without the rail");
}

/// Opening the dock shrinks the thread, as Claude's does (the thread there
/// 666 of 1666), but never under its least.
#[gpui::test]
fn the_dock_narrows_the_thread_to_its_least_at_most(cx: &mut TestAppContext) {
    let (view, cx) = solo("tools-thread-narrows", cx);
    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    tick(cx);
    let width = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        let rects = cx.update(|window, cx| view.read(cx).pane_rects(window));
        rects[0].1.w
    };
    let before = width(&view, cx);
    press(&view, Tool::Files, cx);
    let after = width(&view, cx);
    assert!(after < before, "{after} narrower than {before}");
    assert!(after >= ferrite_ui::theme::DOCK_THREAD_MIN_W - 1., "{after}");
    let dock = cx.debug_bounds("panel").unwrap().size.width;
    assert!(f32::from(dock) > 0.5 * after, "the dock gets the room: {dock:?}");
}

/// The shell follows its grid: widening the column resizes the PTY.
#[gpui::test]
fn the_shell_is_resized_with_its_column(cx: &mut TestAppContext) {
    let (view, cx) = solo("tools-terminal-resize", cx);
    press(&view, Tool::Terminal, cx);
    let shell = view.read_with(cx, |view, _| view.terminal_view(view.focused_thread().unwrap())).unwrap();
    let narrow = shell.read_with(cx, |shell, _| shell.grid_size().0);
    press(&view, Tool::Files, cx);
    view.update(cx, |view, cx| {
        view.panel.expanded = true;
        cx.notify();
    });
    tick(cx);
    let wide = shell.read_with(cx, |shell, _| shell.grid_size().0);
    assert_eq!(wide, narrow, "the terminal's own column keeps its width");
    view.update(cx, |view, cx| view.press_tool(Tool::Terminal, cx));
    tick(cx);
}

/// The doors stand where they are at the window's top right (Claude's):
/// opening the rail, Files or the terminal moves none of them.
#[gpui::test]
fn the_title_tools_do_not_move_when_a_column_opens(cx: &mut TestAppContext) {
    let (view, cx) = solo("tools-pinned", cx);
    let at_rest = cx.debug_bounds("title-tools").expect("the tools");
    for tool in [Tool::Rail, Tool::Files, Tool::Terminal, Tool::Changes] {
        let before = pressed(&view, tool, cx);
        press(&view, tool, cx);
        assert_ne!(pressed(&view, tool, cx), before, "{tool:?} toggled");
        assert_eq!(cx.debug_bounds("title-tools"), Some(at_rest), "{tool:?} left them where they were");
    }
    // Everything open at once: still where they were.
    assert_eq!(cx.debug_bounds("title-tools"), Some(at_rest));
}
