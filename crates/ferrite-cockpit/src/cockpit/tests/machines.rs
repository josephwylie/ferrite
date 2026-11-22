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

/// `box`'s Server runs in this process (`ferrite_server::fixture_machine`), its
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
pub(super) struct OnBox(pub(super) std::sync::Arc<std::sync::Mutex<Vec<std::path::PathBuf>>>);

struct Quiet {
    _alive: ferrite_core::doorbell::Sender<SessionEvent>,
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
        let (alive, events) = ferrite_core::doorbell::channel();
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
            driven: false,
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
    machine_window_with(cx, core, machines, Preferences::ephemeral())
}

/// `machine_window`, with these preferences.
fn machine_window_with(
    cx: &mut TestAppContext,
    core: Cockpit,
    machines: Entity<Machines>,
    prefs: Preferences,
) -> (Entity<gpui::component::Root>, &mut gpui::VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| CockpitView::new_with_machines(core, Provider::Claude, prefs, Some(machines), cx));
        gpui::component::Root::new(
            cx.new(|cx| crate::cockpit_window::CockpitWindow::new(view, window, cx)),
            window,
            cx,
        )
    })
}

#[gpui::test]
fn escape_closes_settings_on_an_empty_remote_workspace(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let machines = cx.new(|_| machines("empty-remote-escape", Some("me@box")));
    let core = machines.update(cx, |machines, _| machines.cockpit()).unwrap();
    let (root, cx) = machine_window(cx, core, machines);
    tick(cx);
    let view = shown(&root, cx);
    view.read_with(cx, |view, _| assert!(view.panes.is_empty()));
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| assert!(view.settings_open(cx)));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| assert!(!view.settings_open(cx)));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        view.unqueue_from_backspace(&ferrite_pane::composer::Backspace, window, cx);
    });
    view.read_with(cx, |view, _| assert!(view.panes.is_empty()));
}

/// A pin made while the window is on a machine is saved as that machine's
/// Thread: ids are per Store, so box's Thread 1 is not this computer's.
#[gpui::test]
fn a_pin_made_on_a_machine_saves_that_machine(cx: &mut TestAppContext) {
    let machines = cx.new(|_| machines("pin-on-box", Some("me@box")));
    let core = machines.update(cx, |machines, _| machines.cockpit()).unwrap();
    let (root, cx) = machine_window(cx, core, machines);
    tick(cx);
    let view = shown(&root, cx);
    view.update(cx, |view, cx| {
        let key = view.cockpit.read(cx).key(ThreadId::new(1));
        view.set_pinned(key, true, cx)
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            serde_json::to_value(&view.prefs.settings).unwrap()["pinned_threads"],
            serde_json::json!([{ "machine": "me@box", "thread": 1 }])
        );
    });
}

/// A pin names the machine its Thread was reached on when the window
/// attached there (F-A8), not the one selected when it is read: through a
/// trip to box and back, this computer's pin and box's pin of a Thread of
/// the same number stay each machine's, each lists only on its own
/// machine, and unpinning one leaves the other.
#[gpui::test]
fn a_trip_to_a_machine_and_back_keeps_each_machines_pins(cx: &mut TestAppContext) {
    let (core, _fake, _) = bound_cockpit("machines-pins", Provider::Claude);
    let mine = core.known_threads().into_iter().next().unwrap();
    let number = mine.thread().get();
    let machines = cx.new(|_| machines("pins", None));
    let mut prefs = Preferences::ephemeral();
    prefs.settings.machines = vec![Machine {
        name: "box".into(),
        target: "me@box".into(),
        enabled: true,
    }];
    let (root, cx) = machine_window_with(cx, core, machines, prefs);
    let pins = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let saved = serde_json::to_value(&view.prefs.settings).unwrap()["pinned_threads"].clone();
            (saved, view.pins(cx))
        })
    };
    let home = shown(&root, cx);
    home.update(cx, |view, cx| view.set_pinned(mine, true, cx));
    assert_eq!(pins(&home, cx), (serde_json::json!([{ "machine": null, "thread": number }]), vec![mine]));

    home.update(cx, |_, cx| cx.emit(GoToMachine { index: Some(1), open: None }));
    tick(cx);
    let on_box = shown(&root, cx);
    assert_ne!(on_box.entity_id(), home.entity_id());
    let theirs = on_box.read_with(cx, |view, cx| view.cockpit.read(cx).key(mine.thread()));
    assert_eq!(pins(&on_box, cx).1, [], "this computer's pin is not box's Thread");
    on_box.update(cx, |view, cx| view.set_pinned(theirs, true, cx));
    let both = serde_json::json!([
        { "machine": null, "thread": number },
        { "machine": "me@box", "thread": number },
    ]);
    assert_eq!(pins(&on_box, cx), (both.clone(), vec![theirs]), "box's pin, saved as box's");

    on_box.update(cx, |_, cx| cx.emit(GoToMachine { index: Some(0), open: None }));
    tick(cx);
    let back = shown(&root, cx);
    assert_ne!(back.entity_id(), on_box.entity_id());
    let mine = back.read_with(cx, |view, cx| view.cockpit.read(cx).key(mine.thread()));
    assert_eq!(pins(&back, cx), (both, vec![mine]), "home lists only its own pin");
    back.update(cx, |view, cx| view.set_pinned(mine, false, cx));
    assert_eq!(
        pins(&back, cx),
        (serde_json::json!([{ "machine": "me@box", "thread": number }]), vec![]),
        "unpinning here leaves box's pin"
    );
}

#[gpui::test]
fn remote_files_and_terminal_open_through_their_keyboard_shortcuts(cx: &mut TestAppContext) {
    use ferrite_core::cockpit::{Attach, Endpoint, ServerState};
    use crate::cockpit::surfaces::SurfaceId;
    let dir = settings_dir("tool-keys");
    let _ = std::fs::remove_dir_all(&dir);
    let dial = ferrite_server::fixture_machine(
        dir.join("box"), Box::new(OnBox(Default::default())),
    ).unwrap();
    let mut core = Cockpit::attach(Attach {
        saved_as: Some(ferrite_core::MachineKey(Some("me@box".into()))),
        browser: false,
        driven: false,
        store: dir.join("window/threads"),
        server: Endpoint::Machine(std::sync::Arc::new(Bridge::new(
            "box", dir.join("bridge/bridge.sock"), dial,
        ))),
        client: ferrite_core::protocol::Peer { name: "test".into(), version: "0".into() },
        renderer: None,
        discovery: Box::new(Fake::default()),
    }).unwrap();
    ferrite_core::doorbell::until("remote server never attached", || {
        core.pump();
        matches!(core.server(), Some(ServerState::Attached { .. }))
    });
    let thread = core.open(Provider::Codex, WorkspaceChoice::Main { checkout: here() }).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.elsewhere(view.on_screen(cx), cx));
        assert!(view.opener_unavailable(thread, crate::cockpit::panel::Opener::Files, cx).is_none());
        assert!(view.opener_unavailable(thread, crate::cockpit::panel::Opener::Terminal, cx).is_none());
        assert!(view.opener_unavailable(thread, crate::cockpit::panel::Opener::Changes, cx).is_none());
    });
    cx.simulate_keystrokes("cmd-shift-f");
    tick(cx);
    view.read_with(cx, |view, _| assert_eq!(view.active_surface(thread), Some(SurfaceId::Files)));
    cx.simulate_keystrokes("cmd-j");
    tick(cx);
    view.read_with(cx, |view, _| assert!(matches!(view.active_surface(thread), Some(SurfaceId::Terminal(_)))));
    assert!(cx.debug_bounds("terminal-failed").is_some(), "a fake SSH bridge cannot start a local shell");
}

/// The Cockpit the window shows now.
fn shown(
    root: &Entity<gpui::component::Root>,
    cx: &mut gpui::VisualTestContext,
) -> Entity<CockpitView> {
    root.read_with(cx, |root, cx| {
        crate::cockpit_window::testing::cockpit(root.view(), cx)
    })
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

/// Another client (the board or another window) can close and open Threads
/// while this window is editing. Its UI mirror must follow the Server roster
/// before routing keyboard input by pane index.
#[gpui::test]
fn background_thread_changes_keep_the_visible_composer_live(cx: &mut TestAppContext) {
    use ferrite_core::cockpit::{Attach, Endpoint, ServerState};
    let dir = scratch("input-sync");
    let dial =
        ferrite_server::fixture_machine(dir.join("box"), Box::new(OnBox(Default::default())))
            .unwrap();
    let bridge = std::sync::Arc::new(Bridge::new(
        "box",
        dir.join("bridge").join("bridge.sock"),
        dial,
    ));
    let attach = |name: &str| {
        let mut core = Cockpit::attach(Attach {
            saved_as: Some(ferrite_core::MachineKey(Some("me@box".into()))),
            browser: false,
            driven: false,
            store: dir.join(name).join("threads"),
            server: Endpoint::Machine(bridge.clone()),
            client: ferrite_core::protocol::Peer {
                name: name.into(),
                version: "0".into(),
            },
            renderer: None,
            discovery: Box::new(Fake::default()),
        })
        .unwrap();
        ferrite_core::doorbell::until("client did not attach", || {
            core.pump();
            matches!(core.server(), Some(ServerState::Attached { .. }))
        });
        core
    };
    let mut background = attach("background");
    let earlier = background
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    let editing = background
        .open(Provider::Codex, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    background
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    let mut core = attach("window");
    // This window holds box under an id of its own.
    let editing = core.key(editing.thread());
    core.focus_thread(editing);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    tick(cx);
    cx.simulate_input("draft");
    tick(cx);
    let composer = view.read_with(cx, |view, _| {
        view.panes[view.pane_for(editing).unwrap()].composer.clone()
    });

    background.park_thread(earlier).unwrap();
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            let cockpit = view.cockpit.read(cx);
            cockpit.thread(cockpit.key(earlier.thread())).is_none()
        })
    });
    let editor = cx
        .debug_bounds("focused-prompt-editor")
        .expect("the surviving editor is focused");
    cx.simulate_click(editor.center(), gpui::Modifiers::none());
    cx.simulate_keystrokes("end");
    cx.simulate_input(" typed");
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(" pasted".into())));
    cx.dispatch_action(Paste);
    tick(cx);
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.text().to_owned()),
        "draft typed pasted"
    );
    assert!(cx.debug_bounds("prompt-placeholder").is_none());

    let opened = background
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    let opened = view.read_with(cx, |view, cx| view.cockpit.read(cx).key(opened.thread()));
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.cockpit.read(cx).thread(opened).is_some())
    });
    view.read_with(cx, |view, cx| {
        assert!(
            view.pane_for(opened).is_some(),
            "a background open gets a UI Pane too"
        );
        assert_eq!(view.focused_thread(cx), Some(editing));
        assert_eq!(
            view.panes[view.focused(cx)].composer,
            composer,
            "the existing editor survives roster changes"
        );
        assert_eq!(composer.read(cx).text(), "draft typed pasted");
    });
}

#[gpui::test]
fn a_machine_off_screen_heads_its_rows_with_its_connection(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("machines-listed", 1);
    let machines = cx.new(|_| machines("listed", None));
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_machines(core, Provider::Claude, Preferences::ephemeral(), Some(machines), cx)
    });
    tick(cx);
    assert!(
        debug_bounds(cx, "nav-machine-0".into()).is_some(),
        "this computer"
    );
    assert!(debug_bounds(cx, "nav-machine-1".into()).is_some(), "box");
    until(cx, |cx| connection(&view, 1, cx) == Connection::Attention);
    assert_eq!(connection(&view, 0, cx), Connection::Online);
    view.update(cx, |view, cx| {
        let machines = view.machines.as_ref().unwrap();
        assert!(matches!(
            machines.read(cx).server_state(1, view.cockpit.read(cx).server()),
            Some(ferrite_core::cockpit::ServerState::Detached {
                attention: Some(Failure::SignIn),
                ..
            })
        ));
        machines.update(cx, |machines, _| machines.retry(1));
        assert_eq!(
            machines.read(cx).rows(view.cockpit.read(cx).server())[1].connection,
            Connection::Connecting,
            "Retry also releases an off-screen authentication stop"
        );
    });
}

