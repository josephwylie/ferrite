//! When the window reads the Threads' checkout labels from git: one read at
//! a time, always off the UI thread, and git only for a Thread whose
//! checkouts' files moved since its labels were last read
//! (`checkouts_stamp`), so an idle window starts none.
//!
//! What the labels say is the Facts' (`set_branches`); this keeps what
//! decides whether git runs at all.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use ferrite_core::checkout::BranchStatus;
use ferrite_core::ThreadKey;
use gpui::SharedString;

#[derive(Default)]
pub(super) struct BranchReads {
    /// One read at a time.
    reading: bool,
    /// A moment that must not be skipped (a turn ended) asked for a read
    /// while one was under way: another follows it.
    queued: bool,
    /// What each Thread's checkouts said (`checkouts_stamp`) when its
    /// labels were last read: a read runs git for a Thread only once they
    /// say something else.
    stamps: HashMap<ThreadKey, u64>,
    /// The files git tracks in each Thread's checkout (by its cwd) though
    /// an ignore rule matches them, as git last listed them
    /// (`git_read::tracked_ignored`): its stamp reads them by name.
    tracked_ignored: HashMap<PathBuf, Arc<[PathBuf]>>,
}

/// A Thread whose labels a read may ask git for: its cwd, each of its
/// Project's directories by label (the first one read at the cwd), and the
/// repo whose worktrees are listed.
pub(super) struct Target {
    pub(super) thread: ThreadKey,
    pub(super) cwd: PathBuf,
    pub(super) directories: Vec<(SharedString, PathBuf)>,
    pub(super) repo: Option<PathBuf>,
}

/// What git was asked, ready to run off the UI thread.
pub(super) struct Read {
    targets: Vec<Target>,
    known: HashMap<ThreadKey, u64>,
    tracked_ignored: HashMap<PathBuf, Arc<[PathBuf]>>,
    listing_wanted: bool,
}

/// What git said: each read Thread's stamp, branch status and its Project
/// directories' branches; each repo's worktrees; the tracked-ignored files
/// learned on the way.
pub(super) struct Said {
    pub(super) branches: Vec<(ThreadKey, u64, Option<BranchStatus>, Vec<(SharedString, SharedString)>)>,
    pub(super) listings: Vec<(ThreadKey, Vec<PathBuf>)>,
    learned: HashMap<PathBuf, Arc<[PathBuf]>>,
}

impl BranchReads {
    /// Whether a read is under way.
    pub(super) fn reading(&self) -> bool {
        self.reading
    }

    /// A read that must not be skipped: while one is under way it is
    /// queued behind it, and true is returned; otherwise the caller reads
    /// now.
    pub(super) fn queue_behind_read(&mut self) -> bool {
        if self.reading {
            self.queued = true;
        }
        self.reading
    }

    /// Whether a read was queued behind the one that landed: taking it
    /// clears it.
    pub(super) fn take_queued(&mut self) -> bool {
        std::mem::take(&mut self.queued)
    }

    /// Start reading `targets`: what is kept of a Thread or a checkout no
    /// longer among them goes, and the read is under way until `landed`.
    /// `listing_wanted` reads every Thread whatever its stamp says (a
    /// worktree a Main just made is named only by a listing taken after
    /// it). `None`, with nothing under way, when there is nothing to read.
    pub(super) fn begin(&mut self, targets: Vec<Target>, listing_wanted: bool) -> Option<Read> {
        self.stamps
            .retain(|thread, _| targets.iter().any(|target| target.thread == *thread));
        self.tracked_ignored
            .retain(|checkout, _| targets.iter().any(|target| target.cwd == *checkout));
        if targets.is_empty() {
            return None;
        }
        self.reading = true;
        Some(Read {
            targets,
            known: self.stamps.clone(),
            tracked_ignored: self.tracked_ignored.clone(),
            listing_wanted,
        })
    }

    /// A read landed: nothing is under way, and its stamps and the
    /// tracked-ignored files it learned are kept for the next.
    pub(super) fn landed(&mut self, said: &mut Said) {
        self.reading = false;
        self.stamps
            .extend(said.branches.iter().map(|(thread, stamp, ..)| (*thread, *stamp)));
        self.tracked_ignored.extend(std::mem::take(&mut said.learned));
    }
}

