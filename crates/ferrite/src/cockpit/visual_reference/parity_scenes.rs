//! The parity scenes: every view of the approved prototype, rebuilt with
//! the real renderer on the one parity world (`crate::demo::parity`) and
//! captured at the app size, to be diffed against the prototype's own
//! shots (`/tmp/ferrite-tn-proto/<view>.png`).
//!
//! The world is built for the scene's `Look`; the setup then does only what
//! the operator would have done on the way to that view: scroll, hover a
//! path, open a picker, the palette or the bell, fold the nav.

use std::path::PathBuf;

use ferrite_core::groups::GroupId;
use ferrite_core::workspace::BranchStatus;
use ferrite_core::ThreadId;
use gpui::{Context, Window};

use super::{pane_of, CockpitView, HeldLoops, Home, Scene, Setup};
use crate::demo::parity::{self, Cast, Look, World, CAPTURE};
use crate::demo::world::{self, Clock, Fixture};
use crate::motion;
use crate::nav::NavFold;
use crate::palette::PaletteScope;
use crate::platform_text::disable_font_smoothing;
use crate::transcript::ScrollTarget;

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
    /// group-toast.png: Close stale's request arriving as a toast, the
    /// operator away.
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
    /// Where the prototype's shot caught its loops: the caret's blink and
    /// the shimmer on the browser's CSS clock, the spinners' frames on its
    /// timers' — read off each shot (the caret's ink, the braille frame).
    /// Two instants recur: the caret at 0.557 of its ink (550ms into its
    /// blink) or at 0.775 (533ms), the braille spinner on `⠧` (575ms) or
    /// `⠦` (520ms); the star spinner is on `✻` in both.
    fn held(self) -> motion::HeldLoops {
        use std::time::Duration;
        let (css, script) = match self {
            Shot::Solo | Shot::SoloTop | Shot::Wall => (550, 575),
            Shot::Group => (550, 520),
            Shot::SoloHover
            | Shot::SoloPicker
            | Shot::Toast
            | Shot::Notes
            | Shot::Palette
            | Shot::Empty
            | Shot::Collapsed
            | Shot::Ride => (533, 520),
        };
        motion::HeldLoops {
            css: Duration::from_millis(css),
            script: Duration::from_millis(script),
        }
    }

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

/// The parity scene `state` (one of `SHOTS`) and its setup.
pub(super) fn build(state: &str) -> (Scene, Setup) {
    let shot = SHOTS
        .iter()
        .find(|(name, _)| *name == state)
        .map(|(_, shot)| *shot)
        .expect("a parity state");
    // The prototype's shots are grayscale-antialiased.
    disable_font_smoothing();
    let held_loops = HeldLoops::hold(shot.held());
    // The changed tokens the prototype marks by hand, on its world's diffs.
    crate::pane::set_word_marks(parity::word_marks());
    let root =
        std::env::temp_dir().join(format!("ferrite-reference-{}-{state}", std::process::id()));
    // Only our own disposable directory; never the operator's store.
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create disposable reference root");
    // Midnight is read off the real clock, before the fixture replaces it.
    let midnight = world::midnight();
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
    let home = Home::point_at(&world.home);
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
            Box::new(held_loops) as Box<dyn std::any::Any>,
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
                // Its Pane is on the board, and a request in view rings
                // nothing while the operator is there (ADR 0017), so the
                // Cockpit counts the operator away once the window's
                // activation has landed. The window keeps the keyboard, so
                // the caret still draws as the prototype's.
                let cockpit = cx.entity();
                window.on_next_frame(move |_, cx| {
                    cockpit.update(cx, |view, _| view.cockpit.set_window_active(false));
                });
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
    // The prototype's world runs the 0.5.0 release (its banner's version).
    view.shown_version = "0.5.0".into();
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
    // The requests standing when the window opened were heard then: none
    // of them toasts (the toast scene delivers its one request after).
    view.ring_bell(cx);
    view.bell.clear_toasts();
    // The prototype's window has the keyboard: the focused Composer's caret
    // is the solid block, not the hollow one an inactive window draws.
    window.activate_window();
}

/// Solo as solo.png scrolls it: the prototype's own call, turn two's band
/// top plus 180px, which lands at the end — the band pinned and the hunk
/// header at y≈92.
fn solo_scroll(view: &mut CockpitView, cast: &Cast, cx: &mut Context<CockpitView>) {
    let pane = pane_of(view, cast.nav);
    view.scroll_transcript(
        pane,
        ScrollTarget::TurnBand {
            turn: 1,
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
                turn: 0,
                offset: 0.0,
            },
            cx,
        );
    }
}
