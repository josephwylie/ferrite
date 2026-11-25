//! What makes a chosen plugin real on a side (ADR 0050). A sync carries only
//! the choice, and each side's own tools install it: a plugin's download is
//! derived, and never moves. Once a sync writes a plugin or marketplace key
//! on a side, that side's next scan, the one that ends the sync, runs its
//! tools' installers on a thread of its own, one run at a time, for the
//! plugins of marketplaces that travel alone: what a tool bundles, Claude's
//! `@builtin` for one, is never missing. A run holds the side's home as a
//! sync does, so no tool rewrites a file a sync is writing: on either side
//! of the sync that wanted it, it begins once that sync lets go, and before
//! the next sync of that home, which says what it found. A scan waits for a
//! run once at most, and each tool is stopped once it runs too long.
//!
//! - Claude installs nothing by itself: each plugin set on that it has not
//!   installed is `claude plugin install`ed, after `claude plugin
//!   marketplace add` when it does not know the marketplace. That add
//!   writes the marketplace's settings entry again without what the
//!   operator set on it, so the entry is put back as it was.
//! - Codex installs each plugin its config enables as a thread starts, but
//!   lists none while a marketplace it names has no copy fetched, so each
//!   such one is fetched with `codex plugin marketplace upgrade`.
//!
//! A tool is handed only names and sources of the shape it takes itself,
//! each after `--`. A failure is said by each scan of that side, and tried
//! again by the next write of a plugin key and by the side's next start;
//! never in a loop. Both the wish a write leaves and each failure are kept
//! in a file of the side's settings folder, so a sync cut short before its
//! last scan, or a write one Ferrite process made, is installed by the next
//! scan of any. Nothing is ever uninstalled: a plugin turned off is a key
//! set false, which the tool reads itself. What runs a tool is the
//! engine's (`Installer`), which knows where it is.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::keys::{self, plain};
use super::lock::Lock;
use super::store::{private_dirs, write_over};
use super::Format;
use crate::provider::Provider;

/// Where a scan names a plugin that could not be installed, by tool: the
/// derived folder the plugin would be in, and the plugin's name after it.
/// No entry is there, so the problem holds nothing back.
const FAILED: [(Provider, &str); 2] = [(Provider::Claude, ".claude/plugins/"), (Provider::Codex, ".codex/plugins/")];

/// Where a side keeps [`Kept`], in its Ferrite settings folder.
const KEPT: &str = "setup-installs.json";

/// The plugin a problem at `path` says could not be installed, as the row
/// names it: `pstack`, or `pstack for Codex`.
pub fn installing(path: &str) -> Option<String> {
    let (tool, plugin) = FAILED.iter().find_map(|(tool, under)| Some((*tool, path.strip_prefix(under)?)))?;
    let name = plugin.rsplit_once('@').map_or(plugin, |(name, _)| name);
    Some(match tool {
        Provider::Claude => name.to_string(),
        Provider::Codex => format!("{name} for Codex"),
    })
}

/// What runs an agent's installer: `<tool> plugin <words>`, `Ok` once it
/// did what was asked, else why not in the tool's words.
pub trait Installer: Send + Sync {
    fn run(&self, tool: Provider, words: &[&str]) -> io::Result<()>;
}

/// Where a side's agents keep their files, and what holds that side's home.
#[derive(Clone)]
pub(super) struct Folders {
    pub claude: PathBuf,
    pub codex: PathBuf,
    pub lock: Arc<Lock>,
}

/// One side's installs of the plugins chosen there.
pub struct Installs {
    installer: Box<dyn Installer>,
    /// Where [`Kept`] is.
    kept: PathBuf,
    state: Mutex<State>,
    idle: Condvar,
}

#[derive(Default)]
struct State {
    running: bool,
    /// The run under way waits for its home, which a sync holds.
    blocked: bool,
    /// A scan waited for the run under way: no other waits for it again.
    waited: bool,
    /// An earlier start left a failure and this one has not run yet: its
    /// first scan tries again.
    retry: bool,
    /// Each plugin whose install failed, by where a scan says it, and why.
    failed: BTreeMap<String, String>,
}

