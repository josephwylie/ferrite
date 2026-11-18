//! Background work at the transcript's foot (T3's `ThreadNowLine`, ADR
//! 0038): while a Thread has work running in the background, the last row
//! of its Main transcript says so (`Waiting on 2 subagents`), with
//! `Details`, which opens the `Background work` dialog
//! (`background_dialog_view.rs`), and `Stop`. The
//! work itself lives in the rail's Background tasks section and the
//! Background Surface (`panel::background`); the line and its dialog read
//! the same calls (`Transcript::background_calls`).

use std::hash::{Hash, Hasher};

use super::panel::background::label;
use super::*;

use ferrite_core::activity::AgentKey;
use ferrite_core::transcript::tools::{policy, ToolKind};
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
/// subagent is an agent launched into the background, or any still
/// working after Main's turn.
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
/// the provider's tasks, the agents still working and the calls that
/// launched work into the background that nothing else accounts for.
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
    for (names, agent) in open.activity().running_agents() {
        let stoppable = agent.status() != AgentStatus::Unknown;
        // While Main's turn runs, an agent it launched in the foreground is
        // work it waits on, not background work.
        if open.busy() && !names.iter().any(|name| transcript.running_in_background(name)) {
            continue;
        }
        // The name the transcript holds a call by, for the label and the
        // drill-in.
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
    let known: Vec<String> = work.iter().filter_map(|work| work.call.clone()).collect();
    for call in transcript.background_calls() {
        let Some(background) = call.background.as_ref().filter(|_| call.in_background()) else {
            continue;
        };
        if known.contains(&call.call) {
            continue;
        }
        let agent = policy(&call.name).kind == ToolKind::Agent;
        work.push(Work {
            task: BackgroundTask {
                kind: if agent { WorkKind::Subagent } else { WorkKind::Command },
                label: label(call).to_string(),
                launched: Some(background.launched),
            },
            stop: (!agent).then(|| StopTarget::Command(background.task.clone())),
            call: Some(call.call.clone()),
        });
    }
    work
}

/// What the line under Main's transcript counts: while Main's turn runs its
/// agents are rows of the transcript, so only commands are waited on.
pub(super) fn line_work(open: &ferrite_core::cockpit::ThreadView<'_>) -> Vec<Work> {
    let mut work = running_work(open);
    if open.busy() {
        work.retain(|work| work.task.kind == WorkKind::Command);
    }
    work
}

/// What `Stop` can end among `work`: a command only where the Session can
/// stop tasks, a subagent only once its status is known to be running.
pub(super) fn stop_targets(open: &ferrite_core::cockpit::ThreadView<'_>, work: &[Work]) -> Vec<StopTarget> {
    let tasks = open.supports_control(ferrite_core::ControlKind::StopTask);
    work.iter()
        .filter_map(|work| work.stop.clone())
        .filter(|target| tasks || matches!(target, StopTarget::Agent(_)))
        .collect()
}

/// What `Stop` ends, and what it leaves running: a bare `Stop` is only for
/// a line it ends whole.
#[derive(Hash)]
struct StopScope {
    label: &'static str,
    remains: Option<String>,
}

fn stop_scope(work: &[Work], targets: &[StopTarget]) -> StopScope {
    let ends_commands = targets.iter().any(|target| matches!(target, StopTarget::Command(_)));
    let ends_agents = targets.len() - targets.iter().filter(|target| matches!(target, StopTarget::Command(_))).count();
    let left = |kind| {
        work.iter()
            .filter(|work| work.task.kind == kind && !work.stop.as_ref().is_some_and(|stop| targets.contains(stop)))
            .count()
    };
    let (commands, agents) = (left(WorkKind::Command), left(WorkKind::Subagent));
    let whole = commands + agents == 0;
    let label = match (ends_commands, ends_agents) {
        (true, 0) if whole => "Stop",
        (true, 0) => "Stop commands",
        (false, 1) => "Stop agent",
        (false, _) => "Stop agents",
        (true, _) if whole => "Stop all",
        (true, _) => "Stop commands and agents",
    };
    let counted = |count: usize, noun: &str| {
        (count > 0).then(|| format!("{count} {noun}{}", if count == 1 { "" } else { "s" }))
    };
    let parts: Vec<String> = [counted(commands, "command"), counted(agents, "agent")]
        .into_iter()
        .flatten()
        .collect();
    StopScope {
        label,
        remains: (!parts.is_empty()).then(|| format!("{} left running", parts.join(" and "))),
    }
}

/// The line's and the dialog's `Stop`, as they draw.
#[derive(Hash)]
pub(super) struct StopControl {
    scope: StopScope,
    stopping: bool,
    refused: Option<String>,
}