#[gpui::test]
fn removing_the_selected_connection_confirms_then_returns_to_this_computer(
    cx: &mut TestAppContext,
) {
    let machines = cx.new(|_| machines("remove-selected", Some("me@box")));
    let core = machines
        .update(cx, |machines, _| machines.cockpit())
        .unwrap();
    let (root, cx) = machine_window(cx, core, machines);
    let remote = shown(&root, cx);
    remote.update(cx, |view, _| {
        view.prefs.settings.machines = vec![Machine {
            name: "box".into(),
            target: "me@box".into(),
            enabled: true,
        }];
    });
    let remove = connection_act(&remote, "connection-remove-1", cx);
    cx.update(|window, cx| remove(window, cx));
    remote.read_with(cx, |view, _| {
        assert_eq!(
            view.prefs.settings.machines.len(),
            1,
            "the first press only asks"
        );
    });
    let confirm = connection_removal(&remote, cx);
    cx.update(|window, cx| confirm(window, cx));
    tick(cx);
    let local = shown(&root, cx);
    assert_ne!(local.entity_id(), remote.entity_id());
    local.read_with(cx, |view, cx| {
        assert!(view.bridge_on_screen(cx).is_none());
        assert!(view.prefs.settings.machines.is_empty());
        assert!(!view.machines.as_ref().unwrap().read(cx).listed());
    });
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
                driven: false,
                defaults: Default::default(),
                bridge: needs_sign_in,
            },
            &[],
            None,
        )
    });
    let (_view, cx) =
        add_cockpit_window(cx, |_, cx| CockpitView::new_with_machines(core, Provider::Claude, Preferences::ephemeral(), Some(alone), cx));
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
    first.update(cx, |_, cx| {
        cx.emit(GoToMachine {
            index: Some(1),
            open: None,
        })
    });
    tick(cx);
    let on_box = shown(cx);
    assert_ne!(on_box.entity_id(), first.entity_id(), "built afresh");
    on_box.read_with(cx, |view, cx| {
        assert_eq!(
            view.bridge_on_screen(cx).map(|bridge| bridge.name()),
            Some("box")
        );
        assert!(view.panes.is_empty(), "no draft before the machine answers");
    });
    until(cx, |cx| {
        on_box.read_with(cx, |view, cx| {
            matches!(
                view.cockpit.read(cx).server(),
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
    on_box.update(cx, |_, cx| {
        cx.emit(GoToMachine {
            index: Some(0),
            open: None,
        })
    });
    tick(cx);
    let home = shown(cx);
    assert_ne!(home.entity_id(), on_box.entity_id(), "built afresh again");
    home.read_with(cx, |view, cx| {
        assert!(view.bridge_on_screen(cx).is_none(), "this computer");
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

#[gpui::test]
fn switching_machines_transfers_unsaved_panel_state_and_retires_the_previous_writer(cx: &mut TestAppContext) {
    use crate::cockpit::surfaces::{SurfaceAction, SurfaceId, Surfaces};
    let (core, _fake, checkout) = bound_cockpit("machines-panel-transfer", Provider::Claude);
    let machines = cx.new(|_| machines("panel-transfer", None));
    let (root, cx) = machine_window(cx, core, machines);
    let first = shown(&root, cx);
    let dir = scratch("machines-panel-transfer-prefs");
    std::fs::write(&dir, "the settings directory is temporarily unavailable").unwrap();
    let thread = first.update(cx, |view, cx| {
        view.prefs.dir = dir.clone();
        let thread = view.panes[0].thread().unwrap();
        view.open_in_files(thread, checkout.join("src/lib.rs"), None, true, cx);
        view.pump(cx);
        view.flush_panel_persistence(cx);
        view.apply_surface(thread, SurfaceAction::Open(SurfaceId::Browser));
        view.apply_surface(thread, SurfaceAction::Open(SurfaceId::Changes));
        cx.emit(GoToMachine { index: Some(1), open: None });
        thread
    });
    cx.run_until_parked();
    let successor = shown(&root, cx);
    assert_ne!(successor.entity_id(), first.entity_id());
    successor.read_with(cx, |view, cx| {
        let on_box = view.cockpit.read(cx).key(thread.thread());
        assert!(
            view.panel.surfaces.get(on_box).is_none(),
            "box's Thread of the same number is another Thread, with no Panel yet"
        );
    });
    std::fs::remove_file(&dir).unwrap();
    successor.update(cx, |view, cx| view.flush_panel_persistence(cx));
    first.update(cx, |view, cx| {
        view.apply_surface(thread, SurfaceAction::Open(SurfaceId::Overview));
        view.pump(cx);
        view.flush_panel_persistence(cx);
    });
    let state = Surfaces::saved_in(&dir, thread.thread()).expect("this computer's Panel, saved by the successor");
    assert_eq!(state.active, Some(SurfaceId::Changes));
    assert!(!state.tabs.contains(&SurfaceId::Browser), "live resources end with their owner");
    assert_eq!(saved_files(&dir), [(None, thread.thread().get(), vec!["lib.rs".to_string()])]);
}

/// files.json as `(machine, thread, file names)`, sorted.
fn saved_files(dir: &std::path::Path) -> Vec<(Option<String>, u64, Vec<String>)> {
    let files: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("files.json")).unwrap()).unwrap();
    let name = |file: &serde_json::Value| {
        let path = std::path::Path::new(file["path"].as_str().unwrap());
        path.file_name().unwrap().to_string_lossy().into_owned()
    };
    files["threads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|set| {
            let names = set["files"].as_array().unwrap().iter().map(name).collect();
            (set["machine"].as_str().map(str::to_string), set["thread"].as_u64().unwrap(), names)
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// files.json in `dir` holding one file for each of `sets`, as
/// `(machine, thread, file name)`: what earlier visits left.
fn seed_saved_files(dir: &std::path::Path, sets: &[(Option<&str>, u64, &str)]) {
    let threads: Vec<_> = sets
        .iter()
        .map(|(machine, thread, file)| {
            serde_json::json!({
                "machine": machine,
                "thread": thread,
                "files": [{ "path": dir.join(file), "preview": false }],
                "active": 0,
                "settings": { "wrap": true, "auto_save": true, "tab_size": 4 },
                "preview_tabs": true,
                "show_tree": true,
            })
        })
        .collect();
    let files = serde_json::json!({ "version": 2, "threads": threads });
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("files.json"), serde_json::to_vec(&files).unwrap()).unwrap();
}

/// `machine_window`, its preferences (files.json and cited-sources.json
/// among them) in `dir`.
fn machine_window_saving_to<'a>(
    cx: &'a mut TestAppContext,
    core: Cockpit,
    machines: Entity<Machines>,
    dir: &std::path::Path,
) -> (Entity<gpui::component::Root>, &'a mut gpui::VisualTestContext) {
    let mut prefs = Preferences::ephemeral();
    prefs.dir = dir.to_path_buf();
    machine_window_with(cx, core, machines, prefs)
}

/// The verifier's run: this computer's Thread has Files open, and files.json
/// holds box's sets from an earlier visit. Going to box (which has not
/// answered yet) and back saves this computer's tabs as this computer's
/// and leaves box's sets alone.
#[gpui::test]
fn going_to_a_machine_and_back_keeps_each_machines_files_tabs(cx: &mut TestAppContext) {
    let (core, _fake, checkout) = bound_cockpit("machines-files-there", Provider::Claude);
    let thread = core.known_threads().into_iter().next().unwrap();
    let dir = scratch("machines-files-there-prefs");
    seed_saved_files(&dir, &[(Some("me@box"), thread.thread().get(), "box-only-1.rs"), (Some("me@box"), 7, "box-only-7.rs")]);
    let machines = cx.new(|_| machines("files-there", None));
    let (root, cx) = machine_window_saving_to(cx, core, machines, &dir);
    let home = shown(&root, cx);
    home.update(cx, |view, cx| {
        view.open_in_files(thread, checkout.join("src/lib.rs"), None, true, cx);
        view.open_in_files(thread, checkout.join("README.md"), None, true, cx);
        view.flush_panel_persistence(cx);
    });
    let expected = [
        (None, thread.thread().get(), vec!["lib.rs".to_string(), "README.md".to_string()]),
        (Some("me@box".to_string()), thread.thread().get(), vec!["box-only-1.rs".to_string()]),
        (Some("me@box".to_string()), 7, vec!["box-only-7.rs".to_string()]),
    ];
    assert_eq!(saved_files(&dir), expected);

    home.update(cx, |_, cx| cx.emit(GoToMachine { index: Some(1), open: None }));
    tick(cx);
    let on_box = shown(&root, cx);
    assert_ne!(on_box.entity_id(), home.entity_id());
    on_box.update(cx, |view, cx| view.flush_panel_persistence(cx));
    assert_eq!(saved_files(&dir), expected, "on box");

    on_box.update(cx, |_, cx| cx.emit(GoToMachine { index: Some(0), open: None }));
    tick(cx);
    shown(&root, cx).update(cx, |view, cx| view.flush_panel_persistence(cx));
    assert_eq!(saved_files(&dir), expected, "back home");
}

/// The other way: on box, which knows its Thread, with Files open there.
/// Coming home saves box's tabs as box's, not over this computer's set
/// of the same Thread id, and box's list prunes only box's sets.
#[gpui::test]
fn coming_home_from_a_machine_saves_its_files_tabs_as_its_own(cx: &mut TestAppContext) {
    let (core, _fake, checkout) = bound_cockpit_as("machines-files-back", Provider::Claude, ferrite_core::MachineKey(Some("me@box".into())));
    let thread = core.known_threads().into_iter().next().unwrap();
    let dir = scratch("machines-files-back-prefs");
    seed_saved_files(&dir, &[(None, thread.thread().get(), "here-1.rs")]);
    let machines = cx.new(|_| machines("files-back", Some("me@box")));
    let (root, cx) = machine_window_saving_to(cx, core, machines, &dir);
    let on_box = shown(&root, cx);
    on_box.update(cx, |view, cx| {
        view.open_in_files(thread, checkout.join("src/lib.rs"), None, true, cx);
        view.flush_panel_persistence(cx);
    });
    let expected = [
        (None, thread.thread().get(), vec!["here-1.rs".to_string()]),
        (Some("me@box".to_string()), thread.thread().get(), vec!["lib.rs".to_string()]),
    ];
    assert_eq!(saved_files(&dir), expected);

    on_box.update(cx, |_, cx| cx.emit(GoToMachine { index: Some(0), open: None }));
    tick(cx);
    let home = shown(&root, cx);
    assert_ne!(home.entity_id(), on_box.entity_id());
    home.update(cx, |view, cx| view.flush_panel_persistence(cx));
    assert_eq!(saved_files(&dir), expected);
}

/// drafts.json as `(machine, thread, text)`, sorted.
fn saved_drafts(dir: &std::path::Path) -> Vec<(Option<String>, u64, String)> {
    let drafts: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("drafts.json")).unwrap()).unwrap();
    let mut saved: Vec<_> = drafts["threads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|draft| {
            let thread = &draft["thread"];
            (
                thread["machine"].as_str().map(str::to_string),
                thread["thread"].as_u64().unwrap(),
                draft["content"]["text"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    saved.sort();
    saved
}

/// drafts.json in `dir` holding `drafts`, as `(machine, thread, text)`:
/// what earlier visits left.
fn seed_saved_drafts(dir: &std::path::Path, drafts: &[(Option<&str>, u64, &str)]) {
    let mut saved = ferrite_core::drafts::Drafts::default();
    for (machine, thread, text) in drafts {
        let thread = ferrite_core::SavedThread {
            machine: ferrite_core::MachineKey(machine.map(str::to_string)),
            thread: ThreadId::new(*thread),
        };
        saved.keep_thread(thread, ferrite_core::draft_content::DraftContent::from_prompt(text.to_string()));
    }
    saved.save(dir).unwrap();
}

/// `machine_window`, its drafts.json in `dir`.
fn machine_window_drafting_in<'a>(
    cx: &'a mut TestAppContext,
    core: Cockpit,
    machines: Entity<Machines>,
    dir: &std::path::Path,
) -> (Entity<gpui::component::Root>, &'a mut gpui::VisualTestContext) {
    let mut prefs = Preferences::ephemeral();
    prefs.dir = dir.to_path_buf();
    machine_window_with(cx, core, machines, prefs)
}

/// `view`'s first Pane's Composer takes `text`, as typing would leave it.
fn draft_on_first_pane(view: &Entity<CockpitView>, text: &str, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| {
        let composer = view.panes[0].composer.clone();
        let content = ferrite_core::draft_content::DraftContent::from_prompt(text.to_string());
        composer.update(cx, |composer, cx| composer.load(content, cx));
    });
}

/// This computer's Thread and box's Thread of the same number each have
/// a draft, and box has one on a Thread only it has. Going to box (which
/// has not answered yet) and back saves this computer's draft as this
/// computer's, and box's stay box's, whichever view saves last.
#[gpui::test]
fn going_to_a_machine_and_back_keeps_each_machines_drafts(cx: &mut TestAppContext) {
    let (core, _fake, _checkout) = bound_cockpit("machines-drafts-there", Provider::Claude);
    let thread = core.known_threads().into_iter().next().unwrap();
    let dir = scratch("machines-drafts-there-prefs");
    seed_saved_drafts(
        &dir,
        &[(None, thread.thread().get(), "here"), (Some("me@box"), thread.thread().get(), "there"), (Some("me@box"), 7, "box only")],
    );
    let machines = cx.new(|_| machines("drafts-there", None));
    let (root, cx) = machine_window_drafting_in(cx, core, machines, &dir);
    let home = shown(&root, cx);
    assert_eq!(composer_text(&home, cx), "here", "this computer's Thread opens with its own draft");
    draft_on_first_pane(&home, "here, and more", cx);
    home.update(cx, |view, cx| view.save_drafts(cx));
    let expected = [
        (None, thread.thread().get(), "here, and more".to_string()),
        (Some("me@box".to_string()), thread.thread().get(), "there".to_string()),
        (Some("me@box".to_string()), 7, "box only".to_string()),
    ];
    assert_eq!(saved_drafts(&dir), expected);

    home.update(cx, |_, cx| cx.emit(GoToMachine { index: Some(1), open: None }));
    tick(cx);
    let on_box = shown(&root, cx);
    assert_ne!(on_box.entity_id(), home.entity_id());
    on_box.update(cx, |view, cx| view.save_drafts(cx));
    assert_eq!(saved_drafts(&dir), expected, "on box");
    home.update(cx, |view, cx| view.save_drafts(cx));
    assert_eq!(saved_drafts(&dir), expected, "the view left saves late");

    on_box.update(cx, |_, cx| cx.emit(GoToMachine { index: Some(0), open: None }));
    tick(cx);
    shown(&root, cx).update(cx, |view, cx| view.save_drafts(cx));
    assert_eq!(saved_drafts(&dir), expected, "back home");
    on_box.update(cx, |view, cx| view.save_drafts(cx));
    assert_eq!(saved_drafts(&dir), expected, "box's view saves late");
}

/// The other way: on box, whose Thread shares its number with this
/// computer's, with a draft typed there. Coming home saves box's draft
/// as box's, not over this computer's draft of the same Thread number.
#[gpui::test]
fn coming_home_from_a_machine_saves_its_drafts_as_its_own(cx: &mut TestAppContext) {
    let (core, _fake, _checkout) = bound_cockpit_as("machines-drafts-back", Provider::Claude, ferrite_core::MachineKey(Some("me@box".into())));
    let thread = core.known_threads().into_iter().next().unwrap();
    let dir = scratch("machines-drafts-back-prefs");
    seed_saved_drafts(&dir, &[(None, thread.thread().get(), "here"), (Some("me@box"), thread.thread().get(), "there")]);
    let machines = cx.new(|_| machines("drafts-back", Some("me@box")));
    let (root, cx) = machine_window_drafting_in(cx, core, machines, &dir);
    let on_box = shown(&root, cx);
    assert_eq!(composer_text(&on_box, cx), "there", "box's Thread opens with box's draft");
    draft_on_first_pane(&on_box, "there, and more", cx);
    on_box.update(cx, |view, cx| view.save_drafts(cx));
    let expected = [
        (None, thread.thread().get(), "here".to_string()),
        (Some("me@box".to_string()), thread.thread().get(), "there, and more".to_string()),
    ];
    assert_eq!(saved_drafts(&dir), expected);

    on_box.update(cx, |_, cx| cx.emit(GoToMachine { index: Some(0), open: None }));
    tick(cx);
    let home = shown(&root, cx);
    assert_ne!(home.entity_id(), on_box.entity_id());
    home.update(cx, |view, cx| view.save_drafts(cx));
    assert_eq!(saved_drafts(&dir), expected, "home");
    on_box.update(cx, |view, cx| view.save_drafts(cx));
    assert_eq!(saved_drafts(&dir), expected, "the view left saves late");
}

/// cited-sources.json as `(machine, thread, [(quote, block)])`, sorted.
fn saved_cited_sources(dir: &std::path::Path) -> Vec<(Option<String>, u64, Vec<(String, u64)>)> {
    let file: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("cited-sources.json")).unwrap()).unwrap();
    assert_eq!(file["version"], 2);
    let mut kept: Vec<_> = file["threads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|kept| {
            let sources = kept["sources"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(quote, block)| (quote.clone(), block.as_u64().unwrap()))
                .collect();
            (kept["machine"].as_str().map(str::to_string), kept["thread"].as_u64().unwrap(), sources)
        })
        .collect();
    kept.sort();
    kept
}

/// cited-sources.json in `dir` holding one source for each of `kept`, as
/// `(machine, thread, quote, block)`: what earlier visits left.
fn seed_cited_sources(dir: &std::path::Path, kept: &[(Option<&str>, u64, &str, u64)]) {
    let threads: Vec<_> = kept
        .iter()
        .map(|(machine, thread, quote, block)| {
            serde_json::json!({ "machine": machine, "thread": thread, "sources": { *quote: block } })
        })
        .collect();
    let file = serde_json::json!({ "version": 2, "threads": threads });
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("cited-sources.json"), serde_json::to_vec(&file).unwrap()).unwrap();
}

