//! Subject navigation and supervision. Provider execution stays in core.
use super::*;
use ferrite_core::activity::{
    ActivityView, AgentInfo, AgentStatus, DecisionHandle, PendingDecision, Subject,
};
use super::decisions::DecisionEvent;
use ferrite_pane::decision;
use ferrite_ui::components::MenuButton as _;
use ferrite_ui::theme;
use gpui::component::{
    checkbox::Checkbox,
    input::{Input, InputState},
    radio::{Radio, RadioGroup},
    scroll::ScrollableElement,
    Disableable,
};
use gpui::KeyDownEvent;
use std::{cell::{Cell, RefCell}, collections::HashMap, rc::Rc};

/// A Pane's Decision state, shared with the cards it draws: each question
/// or MCP form's answers, and each Decision's cursor and amend note.
#[derive(Clone, Default)]
pub(crate) struct RequestForms(
    Rc<RefCell<HashMap<DecisionHandle, RequestForm>>>,
    pub(super) Rc<RefCell<HashMap<DecisionHandle, DecisionCursor>>>,
);

/// One Decision's keyboard state (D-5, D-6): the option under the `❯`
/// once the operator moved it (the first option that can act until then),
/// and the amend note while it is open.
#[derive(Default)]
pub(crate) struct DecisionCursor {
    pub(super) index: Option<usize>,
    pub(super) note: Option<Entity<ferrite_pane::composer::Composer>>,
    /// The approval's split-button menu is open.
    pub(super) menu: bool,
}

impl RequestForms {
    /// Where `handle`'s cursor was moved, if it was.
    pub(super) fn cursor(&self, handle: &DecisionHandle) -> Option<usize> {
        self.1.borrow().get(handle).and_then(|cursor| cursor.index)
    }

    pub(super) fn set_cursor(&self, handle: &DecisionHandle, index: Option<usize>) {
        self.1.borrow_mut().entry(handle.clone()).or_default().index = index;
    }

    /// Whether `handle`'s split-button menu is open.
    pub(super) fn menu(&self, handle: &DecisionHandle) -> bool {
        self.1
            .borrow()
            .get(handle)
            .is_some_and(|cursor| cursor.menu)
    }

    pub(super) fn set_menu(&self, handle: &DecisionHandle, open: bool) {
        self.1.borrow_mut().entry(handle.clone()).or_default().menu = open;
    }

    /// `handle`'s amend note, while it is open.
    pub(super) fn note(
        &self,
        handle: &DecisionHandle,
    ) -> Option<Entity<ferrite_pane::composer::Composer>> {
        self.1
            .borrow()
            .get(handle)
            .and_then(|cursor| cursor.note.clone())
    }

    pub(super) fn set_note(
        &self,
        handle: &DecisionHandle,
        note: Option<Entity<ferrite_pane::composer::Composer>>,
    ) {
        self.1.borrow_mut().entry(handle.clone()).or_default().note = note;
    }

    /// A question form's answers so far, once its card has drawn.
    pub(super) fn answers(
        &self,
        handle: &DecisionHandle,
    ) -> Option<Vec<ferrite_core::questions::Answer>> {
        self.0.borrow().get(handle).map(|form| form.answers.clone())
    }

    /// `handle`'s amend note's words, taken: the note closes.
    pub(super) fn take_note(&self, handle: &DecisionHandle, cx: &mut gpui::App) -> Option<String> {
        let note = self.note(handle)?;
        self.set_note(handle, None);
        Some(note.update(cx, |line, cx| line.take(cx)))
    }

    /// The Decisions `keep` refuses take what their cards kept with them.
    pub(super) fn retain(&self, keep: impl Fn(&DecisionHandle) -> bool) {
        self.0.borrow_mut().retain(|handle, _| keep(handle));
        self.1.borrow_mut().retain(|handle, _| keep(handle));
    }

