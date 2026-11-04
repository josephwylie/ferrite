//! The transcript's right-click menus, as Claude's (`claude/rc-answer-text.png`,
//! `claude/rc-prompt.png`), and an answer's `⋯`, which opens the answer's own:
//!
//! - **an answer:** Copy message, Copy message as Markdown │ Attach message
//!   as context, Pin as chapter │ Fork from here;
//! - **a prompt:** Copy message, Copy message as Markdown │ Attach message
//!   as context │ Rewind to here, Fork from here;
//! - **a link** (a press that landed on one): Open link, Copy link, and for
//!   a file's path, Open in Files.
//!
//! Every row is the same face (the UI font, no shortcut glyphs); a row's
//! subtitle is a smaller muted line under its label. The menu opens in one
//! frame and a right-click on a message clears the selection, as Claude's.
//! Attach is a Reply card in the Composer's tray, so it is offered only
//! where the Pane has a Composer (`TranscriptInput::quotes`); Rewind and
//! Fork only where the agent can (`TranscriptInput::rewind`), disabled
//! with the reason while it cannot be asked now.
//!
//! Pin as chapter marks the answer in this transcript only: the minimum
//! behind the row until chapters have somewhere to be listed.

use ferrite_core::transcript::BlockId;
use gpui::prelude::*;
use gpui::{AnyElement, ClipboardItem, Context, MouseButton, Pixels, Point, SharedString};

use super::{PromptAction, TranscriptEvent, TranscriptView};
use crate::file_links::FileLink;
use crate::quote::Reply;
use ferrite_ui::menu;

/// What the menu is on.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Target {
    /// An answer, its last block where a fork from it ends.
    Answer { last: BlockId },
    /// A sent prompt: whether it can be rewound or forked (its id, and the
    /// agent), and the reason while it cannot be asked now.
    Prompt {
        block: BlockId,
        cuts: Option<Result<(), SharedString>>,
    },
    /// A link: its destination, and the file it names.
    Link { url: String, file: Option<FileLink> },
}

/// The menu while it is open: what it acts on and where it hangs.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct MessageMenu {
    pub(super) target: Target,
    /// The message as plain words, as Copy copies it.
    pub(super) plain: String,
    /// The message's Markdown, as Copy as Markdown and Attach take it.
    pub(super) markdown: String,
    pub(super) at: Point<Pixels>,
}

/// One row of a menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Verb {
    Copy,
    CopyMarkdown,
    Attach,
    Pin,
    Unpin,
    Rewind,
    Fork,
    OpenLink,
    CopyLink,
    OpenInFiles,
}

impl Verb {
    fn label(self) -> &'static str {
        match self {
            Verb::Copy => "Copy message",
            Verb::CopyMarkdown => "Copy message as Markdown",
            Verb::Attach => "Attach message as context",
            Verb::Pin => "Pin as chapter",
            Verb::Unpin => "Unpin chapter",
            Verb::Rewind => "Rewind to here",
            Verb::Fork => "Fork from here",
            Verb::OpenLink => "Open link",
            Verb::CopyLink => "Copy link",
            Verb::OpenInFiles => "Open in Files",
        }
    }

    /// The line under the label, where Claude's menu has one.
    fn subtitle(self) -> Option<&'static str> {
        match self {
            Verb::Rewind => Some("Takes this and what follows out of the conversation"),
            Verb::Fork => Some("Starts a new session, keeps this one"),
            _ => None,
        }
    }
}

/// A row: its verb, and why it cannot be pressed now when it cannot.
type Row = (Verb, Option<SharedString>);

impl TranscriptView {
    /// A menu's rows, `None` a rule between groups (Claude's).
    pub(super) fn menu_rows(&self, menu: &MessageMenu) -> Vec<Option<Row>> {
        let attach = self.input.quotes.then_some((Verb::Attach, None));
        let copies = [Some((Verb::Copy, None)), Some((Verb::CopyMarkdown, None))];
        let group = |rows: Vec<Option<Row>>| rows.into_iter().flatten().collect::<Vec<_>>();
        let mut groups: Vec<Vec<Option<Row>>> = Vec::new();
        match &menu.target {
            Target::Answer { last } => {
                groups.push(copies.to_vec());
                let pin = if self.pinned.contains(last) {
                    Verb::Unpin
                } else {
                    Verb::Pin
                };
                groups.push(vec![attach.clone(), Some((pin, None))]);
                if let Some(allowed) = &self.input.rewind {
                    groups.push(vec![Some((Verb::Fork, allowed.clone().err()))]);
                }
            }
            Target::Prompt { cuts, .. } => {
                groups.push(copies.to_vec());
                groups.push(vec![attach.clone()]);
                if let Some(allowed) = cuts {
                    let why = allowed.clone().err();
                    groups.push(vec![
                        Some((Verb::Rewind, why.clone())),
                        Some((Verb::Fork, why)),
                    ]);
                }
            }
            Target::Link { file, .. } => {
                groups.push(vec![
                    Some((Verb::OpenLink, None)),
                    Some((Verb::CopyLink, None)),
                ]);
                if file.is_some() {
                    groups.push(vec![Some((Verb::OpenInFiles, None))]);
                }
            }
        }
        let mut rows = Vec::new();
        for rows_of in groups {
            let rows_of = group(rows_of);
            if rows_of.is_empty() {
                continue;
            }
            if !rows.is_empty() {
                rows.push(None);
            }
            rows.extend(rows_of.into_iter().map(Some));
        }
        rows
    }

