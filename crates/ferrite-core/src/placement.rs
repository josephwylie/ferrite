//! A Project of this computer put on a Machine (ADR 0048): the folder
//! copied there through the tree ops, and which Project there is its
//! counterpart, the one a Draft's environment chip swaps to.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use base64::Engine as _;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::chunked::{self, Cancellation, Progress};
use crate::spawn::NoConsoleWindow;
use crate::tree::{self, Entry, Exchange, Manifest, Root, Sent, Side, Stopped};

/// Where a Project of this computer was copied to on a Machine: which
/// Project there is its counterpart, whatever git says of either.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub local_root: PathBuf,
    pub remote_root: PathBuf,
}

const PLACEMENTS: &str = "placements.json";

/// The Placements on the Machine whose files are in `dir` (`machine::dir`).
pub fn placements(dir: &Path) -> Vec<Placement> {
    read_list(&dir.join(PLACEMENTS))
}

/// Keep `placement`, in place of any for the same Project of this computer.
pub fn place(dir: &Path, placement: Placement) -> io::Result<()> {
    let mut known = placements(dir);
    known.retain(|old| old.local_root != placement.local_root);
    known.push(placement);
    write_list(&dir.join(PLACEMENTS), &known)
}

/// A copy begun on a Machine and not finished: cancelled, cut off, or
/// stopped with the app. Its destination holds part of the Project, which
/// only a resume of this copy adds to. What it sent is not listed: a file
/// the copy put there holds just what the copy sends, so a resume reads
/// that off the destination (`left_there`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unfinished {
    pub local_root: PathBuf,
    /// Where it goes, as the copy gave it to the Machine's Server.
    pub remote_root: PathBuf,
}

const UNFINISHED: &str = "unfinished.json";

/// Where the unfinished copies of the Project at `local_root` go, on the
/// Machine whose files are in `dir`, the one begun last at the end.
pub fn unfinished(dir: &Path, local_root: &Path) -> Vec<PathBuf> {
    read_list::<Unfinished>(&dir.join(UNFINISHED))
        .into_iter()
        .filter(|copy| copy.local_root == local_root)
        .map(|copy| copy.remote_root)
        .collect()
}

/// The Project of this computer whose copy to `remote_root` is unfinished,
/// on the Machine whose files are in `dir`, other than `local_root`'s.
fn unfinished_of_another(dir: &Path, local_root: &Path, remote_root: &Path) -> Option<PathBuf> {
    read_list::<Unfinished>(&dir.join(UNFINISHED))
        .into_iter()
        .find(|copy| copy.remote_root == remote_root && copy.local_root != local_root)
        .map(|copy| copy.local_root)
}

/// Keep `copy`, in place of any to the same destination of the same
/// Project, last.
fn begun(dir: &Path, copy: Unfinished) -> io::Result<()> {
    let mut known: Vec<Unfinished> = read_list(&dir.join(UNFINISHED));
    known.retain(|old| !(old.local_root == copy.local_root && old.remote_root == copy.remote_root));
    known.push(copy);
    write_list(&dir.join(UNFINISHED), &known)
}

fn finished(dir: &Path, local_root: &Path, remote_root: &Path) -> io::Result<()> {
    let mut known: Vec<Unfinished> = read_list(&dir.join(UNFINISHED));
    let before = known.len();
    known.retain(|old| !(old.local_root == local_root && old.remote_root == remote_root));
    if known.len() == before {
        return Ok(());
    }
    write_list(&dir.join(UNFINISHED), &known)
}

/// What this computer keeps of a Machine whose files were in `from` goes
/// with it to `to`: its target changed, and it is the same Machine.
pub fn follow(from: &Path, to: &Path) -> io::Result<()> {
    if from == to {
        return Ok(());
    }
    for placement in placements(from) {
        place(to, placement)?;
    }
    for copy in read_list::<Unfinished>(&from.join(UNFINISHED)) {
        begun(to, copy)?;
    }
    forget(from)
}

