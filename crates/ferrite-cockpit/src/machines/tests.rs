//! Which machines the window holds. Nothing here connects: a machine's
//! link attaches only when polled.

use super::*;

fn reach(name: &str) -> Reach {
    let dir = std::env::temp_dir().join(format!("fmach-{}-{name}", std::process::id()));
    Reach {
        settings_dir: dir.clone(),
        store: dir.join("threads"),
        executable: "/bin/false".into(),
        client: Peer {
            name: "test".into(),
            version: "0".into(),
        },
        renderer: None,
        driven: false,
        defaults: Arc::default(),
        bridge: Bridge::ssh,
    }
}

fn saved() -> Vec<Machine> {
    vec![
        Machine::new("me@build"),
        Machine {
            name: "gpu".into(),
            target: "me@gpu".into(),
            enabled: false,
        },
    ]
}

/// Which entries the Cockpit holds.
fn holding(machines: &Machines) -> Vec<bool> {
    machines
        .entries
        .iter()
        .map(|entry| entry.held.is_some())
        .collect()
}

#[test]
fn a_store_that_cannot_be_read_builds_no_cockpit() {
    let reach = reach("unbuilt");
    // A file where this computer's Threads would go.
    std::fs::create_dir_all(&reach.settings_dir).unwrap();
    std::fs::write(&reach.store, "").unwrap();
    let mut machines = Machines::new(reach, &saved(), None, true);
    assert!(machines.cockpit().is_err());
}

#[test]
fn this_computer_alone_lists_no_machines() {
    let mut machines = Machines::new(reach("alone"), &[], None, true);
    let cockpit = machines.cockpit().unwrap();
    assert!(!machines.listed());
    assert_eq!(holding(&machines), [true]);
    assert_eq!(cockpit.machines().count(), 1);
    assert_eq!(cockpit.this_computer(), machines.held(0));
}

#[test]
fn every_enabled_machine_is_held_at_once() {
    let mut machines = Machines::new(reach("every"), &saved(), None, true);
    let cockpit = machines.cockpit().unwrap();
    assert!(machines.listed());
    assert_eq!(holding(&machines), [true, true, false]);
    assert_eq!(cockpit.machines().count(), 2);
    let rows = machines.rows(&cockpit);
    assert_eq!(rows[0].name, LOCAL);
    assert_eq!(rows[1].connection, Connection::Connecting, "not polled yet");
    assert_eq!(rows[2].connection, Connection::Disabled);
    assert_eq!(
        cockpit.machine_saved_as(&MachineKey(Some("me@build".into()))),
        machines.held(1),
        "held under the name its saved files use"
    );
}

#[test]
fn a_driven_window_answers_only_the_first_machine_enabled() {
    let driven = |name| Reach {
        driven: true,
        ..reach(name)
    };
    let mut machines = Machines::new(driven("driven"), &saved(), None, true);
    assert!(machines.attach(0).driven);
    assert!(!machines.attach(1).driven, "a paired machine's API");

    let mut machines = Machines::new(driven("driven-off"), &saved(), None, false);
    assert!(machines.attach(1).driven, "with this computer off");
}

#[test]
fn a_remote_launch_is_held_beside_the_rest_saved_or_not() {
    let mut machines = Machines::new(reach("by-name"), &saved(), Some("me@build"), true);
    machines.cockpit().unwrap();
    assert!(machines.unsaved().is_none());
    assert_eq!(holding(&machines), [true, true, false]);

    let mut machines = Machines::new(reach("unsaved"), &saved(), Some("me@new"), true);
    let mut cockpit = machines.cockpit().unwrap();
    assert_eq!(machines.unsaved().unwrap().target, "me@new");
    assert_eq!(holding(&machines), [true, true, false, true]);

    let mut kept = saved();
    kept.push(Machine::new("me@new"));
    let held = machines.held(3);
    machines.settings_changed(&kept, &mut cockpit);
    assert!(machines.unsaved().is_none(), "saved now");
    assert_eq!(machines.held(3), held, "still the machine it was");

    assert!(!machines.forget_unsaved("me@new", &mut cockpit), "saved");
}

#[test]
fn forgetting_the_unsaved_machine_lets_it_go() {
    let mut machines = Machines::new(reach("forget"), &[], Some("me@new"), true);
    let mut cockpit = machines.cockpit().unwrap();
    let held = machines.held(1).unwrap();
    assert!(machines.forget_unsaved("me@new", &mut cockpit));
    assert!(!machines.listed());
    assert!(!cockpit.machines().any(|machine| machine == held));
}

#[test]
fn turning_a_machine_on_and_off_holds_it_and_lets_it_go() {
    let mut machines = Machines::new(reach("turned"), &saved(), None, true);
    let mut cockpit = machines.cockpit().unwrap();
    let build = machines.held(1).unwrap();

    let mut changed = saved();
    changed[0].enabled = false;
    changed[1].enabled = true;
    machines.settings_changed(&changed, &mut cockpit);
    assert_eq!(holding(&machines), [true, false, true]);
    assert!(!cockpit.machines().any(|machine| machine == build));
    assert_eq!(cockpit.machines().count(), 2);

    machines.settings_changed(&[], &mut cockpit);
    assert_eq!(holding(&machines), [true]);
    assert_eq!(cockpit.machines().count(), 1, "the removed machine is let go");
}

#[test]
fn a_machine_whose_target_is_an_option_is_never_listed() {
    let machines = Machines::new(reach("option"), &[Machine::new("-oProxyCommand=x")], None, true);
    assert!(!machines.listed());
}

#[test]
fn this_computer_turned_off_is_never_opened() {
    let reach = reach("local-off");
    let store = reach.store.clone();
    let mut machines = Machines::new(reach, &saved(), None, false);
    let mut cockpit = machines.cockpit().unwrap();
    assert_eq!(holding(&machines), [false, true, false]);
    assert_eq!(cockpit.this_computer(), None);
    assert_eq!(
        machines
            .rows(&cockpit)
            .iter()
            .map(|row| row.index)
            .collect::<Vec<_>>(),
        [1, 2],
        "this computer is not listed"
    );
    cockpit.pump();
    assert!(!store.exists(), "a disabled computer never opens its store");

    machines.settings_changed(&[], &mut cockpit);
    assert!(!cockpit.environment_available(), "nothing held");
    machines.set_local_enabled(true, &mut cockpit);
    assert_eq!(cockpit.this_computer(), machines.held(0));
    assert!(cockpit.environment_available());
}
