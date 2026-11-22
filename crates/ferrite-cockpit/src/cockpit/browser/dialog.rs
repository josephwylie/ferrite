//! A page's script dialog (ADR 0041): the card an alert, a confirm or a
//! prompt shows over its page, with the prompt's field and the card's
//! focus, by Thread. It has no render of its own: the root has it draw the
//! card over the page (`browser_body`) and hands it what the shown page
//! asks (`browser_dialog_inputs`). The answer it says as a
//! `BrowserDialogEvent`; the page and its engine are the root's, as is the
//! focus pump that hands the card the keyboard.
use super::*;

/// One Thread's dialog.
struct Dialog {
    prompt: Entity<Composer>,
    focus: FocusHandle,
    /// The page's dialog the card was last set up for.
    request: Option<DialogRequest>,
    /// The dialog has not yet had the keyboard: it takes it the first frame
    /// its own Browser is on screen, never while another Thread's is.
    unfocused: bool,
}

/// The script dialog of every Thread's Browser.
#[derive(Default)]
pub(in crate::cockpit) struct BrowserDialogView {
    threads: BTreeMap<ThreadKey, Dialog>,
}

/// What the operator answered a page's dialog.
pub(in crate::cockpit) enum BrowserDialogEvent {
    /// OK, Cancel, enter or escape: the shown page hears the answer.
    Answer(ThreadKey, DialogAnswer),
}

impl EventEmitter<BrowserDialogEvent> for BrowserDialogView {}

impl BrowserDialogView {
    /// `thread`'s dialog, made the first time its Browser is.
    pub(super) fn make(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        self.threads.entry(thread).or_insert_with(|| Dialog {
            prompt: cx.new(|cx| {
                let mut field = Composer::new(cx);
                field.set_role(Role::Field(Some("BrowserPrompt")), cx);
                field.set_visible_row_limit(1, cx);
                field
            }),
            focus: cx.focus_handle(),
            request: None,
            unfocused: false,
        });
    }

    /// Every Thread `keep` refuses takes its dialog with it.
    pub(in crate::cockpit) fn retain_threads(&mut self, keep: impl Fn(ThreadId) -> bool) {
        self.threads.retain(|key, _| keep(key.thread()));
    }

    #[cfg(test)]
    pub(in crate::cockpit) fn held_for_test(&self, thread: ThreadId) -> usize {
        self.threads.keys().filter(|key| key.thread() == thread).count()
    }

    /// The shown page asks `asked`: a new dialog sets the card up and waits
    /// for the keyboard.
    pub(super) fn sync(&mut self, thread: ThreadKey, asked: Option<DialogRequest>, cx: &mut Context<Self>) {
        let Some(dialog) = self.threads.get_mut(&thread) else {
            return;
        };
        if asked == dialog.request {
            return;
        }
        if let Some(DialogRequest {
            kind: DialogKind::Prompt { default },
            ..
        }) = &asked
        {
            let default = default.clone();
            dialog.prompt.update(cx, |field, cx| field.set(default, cx));
        }
        dialog.unfocused = asked.is_some();
        dialog.request = asked;
        cx.notify();
    }

    /// The focus pump found the card on screen: a dialog that has not had
    /// the keyboard takes it. True when it did.
    pub(super) fn take_keyboard(&mut self, thread: ThreadKey, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(dialog) = self.threads.get_mut(&thread) else {
            return false;
        };
        if !dialog.unfocused || dialog.request.is_none() {
            return false;
        }
        dialog.unfocused = false;
        dialog.take_focus(window, cx);
        true
    }

    /// The keyboard is in `thread`'s dialog.
    pub(super) fn holds_focus(&self, thread: ThreadKey, window: &Window, cx: &gpui::App) -> bool {
        self.threads
            .get(&thread)
            .is_some_and(|dialog| dialog.request.is_some() && dialog.focus.contains_focused(window, cx))
    }

