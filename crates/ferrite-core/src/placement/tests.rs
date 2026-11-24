use std::collections::{BTreeMap, VecDeque};

use super::*;
use crate::project::ProjectId;
use crate::protocol::Repositories;
use crate::tree::{Asked, Reply, Request, Session};
use crate::workspace::registry::Registry;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-placement-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dunce::canonicalize(&dir).unwrap()
}

/// The id and root of a Project at each of `roots`, registered on a
/// Machine whose registry is under `dir`.
fn registered(dir: &Path, roots: &[&str]) -> Vec<(ProjectId, PathBuf)> {
    let mut registry = Registry::open(&dir.join("registry")).unwrap();
    roots
        .iter()
        .map(|root| {
            let root = dir.join(root);
            fs::create_dir_all(&root).unwrap();
            let id = registry.register(&root).unwrap();
            (id, registry.project(id).unwrap().root.clone())
        })
        .collect()
}

#[test]
fn a_counterpart_is_the_first_project_there_of_the_same_repository() {
    let dir = scratch("by-repository");
    let projects = registered(&dir, &["other", "app", "app-again"]);
    let repositories: Repositories = BTreeMap::from([
        (projects[0].0, "github.com/acme/other".to_string()),
        (projects[1].0, "github.com/acme/app".to_string()),
        (projects[2].0, "github.com/acme/app".to_string()),
    ]);
    let found = counterpart(
        Path::new("C:/src/app"),
        Some("github.com/acme/app"),
        Across::ToMachine(&[]),
        &projects,
        &repositories,
    );
    assert_eq!(found, Some(projects[1].0));
}

#[test]
fn a_project_with_no_repository_is_found_by_its_placement_either_way() {
    let dir = scratch("by-placement");
    let projects = registered(&dir, &["notes", "elsewhere"]);
    let local = PathBuf::from("C:/Users/me/notes");
    let placements = [Placement {
        local_root: local.clone(),
        remote_root: projects[0].1.clone(),
    }];
    let none = Repositories::new();
    assert_eq!(
        counterpart(&local, None, Across::ToMachine(&placements), &projects, &none),
        Some(projects[0].0)
    );

    let local_projects = registered(&dir.join("here"), &["notes"]);
    let back = [Placement {
        local_root: local_projects[0].1.clone(),
        remote_root: PathBuf::from("/home/me/projects/notes"),
    }];
    assert_eq!(
        counterpart(
            Path::new("/home/me/projects/notes"),
            None,
            Across::FromMachine(&back),
            &local_projects,
            &none
        ),
        Some(local_projects[0].0)
    );
}

#[test]
fn a_project_with_neither_has_no_counterpart() {
    let dir = scratch("none");
    let projects = registered(&dir, &["app"]);
    let repositories: Repositories = BTreeMap::from([(projects[0].0, "github.com/acme/app".to_string())]);
    let root = Path::new("C:/src/tool");
    assert_eq!(
        counterpart(root, Some("github.com/acme/tool"), Across::ToMachine(&[]), &projects, &repositories),
        None
    );
    assert_eq!(counterpart(root, None, Across::Between, &projects, &repositories), None);
    let elsewhere = [Placement {
        local_root: PathBuf::from("C:/src/other"),
        remote_root: projects[0].1.clone(),
    }];
    assert_eq!(
        counterpart(root, None, Across::ToMachine(&elsewhere), &projects, &repositories),
        None
    );
}

#[test]
fn a_placement_is_kept_once_per_project_of_this_computer() {
    let dir = scratch("kept");
    assert_eq!(placements(&dir), []);
    let placed = |remote: &str| Placement {
        local_root: "C:/src/notes".into(),
        remote_root: remote.into(),
    };
    place(&dir, placed("/home/me/notes")).unwrap();
    place(&dir, placed("/home/me/projects/notes")).unwrap();
    assert_eq!(placements(&dir), [placed("/home/me/projects/notes")]);
}

