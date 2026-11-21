//! Background work at the transcript's foot (T3's `ThreadNowLine`, ADR
//! 0038): while a Thread has work running in the background, the last row
//! of its Main transcript says so (`Waiting on 2 subagents`), with
//! `Details`, which opens the `Background work` dialog
//! (`background_dialog_view.rs`), and `Stop`. The
//! work itself lives in the rail's Background tasks section and the
//! Background Surface (`panel::background`); the line and its dialog read
//! the same calls (`Transcript::background_calls`).
//!
//! Its state is its own view's (ADR 0041), `BackgroundView`: the stops
//! sent and where each stands, and the cards turned from how they start.
//! It has no render of its own: the root draws it wherever it shows (the
//! line, the Overview's section, the Surface) and the dialog reads its
//! `Stop` from it, each with `BackgroundInputs`, which the root reads off
//! the Thread. What the operator asks of it it says as a
//! `BackgroundEvent`. Sending a stop is a Server call, so the root sends
//! it and tells the view; the view keeps the count of sends and the timer
//! that offers `Retry`.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use gpui::{App, WeakEntity};

use super::panel::background::label;
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
/// subagent is an agent launched into the background, or any still
/// working after Main's turn.
#[derive(Clone)]
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
fn running_work(open: &ferrite_core::cockpit::ThreadView<'_>) -> Vec<Work> {
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
                    activity: None,
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
                activity: Some(
                    agent.transcript().progress().detail.split_whitespace().collect::<Vec<_>>().join(" "),
                ),
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
                activity: None,
            },
            stop: (!agent).then(|| StopTarget::Command(background.task.clone())),
            call: Some(call.call.clone()),
        });
    }
    work
}

/// A sent stop: by Thread and Session generation (a provider reuses task
/// ids), and what it ends.
type Stop = (ThreadId, u64, StopTarget);

/// A call whose background work has ended.
pub(super) struct Finished {
    pub(super) call: String,
    pub(super) title: SharedString,
    /// How long it ran, where the activity timed it.
    pub(super) took: Option<String>,
}

/// What the background work draws of one Thread, as the window has it this
/// frame.
pub(crate) struct BackgroundInputs {
    pub(super) thread: ThreadId,
    pub(super) generation: u64,
    /// Main's turn runs.
    pub(super) busy: bool,
    /// The Session can stop a command.
    stops_tasks: bool,
    pub(super) running: Vec<Work>,
    /// What the provider said to the stops of each running command: how
    /// many times it refused, and the last reason.
    refusals: HashMap<String, (u32, String)>,
    /// Oldest first.
    pub(super) finished: Vec<Finished>,
    /// The blocks of the calls above, by call, where they were asked for.
    pub(super) calls: HashMap<String, ToolBlock>,
}

impl BackgroundInputs {
    /// What the line under Main's transcript counts: while Main's turn runs
    /// its agents are rows of the transcript, so only commands are waited
    /// on.
    pub(super) fn line_work(&self) -> Vec<Work> {
        let mut work = self.running.clone();
        if self.busy {
            work.retain(|work| work.task.kind == WorkKind::Command);
        }
        work
    }

    /// What `Stop` can end among `work`: a command only where the Session
    /// can stop tasks, a subagent only once its status is known to be
    /// running.
    pub(super) fn targets(&self, work: &[Work]) -> Vec<StopTarget> {
        work.iter()
            .filter_map(|work| work.stop.clone())
            .filter(|target| self.stops_tasks || matches!(target, StopTarget::Agent(_)))
            .collect()
    }

    fn refusal(&self, target: &StopTarget) -> Option<(u32, &str)> {
        match target {
            StopTarget::Command(id) => self.refusals.get(id).map(|(count, why)| (*count, why.as_str())),
            StopTarget::Agent(_) => None,
        }
    }
}

