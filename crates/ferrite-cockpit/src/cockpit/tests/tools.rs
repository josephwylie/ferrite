//! The Panel's one tabbed host (ADR 0039, `cockpit/panel.rs`,
//! `panel/strip.rs`): the tool chords open their Surfaces through the one
//! door, the head row's tabs, `+` menu, maximize and edge, and "Open a
//! surface".
use super::*;
use crate::cockpit::surfaces::SurfaceId;
use ferrite_ui::theme::*;

fn solo<'a>(
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

fn upsert(view: &Entity<CockpitView>, thread: ThreadId, id: SurfaceId, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| view.upsert_surface(thread, id, cx));
    tick(cx);
}

#[gpui::test]
fn the_chords_open_changes_files_and_a_terminal_once(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("surfaces-chords", cx);
    opens_once(&view, thread, SurfaceId::Changes, cx, |_, cx| cx.simulate_keystrokes("cmd-shift-d"));
    assert!(cx.debug_bounds("changes").is_some(), "its body");
    opens_once(&view, thread, SurfaceId::Files, cx, |_, cx| cx.simulate_keystrokes("cmd-shift-f"));
    assert!(cx.debug_bounds("files").is_some());
    opens_once(&view, thread, SurfaceId::Terminal(1), cx, |_, cx| cx.simulate_keystrokes("cmd-j"));
    assert!(cx.debug_bounds("terminal-pane").is_some());
    let (tabs, _) = panel_tabs(&view, thread, cx);
    assert_eq!(
        tabs,
        [SurfaceId::Overview, SurfaceId::Changes, SurfaceId::Files, SurfaceId::Terminal(1)]
    );
}

#[gpui::test]
fn the_title_bar_holds_only_the_panel_toggle(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("surfaces-title-tools", cx);
    assert!(cx.debug_bounds("panel-toggle").is_some());
    for gone in ["title-split", "title-terminal", "title-changes", "title-files", "open-browser", "title-more"] {
        assert!(cx.debug_bounds(gone).is_none(), "{gone} is no title tool now");
    }
    let toggle = cx.debug_bounds("panel-toggle").unwrap();
    cx.simulate_click(toggle.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none(), "⌥⌘B's door hides it");
    assert_eq!(cx.debug_bounds("panel-toggle"), Some(toggle), "where it was");
    // Anything opened opens the Panel again.
    cx.simulate_keystrokes("cmd-shift-d");
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some());
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Changes));
}

/// T3's tab: 24 tall, never wider than 144, radius 6, 12px text; the shown
/// one on `SELECTED`, the rest in `INK_SUBTLE`.
#[gpui::test]
fn a_tab_reads_at_t3s_size(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("surfaces-tab-size", cx);
    upsert(&view, thread, SurfaceId::File("/tmp/a-file-with-a-very-long-name-for-the-strip.rs".into()), cx);
    let file = cx
        .debug_bounds("panel-tab-File-a-file-with-a-very-long-name-for-the-strip.rs")
        .expect("its tab");
    assert_eq!(file.size.height, px(SURFACE_TAB_H));
    assert_eq!(file.size.height, px(24.));
    assert!(file.size.width <= px(SURFACE_TAB_MAX_W) && SURFACE_TAB_MAX_W == 144.);
    assert_eq!((R_CONTROL, FS_BADGE), (6., 12.));
    let overview = cx.debug_bounds("panel-tab-Overview").unwrap();
    assert_eq!(overview.size.height, px(24.));
    let head = cx.debug_bounds("panel-head").unwrap();
    assert_eq!(head.size.height, px(WIN_CHROME_H), "the title row");
}

