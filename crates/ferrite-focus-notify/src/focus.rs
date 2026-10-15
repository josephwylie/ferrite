//! The Ferrite adapter.
//!
//! Upstream this module was the Herdr adapter: it read a pane's state with
//! `herdr agent get`, `workspace list`, `tab get` and `worktree list`, learned
//! which terminal app shows a workspace from the frontmost app, and focused a
//! pane through Herdr's socket when a notification was clicked.
//!
//! Ferrite is its own terminal and knows all of that in-process. The app says
//! whether the Thread's Pane is the focused one and whether its window is
//! frontmost ([`notification_decision`]), hands over the Thread's words as an
//! [`Alert`], and gets the click back through [`Notifier::post`]'s callback,
//! which lands on the Thread the way a nav row does. The rest of the crate
//! (event copy, icons, git label, the alerter script, cleared markers) is
//! upstream's, unchanged in what it does.

use std::path::PathBuf;

/// Whether a notification is worth showing, and whether it needs watching
/// until the pane is seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationDecision {
    Skip,
    Send,
    SendWithVisibilityMonitor,
}

/// The two agent statuses that notify: the only ones needing the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Blocked,
    Done,
}

impl Status {
    /// The status as upstream's events spell it.
    fn as_event_status(self) -> &'static str {
        match self {
            Status::Blocked => "blocked",
            Status::Done => "done",
        }
    }
}

/// One Thread's alert, as Ferrite hands it over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    /// The notification's identity (upstream's pane id): a newer alert with
    /// the same id replaces the older one, and withdrawing it takes it back.
    pub pane_id: String,
    pub status: Status,
    /// The provider the Thread runs on, as the subtitle names it and its icon
    /// is picked by.
    pub agent: Option<String>,
    /// The Thread's title (upstream's tab label), joined to the status in the
    /// notification's title.
    pub thread: String,
    /// What the Thread is about now (upstream's event title): the request, or
    /// the turn's answer. Upstream's static copy stands in when it is empty.
    pub detail: Option<String>,
    /// The Thread's directory on this computer, for the git label.
    pub cwd: Option<PathBuf>,
}

/// Bundle-id stand-ins for [`notification_decision_from_bundles`]. Ferrite is
/// the terminal bound to every Thread, so the frontmost app is Ferrite exactly
/// when its window is active.
const FERRITE: &str = "ferrite";
const ELSEWHERE: &str = "elsewhere";

/// Whether to post a notification for a Thread: upstream's rule, with the
/// focused Pane standing for Herdr's focused pane and Ferrite's window for the
/// bound terminal.
pub fn notification_decision(focused: bool, window_active: bool) -> NotificationDecision {
    // An unfocused pane always sends, so the binding and the frontmost app
    // are not worth looking up.
    if !focused {
        return NotificationDecision::Send;
    }
    notification_decision_from_bundles(
        Some(FERRITE.to_string()),
        Some(if window_active { FERRITE } else { ELSEWHERE }.to_string()),
    )
}

/// The decision for a pane Herdr calls focused: whether the user can see it
/// depends on the frontmost app being the terminal bound to its workspace.
fn notification_decision_from_bundles(
    bound_terminal: Option<String>,
    frontmost: Option<String>,
) -> NotificationDecision {
    match (frontmost, bound_terminal) {
        (Some(frontmost), Some(bound)) if frontmost == bound => NotificationDecision::Skip,
        (Some(_), Some(_)) => NotificationDecision::SendWithVisibilityMonitor,
        _ => NotificationDecision::Send,
    }
}

#[cfg(target_os = "macos")]
pub use delivery::{cleanup, Notifier};

/// Delivery through alerter, macOS only as upstream is.
#[cfg(target_os = "macos")]
mod delivery {
    use std::collections::HashMap;
    use std::io::{self, Read};
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::Alert;
    use crate::event::{enrich_notification, notification_from_event_json};
    use crate::notification::{FocusNotification, PaneLabels};
    use crate::notifier::{remove_notification, resolve_notifier_bin};
    use crate::script::{
        rewrite_generated_scripts_without_activation, write_focus_script, CLICKED,
    };
    use crate::state::{
        cleanup_stale_state_files, mark_notification_cleared, reset_notification_clearance,
    };
    use crate::util::command_stdout_until;

    /// Time the git calls behind one notification get, together (upstream's
    /// `LOOKUP_BUDGET`): a pathological repository costs at most this much
    /// delay, and the notification goes out with whatever was answered.
    pub(super) const LOOKUP_BUDGET: Duration = Duration::from_secs(2);