/// A Machine set up again under another target is the same Machine: its
/// Placements and unfinished copies go with it.
#[test]
fn what_is_kept_of_a_machine_follows_it_to_its_new_target() {
    let dir = scratch("follow");
    let (old, new) = (dir.join("old"), dir.join("new"));
    let placed = Placement {
        local_root: "C:/src/notes".into(),
        remote_root: "/home/me/projects/notes".into(),
    };
    place(&old, placed.clone()).unwrap();
    let copy = Unfinished {
        local_root: "C:/src/app".into(),
        remote_root: "/home/me/projects/app".into(),
        paths: BTreeSet::from(["main.rs".to_string()]),
    };
    begun(&old, copy.clone()).unwrap();

    follow(&old, &new).unwrap();
    assert_eq!(placements(&new), [placed]);
    assert_eq!(unfinished_at(&new, &copy.local_root, &copy.remote_root), Some(copy));
    assert_eq!(placements(&old), []);
    assert_eq!(unfinished(&old, Path::new("C:/src/app")), Vec::<PathBuf>::new());
}

#[test]
fn a_destination_is_a_folder_under_the_home_or_the_root_never_either_itself() {
    for fine in ["~/projects/app", "/srv/app", "~/a b/c/", r"D:\work\app", "c:/work/app", "~/projects/.app"] {
        assert_eq!(refuse_destination(fine), None, "{fine}");
    }
    for refused in [
        "",
        "~",
        "~/",
        "/",
        "projects/app",
        "~/../x",
        "/srv/../etc",
        r"D:\",
        "D:",
        r"D:\work\..\x",
        "~/projects/.",
        "~/projects/./app",
    ] {
        assert!(refuse_destination(refused).is_some(), "{refused:?}");
    }
}

fn git(root: &Path, args: &[&str]) -> String {
    crate::workspace::git_for_tests(root, args)
}

/// A committed checkout at `root`.
fn checkout(root: &Path, files: &[(&str, &[u8])], ignore: &str) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "operator@example.invalid"]);
    git(root, &["config", "user.name", "operator"]);
    git(root, &["config", "core.autocrlf", "false"]);
    fs::write(root.join(".gitignore"), ignore).unwrap();
    for (path, bytes) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "root"]);
}

#[test]
fn what_git_ignores_is_listed_each_directory_whole_with_its_size() {
    let root = scratch("ignored");
    git(&root, &["init", "-q"]);
    fs::write(root.join(".gitignore"), "target/\n*.log\n.env\n").unwrap();
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    fs::create_dir_all(root.join("target/debug")).unwrap();
    fs::write(root.join("target/debug/app"), [0u8; 300]).unwrap();
    fs::write(root.join("target/notes.txt"), "built").unwrap();
    fs::write(root.join("run.log"), "log").unwrap();
    fs::write(root.join(".env"), "KEY=1\n").unwrap();
    fs::create_dir_all(root.join("logs")).unwrap();
    fs::write(root.join("logs/a.log"), "a").unwrap();
    fs::write(root.join("logs/b.log"), "b").unwrap();

    let mut listed = ignored(&root);
    listed.sort_by(|a, b| a.path.cmp(&b.path));
    let entry = |path: &str, directory| Ignored {
        path: path.into(),
        directory,
    };
    assert_eq!(
        listed,
        [
            entry(".env", false),
            entry("logs", true),
            entry("run.log", false),
            entry("target", true)
        ],
        "a folder of only ignored files is one entry: its files are not listed again"
    );
    assert_eq!(size(&root, "target"), 305);
    assert_eq!(ignored(&scratch("not-git")), []);
}

