//! What makes a chosen plugin real on a side (ADR 0050). A sync carries only
//! the choice, and each side's own tool installs: a plugin's download is
//! derived, and never moves. Codex installs each plugin its config enables,
//! from each marketplace it names, as a thread starts, so nothing more is
//! done for it. Claude does not: once a sync writes one of Claude's plugin
//! or marketplace keys on a side, that side installs each plugin set on
//! that Claude has not installed, with `claude plugin`, off the thread that
//! wrote the key and one run at a time. A failure is said by each scan of
//! that side until a later write tries again. Nothing is ever uninstalled:
//! a plugin turned off is a key set false, which Claude reads itself.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use serde_json::Value;

use crate::provider::Provider;

/// Where a scan names a plugin that could not be installed:
/// `.claude/plugins/<plugin>@<marketplace>`, the derived folder the plugin
/// would be in. No entry is there, so the problem holds nothing back.
pub(super) const FAILED: &str = ".claude/plugins/";

/// What runs Claude's installer: `claude plugin <words>`, `Ok` once it did
/// what was asked, else why not in Claude's words.
pub trait Installer: Send + Sync {
    fn run(&self, words: &[&str]) -> io::Result<()>;
}

/// One side's installs of Claude's plugins.
pub struct Installs {
    installer: Box<dyn Installer>,
    state: Mutex<State>,
    idle: Condvar,
}

#[derive(Default)]
struct State {
    running: bool,
    /// A key was written while a run was under way: run once more after.
    again: bool,
    /// Each plugin whose install failed, and why.
    failed: BTreeMap<String, String>,
}

impl Installs {
    pub fn new(installer: impl Installer + 'static) -> Arc<Self> {
        Arc::new(Self {
            installer: Box::new(installer),
            state: Mutex::default(),
            idle: Condvar::new(),
        })
    }

    /// This process's, through the `claude` a Session would run: one for
    /// every Home of the process, so what failed is said by any later scan.
    pub(super) fn claude() -> Arc<Self> {
        static ONE: OnceLock<Arc<Installs>> = OnceLock::new();
        ONE.get_or_init(|| Self::new(Claude)).clone()
    }

    /// A plugin or marketplace key of the Claude whose folder is `claude`
    /// was written: install what is on there and not installed, once any
    /// run under way is over. What a run finds failed stands until the
    /// next run that no later write overtook.
    pub(super) fn changed(self: &Arc<Self>, claude: &Path) {
        let mut state = self.lock();
        if state.running {
            state.again = true;
            return;
        }
        state.running = true;
        drop(state);
        let (installs, dir) = (self.clone(), claude.to_path_buf());
        let spawned = std::thread::Builder::new().name("setup-sync-installs".into()).spawn(move || loop {
            let failed = installs.install(&dir);
            let mut state = installs.lock();
            if std::mem::take(&mut state.again) {
                continue;
            }
            state.failed = failed;
            state.running = false;
            installs.idle.notify_all();
            return;
        });
        if let Err(error) = spawned {
            let mut state = self.lock();
            state.running = false;
            state.failed = Plugins::of(claude).missing().map(|plugin| (plugin.to_string(), error.to_string())).collect();
        }
    }

    /// Install each plugin set on in `claude` that is not installed: its
    /// marketplace added first when Claude does not know it yet, from the
    /// source the settings name. What failed, by plugin.
    fn install(&self, claude: &Path) -> BTreeMap<String, String> {
        let plugins = Plugins::of(claude);
        let mut known = plugins.known.clone();
        let mut failed = BTreeMap::new();
        for plugin in plugins.missing() {
            let market = plugin.rsplit_once('@').map(|(_, market)| market);
            let installed = (|| {
                if let Some(market) = market.filter(|market| !known.contains(*market)) {
                    let source = plugins.sources.get(market).ok_or_else(|| {
                        io::Error::other(format!("Claude there knows no marketplace `{market}`, and the settings name no source for it"))
                    })?;
                    self.installer.run(&["marketplace", "add", source])?;
                    known.insert(market.to_string());
                }
                self.installer.run(&["install", plugin, "--json"])
            })();
            if let Err(error) = installed {
                failed.insert(plugin.to_string(), error.to_string());
            }
        }
        failed
    }

    /// Each plugin whose install failed and that is still on and not
    /// installed in `claude`, as a scan's problem.
    pub(super) fn problems(&self, claude: &Path) -> Vec<(String, String)> {
        let failed = self.lock().failed.clone();
        if failed.is_empty() {
            return Vec::new();
        }
        let missing: BTreeSet<String> = Plugins::of(claude).missing().map(str::to_string).collect();
        failed
            .into_iter()
            .filter(|(plugin, _)| missing.contains(plugin))
            .map(|(plugin, why)| (format!("{FAILED}{plugin}"), why))
            .collect()
    }

