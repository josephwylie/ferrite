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
use ferrite_reviewr::diff::Row;
use ferrite_reviewr::ferrite::FileSides;
use std::collections::{HashMap, HashSet};

/// Context lines kept beside each change. Hunks closer than this merge.
const CONTEXT: usize = 1;

/// The most lines an abbreviated file's hunks draw; past them, `… +N lines`
/// (the whole diff is the Changes Surface's).
const MAX_LINES: usize = 120;

/// How a file's hunks are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layout {
    /// The Overview's block: the first `MAX_LINES` lines, each cut at the edge.
    Abbreviated,
    /// The Changes Surface: every line. A long line wraps, or, unwrapped,
    /// scrolls sideways.
    Whole { wrap: bool },
}

/// A file in the panel: its Thread and the path its facts carry.
type FileKey = (ThreadId, String);

/// A file's `+N −N`, as the facts count it.
type Stat = (usize, usize);

/// What a file's block shows under its head once its read lands.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Shown {
    /// Its hunks, in file order, each a run of lines.
    Hunks(Vec<Vec<Line>>),
    /// Why there are none: not in the uncommitted diff, binary, too large,
    /// or the read failed.
    Note(SharedString),
}

/// One drawn line of a hunk.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Line {
    pub sign: Sign,
    pub text: SharedString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Sign {
    Context,
    Added,
    Removed,
}

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
    pub(super) fn retain_threads(&mut self, keep: impl Fn(ThreadId) -> bool) {
        self.unfolded.retain(|(thread, _)| keep(*thread));
        self.reads.retain(|(thread, _), _| keep(*thread));
    }

    #[cfg(test)]
    pub(super) fn held_for_test(&self, thread: ThreadId) -> usize {
        self.unfolded.iter().chain(self.reads.keys()).filter(|(of, _)| *of == thread).count()
    }
}

impl OverviewView {
    /// Fold or unfold `path`'s hunks under its block. The read, if one is
    /// due, is asked by the next frame that draws the block.
    pub(in crate::cockpit) fn toggle_file(&mut self, thread: ThreadId, path: String, cx: &mut Context<Self>) {
        self.hunks.toggle((thread, path));
        cx.notify();
    }

