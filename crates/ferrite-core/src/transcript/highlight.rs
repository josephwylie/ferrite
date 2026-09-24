//! The highlighter Ferrite ships with: a small hand-rolled lexer.
//!
//! Deliberately not a grammar engine. A Pane shows short fenced blocks at
//! terminal density, and the reader shows whole files, where strings,
//! comments, numbers and a language's keywords carry nearly all the
//! legibility a full parse would buy — at a fraction of the cost, with no
//! dependency, and with no chance of a slow parse stalling a frame.

use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};

use super::{Class, HighlightRequest, Highlighter, Input, Token};

/// What the lexer needs to know about a language: which words are keywords,
/// how comments open, and which quotes open strings.
struct Syntax {
    keywords: &'static [&'static str],
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    quotes: &'static [char],
    /// `'` opens a string only as a char literal (`'x'`, `'\n'`); anywhere
    /// else it is a lifetime or a label, and colouring it would paint the
    /// rest of the line as a string.
    char_literals: bool,
    /// `name!` is a macro call (Rust).
    bang_calls: bool,
}

/// Rust's keywords, plus the few contextual ones a Pane reads as keywords.
const RUST: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];

/// Python's keywords, soft keywords included.
#[rustfmt::skip]
const PYTHON: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "case", "class",
    "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
    "import", "in", "is", "lambda", "match", "nonlocal", "not", "or", "pass", "raise", "return",
    "self", "try", "while", "with", "yield",
];

/// JavaScript and TypeScript share one list; the TypeScript-only words are
/// rare enough as identifiers in plain JavaScript not to mislead.
#[rustfmt::skip]
const SCRIPT: &[&str] = &[
    "abstract", "as", "async", "await", "break", "case", "catch", "class", "const", "continue",
    "debugger", "declare", "default", "delete", "do", "else", "enum", "export", "extends", "false",
    "finally", "for", "from", "function", "if", "implements", "import", "in", "instanceof",
    "interface", "keyof", "let", "namespace", "new", "null", "of", "private", "protected", "public",
    "readonly", "return", "static", "super", "switch", "this", "throw", "true", "try", "type",
    "typeof", "undefined", "var", "void", "while", "yield",
];

#[rustfmt::skip]
const GO: &[&str] = &[
    "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough",
    "false", "for", "func", "go", "goto", "if", "import", "interface", "iota", "map", "nil",
    "package", "range", "return", "select", "struct", "switch", "true", "type", "var",
];

/// C and C++ share one list, for the same reason as JavaScript's.
#[rustfmt::skip]
const C: &[&str] = &[
    "auto", "bool", "break", "case", "catch", "char", "class", "const", "constexpr", "continue",
    "default", "delete", "do", "double", "else", "enum", "extern", "false", "float", "for", "goto",
    "if", "inline", "int", "long", "namespace", "new", "noexcept", "nullptr", "NULL", "override",
    "private", "protected", "public", "register", "return", "short", "signed", "sizeof", "static",
    "struct", "switch", "template", "this", "throw", "true", "try", "typedef", "typename", "union",
    "unsigned", "using", "virtual", "void", "volatile", "while",
];

#[rustfmt::skip]
const JAVA: &[&str] = &[
    "abstract", "boolean", "break", "byte", "case", "catch", "char", "class", "continue", "default",
    "do", "double", "else", "enum", "extends", "false", "final", "finally", "float", "for", "if",
    "implements", "import", "instanceof", "int", "interface", "long", "new", "null", "package",
    "private", "protected", "public", "record", "return", "short", "static", "super", "switch",
    "synchronized", "this", "throw", "throws", "true", "try", "var", "void", "volatile", "while",
];

const SHELL: &[&str] = &[
    "break", "case", "continue", "do", "done", "elif", "else", "esac", "export", "fi", "for",
    "function", "if", "in", "local", "return", "then", "until", "while",
];

/// Data formats have no keywords, only their literal constants.
const DATA: &[&str] = &["false", "null", "true"];

const SLASHES: &[&str] = &["//"];
const HASH: &[&str] = &["#"];
const C_BLOCK: Option<(&str, &str)> = Some(("/*", "*/"));