/// What this computer keeps of a Machine whose files are in `dir` goes
/// with it: it was removed.
pub fn forget(dir: &Path) -> io::Result<()> {
    for name in [PLACEMENTS, UNFINISHED] {
        match fs::remove_file(dir.join(name)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
    }
    Ok(())
}

fn read_list<T: DeserializeOwned>(path: &Path) -> Vec<T> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_list<T: Serialize>(path: &Path, list: &[T]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    crate::file_operations::replace_file(path, &serde_json::to_vec_pretty(list)?)
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

/// The counterpart of the Project at `root` whose repository is
/// `repository`, among the Projects `there`, each by its id and root: the
/// first of the same repository, else the one a Placement says it was
/// copied to or from.
pub fn counterpart<Id: Copy + Ord>(
    root: &Path,
    repository: Option<&str>,
    across: Across<'_>,
    there: &[(Id, PathBuf)],
    repositories: &BTreeMap<Id, String>,
) -> Option<Id> {
    let by_repository = repository.and_then(|repository| {
        there
            .iter()
            .find(|(id, _)| repositories.get(id).map(String::as_str) == Some(repository))
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
            there.iter().find(|(_, root)| root == placed)
        })
        .map(|(id, _)| *id)
}

/// Why `dest`, a directory on a Machine as the operator gave it, cannot
/// take a copy: it must be absolute (`/`, or a drive's on a Windows
/// Machine) or under `~`, and not the home or a filesystem's root itself,
/// nor reached through `.` or `..`.
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
    let named = |rest: &str, name: &str| rest.split(['/', '\\']).any(|part| part == name);
    match under {
        _ if dest.is_empty() || dest == "~" => Some("Choose a folder, not the home folder"),
        None => Some("Start the path with / or ~/"),
        Some(rest) if named(rest, "..") => Some("A path through .. is not allowed"),
        Some(rest) if named(rest, ".") => Some("A folder cannot be named ."),
        Some(_) => None,
    }
}

/// Why the Project folder at `root` cannot be copied as it is: a linked
/// worktree's `.git` is a file naming its repository by a path of this
/// computer, which the copy would carry to where nothing is. It names the
/// main checkout where that file says which.
pub fn refuse_source(root: &Path) -> Option<String> {
    let pointer = fs::read_to_string(root.join(".git")).ok()?;
    Some(match main_checkout(root, &pointer) {
        Some(main) => format!("This folder is a git worktree. Copy its main checkout, {}, instead.", main.display()),
        None => "This folder is a git worktree. Copy its main checkout instead.".into(),
    })
}

/// The main checkout of the linked worktree at `root`, whose `.git` file
/// says `pointer`: the folder that holds the repository's git directory.
fn main_checkout(root: &Path, pointer: &str) -> Option<PathBuf> {
    let admin = root.join(pointer.trim().strip_prefix("gitdir:")?.trim());
    let common = admin.join(fs::read_to_string(admin.join("commondir")).ok()?.trim());
    let common = dunce::canonicalize(common).ok()?;
    if common.file_name()? != ".git" {
        return None;
    }
    common.parent().map(Path::to_path_buf)
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

/// What git ignores in the checkout at `root`, each directory as a whole,
/// none under it listed again: none outside git.
pub fn ignored(root: &Path) -> Vec<Ignored> {
    let Some(listed) = git(
        root,
        &["ls-files", "-z", "--others", "--ignored", "--exclude-standard", "--directory"],
    ) else {
        return Vec::new();
    };
    let mut ignored: Vec<Ignored> = listed
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(|path| Ignored {
            path: path.trim_end_matches('/').to_string(),
            directory: path.ends_with('/'),
        })
        .collect();
    // A folder that holds only ignored files is listed, and so is each of
    // them: unticked, they would keep its files here when it is ticked.
    let folders: BTreeSet<String> = ignored
        .iter()
        .filter(|entry| entry.directory)
        .map(|entry| entry.path.clone())
        .collect();
    ignored.retain(|entry| {
        !entry
            .path
            .match_indices('/')
            .any(|(at, _)| folders.contains(&entry.path[..at]))
    });
    ignored
}

/// The linked worktrees of the checkout at `root` that lie inside it
/// (`.worktrees/feature`), by their paths under it: each one's `.git` names
/// its repository by a path of this computer, so none is copied.
pub fn worktrees(root: &Path) -> Vec<String> {
    let Some(listed) = git(root, &["worktree", "list", "--porcelain", "-z"]) else {
        return Vec::new();
    };
    let Ok(root) = dunce::canonicalize(root) else {
        return Vec::new();
    };
    listed
        .split('\0')
        .filter_map(|line| line.strip_prefix("worktree "))
        .filter_map(|worktree| dunce::canonicalize(worktree).ok())
        .filter_map(|worktree| {
            let inside = worktree.strip_prefix(&root).ok()?;
            let parts: Vec<String> = inside
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            (!parts.is_empty()).then(|| parts.join("/"))
        })
        .collect()
}

/// `ignored` without each entry that holds one of `worktrees`, and what a
/// copy leaves out for them: those entries whole, and each worktree no
/// ignored entry holds.
pub fn without_worktrees(ignored: Vec<Ignored>, worktrees: &[String]) -> (Vec<Ignored>, Vec<String>) {
    let holds = |entry: &str, worktree: &str| {
        worktree
            .strip_prefix(entry)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    };
    let (holding, kept): (Vec<Ignored>, Vec<Ignored>) = ignored
        .into_iter()
        .partition(|entry| worktrees.iter().any(|worktree| holds(&entry.path, worktree)));
    let mut left_out: Vec<String> = holding.iter().map(|entry| entry.path.clone()).collect();
    left_out.extend(
        worktrees
            .iter()
            .filter(|worktree| !holding.iter().any(|entry| holds(&entry.path, worktree)))
            .cloned(),
    );
    (kept, left_out)
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

/// The bytes of the files under `root` but those at `skip` (paths under
/// it, as [`ignored`] and [`worktrees`] give them), links not followed.
pub fn size_but(root: &Path, skip: &[String]) -> u64 {
    let skip: BTreeSet<&str> = skip.iter().map(String::as_str).collect();
    fn walk(path: &Path, relative: &str, skip: &BTreeSet<&str>) -> u64 {
        if skip.contains(relative) {
            return 0;
        }
        let Ok(meta) = fs::symlink_metadata(path) else {
            return 0;
        };
        if !meta.is_dir() {
            return meta.len();
        }
        let Ok(entries) = fs::read_dir(path) else {
            return 0;
        };
        entries
            .flatten()
            .map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let relative = match relative {
                    "" => name,
                    folder => format!("{folder}/{name}"),
                };
                walk(&entry.path(), &relative, skip)
            })
            .sum()
    }
    walk(root, "", &skip)
}

/// What git prints for `args` in the checkout at `root`, or None when it
/// fails. Without git's optional locks, as the Engine's reads run: an agent
/// may be committing in that checkout.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .no_console_window()
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// A Project folder of this computer as a copy sends it: what git would
/// check out on a Machine. From a computer that keeps no permission bits
/// (Windows), each file gets its own ([`Checkout::list`]); a file git has
/// as LF in its index and this checkout as CRLF by git's own conversion
/// (`text`, `text=auto`, or no attribute under `core.autocrlf`) goes as LF,
/// edits and all, and the index lists it with no size, as git lists a file
/// it must read again; and the copy's `.git/config` goes without this
/// computer's line-end, file-mode and case settings. So git there reads
/// the files as unchanged and keeps the bits an agent gives them.
/// Everything else goes byte for byte.
pub struct Checkout {
    root: PathBuf,
    /// Files git has as LF in its index and made CRLF here.
    crlf: BTreeSet<String>,
    /// Files git has as executable (100755).
    executable: BTreeSet<String>,
    /// What git ignores, each directory whole.
    ignored: Vec<String>,
    /// How many bytes name an object in the repository: 20, or 32 where
    /// it names them by SHA-256.
    object_id_len: usize,
    from_windows: bool,
}

const GIT_CONFIG: &str = ".git/config";
const GIT_INDEX: &str = ".git/index";

impl Checkout {
    /// The checkout at `root`, as this computer's copies send it.
    pub fn of(root: &Path) -> Self {
        if cfg!(windows) {
            Self::from_windows(root)
        } else {
            Self::as_it_is(root)
        }
    }

    /// The checkout at `root`, sent byte for byte with its own bits.
    pub fn as_it_is(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            crlf: BTreeSet::new(),
            executable: BTreeSet::new(),
            ignored: Vec::new(),
            object_id_len: 20,
            from_windows: false,
        }
    }

    /// The checkout at `root` on a computer that keeps no permission bits
    /// and checks text out with CRLF.
    pub fn from_windows(root: &Path) -> Self {
        let lines = |args: &[&str]| git(root, args).unwrap_or_default();
        let autocrlf = lines(&["config", "--type=bool", "--get", "core.autocrlf"]).trim() == "true";
        // Git says each file's line ends and attributes: one it checks out
        // as CRLF by its own conversion, never one it keeps byte for byte.
        let crlf = lines(&["ls-files", "--eol", "-z", "--", ":(attr:!working-tree-encoding)"])
            .split('\0')
            .filter_map(|record| {
                let (info, path) = record.split_once('\t')?;
                let (ends, attr) = info.split_once("attr/")?;
                let converted = match attr.trim() {
                    "text" | "text=auto" => true,
                    "" => autocrlf,
                    _ => false,
                };
                let ends: Vec<&str> = ends.split_whitespace().collect();
                (ends == ["i/lf", "w/crlf"] && converted).then(|| path.to_string())
            })
            .collect();
        let executable = lines(&["ls-files", "--stage", "-z"])
            .split('\0')
            .filter_map(|line| line.strip_prefix("100755 ")?.split_once('\t'))
            .map(|(_, path)| path.to_string())
            .collect();
        Self {
            root: root.to_path_buf(),
            crlf,
            executable,
            ignored: ignored(root).into_iter().map(|ignored| ignored.path).collect(),
            object_id_len: match lines(&["config", "--get", "extensions.objectformat"]).trim() {
                "sha256" => 32,
                _ => 20,
            },
            from_windows: true,
        }
    }

    /// `listed`, this checkout's scan, as it lands: each file's size and
    /// hash those of what is sent, and each file without bits of its own
    /// given 0755 when git has it as executable, 0600 when git ignores it
    /// (this computer's own, as a `.env` is), else 0644.
    pub fn list(&self, listed: &mut Manifest) {
        for (path, entry) in &mut listed.entries {
            let Entry::File { size, sha256, mode, .. } = entry else {
                continue;
            };
            if self.converts(path) {
                if let Some((sent_size, sent_sha256)) = self.as_sent(path, sha256) {
                    (*size, *sha256) = (sent_size, sent_sha256);
                }
            }
            if mode.is_none() && self.from_windows {
                *mode = Some(if self.executable.contains(path) {
                    0o755
                } else if self.is_ignored(path) {
                    0o600
                } else {
                    0o644
                });
            }
        }
    }

    fn converts(&self, path: &str) -> bool {
        self.crlf.contains(path)
            || (self.from_windows && path == GIT_CONFIG)
            || (path == GIT_INDEX && !self.crlf.is_empty())
    }

    fn is_ignored(&self, path: &str) -> bool {
        self.ignored.iter().any(|ignored| {
            path.strip_prefix(ignored.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        })
    }

    /// The size and hash of `path` as it is sent, when what is here now
    /// still hashes to `listed`, its scan's hash.
    fn as_sent(&self, path: &str, listed: &str) -> Option<(u64, String)> {
        static SENT: Mutex<BTreeMap<String, (u64, String)>> = Mutex::new(BTreeMap::new());
        let key = format!("{path}\0{listed}");
        let cache = || SENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(known) = cache().get(&key) {
            return Some(known.clone());
        }
        let bytes = self.read(path).ok()?;
        if chunked::hex(&Sha256::digest(&bytes)) != listed {
            return None;
        }
        let sent = self.converted(path, bytes);
        let known = (sent.len() as u64, chunked::hex(&Sha256::digest(&sent)));
        let mut cache = cache();
        if cache.len() >= 200_000 {
            cache.clear();
        }
        cache.insert(key, known.clone());
        Some(known)
    }

    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        tree::Source::open(&self.root, path)?.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn converted(&self, path: &str, bytes: Vec<u8>) -> Vec<u8> {
        match path {
            GIT_CONFIG => without_checkout_settings(&String::from_utf8_lossy(&bytes)).into_bytes(),
            GIT_INDEX => {
                let mut index = bytes.clone();
                without_sizes(&mut index, &self.crlf, self.object_id_len).map_or(bytes, |()| index)
            }
            _ => lf(bytes),
        }
    }
}

impl tree::Source for Checkout {
    fn open(&self, path: &str) -> io::Result<Box<dyn Read + '_>> {
        if self.converts(path) {
            let bytes = self.read(path)?;
            return Ok(Box::new(io::Cursor::new(self.converted(path, bytes))));
        }
        tree::Source::open(&self.root, path)
    }
}

