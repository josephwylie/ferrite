//! The Review slot's model (ADR 0021): which changes a review shows, the
//! diff lines an operator can select in it, and the comments they leave —
//! gathered on the Thread's Composer and sent ahead of its next prompt as
//! plain text addressed to the Thread.
//!
//! Headless: the git reads and the drawing live in the UI crate; this is
//! what both sides agree on, and what the tests pin.

use std::path::Path;

use crate::transcript::{Body, Transcript};
use crate::FileEdit;

/// What a review compares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Scope {
    /// The checkout against its last commit, untracked files included.
    #[default]
    Uncommitted,
    /// The branch against its base (`git diff base...HEAD`).
    Branch,
    /// What Main's edits changed since the operator's last prompt, edit by
    /// edit, from the tool results the transcript already holds.
    LastTurn,
}

impl Scope {
    /// The scopes in the order the slot's bar draws them.
    pub const ALL: [Scope; 3] = [Scope::Uncommitted, Scope::Branch, Scope::LastTurn];

    pub fn label(self) -> &'static str {
        match self {
            Scope::Uncommitted => "uncommitted",
            Scope::Branch => "branch",
            Scope::LastTurn => "last turn",
        }
    }

    /// The next scope along the bar, wrapping (⇥).
    pub fn next(self) -> Scope {
        let at = Scope::ALL
            .iter()
            .position(|scope| *scope == self)
            .unwrap_or(0);
        Scope::ALL[(at + 1) % Scope::ALL.len()]
    }
}

/// The edits Main's tools made since the last prompt, in the order they
/// landed — one entry per edit, never merged, because each edit's hunks
/// number the file as it stood at that moment. A path inside `workspace`
/// reads relative to it, as git names it in the other scopes.
pub fn last_turn(transcript: &Transcript, workspace: Option<&Path>) -> Vec<FileEdit> {
    let blocks = transcript.blocks();
    let start = blocks
        .iter()
        .rposition(|block| matches!(block.body, Body::Prompt(_)))
        .map_or(0, |at| at + 1);
    blocks[start..]
        .iter()
        .filter_map(|block| match &block.body {
            Body::Tool(tool) => Some(tool.diffs.iter()),
            _ => None,
        })
        .flatten()
        .filter(|diff| !diff.hunks.is_empty())
        .map(|diff| FileEdit {
            path: workspace
                .and_then(|root| Path::new(&diff.path).strip_prefix(root).ok())
                .map_or_else(
                    || diff.path.clone(),
                    |relative| relative.display().to_string(),
                ),
            hunks: diff.hunks.clone(),
        })
        .collect()
}

/// `3 files +40 −12`: what a set of edits adds up to.
pub fn stat(edits: &[FileEdit]) -> String {
    let (added, removed) = edits
        .iter()
        .flat_map(|edit| edit.hunks.iter())
        .flat_map(|hunk| hunk.lines.iter())
        .fold((0usize, 0usize), |(added, removed), line| {
            match line.as_bytes().first() {
                Some(b'+') => (added + 1, removed),
                Some(b'-') => (added, removed + 1),
                _ => (added, removed),
            }
        });
    let files = edits.len();
    format!(
        "{files} file{} +{added} \u{2212}{removed}",
        if files == 1 { "" } else { "s" }
    )
}

/// One selectable diff line: the edit and hunk it belongs to, its numbers
/// on each side (a removed line has no new number, an added one no old),
/// and the line with its marker as the hunk holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub edit: usize,
    pub hunk: usize,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

/// Every diff line of `edits`, numbered, in drawing order.
pub fn lines(edits: &[FileEdit]) -> Vec<Line> {
    let mut lines = Vec::new();
    for (edit, file) in edits.iter().enumerate() {
        for (at, hunk) in file.hunks.iter().enumerate() {
            let (mut old, mut new) = (hunk.old_start, hunk.new_start);
            for text in &hunk.lines {
                let (at_old, at_new) = match text.as_bytes().first() {
                    Some(b'+') => {
                        new += 1;
                        (None, Some(new - 1))
                    }
                    Some(b'-') => {
                        old += 1;
                        (Some(old - 1), None)
                    }
                    _ => {
                        old += 1;
                        new += 1;
                        (Some(old - 1), Some(new - 1))
                    }
                };
                lines.push(Line {
                    edit,
                    hunk: at,
                    old: at_old,
                    new: at_new,
                    text: text.clone(),
                });
            }
        }
    }
    lines
}

