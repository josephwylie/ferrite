//! Native rich text with a stable parser per answer run. Appending tokens
//! advances the toolkit parser; unrelated pane renders do not parse again.
use gpui::base::text::{TextView, TextViewState, TextViewStyle};
use gpui::component::input::{Textarea, TextareaState};
use gpui::{prelude::*, px, rems, rgb, rgba, App, Entity, Focusable, SharedString, Window};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::theme;

#[derive(Clone)]
enum NativeText {
    Rich(Entity<TextViewState>),
    Output(Entity<TextareaState>),
}

struct CachedText {
    source: String,
    state: NativeText,
    touched: u64,
}

/// Pane-owned native text entities survive temporarily hidden Subjects. The
/// least recently used run is discarded only at this explicit cache limit.
#[derive(Clone, Default)]
pub struct TextCache(Rc<RefCell<(u64, HashMap<SharedString, CachedText>, usize)>>);

impl TextCache {
    pub fn output_focused(&self, namespace: &str, window: &Window, cx: &App) -> bool {
        let prefix = format!("output-{namespace}-");
        self.0.borrow().1.iter().any(|(id, text)| {
            id.starts_with(&prefix)
                && matches!(&text.state,
                NativeText::Output(state) if state.focus_handle(cx).is_focused(window))
        })
    }

    pub fn clear_output_selection(&self, namespace: &str, cx: &mut App) {
        let prefix = format!("output-{namespace}-");
        for (id, text) in &self.0.borrow().1 {
            if let NativeText::Output(state) = &text.state {
                if id.starts_with(&prefix) && !state.read(cx).selected_range().is_empty() {
                    state.update(cx, |state, cx| {
                        let cursor = state.cursor();
                        state.set_selected_range(cursor..cursor, cx);
                    });
                }
            }
        }
    }

    pub fn redirect_namespace(&self, from: &str, to: &str) {
        let mut cache = self.0.borrow_mut();
        let keys: Vec<_> = cache
            .1
            .keys()
            .filter_map(|key| {
                ["markdown-", "literal-", "thinking-", "output-"]
                    .into_iter()
                    .find_map(|kind| {
                        key.strip_prefix(&format!("{kind}{from}")).map(|tail| {
                            (key.clone(), SharedString::from(format!("{kind}{to}{tail}")))
                        })
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

    fn cached(
        &self,
        id: SharedString,
        source: &str,
        output: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> NativeText {
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
            state: if output {
                NativeText::Output(cx.new(|cx| {
                    let mut state = TextareaState::new(window, cx).auto_grow(1, 12);
                    state.set_value(source.to_string(), window, cx);
                    state
                }))
            } else {
                NativeText::Rich(cx.new(|cx| TextViewState::markdown(source, cx)))
            },
            touched,
        });
        if text.source != source {
            match &text.state {
                NativeText::Rich(state) => state.update(cx, |state, cx| {
                    if let Some(suffix) = source.strip_prefix(&text.source) {
                        state.push_str(suffix, cx);
                    } else {
                        state.set_text(source, cx);
                    }
                }),
                NativeText::Output(state) => state.update(cx, |state, cx| {
                    let mut selected = state.selected_range();
                    // The native range setter accepts anchor → caret order.
                    // Keep backward selections backward across new chunks.
                    if !selected.is_empty() && state.cursor() == selected.start {
                        selected = selected.end..selected.start;
                    }
                    let scroll = state.scroll_offset();
                    state.set_value(source.to_string(), window, cx);
                    // Native setters clip replacement offsets to UTF-8 and
                    // defer scroll clamping until the new layout is ready.
                    state.set_selected_range(selected, cx);
                    state.set_scroll_offset(scroll, cx);
                }),
            }
            text.source = source.to_string();
        }
        text.touched = touched;
        text.state.clone()
    }

    fn state(
        &self,
        id: SharedString,
        source: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<TextViewState> {
        let NativeText::Rich(state) = self.cached(id, source, false, window, cx) else {
            unreachable!("rich text and output have separate namespaces")
        };
        state
    }
}

#[derive(IntoElement)]
pub struct Markdown {
    id: SharedString,
    source: String,
    cache: TextCache,
    muted: bool,
}

impl Markdown {
    pub fn muted(mut self) -> Self {
        self.muted = true;
        self
    }
    pub fn new(id: impl Into<SharedString>, source: String, cache: TextCache) -> Self {
        Self {
            id: id.into(),
            source,
            cache,
            muted: false,
        }
    }
}

impl gpui::RenderOnce for Markdown {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.cache.state(self.id.clone(), &self.source, _window, cx);
        #[cfg(test)]
        testing::record(self.id, state.clone(), _window.text_style().clone(), cx);
        let text_style = if self.muted {
            style().with_foreground(rgb(theme::TEXT_MUTED).into())
        } else {
            style()
        };
        TextView::new(&state)
            .w_full()
            .min_w_0()
            // Use natural height inside the transcript's own scroll container.
            .max_lines(usize::MAX)
            .style(text_style)
            .code_block_highlighter(|block| {
                let source = block.code();
                let language = block.lang();
                let tokens =
                    ferrite_core::transcript::highlight_tokens(language.as_deref(), &source);
                crate::pane::code(&source, Some(&tokens))
            })
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
        let state = self.cache.state(self.id.clone(), &source, window, cx);
        #[cfg(test)]
        testing::record(self.id, state.clone(), inherited.clone(), cx);
        TextView::new(&state)
            .w_full()
            .min_w_0()
            // Use natural height inside the transcript's own scroll container.
            .max_lines(usize::MAX)
            .style(style)
            .code_block_highlighter(move |_| highlights.clone())
    }
}

/// Large tool disclosures use the toolkit's virtualized text input layout.
/// One exact source preserves Unicode/newlines and avoids Markdown's per-glyph
/// selection hit regions for very long lines. The native control owns copying.
#[derive(IntoElement)]
pub struct Output {
    pub id: SharedString,
    pub text: SharedString,
    pub cache: TextCache,
}

impl gpui::RenderOnce for Output {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let NativeText::Output(state) =
            self.cache
                .cached(self.id.clone(), &self.text, true, window, cx)
        else {
            unreachable!("output has its own namespace")
        };
        #[cfg(test)]
        testing::record_output(self.id, state.clone(), cx);
        Textarea::new(&state)
            .readonly(true)
            .appearance(false)
            .bordered(false)
            .aria_label("Tool output")
            .w_full()
            .min_w_0()
            .p_0()
            .font_family(window.text_style().font_family.clone())
            .text_size(window.text_style().font_size.to_pixels(window.rem_size()))
            .text_color(window.text_style().color)
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Views(HashMap<SharedString, (Entity<TextViewState>, gpui::TextStyle)>);
    impl gpui::Global for Views {}

    #[derive(Default)]
    struct Outputs(HashMap<SharedString, Entity<TextareaState>>);
    impl gpui::Global for Outputs {}

    pub fn record_output(id: SharedString, state: Entity<TextareaState>, cx: &mut App) {
        if cx.try_global::<Outputs>().is_none() {
            cx.set_global(Outputs::default());
        }
        cx.global_mut::<Outputs>().0.insert(id, state);
    }

    pub fn output(id: &str, cx: &App) -> Option<Entity<TextareaState>> {
        cx.try_global::<Outputs>()?.0.get(id).cloned()
    }

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
