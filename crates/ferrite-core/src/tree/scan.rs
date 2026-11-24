//! A directory listed as a [`Manifest`], hashing only the files that moved.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

use super::{at, check_path, Entry, Manifest};
use crate::chunked::{self, hex};

/// A file modified this close to its listing may change again within the
/// same tick of its clock, unseen, so its hash is not kept.
pub(super) const SETTLED: Duration = Duration::from_secs(2);

/// How many files' hashes are kept, of all roots together: at most about
/// 50 MB.
const CACHED: usize = 200_000;

/// What each root's files hashed to when this process last listed it.
static HASHED: Mutex<Cache> = Mutex::new(Cache(Vec::new()));

/// `root` as a Manifest, with `skip` (paths in it) and everything under
/// them left out. Symlinks are listed and never followed; what is none of a
/// file, a directory or a symlink (a socket, a FIFO), and a write's
/// temporary file, is left out. An entry that goes while it is listed is
/// left out; one that cannot be read or named is a problem, by its path. A
/// file is read only when what its metadata says of it moved since this
/// process last listed `root`.
pub fn scan(root: &Path, skip: &[String]) -> io::Result<Manifest> {
    list(root, skip, Listing::default(), &mut |_| Ok(()))
}

/// [`scan`], `watch` told how many entries are listed as each is, and the
/// scan stopped by the error it returns.
pub fn scan_watched(
    root: &Path,
    skip: &[String],
    watch: &mut dyn FnMut(usize) -> io::Result<()>,
) -> io::Result<Manifest> {
    list(root, skip, Listing::default(), watch)
}

/// [`scan`], removing each temporary file a write that is over left behind:
/// a Server's scan of a tree it writes to.
pub(super) fn sweeping(root: &Path, skip: &[String]) -> io::Result<Manifest> {
    list(root, skip, Listing { sweep: true, ..Listing::default() }, &mut |_| Ok(()))
}

/// One tree of the operator's setup: [`sweeping`], but each symlink is
/// followed and listed as what it points to, its path kept in the
/// manifest's `links`; each file's modification time is listed; and every
/// directory called one of `left_out`, at any depth, is left out with all
/// under it. A link to a directory that holds it is a problem.
pub(crate) fn setup(root: &Path, skip: &[String], left_out: &[&str]) -> io::Result<Manifest> {
    let listing = Listing {
        sweep: true,
        follow: true,
        left_out,
    };
    list(root, skip, listing, &mut |_| Ok(()))
}

#[derive(Clone, Copy, Default)]
struct Listing<'a> {
    sweep: bool,
    follow: bool,
    left_out: &'a [&'a str],
}

fn list(
    root: &Path,
    skip: &[String],
    listing: Listing,
    watch: &mut dyn FnMut(usize) -> io::Result<()>,
) -> io::Result<Manifest> {
    for path in skip {
        check_path(path)?;
    }
    if !fs::metadata(root).map_err(|error| at(root, error))?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            format!("{} is not a directory", root.display()),
        ));
    }
    let mut walk = Walk {
        root: root.to_path_buf(),
        skip,
        listing,
        started: SystemTime::now(),
        known: lock().take(root),
        kept: BTreeMap::new(),
        manifest: Manifest::default(),
        dirs: Vec::new(),
        links_followed: BTreeSet::new(),
        watch,
        stopped: None,
    };
    let mut listed = walk.list(root, "").map_err(|error| at(root, error));
    while listed.is_ok() {
        let Some((dir, path)) = walk.dirs.pop() else {
            break;
        };
        if let Err(error) = walk.list(&dir, &path) {
            match walk.stopped.take() {
                Some(stopped) => listed = Err(stopped),
                None => walk.unlisted(&path, error),
            }
        }
    }
    if let Some(stopped) = walk.stopped.take() {
        return Err(stopped);
    }
    listed?;
    lock().keep(root.to_path_buf(), walk.kept, CACHED);
    Ok(walk.manifest)
}

/// A scan under way.
struct Walk<'a> {
    root: PathBuf,
    skip: &'a [String],
    listing: Listing<'a>,
    started: SystemTime,
    /// The hashes the last scan kept, each taken as its file is listed.
    known: BTreeMap<String, Hashed>,
    kept: BTreeMap<String, Hashed>,
    manifest: Manifest,
    /// Directories listed and not yet looked in, each with its path.
    dirs: Vec<(PathBuf, String)>,
    /// Where each directory link it followed points.
    links_followed: BTreeSet<PathBuf>,
    watch: &'a mut dyn FnMut(usize) -> io::Result<()>,
    /// Why `watch` stopped the scan.
    stopped: Option<io::Error>,
}

