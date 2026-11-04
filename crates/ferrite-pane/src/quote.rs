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

/// A passage the operator replied to, and what they said about it (empty
/// for a bare quote).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reply {
    pub quote: String,
    pub text: String,
}

impl Reply {
    pub fn new(quote: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            quote: quote.into(),
            text: text.into(),
        }
    }
}

/// What leads a reply's words in the prompt text, so they read as the
/// reply's own.
const WORDS_MARK: &str = "\u{21a9} ";

/// The prompt text: each reply's quote block and its marked words, a blank
/// line apart, then `text`. A reply's words are one line (the field they
/// are typed in is), so a line break in them is a space.
pub fn compose(replies: &[Reply], text: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for reply in replies {
        parts.push(block(&reply.quote));
        if !reply.text.is_empty() {
            parts.push(format!("{WORDS_MARK}{}", reply.text.replace('\n', " ")));
        }
    }
    if !text.is_empty() || parts.is_empty() {
        parts.push(text.to_string());
    }
    parts.join("\n\n")
}

/// `compose` undone: the replies leading `prompt` — each a paragraph whose
/// every line is quoted, and the paragraph of marked words that may follow
/// it — and the text after them.
pub fn split(prompt: &str) -> (Vec<Reply>, &str) {
    let mut replies: Vec<Reply> = Vec::new();
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
        let words = paragraph
            .strip_prefix(WORDS_MARK)
            .filter(|_| replies.last().is_some_and(|reply| reply.text.is_empty()));
        match (lines, words) {
            (Some(lines), _) if !paragraph.is_empty() => {
                replies.push(Reply::new(lines.join("\n"), ""));
            }
            (_, Some(words)) => {
                if let Some(reply) = replies.last_mut() {
                    reply.text = words.to_string();
                }
            }
            _ => return (replies, rest),
        }
        rest = after;
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
    fn replies_go_ahead_of_the_words_as_a_block_quote_and_their_marked_words() {
        let replies = vec![
            Reply::new("The fix is in.\n\nRun the tests.", "Which tests?"),
            Reply::new("two", ""),
        ];
        assert_eq!(
            compose(&replies, "Why?"),
            "> The fix is in.\n>\n> Run the tests.\n\n\u{21a9} Which tests?\n\n> two\n\nWhy?"
        );
        assert_eq!(compose(&replies[1..], ""), "> two");
        assert_eq!(compose(&[], "plain"), "plain");
    }

    #[test]
    fn a_prompt_splits_back_into_its_replies_and_words() {
        let replies = vec![
            Reply::new("a\n\nb", ""),
            Reply::new("c", "why c?"),
        ];
        let prompt = compose(&replies, "words\n\n> not a lead quote");
        assert_eq!(split(&prompt), (replies, "words\n\n> not a lead quote"));
        assert_eq!(split("> only"), (vec![Reply::new("only", "")], ""));
        assert_eq!(split("no quote"), (vec![], "no quote"));
        // A paragraph with an unquoted line is words, not a quote.
        assert_eq!(split("> half\nwords"), (vec![], "> half\nwords"));
        // Marked words with no reply before them are the prompt's.
        assert_eq!(split("\u{21a9} alone"), (vec![], "\u{21a9} alone"));
    }

    #[test]
    fn three_replies_round_trip_with_the_main_text() {
        let replies = vec![
            Reply::new("card_link freezes the Business Name", "Yeah should be adaptable?"),
            Reply::new("The eWay V6023 error\nmessage was added", ""),
            Reply::new("third > passage", "Yeah thats poor, fix that"),
        ];
        let text = "This is a stale spec\n\nfix the rest please";
        let prompt = compose(&replies, text);
        assert_eq!(split(&prompt), (replies, text));
    }
}
