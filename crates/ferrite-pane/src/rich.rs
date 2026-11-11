//! Native rich text with a stable parser per answer run. Appending tokens
//! advances the toolkit parser; unrelated pane renders do not parse again.
use gpui::base::text::{TextView, TextViewState, TextViewStyle};
use gpui::component::input::{Textarea, TextareaState};
use gpui::{prelude::*, px, rems, rgb, rgba, App, Entity, Focusable, SharedString, Window};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

use ferrite_ui::theme;

pub fn init(cx: &mut App) {
    cx.bind_keys([gpui::KeyBinding::new(
        "enter",
        gpui::NoAction {},
        Some("TranscriptCodeActions"),
    )]);
}

/// Code controls form a native keyboard island inside the transcript.
pub fn code_actions_focused(window: &Window) -> bool {
    window
        .context_stack()
        .iter()
        .any(|context| context.contains("TranscriptCodeActions"))
}

/// Replace a native text output's source without losing the reader's
/// place: the selection, its direction, and the scroll offset.
fn keep_place(
    state: &mut TextareaState,
    source: &str,
    window: &mut Window,
    cx: &mut gpui::Context<TextareaState>,
) {
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
}

#[derive(Clone)]
enum NativeText {
    Rich(Entity<TextViewState>),
    Output(Entity<TextareaState>),
}

/// Which native control a cached source is shown in.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Rich,
    Output,
}

impl NativeText {
    fn is(&self, kind: Kind) -> bool {
        matches!(
            (self, kind),
            (NativeText::Rich(_), Kind::Rich) | (NativeText::Output(_), Kind::Output)
        )
    }
}

struct CachedText {
    source: String,
    state: NativeText,
    touched: u64,
}

/// The kinds of native text a transcript keys under its namespace
/// (`select.rs`): `{kind}{namespace}-…`.
const TEXT_KINDS: [&str; 4] = ["markdown-", "literal-", "thinking-", "output-"];

/// What a Pane's cache may hold of native text, by `held`, and in entries.
const TEXT_BUDGET: usize = 32 * 1024 * 1024;
const TEXT_ENTRIES: usize = 256;

/// The most recent lookups belong to texts being drawn together; none of
/// them is evicted for another, or a frame showing more than the budget
/// would rebuild them all every frame. A frame looks up fewer than this.
const DRAWN_TOGETHER: u64 = 64;

/// What a cached native text holds, its parsed document and laid-out lines
/// with them: measured in Sep 2026 by the memory freed as entries dropped,
/// about 59 KiB for a 317-byte paragraph, 317 KiB for 2 KB and 3.4 MiB for
/// 25 KB of drawn Markdown. Its source bytes alone understate it a
/// hundredfold.
fn held(source: &str) -> usize {
    32 * 1024 + source.len() * 144
}

/// Pane-owned native text entities survive temporarily hidden Subjects. The
/// least recently used run is discarded only at this explicit cache limit:
/// `TEXT_ENTRIES`, or `TEXT_BUDGET` of what the texts hold.
#[derive(Clone, Default)]
pub struct TextCache(
    Rc<RefCell<(u64, HashMap<SharedString, CachedText>, usize)>>,
    Rc<RefCell<(Option<std::path::PathBuf>, Option<crate::preview::Preview>)>>,
    /// The transcript (its namespace) that hosts this cache's path targets:
    /// their ⌘-click (`file_links::PathTarget::scope`).
    Rc<RefCell<Option<SharedString>>>,
);

impl TextCache {
    #[cfg(any(test, feature = "test-support"))]
    pub fn retained_handles(&self) -> usize {
        Rc::strong_count(&self.0)
    }

    /// Every native text this cache holds, by id.
    #[cfg(any(test, feature = "test-support"))]
    pub fn cached_ids(&self) -> Vec<SharedString> {
        self.0.borrow().1.keys().cloned().collect()
    }

    pub fn file_context(&self, cwd: Option<&std::path::Path>, preview: &crate::preview::Preview) {
        *self.1.borrow_mut() = (cwd.map(std::path::Path::to_path_buf), Some(preview.clone()));
    }

    /// Host this cache's path targets in the transcript `scope` (its
    /// namespace): the one drawing now, so its card and ⌘-click route there.
    pub(crate) fn set_path_scope(&self, scope: SharedString) {
        let mut current = self.2.borrow_mut();
        if current.as_ref() != Some(&scope) {
            *current = Some(scope);
        }
    }

