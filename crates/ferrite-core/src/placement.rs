//! A Project of this computer put on a Machine (ADR 0048): the folder
//! copied there through the tree ops, and which Project there is its
//! counterpart, the one a Draft's environment chip swaps to.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::chunked::{Cancellation, Progress};
use crate::tree::{self, Entry, Exchange, Manifest, Root, Sent, Stopped};
use crate::project::ProjectId;
use crate::workspace::registry::Registry;
use crate::workspace::Repositories;

/// Where a Project of this computer that was no repository was copied to
/// on a Machine: the one record of which Project there is its counterpart.
/// A repository needs none, since a copy carries its `.git`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub local_root: PathBuf,
    pub remote_root: PathBuf,
}

const PLACEMENTS: &str = "placements.json";

/// The Placements on the Machine whose files are in `dir` (`machine::dir`).
pub fn placements(dir: &Path) -> Vec<Placement> {
    fs::read(dir.join(PLACEMENTS))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Keep `placement`, in place of any for the same Project of this computer.
pub fn place(dir: &Path, placement: Placement) -> io::Result<()> {
    let mut known = placements(dir);
    known.retain(|old| old.local_root != placement.local_root);
    known.push(placement);
    fs::create_dir_all(dir)?;
    crate::file_operations::replace_file(&dir.join(PLACEMENTS), &serde_json::to_vec_pretty(&known)?)
}

/// Which way a Project's counterpart is looked for, and the Placements of
/// the Machine on the far side of it.
pub enum Across<'a> {
    /// From this computer to a Machine.
    ToMachine(&'a [Placement]),
    /// From a Machine to this computer.
    FromMachine(&'a [Placement]),
    /// Between two Machines, where no Placement is kept.
    Between,
}

/// The counterpart there of the Project at `root` whose repository is
/// `repository`: the first Project there of the same repository, else the
/// one a Placement says it was copied to or from.
pub fn counterpart(
    root: &Path,
    repository: Option<&str>,
    across: Across<'_>,
    there: &Registry,
    repositories: &Repositories,
) -> Option<ProjectId> {
    let by_repository = repository.and_then(|repository| {
        there
            .projects()
            .iter()
            .find(|project| repositories.get(&project.id).map(String::as_str) == Some(repository))
    });
    let placed = match across {
        Across::ToMachine(placements) => placements
            .iter()
            .find(|placement| placement.local_root == root)
            .map(|placement| &placement.remote_root),
        Across::FromMachine(placements) => placements
            .iter()
            .find(|placement| placement.remote_root == root)
            .map(|placement| &placement.local_root),
        Across::Between => None,
    };
    by_repository
        .or_else(|| {
            let placed = placed?;
            there.projects().iter().find(|project| &project.root == placed)
        })
        .map(|project| project.id)
}

/// Why `dest`, a directory on a Machine as the operator gave it, cannot
/// take a copy: it must be absolute (`/`, or a drive's on a Windows
/// Machine) or under `~`, and not the home or a filesystem's root itself.
pub fn refuse_destination(dest: &str) -> Option<&'static str> {
    fn under_drive(dest: &str) -> Option<&str> {
        let mut chars = dest.chars();
        let lettered = chars.next().is_some_and(|letter| letter.is_ascii_alphabetic()) && chars.next() == Some(':');
        lettered.then(|| &dest[2..])?.strip_prefix(['/', '\\'])
    }
    let dest = dest.trim().trim_end_matches(['/', '\\']);
    let under = dest
        .strip_prefix("~/")
        .or_else(|| dest.strip_prefix('/'))
        .or_else(|| under_drive(dest));
    match under {
        _ if dest.is_empty() || dest == "~" => Some("Choose a folder, not the home folder"),
        None => Some("Start the path with / or ~/"),
        Some(rest) if rest.split(['/', '\\']).any(|part| part == "..") => Some("A path through .. is not allowed"),
        Some(_) => None,
    }
}

/// An entry git ignores at the top of a Project folder's tree, which a
/// copy leaves out unless the operator picks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ignored {
    /// Relative to the folder, `/`-separated, a directory's without its
    /// trailing `/`.
    pub path: String,
    pub directory: bool,
}

/// What git ignores in the checkout at `root`, each directory as a whole:
/// none outside git.
pub fn ignored(root: &Path) -> Vec<Ignored> {
    let Ok(listed) = crate::workspace::git(
        root,
        &["ls-files", "-z", "--others", "--ignored", "--exclude-standard", "--directory"],
    ) else {
        return Vec::new();
    };
    listed
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(|path| Ignored {
            path: path.trim_end_matches('/').to_string(),
            directory: path.ends_with('/'),
        })
        .collect()
}

/// The bytes of the files at `path` under `root`, links not followed.
pub fn size(root: &Path, path: &str) -> u64 {
    fn walk(path: &Path) -> u64 {
        let Ok(meta) = fs::symlink_metadata(path) else {
            return 0;
        };
        if !meta.is_dir() {
            return meta.len();
        }
        fs::read_dir(path)
            .map(|entries| entries.flatten().map(|entry| walk(&entry.path())).sum())
            .unwrap_or(0)
    }
    walk(&root.join(path))
}

