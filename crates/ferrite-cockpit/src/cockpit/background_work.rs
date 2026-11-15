//! Background work at the transcript's foot (T3's `ThreadNowLine`, ADR
//! 0038): while a Thread has work running in the background, the last row
//! of its Main transcript says so (`Waiting on 2 subagents`), with
//! `Details`, which opens the `Background work` dialog, and `Stop`. The
//! work itself lives in the rail's Background tasks section and the
//! Background Surface (`panel::background`); the line and its dialog read
//! the same calls (`Transcript::background_calls`).

use std::hash::{Hash, Hasher};

use super::panel::background::{label, output_block};
use super::*;

use ferrite_core::activity::AgentKey;
use ferrite_core::transcript::tools::{policy, ToolKind};
use ferrite_core::transcript::ToolBlock;
use ferrite_pane::pane::{BackgroundTask, WorkKind};
use ferrite_pane::transcript::TranscriptTail;

/// How long a `Stop` may go unanswered while the task still runs before the
/// line offers `Retry`.
pub(super) const STOP_SETTLES_WITHIN: std::time::Duration = std::time::Duration::from_secs(10);

const STOP_NOT_SETTLED: &str = "the provider did not stop it";

/// What a `Stop` ends: a provider task by its id, or a subagent by its key.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) enum StopTarget {
    Command(String),
    Agent(AgentKey),
}

/// Where a `Stop` on one target stands. `answered` is how many refusals of
/// the target's task were already accounted for when the stop was sent (a
/// count above it is this stop's own).
#[derive(Clone, PartialEq, Eq)]
pub(super) enum StopState {
    /// Sent, the provider not yet heard from. `nonce` names this send, so
    /// the timer of an earlier one cannot settle it.
    Stopping { nonce: u64, answered: u32 },
    /// The provider said no, or the send failed, with the reason.
    Refused(String),
    /// Unanswered past `STOP_SETTLES_WITHIN`: `Retry` is offered, and an
    /// answer that still comes is read as this stop's.
    Unsettled { answered: u32 },
}

/// One piece of work still running. A command is the provider's task
/// (`call` is the call that launched it, which holds its output); a
/// subagent is the agent a call started, still working after Main's turn.
pub(super) struct Work {
    pub(super) task: BackgroundTask,
    pub(super) stop: Option<StopTarget>,
    pub(super) call: Option<String>,
}

impl Work {
    /// What names the work in a control or a list: its call, else its task.
    pub(super) fn handle(&self) -> String {
        match (&self.call, &self.stop) {
            (Some(call), _) => call.clone(),
            (None, Some(StopTarget::Command(id))) => id.clone(),
            _ => self.task.label.clone(),
        }
    }
}

/// Every piece of background work the Thread knows of, whichever view asks:
/// the provider's tasks, the calls that launched work into the background
/// and the agents still working once Main's turn has ended.
pub(super) fn running_work(open: &ferrite_core::cockpit::ThreadView<'_>) -> Vec<Work> {
    use ferrite_core::activity::AgentStatus;
    use ferrite_core::progress::TaskStatus;
    let transcript = open.transcript();
    let mut work: Vec<Work> = transcript
        .progress()
        .background()
        .iter()
        .filter(|task| task.status == TaskStatus::Working)
        .map(|task| {
            let call = transcript.background_calls().find(|call| {
                call.background
                    .as_ref()
                    .is_some_and(|background| background.task == task.id)
            });
            let named = task.label.trim();
            Work {
                task: BackgroundTask {
                    kind: WorkKind::Command,
                    label: if named.is_empty() {
                        call.map(|call| label(call).to_string()).unwrap_or_default()
                    } else {
                        named.to_owned()
                    },
                    launched: call
                        .and_then(|call| call.background.as_ref())
                        .map(|background| background.launched),
                },
                stop: Some(StopTarget::Command(task.id.clone())),
                call: call.map(|call| call.call.clone()),
            }
        })
        .collect();
    let known: Vec<String> = work.iter().filter_map(|work| work.call.clone()).collect();
    for call in transcript.background_calls() {
        let agent = policy(&call.name).kind == ToolKind::Agent;
        if !call.in_background() || agent || known.contains(&call.call) {
            continue;
        }
        let Some(background) = &call.background else {
            continue;
        };
        work.push(Work {
            task: BackgroundTask {
                kind: WorkKind::Command,
                label: label(call).to_string(),
                launched: Some(background.launched),
            },
            stop: Some(StopTarget::Command(background.task.clone())),
            call: Some(call.call.clone()),
        });
    }
    // While Main's turn runs its agents are rows of the transcript; once it
    // has ended, the ones still working are what the Thread waits on.
    if !open.busy() {
        for (names, agent) in open.activity().named_agents() {
            let stoppable = matches!(
                agent.status(),
                AgentStatus::Working | AgentStatus::Pending | AgentStatus::Waiting
            );
            if !(stoppable || agent.status() == AgentStatus::Unknown) || !agent.fresh() {
                continue;
            }
            // The name the transcript holds a call by, for the label and
            // the drill-in.
            let call = names
                .iter()
                .find(|name| transcript.tool(name).is_some())
                .or(names.first())
                .cloned();
            work.push(Work {
                task: BackgroundTask {
                    kind: WorkKind::Subagent,
                    label: call
                        .as_deref()
                        .and_then(|call| transcript.tool(call))
                        .map(|tool| label(tool).to_string())
                        .unwrap_or_default(),
                    launched: agent.transcript().turn_started(),
                },
                stop: stoppable.then(|| StopTarget::Agent(agent.key().clone())),
                call,
            });
        }
    }
    work
}

