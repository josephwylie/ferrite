//! Citation chips navigate to their source. Their pencil opens the comment editor.

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Instant;

use gpui::component::Disableable;
use gpui::{AnyElement, App, ElementId, Entity, SharedString, Window, div, prelude::*, px, rgb};

use super::Composer;
use super::departed::{Gone, Place};
use ferrite_core::replies::{Reply, Source};
use ferrite_ui::{components, icons, motion, theme};

gpui::actions!(cite, [SaveComment, SaveAndSendComment, CancelComment]);

/// The operator asked to see a reply's source: the Pane's transcript
/// scrolls to it, or says it is gone (`Composer::source_missing`).
pub struct OpenSource {
    pub source: Source,
    pub quote: String,
}

impl gpui::EventEmitter<OpenSource> for Composer {}

pub struct CommentOpened {
    pub index: usize,
    pub window: gpui::AnyWindowHandle,
}
impl gpui::EventEmitter<CommentOpened> for Composer {}

#[derive(Default)]
pub(super) struct Citations {
    source_anchor: Option<gpui::base::TextSelectionAnchor>,
    trigger: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    editing: Option<Editing>,
    /// Sources a jump found no longer in the transcript.
    missing: HashSet<Source>,
}

struct Editing {
    index: usize,
    field: Entity<Composer>,
    focus_pending: Rc<Cell<bool>>,
    remove_on_cancel: bool,
    source_highlight: Option<gpui::base::TextSelectionAnchor>,
}

const MAX_COMMENT_LENGTH: usize = 8_000;

fn valid_comment(text: &str) -> bool {
    text.encode_utf16().count() <= MAX_COMMENT_LENGTH
}

pub(super) fn label(text: &str) -> String {
    let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.encode_utf16().count() <= 64 {
        return words;
    }
    let mut units = 0;
    let mut label = String::new();
    for ch in words.chars() {
        if units + ch.len_utf16() > 64 {
            break;
        }
        units += ch.len_utf16();
        label.push(ch);
    }
    label.push('…');
    label
}

fn reply_label(reply: &Reply) -> String {
    label(if reply.text.trim().is_empty() {
        &reply.quote
    } else {
        reply.text.trim()
    })
}

impl Composer {
    pub(super) fn reset_citations(&mut self) {
        let missing = std::mem::take(&mut self.citations.missing);
        self.citations = Citations {
            missing,
            ..Citations::default()
        };
    }

    pub fn cite_with_comment(
        &mut self,
        reply: Reply,
        anchor: Option<gpui::base::TextSelectionAnchor>,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.dismiss_comment(cx) {
            return;
        }
        self.reply(reply, cx);
        self.begin_comment(self.replies.len() - 1, cx);
        self.citations.editing.as_mut().unwrap().remove_on_cancel = true;
        if let Some(anchor) = &anchor {
            let editor = self.citations.editing.as_ref().unwrap().field.entity_id();
            let composer = cx.entity().downgrade();
            anchor.on_change(move |bounds, window, cx| {
                let _ = composer.update(cx, |composer, cx| {
                    if !composer
                        .citations
                        .editing
                        .as_ref()
                        .is_some_and(|edit| edit.field.entity_id() == editor)
                    {
                        return;
                    }
                    if bounds.is_none() {
                        let restore = composer.comment_focus(window, cx).is_some();
                        if composer.dismiss_comment(cx) {
                            if restore {
                                composer.focus_handle.focus(window, cx);
                            }
                        } else {
                            composer.citations.source_anchor = None;
                        }
                    }
                    cx.notify();
                    cx.refresh_windows();
                });
            });
        }
        self.citations.source_anchor = anchor;
        cx.refresh_windows();
    }

    fn begin_comment(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        let Some(reply) = self.replies.get(index) else {
            return;
        };
        let words = reply.text.clone();
        let field = cx.new(|cx| {
            let mut field = Composer::new(cx);
            field.set_role(super::Role::Field(Some("CiteComment")), cx);
            field.set(words, cx);
            field.set_visible_row_limit(7, cx);
            field
        });
        cx.subscribe(&field, |_, _, _: &super::Edited, cx| cx.notify())
            .detach();
        self.citations.editing = Some(Editing {
            index,
            field,
            focus_pending: Rc::new(Cell::new(true)),
            remove_on_cancel: false,
            source_highlight: None,
        });
    }

