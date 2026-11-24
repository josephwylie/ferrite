use std::fs;
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

use super::*;

fn file(sha256: &str) -> Entry {
    Entry::File {
        size: sha256.len() as u64,
        sha256: sha256.into(),
        mode: Some(0o644),
        modified: None,
    }
}

fn manifest(entries: &[(&str, Entry)]) -> Manifest {
    Manifest {
        entries: entries
            .iter()
            .map(|(path, entry)| (path.to_string(), entry.clone()))
            .collect(),
        ..Manifest::default()
    }
}

fn write(path: &str, entry: &Entry) -> Step {
    let Entry::File { size, sha256, mode, .. } = entry else {
        unreachable!()
    };
    Step::Write {
        path: path.into(),
        size: *size,
        sha256: sha256.clone(),
        mode: *mode,
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-tree-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn into_nothing_every_directory_comes_before_what_it_holds() {
    let source = manifest(&[
        ("a", Entry::Dir),
        ("a/b", Entry::Dir),
        ("a/b/c.txt", file("c")),
        ("a-z.txt", file("az")),
        ("empty", Entry::Dir),
        ("link", Entry::Symlink { target: "a/b/c.txt".into() }),
    ]);
    assert_eq!(
        plan(&source, &Manifest::default(), &[]),
        [
            Step::MakeDir("a".into()),
            write("a-z.txt", &file("az")),
            Step::MakeDir("a/b".into()),
            write("a/b/c.txt", &file("c")),
            Step::MakeDir("empty".into()),
            Step::Link {
                path: "link".into(),
                target: "a/b/c.txt".into(),
            },
        ]
    );
}

#[test]
fn what_is_already_the_same_is_left() {
    let source = manifest(&[
        ("a", Entry::Dir),
        ("a/c.txt", file("c")),
        ("link", Entry::Symlink { target: "a".into() }),
    ]);
    assert!(plan(&source, &source, &[]).is_empty());
}

#[test]
fn what_only_the_far_side_holds_goes_outermost_first() {
    let source = manifest(&[("kept.txt", file("k"))]);
    let dest = manifest(&[
        ("kept.txt", file("k")),
        ("old", Entry::Dir),
        ("old/a.txt", file("a")),
        ("old/deep", Entry::Dir),
        ("old/deep/b.txt", file("b")),
        ("stale.txt", file("s")),
    ]);
    assert_eq!(
        plan(&source, &dest, &[]),
        [Step::Remove("old".into()), Step::Remove("stale.txt".into())]
    );
}

#[test]
fn a_changed_file_is_rewritten_in_place_and_a_changed_kind_replaced() {
    let source = manifest(&[
        ("changed.txt", file("new")),
        ("was-file", Entry::Dir),
        ("was-file/inner.txt", file("i")),
        ("was-dir", file("d")),
        ("moved", Entry::Symlink { target: "there".into() }),
    ]);
    let dest = manifest(&[
        ("changed.txt", file("old")),
        ("was-file", file("f")),
        ("was-dir", Entry::Dir),
        ("was-dir/inner.txt", file("i")),
        ("moved", Entry::Symlink { target: "here".into() }),
    ]);
    assert_eq!(
        plan(&source, &dest, &[]),
        [
            Step::Remove("moved".into()),
            Step::Remove("was-dir".into()),
            Step::Remove("was-file".into()),
            write("changed.txt", &file("new")),
            Step::Link {
                path: "moved".into(),
                target: "there".into(),
            },
            write("was-dir", &file("d")),
            Step::MakeDir("was-file".into()),
            write("was-file/inner.txt", &file("i")),
        ]
    );
}

#[test]
fn an_executable_bit_differs_only_between_two_sides_that_keep_one() {
    let with = |mode| Entry::File {
        size: 1,
        sha256: "r".into(),
        mode,
        modified: None,
    };
    let source = manifest(&[("run", with(Some(0o755)))]);
    assert_eq!(
        plan(&source, &manifest(&[("run", with(Some(0o644)))]), &[]),
        [write("run", &with(Some(0o755)))]
    );
    assert!(
        plan(&source, &manifest(&[("run", with(Some(0o711)))]), &[]).is_empty(),
        "the rest of the bits are the far side's own"
    );
    assert!(plan(&source, &manifest(&[("run", with(None))]), &[]).is_empty());
    assert!(plan(&manifest(&[("run", with(None))]), &source, &[]).is_empty());
}

#[test]
fn what_is_skipped_or_unlisted_is_left_with_each_directory_that_holds_it() {
    let mut source = manifest(&[("a.txt", file("a")), ("dir", file("now a file"))]);
    source.problems.insert("private".into(), "Permission denied".into());
    let mut dest = manifest(&[
        ("keep", Entry::Dir),
        ("keep/deep", Entry::Dir),
        ("keep/other.txt", file("o")),
        ("dir", Entry::Dir),
        ("private", Entry::Dir),
        ("private/x.txt", file("x")),
    ]);
    dest.problems.insert("dir/unreadable".into(), "Permission denied".into());
    assert_eq!(
        plan(&source, &dest, &["keep/deep/inner.txt".into()]),
        [Step::Remove("keep/other.txt".into()), write("a.txt", &file("a"))],
        "keep, keep/deep and dir hold a held path, and private is held"
    );
}

#[test]
fn a_path_outside_the_tree_is_refused() {
    for path in ["", "/etc/passwd", "../x", "a/../../x", "a//b", "./a", "a/", "a\0b"] {
        assert!(check_path(path).is_err(), "{path:?}");
    }
    #[cfg(windows)]
    for path in ["C:x", "a\\..\\..\\x", "C:\\x"] {
        assert!(check_path(path).is_err(), "{path:?}");
    }
    for path in ["a", "a/b.txt", ".hidden/x", "..x", "x.."] {
        check_path(path).unwrap();
    }
}

#[test]
fn a_scan_lists_without_following_and_leaves_out_what_is_skipped() {
    let root = scratch("scan");
    fs::create_dir_all(root.join("src/empty")).unwrap();
    fs::create_dir_all(root.join("target/debug")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(root.join("target/debug/big"), "built").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join("src/main.rs"), fs::Permissions::from_mode(0o640)).unwrap();
        std::os::unix::fs::symlink("src", root.join("code")).unwrap();
    }

    let scanned = scan(&root, &["target".into()]).unwrap();
    let link = cfg!(unix).then(|| ("code", Entry::Symlink { target: "src".into() }));
    let expected: Vec<_> = [
        ("src", Entry::Dir),
        ("src/empty", Entry::Dir),
        (
            "src/main.rs",
            Entry::File {
                size: 12,
                sha256: chunked::hex(&Sha256::digest(b"fn main() {}")),
                mode: cfg!(unix).then_some(0o640),
                modified: None,
            },
        ),
    ]
    .into_iter()
    .chain(link)
    .collect();
    assert_eq!(scanned, manifest(&expected));
    assert!(scan(&root, &["../x".into()]).is_err());
}

#[test]
fn a_root_that_is_not_there_is_named_in_its_error() {
    let missing = scratch("missing").join("nowhere");
    let refused = scan(&missing, &[]).unwrap_err();
    assert!(refused.to_string().contains(&*missing.to_string_lossy()), "{refused}");
}

/// A name this computer cannot spell as UTF-8: a lone surrogate on Windows,
/// a stray byte on Linux. macOS refuses to make one.
#[cfg(any(windows, target_os = "linux"))]
fn unspellable() -> std::ffi::OsString {
    #[cfg(windows)]
    return std::os::windows::ffi::OsStringExt::from_wide(&[0x61, 0xD800]);
    #[cfg(target_os = "linux")]
    return std::os::unix::ffi::OsStringExt::from_vec(vec![b'a', 0xFF]);
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn a_name_that_is_not_utf8_is_a_problem_of_its_own_and_the_rest_is_listed() {
    let root = scratch("unspellable");
    fs::create_dir_all(root.join("dir")).unwrap();
    fs::write(root.join("dir").join(unspellable()), "odd").unwrap();
    fs::write(root.join("kept.txt"), "kept").unwrap();
    let scanned = scan(&root, &[]).unwrap();
    assert!(scanned.entries.contains_key("kept.txt"));
    assert!(scanned.entries.contains_key("dir"));
    assert_eq!(
        scanned.problems.keys().collect::<Vec<_>>(),
        ["dir/a\u{FFFD}"],
        "named as near as it can be"
    );
}

/// Run as root, every file reads.
#[cfg(unix)]
#[test]
fn what_cannot_be_read_is_a_problem_by_its_path_and_the_rest_is_listed() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let root = scratch("unreadable");
    fs::write(root.join("kept.txt"), "kept").unwrap();
    fs::write(root.join("secret.txt"), "secret").unwrap();
    fs::create_dir_all(root.join("locked/deep")).unwrap();
    let set = |path: &str, mode| {
        fs::set_permissions(root.join(path), fs::Permissions::from_mode(mode)).unwrap()
    };
    set("secret.txt", 0o000);
    set("locked", 0o000);
    let scanned = scan(&root, &[]);
    set("locked", 0o755);
    let scanned = scanned.unwrap();
    assert_eq!(scanned.entries.keys().collect::<Vec<_>>(), ["kept.txt"]);
    assert_eq!(scanned.problems.keys().collect::<Vec<_>>(), ["locked", "secret.txt"]);
}

/// Files made and removed while the tree is scanned are each listed or
/// not, never an error.
#[test]
fn a_file_that_goes_while_it_is_scanned_is_left_out() {
    let root = scratch("churn");
    fs::write(root.join("kept.txt"), "kept").unwrap();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let churn = {
        let (root, stop) = (root.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                for at in 0..64 {
                    let _ = fs::write(root.join(format!("{at}.tmp")), "x".repeat(at));
                }
                for at in 0..64 {
                    let _ = fs::remove_file(root.join(format!("{at}.tmp")));
                }
            }
        })
    };
    let scans: Vec<_> = (0..200).map(|_| scan(&root, &[])).collect();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    churn.join().unwrap();
    for scanned in scans {
        assert!(scanned.unwrap().entries.contains_key("kept.txt"));
    }
}

