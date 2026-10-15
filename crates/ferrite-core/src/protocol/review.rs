//! The Review slot's wire shapes (ADR 0021): what a Server reads with the
//! vendored review core (`ferrite_reviewr::ferrite`) where the checkout is,
//! and what a window draws. A local read (a window that runs its agents
//! itself) passes through the same shapes, so both draw alike.

use serde::{Deserialize, Serialize};

use ferrite_reviewr::diff::Notice;
use ferrite_reviewr::ferrite::FileSides;
use ferrite_reviewr::forge::{self, PrView};
use ferrite_reviewr::git::CommitRow;
use ferrite_reviewr::model::{ChangeKind, ChangedFile};
use ferrite_reviewr::search::{CodeHit, FileHit, SearchResults};

use super::Done;

/// One changed file of a `review_diff`, with both its sides — or the
/// notice that stands in for them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReviewFile {
    pub path: String,
    pub change: WireChange,
    /// A rename's or copy's source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
    pub additions: u32,
    pub deletions: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<WireFileNotice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireChange {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    #[serde(other)]
    Unknown,
}

/// Why a file's sides are not carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireFileNotice {
    Binary,
    TooLarge,
    Unreadable,
    #[serde(other)]
    Unknown,
}

impl From<FileSides> for WireReviewFile {
    fn from(FileSides { file, sides }: FileSides) -> Self {
        let change = match file.kind {
            ChangeKind::Added => WireChange::Added,
            ChangeKind::Modified => WireChange::Modified,
            ChangeKind::Deleted => WireChange::Deleted,
            ChangeKind::Renamed => WireChange::Renamed,
            ChangeKind::Copied => WireChange::Copied,
            ChangeKind::Untracked => WireChange::Untracked,
        };
        let (old, new, notice) = match sides {
            Ok((old, new)) => (Some(old), Some(new), None),
            Err(notice) => (
                None,
                None,
                Some(match notice {
                    Notice::Binary => WireFileNotice::Binary,
                    Notice::TooLarge => WireFileNotice::TooLarge,
                    Notice::Unreadable => WireFileNotice::Unreadable,
                }),
            ),
        };
        Self {
            path: file.path,
            change,
            previous_path: file.previous_path,
            additions: file.additions,
            deletions: file.deletions,
            old,
            new,
            notice,
        }
    }
}

impl From<WireReviewFile> for FileSides {
    fn from(file: WireReviewFile) -> Self {
        let kind = match file.change {
            WireChange::Added => ChangeKind::Added,
            WireChange::Deleted => ChangeKind::Deleted,
            WireChange::Renamed => ChangeKind::Renamed,
            WireChange::Copied => ChangeKind::Copied,
            WireChange::Untracked => ChangeKind::Untracked,
            WireChange::Modified | WireChange::Unknown => ChangeKind::Modified,
        };
        let sides = match (file.old, file.new, file.notice) {
            (Some(old), Some(new), None) => Ok((old, new)),
            (_, _, Some(WireFileNotice::Binary)) => Err(Notice::Binary),
            (_, _, Some(WireFileNotice::TooLarge)) => Err(Notice::TooLarge),
            _ => Err(Notice::Unreadable),
        };
        FileSides {
            file: ChangedFile {
                path: file.path,
                kind,
                additions: file.additions,
                deletions: file.deletions,
                previous_path: file.previous_path,
                binary: sides == Err(Notice::Binary),
                old_size: 0,
                new_size: None,
            },
            sides,
        }
    }
}

/// A `search_repo` path match: the path, and the byte spans that matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireFileHit {
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<(u32, u32)>,
}

/// A `search_repo` content match: where, the line's text, and the byte
/// spans of it that matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireCodeHit {
    pub path: String,
    pub line: u64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<(u32, u32)>,
}

impl From<SearchResults> for Done {
    fn from(results: SearchResults) -> Self {
        Done::Found {
            files: results
                .files
                .into_iter()
                .map(|hit| WireFileHit {
                    path: hit.path,
                    spans: hit.spans,
                })
                .collect(),
            code: results
                .code
                .into_iter()
                .map(|hit| WireCodeHit {
                    path: hit.path,
                    line: hit.line,
                    text: hit.text,
                    spans: hit.spans,
                })
                .collect(),
            file_total: results.file_total,
            code_more: results.code_more,
        }
    }
}

