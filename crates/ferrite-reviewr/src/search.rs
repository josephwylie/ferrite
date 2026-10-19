//! The search worker: the `fff-search` engine off the frame loop, latest result wins.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use fff_search::{
    FFFQuery, FilePicker, FilePickerOptions, FileSearchConfig, FrecencyTracker, FuzzySearchOptions,
    GrepSearchOptions, PaginationArgs, SharedFilePicker, SharedFrecency,
};

/// The most results one query fetches per group; the rest show as `… N more`.
const FILE_LIMIT: usize = 50;
const CODE_LIMIT: usize = 200;
/// One grep's time budget; past it, partial results.
const GREP_BUDGET_MS: u64 = 80;
/// How often a cold worker re-checks whether its first scan finished.
const WARMUP_POLL: Duration = Duration::from_millis(50);

#[cfg(test)]
thread_local! {
    /// Full rescans this thread asked for.
    static RESCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The files under `folders` in `parent`, walked as fff's scan walks (ignore rules, hidden files,
/// no `.git`); one walk from the parent holds each folder to the rules too.
fn folder_files(parent: &Path, folders: BTreeSet<PathBuf>) -> impl Iterator<Item = PathBuf> {
    ignore::WalkBuilder::new(parent)
        .hidden(false)
        .filter_entry(move |entry| {
            entry.file_name() != ".git" && (entry.depth() != 1 || folders.contains(entry.path()))
        })
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|t| t.is_file()))
        .map(ignore::DirEntry::into_path)
}

/// The engine's cache home. The frecency store lives here, never the worktree
pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("herdr-reviewr")
}

/// One request to the worker.
#[derive(Debug)]
pub enum SearchJob {
    /// Run `query`; the completion echoes the generation back.
    Query { generation: u64, query: String },
    /// Record a picked result in the engine's frecency store.
    Track { path: String },
    /// Paths reviewr's watcher saw change, relative to the repo: the engine has no watcher of its own.
    Changed { paths: Vec<String> },
    /// Changes were lost: the engine scans the worktree again.
    Rescan,
}

/// A path match, in engine order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileHit {
    pub path: String,
    /// Byte spans into `path` the engine matched — the emphasis input.
    pub spans: Vec<(u32, u32)>,
}

/// A content match, in engine order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeHit {
    pub path: String,
    /// 1-based line number.
    pub line: u64,
    pub text: String,
    /// Byte spans into `text` the engine matched — the emphasis input.
    pub spans: Vec<(u32, u32)>,
}

/// One query's results; `code_more` marks a grep the cap or time budget cut short.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchResults {
    pub files: Vec<FileHit>,
    pub code: Vec<CodeHit>,
    pub file_total: usize,
    pub code_more: bool,
}

/// A finished query's outcome.
#[derive(Debug)]
pub enum SearchOutcome {
    /// Results for the query; the previously landed set stays painted until this lands.
    Ready(SearchResults),
    /// The first scan is still running; the query re-runs when it lands.
    Indexing,
    /// The engine failed; its message shows in the results pane.
    Failed(String),
}

/// A finished query: the tag it ran for, and its outcome.
#[derive(Debug)]
pub struct SearchCompletion {
    pub generation: u64,
    pub outcome: SearchOutcome,
}

/// The engine, initialized once on the worker thread.
struct Engine {
    shared: SharedFilePicker,
    frecency: SharedFrecency,
    repo: PathBuf,
}