#[gpui::test]
fn hovering_a_tab_swaps_its_icon_for_its_x_and_middle_click_closes(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("surfaces-tab-close", cx);
    upsert(&view, thread, SurfaceId::Changes, cx);
    assert!(cx.debug_bounds("panel-tab-Changes-x").is_none(), "the icon at rest");
    let tab = cx.debug_bounds("panel-tab-Changes").unwrap();
    cx.simulate_mouse_move(tab.center(), None, gpui::Modifiers::none());
    tick(cx);
    let x = cx.debug_bounds("panel-tab-Changes-x").expect("× under the pointer");
    assert!(tab.contains(&x.center()));
    assert_eq!(cx.debug_bounds("panel-tab-Changes").unwrap().size, tab.size, "no width jump");
    cx.simulate_click(x.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(panel_tabs(&view, thread, cx).0, [SurfaceId::Overview], "× closed it");

    upsert(&view, thread, SurfaceId::Files, cx);
    let tab = cx.debug_bounds("panel-tab-Files").unwrap();
    cx.simulate_mouse_down(tab.center(), gpui::MouseButton::Middle, gpui::Modifiers::none());
    cx.simulate_mouse_up(tab.center(), gpui::MouseButton::Middle, gpui::Modifiers::none());
    tick(cx);
    assert_eq!(panel_tabs(&view, thread, cx).0, [SurfaceId::Overview], "middle click closed it");
}

#[gpui::test]
fn a_tab_dragged_past_six_pixels_moves_and_five_is_a_click(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("surfaces-tab-drag", cx);
    upsert(&view, thread, SurfaceId::Changes, cx);
    upsert(&view, thread, SurfaceId::Background, cx);
    let changes = cx.debug_bounds("panel-tab-Changes").unwrap().center();
    let overview = cx.debug_bounds("panel-tab-Overview").unwrap().center();
    // 5px and back onto itself: a click, which shows it.
    cx.simulate_mouse_down(changes, gpui::MouseButton::Left, gpui::Modifiers::none());
    let near = gpui::point(changes.x + px(5.), changes.y);
    cx.simulate_mouse_move(near, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(near, gpui::MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    let (tabs, active) = panel_tabs(&view, thread, cx);
    assert_eq!(tabs, [SurfaceId::Overview, SurfaceId::Changes, SurfaceId::Background]);
    assert_eq!(active, Some(SurfaceId::Changes));
    // Dragged onto Overview: it moves there.
    cx.simulate_mouse_down(changes, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(overview, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(overview, gpui::MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    let (tabs, active) = panel_tabs(&view, thread, cx);
    assert_eq!(tabs, [SurfaceId::Changes, SurfaceId::Overview, SurfaceId::Background]);
    assert_eq!(active, Some(SurfaceId::Changes), "the shown tab stays shown");
}

#[gpui::test]
fn the_plus_menu_lists_every_surface_with_its_letter(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("surfaces-plus-menu", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let plus = cx.debug_bounds("panel-plus").expect("the + after the tabs");
    cx.simulate_click(plus.center(), gpui::Modifiers::none());
    tick(cx);
    let rows = view.read_with(cx, |view, _| {
        view.context_menu
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .flatten()
            .map(|(item, _)| {
                (
                    item.label.to_string(),
                    item.shortcut.as_ref().map(|key| key.to_string()),
                    item.disabled,
                    item.why.is_some(),
                )
            })
            .collect::<Vec<_>>()
    });
    let s = |text: &str| text.to_string();
    // No PR on its branch and an agent that cannot fork: those two say why.
    assert_eq!(
        rows,
        [
            (s("Browser"), Some(s("B")), false, false),
            (s("Terminal"), Some(s("T")), false, false),
            (s("Files"), Some(s("F")), false, false),
            (s("Changes"), Some(s("D")), false, false),
            (s("Pull request"), Some(s("P")), true, true),
            (s("Side chat"), Some(s("S")), true, true),
            (s("Background"), Some(s("G")), false, false),
            (s("Overview"), Some(s("O")), false, false),
        ]
    );
    // An unavailable row's letter does nothing; an available one opens.
    cx.simulate_keystrokes("s");
    tick(cx);
    assert!(!panel_tabs(&view, thread, cx)
        .0
        .iter()
        .any(|tab| matches!(tab, SurfaceId::SideChat(_))));
    if view.read_with(cx, |view, _| view.context_menu.is_none()) {
        cx.simulate_click(plus.center(), gpui::Modifiers::none());
        tick(cx);
    }
    cx.simulate_keystrokes("g");
    tick(cx);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Background));
}

#[gpui::test]
fn an_empty_panel_says_open_a_surface_and_takes_its_keys(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("surfaces-empty", cx);
    view.update(cx, |view, cx| view.close_surface(thread, &SurfaceId::Overview, cx));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_some(), "the Panel stays on");
    assert!(cx.debug_bounds("panel-empty-head").is_some(), "Open a surface");
    let row = cx.debug_bounds("panel-open-B").expect("its rows");
    assert_eq!(row.size.height, px(SURFACE_EMPTY_ROW_H));
    assert_eq!(SURFACE_EMPTY_ROW_H, 32.);
    assert_eq!(KEY_CAP_SOFT, 20.);
    // ↓ from the top, then ↑ twice wraps to the foot (Overview); ↵ opens it.
    view.update(cx, |view, cx| {
        let down = gpui::Keystroke::parse("down").unwrap();
        let up = gpui::Keystroke::parse("up").unwrap();
        let enter = gpui::Keystroke::parse("enter").unwrap();
        for key in [&down, &up, &up] {
            view.surface_empty_key(thread, key, cx);
        }
        assert_eq!(view.panel.empty_highlight, 7, "wrapped to the last row");
        view.surface_empty_key(thread, &enter, cx);
    });
    tick(cx);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Overview));
    // A bare letter fires its row.
    view.update(cx, |view, cx| view.close_surface(thread, &SurfaceId::Overview, cx));
    view.update(cx, |view, cx| {
        view.surface_empty_key(thread, &gpui::Keystroke::parse("d").unwrap(), cx);
    });
    tick(cx);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Changes));
}