    fn path_scope(&self) -> Option<SharedString> {
        self.2.borrow().clone()
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

    /// Release every native text built under `namespace`: a transcript's
    /// key that can never be drawn again (its history regenerated under a
    /// new one, or the transcript released).
    pub fn forget_namespace(&self, namespace: &str) {
        let prefixes = TEXT_KINDS.map(|kind| format!("{kind}{namespace}-"));
        let mut cache = self.0.borrow_mut();
        let cache = &mut *cache;
        let mut freed = 0;
        cache.1.retain(|id, text| {
            let stale = prefixes
                .iter()
                .any(|prefix| id.starts_with(prefix.as_str()));
            if stale {
                freed += held(&text.source);
            }
            !stale
        });
        cache.2 -= freed;
    }

    pub fn redirect_namespace(&self, from: &str, to: &str) {
        let mut cache = self.0.borrow_mut();
        let keys: Vec<_> = cache
            .1
            .keys()
            .filter_map(|key| {
                TEXT_KINDS.into_iter().find_map(|kind| {
                    key.strip_prefix(&format!("{kind}{from}"))
                        .map(|tail| (key.clone(), SharedString::from(format!("{kind}{to}{tail}"))))
                })
            })
            .collect();
        for (from, to) in keys {
            if let Some(state) = cache.1.remove(&from) {
                if let Some(replaced) = cache.1.insert(to, state) {
                    cache.2 -= held(&replaced.source);
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
                cache.2 -= held(&old.source);
            }
        }
        cache.0 += 1;
        let touched = cache.0;
        let prior = cache.1.get(&id).map_or(0, |text| held(&text.source));
        let holds = held(source);
        while (!cache.1.contains_key(&id) && cache.1.len() >= TEXT_ENTRIES)
            || cache.2.saturating_sub(prior) + holds > TEXT_BUDGET
        {
            let Some(oldest) = cache
                .1
                .iter()
                .filter(|(key, text)| *key != &id && text.touched + DRAWN_TOGETHER < touched)
                .min_by_key(|(_, text)| text.touched)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            if let Some(old) = cache.1.remove(&oldest) {
                cache.2 -= held(&old.source);
            }
        }
        cache.2 = cache.2.saturating_sub(prior) + holds;
        let text = cache.1.entry(id).or_insert_with(|| CachedText {
            source: source.to_string(),
            state: match kind {
                Kind::Output => NativeText::Output(cx.new(|cx| {
                    let mut state = TextareaState::new(window, cx).auto_grow(1, 12);
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
                    state.update(cx, |state, cx| keep_place(state, source, window, cx))
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
    /// Shared, so a preview's document renders without being copied.
    source: SharedString,
    cache: TextCache,
    muted: bool,
    /// Body ink in place of the default content ink.
    ink: Option<u32>,
    /// The work's tier (`quiet`): muted, its code plain.
    quiet: bool,
    /// One line, never wrapped (`activity`).
    single_line: bool,
    /// `ferrite-chip:` links draw as a prompt's inline file chips, opening
    /// in this Pane's preview among the prompt's images (`pane::prompt_chip`).
    chips: Option<(
        crate::preview::Preview,
        std::sync::Arc<[std::path::PathBuf]>,
    )>,
    /// The skills `ferrite-skill:` links name (`skills`).
    skills: std::sync::Arc<[ferrite_core::SessionCommand]>,
    document: Option<gpui::base::TextSelectionDocument>,
    /// A find's occurrences to mark (`crate::find`).
    find: Option<gpui::base::text::TextFind>,
    /// The answer is streaming: each run of words that arrives fades in
    /// (`motion::STREAM_FADE`, `TextView::fresh`).
    streaming: bool,
    /// The stream ended at this instant: what still fades lands within
    /// `motion::STREAM_LAND`.
    landing: Option<std::time::Instant>,
}

impl Markdown {
    pub fn selection_document(
        mut self,
        document: Option<gpui::base::TextSelectionDocument>,
    ) -> Self {
        self.document = document;
        self
    }

    pub fn find(mut self, find: Option<gpui::base::text::TextFind>) -> Self {
        self.find = find;
        self
    }

    /// Fade each run of words that streams in (`streaming`), from the
    /// transcript's ground; off, any fade still running lands at once.
    pub fn streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    /// The stream ended at `ended`: the runs still fading speed up to land
    /// within `motion::STREAM_LAND` (Claude's fast finish at turn end),
    /// rather than at once.
    pub fn landing(mut self, ended: Option<std::time::Instant>) -> Self {
        self.landing = ended;
        self
    }

    pub fn muted(mut self) -> Self {
        self.muted = true;
        self
    }

    /// Set the body in `ink`, its bold and native links too — a link lifts
    /// by its underline (headings keep their ink).
    pub fn ink(mut self, ink: u32) -> Self {
        self.ink = Some(ink);
        self
    }

    /// Set the run in the work's tier (narration, activity): `INK_SUBTLE`
    /// unless inked otherwise, its code (a command, a bundle id) a step
    /// brighter on no chip.
    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    /// Set the run as an activity line: quiet, one line at its own width,
    /// never wrapped (its box cuts it, `pane::EdgeCut`), its paths plain
    /// words underlined only under the pointer.
    pub fn activity(mut self) -> Self {
        self.single_line = true;
        self.quiet()
    }

    /// Draw `ferrite-chip:` links as a prompt's inline chips (a prompt's
    /// band: its words, then its files), opening in `preview` — an image
    /// among the prompt's `images`; its code in the prompt's own chip.
    pub fn chips(
        mut self,
        preview: crate::preview::Preview,
        images: std::sync::Arc<[std::path::PathBuf]>,
    ) -> Self {
        self.chips = Some((preview, images));
        self
    }

    /// The skills a prompt's `ferrite-skill:` links name, with the
    /// descriptions their tooltips read.
    pub fn skills(mut self, skills: std::sync::Arc<[ferrite_core::SessionCommand]>) -> Self {
        self.skills = skills;
        self
    }

    pub fn new(
        id: impl Into<SharedString>,
        source: impl Into<SharedString>,
        cache: TextCache,
    ) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            cache,
            muted: false,
            ink: None,
            quiet: false,
            single_line: false,
            chips: None,
            skills: Default::default(),
            document: None,
            find: None,
            streaming: false,
            landing: None,
        }
    }
}

impl gpui::RenderOnce for Markdown {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // A parsed document keeps parts of the style it was first laid out
        // in (a list's items): prose that turns to narration once more work
        // follows it is laid out afresh in the quiet style.
        let key = if self.quiet {
            SharedString::from(format!("{}~quiet", self.id))
        } else {
            self.id.clone()
        };
        let state = self.cache.state(key, &self.source, window, cx);
        // Prose sets its own face (the transcript's, Geist); the inherited
        // style is the Pane's.
        let face: SharedString = theme::FONT_UI.into();
        #[cfg(any(test, feature = "test-support"))]
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
        // A view of prose runs a cell past its column for a line's trailing
        // space (below); its blocks take that cell back. A single line
        // never wraps, so runs on its column.
        let text_style = if self.single_line {
            style_at(window.rem_size(), base)
        } else {
            hung_style(window.rem_size(), base)
        };
        let text_style = if self.muted {
            // A thought stays one step down the ink ladder, headings too.
            (1..=6).fold(
                text_style.with_foreground(rgb(theme::INK_SUBTLE).into()),
                |style, level| {
                    style.with_heading(
                        level,
                        heading(level, base).text_color(rgb(theme::INK_SUBTLE)),
                    )
                },
            )
        } else {
            text_style
        };
        let ink = self.ink.or(self.quiet.then_some(theme::INK_SUBTLE));
        let text_style = match ink {
            Some(ink) => {
                let strong = gpui::HighlightStyle {
                    color: Some(rgb(ink).into()),
                    ..text_style.strong()
                };
                text_style
                    .with_foreground(rgb(ink).into())
                    .with_link(rgb(ink).into())
                    .with_strong(strong)
            }
            None => text_style,
        };
        // A prompt's band runs its words the band's whole width (the
        // prototype's `.prompt .txt` has no measure).
        let text_style = if self.quiet {
            text_style
                .with_inline_code(gpui::HighlightStyle {
                    color: Some(rgb(theme::INK_SUBTLE).into()),
                    font_weight: Some(theme::W_BODY),
                    ..Default::default()
                })
                .with_inline_code_wash(None)
        } else {
            text_style
        };
        // A sent prompt's code is Claude's: `PROMPT_CODE_INK` on a
        // `PROMPT_CODE_CHIP`, a step over the bubble.
        let text_style = if self.chips.is_some() {
            text_style
                .with_inline_code(gpui::HighlightStyle {
                    color: Some(rgb(theme::PROMPT_CODE_INK).into()),
                    font_weight: Some(theme::W_BODY),
                    ..Default::default()
                })
                .with_inline_code_wash(Some(prompt_code_wash()))
        } else {
            text_style
        };
        let text_style = if self.single_line || self.chips.is_some() {
            text_style.with_prose_max_width(None)
        } else {
            text_style
        };
        // An activity line's path is the row's own words, as Claude's are:
        // no link ink, no underline until the pointer is on it.
        let text_style = if self.single_line {
            let words = text_style.foreground();
            text_style
                .with_link(words)
                .with_link_underline(None)
                .with_link_underline_on_hover(true)
        } else {
            text_style
        };
        let actions_namespace = self.id.clone();
        let (cwd, preview) = self.cache.1.borrow().clone();
        let scope = self.cache.path_scope();
        let chips = self.chips.clone();
        let skills = self.skills.clone();
        let link_cwd = cwd.clone();
        let link_preview = preview.clone();
        let link_scope = scope.clone();
        let hang = theme::tx_cell(f32::from(base));
        // A ```mermaid fence draws as its diagram (`visual::diagram`); the
        // fence's Markdown (the source, moved in) says whether it has closed.
        let diagrams = self
            .source
            .contains(crate::visual::diagram::LANGUAGE)
            .then_some(self.source);
        let view = TextView::new(&state)
            .link_renderer(move |url, label, window, cx| {
                // A prompt's file, as its band's chip.
                if let Some(name) = url.strip_prefix(SKILL_SCHEME) {
                    let skill = skills.iter().find(|skill| skill.name == name)?;
                    return Some(crate::pane::skill_token(label, skill, window));
                }
                if let Some(path) = url.strip_prefix(CHIP_SCHEME) {
                    let (preview, images) = chips.as_ref()?;
                    return Some(crate::pane::prompt_chip(
                        std::path::PathBuf::from(path),
                        label,
                        preview,
                        images,
                        window,
                        cx,
                    ));
                }
                if let Some(id) = url.strip_prefix(THREAD_SCHEME) {
                    let thread = ferrite_core::ThreadId::new(id.parse().ok()?);
                    return Some(crate::link_chip::thread_chip(
                        thread,
                        label,
                        scope.clone(),
                        window,
                    ));
                }
                // An issue ref and an activity line's path stay native links
                // (their underline under the pointer); a click opens them.
                if url.starts_with(ISSUE_SCHEME) || url.starts_with(PATH_SCHEME) {
                    return None;
                }
                let file = crate::file_links::FileLink::resolve(url, cwd.as_deref())?;
                Some(crate::attachments::inline_file(
                    file,
                    label,
                    preview.as_ref(),
                    scope.clone(),
                    window,
                    cx,
                ))
            })
            .when_some(self.document, |view, document| {
                view.selection_document(document, self.id)
            })
            .find(self.find)
            .fresh((self.streaming || self.landing.is_some()).then(|| {
                gpui::base::text::TextFresh {
                    fade: ferrite_ui::motion::STREAM_FADE.duration(),
                    ease: Rc::new(|t| ferrite_ui::motion::STREAM_FADE.progress(t)),
                    ground: theme::paint::CARD.hsla(),
                    land: self
                        .landing
                        .map(|ended| (ended, ferrite_ui::motion::STREAM_LAND)),
                }
            }))
            .font_family(face)
            // A line's trailing space hangs past its edge, as CSS lets it:
            // GPUI counts it when wrapping, so a word that ends within a
            // cell of the edge would wrap a line early. The view runs one
            // cell past its column; its words still end inside it. A single
            // line never wraps, and its box must hold all of its words:
            // `pane::EdgeCut` clips to it.
            .when(!self.single_line, |view| view.mr(px(-hang)))
            .min_w_0()
            // Use natural height inside the transcript's own scroll container.
            .max_lines(if self.single_line { 1 } else { usize::MAX })
            // A list's items take the inherited ink, not the style's.
            .when_some(ink, |view, ink| view.text_color(rgb(ink)))
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
                if let Some(number) = url.strip_prefix(ISSUE_SCHEME) {
                    if let Some(page) = link_cwd
                        .as_deref()
                        .and_then(|cwd| crate::file_links::issue_url(cwd, number))
                    {
                        if !link_preview.as_ref().is_some_and(|preview| preview.open_url(&page, window)) {
                            cx.open_url(&page);
                        }
                    }
                    return;
                }
                let from_call = url.starts_with(PATH_SCHEME);
                let url = url.strip_prefix(PATH_SCHEME).unwrap_or(url);
                if let Some(file) = crate::file_links::FileLink::resolve(url, link_cwd.as_deref()) {
                    // A call's path opens where a prose path's ⌘-click does:
                    // the Thread's preview, in Files.
                    if let (true, Some(scope)) = (from_call, link_scope.clone()) {
                        let target = crate::file_links::PathTarget::new(file.path.clone())
                            .at_line(file.location.as_deref().and_then(crate::file_links::location_line))
                            .in_scope(scope);
                        crate::file_links::open_target(&target, window, cx);
                    } else {
                        file.open(window, cx);
                    }
                } else {
                    if !link_preview.as_ref().is_some_and(|preview| preview.open_url(url, window)) {
                        cx.open_url(url);
                    }
                }
            })
            .when_some(diagrams, |view, source| {
                view.code_block_view(move |block, element, window, cx| {
                    let fence = block.span.and_then(|span| source.get(span.start..span.end));
                    match fence {
                        Some(fence)
                            if block.lang().as_deref()
                                == Some(crate::visual::diagram::LANGUAGE) =>
                        {
                            crate::visual::diagram::or_code(
                                fence,
                                block.code().to_string(),
                                element,
                                f32::from(base),
                                window,
                                cx,
                            )
                        }
                        _ => element,
                    }
                })
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
            });
        // A block of the column's width; the view inside it stretches a
        // cell past it (its negative margin), so a trailing space hangs.
        gpui::div()
            .w_full()
            .min_w_0()
            .child(view)
            .into_any_element()
    }
}

/// Each fenced block holds its own copy confirmation (`COPIED_HOLD_MS`), also
/// while the answer streams.
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
            ferrite_ui::motion::hover_t(&self.key)
        };
        // The transcript around this overlay is a cached view: while the
        // blend is mid-flight, ask for the next frame's render here (the
        // cached ancestors follow).
        if shown > 0. && shown < 1. {
            cx.notify();
        }
        // The block's header (Claude's): the language at the left at rest,
        // the actions at the right blending in under the pointer. The row
        // stands in the block's top padding, the code's own inset in from
        // both sides.
        let code_size = window.text_style().font_size.to_pixels(window.rem_size());
        let inset = px(theme::CODE_PAD_CELLS * theme::tx_cell(f32::from(code_size)));
        let mut actions = gpui::div()
            .key_context("TranscriptCodeActions")
            .track_focus(&self.focus)
            .debug_selector(|| "code-actions".into())
            .absolute()
            .top(px(theme::BLOCK_PAD_Y))
            .left(inset)
            .right(inset)
            .h(px(theme::LH_UI))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(theme::CH))
            .font_family(theme::FONT_UI)
            .text_size(px(theme::FS_UI))
            .line_height(px(theme::LH_UI))
            .text_color(rgb(theme::INK_SUBTLE))
            .child(
                gpui::div()
                    .debug_selector(|| "code-language".into())
                    .min_w_0()
                    .truncate()
                    .children(self.language.clone()),
            );
        let mut hovered = gpui::div()
            .flex()
            .items_center()
            .gap(px(theme::SPACE_1))
            .opacity(shown);
        if self
            .language
            .as_ref()
            .is_some_and(|lang| lang.eq_ignore_ascii_case("html"))
        {
            hovered = hovered.child(code_action("preview-html", cx).label("Preview").on_click(
                move |_, window, cx| {
                    use gpui::component::WindowExt as _;
                    let html = code.clone();
                    window.open_dialog(cx, move |dialog, window, _| {
                        reading_dialog(
                            dialog,
                            "html-preview",
                            "HTML preview",
                            TextView::html("html-preview-text", html.clone())
                                .style(style(window.rem_size())),
                        )
                    });
                },
            ));
        }
        // `Copy ⌘C`: the key read from the key table, as a mono suffix on a
        // wrapper (a kit button's own tooltip is plain text).
        hovered = hovered.child(
            gpui::div()
                .id("copy-code-tip")
                .flex_shrink_0()
                .tooltip(ferrite_ui::menu::action_tooltip(
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
                            // `Copied` holds a moment, then the block's
                            // `Copy` is ready again.
                            let hold = std::time::Duration::from_millis(theme::COPIED_HOLD_MS);
                            cx.spawn(async move |view, cx| {
                                cx.background_executor().timer(hold).await;
                                let _ = view.update(cx, |view, cx| {
                                    view.copied = false;
                                    cx.notify();
                                });
                            })
                            .detach();
                        })),
                ),
        );
        actions = actions.child(hovered);
        gpui::div()
            .id(SharedString::from(format!("{}-overlay", self.key)))
            .debug_selector(|| "code-overlay".into())
            .relative()
            .size_full()
            .on_hover(ferrite_ui::motion::hover_listener(self.key.clone()))
            .child(actions)
    }
}

