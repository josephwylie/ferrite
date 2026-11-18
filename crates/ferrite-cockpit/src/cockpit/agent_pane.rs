//! The subagent inspector: an agent held open beside Main as a Surface of
//! the Thread's Panel (ADR 0039), its tab named by the agent's task; the
//! body is the agent's own transcript, drawn by the same compact transcript
//! the side chat uses (`sync_compact_transcript`). It is the secondary way
//! to read an agent: a press on a `Ran agent` row takes the whole Pane for
//! it (`select_subject`, `open_agent_from`), and the head's `Side panel`
//! button holds it here instead. Both read the Thread's one Subject: its
//! saved history, its ancestry and coverage line, its stop.

use std::collections::HashMap;

use super::surfaces::SurfaceId;
use super::*;

use ferrite_core::activity::Subject;

/// One agent held open in a Thread's Panel.
struct Drill {
    subject: Subject,
    /// Why its saved history could not load, while it could not.
    history_error: Option<String>,
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

    /// Open `subject` (an agent of `thread`) as a Surface of its Panel: one
    /// tab per agent, brought forward if it is open already.
    pub(super) fn open_agent_drill(
        &mut self,
        thread: ThreadId,
        subject: Subject,
        cx: &mut Context<Self>,
    ) {
        if let Some(key) = self.hold_agent_drill(thread, &subject, cx) {
            self.upsert_surface(thread, SurfaceId::Agent(key), cx);
        }
    }

    fn hold_agent_drill(
        &mut self,
        thread: ThreadId,
        subject: &Subject,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let open = self.cockpit.thread(thread)?;
        let subject = open.activity().canonical_subject(subject);
        let generation = open.activity().subject(&subject)?.revision();
        let key = agent_key(&subject)?;
        if !self
            .agent_drills
            .drills
            .contains_key(&(thread, key.clone()))
        {
            let mut pane = PaneView::new(thread, cx);
            pane.select_subject(subject.clone(), generation, cx);
            if let Some(entity) = pane.transcript() {
                cx.subscribe(&entity, Self::agent_transcript_event).detach();
            }
            let history_error = self.load_subject_history(thread, &subject);
            self.agent_drills.drills.insert(
                (thread, key.clone()),
                Drill {
                    subject,
                    history_error,
                    pane,
                },
            );
        }
        Some(key)
    }

    fn restore_shown_agent_drills(&mut self, cx: &mut Context<Self>) {
        let wanted: Vec<(ThreadId, String)> = self
            .panes
            .iter()
            .filter_map(|pane| pane.thread())
            .filter_map(|thread| match self.active_surface(thread)? {
                SurfaceId::Agent(key) => Some((thread, key)),
                _ => None,
            })
            .filter(|(thread, key)| {
                !self
                    .agent_drills
                    .drills
                    .contains_key(&(*thread, key.clone()))
            })
            .collect();
        for (thread, key) in wanted {
            let subject = Subject::Subagent(ferrite_core::activity::AgentKey::from_stored(key));
            self.hold_agent_drill(thread, &subject, cx);
        }
    }