/// Send `quote`, cited from Block `block` of `thread`, on `view`'s first
/// Pane, as the operator does: the quote in its Composer, a line, ↵.
fn send_cited(
    view: &Entity<CockpitView>,
    thread: ThreadKey,
    quote: &str,
    block: u64,
    cx: &mut gpui::VisualTestContext,
) {
    let source = ferrite_core::replies::Source {
        thread: thread.thread(),
        block: serde_json::from_str(&block.to_string()).unwrap(),
    };
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.reply(ferrite_core::replies::Reply::new(quote, "").cited_from(source), cx)
        })
    });
    tick(cx);
    let line = cx.debug_bounds("focused-prompt-editor").expect("the line");
    cx.simulate_click(line.center(), gpui::Modifiers::none());
    cx.simulate_input("go on");
    cx.simulate_keystrokes("enter");
    tick(cx);
}

/// This computer's Thread cites a passage, and cited-sources.json holds
/// box's sources from an earlier visit, one on a Thread of the same number.
/// Going to box (which has not answered yet) and back keeps this
/// computer's sources as this computer's and leaves box's alone.
#[gpui::test]
fn going_to_a_machine_and_back_keeps_each_machines_cited_sources(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, _fake, _) = bound_cockpit("machines-cited-there", Provider::Claude);
    let thread = core.known_threads().into_iter().next().unwrap();
    let dir = scratch("machines-cited-there-prefs");
    seed_cited_sources(
        &dir,
        &[
            (None, thread.thread().get(), "here before", 3),
            (Some("me@box"), thread.thread().get(), "box's", 11),
            (Some("me@box"), 7, "box only", 17),
        ],
    );
    let machines = cx.new(|_| machines("cited-there", None));
    let (root, cx) = machine_window_saving_to(cx, core, machines, &dir);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let home = shown(&root, cx);
    send_cited(&home, thread, "alpha", 5, cx);
    let expected = [
        (None, thread.thread().get(), vec![("alpha".to_string(), 5), ("here before".to_string(), 3)]),
        (Some("me@box".to_string()), thread.thread().get(), vec![("box's".to_string(), 11)]),
        (Some("me@box".to_string()), 7, vec![("box only".to_string(), 17)]),
    ];
    assert_eq!(saved_cited_sources(&dir), expected);

    home.update(cx, |_, cx| cx.emit(GoToMachine { index: Some(1), open: None }));
    tick(cx);
    let on_box = shown(&root, cx);
    assert_ne!(on_box.entity_id(), home.entity_id());
    assert_eq!(saved_cited_sources(&dir), expected, "on box");

    on_box.update(cx, |_, cx| cx.emit(GoToMachine { index: Some(0), open: None }));
    tick(cx);
    assert_ne!(shown(&root, cx).entity_id(), on_box.entity_id());
    assert_eq!(saved_cited_sources(&dir), expected, "back home");
}

/// The other way: on box, whose Thread has the number of one of this
/// computer's, a passage is cited. Coming home keeps it as box's, not over
/// this computer's sources of the same Thread number.
#[gpui::test]
fn coming_home_from_a_machine_keeps_its_cited_sources_as_its_own(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let machines = cx.new(|_| machines_through("cited-back", Some("me@box"), serving));
    let _ = ferrite_server::fixture_machine(
        settings_dir("cited-back").join("box"),
        Box::new(OnBox(Default::default())),
    )
    .unwrap();
    let mut core = machines.update(cx, |machines, _| machines.cockpit()).unwrap();
    ferrite_core::doorbell::until("box did not attach", || {
        core.pump();
        matches!(core.server(), Some(ferrite_core::cockpit::ServerState::Attached { .. }))
    });
    let thread = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    let dir = scratch("machines-cited-back-prefs");
    seed_cited_sources(&dir, &[(None, thread.thread().get(), "here before", 3), (Some("me@box"), 7, "box only", 17)]);
    let (root, cx) = machine_window_saving_to(cx, core, machines, &dir);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let on_box = shown(&root, cx);
    until(cx, |cx| on_box.read_with(cx, |view, _| !view.panes.is_empty()));
    send_cited(&on_box, thread, "alpha", 5, cx);
    let expected = [
        (None, thread.thread().get(), vec![("here before".to_string(), 3)]),
        (Some("me@box".to_string()), thread.thread().get(), vec![("alpha".to_string(), 5)]),
        (Some("me@box".to_string()), 7, vec![("box only".to_string(), 17)]),
    ];
    assert_eq!(saved_cited_sources(&dir), expected);

    on_box.update(cx, |_, cx| cx.emit(GoToMachine { index: Some(0), open: None }));
    tick(cx);
    let home = shown(&root, cx);
    assert_ne!(home.entity_id(), on_box.entity_id());
    assert_eq!(saved_cited_sources(&dir), expected, "home");
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
        CockpitView::new_with_machines(core, Provider::Claude, Preferences::ephemeral(), Some(machines), cx)
    });
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            matches!(
                view.cockpit.read(cx).server(),
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
        debug_bounds(cx, "machine-lost-list".into()).is_some(),
        "its list in the nav"
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
        assert!(view.bridge_on_screen(cx).is_none(), "this computer");
        assert_eq!(
            view.machine_rows(cx).unwrap()[1].connection,
            Connection::Disabled
        );
    });
}