/// A linked worktree's `.git` names its repository by a path of this
/// computer: one is never copied, neither as the Project nor inside it.
#[test]
fn a_linked_worktree_is_never_copied() {
    let root = scratch("worktrees");
    checkout(&root, &[("main.rs", b"fn main() {}\n")], ".worktrees/\n.env\n");
    fs::write(root.join(".env"), "KEY=1\n").unwrap();
    git(&root, &["worktree", "add", "-q", "-b", "feature", ".worktrees/feature"]);
    let outside = root.with_file_name(format!("{}-beside", root.file_name().unwrap().to_string_lossy()));
    let _ = fs::remove_dir_all(&outside);
    git(&root, &["worktree", "add", "-q", "-b", "beside", &outside.display().to_string()]);

    assert_eq!(refuse_source(&root), None);
    assert!(refuse_source(&root.join(".worktrees/feature")).is_some(), "a worktree is refused");
    assert_eq!(worktrees(&root), [".worktrees/feature"], "only those inside it");
    let (shown, left_out) = without_worktrees(ignored(&root), &worktrees(&root));
    assert_eq!(shown.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>(), [".env"]);
    assert_eq!(left_out, [".worktrees"]);
    let (shown, left_out) = without_worktrees(Vec::new(), &["wt/one".to_string()]);
    assert!(shown.is_empty());
    assert_eq!(left_out, ["wt/one"], "a worktree git does not ignore is left out too");
}

/// Listed where no permission bits are kept (Windows), a copy's files take
/// theirs from git: executable as the index has it, private where git
/// ignores them, readable by all otherwise. Bits kept stay as they are.
#[test]
fn a_copy_from_a_side_that_keeps_no_bits_takes_them_from_git() {
    let root = scratch("modes");
    git(&root, &["init", "-q"]);
    fs::write(root.join(".gitignore"), ".env\ntarget/\n").unwrap();
    fs::write(root.join("run.sh"), "#!/bin/sh\n").unwrap();
    fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("notes.txt"), "not added yet").unwrap();
    fs::write(root.join(".env"), "KEY=1\n").unwrap();
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("target/app"), "built").unwrap();
    git(&root, &["add", ".gitignore", "run.sh", "main.rs"]);
    git(&root, &["update-index", "--chmod=+x", "run.sh"]);

    let mut listed = tree::scan(&root, &[]).unwrap();
    for entry in listed.entries.values_mut() {
        if let Entry::File { mode, .. } = entry {
            *mode = None;
        }
    }
    listed.entries.insert(
        "kept".into(),
        Entry::File {
            size: 0,
            sha256: String::new(),
            mode: Some(0o700),
            modified: None,
        },
    );
    Checkout::from_windows(&root).list(&mut listed);
    let mode = |path: &str| match &listed.entries[path] {
        Entry::File { mode, .. } => *mode,
        entry => panic!("{path} is {entry:?}"),
    };
    assert_eq!(mode("run.sh"), Some(0o755));
    for readable in ["main.rs", "notes.txt", ".gitignore", ".git/HEAD"] {
        assert_eq!(mode(readable), Some(0o644), "{readable}");
    }
    assert_eq!(mode(".env"), Some(0o600));
    assert_eq!(mode("target/app"), Some(0o600));
    assert_eq!(mode("kept"), Some(0o700));
}

