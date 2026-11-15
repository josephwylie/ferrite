//! The draft's replies in the tray: each a one-line chip that opens, in
//! place, to the whole quote, where it came from, and the operator's
//! comment, which a multiline field edits (⏎ saves, ⇧⏎ breaks the line,
//! esc cancels). The replies themselves are the draft's (`Composer::replies`);
//! this holds only what the tray is showing of them.

use std::collections::HashSet;

use gpui::{div, prelude::*, px, rgb, AnyElement, App, Entity, SharedString, Window};

use super::Composer;
use ferrite_core::replies::{Reply, Source};
use ferrite_ui::{components, icons, theme};

gpui::actions!(cite, [SaveComment, CancelComment]);

/// The operator asked to see a reply's source: the Pane's transcript
/// scrolls to it, or says it is gone (`Composer::source_missing`).
pub struct OpenSource {
    pub source: Source,
    pub quote: String,
}

impl gpui::EventEmitter<OpenSource> for Composer {}

#[derive(Default)]
pub(super) struct Citations {
    open: Option<usize>,
    editing: Option<Editing>,
    /// Sources a jump found no longer in the transcript.
    missing: HashSet<Source>,
}

struct Editing {
    index: usize,
    field: Entity<Composer>,
}

const LABEL_HEAD: usize = 24;
const LABEL_TAIL: usize = 14;

/// A quote on one line for its chip: its words with their line breaks
/// squeezed, a long one cut in the middle so two passages that begin
/// alike still read apart by their ends.
pub(super) fn label(quote: &str) -> String {
    let words = quote.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = words.chars().collect();
    if chars.len() <= LABEL_HEAD + LABEL_TAIL + 1 {
        return words;
    }
    let head: String = chars[..LABEL_HEAD].iter().collect();
    let tail: String = chars[chars.len() - LABEL_TAIL..].iter().collect();
    format!("{head}\u{2026}{tail}")
}

impl Composer {
    pub(super) fn reset_citations(&mut self) {
        let missing = std::mem::take(&mut self.citations.missing);
        self.citations = Citations {
            missing,
            ..Citations::default()
        };
    }

    pub fn toggle_citation(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        if self.citations.open == Some(index) {
            self.citations.open = None;
            self.citations.editing = None;
        } else if index < self.replies.len() {
            self.citations.open = Some(index);
            if self
                .citations
                .editing
                .as_ref()
                .is_some_and(|edit| edit.index != index)
            {
                self.citations.editing = None;
            }
        }
        cx.notify();
    }

    pub fn edit_comment(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(reply) = self.replies.get(index) else {
            return;
        };
        let words = reply.text.clone();
        let field = cx.new(|cx| {
            let mut field = Composer::new(cx);
            field.set_role(super::Role::Field(Some("CiteComment")), cx);
            field.set(words, cx);
            field
        });
        window.focus(&gpui::Focusable::focus_handle(field.read(cx), cx), cx);
        self.citations.open = Some(index);
        self.citations.editing = Some(Editing { index, field });
        cx.notify();
    }

    /// The open comment field's focus, while it holds the keyboard. A click
    /// on the main line moves focus away; the field stays open but no longer
    /// pulls it back.
    pub(super) fn comment_focus(&self, window: &Window, cx: &App) -> Option<gpui::FocusHandle> {
        let edit = self.citations.editing.as_ref()?;
        let focus = gpui::Focusable::focus_handle(edit.field.read(cx), cx);
        focus.contains_focused(window, cx).then_some(focus)
    }

    /// The comment field's words onto its reply, and the field closed.
    /// Whether one was open.
    pub(super) fn commit_comment(&mut self, cx: &App) -> bool {
        let Some(Editing { index, field }) = self.citations.editing.take() else {
            return false;
        };
        let words = field.read(cx).text().trim().to_string();
        if let Some(reply) = self.replies.get_mut(index) {
            reply.text = words;
        }
        true
    }

    pub fn save_comment(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.commit_comment(cx) {
            return;
        }
        self.focus_handle.focus(window, cx);
        self.edited(cx);
    }

