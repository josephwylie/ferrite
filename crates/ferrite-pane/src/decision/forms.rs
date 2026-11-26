//! A Pane's Decision state, shared with the cards it draws: each question's
//! or MCP form's answers and fields, each Decision's cursor, its amend
//! note and its split button's menu.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ferrite_core::activity::DecisionHandle;
use gpui::component::input::InputState;
use gpui::{Entity, FocusHandle, Focusable, ScrollHandle};

/// A Pane's Decision state, shared with the cards it draws: each question
/// or MCP form's answers, and each Decision's cursor and amend note.
#[derive(Clone, Default)]
pub struct RequestForms(
    pub(super) Rc<RefCell<HashMap<DecisionHandle, RequestForm>>>,
    pub(super) Rc<RefCell<HashMap<DecisionHandle, DecisionCursor>>>,
);

/// One Decision's keyboard state (D-5, D-6): the option under the `❯`
/// once the operator moved it (the first option that can act until then),
/// and the amend note while it is open.
#[derive(Default)]
pub struct DecisionCursor {
    pub index: Option<usize>,
    pub note: Option<Entity<crate::composer::Composer>>,
    /// The approval's split-button menu is open.
    pub menu: bool,
}

impl RequestForms {
    /// Where `handle`'s cursor was moved, if it was.
    pub fn cursor(&self, handle: &DecisionHandle) -> Option<usize> {
        self.1.borrow().get(handle).and_then(|cursor| cursor.index)
    }

    pub fn set_cursor(&self, handle: &DecisionHandle, index: Option<usize>) {
        self.1.borrow_mut().entry(handle.clone()).or_default().index = index;
    }

    /// Whether `handle`'s split-button menu is open.
    pub fn menu(&self, handle: &DecisionHandle) -> bool {
        self.1
            .borrow()
            .get(handle)
            .is_some_and(|cursor| cursor.menu)
    }

    pub fn set_menu(&self, handle: &DecisionHandle, open: bool) {
        self.1.borrow_mut().entry(handle.clone()).or_default().menu = open;
    }

    /// Close every Decision's split-button menu.
    pub fn close_menus(&self) {
        for cursor in self.1.borrow_mut().values_mut() {
            cursor.menu = false;
        }
    }

    /// `handle`'s amend note, while it is open.
    pub fn note(
        &self,
        handle: &DecisionHandle,
    ) -> Option<Entity<crate::composer::Composer>> {
        self.1
            .borrow()
            .get(handle)
            .and_then(|cursor| cursor.note.clone())
    }

    pub fn set_note(
        &self,
        handle: &DecisionHandle,
        note: Option<Entity<crate::composer::Composer>>,
    ) {
        self.1.borrow_mut().entry(handle.clone()).or_default().note = note;
    }

    /// A question form's answers so far, once its card has drawn.
    pub fn answers(
        &self,
        handle: &DecisionHandle,
    ) -> Option<Vec<ferrite_core::questions::Answer>> {
        self.0.borrow().get(handle).map(|form| form.answers.clone())
    }

    /// `handle`'s amend note's words, taken: the note closes.
    pub fn take_note(&self, handle: &DecisionHandle, cx: &mut gpui::App) -> Option<String> {
        let note = self.note(handle)?;
        self.set_note(handle, None);
        Some(note.update(cx, |line, cx| line.take(cx)))
    }

    /// The Decisions `keep` refuses take what their cards kept with them.
    pub fn retain(&self, keep: impl Fn(&DecisionHandle) -> bool) {
        self.0.borrow_mut().retain(|handle, _| keep(handle));
        self.1.borrow_mut().retain(|handle, _| keep(handle));
    }

    /// A question form's answers as they stand, each with the words typed
    /// under it; None while its card has not drawn.
    pub fn question_answers(
        &self,
        handle: &DecisionHandle,
        cx: &gpui::App,
    ) -> Option<Vec<ferrite_core::questions::Answer>> {
        let mut state = self.0.borrow_mut();
        let form = state.get_mut(handle)?;
        for (answer, input) in form.answers.iter_mut().zip(&form.inputs) {
            answer.other =
                Some(input.read(cx).value().to_string()).filter(|text| !text.trim().is_empty());
        }
        Some(form.answers.clone())
    }

