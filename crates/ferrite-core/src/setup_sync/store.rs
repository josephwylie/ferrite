//! The operator's setup on one side, as [`ITEMS`] spells it: listed, read,
//! written and removed by the table's paths, each in this side's home
//! directory. A write lands in a temporary file that is synced to disk
//! before it is renamed into place, so a torn file never reads as an
//! edit. Nothing is ever written or removed through a symlink, and a
//! sign-in is only ever owner-only bytes: never parsed, logged or named
//! by its content in an error.

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

/// One side's home directory, and where its agents keep what the table
/// names.
#[derive(Clone)]
pub struct Home {
    root: PathBuf,
    /// `$CODEX_HOME`, else `.codex` in the home directory.
    codex: PathBuf,
    /// Where Claude's sign-in is kept in place of its file: macOS's.
    keychain: Option<Arc<dyn Keychain>>,
}

/// A store of secrets by service, for this computer's user.
pub trait Keychain: Send + Sync {
    fn get(&self, service: &str) -> io::Result<Option<String>>;
    fn set(&self, service: &str, secret: &str) -> io::Result<()>;
    /// Whether there was one to remove.
    fn delete(&self, service: &str) -> io::Result<bool>;
}

impl Home {
    /// This process's: the home directory every other part of Ferrite
    /// reads (`HOME`, else `USERPROFILE`), and Codex's as Codex reads it.
    /// macOS's Keychain is the account's, not the home directory's, so
    /// Claude's sign-in is the Keychain's only when this is the account's
    /// own home: a process given another HOME (a test, an isolated run)
    /// never reaches the operator's Keychain.
    pub fn from_env() -> io::Result<Self> {
        let tilde = Path::new("~");
        let root = crate::workspace::expand_home(tilde);
        if root == tilde {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no home directory: HOME is not set"));
        }
        let codex = std::env::var_os("CODEX_HOME")
            .filter(|codex| !codex.is_empty())
            .map(|codex| crate::workspace::expand_home(Path::new(&codex)));
        let home = Self::at(root, codex);
        Ok(if cfg!(target_os = "macos") && account_home().is_some_and(|account| account == home.root) {
            home.with_keychain(Arc::new(SystemKeychain))
        } else {
            home
        })
    }

    /// A home at `root` whose sign-ins are files, with Codex's at `codex`
    /// or in `root`.
    pub fn at(root: PathBuf, codex: Option<PathBuf>) -> Self {
        Self {
            codex: codex.unwrap_or_else(|| root.join(".codex")),
            root,
            keychain: None,
        }
    }