impl Read {
    /// Ask git, blocking: one `git worktree list` per repo, and one status
    /// per checkout, however many Threads share them.
    pub(super) fn run(self) -> Said {
        let Read {
            targets,
            known,
            tracked_ignored,
            listing_wanted,
        } = self;
        let mut listed: HashMap<PathBuf, Option<Vec<PathBuf>>> = HashMap::new();
        let mut walked = HashMap::new();
        let mut statuses = HashMap::new();
        let mut learned = HashMap::new();
        let mut listings = Vec::new();
        let branches = targets
            .into_iter()
            .filter_map(|Target { thread, cwd, directories, repo }| {
                // Before git reads: a write from here on moves the next
                // stamp.
                let checkout = *walked.entry(cwd.clone()).or_insert_with(|| {
                    let listed = tracked_ignored.get(&cwd).map_or(&[][..], |files| files);
                    ferrite_core::git_read::checkout_stamp(&cwd, listed)
                });
                let stamp = checkouts_stamp(checkout, &directories);
                if !listing_wanted && known.get(&thread) == Some(&stamp) {
                    return None;
                }
                if let Some(repo) = repo {
                    let listing = listed
                        .entry(repo.clone())
                        .or_insert_with(|| ferrite_core::git_read::worktree_paths(&repo).ok())
                        .clone();
                    if let Some(listing) = listing {
                        listings.push((thread, listing));
                    }
                }
                // A list that changed moves the next stamp, which reads it:
                // git runs once more, then rests.
                let status = statuses
                    .entry(cwd.clone())
                    .or_insert_with(|| {
                        let listed = ferrite_core::git_read::tracked_ignored(&cwd);
                        learned.insert(cwd.clone(), listed.into());
                        ferrite_core::git_read::branch_status(&cwd)
                    })
                    .clone();
                let project_branches = directories
                    .into_iter()
                    .filter_map(|(label, directory)| {
                        ferrite_core::git_read::checkout_branch(&directory)
                            .map(|branch| (label, SharedString::from(branch)))
                    })
                    .collect();
                Some((thread, stamp, status, project_branches))
            })
            .collect::<Vec<_>>();
        Said {
            branches,
            listings,
            learned,
        }
    }
}

/// What a Thread's checkouts say, read with no git process: its own
/// checkout whole (`checkout`, its `git_read::checkout_stamp`), and the
/// `HEAD` of each of its Project's directories, whose branch is all the
/// Thread shows of them.
fn checkouts_stamp(checkout: Option<u64>, directories: &[(SharedString, PathBuf)]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    checkout.hash(&mut hasher);
    for (_, directory) in directories {
        ferrite_core::git_read::head_of(directory).hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(thread: u64) -> Target {
        Target {
            thread: ThreadKey::fixture(0, thread),
            cwd: PathBuf::from(format!("/nowhere/{thread}")),
            directories: Vec::new(),
            repo: None,
        }
    }

    /// One read at a time: a read that must not be skipped, asked while
    /// one is under way, waits behind it and is there to take once it
    /// lands; asked with none under way, it is the caller's to run.
    #[test]
    fn a_read_asked_while_one_is_under_way_follows_it() {
        let mut reads = BranchReads::default();
        assert!(!reads.queue_behind_read(), "none under way: read now");
        assert!(!reads.take_queued());

        let read = reads.begin(vec![target(1)], false).expect("a Thread to read");
        assert!(reads.reading());
        assert!(reads.queue_behind_read(), "one under way: wait behind it");
        let mut said = read.run();
        reads.landed(&mut said);
        assert!(!reads.reading());
        assert!(reads.take_queued(), "the queued read follows the one that landed");
        assert!(!reads.take_queued(), "once");
    }

    /// Nothing to read starts nothing, and what is kept of a Thread no
    /// longer read goes: its next read runs git whatever its stamp said.
    #[test]
    fn a_thread_no_longer_read_is_forgotten() {
        let mut reads = BranchReads::default();
        let mut said = reads.begin(vec![target(1)], false).unwrap().run();
        reads.landed(&mut said);
        let thread = target(1).thread;
        assert!(reads.stamps.contains_key(&thread), "a read Thread keeps its stamp");

        assert!(reads.begin(Vec::new(), false).is_none());
        assert!(!reads.reading(), "nothing under way");
        assert!(!reads.stamps.contains_key(&thread));
    }
}
