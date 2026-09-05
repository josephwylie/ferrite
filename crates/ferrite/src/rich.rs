//! Native rich text with a stable parser per answer run. Appending tokens
//! advances the toolkit parser; unrelated pane renders do not parse again.
use gpui::base::text::{TextView, TextViewState, TextViewStyle};
use gpui::{prelude::*, px, rems, rgb, rgba, App, Entity, SharedString, Window};

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::theme;

struct CachedText {
    source: String,
    state: Entity<TextViewState>,
    touched: u64,
}

/// Pane-owned native text entities survive temporarily hidden Subjects. The
/// least recently used run is discarded only at this explicit cache limit.
#[derive(Clone, Default)]
pub struct TextCache(Rc<RefCell<(u64, HashMap<SharedString, CachedText>, usize)>>);

impl TextCache {
    pub fn redirect_namespace(&self, from: &str, to: &str) {
        let mut cache = self.0.borrow_mut();
        let keys: Vec<_> = cache
            .1
            .keys()
            .filter_map(|key| {
                ["markdown-", "literal-"].into_iter().find_map(|kind| {
                    key.strip_prefix(&format!("{kind}{from}"))
                        .map(|tail| (key.clone(), SharedString::from(format!("{kind}{to}{tail}"))))
                })
            })
            .collect();
        for (from, to) in keys {
            if let Some(state) = cache.1.remove(&from) {
                if let Some(replaced) = cache.1.insert(to, state) {
                    cache.2 -= replaced.source.len();
                }
            }
        }
    }

    fn state(&self, id: SharedString, source: &str, cx: &mut App) -> Entity<TextViewState> {
        let mut cache = self.0.borrow_mut();
        cache.0 += 1;
        let touched = cache.0;
        let prior_bytes = cache.1.get(&id).map_or(0, |text| text.source.len());
        while (!cache.1.contains_key(&id) && cache.1.len() >= 256)
            || cache.2.saturating_sub(prior_bytes) + source.len() > 8 * 1024 * 1024
        {
            let Some(oldest) = cache
                .1
                .iter()
                .filter(|(key, _)| *key != &id)
                .min_by_key(|(_, text)| text.touched)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            if let Some(old) = cache.1.remove(&oldest) {
                cache.2 -= old.source.len();
            }
        }
        cache.2 = cache.2.saturating_sub(prior_bytes) + source.len();
        let text = cache.1.entry(id).or_insert_with(|| CachedText {
            source: source.to_string(),
            state: cx.new(|cx| TextViewState::markdown(source, cx)),
            touched,
        });
        if text.source != source {
            text.state.update(cx, |state, cx| {
                if let Some(suffix) = source.strip_prefix(&text.source) {
                    state.push_str(suffix, cx);
                } else {
                    state.set_text(source, cx);
                }
            });
            text.source = source.to_string();
        }
        text.touched = touched;
        text.state.clone()
    }
}

#[derive(IntoElement)]
pub struct Markdown {
    id: SharedString,
    source: String,
    cache: TextCache,
}

impl Markdown {
    pub fn new(id: impl Into<SharedString>, source: String, cache: TextCache) -> Self {
        Self {
            id: id.into(),
            source,
            cache,
        }
    }
}

impl gpui::RenderOnce for Markdown {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.cache.state(self.id.clone(), &self.source, cx);
        #[cfg(test)]
        testing::record(self.id, state.clone(), window.text_style().clone(), cx);
        TextView::new(&state)
            .w_full()
            .min_w_0()
            // GPUI 0.6 otherwise gives the document h_full inside an auto-height
            // transcript, creating circular sizing. An unbounded line cap opts
            // into its natural-height path without truncating the document.
            .max_lines(usize::MAX)
            .style(style())
            .code_block_actions(|block, _, _| {
                if !block
                    .lang()
                    .is_some_and(|lang| lang.eq_ignore_ascii_case("html"))
                {
                    return gpui::Empty.into_any_element();
                };
                let html = block.code();
                crate::components::button("preview-html")
                    .child(crate::components::label("Preview", theme::TEXT_2))
                    .on_click(move |_, window, cx| {
                        use gpui::component::WindowExt as _;
                        let html = html.clone();
                        window.open_dialog(cx, move |dialog, _, _| {
                            dialog
                                .title("HTML preview")
                                .width(px(720.))
                                .bg(rgb(theme::MENU))
                                .child(
                                    gpui::div()
                                        .id("html-preview")
                                        .font_family(theme::FONT_MONO)
                                        .max_h(px(520.))
                                        .overflow_y_scroll()
                                        .child(
                                            TextView::html("html-preview-text", html.clone())
                                                .style(style()),
                                        ),
                                )
                        });
                    })
                    .into_any_element()
            })
            .into_any_element()
    }
}

