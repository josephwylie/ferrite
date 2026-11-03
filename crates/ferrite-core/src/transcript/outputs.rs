//! What a turn left for the operator to open, as Claude's transcript shows
//! it under the answer: a card for each page the turn wrote or named — an
//! `.html` file, a local address — and for each other file it wrote.
//!
//! Provider-neutral. A file is written by a write call ([`ToolKind::Write`])
//! or created by any provider's patch (`@@ -0,0 +1,n @@`, the shape both
//! bridges give a new file: [`Diff::created`]); a call that failed wrote
//! nothing. A page is also one the answer names: a local address
//! (`http://localhost:5173/`) or an `.html` file. Each is named once, in
//! the order the turn first named it, as the turn named it (relative to its
//! workspace, or absolute): the reader resolves it.

use super::tools::{policy, ToolKind};
use super::{Block, Body, Diff, ToolBlock, ToolState};

/// One thing a turn left to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    /// A page to open beside the transcript.
    Page(Page),
    /// A file the turn wrote that is no page.
    File(String),
}

/// A page, by where it lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Page {
    /// An `.html` file.
    File(String),
    /// A local address: a server on this machine.
    Address(String),
}

impl Diff {
    /// Whether this diff made its file: one hunk from nothing.
    pub fn created(&self) -> bool {
        matches!(self.hunks.as_slice(), [hunk] if hunk.old_start == 0 && hunk.old_lines == 0)
    }
}

/// What `turn` — its blocks, from its prompt to its end — left to open.
pub fn of_turn(turn: &[Block]) -> Vec<Output> {
    let mut outputs: Vec<Output> = Vec::new();
    let mut add = |output: Output| {
        if !outputs.contains(&output) {
            outputs.push(output);
        }
    };
    for block in turn {
        match &block.body {
            Body::Tool(tool) => {
                for path in written(tool) {
                    add(if is_html(path) {
                        Output::Page(Page::File(path.to_owned()))
                    } else {
                        Output::File(path.to_owned())
                    });
                }
            }
            _ => {
                for page in block.markdown.as_deref().into_iter().flat_map(named_pages) {
                    add(Output::Page(page));
                }
            }
        }
    }
    outputs
}

/// The files a call wrote: a write call's, or the ones its patch created.
fn written(tool: &ToolBlock) -> impl Iterator<Item = &str> {
    let ok = tool.state == ToolState::Ok;
    let writes = policy(&tool.name).kind == ToolKind::Write;
    tool.diffs
        .iter()
        .filter(move |diff| ok && (writes || diff.created()))
        .map(|diff| diff.path.as_str())
}

/// The pages prose names: local addresses and `.html` files, read off its
/// words (a Markdown link's target, a code span, a bare path).
fn named_pages(prose: &str) -> impl Iterator<Item = Page> + '_ {
    prose
        .split(|c: char| c.is_whitespace() || "()[]<>\"'`".contains(c))
        .map(|word| word.trim_end_matches(['.', ',', ';', ':', '!', '?']))
        .filter_map(|word| {
            if is_local_address(word) {
                Some(Page::Address(word.to_owned()))
            } else if is_html(word) && !word.contains("://") {
                Some(Page::File(word.to_owned()))
            } else {
                None
            }
        })
}

fn is_html(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".html") || lower.ends_with(".htm")
}

/// An `http(s)` address on this machine.
fn is_local_address(word: &str) -> bool {
    let Some(rest) = word
        .strip_prefix("http://")
        .or_else(|| word.strip_prefix("https://"))
    else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host
        .rsplit_once(':')
        .filter(|(_, port)| port.bytes().all(|b| b.is_ascii_digit()))
        .map_or(host, |(host, _)| host);
    matches!(host, "localhost" | "127.0.0.1" | "0.0.0.0" | "[::1]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_event::Hunk;
    use crate::transcript::BlockId;

    fn block(id: u64, body: Body, markdown: Option<&str>) -> Block {
        Block {
            id: BlockId(id),
            body,
            markdown: markdown.map(str::to_owned),
            markdown_run: None,
            sent_at: None,
            prompt_id: None,
        }
    }

    fn hunk(old_start: u32, old_lines: u32) -> Hunk {
        Hunk {
            old_start,
            old_lines,
            new_start: 1,
            new_lines: 1,
            lines: vec!["+x".into()],
            section: None,
        }
    }

    fn call(id: u64, name: &str, state: ToolState, diffs: &[(&str, Hunk)]) -> Block {
        block(
            id,
            Body::Tool(ToolBlock {
                call: format!("call-{id}"),
                name: name.into(),
                title: None,
                summary: String::new(),
                state,
                diffs: diffs
                    .iter()
                    .map(|(path, hunk)| Diff::new(path.to_string(), vec![hunk.clone()]))
                    .collect(),
                structured_preview: None,
                result_line: None,
                output: None,
                progress: None,
                caption: None,
            }),
            None,
        )
    }

    fn prose(id: u64, text: &str) -> Block {
        block(id, Body::Paragraph { spans: Vec::new() }, Some(text))
    }

    /// A write call's files, and any provider's created ones, are the
    /// turn's; an edit to a file already there and a failed write are not.
    #[test]
    fn a_turn_names_the_files_it_wrote() {
        let turn = [
            call(1, "Write", ToolState::Ok, &[("/w/notes.md", hunk(3, 2))]),
            call(
                2,
                "fileChange",
                ToolState::Ok,
                &[("src/new.rs", hunk(0, 0))],
            ),
            call(3, "Edit", ToolState::Ok, &[("src/old.rs", hunk(4, 1))]),
            call(
                4,
                "Write",
                ToolState::Failed("denied".into()),
                &[("/w/no.md", hunk(0, 0))],
            ),
            call(5, "Write", ToolState::Ok, &[("/w/notes.md", hunk(0, 0))]),
        ];
        assert_eq!(
            of_turn(&turn),
            [
                Output::File("/w/notes.md".into()),
                Output::File("src/new.rs".into()),
            ]
        );
    }

    /// An `.html` file written or named is a page, and so is a local
    /// address the answer gives; a remote address is not.
    #[test]
    fn a_turn_names_its_pages() {
        let turn = [
            call(
                1,
                "Write",
                ToolState::Ok,
                &[("site/index.html", hunk(0, 0))],
            ),
            prose(
                2,
                "Open [the chart](out/chart.HTML), or run it at \
                 http://localhost:5173/app. Docs: https://example.com/a.html, \
                 and `http://127.0.0.1:8000`; site/index.html again.",
            ),
        ];
        assert_eq!(
            of_turn(&turn),
            [
                Output::Page(Page::File("site/index.html".into())),
                Output::Page(Page::File("out/chart.HTML".into())),
                Output::Page(Page::Address("http://localhost:5173/app".into())),
                Output::Page(Page::Address("http://127.0.0.1:8000".into())),
            ]
        );
    }

    #[test]
    fn only_this_machine_is_local() {
        for local in [
            "http://localhost",
            "http://localhost:3000/x?y#z",
            "https://127.0.0.1:8443",
            "http://0.0.0.0:8080/",
            "http://[::1]:5000",
        ] {
            assert!(is_local_address(local), "{local}");
        }
        for remote in [
            "http://localhost.example.com",
            "https://example.com",
            "localhost:3000",
        ] {
            assert!(!is_local_address(remote), "{remote}");
        }
    }
}