/// Whether `dest` on a Machine holds anything, read as a mirror there
/// reads it: one that does takes no copy, which would remove what is there.
pub fn occupied(dest: &Path, exchange: &mut dyn Exchange) -> io::Result<bool> {
    let scan = tree::Request::Scan {
        root: Root::Path { path: dest.to_path_buf() },
        skip: Vec::new(),
        after: None,
    };
    match exchange.call(scan)? {
        tree::Reply::Scanned { manifest, .. } => Ok(!manifest.entries.is_empty() || !manifest.problems.is_empty()),
        _ => Err(io::Error::other("the Machine did not answer the scan")),
    }
}

/// Make `dest` on a Machine hold the Project folder `source`, `skip` left
/// out: a mirror, so `dest` must be new or this copy's own. What could not
/// be read as it was listed is sent once more, and what still could not is
/// in the answer's problems. From Windows, which keeps no permission bits,
/// each file lands with the bits [`give_modes`] gives it, and the copy's
/// git config then agrees with its files (see [`align_git`]).
pub fn copy(
    source: &Path,
    dest: &Path,
    skip: &[String],
    exchange: &mut dyn Exchange,
    progress: &mut dyn FnMut(Progress),
    cancel: &Cancellation,
) -> Result<Sent, Stopped> {
    let root = Root::Path { path: dest.to_path_buf() };
    let mut listed = |here: &mut Manifest| give_modes(source, here);
    let mut sent = tree::mirror_listed(source, &root, skip, &mut listed, exchange, progress, cancel)?;
    if sent.problems.iter().any(|problem| problem.side == tree::Side::Here) {
        let again = tree::mirror_listed(source, &root, skip, &mut listed, exchange, progress, cancel)
            .map_err(|stopped| Stopped {
                sent: added(sent.clone(), stopped.sent),
                error: stopped.error,
            })?;
        sent = added(sent, again);
    }
    if cfg!(windows) {
        if let Err(error) = align_git(source, &root, exchange, cancel) {
            return Err(Stopped { error, sent });
        }
    }
    Ok(sent)
}

/// `first` and then `then`, whose problems are the ones left.
fn added(first: Sent, then: Sent) -> Sent {
    Sent {
        removed: first.removed + then.removed,
        made: first.made + then.made,
        written: first.written + then.written,
        linked: first.linked + then.linked,
        bytes: first.bytes + then.bytes,
        problems: then.problems,
    }
}

/// Give each file of `listed`, a scan of the Project folder `source` on a
/// computer that keeps no permission bits, the bits it lands with: 0755
/// for one git tracks as executable, 0600 for one git ignores, this
/// computer's own as a `.env` is, and 0644 for the rest. What kept its own
/// bits keeps them.
fn give_modes(source: &Path, listed: &mut Manifest) {
    if !listed.entries.values().any(|entry| matches!(entry, Entry::File { mode: None, .. })) {
        return;
    }
    let executable = executables(source);
    let private: Vec<String> = ignored(source).into_iter().map(|ignored| ignored.path).collect();
    let is_private = |path: &str| {
        private.iter().any(|ignored| {
            path.strip_prefix(ignored.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        })
    };
    for (path, entry) in &mut listed.entries {
        if let Entry::File { mode: mode @ None, .. } = entry {
            *mode = Some(if executable.contains(path) {
                0o755
            } else if is_private(path) {
                0o600
            } else {
                0o644
            });
        }
    }
}

/// The files git tracks as executable (100755) in the checkout at `root`,
/// by their paths under it: none outside git.
fn executables(root: &Path) -> BTreeSet<String> {
    let Ok(listed) = crate::workspace::git(root, &["ls-files", "--stage", "-z"]) else {
        return BTreeSet::new();
    };
    listed
        .split('\0')
        .filter_map(|line| line.strip_prefix("100755 ")?.split_once('\t'))
        .map(|(_, path)| path.to_string())
        .collect()
}

/// Give the copy at `dest` of the repository at `source` this side's
/// `core.autocrlf` and `core.filemode` false, as a checkout on a computer
/// that keeps no executable bits has it, so git there reads files written
/// as this computer's checkout has them, with its line ends, as unchanged.
/// Nothing for a folder whose `.git` is not a directory.
pub fn align_git(source: &Path, dest: &Root, exchange: &mut dyn Exchange, cancel: &Cancellation) -> io::Result<()> {
    if !source.join(".git").is_dir() {
        return Ok(());
    }
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let staged = std::env::temp_dir().join(format!("ferrite-git-config-{}-{nonce}", std::process::id()));
    let aligned = (|| {
        fs::create_dir_all(staged.join(".git"))?;
        let config = staged.join(".git/config");
        fs::copy(source.join(".git/config"), &config)?;
        let autocrlf = crate::workspace::git(source, &["config", "--get", "core.autocrlf"])
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|_| "false".into());
        let file = config.to_string_lossy().into_owned();
        for (key, value) in [("core.autocrlf", autocrlf.as_str()), ("core.filemode", "false")] {
            crate::workspace::git(&staged, &["config", "--file", &file, key, value]).map_err(io::Error::other)?;
        }
        let steps = tree::plan(&tree::scan(&staged, &[])?, &Manifest::default(), &[]);
        let sent = tree::send(&staged, &steps, dest, exchange, &mut |_| {}, cancel)?;
        match sent.problems.first() {
            Some(problem) => Err(io::Error::other(format!("{}: {}", problem.path, problem.error))),
            None => Ok(()),
        }
    })();
    let _ = fs::remove_dir_all(&staged);
    aligned
}

#[cfg(test)]
mod tests;