    /// Claude's sign-in kept in `keychain`, as macOS keeps it.
    pub fn with_keychain(mut self, keychain: Arc<dyn Keychain>) -> Self {
        self.keychain = Some(keychain);
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The folders a change to this side's setup is seen in, each with
    /// whether all under it is: each tree, and the folder of each file.
    pub fn watched(&self) -> Vec<(PathBuf, bool)> {
        let mut watched: Vec<(PathBuf, bool)> = ITEMS
            .iter()
            .map(|item| {
                let at = self.physical(item.path);
                match item.source {
                    Source::Tree { .. } => (at, true),
                    _ => (at.parent().map_or_else(|| self.root.clone(), Path::to_path_buf), false),
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

    /// The folder `path` is from and its names in it.
    fn split<'a>(&self, path: &'a str) -> (&Path, &'a str) {
        match path.strip_prefix(".codex/") {
            Some(rest) => (&self.codex, rest),
            None => (&self.root, path),
        }
    }

    fn physical(&self, path: &str) -> PathBuf {
        let (base, rest) = self.split(path);
        rest.split('/').fold(base.to_path_buf(), |at, part| at.join(part))
    }

    /// Whether the item, or what `rest` names in it, lies through a link:
    /// the agents' own folders may be links of their own.
    fn linked(&self, item: &Item, rest: Option<&str>) -> bool {
        let link = |at: &Path| fs::symlink_metadata(at).is_ok_and(|meta| meta.file_type().is_symlink());
        let mut at = self.physical(item.path);
        link(&at)
            || rest.into_iter().flat_map(|rest| rest.split('/')).any(|part| {
                at.push(part);
                link(&at)
            })
    }

    fn writable(&self, path: &str) -> io::Result<()> {
        let (item, rest) = item(path)?;
        if self.linked(item, rest) {
            return Err(invalid(format!(
                "{path} is reached through a link here, and Setup sync never writes through one"
            )));
        }
        Ok(())
    }

    /// What this side holds of the table, by its paths. What it could not
    /// read is a problem; what is not there is nothing.
    pub(crate) fn scan(&self) -> Manifest {
        let mut manifest = Manifest::default();
        for item in ITEMS {
            match self.list(item, &mut manifest) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    manifest.problems.insert(item.path.to_string(), error.to_string());
                }
            }
        }
        manifest
    }

    fn list(&self, item: &Item, manifest: &mut Manifest) -> io::Result<()> {
        if let Source::Tree { except } = item.source {
            let root = self.physical(item.path);
            if !fs::metadata(&root)?.is_dir() {
                return Err(io::Error::other("it is not a folder"));
            }
            let skip: Vec<String> = except.iter().map(|path| path.to_string()).collect();
            let listed = tree::scan_setup(&root, &skip, LEFT_OUT)?;
            let at = |path: String| format!("{}/{path}", item.path);
            manifest.entries.extend(listed.entries.into_iter().map(|(path, entry)| (at(path), entry)));
            manifest.problems.extend(listed.problems.into_iter().map(|(path, why)| (at(path), why)));
            manifest.links.extend(listed.links.into_iter().map(at));
        } else {
            let Some((bytes, meta)) = self.bytes(item)? else {
                return Ok(());
            };
            let mode = (item.source == Source::File).then(|| meta.as_ref().and_then(tree::mode)).flatten();
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
            Source::Tree { .. } => return Err(not_setup(item.path)),
            Source::File => read(&at)?,
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
            Source::Credential(provider) => match self.secret(provider, item.path) {
                Secret::File(at) => read(&at)?,
                Secret::Keychain(keychain) => keychain.get(CLAUDE_KEYCHAIN)?.map(|secret| (secret.into_bytes(), None)),
            },
        })
    }

    fn secret(&self, provider: Provider, path: &str) -> Secret<'_> {
        match (provider, &self.keychain) {
            (Provider::Claude, Some(keychain)) => Secret::Keychain(keychain.as_ref()),
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
            (Source::Tree { .. }, Some(rest)) => {
                let root = self.physical(item.path);
                fs::create_dir_all(&root).map_err(|error| tree::at(&root, error))?;
                let at = tree::inside(&root, rest)?;
                let (mode, new) = replaced(&at)?;
                let dir = at.parent().map(Path::to_path_buf).unwrap_or(root);
                Place { dir, mode, how: How::Rename { at, new } }
            }
            (Source::File, None) => {
                fs::create_dir_all(&parent).map_err(|error| tree::at(&parent, error))?;
                let (mode, new) = replaced(&at)?;
                Place { dir: parent, mode, how: How::Rename { at, new } }
            }
            (Source::PortableJson, None) => {
                fs::create_dir_all(&parent).map_err(|error| tree::at(&parent, error))?;
                Place { dir: parent, mode: Some(0o600), how: How::Portable(at) }
            }
            (Source::JsonKey(key), None) => Place { dir: parent, mode: Some(0o600), how: How::Key(at, key) },
            (Source::Credential(provider), None) => match self.secret(provider, path) {
                Secret::File(at) => {
                    private_dir(&parent)?;
                    Place { dir: parent, mode: Some(0o600), how: How::Secret(at) }
                }
                Secret::Keychain(_) => {
                    let dir = self.root.join(".claude");
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
                self.keychain()?.set(CLAUDE_KEYCHAIN, &secret)
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
        fs::create_dir_all(&root).map_err(|error| tree::at(&root, error))?;
        let at = tree::inside(&root, rest)?;
        match fs::create_dir(&at) {
            Ok(()) => Ok(true),
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
        self.writable(path)?;
        match (item.source, rest) {
            (Source::Tree { .. }, Some(rest)) => tree::remove_entry(&self.physical(item.path), rest),
            (Source::File | Source::PortableJson, None) => remove_file(&self.physical(path)),
            (Source::JsonKey(key), None) => {
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
            (Source::Credential(provider), None) => match self.secret(provider, path) {
                Secret::File(at) => remove_file(&at),
                Secret::Keychain(keychain) => keychain.delete(CLAUDE_KEYCHAIN),
            },
            _ => Err(not_setup(path)),
        }
    }

    fn keychain(&self) -> io::Result<&dyn Keychain> {
        self.keychain
            .as_deref()
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
    Keychain(&'a dyn Keychain),
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

/// The item `path` is, or lies in, and the rest of it inside that tree.
fn item(path: &str) -> io::Result<(&'static Item, Option<&str>)> {
    tree::check_path(path)?;
    if let Some(item) = ITEMS
        .iter()
        .find(|item| item.path == path && !matches!(item.source, Source::Tree { .. }))
    {
        return Ok((item, None));
    }
    for item in ITEMS {
        let Source::Tree { except } = item.source else {
            continue;
        };
        let Some(rest) = path.strip_prefix(item.path).and_then(|rest| rest.strip_prefix('/')) else {
            continue;
        };
        let excepted = except
            .iter()
            .any(|kept| rest == *kept || rest.starts_with(&format!("{kept}/")));
        if excepted || rest.split('/').any(|name| LEFT_OUT.contains(&name)) {
            break;
        }
        return Ok((item, Some(rest)));
    }
    Err(not_setup(path))
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

fn remove_file(at: &Path) -> io::Result<bool> {
    match fs::remove_file(at) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(tree::at(at, error)),
    }
}

/// `dir`, made if it is not there, and owner-only where bits are kept.
fn private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir).map_err(|error| tree::at(dir, error))?;
    tree::set_mode(dir, 0o700)
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

fn not_json(error: serde_json::Error) -> io::Error {
    invalid(format!("it is not JSON ({error})"))
}

fn not_setup(path: &str) -> io::Error {
    invalid(format!("{path} is not part of the setup Ferrite syncs"))
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
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

#[cfg(not(unix))]
fn account_home() -> Option<PathBuf> {
    None
}

/// macOS's login Keychain, as Claude Code keeps its sign-in there: a
/// generic password by service, for this user's account.
struct SystemKeychain;

impl SystemKeychain {
    fn entry(service: &str) -> io::Result<keyring::Entry> {
        let account = std::env::var("USER").map_err(|_| io::Error::other("USER is not set"))?;
        keyring::Entry::new(service, &account).map_err(refused)
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

    fn delete(&self, service: &str) -> io::Result<bool> {
        match Self::entry(service)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(error) => Err(refused(error)),
        }
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
