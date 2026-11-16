use super::*;
use ferrite_ui::frame::settings::{ConnectionAction, ConnectionForm, ConnectionRow, WindowAct};

fn button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    enabled: bool,
    act: WindowAct,
    cx: &App,
) -> Button {
    let id = id.into();
    quiet_button(id.clone(), label.into(), cx)
        .debug_selector(move || id.to_string())
        .disabled(!enabled)
        .tab_stop(enabled)
        .when(!enabled, |button| button.opacity(0.5).cursor_default())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            if enabled {
                act(window, cx);
            }
        })
}

fn text(value: impl Into<SharedString>) -> Div {
    components::text_ui()
        .min_w_0()
        .text_color(rgb(INK_SUBTLE))
        .child(value.into())
}

fn field(id: &'static str, label: &'static str, input: &Entity<InputState>, disabled: bool) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(px(SPACE_1))
        .child(components::form_label(label, INK_SUBTLE))
        .child(
            div().debug_selector(move || id.into()).child(
                Input::new(input)
                    .small()
                    .aria_label(label)
                    .disabled(disabled),
            ),
        )
}

pub(super) fn form(form: ConnectionForm, cx: &App) -> Div {
    let body = div()
        .flex()
        .flex_col()
        .min_w_0()
        .gap(px(SPACE_2))
        .px(px(SETTINGS_ROW_PAD_X))
        .py(px(SETTINGS_ROW_PAD_Y));
    let ConnectionForm::Editing {
        fields,
        suggestions,
        choose_host,
        progress,
        error,
        warning,
        verification,
        submit,
        cancel,
    } = form
    else {
        let ConnectionForm::Closed { open, error } = form else {
            unreachable!()
        };
        return body
            .child(text(
                "Connect to a cloud machine or another computer over SSH.",
            ))
            .children(error.map(|error| {
                text(error)
                    .debug_selector(|| "connection-error".into())
                    .text_color(rgb(RED))
            }))
            .child(div().child(button("connection-add", "Add environment", true, open, cx)));
    };
    let busy = progress.is_some();
    let inputs = div()
        .flex()
        .flex_col()
        .min_w_0()
        .gap(px(SPACE_2))
        .child(field(
            "connection-host",
            "SSH host or alias",
            &fields.host,
            busy,
        ))
        .when(!busy && !suggestions.is_empty(), |body| {
            body.child(div().flex().flex_wrap().gap(px(SPACE_1)).children(
                suggestions.into_iter().enumerate().map(|(index, host)| {
                    let choose = choose_host.clone();
                    let value = host.clone();
                    button(
                        format!("connection-host-suggestion-{index}"),
                        host,
                        true,
                        Rc::new(move |window, cx| choose(value.clone(), window, cx)),
                        cx,
                    )
                }),
            ))
        })
        .child(
            div()
                .flex()
                .min_w_0()
                .gap(px(SPACE_2))
                .child(field(
                    "connection-username",
                    "Username (optional)",
                    &fields.username,
                    busy,
                ))
                .child(div().w(px(120.)).child(field(
                    "connection-port",
                    "Port (optional)",
                    &fields.port,
                    busy,
                ))),
        );
    let mut body = body
        .when(verification.is_none(), |body| body.child(inputs))
        .children(error.map(|error| {
            text(error)
                .debug_selector(|| "connection-error".into())
                .text_color(rgb(RED))
        }))
        .children(
            warning.map(|warning| text(warning).debug_selector(|| "connection-warning".into())),
        )
        .children(
            progress.map(|progress| text(progress).debug_selector(|| "connection-progress".into())),
        );
    if let Some(verification) = verification {
        let submit = verification.submit.clone();
        let mut prompt = div()
            .id("connection-verification")
            .debug_selector(|| "connection-verification".into())
            .when_some(verification.focus.as_ref(), |prompt, focus| {
                prompt.track_focus(focus)
            })
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" && !event.keystroke.modifiers.modified() {
                    cx.stop_propagation();
                    submit(window, cx);
                }
            })
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .pt(px(SPACE_2))
            .border_t_1()
            .border_color(paint::FLOAT_EDGE)
            .child(components::form_label(
                "SSH verification required",
                INK_STRONG,
            ))
            .child(text(verification.target))
            .child(
                div()
                    .id("connection-prompt-scroll")
                    .debug_selector(|| "connection-prompt".into())
                    .max_h(px(96.))
                    .overflow_y_scroll()
                    .child(text(verification.prompt).text_color(rgb(INK_STRONG))),
            )
            .child(text(verification.expires).text_size(px(FS_META)));
        if let Some(response) = verification.response {
            prompt = prompt.child(field(
                "connection-response",
                "Password or passphrase",
                &response,
                false,
            ));
        }
        body = body.child(
            prompt.child(
                div()
                    .flex()
                    .gap(px(SPACE_2))
                    .child(button("connection-cancel", "Cancel", true, cancel, cx))
                    .child(button(
                        "connection-verify-submit",
                        "Continue",
                        true,
                        verification.submit,
                        cx,
                    )),
            ),
        );
    } else {
        body = body.child(
            div()
                .flex()
                .gap(px(SPACE_2))
                .child(button("connection-cancel", "Cancel", true, cancel, cx))
                .child(button(
                    "connection-submit",
                    if busy { "Adding…" } else { "Add environment" },
                    !busy,
                    submit,
                    cx,
                )),
        );
    }
    body
}

pub(super) fn row(row: ConnectionRow, cx: &App) -> Div {
    let mut body = div()
        .flex()
        .flex_col()
        .min_w_0()
        .gap(px(SPACE_1_5))
        .px(px(SETTINGS_ROW_PAD_X))
        .py(px(SETTINGS_ROW_PAD_Y))
        .child(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .justify_between()
                .gap(px(SPACE_2))
                .child(
                    div()
                        .min_w_0()
                        .child(components::form_label(row.name, INK_STRONG)),
                )
                .child(
                    text(row.status)
                        .flex_shrink_0()
                        .text_color(rgb(if row.connected { RUNNING } else { INK_SUBTLE })),
                ),
        )
        .when(!row.target.is_empty(), |body| body.child(text(row.target)))
        .children(row.detail.map(text));
    if let Some((confirm, cancel)) = row.removal {
        body = body
            .child(text(
                "Remove this environment? Remote agents will keep running.",
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(SPACE_2))
                    .child(button(
                        "connection-remove-cancel",
                        "Cancel",
                        true,
                        cancel,
                        cx,
                    ))
                    .child(button(
                        "connection-remove-confirm",
                        "Remove environment",
                        true,
                        confirm,
                        cx,
                    )),
            );
    } else if !row.actions.is_empty() {
        body = body.child(div().flex().flex_wrap().gap(px(SPACE_2)).children(
            row.actions.into_iter().map(
                |ConnectionAction {
                     id,
                     label,
                     enabled,
                     act,
                 }| button(id, label, enabled, act, cx),
            ),
        ));
    }
    body
}
