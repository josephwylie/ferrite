//! Native rich text with a stable parser per answer run. Appending tokens
//! advances the toolkit parser; unrelated pane renders do not parse again.
use gpui::base::text::{TextView, TextViewState, TextViewStyle};
use gpui::component::input::{
    Editor, EditorState, FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter, Rope,
    Textarea, TextareaState,
};
use gpui::{prelude::*, px, rems, rgb, rgba, App, Entity, Focusable, SharedString, Window};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

use crate::theme;

pub fn init(cx: &mut App) {
    cx.bind_keys([gpui::KeyBinding::new(
        "enter",
        gpui::NoAction {},
        Some("TranscriptCodeActions"),
    )]);
}

/// Code controls form a native keyboard island inside the transcript.
pub(crate) fn code_actions_focused(window: &Window) -> bool {
    window
        .context_stack()
        .iter()
        .any(|context| context.contains("TranscriptCodeActions"))
}

/// Replace a native text control's source without losing the reader's
/// place: the selection, its direction, and the scroll offset. A macro
/// because the textarea and the editor share these methods but no type.
macro_rules! keep_place {
    ($state:expr, $source:expr, $window:expr, $cx:expr) => {{
        let state = $state;
        let mut selected = state.selected_range();
        // The native range setter accepts anchor → caret order.
        // Keep backward selections backward across new chunks.
        if !selected.is_empty() && state.cursor() == selected.start {
            selected = selected.end..selected.start;
        }
        let scroll = state.scroll_offset();
        state.set_value($source.to_string(), $window, $cx);
        // Native setters clip replacement offsets to UTF-8 and
        // defer scroll clamping until the new layout is ready.
        state.set_selected_range(selected, $cx);
        state.set_scroll_offset(scroll, $cx);
    }};
}

#[derive(Clone)]
enum NativeText {
    Rich(Entity<TextViewState>),
    Output(Entity<TextareaState>),
    Code(Entity<EditorState>),
}

/// Which native control a cached source is shown in.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Rich,
    Output,
    /// A source file, coloured by the lexer for this language.
    Code(&'static str),
}

impl NativeText {
    fn is(&self, kind: Kind) -> bool {
        matches!(
            (self, kind),
            (NativeText::Rich(_), Kind::Rich)
                | (NativeText::Output(_), Kind::Output)
                | (NativeText::Code(_), Kind::Code(_))
        )
    }
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
    #[cfg(test)]
    pub(crate) fn retained_handles(&self) -> usize {
        Rc::strong_count(&self.0)
    }

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
        kind: Kind,
        window: &mut Window,
        cx: &mut App,
    ) -> NativeText {
        let mut cache = self.0.borrow_mut();
        // A retained entry of another kind is a different control; it cannot
        // be updated in place, so it is rebuilt.
        if cache.1.get(&id).is_some_and(|text| !text.state.is(kind)) {
            if let Some(old) = cache.1.remove(&id) {
                cache.2 -= old.source.len();
            }
        }
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
            state: match kind {
                Kind::Output => NativeText::Output(cx.new(|cx| {
                    let mut state = TextareaState::new(window, cx).auto_grow(1, 12);
                    state.set_value(source.to_string(), window, cx);
                    state
                })),
                // A reader, not an editor: line numbers to find your place,
                // but no folds or guides.
                Kind::Code(language) => NativeText::Code(cx.new(|cx| {
                    let mut state = EditorState::new(window, cx)
                        .language(language)
                        .line_number(true)
                        .folding(false)
                        .indent_guides(false);
                    state.set_highlighter_factory(
                        Rc::new(|language: &str| {
                            Some(Box::new(Lexed {
                                language: SharedString::from(language.to_string()),
                                runs: Vec::new(),
                            }) as Box<dyn InputHighlighter>)
                        }),
                        cx,
                    );
                    state.set_value(source.to_string(), window, cx);
                    state
                })),
                Kind::Rich => NativeText::Rich(cx.new(|cx| TextViewState::markdown(source, cx))),
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
                NativeText::Output(state) => {
                    state.update(cx, |state, cx| keep_place!(state, source, window, cx))
                }
                NativeText::Code(state) => {
                    state.update(cx, |state, cx| keep_place!(state, source, window, cx))
                }
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
        let NativeText::Rich(state) = self.cached(id, source, Kind::Rich, window, cx) else {
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
    document: Option<gpui::base::TextSelectionDocument>,
}

impl Markdown {
    pub fn selection_document(
        mut self,
        document: Option<gpui::base::TextSelectionDocument>,
    ) -> Self {
        self.document = document;
        self
    }

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
            document: None,
        }
    }
}

impl gpui::RenderOnce for Markdown {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.cache.state(self.id.clone(), &self.source, window, cx);
        // Prose sets its own face; the inherited style is the Pane's.
        let face: SharedString = theme::FONT_UI.into();
        #[cfg(test)]
        testing::record(
            self.id.clone(),
            state.clone(),
            gpui::TextStyle {
                font_family: face.clone(),
                ..window.text_style().clone()
            },
            cx,
        );
        // Headings scale with the reading size the answer row set.
        let base = window.text_style().font_size.to_pixels(window.rem_size());
        let text_style = style_at(window.rem_size(), base);
        let text_style = if self.muted {
            // A thought stays one step down the ink ladder, headings too.
            (1..=6).fold(
                text_style.with_foreground(rgb(theme::TEXT_2).into()),
                |style, level| {
                    style.with_heading(level, heading(level, base).text_color(rgb(theme::TEXT_2)))
                },
            )
        } else {
            text_style
        };
        let actions_namespace = self.id.clone();
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
            .when_some(self.document, |view, document| {
                view.selection_document(document, self.id)
            })
            .font_family(face)
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
            .code_block_actions(move |block, window, cx| {
                let id: SharedString =
                    format!("code-actions-{actions_namespace}-{:?}", block.span).into();
                let code = block.code();
                let language = block.lang();
                let selected = block.has_selection();
                let key = id.clone();
                let actions = window.use_keyed_state(id, cx, |_, cx| CodeActions {
                    code: code.clone(),
                    language: language.clone(),
                    copied: false,
                    selected,
                    focus: cx.focus_handle(),
                    key,
                });
                actions.update(cx, |actions, cx| {
                    if actions.code != code || actions.language != language {
                        actions.code = code;
                        actions.language = language;
                        actions.copied = false;
                        cx.notify();
                    }
                    if actions.selected != selected {
                        actions.selected = selected;
                        cx.notify();
                    }
                });
                actions
            })
            .into_any_element()
    }
}

/// Each fenced block retains its own copy confirmation while the answer streams.
/// The source comes directly from the native code node, never rendered labels.
struct CodeActions {
    code: SharedString,
    language: Option<SharedString>,
    copied: bool,
    /// The caret or a selection is inside the block.
    selected: bool,
    focus: gpui::FocusHandle,
    /// The hover blend's key: the block's own actions id.
    key: SharedString,
}

/// A fence's language id as the overlay names it: none for plain text.
fn code_language(language: Option<&SharedString>) -> Option<SharedString> {
    language
        .filter(|lang| {
            !["text", "txt", "plaintext"]
                .iter()
                .any(|plain| lang.eq_ignore_ascii_case(plain))
        })
        .cloned()
}

