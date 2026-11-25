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

/// The environment chip's menu, open while a copy moves, reads the copy
/// as it is now, as its chip and its toast do: under way, then done.
#[gpui::test]
fn an_open_environment_menu_follows_the_copy(cx: &mut TestAppContext) {
    let (view, _draft, _mine, _on_box, _dest, cx) = copy_under_way("env-menu-follows", cx);
    // The draft is sent here: the copy carries on, on every chip's menu.
    pick_on(&view, ENVIRONMENT, "Local", cx);
    settles(cx);
    press(cx, ENVIRONMENT);
    until(cx, |cx| view.read_with(cx, |view, _| view.band_menu().is_some()));
    let copy_row = |cx: &mut gpui::VisualTestContext| {
        band_rows(&view, cx)
            .into_iter()
            .find(|row| row.0.ends_with("app to box"))
            .map(|row| (row.0, row.1))
    };
    let opened = copy_row(cx);
    assert!(
        opened.as_ref().is_some_and(|(name, how)| name == "Copying app to box" && how.ends_with('%')),
        "{opened:?}"
    );

    BOX_SLOW.store(false, Ordering::SeqCst);
    until(cx, |cx| copy_is(&view, cx, |phase| matches!(phase, Phase::Done(_))));
    settles(cx);
    assert!(view.read_with(cx, |view, _| view.band_menu().is_some()), "the menu stays open");
    assert_eq!(copy_row(cx), Some(("Copied app to box".into(), "done".into())), "the row says it is done");
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

/// A copy that stopped while its draft waited on it, the draft's Pane then
/// closed with no pick, waits for no draft: it is a pick away on any
/// chip's menu, as stopped, with its Resume, until the operator closes
/// its card.
#[gpui::test]
fn a_stopped_copy_whose_draft_closed_is_still_a_pick_away(cx: &mut TestAppContext) {
    let (view, draft, mine, _on_box, _dest, cx) = copy_under_way("env-draft-closed", cx);
    box_down(true);
    BOX_SLOW.store(false, Ordering::SeqCst);
    on_toast(cx, "copy-toast-again");
    close_toast(cx);
    assert_eq!(text_of(cx, "band-copy-waits").as_deref(), Some("Copy stopped"));

    view.update(cx, |view, cx| view.close_pane(draft, cx));
    until(cx, |cx| view.read_with(cx, |view, _| view.index_of(draft).is_none()));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    assert_ne!(focused_draft(&view, cx), draft, "another draft");
    settles(cx);
    assert!(debug_bounds(cx, "band-copy-waits".into()).is_none(), "no draft waits on it");
    let rows = environment_rows(&view, cx);
    assert!(
        rows.iter().any(|row| row.0 == "Copy of app to box" && row.1 == "stopped"),
        "the stopped copy is a pick away: {rows:?}"
    );
    pick_on(&view, ENVIRONMENT, "Copy of app to box", cx);
    drawn(cx, "placement-failed");
    assert!(debug_bounds(cx, "placement-resume".into()).is_some(), "its Resume");
    press(cx, "placement-dismiss");
    assert!(!copy_is(&view, cx, |_| true), "closed, it goes");
    box_down(false);
}

/// A copy in on box whose Placement this computer cannot keep is no
/// silent dead end: it says so in plain words, with Resume, and its
/// Resume keeps it once it can.
#[gpui::test]
fn a_copy_whose_placement_could_not_be_kept_says_so_and_resumes(cx: &mut TestAppContext) {
    let base = resolved_dir("env-unkept");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) = two_machines_keeping_projects_in("env-unkept", &projects);
    let on_box = machines.held(1).unwrap();
    let mine = registered_here(&mut core, &project_folder(&base.join("here/app"), None));
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    // Where copies went cannot be written: its file is a folder.
    let jammed = box_dir(&view, cx).join("placements.json");
    std::fs::create_dir_all(&jammed).unwrap();
    press(cx, "placement-copy");
    until(cx, |cx| copy_is(&view, cx, |phase| matches!(phase, Phase::Done(_) | Phase::Failed { .. })));
    settles(cx);
    let said = text_of(cx, "placement-failed").unwrap_or_default();
    assert!(
        said.starts_with("Copied to box, but this computer could not keep where it went: "),
        "{said:?}"
    );
    assert!(!said.contains("os error"), "{said}");
    assert!(debug_bounds(cx, "placement-resume".into()).is_some(), "its Resume keeps it");
    assert_eq!(draft_project(&view, draft, cx), mine, "not sent there yet");

    std::fs::remove_dir_all(&jammed).unwrap();
    press(cx, "placement-resume");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    let kept = ferrite_core::placement::placements(&box_dir(&view, cx));
    assert_eq!(kept.len(), 1, "where it went is kept: {kept:?}");
}
