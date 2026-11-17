use super::*;
use ferrite_ui::frame::settings::{
    ConnectionForm, ConnectionMode, ConnectionRow, ConnectionSuggestions, WindowAct,
};
use gpui::component::{switch::Switch, ActiveTheme, ElementExt};

fn ink(cx: &App) -> u32 {
    if cx.theme().is_dark() {
        0xf4f4f5
    } else {
        0x18181b
    }
}
fn muted(cx: &App) -> u32 {
    if cx.theme().is_dark() {
        0xa1a1aa
    } else {
        0x71717a
    }
}
fn edge(cx: &App) -> u32 {
    if cx.theme().is_dark() {
        0x27272a
    } else {
        0xe4e4e7
    }
}
fn surface(cx: &App) -> u32 {
    if cx.theme().is_dark() {
        0x09090b
    } else {
        0xffffff
    }
}
const PRIMARY: u32 = 0x4e6ce8;

fn text(value: impl Into<SharedString>, size: f32, color: u32) -> Div {
    div()
        .min_w_0()
        .text_size(px(size))
        .line_height(px(size * 1.5))
        .text_color(rgb(color))
        .child(value.into())
}

fn button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    enabled: bool,
    act: WindowAct,
    cx: &App,
) -> Button {
    let id = id.into();
    let label: SharedString = label.into();
    components::faded_button(
        id.clone(),
        rgb(surface(cx)).into(),
        paint::HOVER.into(),
        paint::PRESS.into(),
        rgb(ink(cx)).into(),
        cx,
    )
    .debug_selector(move || id.to_string())
    .accessibility_label(label)
    .disabled(!enabled)
    .tab_stop(enabled)
    .h(px(32.))
    .px(px(12.))
    .rounded(px(8.))
    .border_1()
    .border_color(rgb(edge(cx)))
    .when(!enabled, |button| button.opacity(0.5).cursor_default())
    .on_click(move |_, window, cx| {
        cx.stop_propagation();
        if enabled {
            act(window, cx);
        }
    })
}

fn field(
    id: &'static str,
    label: &'static str,
    input: &Entity<InputState>,
    disabled: bool,
    cx: &App,
) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(px(6.))
        .child(text(label, 12., ink(cx)).font_weight(gpui::FontWeight::MEDIUM))
        .child(
            div().debug_selector(move || id.into()).child(
                Input::new(input)
                    .h(px(32.))
                    .text_size(px(14.))
                    .rounded(px(8.))
                    .bg(rgb(if cx.theme().is_dark() {
                        0x101012
                    } else {
                        0xffffff
                    }))
                    .border_color(rgb(edge(cx)))
                    .aria_label(label)
                    .disabled(disabled),
            ),
        )
}

fn mode_card(
    mode: ConnectionMode,
    selected: bool,
    busy: bool,
    compact: bool,
    choose: Rc<dyn Fn(ConnectionMode, &mut Window, &mut App)>,
    cx: &App,
) -> Button {
    let (id, title, description, glyph) = match mode {
        ConnectionMode::Remote => (
            "connection-mode-remote",
            "Remote link",
            "Enter a backend host and pairing code.",
            icons::CODE,
        ),
        ConnectionMode::Ssh => (
            "connection-mode-ssh",
            "SSH",
            "Use local SSH config, agent, and tunnels for the backend.",
            icons::TERMINAL,
        ),
    };
    components::faded_button(
        id,
        rgb(if selected && cx.theme().is_dark() {
            0x0b1020
        } else {
            surface(cx)
        })
        .into(),
        paint::HOVER.into(),
        paint::PRESS.into(),
        rgb(ink(cx)).into(),
        cx,
    )
    .debug_selector(move || id.into())
    .accessibility_label(title)
    .selected(selected)
    .disabled(busy)
    .tab_stop(!busy)
    .flex_1()
    .min_w_0()
    .h(px(if compact { 44. } else { 96. }))
    .p(px(if compact { 8. } else { 16. }))
    .items_start()
    .justify_start()
    .rounded(px(8.))
    .border_1()
    .border_color(rgb(if selected { 0x344881 } else { edge(cx) }))
    .child(
        div()
            .id((id, 0usize))
            .flex()
            .items_start()
            .when(compact, |row| row.items_center())
            .when(compact, |row| row.tooltip(ferrite_ui::menu::tooltip(description)))
            .w_full()
            .h_full()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .size(px(if compact { 24. } else { 32. }))
                    .mt(px(if compact { 0. } else { 2. }))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(if selected { 0x293961 } else { edge(cx) }))
                    .child(icon(glyph, 16., if selected { PRIMARY } else { muted(cx) })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .flex_1()
                    .gap(px(4.))
                    .child(text(title, 14., ink(cx)).font_weight(gpui::FontWeight::MEDIUM))
                    .when(!compact, |column| {
                        column.child(text(description, 12., muted(cx)).whitespace_normal())
                    }),
            ),
    )
    .on_click(move |_, window, cx| {
        cx.stop_propagation();
        if !busy {
            choose(mode, window, cx);
        }
    })
}

