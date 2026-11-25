//! A draft's environment chip (ADR 0048): where a new chat runs, chosen
//! until its first send. A pick moves the draft to its Project's
//! counterpart on the machine picked, the Project there of the same
//! repository or the one the setup sheet copied it to; a Project of this
//! computer with none there is copied by the sheet, and one of a machine
//! gets the Project card. What the draft holds goes with it.

use super::*;
use ferrite_settings::connections::ConnectionsEvent;
use crate::cockpit::placement_view::PlacementView;
use std::path::{Path, PathBuf};

mod folder_browser;
mod setup_sheet;

/// `settings_dir(name)` spelled as the registry keeps what it registers
/// there, its links resolved: macOS reaches `/tmp` through one.
pub(super) fn resolved_dir(name: &str) -> PathBuf {
    let temp = std::env::temp_dir();
    let dir = settings_dir(name);
    dunce::canonicalize(&temp).unwrap().join(dir.strip_prefix(&temp).unwrap())
}

/// `root` registered as a Project of `machine`, once its Server says so.
pub(super) fn registered(core: &mut Cockpit, machine: MachineId, root: &Path) -> ProjectKey {
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

/// `two_machines`, box keeping its projects in `projects`, which is there:
/// the sheet's browser opens in it.
fn two_machines_keeping_projects_in(
    name: &str,
    projects: &Path,
) -> (Cockpit, Fake, Machines, Agents) {
    let machine = Machine {
        projects_dir: projects.display().to_string(),
        ..box_machine()
    };
    let machines = two_machines_with(name, 0, machine);
    std::fs::create_dir_all(projects).unwrap();
    machines
}

pub(super) fn focused_draft(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> PaneIdentity {
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

/// Press the centre of what `selector` drew. A setup card up is let come
/// to its size first: it takes the height of what it shows a frame after
/// that changes, its controls moving with it, so a press aimed at the
/// frame before lands where they were.
fn press(cx: &mut gpui::VisualTestContext, selector: &str) {
    if debug_bounds(cx, "placement-card".into()).is_some() {
        settles(cx);
    }
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

/// Press `chip`, then its menu's row named `name`. What moves the band
/// (an upload's progress row going) must have landed first: a press goes
/// where the chip was last drawn.
fn pick_on(view: &Entity<CockpitView>, chip: &str, name: &str, cx: &mut gpui::VisualTestContext) {
    press(cx, chip);
    until(cx, |cx| view.read_with(cx, |view, _| view.band_menu().is_some()));
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

/// The setup sheet's browser line, as typed there.
fn sheet_line(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Entity<Composer> {
    sheet(view, cx)
        .expect("the setup sheet")
        .read_with(cx, |sheet, cx| sheet.dest.read(cx).field.clone())
}

fn text_in(field: &Entity<Composer>, cx: &mut gpui::VisualTestContext) -> String {
    field.read_with(cx, |field, _| field.text().to_string())
}

/// Where the setup sheet copies to.
fn destination(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> String {
    sheet(view, cx)
        .expect("the setup sheet")
        .read_with(cx, |sheet, cx| sheet.destination(cx))
}

/// The setup sheet aimed at `dest` by typing: its folder on the browser's
/// line, its name in the name field.
fn aim_sheet_at(view: &Entity<CockpitView>, dest: &Path, cx: &mut gpui::VisualTestContext) {
    let (line, name) = sheet(view, cx)
        .expect("the setup sheet")
        .read_with(cx, |sheet, cx| {
            (sheet.dest.read(cx).field.clone(), sheet.name.clone())
        });
    let folder = format!("{}/", dest.parent().unwrap().display());
    let leaf = dest.file_name().unwrap().to_string_lossy().into_owned();
    line.update(cx, |line, cx| line.set(folder, cx));
    name.update(cx, |name, cx| name.set(leaf, cx));
    tick(cx);
}

/// The setup sheet's folder browser opened, as Change… opens it.
fn browse(cx: &mut gpui::VisualTestContext) {
    drawn(cx, "placement-change");
    press(cx, "placement-change");
}

/// `selector` drawn, once box's Server answers what it waits on.
#[track_caller]
fn drawn(cx: &mut gpui::VisualTestContext, selector: &str) {
    until(cx, |cx| debug_bounds(cx, selector.to_string()).is_some());
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
    let folder = resolved_dir("env-rows").join("mine");
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
        assert_eq!(settings.read(cx).page(), settings::PageKey::Connections);
    });
}

/// A machine the window cannot reach is listed dimmed with why, and its
/// row opens Connections, where it is set right.
#[gpui::test]
fn a_machine_the_window_cannot_reach_is_dimmed_and_opens_connections(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("machines-env-unreached", 0);
    let machines = beside_box("env-unreached", &mut core, needs_sign_in);
    let on_box = machines.held(1).unwrap();
    let folder = resolved_dir("env-unreached").join("mine");
    std::fs::create_dir_all(&folder).unwrap();
    let mine = registered_here(&mut core, &folder);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            view.cockpit.read(cx).server(on_box).is_some_and(|server| {
                matches!(
                    server,
                    ferrite_client::cockpit::ServerState::Detached {
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
        assert_eq!(settings.read(cx).page(), settings::PageKey::Connections);
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
    let base = resolved_dir("env-counterpart");
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
    let codex = ferrite_client::cockpit::ProviderChoice {
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
    // Its progress row goes once box has it, moving the band under it.
    until(cx, |cx| cx.debug_bounds("attachment-upload-progress").is_none());

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

/// A draft's `Add project…` opens the Project card on the draft's
/// machine, and the Project made there is chosen on the draft. The card
/// holds its size as a directory is added to it.
#[gpui::test]
fn add_project_on_a_draft_makes_it_on_the_drafts_machine(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake, machines, _agents) = two_machines("env-add-project", 0);
    let on_box = machines.held(1).unwrap();
    let base = resolved_dir("env-add-project");
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
        let browser = card.remote.as_ref().expect("its folder browser");
        browser.read(cx).field.clone()
    });
    field.update(cx, |field, cx| {
        field.set(format!("{}/", base.display()), cx)
    });
    drawn(cx, "folder-row:second");
    settles(cx);
    let card_size = |cx: &mut gpui::VisualTestContext| {
        debug_bounds(cx, "project-editor-card".into()).expect("the card").size
    };
    let before = card_size(cx);
    press(cx, "folder-row:second");
    press(cx, "project-add-shown");
    view.read_with(cx, |view, cx| {
        let card = view.project_card(cx).unwrap();
        assert_eq!(card.read(cx).staged, std::slice::from_ref(&second));
    });
    settles(cx);
    assert_eq!(card_size(cx), before, "the card holds its size");
    assert_eq!(
        field.read_with(cx, |field, _| field.text().to_string()),
        format!("{}/", base.display()),
        "back in the folder it is in"
    );
    cx.update(|window, cx| window.focus(&field.read(cx).focus_handle(cx), cx));
    cx.simulate_keystrokes("enter");
    until(cx, |cx| draft_project(&view, draft, cx) != theirs);
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
    let base = resolved_dir("env-projects-dir");
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
    std::fs::create_dir_all(&work).unwrap();
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
    assert_eq!(destination(&view, cx), format!("{}/app", work.display()));
}

/// Set up again, a saved environment keeps its projects folder, and where
/// its Projects were copied goes with it to its new address.
#[gpui::test]
fn an_environment_set_up_again_keeps_its_projects_folder_and_copies(cx: &mut TestAppContext) {
    let base = resolved_dir("env-set-up-again");
    let projects = base.join("projects");
    let (core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-set-up-again", &projects);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    let settings = view.read_with(cx, |view, _| view.prefs.dir.clone());
    let old = box_machine().target;
    let placed = ferrite_core::placement::Placement {
        local_root: base.join("here/notes"),
        remote_root: projects.join("notes"),
    };
    ferrite_core::placement::place(&ferrite_core::machine::dir(&settings, &old), placed.clone()).unwrap();

    let connections = connections_view(&view, cx);
    let again = Machine {
        name: "box".into(),
        ..Machine::new("me@box.example")
    };
    view.update(cx, |view, cx| {
        view.on_connections_event(
            connections,
            &ConnectionsEvent::Ready {
                machine: again.clone(),
                replacing: Some(old.clone()),
            },
            cx,
        )
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        let saved = &view.prefs.settings.machines[0];
        assert_eq!(saved.target, again.target);
        assert_eq!(saved.projects_dir, projects.display().to_string(), "its own folder kept");
    });
    assert_eq!(
        ferrite_core::placement::placements(&ferrite_core::machine::dir(&settings, &again.target)),
        [placed]
    );
    assert_eq!(
        ferrite_core::placement::placements(&ferrite_core::machine::dir(&settings, &old)),
        []
    );
}

/// A draft whose environment is turned off goes with it, and Connections
/// says so; turned on again, the draft comes back with its words.
#[gpui::test]
fn a_draft_put_away_with_its_environment_comes_back_with_it(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, _agents) = two_machines("env-put-away", 0);
    let on_box = machines.held(1).unwrap();
    let folder = resolved_dir("env-put-away").join("theirs");
    std::fs::create_dir_all(&folder).unwrap();
    let theirs = registered(&mut core, on_box, &folder);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(theirs, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    view.update(cx, |view, cx| {
        view.panes[view.index_of(draft).unwrap()]
            .composer
            .update(cx, |composer, cx| composer.set("keep these words".into(), cx));
    });
    tick(cx);
    let drafts_saying = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.panes
                .iter()
                .filter(|pane| pane.draft().is_some())
                .filter(|pane| pane.composer.read(cx).text() == "keep these words")
                .count()
        })
    };
    let toggle = |cx: &mut gpui::VisualTestContext| {
        let connections = connections_view(&view, cx);
        view.update(cx, |view, cx| {
            view.on_connections_event(connections, &ConnectionsEvent::Toggle(box_machine().target), cx)
        });
        tick(cx);
    };

    toggle(cx);
    assert_eq!(drafts_saying(cx), 0, "put away with box");
    let warned = connection_groups(&view, cx)
        .into_iter()
        .filter_map(|group| group.note)
        .any(|note| note == "Your draft on box is put away until box is on again.");
    assert!(warned, "Connections says where the draft went");

    toggle(cx);
    until(cx, |cx| drafts_saying(cx) == 1);
}

/// A draft's file of this computer is kept by its path here through its
/// Machine going off and on: back on box it is box's copy, not sent again,
/// and picked onto this computer it is sent from here, never looked for at
/// box's copy of it.
#[gpui::test]
fn a_drafts_file_of_this_computer_goes_either_way_after_its_machine_comes_back(
    cx: &mut TestAppContext,
) {
    let (mut core, _fake, machines, _agents) = two_machines("env-file-back", 0);
    let here_machine = core.this_computer().unwrap();
    let on_box = machines.held(1).unwrap();
    let base = resolved_dir("env-file-back");
    let remote = "https://github.com/acme/app.git";
    let ours = project_folder(&base.join("here/app"), Some(remote));
    let theirs = project_folder(&base.join("box/app"), Some(remote));
    let mine = registered(&mut core, here_machine, &ours);
    let theirs = registered(&mut core, on_box, &theirs);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    let notes = base.join("notes.md");
    std::fs::write(&notes, "notes").unwrap();
    view.update(cx, |view, cx| {
        view.panes[view.index_of(draft).unwrap()].composer.update(cx, |composer, cx| {
            composer.set("Read the notes".into(), cx);
            composer.add_local_files(std::slice::from_ref(&notes), cx);
        });
    });
    tick(cx);
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| draft_project(&view, draft, cx) == theirs);
    let uploads = || {
        String::from_utf8_lossy(&SENT_TO_BOX.lock().unwrap())
            .matches(r#""op":"attachment","request":{"action":"begin","name":"notes.md""#)
            .count()
    };
    until(cx, |_| uploads() == 1);
    until(cx, |cx| cx.debug_bounds("attachment-upload-progress").is_none());

    let toggle = |cx: &mut gpui::VisualTestContext| {
        let connections = connections_view(&view, cx);
        view.update(cx, |view, cx| {
            view.on_connections_event(connections, &ConnectionsEvent::Toggle(box_machine().target), cx)
        });
        tick(cx);
    };
    toggle(cx);
    toggle(cx);
    let back = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.panes
                .iter()
                .position(|pane| pane.draft().is_some() && pane.composer.read(cx).text() == "Read the notes")
        })
    };
    until(cx, |cx| back(cx).is_some());
    let index = back(cx).unwrap();
    let draft = view.update(cx, |view, cx| {
        view.focus_pane(index, cx);
        view.panes[index].identity
    });
    tick(cx);
    assert_ne!(draft_project(&view, draft, cx).machine(), here_machine, "back on box");
    let composer = view.read_with(cx, |view, _| view.panes[view.index_of(draft).unwrap()].composer.clone());
    assert!(!composer.update(cx, |composer, cx| composer.has_blocked_file(cx)), "box has it");
    assert_eq!(uploads(), 1, "not sent again");
    view.update(cx, |view, cx| view.hide_sheet(cx));
    settles(cx);

    pick_on(&view, ENVIRONMENT, "Local", cx);
    until(cx, |cx| draft_project(&view, draft, cx) == mine);
    assert_eq!(composer.read_with(cx, |composer, _| composer.files().to_vec()), [notes.clone()]);
    assert!(
        !composer.update(cx, |composer, cx| composer.has_blocked_file(cx)),
        "sent from here"
    );
    tick(cx);
    assert!(debug_bounds(cx, "tray-file-0-retry".into()).is_none());
}

/// Tab walks a draft's files, then its band: one control holds the ring
/// at a time, never a file and a chip at once.
#[gpui::test]
fn tab_rings_one_control_of_a_draft_at_a_time(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake, machines, _agents) = two_machines("env-one-ring", 0);
    let base = resolved_dir("env-one-ring");
    let folder = base.join("mine");
    std::fs::create_dir_all(&folder).unwrap();
    let note = base.join("note.txt");
    std::fs::write(&note, "note").unwrap();
    let mine = registered_here(&mut core, &folder);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    let composer = view.read_with(cx, |view, _| view.panes[view.index_of(draft).unwrap()].composer.clone());
    composer.update(cx, |composer, cx| {
        composer.set("look".into(), cx);
        composer.add_local_files(std::slice::from_ref(&note), cx);
    });
    cx.update(|window, cx| window.focus(&composer.read(cx).focus_handle(cx), cx));
    tick(cx);
    let mut chips = 0;
    for _ in 0..8 {
        cx.simulate_keystrokes("tab");
        tick(cx);
        let (on_file, chip) = cx.update(|window, cx| {
            let view = view.read(cx);
            let pane = &view.panes[view.index_of(draft).unwrap()];
            let composer = pane.composer.read(cx);
            let on_file = composer.focus_target(window, cx) != composer.focus_handle(cx);
            (on_file, pane.draft().unwrap().band_focus)
        });
        assert!(!(on_file && chip.is_some()), "a file and {chip:?} ringed at once");
        chips += usize::from(chip.is_some());
    }
    assert!(chips > 0, "the band was reached");
}

/// A sheet coming up over a draft takes the ring off its band.
#[gpui::test]
fn a_sheet_coming_up_takes_the_ring_off_the_band(cx: &mut TestAppContext) {
    let base = resolved_dir("env-ring-sheet");
    let projects = base.join("projects");
    let (mut core, _fake, machines, _agents) =
        two_machines_keeping_projects_in("env-ring-sheet", &projects);
    let source = project_folder(&base.join("here/app"), None);
    let mine = registered_here(&mut core, &source);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    let ring = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.panes[view.index_of(draft).unwrap()].draft().unwrap().band_focus)
    };
    view.update(cx, |view, cx| {
        let index = view.index_of(draft).unwrap();
        view.panes[index].draft_mut().unwrap().band_focus = Some(pane::BandChip::Environment);
        cx.notify();
    });
    pick_on(&view, ENVIRONMENT, "box", cx);
    until(cx, |cx| sheet(&view, cx).is_some());
    assert_eq!(ring(cx), None);
}