/// A machine's parked Threads are its Server's to list: its logs stay on
/// it. They show in the nav's Parked section, called what the machine's
/// log calls them, and a click revives one there.
#[gpui::test]
fn a_machines_parked_threads_show_in_the_nav_and_revive_on_click(cx: &mut TestAppContext) {
    use ferrite_core::cockpit::{Attach, Endpoint, ServerState};
    let dir = scratch("machine-parked");
    let dial =
        ferrite_server::fixture_machine(dir.join("box"), Box::new(OnBox(Default::default())))
            .unwrap();
    let mut core = Cockpit::attach(Attach {
            saved_as: Some(ferrite_core::MachineKey(Some("me@box".into()))),
            browser: false,
            driven: false,
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
    ferrite_core::doorbell::until("never attached", || {
        core.pump();
        matches!(core.server(), Some(ServerState::Attached { .. }))
    });
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
        assert_eq!(view.facts.parked().to_vec(), [named, other], "in creation order");
        assert_eq!(view.facts.name(named).as_ref(), "Release notes");
        assert!(view.panes.is_empty(), "nothing open on the machine");
    });
    assert!(
        !dir.join("window").join("threads").exists(),
        "the window keeps no Store for the machine"
    );

    let door = cx
        .debug_bounds("nav-shelf-parked")
        .expect("the Parked shelf's header");
    cx.simulate_click(door.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let row_id: &'static str = format!("nav-thread-{}", named.thread().get()).leak();
    let row = cx
        .debug_bounds(row_id)
        .expect("shown, the machine's parked Thread has a row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    until(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.panes.len() == 1 && view.facts.parked().to_vec() == [other]
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.panes[0].thread(),
            Some(named),
            "revived on the machine"
        );
    });
}

/// A Thread on box, as its Server says it: Claude, `title`, idle.
fn box_thread(thread: ThreadId, title: &str) -> ferrite_core::protocol::ThreadState {
    ferrite_core::protocol::ThreadState {
        thread,
        generation: 1,
        provider: Provider::Claude,
        project: None,
        model: None,
        effort: None,
        title: Some(title.into()),
        snooze: None,
        owner: None,
        workspace: None,
        session_project_root: None,
        first_prompt_sent: true,
        has_prompt_history: true,
        resume_lost: false,
        starting: false,
        working: false,
        tuning: false,
        native_queue_pending: false,
        queued: vec![],
        queue: vec![],
        suggestion: None,
        commands: vec![],
        models: vec![],
        permission_mode: None,
        sandbox: None,
        permission_modes: vec![],
        controls: vec![],
        store_error: None,
        for_draft: false,
        recovered: vec![],
        wants_listing: false,
        history_errors: vec![],
    }
}

/// The handle of box's Decision `serial`.
fn box_handle(serial: u64) -> ferrite_core::protocol::WireHandle {
    ferrite_core::protocol::WireHandle {
        generation: 1,
        serial,
        request_id: format!("r{serial}"),
    }
}

/// box's request Notice for Decision `serial` on `thread`, unread.
pub(super) fn box_request(thread: ThreadId, serial: u64) -> ferrite_core::protocol::WireNotice {
    ferrite_core::protocol::WireNotice::Request {
        thread,
        handle: box_handle(serial),
        subject: Some(ferrite_core::protocol::WireSubject::Main),
        question: false,
        read: false,
        at_ms: 0,
        seq: serial,
        dismissed: false,
    }
}

/// What the Inbox calls box's Decision `serial` on `thread`.
fn box_asked(thread: ThreadId, serial: u64) -> crate::notifications::RowTarget {
    crate::notifications::RowTarget::Decision(ferrite_core::notifications::DecisionNoticeId {
        thread,
        handle: (&box_handle(serial)).into(),
    })
}

/// box's Server attaching: `threads` open, titled, holding `notices`.
fn box_snapshot(
    threads: &[(ThreadId, &str)],
    notices: Vec<ferrite_core::protocol::WireNotice>,
) -> ferrite_core::protocol::ServerMessage {
    ferrite_core::protocol::ServerMessage::Snapshot(ferrite_core::protocol::Snapshot {
        boot_id: "box".into(),
        revision: 1,
        threads: threads
            .iter()
            .map(|(thread, title)| box_thread(*thread, title))
            .collect(),
        roster: ferrite_core::protocol::SharedRoster {
            open: threads.iter().map(|(thread, _)| *thread).collect(),
            parked: vec![],
        },
        decisions: vec![],
        groups: None,
        registry: None,
        notices: Some(notices),
        providers: None,
        parked: None,
        digests: None,
        factory: None,
    })
}

/// box's Thread `thread` asking to run `command`: Decision `serial`.
fn box_decision(
    thread: ThreadId,
    serial: u64,
    command: &str,
) -> ferrite_core::protocol::ServerMessage {
    use ferrite_core::protocol::{WireDecision, WireDecisionKind, WireDelivery, WirePolicy};
    ferrite_core::protocol::ServerMessage::Decision {
        thread,
        subject: Some(ferrite_core::protocol::WireSubject::Main),
        handle: box_handle(serial),
        decision: WireDecision {
            id: format!("r{serial}"),
            delivery: WireDelivery::Blocking,
            kind: WireDecisionKind::Approval,
            policy: WirePolicy {
                allow: true,
                deny: true,
                prefer_deny: false,
                interaction_required: false,
            },
            tool_use_id: format!("toolu_{serial}"),
            tool_name: "Bash".into(),
            description: "Publish".into(),
            input: serde_json::json!({ "command": command }),
            suggestions: vec![],
        },
    }
}

/// What box's status link hears, as if box's Server had said it.
fn hear_box(
    machines: &Entity<Machines>,
    message: ferrite_core::protocol::ServerMessage,
    cx: &mut gpui::VisualTestContext,
) {
    // As a status that read a message does (`Machines::poll`), the
    // machines notify: what hears them reads it then.
    machines.update(cx, |machines, cx| {
        machines.status_mut(1).unwrap().fixture_receive(message);
        cx.notify();
    });
}

/// ADR 0026 with ADR 0017: a Decision on a machine off screen toasts and
/// chimes once, names its machine, and with the window in the background
/// posts a system notification. What box held when first heard is not
/// news. Clicking the notification puts the window on box, which opens
/// the request once it is attached, and whose Needs-you state has heard box already.
#[gpui::test]
fn a_decision_off_screen_rings_once_and_its_click_goes_to_its_machine(cx: &mut TestAppContext) {
    use crate::notifications::{Alert, Detail};
    use ferrite_core::protocol::ServerMessage;
    let (core, _fake) = cockpit("machines-alert", 1);
    let machines = cx.new(|_| machines("alert", None));
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (root, cx) = machine_window(cx, core, machines.clone());
    let first = shown(&root, cx);
    let thread = ThreadId::new(1);
    hear_box(
        &machines,
        box_snapshot(&[(thread, "Release")], vec![box_request(thread, 1)]),
        cx,
    );
    ring(cx);
    first.read_with(cx, |view, cx| {
        assert!(view.toasts.read(cx).needs_you.toasts().is_empty(), "held when first heard");
        assert!(view.chimed.is_empty());
    });

    cx.deactivate_window();
    hear_box(
        &machines,
        box_decision(thread, 2, "cargo publish --dry-run"),
        cx,
    );
    hear_box(
        &machines,
        ServerMessage::Notices {
            revision: 2,
            notices: vec![box_request(thread, 1), box_request(thread, 2)],
        },
        cx,
    );
    tick(cx);
    first.read_with(cx, |view, cx| {
        assert!(view.toasts.read(cx).needs_you.toasts().is_empty(), "it waits its second")
    });
    ring(cx);
    ring(cx);
    let key = ferrite_core::MachineKey(Some("me@box".into()));
    let asked = box_asked(thread, 2);
    let alert = Alert {
        away: Some(key.clone()),
        target: asked.clone(),
        thread,
    };
    first.read_with(cx, |view, cx| {
        assert_eq!(view.chimed, [Sound::NeedsYou], "once");
        let toasts = view.toasts.read(cx).needs_you.toasts();
        assert_eq!(toasts.len(), 1);
        assert_eq!(toasts[0].alert, alert);
        let rows = view.needs_you_rows(cx);
        let queued: Vec<_> = rows
            .iter()
            .filter(|row| row.away.as_ref() == Some(&key) && row.thread == thread)
            .collect();
        assert_eq!(queued.len(), 1, "one queue row per machine and Thread");
        assert_eq!(
            queued[0].target,
            box_asked(thread, 1),
            "oldest request owns the queue row"
        );
        let row = view
            .alert_row(
                &alert,
                Some(machines.read(cx)),
                ferrite_core::clock::system_time(),
                cx,
            )
            .unwrap();
        assert_eq!(
            row.title.as_ref(),
            "box \u{203a} Release",
            "named by its machine"
        );
        assert_eq!(
            row.detail,
            Detail::Run {
                tool: "Bash".into(),
                command: "cargo publish".into(),
            }
        );
        assert_eq!(
            row.open_verb(),
            Verb::GoTo(alert.clone()),
            "a toast's open too"
        );
    });
    let tag = system_tag(&key, thread);
    assert_eq!(
        cx.delivered_system_notifications()
            .iter()
            .map(|notification| notification.tag.to_string())
            .collect::<Vec<_>>(),
        std::slice::from_ref(&tag)
    );

    cx.simulate_system_notification_response(gpui::SystemNotificationResponse {
        tag: tag.into(),
        action_id: None,
    });
    tick(cx);
    let on_box = shown(&root, cx);
    assert_ne!(on_box.entity_id(), first.entity_id(), "the window went");
    on_box.update(cx, |view, cx| {
        assert_eq!(
            view.bridge_on_screen(cx).map(|bridge| bridge.name()),
            Some("box")
        );
        assert_eq!(
            view.landing.as_ref().map(|(landing, _)| landing.clone()),
            Some(Landing::Alert(Alert {
                away: None,
                target: asked,
                thread,
            })),
            "opened once box answers"
        );
        // Once box answers, its requests are no news: both rang, or were
        // held, while it was off screen.
        let now = cx.background_executor().now();
        let held = [box_asked(thread, 1), box_asked(thread, 2)].map(|target| match target {
            crate::notifications::RowTarget::Decision(id) => (id, false),
            _ => unreachable!(),
        });
        view.toasts.update(cx, |toasts, _| {
            let needs_you = &mut toasts.needs_you;
            needs_you.hear(None, [], held, now);
            let ring = needs_you.ring(
                now + crate::notifications::ALERT_DELAY,
                &|_| Some(true),
                &|_| false,
            );
            assert!(ring.rang.is_empty(), "nothing rings twice");
        });
        assert_eq!(
            view.toasts
                .read(cx)
                .needs_you
                .toasts()
                .iter()
                .map(|toast| &toast.alert.away)
                .collect::<Vec<_>>(),
            [&None],
            "its toast came along, on screen now"
        );
    });
}