fn show_suggestions(
    list: ConnectionSuggestions,
    choose: Rc<dyn Fn(SharedString, &mut Window, &mut App)>,
    anchor: gpui::Bounds<gpui::Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let below = (window.viewport_size().height - anchor.bottom() - px(20.)).max(px(0.));
    let above = (anchor.top() - px(20.)).max(px(0.));
    let upwards = below < px(96.) && above > below;
    let height = if upwards { above } else { below }.min(px(296.));
    let highlighted = list.highlighted;
    let message_height = if list.message.is_some() {
        px(32.)
    } else {
        px(0.)
    };
    let mut popup = div()
        .id("connection-host-suggestions")
        .debug_selector(|| "connection-host-suggestions".into())
        .flex()
        .flex_col()
        .w(anchor.size.width)
        .rounded(px(8.))
        .bg(rgb(surface(cx)))
        .border_1()
        .border_color(rgb(edge(cx)))
        .shadow(components::elevation(components::Elevation::Sheet))
        .p(px(4.))
        .children(list.message.map(|message| {
            text(message, 12., muted(cx))
                .h(px(32.))
                .px(px(8.))
                .py(px(6.))
                .truncate()
        }))
        .child(
            div()
                .id("connection-host-suggestions-scroll")
                .max_h((height - px(8.) - message_height).max(px(32.)))
                .overflow_y_scroll()
                .track_scroll(&list.scroll)
                .children(list.rows.into_iter().enumerate().map(|(at, host)| {
                    let choose = choose.clone();
                    div()
                        .id(("connection-host-suggestion", at))
                        .debug_selector(move || format!("connection-host-suggestion-{at}"))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .h(px(32.))
                        .px(px(8.))
                        .rounded(px(4.))
                        .cursor_pointer()
                        .when(highlighted == Some(at), |row| row.bg(paint::HOVER))
                        .hover(|row| row.bg(paint::HOVER))
                        .child(
                            text(host.label, 14., ink(cx))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .truncate(),
                        )
                        .child(text(host.address, 12., muted(cx)).flex_1().truncate())
                        .when(at < 9, |row| {
                            row.child(text(format!("⌘{}", at + 1), 12., muted(cx)))
                        })
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            cx.stop_propagation();
                            choose(host.value.clone(), window, cx);
                        })
                })),
        )
        .into_any_element();
    let size = popup.layout_as_root(gpui::AvailableSpace::min_size(), window, cx);
    let top = if upwards {
        anchor.top() - px(4.) - size.height
    } else {
        anchor.bottom() + px(4.)
    };
    window.defer_draw(popup, gpui::point(anchor.left(), top), 5, None);
}

