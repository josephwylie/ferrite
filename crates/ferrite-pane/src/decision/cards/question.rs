//! A question's card: its questions, their options, the words field and
//! the note.

use super::*;

impl Cards {
    /// A question (D-8): the form, in the Decision's row grammar — the `?`
    /// head with the question, numbered options with the `❯` cursor and its
    /// bar, the own-answer line, and the dim hint beside the form's Skip
    /// and Send.
    pub(super) fn question_card(
        &self,
        request: PendingDecision,
        questions: Vec<ferrite_core::questions::Question>,
        working: bool,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> AnyElement {
        let thread = self.thread;
        let handle = request.handle.clone();
        let forms = self.forms.clone();
        if !forms.0.borrow().contains_key(&handle) {
            let inputs = questions
                .iter()
                .map(|question| {
                    cx.new(|cx| {
                        InputState::new(window, cx)
                            .placeholder("Or type your own answer\u{2026}")
                            .masked(question.secret)
                    })
                })
                .collect();
            forms.0.borrow_mut().insert(
                handle.clone(),
                RequestForm {
                    answers: vec![Default::default(); questions.len()],
                    inputs,
                    form_inputs: Default::default(),
                    values: Default::default(),
                    scroll: ScrollHandle::new(),
                },
            );
        }
        let sending = request.submitting;
        let answers = forms.0.borrow()[&handle].answers.clone();
        let scroll = forms.0.borrow()[&handle].scroll.clone();
        // The question the digit keys and the cursor pick in; only its rows
        // show digits.
        let target = digit_question(&answers, &questions);
        let cursor = target.map(|qi| {
            forms
                .cursor(&handle)
                .unwrap_or(0)
                .min(questions[qi].options.len().saturating_sub(1))
        });
        let single = questions.len() == 1;
        let overlay = self.overlay;
        let mut content = div()
            .id(("question-content", handle.serial as usize))
            .debug_selector(|| "question-scroll-content".into())
            .w_full()
            .min_w_0()
            .flex_shrink_1()
            .flex()
            .flex_col();
        if self.overlay {
            content = content
                .max_h(px(if self.short {
                    theme::DECISION_SHORT_BODY_MAX_H
                } else {
                    theme::DECISION_BODY_MAX_H
                }))
                .overflow_y_scroll()
                .track_scroll(&scroll)
                .vertical_scrollbar(&scroll)
                .pr(px(theme::DECISION_SCROLL_GUTTER));
        }
        for (qi, question) in questions.iter().enumerate() {
            let keyed = target == Some(qi);
            let digit = |at: usize| {
                (keyed && at < decision::DIGIT_KEYS)
                    .then(|| SharedString::from((at + 1).to_string()))
            };
            let mut section = div()
                .flex_shrink_0()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .when(qi > 0, |section| {
                    section.mt(px(theme::DECISION_QUESTIONS_GAP))
                });
            // A lone question is the head itself; several each lead their
            // own rows.
            if !single {
                section = section.child(
                    decision::question_text(question.question.clone())
                        .mb(px(theme::DECISION_QUESTION_GAP)),
                );
            }
            // The hint's `1–N pick` already says "choose any" for the
            // question the digits reach; another multi-select says it here.
            if question.multi_select && !keyed {
                section = section.child(decision::note("choose any"));
            }
            let mut rows = div().w_full().min_w_0().flex().flex_col();
            for (oi, option) in question.options.iter().enumerate() {
                let (label, recommended) = decision::split_recommended(&option.label);
                let pick_forms = forms.clone();
                let pick_handle = handle.clone();
                let multi = question.multi_select;
                rows = rows.child(
                    decision::option_row(
                        ("question-choice", qi * 256 + oi),
                        decision::Row {
                            key: digit(oi),
                            label: SharedString::from(label.to_string()),
                            description: Some(option.description.clone().into()),
                            recommended,
                            cursor: keyed && cursor == Some(oi),
                            picked: answers[qi].picks.contains(&oi),
                            enabled: !sending,
                            ..Default::default()
                        },
                    )
                    .debug_selector(move || format!("question-choice-{qi}-{oi}"))
                    .on_click({
                        let owner = self.owner.clone();
                        move |_, _, cx| {
                            if let Some(form) = pick_forms.0.borrow_mut().get_mut(&pick_handle) {
                                pick_option(&mut form.answers[qi].picks, oi, multi);
                            }
                            pick_forms.set_cursor(&pick_handle, Some(oi));
                            let handle = pick_handle.clone();
                            ask(&owner, DecisionEvent::Picked { thread, handle }, cx);
                        }
                    }),
                );
            }
            section = section.child(rows);
            // The option in hand shows its preview (a layout, a snippet) in
            // the command well's own style, as plain text: the cursor's
            // option, else the one picked last.
            let previewed = (keyed.then_some(cursor).flatten())
                .or_else(|| answers[qi].picks.last().copied())
                .and_then(|at| question.options.get(at))
                .and_then(|option| option.preview.as_deref())
                .filter(|preview| !preview.trim().is_empty());
            if let Some(preview) = previewed {
                section = section.child(
                    decision::well(false, preview.to_owned())
                        .debug_selector(move || format!("question-preview-{qi}"))
                        .mt(px(theme::DECISION_QUESTION_GAP)),
                );
            }
            if question.allow_other {
                // The own answer is not a second field: one bare line on
                // the options' grid — an option row high, its digit where
                // theirs stand, its text on their labels' column, in their
                // face — that the digit one past the options arms.
                let selector = format!("request-other-{}-{}-{qi}", thread.thread().get(), handle.serial);
                section = section.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .min_h(px(theme::DECISION_OPTION_H))
                        .flex()
                        .items_center()
                        .gap(px(theme::DECISION_OPTION_GAP))
                        .px(px(theme::DECISION_OPTION_PAD_X))
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(px(theme::FS_META * theme::CODE_ADVANCE))
                                .font_family(theme::FONT_CODE)
                                .text_size(px(theme::FS_META))
                                .text_color(rgb(theme::INK_SUBTLE))
                                .children(digit(question.options.len())),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .h(px(theme::QUESTION_OTHER_H))
                                .debug_selector(move || selector.clone())
                                .cursor_text()
                                .child(
                                    gpui::Styled::h(
                                        Input::new(&forms.0.borrow()[&handle].inputs[qi])
                                            .appearance(false)
                                            .disabled(sending),
                                        px(theme::QUESTION_OTHER_H),
                                    )
                                    .px(px(0.))
                                    .py(px(0.))
                                    .font_family(theme::FONT_UI)
                                    .text_size(px(theme::FS_UI))
                                    .text_color(rgb(theme::INK_STRONG)),
                                ),
                        ),
                );
            }
            content = content.child(section);
        }
        // A plain wait says nothing the card does not: only a status that
        // adds something rides the head.
        let status = if sending {
            Some(decision::sending().into_any_element())
        } else if working {
            Some(decision::status("work continues").into_any_element())
        } else if !request.decision.blocks_execution() {
            Some(decision::status("answer when ready").into_any_element())
        } else {
            None
        };
        let lead = if single {
            questions[0].question.clone()
        } else {
            format!("{} questions", questions.len())
        };
        let head = decision::question_head(
            lead,
            decision::head_detail([if request.subject.is_none() {
                "agent unknown"
            } else {
                ""
            }]),
            status,
        );
        let mut children = vec![
            head.into_any_element(),
            decision::section(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .min_h_0()
                    .overflow_hidden()
                    .debug_selector(|| "question-viewport".into())
                    .child(content),
            )
            .into_any_element(),
        ];
        if let Some(error) = request.reply_error.as_ref().or_else(|| {
            self.request_error
                .as_ref()
                .filter(|(failed, _)| failed == &handle)
                .map(|(_, error)| error)
        }) {
            children.push(
                decision::section(decision::error_line("not sent", error.clone()))
                    .into_any_element(),
            );
        }
        if let Some(note) = self.forms.note(&handle) {
            children.push(self.note_field(&handle, note));
        }
        // The form keeps its Skip and Send for the pointer (several
        // questions, several picks, typed words); the rows carry their digits.
        let skip_handle = handle.clone();
        let submit_handle = handle.clone();
        let selector = format!("request-submit-{}-{}", thread.thread().get(), handle.serial);
        children.push(
            decision::section(decision::footer([
                decision::skip_button("question-skip", "Skip", cx)
                    .disabled(sending)
                    .on_click(self.act(move || DecisionEvent::Respond {
                        thread,
                        handle: skip_handle.clone(),
                        answer: DecisionAnswer::Deny {
                            message: decision::QUESTION_SKIPPED.into(),
                        },
                    }))
                    .into_any_element(),
                decision::send_button(
                    "question-send",
                    if sending { "Sending\u{2026}" } else { "Send" },
                    sending,
                    !sending,
                    cx,
                )
                .debug_selector(move || selector.clone())
                .on_click(self.act(move || DecisionEvent::SendQuestions {
                    thread,
                    handle: submit_handle.clone(),
                }))
                .into_any_element(),
            ]))
            .into_any_element(),
        );
        self.frame(decision::card(handle.serial, children), overlay)
            .into_any_element()
    }
}
