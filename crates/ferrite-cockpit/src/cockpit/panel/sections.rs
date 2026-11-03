//! The Panel's one scroll of sections, Claude's: what a Thread works with,
//! top to bottom, each under the Panel's one section head
//! (`panel_section_head`) and split from the next by a hairline.
//!
//! - **Environment**: the machine it runs on, its folder (a click shows the
//!   folder in the system's file browser), and its branch, with where it
//!   branched from and the checkout's drift and dirt when git has said.
//! - **Changes**: what it edited, and its pull request once one exists
//!   (`changes`); left out with neither.
//! - **Context**: what fills its context window, by category, and the
//!   lists behind the categories (`pane::context_usage`); left out until
//!   the provider reports it. The one place context is read: the
//!   Composer's ring reads plan usage.
//! - **Connectors**: `N connected` at its head, which folds its MCP servers
//!   open, each with how it stands; left out with none.
//!
//! The work it left running (a dev server, a watch) is not listed here: the
//! chips over the Composer hold that fact (theme rule 7).

use super::*;
use ferrite_core::{McpServer, McpStatus};

impl CockpitView {
    /// The sections (see the module doc).
    pub(super) fn panel_sections(
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
        // A section with nothing to say is not drawn (Claude's): Changes
        // waits for an edit or a pull request, Context for the provider's
        // reading, Connectors for a server.
        let mut sections = vec![self.environment_section(thread)];
        sections.extend(self.changes_section(thread, cx));
        if let Some(usage) = transcript.usage() {
            let error = self
                .session_control_error
                .as_ref()
                .filter(|(shown, generation, _)| {
                    *shown == thread && *generation == open.generation()
                })
                .map(|(_, _, error)| error.clone());
            sections.push(
                div()
                    .flex()
                    .flex_col()
                    .child(panel_section_head("panel-context-head", "Context"))
                    .child(div().debug_selector(|| "panel-context".into()).child(
                        pane::context_usage(
                            usage,
                            transcript.context_details(),
                            transcript.usage_details(),
                        ),
                    ))
                    .children(
                        error.map(|error| pane::card_error(error).id("session-control-error")),
                    ),
            );
        }
        let servers = transcript.mcp_servers();
        if !servers.is_empty() {
            sections.push(connectors(servers, window, cx));
        }

        let mut body = div()
            .debug_selector(|| "panel-sections".into())
            .flex()
            .flex_col()
            .pb(px(SPACE_2));
        for (index, section) in sections.into_iter().enumerate() {
            if index > 0 {
                body = body.child(section_rule());
            }
            body = body.child(section);
        }
        body.into_any_element()
    }

    /// Environment: the machine, the folder, the branch.
    fn environment_section(&self, thread: ThreadId) -> Div {
        let machine = self.cockpit.machine().map_or_else(
            || "This computer".to_string(),
            |bridge| bridge.name().to_string(),
        );
        let mut section = div()
            .flex()
            .flex_col()
            .child(panel_section_head("panel-environment-head", "Environment"))
            .child(detail_row(
                "panel-environment-machine",
                icons::LAPTOP,
                machine,
                None,
            ));
        if let Some(cwd) = self.thread_path(thread) {
            let (name, place) = folder_words(&cwd);
            section = section.child(
                detail_row("panel-environment-folder", icons::FOLDER, name, place)
                    .cursor_pointer()
                    .tooltip(ferrite_ui::menu::tooltip("Show in its folder"))
                    .on_click(move |_, window, cx| {
                        ferrite_pane::preview::open_original(&cwd, window, cx);
                    }),
            );
        }
        if let (Some(facts), Some(branch)) = (self.facts.get(thread), self.thread_branch(thread)) {
            let meta = branch_meta(&branch, facts);
            section = section.child(detail_row("panel-branch", icons::BRANCH, branch, meta));
        }
        section
    }

    /// The branch `thread`'s checkout is on: as git last said, else as the
    /// Thread was bound.
    pub(super) fn thread_branch(&self, thread: ThreadId) -> Option<SharedString> {
        let facts = self.facts.get(thread)?;
        facts
            .status
            .as_ref()
            .and_then(|status| status.branch.clone())
            .map(SharedString::from)
            .or_else(|| facts.branch.clone())
    }
}

/// A folder as Claude's Environment names it: its own name, then dim where
/// it lies with the home spelled `~` (`ferrite · ~/Desktop/Projects`). A
/// folder with no name of its own (the root) reads whole.
fn folder_words(folder: &std::path::Path) -> (String, Option<String>) {
    match (folder.file_name(), folder.parent()) {
        (Some(name), Some(parent)) => (
            name.to_string_lossy().into_owned(),
            Some(format!("\u{b7} {}", home_relative(parent))),
        ),
        _ => (home_relative(folder), None),
    }
}

