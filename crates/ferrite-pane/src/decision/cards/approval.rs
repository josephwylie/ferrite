//! An approval's card: its head, the command or edit it asks for, its
//! three options and the note sent with one.

use super::*;

impl Cards {
    /// An approval (D-1…D-7), as Claude's card: the ask on the head band,
    /// the prose, the command band (or the edit's diff), the amend note
    /// while it is open, then `Deny` and the split `Allow once` whose menu
    /// holds `Always allow <pattern>` (its note says what that covers) and
    /// `Allow and switch to …` — inline, or stacked in the compact
    /// inspector's narrow column (`narrow`). The keys are on the buttons (⌘⏎ on `Allow once`, ⇧⌘⏎
    /// on the menu's `Always allow`, esc); 1–3, ↑↓ and ⏎ still act.
    pub(super) fn approval_card(&self, request: PendingDecision, cx: &gpui::App) -> AnyElement {
        let thread = self.thread;
        let handle = request.handle.clone();
        let serial = handle.serial;
        let (head, title, error) = self.preamble(&request);
        let mut children = vec![head.into_any_element()];
        // A short Pane gives up the prose first — the head and the command
        // still say what is asked.
        children.extend(
            title
                .filter(|_| !self.short)
                .map(|title| decision::section(title).into_any_element()),
        );
        let asked = self
            .asked
            .iter()
            .find(|(id, _)| *id == request.decision.tool_use_id)
            .map(|(_, diff)| diff);
        if let Some(diff) = pane::approval_diff(
            &request.decision,
            asked,
            self.workspace.as_deref(),
            self.reading,
        ) {
            children.push(decision::section(diff).into_any_element());
        } else if let Some(input) = pane::approval_input(
            &request.decision,
            &self.rich,
            format!(
                "approval-input-{}-{}-{}-{}",
                if self.overlay { "overlay" } else { "tail" },
                thread.thread().get(),
                handle.generation,
                serial
            )
            .into(),
        ) {
            children.push(
                decision::section(
                    decision::well(pane::shell_command(&request.decision), input)
                        .debug_selector(|| "approval-well".into()),
                )
                .into_any_element(),
            );
        }
        children.extend(error.map(|error| {
            decision::section(decision::error_line("could not send", error)).into_any_element()
        }));
        let rows =
            decision::approval_rows(&request.decision, self.provider, self.workspace.as_deref());
        let enabled = |choice: decision::ApprovalChoice| {
            !request.submitting && rows.iter().any(|row| row.choice == choice && row.enabled)
        };
        if let Some(note) = self.forms.note(&handle) {
            children.push(self.note_field(&handle, note));
        }
        let answer = |choice: decision::ApprovalChoice| {
            let pick = request.clone();
            self.act(move || DecisionEvent::Approve {
                thread,
                request: pick.clone(),
                choice,
                switch: None,
            })
        };
        let stacked = self.narrow;
        let deny = decision::deny_button(
            SharedString::from(format!("request-deny-{}-{serial}", thread.thread().get())),
            stacked,
            !enabled(decision::ApprovalChoice::DenyAndSteer),
            cx,
        )
        .debug_selector(move || format!("request-deny-{}-{serial}", thread.thread().get()))
        .on_click(answer(decision::ApprovalChoice::DenyAndSteer))
        .relative()
        // The deny control has always answered to this name.
        .child(
            div()
                .absolute()
                .inset_0()
                .debug_selector(|| "decision-deny".into()),
        );
        let allow = decision::allow_button(
            SharedString::from(format!("request-allow-{}-{serial}", thread.thread().get())),
            stacked,
            !enabled(decision::ApprovalChoice::Allow),
            cx,
        )
        .debug_selector(move || format!("request-allow-{}-{serial}", thread.thread().get()))
        .on_click(answer(decision::ApprovalChoice::Allow));
        let open = self.forms.menu(&handle);
        let forms = self.forms.clone();
        let toggled = handle.clone();
        let owner = self.owner.clone();
        // Where the `▾` was laid out: a press there is its own, which shuts
        // the menu, not a press outside it.
        let door: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>> = Rc::default();
        let submitting = request.submitting;
        let more = decision::more_button(
            SharedString::from(format!("request-more-{}-{serial}", thread.thread().get())),
            submitting,
            cx,
        )
        .debug_selector(move || format!("request-more-{}-{serial}", thread.thread().get()))
        .menu_button(
            open,
            |button| button,
            {
                let door = door.clone();
                move |bounds, _, _| door.set(Some(bounds))
            },
            move |_, cx| {
                if !submitting {
                    forms.set_menu(&toggled, !forms.menu(&toggled));
                    refresh(&owner, cx);
                }
            },
        );
        let menu = open.then(|| {
            let mut rows = Vec::new();
            if enabled(decision::ApprovalChoice::AllowForThread) {
                let forms = self.forms.clone();
                let closing = handle.clone();
                let always = answer(decision::ApprovalChoice::AllowForThread);
                let (label, note) = decision::always_allow_words(&decision::option_two_pattern(
                    &request.decision,
                    self.workspace.as_deref(),
                ));
                rows.push(
                    decision::allow_menu_row(
                        SharedString::from(format!("request-always-{}-{serial}", thread.thread().get())),
                        label,
                        decision::decision_chord("cockpit::AlwaysAllow"),
                        Some(note),
                    )
                    .debug_selector(move || format!("request-always-{}-{serial}", thread.thread().get()))
                    .on_click(move |event, window, cx| {
                        forms.set_menu(&closing, false);
                        always(event, window, cx);
                    })
                    .into_any_element(),
                );
            }
            if let Some((mode, label, generation)) = self
                .switch
                .clone()
                .filter(|_| enabled(decision::ApprovalChoice::Allow))
            {
                let pick = request.clone();
                let forms = self.forms.clone();
                let closing = handle.clone();
                let what = ferrite_ui::theme::mode_note(&mode).map(SharedString::from);
                rows.push(
                    decision::allow_menu_row(
                        SharedString::from(format!("request-switch-{}-{serial}", thread.thread().get())),
                        format!("Allow and switch to {label}"),
                        None,
                        what,
                    )
                    .debug_selector(move || format!("request-switch-{}-{serial}", thread.thread().get()))
                    .on_click({
                        let act = self.act(move || DecisionEvent::Approve {
                            thread,
                            request: pick.clone(),
                            choice: decision::ApprovalChoice::Allow,
                            switch: Some((mode.clone(), generation)),
                        });
                        move |event, window, cx| {
                            forms.set_menu(&closing, false);
                            act(event, window, cx);
                        }
                    })
                    .into_any_element(),
                );
            }
            // A press outside closes it, as it closes every popup (ADR 0043).
            let forms = self.forms.clone();
            let closing = handle.clone();
            let owner = self.owner.clone();
            decision::allow_menu(rows)
                .on_mouse_down_out(move |event, _, cx| {
                    if !door.get().is_some_and(|door| door.contains(&event.position)) {
                        forms.set_menu(&closing, false);
                        refresh(&owner, cx);
                    }
                })
                .into_any_element()
        });
        let mut actions = decision::approval_actions(
            deny.into_any_element(),
            allow.into_any_element(),
            more.into_any_element(),
            menu,
            stacked,
        );
        // Allow and Deny: what must stay in sight at a transcript's foot.
        if let Some(geometry) = self.kept_in.clone() {
            actions = ferrite_ui::components::on_bounds(actions, move |at, _, _| {
                let kept = geometry.get();
                let answers = kept.answers.with_buttons(at);
                geometry.set(crate::pane::PaneGeometry { answers, ..kept });
            });
        }
        children.push(decision::section(actions).into_any_element());
        self.frame(decision::approval_card(serial, children), self.overlay)
            .into_any_element()
    }

