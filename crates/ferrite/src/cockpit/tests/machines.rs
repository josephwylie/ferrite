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

/// `box`'s Server runs in this process (`server::fixture_machine`), its
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
    first.update(cx, |_, cx| {
        cx.emit(GoToMachine {
            index: 1,
            open: None,
        })
    });
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
    on_box.update(cx, |_, cx| {
        cx.emit(GoToMachine {
            index: 0,
            open: None,
        })
    });
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

/// A machine's parked Threads are its Server's to list: its logs stay on
/// it. They show in the nav's Parked section, called what the machine's
/// log calls them, and a click revives one there.
#[gpui::test]
fn a_machines_parked_threads_show_in_the_nav_and_revive_on_click(cx: &mut TestAppContext) {
    use ferrite_core::cockpit::{Attach, Endpoint, ServerState};
    let dir = scratch("machine-parked");
    let dial =
        ferrite_core::server::fixture_machine(dir.join("box"), Box::new(OnBox(Default::default())))
            .unwrap();
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
        model: None,
        effort: None,
        title: Some(title.into()),
        workspace: None,
        session_project_root: None,
        first_prompt_sent: true,
        has_prompt_history: true,
        starting: false,
        working: false,
        tuning: false,
        native_queue_pending: false,
        queued: vec![],
        suggestion: None,
        commands: vec![],
        models: vec![],
        permission_mode: None,
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
fn box_request(thread: ThreadId, serial: u64) -> ferrite_core::protocol::WireNotice {
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

/// What the bell calls box's Decision `serial` on `thread`.
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
    machines.update(cx, |machines, _| {
        machines.status_mut(1).unwrap().fixture_receive(message)
    });
}

/// ADR 0026 with ADR 0017: a Decision on a machine off screen toasts and
/// chimes once, names its machine, and with the window in the background
/// posts a system notification. What box held when first heard is not
/// news. Clicking the notification puts the window on box, which opens
/// the request once it is attached, and whose bell has heard box already.
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
    first.read_with(cx, |view, _| {
        assert!(view.bell.toasts().is_empty(), "held when first heard");
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
    first.read_with(cx, |view, _| {
        assert!(view.bell.toasts().is_empty(), "it waits its second")
    });
    ring(cx);
    ring(cx);
    let key = crate::machines::MachineKey(Some("me@box".into()));
    let asked = box_asked(thread, 2);
    let alert = Alert {
        away: Some(key.clone()),
        target: asked.clone(),
        thread,
    };
    first.read_with(cx, |view, cx| {
        assert_eq!(view.chimed, [Sound::NeedsYou], "once");
        let toasts = view.bell.toasts();
        assert_eq!(toasts.len(), 1);
        assert_eq!(toasts[0].alert, alert);
        let rows = view.bell_rows(cx);
        let row = rows.iter().find(|row| row.target == asked).unwrap();
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
        [tag.clone()]
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
            view.cockpit.machine().map(|bridge| bridge.name()),
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
        view.bell.hear(None, [], held, now);
        let ring = view.bell.ring(
            now + crate::notifications::ALERT_DELAY,
            &|_| Some(true),
            &|_| false,
        );
        assert!(ring.rang.is_empty(), "nothing rings twice");
        assert_eq!(
            view.bell
                .toasts()
                .iter()
                .map(|toast| &toast.alert.away)
                .collect::<Vec<_>>(),
            [&None],
            "its toast came along, on screen now"
        );
    });
}

/// The bell moves with the window: a system notification posted about
/// this computer while the window was on it still opens, from box, on
/// this computer's Thread.
#[gpui::test]
fn a_system_notification_outlives_a_move(cx: &mut TestAppContext) {
    use crate::machines::MachineKey;
    use crate::notifications::Alert;
    let (core, fake) = cockpit("machines-posted", 1);
    let machines = cx.new(|_| machines("posted", None));
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (root, cx) = machine_window(cx, core, machines.clone());
    let first = shown(&root, cx);
    let here = first.read_with(cx, |view, _| view.cockpit.threads()[0]);
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
    let home_tag = system_tag(&MachineKey(None), here);
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
    home.read_with(cx, |view, _| {
        assert!(view.cockpit.machine().is_none(), "this computer");
        assert_eq!(
            view.landing
                .as_ref()
                .and_then(|(landing, _)| match landing {
                    Landing::Alert(Alert { away, thread, .. }) => Some((away.clone(), *thread)),
                    Landing::Thread(_) => None,
                }),
            Some((None, here)),
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
    first.update(cx, |view, _| {
        view.prefs
            .settings
            .set_provider_sounds(Provider::Claude, false)
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
    first.read_with(cx, |view, _| {
        assert_eq!(view.bell.toasts().len(), 1, "it toasts");
        assert!(view.chimed.is_empty(), "Claude is muted");
    });

    let button = cx.debug_bounds("toast-open-4").expect("`⏎ open`");
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    tick(cx);
    let on_box = shown(&root, cx);
    assert_ne!(on_box.entity_id(), first.entity_id(), "the window went");
    on_box.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.machine().map(|bridge| bridge.name()),
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
    let threads = view.read_with(cx, |view, _| view.cockpit.threads());
    let off = view.read_with(cx, |view, _| {
        threads
            .iter()
            .position(|thread| Some(*thread) != view.focused_thread())
            .unwrap()
    });
    fake.streams.borrow()[off].send(decision("there")).unwrap();
    tick(cx);
    let id = view.read_with(cx, |view, _| {
        view.cockpit
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
                thread: threads[off],
            }),
            until,
        ))
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused_thread(), Some(threads[off]), "on its Thread");
        assert!(
            view.cockpit
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
    let gone = |thread: ThreadId| {
        Landing::Alert(Alert {
            away: None,
            target: RowTarget::Notice(ferrite_core::notifications::NoticeId::from_u64(99)),
            thread,
        })
    };
    view.update(cx, |view, cx| {
        let until = cx.background_executor().now() + Duration::from_secs(10);
        view.landing = Some((gone(threads[1]), until));
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused_thread(), Some(threads[1]), "revived");
        assert!(view.landing.is_none());
    });

    view.update(cx, |view, cx| {
        let lapsed = cx.background_executor().now() - Duration::from_millis(1);
        view.landing = Some((gone(threads[0]), lapsed));
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused_thread(), Some(threads[1]), "stays put");
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
    cx.update(|cx| crate::bind_keys(crate::keymap::bindings(crate::keymap::PLATFORM), cx));
    let machines = cx.new(|_| machines_through("typed", Some("me@box"), serving));
    let folder = settings_dir("typed").join("src");
    let repo = repo_with_worktree_and_branches(&folder);
    std::fs::create_dir_all(folder.join(".re-hidden")).unwrap();
    std::fs::write(folder.join("readme.txt"), "").unwrap();
    let started = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    // `serving` dials it itself, to keep what the window sends.
    let _ = ferrite_core::server::fixture_machine(
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

/// A review beside a machine's Thread reads its checkout through the
/// machine's Server (`review_diff`, `list_commits`, ADR 0021) — no git here
/// is asked — and its lines take comments as a local review's do: the
/// uncommitted work, then one of the branch's commits picked from the list.
#[gpui::test]
fn a_review_of_a_machines_thread_reads_its_checkout_there(cx: &mut TestAppContext) {
    cx.update(|cx| crate::bind_keys(crate::keymap::bindings(crate::keymap::PLATFORM), cx));
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
    let _ = ferrite_core::server::fixture_machine(
        settings_dir("reviewed").join("box"),
        Box::new(OnBox(Default::default())),
    )
    .unwrap();
    let core = machines
        .update(cx, |machines, _| machines.cockpit())
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new(core, cx).with_machines(machines)
    });
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    until(cx, |cx| {
        view.read_with(cx, |view, _| {
            matches!(
                view.cockpit.server(),
                Some(ferrite_core::cockpit::ServerState::Attached { .. })
            )
        })
    });
    view.update(cx, |view, cx| {
        view.cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::Main {
                    checkout: repo.clone(),
                },
            )
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

    open_review(ferrite_core::review::Scope::Uncommitted, cx);
    until(cx, |cx| cx.debug_bounds("review-lines").is_some());
    assert!(
        String::from_utf8_lossy(&SENT_TO_BOX.lock().unwrap()).contains(r#""op":"review_diff""#),
        "the machine's Server read the checkout"
    );
    cx.simulate_keystrokes("c");
    cx.simulate_input("why notes?");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(comments(cx), [("notes.md:1".into(), "why notes?".into())]);

    open_review(ferrite_core::review::Scope::Commits, cx);
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

/// A Projects row's ↵ on a Thread lands on it where it runs: here when
/// the record names no machine, else the window goes to the machine the
/// record names and lands there once attached.
#[gpui::test]
fn a_projects_jump_lands_on_its_thread_on_its_machine(cx: &mut TestAppContext) {
    use crate::projects::Jump;
    let (core, _fake) = cockpit("machines-jump", 2);
    let machines = cx.new(|_| machines("jump", None));
    let (root, cx) = machine_window(cx, core, machines);
    let first = shown(&root, cx);
    let here = first.read_with(cx, |view, _| {
        let threads = view.cockpit.threads();
        *threads
            .iter()
            .find(|thread| Some(**thread) != view.focused_thread())
            .unwrap()
    });
    first.update_in(cx, |view, window, cx| {
        view.jump_to(Jump::of("", &here.to_string()).unwrap(), window, cx)
    });
    tick(cx);
    assert_eq!(shown(&root, cx).entity_id(), first.entity_id(), "stays");
    first.read_with(cx, |view, _| assert_eq!(view.focused_thread(), Some(here)));

    first.update_in(cx, |view, window, cx| {
        view.jump_to(Jump::of("box", "7").unwrap(), window, cx)
    });
    tick(cx);
    let on_box = shown(&root, cx);
    assert_ne!(on_box.entity_id(), first.entity_id(), "the window went");
    on_box.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.machine().map(|bridge| bridge.name()),
            Some("box")
        );
        assert_eq!(
            view.landing.as_ref().map(|(landing, _)| landing.clone()),
            Some(Landing::Thread(ThreadId::new(7))),
            "on its Thread once box answers"
        );
    });
}
