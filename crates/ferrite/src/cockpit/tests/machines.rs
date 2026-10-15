//! Machines in the window (ADR 0026), with fake bridges: no ssh runs. A
//! machine off screen lists its connection, the window goes to another
//! machine by building its Cockpit afresh, a machine that needs the
//! operator says so and stays dimmed and read-only, and a Project and its
//! first Thread are made on a machine in a directory typed there.

use super::*;
use crate::cockpit::server::Gone;
use crate::machines::{Connection, Machines, Reach};
use ferrite_core::machine::{self, Bridge, Failure, Machine, Tunnel};

/// A bridge whose every tunnel ends as ssh does after `failure`.
fn failing(machine: &Machine, settings_dir: &std::path::Path, failure: Failure) -> Bridge {
    Bridge::new(
        &machine.name,
        machine::dir(settings_dir, &machine.target).join("bridge.sock"),
        Box::new(move || {
            let failure = failure.clone();
            Ok(Tunnel {
                from: Box::new(std::io::empty()),
                to: Box::new(std::io::sink()),
                ended: Box::new(move || Some(failure)),
            })
        }),
    )
}

/// Ssh needs the operator to sign in.
fn needs_sign_in(machine: &Machine, settings_dir: &std::path::Path) -> Bridge {
    failing(machine, settings_dir, Failure::SignIn)
}

/// The machine's shell has no `ferrited`.
fn not_installed(machine: &Machine, settings_dir: &std::path::Path) -> Bridge {
    failing(machine, settings_dir, Failure::NotInstalled)
}

/// Every byte a window sent `box`'s Server through `serving`.
static SENT_TO_BOX: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());

/// `box`'s Server runs in this process (`server::serve_in_thread`), its
/// Store in `box` under the settings directory; each tunnel reaches it as
/// ssh would, keeping what the window sends in `SENT_TO_BOX`.
fn serving(machine: &Machine, settings_dir: &std::path::Path) -> Bridge {
    use ferrite_core::link::{self, TryClone};
    struct Kept(link::Stream);
    impl std::io::Write for Kept {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            SENT_TO_BOX.lock().unwrap().extend_from_slice(buf);
            self.0.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.flush()
        }
    }
    impl Drop for Kept {
        fn drop(&mut self) {
            let _ = self.0.shutdown();
        }
    }
    let address = link::address(&settings_dir.join("box"));
    Bridge::new(
        &machine.name,
        machine::dir(settings_dir, &machine.target).join("bridge.sock"),
        Box::new(move || {
            let stream = link::connect(&address)?;
            Ok(Tunnel {
                from: Box::new(stream.try_clone()?),
                to: Box::new(Kept(stream)),
                ended: Box::new(|| None),
            })
        }),
    )
}

/// `box`'s Sessions: each keeps where it was started, and says nothing.
struct OnBox(std::sync::Arc<std::sync::Mutex<Vec<std::path::PathBuf>>>);

struct Quiet {
    _alive: mpsc::Sender<SessionEvent>,
    events: mpsc::Receiver<SessionEvent>,
}

impl ferrite_core::engine::Spawner for OnBox {
    fn spawn(
        &mut self,
        request: ferrite_core::engine::SpawnRequest,
    ) -> std::io::Result<Box<dyn ferrite_core::providers::Session>> {
        self.0
            .lock()
            .unwrap()
            .extend(request.cwd.map(std::path::Path::to_path_buf));
        let (alive, events) = mpsc::channel();
        Ok(Box::new(Quiet {
            _alive: alive,
            events,
        }))
    }
}

impl ferrite_core::providers::Session for Quiet {
    fn events(&self) -> &mpsc::Receiver<SessionEvent> {
        &self.events
    }
    fn send(&mut self, _: &str) -> std::io::Result<()> {
        Ok(())
    }
    fn interrupt(&mut self) -> std::io::Result<()> {
        Ok(())
    }
    fn respond_to_decision(&mut self, _: &str, _: DecisionAnswer) -> std::io::Result<()> {
        Ok(())
    }
}

/// This computer, whose Server cannot start, and `box`, which needs a
/// sign-in; the window starts on `remote` when given.
fn machines(name: &str, remote: Option<&str>) -> Machines {
    machines_through(name, remote, needs_sign_in)
}

/// Where `machines_through(name, …)` keeps this computer's settings.
fn settings_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("fv-{}-{name}", std::process::id()))
}