/// A quiet text action in a fence's overlay: `FS_UI` `INK_SUBTLE` with no
/// ground, the hover overlay under the pointer (the one 150ms blend),
/// pressed at once, a stable `CODE_ACTION_MIN_W` target one row high.
fn code_action(id: &'static str, cx: &App) -> gpui::component::button::Button {
    ferrite_ui::components::faded_button(
        id,
        rgba(theme::TRANSPARENT).into(),
        theme::paint::HOVER.into(),
        theme::paint::PRESS.into(),
        rgb(theme::INK_SUBTLE).into(),
        cx,
    )
    .h(px(theme::LH_UI))
    .min_w(px(theme::CODE_ACTION_MIN_W))
    .px(px(theme::CH))
    .rounded(px(theme::R_CONTROL))
    .font_family(theme::FONT_UI)
    .text_size(px(theme::FS_UI))
    .line_height(px(theme::LH_UI))
    .flex_shrink_0()
    .tab_stop(true)
}

/// The scheme of a prompt's inline file chip link (`pane::prompt_text`).
pub(crate) const CHIP_SCHEME: &str = "ferrite-chip:";
/// A prompt's skill: `[/name](<ferrite-skill:name>)`, drawn in `SKILL_INK`.
pub(crate) const SKILL_SCHEME: &str = "ferrite-skill:";
/// A Thread linked in prose: `[title](ferrite-thread:12)`, drawn as
/// `link_chip::thread_chip`.
pub(crate) const THREAD_SCHEME: &str = "ferrite-thread:";
/// The scheme `file_links::link_targets` gives an issue ref (`#212`).
pub(crate) const ISSUE_SCHEME: &str = "issue:";
/// The scheme of a path in an activity line (a call's argument, a
/// directory): a native link in the line's flow, never a chip,
/// that opens the file as a prose path does.
pub(crate) const PATH_SCHEME: &str = "ferrite-path:";

/// Markdown's look at the Standard reading size (see the WP-B section of
/// `theme.rs` for the rules). Rems convert with the active root font size.
pub fn style(rem_size: gpui::Pixels) -> TextViewStyle {
    style_at(rem_size, px(theme::FS_PROSE))
}

/// Markdown's look for prose set at `base`: headings are ratios of it
/// (`theme::heading_scale`), each on its own pixel line height.
pub fn style_at(rem_size: gpui::Pixels, base: gpui::Pixels) -> TextViewStyle {
    style_hanging(rem_size, base, 0.)
}

