//! Cite a selection, as T3 does (`AssistantSelectionToolbar`): a drag that
//! selects a passage of the transcript (anyone's words, the operator's or
//! the agent's) ends with a glass button at the pointer, `Cite`, which
//! quotes the passage into the Composer (`TranscriptEvent::Reply`, a Reply
//! card in the Pane's tray). A passage too long to quote offers a disabled
//! `Shorten selection` instead.
//!
//! The button is placed where the mouse went up, inside the window by
//! `SELECTION_CITE_EDGE`; where the pointer is not on the selection (a
//! keyboard or a far release) it stands at the selection's last line's end,
//! `SELECTION_CITE_BELOW` under it. It shows at once, or
//! `SELECTION_CITE_MULTI_CLICK_MS` after a double or triple click. A press
//! clears the selection (the text selection's capture phase), so the
//! button acts on the press, not the click. It goes when the selection
//! does (a press anywhere else), on Escape, and a scroll fades it out over
//! `FADE_QUICK`.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    prelude::*, px, AnyElement, Bounds, Context, MouseButton, MouseDownEvent, MouseUpEvent, Pixels,
    Point, SharedString, Size, Window,
};

use super::{Navigate, TranscriptEvent, TranscriptView};
use crate::select::FindMarks;
use ferrite_core::replies::Reply;
use ferrite_core::transcript::BlockId;
use ferrite_ui::{icons, motion, theme};

/// The button on offer: the passage it quotes (none when the selection is
/// too long to), read when the drag ended (a press clears the selection
/// before a click could land), where the pointer let go, when it appears,
/// and when a scroll began to fade it.
pub(super) struct SelectionReply {
    /// A reference scene's, offered with no selection behind it.
    staged: bool,
    anchor: Option<gpui::base::TextSelectionAnchor>,
    passage: Option<String>,
    /// The Block under the selection's first line, the citation's source.
    block: Option<BlockId>,
    at: Point<Pixels>,
    appears: Instant,
    fading: Option<Instant>,
}

/// What a Go to source landed on within its row: the cited words, marked
/// as a Find occurrence (`FindMarks`), and the frames left to wait for them
/// to paint and report where.
pub(super) struct CitedPassage {
    pub(super) marks: FindMarks,
    reveal: u8,
}

/// The row holding `block`.
pub(super) fn row_of(rows: &super::TranscriptRows, block: BlockId) -> Option<usize> {
    rows.rows()
        .iter()
        .position(|row| row.blocks().iter().any(|candidate| candidate.id == block))
}

impl TranscriptView {
    /// A left press over the transcript: where a drag would select from.
    pub(super) fn pressed(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Left {
            self.pressed_at = Some(event.position);
        }
    }