/// `bytes` with each CRLF made LF; a lone CR stays.
fn lf(bytes: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut bytes = bytes.into_iter().peekable();
    while let Some(byte) = bytes.next() {
        if byte != b'\r' || bytes.peek() != Some(&b'\n') {
            out.push(byte);
        }
    }
    out
}

/// Mark each of `paths` in the git index `index` as git marks a file it
/// must read again, its size 0, and sign the index again: git then
/// compares what the file holds rather than the size this computer
/// checked it out at. `hash` is how many bytes name an object. None, with
/// `index` as it may be, for an index this cannot read.
fn without_sizes(index: &mut [u8], paths: &BTreeSet<String>, hash: usize) -> Option<()> {
    const SIZE: usize = 36;
    let word = |index: &[u8], at: usize| Some(u32::from_be_bytes(index.get(at..at + 4)?.try_into().ok()?));
    let version = word(index, 4)?;
    if index.get(..4)? != b"DIRC" || !(2..=4).contains(&version) {
        return None;
    }
    let mut at = 12;
    let mut path: Vec<u8> = Vec::new();
    for _ in 0..word(index, 8)? {
        let flags = index.get(at + 40 + hash..at + 42 + hash)?;
        let name = at + 42 + hash + if flags[0] & 0x40 != 0 { 2 } else { 0 };
        let next = if version == 4 {
            let (strip, used) = varint(index.get(name..)?)?;
            let end = name + used + index.get(name + used..)?.iter().position(|&byte| byte == 0)?;
            path.truncate(path.len().checked_sub(strip)?);
            path.extend_from_slice(&index[name + used..end]);
            end + 1
        } else {
            let end = name + index.get(name..)?.iter().position(|&byte| byte == 0)?;
            path = index[name..end].to_vec();
            at + ((end - at + 8) & !7)
        };
        if std::str::from_utf8(&path).is_ok_and(|path| paths.contains(path)) {
            index.get_mut(at + SIZE..at + SIZE + 4)?.fill(0);
        }
        at = next;
    }
    let body = index.len().checked_sub(hash)?;
    if index[body..].iter().all(|&byte| byte == 0) {
        return Some(());
    }
    let sum = match hash {
        20 => sha1::Sha1::digest(&index[..body]).to_vec(),
        _ => Sha256::digest(&index[..body]).to_vec(),
    };
    index[body..].copy_from_slice(&sum);
    Some(())
}