/// A `found` reply as the search core gives it.
pub fn search_results(
    files: Vec<WireFileHit>,
    code: Vec<WireCodeHit>,
    file_total: usize,
    code_more: bool,
) -> SearchResults {
    SearchResults {
        files: files
            .into_iter()
            .map(|hit| FileHit {
                path: hit.path,
                spans: hit.spans,
            })
            .collect(),
        code: code
            .into_iter()
            .map(|hit| CodeHit {
                path: hit.path,
                line: hit.line,
                text: hit.text,
                spans: hit.spans,
            })
            .collect(),
        file_total,
        code_more,
    }
}

impl From<CommitRow> for super::WireCommit {
    fn from(commit: CommitRow) -> Self {
        Self {
            id: commit.sha,
            subject: commit.subject,
            author: Some(commit.author),
            time: Some(commit.time),
        }
    }
}

/// A listed commit as the window draws it; the refs pointing at it are
/// not carried.
impl From<super::WireCommit> for CommitRow {
    fn from(commit: super::WireCommit) -> Self {
        Self {
            sha: commit.id,
            subject: commit.subject,
            time: commit.time.unwrap_or_default(),
            author: commit.author.unwrap_or_default(),
            refs: Vec::new(),
            merge: false,
        }
    }
}

/// A `pull_request`'s pull request: what its forge's CLI read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePr {
    pub number: u64,
    pub title: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    pub state: WirePrState,
    #[serde(default)]
    pub draft: bool,
    /// Its head branch and the branch it merges into.
    pub head: String,
    pub base: String,
    pub merge: WireMerge,
    pub sync: WireSync,
    #[serde(default)]
    pub checks: Vec<WireCheck>,
    #[serde(default)]
    pub comments: Vec<WirePrComment>,
    /// More checks or comments than one read fetches.
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WirePrState {
    Open,
    Merged,
    Closed,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireMerge {
    Clean,
    Conflicting,
    Blocked,
    #[serde(other)]
    Unknown,
}

/// The local branch against the pull request's head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireSync {
    InSync,
    /// Local commits the pull request lacks.
    Unpushed {
        commits: u32,
    },
    /// Pull request commits the checkout lacks.
    Behind {
        commits: u32,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireCheck {
    pub name: String,
    pub status: WireCheckStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireCheckStatus {
    Success,
    Failure,
    Running,
    Pending,
    Skipped,
    #[serde(other)]
    Unknown,
}

/// A review, a plain comment, or an inline finding on the pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WirePrComment {
    pub kind: WirePrCommentKind,
    pub author: String,
    /// `path:start-end` for a finding, the kind's word otherwise.
    pub anchor: String,
    pub body: String,
    /// A finding's quoted hunk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub resolved: bool,
    #[serde(default)]
    pub outdated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replies: Vec<WireReply>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WirePrCommentKind {
    Review,
    Comment,
    Finding,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireReply {
    pub author: String,
    pub body: String,
    pub created_at: String,
}

impl From<forge::PrSnapshot> for WirePr {
    fn from(pr: forge::PrSnapshot) -> Self {
        Self {
            number: pr.number,
            title: pr.title,
            url: pr.url,
            body: pr.body,
            state: match pr.state {
                forge::PrState::Open => WirePrState::Open,
                forge::PrState::Merged => WirePrState::Merged,
                forge::PrState::Closed => WirePrState::Closed,
            },
            draft: pr.is_draft,
            head: pr.head_ref,
            base: pr.base_ref,
            merge: match pr.merge {
                forge::Merge::Clean => WireMerge::Clean,
                forge::Merge::Conflicting => WireMerge::Conflicting,
                forge::Merge::Blocked => WireMerge::Blocked,
            },
            sync: match pr.sync {
                forge::Sync::InSync => WireSync::InSync,
                forge::Sync::Unpushed(commits) => WireSync::Unpushed { commits },
                forge::Sync::Behind(commits) => WireSync::Behind { commits },
                forge::Sync::Unknown => WireSync::Unknown,
            },
            checks: pr
                .checks
                .into_iter()
                .map(|check| WireCheck {
                    name: check.name,
                    status: match check.status {
                        forge::CheckStatus::Success => WireCheckStatus::Success,
                        forge::CheckStatus::Failure => WireCheckStatus::Failure,
                        forge::CheckStatus::Running => WireCheckStatus::Running,
                        forge::CheckStatus::Pending => WireCheckStatus::Pending,
                        forge::CheckStatus::Skipped => WireCheckStatus::Skipped,
                    },
                })
                .collect(),
            comments: pr
                .comments
                .into_iter()
                .map(|comment| WirePrComment {
                    kind: match comment.kind {
                        forge::CommentKind::Review => WirePrCommentKind::Review,
                        forge::CommentKind::Comment => WirePrCommentKind::Comment,
                        forge::CommentKind::Finding => WirePrCommentKind::Finding,
                    },
                    author: comment.author,
                    anchor: comment.anchor,
                    body: comment.body,
                    snippet: comment.snippet,
                    created_at: comment.created_at,
                    resolved: comment.is_resolved,
                    outdated: comment.is_outdated,
                    replies: comment
                        .replies
                        .into_iter()
                        .map(|reply| WireReply {
                            author: reply.author,
                            body: reply.body,
                            created_at: reply.created_at,
                        })
                        .collect(),
                })
                .collect(),
            truncated: pr.comments_truncated || pr.checks_truncated,
        }
    }
}

/// A `pull_request`'s answer as the window draws it: the forge's noun for
/// one (`PR`, `MR`), and the pull request or what stands in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub noun: String,
    pub pr: Result<WirePr, String>,
}

impl PullRequest {
    /// What the forge read gave: the snapshot, else its note (`r` retries).
    pub fn of(view: PrView, noun: &str) -> Self {
        let pr = match view {
            PrView::Pr(snapshot) => Ok((*snapshot).into()),
            view => Err(ferrite_reviewr::ferrite::pr_note(
                &view,
                noun,
                ferrite_reviewr::keymap::Key::plain('r'),
            )),
        };
        Self {
            noun: noun.to_string(),
            pr,
        }
    }
}

impl From<PullRequest> for Done {
    fn from(PullRequest { noun, pr }: PullRequest) -> Self {
        let (pr, note) = match pr {
            Ok(pr) => (Some(pr), None),
            Err(note) => (None, Some(note)),
        };
        Done::PullRequest { noun, pr, note }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(sides: Result<(String, String), Notice>) -> FileSides {
        FileSides {
            file: ChangedFile {
                path: "logo.png".into(),
                kind: ChangeKind::Renamed,
                additions: 2,
                deletions: 1,
                previous_path: Some("old.png".into()),
                binary: sides == Err(Notice::Binary),
                old_size: 0,
                new_size: None,
            },
            sides,
        }
    }

    /// A file crosses the wire and back as it was read: its sides, or
    /// the notice in their place.
    #[test]
    fn a_review_file_round_trips() {
        for sides in [
            Ok(("one\n".to_string(), "two\n".to_string())),
            Err(Notice::Binary),
            Err(Notice::TooLarge),
            Err(Notice::Unreadable),
        ] {
            let read = file(sides);
            let wire = WireReviewFile::from(read.clone());
            assert_eq!(FileSides::from(wire), read);
        }
    }

    /// A pull request's snapshot becomes the wire's; with none, the note
    /// says why, in the forge's noun.
    #[test]
    fn a_pull_request_carries_its_snapshot_or_its_note() {
        let note = PullRequest::of(PrView::NoPr, "MR");
        assert_eq!(note.pr, Err("No MR yet. Ready to ship?".into()));
        let Done::PullRequest { noun, pr, note } = note.into() else {
            panic!("a pull_request reply");
        };
        assert_eq!(
            (noun.as_str(), pr, note.as_deref()),
            ("MR", None, Some("No MR yet. Ready to ship?"))
        );
        let retry = PullRequest::of(PrView::NoCli(ferrite_reviewr::git::Forge::GitHub), "PR");
        assert!(
            retry.pr.is_err_and(|note| note.contains("press r")),
            "the remedy names r"
        );
    }
}
