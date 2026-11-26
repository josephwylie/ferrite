//! The Decision's answer paths and keys (WP-F). One door answers an
//! approval whatever surface asked — the transcript's Decision row or a
//! toast.
//!
//! `DecisionsView` (`ferrite_pane::decision`, ADR 0041) keeps the thread
//! rules and each Pane's forms, answers the Decision keys, and says each
//! answer a card asks for as a `DecisionEvent`, which the root hears here
//! and tells the Server. The cards (`ferrite_pane::decision::Cards`) draw
//! from a snapshot the root takes (`decision_inputs`), as values the cell
//! and the transcript draw themselves.

use super::subagents::shown_on;
use super::*;
use ferrite_core::activity::{DecisionHandle, PendingDecision, Subject};
use ferrite_pane::decision::{self, ApprovalChoice, Cards, ThreadRule};
pub(crate) use ferrite_pane::decision::{DecisionEvent, DecisionKey, DecisionsView, KeyTarget};
use gpui::App;
use ferrite_core::ThreadKey;

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
                self.clear_request_error(*thread, handle, cx);
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

fn card_tail(cards: Cards) -> ferrite_pane::transcript::TranscriptTail {
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
        let cards = self.decision_cards(index, false, false, cx)?;
        let kept_in = Some(self.panes[index].read(cx).geometry.clone());
        Some(card_tail(Cards { kept_in, ..cards }))
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
            .find(|request| pane.is_some_and(|pane| shown_on(pane.read(cx), request)))
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
        if !self.panes[index].read(cx).is_main() {
            self.select_subject(thread, Subject::Main, window, cx);
        }
        self.cockpit.update(cx, |cockpit, _| cockpit.focus_thread(thread));
        self.focus_pane(index, cx);
        let focus = self.panes[index].read(cx).composer.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    /// Ferrite's thread rules at work: a new approval in a Thread a rule
    /// covers is allowed as it arrives.
    pub(super) fn apply_thread_rules(
        &mut self,
        frame: &[ferrite_client::cockpit::PaneUpdate],
        cx: &mut Context<Self>,
    ) {
        if self.decisions.read(cx).rules().is_empty() {
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
        if self.panes[index].read(cx).is_main() {
            self.panes[index].read(cx).composer.read(cx).focus_handle(cx)
        } else {
            self.panes[index].read(cx).transcript_focus.clone()
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
