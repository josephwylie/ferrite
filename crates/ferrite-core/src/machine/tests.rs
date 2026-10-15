//! Machines: what ssh is asked, how its endings read, and a bridge whose
//! tunnels are fakes.

use super::*;

#[test]
fn a_target_is_one_word_and_never_an_option() {
    assert!(check_target("box").is_ok());
    assert!(check_target("me@box.example.com").is_ok());
    assert!(check_target("").is_err());
    assert!(check_target("-oProxyCommand=touch /tmp/x").is_err());
    assert!(check_target("box rm").is_err());
    assert!(check_target("box\n").is_err());
}

#[test]
fn a_saved_machine_is_enabled_unless_it_says_not() {
    let machine: Machine = serde_json::from_str(r#"{"name":"box","target":"me@box"}"#).unwrap();
    assert!(machine.enabled);
    assert_eq!(Machine::new("me@box").name, "me@box");
}

#[test]
fn the_bridge_never_asks_and_keeps_its_connection_alive() {
    let args = bridge_args("me@box", Path::new("/tmp/m/c"), WINDOW);
    let has = |pair: &str| args.windows(2).any(|w| w[0] == "-o" && w[1] == pair);
    assert!(has("BatchMode=yes"));
    assert!(has("ServerAliveInterval=15"));
    assert!(args.contains(&"-C".to_string()), "compressed");
    assert!(args.contains(&"-T".to_string()), "no terminal");
    if cfg!(unix) {
        assert!(has("ControlMaster=auto"));
        assert!(has("ControlPath=/tmp/m/c"), "Ferrite's own master");
    }
    assert_eq!(
        args[args.len() - 4..],
        ["--", "me@box", "ferrited", "bridge"],
        "the target can never be read as an option"
    );
    let api = bridge_args("me@box", Path::new("/tmp/m/c"), API);
    assert_eq!(
        api[..args.len() - 2],
        args[..args.len() - 2],
        "the same ssh"
    );
    assert_eq!(api[api.len() - 3..], ["ferrited", "bridge", "--api"]);
}

#[test]
fn signing_in_asks_in_a_terminal_and_leaves_the_master() {
    let args = sign_in_args("me@box", Path::new("/tmp/m/c"));
    assert!(!args.iter().any(|arg| arg == "BatchMode=yes"));
    if cfg!(unix) {
        assert!(args.iter().any(|arg| arg == "ControlPersist=600"));
    }
    let script = sign_in_script("it's mine", "me@box", Path::new("/tmp/m/c"));
    assert!(script.starts_with("#!/bin/sh\n"));
    assert!(script.contains("'Signing in to it'\\''s mine for Ferrite…'"));
    assert!(script.contains("'--' 'me@box' 'true'"));
}

#[test]
fn an_ending_says_what_the_operator_must_do() {
    assert_eq!(Failure::of(Some(0), ""), None);
    assert_eq!(
        Failure::of(Some(127), "bash: ferrited: command not found\n"),
        Some(Failure::NotInstalled)
    );
    assert_eq!(
        Failure::of(
            Some(255),
            "me@box: Permission denied (publickey,keyboard-interactive).\n"
        ),
        Some(Failure::SignIn)
    );
    assert_eq!(
        Failure::of(Some(255), "Host key verification failed.\n"),
        Some(Failure::SignIn)
    );
    assert_eq!(
        Failure::of(
            Some(255),
            "ssh: connect to host box port 22: Connection refused\n\n"
        ),
        Some(Failure::Unreachable(
            "ssh: connect to host box port 22: Connection refused".into()
        ))
    );
    assert_eq!(
        Failure::of(Some(1), ""),
        Some(Failure::Unreachable("ssh exited with 1".into()))
    );
    assert!(Failure::NotInstalled.needs_operator());
    assert!(Failure::SignIn.needs_operator());
    assert!(!Failure::Unreachable(String::new()).needs_operator());
}

#[test]
fn each_machine_has_its_own_short_dir() {
    let home = Path::new("/Users/me/.ferrite");
    let one = dir(home, "me@box");
    assert_eq!(one, dir(home, "me@box"));
    assert_ne!(one, dir(home, "me@other"));
    assert_eq!(one.parent(), Some(home.join("machines").as_path()));
    assert_eq!(one.file_name().unwrap().len(), 12);
}

/// A tunnel that ends at once, the way ssh ends when the machine has no
/// `ferrited`: the window's attach fails, carrying why.
#[test]
fn a_failed_tunnel_tells_the_window_why() {
    let socket = std::env::temp_dir()
        .join(format!("fm-{}", std::process::id()))
        .join("bridge.sock");
    let bridge = Bridge::new(
        "box",
        socket,
        Box::new(|| {
            Ok(Tunnel {
                from: Box::new(io::empty()),
                to: Box::new(io::sink()),
                ended: Box::new(|| Some(Failure::NotInstalled)),
            })
        }),
    );
    let hello = ClientMessage::Hello {
        generation: crate::protocol::GENERATION,
        client: crate::protocol::Peer {
            name: "test".into(),
            version: "0".into(),
        },
        capabilities: Vec::new(),
    };
    let Err(error) = bridge.attach(hello) else {
        panic!("attached to nothing");
    };
    assert_eq!(failure(&error), Some(&Failure::NotInstalled));
    assert!(bridge.sign_in().is_err(), "a fake is not reached over ssh");
}

/// Each Ferrite serves a socket of its own; the ones left by Ferrites
/// that are gone are removed, and a live one's never is.
#[cfg(unix)]
#[test]
fn a_gone_ferrites_bridge_socket_is_swept() {
    let dir = std::env::temp_dir().join(format!("fm-sweep-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let live = dir.join(format!("bridge-{}.sock", std::process::id()));
    let gone = dir.join(format!("bridge-{}.sock", libc::pid_t::MAX));
    let other = dir.join("c");
    for path in [&live, &gone, &other] {
        std::fs::write(path, "").unwrap();
    }
    sweep(&dir);
    assert!(live.exists());
    assert!(!gone.exists());
    assert!(other.exists(), "only bridge sockets");
    let _ = std::fs::remove_dir_all(&dir);
}
