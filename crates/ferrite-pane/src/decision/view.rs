//! `DecisionsView` (ADR 0041) keeps Ferrite's own thread rules and each
//! Pane's `RequestForms` (the cursor, the amend note, the split button's
//! menu, a question's or form's answers), answers the Decision keys, and
//! is what a card presses through: each answer it asks for it says as a
//! `DecisionEvent`, which the window hears and tells the Server.

use std::collections::HashMap;

use ferrite_core::activity::{DecisionHandle, PendingDecision};
use ferrite_core::roster::PaneIdentity;
use ferrite_core::{DecisionAnswer, ThreadKey};
use gpui::prelude::*;
use gpui::{App, Context, EventEmitter, FocusHandle, Focusable, Window};

use super::forms::{answered, digit_question, Pick, RequestForms};
use super::{ApprovalChoice, ThreadRule};
use crate::decision;
use crate::pane;

/// What a Decision card's press asks of the window.
pub enum DecisionEvent {
    /// An approval's option, with the note if one is open; `switch` then
    /// moves the Session to that permission mode, read on that Session
    /// generation (`Allow and switch to …`).
    Approve {
        thread: ThreadKey,
        request: PendingDecision,
        choice: ApprovalChoice,
        switch: Option<(String, u64)>,
    },
    /// This answer, as it stands: a Cancel, a Skip, a Complete.
    Respond {
        thread: ThreadKey,
        handle: DecisionHandle,
        answer: DecisionAnswer,
    },
    /// A question's Send.
    SendQuestions {
        thread: ThreadKey,
        handle: DecisionHandle,
    },
    /// An MCP form's Send.
    SendForm {
        thread: ThreadKey,
        handle: DecisionHandle,
        fields: Vec<ferrite_core::FormField>,
    },
    /// A question's option was picked: its error goes.
    Picked {
        thread: ThreadKey,
        handle: DecisionHandle,
    },
    /// ⏎ in the amend note.
    SendNote {
        thread: ThreadKey,
        handle: DecisionHandle,
    },
    /// esc in the amend note.
    CloseNote {
        thread: ThreadKey,
        handle: DecisionHandle,
    },
    /// ⏎ or a digit answered a question: these answers go, and the note
    /// taken with them becomes the Thread's next prompt once they land.
    Answered {
        thread: ThreadKey,
        handle: DecisionHandle,
        answers: Vec<ferrite_core::questions::Answer>,
        note: Option<String>,
    },
}

/// A Decision key, as the root's element hears it.
#[derive(Clone, Copy)]
pub enum DecisionKey {
    /// ↑↓: the cursor moves this many rows.
    Step(isize),
    /// ⏎
    Confirm,
    /// esc
    Dismiss,
    /// ⇥
    Amend,
    /// `1`–`4`, 0-based.
    Pick(usize),
}

/// What a Decision key acts on, read off the root: a request its Pane
/// shows, whether each of an approval's options can act, and where the
/// keyboard goes as the note closes.
pub struct KeyTarget {
    pub pane: PaneIdentity,
    pub thread: ThreadKey,
    pub request: PendingDecision,
    /// An approval's options, each whether it can act; empty for any
    /// other kind.
    pub approval_rows: Vec<bool>,
    /// The Composer on Main, else the transcript.
    pub back: FocusHandle,
}

/// The thread rules option 2 made where the provider offered no standing
/// answer, each Pane's Decision forms, and the cards' way back to the
/// window.
#[derive(Default)]
pub struct DecisionsView {
    rules: Vec<ThreadRule>,
    /// By Pane, made with it (`open_pane`), so a card always has its
    /// Pane's to draw from.
    forms: HashMap<PaneIdentity, RequestForms>,
}

impl EventEmitter<DecisionEvent> for DecisionsView {}

impl DecisionsView {
    /// Keep `rule`: what its pattern covers in its Thread is allowed from
    /// now on.
    pub fn keep(&mut self, rule: ThreadRule) {
        if !self.rules.contains(&rule) {
            self.rules.push(rule);
        }
    }

    /// Whether any rule covers a call in `thread`.
    pub fn covers_any(&self, thread: ThreadKey) -> bool {
        self.rules.iter().any(|rule| rule.thread == thread)
    }

    /// Whether a rule covers `decision` in `thread`.
    pub fn covers(
        &self,
        thread: ThreadKey,
        decision: &ferrite_core::Decision,
        workspace: Option<&std::path::Path>,
    ) -> bool {
        self.rules
            .iter()
            .any(|rule| rule.covers(thread, decision, workspace))
    }

    /// A gone Thread's rules go with it.
    pub fn retain_threads(&mut self, keep: impl Fn(ThreadKey) -> bool) {
        self.rules.retain(|rule| keep(rule.thread));
    }

    /// The thread rules kept.
    pub fn rules(&self) -> &[ThreadRule] {
        &self.rules
    }

    /// Pane `pane` was made: its forms are made with it.
    pub fn open_pane(&mut self, pane: PaneIdentity) {
        self.forms.entry(pane).or_default();
    }