/// The syntax for a fence label or a language name, case-insensitively.
fn syntax(language: &str) -> Option<Syntax> {
    let (keywords, line_comments, block_comment, quotes): (_, _, _, &[char]) =
        match language.to_ascii_lowercase().as_str() {
            "rust" | "rs" => {
                return Some(Syntax {
                    keywords: RUST,
                    line_comments: SLASHES,
                    block_comment: C_BLOCK,
                    quotes: &['"', '\''],
                    char_literals: true,
                    bang_calls: true,
                })
            }
            "python" | "py" => (PYTHON, HASH, None, &['"', '\''][..]),
            "javascript" | "js" | "jsx" | "mjs" | "cjs" | "typescript" | "ts" | "tsx" => {
                (SCRIPT, SLASHES, C_BLOCK, &['"', '\'', '`'][..])
            }
            "go" | "golang" => (GO, SLASHES, C_BLOCK, &['"', '\'', '`'][..]),
            "c" | "h" | "cpp" | "c++" | "cc" | "cxx" | "hpp" | "hh" => {
                (C, SLASHES, C_BLOCK, &['"', '\''][..])
            }
            "java" => (JAVA, SLASHES, C_BLOCK, &['"', '\''][..]),
            "shell" | "sh" | "bash" | "zsh" => (SHELL, HASH, None, &['"', '\''][..]),
            "toml" | "yaml" | "yml" => (DATA, HASH, None, &['"', '\''][..]),
            // JSONC's comments are harmless to plain JSON, which has none.
            "json" | "jsonc" => (DATA, SLASHES, C_BLOCK, &['"'][..]),
            _ => return None,
        };
    Some(Syntax {
        keywords,
        line_comments,
        block_comment,
        quotes,
        char_literals: false,
        bang_calls: false,
    })
}

/// The language a file's extension names, in the same vocabulary as a fence
/// label — or none, and the file is shown as plain text.
pub fn language_for_path(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "rs" => "rust",
        "py" | "pyi" => "python",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "go" => "go",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
        "java" => "java",
        "sh" | "bash" | "zsh" => "shell",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" | "jsonc" => "json",
        _ => return None,
    })
}

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
    let Some(syntax) = language.and_then(syntax) else {
        return (!source.is_empty())
            .then(|| Token {
                text: source.into(),
                class: Class::Plain,
            })
            .into_iter()
            .collect();
    };

    let mut tokens: Vec<Token> = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut at = 0;

    while at < chars.len() {
        let (class, len) = scan(&chars, at, &syntax);
        let text = chars[at..at + len].iter();
        // Plain and punctuation runs coalesce, so a line of prose-like code
        // stays one token rather than one per character.
        match tokens.last_mut() {
            Some(last) if last.class == class && matches!(class, Class::Plain | Class::Punct) => {
                last.text.extend(text)
            }
            _ => tokens.push(Token {
                text: text.collect(),
                class,
            }),
        }
        at += len;
    }
    tokens
}

/// The run starting at `at`: what it is, and how many chars it spans.
fn scan(chars: &[char], at: usize, syntax: &Syntax) -> (Class, usize) {
    let rest = &chars[at..];
    if syntax
        .line_comments
        .iter()
        .any(|prefix| starts_with(rest, prefix))
    {
        return (Class::Comment, line(rest));
    }
    if let Some((open, close)) = syntax.block_comment {
        if starts_with(rest, open) {
            return (Class::Comment, block_comment(rest, open, close));
        }
    }
    match rest {
        ['\'', ..] if syntax.char_literals => match rest {
            ['\'', '\\', ..] => (Class::Str, string(rest)),
            ['\'', _, '\'', ..] => (Class::Str, 3),
            _ => (Class::Plain, 1),
        },
        [quote, ..] if syntax.quotes.contains(quote) => (Class::Str, string(rest)),
        [c, ..] if c.is_ascii_digit() => (
            Class::Number,
            run(rest, |c| c.is_ascii_alphanumeric() || c == '.' || c == '_'),
        ),
        [c, ..] if c.is_alphabetic() || *c == '_' => {
            let len = run(rest, |c| c.is_alphanumeric() || c == '_');
            let word: String = rest[..len].iter().collect();
            if syntax.keywords.contains(&word.as_str()) {
                return (Class::Keyword, len);
            }
            match rest.get(len) {
                Some('(') => (Class::Function, len),
                Some('!') if syntax.bang_calls => (Class::Function, len + 1),
                _ if rest[0].is_ascii_uppercase() => (Class::Type, len),
                _ => (Class::Plain, len),
            }
        }
        [c, ..] if c.is_ascii_punctuation() => (Class::Punct, 1),
        _ => (Class::Plain, 1),
    }
}

fn line(rest: &[char]) -> usize {
    rest.iter().position(|c| *c == '\n').unwrap_or(rest.len())
}

fn starts_with(rest: &[char], prefix: &str) -> bool {
    let mut rest = rest.iter();
    prefix.chars().all(|c| rest.next() == Some(&c))
}

