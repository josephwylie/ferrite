//! Subject navigation and supervision. Provider execution stays in core.
use super::*;
use ferrite_core::activity::{
    ActivityView, AgentInfo, AgentStatus, DecisionHandle, PendingDecision, Subject,
};
use ferrite_pane::decision::{self, Cards};
use ferrite_ui::theme;

/// Pending handles can share a destination. Visit each destination once,
/// retaining request discovery order and wrapping after the current one.
pub(super) fn next_request<T: Clone + Eq>(
    targets: impl IntoIterator<Item = T>,
    current: Option<&T>,
) -> Option<T> {
    let mut distinct = Vec::new();
    for target in targets {
        if !distinct.contains(&target) {
            distinct.push(target);
        }
    }
    if distinct.is_empty() {
        return None;
    }
    let at = current
        .and_then(|current| distinct.iter().position(|target| target == current))
        .map_or(0, |at| (at + 1) % distinct.len());
    Some(distinct[at].clone())
}

/// Whether `pane` shows `request`: its selected Subject's, and on Main
/// also a request whose owner the provider did not name.
pub(super) fn shown_on(pane: &PaneView, request: &PendingDecision) -> bool {
    shown_for(&pane.selected, request)
}

/// Whether `request` asks in `subject`'s transcript: its own, or Main's for
/// one that names no agent.
pub(super) fn shown_for(subject: &Subject, request: &PendingDecision) -> bool {
    request.subject.as_ref() == Some(subject)
        || (request.subject.is_none() && *subject == Subject::Main)
}

/// The word Lineage says of an agent (T3's `threadRelationshipStatusLabel`).
/// How it ended outlives the Session that saw it end; what it was doing
/// needs a live one to say so.
pub(super) fn state_word(status: AgentStatus, fresh: bool) -> &'static str {
    match status {
        AgentStatus::Interrupted | AgentStatus::Shutdown => "Stopped",
        AgentStatus::Failed | AgentStatus::NotFound => "Failed",
        AgentStatus::Idle => "Done",
        _ if !fresh => "Unavailable",
        AgentStatus::Working => "Running",
        AgentStatus::Waiting => "Waiting",
        AgentStatus::Pending => "Queued",
        AgentStatus::Paused => "Paused",
        AgentStatus::NotLoaded => "Unavailable",
        AgentStatus::Unknown => "Done",
    }
}

/// The Subject that spawned `subject` by its agent's recorded parent: Main
/// when none is recorded (or the record names the agent itself); none for
/// Main itself.
pub(super) fn parent_of(activity: ActivityView<'_>, subject: &Subject) -> Option<Subject> {
    if *subject == Subject::Main {
        return None;
    }
    let parent = activity
        .children()
        .into_iter()
        .find(|agent| agent.subject() == *subject)
        .and_then(|agent| agent.info().parent.clone())
        .map(|parent| activity.canonical_subject(&parent))
        .filter(|parent| parent != subject);
    Some(parent.unwrap_or(Subject::Main))
}

pub(crate) fn agent_name(info: &AgentInfo) -> String {
    info.name
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            info.description
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
        })
        .or_else(|| info.kind.clone())
        .unwrap_or_else(|| "Subagent".into())
}

impl CockpitView {
    /// The agent's Subject, from the call that started it.
    pub(super) fn agent_of_call(&self, thread: ThreadKey, call: &str, cx: &gpui::App) -> Option<Subject> {
        self.cockpit.read(cx).thread(thread)?.activity().invocations().find_map(
            |(native, agent)| (native == call).then(|| agent.subject()),
        )
    }