    /// The amend note (⇥, D-6): the one-line field under the options. ⏎
    /// sends the cursor's option with it; esc closes it. Inside a child's
    /// `Decision` context the answer keys are bound — in the note they are
    /// letters again.
    pub(super) fn note_field(
        &self,
        handle: &DecisionHandle,
        note: Entity<crate::composer::Composer>,
    ) -> AnyElement {
        let thread = self.thread;
        let (send, confirm, close, dismiss) = (
            handle.clone(),
            handle.clone(),
            handle.clone(),
            handle.clone(),
        );
        let owner = self.owner.clone();
        let enter = handle.clone();
        let mut field = div()
            .w_full()
            .min_w_0()
            // The card's controls keep ↵ for themselves (`native_keys`), so
            // no binding reaches the note: it hears its own ↵.
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let keys = &event.keystroke;
                let plain = !(keys.modifiers.shift
                    || keys.modifiers.control
                    || keys.modifiers.alt
                    || keys.modifiers.platform);
                if keys.key == "enter" && plain {
                    cx.stop_propagation();
                    window.prevent_default();
                    let handle = enter.clone();
                    ask(&owner, DecisionEvent::SendNote { thread, handle }, cx);
                }
            })
            .on_action(self.act::<Submit>(move || DecisionEvent::SendNote {
                thread,
                handle: send.clone(),
            }))
            .on_action(self.act::<decision::Confirm>(move || DecisionEvent::SendNote {
                thread,
                handle: confirm.clone(),
            }))
            .on_action(self.act::<Interrupt>(move || DecisionEvent::CloseNote {
                thread,
                handle: close.clone(),
            }))
            .on_action(self.act::<decision::Dismiss>(move || DecisionEvent::CloseNote {
                thread,
                handle: dismiss.clone(),
            }));
        macro_rules! letter {
            ($action:ty, $text:expr) => {{
                let note = note.clone();
                field = field.on_action(move |_: &$action, _: &mut Window, cx: &mut gpui::App| {
                    note.update(cx, |line, cx| line.insert($text, cx));
                });
            }};
        }
        letter!(PickOption1, "1");
        letter!(PickOption2, "2");
        letter!(PickOption3, "3");
        letter!(PickOption4, "4");
        field.child(decision::note_row(note)).into_any_element()
    }
}
