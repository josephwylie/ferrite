//! Machines in the window (ADR 0026, ADR 0045), with fake bridges: no ssh
//! runs. The window holds every enabled machine at once, and the nav lists
//! every machine's Threads in its one list, another machine's with its
//! server glyph; a machine the window cannot reach says so in a notice; a
//! press on any machine's Thread opens it in place; every act goes to its
//! Thread's machine; a machine that went dims only its own rows and Panes;
//! each machine's saved state is its own; and a Project and its first
//! Thread are made on a machine in a directory typed there.

use super::*;
use crate::cockpit::server::Gone;
use crate::machines::{Connection, Machines, Reach};
use ferrite_core::machine::{self, Bridge, Failure, Machine, Tunnel};
use ferrite_core::{MachineId, MachineKey, SavedThread};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

mod environments;
mod everywhere;
mod on_its_machine;

/// A bridge whose every tunnel ends as ssh does after `failure`.
fn failing(machine: &Machine, settings_dir: &std::path::Path, failure: Failure) -> Bridge {
    Bridge::new(
        &machine.name,
        machine::dir(settings_dir, &machine.target).join("bridge.sock"),
        Box::new(move || {
            let failure = failure.clone();
            Ok(Tunnel {
                from: Box::new(std::io::empty()),
                to: Box::new(std::io::sink()),
                ended: Box::new(move || Some(failure)),
            })
        }),
    )
}

/// Ssh needs the operator to sign in.
fn needs_sign_in(machine: &Machine, settings_dir: &std::path::Path) -> Bridge {
    failing(machine, settings_dir, Failure::SignIn)
}

/// The machine no longer takes this link's session.
fn revoked(machine: &Machine, settings_dir: &std::path::Path) -> Bridge {
    failing(machine, settings_dir, Failure::Revoked)
}

/// The machine's shell has no `ferrited`.
fn not_installed(machine: &Machine, settings_dir: &std::path::Path) -> Bridge {
    failing(machine, settings_dir, Failure::NotInstalled)
}

/// Every byte a window sent `box`'s Server through `serving`.
static SENT_TO_BOX: Mutex<Vec<u8>> = Mutex::new(Vec::new());
/// While set, box's Server is down: its links have ended and no tunnel
/// reaches it (`box_down`).
static BOX_DOWN: AtomicBool = AtomicBool::new(false);
/// Where box's Server takes nothing more a window sends, as over a line
/// that stalled, till the test lets it go.
enum Hold {
    /// Once this file is there (`hold_box_once`).
    Landed(std::path::PathBuf),
    /// From the write that asks it to put in place the file whose SHA-256
    /// this is: that file's `finish` is on its way, not yet taken
    /// (`hold_box_at_finish`).
    Finish(String),
}

impl Hold {
    /// Whether box holds what a window sends, `seen` the end of what it
    /// sent on this link, this write last.
    fn holds(&self, seen: &[u8]) -> bool {
        match self {
            Hold::Landed(landed) => landed.exists(),
            Hold::Finish(sha256) => seen.windows(sha256.len()).any(|at| at == sha256.as_bytes()),
        }
    }
}

static BOX_HELD: Mutex<Option<Hold>> = Mutex::new(None);
static BOX_LET_GO: std::sync::Condvar = std::sync::Condvar::new();
/// Box holds a write now.
static BOX_HOLDING: AtomicBool = AtomicBool::new(false);
/// Box's links as `serving` made them, to end when it goes down.
static BOX_LINKS: Mutex<Vec<ferrite_core::link::Stream>> = Mutex::new(Vec::new());

/// Box's Server takes nothing more once `landed` is there: a copy that
/// lands it is held part way, however loaded the machine, till the hold
/// lets go or drops, so a test that fails while box holds ends.
fn hold_box_once(landed: std::path::PathBuf) -> BoxHeld {
    hold_box(Hold::Landed(landed))
}

/// Box's Server takes nothing more from the write that asks it to put
/// `path` of the Project at `source` in place: a copy is held with that
/// file asked for and not yet there, till the hold lets go or drops.
fn hold_box_at_finish(source: &std::path::Path, path: &str) -> BoxHeld {
    let listed = ferrite_core::tree::scan(source, &[]).unwrap();
    let Some(ferrite_core::tree::Entry::File { sha256, .. }) = listed.entries.get(path) else {
        panic!("{path} is no file of {}", source.display());
    };
    hold_box(Hold::Finish(sha256.clone()))
}

fn hold_box(hold: Hold) -> BoxHeld {
    BOX_HOLDING.store(false, Ordering::SeqCst);
    *BOX_HELD.lock().unwrap() = Some(hold);
    BoxHeld
}

#[must_use = "box is let go when the hold drops"]
struct BoxHeld;

impl BoxHeld {
    /// Whether box holds a write at the hold now.
    fn holds(&self) -> bool {
        BOX_HOLDING.load(Ordering::SeqCst)
    }

    /// Box takes what it is sent again.
    fn let_go(self) {}
}

impl Drop for BoxHeld {
    fn drop(&mut self) {
        *BOX_HELD.lock().unwrap_or_else(|e| e.into_inner()) = None;
        BOX_HOLDING.store(false, Ordering::SeqCst);
        BOX_LET_GO.notify_all();
    }
}

/// Box's Server stops, ending every link to it, or starts again.
fn box_down(down: bool) {
    use ferrite_core::link::TryClone;
    BOX_DOWN.store(down, Ordering::SeqCst);
    if down {
        for link in BOX_LINKS.lock().unwrap().drain(..) {
            let _ = link.shutdown();
        }
    }
}

/// `box`'s Server runs in this process (`ferrite_server::fixture_machine`),
/// its Store in `box` under the settings directory; each tunnel reaches it
/// as ssh would, keeping what the window sends in `SENT_TO_BOX`, unless
/// box is down.
fn serving(machine: &Machine, settings_dir: &std::path::Path) -> Bridge {
    use ferrite_core::link::{self, TryClone};
    /// Box's end of a link, and the end of what was sent on it: a request
    /// split across writes is seen whole.
    struct Kept(link::Stream, Vec<u8>);
    impl std::io::Write for Kept {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut seen = std::mem::take(&mut self.1);
            seen.extend_from_slice(buf);
            let holds = |held: &Option<Hold>| held.as_ref().is_some_and(|hold| hold.holds(&seen));
            let mut held = BOX_HELD.lock().unwrap();
            if holds(&held) {
                BOX_HOLDING.store(true, Ordering::SeqCst);
                // A test waiting for the hold looks again: a held copy
                // says nothing more.
                ferrite_core::doorbell::ring();
            }
            while holds(&held) {
                held = BOX_LET_GO.wait(held).unwrap();
            }
            drop(held);
            self.1 = seen[seen.len().saturating_sub(128)..].to_vec();
            SENT_TO_BOX.lock().unwrap().extend_from_slice(buf);
            self.0.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.flush()
        }
    }
    impl Drop for Kept {
        fn drop(&mut self) {
            let _ = self.0.shutdown();
        }
    }
    let address = link::address(&settings_dir.join("box"));
    Bridge::new(
        &machine.name,
        machine::dir(settings_dir, &machine.target).join("bridge.sock"),
        Box::new(move || {
            if BOX_DOWN.load(Ordering::SeqCst) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::ConnectionRefused,
                    "box is down",
                ));
            }
            let stream = link::connect(&address)?;
            BOX_LINKS.lock().unwrap().push(stream.try_clone()?);
            Ok(Tunnel {
                from: Box::new(stream.try_clone()?),
                to: Box::new(Kept(stream, Vec::new())),
                ended: Box::new(|| None),
            })
        }),
    )
}

/// `box`'s Sessions: each keeps where it was started, and says nothing.
pub(super) struct OnBox(pub(super) std::sync::Arc<std::sync::Mutex<Vec<std::path::PathBuf>>>);

struct Quiet {
    _alive: ferrite_core::doorbell::Sender<SessionEvent>,
    events: mpsc::Receiver<SessionEvent>,
}

impl ferrite_engine::engine::Spawner for OnBox {
    fn spawn(
        &mut self,
        request: ferrite_engine::engine::SpawnRequest,
    ) -> std::io::Result<Box<dyn ferrite_engine::providers::Session>> {
        self.0
            .lock()
            .unwrap()
            .extend(request.cwd.map(std::path::Path::to_path_buf));
        let (alive, events) = ferrite_core::doorbell::channel();
        Ok(Box::new(Quiet {
            _alive: alive,
            events,
        }))
    }
}

impl ferrite_engine::providers::Session for Quiet {
    fn events(&self) -> &mpsc::Receiver<SessionEvent> {
        &self.events
    }
    fn send(&mut self, _: &str) -> std::io::Result<()> {
        Ok(())
    }
    fn interrupt(&mut self) -> std::io::Result<()> {
        Ok(())
    }
    fn respond_to_decision(&mut self, _: &str, _: DecisionAnswer) -> std::io::Result<()> {
        Ok(())
    }
}

/// box's agents, as the test speaks for them: a line into each Session in
/// the order they started, what each was sent, and how often one was
/// stopped. A stopped turn ends, as a provider's does.
#[derive(Clone, Default)]
struct Agents {
    lines: Arc<Mutex<Vec<ferrite_core::doorbell::Sender<SessionEvent>>>>,
    sent: Arc<Mutex<Vec<String>>>,
    interrupts: Arc<AtomicUsize>,
    /// Set, every Session refuses what it is sent.
    refuse: Arc<AtomicBool>,
}

impl Agents {
    /// Session `index` says `event`.
    fn say(&self, index: usize, event: SessionEvent) {
        self.lines.lock().unwrap()[index].send(event).unwrap();
    }

    fn sent(&self, text: &str) -> bool {
        self.sent.lock().unwrap().iter().any(|sent| sent.contains(text))
    }
}

struct Agent {
    line: ferrite_core::doorbell::Sender<SessionEvent>,
    events: mpsc::Receiver<SessionEvent>,
    sent: Arc<Mutex<Vec<String>>>,
    interrupts: Arc<AtomicUsize>,
    refuse: Arc<AtomicBool>,
}

impl ferrite_engine::engine::Spawner for Agents {
    fn spawn(
        &mut self,
        _: ferrite_engine::engine::SpawnRequest,
    ) -> std::io::Result<Box<dyn ferrite_engine::providers::Session>> {
        let (line, events) = ferrite_core::doorbell::channel();
        self.lines.lock().unwrap().push(line.clone());
        Ok(Box::new(Agent {
            line,
            events,
            sent: self.sent.clone(),
            interrupts: self.interrupts.clone(),
            refuse: self.refuse.clone(),
        }))
    }
}

impl ferrite_engine::providers::Session for Agent {
    fn events(&self) -> &mpsc::Receiver<SessionEvent> {
        &self.events
    }
    fn send(&mut self, text: &str) -> std::io::Result<()> {
        if self.refuse.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("the agent refused it"));
        }
        self.sent.lock().unwrap().push(text.into());
        Ok(())
    }
    fn interrupt(&mut self) -> std::io::Result<()> {
        self.interrupts.fetch_add(1, Ordering::SeqCst);
        let _ = self.line.send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Interrupted,
            cost_usd: None,
        });
        Ok(())
    }
    fn respond_to_decision(&mut self, _: &str, _: DecisionAnswer) -> std::io::Result<()> {
        Ok(())
    }
    /// It can cut its conversation, so it offers a side chat.
    fn supports_control(&self, kind: ferrite_core::ControlKind) -> bool {
        kind == ferrite_core::ControlKind::Rewind
    }
    /// Forks afresh, at once: the fork's agent is the next Session.
    fn fork(
        &mut self,
        _: ferrite_engine::providers::ForkRequest,
    ) -> std::io::Result<ferrite_engine::providers::ForkAck> {
        let (answer, ack) = ferrite_core::doorbell::sync_channel(1);
        answer.send(Ok(ferrite_engine::providers::Forked::Fresh)).unwrap();
        Ok(ack)
    }
}

/// Where the machines of a test called `name` keep their settings.
fn settings_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("fv-{}-{name}", std::process::id()))
}

/// `settings_dir(name)`, emptied of what an earlier run left.
fn fresh(name: &str) {
    let _ = std::fs::remove_dir_all(settings_dir(name));
}

/// How the window of a test called `name` reaches its machines: box
/// through `bridge`, and this computer through a Server that cannot start.
fn reach(name: &str, bridge: fn(&Machine, &std::path::Path) -> Bridge) -> Reach {
    let dir = settings_dir(name);
    Reach {
        settings_dir: dir.clone(),
        store: dir.join("threads"),
        executable: dir.join("no-such-ferrite"),
        client: ferrite_core::protocol::Peer {
            name: "test".into(),
            version: "0".into(),
        },
        renderer: None,
        driven: false,
        bridge,
    }
}

/// box, saved in Settings and turned on.
fn box_machine() -> Machine {
    Machine {
        name: "box".into(),
        ..Machine::new("me@box")
    }
}

/// box's name in saved files.
fn on_box_key() -> MachineKey {
    MachineKey(Some("me@box".into()))
}

/// This computer as `core` holds it, and box beside it, reached through
/// `bridge`.
fn beside_box(
    name: &str,
    core: &mut Cockpit,
    bridge: fn(&Machine, &std::path::Path) -> Bridge,
) -> Machines {
    fresh(name);
    let mut machines = Machines::new(reach(name, bridge), &[box_machine()], None, true);
    machines.beside(core);
    machines
}

/// box alone, reached through `bridge`: this computer is turned off.
fn box_alone(name: &str, bridge: fn(&Machine, &std::path::Path) -> Bridge) -> (Cockpit, Machines) {
    fresh(name);
    let mut machines = Machines::new(reach(name, bridge), &[box_machine()], None, false);
    let core = machines.cockpit();
    (core, machines)
}

fn attached(core: &Cockpit, machine: MachineId) -> bool {
    matches!(
        core.server(machine),
        Some(ferrite_client::cockpit::ServerState::Attached { .. })
    )
}

/// This computer, an Engine of its own with `panes` Threads, and box
/// beside it, whose Server runs in this process with `Agents` for its
/// Sessions: both attached.
fn two_machines(name: &str, panes: usize) -> (Cockpit, Fake, Machines, Agents) {
    two_machines_with(name, panes, box_machine())
}

/// `two_machines`, box saved as `saved`.
fn two_machines_with(name: &str, panes: usize, saved: Machine) -> (Cockpit, Fake, Machines, Agents) {
    let agents = Agents::default();
    let (core, fake, machines) = beside_serving(name, panes, saved, Box::new(agents.clone()));
    (core, fake, machines, agents)
}

/// This computer, an Engine of its own with `panes` Threads, and box
/// beside it, saved as `saved`, whose Server runs in this process with
/// `spawner` for its CLIs: both attached.
fn beside_serving(
    name: &str,
    panes: usize,
    saved: Machine,
    spawner: Box<dyn ferrite_engine::engine::Spawner + Send>,
) -> (Cockpit, Fake, Machines) {
    let (mut core, fake) = cockpit(&format!("machines-{name}"), panes);
    fresh(name);
    // `serving` dials it itself, to keep what the window sends.
    let _ = ferrite_server::fixture_machine(settings_dir(name).join("box"), spawner).unwrap();
    let mut machines = Machines::new(reach(name, serving), &[saved], None, true);
    machines.beside(&mut core);
    let on_box = machines.held(1).expect("box is held");
    ferrite_core::doorbell::until("box never attached", || {
        core.pump();
        attached(&core, on_box)
    });
    (core, fake, machines)
}

/// A Claude Thread opened on `machine`, in this checkout.
fn open_on(core: &mut Cockpit, machine: MachineId) -> ThreadKey {
    core.open(machine, Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap()
}

/// Another client of box's Server — the board, another window — attached.
fn another_client(name: &str) -> Cockpit {
    use ferrite_client::cockpit::{Attach, Endpoint};
    use ferrite_core::link;
    let address = link::address(&settings_dir(name).join("box"));
    let dial: machine::Dial = Box::new(move || {
        let stream = link::connect(&address)?;
        Ok(Tunnel {
            from: Box::new(link::TryClone::try_clone(&stream)?),
            to: Box::new(stream),
            ended: Box::new(|| None),
        })
    });
    let other = settings_dir(name).join("other");
    let mut core = Cockpit::attach(Attach {
        saved_as: on_box_key(),
        browser: false,
        driven: false,
        store: other.join("threads"),
        server: Endpoint::Machine(Arc::new(Bridge::new("box", other.join("bridge.sock"), dial))),
        client: ferrite_core::protocol::Peer {
            name: "other".into(),
            version: "0".into(),
        },
        renderer: None,
    });
    let machine = core.first_machine();
    ferrite_core::doorbell::until("the other client never attached", || {
        core.pump();
        attached(&core, machine)
    });
    core
}

/// A window on every machine `machines` holds, which `core` holds.
fn window_of(
    cx: &mut TestAppContext,
    core: Cockpit,
    machines: Machines,
    prefs: Preferences,
) -> (Entity<CockpitView>, &mut gpui::VisualTestContext) {
    let prefs = listing(&machines, prefs);
    add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_machines(core, Provider::Claude, prefs, Some(machines), cx)
    })
}