/// A Windows checkout's text files are CRLF where git's index has LF. The
/// copy sends what git would check out on a Machine: such a file as LF,
/// an uncommitted edit included, its size and hash those of what is sent;
/// a file committed with CRLF, or binary, byte for byte; the copy's git
/// config without this computer's line-end, file-mode and case settings;
/// and its index read there as git would, so only the edit is a change.
#[test]
fn a_copy_from_windows_sends_what_git_would_check_out_on_the_machine() {
    let root = scratch("crlf");
    checkout(
        &root,
        &[
            ("scripts/run.sh", b"#!/bin/sh\necho ran\n"),
            ("edited.txt", b"one\n"),
            ("crlf.bat", b"echo\r\n"),
            ("image.bin", b"\x00\x01\r\n\x02"),
        ],
        "",
    );
    git(&root, &["config", "core.filemode", "false"]);
    git(&root, &["config", "core.autocrlf", "true"]);
    git(&root, &["config", "core.ignorecase", "true"]);
    for path in ["scripts/run.sh", "edited.txt"] {
        fs::remove_file(root.join(path)).unwrap();
    }
    git(&root, &["checkout", "--", "."]);
    assert_eq!(fs::read(root.join("scripts/run.sh")).unwrap(), b"#!/bin/sh\r\necho ran\r\n");
    fs::write(root.join("edited.txt"), b"one\r\ntwo\r\n").unwrap();

    let checkout = Checkout::from_windows(&root);
    let mut listed = tree::scan(&root, &[]).unwrap();
    checkout.list(&mut listed);
    let sent = |path: &str| {
        let mut bytes = Vec::new();
        tree::Source::open(&checkout, path).unwrap().read_to_end(&mut bytes).unwrap();
        bytes
    };
    assert_eq!(sent("scripts/run.sh"), b"#!/bin/sh\necho ran\n");
    assert_eq!(sent("edited.txt"), b"one\ntwo\n", "the edit goes, as LF");
    assert_eq!(sent("crlf.bat"), b"echo\r\n", "committed as CRLF");
    assert_eq!(sent("image.bin"), b"\x00\x01\r\n\x02", "binary");
    for path in ["scripts/run.sh", "edited.txt", "crlf.bat", ".git/config"] {
        let Entry::File { size, sha256, .. } = &listed.entries[path] else {
            panic!("{path} is a file");
        };
        let bytes = sent(path);
        assert_eq!((*size, sha256.as_str()), (bytes.len() as u64, chunked::hex(&Sha256::digest(&bytes)).as_str()), "{path}");
    }
    let config = String::from_utf8(sent(".git/config")).unwrap();
    assert!(
        ["autocrlf", "filemode", "ignorecase"].iter().all(|setting| !config.contains(setting)),
        "{config}"
    );
    assert!(config.contains("[user]") && config.contains("repositoryformatversion"), "{config}");

    assert_eq!(status_there(&root, "crlf-there"), " M edited.txt\n", "only the edit is a change");
    git(&root, &["update-index", "--index-version", "4"]);
    assert_eq!(status_there(&root, "crlf-there-v4"), " M edited.txt\n", "an index of version 4 too");
}

/// A copy from Windows makes LF only what git itself made CRLF: a file
/// whose attributes say text (`text`, `text=auto`) or say nothing under
/// `core.autocrlf`. One git keeps byte for byte (`-text`, `binary`,
/// `eol=crlf`, a `working-tree-encoding`) goes as it is here, an
/// uncommitted CRLF edit and all, which git there still reads as one.
#[test]
fn a_copy_from_windows_converts_only_what_git_itself_converts() {
    let root = scratch("eol-table");
    let utf16 = b"\xff\xfeo\x00n\x00e\x00\n\x00";
    checkout(
        &root,
        &[
            (
                ".gitattributes",
                b"raw.txt -text\nbin.dat binary\nwin.bat eol=crlf\nenc.txt working-tree-encoding=UTF-16LE-BOM text\nauto.txt text=auto\ntext.txt text\n",
            ),
            ("auto.txt", b"one\ntwo\n"),
            ("text.txt", b"one\ntwo\n"),
            ("plain.txt", b"one\ntwo\n"),
            ("raw.txt", b"one\ntwo\n"),
            ("bin.dat", b"a\nb\n"),
            ("win.bat", b"echo\n"),
            ("enc.txt", utf16),
        ],
        "",
    );
    git(&root, &["config", "core.autocrlf", "true"]);
    for path in ["auto.txt", "text.txt", "plain.txt", "raw.txt", "bin.dat", "win.bat", "enc.txt"] {
        fs::remove_file(root.join(path)).unwrap();
    }
    git(&root, &["checkout", "--", "."]);
    fs::write(root.join("raw.txt"), b"one\r\ntwo\r\nthree\r\n").unwrap();
    fs::write(root.join("bin.dat"), b"a\r\nb\r\n").unwrap();
    assert_eq!(git(&root, &["status", "--porcelain"]), " M bin.dat\n M raw.txt\n");

    let sent = |checkout: &Checkout, path: &str| {
        let mut bytes = Vec::new();
        tree::Source::open(checkout, path).unwrap().read_to_end(&mut bytes).unwrap();
        bytes
    };
    let checkout = Checkout::from_windows(&root);
    for converted in ["auto.txt", "text.txt", "plain.txt"] {
        assert_eq!(sent(&checkout, converted), b"one\ntwo\n", "{converted}");
    }
    for (kept, why) in [("raw.txt", "-text"), ("bin.dat", "binary"), ("win.bat", "eol=crlf"), ("enc.txt", "an encoding")] {
        assert_eq!(sent(&checkout, kept), fs::read(root.join(kept)).unwrap(), "{kept}: {why}");
    }
    assert_eq!(fs::read(root.join("win.bat")).unwrap(), b"echo\r\n");
    assert_eq!(
        status_there(&root, "eol-table-there"),
        " M bin.dat\n M raw.txt\n",
        "the edits are still edits there"
    );

    git(&root, &["config", "core.autocrlf", "false"]);
    let checkout = Checkout::from_windows(&root);
    assert_eq!(
        sent(&checkout, "plain.txt"),
        b"one\r\ntwo\r\n",
        "with no attribute and no core.autocrlf, git made no CRLF"
    );
    assert_eq!(sent(&checkout, "text.txt"), b"one\ntwo\n");
}

