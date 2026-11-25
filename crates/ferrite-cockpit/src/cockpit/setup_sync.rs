//! Setup sync (ADR 0048) as the window runs it: this computer's agent
//! setup kept the same as each connected Machine's, both ways, with no
//! step of the operator's. A Machine syncs as it connects, as a Thread
//! starts on it, a few seconds after the setup here changes, and once a
//! minute while it stays connected, on a clock of its own that no other
//! sync moves; off the window's thread, and one sync of the setup at a time
//! for them all: one that finds another under way tries again. Each
//! Machine's last sync is its line in Settings › Connections, and what
//! needs the operator's eye is one toast for them all.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::*;
use ferrite_core::chunked::Cancellation;
use ferrite_client::cockpit::ServerState;
use ferrite_core::setup_sync::{self, Hold, Home, Job, Kept, Outcome, Replaced, Synced, Valve};
use ferrite_core::tree::{self, Side, Stuck};
use gpui::App;

/// How often a connected Machine is looked at for what changed there.
const CHECK: Duration = Duration::from_secs(60);
/// How long the setup here is left to settle after a change before it
/// syncs: an edit is often several writes.
const SETTLE: Duration = Duration::from_secs(3);

/// How long a sync that found another under way waits to try again:
/// [`SETTLE`], and up to as long again by chance. Two computers that are
/// each other's hub each hold their own home as they ask for the other's,
/// so two that ask at once both find the other busy, and would again each
/// time they tried again together.
fn retry_after() -> Duration {
    SETTLE + SETTLE.mul_f64(rand::random::<f64>())
}

pub(crate) struct SetupSync {
    /// This computer's setup, or why this window syncs none of it; `None`
    /// syncs nothing and says nothing (tests, the demo).
    home: Option<Result<Home, String>>,
    settings_dir: PathBuf,
    cockpit: Entity<Cockpit>,
    /// Each Machine's, by its saved target.
    machines: HashMap<String, Track>,
    /// When the setup here last changed, as the watcher saw it.
    changed: Arc<Mutex<Option<Instant>>>,
    _watcher: Option<notify::RecommendedWatcher>,
    /// What the toast says, each Machine's by its name.
    news: BTreeMap<String, News>,
    /// The news moved since the toast was last put up.
    untold: bool,
}

/// A Machine as the window holds it now.
pub(crate) struct Wanted {
    pub target: String,
    pub name: String,
    pub machine: MachineId,
    pub attached: bool,
    /// Its "Sync my setup" switch.
    pub on: bool,
}

struct Track {
    machine: MachineId,
    name: String,
    attached: bool,
    running: Option<Running>,
    /// Asked while a sync ran, or when one found another under way: run
    /// once it may.
    again: Option<Ask>,
    /// When the clock next looks at the Machine.
    check: Option<Instant>,
    /// Before this, `again` waits: another sync held the Machine.
    retry: Option<Instant>,
    status: Status,
    /// What its syncs found, each part as the last sync that read it left
    /// it: neither a sync under way nor one that left a path unread moves
    /// what it did not look at.
    found: Found,
}

/// What the syncs of a Machine found.
#[derive(Debug, Default)]
struct Found {
    /// Each path left as each side has it, and why.
    held: BTreeMap<String, Why>,
    /// Where a sync stopped, and what it was asked: "Sync anyway" asks it
    /// again.
    stop: Option<(Valve, Ask)>,
}

impl Found {
    /// The line it makes.
    fn status(&self) -> Status {
        match &self.stop {
            Some((valve, _)) => Status::Stopped(valve.clone()),
            None => Status::InSync(self.held.clone()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ask {
    only_if_changed_here: bool,
    read_sign_ins: bool,
    allow_removals: bool,
    sign_ins: Option<Side>,
}

impl Ask {
    /// As it connects and on the clock: all of it, Claude's sign-in with
    /// it, which macOS asks the Keychain for.
    const CHECK: Ask = Ask {
        only_if_changed_here: false,
        read_sign_ins: true,
        allow_removals: false,
        sign_ins: None,
    };
    /// The setup here settled after a change: what changed, if anything
    /// did, and what changed there with it.
    const HERE: Ask = Ask {
        only_if_changed_here: true,
        read_sign_ins: false,
        allow_removals: false,
        sign_ins: None,
    };
    /// A Thread starts there, or the operator said to go on.
    const WHOLE: Ask = Ask {
        only_if_changed_here: false,
        read_sign_ins: false,
        allow_removals: false,
        sign_ins: None,
    };

    /// One sync that does what both would.
    fn and(self, other: Ask) -> Ask {
        Ask {
            only_if_changed_here: self.only_if_changed_here && other.only_if_changed_here,
            read_sign_ins: self.read_sign_ins || other.read_sign_ins,
            allow_removals: self.allow_removals || other.allow_removals,
            sign_ins: other.sign_ins.or(self.sign_ins),
        }
    }

    /// Whether a sync of it reads `path`, a path of the setup.
    fn reads(self, path: &str) -> bool {
        let under = |unread: &&str| path.strip_prefix(*unread).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'));
        self.read_sign_ins || !setup_sync::UNREAD_WITHOUT_SIGN_INS.iter().any(under)
    }
}

struct Running {
    heard: mpsc::Receiver<Heard>,
    cancel: Cancellation,
    ask: Ask,
}

enum Heard {
    Planned(usize),
    Done(std::io::Result<Outcome>),
}

/// What a Machine's line says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Off,
    /// Why this window syncs no setup.
    Refused(String),
    Waiting,
    Syncing(Option<usize>),
    /// Synced, but for each path left as each side has it, by why.
    InSync(BTreeMap<String, Why>),
    Stopped(Valve),
    Failed(Failure),
    /// The Machine's Ferrite syncs setup by an older table, or none.
    Older,
}

/// Why a sync left a path as each side has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Why {
    /// The sync held it: two sign-ins for the operator to choose between,
    /// a link, or a file against a folder.
    Held(Hold),
    /// A side could not read or write it.
    Problem(Side, String),
}

/// Why a sync did not finish: what failed, at the path and on the side it
/// met it when it says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Failure {
    at: Option<(String, Side)>,
    why: String,
}

impl Failure {
    fn of(error: &std::io::Error) -> Self {
        let failure = match error.get_ref().and_then(|inner| inner.downcast_ref::<Stuck>()) {
            Some(stuck) => Self {
                at: Some((stuck.path.clone(), stuck.side)),
                why: plain(&stuck.error.to_string()),
            },
            None => Self {
                at: None,
                why: plain(&error.to_string()),
            },
        };
        // A lapsed hold is the whole sync's, not the path's it met it at.
        match failure.why == tree::LAPSED {
            true => Self { at: None, ..failure },
            false => failure,
        }
    }

