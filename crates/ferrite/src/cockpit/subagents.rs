//! Subject navigation and supervision. Provider execution stays in core.
use super::*;
use crate::{components, decision, theme};
use ferrite_core::activity::{
    AgentInfo, AgentStatus, DecisionHandle, PendingDecision, Subject, TranscriptCoverage,
};
use ferrite_core::transcript::Status;
use gpui::component::{
    checkbox::Checkbox,
    input::{Input, InputState},
    radio::{Radio, RadioGroup},
    scroll::ScrollableElement,
    Disableable,
};
use gpui::KeyDownEvent;
use gpui_base::ElementExt as _;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

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
    pub(super) note: Option<Entity<crate::composer::Composer>>,
}

impl RequestForms {
    /// Where `handle`'s cursor was moved, if it was.
    pub(super) fn cursor(&self, handle: &DecisionHandle) -> Option<usize> {
        self.1.borrow().get(handle).and_then(|cursor| cursor.index)
    }

    pub(super) fn set_cursor(&self, handle: &DecisionHandle, index: Option<usize>) {
        self.1.borrow_mut().entry(handle.clone()).or_default().index = index;
    }

    /// `handle`'s amend note, while it is open.
    pub(super) fn note(
        &self,
        handle: &DecisionHandle,
    ) -> Option<Entity<crate::composer::Composer>> {
        self.1
            .borrow()
            .get(handle)
            .and_then(|cursor| cursor.note.clone())
    }