impl Engine {
    /// Start the scan and indexing with no watcher of its own (reviewr's feeds it), the frecency
    /// store under `cache_dir`.
    fn start(repo: PathBuf, cache_dir: &Path) -> Result<Self, String> {
        let shared = SharedFilePicker::default();
        let frecency = SharedFrecency::default();
        match std::fs::create_dir_all(cache_dir) {
            Ok(()) => match FrecencyTracker::open(cache_dir.join("frecency")) {
                Ok(tracker) => {
                    if let Err(e) = frecency.init(tracker) {
                        logln!("search frecency init failed: {e}");
                    }
                }
                // Search works without frecency; ranking just stops improving with use.
                Err(e) => logln!("search frecency open failed: {e}"),
            },
            Err(e) => logln!("search cache dir failed: {e}"),
        }
        FilePicker::new_with_shared_state(
            shared.clone(),
            frecency.clone(),
            FilePickerOptions {
                base_path: repo.to_string_lossy().into_owned(),
                enable_content_indexing: true,
                watch: false,
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(Self { shared, frecency, repo })
    }

    /// Follow paths the watcher saw change: a file is added or re-read, a folder's files are
    /// added, a gone one dropped with what was under it. An index out of room scans again instead.
    fn apply(&self, paths: &[String]) {
        let rescan = {
            let Ok(mut guard) = self.shared.write() else { return };
            let Some(picker) = guard.as_mut() else { return };
            let mut rescan = false;
            let mut folders: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
            for rel in paths {
                let path = self.repo.join(rel);
                // fff's scan indexes no link, to a file or a folder: what stood there goes.
                let link = path.symlink_metadata().is_ok_and(|m| m.is_symlink());
                if link || !path.exists() {
                    if !picker.remove_file_by_path(&path) {
                        picker.remove_all_files_in_dir(&path);
                    }
                } else if path.is_dir() {
                    // A file that stood here before the folder goes.
                    picker.remove_file_by_path(&path);
                    let parent = path.parent().unwrap_or(&path).to_path_buf();
                    folders.entry(parent).or_default().insert(path);
                } else if path.is_file() {
                    rescan |= picker.handle_create_or_modify(&path).is_none();
                }
            }
            // Past the index's room, a rescan settles it: stop adding at the first refusal.
            for (parent, inside) in folders {
                rescan = rescan
                    || folder_files(&parent, inside)
                        .any(|file| picker.handle_create_or_modify(&file).is_none());
            }
            rescan
        };
        if rescan {
            self.rescan();
        }
    }

    /// Scan the worktree again, behind the queries.
    fn rescan(&self) {
        #[cfg(test)]
        RESCANS.with(|n| n.set(n.get() + 1));
        if let Err(e) = self.shared.trigger_full_rescan_async(&self.frecency) {
            logln!("search rescan failed: {e}");
        }
    }

    fn warm(&self) -> bool {
        self.shared.wait_for_scan(Duration::from_millis(1))
    }

    /// Run one query against the warm index: the path group, then the content group.
    fn run(&self, raw: &str) -> Result<SearchResults, String> {
        let guard = self.shared.read().map_err(|e| e.to_string())?;
        let picker = guard.as_ref().ok_or("search engine not ready")?;
        let query = FFFQuery::parse(raw, FileSearchConfig);

        let found = picker.fuzzy_search(
            &query,
            None,
            FuzzySearchOptions {
                pagination: PaginationArgs { offset: 0, limit: FILE_LIMIT },
                ..Default::default()
            },
        );
        let files: Vec<FileHit> = found
            .items
            .iter()
            .zip(&found.match_byte_offsets)
            .map(|(i, spans)| FileHit {
                path: i.relative_path(picker),
                spans: spans.iter().copied().collect(),
            })
            .collect();
        let file_total = found.total_matched.max(files.len());

        // An empty query shows the frecency-ranked files alone; an empty grep is noise.
        if raw.trim().is_empty() {
            return Ok(SearchResults { files, code: Vec::new(), file_total, code_more: false });
        }

        let mut grep = picker.grep(
            &query,
            &GrepSearchOptions {
                page_limit: CODE_LIMIT,
                time_budget_ms: GREP_BUDGET_MS,
                ..Default::default()
            },
        );
        // Rows drop their indentation; the engine shifts its offsets to match.
        for m in &mut grep.matches {
            m.trim_leading_whitespace();
        }
        let code: Vec<CodeHit> = grep
            .matches
            .iter()
            .map(|m| CodeHit {
                path: grep.files[m.file_index].relative_path(picker),
                line: m.line_number,
                text: m.line_content.clone(),
                spans: m.match_byte_offsets.iter().copied().collect(),
            })
            .collect();
        let code_more = grep.next_file_offset != 0;

        Ok(SearchResults { files, code, file_total, code_more })
    }

    /// Record a pick in the frecency store. Failures only log — a pick must always land.
    fn track(&self, path: &str) {
        match self.frecency.read() {
            Ok(guard) => {
                if let Some(tracker) = guard.as_ref()
                    && let Err(e) = tracker.track_access(&self.repo.join(path))
                {
                    logln!("search frecency track failed: {e}");
                }
            }
            Err(e) => logln!("search frecency lock failed: {e}"),
        }
    }
}

/// Run the search worker until its channel closes; queued queries coalesce into the newest.
pub fn spawn(
    repo: PathBuf,
    cache_dir: PathBuf,
    rx: Receiver<SearchJob>,
    tx: crate::wake::Sender<SearchCompletion>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("search".into())
        .spawn(move || {
            let engine = match Engine::start(repo, &cache_dir) {
                Ok(engine) => engine,
                Err(e) => {
                    // Report on the first query and exit: every later one would fail alike.
                    if let Ok(SearchJob::Query { generation, .. }) = rx.recv() {
                        let outcome = SearchOutcome::Failed(e);
                        let _ = tx.send(SearchCompletion { generation, outcome });
                    }
                    return;
                }
            };
            // The query awaiting a warm index, re-run when the scan lands.
            let mut pending: Option<(u64, String)> = None;
            loop {
                let request = if pending.is_some() {
                    match rx.recv_timeout(WARMUP_POLL) {
                        Ok(request) => Some(request),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                } else {
                    match rx.recv() {
                        Ok(request) => Some(request),
                        Err(_) => break,
                    }
                };
                // Latest keystroke wins: drain the queue before running anything.
                let mut job = None;
                for next in request.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
                    match next {
                        SearchJob::Query { generation, query } => {
                            job = Some((generation, query));
                        }
                        SearchJob::Track { path } => engine.track(&path),
                        SearchJob::Changed { paths } => engine.apply(&paths),
                        SearchJob::Rescan => engine.rescan(),
                    }
                }
                // A fresh job drops any query parked for warm-up.
                let fresh = job.is_some();
                if fresh {
                    pending = None;
                }
                let Some((generation, query)) = job.or_else(|| pending.take()) else {
                    continue;
                };
                if !engine.warm() {
                    pending = Some((generation, query));
                    // Said once per query: a parked one re-checks quietly, waking nobody.
                    if fresh {
                        let outcome = SearchOutcome::Indexing;
                        let _ = tx.send(SearchCompletion { generation, outcome });
                    }
                    continue;
                }
                let outcome = match engine.run(&query) {
                    Ok(results) => SearchOutcome::Ready(results),
                    Err(e) => SearchOutcome::Failed(e),
                };
                let completion = SearchCompletion { generation, outcome };
                if tx.send(completion).is_err() {
                    break;
                }
            }
        })
        .expect("spawn search worker")
}

#[cfg(test)]
mod tests {
    use super::{Engine, RESCANS};

    #[test]
    fn a_folder_that_appears_joins_the_index_without_a_rescan() {
        let (dir, git) = crate::test_support::test_repo();
        std::fs::write(dir.path().join(".gitignore"), "*.log\n").unwrap();
        std::fs::write(dir.path().join("a.rs"), "a\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "init"]);
        let cache = tempfile::tempdir().unwrap();
        let engine = Engine::start(dir.path().to_path_buf(), cache.path()).unwrap();
        assert!(engine.shared.wait_for_scan(std::time::Duration::from_secs(30)), "warm");
        std::fs::create_dir_all(dir.path().join("lib/deep")).unwrap();
        std::fs::write(dir.path().join("lib/deep/gamma.rs"), "g\n").unwrap();
        std::fs::write(dir.path().join("lib/build.log"), "x\n").unwrap();
        let before = RESCANS.with(std::cell::Cell::get);
        engine.apply(&["lib".to_string()]);
        assert_eq!(RESCANS.with(std::cell::Cell::get), before, "no full rescan");
        let guard = engine.shared.read().unwrap();
        let picker = guard.as_ref().unwrap();
        assert!(picker.get_file_by_path(dir.path().join("lib/deep/gamma.rs")).is_some());
        assert!(picker.get_file_by_path(dir.path().join("lib/build.log")).is_none(), "ignored");
        drop(guard);

        // Sibling folders arriving together are listed from their parent once, all indexed.
        let names: Vec<String> = (0..50).map(|i| format!("pkgs/p{i}")).collect();
        for name in &names {
            std::fs::create_dir_all(dir.path().join(name)).unwrap();
            std::fs::write(dir.path().join(name).join("lib.rs"), "l\n").unwrap();
        }
        engine.apply(&names);
        let guard = engine.shared.read().unwrap();
        let picker = guard.as_ref().unwrap();
        let indexed = names
            .iter()
            .filter(|n| picker.get_file_by_path(dir.path().join(n).join("lib.rs")).is_some())
            .count();
        assert_eq!(indexed, 50, "every sibling's file");
        drop(guard);

        // A file that gives way to an ignored folder goes, and the folder's insides stay out.
        std::fs::write(dir.path().join(".gitignore"), "*.log\nbuild/\n").unwrap();
        std::fs::write(dir.path().join("build"), "a file\n").unwrap();
        engine.apply(&["build".to_string()]);
        std::fs::remove_file(dir.path().join("build")).unwrap();
        std::fs::create_dir(dir.path().join("build")).unwrap();
        std::fs::write(dir.path().join("build/out.o"), "o\n").unwrap();
        engine.apply(&["build".to_string()]);
        let guard = engine.shared.read().unwrap();
        let picker = guard.as_ref().unwrap();
        assert!(
            picker.get_file_by_path(dir.path().join("build/out.o")).is_none(),
            "ignored folder"
        );
        drop(guard);

        // A link to a folder elsewhere is not followed, as fff's own scan does not.
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::write(elsewhere.path().join("far.rs"), "f\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("data")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(elsewhere.path(), dir.path().join("data")).unwrap();
        engine.apply(&["data".to_string()]);
        let guard = engine.shared.read().unwrap();
        let picker = guard.as_ref().unwrap();
        assert!(picker.get_file_by_path(dir.path().join("data/far.rs")).is_none(), "not followed");
        assert_eq!(RESCANS.with(std::cell::Cell::get), before, "and no rescan");
    }
}
