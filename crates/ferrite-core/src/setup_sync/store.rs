//! The operator's setup on one side, as [`ITEMS`] spells it: listed, read,
//! written and removed by the table's paths, each in this side's home
//! directory. A write lands in a temporary file that is synced to disk
//! before it is renamed into place, so a torn file never reads as an
//! edit. Nothing is ever written through a symlink, and nothing removed
//! through one: a link itself may go. A sign-in is only ever owner-only
//! bytes: never logged or named by its content in an error.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::portable::{self, Spelling};
use super::{Item, Source, CLAUDE_KEYCHAIN, ITEMS, LEFT_OUT};
use crate::chunked::{self, CHUNK_SIZE};
use crate::store::Provider;
use crate::tree::{self, Entry, Manifest};

/// Why a side holds a sign-in that does not read whole: its agent is
/// writing it, and the next sync carries it, unsaid.
pub(super) const MID_WRITE: &str = "the sign-in is being written; it syncs once it reads whole";

/// One side's home directory, and where its agents keep what the table
/// names.
#[derive(Clone)]
pub struct Home {
    root: PathBuf,
    /// `$CLAUDE_CONFIG_DIR`, else `.claude` in the home directory.
    claude: PathBuf,
    /// Claude's `.claude.json`: in `$CLAUDE_CONFIG_DIR` when that is set.
    claude_json: PathBuf,
    /// `$CODEX_HOME`, else `.codex` in the home directory.
    codex: PathBuf,
    /// Where Claude's sign-in is kept in place of its file (macOS's), and
    /// the service of its item there.
    keychain: Option<(Arc<dyn Keychain>, String)>,
}

/// A store of secrets by service, for this computer's user.
pub trait Keychain: Send + Sync {
    fn get(&self, service: &str) -> io::Result<Option<String>>;
    fn set(&self, service: &str, secret: &str) -> io::Result<()>;
}

