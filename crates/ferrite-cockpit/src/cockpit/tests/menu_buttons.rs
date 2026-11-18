//! A button that opens a menu, pressed by the pointer as the operator
//! does: its menu hangs under it, a second press shuts it, no tooltip is
//! drawn over it, and Escape gives the keyboard back to what had it.
#[allow(unused_imports)]
use super::*;
use crate::cockpit::panel::Opener;

/// Which of the button's edges the menu's own lines up with.
#[derive(Clone, Copy, Debug)]
enum Edge {
    Left,
    Right,
}

struct Door {
    button: String,
    target: MenuTarget,
    rows: Vec<String>,
    edge: Edge,
}

fn labels(rows: &[&str]) -> Vec<String> {
    rows.iter().map(|row| row.to_string()).collect()
}

fn focus_composer(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    view.update_in(cx, |view, window, cx| {
        let composer = view.panes[0].composer.focus_handle(cx);
        window.focus(&composer, cx);
    });
    tick(cx);
}

fn rest_on(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
    cx.simulate_mouse_move(at + gpui::point(px(1.), px(0.)), None, gpui::Modifiers::none());
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
}

fn shut(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| {
        view.close_popup(cx);
    });
    tick(cx);
}

fn menu_up(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Option<MenuTarget> {
    view.read_with(cx, |view, _| view.context_menu().map(|menu| menu.target))
}

/// Every rule a menu button owns, on `door`. `show` brings the button up
/// where it only stands under the pointer. Each fault found is listed,
/// rather than the first one stopping the rest.
fn faults(
    view: &Entity<CockpitView>,
    door: Door,
    show: impl Fn(&mut gpui::VisualTestContext),
    cx: &mut gpui::VisualTestContext,
) -> Vec<String> {
    let name = door.button.clone();
    let mut faults = Vec::new();
    let button = |cx: &mut gpui::VisualTestContext| {
        show(cx);
        debug_bounds(cx, door.button.clone()).unwrap_or_else(|| panic!("{name} stands"))
    };

    focus_composer(view, cx);
    let at = button(cx);
    rest_on(cx, at.center());
    if cx.debug_bounds("tooltip").is_none() {
        faults.push("no tooltip at rest with its menu shut".to_string());
    }
    let at = button(cx);
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
    let rows = view.read_with(cx, |view, _| {
        view.context_menu().map(|menu| {
            menu.rows.iter().flatten().map(|(item, _)| item.label.to_string()).collect::<Vec<_>>()
        })
    });
    assert_eq!(menu_up(view, cx), Some(door.target), "{name}: a press opens its menu");
    assert_eq!(rows.unwrap(), door.rows, "{name}: its rows");

    let menu = cx.debug_bounds("context-menu").expect("the menu is drawn");
    let gap = menu.top() - at.bottom();
    if gap < px(0.) || gap > px(ferrite_ui::theme::FLOAT_OFFSET + 1.) {
        faults.push(format!("not under its button: menu {menu:?}, button {at:?}"));
    }
    let (edge, button_edge) = match door.edge {
        Edge::Left => (menu.left(), at.left()),
        Edge::Right => (menu.right(), at.right()),
    };
    if (edge - button_edge).abs() > px(1.) {
        faults.push(format!("its {:?} edge is not on the button's: menu {menu:?}, button {at:?}", door.edge));
    }

    rest_on(cx, at.center());
    if cx.debug_bounds("tooltip").is_some() {
        faults.push("a tooltip is drawn while its menu is up".to_string());
    }

    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(menu_up(view, cx), None, "{name}: Escape closes it");
    let before = composer_text(view, cx);
    cx.simulate_input("z");
    tick(cx);
    if composer_text(view, cx) != format!("{before}z") {
        faults.push("after Escape, typing does not reach the Composer that had the keyboard".to_string());
    }

    let at = button(cx);
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(menu_up(view, cx), Some(door.target), "{name}: it opens again");
    let at = button(cx);
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
    if menu_up(view, cx).is_some() {
        faults.push("a second press on it does not shut its menu".to_string());
    }
    shut(view, cx);
    faults.into_iter().map(|fault| format!("{name}: {fault}")).collect()
}

fn no_faults(faults: Vec<String>) {
    assert!(faults.is_empty(), "{faults:#?}");
}

#[gpui::test]
fn the_navs_filter_button_owns_its_menu(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("menu-button-nav-filter", 3);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    tick(cx);
    let door = Door {
        button: "nav-filter".into(),
        target: MenuTarget::NavFilter,
        rows: labels(&["All projects", "ferrite-cockpit"]),
        edge: Edge::Right,
    };
    no_faults(faults(&view, door, |_| {}, cx));
}

#[gpui::test]
fn the_view_titles_menu_button_owns_its_menu(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("menu-button-view-title", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let door = Door {
        button: "view-title-chevron-shown".into(),
        target: MenuTarget::Thread(thread),
        rows: labels(&[
            "Open in",
            "Pin",
            "Rename",
            "New thread in this project",
            "Copy path",
            "Copy thread link",
            "Refresh PR status",
            "Snooze",
            "Park thread",
            "Delete thread",
        ]),
        edge: Edge::Left,
    };
    let show = |cx: &mut gpui::VisualTestContext| {
        let title = cx.debug_bounds("view-title").expect("the title");
        cx.simulate_mouse_move(title.center(), None, gpui::Modifiers::none());
        tick(cx);
    };
    no_faults(faults(&view, door, show, cx));
}

#[gpui::test]
fn the_files_more_door_owns_its_menu(cx: &mut TestAppContext) {
    let (core, _fake, _checkout) = bound_cockpit("menu-button-files-more", Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Files, cx));
    tick(cx);
    let door = Door {
        button: "files-more".into(),
        target: MenuTarget::FilesMore(thread),
        rows: labels(&["Show file tree", "Preview tabs", "Auto save", "Word wrap", "Tab size"]),
        edge: Edge::Right,
    };
    no_faults(faults(&view, door, |_| {}, cx));
}

#[gpui::test]
fn a_nav_cards_snooze_button_owns_its_menu(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("menu-button-snooze", 2);
    let mut threads = core.threads();
    threads.sort();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = threads[0];
    let door = Door {
        button: format!("nav-card-snooze-{}", thread.get()),
        target: MenuTarget::Snooze(thread),
        rows: labels(&["1 hour", "3 hours", "Tomorrow at 9 am", "Next Monday at 9 am", "Custom…"]),
        edge: Edge::Right,
    };
    let card = format!("nav-thread-{}", thread.get());
    let show = move |cx: &mut gpui::VisualTestContext| {
        if debug_bounds(cx, format!("nav-card-snooze-{}", thread.get())).is_none() {
            let card = bounds(cx, card.clone());
            cx.simulate_mouse_move(card.center(), None, gpui::Modifiers::none());
            tick(cx);
        }
    };
    no_faults(faults(&view, door, show, cx));
}

#[gpui::test]
fn the_panels_plus_owns_its_menu(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("menu-button-panel-plus", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let door = Door {
        button: "panel-plus".into(),
        target: MenuTarget::SurfaceMenu(thread),
        rows: labels(&["Browser", "Terminal", "Files", "Changes", "Pull request", "Side chat", "Background", "Overview"]),
        edge: Edge::Left,
    };
    no_faults(faults(&view, door, |_| {}, cx));
}
