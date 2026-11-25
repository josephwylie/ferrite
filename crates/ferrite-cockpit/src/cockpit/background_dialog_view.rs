//! The `Background work` dialog (T3's `BackgroundWorkDetails`) as a view of
//! its own (ADR 0041), made when `Details` opens it and dropped once it has
//! gone (ADR 0042). It owns only which Thread it shows and its focus. It
//! draws that Thread's running work from what the Cockpit answers for it
//! (`BackgroundDialogInputs`), read each frame through a `Source` the root
//! makes over itself, so the Cockpit's own notify still redraws it; and what
//! the operator asks of it it says as a `BackgroundDialogEvent`. Its `Stop`
//! stands where `BackgroundView` (`background_work.rs`) says, shared with
//! the line under the transcript.

use super::background_work::{stop_elements, BackgroundInputs, StopControl, Work};
use super::panel::background::output_block;
use super::*;
use ferrite_core::transcript::ToolBlock;
use ferrite_pane::pane::WorkKind;
use ferrite_ui::source::Source;
use gpui::App;

pub(crate) struct BackgroundDialogView {
    /// What it draws of `thread`, as the window has it.
    inputs: Source<BackgroundDialogInputs, ThreadKey>,
    pub(super) thread: ThreadKey,
    pub(super) focus: FocusHandle,
}

/// What the dialog draws from, as the Cockpit has it this frame.
pub(super) struct BackgroundDialogInputs {
    /// Main's turn has ended: the work is waited on, not running beside it.
    waiting: bool,
    /// Each piece of work, with the call that launched it where the
    /// transcript holds one.
    work: Vec<(Work, Option<ToolBlock>)>,
    stop: Option<StopControl>,
}

/// What the dialog asks of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    pub(super) fn new(
        inputs: Source<BackgroundDialogInputs, ThreadKey>,
        thread: ThreadKey,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            inputs,
            thread,
            focus: cx.focus_handle(),
        }
    }

    /// One section per piece of work, 20px apart, for the sheet host.
    pub(super) fn card(&self, window: &Window, cx: &mut Context<Self>) -> Option<ferrite_ui::sheet::Drawn> {
        use ferrite_ui::theme::*;
        let thread = self.thread;
        let BackgroundDialogInputs { waiting, work, stop } = self.inputs.read(&thread, cx)?;
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
        let inputs = Source::of(&cx.entity(), |root: &Self, thread: &ThreadKey, cx| {
            root.background_dialog_inputs(*thread, cx)
        });
        let view = cx.new(|cx| BackgroundDialogView::new(inputs, thread, cx));
        cx.subscribe(&view, Self::on_background_dialog_event).detach();
        self.show_sheet(sheets::Sheet::BackgroundDialog(view), cx);
    }
}

