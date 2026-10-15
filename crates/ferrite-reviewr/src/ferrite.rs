//! The Ferrite adapter, in place of upstream's herdr adapter (`herdr.rs`, `herdr_socket.rs`):
//! what Ferrite's Server answers its review commands with (ADR 0021), and what its window
//! builds the Review slot from. The scope reads follow upstream's `world.rs`, the file reads,
//! the comment anchor and the PR notes its `app.rs` and `ui.rs`, whose terminal UI Ferrite's
//! GPUI view replaces.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError, mpsc};
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::diff::{DiffCache, FileDiff, MAX_BYTES, Notice, Row, over_byte_budget};
use crate::forge::{PrInputError, PrView};
use crate::git;
use crate::highlight::Highlighter;
use crate::model::{ChangeKind, ChangedFile, Comment, CommentStore, CommitPick, Side};
use crate::search::{SearchCompletion, SearchJob, SearchOutcome, SearchResults};
use crate::turn::{Status, TurnHost};

/// The most file text one review carries; the files past it read as too large.
pub const MAX_REVIEW: usize = 8 << 20;

/// What a review reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Read {
    /// The worktree against `HEAD`, untracked files included.
    Uncommitted,
    /// The worktree against the merge base of the resolved base branch.
    Branch,
    /// The worktree against its snapshot when the last turn that changed it began
    /// ([`agents_changed`]).
    LastTurn,
    /// A picked run of commits, `oldest^` against `newest`.
    Commits(CommitPick),
}

/// The ends a changeset was diffed between (`new` `None` for the worktree).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffEnds {
    pub old: String,
    pub new: Option<String>,
}

/// A read's changed files, by path, and the ends they were diffed between.
#[derive(Clone, Debug, Default)]
pub struct Changeset {
    pub files: Vec<ChangedFile>,
    /// `None` when the read has nothing to diff: no baseline, base, or live pick.
    pub ends: Option<DiffEnds>,
}

/// `read`'s changeset in `repo`, `base` naming the branch base as upstream's `--base` does
/// (`None`: the pick, then the default branch). Upstream's `world::scope_build`.
pub fn changed(repo: &Path, read: &Read, base: Option<&str>) -> Result<Changeset> {
    if !git::is_repo(repo) {
        anyhow::bail!("not a git repository");
    }
    session_started(repo);
    match read {
        Read::LastTurn => match git::read_baseline_ref(repo) {
            Some(tree) => {
                let now = git::snapshot_worktree(repo)?;
                at_ends(repo, DiffEnds { old: tree, new: Some(now) })
            }
            None => Ok(Changeset::default()),
        },
        Read::Uncommitted => {
            at_ends(repo, DiffEnds { old: git::diff_base(git::head_oid(repo)), new: None })
        }
        Read::Branch => {
            let resolution = git::resolve_base(repo, base)?;
            let merge_base =
                resolution.status.winner.as_ref().and_then(|w| git::merge_base(repo, w.oid()));
            match merge_base {
                Some(old) => at_ends(repo, DiffEnds { old, new: None }),
                None => Ok(Changeset::default()),
            }
        }
        Read::Commits(pick) => {
            if !git::commit_exists(repo, &pick.newest) {
                anyhow::bail!("commit {} is gone", git::abbreviate_oid(&pick.newest));
            }
            let Some(old) = git::parent_or_empty(repo, &pick.oldest) else {
                anyhow::bail!("commit {} is gone", git::abbreviate_oid(&pick.oldest));
            };
            at_ends(repo, DiffEnds { old, new: Some(pick.newest.clone()) })
        }
    }
}

/// The changeset between `ends`, carried beside them.
fn at_ends(repo: &Path, ends: DiffEnds) -> Result<Changeset> {
    let changed = match &ends.new {
        None => git::changed_from(repo, &ends.old)?,
        Some(new) => git::changed_between(repo, &ends.old, new)?,
    };
    let files: BTreeMap<String, ChangedFile> =
        changed.into_iter().map(|f| (f.path.clone(), f)).collect();
    Ok(Changeset { files: files.into_values().collect(), ends: Some(ends) })
}

/// The checkouts this process has read in, a review or a turn.
fn reviewed() -> &'static Mutex<HashSet<PathBuf>> {
    static REVIEWED: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    REVIEWED.get_or_init(Mutex::default)
}

