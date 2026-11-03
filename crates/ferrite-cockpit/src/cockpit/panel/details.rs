//! The Panel's Details tab: what a Thread works with, top to bottom, each
//! under the Panel's one section head (`panel_section_head`).
//!
//! - **Environment**: the machine it runs on and its folder (a click shows
//!   the folder in the system's file browser).
//! - **Context**: what fills its context window, by category, with its
//!   account windows (`pane::context_usage`, whose heading reads as a
//!   section head and opens the category table).
//! - **Outputs**: the files it created — a diff whose every hunk starts
//!   from nothing (`@@ -0,0 …`), first seen for its path — each a click
//!   from the preview.
//! - **Connectors**: its MCP servers and how each stands.
//!
//! The work it left running (a dev server, a watch) is not listed here: the
//! chips over the Composer hold that fact (theme rule 7).
//!
//! A section with nothing to list is left out, but for Environment and
//! Context, which always have something to say.

use super::*;
use ferrite_core::transcript::{Body, Transcript};
use ferrite_core::{McpServer, McpStatus};

impl CockpitView {
    /// The Details tab's body (see the module doc).
    pub(super) fn panel_details(
        &self,
        thread: ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let Some(open) = self.cockpit.thread(thread) else {
            return div().into_any_element();
        };
        let transcript = open.transcript();
        let cwd = self.thread_path(thread);
        let mut body = div()
            .debug_selector(|| "panel-details".into())
            .flex()
            .flex_col()
            .pb(px(SPACE_2));

        // Environment.
        let machine = self
            .cockpit
            .machine()
            .map_or_else(|| "This computer".to_string(), |bridge| bridge.name().to_string());
        body = body
            .child(panel_section_head("panel-environment-head", "Environment", None))
            .child(detail_row("panel-environment-machine", icons::LAPTOP, machine, None));
        if let Some(cwd) = cwd.clone() {
            let shown = home_relative(&cwd);
            body = body.child(
                detail_row("panel-environment-folder", icons::FOLDER, shown, None)
                    .tooltip(ferrite_ui::menu::tooltip("Show in its folder"))
                    .on_click(move |_, window, cx| {
                        ferrite_pane::preview::open_original(&cwd, window, cx);
                    }),
            );
        }

        // Context.
        body = body.child(section_gap());
        body = match transcript.usage() {
            Some(usage) => {
                // The tab scrolls inside the body, under the head.
                let room =
                    f32::from(window.viewport_size().height) - WIN_CHROME_H - STATUS_BAR_H;
                let error = self
                    .session_control_error
                    .as_ref()
                    .filter(|(shown, generation, _)| {
                        *shown == thread && *generation == open.generation()
                    })
                    .map(|(_, _, error)| error.clone());
                body.child(
                    div()
                        .debug_selector(|| "panel-context".into())
                        .child(pane::context_usage(
                            usage,
                            self.cockpit.account_limits(open.provider()),
                            transcript.context_details(),
                            transcript.usage_details(),
                            transcript.last_cost(),
                            self.context_usage_expanded,
                            room,
                            cx.listener(|view, _: &ClickEvent, _, cx| {
                                view.context_usage_expanded = !view.context_usage_expanded;
                                cx.notify();
                            }),
                        )),
                )
                .children(error.map(|error| pane::card_error(error).id("session-control-error")))
            }
            None => body
                .child(panel_section_head("panel-context-head", "Context", None))
                .child(
                    quiet_line("panel-no-context")
                        .child("Shows once the provider reports what fills the window"),
                ),
        };

        // Outputs.
        let outputs = created_files(transcript);
        if !outputs.is_empty() {
            body = body.child(section_gap()).child(panel_section_head(
                "panel-outputs-head",
                "Outputs",
                Some(outputs.len().to_string()),
            ));
            for (index, path) in outputs.into_iter().enumerate() {
                let raw = std::path::PathBuf::from(&path);
                let full = match &cwd {
                    Some(cwd) if !raw.is_absolute() => cwd.join(&raw),
                    _ => raw,
                };
                let name = full
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let dir = full
                    .parent()
                    .map(|dir| {
                        cwd.as_deref()
                            .and_then(|cwd| dir.strip_prefix(cwd).ok())
                            .unwrap_or(dir)
                            .display()
                            .to_string()
                    })
                    .filter(|dir| !dir.is_empty());
                body = body.child(
                    detail_row(("panel-output", index), icons::FILE, name, dir)
                        .tooltip(ferrite_ui::menu::tooltip("Open in the preview"))
                        .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                            view.open_preview(full.clone(), None, window, cx);
                        })),
                );
            }
        }

        // Connectors.
        let servers = transcript.mcp_servers();
        if !servers.is_empty() {
            let connected = servers
                .iter()
                .filter(|server| server.status == McpStatus::Connected)
                .count();
            body = body.child(section_gap()).child(panel_section_head(
                "panel-connectors-head",
                "Connectors",
                Some(format!("{connected} connected")),
            ));
            for (index, server) in servers.iter().enumerate() {
                body = body.child(connector_row(index, server));
            }
        }

        body.into_any_element()
    }
}