impl Walk<'_> {
    /// List `dir`, the directory at `prefix` (`""` the root).
    fn list(&mut self, dir: &Path, prefix: &str) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                self.manifest
                    .problems
                    .insert(join(prefix, &name.to_string_lossy()), "its name is not UTF-8".into());
                continue;
            };
            let path = join(prefix, name);
            if self.skip.contains(&path) || self.listing.left_out.contains(&name) {
                continue;
            }
            if let Err(stopped) = (self.watch)(self.manifest.entries.len()) {
                self.stopped = Some(stopped);
                return Err(io::ErrorKind::Interrupted.into());
            }
            match self.entry(dir, &entry, &path) {
                Ok(Some(listed)) => {
                    self.manifest.entries.insert(path, listed);
                }
                Ok(None) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    self.manifest.problems.insert(path, error.to_string());
                }
            }
        }
        Ok(())
    }

    /// What `entry`, at `path` in `dir`, is: `None` for what is listed as
    /// nothing.
    fn entry(&mut self, dir: &Path, entry: &fs::DirEntry, path: &str) -> io::Result<Option<Entry>> {
        let mut meta = entry.metadata()?;
        if meta.file_type().is_symlink() {
            if !self.listing.follow {
                let target = fs::read_link(entry.path())?
                    .into_os_string()
                    .into_string()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "its target is not UTF-8"))?;
                return Ok(Some(Entry::Symlink {
                    target: if cfg!(windows) { target.replace('\\', "/") } else { target },
                }));
            }
            meta = self.followed(dir, &entry.path())?;
            self.manifest.links.insert(path.to_string());
        }
        let kind = meta.file_type();
        if kind.is_dir() {
            self.dirs.push((entry.path(), path.to_string()));
            return Ok(Some(Entry::Dir));
        }
        if !kind.is_file() {
            return Ok(None);
        }
        if chunked::temporary(&entry.file_name()) {
            if self.listing.sweep && chunked::stale(&entry.file_name(), &meta) {
                let _ = fs::remove_file(entry.path());
            }
            return Ok(None);
        }
        let stamp = Stamp::of(&meta)?;
        let sha256 = match self.known.remove(path) {
            Some(hashed) if hashed.stamp == stamp => hashed.sha256,
            _ => hash(&entry.path())?,
        };
        if stamp.settled(self.started) {
            self.kept.insert(
                path.to_string(),
                Hashed {
                    stamp,
                    sha256: sha256.clone(),
                },
            );
        }
        let modified = self.listing.follow.then(|| millis(&meta)).flatten();
        Ok(Some(Entry::File {
            size: meta.len(),
            sha256,
            mode: mode(&meta),
            modified,
        }))
    }

    /// What the symlink at `link`, in `dir`, points to. One whose target
    /// is not there, holds the root or `dir`, or was followed already in
    /// this listing is refused: following it again would never end.
    fn followed(&mut self, dir: &Path, link: &Path) -> io::Result<fs::Metadata> {
        let refused = |why: &str| io::Error::other(why.to_string());
        let target = dunce::canonicalize(link).map_err(|_| refused("its target is not there"))?;
        let meta = fs::metadata(&target)?;
        if meta.is_dir() {
            let within = |outer: &Path| dunce::canonicalize(outer).is_ok_and(|outer| super::holds(&target, &outer));
            let root = self.root.clone();
            if within(dir) || within(&root) || !self.links_followed.insert(target.clone()) {
                return Err(refused("it links to a folder that holds it, or one listed already"));
            }
        }
        Ok(meta)
    }

    /// The directory at `path` could not be listed: it is left out with all
    /// under it, and is a problem unless it went.
    fn unlisted(&mut self, path: &str, error: io::Error) {
        let under = format!("{path}/");
        let end = format!("{path}0");
        let inside = (Bound::Included(under.as_str()), Bound::Excluded(end.as_str()));
        let entries = self.manifest.entries.range::<str, _>(inside).map(|(path, _)| path);
        let problems = self.manifest.problems.range::<str, _>(inside).map(|(path, _)| path);
        let gone: Vec<String> = entries.chain(problems).cloned().chain([path.to_string()]).collect();
        for gone in &gone {
            self.manifest.entries.remove(gone);
            self.manifest.problems.remove(gone);
        }
        self.dirs.retain(|(_, dir)| !dir.starts_with(&under));
        if error.kind() != io::ErrorKind::NotFound {
            self.manifest.problems.insert(path.to_string(), error.to_string());
        }
    }
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

