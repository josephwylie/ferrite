//! The prototype's world (`spikes/terminal-native/index.html`): the one
//! roster behind every parity scene of the visual reference and behind
//! `--demo parity`. Three Projects under a disposable HOME, twelve Threads
//! in the states the prototype draws, two Groups, and a timeline written in
//! the prototype's own wall-clock times so every stamp, age and duration a
//! capture shows falls out of the fixture clock rather than being painted.
//!
//! Declared from `demo.rs` (the live demo is always built) and used by the
//! visual reference's `parity_scenes.rs` (feature `visual-reference`). The
//! builder touches nothing global: the caller owns HOME and the clock.
//!
//! Where two prototype strings imply different clock times for one event,
//! the transcript keeps the clock's time and the notice is restamped
//! (`Cockpit::fixture_backdate_notices`); each case says so where it
//! happens.

#![cfg_attr(not(feature = "visual-reference"), allow(dead_code))]

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ferrite_core::cockpit::{Cockpit, SpawnRequest, Spawner};
use ferrite_core::groups::{GroupChange, GroupId};
use ferrite_core::progress::{Phase, ProgressEvent};
use ferrite_core::providers::Session;
use ferrite_core::store::{Provider, Store};
use ferrite_core::workspace::WorkspaceChoice;
use ferrite_core::{
    Decision, DecisionAnswer, DecisionChoice, DecisionKind, Hunk, ModelInfo, PermissionModeChoice,
    QueueEvent, QueuedPrompt, RateLimitWindow, RunState, SessionEvent, ThreadId, ToolResult,
    TurnOutcome, UsageDetails, UsageScope,
};

// ------------------------------------------------------------------ clock

/// The process clock's fixture (`ferrite_core::clock::Fixture`), moved in
/// the `SystemTime`s the world is scripted in.
pub(crate) struct Fixture(ferrite_core::clock::Fixture);

impl Fixture {
    /// Freeze every visible time at `at` until dropped.
    pub(crate) fn install(at: SystemTime) -> Self {
        Fixture(ferrite_core::clock::Fixture::install(at.into()))
    }

    /// Move the clock to `at`.
    pub(crate) fn set(&self, at: SystemTime) {
        self.0.set(at.into());
    }

    /// Step the clock forward by `by`.
    #[allow(dead_code)]
    pub(crate) fn advance(&self, by: Duration) {
        self.0.advance(by);
    }
}

/// A wall-clock time of the capture's day, in seconds since local midnight:
/// the world is scripted in the times the prototype prints.
pub(crate) type Secs = i64;

pub(crate) const fn hms(h: i64, m: i64, s: i64) -> Secs {
    h * 3600 + m * 60 + s
}

const MINUTE: Secs = 60;
const HOUR: Secs = 3600;
const DAY: Secs = 24 * HOUR;

/// The capture: the bottom bar reads `7:41 pm`. The second is the one every
/// prototype string agrees with — Perf's prompt band reads `7:40 pm` while
/// its turn has run `1m04s`, so the clock stands at least four seconds into
/// the minute; thirty keeps every other stamp on its printed minute too.
pub(crate) const CAPTURE: Secs = hms(19, 41, 30);

/// Local midnight of today, from the core's own wall-clock label, so the
/// app crate needs no timezone library. Read it before a fixture clock is
/// installed: once one is, the label is the fixture's.
pub(crate) fn midnight() -> SystemTime {
    loop {
        let before = SystemTime::now();
        let label = ferrite_core::progress::clock_label();
        let after = SystemTime::now();
        let seconds = |at: SystemTime| {
            at.duration_since(UNIX_EPOCH)
                .map(|since| since.as_secs())
                .unwrap_or_default()
        };
        // A label read across a minute's edge says either minute; ask again.
        if seconds(before) / 60 != seconds(after) / 60 {
            continue;
        }
        let Some(local) = label_seconds(&label) else {
            // An unreadable label: the epoch's midnight still gives every
            // stamp its printed minute in a UTC zone, and no scene panics.
            return before - Duration::from_secs(seconds(before) % 86_400);
        };
        // Zones are whole minutes off UTC, so the second is the epoch's.
        let local = local + (seconds(before) % 60) as i64;
        return before - Duration::from_secs(local as u64);
    }
}

/// `7:41 pm` → the minute's seconds since midnight.
fn label_seconds(label: &str) -> Option<i64> {
    let (clock, meridiem) = label.trim().split_once(' ')?;
    let (hour, minute) = clock.split_once(':')?;
    let hour: i64 = hour.parse().ok()?;
    let minute: i64 = minute.parse().ok()?;
    let hour = hour % 12
        + if meridiem.eq_ignore_ascii_case("pm") {
            12
        } else {
            0
        };
    Some(hms(hour, minute, 0))
}

/// The world's clock: where midnight was, and the fixture to move.
#[derive(Clone, Copy)]
pub(crate) struct Clock<'a> {
    pub midnight: SystemTime,
    pub fixture: Option<&'a Fixture>,
}

impl Clock<'_> {
    /// The instant `secs` names (negative: before today).
    pub(crate) fn instant(&self, secs: Secs) -> SystemTime {
        if secs >= 0 {
            self.midnight + Duration::from_secs(secs as u64)
        } else {
            self.midnight - Duration::from_secs(secs.unsigned_abs())
        }
    }

    fn set(&self, secs: Secs) {
        if let Some(fixture) = self.fixture {
            fixture.set(self.instant(secs));
        }
    }
}

// ---------------------------------------------------------------- sessions

pub(crate) type Feeds = Rc<RefCell<Vec<mpsc::Sender<SessionEvent>>>>;

/// Hands every spawn a fresh channel; the world keeps the sending half.
struct Spawn(Feeds);

/// A scripted provider: it says what the timeline feeds it and nothing else.
struct Scripted {
    rx: mpsc::Receiver<SessionEvent>,
    tx: mpsc::Sender<SessionEvent>,
    provider: Provider,
}

impl Spawner for Spawn {
    fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn Session>> {
        let (tx, rx) = mpsc::channel();
        self.0.borrow_mut().push(tx.clone());
        Ok(Box::new(Scripted {
            rx,
            tx,
            provider: request.provider,
        }))
    }
}

impl Session for Scripted {
    fn events(&self) -> &mpsc::Receiver<SessionEvent> {
        &self.rx
    }
    fn send(&mut self, _: &str) -> io::Result<()> {
        Ok(())
    }
    /// The provider accepts every held prompt, as Claude's native queue does.
    fn enqueue(&mut self, id: &str, text: &str) -> io::Result<()> {
        let _ = self
            .tx
            .send(SessionEvent::Queue(QueueEvent::Accepted(QueuedPrompt {
                id: id.into(),
                client_id: id.into(),
                text: text.into(),
            })));
        Ok(())
    }
    /// A chosen effort lands on the live Session, as both CLIs allow. The
    /// model stays unsupported: chosen before the first prompt it respawns
    /// the Session fresh, which is how Nav rows gets `opus[1m]` and
    /// `medium` without a notice in its transcript.
    fn set_effort(&mut self, _: Option<&str>) -> io::Result<()> {
        Ok(())
    }
    fn permission_modes(&self) -> Vec<PermissionModeChoice> {
        let modes: &[(&str, &str)] = match self.provider {
            Provider::Claude => &[
                ("default", "Default"),
                ("acceptEdits", "Accept edits"),
                ("plan", "Plan"),
                ("bypassPermissions", "Bypass permissions"),
            ],
            Provider::Codex => &[
                ("untrusted", "Untrusted"),
                ("on-request", "On request"),
                ("never", "Never"),
            ],
        };
        modes
            .iter()
            .map(|(value, label)| PermissionModeChoice {
                value: (*value).into(),
                label: (*label).into(),
            })
            .collect()
    }
    fn interrupt(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn respond_to_decision(&mut self, _: &str, _: DecisionAnswer) -> io::Result<()> {
        Ok(())
    }
}

// ------------------------------------------------------------------- world

/// Which scene the world is built for. The roster, Projects, Groups and
/// clock never change; what differs is what a few Threads said, because
/// the prototype's views disagree about them (the wall's lines are not the
/// Group's transcript), and which Pane the capture lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    /// Nav rows jitter in Solo (also the empty board and the folded nav).
    Solo,
    /// The Perf sweep board, Pane 1 focused.
    Group,
    /// The board with the notifications list down: Bump deps' final answer
    /// is the one line the list prints (R7).
    Notes,
    /// The board with Close stale's request held back, to arrive while the
    /// window is up and toast.
    Toast,
    /// Everything at wall range: the Group's members say the wall's lines.
    Wall,
}

/// Every Thread of the world, by role.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cast {
    pub nav: ThreadId,
    pub perf: ThreadId,
    pub close: ThreadId,
    pub theme: ThreadId,
    pub fold: ThreadId,
    pub port: ThreadId,
    pub bump: ThreadId,
    pub docs: ThreadId,
    pub flaky: ThreadId,
    pub release: ThreadId,
    pub icons: ThreadId,
    pub bench: ThreadId,
}

impl Cast {
    /// Perf sweep, in board order.
    pub(crate) fn sweep(&self) -> [ThreadId; 4] {
        [self.perf, self.close, self.theme, self.fold]
    }

    /// The Threads with a live Session at capture.
    pub(crate) fn open(&self) -> [ThreadId; 9] {
        [
            self.nav, self.perf, self.close, self.theme, self.fold, self.port, self.bump,
            self.docs, self.flaky,
        ]
    }
}

/// The built world: a Cockpit over a disposable store, and what a scene
/// needs to aim at it.
pub(crate) struct World {
    pub core: Cockpit,
    pub feeds: Feeds,
    pub home: PathBuf,
    /// `~/Desktop/Projects`: where Ferrite was launched (R13).
    pub launch: PathBuf,
    pub zeron: PathBuf,
    pub cast: Cast,
    pub everything: GroupId,
    /// Close stale's approval, not yet sent: the toast scene delivers it
    /// once the window is up, so it is the one new request (R8).
    pub held: Option<(mpsc::Sender<SessionEvent>, SessionEvent)>,
}

