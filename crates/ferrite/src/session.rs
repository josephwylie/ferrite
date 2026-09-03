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
use ferrite_core::{SessionEvent, ThreadId};

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
    /// Answers at once with a Session whose process is still starting:
    /// the CLI's own start and handshake take one to three seconds, and
    /// spawning on the UI thread froze every frame of a revive, a model
    /// change or an effort change for exactly that long. The real spawn
    /// runs on a thread; prompts sent meanwhile are queued and go out in
    /// order once it is up, and a spawn that fails closes the Session
    /// with the error's own words.
    fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn Session>> {
        let config = self.config(request);
        Ok(Box::new(LazySession::start(move || config.spawn())))
    }
}

/// Everything a spawn needs, resolved on the UI thread (the defaults are
/// read at the moment of the request, as before) and carried to the
/// spawning thread.
enum SessionConfig {
    Claude(ClaudeConfig),
    Codex(CodexConfig),
}

impl SessionConfig {
    fn spawn(self) -> io::Result<Box<dyn Session + Send>> {
        match self {
            SessionConfig::Claude(config) => ClaudeSession::spawn(config)
                .map(|session| Box::new(session) as Box<dyn Session + Send>)
                // The typed spawn error keeps its words, which is what the
                // Pane shows; only its type is lost crossing this seam.
                .map_err(|e| io::Error::other(e.to_string())),
            SessionConfig::Codex(config) => CodexSession::spawn(config)
                .map(|session| Box::new(session) as Box<dyn Session + Send>)
                .map_err(|e| io::Error::other(e.to_string())),
        }
    }
}

/// How often the forwarding thread looks for the real Session's events
/// while it lives. Below the pump's own 8ms, so it never adds a frame.
const FORWARD_POLL: std::time::Duration = std::time::Duration::from_millis(4);

/// What the lazy Session holds until the real one is up, then the real
/// one itself.
struct Pending {
    session: Option<Box<dyn Session + Send>>,
    /// Prompts sent before the process was ready, in order.
    queued: Vec<String>,
    /// A title given before the process was ready.
    name: Option<String>,
    /// The spawn failed: nothing will ever be sent.
    dead: bool,
}

/// A Session that stands in for one still spawning. See `Spawn::spawn`.
pub struct LazySession {
    inner: Arc<Mutex<Pending>>,
    events: std::sync::mpsc::Receiver<SessionEvent>,
}

impl LazySession {
    pub fn start(
        spawn: impl FnOnce() -> io::Result<Box<dyn Session + Send>> + Send + 'static,
    ) -> Self {
        let (tx, events) = std::sync::mpsc::channel();
        let inner = Arc::new(Mutex::new(Pending {
            session: None,
            queued: Vec::new(),
            name: None,
            dead: false,
        }));
        let shared = inner.clone();
        std::thread::Builder::new()
            .name("ferrite-spawn".into())
            .spawn(move || Self::run(shared, tx, spawn))
            .expect("spawn the session thread");
        Self { inner, events }
    }

    fn run(
        inner: Arc<Mutex<Pending>>,
        tx: std::sync::mpsc::Sender<SessionEvent>,
        spawn: impl FnOnce() -> io::Result<Box<dyn Session + Send>>,
    ) {
        let session = match spawn() {
            Ok(session) => session,
            Err(e) => {
                if let Ok(mut pending) = inner.lock() {
                    pending.dead = true;
                    pending.queued.clear();
                }
                let _ = tx.send(SessionEvent::Closed {
                    reason: format!("could not start: {e}"),
                });
                return;
            }
        };
        // Adopt the process, then let out what waited for it, in order.
        {
            let Ok(mut pending) = inner.lock() else {
                return;
            };
            let mut session = session;
            if let Some(name) = pending.name.take() {
                let _ = session.set_name(&name);
            }
            for text in std::mem::take(&mut pending.queued) {
                if let Err(e) = session.send(&text) {
                    let _ = tx.send(SessionEvent::Closed {
                        reason: format!("send failed: {e}"),
                    });
                    pending.dead = true;
                    return;
                }
            }
            pending.session = Some(session);
        }
        // Forward events until the lazy Session is dropped (this thread
        // then holds the only reference) or the pump has gone.
        loop {
            if Arc::strong_count(&inner) == 1 {
                return;
            }
            let Ok(pending) = inner.lock() else {
                return;
            };
            let mut closed = false;
            if let Some(session) = pending.session.as_ref() {
                while let Ok(event) = session.events().try_recv() {
                    closed |= matches!(event, SessionEvent::Closed { .. });
                    if tx.send(event).is_err() {
                        return;
                    }
                }
            }
            drop(pending);
            if closed {
                return;
            }
            std::thread::sleep(FORWARD_POLL);
        }
    }
}

impl Session for LazySession {
    fn events(&self) -> &std::sync::mpsc::Receiver<SessionEvent> {
        &self.events
    }