/// A number as a git index of version 4 writes it, and how many bytes it
/// took.
fn varint(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut value = usize::from(*bytes.first()? & 0x7f);
    let mut used = 1;
    while bytes[used - 1] & 0x80 != 0 {
        value = value.checked_add(1)?.checked_mul(128)? + usize::from(*bytes.get(used)? & 0x7f);
        used += 1;
    }
    Some((value, used))
}

/// A git config without its `core.autocrlf`, `core.filemode` and
/// `core.ignorecase`: how this computer checks files out, which is not how
/// a Machine does.
fn without_checkout_settings(config: &str) -> String {
    let mut core = false;
    config
        .split_inclusive('\n')
        .filter(|line| {
            let line = line.trim();
            if let Some(header) = line.strip_prefix('[') {
                let section = header.split([']', ' ', '"']).next().unwrap_or_default();
                core = section.eq_ignore_ascii_case("core");
                return true;
            }
            let key = line.split('=').next().unwrap_or_default().trim();
            !(core && ["autocrlf", "filemode", "ignorecase"].iter().any(|setting| key.eq_ignore_ascii_case(setting)))
        })
        .collect()
}

/// A copy of the Project folder at `source` to `dest` on a Machine.
pub struct CopyJob<'a> {
    pub source: &'a Path,
    /// Where it goes, as the Machine's Server spells it.
    pub dest: &'a Path,
    /// What stays here: git-ignored entries left unticked, worktrees.
    pub skip: &'a [String],
    /// The Machine's own folder on this computer (`machine::dir`), which
    /// keeps its unfinished copies.
    pub kept: &'a Path,
}