/// What a side keeps of its installs between its Ferrite processes.
#[derive(Default, Serialize, Deserialize)]
struct Kept {
    /// A plugin or marketplace key was written since the last run began.
    #[serde(default)]
    wanted: bool,
    /// What the last run found failed.
    #[serde(default)]
    failed: BTreeMap<String, String>,
}

impl Installs {
    /// Installs by `installer`, what is wanted and what failed kept in
    /// `settings`, a Ferrite settings folder: what an earlier start left
    /// failed there is tried again at the first scan.
    pub fn new(installer: impl Installer + 'static, settings: &Path) -> Arc<Self> {
        let kept = settings.join(KEPT);
        let failed = read(&kept).failed;
        let state = State { retry: !failed.is_empty(), failed, ..State::default() };
        Arc::new(Self {
            installer: Box::new(installer),
            kept,
            state: Mutex::new(state),
            idle: Condvar::new(),
        })
    }

    /// A plugin or marketplace key was written: the next scan installs
    /// what is on and not installed. Not before: a sync still writing the
    /// file a tool's installer writes too would race it.
    pub(super) fn changed(&self) {
        let _state = self.lock();
        self.keep(|kept| kept.wanted = true);
    }

    /// What a scan says of the installs: the run under way waited for,
    /// then one for what was wanted since, both within `within` of the
    /// scan's start; then each plugin that failed and is still on and not
    /// installed. What a run finds failed stands until the next run.
    pub(super) fn settled(self: &Arc<Self>, folders: &Folders, within: Duration) -> Vec<(String, String)> {
        let until = Instant::now() + within;
        self.wait(until);
        self.start(folders);
        self.wait(until);
        let failed = self.lock().failed.clone();
        if failed.is_empty() {
            return Vec::new();
        }
        let missing = missing(folders);
        failed.into_iter().filter(|(plugin, _)| missing.contains(plugin)).collect()
    }

    /// Run the installers for `folders` on a thread of their own, when one
    /// is wanted and none is under way. The run holds the side's home while
    /// it runs: a sync holding it now is let go first.
    fn start(self: &Arc<Self>, folders: &Folders) {
        let mut state = self.lock();
        let retry = std::mem::take(&mut state.retry);
        if state.running || !(read(&self.kept).wanted || retry) {
            return;
        }
        let held = match folders.lock.try_hold() {
            Ok(held) => held,
            Err(error) => {
                state.retry = retry;
                state.failed = missing(folders).into_iter().map(|plugin| (plugin, error.to_string())).collect();
                return;
            }
        };
        self.keep(|kept| kept.wanted = false);
        state.running = true;
        state.blocked = held.is_none();
        state.waited = false;
        drop(state);
        let (installs, of) = (self.clone(), folders.clone());
        let spawned = std::thread::Builder::new().name("setup-sync-installs".into()).spawn(move || {
            let held = held.map_or_else(|| of.lock.hold(), Ok);
            installs.lock().blocked = false;
            let failed = match &held {
                Ok(_) => installs.install(&of),
                Err(error) => missing(&of).into_iter().map(|plugin| (plugin, error.to_string())).collect(),
            };
            let mut state = installs.lock();
            installs.keep(|kept| kept.failed = failed.clone());
            state.failed = failed;
            state.running = false;
            installs.idle.notify_all();
        });
        if let Err(error) = spawned {
            let mut state = self.lock();
            state.running = false;
            state.blocked = false;
            state.failed = missing(folders).into_iter().map(|plugin| (plugin, error.to_string())).collect();
            self.keep(|kept| kept.wanted = true);
        }
    }

    /// Install what each tool lacks of what is chosen in `folders`. What
    /// failed, by where a scan says it.
    fn install(&self, folders: &Folders) -> BTreeMap<String, String> {
        let mut failed = self.claude(&folders.claude);
        failed.extend(self.codex(&folders.codex));
        failed
    }