/// `prefs`, its Settings saying what `machines` was made with: this
/// computer on or off, and the machines, when they list none. A window
/// follows Settings' machines at every change to Settings, as it was
/// started from them.
fn listing(machines: &Machines, mut prefs: Preferences) -> Preferences {
    prefs.settings.local_environment_enabled = machines.held(0).is_some();
    if prefs.settings.machines.is_empty() {
        prefs.settings.machines = (1..)
            .map_while(|index| machines.machine(index))
            .filter(|machine| machines.unsaved() != Some(*machine))
            .cloned()
            .collect();
    }
    prefs
}

/// `window_of`, inside the `CockpitWindow` main opens it in: what a
/// clicked system notification reaches.
fn machine_window(
    cx: &mut TestAppContext,
    core: Cockpit,
    machines: Machines,
    prefs: Preferences,
) -> (Entity<gpui::component::Root>, &mut gpui::VisualTestContext) {
    let prefs = listing(&machines, prefs);
    cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            CockpitView::new_with_machines(core, Provider::Claude, prefs, Some(machines), cx)
        });
        gpui::component::Root::new(
            cx.new(|cx| crate::cockpit_window::CockpitWindow::new(view, window, cx)),
            window,
            cx,
        )
    })
}

/// The Cockpit the window shows.
fn shown(
    root: &Entity<gpui::component::Root>,
    cx: &mut gpui::VisualTestContext,
) -> Entity<CockpitView> {
    root.read_with(cx, |root, cx| {
        crate::cockpit_window::testing::cockpit(root.view(), cx)
    })
}

/// Machine `index`'s connection, as Settings › Connections says it.
fn connection(
    view: &Entity<CockpitView>,
    index: usize,
    cx: &mut gpui::VisualTestContext,
) -> Connection {
    view.read_with(cx, |view, cx| {
        view.machines.as_ref().unwrap().rows(view.cockpit.read(cx))[index].connection
    })
}

/// What `thread`'s rows' ids end with: its machine's number first.
fn on(machine: MachineId, thread: ThreadKey) -> String {
    format!("{}-{}", machine.get(), thread.thread().get())
}

/// The notices of the machines the window cannot reach, as the nav draws
/// them.
fn machine_notices(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    view.read_with(cx, |view, cx| {
        let nav = view.nav_view.read(cx);
        let inputs = nav.inputs().expect("the nav has looked");
        inputs.machines.iter().map(|notice| notice.words.to_string()).collect()
    })
}

/// Whether `thread`'s transcript, as the window holds it, says `said`.
fn shows(
    view: &Entity<CockpitView>,
    thread: ThreadKey,
    said: &str,
    cx: &mut gpui::VisualTestContext,
) -> bool {
    view.read_with(cx, |view, cx| {
        view.cockpit.read(cx).thread(thread).is_some_and(|open| {
            open.transcript()
                .blocks()
                .iter()
                .any(|block| format!("{:?}", block.body).contains(said))
        })
    })
}

/// Whether `thread`'s turn runs, as the window holds it.
fn running(view: &Entity<CockpitView>, thread: ThreadKey, cx: &mut gpui::VisualTestContext) -> bool {
    view.read_with(cx, |view, cx| {
        view.cockpit
            .read(cx)
            .thread(thread)
            .is_some_and(|open| turn_running(&open))
    })
}

/// How many Decisions wait on `machine`, as the window heard them.
fn decisions_on(
    view: &Entity<CockpitView>,
    machine: MachineId,
    cx: &mut gpui::VisualTestContext,
) -> usize {
    view.read_with(cx, |view, cx| {
        view.cockpit.read(cx).notifications(machine).decisions().count()
    })
}

/// Type `text` in `thread`'s Pane and send it, as the operator does.
fn prompt(
    view: &Entity<CockpitView>,
    thread: ThreadKey,
    text: &str,
    cx: &mut gpui::VisualTestContext,
) {
    view.update(cx, |view, cx| view.focus_thread(thread, cx));
    tick(cx);
    let line = cx.debug_bounds("focused-prompt-editor").expect("the line");
    cx.simulate_click(line.center(), gpui::Modifiers::none());
    cx.simulate_input(text);
    cx.simulate_keystrokes("enter");
    tick(cx);
}

#[gpui::test]
fn escape_closes_settings_on_an_empty_remote_workspace(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, machines) = box_alone("empty-remote-escape", needs_sign_in);
    let (root, cx) = machine_window(cx, core, machines, Preferences::ephemeral());
    tick(cx);
    let view = shown(&root, cx);
    view.read_with(cx, |view, _| assert!(view.panes.is_empty()));
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| assert!(view.settings_open(cx)));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| assert!(!view.settings_open(cx)));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        view.unqueue_from_backspace(&ferrite_pane::composer::Backspace, window, cx);
    });
    view.read_with(cx, |view, _| assert!(view.panes.is_empty()));
}

/// A pin is saved as its Thread's machine's (F-A8): ids are per Store, so
/// box's Thread 1 is not this computer's. Both machines' pins list at
/// once, and unpinning one leaves the other.
#[gpui::test]
fn each_machines_pin_is_its_own(cx: &mut TestAppContext) {
    let (mut core, _fake, _) = bound_cockpit("machines-pins", Provider::Claude);
    let mine = core.known_threads().into_iter().next().unwrap();
    let number = mine.thread().get();
    let machines = beside_box("pins", &mut core, needs_sign_in);
    let theirs = core
        .resolve(&SavedThread {
            machine: on_box_key(),
            thread: mine.thread(),
        })
        .expect("box is held");
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    let pins = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let saved = serde_json::to_value(&view.prefs.settings).unwrap()["pinned_threads"].clone();
            (saved, view.pins(cx))
        })
    };
    view.update(cx, |view, cx| {
        view.set_pinned(mine, true, cx);
        view.set_pinned(theirs, true, cx);
    });
    assert_eq!(
        pins(cx),
        (
            serde_json::json!([
                { "machine": null, "thread": number },
                { "machine": "me@box", "thread": number },
            ]),
            vec![mine, theirs]
        )
    );
    view.update(cx, |view, cx| view.set_pinned(mine, false, cx));
    assert_eq!(
        pins(cx),
        (serde_json::json!([{ "machine": "me@box", "thread": number }]), vec![theirs]),
        "unpinning here leaves box's pin"
    );
}

/// box's card opens its Thread's menu on a right-click, as every card
/// does, without what only this computer's paths can do; its Pin pins
/// box's Thread, saved under box's name, and it moves from the Inbox to
/// a Pinned card wearing the pin, its ids box's own.
#[gpui::test]
fn a_machines_card_opens_its_threads_menu_and_wears_its_pin(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake, machines, _agents) = two_machines("row-menu", 1);
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    tick(cx);
    let row = debug_bounds(cx, format!("nav-thread-{}", on(on_box, theirs)))
        .expect("box's card");
    cx.simulate_mouse_down(row.center(), gpui::MouseButton::Right, gpui::Modifiers::none());
    tick(cx);
    let labels = view.read_with(cx, |view, _| {
        view.context_menu()
            .map(|menu| {
                menu.rows
                    .iter()
                    .flatten()
                    .map(|(item, _)| item.label.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    assert!(labels.iter().any(|label| label == "Pin"), "{labels:?}");
    assert!(!labels.iter().any(|label| label == "Open in"), "{labels:?}");
    let inbox = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.needs_you_rows(cx).iter().filter(|row| row.thread == theirs).count()
        })
    };
    assert_eq!(inbox(cx), 1);
    assert!(debug_bounds(cx, format!("nav-card-pin-{}", on(on_box, theirs))).is_none());
    cx.simulate_keystrokes("p");
    tick(cx);
    assert_eq!(inbox(cx), 0, "pinned, it leaves the Inbox");
    assert!(
        debug_bounds(cx, format!("nav-card-pin-{}", on(on_box, theirs))).is_some(),
        "its Pinned card wears the pin"
    );
    view.read_with(cx, |view, cx| {
        assert_eq!(view.pins(cx), [theirs]);
        assert_eq!(
            view.prefs.settings.pinned_threads,
            [SavedThread {
                machine: on_box_key(),
                thread: theirs.thread(),
            }]
        );
    });
}

/// box needs a sign-in: the nav's notice at the top of its list says
/// so in its words, and its press is the sign-in (here, a bridge with no
/// ssh refuses it, and the nav says that). No machine has a heading.
#[gpui::test]
fn a_machine_that_needs_a_sign_in_has_a_notice_that_signs_in(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("machines-listed", 1);
    let machines = beside_box("listed", &mut core, needs_sign_in);
    let on_box = machines.held(1).unwrap();
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    until(cx, |cx| connection(&view, 1, cx) == Connection::Attention);
    assert_eq!(connection(&view, 0, cx), Connection::Online);
    tick(cx);
    assert_eq!(machine_notices(&view, cx), ["Sign in to box"]);
    let notice = debug_bounds(cx, format!("nav-machine-notice-{}", on_box.get()))
        .expect("box's notice");
    assert!(debug_bounds(cx, "nav-machine-0".into()).is_none(), "no headings");
    cx.simulate_click(notice.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        let said = view.nav_notice.text().map(|text| text.to_string()).unwrap_or_default();
        assert!(said.starts_with("cannot sign in"), "the press signs in: {said:?}");
    });
    view.update(cx, |view, cx| {
        let machines = view.machines.as_ref().unwrap();
        assert!(matches!(
            machines.server_state(1, view.cockpit.read(cx)),
            Some(ferrite_client::cockpit::ServerState::Detached {
                attention: Some(Failure::SignIn),
                ..
            })
        ));
        view.cockpit.update(cx, |cockpit, _| machines.retry(1, cockpit));
        assert_eq!(
            machines.rows(view.cockpit.read(cx))[1].connection,
            Connection::Connecting,
            "Retry also releases an authentication stop"
        );
    });
}

/// A session the machine refuses says "Sign in", not "reconnecting", on
/// the rows' left edge and across their width, and its press opens
/// Connections on box to pair it again.
#[gpui::test]
fn a_revoked_session_asks_to_sign_in_and_opens_its_pairing(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{NAV_BADGE, NAV_CARD_GAP, NAV_ROW_PAD_X};
    let (mut core, _fake) = cockpit("machines-revoked", 1);
    let mine = core.threads()[0];
    fresh("revoked");
    let link = Machine {
        name: "box".into(),
        ..Machine::new("ferrite-link:00112233445566778899aabbccddeeff")
    };
    let mut machines = Machines::new(reach("revoked", revoked), &[link.clone()], None, true);
    machines.beside(&mut core);
    let on_box = machines.held(1).unwrap();
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    // The link's host is kept where the window's settings live: the
    // editor spells it from there.
    let dir = view.read_with(cx, |view, _| view.prefs.dir.clone());
    let profile = ferrite_core::machine::dir(&dir, &link.target).join("link.json");
    std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
    std::fs::write(
        &profile,
        r#"{"origin":"https://box.example.com","server_id":"00112233445566778899aabbccddeeff","credential":"box"}"#,
    )
    .unwrap();
    until(cx, |cx| connection(&view, 1, cx) == Connection::Attention);
    tick(cx);
    assert_eq!(machine_notices(&view, cx), ["Sign in to box"]);
    let notice = debug_bounds(cx, format!("nav-machine-notice-{}", on_box.get()))
        .expect("box's notice");
    let card = debug_bounds(cx, format!("nav-thread-{}", mine.thread().get())).expect("a row");
    assert_eq!((notice.left(), notice.size.width), (card.left(), card.size.width));
    let words = cx.debug_bounds("nav-notice-words").expect("its words");
    assert_eq!(
        words.left(),
        notice.left() + px(NAV_ROW_PAD_X + NAV_BADGE + NAV_CARD_GAP),
        "its words start where a row's Project name does, not centred"
    );
    cx.simulate_click(notice.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        let settings = view.sheets.read(cx).settings().expect("Settings opened");
        assert!(settings.read(cx).connections().read(cx).editing(), "on box's pairing");
    });
    let dialog = cx
        .update(|window, _| window.debug_text("connection-dialog"))
        .expect("its dialog");
    assert!(dialog.iter().any(|line| line == "Sign in to box"), "{dialog:?}");
    assert!(!dialog.iter().any(|line| line == "SSH"), "a link stays a link: {dialog:?}");
    let host = cx
        .update(|window, _| window.debug_text("connection-remote-host"))
        .unwrap_or_default()
        .join(" ");
    assert!(host.contains("https://box.example.com"), "its host, as the window keeps it: {host}");
    let row = connection_groups(&view, cx)
        .into_iter()
        .flat_map(|group| group.rows)
        .find_map(|row| match row.control {
            settings::Control::ConnectionRow(row) if row.name.as_ref() == "box" => Some(row),
            _ => None,
        })
        .expect("box's Connections row");
    assert_eq!(row.status.as_ref(), "Needs attention", "the row says it too");
    assert!(row.actions.iter().any(|action| action.id.as_ref() == "connection-retry-1" && action.label.as_ref() == "Sign in"));
}

/// Connections reads its rows from the window (`connections_inputs`):
/// each environment where the window lists it, as the window reaches it,
/// with the reason the window gives; and the window answers a press on
/// one of them.
#[gpui::test]
fn the_connections_rows_are_the_windows_environments(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("machines-reached", 1);
    fresh("reached");
    let link = Machine {
        name: "box".into(),
        ..Machine::new("ferrite-link:00112233445566778899aabbccddeeff")
    };
    let spare = Machine {
        name: "spare".into(),
        enabled: false,
        ..Machine::new("me@spare")
    };
    let mut machines = Machines::new(reach("reached", revoked), &[link, spare], None, true);
    machines.beside(&mut core);
    let on_box = machines.held(1).unwrap();
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    until(cx, |cx| connection(&view, 1, cx) == Connection::Attention);
    view.update(cx, |view, cx| view.open_connections(cx));
    tick(cx);
    let reason = view.read_with(cx, |view, cx| match view.cockpit.read(cx).server(on_box) {
        Some(ferrite_client::cockpit::ServerState::Detached { reason, .. }) => reason,
        _ => panic!("box is detached"),
    });
    let rows = |cx: &mut gpui::VisualTestContext| -> Vec<settings::ConnectionRow> {
        connection_groups(&view, cx)
            .into_iter()
            .flat_map(|group| group.rows)
            .filter_map(|row| match row.control {
                settings::Control::ConnectionRow(row) => Some(row),
                _ => None,
            })
            .collect()
    };
    let said: Vec<_> = rows(cx)
        .iter()
        .map(|row| {
            let retry = row
                .actions
                .iter()
                .find(|action| action.id.starts_with("connection-retry-"))
                .map(|action| (action.id.to_string(), action.label.to_string()));
            (row.name.to_string(), row.status.to_string(), row.detail.as_ref().map(|detail| detail.to_string()), row.connected, retry)
        })
        .collect();
    assert_eq!(
        said,
        [
            (
                "Local environment".into(),
                "Run agents on this computer. Turn off to use Ferrite only with remote environments.".into(),
                None,
                true,
                None,
            ),
            (
                "box".into(),
                "Needs attention".into(),
                Some(reason),
                false,
                Some(("connection-retry-1".into(), "Sign in".into())),
            ),
            ("spare".into(), "Disconnected".into(), None, false, None),
        ],
    );
    let open = rows(cx)[1]
        .actions
        .iter()
        .find(|action| action.id.as_ref() == "connection-open-1")
        .expect("box's Open")
        .act
        .clone();
    cx.update(|window, cx| open(window, cx));
    tick(cx);
    assert_eq!(view.read_with(cx, |view, cx| view.settings_machine(cx)), Some(on_box));
    let open = rows(cx)[1].actions.iter().find(|action| action.id.as_ref() == "connection-open-1").map(|action| action.enabled);
    assert_eq!(open, Some(false), "Settings is about box now");
}

/// Another machine's Thread draws the row this computer's does: its
/// branch, read by its Server, on a card of the same height.
#[gpui::test]
fn a_machines_row_has_its_branch_and_a_local_rows_height(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, _agents) = two_machines("branch-row", 1);
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let branch = ferrite_core::git_read::checkout_branch(&here()).expect("this checkout's branch");
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            view.nav_state(cx)
                .active
                .iter()
                .any(|row| row.key == theirs && row.branch.as_deref() == Some(branch.as_str()))
        })
    });
    tick(cx);
    let card = debug_bounds(cx, format!("nav-thread-{}", on(on_box, theirs))).expect("box's card");
    assert_eq!(card.size.height, px(ferrite_ui::theme::NAV_CARD_H));
}

/// Removing box asks first, then lets box go; this computer's Pane stays.
#[gpui::test]
fn removing_a_connection_confirms_then_lets_its_machine_go(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("machines-remove", 1);
    let machines = beside_box("remove", &mut core, needs_sign_in);
    let mut prefs = Preferences::ephemeral();
    prefs.settings.machines = vec![box_machine()];
    let (view, cx) = window_of(cx, core, machines, prefs);
    let remove = connection_act(&view, "connection-remove-1", cx);
    cx.update(|window, cx| remove(window, cx));
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.prefs.settings.machines.len(),
            1,
            "the first press only asks"
        );
    });
    let confirm = connection_removal(&view, cx);
    cx.update(|window, cx| confirm(window, cx));
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.prefs.settings.machines.is_empty());
        assert!(!view.machines.as_ref().unwrap().listed());
        assert_eq!(view.cockpit.read(cx).machines().count(), 1, "box is let go");
        assert_eq!(view.panes.len(), 1, "this computer's Pane stays");
    });
}