impl Home {
    /// This process's: the home directory Claude Code and Codex read, with
    /// `.claude` and `.codex` where `CLAUDE_CONFIG_DIR` and `CODEX_HOME`
    /// put them. A Ferrite on a Store of its own (`FERRITE_STORE`, a test
    /// or an isolated run) is refused unless every folder it would sync is
    /// in a home of its own, not the account's real one, so it never syncs
    /// the operator's setup. macOS's Keychain is the account's, not the
    /// home directory's, so Claude's sign-in is the Keychain's only when
    /// this is the account's own home.
    pub fn from_env() -> io::Result<Self> {
        let var = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
        let root = home_dir(var, cfg!(windows))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home directory: HOME is not set"))?;
        let account = account_home();
        let under = |dir: OsString| match Path::new(&dir).strip_prefix("~") {
            Ok(rest) => root.join(rest),
            Err(_) => PathBuf::from(dir),
        };
        let codex = var("CODEX_HOME").map(under);
        let claude = var("CLAUDE_CONFIG_DIR").map(under);
        let keychain = cfg!(target_os = "macos") && account.as_ref().is_some_and(|account| *account == root);
        let mut home = Self::at(root, codex);
        if let Some(claude) = claude {
            home = home.with_claude(claude);
        }
        if var("FERRITE_STORE").is_some() && !home.of_its_own(account.as_deref()) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Off: this Ferrite runs on a Store of its own, and its setup is not in a home of its own",
            ));
        }
        Ok(if keychain {
            let service = claude_service(var("CLAUDE_CONFIG_DIR").as_deref());
            home.with_keychain(Arc::new(SystemKeychain)).serving(service)
        } else {
            home
        })
    }

    /// A home at `root` whose sign-ins are files, with Codex's at `codex`
    /// or in `root`.
    pub fn at(root: PathBuf, codex: Option<PathBuf>) -> Self {
        Self {
            claude: root.join(".claude"),
            claude_json: root.join(".claude.json"),
            codex: codex.unwrap_or_else(|| root.join(".codex")),
            root,
            keychain: None,
        }
    }

    /// Claude's folder at `dir`, as `CLAUDE_CONFIG_DIR` puts it, its
    /// `.claude.json` in it.
    pub fn with_claude(mut self, dir: PathBuf) -> Self {
        self.claude_json = dir.join(".claude.json");
        self.claude = dir;
        self
    }

    /// Claude's sign-in kept in `keychain`, as macOS keeps it.
    pub fn with_keychain(mut self, keychain: Arc<dyn Keychain>) -> Self {
        self.keychain = Some((keychain, CLAUDE_KEYCHAIN.to_string()));
        self
    }

    fn serving(mut self, service: String) -> Self {
        if let Some((_, kept)) = &mut self.keychain {
            *kept = service;
        }
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether every folder this side syncs lies in its home, and that
    /// home is not `account`, the account's real one.
    fn of_its_own(&self, account: Option<&Path>) -> bool {
        let real = account.is_none_or(|account| same_dir(account, &self.root));
        let inside = |at: &PathBuf| tree::within(at, &self.root).unwrap_or(false);
        !real && [&self.claude, &self.claude_json, &self.codex].into_iter().all(inside)
    }

    /// The folders a change to this side's setup is seen in, each with
    /// whether all under it is: each tree, and the folder of each file.
    pub fn watched(&self) -> Vec<(PathBuf, bool)> {
        let mut watched: Vec<(PathBuf, bool)> = ITEMS
            .iter()
            .map(|item| {
                let at = self.physical(item.path);
                if folder(item) {
                    (at, true)
                } else {
                    (at.parent().map_or_else(|| self.root.clone(), Path::to_path_buf), false)
                }
            })
            .collect();
        let trees: Vec<PathBuf> = watched.iter().filter(|(_, all)| *all).map(|(at, _)| at.clone()).collect();
        watched.retain(|(at, all)| *all || !trees.contains(at));
        watched.sort();
        watched.dedup();
        watched
    }

    /// Whether `path`, of this computer, holds something the table names.
    pub fn names(&self, path: &Path) -> bool {
        ITEMS.iter().any(|item| path.starts_with(self.physical(item.path)))
    }

    fn spelling(&self) -> Spelling {
        Spelling::new(&self.root.to_string_lossy(), cfg!(windows))
    }

    fn physical(&self, path: &str) -> PathBuf {
        let (base, rest) = if path == ".claude.json" {
            return self.claude_json.clone();
        } else if let Some(rest) = path.strip_prefix(".codex/") {
            (&self.codex, rest)
        } else if let Some(rest) = path.strip_prefix(".claude/") {
            (&self.claude, rest)
        } else {
            (&self.root, path)
        };
        rest.split('/').fold(base.to_path_buf(), |at, part| at.join(part))
    }

    /// Whether the item, or a folder of `dirs` in it, is a link: the
    /// agents' own folders may be links of their own.
    fn linked(&self, item: &Item, dirs: Option<&str>) -> bool {
        let link = |at: &Path| fs::symlink_metadata(at).is_ok_and(|meta| meta.file_type().is_symlink());
        let mut at = self.physical(item.path);
        link(&at)
            || dirs.into_iter().flat_map(|dirs| dirs.split('/')).any(|part| {
                at.push(part);
                link(&at)
            })
    }

    fn writable(&self, path: &str) -> io::Result<()> {
        let (item, rest) = item(path)?;
        if self.linked(item, rest) {
            return Err(through_link(path));
        }
        Ok(())
    }

    /// A removal may take a link itself, never what lies through one.
    fn removable(&self, item: &Item, rest: Option<&str>) -> io::Result<()> {
        let through = match rest {
            Some(rest) => self.linked(item, rest.rsplit_once('/').map(|(dirs, _)| dirs)),
            None => false,
        };
        match through {
            true => Err(through_link(item.path)),
            false => Ok(()),
        }
    }

    /// What this side holds of the table, by its paths, but `skip`, which
    /// it does not read. What it could not read is a problem, and so is an
    /// item behind a link to something not there; only an item that is
    /// not there at all is nothing.
    pub(crate) fn scan(&self, skip: &[String]) -> Manifest {
        for dir in self.file_dirs() {
            sweep(&dir);
        }
        let mut manifest = Manifest::default();
        for item in ITEMS.iter().filter(|item| !skip.iter().any(|skipped| skipped == item.path)) {
            if let Err(error) = self.list(item, &mut manifest) {
                manifest.problems.insert(item.path.to_string(), error.to_string());
            }
        }
        manifest
    }

    /// The folders this side writes a file item's temporary file in.
    fn file_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = ITEMS
            .iter()
            .filter(|item| !folder(item))
            .filter_map(|item| self.physical(item.path).parent().map(Path::to_path_buf))
            .collect();
        dirs.sort();
        dirs.dedup();
        dirs
    }

    fn list(&self, item: &Item, manifest: &mut Manifest) -> io::Result<()> {
        let at = self.physical(item.path);
        if !self.in_keychain(item) && !present(&at)? {
            return Ok(());
        }
        if folder(item) {
            if !fs::metadata(&at).map_err(|error| tree::at(&at, error))?.is_dir() {
                return Err(io::Error::other("it is not a folder"));
            }
            let except = match item.source {
                Source::Tree { except } => except,
                _ => &[],
            };
            let skip: Vec<String> = except.iter().filter(|kept| !kept.contains('*')).map(|kept| kept.to_string()).collect();
            let left_out = if item.source == Source::Seed { &[][..] } else { LEFT_OUT };
            let listed = tree::scan_setup(&at, &skip, left_out)?;
            let kept = |path: &String| !excepted(except, path);
            let named = |path: String| format!("{}/{path}", item.path);
            manifest.entries.extend(listed.entries.into_iter().filter(|(path, _)| kept(path)).map(|(path, entry)| (named(path), entry)));
            manifest.problems.extend(listed.problems.into_iter().filter(|(path, _)| kept(path)).map(|(path, why)| (named(path), why)));
            manifest.links.extend(listed.links.into_iter().filter(kept).map(named));
        } else {
            let Some((bytes, meta)) = self.bytes(item)? else {
                return Ok(());
            };
            let mode = matches!(item.source, Source::File { .. }).then(|| meta.as_ref().and_then(tree::mode)).flatten();
            manifest.entries.insert(
                item.path.to_string(),
                Entry::File {
                    size: bytes.len() as u64,
                    sha256: chunked::hex(&Sha256::digest(&bytes)),
                    mode,
                    modified: meta.as_ref().and_then(tree::millis),
                },
            );
        }
        if self.linked(item, None) {
            manifest.links.insert(item.path.to_string());
        }
        Ok(())
    }

    /// An item that is not a tree, as it is carried: `None` when it is not
    /// there.
    fn bytes(&self, item: &Item) -> io::Result<Option<(Vec<u8>, Option<fs::Metadata>)>> {
        let at = self.physical(item.path);
        Ok(match item.source {
            Source::Tree { .. } | Source::Seed => return Err(not_setup(item.path)),
            Source::File { .. } => read(&at)?,
            Source::PortableJson => match read(&at)? {
                Some((raw, meta)) => {
                    serde_json::from_slice::<serde::de::IgnoredAny>(&raw).map_err(not_json)?;
                    Some((portable::portable(&raw, &self.spelling()), meta))
                }
                None => None,
            },
            Source::JsonKey(key) => match read(&at)? {
                Some((raw, meta)) => {
                    let document: serde_json::Value = serde_json::from_slice(&raw).map_err(not_json)?;
                    match document.get(key) {
                        Some(value) => {
                            let text = serde_json::to_vec_pretty(value).map_err(not_json)?;
                            Some((portable::portable(&text, &self.spelling()), meta))
                        }
                        None => None,
                    }
                }
                None => None,
            },
            Source::Credential(provider) => {
                let read = match self.secret(provider, item.path) {
                    Secret::File(at) => read(&at)?,
                    Secret::Keychain(keychain, service) => keychain.get(service)?.map(|secret| (secret.into_bytes(), None)),
                };
                if read.as_ref().is_some_and(|(bytes, _)| serde_json::from_slice::<serde::de::IgnoredAny>(bytes).is_err()) {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, MID_WRITE));
                }
                read
            }
        })
    }

    /// Whether `item` is Claude's sign-in, kept in this side's Keychain.
    fn in_keychain(&self, item: &Item) -> bool {
        matches!((item.source, &self.keychain), (Source::Credential(Provider::Claude), Some(_)))
    }

    fn secret(&self, provider: Provider, path: &str) -> Secret<'_> {
        match (provider, &self.keychain) {
            (Provider::Claude, Some((keychain, service))) => Secret::Keychain(keychain.as_ref(), service),
            _ => Secret::File(self.physical(path)),
        }
    }

    /// Up to a chunk of `path` from `offset`, as it is carried, and its
    /// whole size.
    pub(crate) fn read(&self, path: &str, offset: u64) -> io::Result<(Vec<u8>, u64)> {
        let past = || invalid(format!("{path} has no file to read there"));
        match item(path)? {
            (_, Some(_)) => {
                let at = self.physical(path);
                let meta = fs::metadata(&at).map_err(|error| tree::at(&at, error))?;
                if !meta.is_file() || offset > meta.len() {
                    return Err(past());
                }
                let mut file = fs::File::open(&at).map_err(|error| tree::at(&at, error))?;
                file.seek(SeekFrom::Start(offset))?;
                let mut bytes = Vec::with_capacity(CHUNK_SIZE);
                file.take(CHUNK_SIZE as u64).read_to_end(&mut bytes)?;
                Ok((bytes, meta.len()))
            }
            (item, None) => {
                let (bytes, _) = self.bytes(item)?.ok_or_else(past)?;
                let from = usize::try_from(offset).ok().filter(|from| *from <= bytes.len()).ok_or_else(past)?;
                let to = bytes.len().min(from + CHUNK_SIZE);
                Ok((bytes[from..to].to_vec(), bytes.len() as u64))
            }
        }
    }

    /// `carried`, the bytes of `path` as a sync carries them, as this side
    /// writes them out.
    pub(crate) fn written(&self, path: &str, carried: Vec<u8>) -> Vec<u8> {
        match item(path) {
            Ok((Item { source: Source::PortableJson | Source::JsonKey(_), .. }, None)) => {
                portable::expand(&carried, &self.spelling())
            }
            _ => carried,
        }
    }

    /// Where a write of `path` is received, the bits its temporary file is
    /// made with, and how it is put in place once whole. `sent` is the
    /// sender's permission bits, where it keeps them.
    pub(crate) fn receive(&self, path: &str, sent: Option<u32>) -> io::Result<Place> {
        let (item, rest) = item(path)?;
        self.writable(path)?;
        let at = self.physical(path);
        let parent = at.parent().map(Path::to_path_buf).unwrap_or_else(|| self.root.clone());
        let replaced = |at: &Path| -> io::Result<(Option<u32>, bool)> {
            let there = fs::symlink_metadata(at).ok();
            if there.as_ref().is_some_and(fs::Metadata::is_dir) {
                return Err(invalid(format!("{} is a directory", at.display())));
            }
            Ok((tree::permissions(there.as_ref(), sent), there.is_none()))
        };
        Ok(match (item.source, rest) {
            (Source::Tree { .. } | Source::Seed, Some(rest)) => {
                let root = self.physical(item.path);
                private_dirs(&root)?;
                let at = tree::inside(&root, rest)?;
                let (mode, new) = replaced(&at)?;
                let dir = at.parent().map(Path::to_path_buf).unwrap_or(root);
                Place { dir, mode, how: How::Rename { at, new } }
            }
            (Source::File { owner_only }, None) => {
                private_dirs(&parent)?;
                let (mode, new) = replaced(&at)?;
                let mode = if owner_only && new { Some(0o600) } else { mode };
                Place { dir: parent, mode, how: How::Rename { at, new } }
            }
            (Source::PortableJson, None) => {
                private_dirs(&parent)?;
                Place { dir: parent, mode: Some(0o600), how: How::Portable(at) }
            }
            (Source::JsonKey(key), None) => {
                private_dirs(&parent)?;
                Place { dir: parent, mode: Some(0o600), how: How::Key(at, key) }
            }
            (Source::Credential(provider), None) => match self.secret(provider, path) {
                Secret::File(at) => {
                    private_dir(&parent)?;
                    Place { dir: parent, mode: Some(0o600), how: How::Secret(at) }
                }
                Secret::Keychain(..) => {
                    let dir = self.claude.clone();
                    private_dir(&dir)?;
                    Place { dir, mode: Some(0o600), how: How::Keychain }
                }
            },
            _ => return Err(not_setup(path)),
        })
    }

    /// Put `received`, a whole file synced to disk, where `place` says.
    pub(crate) fn place(&self, place: &Place, received: &Path) -> io::Result<()> {
        match &place.how {
            How::Rename { at, new } => {
                let mode = match place.mode {
                    None if *new && script(received)? => Some(0o755),
                    mode => mode,
                };
                if let Some(mode) = mode {
                    tree::set_mode(received, mode)?;
                }
                fs::rename(received, at).map_err(|error| tree::at(at, error))
            }
            How::Portable(at) => {
                let bytes = portable::expand(&fs::read(received)?, &self.spelling());
                write_over(at, &bytes)
            }
            How::Key(at, key) => {
                let carried = portable::expand(&fs::read(received)?, &self.spelling());
                let value: serde_json::Value = serde_json::from_slice(&carried).map_err(not_json)?;
                let mut document = document(at)?;
                document.insert(key.to_string(), value);
                write_over(at, &serde_json::to_vec_pretty(&document).map_err(not_json)?)
            }
            How::Secret(at) => {
                tree::set_mode(received, 0o600)?;
                fs::rename(received, at).map_err(|error| tree::at(at, error))
            }
            How::Keychain => {
                let secret = String::from_utf8(fs::read(received)?)
                    .map_err(|_| invalid("Claude's sign-in is not text".into()))?;
                let (keychain, service) = self.keychain()?;
                keychain.set(service, &secret)
            }
        }
    }

    /// Make the folder `path` in a tree: whether it was not there.
    pub(crate) fn make_dir(&self, path: &str) -> io::Result<bool> {
        let (item, Some(rest)) = item(path)? else {
            return Err(not_setup(path));
        };
        self.writable(path)?;
        let root = self.physical(item.path);
        private_dirs(&root)?;
        let at = tree::inside(&root, rest)?;
        match fs::create_dir(&at) {
            Ok(()) => tree::set_mode(&at, DIR_MODE).map(|()| true),
            Err(error)
                if error.kind() == io::ErrorKind::AlreadyExists
                    && fs::symlink_metadata(&at).is_ok_and(|meta| meta.is_dir()) =>
            {
                Ok(false)
            }
            Err(error) => Err(tree::at(&at, error)),
        }
    }

    /// Remove `path`, and all under it: whether it was there.
    pub(crate) fn remove(&self, path: &str) -> io::Result<bool> {
        let (item, rest) = item(path)?;
        match (item.source, rest) {
            (Source::Tree { .. } | Source::Seed, Some(rest)) => {
                self.removable(item, Some(rest))?;
                tree::remove_entry(&self.physical(item.path), rest)
            }
            (Source::File { .. } | Source::PortableJson, None) => remove_file(&self.physical(path)),
            (Source::JsonKey(key), None) => {
                self.writable(path)?;
                let at = self.physical(path);
                if !at.exists() {
                    return Ok(false);
                }
                let mut document = document(&at)?;
                if document.shift_remove(key).is_none() {
                    return Ok(false);
                }
                write_over(&at, &serde_json::to_vec_pretty(&document).map_err(not_json)?)?;
                Ok(true)
            }
            (Source::Credential(_), None) => Err(kept_sign_in(path)),
            _ => Err(not_setup(path)),
        }
    }

    /// Move `path`, and all under it, to `into`, a trash folder of this
    /// computer, at the same path: whether it was there. A key of a JSON
    /// file is kept there as a file, owner-only. A sign-in is refused: no
    /// sync removes one, so none is ever in the trash.
    pub(crate) fn trash(&self, path: &str, into: &Path) -> io::Result<bool> {
        let (item, rest) = item(path)?;
        if matches!(item.source, Source::Credential(_)) {
            return Err(kept_sign_in(path));
        }
        let kept = local(into, path);
        if matches!(item.source, Source::JsonKey(_)) {
            let Some((carried, _)) = self.bytes(item)? else {
                return Ok(false);
            };
            private_dirs(kept.parent().unwrap_or(into))?;
            write_over(&kept, &self.written(path, carried))?;
            return self.remove(path);
        }
        self.removable(item, rest)?;
        let at = self.physical(path);
        match fs::symlink_metadata(&at) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            meta => meta.map_err(|error| tree::at(&at, error))?,
        };
        private_dirs(kept.parent().unwrap_or(into))?;
        moved(&at, &kept).map_err(|error| tree::at(&at, error))?;
        Ok(true)
    }

    fn keychain(&self) -> io::Result<(&dyn Keychain, &str)> {
        self.keychain
            .as_ref()
            .map(|(keychain, service)| (keychain.as_ref(), service.as_str()))
            .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "this computer keeps no Keychain"))
    }
}