    /// Show `subject`'s transcript in `thread`'s Pane: Main, or a subagent
    /// opened from its row (`open_agent_from`) and left by the title bar's
    /// back arrow (`history`) or escape. Every Subject switch the operator
    /// makes comes through here.
    pub(super) fn select_subject(
        &mut self,
        thread: ThreadKey,
        subject: Subject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.take_subject(thread, subject, cx) else {
            return;
        };
        self.cockpit.update(cx, |cockpit, _| cockpit.focus_thread(thread));
        self.focus_pane(index, cx);
        // Keep native text selections in their retained Subject entities.
        // A subagent's transcript must not edit or focus Main's hidden
        // Composer.
        let pane = self.panes[index].read(cx);
        let focus = if pane.is_main() {
            pane.composer.read(cx).focus_handle(cx)
        } else {
            pane.transcript_focus.clone()
        };
        window.focus(&focus, cx);
    }

    /// A press on an agent call's row in Pane `index`, or ↵ on it: the
    /// Pane takes that agent's transcript at full reading width. The
    /// Subject's own viewport (scroll, open tools) and Main's draft stay
    /// where they were; the focus pump hands the transcript the keyboard.
    /// A Delegate's row opens its Thread on its machine (`open_elsewhere`).
    pub(super) fn open_agent_from(&mut self, index: usize, call: String, cx: &mut Context<Self>) {
        let Some(thread) = self.panes.get(index).and_then(|pane| pane.thread()) else {
            return;
        };
        let selected = &self.panes[index].read(cx).selected;
        if let Some((server, there)) = self.delegate_of_call(thread, selected, &call, cx) {
            return self.open_elsewhere(&server, there, cx);
        }
        if let Some(subject) = self.agent_of_call(thread, &call, cx) {
            self.open_subagent(thread, subject, cx);
        }
    }

    /// Give `thread`'s Pane `subject`'s full chat and the focus, from
    /// wherever the agent is named: its row, Background work, the details
    /// card's Lineage. A subagent has no other view.
    pub(super) fn open_subagent(&mut self, thread: ThreadKey, subject: Subject, cx: &mut Context<Self>) {
        if let Some(index) = self.take_subject(thread, subject, cx) {
            self.cockpit.update(cx, |cockpit, _| cockpit.focus_thread(thread));
            self.focus_pane(index, cx);
        }
    }

    /// A NeedsYou request already focused its Thread in core. Select its owning
    /// Subject without synthesizing a window input event or answering it.
    pub(super) fn select_subject_from_notice(
        &mut self,
        thread: ThreadKey,
        subject: Subject,
        cx: &mut Context<Self>,
    ) {
        self.take_subject(thread, subject, cx);
    }

    /// The Pane that now shows `subject`, its saved history asked for
    /// without resuming the provider.
    fn take_subject(
        &mut self,
        thread: ThreadKey,
        subject: Subject,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let index = self.pane_for(thread)?;
        let open = self.cockpit.read(cx).thread(thread)?;
        let subject = open.activity().canonical_subject(&subject);
        let generation = open.activity().subject(&subject)?.revision();
        self.panes[index].update(cx, |pane, cx| pane.select_subject(subject, generation, cx));
        self.panes[index].read(cx).details.reset_pages();
        self.retry_subject_history(index, cx);
        let selected = self.panes[index].read(cx).selected.clone();
        self.cockpit
            .update(cx, |cockpit, _| cockpit.set_visible_subject(thread, selected));
        self.close_composer_menu(cx);
        cx.notify();
        Some(index)
    }

    /// `subject`'s ancestors by each agent's recorded parent, the
    /// immediate parent last (Main, which every chain ends in, first).
    pub(super) fn agent_lineage(&self, thread: ThreadKey, subject: &Subject, cx: &gpui::App) -> Vec<Subject> {
        let Some(open) = self.cockpit.read(cx).thread(thread) else {
            return vec![Subject::Main];
        };
        let activity = open.activity();
        let mut lineage = vec![Subject::Main];
        let mut at = subject.clone();
        let mut reversed = Vec::new();
        while let Some(parent) =
            parent_of(activity, &at).filter(|parent| *parent != Subject::Main)
        {
            if reversed.contains(&parent) {
                break;
            }
            reversed.push(parent.clone());
            at = parent;
        }
        lineage.extend(reversed.into_iter().rev());
        lineage
    }