/// A block comment, ending after `close`; an unclosed one runs to the end.
fn block_comment(rest: &[char], open: &str, close: &str) -> usize {
    let mut at = open.chars().count();
    while at < rest.len() {
        if starts_with(&rest[at..], close) {
            return at + close.chars().count();
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
                (Class::Plain, " x ".into()),
                (Class::Punct, "=".into()),
                (Class::Plain, " ".into()),
                (Class::Str, "\"hi\"".into()),
                (Class::Punct, ";".into()),
                (Class::Plain, " ".into()),
                (Class::Comment, "// note".into()),
                (Class::Plain, "\n".into()),
            ]
        );
        assert_eq!(
            classed("y = 42"),
            [
                (Class::Plain, "y ".into()),
                (Class::Punct, "=".into()),
                (Class::Plain, " ".into()),
                (Class::Number, "42".into()),
            ]
        );
    }

    #[test]
    fn calls_types_and_punctuation_get_their_own_classes() {
        assert_eq!(
            classed("Vec::new(); println!(\"x\")"),
            [
                (Class::Type, "Vec".into()),
                (Class::Punct, "::".into()),
                (Class::Function, "new".into()),
                (Class::Punct, "();".into()),
                (Class::Plain, " ".into()),
                (Class::Function, "println!".into()),
                (Class::Punct, "(".into()),
                (Class::Str, "\"x\"".into()),
                (Class::Punct, ")".into()),
            ]
        );
        let python: Vec<(Class, String)> = tokens(Some("python"), "def f(x): return None # c")
            .into_iter()
            .map(|token| (token.class, token.text))
            .collect();
        assert_eq!(python[0], (Class::Keyword, "def".into()));
        assert_eq!(python[2], (Class::Function, "f".into()));
        assert!(python.contains(&(Class::Keyword, "return".into())));
        assert!(python.contains(&(Class::Keyword, "None".into())));
        assert_eq!(python.last(), Some(&(Class::Comment, "# c".into())));
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
    /// A lifetime is not a string: colouring `'a` as one painted the rest of
    /// every generic signature string-green.
    #[test]
    fn rust_lifetimes_stay_plain_and_char_literals_are_strings() {
        let result = classed(r"fn f<'a>(x: &'a str) -> char { '\n'; 'x' }");
        let strings: Vec<&str> = result
            .iter()
            .filter(|(class, _)| *class == Class::Str)
            .map(|(_, text)| text.as_str())
            .collect();
        assert_eq!(strings, [r"'\n'", "'x'"], "{result:?}");
    }

    #[test]
    fn each_language_knows_its_own_comments() {
        for (language, comment, not_comment) in [
            ("python", "# note", "//"),
            ("typescript", "// note", "#"),
            ("go", "/* note */", "#"),
            ("cpp", "// note", "#include"),
            ("shell", "# note", "//"),
            ("toml", "# note", "//"),
            ("yaml", "# note", "//"),
        ] {
            let source = format!("{not_comment} x\n{comment}\n");
            let result = tokens(Some(language), &source);
            assert_eq!(
                result
                    .iter()
                    .map(|token| token.text.as_str())
                    .collect::<String>(),
                source
            );
            assert!(
                result
                    .iter()
                    .any(|token| token.class == Class::Comment && token.text == comment),
                "{language}: {result:?}"
            );
            assert!(
                result
                    .iter()
                    .all(|token| token.class != Class::Comment || token.text == comment),
                "{language}: {result:?}"
            );
        }
    }

    #[test]
    fn fence_labels_are_case_insensitive_and_aliases_agree() {
        for language in ["TS", "tsx", "JavaScript", "js"] {
            assert!(tokens(Some(language), "const x = 1;")
                .iter()
                .any(|token| token.class == Class::Keyword && token.text == "const"));
        }
        assert!(tokens(Some("python"), "def f(): pass")
            .iter()
            .any(|token| token.class == Class::Keyword && token.text == "def"));
    }

    #[test]
    fn a_file_extension_names_the_language_its_fence_would() {
        for (path, language) in [
            ("src/main.rs", Some("rust")),
            ("tool.PY", Some("python")),
            ("app.tsx", Some("typescript")),
            ("Cargo.toml", Some("toml")),
            ("config.yml", Some("yaml")),
            ("include/a.hpp", Some("cpp")),
            ("notes.txt", None),
            ("Makefile", None),
        ] {
            let found = language_for_path(Path::new(path));
            assert_eq!(found, language, "{path}");
            if let Some(found) = found {
                assert!(syntax(found).is_some(), "{found} must be lexable");
            }
        }
    }
}