    /// It in words, the Machine called `name`.
    fn said(&self, name: &str) -> String {
        match &self.at {
            Some((path, on)) => format!("{} on {}: {}", named(path), place(*on, name), self.why),
            None if self.why == tree::LAPSED => self.why.replacen("The Machine", name, 1),
            None => self.why.clone(),
        }
    }
}

/// The first line of an error, without the path a side's error may begin
/// with or the number an OS error ends in.
fn plain(error: &str) -> String {
    let line = error.lines().next().unwrap_or_default();
    let absolute = |head: &str| {
        let bytes = head.as_bytes();
        head.starts_with(['/', '\\']) || (bytes.len() > 2 && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/'))
    };
    let line = match line.split_once(": ") {
        Some((head, rest)) if absolute(head) => rest,
        _ => line,
    };
    match line.rfind(" (os error ") {
        Some(at) if line.ends_with(')') => line[..at].to_string(),
        _ => line.to_string(),
    }
}

impl Status {
    /// The line of the Machine called `name`.
    pub(crate) fn line(&self, name: &str) -> String {
        match self {
            Status::Off => "Off".into(),
            Status::Refused(why) => why.clone(),
            Status::Waiting => "Not synced yet".into(),
            Status::Syncing(None) => "Syncing…".into(),
            Status::Syncing(Some(count)) => format!("Syncing {}", files(*count)),
            Status::InSync(_) if self.sign_ins() => "Signed in differently on each side: choose which to keep".into(),
            Status::InSync(held) => match held.iter().next() {
                None => "In sync".into(),
                Some((path, why)) if held.len() == 1 => format!("In sync except {}", why.said(path, name)),
                Some((path, why)) => {
                    let plugins = held.keys().filter(|path| setup_sync::installing(path).is_some()).count();
                    let count = |count: usize, one: &str| match count {
                        1 => format!("1 {one}"),
                        count => format!("{count} {one}s"),
                    };
                    let counted = match (held.len() - plugins, plugins) {
                        (paths, 0) => count(paths, "path"),
                        (0, plugins) => count(plugins, "plugin"),
                        (paths, plugins) => format!("{} and {}", count(paths, "path"), count(plugins, "plugin")),
                    };
                    format!("In sync except {counted}. {}", why.said(path, name))
                }
            },
            Status::Stopped(valve) => format!("Stopped: {}", stopped(valve, name)),
            Status::Failed(failure) => failure.said(name),
            Status::Older => format!("Update Ferrite on {name} to sync your setup"),
        }
    }

    pub(crate) fn failed(&self) -> bool {
        matches!(self, Status::Failed(_) | Status::Stopped(_) | Status::Older)
    }

    /// Two sign-ins wait for the operator to say whose to keep.
    pub(crate) fn sign_ins(&self) -> bool {
        matches!(self, Status::InSync(held) if held.values().any(|why| *why == Why::Held(Hold::SignIn)))
    }

    /// The status as Connections' row of the Machine `name` says it.
    pub(crate) fn row(&self, name: &str) -> ferrite_settings::connections::Sync {
        ferrite_settings::connections::Sync {
            refused: matches!(self, Status::Refused(_)),
            stopped: matches!(self, Status::Stopped(_)),
            sign_ins: self.sign_ins(),
            failed: self.failed(),
            line: self.line(name),
        }
    }
}

impl Why {
    /// `path` and why it was held, the Machine called `name`.
    fn said(&self, path: &str, name: &str) -> String {
        let named = named(path);
        match self {
            Why::Problem(on, error) => match setup_sync::installing(path) {
                Some(plugin) => format!("{plugin} could not be installed on {}: {}", place(*on, name), plain(error)),
                None => format!("{named} on {}: {}", place(*on, name), plain(error)),
            },
            Why::Held(Hold::Linked(on)) => format!(
                "{named}: it lies through a link on {}, and Setup sync never writes through one",
                place(*on, name)
            ),
            Why::Held(Hold::Kinds) => format!("{named}: it is a file on one side and a folder on the other"),
            Why::Held(Hold::FolderAndLink(folder)) => match folder {
                Side::Here => format!("{named} is a folder here and a link on {name}: left as each has it"),
                Side::There => format!("{named} is a folder on {name} and a link here: left as each has it"),
            },
            Why::Held(Hold::SignIn) => format!("{named}: it is signed in differently on each side"),
        }
    }
}

/// A setup path as the operator names what it is: a skill, a command, an
/// agent, a rule or a prompt by its name, an instruction file by its own,
/// and a chosen key by what it sets and for which tool. What has no plain
/// name is its path from the home folder.
fn named(path: &str) -> String {
    if let Some((item, key)) = setup_sync::chosen(path) {
        let tool = if item.path.starts_with(".codex/") { "Codex's" } else { "Claude's" };
        let (kind, name) = key.split_once('/').unwrap_or(("", key));
        let what = match kind {
            "enabledPlugins" | "plugins" => format!("plugin {}", name.rsplit_once('@').map_or(name, |(plugin, _)| plugin)),
            "extraKnownMarketplaces" | "marketplaces" => format!("marketplace {name}"),
            "mcpServers" | "mcp_servers" => format!("MCP server {name}"),
            "" => words(name),
            _ => format!("{} › {name}", words(kind)),
        };
        return format!("{tool} {what}");
    }
    let file = |under: &str, suffix: &str| path.strip_prefix(under).and_then(|rest| rest.strip_suffix(suffix));
    let skill = |under: &str| {
        let rest = path.strip_prefix(under)?;
        match rest.split_once('/') {
            Some((folder, _)) => Some(folder),
            None => {
                let loose_markdown = rest.ends_with(".md");
                (!rest.is_empty() && !loose_markdown).then_some(rest)
            }
        }
    };
    let named = [
        (".claude/skills/", "the skill"),
        (".agents/skills/", "the skill"),
        (".codex/skills/", "the Codex skill"),
    ]
    .iter()
    .find_map(|(under, kind)| Some(format!("{kind} {}", skill(under)?)))
    .or_else(|| {
        [
            (".claude/commands/", "the command"),
            (".claude/agents/", "the agent"),
            (".claude/rules/", "the rule"),
            (".claude/output-styles/", "the output style"),
            (".codex/prompts/", "the Codex prompt"),
            (".codex/rules/", "the Codex rule"),
        ]
        .iter()
        .find_map(|(under, kind)| Some(format!("{kind} {}", file(under, ".md")?)))
    });
    named.unwrap_or_else(|| match path {
        ".claude/CLAUDE.md" => "CLAUDE.md".to_string(),
        ".codex/AGENTS.md" => "Codex's AGENTS.md".to_string(),
        ".claude/keybindings.json" => "Claude's keybindings".to_string(),
        ".claude/statusline.sh" => "Claude's status line script".to_string(),
        _ => format!("~/{path}"),
    })
}

/// A tool's key in plain words: `skillOverrides` is "skill overrides",
/// and `model_reasoning_effort` "model reasoning effort".
fn words(key: &str) -> String {
    let mut words = String::new();
    for letter in key.chars() {
        if letter == '_' || letter.is_uppercase() {
            words.push(' ');
        }
        if letter != '_' {
            words.extend(letter.to_lowercase());
        }
    }
    words
}

/// A key's old value, short enough for a line.
fn short(value: &str) -> String {
    const MOST: usize = 60;
    match value.char_indices().nth(MOST) {
        Some((at, _)) => format!("{}…", &value[..at]),
        None => value.to_string(),
    }
}

fn files(count: usize) -> String {
    if count == 1 {
        "1 file".to_string()
    } else {
        format!("{count} files")
    }
}

/// What a stopped sync with the Machine called `name` would have done, as
/// a sentence's end.
fn stopped(valve: &Valve, name: &str) -> String {
    if valve.vanished {
        let (gone, removed) = match valve.side {
            Side::Here => (format!("from {name}"), "here".to_string()),
            Side::There => ("here".to_string(), format!("on {name}")),
        };
        format!(
            "~/{} is gone {gone}, so syncing would remove its {} {removed}",
            valve.item,
            files(valve.holding)
        )
    } else {
        format!(
            "syncing would remove {} of the {} in ~/{} on {}",
            valve.removing,
            files(valve.holding),
            valve.item,
            place(valve.side, name)
        )
    }
}

/// `side` in words, the Machine called `name`.
fn place(side: Side, name: &str) -> &str {
    match side {
        Side::Here => "this computer",
        Side::There => name,
    }
}

/// What the toast says of one Machine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct News {
    /// Each file both sides changed: the newer of each is on both, and
    /// the other is kept in `kept`.
    conflicts: Vec<String>,
    kept: Option<PathBuf>,
    /// Each key both sides held differently, and what the loser held.
    replaced: Vec<Replaced>,
    stopped: Option<Valve>,
    failed: Option<Failure>,
}

impl News {
    /// Note each key a sync replaced on one side, the last of each.
    fn replaced(&mut self, replaced: &[Replaced]) {
        for key in replaced {
            self.replaced.retain(|said| said.path != key.path);
            self.replaced.push(key.clone());
        }
    }

