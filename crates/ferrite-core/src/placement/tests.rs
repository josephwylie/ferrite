use std::collections::{BTreeMap, VecDeque};

use super::*;
use crate::project::ProjectId;
use crate::protocol::Repositories;
use crate::tree::{Asked, Reply, Request, Session};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-placement-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dunce::canonicalize(&dir).unwrap()
}

/// The id and root of a Project at each of `roots`, as a Machine whose
/// registry is new numbers and canonicalizes them.
fn registered(dir: &Path, roots: &[&str]) -> Vec<(ProjectId, PathBuf)> {
    roots
        .iter()
        .zip(1..)
        .map(|(root, id)| {
            let root = dir.join(root);
            fs::create_dir_all(&root).unwrap();
            (ProjectId(id), dunce::canonicalize(&root).unwrap())
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
    };
    begun(&old, copy.clone()).unwrap();

    follow(&old, &new).unwrap();
    assert_eq!(placements(&new), [placed]);
    assert_eq!(unfinished(&new, &copy.local_root), [copy.remote_root]);
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
    crate::git_read::git(root, args).unwrap()
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
    // What a copy sends but the entries it leaves out: the rest of the
    // folder, `.git` and `.gitignore` included.
    let skip: Vec<String> = listed.iter().map(|entry| entry.path.clone()).collect();
    let rest = size_but(&root, &skip);
    assert_eq!(rest, size_but(&root, &[]) - 305 - 3 - 6 - 2);
    assert_eq!(rest, size(&root, ".git") + 19 + 13);
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
    assert_eq!(
        refuse_source(&root.join(".worktrees/feature")),
        Some(format!(
            "This folder is a git worktree. Copy its main checkout, {}, instead.",
            root.display()
        )),
        "a worktree is refused, naming its main checkout"
    );
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
            text: None,
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
/// cancels `cancel` once `after` of the requests `counts` picks have
/// been asked.
struct Local {
    session: Session,
    replies: VecDeque<(u64, io::Result<Reply>)>,
    next: u64,
    counted: usize,
    cancel: Option<(Cancellation, fn(&Request) -> bool, usize)>,
    refuse: bool,
    /// Refuses every link, as a Windows Server did before it made them.
    refuse_links: bool,
    /// Once it has cancelled, the line drops: no reply is heard again,
    /// though what was asked was done.
    drops_once_cancelled: bool,
}

impl Local {
    fn new() -> Self {
        Self {
            session: Session::default(),
            replies: VecDeque::new(),
            next: 0,
            counted: 0,
            cancel: None,
            refuse: false,
            refuse_links: false,
            drops_once_cancelled: false,
        }
    }

    /// One that cancels `cancel` once `after` files have been begun.
    fn cancelling(cancel: &Cancellation, after: usize) -> Self {
        Self {
            cancel: Some((cancel.clone(), |request| matches!(request, Request::Begin { .. }), after)),
            ..Self::new()
        }
    }

    /// One that cancels `cancel` once `after` files have been finished:
    /// with the last, the copy has asked all it sends.
    fn cancelling_once_finished(cancel: &Cancellation, after: usize) -> Self {
        Self {
            cancel: Some((cancel.clone(), |request| matches!(request, Request::Finish { .. }), after)),
            ..Self::new()
        }
    }

    /// One that refuses every write, as a folder no one may write to does.
    fn refusing() -> Self {
        Self {
            refuse: true,
            ..Self::new()
        }
    }
}

impl Exchange for Local {
    fn ask(&mut self, request: Request) -> io::Result<Asked> {
        if let Some((cancel, counts, after)) = &self.cancel {
            if counts(&request) {
                self.counted += 1;
                if self.counted >= *after {
                    cancel.cancel();
                }
            }
        }
        self.next += 1;
        let reply = if self.refuse && !matches!(request, Request::Scan { .. }) {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "Permission denied"))
        } else if self.refuse_links && matches!(request, Request::Link { .. }) {
            Err(io::Error::new(io::ErrorKind::Unsupported, "this Server does not make symlinks on Windows"))
        } else {
            self.session.execute(request)
        };
        self.replies.push_back((self.next, reply));
        Ok(Asked::numbered(self.next))
    }

    fn answer(&mut self, asked: Asked) -> io::Result<Reply> {
        if self.drops_once_cancelled && self.cancel.as_ref().is_some_and(|(_, _, after)| self.counted >= *after) {
            return Err(io::Error::new(io::ErrorKind::ConnectionReset, "The connection to the server closed"));
        }
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

/// What a copy that ended sent there, and what it found changed there.
fn sent(copied: Copied) -> (Sent, Vec<String>) {
    match copied {
        Copied::Sent { sent, changed } => (sent, changed),
        copied => panic!("nothing was sent: {copied:?}"),
    }
}

/// `dir/app`, `files` files, copied to `dir/there/app` and stopped part
/// way: what landed there.
fn half_copied(dir: &Path, files: usize) -> BTreeSet<String> {
    let (source, dest, kept) = (dir.join("app"), dir.join("there/app"), dir.join("machine"));
    fs::create_dir_all(&source).unwrap();
    for at in 0..files {
        fs::write(source.join(format!("file-{at}.txt")), format!("{at}")).unwrap();
    }
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
    // Each reading names the file its last bytes were of.
    assert!(heard.iter().any(|copying| matches!(
        copying,
        Copying::Sending { file: Some(file), .. } if file.starts_with("file-")
    )));
    assert_eq!(unfinished(&kept, &source), [dest.clone()], "kept as unfinished");
    let landed = holds(&dest);
    assert!(!landed.is_empty() && landed.len() < files, "part of it landed: {landed:?}");
    landed
}

/// A copy cancelled once it has asked all it sends, while the replies
/// come, is stopped all the same: kept as unfinished, as the sheet says,
/// and its Resume finds all of it there.
#[test]
fn a_copy_cancelled_as_its_last_replies_come_stays_unfinished() {
    let dir = scratch("cancelled-last");
    let (source, dest, kept) = (dir.join("app"), dir.join("there/app"), dir.join("machine"));
    fs::create_dir_all(&source).unwrap();
    for at in 0..3 {
        fs::write(source.join(format!("file-{at}.txt")), format!("{at}")).unwrap();
    }
    let job = CopyJob {
        source: &source,
        dest: &dest,
        skip: &[],
        kept: &kept,
    };
    let cancel = Cancellation::default();
    let stopped = copy(&job, &mut Local::cancelling_once_finished(&cancel, 3), &mut |_| {}, &cancel)
        .expect_err("cancelled before it was done");
    assert_eq!(stopped.error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(holds(&dest), holds(&source), "all of it landed");
    assert_eq!(unfinished(&kept, &source), [dest.clone()], "kept as unfinished");

    let (resumed, changed) = sent(copy(&job, &mut Local::new(), &mut |_| {}, &Cancellation::default()).unwrap());
    assert_eq!((resumed.written, changed), (0, Vec::<String>::new()), "nothing left to send");
    assert_eq!(unfinished(&kept, &source), [dest.clone()], "unfinished until it is added");
    added_there(&kept, &source, &dest).unwrap();
    assert_eq!(unfinished(&kept, &source), Vec::<PathBuf>::new(), "finished");
}

/// A first copy cancelled once the Machine took its first file, the line
/// dropping before that file's replies are heard: what landed there is
/// not known to be nothing, so the copy is kept as unfinished, and its
/// Resume finds that file there.
#[test]
fn a_copy_whose_line_drops_as_its_cancel_is_heard_stays_unfinished() {
    let dir = scratch("cancelled-dropped");
    let (source, dest, kept) = (dir.join("app"), dir.join("there/app"), dir.join("machine"));
    fs::create_dir_all(&source).unwrap();
    for at in 0..3 {
        fs::write(source.join(format!("file-{at}.txt")), format!("{at}")).unwrap();
    }
    let job = CopyJob {
        source: &source,
        dest: &dest,
        skip: &[],
        kept: &kept,
    };
    let cancel = Cancellation::default();
    let mut dropping = Local {
        drops_once_cancelled: true,
        ..Local::cancelling_once_finished(&cancel, 1)
    };
    let stopped = copy(&job, &mut dropping, &mut |_| {}, &cancel).expect_err("cancelled part way");
    assert!(dest.join("file-0.txt").is_file(), "the Machine took the first file");
    assert_eq!(unfinished(&kept, &source), [dest.clone()], "kept as unfinished");
    assert!(!stopped.took_nothing(), "its replies were never heard: {stopped:?}");

    let (resumed, changed) = sent(copy(&job, &mut Local::new(), &mut |_| {}, &Cancellation::default()).unwrap());
    assert_eq!((resumed.written, changed), (2, Vec::<String>::new()), "the rest, and only the rest");
}

/// A copy stopped part way is kept as unfinished, on this computer, from
/// before its first byte, with no list of what it sent. Picked up again, it
/// sends what is not there: a file there as the copy sends it is done, and
/// one that is not is someone else's, left as it is and listed as changed.
/// It removes nothing, a file the Project no longer holds included.
#[test]
fn a_resume_sends_only_what_is_not_there_and_leaves_what_someone_put_there() {
    let dir = scratch("resume");
    let landed = half_copied(&dir, 8);
    let (source, dest, kept) = (dir.join("app"), dir.join("there/app"), dir.join("machine"));
    let unreached = (0..8)
        .map(|at| format!("file-{at}.txt"))
        .find(|path| !landed.contains(path))
        .expect("a file the copy had not reached");
    fs::write(dest.join(&unreached), "theirs").unwrap();
    fs::write(dest.join("added-there.txt"), "theirs too").unwrap();
    fs::remove_file(source.join("file-0.txt")).unwrap();

    let job = CopyJob {
        source: &source,
        dest: &dest,
        skip: &[],
        kept: &kept,
    };
    let (sent, changed) = sent(copy(&job, &mut Local::new(), &mut |_| {}, &Cancellation::default()).unwrap());
    assert_eq!(fs::read_to_string(dest.join(&unreached)).unwrap(), "theirs", "their file stays");
    assert_eq!(changed, [unreached.clone()], "and is listed as changed there");
    assert_eq!(fs::read_to_string(dest.join("added-there.txt")).unwrap(), "theirs too");
    assert_eq!(sent.removed, 0, "a resume removes nothing");
    let mut expected = holds(&source);
    expected.extend(["added-there.txt".to_string(), "file-0.txt".to_string()]);
    assert_eq!(holds(&dest), expected);
    for at in 1..8 {
        let path = format!("file-{at}.txt");
        if path != unreached {
            assert_eq!(fs::read_to_string(dest.join(&path)).unwrap(), at.to_string(), "{path}");
        }
    }
    assert_eq!(unfinished(&kept, &source), [dest.clone()], "unfinished until it is added");
}

/// A resume's progress is of the whole copy: what the stopped copy landed
/// counts as sent from its first reading, and the total is all the copy
/// carries, never only what is left.
#[test]
fn a_resume_counts_what_landed_as_sent_from_its_first_reading() {
    let dir = scratch("resume-progress");
    let landed = half_copied(&dir, 8);
    let (source, dest, kept) = (dir.join("app"), dir.join("there/app"), dir.join("machine"));
    let size = |root: &Path, path: &str| fs::metadata(root.join(path)).unwrap().len();
    let whole: u64 = (0..8).map(|at| size(&source, &format!("file-{at}.txt"))).sum();
    let there: u64 = landed.iter().filter(|path| path.ends_with(".txt")).map(|path| size(&dest, path)).sum();
    assert!(there > 0 && there < whole);
    let job = CopyJob {
        source: &source,
        dest: &dest,
        skip: &[],
        kept: &kept,
    };
    let mut heard = Vec::new();
    copy(&job, &mut Local::new(), &mut |copying| heard.push(copying), &Cancellation::default()).unwrap();
    let readings: Vec<Progress> = heard
        .into_iter()
        .filter_map(|copying| match copying {
            Copying::Sending { progress, .. } => Some(progress),
            Copying::Listing(_) => None,
        })
        .collect();
    assert_eq!(readings.first(), Some(&Progress { sent: there, total: whole }));
    assert_eq!(readings.last(), Some(&Progress { sent: whole, total: whole }));
    assert!(readings.windows(2).all(|pair| pair[0].sent <= pair[1].sent), "{readings:?}");
}

/// A link to the folder `target` (relative to the link's folder) at
/// `link`, as a package manager makes one: a symlink on Unix, a junction
/// on Windows, which needs no privilege.
fn dir_link(target: &str, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    {
        // `cmd` reads a `/` in a path as a switch.
        let spelled = |path: &Path| path.components().collect::<PathBuf>();
        let made = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(spelled(link))
            .arg(spelled(&link.parent().unwrap().join(target)))
            .output()
            .unwrap();
        assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    }
}

/// A copy an older Windows Server stopped at its first link is finished by
/// its Resume, the link made there and read through. A link a resume
/// finds there as the destination holds it is done, not someone else's.
#[test]
fn a_copy_stopped_at_a_link_is_finished_by_its_resume() {
    let dir = scratch("stopped-at-a-link");
    let (source, dest, kept) = (dir.join("app"), dir.join("there/app"), dir.join("machine"));
    let package = source.join("node_modules/.pnpm/pkg@1.0.0/node_modules/pkg");
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("index.js"), "module.exports = 1;\n").unwrap();
    fs::write(source.join("README.md"), "# app\n").unwrap();
    dir_link(".pnpm/pkg@1.0.0/node_modules/pkg", &source.join("node_modules/pkg"));
    let job = CopyJob {
        source: &source,
        dest: &dest,
        skip: &[],
        kept: &kept,
    };
    let mut refusing = Local {
        refuse_links: true,
        ..Local::new()
    };
    let stopped = copy(&job, &mut refusing, &mut |_| {}, &Cancellation::default()).expect_err("stopped at the link");
    assert!(stopped.to_string().starts_with("node_modules/pkg: "), "{stopped}");
    assert_eq!(unfinished(&kept, &source), [dest.clone()]);

    let (resumed, changed) = sent(copy(&job, &mut Local::new(), &mut |_| {}, &Cancellation::default()).unwrap());
    assert_eq!((resumed.linked, resumed.unmade.len()), (1, 0), "{resumed:?}");
    assert_eq!(changed, Vec::<String>::new());
    assert_eq!(fs::read(dest.join("node_modules/pkg/index.js")).unwrap(), b"module.exports = 1;\n");
    assert_eq!(unfinished(&kept, &source), [dest.clone()], "unfinished until it is added");

    fs::remove_file(dest.join("README.md")).unwrap();
    let (again, changed) = sent(copy(&job, &mut Local::new(), &mut |_| {}, &Cancellation::default()).unwrap());
    assert_eq!(changed, Vec::<String>::new(), "the link there is this copy's");
    assert_eq!((again.written, again.linked), (1, 0));
}

/// A copy that stops before anything lands is not kept as unfinished:
/// there is nothing there to resume.
#[test]
fn a_copy_that_lands_nothing_is_not_kept_as_unfinished() {
    let dir = scratch("refused");
    let (source, dest, kept) = (dir.join("app"), dir.join("there/app"), dir.join("machine"));
    fs::create_dir_all(source.join("src")).unwrap();
    fs::write(source.join("src/main.rs"), "fn main() {}").unwrap();
    let job = CopyJob {
        source: &source,
        dest: &dest,
        skip: &[],
        kept: &kept,
    };
    let stopped = copy(&job, &mut Local::refusing(), &mut |_| {}, &Cancellation::default())
        .expect_err("every write refused");
    assert_eq!(stopped.error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(unfinished(&kept, &source), Vec::<PathBuf>::new());
}

/// A `Local` that at its first Begin cancels the copy, says so on `held`,
/// and goes on only once `gate` opens: a copy cancelled before anything it
/// asked was taken, slow to end.
struct Held {
    local: Local,
    cancel: Cancellation,
    held: std::sync::mpsc::Sender<()>,
    gate: std::sync::mpsc::Receiver<()>,
    began: bool,
}

impl Exchange for Held {
    fn ask(&mut self, request: Request) -> io::Result<Asked> {
        if matches!(request, Request::Begin { .. }) && !self.began {
            self.began = true;
            self.cancel.cancel();
            let _ = self.held.send(());
            let _ = self.gate.recv();
        }
        self.local.ask(request)
    }

    fn answer(&mut self, asked: Asked) -> io::Result<Reply> {
        self.local.answer(asked)
    }
}

/// Cancel, then Resume at once: the cancelled run, which took nothing,
/// ends after the next one began. It never undoes what that one keeps, so
/// the next run stopped part way is still kept for its own Resume.
#[test]
fn a_run_cancelled_before_anything_landed_never_undoes_the_next_ones_unfinished_copy() {
    use std::sync::mpsc;
    let dir = scratch("cancel-then-resume");
    let (source, dest, kept) = (dir.join("app"), dir.join("there/app"), dir.join("machine"));
    fs::create_dir_all(&source).unwrap();
    for at in 0..8 {
        fs::write(source.join(format!("file-{at}.txt")), format!("{at}")).unwrap();
    }
    let run = |exchange: Box<dyn Exchange + Send>, cancel: Cancellation| {
        let (source, dest, kept) = (source.clone(), dest.clone(), kept.clone());
        move || {
            let mut exchange = exchange;
            let job = CopyJob {
                source: &source,
                dest: &dest,
                skip: &[],
                kept: &kept,
            };
            copy(&job, &mut *exchange, &mut |_| {}, &cancel).map(drop)
        }
    };
    let (held, holding) = mpsc::channel();
    let (open, gate) = mpsc::channel();
    let cancel = Cancellation::default();
    let first = Held {
        local: Local::new(),
        cancel: cancel.clone(),
        held,
        gate,
        began: false,
    };
    let first = std::thread::spawn(run(Box::new(first), cancel));
    holding.recv().unwrap();
    assert_eq!(unfinished(&kept, &source), [dest.clone()], "kept from before its first byte");

    let cancel = Cancellation::default();
    let (ended, ending) = mpsc::channel();
    let second = run(Box::new(Local::cancelling(&cancel, 3)), cancel);
    let second = std::thread::spawn(move || {
        let stopped = second();
        let _ = ended.send(());
        stopped
    });
    // Without waiting on the first run, the second has ended by now.
    let _ = ending.recv_timeout(std::time::Duration::from_secs(2));
    open.send(()).unwrap();
    first.join().unwrap().expect_err("the first was cancelled");
    second.join().unwrap().expect_err("the second was cancelled part way");
    assert!(!holds(&dest).is_empty(), "the second landed part of it");
    assert_eq!(unfinished(&kept, &source), [dest.clone()], "kept for the second's Resume");
}

/// An unfinished copy whose destination holds another repository by now
/// is not this copy's: the copy takes none there, and it is no longer
/// kept. The same repository, its config changed here since, is resumed.
#[test]
fn a_resume_into_another_repository_takes_none() {
    let dir = scratch("another-repository");
    let (source, dest, kept) = (dir.join("app"), dir.join("there/app"), dir.join("machine"));
    fs::create_dir_all(&source).unwrap();
    checkout(&source, &[("main.rs", b"fn main() {}\n")], "");
    git(&source, &["remote", "add", "origin", "https://github.com/acme/app.git"]);
    let job = CopyJob {
        source: &source,
        dest: &dest,
        skip: &[],
        kept: &kept,
    };
    let cancel = Cancellation::default();
    copy(&job, &mut Local::cancelling(&cancel, 12), &mut |_| {}, &cancel).expect_err("cancelled part way");
    assert!(dest.join(".git/config").is_file(), "the copy got as far as the config");
    git(&source, &["config", "branch.main.remote", "origin"]);
    let resumed = copy(&job, &mut Local::new(), &mut |_| {}, &Cancellation::default()).unwrap();
    assert!(matches!(resumed, Copied::Sent { .. }), "the same repository: {resumed:?}");

    fs::remove_dir_all(&dest).unwrap();
    fs::create_dir_all(&dest).unwrap();
    git(&dest, &["init", "-q"]);
    git(&dest, &["remote", "add", "origin", "https://github.com/acme/other.git"]);
    begun(
        &kept,
        Unfinished {
            local_root: source.clone(),
            remote_root: dest.clone(),
        },
    )
    .unwrap();
    let before = holds(&dest);
    let copied = copy(&job, &mut Local::new(), &mut |_| {}, &Cancellation::default()).unwrap();
    assert!(matches!(copied, Copied::Occupied), "{copied:?}");
    assert_eq!(holds(&dest), before, "nothing there changed");
    assert_eq!(unfinished(&kept, &source), Vec::<PathBuf>::new(), "and it is not this copy's");
}

/// Aimed where another Project's copy is unfinished, a copy takes none,
/// and says whose it is.
#[test]
fn a_destination_with_another_projects_unfinished_copy_is_that_ones() {
    let dir = scratch("two-projects");
    half_copied(&dir, 8);
    let (first, dest, kept) = (dir.join("app"), dir.join("there/app"), dir.join("machine"));
    let second = dir.join("other/app");
    fs::create_dir_all(&second).unwrap();
    fs::write(second.join("main.rs"), "fn main() {}").unwrap();
    let before = holds(&dest);
    let job = CopyJob {
        source: &second,
        dest: &dest,
        skip: &[],
        kept: &kept,
    };
    let copied = copy(&job, &mut Local::new(), &mut |_| {}, &Cancellation::default()).unwrap();
    assert!(matches!(&copied, Copied::AnotherCopy { of } if *of == first), "{copied:?}");
    assert_eq!(holds(&dest), before);
    assert_eq!(unfinished(&kept, &first), [dest.clone()], "the first Project's copy is still unfinished");
}

/// Removed, a Machine takes along what this computer kept of it.
#[test]
fn a_machine_removed_is_forgotten() {
    let dir = scratch("forget");
    let placed = Placement {
        local_root: "C:/src/notes".into(),
        remote_root: "/home/me/projects/notes".into(),
    };
    place(&dir, placed).unwrap();
    let copy = Unfinished {
        local_root: "C:/src/app".into(),
        remote_root: "/home/me/projects/app".into(),
    };
    begun(&dir, copy.clone()).unwrap();
    forget(&dir).unwrap();
    assert_eq!(placements(&dir), []);
    assert_eq!(unfinished(&dir, &copy.local_root), Vec::<PathBuf>::new());
    forget(&dir).unwrap();
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