/// `style_at` for a view that runs one cell past its column so a line's
/// trailing space can hang there (`Markdown::render`): its blocks — a
/// fence, a table, a rule — keep a cell's margin on the right, so they end
/// on the column's edge with the prompt bubble and the Composer.
fn hung_style(rem_size: gpui::Pixels, base: gpui::Pixels) -> TextViewStyle {
    style_hanging(rem_size, base, theme::tx_cell(f32::from(base)))
}

/// Markdown's look at `base`, its blocks `hang` short of the view's right
/// edge.
fn style_hanging(rem_size: gpui::Pixels, base: gpui::Pixels, hang: f32) -> TextViewStyle {
    let rem = |value: f32| rems(value / f32::from(rem_size));
    let size = f32::from(base);
    let line = theme::prose_line_height(size);
    let cell = theme::tx_cell(size);
    // A fence is set a half step under the prose, as a diff is.
    let code_size = size - theme::TX_BLOCK_STEP;
    let code_line = theme::prose_line_height(code_size);
    let code_cell = theme::tx_cell(code_size);
    let mut style = TextViewStyle::default()
        .with_dark(true)
        .with_foreground(theme::prose_ink())
        .with_muted_foreground(rgb(theme::INK_SUBTLE).into())
        // Links and paths read `LINK_INK`, underlined only under the
        // pointer, as T3's are.
        .with_link(rgb(theme::LINK_INK).into())
        .with_link_underline(Some(rgb(theme::LINK_INK).into()))
        .with_link_underline_on_hover(true)
        // An image in an answer sits in the prompt image's frame:
        // `IMAGE_CELLS` wide at the prose's size, a 1px `LINE` edge.
        .with_image(
            gpui::StyleRefinement::default()
                .w(px(theme::IMAGE_CELLS * cell))
                .border_1()
                .border_color(theme::paint::LINE),
        )
        .with_selection(rgba(theme::TEXT_SELECTION_WASH).into())
        .with_strong(gpui::HighlightStyle {
            color: Some(rgb(theme::INK_STRONG).into()),
            font_weight: Some(theme::W_STRONG),
            ..Default::default()
        })
        // A fence is the block box: `BLOCK`,
        // `R_CONTROL` corners, a 1px `BLOCK_RING`, `BLOCK_PAD_Y` above and
        // below (over the code, the header's `CODE_HEADER_H`), the code
        // `CODE_PAD_CELLS` in, a half step under the prose.
        .with_code_background(gpui::transparent_black())
        .with_code_block(
            gpui::StyleRefinement::default()
                .px(px(theme::CODE_PAD_CELLS * code_cell))
                .pt(px(theme::BLOCK_PAD_Y + theme::CODE_HEADER_H))
                .pb(px(theme::BLOCK_PAD_Y))
                // A fence holds to the measure too (`max-width: 108ch`).
                .max_w(px(theme::MEASURE_CH * cell))
                // Auto, not the vendor's full width, so the margin narrows
                // it rather than pushing it past the view.
                .w_auto()
                .mr(px(hang))
                // Its ring hangs `CARD_OUTSET` past the column, as every
                // transcript card's does.
                .ml(px(-theme::CARD_OUTSET))
                .bg(theme::paint::BLOCK)
                .border_1()
                .border_color(theme::paint::BLOCK_RING)
                .rounded(px(theme::R_CONTROL))
                .font_family(theme::FONT_CODE)
                .font_weight(theme::W_BODY)
                .text_size(px(code_size))
                .line_height(px(code_line))
                .text_color(rgb(theme::SYN_PLAIN)),
        )
        // Inline code is a chip (SPEC §5.4): `INK_STRONG` at body weight
        // on an `INLINE_CODE_CHIP` wash, `R_CONTROL` corners, `INLINE_CODE_PAD_X` of
        // room either side.
        .with_inline_code(gpui::HighlightStyle {
            color: Some(rgb(theme::INLINE_CODE_INK).into()),
            font_weight: Some(theme::W_BODY),
            ..Default::default()
        })
        .with_inline_code_font(Some(theme::FONT_CODE.into()))
        .with_inline_code_wash(Some(inline_code_wash()))
        // A table's rules run the full column (Claude's), each column its
        // content's width — inline code at its own face — and a share of
        // the spare width in proportion, so a table that fits wraps no
        // cell; every one left-aligned as Claude's are — numbers too,
        // unless the table's own `---:` asks otherwise; its rules, the
        // head's and the rows', are the one `LINE`.
        .with_border(theme::paint::LINE.into())
        .with_table({
            // No box and no ground.
            let mut table = gpui::StyleRefinement::default()
                .border_0()
                // Auto, as the fence's: the vendor's scroll box is full
                // width, so a margin alone would push it past the view.
                .w_auto()
                .mr(px(hang))
                .bg(gpui::transparent_black());
            table.overflow.x = Some(gpui::Overflow::Scroll);
            table
        })
        // A row is `TABLE_ROW_H` (the head `TABLE_HEAD_H`) with a one-line
        // cell's text centred in it; a cell that wraps grows its row. A
        // column's text starts on the column's edge (the first on the
        // prose's), two cells of room after it.
        .with_table_cell(ferrite_ui::components::tabular(
            gpui::StyleRefinement::default()
                .border_r_0()
                .pl(px(0.))
                .pr(px(2.0 * cell))
                .py(px(0.))
                .min_h(px(theme::TABLE_ROW_H))
                .flex()
                .flex_col()
                .justify_center()
                .text_size(px(size))
                .line_height(px(line)),
        ))
        .with_table_head(
            gpui::StyleRefinement::default()
                .min_h(px(theme::TABLE_HEAD_H))
                .bg(gpui::transparent_black())
                .text_color(rgb(theme::INK_STRONG))
                .font_weight(theme::W_STRONG)
                .border_color(theme::paint::LINE),
        )
        .with_blockquote(
            gpui::StyleRefinement::default()
                .border_l(px(theme::QUOTE_RULE_W))
                .border_color(theme::paint::LINE)
                .text_color(rgb(theme::INK_SUBTLE))
                .not_italic()
                .pl(px(2.0 * cell - theme::QUOTE_RULE_W))
                .pr(px(0.)),
        )
        .with_rule(
            gpui::StyleRefinement::default()
                .h(px(1.))
                .bg(theme::paint::LINE)
                .my(px(theme::RULE_MARGIN_Y))
                .mr(px(hang)),
        )
        // Prose holds to the measure (`MEASURE_CH` cells), left-aligned;
        // code, tables and rules keep the column (`hang` short of a hung
        // view's edge). One cell more, for the trailing space a line hangs
        // (see `Markdown::render`).
        .with_prose_max_width(Some(px(((theme::MEASURE_CH + 1.0) * cell).round())))
        // A list's `•` hangs in the first of two cells, dim; its text on
        // the second's far side.
        .with_list_hang(Some(gpui::base::text::ListHang {
            width: px(2.0 * cell),
            gap: px(cell),
        }))
        .with_list_markers(
            gpui::StyleRefinement::default().text_color(rgb(theme::INK_SUBTLE)),
            ferrite_ui::components::tabular(
                gpui::StyleRefinement::default().text_color(rgb(theme::INK_SUBTLE)),
            ),
        )
        // Blocks stand `tx_paragraph_gap` apart (12 on the 21 line), so an
        // answer reads as one piece; a heading takes the same. A tight
        // list's items stand `TX_LIST_ITEM_GAP` further apart than their
        // lines.
        .with_paragraph_gap(rem(theme::tx_paragraph_gap(line)))
        .with_list_item_gap(px(theme::TX_LIST_ITEM_GAP))
        .with_heading_spacing(rem(0.), None)
        .with_heading_base_font_size(base)
        .with_heading_font_size(|level, base| px(heading_size(level, f32::from(base))));
    for level in 1..=6 {
        style = style.with_heading(level, heading(level, base));
    }
    style
}

/// The inline-code chip's wash (SPEC §5.4's `padding: 1px 5px`): its
/// horizontal padding laid out as room either side of the run, so the
/// text after it moves along, and the ground painted over that room.
pub fn inline_code_wash() -> gpui::base::text::InlineCodeWash {
    gpui::base::text::InlineCodeWash {
        color: theme::paint::INLINE_CODE_CHIP.hsla(),
        radius: px(theme::R_CONTROL),
        // The wash covers the laid-out room either side and no more, so
        // it meets neither neighbouring space.
        overhang: px(0.),
        pad: px(theme::INLINE_CODE_PAD_X),
        inset_y: px(theme::INLINE_CODE_INSET_Y),
    }
}