    /// Whether every one of `paths` is unfolded (the section line's fold).
    pub(super) fn files_unfolded<'a>(&self, thread: ThreadId, mut paths: impl Iterator<Item = &'a str>) -> bool {
        paths.all(|path| self.hunks.unfolded(&(thread, path.to_string())))
    }

    /// The section line's fold: unfold every one of `paths` unless all
    /// are, then fold them all.
    pub(super) fn toggle_files(&mut self, thread: ThreadId, paths: Vec<String>, cx: &mut Context<Self>) {
        let unfold = !self.files_unfolded(thread, paths.iter().map(String::as_str));
        self.hunks
            .set_all(paths.into_iter().map(|path| (thread, path)), unfold);
        cx.notify();
    }

    /// Whether `path`'s block is unfolded; when it is and its hunks are
    /// stale (or unread), ask the window for them.
    pub(super) fn file_unfolded(&self, thread: ThreadId, file: &FileChange, cx: &mut Context<Self>) -> bool {
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
    pub(in crate::cockpit) fn wants_read(&self, thread: ThreadId, path: &str, stat: Stat) -> bool {
        self.hunks.wants_read(&(thread, path.to_string()), stat)
    }

    /// Wait on the window's `answer`, the checkout's uncommitted diff, and
    /// build `path`'s hunks off the UI thread.
    pub(in crate::cockpit) fn wait_hunks(
        &mut self,
        thread: ThreadId,
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
            let read = beside::answered(answer, cx).await;
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
    pub(in crate::cockpit) fn hunks_for_test(&self, thread: ThreadId, path: &str) -> Option<&Shown> {
        self.hunks.shown(&(thread, path.to_string()))
    }

    /// The unfolded block's hunk area, once its read has landed; nothing
    /// before, so a quick read never flashes an empty band. It lands in
    /// the frame it arrives and goes in the frame the block folds, as every
    /// fold does (motion-measured.md, "Tool group expand / collapse":
    /// snap everything).
    pub(super) fn hunks_element(&self, thread: ThreadId, index: usize, path: &str, unfolded: bool) -> Option<Div> {
        unfolded
            .then(|| self.hunks.shown(&(thread, path.to_string())))
            .flatten()
            .map(|shown| hunks_element(index, shown, Layout::Abbreviated))
    }
}

/// A file's sides as the panel shows them: its hunks, or why there are none.
pub(crate) fn shown(sides: Option<&FileSides>) -> Shown {
    let Some(sides) = sides else {
        return Shown::Note("No uncommitted changes".into());
    };
    let diff = sides.rows();
    if let Some(notice) = diff.notice {
        use ferrite_reviewr::diff::Notice;
        return Shown::Note(
            match notice {
                Notice::Binary => "Binary file",
                Notice::TooLarge => "File too large to show",
                Notice::Unreadable => "Couldn't read this file",
            }
            .into(),
        );
    }
    let hunks = hunks(&diff.rows);
    if hunks.is_empty() {
        return Shown::Note("No uncommitted changes".into());
    }
    Shown::Hunks(hunks)
}

/// The Review core's diff rows cut to the panel's hunks: each changed line
/// with `CONTEXT` lines beside it. A single unchanged line between two
/// hunks is drawn rather than cut, as the Review's own folds do.
fn hunks(rows: &[Row]) -> Vec<Vec<Line>> {
    let mut lines = Vec::new();
    flatten(rows, &mut lines);
    let n = lines.len();
    let mut keep = vec![false; n];
    for (at, line) in lines.iter().enumerate() {
        if line.sign != Sign::Context {
            let hi = (at + CONTEXT).min(n - 1);
            keep[at.saturating_sub(CONTEXT)..=hi].fill(true);
        }
    }
    for at in 1..n.saturating_sub(1) {
        if !keep[at] && keep[at - 1] && keep[at + 1] {
            keep[at] = true;
        }
    }
    let mut hunks: Vec<Vec<Line>> = Vec::new();
    let mut open = false;
    for (line, kept) in lines.into_iter().zip(keep) {
        match (kept, open) {
            (true, true) => hunks.last_mut().expect("an open hunk").push(line),
            (true, false) => hunks.push(vec![line]),
            (false, _) => {}
        }
        open = kept;
    }
    hunks
}

/// Every content row, folds opened, as a line.
fn flatten(rows: &[Row], out: &mut Vec<Line>) {
    let text = |spans: &[ferrite_reviewr::diff::Span]| -> SharedString {
        spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>()
            .replace('\t', "    ")
            .into()
    };
    for row in rows {
        let (sign, spans) = match row {
            Row::Context { spans, .. } => (Sign::Context, spans),
            Row::Insertion { spans, .. } => (Sign::Added, spans),
            Row::Deletion { spans, .. } => (Sign::Removed, spans),
            Row::Fold { lines } => {
                flatten(lines, out);
                continue;
            }
            Row::Rendered { .. } => continue,
        };
        out.push(Line {
            sign,
            text: text(spans),
        });
    }
}

/// The hunk area (`BLOCK`, the panel's diff face): each line on a 2.5ch sign
/// column inset 1ch, changed lines on their wash in body ink, context in
/// `INK_SUBTLE`; a hairline between hunks.
pub(crate) fn hunks_element(index: usize, shown: &Shown, layout: Layout) -> Div {
    use ferrite_ui::theme::*;
    let cell = FS_PANEL_DIFF * CODE_ADVANCE;
    let wrap = layout == Layout::Whole { wrap: true };
    let scrolls = layout == Layout::Whole { wrap: false };
    // A wrapped line grows by rows; an unwrapped one is cut at the edge, or
    // keeps its width for the area to scroll.
    let row = || {
        div()
            .flex()
            .flex_shrink_0()
            .when(wrap, |row| row.min_h(px(LH_PANEL_DIFF)))
            .when(!wrap, |row| row.h(px(LH_PANEL_DIFF)))
            .pl(px(PANEL_HUNK_INSET_CELLS * cell))
            .when(!scrolls, |row| row.overflow_hidden())
    };
    let area = div()
        .debug_selector(move || format!("panel-hunks-{index}"))
        .flex()
        .flex_col()
        .pt(px(PANEL_HUNK_PAD_T))
        .pb(px(PANEL_HUNK_PAD_B))
        .bg(paint::BLOCK)
        .font_family(FONT_CODE)
        .text_size(px(FS_PANEL_DIFF))
        .line_height(px(LH_PANEL_DIFF))
        .when(!wrap, |area| area.whitespace_nowrap());
    let hunks = match shown {
        Shown::Hunks(hunks) => hunks,
        Shown::Note(note) => {
            return area.child(row().text_color(rgb(INK_SUBTLE)).child(note.clone()));
        }
    };
    let total: usize = hunks.iter().map(Vec::len).sum();
    let mut left = if layout == Layout::Abbreviated { MAX_LINES } else { usize::MAX };
    let mut drawn = 0;
    let mut area = area;
    for (at, hunk) in hunks.iter().enumerate() {
        if left == 0 {
            break;
        }
        if at > 0 {
            area = area.child(
                div()
                    .flex_shrink_0()
                    .h(px(1.))
                    .my(px(PANEL_HUNK_PAD_T))
                    .bg(paint::LINE),
            );
        }
        for line in hunk.iter().take(left) {
            let (glyph, sign_ink, ink, wash) = match line.sign {
                Sign::Context => ("", INK_SUBTLE, INK_SUBTLE, None),
                Sign::Added => ("+", DIFF_ADDED_SIGN, SYN_PLAIN, Some(DIFF_ADDED_WASH)),
                Sign::Removed => (
                    "\u{2212}",
                    DIFF_REMOVED_SIGN,
                    SYN_PLAIN,
                    Some(DIFF_REMOVED_WASH),
                ),
            };
            let n = drawn;
            area = area.child(
                row()
                    .debug_selector(move || format!("panel-hunk-{index}-{n}"))
                    .when_some(wash, |row, wash| row.bg(rgba(wash)))
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(PANEL_HUNK_SIGN_CELLS * cell))
                            .text_color(rgb(sign_ink))
                            .child(glyph),
                    )
                    .child(
                        div()
                            .when(scrolls, |text| text.flex_none())
                            .when(!scrolls, |text| text.flex_1().min_w_0().overflow_hidden())
                            .text_color(rgb(ink))
                            .child(line.text.clone()),
                    ),
            );
            drawn += 1;
        }
        left = left.saturating_sub(hunk.len());
    }
    if total > drawn {
        area = area.child(
            row()
                .pl(px((PANEL_HUNK_INSET_CELLS + PANEL_HUNK_SIGN_CELLS) * cell))
                .text_color(rgb(INK_SUBTLE))
                .child(SharedString::from(format!(
                    "\u{2026} +{} lines",
                    total - drawn
                ))),
        );
    }
    if scrolls {
        // The area is as wide as its longest line (at least the column);
        // the scroller carries it sideways.
        return div().flex().flex_col().child(
            div()
                .id(SharedString::from(format!("panel-hunks-scroll-{index}")))
                .debug_selector(move || format!("panel-hunks-scroll-{index}"))
                .flex()
                .overflow_x_scroll()
                .child(area.flex_none().min_w_full()),
        );
    }
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(text: &str) -> Row {
        Row::Context {
            old_no: 0,
            new_no: 0,
            spans: vec![span(text)],
        }
    }

    fn added(text: &str) -> Row {
        Row::Insertion {
            new_no: 0,
            spans: vec![span(text)],
            emphasis: vec![],
            cr: false,
        }
    }

    fn removed(text: &str) -> Row {
        Row::Deletion {
            old_no: 0,
            spans: vec![span(text)],
            emphasis: vec![],
            cr: false,
        }
    }

    fn span(text: &str) -> ferrite_reviewr::diff::Span {
        ferrite_reviewr::diff::Span {
            text: text.into(),
            color: Default::default(),
        }
    }

    fn signs(hunks: &[Vec<Line>]) -> Vec<String> {
        hunks
            .iter()
            .map(|hunk| {
                hunk.iter()
                    .map(|line| match line.sign {
                        Sign::Context => ' ',
                        Sign::Added => '+',
                        Sign::Removed => '-',
                    })
                    .collect()
            })
            .collect()
    }

    /// One line of context either side of a change, the Review's folds
    /// opened; a lone unchanged line between two changes is drawn, two
    /// split the hunks.
    #[test]
    fn hunks_keep_one_line_of_context() {
        let rows = vec![
            context("a"),
            Row::Fold {
                lines: vec![context("b"), context("c")],
            },
            context("d"),
            removed("e"),
            added("E"),
            context("f"),
            context("g"),
            added("h"),
            context("i"),
            context("j"),
            context("k"),
            context("l"),
            added("m"),
        ];
        let hunks = hunks(&rows);
        assert_eq!(signs(&hunks), [" -+  + ", " +"]);
        assert_eq!(hunks[0][0].text, "d");
        assert_eq!(hunks[1][0].text, "l");
    }

    /// A read is asked once per diffstat: a moved diffstat asks again, a
    /// folded file asks nothing, and an older read landing late is dropped.
    #[test]
    fn a_file_reads_again_when_its_diffstat_moves() {
        let mut hunks = Hunks::default();
        let key = (ThreadId::new(1), "src/nav.rs".to_string());
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
