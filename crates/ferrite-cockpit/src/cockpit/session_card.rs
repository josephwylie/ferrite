//! The Session controls card, a popup that hangs from a Thread's Composer
//! (`Popup::Session`, ADR 0043). It gets no view (ADR 0041, What does not
//! get a view): the slot holds all it keeps, the Session it is for, and
//! what it shows is the Session's own (its permission modes, MCP servers
//! and background tasks) and why its last control was refused, which the
//! root reads. Here it is drawn from what was read; each control it offers
//! is sent through the root (`request_mode`, `run_session_control`).

use super::*;
use ferrite_core::{ControlKind, McpStatus, SessionControl};
use ferrite_ui::theme::*;

/// The card as it reads, for one Session: a Thread and its generation.
pub(super) struct SessionCard {
    thread: ThreadId,
    generation: u64,
    /// Why the Session refused the last control.
    error: Option<String>,
    modes: Vec<ModeRow>,
    servers: Vec<ServerRow>,
    /// Whether the Session reloads its MCP servers on ask.
    reload: bool,
    tasks: Vec<TaskRow>,
    /// Whether the Session sends its running commands to the background on
    /// ask: its section stands then even with no task.
    background: bool,
}

struct ModeRow {
    value: String,
    label: String,
    checked: bool,
}

struct ServerRow {
    name: String,
    status: McpStatus,
    error: Option<String>,
    /// The Session signs in to it on ask (it waits on a sign-in).
    sign_in: bool,
    /// The sign-in page the provider handed over.
    authorize: Option<String>,
    reconnect: bool,
}

struct TaskRow {
    id: String,
    label: String,
    working: bool,
    /// The Session stops it on ask.
    stop: bool,
}

impl SessionCard {
    /// The card for `open`'s Session `generation`, with `error` its last
    /// refusal; none once another Session runs.
    pub(super) fn of(
        thread: ThreadId,
        generation: u64,
        open: &ThreadView,
        error: Option<&str>,
    ) -> Option<Self> {
        if open.generation() != generation {
            return None;
        }
        let transcript = open.transcript();
        let current = open.permission_mode().map(str::to_owned);
        let modes = open
            .permission_modes()
            .into_iter()
            .map(|mode| ModeRow {
                checked: current.as_deref() == Some(mode.value.as_str()),
                value: mode.value,
                label: mode.label,
            })
            .collect();
        let servers = transcript
            .mcp_servers()
            .iter()
            .map(|server| ServerRow {
                name: server.name.clone(),
                status: server.status,
                error: server.error.clone(),
                sign_in: server.status == McpStatus::NeedsAuth
                    && open.supports_control(ControlKind::LoginMcp),
                authorize: transcript
                    .mcp_authorizations()
                    .get(&server.name)
                    .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
                    .cloned(),
                reconnect: open.supports_control(ControlKind::ReconnectMcp),
            })
            .collect();
        let tasks = transcript
            .progress()
            .background()
            .iter()
            .map(|task| {
                let working = task.status == ferrite_core::progress::TaskStatus::Working;
                TaskRow {
                    id: task.id.clone(),
                    label: task.label.clone(),
                    working,
                    stop: working && open.supports_control(ControlKind::StopTask),
                }
            })
            .collect();
        Some(Self {
            thread,
            generation,
            error: error.map(str::to_owned),
            modes,
            servers,
            reload: open.supports_control(ControlKind::ReloadMcp),
            tasks,
            background: open.supports_control(ControlKind::BackgroundTasks),
        })
    }

