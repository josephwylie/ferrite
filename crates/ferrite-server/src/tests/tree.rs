//! A tree sent to a machine's Server through its bridge, on the connection
//! of its own a window's `Cockpit::tree` opens (ADR 0048).

use base64::Engine as _;
use ferrite_core::chunked::{Cancellation, CHUNK_SIZE};
use ferrite_core::tree::{self, Reply, Request, Root, Sent};

use super::remote_operations::machine;
use super::*;

fn connect(cockpit: &mut ferrite_core::cockpit::Cockpit) -> tree::Exchange {
    cockpit.tree(cockpit.first_machine()).unwrap()().unwrap()
}

fn mirror(source: &Path, dest: &Path, skip: &[String], exchange: &mut tree::Exchange) -> io::Result<Sent> {
    tree::mirror(
        source,
        &Root::Path(dest.to_path_buf()),
        skip,
        exchange,
        &mut |_| {},
        &Cancellation::default(),
    )
}

fn same(source: &Path, dest: &Path) {
    assert_eq!(
        tree::scan(source, &[]).unwrap().entries,
        tree::scan(dest, &[]).unwrap().entries
    );
}

/// The tree ops on the wire since `from`, by action.
fn actions(sent: &Arc<Mutex<Vec<u8>>>, from: usize) -> Vec<String> {
    sent.lock().unwrap()[from..]
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
        .filter(|message| message["op"] == "tree")
        .map(|message| message["request"]["action"].as_str().unwrap().to_string())
        .collect()
}

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn big() -> Vec<u8> {
    (0..CHUNK_SIZE * 2 + 3).map(|at| (at % 251) as u8).collect()
}

#[test]
fn a_tree_reaches_a_machine_whole_and_sending_it_again_moves_nothing() {
    let harness = start("tree-whole");
    let sent = Arc::default();
    let mut cockpit = machine(&harness, &sent);
    let source = harness.dir.join("desktop-tree");
    let dest = harness.dir.join("box-tree");
    write(&source.join("src/lib.rs"), "pub fn it() {}\n");
    write(&source.join("src/deep/er/mod.rs"), "mod er;\n");
    write(&source.join("big.bin"), big());
    std::fs::create_dir_all(source.join("empty")).unwrap();
    write(&source.join("run.sh"), "#!/bin/sh\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(source.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink("src/lib.rs", source.join("lib")).unwrap();
    }
    write(&dest.join("stale.txt"), "gone soon");
    write(&dest.join("old/inner.txt"), "gone soon");

    let mut exchange = connect(&mut cockpit);
    let first = mirror(&source, &dest, &[], &mut exchange).unwrap();
    same(&source, &dest);
    assert_eq!(
        first,
        Sent {
            removed: 2,
            made: 4,
            written: 4,
            linked: usize::from(cfg!(unix)),
            bytes: big().len() as u64 + 15 + 8 + 10,
        }
    );
    assert_eq!(
        actions(&sent, 0).iter().filter(|action| *action == "chunk").count(),
        3 + 3,
        "the big file in three chunks, each small one in one"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dest.join("run.sh")).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111);
        assert_eq!(std::fs::read_link(dest.join("lib")).unwrap(), Path::new("src/lib.rs"));
    }

    let Reply::Bytes { data, size } = exchange(Request::Read {
        root: Root::Path(dest.clone()),
        path: "big.bin".into(),
        offset: CHUNK_SIZE as u64,
    })
    .unwrap() else {
        panic!("not bytes");
    };
    assert_eq!(size, big().len() as u64);
    assert_eq!(
        base64::engine::general_purpose::STANDARD.decode(data).unwrap(),
        big()[CHUNK_SIZE..CHUNK_SIZE * 2]
    );

    let mark = sent.lock().unwrap().len();
    assert_eq!(mirror(&source, &dest, &[], &mut exchange).unwrap(), Sent::default());
    assert_eq!(actions(&sent, mark), ["scan"]);
}