    /// Note a sync's conflicts: the folder its losing copies are in, or
    /// the folder of every sync's when they are in more than one.
    fn conflicted(&mut self, kept: &[Kept]) {
        for kept in kept {
            if !self.conflicts.contains(&kept.path) {
                self.conflicts.push(kept.path.clone());
            }
            let Some(sync) = kept.kept.ancestors().nth(kept.path.split('/').count()) else {
                continue;
            };
            self.kept = match &self.kept {
                Some(folder) if folder != sync => sync.parent().map(Path::to_path_buf),
                _ => Some(sync.to_path_buf()),
            };
        }
    }

    fn is_empty(&self) -> bool {
        self.conflicts.is_empty() && self.replaced.is_empty() && self.stopped.is_none() && self.failed.is_none()
    }

    /// Whether it asks anything of the operator: a key replaced is only
    /// said.
    fn warns(&self) -> bool {
        !self.conflicts.is_empty() || self.stopped.is_some() || self.failed.is_some()
    }
}

/// What the toast says.
#[derive(Debug, PartialEq, Eq)]
struct Toast {
    title: String,
    message: String,
    /// What its press opens.
    folder: Option<PathBuf>,
    /// Something in it asks the operator to look.
    warning: bool,
}

/// The toast for `news`; `None` when there is nothing to say.
fn toast(news: &BTreeMap<String, News>) -> Option<Toast> {
    let kept = format!("Ferrite's {} folder", setup_sync::CONFLICTS);
    let mut said: Vec<(String, String, String)> = Vec::new();
    let mut folder = None;
    for (machine, news) in news {
        if let [path, ..] = news.conflicts.as_slice() {
            let changed = match news.conflicts.len() {
                1 => format!("{} changed on both sides", named(path)),
                count => format!("{count} files changed on both sides"),
            };
            let message = match news.conflicts.len() {
                1 => format!("Here and on {machine}. The newer copy is on both now; the other is kept in {kept}."),
                _ => format!("Here and on {machine}. The newer copy of each is on both now; the other copy of each is kept in {kept}."),
            };
            said.push((capitalised(&changed), message, format!("{machine}: {changed}")));
            folder = news.kept.clone();
        }
        for key in &news.replaced {
            let (whose, lost, trash) = match key.winner {
                Side::Here => ("this computer's".to_string(), format!("on {machine}"), "its"),
                Side::There => (format!("{machine}'s"), "here".to_string(), "Ferrite's"),
            };
            let (named, was) = (named(&key.path), short(&key.was));
            let kept = format!("kept in {trash} {} folder", setup_sync::TRASH);
            said.push((
                capitalised(&format!("{named} was set differently on {machine}")),
                format!("It is {whose} on both now. It was {was} {lost}, {kept}."),
                format!("{machine}: {named} is {whose} on both now; it was {was} {lost}, {kept}"),
            ));
        }
        if let Some(valve) = &news.stopped {
            let stopped = stopped(valve, machine);
            said.push((
                format!("Setup sync with {machine} stopped"),
                format!("{}. Sync anyway in Settings › Connections, or put it back.", capitalised(&stopped)),
                format!("{machine}: stopped, {stopped}"),
            ));
        }
        if let Some(failure) = &news.failed {
            let failure = failure.said(machine);
            said.push((format!("Setup sync with {machine} failed"), failure.clone(), format!("{machine}: failed, {failure}")));
        }
    }
    let act = if news.values().any(|news| news.stopped.is_some()) {
        Some("Sync anyway in Settings › Connections.".to_string())
    } else if news.values().any(|news| news.failed.is_some()) {
        Some("More in Settings › Connections.".to_string())
    } else if news.values().any(|news| !news.conflicts.is_empty()) {
        Some(format!("The other copy of each file is kept in {kept}."))
    } else {
        None
    };
    let warning = news.values().any(News::warns);
    match said.as_slice() {
        [] => None,
        [(title, message, _)] => Some(Toast {
            title: title.clone(),
            message: message.clone(),
            folder,
            warning,
        }),
        _ => Some(Toast {
            title: format!("Setup sync: {} things to look at", said.len()),
            message: said.iter().map(|(_, _, line)| line.as_str()).chain(act.as_deref()).collect::<Vec<_>>().join("\n"),
            folder,
            warning,
        }),
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

/// The toast's kind: one for every Machine's news, so one never hides
/// another.
struct SetupSyncToast;

impl SetupSync {
    pub(crate) fn new(home: Option<Result<Home, String>>, settings_dir: PathBuf, cockpit: Entity<Cockpit>) -> Self {
        let changed = Arc::default();
        let watcher = match &home {
            Some(Ok(home)) => watch(home, &changed),
            _ => None,
        };
        Self {
            home,
            settings_dir,
            cockpit,
            machines: HashMap::new(),
            changed,
            _watcher: watcher,
            news: BTreeMap::new(),
            untold: false,
        }
    }

    /// What the Machine `target` names' line says; `None` when this window
    /// syncs no setup.
    pub(crate) fn status(&self, target: &str) -> Option<Status> {
        Some(match self.home.as_ref()? {
            Err(why) => Status::Refused(why.clone()),
            Ok(_) => self.machines.get(target).map_or(Status::Waiting, |track| track.status.clone()),
        })
    }

    /// The toast, when what it says moved since it was put up.
    fn untold(&mut self) -> Option<Option<Toast>> {
        std::mem::take(&mut self.untold).then(|| toast(&self.news))
    }

    /// The operator closed the toast: what it said is heard.
    fn heard(&mut self) {
        self.news.clear();
        self.untold = false;
    }

    /// Hear each sync under way, and start each that is due: a Machine
    /// that connected, one whose clock came round, and every connected one
    /// once the setup here settles after a change. Whether a line moved.
    pub(crate) fn tick(&mut self, wanted: Vec<Wanted>, cx: &mut Context<Self>) -> bool {
        let Some(Ok(home)) = self.home.clone() else {
            return false;
        };
        let now = Instant::now();
        let settled = {
            let mut changed = self.changed.lock().unwrap_or_else(PoisonError::into_inner);
            let settled = changed.is_some_and(|at| now.duration_since(at) >= SETTLE);
            if settled {
                *changed = None;
            }
            settled
        };
        let before = self.statuses();
        self.machines.retain(|target, _| wanted.iter().any(|want| want.target == *target));
        for want in wanted {
            let track = self.machines.entry(want.target.clone()).or_insert_with(|| Track::new(want.machine, &want.name));
            track.machine = want.machine;
            track.name = want.name;
            if let Some((done, ask)) = track.hear() {
                let news = self.news.entry(track.name.clone()).or_default();
                self.untold |= track.finished(done, ask, now, news);
                if news.is_empty() {
                    self.news.remove(&track.name);
                }
            }
            if !want.on {
                if let Some(running) = track.running.take() {
                    running.cancel.cancel();
                }
                track.again = None;
                track.check = None;
                track.retry = None;
                track.attached = want.attached;
                track.status = Status::Off;
                track.found = Found::default();
                continue;
            }
            if track.status == Status::Off {
                // Turned on again: it syncs as if it had just connected.
                track.status = Status::Waiting;
                track.attached = false;
            }
            let connected = want.attached && !track.attached;
            track.attached = want.attached;
            if !want.attached {
                track.check = None;
                continue;
            }
            if let Some(ask) = track.due(now, connected, settled) {
                track.start(ask, &home, &self.settings_dir, &want.target, &self.cockpit, cx);
            }
        }
        self.statuses() != before || self.untold
    }

    /// Sync the Machine the Cockpit holds as `machine` now: a Thread starts
    /// there, so its setup should be this computer's.
    pub(crate) fn before_start(&mut self, machine: MachineId, cx: &mut Context<Self>) {
        self.ask(|_, track| track.machine == machine, Ask::WHOLE, cx);
    }

    /// Sync the Machine `target` names as the sync that stopped did,
    /// removing what the plan removes.
    pub(crate) fn anyway(&mut self, target: &str, cx: &mut Context<Self>) {
        if let Some(ask) = self.machines.get(target).and_then(Track::anyway) {
            self.ask(|named, _| named == target, ask, cx);
        }
    }

    /// Sync the Machine `target` names, each sign-in that differs with
    /// none agreed taken from `side`.
    pub(crate) fn use_sign_ins(&mut self, target: &str, side: Side, cx: &mut Context<Self>) {
        let ask = Ask {
            sign_ins: Some(side),
            ..Ask::CHECK
        };
        self.ask(|named, _| named == target, ask, cx);
    }

    fn ask(&mut self, which: impl Fn(&str, &Track) -> bool, ask: Ask, cx: &mut Context<Self>) {
        let Some(Ok(home)) = self.home.clone() else {
            return;
        };
        for (target, track) in &mut self.machines {
            if which(target, track) && track.attached && track.status != Status::Off {
                track.start(ask, &home, &self.settings_dir, target, &self.cockpit, cx);
            }
        }
    }

    fn statuses(&self) -> BTreeMap<String, Status> {
        self.machines.iter().map(|(target, track)| (target.clone(), track.status.clone())).collect()
    }
}

impl Track {
    fn new(machine: MachineId, name: &str) -> Self {
        Self {
            machine,
            name: name.to_string(),
            attached: false,
            running: None,
            again: None,
            check: None,
            retry: None,
            status: Status::Waiting,
            found: Found::default(),
        }
    }

    /// What "Sync anyway" asks: what the sync that stopped asked, so it
    /// reads all that one read, whole, and removing what it would remove.
    /// Nothing when no sync stopped.
    fn anyway(&self) -> Option<Ask> {
        let (_, ask) = self.found.stop.as_ref()?;
        Some(Ask {
            only_if_changed_here: false,
            allow_removals: true,
            ..*ask
        })
    }

    /// The sync due `now`, if any: all of it as the Machine connects and
    /// each time the clock comes round, what changed here once that
    /// settles, and else what waited. Nothing but the clock moves it.
    fn due(&mut self, now: Instant, connected: bool, settled: bool) -> Option<Ask> {
        if connected || self.check.is_some_and(|check| now >= check) {
            self.check = Some(now + CHECK);
            return Some(Ask::CHECK);
        }
        if settled {
            return Some(Ask::HERE);
        }
        let free = self.running.is_none() && self.retry.is_none_or(|retry| now >= retry);
        free.then(|| self.again.take()).flatten()
    }

    /// What the sync under way came to, once it is done.
    fn hear(&mut self) -> Option<(std::io::Result<Outcome>, Ask)> {
        let running = self.running.as_ref()?;
        loop {
            match running.heard.try_recv() {
                Ok(Heard::Planned(0)) => {}
                Ok(Heard::Planned(files)) => self.status = Status::Syncing(Some(files)),
                Ok(Heard::Done(done)) => {
                    let ask = running.ask;
                    self.running = None;
                    return Some((done, ask));
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    let ask = running.ask;
                    self.running = None;
                    return Some((Err(std::io::Error::other("The sync stopped before it finished")), ask));
                }
            }
        }
    }

    /// Take in what a sync came to: the line it leaves, and what `news`
    /// says of it. Whether the news moved. A sync of what changed here
    /// that found nothing changed learned nothing of the Machine, so the
    /// line and the news stand; one that found another under way looks at
    /// all of it next, since the other may have moved anything. What a
    /// sync did not read, a hold or a stop, stands as the last sync that
    /// read it left it.
    fn finished(&mut self, done: std::io::Result<Outcome>, ask: Ask, now: Instant, news: &mut News) -> bool {
        if matches!(&done, Ok(Outcome::Synced(synced)) if ask.only_if_changed_here && *synced == Synced::default()) {
            return false;
        }
        let before = news.clone();
        self.status = match done {
            Ok(Outcome::Synced(synced)) => {
                self.found.held.retain(|path, _| !ask.reads(path));
                self.found.held.extend(held(&synced));
                if self.found.stop.as_ref().is_some_and(|(valve, _)| ask.reads(&valve.item)) {
                    self.found.stop = None;
                    news.stopped = None;
                }
                news.failed = None;
                news.conflicted(&synced.conflicts);
                news.replaced(&synced.replaced);
                self.found.status()
            }
            Ok(Outcome::Stopped(valve)) => {
                news.failed = None;
                if self.found.stop.as_ref().is_none_or(|(stopped, _)| *stopped != valve) {
                    news.stopped = Some(valve.clone());
                }
                self.found.stop = Some((valve, ask));
                self.found.status()
            }
            Ok(Outcome::Busy) => {
                let ask = Ask {
                    only_if_changed_here: false,
                    ..ask
                };
                self.again = Some(self.again.map_or(ask, |again| again.and(ask)));
                self.retry = Some(now + retry_after());
                Status::Syncing(None)
            }
            Err(error) if error.kind() == std::io::ErrorKind::Unsupported => Status::Older,
            Err(error) => {
                let failure = Failure::of(&error);
                if !matches!(self.status, Status::Failed(_)) {
                    news.failed = Some(failure.clone());
                }
                Status::Failed(failure)
            }
        };
        *news != before
    }

    /// Start a sync of `target`, or ask for one after the sync under way.
    fn start(&mut self, ask: Ask, home: &Home, settings_dir: &Path, target: &str, cockpit: &Entity<Cockpit>, cx: &mut App) {
        if self.running.is_some() {
            self.again = Some(self.again.map_or(ask, |again| again.and(ask)));
            return;
        }
        let ask = self.again.take().map_or(ask, |again| again.and(ask));
        self.retry = None;
        let machine = self.machine;
        let connect = match cockpit.update(cx, |cockpit, _| cockpit.tree(machine)) {
            Ok(connect) => connect,
            Err(error) => {
                self.status = Status::Failed(Failure::of(&error));
                return;
            }
        };
        let job = Job {
            allow_removals: ask.allow_removals,
            only_if_changed_here: ask.only_if_changed_here,
            read_sign_ins: ask.read_sign_ins,
            sign_ins: ask.sign_ins,
            ..Job::for_machine(home.clone(), settings_dir, target)
        };
        let (said, heard) = ferrite_core::doorbell::channel();
        let cancel = Cancellation::default();
        let stop = cancel.clone();
        let spawned = std::thread::Builder::new().name("setup-sync".into()).spawn(move || {
            let planned = said.clone();
            let done = connect().and_then(|mut there| {
                setup_sync::run(&job, &mut *there, &mut |files| drop(planned.send(Heard::Planned(files))), &stop)
            });
            let _ = said.send(Heard::Done(done));
        });
        if let Err(error) = spawned {
            self.status = Status::Failed(Failure::of(&error));
            return;
        }
        self.running = Some(Running { heard, cancel, ask });
        if self.status == Status::Waiting {
            self.status = Status::Syncing(None);
        }
    }
}

/// Each path a sync that finished left as each side has it, and why.
fn held(synced: &Synced) -> impl Iterator<Item = (String, Why)> + '_ {
    let problems = synced.problems.iter().map(|problem| {
        let error = problem.error.lines().next().unwrap_or_default().to_string();
        (problem.path.clone(), Why::Problem(problem.side, error))
    });
    problems.chain(synced.held.iter().map(|(path, why)| (path.clone(), Why::Held(*why))))
}

/// A watcher that notes when anything the table names changes here.
fn watch(home: &Home, changed: &Arc<Mutex<Option<Instant>>>) -> Option<notify::RecommendedWatcher> {
    use notify::Watcher as _;
    let seen = changed.clone();
    let named = home.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok_and(|event| event.paths.iter().any(|path| named.names(path))) {
            *seen.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
        }
    })
    .ok()?;
    for (dir, all) in home.watched() {
        let mode = if all {
            notify::RecursiveMode::Recursive
        } else {
            notify::RecursiveMode::NonRecursive
        };
        // A folder not there yet is not watched: its making is seen from
        // the folder holding it, and what is written in it after waits for
        // the clock.
        let _ = watcher.watch(&dir, mode);
    }
    Some(watcher)
}

