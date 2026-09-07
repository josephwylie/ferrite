//! The highlighter Ferrite ships with: a small hand-rolled lexer.
//!
//! Deliberately not a grammar engine. A Pane shows short fenced blocks at
//! terminal density, where strings, comments, numbers and a language's
//! keywords carry nearly all the legibility a full parse would buy — at a
//! fraction of the cost, with no dependency, and with no chance of a slow
//! parse stalling a frame.

use std::sync::mpsc::{self, Receiver, Sender};

use super::{Class, HighlightRequest, Highlighter, Input, Token};

/// Rust's keywords, plus the few contextual ones a Pane reads as keywords.
const RUST: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];

/// Answers highlight requests immediately, onto a channel the caller drains
/// back into `Transcript::apply` — the same path a slow highlighter on its own
/// thread would use, so the shipped one cannot be the odd case out.
pub struct Lexer {
    answers: Sender<Input>,
}

impl Lexer {
    pub fn new() -> (Self, Receiver<Input>) {
        let (answers, requests) = mpsc::channel();
        (Self { answers }, requests)
    }
}

impl Highlighter for Lexer {
    fn request(&self, request: HighlightRequest) {
        let _ = self.answers.send(Input::Highlighted {
            block: request.block,
            tokens: tokens(request.language.as_deref(), &request.source),
        });
    }
}

/// Classify `source`. Every byte lands in exactly one token, in order, so the
/// tokens concatenate back to the source a Pane already has.
pub fn tokens(language: Option<&str>, source: &str) -> Vec<Token> {
    if !matches!(language, Some("rust" | "rs" | "python" | "py")) {
        return (!source.is_empty())
            .then(|| Token {
                text: source.into(),
                class: Class::Plain,
            })
            .into_iter()
            .collect();
    }

    let keywords: &[&str] = match language {
        Some("rust" | "rs") => RUST,
        _ => &[],
    };

    let mut tokens: Vec<Token> = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut at = 0;
    let mut plain = String::new();

    while at < chars.len() {
        let (class, len) = scan(&chars, at, keywords);
        if class == Class::Plain {
            plain.extend(&chars[at..at + len]);
            at += len;
            continue;
        }
        if !plain.is_empty() {
            tokens.push(Token {
                text: std::mem::take(&mut plain),
                class: Class::Plain,
            });
        }
        tokens.push(Token {
            text: chars[at..at + len].iter().collect(),
            class,
        });
        at += len;
    }
    if !plain.is_empty() {
        tokens.push(Token {
            text: plain,
            class: Class::Plain,
        });
    }
    tokens
}

/// The run starting at `at`: what it is, and how many chars it spans.
fn scan(chars: &[char], at: usize, keywords: &[&str]) -> (Class, usize) {
    let rest = &chars[at..];
    match rest {
        ['/', '/', ..] => (Class::Comment, line(rest)),
        ['#', ..] if keywords.is_empty() => (Class::Comment, line(rest)),
        ['/', '*', ..] => (Class::Comment, block_comment(rest)),
        ['"', ..] | ['\'', ..] => (Class::Str, string(rest)),
        [c, ..] if c.is_ascii_digit() => (
            Class::Number,
            run(rest, |c| c.is_ascii_alphanumeric() || c == '.' || c == '_'),
        ),
        [c, ..] if c.is_alphabetic() || *c == '_' => {
            let len = run(rest, |c| c.is_alphanumeric() || c == '_');
            let word: String = rest[..len].iter().collect();
            let class = if keywords.contains(&word.as_str()) {
                Class::Keyword
            } else {
                Class::Plain
            };
            (class, len)
        }
        _ => (Class::Plain, 1),
    }
}

fn line(rest: &[char]) -> usize {
    rest.iter().position(|c| *c == '\n').unwrap_or(rest.len())
}

fn block_comment(rest: &[char]) -> usize {
    let mut at = 2;
    while at + 1 < rest.len() {
        if rest[at] == '*' && rest[at + 1] == '/' {
            return at + 2;
        }
        at += 1;
    }
    rest.len()
}

/// A quoted run, ending at the matching quote. An unterminated string runs to
/// the end of the block rather than swallowing the next one.
fn string(rest: &[char]) -> usize {
    let quote = rest[0];
    let mut at = 1;
    while at < rest.len() {
        match rest[at] {
            '\\' => at += 2,
            c if c == quote => return at + 1,
            '\n' => return at,
            _ => at += 1,
        }
    }
    rest.len()
}

fn run(rest: &[char], keep: impl Fn(char) -> bool) -> usize {
    rest.iter().take_while(|c| keep(**c)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classed(source: &str) -> Vec<(Class, String)> {
        tokens(Some("rust"), source)
            .into_iter()
            .map(|token| (token.class, token.text))
            .collect()
    }

    #[test]
    fn keywords_strings_numbers_and_comments_are_told_apart() {
        assert_eq!(
            classed("let x = \"hi\"; // note\n"),
            [
                (Class::Keyword, "let".into()),
                (Class::Plain, " x = ".into()),
                (Class::Str, "\"hi\"".into()),
                (Class::Plain, "; ".into()),
                (Class::Comment, "// note".into()),
                (Class::Plain, "\n".into()),
            ]
        );
        assert_eq!(
            classed("y = 42"),
            [(Class::Plain, "y = ".into()), (Class::Number, "42".into()),]
        );
    }

    #[test]
    fn plain_and_unknown_fences_preserve_literal_text_without_syntax_claims() {
        let source = "print  \"hi\" 3\t# literal  \n\n界";
        for language in [
            None,
            Some("text"),
            Some("plaintext"),
            Some("not-a-language"),
        ] {
            let result = tokens(language, source);
            assert_eq!(
                result
                    .iter()
                    .map(|token| token.text.as_str())
                    .collect::<String>(),
                source
            );
            assert!(
                result.iter().all(|token| token.class == Class::Plain),
                "{language:?}: {result:?}"
            );
        }
        assert!(tokens(Some("rust"), "let x = 3;")
            .iter()
            .any(|token| token.class == Class::Keyword));
    }

    #[test]
    fn python_fences_style_strings_comments_and_numbers_without_losing_spacing() {
        let source = "print(\"hi\",  3)  # note\n";
        for language in ["python", "py"] {
            let result = tokens(Some(language), source);
            assert_eq!(
                result
                    .iter()
                    .map(|token| token.text.as_str())
                    .collect::<String>(),
                source
            );
            assert!(result
                .iter()
                .any(|token| token.class == Class::Str && token.text == "\"hi\""));
            assert!(result
                .iter()
                .any(|token| token.class == Class::Number && token.text == "3"));
            assert!(result
                .iter()
                .any(|token| token.class == Class::Comment && token.text == "# note"));
        }
    }

    /// The Pane maps tokens onto the source by length; a lexer that dropped or
    /// invented a byte would silently mis-colour every block after it.
    #[test]
    fn every_byte_of_the_source_comes_back_exactly_once() {
        for source in [
            "fn main() { let x = 1; }",
            "/* block */ let s = \"a\\\"b\";\n# not rust\n",
            "unterminated \"string",
            "",
            "🌒 let e = '𝄞';",
        ] {
            let covered: String = tokens(Some("rust"), source)
                .iter()
                .map(|token| token.text.as_str())
                .collect();
            assert_eq!(covered, source);
        }
    }
}