#[test]
fn a_send_cut_off_part_way_is_finished_by_the_next_without_resending_what_arrived() {
    let harness = start("tree-cut");
    let sent = Arc::default();
    let mut cockpit = machine(&harness, &sent);
    let source = harness.dir.join("desktop-tree");
    let dest = harness.dir.join("box-tree");
    write(&source.join("a.txt"), "first");
    write(&source.join("b.txt"), "second");
    write(&source.join("c.bin"), big());
    write(&source.join("d.txt"), "fourth");

    let (scan, whole_file, begin_and_first_chunk) = (1, 3, 2);
    let answered = scan + 2 * whole_file + begin_and_first_chunk;
    let mut exchange = connect(&mut cockpit);
    let mut asked = 0;
    let mut cut = |request| {
        asked += 1;
        if asked > answered {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "the line dropped"));
        }
        exchange(request)
    };
    let refused = tree::mirror(
        &source,
        &Root::Path(dest.clone()),
        &[],
        &mut cut,
        &mut |_| {},
        &Cancellation::default(),
    )
    .unwrap_err();
    assert!(refused.to_string().starts_with("c.bin: "), "{refused}");
    drop(exchange);
    assert_eq!(std::fs::read(dest.join("b.txt")).unwrap(), b"second");
    assert!(!dest.join("c.bin").exists());

    let deadline = Instant::now() + PATIENCE;
    while std::fs::read_dir(&dest).unwrap().any(|entry| {
        entry.unwrap().file_name().to_string_lossy().ends_with(".part")
    }) {
        assert!(Instant::now() < deadline, "the half-sent file stayed");
        std::thread::sleep(Duration::from_millis(5));
    }

    let mut exchange = connect(&mut cockpit);
    assert_eq!(
        mirror(&source, &dest, &[], &mut exchange).unwrap(),
        Sent {
            written: 2,
            bytes: big().len() as u64 + 6,
            ..Sent::default()
        }
    );
    same(&source, &dest);
}

#[test]
fn a_tree_op_outside_its_root_is_refused() {
    let harness = start("tree-refused");
    let sent = Arc::default();
    let mut cockpit = machine(&harness, &sent);
    let dest = harness.dir.join("box-tree");
    let victim = harness.dir.join("victim.txt");
    write(&victim, "keep me");
    let root = Root::Path(dest.clone());
    let mut exchange = connect(&mut cockpit);

    for path in ["../victim.txt", "a/../../victim.txt", "", "./x"] {
        let removed = exchange(Request::Remove {
            root: root.clone(),
            path: path.into(),
        });
        assert!(removed.is_err(), "{path:?}");
        let begun = exchange(Request::Begin {
            root: root.clone(),
            path: path.into(),
            size: 1,
            executable: false,
        });
        assert!(begun.is_err(), "{path:?}");
    }
    let absolute = victim.to_string_lossy().replace('\\', "/");
    assert!(exchange(Request::Remove {
        root: root.clone(),
        path: absolute.clone()
    })
    .is_err());
    assert!(exchange(Request::MakeDir {
        root: root.clone(),
        path: absolute
    })
    .is_err());
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep me");

    for named in ["relative/dir", "~", "/"] {
        let refused = exchange(Request::Read {
            root: Root::Path(named.into()),
            path: "nothing-here".into(),
            offset: 0,
        })
        .unwrap_err();
        assert!(
            refused.to_string().contains("absolute") || refused.to_string().contains("home directory"),
            "{named}: {refused}"
        );
    }
    let unknown = exchange(Request::Scan {
        root: Root::Unknown,
        skip: Vec::new(),
    })
    .unwrap_err();
    assert_eq!(unknown.kind(), io::ErrorKind::Unsupported);

    #[cfg(unix)]
    {
        let outside = harness.dir.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        std::os::unix::fs::symlink(&outside, dest.join("through")).unwrap();
        assert!(exchange(Request::Begin {
            root: root.clone(),
            path: "through/x.txt".into(),
            size: 1,
            executable: false,
        })
        .is_err());
        assert!(exchange(Request::Remove {
            root,
            path: "through/x.txt".into()
        })
        .is_err());
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    }
}

#[test]
fn the_cli_sends_a_tree_to_its_server_leaving_what_it_skips() {
    let harness = start("tree-cli");
    let source = harness.dir.join("desktop-tree");
    let dest = harness.dir.join("box-tree");
    write(&source.join("src/main.rs"), "fn main() {}\n");
    write(&source.join("target/debug/app"), "built");
    let args: Vec<String> = ["ferrite", "tree", "send"]
        .into_iter()
        .map(String::from)
        .chain([source.to_string_lossy().into(), dest.to_string_lossy().into()])
        .chain(["--skip".into(), "target".into()])
        .collect();
    assert_eq!(
        ferrite_core::api::cli::run(&args, &harness.dir.join("threads"), &harness.dir),
        Some(0)
    );
    assert_eq!(std::fs::read(dest.join("src/main.rs")).unwrap(), b"fn main() {}\n");
    assert!(!dest.join("target").exists());
}
