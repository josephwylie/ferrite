//! How the app starts Sessions: the production `Spawner` adapter over the
//! two provider CLIs, and the process-memory sampler the watchdog reads.
//! The scripted demo Sessions are their own adapter, in `crate::demo` —
//! both hand the pump the same `Receiver<SessionEvent>`.

use std::io;
use std::sync::{Arc, Mutex};

use ferrite_core::cockpit::{RssSampler, SpawnRequest, Spawner};
use ferrite_core::providers::{ClaudeConfig, ClaudeSession, CodexConfig, CodexSession, Session};
use ferrite_core::settings::Settings;
use ferrite_core::store::Provider;
use ferrite_core::ThreadId;

/// What every new Session is started with beyond the Thread's own choice:
/// the operator's permission, sandbox and effort defaults, read from
/// Settings and shared between the window (which edits them) and the
/// spawner (which reads them at each spawn — so a change applies to the
/// next Session). A Thread's own effort, once chosen, wins over the
/// default.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SessionDefaults {
    pub claude_permission_mode: Option<String>,
    pub codex_approval_policy: Option<String>,
    pub codex_sandbox: Option<String>,
    pub claude_effort: Option<String>,
    pub codex_effort: Option<String>,
}

impl SessionDefaults {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            claude_permission_mode: settings.claude_permission_mode.clone(),
            codex_approval_policy: Some(settings.codex_approval_policy.clone())
                .filter(|policy| !policy.is_empty()),
            codex_sandbox: settings.codex_sandbox.clone(),
            claude_effort: settings.claude_effort.clone(),
            codex_effort: settings.codex_effort.clone(),
        }
    }

    /// The effort a spawn on `provider` gets when the Thread chose none.
    fn effort_for(&self, provider: Provider) -> Option<String> {
        match provider {
            Provider::Claude => self.claude_effort.clone(),
            Provider::Codex => self.codex_effort.clone(),
        }
    }
}

/// How the app starts Sessions: the Thread's stored choice becomes a
/// provider CLI process working in its binding.
pub struct Spawn {
    pub defaults: Arc<Mutex<SessionDefaults>>,
}

impl Spawn {
    pub fn new(defaults: Arc<Mutex<SessionDefaults>>) -> Self {
        Self { defaults }
    }
}

impl Spawner for Spawn {
    fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn Session>> {
        // The Thread's workspace binding decides where the Session works; a
        // Thread from before bindings falls back to where Ferrite started.
        let cwd = request
            .cwd
            .map(|dir| dir.to_path_buf())
            .or_else(|| std::env::current_dir().ok());
        let model = request.model.map(|model| model.to_string());
        let resume = request.resume.map(|target| target.to_string());
        let name = request.name.map(|name| name.to_string());
        let defaults = self
            .defaults
            .lock()
            .map(|defaults| defaults.clone())
            .unwrap_or_default();
        // The Thread's own effort, else the operator's default for the
        // provider, else nothing — the provider's own.
        let effort = request
            .effort
            .map(|effort| effort.to_string())
            .or_else(|| defaults.effort_for(request.provider));
        match request.provider {
            Provider::Claude => ClaudeSession::spawn(ClaudeConfig {
                cwd,
                model,
                effort,
                name,
                permission_mode: defaults.claude_permission_mode,
                resume,
                ..Default::default()
            })
            .map(|session| Box::new(session) as Box<dyn Session>)
            // The typed spawn error keeps its words, which is what the Pane
            // shows; only its type is lost crossing this seam.
            .map_err(|e| io::Error::other(e.to_string())),
            Provider::Codex => CodexSession::spawn(CodexConfig {
                cwd,
                model,
                effort,
                approval_policy: defaults
                    .codex_approval_policy
                    .or_else(|| Some("on-request".into())),
                sandbox: defaults.codex_sandbox,
                resume,
                ..Default::default()
            })
            .map(|session| Box::new(session) as Box<dyn Session>)
            .map_err(|e| io::Error::other(e.to_string())),
        }
    }
}

/// Resident memory per Session, read the way the panes24 spike reads it.
#[derive(Default)]
pub struct ProcessRss;

impl RssSampler for ProcessRss {
    fn sample(&mut self, _thread: ThreadId, pid: Option<u32>) -> Option<u64> {
        rss_bytes(pid?)
    }
}

/// One process's resident bytes, the way the panes24 spike reads it.
#[cfg(not(windows))]
pub(crate) fn rss_bytes(pid: u32) -> Option<u64> {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let kilobytes: u64 = String::from_utf8(out.stdout).ok()?.trim().parse().ok()?;
    Some(kilobytes * 1024)
}

/// One process's resident bytes. Windows has no `ps`; `tasklist` is the
/// stock instrument, and its working-set column is also kilobytes.
#[cfg(windows)]
pub(crate) fn rss_bytes(pid: u32) -> Option<u64> {
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .ok()?;
    tasklist_rss(&String::from_utf8_lossy(&out.stdout))
}

/// The memory column of one `tasklist` CSV row — the last quoted field,
/// `"12,345 K"`. Digit grouping is locale-typed (`.` in German), so only the
/// digits are read.
#[cfg(any(windows, test))]
fn tasklist_rss(row: &str) -> Option<u64> {
    let field = row.trim().rsplit('"').nth(1)?;
    let digits: String = field.chars().filter(char::is_ascii_digit).collect();
    let kilobytes: u64 = digits.parse().ok()?;
    Some(kilobytes * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Thread's effort wins; the Settings default fills in only when
    /// the Thread chose none; and each provider reads its own.
    #[test]
    fn the_settings_effort_is_the_default_when_the_thread_chose_none() {
        let mut settings = Settings::default();
        settings.claude_effort = Some("high".into());
        let defaults = SessionDefaults::from_settings(&settings);
        assert_eq!(
            defaults.effort_for(Provider::Claude).as_deref(),
            Some("high")
        );
        assert_eq!(defaults.effort_for(Provider::Codex), None);
        assert_eq!(
            Some("max".to_string()).or_else(|| defaults.effort_for(Provider::Claude)),
            Some("max".into())
        );
        assert_eq!(
            None.or_else(|| defaults.effort_for(Provider::Claude)),
            Some("high".into())
        );
    }

    /// Runs against whichever process table this platform has — `ps` here,
    /// `tasklist` when the suite runs on Windows.
    #[test]
    fn the_rss_sampler_reads_this_very_process() {
        let rss = rss_bytes(std::process::id()).expect("own pid must sample");
        assert!(rss > 1024 * 1024, "resident bytes were {rss}");
    }

    #[test]
    fn a_tasklist_row_yields_bytes_whatever_the_locale_groups_with() {
        for row in [
            "\"node.exe\",\"1234\",\"Console\",\"1\",\"54,321 K\"\r\n",
            "\"node.exe\",\"1234\",\"Console\",\"1\",\"54.321 K\"",
        ] {
            assert_eq!(tasklist_rss(row), Some(54_321 * 1024), "{row:?}");
        }
        assert_eq!(tasklist_rss("INFO: No tasks are running.\r\n"), None);
        assert_eq!(tasklist_rss(""), None);
    }
}