impl CockpitView {
    /// Each saved Machine the window holds, as Setup sync wants it.
    fn setup_sync_wanted(&self, cx: &App) -> Vec<Wanted> {
        let Some(machines) = self.machines.as_ref() else {
            return Vec::new();
        };
        let cockpit = self.cockpit.read(cx);
        machines
            .rows(cockpit)
            .into_iter()
            .filter_map(|row| {
                let saved = machines.machine(row.index)?;
                let machine = row.held?;
                Some(Wanted {
                    target: saved.target.clone(),
                    name: saved.name.clone(),
                    machine,
                    attached: matches!(cockpit.server(machine), Some(ServerState::Attached { .. })),
                    on: saved.sync_setup,
                })
            })
            .collect()
    }

    pub(super) fn tick_setup_sync(&mut self, cx: &mut Context<Self>) {
        let wanted = self.setup_sync_wanted(cx);
        if self.setup_sync.update(cx, |sync, cx| sync.tick(wanted, cx)) {
            self.setup_sync_moved(cx);
        }
    }

    /// A line or the news moved: Settings › Connections draws it, and the
    /// toast is put up as the window draws.
    fn setup_sync_moved(&mut self, cx: &mut Context<Self>) {
        if let Some(settings) = self.settings_sheet(cx) {
            settings.update(cx, |_, cx| cx.notify());
        }
        if self.setup_sync.read(cx).untold {
            cx.notify();
        }
    }

