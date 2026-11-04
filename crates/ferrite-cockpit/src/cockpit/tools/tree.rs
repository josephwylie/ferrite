//! The Files tree's model (Claude's explorer, `files-empty.png`): a
//! checkout's folders read lazily as they open, each name carrying what
//! git says of it, and a filter that turns the tree into a flat list of the
//! files whose paths match. No window: the view (`files`) draws `rows`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Never listed: git's own store.
const HIDDEN: &[&str] = &[".git"];
/// Never searched by the filter: what a checkout builds or installs.
const NOT_SEARCHED: &[&str] = &[".git", "target", "node_modules", ".worktrees"];
/// The most files the filter looks through.
const SEARCH_LIMIT: usize = 20_000;

/// What git says of a path (`git status`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Modified,
    /// New to git: added, or untracked.
    Added,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub dir: bool,
}

/// One row of the tree as drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    pub entry: Entry,
    pub depth: usize,
    pub expanded: bool,
    pub status: Option<Status>,
}

pub(crate) struct FileTree {
    root: PathBuf,
    open: HashSet<PathBuf>,
    /// Each folder's entries, read once until the next `refresh`.
    listed: RefCell<HashMap<PathBuf, Vec<Entry>>>,
    status: HashMap<PathBuf, Status>,
    filter: String,
}

impl FileTree {
    pub(crate) fn new(root: PathBuf) -> Self {
        let mut tree = Self {
            root,
            open: HashSet::new(),
            listed: RefCell::default(),
            status: HashMap::new(),
            filter: String::new(),
        };
        tree.refresh();
        tree
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Read git's word on every path again, and every open folder afresh.
    pub(crate) fn refresh(&mut self) {
        self.listed.borrow_mut().clear();
        self.status = git_status(&self.root)
            .into_iter()
            .map(|(path, status)| (self.root.join(path), status))
            .collect();
    }

    pub(crate) fn set_filter(&mut self, filter: String) {
        self.filter = filter;
    }

    pub(crate) fn toggle(&mut self, dir: &Path) {
        if !self.open.remove(dir) {
            self.open.insert(dir.to_path_buf());
        }
    }

    pub(crate) fn collapse_all(&mut self) {
        self.open.clear();
    }

    /// Open every folder above `path`, so its row shows.
    pub(crate) fn reveal(&mut self, path: &Path) {
        for ancestor in path.ancestors().skip(1) {
            if ancestor.starts_with(&self.root) && ancestor != self.root {
                self.open.insert(ancestor.to_path_buf());
            }
        }
    }

    pub(crate) fn status_of(&self, path: &Path) -> Option<Status> {
        self.status.get(path).copied()
    }

    /// The rows to draw: the tree as the open folders make it, or — while
    /// the filter holds words — the files whose paths match, best first.
    pub(crate) fn rows(&self) -> Vec<Row> {
        if self.filter.trim().is_empty() {
            let mut rows = Vec::new();
            let root = self.root.clone();
            self.push_children(&root, 0, &mut rows);
            rows
        } else {
            self.matching_files()
        }
    }

    fn push_children(&self, dir: &Path, depth: usize, rows: &mut Vec<Row>) {
        for entry in self.children(dir) {
            let expanded = entry.dir && self.open.contains(&entry.path);
            let status = self.status_of(&entry.path);
            let path = entry.path.clone();
            rows.push(Row {
                entry,
                depth,
                expanded,
                status,
            });
            if expanded {
                self.push_children(&path, depth + 1, rows);
            }
        }
    }

    fn children(&self, dir: &Path) -> Vec<Entry> {
        self.listed
            .borrow_mut()
            .entry(dir.to_path_buf())
            .or_insert_with(|| read_dir(dir))
            .clone()
    }

    fn matching_files(&self) -> Vec<Row> {
        let mut found: Vec<(i64, Entry)> = Vec::new();
        let mut seen = 0;
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in read_dir(&dir) {
                if entry.dir {
                    if !NOT_SEARCHED.contains(&entry.name.as_str()) {
                        stack.push(entry.path);
                    }
                    continue;
                }
                seen += 1;
                if seen > SEARCH_LIMIT {
                    stack.clear();
                    break;
                }
                let relative = entry.path.strip_prefix(&self.root).unwrap_or(&entry.path);
                if let Some((score, _)) =
                    crate::fuzzy::matches(&self.filter, &relative.to_string_lossy())
                {
                    found.push((score, entry));
                }
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.path.cmp(&b.1.path)));
        found
            .into_iter()
            .map(|(_, entry)| Row {
                status: self.status_of(&entry.path),
                entry,
                depth: 0,
                expanded: false,
            })
            .collect()
    }
}

