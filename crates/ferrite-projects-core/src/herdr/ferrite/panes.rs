//! Ferrite: the panes the board placed. herdr makes a pane at a shell and starts an agent in it later, again after its
//! agent ended, and the board keeps the pane's id across all of that; a Ferrite Thread is born with its agent. So a
//! placement (`tab create`, `pane split`, `workspace create`, `worktree create` and `open`) is kept here, by an id of
//! its own, as where a Thread will start, and that id stays the pane's: it names the Thread started there (`agent
//! start`), and the same Thread again when its agent is started anew (`thread.revive`). One file, beside the board's
//! data, so every process of the board finds the same panes; its lock is held only for one change.
use crate::err::Res;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

/// One pane the board placed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Pane {
    /// Its Space: a Project's id, or a linked worktree's path.
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

#[derive(Debug, Default, Serialize, Deserialize)]
struct Book { next: u64, panes: BTreeMap<String, Pane> }

/// The panes, in the file at `path`.
pub struct Panes { pub path: PathBuf }

impl Panes {
    pub fn all(&self) -> Res<BTreeMap<String, Pane>> { Ok(self.read()?.panes) }
    /// A new pane; its id.
    pub fn place(&self, pane: Pane) -> Res<String> {
        self.change(|book| { book.next += 1; let id = format!("p{}", book.next); book.panes.insert(id.clone(), pane); id })
    }
    pub fn update(&self, id: &str, f: impl FnOnce(&mut Pane)) -> Res<()> {
        self.change(|book| { if let Some(p) = book.panes.get_mut(id) { f(p); } })
    }
    /// The panes `keep` says no to are forgotten.
    pub fn retain(&self, keep: impl Fn(&str, &Pane) -> bool) -> Res<()> {
        self.change(|book| book.panes.retain(|id, p| keep(id, p)))
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