/// What one Thread does at one moment of the timeline. A fixture's script,
/// built once per capture: an event's size is no concern here.
#[allow(clippy::large_enum_variant)]
enum Act {
    /// The operator sends a line.
    Prompt(String),
    /// The provider says something.
    Event(SessionEvent),
    /// The operator parks the Thread.
    Park,
}

struct Beat {
    at: Secs,
    who: ThreadId,
    act: Act,
}

/// The timeline under construction.
#[derive(Default)]
struct Script {
    beats: Vec<Beat>,
}

impl Script {
    fn prompt(&mut self, at: Secs, who: ThreadId, text: impl Into<String>) {
        self.beats.push(Beat {
            at,
            who,
            act: Act::Prompt(text.into()),
        });
    }

    fn ev(&mut self, at: Secs, who: ThreadId, event: SessionEvent) {
        self.beats.push(Beat {
            at,
            who,
            act: Act::Event(event),
        });
    }

    fn park(&mut self, at: Secs, who: ThreadId) {
        self.beats.push(Beat {
            at,
            who,
            act: Act::Park,
        });
    }

    /// A tool call that starts and settles.
    fn tool(
        &mut self,
        (start, end): (Secs, Secs),
        who: ThreadId,
        id: &str,
        name: &str,
        input: serde_json::Value,
        (output, is_error, result): Outcome,
    ) {
        self.ev(start, who, started(id, name, input));
        self.ev(end, who, completed(id, &output, is_error, result));
    }

    /// A shell command that ran from `start` for `ms`.
    fn bash(&mut self, start: Secs, who: ThreadId, id: &str, command: &str, run: Run) {
        let end = start + (run.ms / 1000) as Secs;
        self.tool(
            (start, end),
            who,
            id,
            "Bash",
            serde_json::json!({ "command": command }),
            (
                run.stdout.clone(),
                run.exit != 0,
                ToolResult::Command {
                    stdout: run.stdout.clone(),
                    stderr: String::new(),
                    exit_code: Some(run.exit),
                    duration_ms: Some(run.ms),
                },
            ),
        );
    }

    /// An edit the provider applied, with its structured patch.
    fn edit(&mut self, at: Secs, who: ThreadId, id: &str, path: &str, hunks: Vec<Hunk>) {
        self.tool(
            (at, at + 1),
            who,
            id,
            "Edit",
            serde_json::json!({ "file_path": path }),
            (
                String::new(),
                false,
                ToolResult::FileEdit {
                    path: path.into(),
                    hunks,
                },
            ),
        );
    }

    /// A completed turn: the provider's accounting, then its end.
    fn end(&mut self, at: Secs, who: ThreadId) {
        self.ev(
            at,
            who,
            SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            },
        );
    }
}

/// What a call handed back: its output, whether it failed, and the
/// provider's structured result.
type Outcome = (String, bool, ToolResult);

/// A call that settled with nothing to say.
fn quiet() -> Outcome {
    (String::new(), false, ToolResult::Opaque)
}

/// One command's run: what it printed, how it exited, how long it took.
struct Run {
    stdout: String,
    exit: i64,
    ms: u64,
}

/// Build the world for `look` under `root` (which must exist; it is made
/// canonical so the registry's roots and HOME agree). Nothing global is
/// touched: the caller points HOME at `World::home` and owns the clock.
pub(crate) fn build(root: &Path, look: Look, clock: Clock) -> World {
    let root = std::fs::canonicalize(root).expect("a canonical disposable root");
    let home = root.join("home");
    let launch = home.join("Desktop").join("Projects");
    let ferrite = home.join("ferrite");
    let zeron = launch.join("zeron");
    let lumen = launch.join("lumen");
    clock.set(hms(9, 0, 0));
    lay_out_ferrite(&ferrite, &home);
    lay_out_plain(&zeron, &home, "zeron");
    lay_out_plain(&lumen, &home, "lumen");
    let screenshot = home.join("Desktop").join("nav-jitter.png");
    std::fs::write(&screenshot, NAV_JITTER_PNG).expect("write the attached screenshot");

    let feeds = Feeds::default();
    let mut core = Cockpit::new(
        Store::open(root.join("store")).expect("open the disposable store"),
        Box::new(Spawn(feeds.clone())),
    );
    core.set_suggestions_enabled(false);

    // Created in the nav's order (R14: rows go in creation order).
    let roster: [(Provider, &Path, &str); 12] = [
        (
            Provider::Claude,
            ferrite.as_path(),
            "Nav rows jitter on stream start",
        ),
        (Provider::Claude, ferrite.as_path(), "Perf: layout cache"),
        (Provider::Codex, ferrite.as_path(), "Close stale issues"),
        (Provider::Claude, ferrite.as_path(), "Theme retune"),
        (Provider::Claude, ferrite.as_path(), "Fold regression"),
        (Provider::Claude, zeron.as_path(), "Port onboarding flow"),
        (Provider::Codex, zeron.as_path(), "Bump deps"),
        (Provider::Codex, zeron.as_path(), "Docs: ADR 0008"),
        (Provider::Claude, ferrite.as_path(), "Flaky provider test"),
        (Provider::Claude, ferrite.as_path(), "Release 0.5.0 notes"),
        (Provider::Codex, zeron.as_path(), "Icon set refresh"),
        (Provider::Claude, lumen.as_path(), "Bench the old renderer"),
    ];
    // Each Thread with the provider side of its latest Session.
    let mut senders: Vec<(ThreadId, mpsc::Sender<SessionEvent>)> = Vec::new();
    for (at, (provider, checkout, title)) in roster.into_iter().enumerate() {
        let thread = core
            .open(
                provider,
                WorkspaceChoice::Main {
                    checkout: checkout.to_path_buf(),
                },
            )
            .expect("open a parity Thread");
        core.rename_thread(thread, title)
            .expect("name a parity Thread");
        if at == 0 {
            // Nav rows chooses before its first prompt (R3): the effort lands
            // on the Session, then the model respawns it fresh, which also
            // clears the effort notice.
            core.set_effort(thread, Some("medium".into()))
                .expect("choose Nav rows' effort");
            core.set_model(thread, Some("opus[1m]".into()))
                .expect("choose Nav rows' model");
        }
        let sender = feeds
            .borrow()
            .last()
            .cloned()
            .expect("a parity Session spawned");
        senders.push((thread, sender));
    }
    let id = |at: usize| senders[at].0;
    let cast = Cast {
        nav: id(0),
        perf: id(1),
        close: id(2),
        theme: id(3),
        fold: id(4),
        port: id(5),
        bump: id(6),
        docs: id(7),
        flaky: id(8),
        release: id(9),
        icons: id(10),
        bench: id(11),
    };
    let feed = |thread: ThreadId| {
        senders
            .iter()
            .find(|(who, _)| *who == thread)
            .map(|(_, sender)| sender.clone())
            .expect("every parity Thread has a Session")
    };

    let mut script = Script::default();
    nav_rows(&mut script, &cast, &screenshot);
    let wall = look == Look::Wall;
    if wall {
        wall_perf(&mut script, &cast);
        wall_close(&mut script, &cast);
        wall_theme(&mut script, &cast);
        wall_fold(&mut script, &cast);
    } else {
        perf(&mut script, &cast);
        close_stale(&mut script, &cast);
        theme(&mut script, &cast);
        fold(&mut script, &cast);
    }
    port(&mut script, &cast);
    bump(&mut script, &cast, look);
    docs(&mut script, &cast);
    flaky(&mut script, &cast);
    parked(&mut script, &cast);

    // Close stale's approval is the last thing it says; the toast scene
    // keeps it for the window to receive.
    let mut held = None;
    if look == Look::Toast {
        let at = script
            .beats
            .iter()
            .rposition(|beat| {
                beat.who == cast.close
                    && matches!(
                        &beat.act,
                        Act::Event(SessionEvent::DecisionRequested { .. })
                    )
            })
            .expect("Close stale asks");
        if let Act::Event(event) = script.beats.remove(at).act {
            held = Some((feed(cast.close), event));
        }
    }

    // The day is played with Nav rows focused: the pump reads the focused
    // Thread's notices, and the two requests must arrive unread.
    core.focus_thread(cast.nav);
    // Play the day in order: the clock first, then the act, then a pump so
    // the act lands on its own second.
    script.beats.sort_by_key(|beat| beat.at);
    for beat in script.beats {
        clock.set(beat.at);
        match beat.act {
            Act::Prompt(text) => core.send(beat.who, text),
            Act::Event(event) => feed(beat.who)
                .send(event)
                .expect("a parity Session is listening"),
            Act::Park => core.park(beat.who).expect("park a parity Thread"),
        }
        core.pump();
        core.pump();
    }
    clock.set(CAPTURE);
    core.pump();
    // Fold regression's first turn failed its suite and went idle without a
    // turn end of its own (no stamp in the prototype's Pane), then resumed;
    // its finish is the list's `failed` row all the same (FL-16).
    core.fixture_notice(cast.fold, TurnOutcome::Completed);

    // The Groups, in creation order: Perf sweep first, so a member's Group
    // is Perf sweep (R1); Everything holds it again by inclusion.
    let group = |core: &mut Cockpit, first, second, rest: &[ThreadId], title: &str| {
        let group = core
            .apply_group(GroupChange::Create { first, second })
            .expect("create a parity Group")
            .group
            .expect("a new Group");
        for thread in rest {
            core.apply_group(GroupChange::Join {
                thread: *thread,
                group,
                index: None,
            })
            .expect("join a parity Group");
        }
        core.apply_group(GroupChange::Rename {
            group,
            title: title.into(),
        })
        .expect("name a parity Group");
        group
    };
    group(
        &mut core,
        cast.perf,
        cast.close,
        &[cast.theme, cast.fold],
        "Perf sweep",
    );
    // Everything is home only to the two ferrite Threads no other Group
    // holds (Flaky provider test, Release notes); the sweep's members and
    // zeron's three are its guests (R1, `GroupChange::Include`), so zeron's
    // Threads stay loose rows under zeron and the Group sits under ferrite,
    // the Project of its first member. Its order is the wall's.
    let everything = group(&mut core, cast.flaky, cast.release, &[], "Everything");
    let guests = cast
        .sweep()
        .into_iter()
        .chain([cast.port, cast.bump, cast.docs]);
    for (index, thread) in guests.enumerate() {
        core.apply_group(GroupChange::Include {
            thread,
            group: everything,
            index: Some(index),
        })
        .expect("Everything includes the sweep's and zeron's Threads");
    }

    // The notifications list holds exactly the prototype's rows: Theme
    // retune and Bump deps done, Fold regression failed (FL-16), all read;
    // Close stale's and Flaky provider test's requests unread. Every other
    // completion — Nav rows' two turns, the parked Threads' — is dismissed.
    let keep = [cast.theme, cast.bump, cast.fold];
    let dismissed: Vec<_> = core
        .notifications()
        .notices()
        .filter(|notice| !keep.contains(&notice.thread))
        .map(|notice| notice.id)
        .collect();
    for id in dismissed {
        core.dismiss_notice(id);
    }
    for thread in [cast.theme, cast.bump, cast.fold, cast.nav] {
        core.focus_thread(thread);
    }
    // Theme retune's turn ended at 7:33 pm (its stamp), eight minutes
    // before capture; the list reads `12m`.
    core.fixture_backdate_notices(cast.theme, clock.instant(CAPTURE - 12 * MINUTE));
    // Fold regression's run failed after its 7:34 pm prompt; the list reads
    // `9m`, earlier than that prompt.
    core.fixture_backdate_notices(cast.fold, clock.instant(CAPTURE - 9 * MINUTE));
    // Flaky provider test asked after Close stale did (the nav's needs-you
    // strip lists Close stale first, in arrival order); the list ages it
    // `5m`, older than Close stale's `2m`.
    core.fixture_backdate_notices(cast.flaky, clock.instant(CAPTURE - 5 * MINUTE));

    match look {
        Look::Solo => {
            core.focus_thread(cast.nav);
        }
        Look::Group | Look::Notes | Look::Toast => {
            core.focus_thread(cast.perf);
        }
        Look::Wall => {
            core.focus_thread(cast.perf);
            core.enter_group(everything).expect("enter Everything");
        }
    }
    core.pump();

    // Recency is the log's mtime (`Store::last_used`). The recent list
    // reads Nav rows 12m, Fold 20m, Bump 1h and Release 2h; everyone else
    // is older (R13). Nav rows' last turn ended 7:32 pm, nine minutes ago:
    // its log is restamped to the twelve the list prints.
    // The hover card's footer ages nav.rs `2 min ago`, though the Thread's
    // last edit to it was at 7:31 pm; where the age is read off the file,
    // the file says two minutes.
    if let Ok(file) = std::fs::File::options()
        .append(true)
        .open(ferrite.join("crates/ferrite/src/nav.rs"))
    {
        let _ = file.set_modified(clock.instant(CAPTURE - 2 * MINUTE));
    }
    let store = root.join("store");
    for (thread, ago) in [
        (cast.nav, 12 * MINUTE),
        (cast.fold, 20 * MINUTE),
        (cast.bump, HOUR),
        (cast.release, 2 * HOUR),
        (cast.perf, 3 * HOUR),
        (cast.close, 3 * HOUR + 10 * MINUTE),
        (cast.theme, 3 * HOUR + 20 * MINUTE),
        (cast.port, 3 * HOUR + 30 * MINUTE),
        (cast.docs, 3 * HOUR + 40 * MINUTE),
        (cast.flaky, 3 * HOUR + 50 * MINUTE),
        (cast.icons, DAY),
        (cast.bench, 3 * DAY),
    ] {
        let log = store.join(thread.to_string()).join("log.jsonl");
        if let Ok(file) = std::fs::File::options().append(true).open(&log) {
            let _ = file.set_modified(clock.instant(CAPTURE - ago));
        }
    }

    World {
        core,
        feeds,
        home,
        launch,
        zeron,
        cast,
        everything,
        held,
    }
}

