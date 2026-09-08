//! Subject navigation and supervision. Provider execution stays in core.
use super::*;
use crate::{components, theme};
use ferrite_core::activity::{
    AgentInfo, AgentStatus, DecisionHandle, PendingDecision, Subject, TranscriptCoverage,
};
use ferrite_core::transcript::Status;
use gpui::component::{
    button::ButtonVariants,
    checkbox::Checkbox,
    group_box::{GroupBox, GroupBoxVariants},
    input::{Input, InputState},
    radio::{Radio, RadioGroup},
    scroll::ScrollableElement,
    tab::{Tab, TabBar},
    Disableable, Sizable,
};
use gpui::{Animation, AnimationExt, KeyDownEvent};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Clone, Default)]
pub(crate) struct RequestForms(Rc<RefCell<HashMap<DecisionHandle, RequestForm>>>);
struct RequestForm {
    answers: Vec<ferrite_core::questions::Answer>,
    inputs: Vec<Entity<InputState>>,
    form_inputs: HashMap<String, Entity<InputState>>,
    values: serde_json::Map<String, serde_json::Value>,
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

/// The one shell every pending request wears — approvals, questions, forms,
/// external links and the unreadable fallback alike. It floats above the
/// Composer as a self-contained island: enough separation to read as the
/// current task, while the quiet attention edge still communicates urgency.
fn request_island(
    handle: &DecisionHandle,
    body: impl IntoElement,
    cx: &mut gpui::App,
) -> AnyElement {
    let radius = gpui::component::Theme::global(cx).radius_2xl();
    let surface = div()
        .bg(rgb(theme::RAISED))
        .border_1()
        .border_color(rgba(theme::ATTENTION_EDGE))
        .rounded(radius)
        .p(px(16.))
        .w_full()
        .min_w_0()
        .style()
        .clone();
    native_keys(
        div()
            .w_full()
            .min_w_0()
            .flex()
            .justify_center()
            .px(radius)
            .pb(px(8.))
            .child(
                div()
                    .id(("question-island", handle.serial as usize))
                    .debug_selector(|| "question-island".into())
                    .relative()
                    .w_full()
                    .max_w(px(680.))
                    .min_w_0()
                    .font_family(theme::FONT_UI)
                    .child(
                        GroupBox::new()
                            .id("question-surface")
                            .fill()
                            .min_w_0()
                            .content_style(surface)
                            .child(body),
                    ),
            ),
    )
    .into_any_element()
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
        self.facts.selected(&self.cockpit, thread, &subject);
        self.panes[index].select_subject(subject, generation, cx);
        self.retry_subject_history(index, cx);
        self.cockpit
            .set_visible_subject(thread, self.panes[index].selected.clone());
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

    /// A Bell request already focused its Thread in core. Select its owning
    /// Subject without synthesizing a window input event or answering it.
    pub(super) fn select_subject_from_notice(
        &mut self,
        thread: ThreadId,
        subject: Subject,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        let subject = open.activity().canonical_subject(&subject);
        let Some(subject_view) = open.activity().subject(&subject) else {
            return;
        };
        self.facts.selected(&self.cockpit, thread, &subject);
        self.panes[index].select_subject(subject, subject_view.revision(), cx);
        self.retry_subject_history(index, cx);
        self.cockpit
            .set_visible_subject(thread, self.panes[index].selected.clone());
        self.popover = None;
        self.context_usage = None;
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
                    let pending = self.cockpit.thread(thread).is_some_and(|open| {
                        open.activity()
                            .pending_decisions()
                            .iter()
                            .any(|p| &p.handle == handle)
                    });
                    if !pending {
                        self.panes[index]
                            .request_forms
                            .0
                            .borrow_mut()
                            .remove(handle);
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
            && all.iter().all(|request| {
                matches!(request.decision.kind, ferrite_core::DecisionKind::Approval)
            })
            && all
                .iter()
                .all(|request| request.decision.suggestions.is_empty())
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
        let multiple_requests = requests.len() > 1;
        let mut cards = div()
            .id(("subject-requests", thread.get()))
            .w_full()
            .min_w_0()
            .flex()
            .flex_shrink_0()
            .flex_col()
            .gap(px(8.));
        for request in requests {
            cards = cards.child(self.request_card(index, thread, request, window, cx));
        }
        if multiple_requests {
            Some(
                native_keys(
                    cards
                        .max_h(px(
                            (f32::from(window.viewport_size().height) * 0.55).min(440.)
                        ))
                        .overflow_y_scrollbar(),
                )
                .into_any_element(),
            )
        } else {
            // A single request owns its own bounded content viewport. Giving
            // its surrounding stack another scroll container makes that
            // stack consume the transcript's flex space instead of docking.
            Some(native_keys(cards).into_any_element())
        }
    }

    pub(super) fn request_card(
        &self,
        index: usize,
        thread: ThreadId,
        request: PendingDecision,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Keep this dispatcher shallow. Windows gives the GUI main thread a
        // 1 MiB stack; compiling every Decision builder into this one frame
        // overflowed it as soon as a question arrived.
        if let Some(questions) = pane::question_of(&request.decision) {
            let handle = request.handle.clone();
            return self
                .question_request_card(index, thread, request, handle, questions, window, cx);
        }
        self.non_question_request_card(index, thread, request, window, cx)
    }

    fn non_question_request_card(
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
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .text_size(px(theme::FS_MD))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme::TEXT))
                    .child(format!(
                        "{} · {}",
                        request.decision.tool_name, request.decision.description
                    )),
            );
        if request.subject.is_none() {
            card = card.child(components::label(
                "Agent identity unavailable",
                theme::TEXT_2,
            ));
        }
        if let Some(input) = pane::approval_input(
            &request.decision,
            &self.panes[index].rich,
            format!(
                "approval-input-request-{}-{}-{}",
                thread.get(),
                handle.generation,
                handle.serial
            )
            .into(),
        ) {
            card = card.child(input);
        }
        if let Some((failed, error)) = &self.panes[index].request_error {
            if failed == &handle {
                card = card.child(components::label(
                    format!("Could not send answer: {error}"),
                    theme::BLOCKED,
                ));
            }
        }
        if let ferrite_core::DecisionKind::Form { fields } = &request.decision.kind {
            let fields = fields.clone();
            let forms = self.panes[index].request_forms.clone();
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
                    },
                );
            }
            let submit_handle = handle.clone();
            let selector = format!("request-submit-{}-{}", thread.get(), handle.serial);
            let mut body = div().w_full().flex().flex_col().gap(px(12.));
            for (field_index, field) in fields.iter().enumerate() {
                let mut section = div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .debug_selector({
                        let id = field.id.clone();
                        move || format!("form-field-{id}")
                    })
                    .child(
                        div()
                            .text_color(rgb(theme::TEXT))
                            .child(field.label.clone()),
                    );
                if !field.description.is_empty() {
                    section =
                        section.child(components::label(field.description.clone(), theme::TEXT_2));
                }
                match &field.kind {
                    ferrite_core::FormFieldKind::String { .. }
                    | ferrite_core::FormFieldKind::Number { .. }
                    | ferrite_core::FormFieldKind::Integer { .. } => {
                        if let Some(input) = forms.0.borrow()[&handle].form_inputs.get(&field.id) {
                            section = section.child(Input::new(input).disabled(request.submitting));
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
                        section = section.child(
                            Checkbox::new(("form-bool", field_index))
                                .checked(checked)
                                .disabled(request.submitting)
                                .child("Enabled")
                                .on_click(cx.listener(move |_, checked: &bool, _, cx| {
                                    if let Some(form) = forms.0.borrow_mut().get_mut(&handle) {
                                        form.values
                                            .insert(id.clone(), serde_json::Value::Bool(*checked));
                                    }
                                    cx.notify();
                                })),
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
                                            value
                                                == &serde_json::Value::String(option.value.clone())
                                        })
                                    });
                                let forms = forms.clone();
                                let handle = handle.clone();
                                let id = field.id.clone();
                                let value = option.value.clone();
                                section = section.child(
                                    Checkbox::new(("form-enum", field_index * 256 + option_index))
                                        .checked(checked)
                                        .disabled(request.submitting)
                                        .child(option.label.clone())
                                        .on_click(cx.listener(move |_, checked: &bool, _, cx| {
                                            if let Some(form) =
                                                forms.0.borrow_mut().get_mut(&handle)
                                            {
                                                let values =
                                                    form.values.entry(id.clone()).or_insert_with(
                                                        || serde_json::Value::Array(Vec::new()),
                                                    );
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
                                            cx.notify();
                                        })),
                                );
                            }
                        } else {
                            let selected_index =
                                selected.as_ref().and_then(|value| value.as_str()).and_then(
                                    |value| options.iter().position(|option| option.value == value),
                                );
                            let forms = forms.clone();
                            let handle = handle.clone();
                            let id = field.id.clone();
                            let options = options.clone();
                            section = section.child(
                                RadioGroup::vertical(("form-enum", field_index))
                                    .selected_index(selected_index)
                                    .disabled(request.submitting)
                                    .children(options.iter().enumerate().map(|(index, option)| {
                                        Radio::new(index).child(option.label.clone())
                                    }))
                                    .on_click(cx.listener(move |_, selected: &usize, _, cx| {
                                        if let (Some(form), Some(option)) = (
                                            forms.0.borrow_mut().get_mut(&handle),
                                            options.get(*selected),
                                        ) {
                                            form.values.insert(
                                                id.clone(),
                                                serde_json::Value::String(option.value.clone()),
                                            );
                                        }
                                        cx.notify();
                                    })),
                            );
                        }
                    }
                }
                body = body.child(section);
            }
            if let Some((failed, error)) = &self.panes[index].request_error {
                if failed == &handle {
                    body = body.child(
                        div()
                            .debug_selector(|| "form-validation-error".into())
                            .text_color(rgb(theme::BLOCKED))
                            .child(error.clone()),
                    );
                }
            }
            let cancel_handle = handle.clone();
            return request_island(
                &handle,
                card.child(
                    div()
                        .max_h(px(
                            (f32::from(window.viewport_size().height) * 0.45).min(360.)
                        ))
                        .flex()
                        .flex_col()
                        .overflow_y_scrollbar()
                        .child(body),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(
                            gpui::component::button::Button::new("form-cancel")
                                .small()
                                .label("Cancel")
                                .disabled(!request.decision.policy.deny || request.submitting)
                                .debug_selector(|| "form-cancel".into())
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.respond_exact(
                                        thread,
                                        &cancel_handle,
                                        DecisionAnswer::Cancel,
                                        cx,
                                    )
                                })),
                        )
                        .child(
                            gpui::component::button::Button::new("form-send")
                                .primary()
                                .small()
                                .label("Send")
                                .disabled(!request.decision.policy.allow || request.submitting)
                                .debug_selector(move || selector.clone())
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    let mut state = forms.0.borrow_mut();
                                    let Some(form) = state.get_mut(&submit_handle) else {
                                        return;
                                    };
                                    for field in &fields {
                                        let Some(input) = form.form_inputs.get(&field.id) else {
                                            continue;
                                        };
                                        let text = input.read(cx).value().to_string();
                                        if text.trim().is_empty() {
                                            if field.required {
                                                form.values.insert(
                                                    field.id.clone(),
                                                    serde_json::Value::Null,
                                                );
                                            } else {
                                                form.values.remove(&field.id);
                                            }
                                            continue;
                                        }
                                        let value = match &field.kind {
                                            ferrite_core::FormFieldKind::String { .. } => {
                                                serde_json::Value::String(text)
                                            }
                                            ferrite_core::FormFieldKind::Number { .. } => {
                                                match text
                                                    .parse::<f64>()
                                                    .ok()
                                                    .and_then(serde_json::Number::from_f64)
                                                {
                                                    Some(value) => serde_json::Value::Number(value),
                                                    None => {
                                                        form.values.insert(
                                                            field.id.clone(),
                                                            serde_json::Value::String(text),
                                                        );
                                                        continue;
                                                    }
                                                }
                                            }
                                            ferrite_core::FormFieldKind::Integer { .. } => {
                                                match text.parse::<i64>() {
                                                    Ok(value) => serde_json::Value::from(value),
                                                    Err(_) => {
                                                        form.values.insert(
                                                            field.id.clone(),
                                                            serde_json::Value::String(text),
                                                        );
                                                        continue;
                                                    }
                                                }
                                            }
                                            _ => continue,
                                        };
                                        form.values.insert(field.id.clone(), value);
                                    }
                                    let values = serde_json::Value::Object(form.values.clone());
                                    drop(state);
                                    if let Err(error) =
                                        ferrite_core::validate_form(&fields, &values)
                                    {
                                        if let Some(index) = view.pane_for(thread) {
                                            view.panes[index].request_error =
                                                Some((submit_handle.clone(), error));
                                        }
                                        cx.notify();
                                        return;
                                    }
                                    view.respond_exact(
                                        thread,
                                        &submit_handle,
                                        DecisionAnswer::Form { values },
                                        cx,
                                    );
                                })),
                        ),
                ),
                cx,
            );
        } else if let ferrite_core::DecisionKind::External { url } = &request.decision.kind {
            let url = url.clone();
            let complete_handle = handle.clone();
            let cancel_handle = handle.clone();
            return request_island(
                &handle,
                card.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(components::label(
                            "Complete this request in your browser, then confirm here.",
                            theme::TEXT_2,
                        ))
                        .child(
                            gpui::component::button::Button::new("external-open")
                                .small()
                                .label("Open link")
                                .disabled(!safe_external_url(&url))
                                .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url))),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(8.))
                                .child(
                                    gpui::component::button::Button::new("external-cancel")
                                        .small()
                                        .label("Cancel")
                                        .disabled(
                                            !request.decision.policy.deny || request.submitting,
                                        )
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            view.respond_exact(
                                                thread,
                                                &cancel_handle,
                                                DecisionAnswer::Cancel,
                                                cx,
                                            )
                                        })),
                                )
                                .child(
                                    gpui::component::button::Button::new("external-complete")
                                        .primary()
                                        .small()
                                        .label("Complete")
                                        .disabled(
                                            !request.decision.policy.allow || request.submitting,
                                        )
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            view.respond_exact(
                                                thread,
                                                &complete_handle,
                                                DecisionAnswer::Allow {
                                                    input: serde_json::Value::Null,
                                                },
                                                cx,
                                            )
                                        })),
                                ),
                        ),
                ),
                cx,
            );
        } else if let ferrite_core::DecisionKind::Unsupported { reason } = &request.decision.kind {
            let cancel_handle = handle.clone();
            return request_island(
                &handle,
                card.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(components::label(reason.clone(), theme::TEXT_2))
                        .child(
                            gpui::component::button::Button::new("unsupported-cancel")
                                .small()
                                .label("Cancel")
                                .disabled(!request.decision.policy.deny || request.submitting)
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.respond_exact(
                                        thread,
                                        &cancel_handle,
                                        DecisionAnswer::Cancel,
                                        cx,
                                    )
                                })),
                        ),
                ),
                cx,
            );
        } else {
            let accepted = request.decision.input.clone();
            let allow_handle = handle.clone();
            let choice_handle = handle.clone();
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
                        .disabled(!request.decision.policy.allow || request.submitting)
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
                        .disabled(!request.decision.policy.deny || request.submitting)
                        .on_click(cx.listener({
                            let handle = handle.clone();
                            move |view, _, _, cx| {
                                view.respond_exact(
                                    thread,
                                    &handle,
                                    DecisionAnswer::Deny {
                                        message: "The operator denied this tool.".into(),
                                    },
                                    cx,
                                )
                            }
                        })),
                    ),
            );
            for (choice_index, choice) in request.decision.suggestions.iter().enumerate() {
                let handle = choice_handle.clone();
                let value = choice.value.clone();
                let label = choice.label.clone();
                card = card.child(
                    components::button(SharedString::from(format!(
                        "approval-choice-{}-{}",
                        handle.serial, choice_index
                    )))
                    .tab_stop(true)
                    .label(label.clone())
                    .disabled(request.submitting)
                    .debug_selector(move || format!("approval-choice-{choice_index}"))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.respond_exact(
                            thread,
                            &handle,
                            DecisionAnswer::Choose {
                                value: value.clone(),
                            },
                            cx,
                        )
                    })),
                );
            }
        }
        request_island(&handle, card, cx)
    }

    fn question_request_card(
        &self,
        index: usize,
        thread: ThreadId,
        request: PendingDecision,
        handle: DecisionHandle,
        questions: Vec<ferrite_core::questions::Question>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let forms = self.panes[index].request_forms.clone();
        if !forms.0.borrow().contains_key(&handle) {
            let inputs = questions
                .iter()
                .map(|question| {
                    cx.new(|cx| {
                        InputState::new(window, cx)
                            .placeholder("Or write your own answer…")
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
                },
            );
        }
        let mut content = div()
            .id(("question-content", handle.serial as usize))
            .w_full()
            .min_w_0()
            .max_h(px(
                (f32::from(window.viewport_size().height) * 0.4).min(320.)
            ))
            .overflow_y_scrollbar()
            .pr(px(4.))
            .flex()
            .flex_col()
            .gap(px(20.));
        for (qi, question) in questions.iter().enumerate() {
            let mut section = div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .text_size(px(theme::FS_ANSWER))
                        .line_height(gpui::relative(theme::LINE_UI))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme::TEXT_STRONG))
                        .child(question.question.clone()),
                );
            if question.multi_select {
                section = section.child(components::label("Choose any that apply", theme::TEXT_2));
                for (oi, option) in question.options.iter().enumerate() {
                    let checked = forms.0.borrow()[&handle].answers[qi].picks.contains(&oi);
                    let forms = forms.clone();
                    let handle = handle.clone();
                    section = section.child(
                        div()
                            .id(("question-checkbox-hover", qi * 256 + oi))
                            .w_full()
                            .min_w_0()
                            .rounded(px(theme::R_CONTROL))
                            .border_1()
                            .border_color(if checked {
                                rgb(theme::FOCUS)
                            } else {
                                rgba(theme::TRANSPARENT)
                            })
                            .when(checked, |row| row.bg(rgb(theme::FILL)))
                            .when(!request.submitting, |row| {
                                row.hover(|style| {
                                    style
                                        .bg(rgb(theme::FILL_HOVER))
                                        .border_color(rgb(theme::SEP))
                                })
                            })
                            .child(
                                Checkbox::new(("question-checkbox", qi * 256 + oi))
                                    .debug_selector(move || format!("question-choice-{qi}-{oi}"))
                                    .checked(checked)
                                    .disabled(request.submitting)
                                    .accessibility_label(option.label.clone())
                                    .w_full()
                                    .min_w_0()
                                    .px(px(10.))
                                    .py(px(6.))
                                    .child(question_choice(option))
                                    .on_click(cx.listener(move |_, checked: &bool, _, cx| {
                                        if let Some(form) = forms.0.borrow_mut().get_mut(&handle) {
                                            let picks = &mut form.answers[qi].picks;
                                            picks.retain(|at| *at != oi);
                                            if *checked {
                                                picks.push(oi);
                                            }
                                        }
                                        cx.notify();
                                    })),
                            ),
                    );
                }
            } else if !question.options.is_empty() {
                let selected = forms.0.borrow()[&handle].answers[qi].picks.first().copied();
                let forms = forms.clone();
                let handle = handle.clone();
                section = section.child(
                    RadioGroup::vertical(("question-radios", qi))
                        .w_full()
                        .min_w_0()
                        .selected_index(selected)
                        .disabled(request.submitting)
                        .children(question.options.iter().enumerate().map(|(oi, option)| {
                            let checked = selected == Some(oi);
                            Radio::new(oi)
                                .w_full()
                                .min_w_0()
                                .group("question-option")
                                .rounded(px(theme::R_CONTROL))
                                .border_1()
                                .border_color(if checked {
                                    rgb(theme::FOCUS)
                                } else {
                                    rgba(theme::TRANSPARENT)
                                })
                                .when(checked, |row| row.bg(rgb(theme::FILL)))
                                .when(!request.submitting, |row| {
                                    row.hover(|style| {
                                        style
                                            .bg(rgb(theme::FILL_HOVER))
                                            .border_color(rgb(theme::SEP))
                                    })
                                })
                                .px(px(10.))
                                .py(px(6.))
                                .accessibility_label(option.label.clone())
                                .debug_selector(move || format!("question-choice-{qi}-{oi}"))
                                .child(question_choice(option))
                        }))
                        .on_click(cx.listener(move |_, selected: &usize, _, cx| {
                            if let Some(form) = forms.0.borrow_mut().get_mut(&handle) {
                                form.answers[qi].picks = vec![*selected];
                            }
                            cx.notify();
                        })),
                );
            }
            if question.allow_other {
                let selector = format!("request-other-{}-{}-{qi}", thread.get(), handle.serial);
                content = content.child(
                    section.child(
                        div()
                            .w_full()
                            .min_w_0()
                            .debug_selector(move || selector.clone())
                            .child(
                                Input::new(&forms.0.borrow()[&handle].inputs[qi])
                                    .disabled(request.submitting),
                            ),
                    ),
                );
            } else {
                content = content.child(section);
            }
        }
        let async_question = !request.decision.blocks_execution();
        let working = async_question
            && request
                .subject
                .as_ref()
                .and_then(|subject| self.cockpit.thread(thread)?.activity().subject(subject))
                .is_some_and(|subject| subject.busy());
        let status = if request.submitting {
            "Sending answer…"
        } else if working {
            "Work continues while you answer"
        } else if async_question {
            "Answer when ready"
        } else {
            "Waiting for your answer"
        };
        let status = div()
            .min_w_0()
            .text_size(px(theme::FS_SM))
            .text_color(rgb(theme::TEXT_2))
            .child(status);
        let status = if request.submitting || working {
            pane::live_text(status, "question-live".into())
        } else {
            status.into_any_element()
        };
        let mut body = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(theme::FS_MD))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme::ATTENTION))
                            .child(if questions.len() == 1 {
                                "Question for you".into()
                            } else {
                                format!("{} questions for you", questions.len())
                            }),
                    )
                    .child(div().flex_1())
                    .child(status),
            )
            .when(request.subject.is_none(), |body| {
                body.child(components::label(
                    "Agent identity unavailable",
                    theme::TEXT_2,
                ))
            })
            .child(content);
        if let Some(error) = request.reply_error.as_ref().or_else(|| {
            self.panes[index]
                .request_error
                .as_ref()
                .filter(|(failed, _)| failed == &handle)
                .map(|(_, error)| error)
        }) {
            body = body.child(div().text_color(rgb(theme::BLOCKED)).child(error.clone()));
        }
        let skip_handle = handle.clone();
        let submit_handle = handle.clone();
        let selector = format!("request-submit-{}-{}", thread.get(), handle.serial);
        body = body.child(
            div()
                .flex()
                .justify_end()
                .items_center()
                .gap(px(10.))
                .child(
                    question_action_button("question-skip", "Skip", false)
                        .disabled(request.submitting)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.respond_exact(
                                thread,
                                &skip_handle,
                                DecisionAnswer::Deny {
                                    message: "The operator skipped this question.".into(),
                                },
                                cx,
                            )
                        })),
                )
                .child(
                    question_action_button(
                        "question-send",
                        if request.submitting {
                            "Sending…"
                        } else {
                            "Send answer"
                        },
                        true,
                    )
                    .disabled(request.submitting)
                    .debug_selector(move || selector.clone())
                    .on_click(cx.listener(move |view, _, _, cx| {
                        let mut state = forms.0.borrow_mut();
                        let Some(form) = state.get_mut(&submit_handle) else {
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
                            if let Some(index) = view.pane_for(thread) {
                                view.panes[index].request_error = Some((
                                    submit_handle.clone(),
                                    "Answer each question before sending.".into(),
                                ));
                            }
                            cx.notify();
                            return;
                        }
                        let answers = form.answers.clone();
                        drop(state);
                        view.respond_exact(
                            thread,
                            &submit_handle,
                            DecisionAnswer::Questions { answers },
                            cx,
                        );
                    })),
                ),
        );
        request_island(&handle, body, cx)
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
        if (pane::question_of(&request.decision).is_some()
            || matches!(
                request.decision.kind,
                ferrite_core::DecisionKind::Form { .. }
                    | ferrite_core::DecisionKind::External { .. }
                    | ferrite_core::DecisionKind::Unsupported { .. }
            ))
            && answer != Answer::Deny
        {
            return;
        }
        if request.decision.policy.interaction_required && answer != Answer::Deny {
            return;
        }
        if answer == Answer::Allow && !request.decision.policy.allow {
            return;
        }
        if answer == Answer::Deny && !request.decision.policy.deny {
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

fn safe_external_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Question actions deliberately bypass the kit's light primary preset. Its
/// inherited hover foreground can erase the label on this dark island.
fn question_action_button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    primary: bool,
) -> gpui_base::Button {
    let button = gpui_base::Button::new(id)
        .tab_stop(true)
        .h(px(32.))
        .px(px(12.))
        .rounded(px(theme::R_CONTROL))
        .border_1()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_SM))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .cursor_pointer()
        .styles(|styles| styles.disabled(|style| style.opacity(0.45)))
        .child(label.into());
    if primary {
        button
            .bg(rgb(theme::TEXT))
            .border_color(rgb(theme::TEXT))
            .text_color(rgb(theme::GROUND))
            .hover(|style| {
                style
                    .bg(rgb(theme::TEXT_STRONG))
                    .border_color(rgb(theme::TEXT_STRONG))
                    .text_color(rgb(theme::GROUND))
            })
            .active(|style| {
                style
                    .bg(rgb(theme::TEXT_2))
                    .border_color(rgb(theme::TEXT_2))
                    .text_color(rgb(theme::GROUND))
            })
            .focus_visible(|style| style.border_color(rgb(theme::ATTENTION)))
    } else {
        button
            .bg(rgba(theme::TRANSPARENT))
            .border_color(rgb(theme::FILL))
            .text_color(rgb(theme::TEXT))
            .hover(|style| {
                style
                    .bg(rgb(theme::HOVER))
                    .border_color(rgb(theme::FILL_HOVER))
                    .text_color(rgb(theme::TEXT_STRONG))
            })
            .active(|style| {
                style
                    .bg(rgb(theme::FILL))
                    .border_color(rgb(theme::FILL_HOVER))
                    .text_color(rgb(theme::TEXT_STRONG))
            })
            .focus_visible(|style| style.border_color(rgb(theme::ATTENTION)))
    }
}

/// Labels and descriptions wrap inside the native choice's content slot.
fn question_choice(choice: &ferrite_core::questions::Choice) -> impl IntoElement {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(3.))
        .text_size(px(theme::FS_MD))
        .line_height(gpui::relative(theme::LINE_BODY))
        .child(
            div()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(theme::TEXT))
                .child(choice.label.clone()),
        )
        .when(!choice.description.is_empty(), |column| {
            column.child(
                div()
                    .text_color(rgb(theme::TEXT_2))
                    .child(choice.description.clone()),
            )
        })
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