fn machines_through(
    name: &str,
    remote: Option<&str>,
    bridge: fn(&Machine, &std::path::Path) -> Bridge,
) -> Machines {
    let dir = settings_dir(name);
    let _ = std::fs::remove_dir_all(&dir);
    Machines::new(
        Reach {
            settings_dir: dir.clone(),
            store: dir.join("threads"),
            executable: dir.join("no-such-ferrite"),
            client: ferrite_core::protocol::Peer {
                name: "test".into(),
                version: "0".into(),
            },
            renderer: None,
            defaults: Default::default(),
            bridge,
        },
        &[Machine {
            name: "box".into(),
            target: "me@box".into(),
            enabled: true,
        }],
        remote,
    )
}

/// A window as main opens one: the Cockpit inside its `CockpitWindow`,
/// which builds it afresh on another machine.
fn machine_window(
    cx: &mut TestAppContext,
    core: Cockpit,
    machines: Entity<Machines>,
) -> (Entity<gpui::component::Root>, &mut gpui::VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| CockpitView::new(core, cx).with_machines(machines));
        gpui::component::Root::new(
            cx.new(|cx| crate::loops_overlay::CockpitWindow::new(view, window, cx)),
            window,
            cx,
        )
    })
}

/// The Cockpit the window shows now.
fn shown(
    root: &Entity<gpui::component::Root>,
    cx: &mut gpui::VisualTestContext,
) -> Entity<CockpitView> {
    root.read_with(cx, |root, cx| {
        crate::loops_overlay::testing::cockpit(root.view(), cx)
    })
}

/// Tick until `done`, giving the links' threads real time.
fn until(
    cx: &mut gpui::VisualTestContext,
    mut done: impl FnMut(&mut gpui::VisualTestContext) -> bool,
) {
    for _ in 0..300 {
        tick(cx);
        if done(cx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("never happened");
}

fn connection(
    view: &Entity<CockpitView>,
    index: usize,
    cx: &mut gpui::VisualTestContext,
) -> Connection {
    view.read_with(cx, |view, cx| {
        view.machine_rows(cx).unwrap()[index].connection
    })
}

#[gpui::test]
fn a_machine_off_screen_heads_its_rows_with_its_connection(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("machines-listed", 1);
    let machines = cx.new(|_| machines("listed", None));
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new(core, cx).with_machines(machines)
    });
    tick(cx);
    assert!(
        debug_bounds(cx, "nav-machine-0".into()).is_some(),
        "this computer"
    );
    assert!(debug_bounds(cx, "nav-machine-1".into()).is_some(), "box");
    until(cx, |cx| connection(&view, 1, cx) == Connection::Attention);
    assert_eq!(connection(&view, 0, cx), Connection::Online);
}

#[gpui::test]
fn a_window_with_only_this_computer_lists_no_machines(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("machines-none", 1);
    let dir = std::env::temp_dir().join(format!("fv-{}-none", std::process::id()));
    let alone = cx.new(|_| {
        Machines::new(
            Reach {
                settings_dir: dir.clone(),
                store: dir.join("threads"),
                executable: dir.join("no-such-ferrite"),
                client: ferrite_core::protocol::Peer {
                    name: "test".into(),
                    version: "0".into(),
                },
                renderer: None,
                defaults: Default::default(),
                bridge: needs_sign_in,
            },
            &[],
            None,
        )
    });
    let (_view, cx) =
        add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx).with_machines(alone));
    tick(cx);
    assert!(debug_bounds(cx, "nav-machine-0".into()).is_none());
}

/// Going to a machine builds the window's Cockpit afresh on it. It needs
/// a sign-in: the band offers one, and the board stays dimmed and takes
/// no press until the machine is back.
#[gpui::test]
fn going_to_a_machine_that_needs_a_sign_in_says_so_and_holds_still(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("machines-go", 1);
    let machines = cx.new(|_| machines("go", None));
    let (root, cx) = machine_window(cx, core, machines);
    let shown = |cx: &mut gpui::VisualTestContext| shown(&root, cx);
    let first = shown(cx);
    first.update(cx, |_, cx| cx.emit(GoToMachine(1)));
    tick(cx);
    let on_box = shown(cx);
    assert_ne!(on_box.entity_id(), first.entity_id(), "built afresh");
    on_box.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.machine().map(|bridge| bridge.name()),
            Some("box")
        );
        assert!(view.panes.is_empty(), "no draft before the machine answers");
    });
    until(cx, |cx| {
        on_box.read_with(cx, |view, _| {
            matches!(
                view.cockpit.server(),
                Some(ferrite_core::cockpit::ServerState::Detached {
                    attention: Some(Failure::SignIn),
                    ..
                })
            )
        })
    });
    tick(cx);
    assert!(debug_bounds(cx, "machine-lost".into()).is_some());
    cx.update(|window, cx| {
        use gpui::component::WindowExt;
        assert_eq!(window.notifications(cx).len(), 1, "the sign-in band");
    });
    on_box.read_with(cx, |view, _| {
        assert_eq!(view.server_bands.detached(), Some(Gone::SignIn));
    });
    assert_eq!(connection(&on_box, 1, cx), Connection::Attention);
    on_box.read_with(cx, |view, cx| {
        let rows = view.machine_rows(cx).unwrap();
        assert!(rows[1].selected);
        assert!(!rows[0].selected, "this computer streams status now");
    });

    // And back: the window follows the Cockpit it built last.
    on_box.update(cx, |_, cx| cx.emit(GoToMachine(0)));
    tick(cx);
    let home = shown(cx);
    assert_ne!(home.entity_id(), on_box.entity_id(), "built afresh again");
    home.read_with(cx, |view, cx| {
        assert!(view.cockpit.machine().is_none(), "this computer");
        assert!(view.machine_rows(cx).unwrap()[0].selected);
        assert_eq!(
            view.server_bands.detached(),
            Some(Gone::NoServer),
            "this computer's Server cannot start"
        );
    });
    on_box.read_with(cx, |view, _| {
        assert_eq!(
            view.server_bands.detached(),
            None,
            "box's band went with it"
        );
    });
    cx.update(|window, cx| {
        use gpui::component::WindowExt;
        assert_eq!(
            window.notifications(cx).len(),
            1,
            "box's sign-in band stays with box"
        );
    });
}