impl gpui::Render for CodeActions {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let code = self.code.clone();
        // The overlay covers the whole block (the vendor lays it over the
        // code with no layout of its own): the pointer anywhere on the block
        // blends the actions in over 150ms; keyboard focus inside them, or
        // the caret or a selection inside the block, shows them at once.
        let revealed = self.selected || self.focus.contains_focused(window, cx);
        let shown = if revealed {
            1.
        } else {
            crate::motion::hover_t(&self.key)
        };
        // The transcript around this overlay is a cached view: while the
        // blend is mid-flight, ask for the next frame's render here (the
        // cached ancestors follow).
        if shown > 0. && shown < 1. {
            cx.notify();
        }
        let mut actions = gpui::div()
            .key_context("TranscriptCodeActions")
            .track_focus(&self.focus)
            .debug_selector(|| "code-actions".into())
            .absolute()
            .top(px(theme::CODE_ACTIONS_TOP))
            .right(px(theme::CODE_ACTIONS_RIGHT))
            .flex()
            .items_center()
            .justify_end()
            .min_w(px(theme::CODE_ACTION_MIN_W))
            .gap(px(theme::SPACE_1))
            .rounded(px(theme::R_CHIP))
            .bg(rgb(theme::RAISED))
            .font_family(theme::FONT_UI)
            .text_size(px(theme::FS_SM))
            .line_height(px(theme::LH_META))
            .text_color(rgb(theme::TEXT_MUTED))
            .opacity(shown)
            .when_some(
                code_language(self.language.as_ref()),
                |actions, language| {
                    actions.child(
                        gpui::div()
                            .flex_shrink_0()
                            .pl(px(theme::SPACE_1))
                            .font_family(theme::FONT_CODE)
                            .child(language),
                    )
                },
            );
        if self
            .language
            .as_ref()
            .is_some_and(|lang| lang.eq_ignore_ascii_case("html"))
        {
            actions = actions.child(code_action("preview-html", cx).label("Preview").on_click(
                move |_, window, cx| {
                    use gpui::component::WindowExt as _;
                    let html = code.clone();
                    window.open_dialog(cx, move |dialog, window, _| {
                        // The sheet recipe: raised, the strong edge (the
                        // kit's border), the Pane radius, a mono title.
                        dialog
                            .title(
                                gpui::div()
                                    .font_family(theme::FONT_UI)
                                    .text_size(px(theme::FS_UI))
                                    .line_height(px(theme::LH_UI))
                                    .font_weight(theme::W_LABEL)
                                    .text_color(rgb(theme::TEXT_STRONG))
                                    .child("HTML preview"),
                            )
                            .width(px(theme::READING_MAX_W))
                            .bg(rgb(theme::RAISED))
                            .rounded(px(theme::R_PANE))
                            .child(
                                gpui::div()
                                    .id("html-preview")
                                    .font_family(theme::FONT_UI)
                                    .text_size(px(theme::FS_PROSE))
                                    .line_height(px(theme::LH_PROSE))
                                    .text_color(rgb(theme::TEXT))
                                    .max_h(px(theme::HTML_PREVIEW_MAX_H))
                                    .overflow_y_scroll()
                                    .child(
                                        TextView::html("html-preview-text", html.clone())
                                            .style(style(window.rem_size())),
                                    ),
                            )
                    });
                },
            ));
        }
        // `Copy ⌘C`: the key read from the key table, as a mono suffix on a
        // wrapper (a kit button's own tooltip is plain text).
        actions = actions.child(
            gpui::div()
                .id("copy-code-tip")
                .flex_shrink_0()
                .tooltip(crate::menu::action_tooltip(
                    "Copy",
                    "cockpit::CopySelection",
                ))
                .child(
                    code_action("copy-code", cx)
                        .debug_selector(|| "copy-code".into())
                        .accessibility_label("Copy code")
                        .label(if self.copied { "Copied" } else { "Copy" })
                        .when(self.copied, |button| {
                            button.debug_selector(|| "code-copied".into())
                        })
                        .on_click(cx.listener(|view, _, _, cx| {
                            cx.stop_propagation();
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                view.code.to_string(),
                            ));
                            view.copied = true;
                            cx.notify();
                        })),
                ),
        );
        gpui::div()
            .id(SharedString::from(format!("{}-overlay", self.key)))
            .relative()
            .size_full()
            .on_hover(crate::motion::hover_listener(self.key.clone()))
            .child(actions)
    }
}

/// A quiet text action in a fence's overlay: Geist `FS_SM` `TEXT_2` on the
/// block's own `RAISED`, `HOVER_RAISED` under the pointer (the one 150ms
/// blend), pressed at once, a stable `CODE_ACTION_H` × `CODE_ACTION_MIN_W`
/// target.
fn code_action(id: &'static str, cx: &App) -> gpui::component::button::Button {
    crate::components::faded_button(
        id,
        rgba(theme::TRANSPARENT).into(),
        rgb(theme::HOVER_RAISED).into(),
        rgb(theme::FILL_HOVER).into(),
        rgb(theme::TEXT_2).into(),
        cx,
    )
    .h(px(theme::CODE_ACTION_H))
    .min_w(px(theme::CODE_ACTION_MIN_W))
    .px(px(theme::CODE_ACTION_PAD_X))
    .rounded(px(theme::R_CHIP))
    .font_family(theme::FONT_UI)
    .text_size(px(theme::FS_SM))
    .line_height(px(theme::LH_META))
    .flex_shrink_0()
    .tab_stop(true)
}

/// Markdown's look at the Standard reading size (see the WP-B section of
/// `theme.rs` for the rules). Rems convert with the active root font size.
pub fn style(rem_size: gpui::Pixels) -> TextViewStyle {
    style_at(rem_size, px(theme::FS_PROSE))
}

/// Markdown's look for prose set at `base`: headings are ratios of it
/// (`theme::heading_scale`), each on its own pixel line height.
pub fn style_at(rem_size: gpui::Pixels, base: gpui::Pixels) -> TextViewStyle {
    let rem = |value: f32| rems(value / f32::from(rem_size));
    let step = |value: f32| theme::reading_step(value, f32::from(base));
    let reading = reading_size_at(f32::from(base));
    let mut style = TextViewStyle::default()
        .with_dark(true)
        .with_foreground(rgb(theme::TEXT).into())
        .with_muted_foreground(rgb(theme::TEXT_2).into())
        .with_link(rgb(theme::LINK_INK).into())
        .with_link_underline(Some(rgba(theme::ACCENT_EDGE).into()))
        .with_selection(rgba(theme::TEXT_SELECTION_WASH).into())
        .with_strong(gpui::HighlightStyle {
            font_weight: Some(theme::W_STRONG),
            ..Default::default()
        })
        .with_code_background(rgb(theme::RAISED).into())
        .with_code_block(
            gpui::StyleRefinement::default()
                .px(px(theme::CODE_PAD_X))
                .py(px(theme::CODE_PAD_Y))
                .rounded(px(theme::R_BLOCK))
                .font_family(theme::FONT_CODE)
                .font_weight(theme::W_BODY)
                .text_size(px(theme::FS_UI))
                .line_height(px(theme::LH_CODE))
                .text_color(rgb(theme::SYN_PLAIN)),
        )
        // Inline code is body ink at body weight whatever it sits in (a
        // heading, `**strong**`): the chip, not the weight, sets it apart.
        .with_inline_code(gpui::HighlightStyle {
            color: Some(rgb(theme::INLINE_CODE_INK).into()),
            font_weight: Some(theme::W_BODY),
            ..Default::default()
        })
        .with_inline_code_font(Some(theme::FONT_CODE.into()))
        .with_inline_code_wash(Some(gpui::base::text::InlineCodeWash {
            color: rgba(theme::INLINE_CODE_WASH).into(),
            radius: px(theme::R_CHIP),
            overhang: px(theme::INLINE_CODE_OVERHANG),
            inset_y: px(theme::inline_code_inset_y(reading)),
        }))
        // No row rules: a transparent border draws none (the header keeps
        // `TABLE_HEAD_RULE` through its own refinement).
        .with_border(gpui::transparent_black())
        .with_table({
            // No box and no ground: the header's rule is the table.
            let mut table = gpui::StyleRefinement::default()
                .border_0()
                .bg(gpui::transparent_black());
            table.overflow.x = Some(gpui::Overflow::Scroll);
            table
        })
        .with_table_cell(crate::components::tabular(
            gpui::StyleRefinement::default()
                .border_r_0()
                .py(px(theme::TABLE_CELL_PAD_Y))
                .text_size(px(theme::inline_code_size(reading)))
                .line_height(px(theme::table_line_height(reading))),
        ))
        .with_table_head(
            gpui::StyleRefinement::default()
                .bg(gpui::transparent_black())
                .text_color(rgb(theme::TEXT_MUTED))
                .font_weight(theme::W_BODY)
                .border_color(rgba(theme::TABLE_HEAD_RULE)),
        )
        .with_blockquote(
            gpui::StyleRefinement::default()
                .border_l(px(theme::QUOTE_RULE_W))
                .border_color(rgba(theme::HAIRLINE_STRONG))
                .text_color(rgb(theme::TEXT_2))
                .not_italic()
                .pl(px(step(theme::PROSE_HANG) - theme::QUOTE_RULE_W))
                .pr(px(0.)),
        )
        .with_rule(
            gpui::StyleRefinement::default()
                .h(px(1.))
                .bg(rgba(theme::HAIRLINE))
                .my(px(theme::RULE_MARGIN_Y)),
        )
        // Paragraphs and list items hold to the prose measure; code, tables
        // and rules keep the column.
        .with_prose_max_width(Some(px(theme::PROSE_MEASURE)))
        .with_list_hang(Some(gpui::base::text::ListHang {
            width: px(step(theme::PROSE_HANG)),
            gap: px(theme::LIST_MARKER_GAP),
        }))
        .with_list_markers(
            gpui::StyleRefinement::default().text_color(rgb(theme::TEXT_MUTED)),
            crate::components::tabular(
                gpui::StyleRefinement::default().text_color(rgb(theme::TEXT_MUTED)),
            ),
        )
        // The gaps are em-proportional to the prose size, so a larger
        // reading size keeps the Standard rhythm.
        .with_paragraph_gap(rem(step(theme::PROSE_GAP)))
        .with_heading_spacing(
            rem(step(theme::HEADING_SPACE_ABOVE)),
            Some(rem(step(theme::HEADING_SPACE_BELOW))),
        )
        .with_heading_base_font_size(base)
        .with_heading_font_size(|level, base| px(heading_size(level, f32::from(base))));
    for level in 1..=6 {
        style = style.with_heading(level, heading(level, base));
    }
    style
}

/// The reading size whose answer size is `base`.
fn reading_size_at(base: f32) -> ferrite_core::settings::ReadingSize {
    ferrite_core::settings::ReadingSize::nearest(base.round() as u8)
}