/// Note a read in `repo`; the first sweeps the index copies a dead process left in its git dir.
fn session_started(repo: &Path) {
    if reviewed().lock().unwrap_or_else(PoisonError::into_inner).insert(repo.to_path_buf()) {
        git::sweep_dead_copies(repo);
    }
}

/// One changed file and both its sides, or the notice that stands in for them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSides {
    pub file: ChangedFile,
    pub sides: Result<(String, String), Notice>,
}

impl FileSides {
    /// The file's rows, built here: plain spans, for the window to paint its own syntax.
    pub fn rows(&self) -> FileDiff {
        let (path, previous) = (self.file.path.clone(), self.file.previous_path.clone());
        match &self.sides {
            Ok((old, new)) => FileDiff::build(path, previous, old, new, highlighter()),
            Err(notice) => FileDiff::notice(path, previous, *notice, crate::diff::View::Diff),
        }
    }
}

/// `read`'s files in `repo`, each with its sides, in path order — or why it cannot be read.
pub fn review(repo: &Path, read: &Read) -> Result<Vec<FileSides>, String> {
    let changeset = changed(repo, read, None).map_err(|error| format!("{error:#}"))?;
    let Some(ends) = changeset.ends else {
        return Err(match read {
            Read::LastTurn => "waiting for the first turn",
            Read::Branch => "no base branch to compare against",
            Read::Uncommitted | Read::Commits(_) => "nothing to compare",
        }
        .to_string());
    };
    let mut carried = 0usize;
    Ok(changeset
        .files
        .into_iter()
        .map(|file| {
            let sides = content_sides(repo, &ends, &file).and_then(|(old, new)| {
                carried += old.len() + new.len();
                if carried > MAX_REVIEW {
                    return Err(Notice::TooLarge);
                }
                Ok((old, new))
            });
            FileSides { file, sides }
        })
        .collect())
}

/// `file`'s sides between `ends`: a notice, or one `git diff` of them. Upstream's
/// `App::content_sides`.
fn content_sides(
    repo: &Path,
    ends: &DiffEnds,
    file: &ChangedFile,
) -> Result<(String, String), Notice> {
    if file.binary {
        return Err(Notice::Binary);
    }
    // An untracked file is all new side, read raw.
    if file.kind == ChangeKind::Untracked {
        return worktree_content(repo, &file.path).map(|new| (String::new(), new));
    }
    // git diffs a tracked symlink as its target path, so the worktree side is the link's size.
    let new_size = file
        .new_size
        .unwrap_or_else(|| std::fs::symlink_metadata(repo.join(&file.path)).map_or(0, |m| m.len()));
    let total = file.old_size.saturating_add(new_size);
    if over_byte_budget(usize::try_from(total).unwrap_or(usize::MAX)) {
        return Err(Notice::TooLarge);
    }
    let source = file.previous_path.as_deref();
    match git::diff_sides(repo, &ends.old, ends.new.as_deref(), &file.path, source) {
        Ok(git::DiffSides::Text { old, new }) => Ok((old, new)),
        Ok(git::DiffSides::Binary) => Err(Notice::Binary),
        Err(error) => {
            logln!("diff of {} failed: {error:#}", file.path);
            Err(Notice::Unreadable)
        }
    }
}

/// `path`'s worktree content under the byte budget. Upstream's `app::worktree_content`.
fn worktree_content(repo: &Path, path: &str) -> Result<String, Notice> {
    use std::io::{ErrorKind, Read};
    let at = repo.join(path);
    let meta = match std::fs::metadata(&at) {
        Ok(meta) if meta.is_file() => meta,
        Ok(_) => return Ok(String::new()),
        // Gone, or a parent turned into a file (which Windows also reports as not found).
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            return Ok(String::new());
        }
        Err(_) => return Err(Notice::Unreadable),
    };
    if over_byte_budget(usize::try_from(meta.len()).unwrap_or(usize::MAX)) {
        return Err(Notice::TooLarge);
    }
    // Capped, so a file grown since its stat still stops past the budget.
    let mut bytes = Vec::new();
    let cap = MAX_BYTES as u64 + 1;
    match std::fs::File::open(at).and_then(|f| f.take(cap).read_to_end(&mut bytes)) {
        Ok(_) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
        Err(_) => Err(Notice::Unreadable),
    }
}

