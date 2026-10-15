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

/// ADR 0026 with ADR 0017: a Decision on a machine off screen toasts and
/// chimes once, names its machine, and with the window in the background
/// posts a system notification. What box held when first heard is not
/// news. Clicking the notification puts the window on box, which opens
/// the request once it is attached.
#[gpui::test]
fn a_decision_off_screen_rings_once_and_its_click_goes_to_its_machine(cx: &mut TestAppContext) {
    use crate::notifications::{Alert, Detail, RowTarget};
    use ferrite_core::protocol::{
        ServerMessage, SharedRoster, Snapshot, WireDecision, WireDecisionKind, WireDelivery,
        WireHandle, WireNotice, WirePolicy, WireSubject,
    };
    let (core, _fake) = cockpit("machines-alert", 1);
    let machines = cx.new(|_| machines("alert", None));
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (root, cx) = machine_window(cx, core, machines.clone());
    let first = shown(&root, cx);
    let thread = ThreadId::new(1);
    let hear = |message: ServerMessage, cx: &mut gpui::VisualTestContext| {
        machines.update(cx, |machines, _| {
            machines.status_mut(1).unwrap().fixture_receive(message)
        });
    };
    let handle = |serial: u64| WireHandle {
        generation: 1,
        serial,
        request_id: format!("r{serial}"),
    };
    let request = |serial: u64| WireNotice::Request {
        thread,
        handle: handle(serial),
        subject: Some(WireSubject::Main),
        question: false,
        read: false,
        at_ms: 0,
        seq: serial,
        dismissed: false,
    };
    hear(
        ServerMessage::Snapshot(Snapshot {
            boot_id: "box".into(),
            revision: 1,
            threads: vec![box_thread(thread, "Release")],
            roster: SharedRoster {
                open: vec![thread],
                parked: vec![],
            },
            decisions: vec![],
            groups: None,
            registry: None,
            notices: Some(vec![request(1)]),
            providers: None,
        }),
        cx,
    );
    ring(cx);
    first.read_with(cx, |view, _| {
        assert!(view.bell.toasts().is_empty(), "held when first heard");
        assert!(view.chimed.is_empty());
    });

    cx.deactivate_window();
    hear(
        ServerMessage::Decision {
            thread,
            subject: Some(WireSubject::Main),
            handle: handle(2),
            decision: WireDecision {
                id: "r2".into(),
                delivery: WireDelivery::Blocking,
                kind: WireDecisionKind::Approval,
                policy: WirePolicy {
                    allow: true,
                    deny: true,
                    prefer_deny: false,
                    interaction_required: false,
                },
                tool_use_id: "toolu_2".into(),
                tool_name: "Bash".into(),
                description: "Publish".into(),
                input: serde_json::json!({ "command": "cargo publish --dry-run" }),
                suggestions: vec![],
            },
        },
        cx,
    );
    hear(
        ServerMessage::Notices {
            revision: 2,
            notices: vec![request(1), request(2)],
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
    let asked = RowTarget::Decision(ferrite_core::notifications::DecisionNoticeId {
        thread,
        handle: (&handle(2)).into(),
    });
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
    let tag = system_tag(Some(&key), thread);
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
    on_box.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.machine().map(|bridge| bridge.name()),
            Some("box")
        );
        assert_eq!(
            view.landing,
            Some(Alert {
                away: None,
                target: asked,
                thread,
            }),
            "opened once box answers"
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
    view.update(cx, |view, _| {
        view.landing = Some(Alert {
            away: None,
            target: RowTarget::Decision(id),
            thread: threads[off],
        })
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
