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
pub struct TextCache(
    Rc<RefCell<(u64, HashMap<SharedString, CachedText>, usize)>>,
    Rc<
        RefCell<(
            Option<std::path::PathBuf>,
            Option<crate::attachment_preview::Preview>,
        )>,
    >,
);

impl TextCache {
    pub fn file_context(
        &self,
        cwd: Option<&std::path::Path>,
        preview: &crate::attachment_preview::Preview,
    ) {
        *self.1.borrow_mut() = (cwd.map(std::path::Path::to_path_buf), Some(preview.clone()));
    }

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
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.cache.state(self.id.clone(), &self.source, window, cx);
        #[cfg(test)]
        testing::record(self.id, state.clone(), window.text_style().clone(), cx);
        let text_style = if self.muted {
            style(window.rem_size()).with_foreground(rgb(theme::TEXT_2).into())
        } else {
            style(window.rem_size()).with_foreground(rgb(theme::TEXT).into())
        };
        let (cwd, preview) = self.cache.1.borrow().clone();
        let link_cwd = cwd.clone();
        TextView::new(&state)
            .link_renderer(move |url, label, window, cx| {
                let file = crate::file_links::FileLink::resolve(url, cwd.as_deref())?;
                Some(crate::attachments::inline_file(
                    file,
                    label,
                    preview.as_ref(),
                    window,
                    cx,
                ))
            })
            .font_family(theme::FONT_UI)
            .w_full()
            .min_w_0()
            // Use natural height inside the transcript's own scroll container.
            .max_lines(usize::MAX)
            .style(text_style)
            .on_link_click(move |url, event, window, cx| {
                let activate = match event {
                    gpui::ClickEvent::Mouse(click) => matches!(
                        click.up.button,
                        gpui::MouseButton::Left | gpui::MouseButton::Middle
                    ),
                    gpui::ClickEvent::Keyboard(_) => true,
                    gpui::ClickEvent::Touch(click) => !click.long_press,
                };
                if !activate {
                    return;
                }
                if let Some(file) = crate::file_links::FileLink::resolve(url, link_cwd.as_deref()) {
                    file.open(window, cx);
                } else {
                    cx.open_url(url);
                }
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
                        window.open_dialog(cx, move |dialog, window, _| {
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
                                                .style(style(window.rem_size())),
                                        ),
                                )
                        });
                    })
                    .into_any_element()
            })
            .into_any_element()
    }
}