/// A run of selected lines: where it was started (`anchor`) and where it
/// ends now (`head`), both indices into `lines`. A selection never leaves
/// the file it started in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn at(line: usize) -> Self {
        Self {
            anchor: line,
            head: line,
        }
    }

    pub fn first(self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn last(self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn contains(self, line: usize) -> bool {
        (self.first()..=self.last()).contains(&line)
    }

    /// Move the head to `line`, held inside the anchor's file.
    pub fn extend_to(self, line: usize, lines: &[Line]) -> Self {
        let Some(edit) = lines.get(self.anchor).map(|line| line.edit) else {
            return self;
        };
        let line = line.min(lines.len().saturating_sub(1));
        let head = if lines[line].edit == edit {
            line
        } else if line > self.anchor {
            lines
                .iter()
                .rposition(|line| line.edit == edit)
                .unwrap_or(self.anchor)
        } else {
            lines
                .iter()
                .position(|line| line.edit == edit)
                .unwrap_or(self.anchor)
        };
        Self {
            anchor: self.anchor,
            head,
        }
    }
}

/// Which numbering a comment's lines are given in: the new file's, unless
/// every selected line was removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    New,
    Old,
}

/// One review comment: the lines it is about and what the operator said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub path: String,
    pub side: Side,
    pub start: u32,
    pub end: u32,
    /// The selected diff lines, markers kept, so the agent sees exactly
    /// what was looked at.
    pub quote: Vec<String>,
    pub text: String,
}

impl Comment {
    /// A comment on `selection` of `lines`, from `edits`. `None` for an
    /// empty selection or blank words.
    pub fn on(
        edits: &[FileEdit],
        lines: &[Line],
        selection: Selection,
        text: &str,
    ) -> Option<Self> {
        let text = text.trim();
        let picked = lines.get(selection.first()..=selection.last())?;
        let first = picked.first()?;
        if text.is_empty() {
            return None;
        }
        let news: Vec<u32> = picked.iter().filter_map(|line| line.new).collect();
        let (side, numbers) = if news.is_empty() {
            (
                Side::Old,
                picked.iter().filter_map(|line| line.old).collect(),
            )
        } else {
            (Side::New, news)
        };
        Some(Self {
            path: edits.get(first.edit)?.path.clone(),
            side,
            start: *numbers.iter().min()?,
            end: *numbers.iter().max()?,
            quote: picked.iter().map(|line| line.text.clone()).collect(),
            text: text.to_string(),
        })
    }

    /// Whether this comment hangs under `line` of a review of `path`: the
    /// line its range ends on, in its own numbering.
    pub fn ends_at(&self, path: &str, line: &Line) -> bool {
        let number = match self.side {
            Side::New => line.new,
            Side::Old if line.new.is_none() => line.old,
            Side::Old => None,
        };
        self.path == path && number == Some(self.end)
    }

    /// `src/a.rs:12-14`, or `src/a.rs:30 (removed)` for old-side lines.
    pub fn location(&self) -> String {
        let lines = if self.start == self.end {
            self.start.to_string()
        } else {
            format!("{}-{}", self.start, self.end)
        };
        match self.side {
            Side::New => format!("{}:{lines}", self.path),
            Side::Old => format!("{}:{lines} (removed)", self.path),
        }
    }
}

