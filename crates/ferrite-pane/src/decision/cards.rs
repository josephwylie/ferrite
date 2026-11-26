//! The Decision cards a Pane draws: on its plane over the transcript's
//! foot, or as the transcript's last rows. They draw from `Cards`, a
//! snapshot the window takes (the requests, their owner's provider and
//! context, the Pane's forms), and say each press to `DecisionsView` as a
//! `DecisionEvent` the window hears, so the cell and the transcript draw
//! them as values, never through a draw the window hands them.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use ferrite_core::activity::{DecisionHandle, PendingDecision};
use ferrite_core::provider::Provider;
use ferrite_core::{DecisionAnswer, ThreadKey};
use ferrite_ui::components::MenuButton as _;
use ferrite_ui::theme;
use gpui::component::{
    checkbox::Checkbox,
    input::{Input, InputState},
    radio::{Radio, RadioGroup},
    scroll::ScrollableElement,
    Disableable,
};
use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, Div, Entity, KeyDownEvent, ScrollHandle, SharedString, Window};

use super::forms::{digit_question, pick_option, RequestForm, RequestForms};
use super::{PickOption1, PickOption2, PickOption3, PickOption4};
use crate::composer::{Interrupt, Submit};
use super::view::{DecisionEvent, DecisionsView};
use crate::decision;
use crate::pane;

mod approval;
mod form;
mod question;
#[cfg(test)]
mod tests;

/// The keys a card's own controls keep: ⏎, ⇥ and ⇧⇥ stay with them rather
/// than the Composer's shortcuts.
pub fn init(cx: &mut gpui::App) {
    // Keep Composer shortcuts outside toolkit controls. Their low-level
    // Enter/Space activation and focus traversal continue through GPUI.
    cx.bind_keys(
        ["enter", "tab", "shift-tab"]
            .map(|key| gpui::KeyBinding::new(key, gpui::NoAction {}, Some("SubjectControls"))),
    );
}

fn native_keys<E: gpui::InteractiveElement>(element: E) -> E {
    element
        .key_context("SubjectControls")
        .on_key_down(|event, window, cx| {
            if event.keystroke.key == "tab" {
                if event.keystroke.modifiers.shift {
                    window.focus_prev(cx);
                } else {
                    window.focus_next(cx);
                }
                cx.stop_propagation();
                window.prevent_default();
            }
        })
}

/// Every request card's frame. In the transcript the card is the row
/// itself, on the transcript's own column; in the requests overlay it lies
/// on the plane, inset on the transcript's axes, so the rows it docks over
/// do not show through. `narrow` is the compact inspector's column
/// (`Cards::narrow`).
fn request_frame(card: impl IntoElement, overlay: bool, narrow: bool, inset: f32) -> Div {
    native_keys(
        div()
            .w_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(theme::DECISION_PILL_GAP))
            .overflow_hidden()
            // The card's ring hangs `CARD_OUTSET` past the transcript
            // column, as every transcript card's does (one left edge).
            .when(overlay, |frame| {
                frame
                    .bg(theme::paint::CARD)
                    // The transcript column's inset (`Cards::inset`).
                    .pl(px(inset - theme::CARD_OUTSET))
                    .pr(px(inset))
                    .py(px(theme::DECISION_PAD_Y))
            })
            .when(!overlay, |frame| frame.ml(px(-theme::CARD_OUTSET)))
            // Every card waits on the operator: Claude's pill says so over
            // it, on the column, the one place its state takes a hue. In the
            // compact inspector the room is the card's.
            .when(!narrow && !overlay, |frame| {
                frame.child(decision::needs_input().ml(px(theme::CARD_OUTSET)))
            })
            .child(card),
    )
}