fn set_modified(path: &Path, at: SystemTime) {
    fs::File::options().write(true).open(path).unwrap().set_modified(at).unwrap();
}

/// cp -p, tar and npm write files whose size and modification time say
/// nothing moved: on Unix the change time says it did. Windows keeps none
/// (ADR 0048).
#[test]
fn a_file_is_read_again_only_when_what_its_metadata_says_moved() {
    let root = scratch("cache");
    let path = root.join("kept.txt");
    let settled = SystemTime::now() - Duration::from_secs(60);
    fs::write(&path, "one").unwrap();
    set_modified(&path, settled);
    if cfg!(unix) {
        // Setting the time moved the change time, which must settle too.
        std::thread::sleep(scan::SETTLED + Duration::from_millis(100));
    }
    let first = scan(&root, &[]).unwrap();
    let read = || scan::READ.with(std::cell::Cell::get);
    let before = read();
    assert_eq!(scan(&root, &[]).unwrap(), first);
    assert_eq!(read(), before, "nothing moved: nothing read");

    fs::write(&path, "two").unwrap();
    set_modified(&path, settled);
    if cfg!(unix) {
        assert_ne!(scan(&root, &[]).unwrap(), first, "rewritten under the same size and time");
    }
    set_modified(&path, settled + Duration::from_secs(1));
    assert_ne!(scan(&root, &[]).unwrap(), first, "a new time: read again");
}

