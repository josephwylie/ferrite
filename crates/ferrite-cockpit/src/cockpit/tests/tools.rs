//! The Panel's one tabbed host (ADR 0039, `cockpit/panel.rs`,
//! `panel/strip.rs`): the tool chords open their Surfaces through the one
//! door, the head row's tabs, `+` menu, maximize and edge, and "Open a
//! surface".
use super::*;
use crate::cockpit::panel::least_w;
use crate::cockpit::surfaces::{SurfaceAction, SurfaceId};
use ferrite_ui::theme::*;

fn solo<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, ThreadKey, &'a mut gpui::VisualTestContext) {
    let (core, _fake, _checkout) = bound_cockpit(name, Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, thread, cx)
}

fn upsert(view: &Entity<CockpitView>, thread: ThreadKey, id: SurfaceId, cx: &mut gpui::VisualTestContext) {
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
    // Wide enough to show all three tabs, as the operator would make it.
    let edge = cx.debug_bounds("panel-edge").unwrap().center();
    let to = gpui::point(px(1900. - 480.), edge.y);
    cx.simulate_mouse_down(edge, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    let changes = cx.debug_bounds("panel-tab-Changes").unwrap().center();
    cx.simulate_mouse_move(changes, None, gpui::Modifiers::none());
    // 5px and back onto itself: a click, which shows it.
    cx.simulate_mouse_down(changes, gpui::MouseButton::Left, gpui::Modifiers::none());
    let near = gpui::point(changes.x + px(5.), changes.y);
    cx.simulate_mouse_move(near, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(near, gpui::MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    let (tabs, active) = panel_tabs(&view, thread, cx);
    assert_eq!(tabs, [SurfaceId::Overview, SurfaceId::Changes, SurfaceId::Background]);
    assert_eq!(active, Some(SurfaceId::Changes));
    // Dragged onto Overview: it moves there. The shown tool's floor sets the
    // Panel's width, so the tabs are read again.
    let changes = cx.debug_bounds("panel-tab-Changes").unwrap().center();
    let overview = cx.debug_bounds("panel-tab-Overview").unwrap().center();
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
        view.context_menu()
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
    if view.read_with(cx, |view, _| view.context_menu().is_none()) {
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
    assert_eq!(rest, px(least_w()));
    let door = cx.debug_bounds("panel-maximize").expect("in the head row");
    cx.simulate_click(door.center(), gpui::Modifiers::none());
    tick(cx);
    let wide = cx.debug_bounds("panel").unwrap().size.width;
    assert_eq!(wide, px(1900. - NAV_COLUMN_W - DOCK_THREAD_MIN_W));
    assert!(view.read_with(cx, |view, _| view.panel.surfaces.get(thread).unwrap().maximized));
    let door = cx.debug_bounds("panel-restore").unwrap();
    cx.simulate_click(door.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(cx.debug_bounds("panel").unwrap().size.width, rest, "restored");
}

#[gpui::test]
fn maximize_with_the_nav_folded_takes_its_room(cx: &mut TestAppContext) {
    let (core, _fake, _checkout) = bound_cockpit("surfaces-maximize-rail", Provider::Claude);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.nav_collapsed = true);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    assert!(cx.debug_bounds("nav-card").is_none(), "the nav is folded");
    let rest = cx.debug_bounds("panel").unwrap().size.width;
    let door = cx.debug_bounds("panel-maximize").expect("in the head row");
    cx.simulate_click(door.center(), gpui::Modifiers::none());
    tick(cx);
    let wide = cx.debug_bounds("panel").unwrap().size.width;
    assert!(wide > rest, "maximize widens the Panel: {wide:?} vs {rest:?}");
    assert_eq!(wide, px(1440. - DOCK_THREAD_MIN_W));
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
    assert_eq!(cx.debug_bounds("panel").unwrap().size.width, px(least_w()));
    // A Terminal widens it to its own floor.
    cx.simulate_keystrokes("cmd-j");
    tick(cx);
    assert_eq!(cx.debug_bounds("panel").unwrap().size.width, px(TERMINAL_W + CAPTION_ALLOWANCE));
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
    for action in [
        SurfaceAction::Open(SurfaceId::Changes),
        SurfaceAction::Open(SurfaceId::Background),
        SurfaceAction::Activate(SurfaceId::Changes),
        SurfaceAction::Resize(700.),
        SurfaceAction::ToggleMaximized,
    ] {
        surfaces.apply(ferrite_core::ThreadKey::fixture(0, thread.thread().get()), true, action);
    }
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
    assert!(cx.debug_bounds("nav-card").is_none());
    let width = cx.debug_bounds("panel").unwrap().size.width;
    assert_eq!(width, px(1900. - DOCK_THREAD_MIN_W), "maximized");
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
    settles(cx);
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

/// However many tabs the strip holds, the `+` stays in the head's row,
/// clear of the right chevron and of maximize, and a click on it opens its
/// menu.
#[gpui::test]
fn the_plus_stays_in_view_past_overflowing_tabs(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("surfaces-overflow-plus", cx);
    let overview = cx.debug_bounds("panel-tab-Overview").unwrap();
    let plus = cx.debug_bounds("panel-plus").expect("the +");
    assert_eq!(plus.left() - overview.right(), px(SURFACE_HEAD_GAP), "the + follows the last tab");
    for at in 0..12 {
        upsert(&view, thread, SurfaceId::File(format!("/tmp/overflowing-plus-{at}.rs").into()), cx);
    }
    view.update(cx, |view, cx| view.activate_surface(thread, &SurfaceId::Overview, cx));
    tick(cx);
    tick(cx);
    let head = cx.debug_bounds("panel-head").unwrap();
    let plus = cx.debug_bounds("panel-plus").expect("the +");
    let maximize = cx.debug_bounds("panel-maximize").unwrap();
    let right = cx.debug_bounds("panel-tabs-right").expect("the strip overflows");
    let within = |inner: gpui::Bounds<Pixels>, outer: gpui::Bounds<Pixels>| {
        inner.left() >= outer.left() && inner.right() <= outer.right()
    };
    assert!(within(plus, head), "the + {plus:?} lies in the head {head:?}");
    assert!(plus.right() <= maximize.left(), "the + {plus:?} is clear of maximize {maximize:?}");
    assert!(plus.left() >= right.right(), "the + {plus:?} is past the chevron {right:?}");
    cx.simulate_click(plus.center(), gpui::Modifiers::none());
    tick(cx);
    let target = view.read_with(cx, |view, _| view.context_menu().map(|menu| menu.target.clone()));
    assert_eq!(target, Some(MenuTarget::SurfaceMenu(thread)), "its menu is up");
    // Near the window's edge (macOS) the menu has no room to line up with
    // the `+` and moves in; short of the caption buttons (Windows) it has.
    let menu = cx.debug_bounds("context-menu").expect("the menu is drawn");
    let window = gpui::Bounds::new(gpui::point(px(0.), px(0.)), gpui::size(px(1900.), px(900.)));
    assert!(within(menu, window), "the menu {menu:?} lies in the window");
    if plus.left() + menu.size.width <= window.right() {
        assert_eq!(menu.left(), plus.left(), "the menu {menu:?} lines up with the + {plus:?}");
    } else {
        assert!(menu.left() < plus.left(), "the menu {menu:?} moves in from the + {plus:?}");
    }
    assert_eq!(
        menu.top() - plus.bottom(),
        px(ferrite_ui::theme::MENU_UNDER_GAP),
        "the menu {menu:?} hangs under the + {plus:?}"
    );
}

/// The shown tab is never left out of view in a strip that overflows:
/// each Surface opened shows whole between the chevrons, the one whose
/// opening brings the chevrons in too, and so does the first tab picked
/// again once the Panel has ridden back to its width.
#[gpui::test]
fn the_shown_tab_scrolls_into_view(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    let (view, thread, cx) = solo("surfaces-overflow-shown", cx);
    fn shows(view: &Entity<CockpitView>, selector: &str, cx: &mut gpui::VisualTestContext) {
        let strip = view.read_with(cx, |view, _| view.panel.tabs_scroll.bounds());
        let tab = cx.update(|window, _| window.debug_bounds(selector)).unwrap();
        assert!(
            tab.left() >= strip.left() && tab.right() <= strip.right(),
            "{selector} {tab:?} shows within the strip {strip:?}"
        );
    }
    let mut opened = vec![
        (SurfaceId::Background, "panel-tab-Background".to_string()),
        (SurfaceId::Changes, "panel-tab-Changes".to_string()),
    ];
    opened.extend((0..6).map(|at| {
        let id = SurfaceId::File(format!("/tmp/shown-tab-{at}.rs").into());
        (id, format!("panel-tab-File-shown-tab-{at}.rs"))
    }));
    for (id, selector) in opened {
        upsert(&view, thread, id, cx);
        settles(cx);
        shows(&view, &selector, cx);
    }
    assert!(cx.debug_bounds("panel-tabs-right-spent").is_some(), "the strip overflows");
    view.update(cx, |view, cx| view.activate_surface(thread, &SurfaceId::Overview, cx));
    settles(cx);
    shows(&view, "panel-tab-Overview", cx);
}

/// A chevron with nothing more its way keeps its slot but takes no press:
/// at the strip's start the left one scrolls nothing, and once the strip
/// has moved the same spot scrolls it back.
#[gpui::test]
fn a_spent_chevron_takes_no_press(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("surfaces-spent-chevron", cx);
    for at in 0..12 {
        upsert(&view, thread, SurfaceId::File(format!("/tmp/spent-tab-{at}.rs").into()), cx);
    }
    view.update(cx, |view, cx| view.activate_surface(thread, &SurfaceId::Overview, cx));
    tick(cx);
    tick(cx);
    let scrolled = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| -f32::from(view.panel.tabs_scroll.offset().x))
    };
    let head = cx.debug_bounds("panel-head").expect("the Panel's head");
    let left = gpui::point(head.left() + px(SURFACE_HEAD_PAD_L + 10.), head.center().y);
    cx.simulate_click(left, gpui::Modifiers::none());
    tick(cx);
    assert_eq!(scrolled(cx), 0., "nothing to the left: the press scrolls nothing");
    let right = cx.debug_bounds("panel-tabs-right").expect("more to the right");
    cx.simulate_click(right.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(scrolled(cx) > 0.);
    cx.simulate_click(left, gpui::Modifiers::none());
    tick(cx);
    assert_eq!(scrolled(cx), 0., "live, the same spot scrolls it back");
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

/// What `thread`'s Files, Changes, a pull request's diff and the Overview's
/// Changes section hold for it, with its browser, its PR read, a turned
/// background card and, when its Panel is the one shown, the stop F6 put
/// the keyboard on.
fn hold_tools(view: &Entity<CockpitView>, thread: ThreadKey, cx: &mut gpui::VisualTestContext) -> usize {
    view.update(cx, |view, cx| {
        let file = view.thread_path(thread, cx).unwrap().join("src/lib.rs");
        view.open_in_files(thread, file, None, true, cx);
        view.upsert_surface(thread, SurfaceId::Changes, cx);
        view.ensure_diff(crate::cockpit::tools::Diff::PullRequest(crate::cockpit::panel::pr_read::PrTarget::thread(thread, 1)), cx);
        view.overview.update(cx, |overview, cx| overview.toggle_file(thread, "src/lib.rs".to_string(), cx));
        view.ensure_browser(thread, cx);
        view.fixture_pr_snapshot(thread, super::pr::snapshot(ferrite_core::protocol::WirePrState::Open), cx);
        view.background.update(cx, |background, cx| background.turn(thread, "call-1".into(), cx));
    });
    tick(cx);
    super::panel_focus::press("f6", cx);
    view.read_with(cx, |view, cx| view.held_for_test(thread, cx))
}

#[gpui::test]
fn deleting_a_thread_frees_its_files_and_changes(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("tools-thread-deleted", cx);
    assert!(
        hold_tools(&view, thread, cx) >= 10,
        "Files, two diffs, an unfolded file, a browser, a PR read, a remembered stop, facts, a PR count and a turned background card"
    );
    press_nav_verb(&view, thread, MenuVerb::Delete, cx);
    assert!(view.read_with(cx, |view, cx| view.cockpit.read(cx).known_threads().is_empty()), "it was the last");
    assert!(!view.read_with(cx, |view, cx| view.cockpit.read(cx).known_threads().contains(&thread)), "the Thread is gone");
    assert_eq!(view.read_with(cx, |view, cx| view.held_for_test(thread, cx)), 0, "and all it held");
}

#[gpui::test]
fn a_parked_thread_keeps_its_files_and_changes(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("tools-thread-parked", cx);
    let held = hold_tools(&view, thread, cx);
    press_nav_verb(&view, thread, MenuVerb::Park, cx);
    assert!(view.read_with(cx, |view, cx| view.cockpit.read(cx).thread(thread).is_none()), "the Thread is parked");
    assert_eq!(view.read_with(cx, |view, cx| view.held_for_test(thread, cx)), held, "it keeps them");
}

/// A Worker (ADR 0037) parked is parked, not gone, though no list of the
/// operator's shows it.
#[gpui::test]
fn a_parked_worker_keeps_its_panel_files_and_changes(cx: &mut TestAppContext) {
    let (mut core, _fake, checkout) = bound_cockpit("tools-worker-parked", Provider::Claude);
    let worker = core
        .fixture_open_owned(
            Provider::Claude,
            WorkspaceChoice::Main { checkout },
            ferrite_core::ThreadOwner::Task("t-1".into()),
        )
        .unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let held = hold_tools(&view, worker, cx);
    let (tabs, _) = panel_tabs(&view, worker, cx);
    view.update(cx, |view, cx| view.close_pane(PaneIdentity::Thread(worker), cx));
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.cockpit.read(cx).thread(worker).is_none(), "the Worker is parked");
        assert!(view.cockpit.read(cx).owner(worker).is_some(), "and still its Task's");
    });
    assert_eq!(panel_tabs(&view, worker, cx).0, tabs, "its Panel keeps its tabs");
    assert_eq!(view.read_with(cx, |view, cx| view.held_for_test(worker, cx)), held, "and its tools");
}