#[gpui::test]
fn maximize_covers_all_but_the_boards_least_and_restores(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("surfaces-maximize", cx);
    let rest = cx.debug_bounds("panel").unwrap().size.width;
    assert_eq!(rest, px(PANEL_W));
    let door = cx.debug_bounds("panel-maximize").expect("in the head row");
    cx.simulate_click(door.center(), gpui::Modifiers::none());
    tick(cx);
    let wide = cx.debug_bounds("panel").unwrap().size.width;
    assert_eq!(wide, px(1900. - NAV_WIDTH - DOCK_THREAD_MIN_W));
    assert!(view.read_with(cx, |view, _| view.panel.surfaces.get(thread).unwrap().maximized));
    let door = cx.debug_bounds("panel-restore").unwrap();
    cx.simulate_click(door.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(cx.debug_bounds("panel").unwrap().size.width, rest, "restored");
}

/// At a laptop's 1440, with the nav drawn as the rail, maximize takes all
/// but the board's least of the content area beside the rail, not beside
/// the full column the rail no longer draws.
#[gpui::test]
fn maximize_beside_the_rail_takes_the_rails_room(cx: &mut TestAppContext) {
    let (core, _fake, _checkout) = bound_cockpit("surfaces-maximize-rail", Provider::Claude);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.nav_collapsed = true);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    assert!(cx.debug_bounds("nav-rail").is_some(), "the nav draws as the rail");
    let rest = cx.debug_bounds("panel").unwrap().size.width;
    let door = cx.debug_bounds("panel-maximize").expect("in the head row");
    cx.simulate_click(door.center(), gpui::Modifiers::none());
    tick(cx);
    let wide = cx.debug_bounds("panel").unwrap().size.width;
    assert!(wide > rest, "maximize widens the Panel: {wide:?} vs {rest:?}");
    assert_eq!(wide, px(1440. - NAV_FOLDED_W - DOCK_THREAD_MIN_W));
    assert!(cx.debug_bounds("panel-maximize").is_none(), "the door now restores");
    assert!(cx.debug_bounds("panel-restore").is_some());
}