    /// A question form's answers as they stand, each with the words typed
    /// under it; None while its card has not drawn.
    pub(super) fn question_answers(
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
    pub(super) fn form_values(
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
    pub(super) fn pick(
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
pub(super) fn answered(answers: &[ferrite_core::questions::Answer]) -> bool {
    answers
        .iter()
        .all(|answer| !answer.picks.is_empty() || answer.other.is_some())
}

/// What a digit did on a question form.
pub(super) enum Pick {
    /// Nothing here takes it: it types.
    Untaken,
    /// It answers the lone single-select question as it is.
    Answers,
    /// It picked (or for a multi-select toggled) the option.
    Picked,
    /// It is the words field's, which takes the keyboard.
    Words(FocusHandle),
}

struct RequestForm {
    answers: Vec<ferrite_core::questions::Answer>,
    inputs: Vec<Entity<InputState>>,
    form_inputs: HashMap<String, Entity<InputState>>,
    values: serde_json::Map<String, serde_json::Value>,
    scroll: ScrollHandle,
}

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

pub(super) fn init(cx: &mut gpui::App) {
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

/// The question digit keys pick in: the first with options and no pick
/// yet, else the first with options (a multi-select keeps toggling).
pub(super) fn digit_question(
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
fn pick_option(picks: &mut Vec<usize>, option: usize, multi: bool) {
    if !multi {
        *picks = vec![option];
    } else if picks.contains(&option) {
        picks.retain(|at| *at != option);
    } else {
        picks.push(option);
    }
}

/// The word Lineage says of an agent: its state in the shared lexicon
/// (`theme::words`), or `done` for an idle or unclassified one.
pub(super) fn state_word(status: AgentStatus, fresh: bool) -> &'static str {
    use theme::words;
    if !fresh {
        return words::UNAVAILABLE;
    }
    match status {
        AgentStatus::Working => words::WORKING,
        AgentStatus::Waiting => words::NEEDS_YOU,
        AgentStatus::Pending => words::STARTING,
        AgentStatus::Paused => words::PAUSED,
        AgentStatus::Interrupted | AgentStatus::Shutdown => words::INTERRUPTED,
        AgentStatus::Failed | AgentStatus::NotFound => words::FAILED,
        AgentStatus::NotLoaded => words::UNAVAILABLE,
        AgentStatus::Idle | AgentStatus::Unknown => words::DONE,
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
        if self.panes[index].is_main() {
            window.focus(&self.panes[index].composer.read(cx).focus_handle(cx), cx);
        } else {
            window.focus(&self.panes[index].transcript_focus, cx);
        }
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
        let selected = &self.panes[index].selected;
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
        self.panes[index].select_subject(subject, generation, cx);
        self.panes[index].details.reset_pages();
        self.retry_subject_history(index, cx);
        self.cockpit
            .update(cx, |cockpit, _| cockpit.set_visible_subject(thread, self.panes[index].selected.clone()));
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
        match self.cockpit.update(cx, |cockpit, _| cockpit.respond_decision(thread, handle, answer)) {
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
                    self.panes[index].request_error = None;
                }
            }
            Ok(false) => {}
            Err(error) => {
                if let Some(index) = self.pane_for(thread) {
                    self.panes[index].request_error = Some((handle.clone(), error.to_string()));
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
        let subject = self.panes.get(index)?.selected.clone();
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
            rich: pane.rich.clone(),
            forms: self.decisions.read(cx).forms(pane.identity)?.clone(),
            request_error: pane.request_error.clone(),
            short,
            overlay,
            narrow: false,
            inset: theme::tx_inset_at(0., true),
            requests,
        })
    }

    pub(super) fn activity_decisions(
        &self,
        index: usize,
        _joined: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // Below the short-Pane height a question body keeps two option rows
        // and scrolls, so its head and answer row stay in reach.
        let rect = self
            .pane_rects(window, cx)
            .into_iter()
            .find(|(at, _)| *at == index)
            .map(|(_, rect)| rect);
        let short = rect.is_some_and(|rect| rect.h < theme::DECISION_SHORT_PANE_H);
        let mut cards = self.decision_cards(index, true, short, cx)?;
        if let Some(rect) = rect {
            cards.inset = theme::tx_inset_at(rect.w, true);
        }
        cards.render(window, cx)
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
            self.panes[index].request_error = Some((handle.clone(), error));
            cx.notify();
            return;
        }
        self.respond_exact(thread, handle, DecisionAnswer::Form { values }, cx);
    }

    pub(super) fn clear_request_error(&mut self, thread: ThreadKey, handle: &DecisionHandle) {
        if let Some(index) = self.pane_for(thread) {
            if self.panes[index]
                .request_error
                .as_ref()
                .is_some_and(|(failed, _)| failed == handle)
            {
                self.panes[index].request_error = None;
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
        if !answered(&answers) {
            if !quiet {
                self.panes[index].request_error = Some((
                    handle.clone(),
                    "Answer each question before sending.".into(),
                ));
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
            .find(|request| shown_on(pane, request))
            .cloned()
    }

    pub(super) fn retry_subject_history(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(thread) = self.panes[index].thread() else {
            return;
        };
        let subject = self.panes[index].selected.clone();
        self.panes[index].history_error = self.load_subject_history(thread, &subject, cx);
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
                        request.subject.as_ref() == Some(&self.panes[index].selected)
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

/// What a Pane's Decision cards are drawn from: a snapshot of the cockpit
/// taken when they are asked for, so the transcript — a cached view — draws
/// them without reading the cockpit (a read would re-render the transcript
/// on every cockpit change). Every press goes back through `owner`, as a
/// `DecisionEvent` the window hears.
#[derive(Clone)]
pub(super) struct Cards {
    pub(super) owner: gpui::WeakEntity<decisions::DecisionsView>,
    /// The diff each approval's gated call names, by the call's id.
    pub(super) asked: Vec<(String, ferrite_core::transcript::Diff)>,
    pub(super) thread: ThreadKey,
    pub(super) provider: Option<Provider>,
    /// The head's last detail: Codex's sandbox, Claude's permission mode.
    pub(super) context: Option<SharedString>,
    /// The mode an approval's `Allow and switch to …` moves the Session to
    /// (its value and label), with the Session generation it was read on;
    /// None where the Session cannot switch or is there already.
    pub(super) switch: Option<(String, SharedString, u64)>,
    pub(super) workspace: Option<std::path::PathBuf>,
    /// The transcript's reading size: an edit approval's diff is drawn at it.
    pub(super) reading: ferrite_core::settings::ReadingSize,
    pub(super) rich: ferrite_pane::rich::TextCache,
    pub(super) forms: RequestForms,
    pub(super) request_error: Option<(DecisionHandle, String)>,
    /// The overlay's Pane is short: a question body keeps two rows.
    pub(super) short: bool,
    /// Drawn in the requests overlay, on the plane, rather than in the
    /// transcript.
    pub(super) overlay: bool,
    /// Drawn in the compact inspector's narrow column: the approval's
    /// actions stack, and no pill names the wait.
    pub(super) narrow: bool,
    /// The overlay's inset on the transcript's axes: the column's at the
    /// Pane's width (`theme::tx_inset_at`).
    pub(super) inset: f32,
    /// The requests shown, each with whether its agent works on meanwhile.
    pub(super) requests: Vec<(PendingDecision, bool)>,
}

/// Ask the cockpit to draw again (a pick that lives in the Pane's forms):
/// the window redraws with the Decisions' view.
fn refresh(owner: &gpui::WeakEntity<decisions::DecisionsView>, cx: &mut gpui::App) {
    let _ = owner.update(cx, |_, cx| cx.notify());
}

/// Tell the window what a press asks.
fn ask(
    owner: &gpui::WeakEntity<decisions::DecisionsView>,
    event: DecisionEvent,
    cx: &mut gpui::App,
) {
    let _ = owner.update(cx, |_, cx| cx.emit(event));
}

impl Cards {
    /// Everything the drawn cards depend on, for the transcript's cache
    /// key: the requests and their sending state, the cursors and notes,
    /// the picks, the errors and the head's words.
    pub(super) fn key(&self) -> u64 {
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
    pub(super) fn render(&self, window: &mut Window, cx: &mut gpui::App) -> Option<AnyElement> {
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

    /// An approval (D-1…D-7), as Claude's card: the ask on the head band,
    /// the prose, the command band (or the edit's diff), the amend note
    /// while it is open, then `Deny` and the split `Allow once` whose menu
    /// holds `Always allow <pattern>` (its note says what that covers) and
    /// `Allow and switch to …` — inline, or stacked in the compact
    /// inspector's narrow column (`narrow`). The keys are on the buttons (⌘⏎ on `Allow once`, ⇧⌘⏎
    /// on the menu's `Always allow`, esc); 1–3, ↑↓ and ⏎ still act.
    fn approval_card(&self, request: PendingDecision, cx: &gpui::App) -> AnyElement {
        let thread = self.thread;
        let handle = request.handle.clone();
        let serial = handle.serial;
        let (head, title, error) = self.preamble(&request);
        let mut children = vec![head.into_any_element()];
        // A short Pane gives up the prose first — the head and the command
        // still say what is asked.
        children.extend(
            title
                .filter(|_| !self.short)
                .map(|title| decision::section(title).into_any_element()),
        );
        let asked = self
            .asked
            .iter()
            .find(|(id, _)| *id == request.decision.tool_use_id)
            .map(|(_, diff)| diff);
        if let Some(diff) = pane::approval_diff(
            &request.decision,
            asked,
            self.workspace.as_deref(),
            self.reading,
        ) {
            children.push(decision::section(diff).into_any_element());
        } else if let Some(input) = pane::approval_input(
            &request.decision,
            &self.rich,
            format!(
                "approval-input-{}-{}-{}-{}",
                if self.overlay { "overlay" } else { "tail" },
                thread.thread().get(),
                handle.generation,
                serial
            )
            .into(),
        ) {
            children.push(
                decision::section(
                    decision::well(pane::shell_command(&request.decision), input)
                        .debug_selector(|| "approval-well".into()),
                )
                .into_any_element(),
            );
        }
        children.extend(error.map(|error| {
            decision::section(decision::error_line("could not send", error)).into_any_element()
        }));
        let rows =
            decision::approval_rows(&request.decision, self.provider, self.workspace.as_deref());
        let enabled = |choice: decision::ApprovalChoice| {
            !request.submitting && rows.iter().any(|row| row.choice == choice && row.enabled)
        };
        if let Some(note) = self.forms.note(&handle) {
            children.push(self.note_field(&handle, note));
        }
        let answer = |choice: decision::ApprovalChoice| {
            let pick = request.clone();
            self.act(move || DecisionEvent::Approve {
                thread,
                request: pick.clone(),
                choice,
                switch: None,
            })
        };
        let stacked = self.narrow;
        let deny = decision::deny_button(
            SharedString::from(format!("request-deny-{}-{serial}", thread.thread().get())),
            stacked,
            !enabled(decision::ApprovalChoice::DenyAndSteer),
            cx,
        )
        .debug_selector(move || format!("request-deny-{}-{serial}", thread.thread().get()))
        .on_click(answer(decision::ApprovalChoice::DenyAndSteer))
        .relative()
        // The deny control has always answered to this name.
        .child(
            div()
                .absolute()
                .inset_0()
                .debug_selector(|| "decision-deny".into()),
        );
        let allow = decision::allow_button(
            SharedString::from(format!("request-allow-{}-{serial}", thread.thread().get())),
            stacked,
            !enabled(decision::ApprovalChoice::Allow),
            cx,
        )
        .debug_selector(move || format!("request-allow-{}-{serial}", thread.thread().get()))
        .on_click(answer(decision::ApprovalChoice::Allow));
        let open = self.forms.menu(&handle);
        let forms = self.forms.clone();
        let toggled = handle.clone();
        let owner = self.owner.clone();
        // Where the `▾` was laid out: a press there is its own, which shuts
        // the menu, not a press outside it.
        let door: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>> = Rc::default();
        let submitting = request.submitting;
        let more = decision::more_button(
            SharedString::from(format!("request-more-{}-{serial}", thread.thread().get())),
            submitting,
            cx,
        )
        .debug_selector(move || format!("request-more-{}-{serial}", thread.thread().get()))
        .menu_button(
            open,
            |button| button,
            {
                let door = door.clone();
                move |bounds, _, _| door.set(Some(bounds))
            },
            move |_, cx| {
                if !submitting {
                    forms.set_menu(&toggled, !forms.menu(&toggled));
                    refresh(&owner, cx);
                }
            },
        );
        let menu = open.then(|| {
            let mut rows = Vec::new();
            if enabled(decision::ApprovalChoice::AllowForThread) {
                let forms = self.forms.clone();
                let closing = handle.clone();
                let always = answer(decision::ApprovalChoice::AllowForThread);
                let (label, note) = decision::always_allow_words(&decision::option_two_pattern(
                    &request.decision,
                    self.workspace.as_deref(),
                ));
                rows.push(
                    decision::allow_menu_row(
                        SharedString::from(format!("request-always-{}-{serial}", thread.thread().get())),
                        label,
                        decision::decision_chord("cockpit::AlwaysAllow"),
                        Some(note),
                    )
                    .debug_selector(move || format!("request-always-{}-{serial}", thread.thread().get()))
                    .on_click(move |event, window, cx| {
                        forms.set_menu(&closing, false);
                        always(event, window, cx);
                    })
                    .into_any_element(),
                );
            }
            if let Some((mode, label, generation)) = self
                .switch
                .clone()
                .filter(|_| enabled(decision::ApprovalChoice::Allow))
            {
                let pick = request.clone();
                let forms = self.forms.clone();
                let closing = handle.clone();
                let what = ferrite_ui::theme::mode_note(&mode).map(SharedString::from);
                rows.push(
                    decision::allow_menu_row(
                        SharedString::from(format!("request-switch-{}-{serial}", thread.thread().get())),
                        format!("Allow and switch to {label}"),
                        None,
                        what,
                    )
                    .debug_selector(move || format!("request-switch-{}-{serial}", thread.thread().get()))
                    .on_click({
                        let act = self.act(move || DecisionEvent::Approve {
                            thread,
                            request: pick.clone(),
                            choice: decision::ApprovalChoice::Allow,
                            switch: Some((mode.clone(), generation)),
                        });
                        move |event, window, cx| {
                            forms.set_menu(&closing, false);
                            act(event, window, cx);
                        }
                    })
                    .into_any_element(),
                );
            }
            // A press outside closes it, as it closes every popup (ADR 0043).
            let forms = self.forms.clone();
            let closing = handle.clone();
            let owner = self.owner.clone();
            decision::allow_menu(rows)
                .on_mouse_down_out(move |event, _, cx| {
                    if !door.get().is_some_and(|door| door.contains(&event.position)) {
                        forms.set_menu(&closing, false);
                        refresh(&owner, cx);
                    }
                })
                .into_any_element()
        });
        children.push(
            decision::section(decision::approval_actions(
                deny.into_any_element(),
                allow.into_any_element(),
                more.into_any_element(),
                menu,
                stacked,
            ))
            .into_any_element(),
        );
        request_frame(
            decision::approval_card(serial, children),
            self.overlay,
            self.narrow,
            self.inset,
        )
        .into_any_element()
    }

    /// The amend note (⇥, D-6): the one-line field under the options. ⏎
    /// sends the cursor's option with it; esc closes it. Inside a child's
    /// `Decision` context the answer keys are bound — in the note they are
    /// letters again.
    fn note_field(
        &self,
        handle: &DecisionHandle,
        note: Entity<ferrite_pane::composer::Composer>,
    ) -> AnyElement {
        let thread = self.thread;
        let (send, confirm, close, dismiss) = (
            handle.clone(),
            handle.clone(),
            handle.clone(),
            handle.clone(),
        );
        let owner = self.owner.clone();
        let enter = handle.clone();
        let mut field = div()
            .w_full()
            .min_w_0()
            // The card's controls keep ↵ for themselves (`native_keys`), so
            // no binding reaches the note: it hears its own ↵.
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let keys = &event.keystroke;
                let plain = !(keys.modifiers.shift
                    || keys.modifiers.control
                    || keys.modifiers.alt
                    || keys.modifiers.platform);
                if keys.key == "enter" && plain {
                    cx.stop_propagation();
                    window.prevent_default();
                    let handle = enter.clone();
                    ask(&owner, DecisionEvent::SendNote { thread, handle }, cx);
                }
            })
            .on_action(self.act::<Submit>(move || DecisionEvent::SendNote {
                thread,
                handle: send.clone(),
            }))
            .on_action(self.act::<decision::Confirm>(move || DecisionEvent::SendNote {
                thread,
                handle: confirm.clone(),
            }))
            .on_action(self.act::<Interrupt>(move || DecisionEvent::CloseNote {
                thread,
                handle: close.clone(),
            }))
            .on_action(self.act::<decision::Dismiss>(move || DecisionEvent::CloseNote {
                thread,
                handle: dismiss.clone(),
            }));
        macro_rules! letter {
            ($action:ty, $text:expr) => {{
                let note = note.clone();
                field = field.on_action(move |_: &$action, _: &mut Window, cx: &mut gpui::App| {
                    note.update(cx, |line, cx| line.insert($text, cx));
                });
            }};
        }
        letter!(PickOption1, "1");
        letter!(PickOption2, "2");
        letter!(PickOption3, "3");
        letter!(PickOption4, "4");
        field.child(decision::note_row(note)).into_any_element()
    }

    fn form_card(
        &self,
        request: PendingDecision,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> AnyElement {
        let thread = self.thread;
        let handle = request.handle.clone();
        let ferrite_core::DecisionKind::Form { fields } = &request.decision.kind else {
            unreachable!("dispatched on the Form kind")
        };
        let fields = fields.clone();
        let (head, title, error) = self.preamble(&request);
        let forms = self.forms.clone();
        if !forms.0.borrow().contains_key(&handle) {
            let mut form_inputs = HashMap::new();
            for field in &fields {
                let initial = match &field.kind {
                    ferrite_core::FormFieldKind::String { default, .. } => {
                        default.clone().unwrap_or_default()
                    }
                    ferrite_core::FormFieldKind::Number { default, .. } => {
                        default.map(|value| value.to_string()).unwrap_or_default()
                    }
                    ferrite_core::FormFieldKind::Integer { default, .. } => {
                        default.map(|value| value.to_string()).unwrap_or_default()
                    }
                    _ => continue,
                };
                let input = cx.new(|cx| {
                    let mut input = InputState::new(window, cx);
                    input.set_value(initial, window, cx);
                    input
                });
                form_inputs.insert(field.id.clone(), input);
            }
            forms.0.borrow_mut().insert(
                handle.clone(),
                RequestForm {
                    answers: Vec::new(),
                    inputs: Vec::new(),
                    form_inputs,
                    values: form_defaults(&fields),
                    scroll: ScrollHandle::new(),
                },
            );
        }
        let submit_handle = handle.clone();
        let selector = format!("request-submit-{}-{}", thread.thread().get(), handle.serial);
        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(theme::DECISION_QUESTIONS_GAP));
        for (field_index, field) in fields.iter().enumerate() {
            let mut section = div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(theme::SPACE_1_5))
                .debug_selector({
                    let id = field.id.clone();
                    move || format!("form-field-{id}")
                })
                .child(
                    div()
                        .text_color(rgb(theme::INK_STRONG))
                        .child(field.label.clone()),
                );
            if !field.description.is_empty() {
                section = section.child(decision::note(field.description.clone()));
            }
            match &field.kind {
                ferrite_core::FormFieldKind::String { .. }
                | ferrite_core::FormFieldKind::Number { .. }
                | ferrite_core::FormFieldKind::Integer { .. } => {
                    if let Some(input) = forms.0.borrow()[&handle].form_inputs.get(&field.id) {
                        let id = field.id.clone();
                        section = section.child(
                            div()
                                .debug_selector(move || format!("form-input-{id}"))
                                .child(Input::new(input).disabled(request.submitting)),
                        );
                    }
                }
                ferrite_core::FormFieldKind::Boolean { .. } => {
                    let checked = forms.0.borrow()[&handle]
                        .values
                        .get(&field.id)
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false);
                    let forms = forms.clone();
                    let handle = handle.clone();
                    let id = field.id.clone();
                    let owner = self.owner.clone();
                    section = section.child(
                        Checkbox::new(("form-bool", field_index))
                            .checked(checked)
                            .disabled(request.submitting)
                            .child("Enabled")
                            .on_click(move |checked: &bool, _: &mut Window, cx: &mut gpui::App| {
                                if let Some(form) = forms.0.borrow_mut().get_mut(&handle) {
                                    form.values
                                        .insert(id.clone(), serde_json::Value::Bool(*checked));
                                }
                                refresh(&owner, cx);
                            }),
                    );
                }
                ferrite_core::FormFieldKind::Enum {
                    options,
                    multi_select,
                    ..
                } => {
                    let selected = forms.0.borrow()[&handle].values.get(&field.id).cloned();
                    if *multi_select {
                        for (option_index, option) in options.iter().enumerate() {
                            let checked = selected
                                .as_ref()
                                .and_then(|value| value.as_array())
                                .is_some_and(|values| {
                                    values.iter().any(|value| {
                                        value == &serde_json::Value::String(option.value.clone())
                                    })
                                });
                            let forms = forms.clone();
                            let handle = handle.clone();
                            let id = field.id.clone();
                            let value = option.value.clone();
                            let owner = self.owner.clone();
                            section = section.child(
                                Checkbox::new(("form-enum", field_index * 256 + option_index))
                                    .checked(checked)
                                    .disabled(request.submitting)
                                    .child(option.label.clone())
                                    .on_click(
                                        move |checked: &bool, _: &mut Window, cx: &mut gpui::App| {
                                            if let Some(form) =
                                                forms.0.borrow_mut().get_mut(&handle)
                                            {
                                                let values = form
                                                    .values
                                                    .entry(id.clone())
                                                    .or_insert_with(|| {
                                                        serde_json::Value::Array(Vec::new())
                                                    });
                                                let values = values
                                                    .as_array_mut()
                                                    .expect("form enum is an array");
                                                values.retain(|item| {
                                                    item != &serde_json::Value::String(
                                                        value.clone(),
                                                    )
                                                });
                                                if *checked {
                                                    values.push(serde_json::Value::String(
                                                        value.clone(),
                                                    ));
                                                }
                                            }
                                            refresh(&owner, cx);
                                        },
                                    ),
                            );
                        }
                    } else {
                        let selected_index = selected
                            .as_ref()
                            .and_then(|value| value.as_str())
                            .and_then(|value| {
                                options.iter().position(|option| option.value == value)
                            });
                        let forms = forms.clone();
                        let handle = handle.clone();
                        let id = field.id.clone();
                        let options = options.clone();
                        let owner = self.owner.clone();
                        section = section.child(
                            RadioGroup::vertical(("form-enum", field_index))
                                .selected_index(selected_index)
                                .disabled(request.submitting)
                                .children(options.iter().enumerate().map(|(index, option)| {
                                    Radio::new(index).child(option.label.clone())
                                }))
                                .on_click(
                                    move |selected: &usize, _: &mut Window, cx: &mut gpui::App| {
                                        if let (Some(form), Some(option)) = (
                                            forms.0.borrow_mut().get_mut(&handle),
                                            options.get(*selected),
                                        ) {
                                            form.values.insert(
                                                id.clone(),
                                                serde_json::Value::String(option.value.clone()),
                                            );
                                        }
                                        refresh(&owner, cx);
                                    },
                                ),
                        );
                    }
                }
            }
            body = body.child(section);
        }
        let cancel_handle = handle.clone();
        let mut children = vec![head.into_any_element()];
        children.extend(title.map(|title| decision::section(title).into_any_element()));
        let scroll = forms.0.borrow()[&handle].scroll.clone();
        children.push(
            div()
                .id(("form-body", handle.serial as usize))
                .w_full()
                .min_w_0()
                .flex_shrink_1()
                .max_h(px(theme::DECISION_BODY_MAX_H))
                .overflow_y_scroll()
                .track_scroll(&scroll)
                .vertical_scrollbar(&scroll)
                .pr(px(theme::DECISION_SCROLL_GUTTER))
                .child(body)
                .into_any_element(),
        );
        children.extend(error.map(|error| {
            decision::section(
                decision::error_line("not sent", error)
                    .debug_selector(|| "form-validation-error".into()),
            )
            .into_any_element()
        }));
        let sending = request.submitting;
        children.push(
            decision::section(decision::footer([
                decision::skip_button("form-cancel", "Cancel", cx)
                    .disabled(!request.decision.policy.deny || sending)
                    .debug_selector(|| "form-cancel".into())
                    .on_click(self.act(move || DecisionEvent::Respond {
                        thread,
                        handle: cancel_handle.clone(),
                        answer: DecisionAnswer::Cancel,
                    }))
                    .into_any_element(),
                decision::send_button(
                    "form-send",
                    if sending { "Sending…" } else { "Send" },
                    !request.decision.policy.allow || sending,
                    false,
                    cx,
                )
                .debug_selector(move || selector.clone())
                .on_click(self.act(move || DecisionEvent::SendForm {
                    thread,
                    handle: submit_handle.clone(),
                    fields: fields.clone(),
                }))
                .into_any_element(),
            ]))
            .into_any_element(),
        );
        request_frame(
            decision::card(handle.serial, children),
            self.overlay,
            self.narrow,
            self.inset,
        )
        .into_any_element()
    }

    /// A request finished outside Ferrite (`External`), or one it cannot
    /// answer here (`Unsupported`): the reason, and the actions it allows.
    fn link_card(&self, request: PendingDecision, cx: &mut gpui::App) -> AnyElement {
        let thread = self.thread;
        let handle = request.handle.clone();
        let (head, title, error) = self.preamble(&request);
        let sending = request.submitting;
        let mut children = vec![head.into_any_element()];
        children.extend(title.map(|title| decision::section(title).into_any_element()));
        let cancel_handle = handle.clone();
        let cancel = decision::skip_button(
            match &request.decision.kind {
                ferrite_core::DecisionKind::External { .. } => "external-cancel",
                _ => "unsupported-cancel",
            },
            "Cancel",
            cx,
        )
        .disabled(!request.decision.policy.deny || sending)
        .on_click(self.act(move || DecisionEvent::Respond {
            thread,
            handle: cancel_handle.clone(),
            answer: DecisionAnswer::Cancel,
        }))
        .into_any_element();
        let actions = match &request.decision.kind {
            ferrite_core::DecisionKind::External { url } => {
                children.push(
                    decision::section(decision::note(
                        "Complete this request in your browser, then confirm here.",
                    ))
                    .into_any_element(),
                );
                let url = url.clone();
                let complete_handle = handle.clone();
                vec![
                    decision::skip_button("external-open", "Open link", cx)
                        .disabled(!safe_external_url(&url))
                        .on_click(move |_, _, cx| cx.open_url(&url))
                        .into_any_element(),
                    cancel,
                    decision::send_button(
                        "external-complete",
                        "Complete",
                        !request.decision.policy.allow || sending,
                        false,
                        cx,
                    )
                    .on_click(self.act(move || DecisionEvent::Respond {
                        thread,
                        handle: complete_handle.clone(),
                        answer: DecisionAnswer::Allow {
                            input: serde_json::Value::Null,
                        },
                    }))
                    .into_any_element(),
                ]
            }
            ferrite_core::DecisionKind::Unsupported { reason } => {
                children.push(decision::section(decision::note(reason.clone())).into_any_element());
                vec![cancel]
            }
            _ => vec![cancel],
        };
        children.extend(error.map(|error| {
            decision::section(decision::error_line("could not send", error)).into_any_element()
        }));
        children.push(decision::section(decision::footer(actions)).into_any_element());
        request_frame(
            decision::card(handle.serial, children),
            self.overlay,
            self.narrow,
            self.inset,
        )
        .into_any_element()
    }

    /// A question (D-8): the form, in the Decision's row grammar — the `?`
    /// head with the question, numbered options with the `❯` cursor and its
    /// bar, the own-answer line, and the dim hint beside the form's Skip
    /// and Send.
    fn question_card(
        &self,
        request: PendingDecision,
        questions: Vec<ferrite_core::questions::Question>,
        working: bool,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> AnyElement {
        let thread = self.thread;
        let handle = request.handle.clone();
        let forms = self.forms.clone();
        if !forms.0.borrow().contains_key(&handle) {
            let inputs = questions
                .iter()
                .map(|question| {
                    cx.new(|cx| {
                        InputState::new(window, cx)
                            .placeholder("Or type your own answer\u{2026}")
                            .masked(question.secret)
                    })
                })
                .collect();
            forms.0.borrow_mut().insert(
                handle.clone(),
                RequestForm {
                    answers: vec![Default::default(); questions.len()],
                    inputs,
                    form_inputs: Default::default(),
                    values: Default::default(),
                    scroll: ScrollHandle::new(),
                },
            );
        }
        let sending = request.submitting;
        let answers = forms.0.borrow()[&handle].answers.clone();
        let scroll = forms.0.borrow()[&handle].scroll.clone();
        // The question the digit keys and the cursor pick in; only its rows
        // show digits.
        let target = digit_question(&answers, &questions);
        let cursor = target.map(|qi| {
            forms
                .cursor(&handle)
                .unwrap_or(0)
                .min(questions[qi].options.len().saturating_sub(1))
        });
        let single = questions.len() == 1;
        let overlay = self.overlay;
        let mut content = div()
            .id(("question-content", handle.serial as usize))
            .debug_selector(|| "question-scroll-content".into())
            .w_full()
            .min_w_0()
            .flex_shrink_1()
            .flex()
            .flex_col();
        if self.overlay {
            content = content
                .max_h(px(if self.short {
                    theme::DECISION_SHORT_BODY_MAX_H
                } else {
                    theme::DECISION_BODY_MAX_H
                }))
                .overflow_y_scroll()
                .track_scroll(&scroll)
                .vertical_scrollbar(&scroll)
                .pr(px(theme::DECISION_SCROLL_GUTTER));
        }
        for (qi, question) in questions.iter().enumerate() {
            let keyed = target == Some(qi);
            let digit = |at: usize| {
                (keyed && at < decision::DIGIT_KEYS)
                    .then(|| SharedString::from((at + 1).to_string()))
            };
            let mut section = div()
                .flex_shrink_0()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .when(qi > 0, |section| {
                    section.mt(px(theme::DECISION_QUESTIONS_GAP))
                });
            // A lone question is the head itself; several each lead their
            // own rows.
            if !single {
                section = section.child(
                    decision::question_text(question.question.clone())
                        .mb(px(theme::DECISION_QUESTION_GAP)),
                );
            }
            // The hint's `1–N pick` already says "choose any" for the
            // question the digits reach; another multi-select says it here.
            if question.multi_select && !keyed {
                section = section.child(decision::note("choose any"));
            }
            let mut rows = div().w_full().min_w_0().flex().flex_col();
            for (oi, option) in question.options.iter().enumerate() {
                let (label, recommended) = decision::split_recommended(&option.label);
                let pick_forms = forms.clone();
                let pick_handle = handle.clone();
                let multi = question.multi_select;
                rows = rows.child(
                    decision::option_row(
                        ("question-choice", qi * 256 + oi),
                        decision::Row {
                            key: digit(oi),
                            label: SharedString::from(label.to_string()),
                            description: Some(option.description.clone().into()),
                            recommended,
                            cursor: keyed && cursor == Some(oi),
                            picked: answers[qi].picks.contains(&oi),
                            enabled: !sending,
                            ..Default::default()
                        },
                    )
                    .debug_selector(move || format!("question-choice-{qi}-{oi}"))
                    .on_click({
                        let owner = self.owner.clone();
                        move |_, _, cx| {
                            if let Some(form) = pick_forms.0.borrow_mut().get_mut(&pick_handle) {
                                pick_option(&mut form.answers[qi].picks, oi, multi);
                            }
                            pick_forms.set_cursor(&pick_handle, Some(oi));
                            let handle = pick_handle.clone();
                            ask(&owner, DecisionEvent::Picked { thread, handle }, cx);
                        }
                    }),
                );
            }
            section = section.child(rows);
            // The option in hand shows its preview (a layout, a snippet) in
            // the command well's own style, as plain text: the cursor's
            // option, else the one picked last.
            let previewed = (keyed.then_some(cursor).flatten())
                .or_else(|| answers[qi].picks.last().copied())
                .and_then(|at| question.options.get(at))
                .and_then(|option| option.preview.as_deref())
                .filter(|preview| !preview.trim().is_empty());
            if let Some(preview) = previewed {
                section = section.child(
                    decision::well(false, preview.to_owned())
                        .debug_selector(move || format!("question-preview-{qi}"))
                        .mt(px(theme::DECISION_QUESTION_GAP)),
                );
            }
            if question.allow_other {
                // The own answer is not a second field: one bare line on
                // the options' grid — an option row high, its digit where
                // theirs stand, its text on their labels' column, in their
                // face — that the digit one past the options arms.
                let selector = format!("request-other-{}-{}-{qi}", thread.thread().get(), handle.serial);
                section = section.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .min_h(px(theme::DECISION_OPTION_H))
                        .flex()
                        .items_center()
                        .gap(px(theme::DECISION_OPTION_GAP))
                        .px(px(theme::DECISION_OPTION_PAD_X))
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(px(theme::FS_META * theme::CODE_ADVANCE))
                                .font_family(theme::FONT_CODE)
                                .text_size(px(theme::FS_META))
                                .text_color(rgb(theme::INK_SUBTLE))
                                .children(digit(question.options.len())),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .h(px(theme::QUESTION_OTHER_H))
                                .debug_selector(move || selector.clone())
                                .cursor_text()
                                .child(
                                    gpui::Styled::h(
                                        Input::new(&forms.0.borrow()[&handle].inputs[qi])
                                            .appearance(false)
                                            .disabled(sending),
                                        px(theme::QUESTION_OTHER_H),
                                    )
                                    .px(px(0.))
                                    .py(px(0.))
                                    .font_family(theme::FONT_UI)
                                    .text_size(px(theme::FS_UI))
                                    .text_color(rgb(theme::INK_STRONG)),
                                ),
                        ),
                );
            }
            content = content.child(section);
        }
        // A plain wait says nothing the card does not: only a status that
        // adds something rides the head.
        let status = if sending {
            Some(decision::sending().into_any_element())
        } else if working {
            Some(decision::status("work continues").into_any_element())
        } else if !request.decision.blocks_execution() {
            Some(decision::status("answer when ready").into_any_element())
        } else {
            None
        };
        let lead = if single {
            questions[0].question.clone()
        } else {
            format!("{} questions", questions.len())
        };
        let head = decision::question_head(
            lead,
            decision::head_detail([if request.subject.is_none() {
                "agent unknown"
            } else {
                ""
            }]),
            status,
        );
        let mut children = vec![
            head.into_any_element(),
            decision::section(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .min_h_0()
                    .overflow_hidden()
                    .debug_selector(|| "question-viewport".into())
                    .child(content),
            )
            .into_any_element(),
        ];
        if let Some(error) = request.reply_error.as_ref().or_else(|| {
            self.request_error
                .as_ref()
                .filter(|(failed, _)| failed == &handle)
                .map(|(_, error)| error)
        }) {
            children.push(
                decision::section(decision::error_line("not sent", error.clone()))
                    .into_any_element(),
            );
        }
        if let Some(note) = self.forms.note(&handle) {
            children.push(self.note_field(&handle, note));
        }
        // The form keeps its Skip and Send for the pointer (several
        // questions, several picks, typed words); the rows carry their digits.
        let skip_handle = handle.clone();
        let submit_handle = handle.clone();
        let selector = format!("request-submit-{}-{}", thread.thread().get(), handle.serial);
        children.push(
            decision::section(decision::footer([
                decision::skip_button("question-skip", "Skip", cx)
                    .disabled(sending)
                    .on_click(self.act(move || DecisionEvent::Respond {
                        thread,
                        handle: skip_handle.clone(),
                        answer: DecisionAnswer::Deny {
                            message: decision::QUESTION_SKIPPED.into(),
                        },
                    }))
                    .into_any_element(),
                decision::send_button(
                    "question-send",
                    if sending { "Sending\u{2026}" } else { "Send" },
                    sending,
                    !sending,
                    cx,
                )
                .debug_selector(move || selector.clone())
                .on_click(self.act(move || DecisionEvent::SendQuestions {
                    thread,
                    handle: submit_handle.clone(),
                }))
                .into_any_element(),
            ]))
            .into_any_element(),
        );
        request_frame(decision::card(handle.serial, children), overlay, self.narrow, self.inset)
            .into_any_element()
    }
}

fn safe_external_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

fn form_defaults(fields: &[ferrite_core::FormField]) -> serde_json::Map<String, serde_json::Value> {
    fields
        .iter()
        .filter_map(|field| {
            let value = match &field.kind {
                ferrite_core::FormFieldKind::String { default, .. } => {
                    default.clone().map(serde_json::Value::String)
                }
                ferrite_core::FormFieldKind::Number { default, .. } => default
                    .and_then(serde_json::Number::from_f64)
                    .map(serde_json::Value::Number),
                ferrite_core::FormFieldKind::Integer { default, .. } => {
                    default.map(serde_json::Value::from)
                }
                ferrite_core::FormFieldKind::Boolean { default } => default
                    .or(field.required.then_some(false))
                    .map(serde_json::Value::from),
                ferrite_core::FormFieldKind::Enum {
                    options,
                    multi_select,
                    default,
                    ..
                } => {
                    let allowed = |value: &str| options.iter().any(|option| option.value == value);
                    match default {
                        Some(serde_json::Value::String(value))
                            if !multi_select && allowed(value) =>
                        {
                            Some(serde_json::Value::String(value.clone()))
                        }
                        Some(serde_json::Value::Array(values)) if *multi_select => {
                            Some(serde_json::Value::Array(
                                values
                                    .iter()
                                    .filter(|value| value.as_str().is_some_and(allowed))
                                    .cloned()
                                    .collect(),
                            ))
                        }
                        _ => None,
                    }
                }
            }?;
            Some((field.id.clone(), value))
        })
        .collect()
}
