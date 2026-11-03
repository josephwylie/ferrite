//! The UI refresh's scenes: every mockup of `docs/design/ui-refresh/`,
//! drawn by the real renderer on the one mockup world
//! (`crate::demo::mockup`) at the mockups' 1600×960 (size `refresh`), to be
//! compared with `docs/design/ui-refresh/mockups/<view>.png`.
//!
//! The world is built for the scene's `Look` and pull request; the setup
//! then does only what the operator would have done on the way to that
//! view: open the panel, or the palette with a query.

use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::workspace::BranchStatus;
use ferrite_core::ThreadId;
use gpui::{Context, Window};

use super::{CockpitView, HeldLoops, Home, Scene, Setup};
use crate::demo::mockup::{self, Look, Pr, World, CAPTURE};
use crate::demo::world::{self, Clock, Fixture};
use crate::nav::NavFold;
use crate::palette::PaletteScope;
use ferrite_ui::platform_text::disable_font_smoothing;

/// One refresh capture: the mockup it reproduces (saved as
/// `<state>-refresh.png`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shot {
    /// solo.png: Ferrite idle memory usage, the panel open, the pull
    /// request open with its checks running.
    Solo,
    /// solo-changes.png: the same, a changed file unfolded.
    SoloChanges,
    /// group.png: WORK's four Panes, Supplier Price Review focused, Find
    /// all calls asking.
    Group,
    /// pr-states.png, first column: the panel's pull request row, a check
    /// failed and the branch conflicting with main.
    PrFailing,
    /// pr-states.png, second column: the row, merged.
    PrMerged,
    /// pr-states.png, third column: the panel with no pull request.
    PrNone,
    /// No mockup: the row for a pull request with a description and a head
    /// branch too long for its line.
    PrDescribed,
    /// command-search.png: the palette over WORK with `supp` typed, the
    /// cursor on Supplier Price Review.
    Palette,
}

/// Every refresh state, with the shot it takes.
const SHOTS: [(&str, Shot); 8] = [
    ("refresh-solo", Shot::Solo),
    ("refresh-solo-changes", Shot::SoloChanges),
    ("refresh-group", Shot::Group),
    ("refresh-pr-failing", Shot::PrFailing),
    ("refresh-pr-merged", Shot::PrMerged),
    ("refresh-pr-none", Shot::PrNone),
    ("refresh-pr-described", Shot::PrDescribed),
    ("refresh-palette", Shot::Palette),
];

/// Whether `state` is one of the refresh scenes.
pub(super) fn handles(state: &str) -> bool {
    SHOTS.iter().any(|(name, _)| *name == state)
}

impl Shot {
    fn look(self) -> Look {
        match self {
            Shot::Group | Shot::Palette => Look::Group,
            Shot::Solo
            | Shot::SoloChanges
            | Shot::PrFailing
            | Shot::PrMerged
            | Shot::PrNone
            | Shot::PrDescribed => Look::Solo,
        }
    }

    /// What the forge reads for the crash fix's branch.
    fn pr(self) -> Pr {
        match self {
            Shot::PrFailing => Pr::Failing,
            Shot::PrMerged => Pr::Merged,
            Shot::PrNone => Pr::None,
            Shot::PrDescribed => Pr::Described,
            Shot::Solo | Shot::SoloChanges | Shot::Group | Shot::Palette => Pr::Open,
        }
    }

    /// Whether the scene opens the panel; not on a Group.
    fn panel(self) -> bool {
        !matches!(self, Shot::Group | Shot::Palette)
    }
}

