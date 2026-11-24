//! A draft's environment chip (ADR 0048): where a new chat runs, chosen
//! until its first send. A pick moves the draft to its Project's
//! counterpart on the machine picked, the Project there of the same
//! repository or the one the setup sheet copied it to; a Project of this
//! computer with none there is copied by the sheet, and one of a machine
//! gets the Project card. What the draft holds goes with it.

use super::*;
use crate::cockpit::placement_view::PlacementView;
use std::path::{Path, PathBuf};

/// `root` registered as a Project of `machine`, once its Server says so.
fn registered(core: &mut Cockpit, machine: MachineId, root: &Path) -> ProjectKey {
    let asked = core.register_project(machine, root);
    core.wait_for(asked).expect("the Server registers the Project")
}

fn registered_here(core: &mut Cockpit, root: &Path) -> ProjectKey {
    let here = core.this_computer().unwrap();
    registered(core, here, root)
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git");
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A Project folder of this computer at `dir`: a committed checkout that
/// ignores `target/` and `.env`, holding both, with `origin` at `remote`
/// when there is one.
fn project_folder(dir: &Path, remote: Option<&str>) -> PathBuf {
    std::fs::create_dir_all(dir.join("target")).unwrap();
    std::fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(dir.join(".gitignore"), "target/\n.env\n").unwrap();
    std::fs::write(dir.join("target/app.bin"), [7u8; 2048]).unwrap();
    std::fs::write(dir.join(".env"), "KEY=1\n").unwrap();
    for args in [
        &["init", "-q"][..],
        &["config", "user.email", "operator@example.invalid"],
        &["config", "user.name", "operator"],
        &["add", "."],
        &["commit", "-q", "-m", "root"],
    ] {
        git(dir, args);
    }
    if let Some(remote) = remote {
        git(dir, &["remote", "add", "origin", remote]);
    }
    dir.to_path_buf()
}

/// box, its projects folder `projects`.
fn box_keeping_projects_in(projects: &Path) -> Machine {
    Machine {
        projects_dir: projects.display().to_string(),
        ..box_machine()
    }
}

fn focused_draft(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> PaneIdentity {
    view.read_with(cx, |view, cx| {
        let pane = &view.panes[view.focused(cx)];
        assert!(pane.draft().is_some(), "a draft is focused");
        pane.identity
    })
}

/// The Project `draft` works in.
fn draft_project(
    view: &Entity<CockpitView>,
    draft: PaneIdentity,
    cx: &mut gpui::VisualTestContext,
) -> ProjectKey {
    view.read_with(cx, |view, _| {
        let index = view.index_of(draft).expect("the draft's Pane");
        view.panes[index]
            .draft()
            .expect("still a draft")
            .binding
            .project()
    })
}

fn text_of(cx: &mut gpui::VisualTestContext, selector: &str) -> Option<String> {
    cx.update(|window, _| window.debug_text(selector))
        .map(|lines| lines.join(" "))
}

fn press(cx: &mut gpui::VisualTestContext, selector: &str) {
    let at = debug_bounds(cx, selector.to_string())
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .center();
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
}

/// The rows of the band menu `chip` opened, as the operator reads them:
/// each name, what it says beside it, its tick, and whether it is dimmed.
fn band_rows(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> Vec<(String, String, bool, bool)> {
    view.read_with(cx, |view, _| {
        let open = view.band_menu().expect("a band menu is up");
        open.rows
            .iter()
            .map(|row| {
                (
                    row.name.to_string(),
                    row.detail.to_string(),
                    row.active,
                    row.inert,
                )
            })
            .collect()
    })
}

/// Press `chip`, then its menu's row named `name`.
fn pick_on(view: &Entity<CockpitView>, chip: &str, name: &str, cx: &mut gpui::VisualTestContext) {
    press(cx, chip);
    let at = view.read_with(cx, |view, _| {
        let open = view.band_menu().expect("the chip's menu is up");
        open.rows
            .iter()
            .position(|row| row.name == name)
            .unwrap_or_else(|| panic!("a row named {name}"))
    });
    press(cx, &format!("composer-menu-row-{at}"));
}

const ENVIRONMENT: &str = "band-chip-2";

fn sheet(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> Option<Entity<PlacementView>> {
    view.read_with(cx, |view, cx| view.placement_sheet(cx))
}

/// The setup sheet's destination field, with the keyboard, holding `dest`.
fn aim_sheet_at(view: &Entity<CockpitView>, dest: &Path, cx: &mut gpui::VisualTestContext) {
    let field = sheet(view, cx)
        .expect("the setup sheet")
        .read_with(cx, |sheet, _| sheet.dest.field.clone());
    field.update(cx, |field, cx| field.set(dest.display().to_string(), cx));
    tick(cx);
}

/// The roots of box's Projects.
fn roots_on(
    view: &Entity<CockpitView>,
    machine: MachineId,
    cx: &mut gpui::VisualTestContext,
) -> Vec<PathBuf> {
    view.read_with(cx, |view, cx| {
        let cockpit = view.cockpit.read(cx);
        let registry = cockpit.registry(machine);
        registry
            .reading()
            .projects()
            .iter()
            .map(|project| project.root.clone())
            .collect()
    })
}

/// The chip names where the draft runs and lists every environment on,
/// the draft's ticked, then `Add environment…`, which opens Connections.
#[gpui::test]
fn the_environment_chip_lists_each_environment_with_the_drafts_ticked(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, _agents) = two_machines("env-rows", 0);
    let folder = settings_dir("env-rows").join("mine");
    std::fs::create_dir_all(&folder).unwrap();
    let mine = registered_here(&mut core, &folder);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    assert_eq!(text_of(cx, ENVIRONMENT).as_deref(), Some("Local"));
    press(cx, ENVIRONMENT);
    let row = |name: &str, active| (name.to_string(), String::new(), active, false);
    assert_eq!(
        band_rows(&view, cx),
        [
            row("Local", true),
            row("box", false),
            row("Add environment…", false)
        ]
    );
    press(cx, "composer-menu-row-2");
    view.read_with(cx, |view, cx| {
        let settings = view.settings_sheet(cx).expect("Settings is up");
        assert_eq!(settings.read(cx).page, settings::PageKey::Connections);
    });
}

/// A machine the window cannot reach is listed dimmed with why, and its
/// row opens Connections, where it is set right.
#[gpui::test]
fn a_machine_the_window_cannot_reach_is_dimmed_and_opens_connections(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("machines-env-unreached", 0);
    let machines = beside_box("env-unreached", &mut core, needs_sign_in);
    let on_box = machines.held(1).unwrap();
    let folder = settings_dir("env-unreached").join("mine");
    std::fs::create_dir_all(&folder).unwrap();
    let mine = registered_here(&mut core, &folder);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            view.cockpit.read(cx).server(on_box).is_some_and(|server| {
                matches!(
                    server,
                    ferrite_core::cockpit::ServerState::Detached {
                        attention: Some(_),
                        ..
                    }
                )
            })
        })
    });
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    press(cx, ENVIRONMENT);
    assert_eq!(
        band_rows(&view, cx)[1],
        ("box".into(), "sign in".into(), false, true)
    );
    press(cx, "composer-menu-row-1");
    view.read_with(cx, |view, cx| {
        let settings = view.settings_sheet(cx).expect("Settings is up");
        assert_eq!(settings.read(cx).page, settings::PageKey::Connections);
        assert!(view.placement_sheet(cx).is_none());
    });
}