/// `inline_code_wash` in a sent prompt's bubble: `PROMPT_CODE_CHIP`.
fn prompt_code_wash() -> gpui::base::text::InlineCodeWash {
    gpui::base::text::InlineCodeWash {
        color: theme::paint::PROMPT_CODE_CHIP.hsla(),
        ..inline_code_wash()
    }
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
    gpui::StyleRefinement::default()
        .font_weight(theme::W_STRONG)
        .text_color(rgb(theme::INK_STRONG))
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

/// Literal provider output shares Markdown's selection engine and keeps
/// the parent's typography. A collision-free code fence preserves exact text.
#[derive(IntoElement)]
pub struct Literal {
    pub id: SharedString,
    pub text: SharedString,
    pub highlights: Vec<(std::ops::Range<usize>, gpui::HighlightStyle)>,
    pub cache: TextCache,
    pub document: Option<gpui::base::TextSelectionDocument>,
    /// A find's occurrences to mark (`crate::find`).
    pub find: Option<gpui::base::text::TextFind>,
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
        let text = self.text.clone();
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
        #[cfg(any(test, feature = "test-support"))]
        testing::record(self.id.clone(), state.clone(), inherited.clone(), cx);
        TextView::new(&state)
            .when_some(self.document, |view, document| {
                view.selection_document(document, self.id)
            })
            .find(self.find)
            .w_full()
            .min_w_0()
            // Use natural height inside the transcript's own scroll container.
            .max_lines(usize::MAX)
            .style(style)
            // The retained state re-parses off the main thread, so for a frame
            // after the text changes its block still holds the previous text.
            // The highlights index this text; they wait for its parse.
            .code_block_highlighter(move |block| {
                if block.code() == text {
                    highlights.clone()
                } else {
                    Vec::new()
                }
            })
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
    /// A find's occurrences to mark (`crate::find`).
    pub find: Option<gpui::base::text::TextFind>,
}

impl gpui::RenderOnce for Output {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let NativeText::Output(state) =
            self.cache
                .cached(self.id.clone(), &self.text, Kind::Output, window, cx)
        else {
            unreachable!("output has its own namespace")
        };
        state.update(cx, |state, cx| state.set_find(self.find, cx));
        #[cfg(any(test, feature = "test-support"))]
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

/// A dialog to read in (the HTML preview, What's New), on the sheet
/// recipe: raised, the strong edge (the kit's border), the Pane radius, a
/// mono title, over prose at reading width that scrolls past
/// `READING_DIALOG_MAX_H`.
pub fn reading_dialog(
    dialog: gpui::component::dialog::Dialog,
    id: &'static str,
    title: impl Into<SharedString>,
    body: impl IntoElement,
) -> gpui::component::dialog::Dialog {
    dialog
        .title(
            gpui::div()
                .font_family(theme::FONT_UI)
                .text_size(px(theme::FS_UI))
                .line_height(px(theme::LH_UI))
                .font_weight(theme::W_LABEL)
                .text_color(rgb(theme::INK_STRONG))
                .child(title.into()),
        )
        .width(px(theme::READING_MAX_W))
        .bg(theme::paint::FLOAT)
        .rounded(px(theme::R_FLOAT))
        .child(
            gpui::div()
                .id(id)
                .font_family(theme::FONT_UI)
                .text_size(px(theme::FS_PROSE))
                .line_height(px(theme::LH_PROSE))
                .text_color(rgb(theme::INK_STRONG))
                .max_h(px(theme::READING_DIALOG_MAX_H))
                .overflow_y_scroll()
                .child(body),
        )
}

#[cfg(any(test, feature = "test-support"))]
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
        // A paragraph wraps at the prose measure (`MEASURE_CH` cells at its
        // size) inside a wider view.
        let font_size = style.font_size.to_pixels(window.rem_size());
        let measure = (theme::MEASURE_CH * theme::tx_cell(f32::from(font_size))).round();
        let wrap = bounds.size.width.min(px(measure));
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
        source: String,
        cwd: std::path::PathBuf,
        preview: crate::preview::Preview,
        font_size: f32,
        line_height: Option<f32>,
    }
    impl Render for LinkFixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.cache.file_context(Some(&self.cwd), &self.preview);
            use gpui::base::ElementExt;
            div().flex().size_full().child(
                div().flex_1().min_w_0().child(
                    div().size_full().child(
                        div()
                            .text_size(px(self.font_size))
                            .when_some(self.line_height, |this, line| this.line_height(px(line)))
                            .child(Markdown::new(
                                "file-link-fixture",
                                self.source.clone(),
                                self.cache.clone(),
                            ))
                            .text_selection_scope(gpui::base::TextSelectionScopeId::default()),
                    ),
                ),
            )
        }
    }

    fn fixture<'a>(
        cx: &'a mut TestAppContext,
        source: &str,
    ) -> (Entity<LinkFixture>, &'a mut VisualTestContext) {
        cx.update(crate::pane::init_components);
        let (root, cx) = cx.add_window_view(|window, cx| {
            let preview = crate::preview::Preview::new(cx);
            let view = cx.new(|_| LinkFixture {
                cache: TextCache::default(),
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
            // Terminal-native: one hang for every list and quote — bullet
            // text, ordinal text (one digit or two) and quoted text all start
            // two cells in at the reading size.
            view.update(cx, |view, cx| {
                view.source = "- [bullet](ul.md)\n\ntext\n\n9. [nine](nine.md)\n10. [ten](ten.md)\n\ntext\n\n> [quoted](quote.md)".into();
                cx.notify();
            });
            cx.run_until_parked();
            let bullet = card(cx, "ul.md");
            let hang = px(theme::tx_gutter(font_size));
            assert!(
                (bullet.left() - hang).abs() < px(0.5),
                "list text hangs two cells in at {font_size}px: {bullet:?}"
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
            let column = hang.max(ten + px(theme::tx_cell(font_size)));
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

    /// A fence is its header row and its code in the block box (SPEC §5.4):
    /// `CODE_HEADER_H` for the header, a line at the block size, `BLOCK_PAD_Y`
    /// above and below, the 1px hairline round it. The header overlay takes
    /// no layout of its own, and revealing the actions from the keyboard
    /// moves nothing.
    #[gpui::test]
    fn a_fence_is_its_header_code_and_padding_and_its_actions_take_no_room(
        cx: &mut TestAppContext,
    ) {
        let (_, cx) = fixture(cx, "```rust\nfn main() {}\n```");
        cx.simulate_resize(gpui::size(px(420.), px(300.)));
        cx.run_until_parked();
        let block = cx.update(|_, cx| testing::bounds("file-link-fixture", 0, cx).unwrap());
        assert_eq!(
            block.size.height,
            px(
                theme::prose_line_height(theme::FS_PROSE - theme::TX_BLOCK_STEP)
                    + theme::CODE_HEADER_H
                    + 2. * theme::BLOCK_PAD_Y
                    + 2.
            ),
            "a header and one line of code in its box"
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

    const MERMAID: &str = "Before\n\n```mermaid\ngraph TD\n  a --> b\n```\n\nAfter";

    /// A ```mermaid fence shows as code, asks for its diagram once that code
    /// is on screen, and shows the diagram (cropped to what it drew, at its
    /// own height) once drawn.
    #[gpui::test]
    fn a_mermaid_fence_draws_as_its_diagram(cx: &mut TestAppContext) {
        let engine = std::rc::Rc::new(crate::visual::web::fake::FakeEngine::new());
        cx.update(|cx| {
            crate::visual::engine::Visuals::install(engine.clone(), cx);
            crate::visual::diagram::init(true, cx);
        });
        let (_, cx) = fixture(cx, MERMAID);
        cx.simulate_resize(gpui::size(px(1000.), px(800.)));
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("mermaid-diagram").is_none(),
            "code until drawn"
        );
        assert_eq!(
            engine.journal().len(),
            1,
            "one page opened: {:?}",
            engine.journal()
        );
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        let diagram = cx
            .debug_bounds("mermaid-diagram")
            .expect("the diagram replaces the fence");
        let height = crate::visual::web::fake::CONTENT_HEIGHT as f32;
        assert!(
            (diagram.size.height - px(height)).abs() < px(1.),
            "{diagram:?}"
        );
        assert_eq!(
            engine.live_views(),
            0,
            "no page stays open for a drawn diagram"
        );
    }

    /// A fence in a quote closes on a quoted line, and draws all the same.
    #[gpui::test]
    fn a_mermaid_fence_in_a_quote_draws(cx: &mut TestAppContext) {
        let engine = std::rc::Rc::new(crate::visual::web::fake::FakeEngine::new());
        cx.update(|cx| {
            crate::visual::engine::Visuals::install(engine.clone(), cx);
            crate::visual::diagram::init(true, cx);
        });
        let (_, cx) = fixture(cx, "> ```mermaid\n> graph TD\n> ```\n\nAfter");
        cx.simulate_resize(gpui::size(px(1000.), px(800.)));
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        assert!(cx.debug_bounds("mermaid-diagram").is_some());
    }

    /// Without an engine, or with visuals off, the fence stays code.
    #[gpui::test]
    fn a_mermaid_fence_stays_code_without_visuals(cx: &mut TestAppContext) {
        cx.update(|cx| crate::visual::diagram::init(true, cx));
        let (_, cx) = fixture(cx, MERMAID);
        cx.simulate_resize(gpui::size(px(1000.), px(800.)));
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        assert!(cx.debug_bounds("mermaid-diagram").is_none());
        assert!(
            cx.debug_bounds("code-actions").is_some(),
            "the fence is drawn"
        );
    }

    #[gpui::test]
    fn a_mermaid_fence_stays_code_with_visuals_off(cx: &mut TestAppContext) {
        let engine = std::rc::Rc::new(crate::visual::web::fake::FakeEngine::new());
        cx.update(|cx| {
            crate::visual::engine::Visuals::install(engine.clone(), cx);
            crate::visual::diagram::init(false, cx);
        });
        let (_, cx) = fixture(cx, MERMAID);
        cx.simulate_resize(gpui::size(px(1000.), px(800.)));
        cx.run_until_parked();
        assert!(engine.journal().is_empty(), "nothing is drawn");
        assert!(cx.debug_bounds("mermaid-diagram").is_none());
    }

    /// Laid out, a fence and a table wider than the column both end on the
    /// column's edge (the view's own, less the hang cell its prose's
    /// trailing space runs into), never a cell past it.
    #[gpui::test]
    fn wide_blocks_end_on_the_column_edge(cx: &mut TestAppContext) {
        let wide = "X".repeat(120);
        let source = format!(
            "Before\n\n```rust\nlet a = 1;\n```\n\n| a | b |\n|---|---|\n| {wide} | 1 |\n\nAfter"
        );
        let (_view, cx) = fixture(cx, &source);
        cx.simulate_resize(gpui::size(px(420.), px(520.)));
        cx.run_until_parked();
        let fence = cx.debug_bounds("code-overlay").expect("the fence");
        let table = cx
            .debug_bounds("horizontal-scroll-viewport")
            .expect("the table's viewport");
        assert!(
            (fence.right() - px(420.)).abs() <= px(1.),
            "the fence ends on the column: {fence:?}"
        );
        // The fence's overlay sits inside its 1px border; the table's
        // viewport is the box itself.
        assert!(
            (table.right() - px(420.)).abs() <= px(1.),
            "the table ends where the fence does: {table:?} / {fence:?}"
        );
    }

    /// CP7 regression 3: a table that fits its column wraps no cell —
    /// inline code is measured in its own face with its chip's room, and
    /// the spare width is shared out — so each row stays one
    /// `TABLE_ROW_H` line under the head, as Claude's does.
    #[gpui::test]
    fn a_table_that_fits_keeps_its_code_on_one_line(cx: &mut TestAppContext) {
        let source = "| Command | Result |\n| --- | --- |\n| `ls /definitely-missing-dir` | Exit 1: `No such file or directory` |\n| `sleep 15 && echo done` | Exit 0: printed `done` after 15s |";
        let (_view, cx) = fixture(cx, source);
        cx.simulate_resize(gpui::size(px(640.), px(400.)));
        cx.run_until_parked();
        let track = cx
            .debug_bounds("markdown-table-track")
            .expect("the table's track");
        let one_line = theme::TABLE_HEAD_H + 2.0 * theme::TABLE_ROW_H + 2.0;
        assert!(
            f32::from(track.size.height) <= one_line,
            "every cell on one line: {track:?}, at most {one_line}"
        );
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
        assert!(copy.size.height >= px(theme::LH_UI));
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

    /// A fence's header stands in the block's top: the language at the
    /// left, the code's inset in from the block's edge, and the actions the
    /// same inset in from the right, on one `LH_UI` line. A fence with no
    /// language says nothing at the left.
    #[gpui::test]
    fn a_fences_header_holds_its_language_and_its_actions(cx: &mut TestAppContext) {
        let (view, cx) = fixture(
            cx,
            "```rust
let count = 2;
```",
        );
        cx.simulate_resize(gpui::size(px(520.), px(400.)));
        cx.run_until_parked();
        let block = cx
            .debug_bounds("code-overlay")
            .expect("the block's overlay");
        let row = cx.debug_bounds("code-actions").expect("the header row");
        let language = cx.debug_bounds("code-language").expect("the language");
        let code_size = theme::FS_PROSE - theme::TX_BLOCK_STEP;
        let inset = px(theme::CODE_PAD_CELLS * theme::tx_cell(code_size));
        assert_eq!(row.top() - block.top(), px(theme::BLOCK_PAD_Y));
        // Within a pixel's snap: the block's edge stands a hang cell short
        // of the view's (`hung_style`), off the pixel grid.
        assert!(
            (row.left() - block.left() - inset).abs() < px(0.5),
            "the language sits the code's inset in from the left"
        );
        assert!(
            (block.right() - row.right() - inset).abs() < px(0.5),
            "the actions sit the code's inset in from the right"
        );
        assert_eq!(row.size.height, px(theme::LH_UI));
        assert_eq!(language.left(), row.left());
        view.update(cx, |view, cx| {
            view.source = "```\nplain\n```".into();
            cx.notify();
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("code-language")
                .is_none_or(|language| language.size.width == px(0.)),
            "no language, nothing at the left"
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

    /// A file link opens in the Pane's preview (the Panel shows it), at
    /// the link's line; the OS is never asked.
    #[gpui::test]
    fn a_file_link_opens_in_the_preview_at_its_line(cx: &mut TestAppContext) {
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
        assert_eq!(&*document.text().unwrap().source, "fixture");
        assert_eq!(document.line(), Some(12));
        let code = std::env::temp_dir().join("ferrite-reader.rs");
        std::fs::write(&code, "fn ferrite() {}\n").unwrap();
        view.update(cx, |view, cx| {
            view.source = format!("[source]({})", code.display());
            cx.notify();
        });
        cx.run_until_parked();
        let target = card(cx, "ferrite-reader.rs").center();
        cx.simulate_click(target, Modifiers::default());
        let document = view
            .read_with(cx, |view, _| view.preview.document())
            .expect("another file replaces it");
        assert_eq!(document.path, code);
        assert!(!document.is_markdown());
        assert_eq!(document.line(), None);
    }

    /// A binary file opens in the preview too, which says it is no text
    /// and offers the original: the OS is asked only from there.
    #[gpui::test]
    fn a_binary_file_link_opens_a_preview_that_says_so(cx: &mut TestAppContext) {
        let path = std::env::temp_dir().join("ferrite-reader.bin");
        std::fs::write(&path, [0xff, 0xfe, 0xfd]).unwrap();
        let source = format!("[binary]({})", path.display());
        let (view, cx) = fixture(cx, &source);
        let target = card(cx, "ferrite-reader.bin").center();

        cx.simulate_click(target, Modifiers::default());

        assert_eq!(cx.opened_url(), None);
        let document = view.read_with(cx, |view, _| view.preview.document().unwrap());
        assert!(matches!(
            document.body,
            crate::preview::Body::Unshown(crate::preview::Unshown::Binary)
        ));
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
        assert!(
            wide.size.height < px(2. * theme::FS_PROSE),
            "the link sits on one prose line: {wide:?}"
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
        // Narrower than `Before ` and the card, less the cell a trailing
        // space may hang into.
        cx.simulate_resize(gpui::size(px(110.), px(500.)));
        let narrow = card(cx, "report.md");
        assert!(
            narrow.top() > wide.top(),
            "card wraps as a unit: {narrow:?}"
        );
        assert!(
            narrow.right() <= px(110.),
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

    /// A file link is measured in the face and size it is drawn in (the
    /// prose's, at medium weight), so a name that fits `INLINE_FILE_MAX_W`
    /// is drawn whole: `transcript.rs:405` at the app's width is never
    /// ellipsized.
    #[gpui::test]
    fn a_file_chip_that_fits_is_drawn_whole(cx: &mut TestAppContext) {
        let (_, cx) = fixture(cx, "Built in [transcript.rs](transcript.rs:405), then.");
        cx.simulate_resize(gpui::size(px(720.), px(200.)));
        cx.run_until_parked();
        let chip = card(cx, "transcript.rs");
        let want = cx.update(|window, _| {
            crate::link_chip::Chip {
                icon: ferrite_ui::icons::FILE,
                ink: theme::MENTION_INK,
                name: "transcript.rs",
                location: ":405",
            }
            .width(px(theme::FS_PROSE), window)
        });
        assert!(want < px(theme::INLINE_FILE_MAX_W), "{want:?}");
        assert_eq!(chip.size.width, want, "the chip is its measured width");
    }

    /// `[title](ferrite-thread:12)` draws as a chip standing on its line,
    /// and a path link's chip leads with its icon (wider than the name).
    #[gpui::test]
    fn a_thread_link_is_a_chip_on_the_line(cx: &mut TestAppContext) {
        let (_, cx) = fixture(cx, "See [Fix the build](ferrite-thread:12) for that.");
        cx.simulate_resize(gpui::size(px(720.), px(200.)));
        cx.run_until_parked();
        let chip = cx
            .debug_bounds("thread-link-12")
            .expect("the thread link is a chip");
        let want = cx.update(|window, _| {
            crate::link_chip::Chip {
                icon: ferrite_ui::icons::MESSAGES_SQUARE,
                ink: theme::THREAD_INK,
                name: "Fix the build",
                location: "",
            }
            .width(px(theme::FS_PROSE), window)
        });
        assert_eq!(chip.size.width, want);
    }

    /// A file link to an image opens it in the Pane's lightbox, as every
    /// image does (`Preview::open`), the viewer holding the keyboard;
    /// the OS is never asked.
    #[gpui::test]
    fn an_image_link_opens_in_the_panes_lightbox(cx: &mut TestAppContext) {
        let path = std::env::temp_dir().join("ferrite-link-image.svg");
        std::fs::write(
            &path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"/>"#,
        )
        .unwrap();
        let (view, cx) = fixture(cx, "[image](ferrite-link-image.svg)");
        let target = card(cx, "ferrite-link-image.svg").center();
        cx.simulate_click(target, Modifiers::default());
        let lightbox = view
            .read_with(cx, |view, _| view.preview.lightbox())
            .expect("the image opened");
        assert_eq!(lightbox.path(), path);
        assert!(view.read_with(cx, |view, _| view.preview.document().is_none()));
        assert!(cx.update(|window, cx| view.read(cx).preview.held_focus(window, cx).is_some()));
        assert_eq!(cx.opened_url(), None);
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
        view.read_with(cx, |view, _| view.preview.close());
        // Closing the preview leaves the keyboard on the card that opened
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
        cx.update(crate::pane::init_components);
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
            // The prose's own size and line, as every host sets them: the
            // window's default line (1.618em) is fractional at 14px.
            div()
                .w(px(360.))
                .text_size(px(theme::FS_PROSE))
                .line_height(px(theme::LH_PROSE))
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
                // Every boundary, a heading's included, is the one
                // paragraph gap.
                let want = theme::tx_paragraph_gap(theme::LH_PROSE);
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

    /// An image on its own line under a caption gets the answer's image
    /// frame; one with text beside it (a badge) stays text-sized in the line.
    #[gpui::test]
    fn an_image_on_its_own_line_is_framed_and_a_badge_stays_inline(cx: &mut TestAppContext) {
        struct ImageRoot(&'static str);
        impl Render for ImageRoot {
            fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().w(px(800.)).child(
                    TextView::markdown("image-lines", self.0)
                        .max_lines(usize::MAX)
                        .style(style(window.rem_size())),
                )
            }
        }
        let frame_w = px(theme::IMAGE_CELLS * theme::tx_cell(theme::FS_PROSE));
        cx.update(gpui::component::init);
        for (source, framed) in [
            ("**Caption**\n![a](https://example.invalid/a.png)", true),
            ("![a](https://example.invalid/a.png)\nCaption under", true),
            ("Above\n![a](https://example.invalid/a.png)\nBelow", true),
            (
                "A badge ![b](https://example.invalid/b.png) in a line.",
                false,
            ),
        ] {
            let (_, cx) = cx.add_window_view(|_, _| ImageRoot(source));
            cx.run_until_parked();
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            let frame = cx.debug_bounds("markdown-image-frame");
            assert_eq!(frame.is_some(), framed, "{source:?}");
            if let Some(frame) = frame {
                assert!((frame.size.width - frame_w).abs() < px(1.), "{source:?}");
            }
        }
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
        // Tight unordered, ordered, task and nested lists keep their item
        // gap either way. Actual paragraphs and loose lists gain two
        // paragraph gaps.
        let g = theme::tx_paragraph_gap(theme::LH_PROSE);
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
        // A hard break is a new line; a paragraph pair is the same two
        // lines with the paragraph gap between them.
        let paragraph_pair = height(2);
        let gap = px(theme::tx_paragraph_gap(theme::LH_PROSE));
        assert_eq!(
            height(0),
            paragraph_pair - gap,
            "two-space hard break must start a line"
        );
        assert_eq!(
            height(1),
            paragraph_pair - gap,
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
                    color: rgba(0xffffff0f).into(),
                    radius: px(theme::R_CONTROL),
                    overhang: px(2.),
                    pad: px(0.),
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

    /// SPEC §5.4: a padded chip lays its room out either side of the code,
    /// so the text after it moves along and wraps sooner — in a plain
    /// paragraph and in one that flows a link's element.
    #[gpui::test]
    fn an_inline_code_chip_takes_its_padding_as_room(cx: &mut TestAppContext) {
        cx.update(gpui::component::init);
        fn chip(style: TextViewStyle) -> TextViewStyle {
            style
                .with_inline_code_font(Some(theme::FONT_CODE.into()))
                .with_inline_code_wash(Some(inline_code_wash()))
        }
        let chips = "`a` ".repeat(24);
        for source in [
            format!("{chips}end."),
            format!("A [link](https://example.com) then {chips}end."),
        ] {
            let before = height(cx, &source, 220., plain);
            let after = height(cx, &source, 220., chip);
            assert!(after > before, "{source:?}: {before:?} → {after:?}");
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

    /// Prose `INK_STRONG` at 90% (T3's `text-foreground/90`), strong
    /// `W_STRONG` `INK_STRONG`, links and paths `LINK_INK` underlined under
    /// the pointer, inline code
    /// a `RAISED` chip, a fence in the block box.
    #[test]
    fn prose_is_prose_ink_with_semibold_strong_and_link_blue_links() {
        let style = style(px(theme::FS_UI));
        assert_eq!(style.foreground(), solid(theme::INK_STRONG).opacity(0.9));
        assert_eq!(style.strong().font_weight, Some(theme::W_STRONG));
        assert_eq!(style.strong().color, Some(solid(theme::INK_STRONG)));
        assert_eq!(style.link(), solid(theme::LINK_INK));
        // The underline is the link's own ink, painted under the pointer.
        assert_eq!(style.link_underline(), Some(solid(theme::LINK_INK)));
        assert!(style.link_underline_on_hover());
        assert_eq!(style.inline_code_font().as_deref(), Some(theme::FONT_CODE));
        assert_eq!(style.inline_code_wash(), Some(inline_code_wash()));
        assert_eq!(
            style.inline_code().color,
            Some(solid(theme::INLINE_CODE_INK))
        );
        assert!(
            style.code_background().is_transparent(),
            "a fence's ground is its box's, not its runs'"
        );
        assert_eq!(
            style.code_block().border_color,
            Some(theme::paint::BLOCK_RING.into()),
            "a fence's box has its ring"
        );
    }

    /// Theme rule 1: a heading is the body size, set apart by weight and
    /// ink, on the reading size's own line.
    #[test]
    fn headings_follow_the_type_table_at_every_reading_size() {
        for base in [theme::FS_PROSE, 18.] {
            let h1_line = theme::prose_line_height(base);
            let style = style_at(px(theme::FS_UI), px(base));
            assert_eq!(style.heading_font_size(1), Some(px(base)));
            assert_eq!(style.heading_font_size(3), Some(px(base)));
            let h1 = style.heading(1);
            assert_eq!(h1.text.font_weight, Some(theme::W_STRONG));
            assert_eq!(h1.text.color, Some(solid(theme::INK_STRONG)));
            assert_eq!(h1.text.line_height, Some(px(h1_line).into()));
            let h4 = style.heading(4);
            assert_eq!(h4.text.font_weight, Some(theme::W_STRONG));
            assert_eq!(h4.text.color, Some(solid(theme::INK_STRONG)));
            assert_eq!(h4.text.font_style, None, "headings are never italic");
        }
        // Every heading is the body size at every reading size.
        for (base, sizes) in [
            (14., [14., 14., 14.]),
            (16., [16., 16., 16.]),
            (18., [18., 18., 18.]),
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
        // Blocks sit `tx_paragraph_gap` apart at every reading size (12 on
        // the 21 line), a heading too.
        let rem = px(theme::FS_UI);
        for base in [14., 16., 18.] {
            let gap = theme::tx_paragraph_gap(theme::prose_line_height(base));
            let style = style_at(rem, px(base));
            let near = |value: gpui::Rems, want: f32| {
                (f32::from(value.to_pixels(rem)) - want).abs() < 0.01
            };
            assert!(near(style.paragraph_gap(), gap), "{base}");
            assert!(near(style.heading_space_above(), 0.), "{base}");
            assert!(style.heading_space_below().is_none(), "{base}");
        }
    }

    /// SPEC §5.4: inline code is `INK_STRONG` (`INLINE_CODE_INK`) at body
    /// weight on its `INLINE_CODE_CHIP`, `R_CONTROL` round.
    #[test]
    fn inline_code_is_a_strong_chip_at_body_weight() {
        use ferrite_core::settings::ReadingSize;
        assert_eq!(theme::INLINE_CODE_INK, theme::INK_STRONG);
        let wash = inline_code_wash();
        assert_eq!(wash.color, theme::paint::INLINE_CODE_CHIP.hsla());
        assert_eq!(wash.radius, px(theme::R_CONTROL));
        // The same chip at every reading size.
        for size in [
            ReadingSize::STANDARD,
            ReadingSize::nearest(16),
            ReadingSize::nearest(18),
        ] {
            let style = style_at(px(theme::FS_UI), px(theme::answer_text_size(size)));
            assert_eq!(style.inline_code_wash(), Some(wash));
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
        // `MEASURE_CH` cells at the Standard reading size, and the cell a
        // line's trailing space hangs into.
        assert_eq!(
            style.prose_max_width(),
            Some(px(((theme::MEASURE_CH + 1.0)
                * theme::tx_cell(theme::FS_PROSE))
            .round()))
        );
        // A fence holds to the measure; a
        // table keeps the column.
        assert_eq!(
            style.code_block().max_size.width,
            Some(px(theme::MEASURE_CH * theme::tx_cell(theme::FS_PROSE)).into())
        );
        assert_eq!(style.table().max_size.width, None);
    }

    /// The answer view runs a cell past its column for a line's trailing
    /// space; its fence, table and rule take that cell back, so they end on
    /// the column's edge with the prompt bubble and the Composer — at every
    /// reading size. A view without the hang keeps them flush.
    #[test]
    fn blocks_take_back_the_hang_cell() {
        for base in [theme::FS_PROSE, 18.] {
            let cell = Some(px(theme::tx_cell(base)).into());
            let hung = hung_style(px(theme::FS_UI), px(base));
            assert_eq!(hung.code_block().margin.right, cell, "fence at {base}");
            assert_eq!(
                hung.code_block().margin.left,
                Some(px(-theme::CARD_OUTSET).into()),
                "a fence's ring hangs past the column, as every card's"
            );
            assert_eq!(hung.table().margin.right, cell, "table at {base}");
            assert_eq!(hung.rule().margin.right, cell, "rule at {base}");
            let flush = style_at(px(theme::FS_UI), px(base));
            assert_eq!(flush.code_block().margin.right, Some(px(0.).into()));
        }
    }

    /// Claude's table: a `W_STRONG` head `TABLE_HEAD_H` tall over a `LINE`
    /// rule, `TABLE_ROW_H` rows under `LINE` rules, no box.
    #[test]
    fn tables_are_rows_on_the_grid_under_a_strong_header_rule() {
        let style = style(px(theme::FS_UI));
        assert_eq!(style.border(), theme::paint::LINE.into(), "row rules");
        let cell = style.table_cell();
        assert_eq!(cell.min_size.height, Some(px(theme::TABLE_ROW_H).into()));
        assert_eq!(
            style.table_head().min_size.height,
            Some(px(theme::TABLE_HEAD_H).into())
        );
        assert_eq!(cell.text.font_size, Some(px(theme::FS_PROSE).into()));
        assert_eq!(cell.text.line_height, Some(px(theme::LH_PROSE).into()));
        assert_eq!(
            cell.text
                .font_features
                .as_ref()
                .map(|features| features.tag_value_list().to_vec()),
            Some(vec![("tnum".to_string(), 1)])
        );
        let head = style.table_head();
        assert_eq!(head.text.font_weight, Some(theme::W_STRONG));
        assert_eq!(head.text.color, Some(solid(theme::INK_STRONG)));
        assert_eq!(head.border_color, Some(theme::paint::LINE.into()));
    }

    #[test]
    fn quotes_and_breaks_are_hairlines() {
        let style = style(px(theme::FS_UI));
        assert_eq!(
            style.blockquote().border_color,
            Some(theme::paint::LINE.into())
        );
        assert_eq!(
            style.rule().background,
            Some(gpui::Fill::from(theme::paint::LINE))
        );
        assert_eq!(
            style.blockquote().padding.left,
            Some(px(theme::tx_gutter(theme::FS_PROSE) - theme::QUOTE_RULE_W).into()),
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

#[cfg(test)]
mod cache_cost_tests {
    use super::*;

    /// A native text holds far more than its source: a 2 KB answer run
    /// freed about 317 KB when dropped (Sep 2026 probe, drawn once).
    const HELD_BY_A_2KB_RUN: usize = 317 * 1024;

    fn draw(cache: &TextCache, id: String, source: &str, cx: &mut gpui::VisualTestContext) {
        cx.update(|window, cx| {
            cache.state(id.into(), source, window, cx);
        });
    }

    /// A long-lived Pane draws one new answer run after another. Charged
    /// only its source bytes, its cache held 256 runs, ~80 MB for runs of
    /// 2 KB; what it holds is now bounded by what its texts really hold.
    #[gpui::test]
    fn a_panes_text_cache_holds_a_bounded_amount_of_native_text(cx: &mut gpui::TestAppContext) {
        cx.update(crate::pane::init_components);
        let cx = cx.add_empty_window();
        let cache = TextCache::default();
        let source = "words ".repeat(2048 / 6);
        for run in 0..256 {
            draw(&cache, format!("markdown-pane-{run}"), &source, cx);
        }
        let ids = cache.cached_ids();
        assert!(
            ids.len() * HELD_BY_A_2KB_RUN <= TEXT_BUDGET,
            "{} runs of 2 KB held, ~{} MB",
            ids.len(),
            ids.len() * HELD_BY_A_2KB_RUN / (1024 * 1024)
        );
        assert!(ids.iter().any(|id| id.as_ref() == "markdown-pane-255"));
    }

    /// Texts drawn together never evict each other, whatever they hold:
    /// a frame showing more than the budget would otherwise rebuild them
    /// all on every frame.
    #[gpui::test]
    fn texts_drawn_together_stay_cached_past_the_budget(cx: &mut gpui::TestAppContext) {
        cx.update(crate::pane::init_components);
        let cx = cx.add_empty_window();
        let cache = TextCache::default();
        let source = "words ".repeat(50_000 / 6);
        for frame in 0..3 {
            for run in 0..12 {
                draw(&cache, format!("markdown-big-{run}"), &source, cx);
            }
            assert_eq!(cache.cached_ids().len(), 12, "frame {frame}");
        }
    }
}