    pub(super) fn open_message_menu(&mut self, menu: MessageMenu, cx: &mut Context<Self>) {
        self.message_menu = Some(menu);
        cx.notify();
    }

    fn run_verb(&mut self, menu: &MessageMenu, verb: Verb, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let copy = |text: &str, cx: &mut Context<Self>| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()))
        };
        match (verb, &menu.target) {
            (Verb::Copy, _) => copy(&menu.plain, cx),
            (Verb::CopyMarkdown, _) => copy(&menu.markdown, cx),
            (Verb::Attach, _) => {
                if let Some(passage) = crate::quote::passage(&menu.markdown) {
                    cx.emit(TranscriptEvent::Reply(Reply::new(passage, "")));
                }
            }
            (Verb::Pin, Target::Answer { last }) => {
                self.pinned.insert(*last);
            }
            (Verb::Unpin, Target::Answer { last }) => {
                self.pinned.remove(last);
            }
            (Verb::Fork, Target::Answer { last }) => {
                cx.emit(TranscriptEvent::Fork { after: *last })
            }
            (Verb::Rewind, Target::Prompt { block, .. }) => cx.emit(TranscriptEvent::Prompt {
                block: *block,
                action: PromptAction::Rewind,
                at: menu.at,
            }),
            (Verb::Fork, Target::Prompt { block, .. }) => cx.emit(TranscriptEvent::Prompt {
                block: *block,
                action: PromptAction::Fork,
                at: menu.at,
            }),
            (Verb::OpenLink, Target::Link { url, file }) => match file {
                Some(file) => file.open(window, cx),
                None => cx.open_url(url),
            },
            (Verb::CopyLink, Target::Link { url, .. }) => copy(url, cx),
            (Verb::OpenInFiles, Target::Link { file: Some(file), .. }) => {
                cx.emit(TranscriptEvent::OpenReader {
                    path: file.path.clone(),
                    line: None,
                })
            }
            _ => {}
        }
    }

    /// The open menu, hung at the press that opened it.
    pub(super) fn message_menu_float(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.message_menu.clone()?;
        let mut shell = menu::shell()
            .debug_selector(|| "answer-menu".into())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|view, _: &gpui::MouseDownEvent, _, cx| {
                view.message_menu = None;
                cx.notify();
            }));
        for (index, row) in self.menu_rows(&open).into_iter().enumerate() {
            let Some((verb, why)) = row else {
                shell = shell.child(menu::rule());
                continue;
            };
            let mut item = menu::Item::new(verb.label());
            item.disabled = why.is_some();
            item.note = why.or_else(|| verb.subtitle().map(SharedString::from));
            let mut row = menu::item_row(("answer-menu-row", index), &item, false, false)
                .debug_selector(move || format!("answer-menu-row-{index}"));
            if !item.disabled {
                let menu = open.clone();
                row = row.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        view.message_menu = None;
                        view.run_verb(&menu, verb, window, cx);
                        cx.notify();
                    }),
                );
            }
            shell = shell.child(row);
        }
        Some(super::hang_menu("answer-menu-in", open.at, shell))
    }

    /// The answer row `row`'s menu: its `last` block and `markdown`.
    pub(super) fn answer_menu(
        &self,
        row: &super::rows::RowId,
        last: BlockId,
        markdown: String,
        at: Point<Pixels>,
    ) -> MessageMenu {
        let plain = self
            .rows
            .rows()
            .iter()
            .find(|candidate| candidate.id() == row)
            .map(|row| ferrite_core::transcript::plain_text(row.blocks()))
            .unwrap_or_default();
        MessageMenu {
            target: Target::Answer { last },
            plain,
            markdown,
            at,
        }
    }

    /// A right-click on a message (`menu` its menu): the selection clears,
    /// and the menu opens where the press was — a link's, instead, when
    /// the press landed on one in the message.
    pub(super) fn right_click_message(
        &mut self,
        mut menu: MessageMenu,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        gpui::base::TextSelection::clear(window, cx);
        if let Some(url) = gpui::base::text::pressed_link(cx) {
            let url = url.to_string();
            let file = FileLink::resolve(&url, self.input.workspace.as_deref());
            menu.target = Target::Link { url, file };
        }
        self.open_message_menu(menu, cx);
    }
}

/// The answer's `⋯`'s tooltip, Claude's.
pub(super) const MORE_LABEL: &str = "Message actions";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows_name_claudes_verbs() {
        assert_eq!(
            [
                Verb::Copy,
                Verb::CopyMarkdown,
                Verb::Attach,
                Verb::Pin,
                Verb::Rewind,
                Verb::Fork
            ]
            .map(Verb::label),
            [
                "Copy message",
                "Copy message as Markdown",
                "Attach message as context",
                "Pin as chapter",
                "Rewind to here",
                "Fork from here"
            ]
        );
        assert_eq!(
            Verb::Fork.subtitle(),
            Some("Starts a new session, keeps this one")
        );
    }
}