    /// Every Pane `keep` refuses takes its forms with it.
    pub fn retain_panes(&mut self, keep: impl Fn(PaneIdentity) -> bool) {
        self.forms.retain(|pane, _| keep(*pane));
    }

    /// A draft's first send made it `thread`'s Pane: it keeps its forms.
    pub fn adopt(&mut self, draft: PaneIdentity, thread: PaneIdentity) {
        if let Some(forms) = self.forms.remove(&draft) {
            self.forms.insert(thread, forms);
        }
    }

    /// What `pane`'s cards draw from and write.
    pub fn forms(&self, pane: PaneIdentity) -> Option<&RequestForms> {
        self.forms.get(&pane)
    }

    /// Close every Pane's Decision menus.
    pub fn close_menus(&mut self, cx: &mut Context<Self>) {
        self.forms.values().for_each(RequestForms::close_menus);
        cx.notify();
    }

    /// `handle` was answered: what its card kept goes.
    pub fn forget(&mut self, pane: PaneIdentity, handle: &DecisionHandle) {
        if let Some(forms) = self.forms.get(&pane) {
            forms.retain(|kept| kept != handle);
        }
    }

    /// The open note's words, taken (the note closes).
    pub fn take_note(
        &mut self,
        pane: PaneIdentity,
        handle: &DecisionHandle,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.forms.get(&pane)?.take_note(handle, cx)?;
        cx.notify();
        Some(text)
    }

    /// The open note on `handle`, which holds the keyboard while it is open.
    pub fn note_focus(
        &self,
        pane: PaneIdentity,
        handle: &DecisionHandle,
        cx: &App,
    ) -> Option<FocusHandle> {
        Some(self.forms.get(&pane)?.note(handle)?.focus_handle(cx))
    }

    /// esc in the note: the split button's open menu closes first, else
    /// the note closes and the keyboard goes `back` (a second esc denies).
    /// True when either was open.
    pub fn close_note(
        &mut self,
        pane: PaneIdentity,
        handle: &DecisionHandle,
        back: &FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(forms) = self.forms.get(&pane).cloned() else {
            return false;
        };
        close_menu(&forms, handle, cx) || close_note(&forms, handle, back, window, cx)
    }

    /// `key` on `target`; false when nothing here takes it, so it goes on
    /// to its other bindings (a digit types).
    pub fn key(
        &mut self,
        key: DecisionKey,
        target: &KeyTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(forms) = self.forms.get(&target.pane).cloned() else {
            return false;
        };
        match key {
            DecisionKey::Step(step) => {
                let rows = cursor_rows(&forms, target);
                if rows.is_empty() {
                    return false;
                }
                let handle = &target.request.handle;
                let next = decision::step_cursor(&rows, cursor_at(&forms, handle, &rows), step);
                forms.set_cursor(handle, Some(next));
                cx.notify();
                true
            }
            DecisionKey::Confirm => confirm(&forms, target, window, cx),
            DecisionKey::Dismiss => dismiss(&forms, target, window, cx),
            DecisionKey::Amend => amend(&forms, target, window, cx),
            DecisionKey::Pick(n) => pick(&forms, target, n, window, cx),
        }
    }
}

/// Which rows the cursor walks on `target`'s request: an approval's three
/// options, each whether it can act, or the options of the question its
/// digits pick in.
fn cursor_rows(forms: &RequestForms, target: &KeyTarget) -> Vec<bool> {
    let request = &target.request;
    let Some(questions) = pane::questions_of(&request.decision) else {
        return target.approval_rows.clone();
    };
    let answers = forms
        .answers(&request.handle)
        .unwrap_or_else(|| vec![Default::default(); questions.len()]);
    digit_question(&answers, questions)
        .map(|qi| vec![true; questions[qi].options.len()])
        .unwrap_or_default()
}

/// The row under `handle`'s cursor: where it was moved, else the first
/// option that can act.
fn cursor_at(forms: &RequestForms, handle: &DecisionHandle, rows: &[bool]) -> usize {
    forms
        .cursor(handle)
        .filter(|at| *at < rows.len())
        .unwrap_or_else(|| rows.iter().position(|enabled| *enabled).unwrap_or(0))
}

fn approve(target: &KeyTarget, choice: ApprovalChoice) -> DecisionEvent {
    DecisionEvent::Approve {
        thread: target.thread,
        request: target.request.clone(),
        choice,
        switch: None,
    }
}

/// ⏎: the row under the cursor. An approval answers that option; a
/// question sends when every question is answered, else picks the row (a
/// lone single-select question then sends). The open note rides the
/// answer. False when nothing here takes ⏎.
fn confirm(
    forms: &RequestForms,
    target: &KeyTarget,
    window: &mut Window,
    cx: &mut Context<DecisionsView>,
) -> bool {
    let request = &target.request;
    if request.submitting {
        return true;
    }
    let rows = cursor_rows(forms, target);
    let at = cursor_at(forms, &request.handle, &rows);
    if let Some(questions) = pane::questions_of(&request.decision) {
        let note = forms.take_note(&request.handle, cx);
        match forms
            .question_answers(&request.handle, cx)
            .filter(|answers| answered(answers))
        {
            Some(answers) => cx.emit(DecisionEvent::Answered {
                thread: target.thread,
                handle: request.handle.clone(),
                answers,
                note,
            }),
            None if !rows.is_empty() => {
                pick_question(forms, target, questions, at, note, window, cx);
            }
            None => {}
        }
        cx.notify();
        return true;
    }
    let Some(choice) = ApprovalChoice::from_index(at)
        .filter(|_| matches!(request.decision.kind, ferrite_core::DecisionKind::Approval))
    else {
        return false;
    };
    if rows.get(at).copied().unwrap_or(false) {
        cx.emit(approve(target, choice));
    }
    true
}

