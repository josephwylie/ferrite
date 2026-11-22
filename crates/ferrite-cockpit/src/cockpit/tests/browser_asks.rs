//! A Browser's asks of the Server never hold the window's thread (ADR
//! 0041): what each costs the window, read from core's probe
//! (`ferrite_core::cockpit::waits`), and a window whose Server never
//! answers that keeps pumping and drawing until the ask times out.
use super::machines::OnBox;
use super::*;
use ferrite_core::browser::{Action, State};
use ferrite_core::cockpit::{waits, Attach, Endpoint, ServerState};
use ferrite_core::machine::{Bridge, Dial, Tunnel};
use ferrite_pane::visual::{engine::Visuals, web::fake::FakeEngine};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A window's end of a machine's link that, while `silent`, drops every
/// Browser command it would send: a Server that never answers one.
struct Silencing {
    to: Box<dyn std::io::Write + Send>,
    silent: Arc<AtomicBool>,
    line: Vec<u8>,
}

impl std::io::Write for Silencing {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for &byte in buf {
            self.line.push(byte);
            if byte != b'\n' {
                continue;
            }
            let line = std::mem::take(&mut self.line);
            let browser = serde_json::from_slice::<serde_json::Value>(&line).is_ok_and(|message| {
                message["type"] == "command"
                    && message["op"].as_str().is_some_and(|op| op.starts_with("browser"))
            });
            if !(browser && self.silent.load(Ordering::SeqCst)) {
                self.to.write_all(&line)?;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.to.flush()
    }
}

/// A window attached to a machine's Server through a link that goes silent
/// for Browser commands when told, with one Thread whose Browser has one
/// tab open on a local page.
fn silenceable_browser(name: &str) -> (Cockpit, ThreadKey, Arc<AtomicBool>) {
    let dir = scratch(name);
    let dial = ferrite_server::fixture_machine(dir.join("box"), Box::new(OnBox(Default::default()))).unwrap();
    let silent = Arc::new(AtomicBool::new(false));
    let through = silent.clone();
    let dial: Dial = Box::new(move || {
        let Tunnel { from, to, ended } = dial()?;
        Ok(Tunnel { from, to: Box::new(Silencing { to, silent: through.clone(), line: Vec::new() }), ended })
    });
    let bridge = Arc::new(Bridge::new("box", dir.join("bridge.sock"), dial));
    let mut core = Cockpit::attach(Attach {
        store: dir.join("window"), server: Endpoint::Machine(bridge),
        client: ferrite_core::protocol::Peer { name: name.into(), version: "0".into() },
        renderer: None, browser: true, driven: false, discovery: Box::new(Fake::default()),
        saved_as: Some(ferrite_core::MachineKey(Some("box".into()))),
    }).unwrap();
    ferrite_core::doorbell::until("browser window did not attach", || {
        core.pump(); matches!(core.server(), Some(ServerState::Attached { .. }))
    });
    let thread = core.open(Provider::Claude, WorkspaceChoice::Main { checkout: here() }).unwrap();
    super::machines::served(&mut core, thread);
    super::machines::browsed(&mut core, thread, Action::Open { url: "http://localhost:5173/start".into() });
    (core, thread, silent)
}

/// The Browser's tabs as the window has them, and its notice.
fn shown(view: &Entity<CockpitView>, thread: ThreadKey, cx: &mut gpui::VisualTestContext) -> (State, Option<String>) {
    view.read_with(cx, |view, _| view.browsers.shown_for_test(thread)).unwrap_or_default()
}

fn loaded(view: &Entity<CockpitView>, thread: ThreadKey, url: &str, cx: &mut gpui::VisualTestContext) -> bool {
    let (state, _) = shown(view, thread, cx);
    state.tabs.iter().any(|tab| Some(tab.id) == state.active && tab.url == url && tab.live && !tab.loading)
}

/// Pump until `done`, for at most ten seconds, saying which step it was
/// that never came.
fn step(cx: &mut gpui::VisualTestContext, label: &str, mut done: impl FnMut(&mut gpui::VisualTestContext) -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !done(cx) {
        assert!(std::time::Instant::now() < deadline, "{label} never happened");
        leap(cx, super::super::user_commands::ENDED_POLL);
        std::thread::yield_now();
    }
}

/// What each kind of command cost the window's thread: how many, whether
/// it waited for the reply, the median, the 90th centile and the worst.
fn distribution(waits: &[waits::Wait]) -> Vec<String> {
    let mut by: std::collections::BTreeMap<(String, bool), Vec<Duration>> = Default::default();
    for wait in waits {
        by.entry((wait.op.clone(), wait.waited)).or_default().push(wait.took);
    }
    by.into_iter()
        .map(|((op, waited), mut took)| {
            took.sort();
            let at = |share: f64| took[((took.len() - 1) as f64 * share).round() as usize];
            format!(
                "BROWSER_ASKS {op} {} n={} median={:?} p90={:?} max={:?}",
                if waited { "waited" } else { "sent" },
                took.len(),
                at(0.5),
                at(0.9),
                took[took.len() - 1],
            )
        })
        .collect()
}

/// A Browser's ordinary life over a machine's link, from its first frame:
/// the window attaching, its page realized and reported, an address gone
/// to, a click on the page, a second tab, which leaves the first to be
/// reclaimed. Each Browser ask is sent and answered later; none waits on
/// the window's thread (`waited`).
#[gpui::test]
fn a_browsers_asks_never_wait_on_the_windows_thread(cx: &mut TestAppContext) {
    let (core, thread, _) = silenceable_browser("asks-cost");
    bind_production_keys(cx);
    let engine = std::rc::Rc::new(FakeEngine::new());
    engine.clean_browser_pages(true);
    cx.update(|cx| Visuals::install(engine.clone(), cx));
    waits::start();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    view.update(cx, |view, cx| view.open_browser(thread, cx));
    step(cx, "the first page loads", |cx| engine.live_views() == 1 && loaded(&view, thread, "http://localhost:5173/start", cx));

    let address = cx.debug_bounds("browser-address").unwrap().center();
    cx.simulate_click(address, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("http://localhost:5173/next");
    cx.simulate_keystrokes("enter");
    step(cx, "Go loads the address", |cx| loaded(&view, thread, "http://localhost:5173/next", cx));

    let page = cx.debug_bounds("browser-preview").unwrap().center();
    cx.simulate_click(page, gpui::Modifiers::none());
    step(cx, "a click protects the page", |cx| shown(&view, thread, cx).0.tabs.iter().any(|tab| tab.protected));
    step(cx, "the click settles", |cx| !shown(&view, thread, cx).0.tabs.iter().any(|tab| tab.protected));

    let new_tab = cx.debug_bounds("browser-new-tab").unwrap().center();
    cx.simulate_click(new_tab, gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input("http://localhost:5173/second");
    cx.simulate_keystrokes("enter");
    step(cx, "a second tab loads", |cx| loaded(&view, thread, "http://localhost:5173/second", cx));
    step(cx, "the first tab is reclaimed", |cx| shown(&view, thread, cx).0.tabs.first().is_some_and(|tab| !tab.live));

    let waits: Vec<_> = waits::take().into_iter().filter(|wait| wait.op.starts_with("browser")).collect();
    for line in distribution(&waits) {
        eprintln!("{line}");
    }
    let ops: std::collections::BTreeSet<_> = waits.iter().map(|wait| wait.op.as_str()).collect();
    for op in ["browser_attach", "browser", "browser_page", "browser_input", "browser_settled", "browser_reclaim", "browser_reclaimed"] {
        assert!(ops.contains(op), "the Browser sent {op}: {ops:?}");
    }
    let waited: Vec<_> = waits.iter().filter(|wait| wait.waited).collect();
    assert!(waited.is_empty(), "no Browser ask waits for its reply on the window's thread: {waited:?}");
}

/// A Server that never answers a Browser ask: the press that asks returns
/// at once, the window keeps pumping and drawing while the ask is out, and
/// once the ask's time is up the Browser says the Server did not answer.
#[gpui::test]
fn a_server_that_never_answers_a_browser_ask_leaves_the_window_responsive(cx: &mut TestAppContext) {
    let clock = hold_wall_clock();
    let (core, thread, silent) = silenceable_browser("asks-silent");
    bind_production_keys(cx);
    let engine = std::rc::Rc::new(FakeEngine::new());
    cx.update(|cx| Visuals::install(engine.clone(), cx));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    step(cx, "the first page loads", |cx| engine.live_views() == 1 && loaded(&view, thread, "http://localhost:5173/start", cx));

    silent.store(true, Ordering::SeqCst);
    waits::start();
    let reload = cx.debug_bounds("browser-reload").unwrap().center();
    let pressed = std::time::Instant::now();
    cx.simulate_click(reload, gpui::Modifiers::none());
    let press = pressed.elapsed();

    let (pumps, renders) = (crate::cockpit::drawn::pumps(), crate::cockpit::RENDERS.with(std::cell::Cell::get));
    let beats = std::time::Instant::now();
    for width in 0..20 {
        cx.simulate_resize(gpui::size(px(1600. - width as f32), px(900.)));
        cx.executor().advance_clock(Duration::from_millis(PUMP_MS));
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    let beats = beats.elapsed();
    let pumped = crate::cockpit::drawn::pumps() - pumps;
    let drew = crate::cockpit::RENDERS.with(std::cell::Cell::get) - renders;
    let (_, outstanding) = shown(&view, thread, cx);
    let waited: Vec<_> = waits::take().into_iter().filter(|wait| wait.op.starts_with("browser") && wait.waited).collect();
    eprintln!("SILENT_SERVER press={press:?} beats={beats:?} pumps={pumped} renders={drew} waited={waited:?}");
    assert!(press < Duration::from_secs(1), "the press returns at once: {press:?}");
    assert!(pumped >= 10 && drew >= 10, "the window pumps and draws while the ask is out: {pumped} pumps, {drew} renders");
    assert!(waited.is_empty(), "nothing waited on the Server: {waited:?}");
    assert!(outstanding.is_none(), "no error while the ask is out: {outstanding:?}");

    clock.advance(Duration::from_secs(11));
    tick(cx);
    let (_, notice) = shown(&view, thread, cx);
    assert!(
        notice.as_deref().is_some_and(|notice| notice.contains("did not answer in time")),
        "the timed out ask says so: {notice:?}"
    );
    assert!(cx.debug_bounds("browser-notice").is_some(), "and the Browser draws it");
}

/// An ask's answer is an arrival: the beat that brings it has the model
/// say `Arrived`, even a beat on which the Server said nothing (the ask
/// timed out), and `Arrived` is what every feature that subscribes to the
/// model hears (ADR 0041). So a feature view could ask the Server itself
/// and hear its answer without the root.
#[test]
fn an_asks_answer_is_an_arrival() {
    let clock = hold_wall_clock();
    let (mut core, thread, silent) = silenceable_browser("asks-arrive");
    silent.store(true, Ordering::SeqCst);
    let mut ask = core.browser_attach(thread);
    core.pump();
    assert!(ask.answer().is_none(), "a silent Server has not answered");
    core.take_arrived();
    clock.advance(Duration::from_secs(11));
    core.pump();
    assert!(core.take_arrived(), "the beat that answered it says Arrived");
    let answer = ask.answer().expect("its time is up, which answers it");
    let error = answer.expect_err("a silent Server's ask fails");
    assert!(error.to_string().contains("did not answer in time"), "{error}");
}

/// Keys typed on a page faster than the Server answers each one all reach
/// the page, in the order typed.
#[gpui::test]
fn a_burst_of_keys_all_reach_the_page_in_order(cx: &mut TestAppContext) {
    typed_keys_reach_the_page("asks-burst", true, cx);
}

/// A gesture that ends while the Server is asked (here the window lost the
/// operator, which ends one each frame) still gives the page its input.
#[gpui::test]
fn keys_reach_the_page_though_their_gesture_ended_while_the_server_was_asked(cx: &mut TestAppContext) {
    typed_keys_reach_the_page("asks-ended", false, cx);
}

fn typed_keys_reach_the_page(name: &str, window_active: bool, cx: &mut TestAppContext) {
    use ferrite_pane::visual::web::fake::Call;
    use ferrite_pane::visual::web::Input;
    let (core, thread, _) = silenceable_browser(name);
    bind_production_keys(cx);
    let engine = std::rc::Rc::new(FakeEngine::new());
    engine.clean_browser_pages(true);
    cx.update(|cx| Visuals::install(engine.clone(), cx));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    view.update(cx, |view, cx| view.open_browser(thread, cx));
    step(cx, "the first page loads", |cx| engine.live_views() == 1 && loaded(&view, thread, "http://localhost:5173/start", cx));
    let page = cx.debug_bounds("browser-preview").unwrap().center();
    cx.simulate_click(page, gpui::Modifiers::none());
    step(cx, "the click settles", |cx| !shown(&view, thread, cx).0.tabs.iter().any(|tab| tab.protected));

    if !window_active {
        cx.deactivate_window();
        tick(cx);
    }
    let typed = "packaged annotation feedback";
    cx.update(|window, cx| {
        for character in typed.chars() {
            let mut key = gpui::Keystroke::parse(if character == ' ' { "space" } else { "a" }).unwrap();
            key.key = character.to_string();
            key.key_char = Some(character.to_string());
            window.dispatch_keystroke(key.clone(), cx);
            window.dispatch_event(gpui::PlatformInput::KeyUp(gpui::KeyUpEvent { keystroke: key }), cx);
        }
    });
    let reached = || -> String {
        engine
            .journal()
            .iter()
            .filter_map(|call| match call {
                Call::Input(Input::Key { key, down: true, .. }) => Some(key.clone()),
                _ => None,
            })
            .collect()
    };
    step(cx, "the keys settle", |cx| !reached().is_empty() && !shown(&view, thread, cx).0.tabs.iter().any(|tab| tab.protected));
    assert_eq!(reached(), typed);
}