    pub(super) fn respond_exact(
        &mut self,
        thread: ThreadKey,
        handle: &DecisionHandle,
        answer: DecisionAnswer,
        cx: &mut Context<Self>,
    ) {
        let answered = self.cockpit.update(cx, |cockpit, _| cockpit.respond_decision(thread, handle, answer));
        let handle = handle.clone();
        self.when_answered(answered, move |view, answered, cx| view.responded(thread, &handle, answered, cx), cx);
    }

    /// The Server's word on an answer to the Decision `handle` names: taken,
    /// no longer waiting, or refused, which the Pane says under the request.
    fn responded(
        &mut self,
        thread: ThreadKey,
        handle: &DecisionHandle,
        answered: std::io::Result<bool>,
        cx: &mut Context<Self>,
    ) {
        match answered {
            Ok(true) => {
                if let Some(index) = self.pane_for(thread) {
                    let pending = self.cockpit.read(cx).thread(thread).is_some_and(|open| {
                        open.pending_decisions().iter().any(|p| &p.handle == handle)
                    });
                    if !pending {
                        let pane = self.panes[index].identity;
                        self.decisions
                            .update(cx, |decisions, _| decisions.forget(pane, handle));
                    }
                    let value = None;
                    self.panes[index].update(cx, |pane, _| pane.request_error = value);
                }
            }
            Ok(false) => {}
            Err(error) => {
                if let Some(index) = self.pane_for(thread) {
                    let value = Some((handle.clone(), error.to_string()));
                    self.panes[index].update(cx, |pane, _| pane.request_error = value);
                }
            }
        }
        self.facts.acted(self.cockpit.read(cx), thread);
        cx.notify();
    }

    /// The Decision cards this Pane's selected Subject shows, as a snapshot
    /// of the cockpit: the requests, their owner's provider and context,
    /// and the Pane's forms. `overlay` cards lie on the plane over the
    /// transcript's foot; the transcript's own cards scroll with its rows.
    pub(super) fn decision_cards(
        &self,
        index: usize,
        overlay: bool,
        short: bool,
        cx: &gpui::App,
    ) -> Option<Cards> {
        let subject = self.panes.get(index)?.read(cx).selected.clone();
        self.decision_inputs(index, &subject, overlay, short, cx)
    }