/// The Inbox moves with the window: a system notification posted about
/// this computer while the window was on it still opens, from box, on
/// this computer's Thread.
#[gpui::test]
fn a_system_notification_outlives_a_move(cx: &mut TestAppContext) {
    use ferrite_core::MachineKey;
    use crate::notifications::Alert;
    let (core, fake) = cockpit("machines-posted", 1);
    let machines = cx.new(|_| machines("posted", None));
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (root, cx) = machine_window(cx, core, machines.clone());
    let first = shown(&root, cx);
    let here = first.read_with(cx, |view, cx| view.cockpit.read(cx).threads()[0]);
    let thread = ThreadId::new(1);
    hear_box(&machines, box_snapshot(&[(thread, "Release")], vec![]), cx);
    ring(cx);

    cx.deactivate_window();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    hear_box(&machines, box_decision(thread, 1, "make deploy"), cx);
    hear_box(
        &machines,
        ferrite_core::protocol::ServerMessage::Notices {
            revision: 2,
            notices: vec![box_request(thread, 1)],
        },
        cx,
    );
    tick(cx);
    ring(cx);
    ring(cx);
    let home_tag = system_tag(&MachineKey(None), here.thread());
    let box_tag = system_tag(&MachineKey(Some("me@box".into())), thread);
    let mut tags: Vec<String> = cx
        .delivered_system_notifications()
        .iter()
        .map(|notification| notification.tag.to_string())
        .collect();
    tags.sort();
    assert_eq!(tags, [home_tag.clone(), box_tag.clone()]);

    let click = |tag: String, cx: &mut gpui::VisualTestContext| {
        cx.simulate_system_notification_response(gpui::SystemNotificationResponse {
            tag: tag.into(),
            action_id: None,
        });
        tick(cx);
    };
    click(box_tag, cx);
    let on_box = shown(&root, cx);
    assert_ne!(on_box.entity_id(), first.entity_id(), "the window went");
    click(home_tag, cx);
    let home = shown(&root, cx);
    assert_ne!(home.entity_id(), on_box.entity_id(), "and came back");
    home.read_with(cx, |view, cx| {
        assert!(view.bridge_on_screen(cx).is_none(), "this computer");
        assert_eq!(
            view.landing.as_ref().map(|(landing, _)| match landing {
                Landing::Alert(Alert { away, thread, .. })
=> (away.clone(), *thread),
            }),
            Some((None, here.thread())),
            "on its Thread once it answers"
        );
    });
}

/// A toast's open about box goes there as a click does. Claude muted, a
/// Decision on box toasts without a chime.
#[gpui::test]
fn a_toast_press_goes_to_its_machine(cx: &mut TestAppContext) {
    use crate::notifications::Alert;
    let (core, _fake) = cockpit("machines-toast", 1);
    let machines = cx.new(|_| machines("toast", None));
    let (root, cx) = machine_window(cx, core, machines.clone());
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    let first = shown(&root, cx);
    first.update(cx, |view, cx| {
        view.prefs
            .settings
            .set_provider_sounds(Provider::Claude, false);
        view.set_nav_collapsed(true, cx);
    });
    let thread = ThreadId::new(4);
    hear_box(&machines, box_snapshot(&[(thread, "Deploy")], vec![]), cx);
    ring(cx);
    hear_box(&machines, box_decision(thread, 1, "make deploy"), cx);
    hear_box(
        &machines,
        ferrite_core::protocol::ServerMessage::Notices {
            revision: 2,
            notices: vec![box_request(thread, 1)],
        },
        cx,
    );
    ring(cx);
    ring(cx);
    first.read_with(cx, |view, cx| {
        assert_eq!(view.toasts.read(cx).needs_you.toasts().len(), 1, "it toasts");
        assert!(view.chimed.is_empty(), "Claude is muted");
    });

    let button = cx.debug_bounds("toast-open-4").expect("`⏎ open`");
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    tick(cx);
    let on_box = shown(&root, cx);
    assert_ne!(on_box.entity_id(), first.entity_id(), "the window went");
    on_box.read_with(cx, |view, cx| {
        assert_eq!(
            view.bridge_on_screen(cx).map(|bridge| bridge.name()),
            Some("box")
        );
        assert_eq!(
            view.landing.as_ref().map(|(landing, _)| landing.clone()),
            Some(Landing::Alert(Alert {
                away: None,
                target: box_asked(thread, 1),
                thread,
            }))
        );
    });
}

/// Once its Cockpit is attached, the window opens what it came to the
/// machine for: the request, read, and its Thread focused.
#[gpui::test]
fn the_window_opens_what_it_came_for_once_attached(cx: &mut TestAppContext) {
    use crate::notifications::{Alert, RowTarget};
    let (core, fake) = cockpit("machines-landing", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let threads = view.read_with(cx, |view, cx| view.cockpit.read(cx).threads());
    let off = view.read_with(cx, |view, cx| {
        threads
            .iter()
            .position(|thread| Some(*thread) != view.focused_thread(cx))
            .unwrap()
    });
    fake.streams.borrow()[off].send(decision("there")).unwrap();
    tick(cx);
    let id = view.read_with(cx, |view, cx| {
        view.cockpit
            .read(cx)
            .notifications()
            .decisions()
            .next()
            .unwrap()
            .id
            .clone()
    });
    view.update(cx, |view, cx| {
        let until = cx.background_executor().now() + Duration::from_secs(10);
        view.landing = Some((
            Landing::Alert(Alert {
                away: None,
                target: RowTarget::Decision(id),
                thread: threads[off].thread(),
            }),
            until,
        ));
        // A window gets its landing as it opens, and its first pump reads
        // it: the notify rings for that pump (ADR 0041).
        cx.notify();
    });
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(cx), Some(threads[off]), "on its Thread");
        assert!(
            view.cockpit
                .read(cx)
                .notifications()
                .decisions()
                .next()
                .unwrap()
                .read
        );
        assert!(view.landing.is_none());
    });
}

/// What the window came for has resolved and its Thread was parked since:
/// the Thread comes back, focused. A landing past its wait opens nothing.
#[gpui::test]
fn a_landing_revives_a_parked_thread_and_lapses_unattached(cx: &mut TestAppContext) {
    use crate::notifications::{Alert, RowTarget};
    let (mut core, _fake) = cockpit("machines-landing-parked", 2);
    let threads = core.threads();
    core.park(threads[1]).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let gone = |thread: ThreadKey| {
        Landing::Alert(Alert {
            away: None,
            target: RowTarget::Notice(ferrite_core::notifications::NoticeId::from_u64(99)),
            thread: thread.thread(),
        })
    };
    view.update(cx, |view, cx| {
        let until = cx.background_executor().now() + Duration::from_secs(10);
        view.landing = Some((gone(threads[1]), until));
        // A window gets its landing as it opens, and its first pump reads
        // it: the notify rings for that pump (ADR 0041).
        cx.notify();
    });
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(cx), Some(threads[1]), "revived");
        assert!(view.landing.is_none());
    });

    view.update(cx, |view, cx| {
        let lapsed = cx.background_executor().now() - Duration::from_millis(1);
        view.landing = Some((gone(threads[0]), lapsed));
        cx.notify();
    });
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(cx), Some(threads[1]), "stays put");
        assert!(view.landing.is_none(), "dropped");
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
    cx.update(|cx| {
        crate::frame().bind_keys(
            ferrite_ui::keymap::bindings(ferrite_ui::keymap::PLATFORM),
            cx,
        )
    });
    let machines = cx.new(|_| machines_through("typed", Some("me@box"), serving));
    let folder = settings_dir("typed").join("src");
    let repo = repo_with_worktree_and_branches(&folder);
    std::fs::create_dir_all(folder.join(".re-hidden")).unwrap();
    std::fs::write(folder.join("readme.txt"), "").unwrap();
    let started = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    // `serving` dials it itself, to keep what the window sends.
    let _ = ferrite_server::fixture_machine(
        settings_dir("typed").join("box"),
        Box::new(OnBox(started.clone())),
    )
    .unwrap();
    let core = machines
        .update(cx, |machines, _| machines.cockpit())
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_machines(core, Provider::Claude, Preferences::ephemeral(), Some(machines), cx)
    });
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            matches!(
                view.cockpit.read(cx).server(),
                Some(ferrite_core::cockpit::ServerState::Attached { .. })
            )
        })
    });

    let field = view.update(cx, |view, cx| {
        view.open_project_creator(cx);
        let card = view.project_card(cx).unwrap();
        let field = card.read(cx).remote.as_ref();
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
        view.read_with(cx, |view, cx| {
            view.project_card(cx).unwrap().read(cx).remote_completions(cx) == ["repo"]
        })
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
        assert_eq!(
            view.project_card(cx).unwrap().read(cx).staged,
            std::slice::from_ref(&repo)
        );
        assert_eq!(field.read(cx).text(), "", "the field empties");
    });
    cx.simulate_keystrokes("enter");
    let project = view.update(cx, |view, cx| {
        assert!(
            view.project_card(cx).is_none(),
            "the next ↵ confirms the card, adding nothing more"
        );
        let project = view.cockpit.read(cx).registry().projects().last().unwrap().clone();
        assert_eq!(project.title, "repo");
        assert_eq!(project.root, ferrite_core::workspace::follow::canonical(&repo));
        project.id
    });

    let composer = view.update(cx, |view, cx| {
        if view.panes.is_empty() {
            view.open_draft(DraftTarget::Main, cx);
        }
        let draft = view.panes[view.focused(cx)].draft().unwrap();
        let offered = view
            .project_menu_rows(draft, cx)
            .into_iter()
            .any(|row| row.consequence == Consequence::Band(BandChoice::Project(project)));
        assert!(offered, "the machine's Project is on the draft's chip");
        let composer = view.panes[view.focused(cx)].composer.clone();
        view.pick_band(&BandChoice::Project(project), &composer, cx);
        composer
    });
    until(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.checkouts.branch(project).as_deref() == Some("trunk")
        })
    });
    view.update(cx, |view, cx| {
        assert!(
            !view.checkouts.asking_any(),
            "no git here was asked what the machine's checkout is on"
        );
        view.open_band_menu(pane::BandChip::Workspace, false, cx);
    });
    let rows = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.band_menu().map_or_else(Vec::new, |popover| {
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
            named("Current checkout", "trunk"),
            named("Previous worktree (feature/wt)", "wt-feature"),
            named("main", "existing branch"),
            named("topic/spare", "existing branch"),
            named("New branch", "in the project checkout"),
            named("New worktree", "in an isolated checkout"),
        ],
        "git on the machine lists the repo, its own worktrees and branches"
    );
    assert!(
        String::from_utf8_lossy(&SENT_TO_BOX.lock().unwrap()).contains(r#""op":"list_repo""#),
        "the machine's Server read the repo"
    );

    view.update(cx, |view, cx| {
        view.close_popup(cx);
        composer.update(cx, |composer, cx| composer.set("hello box".into(), cx));
        view.bootstrap_draft(cx);
    });
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.cockpit.read(cx).threads().len() == 1)
    });
    assert_eq!(
        *started.lock().unwrap(),
        [ferrite_core::workspace::follow::canonical(&repo)]
    );
}

/// A review beside a machine's Thread reads its checkout through the
/// machine's Server (`review_read`, `list_commits`, ADR 0021) — no git here
/// is asked — and its lines take comments as a local review's do: the
/// uncommitted work, then one of the branch's commits picked from the list.
#[gpui::test]
fn a_review_of_a_machines_thread_reads_its_checkout_there(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::frame().bind_keys(
            ferrite_ui::keymap::bindings(ferrite_ui::keymap::PLATFORM),
            cx,
        )
    });
    let machines = cx.new(|_| machines_through("reviewed", Some("me@box"), serving));
    let repo = repo_in(&settings_dir("reviewed").join("src"));
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&["branch", "-M", "main"]);
    git(&["checkout", "-q", "-b", "dev"]);
    std::fs::write(repo.join("nav.rs"), "fn row() {}\n").unwrap();
    git(&["add", "nav.rs"]);
    git(&["commit", "-q", "-m", "Tidy the nav"]);
    std::fs::write(repo.join("notes.md"), "fresh\n").unwrap();
    let _ = ferrite_server::fixture_machine(
        settings_dir("reviewed").join("box"),
        Box::new(OnBox(Default::default())),
    )
    .unwrap();
    let core = machines
        .update(cx, |machines, _| machines.cockpit())
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_machines(core, Provider::Claude, Preferences::ephemeral(), Some(machines), cx)
    });
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            matches!(
                view.cockpit.read(cx).server(),
                Some(ferrite_core::cockpit::ServerState::Attached { .. })
            )
        })
    });
    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.open(
                Provider::Claude,
                WorkspaceChoice::Main {
                    checkout: repo.clone(),
                },
            ))
            .unwrap();
        view.sync_panes(cx);
    });
    until(cx, |cx| {
        view.read_with(cx, |view, _| !view.panes.is_empty())
    });
    let open_review = |scope, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_beside(beside::Beside::Review(scope), window, cx)
            })
        });
    };
    let comments = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let composer = view.panes[0].composer.read(cx);
            composer
                .review_comments()
                .iter()
                .map(|comment| (comment.location(), comment.text.clone()))
                .collect::<Vec<_>>()
        })
    };

    open_review(ferrite_reviewr::model::Scope::Uncommitted, cx);
    until(cx, |cx| cx.debug_bounds("review-lines").is_some());
    assert!(
        String::from_utf8_lossy(&SENT_TO_BOX.lock().unwrap()).contains(r#""op":"review_read""#),
        "the machine's Server read the checkout"
    );
    cx.simulate_keystrokes("c");
    cx.simulate_input("why notes?");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(comments(cx), [("notes.md:1".into(), "why notes?".into())]);

    open_review(ferrite_reviewr::model::Scope::Commits, cx);
    until(cx, |cx| cx.debug_bounds("review-commit-0").is_some());
    cx.simulate_keystrokes("enter");
    until(cx, |cx| cx.debug_bounds("review-lines").is_some());
    assert!(cx.debug_bounds("review-commit-reviewed").is_some());
    cx.simulate_keystrokes("c");
    cx.simulate_input("keep it");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        comments(cx),
        [
            ("notes.md:1".into(), "why notes?".into()),
            ("nav.rs:1".into(), "keep it".into()),
        ],
        "a commit's lines take comments as the uncommitted work's do"
    );
}

