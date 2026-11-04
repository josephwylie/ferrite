//! Background work (Claude's, `claude/parity/bg-tasks-panel.png`): the
//! calls a Thread launched into the background (`ToolBlock::background`,
//! HD64), read off its transcript's rows, the one source of truth.
//!
//! - **The rail's section**, after Changes: each piece of work still
//!   running, its title, its clock from the launch and a stop square where
//!   the Session can stop it, then `N finished`. A row opens the pane.
//! - **The pane** (`bg-pane-running.png`, `bg-card-*.png`): the column's
//!   other body (as the side chat is), its head `Background tasks` with
//!   close. `Running N`, a card per piece of work: its `▾`, title, clock and
//!   stop, open on its output in mono (`$ command`, the lines so far, or
//!   `No output yet`); then `Finished N`, a card each, closed, with how
//!   long it took, opening on its output and `[exited with code 0]`.
//!
//! There is no shelf over the Composer: running work lives here and in its
//! own row (`pane::background_link`, `View live output ▸`).

use super::*;
use crate::cockpit::view_head::Column;
use ferrite_core::transcript::{ToolBlock, ToolState};
use ferrite_core::transcript::tools::{policy, ToolKind};

/// How many of a running command's last output lines its card shows.
const TAIL_LINES: usize = 12;