/// How a copy stands as it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Copying {
    /// The Project's files being listed: this many so far.
    Listing(usize),
    /// Sent, the copy kept as unfinished until it ends: how far, and the
    /// file the last bytes were of.
    Sending {
        progress: Progress,
        file: Option<String>,
    },
}

#[derive(Debug)]
pub enum Copied {
    /// The destination holds what this copy did not put there: nothing was
    /// sent.
    Occupied,
    /// The destination holds the unfinished copy of another Project of this
    /// computer, the one at `of`: nothing was sent.
    AnotherCopy { of: PathBuf },
    /// What was sent, and the files there that are not as this copy sends
    /// them, left as they are: someone else's.
    Sent { sent: Sent, changed: Vec<String> },
}

/// How many entries a copy lists between two counts it reports.
const COUNTED: usize = 250;

/// Make `copy.dest` hold the Project folder `copy.source`, as git would
/// check it out there ([`Checkout`]). A destination that holds anything
/// takes a first copy of nothing. Before the first byte, the copy is kept
/// as unfinished; a later copy there resumes it, unless the destination
/// holds another repository by then. A resume sends only what is not
/// there: a file there as the copy sends it is done, and one that is not
/// is someone else's, left and listed as changed. Neither removes
/// anything. What could not be read as it was listed is sent once more,
/// and what still could not is in the answer's problems. Once done, or
/// stopped before anything landed, the copy is no longer kept as
/// unfinished.
pub fn copy(
    copy: &CopyJob,
    exchange: &mut dyn Exchange,
    progress: &mut dyn FnMut(Copying),
    cancel: &Cancellation,
) -> Result<Copied, Stopped> {
    if let Some(refusal) = refuse_source(copy.source) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, refusal).into());
    }
    let checkout = Checkout::of(copy.source);
    let resumed = unfinished(copy.kept, copy.source).contains(&copy.dest.to_path_buf());
    let first = pass(copy, &checkout, exchange, progress, cancel).map_err(|stopped| {
        if !resumed && stopped.took_nothing() {
            let _ = finished(copy.kept, copy.source, copy.dest);
        }
        stopped
    })?;
    let Copied::Sent { mut sent, mut changed } = first else {
        return Ok(first);
    };
    if sent.problems.iter().any(|problem| problem.side == Side::Here) {
        let again = pass(copy, &checkout, exchange, progress, cancel).map_err(|stopped| Stopped {
            sent: added(sent.clone(), stopped.sent),
            error: stopped.error,
        })?;
        if let Copied::Sent { sent: then, changed: now } = again {
            (sent, changed) = (added(sent, then), now);
        }
    }
    if let Err(error) = finished(copy.kept, copy.source, copy.dest) {
        return Err(Stopped { error, sent });
    }
    Ok(Copied::Sent { sent, changed })
}