pub(super) fn form(form: ConnectionForm, window: &Window, cx: &App) -> AnyElement {
    let ConnectionForm::Editing {
        fields,
        mode,
        focus,
        suggestions: list,
        choose_mode,
        open_suggestions,
        choose_host,
        progress,
        error,
        warning,
        verification,
        submit,
        cancel,
    } = form
    else {
        return div().into_any_element();
    };
    let busy = progress.is_some();
    let compact = window.viewport_size().height < px(600.);
    let host = field(
        "connection-host",
        "SSH host or alias",
        &fields.host,
        busy,
        cx,
    )
    .relative()
    .capture_any_mouse_down(move |event, window, cx| {
        if event.button == MouseButton::Left {
            open_suggestions(window, cx);
        }
    })
    .when_some(list, |host, list| {
        host.on_prepaint(move |bounds, window, cx| {
            show_suggestions(list, choose_host, bounds, window, cx);
        })
    });
    let fields = match mode {
        ConnectionMode::Remote => div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(field(
                        "connection-remote-host",
                        "Host",
                        &fields.remote_host,
                        busy,
                        cx,
                    ))
                    .child(div().w(px(160.)).flex_shrink_0().child(field(
                        "connection-pairing-code",
                        "Pairing code",
                        &fields.pairing_code,
                        busy,
                        cx,
                    ))),
            )
            .child(text(
                "Paste a full pairing URL here to fill both fields automatically.",
                12.,
                muted(cx),
            )),
        ConnectionMode::Ssh => div()
            .flex()
            .flex_col()
            .gap(px(if compact { 8. } else { 12. }))
            .child(host)
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(field(
                        "connection-username",
                        "Username",
                        &fields.username,
                        busy,
                        cx,
                    ))
                    .child(div().w(px(112.)).flex_shrink_0().child(field(
                        "connection-port",
                        "Port",
                        &fields.port,
                        busy,
                        cx,
                    ))),
            ),
    };
    let submit_button = || {
        button(
            "connection-submit",
            "Add environment",
            !busy,
            submit.clone(),
            cx,
        )
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .child(icon(icons::PLUS, 14., muted(cx)))
                .child(text(
                    if busy { "Adding…" } else { "Add environment" },
                    14.,
                    ink(cx),
                )),
        )
    };
    let close = cancel.clone();
    let card = div()
        .id("connection-dialog")
        .debug_selector(|| "connection-dialog".into())
        .relative()
        .flex()
        .flex_col()
        .w(px(768.))
        .max_w_full()
        .max_h(gpui::relative(if compact { 0.94 } else { 0.8 }))
        .rounded(px(16.))
        .bg(rgb(surface(cx)))
        .border_1()
        .border_color(rgb(edge(cx)))
        .shadow(components::elevation(components::Elevation::Sheet))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .px(px(if compact { 16. } else { 24. }))
                .pt(px(if compact { 12. } else { 24. }))
                .pb(px(if compact { 8. } else { 12. }))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    text("Add Environment", if compact { 18. } else { 20. }, ink(cx))
                        .font_weight(gpui::FontWeight::SEMIBOLD),
                )
                .when(!compact, |header| {
                    header.child(text(
                        "Pair another environment to this client.",
                        14.,
                        muted(cx),
                    ))
                }),
        )
        .child(
            div().absolute().top(px(8.)).right(px(8.)).child(
                button("connection-close", "Close Add Environment", true, close, cx)
                    .size(px(32.))
                    .p_0()
                    .border_0()
                    .child(icon(icons::CLOSE, 16., muted(cx))),
            ),
        )
        .child(
            div()
                .id("connection-dialog-body")
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .min_h_0()
                .px(px(if compact { 16. } else { 24. }))
                .pt(px(4.))
                .pb(px(if compact { 0. } else { 24. }))
                .gap(px(if compact { 10. } else { 16. }))
                .child(
                    div()
                        .flex()
                        .gap(px(12.))
                        .child(mode_card(
                            ConnectionMode::Remote,
                            mode == ConnectionMode::Remote,
                            busy,
                            compact,
                            choose_mode.clone(),
                            cx,
                        ))
                        .child(mode_card(
                            ConnectionMode::Ssh,
                            mode == ConnectionMode::Ssh,
                            busy,
                            compact,
                            choose_mode,
                            cx,
                        )),
                )
                .child(fields)
                .children(error.map(|error| {
                    text(error, 12., RED)
                        .debug_selector(|| "connection-error".into())
                        .when(mode == ConnectionMode::Ssh, |error| {
                            error
                                .p(px(12.))
                                .rounded(px(8.))
                                .border_1()
                                .border_color(rgb(RED))
                        })
                }))
                .children(warning.map(|warning| {
                    text(warning, 12., muted(cx)).debug_selector(|| "connection-warning".into())
                }))
                .children(progress.map(|progress| {
                    text(progress, 12., muted(cx)).debug_selector(|| "connection-progress".into())
                }))
                .when(!compact, |body| body.child(submit_button())),
        )
        .when(compact, |card| {
            card.child(
                div()
                    .flex_shrink_0()
                    .px(px(16.))
                    .pt(px(8.))
                    .pb(px(12.))
                    .child(submit_button()),
            )
        });
    let backdrop_cancel = cancel.clone();
    let mut overlay = div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .p(px(16.))
        .id("connection-backdrop")
        .debug_selector(|| "connection-backdrop".into())
        .track_focus(&focus)
        .tab_group()
        .bg(gpui::rgba(if cx.theme().is_dark() {
            0x000000a3
        } else {
            0xffffff99
        }))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            backdrop_cancel(window, cx);
        })
        .child(card);
    if let Some(verification) = verification {
        let submit = verification.submit.clone();
        let confirm = verification.response.is_none();
        let dialog = div()
            .id("connection-verification")
            .debug_selector(|| "connection-verification".into())
            .when_some(verification.focus.as_ref(), |dialog, focus| {
                dialog.track_focus(focus)
            })
            .on_key_down(move |event, window, cx| {
                if confirm
                    && event.keystroke.key == "enter"
                    && !event.keystroke.modifiers.modified()
                {
                    cx.stop_propagation();
                    submit(window, cx);
                }
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .w(px(448.))
            .max_w_full()
            .p(px(24.))
            .rounded(px(16.))
            .bg(rgb(surface(cx)))
            .border_1()
            .border_color(rgb(edge(cx)))
            .shadow(components::elevation(components::Elevation::Sheet))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                text("SSH verification required", 20., ink(cx))
                    .font_weight(gpui::FontWeight::SEMIBOLD),
            )
            .child(text(verification.target, 14., muted(cx)))
            .child(
                div()
                    .id("connection-prompt-scroll")
                    .debug_selector(|| "connection-prompt".into())
                    .max_h(px(96.))
                    .overflow_y_scroll()
                    .child(text(verification.prompt, 14., ink(cx))),
            )
            .children(verification.response.map(|response| {
                field(
                    "connection-response",
                    "Password or passphrase",
                    &response,
                    false,
                    cx,
                )
            }))
            .child(text(verification.expires, 12., muted(cx)))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button("connection-cancel", "Cancel", true, cancel, cx).child(text(
                            "Cancel",
                            14.,
                            ink(cx),
                        )),
                    )
                    .child(
                        button(
                            "connection-verify-submit",
                            "Continue",
                            true,
                            verification.submit,
                            cx,
                        )
                        .child(text("Continue", 14., ink(cx))),
                    ),
            );
        overlay = overlay.child(
            div()
                .absolute()
                .inset_0()
                .p(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui::rgba(0x000000a3))
                .child(dialog),
        );
    }
    overlay.into_any_element()
}

