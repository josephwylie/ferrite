//! Where the web engine comes from: one per process, started the first time
//! a visual wants it.
//!
//! - [`boot`] runs first in `main`. In a Chromium helper process it runs the
//!   helper and exits; in the app it only notes whether Chromium is here
//!   (macOS, the `cef` feature, inside the bundle) and readies it — no
//!   Chromium process exists yet.
//! - [`init`] installs the [`Visuals`] global once GPUI is up.
//! - [`Visuals::engine`] is what everything else asks. The first ask starts
//!   Chromium (in a foreground task, outside any update: CEF may spin the
//!   native loop) and answers [`Availability::Starting`]; windows redraw
//!   when it is up. A process that never shows a visual never starts it.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, Global, SharedString};

use super::web::Engine;

/// Whether visuals can be drawn live here, and with what.
#[derive(Clone)]
pub(crate) enum Availability {
    Ready(Rc<dyn Engine>),
    /// Chromium is starting; ask again on the next draw.
    Starting,
    /// No engine in this build or on this platform: why, in the operator's
    /// words.
    Unavailable(SharedString),
}

#[cfg_attr(not(all(target_os = "macos", feature = "cef")), allow(dead_code))]
enum State {
    #[cfg(all(target_os = "macos", feature = "cef"))]
    Dormant(super::web::cef::Dormant),
    Starting,
    Ready(Rc<dyn Engine>),
    Unavailable(SharedString),
}

/// The process's web engine (a GPUI global).
pub(crate) struct Visuals {
    state: State,
}

impl Global for Visuals {}

thread_local! {
    /// What [`boot`] found, until [`init`] takes it into the global.
    static BOOTED: RefCell<Option<State>> = const { RefCell::new(None) };
    /// The engine once started: Chromium starts once per process, so an
    /// app built again in the same process (the captures build one per
    /// scene) gets the same engine.
    static STARTED: RefCell<Option<Rc<dyn Engine>>> = const { RefCell::new(None) };
}

/// Visuals aren't drawn on this platform yet.
pub(crate) const NOT_ON_THIS_PLATFORM: &str = "Visuals aren't available on Windows yet";
/// A build without the engine (`cargo run` without `--features cef`).
pub(crate) const NOT_IN_THIS_BUILD: &str = "Visuals aren't available in this build";
/// The engine is built in but Chromium isn't beside the binary.
#[cfg_attr(not(all(target_os = "macos", feature = "cef")), allow(dead_code))]
pub(crate) const NOT_OUTSIDE_THE_BUNDLE: &str = "Visuals need Ferrite.app (Chromium isn't here)";

/// Call first thing in `main`, before anything else runs. A Chromium helper
/// process (this same binary, started by Chromium) runs and exits here.
pub(crate) fn boot() {
    let state = boot_state();
    BOOTED.with(|booted| *booted.borrow_mut() = Some(state));
}

#[cfg(all(target_os = "macos", feature = "cef"))]
fn boot_state() -> State {
    use super::web::cef;
    match cef::boot() {
        Ok(cef::Boot::Subprocess(code)) => std::process::exit(code),
        Ok(cef::Boot::Dormant(dormant)) => State::Dormant(dormant),
        Err(error) => {
            // Expected under `cargo run`; worth one line, never a failure.
            eprintln!("ferrite: visuals off: {error}");
            State::Unavailable(NOT_OUTSIDE_THE_BUNDLE.into())
        }
    }
}

#[cfg(not(all(target_os = "macos", feature = "cef")))]
fn boot_state() -> State {
    State::Unavailable(if cfg!(target_os = "macos") { NOT_IN_THIS_BUILD } else { NOT_ON_THIS_PLATFORM }.into())
}

/// Install the global from what [`boot`] found (nothing found: unavailable).
pub(crate) fn init(cx: &mut App) {
    if let Some(engine) = STARTED.with(|started| started.borrow().clone()) {
        Visuals::install(engine, cx);
        return;
    }
    let state = BOOTED
        .with(|booted| booted.borrow_mut().take())
        .unwrap_or_else(boot_state_unbooted);
    cx.set_global(Visuals { state });
}

fn boot_state_unbooted() -> State {
    State::Unavailable(if cfg!(target_os = "windows") { NOT_ON_THIS_PLATFORM } else { NOT_IN_THIS_BUILD }.into())
}

impl Visuals {
    /// Use `engine` for every visual (tests, captures): ready at once, its
    /// pump started.
    #[allow(dead_code)]
    pub(crate) fn install(engine: Rc<dyn Engine>, cx: &mut App) {
        super::element::pump(engine.clone(), cx);
        cx.set_global(Visuals { state: State::Ready(engine) });
    }

    /// The engine, starting it on the first ask. Never blocks.
    pub(crate) fn engine(cx: &mut App) -> Availability {
        if !cx.has_global::<Visuals>() {
            return Availability::Unavailable(NOT_IN_THIS_BUILD.into());
        }
        // Read, not `global_mut`: touching the global mutably notifies its
        // observers (every transcript), which would draw again and ask again.
        match &cx.global::<Visuals>().state {
            State::Ready(engine) => return Availability::Ready(engine.clone()),
            State::Starting => return Availability::Starting,
            State::Unavailable(why) => return Availability::Unavailable(why.clone()),
            #[cfg(all(target_os = "macos", feature = "cef"))]
            State::Dormant(_) => {}
        }
        #[cfg(all(target_os = "macos", feature = "cef"))]
        {
            let visuals = cx.global_mut::<Visuals>();
            let State::Dormant(dormant) = std::mem::replace(&mut visuals.state, State::Starting) else {
                unreachable!("matched above")
            };
            // Outside this update: cef_initialize may run the native loop.
            cx.spawn(async move |cx| {
                let started = std::time::Instant::now();
                let state = match dormant.start() {
                    Ok(engine) => State::Ready(Rc::new(engine)),
                    Err(error) => {
                        eprintln!("ferrite: Chromium failed to start: {error}");
                        State::Unavailable(format!("Visuals failed to start: {error}").into())
                    }
                };
                if std::env::var_os("FERRITE_CEF_TRACE").is_some() {
                    eprintln!("ferrite visuals: Chromium started in {:?}", started.elapsed());
                }
                let _ = cx.update(|cx| {
                    if let State::Ready(engine) = &state {
                        STARTED.with(|started| *started.borrow_mut() = Some(engine.clone()));
                        super::element::pump(engine.clone(), cx);
                    }
                    cx.set_global(Visuals { state });
                    cx.refresh_windows();
                });
            })
            .detach();
            Availability::Starting
        }
    }
}
