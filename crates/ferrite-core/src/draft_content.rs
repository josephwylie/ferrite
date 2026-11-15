//! What an operator authored in a Composer, as typed parts. The words stay
//! literal; each kind of context (a reply to a passage, a file, a review
//! comment, a captured annotation, a picked token) is its own part, so none
//! has to be read back out of the words. It is what a draft is saved as, put
//! back as, and merged as. `from_prompt` is the one place prompt text is
//! split into parts: for text that only ever existed as a string (legacy
//! drafts, a prompt handed back by a Session).

use std::path::PathBuf;

use ferrite_reviewr::model::{Author, Comment, CommitPick, Reply as ReviewReply, Rev, Side};
use serde::{Deserialize, Serialize};

use crate::browser_annotations::{self, Annotation};
use crate::replies::{self, Reply};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DraftContent {
    /// The line as typed: Markdown stays Markdown, a leading `> ` stays text.
    pub text: String,
    pub files: Vec<PathBuf>,
    pub replies: Vec<Reply>,
    pub review: Vec<ReviewNote>,
    pub annotations: Vec<Annotation>,
    /// Tokens picked from a menu (`@path`, `/name`, `$skill`).
    pub chips: Vec<String>,
}

impl DraftContent {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// The parts of a prompt that only exists as text. Review comments sent
    /// as prose stay prose: nothing marks them apart from the words.
    pub fn from_prompt(prompt: String) -> Self {
        let (text, files, annotations) = browser_annotations::split(prompt);
        let (replies, text) = replies::split(&text);
        Self {
            text: text.to_string(),
            files,
            replies,
            annotations,
            ..Self::default()
        }
    }
}

/// A review comment as saved: `ferrite_reviewr`'s own type, which carries no
/// serde, mirrored field for field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReviewNote {
    file: String,
    side: NoteSide,
    start: u32,
    end: u32,
    lines: String,
    text: String,
    diff_anchored: bool,
    rev: NoteRev,
    replies: Vec<NoteReply>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum NoteSide {
    New,
    Old,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum NoteRev {
    Worktree,
    Commit { oldest: String, newest: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct NoteReply {
    /// None is the operator.
    other: Option<String>,
    text: String,
}

impl From<&Comment> for ReviewNote {
    fn from(comment: &Comment) -> Self {
        Self {
            file: comment.file.clone(),
            side: match comment.side {
                Side::New => NoteSide::New,
                Side::Old => NoteSide::Old,
            },
            start: comment.start,
            end: comment.end,
            lines: comment.lines.clone(),
            text: comment.text.clone(),
            diff_anchored: comment.diff_anchored,
            rev: match &comment.rev {
                Rev::Worktree => NoteRev::Worktree,
                Rev::Commit(pick) => NoteRev::Commit {
                    oldest: pick.oldest.clone(),
                    newest: pick.newest.clone(),
                },
            },
            replies: comment
                .replies
                .iter()
                .map(|reply| NoteReply {
                    other: match &reply.author {
                        Author::You => None,
                        Author::Other(name) => Some(name.clone()),
                    },
                    text: reply.text.clone(),
                })
                .collect(),
        }
    }
}

impl From<ReviewNote> for Comment {
    fn from(note: ReviewNote) -> Self {
        Self {
            file: note.file,
            side: match note.side {
                NoteSide::New => Side::New,
                NoteSide::Old => Side::Old,
            },
            start: note.start,
            end: note.end,
            lines: note.lines,
            text: note.text,
            diff_anchored: note.diff_anchored,
            rev: match note.rev {
                NoteRev::Worktree => Rev::Worktree,
                NoteRev::Commit { oldest, newest } => Rev::Commit(CommitPick { oldest, newest }),
            },
            replies: note
                .replies
                .into_iter()
                .map(|reply| ReviewReply {
                    author: reply.other.map_or(Author::You, Author::Other),
                    text: reply.text,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn comment() -> Comment {
        Comment {
            file: "src/lib.rs".into(),
            side: Side::Old,
            start: 3,
            end: 5,
            lines: "-a\n-b\n-c".into(),
            text: "why\nremoved?".into(),
            diff_anchored: true,
            rev: Rev::Commit(CommitPick::single("abc")),
            replies: vec![
                ReviewReply {
                    author: Author::You,
                    text: "mine".into(),
                },
                ReviewReply {
                    author: Author::Other("bot".into()),
                    text: "theirs".into(),
                },
            ],
        }
    }

    #[test]
    fn a_review_comment_survives_the_record_whole() {
        let comment = comment();
        let json = serde_json::to_string(&ReviewNote::from(&comment)).unwrap();
        let back: ReviewNote = serde_json::from_str(&json).unwrap();
        assert_eq!(Comment::from(back), comment);
    }

    #[test]
    fn a_prompt_as_text_is_split_into_typed_parts() {
        let file = std::env::temp_dir().join("f");
        let prompt = crate::prompt_files::compose(
            "> quoted\n\n\u{21a9} why\n\nbody",
            std::slice::from_ref(&file),
        );
        let content = DraftContent::from_prompt(prompt);
        assert_eq!(content.replies, vec![Reply::new("quoted", "why")]);
        assert_eq!(content.text, "body");
        assert_eq!(content.files, vec![file]);
    }
}
