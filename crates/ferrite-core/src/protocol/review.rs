//! The Review slot's wire shapes (ADR 0021): what a Server reads with the
//! vendored review core (`ferrite_reviewr::ferrite`) where the checkout is,
//! and what a window draws. A local read (a window that runs its agents
//! itself) passes through the same shapes, so both draw alike.

use serde::{Deserialize, Serialize};

use ferrite_reviewr::diff::Notice;
use ferrite_reviewr::ferrite::{BaseRow, Bases, FileSides};
use ferrite_reviewr::forge::{self, PrView};
use ferrite_reviewr::git::{CommitRow, WorktreeEntry};
use ferrite_reviewr::model::{ChangeKind, ChangedFile};
use ferrite_reviewr::search::{CodeHit, FileHit, SearchResults};

use super::Done;

/// One changed file of a `review_read`, with both its sides — or the
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

/// A review's files as the git-style unified diff a `review_diff` answers,
/// for a window from before `review_read`: each file with both sides, its
/// sides' changes with three lines around them. A file shown by a notice
/// is left out, as git's text left a binary one out of that window.
pub fn unified(files: &[FileSides]) -> String {
    let mut text = String::new();
    for FileSides { file, sides } in files {
        let Ok((old, new)) = sides else { continue };
        let from = file.previous_path.as_deref().unwrap_or(&file.path);
        let (old_name, new_name) = match file.kind {
            ChangeKind::Added | ChangeKind::Untracked => {
                ("/dev/null".into(), format!("b/{}", file.path))
            }
            ChangeKind::Deleted => (format!("a/{from}"), "/dev/null".into()),
            _ => (format!("a/{from}"), format!("b/{}", file.path)),
        };
        let diff = similar::TextDiff::from_lines(old, new);
        let hunks = diff
            .unified_diff()
            .context_radius(3)
            .header(&old_name, &new_name)
            .to_string();
        if hunks.is_empty() {
            continue;
        }
        text.push_str(&format!("diff --git a/{from} b/{}\n", file.path));
        text.push_str(&hunks);
    }
    text
}

/// A `review_diff`'s unified diff as a review's files, for a Server from
/// before `review_read`. The text carries only the lines around each
/// change, so each side is its hunks' lines with the lines between them
/// left blank: every line keeps its number, and a fold the window opens
/// shows blank lines. A file with no hunks (binary, or a bare rename) is
/// left out.
pub fn sides_of_unified(text: &str) -> Vec<FileSides> {
    #[derive(Default)]
    struct Building {
        old_path: Option<String>,
        new_path: Option<String>,
        old: String,
        new: String,
        old_lines: u32,
        new_lines: u32,
        additions: u32,
        deletions: u32,
        hunks: bool,
    }
    fn finish(building: Building, files: &mut Vec<FileSides>) {
        if !building.hunks {
            return;
        }
        let (kind, path, previous_path) = match (building.old_path, building.new_path) {
            (None, Some(path)) => (ChangeKind::Added, path, None),
            (Some(path), None) => (ChangeKind::Deleted, path, None),
            (Some(old), Some(new)) if old != new => (ChangeKind::Renamed, new, Some(old)),
            (_, Some(path)) => (ChangeKind::Modified, path, None),
            (None, None) => return,
        };
        files.push(FileSides {
            file: ChangedFile {
                path,
                kind,
                additions: building.additions,
                deletions: building.deletions,
                previous_path,
                binary: false,
                old_size: 0,
                new_size: None,
            },
            sides: Ok((building.old, building.new)),
        });
    }
    // git ends a path holding a space with a tab on the `---`/`+++` lines.
    let named = |path: &str, prefix: &str| {
        path.trim_end_matches('\t')
            .strip_prefix(prefix)
            .map(str::to_string)
    };
    let pad = |side: &mut String, lines: &mut u32, to: u32| {
        while *lines < to {
            side.push('\n');
            *lines += 1;
        }
    };
    let mut files = Vec::new();
    let mut building = Building::default();
    let mut in_hunk = false;
    for line in text.lines() {
        if line.starts_with("diff --git ") {
            finish(std::mem::take(&mut building), &mut files);
            in_hunk = false;
            continue;
        }
        if !in_hunk {
            if let Some(path) = line.strip_prefix("--- ") {
                building.old_path = named(path, "a/");
                continue;
            }
            if let Some(path) = line.strip_prefix("+++ ") {
                building.new_path = named(path, "b/");
                continue;
            }
        }
        if let Some((old_start, old_count, new_start, new_count)) = hunk_header(line) {
            // A side the hunk adds nothing to names the line it follows.
            let before = |start: u32, count: u32| {
                if count == 0 {
                    start
                } else {
                    start.saturating_sub(1)
                }
            };
            pad(
                &mut building.old,
                &mut building.old_lines,
                before(old_start, old_count),
            );
            pad(
                &mut building.new,
                &mut building.new_lines,
                before(new_start, new_count),
            );
            building.hunks = true;
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            continue;
        }
        let (marker, body) = line.split_at(line.len().min(1));
        let push = |side: &mut String, lines: &mut u32| {
            side.push_str(body);
            side.push('\n');
            *lines += 1;
        };
        match marker {
            " " | "" => {
                push(&mut building.old, &mut building.old_lines);
                push(&mut building.new, &mut building.new_lines);
            }
            "-" => {
                push(&mut building.old, &mut building.old_lines);
                building.deletions += 1;
            }
            "+" => {
                push(&mut building.new, &mut building.new_lines);
                building.additions += 1;
            }
            _ => {}
        }
    }
    finish(building, &mut files);
    files
}

