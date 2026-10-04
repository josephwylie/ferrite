//! The parity scenes: every view of the approved prototype, rebuilt with
//! the real renderer on the one parity world (`crate::demo::parity`) and
//! captured at the app size, to be diffed against the prototype's own
//! shots (`/tmp/ferrite-tn-proto/<view>.png`).
//!
//! The world is built for the scene's `Look`; the setup then does only what
//! the operator would have done on the way to that view: scroll, hover a
//! path, open a picker, the palette or the bell, fold the nav.

use std::ffi::OsString;
use std::path::PathBuf;

use ferrite_core::groups::GroupId;
use ferrite_core::workspace::BranchStatus;
use ferrite_core::ThreadId;
use gpui::{Context, Window};

use super::{CockpitView, Scene, Setup};
use crate::demo::parity::phase0::{
    clock::Fixture, disable_font_smoothing, NavFold, PaletteScope, Phase0Hooks as _, ScrollTarget,
};
use crate::demo::parity::{self, Cast, Clock, Look, World, CAPTURE};

/// One parity capture: the prototype view it reproduces, and the state
/// name the capture is saved under (`parity-<view>-app.png`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shot {
    /// solo.png: turn two's band pinned, the Composer focused.
    Solo,
    /// solo-top.png: the banner and turn one.
    SoloTop,
    /// solo-hover.png: the path preview under the first Update's path.
    SoloHover,
    /// solo-picker.png: the model picker over the status line.
    SoloPicker,
    /// group.png: Perf sweep, four transcript Panes.
    Group,
    /// group-toast.png: Close stale's request arriving as a toast.
    Toast,
    /// group-notes.png: the notifications list down.
    Notes,
    /// palette.png: ⌘K with `par` typed.
    Palette,
    /// wall.png: Everything at wall range.
    Wall,
    /// empty.png: nothing on the board.
    Empty,
    /// collapsed.png: Solo with the nav folded, settled.
    Collapsed,
    /// The nav's fold held halfway, proving the board rides with it.
    Ride,
}

/// Every parity state, with the shot it takes.
const SHOTS: [(&str, Shot); 12] = [
    ("parity-solo", Shot::Solo),
    ("parity-solo-top", Shot::SoloTop),
    ("parity-solo-hover", Shot::SoloHover),
    ("parity-solo-picker", Shot::SoloPicker),
    ("parity-group", Shot::Group),
    ("parity-group-toast", Shot::Toast),
    ("parity-group-notes", Shot::Notes),
    ("parity-palette", Shot::Palette),
    ("parity-wall", Shot::Wall),
    ("parity-empty", Shot::Empty),
    ("parity-collapsed", Shot::Collapsed),
    ("parity-collapsed-ride", Shot::Ride),
];

/// Whether `state` is one of the parity scenes.
pub(super) fn handles(state: &str) -> bool {
    SHOTS.iter().any(|(name, _)| *name == state)
}

impl Shot {
    fn look(self) -> Look {
        match self {
            Shot::Group | Shot::Palette => Look::Group,
            Shot::Toast => Look::Toast,
            Shot::Notes => Look::Notes,
            Shot::Wall => Look::Wall,
            Shot::Solo
            | Shot::SoloTop
            | Shot::SoloHover
            | Shot::SoloPicker
            | Shot::Empty
            | Shot::Collapsed
            | Shot::Ride => Look::Solo,
        }
    }
}

/// HOME as it was before the capture pointed it at the world's: restored
/// when the scene is dropped, after its screenshot is saved.
struct Home(Option<OsString>);

impl Drop for Home {
    fn drop(&mut self) {
        match self.0.take() {
            Some(home) => std::env::set_var("HOME", home),
            None => std::env::remove_var("HOME"),
        }
    }
}