/// Convert the shared pixel gap using the active root font size.
pub fn style(rem_size: gpui::Pixels) -> TextViewStyle {
    TextViewStyle::default()
        .with_dark(true)
        .with_foreground(rgb(theme::TEXT_2).into())
        .with_muted_foreground(rgb(theme::TEXT_2).into())
        .with_link(rgb(theme::LINK_INK).into())
        .with_selection(rgba(theme::TEXT_SELECTION_WASH).into())
        .with_code_background(rgb(theme::RAISED).into())
        .with_inline_code(gpui::HighlightStyle {
            color: Some(rgb(theme::INLINE_CODE_INK).into()),
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
        .with_paragraph_gap(rems(theme::BLOCK_GAP / f32::from(rem_size)))
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
        let style = style(window.rem_size())
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
    pub(super) struct Views(
        pub(super) HashMap<SharedString, (Entity<TextViewState>, gpui::TextStyle)>,
    );
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
        let gap = px(theme::BLOCK_GAP);
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

#[cfg(test)]
mod file_link_tests {
    use super::*;
    use gpui::{div, Context, Modifiers, Render, TestAppContext, VisualTestContext};

    struct LinkFixture {
        cache: TextCache,
        source: String,
        cwd: std::path::PathBuf,
        preview: crate::attachment_preview::Preview,
    }
    impl Render for LinkFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.cache.file_context(Some(&self.cwd), &self.preview);
            use gpui::base::ElementExt;
            self.preview.mount(
                div().size_full().child(
                    div()
                        .text_size(px(13.))
                        .child(Markdown::new(
                            "file-link-fixture",
                            self.source.clone(),
                            self.cache.clone(),
                        ))
                        .text_selection_scope(gpui::base::TextSelectionScopeId::default()),
                ),
            )
        }
    }

    fn fixture<'a>(
        cx: &'a mut TestAppContext,
        source: &str,
    ) -> (Entity<LinkFixture>, &'a mut VisualTestContext) {
        cx.update(crate::theme::init_components);
        let (root, cx) = cx.add_window_view(|window, cx| {
            let preview = crate::attachment_preview::Preview::new(cx);
            let view = cx.new(|_| LinkFixture {
                cache: TextCache::default(),
                source: source.into(),
                cwd: std::env::temp_dir(),
                preview,
            });
            gpui::component::Root::new(view, window, cx).bordered(false)
        });
        let view = root.read_with(cx, |root, _| {
            root.view().clone().downcast::<LinkFixture>().unwrap()
        });
        (view, cx)
    }

    fn card(cx: &mut VisualTestContext, name: &str) -> gpui::Bounds<gpui::Pixels> {
        cx.debug_bounds(Box::leak(
            format!(
                "file-attachment-{}",
                std::env::temp_dir().join(name).display()
            )
            .into_boxed_str(),
        ))
        .unwrap()
    }

    #[gpui::test]
    fn local_file_click_opens_a_file_url_without_line_suffix(cx: &mut TestAppContext) {
        let path = std::env::temp_dir().join("ferrite-report.md");
        std::fs::write(&path, "fixture").unwrap();
        let source = format!("[report]({}:12)", path.display());
        let (_, cx) = fixture(cx, &source);
        let target = card(cx, "ferrite-report.md").center();
        cx.simulate_click(target, Modifiers::default());
        assert_eq!(
            cx.opened_url(),
            Some(url::Url::from_file_path(&path).unwrap().to_string())
        );
    }

    #[gpui::test]
    fn file_cards_wrap_inline_and_preserve_markdown_and_copying(cx: &mut TestAppContext) {
        let source = "Before [**report**](report.md:12) after.\n\n- Read [notes](notes.txt).\n\n| File | Result |\n| --- | --- |\n| [data](data.csv) | Ready |\n\n[web](https://example.com/report.pdf)";
        let (view, cx) = fixture(cx, source);
        cx.simulate_resize(gpui::size(px(600.), px(400.)));
        let wide = card(cx, "report.md");
        assert!(
            wide.left() > px(20.),
            "card follows the leading prose inline: {wide:?}"
        );
        assert_eq!(wide.size.height, px(26.));
        assert!(card(cx, "notes.txt").top() > wide.bottom());
        assert!(card(cx, "data.csv").top() > card(cx, "notes.txt").bottom());
        cx.update(|_, cx| {
            let state = cx
                .global::<super::testing::Views>()
                .0
                .get("file-link-fixture")
                .unwrap()
                .0
                .clone();
            state.update(cx, |state, cx| state.select_all(cx));
            let selected = state.read(cx).selected_text();
            assert!(selected.contains("Before report after."), "{selected}");
            assert!(selected.contains("notes") && selected.contains("data"));
        });
        cx.simulate_resize(gpui::size(px(130.), px(500.)));
        let narrow = card(cx, "report.md");
        assert!(
            narrow.top() > wide.top(),
            "card wraps as a unit: {narrow:?}"
        );
        assert!(
            narrow.right() <= px(130.),
            "card fits narrow pane: {narrow:?}"
        );
        // Appending source updates the same native parser/entity.
        let original = cx.update(|_, cx| super::testing::first_entity("file-link-fixture", cx));
        view.update(cx, |view, cx| {
            view.source.push_str(" More text.");
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(
            cx.update(|_, cx| super::testing::first_entity("file-link-fixture", cx)),
            original
        );
    }

    #[gpui::test]
    fn image_file_card_uses_the_existing_pane_preview(cx: &mut TestAppContext) {
        let path = std::env::temp_dir().join("ferrite-link-image.svg");
        std::fs::write(
            &path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"/>"#,
        )
        .unwrap();
        let (view, cx) = fixture(cx, "[image](ferrite-link-image.svg)");
        let target = card(cx, "ferrite-link-image.svg").center();
        cx.simulate_click(target, Modifiers::default());
        assert!(view.read_with(cx, |view, _| view.preview.focus_target().is_some()));
        assert_eq!(cx.opened_url(), None);
        cx.simulate_keystrokes("escape");
        assert!(view.read_with(cx, |view, _| view.preview.focus_target().is_none()));
    }

    #[gpui::test]
    fn missing_file_does_not_reach_the_os(cx: &mut TestAppContext) {
        let (_, cx) = fixture(cx, "[missing](ferrite-no-such-file-67a12.pdf)");
        let target = card(cx, "ferrite-no-such-file-67a12.pdf").center();
        cx.simulate_click(target, Modifiers::default());
        assert_eq!(cx.opened_url(), None);
        cx.update(|window, cx| {
            use gpui::component::WindowExt;
            assert_eq!(
                window.notifications(cx).len(),
                1,
                "missing file has actionable feedback"
            );
        });
    }
    #[gpui::test]
    fn drag_selection_across_file_card_copies_the_original_sentence(cx: &mut TestAppContext) {
        let (_, cx) = fixture(cx, "Before [report](report.md) after.");
        cx.simulate_resize(gpui::size(px(400.), px(200.)));
        let bounds = card(cx, "report.md");
        let start = gpui::point(px(1.), bounds.center().y);
        let end = gpui::point(px(390.), bounds.center().y);
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(end, gpui::MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        cx.update(|_, cx| {
            let state = &cx
                .global::<super::testing::Views>()
                .0
                .get("file-link-fixture")
                .unwrap()
                .0;
            assert_eq!(
                state.read(cx).selected_text().trim_end(),
                "Before report after."
            );
        });
    }

    #[gpui::test]
    fn keyboard_can_open_a_file_card(cx: &mut TestAppContext) {
        let path = std::env::temp_dir().join("ferrite-keyboard.txt");
        std::fs::write(&path, "fixture").unwrap();
        let second = std::env::temp_dir().join("ferrite-keyboard-second.txt");
        std::fs::write(&second, "second").unwrap();
        let (_, cx) = fixture(
            cx,
            "[the **report** file](ferrite-keyboard.txt) and [second](ferrite-keyboard-second.txt)",
        );
        cx.update(|window, cx| window.focus_next(cx));
        cx.simulate_keystrokes("enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        assert_eq!(
            cx.opened_url(),
            Some(url::Url::from_file_path(path).unwrap().to_string())
        );
        cx.update(|window, cx| window.focus_next(cx));
        cx.simulate_keystrokes("enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        assert_eq!(
            cx.opened_url(),
            Some(url::Url::from_file_path(second).unwrap().to_string())
        );
    }
    #[gpui::test]
    fn custom_inline_link_preserves_source_selection(cx: &mut TestAppContext) {
        struct CustomLinkRoot {
            state: Entity<TextViewState>,
        }
        impl Render for CustomLinkRoot {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .w(px(300.))
                    .child(gpui::base::TextSelectionLayer)
                    .child(
                        TextView::new(&self.state)
                            .selection_format(gpui::base::text::SelectionFormat::Source)
                            .link_renderer(|_, label, _, _| {
                                Some((
                                    gpui::size(px(80.), px(24.)),
                                    div().size_full().child(label.clone()).into_any_element(),
                                ))
                            }),
                    )
            }
        }
        cx.update(crate::theme::init_components);
        let (view, cx) = cx.add_window_view(|_, cx| CustomLinkRoot {
            state: cx.new(|cx| TextViewState::markdown("Before [report](report.md) after.", cx)),
        });
        let cx: &mut VisualTestContext = cx;
        cx.simulate_mouse_down(
            gpui::point(px(1.), px(10.)),
            gpui::MouseButton::Left,
            Modifiers::default(),
        );
        cx.simulate_mouse_move(
            gpui::point(px(299.), px(10.)),
            Some(gpui::MouseButton::Left),
            Modifiers::default(),
        );
        cx.simulate_mouse_up(
            gpui::point(px(299.), px(10.)),
            gpui::MouseButton::Left,
            Modifiers::default(),
        );
        assert_eq!(
            view.read_with(cx, |view, cx| view.state.read(cx).selected_text())
                .trim_end(),
            "Before [report](report.md) after."
        );
    }
}

#[cfg(test)]
mod spacing_tests {
    use super::*;
    use gpui::{div, Context, Render, TestAppContext};
    const SELECTORS: [&str; 7] = [
        "spacing-0",
        "spacing-1",
        "spacing-2",
        "spacing-3",
        "spacing-4",
        "spacing-5",
        "spacing-6",
    ];

    struct SpacingRoot {
        samples: Vec<String>,
        gap: f32,
    }

    impl Render for SpacingRoot {
        fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            window.set_rem_size(px(theme::FS_MD));
            div()
                .w(px(360.))
                .children(self.samples.iter().enumerate().map(|(ix, source)| {
                    div()
                        .debug_selector(move || format!("spacing-{ix}"))
                        .w_full()
                        .child(
                            TextView::markdown(format!("spacing-{ix}"), source.clone())
                                .max_lines(usize::MAX)
                                .style(if self.gap == 0. {
                                    style(window.rem_size()).with_paragraph_gap(rems(0.))
                                } else {
                                    style(window.rem_size())
                                }),
                        )
                }))
        }
    }

    // Measure the real renderer, including nested blocks. Each pair must add
    // exactly one boundary gap, regardless of either block's type.
    #[gpui::test]
    fn markdown_spacing_between_all_block_kinds(cx: &mut TestAppContext) {
        cx.update(gpui::component::init);
        let blocks = [
            "Paragraph with **bold** and `code`.",
            "## Heading",
            "- First item",
            "1. First item",
            "- [ ] Task",
            "> Quoted paragraph",
            "```text\ncode\n```",
            "| Head |\n| --- |\n| Cell |",
            "---",
        ];
        let (root, cx) = cx.add_window_view(|_, _| SpacingRoot {
            samples: vec![],
            gap: 10.,
        });
        for first in blocks {
            for second in blocks {
                // Adjacent list/quote blocks merge in Markdown. Their internal
                // sibling spacing must still be exactly one gap.
                let combined = format!("{first}\n\n{second}");
                root.update(cx, |root, cx| {
                    root.samples = vec![first.into(), second.into(), combined.clone()];
                    cx.notify();
                });
                cx.run_until_parked();
                cx.update(|window, cx| {
                    let _ = window.draw(cx);
                });
                let mut height = |ix| cx.debug_bounds(SELECTORS[ix]).unwrap().size.height;
                let gap = height(2) - height(0) - height(1);
                assert!((gap - px(10.)).abs() < px(0.5), "{combined:?}: gap {gap:?}");
            }
        }
    }

    #[gpui::test]
    fn markdown_spacing_nested_lists_and_zero_gap(cx: &mut TestAppContext) {
        cx.update(gpui::component::init);
        let samples = vec![
            "- One\n- Two\n- Three".into(),
            "1. One\n2. Two\n3. Three".into(),
            "- [ ] One\n- [x] Two\n- [ ] Three".into(),
            "- One\n  - Two\n  - Three".into(),
            "- One\n\n  Two\n\n  Three".into(),
            "> One\n>\n> Two\n>\n> Three".into(),
            "- One\n\n  ```text\n  Two\n  ```\n\n  Three".into(),
        ];
        let (root, cx) = cx.add_window_view(|_, _| SpacingRoot { samples, gap: 0. });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let compact: Vec<_> = (0..7)
            .map(|ix| cx.debug_bounds(SELECTORS[ix]).unwrap().size.height)
            .collect();
        root.update(cx, |root, cx| {
            root.gap = 10.;
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        for (ix, before) in compact.into_iter().enumerate() {
            let after = cx.debug_bounds(SELECTORS[ix]).unwrap().size.height;
            assert!(
                (after - before - px(20.)).abs() < px(0.5),
                "nested sample {ix}: {before:?} -> {after:?}"
            );
        }
    }

    #[gpui::test]
    fn markdown_spacing_preserves_hard_breaks_and_ignores_definitions(cx: &mut TestAppContext) {
        cx.update(gpui::component::init);
        let samples = vec![
            "Before  \n**After**".into(),
            "Before\\\n**After**".into(),
            "Before\n\n**After**".into(),
            "Before".into(),
            "Before\n\n[unused]: https://example.com".into(),
            "Before\n\n[unused]: https://example.com\n\n**After**".into(),
        ];
        let (_, cx) = cx.add_window_view(|_, _| SpacingRoot { samples, gap: 10. });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let mut height = |ix| cx.debug_bounds(SELECTORS[ix]).unwrap().size.height;
        let paragraph_pair = height(2);
        assert_eq!(
            height(0),
            paragraph_pair - px(10.),
            "two-space hard break must start a line"
        );
        assert_eq!(
            height(1),
            paragraph_pair - px(10.),
            "backslash hard break must start a line"
        );
        assert_eq!(
            height(3),
            height(4),
            "a trailing reference definition adds no blank space"
        );
        assert_eq!(
            height(5),
            paragraph_pair,
            "definitions between paragraphs add no extra gap"
        );
    }
}
