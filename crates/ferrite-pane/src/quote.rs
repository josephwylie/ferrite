//! Replies a prompt carries: transcript passages the operator replied to
//! (Claude's selection Reply), each with the words they typed about it. The
//! draft holds them as Reply cards in its tray; the prompt carries them as
//! ordinary text, the same for every provider: each passage a Markdown
//! block quote, then the reply's words behind `WORDS_MARK` on a line of
//! their own, ahead of the prompt's own words, so the model reads what the
//! reply refers to. The mark is what keeps any number of replies apart: a
//! reply's words are never taken for the next quote or the prompt's text.
//! The sent prompt's bubble reads the replies back off its head (`split`)
//! and draws each as a Quote row with its words, and a prompt recalled into
//! the Composer gets its replies back as Reply cards.

/// Claude's bounds on a quotable passage, in characters, once trimmed: a
/// selection shorter offers no Reply, one longer is not quoted.
const MIN_CHARS: usize = 2;
const MAX_CHARS: usize = 10_000;

/// The passage a selection quotes: line ends normalized and the ends
/// trimmed, or `None` when Claude would offer no Reply for it.
pub fn passage(selected: &str) -> Option<String> {
    let text = selected.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim();
    let chars = text.chars().count();
    (MIN_CHARS..=MAX_CHARS)
        .contains(&chars)
        .then(|| text.to_string())
}

/// Whether a selection is too long to quote (T3's `Shorten selection`).
pub fn too_long(selected: &str) -> bool {
    selected.trim().chars().count() > MAX_CHARS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_passage_is_trimmed_with_its_line_ends_normalized() {
        assert_eq!(
            passage("  first\r\nsecond\rthird \n").as_deref(),
            Some("first\nsecond\nthird")
        );
    }

    #[test]
    fn a_passage_holds_two_to_ten_thousand_characters() {
        assert_eq!(passage(" a "), None);
        assert_eq!(passage("   "), None);
        assert_eq!(passage("界e").as_deref(), Some("界e"));
        assert!(passage(&"x".repeat(MAX_CHARS)).is_some());
        assert_eq!(passage(&"x".repeat(MAX_CHARS + 1)), None);
    }
}