    /// Install each plugin set on in the Claude at `dir` that it has not
    /// installed: its marketplace added first when Claude does not know it.
    fn claude(&self, dir: &Path) -> BTreeMap<String, String> {
        let claude = Claude::of(dir);
        let mut known = claude.known.clone();
        let mut failed = BTreeMap::new();
        for (plugin, market) in claude.missing() {
            let installed = (|| {
                if !plugin.split_once('@').is_some_and(|(name, _)| plain(name)) || !plain(market) {
                    return Err(refused("its name"));
                }
                if !known.contains(market) {
                    let source = claude.sources.get(market).ok_or_else(|| refused("its marketplace's source"))?;
                    self.add(dir, market, source)?;
                    known.insert(market.to_string());
                }
                self.installer.run(Provider::Claude, &["install", "--json", "--", plugin])
            })();
            if let Err(error) = installed {
                failed.insert(format!("{}{plugin}", FAILED[0].1), error.to_string());
            }
        }
        failed
    }

    /// `claude plugin marketplace add` of `market` from `source`, its entry
    /// in the settings put back as it was: Claude writes it again without
    /// what the operator set on it, `autoUpdate` for one.
    fn add(&self, dir: &Path, market: &str, source: &str) -> io::Result<()> {
        let settings = dir.join("settings.json");
        let entry = || -> Option<Value> {
            let settings: Value = serde_json::from_slice(&fs::read(&settings).ok()?).ok()?;
            Some(settings["extraKnownMarketplaces"].get(market)?.clone())
        };
        let was = entry();
        let added = self.installer.run(Provider::Claude, &["marketplace", "add", "--", source]);
        if let Some(was) = was.filter(|was| entry().as_ref() != Some(was)) {
            let text = fs::read(&settings)?;
            write_over(&settings, &keys::put_json(&text, "extraKnownMarketplaces", market, was)?)?;
        }
        added
    }

    /// Fetch each marketplace the Codex at `dir` lacks a copy of, for the
    /// plugins its config enables.
    fn codex(&self, dir: &Path) -> BTreeMap<String, String> {
        let mut failed = BTreeMap::new();
        for (market, plugins) in Codex::of(dir).missing() {
            let fetched = match plain(&market) {
                true => self.installer.run(Provider::Codex, &["marketplace", "upgrade", "--", &market]),
                false => Err(refused("its marketplace's name")),
            };
            if let Err(error) = fetched {
                failed.extend(plugins.into_iter().map(|plugin| (format!("{}{plugin}", FAILED[1].1), error.to_string())));
            }
        }
        failed
    }

    /// Change what is kept by `change`, the state's lock held. A file that
    /// cannot be written costs only what another process or start would
    /// have read from it: this one still runs and says each failure.
    fn keep(&self, change: impl FnOnce(&mut Kept)) {
        let mut kept = read(&self.kept);
        change(&mut kept);
        let _ = match !kept.wanted && kept.failed.is_empty() {
            true => fs::remove_file(&self.kept),
            false => self.kept.parent().map_or(Ok(()), private_dirs).and_then(|()| {
                write_over(&self.kept, &serde_json::to_vec_pretty(&kept).unwrap_or_default())
            }),
        };
    }

    /// Whether a run waits for its home, which a sync holds: it goes before
    /// the next sync of that home, which then says what it found.
    pub(super) fn waiting(&self) -> bool {
        self.lock().blocked
    }