/// A [`tree::send`] of setup files reads them as they are carried.
impl tree::Source for Home {
    fn open(&self, path: &str) -> io::Result<Box<dyn Read + '_>> {
        Ok(match item(path)? {
            (_, Some(_)) => Box::new(fs::File::open(self.physical(path))?),
            (item, None) => {
                let (bytes, _) = self.bytes(item)?.ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
                Box::new(io::Cursor::new(bytes))
            }
        })
    }
}

enum Secret<'a> {
    File(PathBuf),
    Keychain(&'a dyn Keychain, &'a str),
}

/// Where a write is received and how it is put in place.
pub(crate) struct Place {
    pub(crate) dir: PathBuf,
    pub(crate) mode: Option<u32>,
    how: How,
}

enum How {
    /// Renamed over `at`; `new` when nothing was there.
    Rename { at: PathBuf, new: bool },
    /// Written out at `at` in this side's home and separators.
    Portable(PathBuf),
    /// Set as the key of the JSON object at `at`, the rest of it kept.
    Key(PathBuf, &'static str),
    /// A sign-in file: owner-only, in an owner-only folder.
    Secret(PathBuf),
    Keychain,
}

fn folder(item: &Item) -> bool {
    matches!(item.source, Source::Tree { .. } | Source::Seed)
}

/// The item `path` is, or lies in, and the rest of it inside that folder:
/// of the folders it lies in, the table's deepest.
fn item(path: &str) -> io::Result<(&'static Item, Option<&str>)> {
    tree::check_path(path)?;
    if let Some(item) = ITEMS.iter().find(|item| item.path == path && !folder(item)) {
        return Ok((item, None));
    }
    let (item, rest) = ITEMS
        .iter()
        .filter(|item| folder(item))
        .filter_map(|item| Some((item, path.strip_prefix(item.path)?.strip_prefix('/')?)))
        .max_by_key(|(item, _)| item.path.len())
        .ok_or_else(|| not_setup(path))?;
    let left_out = match item.source {
        Source::Tree { except } => {
            excepted(except, rest) || rest.split('/').any(|name| LEFT_OUT.contains(&name))
        }
        _ => false,
    };
    if left_out {
        return Err(not_setup(path));
    }
    Ok((item, Some(rest)))
}

/// Whether `rest`, a path in a tree, is one of `except` or under one. An
/// `except` whose last name is `*<end>` is any name ending so.
fn excepted(except: &[&str], rest: &str) -> bool {
    except.iter().any(|kept| match kept.split_once('*') {
        Some((dir, end)) => rest
            .strip_prefix(dir)
            .and_then(|under| under.split('/').next())
            .is_some_and(|name| name.ends_with(end)),
        None => rest == *kept || rest.strip_prefix(kept).is_some_and(|under| under.starts_with('/')),
    })
}

/// Whether `at` is there. Only what is not there at all, or a folder on
/// the way to it that is not, is absent: a link on the way that points at
/// nothing is an error, since what it points at may only be unmounted.
fn present(at: &Path) -> io::Result<bool> {
    let ancestors: Vec<&Path> = at.ancestors().collect();
    for step in ancestors.into_iter().rev() {
        match fs::symlink_metadata(step) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(tree::at(step, error)),
            Ok(meta) if meta.file_type().is_symlink() => {
                if let Err(error) = fs::metadata(step) {
                    return Err(match error.kind() {
                        io::ErrorKind::NotFound if step == at => io::Error::other("it is a link to something not there"),
                        io::ErrorKind::NotFound => {
                            io::Error::other(format!("{} is a link to something not there", step.display()))
                        }
                        _ => tree::at(step, error),
                    });
                }
            }
            Ok(_) => {}
        }
    }
    Ok(true)
}

