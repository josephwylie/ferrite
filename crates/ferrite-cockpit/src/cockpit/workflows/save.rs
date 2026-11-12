//! Save: the next version, checked against what is installed; the note it
//! leaves.

use super::*;

impl CockpitView {
    /// Save: refused with the problems listed, else the next version.
    /// Nothing starts.
    pub(crate) fn save_workflow(&mut self, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_id() else {
            return;
        };
        let problems = self.draft_problems();
        let note = if !problems.is_empty() {
            Some(SharedString::from("Fix the problems listed before saving."))
        } else {
            let skills = self.factory_catalog().skills;
            match self.workflow_ask(WorkflowAction::Save { workflow, skills }, cx) {
                Some(Done::Version { version }) => {
                    Some(SharedString::from(format!("Saved version {version}.")))
                }
                _ => return,
            }
        };
        if let Some(view) = self.factory_view.as_mut() {
            view.note = note;
        }
        cx.notify();
    }

    /// Dismiss the view's notice.
    pub(crate) fn dismiss_factory_note(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_view.as_mut() {
            view.note = None;
        }
        cx.notify();
    }
}