    pub(super) fn sync_setup_anyway(&mut self, target: &str, cx: &mut Context<Self>) {
        self.setup_sync.update(cx, |sync, cx| sync.anyway(target, cx));
        self.setup_sync_moved(cx);
    }

    pub(super) fn sync_setup_sign_ins(&mut self, target: &str, side: Side, cx: &mut Context<Self>) {
        self.setup_sync.update(cx, |sync, cx| sync.use_sign_ins(target, side, cx));
        self.setup_sync_moved(cx);
    }

    pub(super) fn sync_setup_before_start(&mut self, machine: MachineId, cx: &mut Context<Self>) {
        self.setup_sync.update(cx, |sync, cx| sync.before_start(machine, cx));
        self.setup_sync_moved(cx);
    }

    /// Put up what the syncs have to say, in place of what it said before,
    /// or take it down once there is nothing.
    pub(super) fn present_setup_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use gpui::component::button::Button;
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        let Some(toast) = self.setup_sync.update(cx, |sync, _| sync.untold()) else {
            return;
        };
        let Some(Toast { title, message, folder, warning }) = toast else {
            window.remove_notification::<SetupSyncToast>(cx);
            return;
        };
        let sync = self.setup_sync.downgrade();
        let notification = Notification::new()
            .id::<SetupSyncToast>()
            .title(title)
            .message(message)
            .with_type(if warning { NotificationType::Warning } else { NotificationType::Info })
            .autohide(false)
            .on_close(move |_, cx| {
                sync.update(cx, |sync, _| sync.heard()).ok();
            });
        let notification = match folder {
            Some(folder) => notification.action(move |_, _, _| {
                let folder = folder.clone();
                Button::new("setup-sync-open-kept")
                    .label("Open folder")
                    .on_click(move |_, _, cx| cx.open_with_system(&folder))
            }),
            None => notification,
        };
        window.push_notification(notification, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track::new(MachineId::unheld(), "box")
    }

