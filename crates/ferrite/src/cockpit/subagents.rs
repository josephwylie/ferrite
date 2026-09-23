//! Subject navigation and supervision. Provider execution stays in core.
use super::*;
use crate::{components, decision, icons, theme};
use ferrite_core::activity::{
    AgentInfo, AgentStatus, DecisionHandle, PendingDecision, Subject, TranscriptCoverage,
};
use ferrite_core::transcript::Status;
use gpui::component::{
    button::ButtonVariants,
    checkbox::Checkbox,
    input::{Input, InputState},
    radio::{Radio, RadioGroup},
    scroll::ScrollableElement,
    tab::{Tab, TabBar},
    Disableable, Sizable,
};
use gpui::{Animation, AnimationExt, KeyDownEvent};
use gpui_base::ElementExt as _;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Clone, Default)]
pub(crate) struct RequestForms(Rc<RefCell<HashMap<DecisionHandle, RequestForm>>>);
struct RequestForm {
    answers: Vec<ferrite_core::questions::Answer>,
    inputs: Vec<Entity<InputState>>,
    form_inputs: HashMap<String, Entity<InputState>>,
    values: serde_json::Map<String, serde_json::Value>,
    fit: QuestionFit,
}

#[derive(Default)]
struct QuestionFit {
    available: Option<gpui::Size<gpui::Pixels>>,
    island: Option<gpui::Bounds<gpui::Pixels>>,
    viewport: Option<gpui::Bounds<gpui::Pixels>>,
    content: Option<gpui::Bounds<gpui::Pixels>>,
    first_control: Option<gpui::Bounds<gpui::Pixels>>,
    required: Option<gpui::Pixels>,
}

impl QuestionFit {
    fn needs_expansion(&self) -> bool {
        self.available
            .zip(self.required)
            .is_some_and(|(available, required)| available.height < required)
    }

    fn measure(&mut self, part: QuestionMeasure, bounds: gpui::Bounds<gpui::Pixels>) {
        match part {
            QuestionMeasure::Available => {
                if self
                    .available
                    .is_some_and(|previous| (previous.width - bounds.size.width).abs() > px(0.5))
                {
                    self.island = None;
                    self.viewport = None;
                    self.content = None;
                    self.first_control = None;
                    self.required = None;
                }
                self.available = Some(bounds.size);
            }
            QuestionMeasure::Island => self.island = Some(bounds),
            QuestionMeasure::Viewport => self.viewport = Some(bounds),
            QuestionMeasure::Content => self.content = Some(bounds),
            QuestionMeasure::FirstControl => self.first_control = Some(bounds),
        }
        if let (Some(island), Some(viewport), Some(content), Some(first)) =
            (self.island, self.viewport, self.content, self.first_control)
        {
            // Both content and its first control move by the same scroll
            // offset. Their difference is a size, not a scrolling position.
            let first_section = first.bottom() - content.top();
            self.required = Some(island.size.height - viewport.size.height + first_section);
        }
    }
}

#[derive(Clone, Copy)]
enum QuestionMeasure {
    Available,
    Island,
    Viewport,
    Content,
    FirstControl,
}