    /// Removes stale generated state: upstream's `--cleanup`, which Herdr ran
    /// at plugin startup and Ferrite runs at launch.
    pub fn cleanup() -> io::Result<()> {
        cleanup_stale_state_files()?;
        rewrite_generated_scripts_without_activation()
    }

    /// Posts and withdraws notifications through alerter. Clones share the
    /// notifications they have up.
    #[derive(Debug, Clone)]
    pub struct Notifier {
        notifier_bin: String,
        /// Pane ids with a post still waiting on the operator, and how many:
        /// a newer post can replace one whose thread has not finished yet.
        standing: Arc<Mutex<HashMap<String, usize>>>,
    }

    impl Notifier {
        /// The installed alerter, or why there is none.
        pub fn find() -> Result<Self, String> {
            resolve_notifier_bin().map(Self::new)
        }

        pub(super) fn new(notifier_bin: String) -> Self {
            Self {
                notifier_bin,
                standing: Arc::default(),
            }
        }

        /// Shows `alert`, replacing any standing notification with its pane
        /// id. Everything slow (git, writing the script, alerter itself, which
        /// waits for the operator) runs on a thread of its own; `clicked` runs
        /// there if the operator clicks the notification, and never otherwise.
        pub fn post(&self, alert: Alert, clicked: impl FnOnce() + Send + 'static) {
            // Reset here rather than on the thread: a withdraw that follows at
            // once must find the marker this post would otherwise clear.
            let _ = reset_notification_clearance(&alert.pane_id);
            *self
                .standing
                .lock()
                .unwrap()
                .entry(alert.pane_id.clone())
                .or_default() += 1;
            let (notifier_bin, standing) = (self.notifier_bin.clone(), self.standing.clone());
            std::thread::spawn(move || {
                let clicked_it = deliver(&alert, &notifier_bin) == Ok(true);
                let mut standing = standing.lock().unwrap();
                if let Some(count) = standing.get_mut(&alert.pane_id) {
                    *count -= 1;
                    if *count == 0 {
                        standing.remove(&alert.pane_id);
                    }
                }
                drop(standing);
                if clicked_it {
                    clicked();
                }
            });
        }

        /// Takes the pane's notification back, and keeps one still queued
        /// from appearing (upstream's cleared marker, set when the pane was
        /// focused in its terminal).
        pub fn withdraw(&self, pane_id: &str) {
            let _ = mark_notification_cleared(pane_id);
            let (pane_id, notifier_bin) = (pane_id.to_string(), self.notifier_bin.clone());
            std::thread::spawn(move || {
                let _ = remove_notification(&pane_id, &notifier_bin);
            });
        }

        /// Takes back every notification still up, before returning. A click
        /// comes back through the process that posted it (where upstream's
        /// exec'd a helper of its own), so once that process is gone the click
        /// would land nowhere: the app calls this as it quits.
        pub fn withdraw_all(&self) {
            let pane_ids: Vec<String> = self.standing.lock().unwrap().keys().cloned().collect();
            for pane_id in pane_ids {
                let _ = mark_notification_cleared(&pane_id);
                let _ = remove_notification(&pane_id, &self.notifier_bin);
            }
        }
    }

    /// Shows the notification and waits for the operator: whether they
    /// clicked it. None of the alert's words is required; upstream's copy
    /// fills what is missing.
    fn deliver(alert: &Alert, notifier_bin: &str) -> Result<bool, String> {
        let Some(mut notification) = notification_for(alert)? else {
            return Ok(false);
        };
        enrich_notification(
            &mut notification,
            &pane_labels(alert, Instant::now() + LOOKUP_BUDGET),
        );
        // Ferrite takes a seen Thread's notification back itself (`withdraw`),
        // so the script never polls for visibility.
        let script = write_focus_script(&notification, notifier_bin, false, false)
            .map_err(|err| format!("failed to write focus script: {err}"))?;
        run_reporting_click(&script)
    }

    /// The notification upstream builds from Herdr's
    /// `pane.agent_status_changed` event: the alert, spoken as that event.
    pub(super) fn notification_for(alert: &Alert) -> Result<Option<FocusNotification>, String> {
        let event = serde_json::json!({
            "event": "pane.agent_status_changed",
            "data": {
                "pane_id": alert.pane_id,
                "agent_status": alert.status.as_event_status(),
                "agent": alert.agent,
                "title": alert.detail,
            },
        });
        notification_from_event_json(&event.to_string())
    }

