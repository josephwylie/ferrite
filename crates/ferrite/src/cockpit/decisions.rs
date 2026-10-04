//! The Decision's answer paths and keys (WP-F). One door answers an
//! approval whatever surface asked — the transcript's Decision row, the
//! wall's quick answers, a toast — and one door answers a question's
//! option. The per-Decision cursor and amend note live in each Pane's
//! `RequestForms`; Ferrite's own thread rules live here.

use super::subagents::shown_on;
use super::*;
use crate::decision::{self, ApprovalChoice, ThreadRule};
use ferrite_core::activity::{DecisionHandle, PendingDecision, Subject};
use gpui::App;

/// The cockpit's Decision state: a handle back to the view (what a card
/// drawn inside the cached transcript presses through) and the thread rules
/// option 2 made where the provider offered no standing answer.
pub(crate) struct DecisionState {
    owner: gpui::WeakEntity<CockpitView>,
    rules: Vec<ThreadRule>,
}

impl DecisionState {
    pub(super) fn new(owner: gpui::WeakEntity<CockpitView>) -> Self {
        Self {
            owner,
            rules: Vec::new(),
        }
    }

    pub(super) fn owner(&self) -> gpui::WeakEntity<CockpitView> {
        self.owner.clone()
    }

    #[cfg(test)]
    pub(super) fn rules(&self) -> &[ThreadRule] {
        &self.rules
    }
}

impl CockpitView {
    /// The Decision row(s) this Pane's transcript draws after its last row
    /// (`TranscriptInput.tail`): every request its selected Subject shows,
    /// as one transcript row each. `key` changes whenever what they draw
    /// does — a request, its sending state, the cursor, a note, a pick, an
    /// error, the head's words.
    #[allow(dead_code)] // Called by core-transcript's `sync_transcript`.
    pub(crate) fn decision_tail(
        &self,
        index: usize,
        _cx: &App,
    ) -> Option<crate::transcript::TranscriptTail> {
        let cards = self.decision_cards(index, false, false)?;
        let key = cards.key();
        Some(crate::transcript::TranscriptTail {
            key,
            render: std::rc::Rc::new(move |window: &mut Window, cx: &mut App| -> AnyElement {
                cards
                    .render(window, cx)
                    .unwrap_or_else(|| div().into_any_element())
            }),
        })
    }

    /// The head's provider and its context word: Codex's sandbox, Claude's
    /// permission mode (the Session's, else the one it was spawned with).
    pub(super) fn decision_context(
        &self,
        thread: ThreadId,
    ) -> (Option<Provider>, Option<SharedString>) {
        let Some(open) = self.cockpit.thread(thread) else {
            return (None, None);
        };
        let provider = open.provider();
        let settings = &self.prefs.settings;
        let context = decision::context_label(
            provider,
            open.permission_mode()
                .or(settings.claude_permission_mode.as_deref()),
            settings.codex_sandbox.as_deref(),
        );
        (Some(provider), Some(context))
    }

    fn thread_workspace(&self, thread: ThreadId) -> Option<std::path::PathBuf> {
        self.cockpit
            .thread(thread)?
            .workspace()
            .map(|workspace| workspace.cwd().to_path_buf())
    }