    pub fn cancel_comment(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.citations.editing.take().is_some() {
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    pub fn remove_reply(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if index >= self.replies.len() {
            return;
        }
        self.replies.remove(index);
        let shift = |at: &mut Option<usize>| match *at {
            Some(open) if open == index => *at = None,
            Some(open) if open > index => *at = Some(open - 1),
            _ => {}
        };
        shift(&mut self.citations.open);
        let mut editing = self.citations.editing.as_ref().map(|edit| edit.index);
        shift(&mut editing);
        match (editing, self.citations.editing.as_mut()) {
            (Some(now), Some(edit)) => edit.index = now,
            _ => self.citations.editing = None,
        }
        self.focus_handle.focus(window, cx);
        self.edited(cx);
    }

    pub fn open_source(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        let Some(reply) = self.replies.get(index) else {
            return;
        };
        if let Some(source) = reply.source {
            let quote = reply.quote.clone();
            // The transcript may hold the message now: ask it again.
            self.citations.missing.remove(&source);
            cx.emit(OpenSource { source, quote });
        }
    }

    /// The transcript no longer has `source`: its replies say so, and keep
    /// their quotes.
    pub fn source_missing(&mut self, source: Source, cx: &mut gpui::Context<Self>) {
        self.citations.missing.insert(source);
        cx.notify();
    }

    pub fn is_source_missing(&self, index: usize) -> bool {
        self.replies
            .get(index)
            .and_then(|reply| reply.source)
            .is_some_and(|source| self.citations.missing.contains(&source))
    }
}

pub(super) fn tray(entity: &Entity<Composer>, cx: &App) -> Option<AnyElement> {
    let composer = entity.read(cx);
    if composer.replies.is_empty() {
        return None;
    }
    let open = composer.citations.open;
    let chips = composer
        .replies
        .iter()
        .enumerate()
        .map(|(index, reply)| chip(entity, index, reply, open == Some(index)));
    let panel = open
        .and_then(|index| composer.replies.get(index).map(|reply| (index, reply)))
        .map(|(index, reply)| panel(entity, index, reply, cx));
    Some(
        div()
            .debug_selector(|| "tray-replies".into())
            .flex()
            .flex_col()
            .gap(px(theme::CH))
            .min_w_0()
            .max_w_full()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(theme::CH))
                    .min_w_0()
                    .max_w_full()
                    .children(chips),
            )
            .children(panel)
            .into_any_element(),
    )
}

fn chip(entity: &Entity<Composer>, index: usize, reply: &Reply, open: bool) -> AnyElement {
    let text = label(&reply.quote);
    let toggle = entity.downgrade();
    let remove = entity.downgrade();
    div()
        .debug_selector(move || format!("cite-chip-{index}"))
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::ATTACH_CHIP_H))
        .max_w(px(theme::ATTACH_CHIP_MAX_W))
        .min_w_0()
        .rounded(px(theme::R_CONTROL))
        .bg(theme::paint::RAISED)
        .when(open, |chip| chip.bg(theme::paint::SELECTED))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_META))
        .font_weight(theme::W_BODY)
        .text_color(rgb(theme::LINK_INK))
        .whitespace_nowrap()
        .child(
            components::button(("cite-open", index))
                .tab_stop(true)
                .key_context("PromptAttachment")
                .p_0()
                .px(px(theme::SPACE_2))
                .h(px(theme::ATTACH_CHIP_H))
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(theme::CH))
                .accessibility_label(format!("Quote: {}", reply.quote))
                .tooltip_builder(ferrite_ui::menu::tooltip(if open {
                    "Hide the quote"
                } else {
                    "Show the quote and comment"
                }))
                .child(icons::icon(
                    icons::QUOTE,
                    theme::ATTACH_THUMB,
                    theme::LINK_INK,
                ))
                .child(div().min_w_0().truncate().child(SharedString::from(text)))
                .when(!reply.text.is_empty(), |button| {
                    button.child(
                        div()
                            .debug_selector(move || format!("cite-chip-{index}-comment"))
                            .flex_shrink_0()
                            .child(icons::icon(
                                icons::PENCIL,
                                theme::ATTACH_THUMB,
                                theme::INK_SUBTLE,
                            )),
                    )
                })
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    let _ = toggle.update(cx, |composer, cx| composer.toggle_citation(index, cx));
                }),
        )
        .child(
            crate::attachments::remove_button(("remove-reply", index))
                .tab_stop(!open)
                .debug_selector(move || format!("cite-chip-{index}-remove"))
                .accessibility_label("Remove quote")
                .tooltip_builder(ferrite_ui::menu::tooltip("Remove quote"))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    let _ =
                        remove.update(cx, |composer, cx| composer.remove_reply(index, window, cx));
                }),
        )
        .into_any_element()
}