    fn kept(path: &str, sync: &str) -> Kept {
        let kept = path.split('/').fold(PathBuf::from("/settings/sync-conflicts").join(sync), |at, part| at.join(part));
        Kept {
            path: path.into(),
            winner: Side::There,
            kept,
        }
    }

    #[test]
    fn the_clock_comes_round_every_minute_whatever_else_syncs() {
        let mut track = track();
        let start = Instant::now();
        assert_eq!(track.due(start, true, false), Some(Ask::CHECK), "as it connects");
        for second in [3, 23, 43] {
            assert_eq!(track.due(start + Duration::from_secs(second), false, true), Some(Ask::HERE), "a change here at {second}s");
        }
        assert_eq!(track.due(start + Duration::from_secs(59), false, false), None);
        assert_eq!(track.due(start + CHECK, false, false), Some(Ask::CHECK), "not moved by the syncs a change here started");
        assert_eq!(track.due(start + CHECK + Duration::from_secs(1), false, false), None);
        assert_eq!(track.due(start + 2 * CHECK, false, true), Some(Ask::CHECK), "the clock's sync carries a change here too");
    }

    #[test]
    fn only_the_connection_and_the_clock_read_the_sign_ins() {
        assert!(Ask::CHECK.read_sign_ins);
        assert!(!Ask::HERE.read_sign_ins && !Ask::WHOLE.read_sign_ins);
        assert!(Ask::HERE.and(Ask::CHECK).read_sign_ins);
        assert!(!Ask::HERE.and(Ask::CHECK).only_if_changed_here);
    }

    #[test]
    fn a_sync_that_finds_another_under_way_shows_syncing_and_tries_again() {
        let mut track = track();
        let mut news = News::default();
        let now = Instant::now();
        track.status = Status::InSync(BTreeMap::new());
        assert!(!track.finished(Ok(Outcome::Busy), Ask::HERE, now, &mut news));
        assert_eq!(track.status, Status::Syncing(None));
        assert!(!track.status.failed());
        assert_eq!(track.due(now + Duration::from_secs(1), false, false), None, "it waits a moment");
        assert_eq!(track.due(now + 2 * SETTLE, false, false), Some(Ask::WHOLE), "all of it, as the other sync may have moved anything");
    }

    /// Two computers that are each other's hub, each holding its own home
    /// as it asks for the other's, find each other busy at once: each
    /// tries again at a moment of its own, or they would meet each time.
    #[test]
    fn syncs_that_found_each_other_busy_try_again_at_moments_of_their_own() {
        let now = Instant::now();
        let tried: std::collections::BTreeSet<Duration> = (0..8)
            .map(|_| {
                let mut track = track();
                track.finished(Ok(Outcome::Busy), Ask::HERE, now, &mut News::default());
                let mut after = (0..=600).map(|step| Duration::from_millis(10 * step));
                after.find(|after| track.due(now + *after, false, false).is_some()).unwrap()
            })
            .collect();
        assert!(tried.iter().all(|after| (SETTLE..=2 * SETTLE).contains(after)), "{tried:?}");
        assert!(tried.len() > 1, "each tried again at the same moment: {tried:?}");
    }

    /// A hold the Machine let lapse reads as that Machine, by its name,
    /// giving up on this computer, not as the path the sync was at.
    #[test]
    fn a_lapsed_hold_says_which_machine_stopped_waiting() {
        let mut track = track();
        let stuck = Stuck {
            path: ".claude/CLAUDE.md".into(),
            side: Side::There,
            error: std::io::Error::other(tree::LAPSED),
        };
        track.finished(Err(std::io::Error::other(stuck)), Ask::HERE, Instant::now(), &mut News::default());
        assert_eq!(track.status.line("box"), "box stopped waiting for this computer; the next sync starts over");
    }

    const SIGN_IN: &str = ".claude/.credentials.json";

    fn valve(item: &str) -> Valve {
        Valve {
            side: Side::Here,
            item: item.into(),
            removing: 4,
            holding: 4,
            vanished: true,
        }
    }

    fn moved_one() -> Outcome {
        Outcome::Synced(Synced {
            pushed: 1,
            ..Synced::default()
        })
    }

    /// Connections' row says each status as what it offers: a refusal
    /// draws the switch off, a stop offers to go on, two sign-ins offer
    /// each side's, and the line is the one the Machine's status says.
    #[test]
    fn a_status_is_said_on_its_connections_row() {
        use ferrite_settings::connections::Sync;
        let refused = Status::Refused("This window syncs no setup".into());
        assert_eq!(refused.row("box"), Sync { refused: true, line: "This window syncs no setup".into(), ..Sync::default() });
        let stopped = Status::Stopped(valve(SIGN_IN));
        assert_eq!(stopped.row("box"), Sync { stopped: true, failed: true, line: stopped.line("box"), ..Sync::default() });
        let held = Status::InSync(BTreeMap::from([(SIGN_IN.to_string(), Why::Held(Hold::SignIn))]));
        assert_eq!(held.row("box"), Sync { sign_ins: true, line: held.line("box"), ..Sync::default() });
        assert_eq!(Status::InSync(BTreeMap::new()).row("box"), Sync { line: "In sync".into(), ..Sync::default() });
        assert_eq!(Status::Older.row("box"), Sync { failed: true, line: "Update Ferrite on box to sync your setup".into(), ..Sync::default() });
    }

    /// "Sync anyway" asks what the sync that stopped asked, whole and
    /// removing what it removes, so whatever stop the line shows, from
    /// whichever sync, it clears.
    #[test]
    fn sync_anyway_asks_what_stopped_and_clears_any_stop() {
        let now = Instant::now();
        for ask in [Ask::CHECK, Ask::HERE, Ask::WHOLE] {
            for item in setup_sync::ITEMS.iter().map(|item| item.path).filter(|item| ask.reads(item)) {
                let mut track = track();
                let mut news = News::default();
                assert_eq!(track.anyway(), None, "nothing stopped");
                track.finished(Ok(Outcome::Stopped(valve(item))), ask, now, &mut news);
                let again = track.anyway().expect("a stop");
                assert!(again.allow_removals && !again.only_if_changed_here, "{ask:?}");
                assert!(again.reads(item), "{ask:?} stopped at {item}: Sync anyway reads it too");
                track.status = Status::Syncing(Some(4));
                track.finished(Ok(Outcome::Synced(Synced::default())), again, now, &mut news);
                assert_eq!(track.status, Status::InSync(BTreeMap::new()), "{ask:?} stopped at {item}");
                assert_eq!((news.stopped.as_ref(), track.anyway()), (None, None), "{ask:?} stopped at {item}");
            }
        }
    }