    /// An MCP form's values as they stand, its typed fields read in; None
    /// while its card has not drawn.
    pub fn form_values(
        &self,
        handle: &DecisionHandle,
        fields: &[ferrite_core::FormField],
        cx: &gpui::App,
    ) -> Option<serde_json::Value> {
        let mut state = self.0.borrow_mut();
        let form = state.get_mut(handle)?;
        for field in fields {
            let Some(input) = form.form_inputs.get(&field.id) else {
                continue;
            };
            let text = input.read(cx).value().to_string();
            if text.trim().is_empty() {
                if field.required {
                    form.values
                        .insert(field.id.clone(), serde_json::Value::Null);
                } else {
                    form.values.remove(&field.id);
                }
                continue;
            }
            let value = match &field.kind {
                ferrite_core::FormFieldKind::String { .. } => serde_json::Value::String(text),
                ferrite_core::FormFieldKind::Number { .. } => match text
                    .parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                {
                    Some(value) => serde_json::Value::Number(value),
                    None => serde_json::Value::String(text),
                },
                ferrite_core::FormFieldKind::Integer { .. } => match text.parse::<i64>() {
                    Ok(value) => serde_json::Value::from(value),
                    Err(_) => serde_json::Value::String(text),
                },
                _ => continue,
            };
            form.values.insert(field.id.clone(), value);
        }
        Some(serde_json::Value::Object(form.values.clone()))
    }

    /// Digit `n` (0-based) on `handle`'s question form: option n of its
    /// digit question, or its words field one past its options.
    pub fn pick(
        &self,
        handle: &DecisionHandle,
        questions: &[ferrite_core::questions::Question],
        n: usize,
        cx: &gpui::App,
    ) -> Pick {
        let mut state = self.0.borrow_mut();
        // The card builds its form on first paint; a question never drawn
        // (a compact cell's expander) has nothing to pick in yet.
        let Some(form) = state.get_mut(handle) else {
            return Pick::Untaken;
        };
        let Some(qi) = digit_question(&form.answers, questions) else {
            return Pick::Untaken;
        };
        let question = &questions[qi];
        if n < question.options.len() {
            // One key answers a lone single-select question outright.
            if questions.len() == 1
                && !question.multi_select
                && form.inputs[qi].read(cx).value().trim().is_empty()
            {
                return Pick::Answers;
            }
            pick_option(&mut form.answers[qi].picks, n, question.multi_select);
            drop(state);
            self.set_cursor(handle, Some(n));
            Pick::Picked
        } else if question.allow_other && n == question.options.len() {
            Pick::Words(form.inputs[qi].read(cx).focus_handle(cx))
        } else {
            Pick::Untaken
        }
    }
}

/// Whether every question has a pick or typed words.
pub fn answered(answers: &[ferrite_core::questions::Answer]) -> bool {
    answers
        .iter()
        .all(|answer| !answer.picks.is_empty() || answer.other.is_some())
}

/// What a digit did on a question form.
pub enum Pick {
    /// Nothing here takes it: it types.
    Untaken,
    /// It answers the lone single-select question as it is.
    Answers,
    /// It picked (or for a multi-select toggled) the option.
    Picked,
    /// It is the words field's, which takes the keyboard.
    Words(FocusHandle),
}

/// A question's or MCP form's answers as they stand, and the fields
/// they are typed in.
pub(super) struct RequestForm {
    pub(super) answers: Vec<ferrite_core::questions::Answer>,
    pub(super) inputs: Vec<Entity<InputState>>,
    pub(super) form_inputs: HashMap<String, Entity<InputState>>,
    pub(super) values: serde_json::Map<String, serde_json::Value>,
    pub(super) scroll: ScrollHandle,
}

/// The question digit keys pick in: the first with options and no pick
/// yet, else the first with options (a multi-select keeps toggling).
pub fn digit_question(
    answers: &[ferrite_core::questions::Answer],
    questions: &[ferrite_core::questions::Question],
) -> Option<usize> {
    let with_options = |at: &usize| !questions[*at].options.is_empty();
    (0..questions.len())
        .filter(with_options)
        .find(|at| {
            answers
                .get(*at)
                .is_some_and(|answer| answer.picks.is_empty())
        })
        .or_else(|| (0..questions.len()).find(with_options))
}

/// A pick: single-select replaces, multi-select toggles.
pub(super) fn pick_option(picks: &mut Vec<usize>, option: usize, multi: bool) {
    if !multi {
        *picks = vec![option];
    } else if picks.contains(&option) {
        picks.retain(|at| *at != option);
    } else {
        picks.push(option);
    }
}