/// A hunk header's ranges: `@@ -88,6 +88,9 @@ fn draw` → (88, 6, 88, 9).
fn hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let (old, rest) = line.strip_prefix("@@ -")?.split_once(" +")?;
    let (new, _) = rest.split_once(" @@")?;
    let range = |range: &str| -> Option<(u32, u32)> {
        let (start, count) = range.split_once(',').unwrap_or((range, "1"));
        Some((start.parse().ok()?, count.parse().ok()?))
    };
    let ((old_start, old_count), (new_start, new_count)) = (range(old)?, range(new)?);
    Some((old_start, old_count, new_start, new_count))
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

/// One `list_files` entry: a file, or an ignored directory standing for
/// everything under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireTreeEntry {
    pub path: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ignored: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dir: bool,
}

impl From<WorktreeEntry> for WireTreeEntry {
    fn from(entry: WorktreeEntry) -> Self {
        Self {
            path: entry.path,
            ignored: entry.ignored,
            dir: entry.is_dir,
        }
    }
}

impl From<WireTreeEntry> for WorktreeEntry {
    fn from(entry: WireTreeEntry) -> Self {
        Self {
            path: entry.path,
            ignored: entry.ignored,
            is_dir: entry.dir,
        }
    }
}

/// One `list_bases` row: a branch, with whether it is the default and the
/// one checked out and when its tip was made; or the revision (`oid`) the
/// pick names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireBase {
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub current: bool,
    #[serde(default)]
    pub tip: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oid: Option<String>,
}

impl From<Bases> for Done {
    fn from(Bases { rows, picked }: Bases) -> Self {
        let bases = rows
            .into_iter()
            .map(|row| WireBase {
                name: row.name,
                default: row.is_default,
                current: row.current,
                tip: row.tip_secs,
                oid: row.oid,
            })
            .collect();
        Done::Bases { bases, picked }
    }
}

/// A `bases` answer as the window's picker reads it.
pub fn bases(bases: Vec<WireBase>, picked: Option<String>) -> Bases {
    let rows = bases
        .into_iter()
        .map(|base| BaseRow {
            name: base.name,
            is_default: base.default,
            current: base.current,
            tip_secs: base.tip,
            oid: base.oid,
        })
        .collect();
    Bases { rows, picked }
}

/// A `review_pull_request`'s pull request: what its forge's CLI read.
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
    /// Which of the two ran past the fetch (`truncated` is both, for a
    /// window from before these): more checks, more comments.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub checks_truncated: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub comments_truncated: bool,
}

impl WirePr {
    /// Whether the forge holds more checks than were read. A Server from
    /// before the split says only `truncated`: then either may have.
    pub fn more_checks(&self) -> bool {
        self.checks_truncated || (self.truncated && !self.comments_truncated)
    }

    /// Whether the forge holds more comments than were read.
    pub fn more_comments(&self) -> bool {
        self.comments_truncated || (self.truncated && !self.checks_truncated)
    }
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
            checks_truncated: pr.checks_truncated,
            comments_truncated: pr.comments_truncated,
        }
    }
}

/// A `review_pull_request`'s answer as the window draws it: the forge's noun for
/// one (`PR`, `MR`), and the pull request or what stands in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewPullRequest {
    pub noun: String,
    pub pr: Result<WirePr, String>,
    /// With no pull request: why, where it is more than "the branch has
    /// none". `None` beside a snapshot, and for a branch with no PR.
    pub unread: Option<WirePrUnread>,
}

/// Why a `review_pull_request` brought no pull request back, beyond its
/// note: what the panel's PR tab tells apart from a branch with none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WirePrUnread {
    /// The forge's CLI is not installed (or lacks its extension), or not
    /// signed in.
    NoCli,
    /// The read failed (offline, a rate limit, a git error): the last
    /// snapshot still stands.
    #[serde(other)]
    Failed,
}