/// Removing box forgets where this computer's Projects were copied to
/// on it: a Machine saved again under its name is another (ADR 0048).
#[gpui::test]
fn removing_a_connection_forgets_where_copies_to_it_went(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("machines-remove-placed", 1);
    let machines = beside_box("remove-placed", &mut core, needs_sign_in);
    let mut prefs = Preferences::ephemeral();
    prefs.settings.machines = vec![box_machine()];
    let kept = ferrite_core::machine::dir(&prefs.dir, &box_machine().target);
    let copy = ferrite_core::placement::Placement {
        local_root: here(),
        remote_root: "/home/operator/projects/ferrite".into(),
    };
    ferrite_core::placement::place(&kept, copy.clone()).unwrap();
    assert_eq!(ferrite_core::placement::placements(&kept), [copy]);
    let (view, cx) = window_of(cx, core, machines, prefs);
    let remove = connection_act(&view, "connection-remove-1", cx);
    cx.update(|window, cx| remove(window, cx));
    let confirm = connection_removal(&view, cx);
    cx.update(|window, cx| confirm(window, cx));
    tick(cx);
    view.read_with(cx, |view, _| assert!(view.prefs.settings.machines.is_empty()));
    assert_eq!(ferrite_core::placement::placements(&kept), []);
}

/// Both machines' Threads are cards in the one list, box's after this
/// computer's as they arrived: box's ends its meta line with the server
/// glyph, at the card's right, and resting the pointer on it shows its
/// detail, which names box.
#[gpui::test]
fn a_machines_threads_sit_in_the_one_list_with_its_icon(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, _agents) = two_machines("one-list", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    tick(cx);
    let active = view.read_with(cx, |view, cx| {
        view.nav_state(cx)
            .active
            .iter()
            .map(|row| (row.key, row.project.clone()))
            .collect::<Vec<_>>()
    });
    let project = |thread| active.iter().find(|(key, _)| *key == thread).map(|(_, project)| project);
    assert!(project(mine).is_some() && project(theirs).is_some(), "{active:?}");
    assert!(project(theirs).unwrap().is_some(), "box's card names its Project: {active:?}");
    let card = debug_bounds(cx, format!("nav-thread-{}", on(on_box, theirs))).expect("box's card");
    let glyph = debug_bounds(cx, format!("nav-card-machine-{}", on(on_box, theirs)))
        .expect("its server glyph");
    assert!(card.contains(&glyph.center()), "{glyph:?} on {card:?}");
    assert!(glyph.left() > card.center().x, "at the line's right end");
    assert_eq!(glyph.size.width, px(ferrite_ui::theme::NAV_MACHINE_GLYPH));

    cx.simulate_mouse_move(card.center(), None, gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("nav-hover-card").is_some(), "its detail");
    let said = ferrite_nav::testing::drawn_hover_card();
    assert_eq!(said.first().map(String::as_str), Some("box"), "{said:?}");
    assert_eq!(
        said.last().map(String::as_str),
        Some("Claude"),
        "a model box has not said is not guessed"
    );
    let mut words = |selector: &'static str| cx.debug_bounds(selector).expect(selector).left();
    let project = words("nav-hover-project");
    assert_eq!(project, words("nav-hover-fact"), "one edge");
    assert_eq!(project, words("nav-hover-provider"), "one edge");

    let _asked = view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.set_model(theirs, Some("claude-haiku-4-5".into())))
    });
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            view.cockpit.read(cx).thread(theirs).is_some_and(|open| open.model().is_some())
        })
    });
    cx.simulate_mouse_move(gpui::point(px(900.), px(500.)), None, gpui::Modifiers::none());
    tick(cx);
    cx.simulate_mouse_move(card.center(), None, gpui::Modifiers::none());
    tick(cx);
    let said = ferrite_nav::testing::drawn_hover_card();
    assert_eq!(said.last().map(String::as_str), Some("Haiku 4.5"), "it ends on the model, as T3's does");
}

/// A Thread of box's the window does not mirror names the model its
/// agent announced, as its digest says it: T3's shell carries the model.
#[gpui::test]
fn an_unopened_machines_thread_names_its_model(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, agents) = two_machines("digest-model", 1);
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    agents.say(
        0,
        SessionEvent::Init {
            session_id: "box-session".into(),
            model: "claude-haiku-4-5".into(),
        },
    );
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            view.cockpit.read(cx).thread(theirs).is_some_and(|open| {
                open.digest().model.as_deref() == Some("claude-haiku-4-5")
            })
        })
    });
    view.read_with(cx, |view, cx| {
        let open = view.cockpit.read(cx).thread(theirs);
        assert_eq!(open.and_then(|open| open.transcript().model()), None, "not mirrored");
    });
    tick(cx);
    let card = debug_bounds(cx, format!("nav-thread-{}", on(on_box, theirs))).expect("box's card");
    cx.simulate_mouse_move(card.center(), None, gpui::Modifiers::none());
    tick(cx);
    let said = ferrite_nav::testing::drawn_hover_card();
    assert_eq!(said.last().map(String::as_str), Some("Haiku 4.5"), "{said:?}");
}

/// A machine's notice sits on the grid of the cards under it: its dot on
/// the edge the Project badge starts at, its words where the Project name
/// starts, in both of its states' words.
#[gpui::test]
fn a_machines_notice_sits_on_a_cards_grid(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{NAV_BADGE, NAV_CARD_GAP};
    let (mut core, _fake) = cockpit("machines-notice-grid", 1);
    let here = core.this_computer().unwrap();
    let mine = core
        .open(here, Provider::Claude, WorkspaceChoice::Main { checkout: self::here() })
        .unwrap();
    let machines = beside_box("notice-grid", &mut core, needs_sign_in);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| connection(&view, 1, cx) == Connection::Attention);
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(mine).is_some()));
    tick(cx);
    assert_eq!(machine_notices(&view, cx), ["Sign in to box"]);
    let project = debug_bounds(cx, format!("nav-card-project-{}", mine.thread().get()))
        .expect("the row's Project name");
    let words = cx.debug_bounds("nav-notice-words").expect("the notice's words");
    let dot = cx.debug_bounds("nav-notice-dot").expect("the notice's dot");
    assert_eq!(words.left(), project.left(), "its words on the Project name's edge");
    assert_eq!(
        dot.left(),
        project.left() - px(NAV_BADGE + NAV_CARD_GAP),
        "its dot on the badge's edge"
    );
}

/// This computer's rows keep their bare ids and draw no glyph, beside
/// another machine's.
#[gpui::test]
fn this_computers_rows_draw_no_icon(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, _agents) = two_machines("no-icon", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let here = core.this_computer().unwrap();
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    tick(cx);
    let card = debug_bounds(cx, format!("nav-thread-{}", mine.thread().get()))
        .expect("this computer's card, its id bare");
    cx.simulate_mouse_move(card.center(), None, gpui::Modifiers::none());
    tick(cx);
    assert!(debug_bounds(cx, format!("nav-card-machine-{}", mine.thread().get())).is_none());
    assert!(debug_bounds(cx, format!("nav-card-machine-{}", on(here, mine))).is_none());
    assert!(cx.debug_bounds("nav-hover-card").is_some(), "its detail");
    assert!(
        !ferrite_nav::testing::drawn_hover_card().iter().any(|line| line == "box"),
        "names no machine"
    );
    assert!(machine_notices(&view, cx).is_empty(), "both are there");
}

/// The same repository on two machines is two Projects, and the Filter
/// menu marks the other machine's with T3 Code's icon, its tooltip saying
/// where it is (`ProjectEnvironmentBadge`'s "On box").
#[gpui::test]
fn the_same_repository_on_two_machines_is_two_projects(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, _agents) = two_machines("same-repo", 1);
    let on_box = machines.held(1).unwrap();
    let here_machine = core.this_computer().unwrap();
    let asked = core.register_project(here_machine, &here());
    let mine = core.wait_for(asked).unwrap();
    let asked = core.register_project(on_box, &here());
    let theirs = core.wait_for(asked).unwrap();
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    let rows = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.filter_rows(cx)
                .into_iter()
                .flatten()
                .filter_map(|(item, verb)| match verb {
                    MenuVerb::FilterProject(Some(key)) if key == mine || key == theirs => {
                        Some((key, item.label.to_string(), item.detail.is_some(), item.mark))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
    };
    until(cx, |cx| rows(cx).len() == 2);
    let rows = rows(cx);
    assert_eq!(rows[0].0, mine, "this computer's first");
    assert_eq!(rows[0].1, rows[1].1, "one name: {rows:?}");
    assert!(!rows[0].2 && !rows[1].2, "neither has detail text: {rows:?}");
    assert_eq!(rows[0].3, None, "this computer's says nothing more");
    assert_eq!(
        rows[1].3,
        Some((ferrite_ui::icons::SERVER, "On box".into())),
        "{rows:?}"
    );
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    tick(cx);
    let filter = cx.debug_bounds("nav-filter").expect("the Filter");
    cx.simulate_click(filter.center(), gpui::Modifiers::none());
    tick(cx);
    let mark = cx.debug_bounds("menu-row-mark").expect("box's row wears the icon");
    assert_eq!(tooltip_at(cx, mark.center()).as_deref(), Some("On box"));
}

/// Details names the checkout a Thread works in: this computer's is the
/// `Local checkout`, a linked machine's just its `Checkout`.
#[gpui::test]
fn only_this_computers_checkout_is_local_in_details(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, _agents) = two_machines("details-checkout", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    tick(cx);
    let card = |thread, cx: &mut gpui::VisualTestContext| {
        view.update_in(cx, |view, window, cx| {
            view.focus_thread(thread, cx);
            view.toggle_details_now(window, cx);
        });
        tick(cx);
        let lines = cx
            .update(|window, _| window.debug_text("details-card"))
            .expect("the card");
        view.update_in(cx, |view, window, cx| view.toggle_details_now(window, cx));
        tick(cx);
        lines
    };
    let here = card(mine, cx);
    assert!(here.iter().any(|line| line == "Local checkout"), "{here:?}");
    let there = card(theirs, cx);
    assert!(there.iter().any(|line| line == "Checkout"), "{there:?}");
    assert!(!there.iter().any(|line| line == "Local checkout"), "{there:?}");
}

/// What the tooltip says once the pointer has rested on `at`.
pub(super) fn tooltip_at(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>) -> Option<String> {
    cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
    cx.simulate_mouse_move(at + gpui::point(px(1.), px(0.)), None, gpui::Modifiers::none());
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|window, _| window.debug_text("tooltip")).map(|lines| lines.join(" "))
}

/// With no machine reachable, Add project offers T3 Code's one row,
/// "Connect an environment", and it opens Settings on Connections.
#[gpui::test]
fn add_project_with_no_machine_reached_offers_to_connect_one(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, machines) = box_alone("add-none", needs_sign_in);
    let (root, cx) = machine_window(cx, core, machines, Preferences::ephemeral());
    let view = shown(&root, cx);
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    tick(cx);
    view.update_in(cx, |view, window, cx| view.add_project(window, cx));
    tick(cx);
    view.read_with(cx, |view, cx| {
        let palette = view.floats.palette.as_ref().expect("the pick").read(cx);
        let row = palette.selected().expect("its one row");
        assert_eq!(palette.survivors().len(), 1);
        assert_eq!(row.name.as_ref(), "Connect an environment");
        assert_eq!(row.context.as_ref(), "No connected environment can add a project");
    });
    cx.simulate_keystrokes("enter");
    tick(cx);
    let sheet = view.read_with(cx, |view, cx| view.settings_sheet(cx).expect("Settings"));
    assert_eq!(sheet.read_with(cx, |sheet, _| sheet.page()), settings::PageKey::Connections);
}

/// The machine pick goes back a step, to the palette it was opened from
/// with its line as it was, by backspace on an empty line and by its arrow.
/// Esc closes the whole palette from it (T3's `CommandPalette`).
#[gpui::test]
fn the_machine_pick_goes_back_a_step_and_esc_closes(cx: &mut TestAppContext) {
    use crate::palette::PaletteScope;
    bind_production_keys(cx);
    let (core, _fake, machines, _agents) = two_machines("pick-back", 1);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    tick(cx);
    let open = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.floats.palette.as_ref().map(|palette| {
                let palette = palette.read(cx);
                (palette.scope, palette.query(cx))
            })
        })
    };
    let pick = |cx: &mut gpui::VisualTestContext| {
        cx.simulate_keystrokes("cmd-k");
        tick(cx);
        cx.simulate_input("new project");
        tick(cx);
        cx.simulate_keystrokes("enter");
        tick(cx);
    };
    pick(cx);
    assert_eq!(open(cx), Some((PaletteScope::Machines, String::new())));
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(open(cx), None, "esc closes the palette");

    pick(cx);
    cx.simulate_keystrokes("backspace");
    tick(cx);
    assert_eq!(open(cx), Some((PaletteScope::All, "new project".into())), "backspace: back");
    cx.simulate_keystrokes("escape");
    tick(cx);

    pick(cx);
    let arrow = cx.debug_bounds("palette-back").expect("its arrow");
    cx.simulate_click(arrow.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(open(cx), Some((PaletteScope::All, "new project".into())), "the arrow: back");
}

/// The pick's row for this computer is named as T3 names it: its host,
/// described as this computer; with no host to say, the words alone.
#[test]
fn the_pick_describes_this_computer() {
    use crate::cockpit::palette::this_computer_row;
    assert_eq!(this_computer_row(Some("pc".into())), ("pc".into(), "This computer".into()));
    assert_eq!(this_computer_row(Some("  ".into())), ("This computer".into(), String::new()));
    assert_eq!(this_computer_row(None), ("This computer".into(), String::new()));
}

/// A draft on box, the one environment on, names box first in the strip
/// over its Composer, a word with nothing to choose; beside this computer
/// the environment chip stands there instead, naming box, then this
/// computer's draft `Local`.
#[gpui::test]
fn a_draft_on_a_machine_names_it(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, _agents) = two_machines("draft-machine", 1);
    let on_box = machines.held(1).unwrap();
    let here_machine = core.this_computer().unwrap();
    let asked = core.register_project(here_machine, &here());
    let mine = core.wait_for(asked).unwrap();
    let asked = core.register_project(on_box, &here());
    let theirs = core.wait_for(asked).unwrap();
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    tick(cx);
    view.update(cx, |view, cx| view.open_draft_in_project(theirs, cx));
    tick(cx);
    assert!(cx.debug_bounds("band-machine").is_none(), "the chip stands there");
    let chip = cx.debug_bounds("band-chip-2").expect("the environment chip");
    let project = cx.debug_bounds("band-chip-0").expect("its Project's chip");
    assert!(chip.right() <= project.left(), "first: {chip:?} {project:?}");
    let said = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, _| window.debug_text("band-chip-2"))
            .map(|lines| lines.join(" "))
    };
    assert_eq!(said(cx).as_deref(), Some("box"));
    view.update(cx, |view, cx| view.open_draft_in_project(mine, cx));
    tick(cx);
    assert_eq!(said(cx).as_deref(), Some("Local"));
}

/// With box the one environment on, its draft names it in a word.
#[gpui::test]
fn a_draft_on_the_one_machine_on_names_it_in_a_word(cx: &mut TestAppContext) {
    fresh("draft-machine-alone");
    let _ = ferrite_server::fixture_machine(
        settings_dir("draft-machine-alone").join("box"),
        Box::new(Agents::default()),
    )
    .unwrap();
    let mut machines = Machines::new(reach("draft-machine-alone", serving), &[box_machine()], None, false);
    let core = machines.cockpit();
    let on_box = machines.held(1).unwrap();
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    until(cx, |cx| view.read_with(cx, |view, cx| attached(view.cockpit.read(cx), on_box)));
    let theirs = view.update(cx, |view, cx| {
        view.cockpit.update(cx, |core, _| {
            let asked = core.register_project(on_box, &here());
            core.wait_for(asked).unwrap()
        })
    });
    view.update(cx, |view, cx| view.open_draft_in_project(theirs, cx));
    tick(cx);
    assert!(cx.debug_bounds("band-chip-2").is_none(), "nothing to choose");
    let said = cx.update(|window, _| window.debug_text("band-machine"));
    assert_eq!(said.map(|lines| lines.join(" ")).as_deref(), Some("box"));
}

/// A Thread on box is one row in the one list: no second listing under
/// this computer's cards or the Inbox's.
#[gpui::test]
fn a_machines_thread_is_listed_once(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, _agents) = two_machines("listed-once", 1);
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    tick(cx);
    view.read_with(cx, |view, cx| {
        let state = view.nav_state(cx);
        let listed = state
            .pinned
            .iter()
            .chain(&state.active)
            .chain(&state.working)
            .filter(|row| row.key == theirs)
            .count();
        assert_eq!(listed, 1, "one row");
        let inbox = view.inbox_rows(&state, cx);
        assert_eq!(inbox.iter().filter(|row| row.thread == theirs).count(), 1);
    });
    assert!(debug_bounds(cx, format!("nav-thread-{}", on(on_box, theirs))).is_some());
}

