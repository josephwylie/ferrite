//! A text field of the Workflows editor, and how one is drawn: its label
//! over its Composer, ringed while it has the keyboard. The editor and
//! the start's inspector draw theirs with it.

use ferrite_core::factory_board::{StepId, WorkflowId};
use ferrite_pane::composer::Composer;
use gpui::prelude::*;
use gpui::{div, AnyElement, App, Entity, MouseButton, MouseDownEvent, SharedString, Window};

/// A text field of the editor.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    WorkflowName(WorkflowId),
    StepName(WorkflowId, StepId),
    Instructions(WorkflowId, StepId),
    /// The skills picker's search.
    SkillSearch,
    /// Agent approval's decision instructions (#222).
    ApprovalInstructions(WorkflowId),
    /// What each scheduled run's Task is told (#225).
    ScheduleInstructions(WorkflowId),
    /// A GitHub start's extra instructions for every Task.
    ExtraInstructions(WorkflowId),
}

impl Field {
    /// Instructions, written as prose in a text area that grows.
    pub fn prose(&self) -> bool {
        matches!(
            self,
            Field::Instructions(..)
                | Field::ApprovalInstructions(_)
                | Field::ScheduleInstructions(_)
                | Field::ExtraInstructions(_)
        )
    }

    /// Its selector.
    pub fn key(&self) -> String {
        match self {
            Field::WorkflowName(_) => "workflow-name".into(),
            Field::StepName(..) => "inspector-name".into(),
            Field::Instructions(..) => "inspector-instructions".into(),
            Field::SkillSearch => "skill-search".into(),
            Field::ApprovalInstructions(_) => "approval-instructions".into(),
            Field::ScheduleInstructions(_) => "start-schedule-instructions".into(),
            Field::ExtraInstructions(_) => "start-extra-instructions".into(),
        }
    }
}

/// A Workflows field drawn: its label over its Composer, ringed while it
/// has the keyboard; a press on it calls `pressed` first.
pub fn field_input(
    field: &Field,
    label: &'static str,
    composer: Entity<Composer>,
    focused: bool,
    pressed: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let key = field.key();
    div()
        .id(SharedString::from(key.clone()))
        .debug_selector(move || key.clone())
        .capture_any_mouse_down(pressed)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(if field.prose() {
            ferrite_ui::field::text_area(label.into(), composer, focused, "factory-field-focused")
        } else {
            ferrite_ui::field::text_field(label.into(), composer, focused, "factory-field-focused")
        })
        .into_any_element()
}