// ------------------------------------------------------------ the checkouts

/// Run git in `dir` as the fixture's own user, under the disposable HOME:
/// the operator's config, hooks and signing never apply.
fn git(dir: &Path, home: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=Ferrite",
            "-c",
            "user.email=fixture@ferrite.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(dir)
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_DATE", "2026-10-04T09:00:00")
        .env("GIT_COMMITTER_DATE", "2026-10-04T09:00:00")
        .status();
    if !matches!(status, Ok(status) if status.success()) {
        eprintln!(
            "ferrite: parity fixture: git {args:?} failed in {}",
            dir.display()
        );
    }
}

fn write_lines(path: &Path, lines: &[String]) {
    std::fs::create_dir_all(path.parent().expect("a file has a directory"))
        .expect("create a fixture directory");
    let mut text = lines.join("\n");
    text.push('\n');
    std::fs::write(path, text).expect("write a fixture file");
}

/// `~/ferrite`: default branch `main`, checked out on `dev`, with the three
/// files the scenes edit committed as they were and the working tree as
/// the Threads left it.
fn lay_out_ferrite(ferrite: &Path, home: &Path) {
    std::fs::create_dir_all(ferrite).expect("create ~/ferrite");
    let src = ferrite.join("crates/ferrite/src");
    let nav = nav_before();
    let canvas = canvas_before();
    let theme = theme_before();
    write_lines(&src.join("nav.rs"), &nav);
    write_lines(&src.join("canvas.rs"), &canvas);
    write_lines(&src.join("theme.rs"), &theme);
    std::fs::write(ferrite.join("README.md"), "# ferrite\n").expect("write README");
    git(ferrite, home, &["init", "-q", "-b", "main"]);
    git(ferrite, home, &["add", "-A"]);
    git(ferrite, home, &["commit", "-q", "-m", "ferrite 0.4.0"]);
    git(ferrite, home, &["checkout", "-q", "-b", "dev"]);
    let nav_after = apply(&apply(&nav, &[nav_edit_one()]), &[nav_edit_two()]);
    write_lines(&src.join("nav.rs"), &nav_after);
    write_lines(
        &src.join("canvas.rs"),
        &apply(&canvas, &canvas_hunks(&canvas)),
    );
    write_lines(&src.join("theme.rs"), &apply(&theme, &theme_hunks()));
}

/// A Project with nothing but a first commit on `main`.
fn lay_out_plain(dir: &Path, home: &Path, name: &str) {
    std::fs::create_dir_all(dir).expect("create a fixture Project");
    std::fs::write(dir.join("README.md"), format!("# {name}\n")).expect("write README");
    git(dir, home, &["init", "-q", "-b", "main"]);
    git(dir, home, &["add", "-A"]);
    git(dir, home, &["commit", "-q", "-m", "first"]);
}

// ---------------------------------------------------------------- events

fn started(id: &str, name: &str, input: serde_json::Value) -> SessionEvent {
    SessionEvent::ToolStarted {
        id: id.into(),
        name: name.into(),
        input,
    }
}

fn completed(id: &str, output: &str, is_error: bool, result: ToolResult) -> SessionEvent {
    SessionEvent::ToolCompleted {
        id: id.into(),
        output: output.into(),
        is_error,
        result,
    }
}

fn text(text: &str) -> SessionEvent {
    SessionEvent::TextDelta { text: text.into() }
}

fn init(session: &str, model: &str) -> SessionEvent {
    SessionEvent::Init {
        session_id: session.into(),
        model: model.into(),
    }
}

fn mode(mode: &str) -> SessionEvent {
    SessionEvent::PermissionMode { mode: mode.into() }
}

/// Occupancy alone: the context meter, no turn accounting.
fn context(total: u64, window: u64) -> SessionEvent {
    SessionEvent::ContextUsage {
        total_tokens: total,
        context_window: Some(window),
    }
}

/// A turn's accounting as Claude reports it: what it read and wrote, and
/// the context it now occupies.
fn tokens(total: u64, input: u64, output: u64, window: u64) -> [SessionEvent; 2] {
    [
        SessionEvent::TokenUsage {
            total_tokens: total,
            input_tokens: input,
            cached_input_tokens: total.saturating_sub(input + output),
            output_tokens: output,
            reasoning_output_tokens: 0,
            context_window: Some(window),
        },
        SessionEvent::UsageDetails {
            details: UsageDetails {
                scope: UsageScope::Turn,
                input_tokens: input,
                cached_input_tokens: total.saturating_sub(input + output),
                output_tokens: output,
                reasoning_output_tokens: 0,
            },
        },
    ]
}

fn limits(five_hour: f32, weekly: f32) -> SessionEvent {
    let window = |used_fraction| {
        Some(RateLimitWindow {
            used_fraction,
            resets_at: None,
        })
    };
    SessionEvent::RateLimits {
        five_hour: window(five_hour),
        weekly: window(weekly),
    }
}

fn phase(detail: &str) -> SessionEvent {
    SessionEvent::Progress {
        event: ProgressEvent::Phase {
            phase: Phase::Working,
            detail: detail.into(),
        },
    }
}

/// A Claude `Read`'s structured result (`toolUseResult.file`).
fn read_result(path: &str, lines: &[String], total: usize) -> Outcome {
    let content = lines.join("\n");
    (
        numbered(lines),
        false,
        ToolResult::Structured {
            value: serde_json::json!({
                "type": "text",
                "file": {
                    "filePath": path,
                    "content": content,
                    "numLines": total,
                    "startLine": 1,
                    "totalLines": total,
                }
            }),
            duration_ms: None,
        },
    )
}