/// box's Project of the same repository is the counterpart: the pick
/// moves the draft there with no sheet, its words, file, model and effort
/// kept, and its file uploaded to box. A draft on a Project of box with
/// none here gets the Project card here.
#[gpui::test]
fn picking_a_machine_moves_the_draft_to_its_projects_counterpart_with_all_it_holds(
    cx: &mut TestAppContext,
) {
    let (mut core, _fake, machines, _agents) = two_machines("env-counterpart", 0);
    let here_machine = core.this_computer().unwrap();
    let on_box = machines.held(1).unwrap();
    let base = settings_dir("env-counterpart");
    let remote = "https://github.com/acme/app.git";
    let ours = project_folder(&base.join("here/app"), Some(remote));
    let theirs = project_folder(&base.join("box/app"), Some(remote));
    let lone = base.join("box/lone");
    std::fs::create_dir_all(&lone).unwrap();
    let mine = registered(&mut core, here_machine, &ours);
    let lone = registered(&mut core, on_box, &lone);
    let theirs = registered(&mut core, on_box, &theirs);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    let shot = base.join("shot.png");
    std::fs::write(&shot, [0u8; 16]).unwrap();
    let codex = ferrite_core::cockpit::ProviderChoice {
        provider: Provider::Codex,
        model: None,
    };
    view.update(cx, |view, cx| {
        let index = view.index_of(draft).unwrap();
        let composer = view.panes[index].composer.clone();
        composer.update(cx, |composer, cx| {
            composer.set("Look at the build".into(), cx);
            composer.add_local_files(std::slice::from_ref(&shot), cx);
        });
        let binding = &mut view.panes[index].draft_mut().unwrap().binding;
        binding.choose_provider(codex.clone(), &[]);
        binding.choose_effort(Some("high".into()));
    });
    tick(cx);
    assert!(
        cx.debug_bounds("attachment-upload-progress").is_none(),
        "a file of this computer here"
    );

    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| draft_project(&view, draft, cx) != mine);
    assert_eq!(
        draft_project(&view, draft, cx),
        theirs,
        "the same repository on box"
    );
    assert!(sheet(&view, cx).is_none(), "no copy for a Project box has");
    assert_eq!(text_of(cx, ENVIRONMENT).as_deref(), Some("box"));
    view.read_with(cx, |view, cx| {
        let pane = &view.panes[view.index_of(draft).unwrap()];
        assert_eq!(pane.composer.read(cx).text(), "Look at the build");
        assert_eq!(pane.composer.read(cx).files(), std::slice::from_ref(&shot));
        let binding = &pane.draft().unwrap().binding;
        assert_eq!(binding.provider(), &codex);
        assert_eq!(binding.effort(), Some("high"));
    });
    until(cx, |_| {
        String::from_utf8_lossy(&SENT_TO_BOX.lock().unwrap())
            .contains(r#""op":"attachment","request":{"action":"begin","name":"shot.png""#)
    });

    view.update(cx, |view, cx| view.open_draft_in_project(lone, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "Local", cx);
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.project_card(cx).is_some())
    });
    view.read_with(cx, |view, cx| {
        let card = view.project_card(cx).unwrap();
        let card = card.read(cx);
        assert_eq!(card.machine, here_machine, "the card is this computer's");
        assert_eq!(
            card.draft,
            Some(draft),
            "and chooses what it makes on the draft"
        );
        assert!(card.remote.is_none(), "with the folder dialog");
        assert!(view.placement_sheet(cx).is_none());
    });
}