    pub fn edit_comment(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self
            .citations
            .editing
            .as_ref()
            .is_some_and(|edit| edit.index == index)
        {
            self.dismiss_comment(cx);
            return;
        }
        if !self.dismiss_comment(cx) {
            return;
        }
        self.begin_comment(index, cx);
        self.citations.source_anchor = None;
        cx.emit(CommentOpened {
            index,
            window: window.window_handle(),
        });
        if let Some(edit) = &self.citations.editing {
            window.focus(&gpui::Focusable::focus_handle(edit.field.read(cx), cx), cx);
            edit.focus_pending.set(false);
        }
        cx.notify();
    }

    pub fn highlight_comment_source(
        &mut self,
        index: usize,
        anchor: gpui::base::TextSelectionAnchor,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(edit) = self
            .citations
            .editing
            .as_mut()
            .filter(|edit| edit.index == index)
        {
            edit.source_highlight = Some(anchor);
            cx.refresh_windows();
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn comment_source_bounds(&self) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.citations
            .editing
            .as_ref()?
            .source_highlight
            .as_ref()?
            .bounds()
    }

    /// Claim the first frame before prepaint, then preserve the comment's
    /// keyboard only while it holds focus. An outside click can take it away.
    pub(super) fn comment_focus(&self, window: &Window, cx: &App) -> Option<gpui::FocusHandle> {
        let edit = self.citations.editing.as_ref()?;
        let focus = gpui::Focusable::focus_handle(edit.field.read(cx), cx);
        (edit.focus_pending.get() || focus.contains_focused(window, cx)).then_some(focus)
    }

    pub fn comment_valid(&self, cx: &App) -> bool {
        self.citations
            .editing
            .as_ref()
            .is_none_or(|edit| valid_comment(edit.field.read(cx).text()))
    }

    pub(super) fn commit_comment(&mut self, cx: &App) -> bool {
        if !self.comment_valid(cx) {
            return false;
        }
        let Some(Editing { index, field, .. }) = self.citations.editing.take() else {
            return false;
        };
        if let Some(reply) = self.replies.get_mut(index) {
            reply.text = field.read(cx).text().trim().to_string();
        }
        true
    }

    fn dismiss_comment(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        if let Some(edit) = self.citations.editing.as_ref() {
            let draft = edit.field.read(cx).text();
            let saved = self
                .replies
                .get(edit.index)
                .map_or("", |reply| reply.text.as_str());
            if draft.trim() != saved.trim() && !self.commit_comment(cx) {
                return false;
            }
        }
        self.citations.editing = None;
        self.citations.source_anchor = None;
        cx.refresh_windows();
        self.edited(cx);
        true
    }

    pub fn save_comment(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> bool {
        if self
            .citations
            .editing
            .as_ref()
            .is_some_and(|edit| edit.field.read(cx).line.marked().is_some())
            || !self.commit_comment(cx)
        {
            return false;
        }
        self.citations.source_anchor = None;
        cx.refresh_windows();
        self.focus_handle.focus(window, cx);
        self.edited(cx);
        true
    }

    pub fn cancel_comment(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self
            .citations
            .editing
            .as_ref()
            .is_some_and(|edit| edit.field.read(cx).line.marked().is_some())
        {
            return;
        }
        if let Some(edit) = self.citations.editing.take() {
            if edit.remove_on_cancel {
                self.remove_reply(edit.index, window, cx);
            }
            self.citations.source_anchor = None;
            cx.refresh_windows();
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
        let tray_held = self.files_focus.contains_focused(window, cx);
        let reply = self.replies.remove(index);
        self.reply_added = None;
        self.depart(super::departed::Gone::Reply(reply), index, cx);
        let shift = |at: &mut Option<usize>| match *at {
            Some(open) if open == index => *at = None,
            Some(open) if open > index => *at = Some(open - 1),
            _ => {}
        };
        let mut editing = self.citations.editing.as_ref().map(|edit| edit.index);
        shift(&mut editing);
        match (editing, self.citations.editing.as_mut()) {
            (Some(now), Some(edit)) => edit.index = now,
            _ => self.citations.editing = None,
        }
        if self.citations.editing.is_none() {
            self.citations.source_anchor = None;
            cx.refresh_windows();
        }
        self.refocus_after_removal(tray_held, window, cx);
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

/// The replies' chips, the ones the draft holds and, in the places they
/// held, the ones taken off it that are still closing (`departed`).
pub(super) fn tray(entity: &Entity<Composer>, now: Instant, cx: &App) -> Option<AnyElement> {
    let composer = entity.read(cx);
    let places = composer.places(composer.replies.len(), now, |gone| {
        matches!(gone, Gone::Reply(_))
    });
    if places.is_empty() {
        return None;
    }
    let growing = composer
        .reply_added
        .filter(|(_, at)| now.saturating_duration_since(*at) < motion::CHIP_IN.duration())
        .map(|(index, _)| index);
    let open = composer.citations.editing.as_ref().map(|edit| edit.index);
    let chips: Vec<_> = places
        .into_iter()
        .filter_map(|place| match place {
            Place::Gone(ghost) => match &ghost.what {
                Gone::Reply(reply) => Some(ghost_chip(ghost.left, reply)),
                Gone::File { .. } => None,
            },
            Place::Kept(index) => {
                let chip = chip(
                    entity,
                    index,
                    &composer.replies[index],
                    open == Some(index),
                    composer.citations.trigger.clone(),
                );
                Some(if growing == Some(index) {
                    motion::chip_in(
                        SharedString::from(format!("reply-in-{index}")),
                        div().child(chip),
                    )
                    .into_any_element()
                } else {
                    chip
                })
            }
        })
        .collect();
    let panel = open
        .and_then(|index| composer.replies.get(index).map(|reply| (index, reply)))
        .map(|(index, reply)| {
            let source = composer
                .citations
                .source_anchor
                .as_ref()
                .and_then(|anchor| anchor.bounds());
            let trigger = composer.citations.trigger.get();
            gpui::deferred(
                gpui::base::Positioner::side(source.or(trigger).unwrap_or_default())
                    .placement(if source.is_some() {
                        gpui::base::Placement::Bottom
                    } else {
                        gpui::base::Placement::Top
                    })
                    .align(gpui::base::Align::End)
                    .offset(px(4.))
                    .margin(px(16.))
                    .child(panel(entity, index, reply, cx)),
            )
            .with_priority(4)
            .into_any_element()
        });
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

/// What a chip shows of its reply: the quote's mark and words, and a pencil
/// when it carries a comment.
fn chip_face(_index: Option<usize>, reply: &Reply) -> Vec<AnyElement> {
    let face = vec![
        icons::icon(icons::QUOTE, theme::ATTACH_THUMB, theme::LINK_INK).into_any_element(),
        div()
            .min_w_0()
            .truncate()
            .child(SharedString::from(reply_label(reply)))
            .into_any_element(),
    ];
    face
}

fn chip_shell() -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::ATTACH_CHIP_H))
        .max_w(px(theme::ATTACH_CHIP_MAX_W))
        .min_w_0()
        .rounded(px(theme::R_CONTROL))
        .bg(theme::paint::RAISED)
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_META))
        .font_weight(theme::W_BODY)
        .text_color(rgb(theme::LINK_INK))
        .whitespace_nowrap()
}

/// A reply taken off the draft at `left`, closing its place: its chip
/// drawn as it stood, with no controls to reach. Its id is the moment it
/// went and its words, not its place, which moves as the ghosts before it
/// expire.
fn ghost_chip(left: Instant, reply: &Reply) -> AnyElement {
    let id = SharedString::from(format!("reply-out-{left:?}-{}", reply.quote));
    motion::slip_out(
        id.clone(),
        left,
        motion::CHIP_OUT,
        theme::CH,
        chip_view(id, None, reply, false, None, None),
    )
    .into_any_element()
}

fn chip(
    entity: &Entity<Composer>,
    index: usize,
    reply: &Reply,
    open: bool,
    trigger: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
) -> AnyElement {
    chip_view(
        ElementId::from(("cite-open", index)),
        Some(index),
        reply,
        open,
        Some(entity),
        Some(trigger),
    )
}

/// A reply's chip: the live one when `entity` is given (`index` its place
/// among the draft's replies), else the ghost of one gone. They are one
/// recipe, so a closing chip is the chip it was.
fn chip_view(
    id: impl Into<ElementId>,
    index: Option<usize>,
    reply: &Reply,
    open: bool,
    entity: Option<&Entity<Composer>>,
    trigger: Option<Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>>,
) -> AnyElement {
    let live = entity.is_some();
    let mut open_chip = components::button(id)
        .tab_stop(live)
        .key_context("PromptAttachment")
        .p_0()
        .px(px(theme::SPACE_2))
        .h(px(theme::ATTACH_CHIP_H))
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(theme::CH))
        .children(chip_face(index, reply));
    let mut remove = crate::attachments::remove_button(SharedString::from(format!(
        "remove-reply-{}",
        index.map_or_else(|| "gone".to_string(), |index| index.to_string())
    )))
    .tab_stop(live && !open);
    if let (Some(entity), Some(index)) = (entity, index) {
        let toggle = entity.downgrade();
        let removed = entity.downgrade();
        open_chip = open_chip
            .debug_selector(move || format!("cite-chip-{index}-source"))
            .accessibility_label(format!("View cited assistant text: {}", reply_label(reply)))
            .tooltip_builder(ferrite_ui::menu::tooltip("Go to quoted response"))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                let _ = toggle.update(cx, |composer, cx| composer.open_source(index, cx));
            });
        remove = remove
            .debug_selector(move || format!("cite-chip-{index}-remove"))
            .accessibility_label("Remove quote")
            .tooltip_builder(ferrite_ui::menu::tooltip("Remove quote"))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                let _ = removed.update(cx, |composer, cx| composer.remove_reply(index, window, cx));
            });
    }
    let commented = !reply.text.is_empty();
    let mut edit = components::button(SharedString::from(format!(
        "cite-comment-trigger-{}",
        index.map_or_else(|| "gone".to_string(), |index| index.to_string())
    )))
    .tab_stop(live)
    .h(px(theme::ATTACH_CHIP_H))
    .w(px(theme::ATTACH_CHIP_H))
    .p_0()
    .child(
        div()
            .when_some(index.filter(|_| commented), |mark, index| {
                mark.debug_selector(move || format!("cite-chip-{index}-comment"))
            })
            .child(icons::icon(
                icons::PENCIL,
                theme::ATTACH_THUMB,
                theme::INK_SUBTLE,
            )),
    );
    if let (Some(entity), Some(index)) = (entity, index) {
        let edited = entity.downgrade();
        edit = edit
            .debug_selector(move || format!("cite-chip-{index}-comment-trigger"))
            .accessibility_label(if commented {
                "Edit citation comment"
            } else {
                "Add comment to citation"
            })
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                let _ = edited.update(cx, |composer, cx| composer.edit_comment(index, window, cx));
            });
    }
    let edit = match trigger.filter(|_| open) {
        Some(trigger) => components::on_bounds(edit, move |bounds, _, cx| {
            if trigger.replace(Some(bounds)) != Some(bounds) {
                cx.refresh_windows();
            }
        })
        .into_any_element(),
        None => edit.into_any_element(),
    };
    let drawn = chip_shell()
        .when_some(index, |chip, index| {
            chip.debug_selector(move || format!("cite-chip-{index}"))
        })
        .when(open, |chip| chip.bg(theme::paint::SELECTED))
        .child(open_chip)
        .child(edit)
        .child(remove);
    drawn.into_any_element()
}

