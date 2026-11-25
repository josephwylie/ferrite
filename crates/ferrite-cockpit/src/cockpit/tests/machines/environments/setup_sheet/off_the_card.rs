//! A copy followed off its card (ADR 0048 §2): its toast, drawn under
//! every menu the operator opens and letting the copy go only when the
//! operator closes it, and its row on every environment chip's menu, which
//! reads the copy as it is now.

use super::*;

/// A menu the operator opens is drawn over the kit's toasts
/// (`ferrite_ui::layer`): with the environment chip's menu open where a
/// toast stands, a press where the two overlap is the menu's.
#[gpui::test]
fn an_open_environment_menu_is_drawn_over_a_toast(cx: &mut TestAppContext) {
    use gpui::component::notification::Notification;
    use gpui::component::WindowExt as _;
    toasts_rest(cx);
    let base = resolved_dir("env-menu-over-toast");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) = two_machines_keeping_projects_in("env-menu-over-toast", &projects);
    let mine = registered_here(&mut core, &project_folder(&base.join("here/app"), None));
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let pressed = std::rc::Rc::new(std::cell::Cell::new(0));
    cx.update(|window, cx| {
        let pressed = pressed.clone();
        let toast = Notification::new()
            .title("Copying app to box")
            .message("Long enough a toast to stand where the menu opens.")
            .autohide(false)
            .on_click(move |_, _, _| pressed.set(pressed.get() + 1));
        window.push_notification(toast, cx);
    });
    settles(cx);
    press(cx, ENVIRONMENT);
    until(cx, |cx| view.read_with(cx, |view, _| view.band_menu().is_some()));
    settles(cx);
    let menu = debug_bounds(cx, "composer-menu".into()).expect("the menu");
    let toast = debug_bounds(cx, "notification-card".into()).expect("the toast");
    let both = menu.intersect(&toast);
    assert!(
        both.size.width > px(0.) && both.size.height > px(0.),
        "the menu at {menu:?} opens where the toast at {toast:?} stands"
    );
    cx.simulate_click(both.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(pressed.get(), 0, "the press was the menu's, not the toast's");
}
