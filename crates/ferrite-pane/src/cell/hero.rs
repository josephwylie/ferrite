//! The start hero a Pane's cell draws while it has nothing to show: a
//! draft's Project word, which drops the Project menu, or an empty
//! Thread's settled word.

use gpui::prelude::*;
use gpui::{Entity, MouseButton, SharedString};

use super::{PaneAsk, PaneAsks};
use crate::pane::{self, PaneView};

/// An empty draft's hero project word (`pane::start_hero`, Codex's home):
/// the draft's Project, `project`, a switcher that drops the Project menu
/// under it (`Band::from_hero`).
pub(super) fn draft_hero(
    project: &SharedString,
    pane: &Entity<PaneView>,
    door: ferrite_ui::components::MenuDoor,
    asks: &PaneAsks,
    cx: &gpui::App,
) -> Option<pane::Hero> {
    pane.read(cx).draft()?;
    // The press stops here, so the root's dismissal cannot close what it
    // opens.
    let word = pane::hero_project(project.clone(), true).on_mouse_down(
        MouseButton::Left,
        asks.press(PaneAsk::BandMenu {
            chip: pane::BandChip::Project,
            from_hero: true,
        }),
    );
    Some(pane::Hero::Project(door.hung(word).into_any_element()))
}

/// An empty Thread's hero: its Project's word, settled, so a word and not
/// a switcher, or the scratch wording for a Thread with no Project. None
/// while the Thread's facts (its Project's and branch's words) are unknown.
pub(super) fn thread_hero(
    facts: Option<&(Option<SharedString>, Option<SharedString>)>,
) -> Option<pane::Hero> {
    Some(match facts?.0.clone() {
        Some(project) => pane::Hero::Project(pane::hero_project(project, false).into_any_element()),
        None => pane::Hero::Scratch,
    })
}
