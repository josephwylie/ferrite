//! A Task's Rework and Direct this task sheets in the window (#222, ADR
//! 0041, ADR 0042). The sheet is `ferrite_factory::direct::DirectView`,
//! which the sheet host holds while it is up. It reads its Task through a
//! `Source` the root makes over itself (`direct_inputs`), asking about its
//! Project and Task; what it asks it says as a `DirectEvent`. Opening it,
//! the picker its choosers open, keeping what it held on its Factory once
//! it closes, and the sends, Server calls, stay the root's (`direction`).

use super::workflows::Pick;
use super::*;
use ferrite_core::factory_board::{Task, TaskId};
use ferrite_core::ProjectKey;
use ferrite_factory::direct::{Choice, DirectEvent, DirectQuery, DirectSheet, DirectView};
use ferrite_ui::source::Source;
use gpui::App;

impl CockpitView {
    /// The Task the sheet of `project`'s Factory draws, as the board has it
    /// now: none once that Factory no longer shows, or the Task left the
    /// board or runs no Workflow. It never reads the sheet: the sheet calls
    /// it while its own update is running.
    pub(super) fn direct_inputs(&self, project: ProjectKey, task: &str, cx: &App) -> Option<Task> {
        if self.factory_project()? != project {
            return None;
        }
        self.cockpit
            .read(cx)
            .factory(project.machine())
            .task(task)
            .filter(|task| task.workflow.is_some())
            .cloned()
    }

    fn on_direct_event(&mut self, _: Entity<DirectView>, event: &DirectEvent, cx: &mut Context<Self>) {
        match event.clone() {
            DirectEvent::SendRework => self.send_rework(cx),
            DirectEvent::ApplyRedirect => self.apply_redirect(cx),
            DirectEvent::OpenPicks(picks, trigger, at) => {
                let picks = picks.iter().map(|(item, pick)| (item.clone(), Pick::Task(pick.clone()))).collect();
                self.open_picks(picks, &trigger, at, cx)
            }
            DirectEvent::Closed => {
                self.hide_sheet(cx);
            }
        }
    }

    /// The Rework or Direct this task sheet, while it is up.
    pub(crate) fn direct_sheet(&self, cx: &App) -> Option<Entity<DirectView>> {
        self.sheets.read(cx).direct().cloned()
    }

    /// A sheet on the open Task, in place of any other sheet, with what
    /// was typed and chosen in it before.
    pub(crate) fn open_direct_sheet(&mut self, sheet: DirectSheet, cx: &mut Context<Self>) {
        let Some(task) = self.open_task_now(cx).map(|task| task.id.clone()) else {
            return;
        };
        let (Some(project), Some(view)) = (self.factory_project(), self.factory()) else {
            return;
        };
        let direction = view.read(cx).direction.clone();
        let inputs = Source::of(&cx.entity(), |root: &Self, (project, task): &DirectQuery, cx| {
            root.direct_inputs(*project, task, cx)
        });
        let sheet = direction.update(cx, |direction, cx| {
            // The sheet's field takes the keyboard; the answer box does not
            // have it back once the sheet closes.
            direction.set_answering(false);
            let kept = direction.kept(&task);
            cx.new(|cx| DirectView::new(inputs, project, task, sheet, kept, cx))
        });
        cx.subscribe(&sheet, Self::on_direct_event).detach();
        self.show_sheet(sheets::Sheet::Direct(sheet), cx);
    }

    /// The sheet gone, closed or replaced: what it held stays on its
    /// Factory, if that still shows.
    pub(super) fn direct_gone(&mut self, sheet: &Entity<DirectView>, cx: &mut App) {
        let project = sheet.read(cx).project();
        let Some(direction) = self
            .factory()
            .filter(|_| self.factory_project() == Some(project))
            .map(|view| view.read(cx).direction.clone())
        else {
            return;
        };
        direction.update(cx, |direction, cx| {
            let sheet = sheet.read(cx);
            sheet.keep(direction.kept(sheet.task()), cx);
        });
    }

    /// The sheet up and its Task as the board has it now, with what the
    /// sheet holds: its Task, its text trimmed and its choice.
    pub(super) fn direct_send(&self, cx: &App) -> Option<(TaskId, String, Choice)> {
        let sheet = self.direct_sheet(cx)?;
        let sheet = sheet.read(cx);
        self.direct_inputs(sheet.project(), sheet.task(), cx)?;
        let (text, choice) = sheet.asked(cx);
        Some((sheet.task().clone(), text, choice))
    }
}