/// With none on box, the setup sheet copies the draft's Project there:
/// its destination starts in box's projects folder and can be changed,
/// what git ignores is listed unticked, and what is ticked goes too. The
/// copy is registered on box and chosen on the draft, its git reads it as
/// unchanged, and from then on each environment's Project is the other's
/// counterpart.
#[gpui::test]
fn a_project_with_none_there_is_copied_by_the_sheet_and_known_there_after(cx: &mut TestAppContext) {
    let base = settings_dir("env-copy");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_with("env-copy", 0, box_keeping_projects_in(&projects));
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    view.update(cx, |view, cx| {
        view.panes[view.index_of(draft).unwrap()]
            .composer
            .update(cx, |composer, cx| composer.set("hello".into(), cx));
    });

    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| {
        debug_bounds(cx, "placement-ignored:.env".into()).is_some()
    });
    let field = sheet(&view, cx)
        .unwrap()
        .read_with(cx, |sheet, _| sheet.dest.field.clone());
    assert_eq!(
        field.read_with(cx, |field, _| field.text().to_string()),
        format!("{}/app", projects.display()),
        "box's projects folder, then the Project's folder name"
    );
    assert!(debug_bounds(cx, "placement-ignored:target".into()).is_some());
    let dest = projects.join("copied");
    aim_sheet_at(&view, &dest, cx);
    press(cx, "placement-ignored:.env");
    press(cx, "placement-copy");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    tick(cx);
    assert!(
        sheet(&view, cx).is_none(),
        "it closes once the copy is chosen"
    );
    let copy = draft_project(&view, draft, cx);
    assert!(
        roots_on(&view, on_box, cx).contains(&dest),
        "registered on box"
    );
    assert!(dest.join("main.rs").is_file());
    assert!(dest.join(".git").is_dir());
    assert!(dest.join(".env").is_file(), "the ticked entry went");
    assert!(!dest.join("target").exists(), "the unticked one did not");
    assert_eq!(
        git(&dest, &["status", "--porcelain"]),
        "",
        "git there reads the copy as unchanged"
    );
    assert_eq!(text_of(cx, ENVIRONMENT).as_deref(), Some("box"));
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.panes[view.index_of(draft).unwrap()]
                .composer
                .read(cx)
                .text(),
            "hello"
        );
    });

    pick_on(&view, ENVIRONMENT, "Local", cx);
    until(cx, |cx| draft_project(&view, draft, cx) == mine);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| draft_project(&view, draft, cx) == copy);
    assert!(
        sheet(&view, cx).is_none(),
        "the copy is the counterpart now"
    );
}