/// box needs a sign-in: its band offers one and the nav's notice says so,
/// while this computer, there, has no band and its Pane no cover.
#[gpui::test]
fn a_machine_that_needs_a_sign_in_says_so_beside_the_rest(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("machines-sign-in", 1);
    let machines = beside_box("sign-in", &mut core, needs_sign_in);
    let on_box = machines.held(1).unwrap();
    let (root, cx) = machine_window(cx, core, machines, Preferences::ephemeral());
    let view = shown(&root, cx);
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            matches!(
                view.cockpit.read(cx).server(on_box),
                Some(ferrite_client::cockpit::ServerState::Detached {
                    attention: Some(Failure::SignIn),
                    ..
                })
            )
        })
    });
    tick(cx);
    cx.update(|window, cx| {
        use gpui::component::WindowExt;
        assert_eq!(window.notifications(cx).len(), 1, "the sign-in band");
    });
    view.read_with(cx, |view, cx| {
        let here = view.cockpit.read(cx).this_computer().unwrap();
        assert_eq!(view.server_bands.detached(on_box), Some(Gone::SignIn));
        assert_eq!(view.server_bands.detached(here), None, "this computer is there");
        assert!(view.machine_lost(on_box, cx));
        assert!(!view.machine_lost(here, cx));
    });
    assert_eq!(connection(&view, 1, cx), Connection::Attention);
    assert_eq!(connection(&view, 0, cx), Connection::Online);
    let here = view.read_with(cx, |view, cx| view.cockpit.read(cx).this_computer().unwrap());
    assert!(
        debug_bounds(cx, format!("machine-lost-{}", here.get())).is_none(),
        "this computer's Pane takes input"
    );
}

/// A machine with no `ferrited` says so; a window holding it alone has
/// nothing to take the keyboard but its root.
#[gpui::test]
fn a_machine_without_ferrited_says_so(cx: &mut TestAppContext) {
    let (core, machines) = box_alone("missing", not_installed);
    let on_box = machines.held(1).unwrap();
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            matches!(
                view.cockpit.read(cx).server(on_box),
                Some(ferrite_client::cockpit::ServerState::Detached {
                    attention: Some(Failure::NotInstalled),
                    ..
                })
            )
        })
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.server_bands.detached(on_box), Some(Gone::NotInstalled));
    });
    assert_eq!(connection(&view, 0, cx), Connection::Attention, "Settings says so");
    tick(cx);
    assert_eq!(machine_notices(&view, cx), ["box is offline"], "T3's words");
    cx.update(|window, cx| {
        assert!(
            view.read(cx).focus.is_focused(window),
            "the root holds the keyboard"
        );
    });
}

/// Turning box off in Settings lets it go: Settings says it is off and
/// its Panes leave; this computer's stay.
#[gpui::test]
fn turning_a_machine_off_lets_only_it_go(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, _agents) = two_machines("off", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    view.update(cx, |view, cx| {
        view.prefs.settings.machines = vec![Machine {
            enabled: false,
            ..box_machine()
        }];
        view.machines_changed(cx);
    });
    tick(cx);
    assert_eq!(connection(&view, 1, cx), Connection::Disabled);
    view.read_with(cx, |view, _| {
        assert!(view.pane_for(theirs).is_none(), "box's Pane went with it");
        assert!(view.pane_for(mine).is_some(), "this computer's stays");
    });
}

/// box, a Server of its own standing in for it over a Bridge, holding a
/// Codex Thread in a checkout of its own whose `notes.txt` box wrote.
fn box_holding_notes(name: &str) -> (Cockpit, ThreadKey, std::path::PathBuf) {
    use ferrite_client::cockpit::{Attach, Endpoint, ServerState};
    let dir = settings_dir(name);
    let _ = std::fs::remove_dir_all(&dir);
    let checkout = repo_in(&dir);
    std::fs::write(checkout.join("notes.txt"), "written on box\n").unwrap();
    let dial = ferrite_server::fixture_machine(
        dir.join("box"), Box::new(OnBox(Default::default())),
    ).unwrap();
    let mut core = Cockpit::attach(Attach {
        saved_as: ferrite_core::MachineKey(Some("me@box".into())),
        browser: false,
        driven: false,
        store: dir.join("window/threads"),
        server: Endpoint::Machine(std::sync::Arc::new(Bridge::new(
            "box", dir.join("bridge/bridge.sock"), dial,
        ))),
        client: ferrite_core::protocol::Peer { name: "test".into(), version: "0".into() },
        renderer: None,
    });
    ferrite_core::doorbell::until("remote server never attached", || {
        core.pump();
        matches!(core.server(core.first_machine()), Some(ServerState::Attached { .. }))
    });
    let thread = core.open(core.first_machine(), Provider::Codex, WorkspaceChoice::Main { checkout: checkout.clone() }).unwrap();
    (core, thread, checkout)
}

/// Files and the Terminal of a Thread on box open through their keys, and
/// a file opened there shows what box read of it: the window's own
/// `Submit` carried the read to box and its answer back.
#[gpui::test]
fn remote_files_and_terminal_open_through_their_keyboard_shortcuts(cx: &mut TestAppContext) {
    use crate::cockpit::surfaces::SurfaceId;
    let (core, thread, checkout) = box_holding_notes("tool-keys");
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.elsewhere(thread.machine(), cx));
        assert!(view.opener_unavailable(thread, crate::cockpit::panel::Opener::Files, cx).is_none());
        assert!(view.opener_unavailable(thread, crate::cockpit::panel::Opener::Terminal, cx).is_none());
        assert!(view.opener_unavailable(thread, crate::cockpit::panel::Opener::Changes, cx).is_none());
    });
    cx.simulate_keystrokes("cmd-shift-f");
    tick(cx);
    view.read_with(cx, |view, _| assert_eq!(view.active_surface(thread), Some(SurfaceId::Files)));
    view.update(cx, |view, cx| view.open_in_files(thread, checkout.join("notes.txt"), None, true, cx));
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            view.files.read(cx).shown_editor(thread).is_some_and(|editor| editor.read(cx).text() == "written on box\n")
        })
    });
    cx.simulate_keystrokes("cmd-j");
    tick(cx);
    view.read_with(cx, |view, _| assert!(matches!(view.active_surface(thread), Some(SurfaceId::Terminal(_)))));
    assert!(cx.debug_bounds("terminal-failed").is_some(), "a fake SSH bridge cannot start a local shell");
}

/// A write to box queued as the window closes never reaches box, though
/// the window's connection to box outlives it.
#[gpui::test]
fn a_remote_write_queued_as_the_window_closes_never_reaches_the_machine(cx: &mut TestAppContext) {
    use ferrite_core::file_operations::Request;
    let (core, thread, checkout) = box_holding_notes("closing-write");
    let view = cx.new(|cx| CockpitView::new(core, cx));
    let (connection, bridge) = view.read_with(cx, |view, cx| {
        (view.cockpit.clone(), view.cockpit.read(cx).bridge(thread.machine()).cloned().expect("box's bridge"))
    });
    let submit = crate::cockpit::tools::remote_files::submit(view.downgrade(), thread.machine(), bridge);
    let write = Request::Write {
        root: checkout.clone(),
        path: checkout.join("notes.txt"),
        text: "written after the window closed\n".into(),
        expected: None,
    };
    // The request waits for the window's next turn; the window closes
    // before it comes.
    let mut answer = cx.update(move |cx| {
        let answer = submit(write, cx);
        drop(view);
        answer
    });
    // The connection lives on (another window holds it): it carries what
    // reaches it and hands back box's answers.
    let mut reply = None;
    ferrite_core::doorbell::until("the write never answered", || {
        connection.update(cx, |connection, _| connection.pump());
        cx.executor().advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        if let Ok(Some(answered)) = answer.try_recv() {
            reply = Some(answered);
        }
        reply.is_some()
    });
    assert!(reply.unwrap().is_err(), "the closed window's write is refused");
    assert_eq!(
        std::fs::read_to_string(checkout.join("notes.txt")).unwrap(),
        "written on box\n",
        "box never got the write"
    );
    drop(connection);
}

/// Another client (the board or another window) can close and open Threads
/// while this window is editing. Its UI mirror must follow the Server roster
/// before routing keyboard input by pane index.
#[gpui::test]
fn background_thread_changes_keep_the_visible_composer_live(cx: &mut TestAppContext) {
    use ferrite_client::cockpit::{Attach, Endpoint, ServerState};
    let dir = scratch("input-sync");
    let dial =
        ferrite_server::fixture_machine(dir.join("box"), Box::new(OnBox(Default::default())))
            .unwrap();
    let bridge = std::sync::Arc::new(Bridge::new(
        "box",
        dir.join("bridge").join("bridge.sock"),
        dial,
    ));
    let attach = |name: &str| {
        let mut core = Cockpit::attach(Attach {
            saved_as: ferrite_core::MachineKey(Some("me@box".into())),
            browser: false,
            driven: false,
            store: dir.join(name).join("threads"),
            server: Endpoint::Machine(bridge.clone()),
            client: ferrite_core::protocol::Peer {
                name: name.into(),
                version: "0".into(),
            },
            renderer: None,
        });
        ferrite_core::doorbell::until("client did not attach", || {
            core.pump();
            matches!(core.server(core.first_machine()), Some(ServerState::Attached { .. }))
        });
        core
    };
    let mut background = attach("background");
    let earlier = background
        .open(background.first_machine(), Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    let editing = background
        .open(background.first_machine(), Provider::Codex, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    background
        .open(background.first_machine(), Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    let mut core = attach("window");
    // This window holds box under an id of its own.
    let editing = core.key(editing.thread());
    core.focus_thread(editing);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    tick(cx);
    cx.simulate_input("draft");
    tick(cx);
    let composer = view.read_with(cx, |view, _| {
        view.panes[view.pane_for(editing).unwrap()].composer.clone()
    });

    let asked = background.park_thread(earlier);

    background.wait_for(asked).unwrap();
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            let cockpit = view.cockpit.read(cx);
            cockpit.thread(cockpit.key(earlier.thread())).is_none()
        })
    });
    let editor = cx
        .debug_bounds("focused-prompt-editor")
        .expect("the surviving editor is focused");
    cx.simulate_click(editor.center(), gpui::Modifiers::none());
    cx.simulate_keystrokes("end");
    cx.simulate_input(" typed");
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(" pasted".into())));
    cx.dispatch_action(Paste);
    tick(cx);
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.text().to_owned()),
        "draft typed pasted"
    );
    assert!(cx.debug_bounds("prompt-placeholder").is_none());

    let opened = background
        .open(background.first_machine(), Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    let opened = view.read_with(cx, |view, cx| view.cockpit.read(cx).key(opened.thread()));
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.cockpit.read(cx).thread(opened).is_some())
    });
    view.read_with(cx, |view, cx| {
        assert!(
            view.pane_for(opened).is_some(),
            "a background open gets a UI Pane too"
        );
        assert_eq!(view.focused_thread(cx), Some(editing));
        assert_eq!(
            view.panes[view.focused(cx)].composer,
            composer,
            "the existing editor survives roster changes"
        );
        assert_eq!(composer.read(cx).text(), "draft typed pasted");
    });
}

/// The window holds both machines at once: box's Thread is a card in the
/// one list, and a press on it opens it in place, beside this computer's
/// Thread, its turn streamed whole now it is drawn (F-A2). The window goes
/// nowhere.
#[gpui::test]
fn a_machines_thread_opens_in_place_beside_a_local_one(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, agents) = two_machines("in-place", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    core.focus_thread(mine);
    let (root, cx) = machine_window(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    let view = shown(&root, cx);
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    tick(cx);
    assert!(
        debug_bounds(cx, format!("nav-thread-{}", mine.thread().get())).is_some(),
        "this computer's Thread, a card"
    );
    let row = debug_bounds(cx, format!("nav-thread-{}", on(on_box, theirs)))
        .expect("box's Thread, a card");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(cx), Some(theirs), "opened in place");
        assert!(view.pane_for(mine).is_some(), "beside this computer's Thread");
    });
    assert_eq!(shown(&root, cx), view, "the window goes nowhere");

    agents.say(0, SessionEvent::TextDelta { text: "on it\n\n".into() });
    until(cx, |cx| shows(&view, theirs, "on it", cx));
}

/// This computer has no Project and box has one. box's Thread opening puts
/// nobody in it, so ⌘N opens the New project card on this computer, as it
/// does with no other machine: no agent starts on a machine the operator
/// did not pick. Once they go to box's Thread, ⌘N starts in its Project.
#[gpui::test]
fn a_new_thread_with_no_project_here_stays_on_this_computer(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, _fake, machines, _agents) = two_machines("no-project-here", 0);
    let on_box = machines.held(1).unwrap();
    let asked = core.register_project(on_box, &here());
    let boxes = core.wait_for(asked).unwrap();
    let theirs = open_on(&mut core, on_box);
    assert_eq!(core.project_of(theirs), Some(boxes));
    assert_eq!(startup_project(&core), None, "box's work is no default here");
    let this_computer = core.this_computer().unwrap();
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    // A test window seeds this computer a Project; this one has none.
    view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| {
            let registry = cockpit.registry(this_computer);
            let seeded: Vec<ProjectKey> = registry
                .projects()
                .iter()
                .map(|project| registry.project_key(project.id))
                .collect();
            for project in seeded {
                let removed = cockpit.remove_project(project);
                cockpit.wait_for(removed).unwrap();
            }
        });
        view.launch_project = None;
        view.hide_sheet(cx);
    });
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(cx), None, "nobody went to box's Thread");
        assert!(view.empty_board_shown(cx));
    });

    cx.simulate_keystrokes("cmd-n");
    tick(cx);
    view.read_with(cx, |view, cx| {
        let card = view.project_card(cx).expect("the New project card");
        assert_eq!(card.read(cx).machine, this_computer);
        assert!(view.panes.iter().all(|pane| pane.draft().is_none()), "no draft on box");
    });

    view.update(cx, |view, cx| {
        view.hide_sheet(cx);
        view.focus_thread(theirs, cx);
    });
    tick(cx);
    cx.simulate_keystrokes("cmd-n");
    tick(cx);
    view.read_with(cx, |view, _| {
        let draft = view.panes.iter().find_map(|pane| pane.draft()).expect("a draft");
        assert_eq!(draft.binding.project(), boxes, "in the Thread the operator went to");
    });
}

/// A Delegate's row and its child's pill name a Thread by its Server: the
/// press opens it in place on the machine that answers as that Server, and
/// says so when that machine no longer has it.
#[gpui::test]
fn a_delegates_thread_opens_in_place_on_its_machine(cx: &mut TestAppContext) {
    use ferrite_client::cockpit::ServerState;
    bind_production_keys(cx);
    let (mut core, _fake, machines, _agents) = two_machines("delegate-in-place", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let child = open_on(&mut core, on_box);
    core.focus_thread(mine);
    let Some(ServerState::Attached { id: Some(server), .. }) = core.server(on_box) else {
        panic!("box answers as a Server");
    };
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(child).is_some()));

    view.update(cx, |view, cx| view.open_elsewhere(&server, child.thread(), cx));
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(cx), Some(child), "opened on box, in place");
        assert!(view.pane_for(mine).is_some(), "this computer's Thread stays");
        assert_eq!(view.serving(&server, cx).and_then(|serving| serving.name), Some("box".into()));
    });

    view.update(cx, |view, cx| {
        view.focus_thread(mine, cx);
        let deleted = view.cockpit.update(cx, |cockpit, _| cockpit.delete(child));
        view.cockpit.update(cx, |cockpit, _| cockpit.wait_for(deleted)).unwrap();
    });
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.cockpit.read(cx).holds(child) == Some(false))
    });
    view.update(cx, |view, cx| view.open_delegator(&server, child.thread(), cx));
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(cx), Some(mine), "a deleted Thread opens nothing");
        assert_eq!(
            view.nav_notice.text().map(|text| text.to_string()),
            Some("The parent thread was deleted".into())
        );
    });
}

/// Every act goes to its Thread's machine: a prompt typed in box's Pane
/// reaches box's agent and one typed in this computer's reaches its own;
/// ⎋ stops box's turn alone; ⌘W parks box's Thread on box, and this
/// computer's stays open.
#[gpui::test]
fn prompt_stop_and_park_reach_the_threads_machine(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, fake, machines, agents) = two_machines("acts", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));

    prompt(&view, theirs, "hello box", cx);
    until(cx, |cx| running(&view, theirs, cx));
    assert!(agents.sent("hello box"), "box's agent has it");
    prompt(&view, mine, "hello here", cx);
    assert!(fake.sent.borrow().iter().any(|sent| sent.contains("hello here")));
    assert!(
        !fake.sent.borrow().iter().any(|sent| sent.contains("hello box")),
        "box's prompt stayed on box"
    );
    assert!(!agents.sent("hello here"), "and this computer's here");
    tick(cx);
    assert!(running(&view, theirs, cx), "off screen, box's turn still reads as running (F-A2)");

    view.update(cx, |view, cx| view.focus_thread(theirs, cx));
    tick(cx);
    cx.simulate_keystrokes("escape");
    // box's agent hears the stop a beat after the window says it.
    until(cx, |_| agents.interrupts.load(Ordering::SeqCst) == 1);
    until(cx, |cx| !running(&view, theirs, cx));
    assert_eq!(agents.interrupts.load(Ordering::SeqCst), 1, "box's turn stopped once");
    assert_eq!(*fake.interrupts.borrow(), 0, "this computer's runs on");

    cx.simulate_keystrokes("cmd-w");
    // box lists its parked Threads a beat after it closes one.
    until(cx, |cx| view.read_with(cx, |view, _| view.facts.parked().contains(&theirs)));
    view.read_with(cx, |view, cx| {
        assert!(view.cockpit.read(cx).thread(theirs).is_none(), "parked on box");
        assert!(view.cockpit.read(cx).thread(mine).is_some(), "this computer's stays open");
        assert!(view.pane_for(mine).is_some());
    });
}

