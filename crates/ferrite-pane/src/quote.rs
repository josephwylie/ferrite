//! Quotes a prompt carries: transcript passages the operator replied to
//! (Claude's selection Reply). The draft holds them as Quote rows over its
//! line; the prompt carries them as ordinary text, the same for every
//! provider: each passage a Markdown block quote, ahead of the words, so
//! the model reads what the reply refers to. The sent prompt's bubble reads
//! them back off its head (`split`) and draws the same rows, and a prompt
//! recalled into the Composer gets its rows back.

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

/// `passage` as a block quote: each line behind `> `, a blank one a bare
/// `>`, as Claude's own Copy as quote writes one.
fn block(passage: &str) -> String {
    passage
        .lines()
        .map(|line| {
            if line.is_empty() {
                ">".to_string()
            } else {
                format!("> {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The prompt text: every quote's block, a blank line apart, then `text`.
pub fn compose(quotes: &[String], text: &str) -> String {
    let mut parts: Vec<String> = quotes.iter().map(|quote| block(quote)).collect();
    if !text.is_empty() || parts.is_empty() {
        parts.push(text.to_string());
    }
    parts.join("\n\n")
}

/// `compose` undone: the block quotes leading `prompt`, each a paragraph
/// whose every line is quoted, and the text after them.
pub fn split(prompt: &str) -> (Vec<String>, &str) {
    let mut quotes = Vec::new();
    let mut rest = prompt;
    loop {
        let (paragraph, after) = rest.split_once("\n\n").unwrap_or((rest, ""));
        let lines: Option<Vec<&str>> = paragraph
            .split('\n')
            .map(|line| {
                line.strip_prefix("> ")
                    .or_else(|| (line == ">").then_some(""))
            })
            .collect();
        match lines {
            Some(lines) if !paragraph.is_empty() => {
                quotes.push(lines.join("\n"));
                rest = after;
            }
            _ => return (quotes, rest),
        }
    }
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

    #[test]
    fn quotes_go_ahead_of_the_words_as_block_quotes() {
        let quotes = vec!["The fix is in.\n\nRun the tests.".to_string(), "two".into()];
        assert_eq!(
            compose(&quotes, "Why?"),
            "> The fix is in.\n>\n> Run the tests.\n\n> two\n\nWhy?"
        );
        assert_eq!(compose(&quotes[1..], ""), "> two");
        assert_eq!(compose(&[], "plain"), "plain");
    }

    #[test]
    fn a_prompt_splits_back_into_its_quotes_and_words() {
        let quotes = vec!["a\n\nb".to_string(), "c".into()];
        let prompt = compose(&quotes, "words\n\n> not a lead quote");
        assert_eq!(split(&prompt), (quotes, "words\n\n> not a lead quote"));
        assert_eq!(split("> only"), (vec!["only".to_string()], ""));
        assert_eq!(split("no quote"), (vec![], "no quote"));
        // A paragraph with an unquoted line is words, not a quote.
        assert_eq!(split("> half\nwords"), (vec![], "> half\nwords"));
    }
}