/// The worktree file `path` (relative, inside `repo`) for the file view: its text, or the
/// notice in its place. A path that climbs out of the repo reads as unreadable.
pub fn read_file(repo: &Path, path: &str) -> Result<String, Notice> {
    let inside = Path::new(path).components().all(|c| matches!(c, Component::Normal(_)));
    if !inside {
        return Err(Notice::Unreadable);
    }
    worktree_content(repo, path)
}

/// The file view of `content`: every line a context row, under the diff's budgets.
pub fn file_rows(path: &str, content: &str) -> FileDiff {
    DiffCache::new().get_file(path.to_string(), content, highlighter())
}

/// The highlighter rows are built with: none, since Ferrite paints syntax itself.
fn highlighter() -> &'static Highlighter {
    static PLAIN: OnceLock<Highlighter> = OnceLock::new();
    PLAIN.get_or_init(Highlighter::plain)
}

/// One of the Engine's agents: its Thread, where it works, and its status in upstream's terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agent {
    pub thread: String,
    pub cwd: PathBuf,
    pub status: Status,
}

/// The Engine's agents changed, `at` when it saw them: turn tracking follows them, as upstream's
/// follows herdr's `agent_status` (`herdr_socket.rs`). Every checkout an agent works in gets a
/// [`TurnHost`] on a thread of its own, which snapshots its turn's start and promotes it to
/// `last turn` once the turn ends having changed the checkout. Ferrite runs no watcher, so a
/// turn is judged at its end, as upstream judges one its watcher was down for.
pub fn agents_changed(agents: Vec<Agent>, at: Instant) {
    static TURNS: OnceLock<Mutex<mpsc::Sender<(Vec<Agent>, Instant)>>> = OnceLock::new();
    let turns = TURNS.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new().name("turns".into()).spawn(move || track_turns(&rx));
        Mutex::new(tx)
    });
    let _ = turns.lock().unwrap_or_else(PoisonError::into_inner).send((agents, at));
}

/// Find each agent's checkout and tell every checkout's turn host what the agents are now.
fn track_turns(news: &mpsc::Receiver<(Vec<Agent>, Instant)>) {
    // A resolved root never moves, so it is kept; outside and unknown are asked again.
    let mut roots: HashMap<PathBuf, PathBuf> = HashMap::new();
    let mut hosts: HashMap<PathBuf, mpsc::Sender<(Arc<Vec<Agent>>, Instant)>> = HashMap::new();
    for (agents, at) in news {
        for agent in &agents {
            if roots.contains_key(&agent.cwd) || !agent.cwd.is_absolute() {
                continue;
            }
            if let git::Worktree::Root(root) = git::worktree_of(&agent.cwd) {
                roots.insert(agent.cwd.clone(), root);
            }
        }
        for agent in &agents {
            let Some(root) = roots.get(&agent.cwd) else { continue };
            hosts.entry(root.clone()).or_insert_with(|| turn_host(root.clone()));
        }
        let agents = Arc::new(agents);
        hosts.retain(|_, host| host.send((Arc::clone(&agents), at)).is_ok());
    }
}

/// The turn host of the checkout at `root`, on its own thread so one slow snapshot holds up no
/// other checkout's turn.
fn turn_host(root: PathBuf) -> mpsc::Sender<(Arc<Vec<Agent>>, Instant)> {
    let (tx, rx) = mpsc::channel::<(Arc<Vec<Agent>>, Instant)>();
    let _ = std::thread::Builder::new().name("turn-host".into()).spawn(move || {
        session_started(&root);
        let mut host = TurnHost::open(root);
        host.watcher_down(true);
        // The Engine owns its agents from their start: before any was seen here, none worked,
        // so the first status it reports can start a turn.
        host.observe_statuses(Some(&[]), Instant::now());
        for (agents, at) in rx {
            let cwds: Vec<String> = agents.iter().map(|a| a.cwd.to_string_lossy().into()).collect();
            let named: Vec<(&str, Option<&str>)> = agents
                .iter()
                .zip(&cwds)
                .map(|(a, cwd)| (a.thread.as_str(), Some(cwd.as_str())))
                .collect();
            let statuses = host.members(&named).map(|members| {
                agents
                    .iter()
                    .filter(|a| members.contains(&a.thread.as_str()))
                    .map(|a| a.status)
                    .collect::<Vec<_>>()
            });
            host.observe_statuses(statuses.as_deref(), at);
        }
    });
    tx
}