    /// The approval `thread` waits on: the one its Pane shows, else Main's,
    /// else any.
    fn pending_approval(&self, thread: ThreadId) -> Option<PendingDecision> {
        let pending = self.cockpit.thread(thread)?.activity().pending_decisions();
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

    /// The question `thread` waits on, chosen the same way.
    fn pending_question(&self, thread: ThreadId) -> Option<PendingDecision> {
        let pending = self.cockpit.thread(thread)?.activity().pending_decisions();
        let question = |request: &&PendingDecision| pane::questions_of(&request.decision).is_some();
        let pane = self.pane_for(thread).map(|index| &self.panes[index]);
        pending
            .iter()
            .filter(question)
            .find(|request| pane.is_some_and(|pane| shown_on(pane, request)))
            .or_else(|| {
                pending
                    .iter()
                    .filter(question)
                    .find(|request| matches!(request.subject, Some(Subject::Main) | None))
            })
            .or_else(|| pending.iter().find(question))
            .cloned()
    }

    /// Whether option `choice` of `request` can act (the row draws it
    /// enabled).
    pub(super) fn approval_choice_enabled(
        &self,
        thread: ThreadId,
        request: &PendingDecision,
        choice: ApprovalChoice,
    ) -> bool {
        let workspace = self.thread_workspace(thread);
        let provider = self.cockpit.thread(thread).map(|open| open.provider());
        decision::approval_rows(&request.decision, provider, workspace.as_deref())
            .get(choice.index())
            .is_some_and(|row| row.enabled)
    }

    /// Answer `thread`'s approval with one of its three options — the one
    /// door the Decision row, the wall and a toast share. `note` rides the
    /// answer: a denial's message, or the Thread's next prompt after an
    /// allow.
    pub(crate) fn answer_approval(
        &mut self,
        thread: ThreadId,
        choice: ApprovalChoice,
        note: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(request) = self.pending_approval(thread) {
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
        thread: ThreadId,
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
                    let workspace = self.thread_workspace(thread);
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
            if !self.decisions.rules.contains(&rule) {
                self.decisions.rules.push(rule);
            }
        }
        match choice {
            ApprovalChoice::DenyAndSteer => {
                if let Some(window) = window {
                    self.steer(thread, window, cx);
                }
            }
            _ => {
                if let Some(note) = note {
                    self.cockpit.queue(thread, note);
                }
            }
        }
        cx.notify();
        true
    }

    /// After a denial the operator says what to do instead: the keyboard
    /// lands in that Pane's Composer (Main's, for a child's request).
    fn steer(&mut self, thread: ThreadId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        if !self.panes[index].is_main() {
            self.select_subject(thread, Subject::Main, window, cx);
        }
        self.cockpit.focus_thread(thread);
        self.focus_pane(index);
        let focus = self.panes[index].composer.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    /// Answer `thread`'s question with option `option` (0-based): a lone
    /// single-select question sends it and answers true; anything more
    /// needs its form, and false tells the caller to open the Pane.
    pub(crate) fn answer_question_option(
        &mut self,
        thread: ThreadId,
        option: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(request) = self.pending_question(thread) else {
            return false;
        };
        if request.submitting {
            return false;
        }
        let Some(questions) = pane::questions_of(&request.decision) else {
            return false;
        };
        match questions {
            [question] if !question.multi_select && option < question.options.len() => {
                self.respond_exact(
                    thread,
                    &request.handle,
                    DecisionAnswer::Questions {
                        answers: vec![ferrite_core::questions::Answer {
                            picks: vec![option],
                            other: None,
                        }],
                    },
                    cx,
                );
                true
            }
            _ => false,
        }
    }

    /// Ferrite's thread rules at work: a new approval in a Thread a rule
    /// covers is allowed as it arrives.
    pub(super) fn apply_thread_rules(
        &mut self,
        frame: &[ferrite_core::cockpit::PaneUpdate],
        cx: &mut Context<Self>,
    ) {
        if self.decisions.rules.is_empty() {
            return;
        }
        for update in frame {
            let thread = update.thread;
            if !self
                .decisions
                .rules
                .iter()
                .any(|rule| rule.thread == thread)
            {
                continue;
            }
            let workspace = self.thread_workspace(thread);
            let covered: Vec<PendingDecision> = self
                .cockpit
                .thread(thread)
                .map(|open| {
                    open.activity()
                        .pending_decisions()
                        .iter()
                        .filter(|request| {
                            !request.submitting
                                && request.reply_error.is_none()
                                && request.decision.policy.allow
                                && !request.decision.policy.interaction_required
                                && self.decisions.rules.iter().any(|rule| {
                                    rule.covers(thread, &request.decision, workspace.as_deref())
                                })
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
    /// package registers). The keys are bound where a Decision holds the
    /// keyboard on an empty Composer line; with nothing to answer they
    /// fall through to their other bindings.
    pub(super) fn register_decision_actions<E: InteractiveElement>(
        &self,
        element: E,
        cx: &mut Context<Self>,
    ) -> E {
        element
            .on_action(
                cx.listener(|view, _: &decision::SelectNext, _, cx| view.decision_step(1, cx)),
            )
            .on_action(
                cx.listener(|view, _: &decision::SelectPrevious, _, cx| view.decision_step(-1, cx)),
            )
            .on_action(cx.listener(|view, _: &decision::Confirm, window, cx| {
                view.decision_confirm(window, cx)
            }))
            .on_action(cx.listener(|view, _: &decision::Dismiss, window, cx| {
                view.decision_dismiss(window, cx)
            }))
            .on_action(
                cx.listener(|view, _: &decision::Amend, window, cx| {
                    view.decision_amend(window, cx)
                }),
            )
    }

    /// Which rows the cursor walks on `request`, and whether each can act:
    /// an approval's three options, or the options of the question its
    /// digits pick in.
    fn cursor_rows(&self, index: usize, request: &PendingDecision) -> Vec<bool> {
        let Some(thread) = self.panes[index].thread() else {
            return Vec::new();
        };
        if let Some(questions) = pane::questions_of(&request.decision) {
            let answers = self.panes[index]
                .request_forms
                .answers(&request.handle)
                .unwrap_or_else(|| vec![Default::default(); questions.len()]);
            return super::subagents::digit_question(&answers, questions)
                .map(|qi| vec![true; questions[qi].options.len()])
                .unwrap_or_default();
        }
        if !matches!(request.decision.kind, ferrite_core::DecisionKind::Approval) {
            return Vec::new();
        }
        let workspace = self.thread_workspace(thread);
        let provider = self.cockpit.thread(thread).map(|open| open.provider());
        decision::approval_rows(&request.decision, provider, workspace.as_deref())
            .iter()
            .map(|row| row.enabled)
            .collect()
    }

    /// The row under `request`'s cursor: where it was moved, else the
    /// first option that can act.
    fn cursor_at(&self, index: usize, request: &PendingDecision, rows: &[bool]) -> usize {
        self.panes[index]
            .request_forms
            .cursor(&request.handle)
            .filter(|at| *at < rows.len())
            .unwrap_or_else(|| rows.iter().position(|enabled| *enabled).unwrap_or(0))
    }

    /// ↑↓: the `❯` and its bar move, wrapping, over the rows that can act.
    fn decision_step(&mut self, step: isize, cx: &mut Context<Self>) {
        let index = self.focused();
        let Some(request) = self.shown_request(index) else {
            cx.propagate();
            return;
        };
        let rows = self.cursor_rows(index, &request);
        if rows.is_empty() {
            cx.propagate();
            return;
        }
        let at = self.cursor_at(index, &request, &rows);
        let next = decision::step_cursor(&rows, at, step);
        self.panes[index]
            .request_forms
            .set_cursor(&request.handle, Some(next));
        cx.notify();
    }

    /// ⏎: the option under the cursor, with the note if one is open.
    fn decision_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.focused();
        let Some(request) = self.shown_request(index) else {
            cx.propagate();
            return;
        };
        if !self.confirm_request(index, &request, window, cx) {
            cx.propagate();
        }
    }

    /// Pick `request`'s cursor row: an approval answers that option; a
    /// question sends when every question is answered, else picks the row
    /// (a lone single-select question then sends). The open note rides the
    /// answer. False when nothing here takes ⏎.
    fn confirm_request(
        &mut self,
        index: usize,
        request: &PendingDecision,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(thread) = self.panes[index].thread() else {
            return false;
        };
        if request.submitting {
            return true;
        }
        let rows = self.cursor_rows(index, request);
        let at = self.cursor_at(index, request, &rows);
        if pane::questions_of(&request.decision).is_some() {
            let note = self.take_decision_note(thread, &request.handle, cx);
            if !self.send_question_form(thread, &request.handle, true, cx) && !rows.is_empty() {
                self.pick_request_row(index, at, window, cx);
            }
            if let Some(note) = note.filter(|_| !self.still_waiting(thread, &request.handle)) {
                self.cockpit.queue(thread, note);
            }
            cx.notify();
            return true;
        }
        let Some(choice) = ApprovalChoice::from_index(at)
            .filter(|_| matches!(request.decision.kind, ferrite_core::DecisionKind::Approval))
        else {
            return false;
        };
        if !rows.get(at).copied().unwrap_or(false) {
            return true;
        }
        let note = self.take_decision_note(thread, &request.handle, cx);
        self.answer_approval_request(thread, request, choice, note, Some(window), cx);
        true
    }

    /// Whether `handle` still waits on the operator (not answered, not in
    /// flight).
    fn still_waiting(&self, thread: ThreadId, handle: &DecisionHandle) -> bool {
        self.cockpit.thread(thread).is_some_and(|open| {
            open.activity()
                .pending_decisions()
                .iter()
                .any(|request| &request.handle == handle && !request.submitting)
        })
    }

    /// esc: an open note closes; otherwise an approval is denied (option 3)
    /// and a question dismissed.
    fn decision_dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.focused();
        let Some(request) = self.shown_request(index) else {
            cx.propagate();
            return;
        };
        let Some(thread) = self.panes[index].thread() else {
            return;
        };
        if self.close_decision_note(thread, &request.handle, window, cx) {
            return;
        }
        if request.submitting {
            return;
        }
        if pane::questions_of(&request.decision).is_some() {
            self.respond_exact(
                thread,
                &request.handle,
                DecisionAnswer::Deny {
                    message: decision::QUESTION_SKIPPED.into(),
                },
                cx,
            );
            return;
        }
        match request.decision.kind {
            ferrite_core::DecisionKind::Approval => {
                if !self.answer_approval_request(
                    thread,
                    &request,
                    ApprovalChoice::DenyAndSteer,
                    None,
                    Some(window),
                    cx,
                ) {
                    cx.propagate();
                }
            }
            _ if request.decision.policy.deny => {
                self.respond_exact(thread, &request.handle, DecisionAnswer::Cancel, cx)
            }
            _ => cx.propagate(),
        }
    }

    /// ⇥: the note under the options opens and takes the keyboard.
    fn decision_amend(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.focused();
        let Some(request) = self.shown_request(index) else {
            cx.propagate();
            return;
        };
        let amendable = pane::questions_of(&request.decision).is_some()
            || matches!(request.decision.kind, ferrite_core::DecisionKind::Approval);
        if !amendable || request.submitting {
            cx.propagate();
            return;
        }
        let forms = self.panes[index].request_forms.clone();
        let note = forms.note(&request.handle).unwrap_or_else(|| {
            let note = cx.new(crate::composer::Composer::new);
            forms.set_note(&request.handle, Some(note.clone()));
            note
        });
        window.focus(&note.focus_handle(cx), cx);
        cx.notify();
    }

    /// The open note's words, taken (the note closes).
    pub(super) fn take_decision_note(
        &mut self,
        thread: ThreadId,
        handle: &DecisionHandle,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let index = self.pane_for(thread)?;
        let forms = self.panes[index].request_forms.clone();
        let note = forms.note(handle)?;
        forms.set_note(handle, None);
        let text = note.update(cx, |line, cx| line.take(cx));
        cx.notify();
        Some(text)
    }

    /// ⏎ in the note: the cursor's option goes with the note.
    pub(super) fn send_decision_note(
        &mut self,
        thread: ThreadId,
        handle: &DecisionHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let request = self.cockpit.thread(thread).and_then(|open| {
            open.activity()
                .pending_decisions()
                .iter()
                .find(|request| &request.handle == handle)
                .cloned()
        });
        if let Some(request) = request {
            self.confirm_request(index, &request, window, cx);
        }
        self.focus_composer_after_note(index, window, cx);
    }

    /// esc in the note: it closes and the keyboard goes back to the
    /// Composer (a second esc denies). True when a note was open.
    pub(super) fn close_decision_note(
        &mut self,
        thread: ThreadId,
        handle: &DecisionHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.pane_for(thread) else {
            return false;
        };
        let forms = self.panes[index].request_forms.clone();
        if forms.note(handle).is_none() {
            return false;
        }
        forms.set_note(handle, None);
        self.focus_composer_after_note(index, window, cx);
        cx.notify();
        true
    }

    fn focus_composer_after_note(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = if self.panes[index].is_main() {
            self.panes[index].composer.read(cx).focus_handle(cx)
        } else {
            self.panes[index].transcript_focus.clone()
        };
        window.focus(&focus, cx);
    }

    /// The open note on the focused Pane's Decision, which holds the
    /// keyboard while it is open (the render's focus snap asks).
    pub(super) fn decision_note_focus(&self, cx: &App) -> Option<FocusHandle> {
        let pane = self.panes.get(self.focused())?;
        let request = self.shown_request(self.focused())?;
        let note = pane.request_forms.note(&request.handle)?;
        Some(note.focus_handle(cx))
    }
}
