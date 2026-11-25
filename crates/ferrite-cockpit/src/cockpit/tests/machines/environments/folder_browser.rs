//! The setup sheet's browser of a Machine's folders: walked with the
//! mouse or the keys, however many folders it holds.

use super::*;

/// The sheet's browser is the Machine's folders, walked with the mouse:
/// the folders in the one shown, never its files or its dot folders, a
/// press into one, `..` out; New folder names one there, which the copy
/// makes. Where the copy goes reads as the folder shown and its name.
#[gpui::test]
fn the_sheet_aims_the_copy_by_browsing_the_machines_folders(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let base = resolved_dir("env-browse");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-browse", &projects);
    let on_box = machines.held(1).unwrap();
    for folder in ["work/clients", "play", ".cache"] {
        std::fs::create_dir_all(projects.join(folder)).unwrap();
    }
    std::fs::write(projects.join("notes.txt"), "").unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    browse(cx);
    drawn(cx, "folder-row:work");
    assert!(debug_bounds(cx, "folder-row:play".into()).is_some());
    assert!(
        debug_bounds(cx, "folder-row-up".into()).is_some(),
        "a way out"
    );
    assert!(
        debug_bounds(cx, "folder-row:notes.txt".into()).is_none(),
        "no files"
    );
    assert!(
        debug_bounds(cx, "folder-row:.cache".into()).is_none(),
        "no dot folders"
    );
    let line = sheet_line(&view, cx);
    let at = |folder: &str| format!("{}/{folder}", projects.display());

    press(cx, "folder-row:work");
    assert_eq!(text_in(&line, cx), at("work/"));
    drawn(cx, "folder-row:clients");
    assert_eq!(destination(&view, cx), at("work/app"));
    press(cx, "folder-row-up");
    assert_eq!(text_in(&line, cx), at(""));
    drawn(cx, "folder-row:play");
    press(cx, "folder-row:play");
    assert_eq!(destination(&view, cx), at("play/app"));
    drawn(cx, "folder-browser-empty");

    press(cx, "placement-new-folder");
    assert!(debug_bounds(cx, "folder-browser-naming".into()).is_some());
    cx.simulate_input("fresh");
    cx.simulate_keystrokes("enter");
    assert!(
        sheet(&view, cx).is_some(),
        "the name's ↵ is not the sheet's"
    );
    assert_eq!(text_in(&line, cx), at("play/fresh/"));
    assert_eq!(
        text_of(cx, "folder-browser-new").as_deref(),
        Some("The copy makes this folder"),
        "said otherwise than the button that named it"
    );
    assert!(
        !projects.join("play/fresh").exists(),
        "named, not made until the copy"
    );
    assert_eq!(destination(&view, cx), at("play/fresh/app"));
    let name = sheet(&view, cx)
        .unwrap()
        .read_with(cx, |sheet, _| sheet.name.clone());
    name.update(cx, |name, cx| name.set(String::new(), cx));
    let focus = name.read_with(cx, |name, cx| name.focus_handle(cx));
    cx.update(|window, cx| window.focus(&focus, cx));
    tick(cx);
    cx.simulate_input("mirror");
    assert_eq!(
        destination(&view, cx),
        at("play/fresh/mirror"),
        "the name keeps the keys"
    );

    drawn(cx, "placement-ignored:.env");
    press(cx, "placement-copy");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    let dest = projects.join("play/fresh/mirror");
    assert!(dest.join("main.rs").is_file(), "the copy made the folder");
    assert!(roots_on(&view, on_box, cx).contains(&dest));
}

/// A copy named `.` would land in the folder shown itself: refused.
#[gpui::test]
fn a_copy_named_dot_is_refused(cx: &mut TestAppContext) {
    let base = resolved_dir("env-dot");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-dot", &projects);
    std::fs::write(projects.join("keep.txt"), "mine").unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-ignored:.env");
    let name = sheet(&view, cx)
        .unwrap()
        .read_with(cx, |sheet, _| sheet.name.clone());
    name.update(cx, |name, cx| name.set(".".into(), cx));
    tick(cx);
    press(cx, "placement-copy");
    assert_eq!(
        text_of(cx, "placement-refusal").as_deref(),
        Some("A folder cannot be named .")
    );
    for _ in 0..4 {
        tick(cx);
    }
    assert!(debug_bounds(cx, "placement-occupied".into()).is_none(), "nothing was asked of box");
}

