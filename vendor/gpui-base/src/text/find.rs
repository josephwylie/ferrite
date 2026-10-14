//! Ferrite: a find bar's marks over a view's text.
//!
//! Every occurrence of the needle wears `wash` under its glyphs, the active
//! one `active_wash`, and the active one reports where it painted (window
//! coordinates) so its host can scroll it into view. Occurrences are counted
//! per source run (a paragraph, a code block, a table cell) in document
//! order — the order [`find_matches`] meets them in the view's plain text —
//! so a host that counts over that text names the active one by its ordinal.
//! The text it counts over is `TextView::markdown_find_text`: every run on a
//! line of its own, so a one-line needle never spans two runs there either.

use std::{
    cell::Cell,
    collections::HashMap,
    ops::Range,
    rc::Rc,
    sync::{Arc, Mutex},
};

use gpui::{
    BorderStyle, Bounds, Edges, Hsla, IntoElement, Pixels, SharedString, Styled, Window, canvas,
    px, quad, transparent_black,
};

use super::{inline::InlineState, state::TextViewState};
use crate::GlobalState;

/// What a view marks, and which occurrence (its ordinal in the view) is the
/// active one.
#[derive(Clone)]
pub struct TextFind {
    pub needle: SharedString,
    pub case_sensitive: bool,
    pub active: Option<usize>,
    pub wash: Hsla,
    pub active_wash: Hsla,
    pub report: Rc<FindReport>,
}

/// What a find's paint tells its host.
#[derive(Default)]
pub struct FindReport {
    /// Where the active occurrence painted last, its wrapped lines' union.
    pub revealed: Cell<Option<Bounds<Pixels>>>,
    /// The first occurrence painted at or below the top of its view, by
    /// ordinal, since the host last cleared it: where a find that starts in
    /// view starts. Meaningful for a report one view fills.
    pub ahead: Cell<Option<usize>>,
}

/// Mark the occurrences of `view`'s find in one painted part of a source run
/// (`root`, its text `source`): the part covers `own` of the run and paints
/// in `bounds`, and `part` gives where a range of the part (relative to
/// `own`) painted. Every occurrence is found in the whole run, so one split
/// by a wrap or an inline card is still found whole, and the run counts even
/// when clipped: the ordinals must not depend on what is on screen.
pub(super) fn paint_marks(
    view: &TextViewState,
    root: usize,
    source: &str,
    own: Range<usize>,
    bounds: Bounds<Pixels>,
    part: impl Fn(Range<usize>) -> Vec<Bounds<Pixels>>,
    window: &mut Window,
) {
    let Some(find) = &view.find else {
        return;
    };
    let found = find_matches(source, &find.needle, find.case_sensitive);
    let base = view.find_tally.borrow_mut().base(root, found.len());
    let mask = window.content_mask().bounds;
    let clip = bounds.intersect(&mask);
    let clipped = clip.size.width <= px(0.) || clip.size.height <= px(0.);
    for (ordinal, range) in found.into_iter().enumerate() {
        let start = range.start.max(own.start);
        let end = range.end.min(own.end);
        if start >= end {
            continue;
        }
        let ordinal = base + ordinal;
        let active = find.active == Some(ordinal);
        // The active occurrence reports its place even off screen.
        let lines = if clipped && !active {
            Vec::new()
        } else {
            part(start - own.start..end - own.start)
        };
        let placed = lines
            .iter()
            .copied()
            .reduce(|union, line| union.union(&line));
        if placed.unwrap_or(bounds).bottom() > mask.top()
            && find.report.ahead.get().is_none_or(|ahead| ordinal < ahead)
        {
            find.report.ahead.set(Some(ordinal));
        }
        for line in &lines {
            window.paint_quad(quad(
                *line,
                px(0.),
                if active { find.active_wash } else { find.wash },
                Edges::default(),
                transparent_black(),
                BorderStyle::default(),
            ));
        }
        if active {
            find.report.revealed.set(placed);
        }
    }
}

/// Count the occurrences in a source run (`root`) its view draws as
/// something else (`TextView::code_block_view`: a fence as its diagram), at
/// its place in paint order — laid over the drawing, painted before it — so
/// the runs after it keep the ordinals their host counts in the view's find
/// text. Nothing is washed; the active one reports the drawing's bounds. A
/// drawing that is the run's own text after all paints its own marks, on
/// the ordinals counted here.
pub(super) fn count_drawn(root: Arc<Mutex<InlineState>>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let Some(view) = GlobalState::global(cx).text_view_state() else {
                return;
            };
            let view = view.read(cx);
            let Some(find) = &view.find else {
                return;
            };
            let Some(source) = root.try_lock().ok().map(|root| root.text.clone()) else {
                return;
            };
            let count = find_matches(&source, &find.needle, find.case_sensitive).len();
            let base = view
                .find_tally
                .borrow_mut()
                .base(Arc::as_ptr(&root) as usize, count);
            if count == 0 {
                return;
            }
            if bounds.bottom() > window.content_mask().bounds.top()
                && find.report.ahead.get().is_none_or(|ahead| base < ahead)
            {
                find.report.ahead.set(Some(base));
            }
            if find
                .active
                .is_some_and(|active| (base..base + count).contains(&active))
            {
                find.report.revealed.set(Some(bounds));
            }
        },
    )
    .absolute()
    .size_full()
}

/// Every non-overlapping occurrence of `needle` in `haystack`, as byte
/// ranges of `haystack`. Without `case_sensitive`, characters compare by
/// their first lowercase form.
pub fn find_matches(haystack: &str, needle: &str, case_sensitive: bool) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }
    if case_sensitive {
        return haystack
            .match_indices(needle)
            .map(|(at, found)| at..at + found.len())
            .collect();
    }
    let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
    let needle: Vec<char> = needle.chars().map(fold).collect();
    let mut found = Vec::new();
    let mut from = 0;
    while from < haystack.len() {
        let mut end = from;
        let mut chars = haystack[from..].chars();
        let hit = needle.iter().all(|want| {
            chars.next().is_some_and(|c| {
                end += c.len_utf8();
                fold(c) == *want
            })
        });
        if hit {
            found.push(from..end);
            from = end;
        } else {
            from += haystack[from..].chars().next().map_or(1, char::len_utf8);
        }
    }
    found
}

/// The ordinal of each source run's first occurrence, assigned as the runs
/// first paint in a frame: a run split across wrapped lines or around inline
/// cards keeps one count.
#[derive(Default)]
pub(super) struct FindTally {
    bases: HashMap<usize, usize>,
    next: usize,
}

impl FindTally {
    pub(super) fn clear(&mut self) {
        self.bases.clear();
        self.next = 0;
    }

    /// The ordinal of `root`'s first occurrence; `count` is how many it has.
    pub(super) fn base(&mut self, root: usize, count: usize) -> usize {
        *self.bases.entry(root).or_insert_with(|| {
            let base = self.next;
            self.next += count;
            base
        })
    }
}