    /// A press let go over the transcript: offer the Cite once the
    /// selection it finished has settled, if it selected a passage.
    pub(super) fn released(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pressed_at = None;
        if !self.input.presentation.focused || !self.input.presentation.quotes {
            return;
        }
        let at = event.position;
        let delay = if event.click_count >= 2 {
            Duration::from_millis(theme::SELECTION_CITE_MULTI_CLICK_MS)
        } else {
            Duration::ZERO
        };
        cx.defer_in(window, move |view, window, cx| {
            let selected = gpui::base::TextSelection::selected_text(window, cx);
            let passage = crate::quote::passage(&selected);
            let block = gpui::base::TextSelection::painted_bounds(window, cx)
                .first()
                .and_then(|first| view.block_at(first.center().y));
            view.reply =
                (passage.is_some() || crate::quote::too_long(&selected)).then(|| SelectionReply {
                    staged: false,
                    anchor: gpui::base::TextSelection::retain_range(window, cx),
                    passage,
                    block,
                    at,
                    appears: cx.background_executor().now() + delay,
                    fading: None,
                });
            if !delay.is_zero() {
                cx.spawn(async move |view, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = view.update(cx, |_, cx| cx.notify());
                })
                .detach();
            }
            cx.notify();
        });
    }

    /// A scroll under the button: it fades out, then goes (at once under
    /// reduced motion).
    pub(super) fn scrolled(&mut self, cx: &mut Context<Self>) {
        let Some(reply) = self.reply.as_mut() else {
            return;
        };
        if reply.fading.is_some() {
            return;
        }
        if motion::reduced_motion(cx) {
            self.dismiss_reply(cx);
            return;
        }
        reply.fading = Some(cx.background_executor().now());
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(motion::FADE_QUICK.duration())
                .await;
            let _ = view.update(cx, |view, cx| view.dismiss_reply(cx));
        })
        .detach();
        cx.notify();
    }

    pub(super) fn dismiss_reply(&mut self, cx: &mut Context<Self>) {
        if self.reply.take().is_some() {
            cx.notify();
        }
    }

    /// Escape: the button goes, else an open message or link menu. Whether
    /// either was up.
    pub fn dismiss_offer(&mut self, cx: &mut Context<Self>) -> bool {
        let offered = self.reply.is_some();
        self.dismiss_reply(cx);
        if offered {
            return true;
        }
        let menu_open = self.message_menu.take().is_some();
        if menu_open {
            cx.notify();
        }
        menu_open
    }

    fn cite_passage(
        &mut self,
        passage: String,
        block: Option<BlockId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let anchor = self.reply.as_ref().and_then(|reply| reply.anchor.clone())
            .or_else(|| gpui::base::TextSelection::retain_range(window, cx));
        gpui::base::TextSelection::clear(window, cx);
        self.reply = None;
        let mut reply = Reply::new(passage, "");
        if let Some((key, selector)) = anchor.as_ref().and_then(|anchor| anchor.selector()) {
            let part = block.and_then(|block| self.row_holding(block)).and_then(|row| {
                self.texts.iter().filter(|(at, _)| *at == row).position(|(_, text)| text.key == key)
            }).unwrap_or(0);
            reply.selector = Some(ferrite_core::replies::Selector { part, start: selector.start, end: selector.end, prefix: selector.prefix, suffix: selector.suffix });
        }
        cx.emit(TranscriptEvent::Reply(reply, block, anchor));
        cx.notify();
    }

    /// Cite from the keyboard: the passage on offer, else the selection as
    /// it stands. Whether there was a passage to quote.
    pub fn cite_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.input.presentation.quotes {
            return false;
        }
        let offered = self
            .reply
            .as_ref()
            .and_then(|reply| reply.passage.clone().map(|passage| (passage, reply.block)));
        let (passage, block) = match offered {
            Some(offered) => offered,
            None => {
                let selected = gpui::base::TextSelection::selected_text(window, cx);
                let Some(passage) = crate::quote::passage(&selected) else {
                    return false;
                };
                let block = gpui::base::TextSelection::painted_bounds(window, cx)
                    .first()
                    .and_then(|first| self.block_at(first.center().y));
                (passage, block)
            }
        };
        self.cite_passage(passage, block, window, cx);
        true
    }

    /// The Block the row under window `y` begins with: what a citation
    /// there names as its source.
    fn block_at(&self, y: Pixels) -> Option<BlockId> {
        let state = self.scroll.list_state();
        let first = state.logical_scroll_top().item_ix;
        (first..self.rows.rows().len()).find_map(|index| {
            let bounds = state.bounds_for_item(index)?;
            (bounds.top() <= y && y < bounds.bottom())
                .then(|| self.rows.get(index)?.blocks().first().map(|block| block.id))
                .flatten()
        })
    }

    pub fn comment_source(&self, block: BlockId) -> Option<(gpui::base::TextSelectionDocument, Vec<SharedString>)> {
        let row = self.row_holding(block)?;
        Some((self.document.clone(), self.texts.iter().filter(|(at, _)| *at == row).map(|(_, text)| text.key.clone()).collect()))
    }

    /// Bring the cited `passage` of the row holding `block` into view: left
    /// where it lies when it is fully readable, else settled a third of the
    /// way down the view as a Find hit is, and marked. A passage the row's
    /// text does not hold lands the row's top below the pinned band instead.
    /// Says whether the Block is still in this transcript (a replayed or
    /// evicted history may have dropped it).
    pub fn reveal_passage(&mut self, block: BlockId, quote: &str, cx: &mut Context<Self>) -> bool {
        let Some(row) = self.row_holding(block) else {
            return false;
        };
        let query = crate::find::Query::new(quote);
        let holder = self
            .texts
            .iter()
            .find(|(at, text)| {
                *at == row
                    && !query.needle.is_empty()
                    && !query.matches(&text.find_text()).is_empty()
            })
            .map(|(_, text)| text.key.clone());
        match holder {
            Some(key) => {
                let report = Rc::new(gpui::base::text::FindReport::default());
                self.navigate(Navigate::Reveal { row }, cx);
                self.cited = Some(CitedPassage {
                    marks: FindMarks {
                        query,
                        active: Some((key, 0)),
                        report,
                    },
                    reveal: crate::find::REVEAL_FRAMES,
                });
            }
            None => self.navigate(Navigate::Cite { row }, cx),
        }
        cx.notify();
        true
    }

    /// Each render while a landing is due: scroll the passage into the
    /// reading band once it has painted (`settle_hit`). The settle is this
    /// landing's own movement, so the mark it set survives it.
    pub(super) fn settle_cited(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(cited) = self.cited.as_mut() else {
            return;
        };
        let found = super::find::painted(&mut cited.reveal, &cited.marks.report, window);
        if found.is_none() && cited.reveal > 0 {
            // An animation frame alone can reuse this cached view before the mark reports.
            cx.defer_in(window, |_, _, cx| cx.notify());
        }
        if let Some(found) = found {
            let cited = self.cited.take();
            self.settle_hit(found, cx);
            self.cited = cited;
        }
    }

    /// Where the landed passage last painted, as the tests read it.
    #[cfg(any(test, feature = "test-support"))]
    pub fn cited_painted(&self) -> Option<gpui::Bounds<Pixels>> {
        self.cited.as_ref()?.marks.report.revealed.get()
    }

    /// Whether `block` is in this transcript, as the tests read it.
    #[cfg(any(test, feature = "test-support"))]
    pub fn holds_block(&self, block: BlockId) -> bool {
        self.row_holding(block).is_some()
    }

    fn row_holding(&self, block: BlockId) -> Option<usize> {
        row_of(&self.rows, block)
    }

    /// Where the row holding `block` is drawn, as the tests read it.
    #[cfg(any(test, feature = "test-support"))]
    pub fn block_row_bounds(&self, block: BlockId) -> Option<gpui::Bounds<Pixels>> {
        self.scroll
            .list_state()
            .bounds_for_item(self.row_holding(block)?)
    }

    /// A sent Quote row's Go to source: scroll to the Block it was cited
    /// from, or have the row say it is gone.
    fn open_cited(&mut self, block: BlockId, quote: &str, cx: &mut Context<Self>) {
        if !self.reveal_passage(block, quote, cx) {
            self.missing_sources.insert(block);
            cx.notify();
        }
    }

    /// Where each Quote row of the prompt `line` was cited from.
    pub(super) fn quote_sources(
        &self,
        line: &str,
        view: Option<&gpui::Entity<Self>>,
    ) -> Vec<Option<crate::attachments::QuoteSource>> {
        let Some(view) = view.map(gpui::Entity::downgrade) else {
            return Vec::new();
        };
        ferrite_core::replies::split(line)
            .0
            .iter()
            .map(|reply| {
                let key = ferrite_core::cited_sources::key(&reply.quote);
                let block = *self.input.presentation.cited.get(&key)?;
                let view = view.clone();
                let quote = reply.quote.clone();
                Some(crate::attachments::QuoteSource {
                    missing: self.missing_sources.contains(&block),
                    go: std::rc::Rc::new(move |_, cx| {
                        let _ = view.update(cx, |transcript, cx| transcript.open_cited(block, &quote, cx));
                    }),
                })
            })
            .collect()
    }

    /// The button on offer at `at` for `passage` (a reference scene's: no
    /// pointer drags a selection there).
    #[cfg(feature = "visual-reference")]
    pub fn fixture_cite(&mut self, passage: &str, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.reply = Some(SelectionReply {
            staged: true,
            anchor: None,
            passage: Some(passage.to_string()),
            block: None,
            at,
            appears: cx.background_executor().now(),
            fading: None,
        });
        cx.notify();
    }

    /// Whether the Cite button is on offer — what the tests read.
    #[cfg(any(test, feature = "test-support"))]
    pub fn reply_offered(&self) -> bool {
        self.reply.is_some()
    }

    /// The button, while its selection stands: at the pointer
    /// (`cite_place`), fading once a scroll has begun, none before it
    /// appears.
    pub(super) fn reply_float(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.reply.is_none() {
            return None;
        }
        let staged = self.reply.as_ref().is_some_and(|reply| reply.staged);
        if !staged
            && (!self.input.presentation.focused
                || !gpui::base::TextSelection::has_selection(window, cx))
        {
            self.reply = None;
            return None;
        }
        let now = cx.background_executor().now();
        let reply = self.reply.as_ref()?;
        if now < reply.appears {
            return None;
        }
        let opacity = match reply.fading {
            Some(since) => {
                window.request_animation_frame();
                1. - motion::FADE_QUICK.progress_at(now.saturating_duration_since(since))
            }
            None => 1.,
        };
        let size = cite_size(reply.passage.is_none(), window);
        let rects = gpui::base::TextSelection::painted_bounds(window, cx);
        let place = cite_place(
            self.scroll.list_state().viewport_bounds(),
            &rects,
            reply.at,
            size,
            window.viewport_size(),
        );
        let passage = reply.passage.clone();
        let block = reply.block;
        let fading = reply.fading.is_some();
        let weak = cx.entity().downgrade();
        let away = weak.clone();
        let namespace = self.input.presentation.namespace.clone();
        let id = gpui::ElementId::from(SharedString::from(format!(
            "selection-reply-{}",
            self.input.presentation.namespace
        )));
        // On the press, not the click: the press clears the selection,
        // and with it this button, before a release could land on it.
        let cite = match passage {
            Some(passage) => glass_button(&namespace, "selection-reply-quote", "Cite", true)
                .cursor_pointer()
                .when(!fading, |button| {
                    let weak = weak.clone();
                    button.on_mouse_down(
                        MouseButton::Left,
                        move |_: &MouseDownEvent, window: &mut Window, cx: &mut gpui::App| {
                            cx.stop_propagation();
                            let passage = passage.clone();
                            let _ = weak.update(cx, |view, cx| {
                                view.cite_passage(passage, block, window, cx)
                            });
                        },
                    )
                }),
            None => glass_button(
                &namespace,
                "selection-reply-quote",
                "Shorten selection",
                false,
            )
            .opacity(0.5)
            .tooltip(ferrite_ui::menu::tooltip("Selection is too long to cite")),
        };
        let row = gpui::div()
            .id(id)
            .debug_selector(|| "selection-reply".into())
            .flex()
            .opacity(opacity)
            .occlude()
            .on_mouse_down_out(move |_, _, cx| {
                let _ = away.update(cx, |view, cx| view.dismiss_reply(cx));
            })
            .child(cite);
        Some(
            gpui::deferred(
                gpui::anchored()
                    .anchor(gpui::Anchor::TopLeft)
                    .position(place)
                    .snap_to_window()
                    .child(row),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}

/// T3's `resolveSelectionActionPosition` then its layout clamp: the button's
/// top-left is the pointer where the pointer is on the selection (within
/// `SELECTION_CITE_NEAR` of a line of it), else the last line's end,
/// `SELECTION_CITE_BELOW` under it; held in `column`, then
/// `SELECTION_CITE_EDGE` inside the window with the button's `size`.
fn cite_place(
    column: Bounds<Pixels>,
    rects: &[Bounds<Pixels>],
    pointer: Point<Pixels>,
    size: Size<Pixels>,
    window: Size<Pixels>,
) -> Point<Pixels> {
    let near = px(theme::SELECTION_CITE_NEAR);
    let on_text = rects.iter().any(|rect| {
        pointer.x >= rect.left() - near
            && pointer.x <= rect.right() + near
            && pointer.y >= rect.top() - near
            && pointer.y <= rect.bottom() + near
    });
    let preferred = match rects.last() {
        Some(last) if !on_text => gpui::point(
            last.right(),
            last.bottom() + px(theme::SELECTION_CITE_BELOW),
        ),
        _ => pointer,
    };
    let edge = px(theme::SELECTION_CITE_EDGE);
    let held = |value: Pixels, low: Pixels, high: Pixels, limit: Pixels, extent: Pixels| {
        let value = value.max(low).min(high).min(limit - edge);
        value.min(limit - extent - edge).max(edge)
    };
    gpui::point(
        held(
            preferred.x,
            column.left(),
            column.right(),
            window.width,
            size.width,
        ),
        held(
            preferred.y,
            column.top(),
            column.bottom(),
            window.height,
            size.height,
        ),
    )
}

/// The button's size: `SELECTION_CITE_H` tall, its words shaped at
/// `FS_BADGE` after Cite's icon, each side padded `SELECTION_CITE_PAD_X`
/// inside a 1px ring.
fn cite_size(too_long: bool, window: &mut Window) -> Size<Pixels> {
    let mut face = window.text_style();
    face.font_family = theme::FONT_UI.into();
    face.font_weight = theme::W_LABEL;
    let words = if too_long { "Shorten selection" } else { "Cite" };
    let mut width = window
        .text_system()
        .shape_line(
            words.into(),
            px(theme::FS_BADGE),
            &[face.to_run(words.len())],
            None,
        )
        .width()
        + px(2. * theme::SELECTION_CITE_PAD_X + 2.);
    if !too_long {
        width += px(theme::SELECTION_CITE_ICON + theme::SELECTION_CITE_ICON_GAP);
    }
    gpui::size(width.ceil(), px(theme::SELECTION_CITE_H))
}

/// T3's `Button size="xs" variant="glass"`: `SELECTION_CITE_H` tall, 12px
/// medium words, fully rounded inside a 1px `FLOAT_EDGE` ring, a small
/// shadow, on `paint::FLOAT` (GPUI has no backdrop blur to give the glass).
/// `quote`: the Quote icon leads it. Lit `FLOAT_HOVER` under the pointer.
fn glass_button(
    namespace: &str,
    selector: &'static str,
    label: &'static str,
    quote: bool,
) -> gpui::Stateful<gpui::Div> {
    let id = gpui::ElementId::from(SharedString::from(format!("{selector}-{namespace}")));
    let key = ferrite_ui::pointer::hover_key(&id);
    let t = motion::hover_t(&key);
    if t > 0. && t < 1. {
        crate::file_links::mark_fading();
    }
    gpui::div()
        .id(id)
        .debug_selector(move || selector.into())
        .flex_none()
        .flex()
        .items_center()
        .gap(px(theme::SELECTION_CITE_ICON_GAP))
        .h(px(theme::SELECTION_CITE_H))
        .px(px(theme::SELECTION_CITE_PAD_X))
        .rounded_full()
        .border_1()
        .border_color(theme::paint::FLOAT_EDGE)
        .shadow(ferrite_ui::components::float_shadow())
        .bg(motion::mix(
            theme::paint::FLOAT.into(),
            theme::paint::FLOAT_HOVER.into(),
            t,
        ))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_BADGE))
        .font_weight(theme::W_LABEL)
        .text_color(gpui::rgb(theme::INK_STRONG))
        .whitespace_nowrap()
        .on_hover(motion::hover_listener(key))
        .when(quote, |button| {
            button.child(icons::icon(
                icons::QUOTE,
                theme::SELECTION_CITE_ICON,
                theme::INK_SUBTLE,
            ))
        })
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(gpui::point(px(x), px(y)), gpui::size(px(w), px(h)))
    }

    fn place(rects: &[Bounds<Pixels>], pointer: (f32, f32)) -> Point<Pixels> {
        cite_place(
            bounds(100., 40., 700., 500.),
            rects,
            gpui::point(px(pointer.0), px(pointer.1)),
            gpui::size(px(80.), px(24.)),
            gpui::size(px(1000.), px(700.)),
        )
    }

    #[test]
    fn the_button_stands_at_the_pointer_on_the_selection() {
        let at = place(&[bounds(200., 100., 60., 20.)], (250., 110.));
        assert_eq!((at.x, at.y), (px(250.), px(110.)));
    }

    #[test]
    fn a_pointer_off_the_selection_puts_it_under_the_last_line_end() {
        let at = place(
            &[bounds(200., 100., 60., 20.), bounds(200., 120., 40., 20.)],
            (600., 400.),
        );
        assert_eq!((at.x, at.y), (px(240.), px(144.)));
    }

    #[test]
    fn it_is_held_in_the_column_and_inside_the_window() {
        let rects = [bounds(200., 100., 60., 20.)];
        let left = place(&rects, (195., 110.));
        assert_eq!(left.x, px(195.), "the pointer, a little left of the text");
        let far = cite_place(
            bounds(0., 0., 1000., 700.),
            &[bounds(950., 680., 40., 15.)],
            gpui::point(px(990.), px(695.)),
            gpui::size(px(80.), px(24.)),
            gpui::size(px(1000.), px(700.)),
        );
        assert_eq!((far.x, far.y), (px(912.), px(668.)));
        let corner = cite_place(
            bounds(0., 0., 1000., 700.),
            &[bounds(0., 0., 40., 15.)],
            gpui::point(px(0.), px(0.)),
            gpui::size(px(80.), px(24.)),
            gpui::size(px(1000.), px(700.)),
        );
        assert_eq!((corner.x, corner.y), (px(8.), px(8.)));
    }
}
