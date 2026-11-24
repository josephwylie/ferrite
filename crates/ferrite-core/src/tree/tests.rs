use std::fs;
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

use super::*;

fn file(sha256: &str) -> Entry {
    Entry::File {
        size: sha256.len() as u64,
        sha256: sha256.into(),
        executable: false,
    }
}

fn manifest(entries: &[(&str, Entry)]) -> Manifest {
    Manifest {
        entries: entries
            .iter()
            .map(|(path, entry)| (path.to_string(), entry.clone()))
            .collect(),
        modes: true,
    }
}

fn write(path: &str, entry: &Entry) -> Step {
    let Entry::File {
        size, executable, ..
    } = entry
    else {
        unreachable!()
    };
    Step::Write {
        path: path.into(),
        size: *size,
        executable: *executable,
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
        plan(&source, &Manifest::default()),
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
    assert!(plan(&source, &source).is_empty());
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
        plan(&source, &dest),
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
        plan(&source, &dest),
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
    let run = Entry::File {
        size: 1,
        sha256: "r".into(),
        executable: true,
    };
    let source = manifest(&[("run", run.clone())]);
    let mut dest = manifest(&[("run", file("r"))]);
    assert_eq!(plan(&source, &dest), [write("run", &run)]);
    dest.modes = false;
    assert!(plan(&source, &dest).is_empty());
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
    std::os::unix::fs::symlink("src", root.join("code")).unwrap();

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
                executable: false,
            },
        ),
    ]
    .into_iter()
    .chain(link)
    .collect();
    assert_eq!(scanned.entries, manifest(&expected).entries);
    assert_eq!(scanned.modes, cfg!(unix));
    assert!(scan(&root, &["../x".into()]).is_err());
}

#[test]
fn a_file_whose_size_and_time_held_is_not_read_again() {
    let root = scratch("cache");
    let path = root.join("kept.txt");
    let settled = SystemTime::now() - Duration::from_secs(60);
    fs::write(&path, "one").unwrap();
    fs::File::options().write(true).open(&path).unwrap().set_modified(settled).unwrap();
    let first = scan(&root, &[]).unwrap();

    fs::write(&path, "two").unwrap();
    fs::File::options().write(true).open(&path).unwrap().set_modified(settled).unwrap();
    assert_eq!(scan(&root, &[]).unwrap(), first, "the same size and time: the hash kept");

    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(settled + Duration::from_secs(1))
        .unwrap();
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
    fs::File::options().write(true).open(&path).unwrap().set_modified(modified).unwrap();
    assert_ne!(scan(&root, &[]).unwrap(), first);
}