/// A copy says how far it has got, and stopped part way registers
/// nothing and leaves the draft where it was; its retry finishes it.
#[gpui::test]
fn a_copy_cancelled_part_way_adds_nothing_and_its_retry_finishes_it(cx: &mut TestAppContext) {
    let base = settings_dir("env-cancel");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_with("env-cancel", 0, box_keeping_projects_in(&projects));
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let big = vec![9u8; 4 << 20];
    std::fs::write(source.join("big.bin"), &big).unwrap();
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| {
        debug_bounds(cx, "placement-ignored:.env".into()).is_some()
    });

    BOX_SLOW.store(true, Ordering::SeqCst);
    press(cx, "placement-copy");
    until(cx, |cx| {
        text_of(cx, "placement-progress").is_some_and(|said| said.starts_with("Copying "))
    });
    press(cx, "placement-cancel");
    BOX_SLOW.store(false, Ordering::SeqCst);
    assert_eq!(
        text_of(cx, "placement-failed").as_deref(),
        Some("Cancelled")
    );
    for _ in 0..4 {
        tick(cx);
    }
    assert_eq!(draft_project(&view, draft, cx), mine, "the draft stays");
    assert!(roots_on(&view, on_box, cx).is_empty(), "nothing registered");

    press(cx, "placement-retry");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    let dest = projects.join("app");
    assert_eq!(std::fs::read(dest.join("big.bin")).unwrap(), big);
}

/// A destination that holds something takes no copy: the sheet says so
/// and nothing there changes. Another path copies; `Use what is there`
/// registers it as it is.
#[gpui::test]
fn a_destination_that_holds_something_is_never_copied_over(cx: &mut TestAppContext) {
    let base = settings_dir("env-occupied");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_with("env-occupied", 0, box_keeping_projects_in(&projects));
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let there = projects.join("app");
    std::fs::create_dir_all(&there).unwrap();
    std::fs::write(there.join("keep.txt"), "mine").unwrap();
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| {
        debug_bounds(cx, "placement-ignored:.env".into()).is_some()
    });

    press(cx, "placement-copy");
    until(cx, |cx| {
        debug_bounds(cx, "placement-occupied".into()).is_some()
    });
    assert!(debug_bounds(cx, "placement-use".into()).is_some());
    let listed = |dir: &Path| {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    assert_eq!(listed(&there), ["keep.txt"], "nothing there is touched");

    aim_sheet_at(&view, &projects.join("other"), cx);
    assert!(
        debug_bounds(cx, "placement-occupied".into()).is_none(),
        "another path is chosen afresh"
    );
    assert!(debug_bounds(cx, "placement-copy".into()).is_some());
    aim_sheet_at(&view, &there, cx);
    press(cx, "placement-copy");
    until(cx, |cx| debug_bounds(cx, "placement-use".into()).is_some());
    press(cx, "placement-use");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    assert_eq!(roots_on(&view, on_box, cx), [there.clone()]);
    assert_eq!(listed(&there), ["keep.txt"], "used as it is");
}

/// A file another program holds while the copy runs is left behind, and
/// the sheet stays up to list it once the rest is chosen on the draft.
#[gpui::test]
fn a_file_in_use_while_copying_is_left_behind_and_listed(cx: &mut TestAppContext) {
    let base = settings_dir("env-busy");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_with("env-busy", 0, box_keeping_projects_in(&projects));
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let busy = source.join("server.log");
    std::fs::write(&busy, "running").unwrap();
    #[cfg(windows)]
    let _held = {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&busy)
            .unwrap()
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&busy, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&busy).is_ok() {
            // Root reads it anyway: nothing here is busy.
            return;
        }
    }
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| {
        debug_bounds(cx, "placement-ignored:.env".into()).is_some()
    });
    press(cx, "placement-copy");
    until(cx, |cx| {
        debug_bounds(cx, "placement-skipped".into()).is_some()
    });
    assert_eq!(
        text_of(cx, "placement-skipped").as_deref(),
        Some("server.log")
    );
    assert_eq!(
        draft_project(&view, draft, cx).machine(),
        on_box,
        "the rest is chosen"
    );
    assert!(projects.join("app/main.rs").is_file());
    press(cx, "placement-done");
    assert!(sheet(&view, cx).is_none());
}