/// box's Server stops: its Pane stays, dimmed under the lost-link cover,
/// its card stays, dimmed, the nav says box is reconnecting once that has
/// taken T3's grace, and nothing else waits on it — this computer's Pane
/// takes a prompt as before. Back, box's Pane is live again.
#[gpui::test]
fn a_lost_machine_holds_only_its_own_panes(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (mut core, fake, machines, _agents) = two_machines("lost", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    view.update(cx, |view, cx| view.focus_thread(theirs, cx));
    tick(cx);
    let cover = format!("machine-lost-{}", on_box.get());
    assert!(debug_bounds(cx, cover.clone()).is_none(), "box is there");

    box_down(true);
    until(cx, |cx| view.read_with(cx, |view, cx| view.machine_lost(on_box, cx)));
    tick(cx);
    assert!(debug_bounds(cx, cover.clone()).is_some(), "box's Pane, covered");
    view.read_with(cx, |view, cx| {
        assert!(view.pane_for(theirs).is_some(), "box's Pane stays");
        assert!(!view.machine_lost(mine.machine(), cx), "this computer is there");
    });
    assert_ne!(connection(&view, 1, cx), Connection::Online, "Settings says so");
    assert_eq!(connection(&view, 0, cx), Connection::Online);
    let card = |thread| ferrite_nav::testing::drawn_card(thread).expect("its card").opacity;
    assert!(card(theirs) <= ferrite_ui::theme::MACHINE_LOST_OPACITY, "box's card, dimmed");
    assert_eq!(card(mine), 1.0, "this computer's, not");
    assert!(machine_notices(&view, cx).is_empty(), "nothing said within T3's grace");
    cx.executor().advance_clock(ferrite_nav::view::RECONNECT_GRACE);
    tick(cx);
    assert_eq!(machine_notices(&view, cx), ["box is reconnecting"]);
    prompt(&view, mine, "still here", cx);
    assert!(
        fake.sent.borrow().iter().any(|sent| sent.contains("still here")),
        "this computer takes a prompt as before"
    );

    box_down(false);
    view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.retry_server(on_box));
        cx.notify();
    });
    until(cx, |cx| view.read_with(cx, |view, cx| !view.machine_lost(on_box, cx)));
    view.update(cx, |view, cx| view.focus_thread(theirs, cx));
    tick(cx);
    assert!(debug_bounds(cx, cover).is_none(), "back, box's Pane is live");
    view.read_with(cx, |view, _| assert!(view.pane_for(theirs).is_some()));
}

/// files.json as `(machine, thread, file names)`, sorted.
fn saved_files(dir: &std::path::Path) -> Vec<(Option<String>, u64, Vec<String>)> {
    let files: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("files.json")).unwrap()).unwrap();
    let name = |file: &serde_json::Value| {
        let path = std::path::Path::new(file["path"].as_str().unwrap());
        path.file_name().unwrap().to_string_lossy().into_owned()
    };
    files["threads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|set| {
            let names = set["files"].as_array().unwrap().iter().map(name).collect();
            (set["machine"].as_str().map(str::to_string), set["thread"].as_u64().unwrap(), names)
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// files.json in `dir` holding one file for each of `sets`, as
/// `(machine, thread, file name)`: what earlier visits left.
fn seed_saved_files(dir: &std::path::Path, sets: &[(Option<&str>, u64, &str)]) {
    let threads: Vec<_> = sets
        .iter()
        .map(|(machine, thread, file)| {
            serde_json::json!({
                "machine": machine,
                "thread": thread,
                "files": [{ "path": dir.join(file), "preview": false }],
                "active": 0,
                "settings": { "wrap": true, "auto_save": true, "tab_size": 4 },
                "preview_tabs": true,
                "show_tree": true,
            })
        })
        .collect();
    let files = serde_json::json!({ "version": 2, "threads": threads });
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("files.json"), serde_json::to_vec(&files).unwrap()).unwrap();
}

/// drafts.json as `(machine, thread, text)`, sorted.
fn saved_drafts(dir: &std::path::Path) -> Vec<(Option<String>, u64, String)> {
    let drafts: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("drafts.json")).unwrap()).unwrap();
    let mut saved: Vec<_> = drafts["threads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|draft| {
            let thread = &draft["thread"];
            (
                thread["machine"].as_str().map(str::to_string),
                thread["thread"].as_u64().unwrap(),
                draft["content"]["text"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    saved.sort();
    saved
}

/// drafts.json in `dir` holding `drafts`, as `(machine, thread, text)`:
/// what earlier visits left.
fn seed_saved_drafts(dir: &std::path::Path, drafts: &[(Option<&str>, u64, &str)]) {
    let mut saved = ferrite_core::drafts::Drafts::default();
    for (machine, thread, text) in drafts {
        let thread = ferrite_core::SavedThread {
            machine: ferrite_core::MachineKey(machine.map(str::to_string)),
            thread: ThreadId::new(*thread),
        };
        saved.keep_thread(thread, ferrite_core::draft_content::DraftContent::from_prompt(text.to_string()));
    }
    saved.save(dir).unwrap();
}

/// `thread`'s Composer takes `text`, as typing would leave it.
fn draft_in(
    view: &Entity<CockpitView>,
    thread: ThreadKey,
    text: &str,
    cx: &mut gpui::VisualTestContext,
) {
    view.update(cx, |view, cx| {
        let composer = view.panes[view.pane_for(thread).unwrap()].composer.clone();
        let content = ferrite_core::draft_content::DraftContent::from_prompt(text.to_string());
        composer.update(cx, |composer, cx| composer.load(content, cx));
    });
}

/// What `thread`'s Composer holds.
fn draft_of(view: &Entity<CockpitView>, thread: ThreadKey, cx: &mut gpui::VisualTestContext) -> String {
    view.read_with(cx, |view, cx| {
        let composer = &view.panes[view.pane_for(thread).unwrap()].composer;
        composer.read(cx).text().to_string()
    })
}

/// F-A5: each machine's Thread list prunes only its own saved state, and
/// only once its Server has said which Threads it has. This computer has
/// answered: the Files of a Thread it no longer has go. box has not: its
/// Files, drafts and pin stay, its Thread of this computer's number
/// among them.
#[gpui::test]
fn an_unanswered_machine_loses_none_of_its_saved_state(cx: &mut TestAppContext) {
    let (mut core, _fake, checkout) = bound_cockpit("machines-unanswered", Provider::Claude);
    let mine = core.known_threads().into_iter().next().unwrap();
    let number = mine.thread().get();
    let dir = scratch("machines-unanswered-prefs");
    seed_saved_files(
        &dir,
        &[
            (None, 99, "gone.rs"),
            (Some("me@box"), number, "box-same.rs"),
            (Some("me@box"), 7, "box-7.rs"),
        ],
    );
    seed_saved_drafts(&dir, &[(Some("me@box"), number, "there"), (Some("me@box"), 7, "box only")]);
    let machines = beside_box("unanswered", &mut core, needs_sign_in);
    let mut prefs = Preferences::ephemeral();
    prefs.dir = dir.clone();
    prefs.settings.pinned_threads = vec![SavedThread {
        machine: on_box_key(),
        thread: ThreadId::new(7),
    }];
    let (view, cx) = window_of(cx, core, machines, prefs);
    view.update(cx, |view, cx| {
        view.open_in_files(mine, checkout.join("src/lib.rs"), None, true, cx);
        view.flush_panel_persistence(cx);
        view.save_drafts(cx);
    });
    let named = |machine: Option<&str>, thread: u64, file: &str| {
        (machine.map(str::to_string), thread, vec![file.to_string()])
    };
    assert_eq!(
        saved_files(&dir),
        [
            named(None, number, "lib.rs"),
            named(Some("me@box"), number, "box-same.rs"),
            named(Some("me@box"), 7, "box-7.rs"),
        ],
        "this computer's Thread 99 went; box's sets wait for box"
    );
    assert_eq!(
        saved_drafts(&dir),
        [
            (Some("me@box".to_string()), number, "there".to_string()),
            (Some("me@box".to_string()), 7, "box only".to_string()),
        ]
    );
    view.read_with(cx, |view, cx| assert_eq!(view.pins(cx).len(), 1, "box's pin stays"));
}

/// F-A8: saved state names each Thread by its machine and is read for
/// every machine. A window started again on the same settings opens both
/// machines' Threads, each with its own draft, and lists both pins, though
/// the two Threads share a number.
#[gpui::test]
fn a_restart_restores_each_machines_pins_drafts_and_panes(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, _agents) = two_machines("restart", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    assert_eq!(theirs.thread(), mine.thread(), "ids are per Store");
    let dir = scratch("machines-restart-prefs");
    let mut prefs = Preferences::ephemeral();
    prefs.dir = dir.clone();
    let (view, cx) = window_of(cx, core, machines, prefs);
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    draft_in(&view, mine, "here", cx);
    draft_in(&view, theirs, "there", cx);
    view.update(cx, |view, cx| {
        view.set_pinned(mine, true, cx);
        view.set_pinned(theirs, true, cx);
        view.save_drafts(cx);
    });
    let settings = view.read_with(cx, |view, _| view.prefs.settings.clone());
    // The window lets box go, as closing it does, so the next run's
    // bridge can take box's socket.
    view.update(cx, |view, cx| {
        view.prefs.settings.machines.clear();
        view.machines_changed(cx);
    });
    cx.update(|window, _| window.remove_window());
    drop(view);
    cx.run_until_parked();

    // Started again: this computer's Store afresh, whose first Thread is
    // its Thread 1 too, and box's Server as it was.
    let (mut core, _fake) = cockpit("machines-restart-again", 1);
    let mine = core.threads()[0];
    let mut machines = Machines::new(reach("restart", serving), &[box_machine()], None, true);
    machines.beside(&mut core);
    let on_box = machines.held(1).unwrap();
    ferrite_core::doorbell::until("box never attached again", || {
        core.pump();
        attached(&core, on_box)
    });
    let theirs = core
        .resolve(&SavedThread {
            machine: on_box_key(),
            thread: theirs.thread(),
        })
        .unwrap();
    let prefs = Preferences {
        settings,
        dir,
        ..Preferences::ephemeral()
    };
    let (view, cx) = window_of(cx, core, machines, prefs);
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    view.read_with(cx, |view, cx| {
        assert_eq!(view.pins(cx), [mine, theirs], "both machines' pins");
        assert!(view.pane_for(mine).is_some(), "this computer's Thread is open");
    });
    assert_eq!(draft_of(&view, mine, cx), "here");
    assert_eq!(draft_of(&view, theirs, cx), "there", "box's draft is box's");
}

/// A machine's parked Threads are its Server's to list: its logs stay on
/// it. They show in the nav's Parked section, called what the machine's
/// log calls them, and a click revives one there.
#[gpui::test]
fn a_machines_parked_threads_show_in_the_nav_and_revive_on_click(cx: &mut TestAppContext) {
    use ferrite_client::cockpit::{Attach, Endpoint, ServerState};
    let dir = scratch("machine-parked");
    let dial =
        ferrite_server::fixture_machine(dir.join("box"), Box::new(OnBox(Default::default())))
            .unwrap();
    let mut core = Cockpit::attach(Attach {
            saved_as: ferrite_core::MachineKey(Some("me@box".into())),
            browser: false,
            driven: false,
        store: dir.join("window").join("threads"),
        server: Endpoint::Machine(std::sync::Arc::new(Bridge::new(
            "box",
            dir.join("bridge").join("bridge.sock"),
            dial,
        ))),
        client: ferrite_core::protocol::Peer {
            name: "test".into(),
            version: "0".into(),
        },
        renderer: None,
    });
    ferrite_core::doorbell::until("never attached", || {
        core.pump();
        matches!(core.server(core.first_machine()), Some(ServerState::Attached { .. }))
    });
    let named = core
        .open(core.first_machine(), Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    let other = core
        .open(core.first_machine(), Provider::Codex, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    let renamed = core.rename_thread(named, "Release notes");
    core.wait_for(renamed).unwrap();
    let asked = core.park(named);
    core.wait_for(asked).unwrap();
    let asked = core.park(other);
    core.wait_for(asked).unwrap();

    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    until(cx, |cx| {
        view.read_with(cx, |view, _| view.facts.parked().len() == 2)
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.facts.parked().to_vec(), [named, other], "in creation order");
        assert_eq!(view.facts.name(named).as_ref(), "Release notes");
        assert!(view.panes.is_empty(), "nothing open on the machine");
    });
    assert!(
        !dir.join("window").join("threads").exists(),
        "the window keeps no Store for the machine"
    );

    let door = cx
        .debug_bounds("nav-shelf-parked")
        .expect("the Parked shelf's header");
    cx.simulate_click(door.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let row_id: &'static str = format!("nav-thread-{}", named.thread().get()).leak();
    let row = cx
        .debug_bounds(row_id)
        .expect("shown, the machine's parked Thread has a row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    until(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.panes.len() == 1 && view.facts.parked().to_vec() == [other]
        })
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.panes[0].thread(),
            Some(named),
            "revived on the machine"
        );
    });
}

/// The handle of box's Decision `serial`.
fn box_handle(serial: u64) -> ferrite_core::protocol::WireHandle {
    ferrite_core::protocol::WireHandle {
        generation: 1,
        serial,
        request_id: format!("r{serial}"),
    }
}

/// box's request Notice for Decision `serial` on `thread`, unread.
pub(super) fn box_request(thread: ThreadId, serial: u64) -> ferrite_core::protocol::WireNotice {
    ferrite_core::protocol::WireNotice::Request {
        thread,
        handle: box_handle(serial),
        subject: Some(ferrite_core::protocol::WireSubject::Main),
        question: false,
        read: false,
        at_ms: 0,
        seq: serial,
        dismissed: false,
    }
}

/// ADR 0026 with ADR 0017: a Decision on box toasts and chimes once,
/// named by its machine, is in the Inbox, and with the window in the
/// background posts a system notification; what box held when the window
/// first heard it is no news. Clicking the notification opens box's Thread
/// in place, beside this computer's.
#[gpui::test]
fn a_machines_decision_rings_once_and_opens_in_place(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, agents) = two_machines("alert", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let renamed = core.rename_thread(theirs, "Release");
    core.wait_for(renamed).unwrap();
    core.focus_thread(mine);
    agents.say(0, decision("first"));
    ferrite_core::doorbell::until("box's first Decision never came", || {
        core.pump();
        core.notifications(on_box).decisions().count() == 1
    });
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (root, cx) = machine_window(cx, core, machines, Preferences::ephemeral());
    let view = shown(&root, cx);
    ring(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.toasts.read(cx).needs_you().toasts().is_empty(), "held when first heard");
        assert!(view.chimed.is_empty());
    });

    cx.deactivate_window();
    agents.say(0, decision("second"));
    until(cx, |cx| decisions_on(&view, on_box, cx) == 2);
    ring(cx);
    ring(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.chimed, [Sound::NeedsYou], "once");
        let toasts = view.toasts.read(cx).needs_you().toasts();
        assert_eq!(toasts.len(), 1);
        assert_eq!(toasts[0].alert.thread, theirs);
        let row = view
            .alert_row(&toasts[0].alert, ferrite_core::clock::system_time(), cx)
            .unwrap();
        assert_eq!(row.title.as_ref(), "box \u{203a} Release", "named by its machine");
        let queued = view
            .needs_you_rows(cx)
            .into_iter()
            .filter(|row| row.thread == theirs)
            .count();
        assert_eq!(queued, 1, "the Inbox reads box: one row per machine and Thread");
    });
    let tag = system_tag(&on_box_key(), theirs.thread());
    assert_eq!(
        cx.delivered_system_notifications()
            .iter()
            .map(|notification| notification.tag.to_string())
            .collect::<Vec<_>>(),
        std::slice::from_ref(&tag)
    );

    cx.simulate_system_notification_response(gpui::SystemNotificationResponse {
        tag: tag.into(),
        action_id: None,
    });
    tick(cx);
    assert_eq!(shown(&root, cx), view, "the window goes nowhere");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(cx), Some(theirs), "opened in place");
        assert!(view.pane_for(mine).is_some(), "beside this computer's Thread");
    });
}