/// A machine with no `ferrited` says so, and while it is away its board,
/// its tree and the keyboard are held: the root keeps the keys, so no
/// Composer or answer reaches it.
#[gpui::test]
fn a_machine_without_ferrited_says_so_and_takes_no_input(cx: &mut TestAppContext) {
    let machines = cx.new(|_| machines_through("missing", Some("me@box"), not_installed));
    let core = machines
        .update(cx, |machines, _| machines.cockpit())
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new(core, cx).with_machines(machines)
    });
    until(cx, |cx| {
        view.read_with(cx, |view, _| {
            matches!(
                view.cockpit.server(),
                Some(ferrite_core::cockpit::ServerState::Detached {
                    attention: Some(Failure::NotInstalled),
                    ..
                })
            )
        })
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.server_bands.detached(), Some(Gone::NotInstalled));
    });
    assert!(
        debug_bounds(cx, "machine-lost".into()).is_some(),
        "the board"
    );
    assert!(
        debug_bounds(cx, "machine-lost-tree".into()).is_some(),
        "its tree in the nav"
    );
    cx.update(|window, cx| {
        assert!(
            view.read(cx).focus.is_focused(window),
            "the root holds the keyboard"
        );
    });
}

/// Turning off the machine the window is on puts the window on this
/// computer: a machine turned off is never connected to.
#[gpui::test]
fn turning_off_the_machine_on_screen_goes_home(cx: &mut TestAppContext) {
    let machines = cx.new(|_| machines("off", Some("me@box")));
    let core = machines
        .update(cx, |machines, _| machines.cockpit())
        .unwrap();
    let (root, cx) = machine_window(cx, core, machines);
    let on_box = shown(&root, cx);
    on_box.update(cx, |view, cx| {
        view.prefs.settings.machines = vec![Machine {
            name: "box".into(),
            target: "me@box".into(),
            enabled: false,
        }];
        view.machines_changed(cx);
    });
    tick(cx);
    let home = shown(&root, cx);
    assert_ne!(home.entity_id(), on_box.entity_id(), "built afresh");
    home.read_with(cx, |view, cx| {
        assert!(view.cockpit.machine().is_none(), "this computer");
        assert_eq!(
            view.machine_rows(cx).unwrap()[1].connection,
            Connection::Disabled
        );
    });
}

