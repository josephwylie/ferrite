//! The Composer menus' fuzzy filter (#23): an ASCII case-insensitive
//! subsequence match, scored so contiguous runs and early hits float up,
//! answering the byte ranges the rows highlight in ACCENT. Pure — no
//! window, no provider.
//!
//! The palette's match (`palette_match`, the prototype's `#palette`) is
//! stricter and never ranks: a row stays when the query is a contiguous
//! run of its name and context, or a subsequence that starts on a word.

use std::ops::Range;

/// Where `needle` matches inside `candidate`, or None where it does not.
/// The score orders candidates (higher first); the ranges are the matched
/// bytes, merged where consecutive, ready for `StyledText` highlights.
///
/// An empty needle matches everything with nothing highlighted — the menu
/// just opened and lists as-is.
pub fn matches(needle: &str, candidate: &str) -> Option<(i64, Vec<Range<usize>>)> {
    if needle.is_empty() {
        return Some((0, Vec::new()));
    }
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut score = 0i64;
    let mut wanted = needle.chars().map(|c| c.to_ascii_lowercase()).peekable();
    let mut previous_hit: Option<usize> = None;
    for (at, ch) in candidate.char_indices() {
        let Some(target) = wanted.peek() else {
            break;
        };
        if ch.to_ascii_lowercase() != *target {
            continue;
        }
        wanted.next();
        // Contiguity is worth more than anything; a match at the very start
        // outranks one buried mid-word; every gap costs a little.
        match previous_hit {
            Some(previous) if previous == at - ch_before(candidate, at) => score += 8,
            Some(previous) => score -= ((at - previous) / 4).min(4) as i64,
            None if at == 0 => score += 12,
            None => score -= (at / 4).min(6) as i64,
        }
        previous_hit = Some(at);
        let end = at + ch.len_utf8();
        match ranges.last_mut() {
            Some(last) if last.end == at => last.end = end,
            _ => ranges.push(at..end),
        }
    }
    if wanted.peek().is_some() {
        return None;
    }
    Some((score, ranges))
}

/// The palette's filter (FL-3): does `query` match a row named `name` with
/// the muted `context` after it, and which bytes of the name light up?
///
/// The haystack is `name + ' ' + context`. A row matches when the query is
/// a case-insensitive contiguous run of it, or a subsequence of it whose
/// first character lands on a word start. Only the name is highlighted:
/// its leftmost contiguous run when it holds one, else the greedy
/// subsequence from the leftmost word-start anchor, clipped to the name.
/// Rows are filtered, never re-ranked, so the caller keeps its order. An
/// empty query matches every row with nothing lit.
pub fn palette_match(query: &str, name: &str, context: &str) -> Option<Vec<Range<usize>>> {
    if query.trim().is_empty() {
        return Some(Vec::new());
    }
    let wanted: Vec<char> = query.chars().collect();
    let haystack = if context.is_empty() {
        name.to_string()
    } else {
        format!("{name} {context}")
    };
    let chars: Vec<(usize, char)> = haystack.char_indices().collect();
    let same = |a: char, b: char| a.eq_ignore_ascii_case(&b);
    let run_at = |start: usize| {
        start + wanted.len() <= chars.len()
            && wanted
                .iter()
                .enumerate()
                .all(|(offset, want)| same(chars[start + offset].1, *want))
    };
    let in_name = |start: usize| {
        let (at, ch) = chars[start + wanted.len() - 1];
        at + ch.len_utf8() <= name.len()
    };
    let word_start = |index: usize| index == 0 || !chars[index - 1].1.is_alphanumeric();
    // The greedy subsequence from a word-start anchor: the anchor itself,
    // then each next wanted character as early as it comes.
    let subsequence = |anchor: usize| -> Option<Vec<usize>> {
        let mut hits = vec![anchor];
        let mut from = anchor + 1;
        for want in &wanted[1..] {
            let found = (from..chars.len()).find(|index| same(chars[*index].1, *want))?;
            hits.push(found);
            from = found + 1;
        }
        Some(hits)
    };
    let anchored = (0..chars.len())
        .filter(|index| word_start(*index) && same(chars[*index].1, wanted[0]))
        .find_map(subsequence);
    let run = (0..chars.len()).find(|start| run_at(*start));
    let hits: Vec<usize> = match (run, anchored) {
        (None, None) => return None,
        // The leftmost run lies in the name: it is the light.
        (Some(start), _) if in_name(start) => (start..start + wanted.len()).collect(),
        (_, Some(hits)) => hits,
        // A run only in the context: the row stays, the name stays dark.
        (Some(_), None) => Vec::new(),
    };
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for index in hits {
        let (at, ch) = chars[index];
        let end = at + ch.len_utf8();
        if end > name.len() {
            continue;
        }
        match ranges.last_mut() {
            Some(last) if last.end == at => last.end = end,
            _ => ranges.push(at..end),
        }
    }
    Some(ranges)
}

