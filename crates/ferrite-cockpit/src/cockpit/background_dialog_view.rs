//! The `Background work` dialog (T3's `BackgroundWorkDetails`) as a view of
//! its own (ADR 0041), made when `Details` opens it and dropped once it has
//! gone (ADR 0042). It owns only which Thread it shows and its focus. It
//! draws that Thread's running work from what the Cockpit gives it
//! (`BackgroundDialogInputs`), asked for each frame, so the Cockpit's own
//! notify still redraws it; and what the operator asks of it it says as a
//! `BackgroundDialogEvent`. Its `Stop` stands where `BackgroundView`
//! (`background_work.rs`) says, shared with the line under the transcript.

use super::background_work::{stop_elements, BackgroundInputs, StopControl, Work};
use super::panel::background::output_block;
use super::*;
use ferrite_core::transcript::ToolBlock;
use ferrite_pane::pane::WorkKind;
use gpui::{App, WeakEntity};

pub(crate) struct BackgroundDialogView {
    cockpit: WeakEntity<CockpitView>,
    pub(super) thread: ThreadKey,
    pub(super) focus: FocusHandle,
}

/// What the dialog draws from, as the Cockpit has it this frame.
struct BackgroundDialogInputs {
    /// Main's turn has ended: the work is waited on, not running beside it.
    waiting: bool,
    /// Each piece of work, with the call that launched it where the
    /// transcript holds one.
    work: Vec<(Work, Option<ToolBlock>)>,
    stop: Option<StopControl>,
}

/// What the dialog asks of the window.
#[derive(Clone)]
pub(crate) enum BackgroundDialogEvent {
    /// `Stop`, or `Retry` after a refusal: every piece of work listed that
    /// the Session can stop.
    Stop,
    /// `Open subagent` on an agent, by the call that launched it.
    OpenThread(String),
    /// The close button closed it.
    Closed,
}

impl EventEmitter<BackgroundDialogEvent> for BackgroundDialogView {}

impl BackgroundDialogView {
    pub(super) fn new(cockpit: WeakEntity<CockpitView>, thread: ThreadKey, cx: &mut Context<Self>) -> Self {
        Self {
            cockpit,
            thread,
            focus: cx.focus_handle(),
        }
    }

    /// One section per piece of work, 20px apart, for the sheet host.
    pub(super) fn card(&self, window: &Window, cx: &mut Context<Self>) -> Option<ferrite_ui::sheet::Drawn> {
        use ferrite_ui::theme::*;
        let thread = self.thread;
        let BackgroundDialogInputs { waiting, work, stop } = self
            .cockpit
            .read_with(cx, |cockpit, cx| cockpit.background_dialog_inputs(thread, cx))
            .ok()??;
        let mut sections = div().flex().flex_col().gap(px(BACKGROUND_DIALOG_SECTION_GAP));
        for (work, call) in &work {
            sections = sections.child(Self::work_section(work, call.as_ref(), waiting && work.holds, cx));
        }
        if work.is_empty() {
            sections = sections.child(
                div()
                    .debug_selector(|| "background-dialog-empty".into())
                    .text_size(px(FS_BADGE))
                    .text_color(rgb(INK_SUBTLE))
                    .child("No background work"),
            );
        }
        let stop = stop.map(|control| {
            div()
                .flex()
                .flex_wrap()
                .items_baseline()
                .justify_end()
                .gap(px(BACKGROUND_LINE_ACTIONS_GAP))
                .text_size(px(FS_LABEL))
                .children(stop_elements(
                    &control,
                    thread,
                    "background-dialog",
                    cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(BackgroundDialogEvent::Stop)),
                ))
        });
        let close = ferrite_ui::sheet::sheet_close("background-dialog-close", "Close", cx)
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(BackgroundDialogEvent::Closed)));
        let card = ferrite_ui::sheet::sheet_fit(
            BACKGROUND_DIALOG_W,
            BACKGROUND_DIALOG_MAX_H.min(f32::from(window.viewport_size().height) * MODAL_VIEWPORT_FRACTION),
        )
        .id("background-dialog")
        .debug_selector(|| "background-dialog".into())
        .track_focus(&self.focus)
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
                .child(sections)
                .children(stop.map(|stop| stop.mt(px(BACKGROUND_DIALOG_SECTION_GAP)))),
        );
        Some(ferrite_ui::sheet::Drawn { card, above: None })
    }

    /// One piece of work: its title and standing, the command in a box, its
    /// output so far, and `Open subagent` for an agent.
    fn work_section(work: &Work, call: Option<&ToolBlock>, waiting: bool, cx: &mut Context<Self>) -> Div {
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
                "Open subagent",
            )
            .debug_selector({
                let target = target.clone();
                move || format!("background-dialog-open-{target}")
            })
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.emit(BackgroundDialogEvent::OpenThread(target.clone()))
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

impl CockpitView {
    /// Everything the dialog on `thread` draws from, none once that Thread
    /// is not open. It never reads the dialog: the dialog calls it while its
    /// own update is running.
    fn background_dialog_inputs(&self, thread: ThreadKey, cx: &App) -> Option<BackgroundDialogInputs> {
        let inputs = self.background_inputs(thread, true, cx)?;
        let stop = self.background.read(cx).stop_control(&inputs, &inputs.running);
        let BackgroundInputs { busy, running, calls, .. } = inputs;
        let work = running
            .into_iter()
            .map(|work| {
                let call = work.call.as_ref().and_then(|call| calls.get(call)).cloned();
                (work, call)
            })
            .collect();
        Some(BackgroundDialogInputs {
            waiting: !busy,
            work,
            stop,
        })
    }

    pub(super) fn on_background_dialog_event(
        &mut self,
        dialog: Entity<BackgroundDialogView>,
        event: &BackgroundDialogEvent,
        cx: &mut Context<Self>,
    ) {
        let thread = dialog.read(cx).thread;
        match event.clone() {
            BackgroundDialogEvent::Stop => {
                self.stop_background_work(thread, |inputs| inputs.running.clone(), cx)
            }
            BackgroundDialogEvent::OpenThread(call) => self.open_background_agent(thread, &call, cx),
            BackgroundDialogEvent::Closed => {
                self.hide_sheet(cx);
            }
        }
    }

    /// The `Background work` dialog, while it is up.
    pub(crate) fn background_dialog(&self, cx: &App) -> Option<Entity<BackgroundDialogView>> {
        self.sheets.read(cx).background_dialog().cloned()
    }

    /// `Details` on the line: `thread`'s dialog, in place of any other sheet.
    pub(super) fn open_background_dialog(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let cockpit = cx.weak_entity();
        let view = cx.new(|cx| BackgroundDialogView::new(cockpit, thread, cx));
        cx.subscribe(&view, Self::on_background_dialog_event).detach();
        self.show_sheet(sheets::Sheet::BackgroundDialog(view), cx);
    }
}