/// One pass of a copy.
fn pass(
    copy: &CopyJob,
    checkout: &Checkout,
    exchange: &mut dyn Exchange,
    progress: &mut dyn FnMut(Copying),
    cancel: &Cancellation,
) -> Result<Copied, Stopped> {
    let root = Root::Path {
        path: copy.dest.to_path_buf(),
    };
    let asked = exchange.ask(tree::scan_request(&root, &[], None))?;
    let mut counted = 0;
    let mut here = tree::scan_watched(copy.source, copy.skip, &mut |listed| {
        cancel.check()?;
        if listed >= counted + COUNTED {
            counted = listed;
            progress(Copying::Listing(listed));
        }
        Ok(())
    })?;
    checkout.list(&mut here);
    let there = tree::pages(exchange, &root, &[], asked)?;
    let empty = there.entries.is_empty() && there.problems.is_empty();
    if !empty {
        let resumes = unfinished(copy.kept, copy.source).contains(&copy.dest.to_path_buf());
        if !resumes || !same_repository(copy.source, &here, &there, &root, exchange) {
            if resumes {
                finished(copy.kept, copy.source, copy.dest)?;
            }
            return Ok(match unfinished_of_another(copy.kept, copy.source, copy.dest) {
                Some(of) => Copied::AnotherCopy { of },
                None => Copied::Occupied,
            });
        }
    }
    begun(
        copy.kept,
        Unfinished {
            local_root: copy.source.to_path_buf(),
            remote_root: copy.dest.to_path_buf(),
        },
    )?;
    let (left, changed) = left_there(&here, &there);
    let held: Vec<String> = copy.skip.iter().cloned().chain(left).collect();
    let steps = tree::plan(&here, &there, &held);
    let mut sending = |sent: tree::Sending| {
        progress(Copying::Sending {
            progress: sent.progress,
            file: sent.file.map(str::to_string),
        })
    };
    let sent = tree::send(checkout, &steps, &root, exchange, &mut sending, cancel)?;
    Ok(Copied::Sent {
        sent: sent.with_problems(here, there),
        changed,
    })
}