#[test]
fn a_file_just_written_is_read_again_however_its_time_reads() {
    let root = scratch("racy");
    let path = root.join("fresh.txt");
    fs::write(&path, "one").unwrap();
    let first = scan(&root, &[]).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, "two").unwrap();
    set_modified(&path, modified);
    assert_ne!(scan(&root, &[]).unwrap(), first);
}

#[test]
fn a_scan_of_many_entries_goes_in_bounded_pages_that_add_up_to_it() {
    use crate::protocol::{encode, ServerMessage};
    let mut whole = Manifest::default();
    for at in 0..250_000 {
        let path = format!("node_modules/@scope/package-{at:06}/lib/index.js");
        whole.entries.insert(path, file(&format!("{at:064}")));
    }
    whole.problems.insert("node_modules/@scope/package-000100/lib/private".into(), "Permission denied".into());
    let line = |manifest: Manifest, more| {
        encode(&ServerMessage::Reply {
            id: 1,
            ok: Some(Done::Tree {
                reply: Reply::Scanned { manifest, more },
            }),
            error: None,
        })
        .len()
    };
    assert!(line(whole.clone(), false) > link::LAGGED_BYTES, "whole, it is past what a connection carries");

    let (mut together, mut after, mut pages) = (Manifest::default(), None, 0);
    loop {
        let (page, more) = whole.page(after.as_deref(), serve::PAGE_BYTES);
        assert!(line(page.clone(), more) <= serve::PAGE_BYTES + 100);
        after = page.entries.keys().chain(page.problems.keys()).max().cloned();
        together.entries.extend(page.entries);
        together.problems.extend(page.problems);
        pages += 1;
        if !more {
            break;
        }
    }
    assert!(pages > 1);
    assert_eq!(together, whole);
}