/// A heading's size at prose size `base`: the type table's ratio, rounded
/// to a whole pixel (18 · 16 · 14 at Standard, 21 · 18 · 16 at Comfortable,
/// 23 · 21 · 18 at Large), so no heading lands on a half pixel.
fn heading_size(level: u8, base: f32) -> f32 {
    (base * theme::heading_scale(level)).round()
}

/// A heading's weight, ink and pixel line height at prose size `base`.
/// H4–H6 are set apart by weight alone: 600 is only for H1–H3.
fn heading(level: u8, base: gpui::Pixels) -> gpui::StyleRefinement {
    let size = heading_size(level, f32::from(base));
    let (weight, ink) = if level <= 3 {
        (theme::W_STRONG, theme::TEXT_STRONG)
    } else {
        (theme::W_LABEL, theme::TEXT_STRONG)
    };
    gpui::StyleRefinement::default()
        .font_weight(weight)
        .text_color(rgb(ink))
        .line_height(px(theme::prose_line_height(size)))
}

/// A code token's highlight: ink by class (slice 01 §2.6), comments italic.
/// The one mapping both the Markdown fence and the plain fallback paint.
pub(crate) fn syntax_style(class: ferrite_core::transcript::Class) -> gpui::HighlightStyle {
    use ferrite_core::transcript::Class;
    let ink = match class {
        Class::Plain => theme::SYN_PLAIN,
        Class::Keyword => theme::SYN_KEYWORD,
        Class::Str => theme::SYN_STRING,
        Class::Number => theme::SYN_NUMBER,
        Class::Comment => theme::SYN_COMMENT,
        Class::Function => theme::SYN_FUNCTION,
        Class::Type => theme::SYN_TYPE,
        Class::Punct => theme::SYN_PUNCT,
    };
    gpui::HighlightStyle {
        color: Some(rgb(ink).into()),
        font_style: (class == Class::Comment).then_some(gpui::FontStyle::Italic),
        ..Default::default()
    }
}

/// The pixel line box of a heading of `level` in prose set at `size`: what a
/// leading heading's first line occupies (the answer mark centres on it).
#[allow(dead_code)]
pub fn heading_line_height(level: u8, size: f32) -> f32 {
    theme::prose_line_height(heading_size(level, size))
}

/// Literal provider output shares Markdown's selection engine and keeps
/// the parent's typography. A collision-free code fence preserves exact text.
#[derive(IntoElement)]
pub struct Literal {
    pub id: SharedString,
    pub text: SharedString,
    pub highlights: Vec<(std::ops::Range<usize>, gpui::HighlightStyle)>,
    pub cache: TextCache,
    pub document: Option<gpui::base::TextSelectionDocument>,
}

/// The same literal document feeds native rendering and offscreen copy.
pub(crate) fn literal_source(text: &str) -> String {
    let fence = "`".repeat(
        text.split(|c| c != '`')
            .map(str::len)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(3),
    );
    format!("{fence}\n{text}\n{fence}")
}

impl gpui::RenderOnce for Literal {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let inherited = window.text_style();
        let source = literal_source(&self.text);
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
        testing::record(self.id.clone(), state.clone(), inherited.clone(), cx);
        TextView::new(&state)
            .when_some(self.document, |view, document| {
                view.selection_document(document, self.id)
            })
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
    pub aria_label: SharedString,
    pub fill: bool,
}

impl gpui::RenderOnce for Output {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let NativeText::Output(state) =
            self.cache
                .cached(self.id.clone(), &self.text, Kind::Output, window, cx)
        else {
            unreachable!("output has its own namespace")
        };
        #[cfg(test)]
        testing::record_output(self.id, state.clone(), cx);
        Textarea::new(&state)
            .readonly(true)
            .appearance(false)
            .bordered(false)
            .aria_label(self.aria_label)
            .w_full()
            .min_w_0()
            .when(self.fill, |view| view.h_full())
            .p_0()
            .font_family(window.text_style().font_family.clone())
            .text_size(window.text_style().font_size.to_pixels(window.rem_size()))
            .text_color(window.text_style().color)
    }
}

/// The reader's body for an open document: Markdown rendered, a file in a
/// language the lexer knows coloured, anything else as plain text.
pub fn document_body(
    document: crate::attachment_preview::Document,
    cache: TextCache,
) -> gpui::AnyElement {
    let id = SharedString::from(format!("file-{}", document.path.display()));
    if document.is_markdown() {
        return Markdown::new(
            format!("document-{}", document.path.display()),
            document.source,
            cache,
        )
        .into_any_element();
    }
    let aria_label = SharedString::from("File contents");
    match ferrite_core::transcript::language_for_path(&document.path) {
        Some(language) => Code {
            id,
            text: document.source.into(),
            language,
            cache,
            aria_label,
        }
        .into_any_element(),
        None => Output {
            id,
            text: document.source.into(),
            cache,
            aria_label,
            fill: true,
        }
        .into_any_element(),
    }
}

/// A source file in the reader: the same bounded, virtualized native text
/// as `Output`, in the control's code mode so Ferrite's lexer can colour it.
#[derive(IntoElement)]
pub struct Code {
    pub id: SharedString,
    pub text: SharedString,
    pub language: &'static str,
    pub cache: TextCache,
    pub aria_label: SharedString,
}

impl gpui::RenderOnce for Code {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let NativeText::Code(state) = self.cache.cached(
            self.id.clone(),
            &self.text,
            Kind::Code(self.language),
            window,
            cx,
        ) else {
            unreachable!("a mismatched kind is rebuilt, never returned")
        };
        #[cfg(test)]
        testing::record_code(self.id, state.clone(), cx);
        Editor::new(&state)
            .readonly(true)
            .appearance(false)
            .bordered(false)
            .aria_label(self.aria_label)
            .w_full()
            .min_w_0()
            .h_full()
            .p_0()
            .font_family(window.text_style().font_family.clone())
            .text_size(window.text_style().font_size.to_pixels(window.rem_size()))
            .text_color(window.text_style().color)
    }
}

/// Ferrite's own lexer behind the editor's highlighter seam, painting the
/// inks a transcript code block does, except for plain text. A file is lexed
/// whole on each change: the lexer is linear and a reader's source changes
/// only when the file does.
struct Lexed {
    language: SharedString,
    /// Contiguous runs covering every byte of the source, in order.
    runs: Vec<(std::ops::Range<usize>, gpui::HighlightStyle)>,
}

impl InputHighlighter for Lexed {
    fn language(&self) -> SharedString {
        self.language.clone()
    }

    fn update(
        &mut self,
        _: Option<InputEdit>,
        text: &Rope,
        _: bool,
        _: &mut Window,
        _: &mut gpui::Context<EditorState>,
    ) {
        let source = text.to_string();
        let tokens = ferrite_core::transcript::highlight_tokens(Some(&self.language), &source);
        let mut runs = crate::pane::code(&source, Some(&tokens));
        // A transcript block's plain ink is the body's grey, which sits too
        // close to the comment grey across a whole file. The reader's plain
        // text is the reader's own ink.
        for ((_, style), token) in runs.iter_mut().zip(&tokens) {
            if token.class == ferrite_core::transcript::Class::Plain {
                style.color = Some(rgb(theme::TEXT).into());
            }
        }
        self.runs = runs;
    }

    fn styles(
        &self,
        range: &std::ops::Range<usize>,
        _: &dyn HighlightStyleResolver,
    ) -> Vec<(std::ops::Range<usize>, gpui::HighlightStyle)> {
        lexed_styles(&self.runs, range)
    }

