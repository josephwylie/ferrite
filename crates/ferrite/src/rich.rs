//! Native rich text with a stable parser per answer run. Appending tokens
//! advances the toolkit parser; unrelated pane renders do not parse again.
use gpui::base::text::{TextView, TextViewState, TextViewStyle};
use gpui::{prelude::*, px, rems, rgb, rgba, App, Context, Entity, SharedString, Window};

use crate::theme;

struct CachedText {
    source: String,
    state: Entity<TextViewState>,
}

#[derive(IntoElement)]
pub struct Markdown {
    id: SharedString,
    source: String,
}

impl Markdown {
    pub fn new(id: impl Into<SharedString>, source: String) -> Self {
        Self {
            id: id.into(),
            source,
        }
    }
}

impl gpui::RenderOnce for Markdown {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let cache =
            window.use_keyed_state(self.id.clone(), cx, |_, cx: &mut Context<CachedText>| {
                CachedText {
                    source: self.source.clone(),
                    state: cx.new(|cx| TextViewState::markdown(&self.source, cx)),
                }
            });
        let state = cache.update(cx, |cache, cx| {
            if cache.source != self.source {
                cache.state.update(cx, |state, cx| {
                    if let Some(suffix) = self.source.strip_prefix(&cache.source) {
                        state.push_str(suffix, cx);
                    } else {
                        state.set_text(&self.source, cx);
                    }
                });
                cache.source = self.source;
            }
            cache.state.clone()
        });
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
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| {
            TextViewState::markdown(&source, cx)
        });
        state.update(cx, |state, cx| state.set_text(&source, cx));
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