/// The Server's word on `thread`'s Browser, as an attach answers it.
pub(super) fn served(cockpit: &mut Cockpit, thread: ThreadKey) -> ferrite_core::browser::State {
    let ask = cockpit.browser_attach(thread);
    cockpit.wait_for(ask).unwrap()
}

/// `action` on `thread`'s Browser, as the Server answers it.
pub(super) fn browsed(
    cockpit: &mut Cockpit,
    thread: ThreadKey,
    action: ferrite_core::browser::Action,
) -> ferrite_core::browser::State {
    let ask = cockpit.browser(thread, action);
    cockpit.wait_for(ask).unwrap()
}

/// A Cockpit attached to a Server whose browser window has one tab open on
/// a local page, and the Thread and tab document it belongs to.
pub(super) fn browser_core(name: &str) -> (Cockpit, ThreadKey, ferrite_core::browser::Document) {
    use ferrite_core::browser::Action;
    use ferrite_core::cockpit::{Attach, Endpoint, ServerState};
    let dir = scratch(name);
    let dial = ferrite_server::fixture_machine(dir.join("box"), Box::new(OnBox(Default::default()))).unwrap();
    let bridge = std::sync::Arc::new(Bridge::new("box", dir.join("bridge.sock"), dial));
    let mut core = Cockpit::attach(Attach {
        saved_as: Some(ferrite_core::MachineKey(Some("me@box".into()))),
        store: dir.join("window"), server: Endpoint::Machine(bridge),
        client: ferrite_core::protocol::Peer { name: name.into(), version: "0".into() },
        renderer: None, browser: true, driven: false, discovery: Box::new(Fake::default()),
    }).unwrap();
    ferrite_core::doorbell::until("browser window did not attach", || {
        core.pump(); matches!(core.server(), Some(ServerState::Attached { .. }))
    });
    let thread = core.open(Provider::Claude, WorkspaceChoice::Main { checkout: here() }).unwrap();
    served(&mut core, thread);
    let opened = browsed(&mut core, thread, Action::Open { url: "http://localhost:5173/start".into() });
    let document = opened.tabs[0].document();
    (core, thread, document)
}

/// The public Server/window path must show its exact native page, including
/// provider-initiated opens and navigation that finishes before the next paint.
#[gpui::test]
fn conversation_browser_shares_the_live_preview_and_new_tab_opens_separately(cx: &mut TestAppContext) {
    use ferrite_core::browser::Action;
    use ferrite_pane::visual::{engine::Visuals, web::fake::{Call, FakeEngine}};
    let (core, thread, document) = browser_core("browser-preview");
    bind_production_keys(cx);
    let engine = std::rc::Rc::new(FakeEngine::new());
    cx.update(|cx| Visuals::install(engine.clone(), cx));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    until(cx, |cx| engine.live_views() == 1 && cx.debug_bounds("browser-preview").is_some());
    view.update(cx, |view, cx| {
        let state = view.cockpit.update(cx, |cockpit, _| browsed(cockpit, thread, Action::Navigate { document, url: "http://localhost:5173/first".into() }));
        view.cockpit.update(cx, |cockpit, _| browsed(cockpit, thread, Action::Navigate { document: state.tabs[0].document(), url: "http://localhost:5173/final".into() }));
    });
    until(cx, |cx| view.update(cx, |view, cx| {
        let state = view.cockpit.update(cx, |cockpit, _| served(cockpit, thread));
        state.tabs[0].url == "http://localhost:5173/final" && !state.tabs[0].loading
    }));
    assert_eq!(engine.live_views(), 1, "navigation retains the page displayed by the preview");
    assert!(engine.journal().contains(&Call::BrowserNavigate("http://localhost:5173/final".into())));
    let new_tab = cx.debug_bounds("browser-new-tab").unwrap();
    cx.simulate_click(new_tab.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input("http://localhost:5173/second");
    cx.simulate_keystrokes("enter");
    until(cx, |cx| view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs.len()) == 2));
    let state = view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| served(cockpit, thread)));
    assert_eq!(state.tabs[0].url, "http://localhost:5173/final", "new tab does not navigate the existing page");
    assert_eq!(state.tabs[1].url, "http://localhost:5173/second");
    // The address acts on the second tab once the window itself has it:
    // until then Enter would open a third.
    super::browser_chrome::page_shown_at(&view, thread, "http://localhost:5173/second", cx);
    engine.defer_browser_navigation(true);
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("http://localhost:5173/delayed");
    cx.simulate_keystrokes("enter");
    until(cx, |_| engine.journal().contains(&Call::BrowserNavigate("http://localhost:5173/delayed".into())));
    tick(cx);
    let pending = view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| served(cockpit, thread)));
    assert_eq!(pending.tabs[1].url, "http://localhost:5173/delayed", "old native metadata must not overwrite an accepted navigation intent");
    assert!(pending.tabs[1].loading);
    assert_eq!(pending.tabs[1].dirty, ferrite_core::browser::Dirty::Unknown);
    engine.finish_browser_navigations();
    until(cx, |cx| view.update(cx, |view, cx| {
        let state = view.cockpit.update(cx, |cockpit, _| served(cockpit, thread));
        state.tabs[1].url == "http://localhost:5173/delayed" && !state.tabs[1].loading
    }));
    engine.defer_browser_navigation(false);
    // And on the page it went to once the window has that one.
    super::browser_chrome::page_shown_at(&view, thread, "http://localhost:5173/delayed", cx);
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("http://localhost:5173/delayed#section");
    cx.simulate_keystrokes("enter");
    until(cx, |cx| view.update(cx, |view, cx| {
        let state = view.cockpit.update(cx, |cockpit, _| served(cockpit, thread));
        state.tabs[1].url == "http://localhost:5173/delayed#section" && !state.tabs[1].loading
    }));
    let wait_protection = |cx: &mut gpui::VisualTestContext, expected: bool, stage: &str| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        until(cx, |cx| {
            let (state, active) = view.update(cx, |view, cx| {
                (view.cockpit.update(cx, |cockpit, _| served(cockpit, thread)), view.cockpit.read(cx).window_active())
            });
            assert!(std::time::Instant::now() < deadline,
                "{stage}: expected protected={expected}, active={active}, state={state:?}");
            state.tabs.iter().any(|tab| tab.protected) == expected
        });
    };
    let viewport = cx.debug_bounds("browser-preview").unwrap();
    let page_point = viewport.center();
    cx.simulate_mouse_down(page_point, gpui::MouseButton::Left, gpui::Modifiers::none());
    wait_protection(cx, true, "held pointer begins");
    for _ in 0..3 { tick(cx); }
    assert!(view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs.iter().any(|tab| tab.protected))),
        "a held gesture must not settle between pointer events");
    cx.simulate_mouse_up(viewport.origin - gpui::point(px(10.), px(10.)), gpui::MouseButton::Left, gpui::Modifiers::none());
    wait_protection(cx, false, "outside pointer release settles");
    cx.simulate_click(page_point, gpui::Modifiers::none());
    // GPUI's keystroke helper emits only KeyDown. Real typing also releases
    // each key; held keys deliberately keep manual interaction active.
    cx.simulate_keystrokes("a");
    cx.simulate_event(gpui::KeyUpEvent { keystroke: gpui::Keystroke::parse("a").unwrap() });
    tick(cx);
    cx.simulate_keystrokes("b");
    cx.simulate_event(gpui::KeyUpEvent { keystroke: gpui::Keystroke::parse("b").unwrap() });
    tick(cx);
    assert!(view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs.iter().any(|tab| tab.protected))),
        "a typing burst must remain protected between keys");
    wait_protection(cx, false, "typing burst settles");
    cx.simulate_mouse_down(page_point, gpui::MouseButton::Left, gpui::Modifiers::none());
    wait_protection(cx, true, "second gesture begins");
    let add_tab = cx.debug_bounds("browser-new-tab").unwrap().center();
    cx.simulate_click(add_tab, gpui::Modifiers::none());
    wait_protection(cx, false, "focus loss settles");
}

#[gpui::test]
fn remote_inbox_uses_digests_and_follows_another_clients_park(cx: &mut TestAppContext) {
    use ferrite_core::protocol::{ServerMessage, SharedRoster};
    let (core, _) = cockpit("remote-inbox", 1);
    let machines = cx.new(|_| machines("remote-inbox", None));
    let (root, cx) = machine_window(cx, core, machines.clone());
    let view = shown(&root, cx);
    let thread = ThreadId::new(1);
    let key = ferrite_core::MachineKey(Some("me@box".into()));
    hear_box(&machines, box_snapshot(&[(thread, "Review")], vec![]), cx);
    tick(cx);
    let remote = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.needs_you_rows(cx)
                .into_iter()
                .filter(|row| row.away.as_ref() == Some(&key))
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(remote(cx).len(), 1, "an idle remote Thread needs no Notice");
    hear_box(
        &machines,
        ServerMessage::Digest {
            thread,
            digest: ferrite_core::digest::Digest {
                background_work: true,
                ..Default::default()
            },
        },
        cx,
    );
    tick(cx);
    assert!(remote(cx).is_empty(), "background work is Working");
    hear_box(
        &machines,
        ServerMessage::Digest {
            thread,
            digest: Default::default(),
        },
        cx,
    );
    tick(cx);
    assert_eq!(remote(cx).len(), 1);
    hear_box(
        &machines,
        ServerMessage::Roster {
            revision: 2,
            roster: SharedRoster {
                open: vec![],
                parked: vec![thread],
            },
        },
        cx,
    );
    tick(cx);
    assert!(remote(cx).is_empty(), "another client's Park removes it");
}