/// The room between one Details section and the next's head.
fn section_gap() -> Div {
    div().flex_shrink_0().h(px(ferrite_ui::theme::SPACE_2))
}

/// One line under a Details section (`PANEL_SECTION_H`): its icon on the
/// rows' icon column, its words on their text column, a second word dim
/// after them (a file's folder) when there is one. It answers the pointer
/// as a row when the caller gives it a click.
fn detail_row(
    id: impl Into<gpui::ElementId>,
    glyph: &'static str,
    label: impl Into<SharedString>,
    meta: Option<String>,
) -> gpui::Stateful<Div> {
    use ferrite_ui::theme::*;
    let id = id.into();
    let key = ferrite_ui::pointer::hover_key(&id);
    let selector = format!("{id}");
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_2))
        .h(px(PANEL_SECTION_H))
        .pl(px(PANEL_ROW_TEXT_X))
        .pr(px(PANEL_ROW_ICON_X))
        .rounded(px(R_CONTROL))
        .hover_row(key)
        .press_row()
        .child(
            div()
                .absolute()
                .left(px(PANEL_ROW_ICON_X))
                .top(px((PANEL_SECTION_H - PANEL_ROW_ICON) / 2.))
                .child(icon(glyph, PANEL_ROW_ICON, INK)),
        )
        .child(
            div()
                .flex_shrink_0()
                .max_w_full()
                .min_w_0()
                .truncate()
                .text_color(rgb(INK))
                .child(label.into()),
        )
        .children(meta.map(|meta| {
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(FS_META))
                .text_color(rgb(INK_SUBTLE))
                .child(SharedString::from(meta))
        }))
}

/// An MCP server's line: its name, and how it stands in the lexicon's
/// lowercase words — coloured only when it needs the operator (`needs
/// auth`) or failed.
fn connector_row(index: usize, server: &McpServer) -> Div {
    use ferrite_ui::theme::*;
    let (word, ink) = match server.status {
        McpStatus::Connected => ("connected", INK_SUBTLE),
        McpStatus::Connecting => ("connecting", INK_SUBTLE),
        McpStatus::NeedsAuth => ("needs auth", ATTENTION),
        McpStatus::Failed => (words::FAILED, BLOCKED),
        McpStatus::Disabled => ("off", INK_SUBTLE),
        McpStatus::Unknown => ("", INK_SUBTLE),
    };
    div()
        .debug_selector(move || format!("panel-connector-{index}"))
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_2))
        .h(px(PANEL_SECTION_H))
        .pl(px(PANEL_ROW_TEXT_X))
        .pr(px(PANEL_ROW_ICON_X))
        .child(
            div()
                .absolute()
                .left(px(PANEL_ROW_ICON_X))
                .top(px((PANEL_SECTION_H - PANEL_ROW_ICON) / 2.))
                .child(icon(icons::PLUG, PANEL_ROW_ICON, INK)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(rgb(INK))
                .child(SharedString::from(server.name.clone())),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(FS_META))
                .text_color(rgb(ink))
                .child(word),
        )
}

/// A quiet line on the rows' text column.
fn quiet_line(selector: &'static str) -> Div {
    use ferrite_ui::theme::*;
    div()
        .debug_selector(move || selector.into())
        .flex()
        .items_center()
        .h(px(PANEL_SECTION_H))
        .px(px(PANEL_ROW_ICON_X))
        .text_size(px(FS_META))
        .text_color(rgb(INK_SUBTLE))
}

/// The files a Thread created, in the order it created them: a path whose
/// first diff starts from nothing — every hunk `@@ -0,0 …`, the patch a new
/// file makes. A file it only edited is the Changes tab's, not an output.
pub(crate) fn created_files(transcript: &Transcript) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut created = Vec::new();
    for block in transcript.blocks() {
        let Body::Tool(tool) = &block.body else {
            continue;
        };
        for diff in &tool.diffs {
            if !seen.insert(diff.path.clone()) {
                continue;
            }
            let fresh = !diff.hunks.is_empty()
                && diff
                    .hunks
                    .iter()
                    .all(|hunk| hunk.old_start == 0 && hunk.old_lines == 0);
            if fresh {
                created.push(diff.path.clone());
            }
        }
    }
    created
}