fn measure_question(
    forms: RequestForms,
    handle: DecisionHandle,
    part: QuestionMeasure,
    owner: gpui::WeakEntity<CockpitView>,
) -> impl Fn(gpui::Bounds<gpui::Pixels>, &mut Window, &mut gpui::App) + 'static {
    move |bounds, window, cx| {
        let changed = {
            let mut forms = forms.0.borrow_mut();
            let Some(form) = forms.get_mut(&handle) else {
                return;
            };
            let before = form.fit.needs_expansion();
            form.fit.measure(part, bounds);
            before != form.fit.needs_expansion()
        };
        if changed {
            let owner = owner.clone();
            window.defer(cx, move |_, cx| {
                let _ = owner.update(cx, |_, cx| cx.notify());
            });
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct TabInteraction(Rc<RefCell<TabInteractionState>>);
#[derive(Default)]
struct TabInteractionState {
    focus: HashMap<Subject, FocusHandle>,
    keyboard: Option<Subject>,
}

impl TabInteraction {
    pub(super) fn main_focus(&self) -> Option<FocusHandle> {
        self.0.borrow().focus.get(&Subject::Main).cloned()
    }
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

/// Every request card's frame in the overlay: the Pane's inline inset, the
/// card centred in the reading column, and the dock gap above the
/// Composer. For a question this frame is the island `QuestionFit`
/// measures, so the gap is part of what must fit.
fn request_frame(card: impl IntoElement) -> Div {
    native_keys(
        div()
            .w_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .items_center()
            .overflow_hidden()
            .px(px(theme::PANE_PAD_X))
            .pb(px(theme::DECISION_DOCK_GAP))
            .child(card),
    )
}

/// Whether `pane` shows `request`: its selected Subject's, and on Main
/// also a request whose owner the provider did not name.
fn shown_on(pane: &PaneView, request: &PendingDecision) -> bool {
    request.subject.as_ref() == Some(&pane.selected)
        || (request.subject.is_none() && pane.is_main())
}

/// A head's `· detail`: the tool or header, and `agent unknown` when the
/// provider named no owner.
fn request_detail(detail: &str, unowned: bool) -> Option<SharedString> {
    match (detail.is_empty(), unowned) {
        (true, false) => None,
        (false, false) => Some(detail.to_string().into()),
        (true, true) => Some("agent unknown".into()),
        (false, true) => Some(format!("{detail} · agent unknown").into()),
    }
}

/// The question digit keys pick in: the first with options and no pick
/// yet, else the first with options (a multi-select keeps toggling).
fn digit_question(
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

/// What a subagent tab carries after its label.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Marks {
    working: bool,
    waiting: bool,
    failed: bool,
}

/// Busy dots while it works, the needs-you dot while a request waits, a
/// drawn `✗` once it failed; idle, stopped and starting agents carry none
/// (the tooltip keeps the status word).
fn subject_marks(agent: &ferrite_core::activity::AgentView<'_>, waiting: bool) -> Marks {
    Marks {
        working: agent.fresh() && agent.status() == AgentStatus::Working,
        waiting,
        failed: matches!(agent.status(), AgentStatus::Failed | AgentStatus::NotFound),
    }
}

/// One tab's laid-out width in the plain Tab variant: its inline padding
/// and 1px edges, the label up to its cap, and each mark after a gap. The
/// overflow model and the tab's own `w` both come from here.
fn tab_width(label: f32, marks: Marks) -> f32 {
    let mark = |on: bool, width: f32| {
        if on {
            theme::SUBJECT_TAB_INNER_GAP + width
        } else {
            0.
        }
    };
    2. * (theme::SUBJECT_TAB_PAD_X + theme::SUBJECT_TAB_EDGE)
        + label.min(theme::SUBJECT_LABEL_MAX_W)
        + mark(marks.working, theme::BUSY_DOTS_W)
        + mark(marks.waiting, theme::ATTENTION_DOT)
        + mark(marks.failed, theme::SUBJECT_FAILED_MARK)
}

/// The one needs-you dot: a waiting tab, the overflow, the head's jump.
fn attention_dot() -> Div {
    div()
        .flex_shrink_0()
        .size(px(theme::ATTENTION_DOT))
        .rounded_full()
        .bg(rgb(theme::ATTENTION))
}

/// A working tab's busy dots: three `RUNNING` dots in a box exactly as wide
/// as they are, lifting in turn; still under reduced motion.
fn working_dots(animated: bool) -> AnyElement {
    let mut row = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::BUSY_DOT_GAP))
        .w(px(theme::BUSY_DOTS_W))
        .h(px(theme::LH_META));
    for index in 0usize..3 {
        let dot = div()
            .relative()
            .size(px(theme::BUSY_DOT_D))
            .rounded_full()
            .bg(rgb(theme::RUNNING));
        row = row.child(if animated {
            dot.with_animation(
                ("working-dot", index),
                Animation::new(Duration::from_millis(theme::BUSY_DOTS_MS)).repeat(),
                move |dot, progress| {
                    let phase = progress * std::f32::consts::TAU - index as f32 * 0.7;
                    dot.top(px(-phase.sin().max(0.) * theme::BUSY_DOT_LIFT))
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
                let waiting = activity
                    .pending_decisions()
                    .iter()
                    .any(|request| request.subject.as_ref() == Some(&agent.subject()));
                tab_width(
                    measure(&agent_name(agent.info())).ceil(),
                    subject_marks(agent, waiting),
                )
            })
            .collect();
        // The slot is laid out after the real title, branch, attention and usage
        // controls; the plain Tab variant packs its tabs with no gap.
        let main_width = tab_width(measure("Main").ceil(), Marks::default());
        let selected = children
            .iter()
            .position(|agent| agent.subject() == pane.selected);
        let overflow_width = |hidden: usize| {
            if hidden == 0 {
                0.
            } else {
                2. * (theme::SUBJECT_TAB_GAP + theme::CHIP_PAD_X) + measure(&format!("+{hidden}"))
            }
        };
        // Main and the selected Subject are navigation anchors. Reserve their
        // room first so resizing never hides the transcript being read.
        let minimum = main_width
            + selected.map_or(0., |at| widths[at])
            + overflow_width(children.len() - usize::from(selected.is_some()));
        let available = pane.subject_strip_width.max(minimum);
        let mut visible_indices: Vec<usize> = (0..children.len()).collect();
        let all_width = main_width + widths.iter().sum::<f32>();
        if all_width > available {
            visible_indices = selected.into_iter().collect();
            let mut used = main_width + selected.map_or(0., |at| widths[at]);
            for (at, width) in widths.iter().enumerate() {
                if selected == Some(at) {
                    continue;
                }
                let overflow = overflow_width(children.len() - visible_indices.len() - 1);
                if used + width + overflow > available {
                    break;
                }
                used += width;
                visible_indices.push(at);
            }
            // Priority changes visibility, not the Provider's child order.
            visible_indices.sort_unstable();
        }
        let visible: Vec<_> = visible_indices.iter().map(|at| &children[*at]).collect();
        let mut order = vec![Subject::Main];
        order.extend(visible.iter().map(|agent| agent.subject()));
        let nav = Rc::new(order);
        let interaction = pane.tab_interaction.clone();
        let identity = format!("subject-tabs-{}-{:?}", thread.get(), nav);
        // The plain Tab variant: the active tab is a `FILL` pill (the kit's
        // `tab_active`), never a sliding accent underline.
        let mut tabs = TabBar::new(SharedString::from(identity))
            .xsmall()
            .h(px(theme::CHIP_H))
            .last_empty_space(div().w_0());
        if let Some(at) = nav.iter().position(|subject| subject == &pane.selected) {
            tabs = tabs.selected_index(at);
        }
        let main = Tab::new()
            .aria_label("Main transcript")
            .tooltip(|window, cx| {
                gpui::component::tooltip::Tooltip::new("Main transcript").build(window, cx)
            })
            .w(px(main_width))
            .rounded(px(theme::R_CHIP))
            .debug_selector(move || format!("subject-main-{}", thread.get()))
            .child(
                div()
                    .debug_selector(move || format!("subject-main-label-{}", thread.get()))
                    .text_size(px(theme::FS_SM))
                    .text_color(rgb(if pane.is_main() {
                        theme::TEXT_STRONG
                    } else {
                        theme::TEXT_MUTED
                    }))
                    .child("Main"),
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
            let marks = subject_marks(agent, waiting);
            let mut content = div()
                .flex()
                .items_center()
                .gap(px(theme::SUBJECT_TAB_INNER_GAP))
                .min_w_0()
                .child(
                    div()
                        .max_w(px(theme::SUBJECT_LABEL_MAX_W))
                        .truncate()
                        .text_size(px(theme::FS_SM))
                        .child(name.clone()),
                );
            if marks.working {
                content = content.child(working_dots(!cx.reduce_motion()));
            }
            if marks.waiting {
                content = content.child(attention_dot());
            }
            if marks.failed {
                content = content.child(icons::icon(
                    icons::CLOSE,
                    theme::SUBJECT_FAILED_MARK,
                    theme::BLOCKED,
                ));
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
                .w(px(widths[visible_indices[at]]))
                .rounded(px(theme::R_CHIP))
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
            .min_w(px(minimum))
            .h(px(theme::SUBJECT_STRIP_H))
            .debug_selector(move || format!("subject-strip-{}", thread.get()))
            .child(tabs);
        if visible_indices.len() < children.len() {
            let hidden: Vec<_> = children
                .iter()
                .enumerate()
                .filter(|(at, _)| !visible_indices.contains(at))
                .map(|(_, agent)| agent)
                .collect();
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
            // A request waiting behind the overflow still shows its dot.
            let hidden_waiting = hidden.iter().any(|agent| {
                activity
                    .pending_decisions()
                    .iter()
                    .any(|request| request.subject.as_ref() == Some(&agent.subject()))
            });
            let weak = cx.entity().downgrade();
            let picking = weak.clone();
            strip = strip.child(components::ChoiceMenu {
                id: format!("subject-overflow-{}", thread.get()).into(),
                trigger: components::button(("subject-overflow", thread.get()))
                    .tab_stop(true)
                    .h(px(theme::CHIP_H))
                    .px(px(theme::CHIP_PAD_X))
                    .mx(px(theme::SUBJECT_TAB_GAP))
                    .rounded(px(theme::R_CHIP))
                    .accessibility_label("More subagents")
                    .tooltip("More subagents")
                    .debug_selector(move || format!("subject-overflow-{}", thread.get()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(theme::SUBJECT_TAB_INNER_GAP))
                            .text_size(px(theme::FS_SM))
                            .line_height(px(theme::LH_META))
                            .text_color(rgb(theme::TEXT_MUTED))
                            .child(format!("+{}", hidden.len()))
                            .when(hidden_waiting, |label| label.child(attention_dot())),
                    ),
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
            .focus_visible(components::control_focus)
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
        // `↳`: this transcript is a child of the Thread's Main.
        div()
            .flex()
            .items_center()
            .gap(px(theme::SPACE_1_5))
            .min_w_0()
            .debug_selector(move || format!("subject-title-{}", thread.get()))
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(rgb(theme::TEXT_MUTED))
                    .child("↳"),
            )
            .child(div().min_w_0().truncate().child(label))
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
        let text = match CockpitView::key_label("cockpit::NextDecision") {
            Some(key) => format!("Jump to next request ({key})"),
            None => "Jump to next request".into(),
        };
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
                    .size(px(theme::ATTENTION_JUMP))
                    .p_0()
                    .rounded(px(theme::R_CHIP))
                    .accessibility_label("Jump to next request")
                    .tooltip(text)
                    .child(attention_dot())
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
            "loading saved transcript…"
        } else {
            match subject.coverage() {
                TranscriptCoverage::Unavailable => "transcript unavailable",
                TranscriptCoverage::ToolActivity => "tool activity only",
                TranscriptCoverage::Live => "live transcript · earlier messages may be unavailable",
                TranscriptCoverage::Partial => "partial transcript",
                TranscriptCoverage::Complete => "subagent transcript",
            }
        };
        let error = pane.history_error.clone();
        // A read-only Composer: the Composer's own ground and edge in its
        // slot, a dimmed `❯` (nothing to type here), what this transcript
        // covers, and the way back to Main.
        Some(
            native_keys(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(theme::SPACE_2))
                    .px(px(theme::PANE_PAD_X))
                    .py(px(theme::SPACE_1_5))
                    .bg(rgb(theme::RAISED))
                    .border_t_1()
                    .border_color(rgba(theme::COMPOSER_EDGE))
                    .rounded_bl(px(theme::R_PANE - 1.))
                    .rounded_br(px(theme::R_PANE - 1.))
                    .font_family(theme::FONT_MONO)
                    .text_size(px(theme::FS_SM))
                    .line_height(px(theme::LH_META))
                    .child(components::prompt_mark(theme::TEXT_MUTED))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(if error.is_some() {
                                theme::BLOCKED
                            } else {
                                theme::TEXT_MUTED
                            }))
                            .child(SharedString::from(
                                error.as_deref().unwrap_or(coverage).to_string(),
                            )),
                    )
                    .when(error.is_some(), |footer| {
                        footer.child(
                            components::ghost_button(
                                ("retry-child-history", thread.get()),
                                "Retry",
                                cx,
                            )
                            .tab_stop(true)
                            .on_click(cx.listener(
                                move |view, _, _, cx| view.reload_subject_history(thread, cx),
                            )),
                        )
                    })
                    .child(
                        components::ghost_button(("return-main", thread.get()), "Back to Main", cx)
                            .tab_stop(true)
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

    /// Small Panes keep a clear path to the retained form instead of compressing
    /// Question chrome into a viewport too small for an option. The fixed Pane
    /// header owns this button, so even a tall draft cannot cover it.
    pub(super) fn activity_question_expander(
        &self,
        index: usize,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pane = &self.panes[index];
        if self.cockpit.roster().fullscreen() == Some(pane.identity) {
            return None;
        }
        let thread = pane.thread()?;
        let pending = self.cockpit.thread(thread)?.activity().pending_decisions();
        if !pending.iter().any(|request| {
            (request.subject.as_ref() == Some(&pane.selected)
                || (request.subject.is_none() && pane.is_main()))
                && pane::questions_of(&request.decision).is_some()
                && (compact
                    || pane
                        .request_forms
                        .0
                        .borrow()
                        .get(&request.handle)
                        .is_some_and(|form| form.fit.needs_expansion()))
        }) {
            return None;
        }
        Some(
            native_keys(
                components::button(("expand-question", thread.get()))
                    .tab_stop(true)
                    .h(px(theme::CHIP_H))
                    .px(px(theme::CHIP_PAD_X))
                    .rounded(px(theme::R_CHIP))
                    .bg(rgba(theme::ATTENTION_WASH))
                    .accessibility_label("Expand this Thread to answer its question")
                    .tooltip(match CockpitView::key_label("cockpit::ToggleFullscreen") {
                        Some(key) => format!("Expand this Thread to answer its question ({key})"),
                        None => "Expand this Thread to answer its question".into(),
                    })
                    .debug_selector(|| "question-expand".into())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(theme::SPACE_1_5))
                            .text_size(px(theme::FS_SM))
                            .line_height(px(theme::LH_META))
                            .text_color(rgb(theme::ATTENTION))
                            .child(decision::mark())
                            .child("expand to answer"),
                    )
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if let Some(index) = view.pane_for(thread) {
                            view.focus_pane(index);
                            if view.cockpit.roster().fullscreen()
                                != Some(view.panes[index].identity)
                            {
                                view.cockpit.toggle_fullscreen();
                            }
                            cx.notify();
                        }
                    })),
            )
            .into_any_element(),
        )
    }

    pub(super) fn activity_question_measurement(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pane = &self.panes[index];
        let thread = pane.thread()?;
        let handles: Vec<_> = self
            .cockpit
            .thread(thread)?
            .activity()
            .pending_decisions()
            .iter()
            .filter(|request| {
                (request.subject.as_ref() == Some(&pane.selected)
                    || (request.subject.is_none() && pane.is_main()))
                    && pane::questions_of(&request.decision).is_some()
            })
            .map(|request| request.handle.clone())
            .collect();
        if handles.is_empty() {
            return None;
        }
        let mut measure = div().absolute().inset_0();
        for handle in handles {
            measure = measure.on_prepaint(measure_question(
                pane.request_forms.clone(),
                handle,
                QuestionMeasure::Available,
                cx.entity().downgrade(),
            ));
        }
        Some(measure.into_any_element())
    }

    pub(super) fn activity_decisions(
        &self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        // An L2 cell draws Main's single approval as its own compact body
        // with y/n keycaps; every other request, at L1 all of them, is the
        // one card in the requests overlay.
        if self.level_of(index, window) == Level::Instruments && self.l2_decision_card(index) {
            return None;
        }
        let pane = &self.panes[index];
        let thread = pane.thread()?;
        let activity = self.cockpit.thread(thread)?.activity();
        let fullscreen = self.cockpit.roster().fullscreen() == Some(pane.identity);
        let requests: Vec<_> = activity
            .pending_decisions()
            .iter()
            .filter(|request| shown_on(pane, request))
            .filter(|request| {
                fullscreen
                    || !pane
                        .request_forms
                        .0
                        .borrow()
                        .get(&request.handle)
                        .is_some_and(|form| form.fit.needs_expansion())
            })
            .cloned()
            .collect();
        if requests.is_empty() {
            return None;
        }
        // Below the short-Pane height a question body keeps two option rows
        // and scrolls, so its head and answer row stay in reach.
        let short = self
            .pane_rects(window)
            .into_iter()
            .find(|(at, _)| *at == index)
            .is_some_and(|(_, rect)| rect.h < theme::DECISION_SHORT_PANE_H);
        let multiple_requests = requests.len() > 1;
        let mut cards = div()
            .id(("subject-requests", thread.get()))
            .w_full()
            .min_w_0()
            .flex()
            .min_h_0()
            .max_h_full()
            .flex_col();
        for request in requests {
            cards = cards.child(self.request_card(index, thread, request, short, window, cx));
        }
        if multiple_requests {
            Some(native_keys(cards.max_h_full().overflow_y_scrollbar()).into_any_element())
        } else {
            // A single request owns its own bounded content viewport. Giving
            // its surrounding stack another scroll container makes that
            // stack consume the transcript's flex space instead of docking.
            Some(native_keys(cards).into_any_element())
        }
    }

    /// Whether this Pane's L2 cell shows Main's pending approval as its own
    /// compact card (`l2_decision_body`) — the only request the cell draws
    /// without a Composer, so the keyboard must go to `decision_focus`.
    pub(super) fn l2_decision_card(&self, index: usize) -> bool {
        let pane = &self.panes[index];
        if !pane.is_main() {
            return false;
        }
        let Some(open) = pane.thread().and_then(|thread| self.cockpit.thread(thread)) else {
            return false;
        };
        let pending = open.activity().pending_decisions();
        let mut shown = pending.iter().filter(|request| shown_on(pane, request));
        match (shown.next(), shown.next()) {
            (Some(request), None) => {
                request.subject == Some(Subject::Main)
                    && matches!(request.decision.kind, ferrite_core::DecisionKind::Approval)
                    && open.pending() == Some(&request.decision)
            }
            _ => false,
        }
    }

    pub(super) fn request_card(
        &self,
        index: usize,
        thread: ThreadId,
        request: PendingDecision,
        short: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Keep this dispatcher shallow. Windows gives the GUI main thread a
        // 1 MiB stack; compiling every Decision builder into this one frame
        // overflowed it as soon as a question arrived.
        if let Some(questions) = pane::questions_of(&request.decision) {
            let handle = request.handle.clone();
            let questions = questions.to_vec();
            return self.question_request_card(
                index, thread, request, handle, questions, short, window, cx,
            );
        }
        match &request.decision.kind {
            ferrite_core::DecisionKind::Form { .. } => {
                self.form_request_card(index, thread, request, window, cx)
            }
            ferrite_core::DecisionKind::External { .. }
            | ferrite_core::DecisionKind::Unsupported { .. } => {
                self.link_request_card(index, thread, request, cx)
            }
            _ => self.approval_request_card(index, thread, request, cx),
        }
    }

    /// The head, the prose and the send error every non-question card
    /// shares; the tool's name rides the head, the description reads as
    /// prose (`request-title-*`).
    fn request_preamble(
        &self,
        index: usize,
        thread: ThreadId,
        request: &PendingDecision,
    ) -> (Div, Option<Div>, Option<SharedString>) {
        let decision = &request.decision;
        let status = if request.submitting {
            pane::live_text(decision::status("sending…"), "request-live".into())
        } else {
            decision::status("waiting").into_any_element()
        };
        let head = decision::head(
            decision::kind_word(decision),
            request_detail(&decision.tool_name, request.subject.is_none()),
            Some(status),
        );
        let title = if decision.description.is_empty() && decision.tool_name.is_empty() {
            Some(SharedString::from(
                "The provider sent a request Ferrite could not read.",
            ))
        } else {
            (!decision.description.is_empty())
                .then(|| SharedString::from(decision.description.clone()))
        };
        let serial = request.handle.serial;
        let title = title.map(|title| {
            decision::prose(title)
                .debug_selector(move || format!("request-title-{}-{serial}", thread.get()))
        });
        let error = request
            .reply_error
            .clone()
            .map(SharedString::from)
            .or_else(|| {
                self.panes[index]
                    .request_error
                    .as_ref()
                    .filter(|(failed, _)| failed == &request.handle)
                    .map(|(_, error)| SharedString::from(error.clone()))
            });
        (head, title, error)
    }

    fn approval_request_card(
        &self,
        index: usize,
        thread: ThreadId,
        request: PendingDecision,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let handle = request.handle.clone();
        let (head, title, error) = self.request_preamble(index, thread, &request);
        let mut children = vec![head.into_any_element()];
        children.extend(title.map(IntoElement::into_any_element));
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
            children.push(
                decision::well(input)
                    .debug_selector(|| "approval-well".into())
                    .into_any_element(),
            );
        }
        children.extend(
            error.map(|error| decision::error_line("could not send", error).into_any_element()),
        );
        let mut rows = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(theme::DECISION_ROW_GAP));
        for (at, row) in decision::approval_rows(&request.decision)
            .into_iter()
            .enumerate()
        {
            let serial = handle.serial;
            let selector = match row.verb {
                decision::Verb::Allow => format!("request-allow-{}-{serial}", thread.get()),
                decision::Verb::Deny => format!("request-deny-{}-{serial}", thread.get()),
                decision::Verb::Always(choice) | decision::Verb::Choose(choice) => {
                    format!("approval-choice-{choice}")
                }
            };
            let verb = row.verb;
            let request = request.clone();
            let mut button = decision::option_row(
                SharedString::from(format!(
                    "request-row-{}-{}-{at}",
                    handle.generation, handle.serial
                )),
                decision::Row {
                    key: row.key,
                    label: row.label,
                    description: None,
                    recommended: false,
                    selected: false,
                    enabled: row.enabled && !request.submitting,
                    prose: false,
                },
            )
            .debug_selector(move || selector.clone())
            .on_click(
                cx.listener(move |view, _, _, cx| view.pick_approval(thread, &request, verb, cx)),
            );
            if verb == decision::Verb::Deny {
                // The Main card's deny has always answered to this name.
                button = button.child(
                    div()
                        .absolute()
                        .inset_0()
                        .debug_selector(|| "decision-deny".into()),
                );
            }
            rows = rows.child(button);
        }
        children.push(rows.into_any_element());
        request_frame(decision::card(handle.serial, children)).into_any_element()
    }

    /// An approval row's verb, from its click or its digit. The standing
    /// "always" row keeps the native `Choose` a click has always sent; only
    /// the `a` key sends `AllowAlways` (`answer_request`).
    fn pick_approval(
        &mut self,
        thread: ThreadId,
        request: &PendingDecision,
        verb: decision::Verb,
        cx: &mut Context<Self>,
    ) {
        match verb {
            decision::Verb::Allow => {
                self.answer_request(thread, request.clone(), Answer::Allow, cx)
            }
            decision::Verb::Deny => self.answer_request(thread, request.clone(), Answer::Deny, cx),
            decision::Verb::Always(choice) | decision::Verb::Choose(choice) => {
                if let Some(choice) = request.decision.suggestions.get(choice) {
                    self.respond_exact(
                        thread,
                        &request.handle,
                        DecisionAnswer::Choose {
                            value: choice.value.clone(),
                        },
                        cx,
                    )
                }
            }
        }
    }

    fn form_request_card(
        &self,
        index: usize,
        thread: ThreadId,
        request: PendingDecision,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let handle = request.handle.clone();
        let ferrite_core::DecisionKind::Form { fields } = &request.decision.kind else {
            unreachable!("dispatched on the Form kind")
        };
        let fields = fields.clone();
        let (head, title, error) = self.request_preamble(index, thread, &request);
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
                    fit: Default::default(),
                },
            );
        }
        let submit_handle = handle.clone();
        let selector = format!("request-submit-{}-{}", thread.get(), handle.serial);
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
                        .text_color(rgb(theme::TEXT))
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
                                        value == &serde_json::Value::String(option.value.clone())
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
                                        if let Some(form) = forms.0.borrow_mut().get_mut(&handle) {
                                            let values =
                                                form.values.entry(id.clone()).or_insert_with(
                                                    || serde_json::Value::Array(Vec::new()),
                                                );
                                            let values = values
                                                .as_array_mut()
                                                .expect("form enum is an array");
                                            values.retain(|item| {
                                                item != &serde_json::Value::String(value.clone())
                                            });
                                            if *checked {
                                                values
                                                    .push(serde_json::Value::String(value.clone()));
                                            }
                                        }
                                        cx.notify();
                                    })),
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
        let cancel_handle = handle.clone();
        let mut children = vec![head.into_any_element()];
        children.extend(title.map(IntoElement::into_any_element));
        children.push(
            div()
                .min_h_0()
                .max_h(px(theme::DECISION_BODY_MAX_H))
                .flex()
                .flex_col()
                .overflow_y_scrollbar()
                .child(body)
                .into_any_element(),
        );
        children.extend(error.map(|error| {
            decision::error_line("not sent", error)
                .debug_selector(|| "form-validation-error".into())
                .into_any_element()
        }));
        let sending = request.submitting;
        children.push(
            decision::footer(
                &[],
                [
                    decision::skip_button("form-cancel", "Cancel", cx)
                        .disabled(!request.decision.policy.deny || sending)
                        .debug_selector(|| "form-cancel".into())
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.respond_exact(thread, &cancel_handle, DecisionAnswer::Cancel, cx)
                        }))
                        .into_any_element(),
                    decision::send_button(
                        "form-send",
                        if sending { "Sending…" } else { "Send" },
                        !request.decision.policy.allow || sending,
                        cx,
                    )
                    .debug_selector(move || selector.clone())
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.send_form(thread, &submit_handle, &fields, cx)
                    }))
                    .into_any_element(),
                ],
            )
            .into_any_element(),
        );
        request_frame(decision::card(handle.serial, children)).into_any_element()
    }

    fn send_form(
        &mut self,
        thread: ThreadId,
        handle: &DecisionHandle,
        fields: &[ferrite_core::FormField],
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let forms = self.panes[index].request_forms.clone();
        let mut state = forms.0.borrow_mut();
        let Some(form) = state.get_mut(handle) else {
            return;
        };
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
        let values = serde_json::Value::Object(form.values.clone());
        drop(state);
        if let Err(error) = ferrite_core::validate_form(fields, &values) {
            self.panes[index].request_error = Some((handle.clone(), error));
            cx.notify();
            return;
        }
        self.respond_exact(thread, handle, DecisionAnswer::Form { values }, cx);
    }

    /// A request finished outside Ferrite (`External`), or one it cannot
    /// answer here (`Unsupported`): the reason, and the actions it allows.
    fn link_request_card(
        &self,
        index: usize,
        thread: ThreadId,
        request: PendingDecision,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let handle = request.handle.clone();
        let (head, title, error) = self.request_preamble(index, thread, &request);
        let sending = request.submitting;
        let mut children = vec![head.into_any_element()];
        children.extend(title.map(IntoElement::into_any_element));
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
        .on_click(cx.listener(move |view, _, _, cx| {
            view.respond_exact(thread, &cancel_handle, DecisionAnswer::Cancel, cx)
        }))
        .into_any_element();
        let actions = match &request.decision.kind {
            ferrite_core::DecisionKind::External { url } => {
                children.push(
                    decision::note("Complete this request in your browser, then confirm here.")
                        .into_any_element(),
                );
                let url = url.clone();
                let complete_handle = handle.clone();
                vec![
                    decision::skip_button("external-open", "Open link", cx)
                        .disabled(!safe_external_url(&url))
                        .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&url)))
                        .into_any_element(),
                    cancel,
                    decision::send_button(
                        "external-complete",
                        "Complete",
                        !request.decision.policy.allow || sending,
                        cx,
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
                    }))
                    .into_any_element(),
                ]
            }
            ferrite_core::DecisionKind::Unsupported { reason } => {
                children.push(decision::note(reason.clone()).into_any_element());
                vec![cancel]
            }
            _ => vec![cancel],
        };
        children.extend(
            error.map(|error| decision::error_line("could not send", error).into_any_element()),
        );
        children.push(decision::footer(&[], actions).into_any_element());
        request_frame(decision::card(handle.serial, children)).into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn question_request_card(
        &self,
        index: usize,
        thread: ThreadId,
        request: PendingDecision,
        handle: DecisionHandle,
        questions: Vec<ferrite_core::questions::Question>,
        short: bool,
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
                            .placeholder("or type your own answer…")
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
                    fit: Default::default(),
                },
            );
        }
        let sending = request.submitting;
        let answers = forms.0.borrow()[&handle].answers.clone();
        // The question the digit keys pick in; only its rows show digits.
        let target = digit_question(&answers, &questions);
        let mut content = div()
            .id(("question-content", handle.serial as usize))
            .debug_selector(|| "question-scroll-content".into())
            .on_prepaint(measure_question(
                forms.clone(),
                handle.clone(),
                QuestionMeasure::Content,
                cx.entity().downgrade(),
            ))
            .w_full()
            .min_w_0()
            .flex_shrink_1()
            .max_h(px(if short {
                theme::DECISION_SHORT_BODY_MAX_H
            } else {
                theme::DECISION_BODY_MAX_H
            }))
            .overflow_y_scrollbar()
            .pr(px(theme::DECISION_SCROLL_GUTTER))
            .flex()
            .flex_col()
            .gap(px(theme::DECISION_QUESTIONS_GAP));
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
                .gap(px(theme::DECISION_QUESTION_GAP))
                .child(decision::question_text(question.question.clone()));
            if question.multi_select {
                section = section.child(decision::note("choose any"));
            }
            let mut rows = div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(theme::DECISION_ROW_GAP));
            for (oi, option) in question.options.iter().enumerate() {
                let selected = answers[qi].picks.contains(&oi);
                let (label, recommended) = decision::split_recommended(&option.label);
                let forms = forms.clone();
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
                            selected,
                            enabled: !sending,
                            prose: true,
                        },
                    )
                    .debug_selector(move || format!("question-choice-{qi}-{oi}"))
                    .when(qi == 0 && oi == 0, |row| {
                        row.on_prepaint(measure_question(
                            forms.clone(),
                            handle.clone(),
                            QuestionMeasure::FirstControl,
                            cx.entity().downgrade(),
                        ))
                    })
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if let Some(form) = forms.0.borrow_mut().get_mut(&pick_handle) {
                            pick_option(&mut form.answers[qi].picks, oi, multi);
                        }
                        view.clear_request_error(thread, &pick_handle);
                        cx.notify();
                    })),
                );
            }
            section = section.child(rows);
            if question.allow_other {
                let selector = format!("request-other-{}-{}-{qi}", thread.get(), handle.serial);
                section = section.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(theme::DECISION_ROW_INNER_GAP))
                        .px(px(theme::DECISION_ROW_PAD_X))
                        .child(
                            div()
                                .flex()
                                .flex_shrink_0()
                                .min_w(px(theme::KBD_H))
                                .children(digit(question.options.len()).map(components::kbd)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .debug_selector(move || selector.clone())
                                .when(qi == 0 && question.options.is_empty(), |input| {
                                    input.on_prepaint(measure_question(
                                        forms.clone(),
                                        handle.clone(),
                                        QuestionMeasure::FirstControl,
                                        cx.entity().downgrade(),
                                    ))
                                })
                                .cursor_text()
                                .child(
                                    Input::new(&forms.0.borrow()[&handle].inputs[qi])
                                        .small()
                                        .disabled(sending),
                                ),
                        ),
                );
            }
            content = content.child(section);
        }
        let async_question = !request.decision.blocks_execution();
        let working = async_question
            && request
                .subject
                .as_ref()
                .and_then(|subject| self.cockpit.thread(thread)?.activity().subject(subject))
                .is_some_and(|subject| subject.busy());
        let status = decision::status(if sending {
            "sending…"
        } else if working {
            "work continues"
        } else if async_question {
            "answer when ready"
        } else {
            "waiting"
        });
        let status = if sending || working {
            pane::live_text(status, "question-live".into())
        } else {
            status.into_any_element()
        };
        let detail = match questions.as_slice() {
            [question] if !question.header.is_empty() => Some(question.header.clone()),
            _ => None,
        };
        let head = decision::head(
            if questions.len() == 1 {
                "question".to_string()
            } else {
                format!("{} questions", questions.len())
            },
            request_detail(
                detail.as_deref().unwrap_or_default(),
                request.subject.is_none(),
            ),
            Some(status),
        );
        let mut children = vec![
            head.into_any_element(),
            div()
                .flex()
                .flex_col()
                .min_h_0()
                .overflow_hidden()
                .debug_selector(|| "question-viewport".into())
                .on_prepaint(measure_question(
                    forms.clone(),
                    handle.clone(),
                    QuestionMeasure::Viewport,
                    cx.entity().downgrade(),
                ))
                .child(content)
                .into_any_element(),
        ];
        if let Some(error) = request.reply_error.as_ref().or_else(|| {
            self.panes[index]
                .request_error
                .as_ref()
                .filter(|(failed, _)| failed == &handle)
                .map(|(_, error)| error)
        }) {
            children.push(decision::error_line("not sent", error.clone()).into_any_element());
        }
        let skip_handle = handle.clone();
        let submit_handle = handle.clone();
        let selector = format!("request-submit-{}-{}", thread.get(), handle.serial);
        let hints: Vec<(String, &str)> = target
            .map(|qi| {
                let question = &questions[qi];
                let keys = (question.options.len() + usize::from(question.allow_other))
                    .min(decision::DIGIT_KEYS);
                let range = if keys > 1 {
                    format!("1-{keys}")
                } else {
                    "1".into()
                };
                if question.multi_select {
                    vec![(range, "toggle"), ("↵".into(), "send")]
                } else if questions.len() == 1 {
                    vec![(range, "answer")]
                } else {
                    vec![(range, "pick"), ("↵".into(), "send")]
                }
            })
            .unwrap_or_default();
        let hints: Vec<(&str, &str)> = hints
            .iter()
            .map(|(key, verb)| (key.as_str(), *verb))
            .collect();
        children.push(
            decision::footer(
                &hints,
                [
                    decision::skip_button("question-skip", "Skip", cx)
                        .disabled(sending)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.respond_exact(
                                thread,
                                &skip_handle,
                                DecisionAnswer::Deny {
                                    message: "The operator skipped this question.".into(),
                                },
                                cx,
                            )
                        }))
                        .into_any_element(),
                    decision::send_button(
                        "question-send",
                        if sending { "Sending…" } else { "Send" },
                        sending,
                        cx,
                    )
                    .debug_selector(move || selector.clone())
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.send_question_form(thread, &submit_handle, false, cx);
                    }))
                    .into_any_element(),
                ],
            )
            .into_any_element(),
        );
        // The frame is the island QuestionFit measures: it carries the dock
        // gap as well as the card, so `required` counts every pixel the
        // overlay must hold.
        request_frame(decision::card(handle.serial, children))
            .on_prepaint(measure_question(
                forms,
                handle.clone(),
                QuestionMeasure::Island,
                cx.entity().downgrade(),
            ))
            .into_any_element()
    }

    fn clear_request_error(&mut self, thread: ThreadId, handle: &DecisionHandle) {
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
        thread: ThreadId,
        handle: &DecisionHandle,
        quiet: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.pane_for(thread) else {
            return false;
        };
        let forms = self.panes[index].request_forms.clone();
        let mut state = forms.0.borrow_mut();
        let Some(form) = state.get_mut(handle) else {
            return false;
        };
        for (answer, input) in form.answers.iter_mut().zip(&form.inputs) {
            answer.other =
                Some(input.read(cx).value().to_string()).filter(|text| !text.trim().is_empty());
        }
        if form
            .answers
            .iter()
            .any(|answer| answer.picks.is_empty() && answer.other.is_none())
        {
            drop(state);
            if !quiet {
                self.panes[index].request_error = Some((
                    handle.clone(),
                    "Answer each question before sending.".into(),
                ));
                cx.notify();
            }
            return false;
        }
        let answers = form.answers.clone();
        drop(state);
        self.respond_exact(thread, handle, DecisionAnswer::Questions { answers }, cx);
        true
    }

    /// ↵ on an empty Main Composer line: send the question this Pane shows
    /// when every one of its questions is answered. False leaves the key to
    /// the Composer (an empty line's ↵ takes a held prompt back).
    pub(super) fn send_ready_question(&mut self, cx: &mut Context<Self>) -> bool {
        let index = self.focused();
        let Some(request) = self.shown_request(index) else {
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
    fn shown_request(&self, index: usize) -> Option<PendingDecision> {
        let pane = self.panes.get(index)?;
        self.cockpit
            .thread(pane.thread()?)?
            .activity()
            .pending_decisions()
            .iter()
            .find(|request| shown_on(pane, request))
            .cloned()
    }

    /// Digit `n` (0-based) on the request this Pane shows: an approval runs
    /// row n's verb; a question picks option n of its digit question — a
    /// single single-select question with no typed words then sends at
    /// once (one key) — or focuses the words field one past its options.
    /// False when nothing here takes the digit, so it types.
    pub(super) fn pick_request_row(
        &mut self,
        index: usize,
        n: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(request) = self.shown_request(index) else {
            return false;
        };
        let thread = self.panes[index].thread().expect("a request has a Thread");
        if request.submitting {
            return true;
        }
        let Some(questions) = pane::questions_of(&request.decision) else {
            if !matches!(request.decision.kind, ferrite_core::DecisionKind::Approval) {
                return false;
            }
            let Some(verb) = decision::digit_verb(&request.decision, n) else {
                return false;
            };
            self.pick_approval(thread, &request, verb, cx);
            return true;
        };
        let forms = self.panes[index].request_forms.clone();
        let mut state = forms.0.borrow_mut();
        // The card builds its form on first paint; a question never drawn
        // (a compact cell's expander) has nothing to pick in yet.
        let Some(form) = state.get_mut(&request.handle) else {
            return false;
        };
        let Some(qi) = digit_question(&form.answers, questions) else {
            return false;
        };
        let question = &questions[qi];
        if n < question.options.len() {
            pick_option(&mut form.answers[qi].picks, n, question.multi_select);
            let one_key = questions.len() == 1
                && !question.multi_select
                && form.inputs[qi].read(cx).value().trim().is_empty();
            drop(state);
            self.clear_request_error(thread, &request.handle);
            if one_key {
                self.send_question_form(thread, &request.handle, false, cx);
            }
            cx.notify();
            true
        } else if question.allow_other && n == question.options.len() {
            let focus = form.inputs[qi].read(cx).focus_handle(cx);
            drop(state);
            window.focus(&focus, cx);
            true
        } else {
            false
        }
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
        if (pane::questions_of(&request.decision).is_some()
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
