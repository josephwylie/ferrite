//! The text form of a prompt's replies, shared by the Composer that writes it
//! and the provider adapters that must not act on it: a quoted passage is
//! reference material, so no `@path` or `/skill` inside one is input.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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

/// `prompt` cut into its leading replies, the text after them, and the file
/// trailer behind that, which hides the last paragraph's quote when the
/// operator typed no words.
struct Parts<'a> {
    replies: Vec<Reply>,
    head: &'a str,
    rest: &'a str,
    trailer: &'a str,
}

fn parts(prompt: &str) -> Parts<'_> {
    let body_len = crate::prompt_files::split(prompt.to_owned()).0.len();
    let (body, trailer) = prompt.split_at(body_len);
    let (replies, rest) = split(body);
    let head = &body[..body.len() - rest.len()];
    Parts {
        replies,
        head,
        rest,
        trailer,
    }
}

/// What the operator wrote in `prompt`: everything but the replies' quoted
/// passages. Adapters scan this, never the passages, for `@path` and `/skill`
/// input.
pub fn authored(prompt: &str) -> Cow<'_, str> {
    let parts = parts(prompt);
    if parts.replies.is_empty() {
        return Cow::Borrowed(prompt);
    }
    let mut text = String::new();
    for words in parts
        .replies
        .iter()
        .map(|reply| &reply.text)
        .filter(|words| !words.is_empty())
    {
        text.push_str(words);
        text.push('\n');
    }
    text.push_str(parts.rest);
    text.push_str(parts.trailer);
    Cow::Owned(text)
}

/// Joins an `@` to what follows it without a space, so a CLI that expands
/// `@path` itself reads a quoted one as plain words.
const MENTION_BREAK: char = '\u{2060}';

/// `prompt` as a CLI that expands `@path` in the text it is sent should
/// receive it: the quoted passages carry no live `@`. The stored prompt keeps
/// its words as typed.
pub fn inert(prompt: &str) -> Cow<'_, str> {
    let parts = parts(prompt);
    if parts.replies.is_empty() || !parts.head.contains('@') {
        return Cow::Borrowed(prompt);
    }
    let mut text = parts
        .head
        .split('\n')
        .map(|line| {
            if line.starts_with('>') {
                line.replace('@', &format!("@{MENTION_BREAK}"))
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    text.push_str(parts.rest);
    text.push_str(parts.trailer);
    Cow::Owned(text)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let replies = vec![Reply::new("a\n\nb", ""), Reply::new("c", "why c?")];
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
    fn only_the_quoted_passages_are_not_authored() {
        let replies = vec![
            Reply::new("see @a.png and /skill", "and @b.png"),
            Reply::new("@c.png", ""),
        ];
        let prompt = compose(&replies, "read @d.png\n@\"/e.png\"");
        assert_eq!(authored(&prompt), "and @b.png\nread @d.png\n@\"/e.png\"");
        assert_eq!(authored("plain @x"), "plain @x");
    }

    #[test]
    fn a_file_trailer_does_not_hide_the_quotes_behind_it() {
        let trailer = crate::prompt_files::compose(
            &compose(&[Reply::new("see @pic.png", "")], ""),
            &[std::path::PathBuf::from("/abs/f")],
        );
        assert_eq!(trailer, "> see @pic.png\n@\"/abs/f\"");
        assert_eq!(authored(&trailer), "\n@\"/abs/f\"");
        assert_eq!(inert(&trailer), "> see @\u{2060}pic.png\n@\"/abs/f\"");
    }

    #[test]
    fn inert_breaks_only_the_quoted_mentions() {
        let prompt = compose(&[Reply::new("a @x\n\nb @y", "see @z")], "and @w");
        assert_eq!(
            inert(&prompt),
            "> a @\u{2060}x\n>\n> b @\u{2060}y\n\n\u{21a9} see @z\n\nand @w"
        );
        assert_eq!(inert("plain @x"), "plain @x");
    }

    #[test]
    fn three_replies_round_trip_with_the_main_text() {
        let replies = vec![
            Reply::new(
                "card_link freezes the Business Name",
                "Yeah should be adaptable?",
            ),
            Reply::new("The eWay V6023 error\nmessage was added", ""),
            Reply::new("third > passage", "Yeah thats poor, fix that"),
        ];
        let text = "This is a stale spec\n\nfix the rest please";
        let prompt = compose(&replies, text);
        assert_eq!(split(&prompt), (replies, text));
    }
}
