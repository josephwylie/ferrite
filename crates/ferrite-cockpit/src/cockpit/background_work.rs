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

use ferrite_core::transcript::ToolBlock;
use ferrite_pane::pane::{BackgroundTask, WorkKind};
use ferrite_pane::transcript::TranscriptTail;

/// One piece of work still running. A command is the provider's task
/// (`id` names it for a stop, `call` the call that launched it, which holds
/// its output); a subagent is the agent a call started, still working
/// after Main's turn.
struct Work {
    task: BackgroundTask,
    id: Option<String>,
    call: Option<String>,
}

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
                },
                id: Some(task.id.clone()),
                call: call.map(|call| call.call.clone()),
            }
        })
        .collect();
    // While Main's turn runs its agents are rows of the transcript; once it
    // has ended, the ones still working are what the Thread waits on.
    if !open.busy() {
        for (call, agent) in open.activity().invocations() {
            let working = matches!(
                agent.status(),
                AgentStatus::Working
                    | AgentStatus::Pending
                    | AgentStatus::Waiting
                    | AgentStatus::Unknown
            ) && agent.fresh();
            if !working {
                continue;
            }
            work.push(Work {
                task: BackgroundTask {
                    kind: WorkKind::Subagent,
                    label: transcript
                        .tool(&call)
                        .map(|tool| label(tool).to_string())
                        .unwrap_or_default(),
                    launched: agent.transcript().turn_started(),
                },
                id: None,
                call: Some(call),
            });
        }
    }
    work
}

impl CockpitView {
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
        let stoppable = open.supports_control(ferrite_core::ControlKind::StopTask)
            && work.iter().any(|work| work.id.is_some());
        let stopping = work
            .iter()
            .filter_map(|work| work.id.as_ref())
            .all(|id| self.background_stopping.contains(id));
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
                stoppable,
                stopping,
                work.iter().map(|work| (&work.id, &work.call)).collect::<Vec<_>>(),
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
                    let stop = pane::inline_button(
                        ("background-stop-all", thread.get() as usize),
                        if stopping { "Stopping..." } else { "Stop" },
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

    /// `Stop` on the line: every piece of work this Session still runs.
    fn stop_background_work(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(open) = self.cockpit.thread(thread) else {
            return;
        };
        let generation = open.generation();
        let ids: Vec<String> = running_work(&open)
            .into_iter()
            .filter_map(|work| work.id)
            .collect();
        for id in ids {
            self.background_stopping.insert(id.clone());
            self.run_session_control(
                thread,
                generation,
                ferrite_core::SessionControl::StopTask { id },
            );
        }
        cx.notify();
    }

    /// `Open` on the line, or `Open thread` in the dialog: the agent the
    /// call started, in the Panel.
    fn open_background_agent(&mut self, thread: ThreadId, call: &str, cx: &mut Context<Self>) {
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