/// A folder box cannot read says so where its rows would be, and the
/// sheet goes on: `..` leads out of it.
#[gpui::test]
fn a_folder_that_cannot_be_read_says_so_in_place(cx: &mut TestAppContext) {
    let base = resolved_dir("env-unread");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-unread", &projects);
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| sheet(&view, cx).is_some());
    browse(cx);
    let line = sheet_line(&view, cx);
    let missing = format!("{}/missing/", projects.display());
    line.update(cx, |line, cx| line.set(missing.clone(), cx));
    drawn(cx, "folder-browser-unread");
    assert!(
        text_of(cx, "folder-browser-unread")
            .is_some_and(|said| said.starts_with(&format!("{missing} cannot be read"))),
        "it names the folder"
    );
    assert_eq!(text_in(&line, cx), missing, "and stays where it was typed");
    press(cx, "folder-row-up");
    assert_eq!(text_in(&line, cx), format!("{}/", projects.display()));
    assert!(debug_bounds(cx, "folder-browser-unread".into()).is_none());
}

/// A projects folder box does not have yet is where the copy goes all the
/// same, so the browser stays shut: opened, it is on that folder and says
/// the copy makes it, and the copy does.
#[gpui::test]
fn a_projects_folder_the_machine_lacks_is_made_by_the_copy(cx: &mut TestAppContext) {
    let base = resolved_dir("env-no-projects");
    let missing = base.join("nowhere");
    let (mut core, _fake, machines, _agents) = two_machines_with(
        "env-no-projects",
        0,
        Machine {
            projects_dir: missing.display().to_string(),
            ..box_machine()
        },
    );
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    drawn(cx, "placement-destination");
    settles(cx);
    assert!(debug_bounds(cx, "folder-browser-new".into()).is_none(), "shut while where it goes is sure");
    browse(cx);
    drawn(cx, "folder-browser-new");
    let line = sheet_line(&view, cx);
    assert_eq!(text_in(&line, cx), format!("{}/", missing.display()), "not home");
    assert_eq!(destination(&view, cx), format!("{}/app", missing.display()));
    assert!(debug_bounds(cx, "folder-browser-unread".into()).is_none());
    drawn(cx, "placement-ignored:.env");
    press(cx, "placement-copy");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    assert!(missing.join("app/main.rs").is_file());
}

/// The sheet is finished with the arrows and ↵ alone: ↓ walks the rows,
/// ↵ goes into the one it rests on, and ↵ once more copies there.
#[gpui::test]
fn the_sheet_is_finished_with_the_arrows_and_enter(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let base = resolved_dir("env-keys");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-keys", &projects);
    let on_box = machines.held(1).unwrap();
    std::fs::create_dir_all(projects.join("work")).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    browse(cx);
    drawn(cx, "folder-row:work");
    drawn(cx, "placement-ignored:.env");
    let line = sheet_line(&view, cx);
    let focus = line.read_with(cx, |line, cx| line.focus_handle(cx));
    cx.update(|window, cx| window.focus(&focus, cx));
    tick(cx);

    cx.simulate_keystrokes("down down");
    cx.simulate_keystrokes("enter");
    assert_eq!(text_in(&line, cx), format!("{}/work/", projects.display()));
    cx.simulate_keystrokes("enter");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    assert!(projects.join("work/app/main.rs").is_file());
}

/// Tab on the browser's line goes into the folder its last name starts;
/// with no name to finish it moves on, as Tab does everywhere.
#[gpui::test]
fn tab_finishes_a_folder_name_or_moves_on(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let base = resolved_dir("env-tab");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-tab", &projects);
    std::fs::create_dir_all(projects.join("work")).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    browse(cx);
    drawn(cx, "folder-row:work");
    let line = sheet_line(&view, cx);
    let focus = line.read_with(cx, |line, cx| line.focus_handle(cx));
    cx.update(|window, cx| window.focus(&focus, cx));
    tick(cx);
    cx.simulate_input("wo");
    cx.simulate_keystrokes("tab");
    assert_eq!(text_in(&line, cx), format!("{}/work/", projects.display()));
    assert!(cx.update(|window, _| focus.is_focused(window)), "the name finished, the line keeps the keys");

    cx.simulate_keystrokes("tab");
    tick(cx);
    assert!(!cx.update(|window, _| focus.is_focused(window)), "nothing to finish: Tab moves on");
}

/// A folder of twenty thousand folders draws only the rows in view, and
/// the arrows reach the last of them.
#[gpui::test]
fn a_folder_of_thousands_draws_only_the_rows_in_view(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let base = resolved_dir("env-thousands");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-thousands", &projects);
    for at in 0..20_000 {
        std::fs::create_dir(projects.join(format!("f{at:05}"))).unwrap();
    }
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    browse(cx);
    drawn(cx, "folder-row:f00000");
    assert!(debug_bounds(cx, "folder-row:f19999".into()).is_none(), "out of view, not drawn");
    let line = sheet_line(&view, cx);
    let focus = line.read_with(cx, |line, cx| line.focus_handle(cx));
    cx.update(|window, cx| window.focus(&focus, cx));
    tick(cx);
    cx.simulate_keystrokes("up");
    drawn(cx, "folder-row:f19999");
    assert!(debug_bounds(cx, "folder-row:f00000".into()).is_none());
}