    /// What the notification names besides the event: the Thread's title in
    /// the title, and the git state of its directory in the subtitle.
    pub(super) fn pane_labels(alert: &Alert, deadline: Instant) -> PaneLabels {
        // Without a branch there is nothing to attribute the changes to, so
        // the git probe is not worth its time (upstream's rule).
        let git = alert.cwd.as_deref().and_then(Path::to_str).and_then(|cwd| {
            let branch = worktree_branch(cwd, deadline)?;
            Some(crate::git::label(
                &branch,
                crate::git::changes(cwd, deadline),
            ))
        });
        PaneLabels {
            workspace: None,
            tab: Some(alert.thread.clone()),
            terminal_title: None,
            git,
        }
    }

    /// The branch checked out in `cwd`'s worktree. Like upstream's Herdr
    /// lookup it still names the branch of a repository with no commits, and
    /// a detached `HEAD` or a directory outside a repository has none.
    pub(super) fn worktree_branch(cwd: &str, deadline: Instant) -> Option<String> {
        let branch = command_stdout_until(
            "git",
            &[
                "--no-optional-locks",
                "-C",
                cwd,
                "symbolic-ref",
                "--quiet",
                "--short",
                "HEAD",
            ],
            &[("LC_ALL", "C")],
            deadline,
        )?;
        let branch = branch.trim();
        (!branch.is_empty()).then(|| branch.to_string())
    }

    /// Runs the generated script and waits for it: whether the operator
    /// clicked. Where upstream detached the script and let a click exec the
    /// focus helper, Ferrite waits on it, so the click comes back in-process.
    pub(super) fn run_reporting_click(script: &Path) -> Result<bool, String> {
        let mut child = Command::new("sh")
            .arg(script)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("failed to run notification script: {err}"))?;
        let mut stdout = String::new();
        if let Some(mut pipe) = child.stdout.take() {
            let _ = pipe.read_to_string(&mut stdout);
        }
        let status = child
            .wait()
            .map_err(|err| format!("failed to wait for notification script: {err}"))?;
        if !status.success() {
            return Err(format!("notification script exited with {status}"));
        }
        Ok(stdout.lines().any(|line| line == CLICKED))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decides_like_upstream_with_ferrite_as_the_bound_terminal() {
        // Focused Pane, Ferrite frontmost: the operator is looking at it.
        assert_eq!(
            notification_decision(true, true),
            NotificationDecision::Skip
        );
        // Focused Pane, Ferrite in the background: sent, and taken back once
        // the Pane is seen.
        assert_eq!(
            notification_decision(true, false),
            NotificationDecision::SendWithVisibilityMonitor
        );
        // Another Pane focused: sent, whether or not Ferrite is frontmost.
        assert_eq!(
            notification_decision(false, true),
            NotificationDecision::Send
        );
        assert_eq!(
            notification_decision(false, false),
            NotificationDecision::Send
        );
    }

    #[test]
    fn decides_when_to_skip_or_monitor_notifications() {
        // Pane focused + frontmost matches the bound terminal -> skip.
        assert_eq!(
            notification_decision_from_bundles(
                Some("com.example.Herdr".to_string()),
                Some("com.example.Herdr".to_string())
            ),
            NotificationDecision::Skip
        );
        // Pane focused + frontmost outside the bound terminal -> notify, with
        // a visibility monitor since a terminal is bound.
        assert_eq!(
            notification_decision_from_bundles(
                Some("com.example.Herdr".to_string()),
                Some("com.apple.Terminal".to_string())
            ),
            NotificationDecision::SendWithVisibilityMonitor
        );
        // Pane focused + frontmost with no bound terminal -> notify.
        assert_eq!(
            notification_decision_from_bundles(None, Some("com.google.Chrome".to_string())),
            NotificationDecision::Send
        );
        // Pane focused + frontmost unknown -> notify (conservative).
        assert_eq!(
            notification_decision_from_bundles(Some("com.example.Herdr".to_string()), None),
            NotificationDecision::Send
        );
    }

    #[cfg(target_os = "macos")]
    mod delivery {
        use std::fs;
        use std::path::{Path, PathBuf};
        use std::time::{Duration, Instant};

        use super::super::delivery::*;
        use super::super::{Alert, Status};
        use crate::event::enrich_notification;
        use crate::script::write_focus_script;
        use crate::state::{mark_notification_cleared, reset_notification_clearance};

        fn alert(pane_id: &str, status: Status) -> Alert {
            isolate_state();
            Alert {
                pane_id: pane_id.to_string(),
                status,
                agent: Some("Codex".to_string()),
                thread: "Release".to_string(),
                detail: Some("Bash wants to run cargo publish".to_string()),
                cwd: None,
            }
        }