/// What git on a Machine says of the Windows checkout at `root` once it
/// lands in the folder `name`, line ends and modes as a Machine has them.
fn status_there(root: &Path, name: &str) -> String {
    let checkout = Checkout::from_windows(root);
    let mut listed = tree::scan(root, &[]).unwrap();
    checkout.list(&mut listed);
    let there = scratch(name);
    for (path, entry) in &listed.entries {
        let at = there.join(path);
        match entry {
            Entry::Dir => fs::create_dir_all(&at).unwrap(),
            Entry::File { .. } => {
                let mut bytes = Vec::new();
                tree::Source::open(&checkout, path).unwrap().read_to_end(&mut bytes).unwrap();
                fs::create_dir_all(at.parent().unwrap()).unwrap();
                fs::write(&at, bytes).unwrap();
            }
            Entry::Symlink { .. } | Entry::Unknown => {}
        }
    }
    git(&there, &["-c", "core.autocrlf=false", "-c", "core.filemode=false", "status", "--porcelain"])
}

/// A Server in this process, each reply heard in the order asked. It
/// cancels `cancel` once `begun` files have been begun.
struct Local {
    session: Session,
    replies: VecDeque<(u64, io::Result<Reply>)>,
    next: u64,
    begun: usize,
    cancel: Option<(Cancellation, usize)>,
}

impl Local {
    fn new() -> Self {
        Self {
            session: Session::default(),
            replies: VecDeque::new(),
            next: 0,
            begun: 0,
            cancel: None,
        }
    }

    fn cancelling(cancel: &Cancellation, after: usize) -> Self {
        Self {
            cancel: Some((cancel.clone(), after)),
            ..Self::new()
        }
    }
}

impl Exchange for Local {
    fn ask(&mut self, request: Request) -> io::Result<Asked> {
        if matches!(request, Request::Begin { .. }) {
            self.begun += 1;
            if let Some((cancel, after)) = &self.cancel {
                if self.begun >= *after {
                    cancel.cancel();
                }
            }
        }
        self.next += 1;
        self.replies.push_back((self.next, self.session.execute(request)));
        Ok(Asked::numbered(self.next))
    }

    fn answer(&mut self, asked: Asked) -> io::Result<Reply> {
        while let Some((id, reply)) = self.replies.pop_front() {
            if id == asked.id() {
                return reply;
            }
        }
        Err(io::Error::other("never asked"))
    }
}

/// What a tree holds, by path, as a manifest has it.
fn holds(dir: &Path) -> BTreeSet<String> {
    tree::scan(dir, &[]).unwrap().entries.into_keys().collect()
}