/// A file's bytes and what its metadata says, `None` when it is not there.
fn read(at: &Path) -> io::Result<Option<(Vec<u8>, Option<fs::Metadata>)>> {
    match fs::read(at) {
        Ok(bytes) => Ok(Some((bytes, fs::metadata(at).ok()))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(tree::at(at, error)),
    }
}

/// The JSON object at `at`, or a new one when there is no file.
fn document(at: &Path) -> io::Result<serde_json::Map<String, serde_json::Value>> {
    match fs::read(at) {
        Ok(raw) => match serde_json::from_slice(&raw).map_err(not_json)? {
            serde_json::Value::Object(document) => Ok(document),
            _ => Err(invalid(format!("{} is not a JSON object", at.display()))),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(serde_json::Map::new()),
        Err(error) => Err(tree::at(at, error)),
    }
}

/// `bytes` in place of the file at `at`, which keeps its permission bits,
/// through a temporary file beside it, synced before it is renamed. A new
/// one is owner-only: settings and MCP servers may hold keys.
pub(super) fn write_over(at: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = at.parent().unwrap_or(Path::new("."));
    let kept = fs::metadata(at).ok().as_ref().and_then(tree::mode);
    let (temporary, mut file) = chunked::temporary_file(dir, kept.or(Some(0o600)))?;
    let written = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        if let Some(mode) = kept {
            tree::set_mode(&temporary, mode)?;
        }
        fs::rename(&temporary, at)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written.map_err(|error| tree::at(at, error))
}

/// What each folder Setup sync makes is given, whatever the umask: the
/// setup holds keys and sign-ins.
const DIR_MODE: u32 = 0o700;

/// `dir` and each folder holding it, each one that is new made with
/// [`DIR_MODE`].
pub(super) fn private_dirs(dir: &Path) -> io::Result<()> {
    for made in dir.ancestors().collect::<Vec<_>>().into_iter().rev() {
        if made.exists() {
            continue;
        }
        match fs::create_dir(made) {
            Ok(()) => tree::set_mode(made, DIR_MODE)?,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(tree::at(made, error)),
        }
    }
    Ok(())
}

/// `path`, a setup path, under `root` as this computer spells it.
pub(super) fn local(root: &Path, path: &str) -> PathBuf {
    path.split('/').fold(root.to_path_buf(), |at, part| at.join(part))
}

/// Move `from` to `to`, a link as itself; across volumes, a copy then the
/// removal.
fn moved(from: &Path, to: &Path) -> io::Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_all(from, to)?;
    if fs::symlink_metadata(from)?.is_dir() {
        fs::remove_dir_all(from)
    } else {
        fs::remove_file(from)
    }
}

fn copy_all(from: &Path, to: &Path) -> io::Result<()> {
    if !fs::metadata(from)?.is_dir() {
        return fs::copy(from, to).map(drop);
    }
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        copy_all(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

/// Remove each temporary file in `dir` a write that is over left there.
fn sweep(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry.metadata().is_ok_and(|meta| meta.is_file() && chunked::stale(&meta));
        if stale && chunked::temporary(&entry.file_name()) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// `dir`, owner-only whether or not it was there: a sign-in's folder.
fn private_dir(dir: &Path) -> io::Result<()> {
    private_dirs(dir)?;
    tree::set_mode(dir, DIR_MODE)
}

fn remove_file(at: &Path) -> io::Result<bool> {
    match fs::remove_file(at) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(tree::at(at, error)),
    }
}

/// Whether the file begins `#!`: a new one sent from a side that keeps
/// no permission bits runs as a script here.
fn script(path: &Path) -> io::Result<bool> {
    if cfg!(not(unix)) {
        return Ok(false);
    }
    let mut start = [0; 2];
    let read = fs::File::open(path)?.read(&mut start)?;
    Ok(read == 2 && start == *b"#!")
}

fn through_link(path: &str) -> io::Error {
    invalid(format!("{path} is reached through a link here, and Setup sync never writes through one"))
}

fn not_json(error: serde_json::Error) -> io::Error {
    invalid(format!("it is not JSON ({error})"))
}

fn kept_sign_in(path: &str) -> io::Error {
    invalid(format!("{path} is a sign-in, and Setup sync never removes one"))
}

fn not_setup(path: &str) -> io::Error {
    invalid(format!("{path} is not part of the setup Ferrite syncs"))
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

/// The home directory Claude Code and Codex read, from `var`. On Windows
/// `USERPROFILE` comes first, as Node's `os.homedir()` reads it and as
/// Codex's profile folder is; `HOME` is often set there for Git's own
/// shell, and is not theirs. Elsewhere `HOME` comes first, as both read it.
fn home_dir(var: impl Fn(&str) -> Option<OsString>, windows: bool) -> Option<PathBuf> {
    let order = if windows { ["USERPROFILE", "HOME"] } else { ["HOME", "USERPROFILE"] };
    order.into_iter().find_map(|name| var(name)).map(PathBuf::from)
}

/// The Keychain service Claude Code keeps its sign-in under: with
/// `CLAUDE_CONFIG_DIR` set, the first 8 hex of that folder's SHA-256 after
/// it, as Claude Code 2.1.330 names it.
fn claude_service(config_dir: Option<&std::ffi::OsStr>) -> String {
    match config_dir {
        Some(dir) => {
            let hash = chunked::hex(&Sha256::digest(dir.to_string_lossy().as_bytes()));
            format!("{CLAUDE_KEYCHAIN}-{}", &hash[..8])
        }
        None => CLAUDE_KEYCHAIN.to_string(),
    }
}

/// Whether two paths name one folder, as this computer compares names.
fn same_dir(one: &Path, other: &Path) -> bool {
    let canonical = |path: &Path| dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let (one, other) = (canonical(one), canonical(other));
    if cfg!(windows) {
        one.to_string_lossy().to_lowercase() == other.to_string_lossy().to_lowercase()
    } else {
        one == other
    }
}

/// The home directory the user database gives this process's account.
#[cfg(unix)]
fn account_home() -> Option<PathBuf> {
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;
    // SAFETY: getpwuid_r writes the entry's strings into `buffer`, which
    // outlives every read of them here; a null `found` means no entry.
    unsafe {
        let mut entry: libc::passwd = std::mem::zeroed();
        let mut buffer = vec![0 as libc::c_char; 16 * 1024];
        let mut found = std::ptr::null_mut();
        let status = libc::getpwuid_r(libc::getuid(), &mut entry, buffer.as_mut_ptr(), buffer.len(), &mut found);
        if status != 0 || found.is_null() || entry.pw_dir.is_null() {
            return None;
        }
        Some(PathBuf::from(OsStr::from_bytes(CStr::from_ptr(entry.pw_dir).to_bytes())))
    }
}

/// The profile folder Windows keeps for this process's account, whatever
/// `USERPROFILE` and `HOME` say.
#[cfg(windows)]
fn account_home() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::TOKEN_QUERY;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    #[link(name = "userenv")]
    extern "system" {
        fn GetUserProfileDirectoryW(token: HANDLE, dir: *mut u16, size: *mut u32) -> i32;
    }
    // SAFETY: the token is this process's own, closed once read; the
    // buffer is as long as the first call said the folder is.
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut size = 0u32;
        GetUserProfileDirectoryW(token, std::ptr::null_mut(), &mut size);
        let mut buffer = vec![0u16; size as usize];
        let read = size > 0 && GetUserProfileDirectoryW(token, buffer.as_mut_ptr(), &mut size) != 0;
        CloseHandle(token);
        read.then(|| {
            let end = buffer.iter().position(|unit| *unit == 0).unwrap_or(buffer.len());
            PathBuf::from(OsString::from_wide(&buffer[..end]))
        })
    }
}

#[cfg(not(any(unix, windows)))]
fn account_home() -> Option<PathBuf> {
    None
}

/// macOS's login Keychain, as Claude Code keeps its sign-in there: a
/// generic password by service, for the account Claude Code 2.1.330 names
/// every item with.
struct SystemKeychain;

impl SystemKeychain {
    fn entry(service: &str) -> io::Result<keyring::Entry> {
        keyring::Entry::new(service, "claude-code-user").map_err(refused)
    }
}

impl Keychain for SystemKeychain {
    fn get(&self, service: &str) -> io::Result<Option<String>> {
        match Self::entry(service)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(refused(error)),
        }
    }

    fn set(&self, service: &str, secret: &str) -> io::Result<()> {
        Self::entry(service)?.set_password(secret).map_err(refused)
    }
}

/// What the Keychain said, in words of Ferrite's own: none of its errors
/// is shown, in case one carries what it holds.
fn refused(error: keyring::Error) -> io::Error {
    io::Error::other(match error {
        keyring::Error::NoStorageAccess(_) => "the Keychain is locked, or did not let Ferrite in",
        keyring::Error::PlatformFailure(_) => "the Keychain failed",
        _ => "the Keychain refused the sign-in",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_reads_the_home_from_userprofile_before_home() {
        let vars = |name: &str| match name {
            "HOME" => Some(OsString::from("/c/Users/git-bash")),
            "USERPROFILE" => Some(OsString::from(r"C:\Users\josep")),
            _ => None,
        };
        assert_eq!(home_dir(vars, true), Some(PathBuf::from(r"C:\Users\josep")));
        assert_eq!(home_dir(vars, false), Some(PathBuf::from("/c/Users/git-bash")));
        let only_home = |name: &str| (name == "HOME").then(|| OsString::from("/home/me"));
        assert_eq!(home_dir(only_home, true), Some(PathBuf::from("/home/me")));
    }

    /// Each test runs in a process of its own under nextest, so the
    /// environment is this test's.
    #[test]
    fn a_ferrite_on_its_own_store_in_the_accounts_real_home_never_syncs_it() {
        let real = account_home().expect("the account's home");
        let own = std::env::temp_dir().join("ferrite-setup-sync-guard");
        fs::create_dir_all(&own).unwrap();
        for name in ["CODEX_HOME", "CLAUDE_CONFIG_DIR"] {
            std::env::remove_var(name);
        }
        let at = |home: &Path| {
            std::env::set_var("HOME", home);
            std::env::set_var("USERPROFILE", home);
        };
        let refused = || {
            let refused = Home::from_env().err().expect("refused");
            assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
            assert!(refused.to_string().starts_with("Off: "), "{refused}");
        };
        std::env::set_var("FERRITE_STORE", own.join("store"));
        at(&real);
        refused();
        at(&own);
        assert_eq!(Home::from_env().unwrap().root(), own);
        for (name, folder) in [("CODEX_HOME", ".codex"), ("CLAUDE_CONFIG_DIR", ".claude")] {
            std::env::set_var(name, real.join(folder));
            refused();
            std::env::set_var(name, format!("~/{folder}-elsewhere"));
            assert!(Home::from_env().is_ok(), "{name} in its own home");
            std::env::remove_var(name);
        }
        std::env::remove_var("FERRITE_STORE");
        at(&real);
        assert!(Home::from_env().is_ok(), "the operator's own Ferrite syncs the operator's home");
    }

    #[test]
    fn a_moved_claude_folder_has_its_own_keychain_service() {
        assert_eq!(claude_service(None), "Claude Code-credentials");
        let moved = claude_service(Some(std::ffi::OsStr::new("/home/me/claude-work")));
        assert!(moved.starts_with("Claude Code-credentials-") && moved.len() == "Claude Code-credentials-".len() + 8, "{moved}");
    }

    #[test]
    fn a_lock_in_the_plugin_store_and_a_marketplace_are_not_the_plugin_tree() {
        let except = &["marketplaces", "store/*.lock"];
        assert!(excepted(except, "store/a.lock"));
        assert!(excepted(except, "store/a.lock/inner"));
        assert!(excepted(except, "marketplaces/m/README.md"));
        assert!(!excepted(except, "store/a.json"));
        assert!(!excepted(except, "cache/yarn.lock"));
        assert!(!excepted(except, "marketplaces-old/x"));
        assert!(matches!(item(".claude/plugins/marketplaces/m/.git/HEAD"), Ok((Item { source: Source::Seed, .. }, Some("m/.git/HEAD")))));
        assert!(item(".claude/plugins/store/a.lock").is_err());
        assert!(item(".claude/plugins/cache/m/.git/HEAD").is_err());
    }
}