pub(super) fn group(label: Option<SharedString>, group: &Group, cx: &App) -> Div {
    let form = group.rows.iter().find_map(|row| match &row.control {
        Control::ConnectionForm(form) => Some(form),
        _ => None,
    });
    let rows: Vec<_> = group
        .rows
        .iter()
        .filter_map(|row| match &row.control {
            Control::ConnectionRow(row) => Some(row.clone()),
            _ => None,
        })
        .collect();
    let remote = form.is_some();
    let mut header = div()
        .flex()
        .items_center()
        .justify_between()
        .min_h(px(28.))
        .px(px(16.))
        .child(
            text(label.unwrap_or_default(), 16., ink(cx)).font_weight(gpui::FontWeight::SEMIBOLD),
        );
    if let Some(ConnectionForm::Closed { open, .. }) = form {
        header = header.child(
            button("connection-add", "Add", true, open.clone(), cx)
                .h(px(24.))
                .border_0()
                .px(px(4.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .child(icon(icons::PLUS, 12., muted(cx)))
                        .child(text("Add", 12., muted(cx))),
                ),
        );
    } else if form.is_some() {
        header = header.child(text("+ Add", 12., muted(cx)));
    }
    let empty = rows.is_empty();
    let content = div()
        .flex()
        .flex_col()
        .rounded(px(12.))
        .border_1()
        .border_color(rgb(edge(cx)))
        .bg(rgb(surface(cx)))
        .when(remote && empty, |body| {
            body.child(
                div()
                    .debug_selector(|| "connections-empty".into())
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .min_h(px(208.))
                    .px(px(16.))
                    .gap(px(8.))
                    .child(icon(icons::GLOBE, 24., muted(cx)))
                    .child(
                        text("No saved remote environments", 14., ink(cx))
                            .font_weight(gpui::FontWeight::MEDIUM),
                    )
                    .child(text(
                        "Pair a remote backend to make it available in this client.",
                        12.,
                        muted(cx),
                    )),
            )
        })
        .children(rows.into_iter().enumerate().map(|(at, connection)| {
            row(connection, cx).when(at > 0, |row| row.border_t_1().border_color(rgb(edge(cx))))
        }));
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(header)
        .child(content)
        .children(group.note.clone().map(|note| text(note, 12., muted(cx))))
        .children(
            form.and_then(|form| match form {
                ConnectionForm::Closed { error, .. } => error.clone(),
                _ => None,
            })
            .map(|error| text(error, 12., RED).debug_selector(|| "connection-error".into())),
        )
}

pub(super) fn row(row: ConnectionRow, cx: &App) -> Div {
    let enabled = row.enabled;
    let toggle = row
        .actions
        .iter()
        .find(|action| action.id.starts_with("connection-toggle-"))
        .cloned();
    let menu_actions: Vec<_> = row
        .actions
        .iter()
        .filter(|action| !action.id.starts_with("connection-toggle-"))
        .cloned()
        .collect();
    let menu_id: SharedString = row
        .actions
        .first()
        .and_then(|action| action.id.rsplit('-').next())
        .map(|index| format!("connection-menu-{index}"))
        .unwrap_or_else(|| "connection-menu-local".into())
        .into();
    let mut actions = div().flex().items_center().flex_shrink_0().gap(px(8.));
    if let Some(toggle) = toggle {
        let id = toggle.id.clone();
        actions = actions.child(
            div().debug_selector(move || id.to_string()).child(
                Switch::new(toggle.id)
                    .checked(enabled)
                    .disabled(!toggle.enabled)
                    .small()
                    .accessibility_label(format!("Enable {}", row.name))
                    .on_click(move |_, window, cx| (toggle.act)(window, cx)),
            ),
        );
    }
    if !menu_actions.is_empty() {
        let selector = menu_id.clone();
        actions = actions.child(
            components::form_button(menu_id, cx)
                .debug_selector(move || selector.to_string())
                .accessibility_label("Environment actions")
                .size(px(28.))
                .p_0()
                .child(icon(icons::ELLIPSIS, 16., muted(cx)))
                .dropdown_menu_with_anchor(gpui::Anchor::TopRight, move |menu, _, _| {
                    menu_actions
                        .iter()
                        .fold(menu.min_w(px(160.)), |menu, action| {
                            let action = action.clone();
                            let id = action.id.clone();
                            let label = action.label.clone();
                            menu.item(
                                PopupMenuItem::element(move |_, cx| {
                                    text(label.clone(), 14., ink(cx))
                                        .px(px(8.))
                                        .py(px(6.))
                                        .debug_selector({
                                            let id = id.clone();
                                            move || id.to_string()
                                        })
                                })
                                .disabled(!action.enabled)
                                .on_click(move |_, window, cx| (action.act)(window, cx)),
                            )
                        })
                }),
        );
    }
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .px(px(16.))
        .py(px(10.))
        .gap(px(8.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .when(!row.target.is_empty(), |row| {
                    row.child(icon(icons::LAPTOP, 16., muted(cx)))
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .flex_1()
                        .when(!enabled && !row.target.is_empty(), |identity| {
                            identity.opacity(0.5)
                        })
                        .child(text(row.name, 14., ink(cx)).font_weight(gpui::FontWeight::MEDIUM))
                        .child(text(
                            if row.target.is_empty() {
                                row.status.to_string()
                            } else {
                                format!("{} · {}", row.target, row.status)
                            },
                            12.,
                            muted(cx),
                        ))
                        .children(row.detail.map(|detail| text(detail, 12., muted(cx)))),
                )
                .child(actions),
        )
        .children(row.removal.map(|(confirm, cancel)| {
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(text(
                    "Remove this environment? Remote agents will keep running.",
                    12.,
                    muted(cx),
                ))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(
                            button("connection-remove-cancel", "Cancel", true, cancel, cx)
                                .child(text("Cancel", 14., ink(cx))),
                        )
                        .child(
                            button(
                                "connection-remove-confirm",
                                "Remove environment",
                                true,
                                confirm,
                                cx,
                            )
                            .child(text(
                                "Remove environment",
                                14.,
                                ink(cx),
                            )),
                        ),
                )
        }))
}