    /// A sync that leaves a path unread, as one a change here starts
    /// leaves Claude's sign-in, never clears a hold or a stop about it,
    /// whatever the line showed while it ran.
    #[test]
    fn a_narrower_sync_never_clears_what_it_did_not_read() {
        let now = Instant::now();
        let mut track = track();
        let mut news = News::default();
        let signed_in = |path: &str| {
            Outcome::Synced(Synced {
                held: BTreeMap::from([(path.to_string(), Hold::SignIn)]),
                ..Synced::default()
            })
        };
        track.finished(Ok(signed_in(SIGN_IN)), Ask::CHECK, now, &mut news);
        for narrower in [Ask::HERE, Ask::WHOLE] {
            track.status = Status::Syncing(Some(1));
            track.finished(Ok(moved_one()), narrower, now, &mut news);
            assert!(track.status.sign_ins(), "{narrower:?}: {:?}", track.status);
        }
        track.finished(Ok(Outcome::Synced(Synced::default())), Ask::CHECK, now, &mut news);
        assert_eq!(track.status, Status::InSync(BTreeMap::new()), "the clock read it again");
        track.finished(Ok(signed_in(".codex/auth.json")), Ask::CHECK, now, &mut news);
        track.finished(Ok(moved_one()), Ask::HERE, now, &mut news);
        assert_eq!(track.status, Status::InSync(BTreeMap::new()), "Codex's sign-in is read by every sync");

        track.finished(Ok(Outcome::Stopped(valve(SIGN_IN))), Ask::CHECK, now, &mut news);
        for narrower in [Ask::HERE, Ask::WHOLE] {
            track.status = Status::Syncing(Some(1));
            track.finished(Ok(moved_one()), narrower, now, &mut news);
            assert_eq!(track.status, Status::Stopped(valve(SIGN_IN)), "{narrower:?}");
            assert_eq!(news.stopped, Some(valve(SIGN_IN)));
        }
        track.finished(Ok(Outcome::Stopped(valve(".claude/skills"))), Ask::CHECK, now, &mut news);
        track.finished(Ok(moved_one()), Ask::HERE, now, &mut news);
        assert_eq!(track.status, Status::InSync(BTreeMap::new()), "it read the skills");
    }

    #[test]
    fn a_change_here_that_changed_nothing_leaves_a_stop_standing() {
        let mut track = track();
        let mut news = News::default();
        let now = Instant::now();
        let valve = Valve {
            side: Side::Here,
            item: ".claude/skills".into(),
            removing: 410,
            holding: 410,
            vanished: true,
        };
        assert!(track.finished(Ok(Outcome::Stopped(valve.clone())), Ask::CHECK, now, &mut news));
        assert!(!track.finished(Ok(Outcome::Synced(Synced::default())), Ask::HERE, now, &mut news));
        assert_eq!(track.status, Status::Stopped(valve.clone()));
        assert_eq!(news.stopped, Some(valve));
    }

