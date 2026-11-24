//! The two cards that drop from a Thread's Pane head as popups (ADR 0043):
//! the plan card and the CI checks card. They get no view (ADR 0041, What
//! does not get a view): what they show is the Thread's transcript and its
//! pull request's checks, which the root reads; the slot opens and closes
//! them and the root hangs them under the head. Here they are drawn from
//! what was read.

use super::*;
use ferrite_core::pull_request::PullRequest;
use ferrite_ui::theme::*;

/// The plan card as it reads: the done and total counts and each step.
pub(super) struct PlanCard {
    done: usize,
    total: usize,
    steps: Vec<PlanStep>,
}

struct PlanStep {
    text: SharedString,
    mark: StepMark,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StepMark {
    Done,
    /// The step being worked: the first open step the agent names as its
    /// current task.
    Working,
    Open,
}

impl PlanCard {
    /// The plan `transcript` holds, if the agent wrote one.
    pub(super) fn of(transcript: &ferrite_core::transcript::Transcript) -> Option<Self> {
        let todos = transcript.todos()?;
        let current = transcript.current_task();
        let mut marked = false;
        let steps = transcript
            .progress()
            .plan
            .iter()
            .map(|step| {
                let done = step.status == ferrite_core::progress::StepStatus::Completed;
                let working = !done && !marked && current == Some(step.text.as_str());
                marked |= working;
                PlanStep {
                    text: SharedString::from(step.text.clone()),
                    mark: if done {
                        StepMark::Done
                    } else if working {
                        StepMark::Working
                    } else {
                        StepMark::Open
                    },
                }
            })
            .collect();
        Some(Self {
            done: todos.done,
            total: todos.total,
            steps,
        })
    }

    /// The float grammar's head (`plan · 3/4`), then one row per step —
    /// done `INK_SUBTLE` behind a dim check, the step being worked
    /// `INK_STRONG` behind the strong `❯`, the rest `INK_STRONG` behind a
    /// faint ring.
    pub(super) fn render(self) -> Stateful<Div> {
        let mut card = ferrite_ui::menu::float()
            .min_w(px(CHECKS_CARD_W))
            .max_w(px(CHECKS_CARD_MAX_W))
            .pb(px(FLOAT_PAD))
            .child(
                ferrite_ui::menu::head("Plan")
                    .child(SharedString::from(format!("{}/{}", self.done, self.total))),
            );
        for (index, step) in self.steps.into_iter().enumerate() {
            let mark = match step.mark {
                StepMark::Done => {
                    ferrite_ui::components::glyph("\u{2713}", INK_SUBTLE).into_any_element()
                }
                StepMark::Working => ferrite_ui::components::prompt_mark(INK_STRONG),
                StepMark::Open => div()
                    .text_color(rgb(TEXT_FAINT))
                    .child("\u{25cb}")
                    .into_any_element(),
            };
            let done = step.mark == StepMark::Done;
            card = card.child(
                div()
                    .debug_selector(move || format!("plan-step-{index}"))
                    .flex()
                    .items_center()
                    .h(px(FLOAT_ROW_H))
                    .px(px(FLOAT_PAD_X))
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .w(px(SPACE_4 + SPACE_1))
                            .child(mark),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(if done { INK_SUBTLE } else { INK_STRONG }))
                            .child(step.text),
                    ),
            );
        }
        card.id("plan-card").debug_selector(|| "plan-card".into())
    }
}

/// The checks card: every run behind the header's rollup, grouped by the
/// workflow that owns it, each row with a log opening it among the
/// Thread's other browser tabs.
pub(super) fn checks_card(
    thread: ThreadKey,
    pr: &PullRequest,
    cx: &mut Context<CockpitView>,
) -> Stateful<Div> {
    let mut card = pane::checks_card().child(pane::checks_head(pr));
    let mut workflow: Option<Option<&str>> = None;
    for (index, run) in pr.runs.iter().enumerate() {
        let group = run.workflow.as_deref();
        if workflow != Some(group) {
            card = card.child(pane::checks_group(group, workflow.is_none()));
            workflow = Some(group);
        }
        let row = pane::check_row(index, run);
        card = card.child(match run.url.clone() {
            // Keep the run's log with the Thread's other browser tabs.
            Some(url) => row.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.browser_action(thread, ferrite_core::browser::Action::Open { url: url.clone() }, cx);
                    view.close_popup(cx);
                }),
            ),
            None => row,
        });
    }
    card.id("context-checks-card")
        .debug_selector(|| "context-checks-card".into())
}
