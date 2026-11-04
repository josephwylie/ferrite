//! An answer's actions, as Claude's (`claude/hover-assistant.png`): Copy,
//! Fork and `⋯`, under the answer and shown with the message actions'
//! motion. Fork starts a new Thread holding the conversation through this
//! answer (`TranscriptEvent::Fork`); `⋯` opens the answer's menu — what
//! Claude's `Message actions` holds that Ferrite backs: Copy message, Reply
//! (Claude's `Attach message as context`, a Reply card in the Composer's tray) and
//! Fork from here. Claude's Pin and Read aloud have nothing behind them in
//! Ferrite, so they are left out.

use ferrite_core::transcript::BlockId;
use gpui::prelude::*;
use gpui::{AnyElement, ClipboardItem, Context, MouseButton, Pixels, Point, SharedString};

use super::{TranscriptEvent, TranscriptView};
use crate::quote::Reply;
use ferrite_ui::menu;

/// The answer's `⋯` menu while it is open: the answer it acts on and where
/// it hangs.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct AnswerMenu {
    /// The answer's last block: where a fork from it ends.
    block: BlockId,
    /// The answer's Markdown, as its Copy copies it.
    text: String,
    at: Point<Pixels>,
}

/// One row of the answer's menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AnswerVerb {
    Copy,
    Reply,
    Fork,
}

impl AnswerVerb {
    fn label(self) -> &'static str {
        match self {
            AnswerVerb::Copy => "Copy message",
            AnswerVerb::Reply => "Reply",
            AnswerVerb::Fork => "Fork from here",
        }
    }
}

impl TranscriptView {
    /// The answer menu's rows, `None` a rule between groups (Claude's: the
    /// copies, the reply, then the fork): Reply where the Pane has a
    /// Composer to quote into, Fork where the agent can, disabled with why
    /// while it cannot be asked now.
    pub(super) fn answer_verbs(&self) -> Vec<Option<(AnswerVerb, Option<SharedString>)>> {
        let mut rows = vec![Some((AnswerVerb::Copy, None))];
        if self.input.quotes {
            rows.push(Some((AnswerVerb::Reply, None)));
        }
        if let Some(allowed) = &self.input.rewind {
            rows.push(None);
            rows.push(Some((AnswerVerb::Fork, allowed.clone().err())));
        }
        rows
    }

    pub(super) fn open_answer_menu(
        &mut self,
        block: BlockId,
        text: String,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.answer_menu = Some(AnswerMenu { block, text, at });
        cx.notify();
    }

    fn run_answer_verb(&mut self, menu: &AnswerMenu, verb: AnswerVerb, cx: &mut Context<Self>) {
        match verb {
            AnswerVerb::Copy => cx.write_to_clipboard(ClipboardItem::new_string(menu.text.clone())),
            AnswerVerb::Reply => {
                if let Some(passage) = crate::quote::passage(&menu.text) {
                    cx.emit(TranscriptEvent::Reply(Reply::new(passage, "")));
                }
            }
            AnswerVerb::Fork => cx.emit(TranscriptEvent::Fork { after: menu.block }),
        }
    }

    /// The open answer menu, hung at the press that opened it.
    pub(super) fn answer_menu_float(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.answer_menu.clone()?;
        let mut shell = menu::shell()
            .debug_selector(|| "answer-menu".into())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|view, _: &gpui::MouseDownEvent, _, cx| {
                view.answer_menu = None;
                cx.notify();
            }));
        for (index, row) in self.answer_verbs().into_iter().enumerate() {
            let Some((verb, why)) = row else {
                shell = shell.child(menu::rule());
                continue;
            };
            let mut item = menu::Item::new(verb.label());
            item.disabled = why.is_some();
            item.note = why;
            let mut row = menu::item_row(("answer-menu-row", index), &item, false, false)
                .debug_selector(move || format!("answer-menu-row-{index}"));
            if !item.disabled {
                let menu = open.clone();
                row = row.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _: &gpui::MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        view.answer_menu = None;
                        view.run_answer_verb(&menu, verb, cx);
                        cx.notify();
                    }),
                );
            }
            shell = shell.child(row);
        }
        Some(super::hang_menu("answer-menu-in", open.at, shell))
    }
}

/// The answer's `⋯`'s tooltip, Claude's.
pub(super) const MORE_LABEL: &str = "Message actions";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_names_claudes_verbs() {
        assert_eq!(
            [AnswerVerb::Copy, AnswerVerb::Reply, AnswerVerb::Fork].map(AnswerVerb::label),
            ["Copy message", "Reply", "Fork from here"]
        );
    }
}
