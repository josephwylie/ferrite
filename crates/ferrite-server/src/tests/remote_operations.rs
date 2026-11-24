use super::*;

pub(super) fn machine(harness: &Harness, sent: &Arc<Mutex<Vec<u8>>>) -> ferrite_client::cockpit::Cockpit {
    let mut cockpit = ferrite_client::cockpit::Cockpit::attach(ferrite_client::cockpit::Attach {
        store: harness.dir.join("desktop"),
        server: ferrite_client::cockpit::Endpoint::Machine(bridge(harness, sent)),
        saved_as: ferrite_core::MachineKey(Some("box".into())),
        client: peer(),
        renderer: None,
        browser: false,
        driven: false,
    });
    pump_until(&mut cockpit, |cockpit| {
        matches!(
            cockpit.server(cockpit.first_machine()),
            Some(ferrite_client::cockpit::ServerState::Attached { .. })
        )
    });
    cockpit
}

fn answer<T>(cockpit: &mut ferrite_client::cockpit::Cockpit, receiver: Receiver<io::Result<T>>) -> T {
    let deadline = Instant::now() + PATIENCE;
    loop {
        cockpit.pump();
        match receiver.try_recv() {
            Ok(result) => return result.unwrap(),
            Err(mpsc::TryRecvError::Empty) => {}
            Err(error) => panic!("{error}"),
        }
        assert!(Instant::now() < deadline, "operation timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn git(root: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

fn repo(root: &Path) {
    std::fs::create_dir_all(root).unwrap();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(root.join("same.txt"), "base\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "initial"]);
}

#[test]
fn remote_changes_reads_and_stages_only_the_servers_index() {
    let harness = start("remote-changes");
    let remote = harness.dir.join("remote-repo");
    let local = harness.dir.join("desktop-repo");
    repo(&remote);
    repo(&local);
    std::fs::write(remote.join("same.txt"), "server\n").unwrap();
    std::fs::write(local.join("same.txt"), "desktop\n").unwrap();
    let sent = Arc::default();
    let mut cockpit = machine(&harness, &sent);
    let read = cockpit
        .changes(cockpit.first_machine(), &remote, ferrite_core::changes::Scope::Uncommitted)
        .unwrap();
    let snapshot = answer(&mut cockpit, read);
    assert_eq!(snapshot.files[0].sides.as_ref().unwrap().1, "server\n");
    assert!(snapshot.branches.contains(&"main".into()));
    assert!(snapshot.staged.is_empty());
    let stage = cockpit
        .set_file_staged(cockpit.first_machine(), &remote, "same.txt".into(), true)
        .unwrap();
    answer(&mut cockpit, stage);
    assert_eq!(
        git(&remote, &["diff", "--cached", "--name-only"]).trim(),
        "same.txt"
    );
    assert!(git(&local, &["diff", "--cached", "--name-only"]).is_empty());
    let read = cockpit
        .changes(cockpit.first_machine(), &remote, ferrite_core::changes::Scope::Uncommitted)
        .unwrap();
    assert!(answer(&mut cockpit, read).staged.contains("same.txt"));
    let unstage = cockpit
        .set_file_staged(cockpit.first_machine(), &remote, "same.txt".into(), false)
        .unwrap();
    answer(&mut cockpit, unstage);
    assert!(git(&remote, &["diff", "--cached", "--name-only"]).is_empty());
    let wire = sent.lock().unwrap();
    assert!(String::from_utf8_lossy(&wire).contains("\"op\":\"changes\""));
}

#[test]
fn remote_attachment_bytes_and_previews_cross_the_bridge_without_polling_the_window() {
    let harness = start("remote-attachment");
    let sent = Arc::default();
    let mut cockpit = machine(&harness, &sent);
    let source = harness.dir.join("desktop-capture.png");
    let bytes = vec![97; ferrite_core::chunked::CHUNK_SIZE + 7];
    std::fs::write(&source, &bytes).unwrap();
    let upload = cockpit.upload_attachment(cockpit.first_machine(), source.clone()).unwrap();
    let retained = upload
        .result
        .recv_timeout(PATIENCE)
        .unwrap()
        .unwrap();
    assert_ne!(retained.path, source);
    assert!(retained
        .path
        .starts_with(harness.dir.join("threads/attachments")));
    std::fs::remove_file(source).unwrap();
    assert_eq!(std::fs::read(&retained.path).unwrap(), bytes);
    let preview = cockpit.read_attachment(cockpit.first_machine(), &retained.path).unwrap();
    assert_eq!(
        preview
            .recv_timeout(PATIENCE)
            .unwrap()
            .unwrap(),
        bytes
    );
    assert!(upload
        .progress
        .try_iter()
        .any(|progress| progress.sent == bytes.len() as u64));
    let wire = sent.lock().unwrap();
    let commands: Vec<serde_json::Value> = wire
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(
        commands
            .iter()
            .filter(|message| message["request"]["action"] == "chunk")
            .count(),
        2
    );
    assert!(commands
        .iter()
        .any(|message| message["request"]["action"] == "read"));
}