pub fn style() -> TextViewStyle {
    TextViewStyle::default()
        .with_dark(true)
        .with_foreground(rgb(theme::TEXT_2).into())
        .with_muted_foreground(rgb(theme::TEXT_MUTED).into())
        .with_link(rgb(theme::TEXT_STRONG).into())
        .with_selection(rgba(theme::TEXT_SELECTION_WASH).into())
        .with_code_background(rgb(theme::RAISED).into())
        .with_inline_code(gpui::HighlightStyle {
            color: Some(rgb(theme::TEXT).into()),
            background_color: Some(rgb(theme::RAISED).into()),
            ..Default::default()
        })
        .with_border(rgb(theme::TABLE_RULE).into())
        .with_table(
            gpui::StyleRefinement::default()
                .bg(rgb(theme::PANE))
                .border_0(),
        )
        .with_table_head(
            gpui::StyleRefinement::default()
                .bg(rgb(theme::PANE))
                .text_color(rgb(theme::TEXT))
                .font_weight(gpui::FontWeight::SEMIBOLD),
        )
        .with_table_cell(
            gpui::StyleRefinement::default()
                .border_0()
                .px(px(8.))
                .py(px(6.)),
        )
        .with_paragraph_gap(rems(theme::P_MARGIN_B / 16.))
        .with_heading_base_font_size(px(theme::FS_MD))
        .with_heading_font_size(|level, base| {
            base * match level {
                1 => 1.5,
                2 => 1.3,
                _ => 1.15,
            }
        })
}

/// Literal provider output shares Markdown's selection engine and keeps
/// the parent's typography. A collision-free code fence preserves exact text.
#[derive(IntoElement)]
pub struct Literal {
    pub id: SharedString,
    pub text: SharedString,
    pub highlights: Vec<(std::ops::Range<usize>, gpui::HighlightStyle)>,
    pub cache: TextCache,
}

impl gpui::RenderOnce for Literal {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let inherited = window.text_style();
        let fence = "`".repeat(
            self.text
                .split(|c| c != '`')
                .map(str::len)
                .max()
                .unwrap_or(0)
                .saturating_add(1)
                .max(3),
        );
        let source = format!("{fence}\n{}\n{fence}", self.text);
        let highlights = self.highlights;
        let style = style()
            .with_foreground(inherited.color)
            .with_paragraph_gap(rems(0.))
            .with_code_background(gpui::transparent_black())
            .with_code_block(
                gpui::StyleRefinement::default()
                    .p_0()
                    .font_family(inherited.font_family.clone())
                    .text_size(inherited.font_size.to_pixels(window.rem_size()))
                    .line_height(inherited.line_height_in_pixels(window.rem_size())),
            );
        let state = self.cache.state(self.id.clone(), &source, cx);
        #[cfg(test)]
        testing::record(self.id, state.clone(), inherited.clone(), cx);
        TextView::new(&state)
            .max_lines(usize::MAX)
            .style(style)
            .code_block_highlighter(move |_| highlights.clone())
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Views(HashMap<SharedString, (Entity<TextViewState>, gpui::TextStyle)>);
    impl gpui::Global for Views {}

    pub fn first_entity(prefix: &str, cx: &App) -> Option<gpui::EntityId> {
        cx.global::<Views>()
            .0
            .iter()
            .find(|(id, _)| id.starts_with(prefix))
            .map(|(_, (state, _))| state.entity_id())
    }

    pub fn record(
        id: SharedString,
        state: Entity<TextViewState>,
        style: gpui::TextStyle,
        cx: &mut App,
    ) {
        if cx.try_global::<Views>().is_none() {
            cx.set_global(Views::default());
        }
        cx.global_mut::<Views>().0.insert(id, (state, style));
    }

    pub fn bounds(id: &str, item: usize, cx: &App) -> Option<gpui::Bounds<gpui::Pixels>> {
        cx.global::<Views>()
            .0
            .get(id)?
            .0
            .read(cx)
            .list_state()
            .bounds_for_item(item)
            .or_else(|| {
                (item == 0).then(|| cx.global::<Views>().0.get(id).unwrap().0.read(cx).bounds())
            })
    }

    pub fn select_all(cx: &mut App) {
        let views: Vec<_> = cx
            .global::<Views>()
            .0
            .values()
            .map(|(state, _)| state.clone())
            .collect();
        for state in views {
            state.update(cx, |state, cx| state.select_all(cx));
        }
    }

    // Aim within simple paragraph fixtures using native view bounds and font metrics.
    pub fn caret(
        id: &str,
        item: usize,
        paragraphs: usize,
        text: &str,
        byte: usize,
        window: &Window,
        cx: &App,
    ) -> Option<gpui::Point<gpui::Pixels>> {
        let (state, style) = cx.global::<Views>().0.get(id)?;
        let mut bounds = state.read(cx).bounds();
        let gap = window.rem_size() * (theme::P_MARGIN_B / 16.);
        let stride = (bounds.size.height + gap) / paragraphs.max(1) as f32;
        let line_height = stride - gap;
        bounds.origin.y += stride * item as f32;
        let size = style.font_size.to_pixels(window.rem_size());
        let run = gpui::TextRun {
            len: text.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window
            .text_system()
            .shape_line(text.to_string().into(), size, &[run], None);
        Some(gpui::point(
            bounds.left() + line.x_for_index(byte) + px(0.5),
            bounds.top() + line_height * 0.5,
        ))
    }
}
