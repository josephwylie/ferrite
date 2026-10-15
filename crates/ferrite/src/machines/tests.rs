//! Which machine the window is on, and which stream only status. Nothing
//! here connects: a link attaches only when polled.

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

/// Off screen and enabled: a status link. On screen or disabled: none.
fn streaming(machines: &Machines) -> Vec<bool> {
    machines
        .entries
        .iter()
        .map(|entry| entry.status.is_some())
        .collect()
}

#[test]
fn this_computer_alone_lists_no_machines() {
    let machines = Machines::new(reach("alone"), &[], None);
    assert!(!machines.listed());
    assert!(machines.selected().is_none());
}

#[test]
fn only_the_machine_on_screen_streams_more_than_status() {
    let mut machines = Machines::new(reach("status"), &saved(), None);
    assert!(machines.listed());
    assert_eq!(streaming(&machines), [false, true, false]);

    assert!(machines.select(1));
    assert_eq!(machines.selected().unwrap().target, "me@build");
    assert_eq!(streaming(&machines), [true, false, false]);

    assert!(!machines.select(1), "already on it");
    assert!(
        !machines.select(2),
        "a disabled machine is never connected to"
    );
    let rows = machines.rows(None);
    assert_eq!(rows[2].connection, Connection::Disabled);
    assert!(rows[1].selected);
    assert_eq!(rows[0].name, LOCAL);
}

#[test]
fn a_remote_launch_starts_on_its_machine_saved_or_not() {
    let machines = Machines::new(reach("by-name"), &saved(), Some("me@build"));
    assert_eq!(machines.selected().unwrap().name, "me@build");
    assert!(machines.unsaved().is_none());

    let mut machines = Machines::new(reach("unsaved"), &saved(), Some("me@new"));
    assert_eq!(machines.selected().unwrap().target, "me@new");
    assert_eq!(machines.unsaved().unwrap().target, "me@new");

    let mut kept = saved();
    kept.push(Machine::new("me@new"));
    machines.settings_changed(&kept);
    assert!(machines.unsaved().is_none(), "saved now");
    assert_eq!(machines.selected().unwrap().target, "me@new");
}

#[test]
fn the_machine_on_screen_stays_when_settings_drop_it() {
    let mut machines = Machines::new(reach("dropped"), &saved(), Some("me@build"));
    machines.settings_changed(&[]);
    assert_eq!(machines.selected().unwrap().target, "me@build");
    assert_eq!(machines.unsaved().unwrap().target, "me@build");
    assert_eq!(streaming(&machines), [true, false]);
}

#[test]
fn a_machine_whose_target_is_an_option_is_never_listed() {
    let machines = Machines::new(reach("option"), &[Machine::new("-oProxyCommand=x")], None);
    assert!(!machines.listed());
}
