//! A checkout read from its files, with no git process. What it says is
//! compared with what it said last time, so a caller on a cadence runs git
//! only for a checkout that moved: a Server's digest of each Thread's
//! branch (`head_of`), a window's sweep of each Thread's status
//! (`checkout_stamp`).

use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use ignore::WalkBuilder;

/// How long after a write a stamp is not trusted: a second write in the
/// same tick of the file system's clock, of the same size, would leave
/// every time and size as they were (git's "racily clean" entries).
const RACY: Duration = Duration::from_secs(2);

/// The checkout `cwd` is in: its top directory and its git dir.
pub(super) fn checkout_dirs(cwd: &Path) -> Option<(&Path, PathBuf)> {
    let root = cwd.ancestors().find(|dir| dir.join(".git").exists())?;
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some((root, dot_git));
    }
    let pointer = fs::read_to_string(&dot_git).ok()?;
    Some((root, root.join(pointer.trim().strip_prefix("gitdir:")?.trim())))
}

/// What the `HEAD` in git dir `admin` says.
fn head_said(admin: &Path) -> Option<String> {
    let mut said = fs::read_to_string(admin.join("HEAD")).ok()?.trim().to_string();
    // A reftable repository's `HEAD` is a placeholder: its table list moves.
    if let Ok(tables) = fs::read_to_string(admin.join("reftable").join("tables.list")) {
        said.push_str(&tables);
    }
    Some(said)
}

/// What `HEAD` says for the checkout `cwd` is in, read from its file with
/// no git process: the same words mean `checkout_branch` would answer the
/// same, so a caller on a cadence runs git only when they change. `None`
/// outside a checkout.
pub fn head_of(cwd: &Path) -> Option<String> {
    head_said(&checkout_dirs(cwd)?.1)
}

/// Everything `branch_status` and `worktree_paths` read of the checkout
/// `cwd` is in, as one number read with no git process: its `HEAD`, refs,
/// index and config, the names of its repository's worktrees, and the size
/// and time of every file in the work tree that git does not ignore, and of
/// each of `tracked_ignored` (`workspace::tracked_ignored`, as git last
/// listed them), which git reads though the walk never meets them. The
/// same number means git would answer the same, so a caller on a cadence
/// runs git only once it moves. Take it before git reads: a write after it
/// moves the next one. A stamp taken within two seconds of a write matches
/// no other. `None` outside a checkout.
pub fn checkout_stamp(cwd: &Path, tracked_ignored: &[PathBuf]) -> Option<u64> {
    stamp_at(cwd, tracked_ignored, SystemTime::now())
}

fn stamp_at(cwd: &Path, tracked_ignored: &[PathBuf], now: SystemTime) -> Option<u64> {
    let (root, admin) = checkout_dirs(cwd)?;
    let common = fs::read_to_string(admin.join("commondir"))
        .map_or_else(|_| admin.clone(), |dir| admin.join(dir.trim()));
    let mut stamp = Stamp::default();
    stamp.add(head_said(&admin)?);
    for file in [
        admin.join("index"),
        common.join("config"),
        common.join("packed-refs"),
        common.join("info").join("exclude"),
    ] {
        stamp.entry(&file, fs::symlink_metadata(&file).ok());
    }
    let mut refs = vec![admin.join("refs"), admin.join("reftable")];
    if common != admin {
        refs.extend([common.join("refs"), common.join("reftable")]);
    }
    for dir in refs.into_iter().filter(|dir| dir.is_dir()) {
        stamp.walk(WalkBuilder::new(dir).standard_filters(false));
    }
    // One admin directory per linked worktree: what `git worktree list` lists.
    for worktree in fs::read_dir(common.join("worktrees")).into_iter().flatten().flatten() {
        stamp.add(worktree.file_name());
    }
    // The work tree as `git status` walks it: what is not ignored, never
    // the git dir, nor a checkout nested inside (a worktree under
    // `.worktrees`), whose files are that checkout's.
    stamp.walk(
        WalkBuilder::new(root)
            .hidden(false)
            .ignore(false)
            .parents(false)
            .filter_entry(|entry| {
                entry.file_name() != ".git"
                    && !(entry.depth() > 0
                        && entry.file_type().is_some_and(|kind| kind.is_dir())
                        && entry.path().join(".git").exists())
            }),
    );
    for file in tracked_ignored.iter().map(|file| root.join(file)) {
        stamp.entry(&file, fs::symlink_metadata(&file).ok());
    }
    Some(stamp.finish(now))
}

/// A sum of one hash per entry, so the order a directory lists in is no
/// change, and the newest time it saw.
#[derive(Default)]
struct Stamp {
    sum: u64,
    newest: Option<SystemTime>,
}

impl Stamp {
    fn add(&mut self, item: impl Hash) {
        let mut hasher = DefaultHasher::new();
        item.hash(&mut hasher);
        self.sum = self.sum.wrapping_add(hasher.finish());
    }

    /// A directory by its name alone: its files say what changed in it,
    /// and a file it ignores changes its time too.
    fn entry(&mut self, path: &Path, meta: Option<fs::Metadata>) {
        match meta.filter(|meta| !meta.is_dir()) {
            Some(meta) => {
                let modified = meta.modified().ok();
                self.newest = self.newest.max(modified);
                self.add((path, meta.len(), modified));
            }
            None => self.add(path),
        }
    }

