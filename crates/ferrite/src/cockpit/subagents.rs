//! Subject navigation and supervision. Provider execution stays in core.
use super::*;
use crate::{components, theme};
use ferrite_core::activity::{
    AgentInfo, AgentStatus, DecisionHandle, PendingDecision, Subject, TranscriptCoverage,
};
use ferrite_core::transcript::Status;
use gpui::component::{
    button::ButtonVariants,
    input::{Input, InputState},
    tab::{Tab, TabBar},
    Selectable, Sizable,
};
use gpui::{Animation, AnimationExt, KeyDownEvent};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Clone, Default)]
pub(crate) struct RequestForms(Rc<RefCell<HashMap<DecisionHandle, RequestForm>>>);
struct RequestForm {
    answers: Vec<ferrite_core::questions::Answer>,
    inputs: Vec<Entity<InputState>>,
}

#[derive(Clone, Default)]
pub(crate) struct TabInteraction(Rc<RefCell<TabInteractionState>>);
#[derive(Default)]
struct TabInteractionState {
    focus: HashMap<Subject, FocusHandle>,
    keyboard: Option<Subject>,
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

pub(crate) fn transcript_status(status: AgentStatus, fresh: bool) -> Status {
    if !fresh {
        return Status::Idle;
    }
    match status {
        AgentStatus::Working => Status::Streaming,
        AgentStatus::Waiting => Status::Blocked,
        AgentStatus::Failed | AgentStatus::Shutdown | AgentStatus::NotFound => Status::Closed,
        _ => Status::Idle,
    }
}

fn status_label(status: AgentStatus, fresh: bool) -> &'static str {
    if !fresh {
        return "Observation unavailable";
    }
    match status {
        AgentStatus::Working => "Working",
        AgentStatus::Waiting => "Needs input",
        AgentStatus::Idle => "Idle",
        AgentStatus::Pending => "Starting",
        AgentStatus::Paused => "Paused",
        AgentStatus::Interrupted => "Interrupted",
        AgentStatus::Failed => "Failed",
        AgentStatus::Shutdown => "Stopped",
        AgentStatus::NotFound | AgentStatus::NotLoaded => "Unavailable",
        AgentStatus::Unknown => "Status unknown",
    }
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

fn working_dots(animated: bool) -> AnyElement {
    let mut row = div()
        .flex()
        .items_center()
        .gap(px(2.))
        .w(px(14.))
        .h(px(12.));
    for index in 0usize..3 {
        let dot = div()
            .relative()
            .size(px(2.))
            .rounded_full()
            .bg(rgb(theme::RUNNING));
        row = row.child(if animated {
            dot.with_animation(
                ("working-dot", index),
                Animation::new(Duration::from_millis(650)).repeat(),
                move |dot, progress| {
                    let phase = progress * std::f32::consts::TAU - index as f32 * 0.7;
                    dot.top(px(-phase.sin().max(0.) * 3.))
                },
            )
            .into_any_element()
        } else {
            dot.into_any_element()
        });
    }
    row.into_any_element()
}

impl CockpitView {
    fn activate_subject(
        &mut self,
        thread: ThreadId,
        subject: Subject,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = matches!(event, ClickEvent::Keyboard(_))
            .then(|| window.focused(cx))
            .flatten();
        self.select_subject(thread, subject, window, cx);
        if let Some(focus) = focus {
            window.focus(&focus, cx);
        }
    }
    pub(super) fn select_subject(
        &mut self,
        thread: ThreadId,
        subject: Subject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        let subject = open.activity().canonical_subject(&subject);
        if open.activity().subject(&subject).is_none() {
            return;
        }
        let generation = open
            .activity()
            .subject(&subject)
            .map(|subject| subject.revision())
            .unwrap_or(0);
        self.panes[index].select_subject(subject, generation, cx);
        self.retry_subject_history(index, cx);
        self.cockpit.focus_thread(thread);
        self.focus_pane(index);
        self.popover = None;
        self.context_usage = None;
        // Keep native text selections in their retained Subject entities.
        // A tab selection must not edit or focus Main's hidden Composer.
        if self.panes[index].is_main() {
            window.focus(&self.panes[index].composer.read(cx).focus_handle(cx), cx);
        } else {
            window.focus(&self.panes[index].transcript_focus, cx);
        }
        cx.notify();
    }