    /// The cards of `subject`'s requests, answered through Pane `index`'s
    /// forms: the Pane that holds the Thread, so a compact view of an agent
    /// and the Pane showing it fill in one form.
    pub(super) fn decision_inputs(
        &self,
        index: usize,
        subject: &Subject,
        overlay: bool,
        short: bool,
        cx: &gpui::App,
    ) -> Option<Cards> {
        let pane = self.panes.get(index)?;
        let thread = pane.thread()?;
        let open = self.cockpit.read(cx).thread(thread)?;
        let activity = open.activity();
        let requests: Vec<(PendingDecision, bool)> = activity
            .pending_decisions()
            .iter()
            .filter(|request| shown_for(subject, request))
            .map(|request| {
                // An async question asks while its agent works on.
                let working = !request.decision.blocks_execution()
                    && request
                        .subject
                        .as_ref()
                        .and_then(|subject| activity.subject(subject))
                        .is_some_and(|subject| subject.busy());
                (request.clone(), working)
            })
            .collect();
        if requests.is_empty() {
            return None;
        }
        // The edit each approval asks for, where its gated call already
        // names it (Codex's `fileChange` carries its patch on the item, not
        // on the request).
        let asked: Vec<(String, ferrite_core::transcript::Diff)> = requests
            .iter()
            .filter(|(request, _)| !request.decision.tool_use_id.is_empty())
            .filter_map(|(request, _)| {
                let id = &request.decision.tool_use_id;
                let subject = request.subject.clone().unwrap_or(Subject::Main);
                let view = activity.subject(&subject)?;
                view.transcript()
                    .blocks()
                    .iter()
                    .rev()
                    .find_map(|block| match &block.body {
                        ferrite_core::transcript::Body::Tool(tool)
                            if ferrite_pane::transcript::gated(&tool.call, id) =>
                        {
                            tool.diffs.first().map(|diff| (id.clone(), diff.clone()))
                        }
                        _ => None,
                    })
            })
            .collect();
        let (provider, context) = self.decision_context(thread, cx);
        // Claude's `Allow and switch to Auto`: the provider's own auto mode
        // (`Settings::auto_mode_for`), when this Session offers it and it
        // is not in force already.
        let auto = ferrite_core::settings::Settings::auto_mode_for(open.provider());
        let switch = open
            .supports_control(ferrite_core::ControlKind::SetPermissionMode)
            .then(|| open.permission_modes())
            .and_then(|modes| modes.into_iter().find(|mode| mode.value == auto))
            .filter(|_| open.permission_mode() != Some(auto))
            .map(|mode| {
                (
                    mode.value,
                    SharedString::from(mode.label),
                    open.generation(),
                )
            });
        Some(Cards {
            switch,
            owner: self.decisions.downgrade(),
            asked,
            thread,
            provider,
            context,
            workspace: open
                .workspace()
                .map(|workspace| workspace.cwd().to_path_buf()),
            reading: self.prefs.settings.reading_size,
            rich: pane.read(cx).rich.clone(),
            forms: self.decisions.read(cx).forms(pane.identity)?.clone(),
            request_error: pane.read(cx).request_error.clone(),
            short,
            overlay,
            narrow: false,
            inset: theme::tx_inset_at(0., true),
            requests,
            kept_in: None,
        })
    }

    pub(super) fn send_form(
        &mut self,
        thread: ThreadKey,
        handle: &DecisionHandle,
        fields: &[ferrite_core::FormField],
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let pane = self.panes[index].identity;
        let Some(values) = self
            .decisions
            .read(cx)
            .forms(pane)
            .and_then(|forms| forms.form_values(handle, fields, cx))
        else {
            return;
        };
        if let Err(error) = ferrite_core::validate_form(fields, &values) {
            let value = Some((handle.clone(), error));
            self.panes[index].update(cx, |pane, _| pane.request_error = value);
            cx.notify();
            return;
        }
        self.respond_exact(thread, handle, DecisionAnswer::Form { values }, cx);
    }

    pub(super) fn clear_request_error(&mut self, thread: ThreadKey, handle: &DecisionHandle, cx: &mut gpui::App) {
        if let Some(index) = self.pane_for(thread) {
            if self.panes[index]
                .read(cx).request_error
                .as_ref()
                .is_some_and(|(failed, _)| failed == handle)
            {
                let value = None;
                self.panes[index].update(cx, |pane, _| pane.request_error = value);
            }
        }
    }

    /// Send a question form: every question needs a pick or typed words.
    /// `quiet` (the ↵ path) leaves an incomplete form alone instead of
    /// flagging it. Returns whether an answer went out.
    pub(super) fn send_question_form(
        &mut self,
        thread: ThreadKey,
        handle: &DecisionHandle,
        quiet: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.pane_for(thread) else {
            return false;
        };
        let pane = self.panes[index].identity;
        let Some(answers) = self
            .decisions
            .read(cx)
            .forms(pane)
            .and_then(|forms| forms.question_answers(handle, cx))
        else {
            return false;
        };
        if !decision::answered(&answers) {
            if !quiet {
                let error = (handle.clone(), "Answer each question before sending.".into());
                self.panes[index].update(cx, |pane, _| pane.request_error = Some(error));
                cx.notify();
            }
            return false;
        }
        self.respond_exact(thread, handle, DecisionAnswer::Questions { answers }, cx);
        true
    }