#[gpui::test]
fn the_left_edge_resizes_the_panel_never_under_its_floor(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("surfaces-resize", cx);
    let edge = cx.debug_bounds("panel-edge").expect("its left edge");
    let from = edge.center();
    let to = gpui::point(px(1900. - 600.), from.y);
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    let width = cx.debug_bounds("panel").unwrap().size.width;
    assert!((width - px(600.)).abs() < px(1.), "{width:?}");
    // Narrowed past the floor: the floor.
    let edge = cx.debug_bounds("panel-edge").unwrap().center();
    let to = gpui::point(px(1900. - 100.), edge.y);
    cx.simulate_mouse_down(edge, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    assert_eq!(cx.debug_bounds("panel").unwrap().size.width, px(PANEL_W));
    // A Terminal widens it to its own floor.
    cx.simulate_keystrokes("cmd-j");
    tick(cx);
    assert_eq!(cx.debug_bounds("panel").unwrap().size.width, px(TERMINAL_W));
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Terminal(1)));
}

/// The tabs, the shown one, maximized and the width come back with the
/// window.
#[gpui::test]
fn the_tabs_survive_a_restart(cx: &mut TestAppContext) {
    let (core, _fake, _checkout) = bound_cockpit("surfaces-restart", Provider::Claude);
    let thread = core.threads()[0];
    let dir = scratch("surfaces-restart-prefs");
    std::fs::create_dir_all(&dir).unwrap();
    let mut surfaces = crate::cockpit::surfaces::Surfaces::default();
    let state = surfaces.entry(thread, true);
    state.upsert(SurfaceId::Changes);
    state.upsert(SurfaceId::Background);
    state.activate(&SurfaceId::Changes);
    state.maximized = true;
    state.width = 700.;
    surfaces.save(&dir).unwrap();
    let prefs = Preferences {
        dir,
        ..Preferences::ephemeral()
    };
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let (tabs, active) = panel_tabs(&view, thread, cx);
    assert_eq!(tabs, [SurfaceId::Overview, SurfaceId::Changes, SurfaceId::Background]);
    assert_eq!(active, Some(SurfaceId::Changes));
    assert!(cx.debug_bounds("changes").is_some(), "its body, read again");
    // Maximized, the board beside the full column falls under its least:
    // the nav rails, and the Panel takes the rail's room.
    assert!(cx.debug_bounds("nav-rail").is_some());
    let width = cx.debug_bounds("panel").unwrap().size.width;
    assert_eq!(width, px(1900. - NAV_FOLDED_W - DOCK_THREAD_MIN_W), "maximized");
}

/// On overflow a chevron stands at the end with more to show, and scrolls
/// by three quarters of the strip, at least 120.
#[gpui::test]
fn overflowing_tabs_show_chevrons_that_scroll_the_strip(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("surfaces-overflow", cx);
    assert!(cx.debug_bounds("panel-tabs-right").is_none(), "no overflow, no chevron");
    for at in 0..12 {
        upsert(&view, thread, SurfaceId::File(format!("/tmp/overflowing-tab-{at}.rs").into()), cx);
    }
    view.update(cx, |view, cx| view.activate_surface(thread, &SurfaceId::Overview, cx));
    tick(cx);
    tick(cx);
    assert!(cx.debug_bounds("panel-tabs-left").is_none(), "nothing left of the first");
    let right = cx.debug_bounds("panel-tabs-right").expect("more to the right");
    let visible = view.read_with(cx, |view, _| f32::from(view.panel.tabs_scroll.bounds().size.width));
    cx.simulate_click(right.center(), gpui::Modifiers::none());
    tick(cx);
    let scrolled = view.read_with(cx, |view, _| -f32::from(view.panel.tabs_scroll.offset().x));
    let step = crate::cockpit::panel::tab_scroll_step(visible);
    assert!((scrolled - step).abs() < 1., "{scrolled} by {step}");
    assert!(cx.debug_bounds("panel-tabs-left").is_some(), "now more to the left");
    assert_eq!(crate::cockpit::panel::tab_scroll_step(100.), SURFACE_TAB_SCROLL_MIN);
    assert_eq!(crate::cockpit::panel::tab_scroll_step(400.), 300.);
}