/// What a file's metadata says of it: while all of it holds, so does the
/// file's hash.
#[derive(PartialEq, Eq)]
struct Stamp {
    size: u64,
    modified: SystemTime,
    /// On Unix, its last change of any kind: a write, a chmod or a rename
    /// over it each moves it, whatever the modification time says.
    changed: SystemTime,
    /// Which file it is: its device and inode on Unix, its creation time on
    /// Windows.
    identity: (u64, u64),
}

impl Stamp {
    #[cfg(unix)]
    fn of(meta: &fs::Metadata) -> io::Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let changed = SystemTime::UNIX_EPOCH
            + Duration::new(meta.ctime().max(0) as u64, meta.ctime_nsec() as u32);
        Ok(Self {
            size: meta.len(),
            modified: meta.modified()?,
            changed,
            identity: (meta.dev(), meta.ino()),
        })
    }

    #[cfg(windows)]
    fn of(meta: &fs::Metadata) -> io::Result<Self> {
        use std::os::windows::fs::MetadataExt;
        let modified = meta.modified()?;
        Ok(Self {
            size: meta.len(),
            modified,
            changed: modified,
            identity: (meta.creation_time(), 0),
        })
    }

    /// Whether the file last moved [`SETTLED`] or more before `started`.
    fn settled(&self, started: SystemTime) -> bool {
        self.modified
            .max(self.changed)
            .checked_add(SETTLED)
            .is_some_and(|settled| settled < started)
    }
}

struct Hashed {
    stamp: Stamp,
    sha256: String,
}

/// Each root's hashes, the root listed last at the end.
struct Cache(Vec<(PathBuf, BTreeMap<String, Hashed>)>);

impl Cache {
    fn take(&mut self, root: &Path) -> BTreeMap<String, Hashed> {
        match self.0.iter().position(|(at, _)| at == root) {
            Some(at) => self.0.remove(at).1,
            None => BTreeMap::new(),
        }
    }

    /// Keep `root`'s hashes, dropping the roots listed longest ago, and then
    /// what is past `most` of its own, until at most `most` are kept.
    fn keep(&mut self, root: PathBuf, mut hashes: BTreeMap<String, Hashed>, most: usize) {
        self.0.retain(|(at, _)| *at != root);
        while hashes.len() > most {
            hashes.pop_last();
        }
        let mut held = hashes.len() + self.0.iter().map(|(_, files)| files.len()).sum::<usize>();
        while held > most {
            held -= self.0.remove(0).1.len();
        }
        self.0.push((root, hashes));
    }
}

fn lock() -> std::sync::MutexGuard<'static, Cache> {
    HASHED.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
thread_local! {
    /// How many files this thread's scans have read.
    pub(super) static READ: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn hash(path: &Path) -> io::Result<String> {
    #[cfg(test)]
    READ.with(|read| read.set(read.get() + 1));
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        match file.read(&mut buffer)? {
            0 => return Ok(hex(&hash.finalize())),
            count => hash.update(&buffer[..count]),
        }
    }
}

pub(crate) fn millis(meta: &fs::Metadata) -> Option<u64> {
    let since = meta.modified().ok()?.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    u64::try_from(since.as_millis()).ok()
}

#[cfg(unix)]
pub(crate) fn mode(meta: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(meta.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
pub(crate) fn mode(_: &fs::Metadata) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hashes(count: usize) -> BTreeMap<String, Hashed> {
        (0..count)
            .map(|at| {
                let stamp = Stamp {
                    size: 0,
                    modified: SystemTime::UNIX_EPOCH,
                    changed: SystemTime::UNIX_EPOCH,
                    identity: (0, 0),
                };
                (format!("{at:04}"), Hashed { stamp, sha256: String::new() })
            })
            .collect()
    }

    fn held(cache: &Cache) -> Vec<(&str, usize)> {
        cache.0.iter().map(|(root, files)| (root.to_str().unwrap(), files.len())).collect()
    }

    #[test]
    fn the_hash_cache_drops_the_roots_listed_longest_ago_past_its_bound() {
        let mut cache = Cache(Vec::new());
        cache.keep("a".into(), hashes(3), 8);
        cache.keep("b".into(), hashes(4), 8);
        assert_eq!(held(&cache), [("a", 3), ("b", 4)]);
        cache.keep("c".into(), hashes(2), 8);
        assert_eq!(held(&cache), [("b", 4), ("c", 2)]);
        cache.keep("b".into(), hashes(1), 8);
        assert_eq!(held(&cache), [("c", 2), ("b", 1)]);
        cache.keep("d".into(), hashes(20), 8);
        assert_eq!(held(&cache), [("d", 8)]);
        assert_eq!(cache.take(Path::new("d")).len(), 8);
        assert!(cache.0.is_empty());
    }
}
