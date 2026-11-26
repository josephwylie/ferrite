//! An MCP form's card and a link's: the fields it asks for, and the page
//! it opens.

use super::*;

impl Cards {
    pub(super) fn form_card(
        &self,
        request: PendingDecision,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> AnyElement {
        let thread = self.thread;
        let handle = request.handle.clone();
        let ferrite_core::DecisionKind::Form { fields } = &request.decision.kind else {
            unreachable!("dispatched on the Form kind")
        };
        let fields = fields.clone();
        let (head, title, error) = self.preamble(&request);
        let forms = self.forms.clone();
        if !forms.0.borrow().contains_key(&handle) {
            let mut form_inputs = HashMap::new();
            for field in &fields {
                let initial = match &field.kind {
                    ferrite_core::FormFieldKind::String { default, .. } => {
                        default.clone().unwrap_or_default()
                    }
                    ferrite_core::FormFieldKind::Number { default, .. } => {
                        default.map(|value| value.to_string()).unwrap_or_default()
                    }
                    ferrite_core::FormFieldKind::Integer { default, .. } => {
                        default.map(|value| value.to_string()).unwrap_or_default()
                    }
                    _ => continue,
                };
                let input = cx.new(|cx| {
                    let mut input = InputState::new(window, cx);
                    input.set_value(initial, window, cx);
                    input
                });
                form_inputs.insert(field.id.clone(), input);
            }
            forms.0.borrow_mut().insert(
                handle.clone(),
                RequestForm {
                    answers: Vec::new(),
                    inputs: Vec::new(),
                    form_inputs,
                    values: form_defaults(&fields),
                    scroll: ScrollHandle::new(),
                },
            );
        }
        let submit_handle = handle.clone();
        let selector = format!("request-submit-{}-{}", thread.thread().get(), handle.serial);
        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(theme::DECISION_QUESTIONS_GAP));
        for (field_index, field) in fields.iter().enumerate() {
            let mut section = div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(theme::SPACE_1_5))
                .debug_selector({
                    let id = field.id.clone();
                    move || format!("form-field-{id}")
                })
                .child(
                    div()
                        .text_color(rgb(theme::INK_STRONG))
                        .child(field.label.clone()),
                );
            if !field.description.is_empty() {
                section = section.child(decision::note(field.description.clone()));
            }
            match &field.kind {
                ferrite_core::FormFieldKind::String { .. }
                | ferrite_core::FormFieldKind::Number { .. }
                | ferrite_core::FormFieldKind::Integer { .. } => {
                    if let Some(input) = forms.0.borrow()[&handle].form_inputs.get(&field.id) {
                        let id = field.id.clone();
                        section = section.child(
                            div()
                                .debug_selector(move || format!("form-input-{id}"))
                                .child(Input::new(input).disabled(request.submitting)),
                        );
                    }
                }
                ferrite_core::FormFieldKind::Boolean { .. } => {
                    let checked = forms.0.borrow()[&handle]
                        .values
                        .get(&field.id)
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false);
                    let forms = forms.clone();
                    let handle = handle.clone();
                    let id = field.id.clone();
                    let owner = self.owner.clone();
                    section = section.child(
                        Checkbox::new(("form-bool", field_index))
                            .checked(checked)
                            .disabled(request.submitting)
                            .child("Enabled")
                            .on_click(move |checked: &bool, _: &mut Window, cx: &mut gpui::App| {
                                if let Some(form) = forms.0.borrow_mut().get_mut(&handle) {
                                    form.values
                                        .insert(id.clone(), serde_json::Value::Bool(*checked));
                                }
                                refresh(&owner, cx);
                            }),
                    );
                }
                ferrite_core::FormFieldKind::Enum {
                    options,
                    multi_select,
                    ..
                } => {
                    let selected = forms.0.borrow()[&handle].values.get(&field.id).cloned();
                    if *multi_select {
                        for (option_index, option) in options.iter().enumerate() {
                            let checked = selected
                                .as_ref()
                                .and_then(|value| value.as_array())
                                .is_some_and(|values| {
                                    values.iter().any(|value| {
                                        value == &serde_json::Value::String(option.value.clone())
                                    })
                                });
                            let forms = forms.clone();
                            let handle = handle.clone();
                            let id = field.id.clone();
                            let value = option.value.clone();
                            let owner = self.owner.clone();
                            section = section.child(
                                Checkbox::new(("form-enum", field_index * 256 + option_index))
                                    .checked(checked)
                                    .disabled(request.submitting)
                                    .child(option.label.clone())
                                    .on_click(
                                        move |checked: &bool, _: &mut Window, cx: &mut gpui::App| {
                                            if let Some(form) =
                                                forms.0.borrow_mut().get_mut(&handle)
                                            {
                                                let values = form
                                                    .values
                                                    .entry(id.clone())
                                                    .or_insert_with(|| {
                                                        serde_json::Value::Array(Vec::new())
                                                    });
                                                let values = values
                                                    .as_array_mut()
                                                    .expect("form enum is an array");
                                                values.retain(|item| {
                                                    item != &serde_json::Value::String(
                                                        value.clone(),
                                                    )
                                                });
                                                if *checked {
                                                    values.push(serde_json::Value::String(
                                                        value.clone(),
                                                    ));
                                                }
                                            }
                                            refresh(&owner, cx);
                                        },
                                    ),
                            );
                        }
                    } else {
                        let selected_index = selected
                            .as_ref()
                            .and_then(|value| value.as_str())
                            .and_then(|value| {
                                options.iter().position(|option| option.value == value)
                            });
                        let forms = forms.clone();
                        let handle = handle.clone();
                        let id = field.id.clone();
                        let options = options.clone();
                        let owner = self.owner.clone();
                        section = section.child(
                            RadioGroup::vertical(("form-enum", field_index))
                                .selected_index(selected_index)
                                .disabled(request.submitting)
                                .children(options.iter().enumerate().map(|(index, option)| {
                                    Radio::new(index).child(option.label.clone())
                                }))
                                .on_click(
                                    move |selected: &usize, _: &mut Window, cx: &mut gpui::App| {
                                        if let (Some(form), Some(option)) = (
                                            forms.0.borrow_mut().get_mut(&handle),
                                            options.get(*selected),
                                        ) {
                                            form.values.insert(
                                                id.clone(),
                                                serde_json::Value::String(option.value.clone()),
                                            );
                                        }
                                        refresh(&owner, cx);
                                    },
                                ),
                        );
                    }
                }
            }
            body = body.child(section);
        }
        let cancel_handle = handle.clone();
        let mut children = vec![head.into_any_element()];
        children.extend(title.map(|title| decision::section(title).into_any_element()));
        let scroll = forms.0.borrow()[&handle].scroll.clone();
        children.push(
            div()
                .id(("form-body", handle.serial as usize))
                .w_full()
                .min_w_0()
                .flex_shrink_1()
                .max_h(px(theme::DECISION_BODY_MAX_H))
                .overflow_y_scroll()
                .track_scroll(&scroll)
                .vertical_scrollbar(&scroll)
                .pr(px(theme::DECISION_SCROLL_GUTTER))
                .child(body)
                .into_any_element(),
        );
        children.extend(error.map(|error| {
            decision::section(
                decision::error_line("not sent", error)
                    .debug_selector(|| "form-validation-error".into()),
            )
            .into_any_element()
        }));
        let sending = request.submitting;
        children.push(
            decision::section(decision::footer([
                decision::skip_button("form-cancel", "Cancel", cx)
                    .disabled(!request.decision.policy.deny || sending)
                    .debug_selector(|| "form-cancel".into())
                    .on_click(self.act(move || DecisionEvent::Respond {
                        thread,
                        handle: cancel_handle.clone(),
                        answer: DecisionAnswer::Cancel,
                    }))
                    .into_any_element(),
                decision::send_button(
                    "form-send",
                    if sending { "Sending…" } else { "Send" },
                    !request.decision.policy.allow || sending,
                    false,
                    cx,
                )
                .debug_selector(move || selector.clone())
                .on_click(self.act(move || DecisionEvent::SendForm {
                    thread,
                    handle: submit_handle.clone(),
                    fields: fields.clone(),
                }))
                .into_any_element(),
            ]))
            .into_any_element(),
        );
        self.frame(decision::card(handle.serial, children), self.overlay)
            .into_any_element()
    }

    /// A request finished outside Ferrite (`External`), or one it cannot
    /// answer here (`Unsupported`): the reason, and the actions it allows.
    pub(super) fn link_card(&self, request: PendingDecision, cx: &mut gpui::App) -> AnyElement {
        let thread = self.thread;
        let handle = request.handle.clone();
        let (head, title, error) = self.preamble(&request);
        let sending = request.submitting;
        let mut children = vec![head.into_any_element()];
        children.extend(title.map(|title| decision::section(title).into_any_element()));
        let cancel_handle = handle.clone();
        let cancel = decision::skip_button(
            match &request.decision.kind {
                ferrite_core::DecisionKind::External { .. } => "external-cancel",
                _ => "unsupported-cancel",
            },
            "Cancel",
            cx,
        )
        .disabled(!request.decision.policy.deny || sending)
        .on_click(self.act(move || DecisionEvent::Respond {
            thread,
            handle: cancel_handle.clone(),
            answer: DecisionAnswer::Cancel,
        }))
        .into_any_element();
        let actions = match &request.decision.kind {
            ferrite_core::DecisionKind::External { url } => {
                children.push(
                    decision::section(decision::note(
                        "Complete this request in your browser, then confirm here.",
                    ))
                    .into_any_element(),
                );
                let url = url.clone();
                let complete_handle = handle.clone();
                vec![
                    decision::skip_button("external-open", "Open link", cx)
                        .disabled(!safe_external_url(&url))
                        .on_click(move |_, _, cx| cx.open_url(&url))
                        .into_any_element(),
                    cancel,
                    decision::send_button(
                        "external-complete",
                        "Complete",
                        !request.decision.policy.allow || sending,
                        false,
                        cx,
                    )
                    .on_click(self.act(move || DecisionEvent::Respond {
                        thread,
                        handle: complete_handle.clone(),
                        answer: DecisionAnswer::Allow {
                            input: serde_json::Value::Null,
                        },
                    }))
                    .into_any_element(),
                ]
            }
            ferrite_core::DecisionKind::Unsupported { reason } => {
                children.push(decision::section(decision::note(reason.clone())).into_any_element());
                vec![cancel]
            }
            _ => vec![cancel],
        };
        children.extend(error.map(|error| {
            decision::section(decision::error_line("could not send", error)).into_any_element()
        }));
        children.push(decision::section(decision::footer(actions)).into_any_element());
        self.frame(decision::card(handle.serial, children), self.overlay)
            .into_any_element()
    }
}