    /// A readout in sections, not a stack of buttons: a quiet section head,
    /// then rows — a name at the left, its state and its actions (quiet
    /// mono text controls) at the right.
    pub(super) fn render(self, cx: &mut Context<CockpitView>) -> Stateful<Div> {
        let Self {
            thread,
            generation,
            ..
        } = self;
        let text_action = |id: SharedString, verb: &'static str| {
            ferrite_ui::components::faded_button(
                id,
                gpui::rgba(TRANSPARENT).into(),
                paint::FLOAT_HOVER.into(),
                paint::FLOAT_SEL.into(),
                rgb(INK_SUBTLE).into(),
                cx,
            )
            .h(px(TOKEN_H))
            .px(px(TOKEN_PAD_X))
            .rounded(px(R_CONTROL))
            .child(ferrite_ui::components::text_meta().child(verb))
        };
        // A card-wide verb is a whole float row: one row, the rows' inset,
        // `INK_STRONG`, `FLOAT_HOVER` under the pointer.
        let action_row = |id: SharedString, verb: &'static str| {
            ferrite_ui::components::form_button(id, cx)
                .h(px(FLOAT_ROW_H))
                .mx(px(MENU_ROW_INSET_X))
                .px(px(MENU_ROW_PAD_X))
                .rounded(px(R_CONTROL))
                .child(
                    div()
                        .flex()
                        .w_full()
                        .text_size(px(FS_UI))
                        .line_height(px(LH_UI))
                        .text_color(rgb(INK_STRONG))
                        .child(verb),
                )
        };
        // A section head is the float's section heading (theme WP-E): it
        // carries its own air.
        let head = |title: &'static str| ferrite_ui::menu::section(title, None, None);
        let row = || {
            div()
                .flex()
                .items_center()
                .gap(px(SPACE_2))
                .min_h(px(FLOAT_ROW_H))
                .px(px(FLOAT_PAD_X))
                .min_w_0()
        };
        let mut card = ferrite_ui::menu::float()
            .pb(px(FLOAT_PAD))
            .id("session-controls-card")
            .debug_selector(|| "session-controls-card".into())
            .w(px(SESSION_CARD_W))
            .max_h(px(MENU_MAX_H))
            .overflow_y_scroll();
        let errored = self.error.is_some();
        if let Some(error) = self.error {
            card = card.child(pane::card_error(error).id("session-control-error"));
        }
        let modes_empty = self.modes.is_empty();
        if !modes_empty {
            card = card.child(head("Mode"));
        }
        for (index, mode) in self.modes.into_iter().enumerate() {
            let value = mode.value;
            card = card.child(
                ferrite_ui::components::button(SharedString::from(format!(
                    "permission-mode-{index}"
                )))
                .debug_selector(move || format!("permission-mode-{index}"))
                .tab_stop(true)
                .h(px(FLOAT_ROW_H))
                .mx(px(MENU_ROW_INSET_X))
                .px(px(MENU_ROW_PAD_X))
                .rounded(px(R_CONTROL))
                .accessibility_label(mode.label.clone())
                .child(
                    div()
                        .flex()
                        .w_full()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                // The shared button is kit-xsmall; a menu
                                // row reads at the UI size like every menu.
                                .text_size(px(FS_UI))
                                .line_height(px(LH_UI))
                                .text_color(rgb(INK_STRONG))
                                .child(mode.label),
                        )
                        .children(
                            mode.checked
                                .then(|| ferrite_ui::components::glyph("\u{2713}", MENU_CHECK_INK)),
                        ),
                )
                .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                    view.request_mode(thread, generation, value.clone(), window, cx);
                    cx.notify();
                })),
            );
        }
        // With no servers the note alone says so; a `MCP servers` head over
        // `No MCP servers reported` would say it twice.
        if self.servers.is_empty() {
            card = card.child(
                ferrite_ui::menu::note("No MCP servers reported")
                    .when(errored || !modes_empty, |note| note.mt(px(FLOAT_SECTION_GAP))),
            );
        } else {
            card = card.child(head("MCP servers"));
        }
        for (index, server) in self.servers.into_iter().enumerate() {
            // (selector key, the word drawn, the dot's ink). Connecting is
            // work in progress, not a stop: only a sign-in waits on you.
            let (status, word, dot) = match server.status {
                McpStatus::Connected => ("connected", "connected", RUNNING),
                McpStatus::Connecting => ("connecting", "connecting", RUNNING),
                McpStatus::NeedsAuth => ("needs-auth", "needs sign-in", ATTENTION),
                McpStatus::Failed => ("failed", "failed", BLOCKED),
                McpStatus::Disabled => ("disabled", "disabled", TEXT_FAINT),
                McpStatus::Unknown => ("unknown", "unknown", TEXT_FAINT),
            };
            let connected = server.status == McpStatus::Connected;
            let mut line = row()
                .group(SharedString::from(format!("mcp-row-{index}")))
                .child(ferrite_ui::components::status_dot(dot))
                .child(
                    div()
                        .id(("mcp-server-name", index))
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(INK_STRONG))
                        // A long server name gives way to its state and
                        // actions; the whole of it is one hover away.
                        .tooltip(ferrite_ui::menu::tooltip(server.name.clone()))
                        .child(server.name.clone()),
                )
                // A connected server's green dot says so; the word appears
                // only when the state needs reading.
                .when(!connected, |line| {
                    line.child(
                        div()
                            .debug_selector(move || format!("mcp-status-{index}-{status}"))
                            .flex_shrink_0()
                            .text_size(px(FS_UI))
                            .text_color(rgb(if server.status == McpStatus::Failed {
                                BLOCKED
                            } else {
                                INK_SUBTLE
                            }))
                            .child(word),
                    )
                });
            if server.sign_in {
                let name = server.name.clone();
                line = line.child(
                    text_action(SharedString::from(format!("mcp-login-{index}")), "Sign in")
                        .debug_selector(move || format!("mcp-login-{index}"))
                        .tab_stop(true)
                        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                            if has_server(view, thread, generation, &name, cx) {
                                view.run_session_control(
                                    thread,
                                    generation,
                                    SessionControl::LoginMcp {
                                        server: name.clone(),
                                    },
                                    cx,
                                );
                            }
                            cx.notify();
                        })),
                );
            }
            if let Some(url) = server.authorize {
                line = line.child(
                    text_action(
                        SharedString::from(format!("mcp-authorize-{index}")),
                        "Open sign-in page",
                    )
                    .debug_selector(move || format!("mcp-authorize-{index}"))
                    .tab_stop(true)
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url))),
                );
            }
            if server.reconnect {
                let name = server.name.clone();
                line = line.child(
                    text_action(
                        SharedString::from(format!("mcp-reconnect-{index}")),
                        "Reconnect",
                    )
                    .debug_selector(move || format!("mcp-reconnect-{index}"))
                    // A healthy server's reconnect waits under the pointer,
                    // so the name keeps the row; a troubled one shows it.
                    .when(connected, |action| {
                        action
                            .invisible()
                            .group_hover(format!("mcp-row-{index}"), |style| style.visible())
                    })
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        if has_server(view, thread, generation, &name, cx) {
                            view.run_session_control(
                                thread,
                                generation,
                                SessionControl::ReconnectMcp {
                                    server: name.clone(),
                                },
                                cx,
                            );
                        }
                        cx.notify();
                    })),
                );
            }
            card = card.child(line);
            if let Some(error) = server.error {
                card = card.child(
                    div()
                        .pl(px(FLOAT_PAD_X + STATUS_DOT + CH))
                        .pr(px(FLOAT_PAD_X))
                        .text_size(px(FS_UI))
                        .text_color(rgb(INK_SUBTLE))
                        .whitespace_normal()
                        .child(error),
                );
            }
        }
        if self.reload {
            card = card.child(
                action_row("mcp-reload".into(), "Reload MCP servers")
                    .debug_selector(|| "mcp-reload".into())
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.run_session_control(thread, generation, SessionControl::ReloadMcp, cx);
                        cx.notify();
                    })),
            );
        }
        if !self.tasks.is_empty() || self.background {
            card = card.child(head("Background tasks"));
        }
        for (index, task) in self.tasks.into_iter().enumerate() {
            let mut line = row()
                .child(ferrite_ui::components::status_dot(if task.working {
                    RUNNING
                } else {
                    TEXT_FAINT
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(INK_STRONG))
                        .child(task.label),
                );
            if task.stop {
                let id = task.id;
                line = line.child(
                    text_action(
                        SharedString::from(format!("background-stop-{index}")),
                        "Stop",
                    )
                    .debug_selector(move || format!("background-stop-{index}"))
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        let exists = view.cockpit.read(cx).thread(thread).is_some_and(|open| {
                            open.generation() == generation
                                && open.transcript().progress().background().iter().any(|task| {
                                    task.id == id
                                        && task.status
                                            == ferrite_core::progress::TaskStatus::Working
                                })
                        });
                        if exists {
                            view.run_session_control(
                                thread,
                                generation,
                                SessionControl::StopTask { id: id.clone() },
                                cx,
                            );
                        }
                        cx.notify();
                    })),
                );
            }
            card = card.child(line);
        }
        if self.background {
            card = card.child(
                action_row("background-all".into(), "Run tasks in background")
                    .debug_selector(|| "background-all".into())
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.run_session_control(
                            thread,
                            generation,
                            SessionControl::BackgroundTasks,
                            cx,
                        );
                        cx.notify();
                    })),
            );
        }
        card
    }
}

/// Whether `thread`'s Session `generation` still runs and still reports
/// the MCP server `name`, as a press on its row finds it.
fn has_server(view: &CockpitView, thread: ThreadId, generation: u64, name: &str, cx: &gpui::App) -> bool {
    view.cockpit.read(cx).thread(thread).is_some_and(|open| {
        open.generation() == generation
            && open
                .transcript()
                .mcp_servers()
                .iter()
                .any(|server| server.name == name)
    })
}
