//! The vendored Tasks runtime requires Unix. Keep that platform boundary
//! explicit without starting jobs or presenting an empty, apparently live board.

use super::*;
use crate::palette::Row as PaletteRow;
use crate::projects::Command;
use gpui::component::notification::{Notification, NotificationType};
use gpui::component::WindowExt as _;
use std::collections::{HashMap, HashSet};

pub(super) const TASKS_HEADER_H: f32 = 0.;

#[derive(Default)]
pub(super) struct ProjectsState {
    pub live: bool,
    pub sidebar: Sidebar,
}

// The nav can consume the same optional task decorations on every platform.
// No Tasks runtime exists here to populate them.
#[derive(Default)]
pub(super) struct Sidebar {
    pub owned: HashSet<ThreadId>,
    pub agents: HashMap<ThreadId, Told>,
}

pub(super) struct Entry {
    pub name: String,
    pub progress: String,
}

pub(super) struct Told {
    pub entry: Entry,
}

impl Sidebar {
    pub(super) fn heading(&self, _: impl IntoIterator<Item = ThreadId>) -> Option<&Entry> {
        None
    }
}

impl Told {
    pub(super) fn rows(&self) -> crate::nav::TaskRows {
        crate::nav::TaskRows {
            stands_timed: "".into(),
            stands: "".into(),
            ink: 0,
            third: "".into(),
        }
    }
}

impl ProjectsState {
    pub(super) fn new(_: bool) -> Self {
        Self::default()
    }
    pub(super) fn board_shown(&self) -> bool {
        false
    }
    pub(super) fn coordinator_shown(&self) -> bool {
        false
    }
    pub(super) fn coordinator_thread(&self) -> Option<ThreadId> {
        None
    }
    pub(super) fn popup_shown(&self) -> bool {
        false
    }
    pub(super) fn drop_popup(&mut self) {}
}

impl CockpitView {
    pub(super) fn start_sidebar_refresh(_: &mut Context<Self>) {}
    pub(super) fn board_gives_way(&mut self) {}
    pub(super) fn present_projects_toasts(&mut self, _: &mut Window, _: &mut Context<Self>) {}
    pub(super) fn tasks_tally(&self) -> Option<gpui::SharedString> {
        None
    }
    pub(super) fn projects_float(&self, _: &mut Window, _: &mut Context<Self>) -> Vec<AnyElement> {
        Vec::new()
    }
    pub(super) fn palette_projects_rows(&self) -> Vec<PaletteRow> {
        Vec::new()
    }
    pub(super) fn coordinator_page(&self, _: &mut Window, _: &mut Context<Self>) -> Div {
        div()
    }

    pub(super) fn toggle_projects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.push_notification(
            Notification::new()
                .title("Tasks")
                .message("Tasks are not available on Windows yet.")
                .with_type(NotificationType::Info),
            cx,
        );
    }

    pub(super) fn show_tasks_for(
        &mut self,
        _: ProjectId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_projects(window, cx);
    }

    pub(super) fn run_projects_command(
        &mut self,
        _: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_projects(window, cx);
    }
}