    /// Remember why `subject`'s history failed to load, wherever it is open.
    pub(super) fn hold_history_error(
        &mut self,
        thread: ThreadId,
        subject: &Subject,
        error: Option<String>,
    ) {
        if let Some(index) = self.pane_for(thread).filter(|&index| self.panes[index].selected == *subject) {
            self.panes[index].history_error = error.clone();
        }
        if let Some(key) = agent_key(subject) {
            if let Some(drill) = self.agent_drills.drills.get_mut(&(thread, key)) {
                drill.history_error = error;
            }
        }
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
        self.restore_shown_agent_drills(cx);
        let keys: Vec<(ThreadId, String)> = self.agent_drills.drills.keys().cloned().collect();
        for key in keys {
            let Some(drill) = self.agent_drills.drills.get(&key) else {
                continue;
            };
            let note = self.agent_model_note(key.0, &drill.subject);
            let decisions = self.compact_decision_tail(key.0, &drill.subject);
            self.sync_compact_transcript(key.0, &drill.subject, &drill.pane, 0, note, decisions, cx);
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
    pub(super) fn agent_drill_body(
        &self,
        owner: ThreadId,
        key: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let Some(drill) = self.agent_drills.drills.get(&(owner, key.to_owned())) else {
            return self.agent_unavailable(owner, key, cx);
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
            .children(
                self.subject_history_line(owner, &drill.subject, drill.history_error.as_deref(), cx)
                    .map(|line| {
                        div()
                            .debug_selector(|| "agent-drill-history".into())
                            .flex_shrink_0()
                            .px(px(SPACE_3))
                            .py(px(SPACE_1))
                            .text_color(rgb(INK_SUBTLE))
                            .child(line)
                    }),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .children(drill.pane.transcript()),
            )
            .into_any_element()
    }

    fn agent_unavailable(&self, owner: ThreadId, key: &str, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let surface = SurfaceId::Agent(key.to_owned());
        div()
            .debug_selector(|| "agent-unavailable".into())
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_4))
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .text_color(rgb(INK_SUBTLE))
            .child("This agent is no longer available")
            .child(
                div()
                    .text_size(px(FS_META))
                    .line_height(px(LH_META))
                    .text_center()
                    .child("Its saved history is not in this thread."),
            )
            .child(
                ferrite_ui::components::form_button("agent-unavailable-close", cx)
                    .debug_selector(|| "agent-unavailable-close".into())
                    .h(px(ROW))
                    .px(px(CH))
                    .child(ferrite_ui::components::form_label("Close tab", INK_SUBTLE))
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.close_surface(owner, &surface, cx);
                    })),
            )
            .into_any_element()
    }

    /// Tell `pane`'s transcript what `subject` of `thread` holds after its
    /// first `skip` blocks, `note` standing under the first prompt. The
    /// compact, read-only twin of `sync_transcript`, for the right pane's
    /// bodies (the side chat's tabs, a subagent's drill-in): no subagents,
    /// rewinds or Reply, as they have no Pane's Composer to reply into.
    /// `decisions` is the row after the last, where the view answers the
    /// Decisions `subject` waits on.
    pub(super) fn sync_compact_transcript(
        &self,
        thread: ThreadId,
        subject: &Subject,
        pane: &PaneView,
        skip: usize,
        note: Option<SharedString>,
        decisions: Option<ferrite_pane::transcript::TranscriptTail>,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        let Some(subject_view) = open.activity().subject(subject) else {
            return;
        };
        let namespace = pane.text_namespace();
        let revision = subject_view.presentation_revision();
        let status = pane::transcript_status(subject_view.status(), subject_view.fresh());
        let reading_size = self.prefs.settings.reading_size;
        let transcript = subject_view.transcript();
        let working = pane::working_tail(pane::WorkingFacts {
            transcript,
            thread,
            starting: open.starting(),
            solo: false,
            focused: true,
            reduce_motion: cx.reduce_motion(),
        });
        let pending_call = decisions
            .as_ref()
            .and_then(|_| {
                open.activity()
                    .pending_decisions()
                    .iter()
                    .find(|request| subagents::shown_for(subject, request))
            })
            .map(|request| request.decision.tool_use_id.clone())
            .filter(|call| !call.is_empty());
        let workspace =
            ferrite_core::workspace::effective_cwd(open.session_project_root(), open.workspace())
                .map(std::path::Path::to_path_buf);
        let presentation = ferrite_pane::transcript::TranscriptPresentation {
            thread,
            selection_scope: pane.selection_scope,
            disclosure_focus: pane.tool_focus(),
            namespace,
            content_revision: revision,
            display_revision: pane.disclosure_revision(),
            focused: true,
            signal_status: Some(status),
            reading_size,
            working,
            tail: decisions,
            pending_call,
            workspace,
            solo: false,
            thinking: self.prefs.settings.show_thinking,
            agents: Default::default(),
            rewind: None,
            quotes: false,
            reply_to: open.provider().to_string().into(),
            skills: open.commands().into(),
            cited: Default::default(),
            note,
            earlier: None,
            queued: 0,
        };
        let Some(entity) = pane.transcript() else {
            return;
        };
        let selection = self.selection.clone();
        entity.update(cx, |view, cx| {
            view.reconcile(
                presentation,
                || {
                    let mut blocks =
                        pane::rendered_window(transcript.blocks()).to_vec();
                    let first = transcript.blocks().len() - blocks.len();
                    blocks.drain(..skip.saturating_sub(first).min(blocks.len()));
                    let disclosure = pane.transcript_disclosure_snapshot();
                    ferrite_pane::transcript::TranscriptBody {
                        blocks,
                        preview: pane.preview.clone(),
                        expanded: disclosure.0,
                        target: disclosure.1,
                    }
                },
                selection,
                cx,
            );
        });
    }
}