#[gpui::test]
fn browser_live_annotation_keeps_focus_and_protection_until_guest_cleanup(cx: &mut TestAppContext) {
    use ferrite_core::browser::Action;
    use ferrite_core::cockpit::{Attach, Endpoint, ServerState};
    use ferrite_pane::visual::{
        engine::Visuals,
        live_annotation::{Event, Submission},
        web::{fake::{Call, FakeEngine}, Input},
    };
    use ferrite_core::browser_annotations::{LiveFeedback, LiveScreenshot, Rect, RegionTarget, Selection, Viewport};
    // Short: the machine's socket under it must fit macOS's 104-byte SUN_LEN.
    let dir = scratch("annotation-host");
    let dial = ferrite_server::fixture_machine(dir.join("box"), Box::new(OnBox(Default::default()))).unwrap();
    let bridge = std::sync::Arc::new(Bridge::new("box", dir.join("bridge.sock"), dial));
    let mut core = Cockpit::attach(Attach {
        saved_as: Some(ferrite_core::MachineKey(Some("me@box".into()))),
        store: dir.join("window"), server: Endpoint::Machine(bridge),
        client: ferrite_core::protocol::Peer { name: "browser-preview".into(), version: "0".into() },
        renderer: None, browser: true, driven: false, discovery: Box::new(Fake::default()),
    }).unwrap();
    ferrite_core::doorbell::until("browser window did not attach", || {
        core.pump(); matches!(core.server(), Some(ServerState::Attached { .. }))
    });
    let thread = core.open(Provider::Claude, WorkspaceChoice::Main { checkout: here() }).unwrap();
    served(&mut core, thread);
    browsed(&mut core, thread, Action::Open { url: "http://localhost:5173/start".into() });
    bind_production_keys(cx);
    let engine = std::rc::Rc::new(FakeEngine::new());
    cx.update(|cx| Visuals::install(engine.clone(), cx));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    // Native callbacks and remote retention use several threads. Bound the
    // whole condition: repeated socket replies must not reset its deadline.
    let wait = |cx: &mut gpui::VisualTestContext, label: &str, done: &mut dyn FnMut(&mut gpui::VisualTestContext) -> bool| {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            leap(cx, super::super::user_commands::ENDED_POLL);
            if done(cx) { break; }
            assert!(std::time::Instant::now() < deadline, "{label}; native journal: {:?}", engine.journal().into_iter().rev().take(24).collect::<Vec<_>>());
            std::thread::yield_now();
        }
    };
    wait(cx, "browser becomes visible", &mut |cx| engine.live_views() == 1 && cx.debug_bounds("browser-preview").is_some());
    wait(cx, "page finishes loading", &mut |cx| view.update(cx, |view, cx| {
        !view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs[0].loading)
    }));
    let finished = || engine.journal().iter().filter(|call| matches!(call, Call::AnnotationFinished(_))).count();
    let protected = |cx: &mut gpui::VisualTestContext| view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs[0].protected)
    });
    let begin = |cx: &mut gpui::VisualTestContext| {
        let annotate = cx.debug_bounds("browser-annotate").expect("live page offers annotation");
        cx.simulate_click(annotate.center(), gpui::Modifiers::none());
        tick(cx);
    };
    // The button acts once the window has the page as loaded, and the
    // editor starts once the Server has answered for the page's
    // protection: each a beat or more later.
    let started = || engine.journal().iter().filter(|call| matches!(call, Call::AnnotationStarted(..))).count();
    let begin_live = |cx: &mut gpui::VisualTestContext| {
        super::browser_chrome::page_shown(&view, thread, cx);
        let before = started();
        begin(cx);
        wait(cx, "the guest editor starts", &mut |_| started() > before);
    };

    begin_live(cx);
    assert!(protected(cx), "the live editor protects the exact page");
    assert!(cx.debug_bounds("browser-annotation-overlay").is_none(), "the editor lives in the guest page");
    assert!(cx.update(|window, cx| view.read(cx).browser_annotation_focus(thread, cx).is_some_and(|focus| focus.is_focused(window))), "starting annotation focuses the rendered WebView");
    cx.simulate_keystrokes("escape");
    wait(cx, "Escape reaches the guest", &mut |_| engine.journal().iter().any(|call| matches!(call, Call::Input(Input::Key { key, down: true, .. }) if key == "escape")));
    // simulate_keystrokes sends only key-down; a real release ends the
    // independent operator gesture after the annotation lease is released.
    cx.simulate_event(gpui::KeyUpEvent { keystroke: gpui::Keystroke::parse("escape").unwrap() });
    engine.annotation_event(Event::Cancelled);
    wait(cx, "Cancel requests guest cleanup", &mut |_| finished() == 1);
    assert!(protected(cx), "cancel waits for CSS restoration");
    engine.annotation_event(Event::Closed);
    wait(cx, "cancelled session releases protection", &mut |cx| !protected(cx));

    begin_live(cx);
    let viewport = engine.browser_viewport().unwrap();
    let bounds = Rect { x: 12., y: 20., width: 80., height: 60. };
    engine.annotation_event(Event::Attach {
        feedback: LiveFeedback {
            id: "live-region".into(),
            page_url: "http://localhost:5173/start".into(),
            page_title: Some("Fixture".into()),
            comment: "Keep this region even without a screenshot".into(),
            elements: vec![],
            regions: vec![RegionTarget { id: "region-1".into(), rect: bounds.clone() }],
            strokes: vec![],
            style_changes: vec![],
            screenshot: LiveScreenshot::Unavailable { reason: "Not captured yet".into() },
            bounds,
            created_at: "2026-11-16T00:00:00.000Z".into(),
        },
        viewport: Viewport {
            width_css: f64::from(viewport.width),
            height_css: f64::from(viewport.height),
            device_scale: f64::from(viewport.scale),
        },
        submission: Submission::Attach,
    });
    wait(cx, "Attach capture requests guest cleanup", &mut |_| finished() == 2);
    assert!(protected(cx), "Attach keeps its lease while the guest restores styles");
    assert!(cx.debug_bounds("browser-annotation-attachment").is_none(), "no receipt is exposed before cleanup");
    engine.annotation_event(Event::Closed);
    wait(cx, "cleaned capture reaches the composer", &mut |cx| cx.debug_bounds("browser-annotation-attachment").is_some());
    assert!(!protected(cx), "durable retention releases the lease");
    let annotations = view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).browser_annotations().to_vec());
    assert_eq!(annotations.len(), 1);
    assert!(annotations[0].original().is_none(), "an unavailable capture creates no fake image");
    assert_eq!(annotations[0].comment(), "Keep this region even without a screenshot");
    assert!(matches!(&annotations[0].capture().selection, Selection::Live { feedback } if feedback.regions[0].id == "region-1" && matches!(feedback.screenshot, LiveScreenshot::Unavailable { .. })));

    begin_live(cx);
    let new_tab = cx.debug_bounds("browser-new-tab").unwrap();
    cx.simulate_click(new_tab.center(), gpui::Modifiers::none());
    wait(cx, "New Tab requests guest cleanup", &mut |_| finished() == 3);
    assert!(protected(cx), "hiding the page still waits for restoration");
    engine.annotation_event(Event::Closed);
    wait(cx, "hidden session releases protection", &mut |cx| !protected(cx));
    let before = started();
    begin(cx);
    assert_eq!(started(), before, "new address mode has no visible page to annotate");
}

/// A window on a machine whose own `settings.json` is `there`, with its
/// Settings open and the machine's defaults read. Returns the machine's
/// settings directory too.
fn settings_on_a_machine<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    there: &ferrite_core::settings::Settings,
) -> (
    Entity<CockpitView>,
    &'a mut gpui::VisualTestContext,
    std::path::PathBuf,
) {
    use ferrite_core::cockpit::{Attach, Endpoint, ServerState};
    let dir = scratch(name);
    let remote = dir.join("r");
    std::fs::create_dir_all(&remote).unwrap();
    there.save(&remote).unwrap();
    let dial = ferrite_server::fixture_machine(
        remote.join("threads"),
        Box::new(OnBox(Default::default())),
    )
    .unwrap();
    let bridge = std::sync::Arc::new(Bridge::new(
        "box",
        dir.join("bridge").join("bridge.sock"),
        dial,
    ));
    let mut core = Cockpit::attach(Attach {
        saved_as: Some(ferrite_core::MachineKey(Some("me@box".into()))),
        browser: false,
        driven: false,
        store: dir.join("window").join("threads"),
        server: Endpoint::Machine(bridge),
        client: ferrite_core::protocol::Peer {
            name: "window".into(),
            version: "0".into(),
        },
        renderer: None,
        discovery: Box::new(Fake::default()),
    })
    .unwrap();
    ferrite_core::doorbell::until("client did not attach", || {
        core.pump();
        matches!(core.server(), Some(ServerState::Attached { .. }))
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    bind_production_keys(cx);
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.shown_defaults(cx).is_some())
    });
    tick(cx);
    (view, cx, remote)
}

/// Settings on a machine show and edit that machine's own defaults: the
/// Server writes its file and the sheet shows what it reread, while this
/// computer's preferences stay as they were.
#[gpui::test]
fn settings_on_a_machine_edit_its_defaults_not_this_computers(cx: &mut TestAppContext) {
    use ferrite_core::settings::Settings;
    let there = Settings {
        factory_tasks_at_once: 5,
        placeholder_suggestions: false,
        codex_approval_policy: "never".into(),
        ..Default::default()
    };
    let (view, cx, remote) = settings_on_a_machine(cx, "rd", &there);
    let here = view.read_with(cx, |view, _| view.prefs.settings.clone());
    assert_ne!(here.factory_tasks_at_once, there.factory_tasks_at_once);
    let shown = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| view.shown_defaults(cx).unwrap())
    };
    assert_eq!(shown(cx), there.server_defaults(), "the machine's, not ours");

    let search = cx.debug_bounds("settings-search").unwrap();
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_input("at once");
    tick(cx);
    let at = cx.debug_bounds("settings-factory-limit-1").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            view.shown_defaults(cx)
                .is_some_and(|shown| shown.factory_tasks_at_once == 6)
        })
    });
    let search = cx.debug_bounds("settings-search").unwrap();
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_keystrokes(&ferrite_ui::keymap::PLATFORM.chord("a"));
    cx.simulate_input("follow-up");
    tick(cx);
    let at = cx
        .debug_bounds("settings-placeholder-suggestions")
        .unwrap()
        .center();
    cx.simulate_click(at, gpui::Modifiers::none());
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            view.shown_defaults(cx)
                .is_some_and(|shown| shown.placeholder_suggestions)
        })
    });

    until(cx, |_| {
        Settings::load(&remote).is_ok_and(|written| written.placeholder_suggestions)
    });
    let written = Settings::load(&remote).unwrap();
    assert_eq!(written.factory_tasks_at_once, 6, "written to its file");
    assert!(written.placeholder_suggestions);
    assert_eq!(
        written.codex_approval_policy, "never",
        "what the edit did not name stays"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.prefs.settings.clone()),
        here,
        "this computer's preferences did not move"
    );
    assert!(
        Settings::load(&view.read_with(cx, |view, _| view.prefs.dir.clone()))
            .map_or(true, |saved| saved.factory_tasks_at_once == here.factory_tasks_at_once)
    );
}

/// Two quick edits both land: the second is built on the first, not on what
/// the machine last said.
#[gpui::test]
fn two_quick_edits_to_a_machines_defaults_both_land(cx: &mut TestAppContext) {
    use ferrite_core::settings::Settings;
    let there = Settings {
        factory_tasks_at_once: 5,
        ..Default::default()
    };
    let (view, cx, remote) = settings_on_a_machine(cx, "rd-quick", &there);
    let search = cx.debug_bounds("settings-search").unwrap();
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_input("at once");
    tick(cx);
    let more = cx.debug_bounds("settings-factory-limit-1").unwrap().center();
    cx.simulate_click(more, gpui::Modifiers::none());
    cx.simulate_click(more, gpui::Modifiers::none());
    until(cx, |_| {
        Settings::load(&remote).is_ok_and(|written| written.factory_tasks_at_once == 7)
    });
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.factory_limit(cx)) == Some(7)
    });
}