/// With motion on, a chevron eases the strip to where it sends it (T3's
/// smooth `scrollBy`) rather than jumping.
#[gpui::test]
fn a_chevron_eases_the_strip(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    let (view, thread, cx) = solo("surfaces-overflow-ease", cx);
    for at in 0..12 {
        upsert(&view, thread, SurfaceId::File(format!("/tmp/easing-tab-{at}.rs").into()), cx);
    }
    view.update(cx, |view, cx| view.activate_surface(thread, &SurfaceId::Overview, cx));
    tick(cx);
    tick(cx);
    let visible = view.read_with(cx, |view, _| f32::from(view.panel.tabs_scroll.bounds().size.width));
    let step = crate::cockpit::panel::tab_scroll_step(visible);
    let right = cx.debug_bounds("panel-tabs-right").expect("more to the right");
    cx.simulate_click(right.center(), gpui::Modifiers::none());
    let scrolled = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| -f32::from(view.panel.tabs_scroll.offset().x))
    };
    let frames = |cx: &mut gpui::VisualTestContext, ms: u64| {
        for _ in 0..ms / 16 {
            cx.executor().advance_clock(std::time::Duration::from_millis(16));
            cx.run_until_parked();
            cx.update(|window, cx| window.simulate_next_frame(cx));
            cx.run_until_parked();
        }
    };
    frames(cx, MOTION_TAB_SCROLL_MS / 3);
    let midway = scrolled(cx);
    assert!(midway > 0. && midway < step - 1., "on its way: {midway} of {step}");
    frames(cx, MOTION_TAB_SCROLL_MS);
    assert!((scrolled(cx) - step).abs() < 1., "landed");
}

/// #236: "Open a surface" is the Panel's focus target: a press in it takes
/// the keyboard from the Composer, and a bare letter fires its row.
#[gpui::test]
fn the_empty_panels_letters_work_from_the_keyboard(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("empty-keys", cx);
    view.update(cx, |view, cx| view.close_surface(thread, &SurfaceId::Overview, cx));
    tick(cx);
    let head = cx.debug_bounds("panel-empty-head").expect("Open a surface");
    cx.simulate_click(head.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_keystrokes("d");
    tick(cx);
    let typed = view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).text().to_string());
    assert_eq!(typed, "", "the Composer did not take the letter");
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Changes), "d opens Changes");
}

/// The `+` menu's bare letter picks its row from the keyboard.
#[gpui::test]
fn the_plus_menus_letter_picks_its_row(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("plus-keys", cx);
    view.update(cx, |view, cx| view.open_surface_menu(cx));
    tick(cx);
    cx.simulate_keystrokes("d");
    tick(cx);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Changes), "d opens Changes");
}

/// ADR 0039: an agent's reveal is refused once the operator has acted in
/// the Panel since the revision it read. Hiding the Panel and maximizing
/// it are acting.
#[gpui::test]
fn an_agents_reveal_does_not_undo_a_hide_or_a_maximize(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("reveal-hidden", cx);
    upsert(&view, thread, SurfaceId::Overview, cx);
    let seen = view.read_with(cx, |view, _| view.surface_revision(thread));
    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none(), "the operator hid the Panel");
    let revealed = view.update(cx, |view, _| view.reveal_surface(thread, SurfaceId::Browser, seen));
    tick(cx);
    assert!(!revealed, "the reveal is refused");
    assert!(cx.debug_bounds("panel").is_none(), "the Panel stays hidden");

    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    tick(cx);
    let seen = view.read_with(cx, |view, _| view.surface_revision(thread));
    view.update(cx, |view, cx| view.toggle_panel_maximized(thread, cx));
    let revealed = view.update(cx, |view, _| view.reveal_surface(thread, SurfaceId::Browser, seen));
    assert!(!revealed, "a maximize is acting too");
}