/// The byte length of the character just before `at` — what makes two hits
/// "consecutive" in a multi-byte string.
fn ch_before(text: &str, at: usize) -> usize {
    text[..at]
        .chars()
        .next_back()
        .map(char::len_utf8)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subsequence_matches_and_a_non_subsequence_does_not() {
        assert!(matches("crv", "code-review").is_some());
        assert!(matches("xyz", "code-review").is_none());
        assert!(matches("reviewx", "code-review").is_none());
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(matches("CR", "code-review").is_some());
        assert!(matches("cr", "Code-Review").is_some());
    }

    #[test]
    fn an_empty_needle_matches_everything_with_no_highlight() {
        assert_eq!(matches("", "anything"), Some((0, Vec::new())));
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init)] // assertions compare literal ranges
    fn the_ranges_cover_exactly_the_matched_bytes_merged_where_adjacent() {
        let (_, ranges) = matches("co", "code-review").unwrap();
        assert_eq!(ranges, [0..2], "a contiguous prefix is one range");

        let (_, ranges) = matches("cr", "code-review").unwrap();
        assert_eq!(ranges, [0..1, 5..6], "c of code, r of review");
    }

    /// The ordering the menus lean on: a prefix beats a scattered match, and
    /// a contiguous run beats the same letters spread out.
    #[test]
    fn contiguous_and_early_matches_outscore_scattered_ones() {
        let score = |needle: &str, candidate: &str| matches(needle, candidate).unwrap().0;
        assert!(score("com", "commit") > score("com", "code-empty-mix"));
        assert!(score("rev", "review") > score("rev", "prune-everything"));
    }

    /// FL-3's worked example: `par` keeps three Threads and three commands
    /// and lights exactly the prototype's letters; `open a group` has no
    /// word starting with `p`, so it goes.
    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn the_palette_keeps_runs_and_word_anchored_subsequences() {
        let lit = |name: &str, context: &str| palette_match("par", name, context);
        assert_eq!(
            lit("Perf: layout cache", "perf sweep \u{b7} working"),
            Some(vec![0..1, 7..8]),
            "P, a"
        );
        assert_eq!(
            lit("Parked \u{b7} Release 0.5.0 notes", "ferrite \u{b7} 2h"),
            Some(vec![0..3]),
            "Par"
        );
        assert_eq!(
            lit("Port onboarding flow", "zeron \u{b7} working"),
            Some(vec![0..1, 9..11]),
            "P, ar"
        );
        assert_eq!(lit("park thread", ""), Some(vec![0..3]));
        assert_eq!(lit("show parked", ""), Some(vec![5..8]));
        assert_eq!(
            lit("compare with main", "open the diff reader"),
            Some(vec![3..6])
        );
        for gone in [
            ("open a group", ""),
            ("Close stale issues", "perf sweep \u{b7} needs you"),
            ("Theme retune", "perf sweep \u{b7} done"),
            ("Fold regression", "perf sweep \u{b7} failing"),
            ("Flaky provider test", "ferrite \u{b7} needs you"),
            ("show plan", ""),
            ("fullscreen pane", ""),
            ("filter: all projects", ""),
        ] {
            assert_eq!(lit(gone.0, gone.1), None, "{gone:?}");
        }
        assert_eq!(palette_match("", "anything", ""), Some(Vec::new()));
        assert_eq!(palette_match("SHOW", "show parked", ""), Some(vec![0..4]));
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn multibyte_candidates_neither_panic_nor_misalign() {
        let (_, ranges) = matches("éb", "aébc").unwrap();
        assert_eq!(ranges, [1..4], "é is two bytes and b follows it");
    }
}
