//! What the grid offers to open under a ⌘-press: the address a program
//! tied to a cell (OSC 8), else the web address or source location written
//! there. The grid reads the words; the cockpit decides where they go.

use alacritty_terminal::grid::Dimensions as _;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::Term;

/// The address under `at`, ready for the cockpit to resolve.
pub(crate) fn address_at<L: alacritty_terminal::event::EventListener>(
    term: &Term<L>,
    at: Point,
) -> Option<String> {
    if let Some(link) = term.grid()[at].hyperlink() {
        return Some(link.uri().to_owned());
    }
    let (cells, index) = logical_line(term, at);
    address_in(&cells, index)
}

/// The rows a long line wrapped over, one char per column, and where `at`
/// falls in them. The second half of a wide char is `\0`.
fn logical_line<L: alacritty_terminal::event::EventListener>(
    term: &Term<L>,
    at: Point,
) -> (Vec<char>, usize) {
    let grid = term.grid();
    let columns = grid.columns();
    let wraps = |line: Line| {
        grid[line][Column(columns - 1)]
            .flags
            .contains(Flags::WRAPLINE)
    };
    let mut first = at.line;
    while first > term.topmost_line() && wraps(first - 1) {
        first -= 1;
    }
    let mut cells = Vec::new();
    let mut line = first;
    loop {
        for column in 0..columns {
            let cell = &grid[line][Column(column)];
            cells.push(if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                '\0'
            } else {
                cell.c
            });
        }
        if line >= term.bottommost_line() || !wraps(line) {
            break;
        }
        line += 1;
    }
    let index = (at.line.0 - first.0) as usize * columns + at.column.0;
    (cells, index)
}

/// The address written around `cells[index]`: a web address, a local
/// server's `host:port`, or a path (`src/main.rs:12`, `./run.sh`).
fn address_in(cells: &[char], index: usize) -> Option<String> {
    let is_edge = |c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | '│');
    if is_edge(*cells.get(index)?) {
        return None;
    }
    let start = cells[..index]
        .iter()
        .rposition(|c| is_edge(*c))
        .map_or(0, |at| at + 1);
    let end = cells[index..]
        .iter()
        .position(|c| is_edge(*c))
        .map_or(cells.len(), |at| index + at);
    let word: String = cells[start..end].iter().filter(|c| **c != '\0').collect();
    classify(trim(&word))
}

/// The word without the brackets and punctuation sentences put around it.
fn trim(word: &str) -> &str {
    let mut word = word
        .trim_start_matches(['(', '[', '{'])
        .trim_end_matches(['.', ',', ';', ':', '!', '?']);
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        while word.ends_with(close) && word.matches(close).count() > word.matches(open).count() {
            word = word[..word.len() - 1].trim_end_matches(['.', ',', ';', ':', '!', '?']);
        }
    }
    word
}

fn classify(word: &str) -> Option<String> {
    let lower = word.to_ascii_lowercase();
    for scheme in ["https://", "http://", "file://"] {
        if lower.starts_with(scheme) && word.len() > scheme.len() {
            return Some(word.to_owned());
        }
    }
    if ["localhost:", "127.0.0.1:", "0.0.0.0:"].iter().any(|host| {
        lower
            .strip_prefix(host)
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
    }) {
        return Some(format!("http://{word}"));
    }
    is_path(word).then(|| word.to_owned())
}

/// A source location (`main.rs:12`) or a path with a folder in it that
/// names a file (`src/main.rs`) or begins at a root (`/tmp`, `./run`,
/// `~/notes`). A bare word (`and/or`, `e.g`) is prose.
fn is_path(word: &str) -> bool {
    let mut base = word;
    let mut line = false;
    for _ in 0..2 {
        match base.rsplit_once(':') {
            Some((rest, number))
                if !number.is_empty() && number.bytes().all(|c| c.is_ascii_digit()) =>
            {
                base = rest;
                line = true;
            }
            _ => break,
        }
    }
    let name = base.rsplit('/').next().unwrap_or(base);
    let extension = name.contains('.') && !name.ends_with('.');
    let rooted = ["/", "./", "../", "~/"]
        .iter()
        .any(|root| base.starts_with(root));
    if base.contains('/') {
        rooted || line || extension
    } else {
        line && extension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(line: &str, needle: &str) -> Option<String> {
        let cells: Vec<char> = line.chars().collect();
        let index = line.find(needle).unwrap() + needle.len() / 2;
        address_in(&cells, index)
    }

    #[test]
    fn web_addresses_are_found_without_the_prose_around_them() {
        assert_eq!(
            at("  Local: http://localhost:5173/  ", "5173").as_deref(),
            Some("http://localhost:5173/")
        );
        assert_eq!(
            at("see https://example.com/a?b=c#d.", "example").as_deref(),
            Some("https://example.com/a?b=c#d")
        );
        assert_eq!(
            at("(https://example.com/wiki/A_(b))", "wiki").as_deref(),
            Some("https://example.com/wiki/A_(b)")
        );
        assert_eq!(
            at("serving on localhost:3000, ready", "3000").as_deref(),
            Some("http://localhost:3000")
        );
    }

    #[test]
    fn source_locations_and_paths_are_found() {
        assert_eq!(
            at("  --> src/main.rs:12:5", "main").as_deref(),
            Some("src/main.rs:12:5")
        );
        assert_eq!(
            at("src/main.rs:12: error", "main").as_deref(),
            Some("src/main.rs:12")
        );
        assert_eq!(at("main.rs:7", "main").as_deref(), Some("main.rs:7"));
        assert_eq!(
            at("run ./scripts/dev.sh now", "dev").as_deref(),
            Some("./scripts/dev.sh")
        );
        assert_eq!(
            at("edit crates/a/src/lib.rs please", "lib").as_deref(),
            Some("crates/a/src/lib.rs")
        );
    }

    #[test]
    fn prose_is_not_an_address() {
        for (line, word) in [
            ("use and/or here", "and/or"),
            ("that is, i.e. this", "i.e"),
            ("at 10:30:15 today", "10:30:15"),
            ("version 1.2.3 out", "1.2.3"),
            ("key:12 next", "key:12"),
            ("go to https:// now", "https://"),
        ] {
            assert_eq!(at(line, word), None, "{line}");
        }
    }

    #[test]
    fn a_blank_cell_names_nothing() {
        let cells: Vec<char> = "src/main.rs:12   x".chars().collect();
        assert_eq!(address_in(&cells, 14), None);
        assert_eq!(address_in(&cells, 99), None);
    }
}
