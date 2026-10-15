//! Machines in the window (ADR 0026), with fake bridges: no ssh runs. A
//! machine off screen lists its connection, the window goes to another
//! machine by building its Cockpit afresh, and a machine that needs the
//! operator says so and stays dimmed and read-only.

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

/// This computer, whose Server cannot start, and `box`, which needs a
/// sign-in; the window starts on `remote` when given.
fn machines(name: &str, remote: Option<&str>) -> Machines {
    machines_through(name, remote, needs_sign_in)
}

fn machines_through(
    name: &str,
    remote: Option<&str>,
    bridge: fn(&Machine, &std::path::Path) -> Bridge,
) -> Machines {
    let dir = std::env::temp_dir().join(format!("fv-{}-{name}", std::process::id()));
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

/// A Session that never speaks: what the machine's Server revives onto.
struct Quiet {
    events: std::sync::mpsc::Receiver<SessionEvent>,
    _provider: std::sync::mpsc::Sender<SessionEvent>,
}

impl Session for Quiet {
    fn events(&self) -> &std::sync::mpsc::Receiver<SessionEvent> {
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

struct Quietly;

impl Spawner for Quietly {
    fn spawn(
        &mut self,
        _: ferrite_core::cockpit::SpawnRequest,
    ) -> std::io::Result<Box<dyn Session>> {
        let (provider, events) = std::sync::mpsc::channel();
        Ok(Box::new(Quiet {
            events,
            _provider: provider,
        }))
    }
}

/// A machine's parked Threads are its Server's to list: its logs stay on
/// it. They show in the nav's Parked section, called what the machine's
/// log calls them, and a click revives one there.
#[gpui::test]
fn a_machines_parked_threads_show_in_the_nav_and_revive_on_click(cx: &mut TestAppContext) {
    use ferrite_core::cockpit::{Attach, Endpoint, ServerState};
    let dir = scratch("machine-parked");
    let dial = ferrite_core::server::fixture_machine(dir.join("box"), Box::new(Quietly)).unwrap();
    let mut core = Cockpit::attach(Attach {
        store: dir.join("window").join("threads"),
        server: Endpoint::Machine(std::sync::Arc::new(Bridge::new(
            "box",
            dir.join("bridge").join("bridge.sock"),
            dial,
        ))),
        client: ferrite_core::protocol::Peer {
            name: "test".into(),
            version: "0".into(),
        },
        renderer: None,
        discovery: Box::new(Fake::default()),
    })
    .unwrap();
    for _ in 0..500 {
        core.pump();
        if matches!(core.server(), Some(ServerState::Attached { .. })) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let named = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    let other = core
        .open(Provider::Codex, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    core.rename_thread(named, "Release notes").unwrap();
    core.park(named).unwrap();
    core.park(other).unwrap();

    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    until(cx, |cx| {
        view.read_with(cx, |view, _| view.facts.parked().len() == 2)
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.facts.parked(), &[named, other], "in creation order");
        assert_eq!(view.facts.name(named).as_ref(), "Release notes");
        assert!(view.panes.is_empty(), "nothing open on the machine");
    });
    view.read_with(cx, |view, _| {
        use ferrite_core::groups::{Drag, DropTarget, GroupChange, Plan};
        assert_eq!(
            view.cockpit.plan_group_drop(
                Drag::Thread {
                    thread: other,
                    group: None,
                },
                DropTarget::ThreadRow {
                    thread: named,
                    group: None,
                    index: 0,
                },
            ),
            Plan::Change(GroupChange::Create {
                first: named,
                second: other,
            }),
            "a drop is the machine's Server's to check"
        );
    });
    assert!(
        !dir.join("window").join("threads").exists(),
        "the window keeps no Store for the machine"
    );

    let fold = cx
        .debug_bounds("nav-parked")
        .expect("the machine's parked Threads put the Parked section in the nav");
    cx.simulate_click(fold.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let row_id: &'static str = format!("nav-thread-{}", named.get()).leak();
    let row = cx
        .debug_bounds(row_id)
        .expect("unfolded, the machine's parked Thread has a row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    until(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.panes.len() == 1 && view.facts.parked() == [other]
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes[0].thread(), Some(named), "revived on the machine");
    });
}
