//! A copy followed off its card (ADR 0048 §2): its toast, drawn under
//! every menu the operator opens and letting the copy go only when the
//! operator closes it, and its row on every environment chip's menu, which
//! reads the copy as it is now.

use super::*;

/// A copy of app to box under way, its card sent to the background, with
/// the draft in Pane `draft` waiting on it. `big.bin` makes it long enough
/// to stop part way.
fn copy_under_way<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, PaneIdentity, ProjectKey, MachineId, PathBuf, &'a mut gpui::VisualTestContext) {
    toasts_rest(cx);
    let base = resolved_dir(name);
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) = two_machines_keeping_projects_in(name, &projects);
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    std::fs::write(source.join("big.bin"), vec![9u8; 4 << 20]).unwrap();
    let mine = registered_here(&mut core, &source);
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    let dest = projects.join("app");
    copying_part_way(&dest, cx);
    press(cx, "placement-background");
    on_toast(cx, "copy-toast-show");
    (view, draft, mine, on_box, dest, cx)
}

/// The rows of the environment chip's menu, opened and shut again.
fn environment_rows(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<(String, String, bool, bool)> {
    press(cx, ENVIRONMENT);
    until(cx, |cx| view.read_with(cx, |view, _| view.band_menu().is_some()));
    let rows = band_rows(view, cx);
    view.update(cx, |view, cx| view.close_popup(cx));
    rows
}

/// Whether there is a copy, and `is` says so of how it stands now.
fn copy_is(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext, is: impl Fn(&Phase) -> bool) -> bool {
    view.read_with(cx, |view, cx| view.copies.read(cx).iter().next().is_some_and(|(_, copy)| is(&copy.phase)))
}

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

/// A copy done whose toast went by itself, the operator elsewhere, is not
/// let go: the system's notification stays, and every chip's menu lists
/// the copy until the operator opens it. Its card offers the chat there,
/// which lets it go.
#[gpui::test]
fn a_copy_whose_toast_went_by_itself_stays_until_the_operator_acts(cx: &mut TestAppContext) {
    let (view, _draft, _mine, on_box, dest, cx) = copy_under_way("env-lapsed", cx);
    // The draft is sent here: the copy, once in, offers a chat there.
    pick_on(&view, ENVIRONMENT, "Local", cx);
    settles(cx);

    cx.deactivate_window();
    BOX_SLOW.store(false, Ordering::SeqCst);
    until(cx, |cx| roots_on(&view, on_box, cx).contains(&dest));
    on_toast(cx, "copy-toast-chat");
    let tag = copy_tag(&view, cx);
    let posted = |cx: &mut gpui::VisualTestContext| {
        cx.delivered_system_notifications()
            .iter()
            .any(|notification| notification.tag.as_ref() == tag)
    };
    assert!(posted(cx), "the window away, the system says so");

    leap(cx, Duration::from_secs(6));
    settles(cx);
    assert!(debug_bounds(cx, "notification-card".into()).is_none(), "the toast went by itself");
    assert!(posted(cx), "the system's notification stays");
    cx.update(|window, _| window.activate_window());
    settles(cx);
    assert!(posted(cx), "back in the window, nothing has shown it yet");
    assert!(debug_bounds(cx, "notification-card".into()).is_none(), "nor does its toast come back");

    let rows = environment_rows(&view, cx);
    assert!(
        rows.iter().any(|row| row.0 == "Copied app to box" && row.1 == "done"),
        "the copy is a pick away: {rows:?}"
    );
    pick_on(&view, ENVIRONMENT, "Copied app to box", cx);
    drawn(cx, "placement-chat");
    settles(cx);
    assert!(!posted(cx), "seen on its card, taken back");
    press(cx, "placement-chat");
    let chat = focused_draft(&view, cx);
    assert_eq!(draft_project(&view, chat, cx).machine(), on_box, "a chat on the copy");
    assert!(!copy_is(&view, cx, |_| true), "the chat lets it go");
}