    #[test]
    fn one_toast_says_a_syncs_conflicts_and_a_failure_raises_one() {
        let mut track = track();
        let mut news = News::default();
        let now = Instant::now();
        let synced = Synced {
            conflicts: vec![kept(".claude/CLAUDE.md", "1"), kept(".claude/agents/a.md", "1"), kept(".codex/AGENTS.md", "1")],
            ..Synced::default()
        };
        assert!(track.finished(Ok(Outcome::Synced(synced)), Ask::CHECK, now, &mut news));
        let all = BTreeMap::from([("box".to_string(), news.clone())]);
        let Toast { title, message, folder, warning } = toast(&all).unwrap();
        assert!(warning);
        assert_eq!(title, "3 files changed on both sides");
        assert!(message.ends_with("the other copy of each is kept in Ferrite's sync-conflicts folder."), "{message}");
        assert_eq!(folder, Some(PathBuf::from("/settings/sync-conflicts/1")));

        let failed = || {
            let error = std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "/home/me/.claude/skills/a.md: Permission denied (os error 13)\nmore",
            );
            let stuck = Stuck {
                path: ".claude/skills/a.md".into(),
                side: Side::There,
                error,
            };
            Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, stuck))
        };
        assert!(track.finished(failed(), Ask::CHECK, now, &mut news));
        assert_eq!(track.status.line("box"), "~/.claude/skills/a.md on box: Permission denied");
        assert!(!track.finished(failed(), Ask::CHECK, now, &mut news), "still failing: no second toast");
        let all = BTreeMap::from([("box".to_string(), news.clone())]);
        let Toast { title, message, .. } = toast(&all).unwrap();
        assert_eq!(title, "Setup sync: 2 things to look at", "one toast, the first not hidden");
        assert_eq!(
            message,
            "box: 3 files changed on both sides\nbox: failed, ~/.claude/skills/a.md on box: Permission denied\nMore in Settings › Connections."
        );
        news.stopped = Some(valve(".claude/skills"));
        let all = BTreeMap::from([("box".to_string(), news.clone())]);
        let Toast { message, .. } = toast(&all).unwrap();
        assert!(message.ends_with("\nSync anyway in Settings › Connections."), "{message}");
        news.stopped = None;

        let unplaced = std::io::Error::other("connection reset (os error 104)");
        assert_eq!(Status::Failed(Failure::of(&unplaced)).line("box"), "connection reset");

        assert!(track.finished(Ok(Outcome::Synced(Synced::default())), Ask::CHECK, now, &mut news));
        assert_eq!(news.failed, None, "it synced again");
        assert_eq!(news.conflicts.len(), 3);
    }

    #[test]
    fn a_key_one_side_lost_is_said_with_what_it_was_and_asks_nothing() {
        let mut track = track();
        let mut news = News::default();
        let replaced = Replaced {
            path: ".claude/settings.json#model".into(),
            winner: Side::Here,
            was: "\"sonnet\"".into(),
        };
        let synced = Synced { pushed: 1, replaced: vec![replaced], ..Synced::default() };
        assert!(track.finished(Ok(Outcome::Synced(synced)), Ask::CHECK, Instant::now(), &mut news));
        assert_eq!(track.status.line("box"), "In sync");
        let all = BTreeMap::from([("box".to_string(), news)]);
        let shown = toast(&all).unwrap();
        assert_eq!(shown.title, "Claude's model was set differently on box");
        assert_eq!(shown.message, "It is this computer's on both now. It was \"sonnet\" on box, kept in its sync-trash folder.");
        assert_eq!((shown.folder, shown.warning), (None, false), "nothing to open, nothing to do");
    }

    /// A toast names what changed as the operator calls it, never by a
    /// path where a name does, and says where each old copy went: a file's
    /// in the sync-conflicts folder, a key's in the trash of the side that
    /// lost it.
    #[test]
    fn a_toast_names_what_changed_and_where_each_old_copy_is_kept() {
        let mut news = News::default();
        news.conflicted(&[kept(".claude/skills/both-edit/SKILL.md", "1")]);
        let one = BTreeMap::from([("ferrite-wsl".to_string(), news.clone())]);
        assert_eq!(toast(&one).unwrap().title, "The skill both-edit changed on both sides");
        news.replaced(&[
            Replaced { path: ".claude/settings.json#model".into(), winner: Side::Here, was: "\"sonnet\"".into() },
            Replaced { path: ".codex/config.toml#model".into(), winner: Side::There, was: "\"gpt-6.2-sol\"".into() },
        ]);
        let all = BTreeMap::from([("ferrite-wsl".to_string(), news)]);
        let Toast { title, message, .. } = toast(&all).unwrap();
        assert_eq!(title, "Setup sync: 3 things to look at");
        assert_eq!(
            message,
            "ferrite-wsl: the skill both-edit changed on both sides\n\
             ferrite-wsl: Claude's model is this computer's on both now; it was \"sonnet\" on ferrite-wsl, kept in its sync-trash folder\n\
             ferrite-wsl: Codex's model is ferrite-wsl's on both now; it was \"gpt-6.2-sol\" here, kept in Ferrite's sync-trash folder\n\
             The other copy of each file is kept in Ferrite's sync-conflicts folder."
        );
        assert!(!message.contains('`') && !title.contains('`'), "{message}");

        for (path, name) in [
            (".claude/commands/team.md", "the command team"),
            (".claude/agents/helper.md", "the agent helper"),
            (".claude/rules/style.md", "the rule style"),
            (".agents/skills/reui/SKILL.md", "the skill reui"),
            (".claude/skills/next.js", "the skill next.js"),
            (".claude/skills/README.md", "~/.claude/skills/README.md"),
            (".claude/settings.json#skillOverrides", "Claude's skill overrides"),
            (".claude/settings.json#model", "Claude's model"),
            (".codex/config.toml#model_reasoning_effort", "Codex's model reasoning effort"),
            (".codex/prompts/fix.md", "the Codex prompt fix"),
            (".claude/CLAUDE.md", "CLAUDE.md"),
            (".codex/AGENTS.md", "Codex's AGENTS.md"),
            (".claude/settings.json#enabledPlugins/pstack@cursor-plugins", "Claude's plugin pstack"),
            (".claude.json#mcpServers/docs", "Claude's MCP server docs"),
            (".codex/config.toml#marketplaces/cursor-plugins", "Codex's marketplace cursor-plugins"),
            (".claude/hooks/check.sh", "~/.claude/hooks/check.sh"),
        ] {
            assert_eq!(named(path), name);
        }
    }

    #[test]
    fn a_machine_by_another_table_says_to_update_it_and_a_plugin_that_did_not_install_says_why() {
        let mut track = track();
        let older = std::io::Error::new(std::io::ErrorKind::Unsupported, "This server does not know that kind of root");
        track.finished(Err(older), Ask::CHECK, Instant::now(), &mut News::default());
        assert_eq!(track.status.line("box"), "Update Ferrite on box to sync your setup");
        assert!(track.status.failed());

        let synced = Synced {
            problems: vec![ferrite_core::tree::Problem {
                path: ".claude/plugins/pstack@cursor-plugins".into(),
                side: Side::There,
                error: "Plugin \"pstack\" not found in marketplace \"cursor-plugins\".".into(),
            }],
            ..Synced::default()
        };
        assert_eq!(
            Status::InSync(held(&synced).collect()).line("box"),
            "In sync except pstack could not be installed on box: Plugin \"pstack\" not found in marketplace \"cursor-plugins\"."
        );
    }

    /// Plugins that did not install here are counted as plugins, and this
    /// computer is named as itself.
    #[test]
    fn plugins_that_did_not_install_here_are_counted_as_plugins_on_this_computer() {
        let problem = |path: &str, side: Side, error: &str| ferrite_core::tree::Problem {
            path: path.into(),
            side,
            error: error.into(),
        };
        let synced = Synced {
            problems: vec![
                problem(".claude/plugins/pstack@cursor-plugins", Side::Here, "Plugin \"pstack\" not found."),
                problem(".claude/plugins/sentry@cursor-plugins", Side::Here, "Plugin \"sentry\" not found."),
            ],
            ..Synced::default()
        };
        let line = Status::InSync(held(&synced).collect()).line("box");
        assert_eq!(line, "In sync except 2 plugins. pstack could not be installed on this computer: Plugin \"pstack\" not found.");
        let mixed = Synced {
            problems: vec![
                problem(".claude/plugins/pstack@cursor-plugins", Side::There, "Plugin \"pstack\" not found."),
                problem(".claude/skills/a.md", Side::There, "Permission denied"),
            ],
            ..Synced::default()
        };
        let line = Status::InSync(held(&mixed).collect()).line("box");
        assert!(line.starts_with("In sync except 1 path and 1 plugin. "), "{line}");
    }

    #[test]
    fn a_skill_that_is_a_folder_on_one_side_and_a_link_on_the_other_says_so() {
        let on = |path: &str, folder: Side| {
            let synced = Synced { held: BTreeMap::from([(path.to_string(), Hold::FolderAndLink(folder))]), ..Synced::default() };
            Status::InSync(held(&synced).collect()).line("josephpc")
        };
        assert_eq!(
            on(".claude/skills/reui", Side::There),
            "In sync except the skill reui is a folder on josephpc and a link here: left as each has it"
        );
        assert_eq!(
            on(".claude/skills/reui", Side::Here),
            "In sync except the skill reui is a folder here and a link on josephpc: left as each has it"
        );
        assert_eq!(
            on(".claude/commands/team", Side::There),
            "In sync except ~/.claude/commands/team is a folder on josephpc and a link here: left as each has it"
        );
    }

    #[test]
    fn a_line_says_what_was_held_and_where_a_sync_stopped() {
        let synced = Synced {
            problems: vec![ferrite_core::tree::Problem {
                path: ".claude/skills".into(),
                side: Side::There,
                error: "it is a link to something not there".into(),
            }],
            ..Synced::default()
        };
        let line = |synced: &Synced| Status::InSync(held(synced).collect());
        assert_eq!(
            line(&synced).line("box"),
            "In sync except ~/.claude/skills on box: it is a link to something not there"
        );
        let signed_in = Synced {
            held: BTreeMap::from([(SIGN_IN.to_string(), Hold::SignIn)]),
            ..Synced::default()
        };
        assert!(line(&signed_in).sign_ins());
        let gone = Valve {
            side: Side::Here,
            item: ".claude/skills".into(),
            removing: 4,
            holding: 4,
            vanished: true,
        };
        assert_eq!(
            Status::Stopped(gone.clone()).line("box"),
            "Stopped: ~/.claude/skills is gone from box, so syncing would remove its 4 files here"
        );
        let gone_here = Valve { side: Side::There, ..gone };
        assert_eq!(
            Status::Stopped(gone_here).line("box"),
            "Stopped: ~/.claude/skills is gone here, so syncing would remove its 4 files on box"
        );
    }
}
