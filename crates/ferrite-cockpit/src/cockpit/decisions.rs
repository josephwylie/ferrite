//! The Decision's answer paths and keys (WP-F). One door answers an
//! approval whatever surface asked — the transcript's Decision row or a
//! toast.
//!
//! `DecisionsView` (ADR 0041) keeps Ferrite's own thread rules and each
//! Pane's `RequestForms` (the cursor, the amend note, the split button's
//! menu, a question's or form's answers), answers the Decision keys, and
//! is what a card drawn inside a cached transcript presses through: each
//! answer it asks for it says as a `DecisionEvent`, which the root hears
//! and tells the Server. The cards draw from a snapshot the root takes
//! (`decision_inputs`).

use super::subagents::{shown_on, Pick, RequestForms};
use super::*;
use ferrite_core::activity::{DecisionHandle, PendingDecision, Subject};
use ferrite_pane::decision::{self, ApprovalChoice, ThreadRule};
use gpui::App;
use ferrite_core::ThreadKey;
use std::collections::HashMap;

/// What a Decision card's press asks of the window.
pub(crate) enum DecisionEvent {
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
pub(super) enum DecisionKey {
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
pub(super) struct KeyTarget {
    pane: PaneIdentity,
    thread: ThreadKey,
    request: PendingDecision,
    /// An approval's options, each whether it can act; empty for any
    /// other kind.
    approval_rows: Vec<bool>,
    /// The Composer on Main, else the transcript.
    back: FocusHandle,
}

/// The thread rules option 2 made where the provider offered no standing
/// answer, each Pane's Decision forms, and the cards' way back to the
/// window.
#[derive(Default)]
pub(crate) struct DecisionsView {
    rules: Vec<ThreadRule>,
    /// By Pane, made with it (`open_pane`), so a card always has its
    /// Pane's to draw from.
    forms: HashMap<PaneIdentity, RequestForms>,
}

impl EventEmitter<DecisionEvent> for DecisionsView {}

impl DecisionsView {
    /// Keep `rule`: what its pattern covers in its Thread is allowed from
    /// now on.
    fn keep(&mut self, rule: ThreadRule) {
        if !self.rules.contains(&rule) {
            self.rules.push(rule);
        }
    }

    fn covers_any(&self, thread: ThreadKey) -> bool {
        self.rules.iter().any(|rule| rule.thread == thread)
    }