/// A copy box cannot take says why, and once box is back its retry
/// copies.
#[gpui::test]
fn a_copy_that_fails_says_why_and_its_retry_copies(cx: &mut TestAppContext) {
    let base = settings_dir("env-failed");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_with("env-failed", 0, box_keeping_projects_in(&projects));
    let on_box = machines.held(1).unwrap();
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| {
        debug_bounds(cx, "placement-ignored:.env".into()).is_some()
    });

    box_down(true);
    until(cx, |cx| {
        view.read_with(cx, |view, cx| !attached(view.cockpit.read(cx), on_box))
    });
    press(cx, "placement-copy");
    until(cx, |cx| {
        debug_bounds(cx, "placement-failed".into()).is_some()
    });
    assert!(text_of(cx, "placement-failed").is_some_and(|said| said.starts_with("Not copied: ")));
    assert_eq!(draft_project(&view, draft, cx), mine);

    box_down(false);
    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.retry_server(on_box))
    });
    until(cx, |cx| {
        view.read_with(cx, |view, cx| attached(view.cockpit.read(cx), on_box))
    });
    press(cx, "placement-retry");
    until(cx, |cx| draft_project(&view, draft, cx).machine() == on_box);
    assert!(projects.join("app/main.rs").is_file());
}

/// A draft's `Add project…` opens the Project card on the draft's
/// machine, and the Project made there is chosen on the draft.
#[gpui::test]
fn add_project_on_a_draft_makes_it_on_the_drafts_machine(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake, machines, _agents) = two_machines("env-add-project", 0);
    let on_box = machines.held(1).unwrap();
    let base = settings_dir("env-add-project");
    let first = base.join("first");
    let second = base.join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let theirs = registered(&mut core, on_box, &first);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(theirs, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    pick_on(&view, "band-chip-0", "Add project…", cx);
    let field = view.read_with(cx, |view, cx| {
        let card = view.project_card(cx).expect("the Project card");
        let card = card.read(cx);
        assert_eq!(card.machine, on_box, "on the draft's machine");
        assert_eq!(card.draft, Some(draft));
        card.remote.as_ref().expect("its path field").field.clone()
    });
    cx.update(|window, cx| window.focus(&field.read(cx).focus_handle(cx), cx));
    field.update(cx, |field, cx| field.set(second.display().to_string(), cx));
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("enter");
    tick(cx);
    let made = draft_project(&view, draft, cx);
    assert_ne!(made, theirs);
    assert_eq!(made.machine(), on_box);
    let root = view.read_with(cx, |view, cx| {
        view.cockpit.read(cx).project(made).unwrap().root.clone()
    });
    assert_eq!(root, second);
}

/// A saved environment's projects folder is changed on its Connections
/// row, and a copy there then starts in it.
#[gpui::test]
fn a_machines_projects_folder_is_changed_on_its_connections_row(cx: &mut TestAppContext) {
    let base = settings_dir("env-projects-dir");
    let (mut core, _fake, machines, _agents) = two_machines("env-projects-dir", 0);
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    let edit = connection_act(&view, "connection-projects-1", cx);
    cx.update(|window, cx| edit(window, cx));
    let (input, save) = connection_groups(&view, cx)
        .into_iter()
        .flat_map(|group| group.rows)
        .find_map(|row| match row.control {
            settings::Control::ConnectionRow(row) => {
                row.projects.map(|edit| (edit.input, edit.save))
            }
            _ => None,
        })
        .expect("the row's projects folder field");
    assert_eq!(
        input.read_with(cx, |input, _| input.value().to_string()),
        "~/projects"
    );
    let work = base.join("work");
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_value(work.display().to_string(), window, cx)
        })
    });
    cx.update(|window, cx| save(window, cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.prefs.settings.machines[0].projects_dir,
            work.display().to_string()
        );
    });
    assert!(
        ferrite_core::settings::Settings::load(
            &view.read_with(cx, |view, _| view.prefs.dir.clone())
        )
        .is_ok_and(|saved| saved.machines[0].projects_dir == work.display().to_string()),
        "and saved"
    );

    view.update(cx, |view, cx| {
        view.hide_sheet(cx);
        view.open_draft_in_project(mine, cx);
    });
    tick(cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| sheet(&view, cx).is_some());
    let field = sheet(&view, cx)
        .unwrap()
        .read_with(cx, |sheet, _| sheet.dest.field.clone());
    assert_eq!(
        field.read_with(cx, |field, _| field.text().to_string()),
        format!("{}/app", work.display())
    );
}