        /// Points the crate's state at a scratch directory of this test run,
        /// so scripts, icons, markers and stale-file cleanup never touch the
        /// directory a running Ferrite uses (upstream's tests set
        /// `HERDR_PLUGIN_STATE_DIR` on each process they spawn).
        fn isolate_state() {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| {
                let dir = std::env::temp_dir().join(format!(
                    "ferrite-focus-notify-adapter-state-{}",
                    std::process::id()
                ));
                std::env::set_var("FERRITE_FOCUS_NOTIFY_STATE_DIR", dir);
            });
        }

        /// A scratch directory removed when the test ends, pass or fail.
        struct TempDir(PathBuf);

        impl TempDir {
            fn new(name: &str) -> Self {
                let dir = std::env::temp_dir().join(format!(
                    "ferrite-focus-notify-adapter-{name}-{}",
                    std::process::id()
                ));
                let _ = fs::remove_dir_all(&dir);
                fs::create_dir_all(&dir).unwrap();
                Self(dir)
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        fn write_executable(path: &Path, content: &str) {
            fs::write(path, content).unwrap();
            let mut permissions = fs::metadata(path).unwrap().permissions();
            std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o700);
            fs::set_permissions(path, permissions).unwrap();
        }

        /// A stand-in for alerter that logs its arguments and answers as the
        /// operator would have.
        fn fake_alerter(dir: &Path, answer: &str) -> (PathBuf, PathBuf) {
            let log = dir.join("alerter.log");
            let bin = dir.join("alerter");
            write_executable(
                &bin,
                &format!(
                    "#!/bin/sh\nfor arg in \"$@\"; do printf '%s\\n' \"$arg\" >> '{}'; done\nprintf '%s' '{answer}'\n",
                    log.display()
                ),
            );
            (bin, log)
        }

        /// A repository on `main` with one uncommitted change.
        fn repo_with_a_change(dir: &Path) -> PathBuf {
            let repo = dir.join("repo");
            fs::create_dir_all(&repo).unwrap();
            let git = |args: &[&str]| {
                let status = std::process::Command::new("git")
                    .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                    .args(args)
                    .current_dir(&repo)
                    .status()
                    .unwrap();
                assert!(status.success(), "git {args:?}");
            };
            git(&["init", "--quiet", "--initial-branch=main"]);
            fs::write(repo.join("file"), "one\n").unwrap();
            git(&["add", "file"]);
            git(&["commit", "--quiet", "-m", "one"]);
            fs::write(repo.join("file"), "one\ntwo\n").unwrap();
            repo
        }

        #[test]
        fn a_blocked_thread_reads_like_upstream_s_row() {
            let alert = alert("7@box", Status::Blocked);
            let mut notification = notification_for(&alert).unwrap().unwrap();
            enrich_notification(&mut notification, &pane_labels(&alert, Instant::now()));

            assert_eq!(notification.title, "Blocked · Release");
            assert_eq!(notification.body, "Bash wants to run cargo publish");
            assert_eq!(notification.subtitle.as_deref(), Some("Codex"));
            assert_eq!(notification.group, "ferrite-7-box");
            assert!(notification
                .app_icon
                .as_deref()
                .unwrap()
                .ends_with("/icons/codex-color.png"));
        }

        #[test]
        fn a_done_thread_without_words_falls_back_to_upstream_s_copy() {
            let mut alert = alert("3", Status::Done);
            alert.detail = Some("  ".to_string());
            let notification = notification_for(&alert).unwrap().unwrap();

            assert_eq!(notification.title, "Done");
            assert_eq!(notification.body, "Open the pane to review the result.");
        }

        #[test]
        fn names_the_branch_of_a_repository_without_commits() {
            let dir = TempDir::new("branch");
            let repo = dir.0.join("repo");
            fs::create_dir_all(&repo).unwrap();
            let init = std::process::Command::new("git")
                .args(["init", "--quiet", "--initial-branch=feature/notify"])
                .current_dir(&repo)
                .status()
                .unwrap();
            assert!(init.success());
            let deadline = Instant::now() + Duration::from_secs(5);

            assert_eq!(
                worktree_branch(repo.to_str().unwrap(), deadline).as_deref(),
                Some("feature/notify")
            );
            assert_eq!(worktree_branch(dir.0.to_str().unwrap(), deadline), None);

            let mut alert = alert("4", Status::Done);
            alert.cwd = Some(repo);
            let labels = pane_labels(&alert, deadline);
            assert_eq!(labels.git.as_deref(), Some("feature/notify"));
        }