/// The refusal, what stays running and the button itself; `prefix` names
/// them apart in the line and in the dialog.
pub(super) fn stop_elements(
    control: &StopControl,
    thread: ThreadId,
    prefix: &'static str,
    on_stop: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> Vec<AnyElement> {
    let mut elements: Vec<AnyElement> = Vec::new();
    if let Some(why) = &control.refused {
        elements.push(
            div()
                .debug_selector(move || format!("{prefix}-stop-refused"))
                .text_color(rgb(ferrite_ui::theme::BLOCKED))
                .child(SharedString::from(format!("Couldn\u{2019}t stop: {why}")))
                .into_any_element(),
        );
    }
    let label = if control.stopping {
        "Stopping..."
    } else if control.refused.is_some() {
        "Retry"
    } else {
        control.scope.label
    };
    let button = pane::inline_button(SharedString::from(format!("{prefix}-stop-all-{}", thread.get())), label);
    let button = if control.stopping {
        button.debug_selector(move || format!("{prefix}-stopping"))
    } else {
        button
            .debug_selector(move || format!("{prefix}-stop-all"))
            .on_click(on_stop)
    };
    elements.push(
        div()
            .debug_selector(move || format!("{prefix}-stop-says-{label}"))
            .child(button)
            .into_any_element(),
    );
    if let Some(remains) = &control.scope.remains {
        let selector = format!("{prefix}-stop-remains-{remains}");
        elements.push(
            div()
                .debug_selector(move || selector.clone())
                .text_color(rgb(ferrite_ui::theme::INK_SUBTLE))
                .child(SharedString::from(remains.clone()))
                .into_any_element(),
        );
    }
    elements
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
            line_work(&open).into_iter().map(|work| work.task).collect();
        pane::present_background_work(&tasks, !open.busy()).map(|presented| presented.title)
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
        let work = line_work(&open);
        let waiting = !open.busy();
        let tasks: Vec<BackgroundTask> = work.iter().map(|work| work.task.clone()).collect();
        let presented = pane::present_background_work(&tasks, waiting)?;
        let stop = self.stop_control(&open, thread, &work);
        let single = match work.as_slice() {
            [only] => Some((
                only.task.launched,
                only.call.clone().filter(|_| only.task.kind == WorkKind::Subagent),
            )),
            _ => None,
        };
        let clock_width = lone_launch(&work).map(|launched| clock_reading(launched).len());
        let key = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (
                "background-work",
                thread,
                &presented.title,
                &presented.description,
                &stop,
                work.iter().map(|work| (&work.stop, &work.call)).collect::<Vec<_>>(),
                clock_width,
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
                    actions.push(
                        pane::background_clock(*launched)
                            .debug_selector(|| "background-line-clock".into())
                            .into_any_element(),
                    );
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
                if let Some(control) = &stop {
                    let stopper = owner.clone();
                    actions.extend(stop_elements(control, thread, "background", move |_, _, cx| {
                        let _ = stopper.update(cx, |view, cx| {
                            view.stop_background_work(thread, line_work, cx)
                        });
                    }));
                }
                pane::background_work_line(key, presented.clone(), actions).into_any_element()
            }),
        })
    }

    /// Lay the line out again when its clock next changes width (`9s` to
    /// `10s`, `59s` to `1m`): the overlay redraws the clock each second in
    /// the box the last layout gave it, so a wider reading would run into
    /// `Details`.
    pub(super) fn wake_for_background_clock(&self, index: usize, cx: &mut Context<Self>) {
        if ferrite_ui::motion::reduced_motion(cx) {
            return;
        }
        let Some(launched) = self
            .panes
            .get(index)
            .filter(|pane| pane.is_main())
            .and_then(|pane| pane.thread())
            .and_then(|thread| self.cockpit.thread(thread))
            .and_then(|open| lone_launch(&line_work(&open)))
        else {
            return;
        };
        let elapsed = ferrite_core::clock::instant().saturating_duration_since(launched).as_secs();
        let width = clock_reading(launched).len();
        let Some(change) = (elapsed + 1..=elapsed + 61).find(|secs| {
            ferrite_core::progress::live_seconds(Duration::from_secs(*secs)).len() != width
        }) else {
            return;
        };
        let since = Duration::from_secs(change)
            .saturating_sub(ferrite_core::clock::instant().saturating_duration_since(launched));
        let at = cx.background_executor().now() + since;
        ferrite_ui::motion::wake_at(cx.entity_id(), at, cx);
    }

    /// The `Stop` of `work`: what it says, and where the stops it sent stand.
    pub(super) fn stop_control(
        &self,
        open: &ferrite_core::cockpit::ThreadView<'_>,
        thread: ThreadId,
        work: &[Work],
    ) -> Option<StopControl> {
        let targets = stop_targets(open, work);
        if targets.is_empty() {
            return None;
        }
        let generation = open.generation();
        let stops: Vec<Option<StopState>> = targets
            .iter()
            .map(|target| self.stop_state(open, thread, generation, target))
            .collect();
        Some(StopControl {
            scope: stop_scope(work, &targets),
            stopping: stops.iter().all(|stop| matches!(stop, Some(StopState::Stopping { .. }))),
            refused: stops.iter().find_map(|stop| match stop {
                Some(StopState::Refused(why)) => Some(why.clone()),
                _ => None,
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

    /// `Stop` on the line or in the dialog: the work they list that this
    /// Session can stop.
    pub(super) fn stop_background_work(
        &mut self,
        thread: ThreadId,
        work_of: fn(&ferrite_core::cockpit::ThreadView<'_>) -> Vec<Work>,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        let generation = open.generation();
        for target in stop_targets(&open, &work_of(&open)) {
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
        if self.background_dialog(cx).is_some() {
            self.hide_sheet(cx);
        }
        if let Some(subject) = self.agent_of_call(thread, call) {
            self.open_agent_drill(thread, subject, cx);
        }
        cx.notify();
    }
}

/// When the line's one piece of work launched, the line's clock shows it.
fn lone_launch(work: &[Work]) -> Option<std::time::Instant> {
    match work {
        [only] => only.task.launched,
        _ => None,
    }
}

/// The line's clock as it reads now.
fn clock_reading(launched: std::time::Instant) -> String {
    ferrite_core::progress::live_seconds(
        ferrite_core::clock::instant().saturating_duration_since(launched),
    )
}