/// Send sends, even while a chip of the draft's band holds the ring: it
/// opens no chip's menu.
#[gpui::test]
fn send_sends_while_a_band_chip_holds_the_ring(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake, machines, _agents) = two_machines("env-send-ring", 0);
    let folder = resolved_dir("env-send-ring").join("mine");
    std::fs::create_dir_all(&folder).unwrap();
    let mine = registered_here(&mut core, &folder);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    let draft = focused_draft(&view, cx);
    let composer = view.read_with(cx, |view, _| view.panes[view.index_of(draft).unwrap()].composer.clone());
    composer.update(cx, |composer, cx| composer.set("hello".into(), cx));
    cx.update(|window, cx| window.focus(&composer.read(cx).focus_handle(cx), cx));
    tick(cx);
    cx.simulate_keystrokes("tab");
    tick(cx);
    let ringed = view.read_with(cx, |view, _| {
        view.panes[view.index_of(draft).unwrap()].draft().unwrap().band_focus
    });
    assert!(ringed.is_some(), "a chip holds the ring");
    press(cx, "composer-send");
    assert!(view.read_with(cx, |view, _| view.band_menu().is_none()), "no chip's menu");
    until(cx, |cx| view.read_with(cx, |view, _| view.index_of(draft).is_none()));
}