    fn covers(
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
    pub(super) fn retain_threads(&mut self, keep: impl Fn(ThreadKey) -> bool) {
        self.rules.retain(|rule| keep(rule.thread));
    }

    #[cfg(test)]
    pub(super) fn rules(&self) -> &[ThreadRule] {
        &self.rules
    }

    /// Pane `pane` was made: its forms are made with it.
    pub(super) fn open_pane(&mut self, pane: PaneIdentity) {
        self.forms.entry(pane).or_default();
    }

    /// Every Pane `keep` refuses takes its forms with it.
    pub(super) fn retain_panes(&mut self, keep: impl Fn(PaneIdentity) -> bool) {
        self.forms.retain(|pane, _| keep(*pane));
    }

    /// A draft's first send made it `thread`'s Pane: it keeps its forms.
    pub(super) fn adopt(&mut self, draft: PaneIdentity, thread: PaneIdentity) {
        if let Some(forms) = self.forms.remove(&draft) {
            self.forms.insert(thread, forms);
        }
    }

    /// What `pane`'s cards draw from and write.
    pub(super) fn forms(&self, pane: PaneIdentity) -> Option<&RequestForms> {
        self.forms.get(&pane)
    }

    /// `handle` was answered: what its card kept goes.
    pub(super) fn forget(&mut self, pane: PaneIdentity, handle: &DecisionHandle) {
        if let Some(forms) = self.forms.get(&pane) {
            forms.retain(|kept| kept != handle);
        }
    }

    /// The open note's words, taken (the note closes).
    pub(super) fn take_note(
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
    pub(super) fn note_focus(
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
    pub(super) fn close_note(
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
    pub(super) fn key(
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
    super::subagents::digit_question(&answers, questions)
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
            .filter(|answers| super::subagents::answered(answers))
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
        let note = cx.new(ferrite_pane::composer::Composer::new);
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

impl CockpitView {
    pub(super) fn on_decision_event(
        &mut self,
        _: &Entity<DecisionsView>,
        event: &DecisionEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            DecisionEvent::Approve {
                thread,
                request,
                choice,
                switch,
            } => {
                let note = self.take_decision_note(*thread, &request.handle, cx);
                self.answer_approval_request(*thread, request, *choice, note, Some(window), cx);
                if let Some((mode, generation)) = switch {
                    self.run_session_control(
                        *thread,
                        *generation,
                        ferrite_core::SessionControl::SetPermissionMode { mode: mode.clone() },
                        cx,
                    );
                }
            }
            DecisionEvent::Respond {
                thread,
                handle,
                answer,
            } => self.respond_exact(*thread, handle, answer.clone(), cx),
            DecisionEvent::SendQuestions { thread, handle } => {
                self.send_question_form(*thread, handle, false, cx);
            }
            DecisionEvent::SendForm {
                thread,
                handle,
                fields,
            } => self.send_form(*thread, handle, fields, cx),
            DecisionEvent::Picked { thread, handle } => {
                self.clear_request_error(*thread, handle);
                cx.notify();
            }
            DecisionEvent::SendNote { thread, handle } => {
                self.send_decision_note(*thread, handle, window, cx)
            }
            DecisionEvent::CloseNote { thread, handle } => {
                let Some(index) = self.pane_for(*thread) else {
                    return;
                };
                let pane = self.panes[index].identity;
                let back = self.note_back(index, cx);
                self.decisions.update(cx, |decisions, cx| {
                    decisions.close_note(pane, handle, &back, window, cx)
                });
            }
            DecisionEvent::Answered {
                thread,
                handle,
                answers,
                note,
            } => {
                let answer = DecisionAnswer::Questions {
                    answers: answers.clone(),
                };
                self.respond_exact(*thread, handle, answer, cx);
                if let Some(note) = note.clone().filter(|_| !self.still_waiting(*thread, handle, cx)) {
                    let _ = self.cockpit.update(cx, |cockpit, _| cockpit.queue(*thread, note));
                }
                cx.notify();
            }
        }
    }
}

fn card_tail(cards: subagents::Cards) -> ferrite_pane::transcript::TranscriptTail {
    let key = cards.key();
    ferrite_pane::transcript::TranscriptTail {
        key,
        render: std::rc::Rc::new(move |window: &mut Window, cx: &mut App| -> AnyElement {
            cards
                .render(window, cx)
                .unwrap_or_else(|| div().into_any_element())
        }),
    }
}

impl CockpitView {
    /// The Decision row(s) this Pane's transcript draws after its last row
    /// (`TranscriptPresentation.tail`): every request its selected Subject shows,
    /// as one transcript row each. `key` changes whenever what they draw
    /// does — a request, its sending state, the cursor, a note, a pick, an
    /// error, the head's words.
    pub(crate) fn decision_tail(
        &self,
        index: usize,
        cx: &App,
    ) -> Option<ferrite_pane::transcript::TranscriptTail> {
        Some(card_tail(self.decision_cards(index, false, false, cx)?))
    }

    /// The head's provider and its context word: the sandbox its provider
    /// announced for this Thread, else its permission mode (the Session's,
    /// else the one it was spawned with). Settings' defaults are for the
    /// next Session and never relabel this one.
    pub(super) fn decision_context(
        &self,
        thread: ThreadKey,
        cx: &App,
    ) -> (Option<Provider>, Option<SharedString>) {
        let Some(open) = self.cockpit.read(cx).thread(thread) else {
            return (None, None);
        };
        let provider = open.provider();
        let settings = &self.prefs.settings;
        let context = decision::context_label(
            open.permission_mode()
                .unwrap_or(settings.permission_mode_for(provider)),
            open.sandbox(),
        );
        (Some(provider), Some(context))
    }

    fn thread_workspace(&self, thread: ThreadKey, cx: &App) -> Option<std::path::PathBuf> {
        self.cockpit
            .read(cx)
            .thread(thread)?
            .workspace()
            .map(|workspace| workspace.cwd().to_path_buf())
    }

    /// The approval `thread` waits on: the one its Pane shows, else Main's,
    /// else any.
    fn pending_approval(&self, thread: ThreadKey, cx: &App) -> Option<PendingDecision> {
        let pending = self.cockpit.read(cx).thread(thread)?.pending_decisions();
        let approval = |request: &&PendingDecision| {
            matches!(request.decision.kind, ferrite_core::DecisionKind::Approval)
                && !request.submitting
        };
        let pane = self.pane_for(thread).map(|index| &self.panes[index]);
        pending
            .iter()
            .filter(approval)
            .find(|request| pane.is_some_and(|pane| shown_on(pane, request)))
            .or_else(|| {
                pending
                    .iter()
                    .filter(approval)
                    .find(|request| matches!(request.subject, Some(Subject::Main) | None))
            })
            .or_else(|| pending.iter().find(approval))
            .cloned()
    }

    /// Answer `thread`'s approval with one of its three options — the one
    /// door the Decision row, the wall and a toast share. `note` rides the
    /// answer: a denial's message, or the Thread's next prompt after an
    /// allow.
    pub(crate) fn answer_approval(
        &mut self,
        thread: ThreadKey,
        choice: ApprovalChoice,
        note: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(request) = self.pending_approval(thread, cx) {
            self.answer_approval_request(thread, &request, choice, note, Some(window), cx);
        }
    }

    /// One approval's answer (D-4). 1 allows. 2 adopts the provider's
    /// standing answer when it offered one (as `AllowAlways`, or as the
    /// provider's own choice where a plain allow is forbidden), else allows
    /// and keeps a Ferrite thread rule that allows what its pattern covers
    /// from now on. 3 denies — with the note as the message, else the
    /// steer message — and lands the keyboard in the Pane's Composer. A
    /// note after an allow is the Thread's next prompt. False when the
    /// option cannot act.
    pub(super) fn answer_approval_request(
        &mut self,
        thread: ThreadKey,
        request: &PendingDecision,
        choice: ApprovalChoice,
        note: Option<String>,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> bool {
        let decision = &request.decision;
        if request.submitting || !matches!(decision.kind, ferrite_core::DecisionKind::Approval) {
            return false;
        }
        let policy = decision.policy;
        let allows = policy.allow && !policy.interaction_required;
        let note = note
            .map(|note| note.trim().to_string())
            .filter(|note| !note.is_empty());
        let mut rule = None;
        let response = match choice {
            ApprovalChoice::Allow if allows => DecisionAnswer::Allow {
                input: decision.input.clone(),
            },
            ApprovalChoice::AllowForThread => match decision::standing_choice(decision) {
                Some((at, _)) if allows => DecisionAnswer::AllowAlways {
                    input: decision.input.clone(),
                    suggestion: decision.suggestions[at].value.clone(),
                },
                Some((at, _)) => DecisionAnswer::Choose {
                    value: decision.suggestions[at].value.clone(),
                },
                None if allows => {
                    let workspace = self.thread_workspace(thread, cx);
                    rule = Some(ThreadRule::new(thread, decision, workspace.as_deref()));
                    DecisionAnswer::Allow {
                        input: decision.input.clone(),
                    }
                }
                None => return false,
            },
            ApprovalChoice::DenyAndSteer if policy.deny => DecisionAnswer::Deny {
                message: note
                    .clone()
                    .unwrap_or_else(|| decision::DENY_AND_STEER.into()),
            },
            _ => return false,
        };
        self.respond_exact(thread, &request.handle, response, cx);
        if let Some(rule) = rule {
            self.decisions.update(cx, |decisions, _| decisions.keep(rule));
        }
        match choice {
            ApprovalChoice::DenyAndSteer => {
                if let Some(window) = window {
                    self.steer(thread, window, cx);
                }
            }
            _ => {
                if let Some(note) = note {
                    let _ = self.cockpit.update(cx, |cockpit, _| cockpit.queue(thread, note));
                }
            }
        }
        cx.notify();
        true
    }

    /// After a denial the operator says what to do instead: the keyboard
    /// lands in that Pane's Composer (Main's, for a child's request).
    fn steer(&mut self, thread: ThreadKey, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        if !self.panes[index].is_main() {
            self.select_subject(thread, Subject::Main, window, cx);
        }
        self.cockpit.update(cx, |cockpit, _| cockpit.focus_thread(thread));
        self.focus_pane(index, cx);
        let focus = self.panes[index].composer.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    /// Ferrite's thread rules at work: a new approval in a Thread a rule
    /// covers is allowed as it arrives.
    pub(super) fn apply_thread_rules(
        &mut self,
        frame: &[ferrite_core::cockpit::PaneUpdate],
        cx: &mut Context<Self>,
    ) {
        if self.decisions.read(cx).rules.is_empty() {
            return;
        }
        for update in frame {
            let thread = update.thread;
            let rules = self.decisions.read(cx);
            if !rules.covers_any(thread) {
                continue;
            }
            let workspace = self.thread_workspace(thread, cx);
            let covered: Vec<PendingDecision> = self
                .cockpit
                .read(cx)
                .thread(thread)
                .map(|open| {
                    open.pending_decisions()
                        .iter()
                        .filter(|request| {
                            !request.submitting
                                && request.reply_error.is_none()
                                && request.decision.policy.allow
                                && !request.decision.policy.interaction_required
                                && rules.covers(thread, &request.decision, workspace.as_deref())
                        })
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            for request in covered {
                self.respond_exact(
                    thread,
                    &request.handle,
                    DecisionAnswer::Allow {
                        input: request.decision.input.clone(),
                    },
                    cx,
                );
            }
        }
    }

    // ------------------------------------------------------------- keys

    /// The `decision::*` actions, on the cockpit's root (the hook every
    /// package registers): the Decisions' view has no element of its own
    /// in the focus chain, so its keys are heard here and handed to it.
    /// The keys are bound where a Decision holds the keyboard on an empty
    /// Composer line; with nothing to answer they fall through to their
    /// other bindings.
    pub(super) fn register_decision_actions<E: InteractiveElement>(
        &self,
        element: E,
        cx: &mut Context<Self>,
    ) -> E {
        element
            .on_action(cx.listener(|view, _: &decision::SelectNext, window, cx| {
                view.decision_key(DecisionKey::Step(1), window, cx)
            }))
            .on_action(cx.listener(|view, _: &decision::SelectPrevious, window, cx| {
                view.decision_key(DecisionKey::Step(-1), window, cx)
            }))
            .on_action(cx.listener(|view, _: &decision::Confirm, window, cx| {
                view.decision_key(DecisionKey::Confirm, window, cx)
            }))
            .on_action(cx.listener(|view, _: &decision::Dismiss, window, cx| {
                view.decision_key(DecisionKey::Dismiss, window, cx)
            }))
            .on_action(cx.listener(|view, _: &decision::Amend, window, cx| {
                view.decision_key(DecisionKey::Amend, window, cx)
            }))
    }

    fn decision_key(&mut self, key: DecisionKey, window: &mut Window, cx: &mut Context<Self>) {
        if !self.decision_key_taken(self.focused(cx), key, window, cx) {
            cx.propagate();
        }
    }

    /// `key` on the request Pane `index` shows, answered by the Decisions'
    /// view; false when nothing there takes it.
    pub(super) fn decision_key_taken(
        &mut self,
        index: usize,
        key: DecisionKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self.key_target(index, None, cx) else {
            return false;
        };
        self.decisions
            .update(cx, |decisions, cx| decisions.key(key, &target, window, cx))
    }

    /// What a Decision key on Pane `index` acts on: `request`, else the
    /// first request it shows.
    fn key_target(
        &self,
        index: usize,
        request: Option<PendingDecision>,
        cx: &App,
    ) -> Option<KeyTarget> {
        let pane = self.panes.get(index)?;
        let thread = pane.thread()?;
        let request = request.or_else(|| self.shown_request(index, cx))?;
        let approval = pane::questions_of(&request.decision).is_none()
            && matches!(request.decision.kind, ferrite_core::DecisionKind::Approval);
        let approval_rows = if approval {
            let workspace = self.thread_workspace(thread, cx);
            let provider = self.cockpit.read(cx).thread(thread).map(|open| open.provider());
            decision::approval_rows(&request.decision, provider, workspace.as_deref())
                .iter()
                .map(|row| row.enabled)
                .collect()
        } else {
            Vec::new()
        };
        Some(KeyTarget {
            pane: pane.identity,
            thread,
            request,
            approval_rows,
            back: self.note_back(index, cx),
        })
    }

    /// Whether `handle` still waits on the operator (not answered, not in
    /// flight).
    fn still_waiting(&self, thread: ThreadKey, handle: &DecisionHandle, cx: &App) -> bool {
        self.cockpit.read(cx).thread(thread).is_some_and(|open| {
            open.pending_decisions()
                .iter()
                .any(|request| &request.handle == handle && !request.submitting)
        })
    }

    /// The open note's words, taken (the note closes).
    fn take_decision_note(
        &mut self,
        thread: ThreadKey,
        handle: &DecisionHandle,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let pane = self.panes[self.pane_for(thread)?].identity;
        self.decisions
            .update(cx, |decisions, cx| decisions.take_note(pane, handle, cx))
    }

    /// ⏎ in the note: the cursor's option goes with the note.
    fn send_decision_note(
        &mut self,
        thread: ThreadKey,
        handle: &DecisionHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let request = self.cockpit.read(cx).thread(thread).and_then(|open| {
            open.pending_decisions()
                .iter()
                .find(|request| &request.handle == handle)
                .cloned()
        });
        if let Some(target) = request.and_then(|request| self.key_target(index, Some(request), cx)) {
            self.decisions.update(cx, |decisions, cx| {
                decisions.key(DecisionKey::Confirm, &target, window, cx)
            });
        }
        window.focus(&self.note_back(index, cx), cx);
    }

    /// Where the keyboard goes as Pane `index`'s note closes: its Composer
    /// on Main, else its transcript.
    fn note_back(&self, index: usize, cx: &App) -> FocusHandle {
        if self.panes[index].is_main() {
            self.panes[index].composer.read(cx).focus_handle(cx)
        } else {
            self.panes[index].transcript_focus.clone()
        }
    }

    /// The open note on the focused Pane's Decision, which holds the
    /// keyboard while it is open (the render's focus snap asks).
    pub(super) fn decision_note_focus(&self, cx: &App) -> Option<FocusHandle> {
        let pane = self.panes.get(self.focused(cx))?;
        let request = self.shown_request(self.focused(cx), cx)?;
        self.decisions
            .read(cx)
            .note_focus(pane.identity, &request.handle, cx)
    }
}