/// The branch's commits since its base, newest first along first parents (50 when no base
/// resolves).
pub fn commits(repo: &Path) -> Result<Vec<git::CommitRow>, String> {
    if !git::is_repo(repo) {
        return Err("not a git repository".to_string());
    }
    let resolution = git::resolve_base(repo, None).map_err(|error| error.0)?;
    let merge_base = resolution.status.winner.as_ref().and_then(|w| git::merge_base(repo, w.oid()));
    git::list_commits(repo, merge_base.as_deref()).map_err(|error| format!("{error:#}"))
}

/// The `(side, start, end, snippet)` of diff rows: new side unless all deletions. Upstream's
/// `app::anchor`.
pub fn anchor(selected: &[Row]) -> Option<(Side, u32, u32, String)> {
    let mut new: Option<(u32, u32)> = None;
    let mut old: Option<(u32, u32)> = None;
    let mut snippet = String::new();
    for row in selected.iter().filter(|row| row.is_content()) {
        if !snippet.is_empty() {
            snippet.push('\n');
        }
        snippet.push_str(&row.marker_text());
        if let Some(line) = row.new_no() {
            new = Some(new.map_or((line, line), |(min, max)| (min.min(line), max.max(line))));
        }
        if let Some(line) = row.old_no() {
            old = Some(old.map_or((line, line), |(min, max)| (min.min(line), max.max(line))));
        }
    }
    let (side, (start, end)) =
        new.map(|range| (Side::New, range)).or_else(|| old.map(|range| (Side::Old, range)))?;
    Some((side, start, end, snippet))
}

/// Whether row `row` holds the last line of `comment`'s range, where its card hangs.
pub fn ends_at(comment: &Comment, row: &Row) -> bool {
    match comment.side {
        Side::New => row.new_no() == Some(comment.end),
        Side::Old => row.new_no().is_none() && row.old_no() == Some(comment.end),
    }
}

/// `text` with the review comments after it, as one prompt: the operator's words first,
/// then the comments as upstream sends them ([`crate::export::format_all`]).
pub fn attach(text: &str, comments: &CommentStore) -> String {
    if comments.is_empty() {
        return text.to_string();
    }
    let refs: Vec<&Comment> = comments.iter().collect();
    let review = crate::export::format_all(&refs);
    let text = text.trim_end();
    if text.is_empty() { review } else { format!("{text}\n\n{review}") }
}

/// How long a search worker sits idle before its next query rescans the worktree: the
/// engine has no watcher of its own.
const RESCAN_AFTER: Duration = Duration::from_secs(30);

/// How long one search waits for its answer, a cold index's first scan included.
const SEARCH_WAIT: Duration = Duration::from_secs(20);

/// `query`'s file and content matches in `repo`, from the repo's `fff-search` worker (started
/// on the first search, kept while the process runs). A newer query to the same repo answers
/// in this one's place, which then says it was superseded.
pub fn search(repo: &Path, query: &str) -> Result<SearchResults, String> {
    let worker = Searcher::of(repo);
    let generation = worker.ask(query);
    worker.answer(generation)
}

/// Where the search engine keeps its frecency store: Ferrite's cache, not upstream's.
fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("ferrite").join("search")
}

/// One repo's search worker and the newest answer it gave.
struct Searcher {
    jobs: Mutex<mpsc::Sender<SearchJob>>,
    /// The newest generation asked, and when.
    asked: Mutex<(u64, Instant)>,
    /// The newest completion: its generation and outcome.
    answered: Mutex<Option<(u64, Result<SearchResults, String>)>>,
    landed: Condvar,
}

impl Searcher {
    fn of(repo: &Path) -> Arc<Self> {
        static WORKERS: OnceLock<Mutex<HashMap<PathBuf, Arc<Searcher>>>> = OnceLock::new();
        let mut workers =
            WORKERS.get_or_init(Mutex::default).lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(workers.entry(repo.to_path_buf()).or_insert_with(|| Self::spawn(repo)))
    }