/// A far side whose scan comes in two pages, and that answers nothing else.
struct Paged {
    pages: Vec<(Option<String>, Manifest, bool)>,
    replies: VecDeque<Reply>,
}

impl Exchange for Paged {
    fn ask(&mut self, request: Request) -> io::Result<Asked> {
        let Request::Scan { after, .. } = request else {
            panic!("asked {request:?}");
        };
        let at = self.pages.iter().position(|(from, ..)| *from == after).expect("a page after that");
        let (_, manifest, more) = self.pages.remove(at);
        self.replies.push_back(Reply::Scanned { manifest, more });
        Ok(Asked {
            id: 0,
            patience: PATIENCE,
        })
    }

    fn answer(&mut self, _: Asked) -> io::Result<Reply> {
        Ok(self.replies.pop_front().unwrap())
    }
}

#[test]
fn a_mirror_reads_every_page_of_the_far_side_before_it_plans() {
    let source = scratch("paged");
    fs::write(source.join("a.txt"), "first").unwrap();
    fs::write(source.join("b.txt"), "second").unwrap();
    let here = scan(&source, &[]).unwrap();
    let page = |path: &str| manifest(&[(path, here.entries[path].clone())]);
    let mut far = Paged {
        pages: vec![(None, page("a.txt"), true), (Some("a.txt".into()), page("b.txt"), false)],
        replies: VecDeque::new(),
    };
    let sent = mirror(
        &source,
        &Root::Path { path: "/far".into() },
        &[],
        &mut far,
        &mut |_| {},
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(sent, Sent::default());
    assert!(far.pages.is_empty());
}

/// The Server runs under umask 077; a directory a tree op makes, the root
/// and those on the way to it too, is still readable by all.
#[cfg(unix)]
#[test]
fn a_directory_a_tree_op_makes_is_readable_by_all_whatever_the_umask() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("dir-modes");
    let root = Root::Path {
        path: dir.join("new/tree"),
    };
    let mut session = Session::default();
    // SAFETY: umask only swaps the process's mask, put back at once.
    let before = unsafe { libc::umask(0o077) };
    let made = session.execute(Request::MakeDir {
        root,
        path: "src".into(),
    });
    unsafe { libc::umask(before) };
    assert_eq!(made.unwrap(), Reply::Done);
    for made in ["new", "new/tree", "new/tree/src"] {
        let mode = fs::metadata(dir.join(made)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "{made}");
    }
}

#[cfg(unix)]
#[test]
fn a_root_holding_the_link_that_is_the_home_directory_holds_it() {
    let dir = std::env::temp_dir().join(format!("ferrite-tree-home-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let real = dir.join("elsewhere/home");
    let beside = dir.join("users");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::create_dir_all(beside.join("other")).unwrap();
    let home = beside.join("me");
    std::os::unix::fs::symlink(&real, &home).unwrap();
    let resolved = |path: &std::path::Path| std::fs::canonicalize(path).unwrap();

    assert!(serve::holds_dir(&resolved(&beside), &home));
    assert!(serve::holds_dir(&resolved(&real), &home));
    assert!(serve::holds_dir(&resolved(&dir.join("elsewhere")), &home));
    assert!(!serve::holds_dir(&resolved(&beside.join("other")), &home));
    assert!(!serve::holds_dir(&resolved(&real).join("project"), &home));
    let _ = std::fs::remove_dir_all(&dir);
}