/// On a machine the Project card has no folder dialog, which could only
/// browse this computer: a path field, listing the machine's home from
/// the start, completes against the machine's folders through its Server
/// (`list_dir`), Tab taking the first, ↵ adding it and the next ↵
/// confirming. The Project it makes is the machine's; a draft chooses it,
/// its workspace chip names and offers what git on the machine lists
/// (`list_repo`) — no git here is asked — and its first send starts the
/// Thread in that directory on the machine.
#[gpui::test]
fn a_draft_on_a_machine_starts_a_thread_in_a_directory_typed_there(cx: &mut TestAppContext) {
    cx.update(|cx| crate::bind_keys(crate::keymap::bindings(crate::keymap::PLATFORM), cx));
    let machines = cx.new(|_| machines_through("typed", Some("me@box"), serving));
    let folder = settings_dir("typed").join("src");
    let repo = repo_with_worktree_and_branches(&folder);
    std::fs::create_dir_all(folder.join(".re-hidden")).unwrap();
    std::fs::write(folder.join("readme.txt"), "").unwrap();
    let started = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    ferrite_core::server::serve_in_thread(
        settings_dir("typed").join("box"),
        Box::new(OnBox(started.clone())),
    )
    .unwrap();
    let core = machines
        .update(cx, |machines, _| machines.cockpit())
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new(core, cx).with_machines(machines)
    });
    until(cx, |cx| {
        view.read_with(cx, |view, _| {
            matches!(
                view.cockpit.server(),
                Some(ferrite_core::cockpit::ServerState::Attached { .. })
            )
        })
    });

    let field = view.update(cx, |view, cx| {
        view.open_project_creator(cx);
        let field = view.project_editor.as_ref().unwrap().remote.as_ref();
        field.expect("a path field on a machine").field.clone()
    });
    assert!(
        String::from_utf8_lossy(&SENT_TO_BOX.lock().unwrap())
            .contains(r#""op":"list_dir","path":"~/""#),
        "the card lists the machine's home as it opens"
    );
    cx.update(|window, cx| window.focus(&field.read(cx).focus_handle(cx), cx));
    field.update(cx, |field, cx| {
        field.set(format!("{}/re", folder.display()), cx)
    });
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.remote_completions(cx) == ["repo"])
    });
    tick(cx);
    assert!(debug_bounds(cx, "project-path-completion:repo".into()).is_some());
    assert!(
        debug_bounds(cx, "project-add-directory".into()).is_none(),
        "no folder dialog for a machine"
    );
    assert!(
        String::from_utf8_lossy(&SENT_TO_BOX.lock().unwrap()).contains(r#""op":"list_dir""#),
        "the machine's Server listed it"
    );

    cx.simulate_keystrokes("tab");
    assert_eq!(
        field.read_with(cx, |field, _| field.text().to_string()),
        format!("{}/repo/", folder.display())
    );
    cx.simulate_keystrokes("enter");
    view.update(cx, |view, cx| {
        assert_eq!(view.project_editor.as_ref().unwrap().staged, [repo.clone()]);
        assert_eq!(field.read(cx).text(), "", "the field empties");
    });
    cx.simulate_keystrokes("enter");
    let project = view.update(cx, |view, _| {
        assert!(
            view.project_editor.is_none(),
            "the next ↵ confirms the card, adding nothing more"
        );
        let project = view.cockpit.registry().projects().last().unwrap().clone();
        assert_eq!(project.title, "repo");
        assert_eq!(project.root, std::fs::canonicalize(&repo).unwrap());
        project.id
    });

    let composer = view.update(cx, |view, cx| {
        if view.panes.is_empty() {
            view.open_draft(DraftTarget::Main, cx);
        }
        let draft = view.panes[view.focused()].draft().unwrap();
        let offered = view
            .band_rows(draft, pane::BandChip::Project, cx)
            .into_iter()
            .any(|row| row.consequence == Consequence::Band(BandChoice::Project(project)));
        assert!(offered, "the machine's Project is on the draft's chip");
        let composer = view.panes[view.focused()].composer.clone();
        view.pick_band(&BandChoice::Project(project), &composer, cx);
        composer
    });
    until(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.checked_out
                .borrow()
                .get(&project)
                .map(|branch| branch.as_ref())
                == Some("trunk")
        })
    });
    view.update(cx, |view, cx| {
        assert!(
            view.checking_out.borrow().is_empty(),
            "no git here was asked what the machine's checkout is on"
        );
        view.open_band_popover(pane::BandChip::Workspace, cx);
    });
    let rows = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.popover.as_ref().map_or_else(Vec::new, |popover| {
                popover
                    .rows
                    .iter()
                    .map(|row| (row.name.to_string(), row.detail.to_string()))
                    .collect::<Vec<_>>()
            })
        })
    };
    until(cx, |cx| rows(cx).len() > 3);
    let named = |name: &str, detail: &str| (name.to_string(), detail.to_string());
    assert_eq!(
        rows(cx),
        [
            named("trunk", "checked out"),
            named("feature/wt", "worktree · wt-feature"),
            named("main", "existing branch"),
            named("topic/spare", "existing branch"),
            named("New branch", "in the project checkout"),
            named("New branch + worktree", "in an isolated checkout"),
        ],
        "git on the machine lists the repo, its own worktrees and branches"
    );
    assert!(
        String::from_utf8_lossy(&SENT_TO_BOX.lock().unwrap()).contains(r#""op":"list_repo""#),
        "the machine's Server read the repo"
    );

    view.update(cx, |view, cx| {
        view.popover = None;
        composer.update(cx, |composer, cx| composer.set("hello box".into(), cx));
        view.bootstrap_draft(cx);
    });
    until(cx, |cx| {
        view.read_with(cx, |view, _| view.cockpit.threads().len() == 1)
    });
    assert_eq!(
        *started.lock().unwrap(),
        [std::fs::canonicalize(&repo).unwrap()]
    );
}