    fn spawn(repo: &Path) -> Arc<Self> {
        let (jobs, rx) = mpsc::channel();
        let (tx, completions) = mpsc::channel::<SearchCompletion>();
        crate::search::spawn(repo.to_path_buf(), cache_dir(), rx, tx);
        let searcher = Arc::new(Self {
            jobs: Mutex::new(jobs),
            asked: Mutex::new((0, Instant::now())),
            answered: Mutex::new(None),
            landed: Condvar::new(),
        });
        let landing = Arc::downgrade(&searcher);
        let _ = std::thread::Builder::new().name("search-answers".into()).spawn(move || {
            for completion in completions {
                let outcome = match completion.outcome {
                    // The query re-runs when the scan lands; its answer comes then.
                    SearchOutcome::Indexing => continue,
                    SearchOutcome::Ready(results) => Ok(results),
                    SearchOutcome::Failed(error) => Err(error),
                };
                let Some(searcher) = landing.upgrade() else { return };
                *searcher.answered.lock().unwrap_or_else(PoisonError::into_inner) =
                    Some((completion.generation, outcome));
                searcher.landed.notify_all();
            }
        });
        searcher
    }

    /// Queue `query`, rescanning first when the worker sat idle: its generation.
    fn ask(&self, query: &str) -> u64 {
        let mut asked = self.asked.lock().unwrap_or_else(PoisonError::into_inner);
        let idle = asked.1.elapsed() > RESCAN_AFTER;
        *asked = (asked.0 + 1, Instant::now());
        let jobs = self.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        if idle && asked.0 > 1 {
            let _ = jobs.send(SearchJob::Rescan);
        }
        let _ = jobs.send(SearchJob::Query { generation: asked.0, query: query.to_string() });
        asked.0
    }

    /// The answer to `generation`, once it or a newer one lands.
    fn answer(&self, generation: u64) -> Result<SearchResults, String> {
        let answered = self.answered.lock().unwrap_or_else(PoisonError::into_inner);
        let (answered, _) = self
            .landed
            .wait_timeout_while(answered, SEARCH_WAIT, |answered| {
                answered.as_ref().is_none_or(|(landed, _)| *landed < generation)
            })
            .unwrap_or_else(PoisonError::into_inner);
        match answered.as_ref() {
            Some((landed, outcome)) if *landed == generation => outcome.clone(),
            Some((landed, _)) if *landed > generation => {
                Err("superseded by a newer search".to_string())
            }
            _ => Err("still indexing this repository".to_string()),
        }
    }
}

/// `repo`'s pull request as its forge's CLI reads it (`gh`, `glab` or `az`, read-only), and the
/// forge's noun for one (`PR`, `MR`).
pub fn pull_request(repo: &Path) -> (PrView, &'static str) {
    let config = crate::config::PluginConfig::default();
    match crate::forge::fetch_input(repo, None, &config) {
        Ok(input) => {
            let noun = match &input.repository {
                git::RepositoryIdentity::Repository(target) => target.forge().noun(),
                _ => git::Forge::GitHub.noun(),
            };
            (crate::forge::fetch(repo, &input), noun)
        }
        Err(PrInputError::TargetRead(message) | PrInputError::BranchState { message, .. }) => {
            (PrView::GitError(message), git::Forge::GitHub.noun())
        }
    }
}

