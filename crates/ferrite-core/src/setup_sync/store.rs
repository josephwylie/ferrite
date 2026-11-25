//! The operator's setup on one side, as [`ITEMS`] spells it: listed, read,
//! written and removed by the table's paths, each in this side's home
//! directory. A write lands in a temporary file that is synced to disk
//! before it is renamed into place, so a torn file never reads as an
//! edit. A chosen key is written into its file with every other byte of it
//! kept. Nothing is ever written through a symlink, and nothing removed
//! through one: a link itself may go. A link to a place the table names
//! is listed as that link, and made again on the other side. Nothing a
//! sync removes or writes over is deleted: each side moves it to its own
//! trash first. A sign-in is only ever owner-only bytes: never logged or
//! named by its content in an error, and never put in the trash.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::installs::{Folders, Installs};
use super::links::{self, Places};
use super::lock::{Held, Lock, LOCK};
use super::portable::{self, Spelling};
use super::{keys, Format, Item, Key, Source, CLAUDE_KEYCHAIN, ITEMS, LEFT_OUT};
use crate::chunked::{self, CHUNK_SIZE};
use crate::provider::Provider;
use crate::tree::{self, Entry, Manifest};

/// Why a side holds a sign-in that does not read whole: its agent is
/// writing it, and the next sync carries it, unsaid.
pub(super) const MID_WRITE: &str = "the sign-in is being written; it syncs once it reads whole";

/// Why a side holds a chosen key, unsaid: it is this computer's own, a
/// value that names a place no other side has, or an entry the table says
/// does not travel.
pub(super) const OWN: &str = "it names a place of this computer's own";

/// How long a scan waits for the plugins a sync turned on to install, so
/// it can say which did not.
const INSTALLING: Duration = Duration::from_secs(120);

/// Where what a sync removes or writes over is moved, in Ferrite's
/// settings folder, a folder for each sync.
pub const TRASH: &str = "sync-trash";
/// How long a sync's trash folder is kept.
const KEPT: Duration = Duration::from_secs(14 * 24 * 60 * 60);
/// How a sync's folder of conflicts or trash is named: its time.
pub(super) const STAMP: &str = "%Y%m%d-%H%M%S-%3f";

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
    /// What installs the plugins a sync turns on here.
    installs: Option<Arc<Installs>>,
    /// Where what a sync removes or writes over here goes.
    trash: Arc<Trash>,
    /// Held by whatever writes this side's setup, one at a time.
    lock: Arc<Lock>,
}

/// A side's trash: what a sync removes or writes over there, in a folder
/// for each sync named by its time, each kept [`KEPT`].
struct Trash {
    root: PathBuf,
    /// This sync's folder, once it has one.
    folder: Mutex<Option<PathBuf>>,
}

impl Trash {
    fn at(root: PathBuf) -> Arc<Self> {
        Arc::new(Self { root, folder: Mutex::new(None) })
    }

    /// A sync begins: each folder older than [`KEPT`] goes, and what this
    /// one moves goes in a folder of its own.
    fn begin(&self) {
        prune(&self.root);
        *self.lock() = None;
    }

    /// This sync's folder.
    fn folder(&self) -> PathBuf {
        let stamp = || self.root.join(crate::clock::now().format(STAMP).to_string());
        self.lock().get_or_insert_with(stamp).clone()
    }

