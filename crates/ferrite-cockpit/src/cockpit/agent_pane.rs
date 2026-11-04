//! The subagent drill-in (Claude's, `claude/subagent-drillin.png`): a press
//! on a `Ran agent` row opens that agent in the right pane, over whatever
//! the pane held (the sections, a preview, a side chat), which the head's
//! `‹` goes back to. Its head reads `‹ <the agent's task>` with expand and
//! close at the right; the body is the agent's own transcript — its prompt
//! as a bubble, `Model Haiku 5.5` under it, its tool summary, its answer —
//! drawn by the same compact transcript the side chat's tabs use
//! (`sync_compact_transcript`). Open, back, expand and close are one frame.
//!
//! The Pane keeps showing Main: this is not `select_subject`, which is how
//! a Pane takes a subagent's transcript for itself (a notice's jump, the
//! history arrows).

use crate::cockpit::view_head::Column;
use std::collections::HashMap;

use super::*;

use ferrite_core::activity::Subject;
use ferrite_pane::preview::head_door;
use ferrite_ui::icons::{self, icon};

/// One agent held open in a Thread's right pane.
struct Drill {
    subject: Subject,
    /// What the pane draws it with: the Pane's pieces (its transcript, its
    /// tool disclosures), never in the grid.
    pane: PaneView,
    expanded: bool,
}

/// Every Thread's open drill-in.
#[derive(Default)]
pub(crate) struct AgentDrills {
    drills: HashMap<ThreadId, Drill>,
}

impl AgentDrills {
    pub(crate) fn is_open(&self, owner: ThreadId) -> bool {
        self.drills.contains_key(&owner)
    }

    fn is_expanded(&self, owner: ThreadId) -> bool {
        self.drills.get(&owner).is_some_and(|drill| drill.expanded)
    }

    /// The agent `owner`'s pane holds, what the tests read.
    #[cfg(test)]
    pub(crate) fn subject(&self, owner: ThreadId) -> Option<&Subject> {
        self.drills.get(&owner).map(|drill| &drill.subject)
    }
}

impl CockpitView {
    /// Whether Pane `index`'s Thread has an agent open in the right pane.
    pub(super) fn agent_drill_shown(&self, index: usize) -> bool {
        self.panes
            .get(index)
            .and_then(|pane| pane.thread())
            .is_some_and(|thread| self.agent_drills.is_open(thread))
    }

    /// Whether the docked pane covers the content area, the Thread's head
    /// and tools under it.
    pub(super) fn docked_pane_covers(&self) -> bool {
        self.side_chat_covers()
            || self.focused_thread().is_some_and(|owner| {
                self.agent_drills.is_open(owner) && self.agent_drills.is_expanded(owner)
            })
    }

    /// The agent's Subject, from the call that started it.
    pub(super) fn agent_of_call(&self, thread: ThreadId, call: &str) -> Option<Subject> {
        self.cockpit.thread(thread)?.activity().invocations().find_map(
            |(native, agent)| (native == call).then(|| agent.subject()),
        )
    }

    /// A press on an agent call's row in Pane `index`: that agent opens in
    /// the right pane.
    pub(super) fn open_agent_from(&mut self, index: usize, call: String, cx: &mut Context<Self>) {
        let Some(thread) = self.panes.get(index).and_then(|pane| pane.thread()) else {
            return;
        };
        if let Some(subject) = self.agent_of_call(thread, &call) {
            self.open_agent_drill(thread, subject, cx);
        }
    }

    /// Open `subject` (an agent of `thread`) in the right pane, the panel
    /// shown if it was hidden.
    pub(super) fn open_agent_drill(
        &mut self,
        thread: ThreadId,
        subject: Subject,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        let subject = open.activity().canonical_subject(&subject);
        let Some(generation) = open
            .activity()
            .subject(&subject)
            .map(|subject| subject.revision())
        else {
            return;
        };
        let mut pane = PaneView::new(thread, cx);
        pane.select_subject(subject.clone(), generation, cx);
        if let Some(entity) = pane.transcript() {
            cx.subscribe(&entity, Self::agent_transcript_event).detach();
        }
        let expanded = self
            .agent_drills
            .drills
            .get(&thread)
            .is_some_and(|drill| drill.expanded);
        self.agent_drills.drills.insert(
            thread,
            Drill {
                subject,
                pane,
                expanded,
            },
        );
        if !self.prefs.settings.panel_shown {
            self.change_settings(|settings| settings.panel_shown = true, cx);
        }
        cx.notify();
    }

    /// `‹`: the agent goes, and the pane shows what it held before.
    pub(super) fn close_agent_drill(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if self.agent_drills.drills.remove(&thread).is_some() {
            cx.notify();
        }
    }

    fn toggle_agent_expanded(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if let Some(drill) = self.agent_drills.drills.get_mut(&thread) {
            drill.expanded = !drill.expanded;
            cx.notify();
        }
    }