impl WirePrUnread {
    /// What a forge read with no snapshot says of why; `None` for a branch
    /// with no pull request to read.
    fn of(view: &PrView) -> Option<Self> {
        match view {
            PrView::NoCli(_) | PrView::NoExtension(_) | PrView::NotAuthed(..) => Some(Self::NoCli),
            PrView::Pending
            | PrView::Loading
            | PrView::Held
            | PrView::GitError(_)
            | PrView::Error(..) => Some(Self::Failed),
            PrView::Pr(_)
            | PrView::NoPr
            | PrView::Detached
            | PrView::NeedsForgeRemote
            | PrView::UnsupportedHost(_)
            | PrView::MalformedOrigin(_) => None,
        }
    }
}

impl ReviewPullRequest {
    /// What the forge read gave: the snapshot, else its note (`r` retries).
    pub fn of(view: PrView, noun: &str) -> Self {
        let unread = WirePrUnread::of(&view);
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
            unread,
        }
    }
}

impl From<ReviewPullRequest> for Done {
    fn from(ReviewPullRequest { noun, pr, unread }: ReviewPullRequest) -> Self {
        let (pr, note) = match pr {
            Ok(pr) => (Some(pr), None),
            Err(note) => (None, Some(note)),
        };
        Done::ReviewPullRequest {
            noun,
            pr,
            note,
            unread,
        }
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

    fn changed(
        path: &str,
        kind: ChangeKind,
        previous_path: Option<&str>,
        old: &str,
        new: &str,
    ) -> FileSides {
        FileSides {
            file: ChangedFile {
                path: path.into(),
                kind,
                additions: 0,
                deletions: 0,
                previous_path: previous_path.map(str::to_string),
                binary: false,
                old_size: 0,
                new_size: None,
            },
            sides: Ok((old.into(), new.into())),
        }
    }

    /// A `review_diff`'s text, read back by a window from before or after
    /// `review_read`, gives the same changes at the same line numbers;
    /// only the lines far from a change are left blank.
    #[test]
    fn a_unified_diff_reads_back_as_the_same_changes() {
        let old: String = (1..=30).map(|n| format!("line {n}\n")).collect();
        let new = old
            .replace("line 5\n", "five\n")
            .replace("line 25\n", "line 25\nadded\n");
        let files = vec![
            changed("src/a.rs", ChangeKind::Modified, None, &old, &new),
            changed(
                "src/b.rs",
                ChangeKind::Renamed,
                Some("src/old.rs"),
                "x\n",
                "y\n",
            ),
            changed("new.txt", ChangeKind::Untracked, None, "", "hello\n"),
            changed("gone.txt", ChangeKind::Deleted, None, "bye\n", ""),
            FileSides {
                sides: Err(Notice::Binary),
                ..changed("logo.png", ChangeKind::Modified, None, "", "")
            },
        ];
        let text = unified(&files);
        let read = sides_of_unified(&text);
        assert_eq!(unified(&read), text, "the same hunks");
        let shape: Vec<_> = read
            .iter()
            .map(|f| {
                (
                    f.file.path.as_str(),
                    f.file.kind,
                    f.file.previous_path.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            shape,
            [
                ("src/a.rs", ChangeKind::Modified, None),
                ("src/b.rs", ChangeKind::Renamed, Some("src/old.rs")),
                ("new.txt", ChangeKind::Added, None),
                ("gone.txt", ChangeKind::Deleted, None),
            ]
        );
        let Ok((old_side, new_side)) = &read[0].sides else {
            panic!("sides");
        };
        assert_eq!(old_side.lines().nth(4), Some("line 5"));
        assert_eq!(new_side.lines().nth(4), Some("five"));
        assert_eq!(new_side.lines().nth(25), Some("added"));
        assert_eq!(
            old_side.lines().next(),
            Some(""),
            "far from a change: blank"
        );
        assert_eq!((read[0].file.additions, read[0].file.deletions), (2, 1));
    }

    /// A pull request's snapshot becomes the wire's; with none, the note
    /// says why, in the forge's noun.
    #[test]
    fn a_pull_request_carries_its_snapshot_or_its_note() {
        let note = ReviewPullRequest::of(PrView::NoPr, "MR");
        assert_eq!(note.pr, Err("No MR yet. Ready to ship?".into()));
        let Done::ReviewPullRequest {
            noun,
            pr,
            note,
            unread,
        } = note.into()
        else {
            panic!("a review_pull_request reply");
        };
        assert_eq!(
            (noun.as_str(), pr, note.as_deref(), unread),
            ("MR", None, Some("No MR yet. Ready to ship?"), None),
            "no PR is no unread"
        );
        let retry = ReviewPullRequest::of(PrView::NoCli(ferrite_reviewr::git::Forge::GitHub), "PR");
        assert_eq!(retry.unread, Some(WirePrUnread::NoCli), "no gh");
        assert!(
            retry.pr.is_err_and(|note| note.contains("press r")),
            "the remedy names r"
        );
        let failed = ReviewPullRequest::of(
            PrView::Error(ferrite_reviewr::git::Forge::GitHub, "offline".into()),
            "PR",
        );
        assert_eq!(failed.unread, Some(WirePrUnread::Failed));
    }
}