/// A copy stopped part way is kept as unfinished, on this computer, from
/// before its first byte. Picked up again, it sends only what differs and
/// removes only what it sent that the Project no longer holds: a file
/// someone added there since stays.
#[test]
fn an_unfinished_copy_is_kept_and_its_resume_keeps_what_others_put_there() {
    let dir = scratch("resume");
    let (source, dest, kept) = (dir.join("app"), dir.join("there/app"), dir.join("machine"));
    fs::create_dir_all(&source).unwrap();
    for at in 0..8 {
        fs::write(source.join(format!("file-{at}.txt")), format!("{at}")).unwrap();
    }
    fs::write(source.join("gone-later.txt"), "sent, then removed here").unwrap();
    let job = CopyJob {
        source: &source,
        dest: &dest,
        skip: &[],
        kept: &kept,
    };

    let cancel = Cancellation::default();
    let mut heard = Vec::new();
    let stopped = copy(&job, &mut Local::cancelling(&cancel, 3), &mut |copying| heard.push(copying), &cancel)
        .expect_err("cancelled part way");
    assert_eq!(stopped.error.kind(), io::ErrorKind::Interrupted);
    assert!(heard.iter().any(|copying| matches!(copying, Copying::Sending(_))));
    assert_eq!(unfinished(&kept, &source), [dest.clone()], "kept as unfinished");
    let landed = holds(&dest);
    assert!(!landed.is_empty() && landed.len() < 9, "part of it landed: {landed:?}");

    fs::write(dest.join("added-there.txt"), "theirs").unwrap();
    fs::remove_file(source.join("gone-later.txt")).unwrap();
    fs::write(dest.join("gone-later.txt"), "sent by the copy").unwrap();
    let resumed = copy(&job, &mut Local::new(), &mut |_| {}, &Cancellation::default()).unwrap();
    let Copied::Sent(sent) = resumed else {
        panic!("a resume is never refused as occupied");
    };
    assert_eq!(sent.removed, 1, "what the copy sent and the Project no longer holds");
    let mut expected = holds(&source);
    expected.insert("added-there.txt".into());
    assert_eq!(holds(&dest), expected);
    assert_eq!(fs::read_to_string(dest.join("added-there.txt")).unwrap(), "theirs");
    assert_eq!(unfinished(&kept, &source), Vec::<PathBuf>::new(), "finished");
}

/// A first copy never changes what is there: a destination that holds
/// anything, however recently, takes none, read by the copy's own scan.
#[test]
fn a_first_copy_into_a_destination_that_holds_anything_sends_nothing() {
    let dir = scratch("occupied");
    let (source, dest, kept) = (dir.join("app"), dir.join("there/app"), dir.join("machine"));
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("main.rs"), "fn main() {}").unwrap();
    fs::create_dir_all(dest.join("node_modules")).unwrap();
    fs::write(dest.join("node_modules/theirs.js"), "theirs").unwrap();
    let skip = ["node_modules".to_string()];
    let job = CopyJob {
        source: &source,
        dest: &dest,
        skip: &skip,
        kept: &kept,
    };
    let copied = copy(&job, &mut Local::new(), &mut |_| {}, &Cancellation::default()).unwrap();
    assert!(matches!(copied, Copied::Occupied), "even what it leaves out counts");
    assert_eq!(holds(&dest), BTreeSet::from(["node_modules".into(), "node_modules/theirs.js".into()]));
    assert_eq!(unfinished(&kept, &source), Vec::<PathBuf>::new());
}

/// Cancelled while the Project is listed, a copy stops there: it reads no
/// more of it and sends nothing. The listing says how far it has got.
#[test]
fn a_copy_cancelled_while_listing_stops_listing_and_counts_as_it_goes() {
    let dir = scratch("listing");
    let (source, dest, kept) = (dir.join("app"), dir.join("there/app"), dir.join("machine"));
    fs::create_dir_all(&source).unwrap();
    for at in 0..(COUNTED * 4) {
        fs::write(source.join(format!("{at:04}.txt")), "").unwrap();
    }
    let job = CopyJob {
        source: &source,
        dest: &dest,
        skip: &[],
        kept: &kept,
    };
    let cancel = Cancellation::default();
    let mut counts = Vec::new();
    let stopped = copy(
        &job,
        &mut Local::new(),
        &mut |copying| {
            if let Copying::Listing(count) = copying {
                counts.push(count);
                if count >= COUNTED * 2 {
                    cancel.cancel();
                }
            }
        },
        &cancel,
    )
    .expect_err("cancelled");
    assert_eq!(stopped.error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(counts, [COUNTED, COUNTED * 2], "it stopped at the next entry");
    assert!(!dest.exists(), "nothing sent");
    assert_eq!(unfinished(&kept, &source), Vec::<PathBuf>::new());
}
