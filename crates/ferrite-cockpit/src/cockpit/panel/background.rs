//! Background work (Claude's, `claude/parity/bg-tasks-panel.png`): the
//! calls a Thread launched into the background (`ToolBlock::background`,
//! HD64), read off its transcript's rows, the one source of truth.
//!
//! - **The rail's section**, after Changes: each piece of work still
//!   running, its title, its clock from the launch and a stop square where
//!   the Session can stop it, then `N finished`. A row opens the pane.
//! - **The Surface** (`bg-pane-running.png`, `bg-card-*.png`): a tab of the
//!   Thread's Panel (ADR 0039), `Background`, its tab's × closing it.
//!   `Running N`, a card per piece of work: its `▾`, title, clock and
//!   stop, open on its output in mono (`$ command`, the lines so far, or
//!   `No output yet`); then `Finished N`, a card each, closed, with how
//!   long it took, opening on its output and `[exited with code 0]`.
//!
//! There is no shelf over the Composer: running work lives here, in the
//! line under the transcript, and a command's own row (`pane::background_link`,
//! `View live output ▸`). A subagent's row has no such link: it opens the
//! agent, as T3 Code's does.

use super::*;
use crate::cockpit::background_work::{BackgroundEvent, BackgroundInputs, BackgroundView, StopTarget, Work};
use crate::cockpit::surfaces::SurfaceId;
use ferrite_core::ThreadKey;
use ferrite_pane::pane::WorkKind;
use ferrite_core::transcript::{ToolBlock, ToolState};
use ferrite_core::transcript::tools::{policy, ToolKind};

/// How many of a running command's last output lines its card shows.
const TAIL_LINES: usize = 12;

impl CockpitView {
    /// Open `thread`'s Background Surface.
    pub(crate) fn open_background(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.upsert_surface(self.cockpit.read(cx).thread_key(thread), SurfaceId::Background, cx);
    }
}

impl BackgroundView {
    /// The Overview's `Background tasks` section; none until the Thread has
    /// launched background work.
    pub(in crate::cockpit) fn section(&self, inputs: &BackgroundInputs, cx: &mut Context<Self>) -> Option<Div> {
        let thread = inputs.thread;
        let finished = inputs.finished.len();
        if inputs.running.is_empty() && finished == 0 {
            return None;
        }
        let mut section = div()
            .flex()
            .flex_col()
            .child(panel_section_head("panel-background-head", "Background tasks"));
        for (index, work) in inputs.running.iter().enumerate() {
            let launched = work.task.launched;
            let row = sections::detail_row(
                SharedString::from(format!("panel-background-{index}")),
                glyph(work),
                work.task.label.clone(),
                None,
            )
            .cursor_pointer()
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.emit(BackgroundEvent::Open(thread));
            }))
            .children(launched.map(|launched| {
                div()
                    .flex_shrink_0()
                    .ml_auto()
                    .text_size(px(ferrite_ui::theme::FS_META))
                    .child(ferrite_pane::pane::background_clock(launched))
            }))
            .children(stop_square(inputs, work, cx));
            section = section.child(row);
        }
        if finished > 0 {
            section = section.child(
                sections::detail_row(
                    "panel-background-finished",
                    icons::SUBAGENTS,
                    format!("{finished} finished"),
                    None,
                )
                .cursor_pointer()
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.emit(BackgroundEvent::Open(thread));
                })),
            );
        }
        Some(section)
    }

    /// The Background Surface's body: Running and Finished.
    pub(in crate::cockpit) fn pane(&self, inputs: &BackgroundInputs, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let mut body = div()
            .debug_selector(|| "background-pane".into())
            .flex()
            .flex_col()
            .gap(px(SPACE_2));
        if !inputs.running.is_empty() {
            body = body.child(group_label(format!("Running {}", inputs.running.len())));
            for (index, work) in inputs.running.iter().enumerate() {
                let call = work.call.as_ref().and_then(|call| inputs.calls.get(call));
                let tail = div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .children(work.task.launched.map(|launched| {
                        ferrite_pane::pane::background_clock(launched).text_size(px(FS_META))
                    }))
                    .children(open_agent_button(inputs.thread, work, cx))
                    .children(stop_square(inputs, work, cx));
                let id = SharedString::from(format!("background-running-{index}"));
                let output = call.map(|call| output_block(call, false, true));
                body = body.child(self.card(
                    inputs.thread,
                    id,
                    work.handle(),
                    work_title(work),
                    output,
                    true,
                    tail,
                    cx,
                ));
            }
        }
        if !inputs.finished.is_empty() {
            body = body.child(group_label(format!("Finished {}", inputs.finished.len())));
            for (index, finished) in inputs.finished.iter().rev().enumerate() {
                let tail = div()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .children(finished.took.clone());
                let id = SharedString::from(format!("background-finished-{index}"));
                let output = inputs
                    .calls
                    .get(&finished.call)
                    .map(|call| output_block(call, true, true));
                body = body.child(self.card(
                    inputs.thread,
                    id,
                    finished.call.clone(),
                    finished.title.clone(),
                    output,
                    false,
                    tail,
                    cx,
                ));
            }
        }
        body
    }

    /// One piece of work's card: its disclosure, title and `tail` (clock and
    /// stop, or how long it took), open on its output. Running work opens
    /// by default, finished work is closed; a press on the head turns it.
    #[allow(clippy::too_many_arguments)]
    fn card(
        &self,
        thread: ThreadKey,
        id: SharedString,
        key: String,
        title: SharedString,
        output: Option<Div>,
        running: bool,
        tail: Div,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        use ferrite_ui::theme::*;
        let open = running != self.is_turned(thread, &key);
        let target = key;
        let head_id = format!("{id}-head");
        let head = div()
            .id(SharedString::from(head_id.clone()))
            .debug_selector(move || head_id.clone())
            .flex()
            .items_center()
            .gap(px(SPACE_2))
            .h(px(PANEL_SECTION_H))
            .cursor_pointer()
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.turn(thread, target.clone(), cx);
            }))
            .child(ferrite_ui::components::chevron_at(
                if open { 1. } else { 0. },
                ICON_CHEVRON,
                INK_SUBTLE,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(INK_STRONG))
                    .child(title),
            )
            .child(tail);
        div()
            .id(id.clone())
            .debug_selector(move || id.to_string())
            .flex()
            .flex_col()
            .px(px(SPACE_3))
            .when(open, |card| card.pb(px(SPACE_2)))
            .rounded(px(R_CARD))
            .bg(paint::RAISED)
            .child(head)
            .children(output.filter(|_| open).map(|output| output.pl(px(SPACE_4))))
    }
}