    pub(super) fn subject_strip(
        &self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use gpui::base::ElementExt as _;
        let pane = &self.panes[index];
        let thread = pane.thread()?;
        let activity = self.cockpit.thread(thread)?.activity();
        let children = activity.children();
        if children.is_empty() {
            return None;
        }
        let measure = |text: &str| {
            let run = gpui::TextRun {
                len: text.len(),
                font: gpui::font(theme::FONT_UI),
                color: rgb(theme::TEXT_2).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            f32::from(
                window
                    .text_system()
                    .shape_line(text.to_string().into(), px(theme::FS_SM), &[run], None)
                    .width,
            )
        };
        let widths: Vec<f32> = children
            .iter()
            .map(|agent| {
                let working = agent.fresh() && agent.status() == AgentStatus::Working;
                let waiting = activity
                    .pending_decisions()
                    .iter()
                    .any(|request| request.subject.as_ref() == Some(&agent.subject()));
                measure(&agent_name(agent.info())).ceil().min(96.)
                    + 2.
                    + if working { 19. } else { 0. }
                    + if waiting { 9. } else { 0. }
            })
            .collect();
        // The slot is laid out after the real title, branch, attention and usage
        // controls. Match native Underline/XSmall's 10px inter-tab gap exactly.
        let available = pane.subject_strip_width;
        let mut visible_count = widths.len();
        let all_width = 24. + widths.iter().sum::<f32>() + widths.len() as f32 * 10.;
        if all_width > available {
            visible_count = 0;
            let mut used = 24.;
            for (at, width) in widths.iter().enumerate() {
                let remaining = widths.len() - at - 1;
                let overflow = if remaining > 0 {
                    10. + measure(&format!("+{remaining}")) + 8.
                } else {
                    0.
                };
                if used + 10. + width + overflow > available {
                    break;
                }
                used += 10. + width;
                visible_count = at + 1;
            }
        }
        let visible = &children[..visible_count];
        let mut order = vec![Subject::Main];
        order.extend(visible.iter().map(|agent| agent.subject()));
        let nav = Rc::new(order);
        let interaction = pane.tab_interaction.clone();
        let identity = format!("subject-tabs-{}-{:?}", thread.get(), nav);
        let mut tabs = TabBar::new(SharedString::from(identity))
            .underline()
            .xsmall()
            .h(px(26.))
            .last_empty_space(div().w_0());
        if let Some(at) = nav.iter().position(|subject| subject == &pane.selected) {
            tabs = tabs.selected_index(at);
        }
        let main = Tab::new()
            .aria_label("Main transcript")
            .tooltip(|window, cx| {
                gpui::component::tooltip::Tooltip::new("Main transcript").build(window, cx)
            })
            .w(px(24.))
            .debug_selector(move || format!("subject-main-{}", thread.get()))
            .child(
                div()
                    .w(px(17.))
                    .h(px(2.))
                    .rounded_full()
                    .bg(rgb(if pane.is_main() {
                        theme::TEXT_STRONG
                    } else {
                        theme::TEXT_MUTED
                    })),
            );
        tabs = tabs.child(self.subject_tab(
            main,
            thread,
            Subject::Main,
            0,
            nav.clone(),
            interaction.clone(),
            cx,
        ));
        for (at, agent) in visible.iter().enumerate() {
            let subject = agent.subject();
            let name = agent_name(agent.info());
            let waiting = activity
                .pending_decisions()
                .iter()
                .any(|request| request.subject.as_ref() == Some(&subject));
            let working = agent.fresh() && agent.status() == AgentStatus::Working;
            let mut content = div().flex().items_center().gap(px(5.)).min_w_0().child(
                div()
                    .max_w(px(96.))
                    .truncate()
                    .text_size(px(theme::FS_SM))
                    .child(name.clone()),
            );
            if working {
                content = content.child(working_dots(!cx.reduce_motion()));
            }
            if waiting {
                content =
                    content.child(div().size(px(4.)).rounded_full().bg(rgb(theme::ATTENTION)));
            }
            let selector = format!(
                "subject-agent-{}-{}",
                thread.get(),
                match &subject {
                    Subject::Subagent(key) => key.as_str(),
                    _ => "main",
                }
            );
            let tooltip = format!("{name} — {}", status_label(agent.status(), agent.fresh()));
            let tab = Tab::new()
                .aria_label(tooltip.clone())
                .w(px(widths[at]))
                .debug_selector(move || selector.clone())
                .child(content)
                .tooltip(move |window, cx| {
                    gpui::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                });
            tabs = tabs.child(self.subject_tab(
                tab,
                thread,
                subject,
                at + 1,
                nav.clone(),
                interaction.clone(),
                cx,
            ));
        }
        let mut strip = div()
            .id(("subject-strip", thread.get()))
            .font_family(theme::FONT_UI)
            .font_weight(FontWeight::NORMAL)
            .flex()
            .items_center()
            .flex_1()
            .min_w(px(58.))
            .h(px(26.))
            .debug_selector(move || format!("subject-strip-{}", thread.get()))
            .child(tabs);
        if visible_count < children.len() {
            let hidden = &children[visible_count..];
            let choices = hidden
                .iter()
                .map(|agent| components::Choice {
                    label: format!(
                        "{} — {}",
                        agent_name(agent.info()),
                        status_label(agent.status(), agent.fresh())
                    )
                    .into(),
                    icon: None,
                    checked: agent.subject() == pane.selected,
                    disabled: false,
                    section: false,
                })
                .collect();
            let subjects: Vec<_> = hidden.iter().map(|agent| agent.subject()).collect();
            let weak = cx.entity().downgrade();
            let picking = weak.clone();
            strip = strip.child(components::ChoiceMenu {
                id: format!("subject-overflow-{}", thread.get()).into(),
                trigger: components::button(("subject-overflow", thread.get()))
                    .text()
                    .tab_stop(true)
                    .h(px(24.))
                    .px(px(4.))
                    .ml(px(10.))
                    .rounded(px(0.))
                    .accessibility_label("More subagents")
                    .tooltip("More subagents")
                    .debug_selector(move || format!("subject-overflow-{}", thread.get()))
                    .label(format!("+{}", hidden.len())),
                choices,
                open: pane.agent_menu_open,
                return_focus: pane.transcript_focus.clone(),
                on_open: Rc::new(move |open, _, cx| {
                    let _ = weak.update(cx, |view, cx| {
                        if let Some(index) = view.pane_for(thread) {
                            view.panes[index].agent_menu_open = open;
                            cx.notify();
                        }
                    });
                }),
                on_pick: Rc::new(move |at, window, cx| {
                    if let Some(subject) = subjects.get(at) {
                        let _ = picking.update(cx, |view, cx| {
                            view.select_subject(thread, subject.clone(), window, cx)
                        });
                    }
                }),
            });
        }
        let weak = cx.entity().downgrade();
        let measured_width = pane.subject_strip_width;
        Some(
            native_keys(strip)
                .on_prepaint(move |bounds, window, cx| {
                    let width = f32::from(bounds.size.width);
                    if (measured_width - width).abs() > 0.5 {
                        let weak = weak.clone();
                        // Publish after paint so GPUI schedules the follow-up layout;
                        // notifying while this same entity paints can be coalesced.
                        window.defer(cx, move |_, cx| {
                            let _ = weak.update(cx, |view, cx| {
                                if let Some(index) = view.pane_for(thread) {
                                    view.panes[index].subject_strip_width = width;
                                    cx.notify();
                                }
                            });
                        });
                    }
                })
                .into_any_element(),
        )
    }

    fn subject_tab(
        &self,
        tab: Tab,
        thread: ThreadId,
        subject: Subject,
        at: usize,
        order: Rc<Vec<Subject>>,
        interaction: TabInteraction,
        cx: &mut Context<Self>,
    ) -> Tab {
        let focus = interaction
            .0
            .borrow_mut()
            .focus
            .entry(subject.clone())
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let clicked_subject = subject.clone();
        let keyboard = interaction.clone();
        let keyboard_subject = subject.clone();
        let release = interaction;
        let release_subject = subject.clone();
        let release_focus = focus.clone();
        tab.track_focus(&focus.clone().tab_index(0).tab_stop(true))
            .tab_stop(true)
            .focus_visible(|style| style.bg(rgb(theme::HOVER)))
            .on_click(cx.listener(move |view, event, window, cx| {
                // The TabBar identity includes the ordered Subjects. A reorder
                // discards native positional press state before any release.
                view.activate_subject(thread, clicked_subject.clone(), event, window, cx);
            }))
            .on_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                    if !event.is_held {
                        keyboard.0.borrow_mut().keyboard = Some(keyboard_subject.clone());
                    }
                    cx.stop_propagation();
                    window.prevent_default();
                    return;
                }
                let target = match event.keystroke.key.as_str() {
                    "left" => Some(at.saturating_sub(1)),
                    "right" => Some((at + 1).min(order.len() - 1)),
                    "home" => Some(0),
                    "end" => Some(order.len() - 1),
                    _ => None,
                };
                if let Some(target) = target {
                    cx.stop_propagation();
                    window.prevent_default();
                    let subject = order[target].clone();
                    let focus = keyboard.0.borrow().focus.get(&subject).cloned();
                    view.select_subject(thread, subject, window, cx);
                    if let Some(focus) = focus {
                        window.focus(&focus, cx);
                    }
                }
            }))
            .on_key_up(
                cx.listener(move |view, event: &gpui::KeyUpEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "space" | "enter") {
                        let pressed = release.0.borrow_mut().keyboard.take();
                        if pressed.as_ref() == Some(&release_subject) {
                            view.select_subject(thread, release_subject.clone(), window, cx);
                            window.focus(&release_focus, cx);
                        }
                        cx.stop_propagation();
                        window.prevent_default();
                    }
                }),
            )
    }

    pub(super) fn activity_title(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let pane = &self.panes[index];
        let thread = pane.thread().expect("Thread Pane");
        if pane.is_main() {
            return self.pane_title(index, thread, cx);
        }
        let label = self
            .cockpit
            .thread(thread)
            .and_then(|thread| {
                thread
                    .activity()
                    .children()
                    .into_iter()
                    .find(|agent| agent.subject() == pane.selected)
                    .map(|agent| agent_name(agent.info()))
            })
            .unwrap_or_else(|| "Subagent".into());
        div()
            .truncate()
            .debug_selector(move || format!("subject-title-{}", thread.get()))
            .child(label)
            .into_any_element()
    }

    pub(super) fn activity_attention(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pane = &self.panes[index];
        let thread = pane.thread()?;
        let activity = self.cockpit.thread(thread)?.activity();
        let children = activity.children();
        if children.is_empty() {
            return None;
        }
        let pending = activity.pending_decisions();
        if pending.is_empty() {
            return None;
        }
        let text = String::from("Jump to next request");
        let target = next_request(
            pending
                .iter()
                .map(|request| request.subject.clone().unwrap_or(Subject::Main)),
            Some(&pane.selected),
        )?;
        Some(
            native_keys(
                components::button(("agent-attention", thread.get()))
                    .tab_stop(true)
                    .debug_selector(move || format!("agent-attention-{}", thread.get()))
                    .text()
                    .size(px(20.))
                    .p_0()
                    .rounded(px(0.))
                    .accessibility_label(text.clone())
                    .tooltip(text)
                    .child(div().size(px(5.)).rounded_full().bg(rgb(theme::ATTENTION)))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.select_subject(thread, target.clone(), window, cx)
                    })),
            )
            .into_any_element(),
        )
    }

    pub(super) fn child_footer(&self, index: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pane = &self.panes[index];
        if pane.is_main() {
            return None;
        }
        let thread = pane.thread()?;
        let subject = self
            .cockpit
            .thread(thread)?
            .activity()
            .subject(&pane.selected)?;
        let coverage = if !subject.retained() {
            "Loading saved transcript…"
        } else {
            match subject.coverage() {
                TranscriptCoverage::Unavailable => "Transcript unavailable",
                TranscriptCoverage::ToolActivity => "Tool activity only",
                TranscriptCoverage::Live => "Live transcript · earlier messages may be unavailable",
                TranscriptCoverage::Partial => "Partial transcript",
                TranscriptCoverage::Complete => "Subagent transcript",
            }
        };
        let error = pane.history_error.clone();
        Some(
            native_keys(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .px(px(theme::PANE_PAD_X))
                    .py(px(10.))
                    .bg(rgb(theme::RAISED))
                    .child(div().min_w_0().truncate().child(components::label(
                        error.as_deref().unwrap_or(coverage),
                        if error.is_some() {
                            theme::BLOCKED
                        } else {
                            theme::TEXT_MUTED
                        },
                    )))
                    .when(error.is_some(), |footer| {
                        footer.child(
                            components::button(("retry-child-history", thread.get()))
                                .tab_stop(true)
                                .label("Retry")
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.reload_subject_history(thread, cx)
                                })),
                        )
                    })
                    .child(
                        components::button(("return-main", thread.get()))
                            .tab_stop(true)
                            .label("Return to Main")
                            .debug_selector(move || format!("return-main-{}", thread.get()))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.select_subject(thread, Subject::Main, window, cx)
                            })),
                    ),
            )
            .into_any_element(),
        )
    }

    pub(super) fn respond_exact(
        &mut self,
        thread: ThreadId,
        handle: &DecisionHandle,
        answer: DecisionAnswer,
        cx: &mut Context<Self>,
    ) {
        match self.cockpit.respond_decision(thread, handle, answer) {
            Ok(true) => {
                if let Some(index) = self.pane_for(thread) {
                    self.panes[index]
                        .request_forms
                        .0
                        .borrow_mut()
                        .remove(handle);
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
        self.facts.acted(&self.cockpit, thread);
        cx.notify();
    }

    pub(super) fn activity_decisions(
        &self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pane = &self.panes[index];
        let thread = pane.thread()?;
        let activity = self.cockpit.thread(thread)?.activity();
        let all = activity.pending_decisions();
        if pane.is_main()
            && all.len() <= 1
            && all
                .iter()
                .all(|request| request.subject == Some(Subject::Main))
        {
            return None;
        }
        let requests: Vec<_> = all
            .iter()
            .filter(|request| {
                request.subject.as_ref() == Some(&pane.selected)
                    || (request.subject.is_none() && pane.is_main())
            })
            .cloned()
            .collect();
        if requests.is_empty() {
            return None;
        }
        let mut cards = div()
            .id(("subject-requests", thread.get()))
            .max_h(px(260.))
            .overflow_y_scroll()
            .px(px(theme::PANE_PAD_X))
            .py(px(6.))
            .flex()
            .flex_col()
            .gap(px(8.));
        for request in requests {
            cards = cards.child(self.request_card(index, thread, request, window, cx));
        }
        Some(native_keys(cards).into_any_element())
    }

    fn request_card(
        &self,
        index: usize,
        thread: ThreadId,
        request: PendingDecision,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let handle = request.handle.clone();
        let mut card = div()
            .id(SharedString::from(format!(
                "request-{}-{}-{}",
                thread.get(),
                handle.generation,
                handle.serial
            )))
            .p(px(8.))
            .rounded(px(theme::R_CONTROL))
            .bg(rgb(theme::ATTENTION_WASH))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(components::label(
                format!(
                    "{} · {}",
                    request.decision.tool_name, request.decision.description
                ),
                theme::TEXT_STRONG,
            ));
        if request.subject.is_none() {
            card = card.child(components::label(
                "Agent identity unavailable",
                theme::TEXT_MUTED,
            ));
        }
        if let Some((failed, error)) = &self.panes[index].request_error {
            if failed == &handle {
                card = card.child(components::label(
                    format!("Could not send answer: {error}"),
                    theme::BLOCKED,
                ));
            }
        }
        let questions = pane::question_of(&request.decision);
        if let Some(questions) = questions {
            let forms = self.panes[index].request_forms.clone();
            if !forms.0.borrow().contains_key(&handle) {
                let inputs = questions
                    .iter()
                    .map(|_| cx.new(|cx| InputState::new(window, cx).placeholder("Other answer")))
                    .collect();
                forms.0.borrow_mut().insert(
                    handle.clone(),
                    RequestForm {
                        answers: vec![Default::default(); questions.len()],
                        inputs,
                    },
                );
            }
            for (question_at, question) in questions.iter().enumerate() {
                card = card.child(components::label(
                    question.question.clone(),
                    theme::TEXT_STRONG,
                ));
                let mut options = div().flex().flex_wrap().gap(px(4.));
                for (option_at, option) in question.options.iter().enumerate() {
                    let forms = forms.clone();
                    let handle = handle.clone();
                    let multi = question.multi_select;
                    let selected = forms.0.borrow()[&handle].answers[question_at]
                        .picks
                        .contains(&option_at);
                    options = options.child(
                        components::button(SharedString::from(format!(
                            "request-choice-{}-{}-{question_at}-{option_at}",
                            handle.generation, handle.serial
                        )))
                        .tab_stop(true)
                        .selected(selected)
                        .label(option.label.clone())
                        .tooltip(option.description.clone())
                        .on_click(cx.listener(move |_, _, _, cx| {
                            if let Some(form) = forms.0.borrow_mut().get_mut(&handle) {
                                let picks = &mut form.answers[question_at].picks;
                                if picks.contains(&option_at) {
                                    picks.retain(|at| *at != option_at);
                                } else {
                                    if !multi {
                                        picks.clear();
                                    }
                                    picks.push(option_at);
                                }
                            }
                            cx.notify();
                        })),
                    );
                }
                let input_selector = format!(
                    "request-other-{}-{}-{question_at}",
                    thread.get(),
                    handle.serial
                );
                card = card.child(options).child(
                    div()
                        .debug_selector(move || input_selector.clone())
                        .child(Input::new(&forms.0.borrow()[&handle].inputs[question_at])),
                );
            }
            let deny_handle = handle.clone();
            card = card.child(
                components::button(SharedString::from(format!(
                    "request-cancel-{}-{}",
                    handle.generation, handle.serial
                )))
                .tab_stop(true)
                .label("Decline")
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.respond_exact(
                        thread,
                        &deny_handle,
                        DecisionAnswer::Deny {
                            message: "The operator declined this question.".into(),
                        },
                        cx,
                    )
                })),
            );
            let answered = request.decision.input.clone();
            let submit_selector = format!("request-submit-{}-{}", thread.get(), handle.serial);
            let handle = handle.clone();
            card = card.child(
                components::button(SharedString::from(format!(
                    "request-submit-{}-{}",
                    handle.generation, handle.serial
                )))
                .tab_stop(true)
                .label("Send answer")
                .debug_selector(move || submit_selector.clone())
                .on_click(cx.listener(move |view, _, _, cx| {
                    let mut state = forms.0.borrow_mut();
                    let Some(form) = state.get_mut(&handle) else {
                        return;
                    };
                    for (answer, input) in form.answers.iter_mut().zip(&form.inputs) {
                        answer.other = Some(input.read(cx).value().to_string())
                            .filter(|text| !text.trim().is_empty());
                    }
                    if form
                        .answers
                        .iter()
                        .any(|answer| answer.picks.is_empty() && answer.other.is_none())
                    {
                        return;
                    }
                    let input = ferrite_core::questions::answered_input(
                        &answered,
                        &form.answers,
                        &questions,
                    );
                    drop(state);
                    view.respond_exact(thread, &handle, DecisionAnswer::Allow { input }, cx);
                })),
            );
        } else {
            let accepted = request.decision.input.clone();
            let allow_handle = handle.clone();
            card = card.child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(
                        components::button(SharedString::from(format!(
                            "request-allow-{}-{}",
                            handle.generation, handle.serial
                        )))
                        .tab_stop(true)
                        .label("Allow")
                        .debug_selector(move || {
                            format!("request-allow-{}-{}", thread.get(), allow_handle.serial)
                        })
                        .on_click(cx.listener({
                            let handle = handle.clone();
                            move |view, _, _, cx| {
                                view.respond_exact(
                                    thread,
                                    &handle,
                                    DecisionAnswer::Allow {
                                        input: accepted.clone(),
                                    },
                                    cx,
                                )
                            }
                        })),
                    )
                    .child(
                        components::button(SharedString::from(format!(
                            "request-deny-{}-{}",
                            handle.generation, handle.serial
                        )))
                        .tab_stop(true)
                        .label("Deny")
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.respond_exact(
                                thread,
                                &handle,
                                DecisionAnswer::Deny {
                                    message: "The operator denied this tool.".into(),
                                },
                                cx,
                            )
                        })),
                    ),
            );
        }
        card.into_any_element()
    }

    fn reload_subject_history(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let subject = self.panes[index].selected.clone();
        self.panes[index].history_error = self
            .cockpit
            .retry_subject_history(thread, &subject)
            .err()
            .map(|error| format!("Could not load transcript: {error}"));
        cx.notify();
    }

    pub(super) fn retry_subject_history(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(thread) = self.panes[index].thread() else {
            return;
        };
        let subject = self.panes[index].selected.clone();
        self.panes[index].history_error = self
            .cockpit
            .ensure_subject_history(thread, &subject)
            .err()
            .map(|error| format!("Could not load transcript: {error}"))
            .or_else(|| {
                self.cockpit
                    .subject_history_error(thread, &subject)
                    .map(str::to_string)
            });
        cx.notify();
    }

    pub(super) fn prune_request_forms(&mut self, index: usize) {
        let Some(thread) = self.panes[index]
            .thread()
            .and_then(|id| self.cockpit.thread(id))
        else {
            return;
        };
        let pending = thread.activity().pending_decisions();
        self.panes[index]
            .request_forms
            .0
            .borrow_mut()
            .retain(|handle, _| pending.iter().any(|request| &request.handle == handle));
    }

    pub(super) fn toggle_subject_tool(
        &mut self,
        thread: ThreadId,
        subject: &Subject,
        call: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        if &self.panes[index].selected != subject {
            return;
        }
        self.toggle_tool(thread, call, window, cx);
    }

    pub(super) fn answer_subject(&mut self, answer: Answer, cx: &mut Context<Self>) {
        let index = self.focused();
        let Some(thread) = self.panes[index].thread() else {
            return;
        };
        let Some(request) = self
            .cockpit
            .thread(thread)
            .and_then(|thread| {
                thread
                    .activity()
                    .pending_decisions()
                    .iter()
                    .find(|request| request.subject.as_ref() == Some(&self.panes[index].selected))
            })
            .cloned()
        else {
            return;
        };
        self.answer_request(thread, request, answer, cx);
    }

    pub(super) fn answer_request(
        &mut self,
        thread: ThreadId,
        request: PendingDecision,
        answer: Answer,
        cx: &mut Context<Self>,
    ) {
        if pane::question_of(&request.decision).is_some() && answer != Answer::Deny {
            return;
        }
        let response = match answer {
            Answer::Allow => DecisionAnswer::Allow {
                input: request.decision.input.clone(),
            },
            Answer::Deny => DecisionAnswer::Deny {
                message: "The operator denied this tool.".into(),
            },
            Answer::Always => match request.decision.standing_answer() {
                Some(suggestion) => DecisionAnswer::AllowAlways {
                    input: request.decision.input.clone(),
                    suggestion: suggestion.clone(),
                },
                None => return,
            },
        };
        self.respond_exact(thread, &request.handle, response, cx);
    }
}
