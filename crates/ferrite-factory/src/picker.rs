//! The Factory's pickers and choices: what a picker's row does (`Pick`),
//! a chooser whose rows open in the window's menu, two or more choices
//! side by side, and where a picker's trigger was last drawn, so the
//! window's menu hangs under it rather than over it. A view draws the
//! probe inside its trigger and says the `Trigger` with what it asks; the
//! window reads its bounds as it opens the menu.

use std::cell::Cell;
use std::rc::Rc;

use ferrite_core::factory_board::{Approver, Destination, RouteKind, StepId, Template, WorkflowId};
use ferrite_core::provider::Provider;
use gpui::component::button::Button;
use gpui::prelude::*;
use gpui::{Bounds, ClickEvent, Context, Div, Pixels, Point};

/// What a Factory picker's row does.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Select(WorkflowId),
    Create(Template),
    Route {
        step: StepId,
        kind: RouteKind,
        to: Destination,
    },
    Provider(StepId, Provider),
    /// Empty: the Provider's default.
    Model(StepId, String),
    Effort(StepId, String),
    MoveUp(StepId),
    MoveDown(StepId),
    AddAfter(StepId),
    Delete(StepId),
    /// The New task sheet's process: one-off, or a Workflow.
    Process(Option<WorkflowId>),
    /// A choice on a Task's Rework or Direct this task sheet (#222).
    Task(crate::direct::TaskPick),
    /// Who approves the shown Workflow's results.
    Approver(Approver),
    /// The Step that decides agent approval.
    Decider(StepId),
    /// What runs once approved.
    AfterApproval(Destination),
    /// Where the Workflow sends rework.
    ReworkGoesTo(Destination),
    /// A "Tasks start" setting (#225).
    Start(crate::starts::StartPick),
    /// The Coordinator's Provider, model or effort (#224).
    Coordinator(CoordinatorPick),
}

/// A choice on the Coordinator's settings.
#[derive(Clone, Debug, PartialEq)]
pub enum CoordinatorPick {
    Provider(Provider),
    /// Empty: the Provider's default.
    Model(String),
    Effort(String),
}

/// Where a picker's trigger was last drawn, so its menu hangs under it
/// rather than over it.
#[derive(Clone, Default)]
pub struct Trigger(Rc<Cell<Option<Bounds<Pixels>>>>);

impl Trigger {
    /// The trigger's child that records where it is drawn; the trigger is
    /// `relative`, so this fills it.
    pub fn probe(&self) -> impl IntoElement {
        let drawn = self.0.clone();
        gpui::canvas(move |bounds, _, _| drawn.set(Some(bounds)), |_, _, _, _| {})
            .absolute()
            .size_full()
    }

    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.0.get()
    }
}

/// The chooser face for any view, `press` given where its rows go: the
/// trigger it records itself in and the pointer.
pub fn picker_button<V: 'static>(
    selector: &'static str,
    text: String,
    flagged: bool,
    press: impl Fn(&mut V, Trigger, Point<Pixels>, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> Button {
    let look = ferrite_ui::controls::Look {
        flagged,
        full_width: true,
        disabled: false,
    };
    let trigger = Trigger::default();
    ferrite_ui::controls::chooser(selector, "", text.into(), look, cx)
        .debug_selector(move || {
            if flagged {
                format!("{selector}-flagged")
            } else {
                selector.to_string()
            }
        })
        .relative()
        .child(trigger.probe())
        .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
            cx.stop_propagation();
            press(view, trigger.clone(), event.position(), cx);
        }))
}

/// Two or more choices side by side, full width: the shared segments, the
/// chosen one selected as `{selector}-{at}-on`.
pub fn segmented<V: 'static, T: Copy + PartialEq + 'static>(
    selector: &'static str,
    chosen: T,
    options: &[(T, &'static str)],
    pick: impl Fn(&mut V, T, &mut Context<V>) + Clone + 'static,
    cx: &mut Context<V>,
) -> Div {
    let mut track =
        ferrite_ui::controls::segments(true).debug_selector(move || selector.to_string());
    for (at, (value, word)) in options.iter().enumerate() {
        let value = *value;
        let on = value == chosen;
        let key = format!("{selector}-{at}{}", if on { "-on" } else { "" });
        let pick = pick.clone();
        track = track.child(
            ferrite_ui::controls::segment((selector, at), *word, on, true, cx)
                .debug_selector(move || key.clone())
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| pick(view, value, cx))),
        );
    }
    track
}