fn comment_button(id: impl Into<ElementId>, cx: &App) -> gpui::component::button::Button {
    components::form_button(id, cx)
        .h(px(28.))
        .px(px(8.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme::paint::LINE)
        .text_size(px(12.))
        .line_height(px(16.))
}

fn panel(entity: &Entity<Composer>, index: usize, _reply: &Reply, cx: &App) -> AnyElement {
    let composer = entity.read(cx);
    let Some(edit) = composer.citations.editing.as_ref() else {
        return div().into_any_element();
    };
    let field = edit.field.clone();
    let pending = edit.focus_pending.clone();
    let empty = field.read(cx).text().is_empty();
    let invalid = !composer.comment_valid(cx);
    let focused_field = field.clone();
    let pressed_field = field.clone();
    let focus_owner = entity.downgrade();
    let field = components::on_bounds(
        div()
            .relative()
            .min_h(px(52.))
            .max_h(px(148.))
            .when(empty, |body| {
                body.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .text_color(rgb(theme::INK_SUBTLE))
                        .child("Add an optional comment..."),
                )
            })
            .child(field),
        move |_, window, cx| {
            if pending.replace(false) {
                let focus = gpui::Focusable::focus_handle(focused_field.read(cx), cx);
                // The Cockpit claims pending focus before prepaint. A host
                // without that focus policy needs the same handoff after
                // drawing: changing focus here would miss track_focus and
                // Window::refresh is suppressed during prepaint.
                window.on_next_frame(move |window, cx| {
                    if focus_owner.upgrade().is_some_and(|owner| {
                        owner
                            .read(cx)
                            .citations
                            .editing
                            .as_ref()
                            .is_some_and(|edit| edit.field == focused_field)
                    }) {
                        window.focus(&focus, cx);
                    }
                });
            }
        },
    );
    let save = entity.downgrade();
    let cancel = entity.downgrade();
    let save_key = entity.downgrade();
    let cancel_key = entity.downgrade();
    div()
        .id(("cite-panel", index))
        .debug_selector(|| "cite-panel".into())
        .occlude()
        .key_context("CitationPopup")
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down_out({
            let outside = entity.downgrade();
            move |event, _, cx| {
                let _ = outside.update(cx, |composer, cx| {
                    if composer
                        .citations
                        .trigger
                        .get()
                        .is_some_and(|bounds| bounds.contains(&event.position))
                    {
                        return;
                    }
                    composer.dismiss_comment(cx);
                });
            }
        })
        .on_action(move |_: &SaveComment, window: &mut Window, cx: &mut App| {
            let _ = save_key.update(cx, |composer, cx| {
                composer.save_comment(window, cx);
            });
        })
        .on_action(
            move |_: &CancelComment, window: &mut Window, cx: &mut App| {
                let _ = cancel_key.update(cx, |composer, cx| composer.cancel_comment(window, cx));
            },
        )
        .flex()
        .flex_col()
        .w(px(320.))
        .max_w_full()
        .min_w_0()
        .px(px(12.))
        .py(px(8.))
        .rounded(px(8.))
        .bg(rgb(0x111111))
        .shadow(components::float_shadow())
        .border_1()
        .border_color(theme::paint::LINE)
        .font_family(theme::FONT_UI)
        .text_size(px(14.))
        .line_height(px(20.))
        .font_weight(theme::W_BODY)
        .child(
            div()
                .debug_selector(|| "cite-comment-field".into())
                .px(px(4.))
                .py(px(6.))
                .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                    window.focus(
                        &gpui::Focusable::focus_handle(pressed_field.read(cx), cx),
                        cx,
                    );
                })
                .child(field),
        )
        .when(invalid, |panel| {
            panel.child(
                div()
                    .debug_selector(|| "cite-comment-too-long".into())
                    .pt(px(4.))
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .text_color(rgb(theme::RED))
                    .child("Comments can contain up to 8,000 characters."),
            )
        })
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_end()
                .mt(px(8.))
                .gap(px(8.))
                .child(
                    comment_button(("cite-cancel", index), cx)
                        .debug_selector(|| "cite-cancel".into())
                        .key_context("PromptAttachment")
                        .child("Cancel")
                        .on_click(move |_, window, cx| {
                            let _ = cancel
                                .update(cx, |composer, cx| composer.cancel_comment(window, cx));
                        }),
                )
                .child(
                    components::faded_button(
                        ("cite-save", index),
                        rgb(theme::SEND_LIT).into(),
                        rgb(theme::SEND_LIT_HOVER).into(),
                        rgb(theme::SEND_LIT_HOVER).into(),
                        rgb(theme::ON_SEND_LIT).into(),
                        cx,
                    )
                    .disabled(invalid)
                    .when(invalid, |button| button.opacity(0.5))
                    .tab_stop(true)
                    .h(px(28.))
                    .px(px(8.))
                    .text_size(px(12.))
                    .debug_selector(|| "cite-save".into())
                    .key_context("PromptAttachment")
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(theme::ON_SEND_LIT))
                            .child(if invalid { "Shorten comment" } else { "Save" }),
                    )
                    .on_click(move |_, window, cx| {
                        let _ = save.update(cx, |composer, cx| {
                            composer.save_comment(window, cx);
                        });
                    }),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{label, valid_comment};

    #[test]
    fn comment_limit_counts_javascript_utf16_units() {
        assert!(valid_comment(&"x".repeat(8_000)));
        assert!(!valid_comment(&"x".repeat(8_001)));
        assert!(valid_comment(&"😀".repeat(4_000)));
        assert!(!valid_comment(&"😀".repeat(4_001)));
        assert!(!valid_comment(&format!("{} ", "x".repeat(8_000))));
    }

    #[test]
    fn a_short_quote_is_its_words_on_one_line() {
        assert_eq!(label("two\n  lines"), "two lines");
    }

    #[test]
    fn a_long_label_truncates_at_64_utf16_units_without_splitting_a_surrogate() {
        assert_eq!(
            label(&format!("{}😀tail", "x".repeat(63))),
            format!("{}…", "x".repeat(63))
        );
        assert_eq!(label(&"x".repeat(65)), format!("{}…", "x".repeat(64)));
    }
}
