//! Stand-ins for the Phase-0 hooks the parity world and its scenes consume.
//!
//! The parity scenes are written against the shapes the integrator's Phase-0
//! commit freezes (`ferrite_core::clock::Fixture`,
//! `GroupChange::Include`, `Notifications::fixture_backdate`,
//! `platform_text::disable_font_smoothing`, and the
//! `CockpitView::{scroll_transcript, preview_path, open_palette,
//! set_nav_fold, show_empty_board, set_launch_dir, fixture_hold_nav}`
//! hooks). That commit had not landed on `feat/terminal-native` when this
//! package forked, so each hook is stood in for here, in this one file, with
//! the frozen signature and no behaviour. The trait methods share the
//! inherent methods' names: once the real hooks exist the inherent ones win
//! method resolution, and rebasing onto the integrated tree is deleting this
//! file and pointing the `use` lines in `parity.rs` and `parity_scenes.rs`
//! at the real items.

// Stand-ins take what the real hooks will read and read none of it yet.
#![allow(dead_code)]

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use ferrite_core::cockpit::Cockpit;
use ferrite_core::groups::GroupId;
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::ThreadId;
use gpui::{Context, Window};

use crate::cockpit::CockpitView;

/// `ferrite_core::clock`: the fixture clock every visible time falls out
/// of. Phase 0 ships it as a no-op stub; core-transcript makes it real.
pub(crate) mod clock {
    use super::*;

    /// Installs a fixed wall clock for the process until dropped.
    pub(crate) struct Fixture;

    impl Fixture {
        pub(crate) fn install(_at: SystemTime) -> Self {
            Fixture
        }

        pub(crate) fn set(&self, _at: SystemTime) {}

        pub(crate) fn advance(&self, _by: Duration) {}
    }
}

/// `GroupChange::Include { thread, group, index }`: join a second Group
/// without leaving the first (R1, Groups are non-exclusive). Until core has
/// the variant there is nothing to apply: Everything holds only the members
/// that are in no other Group.
pub(crate) fn include(
    _core: &mut Cockpit,
    _thread: ThreadId,
    _group: GroupId,
    _index: Option<usize>,
) {
}

/// `Notifications::fixture_backdate(thread, at)`: restamp a Thread's
/// notices, for the prototype strings whose ages disagree with its stamps.
pub(crate) fn backdate(_core: &mut Cockpit, _thread: ThreadId, _at: SystemTime) {}

/// `crate::platform_text::disable_font_smoothing()`: grayscale glyph
/// antialiasing, the way the prototype's `-webkit-font-smoothing` draws.
pub(crate) fn disable_font_smoothing() {}

/// `crate::palette::PaletteScope`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaletteScope {
    All,
    Groups,
}

/// `crate::transcript::ScrollTarget`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ScrollTarget {
    Top,
    Tail,
    TurnBand { turn: usize, offset: f32 },
}

/// `crate::nav::NavFold`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NavFold {
    Project(ProjectId),
    Group(GroupId),
    Parked,
}

/// The window-side hooks, by the names and signatures Phase 0 freezes.
pub(crate) trait Phase0Hooks: Sized + 'static {
    fn scroll_transcript(&mut self, pane: usize, target: ScrollTarget, cx: &mut Context<Self>);
    fn preview_path(
        &mut self,
        pane: usize,
        path: &str,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
    fn open_palette(
        &mut self,
        scope: PaletteScope,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
    fn set_nav_fold(&mut self, fold: NavFold, folded: bool, cx: &mut Context<Self>);
    fn show_empty_board(&mut self, cx: &mut Context<Self>);
    fn set_launch_dir(&mut self, dir: PathBuf);
    fn fixture_hold_nav(&mut self, progress: Option<f32>);
}

impl Phase0Hooks for CockpitView {
    fn scroll_transcript(&mut self, _: usize, _: ScrollTarget, _: &mut Context<Self>) {}
    fn preview_path(
        &mut self,
        _: usize,
        _: &str,
        _: Option<u32>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
    }
    fn open_palette(&mut self, _: PaletteScope, _: &str, _: &mut Window, _: &mut Context<Self>) {}
    fn set_nav_fold(&mut self, _: NavFold, _: bool, _: &mut Context<Self>) {}
    fn show_empty_board(&mut self, _: &mut Context<Self>) {}
    fn set_launch_dir(&mut self, _: PathBuf) {}
    fn fixture_hold_nav(&mut self, _: Option<f32>) {}
}