/// What a resume leaves as the destination has it: everything there but a
/// folder the Project has too, whose entries are each looked at. Of those,
/// the files the Project holds and sends otherwise are someone else's,
/// changed there; a link there as the destination holds one is done.
fn left_there(here: &Manifest, there: &Manifest) -> (Vec<String>, Vec<String>) {
    let mut left = Vec::new();
    let mut changed = Vec::new();
    for (path, theirs) in &there.entries {
        let same = match (here.entries.get(path), theirs) {
            (Some(Entry::Dir), Entry::Dir) => continue,
            (None, _) => true,
            (Some(Entry::File { size, sha256, .. }), Entry::File { size: s, sha256: h, .. }) => size == s && sha256 == h,
            (Some(Entry::Symlink { target }), theirs) => here.holds_link(path, target, theirs),
            (Some(ours), theirs) => ours == theirs,
        };
        if !same {
            changed.push(path.clone());
        }
        left.push(path.clone());
    }
    (left, changed)
}

/// Whether the git repository the destination holds, as far as a copy
/// got, is the one at `source`: its `.git` a folder as here, and its
/// config as this copy sends it or naming the same repository. One with
/// no config yet has none to tell.
fn same_repository(
    source: &Path,
    here: &Manifest,
    there: &Manifest,
    root: &Root,
    exchange: &mut dyn Exchange,
) -> bool {
    match (here.entries.get(".git"), there.entries.get(".git")) {
        (_, None) => return true,
        (Some(Entry::Dir), Some(Entry::Dir)) => {}
        _ => return false,
    }
    let (ours, theirs) = (here.entries.get(GIT_CONFIG), there.entries.get(GIT_CONFIG));
    match (ours, theirs) {
        (_, None) => return true,
        (Some(Entry::File { sha256: ours, .. }), Some(Entry::File { sha256: theirs, .. })) if ours == theirs => {
            return true
        }
        _ => {}
    }
    let read = exchange.call(tree::Request::Read {
        root: root.clone(),
        path: GIT_CONFIG.into(),
        offset: 0,
    });
    let Ok(tree::Reply::Bytes { data, .. }) = read else {
        return false;
    };
    let Ok(config) = base64::engine::general_purpose::STANDARD.decode(data) else {
        return false;
    };
    let theirs = crate::repository::repository_key_of(&String::from_utf8_lossy(&config));
    matches!((crate::repository::repository_key(source), theirs), (Some(ours), Some(theirs)) if ours == theirs)
}

/// `first` and then `then`, whose problems and entries left unmade are the
/// ones left.
fn added(first: Sent, then: Sent) -> Sent {
    Sent {
        removed: first.removed + then.removed,
        made: first.made + then.made,
        written: first.written + then.written,
        linked: first.linked + then.linked,
        bytes: first.bytes + then.bytes,
        problems: then.problems,
        unmade: then.unmade,
    }
}

#[cfg(test)]
mod tests;