/// A toast's open about box opens box's Thread in place, as a click on
/// its notification does. Claude muted, a Decision on box toasts without
/// a chime.
#[gpui::test]
fn a_toast_press_opens_its_machines_thread_in_place(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, agents) = two_machines("toast", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    core.focus_thread(mine);
    let mut prefs = Preferences::ephemeral();
    prefs.settings.set_provider_sounds(Provider::Claude, false);
    let (view, cx) = window_of(cx, core, machines, prefs);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    ring(cx);
    // Out of sight: the operator is elsewhere.
    cx.deactivate_window();
    agents.say(0, decision("deploy"));
    until(cx, |cx| decisions_on(&view, on_box, cx) == 1);
    ring(cx);
    ring(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.toasts.read(cx).needs_you().toasts().len(), 1, "it toasts");
        assert!(view.chimed.is_empty(), "Claude is muted");
    });

    let button = debug_bounds(cx, format!("toast-open-{}", on(on_box, theirs))).expect("`⏎ open`");
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(cx), Some(theirs), "opened in place");
        assert!(view.pane_for(mine).is_some());
    });
}

/// box's Threads are in the Inbox from what box says of them, drawn or
/// not (F-A2): idle, it is a row; at work it is not; done, it is again;
/// and another client's Park takes it out.
#[gpui::test]
fn remote_inbox_follows_its_status_and_another_clients_park(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, agents) = two_machines("remote-inbox", 1);
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    let rows = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.needs_you_rows(cx)
                .into_iter()
                .filter(|row| row.thread == theirs)
                .count()
        })
    };
    until(cx, |cx| rows(cx) == 1);
    view.update(cx, |view, cx| {
        let _ = view
            .cockpit
            .update(cx, |cockpit, _| cockpit.send(theirs, "go".into()));
    });
    agents.say(0, SessionEvent::TextDelta { text: "working\n\n".into() });
    until(cx, |cx| running(&view, theirs, cx));
    assert_eq!(rows(cx), 0, "at work, it needs nothing");
    agents.say(
        0,
        SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        },
    );
    until(cx, |cx| rows(cx) == 1);

    let mut other = another_client("remote-inbox");
    let there = other
        .resolve(&SavedThread {
            machine: on_box_key(),
            thread: theirs.thread(),
        })
        .unwrap();
    let asked = other.park(there);
    other.wait_for(asked).unwrap();
    until(cx, |cx| rows(cx) == 0);
    view.read_with(cx, |view, cx| {
        assert!(view.cockpit.read(cx).thread(theirs).is_none(), "another client's Park");
    });
}

/// On a machine the Project card has no folder dialog, which could only
/// browse this computer: a browser of the machine's folders, which its
/// Server lists (`list_dir`), its home from the start. What is typed
/// narrows its rows, Tab goes into the first, ↵ adds it and the next ↵
/// confirms. The Project it makes is the machine's; a draft chooses it,
/// its workspace chip names and offers what git on the machine lists
/// (`list_repo`) — no git here is asked — and its first send starts the
/// Thread in that directory on the machine.
#[gpui::test]
fn a_draft_on_a_machine_starts_a_thread_in_a_directory_typed_there(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::frame().bind_keys(
            ferrite_ui::keymap::bindings(ferrite_ui::keymap::PLATFORM),
            cx,
        )
    });
    fresh("typed");
    let folder = settings_dir("typed").join("src");
    let repo = repo_with_worktree_and_branches(&folder);
    std::fs::create_dir_all(folder.join(".re-hidden")).unwrap();
    std::fs::write(folder.join("readme.txt"), "").unwrap();
    let started = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    // `serving` dials it itself, to keep what the window sends.
    let _ = ferrite_server::fixture_machine(
        settings_dir("typed").join("box"),
        Box::new(OnBox(started.clone())),
    )
    .unwrap();
    let mut machines = Machines::new(reach("typed", serving), &[box_machine()], None, false);
    let core = machines.cockpit();
    let on_box = machines.held(1).unwrap();
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    until(cx, |cx| view.read_with(cx, |view, cx| attached(view.cockpit.read(cx), on_box)));

    let field = view.update(cx, |view, cx| {
        view.open_project_creator_on(on_box, cx);
        let card = view.project_card(cx).unwrap();
        let browser = card.read(cx).remote.as_ref();
        browser.expect("a folder browser on a machine").read(cx).field.clone()
    });
    assert!(
        String::from_utf8_lossy(&SENT_TO_BOX.lock().unwrap())
            .contains(r#""op":"list_dir","path":"~/""#),
        "the card lists the machine's home as it opens"
    );
    cx.update(|window, cx| window.focus(&field.read(cx).focus_handle(cx), cx));
    field.update(cx, |field, cx| {
        field.set(format!("{}/re", folder.display()), cx)
    });
    until(cx, |cx| debug_bounds(cx, "folder-row:repo".into()).is_some());
    for hidden in ["folder-row:.re-hidden", "folder-row:readme.txt"] {
        assert!(debug_bounds(cx, hidden.into()).is_none(), "{hidden}");
    }
    assert!(
        debug_bounds(cx, "project-add-directory".into()).is_none(),
        "no folder dialog for a machine"
    );
    assert!(
        String::from_utf8_lossy(&SENT_TO_BOX.lock().unwrap()).contains(r#""op":"list_dir""#),
        "the machine's Server listed it"
    );

    cx.simulate_keystrokes("tab");
    assert_eq!(
        field.read_with(cx, |field, _| field.text().to_string()),
        format!("{}/repo/", folder.display())
    );
    cx.simulate_keystrokes("enter");
    view.update(cx, |view, cx| {
        assert_eq!(
            view.project_card(cx).unwrap().read(cx).staged,
            std::slice::from_ref(&repo)
        );
        assert_eq!(
            field.read(cx).text(),
            format!("{}/", folder.display()),
            "back in the folder it is in"
        );
    });
    cx.simulate_keystrokes("enter");
    // The machine's Server answers the commit a beat after the press.
    until(cx, |cx| view.read_with(cx, |view, cx| view.project_card(cx).is_none()));
    let project = view.update(cx, |view, cx| {
        assert!(
            view.project_card(cx).is_none(),
            "the next ↵ confirms the card, adding nothing more"
        );
        let registry = view.cockpit.read(cx).registry(on_box);
        let project = registry.projects().last().unwrap();
        assert_eq!(project.title, "repo");
        assert_eq!(project.root, ferrite_core::git_read::canonical(&repo));
        registry.project_key(project.id)
    });

    let composer = view.update(cx, |view, cx| {
        if view.panes.is_empty() {
            view.open_draft(DraftTarget::Main, cx);
        }
        let draft = view.panes[view.focused(cx)].draft().unwrap();
        let offered = view
            .project_menu_rows(draft, cx)
            .into_iter()
            .any(|row| row.consequence == Consequence::Band(BandChoice::Project(project)));
        assert!(offered, "the machine's Project is on the draft's chip");
        let composer = view.panes[view.focused(cx)].composer.clone();
        view.pick_band(&BandChoice::Project(project), &composer, cx);
        composer
    });
    until(cx, |cx| {
        view.read_with(cx, |view, _| {
            view.checkouts.branch(project).as_deref() == Some("trunk")
        })
    });
    view.update(cx, |view, cx| {
        assert!(
            !view.checkouts.asking_any(),
            "no git here was asked what the machine's checkout is on"
        );
        view.open_band_menu(pane::BandChip::Workspace, false, cx);
    });
    let rows = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.band_menu().map_or_else(Vec::new, |popover| {
                popover
                    .rows
                    .iter()
                    .map(|row| (row.name.to_string(), row.detail.to_string()))
                    .collect::<Vec<_>>()
            })
        })
    };
    until(cx, |cx| rows(cx).len() > 3);
    let named = |name: &str, detail: &str| (name.to_string(), detail.to_string());
    assert_eq!(
        rows(cx),
        [
            named("Current checkout", "trunk"),
            named("Previous worktree (feature/wt)", "wt-feature"),
            named("main", "existing branch"),
            named("topic/spare", "existing branch"),
            named("New branch", "in the project checkout"),
            named("New worktree", "in an isolated checkout"),
        ],
        "git on the machine lists the repo, its own worktrees and branches"
    );
    assert!(
        String::from_utf8_lossy(&SENT_TO_BOX.lock().unwrap()).contains(r#""op":"list_repo""#),
        "the machine's Server read the repo"
    );

    view.update(cx, |view, cx| {
        view.close_popup(cx);
        composer.update(cx, |composer, cx| composer.set("hello box".into(), cx));
        view.bootstrap_draft(cx);
    });
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.cockpit.read(cx).threads().len() == 1)
    });
    assert_eq!(
        *started.lock().unwrap(),
        [ferrite_core::git_read::canonical(&repo)]
    );
}

/// A review beside a machine's Thread reads its checkout through the
/// machine's Server (`review_read`, `list_commits`, ADR 0021) — no git here
/// is asked — and its lines take comments as a local review's do: the
/// uncommitted work, then one of the branch's commits picked from the list.
#[gpui::test]
fn a_review_of_a_machines_thread_reads_its_checkout_there(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::frame().bind_keys(
            ferrite_ui::keymap::bindings(ferrite_ui::keymap::PLATFORM),
            cx,
        )
    });
    fresh("reviewed");
    let repo = repo_in(&settings_dir("reviewed").join("src"));
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&["branch", "-M", "main"]);
    git(&["checkout", "-q", "-b", "dev"]);
    std::fs::write(repo.join("nav.rs"), "fn row() {}\n").unwrap();
    git(&["add", "nav.rs"]);
    git(&["commit", "-q", "-m", "Tidy the nav"]);
    std::fs::write(repo.join("notes.md"), "fresh\n").unwrap();
    let _ = ferrite_server::fixture_machine(
        settings_dir("reviewed").join("box"),
        Box::new(OnBox(Default::default())),
    )
    .unwrap();
    let mut machines = Machines::new(reach("reviewed", serving), &[box_machine()], None, false);
    let core = machines.cockpit();
    let on_box = machines.held(1).unwrap();
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, cx| attached(view.cockpit.read(cx), on_box)));
    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.open(
                on_box,
                Provider::Claude,
                WorkspaceChoice::Main {
                    checkout: repo.clone(),
                },
            ))
            .unwrap();
        view.sync_panes(cx);
    });
    until(cx, |cx| {
        view.read_with(cx, |view, _| !view.panes.is_empty())
    });
    let open_review = |scope, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_beside(beside::Beside::Review(scope), window, cx)
            })
        });
    };
    let comments = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let composer = view.panes[0].composer.read(cx);
            composer
                .review_comments()
                .iter()
                .map(|comment| (comment.location(), comment.text.clone()))
                .collect::<Vec<_>>()
        })
    };

    open_review(ferrite_reviewr::model::Scope::Uncommitted, cx);
    until(cx, |cx| cx.debug_bounds("review-lines").is_some());
    assert!(
        String::from_utf8_lossy(&SENT_TO_BOX.lock().unwrap()).contains(r#""op":"review_read""#),
        "the machine's Server read the checkout"
    );
    cx.simulate_keystrokes("c");
    cx.simulate_input("why notes?");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(comments(cx), [("notes.md:1".into(), "why notes?".into())]);

    open_review(ferrite_reviewr::model::Scope::Commits, cx);
    until(cx, |cx| cx.debug_bounds("review-commit-0").is_some());
    cx.simulate_keystrokes("enter");
    until(cx, |cx| cx.debug_bounds("review-lines").is_some());
    assert!(cx.debug_bounds("review-commit-reviewed").is_some());
    cx.simulate_keystrokes("c");
    cx.simulate_input("keep it");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        comments(cx),
        [
            ("notes.md:1".into(), "why notes?".into()),
            ("nav.rs:1".into(), "keep it".into()),
        ],
        "a commit's lines take comments as the uncommitted work's do"
    );
}

/// A board draws its own machine's Pane: box's Thread, under review, is
/// the Pane its board lays out, though this computer's Thread of the same
/// number is open before it.
#[gpui::test]
fn a_board_draws_its_own_machines_pane(cx: &mut TestAppContext) {
    let (mut core, _fake, machines, _agents) = two_machines("board", 1);
    let mine = core.threads()[0];
    let on_box = machines.held(1).unwrap();
    let repo = repo_in(&settings_dir("board").join("src"));
    std::fs::write(repo.join("notes.md"), "fresh
").unwrap();
    let theirs = core
        .open(on_box, Provider::Claude, WorkspaceChoice::Main { checkout: repo })
        .unwrap();
    assert_eq!(theirs.thread(), mine.thread(), "ids are per Store");
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    view.update(cx, |view, cx| view.focus_thread(theirs, cx));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_beside(
                beside::Beside::Review(ferrite_reviewr::model::Scope::Uncommitted),
                window,
                cx,
            )
        })
    });
    until(cx, |cx| cx.debug_bounds("review-lines").is_some());
    let theirs_at = view.read_with(cx, |view, _| view.pane_for(theirs).unwrap());
    assert_ne!(view.read_with(cx, |view, _| view.pane_for(mine)), Some(theirs_at));
    let laid_out = cx.update(|window, cx| {
        view.read(cx)
            .pane_rects(window, cx)
            .into_iter()
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    });
    assert_eq!(laid_out, [theirs_at], "box's Pane, not this computer's");
}

/// The Server's word on `thread`'s Browser, as an attach answers it.
pub(super) fn served(cockpit: &mut Cockpit, thread: ThreadKey) -> ferrite_core::browser::State {
    let ask = cockpit.browser_attach(thread);
    cockpit.wait_for(ask).unwrap()
}

/// `thread` drawn, as a window draws it, and then its Browser attached:
/// box's Thread streams, and so has a Browser, only while it is drawn
/// (F-A2).
pub(super) fn drawn_and_served(cockpit: &mut Cockpit, thread: ThreadKey) {
    cockpit.draws_transcripts([thread]);
    cockpit.pump();
    served(cockpit, thread);
}

/// `action` on `thread`'s Browser, as the Server answers it.
pub(super) fn browsed(
    cockpit: &mut Cockpit,
    thread: ThreadKey,
    action: ferrite_core::browser::Action,
) -> ferrite_core::browser::State {
    let ask = cockpit.browser(thread, action);
    cockpit.wait_for(ask).unwrap()
}

/// While set, `browser_core`'s Server takes nothing more its window sends,
/// as over a line that stalled, till the test lets it go (`stall_browser_line`).
static BROWSER_LINE_STALLED: Mutex<bool> = Mutex::new(false);
static BROWSER_LINE_LET_GO: std::sync::Condvar = std::sync::Condvar::new();

/// `browser_core`'s Server takes nothing its window sends till the stall
/// drops: whatever the window asks meanwhile is answered after, however
/// fast the machine.
pub(super) fn stall_browser_line() -> LineStalled {
    *BROWSER_LINE_STALLED.lock().unwrap() = true;
    LineStalled
}

#[must_use = "the line goes again when the stall drops"]
pub(super) struct LineStalled;

impl LineStalled {
    /// The Server takes what the window sent again.
    pub(super) fn let_go(self) {}
}

impl Drop for LineStalled {
    fn drop(&mut self) {
        *BROWSER_LINE_STALLED.lock().unwrap_or_else(|e| e.into_inner()) = false;
        BROWSER_LINE_LET_GO.notify_all();
    }
}

/// A Cockpit attached to a Server whose browser window has one tab open on
/// a local page, and the Thread and tab document it belongs to.
pub(super) fn browser_core(name: &str) -> (Cockpit, ThreadKey, ferrite_core::browser::Document) {
    use ferrite_core::browser::Action;
    use ferrite_client::cockpit::{Attach, Endpoint, ServerState};
    /// What the window sends, unless the line is stalled.
    struct Line(Box<dyn std::io::Write + Send>);
    impl std::io::Write for Line {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut stalled = BROWSER_LINE_STALLED.lock().unwrap();
            while *stalled {
                stalled = BROWSER_LINE_LET_GO.wait(stalled).unwrap();
            }
            drop(stalled);
            self.0.write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.flush()
        }
    }
    let dir = scratch(name);
    let served = ferrite_server::fixture_machine(dir.join("box"), Box::new(OnBox(Default::default()))).unwrap();
    let dial: machine::Dial = Box::new(move || {
        let tunnel = served()?;
        Ok(Tunnel { to: Box::new(Line(tunnel.to)), ..tunnel })
    });
    let bridge = std::sync::Arc::new(Bridge::new("box", dir.join("bridge.sock"), dial));
    let mut core = Cockpit::attach(Attach {
        saved_as: ferrite_core::MachineKey(Some("me@box".into())),
        store: dir.join("window"), server: Endpoint::Machine(bridge),
        client: ferrite_core::protocol::Peer { name: name.into(), version: "0".into() },
        renderer: None, browser: true, driven: false,
    });
    ferrite_core::doorbell::until("browser window did not attach", || {
        core.pump(); matches!(core.server(core.first_machine()), Some(ServerState::Attached { .. }))
    });
    let thread = core.open(core.first_machine(), Provider::Claude, WorkspaceChoice::Main { checkout: here() }).unwrap();
    drawn_and_served(&mut core, thread);
    let opened = browsed(&mut core, thread, Action::Open { url: "http://localhost:5173/start".into() });
    let document = opened.tabs[0].document();
    (core, thread, document)
}

