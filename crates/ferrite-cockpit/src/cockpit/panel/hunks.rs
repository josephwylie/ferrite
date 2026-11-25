//! A changed file's hunks under its block in the panel's Changes section
//! (SPEC §5.6, phase 7b). Folded by default; the block's head unfolds them.
//!
//! An unfolded file asks the window to read the checkout's Uncommitted
//! diff through the Review core — `Cockpit::review_diff`, the read
//! `Review changes` opens with — and the Overview's view builds its hunks
//! off the UI thread and keeps them per Thread and path. They are read
//! again when the file's diffstat moves (the agent edited it again); until
//! the new read lands the old hunks stay drawn.
//! A folded file reads nothing.
//!
//! The panel draws what changed with one line of context either side, on
//! `BLOCK` in the panel's diff face: a sign column, the add and delete
//! washes, no line numbers and no syntax colour.

use super::overview::{OverviewEvent, OverviewView};
use super::*;
use ferrite_core::docview::FileChange;
use ferrite_pane::hunks::{hunks_element, shown, Layout, Shown};
use ferrite_reviewr::ferrite::FileSides;
use std::collections::{HashMap, HashSet};

/// A file in the panel: its Thread and the path its facts carry.
type FileKey = (ThreadKey, String);

/// A file's `+N −N`, as the facts count it.
type Stat = (usize, usize);

/// A file's hunks as last read, and the diffstat they were read at.
struct Read {
    stat: Stat,
    shown: Option<Shown>,
}

/// Which files are unfolded, and what each has read.
#[derive(Default)]
pub(crate) struct Hunks {
    unfolded: HashSet<FileKey>,
    reads: HashMap<FileKey, Read>,
}

impl Hunks {
    /// Fold or unfold a file; true when it is now unfolded.
    fn toggle(&mut self, key: FileKey) -> bool {
        if self.unfolded.remove(&key) {
            return false;
        }
        self.unfolded.insert(key);
        true
    }

    /// Unfold every one of `keys`, or fold them all.
    fn set_all(&mut self, keys: impl IntoIterator<Item = FileKey>, unfolded: bool) {
        for key in keys {
            if unfolded {
                self.unfolded.insert(key);
            } else {
                self.unfolded.remove(&key);
            }
        }
    }

    fn unfolded(&self, key: &FileKey) -> bool {
        self.unfolded.contains(key)
    }

    /// Whether an unfolded file wants a read: none asked yet, or its last
    /// was asked at another diffstat.
    fn wants_read(&self, key: &FileKey, stat: Stat) -> bool {
        self.unfolded(key) && self.reads.get(key).is_none_or(|read| read.stat != stat)
    }

    /// A read asked at `stat`: what it lands is kept, what an older one
    /// lands is dropped. Whatever was shown stays until then.
    fn asking(&mut self, key: FileKey, stat: Stat) {
        let shown = self.reads.remove(&key).and_then(|read| read.shown);
        self.reads.insert(key, Read { stat, shown });
    }

    /// A read asked at `stat` landed; false when a newer one is out.
    fn landed(&mut self, key: &FileKey, stat: Stat, shown: Shown) -> bool {
        match self.reads.get_mut(key) {
            Some(read) if read.stat == stat => {
                read.shown = Some(shown);
                true
            }
            _ => false,
        }
    }

    fn shown(&self, key: &FileKey) -> Option<&Shown> {
        self.reads.get(key)?.shown.as_ref()
    }

    /// Every Thread `keep` refuses takes its folds and reads with it.
    pub(super) fn retain_threads(&mut self, keep: impl Fn(ThreadKey) -> bool) {
        self.unfolded.retain(|(thread, _)| keep(*thread));
        self.reads.retain(|(thread, _), _| keep(*thread));
    }

    #[cfg(test)]
    pub(super) fn held_for_test(&self, thread: ThreadKey) -> usize {
        self.unfolded.iter().chain(self.reads.keys()).filter(|(of, _)| *of == thread).count()
    }
}

impl OverviewView {
    /// Fold or unfold `path`'s hunks under its block. The read, if one is
    /// due, is asked by the next frame that draws the block.
    pub(in crate::cockpit) fn toggle_file(&mut self, thread: ThreadKey, path: String, cx: &mut Context<Self>) {
        self.hunks.toggle((thread, path));
        cx.notify();
    }