/// What a Pane's Decision cards are drawn from: a snapshot of the cockpit
/// taken when they are asked for, so the transcript — a cached view — draws
/// them without reading the cockpit (a read would re-render the transcript
/// on every cockpit change). Every press goes back through `owner`, as a
/// `DecisionEvent` the window hears.
#[derive(Clone)]
pub struct Cards {
    pub owner: gpui::WeakEntity<DecisionsView>,
    /// The diff each approval's gated call names, by the call's id.
    pub asked: Vec<(String, ferrite_core::transcript::Diff)>,
    pub thread: ThreadKey,
    pub provider: Option<Provider>,
    /// The head's last detail: Codex's sandbox, Claude's permission mode.
    pub context: Option<SharedString>,
    /// The mode an approval's `Allow and switch to …` moves the Session to
    /// (its value and label), with the Session generation it was read on;
    /// None where the Session cannot switch or is there already.
    pub switch: Option<(String, SharedString, u64)>,
    pub workspace: Option<std::path::PathBuf>,
    /// The transcript's reading size: an edit approval's diff is drawn at it.
    pub reading: ferrite_core::settings::ReadingSize,
    pub rich: crate::rich::TextCache,
    pub forms: RequestForms,
    pub request_error: Option<(DecisionHandle, String)>,
    /// The overlay's Pane is short: a question body keeps two rows.
    pub short: bool,
    /// Drawn in the requests overlay, on the plane, rather than in the
    /// transcript.
    pub overlay: bool,
    /// Drawn in the compact inspector's narrow column: the approval's
    /// actions stack, and no pill names the wait.
    pub narrow: bool,
    /// The overlay's inset on the transcript's axes: the column's at the
    /// Pane's width (`theme::tx_inset_at`).
    pub inset: f32,
    /// The requests shown, each with whether its agent works on meanwhile.
    pub requests: Vec<(PendingDecision, bool)>,
    /// The Pane whose transcript's foot the cards stand at: they keep
    /// where they are drawn, and their buttons, in its geometry
    /// (`toast_home`).
    pub kept_in: Option<Rc<Cell<crate::pane::PaneGeometry>>>,
}

/// Ask the cockpit to draw again (a pick that lives in the Pane's forms):
/// the window redraws with the Decisions' view.
fn refresh(owner: &gpui::WeakEntity<DecisionsView>, cx: &mut gpui::App) {
    let _ = owner.update(cx, |_, cx| cx.notify());
}

/// Tell the window what a press asks.
fn ask(
    owner: &gpui::WeakEntity<DecisionsView>,
    event: DecisionEvent,
    cx: &mut gpui::App,
) {
    let _ = owner.update(cx, |_, cx| cx.emit(event));
}

impl Cards {
    /// Everything the drawn cards depend on, for the transcript's cache
    /// key: the requests and their sending state, the cursors and notes,
    /// the picks, the errors and the head's words.
    pub fn key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.thread.hash(&mut hasher);
        self.provider
            .map(ferrite_ui::provider::word)
            .hash(&mut hasher);
        self.context.as_deref().hash(&mut hasher);
        self.switch.hash(&mut hasher);
        self.reading.hash(&mut hasher);
        for (id, diff) in &self.asked {
            id.hash(&mut hasher);
            diff.path.hash(&mut hasher);
            (diff.added, diff.removed).hash(&mut hasher);
        }
        self.request_error.hash(&mut hasher);
        self.narrow.hash(&mut hasher);
        let cursors = self.forms.1.borrow();
        let forms = self.forms.0.borrow();
        for (request, working) in &self.requests {
            request.handle.hash(&mut hasher);
            request.submitting.hash(&mut hasher);
            request.reply_error.hash(&mut hasher);
            working.hash(&mut hasher);
            if let Some(cursor) = cursors.get(&request.handle) {
                cursor.index.hash(&mut hasher);
                cursor.menu.hash(&mut hasher);
                cursor
                    .note
                    .as_ref()
                    .map(|note| note.entity_id())
                    .hash(&mut hasher);
            }
            if let Some(form) = forms.get(&request.handle) {
                for answer in &form.answers {
                    answer.picks.hash(&mut hasher);
                }
                serde_json::Value::Object(form.values.clone())
                    .to_string()
                    .hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    /// A request's card in its frame (`request_frame`). At a transcript's
    /// foot the card keeps where it is drawn (`kept_in`).
    fn frame(&self, card: AnyElement, overlay: bool) -> Div {
        let frame = request_frame(card, overlay, self.narrow, self.inset);
        let Some(geometry) = self.kept_in.clone() else {
            return frame;
        };
        // The card is the frame's last child, under the pill that names the
        // wait.
        frame.on_children_prepainted(move |children, _, _| {
            if let Some(&card) = children.last() {
                let kept = geometry.get();
                let answers = kept.answers.with_cards(card);
                geometry.set(crate::pane::PaneGeometry { answers, ..kept });
            }
        })
    }

    /// A press that asks the window for `event`.
    fn act<E: 'static>(
        &self,
        event: impl Fn() -> DecisionEvent + 'static,
    ) -> impl Fn(&E, &mut Window, &mut gpui::App) + 'static {
        let owner = self.owner.clone();
        move |_, _, cx| ask(&owner, event(), cx)
    }