    /// Wait for the run under way, if any, to end, for at most `within`.
    pub(super) fn wait(&self, within: Duration) {
        let state = self.lock();
        let waited = self.idle.wait_timeout_while(state, within, |state| state.running);
        drop(waited.unwrap_or_else(PoisonError::into_inner));
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What a Claude's own files say of its plugins.
struct Plugins {
    /// Set true in `settings.json`'s `enabledPlugins`.
    on: Vec<String>,
    /// Listed in `plugins/installed_plugins.json`.
    installed: BTreeSet<String>,
    /// Listed in `plugins/known_marketplaces.json`.
    known: BTreeSet<String>,
    /// What `claude plugin marketplace add` is given for each marketplace
    /// `settings.json`'s `extraKnownMarketplaces` names.
    sources: BTreeMap<String, String>,
}

impl Plugins {
    fn of(claude: &Path) -> Self {
        let read = |path: &[&str]| -> Value {
            let at = path.iter().fold(claude.to_path_buf(), |at, part| at.join(part));
            std::fs::read(at).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or(Value::Null)
        };
        let keys = |value: &Value| -> BTreeSet<String> {
            value.as_object().map(|map| map.keys().cloned().collect()).unwrap_or_default()
        };
        let settings = read(&["settings.json"]);
        let on = settings["enabledPlugins"]
            .as_object()
            .map(|on| on.iter().filter(|(_, on)| **on == Value::Bool(true)).map(|(plugin, _)| plugin.clone()).collect())
            .unwrap_or_default();
        let sources = settings["extraKnownMarketplaces"]
            .as_object()
            .map(|markets| markets.iter().filter_map(|(name, market)| Some((name.clone(), source(&market["source"])?))).collect())
            .unwrap_or_default();
        Self {
            on,
            installed: keys(&read(&["plugins", "installed_plugins.json"])["plugins"]),
            known: keys(&read(&["plugins", "known_marketplaces.json"])),
            sources,
        }
    }

    fn missing(&self) -> impl Iterator<Item = &str> {
        self.on.iter().map(String::as_str).filter(|plugin| !self.installed.contains(*plugin))
    }
}

/// A marketplace's `source` as `claude plugin marketplace add` takes it.
fn source(source: &Value) -> Option<String> {
    let field = |name: &str| source[name].as_str().map(str::to_string);
    match source["source"].as_str()? {
        "github" => field("repo"),
        "git" | "url" => field("url"),
        "directory" | "file" => field("path"),
        _ => None,
    }
}

/// The `claude` a Session would run.
struct Claude;

impl Installer for Claude {
    fn run(&self, words: &[&str]) -> io::Result<()> {
        let program = crate::providers::discover::program(Provider::Claude);
        let output = crate::spawn::agent(&program)
            .arg("plugin")
            .args(words)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| match error.kind() {
                io::ErrorKind::NotFound => io::Error::other("Claude Code is not installed there"),
                _ => error,
            })?;
        if output.status.success() {
            return Ok(());
        }
        Err(io::Error::other(said(&output.stdout, &output.stderr)))
    }
}

/// What `claude plugin … --json` said went wrong: the message of its last
/// line of JSON, else its last line.
fn said(stdout: &[u8], stderr: &[u8]) -> String {
    let lines = |bytes: &[u8]| String::from_utf8_lossy(bytes).lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect::<Vec<_>>();
    let (out, err) = (lines(stdout), lines(stderr));
    let message = out
        .iter()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok()?["message"].as_str().map(str::to_string));
    message
        .or_else(|| err.last().or(out.last()).map(|line| line.trim_start_matches(['✘', ' ']).to_string()))
        .unwrap_or_else(|| "Claude's installer failed".into())
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
    fn what_claudes_installer_said_is_its_message() {
        let failed = br#"{"command":"install","outcome":"failed","plugin":"pstack@cursor-plugins","message":"Plugin \"pstack\" not found in marketplace \"cursor-plugins\"."}"#;
        assert_eq!(said(failed, b""), r#"Plugin "pstack" not found in marketplace "cursor-plugins"."#);
        assert_eq!(said(b"", "\u{2718} Failed to add marketplace: no such repo\n".as_bytes()), "Failed to add marketplace: no such repo");
        let github = serde_json::json!({"source": "github", "repo": "cursor/plugins"});
        assert_eq!(source(&github).as_deref(), Some("cursor/plugins"));
        assert_eq!(source(&serde_json::json!({"source": "npm", "package": "x"})), None);
    }
}
