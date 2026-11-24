//! A tree sent to a machine's Server through its bridge, on the connection
//! of its own a window's `Cockpit::tree` opens (ADR 0048).

use base64::Engine as _;
use ferrite_core::chunked::{Cancellation, CHUNK_SIZE};
use ferrite_core::tree::{self, Asked, Exchange, Reply, Request, Root, Sent, Step};

use super::remote_operations::machine;
use super::*;

fn connect(cockpit: &mut ferrite_core::cockpit::Cockpit) -> Box<dyn Exchange> {
    cockpit.tree(cockpit.first_machine()).unwrap()().unwrap()
}

fn mirror(source: &Path, dest: &Path, skip: &[String], exchange: &mut dyn Exchange) -> io::Result<Sent> {
    tree::mirror(
        source,
        &Root::Path { path: dest.to_path_buf() },
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
    let first = mirror(&source, &dest, &[], &mut *exchange).unwrap();
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

    let Reply::Bytes { data, size } = exchange.call(Request::Read {
        root: Root::Path { path: dest.clone() },
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
    assert_eq!(mirror(&source, &dest, &[], &mut *exchange).unwrap(), Sent::default());
    assert_eq!(actions(&sent, mark), ["scan"]);
}

/// An exchange whose line drops at the ask after its first `left`, once
/// all asked before it has run on the Server.
struct Cut {
    exchange: Option<Box<dyn Exchange>>,
    left: usize,
    last: Option<Asked>,
    asked: usize,
    answered: usize,
    /// How many asks the sender had not heard the reply to as it dropped.
    unheard: usize,
}

impl Exchange for Cut {
    fn ask(&mut self, request: Request) -> io::Result<Asked> {
        let exchange = self.exchange.as_mut().ok_or_else(dropped)?;
        if self.left == 0 {
            exchange.answer(self.last.unwrap()).unwrap();
            self.unheard = self.asked - self.answered;
            self.exchange = None;
            return Err(dropped());
        }
        self.left -= 1;
        self.asked += 1;
        let asked = exchange.ask(request)?;
        self.last = Some(asked);
        Ok(asked)
    }

    fn answer(&mut self, asked: Asked) -> io::Result<Reply> {
        self.answered += 1;
        self.exchange.as_mut().ok_or_else(dropped)?.answer(asked)
    }
}

fn dropped() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "the line dropped")
}

fn parts(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .filter(|entry| entry.as_ref().unwrap().file_name().to_string_lossy().ends_with(".part"))
        .count()
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
    let mut cut = Cut {
        exchange: Some(connect(&mut cockpit)),
        left: scan + 2 * whole_file + begin_and_first_chunk,
        last: None,
        asked: 0,
        answered: 0,
        unheard: 0,
    };
    let refused = mirror(&source, &dest, &[], &mut cut).unwrap_err();
    assert!(refused.to_string().starts_with("c.bin: "), "{refused}");
    assert!(cut.unheard > 1, "the send asked ahead of the replies: {}", cut.unheard);
    assert_eq!(std::fs::read(dest.join("b.txt")).unwrap(), b"second");
    assert!(!dest.join("c.bin").exists());

    let deadline = Instant::now() + PATIENCE;
    while parts(&dest) > 0 {
        assert!(Instant::now() < deadline, "the half-sent file stayed");
        std::thread::sleep(Duration::from_millis(5));
    }

    let mut exchange = connect(&mut cockpit);
    assert_eq!(
        mirror(&source, &dest, &[], &mut *exchange).unwrap(),
        Sent {
            written: 2,
            bytes: big().len() as u64 + 6,
            ..Sent::default()
        }
    );
    same(&source, &dest);
}

#[test]
fn a_refused_entry_fails_the_send_by_its_name_and_its_later_requests_do_no_harm() {
    let harness = start("tree-refused-entry");
    let sent = Arc::default();
    let mut cockpit = machine(&harness, &sent);
    let source = harness.dir.join("desktop-tree");
    let dest = harness.dir.join("box-tree");
    write(&source.join("a.txt"), "first");
    write(&source.join("never-made/b.bin"), big());
    write(&source.join("c.txt"), "third");
    let file = |path: &str, size: usize| Step::Write {
        path: path.into(),
        size: size as u64,
        executable: false,
    };
    // `never-made` is not made there first, so the Server refuses b.bin's
    // begin while its chunks and finish are already on their way.
    let steps = [file("a.txt", 5), file("never-made/b.bin", big().len()), file("c.txt", 5)];

    let mut exchange = connect(&mut cockpit);
    let refused = tree::send(
        &source,
        &steps,
        &Root::Path { path: dest.clone() },
        &mut *exchange,
        &mut |_| {},
        &Cancellation::default(),
    )
    .unwrap_err();
    assert!(refused.to_string().starts_with("never-made/b.bin: "), "{refused}");

    let Reply::Scanned { manifest } = exchange
        .call(Request::Scan {
            root: Root::Path { path: dest.clone() },
            skip: Vec::new(),
        })
        .unwrap()
    else {
        panic!("not scanned");
    };
    let here = tree::scan(&source, &[]).unwrap();
    assert!(manifest.entries.contains_key("a.txt"));
    for (path, there) in &manifest.entries {
        assert_eq!(here.entries.get(path), Some(there), "{path} is whole or not there");
    }
    assert!(!dest.join("never-made").exists());
    assert_eq!(parts(&dest), 0);
}

#[test]
fn a_file_that_shrinks_as_it_is_sent_fails_by_its_name_and_the_connection_sends_on() {
    let harness = start("tree-shrunk");
    let sent = Arc::default();
    let mut cockpit = machine(&harness, &sent);
    let source = harness.dir.join("desktop-tree");
    let dest = harness.dir.join("box-tree");
    write(&source.join("a.bin"), big());
    write(&source.join("b.txt"), "second");
    // Listed a byte longer than it reads, so its write is left under way
    // there, never finished.
    let shrunk = Step::Write {
        path: "a.bin".into(),
        size: big().len() as u64 + 1,
        executable: false,
    };

    let mut exchange = connect(&mut cockpit);
    let changed = tree::send(
        &source,
        &[shrunk],
        &Root::Path { path: dest.clone() },
        &mut *exchange,
        &mut |_| {},
        &Cancellation::default(),
    )
    .unwrap_err();
    assert!(changed.to_string().starts_with("a.bin: "), "{changed}");

    assert_eq!(
        mirror(&source, &dest, &[], &mut *exchange).unwrap(),
        Sent {
            written: 2,
            bytes: big().len() as u64 + 6,
            ..Sent::default()
        },
        "the write left under way is neither in the way nor taken for a file there"
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
    let root = Root::Path { path: dest.clone() };
    let mut exchange = connect(&mut cockpit);

    for path in ["../victim.txt", "a/../../victim.txt", "", "./x"] {
        let removed = exchange.call(Request::Remove {
            root: root.clone(),
            path: path.into(),
        });
        assert!(removed.is_err(), "{path:?}");
        let begun = exchange.call(Request::Begin {
            write: 1,
            root: root.clone(),
            path: path.into(),
            size: 1,
            executable: false,
        });
        assert!(begun.is_err(), "{path:?}");
    }
    let absolute = victim.to_string_lossy().replace('\\', "/");
    assert!(exchange.call(Request::Remove {
        root: root.clone(),
        path: absolute.clone()
    })
    .is_err());
    assert!(exchange.call(Request::MakeDir {
        root: root.clone(),
        path: absolute
    })
    .is_err());
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep me");

    for named in ["relative/dir", "~", "/"] {
        let refused = exchange.call(Request::Read {
            root: Root::Path { path: named.into() },
            path: "nothing-here".into(),
            offset: 0,
        })
        .unwrap_err();
        assert!(
            refused.to_string().contains("absolute") || refused.to_string().contains("home directory"),
            "{named}: {refused}"
        );
    }
    let unknown = exchange.call(Request::Scan {
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
        assert!(exchange.call(Request::Begin {
            write: 1,
            root: root.clone(),
            path: "through/x.txt".into(),
            size: 1,
            executable: false,
        })
        .is_err());
        assert!(exchange.call(Request::Remove {
            root,
            path: "through/x.txt".into()
        })
        .is_err());
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    }
}

/// `link` made a directory link to `target`: a symlink on Unix, a junction
/// on Windows, which needs no privilege.
fn dir_link(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    {
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    }
}

/// Every spelling of the home directory is refused as a root. Only a read
/// of a file that is not there is asked, so the home is never touched.
#[test]
fn a_root_that_is_the_home_directory_however_it_is_spelled_is_refused() {
    let harness = start("tree-home-spelled");
    let sent = Arc::default();
    let mut cockpit = machine(&harness, &sent);
    let mut exchange = connect(&mut cockpit);
    let home = ferrite_core::workspace::expand_home(Path::new("~"));
    let link = harness.dir.join("homelink");
    dir_link(&home, &link);
    let parent = harness.dir.join("parentlink");
    dir_link(home.parent().unwrap(), &parent);
    let mut spellings = vec![link, parent];
    if cfg!(any(windows, target_os = "macos")) {
        spellings.push(home.to_string_lossy().to_uppercase().into());
    }
    if cfg!(windows) {
        spellings.push(format!(r"\\?\{}", home.display()).into());
    }
    for root in spellings {
        let refused = exchange
            .call(Request::Read {
                root: Root::Path { path: root.clone() },
                path: format!("ferrite-not-here-{}", std::process::id()),
                offset: 0,
            })
            .unwrap_err();
        assert!(refused.to_string().contains("home directory"), "{}: {refused}", root.display());
    }
}

#[test]
fn a_send_into_or_around_its_own_source_on_this_computer_is_refused() {
    let harness = start("tree-overlap");
    let source = harness.dir.join("desktop-tree");
    write(&source.join("inner/a.txt"), "first");
    write(&source.join("b.txt"), "second");
    let listed = || tree::scan(&source, &[]).unwrap();
    let before = listed();
    for (from, to) in [
        (source.clone(), source.join("copy")),
        (source.join("inner"), source.clone()),
        (source.clone(), source.clone()),
    ] {
        let args: Vec<String> = ["ferrite", "tree", "send"]
            .into_iter()
            .map(String::from)
            .chain([from.to_string_lossy().into(), to.to_string_lossy().into()])
            .collect();
        assert_eq!(
            ferrite_core::api::cli::run(&args, &harness.dir.join("threads"), &harness.dir),
            Some(1),
            "{} into {}",
            from.display(),
            to.display()
        );
        assert_eq!(listed(), before);
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