/// `Open` on an agent's card: the agent in the Panel.
fn open_agent_button(
    thread: ThreadKey,
    work: &Work,
    cx: &mut Context<BackgroundView>,
) -> Option<AnyElement> {
    let call = work.call.clone().filter(|_| work.task.kind == WorkKind::Subagent)?;
    Some(
        ferrite_pane::pane::inline_button(
            SharedString::from(format!("background-tab-open-{call}")),
            "Open",
        )
        .debug_selector({
            let call = call.clone();
            move || format!("background-tab-open-{call}")
        })
        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            cx.emit(BackgroundEvent::OpenAgent { thread, call: call.clone() });
        }))
        .into_any_element(),
    )
}

/// The white square that stops a piece of work, where the Session can.
fn stop_square(
    inputs: &BackgroundInputs,
    work: &Work,
    cx: &mut Context<BackgroundView>,
) -> Option<AnyElement> {
    let target: StopTarget = inputs.targets(std::slice::from_ref(work)).pop()?;
    let (thread, generation) = (inputs.thread, inputs.generation);
    Some(
        ferrite_ui::components::icon_button(
            SharedString::from(format!("background-stop-{}", work.handle())),
            icons::STOP,
            ferrite_ui::components::IconSize::Standard,
            "Stop",
            cx,
        )
        .tip("Stop")
        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            cx.emit(BackgroundEvent::Stop { thread, generation, target: target.clone() });
        }))
        .into_any_element(),
    )
}

fn work_title(work: &Work) -> SharedString {
    let label = work.task.label.trim();
    if !label.is_empty() {
        label.to_owned().into()
    } else if work.task.kind == WorkKind::Subagent {
        "Subagent".into()
    } else {
        "Command".into()
    }
}

/// What a piece of work is called: its provider-authored title, else what
/// the call ran.
pub(in crate::cockpit) fn label(call: &ToolBlock) -> SharedString {
    call.title.clone().unwrap_or_else(|| call.summary.clone()).into()
}

fn glyph(work: &Work) -> &'static str {
    if work.task.kind == WorkKind::Subagent {
        icons::SUBAGENTS
    } else {
        icons::TERMINAL
    }
}

fn group_label(words: String) -> Div {
    use ferrite_ui::theme::*;
    div()
        .flex_shrink_0()
        .h(px(PANEL_SECTION_H))
        .flex()
        .items_center()
        .text_size(px(FS_META))
        .text_color(rgb(INK_SUBTLE))
        .child(words)
}

/// A command's output in mono, as Claude's pane has it: `$ command`, its
/// last lines (`No output yet` while it has printed nothing), and once it
/// ended, how. An agent's block is its report.
pub(in crate::cockpit) fn output_block(call: &ToolBlock, settled: bool, command: bool) -> Div {
    use ferrite_ui::theme::*;
    let agent = policy(&call.name).kind == ToolKind::Agent;
    let mut lines: Vec<SharedString> = Vec::new();
    if command && !agent && !call.summary.is_empty() {
        lines.push(format!("$ {}", call.summary).into());
    }
    let text = call.output.as_ref().map(|output| output.text.as_str()).unwrap_or("");
    let shown: Vec<&str> = text.lines().collect();
    let tail = &shown[shown.len().saturating_sub(TAIL_LINES)..];
    if tail.is_empty() && !settled {
        lines.push("No output yet".into());
    }
    lines.extend(tail.iter().map(|line| SharedString::from(line.to_string())));
    if settled && !agent {
        lines.push(match &call.state {
            ToolState::Failed(reason) => reason.clone().into(),
            _ => "[exited with code 0]".into(),
        });
    }
    div()
        .debug_selector(|| "background-output".into())
        .flex()
        .flex_col()
        .min_w_0()
        .font_family(FONT_CODE)
        .text_size(px(FS_CHROME_CODE))
        .text_color(rgb(INK_SUBTLE))
        .children(
            lines
                .into_iter()
                .map(|line| div().min_w_0().truncate().child(line)),
        )
}