/// The dialog in a window of its own, with no Cockpit: its card drawn as
/// the sheet host draws it, from what a test keeps of the Thread's work.
#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::super::background_work::{StopControl, Work};
    use super::*;
    use ferrite_core::transcript::{ToolOutput, ToolState};
    use ferrite_pane::pane::BackgroundTask;
    use ferrite_ui::testing::{click, mount, record, shows};
    use gpui::{TestAppContext, VisualTestContext};

    /// The sheet host's part in it: the card, where the host lays it.
    struct Host(Entity<BackgroundDialogView>);

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let drawn = self.0.update(cx, |dialog, cx| dialog.card(window, cx));
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .children(drawn.map(|drawn| drawn.card))
        }
    }

    /// What the window keeps of the Thread's background work, as a test
    /// keeps it. `open` is false once the Thread is not.
    struct Kept {
        open: bool,
        waiting: bool,
        work: Vec<(Work, Option<ToolBlock>)>,
        stop: Option<StopControl>,
        /// Each Thread the dialog asked the window about.
        asked: Rc<RefCell<Vec<ThreadKey>>>,
    }

    fn work(kind: WorkKind, label: &str, call: &str) -> Work {
        Work {
            task: BackgroundTask {
                kind,
                label: label.into(),
                launched: None,
                activity: None,
            },
            stop: None,
            call: Some(call.into()),
            holds: true,
        }
    }

    /// The call that launched a command, and its output so far.
    fn launched(call: &str, command: &str) -> ToolBlock {
        ToolBlock {
            call: call.into(),
            name: "Bash".into(),
            title: None,
            summary: command.into(),
            state: ToolState::Running,
            diffs: Vec::new(),
            structured_preview: None,
            result_line: None,
            output: Some(ToolOutput {
                text: "running 3 tests".into(),
                omitted_bytes: 0,
            }),
            progress: None,
            caption: None,
            background: None,
            image: false,
            findings: Vec::new(),
        }
    }

    /// A command and a subagent running, with a plain `Stop`.
    fn kept() -> Kept {
        Kept {
            open: true,
            waiting: false,
            work: vec![
                (
                    work(WorkKind::Command, "cargo test", "toolu_c"),
                    Some(launched("toolu_c", "cargo test --workspace")),
                ),
                (work(WorkKind::Subagent, "Read the docs", "toolu_a"), None),
            ],
            stop: Some(stop_control(false, None, None)),
            asked: Rc::default(),
        }
    }

    fn stop_control(stopping: bool, refused: Option<&str>, leaves: Option<&str>) -> StopControl {
        StopControl {
            leaves: leaves.map(str::to_string),
            stopping,
            refused: refused.map(str::to_string),
        }
    }

    fn thread() -> ThreadKey {
        ThreadKey::fixture(1, 3)
    }

    fn open(
        kept: Kept,
        cx: &mut TestAppContext,
    ) -> (Entity<BackgroundDialogView>, Entity<Kept>, &mut VisualTestContext) {
        let kept = cx.new(|_| kept);
        let inputs = Source::of(&kept, |kept: &Kept, thread: &ThreadKey, _| {
            kept.asked.borrow_mut().push(*thread);
            kept.open.then(|| BackgroundDialogInputs {
                waiting: kept.waiting,
                work: kept.work.clone(),
                stop: kept.stop.clone(),
            })
        });
        let (host, cx) = mount(cx, |_, cx| {
            Host(cx.new(|cx| BackgroundDialogView::new(inputs, thread(), cx)))
        });
        let dialog = host.read_with(cx, |host, _| host.0.clone());
        (dialog, kept, cx)
    }

    fn stand(kept: &Entity<Kept>, stop: StopControl, cx: &mut VisualTestContext) {
        kept.update(cx, |kept, cx| {
            kept.stop = Some(stop);
            cx.notify();
        });
        cx.run_until_parked();
    }

    /// Each piece of work is a section; an agent's has `Open subagent`, a
    /// command's does not. It asks the window about its own Thread.
    #[gpui::test]
    fn each_piece_of_work_is_a_section(cx: &mut TestAppContext) {
        let (_dialog, kept, cx) = open(kept(), cx);
        assert!(shows(cx, "background-dialog"));
        assert!(shows(cx, "background-dialog-section"));
        assert!(shows(cx, "background-dialog-open-toolu_a"), "the agent opens");
        assert!(!shows(cx, "background-dialog-open-toolu_c"), "a command does not");
        assert!(!shows(cx, "background-dialog-empty"));
        let asked = kept.read_with(cx, |kept, _| kept.asked.borrow().clone());
        assert!(!asked.is_empty());
        assert!(asked.iter().all(|asked| *asked == thread()));
    }

    /// With no work left it says so, and work that comes back is listed
    /// again on the window's notify.
    #[gpui::test]
    fn with_no_work_left_it_says_so(cx: &mut TestAppContext) {
        let (_dialog, kept, cx) = open(Kept { work: Vec::new(), ..kept() }, cx);
        assert!(shows(cx, "background-dialog"), "the dialog stays up");
        assert!(shows(cx, "background-dialog-empty"));
        assert!(!shows(cx, "background-dialog-section"));
        kept.update(cx, |kept, cx| {
            kept.work = self::kept().work;
            cx.notify();
        });
        cx.run_until_parked();
        assert!(!shows(cx, "background-dialog-empty"));
        assert!(shows(cx, "background-dialog-section"));
    }

    /// `Stop` asks the window to stop the work; while it is stopping the
    /// button takes no press, and a refusal says why and offers `Retry`.
    #[gpui::test]
    fn stop_asks_the_window_and_says_where_it_stands(cx: &mut TestAppContext) {
        let (dialog, kept, cx) = open(kept(), cx);
        let heard = record(cx, &dialog, BackgroundDialogEvent::clone);
        assert!(shows(cx, "background-dialog-stop-says-Stop"));
        click(cx, "background-dialog-stop-all");
        assert_eq!(heard.take(), [BackgroundDialogEvent::Stop]);

        stand(&kept, stop_control(true, None, None), cx);
        assert!(shows(cx, "background-dialog-stopping"));
        assert!(!shows(cx, "background-dialog-stop-all"), "no press while it stops");

        stand(&kept, stop_control(false, Some("no such task"), Some("1 agent left running")), cx);
        assert!(shows(cx, "background-dialog-stop-refused"));
        assert!(shows(cx, "background-dialog-stop-says-Retry"));
        assert!(shows(cx, "background-dialog-stop-remains-1 agent left running"));
        click(cx, "background-dialog-stop-all");
        assert_eq!(heard.take(), [BackgroundDialogEvent::Stop], "Retry asks again");
    }

    /// `Open subagent` names the call that launched the agent; the close
    /// button closes.
    #[gpui::test]
    fn open_subagent_and_close_say_what_they_ask(cx: &mut TestAppContext) {
        let (dialog, _kept, cx) = open(kept(), cx);
        let heard = record(cx, &dialog, BackgroundDialogEvent::clone);
        click(cx, "background-dialog-open-toolu_a");
        click(cx, "background-dialog-close");
        assert_eq!(
            heard.take(),
            [BackgroundDialogEvent::OpenThread("toolu_a".into()), BackgroundDialogEvent::Closed]
        );
    }

    /// With nothing the Session can stop, there is no `Stop`.
    #[gpui::test]
    fn with_nothing_it_can_stop_there_is_no_stop(cx: &mut TestAppContext) {
        let (_dialog, _kept, cx) = open(Kept { stop: None, ..kept() }, cx);
        assert!(shows(cx, "background-dialog-section"));
        assert!(!shows(cx, "background-dialog-stop-all"));
    }

    /// Once its Thread is not open, it draws no card.
    #[gpui::test]
    fn with_its_thread_gone_it_draws_no_card(cx: &mut TestAppContext) {
        let (_dialog, kept, cx) = open(kept(), cx);
        kept.update(cx, |kept, cx| {
            kept.open = false;
            cx.notify();
        });
        cx.run_until_parked();
        assert!(!shows(cx, "background-dialog"));
    }
}