/// A folder's entries: folders first, then files, each by name without
/// regard to case.
fn read_dir(dir: &Path) -> Vec<Entry> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<Entry> = read
        .flatten()
        .filter_map(|item| {
            let name = item.file_name().to_string_lossy().into_owned();
            if HIDDEN.contains(&name.as_str()) {
                return None;
            }
            let dir = item.path().is_dir();
            Some(Entry {
                path: item.path(),
                name,
                dir,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

/// What `git status` says under `root`, as paths from it: empty where git
/// cannot say (no repository, no `git`).
fn git_status(root: &Path) -> Vec<(PathBuf, Status)> {
    let Ok(output) = Command::new("git")
        .args(["status", "--porcelain=v1", "-z"])
        .current_dir(root)
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_status(&String::from_utf8_lossy(&output.stdout))
}

/// `git status --porcelain=v1 -z`: `XY path` entries ending in NUL (a
/// rename is followed by its old path, which is not a change of its own).
pub(crate) fn parse_status(porcelain: &str) -> Vec<(PathBuf, Status)> {
    let mut entries = porcelain.split('\0').filter(|entry| !entry.is_empty());
    let mut out = Vec::new();
    while let Some(entry) = entries.next() {
        let (Some(code), Some(path)) = (entry.get(..2), entry.get(3..)) else {
            continue;
        };
        let status = match code {
            "??" | "A " | "AM" | "AD" => Status::Added,
            c if c.contains('D') => Status::Deleted,
            c if c.contains('R') || c.contains('C') => {
                entries.next();
                Status::Modified
            }
            _ => Status::Modified,
        };
        out.push((PathBuf::from(path.trim_end_matches('/')), status));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-tree-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(rows: &[Row]) -> Vec<(String, usize)> {
        rows.iter().map(|row| (row.entry.name.clone(), row.depth)).collect()
    }

    #[test]
    fn porcelain_reads_each_state_and_skips_a_renames_old_path() {
        let parsed = parse_status(" M a.rs\0?? new/\0D  gone.rs\0R  to.rs\0from.rs\0A  b.rs\0");
        assert_eq!(
            parsed,
            [
                ("a.rs".into(), Status::Modified),
                ("new".into(), Status::Added),
                ("gone.rs".into(), Status::Deleted),
                ("to.rs".into(), Status::Modified),
                ("b.rs".into(), Status::Added),
            ]
        );
    }

    #[test]
    fn folders_list_before_files_and_open_one_at_a_time() {
        let root = scratch("order");
        std::fs::create_dir_all(root.join("b/inner")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::write(root.join("a.txt"), "").unwrap();
        std::fs::write(root.join("b/x.rs"), "").unwrap();
        let mut tree = FileTree::new(root.clone());
        assert_eq!(names(&tree.rows()), [("b".into(), 0), ("a.txt".into(), 0)], ".git is never listed");
        tree.toggle(&root.join("b"));
        assert_eq!(
            names(&tree.rows()),
            [("b".into(), 0), ("inner".into(), 1), ("x.rs".into(), 1), ("a.txt".into(), 0)]
        );
        tree.collapse_all();
        assert_eq!(tree.rows().len(), 2);
    }

    #[test]
    fn reveal_opens_every_folder_above_a_file() {
        let root = scratch("reveal");
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/c.rs"), "").unwrap();
        let mut tree = FileTree::new(root.clone());
        tree.reveal(&root.join("a/b/c.rs"));
        assert_eq!(names(&tree.rows()).last().unwrap(), &("c.rs".to_string(), 2));
    }

    #[test]
    fn the_filter_lists_matching_files_flat_and_skips_build_folders() {
        let root = scratch("filter");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("src/motion.rs"), "").unwrap();
        std::fs::write(root.join("src/menu.rs"), "").unwrap();
        std::fs::write(root.join("target/motion.o"), "").unwrap();
        let mut tree = FileTree::new(root);
        tree.set_filter("motion".into());
        assert_eq!(names(&tree.rows()), [("motion.rs".into(), 0)]);
    }
}