/// What follows the branch's name, dim: `from main` (none on the default
/// branch itself: `main from main` says nothing), then the checkout's drift
/// and dirt when git has said (`↑2 ↓1 · 3 dirty`).
fn branch_meta(branch: &SharedString, facts: &crate::facts::ThreadFacts) -> Option<String> {
    let mut words = Vec::new();
    if let Some(base) = facts.default_branch.as_ref().filter(|base| *base != branch) {
        words.push(format!("from {base}"));
    }
    if let Some(status) = &facts.status {
        match (status.ahead, status.behind) {
            (0, 0) => {}
            (ahead, 0) => words.push(format!("\u{2191}{ahead}")),
            (0, behind) => words.push(format!("\u{2193}{behind}")),
            (ahead, behind) => words.push(format!("\u{2191}{ahead} \u{2193}{behind}")),
        }
        if status.dirty > 0 {
            words.push(format!("{} dirty", status.dirty));
        }
    }
    (!words.is_empty()).then(|| words.join(" \u{b7} "))
}

/// The hairline between two sections, `PANEL_SECTION_GAP` clear above and
/// below it.
fn section_rule() -> Div {
    use ferrite_ui::theme::*;
    div()
        .flex_shrink_0()
        .my(px(PANEL_SECTION_GAP))
        .h(px(HAIRLINE_W))
        .bg(paint::LINE)
}

/// One line under a section (`PANEL_SECTION_H`): its icon on the content
/// edge, its words on the text column, a second word dim after them (a
/// folder's place, a branch's base) when there is one, cut in its middle
/// when the line runs out of room so both its ends still read.
pub(super) fn detail_row(
    id: impl Into<gpui::ElementId>,
    glyph: &'static str,
    label: impl Into<SharedString>,
    meta: Option<String>,
) -> gpui::Stateful<Div> {
    use ferrite_ui::theme::*;
    let id = id.into();
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
        .child(
            div()
                .absolute()
                .left(px(PANEL_ROW_ICON_X))
                .top(px((PANEL_SECTION_H - PANEL_ROW_ICON) / 2.))
                .child(icon(glyph, PANEL_ROW_ICON, INK_SUBTLE)),
        )
        .child(
            div()
                .flex_shrink_0()
                .max_w_full()
                .min_w_0()
                .truncate()
                .text_color(rgb(INK_STRONG))
                .child(label.into()),
        )
        .children(meta.map(|meta| {
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis_middle()
                .text_size(px(FS_META))
                .text_color(rgb(INK_SUBTLE))
                .child(SharedString::from(meta))
        }))
}

/// Connectors (Claude's): its head, `Connectors ▸` with `N connected` at
/// the right, folds its servers' lines open and shut — folded at first,
/// the count saying what matters. The fold is the panel's own, kept while
/// it shows.
fn connectors(servers: &[McpServer], window: &mut Window, cx: &mut gpui::App) -> Div {
    use ferrite_ui::theme::*;
    let open = window.use_keyed_state("panel-connectors-open", cx, |_, _| false);
    let unfolded = *open.read(cx);
    let connected = servers
        .iter()
        .filter(|server| server.status == McpStatus::Connected)
        .count();
    let head = div()
        .id("panel-connectors-head")
        .debug_selector(|| "panel-connectors-head".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_3))
        .h(px(PANEL_SECTION_H))
        .px(px(PANEL_ROW_ICON_X))
        .cursor_pointer()
        .text_size(px(FS_META))
        .child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .gap(px(SPACE_1))
                .font_weight(W_LABEL)
                .text_color(rgb(INK_STRONG))
                .child("Connectors")
                .child(ferrite_ui::components::fold_chevron(
                    "panel-connectors-turn",
                    unfolded,
                    NAV_HEADING_CARET,
                    INK_SUBTLE,
                )),
        )
        .child(
            div()
                .text_color(rgb(INK_SUBTLE))
                .child(format!("{connected} connected")),
        )
        .on_click(move |_, _, cx| {
            open.update(cx, |open, cx| {
                *open = !*open;
                cx.notify();
            })
        });
    div()
        .flex()
        .flex_col()
        .child(head)
        .when(unfolded, |section| {
            section.children(
                servers
                    .iter()
                    .enumerate()
                    .map(|(index, server)| connector_row(index, server)),
            )
        })
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
                .child(icon(icons::BLOCKS, PANEL_ROW_ICON, INK_SUBTLE)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(rgb(INK_STRONG))
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

#[cfg(test)]
mod tests {
    use super::folder_words;
    use std::path::Path;

    /// The folder reads by its own name, its place dim after it; the root
    /// has no name of its own and reads whole.
    #[test]
    fn a_folder_reads_as_its_name_then_its_place() {
        assert_eq!(
            folder_words(Path::new("/srv/work/ferrite")),
            ("ferrite".to_string(), Some("\u{b7} /srv/work".to_string()))
        );
        assert_eq!(folder_words(Path::new("/")), ("/".to_string(), None));
    }
}
