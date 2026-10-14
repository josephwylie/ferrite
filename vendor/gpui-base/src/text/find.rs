//! Ferrite: a find bar's marks over a view's text.
//!
//! Every occurrence of the needle wears `wash` under its glyphs, the active
//! one `active_wash`, and the active one reports where it painted (window
//! coordinates) so its host can scroll it into view. Occurrences are counted
//! per source run (a paragraph, a code block, a table cell) in document
//! order — the order [`find_matches`] meets them in the view's plain text —
//! so a host that counts over that text names the active one by its ordinal.

use std::{cell::Cell, collections::HashMap, ops::Range, rc::Rc};

use gpui::{Bounds, Hsla, Pixels, SharedString};

/// What a view marks, and which occurrence (its ordinal in the view) is the
/// active one.
#[derive(Clone)]
pub struct TextFind {
    pub needle: SharedString,
    pub case_sensitive: bool,
    pub active: Option<usize>,
    pub wash: Hsla,
    pub active_wash: Hsla,
    /// Where the active occurrence painted last, its wrapped lines' union.
    pub revealed: Rc<Cell<Option<Bounds<Pixels>>>>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_are_byte_ranges_that_never_overlap() {
        assert_eq!(find_matches("aaaa", "aa", true), vec![0..2, 2..4]);
        assert_eq!(find_matches("abc", "", true), Vec::<Range<usize>>::new());
        assert_eq!(find_matches("Find find FIND", "find", true), vec![5..9]);
    }

    #[test]
    fn caseless_matches_index_the_original_text() {
        assert_eq!(
            find_matches("Find find FIND", "find", false),
            vec![0..4, 5..9, 10..14]
        );
        // `É` and `é` are two bytes each; the ranges stay on the original.
        assert_eq!(find_matches("café CAFÉ", "café", false), vec![0..5, 6..11]);
        assert_eq!(find_matches("ab", "abc", false), Vec::<Range<usize>>::new());
    }

    #[test]
    fn a_run_keeps_its_first_ordinal_however_often_it_paints() {
        let mut tally = FindTally::default();
        assert_eq!(tally.base(7, 2), 0);
        assert_eq!(tally.base(9, 3), 2);
        assert_eq!(tally.base(7, 2), 0);
        assert_eq!(tally.base(11, 1), 5);
        tally.clear();
        assert_eq!(tally.base(9, 3), 0);
    }
}