    /// This sync's folder, once something was moved there.
    fn used(&self) -> Option<PathBuf> {
        self.lock().clone().filter(|folder| folder.exists())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<PathBuf>> {
        self.folder.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Remove each sync's trash folder older than [`KEPT`].
fn prune(trash: &Path) {
    let Ok(folders) = fs::read_dir(trash) else {
        return;
    };
    let now = crate::clock::now().naive_local();
    for folder in folders.flatten() {
        let name = folder.file_name();
        let when = name.to_str().and_then(|name| chrono::NaiveDateTime::parse_from_str(name, STAMP).ok());
        if when.is_some_and(|when| (now - when).to_std().is_ok_and(|age| age > KEPT)) {
            let _ = fs::remove_dir_all(folder.path());
        }
    }
}

/// A store of secrets by service, for this computer's user.
pub trait Keychain: Send + Sync {
    fn get(&self, service: &str) -> io::Result<Option<String>>;
    fn set(&self, service: &str, secret: &str) -> io::Result<()>;
}

impl Home {
    /// This process's: the home directory Claude Code and Codex read, with
    /// `.claude` and `.codex` where `CLAUDE_CONFIG_DIR` and `CODEX_HOME`
    /// put them, and nothing to install plugins with (`with_installs`). A
    /// Ferrite on a Store of its own (`FERRITE_STORE`, a test or an
    /// isolated run) is refused unless every folder it would sync is in a
    /// home of its own, not the account's real one, so it never syncs the
    /// operator's setup. macOS's Keychain is the account's, not the home
    /// directory's, so Claude's sign-in is the Keychain's only when this is
    /// the account's own home.
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
        let mut home = Self::at(root, codex).with_settings_in(&crate::server::settings_dir());
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
    /// or in `root`, nothing to install plugins with, and its trash and
    /// lock in `.ferrite` there, as a Ferrite of that home keeps them.
    pub fn at(root: PathBuf, codex: Option<PathBuf>) -> Self {
        let settings = root.join(".ferrite");
        Self {
            claude: root.join(".claude"),
            claude_json: root.join(".claude.json"),
            codex: codex.unwrap_or_else(|| root.join(".codex")),
            trash: Trash::at(settings.join(TRASH)),
            lock: Lock::at(settings.join(LOCK)),
            root,
            keychain: None,
            installs: None,
        }
    }

    /// This side's Ferrite settings folder at `settings`: what a sync
    /// removes or writes over here is kept in its trash, and the home is
    /// held by its lock. A view of the home of its own: a sync through it
    /// holds the home for its own writes alone.
    pub fn with_settings_in(mut self, settings: &Path) -> Self {
        self.trash = Trash::at(settings.join(TRASH));
        self.lock = Lock::at(settings.join(LOCK));
        self
    }

    /// This home held for one sync through this view of it, from its first
    /// scan to its base: `None` while another sync, an install or a write
    /// holds it, in this process or another.
    pub(super) fn hold(&self) -> io::Result<Option<Held>> {
        self.lock.sync()
    }

    /// Where the last sync moved what it removed or wrote over here, when
    /// it moved anything.
    pub(super) fn trashed(&self) -> Option<PathBuf> {
        self.trash.used()
    }

    /// Where this side's tools keep what they install, and what holds it.
    pub(super) fn folders(&self) -> Folders {
        Folders { claude: self.claude.clone(), codex: self.codex.clone(), lock: self.lock.clone() }
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

    /// Plugins a sync turns on in Claude here installed by `installs`.
    pub fn with_installs(mut self, installs: Arc<Installs>) -> Self {
        self.installs = Some(installs);
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

    /// Where the setup path `path` is on this computer: a key's file.
    fn physical(&self, path: &str) -> PathBuf {
        let path = super::file_of(path);
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
        let (item, rest) = match item(path)? {
            At::In(item, rest) => (item, rest.rsplit_once('/').map(|(dirs, _)| dirs)),
            at => (at.item(), None),
        };
        if self.linked(item, rest) {
            return Err(through_link(path));
        }
        Ok(())
    }

    /// A removal may take a link itself, never what lies through one.
    fn removable(&self, item: &Item, rest: &str) -> io::Result<()> {
        match self.linked(item, rest.rsplit_once('/').map(|(dirs, _)| dirs)) {
            true => Err(through_link(item.path)),
            false => Ok(()),
        }
    }

    /// What this side holds of the table, by its paths, but `skip`, which
    /// it does not read. What it could not read is a problem, and so is an
    /// item behind a link to something not there; only an item that is
    /// not there at all is nothing. Each plugin a sync turned on here that
    /// could not be installed is a problem too, once the installs under way
    /// are over. A scan begins a sync here: what it moves aside goes in a
    /// trash folder of its own.
    pub(crate) fn scan(&self, skip: &[String]) -> Manifest {
        self.trash.begin();
        for dir in self.file_dirs() {
            sweep(&dir);
        }
        let places = self.places();
        let mut manifest = Manifest::default();
        for item in ITEMS.iter().filter(|item| !skip.iter().any(|skipped| skipped == item.path)) {
            if let Err(error) = self.list(item, &places, &mut manifest) {
                manifest.problems.insert(item.path.to_string(), error.to_string());
            }
        }
        if let Some(installs) = &self.installs {
            manifest.problems.extend(installs.settled(&self.folders(), INSTALLING));
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

    fn list(&self, item: &'static Item, places: &Places, manifest: &mut Manifest) -> io::Result<()> {
        let at = self.physical(item.path);
        if !self.in_keychain(item) && !present(&at)? {
            return Ok(());
        }
        match item.source {
            Source::Tree { except } => {
                if !fs::metadata(&at).map_err(|error| tree::at(&at, error))?.is_dir() {
                    return Err(io::Error::other("it is not a folder"));
                }
                let skip: Vec<String> = except.iter().filter(|kept| !kept.contains('*')).map(|kept| kept.to_string()).collect();
                let target = |link: &Path| {
                    let rest = links::slashed(link.strip_prefix(&at).ok()?)?;
                    places.target(link, &format!("{}/{rest}", item.path))
                };
                let listed = tree::scan_setup(&at, &skip, LEFT_OUT, &target)?;
                let kept = |path: &String| !excepted(except, path);
                let named = |path: String| format!("{}/{path}", item.path);
                manifest.entries.extend(listed.entries.into_iter().filter(|(path, _)| kept(path)).map(|(path, entry)| (named(path), entry)));
                manifest.problems.extend(listed.problems.into_iter().filter(|(path, _)| kept(path)).map(|(path, why)| (named(path), why)));
                manifest.links.extend(listed.links.into_iter().filter(kept).map(named));
            }
            Source::Keys { format, keys } => {
                let Some((raw, meta)) = read(&at)? else {
                    return Ok(());
                };
                let modified = meta.as_ref().and_then(tree::millis);
                for listed in keys::list(format, keys, &raw, &self.spelling())? {
                    let path = format!("{}#{}", item.path, listed.key);
                    if listed.own {
                        manifest.problems.insert(path, OWN.into());
                        continue;
                    }
                    manifest.entries.insert(path, file(&listed.carried, None, modified, false));
                }
            }
            Source::File { .. } | Source::Credential(_) => {
                let Some((bytes, meta)) = self.bytes(At::Whole(item))? else {
                    return Ok(());
                };
                let text = matches!(item.source, Source::File { .. });
                let mode = text.then(|| meta.as_ref().and_then(tree::mode)).flatten();
                let modified = meta.as_ref().and_then(tree::millis);
                manifest.entries.insert(item.path.to_string(), file(&bytes, mode, modified, text));
            }
        }
        if self.linked(item, None) {
            manifest.links.insert(item.path.to_string());
        }
        Ok(())
    }

    /// Where each tree a link may point into is on this computer.
    fn places(&self) -> Places {
        let places = ITEMS
            .iter()
            .filter(|item| matches!(item.source, Source::Tree { .. }))
            .filter(|item| !self.linked(item, None))
            .map(|item| (links::resolved(&self.physical(item.path)), *item))
            .collect();
        Places(places)
    }

    /// An entry that is not in a tree, as it is carried: `None` when it is
    /// not there.
    fn bytes(&self, at: At) -> io::Result<Option<(Vec<u8>, Option<fs::Metadata>)>> {
        let item = at.item();
        let file = self.physical(item.path);
        Ok(match (item.source, at) {
            (Source::File { .. }, At::Whole(_)) => read(&file)?,
            (Source::Keys { format, keys }, At::Key(_, key)) => match read(&file)? {
                Some((raw, meta)) => keys::list(format, keys, &raw, &self.spelling())?
                    .into_iter()
                    .find(|listed| listed.key == key && !listed.own)
                    .map(|listed| (listed.carried, meta)),
                None => None,
            },
            (Source::Credential(provider), At::Whole(_)) => {
                let read = match self.secret(provider, item.path) {
                    Secret::File(at) => read(&at)?,
                    Secret::Keychain(keychain, service) => keychain.get(service)?.map(|secret| (secret.into_bytes(), None)),
                };
                if read.as_ref().is_some_and(|(bytes, _)| serde_json::from_slice::<serde::de::IgnoredAny>(bytes).is_err()) {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, MID_WRITE));
                }
                read
            }
            _ => return Err(not_setup(item.path)),
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
            At::In(..) => {
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
            at => {
                let (bytes, _) = self.bytes(at)?.ok_or_else(past)?;
                let from = usize::try_from(offset).ok().filter(|from| *from <= bytes.len()).ok_or_else(past)?;
                let to = bytes.len().min(from + CHUNK_SIZE);
                Ok((bytes[from..to].to_vec(), bytes.len() as u64))
            }
        }
    }

    /// `carried`, the bytes of `path` as a sync carries them, as this side
    /// would write them out on their own: a key's value in this side's
    /// home and separators.
    pub(crate) fn written(&self, path: &str, carried: Vec<u8>) -> Vec<u8> {
        match item(path) {
            Ok(At::Key(..)) => portable::expand(&carried, &self.spelling()),
            _ => carried,
        }
    }

    /// Where a write of `path` is received, the bits its temporary file is
    /// made with, and how it is put in place once whole. `sent` is the
    /// sender's permission bits, where it keeps them.
    pub(crate) fn receive(&self, path: &str, sent: Option<u32>) -> io::Result<Place> {
        let at = item(path)?;
        self.writable(path)?;
        let file = self.physical(path);
        let parent = file.parent().map(Path::to_path_buf).unwrap_or_else(|| self.root.clone());
        let replaced = |at: &Path| -> io::Result<(Option<u32>, bool)> {
            let there = fs::symlink_metadata(at).ok();
            if there.as_ref().is_some_and(fs::Metadata::is_dir) {
                return Err(invalid(format!("{} is a directory", at.display())));
            }
            Ok((tree::permissions(there.as_ref(), sent), there.is_none()))
        };
        let placed = |dir: PathBuf, mode: Option<u32>, how: How| Place { path: path.to_string(), dir, mode, how };
        Ok(match (at.item().source, at) {
            (Source::Tree { .. }, At::In(item, rest)) => {
                let root = self.physical(item.path);
                private_dirs(&root)?;
                let at = tree::inside(&root, rest)?;
                let (mode, new) = replaced(&at)?;
                let dir = at.parent().map(Path::to_path_buf).unwrap_or(root);
                placed(dir, mode, How::Rename { at, new })
            }
            (Source::File { owner_only }, At::Whole(_)) => {
                private_dirs(&parent)?;
                let (mode, new) = replaced(&file)?;
                let mode = if owner_only && new { Some(0o600) } else { mode };
                placed(parent, mode, How::Rename { at: file, new })
            }
            (Source::Keys { format, keys }, At::Key(_, key)) => {
                private_dirs(&parent)?;
                placed(parent, Some(0o600), How::Key { at: file, format, keys, key: key.to_string() })
            }
            (Source::Credential(provider), At::Whole(_)) => match self.secret(provider, path) {
                Secret::File(at) => {
                    private_dir(&parent)?;
                    placed(parent, Some(0o600), How::Secret(at))
                }
                Secret::Keychain(..) => {
                    let dir = self.claude.clone();
                    private_dir(&dir)?;
                    placed(dir, Some(0o600), How::Keychain)
                }
            },
            _ => return Err(not_setup(path)),
        })
    }

    /// Put `received`, a whole file synced to disk, where `place` says,
    /// what it writes over copied to this side's trash first.
    pub(crate) fn place(&self, place: &Place, received: &Path) -> io::Result<()> {
        let _held = self.lock.write()?;
        match &place.how {
            How::Rename { at, new } => {
                let mode = match place.mode {
                    None if *new && script(received)? => Some(0o755),
                    mode => mode,
                };
                if let Some(mode) = mode {
                    tree::set_mode(received, mode)?;
                }
                self.copy_aside(&place.path, at)?;
                fs::rename(received, at).map_err(|error| tree::at(at, error))
            }
            How::Key { at, format, keys, key } => {
                let raw = read(at)?.map(|(raw, _)| raw);
                let carried = fs::read(received)?;
                let written = keys::set(*format, keys, raw.as_deref(), key, &carried, &self.spelling())
                    .map_err(|error| tree::at(at, error))?;
                self.key_aside(&place.path)?;
                write_over(at, &written)?;
                self.installing(at, key);
                Ok(())
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

    /// A key at `key` of the file at `at` was written or removed: when it
    /// turns a plugin or a marketplace on or off, install what is on.
    fn installing(&self, at: &Path, key: &str) {
        let claude = *at == self.claude.join("settings.json")
            && (key.starts_with("enabledPlugins/") || key.starts_with("extraKnownMarketplaces/"));
        let codex = *at == self.codex.join("config.toml") && (key.starts_with("plugins/") || key.starts_with("marketplaces/"));
        if let Some(installs) = self.installs.as_ref().filter(|_| claude || codex) {
            installs.changed();
        }
    }

    /// Make the folder `path` in a tree: whether it was not there.
    pub(crate) fn make_dir(&self, path: &str) -> io::Result<bool> {
        let At::In(item, rest) = item(path)? else {
            return Err(not_setup(path));
        };
        self.writable(path)?;
        let _held = self.lock.write()?;
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

    /// Make `path`, in a tree, a link to `target`, the path from its folder
    /// to a folder in the table, as its sender listed it: whether it was not
    /// so already. The link is made to where that folder is here, from the
    /// link's folder, as a tree makes one: a junction on Windows. A link
    /// there to anywhere else goes to the trash first, as anything a write
    /// takes the place of does.
    pub(crate) fn link(&self, path: &str, target: &str) -> io::Result<bool> {
        let At::In(item, rest) = item(path)? else {
            return Err(not_setup(path));
        };
        self.writable(path)?;
        let to = links::joined(path, target).ok_or_else(|| invalid(format!("{target} is not a place Setup sync makes a link to")))?;
        let At::In(..) = self::item(&to)? else {
            return Err(not_setup(&to));
        };
        let _held = self.lock.write()?;
        let root = self.physical(item.path);
        private_dirs(&root)?;
        let at = links::unfollowed(&tree::inside(&root, rest)?);
        let linked = fs::symlink_metadata(&at).is_ok_and(|meta| meta.file_type().is_symlink());
        if linked && self.places().target(&at, path).as_deref() == Some(target) {
            return Ok(false);
        }
        let place = links::resolved(&self.physical(&to));
        if !place.is_dir() {
            return Err(tree::at(&place, io::Error::other("there is no folder there to link to")));
        }
        let (under, spelled) = links::relative(at.parent().unwrap_or(&root), &place)
            .ok_or_else(|| tree::at(&place, io::Error::other("it is on another drive than the link")))?;
        if linked {
            self.copy_aside(path, &at)?;
        }
        tree::make_link(&under, &at, &spelled).map_err(|why| tree::at(&at, unmade(why)))
    }

    /// Remove `path`, and all under it, by moving it to this side's trash
    /// at its [`visible`] path: whether it was there. A key is kept there
    /// as a file of its value, owner-only, then taken out of its file. A
    /// sign-in is refused: no sync removes one, so none is ever in the
    /// trash.
    pub(crate) fn remove(&self, path: &str) -> io::Result<bool> {
        let _held = self.lock.write()?;
        match item(path)? {
            At::Whole(Item { source: Source::Credential(_), .. }) => Err(kept_sign_in(path)),
            At::Key(Item { source: Source::Keys { format, keys }, .. }, key) => {
                self.writable(path)?;
                let at = self.physical(path);
                let Some((raw, _)) = read(&at)? else {
                    return Ok(false);
                };
                let Some(written) = keys::remove(*format, keys, &raw, key).map_err(|error| tree::at(&at, error))? else {
                    return Ok(false);
                };
                self.key_aside(path)?;
                write_over(&at, &written)?;
                self.installing(&at, key);
                Ok(true)
            }
            At::Key(..) => Err(not_setup(path)),
            At::In(item, rest) => {
                self.removable(item, rest)?;
                self.move_aside(path)
            }
            At::Whole(_) => self.move_aside(path),
        }
    }

    /// Where `path` is kept in this sync's trash, its folder made.
    fn aside(&self, path: &str) -> io::Result<PathBuf> {
        let folder = self.trash.folder();
        let kept = visible(&folder, path);
        private_dirs(kept.parent().unwrap_or(&folder))?;
        Ok(kept)
    }

    /// Move the file, folder or link at `path` to this sync's trash:
    /// whether it was there.
    fn move_aside(&self, path: &str) -> io::Result<bool> {
        let file = self.physical(path);
        match fs::symlink_metadata(&file) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            meta => meta.map_err(|error| tree::at(&file, error))?,
        };
        moved(&file, &self.aside(path)?).map_err(|error| tree::at(&file, error))?;
        Ok(true)
    }

    /// Copy the file at `at`, the setup path `path`, to this sync's trash
    /// before a write takes its place; a link is moved there whole.
    fn copy_aside(&self, path: &str, at: &Path) -> io::Result<()> {
        let meta = match fs::symlink_metadata(at) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            meta => meta.map_err(|error| tree::at(at, error))?,
        };
        let kept = self.aside(path)?;
        let copied = if meta.file_type().is_symlink() { moved(at, &kept) } else { fs::copy(at, &kept).map(drop) };
        copied.map_err(|error| tree::at(at, error))
    }

    /// Keep the value the key `path` holds here, when it holds one, in
    /// this sync's trash as a file of it, before it is written or removed.
    fn key_aside(&self, path: &str) -> io::Result<()> {
        let Some((carried, _)) = self.bytes(item(path)?)? else {
            return Ok(());
        };
        write_over(&self.aside(path)?, &self.written(path, carried))
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
            At::In(..) => Box::new(fs::File::open(self.physical(path))?),
            at => {
                let (bytes, _) = self.bytes(at)?.ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
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
    /// The setup path it writes.
    path: String,
    pub(crate) dir: PathBuf,
    pub(crate) mode: Option<u32>,
    how: How,
}

enum How {
    /// Renamed over `at`; `new` when nothing was there.
    Rename { at: PathBuf, new: bool },
    /// Set as the key at `key` of the file at `at`, the rest of it kept.
    Key {
        at: PathBuf,
        format: Format,
        keys: &'static [Key],
        key: String,
    },
    /// A sign-in file: owner-only, in an owner-only folder.
    Secret(PathBuf),
    Keychain,
}

/// What a setup path names.
#[derive(Clone, Copy)]
enum At<'a> {
    /// An item that is one entry: a file, or a sign-in.
    Whole(&'static Item),
    /// A path in a tree, and the rest of it inside the tree.
    In(&'static Item, &'a str),
    /// A key of a chosen file, by its path in the file.
    Key(&'static Item, &'a str),
}

impl At<'_> {
    fn item(self) -> &'static Item {
        match self {
            At::Whole(item) | At::In(item, _) | At::Key(item, _) => item,
        }
    }
}

fn folder(item: &Item) -> bool {
    matches!(item.source, Source::Tree { .. })
}

/// What `path` names in the table: a whole item, a key of one, or a path
/// in the deepest of the folders it lies in.
fn item(path: &str) -> io::Result<At<'_>> {
    tree::check_path(path)?;
    if let Some((item, key)) = super::chosen(path) {
        return match item.source {
            Source::Keys { keys, .. } if keys::names(keys, key) => Ok(At::Key(item, key)),
            _ => Err(not_setup(path)),
        };
    }
    if let Some(item) = ITEMS.iter().find(|item| item.path == path && !folder(item)) {
        return match item.source {
            Source::Keys { .. } => Err(not_setup(path)),
            _ => Ok(At::Whole(item)),
        };
    }
    let (item, rest) = ITEMS
        .iter()
        .filter(|item| folder(item))
        .filter_map(|item| Some((item, path.strip_prefix(item.path)?.strip_prefix('/')?)))
        .max_by_key(|(item, _)| item.path.len())
        .ok_or_else(|| not_setup(path))?;
    let left_out = match item.source {
        Source::Tree { except } => excepted(except, rest) || rest.split('/').any(super::left_out),
        _ => false,
    };
    if left_out {
        return Err(not_setup(path));
    }
    Ok(At::In(item, rest))
}

/// Whether `path` is one the table has now: what an older table named and
/// this one does not is not.
pub(super) fn in_table(path: &str) -> bool {
    item(path).is_ok()
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

/// A file's entry: `text`, the hash of its text without the CR of each
/// CRLF, for an authored file.
fn file(bytes: &[u8], mode: Option<u32>, modified: Option<u64>, text: bool) -> Entry {
    Entry::File {
        size: bytes.len() as u64,
        sha256: chunked::hex(&Sha256::digest(bytes)),
        mode,
        modified,
        text: if text { tree::Text::of(bytes) } else { None },
    }
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

/// `path`, a setup path, under `root` as a file browser shows it: its
/// first name without its dot (`claude/…`, `codex/…`, `agents/…`), which
/// Finder and Explorer would hide, and each name a key's path holds that
/// Windows refuses spelled with `_`.
pub(super) fn visible(root: &Path, path: &str) -> PathBuf {
    let shown = path.strip_prefix('.').unwrap_or(path);
    let shown = match super::chosen(path) {
        Some(_) => shown.replace(|c: char| "<>:\"\\|?*".contains(c) || c < ' ', "_"),
        None => shown.to_string(),
    };
    shown.split('/').fold(root.to_path_buf(), |at, part| at.join(part))
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
        let stale = entry
            .metadata()
            .is_ok_and(|meta| meta.is_file() && chunked::stale(&entry.file_name(), &meta));
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

fn kept_sign_in(path: &str) -> io::Error {
    invalid(format!("{path} is a sign-in, and Setup sync never removes one"))
}

/// Why a link could not be made, in words.
fn unmade(why: tree::Refusal) -> io::Error {
    io::Error::other(match why {
        tree::Refusal::LeadsOut => "it would lead out of the folder it is in".to_string(),
        tree::Refusal::LeadsNowhere => "it would lead to nothing here".to_string(),
        tree::Refusal::Name => "this computer cannot hold its name".to_string(),
        tree::Refusal::Failed { error } => error,
        tree::Refusal::Unknown => "it could not be made".to_string(),
    })
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
    fn what_a_tool_derives_is_no_path_of_the_table_and_a_chosen_key_is() {
        for derived in [
            ".claude/plugins/installed_plugins.json",
            ".claude/plugins/cache/m/p/SKILL.md",
            ".claude/skills/synced/manifest.json",
            ".claude/settings.json",
            ".claude/settings.json#",
            ".codex/config.toml",
            ".codex/config.toml#projects",
            ".codex/config.toml#plugins",
            ".codex/skills/.system/x/SKILL.md",
            ".agents/.skill-lock.json",
            ".agents/skills/x/.git/HEAD",
        ] {
            assert!(!in_table(derived), "{derived}");
        }
        for chosen in [
            ".claude/settings.json#model",
            ".claude/settings.json#enabledPlugins/pstack@cursor-plugins",
            ".claude.json#mcpServers/github",
            ".codex/config.toml#model",
            ".codex/config.toml#plugins/sentry@m",
            ".agents/skills/x/SKILL.md",
            ".claude/CLAUDE.md",
        ] {
            assert!(in_table(chosen), "{chosen}");
        }
    }

    #[test]
    fn what_a_sync_keeps_aside_is_in_folders_a_file_browser_shows() {
        let kept = visible(Path::new("kept"), ".claude/settings.json#enabledPlugins/a:b");
        assert_eq!(kept, Path::new("kept").join("claude").join("settings.json#enabledPlugins").join("a_b"));
        assert_eq!(visible(Path::new("kept"), ".agents/skills/x"), Path::new("kept").join("agents").join("skills").join("x"));
    }
}