    /// ↵ on an empty Main Composer line: send the question this Pane shows
    /// when every one of its questions is answered. False leaves the key to
    /// the Composer (an empty line's ↵ takes a held prompt back).
    pub(super) fn send_ready_question(&mut self, cx: &mut Context<Self>) -> bool {
        let index = self.focused(cx);
        let Some(request) = self.shown_request(index, cx) else {
            return false;
        };
        if request.submitting || pane::questions_of(&request.decision).is_none() {
            return false;
        }
        let thread = self.panes[index].thread().expect("a request has a Thread");
        self.send_question_form(thread, &request.handle, true, cx)
    }

    /// The first request this Pane's selected Subject shows — the one its
    /// digit keys pick in.
    pub(super) fn shown_request(&self, index: usize, cx: &gpui::App) -> Option<PendingDecision> {
        let pane = self.panes.get(index)?;
        self.cockpit
            .read(cx)
            .thread(pane.thread()?)?
            .pending_decisions()
            .iter()
            .find(|request| shown_on(pane.read(cx), request))
            .cloned()
    }

    pub(super) fn retry_subject_history(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(thread) = self.panes[index].thread() else {
            return;
        };
        let subject = self.panes[index].read(cx).selected.clone();
        let value = self.load_subject_history(thread, &subject, cx);
        self.panes[index].update(cx, |pane, _| pane.history_error = value);
        cx.notify();
    }

    /// Ask core for `subject`'s saved history (it never resumes the
    /// provider); the reason it could not be had.
    pub(super) fn load_subject_history(&mut self, thread: ThreadKey, subject: &Subject, cx: &mut gpui::App) -> Option<String> {
        self.cockpit.update(cx, |cockpit, _| {
            cockpit
                .ensure_subject_history(thread, subject)
                .err()
                .map(|error| error.to_string())
                .or_else(|| cockpit.subject_history_error(thread, subject).map(str::to_string))
        })
    }

    /// What the cards of Pane `index` kept goes with each Decision its
    /// Thread no longer waits on.
    pub(super) fn prune_request_forms(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(thread) = self.panes[index]
            .thread()
            .and_then(|id| self.cockpit.read(cx).thread(id))
        else {
            return;
        };
        let pending = thread.pending_decisions();
        let pane = self.panes[index].identity;
        if let Some(forms) = self.decisions.read(cx).forms(pane) {
            forms.retain(|handle| pending.iter().any(|request| &request.handle == handle));
        }
    }

    pub(super) fn answer_subject(&mut self, answer: Answer, cx: &mut Context<Self>) {
        let index = self.focused(cx);
        let Some(thread) = self.panes.get(index).and_then(|pane| pane.thread()) else {
            return;
        };
        let Some(request) =
            self.cockpit
                .read(cx)
                .thread(thread)
                .and_then(|thread| {
                    thread.pending_decisions().iter().find(|request| {
                        request.subject.as_ref() == Some(&self.panes[index].read(cx).selected)
                    })
                })
                .cloned()
        else {
            return;
        };
        self.answer_request(thread, request, answer, cx);
    }

    /// An approval takes its option (allow 1, always 2, deny 3) through the
    /// one answer path; a question, form or link takes only a deny.
    pub(super) fn answer_request(
        &mut self,
        thread: ThreadKey,
        request: PendingDecision,
        answer: Answer,
        cx: &mut Context<Self>,
    ) {
        if matches!(request.decision.kind, ferrite_core::DecisionKind::Approval) {
            let choice = match answer {
                Answer::Allow => decision::ApprovalChoice::Allow,
                Answer::Always => decision::ApprovalChoice::AllowForThread,
                Answer::Deny => decision::ApprovalChoice::DenyAndSteer,
            };
            self.answer_approval_request(thread, &request, choice, None, None, cx);
            return;
        }
        if answer != Answer::Deny || !request.decision.policy.deny {
            return;
        }
        self.respond_exact(
            thread,
            &request.handle,
            DecisionAnswer::Deny {
                message: "The operator denied this tool.".into(),
            },
            cx,
        );
    }
}
