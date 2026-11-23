//! Each open Thread's branch, for the windows on other computers, whose git
//! cannot read its checkout (ADR 0045). The sweep reads each checkout's
//! `HEAD` file off the pump, and runs git only for one that moved. The
//! branch goes out in the Thread's digest, so an idle link carries nothing
//! and an idle machine runs no git.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use ferrite_core::workspace;
use ferrite_core::ThreadId;

/// A checkout whose `HEAD` moved: what it says now, and the branch it names.
struct Moved {
    thread: ThreadId,
    head: Option<String>,
    branch: Option<String>,
}

#[derive(Default)]
pub(crate) struct Checkouts {
    /// What each open Thread's `HEAD` said when its branch was last read.
    heads: HashMap<ThreadId, Option<String>>,
    /// The look running off the pump.
    looking: Option<Receiver<Vec<Moved>>>,
}

impl Checkouts {
    /// Look at every open Thread's checkout, unless a look is still out: a
    /// hung mount holds up this and nothing else.
    pub(crate) fn look(&mut self, checkouts: Vec<(ThreadId, PathBuf)>) {
        if self.looking.is_some() || checkouts.is_empty() {
            return;
        }
        self.heads
            .retain(|thread, _| checkouts.iter().any(|(open, _)| open == thread));
        let known = self.heads.clone();
        let (answer, looking) = mpsc::channel();
        std::thread::spawn(move || {
            ferrite_core::spawn::serve_the_operator();
            let _ = answer.send(moved(checkouts, &known, workspace::checkout_branch));
        });
        self.looking = Some(looking);
    }

    /// The branches a finished look found moved.
    pub(crate) fn answered(&mut self) -> Vec<(ThreadId, Option<String>)> {
        let Some(looking) = &self.looking else {
            return Vec::new();
        };
        let moved = match looking.try_recv() {
            Ok(moved) => moved,
            Err(mpsc::TryRecvError::Empty) => return Vec::new(),
            Err(mpsc::TryRecvError::Disconnected) => Vec::new(),
        };
        self.looking = None;
        moved
            .into_iter()
            .map(|moved| {
                self.heads.insert(moved.thread, moved.head);
                (moved.thread, moved.branch)
            })
            .collect()
    }
}

/// The checkouts whose `HEAD` no longer says what `known` holds, each with
/// the branch `read` names for it. A checkout that did not move costs one
/// small file read and no process.
fn moved(
    checkouts: Vec<(ThreadId, PathBuf)>,
    known: &HashMap<ThreadId, Option<String>>,
    read: impl Fn(&Path) -> Option<String>,
) -> Vec<Moved> {
    checkouts
        .into_iter()
        .filter_map(|(thread, cwd)| {
            let head = workspace::head_of(&cwd);
            if known.get(&thread) == Some(&head) {
                return None;
            }
            let branch = head.as_ref().and_then(|_| read(&cwd));
            Some(Moved {
                thread,
                head,
                branch,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ferrite-checkouts-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        dir
    }

    /// An idle machine runs no git for its windows: only a `HEAD` that
    /// moved is read, once, and a checkout that stays costs no process.
    #[test]
    fn git_runs_only_for_a_head_that_moved() {
        let repo = scratch("moved");
        let head = repo.join(".git").join("HEAD");
        std::fs::write(&head, "ref: refs/heads/main\n").unwrap();
        let thread = ThreadId::new(1);
        let runs = Cell::new(0);
        let read = |_: &Path| {
            runs.set(runs.get() + 1);
            Some("named".to_string())
        };
        let mut known = HashMap::new();
        let look = |known: &mut HashMap<_, _>| {
            let found = moved(vec![(thread, repo.clone())], known, read);
            for moved in &found {
                known.insert(moved.thread, moved.head.clone());
            }
            found.len()
        };
        assert_eq!(look(&mut known), 1, "the first look reads it");
        for _ in 0..3 {
            assert_eq!(look(&mut known), 0, "nothing moved");
        }
        assert_eq!(runs.get(), 1, "git ran once");
        std::fs::write(&head, "ref: refs/heads/next\n").unwrap();
        assert_eq!(look(&mut known), 1, "its HEAD moved");
        assert_eq!(look(&mut known), 0);
        assert_eq!(runs.get(), 2, "and git ran once more");
        let _ = std::fs::remove_dir_all(&repo);
    }

    /// A linked worktree's `.git` is a file naming its admin directory:
    /// its own `HEAD` is the one that moves.
    #[test]
    fn a_linked_worktrees_head_is_its_own() {
        let repo = scratch("linked");
        let admin = repo.join(".git").join("worktrees").join("wt");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::write(admin.join("HEAD"), "ref: refs/heads/wt\n").unwrap();
        let tree = repo.join("wt");
        std::fs::create_dir_all(tree.join("src")).unwrap();
        std::fs::write(tree.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
        assert_eq!(
            workspace::head_of(&tree.join("src")).as_deref(),
            Some("ref: refs/heads/wt"),
            "from inside the tree, too"
        );
        let _ = std::fs::remove_dir_all(&repo);
    }

    /// A reftable repository's `HEAD` is a placeholder that never moves: a
    /// branch switch shows in its table list.
    #[test]
    fn a_reftable_repositorys_switch_is_seen() {
        let repo = scratch("reftable");
        let admin = repo.join(".git");
        std::fs::write(admin.join("HEAD"), "ref: refs/heads/.invalid\n").unwrap();
        std::fs::create_dir_all(admin.join("reftable")).unwrap();
        let tables = admin.join("reftable").join("tables.list");
        std::fs::write(&tables, "0x01.ref\n").unwrap();
        let before = workspace::head_of(&repo);
        std::fs::write(&tables, "0x01.ref\n0x02.ref\n").unwrap();
        assert_ne!(workspace::head_of(&repo), before);
        let _ = std::fs::remove_dir_all(&repo);
    }
}