/// `text` with the review comments after it, as one prompt: the operator's
/// words first, then each comment's location, its quoted lines in a diff
/// fence and what was said about them. Nothing changes without comments.
pub fn attach(text: &str, comments: &[Comment]) -> String {
    if comments.is_empty() {
        return text.to_string();
    }
    let mut prompt = text.trim_end().to_string();
    if !prompt.is_empty() {
        prompt.push_str("\n\n");
    }
    prompt.push_str("Review comments on your changes:");
    for comment in comments {
        // A fence longer than any backtick run in the quote, so quoted
        // Markdown cannot close it.
        let run = comment
            .quote
            .iter()
            .flat_map(|line| line.split(|ch| ch != '`'))
            .map(str::len)
            .max()
            .unwrap_or(0);
        let fence = "`".repeat(run.max(2) + 1);
        prompt.push_str(&format!(
            "\n\n{}\n{fence}diff\n{}\n{fence}\n{}",
            comment.location(),
            comment.quote.join("\n"),
            comment.text
        ));
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Input;
    use crate::{Hunk, SessionEvent, ToolResult};

    fn hunk(old_start: u32, new_start: u32, lines: &[&str]) -> Hunk {
        Hunk {
            old_start,
            old_lines: 0,
            new_start,
            new_lines: 0,
            lines: lines.iter().map(|line| line.to_string()).collect(),
            section: None,
        }
    }

    fn edits() -> Vec<FileEdit> {
        vec![
            FileEdit {
                path: "src/a.rs".into(),
                hunks: vec![hunk(10, 10, &[" keep", "-old", "+new", "+more", " tail"])],
            },
            FileEdit {
                path: "src/b.rs".into(),
                hunks: vec![hunk(3, 3, &["-gone", "-also"])],
            },
        ]
    }

    #[test]
    fn the_scopes_cycle_in_bar_order() {
        assert_eq!(Scope::default(), Scope::Uncommitted);
        assert_eq!(Scope::Uncommitted.next(), Scope::Branch);
        assert_eq!(Scope::Branch.next(), Scope::LastTurn);
        assert_eq!(Scope::LastTurn.next(), Scope::Uncommitted);
        assert_eq!(
            Scope::ALL.map(Scope::label),
            ["uncommitted", "branch", "last turn"]
        );
    }

    /// Each line carries the numbers git would print beside it.
    #[test]
    fn lines_number_both_sides() {
        let lines = lines(&edits());
        let numbers: Vec<(usize, Option<u32>, Option<u32>)> = lines
            .iter()
            .map(|line| (line.edit, line.old, line.new))
            .collect();
        assert_eq!(
            numbers,
            [
                (0, Some(10), Some(10)),
                (0, Some(11), None),
                (0, None, Some(11)),
                (0, None, Some(12)),
                (0, Some(12), Some(13)),
                (1, Some(3), None),
                (1, Some(4), None),
            ]
        );
        assert_eq!(stat(&edits()), "2 files +2 \u{2212}3");
        assert_eq!(stat(&edits()[..1]), "1 file +2 \u{2212}1");
    }

    /// A range can grow either way but never into the next file.
    #[test]
    fn a_selection_stays_in_its_file() {
        let lines = lines(&edits());
        let picked = Selection::at(2).extend_to(6, &lines);
        assert_eq!((picked.first(), picked.last()), (2, 4));
        let back = Selection::at(5).extend_to(0, &lines);
        assert_eq!((back.first(), back.last()), (5, 5));
        assert!(Selection::at(3).extend_to(1, &lines).contains(2));
    }

    /// A comment is numbered on the new side when it can be, on the old
    /// side for removed lines only, and blank words make none.
    #[test]
    fn comments_take_the_selected_lines_numbers() {
        let edits = edits();
        let lines = lines(&edits);
        let new = Comment::on(
            &edits,
            &lines,
            Selection::at(1).extend_to(3, &lines),
            "why?",
        )
        .unwrap();
        assert_eq!((new.side, new.start, new.end), (Side::New, 11, 12));
        assert_eq!(new.location(), "src/a.rs:11-12");
        assert_eq!(new.quote, ["-old", "+new", "+more"]);
        assert!(new.ends_at("src/a.rs", &lines[3]));
        assert!(!new.ends_at("src/a.rs", &lines[2]));
        let old = Comment::on(&edits, &lines, Selection::at(6), " keep it ").unwrap();
        assert_eq!(old.location(), "src/b.rs:4 (removed)");
        assert_eq!(old.text, "keep it");
        assert!(old.ends_at("src/b.rs", &lines[6]));
        assert!(Comment::on(&edits, &lines, Selection::at(0), "  ").is_none());
    }

    /// The prompt keeps the operator's words first and quotes each
    /// comment's lines in a fence no quoted backticks can close.
    #[test]
    fn comments_attach_after_the_prompt() {
        let comment = Comment {
            path: "README.md".into(),
            side: Side::New,
            start: 4,
            end: 4,
            quote: vec!["+```rust".into()],
            text: "Close this fence.".into(),
        };
        assert_eq!(attach("fix it", &[]), "fix it");
        assert_eq!(
            attach("Please address these.\n", std::slice::from_ref(&comment)),
            "Please address these.\n\nReview comments on your changes:\n\n\
             README.md:4\n````diff\n+```rust\n````\nClose this fence."
        );
        assert!(attach("", &[comment]).starts_with("Review comments on your changes:\n\n"));
    }

    /// Last turn reads only the edits after the latest prompt, one per
    /// edit, relative to the workspace.
    #[test]
    fn last_turn_reads_the_edits_since_the_latest_prompt() {
        let mut transcript = Transcript::default();
        let edit = |transcript: &mut Transcript, id: &str, path: &str| {
            transcript.apply(Input::Event(SessionEvent::ToolStarted {
                id: id.into(),
                name: "Edit".into(),
                input: serde_json::json!({ "file_path": path }),
            }));
            transcript.apply(Input::Event(SessionEvent::ToolCompleted {
                id: id.into(),
                output: String::new(),
                is_error: false,
                result: ToolResult::FileEdit {
                    path: path.into(),
                    hunks: vec![hunk(1, 1, &["-a", "+b"])],
                },
            }));
        };
        transcript.apply(Input::Prompt("first".into()));
        edit(&mut transcript, "1", "/repo/old.rs");
        transcript.apply(Input::Prompt("second".into()));
        edit(&mut transcript, "2", "/repo/src/new.rs");
        edit(&mut transcript, "3", "/repo/src/new.rs");
        edit(&mut transcript, "4", "/elsewhere/x.rs");
        let paths: Vec<String> = last_turn(&transcript, Some(Path::new("/repo")))
            .into_iter()
            .map(|edit| edit.path)
            .collect();
        assert_eq!(paths, ["src/new.rs", "src/new.rs", "/elsewhere/x.rs"]);
    }
}