/// What `Stop` can end among `work`: a command only where the Session can
/// stop tasks, any subagent that is running.
pub(super) fn stop_targets(open: &ferrite_core::cockpit::ThreadView<'_>, work: &[Work]) -> Vec<StopTarget> {
    let tasks = open.supports_control(ferrite_core::ControlKind::StopTask);
    work.iter()
        .filter_map(|work| work.stop.clone())
        .filter(|target| tasks || matches!(target, StopTarget::Agent(_)))
        .collect()
}

/// What `Stop` says it ends, so a line of commands and agents never offers
/// a bare `Stop` that ends only some of them.
fn stop_scope(targets: &[StopTarget]) -> &'static str {
    let commands = targets.iter().any(|target| matches!(target, StopTarget::Command(_)));
    let agents = targets.iter().filter(|target| matches!(target, StopTarget::Agent(_))).count();
    match (commands, agents) {
        (true, 0) | (false, 0) => "Stop",
        (false, 1) => "Stop agent",
        (false, _) => "Stop agents",
        (true, _) => "Stop all",
    }
}

fn refusal_of<'a>(
    open: &'a ferrite_core::cockpit::ThreadView<'_>,
    target: &StopTarget,
) -> Option<(u32, &'a str)> {
    match target {
        StopTarget::Command(id) => open.transcript().stop_refusal(id),
        StopTarget::Agent(_) => None,
    }
}

impl CockpitView {
    /// The background work line's title for Pane `index`, as it draws.
    #[cfg(test)]
    pub(super) fn background_work_title(&self, index: usize) -> Option<String> {
        let open = self.cockpit.thread(self.panes.get(index)?.thread()?)?;
        let tasks: Vec<BackgroundTask> =
            running_work(&open).into_iter().map(|work| work.task).collect();
        pane::present_background_work(&tasks, !open.busy()).map(|presented| presented.title)
    }

