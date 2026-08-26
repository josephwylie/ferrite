//! The Composer menus' fuzzy filter (#23): an ASCII case-insensitive
//! subsequence match, scored so contiguous runs and early hits float up,
//! answering the byte ranges the rows highlight in ACCENT. Pure — no
//! window, no provider.

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

    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn multibyte_candidates_neither_panic_nor_misalign() {
        let (_, ranges) = matches("éb", "aébc").unwrap();
        assert_eq!(ranges, [1..4], "é is two bytes and b follows it");
    }
}