fn safe_external_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

fn form_defaults(fields: &[ferrite_core::FormField]) -> serde_json::Map<String, serde_json::Value> {
    fields
        .iter()
        .filter_map(|field| {
            let value = match &field.kind {
                ferrite_core::FormFieldKind::String { default, .. } => {
                    default.clone().map(serde_json::Value::String)
                }
                ferrite_core::FormFieldKind::Number { default, .. } => default
                    .and_then(serde_json::Number::from_f64)
                    .map(serde_json::Value::Number),
                ferrite_core::FormFieldKind::Integer { default, .. } => {
                    default.map(serde_json::Value::from)
                }
                ferrite_core::FormFieldKind::Boolean { default } => default
                    .or(field.required.then_some(false))
                    .map(serde_json::Value::from),
                ferrite_core::FormFieldKind::Enum {
                    options,
                    multi_select,
                    default,
                    ..
                } => {
                    let allowed = |value: &str| options.iter().any(|option| option.value == value);
                    match default {
                        Some(serde_json::Value::String(value))
                            if !multi_select && allowed(value) =>
                        {
                            Some(serde_json::Value::String(value.clone()))
                        }
                        Some(serde_json::Value::Array(values)) if *multi_select => {
                            Some(serde_json::Value::Array(
                                values
                                    .iter()
                                    .filter(|value| value.as_str().is_some_and(allowed))
                                    .cloned()
                                    .collect(),
                            ))
                        }
                        _ => None,
                    }
                }
            }?;
            Some((field.id.clone(), value))
        })
        .collect()
}