/// What a PR read with no snapshot says, `noun` naming the forge's: the remedy for a
/// retryable failure, `refresh` naming the key that retries. Upstream's `ui::pr_empty_msg`.
pub fn pr_note(view: &PrView, noun: &str, refresh: crate::keymap::Key) -> String {
    if let Some(message) = view.retry_remedy(refresh) {
        return message;
    }
    match view {
        PrView::Loading => "loading…".into(),
        // A snapshot, or a retryable failure answered above.
        PrView::Pending
        | PrView::Pr(_)
        | PrView::Held
        | PrView::NoCli(_)
        | PrView::NoExtension(_)
        | PrView::NotAuthed(..)
        | PrView::GitError(_)
        | PrView::Error(..) => String::new(),
        PrView::Detached => format!("No {noun} for a detached HEAD."),
        PrView::NoPr => format!("No {noun} yet. Ready to ship?"),
        PrView::NeedsForgeRemote => {
            "The PR tab needs a GitHub, GitLab, or Azure DevOps remote named upstream or origin."
                .into()
        }
        PrView::UnsupportedHost(host) => format!("Unsupported host: {host}."),
        PrView::MalformedOrigin(host) => {
            format!("The origin remote must point to a repository path on {host}.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Rev;
    use crate::test_support::test_repo;

    fn comment(file: &str, start: u32, end: u32, lines: &str, text: &str) -> Comment {
        Comment {
            file: file.into(),
            side: Side::New,
            start,
            end,
            lines: lines.into(),
            text: text.into(),
            diff_anchored: true,
            rev: Rev::Worktree,
        }
    }

    #[test]
    fn comments_ride_after_the_operators_words() {
        let mut comments = CommentStore::new();
        assert_eq!(attach("fix it", &comments), "fix it");
        comments.add(comment("b.rs", 2, 2, "+b", "why?"));
        assert_eq!(attach("  ", &comments), "b.rs:2\n+b\nwhy?");
        comments.add(comment("a.rs", 1, 1, "+a", "ok"));
        assert_eq!(
            attach("fix these\n", &comments),
            "fix these\n\na.rs:1\n+a\nok\n\nb.rs:2\n+b\nwhy?"
        );
    }

    #[test]
    fn a_selection_anchors_on_the_new_side_unless_all_removed() {
        let file = FileSides {
            file: ChangedFile {
                path: "a.rs".into(),
                kind: ChangeKind::Modified,
                additions: 1,
                deletions: 1,
                previous_path: None,
                binary: false,
                old_size: 0,
                new_size: None,
            },
            sides: Ok(("keep\nold\n".into(), "keep\nnew\n".into())),
        };
        let rows = file.rows().rows;
        let markers: String = rows.iter().map(Row::marker).collect();
        assert_eq!(markers, " -+");
        assert_eq!(anchor(&rows), Some((Side::New, 1, 2, " keep\n-old\n+new".into())));
        assert_eq!(anchor(&rows[1..2]), Some((Side::Old, 2, 2, "-old".into())));
        let removed = Comment { side: Side::Old, ..comment("a.rs", 2, 2, "-old", "x") };
        assert!(ends_at(&removed, &rows[1]) && !ends_at(&removed, &rows[2]));
        assert!(ends_at(&comment("a.rs", 1, 2, "", "x"), &rows[2]));
    }

    #[test]
    fn a_review_reads_each_scope_and_says_why_when_it_cannot() {
        let (dir, git) = test_repo();
        let repo = dir.path();
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "root"]);
        git(&["switch", "-qc", "work"]);
        std::fs::write(repo.join("a.txt"), "two\n").unwrap();
        git(&["commit", "-qam", "two"]);
        std::fs::write(repo.join("new.txt"), "fresh\n").unwrap();

        let paths = |read: &Read| -> Vec<String> {
            review(repo, read).unwrap().into_iter().map(|f| f.file.path).collect()
        };
        assert_eq!(paths(&Read::Uncommitted), ["new.txt"]);
        assert_eq!(paths(&Read::Branch), ["a.txt", "new.txt"]);
        let rows = commits(repo).unwrap();
        assert_eq!(rows.iter().map(|r| r.subject.as_str()).collect::<Vec<_>>(), ["two"]);
        let pick = Read::Commits(CommitPick::single(&rows[0].sha));
        let [only] = review(repo, &pick).unwrap().try_into().unwrap();
        assert_eq!(only.sides, Ok(("one\n".to_string(), "two\n".to_string())));

        assert_eq!(review(repo, &Read::LastTurn).unwrap_err(), "waiting for the first turn");
        let tree = git::snapshot_worktree(repo).unwrap();
        git::write_baseline_ref(repo, &tree).unwrap();
        std::fs::write(repo.join("a.txt"), "three\n").unwrap();
        assert_eq!(paths(&Read::LastTurn), ["a.txt"]);

        let elsewhere = tempfile::tempdir().unwrap();
        assert_eq!(
            review(elsewhere.path(), &Read::Uncommitted).unwrap_err(),
            "not a git repository"
        );
        assert!(review(repo, &Read::Commits(CommitPick::single("0123abcd"))).is_err());
    }

    #[test]
    fn the_file_view_reads_only_inside_the_repo() {
        let (dir, _git) = test_repo();
        std::fs::write(dir.path().join("a.md"), "# A\nbody\n").unwrap();
        assert_eq!(read_file(dir.path(), "a.md"), Ok("# A\nbody\n".into()));
        assert_eq!(read_file(dir.path(), "../a.md"), Err(Notice::Unreadable));
        assert_eq!(read_file(dir.path(), "/etc/hosts"), Err(Notice::Unreadable));
        assert_eq!(read_file(dir.path(), "gone.md"), Ok(String::new()));
        let rows = file_rows("a.md", "# A\nbody\n").rows;
        assert_eq!(rows.iter().map(Row::text).collect::<Vec<_>>(), ["# A", "body"]);
    }

    #[test]
    fn a_turn_that_changed_nothing_leaves_the_last_turn_as_it_was() {
        let (dir, git) = test_repo();
        let repo = dir.path();
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "root"]);
        let git::Worktree::Root(root) = git::worktree_of(repo) else { panic!("a repo") };
        let mut host = TurnHost::open(root);
        host.watcher_down(true);
        let at = Instant::now();
        host.observe_statuses(Some(&[]), at);
        host.observe_statuses(Some(&[Status::Working]), at);
        std::fs::write(repo.join("a.txt"), "two\n").unwrap();
        host.observe_statuses(Some(&[Status::Idle]), at);
        let baseline = git::read_baseline_ref(repo).expect("a turn that wrote promotes its start");
        let paths = || -> Vec<String> {
            review(repo, &Read::LastTurn).unwrap().into_iter().map(|f| f.file.path).collect()
        };
        assert_eq!(paths(), ["a.txt"]);
        // A question: the agent works, writes nothing, rests.
        host.observe_statuses(Some(&[Status::Working]), at);
        host.observe_statuses(Some(&[Status::Idle]), at);
        assert_eq!(git::read_baseline_ref(repo), Some(baseline), "the turn before still shows");
        assert_eq!(paths(), ["a.txt"]);
    }

    #[test]
    fn the_engines_agents_drive_the_last_turn() {
        let (dir, git) = test_repo();
        let repo = dir.path();
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "root"]);
        let agent = |status| Agent { thread: "t1".into(), cwd: repo.to_path_buf(), status };
        agents_changed(vec![agent(Status::Idle)], Instant::now());
        // A write that lands before the start's snapshot is part of it, so each try writes anew
        // after giving the snapshot time.
        for attempt in 0..20 {
            agents_changed(vec![agent(Status::Working)], Instant::now());
            std::thread::sleep(Duration::from_millis(200));
            std::fs::write(repo.join("a.txt"), format!("edit {attempt}\n")).unwrap();
            agents_changed(vec![agent(Status::Idle)], Instant::now());
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if git::read_baseline_ref(repo).is_some() {
                    let [only] = review(repo, &Read::LastTurn).unwrap().try_into().unwrap();
                    assert_eq!(only.file.path, "a.txt");
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        panic!("no turn was promoted");
    }

    #[test]
    fn a_search_finds_files_and_lines_and_a_newer_one_supersedes() {
        let (dir, git) = test_repo();
        std::fs::write(dir.path().join("alpha.rs"), "fn needle() {}\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "root"]);
        let results = search(dir.path(), "needle").unwrap();
        assert!(results.code.iter().any(|hit| hit.path == "alpha.rs" && hit.line == 1));
        let results = search(dir.path(), "alpha").unwrap();
        assert!(results.files.iter().any(|hit| hit.path == "alpha.rs"));
    }

    #[test]
    fn a_pr_read_without_a_forge_remote_says_what_it_needs() {
        let (dir, git) = test_repo();
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "root"]);
        let (view, noun) = pull_request(dir.path());
        assert_eq!(view, PrView::NeedsForgeRemote);
        assert!(pr_note(&view, noun, crate::keymap::Key::plain('r')).contains("remote named"));
        assert_eq!(
            pr_note(&PrView::NoPr, "PR", crate::keymap::Key::plain('r')),
            "No PR yet. Ready to ship?"
        );
    }
}
