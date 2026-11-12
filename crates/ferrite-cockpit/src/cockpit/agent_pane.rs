//! The subagent drill-in (Claude's, `claude/subagent-drillin.png`): a press
//! on a `Ran agent` row opens that agent as a Surface of the Thread's Panel
//! (ADR 0039), its tab named by the agent's task; the body is the agent's
//! own transcript — its prompt as a bubble, `Model Haiku 5.5` under it, its
//! tool summary, its answer — drawn by the same compact transcript the side
//! chat uses (`sync_compact_transcript`). The tab's × closes it.
//!
//! The Pane keeps showing Main: this is not `select_subject`, which is how
//! a Pane takes a subagent's transcript for itself (a notice's jump, the
//! history arrows).

use std::collections::HashMap;

use super::surfaces::SurfaceId;
use super::*;

use ferrite_core::activity::Subject;

/// One agent held open in a Thread's Panel.
struct Drill {
    subject: Subject,
    /// What the pane draws it with: the Pane's pieces (its transcript, its
    /// tool disclosures), never in the grid.
    pane: PaneView,
}

/// Every Thread's open agents, by the Thread and the agent's stored key
/// (`SurfaceId::Agent`'s).
#[derive(Default)]
pub(crate) struct AgentDrills {
    drills: HashMap<(ThreadId, String), Drill>,
}

impl AgentDrills {
    #[cfg(test)]
    pub(crate) fn is_open(&self, owner: ThreadId) -> bool {
        self.drills.keys().any(|(thread, _)| *thread == owner)
    }

    /// An agent `owner`'s Panel holds, what the tests read.
    #[cfg(test)]
    pub(crate) fn subject(&self, owner: ThreadId) -> Option<&Subject> {
        self.drills
            .iter()
            .find(|((thread, _), _)| *thread == owner)
            .map(|(_, drill)| &drill.subject)
    }
}

/// The key an agent's Surface is known by; Main has none (it is the
/// Pane's own).
fn agent_key(subject: &Subject) -> Option<String> {
    match subject {
        Subject::Main => None,
        Subject::Subagent(key) => Some(key.as_str().to_owned()),
    }
}

impl CockpitView {
    /// The agent's Subject, from the call that started it.
    pub(super) fn agent_of_call(&self, thread: ThreadId, call: &str) -> Option<Subject> {
        self.cockpit.thread(thread)?.activity().invocations().find_map(
            |(native, agent)| (native == call).then(|| agent.subject()),
        )
    }

    /// A press on an agent call's row in Pane `index`: that agent opens in
    /// the Panel.
    pub(super) fn open_agent_from(&mut self, index: usize, call: String, cx: &mut Context<Self>) {
        let Some(thread) = self.panes.get(index).and_then(|pane| pane.thread()) else {
            return;
        };
        if let Some(subject) = self.agent_of_call(thread, &call) {
            self.open_agent_drill(thread, subject, cx);
        }
    }

    /// Open `subject` (an agent of `thread`) as a Surface of its Panel: one
    /// tab per agent, brought forward if it is open already.
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
        let Some(key) = agent_key(&subject) else {
            return;
        };
        if !self.agent_drills.drills.contains_key(&(thread, key.clone())) {
            let mut pane = PaneView::new(thread, cx);
            pane.select_subject(subject.clone(), generation, cx);
            if let Some(entity) = pane.transcript() {
                cx.subscribe(&entity, Self::agent_transcript_event).detach();
            }
            self.agent_drills
                .drills
                .insert((thread, key.clone()), Drill { subject, pane });
        }
        self.upsert_surface(thread, SurfaceId::Agent(key), cx);
    }

    /// Its tab closed: the agent goes from the Panel.
    pub(super) fn close_agent_drill(&mut self, thread: ThreadId, key: &str, cx: &mut Context<Self>) {
        if self
            .agent_drills
            .drills
            .remove(&(thread, key.to_owned()))
            .is_some()
        {
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
        let keys: Vec<(ThreadId, String)> = self.agent_drills.drills.keys().cloned().collect();
        for key in keys {
            let Some(drill) = self.agent_drills.drills.get(&key) else {
                continue;
            };
            let note = self.agent_model_note(key.0, &drill.subject);
            self.sync_compact_transcript(key.0, &drill.subject, &drill.pane, 0, note, cx);
        }
    }

    /// The agent's task, for its tab: its description, else its name.
    /// Whether agent `key` is working.
    pub(super) fn agent_running(&self, owner: ThreadId, key: &str) -> bool {
        let subject = Subject::Subagent(ferrite_core::activity::AgentKey::from_stored(key.to_owned()));
        self.cockpit.thread(owner).is_some_and(|open| {
            open.activity()
                .children()
                .into_iter()
                .any(|agent| agent.subject() == subject && agent.status() == ferrite_core::activity::AgentStatus::Working)
        })
    }

    pub(super) fn agent_title(&self, owner: ThreadId, key: &str) -> SharedString {
        let subject = Subject::Subagent(ferrite_core::activity::AgentKey::from_stored(key.to_owned()));
        self.cockpit
            .thread(owner)
            .and_then(|open| {
                let activity = open.activity();
                let agent = activity
                    .children()
                    .into_iter()
                    .find(|agent| agent.subject() == subject)?;
                let info = agent.info();
                Some(
                    info.description
                        .clone()
                        .filter(|task| !task.trim().is_empty())
                        .unwrap_or_else(|| subagents::agent_name(info)),
                )
            })
            .unwrap_or_else(|| "Subagent".into())
            .into()
    }

    // ------------------------------------------------------------ drawing

    /// Agent `key`'s Surface: its transcript.
    pub(super) fn agent_drill_body(&self, owner: ThreadId, key: &str) -> AnyElement {
        use ferrite_ui::theme::*;
        let Some(drill) = self.agent_drills.drills.get(&(owner, key.to_owned())) else {
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
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .children(drill.pane.transcript()),
            )
            .into_any_element()
    }
}