    fn fold_ranges(&self, _: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}

/// The runs over `range`, clipped to it, with any gap the runs leave filled
/// unstyled — the seam asks for ordered runs that cover the range exactly.
fn lexed_styles(
    runs: &[(std::ops::Range<usize>, gpui::HighlightStyle)],
    range: &std::ops::Range<usize>,
) -> Vec<(std::ops::Range<usize>, gpui::HighlightStyle)> {
    let first = runs.partition_point(|(run, _)| run.end <= range.start);
    let mut styles = Vec::new();
    let mut at = range.start;
    for (run, style) in &runs[first..] {
        if run.start >= range.end {
            break;
        }
        let start = run.start.max(at);
        if start > at {
            styles.push((at..start, gpui::HighlightStyle::default()));
        }
        let end = run.end.min(range.end);
        if end > start {
            styles.push((start..end, *style));
            at = end;
        }
    }
    if at < range.end {
        styles.push((at..range.end, gpui::HighlightStyle::default()));
    }
    styles
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

    #[derive(Default)]
    struct Codes(HashMap<SharedString, Entity<EditorState>>);
    impl gpui::Global for Codes {}

    /// Native text wrapper renders, keyed by the stable text identity. Kept
    /// separate from the entity registries: a cached entity may render again
    /// without being reconstructed.
    #[derive(Default)]
    struct Renders(HashMap<SharedString, usize>);
    impl gpui::Global for Renders {}

    fn record_render(id: &SharedString, cx: &mut App) {
        if cx.try_global::<Renders>().is_none() {
            cx.set_global(Renders::default());
        }
        *cx.global_mut::<Renders>().0.entry(id.clone()).or_default() += 1;
    }

    pub fn reset_renders(cx: &mut App) {
        if cx.try_global::<Renders>().is_none() {
            cx.set_global(Renders::default());
        } else {
            cx.global_mut::<Renders>().0.clear();
        }
    }

    pub fn renders_with_prefix(prefix: &str, cx: &App) -> usize {
        cx.try_global::<Renders>()
            .map(|renders| {
                renders
                    .0
                    .iter()
                    .filter(|(id, _)| id.starts_with(prefix))
                    .map(|(_, count)| count)
                    .sum()
            })
            .unwrap_or_default()
    }

    /// The number of native wrappers seen in a clean render, independent of
    /// how often GPUI scheduled that wrapper during the frame.
    pub fn rendered_identities_with_prefix(prefix: &str, cx: &App) -> usize {
        cx.try_global::<Renders>()
            .map(|renders| renders.0.keys().filter(|id| id.starts_with(prefix)).count())
            .unwrap_or_default()
    }

    pub fn record_output(id: SharedString, state: Entity<TextareaState>, cx: &mut App) {
        if cx.try_global::<Outputs>().is_none() {
            cx.set_global(Outputs::default());
        }
        record_render(&id, cx);
        cx.global_mut::<Outputs>().0.insert(id, state);
    }

    pub fn output(id: &str, cx: &App) -> Option<Entity<TextareaState>> {
        cx.try_global::<Outputs>()?.0.get(id).cloned()
    }

    pub fn record_code(id: SharedString, state: Entity<EditorState>, cx: &mut App) {
        if cx.try_global::<Codes>().is_none() {
            cx.set_global(Codes::default());
        }
        record_render(&id, cx);
        cx.global_mut::<Codes>().0.insert(id, state);
    }

    pub fn code(id: &str, cx: &App) -> Option<Entity<EditorState>> {
        cx.try_global::<Codes>()?.0.get(id).cloned()
    }

    pub fn first_entity(prefix: &str, cx: &App) -> Option<gpui::EntityId> {
        cx.global::<Views>()
            .0
            .iter()
            .find(|(id, _)| id.starts_with(prefix))
            .map(|(_, (state, _))| state.entity_id())
    }

    /// Read the native parser's actual output after a fixture has settled.
    pub fn full_text(prefix: &str, cx: &mut App) -> Option<String> {
        let state = cx
            .global::<Views>()
            .0
            .iter()
            .find(|(id, _)| id.starts_with(prefix))
            .map(|(_, (state, _))| state.clone())?;
        state.update(cx, |state, cx| state.select_all(cx));
        Some(state.read(cx).selected_text())
    }

    pub fn selected_text(prefix: &str, cx: &App) -> Option<String> {
        cx.global::<Views>()
            .0
            .iter()
            .find(|(id, _)| id.starts_with(prefix))
            .map(|(_, (state, _))| state.read(cx).selected_text())
    }

    /// The face a recorded text view was laid out in (its inherited family,
    /// or a Markdown view's prose face).
    pub fn font_family(prefix: &str, cx: &App) -> Option<SharedString> {
        cx.global::<Views>()
            .0
            .iter()
            .find(|(id, _)| id.starts_with(prefix))
            .map(|(_, (_, style))| style.font_family.clone())
    }

    pub fn font_size(prefix: &str, cx: &App) -> Option<gpui::Pixels> {
        cx.global::<Views>()
            .0
            .iter()
            .find(|(id, _)| id.starts_with(prefix))
            .map(|(_, (_, style))| style.font_size.to_pixels(px(theme::FS_UI)))
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
        record_render(&id, cx);
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
        let size = style.font_size.to_pixels(window.rem_size());
        let gap = px(theme::reading_step(theme::PROSE_GAP, f32::from(size)));
        let stride = (bounds.size.height + gap) / paragraphs.max(1) as f32;
        let line_height = stride - gap;
        bounds.origin.y += stride * item as f32;
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

    /// Aim at a byte through the native wrapper's actual wrapped line layout.
    /// Unlike `caret`, this is for a single markdown paragraph that wraps.
    pub fn wrapped_caret(
        id: &str,
        text: &str,
        byte: usize,
        window: &Window,
        cx: &App,
    ) -> Option<gpui::Point<gpui::Pixels>> {
        let (state, style) = cx.global::<Views>().0.get(id)?;
        let bounds = state.read(cx).bounds();
        // A paragraph wraps at the prose measure inside a wider view.
        let wrap = bounds.size.width.min(px(theme::PROSE_MEASURE));
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = style.line_height_in_pixels(window.rem_size());
        let run = gpui::TextRun {
            len: text.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let layout = window
            .text_system()
            .shape_text(text.into(), font_size, &[run], Some(wrap), None)
            .ok()?
            .into_iter()
            .next()?;
        let position = layout.position_for_index(byte, line_height)?;
        Some(gpui::point(
            bounds.left() + position.x + px(0.5),
            bounds.top() + position.y + line_height * 0.5,
        ))
    }
}

#[cfg(test)]
mod file_link_tests {
    use super::*;
    use gpui::{div, Context, Modifiers, Render, TestAppContext, VisualTestContext};

    struct LinkFixture {
        cache: TextCache,
        document_cache: TextCache,
        source: String,
        cwd: std::path::PathBuf,
        preview: crate::attachment_preview::Preview,
        font_size: f32,
        line_height: Option<f32>,
    }
    impl Render for LinkFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.cache.file_context(Some(&self.cwd), &self.preview);
            let document_body = self.preview.document().map(|document| {
                self.document_cache
                    .file_context(document.path.parent(), &self.preview);
                document_body(document, self.document_cache.clone())
            });
            use gpui::base::ElementExt;
            // The reader is a board slot of its own; beside the fixture's
            // Pane is where the cockpit first opens it.
            let reader = document_body
                .and_then(|body| self.preview.reader(body, |head| head.into_any_element()))
                .map(|reader| div().flex_1().min_w_0().child(reader));
            div()
                .flex()
                .size_full()
                .child(
                    div().flex_1().min_w_0().child(
                        self.preview.mount(
                            div().size_full().child(
                                div()
                                    .text_size(px(self.font_size))
                                    .when_some(self.line_height, |this, line| {
                                        this.line_height(px(line))
                                    })
                                    .child(Markdown::new(
                                        "file-link-fixture",
                                        self.source.clone(),
                                        self.cache.clone(),
                                    ))
                                    .text_selection_scope(
                                        gpui::base::TextSelectionScopeId::default(),
                                    ),
                            ),
                        ),
                    ),
                )
                .children(reader)
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
                document_cache: TextCache::default(),
                source: source.into(),
                cwd: std::env::temp_dir(),
                preview,
                font_size: theme::FS_PROSE,
                line_height: None,
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
    fn list_text_columns_align_across_digits_continuations_and_reading_sizes(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = fixture(cx, "");
        cx.simulate_resize(gpui::size(px(320.), px(720.)));
        for font_size in [14., 18.] {
            for start in [9_u32, 99] {
                let indent = " ".repeat(start.to_string().len() + 2);
                let source = format!(
                    "{start}. [first](first.md) with a description that wraps onto another line.\n\n{indent}[continued](continued.md)\n\n{indent}- [nested](nested.md)\n\n{indent}[resumed](resumed.md)\n{}. [second](second.md)",
                    start + 1,
                );
                view.update(cx, |view, cx| {
                    view.font_size = font_size;
                    view.source = source;
                    cx.notify();
                });
                cx.run_until_parked();
                let first = card(cx, "first.md");
                let continued = card(cx, "continued.md");
                let second = card(cx, "second.md");
                let resumed = card(cx, "resumed.md");
                let nested = card(cx, "nested.md");
                assert!(
                    (first.left() - second.left()).abs() < px(0.5),
                    "{start} and {} share one text column at {font_size}px: {first:?}, {second:?}",
                    start + 1,
                );
                assert!(
                    (first.left() - continued.left()).abs() < px(0.5),
                    "continuation starts at the text column: {first:?}, {continued:?}"
                );
                assert!(
                    (first.left() - resumed.left()).abs() < px(0.5),
                    "continuation after a nested list keeps its original text column"
                );
                assert!(nested.left() >= first.left() + px(font_size));
                assert!(nested.right() <= px(320.));
                let selected =
                    cx.update(|_, cx| testing::full_text("file-link-fixture", cx).unwrap());
                assert_eq!(
                    selected.trim_end(),
                    "first with a description that wraps onto another line.\ncontinued\nnested\nresumed\nsecond"
                );
                cx.simulate_resize(gpui::size(px(300.), px(720.)));
                cx.run_until_parked();
                assert_eq!(
                    cx.update(|_, cx| testing::selected_text("file-link-fixture", cx)),
                    Some(selected),
                    "resizing the measured list preserves exact selected text"
                );
                cx.simulate_resize(gpui::size(px(320.), px(720.)));
            }
            // One hang for every list and quote: bullet text, ordinal text
            // (one digit or two) and quoted text all start on the same x,
            // `PROSE_HANG` in (scaled with the reading size).
            view.update(cx, |view, cx| {
                view.source = "- [bullet](ul.md)\n\ntext\n\n9. [nine](nine.md)\n10. [ten](ten.md)\n\ntext\n\n> [quoted](quote.md)".into();
                cx.notify();
            });
            cx.run_until_parked();
            let bullet = card(cx, "ul.md");
            let hang = px(theme::reading_step(theme::PROSE_HANG, font_size));
            assert!(
                (bullet.left() - hang).abs() < px(0.5),
                "list text hangs PROSE_HANG in at {font_size}px: {bullet:?}"
            );
            let quote = card(cx, "quote.md");
            assert!(
                (quote.left() - bullet.left()).abs() < px(0.5),
                "quoted text shares the bullet's text column at {font_size}px: {quote:?}, {bullet:?}"
            );
            // An ordered list takes the same hang unless its widest marker
            // (in the face the window shapes it in) and its gap outgrow it:
            // with Geist's figures that is only from 100 up.
            let ten = cx.update(|window, _| {
                let run = gpui::TextRun {
                    len: 3,
                    font: window.text_style().font(),
                    color: gpui::black(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                window
                    .text_system()
                    .layout_line("10.", px(font_size), &[run], None)
                    .width
            });
            let column = hang.max(ten + px(theme::LIST_MARKER_GAP));
            let (nine, ten) = (card(cx, "nine.md"), card(cx, "ten.md"));
            assert!(
                (nine.left() - ten.left()).abs() < px(0.5),
                "9. and 10. share one column: {nine:?}, {ten:?}"
            );
            assert!(
                (nine.left() - column).abs() < px(0.5),
                "the ordered column is the hang or the widest marker: {nine:?} vs {column:?}"
            );
        }
    }

    /// A fence is its code and its padding: the actions overlay takes no
    /// layout, so a one-line block is 10 + 18 + 10, and revealing the
    /// actions from the keyboard moves nothing.
    #[gpui::test]
    fn a_fence_is_its_code_and_padding_and_its_actions_take_no_room(cx: &mut TestAppContext) {
        let (_, cx) = fixture(cx, "```rust\nfn main() {}\n```");
        cx.simulate_resize(gpui::size(px(420.), px(300.)));
        cx.run_until_parked();
        let block = cx.update(|_, cx| testing::bounds("file-link-fixture", 0, cx).unwrap());
        assert_eq!(
            block.size.height,
            px(2. * theme::CODE_PAD_Y + theme::LH_CODE),
            "one line of code in its padding"
        );
        let actions = cx
            .debug_bounds("code-actions")
            .expect("the overlay is laid out at rest");
        assert!(actions.top() >= block.top() && actions.right() <= block.right());
        cx.update(|window, cx| window.focus_next(cx));
        cx.run_until_parked();
        assert_eq!(
            cx.update(|_, cx| testing::bounds("file-link-fixture", 0, cx).unwrap()),
            block,
            "revealing the actions leaves the block where it was"
        );
        assert_eq!(cx.debug_bounds("code-actions"), Some(actions));
    }

    #[gpui::test]
    fn fenced_code_copy_preserves_source_and_confirms_without_copying_chrome(
        cx: &mut TestAppContext,
    ) {
        let code = "    first  line\n\tλ🙂 with spaces  \n\nlast";
        let (view, cx) = fixture(cx, &format!("Before\n\n```rust\n{code}\n```\n\nAfter"));
        cx.simulate_resize(gpui::size(px(420.), px(520.)));
        cx.run_until_parked();
        let copy = cx
            .debug_bounds("copy-code")
            .expect("fenced blocks expose Copy");
        assert!(copy.size.height >= px(theme::CODE_ACTION_H));
        assert!(copy.size.width >= px(theme::CODE_ACTION_MIN_W));
        // The padded edge belongs to the action, not text selection behind it.
        cx.simulate_click(
            gpui::point(copy.left() + px(2.), copy.center().y),
            Modifiers::default(),
        );
        cx.run_until_parked();
        assert_eq!(cx.debug_bounds("code-copied"), Some(copy));
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().unwrap().text())
                .as_deref(),
            Some(code)
        );
        assert!(
            cx.debug_bounds("code-copied").is_some(),
            "copy has local confirmation"
        );
        cx.update(|window, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("stale".into()));
            window.focus_next(cx);
        });
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().unwrap().text())
                .as_deref(),
            Some(code)
        );
        let before = cx.update(|_, cx| testing::first_entity("file-link-fixture", cx).unwrap());
        view.update(cx, |view, cx| {
            view.source.push_str(" more prose");
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(testing::first_entity("file-link-fixture", cx), Some(before));
            let selected = testing::full_text("file-link-fixture", cx).unwrap();
            assert!(selected.contains(code));
            assert!(!selected.contains("Copied"), "header is not source content");
        });
        assert!(
            cx.debug_bounds("code-copied").is_some(),
            "appending prose retains block feedback"
        );
    }

    #[gpui::test]
    fn code_copy_uses_the_new_source_after_a_stream_update(cx: &mut TestAppContext) {
        let (view, cx) = fixture(cx, "```\n    before\n```");
        cx.run_until_parked();
        let copy = cx.debug_bounds("copy-code").unwrap().center();
        cx.simulate_click(copy, Modifiers::default());
        view.update(cx, |view, cx| {
            view.source = "```\n    after\n\tnew line\n```".into();
            cx.notify();
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("code-copied").is_none());
        let copy = cx.debug_bounds("copy-code").unwrap().center();
        cx.simulate_click(copy, Modifiers::default());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().unwrap().text())
                .as_deref(),
            Some("    after\n\tnew line")
        );
    }

    #[gpui::test]
    fn markdown_file_click_opens_the_native_reader(cx: &mut TestAppContext) {
        let path = std::env::temp_dir().join("ferrite-report.md");
        std::fs::write(&path, "fixture").unwrap();
        let source = format!("[report]({}:12)", path.display());
        let (view, cx) = fixture(cx, &source);
        let target = card(cx, "ferrite-report.md").center();
        cx.simulate_click(target, Modifiers::default());
        assert_eq!(cx.opened_url(), None);
        let document = view
            .read_with(cx, |view, _| view.preview.document())
            .expect("the Markdown document is retained by its Pane");
        assert_eq!(document.path, path);
        assert_eq!(document.source, "fixture");
        let reader = cx
            .debug_bounds("markdown-reader")
            .expect("the reader is rendered beside the transcript");
        // Beside the transcript, not over it: it starts right of the Pane
        // and takes only its share of the window.
        let window_w = cx.update(|window, _| window.viewport_size().width);
        assert!(reader.left() > px(0.) && reader.size.width < window_w);
        let close = cx
            .debug_bounds("close-markdown-reader")
            .expect("the reader has an explicit close control");
        cx.simulate_click(close.center(), Modifiers::default());
        assert!(view.read_with(cx, |view, _| view.preview.document().is_none()));
    }

    #[gpui::test]
    fn code_file_click_opens_the_native_reader(cx: &mut TestAppContext) {
        let path = std::env::temp_dir().join("ferrite-reader.rs");
        std::fs::write(&path, "fn ferrite() {}\n").unwrap();
        let source = format!("[source]({})", path.display());
        let (view, cx) = fixture(cx, &source);

        let target = card(cx, "ferrite-reader.rs").center();
        cx.simulate_click(target, Modifiers::default());

        assert_eq!(cx.opened_url(), None);
        let document = view
            .read_with(cx, |view, _| view.preview.document())
            .expect("the code file is retained by the built-in reader");
        assert_eq!(document.path, path);
        assert_eq!(document.source, "fn ferrite() {}\n");
        assert!(!document.is_markdown());
        assert!(cx.debug_bounds("markdown-reader").is_some());
    }

    #[gpui::test]
    fn large_code_file_uses_the_virtualized_reader(cx: &mut TestAppContext) {
        let path = std::env::temp_dir().join("ferrite-large-reader.rs");
        let source = (0..5_000)
            .map(|line| format!("fn line_{line}() {{}}\n"))
            .collect::<String>();
        std::fs::write(&path, source).unwrap();
        let link = format!("[large source]({})", path.display());
        let (_, cx) = fixture(cx, &link);
        let target = card(cx, "ferrite-large-reader.rs").center();

        cx.simulate_click(target, Modifiers::default());

        let id = format!("file-{}", path.display());
        assert!(
            cx.update(|_, cx| testing::code(&id, cx).is_some()),
            "large source files must use the bounded, virtualized code reader"
        );
    }

    /// Only a language the lexer knows gets the code reader; anything else
    /// stays the plain text control, with no syntax claims about it.
    #[gpui::test]
    fn unknown_text_files_use_the_plain_reader(cx: &mut TestAppContext) {
        let path = std::env::temp_dir().join("ferrite-reader-notes.txt");
        std::fs::write(&path, "fn not code\n").unwrap();
        let link = format!("[notes]({})", path.display());
        let (_, cx) = fixture(cx, &link);
        let target = card(cx, "ferrite-reader-notes.txt").center();

        cx.simulate_click(target, Modifiers::default());

        let id = format!("file-{}", path.display());
        assert!(cx.update(|_, cx| testing::output(&id, cx).is_some()));
        assert!(cx.update(|_, cx| testing::code(&id, cx).is_none()));
    }

    #[test]
    fn lexed_styles_cover_exactly_the_asked_range() {
        let ink = |color: u32| gpui::HighlightStyle {
            color: Some(rgb(color).into()),
            ..Default::default()
        };
        let runs = vec![(0..3, ink(1)), (3..7, ink(2)), (7..10, ink(3))];
        let covered = |range: std::ops::Range<usize>| {
            let styles = lexed_styles(&runs, &range);
            let mut at = range.start;
            for (run, _) in &styles {
                assert_eq!(run.start, at, "{styles:?}");
                assert!(run.end > run.start, "{styles:?}");
                at = run.end;
            }
            assert_eq!(at, range.end, "{styles:?}");
            styles
        };
        assert_eq!(covered(0..10).len(), 3);
        assert_eq!(
            covered(2..8),
            [(2..3, ink(1)), (3..7, ink(2)), (7..8, ink(3))]
        );
        assert_eq!(covered(4..5), [(4..5, ink(2))]);
        // Past the lexed source — a file mid-reload — is unstyled, not lost.
        assert_eq!(
            covered(8..14),
            [(8..10, ink(3)), (10..14, gpui::HighlightStyle::default())]
        );
        assert_eq!(covered(12..14), [(12..14, gpui::HighlightStyle::default())]);
    }

    #[gpui::test]
    fn binary_file_card_falls_back_to_the_os(cx: &mut TestAppContext) {
        let path = std::env::temp_dir().join("ferrite-reader.bin");
        std::fs::write(&path, [0xff, 0xfe, 0xfd]).unwrap();
        let source = format!("[binary]({})", path.display());
        let (view, cx) = fixture(cx, &source);
        let target = card(cx, "ferrite-reader.bin").center();

        cx.simulate_click(target, Modifiers::default());

        assert_eq!(
            cx.opened_url(),
            Some(url::Url::from_file_path(path).unwrap().to_string())
        );
        assert!(view.read_with(cx, |view, _| view.preview.document().is_none()));
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
        assert_eq!(
            wide.size.height,
            px(theme::INLINE_FILE_H),
            "the chip fits the prose line"
        );
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

    /// A paragraph holding a chip is laid out in the prose's own style, not
    /// the root's: one line box of the answer's line height, and the text
    /// before the chip as wide as the recorded style shapes it.
    #[gpui::test]
    fn a_paragraph_with_a_file_chip_keeps_the_prose_line(cx: &mut TestAppContext) {
        let (view, cx) = fixture(cx, "Before [report](report.md) after.");
        view.update(cx, |view, cx| {
            view.line_height = Some(theme::LH_PROSE);
            cx.notify();
        });
        cx.simulate_resize(gpui::size(px(400.), px(200.)));
        cx.run_until_parked();
        let paragraph = cx.update(|_, cx| testing::bounds("file-link-fixture", 0, cx).unwrap());
        assert_eq!(paragraph.size.height, px(theme::LH_PROSE));
        let chip = card(cx, "report.md");
        let before = cx.update(|window, cx| {
            testing::caret("file-link-fixture", 0, 1, "Before ", 7, window, cx).unwrap()
        });
        assert!(
            (chip.left() - (before.x - px(0.5))).abs() < px(0.5),
            "the chip follows text shaped at the prose size: {chip:?} vs {before:?}"
        );
    }

    /// A file chip is measured in the face and size it is drawn in, so a
    /// name whose measured width and chrome fit `INLINE_FILE_MAX_W` is drawn
    /// whole: `transcript.rs:405` at the app's width is never ellipsized.
    #[gpui::test]
    fn a_file_chip_that_fits_is_drawn_whole(cx: &mut TestAppContext) {
        let (_, cx) = fixture(cx, "Built in [transcript.rs](transcript.rs:405), then.");
        cx.simulate_resize(gpui::size(px(720.), px(200.)));
        cx.run_until_parked();
        let chip = card(cx, "transcript.rs");
        let want = cx.update(|window, _| {
            crate::attachments::inline_file_width("transcript.rs", ":405", false, window)
        });
        assert!(want < px(theme::INLINE_FILE_MAX_W), "{want:?}");
        assert_eq!(chip.size.width, want, "the chip is its measured width");
        // The name box holds its whole shaped width: nothing to ellipsize.
        let name = cx.update(|window, _| {
            let mut face = window.text_style();
            face.font_family = theme::FONT_CODE.into();
            window
                .text_system()
                .shape_line(
                    "transcript.rs".into(),
                    px(theme::FS_UI),
                    &[face.to_run("transcript.rs".len())],
                    None,
                )
                .width()
        });
        assert!(
            chip.size.width >= name + px(2. * theme::INLINE_FILE_PAD_X),
            "{chip:?} holds {name:?}"
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
        let (view, cx) = fixture(
            cx,
            "[the **report** file](ferrite-keyboard.txt) and [second](ferrite-keyboard-second.txt)",
        );
        cx.update(|window, cx| window.focus_next(cx));
        cx.simulate_keystrokes("enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        assert_eq!(cx.opened_url(), None);
        assert_eq!(
            view.read_with(cx, |view, _| view.preview.document().map(|file| file.path)),
            Some(path)
        );
        let close = cx.debug_bounds("close-markdown-reader").unwrap();
        cx.simulate_click(close.center(), Modifiers::default());
        // Closing the reader leaves the keyboard on the card that opened
        // it, so the next Tab stop is the second card.
        cx.update(|window, cx| window.focus_next(cx));
        cx.simulate_keystrokes("enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        assert_eq!(cx.opened_url(), None);
        assert_eq!(
            view.read_with(cx, |view, _| view.preview.document().map(|file| file.path)),
            Some(second)
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
    const SELECTORS: [&str; 9] = [
        "spacing-0",
        "spacing-1",
        "spacing-2",
        "spacing-3",
        "spacing-4",
        "spacing-5",
        "spacing-6",
        "spacing-7",
        "spacing-8",
    ];

    struct SpacingRoot {
        samples: Vec<String>,
        gapped: bool,
    }

    impl Render for SpacingRoot {
        fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            window.set_rem_size(px(theme::FS_UI));
            div()
                .w(px(360.))
                .children(self.samples.iter().enumerate().map(|(ix, source)| {
                    div()
                        .debug_selector(move || format!("spacing-{ix}"))
                        .w_full()
                        .child(
                            TextView::markdown(format!("spacing-{ix}"), source.clone())
                                .max_lines(usize::MAX)
                                .style(if self.gapped {
                                    style(window.rem_size())
                                } else {
                                    style(window.rem_size()).with_paragraph_gap(rems(0.))
                                }),
                        )
                }))
        }
    }

    // Measure the real renderer, including nested blocks. Each pair must add
    // exactly one boundary gap, which depends only on whether a heading is
    // involved, never on nesting.
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
            gapped: true,
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
                // A heading takes more space above than below; every other
                // boundary is one prose gap.
                let heading = "## Heading";
                let want = match (first == heading, second == heading) {
                    (true, true) => theme::HEADING_SPACE_BELOW + theme::HEADING_SPACE_ABOVE,
                    (true, false) => theme::HEADING_SPACE_BELOW,
                    (false, true) => theme::PROSE_GAP + theme::HEADING_SPACE_ABOVE,
                    (false, false) => theme::PROSE_GAP,
                };
                assert!(
                    (gap - px(want)).abs() < px(0.5),
                    "{combined:?}: gap {gap:?}, want {want}"
                );
            }
        }
    }

    #[gpui::test]
    fn narrow_table_uses_the_native_overflow_track(cx: &mut TestAppContext) {
        struct TableRoot;
        impl Render for TableRoot {
            fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().w(px(220.)).debug_selector(|| "table-frame".into()).child(
                    TextView::markdown("overflow-table", "| One | Two | Three | Four | Five | Six |\n| --- | --- | --- | --- | --- | --- |\n| aaaaaaaaaaaaaaaaaaaa | bbbbbbbbbbbbbbbbbbbb | cccccccccccccccccccc | dddddddddddddddddddd | eeeeeeeeeeeeeeeeeeee | ffffffffffffffffffff |")
                    .max_lines(usize::MAX).style(style(window.rem_size())))
            }
        }
        cx.update(gpui::component::init);
        let (_, cx) = cx.add_window_view(|_, _| TableRoot);
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let frame = cx.debug_bounds("table-frame").unwrap();
        let track = cx
            .debug_bounds("markdown-table-track")
            .expect("wide table must provide a scrollable track");
        assert!(track.size.width > frame.size.width);
        // Wheel routing belongs to the toolkit's horizontal_scroll_area;
        // its own tests exercise ScrollHandle offsets. Debug bounds are
        // layout coordinates and do not report that paint translation.
    }

    #[gpui::test]
    fn native_markdown_preserves_literal_bytes_and_link_destinations(cx: &mut TestAppContext) {
        use gpui::base::text::SelectionFormat;
        use std::sync::{Arc, Mutex};
        struct TextRoot {
            state: Entity<TextViewState>,
            clicked: Arc<Mutex<Vec<String>>>,
            format: SelectionFormat,
        }
        impl Render for TextRoot {
            fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let clicked = self.clicked.clone();
                div().w(px(220.)).child(
                    TextView::new(&self.state)
                        .selection_format(self.format)
                        .max_lines(usize::MAX)
                        .style(style(window.rem_size()))
                        .on_link_click(move |url, _, _, _| {
                            clicked.lock().unwrap().push(url.to_string())
                        }),
                )
            }
        }
        cx.update(gpui::component::init);
        let clicked = Arc::new(Mutex::new(Vec::new()));
        let (root, cx) = cx.add_window_view(|_, cx| TextRoot {
            state: cx.new(|cx| {
                TextViewState::markdown("[label](https://example.com/path?q=a%20b#part)", cx)
                    .selectable(true)
            }),
            clicked: clicked.clone(),
            format: SelectionFormat::Plain,
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        cx.simulate_click(gpui::point(px(10.), px(10.)), gpui::Modifiers::none());
        assert_eq!(
            *clicked.lock().unwrap(),
            vec!["https://example.com/path?q=a%20b#part"]
        );
        let state = root.read_with(cx, |root, _| root.state.clone());
        for (source, plain) in [
            (
                "**bold**, *italic*; `one  two` [link](https://example.com).".to_string(),
                "bold, italic; one  two link.\n".to_string(),
            ),
            (
                "```text\n    one  two\n\n\tCJK 漢字 é   \n```".to_string(),
                "    one  two\n\n\tCJK 漢字 é   \n".to_string(),
            ),
            ("W".repeat(124), format!("{}\n", "W".repeat(124))),
        ] {
            root.update(cx, |root, cx| {
                root.format = SelectionFormat::Plain;
                cx.notify();
            });
            state.update(cx, |state, cx| {
                state.set_text(&source, cx);
                state.set_selection_format(SelectionFormat::Plain, cx);
                state.select_all(cx);
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            state.read_with(cx, |state, _| assert_eq!(state.selected_text(), plain));
            if source.len() == 124 {
                state.read_with(cx, |state, _| {
                    assert!(
                        state.bounds().size.height > px(30.),
                        "long token must wrap into visible lines"
                    )
                });
            }
            root.update(cx, |root, cx| {
                root.format = SelectionFormat::Source;
                cx.notify();
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            state.read_with(cx, |state, _| assert_eq!(state.selected_text(), source));
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
            "- One\n\n- Two\n\n- Three".into(),
            "- One\n\n  - Two\n  - Three".into(),
        ];
        let (root, cx) = cx.add_window_view(|_, _| SpacingRoot {
            samples,
            gapped: false,
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let compact: Vec<_> = (0..9)
            .map(|ix| cx.debug_bounds(SELECTORS[ix]).unwrap().size.height)
            .collect();
        root.update(cx, |root, cx| {
            root.gapped = true;
            cx.notify();
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        // Tight unordered, ordered, task and nested lists gain no inter-item
        // gap. Actual paragraphs and loose lists retain two prose gaps.
        let g = theme::PROSE_GAP;
        let extra_space = [0., 0., 0., 0., 2. * g, 2. * g, 2. * g, 2. * g, g];
        for (ix, before) in compact.into_iter().enumerate() {
            let after = cx.debug_bounds(SELECTORS[ix]).unwrap().size.height;
            assert!(
                (after - before - px(extra_space[ix])).abs() < px(0.5),
                "nested sample {ix}: {before:?} -> {after:?}"
            );
        }
    }

    #[gpui::test]
    fn ordered_list_start_controls_the_native_marker_width(cx: &mut TestAppContext) {
        struct OrderedLists {
            width: f32,
        }
        impl Render for OrderedLists {
            fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().w(px(self.width)).children([
                    div().debug_selector(|| "ordered-nine".into()).child(
                        TextView::markdown("ordered-nine", "9. alpha beta gamma delta")
                            .max_lines(usize::MAX)
                            .style(style(window.rem_size())),
                    ),
                    div().debug_selector(|| "ordered-hundred".into()).child(
                        TextView::markdown("ordered-hundred", "100. alpha beta gamma delta")
                            .max_lines(usize::MAX)
                            .style(style(window.rem_size())),
                    ),
                ])
            }
        }
        cx.update(gpui::component::init);
        let (view, cx) = cx.add_window_view(|_, _| OrderedLists { width: 80. });
        let mut longer_prefix_wraps_earlier = false;
        for width in [60., 70., 80., 90., 100., 110., 120.] {
            view.update(cx, |view, cx| {
                view.width = width;
                cx.notify();
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            let nine = cx.debug_bounds("ordered-nine").unwrap().size.height;
            let hundred = cx.debug_bounds("ordered-hundred").unwrap().size.height;
            assert!(
                hundred >= nine,
                "a longer number cannot create more room for item text"
            );
            longer_prefix_wraps_earlier |= hundred > nine;
        }
        assert!(
            longer_prefix_wraps_earlier,
            "native layout must retain source numbering: 100. needs more marker width than 9."
        );
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
        let (_, cx) = cx.add_window_view(|_, _| SpacingRoot {
            samples,
            gapped: true,
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let mut height = |ix| cx.debug_bounds(SELECTORS[ix]).unwrap().size.height;
        let paragraph_pair = height(2);
        assert_eq!(
            height(0),
            paragraph_pair - px(theme::PROSE_GAP),
            "two-space hard break must start a line"
        );
        assert_eq!(
            height(1),
            paragraph_pair - px(theme::PROSE_GAP),
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

/// The Ferrite knobs in `vendor/gpui-base/src/text` (see `vendor/README.md`),
/// measured through the real renderer. The vendor crate is not a workspace
/// member, so its own tests do not run here; these pin the behaviour.
#[cfg(test)]
mod vendor_knob_tests {
    use super::*;
    use gpui::{div, Context, Render, TestAppContext};

    type Knobs = fn(TextViewStyle) -> TextViewStyle;

    struct KnobRoot {
        source: String,
        knobs: Knobs,
        width: f32,
    }

    impl Render for KnobRoot {
        fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            window.set_rem_size(px(theme::FS_UI));
            div().w(px(self.width)).child(
                div()
                    .debug_selector(|| "knob-sample".into())
                    .w_full()
                    .child(
                        TextView::markdown("knob-sample", self.source.clone())
                            .max_lines(usize::MAX)
                            // Upstream defaults, so each knob is measured alone.
                            .style((self.knobs)(
                                TextViewStyle::default().with_paragraph_gap(rems(0.)),
                            )),
                    ),
            )
        }
    }

    fn height(cx: &mut TestAppContext, source: &str, width: f32, knobs: Knobs) -> gpui::Pixels {
        let (_, cx) = cx.add_window_view(|_, _| KnobRoot {
            source: source.into(),
            knobs,
            width,
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        cx.debug_bounds("knob-sample").unwrap().size.height
    }

    fn plain(style: TextViewStyle) -> TextViewStyle {
        style
    }

    #[gpui::test]
    fn heading_space_rides_above_a_heading_only_after_a_sibling(cx: &mut TestAppContext) {
        cx.update(gpui::component::init);
        fn spaced(style: TextViewStyle) -> TextViewStyle {
            style.with_heading_spacing(rems(20. / theme::FS_UI), None)
        }
        let after = height(cx, "Para\n\n## Heading", 360., spaced)
            - height(cx, "Para\n\n## Heading", 360., plain);
        assert!((after - px(20.)).abs() < px(0.5), "space above: {after:?}");
        let first = height(cx, "## Heading", 360., spaced) - height(cx, "## Heading", 360., plain);
        assert!(
            first.abs() < px(0.5),
            "a first heading takes none: {first:?}"
        );
    }

    /// The test text system gives every family one advance, so the family
    /// itself is checked in the visual-reference captures. Here: the code
    /// knobs render and paint their wash without moving any layout.
    #[gpui::test]
    fn inline_code_knobs_paint_without_moving_layout(cx: &mut TestAppContext) {
        cx.update(gpui::component::init);
        fn code(style: TextViewStyle) -> TextViewStyle {
            style
                .with_inline_code_font(Some(theme::FONT_CODE.into()))
                .with_inline_code_wash(Some(gpui::base::text::InlineCodeWash {
                    color: rgba(theme::INLINE_CODE_WASH).into(),
                    radius: px(theme::R_CHIP),
                    overhang: px(2.),
                    inset_y: px(1.),
                }))
        }
        for source in [
            "Run `cargo test` now.".to_string(),
            format!("Wrapped `{}` code.", "long ".repeat(30)),
            "A [link](https://example.com) and `code` in a flow.".to_string(),
        ] {
            let before = height(cx, &source, 220., plain);
            let after = height(cx, &source, 220., code);
            assert_eq!(before, after, "{source:?}");
        }
    }

    #[gpui::test]
    fn the_rule_takes_its_refinement(cx: &mut TestAppContext) {
        cx.update(gpui::component::init);
        fn hairline(style: TextViewStyle) -> TextViewStyle {
            style.with_rule(gpui::StyleRefinement::default().h(px(1.)))
        }
        let thinner = height(cx, "---", 360., plain) - height(cx, "---", 360., hairline);
        assert!((thinner - px(1.)).abs() < px(0.25), "{thinner:?}");
    }
}

/// The Markdown look as data: the knobs `style()` turns on (the renderer
/// itself is measured by `spacing_tests` and `vendor_knob_tests`).
#[cfg(test)]
mod style_tests {
    use super::*;
    use ferrite_core::transcript::Class;
    use gpui::{FontStyle, Hsla};

    fn solid(value: u32) -> Hsla {
        rgb(value).into()
    }

    #[test]
    fn prose_is_text_with_semibold_strong_and_accent_links() {
        let style = style(px(theme::FS_UI));
        assert_eq!(style.foreground(), solid(theme::TEXT));
        assert_eq!(style.strong().font_weight, Some(theme::W_STRONG));
        assert_eq!(style.link(), solid(theme::ACCENT));
        assert_eq!(
            style.link_underline(),
            Some(rgba(theme::ACCENT_EDGE).into())
        );
        assert_eq!(style.inline_code_font().as_deref(), Some(theme::FONT_CODE));
        let wash = style
            .inline_code_wash()
            .expect("inline code sits on a chip");
        assert_eq!(wash.color, rgba(theme::INLINE_CODE_WASH).into());
        assert_eq!(wash.radius, px(theme::R_CHIP));
        assert_eq!(style.inline_code().color, Some(solid(theme::TEXT)));
        assert_eq!(style.code_background(), solid(theme::RAISED));
    }

    #[test]
    fn headings_follow_the_type_table_at_every_reading_size() {
        for (base, h1_line) in [(14., 28.), (18., 36.)] {
            let style = style_at(px(theme::FS_UI), px(base));
            assert_eq!(
                style.heading_font_size(1),
                Some(px((base * 18. / 14.).round()))
            );
            assert_eq!(style.heading_font_size(3), Some(px(base)));
            let h1 = style.heading(1);
            assert_eq!(h1.text.font_weight, Some(theme::W_STRONG));
            assert_eq!(h1.text.color, Some(solid(theme::TEXT_STRONG)));
            assert_eq!(h1.text.line_height, Some(px(h1_line).into()));
            assert_eq!(heading_line_height(1, base), h1_line);
            let h4 = style.heading(4);
            assert_eq!(h4.text.font_weight, Some(theme::W_LABEL));
            assert_eq!(h4.text.color, Some(solid(theme::TEXT_STRONG)));
            assert_eq!(h4.text.font_style, None, "headings are never italic");
        }
        // Every heading is a whole pixel at every reading size, on the type
        // table: 18 · 16 · 14, 21 · 18 · 16, 23 · 21 · 18.
        for (base, sizes) in [
            (14., [18., 16., 14.]),
            (16., [21., 18., 16.]),
            (18., [23., 21., 18.]),
        ] {
            let style = style_at(px(theme::FS_UI), px(base));
            for level in 1..=6 {
                let size = f32::from(style.heading_font_size(level).unwrap());
                assert_eq!(size.fract(), 0., "H{level} at {base}: {size}");
                assert_eq!(
                    size,
                    sizes[(level as usize - 1).min(2)],
                    "H{level} at {base}"
                );
                assert_eq!(
                    style.heading(level).text.line_height,
                    Some(px(theme::prose_line_height(size)).into()),
                    "H{level} at {base} sits on its rounded size's line"
                );
            }
        }
        // Block gaps are em-proportional: the Standard rhythm at every
        // reading size, never smaller than 0.75em of the prose.
        let rem = px(theme::FS_UI);
        for (base, gap, above, below) in
            [(14., 12., 8., 8.), (16., 14., 9., 9.), (18., 15., 10., 10.)]
        {
            let style = style_at(rem, px(base));
            let near = |value: gpui::Rems, want: f32| {
                (f32::from(value.to_pixels(rem)) - want).abs() < 0.01
            };
            assert!(near(style.paragraph_gap(), gap), "{base}");
            assert!(gap >= 0.75 * base, "{base}");
            assert!(near(style.heading_space_above(), above), "{base}");
            assert!(near(style.heading_space_below().unwrap(), below), "{base}");
        }
    }

    #[test]
    fn inline_code_is_a_neutral_body_ink_chip_at_body_weight() {
        use ferrite_core::settings::ReadingSize;
        assert_eq!(theme::INLINE_CODE_INK, theme::TEXT);
        assert_eq!(theme::inline_code_size(ReadingSize::STANDARD), theme::FS_UI);
        // The chip centres in the prose line box at every reading size.
        for (size, chip, inset) in [
            (ReadingSize::STANDARD, 18., 2.),
            (ReadingSize::nearest(16), 20., 2.),
            (ReadingSize::nearest(18), 22., 3.),
        ] {
            assert_eq!(theme::inline_code_chip_h(size), chip);
            assert_eq!(theme::inline_code_inset_y(size), inset);
            let style = style_at(px(theme::FS_UI), px(theme::answer_text_size(size)));
            assert_eq!(style.inline_code_wash().unwrap().inset_y, px(inset));
        }
        // Code inside `# heading` or `**strong**` shapes at 400: the code
        // highlight is merged over the heading's or the strong run's weight.
        let style = style(px(theme::FS_UI));
        assert_eq!(style.inline_code().font_weight, Some(theme::W_BODY));
        for weight in [theme::W_STRONG, style.strong().font_weight.unwrap()] {
            let merged = gpui::HighlightStyle {
                font_weight: Some(weight),
                ..Default::default()
            }
            .highlight(style.inline_code());
            assert_eq!(merged.font_weight, Some(theme::W_BODY));
        }
        let heading = gpui::TextStyle {
            font_weight: theme::W_STRONG,
            ..Default::default()
        }
        .highlight(style.inline_code());
        assert_eq!(heading.font_weight, theme::W_BODY);
    }

    #[test]
    fn markdown_never_tints_inline_code_with_the_accent() {
        let source = include_str!("rich.rs");
        assert!(!source.contains(concat!("ACCENT", "_WASH")));
    }

    #[test]
    fn prose_holds_the_measure_and_code_keeps_the_column() {
        let style = style(px(theme::FS_UI));
        assert_eq!(style.prose_max_width(), Some(px(theme::PROSE_MEASURE)));
        assert_eq!(style.code_block().max_size.width, None);
        assert_eq!(style.table().max_size.width, None);
    }

    #[test]
    fn tables_are_dense_rows_under_one_header_rule() {
        let style = style(px(theme::FS_UI));
        assert!(style.border().is_transparent(), "no row rules");
        let cell = style.table_cell();
        assert_eq!(cell.text.font_size, Some(px(theme::FS_UI).into()));
        assert_eq!(cell.text.line_height, Some(px(theme::LH_UI).into()));
        assert_eq!(
            2. * theme::TABLE_CELL_PAD_Y + theme::LH_UI,
            theme::MENU_ROW_H,
            "a table row is the list pitch"
        );
        assert_eq!(
            cell.text
                .font_features
                .as_ref()
                .map(|features| features.tag_value_list().to_vec()),
            Some(vec![("tnum".to_string(), 1)])
        );
        let head = style.table_head();
        assert_eq!(head.text.font_weight, Some(theme::W_BODY));
        assert_eq!(head.text.color, Some(solid(theme::TEXT_MUTED)));
        assert_eq!(head.border_color, Some(rgba(theme::TABLE_HEAD_RULE).into()));
    }

    #[test]
    fn quotes_and_breaks_are_hairlines() {
        let style = style(px(theme::FS_UI));
        assert_eq!(
            style.blockquote().border_color,
            Some(rgba(theme::HAIRLINE_STRONG).into())
        );
        assert_eq!(style.rule().background, Some(rgba(theme::HAIRLINE).into()));
        assert_eq!(
            style.blockquote().padding.left,
            Some(px(theme::PROSE_HANG - theme::QUOTE_RULE_W).into()),
            "quoted text starts where list text does"
        );
    }

    #[test]
    fn syntax_classes_paint_their_inks_and_comments_lean() {
        for (class, ink) in [
            (Class::Plain, theme::SYN_PLAIN),
            (Class::Keyword, theme::SYN_KEYWORD),
            (Class::Str, theme::SYN_STRING),
            (Class::Number, theme::SYN_NUMBER),
            (Class::Comment, theme::SYN_COMMENT),
            (Class::Function, theme::SYN_FUNCTION),
            (Class::Type, theme::SYN_TYPE),
            (Class::Punct, theme::SYN_PUNCT),
        ] {
            let style = syntax_style(class);
            assert_eq!(style.color, Some(solid(ink)), "{class:?}");
            assert_eq!(
                style.font_style,
                (class == Class::Comment).then_some(FontStyle::Italic),
                "{class:?}"
            );
            assert_eq!(style.background_color, None);
        }
    }
}