    fn send(&mut self, text: &str) -> io::Result<()> {
        let mut pending = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("the session thread panicked"))?;
        if pending.dead {
            return Err(io::Error::other("the Session never started"));
        }
        match pending.session.as_mut() {
            Some(session) => session.send(text),
            None => {
                pending.queued.push(text.to_string());
                Ok(())
            }
        }
    }

    fn interrupt(&mut self) -> io::Result<()> {
        let mut pending = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("the session thread panicked"))?;
        match pending.session.as_mut() {
            Some(session) => session.interrupt(),
            // Interrupting a turn that has not started is taking the
            // prompts back.
            None => {
                pending.queued.clear();
                Ok(())
            }
        }
    }

    fn respond_to_decision(
        &mut self,
        id: &str,
        answer: ferrite_core::DecisionAnswer,
    ) -> io::Result<()> {
        let mut pending = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("the session thread panicked"))?;
        match pending.session.as_mut() {
            Some(session) => session.respond_to_decision(id, answer),
            // No Decision can have arrived from a process not yet running.
            None => Ok(()),
        }
    }

    fn pid(&self) -> Option<u32> {
        self.inner
            .lock()
            .ok()
            .and_then(|pending| pending.session.as_ref().and_then(|session| session.pid()))
    }

    fn set_name(&mut self, name: &str) -> io::Result<()> {
        let mut pending = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("the session thread panicked"))?;
        match pending.session.as_mut() {
            Some(session) => session.set_name(name),
            None => {
                pending.name = Some(name.to_string());
                Ok(())
            }
        }
    }
}

impl Spawn {
    fn config(&self, request: SpawnRequest) -> SessionConfig {
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
            Provider::Claude => SessionConfig::Claude(ClaudeConfig {
                cwd,
                model,
                effort,
                name,
                permission_mode: defaults.claude_permission_mode,
                resume,
                ..Default::default()
            }),
            Provider::Codex => SessionConfig::Codex(CodexConfig {
                cwd,
                model,
                effort,
                approval_policy: defaults
                    .codex_approval_policy
                    .or_else(|| Some("on-request".into())),
                sandbox: defaults.codex_sandbox,
                resume,
                ..Default::default()
            }),
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
    use std::sync::mpsc::{channel, Receiver, Sender};

    /// A scripted Session the lazy one can wrap: records sends, forwards
    /// whatever the test pushes.
    struct Scripted {
        rx: Receiver<SessionEvent>,
        sent: Arc<Mutex<Vec<String>>>,
        named: Arc<Mutex<Option<String>>>,
    }

    impl Session for Scripted {
        fn events(&self) -> &Receiver<SessionEvent> {
            &self.rx
        }
        fn send(&mut self, text: &str) -> io::Result<()> {
            self.sent.lock().unwrap().push(text.to_string());
            Ok(())
        }
        fn interrupt(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn respond_to_decision(
            &mut self,
            _: &str,
            _: ferrite_core::DecisionAnswer,
        ) -> io::Result<()> {
            Ok(())
        }
        fn set_name(&mut self, name: &str) -> io::Result<()> {
            *self.named.lock().unwrap() = Some(name.to_string());
            Ok(())
        }
    }

    fn wait_for<T>(mut probe: impl FnMut() -> Option<T>) -> T {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(found) = probe() {
                return found;
            }
            assert!(std::time::Instant::now() < deadline, "timed out");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// The lazy Session answers before the process exists: a prompt sent
    /// meanwhile is queued and goes out first once it is up, a name given
    /// meanwhile reaches it, and its events are forwarded.
    #[test]
    fn a_lazy_session_queues_until_the_real_one_is_up_and_forwards_its_events() {
        let (gate_tx, gate_rx) = channel::<()>();
        let (events_tx, events_rx) = channel::<SessionEvent>();
        let sent = Arc::new(Mutex::new(Vec::new()));
        let named = Arc::new(Mutex::new(None));
        let (sent_inner, named_inner) = (sent.clone(), named.clone());
        let mut lazy = LazySession::start(move || {
            // The "process" starts only when the test says so.
            gate_rx.recv().ok();
            Ok(Box::new(Scripted {
                rx: events_rx,
                sent: sent_inner,
                named: named_inner,
            }) as Box<dyn Session + Send>)
        });
        lazy.send("first").unwrap();
        lazy.set_name("A name").unwrap();
        assert!(
            sent.lock().unwrap().is_empty(),
            "nothing goes out before the process"
        );
        gate_tx.send(()).unwrap();
        wait_for(|| (sent.lock().unwrap().len() == 1).then_some(()));
        assert_eq!(sent.lock().unwrap()[0], "first");
        assert_eq!(named.lock().unwrap().as_deref(), Some("A name"));
        lazy.send("second").unwrap();
        assert_eq!(sent.lock().unwrap().len(), 2, "live now: straight through");
        events_tx
            .send(SessionEvent::TextDelta { text: "hi".into() })
            .unwrap();
        let event = wait_for(|| lazy.events().try_recv().ok());
        assert!(matches!(event, SessionEvent::TextDelta { text } if text == "hi"));
    }

    /// A spawn that fails closes the Session with the error's words, and
    /// a later send is refused rather than queued forever.
    #[test]
    fn a_failed_spawn_closes_the_lazy_session() {
        let mut lazy = LazySession::start(|| Err(io::Error::other("no such CLI")));
        let event = wait_for(|| lazy.events().try_recv().ok());
        match event {
            SessionEvent::Closed { reason } => assert!(reason.contains("no such CLI"), "{reason}"),
            other => panic!("{other:?}"),
        }
        assert!(lazy.send("late").is_err());
    }

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