impl CockpitView {
    /// The rail's `Background tasks` section; none until the Thread has
    /// launched background work.
    pub(super) fn background_section(&self, thread: ThreadId, cx: &mut Context<Self>) -> Option<Div> {
        let open = self.cockpit.thread(thread)?;
        let calls: Vec<&ToolBlock> = open.transcript().background_calls().collect();
        if calls.is_empty() {
            return None;
        }
        let finished = calls.iter().filter(|call| !call.in_background()).count();
        let mut section = div()
            .flex()
            .flex_col()
            .child(panel_section_head("panel-background-head", "Background tasks"));
        for (index, call) in calls.iter().filter(|call| call.in_background()).enumerate() {
            let launched = call.background.as_ref().map(|work| work.launched);
            let row = sections::detail_row(
                SharedString::from(format!("panel-background-{index}")),
                glyph(call),
                label(call),
                None,
            )
            .cursor_pointer()
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.open_background(thread, cx);
            }))
            .children(launched.map(|launched| {
                div()
                    .flex_shrink_0()
                    .ml_auto()
                    .text_size(px(ferrite_ui::theme::FS_META))
                    .child(ferrite_pane::pane::background_clock(launched))
            }))
            .children(self.stop_square(thread, call, cx));
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
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.open_background(thread, cx);
                })),
            );
        }
        Some(section)
    }

    /// The pane is open for `thread`.
    pub(super) fn background_shown(&self, thread: Option<ThreadId>) -> bool {
        thread.is_some() && self.panel.background == thread
    }

    /// Open the Background tasks pane over `thread`'s column.
    pub(crate) fn open_background(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.panel.background = Some(thread);
        cx.notify();
    }

    /// The pane's body: its head, then Running and Finished.
    pub(super) fn background_pane(&self, thread: ThreadId, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let head = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(WIN_CHROME_H))
            .pr(px(tools_inset() - PANEL_PAD_R))
            .mt(px(self.head_drop(Column::Rail)))
            .child(
                div()
                    .debug_selector(|| "background-pane-head".into())
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(INK_SUBTLE))
                    .child("Background tasks"),
            )
            .child(
                crate::nav::icon_button(
                    "background-pane-close",
                    icons::CLOSE,
                    NAV_TOGGLE_GLYPH,
                    ICON_BUTTON,
                    R_CONTROL,
                )
                .tooltip(ferrite_ui::menu::tooltip("Close"))
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.panel.background = None;
                    cx.notify();
                })),
            );
        let Some(open) = self.cockpit.thread(thread) else {
            return div().child(head);
        };
        let timings = open.activity().main().timings();
        let calls: Vec<&ToolBlock> = open.transcript().background_calls().collect();
        let (running, finished): (Vec<&ToolBlock>, Vec<&ToolBlock>) =
            calls.into_iter().partition(|call| call.in_background());
        let mut body = div()
            .debug_selector(|| "background-pane".into())
            .flex()
            .flex_col()
            .gap(px(SPACE_2));
        if !running.is_empty() {
            body = body.child(group_label(format!("Running {}", running.len())));
            for (index, call) in running.iter().enumerate() {
                let tail = div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .children(call.background.as_ref().map(|work| {
                        ferrite_pane::pane::background_clock(work.launched)
                            .text_size(px(FS_META))
                    }))
                    .children(self.stop_square(thread, call, cx));
                let id = SharedString::from(format!("background-running-{index}"));
                body = body.child(self.card(id, call, true, tail, cx));
            }
        }
        if !finished.is_empty() {
            body = body.child(group_label(format!("Finished {}", finished.len())));
            for (index, call) in finished.iter().rev().enumerate() {
                let took = timings
                    .get(&call.call)
                    .map(|timing| ferrite_core::progress::settled_duration_label(timing.elapsed()));
                let tail = div()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .children(took);
                let id = SharedString::from(format!("background-finished-{index}"));
                body = body.child(self.card(id, call, false, tail, cx));
            }
        }
        div().flex().flex_col().child(head).child(body)
    }

    /// One piece of work's card: its disclosure, title and `tail` (clock and
    /// stop, or how long it took), open on its output. Running work opens
    /// by default, finished work is closed; a press on the head turns it.
    fn card(
        &self,
        id: SharedString,
        call: &ToolBlock,
        running: bool,
        tail: Div,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        use ferrite_ui::theme::*;
        let open = running != self.panel.background_turned.contains(&call.call);
        let target = call.call.clone();
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
                let turned = &mut view.panel.background_turned;
                if !turned.remove(&target) {
                    turned.insert(target.clone());
                }
                cx.notify();
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
                    .child(label(call)),
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
            .when(open, |card| card.child(output_block(call, !running).pl(px(SPACE_4))))
    }

    /// The white square that stops a piece of work, where the Session can.
    fn stop_square(
        &self,
        thread: ThreadId,
        call: &ToolBlock,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.cockpit.thread(thread)?;
        if !open.supports_control(ferrite_core::ControlKind::StopTask) {
            return None;
        }
        let generation = open.generation();
        let task = call.background.as_ref()?.task.clone();
        let target = call.call.clone();
        Some(
            crate::nav::icon_button(
                SharedString::from(format!("background-stop-{target}")),
                icons::STOP,
                ferrite_ui::theme::PANEL_ROW_ICON,
                ferrite_ui::theme::ICON_BUTTON,
                ferrite_ui::theme::R_CONTROL,
            )
            .flex_shrink_0()
            .tooltip(ferrite_ui::menu::tooltip("Stop"))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                // Only work this very Session still runs: a square pressed
                // as its work is handed back stops nothing else.
                let running = view.cockpit.thread(thread).is_some_and(|open| {
                    open.generation() == generation
                        && open.transcript().running_in_background(&target)
                });
                if running {
                    view.run_session_control(
                        thread,
                        generation,
                        ferrite_core::SessionControl::StopTask { id: task.clone() },
                    );
                }
                cx.notify();
            }))
            .into_any_element(),
        )
    }
}

/// What a piece of work is called: its provider-authored title, else what
/// the call ran.
fn label(call: &ToolBlock) -> SharedString {
    call.title.clone().unwrap_or_else(|| call.summary.clone()).into()
}

fn glyph(call: &ToolBlock) -> &'static str {
    if policy(&call.name).kind == ToolKind::Agent {
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
fn output_block(call: &ToolBlock, settled: bool) -> Div {
    use ferrite_ui::theme::*;
    let agent = policy(&call.name).kind == ToolKind::Agent;
    let mut lines: Vec<SharedString> = Vec::new();
    if !agent && !call.summary.is_empty() {
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