/// What a `Stop` (T3's, one word whatever it ends) leaves running: work the
/// Session cannot stop, which the line names beside it.
fn stop_leaves(work: &[Work], targets: &[StopTarget]) -> Option<String> {
    let left = |kind| {
        work.iter()
            .filter(|work| work.task.kind == kind && !work.stop.as_ref().is_some_and(|stop| targets.contains(stop)))
            .count()
    };
    let (commands, agents) = (left(WorkKind::Command), left(WorkKind::Subagent));
    let counted = |count: usize, noun: &str| {
        (count > 0).then(|| format!("{count} {noun}{}", if count == 1 { "" } else { "s" }))
    };
    let parts: Vec<String> = [counted(commands, "command"), counted(agents, "agent")]
        .into_iter()
        .flatten()
        .collect();
    (!parts.is_empty()).then(|| format!("{} left running", parts.join(" and ")))
}

/// The line's and the dialog's `Stop`, as they draw.
#[derive(Hash)]
pub(super) struct StopControl {
    leaves: Option<String>,
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
        "Stop"
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
    if let Some(remains) = &control.leaves {
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

/// What the background work asks of the window.
#[derive(Clone)]
pub(crate) enum BackgroundEvent {
    /// `Details` on the line: the `Background work` dialog on the Thread.
    Details(ThreadId),
    /// A row of the Overview's section: the Background Surface.
    Open(ThreadId),
    /// `Open` on an agent's card in the Background Surface: the agent the
    /// call started.
    OpenAgent { thread: ThreadId, call: String },
    /// The line's `Stop`, or `Retry`: every piece of work it lists that the
    /// Session can stop.
    StopLine(ThreadId),
    /// A stop square: `target` of the Thread's Session `generation`.
    Stop { thread: ThreadId, generation: u64, target: StopTarget },
}

/// Every Thread's background work as the window keeps it: the stops sent
/// and the cards turned.
pub(crate) struct BackgroundView {
    /// The work a `Stop` was pressed for: the line reads `Stopping...`
    /// until it ends, or `Retry` once one refused.
    stops: HashMap<Stop, StopState>,
    /// How many stops `stops` has taken, to tell one send's timer from
    /// another's.
    sends: u64,
    /// The Background Surface's cards turned from how they start (running
    /// open, finished closed), by call.
    turned: HashMap<ThreadId, HashSet<String>>,
}

impl EventEmitter<BackgroundEvent> for BackgroundView {}

impl BackgroundView {
    pub(super) fn new() -> Self {
        Self {
            stops: HashMap::new(),
            sends: 0,
            turned: HashMap::new(),
        }
    }

    /// The line under Main's transcript, none while nothing runs in the
    /// background.
    pub(super) fn line(&self, this: WeakEntity<Self>, inputs: &BackgroundInputs) -> Option<TranscriptTail> {
        let thread = inputs.thread;
        let work = inputs.line_work();
        let waiting = !inputs.busy;
        let tasks: Vec<BackgroundTask> = work.iter().map(|work| work.task.clone()).collect();
        let presented = pane::present_background_work(&tasks, waiting)?;
        let stop = self.stop_control(inputs, &work);
        let single = match work.as_slice() {
            [only] => only.task.launched,
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
        Some(TranscriptTail {
            key,
            render: std::rc::Rc::new(move |_: &mut Window, _: &mut gpui::App| {
                let emit = |event: BackgroundEvent| {
                    let this = this.clone();
                    move |_: &ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                        let event = event.clone();
                        let _ = this.update(cx, |_, cx| cx.emit(event));
                    }
                };
                let mut actions: Vec<AnyElement> = Vec::new();
                if let Some(launched) = single {
                    actions.push(
                        pane::background_clock(launched)
                            .debug_selector(|| "background-line-clock".into())
                            .into_any_element(),
                    );
                }
                actions.push(
                    pane::inline_button(("background-details", thread.get() as usize), "Details")
                        .debug_selector(|| "background-details".into())
                        .on_click(emit(BackgroundEvent::Details(thread)))
                        .into_any_element(),
                );
                if let Some(control) = &stop {
                    actions.extend(stop_elements(
                        control,
                        thread,
                        "background",
                        emit(BackgroundEvent::StopLine(thread)),
                    ));
                }
                pane::background_work_line(key, presented.clone(), actions).into_any_element()
            }),
        })
    }

    /// The `Stop` of `work`: what it says, and where the stops it sent stand.
    pub(super) fn stop_control(&self, inputs: &BackgroundInputs, work: &[Work]) -> Option<StopControl> {
        let targets = inputs.targets(work);
        if targets.is_empty() {
            return None;
        }
        let stops: Vec<Option<StopState>> = targets
            .iter()
            .map(|target| self.stop_state(inputs, target))
            .collect();
        Some(StopControl {
            leaves: stop_leaves(work, &targets),
            stopping: stops.iter().all(|stop| matches!(stop, Some(StopState::Stopping { .. }))),
            refused: stops.iter().find_map(|stop| match stop {
                Some(StopState::Refused(why)) => Some(why.clone()),
                _ => None,
            }),
        })
    }

    /// Where the stop of `target` stands: what was sent, read with what the
    /// provider has since said of it.
    fn stop_state(&self, inputs: &BackgroundInputs, target: &StopTarget) -> Option<StopState> {
        let sent = self
            .stops
            .get(&(inputs.thread, inputs.generation, target.clone()))?;
        match (sent, inputs.refusal(target)) {
            (
                StopState::Stopping { answered, .. } | StopState::Unsettled { answered },
                Some((count, why)),
            ) if count > *answered => Some(StopState::Refused(why.to_owned())),
            (StopState::Unsettled { .. }, _) => Some(StopState::Refused(STOP_NOT_SETTLED.into())),
            _ => Some(sent.clone()),
        }
    }

    /// `stop` was sent, when the provider had refused its task `refused`
    /// times: it reads `Stopping...` until it is answered or the timer
    /// offers `Retry`.
    fn sent(&mut self, stop: Stop, refused: u32, cx: &mut Context<Self>) {
        let answered = match self.stops.get(&stop) {
            Some(StopState::Unsettled { answered }) if refused <= *answered => answered + 1,
            _ => refused,
        };
        self.sends += 1;
        let nonce = self.sends;
        self.stops.insert(stop.clone(), StopState::Stopping { nonce, answered });
        self.offer_retry_unless_settled(stop, nonce, cx);
        cx.notify();
    }

    /// The send of `stop` failed, with the reason.
    fn refused(&mut self, stop: Stop, why: String, cx: &mut Context<Self>) {
        self.stops.insert(stop, StopState::Refused(why));
        cx.notify();
    }

    /// A provider that never answers a stop leaves the work running: after
    /// `STOP_SETTLES_WITHIN` the line says so and offers `Retry`.
    fn offer_retry_unless_settled(&self, stop: Stop, nonce: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(STOP_SETTLES_WITHIN).await;
            this.update(cx, |view, cx| {
                if let Some(state) = view.stops.get_mut(&stop) {
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

    /// Forget the stops of work that has ended, or of a Session that has:
    /// `running` is each Thread's Session generation and what `Stop` can
    /// end in it, none for a Thread not open.
    fn prune(&mut self, running: impl Fn(ThreadId) -> Option<(u64, Vec<StopTarget>)>) {
        if self.stops.is_empty() {
            return;
        }
        let mut seen: HashMap<ThreadId, Option<(u64, Vec<StopTarget>)>> = HashMap::new();
        self.stops.retain(|(thread, generation, target), _| {
            seen.entry(*thread)
                .or_insert_with(|| running(*thread))
                .as_ref()
                .is_some_and(|(current, targets)| current == generation && targets.contains(target))
        });
    }

    /// A card's head pressed: it turns from how it starts, or back.
    pub(super) fn turn(&mut self, thread: ThreadId, call: String, cx: &mut Context<Self>) {
        let turned = self.turned.entry(thread).or_default();
        if !turned.remove(&call) {
            turned.insert(call);
        }
        cx.notify();
    }

    /// Whether `thread`'s card on `call` is turned from how it starts.
    pub(super) fn is_turned(&self, thread: ThreadId, call: &str) -> bool {
        self.turned.get(&thread).is_some_and(|turned| turned.contains(call))
    }

    /// Everything of a Thread `keep` refuses goes.
    pub(super) fn retain_threads(&mut self, keep: impl Fn(ThreadId) -> bool) {
        self.stops.retain(|(thread, _, _), _| keep(*thread));
        self.turned.retain(|thread, _| keep(*thread));
    }
}

#[cfg(test)]
impl BackgroundView {
    /// How many stops are remembered.
    pub(super) fn stops_held(&self) -> usize {
        self.stops.len()
    }

    /// How much of `thread`'s background work is held.
    pub(super) fn held_for_test(&self, thread: ThreadId) -> usize {
        self.stops.keys().filter(|(held, _, _)| *held == thread).count()
            + self.turned.get(&thread).map_or(0, HashSet::len)
    }
}

impl CockpitView {
    /// What the background work draws of `thread`, none once it is not
    /// open. `outputs`: with the blocks of its calls, for their output.
    pub(super) fn background_inputs(&self, thread: ThreadId, outputs: bool) -> Option<BackgroundInputs> {
        let open = self.cockpit.thread(thread)?;
        let transcript = open.transcript();
        let running = running_work(&open);
        let refusals = running
            .iter()
            .filter_map(|work| match &work.stop {
                Some(StopTarget::Command(id)) => transcript
                    .stop_refusal(id)
                    .map(|(count, why)| (id.clone(), (count, why.to_owned()))),
                _ => None,
            })
            .collect();
        let timings = open.activity().main().timings();
        let ended: Vec<&ToolBlock> = transcript
            .background_calls()
            .filter(|call| !call.in_background())
            .collect();
        let finished = ended
            .iter()
            .map(|call| Finished {
                call: call.call.clone(),
                title: label(call),
                took: timings
                    .get(&call.call)
                    .map(|timing| ferrite_core::progress::settled_duration_label(timing.elapsed())),
            })
            .collect();
        let calls = if outputs {
            running
                .iter()
                .filter_map(|work| work.call.as_deref())
                .filter_map(|call| Some((call.to_owned(), transcript.tool(call)?.clone())))
                .chain(ended.iter().map(|call| (call.call.clone(), (*call).clone())))
                .collect()
        } else {
            HashMap::new()
        };
        Some(BackgroundInputs {
            thread,
            generation: open.generation(),
            busy: open.busy(),
            stops_tasks: open.supports_control(ferrite_core::ControlKind::StopTask),
            running,
            refusals,
            finished,
            calls,
        })
    }

    /// The background work line's title for Pane `index`, as it draws.
    #[cfg(test)]
    pub(super) fn background_work_title(&self, index: usize) -> Option<String> {
        let inputs = self.background_inputs(self.panes.get(index)?.thread()?, false)?;
        let tasks: Vec<BackgroundTask> =
            inputs.line_work().into_iter().map(|work| work.task).collect();
        pane::present_background_work(&tasks, !inputs.busy).map(|presented| presented.title)
    }

    /// Why the line says the stop of task `id` of Pane `index` failed.
    #[cfg(test)]
    pub(super) fn background_stop_reason(&self, index: usize, id: &str, cx: &App) -> Option<String> {
        let inputs = self.background_inputs(self.panes.get(index)?.thread()?, false)?;
        match self.background.read(cx).stop_state(&inputs, &StopTarget::Command(id.into()))? {
            StopState::Refused(why) => Some(why),
            _ => None,
        }
    }

    /// The line under Pane `index`'s transcript, if it is a Main's.
    pub(super) fn background_work_tail(&self, index: usize, cx: &App) -> Option<TranscriptTail> {
        let pane = self.panes.get(index).filter(|pane| pane.is_main())?;
        let inputs = self.background_inputs(pane.thread()?, false)?;
        self.background.read(cx).line(self.background.downgrade(), &inputs)
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
            .and_then(|thread| self.background_inputs(thread, false))
            .and_then(|inputs| lone_launch(&inputs.line_work()))
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

    pub(super) fn on_background_event(
        &mut self,
        _: Entity<BackgroundView>,
        event: &BackgroundEvent,
        cx: &mut Context<Self>,
    ) {
        match event.clone() {
            BackgroundEvent::Details(thread) => self.open_background_dialog(thread, cx),
            BackgroundEvent::Open(thread) => self.open_background(thread, cx),
            BackgroundEvent::OpenAgent { thread, call } => self.open_background_agent(thread, &call, cx),
            BackgroundEvent::StopLine(thread) => {
                self.stop_background_work(thread, BackgroundInputs::line_work, cx)
            }
            BackgroundEvent::Stop { thread, generation, target } => {
                self.send_stop(thread, generation, target, cx)
            }
        }
    }

    /// `Stop` on the line or in the dialog: the work they list that this
    /// Session can stop.
    pub(super) fn stop_background_work(
        &mut self,
        thread: ThreadId,
        work_of: fn(&BackgroundInputs) -> Vec<Work>,
        cx: &mut Context<Self>,
    ) {
        let Some(inputs) = self.background_inputs(thread, false) else {
            return;
        };
        for target in inputs.targets(&work_of(&inputs)) {
            self.send_stop(thread, inputs.generation, target, cx);
        }
        cx.notify();
    }

    /// Stop `target` of `thread`'s Session `generation`, if that very
    /// Session still runs it: a press that lands as the work is handed back
    /// stops nothing else.
    fn send_stop(&mut self, thread: ThreadId, generation: u64, target: StopTarget, cx: &mut Context<Self>) {
        let Some(inputs) = self
            .background_inputs(thread, false)
            .filter(|inputs| inputs.generation == generation)
        else {
            return;
        };
        if !inputs.targets(&inputs.running).contains(&target) {
            return;
        }
        let stop = (thread, generation, target.clone());
        let refused = inputs.refusal(&target).map_or(0, |(count, _)| count);
        self.background
            .update(cx, |background, cx| background.sent(stop.clone(), refused, cx));
        match target {
            StopTarget::Command(id) => {
                self.run_session_control(
                    thread,
                    generation,
                    ferrite_core::SessionControl::StopTask { id },
                );
                if let Some(why) = self.session_control_error.of(thread, generation) {
                    let why = why.to_string();
                    self.background
                        .update(cx, |background, cx| background.refused(stop, why, cx));
                }
            }
            StopTarget::Agent(key) => {
                self.stop_subagent(thread, &key);
            }
        }
        cx.notify();
    }

    /// Forget the stops of work that has ended, or of a Session that has.
    pub(super) fn prune_background_stops(&mut self, cx: &mut Context<Self>) {
        self.background.update(cx, |background, _| {
            background.prune(|thread| {
                let inputs = self.background_inputs(thread, false)?;
                Some((inputs.generation, inputs.targets(&inputs.running)))
            })
        });
    }

    /// `Open subagent` in the dialog, or `Open` on an agent's card in the
    /// Background Surface: the agent the call started, its full chat in the
    /// Pane.
    pub(super) fn open_background_agent(&mut self, thread: ThreadId, call: &str, cx: &mut Context<Self>) {
        if self.background_dialog(cx).is_some() {
            self.hide_sheet(cx);
        }
        if let Some(subject) = self.agent_of_call(thread, call) {
            self.open_subagent(thread, subject, cx);
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