/// The public Server/window path must show its exact native page, including
/// provider-initiated opens and navigation that finishes before the next paint.
#[gpui::test]
fn conversation_browser_shares_the_live_preview_and_new_tab_opens_separately(cx: &mut TestAppContext) {
    use ferrite_core::browser::Action;
    use ferrite_pane::visual::{engine::Visuals, web::fake::{Call, FakeEngine}};
    let (core, thread, document) = browser_core("browser-preview");
    bind_production_keys(cx);
    let engine = std::rc::Rc::new(FakeEngine::new());
    cx.update(|cx| Visuals::install(engine.clone(), cx));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    until(cx, |cx| engine.live_views() == 1 && cx.debug_bounds("browser-preview").is_some());
    view.update(cx, |view, cx| {
        let state = view.cockpit.update(cx, |cockpit, _| browsed(cockpit, thread, Action::Navigate { document, url: "http://localhost:5173/first".into() }));
        view.cockpit.update(cx, |cockpit, _| browsed(cockpit, thread, Action::Navigate { document: state.tabs[0].document(), url: "http://localhost:5173/final".into() }));
    });
    until(cx, |cx| view.update(cx, |view, cx| {
        let state = view.cockpit.update(cx, |cockpit, _| served(cockpit, thread));
        state.tabs[0].url == "http://localhost:5173/final" && !state.tabs[0].loading
    }));
    assert_eq!(engine.live_views(), 1, "navigation retains the page displayed by the preview");
    assert!(engine.journal().contains(&Call::BrowserNavigate("http://localhost:5173/final".into())));
    let new_tab = cx.debug_bounds("browser-new-tab").unwrap();
    cx.simulate_click(new_tab.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input("http://localhost:5173/second");
    cx.simulate_keystrokes("enter");
    until(cx, |cx| view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs.len()) == 2));
    let state = view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| served(cockpit, thread)));
    assert_eq!(state.tabs[0].url, "http://localhost:5173/final", "new tab does not navigate the existing page");
    assert_eq!(state.tabs[1].url, "http://localhost:5173/second");
    // The address acts on the second tab once the window itself has it:
    // until then Enter would open a third.
    super::browser_chrome::page_shown_at(&view, thread, "http://localhost:5173/second", cx);
    engine.defer_browser_navigation(true);
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("http://localhost:5173/delayed");
    cx.simulate_keystrokes("enter");
    until(cx, |_| engine.journal().contains(&Call::BrowserNavigate("http://localhost:5173/delayed".into())));
    tick(cx);
    let pending = view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| served(cockpit, thread)));
    assert_eq!(pending.tabs[1].url, "http://localhost:5173/delayed", "old native metadata must not overwrite an accepted navigation intent");
    assert!(pending.tabs[1].loading);
    assert_eq!(pending.tabs[1].dirty, ferrite_core::browser::Dirty::Unknown);
    engine.finish_browser_navigations();
    until(cx, |cx| view.update(cx, |view, cx| {
        let state = view.cockpit.update(cx, |cockpit, _| served(cockpit, thread));
        state.tabs[1].url == "http://localhost:5173/delayed" && !state.tabs[1].loading
    }));
    engine.defer_browser_navigation(false);
    // And on the page it went to once the window has that one.
    super::browser_chrome::page_shown_at(&view, thread, "http://localhost:5173/delayed", cx);
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("http://localhost:5173/delayed#section");
    cx.simulate_keystrokes("enter");
    until(cx, |cx| view.update(cx, |view, cx| {
        let state = view.cockpit.update(cx, |cockpit, _| served(cockpit, thread));
        state.tabs[1].url == "http://localhost:5173/delayed#section" && !state.tabs[1].loading
    }));
    let wait_protection = |cx: &mut gpui::VisualTestContext, expected: bool, stage: &str| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        until(cx, |cx| {
            let (state, active) = view.update(cx, |view, cx| {
                (view.cockpit.update(cx, |cockpit, _| served(cockpit, thread)), view.cockpit.read(cx).window_active())
            });
            assert!(std::time::Instant::now() < deadline,
                "{stage}: expected protected={expected}, active={active}, state={state:?}");
            state.tabs.iter().any(|tab| tab.protected) == expected
        });
    };
    let viewport = cx.debug_bounds("browser-preview").unwrap();
    let page_point = viewport.center();
    cx.simulate_mouse_down(page_point, gpui::MouseButton::Left, gpui::Modifiers::none());
    wait_protection(cx, true, "held pointer begins");
    for _ in 0..3 { tick(cx); }
    assert!(view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs.iter().any(|tab| tab.protected))),
        "a held gesture must not settle between pointer events");
    cx.simulate_mouse_up(viewport.origin - gpui::point(px(10.), px(10.)), gpui::MouseButton::Left, gpui::Modifiers::none());
    wait_protection(cx, false, "outside pointer release settles");
    cx.simulate_click(page_point, gpui::Modifiers::none());
    // GPUI's keystroke helper emits only KeyDown. Real typing also releases
    // each key; held keys deliberately keep manual interaction active.
    cx.simulate_keystrokes("a");
    cx.simulate_event(gpui::KeyUpEvent { keystroke: gpui::Keystroke::parse("a").unwrap() });
    tick(cx);
    cx.simulate_keystrokes("b");
    cx.simulate_event(gpui::KeyUpEvent { keystroke: gpui::Keystroke::parse("b").unwrap() });
    tick(cx);
    assert!(view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs.iter().any(|tab| tab.protected))),
        "a typing burst must remain protected between keys");
    wait_protection(cx, false, "typing burst settles");
    cx.simulate_mouse_down(page_point, gpui::MouseButton::Left, gpui::Modifiers::none());
    wait_protection(cx, true, "second gesture begins");
    let add_tab = cx.debug_bounds("browser-new-tab").unwrap().center();
    cx.simulate_click(add_tab, gpui::Modifiers::none());
    wait_protection(cx, false, "focus loss settles");
}

#[gpui::test]
fn browser_live_annotation_keeps_focus_and_protection_until_guest_cleanup(cx: &mut TestAppContext) {
    use ferrite_core::browser::Action;
    use ferrite_client::cockpit::{Attach, Endpoint, ServerState};
    use ferrite_pane::visual::{
        engine::Visuals,
        live_annotation::{Event, Submission},
        web::{fake::{Call, FakeEngine}, Input},
    };
    use ferrite_core::browser_annotations::{LiveFeedback, LiveScreenshot, Rect, RegionTarget, Selection, Viewport};
    // Short: the machine's socket under it must fit macOS's 104-byte SUN_LEN.
    let dir = scratch("annotation-host");
    let dial = ferrite_server::fixture_machine(dir.join("box"), Box::new(OnBox(Default::default()))).unwrap();
    let bridge = std::sync::Arc::new(Bridge::new("box", dir.join("bridge.sock"), dial));
    let mut core = Cockpit::attach(Attach {
        saved_as: ferrite_core::MachineKey(Some("me@box".into())),
        store: dir.join("window"), server: Endpoint::Machine(bridge),
        client: ferrite_core::protocol::Peer { name: "browser-preview".into(), version: "0".into() },
        renderer: None, browser: true, driven: false,
    });
    ferrite_core::doorbell::until("browser window did not attach", || {
        core.pump(); matches!(core.server(core.first_machine()), Some(ServerState::Attached { .. }))
    });
    let thread = core.open(core.first_machine(), Provider::Claude, WorkspaceChoice::Main { checkout: here() }).unwrap();
    drawn_and_served(&mut core, thread);
    browsed(&mut core, thread, Action::Open { url: "http://localhost:5173/start".into() });
    bind_production_keys(cx);
    let engine = std::rc::Rc::new(FakeEngine::new());
    cx.update(|cx| Visuals::install(engine.clone(), cx));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    // Native callbacks and remote retention use several threads. Bound the
    // whole condition: repeated socket replies must not reset its deadline.
    let wait = |cx: &mut gpui::VisualTestContext, label: &str, done: &mut dyn FnMut(&mut gpui::VisualTestContext) -> bool| {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            leap(cx, super::super::user_commands::ENDED_POLL);
            if done(cx) { break; }
            assert!(std::time::Instant::now() < deadline, "{label}; native journal: {:?}", engine.journal().into_iter().rev().take(24).collect::<Vec<_>>());
            std::thread::yield_now();
        }
    };
    wait(cx, "browser becomes visible", &mut |cx| engine.live_views() == 1 && cx.debug_bounds("browser-preview").is_some());
    wait(cx, "page finishes loading", &mut |cx| view.update(cx, |view, cx| {
        !view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs[0].loading)
    }));
    let finished = || engine.journal().iter().filter(|call| matches!(call, Call::AnnotationFinished(_))).count();
    let protected = |cx: &mut gpui::VisualTestContext| view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs[0].protected)
    });
    let begin = |cx: &mut gpui::VisualTestContext| {
        let annotate = cx.debug_bounds("browser-annotate").expect("live page offers annotation");
        cx.simulate_click(annotate.center(), gpui::Modifiers::none());
        tick(cx);
    };
    // The button acts once the window has the page as loaded, and the
    // editor starts once the Server has answered for the page's
    // protection: each a beat or more later.
    let started = || engine.journal().iter().filter(|call| matches!(call, Call::AnnotationStarted(..))).count();
    let begin_live = |cx: &mut gpui::VisualTestContext| {
        super::browser_chrome::page_shown(&view, thread, cx);
        let before = started();
        begin(cx);
        wait(cx, "the guest editor starts", &mut |_| started() > before);
    };

    begin_live(cx);
    assert!(protected(cx), "the live editor protects the exact page");
    assert!(cx.debug_bounds("browser-annotation-overlay").is_none(), "the editor lives in the guest page");
    assert!(cx.update(|window, cx| view.read(cx).browser_annotation_focus(thread, cx).is_some_and(|focus| focus.is_focused(window))), "starting annotation focuses the rendered WebView");
    cx.simulate_keystrokes("escape");
    wait(cx, "Escape reaches the guest", &mut |_| engine.journal().iter().any(|call| matches!(call, Call::Input(Input::Key { key, down: true, .. }) if key == "escape")));
    // simulate_keystrokes sends only key-down; a real release ends the
    // independent operator gesture after the annotation lease is released.
    cx.simulate_event(gpui::KeyUpEvent { keystroke: gpui::Keystroke::parse("escape").unwrap() });
    engine.annotation_event(Event::Cancelled);
    wait(cx, "Cancel requests guest cleanup", &mut |_| finished() == 1);
    assert!(protected(cx), "cancel waits for CSS restoration");
    engine.annotation_event(Event::Closed);
    wait(cx, "cancelled session releases protection", &mut |cx| !protected(cx));

    begin_live(cx);
    let viewport = engine.browser_viewport().unwrap();
    let bounds = Rect { x: 12., y: 20., width: 80., height: 60. };
    engine.annotation_event(Event::Attach {
        feedback: LiveFeedback {
            id: "live-region".into(),
            page_url: "http://localhost:5173/start".into(),
            page_title: Some("Fixture".into()),
            comment: "Keep this region even without a screenshot".into(),
            elements: vec![],
            regions: vec![RegionTarget { id: "region-1".into(), rect: bounds.clone() }],
            strokes: vec![],
            style_changes: vec![],
            screenshot: LiveScreenshot::Unavailable { reason: "Not captured yet".into() },
            bounds,
            created_at: "2026-11-16T00:00:00.000Z".into(),
        },
        viewport: Viewport {
            width_css: f64::from(viewport.width),
            height_css: f64::from(viewport.height),
            device_scale: f64::from(viewport.scale),
        },
        submission: Submission::Attach,
    });
    wait(cx, "Attach capture requests guest cleanup", &mut |_| finished() == 2);
    assert!(protected(cx), "Attach keeps its lease while the guest restores styles");
    assert!(cx.debug_bounds("browser-annotation-attachment").is_none(), "no receipt is exposed before cleanup");
    engine.annotation_event(Event::Closed);
    wait(cx, "cleaned capture reaches the composer", &mut |cx| cx.debug_bounds("browser-annotation-attachment").is_some());
    assert!(!protected(cx), "durable retention releases the lease");
    let annotations = view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).browser_annotations().to_vec());
    assert_eq!(annotations.len(), 1);
    assert!(annotations[0].original().is_none(), "an unavailable capture creates no fake image");
    assert_eq!(annotations[0].comment(), "Keep this region even without a screenshot");
    assert!(matches!(&annotations[0].capture().selection, Selection::Live { feedback } if feedback.regions[0].id == "region-1" && matches!(feedback.screenshot, LiveScreenshot::Unavailable { .. })));

    begin_live(cx);
    let new_tab = cx.debug_bounds("browser-new-tab").unwrap();
    cx.simulate_click(new_tab.center(), gpui::Modifiers::none());
    wait(cx, "New Tab requests guest cleanup", &mut |_| finished() == 3);
    assert!(protected(cx), "hiding the page still waits for restoration");
    engine.annotation_event(Event::Closed);
    wait(cx, "hidden session releases protection", &mut |cx| !protected(cx));
    let before = started();
    begin(cx);
    assert_eq!(started(), before, "new address mode has no visible page to annotate");
}

/// A window holding a machine whose own `settings.json` is `there`, with its
/// Settings open and the machine's defaults read. Returns the machine's
/// settings directory too.
fn settings_on_a_machine<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    there: &ferrite_core::settings::Settings,
) -> (
    Entity<CockpitView>,
    &'a mut gpui::VisualTestContext,
    std::path::PathBuf,
) {
    use ferrite_client::cockpit::{Attach, Endpoint, ServerState};
    let dir = scratch(name);
    let remote = dir.join("r");
    std::fs::create_dir_all(&remote).unwrap();
    there.save(&remote).unwrap();
    let dial = ferrite_server::fixture_machine(
        remote.join("threads"),
        Box::new(OnBox(Default::default())),
    )
    .unwrap();
    let bridge = std::sync::Arc::new(Bridge::new(
        "box",
        dir.join("bridge").join("bridge.sock"),
        dial,
    ));
    let mut core = Cockpit::attach(Attach {
        saved_as: ferrite_core::MachineKey(Some("me@box".into())),
        browser: false,
        driven: false,
        store: dir.join("window").join("threads"),
        server: Endpoint::Machine(bridge),
        client: ferrite_core::protocol::Peer {
            name: "window".into(),
            version: "0".into(),
        },
        renderer: None,
    });
    ferrite_core::doorbell::until("client did not attach", || {
        core.pump();
        matches!(core.server(core.first_machine()), Some(ServerState::Attached { .. }))
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    bind_production_keys(cx);
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.shown_defaults(cx).is_some())
    });
    tick(cx);
    (view, cx, remote)
}

/// Settings on a machine show and edit that machine's own defaults: the
/// Server writes its file and the sheet shows what it reread, while this
/// computer's preferences stay as they were.
#[gpui::test]
fn settings_on_a_machine_edit_its_defaults_not_this_computers(cx: &mut TestAppContext) {
    use ferrite_core::settings::Settings;
    let there = Settings {
        factory_tasks_at_once: 5,
        placeholder_suggestions: false,
        codex_approval_policy: "never".into(),
        ..Default::default()
    };
    let (view, cx, remote) = settings_on_a_machine(cx, "rd", &there);
    let here = view.read_with(cx, |view, _| view.prefs.settings.clone());
    assert_ne!(here.factory_tasks_at_once, there.factory_tasks_at_once);
    let shown = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| view.shown_defaults(cx).unwrap())
    };
    assert_eq!(shown(cx), there.server_defaults(), "the machine's, not ours");

    let search = cx.debug_bounds("settings-search").unwrap();
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_input("at once");
    tick(cx);
    let at = cx.debug_bounds("settings-factory-limit-1").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            view.shown_defaults(cx)
                .is_some_and(|shown| shown.factory_tasks_at_once == 6)
        })
    });
    let search = cx.debug_bounds("settings-search").unwrap();
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_keystrokes(&ferrite_ui::keymap::PLATFORM.chord("a"));
    cx.simulate_input("follow-up");
    tick(cx);
    let at = cx
        .debug_bounds("settings-placeholder-suggestions")
        .unwrap()
        .center();
    cx.simulate_click(at, gpui::Modifiers::none());
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            view.shown_defaults(cx)
                .is_some_and(|shown| shown.placeholder_suggestions)
        })
    });

    let search = cx.debug_bounds("settings-search").unwrap();
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_keystrokes(&ferrite_ui::keymap::PLATFORM.chord("a"));
    cx.simulate_input("name threads");
    tick(cx);
    let at = cx.debug_bounds("settings-auto-title").unwrap().center();
    cx.simulate_click(at, gpui::Modifiers::none());
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            view.shown_defaults(cx)
                .is_some_and(|shown| !shown.auto_title)
        })
    });

    until(cx, |_| {
        Settings::load(&remote)
            .is_ok_and(|written| written.placeholder_suggestions && !written.auto_title)
    });
    let written = Settings::load(&remote).unwrap();
    assert_eq!(written.factory_tasks_at_once, 6, "written to its file");
    assert!(written.placeholder_suggestions);
    assert!(!written.auto_title, "its titles are the machine's to turn off");
    assert_eq!(
        written.codex_approval_policy, "never",
        "what the edit did not name stays"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.prefs.settings.clone()),
        here,
        "this computer's preferences did not move"
    );
    assert!(
        Settings::load(&view.read_with(cx, |view, _| view.prefs.dir.clone()))
            .map_or(true, |saved| saved.factory_tasks_at_once == here.factory_tasks_at_once)
    );
}