/// What `Read` hands the model: the lines, numbered.
fn numbered(lines: &[String]) -> String {
    lines
        .iter()
        .enumerate()
        .map(|(at, line)| format!("{:>6}\u{2192}{line}", at + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Codex asking to run a command, with its own standing answer for the
/// command's family (`acceptWithExecpolicyAmendment`).
fn approval(id: &str, command: &str, family: &[&str]) -> SessionEvent {
    let amendment = serde_json::json!({
        "acceptWithExecpolicyAmendment": { "execpolicy_amendment": family }
    });
    SessionEvent::DecisionRequested {
        decision: Decision {
            delivery: Default::default(),
            kind: DecisionKind::Approval,
            policy: Default::default(),
            id: format!("{id}-approval"),
            tool_use_id: id.into(),
            tool_name: "Bash".into(),
            description: command.into(),
            input: serde_json::json!({
                "itemId": id,
                "command": command,
                "reason": null,
                "proposedExecpolicyAmendment": family,
                "availableDecisions": ["accept", amendment.clone(), "decline", "cancel"],
            }),
            suggestions: vec![DecisionChoice {
                label: format!("Always allow {}", family.join(" ")),
                value: amendment,
                standing: true,
            }],
        },
    }
}

/// A Claude `AskUserQuestion` with one single-select question.
fn question(id: &str, question: &str, options: &[(&str, &str)]) -> SessionEvent {
    let input = serde_json::json!({
        "questions": [{
            "question": question,
            "header": "Fix",
            "multiSelect": false,
            "options": options
                .iter()
                .map(|(label, description)| serde_json::json!({
                    "label": label,
                    "description": description,
                }))
                .collect::<Vec<_>>(),
        }]
    });
    SessionEvent::DecisionRequested {
        decision: Decision {
            delivery: Default::default(),
            kind: DecisionKind::Questions(
                ferrite_core::questions::parse(&input).expect("a parity question"),
            ),
            policy: Default::default(),
            id: format!("{id}-question"),
            tool_use_id: id.into(),
            tool_name: "AskUserQuestion".into(),
            description: String::new(),
            input,
            suggestions: vec![],
        },
    }
}

/// The Claude model menu Nav rows' Session announces: the catalog's rows,
/// on the four-step ladder the picker draws.
fn claude_menu() -> Vec<ModelInfo> {
    ferrite_core::providers::models::fallback(Provider::Claude)
        .into_iter()
        .map(|mut row| {
            if !row.efforts.is_empty() {
                row.efforts = ["low", "medium", "high", "max"]
                    .into_iter()
                    .map(String::from)
                    .collect();
            }
            row
        })
        .collect()
}

// ---------------------------------------------------------------- scripts

/// Nav rows jitter on stream start: the Solo transcript, two turns, byte
/// for byte the prototype's (index.html L489-606).
fn nav_rows(s: &mut Script, cast: &Cast, screenshot: &Path) {
    let who = cast.nav;
    let start = hms(19, 18, 50);
    s.ev(start, who, init("parity-nav", "claude-opus-5-5[1m]"));
    s.ev(start, who, mode("acceptEdits"));
    s.ev(
        start,
        who,
        SessionEvent::Models {
            models: claude_menu(),
        },
    );
    s.prompt(
        start,
        who,
        ferrite_core::prompt_files::compose(
            "Nav rows jitter when a thread starts streaming: the whole tree shifts down under \
             the pointer. Screenshot attached.",
            &[screenshot.to_path_buf()],
        ),
    );
    let nav = nav_before();
    s.tool(
        (hms(19, 18, 52), hms(19, 18, 53)),
        who,
        "toolu_nav_read",
        "Read",
        serde_json::json!({ "file_path": NAV_PATH }),
        read_result(NAV_PATH, &nav, nav.len()),
    );
    let matches = [
        "crates/ferrite/src/nav.rs:212:        row = row.child(status_line(summary));",
        "crates/ferrite/src/nav.rs:388:fn status_line(text: SharedString) -> Div {",
        "crates/ferrite/src/nav.rs:402:    status_line(facts.summary.clone().unwrap_or_default())",
        "crates/ferrite/src/facts.rs:96:    /// The row's status line, once the Session has said something.",
        "crates/ferrite/src/facts.rs:141:            summary: status_line_text(&transcript),",
        "crates/ferrite/src/facts.rs:233:fn status_line_text(transcript: &Transcript) -> Option<String> {",
    ];
    s.tool(
        (hms(19, 18, 54), hms(19, 18, 55)),
        who,
        "toolu_nav_grep",
        "Grep",
        serde_json::json!({
            "pattern": "status_line",
            "path": "crates/ferrite/src",
            "output_mode": "content",
            "-n": true,
        }),
        (
            matches.join("\n"),
            false,
            ToolResult::Structured {
                value: serde_json::json!({
                    "mode": "content",
                    "numFiles": 2,
                    "filenames": [],
                    "content": matches.join("\n"),
                    "numLines": 6,
                }),
                duration_ms: None,
            },
        ),
    );
    s.ev(hms(19, 18, 56), who, text(ANSWER_ONE));
    for event in tokens(298_000, 1_800, 620, 1_000_000) {
        s.ev(hms(19, 19, 1), who, event);
    }
    s.end(hms(19, 19, 2), who);

    // Turn two: 7:31 pm, 41 seconds, ending 7:32 pm.
    let start = hms(19, 31, 30);
    s.prompt(start, who, "Yes, apply it and run the nav tests.");
    s.edit(
        start + 2,
        who,
        "toolu_nav_edit_1",
        NAV_PATH,
        vec![nav_edit_one()],
    );
    s.bash(
        start + 3,
        who,
        "toolu_nav_test_1",
        "cargo test -p ferrite nav::",
        Run {
            stdout: nav_tests_failing(),
            exit: 101,
            ms: 18_000,
        },
    );
    s.ev(
        start + 22,
        who,
        text(
            "One regression: parked rows picked up the reserved line too. Parked rows have no \
             live facts, so the reservation should apply to open Threads only.\n\n",
        ),
    );
    s.edit(
        start + 25,
        who,
        "toolu_nav_edit_2",
        NAV_PATH,
        vec![nav_edit_two()],
    );
    s.bash(
        start + 27,
        who,
        "toolu_nav_test_2",
        "cargo test -p ferrite nav::",
        Run {
            stdout: passing("nav", &NAV_TESTS, 38, "0.39s"),
            exit: 0,
            ms: 9_000,
        },
    );
    s.ev(start + 37, who, text(ANSWER_TWO));
    for event in tokens(320_000, 3_200, 1_100, 1_000_000) {
        s.ev(start + 40, who, event);
    }
    s.end(start + 41, who);
    // The account's windows, as Claude last reported them: the bar shows
    // the fuller one, `5h 41%`.
    s.ev(start + 41, who, limits(0.41, 0.23));
}

/// Perf: layout cache, Pane 1 of the board: working for 1m04s, its suite
/// 22 seconds into a run of 357 (index.html L624-656).
fn perf(s: &mut Script, cast: &Cast) {
    let who = cast.perf;
    let start = CAPTURE - 64;
    s.ev(start, who, init("parity-perf", "claude-opus-5-5"));
    s.ev(start, who, mode("acceptEdits"));
    s.prompt(
        start,
        who,
        "Perf: layout cache. The glyph atlas rebuilds every frame on a 12-pane board; keep it \
         per-cell.",
    );
    let canvas = canvas_before();
    s.tool(
        (start + 2, start + 3),
        who,
        "toolu_perf_read",
        "Read",
        serde_json::json!({ "file_path": CANVAS_PATH }),
        read_result(CANVAS_PATH, &canvas, canvas.len()),
    );
    s.ev(start + 5, who, text(PERF_PROSE));
    s.ev(start + 5, who, text("\n\n"));
    s.edit(
        start + 24,
        who,
        "toolu_perf_edit",
        CANVAS_PATH,
        canvas_hunks(&canvas),
    );
    running_suite(s, who);
}

/// The wall's Perf tile: `Update(canvas.rs) +31 −9`, the suite running,
/// `└ 212/357 tests` (index.html W[0]).
fn wall_perf(s: &mut Script, cast: &Cast) {
    let who = cast.perf;
    let start = CAPTURE - 64;
    s.ev(start, who, init("parity-perf", "claude-opus-5-5"));
    s.ev(start, who, mode("acceptEdits"));
    s.prompt(
        start,
        who,
        "Perf: layout cache. The glyph atlas rebuilds every frame on a 12-pane board; keep it \
         per-cell.",
    );
    s.tool(
        (start + 2, start + 3),
        who,
        "toolu_perf_read",
        "Read",
        serde_json::json!({ "file_path": "canvas.rs" }),
        quiet(),
    );
    s.ev(start + 5, who, text(PERF_PROSE));
    s.ev(start + 5, who, text("\n\n"));
    let canvas = canvas_before();
    s.edit(
        start + 24,
        who,
        "toolu_perf_edit",
        "canvas.rs",
        canvas_hunks(&canvas),
    );
    running_suite(s, who);
}

/// `cargo test --workspace`, started 22 seconds before capture and 212
/// results into its 357, the working line captioned `Reticulating`.
fn running_suite(s: &mut Script, who: ThreadId) {
    let started_at = CAPTURE - 22;
    s.ev(
        started_at,
        who,
        started(
            "toolu_perf_suite",
            "Bash",
            serde_json::json!({ "command": "cargo test --workspace" }),
        ),
    );
    let delta = |text: String| SessionEvent::ToolOutputDelta {
        id: "toolu_perf_suite".into(),
        text,
    };
    s.ev(started_at + 1, who, delta("running 357 tests\n".into()));
    let results: Vec<String> = (0..212)
        .map(|at| format!("test {} ... ok\n", suite_test(at)))
        .collect();
    // The results arrive across the run, a chunk every second.
    for (second, chunk) in results.chunks(11).enumerate() {
        s.ev(started_at + 2 + second as Secs, who, delta(chunk.concat()));
    }
    for event in tokens(122_000, 4_100, 4_100, 200_000) {
        s.ev(CAPTURE - 1, who, event);
    }
    s.ev(CAPTURE, who, phase("Reticulating"));
}

/// Close stale issues, Pane 2: a Codex Thread asking to close #212
/// (index.html L659-690).
fn close_stale(s: &mut Script, cast: &Cast) {
    let who = cast.close;
    close_start(s, who);
    s.ev(
        hms(19, 36, 5),
        who,
        text(
            "Six were closed. Closing [#212](https://github.com/josephwylie/ferrite/issues/212) \
             needs your ruling: it's superseded by \
             [#240](https://github.com/josephwylie/ferrite/issues/240) but still has a linked \
             PR.\n\n",
        ),
    );
    close_ask(s, who);
}

/// The wall's Close stale tile (index.html W[1]).
fn wall_close(s: &mut Script, cast: &Cast) {
    let who = cast.close;
    close_start(s, who);
    s.ev(
        hms(19, 36, 5),
        who,
        text("Six of seven stale issues closed\n\n"),
    );
    close_ask(s, who);
}

fn close_start(s: &mut Script, who: ThreadId) {
    let start = hms(19, 36, 0);
    s.ev(start, who, init("parity-close", "gpt-6-astra"));
    s.ev(start, who, mode("on-request"));
    s.ev(start, who, context(24_480, 272_000));
    s.prompt(start, who, "Close stale issues older than 0.3.0.");
    s.bash(
        start + 1,
        who,
        "call_close_list",
        "gh issue list --state open --label stale",
        Run {
            stdout: STALE_ISSUES.into(),
            exit: 0,
            ms: 2_000,
        },
    );
    // The account's windows, as Codex last reported them: `wk 12%`.
    s.ev(start + 3, who, limits(0.04, 0.12));
}

/// The approval arrives two minutes before capture: `2m` in the list.
fn close_ask(s: &mut Script, who: ThreadId) {
    let command = "gh issue close 212 --reason \"not planned\"";
    s.ev(
        CAPTURE - 2 * MINUTE - 2,
        who,
        started(
            "call_close_212",
            "Bash",
            serde_json::json!({ "command": command }),
        ),
    );
    s.ev(
        CAPTURE - 2 * MINUTE,
        who,
        approval("call_close_212", command, &["gh", "issue"]),
    );
}

/// Theme retune, Pane 3: done after 3m 12s at 7:33 pm (index.html
/// L693-723).
fn theme(s: &mut Script, cast: &Cast) {
    let who = cast.theme;
    theme_turn(
        s,
        who,
        THEME_PATH,
        "Landed the retune behind the theme tokens; every status ink now clears 4.5:1 on \
         `PANE` and the suite is green.",
    );
}

/// The wall's Theme tile (index.html W[2]).
fn wall_theme(s: &mut Script, cast: &Cast) {
    theme_turn(s, cast.theme, "theme.rs", "Landed the retune; suite green");
}

fn theme_turn(s: &mut Script, who: ThreadId, path: &str, answer: &str) {
    let start = hms(19, 29, 50);
    s.ev(start, who, init("parity-theme", "claude-sonnet-5-5"));
    s.ev(start, who, mode("acceptEdits"));
    s.ev(start, who, context(36_000, 200_000));
    s.prompt(
        start,
        who,
        "Theme retune: lift the status inks to 4.5:1 on the pane.",
    );
    s.edit(start + 40, who, "toolu_theme_edit", path, theme_hunks());
    s.bash(
        start + 70,
        who,
        "toolu_theme_test",
        "cargo test -p ferrite theme::",
        Run {
            stdout: passing("theme", &THEME_TESTS, 24, "0.12s"),
            exit: 0,
            ms: 6_000,
        },
    );
    s.ev(start + 170, who, text(answer));
    s.end(start + 192, who);
}

/// Fold regression, Pane 4: its suite failed two, and an instrumented
/// rerun of the pair is 3 seconds in (index.html L726-750).
///
/// The prototype times the prompt at 7:34 pm and the working line at 8s.
/// Both hold: the provider settled idle after the failure (its own
/// `RunState` snapshot, no turn end) and resumed eight seconds before
/// capture, which is where the working line's clock starts.
fn fold(s: &mut Script, cast: &Cast) {
    let who = cast.fold;
    fold_failure(
        s,
        who,
        "Two cases regressed after the retune; rerunning the pair with the fold instrumented.",
    );
    s.ev(
        CAPTURE - 3,
        who,
        started(
            "toolu_fold_trace",
            "Bash",
            serde_json::json!({ "command": "FOLD_TRACE=1 cargo test fold::tail" }),
        ),
    );
    s.ev(CAPTURE, who, phase("Instrumenting"));
}

/// The wall's Fold tile (index.html W[3]): the failure, its count, and the
/// line about rerunning.
fn wall_fold(s: &mut Script, cast: &Cast) {
    let who = cast.fold;
    fold_failure(s, who, "Rerunning the pair, fold instrumented");
    s.ev(CAPTURE, who, phase("Instrumenting"));
}

fn fold_failure(s: &mut Script, who: ThreadId, prose: &str) {
    let start = hms(19, 34, 0);
    s.ev(start, who, init("parity-fold", "claude-opus-5-5"));
    s.ev(start, who, mode("acceptEdits"));
    s.ev(start, who, context(88_000, 200_000));
    s.prompt(start, who, "Fold regression after the retune: find it.");
    s.bash(
        start + 2,
        who,
        "toolu_fold_suite",
        "cargo test --workspace",
        Run {
            stdout: fold_tests_failing(),
            exit: 101,
            ms: 61_000,
        },
    );
    s.ev(start + 65, who, text(prose));
    s.ev(start + 66, who, text("\n\n"));
    s.ev(
        start + 67,
        who,
        SessionEvent::RunState {
            state: RunState::Idle,
        },
    );
    s.ev(
        CAPTURE - 8,
        who,
        SessionEvent::RunState {
            state: RunState::Running,
        },
    );
}

/// Port onboarding flow (index.html W[4]): 12 seconds in.
fn port(s: &mut Script, cast: &Cast) {
    let who = cast.port;
    let start = CAPTURE - 12;
    s.ev(start, who, init("parity-port", "claude-opus-5-5"));
    s.ev(start, who, mode("acceptEdits"));
    s.ev(start, who, context(41_000, 200_000));
    s.prompt(
        start,
        who,
        "Port the onboarding flow onto the new copy deck.",
    );
    s.tool(
        (start + 2, start + 3),
        who,
        "toolu_port_read",
        "Read",
        serde_json::json!({ "file_path": "docs/onboarding-copy.md" }),
        quiet(),
    );
    s.edit(
        start + 6,
        who,
        "toolu_port_edit",
        "src/onboarding/steps.tsx",
        port_hunks(),
    );
    s.ev(start + 9, who, text("Wiring step 3 to the new copy deck"));
}

/// Bump deps (index.html W[5]): done in 2m 40s at 6:58 pm, 43 minutes
/// before capture — the list's `43m`, on the clock. The notifications
/// scene's final answer is the one line the list prints (R7).
fn bump(s: &mut Script, cast: &Cast, look: Look) {
    let who = cast.bump;
    let start = hms(18, 55, 20);
    s.ev(start, who, init("parity-bump", "gpt-6-sol"));
    s.ev(start, who, mode("on-request"));
    s.ev(start, who, context(58_000, 272_000));
    s.prompt(start, who, "Bump the workspace deps and check deny.");
    if look == Look::Notes {
        s.ev(start + 155, who, text("14 crates bumped, deny clean"));
    } else {
        s.ev(
            start + 50,
            who,
            text("Ran cargo update; 14 crates moved\n\n"),
        );
        s.ev(
            start + 120,
            who,
            text("Ran cargo deny check \u{b7} clean\n\n"),
        );
        s.ev(start + 155, who, text("Bumped 14 crates; deny is clean"));
    }
    s.end(start + 160, who);
}

/// Docs: ADR 0008 (index.html W[6]): 3 seconds in.
fn docs(s: &mut Script, cast: &Cast) {
    let who = cast.docs;
    let start = CAPTURE - 3;
    s.ev(start, who, init("parity-docs", "gpt-6-sol"));
    s.ev(start, who, mode("on-request"));
    s.ev(start, who, context(12_000, 272_000));
    s.prompt(start, who, "Write ADR 0008 from the panes24 spike.");
    s.tool(
        (start + 1, start + 1),
        who,
        "call_docs_read",
        "Read",
        serde_json::json!({ "file_path": "spikes/panes24/NOTES.md" }),
        quiet(),
    );
    s.ev(
        start + 2,
        who,
        text("Drafting the decision section\nfrom the spike notes"),
    );
}

/// Flaky provider test (index.html W[7]): asks which fix to make.
fn flaky(s: &mut Script, cast: &Cast) {
    let who = cast.flaky;
    let start = hms(19, 39, 40);
    s.ev(start, who, init("parity-flaky", "claude-opus-5-5"));
    s.ev(start, who, mode("acceptEdits"));
    s.ev(start, who, context(27_000, 200_000));
    s.prompt(
        start,
        who,
        "The provider test flakes about one run in twenty. Find out why.",
    );
    s.ev(
        start + 15,
        who,
        text("The flake is a reused temp folder\n\n"),
    );
    s.ev(
        start + 19,
        who,
        started(
            "toolu_flaky_ask",
            "AskUserQuestion",
            serde_json::json!({ "questions": [{ "question": "Which fix do you want?" }] }),
        ),
    );
    s.ev(
        start + 20,
        who,
        question(
            "toolu_flaky_ask",
            "Which fix do you want?",
            &[
                (
                    "retry on EEXIST",
                    "Retry the temp folder's creation when another run holds it",
                ),
                (
                    "isolate per pid",
                    "Give every run a temp folder of its own, keyed by its pid",
                ),
            ],
        ),
    );
}

/// The parked three: Release 0.5.0 notes (11 turns, 2h), Icon set refresh
/// (1d) and Bench the old renderer (3d, lumen's only Thread, so lumen has
/// no nav heading — R14).
fn parked(s: &mut Script, cast: &Cast) {
    let who = cast.release;
    let mut at = hms(16, 40, 0);
    s.ev(at, who, init("parity-release", "claude-opus-5-5"));
    s.ev(at, who, mode("acceptEdits"));
    for (turn, (ask, answer)) in RELEASE_TURNS.iter().enumerate() {
        s.prompt(at, who, *ask);
        s.ev(at + 20, who, text(answer));
        s.end(at + 25, who);
        at += 4 * MINUTE + turn as Secs;
    }
    s.park(at, who);

    for (who, session, model, ask, answer) in [
        (
            cast.icons,
            "parity-icons",
            "gpt-6-sol",
            "Refresh the icon set to the 1.6px stroke.",
            "Redrew 24 icons at 1.6px; the old ones are in `icons/legacy`.",
        ),
        (
            cast.bench,
            "parity-bench",
            "claude-opus-5-5",
            "Bench the old renderer against the retained one.",
            "The retained renderer holds 6.1ms p50 at 24 panes; the old one 14.8ms.",
        ),
    ] {
        let start = hms(9, 30, 0);
        s.ev(start, who, init(session, model));
        s.prompt(start, who, ask);
        s.ev(start + 30, who, text(answer));
        s.end(start + 40, who);
        s.park(start + 60, who);
    }
}

// ---------------------------------------------------------------- the copy

const NAV_PATH: &str = "crates/ferrite/src/nav.rs";
const CANVAS_PATH: &str = "crates/ferrite/src/canvas.rs";
const THEME_PATH: &str = "crates/ferrite/src/theme.rs";

/// The prototype's screenshot of the nav, palette-encoded so the file
/// weighs what its caption says: `580×320 · 41 KB`.
const NAV_JITTER_PNG: &[u8] = include_bytes!("nav-jitter.png");

const ANSWER_ONE: &str = "The jitter comes from the status line under each Thread row in \
`nav.rs`. It only exists once a Session reports its first fact, so a row grows from `44px` to \
`58px` the moment streaming starts, and every row below it moves under the pointer.\n\n\
Reserving the line from the first frame fixes it:\n\n\
```rust\n\
let summary = facts\n    .summary\n    .clone()\n    .unwrap_or_else(|| \"waiting for first output\".into());\n\
row.child(status_line(summary).h(px(theme::ROW_LINE_H)))\n\
```\n\n\
| Row state | Before | After |\n\
| :--- | ---: | ---: |\n\
| idle | 44px | 58px |\n\
| streaming | 58px | 58px |\n\
| parked | 44px | 44px |\n\n\
Want me to apply it and run the nav tests?";

const ANSWER_TWO: &str = "Fixed. Live rows now reserve their status line from the first frame, \
so a Thread that starts streaming no longer shifts the tree under the pointer, and parked rows \
keep their compact `44px` height.\n\n\
- `nav.rs`: reserve the status line for open Threads only\n\
- all 38 `nav::` tests pass, including `parked_rows_keep_their_height`\n\n\
I left `facts.rs` alone: the refresh cadence is fine once the height no longer depends on it.";

const PERF_PROSE: &str = "Wiring the joiner into the canvas path so the atlas stays per-cell; \
the fold keeps the tail following the newest line while it streams.";

/// `gh issue list`, as the Thread printed it: `7 issues`, then a row each.
const STALE_ISSUES: &str = "7 issues\n\
#198\tOPEN\tWall census misses parked members\tstale\t2026-06-02\n\
#203\tOPEN\tResume handshake drops the model on Codex\tstale\t2026-06-11\n\
#207\tOPEN\tNav filter forgets the Project after relaunch\tstale\t2026-06-19\n\
#212\tOPEN\tSeam drag snaps to 50% on release\tstale\t2026-07-01\n\
#219\tOPEN\tDiff card clips the last hunk\tstale\t2026-07-08\n\
#224\tOPEN\tToast stack overlaps the bottom bar\tstale\t2026-07-15\n\
#231\tOPEN\tSettings search ignores aliases\tstale\t2026-07-22";

/// Release 0.5.0 notes' eleven turns.
const RELEASE_TURNS: [(&str, &str); 11] = [
    (
        "Draft the 0.5.0 release notes from the merged PRs.",
        "Drafted `CHANGELOG.md` for 0.5.0 from 23 merged PRs.",
    ),
    (
        "Group them by surface.",
        "Grouped under Nav, Panes, Composer, Floats and Core.",
    ),
    (
        "Lead with the terminal-native redesign.",
        "Moved the redesign to the top with a two-line summary.",
    ),
    ("Cut the internal refactors.", "Cut six refactor entries."),
    (
        "Name the contributors.",
        "Added a contributors line with four names.",
    ),
    (
        "Link each entry to its PR.",
        "Every entry now links its PR number.",
    ),
    (
        "Add the upgrade note about the store schema.",
        "Added an upgrade note: the store migrates itself on first launch.",
    ),
    (
        "Shorten the Composer section.",
        "The Composer section is down to four lines.",
    ),
    (
        "Check the version strings.",
        "Cargo, the bundle and the notes all say 0.5.0.",
    ),
    ("Spell-check it.", "Fixed three typos and one doubled word."),
    (
        "Leave it for the release day.",
        "Left as a draft; nothing is tagged.",
    ),
];

/// The 38 tests of `nav::`, in the order cargo printed them.
const NAV_TESTS: [&str; 38] = [
    "rows_reserve_status_line",
    "live_row_height_is_stable",
    "parked_rows_keep_their_height",
    "group_rail_spans_members",
    "needs_you_section_orders_by_age",
    "filter_keeps_selection",
    "project_heading_needs_an_unparked_row",
    "rows_go_in_creation_order",
    "group_sits_under_its_first_members_project",
    "folding_a_group_hides_its_members",
    "folding_a_project_hides_its_groups",
    "parked_fold_counts_parked_threads",
    "selection_bar_follows_focus",
    "spinner_dot_marks_working_rows",
    "failing_rows_read_failing_count",
    "approval_rows_read_approval",
    "question_rows_read_question",
    "done_rows_read_done",
    "durations_tick_on_the_second",
    "long_titles_truncate_before_the_state",
    "tree_glyphs_close_the_last_member",
    "needs_you_strip_lists_each_request_once",
    "needs_you_strip_hides_when_empty",
    "branch_reads_beside_the_project",
    "default_branch_stays_quiet_on_rows",
    "rename_keeps_the_row_in_place",
    "drag_holds_the_order_under_the_pointer",
    "pointer_leave_re_ranks",
    "collapse_rides_the_width",
    "collapsed_nav_keeps_the_rail_actions",
    "row_hover_paints_the_hover_fill",
    "row_press_focuses_the_thread",
    "parked_row_press_revives",
    "group_row_press_enters_the_group",
    "subagent_count_sits_after_the_title",
    "worktree_branch_gives_way_before_the_title",
    "nav_scroll_keeps_the_selection_visible",
    "hints_hide_in_the_clean_pass",
];

/// The 24 tests of `theme::`.
const THEME_TESTS: [&str; 24] = [
    "running_ink_clears_4_5_on_pane",
    "attention_ink_clears_4_5_on_pane",
    "blocked_ink_clears_4_5_on_pane",
    "muted_ink_clears_4_5_on_pane",
    "faint_ink_stays_decorative",
    "accent_clears_4_5_on_band",
    "prose_line_height_is_the_grid",
    "answer_line_height_keeps_the_ui_row",
    "nav_width_is_36_cells",
    "toast_width_is_52_cells",
    "icon_button_is_28_by_24",
    "status_dot_lifts_one_pixel",
    "glass_is_macos_only",
    "opaque_feature_turns_glass_off",
    "float_glass_is_frosted_or_opaque",
    "band_is_a_step_up_from_the_plane",
    "hover_is_below_selection",
    "line_is_below_line2",
    "diff_washes_keep_code_legible",
    "word_marks_outrank_row_washes",
    "syntax_inks_are_distinct",
    "provider_marks_keep_their_colours",
    "reading_sizes_keep_their_steps",
    "standard_reading_size_is_13",
];

/// A passing `cargo test` run of `tests` under `module`.
fn passing(module: &str, tests: &[&str], count: usize, took: &str) -> String {
    let mut lines = vec![format!("running {count} tests")];
    lines.extend(
        tests
            .iter()
            .take(count)
            .map(|test| format!("test {module}::{test} ... ok")),
    );
    lines.push(format!(
        "test result: ok. {count} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; \
         finished in {took}"
    ));
    lines.join("\n")
}

/// The first `nav::` run: one failure. Its first line is the elbow, its
/// next three the preview, and thirty-five more are folded (CT-14).
fn nav_tests_failing() -> String {
    let mut lines = vec!["running 38 tests".to_string()];
    for (at, test) in NAV_TESTS.iter().enumerate().take(29) {
        let verdict = if at == 2 { "FAILED" } else { "ok" };
        lines.push(format!("test nav::{test} ... {verdict}"));
    }
    lines.extend(
        [
            "failures:",
            "---- nav::parked_rows_keep_their_height stdout ----",
            "thread 'nav::parked_rows_keep_their_height' panicked at crates/ferrite/src/nav.rs:1184:9:",
            "assertion `left == right` failed",
            "  left: 58.0",
            " right: 44.0",
            "failures:",
            "    nav::parked_rows_keep_their_height",
            "test result: FAILED. 37 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.41s",
        ]
        .map(String::from),
    );
    debug_assert_eq!(lines.len(), 1 + 3 + 35);
    lines.join("\n")
}

/// Fold regression's workspace run, as cargo prints it: passing tests, the
/// two failures past the first three test lines (so the elbow reads the
/// tally and those two, CT-14), their details, and forty-one folded lines.
fn fold_tests_failing() -> String {
    let mut lines = vec![
        "running 359 tests".to_string(),
        "test fold::fold_opens_under_its_header ... ok".to_string(),
        "test fold::fold_shuts_at_once ... ok".to_string(),
        "test fold::keeps_the_reader_where_it_was ... ok".to_string(),
        "test fold::tail_follows_newest_line ... FAILED".to_string(),
        "test fold::collapse_keeps_anchor ... FAILED".to_string(),
        "failures:".to_string(),
    ];
    for test in ["tail_follows_newest_line", "collapse_keeps_anchor"] {
        lines.extend([
            format!("---- fold::{test} stdout ----"),
            format!(
                "thread 'fold::{test}' panicked at crates/ferrite-core/src/transcript.rs:2417:9:"
            ),
            "assertion `left == right` failed".to_string(),
            "  left: Some(BlockId(41))".to_string(),
            " right: Some(BlockId(42))".to_string(),
        ]);
    }
    lines.push("failures:".into());
    lines.push("    fold::collapse_keeps_anchor".into());
    lines.push("    fold::tail_follows_newest_line".into());
    let mut crate_at = 0;
    while lines.len() < 43 {
        let (name, tests) = SUITE_CRATES[crate_at % SUITE_CRATES.len()];
        lines.push(format!(
            "     Running unittests src/lib.rs (target/debug/deps/{name})"
        ));
        lines.push(format!(
            "test result: ok. {tests} passed; 0 failed; 0 ignored"
        ));
        crate_at += 1;
    }
    lines.truncate(43);
    lines.push(
        "test result: FAILED. 357 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; \
         finished in 61.02s"
            .into(),
    );
    debug_assert_eq!(lines.len(), 1 + 2 + 41);
    lines.join("\n")
}

const SUITE_CRATES: [(&str, usize); 6] = [
    ("ferrite_core-2c41", 188),
    ("ferrite-9a07", 141),
    ("ferrite_bench-11fe", 6),
    ("gpui_reference-73b0", 9),
    ("ferrite_store-5d2e", 11),
    ("ferrite_cli-e810", 2),
];

/// The name of the `at`th test of Perf's workspace run.
fn suite_test(at: usize) -> String {
    const MODULES: [&str; 8] = [
        "transcript",
        "layout",
        "canvas",
        "fold",
        "docview",
        "groups",
        "store",
        "pump",
    ];
    const CASES: [&str; 6] = [
        "keeps_identity",
        "evicts_the_oldest",
        "follows_the_tail",
        "survives_a_resize",
        "replays_from_the_log",
        "settles_on_turn_end",
    ];
    format!(
        "{}::{}_{}",
        MODULES[at % MODULES.len()],
        CASES[(at / MODULES.len()) % CASES.len()],
        at / (MODULES.len() * CASES.len())
    )
}

// ---------------------------------------------------------------- the files

/// Small methods to stand between the lines a scene shows, up to line
/// `upto` (1-based, exclusive of what follows): whole methods while they
/// fit, then section rules.
fn filler(lines: &mut Vec<String>, upto: usize, names: &[&str], returns: &str) {
    let mut at = 0;
    while lines.len() + 6 <= upto {
        let name = names[at % names.len()];
        let suffix = if at < names.len() {
            String::new()
        } else {
            format!("_{}", at / names.len())
        };
        lines.extend([
            format!("fn {name}{suffix}(&self, cx: &App) -> {returns} {{"),
            "    let theme = cx.theme();".to_string(),
            format!("    let facts = self.facts.{name}();"),
            format!("    self.paint_{name}(facts, theme)"),
            "}".to_string(),
            String::new(),
        ]);
        at += 1;
    }
    while lines.len() < upto {
        lines.push("// ----".into());
    }
}

const NAV_NAMES: [&str; 12] = [
    "project_heading",
    "group_heading",
    "member_row",
    "needs_you_strip",
    "parked_fold",
    "hints_row",
    "branch_label",
    "state_word",
    "tree_glyph",
    "row_duration",
    "selection_bar",
    "provider_mark",
];

/// `nav.rs` as Nav rows jitter read it: 412 lines, `thread_row` at 208.
fn nav_before() -> Vec<String> {
    let mut lines: Vec<String> = [
        "//! The nav column: Projects, Groups and their Threads as rows.",
        "",
        "use gpui::{div, px, App, Div, IntoElement, ParentElement, SharedString, Styled};",
        "",
        "use crate::facts::Facts;",
        "use crate::theme;",
        "",
        "pub(crate) struct ThreadRow {",
        "    pub id: ThreadId,",
        "    pub title: SharedString,",
        "    pub session: Option<SessionFacts>,",
        "    pub parked: bool,",
        "}",
        "",
        "pub(crate) struct Nav {",
        "    facts: Facts,",
        "}",
        "",
        "impl Nav {",
    ]
    .map(String::from)
    .to_vec();
    filler(&mut lines, 207, &NAV_NAMES, "Div");
    lines.extend(
        [
            "fn thread_row(&self, t: &ThreadRow, cx: &App) -> Div {",
            "    let facts = self.facts.get(&t.id);",
            "    let mut row = div().h(px(theme::ROW_H));",
            "    if let Some(summary) = facts.and_then(|f| f.summary.clone()) {",
            "        row = row.child(status_line(summary));",
            "    }",
            "    row",
            "}",
            "",
        ]
        .map(String::from),
    );
    filler(&mut lines, 411, &NAV_NAMES[6..], "Div");
    lines.push("}".into());
    lines
}

/// Turn two's first edit (`+7 −3` in the prototype, whose split hunk
/// draws six of the seven added rows): `@@ -208,7 +208,11 @@`.
fn nav_edit_one() -> Hunk {
    Hunk {
        old_start: 208,
        old_lines: 7,
        new_start: 208,
        new_lines: 11,
        section: Some("fn thread_row".into()),
        lines: [
            " fn thread_row(&self, t: &ThreadRow, cx: &App) -> Div {",
            "     let facts = self.facts.get(&t.id);",
            "+    let live = t.session.is_some();",
            "-    let mut row = div().h(px(theme::ROW_H));",
            "-    if let Some(summary) = facts.and_then(|f| f.summary.clone()) {",
            "+    let mut row = div().h(px(if live { theme::ROW_LIVE_H } else { theme::ROW_H }));",
            "+    if live {",
            "+        let summary = facts.and_then(|f| f.summary.clone())",
            "+            .unwrap_or_else(|| \"waiting for first output\".into());",
            "-        row = row.child(status_line(summary));",
            "+        row = row.child(status_line(summary).h(px(theme::ROW_LINE_H)));",
            "     }",
            "     row",
        ]
        .map(String::from)
        .to_vec(),
    }
}

/// Turn two's second edit (`+2 −1`): the hunk starts at new line 211, the
/// line the hover card opens on (R10), and changes nothing the card shows.
fn nav_edit_two() -> Hunk {
    Hunk {
        old_start: 211,
        old_lines: 6,
        new_start: 211,
        new_lines: 7,
        section: Some("fn thread_row".into()),
        lines: [
            "     let mut row = div().h(px(if live { theme::ROW_LIVE_H } else { theme::ROW_H }));",
            "     if live {",
            "         let summary = facts.and_then(|f| f.summary.clone())",
            "-            .unwrap_or_else(|| \"waiting for first output\".into());",
            "+            .filter(|_| !t.parked)",
            "+            .unwrap_or_else(|| \"waiting for first output\".into());",
            "         row = row.child(status_line(summary).h(px(theme::ROW_LINE_H)));",
            "     }",
        ]
        .map(String::from)
        .to_vec(),
    }
}

const CANVAS_NAMES: [&str; 10] = [
    "cell_bounds",
    "glyph_run",
    "baseline",
    "cursor_cell",
    "selection_quads",
    "underline_quads",
    "cell_background",
    "dirty_rows",
    "scroll_offset",
    "atlas_page",
];

/// `canvas.rs` before Perf's edit: 640 lines, `impl Canvas` at 88.
fn canvas_before() -> Vec<String> {
    let mut lines: Vec<String> = [
        "//! The canvas: one Pane's glyphs, laid out per cell and painted.",
        "",
        "use std::collections::HashMap;",
        "",
        "use crate::atlas::{Atlas, Glyph};",
        "",
        "pub(crate) struct Canvas {",
        "    glyphs: Vec<Glyph>,",
        "    cells: HashMap<CellId, Atlas>,",
        "    live: Vec<CellId>,",
        "    stats: CanvasStats,",
        "}",
        "",
    ]
    .map(String::from)
    .to_vec();
    filler(&mut lines, 87, &CANVAS_NAMES, "Frame");
    lines.extend(
        [
            "impl Canvas {",
            "fn paint(&mut self, cell: CellId, glyphs: &[Glyph]) -> Frame {",
            "    let layout = self.layout_for(cell, glyphs);",
            "    let atlas = Atlas::build(&self.glyphs);",
            "    let frame = atlas.place(&layout);",
            "    frame",
            "}",
            "",
        ]
        .map(String::from),
    );
    filler(&mut lines, 639, &CANVAS_NAMES[3..], "Frame");
    lines.push("}".into());
    lines
}

/// Perf's edit, `+31 −9` in three hunks; the first is the one the Pane
/// shows, `@@ -88,6 +88,9 @@ impl Canvas`, its changed rows 91−, 91+, 92+.
fn canvas_hunks(before: &[String]) -> Vec<Hunk> {
    let first = Hunk {
        old_start: 88,
        old_lines: 6,
        new_start: 88,
        new_lines: 9,
        section: Some("impl Canvas".into()),
        lines: [
            " impl Canvas {",
            " fn paint(&mut self, cell: CellId, glyphs: &[Glyph]) -> Frame {",
            "     let layout = self.layout_for(cell, glyphs);",
            "-    let atlas = Atlas::build(&self.glyphs);",
            "+    let atlas = self.cells.entry(cell)",
            "+        .or_insert_with(|| Atlas::build(&self.glyphs));",
            "     let frame = atlas.place(&layout);",
            "+    self.cells.retain(|id, _| self.live.contains(id));",
            "+    self.stats.atlases = self.cells.len();",
            "     frame",
        ]
        .map(String::from)
        .to_vec(),
    };
    let second = replace(
        before,
        120,
        3,
        3,
        &[
            "    /// One atlas per cell, built the first time the cell paints.",
            "    fn atlas_for(&mut self, cell: CellId) -> &Atlas {",
            "        self.cells",
            "            .entry(cell)",
            "            .or_insert_with(|| Atlas::build(&self.glyphs))",
            "    }",
            "",
            "    /// Cells no Pane shows any more give their atlases back.",
            "    fn evict(&mut self) { self.cells.retain(|id, _| self.live.contains(id)); }",
        ],
        3,
    );
    let third = replace(
        before,
        200,
        3,
        5,
        &[
            "fn cell_frame(&mut self, cell: CellId) -> Frame {",
            "    let glyphs = self.glyphs_for(cell);",
            "    let layout = self.layout_for(cell, &glyphs);",
            "    let atlas = self.atlas_for(cell);",
            "    let frame = atlas.place(&layout);",
            "    self.stats.frames += 1;",
            "    frame",
            "}",
            "",
            "fn paint_board(&mut self, cells: &[CellId]) -> Vec<Frame> {",
            "    self.live = cells.to_vec();",
            "    let frames = cells.iter().map(|cell| self.cell_frame(*cell)).collect();",
            "    self.evict();",
            "    frames",
            "}",
            "",
            "// Atlases are per-cell: a 12-pane board rebuilds none per frame.",
            "",
        ],
        9,
    );
    vec![first, second, third]
}

/// A hunk at `old_start` replacing `remove` lines of `before` with `add`,
/// with `before_context` lines of context ahead and its new start shifted
/// by `shift`.
fn replace(
    before: &[String],
    old_start: u32,
    before_context: usize,
    remove: usize,
    add: &[&str],
    shift: i64,
) -> Hunk {
    let first = old_start as usize - 1;
    let mut lines: Vec<String> = Vec::new();
    lines.extend(
        before[first..first + before_context]
            .iter()
            .map(|line| format!(" {line}")),
    );
    lines.extend(
        before[first + before_context..first + before_context + remove]
            .iter()
            .map(|line| format!("-{line}")),
    );
    lines.extend(add.iter().map(|line| format!("+{line}")));
    Hunk {
        old_start,
        old_lines: (before_context + remove) as u32,
        new_start: (old_start as i64 + shift) as u32,
        new_lines: (before_context + add.len()) as u32,
        section: None,
        lines,
    }
}

const THEME_NAMES: [&str; 8] = [
    "plane",
    "band",
    "hover",
    "selection",
    "line",
    "line2",
    "float",
    "frame",
];

/// `theme.rs` before the retune: the state inks at 237, 240 and 246.
fn theme_before() -> Vec<String> {
    let mut lines: Vec<String> = [
        "//! The palette, the grid and the type: every token the chrome reads.",
        "",
    ]
    .map(String::from)
    .to_vec();
    let mut at = 0;
    while lines.len() < 230 {
        let name = THEME_NAMES[at % THEME_NAMES.len()];
        lines.push(format!(
            "pub const {}_{}: u32 = 0x{:06x};",
            name.to_uppercase(),
            at / THEME_NAMES.len(),
            0x1b1b1c + at * 0x010101
        ));
        at += 1;
    }
    lines.extend(
        [
            "",
            "// ---- state inks: each clears 4.5:1 on PANE",
            "pub const PANE: u32 = 0x1b1b1c;",
            "",
            "/// A Thread at work: the dot, the spinner, the",
            "/// `working` word.",
            "pub const RUNNING: u32 = 0x7fbf95;",
            "",
            "/// A Thread waiting on the operator.",
            "pub const ATTENTION: u32 = 0xcbb280;",
            "",
            "pub const MUTED: u32 = 0x98989d;",
            "pub const FAINT: u32 = 0x5e5e63;",
            "",
            "/// A Thread whose Session closed under it.",
            "pub const BLOCKED: u32 = 0xd9776f;",
        ]
        .map(String::from),
    );
    while lines.len() < 300 {
        lines.push(format!(
            "pub const GRID_{}: f32 = {}.0;",
            lines.len(),
            lines.len() % 40
        ));
    }
    lines
}

/// The retune, `+3 −3`: zero-context rows at 237 and 240 as the prototype
/// draws them, the second hunk's header counting the BLOCKED row at 246
/// its lines leave out (the stat reads the header, `hunk_stat`).
fn theme_hunks() -> Vec<Hunk> {
    [
        (237, 1, "RUNNING", "0x7fbf95", "0x93cf8c"),
        (240, 2, "ATTENTION", "0xcbb280", "0xe6c47c"),
    ]
    .into_iter()
    .map(|(line, counted, name, old, new)| Hunk {
        old_start: line,
        old_lines: counted,
        new_start: line,
        new_lines: counted,
        section: None,
        lines: vec![
            format!("-pub const {name}: u32 = {old};"),
            format!("+pub const {name}: u32 = {new};"),
        ],
    })
    .collect()
}

/// Port's edit to the onboarding steps, `+48 −12`.
fn port_hunks() -> Vec<Hunk> {
    (0..4)
        .map(|part| {
            let start = 20 + part * 60;
            let mut lines = vec![format!(" export const STEP_{part} = {{")];
            lines.extend((0..3).map(|at| format!("-  copy: legacy.step{part}_{at},")));
            lines.extend((0..12).map(|at| format!("+  copy: deck.step{part}.line{at},")));
            Hunk {
                old_start: start,
                old_lines: 4,
                new_start: start + part * 9,
                new_lines: 13,
                section: None,
                lines,
            }
        })
        .collect()
}

/// `before` with `hunks` applied, in order. Each hunk's old range must
/// match `before` (checked in the tests).
fn apply(before: &[String], hunks: &[Hunk]) -> Vec<String> {
    let mut after = Vec::new();
    let mut next = 0;
    for hunk in hunks {
        let start = hunk.old_start as usize - 1;
        after.extend(before[next..start].iter().cloned());
        let mut old = start;
        for line in &hunk.lines {
            let (marker, body) = line.split_at(1);
            match marker {
                " " => {
                    after.push(body.to_string());
                    old += 1;
                }
                "-" => old += 1,
                _ => after.push(body.to_string()),
            }
        }
        next = old;
    }
    after.extend(before[next..].iter().cloned());
    after
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every hunk's old side is what the file said there.
    fn assert_applies(before: &[String], hunks: &[Hunk]) {
        for hunk in hunks {
            let mut at = hunk.old_start as usize - 1;
            for line in &hunk.lines {
                let (marker, body) = line.split_at(1);
                if marker != "+" {
                    assert_eq!(
                        before[at],
                        body,
                        "hunk at {} line {}",
                        hunk.old_start,
                        at + 1
                    );
                    at += 1;
                }
            }
        }
    }

    #[test]
    fn the_hover_card_reads_the_edited_nav_rs() {
        let before = nav_before();
        assert_eq!(before.len(), 412, "Read reports 412 lines");
        assert_applies(&before, &[nav_edit_one()]);
        let middle = apply(&before, &[nav_edit_one()]);
        assert_applies(&middle, &[nav_edit_two()]);
        let after = apply(&middle, &[nav_edit_two()]);
        // index.html L821-826: the card's window, 208 to 213 (R10, 211±).
        assert_eq!(
            &after[207..213],
            &[
                "fn thread_row(&self, t: &ThreadRow, cx: &App) -> Div {",
                "    let facts = self.facts.get(&t.id);",
                "    let live = t.session.is_some();",
                "    let mut row = div().h(px(if live { theme::ROW_LIVE_H } else { theme::ROW_H }));",
                "    if live {",
                "        let summary = facts.and_then(|f| f.summary.clone())",
            ]
        );
    }

    #[test]
    fn the_canvas_and_theme_edits_apply_and_count_as_the_prototype_says() {
        let canvas = canvas_before();
        assert_eq!(canvas.len(), 640, "Read reports 640 lines");
        assert_eq!(canvas[87], "impl Canvas {");
        let hunks = canvas_hunks(&canvas);
        assert_applies(&canvas, &hunks);
        let count = |hunks: &[Hunk], marker: char| {
            hunks
                .iter()
                .flat_map(|hunk| hunk.lines.iter())
                .filter(|line| line.starts_with(marker))
                .count()
        };
        assert_eq!((count(&hunks, '+'), count(&hunks, '-')), (31, 9));
        let theme = theme_before();
        assert_applies(&theme, &theme_hunks());
        // Two drawn pairs; the second header counts the third (`+3 −3`).
        assert_eq!(
            (count(&theme_hunks(), '+'), count(&theme_hunks(), '-')),
            (2, 2)
        );
        assert_eq!(
            (count(&port_hunks(), '+'), count(&port_hunks(), '-')),
            (48, 12)
        );
    }

    #[test]
    fn the_test_runs_fold_where_the_prototype_folds_them() {
        assert_eq!(nav_tests_failing().lines().count(), 39);
        assert_eq!(fold_tests_failing().lines().count(), 44);
        assert!(passing("nav", &NAV_TESTS, 38, "0.39s").ends_with("finished in 0.39s"));
        assert_eq!(STALE_ISSUES.lines().count(), 8);
    }

    #[test]
    fn a_clock_label_reads_back_as_its_minute() {
        assert_eq!(label_seconds("7:41 pm"), Some(CAPTURE - 30));
        assert_eq!(label_seconds("12:05 am"), Some(hms(0, 5, 0)));
        assert_eq!(label_seconds("12:05 pm"), Some(hms(12, 5, 0)));
        assert_eq!(label_seconds("noon"), None);
    }

    #[test]
    fn the_world_holds_the_prototype_roster() {
        let root =
            std::env::temp_dir().join(format!("ferrite-parity-world-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let clock = Clock {
            midnight: midnight(),
            fixture: None,
        };
        let world = build(&root, Look::Group, clock);
        let core = &world.core;
        let mut open = core.threads();
        open.sort();
        let mut wanted = world.cast.open().to_vec();
        wanted.sort();
        assert_eq!(open, wanted, "nine Threads are open");
        assert_eq!(core.parked().unwrap().len(), 3, "three are parked");
        assert_eq!(core.registry().projects().len(), 3);
        let titles: Vec<String> = core.groups().iter().map(|g| g.display_title()).collect();
        assert_eq!(titles, ["Perf sweep", "Everything"]);
        assert_eq!(
            core.groups()
                .of(world.cast.perf)
                .map(|group| group.display_title()),
            Some("Perf sweep".to_string())
        );
        assert_eq!(core.needs_you(), vec![world.cast.close, world.cast.flaky]);
        assert_eq!(core.roster().focused_thread(), Some(world.cast.perf));
        let nav = core.thread(world.cast.nav).unwrap();
        assert_eq!(nav.model(), Some("opus[1m]"));
        assert_eq!(nav.effort(), Some("medium"));
        drop(world);
        let _ = std::fs::remove_dir_all(&root);
    }
}