    /// `thread`'s card over its page, while a page shows (`shown`) and has a
    /// dialog up.
    pub(super) fn card(&mut self, thread: ThreadKey, shown: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
        let dialog = self.threads.get(&thread)?;
        let request = dialog.request.clone().filter(|_| shown)?;
        if dialog.unfocused {
            // The pump hands it the keyboard on the next frame.
            let view = cx.entity();
            cx.defer(move |cx| view.update(cx, |_, cx| cx.notify()));
        }
        let choice = |id: &'static str, label: &'static str, accept: bool, cx: &mut Context<Self>| {
            ferrite_ui::components::button(id)
                .debug_selector(move || id.into())
                .tab_stop(true)
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.answer(thread, accept, cx)))
        };
        let prompt = matches!(request.kind, DialogKind::Prompt { .. });
        Some(
            div()
                .debug_selector(|| "browser-dialog".into())
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_start()
                .justify_center()
                .p(px(SPACE_4))
                .bg(paint::CARD)
                .occlude()
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        if let Some(dialog) = this.threads.get(&thread) {
                            if !dialog.focus.contains_focused(window, cx) {
                                dialog.take_focus(window, cx);
                            }
                        }
                    }),
                )
                .child(
                    div()
                        .id("browser-dialog-card")
                        .key_context("BrowserDialog")
                        .track_focus(&dialog.focus)
                        .flex()
                        .flex_col()
                        .gap(px(SPACE_2))
                        .w_full()
                        .max_w(px(420.))
                        .p(px(SPACE_3))
                        .rounded(px(R_CONTROL))
                        .border_1()
                        .border_color(paint::LINE)
                        .on_action(cx.listener(move |this, _: &AcceptDialog, _, cx| {
                            this.answer(thread, true, cx)
                        }))
                        .on_action(cx.listener(move |this, _: &DismissDialog, _, cx| {
                            this.answer(thread, false, cx)
                        }))
                        .child(
                            div()
                                .text_color(rgb(INK_STRONG))
                                .font_weight(W_LABEL)
                                .child(format!("{} says", request.origin)),
                        )
                        .child(div().text_color(rgb(INK_SUBTLE)).child(request.message))
                        .when(prompt, |card| {
                            card.child(
                                div()
                                    .h(px(ICON_BUTTON))
                                    .px(px(SPACE_2))
                                    .flex()
                                    .items_center()
                                    .rounded(px(R_CONTROL))
                                    .border_1()
                                    .border_color(paint::FIELD_RING)
                                    .bg(paint::FIELD)
                                    .child(div().flex_1().min_w_0().child(dialog.prompt.clone())),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(SPACE_2))
                                .when(!matches!(request.kind, DialogKind::Alert), |row| {
                                    row.child(choice("browser-dialog-cancel", "Cancel", false, cx))
                                })
                                .child(choice("browser-dialog-ok", "OK", true, cx)),
                        ),
                )
                .into_any_element(),
        )
    }

    /// The card closes on its answer, which the page hears from the root.
    fn answer(&mut self, thread: ThreadKey, accept: bool, cx: &mut Context<Self>) {
        let Some(dialog) = self.threads.get_mut(&thread) else {
            return;
        };
        dialog.unfocused = false;
        let Some(request) = dialog.request.take() else {
            return;
        };
        let answer = match request.kind {
            DialogKind::Alert => DialogAnswer::Accept(String::new()),
            DialogKind::Confirm if accept => DialogAnswer::Accept(String::new()),
            DialogKind::Prompt { .. } if accept => {
                DialogAnswer::Accept(dialog.prompt.read(cx).text().to_owned())
            }
            _ => DialogAnswer::Cancel,
        };
        cx.emit(BrowserDialogEvent::Answer(thread, answer));
        cx.notify();
    }
}

impl Dialog {
    fn take_focus(&self, window: &mut Window, cx: &mut gpui::App) {
        let prompt = matches!(
            self.request.as_ref().map(|request| &request.kind),
            Some(DialogKind::Prompt { .. })
        );
        if prompt {
            self.prompt.update(cx, |line, cx| {
                line.select_all(&ferrite_pane::composer::SelectAll, window, cx)
            });
            let target = self.prompt.read(cx).focus_target(window, cx);
            window.focus(&target, cx);
        } else {
            window.focus(&self.focus, cx);
        }
    }
}

impl CockpitView {
    /// The dialog `thread`'s shown page has up: none while a new tab is being
    /// entered, or for a shown tab without a page (closed, not loaded yet,
    /// the Server gone).
    pub(super) fn browser_dialog_inputs(&self, thread: ThreadKey, cx: &gpui::App) -> Option<DialogRequest> {
        let browser = self.browsers.threads.get(&thread)?;
        let shown = browser.shown_tab(self.browser_chrome.read(cx).new_tab(thread))?;
        browser.pages.get(&shown.id)?.native.state().dialog
    }

    /// A dialog is modal to its page: it takes the keyboard when its Browser
    /// is on screen, and lets the operator leave for anything outside it.
    pub(in crate::cockpit) fn focus_shown_dialogs(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let mut took = false;
        for (thread, browser) in &mut self.browsers.threads {
            let drawn = browser.dialog_drawn.replace(false);
            browser.dialog_on_screen = drawn;
            if drawn
                && self
                    .browser_dialog
                    .update(cx, |dialog, cx| dialog.take_keyboard(*thread, window, cx))
            {
                took = true;
            }
        }
        took
    }

    pub(in crate::cockpit) fn dialog_holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        let dialogs = self.browser_dialog.read(cx);
        self.browsers
            .threads
            .iter()
            .any(|(thread, browser)| browser.dialog_on_screen && dialogs.holds_focus(*thread, window, cx))
    }

    /// The operator answered a page's dialog: the shown page hears it.
    pub(in crate::cockpit) fn on_browser_dialog_event(
        &mut self,
        _: &Entity<BrowserDialogView>,
        event: &BrowserDialogEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let BrowserDialogEvent::Answer(thread, answer) = event;
        let new_tab = self.browser_chrome.read(cx).new_tab(*thread);
        let Some(browser) = self.browsers.threads.get(thread) else {
            return;
        };
        let Some(tab) = browser.shown_tab(new_tab) else {
            return;
        };
        if let Some(page) = browser.pages.get(&tab.id) {
            page.native.answer_dialog(answer.clone());
        }
        cx.notify();
    }
}