    /// Wait for the run under way, if any, to end, until `until` at most:
    /// by the first scan that finds it, never by another, and not while it
    /// waits for its home, which a sync holds and lets go only once its
    /// scans are done.
    fn wait(&self, until: Instant) {
        let mut state = self.lock();
        if !state.running || state.blocked || state.waited {
            return;
        }
        state.waited = true;
        let within = until.saturating_duration_since(Instant::now());
        let waited = self.idle.wait_timeout_while(state, within, |state| state.running);
        drop(waited.unwrap_or_else(PoisonError::into_inner));
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What is kept at `at`; nothing when it is not there or does not read.
fn read(at: &Path) -> Kept {
    fs::read(at).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

/// Each plugin the tools in `folders` lack of what is chosen there, by
/// where a scan says it.
fn missing(folders: &Folders) -> BTreeSet<String> {
    let claude = Claude::of(&folders.claude);
    let claude = claude.missing().map(|(plugin, _)| format!("{}{plugin}", FAILED[0].1)).collect::<Vec<_>>();
    let codex = Codex::of(&folders.codex).missing().into_values().flatten().map(|plugin| format!("{}{plugin}", FAILED[1].1));
    claude.into_iter().chain(codex).collect()
}

/// Why a name or a source was not handed to a tool.
fn refused(what: &str) -> io::Error {
    io::Error::other(format!("{what} is not one Setup sync hands to an installer"))
}

/// What a Claude's own files say of its plugins.
struct Claude {
    /// Set true in `settings.json`'s `enabledPlugins`.
    on: Vec<String>,
    /// Listed in `plugins/installed_plugins.json`.
    installed: BTreeSet<String>,
    /// Listed in `plugins/known_marketplaces.json`.
    known: BTreeSet<String>,
    /// Where each marketplace that travels is fetched from, by its name in
    /// `extraKnownMarketplaces`.
    sources: BTreeMap<String, String>,
}

impl Claude {
    fn of(dir: &Path) -> Self {
        let read = |path: &[&str]| -> Value {
            let at = path.iter().fold(dir.to_path_buf(), |at, part| at.join(part));
            fs::read(at).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or(Value::Null)
        };
        let keys = |value: &Value| -> BTreeSet<String> { value.as_object().map(|map| map.keys().cloned().collect()).unwrap_or_default() };
        let settings = read(&["settings.json"]);
        let on = settings["enabledPlugins"]
            .as_object()
            .map(|on| on.iter().filter(|(_, on)| **on == Value::Bool(true)).map(|(plugin, _)| plugin.clone()).collect())
            .unwrap_or_default();
        let sources = settings["extraKnownMarketplaces"]
            .as_object()
            .map(|markets| markets.iter().filter_map(|(name, market)| Some((name.clone(), keys::claude_source(market)?.to_string()))).collect())
            .unwrap_or_default();
        Self {
            on,
            installed: keys(&read(&["plugins", "installed_plugins.json"])["plugins"]),
            known: keys(&read(&["plugins", "known_marketplaces.json"])),
            sources,
        }
    }

    /// Each plugin on and not installed, of a marketplace that travels,
    /// with its marketplace.
    fn missing(&self) -> impl Iterator<Item = (&str, &str)> {
        self.on
            .iter()
            .filter(|plugin| !self.installed.contains(*plugin))
            .filter_map(|plugin| Some((plugin.as_str(), plugin.rsplit_once('@')?.1)))
            .filter(|(_, market)| self.sources.contains_key(*market))
    }
}

/// What a Codex's config says of its plugins.
struct Codex {
    dir: PathBuf,
    config: Map<String, Value>,
}

impl Codex {
    fn of(dir: &Path) -> Self {
        let config = fs::read(dir.join("config.toml")).ok().and_then(|raw| keys::document(Format::Toml, &raw).ok());
        Self { dir: dir.to_path_buf(), config: config.unwrap_or_default() }
    }

    /// Each marketplace that travels that a plugin the config enables is
    /// of and that Codex has no copy of, with those plugins.
    fn missing(&self) -> BTreeMap<String, Vec<String>> {
        let markets = self.config.get("marketplaces");
        let mut missing: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (plugin, entry) in self.config.get("plugins").and_then(Value::as_object).into_iter().flatten() {
            let Some((_, market)) = plugin.rsplit_once('@') else {
                continue;
            };
            let travels = markets.and_then(|markets| markets.get(market)).is_some_and(|market| keys::codex_source(market).is_some());
            let fetched = || plain(market) && self.dir.join(".tmp").join("marketplaces").join(market).is_dir();
            if entry["enabled"] == Value::Bool(true) && travels && !fetched() {
                missing.entry(market.to_string()).or_default().push(plugin.clone());
            }
        }
        missing
    }
}

impl std::fmt::Debug for Installs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Installs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_problem_names_the_plugin_that_did_not_install_and_its_tool() {
        assert_eq!(installing(".claude/plugins/pstack@cursor-plugins").as_deref(), Some("pstack"));
        assert_eq!(installing(".codex/plugins/sentry@cursor-plugins").as_deref(), Some("sentry for Codex"));
        assert_eq!(installing(".claude/skills/x"), None);
    }
}