/// Two quick edits both land: the second is built on the first, not on what
/// the machine last said.
#[gpui::test]
fn two_quick_edits_to_a_machines_defaults_both_land(cx: &mut TestAppContext) {
    use ferrite_core::settings::Settings;
    let there = Settings {
        factory_tasks_at_once: 5,
        ..Default::default()
    };
    let (view, cx, remote) = settings_on_a_machine(cx, "rd-quick", &there);
    let search = cx.debug_bounds("settings-search").unwrap();
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_input("at once");
    tick(cx);
    let more = cx.debug_bounds("settings-factory-limit-1").unwrap().center();
    cx.simulate_click(more, gpui::Modifiers::none());
    cx.simulate_click(more, gpui::Modifiers::none());
    until(cx, |_| {
        Settings::load(&remote).is_ok_and(|written| written.factory_tasks_at_once == 7)
    });
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.factory_limit(cx)) == Some(7)
    });
}

/// A machine that cannot write its file says so beside the values it
/// kept: the window gives Settings the kept values and the refusal, so
/// both are drawn.
#[gpui::test]
fn a_machine_that_cannot_save_its_defaults_says_so(cx: &mut TestAppContext) {
    use ferrite_core::settings::Settings;
    let there = Settings {
        factory_tasks_at_once: 5,
        ..Default::default()
    };
    let (view, cx, remote) = settings_on_a_machine(cx, "rd-readonly", &there);
    let blocked = refuse_writes(&remote);
    let search = cx.debug_bounds("settings-search").unwrap();
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_input("at once");
    tick(cx);
    let more = cx.debug_bounds("settings-factory-limit-1").unwrap().center();
    cx.simulate_click(more, gpui::Modifiers::none());
    // Said on each page that shows the defaults: the sheet's own test.
    until(cx, |cx| matches!(defaults_said(&view, cx), (Ok(5), Some(said)) if said.contains("Not saved on box")));
    std::fs::remove_dir(blocked).unwrap();
}

/// The machine's settings in `remote` can be read but not written: the
/// file a write is made in first is a folder. What to remove to let
/// writes through again.
fn refuse_writes(remote: &std::path::Path) -> std::path::PathBuf {
    let blocked = remote.join(format!("{}.tmp", ferrite_core::settings::Settings::FILE));
    std::fs::create_dir_all(&blocked).unwrap();
    blocked
}

/// What the window gives Settings of the machine's defaults: the factory
/// limit it shows, or what stands in for the values; and what it says
/// beside them.
fn defaults_said(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> (Result<usize, String>, Option<String>) {
    view.read_with(cx, |view, cx| {
        let inputs = view.settings_inputs(cx);
        (
            inputs.defaults.map(|defaults| defaults.factory_tasks_at_once).map_err(|said| said.to_string()),
            inputs.unsaved.map(|said| said.to_string()),
        )
    })
}

/// A read that failed and then lands shows the values, not a refused edit.
#[cfg(unix)]
#[gpui::test]
fn a_failed_read_that_later_lands_is_not_an_unsaved_edit(cx: &mut TestAppContext) {
    use ferrite_core::settings::Settings;
    use std::os::unix::fs::PermissionsExt;
    let (view, cx, remote) = settings_on_a_machine(cx, "rd-reread", &Settings::default());
    let file = remote.join(Settings::FILE);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    view.update(cx, |view, cx| {
        view.environment.defaults = None;
        view.ask_server_defaults(cx);
    });
    until(cx, |cx| matches!(defaults_said(&view, cx), (Err(said), None) if said.contains("Could not read them")));
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    view.update(cx, |view, cx| view.ask_server_defaults(cx));
    until(cx, |cx| {
        view.read_with(cx, |view, cx| view.shown_defaults(cx).is_some())
    });
    tick(cx);
    assert!(matches!(defaults_said(&view, cx), (Ok(_), None)), "the values, and nothing said beside them");
}

/// An edit that saves after one was refused takes the refusal away.
#[gpui::test]
fn a_saved_edit_clears_an_earlier_refusal(cx: &mut TestAppContext) {
    use ferrite_core::settings::Settings;
    let there = Settings {
        factory_tasks_at_once: 5,
        ..Default::default()
    };
    let (view, cx, remote) = settings_on_a_machine(cx, "rd-resave", &there);
    let blocked = refuse_writes(&remote);
    let search = cx.debug_bounds("settings-search").unwrap();
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_input("at once");
    tick(cx);
    let more = cx.debug_bounds("settings-factory-limit-1").unwrap().center();
    cx.simulate_click(more, gpui::Modifiers::none());
    until(cx, |cx| matches!(defaults_said(&view, cx), (Ok(5), Some(said)) if said.contains("Not saved on box")));
    std::fs::remove_dir(blocked).unwrap();
    cx.simulate_click(more, gpui::Modifiers::none());
    until(cx, |_| {
        Settings::load(&remote).is_ok_and(|written| written.factory_tasks_at_once == 6)
    });
    until(cx, |cx| matches!(defaults_said(&view, cx), (Ok(6), None)));
}

/// box — an Engine of its own standing in for it — with an edited file of
/// its Thread open through a fake of box's file operations; box saved in
/// Settings, or with `unsaved` only named for this run.
fn machine_with_edited_remote_file<'a>(
    name: &str,
    unsaved: bool,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    Entity<crate::cockpit::tools::Editor>,
    &'a mut gpui::VisualTestContext,
) {
    use ferrite_core::file_operations::{fingerprint, Reply, Request};
    let (mut core, _fake, checkout) = bound_cockpit_as(name, Provider::Codex, on_box_key());
    fresh(name);
    let saved = if unsaved { Vec::new() } else { vec![box_machine()] };
    let mut machines = Machines::new(
        reach(name, needs_sign_in),
        &saved,
        unsaved.then_some("me@box"),
        false,
    );
    machines.beside(&mut core);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        view.prefs.settings.machines = saved;
        view.prefs.settings.save(&view.prefs.dir).unwrap();
        view.remote_files_for_test(
            thread,
            |request| {
                Ok(match request {
                    Request::Read { .. } => Reply::Text {
                        text: "remote original".into(),
                        fingerprint: fingerprint(b"remote original"),
                    },
                    Request::Write { text, .. } => Reply::Written {
                        fingerprint: fingerprint(text.as_bytes()),
                    },
                    _ => Reply::Unknown,
                })
            },
            cx,
        );
        view.open_in_files(thread, checkout.join("edited.rs"), None, true, cx);
    });
    tick(cx);
    let editor = view.read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread).unwrap());
    editor.update(cx, |editor, cx| editor.type_text("edited ", cx));
    (view, editor, cx)
}

/// An edited file of box's, saving or not, holds box: turning it off,
/// removing it or forgetting it changes nothing until the file is saved.
#[gpui::test]
fn remote_files_block_disconnect_and_remove_until_saved(cx: &mut TestAppContext) {
    for (name, remove, unsaved) in [
        ("disconnect", false, false),
        ("remove", true, false),
        ("forget-unsaved", true, true),
    ] {
        let (view, editor, cx) =
            machine_with_edited_remote_file(&format!("remote-files-{name}"), unsaved, cx);
        let id = if remove {
            "connection-remove-1"
        } else {
            "connection-toggle-1"
        };
        let action = connection_act(&view, id, cx);
        cx.update(|window, cx| action(window, cx));
        let action = if remove {
            connection_removal(&view, cx)
        } else {
            action
        };
        cx.update(|window, cx| action(window, cx));
        tick(cx);
        view.read_with(cx, |view, cx| {
            assert!(editor.read(cx).is_dirty(), "{name} preserves the unsaved buffer");
            assert_eq!(
                view.prefs.settings.machines,
                ferrite_core::settings::Settings::load(&view.prefs.dir)
                    .unwrap()
                    .machines
            );
            let machines = view.machines.as_ref().unwrap();
            assert!(machines.held(1).is_some(), "{name} keeps box held");
            if unsaved {
                assert!(machines.unsaved().is_some());
            } else {
                assert!(view.prefs.settings.machines[0].enabled);
            }
        });
        editor.update(cx, |editor, cx| editor.save(cx));
        assert!(
            view.read_with(cx, |view, cx| {
                let on_box = view.machines.as_ref().unwrap().held(1).unwrap();
                view.files_leave_error(on_box, cx)
            })
            .is_some(),
            "{name}: an in-flight save still holds box"
        );
        tick(cx);
        assert!(!editor.read_with(cx, |editor, _| editor.is_dirty()));
        cx.update(|window, cx| action(window, cx));
        tick(cx);
        view.read_with(cx, |view, cx| {
            if remove {
                assert!(view.prefs.settings.machines.is_empty());
                assert!(!view.machines.as_ref().unwrap().listed(), "{name} forgets box");
            } else {
                assert!(!view.prefs.settings.machines[0].enabled);
            }
            assert_eq!(view.cockpit.read(cx).machines().count(), 0, "{name} lets box go");
        });
    }
}

/// With this computer turned off and no machine else, the board says so
/// and starts nothing; turning this computer on and off again in
/// Connections saves the switch and holds it or lets it go.
#[gpui::test]
fn local_environment_switch_persists_and_empty_client_blocks_new_threads(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    fresh("local-environment-toggle");
    let mut machines = Machines::new(reach("local-environment-toggle", needs_sign_in), &[], None, false);
    let core = machines.cockpit();
    let dir = settings_dir("local-environment-toggle-prefs");
    let mut prefs = Preferences::ephemeral();
    prefs.dir = dir.clone();
    prefs.settings.local_environment_enabled = false;
    prefs.settings.save(&dir).unwrap();
    let (view, cx) = window_of(cx, core, machines, prefs);
    tick(cx);
    assert!(cx.debug_bounds("no-environment").is_some());
    cx.simulate_keystrokes("cmd-n");
    cx.simulate_keystrokes("cmd-shift-n");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.panes.is_empty());
        assert!(view.cockpit.read(cx).this_computer().is_none());
        assert!(
            view.shown_defaults(cx).is_none(),
            "no machine: no local file stands in"
        );
    });
    for enabled in [true, false] {
        let toggle = connection_act(&view, "connection-toggle-local", cx);
        cx.update(|window, cx| toggle(window, cx));
        tick(cx);
        assert_eq!(
            ferrite_core::settings::Settings::load(&dir)
                .unwrap()
                .local_environment_enabled,
            enabled
        );
        view.read_with(cx, |view, cx| {
            assert_eq!(view.cockpit.read(cx).environment_available(), enabled);
            assert_eq!(view.cockpit.read(cx).this_computer().is_some(), enabled);
            assert_eq!(view.prefs.settings.local_environment_enabled, enabled);
        });
    }
    assert!(cx.debug_bounds("no-environment-connections").is_some());
}

/// The empty board's `Open Connections` opens Settings on Connections, as
/// ⌘, would.
#[gpui::test]
fn open_connections_opens_settings_on_connections(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    fresh("open-connections");
    let mut machines = Machines::new(reach("open-connections", needs_sign_in), &[], None, false);
    let core = machines.cockpit();
    let mut prefs = Preferences::ephemeral();
    prefs.settings.local_environment_enabled = false;
    let (view, cx) = window_of(cx, core, machines, prefs);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    view.update(cx, |view, cx| {
        view.hide_sheet(cx);
    });
    tick(cx);
    let open = cx.debug_bounds("no-environment-connections").expect("Open Connections");
    cx.simulate_click(open.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.settings_open(cx), "Settings opens");
        let sheet = view.settings_sheet(cx).unwrap();
        assert_eq!(sheet.read(cx).page(), settings::PageKey::Connections, "on Connections");
    });
}

#[gpui::test]
fn local_environment_switch_preserves_unreadable_settings(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("local-toggle-unreadable", 1);
    let prefs = Preferences {
        unreadable: Some("expected value at line 1 column 1".into()),
        ..Preferences::ephemeral()
    };
    let path = prefs.dir.join(ferrite_core::settings::Settings::FILE);
    std::fs::create_dir_all(&prefs.dir).unwrap();
    std::fs::write(&path, b"not json").unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Codex, prefs, cx)
    });
    let toggle = connection_act(&view, "connection-toggle-local", cx);
    cx.update(|window, cx| toggle(window, cx));
    view.read_with(cx, |view, _| {
        assert!(view.prefs.settings.local_environment_enabled);
    });
    assert_eq!(std::fs::read(&path).unwrap(), b"not json");
}

/// Letting box go forgets what the toasts heard of it: its toast and its
/// Notices' watermark go with it, while this computer's toast and
/// watermark stay.
#[gpui::test]
fn letting_a_machine_go_forgets_only_its_toasts(cx: &mut TestAppContext) {
    let (mut core, fake, machines, agents) = two_machines("forget-toasts", 2);
    let here = core.threads();
    let (mine, local) = (here[0].machine(), here[1]);
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    core.focus_thread(here[0]);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    until(cx, |cx| view.read_with(cx, |view, cx| view.attached(on_box, cx)));
    ring(cx);
    // A turn fails on box and on this computer, each off the board.
    view.update(cx, |view, cx| {
        let _ = view.cockpit.update(cx, |cockpit, _| cockpit.send(theirs, "go".into()));
    });
    agents.say(0, SessionEvent::TextDelta { text: "working\n\n".into() });
    until(cx, |cx| running(&view, theirs, cx));
    let failed = || SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Error("rate limited".into()),
        cost_usd: None,
    };
    agents.say(0, failed());
    fake.streams.borrow()[1].send(failed()).unwrap();
    let heard = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let needs_you = view.toasts.read(cx).needs_you();
            let mut toasts: Vec<_> = needs_you.toasts().iter().map(|toast| toast.alert.thread).collect();
            toasts.sort();
            (toasts, needs_you.presented(on_box), needs_you.presented(mine))
        })
    };
    until(cx, |cx| {
        ring(cx);
        heard(cx).0.len() == 2
    });
    let (toasts, box_mark, mark) = heard(cx);
    let mut both = vec![theirs, local];
    both.sort();
    assert_eq!(toasts, both, "a toast of each machine stands");
    assert!(box_mark.is_some() && mark.is_some(), "each machine's Notices heard");

    view.update(cx, |view, cx| {
        view.prefs.settings.machines.clear();
        view.machines_changed(cx);
    });
    assert_eq!(heard(cx), (vec![local], None, mark), "box's toast and watermark went");
}

/// A side chat's question that box's fork refuses comes back to the field
/// it was asked from, though another tab shows by the time the refusal
/// crosses back.
#[gpui::test]
fn a_side_chat_question_box_refuses_goes_back_to_the_tab_it_was_asked_from(cx: &mut TestAppContext) {
    use super::side_chat::{draft, shown_fork, side_tabs};
    bind_production_keys(cx);
    let (mut core, _fake, machines, agents) = two_machines("side-chat-unsent", 0);
    let on_box = machines.held(1).unwrap();
    let theirs = open_on(&mut core, on_box);
    let (view, cx) = window_of(cx, core, machines, Preferences::ephemeral());
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    until(cx, |cx| view.read_with(cx, |view, _| view.pane_for(theirs).is_some()));
    view.update(cx, |view, cx| view.focus_thread(theirs, cx));
    tick(cx);
    agents.say(0, SessionEvent::TextDelta { text: "alpha\n\n".into() });
    agents.say(
        0,
        SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        },
    );
    until(cx, |cx| shows(&view, theirs, "alpha", cx));
    for _ in 0..2 {
        view.update(cx, |view, cx| view.open_side_chat(theirs, cx));
        until(cx, |cx| {
            view.read_with(cx, |view, cx| {
                shown_fork(view, theirs, cx).is_some() && view.side_chat.read(cx).unsettled().is_empty()
            })
        });
    }
    let tabs = side_tabs(&view, theirs, cx);
    assert_eq!(tabs.len(), 2);
    agents.refuse.store(true, Ordering::SeqCst);
    cx.simulate_input("what is alpha?");
    // Box takes the question only once the field is seen empty and the
    // first tab shows, however loaded the machine: its refusal cannot
    // cross back before.
    let held = hold_box_once(settings_dir("side-chat-unsent"));
    cx.simulate_keystrokes("enter");
    let asked = view.read_with(cx, |view, cx| draft(view, theirs, cx));
    assert_eq!(asked.as_deref(), Some(""), "the field gave its question up");
    let first = debug_bounds(cx, format!("panel-tab-SideChat-{}", tabs[0])).unwrap();
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    held.let_go();
    let field = |view: &CockpitView, index, cx: &gpui::App| {
        let field = view.side_chat.read(cx).field_of(theirs, index).cloned();
        field.map(|field| field.read(cx).text().to_string())
    };
    until(cx, |cx| view.read_with(cx, |view, cx| field(view, 1, cx).as_deref() == Some("what is alpha?")));
    view.read_with(cx, |view, cx| {
        assert_eq!(view.side_chat.read(cx).titles(theirs).1, 0, "the first tab shows");
        assert_eq!(field(view, 0, cx).as_deref(), Some(""), "not in the tab shown");
    });
}