    fn walk(&mut self, walk: &mut WalkBuilder) {
        for entry in walk.build().flatten() {
            self.entry(entry.path(), entry.metadata().ok());
        }
    }

    fn finish(mut self, now: SystemTime) -> u64 {
        if self
            .newest
            .is_some_and(|newest| now.duration_since(newest).is_ok_and(|age| age < RACY))
        {
            self.add(now);
        }
        self.sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A checkout made of plain files, as git leaves one on disk: no git
    /// process, and nowhere near a real checkout.
    fn checkout(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("ferrite-stamp-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let git = root.join(".git");
        fs::create_dir_all(git.join("refs").join("heads")).unwrap();
        fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(git.join("refs").join("heads").join("main"), "1111\n").unwrap();
        fs::write(git.join("index"), "index").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src").join("lib.rs"), "fn a() {}\n").unwrap();
        fs::write(root.join(".gitignore"), "target/\n").unwrap();
        root
    }

    /// Long after every write here, so no stamp is racy unless a test says so.
    fn later() -> SystemTime {
        SystemTime::now() + Duration::from_secs(60)
    }

    fn moves(root: &Path, change: impl FnOnce()) -> bool {
        moves_listing(root, &[], change)
    }

    fn moves_listing(root: &Path, tracked_ignored: &[PathBuf], change: impl FnOnce()) -> bool {
        let before = stamp_at(root, tracked_ignored, later()).expect("a checkout");
        change();
        before != stamp_at(root, tracked_ignored, later()).expect("a checkout")
    }

    #[test]
    fn a_checkout_nobody_touches_keeps_its_stamp() {
        let root = checkout("still");
        assert!(!moves(&root, || {}));
        assert_eq!(
            stamp_at(&root.join("src"), &[], later()),
            stamp_at(&root, &[], later()),
            "any directory of the checkout reads the checkout"
        );
        assert_eq!(stamp_at(&std::env::temp_dir().join("ferrite-stamp-nowhere"), &[], later()), None);
        let _ = fs::remove_dir_all(&root);
    }

    /// A branch switch, a commit, a staged file, a changed file and a new
    /// one: each is something `git status` would answer differently.
    #[test]
    fn what_git_would_answer_differently_moves_it() {
        let root = checkout("moves");
        let git = root.join(".git");
        assert!(moves(&root, || fs::write(git.join("HEAD"), "ref: refs/heads/next\n").unwrap()));
        assert!(moves(&root, || fs::write(git.join("refs").join("heads").join("next"), "2222\n").unwrap()));
        assert!(moves(&root, || fs::write(git.join("index"), "index, staged").unwrap()));
        assert!(moves(&root, || fs::write(root.join("src").join("lib.rs"), "fn b() {}\n").unwrap()));
        assert!(moves(&root, || fs::write(root.join("src").join("new.rs"), "").unwrap()));
        assert!(moves(&root, || fs::create_dir_all(git.join("worktrees").join("wt")).unwrap()));
        let _ = fs::remove_dir_all(&root);
    }

    /// What `git status` never reads: an ignored file, the git dir's own
    /// scratch (reviewr's index copies), and a checkout nested inside.
    #[test]
    fn what_git_does_not_read_leaves_it() {
        let root = checkout("leaves");
        assert!(!moves(&root, || {
            fs::create_dir_all(root.join("target")).unwrap();
            fs::write(root.join("target").join("out.o"), "built").unwrap();
        }));
        assert!(!moves(&root, || {
            fs::create_dir_all(root.join(".git").join("reviewr")).unwrap();
            fs::write(root.join(".git").join("reviewr").join("index"), "copy").unwrap();
        }));
        let nested = root.join(".worktrees").join("wt");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join(".git"), "gitdir: ../../.git/worktrees/wt\n").unwrap();
        assert!(!moves(&root, || fs::write(nested.join("edit.rs"), "").unwrap()));
        let _ = fs::remove_dir_all(&root);
    }

    /// A file git tracks though an ignore rule matches it, force-added
    /// under `target/`: `git status` reports on it, but the walk never
    /// meets it, so it moves the stamp once git has listed it.
    #[test]
    fn a_tracked_file_an_ignore_rule_matches_moves_it_once_listed() {
        let root = checkout("tracked-ignored");
        let file = root.join("target").join("forced.txt");
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(&file, "one").unwrap();
        assert!(!moves(&root, || fs::write(&file, "two!").unwrap()));
        let listed = [PathBuf::from("target/forced.txt")];
        assert!(moves_listing(&root, &listed, || fs::write(&file, "three").unwrap()));
        assert!(moves_listing(&root, &listed, || fs::remove_file(&file).unwrap()));
        let _ = fs::remove_dir_all(&root);
    }

    /// Within two seconds of a write the times cannot vouch for the next
    /// one: the stamp matches nothing until they can.
    #[test]
    fn a_fresh_write_matches_no_other_stamp() {
        let root = checkout("racy");
        let now = SystemTime::now();
        assert_ne!(stamp_at(&root, &[], now), stamp_at(&root, &[], now + Duration::from_millis(1)));
        assert_eq!(
            stamp_at(&root, &[], later()),
            stamp_at(&root, &[], later() + Duration::from_secs(1))
        );
        let _ = fs::remove_dir_all(&root);
    }
}