/// A machine that cannot write its file says so in both groups that show
/// its defaults, beside the values it kept.
#[cfg(unix)]
#[gpui::test]
fn a_machine_that_cannot_save_its_defaults_says_so(cx: &mut TestAppContext) {
    use ferrite_core::settings::Settings;
    use std::os::unix::fs::PermissionsExt;
    let there = Settings {
        factory_tasks_at_once: 5,
        ..Default::default()
    };
    let (view, cx, remote) = settings_on_a_machine(cx, "rd-readonly", &there);
    std::fs::set_permissions(&remote, std::fs::Permissions::from_mode(0o555)).unwrap();
    let search = cx.debug_bounds("settings-search").unwrap();
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_input("at once");
    tick(cx);
    let more = cx.debug_bounds("settings-factory-limit-1").unwrap().center();
    cx.simulate_click(more, gpui::Modifiers::none());
    let says = |group: &super::settings::Group| {
        group
            .rows
            .iter()
            .any(|row| row.words.iter().any(|word| word.contains("Not saved on box")))
    };
    let unsaved = |cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| {
            (
                says(&view.machine_group(cx)),
                view.permission_groups(cx).iter().any(says),
            )
        })
    };
    until(cx, |cx| unsaved(cx).0);
    assert!(unsaved(cx).1, "the Permissions page says it too");
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.factory_limit(cx)) == Some(5)
    });
    std::fs::set_permissions(&remote, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn says_in_machine_group(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
    words: &str,
) -> bool {
    view.update(cx, |view, cx| {
        view.machine_group(cx)
            .rows
            .iter()
            .any(|row| row.words.iter().any(|word| word.contains(words)))
    })
}

/// A read that failed and then lands shows the values, not a refused edit.
#[cfg(unix)]
#[gpui::test]
fn a_failed_read_that_later_lands_is_not_an_unsaved_edit(cx: &mut TestAppContext) {
    use ferrite_core::settings::Settings;
    use std::os::unix::fs::PermissionsExt;
    let (view, cx, remote) = settings_on_a_machine(cx, "rd-reread", &Settings::default());
    let file = remote.join(Settings::FILE);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    view.update(cx, |view, cx| {
        view.environment.defaults = None;
        view.ask_server_defaults(cx);
    });
    until(cx, |cx| says_in_machine_group(&view, cx, "Could not read them"));
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    view.update(cx, |view, cx| view.ask_server_defaults(cx));
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.shown_defaults(cx).is_some())
    });
    tick(cx);
    assert!(!says_in_machine_group(&view, cx, "Not saved"));
}

/// An edit that saves after one was refused takes the refusal away.
#[cfg(unix)]
#[gpui::test]
fn a_saved_edit_clears_an_earlier_refusal(cx: &mut TestAppContext) {
    use ferrite_core::settings::Settings;
    use std::os::unix::fs::PermissionsExt;
    let there = Settings {
        factory_tasks_at_once: 5,
        ..Default::default()
    };
    let (view, cx, remote) = settings_on_a_machine(cx, "rd-resave", &there);
    std::fs::set_permissions(&remote, std::fs::Permissions::from_mode(0o555)).unwrap();
    let search = cx.debug_bounds("settings-search").unwrap();
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_input("at once");
    tick(cx);
    let more = cx.debug_bounds("settings-factory-limit-1").unwrap().center();
    cx.simulate_click(more, gpui::Modifiers::none());
    until(cx, |cx| says_in_machine_group(&view, cx, "Not saved on box"));
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.factory_limit(cx)) == Some(5)
    });
    std::fs::set_permissions(&remote, std::fs::Permissions::from_mode(0o755)).unwrap();
    cx.simulate_click(more, gpui::Modifiers::none());
    until(cx, |_| {
        Settings::load(&remote).is_ok_and(|written| written.factory_tasks_at_once == 6)
    });
    until(cx, |cx| !says_in_machine_group(&view, cx, "Not saved"));
}

fn machine_with_edited_remote_file<'a>(
    name: &str,
    selected: bool,
    unsaved: bool,
    cx: &'a mut TestAppContext,
) -> (
    Entity<gpui::component::Root>,
    Entity<CockpitView>,
    Entity<crate::cockpit::tools::Editor>,
    &'a mut gpui::VisualTestContext,
) {
    use ferrite_core::file_operations::{fingerprint, Reply, Request};
    let (core, _fake, checkout) = bound_cockpit(name, Provider::Codex);
    let machines = cx.new(|_| {
        if unsaved {
            let dir = settings_dir(name);
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
                    driven: false,
                    defaults: Default::default(),
                    bridge: needs_sign_in,
                },
                &[],
                Some("me@box"),
            )
        } else {
            machines(name, selected.then_some("me@box"))
        }
    });
    let (root, cx) = machine_window(cx, core, machines);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let view = shown(&root, cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        view.prefs.settings.machines = if unsaved {
            Vec::new()
        } else {
            vec![Machine {
                name: "box".into(),
                target: "me@box".into(),
                enabled: true,
            }]
        };
        view.prefs.settings.save(&view.prefs.dir).unwrap();
        view.remote_files_for_test(
            thread,
            |request| {
                Ok(match request {
                    Request::Read { .. } => Reply::Text {
                        text: "remote original".into(),
                        fingerprint: fingerprint(b"remote original"),
                    },
                    Request::Write { text, .. } => Reply::Written {
                        fingerprint: fingerprint(text.as_bytes()),
                    },
                    _ => Reply::Unknown,
                })
            },
            cx,
        );
        view.open_in_files(thread, checkout.join("edited.rs"), None, true, cx);
    });
    tick(cx);
    let editor = view.read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread).unwrap());
    editor.update(cx, |editor, cx| editor.type_text("edited ", cx));
    (root, view, editor, cx)
}

#[gpui::test]
fn remote_files_block_machine_switch_until_the_save_has_finished(cx: &mut TestAppContext) {
    let (root, view, editor, cx) =
        machine_with_edited_remote_file("remote-files-leave", false, false, cx);
    view.update(cx, |_, cx| {
        cx.emit(GoToMachine {
            index: Some(1),
            open: None,
        })
    });
    tick(cx);
    assert_eq!(
        shown(&root, cx),
        view,
        "the dirty editor stays in its window"
    );
    assert!(editor.read_with(cx, |editor, _| editor.is_dirty()));
    editor.update(cx, |editor, cx| editor.save(cx));
    assert!(matches!(
        editor.read_with(cx, |editor, _| editor.save_state().clone()),
        crate::cockpit::tools::SaveState::Saving(_)
    ));
    assert!(
        view.read_with(cx, |view, cx| view.files_leave_error(cx))
            .is_some(),
        "an in-flight save still blocks leaving"
    );
    tick(cx);
    assert!(!editor.read_with(cx, |editor, _| editor.is_dirty()));
    view.update(cx, |_, cx| {
        cx.emit(GoToMachine {
            index: Some(1),
            open: None,
        })
    });
    tick(cx);
    assert_ne!(
        shown(&root, cx),
        view,
        "a confirmed save permits the switch"
    );
}

#[gpui::test]
fn remote_files_block_disconnect_and_remove_before_settings_or_selection_change(
    cx: &mut TestAppContext,
) {
    for (name, remove, unsaved) in [
        ("disconnect", false, false),
        ("remove", true, false),
        ("forget-unsaved", true, true),
    ] {
        let (root, view, editor, cx) =
            machine_with_edited_remote_file(&format!("remote-files-{name}"), true, unsaved, cx);
        let id = if remove {
            "connection-remove-1"
        } else {
            "connection-toggle-1"
        };
        let action = connection_act(&view, id, cx);
        cx.update(|window, cx| action(window, cx));
        let action = if remove {
            connection_removal(&view, cx)
        } else {
            action
        };
        cx.update(|window, cx| action(window, cx));
        tick(cx);
        assert_eq!(shown(&root, cx), view, "{name} preserves the dirty editor");
        view.read_with(cx, |view, cx| {
            assert!(editor.read(cx).is_dirty(), "{name} preserves the unsaved buffer");
            assert_eq!(
                view.prefs.settings.machines,
                ferrite_core::settings::Settings::load(&view.prefs.dir)
                    .unwrap()
                    .machines
            );
            assert_eq!(
                view.machines
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .selected_key()
                    .unwrap()
                    .0
                    .as_deref(),
                Some("me@box")
            );
            if !unsaved {
                assert!(view.prefs.settings.machines[0].enabled);
            } else {
                assert!(view.machines.as_ref().unwrap().read(cx).unsaved().is_some());
            }
        });
        editor.update(cx, |editor, cx| editor.save(cx));
        tick(cx);
        cx.update(|window, cx| action(window, cx));
        tick(cx);
        assert_ne!(shown(&root, cx), view, "{name} can leave after saving");
        let next = shown(&root, cx);
        next.read_with(cx, |view, _| {
            if remove {
                assert!(view.prefs.settings.machines.is_empty());
            } else {
                assert!(!view.prefs.settings.machines[0].enabled);
            }
        });
    }
}

#[gpui::test]
fn local_environment_switch_persists_and_empty_client_blocks_new_threads(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let machines = cx.new(|_| {
        let mut machines = machines("local-environment-toggle", None);
        machines.set_local_enabled(false);
        machines.settings_changed(&[]);
        machines
    });
    let core = machines
        .update(cx, |machines, _| machines.go_to(None))
        .unwrap()
        .unwrap();
    let (root, cx) = machine_window(cx, core, machines);
    let dir = settings_dir("local-environment-toggle-prefs");
    shown(&root, cx).update(cx, |view, _| {
        view.prefs.dir = dir.clone();
        view.prefs.settings.local_environment_enabled = false;
        view.prefs.settings.save(&dir).unwrap();
    });
    tick(cx);
    assert!(cx.debug_bounds("no-environment").is_some());
    cx.simulate_keystrokes("cmd-n");
    cx.simulate_keystrokes("cmd-shift-n");
    tick(cx);
    shown(&root, cx).read_with(cx, |view, cx| {
        assert!(view.panes.is_empty());
        assert!(
            view.elsewhere(view.on_screen(cx), cx),
            "empty client cannot use local file fallbacks"
        );
        assert!(view.shown_defaults(cx).is_none());
    });
    for enabled in [true, false] {
        let toggle = connection_act(&shown(&root, cx), "connection-toggle-local", cx);
        cx.update(|window, cx| toggle(window, cx));
        tick(cx);
        assert_eq!(
            ferrite_core::settings::Settings::load(&dir)
                .unwrap()
                .local_environment_enabled,
            enabled
        );
        shown(&root, cx).read_with(cx, |view, cx| {
            assert_eq!(view.cockpit.read(cx).environment_available(), enabled);
            assert_eq!(view.prefs.settings.local_environment_enabled, enabled);
        });
    }
    assert!(cx.debug_bounds("no-environment-connections").is_some());
}

/// The empty board's `Open Connections` opens Settings as ⌘, does: it
/// asks the machine for its defaults, which land or say why not.
#[gpui::test]
fn open_connections_asks_the_machines_defaults(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let machines = cx.new(|_| {
        let mut machines = machines("open-connections", None);
        machines.set_local_enabled(false);
        machines.settings_changed(&[]);
        machines
    });
    let core = machines
        .update(cx, |machines, _| machines.go_to(None))
        .unwrap()
        .unwrap();
    let (root, cx) = machine_window(cx, core, machines);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let view = shown(&root, cx);
    view.update(cx, |view, cx| {
        view.prefs.settings.local_environment_enabled = false;
        view.hide_sheet(cx);
    });
    tick(cx);
    let open = cx.debug_bounds("no-environment-connections").expect("Open Connections");
    cx.simulate_click(open.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.settings_open(cx), "Settings opens");
        assert!(view.elsewhere(view.on_screen(cx), cx));
        assert!(
            view.environment.defaults.is_some() || view.environment.defaults_error.is_some(),
            "the machine's defaults were asked"
        );
    });
}

#[gpui::test]
fn local_environment_switch_preserves_unreadable_settings(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("local-toggle-unreadable", 1);
    let prefs = Preferences {
        unreadable: Some("expected value at line 1 column 1".into()),
        ..Preferences::ephemeral()
    };
    let path = prefs.dir.join(ferrite_core::settings::Settings::FILE);
    std::fs::create_dir_all(&prefs.dir).unwrap();
    std::fs::write(&path, b"not json").unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Codex, prefs, cx)
    });
    let toggle = connection_act(&view, "connection-toggle-local", cx);
    cx.update(|window, cx| toggle(window, cx));
    view.read_with(cx, |view, _| {
        assert!(view.prefs.settings.local_environment_enabled);
    });
    assert_eq!(std::fs::read(&path).unwrap(), b"not json");
}