    pub(super) fn set_note(
        &self,
        handle: &DecisionHandle,
        note: Option<Entity<crate::composer::Composer>>,
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
}

struct RequestForm {
    answers: Vec<ferrite_core::questions::Answer>,
    inputs: Vec<Entity<InputState>>,
    form_inputs: HashMap<String, Entity<InputState>>,
    values: serde_json::Map<String, serde_json::Value>,
    fit: QuestionFit,
    scroll: ScrollHandle,
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

/// Every request card's frame. In the transcript the card is the row
/// itself, on the transcript's own column; in the requests overlay it lies
/// on the plane, inset on the transcript's axes, so the rows it docks over
/// do not show through. For a question this frame is the island
/// `QuestionFit` measures.
fn request_frame(card: impl IntoElement, overlay: bool) -> Div {
    native_keys(
        div()
            .w_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .when(overlay, |frame| {
                frame
                    .bg(theme::paint::PLANE)
                    .pl(px(theme::TX_PAD_L))
                    .pr(px(theme::TX_PAD_R))
                    .py(px(theme::DECISION_PAD_Y))
            })
            .child(card),
    )
}

/// Whether `pane` shows `request`: its selected Subject's, and on Main
/// also a request whose owner the provider did not name.
pub(super) fn shown_on(pane: &PaneView, request: &PendingDecision) -> bool {
    request.subject.as_ref() == Some(&pane.selected)
        || (request.subject.is_none() && pane.is_main())
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

/// An agent's state in the shared lexicon (`theme::words`); an idle or
/// unclassified agent has none, and nothing is drawn for it.
fn status_label(status: AgentStatus, fresh: bool) -> Option<&'static str> {
    use theme::words;
    if !fresh {
        return Some(words::UNAVAILABLE);
    }
    match status {
        AgentStatus::Working => Some(words::WORKING),
        AgentStatus::Waiting => Some(words::NEEDS_YOU),
        AgentStatus::Pending => Some(words::STARTING),
        AgentStatus::Paused => Some(words::PAUSED),
        AgentStatus::Interrupted | AgentStatus::Shutdown => Some(words::INTERRUPTED),
        AgentStatus::Failed | AgentStatus::NotFound => Some(words::FAILED),
        AgentStatus::NotLoaded => Some(words::UNAVAILABLE),
        AgentStatus::Idle | AgentStatus::Unknown => None,
    }
}

/// `name · word`, or the bare name when the agent has no state word.
fn name_and_status(name: &str, status: AgentStatus, fresh: bool) -> String {
    match status_label(status, fresh) {
        Some(word) => format!("{name} \u{b7} {word}"),
        None => name.to_string(),
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

/// A subagent tab's one mark, by precedence: a request waiting
/// (`ATTENTION`) > failed or not found (`BLOCKED`) > working (`RUNNING`).
/// Idle, stopped and starting agents carry none (the tooltip keeps the
/// status word). The mark is a still dot: nothing on a tab moves.
fn subject_mark(agent: &ferrite_core::activity::AgentView<'_>, waiting: bool) -> Option<u32> {
    if waiting {
        Some(theme::ATTENTION)
    } else if matches!(agent.status(), AgentStatus::Failed | AgentStatus::NotFound) {
        Some(theme::BLOCKED)
    } else if agent.fresh() && agent.status() == AgentStatus::Working {
        Some(theme::RUNNING)
    } else {
        None
    }
}

/// The mark's slot: always in layout, `STATUS_DOT` square, holding the dot
/// when there is one — so a tab never changes width with its state.
fn mark_slot(mark: Option<u32>) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(theme::STATUS_DOT))
        .children(mark.map(components::status_dot))
}

/// One Subject tab's face on the headless tab (its role and selection are
/// the platform's): `width` wide (`tab_width`), `CHIP_H` high. Selected, a
/// `SELECTION` pill in `TEXT_STRONG` that goes to `SELECTION_HOVER` under the pointer
/// (`hover_carried`); otherwise no ground, `TEXT_MUTED` blending to `TEXT`
/// through the one hover blend. No edge and no rule: the pill says which.
fn subject_tab_face(at: usize, selected: bool, width: f32, key: SharedString) -> gpui_base::Tab {
    use crate::pointer::Pointer as _;
    gpui_base::Tab::new(at)
        .selected(selected)
        .justify_start()
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(width))
        .h(px(theme::CHIP_H))
        .px(px(theme::SUBJECT_TAB_PAD_X))
        .overflow_hidden()
        .rounded(px(theme::R_CHIP))
        .cursor_pointer()
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .when(selected, |tab| {
            // The tab's own debug name is its Subject's; the pill is named
            // by a box laid exactly over it.
            tab.relative()
                .hover_carried(key.clone())
                // The pill is a face on the strip: lit (rule 4).
                .shadow(crate::components::elevation(
                    crate::components::Elevation::Control,
                ))
                .text_color(rgb(theme::TEXT_STRONG))
                .child(
                    div()
                        .debug_selector(|| "subject-tab-selected".into())
                        .absolute()
                        .inset_0(),
                )
        })
        .when(!selected, |tab| {
            tab.text_color(crate::motion::hover_blend(
                &key,
                rgb(theme::TEXT_MUTED).into(),
                rgb(theme::TEXT).into(),
            ))
            .on_hover(crate::motion::hover_listener(key.clone()))
        })
}

/// A subagent tab's laid-out width: its inline padding, the label up to its
/// cap, and the mark slot after its gap — whatever the agent's state. The
/// overflow model and the tab's own `w` both come from here.
fn tab_width(label: f32) -> f32 {
    main_tab_width(label) + theme::SUBJECT_TAB_INNER_GAP + theme::STATUS_DOT
}

/// Main's tab: no mark, so no slot.
fn main_tab_width(label: f32) -> f32 {
    2. * theme::SUBJECT_TAB_PAD_X + label.min(theme::SUBJECT_LABEL_MAX_W)
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
                color: rgb(theme::TEXT_MUTED).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            f32::from(
                window
                    .text_system()
                    .shape_line(text.to_string().into(), px(theme::FS_UI), &[run], None)
                    .width,
            )
        };
        let widths: Vec<f32> = children
            .iter()
            .map(|agent| tab_width(measure(&agent_name(agent.info())).ceil()))
            .collect();
        // The strip is its own row: the tabs have the whole column, less
        // the indent that puts Main's label on the text column. The plain
        // Tab variant packs its tabs with no gap.
        let main_width = main_tab_width(measure("Main").ceil());
        let selected = children
            .iter()
            .position(|agent| agent.subject() == pane.selected);
        let overflow_width = |hidden: usize| {
            if hidden == 0 {
                0.
            } else {
                2. * (theme::SUBJECT_TAB_GAP + theme::CHIP_PAD_X)
                    + measure(&format!("+{hidden}"))
                    + theme::SUBJECT_TAB_INNER_GAP
                    + theme::STATUS_DOT
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
        // Our own row, not the kit's TabBar: the kit edges its active tab
        // and rules the bar in the global border colour. The active tab is
        // a `SELECTION` pill and nothing else; the id carries the ordered Subjects
        // so a reorder discards positional press state before any release.
        let selected_at = nav.iter().position(|subject| subject == &pane.selected);
        let mut tabs = gpui_base::Tabs::new(SharedString::from(identity))
            .flex()
            .items_center()
            .h(px(theme::CHIP_H));
        let main = subject_tab_face(
            0,
            selected_at == Some(0),
            main_width,
            format!("subject-tab-{}-main", thread.get()).into(),
        )
        .accessibility_label("Main transcript")
        .tooltip(|window, cx| {
            gpui::component::tooltip::Tooltip::new("Main transcript").build(window, cx)
        })
        .debug_selector(move || format!("subject-main-{}", thread.get()))
        .child(
            div()
                .debug_selector(move || format!("subject-main-label-{}", thread.get()))
                .text_size(px(theme::FS_UI))
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
            let mark = subject_mark(agent, waiting);
            // The mark leads the label, in a slot every tab keeps.
            let content = div()
                .flex()
                .items_center()
                .gap(px(theme::SUBJECT_TAB_INNER_GAP))
                .min_w_0()
                .child(mark_slot(mark))
                .child(
                    div()
                        .max_w(px(theme::SUBJECT_LABEL_MAX_W))
                        .truncate()
                        .text_size(px(theme::FS_UI))
                        .child(name.clone()),
                );
            let selector = format!(
                "subject-agent-{}-{}",
                thread.get(),
                match &subject {
                    Subject::Subagent(key) => key.as_str(),
                    _ => "main",
                }
            );
            let tooltip = name_and_status(&name, agent.status(), agent.fresh());
            let tab = subject_tab_face(
                at + 1,
                selected_at == Some(at + 1),
                widths[visible_indices[at]],
                format!("{selector}-hover").into(),
            )
            .accessibility_label(tooltip.clone())
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
            .font_weight(theme::W_BODY)
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
                    label: name_and_status(
                        &agent_name(agent.info()),
                        agent.status(),
                        agent.fresh(),
                    )
                    .into(),
                    checked: agent.subject() == pane.selected,
                    ..Default::default()
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
                // The strip runs across the Pane's top: the list drops below it.
                anchor: gpui::Anchor::TopLeft,
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
                            .text_size(px(theme::FS_UI))
                            .line_height(px(theme::LH_UI))
                            .text_color(rgb(theme::TEXT_MUTED))
                            .child(mark_slot(hidden_waiting.then_some(theme::ATTENTION)))
                            .child(components::tabular(
                                div().child(format!("+{}", hidden.len())),
                            )),
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
                place: None,
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
        tab: gpui_base::Tab,
        thread: ThreadId,
        subject: Subject,
        at: usize,
        order: Rc<Vec<Subject>>,
        interaction: TabInteraction,
        cx: &mut Context<Self>,
    ) -> gpui_base::Tab {
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
                // The row's identity includes the ordered Subjects. A reorder
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

    /// The head's title: the Thread's, whichever Subject is selected — the
    /// strip's selected tab already says which transcript this is.
    pub(super) fn activity_title(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let thread = self.panes[index].thread().expect("Thread Pane");
        self.pane_title(index, thread, cx)
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
        // What this transcript covers, as a lead and an optional tail that
        // shows only where it fits; a complete transcript says nothing.
        let coverage: Option<(&str, Option<&str>)> = if !subject.retained() {
            Some(("loading saved transcript\u{2026}", None))
        } else {
            match subject.coverage() {
                TranscriptCoverage::Unavailable => Some(("transcript unavailable", None)),
                TranscriptCoverage::ToolActivity => Some(("tool activity only", None)),
                TranscriptCoverage::Live => Some((
                    "live transcript",
                    Some(" \u{b7} earlier messages may be unavailable"),
                )),
                TranscriptCoverage::Partial => Some(("partial transcript", None)),
                TranscriptCoverage::Complete => None,
            }
        };
        let error = pane.history_error.clone();
        let seam = || {
            div()
                .flex_shrink_0()
                .px(px(theme::SPACE_1_5))
                .text_color(rgb(theme::TEXT_FAINT))
                .child("\u{b7}")
        };
        let text = div()
            .debug_selector(move || format!("child-footer-text-{}", thread.get()))
            .flex()
            .flex_1()
            .items_center()
            .min_w_0()
            .overflow_hidden()
            .text_color(rgb(theme::TEXT_MUTED));
        let text = match (&error, coverage) {
            // `failed · could not load transcript · <error>`: only the
            // state word carries the hue, and the error is machine text.
            (Some(error), _) => text
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(rgb(theme::BLOCKED))
                        .child(theme::words::FAILED),
                )
                .child(seam())
                .child(div().flex_shrink_0().child("could not load transcript"))
                .child(seam())
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_family(theme::FONT_CODE)
                        .child(SharedString::from(error.clone())),
                ),
            (None, Some((lead, tail))) => text
                .flex_wrap()
                .h(px(theme::LH_UI))
                .child(div().flex_shrink_0().child(lead))
                .children(tail.map(|tail| div().flex_shrink_0().child(tail))),
            (None, None) => text,
        };
        // A read-only line in the Composer's slot: one fixed
        // `COMPOSER_GRID_H` row on the Pane's own ground — no box, no edge,
        // no `❯` (nothing to type here) — its words on the text column and
        // the way back to Main at the right.
        let footer = div()
            .debug_selector(move || format!("child-footer-{}", thread.get()))
            .flex()
            .items_center()
            .h(px(theme::COMPOSER_GRID_H))
            .pl(px(theme::BOX_INSET_X + theme::GUTTER_W))
            .pr(px(theme::COMPOSER_PAD_END))
            .font_family(theme::FONT_UI)
            .text_size(px(theme::FS_UI))
            .line_height(px(theme::LH_UI))
            .child(text)
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(theme::SPACE_2))
                    .ml(px(theme::SPACE_2))
                    .when(error.is_some(), |actions| {
                        actions.child(
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
                        components::quiet_button(("return-main", thread.get()), "Back to Main", cx)
                            .tab_stop(true)
                            .debug_selector(move || format!("return-main-{}", thread.get()))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.select_subject(thread, Subject::Main, window, cx)
                            })),
                    ),
            );
        // The Composer's slot: the Pane's inline padding, the reading
        // column, and the block's inset from the Pane's foot.
        Some(
            native_keys(
                div()
                    .flex_shrink_0()
                    .min_w_0()
                    .px(px(theme::PANE_PAD_X))
                    .pb(px(theme::COMPOSER_INSET_B))
                    .child(components::reading_column(footer)),
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
                        let forms = &self.panes[index].request_forms;
                        forms.0.borrow_mut().remove(handle);
                        forms.1.borrow_mut().remove(handle);
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

    /// Whether this Pane's question is too big for its body: it answers in
    /// fullscreen (the expand key, `ToggleFullscreen`), and the body keeps
    /// its transcript meanwhile. The head's slot says `needs you ·
    /// question`; there is no head chip.
    pub(super) fn question_needs_expansion(&self, index: usize, compact: bool) -> bool {
        let pane = &self.panes[index];
        if self.cockpit.roster().fullscreen() == Some(pane.identity) {
            return false;
        }
        let Some(thread) = pane.thread() else {
            return false;
        };
        let Some(open) = self.cockpit.thread(thread) else {
            return false;
        };
        open.activity().pending_decisions().iter().any(|request| {
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
        })
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

    /// The Decision cards this Pane's selected Subject shows, as a snapshot
    /// of the cockpit: the requests, their owner's provider and context,
    /// and the Pane's forms. `overlay` cards lie on the plane over the
    /// transcript's foot (a question too big for the body answers in
    /// fullscreen there); the transcript's own cards scroll with its rows.
    pub(super) fn decision_cards(&self, index: usize, overlay: bool, short: bool) -> Option<Cards> {
        let pane = self.panes.get(index)?;
        let thread = pane.thread()?;
        let open = self.cockpit.thread(thread)?;
        let activity = open.activity();
        let fullscreen = self.cockpit.roster().fullscreen() == Some(pane.identity);
        let requests: Vec<(PendingDecision, bool)> = activity
            .pending_decisions()
            .iter()
            .filter(|request| shown_on(pane, request))
            .filter(|request| {
                !overlay
                    || fullscreen
                    || !pane
                        .request_forms
                        .0
                        .borrow()
                        .get(&request.handle)
                        .is_some_and(|form| form.fit.needs_expansion())
            })
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
        let (provider, context) = self.decision_context(thread);
        Some(Cards {
            owner: self.decisions.owner(),
            thread,
            provider,
            context,
            workspace: open
                .workspace()
                .map(|workspace| workspace.cwd().to_path_buf()),
            rich: pane.rich.clone(),
            forms: pane.request_forms.clone(),
            request_error: pane.request_error.clone(),
            short,
            overlay,
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
        let short = self
            .pane_rects(window)
            .into_iter()
            .find(|(at, _)| *at == index)
            .is_some_and(|(_, rect)| rect.h < theme::DECISION_SHORT_PANE_H);
        self.decision_cards(index, true, short)?.render(window, cx)
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
    pub(super) fn shown_request(&self, index: usize) -> Option<PendingDecision> {
        let pane = self.panes.get(index)?;
        self.cockpit
            .thread(pane.thread()?)?
            .activity()
            .pending_decisions()
            .iter()
            .find(|request| shown_on(pane, request))
            .cloned()
    }

    /// Digit `n` (0-based) on the request this Pane shows: an approval
    /// answers option n (`1` allow, `2` allow for this thread, `3` deny and
    /// steer); a question picks option n of its digit question — a single
    /// single-select question with no typed words then sends at once (one
    /// key) — or focuses the words field one past its options. False when
    /// nothing here takes the digit, so it types.
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
            let Some(choice) = decision::ApprovalChoice::from_index(n) else {
                return false;
            };
            if !self.approval_choice_enabled(thread, &request, choice) {
                return false;
            }
            let note = self.take_decision_note(thread, &request.handle, cx);
            self.answer_approval_request(thread, &request, choice, note, Some(window), cx);
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
            let one_key = questions.len() == 1
                && !question.multi_select
                && form.inputs[qi].read(cx).value().trim().is_empty();
            if one_key {
                drop(state);
                // One key answers a lone single-select question outright.
                return self.answer_question_option(thread, n, window, cx);
            }
            pick_option(&mut form.answers[qi].picks, n, question.multi_select);
            drop(state);
            forms.set_cursor(&request.handle, Some(n));
            self.clear_request_error(thread, &request.handle);
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
            .map(|error| error.to_string());
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
            .map(|error| error.to_string())
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
        let forms = &self.panes[index].request_forms;
        forms
            .0
            .borrow_mut()
            .retain(|handle, _| pending.iter().any(|request| &request.handle == handle));
        forms
            .1
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

    /// The letter keys (`y` `n` `a`) and the L2 keycaps: an approval takes
    /// its option (`y` 1, `a` 2, `n` 3) through the one answer path; a
    /// question, form or link takes only a deny.
    pub(super) fn answer_request(
        &mut self,
        thread: ThreadId,
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
/// on every cockpit change). Every press goes back through `owner`.
#[derive(Clone)]
pub(super) struct Cards {
    pub(super) owner: gpui::WeakEntity<CockpitView>,
    pub(super) thread: ThreadId,
    pub(super) provider: Option<Provider>,
    /// The head's last detail: Codex's sandbox, Claude's permission mode.
    pub(super) context: Option<SharedString>,
    pub(super) workspace: Option<std::path::PathBuf>,
    pub(super) rich: crate::rich::TextCache,
    pub(super) forms: RequestForms,
    pub(super) request_error: Option<(DecisionHandle, String)>,
    /// The overlay's Pane is short: a question body keeps two rows.
    pub(super) short: bool,
    /// Drawn in the requests overlay, on the plane, rather than in the
    /// transcript.
    pub(super) overlay: bool,
    /// The requests shown, each with whether its agent works on meanwhile.
    pub(super) requests: Vec<(PendingDecision, bool)>,
}

/// Ask the cockpit to draw again (a pick that lives in the Pane's forms).
fn refresh(owner: &gpui::WeakEntity<CockpitView>, cx: &mut gpui::App) {
    let _ = owner.update(cx, |_, cx| cx.notify());
}

impl Cards {
    /// Everything the drawn cards depend on, for the transcript's cache
    /// key: the requests and their sending state, the cursors and notes,
    /// the picks, the errors and the head's words.
    pub(super) fn key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.thread.hash(&mut hasher);
        self.provider.map(decision::provider_word).hash(&mut hasher);
        self.context.as_deref().hash(&mut hasher);
        self.request_error.hash(&mut hasher);
        let cursors = self.forms.1.borrow();
        let forms = self.forms.0.borrow();
        for (request, working) in &self.requests {
            request.handle.hash(&mut hasher);
            request.submitting.hash(&mut hasher);
            request.reply_error.hash(&mut hasher);
            working.hash(&mut hasher);
            if let Some(cursor) = cursors.get(&request.handle) {
                cursor.index.hash(&mut hasher);
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

    /// A press that acts on the cockpit.
    fn act<E: 'static>(
        &self,
        f: impl Fn(&mut CockpitView, &mut Window, &mut Context<CockpitView>) + 'static,
    ) -> impl Fn(&E, &mut Window, &mut gpui::App) + 'static {
        let owner = self.owner.clone();
        move |_, window, cx| {
            let _ = owner.update(cx, |view, cx| f(view, window, cx));
        }
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
                self.thread.get(),
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
            _ => self.approval_card(request),
        }
    }

    /// The head's details: the provider, an approval's context, and
    /// `agent unknown` when the provider named no owner.
    fn detail(&self, tool: Option<&str>, context: bool, unowned: bool) -> Option<SharedString> {
        decision::head_detail([
            tool.unwrap_or_default(),
            self.provider
                .map(decision::provider_word)
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
            decision::head(
                decision::approval_lead(decision),
                self.detail(None, true, unowned),
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
            (!decision.description.is_empty()
                && !decision::prose_repeats_command(&decision.description, command.as_deref()))
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
                self.request_error
                    .as_ref()
                    .filter(|(failed, _)| failed == &request.handle)
                    .map(|(_, error)| SharedString::from(error.clone()))
            });
        (head, title, error)
    }

    /// An approval (D-1…D-7): the head, the command band, the three options
    /// with the cursor on one, the amend note while it is open, and the
    /// hint.
    fn approval_card(&self, request: PendingDecision) -> AnyElement {
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
        if let Some(input) = pane::approval_input(
            &request.decision,
            &self.rich,
            format!(
                "approval-input-{}-{}-{}-{}",
                if self.overlay { "overlay" } else { "tail" },
                thread.get(),
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
        let cursor = self
            .forms
            .cursor(&handle)
            .filter(|at| *at < rows.len())
            .unwrap_or_else(|| decision::first_enabled(&rows));
        let mut options = div().flex().flex_col().flex_shrink_0().w_full().min_w_0();
        for (at, row) in rows.into_iter().enumerate() {
            let choice = row.choice;
            let selector = match choice {
                decision::ApprovalChoice::Allow => {
                    format!("request-allow-{}-{serial}", thread.get())
                }
                decision::ApprovalChoice::AllowForThread => {
                    format!("request-always-{}-{serial}", thread.get())
                }
                decision::ApprovalChoice::DenyAndSteer => {
                    format!("request-deny-{}-{serial}", thread.get())
                }
            };
            let pick = request.clone();
            let mut button = decision::option_row(
                SharedString::from(format!(
                    "request-row-{}-{}-{serial}-{at}",
                    u8::from(self.overlay),
                    handle.generation
                )),
                decision::Row {
                    key: Some(row.key),
                    label: row.label,
                    code: row.code,
                    cursor: at == cursor,
                    enabled: row.enabled && !request.submitting,
                    ..Default::default()
                },
            )
            .debug_selector(move || selector.clone())
            .on_click(self.act(move |view, window, cx| {
                let note = view.take_decision_note(thread, &pick.handle, cx);
                view.answer_approval_request(thread, &pick, choice, note, Some(window), cx);
            }));
            if choice == decision::ApprovalChoice::DenyAndSteer {
                // The deny row has always answered to this name.
                button = button.relative().child(
                    div()
                        .absolute()
                        .inset_0()
                        .debug_selector(|| "decision-deny".into()),
                );
            }
            options = options.child(button);
        }
        children.push(decision::section(options).into_any_element());
        if let Some(note) = self.forms.note(&handle) {
            children.push(self.note_field(&handle, note));
        }
        children.push(
            decision::section(decision::hint_line(&decision::APPROVAL_HINTS[..]))
                .into_any_element(),
        );
        request_frame(decision::card(serial, children), self.overlay).into_any_element()
    }

    /// The amend note (⇥, D-6): the one-line field under the options. ⏎
    /// sends the cursor's option with it; esc closes it. Inside a child's
    /// `Decision` context the answer keys are bound — in the note they are
    /// letters again.
    fn note_field(
        &self,
        handle: &DecisionHandle,
        note: Entity<crate::composer::Composer>,
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
                    let _ = owner.update(cx, |view, cx| {
                        view.send_decision_note(thread, &enter, window, cx)
                    });
                }
            })
            .on_action(self.act::<Submit>(move |view, window, cx| {
                view.send_decision_note(thread, &send, window, cx)
            }))
            .on_action(self.act::<decision::Confirm>(move |view, window, cx| {
                view.send_decision_note(thread, &confirm, window, cx)
            }))
            .on_action(self.act::<Interrupt>(move |view, window, cx| {
                view.close_decision_note(thread, &close, window, cx);
            }))
            .on_action(self.act::<decision::Dismiss>(move |view, window, cx| {
                view.close_decision_note(thread, &dismiss, window, cx);
            }));
        macro_rules! letter {
            ($action:ty, $text:expr) => {{
                let note = note.clone();
                field = field.on_action(move |_: &$action, _: &mut Window, cx: &mut gpui::App| {
                    note.update(cx, |line, cx| line.insert($text, cx));
                });
            }};
        }
        letter!(Allow, "y");
        letter!(Deny, "n");
        letter!(Always, "a");
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
                    fit: Default::default(),
                    scroll: ScrollHandle::new(),
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
        children.push(
            decision::section(
                div()
                    .min_h_0()
                    .max_h(px(theme::DECISION_BODY_MAX_H))
                    .flex()
                    .flex_col()
                    .overflow_y_scrollbar()
                    .child(body),
            )
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
            decision::section(decision::footer::<&str>(
                &[],
                [
                    decision::skip_button("form-cancel", "Cancel", cx)
                        .disabled(!request.decision.policy.deny || sending)
                        .debug_selector(|| "form-cancel".into())
                        .on_click(self.act(move |view, _, cx| {
                            view.respond_exact(thread, &cancel_handle, DecisionAnswer::Cancel, cx)
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
                    .on_click(self.act(move |view, _, cx| {
                        view.send_form(thread, &submit_handle, &fields, cx)
                    }))
                    .into_any_element(),
                ],
            ))
            .into_any_element(),
        );
        request_frame(decision::card(handle.serial, children), self.overlay).into_any_element()
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
        .on_click(self.act(move |view, _, cx| {
            view.respond_exact(thread, &cancel_handle, DecisionAnswer::Cancel, cx)
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
                    .on_click(self.act(move |view, _, cx| {
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
                children.push(decision::section(decision::note(reason.clone())).into_any_element());
                vec![cancel]
            }
            _ => vec![cancel],
        };
        children.extend(error.map(|error| {
            decision::section(decision::error_line("could not send", error)).into_any_element()
        }));
        children.push(decision::section(decision::footer::<&str>(&[], actions)).into_any_element());
        request_frame(decision::card(handle.serial, children), self.overlay).into_any_element()
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
                    fit: Default::default(),
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
        // Only the overlay measures its question against the body (a card
        // too big answers in fullscreen); in the transcript it scrolls with
        // the rows and never asks.
        let measure = |part: QuestionMeasure| {
            measure_question(forms.clone(), handle.clone(), part, self.owner.clone())
        };
        let overlay = self.overlay;
        let mut content = div()
            .id(("question-content", handle.serial as usize))
            .debug_selector(|| "question-scroll-content".into())
            .when(overlay, |content| {
                content.on_prepaint(measure(QuestionMeasure::Content))
            })
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
                    .when(overlay && qi == 0 && oi == 0, |row| {
                        row.on_prepaint(measure(QuestionMeasure::FirstControl))
                    })
                    .on_click(self.act(move |view, _, cx| {
                        if let Some(form) = pick_forms.0.borrow_mut().get_mut(&pick_handle) {
                            pick_option(&mut form.answers[qi].picks, oi, multi);
                        }
                        pick_forms.set_cursor(&pick_handle, Some(oi));
                        view.clear_request_error(thread, &pick_handle);
                        cx.notify();
                    })),
                );
            }
            section = section.child(rows);
            if question.allow_other {
                // The own answer is not a second field: one bare mono line
                // on the options' grid — its digit where theirs stand, its
                // text on their labels' column — that the digit one past
                // the options arms.
                let selector = format!("request-other-{}-{}-{qi}", thread.get(), handle.serial);
                section = section.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .child(div().flex_shrink_0().w(px(theme::GLYPH_GUTTER)))
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(px(3.0 * theme::CH))
                                .text_color(rgb(theme::TEXT_MUTED))
                                .children(
                                    digit(question.options.len())
                                        .map(|key| SharedString::from(decision::key_label(&key))),
                                ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .h(px(theme::QUESTION_OTHER_H))
                                .debug_selector(move || selector.clone())
                                .when(overlay && qi == 0 && question.options.is_empty(), |input| {
                                    input.on_prepaint(measure(QuestionMeasure::FirstControl))
                                })
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
                                    .font_family(theme::FONT_CODE)
                                    .text_size(px(theme::FS_UI))
                                    .text_color(rgb(theme::TEXT)),
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
                    .when(overlay, |viewport| {
                        viewport.on_prepaint(measure(QuestionMeasure::Viewport))
                    })
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
        let keys = target
            .map(|qi| {
                (questions[qi].options.len() + usize::from(questions[qi].allow_other))
                    .min(decision::DIGIT_KEYS)
            })
            .unwrap_or(0);
        let hints = decision::question_hints(keys);
        // The form keeps its Skip and Send for the pointer (several
        // questions, several picks, typed words); the keys say the rest.
        let skip_handle = handle.clone();
        let submit_handle = handle.clone();
        let selector = format!("request-submit-{}-{}", thread.get(), handle.serial);
        children.push(
            decision::section(decision::footer(
                hints.as_slice(),
                [
                    decision::skip_button("question-skip", "Skip", cx)
                        .disabled(sending)
                        .on_click(self.act(move |view, _, cx| {
                            view.respond_exact(
                                thread,
                                &skip_handle,
                                DecisionAnswer::Deny {
                                    message: decision::QUESTION_SKIPPED.into(),
                                },
                                cx,
                            )
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
                    .on_click(self.act(move |view, _, cx| {
                        view.send_question_form(thread, &submit_handle, false, cx);
                    }))
                    .into_any_element(),
                ],
            ))
            .into_any_element(),
        );
        // The frame is the island QuestionFit measures: it carries the dock
        // gap as well as the card, so `required` counts every pixel the
        // overlay must hold.
        request_frame(decision::card(handle.serial, children), overlay)
            .when(overlay, |frame| {
                frame.on_prepaint(measure(QuestionMeasure::Island))
            })
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

#[cfg(test)]
mod status_word_tests {
    use super::*;

    /// A tab's width is its label's and a reserved mark slot's: an agent
    /// starting, finishing or waiting never moves the tabs beside it.
    #[test]
    fn a_tab_keeps_its_width_whatever_its_state() {
        for label in [0., 24., 80., 400.] {
            assert_eq!(
                tab_width(label),
                2. * theme::SUBJECT_TAB_PAD_X
                    + f32::min(label, theme::SUBJECT_LABEL_MAX_W)
                    + theme::SUBJECT_TAB_INNER_GAP
                    + theme::STATUS_DOT
            );
            assert_eq!(
                tab_width(label) - main_tab_width(label),
                theme::SUBJECT_TAB_INNER_GAP + theme::STATUS_DOT,
                "Main carries no slot"
            );
        }
    }

    /// Nothing on the strip or a Decision loops: no repeating animation.
    #[test]
    fn subagents_schedule_no_loops() {
        let source = include_str!("subagents.rs");
        let needle = [".repeat", "()"].concat();
        assert!(!source.contains(&needle), "no repeating animation here");
    }

    #[test]
    fn every_agent_state_reads_in_the_lexicon() {
        use theme::words;
        let cases = [
            (AgentStatus::Working, Some(words::WORKING)),
            (AgentStatus::Waiting, Some(words::NEEDS_YOU)),
            (AgentStatus::Failed, Some(words::FAILED)),
            (AgentStatus::NotFound, Some(words::FAILED)),
            (AgentStatus::Interrupted, Some(words::INTERRUPTED)),
            (AgentStatus::Shutdown, Some(words::INTERRUPTED)),
            (AgentStatus::Pending, Some(words::STARTING)),
            (AgentStatus::Paused, Some(words::PAUSED)),
            (AgentStatus::NotLoaded, Some(words::UNAVAILABLE)),
            (AgentStatus::Idle, None),
            (AgentStatus::Unknown, None),
        ];
        for (status, word) in cases {
            assert_eq!(status_label(status, true), word, "{status:?}");
            assert_eq!(
                status_label(status, false),
                Some(words::UNAVAILABLE),
                "{status:?} unobserved"
            );
        }
        assert_eq!(
            name_and_status("scout", AgentStatus::Waiting, true),
            "scout \u{b7} needs you"
        );
        assert_eq!(name_and_status("scout", AgentStatus::Idle, true), "scout");
        assert!(!name_and_status("scout", AgentStatus::Failed, true).contains('\u{2014}'));
    }
}