fn panel(entity: &Entity<Composer>, index: usize, reply: &Reply, cx: &App) -> AnyElement {
    let composer = entity.read(cx);
    let editing = composer
        .citations
        .editing
        .as_ref()
        .filter(|edit| edit.index == index)
        .map(|edit| edit.field.clone());
    let missing = composer.is_source_missing(index);
    let muted = |text: &'static str| div().text_color(rgb(theme::INK_SUBTLE)).child(text);
    let source = match reply.source {
        None => muted("Source not recorded").into_any_element(),
        Some(_) if missing => div()
            .debug_selector(|| "cite-source-missing".into())
            .text_color(rgb(theme::INK_SUBTLE))
            .child("Source unavailable. The quote is kept.")
            .into_any_element(),
        Some(_) => {
            let go = entity.downgrade();
            div()
                .flex()
                .child(
                    components::form_button(("cite-source", index), cx)
                        .debug_selector(|| "cite-source".into())
                        .key_context("PromptAttachment")
                        .child("Go to source")
                        .on_click(move |_, _, cx| {
                            let _ = go.update(cx, |composer, cx| composer.open_source(index, cx));
                        }),
                )
                .into_any_element()
        }
    };
    let comment = match editing {
        Some(field) => {
            let save = entity.downgrade();
            let cancel = entity.downgrade();
            div()
                .flex()
                .flex_col()
                .gap(px(theme::SPACE_1_5))
                .child(
                    div()
                        .debug_selector(|| "cite-comment-field".into())
                        .px(px(theme::SPACE_2))
                        .rounded(px(theme::R_CONTROL))
                        .border_1()
                        .border_color(theme::paint::LINE)
                        .child(field),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(theme::SPACE_2))
                        .child(
                            components::primary_button(("cite-save", index), false, cx)
                                .debug_selector(|| "cite-save".into())
                                .key_context("PromptAttachment")
                                .child("Save")
                                .on_click(move |_, window, cx| {
                                    let _ = save.update(cx, |composer, cx| {
                                        composer.save_comment(window, cx)
                                    });
                                }),
                        )
                        .child(
                            components::form_button(("cite-cancel", index), cx)
                                .debug_selector(|| "cite-cancel".into())
                                .key_context("PromptAttachment")
                                .child("Cancel")
                                .on_click(move |_, window, cx| {
                                    let _ = cancel.update(cx, |composer, cx| {
                                        composer.cancel_comment(window, cx)
                                    });
                                }),
                        ),
                )
                .into_any_element()
        }
        None => {
            let edit = entity.downgrade();
            let verb = if reply.text.is_empty() {
                "Add comment"
            } else {
                "Edit comment"
            };
            div()
                .flex()
                .flex_col()
                .gap(px(theme::SPACE_1))
                .when(!reply.text.is_empty(), |column| {
                    column.child(
                        div()
                            .debug_selector(|| "cite-comment".into())
                            .text_color(rgb(theme::INK_STRONG))
                            .child(SharedString::from(reply.text.clone())),
                    )
                })
                .child(
                    div().flex().child(
                        components::form_button(("cite-edit", index), cx)
                            .debug_selector(|| "cite-edit".into())
                            .key_context("PromptAttachment")
                            .child(verb)
                            .on_click(move |_, window, cx| {
                                let _ = edit.update(cx, |composer, cx| {
                                    composer.edit_comment(index, window, cx)
                                });
                            }),
                    ),
                )
                .into_any_element()
        }
    };
    let remove = entity.downgrade();
    let close = entity.downgrade();
    let save = entity.downgrade();
    let cancel = entity.downgrade();
    div()
        .id(("cite-panel", index))
        .debug_selector(|| "cite-panel".into())
        .on_action(move |_: &SaveComment, window: &mut Window, cx: &mut App| {
            let _ = save.update(cx, |composer, cx| composer.save_comment(window, cx));
        })
        .on_action(
            move |_: &CancelComment, window: &mut Window, cx: &mut App| {
                let _ = cancel.update(cx, |composer, cx| composer.cancel_comment(window, cx));
            },
        )
        .flex()
        .flex_col()
        .gap(px(theme::SPACE_1_5))
        .w_full()
        .min_w_0()
        .p(px(theme::SPACE_2))
        .rounded(px(theme::R_CARD))
        .bg(theme::paint::RAISED)
        .border_1()
        .border_color(theme::paint::LINE)
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_META))
        .font_weight(theme::W_BODY)
        .child(
            div()
                .id(("cite-quote", index))
                .debug_selector(|| "cite-quote".into())
                .max_h(px(theme::CITE_QUOTE_MAX_H))
                .overflow_y_scroll()
                .border_l(px(theme::QUOTE_RULE_W))
                .border_color(theme::paint::LINE)
                .pl(px(theme::SPACE_2))
                .text_color(rgb(theme::INK_STRONG))
                .child(SharedString::from(reply.quote.clone())),
        )
        .child(comment)
        .child(source)
        .child(
            div()
                .flex()
                .gap(px(theme::SPACE_2))
                .child(
                    components::form_button(("cite-close", index), cx)
                        .debug_selector(|| "cite-close".into())
                        .key_context("PromptAttachment")
                        .child("Close")
                        .on_click(move |_, _, cx| {
                            let _ = close
                                .update(cx, |composer, cx| composer.toggle_citation(index, cx));
                        }),
                )
                .child(
                    components::form_button(("cite-remove", index), cx)
                        .debug_selector(|| "cite-remove".into())
                        .key_context("PromptAttachment")
                        .child("Remove")
                        .on_click(move |_, window, cx| {
                            let _ = remove.update(cx, |composer, cx| {
                                composer.remove_reply(index, window, cx)
                            });
                        }),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::label;

    #[test]
    fn a_short_quote_is_its_words_on_one_line() {
        assert_eq!(label("two\n  lines"), "two lines");
    }

    #[test]
    fn a_long_quote_keeps_its_head_and_its_tail() {
        let a = label(
            "The migration plan covers every table in the main database and then the replica",
        );
        let b = label(
            "The migration plan covers every table in the main database and then the archive",
        );
        assert_ne!(a, b);
        assert!(a.starts_with("The migration plan cove"));
        assert!(a.ends_with("the replica"));
        assert!(a.contains('\u{2026}'));
    }
}