    /// The cards, stacked: in the transcript a blank row apart, in the
    /// overlay flush and scrolling together when there are several.
    pub fn render(&self, window: &mut Window, cx: &mut gpui::App) -> Option<AnyElement> {
        if self.requests.is_empty() {
            return None;
        }
        let mut stack = div()
            .id((
                if self.overlay {
                    "subject-requests"
                } else {
                    "decision-tail"
                },
                self.thread.thread().get(),
            ))
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .min_h_0()
            .when(self.overlay, |stack| stack.max_h_full());
        for (at, (request, working)) in self.requests.iter().enumerate() {
            let card = self.request_card(request.clone(), *working, window, cx);
            stack = stack.child(if at > 0 && !self.overlay {
                div()
                    .w_full()
                    .min_w_0()
                    .mt(px(theme::ROW))
                    .child(card)
                    .into_any_element()
            } else {
                card
            });
        }
        Some(if self.overlay && self.requests.len() > 1 {
            native_keys(stack.overflow_y_scrollbar()).into_any_element()
        } else {
            // A single request owns its own bounded content viewport. Giving
            // its surrounding stack another scroll container makes that
            // stack consume the transcript's flex space instead of docking.
            native_keys(stack).into_any_element()
        })
    }

    fn request_card(
        &self,
        request: PendingDecision,
        working: bool,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> AnyElement {
        // Keep this dispatcher shallow. Windows gives the GUI main thread a
        // 1 MiB stack; compiling every Decision builder into this one frame
        // overflowed it as soon as a question arrived.
        if let Some(questions) = pane::questions_of(&request.decision) {
            let questions = questions.to_vec();
            return self.question_card(request, questions, working, window, cx);
        }
        match &request.decision.kind {
            ferrite_core::DecisionKind::Form { .. } => self.form_card(request, window, cx),
            ferrite_core::DecisionKind::External { .. }
            | ferrite_core::DecisionKind::Unsupported { .. } => self.link_card(request, cx),
            _ => self.approval_card(request, cx),
        }
    }

    /// The head's details: the provider, an approval's context, and
    /// `agent unknown` when the provider named no owner.
    fn detail(&self, tool: Option<&str>, context: bool, unowned: bool) -> Option<SharedString> {
        decision::head_detail([
            tool.unwrap_or_default(),
            self.provider
                .map(ferrite_ui::provider::word)
                .unwrap_or_default(),
            if context {
                self.context.as_deref().unwrap_or_default()
            } else {
                ""
            },
            if unowned { "agent unknown" } else { "" },
        ])
    }

    /// The head, the prose and the send error every non-question card
    /// shares (`request-title-*`).
    fn preamble(&self, request: &PendingDecision) -> (Div, Option<Div>, Option<SharedString>) {
        let thread = self.thread;
        let decision = &request.decision;
        // A status word rides the head only when it adds something — a
        // card is waiting by being there.
        let status = request
            .submitting
            .then(|| decision::sending().into_any_element());
        let unowned = request.subject.is_none();
        let head = if matches!(decision.kind, ferrite_core::DecisionKind::Approval) {
            // The ask says who and what; the mode lives in the Composer's
            // footer, so the head names only an owner it could not find.
            decision::head(
                decision::approval_lead(decision, self.provider),
                unowned.then(|| SharedString::from("agent unknown")),
                status,
            )
        } else {
            decision::head(
                decision::kind_word(decision),
                self.detail(
                    Some(decision::tool_word(&decision.tool_name)),
                    false,
                    unowned,
                ),
                status,
            )
        };
        let title = if decision.description.is_empty() && decision.tool_name.is_empty() {
            Some(SharedString::from(
                "The provider sent a request Ferrite could not read.",
            ))
        } else {
            // The subject is printed once: prose that only repeats the
            // command in the band below goes.
            let command = decision::shell_source(decision)
                .map(std::borrow::Cow::into_owned)
                .or_else(|| pane::approval_source(decision));
            if matches!(decision.kind, ferrite_core::DecisionKind::Approval) {
                decision::approval_prose(decision, command.as_deref())
            } else {
                decision::prose_beyond_subject(decision, command.as_deref())
            }
            .map(SharedString::from)
        };
        let serial = request.handle.serial;
        let title = title.map(|title| {
            decision::prose(title)
                .debug_selector(move || format!("request-title-{}-{serial}", thread.thread().get()))
        });
        let error = request
            .reply_error
            .clone()
            .map(SharedString::from)
            .or_else(|| {
                self.request_error
                    .as_ref()
                    .filter(|(failed, _)| failed == &request.handle)
                    .map(|(_, error)| SharedString::from(error.clone()))
            });
        (head, title, error)
    }
}