    /// Whether every one of `paths` is unfolded (the section line's fold).
    pub(super) fn files_unfolded<'a>(&self, thread: ThreadKey, mut paths: impl Iterator<Item = &'a str>) -> bool {
        paths.all(|path| self.hunks.unfolded(&(thread, path.to_string())))
    }

    /// The section line's fold: unfold every one of `paths` unless all
    /// are, then fold them all.
    pub(super) fn toggle_files(&mut self, thread: ThreadKey, paths: Vec<String>, cx: &mut Context<Self>) {
        let unfold = !self.files_unfolded(thread, paths.iter().map(String::as_str));
        self.hunks
            .set_all(paths.into_iter().map(|path| (thread, path)), unfold);
        cx.notify();
    }

    /// Whether `path`'s block is unfolded; when it is and its hunks are
    /// stale (or unread), ask the window for them.
    pub(super) fn file_unfolded(&self, thread: ThreadKey, file: &FileChange, cx: &mut Context<Self>) -> bool {
        let key = (thread, file.path.clone());
        let stat = (file.added, file.removed);
        if self.hunks.wants_read(&key, stat) {
            cx.emit(OverviewEvent::ReadHunks {
                thread,
                path: file.path.clone(),
                stat,
            });
        }
        self.hunks.unfolded(&key)
    }

    /// Whether `path`'s block wants its hunks read at `stat`.
    pub(in crate::cockpit) fn wants_read(&self, thread: ThreadKey, path: &str, stat: Stat) -> bool {
        self.hunks.wants_read(&(thread, path.to_string()), stat)
    }

    /// Wait on the window's `answer`, the checkout's uncommitted diff, and
    /// build `path`'s hunks off the UI thread.
    pub(in crate::cockpit) fn wait_hunks(
        &mut self,
        thread: ThreadKey,
        path: String,
        stat: Stat,
        workspace: &std::path::Path,
        answer: std::io::Result<std::sync::mpsc::Receiver<std::io::Result<Vec<FileSides>>>>,
        cx: &mut Context<Self>,
    ) {
        let relative = ferrite_pane::file_links::inside_checkout(std::path::Path::new(&path), workspace)
            .unwrap_or(std::path::Path::new(&path))
            .to_string_lossy()
            .into_owned();
        let key = (thread, path);
        self.hunks.asking(key.clone(), stat);
        cx.spawn(async move |view, cx| {
            let read = ferrite_ui::answer::answered(answer, cx).await;
            let shown = cx
                .background_executor()
                .spawn(async move {
                    match read {
                        Ok(files) => shown(files.iter().find(|sides| sides.file.path == relative)),
                        Err(error) => Shown::Note(error.to_string().into()),
                    }
                })
                .await;
            view.update(cx, |view, cx| {
                if view.hunks.landed(&key, stat, shown) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    #[cfg(test)]
    pub(in crate::cockpit) fn hunks_for_test(&self, thread: ThreadKey, path: &str) -> Option<&Shown> {
        self.hunks.shown(&(thread, path.to_string()))
    }

    /// The unfolded block's hunk area, once its read has landed; nothing
    /// before, so a quick read never flashes an empty band. It lands in
    /// the frame it arrives and goes in the frame the block folds, as every
    /// fold does (motion-measured.md, "Tool group expand / collapse":
    /// snap everything).
    pub(super) fn hunks_element(&self, thread: ThreadKey, index: usize, path: &str, unfolded: bool) -> Option<Div> {
        unfolded
            .then(|| self.hunks.shown(&(thread, path.to_string())))
            .flatten()
            .map(|shown| hunks_element(index, shown, Layout::Abbreviated))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A read is asked once per diffstat: a moved diffstat asks again, a
    /// folded file asks nothing, and an older read landing late is dropped.
    #[test]
    fn a_file_reads_again_when_its_diffstat_moves() {
        let mut hunks = Hunks::default();
        let key = (ThreadKey::fixture(0, 1), "src/nav.rs".to_string());
        assert!(!hunks.wants_read(&key, (1, 1)), "folded: nothing");
        assert!(hunks.toggle(key.clone()));
        assert!(hunks.wants_read(&key, (1, 1)));
        hunks.asking(key.clone(), (1, 1));
        assert!(!hunks.wants_read(&key, (1, 1)), "asked once");
        assert!(hunks.wants_read(&key, (2, 1)), "the agent edited it again");
        hunks.asking(key.clone(), (2, 1));
        let note = |text: &str| Shown::Note(SharedString::from(text.to_string()));
        assert!(!hunks.landed(&key, (1, 1), note("old")), "stale: dropped");
        assert!(hunks.landed(&key, (2, 1), note("new")));
        assert_eq!(hunks.shown(&key), Some(&note("new")));
        hunks.asking(key.clone(), (3, 1));
        assert_eq!(hunks.shown(&key), Some(&note("new")), "held while it reads");
        assert!(!hunks.toggle(key.clone()));
        assert!(!hunks.wants_read(&key, (4, 1)), "folded again");
    }
}
