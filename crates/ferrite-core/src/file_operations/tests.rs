use super::*;

fn workspace(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ferrite-file-operations-{}-{name}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn file_operations_read_save_conflict_reload_and_overwrite() {
    let root = workspace("save");
    let path = root.join("notes.md");
    std::fs::write(&path, "remote original").unwrap();
    let Reply::Text {
        text,
        fingerprint: original,
    } = execute(Request::Read {
        root: root.clone(),
        path: path.clone(),
    })
    .unwrap()
    else {
        panic!()
    };
    assert_eq!(text, "remote original");
    assert_eq!(original, fingerprint(b"remote original"));
    let save = |text: &str, expected| {
        execute(Request::Write {
            root: root.clone(),
            path: path.clone(),
            text: text.into(),
            expected,
        })
        .unwrap()
    };
    assert_eq!(
        save("edited", Some(original)),
        Reply::Written {
            fingerprint: fingerprint(b"edited")
        }
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited");
    assert_eq!(save("stale", Some(original)), Reply::Conflict);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited");
    assert_eq!(
        save("overwrite", None),
        Reply::Written {
            fingerprint: fingerprint(b"overwrite")
        }
    );
    assert_eq!(
        execute(Request::Read {
            root: root.clone(),
            path
        })
        .unwrap(),
        Reply::Text {
            text: "overwrite".into(),
            fingerprint: fingerprint(b"overwrite")
        }
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_operations_reject_escape_binary_and_oversized_text_without_writing() {
    let root = workspace("bounds");
    let outside = root.with_extension("outside");
    std::fs::write(&outside, "leave alone").unwrap();
    assert!(execute(Request::Write {
        root: root.clone(),
        path: outside.clone(),
        text: "bad".into(),
        expected: None
    })
    .is_err());
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "leave alone");
    let binary = root.join("image.bin");
    std::fs::write(&binary, [0xff, 0xfe]).unwrap();
    assert_eq!(
        execute(Request::Read {
            root: root.clone(),
            path: binary.clone()
        })
        .unwrap(),
        Reply::Binary
    );
    assert!(execute(Request::Write {
        root: root.clone(),
        path: binary.clone(),
        text: "x".repeat(MAX_TEXT_BYTES + 1),
        expected: None
    })
    .is_err());
    assert_eq!(std::fs::read(&binary).unwrap(), [0xff, 0xfe]);
    std::fs::write(&binary, vec![b'x'; MAX_TEXT_BYTES + 1]).unwrap();
    assert!(execute(Request::Read {
        root: root.clone(),
        path: binary
    })
    .is_err());
    std::fs::remove_file(outside).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn file_operations_do_not_follow_symlinks_outside_the_workspace() {
    let root = workspace("symlink");
    let outside = root.with_extension("outside");
    std::fs::write(&outside, "leave alone").unwrap();
    let link = root.join("link");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    assert!(execute(Request::Read {
        root: root.clone(),
        path: link.clone()
    })
    .is_err());
    assert!(execute(Request::Write {
        root: root.clone(),
        path: link,
        text: "bad".into(),
        expected: None
    })
    .is_err());
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "leave alone");
    std::fs::remove_file(outside).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_operations_snapshot_lists_expanded_folders_search_and_git_status() {
    let root = workspace("snapshot");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("target")).unwrap();
    std::fs::write(root.join("src/remote.rs"), "remote").unwrap();
    std::fs::write(root.join("target/build.rs"), "build").unwrap();
    assert!(std::process::Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(&root)
        .status()
        .unwrap()
        .success());
    let Reply::Snapshot(snapshot) = execute(Request::Snapshot {
        root: root.clone(),
        directories: vec![root.join("src")],
        search: true,
    })
    .unwrap() else {
        panic!()
    };
    assert_eq!(snapshot.directories.len(), 2);
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].path, root.join("src/remote.rs"));
    assert!(snapshot.directories[0]
        .entries
        .iter()
        .all(|entry| entry.name != ".git"));
    assert!(snapshot
        .status
        .iter()
        .any(|(path, status)| path == Path::new("src") && *status == Status::Added));
    assert!(!snapshot.truncated);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_operations_wire_accepts_future_fields_and_unknown_variants() {
    assert_eq!(
        serde_json::from_str::<Request>(r#"{"kind":"future","extra":true}"#).unwrap(),
        Request::Unknown
    );
    assert_eq!(
        serde_json::from_str::<Reply>(r#"{"kind":"future","extra":true}"#).unwrap(),
        Reply::Unknown
    );
    let request: Request =
        serde_json::from_str(r#"{"kind":"snapshot","root":"/workspace","future":true}"#).unwrap();
    assert_eq!(
        request,
        Request::Snapshot {
            root: "/workspace".into(),
            directories: Vec::new(),
            search: false
        }
    );
    assert_eq!(
        fingerprint(b"abc"),
        [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad
        ]
    );
}

#[test]
fn file_operations_keep_available_files_when_an_expanded_folder_disappears() {
    let root = workspace("missing-descendant");
    std::fs::write(root.join("available.rs"), "available").unwrap();
    let Reply::Snapshot(snapshot) = execute(Request::Snapshot {
        root: root.clone(),
        directories: vec![root.join("removed")],
        search: true,
    })
    .unwrap() else {
        panic!()
    };
    assert!(snapshot.truncated);
    assert_eq!(snapshot.files.len(), 1);
    assert_eq!(snapshot.files[0].name, "available.rs");
    assert!(execute(Request::Snapshot {
        root: root.join("missing-root"),
        directories: Vec::new(),
        search: true
    })
    .is_err());
    std::fs::remove_dir_all(root).unwrap();
}