/// The parity scene `state` (one of `SHOTS`) and its setup.
pub(super) fn build(state: &str) -> (Scene, Setup) {
    let shot = SHOTS
        .iter()
        .find(|(name, _)| *name == state)
        .map(|(_, shot)| *shot)
        .expect("a parity state");
    // The prototype's shots are grayscale-antialiased.
    disable_font_smoothing();
    let root =
        std::env::temp_dir().join(format!("ferrite-reference-{}-{state}", std::process::id()));
    // Only our own disposable directory; never the operator's store.
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create disposable reference root");
    // Midnight is read off the real clock, before the fixture replaces it.
    let midnight = parity::midnight();
    let capture = Clock {
        midnight,
        fixture: None,
    }
    .instant(CAPTURE);
    let fixture = Fixture::install(capture);
    let world = parity::build(
        &root,
        shot.look(),
        Clock {
            midnight,
            fixture: Some(&fixture),
        },
    );
    // Paths read `~/ferrite` and `~/Desktop/Projects`, as the prototype
    // prints them, for as long as the capture runs.
    let home = Home(std::env::var_os("HOME"));
    std::env::set_var("HOME", &world.home);
    let World {
        core,
        feeds,
        launch,
        zeron,
        cast,
        everything,
        held,
        ..
    } = world;
    let scene = Scene {
        core,
        feeds,
        root,
        hold: vec![
            Box::new(fixture) as Box<dyn std::any::Any>,
            Box::new(home) as Box<dyn std::any::Any>,
        ],
    };
    let setup: Setup = Box::new(move |view, window, cx| {
        prepare(view, &cast, everything, launch, zeron, window, cx);
        match shot {
            Shot::Solo => solo_scroll(view, &cast, cx),
            Shot::SoloTop => {
                let pane = pane_of(view, cast.nav);
                view.scroll_transcript(pane, ScrollTarget::Top, cx);
            }
            Shot::SoloHover => {
                solo_scroll(view, &cast, cx);
                // The first Update's path, under the pinned band: the card
                // hangs below it at x≈368, y≈76 (solo-hover.png).
                let pane = pane_of(view, cast.nav);
                view.preview_path(pane, "crates/ferrite/src/nav.rs", None, window, cx);
            }
            Shot::SoloPicker => {
                solo_scroll(view, &cast, cx);
                view.open_provider_picker(cast.nav, cx);
            }
            Shot::Group => board_scroll(view, &cast, cx),
            Shot::Toast => {
                board_scroll(view, &cast, cx);
                // The one request that arrives while the window is up.
                if let Some((feed, event)) = held {
                    let _ = feed.send(event);
                    view.pump(cx);
                }
            }
            Shot::Notes => {
                board_scroll(view, &cast, cx);
                view.toggle_notifications(&crate::cockpit::ToggleNotifications, window, cx);
            }
            Shot::Palette => {
                board_scroll(view, &cast, cx);
                view.open_palette(PaletteScope::All, "par", window, cx);
            }
            Shot::Wall => {}
            Shot::Empty => view.show_empty_board(cx),
            Shot::Collapsed => {
                view.toggle_nav(&crate::cockpit::ToggleNav, window, cx);
                solo_scroll(view, &cast, cx);
            }
            Shot::Ride => {
                view.toggle_nav(&crate::cockpit::ToggleNav, window, cx);
                // Halfway through the 200ms ride: the board's left edge is
                // between 0 and the column's 281.8px.
                view.fixture_hold_nav(Some(0.5));
                solo_scroll(view, &cast, cx);
            }
        }
        cx.notify();
    });
    (scene, setup)
}

/// What every parity view shares: where Ferrite was launched, the CLIs it
/// found, the checkouts' branches, Everything folded in the nav, no toast
/// for a request that was already standing when the window opened, and a
/// window that has the keyboard.
fn prepare(
    view: &mut CockpitView,
    cast: &Cast,
    everything: GroupId,
    launch: PathBuf,
    zeron: PathBuf,
    window: &mut Window,
    cx: &mut Context<CockpitView>,
) {
    view.set_launch_dir(launch);
    // Seeded, so no scene probes the operator's own CLIs.
    view.cli_versions = Some((
        "2.1.289 \u{b7} /usr/local/bin/claude".into(),
        "0.160.0 \u{b7} /usr/local/bin/codex".into(),
    ));
    view.cli_probing = true;
    // ~/ferrite is checked out on `dev`; zeron on its default `main`.
    let branch = |name: &str| {
        Some(BranchStatus {
            branch: Some(name.into()),
            ..Default::default()
        })
    };
    let in_zeron = |thread: ThreadId| {
        view.cockpit
            .thread(thread)
            .and_then(|open| open.workspace())
            .is_some_and(|binding| binding.cwd().starts_with(&zeron))
    };
    let branches: Vec<_> = cast
        .open()
        .into_iter()
        .map(|thread| {
            let name = if in_zeron(thread) { "main" } else { "dev" };
            (thread, branch(name))
        })
        .collect();
    view.facts.set_branches(branches);
    view.set_nav_fold(NavFold::Group(everything), true, cx);
    use gpui::component::WindowExt as _;
    window.clear_notifications(cx);
    // The prototype's window has the keyboard: the focused Composer's caret
    // is the solid block, not the hollow one an inactive window draws.
    window.activate_window();
}

/// The Pane showing `thread`.
fn pane_of(view: &CockpitView, thread: ThreadId) -> usize {
    view.panes
        .iter()
        .position(|pane| pane.thread() == Some(thread))
        .expect("a parity Thread has a Pane")
}

/// Solo as solo.png scrolls it: the prototype's own call, turn two's band
/// top plus 180px, which lands at the end — the band pinned and the hunk
/// header at y≈92.
fn solo_scroll(view: &mut CockpitView, cast: &Cast, cx: &mut Context<CockpitView>) {
    let pane = pane_of(view, cast.nav);
    view.scroll_transcript(
        pane,
        ScrollTarget::TurnBand {
            turn: 2,
            offset: 180.0,
        },
        cx,
    );
}

/// Every board Pane with its first prompt band at the body's top, the
/// banner above it (R2), where the transcript is tall enough to get there.
fn board_scroll(view: &mut CockpitView, cast: &Cast, cx: &mut Context<CockpitView>) {
    for thread in cast.sweep() {
        let pane = pane_of(view, thread);
        view.scroll_transcript(
            pane,
            ScrollTarget::TurnBand {
                turn: 1,
                offset: 0.0,
            },
            cx,
        );
    }
}