    /// The agent's transcript asks for something: a tool row folds or
    /// unfolds there. The rest (a path's reader) is the Pane's.
    fn agent_transcript_event(
        &mut self,
        entity: Entity<ferrite_pane::transcript::TranscriptView>,
        event: &ferrite_pane::transcript::TranscriptEvent,
        cx: &mut Context<Self>,
    ) {
        let ferrite_pane::transcript::TranscriptEvent::ToggleDisclosure(call) = event else {
            return;
        };
        for drill in self.agent_drills.drills.values_mut() {
            if drill.pane.transcript().as_ref() == Some(&entity) {
                drill.pane.clear_tool_target();
                drill.pane.toggle_tool(call);
            }
        }
        cx.notify();
    }

    /// `Model Haiku 5.5` for an agent's transcript, when it names its
    /// model.
    fn agent_model_note(&self, thread: ThreadId, subject: &Subject) -> Option<SharedString> {
        let open = self.cockpit.thread(thread)?;
        let model = open.activity().subject(subject)?.transcript().model()?;
        let name = ferrite_core::providers::models::display_name(model);
        (!name.is_empty()).then(|| format!("Model {name}").into())
    }

    /// Every frame: keep each open agent's transcript told what it holds.
    pub(super) fn tend_agent_drills(&mut self, cx: &mut Context<Self>) {
        let threads: Vec<ThreadId> = self.agent_drills.drills.keys().copied().collect();
        for thread in threads {
            let Some(drill) = self.agent_drills.drills.get(&thread) else {
                continue;
            };
            let note = self.agent_model_note(thread, &drill.subject);
            self.sync_compact_transcript(thread, &drill.subject, &drill.pane, 0, note, cx);
        }
    }

    // ------------------------------------------------------------ drawing

    /// The panel's column while it holds `owner`'s agent: laid as the side
    /// chat's is, `SIDE_CHAT_W` wide or, expanded, across the whole content
    /// area over the Thread.
    pub(super) fn agent_drill_column(
        &self,
        owner: ThreadId,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let width = if self.agent_drills.is_expanded(owner) {
            f32::from(window.viewport_size().width) - self.board_left()
        } else {
            self.panel_column_w()
        };
        div()
            .debug_selector(|| "panel".into())
            .absolute()
            .top_0()
            .right_0()
            .bottom(px(ferrite_ui::theme::HAIRLINE_W))
            .w(px(width))
            .overflow_hidden()
            .bg(ferrite_ui::theme::paint::PANEL)
            .child(self.agent_drill_body(owner, cx))
            .into_any_element()
    }

    fn agent_drill_body(&self, owner: ThreadId, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let Some(drill) = self.agent_drills.drills.get(&owner) else {
            return div().into_any_element();
        };
        div()
            .debug_selector(|| "agent-drill".into())
            .size_full()
            .flex()
            .flex_col()
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .text_color(rgb(INK_STRONG))
            .child(self.agent_drill_head(owner, drill, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .children(drill.pane.transcript()),
            )
            .into_any_element()
    }

    /// The head: `‹` and the agent's task, which go back together, and at
    /// the right expand and close.
    fn agent_drill_head(&self, owner: ThreadId, drill: &Drill, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let title = self
            .cockpit
            .thread(owner)
            .and_then(|open| {
                let activity = open.activity();
                let agent = activity
                    .children()
                    .into_iter()
                    .find(|agent| agent.subject() == drill.subject)?;
                let info = agent.info();
                Some(
                    info.description
                        .clone()
                        .filter(|task| !task.trim().is_empty())
                        .unwrap_or_else(|| subagents::agent_name(info)),
                )
            })
            .unwrap_or_else(|| "Subagent".into());
        let back = div()
            .id("agent-drill-back")
            .debug_selector(|| "agent-drill-back".into())
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(ICON_BUTTON))
            .pr(px(SPACE_2))
            .rounded(px(R_CONTROL))
            .cursor_pointer()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .size(px(ICON_BUTTON))
                    .child(icon(icons::BACK, ICON_BUTTON_GLYPH, INK_SUBTLE)),
            )
            .child(
                div()
                    .debug_selector(|| "agent-drill-title".into())
                    .min_w_0()
                    .truncate()
                    .text_size(px(FS_LABEL))
                    .child(SharedString::from(title)),
            )
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.close_agent_drill(owner, cx);
            }));
        let expand = head_door(
            "agent-drill-expand",
            "agent-drill-expand".into(),
            if drill.expanded {
                icons::MINIMIZE
            } else {
                icons::MAXIMIZE
            },
        )
        .tooltip(ferrite_ui::menu::tooltip(if drill.expanded {
            "Collapse"
        } else {
            "Expand"
        }))
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.toggle_agent_expanded(owner, cx);
        }));
        let close = head_door("agent-drill-close", "agent-drill-close".into(), icons::CLOSE)
            .tooltip(ferrite_ui::menu::tooltip("Close"))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.close_agent_drill(owner, cx);
                view.close_side_chat(owner, cx);
                if view.prefs.settings.panel_shown {
                    view.toggle_panel_now(cx);
                }
            }));
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(WIN_CHROME_H))
            .px(px(SIDE_CHAT_INSET))
            .mt(px(self.head_drop(Column::Rail)))
            .child(back)
            .child(expand)
            .child(close)
    }
}
