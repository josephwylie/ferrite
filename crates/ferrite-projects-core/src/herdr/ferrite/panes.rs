//! Ferrite: the panes and Spaces the board placed. herdr makes a pane at a shell and starts an agent in it later, again
//! after its agent ended, and the board keeps the pane's id across all of that; a Ferrite Thread is born with its
//! agent. So a placement (`tab create`, `pane split`, `workspace create`, `worktree create` and `open`) is kept here,
//! by an id of its own, as where a Thread will start, and that id stays the pane's: it names the Thread started there
//! (`agent start`), and the same Thread again when its agent is started anew (`thread.revive`). herdr's `workspace
//! create` always makes a new Space, where a Ferrite folder has one Project: so each is kept here too, a Space of its
//! own inside the Project, renamed and closed alone, and gone with its last pane. One file, beside the board's data, so
//! every process of the board finds the same panes; its lock is held only for one change.
use crate::err::Res;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

/// One pane the board placed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Pane {
    /// Its Space: a Project's id, a Space the board made (`Space`), or a linked worktree's path.
    pub space: String,
    /// Where its Thread works.
    pub cwd: String,
    /// Its tab's label.
    #[serde(default)]
    pub label: String,
    /// Given to its agent's environment (`--env`).
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// The Thread started in it, open or parked.
    #[serde(default)]
    pub thread: Option<u64>,
    /// The kind of agent that Thread runs.
    #[serde(default)]
    pub kind: String,
    /// A worktree Space's repository and label, as `worktree create` and `open` gave them.
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub space_label: String,
}

/// One Space the board made (`workspace create`), inside a Project.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Space {
    /// The Project's id.
    pub project: String,
    /// The folder it was made at.
    pub cwd: String,
    pub label: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Book {
    next: u64,
    panes: BTreeMap<String, Pane>,
    #[serde(default)]
    spaces: BTreeMap<String, Space>,
}

/// The panes, in the file at `path`.
pub struct Panes { pub path: PathBuf }

impl Panes {
    pub fn all(&self) -> Res<BTreeMap<String, Pane>> { Ok(self.read()?.panes) }
    pub fn spaces(&self) -> Res<BTreeMap<String, Space>> { Ok(self.read()?.spaces) }
    /// A new pane; its id.
    pub fn place(&self, pane: Pane) -> Res<String> { self.change(|book| book.add_pane(pane)) }
    /// A new Space with its first pane in it: the Space's id, then the pane's.
    pub fn open_space(&self, space: Space, pane: Pane) -> Res<(String, String)> {
        self.change(|book| {
            book.next += 1;
            let id = format!("s{}", book.next);
            book.spaces.insert(id.clone(), space);
            let pane = book.add_pane(Pane { space: id.clone(), ..pane });
            (id, pane)
        })
    }
    pub fn update(&self, id: &str, f: impl FnOnce(&mut Pane)) -> Res<()> {
        self.change(|book| { if let Some(p) = book.panes.get_mut(id) { f(p); } })
    }
    pub fn rename_space(&self, id: &str, label: &str) -> Res<()> {
        self.change(|book| { if let Some(s) = book.spaces.get_mut(id) { s.label = label.into(); } })
    }
    /// The panes `keep` says no to are forgotten, and a Space the board made goes with its last pane, as herdr's does.
    pub fn retain(&self, keep: impl Fn(&str, &Pane) -> bool) -> Res<()> {
        self.change(|book| {
            book.panes.retain(|id, p| keep(id, p));
            let panes = &book.panes;
            book.spaces.retain(|id, _| panes.values().any(|p| p.space == *id));
        })
    }

    fn read(&self) -> Res<Book> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Book::default()),
            Err(e) => Err(e.into()),
        }
    }
    fn change<T>(&self, f: impl FnOnce(&mut Book) -> T) -> Res<T> {
        if let Some(dir) = self.path.parent() { std::fs::create_dir_all(dir)?; }
        let lock = std::fs::OpenOptions::new().create(true).append(true).open(self.path.with_extension("lock"))?;
        // Held until `lock` is dropped, at the end of this change.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 { return Err(std::io::Error::last_os_error().into()); }
        let mut book = self.read()?;
        let out = f(&mut book);
        crate::store::files::write_atomic(&self.path, &serde_json::to_vec(&book)?)?;
        Ok(out)
    }
}

impl Book {
    fn add_pane(&mut self, pane: Pane) -> String {
        self.next += 1;
        let id = format!("p{}", self.next);
        self.panes.insert(id.clone(), pane);
        id
    }
}