/// The refresh scene `state` (one of `SHOTS`) and its setup.
pub(super) fn build(state: &str) -> (Scene, Setup) {
    let shot = SHOTS
        .iter()
        .find(|(name, _)| *name == state)
        .map(|(_, shot)| *shot)
        .expect("a refresh state");
    // The mockups are the browser's grayscale-antialiased shots.
    disable_font_smoothing();
    // Spinners and the caret held at one frame, so captures compare.
    let held_loops = HeldLoops::hold(ferrite_ui::motion::HeldLoops {
        css: std::time::Duration::from_millis(550),
        script: std::time::Duration::from_millis(575),
    });
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
    let built = mockup::build(
        &root,
        shot.look(),
        shot.pr(),
        Clock {
            midnight,
            fixture: Some(&fixture),
        },
    );
    // Paths read `~/Desktop/Projects/…` for as long as the capture runs.
    let home = Home::point_at(&built.home);
    let World {
        core,
        sessions,
        cast,
        folded,
        branches,
        ..
    } = built;
    let scene = Scene {
        core,
        feeds: sessions.feeds(),
        root,
        hold: vec![
            Box::new(fixture) as Box<dyn std::any::Any>,
            Box::new(home) as Box<dyn std::any::Any>,
            Box::new(held_loops) as Box<dyn std::any::Any>,
        ],
    };
    let setup: Setup = Box::new(move |view, window, cx| {
        // group.png draws Complete BB supplier list `done`: a finish that
        // is on screen while the window has the operator's focus is seen at
        // once (ADR 0015's seen rule), so the operator reads that word only
        // while the window is in the background. The shot is taken so.
        let active = shot != Shot::Group;
        prepare(view, branches, &folded, active, window, cx);
        // The Groups the mockups draw folded: InvoiceMate Business, and
        // WORK wherever it is not the view.
        let shut: Vec<_> = view
            .cockpit
            .groups()
            .iter()
            .filter(|group| {
                let title = group.display_title();
                title == "InvoiceMate Business" || (title == "WORK" && shot.look() == Look::Solo)
            })
            .map(|group| group.id)
            .collect();
        for group in shut {
            view.set_nav_fold(NavFold::Group(group), true, cx);
        }
        if shot.panel() {
            view.open_panel(cast.memory, cx);
        }
        if shot == Shot::SoloChanges {
            unfold_file(view, cast.memory, "scheduled_work.rs", cx);
        }
        if shot == Shot::Palette {
            view.open_palette(PaletteScope::All, "supp", window, cx);
        }
        cx.notify();
    });
    (scene, setup)
}

/// What every refresh view shares: the CLIs it
/// found, the checkouts' branches (before git's own reads land), workmate
/// and tempo folded, no toast for a request that was standing when the
/// window opened, and a window that has the keyboard when `active`.
fn prepare(
    view: &mut CockpitView,
    branches: Vec<(ThreadId, BranchStatus)>,
    folded: &[ProjectId],
    active: bool,
    window: &mut Window,
    cx: &mut Context<CockpitView>,
) {
    // Seeded, so no scene probes the operator's own CLIs.
    view.cli_versions = Some((
        "2.1.289 \u{b7} /usr/local/bin/claude".into(),
        "0.160.0 \u{b7} /usr/local/bin/codex".into(),
    ));
    view.cli_probing = true;
    // The nav foot's `Ferrite 0.5`.
    view.shown_version = "0.5.0".into();
    view.facts.set_branches(
        branches
            .into_iter()
            .map(|(thread, status)| (thread, Some(status)))
            .collect(),
    );
    for project in folded {
        view.set_nav_fold(NavFold::Project(*project), true, cx);
    }
    use gpui::component::WindowExt as _;
    window.clear_notifications(cx);
    view.ring_alerts(cx);
    view.needs_you.clear_toasts();
    if active {
        window.activate_window();
    }
}

/// The Changes tab's block for `name` unfolded, as solo-changes.png draws
/// `scheduled_work.rs`: its hunks read from the crash fix's worktree.
fn unfold_file(
    view: &mut CockpitView,
    thread: ThreadId,
    name: &str,
    cx: &mut Context<CockpitView>,
) {
    let path = view.facts.get(thread).and_then(|facts| {
        facts
            .changed_files
            .iter()
            .find(|file| file.path.ends_with(name))
            .map(|file| file.path.clone())
    });
    match path {
        Some(path) => view.toggle_panel_file(thread, path, cx),
        None => eprintln!("ferrite: refresh scene: no changed file {name} to unfold"),
    }
}