/// esc: the split button's open menu closes, else an open note closes;
/// otherwise an approval is denied (option 3) and a question dismissed.
fn dismiss(
    forms: &RequestForms,
    target: &KeyTarget,
    window: &mut Window,
    cx: &mut Context<DecisionsView>,
) -> bool {
    let request = &target.request;
    if close_menu(forms, &request.handle, cx)
        || close_note(forms, &request.handle, &target.back, window, cx)
        || request.submitting
    {
        return true;
    }
    let respond = |answer| DecisionEvent::Respond {
        thread: target.thread,
        handle: request.handle.clone(),
        answer,
    };
    let event = if pane::questions_of(&request.decision).is_some() {
        respond(DecisionAnswer::Deny {
            message: decision::QUESTION_SKIPPED.into(),
        })
    } else {
        match request.decision.kind {
            _ if !request.decision.policy.deny => return false,
            ferrite_core::DecisionKind::Approval => approve(target, ApprovalChoice::DenyAndSteer),
            _ => respond(DecisionAnswer::Cancel),
        }
    };
    cx.emit(event);
    true
}

/// ⇥: the note under the options opens and takes the keyboard.
fn amend(
    forms: &RequestForms,
    target: &KeyTarget,
    window: &mut Window,
    cx: &mut Context<DecisionsView>,
) -> bool {
    let request = &target.request;
    let amendable = pane::questions_of(&request.decision).is_some()
        || matches!(request.decision.kind, ferrite_core::DecisionKind::Approval);
    if !amendable || request.submitting {
        return false;
    }
    let note = forms.note(&request.handle).unwrap_or_else(|| {
        let note = cx.new(crate::composer::Composer::new);
        forms.set_note(&request.handle, Some(note.clone()));
        note
    });
    window.focus(&note.focus_handle(cx), cx);
    cx.notify();
    true
}

/// Digit `n` (0-based) on `target`'s request: an approval answers option
/// n (`1` allow, `2` allow for this thread, `3` deny and steer); a
/// question picks option n of its digit question, or focuses the words
/// field one past its options. False when nothing here takes the digit,
/// so it types.
fn pick(
    forms: &RequestForms,
    target: &KeyTarget,
    n: usize,
    window: &mut Window,
    cx: &mut Context<DecisionsView>,
) -> bool {
    let request = &target.request;
    if request.submitting {
        return true;
    }
    if let Some(questions) = pane::questions_of(&request.decision) {
        return pick_question(forms, target, questions, n, None, window, cx);
    }
    if !matches!(request.decision.kind, ferrite_core::DecisionKind::Approval) {
        return false;
    }
    let Some(choice) = ApprovalChoice::from_index(n) else {
        return false;
    };
    if !target.approval_rows.get(n).copied().unwrap_or(false) {
        return false;
    }
    cx.emit(approve(target, choice));
    true
}

/// Option `n` of `target`'s question: a lone single-select question with
/// no typed words is answered at once (one key), with `note`; otherwise
/// the option is picked.
fn pick_question(
    forms: &RequestForms,
    target: &KeyTarget,
    questions: &[ferrite_core::questions::Question],
    n: usize,
    note: Option<String>,
    window: &mut Window,
    cx: &mut Context<DecisionsView>,
) -> bool {
    let handle = target.request.handle.clone();
    match forms.pick(&handle, questions, n, cx) {
        Pick::Untaken => false,
        Pick::Answers => {
            cx.emit(DecisionEvent::Answered {
                thread: target.thread,
                handle,
                answers: vec![ferrite_core::questions::Answer {
                    picks: vec![n],
                    other: None,
                }],
                note,
            });
            true
        }
        Pick::Picked => {
            cx.emit(DecisionEvent::Picked {
                thread: target.thread,
                handle,
            });
            cx.notify();
            true
        }
        Pick::Words(focus) => {
            window.focus(&focus, cx);
            true
        }
    }
}

fn close_menu(
    forms: &RequestForms,
    handle: &DecisionHandle,
    cx: &mut Context<DecisionsView>,
) -> bool {
    if !forms.menu(handle) {
        return false;
    }
    forms.set_menu(handle, false);
    cx.notify();
    true
}

fn close_note(
    forms: &RequestForms,
    handle: &DecisionHandle,
    back: &FocusHandle,
    window: &mut Window,
    cx: &mut Context<DecisionsView>,
) -> bool {
    if forms.note(handle).is_none() {
        return false;
    }
    forms.set_note(handle, None);
    window.focus(back, cx);
    cx.notify();
    true
}