        #[test]
        fn a_hanging_change_count_delays_the_notification_by_the_budget_at_most() {
            let dir = TempDir::new("hang");
            let repo = repo_with_a_change(&dir.0);
            // `git diff` asks the fsmonitor hook what changed, and this one
            // never answers in time. The trailing `:` keeps the shell from
            // exec-ing into `sleep`, so the hang is a descendant (upstream's).
            let hook = dir.0.join("hang");
            write_executable(&hook, "#!/bin/sh\nsleep 30; :\n");
            let status = std::process::Command::new("git")
                .args(["config", "core.fsmonitor", hook.to_str().unwrap()])
                .current_dir(&repo)
                .status()
                .unwrap();
            assert!(status.success());
            let mut alert = alert("hang-1", Status::Blocked);
            alert.cwd = Some(repo);

            let started = Instant::now();
            let labels = pane_labels(&alert, started + LOOKUP_BUDGET);

            assert!(
                started.elapsed() < LOOKUP_BUDGET + Duration::from_secs(3),
                "took {:?}",
                started.elapsed()
            );
            // What answered in time is still shown: the branch, not the counts.
            assert_eq!(labels.git.as_deref(), Some("main"));
            assert_eq!(labels.tab.as_deref(), Some("Release"));
        }

        #[test]
        fn a_spent_budget_skips_git_and_still_names_the_thread() {
            let dir = TempDir::new("spent");
            let mut alert = alert("spent-1", Status::Done);
            alert.cwd = Some(repo_with_a_change(&dir.0));

            let labels = pane_labels(&alert, Instant::now());

            assert_eq!(labels.git, None);
            assert_eq!(labels.tab.as_deref(), Some("Release"));
        }

        #[test]
        fn quitting_takes_back_every_notification_still_up() {
            let dir = TempDir::new("quit");
            let log = dir.0.join("alerter.log");
            let bin = dir.0.join("alerter");
            // Logs a removal; anything else stands for a notification nobody
            // has clicked yet.
            write_executable(
                &bin,
                &format!(
                    "#!/bin/sh\nif [ \"$1\" = --remove ]; then printf '%s\\n' \"$2\" >> '{}'; else sleep 5; fi\n",
                    log.display()
                ),
            );
            let notifier = Notifier::new(bin.to_str().unwrap().to_string());
            notifier.post(alert("quit-1", Status::Blocked), || {});

            notifier.withdraw_all();

            assert_eq!(fs::read_to_string(&log).unwrap(), "ferrite-quit-1\n");
            reset_notification_clearance("quit-1").unwrap();
        }

        #[test]
        fn a_click_comes_back_and_the_alerter_gets_the_row() {
            let dir = TempDir::new("click");
            let (bin, log) = fake_alerter(&dir.0, "@CONTENTCLICKED");
            let alert = alert("click-1", Status::Blocked);
            reset_notification_clearance(&alert.pane_id).unwrap();
            let mut notification = notification_for(&alert).unwrap().unwrap();
            enrich_notification(&mut notification, &pane_labels(&alert, Instant::now()));
            let script =
                write_focus_script(&notification, bin.to_str().unwrap(), false, false).unwrap();

            assert_eq!(run_reporting_click(&script), Ok(true));
            let args = fs::read_to_string(&log).unwrap();
            assert!(args.contains("--title=Blocked · Release"));
            assert!(args.contains("--message=Bash wants to run cargo publish"));
            assert!(args.contains("--subtitle=Codex"));
            assert!(args.contains("ferrite-click-1"));
        }

        #[test]
        fn a_dismissal_is_no_click() {
            let dir = TempDir::new("dismiss");
            let (bin, _) = fake_alerter(&dir.0, "Dismiss");
            let alert = alert("dismiss-1", Status::Done);
            reset_notification_clearance(&alert.pane_id).unwrap();
            let notification = notification_for(&alert).unwrap().unwrap();
            let script =
                write_focus_script(&notification, bin.to_str().unwrap(), false, false).unwrap();

            assert_eq!(run_reporting_click(&script), Ok(false));
        }

        #[test]
        fn a_withdrawn_notification_still_queued_never_shows() {
            let dir = TempDir::new("cleared");
            let (bin, log) = fake_alerter(&dir.0, "@CONTENTCLICKED");
            let alert = alert("cleared-1", Status::Done);
            let notification = notification_for(&alert).unwrap().unwrap();
            let script =
                write_focus_script(&notification, bin.to_str().unwrap(), false, false).unwrap();
            mark_notification_cleared(&alert.pane_id).unwrap();

            assert_eq!(run_reporting_click(&script), Ok(false));
            assert!(!log.exists(), "alerter never ran");
            reset_notification_clearance(&alert.pane_id).unwrap();
        }
    }
}