    /// What the line's `Stop` says it ends, for Pane `index`.
    #[cfg(test)]
    pub(super) fn background_stop_scope(&self, index: usize) -> Option<&'static str> {
        let open = self.cockpit.thread(self.panes.get(index)?.thread()?)?;
        let targets = stop_targets(&open, &running_work(&open));
        (!targets.is_empty()).then(|| stop_scope(&targets))
    }

    /// Why the line says the stop of task `id` of Pane `index` failed.
    #[cfg(test)]
    pub(super) fn background_stop_reason(&self, index: usize, id: &str) -> Option<String> {
        let thread = self.panes.get(index)?.thread()?;
        let open = self.cockpit.thread(thread)?;
        let target = StopTarget::Command(id.into());
        match self.stop_state(&open, thread, open.generation(), &target)? {
            StopState::Refused(why) => Some(why),
            _ => None,
        }
    }

    /// The line under Main's transcript, none while nothing runs in the
    /// background.
    pub(super) fn background_work_tail(&self, index: usize) -> Option<TranscriptTail> {
        let pane = self.panes.get(index)?;
        if !pane.is_main() {
            return None;
        }
        let thread = pane.thread()?;
        let open = self.cockpit.thread(thread)?;
        let work = running_work(&open);
        let waiting = !open.busy();
        let tasks: Vec<BackgroundTask> = work.iter().map(|work| work.task.clone()).collect();
        let presented = pane::present_background_work(&tasks, waiting)?;
        let targets = stop_targets(&open, &work);
        let stoppable = !targets.is_empty();
        let scope = stop_scope(&targets);
        let generation = open.generation();
        let stops: Vec<Option<StopState>> = targets
            .iter()
            .map(|target| self.stop_state(&open, thread, generation, target))
            .collect();
        let stopping = stops
            .iter()
            .all(|stop| matches!(stop, Some(StopState::Stopping { .. })));
        let refused = stops.iter().find_map(|stop| match stop {
            Some(StopState::Refused(why)) => Some(why.clone()),
            _ => None,
        });
        let single = match work.as_slice() {
            [only] => Some((
                only.task.launched,
                only.call.clone().filter(|_| only.task.kind == WorkKind::Subagent),
            )),
            _ => None,
        };
        let key = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (
                "background-work",
                thread,
                &presented.title,
                &presented.description,
                scope,
                stopping,
                &refused,
                work.iter().map(|work| (&work.stop, &work.call)).collect::<Vec<_>>(),
            )
                .hash(&mut hasher);
            hasher.finish()
        };
        let owner = self.decisions.owner();
        Some(TranscriptTail {
            key,
            render: std::rc::Rc::new(move |_: &mut Window, _: &mut gpui::App| {
                let mut actions: Vec<AnyElement> = Vec::new();
                if let Some((Some(launched), _)) = &single {
                    actions.push(pane::background_clock(*launched).into_any_element());
                }
                let details = owner.clone();
                actions.push(
                    pane::inline_button(("background-details", thread.get() as usize), "Details")
                        .debug_selector(|| "background-details".into())
                        .on_click(move |_, _, cx| {
                            let _ = details.update(cx, |view, cx| {
                                view.open_background_dialog(thread, cx)
                            });
                        })
                        .into_any_element(),
                );
                if let Some((_, Some(call))) = single.clone() {
                    let opener = owner.clone();
                    actions.push(
                        pane::inline_button(("background-open", thread.get() as usize), "Open")
                            .text_color(rgb(ferrite_ui::theme::INK_SUBTLE))
                            .on_click(move |_, _, cx| {
                                let call = call.clone();
                                let _ = opener.update(cx, |view, cx| {
                                    view.open_background_agent(thread, &call, cx)
                                });
                            })
                            .into_any_element(),
                    );
                }
                if stoppable {
                    let stopper = owner.clone();
                    if let Some(why) = &refused {
                        actions.push(
                            div()
                                .debug_selector(|| "background-stop-refused".into())
                                .text_color(rgb(ferrite_ui::theme::BLOCKED))
                                .child(SharedString::from(format!("Couldn\u{2019}t stop: {why}")))
                                .into_any_element(),
                        );
                    }
                    let stop = pane::inline_button(
                        ("background-stop-all", thread.get() as usize),
                        if stopping {
                            "Stopping..."
                        } else if refused.is_some() {
                            "Retry"
                        } else {
                            scope
                        },
                    );
                    actions.push(
                        if stopping {
                            stop.debug_selector(|| "background-stopping".into())
                        } else {
                            stop.debug_selector(|| "background-stop-all".into()).on_click(move |_, _, cx| {
                                let _ = stopper
                                    .update(cx, |view, cx| view.stop_background_work(thread, cx));
                            })
                        }
                        .into_any_element(),
                    );
                }
                pane::background_work_line(key, presented.clone(), actions).into_any_element()
            }),
        })
    }

    /// Where the stop of `target` stands: what the cockpit sent, read with
    /// what the provider has since said of it.
    fn stop_state(
        &self,
        open: &ferrite_core::cockpit::ThreadView<'_>,
        thread: ThreadId,
        generation: u64,
        target: &StopTarget,
    ) -> Option<StopState> {
        let sent = self
            .background_stops
            .get(&(thread, generation, target.clone()))?;
        let refusal = refusal_of(open, target);
        match (sent, refusal) {
            (
                StopState::Stopping { answered, .. } | StopState::Unsettled { answered },
                Some((count, why)),
            ) if count > *answered => Some(StopState::Refused(why.to_owned())),
            (StopState::Unsettled { .. }, _) => Some(StopState::Refused(STOP_NOT_SETTLED.into())),
            _ => Some(sent.clone()),
        }
    }

    /// `Stop` on the line: every piece of work this Session still runs that
    /// it can stop.
    fn stop_background_work(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        let generation = open.generation();
        for target in stop_targets(&open, &running_work(&open)) {
            self.send_stop(thread, generation, target, cx);
        }
        cx.notify();
    }

    /// Stop `target` of `thread`'s Session `generation`, if that very
    /// Session still runs it: a press that lands as the work is handed back
    /// stops nothing else.
    pub(super) fn send_stop(
        &mut self,
        thread: ThreadId,
        generation: u64,
        target: StopTarget,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self
            .cockpit
            .thread(thread)
            .filter(|open| open.generation() == generation)
        else {
            return;
        };
        if !stop_targets(&open, &running_work(&open)).contains(&target) {
            return;
        }
        let stop = (thread, generation, target.clone());
        let count = refusal_of(&open, &target).map_or(0, |(count, _)| count);
        let answered = match self.background_stops.get(&stop) {
            Some(StopState::Unsettled { answered }) if count <= *answered => answered + 1,
            _ => count,
        };
        self.background_stop_sends += 1;
        let nonce = self.background_stop_sends;
        self.background_stops
            .insert(stop.clone(), StopState::Stopping { nonce, answered });
        self.offer_retry_unless_settled(stop.clone(), nonce, cx);
        match target {
            StopTarget::Command(id) => {
                self.run_session_control(
                    thread,
                    generation,
                    ferrite_core::SessionControl::StopTask { id },
                );
                if let Some((_, _, why)) = self.session_control_error.as_ref().filter(
                    |(shown, shown_generation, _)| *shown == thread && *shown_generation == generation,
                ) {
                    self.background_stops.insert(stop, StopState::Refused(why.clone()));
                }
            }
            StopTarget::Agent(key) => {
                self.stop_subagent(thread, &key);
            }
        }
        cx.notify();
    }

    /// A provider that never answers a stop leaves the work running: after
    /// `STOP_SETTLES_WITHIN` the line says so and offers `Retry`.
    fn offer_retry_unless_settled(
        &self,
        stop: (ThreadId, u64, StopTarget),
        nonce: u64,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(STOP_SETTLES_WITHIN).await;
            this.update(cx, |view, cx| {
                if let Some(state) = view.background_stops.get_mut(&stop) {
                    if let StopState::Stopping { nonce: sent, answered } = *state {
                        if sent == nonce {
                            *state = StopState::Unsettled { answered };
                            cx.notify();
                        }
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// Forget the stops of work that has ended, or of a Session that has.
    pub(super) fn prune_background_stops(&mut self) {
        if self.background_stops.is_empty() {
            return;
        }
        let mut running: std::collections::HashMap<(ThreadId, u64), Vec<StopTarget>> =
            Default::default();
        for (thread, _, _) in self.background_stops.keys() {
            if let Some(open) = self.cockpit.thread(*thread) {
                running
                    .entry((*thread, open.generation()))
                    .or_insert_with(|| stop_targets(&open, &running_work(&open)));
            }
        }
        self.background_stops.retain(|(thread, generation, target), _| {
            running
                .get(&(*thread, *generation))
                .is_some_and(|targets| targets.contains(target))
        });
    }

    /// `Open` on the line, or `Open thread` in the dialog: the agent the
    /// call started, in the Panel.
    pub(super) fn open_background_agent(&mut self, thread: ThreadId, call: &str, cx: &mut Context<Self>) {
        self.background_dialog = None;
        if let Some(subject) = self.agent_of_call(thread, call) {
            self.open_agent_drill(thread, subject, cx);
        }
        cx.notify();
    }

    pub(crate) fn open_background_dialog(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.background_dialog = Some(thread);
        cx.notify();
    }

    /// Close the `Background work` dialog, if it is open; whether it was.
    pub(super) fn close_background_dialog(&mut self, cx: &mut Context<Self>) -> bool {
        let was = self.background_dialog.take().is_some();
        if was {
            cx.notify();
        }
        was
    }

    /// The `Background work` dialog (T3's `BackgroundWorkDetails`): one
    /// section per piece of work, 20px apart.
    pub(super) fn background_dialog_element(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
        let thread = self.background_dialog?;
        let open = self.cockpit.thread(thread)?;
        let waiting = !open.busy();
        let mut sections = div().flex().flex_col().gap(px(BACKGROUND_DIALOG_SECTION_GAP));
        for work in running_work(&open) {
            let call = work
                .call
                .as_deref()
                .and_then(|call| open.transcript().tool(call));
            sections = sections.child(self.work_section(thread, &work, call, waiting, cx));
        }
        let close = ferrite_ui::sheet::sheet_close("background-dialog-close", "Close", cx)
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                view.close_background_dialog(cx);
            }));
        let card = ferrite_ui::sheet::sheet_fit(
            BACKGROUND_DIALOG_W,
            BACKGROUND_DIALOG_MAX_H.min(f32::from(window.viewport_size().height) * MODAL_VIEWPORT_FRACTION),
        )
        .id("background-dialog")
        .debug_selector(|| "background-dialog".into())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
        )
        .child(ferrite_ui::sheet::sheet_head("Background work", close))
        .child(
            div()
                .id("background-dialog-body")
                .flex()
                .flex_col()
                .flex_shrink(1.)
                .min_h_0()
                .overflow_y_scroll()
                .p(px(MODAL_PAD))
                .child(sections),
        );
        let sheet = ferrite_ui::motion::veil_in(
            "background-dialog-veil",
            ferrite_ui::motion::VEIL,
            ferrite_ui::sheet::veil()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.close_background_dialog(cx);
                    }),
                )
                .child(ferrite_ui::motion::dialog_in("background-dialog-in", card)),
        );
        Some(deferred(sheet).with_priority(3).into_any_element())
    }

    /// One piece of work in the dialog: its title and standing, the command
    /// in a box, its output so far, and `Open thread` for an agent.
    fn work_section(
        &self,
        thread: ThreadId,
        work: &Work,
        call: Option<&ToolBlock>,
        waiting: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        use ferrite_ui::theme::*;
        let agent = work.task.kind == WorkKind::Subagent;
        let command = call
            .map(|call| call.summary.trim().to_owned())
            .filter(|command| !agent && !command.is_empty());
        let label = work.task.label.trim();
        let title = if label.is_empty() || command.as_deref() == Some(label) {
            if agent { "Subagent" } else { "Command" }.to_owned()
        } else {
            label.to_owned()
        };
        let command = command.map(|command| {
            div()
                .p(px(SPACE_3))
                .rounded(px(R_CONTROL))
                .border_1()
                .border_color(paint::FLOAT_EDGE)
                .bg(paint::RAISED)
                .font_family(FONT_CODE)
                .text_size(px(FS_BADGE))
                .text_color(rgb(INK_STRONG))
                .child(SharedString::from(command))
        });
        let open_thread = work.call.clone().filter(|_| agent).map(|target| {
            ferrite_pane::pane::inline_button(
                SharedString::from(format!("background-dialog-open-{target}")),
                "Open thread",
            )
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.open_background_agent(thread, &target, cx);
            }))
        });
        div()
            .debug_selector(|| "background-dialog-section".into())
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .text_size(px(FS_LABEL))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .justify_between()
                    .gap(px(SPACE_3))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(W_LABEL)
                            .text_color(rgb(INK_STRONG))
                            .child(SharedString::from(title)),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(FS_BADGE))
                            .text_color(rgb(INK_SUBTLE))
                            .child(if waiting { "Waiting" } else { "Running" }),
                    ),
            )
            .children(command)
            .children(call.map(|call| output_block(call, false, false)))
            .children(open_thread)
    }
}
